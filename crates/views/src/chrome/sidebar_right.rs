// SPDX-License-Identifier: GPL-3.0-or-later

use std::ops::Range;

use gpui::prelude::*;

use gpui::{
    App, Context, Div, DragMoveEvent, Entity, MouseButton, MouseDownEvent, Pixels, Point, Render,
    ScrollHandle, ScrollStrategy, SharedString, UniformListScrollHandle, Window, div, px,
    uniform_list,
};
use i18n::t;
use spotify::Track;
use state::{AppSettings, Playback, Queue, Sonora};
use ui::{
    ActiveTheme as _, Button, Card, MIN_CONTENT, Panel, Popup, Room, Scrollbar, Side, Text,
    eyebrow, snapped, vacant,
};

use crate::chrome::Chrome;
use crate::shared::menu::ItemMenu;

const MIN_WIDTH: Pixels = px(240.);
const MAX_WIDTH: Pixels = px(560.);
const PINNED_SHARE: f32 = 0.25;

fn fills_content(width: Pixels) -> bool {
    !Room::of(width).fits(Room::Wide)
}

fn section_label(key: &'static str, window: &Window, cx: &App) -> Div {
    div()
        .flex()
        .flex_none()
        .items_end()
        .h(snapped(cx.theme().metrics.list_row, window))
        .px_2()
        .pb_1()
        .child(eyebrow(i18n::lookup(key, None), cx))
}

fn track(queue: &Queue, position: QueuePosition) -> Option<Track> {
    match position {
        QueuePosition::Past(index) => queue.past().nth(index).cloned(),
        QueuePosition::Current => queue.current().cloned(),
        QueuePosition::Upcoming(index) => queue.upcoming().nth(index).cloned(),
        QueuePosition::Similar(index) => queue.similar().nth(index).cloned(),
    }
}

#[derive(Clone)]
struct DraggedTrack {
    index: usize,
    revision: u64,
    name: SharedString,
    position: Point<Pixels>,
}

impl DraggedTrack {
    fn at(mut self, position: Point<Pixels>) -> Self {
        self.position = position;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum QueuePosition {
    Past(usize),
    Current,
    Upcoming(usize),
    Similar(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Slot {
    Header(&'static str),
    Track(QueuePosition),
}

#[derive(Clone, Copy)]
struct Sections {
    past: usize,
    current: bool,
    upcoming: usize,
    similar: usize,
}

impl Sections {
    fn past_end(self) -> usize {
        match self.past {
            0 => 0,
            count => count + 1,
        }
    }

    fn current_end(self) -> usize {
        self.past_end() + 2 * usize::from(self.current)
    }

    fn upcoming_end(self) -> usize {
        self.current_end()
            + match self.upcoming {
                0 => 0,
                count => count + 1,
            }
    }

    fn len(self) -> usize {
        self.upcoming_end()
            + match self.similar {
                0 => 0,
                count => count + 1,
            }
    }

    fn current_index(self) -> Option<usize> {
        self.current.then(|| self.past_end() + 1)
    }

    fn slot(self, index: usize) -> Slot {
        if index < self.past_end() {
            return match index {
                0 => Slot::Header("queue-history"),
                _ => Slot::Track(QueuePosition::Past(index - 1)),
            };
        }
        if index < self.current_end() {
            return match index == self.past_end() {
                true => Slot::Header("queue-now-playing"),
                false => Slot::Track(QueuePosition::Current),
            };
        }
        if index < self.upcoming_end() {
            return match index == self.current_end() {
                true => Slot::Header("queue-up-next"),
                false => Slot::Track(QueuePosition::Upcoming(index - self.current_end() - 1)),
            };
        }
        match index == self.upcoming_end() {
            true => Slot::Header("queue-similar"),
            false => Slot::Track(QueuePosition::Similar(index - self.upcoming_end() - 1)),
        }
    }
}

/// Which edge of a row the drop indicator line is drawn at.
#[derive(Clone, Copy)]
enum DropLine {
    Above,
    Below,
}

#[derive(Clone)]
struct ContextMenuState {
    track: Track,
    revision: u64,
    position: Point<Pixels>,
}

impl QueuePosition {
    fn past(self) -> Option<usize> {
        match self {
            Self::Past(index) => Some(index),
            _ => None,
        }
    }

    fn upcoming(self) -> Option<usize> {
        match self {
            Self::Upcoming(index) => Some(index),
            _ => None,
        }
    }

    fn similar(self) -> Option<usize> {
        match self {
            Self::Similar(index) => Some(index),
            _ => None,
        }
    }
}

impl Render for DraggedTrack {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        div()
            .pl(self.position.x + px(8.))
            .pt(self.position.y + px(8.))
            .child(
                div()
                    .max_w(px(240.))
                    .px_2()
                    .py_1()
                    .rounded(theme.radius)
                    .bg(theme.secondary)
                    .text_color(theme.foreground)
                    .truncate()
                    .child(self.name.clone()),
            )
    }
}

pub(crate) struct SidebarRight {
    queue: Entity<Queue>,
    playback: Entity<Playback>,
    context_menu: Option<ContextMenuState>,
    track_menu: ItemMenu,
    drop_gap: Option<usize>,
    scroll: UniformListScrollHandle,
    scrollbar: Entity<Scrollbar>,
    settings: Entity<AppSettings>,
    width: Pixels,
    past_len: usize,
    anchor: bool,
    open: bool,
}

impl SidebarRight {
    pub(crate) fn new(
        queue: Entity<Queue>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&queue, |this, queue, cx| {
            let revision = queue.read(cx).revision();
            if this
                .context_menu
                .as_ref()
                .is_some_and(|menu| menu.revision != revision)
            {
                this.track_menu.reset();
                this.context_menu = None;
            }
            cx.notify();
        })
        .detach();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();

        let scroll = UniformListScrollHandle::new();
        let scrollbar = cx.new(|_| Scrollbar::new(scroll.0.borrow().base_handle.clone()));
        let playlist_scrollbar = cx.new(|_| {
            Scrollbar::new(ScrollHandle::new())
                .always_visible()
                .track_inset(px(4.))
        });
        let settings = Sonora::global(cx).settings.clone();
        let width = px(settings.read(cx).sidebar_right_width()).clamp(MIN_WIDTH, MAX_WIDTH);
        let open = settings.read(cx).queue_open();

        Self {
            queue,
            playback,
            context_menu: None,
            track_menu: ItemMenu::new(playlist_scrollbar),
            drop_gap: None,
            scroll,
            scrollbar,
            settings,
            width,
            past_len: 0,
            anchor: open,
            open,
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn covers_content(&self, window: &Window) -> bool {
        self.open && fills_content(window.viewport_size().width)
    }

    pub(crate) fn occupied_width(&self, window: &Window) -> Pixels {
        match self.open {
            false => Pixels::ZERO,
            true if self.covers_content(window) => window.viewport_size().width,
            true => self.width,
        }
    }

    fn persist(&self, cx: &mut Context<Self>) {
        let width = self.width / px(1.);
        self.settings.update(cx, |settings, cx| {
            settings.set_sidebar_right_width(width, cx)
        });
    }

    pub(crate) fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        self.anchor = self.open;
        self.remember(cx);
        cx.notify();
    }

    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        self.track_menu.reset();
        self.context_menu = None;
        self.open = false;
        self.remember(cx);
        cx.notify();
    }

    fn remember(&self, cx: &mut Context<Self>) {
        let open = self.open;
        self.settings
            .update(cx, |settings, cx| settings.set_queue_open(open, cx));
    }

    fn dismiss_menu(&mut self, cx: &mut Context<Self>) {
        self.track_menu.reset();
        self.context_menu = None;
        cx.notify();
    }

    fn row(
        track: Track,
        index: usize,
        position: QueuePosition,
        queue_revision: u64,
        drop_line: Option<DropLine>,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = *cx.theme();
        let past_index = position.past();
        let queue_index = position.upcoming();
        let similar_index = position.similar();
        let title = match position {
            QueuePosition::Past(_) => theme.muted_foreground,
            QueuePosition::Current => theme.primary,
            QueuePosition::Upcoming(_) | QueuePosition::Similar(_) => theme.foreground,
        };
        let dragged = queue_index.map(|index| DraggedTrack {
            index,
            revision: queue_revision,
            name: SharedString::from(track.name.clone()),
            position: Point::default(),
        });
        let menu_track = track.clone();

        let card = Card::new(
            ("queue-track", index),
            SharedString::from(track.name.clone()),
        )
        .cover(track.cover.clone())
        .bare_meta(
            crate::shared::cells::artist_links(
                SharedString::from(format!("queue-track-artist-{index}")),
                track.artist_refs.clone(),
                track.artists.clone(),
                theme.muted_foreground,
            )
            .text_size(theme.text(Text::Small))
            .truncate(),
        )
        .tint(title)
        .when(track.explicit, Card::explicit)
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                this.track_menu.reset();
                this.context_menu = Some(ContextMenuState {
                    track: menu_track.clone(),
                    revision: queue_revision,
                    position: event.position,
                });
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .when_some(past_index, |this, index| {
            this.press(cx.listener(move |this, _, _, cx| {
                if this.queue.read(cx).revision() == queue_revision {
                    this.playback
                        .update(cx, |playback, cx| playback.play_past(index, cx));
                }
            }))
        })
        .when_some(queue_index, |this, target| {
            this.press(cx.listener(move |this, _, _, cx| {
                if this.queue.read(cx).revision() == queue_revision {
                    this.playback
                        .update(cx, |playback, cx| playback.play_upcoming(target, cx));
                }
            }))
            .action(
                Button::new(("remove-queued-track", index))
                    .ghost()
                    .small()
                    .icon("icons/x.svg")
                    .tooltip("menu-remove-from-queue")
                    .tint(theme.muted_foreground)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.queue.update(cx, |queue, cx| {
                            if queue.revision() == queue_revision {
                                queue.remove_upcoming(target, cx);
                            }
                        });
                    })),
            )
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<DraggedTrack>, _, cx| {
                    let position = event.event.position;
                    if !event.bounds.contains(&position) {
                        return;
                    }
                    let gap = if position.y < event.bounds.center().y {
                        target
                    } else {
                        target + 1
                    };
                    let dragged = event.drag(cx).index;
                    let gap = (gap != dragged && gap != dragged + 1).then_some(gap);
                    if this.drop_gap != gap {
                        this.drop_gap = gap;
                        cx.notify();
                    }
                }),
            )
            .on_drop(cx.listener(move |this, dragged: &DraggedTrack, _, cx| {
                if let Some(gap) = this.drop_gap.take() {
                    this.queue.update(cx, |queue, cx| {
                        if queue.revision() == dragged.revision {
                            queue.move_upcoming_to_gap(dragged.index, gap, cx);
                        }
                    });
                }
            }))
        })
        .when_some(similar_index, |this, target| {
            this.press(cx.listener(move |this, _, _, cx| {
                if this.queue.read(cx).revision() == queue_revision {
                    this.playback
                        .update(cx, |playback, cx| playback.play_similar(target, cx));
                }
            }))
            .action(
                Button::new(("remove-similar-track", index))
                    .ghost()
                    .small()
                    .icon("icons/x.svg")
                    .tooltip("menu-remove-from-queue")
                    .tint(theme.muted_foreground)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.queue.update(cx, |queue, cx| {
                            if queue.revision() == queue_revision {
                                queue.remove_similar(target, cx);
                            }
                        });
                    })),
            )
        })
        .when_some(dragged, |this, dragged| {
            this.on_drag(dragged, |dragged, position, _, cx| {
                cx.new(|_| dragged.clone().at(position))
            })
        });

        div()
            .id(("queue-track-container", index))
            .relative()
            .min_w_0()
            .child(card)
            .when_some(drop_line, |this, edge| {
                let line = div()
                    .absolute()
                    .left_2()
                    .right_2()
                    .h(px(2.))
                    .rounded_full()
                    .bg(theme.primary);
                this.child(match edge {
                    DropLine::Above => line.top_0(),
                    DropLine::Below => line.bottom_0(),
                })
            })
    }

    fn menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let ContextMenuState {
            track, position, ..
        } = self.context_menu.clone()?;

        Some(
            Popup::new(position, self.track_menu.for_track(&track, cx))
                .on_close(cx.listener(|this, _, _, cx| this.dismiss_menu(cx))),
        )
    }

    fn header(&self, sections: Sections, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap_2()
            .h(theme.metrics.header)
            .px_2()
            .border_b_1()
            .border_color(theme.border)
            .child(eyebrow(t!("queue-title"), cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("toggle-radio")
                            .ghost()
                            .small()
                            .icon("icons/radio.svg")
                            .tooltip("queue-radio")
                            .tint(match self.playback.read(cx).radio() {
                                true => theme.primary,
                                false => theme.muted_foreground,
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.playback
                                    .update(cx, |playback, cx| playback.toggle_radio(cx));
                            })),
                    )
                    .child(
                        Button::new("reset-queue")
                            .ghost()
                            .small()
                            .label(t!("queue-reset"))
                            .tint(theme.muted_foreground)
                            .disabled(!self.queue.read(cx).reordered())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.queue.update(cx, |queue, cx| queue.reset(cx));
                            })),
                    )
                    .child(
                        Button::new("clear-queue")
                            .ghost()
                            .small()
                            .label(t!("queue-clear"))
                            .tint(theme.muted_foreground)
                            .disabled(sections.upcoming == 0)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.queue.update(cx, |queue, cx| queue.clear_upcoming(cx));
                            })),
                    ),
            )
    }

    fn pin(&mut self, sections: Sections, window: &Window, cx: &Context<Self>) {
        let Some(index) = sections.current_index() else {
            self.anchor = false;
            return;
        };

        let viewport = self.scroll.0.borrow().base_handle.bounds().size.height;
        if viewport <= px(0.) {
            window.request_animation_frame();
            return;
        }

        let row = snapped(cx.theme().metrics.list_row, window);
        let above = (viewport * PINNED_SHARE / row).round() as usize;
        self.scroll
            .scroll_to_item_strict_with_offset(index, ScrollStrategy::Top, above);
        self.anchor = false;
    }

    fn rows(&self, sections: Sections, cx: &mut Context<Self>) -> gpui::UniformList {
        let queue = self.queue.clone();
        let drop_gap = self.drop_gap;
        let upcoming = sections.upcoming;

        uniform_list(
            "queue-rows",
            sections.len(),
            cx.processor(move |_, range: Range<usize>, window, cx| {
                let (revision, slots) = {
                    let queue = queue.read(cx);
                    let slots = range
                        .clone()
                        .map(|index| {
                            let slot = sections.slot(index);
                            let found = match slot {
                                Slot::Header(_) => None,
                                Slot::Track(position) => track(queue, position),
                            };
                            (index, slot, found)
                        })
                        .collect::<Vec<_>>();
                    (queue.revision(), slots)
                };

                slots
                    .into_iter()
                    .map(|(index, slot, found)| match (slot, found) {
                        (Slot::Header(key), _) => section_label(key, window, cx).into_any_element(),
                        (Slot::Track(position), Some(found)) => {
                            let drop_line = match (position.upcoming(), drop_gap) {
                                (Some(queued), Some(gap)) if gap == queued => Some(DropLine::Above),
                                (Some(queued), Some(gap))
                                    if gap == upcoming && queued + 1 == upcoming =>
                                {
                                    Some(DropLine::Below)
                                }
                                _ => None,
                            };
                            Self::row(found, index, position, revision, drop_line, cx)
                                .into_any_element()
                        }
                        (Slot::Track(_), None) => div().into_any_element(),
                    })
                    .collect()
            }),
        )
    }
}

impl Render for SidebarRight {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }

        let theme = *cx.theme();
        let fullscreen = fills_content(window.viewport_size().width);
        let queue = self.queue.read(cx);
        let sections = Sections {
            past: queue.past().len(),
            current: queue.current().is_some(),
            upcoming: queue.upcoming().len(),
            similar: queue.similar().len(),
        };
        let empty = sections.len() == 0;
        if !cx.has_active_drag() {
            self.drop_gap = None;
        }

        if self.past_len != sections.past {
            self.past_len = sections.past;
            self.anchor = true;
        }
        if self.anchor {
            self.pin(sections, window, cx);
        }

        Panel::new("sidebar-right", Side::Right, self.width)
            .limits(MIN_WIDTH, MAX_WIDTH)
            .reach(super::cap(MIN_WIDTH, MAX_WIDTH, MIN_CONTENT, window))
            .fill(fullscreen)
            .on_resize(cx.listener(|this, width: &Pixels, _, cx| {
                this.width = *width;
                this.persist(cx);
                cx.notify();
            }))
            .on_drag_move(cx.listener(|this, _: &DragMoveEvent<DraggedTrack>, _, cx| {
                if this.drop_gap.take().is_some() {
                    cx.notify();
                }
            }))
            .when(!theme.transparent, |this| this.bg(theme.background))
            .border_color(theme.border)
            .child(self.header(sections, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .when(empty, |this| {
                        this.child(vacant(t!("queue-empty"), cx).flex_1())
                    })
                    .when(!empty, |this| {
                        this.child(
                            div()
                                .relative()
                                .flex_1()
                                .min_h_0()
                                .child(
                                    self.rows(sections, cx)
                                        .px_2()
                                        .pb_2()
                                        .track_scroll(&self.scroll)
                                        .size_full(),
                                )
                                .child(self.scrollbar.clone()),
                        )
                    }),
            )
            .children(self.menu(cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::{QueuePosition, Sections, Slot, fills_content};

    fn slots(sections: Sections) -> Vec<Slot> {
        (0..sections.len()).map(|i| sections.slot(i)).collect()
    }

    #[test]
    fn fills_narrow_content_area() {
        assert!(fills_content(ui::WIDE - px(1.)));
        assert!(!fills_content(ui::WIDE));
    }

    #[test]
    fn lays_out_every_section() {
        let sections = Sections {
            past: 2,
            current: true,
            upcoming: 2,
            similar: 2,
        };

        assert_eq!(sections.current_index(), Some(4));
        assert_eq!(
            slots(sections),
            [
                Slot::Header("queue-history"),
                Slot::Track(QueuePosition::Past(0)),
                Slot::Track(QueuePosition::Past(1)),
                Slot::Header("queue-now-playing"),
                Slot::Track(QueuePosition::Current),
                Slot::Header("queue-up-next"),
                Slot::Track(QueuePosition::Upcoming(0)),
                Slot::Track(QueuePosition::Upcoming(1)),
                Slot::Header("queue-similar"),
                Slot::Track(QueuePosition::Similar(0)),
                Slot::Track(QueuePosition::Similar(1)),
            ]
        );
    }

    #[test]
    fn suggests_similar_tracks_without_anything_up_next() {
        let sections = Sections {
            past: 0,
            current: true,
            upcoming: 0,
            similar: 1,
        };

        assert_eq!(
            slots(sections),
            [
                Slot::Header("queue-now-playing"),
                Slot::Track(QueuePosition::Current),
                Slot::Header("queue-similar"),
                Slot::Track(QueuePosition::Similar(0)),
            ]
        );
    }

    #[test]
    fn drops_headers_for_empty_sections() {
        let sections = Sections {
            past: 0,
            current: true,
            upcoming: 1,
            similar: 0,
        };

        assert_eq!(sections.current_index(), Some(1));
        assert_eq!(
            slots(sections),
            [
                Slot::Header("queue-now-playing"),
                Slot::Track(QueuePosition::Current),
                Slot::Header("queue-up-next"),
                Slot::Track(QueuePosition::Upcoming(0)),
            ]
        );
    }

    #[test]
    fn lays_out_history_without_a_current_track() {
        let sections = Sections {
            past: 1,
            current: false,
            upcoming: 0,
            similar: 0,
        };

        assert_eq!(sections.current_index(), None);
        assert_eq!(
            slots(sections),
            [
                Slot::Header("queue-history"),
                Slot::Track(QueuePosition::Past(0))
            ]
        );
    }

    #[test]
    fn an_empty_queue_has_no_rows() {
        let sections = Sections {
            past: 0,
            current: false,
            upcoming: 0,
            similar: 0,
        };

        assert_eq!(sections.len(), 0);
        assert_eq!(sections.current_index(), None);
    }
}
