mod albums;
mod playlists;

use gpui::prelude::*;
use gpui::{App, Context, Entity, Pixels, Point, Render, ScrollHandle, SharedString, Window, px};
use router::{Destination, LibraryTab, navigate};
use spotify::Track;
use state::{AppSettings, Library, LibraryState, Playback, Sonora};
use ui::{
    GridDelegate, GridEvent, GridState, Scrollbar, Scroller, Sort, Toggle, Viewport, grid, scrolled,
};
use workspace::{Chrome, Searchable};

use crate::cells;
use crate::tracks::{
    self, LIBRARY_COLUMNS, PlaybackStatus, TrackField, TrackSource, Tracks, playback_status,
};
use albums::AlbumSource;
use playlists::PlaylistSource;

impl From<LibraryTab> for Section {
    fn from(tab: LibraryTab) -> Self {
        match tab {
            LibraryTab::Songs => Section::Tracks,
            LibraryTab::Albums => Section::Albums,
            LibraryTab::Playlists => Section::Playlists,
        }
    }
}

impl From<Section> for LibraryTab {
    fn from(section: Section) -> Self {
        match section {
            Section::Tracks => LibraryTab::Songs,
            Section::Albums => LibraryTab::Albums,
            Section::Playlists => LibraryTab::Playlists,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Tracks,
    Albums,
    Playlists,
}

const PINNED: [&str; 3] = ["cover", "title", "name"];

impl Section {
    fn key(self) -> &'static str {
        match self {
            Section::Tracks => "songs",
            Section::Albums => "albums",
            Section::Playlists => "playlists",
        }
    }
}

struct LibraryTracks(Entity<Library>);

impl Tracks for LibraryTracks {
    fn tracks<'a>(&self, cx: &'a App) -> &'a [Track] {
        match self.0.read(cx).state() {
            LibraryState::Ready { tracks, .. } => tracks.as_slice(),
            _ => &[],
        }
    }

    fn is_loading(&self, cx: &App) -> bool {
        self.0.read(cx).is_loading()
    }
}

pub struct LibraryView {
    library: Entity<Library>,
    settings: Entity<AppSettings>,
    playback: Entity<Playback>,
    playback_status: PlaybackStatus,
    section: Section,
    width: Pixels,
    scrollbar: Entity<Scrollbar>,
    tracks: Entity<GridState<TrackSource>>,
    albums: Entity<GridState<AlbumSource>>,
    playlists: Entity<GridState<PlaylistSource>>,
}

impl LibraryView {
    pub fn new(
        library: Entity<Library>,
        playback: Entity<Playback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let width = cells::content_width(window, Pixels::ZERO, cx);
        let settings = Sonora::global(cx).settings.clone();
        let saved = |section: Section, cx: &App| settings.read(cx).hidden_columns(section.key());

        let scrollbar = cx.new(|_| Scrollbar::new(ScrollHandle::new()));
        let scroll = scrollbar.read(cx).scroll().clone();

        let tracks = cx.new(|cx| {
            let playlist_scrollbar = cx.new(|_| {
                Scrollbar::new(ScrollHandle::new())
                    .always_visible()
                    .track_inset(px(4.))
            });
            let source = TrackSource::new(
                LIBRARY_COLUMNS,
                LibraryTracks(library.clone()),
                playback.clone(),
                playlist_scrollbar,
            );
            let source = source.table(cx.weak_entity());
            let mut delegate = GridDelegate::new(source, width, cx).with_sort(
                TrackField::AddedAt,
                Sort::Descending,
                cx,
            );
            delegate.set_hidden(saved(Section::Tracks, cx), cx);
            GridState::new(delegate, cx).follow(scroll.clone())
        });
        let albums = cx.new(|cx| {
            let source = AlbumSource::new(library.clone(), playback.clone());
            let mut delegate = GridDelegate::new(source, width, cx);
            delegate.set_hidden(saved(Section::Albums, cx), cx);
            GridState::new(delegate, cx).follow(scroll.clone())
        });
        let playlists = cx.new(|cx| {
            let source = PlaylistSource::new(library.clone(), playback.clone());
            let mut delegate = GridDelegate::new(source, width, cx);
            delegate.set_hidden(saved(Section::Playlists, cx), cx);
            GridState::new(delegate, cx).follow(scroll)
        });

        cx.observe(&library, |this, _, cx| {
            this.rebuild(cx);
            cx.notify();
        })
        .detach();

        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();

        let current_playback = playback_status(&playback, cx);
        cx.observe(&playback, |this, playback, cx| {
            let current = playback_status(&playback, cx);
            if this.playback_status == current {
                return;
            }
            this.playback_status = current;
            this.tracks.update(cx, |table, cx| table.refresh(cx));
            this.albums.update(cx, |table, cx| table.refresh(cx));
            this.playlists.update(cx, |table, cx| table.refresh(cx));
        })
        .detach();

        cx.subscribe(&tracks, |this, _, event, cx| {
            let GridEvent::DoubleClicked(display) = event;
            this.play(*display, cx);
        })
        .detach();

        cx.subscribe(&albums, |this, _, event, cx| {
            let GridEvent::DoubleClicked(display) = event;
            this.open_album(*display, cx);
        })
        .detach();

        cx.subscribe(&playlists, |this, _, event, cx| {
            let GridEvent::DoubleClicked(display) = event;
            this.open_playlist(*display, cx);
        })
        .detach();

        Self {
            library,
            settings,
            playback,
            playback_status: current_playback,
            section: Section::Tracks,
            width,
            scrollbar,
            tracks,
            albums,
            playlists,
        }
    }

    pub fn section(&self) -> Section {
        self.section
    }

    pub fn toggles(&self, cx: &App) -> Vec<Toggle> {
        let all = match self.section {
            Section::Tracks => self.tracks.read(cx).delegate().toggles(),
            Section::Albums => self.albums.read(cx).delegate().toggles(),
            Section::Playlists => self.playlists.read(cx).delegate().toggles(),
        };

        all.into_iter()
            .filter(|toggle| !PINNED.contains(&toggle.key))
            .collect()
    }

    pub fn toggle_column(&mut self, key: &str, cx: &mut Context<Self>) {
        if PINNED.contains(&key) {
            return;
        }

        let mut hidden = self.hidden(cx);
        match hidden.iter().position(|hidden| hidden == key) {
            Some(at) => {
                hidden.remove(at);
            }
            None => hidden.push(key.to_owned()),
        }

        self.settings.update(cx, |settings, cx| {
            settings.set_hidden_columns(self.section.key(), hidden.clone(), cx);
        });
        self.apply_hidden(hidden, cx);
        cx.notify();
    }

    fn hidden(&self, cx: &App) -> Vec<String> {
        match self.section {
            Section::Tracks => self.tracks.read(cx).delegate().hidden().to_vec(),
            Section::Albums => self.albums.read(cx).delegate().hidden().to_vec(),
            Section::Playlists => self.playlists.read(cx).delegate().hidden().to_vec(),
        }
    }

    fn apply_hidden(&mut self, hidden: Vec<String>, cx: &mut Context<Self>) {
        match self.section {
            Section::Tracks => self.tracks.update(cx, |table, cx| {
                table.delegate_mut().set_hidden(hidden, cx);
                table.refresh(cx);
            }),
            Section::Albums => self.albums.update(cx, |table, cx| {
                table.delegate_mut().set_hidden(hidden, cx);
                table.refresh(cx);
            }),
            Section::Playlists => self.playlists.update(cx, |table, cx| {
                table.delegate_mut().set_hidden(hidden, cx);
                table.refresh(cx);
            }),
        }
    }

    pub fn is_loading(&self, cx: &App) -> bool {
        self.library.read(cx).is_loading()
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.library.update(cx, |library, cx| library.refresh(cx));
    }

    pub fn select(&mut self, section: Section, cx: &mut Context<Self>) {
        if self.section != section {
            self.scrollbar
                .read(cx)
                .scroll()
                .set_offset(Point::default());
        }
        self.section = section;
        cx.notify();
    }

    fn viewport(scroll: &ScrollHandle, window: &Window) -> Viewport {
        let visible = scroll.bounds().size.height;

        Viewport {
            top: scrolled(scroll),
            height: match visible > Pixels::ZERO {
                true => visible,
                false => window.viewport_size().height,
            },
        }
    }

    fn play(&mut self, display: usize, cx: &mut Context<Self>) {
        let queued = tracks::ordered(&self.tracks, cx);
        self.playback
            .update(cx, |playback, cx| playback.start(queued, display, cx));
    }

    fn open_album(&mut self, display: usize, cx: &mut Context<Self>) {
        let album = {
            let state = self.albums.read(cx);
            let row = state.delegate().row(display);
            state.delegate().source().at(row, cx)
        };
        let Some(album) = album else {
            return;
        };
        navigate(Destination::Album(album.id.into()), cx);
    }

    fn open_playlist(&mut self, display: usize, cx: &mut Context<Self>) {
        let playlist = {
            let state = self.playlists.read(cx);
            let row = state.delegate().row(display);
            state.delegate().source().at(row, cx)
        };
        let Some(playlist) = playlist else {
            return;
        };
        navigate(Destination::Playlist(playlist.id.into()), cx);
    }

    fn resize(&mut self, window: &Window, cx: &mut Context<Self>) {
        let width = cells::content_width(window, Pixels::ZERO, cx);
        if (width - self.width).abs() < px(0.5) {
            return;
        }
        self.width = width;

        self.tracks.update(cx, |table, cx| {
            table.delegate_mut().set_width(width, cx);
            table.refresh(cx);
        });
        self.albums.update(cx, |table, cx| {
            table.delegate_mut().set_width(width, cx);
            table.refresh(cx);
        });
        self.playlists.update(cx, |table, cx| {
            table.delegate_mut().set_width(width, cx);
            table.refresh(cx);
        });
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.tracks.update(cx, |table, cx| {
            table.rebuild(cx);
        });
        self.albums.update(cx, |table, cx| {
            table.rebuild(cx);
        });
        self.playlists.update(cx, |table, cx| {
            table.rebuild(cx);
        });
    }
}

impl Render for LibraryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.resize(window, cx);

        let scroll = self.scrollbar.read(cx).scroll().clone();
        let viewport = Self::viewport(&scroll, window);

        let table = match self.section {
            Section::Tracks => {
                self.tracks
                    .update(cx, |table, _| table.set_viewport(viewport));
                grid(&self.tracks).into_any_element()
            }
            Section::Albums => {
                self.albums
                    .update(cx, |table, _| table.set_viewport(viewport));
                grid(&self.albums).into_any_element()
            }
            Section::Playlists => {
                self.playlists
                    .update(cx, |table, _| table.set_viewport(viewport));
                grid(&self.playlists).into_any_element()
            }
        };

        Scroller::new("library-page", &self.scrollbar).child(table)
    }
}

impl Searchable for LibraryView {
    fn search(&mut self, query: &str, cx: &mut Context<Self>) {
        self.tracks.update(cx, |table, cx| {
            table.delegate_mut().set_filter(query, cx);
            table.refresh(cx);
        });
        self.albums.update(cx, |table, cx| {
            table.delegate_mut().set_filter(query, cx);
            table.refresh(cx);
        });
        self.playlists.update(cx, |table, cx| {
            table.delegate_mut().set_filter(query, cx);
            table.refresh(cx);
        });
        cx.notify();
    }

    fn hint() -> SharedString {
        "filter-library".into()
    }
}
