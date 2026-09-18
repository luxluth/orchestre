use std::time::Duration;

use super::source::AudioSource;
use super::track::{Id, Song};

pub struct PlayingSong {
    id: Id,
    qorigin: QueueKind,
    source: AudioSource,
}

#[derive(Default)]
pub struct MusicPlayer {
    pub playback_queue: Vec<Song>,
    pub override_queue: Vec<Song>,
    pub pq_cursor: usize,

    pub playing_song: Option<PlayingSong>,
    pub history: usize,

    pub loop_mode: LoopMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QueueKind {
    Normal,
    Override,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum LoopMode {
    #[default]
    Off,
    Track,
    Queue,
}

impl MusicPlayer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a track to the end of the playback queue
    pub fn enqueue(&mut self, song: Song) {
        self.playback_queue.push(song);
    }

    /// Inserts a track directly after the currently playing item
    pub fn enqueue_next(&mut self, song: Song) {
        self.override_queue.push(song);
    }

    /// Removes a track at a specific index from the specified queue
    pub fn remove(&mut self, index: usize, qkind: QueueKind) -> Option<Song> {
        match qkind {
            QueueKind::Normal => {
                if index < self.playback_queue.len() {
                    Some(self.playback_queue.remove(index))
                } else {
                    None
                }
            }
            QueueKind::Override => {
                if index < self.playback_queue.len() {
                    Some(self.playback_queue.remove(index))
                } else {
                    None
                }
            }
        }
    }

    /// Clears upcoming tracks without stopping the active song
    pub fn clear_queue(&mut self) {
        self.override_queue.clear();
        self.playback_queue.clear();
        self.history = 0;
        self.pq_cursor = 0;
    }

    fn load_and_play(&mut self, song: Song, qorigin: QueueKind) {
        let id = song.id;
        // TODO:(agc_enable) make agc_enable configurable and check sync output
        let mut source = AudioSource::new(song, true);
        if source.sync() {
            source.play();
            self.playing_song = Some(PlayingSong {
                id,
                qorigin,
                source,
            });
        } else {
            // TODO: notify not playable
        }
    }

    /// Transitions to the next song in queue
    pub fn next(&mut self) {
        if let Some(ps) = self.playing_song.as_ref() {
            if ps.qorigin == QueueKind::Normal {
                self.history += 1;
            }
        }

        if !self.override_queue.is_empty() {
            let song = self.override_queue.remove(0);
            self.load_and_play(song, QueueKind::Override);
            return;
        }

        let next_idx = match &self.playing_song {
            Some(ps) if ps.qorigin == QueueKind::Normal => self.pq_cursor + 1,
            _ => self.pq_cursor,
        };

        if next_idx < self.playback_queue.len() {
            self.pq_cursor = next_idx;
            let song = self.playback_queue[self.pq_cursor].clone();
            self.load_and_play(song, QueueKind::Normal);
        } else {
            self.playing_song = None;
        }
    }

    pub fn previous(&mut self) {
        if let Some(ps) = self.playing_song.as_ref() {
            if ps.source.elapsed() > Duration::from_secs(5) {
                let _ = ps.source.seek(Duration::from_secs(0));
                return;
            }
        }

        if let Some(ps) = self.playing_song.as_ref() {
            if ps.qorigin == QueueKind::Override {
                if self.pq_cursor < self.playback_queue.len() {
                    let song = self.playback_queue[self.pq_cursor].clone();
                    self.load_and_play(song, QueueKind::Normal);
                }
                return;
            }
        }

        if self.history == 0 || self.pq_cursor == 0 {
            if let Some(ps) = self.playing_song.as_ref() {
                let _ = ps.source.seek(Duration::from_secs(0));
            }
            return;
        }

        self.history = self.history.saturating_sub(1);
        self.pq_cursor = self.pq_cursor.saturating_sub(1);

        if let Some(song) = self.playback_queue.get(self.pq_cursor).cloned() {
            self.load_and_play(song, QueueKind::Normal);
        }
    }

    pub fn play_index(&mut self, index: usize, qkind: QueueKind) {
        match qkind {
            QueueKind::Normal => {
                if index < self.playback_queue.len() {
                    if let Some(ps) = self.playing_song.as_ref() {
                        if ps.qorigin == QueueKind::Normal && index > self.pq_cursor {
                            self.history += index - self.pq_cursor;
                        }
                    }
                    self.pq_cursor = index;
                    let song = self.playback_queue[self.pq_cursor].clone();
                    self.load_and_play(song, QueueKind::Normal);
                }
            }
            QueueKind::Override => {
                if index < self.override_queue.len() {
                    let song = self.override_queue.remove(index);
                    self.load_and_play(song, QueueKind::Override);
                }
            }
        }
    }

    pub fn queue(&self, qkind: QueueKind) -> &[Song] {
        match qkind {
            QueueKind::Normal => {
                let start = match &self.playing_song {
                    Some(ps) if ps.qorigin == QueueKind::Normal => {
                        (self.pq_cursor + 1).min(self.playback_queue.len())
                    }
                    _ => self.pq_cursor.min(self.playback_queue.len()),
                };
                &self.playback_queue[start..]
            }
            QueueKind::Override => &self.override_queue,
        }
    }

    pub fn update(&mut self) {
        let has_ended = self
            .playing_song
            .as_ref()
            .map_or(false, |ps| ps.source.has_ended());

        if has_ended {
            if self.loop_mode == LoopMode::Track
                && let Some(ps) = self.playing_song.as_ref()
            {
                ps.source.pause();
                let _ = ps.source.seek(Duration::from_secs(0));
                ps.source.play();
                return;
            }

            self.next();

            if self.playing_song.is_none() && self.loop_mode == LoopMode::Queue {
                self.pq_cursor = 0;
                self.next();
            }
        }
    }

    pub fn set_loop_mode(&mut self, mode: LoopMode) {
        self.loop_mode = mode;
    }
}
