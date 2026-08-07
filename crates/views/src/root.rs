// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use gpui::{AnyView, Context, Entity, MouseButton, NavigationDirection, Render};
use gpui::{Window, div};
use gpui::{font, prelude::*};
use input::{OpenFilter, OpenSearch, OpenSettings};
use router::{Destination, NavigationEvent, back, forward, navigate};
use state::{
    ArtistDetail, Detail, Home, Io, Library, Playback, Queue, Search, Session, SessionState,
    SongDetail,
};
use ui::ActiveTheme as _;
use workspace::{Filter, Sidebar, Workspace};

use crate::search::SearchView;
use crate::tracks::{ALBUM_COLUMNS, LIBRARY_COLUMNS};
use crate::{
    Adaptive, ArtistView, ColumnPicker, DetailView, HomeView, LibraryView, LoginView, SettingsView,
    SongView,
};

struct Screens {
    home: Entity<HomeView>,
    library: Entity<LibraryView>,
    picker: Entity<ColumnPicker>,
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

enum Focus {
    Search,
    Workspace,
}

pub struct Root {
    session: Entity<Session>,
    playback: Entity<Playback>,
    io: Io,
    login: Entity<LoginView>,
    workspace: Entity<Workspace>,
    filter: Entity<Filter>,
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
        let sidebar = cx.new(Sidebar::new);

        let navigation = router::trail(cx);

        cx.subscribe(&navigation, |this, _, event, cx| {
            let NavigationEvent::Moved(destination) = event;
            this.show(destination.clone(), cx);
        })
        .detach();

        let library_view =
            cx.new(|cx| LibraryView::new(library.clone(), playback.clone(), window, cx));

        let picker = cx.new(|cx| ColumnPicker::new(library_view.clone(), cx));

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
                window,
                cx,
            )
        });

        let queries = cx.new(|cx| Search::new(session.clone(), search_library, io.clone(), cx));
        let search = cx.new(|cx| SearchView::new(queries, playback.clone(), cx));

        let settings = cx.new(|cx| SettingsView::new(session.clone(), playback.clone(), cx));

        let song_detail = cx.new(|cx| SongDetail::new(session.clone(), io.clone(), cx));
        let song = cx.new(|cx| SongView::new(song_detail.clone(), playback.clone(), cx));

        let filter = cx.new(Filter::new);
        let start = navigation.read(cx).current();
        let workspace = cx.new(|cx| {
            Workspace::new(
                sidebar.clone(),
                playback.clone(),
                queue,
                library_view.clone().into(),
                cx,
            )
        });

        let adaptive = cx.new(|cx| Adaptive::new(playback.clone(), cx));

        let mut root = Self {
            session,
            playback,
            io,
            login,
            workspace,
            filter,
            pending: None,
            screens: Screens {
                home,
                library: library_view,
                picker,
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
        let view = cx
            .new(|cx| ArtistView::new(detail.clone(), self.playback.clone(), LIBRARY_COLUMNS, cx));
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
        self.filter
            .update(cx, |filter, cx| filter.focus(window, cx));
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

        let searchable = matches!(
            destination,
            Destination::Library(_)
                | Destination::Artist(_)
                | Destination::Album(_)
                | Destination::Playlist(_)
        );

        let listing = matches!(destination, Destination::Library(_));

        let content: AnyView = match destination {
            Destination::Home => self.screens.home.clone().into(),
            Destination::Library(tab) => {
                self.screens
                    .library
                    .update(cx, |library, cx| library.select(tab.into(), cx));
                let library = self.screens.library.clone();
                let picker = self.screens.picker.clone().into();
                self.filter.update(cx, |filter, cx| {
                    filter.bind(&library, cx);
                    filter.set_actions(Some(picker), cx);
                });
                library.into()
            }
            Destination::Album(id) => {
                self.screens
                    .album_detail
                    .update(cx, |detail, cx| detail.open_album(&id, cx));
                let album = self.screens.album.clone();
                self.filter.update(cx, |filter, cx| filter.bind(&album, cx));
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
                self.filter
                    .update(cx, |filter, cx| filter.bind(&playlist, cx));
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

        if !listing {
            self.filter
                .update(cx, |filter, cx| filter.set_actions(None, cx));
        }

        if !searchable {
            self.filter.update(cx, |filter, cx| filter.release(cx));
        }

        let toolbar = searchable.then(|| self.filter.clone().into());

        self.workspace.update(cx, |workspace, cx| {
            workspace.set_content(content, cx);
            workspace.set_toolbar(toolbar, cx);
        });
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
                .workspace
                .update(cx, |workspace, cx| workspace.focus(window, cx)),
            None => {}
        }

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
            .when_else(
                show_sign_in,
                |this| this.child(self.login.clone()),
                |this| this.child(self.workspace.clone()),
            )
    }
}
