// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Context, Entity, FontWeight, Pixels, Point, Render, ScrollHandle,
    SharedString, Window, div, px,
};

use crate::chrome::Chrome;
use crate::shared::cells;
use i18n::t;
use spotify::{Album, ReleaseType, Track};
use state::{AppSettings, ArtistDetail, Playback, Sonora};
use ui::ActiveTheme as _;
use ui::{
    Button, ColumnSpec, GridDelegate, GridEvent, GridState, MIN_CONTENT, Popover, Popovers, Popup,
    Scrollbar, Scroller, Text, grid,
};

use crate::shared::album_grid::AlbumGrid;
use crate::shared::hero::{HeroMetaStrip, HeroPlayButton, PageHero};
use crate::shared::menu::{album_menu, artist_menu};
use crate::shared::page;
use crate::shared::tracks::{PlaybackStatus, TrackField, TrackSource, Tracks, playback_status};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReleaseFilter {
    All,
    Albums,
    Singles,
    Eps,
}

impl ReleaseFilter {
    const ALL: [Self; 4] = [Self::All, Self::Singles, Self::Albums, Self::Eps];

    fn id(self) -> &'static str {
        match self {
            Self::All => "release-filter-all",
            Self::Albums => "release-filter-albums",
            Self::Singles => "release-filter-singles",
            Self::Eps => "release-filter-eps",
        }
    }

    fn label(self) -> SharedString {
        match self {
            Self::All => t!("artist-filter-all"),
            Self::Albums => t!("artist-filter-albums"),
            Self::Singles => t!("artist-filter-singles"),
            Self::Eps => t!("artist-filter-eps"),
        }
    }

    fn matches(self, kind: ReleaseType) -> bool {
        self == Self::All
            || matches!(
                (self, kind),
                (Self::Albums, ReleaseType::Album)
                    | (Self::Singles, ReleaseType::Single)
                    | (Self::Eps, ReleaseType::Ep)
            )
    }
}

struct ArtistTracks(Entity<ArtistDetail>);

impl Tracks for ArtistTracks {
    fn tracks<'a>(&self, cx: &'a App) -> &'a [Track] {
        self.0.read(cx).tracks()
    }

    fn is_loading(&self, cx: &App) -> bool {
        self.0.read(cx).is_loading()
    }
}

pub(crate) struct ArtistView {
    detail: Entity<ArtistDetail>,
    playback: Entity<Playback>,
    playback_status: PlaybackStatus,
    release_filter: ReleaseFilter,
    width: Pixels,
    release_layout: Rc<RefCell<ReleaseLayout>>,
    scrollbar: Entity<Scrollbar>,
    table: Entity<GridState<TrackSource>>,
    settings: Entity<AppSettings>,
    popovers: Popovers,
    release_menu: Option<(Album, Point<Pixels>)>,
}

#[derive(Default)]
struct ReleaseLayout {
    bounds: Vec<Bounds<Pixels>>,
    offset: Pixels,
}

const SECTION: &str = "artist";

impl ArtistView {
    pub(crate) fn new(
        detail: Entity<ArtistDetail>,
        playback: Entity<Playback>,
        columns: &'static [ColumnSpec<TrackField>],
        cx: &mut Context<Self>,
    ) -> Self {
        let width = MIN_CONTENT;
        let settings = Sonora::global(cx).settings.clone();
        let saved = settings.read(cx).table(SECTION);
        let sorting = settings.read(cx).sorting(SECTION);
        let scrollbar = cx.new(|_| Scrollbar::new(ScrollHandle::new()));
        let scroll = scrollbar.read(cx).scroll().clone();
        let table = cx.new(|cx| {
            let playlist_scrollbar = cx.new(|_| {
                Scrollbar::new(ScrollHandle::new())
                    .always_visible()
                    .track_inset(px(4.))
            });
            let source = TrackSource::new(
                columns,
                ArtistTracks(detail.clone()),
                playback.clone(),
                playlist_scrollbar,
            )
            .with_liked(Sonora::global(cx).library.clone());
            let source = source.table(cx.weak_entity());
            let mut delegate = GridDelegate::new(source, width, cx);
            delegate.set_layout(saved, cx);
            delegate.set_sorting(sorting.flatten(), cx);
            GridState::new(delegate, cx).follow(scroll)
        });

        cx.observe(&detail, |this, _, cx| {
            this.release_filter = ReleaseFilter::All;
            this.release_layout.borrow_mut().bounds.clear();
            this.scrollbar
                .read(cx)
                .scroll()
                .set_offset(gpui::Point::default());
            this.rebuild(cx);
            cx.notify();
        })
        .detach();
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();

        let library = Sonora::global(cx).library.clone();
        cx.observe(&library, |this, _, cx| {
            this.table.update(cx, |table, cx| table.refresh(cx));
        })
        .detach();
        let current_playback = playback_status(&playback, cx);
        cx.observe(&playback, |this, playback, cx| {
            let current = playback_status(&playback, cx);
            if this.playback_status == current {
                return;
            }
            this.playback_status = current;
            this.table.update(cx, |table, cx| table.refresh(cx));
            cx.notify();
        })
        .detach();
        cx.subscribe(&table, |this, _, event, cx| match event {
            GridEvent::DoubleClicked(display) => {
                page::play(&this.table, &this.playback, *display, cx)
            }
            _ => this.persist(cx),
        })
        .detach();

        Self {
            detail,
            playback,
            playback_status: current_playback,
            release_filter: ReleaseFilter::All,
            width,
            release_layout: Rc::new(RefCell::new(ReleaseLayout::default())),
            scrollbar,
            table,
            settings,
            popovers: Popovers::default(),
            release_menu: None,
        }
    }

    fn persist(&mut self, cx: &mut Context<Self>) {
        page::store(
            &self.settings.clone(),
            &self.table.clone(),
            SECTION,
            SECTION,
            cx,
        );
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.table.update(cx, |table, cx| {
            table.rebuild(cx);
        });
    }

    fn header(&self, cx: &Context<Self>) -> AnyElement {
        let artist = self.detail.read(cx).artist();
        let title = artist
            .map(|artist| SharedString::from(artist.name.clone()))
            .unwrap_or_default();
        let listeners = artist
            .and_then(|artist| artist.monthly_listeners)
            .map(|count| {
                let value = cells::count(count);
                t!("artist-monthly-listeners", count = count, value = &value)
            });
        let overflow = self.detail.read(cx).id().map(|id| {
            Popover::new("artist-overflow", self.popovers.clone())
                .commands()
                .button(
                    Button::new("artist-overflow-button")
                        .outline()
                        .icon("icons/ellipsis.svg")
                        .tooltip("common-more"),
                )
                .menu(
                    artist_menu(id.to_owned())
                        .top(cx.theme().metrics.control)
                        .left_0(),
                )
        });
        let actions = div()
            .flex()
            .items_center()
            .gap_2()
            .child(HeroPlayButton::new(
                "play-artist",
                t!("artist-play"),
                self.detail.read(cx).tracks().to_vec(),
                self.playback.clone(),
            ))
            .children(overflow);

        PageHero::new("artist-hero", title)
            .cover(artist.and_then(|artist| artist.cover_large.clone()))
            .eyebrow(t!("artist-eyebrow"))
            .when_some(listeners, |hero, listeners| {
                hero.meta(HeroMetaStrip::new().text(listeners))
            })
            .actions(actions)
            .circle()
            .into_any_element()
    }

    fn releases(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = *cx.theme();
        let albums = self.detail.read(cx).albums();
        if albums.is_empty() {
            return None;
        }

        let scroll = self.scrollbar.read(cx).scroll().clone();
        let grid = AlbumGrid::layout(self.width);
        let initial = grid.columns * 2;
        let overdraw = grid.card * 2.;
        let albums = albums
            .iter()
            .filter(|album| self.release_filter.matches(album.release_type))
            .cloned()
            .enumerate()
            .collect::<Vec<_>>();
        let load_layout = self.release_layout.clone();
        let load_scroll = scroll.clone();
        let release_layout = self.release_layout.clone();
        let view = cx.entity().downgrade();
        let opened = cx.entity().downgrade();
        let releases = AlbumGrid::new("artist-release", self.width, albums, self.playback.clone())
            .on_context(move |album, position, cx| {
                let Some(view) = opened.upgrade() else {
                    return;
                };
                view.update(cx, |this, cx| {
                    this.release_menu = Some((album.clone(), position));
                    cx.notify();
                });
            })
            .load_art_when(move |index| {
                release_near(
                    &load_layout.borrow().bounds,
                    index,
                    &load_scroll,
                    overdraw,
                    initial,
                )
            })
            .on_layout(move |bounds, cx| {
                let offset = scroll.offset().y;
                let changed = {
                    let mut layout = release_layout.borrow_mut();
                    let changed = layout.bounds != bounds || layout.offset != offset;
                    layout.bounds = bounds;
                    layout.offset = offset;
                    changed
                };
                if changed {
                    view.update(cx, |_, cx| cx.notify()).ok();
                }
            });

        Some(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .pt_6()
                .child(
                    div()
                        .text_size(theme.text(Text::Title))
                        .font_weight(FontWeight::BOLD)
                        .child(t!("artist-releases")),
                )
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .children(ReleaseFilter::ALL.map(|filter| {
                            Button::new(filter.id())
                                .label(filter.label())
                                .small()
                                .outline()
                                .selected(self.release_filter == filter)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.release_filter = filter;
                                    this.release_layout.borrow_mut().bounds.clear();
                                    cx.notify();
                                }))
                        })),
                )
                .child(releases)
                .into_any_element(),
        )
    }

    fn failure(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let error = self.detail.read(cx).error()?.to_owned();
        Some(
            div()
                .pb_4()
                .text_color(cx.theme().danger)
                .child(error)
                .into_any_element(),
        )
    }
}

fn release_near(
    bounds: &[Bounds<Pixels>],
    index: usize,
    scroll: &ScrollHandle,
    overdraw: Pixels,
    initial: usize,
) -> bool {
    let Some(bounds) = bounds.get(index) else {
        return index < initial;
    };
    let viewport = scroll.bounds();
    bounds.bottom() >= viewport.top() - overdraw && bounds.top() <= viewport.bottom() + overdraw
}

impl Render for ArtistView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let inset = theme.metrics.inset;
        page::resize(&self.table, &mut self.width, inset, window, cx);

        let scroll = self.scrollbar.read(cx).scroll().clone();
        let viewport = page::viewport(&scroll, inset, window);
        self.table
            .update(cx, |table, _| table.set_viewport(viewport));

        let release_menu = self.release_menu.clone().map(|(album, position)| {
            let menu = album_menu(album.id, self.playback.clone(), false);
            Popup::new(position, menu).on_close(cx.listener(|this, _, _, cx| {
                this.release_menu = None;
                cx.notify();
            }))
        });

        let page = Scroller::new("artist-page", &self.scrollbar)
            .px(inset)
            .pt(inset)
            .pb(inset)
            .child(
                div()
                    .child(self.header(cx))
                    .children(self.failure(cx))
                    .child(
                        div()
                            .pb_3()
                            .text_size(theme.text(Text::Title))
                            .font_weight(FontWeight::BOLD)
                            .child(t!("artist-popular")),
                    ),
            )
            .child(
                grid(&self.table)
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border),
            )
            .children(self.releases(cx));

        div()
            .relative()
            .size_full()
            .child(page)
            .when_some(release_menu, |this, menu| this.child(menu))
    }
}
