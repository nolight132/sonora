// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, Pixels, Render, ScrollHandle, SharedString, Window, px,
};

use i18n::t;
use spotify::Track;
use state::{Collection, Detail, Playback};
use ui::ActiveTheme as _;
use ui::{ColumnSpec, GridDelegate, GridEvent, GridState, Scrollbar, Scroller, clock, grid};

use crate::hero::{HeroMetaStrip, HeroPlayButton, PageHero, release_date_label};
use crate::tracks::{PlaybackStatus, TrackField, TrackSource, Tracks, playback_status};
use crate::{cells, page};
use workspace::{Chrome, Searchable};

struct DetailTracks(Entity<Detail>);

impl Tracks for DetailTracks {
    fn tracks<'a>(&self, cx: &'a App) -> &'a [Track] {
        self.0.read(cx).tracks()
    }

    fn is_loading(&self, cx: &App) -> bool {
        self.0.read(cx).is_loading()
    }
}

pub(crate) struct DetailView {
    detail: Entity<Detail>,
    playback: Entity<Playback>,
    playback_status: PlaybackStatus,
    width: Pixels,
    scrollbar: Entity<Scrollbar>,
    table: Entity<GridState<TrackSource>>,
}

impl DetailView {
    pub(crate) fn new(
        detail: Entity<Detail>,
        playback: Entity<Playback>,
        columns: &'static [ColumnSpec<TrackField>],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let inset = cx.theme().metrics.inset;
        let width = cells::content_width(window, page::reserved(inset), cx);

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
                DetailTracks(detail.clone()),
                playback.clone(),
                playlist_scrollbar,
            );
            let source = source.table(cx.weak_entity());
            GridState::new(GridDelegate::new(source, width, cx), cx).follow(scroll)
        });

        cx.observe(&detail, |this, _, cx| {
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
            width,
            scrollbar,
            table,
        }
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.table.update(cx, |table, cx| {
            table.delegate_mut().clear_selection();
            table.rebuild(cx);
        });
    }

    fn header(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let header = self.detail.read(cx).header();
        let kind = header
            .map(|header| header.kind)
            .unwrap_or(Collection::Album);
        let title = header
            .map(|header| SharedString::from(header.title.clone()))
            .unwrap_or_default();
        let artist = header.and_then(|header| header.artist.clone());
        let artist_refs = header
            .map(|header| header.artist_refs.clone())
            .unwrap_or_default();
        let release_date = header.and_then(|header| header.release_date.as_deref());
        let meta = header.map(|header| header.meta.clone()).unwrap_or_default();
        let queued = self.detail.read(cx).tracks().to_vec();
        let duration: std::time::Duration = queued.iter().map(|track| track.duration).sum();
        let (eyebrow, label) = match kind {
            Collection::Playlist => (t!("detail-playlist"), t!("detail-play-playlist")),
            Collection::Album => (t!("detail-album"), t!("detail-play-album")),
        };

        let mut strip = HeroMetaStrip::new();
        if let Some(artist) = artist {
            strip = strip.item(cells::artist_links(
                "detail-artist",
                artist_refs,
                artist,
                muted,
            ));
        }
        if let Some(release_date) = release_date {
            strip = strip.text(release_date_label(release_date));
        }
        for item in meta {
            strip = strip.text(item);
        }
        if !duration.is_zero() {
            strip = strip.text(clock(duration));
        }

        PageHero::new("detail-hero", title)
            .cover(header.and_then(|header| header.cover.clone()))
            .eyebrow(eyebrow)
            .meta(strip)
            .actions(HeroPlayButton::new(
                "play-detail",
                label,
                queued,
                self.playback.clone(),
            ))
            .into_any_element()
    }
}

impl Render for DetailView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let inset = theme.metrics.inset;
        page::resize(&self.table, &mut self.width, inset, window, cx);

        let scroll = self.scrollbar.read(cx).scroll().clone();
        let viewport = page::viewport(&scroll, inset, window);
        self.table
            .update(cx, |table, _| table.set_viewport(viewport));

        Scroller::new("detail-page", &self.scrollbar)
            .px(inset)
            .pt(inset)
            .pb(inset)
            .child(self.header(cx))
            .child(
                grid(&self.table)
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border),
            )
    }
}

impl Searchable for DetailView {
    fn search(&mut self, query: &str, cx: &mut Context<Self>) {
        self.table.update(cx, |table, cx| {
            table.delegate_mut().set_filter(query, cx);
            table.refresh(cx);
        });
        cx.notify();
    }

    fn hint() -> SharedString {
        "filter-album".into()
    }
}
