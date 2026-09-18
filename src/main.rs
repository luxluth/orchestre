use std::{
    path::PathBuf,
    sync::{Arc, mpsc::Sender},
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
        mu_thread::{AppMsg, Mu, MuCommand, OrchestraMsg},
    },
    pages::{
        Theme,
        album::{AlbumMsg, AlbumState},
        landing::{LandingMsg, LandingState},
        library::{LibraryMsg, LibraryState},
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
    pub landing: LandingState,
    pub library: LibraryState,
    pub album_page: AlbumState,
    pub theme: Theme,
    pub orchestra: Option<Arc<ArcSwap<Orchestra>>>,
}

fn update(state: &mut Supervisor, msg: AppMsg) {
    match msg {
        AppMsg::Orchestra(omsg) => match omsg {
            OrchestraMsg::Ready(orch) => {
                state.landing.log = None;
                state.landing.is_indexing = true;
                state.orchestra = Some(orch);
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
            }
            LibraryMsg::SetFilterOrder(order) => {
                state.library.active_filter.order = order;
            }
            LibraryMsg::SetSortMetric(sort_metric) => {
                state.library.active_filter.metric = sort_metric;
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
    let mu_sx = mu.sender();

    let orchestra_mgr = Supervisor {
        mu_sx,
        current_page: Page::Landing,
        landing: LandingState::default(),
        library: LibraryState::default(),
        album_page: AlbumState::default(),
        theme: Theme::Dark,
        orchestra: None,
    };

    let mut window = Window::with(orchestra_mgr, update, app);
    window = fonts::Font::Iosevka.load(window);
    window = fonts::Font::InterVariable.load(window);
    window = fonts::Font::NotoSansCJK.load(window);

    mu.spawn(window.handle());

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
