// SPDX-License-Identifier: GPL-3.0-or-later

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
    Button, Card, ColumnSpec, GridDelegate, GridEvent, GridState, MIN_CONTENT, Popover, Popovers,
    Popup, Scrollbar, Scroller, Skeleton, Text, grid,
};

use crate::shared::album_grid::{AlbumGrid, CardGrid};
use crate::shared::hero::{HeroMetaStrip, HeroPlayButton, PageHero};
use crate::shared::menu::{album_menu, artist_menu};
use crate::shared::page;
use crate::shared::tracks::{PlaybackStatus, TrackField, TrackSource, Tracks, playback_status};

const SECTION: &str = "artist";
const END_WIDTH: Pixels = px(72.);
const END_HEIGHT: Pixels = px(1.);

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
    artist_id: Option<String>,
    release_filter: ReleaseFilter,
    width: Pixels,
    release_end: Entity<ReleaseEnd>,
    scrollbar: Entity<Scrollbar>,
    table: Entity<GridState<TrackSource>>,
    settings: Entity<AppSettings>,
    popovers: Popovers,
    release_menu: Option<(Album, Point<Pixels>)>,
}

#[derive(Clone, Copy)]
struct ReleaseMetrics {
    columns: usize,
    card: Pixels,
    gap: Pixels,
}

impl ReleaseMetrics {
    fn height(self, count: usize) -> Pixels {
        if count == 0 {
            return Pixels::ZERO;
        }
        let rows = count.div_ceil(self.columns) as f32;
        self.card * rows + self.gap * (rows - 1.)
    }
}

struct ReleaseEnd {
    hold: Pixels,
    natural: Pixels,
    count: usize,
    metrics: Option<ReleaseMetrics>,
    frame: Option<ReleaseFrame>,
    ready: bool,
}

#[derive(Clone, Copy, PartialEq)]
struct ReleaseFrame {
    count: usize,
    columns: usize,
    height: Pixels,
    hold: Pixels,
}

struct ReleaseUpdate {
    next: bool,
    settle: bool,
}

impl ReleaseEnd {
    fn new() -> Self {
        Self {
            hold: Pixels::ZERO,
            natural: Pixels::ZERO,
            count: 0,
            metrics: None,
            frame: None,
            ready: false,
        }
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        *self = Self::new();
        cx.notify();
    }

    fn select(&mut self, count: usize, depth: Pixels, viewport: Pixels) -> bool {
        let hold = match (self.metrics, self.ready) {
            (Some(metrics), true) => {
                self.natural += metrics.height(count) - metrics.height(self.count);
                (depth - self.natural).max(Pixels::ZERO)
            }
            _ if depth > Pixels::ZERO => {
                self.ready = false;
                depth + viewport
            }
            _ => {
                self.ready = false;
                Pixels::ZERO
            }
        };
        self.count = count;
        let changed = self.hold != hold;
        self.hold = hold;
        changed
    }

    fn resize(&mut self, depth: Pixels, viewport: Pixels) -> bool {
        self.metrics = None;
        self.frame = None;
        self.ready = false;
        let hold = match depth > Pixels::ZERO {
            true => depth + viewport,
            false => Pixels::ZERO,
        };
        let changed = self.hold != hold;
        self.hold = hold;
        changed
    }

    fn retreat(&mut self, depth: Pixels) -> bool {
        if !self.ready {
            return false;
        }
        let hold = match depth > Pixels::ZERO {
            true => (depth - self.natural).max(Pixels::ZERO).min(self.hold),
            false => Pixels::ZERO,
        };
        let changed = self.hold != hold;
        self.hold = hold;
        changed
    }

    fn maximum(&self) -> Option<Pixels> {
        self.ready
            .then(|| (self.natural + self.hold).max(Pixels::ZERO))
    }

    fn observe(
        &mut self,
        bounds: &[Bounds<Pixels>],
        maximum: Pixels,
        columns: usize,
        count: usize,
    ) -> ReleaseUpdate {
        let metrics = release_metrics(bounds, columns, self.metrics);
        let frame = ReleaseFrame {
            count,
            columns,
            height: metrics.map_or(Pixels::ZERO, |metrics| metrics.height(count)),
            hold: self.hold,
        };
        let stable = self.frame == Some(frame);
        let changed = self.frame != Some(frame);
        self.frame = Some(frame);
        self.count = count;
        self.metrics = metrics;
        if !stable {
            return ReleaseUpdate {
                next: changed,
                settle: false,
            };
        }

        self.natural = maximum - self.hold;
        self.ready = true;
        ReleaseUpdate {
            next: false,
            settle: true,
        }
    }
}

impl Render for ReleaseEnd {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        div()
            .flex()
            .flex_col()
            .child(div().flex_none().h(self.hold))
            .child(
                div()
                    .flex()
                    .justify_center()
                    .py_6()
                    .child(div().w(END_WIDTH).h(END_HEIGHT).bg(theme.border)),
            )
    }
}

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
        let release_end = cx.new(|_| ReleaseEnd::new());
        let release_scroll = release_end.clone();
        let scrollbar = cx.new(|_| {
            Scrollbar::new(ScrollHandle::new()).on_scroll(move |depth, cx| {
                release_scroll.update(cx, |end, cx| {
                    if !end.retreat(depth) {
                        return None;
                    }
                    cx.notify();
                    end.maximum()
                })
            })
        });
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

        cx.observe(&detail, |this, detail, cx| {
            let artist_id = detail.read(cx).id().map(str::to_owned);
            if this.artist_id != artist_id {
                this.artist_id = artist_id;
                this.release_filter = ReleaseFilter::All;
                this.release_end.update(cx, |end, cx| end.reset(cx));
                this.scrollbar.update(cx, |bar, cx| {
                    bar.set_max_offset(None, cx);
                    bar.scroll().set_offset(gpui::Point::default());
                });
            }
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
        let artist_id = detail.read(cx).id().map(str::to_owned);
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
            artist_id,
            release_filter: ReleaseFilter::All,
            width,
            release_end,
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
        let detail = self.detail.read(cx);
        let loading = detail.is_loading();
        let albums = detail.albums();
        if albums.is_empty() && !loading {
            return None;
        }

        let grid = CardGrid::layout(self.width);
        let releases = match loading {
            true => div()
                .flex()
                .flex_col()
                .gap_6()
                .children((0..2).map(|row| {
                    CardGrid::new(self.width).children((0..grid.columns).map(move |column| {
                        let index = row * grid.columns + column;
                        Card::new(("artist-release-skeleton", index), "")
                            .tile(grid.card)
                            .loading()
                            .into_any_element()
                    }))
                }))
                .into_any_element(),
            false => {
                let scroll = self.scrollbar.read(cx).scroll().clone();
                let albums = albums
                    .iter()
                    .filter(|album| self.release_filter.matches(album.release_type))
                    .cloned()
                    .enumerate()
                    .collect::<Vec<_>>();
                let count = albums.len();
                let columns = AlbumGrid::columns(self.width);
                let release_end = self.release_end.clone();
                let scrollbar = self.scrollbar.clone();
                let opened = cx.entity().downgrade();

                AlbumGrid::new("artist-release", self.width, albums, self.playback.clone())
                    .on_context(move |album, position, cx| {
                        let Some(view) = opened.upgrade() else {
                            return;
                        };
                        view.update(cx, |this, cx| {
                            this.release_menu = Some((album.clone(), position));
                            cx.notify();
                        });
                    })
                    .on_layout(move |bounds, window, cx| {
                        let update = release_end.update(cx, |end, _| {
                            end.observe(&bounds, scroll.max_offset().y, columns, count)
                        });
                        if update.next {
                            scrollbar.update(cx, |_, cx| cx.notify());
                            window.request_animation_frame();
                        }
                        if update.settle
                            && scrollbar.update(cx, |bar, cx| bar.set_max_offset(None, cx))
                        {
                            window.request_animation_frame();
                        }
                    })
                    .into_any_element()
            }
        };

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
                                    if this.release_filter == filter {
                                        return;
                                    }
                                    let count = this
                                        .detail
                                        .read(cx)
                                        .albums()
                                        .iter()
                                        .filter(|album| filter.matches(album.release_type))
                                        .count();
                                    let scroll = this.scrollbar.read(cx).scroll().clone();
                                    let depth = (-scroll.offset().y).max(Pixels::ZERO);
                                    let viewport = scroll.bounds().size.height;
                                    let maximum = this.release_end.update(cx, |end, cx| {
                                        if end.select(count, depth, viewport) {
                                            cx.notify();
                                        }
                                        end.maximum()
                                    });
                                    this.scrollbar.update(cx, |bar, cx| {
                                        bar.set_max_offset(maximum, cx);
                                    });
                                    this.release_filter = filter;
                                    cx.notify();
                                }))
                        })),
                )
                .child(releases)
                .child(self.release_end.clone())
                .into_any_element(),
        )
    }

    fn tracks_loading(&self, cx: &Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let line = || Skeleton::new().w_full().h(theme.metrics.pad);

        div()
            .w_full()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(theme.metrics.header)
                    .px(theme.metrics.pad)
                    .bg(theme.table_head)
                    .child(line()),
            )
            .children((0..5).map(|_| {
                div()
                    .flex()
                    .items_center()
                    .h(theme.metrics.row)
                    .px(theme.metrics.pad)
                    .border_t_1()
                    .border_color(theme.table_row_border)
                    .child(line())
            }))
            .into_any_element()
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

impl Render for ArtistView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let inset = theme.metrics.inset;
        let previous = self.width;
        page::resize(&self.table, &mut self.width, inset, window, cx);

        let scroll = self.scrollbar.read(cx).scroll().clone();
        if self.width != previous {
            let depth = (-scroll.offset().y).max(Pixels::ZERO);
            let viewport = scroll.bounds().size.height;
            self.release_end.update(cx, |end, cx| {
                if end.resize(depth, viewport) {
                    cx.notify();
                }
            });
            self.scrollbar
                .update(cx, |bar, cx| bar.set_max_offset(None, cx));
        }
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

        let release_end = self.release_end.clone();
        let scrollbar = self.scrollbar.clone();
        let wheel = scroll.clone();
        let page = Scroller::new("artist-page", &self.scrollbar)
            .px(inset)
            .pt(inset)
            .pb(inset)
            .on_scroll_wheel(move |_, window, _| {
                let release_end = release_end.clone();
                let scrollbar = scrollbar.clone();
                let scroll = wheel.clone();
                window.on_next_frame(move |_, cx| {
                    let depth = (-scroll.offset().y).max(Pixels::ZERO);
                    let maximum = release_end.update(cx, |end, cx| {
                        if !end.retreat(depth) {
                            return None;
                        }
                        cx.notify();
                        end.maximum()
                    });
                    if let Some(maximum) = maximum {
                        scrollbar.update(cx, |bar, cx| {
                            bar.set_max_offset(Some(maximum), cx);
                        });
                    }
                });
            })
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
            .child(match self.detail.read(cx).is_loading() {
                true => self.tracks_loading(cx),
                false => grid(&self.table)
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border)
                    .into_any_element(),
            })
            .children(self.releases(cx));

        div()
            .relative()
            .size_full()
            .child(page)
            .when_some(release_menu, |this, menu| this.child(menu))
    }
}

fn release_metrics(
    bounds: &[Bounds<Pixels>],
    columns: usize,
    previous: Option<ReleaseMetrics>,
) -> Option<ReleaseMetrics> {
    let first = bounds.first()?;
    let card = first.size.height;
    let gap = bounds
        .get(columns)
        .map(|next| (next.top() - first.top() - card).max(Pixels::ZERO))
        .or_else(|| previous.map(|metrics| metrics.gap))
        .unwrap_or(Pixels::ZERO);
    Some(ReleaseMetrics { columns, card, gap })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_filter_is_sized_before_rendering() {
        let metrics = ReleaseMetrics {
            columns: 5,
            card: px(170.),
            gap: px(24.),
        };
        let mut end = ReleaseEnd {
            hold: Pixels::ZERO,
            natural: px(1800.),
            count: 50,
            metrics: Some(metrics),
            frame: None,
            ready: true,
        };

        assert!(end.select(5, px(1500.), px(800.)));
        let natural = px(1800.) + metrics.height(5) - metrics.height(50);
        assert_eq!(end.natural, natural);
        assert_eq!(end.natural + end.hold, px(1500.));
        assert_eq!(end.maximum(), Some(px(1500.)));
    }

    #[test]
    fn scrolling_up_retires_the_empty_space() {
        let metrics = ReleaseMetrics {
            columns: 5,
            card: px(170.),
            gap: px(24.),
        };
        let mut end = ReleaseEnd {
            hold: px(600.),
            natural: px(300.),
            count: 5,
            metrics: Some(metrics),
            frame: None,
            ready: true,
        };

        assert!(end.retreat(px(700.)));
        assert_eq!(end.hold, px(400.));
        assert_eq!(end.maximum(), Some(px(700.)));
    }

    #[test]
    fn reaching_the_top_removes_the_empty_space() {
        let metrics = ReleaseMetrics {
            columns: 5,
            card: px(170.),
            gap: px(24.),
        };
        let mut end = ReleaseEnd {
            hold: px(1200.),
            natural: px(-300.),
            count: 5,
            metrics: Some(metrics),
            frame: None,
            ready: true,
        };

        assert!(end.retreat(Pixels::ZERO));
        assert_eq!(end.hold, Pixels::ZERO);
        assert_eq!(end.maximum(), Some(Pixels::ZERO));
    }

    #[test]
    fn measuring_only_calibrates_the_extent() {
        let metrics = ReleaseMetrics {
            columns: 5,
            card: px(170.),
            gap: px(24.),
        };
        let frame = ReleaseFrame {
            count: 5,
            columns: 5,
            height: metrics.height(5),
            hold: px(600.),
        };
        let mut end = ReleaseEnd {
            hold: px(600.),
            natural: Pixels::ZERO,
            count: 5,
            metrics: Some(metrics),
            frame: Some(frame),
            ready: true,
        };
        let bounds = [Bounds::new(
            gpui::point(Pixels::ZERO, Pixels::ZERO),
            gpui::size(px(170.), px(170.)),
        )];

        let update = end.observe(&bounds, px(900.), 5, 5);
        assert!(update.settle);
        assert_eq!(end.natural, px(300.));
        assert_eq!(end.hold, px(600.));
    }
}
