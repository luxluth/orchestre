use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicU64, mpsc::Sender},
    time::Duration,
};

use arc_swap::ArcSwap;

mod fonts;
mod icons;
mod orchestra;
mod pages;

use mtk::{
    Lens, Motion, PageTransition, switch,
    ui::{View, ViewAdaptExt, router},
    windowing::{Window, WindowAttributes},
};

use crate::{
    orchestra::{
        Orchestra,
        audio::{GlobalPlayerCmd, MusicPlayer, PlayerMsg},
        mu_thread::{AppMsg, Mu, MuCommand, OrchestraMsg},
        track::Song,
    },
    pages::{
        Theme,
        album::{AlbumMsg, AlbumState},
        landing::{LandingMsg, LandingState},
        library::{FilterTag, LibraryMsg, LibraryState, Order, SortMetric},
    },
};

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Landing,
    Library,
    Album,
}

#[derive(Lens)]
pub struct Supervisor {
    current_page: Page,
    pub mu_sx: Sender<MuCommand>,
    pub mp_sx: Sender<GlobalPlayerCmd>,
    pub progress: Duration,
    pub progress_raw: Arc<AtomicU64>,
    pub landing: LandingState,
    pub library: LibraryState,
    pub album_page: AlbumState,
    pub theme: Theme,
    pub orchestra: Option<Arc<ArcSwap<Orchestra>>>,
}

fn sort_songs(state: &mut Supervisor) {
    let Some(orch) = state.orchestra.as_ref() else {
        return;
    };

    let guard = orch.load();

    let mut songs: Vec<Song> = guard.collection.songs.values().cloned().collect();

    songs.sort_by(|a, b| {
        let ordering = match state.library.active_filter.metric {
            SortMetric::ByDate => a
                .created_at
                .cmp(&b.created_at)
                .then_with(|| a.title.cmp(&b.title)),
            SortMetric::ByTitle => a
                .title
                .chars()
                .map(|c| c.to_ascii_lowercase())
                .cmp(b.title.chars().map(|c| c.to_ascii_lowercase())),
        };

        match state.library.active_filter.order {
            Order::Asc => ordering,
            Order::Desc => ordering.reverse(),
        }
    });

    state.library.sorted_songs = Arc::new(songs);
}

fn update(state: &mut Supervisor, msg: AppMsg) {
    match msg {
        AppMsg::Orchestra(omsg) => match omsg {
            OrchestraMsg::Ready(orch) => {
                let _ = state
                    .mp_sx
                    .send(GlobalPlayerCmd::SetOrchestra(orch.clone()));
                state.landing.log = None;
                state.landing.is_indexing = true;
                state.orchestra = Some(orch.clone());
                sort_songs(state);
                state.current_page = Page::Library;
            }
            OrchestraMsg::NeedIndexing => {
                state.landing.is_indexing = false;
                // let _ = state.mu_sx.send(MuCommand::StartIndexing);
            }
            OrchestraMsg::Indexing(file_path) => {
                state.landing.log = Some(format!("+ {}", file_path.as_os_str().to_string_lossy()))
            }
        },
        AppMsg::Landing(lmsg) => match lmsg {
            LandingMsg::StartIndexing => {
                let path = PathBuf::from(&state.landing.music_dir);
                if path.exists() && path.is_dir() {
                    state.landing.is_indexing = true;
                    state.landing.error = None;
                    let _ = state.mu_sx.send(MuCommand::StartIndexing(path));
                } else {
                    state.landing.error = Some("Directory does not exist or is invalid".into());
                }
            }
            LandingMsg::FolderPicked(folder) => {
                state.landing.music_dir = folder.as_os_str().to_string_lossy().to_string();
                state.landing.error = None;
            }
            LandingMsg::PickFolder => {
                let _ = state.mu_sx.send(MuCommand::PickFolder(|path| {
                    AppMsg::Landing(LandingMsg::FolderPicked(path))
                }));
            }
        },
        AppMsg::Library(msg) => match msg {
            LibraryMsg::HoverSong(id) => {
                state.library.hovered_song = Some(id);
            }
            LibraryMsg::SetFilterTag(filter_tag) => {
                state.library.active_filter.tag = filter_tag;
                if filter_tag == FilterTag::Songs {
                    sort_songs(state);
                }
            }
            LibraryMsg::SetFilterOrder(order) => {
                state.library.active_filter.order = order;
                if state.library.active_filter.tag == FilterTag::Songs {
                    sort_songs(state);
                }
            }
            LibraryMsg::SetSortMetric(sort_metric) => {
                state.library.active_filter.metric = sort_metric;
                if state.library.active_filter.tag == FilterTag::Songs {
                    sort_songs(state);
                }
            }
            LibraryMsg::ClickArtist(artist_id, _) => {
                let orch = state.orchestra.as_ref().unwrap();
                let guard = orch.load();
                let artist = guard.get_artist(&artist_id);
                println!("{artist:?}");
            }
            LibraryMsg::ClickAlbum(album_id) => {
                state.album_page = AlbumState {
                    album_id,
                    hovered_song_id: None,
                };
                state.current_page = Page::Album;
            }
            LibraryMsg::SetListRunOffset(offset) => {
                state.library.list_run_offset = offset;
            }
            LibraryMsg::Play(id) => {
                let _ = state.mp_sx.send(GlobalPlayerCmd::ClearQueue);
                let _ = state.mp_sx.send(GlobalPlayerCmd::Enqueue(id));
                let _ = state.mp_sx.send(GlobalPlayerCmd::Play);
            }
        },

        AppMsg::AlbumPage(msg) => match msg {
            AlbumMsg::GotoLibrary => {
                state.current_page = Page::Library;
            }
            AlbumMsg::ClickArtist(artist_id, _) => {
                let orch = state.orchestra.as_ref().unwrap();
                let guard = orch.load();
                let artist = guard.get_artist(&artist_id);
                println!("{artist:?}");
            }
            AlbumMsg::HoverSong(id) => {
                state.album_page.hovered_song_id = Some(id);
            }
            AlbumMsg::HoverSongQuit => {
                state.album_page.hovered_song_id = None;
            }
        },
        AppMsg::Player(player_msg) => match player_msg {
            PlayerMsg::Tick(duration) => {
                state.progress = duration;
            }
        },
    }
}

fn app(state: &Supervisor) -> impl View<Supervisor, Message = AppMsg> + use<> {
    router(state.current_page, render_page(state)).transition_spec(|from, to| match from {
        Page::Landing => PageTransition::fade().duration_ms(220.0),
        Page::Library => match to {
            Page::Album => {
                PageTransition::asymmetric(Motion::slide_in_bottom(), Motion::stationary())
                    .duration_ms(220.0)
            }
            _ => PageTransition::fade().duration_ms(220.0),
        },
        Page::Album => PageTransition::asymmetric(Motion::fade_in(), Motion::slide_out_bottom())
            .duration_ms(220.0),
    })
}

fn render_page(state: &Supervisor) -> impl View<Supervisor, Message = AppMsg> + use<> {
    switch! {
        match state.current_page {
            Page::Landing => pages::landing::render(&state.landing, state.theme)
                .adapt(Supervisor::landing, AppMsg::Landing),

            Page::Library => pages::library::render(&state.library, state.orchestra.clone(), state.theme)
                .adapt(Supervisor::library, AppMsg::Library),

            Page::Album => pages::album::render(&state.album_page, state.orchestra.clone(), state.theme)
                .adapt(Supervisor::album_page, AppMsg::AlbumPage),
        }
    }
}

fn main() {
    let _ = env_logger::try_init();

    let (width, height) = (600, 600);

    let mu = Mu::new();
    let (music_player, mp_sx) = MusicPlayer::new();
    let mu_sx = mu.sender();

    let orchestra_mgr = Supervisor {
        mu_sx,
        mp_sx,
        progress: Duration::from_secs(0),
        progress_raw: Arc::new(AtomicU64::new(0)),
        current_page: Page::Landing,
        landing: LandingState::default(),
        library: LibraryState::default(),
        album_page: AlbumState::default(),
        theme: Theme::Light,
        orchestra: None,
    };

    let progress_ref = orchestra_mgr.progress_raw.clone();

    let mut window = Window::with(orchestra_mgr, update, app);
    window = fonts::Font::Iosevka.load(window);
    window = fonts::Font::InterVariable.load(window);
    window = fonts::Font::NotoSansCJK.load(window);

    mu.spawn(window.handle());
    music_player.spawn(window.handle(), progress_ref);

    #[cfg(feature = "debug")]
    window.enable_terminal_debugger();

    window.present_with(
        WindowAttributes::default()
            .with_title("Orchestre")
            .with_size((width, height).into())
            .with_app_id("orchestre")
            .with_min_size(Some((970, 630).into()))
            .with_resizable(true),
    );
}
