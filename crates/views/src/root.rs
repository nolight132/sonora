// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::{AnyView, Context, Entity, MouseButton, NavigationDirection, Render};
use gpui::{Window, div};
use gpui::{font, prelude::*};
use input::{OpenFilter, OpenSearch, OpenSettings, ToggleFullscreen};
use router::{Destination, NavigationEvent, back, forward, navigate};
use state::{
    ArtistDetail, Detail, Home, Io, Library, Playback, Queue, Search, Session, SessionState,
    SongDetail,
};
use ui::ActiveTheme as _;

use crate::chrome::{TitleBar, TitleBarEvent, TitleBarOptions, Toolbar, Tooled};
use crate::screens::search::SearchView;
use crate::shared::tracks::{ALBUM_COLUMNS, ARTIST_COLUMNS, LIBRARY_COLUMNS};
use crate::shells::Shell;
use crate::shells::workspace::Workspace;
use crate::{
    Adaptive, ArtistView, DetailView, FullscreenView, HomeView, LibraryView, LoginView,
    SettingsView, SongView,
};

struct Screens {
    home: Entity<HomeView>,
    library: Entity<LibraryView>,
    artist: Option<Entity<ArtistView>>,
    artist_detail: Option<Entity<ArtistDetail>>,
    album: Entity<DetailView>,
    album_detail: Entity<Detail>,
    song: Entity<SongView>,
    song_detail: Entity<SongDetail>,
    playlist: Entity<DetailView>,
    playlist_detail: Entity<Detail>,
    search: Entity<SearchView>,
    settings: Entity<SettingsView>,
}

struct Shells {
    workspace: Entity<Workspace>,
    fullscreen: Entity<FullscreenView>,
}

enum RootView {
    Workspace,
    Fullscreen,
}

enum Focus {
    Search,
    Workspace,
    Fullscreen,
}

pub struct Root {
    session: Entity<Session>,
    playback: Entity<Playback>,
    io: Io,
    login: Entity<LoginView>,
    title_bar: Entity<TitleBar>,
    shells: Shells,
    view: RootView,
    toolbar: Option<Entity<Toolbar>>,
    pending: Option<Focus>,
    screens: Screens,
    _adaptive: Entity<Adaptive>,
}

impl Root {
    pub fn new(
        session: Entity<Session>,
        library: Entity<Library>,
        playback: Entity<Playback>,
        queue: Entity<Queue>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&session, |_, _, cx| cx.notify()).detach();

        let login = cx.new(|cx| LoginView::new(session.clone(), cx));

        let navigation = router::trail(cx);

        cx.subscribe(&navigation, |this, _, event, cx| {
            let NavigationEvent::Moved(destination) = event;
            this.show(destination.clone(), cx);
        })
        .detach();

        let library_view =
            cx.new(|cx| LibraryView::new(library.clone(), playback.clone(), window, cx));

        let home_state = cx.new(|cx| Home::new(library.clone(), cx));
        let home = cx.new(|cx| HomeView::new(home_state, playback.clone(), cx));

        let io = Io::global(cx);
        let search_library = library.clone();
        let album_detail =
            cx.new(|cx| Detail::new(session.clone(), library.clone(), io.clone(), cx));
        let album = cx.new(|cx| {
            DetailView::new(
                album_detail.clone(),
                playback.clone(),
                ALBUM_COLUMNS,
                true,
                "album",
                window,
                cx,
            )
        });

        let playlist_detail = cx.new(|cx| Detail::new(session.clone(), library, io.clone(), cx));
        let playlist = cx.new(|cx| {
            DetailView::new(
                playlist_detail.clone(),
                playback.clone(),
                LIBRARY_COLUMNS,
                true,
                "playlist",
                window,
                cx,
            )
        });

        let queries = cx.new(|cx| Search::new(session.clone(), search_library, io.clone(), cx));
        let search = cx.new(|cx| SearchView::new(queries, playback.clone(), cx));

        let settings = cx.new(|cx| SettingsView::new(session.clone(), playback.clone(), cx));

        let song_detail = cx.new(|cx| SongDetail::new(session.clone(), io.clone(), cx));
        let song = cx.new(|cx| SongView::new(song_detail.clone(), playback.clone(), cx));

        let start = navigation.read(cx).current();
        let workspace =
            cx.new(|cx| Workspace::new(playback.clone(), queue, library_view.clone().into(), cx));
        let fullscreen = cx.new(|cx| FullscreenView::new(playback.clone(), cx));

        let title_bar = cx.new(TitleBar::new);
        cx.subscribe(&title_bar, |this, _, event, cx| match event {
            TitleBarEvent::ToggleSidebar => this
                .shells
                .workspace
                .update(cx, |workspace, cx| workspace.toggle_sidebar(cx)),
        })
        .detach();

        let adaptive = cx.new(|cx| Adaptive::new(playback.clone(), cx));

        let mut root = Self {
            session,
            playback,
            io,
            login,
            title_bar,
            shells: Shells {
                workspace: workspace.clone(),
                fullscreen,
            },
            view: RootView::Workspace,
            toolbar: None,
            pending: None,
            screens: Screens {
                home,
                library: library_view,
                artist: None,
                artist_detail: None,
                album,
                album_detail,
                song,
                song_detail,
                playlist,
                playlist_detail,
                search,
                settings,
            },
            _adaptive: adaptive,
        };
        root.show(start, cx);
        root
    }

    fn artist(&mut self, cx: &mut Context<Self>) -> (Entity<ArtistView>, Entity<ArtistDetail>) {
        if let (Some(view), Some(detail)) = (&self.screens.artist, &self.screens.artist_detail) {
            return (view.clone(), detail.clone());
        }

        let detail = cx.new(|cx| ArtistDetail::new(self.session.clone(), self.io.clone(), cx));
        let view =
            cx.new(|cx| ArtistView::new(detail.clone(), self.playback.clone(), ARTIST_COLUMNS, cx));
        self.screens.artist = Some(view.clone());
        self.screens.artist_detail = Some(detail.clone());
        (view, detail)
    }

    fn open_search(&mut self, cx: &mut Context<Self>) {
        navigate(Destination::Search, cx);
        self.pending = Some(Focus::Search);
        cx.notify();
    }

    fn open_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(toolbar) = self.toolbar.clone() else {
            return;
        };
        toolbar.update(cx, |toolbar, cx| toolbar.focus(window, cx));
    }

    fn toggle_fullscreen(&mut self, cx: &mut Context<Self>) {
        let entering = matches!(self.view, RootView::Workspace);
        self.view = match entering {
            true => RootView::Fullscreen,
            false => RootView::Workspace,
        };
        self.pending = Some(match entering {
            true => Focus::Fullscreen,
            false => Focus::Workspace,
        });
        cx.notify();
    }

    fn options(&self, cx: &Context<Self>) -> TitleBarOptions {
        let content = self.toolbar.clone().map(Into::into);

        match self.view {
            RootView::Workspace => self.shells.workspace.read(cx).title_bar(content, cx),
            RootView::Fullscreen => self.shells.fullscreen.read(cx).title_bar(content, cx),
        }
    }

    fn open_settings(&mut self, cx: &mut Context<Self>) {
        navigate(Destination::Settings, cx);
        self.pending = Some(Focus::Workspace);
        cx.notify();
    }

    fn show(&mut self, destination: Destination, cx: &mut Context<Self>) {
        self.pending = Some(match destination {
            Destination::Search => Focus::Search,
            _ => Focus::Workspace,
        });

        let mut toolbar = None;

        let content: AnyView = match destination {
            Destination::Home => self.screens.home.clone().into(),
            Destination::Library(tab) => {
                self.screens
                    .library
                    .update(cx, |library, cx| library.select(tab.into(), cx));
                let library = self.screens.library.clone();
                toolbar = Some(library.read(cx).toolbar());
                library.into()
            }
            Destination::Album(id) => {
                self.screens
                    .album_detail
                    .update(cx, |detail, cx| detail.open_album(&id, cx));
                let album = self.screens.album.clone();
                toolbar = Some(album.read(cx).toolbar());
                album.into()
            }
            Destination::Song(id) => {
                self.screens
                    .song_detail
                    .update(cx, |detail, cx| detail.open(&id, cx));
                self.screens.song.clone().into()
            }
            Destination::Playlist(id) => {
                self.screens
                    .playlist_detail
                    .update(cx, |detail, cx| detail.open_playlist(&id, cx));
                let playlist = self.screens.playlist.clone();
                toolbar = Some(playlist.read(cx).toolbar());
                playlist.into()
            }
            Destination::Artist(id) => {
                let (artist, detail) = self.artist(cx);
                detail.update(cx, |artist, cx| artist.open(&id, cx));
                artist.into()
            }
            Destination::Search => self.screens.search.clone().into(),
            Destination::Settings => self.screens.settings.clone().into(),
        };

        self.toolbar = toolbar;

        self.shells
            .workspace
            .update(cx, |workspace, cx| workspace.set_content(content, cx));
        cx.notify();
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let show_sign_in = match self.session.read(cx).state() {
            SessionState::SignedOut | SessionState::Failed(_) | SessionState::Authorizing => true,
            SessionState::Restoring | SessionState::SignedIn(_) => false,
        };

        match self.pending.take() {
            Some(Focus::Search) => self
                .screens
                .search
                .update(cx, |search, cx| search.focus(window, cx)),
            Some(Focus::Workspace) => self
                .shells
                .workspace
                .update(cx, |workspace, cx| workspace.focus(window, cx)),
            Some(Focus::Fullscreen) => self
                .shells
                .fullscreen
                .update(cx, |fullscreen, cx| fullscreen.focus(window, cx)),
            None => {}
        }

        let options = self.options(cx);
        self.title_bar
            .update(cx, |bar, cx| bar.set_options(options, cx));

        let theme = *cx.theme();
        window.set_rem_size(theme.font_size);

        div()
            .flex()
            .font(font("Inter"))
            .flex_col()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                |_, _, cx| back(cx),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                |_, _, cx| forward(cx),
            )
            .on_action(cx.listener(|this, _: &OpenFilter, window, cx| this.open_filter(window, cx)))
            .on_action(cx.listener(|this, _: &OpenSearch, _, cx| this.open_search(cx)))
            .on_action(cx.listener(|this, _: &OpenSettings, _, cx| this.open_settings(cx)))
            .on_action(cx.listener(|this, _: &ToggleFullscreen, _, cx| this.toggle_fullscreen(cx)))
            .when_else(
                show_sign_in,
                |this| this.child(self.login.clone()),
                |this| {
                    this.child(self.title_bar.clone()).child(match self.view {
                        RootView::Workspace => self.shells.workspace.clone().into_any_element(),
                        RootView::Fullscreen => self.shells.fullscreen.clone().into_any_element(),
                    })
                },
            )
    }
}
