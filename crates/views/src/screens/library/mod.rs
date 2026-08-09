// SPDX-License-Identifier: GPL-3.0-or-later

mod albums;
mod playlists;

use std::rc::Rc;

use crate::chrome::tools::{self, Sift, Sliders};
use crate::chrome::{Chrome, Searchable, Toolbar, Tooled};
use crate::shared::menu::{album_menu, playlist_menu};
use crate::shared::playlist_editor::{Edit, PlaylistEditor};

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, FontWeight, ListAlignment, ListState, MouseButton, Pixels,
    Point, Render, ScrollHandle, SharedString, WeakEntity, Window, div, list, px,
};
use i18n::t;
use router::{Destination, LibraryTab, navigate};
use spotify::{Album, Playlist, Track};
use state::{AppSettings, Library, LibraryState, Origin, Playback, PlaybackState, Sonora};
use ui::{
    ActiveTheme as _, Button, Card, FlagAxis, GridDelegate, GridEvent, GridSource, GridState, Menu,
    MenuItem, Mode, Popovers, Popup, RangeAxis, Scrollbar, Scroller, Sort, SortAxis, Text, Toggle,
    Unit, Viewport, clock, grid, heading, scrolled,
};

use crate::shared::hero::{HeroMetaStrip, HeroPlayButton, PageHero};
use crate::shared::release_card::ReleaseCard;
use crate::shared::tracks::{
    self, LIBRARY_COLUMNS, PlaybackStatus, TrackField, TrackSieve, TrackSource, Tracks,
    playback_status,
};
use crate::shared::{cells, page};
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
const CARD_MIN: Pixels = px(130.);
const CARD_MAX: Pixels = px(190.);
const CARD_GAP: Pixels = px(32.);

fn tiling(available: Pixels) -> (Pixels, Pixels) {
    let columns = card_columns(available) as f32;
    let spread = available - CARD_GAP * (columns - 1.);
    let card = (spread / columns).min(CARD_MAX).floor();
    let gap = match columns > 1. {
        true => ((available - card * columns) / (columns - 1.)).floor(),
        false => Pixels::ZERO,
    };
    (card, gap)
}

fn card_columns(available: Pixels) -> usize {
    (((available + CARD_GAP) / (CARD_MIN + CARD_GAP))
        .floor()
        .max(1.)) as usize
}

#[derive(Clone)]
enum LibraryMenu {
    Background,
    Album(Album),
    Playlist(Playlist),
}

#[derive(Clone)]
enum DeckRow {
    Heading(usize),
    Cards(Vec<(usize, usize)>),
}

impl Section {
    const ALL: [Self; 3] = [Self::Tracks, Self::Albums, Self::Playlists];

    fn key(self) -> &'static str {
        match self {
            Section::Tracks => "songs",
            Section::Albums => "albums",
            Section::Playlists => "playlists",
        }
    }

    fn slot(self) -> usize {
        match self {
            Section::Tracks => 0,
            Section::Albums => 1,
            Section::Playlists => 2,
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
    views: [Mode; 3],
    width: Pixels,
    card_width: Pixels,
    card_columns: usize,
    card_rows: Rc<[DeckRow]>,
    cards_dirty: bool,
    card_list: ListState,
    card_scrollbar: Entity<Scrollbar>,
    scrollbar: Entity<Scrollbar>,
    tracks: Entity<GridState<TrackSource>>,
    albums: Entity<GridState<AlbumSource>>,
    playlists: Entity<GridState<PlaylistSource>>,
    context_menu: Option<(LibraryMenu, Point<Pixels>)>,
    toolbar: Entity<Toolbar>,
    popovers: Popovers,
    sliders: [Sliders; 3],
    me: WeakEntity<Self>,
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
        let stored = |section: Section, cx: &App| {
            let settings = settings.read(cx);
            (
                settings.table(section.key()),
                settings.sorting(section.key()),
            )
        };
        let viewed = |section: Section, cx: &App| settings.read(cx).view(section.key());
        let views = Section::ALL.map(|section| viewed(section, cx));

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
            let (layout, sorting) = stored(Section::Tracks, cx);
            delegate.set_layout(layout, cx);
            if let Some(sorting) = sorting {
                delegate.set_sorting(sorting, cx);
            }
            GridState::new(delegate, cx).follow(scroll.clone())
        });
        let albums = cx.new(|cx| {
            let source = AlbumSource::new(library.clone(), playback.clone());
            let mut delegate = GridDelegate::new(source, width, cx);
            let (layout, sorting) = stored(Section::Albums, cx);
            delegate.set_layout(layout, cx);
            delegate.set_sorting(sorting.flatten(), cx);
            GridState::new(delegate, cx).follow(scroll.clone())
        });
        let playlists = cx.new(|cx| {
            let source = PlaylistSource::new(library.clone(), playback.clone());
            let mut delegate = GridDelegate::new(source, width, cx);
            let (layout, sorting) = stored(Section::Playlists, cx);
            delegate.set_layout(layout, cx);
            delegate.set_sorting(sorting.flatten(), cx);
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
            for table in this.tables() {
                table.refresh(cx);
            }
        })
        .detach();

        cx.subscribe(&tracks, |this, _, event, cx| match event {
            GridEvent::DoubleClicked(display) => this.play(*display, cx),
            _ => this.persist(Section::Tracks, cx),
        })
        .detach();

        cx.subscribe(&albums, |this, _, event, cx| match event {
            GridEvent::DoubleClicked(display) => this.open_album(*display, cx),
            _ => {
                this.cards_dirty = true;
                this.persist(Section::Albums, cx);
            }
        })
        .detach();

        cx.subscribe(&playlists, |this, _, event, cx| match event {
            GridEvent::DoubleClicked(display) => this.open_playlist(*display, cx),
            _ => {
                this.cards_dirty = true;
                this.persist(Section::Playlists, cx);
            }
        })
        .detach();

        let me = cx.entity();
        let toolbar = cx.new(|cx| {
            let mut toolbar = Toolbar::new(cx);
            toolbar.bind(&me, cx);
            toolbar.wire(&me, cx);
            toolbar
        });

        let card_list = ListState::new(0, ListAlignment::Top, CARD_MAX * 2.);
        let card_scrollbar = cx.new(|_| Scrollbar::list(card_list.clone()));

        Self {
            library,
            settings,
            playback,
            playback_status: current_playback,
            section: Section::Tracks,
            views,
            width,
            card_width: Pixels::ZERO,
            card_columns: 0,
            card_rows: Rc::from([]),
            cards_dirty: true,
            card_list,
            card_scrollbar,
            scrollbar,
            tracks,
            albums,
            playlists,
            context_menu: None,
            toolbar,
            popovers: Popovers::default(),
            sliders: Section::ALL.map(|_| Sliders::default()),
            me: me.downgrade(),
        }
    }

    fn create_playlist(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;
        PlaylistEditor::open(Edit::Create(None), window, cx);
        cx.notify();
    }

    pub fn section(&self) -> Section {
        self.section
    }

    fn table(&self, section: Section) -> &dyn ui::Table {
        match section {
            Section::Tracks => &self.tracks,
            Section::Albums => &self.albums,
            Section::Playlists => &self.playlists,
        }
    }

    fn tables(&self) -> [&dyn ui::Table; 3] {
        [&self.tracks, &self.albums, &self.playlists]
    }

    fn column_toggles(&self, cx: &App) -> Vec<Toggle> {
        self.table(self.section)
            .toggles(cx)
            .into_iter()
            .filter(|toggle| !PINNED.contains(&toggle.key))
            .collect()
    }

    fn switch_column(&mut self, key: &str, cx: &mut Context<Self>) {
        if PINNED.contains(&key) {
            return;
        }

        let mut layout = self.table(self.section).layout(cx);
        layout.toggle(key);
        self.table(self.section).set_layout(layout, cx);
        self.persist(self.section, cx);
        cx.notify();
    }

    fn persist(&mut self, section: Section, cx: &mut Context<Self>) {
        page::store(
            &self.settings.clone(),
            self.table(section),
            section.key(),
            section.key(),
            cx,
        );
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
            self.cards_dirty = true;
        }
        self.section = section;
        if self.mode() == Mode::List {
            self.table(section).set_width(self.width, cx);
        }
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

    fn liked(&self, cx: &App) -> Vec<Track> {
        match self.library.read(cx).state() {
            LibraryState::Ready { tracks, .. } => tracks.clone(),
            _ => Vec::new(),
        }
    }

    fn liked_header(&self, cx: &Context<Self>) -> AnyElement {
        let queued = self.liked(cx);
        let duration: std::time::Duration = queued.iter().map(|track| track.duration).sum();
        let mut strip = HeroMetaStrip::new().text(t!("count-songs", count = queued.len()));
        if !duration.is_zero() {
            strip = strip.text(clock(duration));
        }

        PageHero::new("liked-songs-hero", t!("library-liked-songs"))
            .fallback("icons/heart-filled.svg")
            .eyebrow(t!("detail-playlist"))
            .meta(strip)
            .actions(HeroPlayButton::new(
                "play-liked-songs",
                t!("library-play-liked-songs"),
                queued,
                self.playback.clone(),
            ))
            .into_any_element()
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

        if self.mode() == Mode::List {
            self.table(self.section).set_width(width, cx);
        }
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.cards_dirty = true;
        for table in self.tables() {
            table.rebuild(cx);
        }
    }

    fn cards(&mut self, window: &Window, cx: &App) -> AnyElement {
        let theme = *cx.theme();
        let inset = theme.metrics.inset;
        let room = cells::content_width(window, page::reserved(inset), cx);
        let room = room.max(CARD_MIN);
        let columns = card_columns(room);
        let (card, gap) = tiling(room);

        if self.card_columns != columns {
            self.card_columns = columns;
            self.cards_dirty = true;
        }
        if self.cards_dirty {
            let rows = match self.section {
                Section::Tracks => deck(&self.tracks, columns, cx),
                Section::Albums => deck(&self.albums, columns, cx),
                Section::Playlists => deck(&self.playlists, columns, cx),
            };
            self.card_list.reset(rows.len());
            self.card_rows = rows.into();
            self.cards_dirty = false;
        }
        if (self.card_width - card).abs() >= px(0.5) {
            self.card_width = card;
            self.card_list.remeasure();
        }

        let rows = self.card_rows.clone();
        let list_state = self.card_list.clone();
        let section = self.section;
        let view = self.me.clone();

        div()
            .relative()
            .size_full()
            .child(
                list(list_state, move |index, _, cx| {
                    let Some(row) = rows.get(index) else {
                        return div().into_any_element();
                    };
                    let Some(view) = view.upgrade() else {
                        return div().into_any_element();
                    };
                    let view = view.read(cx);
                    let separated = index + 1 < rows.len();

                    match row {
                        DeckRow::Heading(display) => {
                            let label = match section {
                                Section::Tracks => {
                                    view.tracks.read(cx).delegate().group(*display, cx)
                                }
                                Section::Albums => {
                                    view.albums.read(cx).delegate().group(*display, cx)
                                }
                                Section::Playlists => {
                                    view.playlists.read(cx).delegate().group(*display, cx)
                                }
                            };

                            div()
                                .when(separated, |this| this.pb_6())
                                .children(label.map(|label| head(label, cx)))
                                .into_any_element()
                        }
                        DeckRow::Cards(cards) => {
                            let cards = cards.iter().filter_map(|&(display, row)| match section {
                                Section::Tracks => view.track_card(display, row, card, cx),
                                Section::Albums => view.album_card(display, row, card, cx),
                                Section::Playlists => view.playlist_card(display, row, card, cx),
                            });

                            div()
                                .flex()
                                .gap_x(gap)
                                .when(separated, |this| this.pb_6())
                                .children(cards)
                                .into_any_element()
                        }
                    }
                })
                .size_full()
                .p(inset),
            )
            .child(self.card_scrollbar.clone())
            .into_any_element()
    }

    fn track_card(&self, display: usize, row: usize, card: Pixels, cx: &App) -> Option<AnyElement> {
        let theme = *cx.theme();
        let track = self.tracks.read(cx).delegate().source().at(row, cx)?;
        let playable = track.playable;
        let pressed = (self.tracks.clone(), self.playback.clone());
        let played = pressed.clone();
        let current = self.playback.read(cx);
        let state = current
            .track()
            .filter(|playing| playing.id.is_some() && playing.id == track.id)
            .map(|_| current.state().clone());
        let playing = matches!(state, Some(PlaybackState::Playing));
        let artists = cells::artist_links(
            SharedString::from(format!("library-track-artist-{display}")),
            track.artist_refs.clone(),
            track.artists.clone(),
            theme.muted_foreground,
        )
        .text_size(theme.text(Text::Small))
        .truncate();

        Some(
            Card::new(("library-track", display), SharedString::from(track.name))
                .tile(card)
                .cover(track.cover)
                .weight(FontWeight::SEMIBOLD)
                .flat()
                .underline()
                .when(track.explicit, Card::explicit)
                .bare_meta(artists)
                .when(playable, move |card| {
                    card.play(playing, move |_, _, cx| match &state {
                        Some(PlaybackState::Playing) => {
                            played.1.update(cx, |playback, cx| playback.pause(cx))
                        }
                        Some(PlaybackState::Paused) => {
                            played.1.update(cx, |playback, cx| playback.resume(cx))
                        }
                        _ => page::play(&played.0, &played.1, display, cx),
                    })
                    .press(move |_, _, cx| page::play(&pressed.0, &pressed.1, display, cx))
                })
                .into_any_element(),
        )
    }

    fn album_card(&self, display: usize, row: usize, card: Pixels, cx: &App) -> Option<AnyElement> {
        let album = self.albums.read(cx).delegate().source().at(row, cx)?;
        let context = album.clone();
        let view = self.me.clone();

        Some(
            div()
                .id(("library-album", display))
                .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    let Some(view) = view.upgrade() else {
                        return;
                    };
                    view.update(cx, |this, cx| {
                        this.context_menu =
                            Some((LibraryMenu::Album(context.clone()), event.position));
                        cx.notify();
                    });
                })
                .child(ReleaseCard::new(display, album, self.playback.clone()).width(card))
                .into_any_element(),
        )
    }

    fn playlist_card(
        &self,
        display: usize,
        row: usize,
        card: Pixels,
        cx: &App,
    ) -> Option<AnyElement> {
        let playlist = self.playlists.read(cx).delegate().source().at(row, cx)?;
        let playback = self.playback.clone();
        let origin = Origin::Playlist(playlist.id.clone());
        let state = self.playback.read(cx).playing_from(&origin);
        let playing = matches!(state, Some(PlaybackState::Playing));
        let played = playlist.id.clone();
        let opened = SharedString::from(playlist.id.clone());
        let context = playlist.clone();
        let view = self.me.clone();

        Some(
            Card::new(
                ("library-playlist", display),
                SharedString::from(playlist.name),
            )
            .tile(card)
            .cover(playlist.cover)
            .weight(FontWeight::SEMIBOLD)
            .flat()
            .underline()
            .meta(SharedString::from(playlist.owner))
            .play(playing, move |_, _, cx| {
                playback.update(cx, |playback, cx| match &state {
                    Some(PlaybackState::Playing) => playback.pause(cx),
                    Some(PlaybackState::Paused) => playback.resume(cx),
                    _ => playback.play_playlist(&played, cx),
                });
            })
            .press(move |_, _, cx| navigate(Destination::Playlist(opened.clone()), cx))
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                let Some(view) = view.upgrade() else {
                    return;
                };
                view.update(cx, |this, cx| {
                    this.context_menu =
                        Some((LibraryMenu::Playlist(context.clone()), event.position));
                    cx.notify();
                });
            })
            .into_any_element(),
        )
    }
}

impl Render for LibraryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.resize(window, cx);

        let theme = *cx.theme();
        let inset = theme.metrics.inset;
        let mode = self.mode();
        if mode == Mode::List {
            let scroll = self.scrollbar.read(cx).scroll().clone();
            let viewport = match self.section {
                Section::Tracks => page::viewport(&scroll, inset, window),
                _ => Self::viewport(&scroll, window),
            };
            self.table(self.section).set_viewport(viewport, cx);
        }

        let context_menu = self.context_menu.clone().map(|(target, position)| {
            let menu = match target {
                LibraryMenu::Album(album) => album_menu(album.id, self.playback.clone(), false),
                LibraryMenu::Playlist(playlist) => {
                    playlist_menu(playlist, self.playback.clone(), false)
                }
                LibraryMenu::Background => Menu::new("playlist-background-menu").item(
                    MenuItem::new("create-playlist", t!("menu-new-playlist"))
                        .icon("icons/plus.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.create_playlist(window, cx);
                        })),
                ),
            };
            Popup::new(position, menu).on_close(cx.listener(|this, _, _, cx| {
                this.context_menu = None;
                cx.notify();
            }))
        });
        let view = cx.entity().downgrade();
        let section = self.section;
        let content = match (self.section, mode) {
            (Section::Tracks, Mode::List) => Scroller::new("library-page", &self.scrollbar)
                .pt(inset)
                .pb(inset)
                .child(div().px(inset).child(self.liked_header(cx)))
                .child(grid(&self.tracks))
                .into_any_element(),
            (_, Mode::List) => Scroller::new("library-page", &self.scrollbar)
                .child(self.table(self.section).element())
                .into_any_element(),
            (_, Mode::Cards) => self.cards(window, cx),
        };

        div()
            .relative()
            .size_full()
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                if section != Section::Playlists {
                    return;
                }
                window.prevent_default();
                let Some(view) = view.upgrade() else {
                    return;
                };
                view.update(cx, |this, cx| {
                    this.context_menu = Some((LibraryMenu::Background, event.position));
                    cx.notify();
                });
            })
            .child(content)
            .when_some(context_menu, |this, menu| this.child(menu))
    }
}

impl Searchable for LibraryView {
    fn search(&mut self, query: &str, cx: &mut Context<Self>) {
        self.cards_dirty = true;
        for table in self.tables() {
            table.set_filter(query, cx);
        }
        cx.notify();
    }

    fn hint() -> SharedString {
        "filter-library".into()
    }
}

impl LibraryView {
    fn sorts(&self, cx: &App) -> Vec<SortAxis> {
        self.table(self.section).sortables(cx)
    }

    fn set_sort(&mut self, key: &'static str, cx: &mut Context<Self>) {
        self.table(self.section).cycle_sort(key, cx);
        self.cards_dirty = true;
        cx.notify();
    }

    fn mode(&self) -> Mode {
        match self.section {
            Section::Tracks => Mode::List,
            _ => self.views[self.section.slot()],
        }
    }

    fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        let section = self.section;
        self.views[section.slot()] = mode;
        if mode == Mode::List {
            self.table(section).set_width(self.width, cx);
        }

        let settings = self.settings.clone();
        settings.update(cx, |settings, cx| {
            settings.set_view(section.key(), mode, cx)
        });
        cx.notify();
    }
}

impl Tooled for LibraryView {
    fn toolbar(&self) -> Entity<Toolbar> {
        self.toolbar.clone()
    }

    fn tools(&self, cx: &App) -> Vec<AnyElement> {
        let columned = self.me.clone();
        let sifted = self.me.clone();
        let sorted = self.me.clone();
        let viewed = self.me.clone();

        let columns = matches!(self.mode(), Mode::List).then(|| {
            tools::columns(&self.popovers, self.column_toggles(cx), move |key, cx| {
                columned
                    .update(cx, |view, cx| view.switch_column(key, cx))
                    .ok();
            })
        });

        let created = self.me.clone();
        let create = (self.section == Section::Playlists).then(|| {
            Button::new("new-playlist")
                .icon("icons/plus.svg")
                .small()
                .ghost()
                .on_click(move |_, window, cx| {
                    created
                        .update(cx, |view, cx| view.create_playlist(window, cx))
                        .ok();
                })
                .into_any_element()
        });

        let mut tools = Vec::new();
        tools.extend(create);
        tools.extend(columns);
        tools.push(tools::filters(
            &self.popovers,
            &self.sliders[self.section.slot()],
            self.ranges(cx),
            self.flags(cx),
            move |change, cx| {
                sifted.update(cx, |view, cx| view.narrow(change, cx)).ok();
            },
            cx,
        ));
        tools.push(tools::sorts(
            &self.popovers,
            self.sorts(cx),
            move |key, cx| {
                sorted.update(cx, |view, cx| view.set_sort(key, cx)).ok();
            },
            cx,
        ));
        let switchable = self.section != Section::Tracks;
        tools.extend(switchable.then(|| {
            tools::views(&self.popovers, self.mode(), move |mode, cx| {
                viewed.update(cx, |view, cx| view.set_mode(mode, cx)).ok();
            })
        }));
        tools
    }
}

impl LibraryView {
    fn sieve(&self, cx: &App) -> TrackSieve {
        self.tracks.read(cx).delegate().source().sieve()
    }

    fn sift(&mut self, sieve: TrackSieve, cx: &mut Context<Self>) {
        self.cards_dirty = true;
        self.tracks.update(cx, |table, cx| {
            table.delegate_mut().source_mut().set_sieve(sieve);
            table.delegate_mut().resift(cx);
            table.refresh(cx);
        });
        cx.notify();
    }

    fn span(&self, cx: &App) -> Option<(f32, f32)> {
        self.albums.read(cx).delegate().source().span()
    }

    fn set_span(&mut self, span: Option<(f32, f32)>, cx: &mut Context<Self>) {
        self.cards_dirty = true;
        self.albums.update(cx, |table, cx| {
            table.delegate_mut().source_mut().set_span(span);
            table.delegate_mut().resift(cx);
            table.refresh(cx);
        });
        cx.notify();
    }

    fn ranges(&self, cx: &App) -> Vec<RangeAxis> {
        match self.section {
            Section::Tracks => {
                let table = self.tracks.read(cx);
                let Some(bounds) = table
                    .delegate()
                    .source()
                    .extent(table.delegate().query(), cx)
                else {
                    return Vec::new();
                };
                let value = self.sieve(cx).duration.unwrap_or(bounds);
                vec![
                    RangeAxis {
                        key: "filter-duration",
                        label: t!("filter-duration"),
                        bounds,
                        value,
                        unit: Unit::Clock,
                        values: None,
                    }
                    .clamped(),
                ]
            }
            Section::Albums => {
                let table = self.albums.read(cx);
                let years = table
                    .delegate()
                    .source()
                    .years(table.delegate().query(), cx);
                let (Some(first), Some(last)) = (years.first(), years.last()) else {
                    return Vec::new();
                };
                let bounds = (*first, *last);
                let value = self.span(cx).unwrap_or(bounds);
                vec![
                    RangeAxis {
                        key: "filter-year",
                        label: t!("filter-year"),
                        bounds,
                        value,
                        unit: Unit::Plain,
                        values: Some(years),
                    }
                    .clamped(),
                ]
            }
            Section::Playlists => Vec::new(),
        }
    }

    fn flags(&self, cx: &App) -> Vec<FlagAxis> {
        if self.section != Section::Tracks {
            return Vec::new();
        }

        let sieve = self.sieve(cx);
        vec![
            FlagAxis {
                key: "filter-explicit",
                label: t!("filter-explicit"),
                on: sieve.explicit,
            },
            FlagAxis {
                key: "filter-playable",
                label: t!("filter-playable"),
                on: sieve.playable,
            },
        ]
    }

    fn narrow(&mut self, change: Sift, cx: &mut Context<Self>) {
        match change {
            Sift::Range("filter-year", value) => self.set_span(Some(value), cx),
            Sift::Range(_, value) => {
                let mut sieve = self.sieve(cx);
                sieve.duration = Some(value);
                self.sift(sieve, cx);
            }
            Sift::Flag(key, on) => {
                let mut sieve = self.sieve(cx);
                match key {
                    "filter-explicit" => sieve.explicit = on,
                    "filter-playable" => sieve.playable = on,
                    _ => return,
                }
                self.sift(sieve, cx);
            }
            Sift::Reset => {
                self.sift(TrackSieve::default(), cx);
                self.set_span(None, cx);
            }
        }
    }
}

fn deck<S: GridSource>(state: &Entity<GridState<S>>, columns: usize, cx: &App) -> Vec<DeckRow> {
    let state = state.read(cx);
    let delegate = state.delegate();
    let mut rows = Vec::new();
    let mut cards = Vec::with_capacity(columns);
    let mut group: Option<SharedString> = None;

    for display in 0..delegate.row_count() {
        let label = delegate.group(display, cx);
        match &label {
            Some(text) if group.as_ref() != Some(text) => {
                if !cards.is_empty() {
                    rows.push(DeckRow::Cards(std::mem::take(&mut cards)));
                }
                rows.push(DeckRow::Heading(display));
            }
            _ => {}
        }
        group = label;
        cards.push((display, delegate.row(display)));
        if cards.len() == columns {
            rows.push(DeckRow::Cards(std::mem::take(&mut cards)));
            cards.reserve(columns);
        }
    }
    if !cards.is_empty() {
        rows.push(DeckRow::Cards(cards));
    }

    rows
}

fn head(label: SharedString, cx: &App) -> AnyElement {
    heading(label, cx).w_full().pt_2().into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_stays_within_its_bounds() {
        for width in (160..2400).step_by(3) {
            let (card, _) = tiling(px(width as f32));
            assert!(card <= CARD_MAX, "{width} yielded {card:?}");
            assert!(card >= CARD_MIN, "{width} yielded {card:?}");
        }
    }

    #[test]
    fn a_row_never_outgrows_the_space_it_was_given() {
        for width in (160..2400).step_by(3) {
            let available = px(width as f32);
            let (card, gap) = tiling(available);
            let columns = ((available + CARD_GAP) / (CARD_MIN + CARD_GAP))
                .floor()
                .max(1.);
            let used = card * columns + gap * (columns - 1.);
            assert!(used <= available, "{width} packed {used:?}");
        }
    }

    #[test]
    fn cards_never_touch() {
        for width in (160..2400).step_by(3) {
            let available = px(width as f32);
            let (_, gap) = tiling(available);
            let columns = ((available + CARD_GAP) / (CARD_MIN + CARD_GAP))
                .floor()
                .max(1.);
            if columns > 1. {
                assert!(gap >= CARD_GAP, "{width} yielded {gap:?}");
            }
        }
    }

    #[test]
    fn slack_goes_to_the_gaps_once_the_cards_are_capped() {
        let capped = CARD_MAX * 2. + CARD_GAP * 2.;
        let (card, gap) = tiling(capped);

        assert_eq!(card, CARD_MAX);
        assert!(gap > CARD_GAP);
    }

    #[test]
    fn a_single_column_has_no_gap() {
        let (_, gap) = tiling(CARD_MIN);
        assert_eq!(gap, Pixels::ZERO);
    }
}
