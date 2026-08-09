// SPDX-License-Identifier: GPL-3.0-or-later

mod quick_picks;

use crate::chrome::Chrome;
use crate::shared::menu::ItemMenu;
use gpui::prelude::*;
use gpui::{Context, Entity, Pixels, Point, Render, ScrollHandle, Window, div, px};
use state::{Home, Playback};
use ui::{ActiveTheme as _, Popup, Scrollbar, Scroller};

use crate::shared::cells;
use crate::shared::tracks::{PlaybackStatus, playback_status};
use quick_picks::{QuickPicks, column_count, page_count};

pub(crate) struct HomeView {
    home: Entity<Home>,
    playback: Entity<Playback>,
    playback_status: PlaybackStatus,
    quick_picks_columns: usize,
    quick_picks_page: usize,
    scrollbar: Entity<Scrollbar>,
    track_menu: ItemMenu,
    context_menu: Option<(usize, Point<Pixels>)>,
}

impl HomeView {
    pub(crate) fn new(
        home: Entity<Home>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        let playlist_scrollbar = cx.new(|_| {
            Scrollbar::new(ScrollHandle::new())
                .always_visible()
                .track_inset(px(4.))
        });
        let track_menu = ItemMenu::new(playlist_scrollbar);

        cx.observe(&home, |this, _, cx| {
            this.quick_picks_page = 0;
            this.track_menu.reset();
            this.context_menu = None;
            cx.notify();
        })
        .detach();
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();

        let current_playback = playback_status(&playback, cx);
        cx.observe(&playback, |this, playback, cx| {
            let current = playback_status(&playback, cx);
            if this.playback_status != current {
                this.playback_status = current;
                cx.notify();
            }
        })
        .detach();

        Self {
            home,
            playback,
            playback_status: current_playback,
            quick_picks_columns: 0,
            quick_picks_page: 0,
            scrollbar: cx.new(|_| Scrollbar::new(ScrollHandle::new())),
            track_menu,
            context_menu: None,
        }
    }
}

impl Render for HomeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let available = cells::content_width(window, theme.metrics.inset * 2., cx);
        let columns = column_count(available);
        if self.quick_picks_columns != columns {
            self.quick_picks_columns = columns;
            self.quick_picks_page = 0;
        }

        let tracks = self.home.read(cx).quick_picks();
        let pages = page_count(tracks.len(), available);
        self.quick_picks_page = self.quick_picks_page.min(pages.saturating_sub(1));
        let page = self.quick_picks_page;
        let home = cx.entity().downgrade();
        let selected = self.context_menu.and_then(|(place, position)| {
            tracks.get(place).cloned().map(|track| (track, position))
        });
        let context_menu = selected.map(|(track, position)| {
            Popup::new(position, self.track_menu.for_track(&track, cx)).on_close(cx.listener(
                |this, _, _, cx| {
                    this.context_menu = None;
                    cx.notify();
                },
            ))
        });

        Scroller::new("home-page", &self.scrollbar)
            .p(theme.metrics.inset)
            .child(
                div().flex().flex_col().gap_6().child(
                    QuickPicks::new(
                        tracks,
                        self.playback.clone(),
                        self.playback_status.0.clone(),
                        available,
                        page,
                    )
                    .loading(self.home.read(cx).is_loading(cx))
                    .on_previous(cx.listener(|this, _, _, cx| {
                        this.quick_picks_page = this.quick_picks_page.saturating_sub(1);
                        this.context_menu = None;
                        cx.notify();
                    }))
                    .on_next(cx.listener(move |this, _, _, cx| {
                        this.quick_picks_page =
                            (this.quick_picks_page + 1).min(pages.saturating_sub(1));
                        this.context_menu = None;
                        cx.notify();
                    }))
                    .on_context_menu(move |place, event, _, cx| {
                        let Some(home) = home.upgrade() else {
                            return;
                        };
                        home.update(cx, |this, cx| {
                            this.track_menu.reset();
                            this.context_menu = Some((place, event.position));
                            cx.notify();
                        });
                    }),
                ),
            )
            .when_some(context_menu, |this, menu| this.child(menu))
    }
}
