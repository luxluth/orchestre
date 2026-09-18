use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use mtk::bytemuck;
use rodio::Source;
use rodio::source::SeekError;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::TrackType;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::orchestra::track::Song;

pub const POINTS_PER_FRAME: usize = 1024;
pub const NUM_TRAIL_FRAMES: usize = 5;
pub const TARGET_FPS: u32 = 60;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[bytemuck(crate = "mtk::bytemuck")]
pub struct SoundBlobUniforms {
    pub time: f32,
    pub aspect: f32,
    pub base_radius: f32,
    pub amp_scale: f32,
    pub point_count: u32,
    pub trail_count: u32,
    pub _pad1: f32,
    pub _pad2: f32,
    pub flair_color: [f32; 4],
    pub secondary_color: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct VisualFrame {
    pub displacements: [f32; POINTS_PER_FRAME],
}

#[derive(Clone, Debug)]
pub struct CachedSoundShape {
    pub sample_rate: u32,
    pub total_frames: usize,
    pub frames: Vec<VisualFrame>,
}

impl CachedSoundShape {
    pub fn from_audio_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let src = File::open(path)?;
        let mss = MediaSourceStream::new(Box::new(src), Default::default());

        let hint = Hint::new();
        let mut format = symphonia::default::get_probe().probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )?;

        let track = format
            .default_track(TrackType::Audio)
            .or_else(|| format.first_track_known_codec(TrackType::Audio))
            .or_else(|| format.first_track(TrackType::Audio))
            .ok_or("No audio track found")?;

        let audio_params = track
            .codec_params
            .as_ref()
            .and_then(|cp| cp.audio())
            .ok_or("No audio codec params found")?;

        let sample_rate = audio_params.sample_rate.unwrap_or(44100);
        let mut decoder = symphonia::default::get_codecs()
            .make_audio_decoder(audio_params, &AudioDecoderOptions::default())?;

        let track_id = track.id;
        let mut raw_mono_samples: Vec<f32> = Vec::new();

        while let Ok(Some(packet)) = format.next_packet() {
            if packet.track_id != track_id {
                continue;
            }

            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let channels = decoded.spec().channels().count().max(1);
                    let mut interleaved = Vec::new();
                    decoded.copy_to_vec_interleaved(&mut interleaved);
                    for chunk in interleaved.chunks(channels) {
                        let sum: f32 = chunk.iter().sum();
                        raw_mono_samples.push(sum / channels as f32);
                    }
                }
                Err(symphonia::core::errors::Error::DecodeError(_)) => (),
                Err(e) => return Err(Box::new(e)),
            }
        }

        if raw_mono_samples.is_empty() {
            return Ok(CachedSoundShape {
                sample_rate,
                total_frames: 0,
                frames: Vec::new(),
            });
        }

        let samples_per_frame = (sample_rate / TARGET_FPS) as usize;
        let total_frames = raw_mono_samples.len() / samples_per_frame.max(1);
        let mut frames = Vec::with_capacity(total_frames);

        for f in 0..total_frames {
            let start_idx = f * samples_per_frame;
            let mut frame = VisualFrame {
                displacements: [0.0; POINTS_PER_FRAME],
            };

            let available = raw_mono_samples.len() - start_idx;
            let window_len = POINTS_PER_FRAME.min(available);

            for i in 0..window_len {
                frame.displacements[i] = raw_mono_samples[start_idx + i];
            }

            let blend_len = 32.min(POINTS_PER_FRAME / 2);
            for i in 0..blend_len {
                let t = i as f32 / blend_len as f32;
                let s_start = frame.displacements[i];
                let s_end = frame.displacements[POINTS_PER_FRAME - blend_len + i];
                frame.displacements[i] = s_start * t + s_end * (1.0 - t);
            }

            frames.push(frame);
        }

        Ok(CachedSoundShape {
            sample_rate,
            total_frames: frames.len(),
            frames,
        })
    }

    pub fn get_interpolated_frame_at_time(&self, seconds: f32) -> Option<VisualFrame> {
        if self.frames.is_empty() {
            return None;
        }
        let total_time_frames = seconds * TARGET_FPS as f32;
        let idx0 = (total_time_frames.floor() as usize).min(self.frames.len() - 1);
        let idx1 = (idx0 + 1).min(self.frames.len() - 1);
        let fract = total_time_frames.fract();

        let f0 = &self.frames[idx0];
        let f1 = &self.frames[idx1];

        let mut result = VisualFrame {
            displacements: [0.0; POINTS_PER_FRAME],
        };

        for i in 0..POINTS_PER_FRAME {
            result.displacements[i] =
                f0.displacements[i] * (1.0 - fract) + f1.displacements[i] * fract;
        }

        Some(result)
    }
}

pub struct AudioSource {
    song: Song,
    elapsed_micros: Arc<AtomicU64>,
    agc_enabled: Arc<AtomicBool>,
    playback_start: Option<Instant>,
    cached_shape: Option<Arc<CachedSoundShape>>,
    player: Option<Arc<rodio::Player>>,
    sink: Option<Arc<rodio::stream::MixerDeviceSink>>,

    sample_rate: Option<u32>,
    channels: Option<u16>,
}

impl AudioSource {
    pub fn new(song: Song, agc_enabled: bool) -> Self {
        AudioSource {
            song,
            elapsed_micros: Arc::new(AtomicU64::new(0)),
            agc_enabled: Arc::new(AtomicBool::new(agc_enabled)),
            playback_start: None,
            cached_shape: None,
            player: None,
            sink: None,

            sample_rate: None,
            channels: None,
        }
    }

    pub fn sync(&mut self) -> bool {
        match CachedSoundShape::from_audio_file(&self.song.file_path) {
            Ok(shape) => {
                self.sample_rate = Some(shape.sample_rate);
                self.cached_shape = Some(Arc::new(shape));
            }
            Err(e) => eprintln!("Failed to parse audio file: {e}"),
        }

        let Ok(sink_handle) = rodio::DeviceSinkBuilder::open_default_sink() else {
            return false;
        };
        let Ok(file) = File::open(&self.song.file_path) else {
            return false;
        };
        let Ok(decoder) = rodio::Decoder::try_from(file) else {
            return false;
        };

        self.sample_rate = Some(decoder.sample_rate().into());
        self.channels = Some(decoder.channels().into());

        let agc_enabled_clone = self.agc_enabled.clone();
        let playback_pos = Arc::clone(&self.elapsed_micros);

        let agc_source = decoder.automatic_gain_control(Default::default());

        let controlled = agc_source.periodic_access(Duration::from_millis(5), move |agc_source| {
            agc_source.set_enabled(agc_enabled_clone.load(Ordering::Relaxed));
        });

        let final_source = controlled.pausable(true).track_position().periodic_access(
            Duration::from_millis(1),
            move |source| {
                let micros = source.get_pos().as_micros() as u64;
                playback_pos.store(micros, Ordering::Relaxed);
            },
        );

        let player = rodio::Player::connect_new(sink_handle.mixer());
        player.append(final_source);
        player.set_speed(432.0 / 440.0);

        self.player = Some(Arc::new(player));
        self.sink = Some(Arc::new(sink_handle));

        return true;
    }

    // pub fn fade_in(&self, duration: Duration) {
    //     if let Some(player) = self.player.as_ref() {
    //         player.
    //     };
    // }

    pub fn elapsed(&self) -> Duration {
        Duration::from_micros(self.elapsed_micros.load(Ordering::Relaxed))
    }

    pub fn current_visual_frame(&self) -> Option<VisualFrame> {
        self.cached_shape
            .as_ref()
            .and_then(|shape| shape.get_interpolated_frame_at_time(self.elapsed().as_secs_f32()))
    }

    pub fn play(&self) {
        if let Some(player) = self.player.as_ref() {
            player.play();
        };
    }

    pub fn pause(&self) {
        if let Some(player) = self.player.as_ref() {
            player.pause();
        };
    }

    pub fn toggle_play_pause(&self) {
        if let Some(player) = self.player.as_ref() {
            if player.is_paused() {
                self.play();
            } else {
                self.pause();
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(player) = self.player.as_ref() {
            self.pause();
            let _ = player.try_seek(Duration::from_secs(0));
        }

        self.elapsed_micros.store(0, Ordering::Relaxed);
    }

    pub fn is_playing(&self) -> bool {
        let Some(player) = self.player.as_ref() else {
            return false;
        };

        !player.is_paused()
    }

    pub fn is_paused(&self) -> bool {
        let Some(player) = self.player.as_ref() else {
            return false;
        };

        player.is_paused()
    }

    pub fn has_ended(&self) -> bool {
        let Some(player) = self.player.as_ref() else {
            return true;
        };

        player.empty()
    }

    pub fn seek(&self, position: Duration) -> Result<(), SeekError> {
        let Some(player) = self.player.as_ref() else {
            return Ok(());
        };

        player.try_seek(position)?;
        self.elapsed_micros
            .store(position.as_micros() as u64, Ordering::Relaxed);

        Ok(())
    }

    pub fn seek_relative(&self, offset: Duration, forward: bool) -> Result<(), SeekError> {
        let current = self.elapsed();
        let target = if forward {
            current.saturating_add(offset)
        } else {
            current.saturating_sub(offset)
        };
        self.seek(target)
    }

    pub fn duration(&self) -> Duration {
        self.song.duration
    }

    pub fn progress(&self) -> f32 {
        self.elapsed().as_secs_f32() / self.duration().as_secs_f32()
    }

    pub fn set_volume(&self, volume: f32) {
        if let Some(player) = self.player.as_ref() {
            player.set_volume(volume);
        };
    }

    pub fn volume(&self) -> f32 {
        if let Some(player) = self.player.as_ref() {
            return player.volume();
        };

        return 0.0;
    }

    pub fn set_agc_enabled(&self, enabled: bool) {
        self.agc_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_agc_enabled(&self) -> bool {
        self.agc_enabled.load(Ordering::Relaxed)
    }

    pub fn sample_rate(&self) -> Option<u32> {
        self.sample_rate
    }

    pub fn channels(&self) -> Option<u16> {
        self.channels
    }

    pub fn reset(&mut self) {
        self.stop();
        drop(self.player.take());
        drop(self.sink.take());
        self.agc_enabled.store(true, Ordering::Relaxed);
    }
}

impl Drop for AudioSource {
    fn drop(&mut self) {
        self.reset();
    }
}
