// SPDX-License-Identifier: GPL-3.0-or-later

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Entity, MouseButton, MouseDownEvent, Pixels, SharedString, Window, div,
};
use i18n::t;
use spotify::Track;
use state::Playback;
use ui::{ActiveTheme as _, Button, Card, Text, eyebrow, heading};

use crate::shared::cells;

const ROWS_PER_COLUMN: usize = 6;
const MAX_COLUMNS: usize = 3;
const MIN_COLUMN_WIDTH: Pixels = gpui::px(280.);

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type ContextHandler = Rc<dyn Fn(usize, &MouseDownEvent, &mut Window, &mut App)>;

pub(crate) fn column_count(width: Pixels) -> usize {
    ((width / MIN_COLUMN_WIDTH).floor().max(1.) as usize).min(MAX_COLUMNS)
}

pub(crate) fn page_count(track_count: usize, width: Pixels) -> usize {
    track_count
        .div_ceil(column_count(width) * ROWS_PER_COLUMN)
        .max(1)
}

#[derive(IntoElement)]
pub(crate) struct QuickPicks {
    tracks: Rc<Vec<Track>>,
    playback: Entity<Playback>,
    active: Option<String>,
    width: Pixels,
    page: usize,
    on_previous: Option<ClickHandler>,
    on_next: Option<ClickHandler>,
    on_context_menu: Option<ContextHandler>,
}

impl QuickPicks {
    pub(crate) fn new(
        tracks: Rc<Vec<Track>>,
        playback: Entity<Playback>,
        active: Option<String>,
        width: Pixels,
        page: usize,
    ) -> Self {
        Self {
            tracks,
            playback,
            active,
            width,
            page,
            on_previous: None,
            on_next: None,
            on_context_menu: None,
        }
    }

    pub(crate) fn on_previous(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_previous = Some(Rc::new(handler));
        self
    }

    pub(crate) fn on_next(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_next = Some(Rc::new(handler));
        self
    }

    pub(crate) fn on_context_menu(
        mut self,
        handler: impl Fn(usize, &MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_context_menu = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for QuickPicks {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.theme();
        let columns = column_count(self.width);
        let page_size = columns * ROWS_PER_COLUMN;
        let pages = page_count(self.tracks.len(), self.width);
        let page = self.page.min(pages.saturating_sub(1));
        let start = page * page_size;
        let end = (start + page_size).min(self.tracks.len());
        let tracks = self.tracks;
        let empty = tracks.is_empty();
        let on_previous = self.on_previous;
        let on_next = self.on_next;
        let on_context_menu = self.on_context_menu;

        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_end()
                    .justify_between()
                    .gap_4()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(eyebrow(t!("home-quick-picks-eyebrow"), cx))
                            .child(heading(t!("home-quick-picks"), cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                Button::new("quick-picks-previous")
                                    .small()
                                    .outline()
                                    .icon("icons/chevron-left.svg")
                                    .disabled(empty || page == 0)
                                    .when_some(on_previous, |button, handler| {
                                        button.on_click(move |event, window, cx| {
                                            handler(event, window, cx)
                                        })
                                    }),
                            )
                            .child(
                                Button::new("quick-picks-next")
                                    .small()
                                    .outline()
                                    .icon("icons/chevron-right.svg")
                                    .disabled(empty || page + 1 >= pages)
                                    .when_some(on_next, |button, handler| {
                                        button.on_click(move |event, window, cx| {
                                            handler(event, window, cx)
                                        })
                                    }),
                            ),
                    ),
            )
            .child(div().flex().gap_2().p_2().when_else(
                empty,
                |this| {
                    this.children((0..columns).map(|column| {
                        column_shell(column, theme.border).children(
                            (0..ROWS_PER_COLUMN)
                                .map(|row| skeleton(column * ROWS_PER_COLUMN + row)),
                        )
                    }))
                },
                |this| {
                    this.children(tracks[start..end].chunks(ROWS_PER_COLUMN).enumerate().map(
                        |(column, column_tracks)| {
                            column_shell(column, theme.border).children(
                                column_tracks.iter().enumerate().map(|(row, track)| {
                                    let place = start + column * ROWS_PER_COLUMN + row;
                                    pick(
                                        track,
                                        place,
                                        tracks.clone(),
                                        self.playback.clone(),
                                        self.active.as_deref(),
                                        on_context_menu.clone(),
                                        cx,
                                    )
                                }),
                            )
                        },
                    ))
                },
            ))
    }
}

fn column_shell(column: usize, border: gpui::Hsla) -> gpui::Div {
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .flex_col()
        .gap_1()
        .when(column > 0, |this| {
            this.border_l_1().border_color(border).pl_2()
        })
}

fn skeleton(place: usize) -> impl IntoElement {
    Card::new(("quick-pick-skeleton", place), "").loading()
}

fn pick(
    track: &Track,
    place: usize,
    tracks: Rc<Vec<Track>>,
    playback: Entity<Playback>,
    active: Option<&str>,
    on_context_menu: Option<ContextHandler>,
    cx: &App,
) -> impl IntoElement {
    let theme = *cx.theme();
    let tint = match track.id.as_deref() == active {
        true => theme.primary,
        false => theme.foreground,
    };

    Card::new(
        ("quick-pick", place),
        SharedString::from(track.name.clone()),
    )
    .cover(track.cover.clone())
    .underline()
    .tint(tint)
    .when(track.explicit, |card| card.explicit())
    .bare_meta(
        cells::artist_links(
            SharedString::from(format!("quick-pick-artist-{place}")),
            track.artist_refs.clone(),
            track.artists.clone(),
            theme.muted_foreground,
        )
        .text_size(theme.text(Text::Small))
        .truncate(),
    )
    .when_some(on_context_menu, |card, handler| {
        card.on_mouse_down(MouseButton::Right, move |event, window, cx| {
            window.prevent_default();
            handler(place, event, window, cx);
        })
    })
    .press(move |_, _, cx| {
        playback.update(cx, |playback, cx| playback.play_radio(&tracks[place], cx));
    })
    .min_w_0()
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::{column_count, page_count};

    #[test]
    fn columns_follow_width_and_stop_at_three() {
        assert_eq!(column_count(px(279.)), 1);
        assert_eq!(column_count(px(560.)), 2);
        assert_eq!(column_count(px(840.)), 3);
        assert_eq!(column_count(px(2_000.)), 3);
    }

    #[test]
    fn pages_include_every_track() {
        assert_eq!(page_count(36, px(279.)), 6);
        assert_eq!(page_count(36, px(560.)), 3);
        assert_eq!(page_count(36, px(840.)), 2);
        assert_eq!(page_count(0, px(840.)), 1);
    }
}
