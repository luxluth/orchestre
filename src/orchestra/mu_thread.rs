use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Sender},
    },
};

use arc_swap::ArcSwap;
use mtk::windowing::WindowHandle;

use crate::{
    orchestra::{Orchestra, audio::PlayerMsg},
    pages::{album::AlbumMsg, landing::LandingMsg, library::LibraryMsg},
};

pub enum MuCommand {
    StartIndexing(PathBuf),
    PickFolder(fn(PathBuf) -> AppMsg),
}

#[derive(Clone)]
pub enum OrchestraMsg {
    Ready(Arc<ArcSwap<Orchestra>>),
    NeedIndexing,
    Indexing(PathBuf),
}

#[derive(Clone)]
pub enum AppMsg {
    Orchestra(OrchestraMsg),
    Landing(LandingMsg),
    Library(LibraryMsg),
    AlbumPage(AlbumMsg),
    Player(PlayerMsg),
}

pub struct Mu {
    sx: mpsc::Sender<MuCommand>,
    rx: mpsc::Receiver<MuCommand>,
}

impl Mu {
    pub fn new() -> Self {
        let (sx, rx) = mpsc::channel();
        Mu { sx, rx }
    }

    pub fn sender(&self) -> Sender<MuCommand> {
        self.sx.clone()
    }

    pub fn spawn(self, handle: WindowHandle<AppMsg>) {
        std::thread::Builder::new()
            .name("mumanager".to_string())
            .spawn(move || {
                let orchestra = Arc::new(ArcSwap::from_pointee(Orchestra::new()));

                let mut init_orch = Orchestra::new();
                if init_orch.load_from_cache() {
                    orchestra.store(Arc::new(init_orch));
                    let _ = handle.send(AppMsg::Orchestra(OrchestraMsg::Ready(orchestra.clone())));
                } else {
                    let _ = handle.send(AppMsg::Orchestra(OrchestraMsg::NeedIndexing));
                }

                while let Ok(cmd) = self.rx.recv() {
                    match cmd {
                        MuCommand::StartIndexing(dir) => {
                            let mut new_orch = (**orchestra.load()).clone();
                            new_orch.index(dir, &handle);
                            new_orch.save();
                            orchestra.store(Arc::new(new_orch));
                            let _ = handle
                                .send(AppMsg::Orchestra(OrchestraMsg::Ready(orchestra.clone())));
                        }
                        MuCommand::PickFolder(to_msg) => {
                            let h = handle.clone();
                            std::thread::spawn(move || {
                                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                    let _ = h.send(to_msg(folder));
                                }
                            });
                        }
                    }
                }
            })
            .unwrap();
    }
}
