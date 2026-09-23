use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use std::{collections::HashMap, sync::mpsc};

use arc_swap::ArcSwap;
use mtk::windowing::WindowHandle;
use rand::seq::SliceRandom;
use uuid::Uuid;

use crate::orchestra::Orchestra;
use crate::orchestra::mu_thread::AppMsg;
use crate::orchestra::track::{Id, MusicCollection};

use super::source::AudioSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueItem {
    pub queue_id: Uuid,
    pub song_id: Id,
}

impl QueueItem {
    pub fn new(song_id: Id) -> Self {
        Self {
            queue_id: Uuid::new_v4(),
            song_id,
        }
    }
}

pub struct PlayingSong {
    pub queue_id: Uuid,
    pub song_id: Id,
    pub qorigin: QueueKind,
    pub source: AudioSource,
}

pub struct MusicPlayer {
    pub playback_queue: Vec<QueueItem>,
    pub override_queue: Vec<QueueItem>,
    pub order: Vec<usize>,
    pub cursor: usize,

    pub playing_song: Option<PlayingSong>,
    pub history: Vec<QueueItem>,

    pub loop_mode: LoopMode,
    pub is_shuffled: bool,

    rx: mpsc::Receiver<GlobalPlayerCmd>,
    orchestra: Option<Arc<ArcSwap<Orchestra>>>,
    progress_ref: Option<Arc<AtomicU64>>,
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

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
enum ClusterKey {
    Artist(Id),
    Album(Id),
    Track(Id),
}

#[derive(Clone)]
pub enum GlobalPlayerCmd {
    SetOrchestra(Arc<ArcSwap<Orchestra>>),
    Enqueue(Id),
    EnqueueMany(Vec<Id>),
    ClearQueue,
    Play,
    Next,
}

#[derive(Clone)]
pub enum PlayerMsg {
    Tick(Duration),
}

impl MusicPlayer {
    pub fn new() -> (Self, mpsc::Sender<GlobalPlayerCmd>) {
        let (sx, rx) = mpsc::channel();
        (
            Self {
                playback_queue: Vec::new(),
                override_queue: Vec::new(),
                order: Vec::new(),
                cursor: 0,

                playing_song: None,
                history: Vec::new(),

                loop_mode: LoopMode::default(),
                is_shuffled: false,

                rx,
                orchestra: None,
                progress_ref: None,
            },
            sx,
        )
    }

    fn handle_cmd(
        &mut self,
        cmd: GlobalPlayerCmd,
        _handle: &WindowHandle<super::mu_thread::AppMsg>,
    ) {
        match cmd {
            GlobalPlayerCmd::SetOrchestra(orch) => self.orchestra = Some(orch),
            GlobalPlayerCmd::Enqueue(song_id) => {
                self.enqueue(song_id);
            }
            GlobalPlayerCmd::EnqueueMany(ids) => {
                for song_id in ids {
                    self.enqueue(song_id);
                }
            }
            GlobalPlayerCmd::Play => {
                if let Some(orch) = self.orchestra.as_ref() {
                    let guard = orch.load();
                    if let Some(ps) = self.playing_song.as_ref() {
                        if ps.source.is_paused() {
                            ps.source.play();
                            return;
                        }
                    }
                    // Start playback of the track currently at cursor
                    if self.cursor < self.order.len() {
                        let actual_idx = self.order[self.cursor];
                        let item = self.playback_queue[actual_idx];
                        self.load_and_play(item, QueueKind::Normal, &guard.collection);
                    }
                }
            }
            GlobalPlayerCmd::Next => {
                if let Some(orch) = self.orchestra.as_ref() {
                    let guard = orch.load();
                    self.next(&guard.collection);
                } else {
                    // TODO: should be unreachable!()
                }
            }
            GlobalPlayerCmd::ClearQueue => {
                self.clear_queue();
            }
        }
    }

    pub fn spawn(
        mut self,
        handle: WindowHandle<super::mu_thread::AppMsg>,
        progress_ref: Arc<AtomicU64>,
    ) {
        self.progress_ref = Some(progress_ref);
        std::thread::Builder::new()
            .name("musicplayer".to_string())
            .spawn(move || {
                loop {
                    let is_active = self
                        .playing_song
                        .as_ref()
                        .map_or(false, |ps| ps.source.is_playing());

                    let cmd_res = if is_active {
                        self.rx.recv_timeout(Duration::from_millis(500))
                    } else {
                        self.rx
                            .recv()
                            .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                    };

                    match cmd_res {
                        Ok(cmd) => {
                            self.handle_cmd(cmd, &handle);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // Heartbeat tick:
                            if let Some(ps) = self.playing_song.as_ref() {
                                let elapsed = ps.source.elapsed();
                                let _ = handle.send(AppMsg::Player(PlayerMsg::Tick(elapsed)));
                            }

                            // Auto-advance if track finished
                            if let Some(orch) = self.orchestra.as_ref() {
                                let guard = orch.load();
                                self.update(&guard.collection);
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .unwrap();
    }

    /// Appends a track to the end of the playback queue
    pub fn enqueue(&mut self, song_id: Id) {
        let idx = self.playback_queue.len();
        self.playback_queue.push(QueueItem::new(song_id));
        self.order.push(idx);
    }

    /// Inserts a track directly after the currently playing item
    pub fn enqueue_next(&mut self, song_id: Id) {
        self.override_queue.push(QueueItem::new(song_id));
    }

    /// Removes a track at a specific index from the specified queue
    pub fn remove(&mut self, index: usize, qkind: QueueKind) -> Option<QueueItem> {
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

    pub fn set_shuffle(&mut self, enabled: bool, collection: &MusicCollection) {
        if self.is_shuffled == enabled {
            return;
        }

        self.is_shuffled = enabled;

        if self.playback_queue.is_empty() {
            self.order.clear();
            self.cursor = 0;
            return;
        }

        if enabled {
            let key_extractor = |&idx: &usize| -> ClusterKey {
                let song_id = self.playback_queue[idx].song_id;
                let Some(song) = collection.songs.get(&song_id) else {
                    return ClusterKey::Track(song_id);
                };

                if let Some(&artist_id) = song.artists.first() {
                    ClusterKey::Artist(artist_id)
                } else if let Some(album_id) = song.album {
                    ClusterKey::Album(album_id)
                } else {
                    ClusterKey::Track(song_id)
                }
            };

            let is_normal_playing = self
                .playing_song
                .as_ref()
                .map_or(false, |ps| ps.qorigin == QueueKind::Normal);

            if is_normal_playing && self.cursor < self.order.len() {
                let current_actual_idx = self.order[self.cursor];

                let remaining: Vec<usize> = (0..self.playback_queue.len())
                    .filter(|&i| i != current_actual_idx)
                    .collect();

                let shuffled_remaining = balanced_shuffle(&remaining, key_extractor);

                let mut new_order = Vec::with_capacity(self.playback_queue.len());
                new_order.push(current_actual_idx);
                new_order.extend(shuffled_remaining);

                self.order = new_order;
                self.cursor = 0;
            } else {
                let all_indices: Vec<usize> = (0..self.playback_queue.len()).collect();
                self.order = balanced_shuffle(&all_indices, key_extractor);
                self.cursor = 0;
            }
        } else {
            // Restore chronological order and reposition cursor to the active track
            if let Some(current_actual_idx) = self.order.get(self.cursor).copied() {
                self.order = (0..self.playback_queue.len()).collect();
                self.cursor = current_actual_idx;
            } else {
                self.order = (0..self.playback_queue.len()).collect();
                self.cursor = 0;
            }
        }
    }

    /// Clears upcoming tracks without stopping the active song
    pub fn clear_queue(&mut self) {
        self.override_queue.clear();
        self.playback_queue.clear();
        self.order.clear();
        self.history.clear();
        self.cursor = 0;
    }

    fn load_and_play(&mut self, item: QueueItem, qorigin: QueueKind, collection: &MusicCollection) {
        let Some(song) = collection.songs.get(&item.song_id) else {
            return;
        };

        eprintln!("[debug]: song to play :: {song:?}");

        let mut source = AudioSource::new(
            song.file_path.clone(),
            song.duration,
            true,
            self.progress_ref.clone().unwrap(),
        );

        if source.sync() {
            source.play();
            self.playing_song = Some(PlayingSong {
                queue_id: item.queue_id,
                song_id: item.song_id,
                qorigin,
                source,
            });
        }
    }

    /// Transitions to the next song in queue
    pub fn next(&mut self, collection: &MusicCollection) -> bool {
        if let Some(ps) = self.playing_song.as_ref() {
            if ps.qorigin == QueueKind::Normal {
                self.history.push(QueueItem {
                    queue_id: ps.queue_id,
                    song_id: ps.song_id,
                });
            }
        }

        if !self.override_queue.is_empty() {
            let item = self.override_queue.remove(0);
            self.load_and_play(item, QueueKind::Override, collection);
            return true;
        }

        let next_cursor = match &self.playing_song {
            Some(ps) if ps.qorigin == QueueKind::Normal => self.cursor + 1,
            _ => self.cursor,
        };

        if next_cursor < self.order.len() {
            self.cursor = next_cursor;
            let actual_idx = self.order[self.cursor];
            let item = self.playback_queue[actual_idx];
            self.load_and_play(item, QueueKind::Normal, collection);
            return true;
        } else {
            self.playing_song = None;
            return false;
        }
    }

    pub fn previous(&mut self, collection: &MusicCollection) {
        if let Some(ps) = self.playing_song.as_ref() {
            if ps.source.elapsed() > Duration::from_secs(5) {
                let _ = ps.source.seek(Duration::from_secs(0));
                return;
            }
        }

        if let Some(ps) = self.playing_song.as_ref() {
            if ps.qorigin == QueueKind::Override {
                if self.cursor < self.order.len() {
                    let actual_idx = self.order[self.cursor];
                    let item = self.playback_queue[actual_idx];
                    self.load_and_play(item, QueueKind::Normal, collection);
                }
                return;
            }
        }

        let Some(prev_item) = self.history.pop() else {
            if let Some(ps) = self.playing_song.as_ref() {
                let _ = ps.source.seek(Duration::from_secs(0));
            }
            return;
        };

        self.cursor = self.cursor.saturating_sub(1);
        self.load_and_play(prev_item, QueueKind::Normal, collection);
    }

    pub fn queue<'a>(&'a self, qkind: QueueKind) -> Box<dyn Iterator<Item = &'a QueueItem> + 'a> {
        match qkind {
            QueueKind::Normal => {
                let start = match &self.playing_song {
                    Some(ps) if ps.qorigin == QueueKind::Normal => {
                        (self.cursor + 1).min(self.order.len())
                    }
                    _ => self.cursor.min(self.order.len()),
                };

                Box::new(
                    self.order[start..]
                        .iter()
                        .map(|&idx| &self.playback_queue[idx]),
                )
            }
            QueueKind::Override => Box::new(self.override_queue.iter()),
        }
    }

    pub fn update(&mut self, collection: &MusicCollection) {
        let has_ended = self
            .playing_song
            .as_ref()
            .map_or(false, |ps| ps.source.has_ended());

        if !has_ended {
            return;
        }

        if self.loop_mode == LoopMode::Track {
            if let Some(ps) = self.playing_song.as_ref() {
                let item = QueueItem {
                    queue_id: ps.queue_id,
                    song_id: ps.song_id,
                };
                let qorigin = ps.qorigin;
                self.load_and_play(item, qorigin, collection);
                return;
            }
        }

        self.next(collection);

        if self.playing_song.is_none()
            && self.loop_mode == LoopMode::Queue
            && !self.order.is_empty()
        {
            self.cursor = 0;
            let actual_idx = self.order[0];
            let item = self.playback_queue[actual_idx];
            self.load_and_play(item, QueueKind::Normal, collection);
        }
    }

    pub fn set_loop_mode(&mut self, mode: LoopMode) {
        self.loop_mode = mode;
    }
}

impl MusicPlayer {
    pub fn play_by_id(
        &mut self,
        queue_id: Uuid,
        qkind: QueueKind,
        collection: &MusicCollection,
    ) -> bool {
        match qkind {
            QueueKind::Normal => {
                let found_pos = self
                    .order
                    .iter()
                    .enumerate()
                    .find_map(|(pos, &actual_idx)| {
                        if self.playback_queue[actual_idx].queue_id == queue_id {
                            Some((pos, actual_idx))
                        } else {
                            None
                        }
                    });

                let Some((target_pos, actual_idx)) = found_pos else {
                    return false;
                };

                if let Some(ps) = self.playing_song.as_ref() {
                    if ps.qorigin == QueueKind::Normal && target_pos > self.cursor {
                        for pos in self.cursor..target_pos {
                            let idx = self.order[pos];
                            self.history.push(self.playback_queue[idx]);
                        }
                    }
                }

                self.cursor = target_pos;
                let item = self.playback_queue[actual_idx];
                self.load_and_play(item, QueueKind::Normal, collection);
                true
            }
            QueueKind::Override => {
                let pos = self
                    .override_queue
                    .iter()
                    .position(|item| item.queue_id == queue_id);

                let Some(idx) = pos else {
                    return false;
                };

                let item = self.override_queue.remove(idx);
                self.load_and_play(item, QueueKind::Override, collection);
                true
            }
        }
    }
}

impl MusicPlayer {
    // Applies global gain scaling across all active and future players
    // pub fn set_master_volume(&mut self, volume: f32) {}

    // Toggles output mute while caching previous gain settings
    // pub fn toggle_mute(&mut self) {}

    // Fades down the active track while initializing and fading up the next track concurrently
    // pub fn crossfade(&mut self, duration: Duration) {}

    // Smoothly lowers gain before halting playback.
    // pub fn fade_out_and_pause(&mut self, duration: Duration) {}

    // Returns the active audio device name
    // pub fn current_output_device(&self) -> String {}
}

fn balanced_shuffle<T: Clone, K: std::hash::Hash + Eq>(
    items: &[T],
    key_extractor: impl Fn(&T) -> K,
) -> Vec<T> {
    if items.len() <= 2 {
        let mut res = items.to_vec();
        res.shuffle(&mut rand::rng());
        return res;
    }

    let mut buckets: HashMap<K, Vec<T>> = HashMap::new();
    for item in items {
        buckets
            .entry(key_extractor(item))
            .or_default()
            .push(item.clone());
    }

    let mut rng = rand::rng();
    for bucket in buckets.values_mut() {
        bucket.shuffle(&mut rng);
    }

    let mut bucket_list: Vec<Vec<T>> = buckets.into_values().collect();
    let mut result = Vec::with_capacity(items.len());

    while !bucket_list.is_empty() {
        // Sort descending by remaining tracks to prevent starvation at the end
        bucket_list.sort_by_key(|b| std::cmp::Reverse(b.len()));

        let mut i = 0;
        while i < bucket_list.len() {
            if let Some(item) = bucket_list[i].pop() {
                result.push(item);
            }
            if bucket_list[i].is_empty() {
                bucket_list.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    // Add mild localized jitter to soften round-robin rhythm
    for i in 0..result.len().saturating_sub(2) {
        if rand::random::<f32>() < 0.35 {
            result.swap(i + 1, i + 2);
        }
    }

    result
}
