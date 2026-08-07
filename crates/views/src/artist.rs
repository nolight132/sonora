// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, FontWeight, Pixels, Render, ScrollHandle, SharedString,
    Window, div, px,
};

use i18n::t;
use spotify::{ReleaseType, Track};
use state::{ArtistDetail, Playback};
use ui::ActiveTheme as _;
use ui::{Button, ColumnSpec, GridDelegate, GridEvent, GridState, Scrollbar, Scroller, Text, grid};
use workspace::Chrome;

use crate::hero::{HeroPlayButton, PageHero};
use crate::page;
use crate::release_card::ReleaseCard;
use crate::tracks::{PlaybackStatus, TrackField, TrackSource, Tracks, playback_status};

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
    scrollbar: Entity<Scrollbar>,
    table: Entity<GridState<TrackSource>>,
}

impl ArtistView {
    pub(crate) fn new(
        detail: Entity<ArtistDetail>,
        playback: Entity<Playback>,
        columns: &'static [ColumnSpec<TrackField>],
        cx: &mut Context<Self>,
    ) -> Self {
        let width = px(200.);
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
            );
            let source = source.table(cx.weak_entity());
            GridState::new(GridDelegate::new(source, width, cx), cx).follow(scroll)
        });

        cx.observe(&detail, |this, _, cx| {
            this.release_filter = ReleaseFilter::All;
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
        cx.subscribe(&table, |this, _, event, cx| {
            let GridEvent::DoubleClicked(display) = event;
            page::play(&this.table, &this.playback, *display, cx);
        })
        .detach();

        Self {
            detail,
            playback,
            playback_status: current_playback,
            release_filter: ReleaseFilter::All,
            width,
            scrollbar,
            table,
        }
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

        PageHero::new("artist-hero", title)
            .cover(artist.and_then(|artist| artist.cover_large.clone()))
            .eyebrow(t!("artist-eyebrow"))
            .actions(HeroPlayButton::new(
                "play-artist",
                t!("artist-play"),
                self.detail.read(cx).tracks().to_vec(),
                self.playback.clone(),
            ))
            .circle()
            .into_any_element()
    }

    fn releases(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = *cx.theme();
        let albums = self.detail.read(cx).albums();
        if albums.is_empty() {
            return None;
        }

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
                                    cx.notify();
                                }))
                        })),
                )
                .child(
                    div().flex().flex_wrap().gap_8().children(
                        albums
                            .iter()
                            .filter(|album| self.release_filter.matches(album.release_type))
                            .cloned()
                            .enumerate()
                            .map(|(index, album)| ReleaseCard::new(index, album)),
                    ),
                )
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

impl Render for ArtistView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let inset = theme.metrics.inset;
        page::resize(&self.table, &mut self.width, inset, window, cx);

        let scroll = self.scrollbar.read(cx).scroll().clone();
        let viewport = page::viewport(&scroll, inset, window);
        self.table
            .update(cx, |table, _| table.set_viewport(viewport));

        Scroller::new("artist-page", &self.scrollbar)
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
            .children(self.releases(cx))
    }
}
