use std::ops::Range;

use gpui::prelude::*;

use gpui::{
    Animation, AnimationExt as _, App, Context, Div, DragMoveEvent, Entity, FontWeight,
    MouseDownEvent, Pixels, Point, Render, ScrollHandle, ScrollStrategy, ScrollWheelEvent,
    SharedString, Task, UniformListScrollHandle, Window, div, ease_in_out, px, relative, svg,
    uniform_list,
};
use i18n::t;
use music::{Track, Voice};
use state::{
    AppSettings, Lyrics, LyricsState, Playback, PlaybackState, Queue, RomanizationScripts, SideTab,
    Sonora,
};
use ui::{
    ActiveTheme as _, Button, Card, DraggedPin, Edge, Motion, Motioned as _, Pin, PinKind,
    Pinnable as _, Popup, Scrollbar, Scroller, Spot, Sweep, Text, drop_gap, drop_marker,
    ease_out_quad, eyebrow, mix, snapped, vacant,
};

use crate::chrome::{Chrome, section_label};
use crate::shared::effects;
use crate::shared::menu::ItemMenu;

const QUEUE: &str = "queue";
const FADE: f32 = 96.;
const REST: f32 = FADE * 0.75;
const TAIL_ROWS: usize = 2;
const BLUR: f32 = 0.07;
const PAST: f32 = 0.4;
const REVEAL: f32 = 0.6;
const KARAOKE_WEIGHT: f32 = 500.;
const KARAOKE_EMBOLDEN_SHARE: f32 = 0.018;
const ACTIVE_VERSE_GROWTH: Pixels = px(2.);
const PINNED_SHARE: f32 = 0.25;
const PIN: f32 = 0.3;
const SETTLE: std::time::Duration = std::time::Duration::from_secs(4);
const INSTRUMENTAL_BREAK: std::time::Duration = std::time::Duration::from_secs(5);
const GLYPH: f32 = 0.35;
const GLYPH_SIZE: f32 = 0.5;
const SWEEP_LEAST: std::time::Duration = std::time::Duration::from_millis(180);
const SWEPT: f32 = 0.98;
const LANDING: f32 = 0.2;

fn track(queue: &Queue, position: QueuePosition) -> Option<Track> {
    match position {
        QueuePosition::Past(index) => queue.past().nth(index).cloned(),
        QueuePosition::Current => queue.current().cloned(),
        QueuePosition::Upcoming(index) => queue.upcoming().nth(index).cloned(),
        QueuePosition::Similar(index) => queue.similar().nth(index).cloned(),
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

#[derive(Clone, Copy)]
struct Sung {
    karaoke: bool,
    sweep: Sweep,
    scripts: Option<RomanizationScripts>,
    theme: ui::Theme,
}

#[derive(Clone, Copy)]
struct RowLook {
    playing: bool,
    drop_line: Option<Edge>,
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

pub(crate) struct Aside {
    queue: Entity<Queue>,
    playback: Entity<Playback>,
    lyrics: Entity<Lyrics>,
    settings: Entity<AppSettings>,
    tab: SideTab,
    verse_bar: Entity<Scrollbar>,
    followed: Option<usize>,
    goal: Option<Pixels>,
    pinned: bool,
    nudged: Option<std::time::Instant>,
    verse_of: Option<String>,
    verse_take: u64,
    placing: bool,
    context_menu: Option<ContextMenuState>,
    track_menu: ItemMenu,
    drop_gap: Option<usize>,
    scroll: UniformListScrollHandle,
    scrollbar: Entity<Scrollbar>,
    past_len: usize,
    anchor: bool,
    titled: bool,
    aiming: bool,
    rested: Option<Pixels>,
    since: std::time::Instant,
    over: Option<usize>,
    hovered: Option<usize>,
    fading: Option<usize>,
    linger: Option<Task<()>>,
    previous_active_line: Option<usize>,
    departing_line: Option<usize>,
    departed: std::time::Instant,
    arrival: u64,
    departure: u64,
    focused: Option<usize>,
    faded: Option<usize>,
    shifted: std::time::Instant,
}

impl Aside {
    pub(crate) fn new(
        queue: Entity<Queue>,
        playback: Entity<Playback>,
        tab: SideTab,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&queue, |this, queue, cx| {
            let revision = queue.read(cx).revision();
            if this
                .context_menu
                .as_ref()
                .is_some_and(|menu| menu.revision != revision)
            {
                this.track_menu.reset(cx);
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
        let playlist_scrollbar = cx.new(|_| Scrollbar::inset());
        let lyrics = Sonora::global(cx).lyrics.clone();
        cx.observe(&lyrics, |_, _, cx| cx.notify()).detach();
        let settings = Sonora::global(cx).settings.clone();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();
        let verse_bar = cx.new(|_| Scrollbar::new(ScrollHandle::new()));

        Self {
            queue,
            playback,
            lyrics,
            settings,
            tab,
            verse_bar,
            followed: None,
            goal: None,
            pinned: true,
            nudged: None,
            verse_of: None,
            verse_take: 0,
            placing: false,
            context_menu: None,
            track_menu: ItemMenu::new(playlist_scrollbar),
            drop_gap: None,
            scroll,
            scrollbar,
            past_len: 0,
            anchor: true,
            titled: true,
            aiming: false,
            rested: None,
            since: std::time::Instant::now(),
            over: None,
            hovered: None,
            fading: None,
            linger: None,
            previous_active_line: None,
            departing_line: None,
            departed: std::time::Instant::now(),
            arrival: 0,
            departure: 0,
            focused: None,
            faded: None,
            shifted: std::time::Instant::now(),
        }
    }

    pub(crate) fn strip(&mut self) {
        self.titled = false;
    }

    pub(crate) fn tab(&self) -> SideTab {
        self.tab
    }

    pub(crate) fn show(&mut self, tab: SideTab, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            self.forget_verse();
            self.anchor_verse();
        }
        self.anchor = true;
        cx.notify();
    }

    pub(crate) fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.track_menu.reset(cx);
        self.context_menu = None;
        cx.notify();
    }

    fn sharpen_progress(&self, window: &mut Window) -> f32 {
        let span = Motion::Quick.span().as_secs_f32().max(f32::EPSILON);
        let progress = (self.since.elapsed().as_secs_f32() / span).clamp(0., 1.);
        if progress < 1. {
            window.request_animation_frame();
        }
        ease_in_out(progress)
    }

    fn forget_verse(&mut self) {
        self.previous_active_line = None;
        self.departing_line = None;
        self.focused = None;
        self.faded = None;
        self.placing = true;
    }

    fn drift_progress(&self, window: &mut Window) -> f32 {
        let span = Motion::Base.span().as_secs_f32().max(f32::EPSILON);
        let progress = (self.shifted.elapsed().as_secs_f32() / span).clamp(0., 1.);
        if progress < 1. {
            window.request_animation_frame();
        }
        ease_in_out(progress)
    }

    fn set_hovered(&mut self, index: usize, over: bool, cx: &mut Context<Self>) {
        if !over {
            if self.over == Some(index) {
                self.over = None;
            }
            if self.hovered == Some(index) {
                self.hovered = None;
                self.fading = Some(index);
                self.since = std::time::Instant::now();
                self.linger = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Motion::Quick.span()).await;
                    this.update(cx, |this, cx| {
                        if this.fading != Some(index) {
                            return;
                        }
                        this.fading = None;
                        cx.notify();
                    })
                    .ok();
                }));
                cx.notify();
            }
            return;
        }

        self.over = Some(index);
        if self.hovered == Some(index) {
            return;
        }
        self.fading = None;
        self.linger = Some(cx.spawn(async move |this, cx| {
            this.update(cx, |this, cx| {
                if this.over != Some(index) {
                    return;
                }
                this.hovered = Some(index);
                cx.notify();
            })
            .ok();
        }));
    }

    fn dismiss_menu(&mut self, cx: &mut Context<Self>) {
        self.track_menu.reset(cx);
        self.context_menu = None;
        cx.notify();
    }

    fn row(
        track: Track,
        index: usize,
        position: QueuePosition,
        queue_revision: u64,
        look: RowLook,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let RowLook { playing, drop_line } = look;
        let theme = *cx.theme();
        let past_index = position.past();
        let queue_index = position.upcoming();
        let similar_index = position.similar();
        let title = match position {
            QueuePosition::Past(_) => theme.muted_foreground,
            QueuePosition::Current => theme.primary,
            QueuePosition::Upcoming(_) | QueuePosition::Similar(_) => theme.foreground,
        };
        let pin = track
            .id
            .clone()
            .map(|id| Pin::new(PinKind::Song, id, track.name.clone()).cover(track.cover.clone()));
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
        .underline()
        .when(track.explicit, Card::explicit)
        .play(
            playing,
            cx.listener(move |this, _, _, cx| {
                let stale = this.queue.read(cx).revision() != queue_revision;
                this.playback.update(cx, |playback, cx| match position {
                    QueuePosition::Current => playback.toggle_play(cx),
                    QueuePosition::Past(index) if !stale => playback.play_past(index, cx),
                    QueuePosition::Upcoming(index) if !stale => playback.play_upcoming(index, cx),
                    QueuePosition::Similar(index) if !stale => playback.play_similar(index, cx),
                    _ => {}
                });
            }),
        )
        .menu(cx.listener(move |this, event: &MouseDownEvent, _, cx| {
            this.track_menu.reset(cx);
            this.context_menu = Some(ContextMenuState {
                track: menu_track.clone(),
                revision: queue_revision,
                position: event.position,
            });
            cx.notify();
        }))
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
                    .mr_1()
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
                cx.listener(move |this, event: &DragMoveEvent<DraggedPin>, _, cx| {
                    let Some(gap) = drop_gap(event.bounds, event.event.position, target) else {
                        return;
                    };
                    let Some(held) = event.drag(cx).spot(QUEUE) else {
                        return;
                    };
                    let gap = (gap != held.index && gap != held.index + 1).then_some(gap);
                    if this.drop_gap != gap {
                        this.drop_gap = gap;
                        cx.notify();
                    }
                }),
            )
            .on_drop(cx.listener(move |this, dragged: &DraggedPin, _, cx| {
                let Some(held) = dragged.spot(QUEUE) else {
                    return;
                };
                if let Some(gap) = this.drop_gap.take() {
                    this.queue.update(cx, |queue, cx| {
                        if queue.revision() == held.revision {
                            queue.move_upcoming_to_gap(held.index, gap, cx);
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
                    .mr_1()
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
        .when_some(pin, |this, pin| match queue_index {
            Some(index) => this.pin_from(pin, Spot::new(QUEUE, index).revision(queue_revision)),
            None => this.pin(pin),
        });

        div()
            .id(("queue-track-container", index))
            .relative()
            .min_w_0()
            .child(card)
            .when_some(drop_line, |this, edge| this.child(drop_marker(edge, cx)))
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
            .when(self.titled, |this| {
                this.border_b_1().border_color(theme.border).child(eyebrow(
                    match self.tab {
                        SideTab::Queue => t!("queue-title"),
                        SideTab::Lyrics => t!("lyrics-title"),
                    },
                    cx,
                ))
            })
            .when(!self.titled, |this| {
                this.justify_end().pr(theme.metrics.control + px(8.))
            })
            .when(self.tab == SideTab::Queue, |this| {
                this.child(
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
            })
    }

    fn follow(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let theme = *cx.theme();
        if self.tab != SideTab::Lyrics || self.pinned {
            return None;
        }

        Some(
            div()
                .absolute()
                .when_else(self.titled, |this| this.bottom_3(), |this| this.bottom_16())
                .w_full()
                .flex()
                .justify_center()
                .child(
                    div().flex().flex_none().block_mouse_except_scroll().child(
                        Button::new("resume-pin")
                            .ghost()
                            .small()
                            .icon("icons/undo-2.svg")
                            .tooltip("lyrics-follow")
                            .rounded_full()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.popover)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.anchor_verse();
                                cx.notify();
                            })),
                    ),
                ),
        )
    }

    fn verses(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let position = self.playback.read(cx).live_position();
        let singing = matches!(self.playback.read(cx).state(), PlaybackState::Playing);
        let lyrics = self.lyrics.read(cx);
        let state = lyrics.state().clone();
        let shown = lyrics.current().map(|hit| hit.lyrics.clone());
        let credit = lyrics
            .current()
            .map(|hit| (hit.source, hit.writers.clone()));
        let following = lyrics.following().map(str::to_owned);
        let take = lyrics.revision();
        let (karaoke_lyrics, sweep, romanization_scripts) = {
            let settings = self.settings.read(cx);
            (
                settings.karaoke_lyrics(),
                settings.karaoke_sweep(),
                settings
                    .romanized_lyrics()
                    .then(|| settings.romanization_scripts()),
            )
        };
        let karaoke_effects = karaoke_lyrics && effects();
        let sung = Sung {
            karaoke: karaoke_effects,
            sweep,
            scripts: romanization_scripts,
            theme,
        };

        if self.verse_of != following {
            self.verse_of = following;
            self.verse_take = take;
            self.forget_verse();
            self.anchor_verse();
            let scroll = self.verse_bar.read(cx).scroll().clone();
            scroll.set_offset(gpui::point(scroll.offset().x, px(0.)));
            self.verse_bar
                .update(cx, |bar, _| bar.remember_offset(scroll.offset().y));
        } else if self.verse_take != take {
            self.verse_take = take;
            self.forget_verse();
            self.anchor_verse();
        }

        let empty = |key: &'static str, cx: &mut Context<Self>| {
            vacant(i18n::lookup(key, None), cx)
                .flex_1()
                .into_any_element()
        };
        let lines = match (&state, &shown) {
            (LyricsState::Ready, Some(music::Lyrics::Synced { lines })) => Some(lines.clone()),
            _ => None,
        };

        let verse = match self.titled {
            true => theme.text(Text::Large),
            false => theme.text(Text::Title),
        };
        let mut body: Vec<gpui::AnyElement> = match (&lines, &state) {
            (Some(lines), _) => {
                let active_line = sung_line(lines, position);
                let active_karaoke = active_line
                    .and_then(|index| lines.get(index))
                    .is_some_and(|line| line.worded());
                if singing && karaoke_effects && active_karaoke {
                    window.request_animation_frame();
                }
                if self.previous_active_line != active_line {
                    if self.previous_active_line.is_some() {
                        self.departing_line = self.previous_active_line;
                        self.departure = self.departure.wrapping_add(1);
                        self.departed = std::time::Instant::now();
                    }
                    if active_line.is_some() {
                        self.arrival = self.arrival.wrapping_add(1);
                    }
                    self.previous_active_line = active_line;
                }
                if self.departing_line.is_some() && self.departed.elapsed() >= Motion::Quick.span()
                {
                    self.departing_line = None;
                }
                let instrumental_line = active_instrumental(lines, position);
                let focus_line = active_line
                    .or(instrumental_line)
                    .or(self.focused)
                    .unwrap_or(0);
                if self.focused != Some(focus_line) {
                    self.faded = self.focused;
                    self.focused = Some(focus_line);
                    self.shifted = std::time::Instant::now();
                }
                let animations = ui::motion::animates(cx);
                let blur = match effects() {
                    true => verse * BLUR,
                    false => px(0.),
                };
                let sharpen = self.sharpen_progress(window);
                let drift = match self.faded.is_some() && animations && blur > px(0.) {
                    true => self.drift_progress(window),
                    false => 1.,
                };
                let mut rendered = Vec::with_capacity(lyric_row_count(lines));

                for (index, line) in lines.iter().enumerate() {
                    let seek = line.start;
                    let gap = instrumental_gap_before(lines, index);
                    let instrumental_start = line.start.saturating_sub(gap);
                    let has_instrumental = gap >= INSTRUMENTAL_BREAK;
                    let instrumental_progress = if has_instrumental {
                        progress_between(instrumental_start, line.start, position)
                    } else {
                        0.
                    };
                    let instrumental_has_passed = position >= line.start;

                    let near = index == focus_line || index == focus_line + 1;
                    let hazed = !near;
                    let waking = self.hovered == Some(index);
                    let settling = self.fading == Some(index);
                    let haze = |hazed: bool| match (hazed, waking, settling) {
                        (false, _, _) => 0.,
                        (true, true, _) => 1. - sharpen,
                        (true, false, true) => sharpen,
                        (true, false, false) => 1.,
                    };
                    let was_near = self
                        .faded
                        .is_some_and(|focus| index == focus || index == focus + 1);
                    let from = haze(!was_near);
                    let to = haze(hazed);
                    let softness = from + (to - from) * drift;

                    if has_instrumental {
                        if singing && instrumental_line == Some(index) {
                            window.request_animation_frame();
                        }
                        let notes = instrumental_row(
                            instrumental_progress,
                            instrumental_has_passed,
                            verse,
                            &theme,
                        )
                        .id(("instrumental", index))
                        .px_2()
                        .rounded(theme.radius)
                        .cursor_pointer()
                        .hover(|style| style.bg(theme.table_hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.seek_lyrics(instrumental_start, cx);
                        }))
                        .when(softness > 0., |this| this.blur(blur * softness));
                        rendered.push(notes.into_any_element());
                    }

                    let text = SharedString::from(line.text.clone());
                    let karaoke = Some(index) == active_line && line.worded() && karaoke_effects;
                    let primary_karaoke_capable = karaoke_effects
                        && line.words.as_ref().is_some_and(|words| !words.is_empty());
                    let karaoke_prepared = karaoke_effects
                        && prepares_karaoke_line(index, focus_line, self.departing_line);
                    let primary_karaoke = karaoke && primary_karaoke_capable;
                    let line_has_ended = line_has_passed(line, position);
                    let tint = match (Some(index) == active_line, line_has_ended) {
                        (true, _) if primary_karaoke => theme.muted_foreground,
                        (true, _) => theme.foreground,
                        (false, true) => theme.muted_foreground.opacity(PAST),
                        (false, false) => theme.muted_foreground,
                    };

                    let dimming = (animations && Some(index) == self.departing_line)
                        .then_some(self.departure);

                    let primary = match (
                        primary_karaoke_capable && karaoke_prepared,
                        line.words.as_ref(),
                    ) {
                        (true, Some(words)) => karaoke_lane(
                            &line.text,
                            line.start,
                            words,
                            position,
                            verse,
                            primary_karaoke,
                            line.voice,
                            sung,
                        )
                        .into_any_element(),
                        _ => div().child(text).into_any_element(),
                    };
                    let content = div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .when(!line.voice.lead(), |this| this.items_end().text_right())
                        .child(primary)
                        .when_some(
                            selected_romanization(&line.romanized, romanization_scripts),
                            |this, text| this.child(romanized_lyrics_lane(text, &theme)),
                        )
                        .children(line.secondary.iter().map(|lane| {
                            let lit_at_end = line.sung_end().is_some_and(|end| {
                                lane.start <= end && lane.sung_end().is_none_or(|sung| sung >= end)
                            });
                            secondary_lyrics_lane(
                                lane,
                                Some(index) == active_line,
                                line_has_ended,
                                position,
                                dimming.filter(|_| lit_at_end),
                                line.voice,
                                Sung {
                                    karaoke: karaoke_prepared,
                                    ..sung
                                },
                            )
                        }));

                    let traded = index
                        .checked_sub(1)
                        .is_some_and(|previous| lines[previous].voice != line.voice);
                    let verse_line = div()
                        .id(("verse", index))
                        .px_2()
                        .py_1()
                        .when(traded, |this| this.pt_3())
                        .rounded(theme.radius)
                        .cursor_pointer()
                        .hover(|style| style.bg(theme.table_hover))
                        .text_size(verse)
                        .text_color(tint)
                        .when(primary_karaoke_capable, |this| {
                            this.font_weight(FontWeight(KARAOKE_WEIGHT))
                        })
                        .when(
                            Some(index) == active_line && !primary_karaoke_capable,
                            |this| this.font_weight(FontWeight::SEMIBOLD),
                        )
                        .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                            this.set_hovered(index, *over, cx)
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.seek_lyrics(seek, cx);
                        }))
                        .child(content);

                    let verse_line =
                        verse_line.when(softness > 0., |this| this.blur(blur * softness));
                    let active = Some(index) == active_line;
                    let growing = animations && active;
                    let departing = dimming.is_some();
                    let active_size = active_verse_size(verse);
                    let unsung = theme.muted_foreground;
                    let lit = theme.foreground;
                    let verse_line = match (growing, departing) {
                        (true, _) => verse_line
                            .with_animation(
                                ("verse-activate", self.arrival as usize),
                                Animation::new(Motion::Base.span()).with_easing(ease_out_quad),
                                move |this, t| {
                                    let this = this.text_size(verse + ACTIVE_VERSE_GROWTH * t);
                                    match karaoke {
                                        true => this,
                                        false => this.text_color(mix(unsung, lit, t)),
                                    }
                                },
                            )
                            .into_any_element(),
                        (_, true) => verse_line
                            .with_animation(
                                ("verse-deactivate", self.departure as usize),
                                Animation::new(Motion::Base.span()).with_easing(ease_out_quad),
                                move |this, t| {
                                    this.text_size(active_size - ACTIVE_VERSE_GROWTH * t)
                                        .text_color(mix(lit, tint, t))
                                },
                            )
                            .into_any_element(),
                        _ if active => verse_line.text_size(active_size).into_any_element(),
                        _ => verse_line.into_any_element(),
                    };
                    rendered.push(verse_line);
                }

                rendered
            }
            (None, LyricsState::Ready) => match &shown {
                Some(music::Lyrics::Plain { text, romanized }) => vec![
                    div()
                        .px_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_size(theme.text(Text::Body))
                        .text_color(theme.muted_foreground)
                        .child(SharedString::from(text.clone()))
                        .when_some(
                            selected_romanization(romanized, romanization_scripts),
                            |this, text| this.child(romanized_lyrics_lane(text, &theme)),
                        )
                        .into_any_element(),
                ],
                _ => vec![wordless("lyrics-missing", "icons/mic-off.svg", cx)],
            },
            (None, LyricsState::Idle) => vec![empty("lyrics-idle", cx)],
            (None, LyricsState::Loading) => vec![empty("lyrics-loading", cx)],
            (None, LyricsState::Instrumental) => {
                vec![wordless("lyrics-instrumental", "icons/guitar.svg", cx)]
            }
            (None, LyricsState::Missing) => {
                vec![wordless("lyrics-missing", "icons/mic-off.svg", cx)]
            }
            (None, LyricsState::Failed(_)) => vec![empty("lyrics-failed", cx)],
        };

        if state == LyricsState::Ready
            && let Some((source, writers)) = &credit
        {
            body.push(
                div()
                    .px_2()
                    .pt_2()
                    .flex()
                    .flex_col()
                    .text_size(theme.text(Text::Small))
                    .text_color(theme.muted_foreground)
                    .child(t!("lyrics-source", source = *source))
                    .when(!writers.is_empty(), |this| {
                        let writers = writers.join(", ");
                        this.child(t!("lyrics-writers", writers = writers.as_str()))
                    })
                    .into_any_element(),
            );
        }

        if let Some(lines) = &lines {
            let focus = active_lyrics_row(lines, position);
            self.pin_verse(focus, window, cx);
        }

        let (over, under) = match &lines {
            Some(lines) => self.verse_slack(lyric_row_count(lines), window, cx),
            None => (px(REST), px(REST)),
        };

        Scroller::new("lyrics", &self.verse_bar)
            .flex()
            .flex_col()
            .gap_1()
            .flex_1()
            .min_h_0()
            .px_1()
            .pt(over)
            .pb(under)
            .when(effects(), |this| this.fade_edges(px(FADE), px(FADE)))
            .children(body)
    }

    fn verse_slack(&self, count: usize, window: &Window, cx: &App) -> (Pixels, Pixels) {
        let scroll = self.verse_bar.read(cx).scroll().clone();
        let view = scroll.bounds().size.height;
        if view <= px(0.) {
            window.request_animation_frame();
            return (px(REST), px(REST));
        }
        let tail = count
            .checked_sub(1)
            .and_then(|last| scroll.bounds_for_item(last))
            .map_or(px(0.), |item| item.size.height);

        (
            snapped((view * PIN).max(px(REST)), window),
            snapped((view * (1. - PIN) - tail).max(px(REST)), window),
        )
    }

    fn anchor_verse(&mut self) {
        self.pinned = true;
        self.aiming = false;
        self.rested = None;
        self.followed = None;
        self.goal = None;
        self.nudged = None;
    }

    fn seek_lyrics(&mut self, position: std::time::Duration, cx: &mut Context<Self>) {
        self.anchor_verse();
        self.playback
            .update(cx, |playback, cx| playback.seek(position, cx));
        cx.notify();
    }

    fn pin_verse(&mut self, sung: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let scroll = self.verse_bar.read(cx).scroll().clone();
        let aimed = self.verse_bar.read(cx).goal();
        let resting = scroll.offset().y;
        if let Some(goal) = self.goal
            && (aimed - goal).abs() > px(1.)
        {
            self.pinned = false;
            self.goal = None;
            self.nudged = Some(std::time::Instant::now());
        }
        if !self.pinned {
            self.followed = sung;
            // Keep the reader in charge for as long as they keep moving: the timer counts from the
            // last scroll, not from the first one.
            if self.rested != Some(resting) {
                self.rested = Some(resting);
                self.nudged = Some(std::time::Instant::now());
            }
            if self.nudged.is_some_and(|at| at.elapsed() >= SETTLE) {
                self.anchor_verse();
            } else {
                return;
            }
        }
        if sung.is_none() {
            return;
        }
        // The rows a verse sits among change on the very frame it starts being sung, and their
        // bounds only settle once that frame has been laid out. Aim on the next one.
        if self.followed != sung {
            self.followed = sung;
            self.aiming = true;
            window.request_animation_frame();
            return;
        }
        if !self.aiming {
            return;
        }
        let Some(item) = sung.and_then(|index| scroll.bounds_for_item(index)) else {
            return;
        };
        self.aiming = false;
        let view = scroll.bounds();
        let goal = anchored_lyrics_offset(
            view.origin.y,
            item.origin.y,
            view.size.height,
            scroll.max_offset().y,
        );
        match std::mem::take(&mut self.placing) {
            true => self.verse_bar.update(cx, |bar, _| bar.place(goal)),
            false => self.verse_bar.update(cx, |bar, _| bar.aim(goal, window)),
        }
        self.goal = Some(self.verse_bar.read(cx).goal());
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
        let audible = matches!(self.playback.read(cx).state(), PlaybackState::Playing);

        uniform_list(
            "queue-rows",
            sections.len() + TAIL_ROWS,
            cx.processor(move |_, range: Range<usize>, window, cx| {
                let (revision, slots) = {
                    let queue = queue.read(cx);
                    let slots = range
                        .clone()
                        .map(|index| {
                            let slot = (index < sections.len()).then(|| sections.slot(index));
                            let found = match slot {
                                Some(Slot::Track(position)) => track(queue, position),
                                Some(Slot::Header(_)) | None => None,
                            };
                            (index, slot, found)
                        })
                        .collect::<Vec<_>>();
                    (queue.revision(), slots)
                };

                slots
                    .into_iter()
                    .map(|(index, slot, found)| match (slot, found) {
                        (None, _) => div().into_any_element(),
                        (Some(Slot::Header(key)), _) => {
                            section_label(key, window, cx).into_any_element()
                        }
                        (Some(Slot::Track(position)), Some(found)) => {
                            let drop_line = match (position.upcoming(), drop_gap) {
                                (Some(queued), Some(gap)) if gap == queued => Some(Edge::Above),
                                (Some(queued), Some(gap))
                                    if gap == upcoming && queued + 1 == upcoming =>
                                {
                                    Some(Edge::Below)
                                }
                                _ => None,
                            };
                            let playing = audible && position == QueuePosition::Current;
                            let look = RowLook { playing, drop_line };
                            Self::row(found, index, position, revision, look, cx).into_any_element()
                        }
                        (Some(Slot::Track(_)), None) => div().into_any_element(),
                    })
                    .collect()
            }),
        )
    }
}

impl Render for Aside {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.scrollbar.read(cx).sync();
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
        if self.anchor && self.tab == SideTab::Queue {
            self.pin(sections, window, cx);
        }

        div()
            .id("aside")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .on_drag_move(cx.listener(|this, _: &DragMoveEvent<DraggedPin>, _, cx| {
                if this.drop_gap.take().is_some() {
                    cx.notify();
                }
            }))
            .child(self.header(sections, cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .when(self.tab == SideTab::Lyrics, |this| {
                        this.child(self.verses(window, cx))
                    })
                    .when(self.tab == SideTab::Queue && empty, |this| {
                        this.child(vacant(t!("queue-empty"), cx).flex_1())
                    })
                    .when(self.tab == SideTab::Queue && !empty, |this| {
                        let gliding = self.scrollbar.clone();

                        this.child(
                            div()
                                .relative()
                                .flex_1()
                                .min_h_0()
                                .child(
                                    div()
                                        .size_full()
                                        .when(effects(), |this| {
                                            this.fade_edges(px(FADE * 0.5), px(FADE))
                                        })
                                        .child(
                                            self.rows(sections, cx)
                                                .px_2()
                                                .pt(px(FADE * 0.5))
                                                .track_scroll(&self.scroll)
                                                .size_full()
                                                .on_scroll_wheel(
                                                    move |event: &ScrollWheelEvent, window, cx| {
                                                        if event.delta.precise() {
                                                            return;
                                                        }
                                                        gliding
                                                            .update(cx, |bar, _| bar.nudge(window));
                                                    },
                                                ),
                                        ),
                                )
                                .child(self.scrollbar.clone()),
                        )
                    })
                    .children(self.follow(cx)),
            )
            .children(self.menu(cx))
    }
}

fn karaoke_lane(
    line: &str,
    line_start: std::time::Duration,
    words: &[music::LyricsWord],
    position: std::time::Duration,
    verse: Pixels,
    active: bool,
    voice: Voice,
    sung: Sung,
) -> Div {
    let theme = &sung.theme;
    let edge_fade = verse * REVEAL;
    let weight = FontWeight(KARAOKE_WEIGHT);
    let rest = px(0.);
    div()
        .flex()
        .flex_wrap()
        .text_left()
        .when(!voice.lead(), |this| this.justify_end())
        .children(karaoke_fragments(line, words).into_iter().enumerate().map(
            |(index, fragment)| {
                let text = SharedString::from(fragment);
                let (highlight_start, highlight_end) = karaoke_window(line_start, words, index);
                let tail = index + 1 >= words.len();
                let highlighted = if active {
                    swept(highlight_start, highlight_end, position, tail, sung.sweep)
                } else {
                    0.
                };
                let landing = ((1. - highlighted) / LANDING).min(1.);
                let embolden = karaoke_embolden(highlighted, verse);
                div()
                    .relative()
                    .whitespace_nowrap()
                    .font_weight(weight)
                    .msdf_text_horizontal(rest)
                    .child(text.clone())
                    .when(highlighted > 0., |this| {
                        this.child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(relative(highlighted))
                                .overflow_hidden()
                                .text_color(theme.foreground)
                                .when(highlighted < 1., |this| {
                                    this.fade_sides(px(0.), edge_fade * landing)
                                })
                                .child(
                                    div()
                                        .whitespace_nowrap()
                                        .msdf_text_horizontal(embolden)
                                        .child(text),
                                ),
                        )
                    })
            },
        ))
}

fn karaoke_embolden(progress: f32, verse: Pixels) -> Pixels {
    verse * KARAOKE_EMBOLDEN_SHARE * progress.clamp(0., 1.)
}

fn active_verse_size(verse: Pixels) -> Pixels {
    verse + ACTIVE_VERSE_GROWTH
}

fn prepares_karaoke_line(index: usize, focus: usize, departing: Option<usize>) -> bool {
    index == focus || index == focus.saturating_add(1) || departing == Some(index)
}

fn secondary_lyrics_lane(
    lane: &music::LyricsLane,
    line_active: bool,
    line_passed: bool,
    position: std::time::Duration,
    dimming: Option<u64>,
    voice: Voice,
    sung: Sung,
) -> gpui::AnyElement {
    let theme = &sung.theme;
    let active =
        line_active && position >= lane.start && lane.sung_end().is_none_or(|end| position < end);
    let passed = line_passed || lane.sung_end().is_some_and(|end| position >= end);
    let karaoke = active && sung.karaoke && lane.worded();
    let tint = match (active, passed, karaoke) {
        (_, _, true) => theme.muted_foreground,
        (true, _, false) => theme.foreground,
        (false, true, false) => theme.muted_foreground.opacity(PAST),
        (false, false, false) => theme.muted_foreground,
    };
    let size = theme.text(Text::Body);
    let karaoke_capable = sung.karaoke && lane.worded();
    let lyrics = div()
        .text_size(size)
        .map(|this| match (karaoke_capable, lane.words.as_ref()) {
            (true, Some(words)) => this.child(karaoke_lane(
                &lane.text, lane.start, words, position, size, karaoke, voice, sung,
            )),
            _ => this.child(SharedString::from(lane.text.clone())),
        });
    let lit = theme.foreground;
    let lyrics = match dimming {
        Some(departure) => lyrics
            .motion(("lane-dim", departure as usize), Motion::Quick, {
                move |this, t| this.text_color(mix(lit, tint, t))
            })
            .into_any_element(),
        None => lyrics.text_color(tint).into_any_element(),
    };
    div()
        .flex()
        .flex_col()
        .when(!voice.lead(), |this| this.items_end().text_right())
        .child(lyrics)
        .when_some(
            selected_romanization(&lane.romanized, sung.scripts),
            |this, text| this.child(romanized_lyrics_lane(text, theme)),
        )
        .into_any_element()
}

fn selected_romanization(
    romanized: &Option<music::RomanizedText>,
    scripts: Option<RomanizationScripts>,
) -> Option<String> {
    let romanized = romanized.as_ref()?;
    scripts?
        .contains(romanized.writing_system)
        .then(|| romanized.text.clone())
}

fn romanized_lyrics_lane(text: String, theme: &ui::Theme) -> Div {
    div()
        .text_size(theme.text(Text::Body))
        .text_color(theme.muted_foreground)
        .child(SharedString::from(text))
}

fn karaoke_window(
    line_start: std::time::Duration,
    words: &[music::LyricsWord],
    index: usize,
) -> (std::time::Duration, std::time::Duration) {
    let word = &words[index];
    let start = match index {
        0 => line_start.min(word.start),
        _ => word.start,
    };
    let end = words
        .get(index + 1)
        .map(|next| next.start.max(start))
        .unwrap_or_else(|| word.end.max(start));
    (start, end)
}

fn karaoke_fragments(line: &str, words: &[music::LyricsWord]) -> Vec<String> {
    let mut starts = Vec::with_capacity(words.len());
    let mut cursor = 0;
    for word in words {
        if word.text.is_empty() {
            return spaced_words(words);
        }
        let Some(remainder) = line.get(cursor..) else {
            return spaced_words(words);
        };
        let Some(relative) = remainder.find(&word.text) else {
            return spaced_words(words);
        };
        let start = cursor + relative;
        starts.push(start);
        cursor = start + word.text.len();
    }

    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let start = match index {
                0 => 0,
                _ => *start,
            };
            let end = starts.get(index + 1).copied().unwrap_or(line.len());
            line[start..end].to_owned()
        })
        .collect()
}

fn spaced_words(words: &[music::LyricsWord]) -> Vec<String> {
    words
        .iter()
        .enumerate()
        .map(|(index, word)| {
            let mut text = word.text.clone();
            if words
                .get(index + 1)
                .is_some_and(|next| needs_space(&word.text, &next.text))
            {
                text.push(' ');
            }
            text
        })
        .collect()
}

fn needs_space(left: &str, right: &str) -> bool {
    let Some(last) = left.chars().next_back() else {
        return false;
    };
    let Some(first) = right.chars().next() else {
        return false;
    };
    if last.is_whitespace() || first.is_whitespace() {
        return false;
    }
    !matches!(last, '(' | '[' | '{' | '\'' | '’' | '-' | '—')
        && !matches!(
            first,
            ')' | ']' | '}' | ',' | '.' | '!' | '?' | ';' | ':' | '%' | '\'' | '’' | '-' | '—'
        )
}

fn anchored_lyrics_offset(view: Pixels, item: Pixels, height: Pixels, reach: Pixels) -> Pixels {
    let delta = view - item + height * PIN;
    delta.clamp(-reach, px(0.))
}

fn swept(
    start: std::time::Duration,
    end: std::time::Duration,
    position: std::time::Duration,
    tail: bool,
    sweep: Sweep,
) -> f32 {
    let span = end.saturating_sub(start);
    let least = match sweep {
        Sweep::Steady => std::time::Duration::ZERO,
        _ => SWEEP_LEAST,
    };
    let travel = match tail {
        true => span.max(least),
        false => span.mul_f32(sweep.stretch()).max(least),
    };
    let eased = sweep.ease(progress_between(start, start + travel, position));
    match eased >= SWEPT {
        true => 1.,
        false => eased,
    }
}

fn progress_between(
    start: std::time::Duration,
    end: std::time::Duration,
    position: std::time::Duration,
) -> f32 {
    if position < start {
        return 0.;
    }
    if position >= end {
        return 1.;
    }
    let span = (end - start).as_secs_f32();
    ((position - start).as_secs_f32() / span).clamp(0., 1.)
}

fn instrumental_gap_before(lines: &[music::LyricsLine], index: usize) -> std::time::Duration {
    let start = lines[index].start;
    match index {
        0 => start,
        _ => {
            let previous = &lines[index - 1];
            start.saturating_sub(previous.sung_end().unwrap_or(previous.start))
        }
    }
}

fn active_instrumental(
    lines: &[music::LyricsLine],
    position: std::time::Duration,
) -> Option<usize> {
    let next_line = lines.iter().position(|line| line.start > position)?;
    let gap = instrumental_gap_before(lines, next_line);
    let start = lines[next_line].start.saturating_sub(gap);
    (gap >= INSTRUMENTAL_BREAK && position >= start).then_some(next_line)
}

fn lyric_row_count(lines: &[music::LyricsLine]) -> usize {
    lines.len()
        + (0..lines.len())
            .filter(|index| instrumental_gap_before(lines, *index) >= INSTRUMENTAL_BREAK)
            .count()
}

fn line_row(lines: &[music::LyricsLine], index: usize) -> usize {
    index
        + (0..=index)
            .filter(|line| instrumental_gap_before(lines, *line) >= INSTRUMENTAL_BREAK)
            .count()
}

fn active_lyrics_row(lines: &[music::LyricsLine], position: std::time::Duration) -> Option<usize> {
    if let Some(index) = sung_line(lines, position) {
        return Some(line_row(lines, index));
    }
    let index = active_instrumental(lines, position)?;
    line_row(lines, index).checked_sub(1)
}

fn sung_line(lines: &[music::LyricsLine], position: std::time::Duration) -> Option<usize> {
    match active_instrumental(lines, position) {
        Some(_) => None,
        None => music::lyrics::active(lines, position),
    }
}

fn line_has_passed(line: &music::LyricsLine, position: std::time::Duration) -> bool {
    line.sung_end().is_some_and(|end| position >= end)
}

fn instrumental_row(progress: f32, past: bool, verse: Pixels, theme: &ui::Theme) -> Div {
    let note_size = verse * 1.;
    div()
        .flex()
        .items_center()
        .gap_2()
        .py(verse * 0.45)
        .children((0..3).map(|index| {
            let note_progress = (progress * 3. - index as f32).clamp(0., 1.);
            let tint = match past {
                true => theme.muted_foreground.opacity(PAST),
                false => mix(theme.muted_foreground, theme.primary, note_progress),
            };
            div()
                .size(note_size)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path("icons/music-2.svg")
                        .size(note_size)
                        .text_color(tint),
                )
        }))
}

fn wordless(key: &'static str, icon: &'static str, cx: &App) -> gpui::AnyElement {
    let theme = *cx.theme();

    div()
        .flex()
        .flex_1()
        .flex_col()
        .items_center()
        .justify_center()
        .child(
            svg()
                .path(icon)
                .size(theme.metrics.cover * GLYPH_SIZE)
                .text_color(theme.muted_foreground.opacity(GLYPH)),
        )
        .child(vacant(i18n::lookup(key, None), cx))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use music::{LyricsLine, LyricsWord, Voice};

    use super::{
        QueuePosition, Sections, Slot, active_lyrics_row, active_verse_size,
        anchored_lyrics_offset, karaoke_embolden, karaoke_fragments, karaoke_window,
        line_has_passed, line_row, lyric_row_count, prepares_karaoke_line,
    };
    use gpui::px;

    fn slots(sections: Sections) -> Vec<Slot> {
        (0..sections.len()).map(|i| sections.slot(i)).collect()
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

    #[test]
    fn a_long_instrumental_pause_gets_its_own_lyrics_row() {
        let lines = [
            LyricsLine {
                start: Duration::from_secs(2),
                end: Some(Duration::from_secs(5)),
                text: "first".to_owned(),
                romanized: None,
                words: None,
                secondary: Vec::new(),
                voice: Voice::Lead,
            },
            LyricsLine {
                start: Duration::from_secs(12),
                end: Some(Duration::from_secs(15)),
                text: "second".to_owned(),
                romanized: None,
                words: None,
                secondary: Vec::new(),
                voice: Voice::Lead,
            },
        ];

        assert_eq!(lyric_row_count(&lines), 3);
        assert_eq!(line_row(&lines, 0), 0);
        assert_eq!(line_row(&lines, 1), 2);
        assert_eq!(active_lyrics_row(&lines, Duration::from_secs(8)), Some(1));
        assert_eq!(active_lyrics_row(&lines, Duration::from_secs(13)), Some(2));
    }

    #[test]
    fn word_timing_exposes_a_pause_hidden_by_the_line_end() {
        let lines = [
            LyricsLine {
                start: Duration::from_secs(2),
                end: Some(Duration::from_secs(12)),
                text: "first".to_owned(),
                romanized: None,
                words: Some(vec![LyricsWord {
                    start: Duration::from_secs(2),
                    end: Duration::from_secs(5),
                    text: "first".to_owned(),
                }]),
                secondary: Vec::new(),
                voice: Voice::Lead,
            },
            LyricsLine {
                start: Duration::from_secs(12),
                end: Some(Duration::from_secs(15)),
                text: "second".to_owned(),
                romanized: None,
                words: None,
                secondary: Vec::new(),
                voice: Voice::Lead,
            },
        ];

        assert_eq!(lyric_row_count(&lines), 3);
        assert_eq!(active_lyrics_row(&lines, Duration::from_secs(8)), Some(1));
        assert!(line_has_passed(&lines[0], Duration::from_secs(8)));
    }

    #[test]
    fn lyrics_follow_uses_unscrolled_item_bounds() {
        let offset = anchored_lyrics_offset(px(0.), px(200.), px(100.), px(500.));

        assert_eq!(offset, px(-170.));
    }

    #[test]
    fn karaoke_uses_spacing_from_the_complete_line() {
        let text = "I said oooh I'm drowning in the night";
        let words = ["I", "said", "oooh", "I'm", "drowning", "in", "the", "night"]
            .into_iter()
            .enumerate()
            .map(|(index, text)| LyricsWord {
                start: Duration::from_millis(index as u64 * 100),
                end: Duration::from_millis(index as u64 * 100 + 100),
                text: text.to_owned(),
            })
            .collect::<Vec<_>>();

        let fragments = karaoke_fragments(text, &words);

        assert_eq!(fragments.concat(), text);
        assert_eq!(
            fragments,
            [
                "I ",
                "said ",
                "oooh ",
                "I'm ",
                "drowning ",
                "in ",
                "the ",
                "night"
            ]
        );
    }

    #[test]
    fn karaoke_embolden_progress_is_bounded() {
        let verse = px(20.);
        assert_eq!(karaoke_embolden(-1., verse), px(0.));
        assert!((karaoke_embolden(0.5, verse) - px(0.18)).abs() < px(0.001));
        assert!((karaoke_embolden(2., verse) - px(0.36)).abs() < px(0.001));
    }

    #[test]
    fn karaoke_prepares_only_focus_next_and_departing_lines() {
        assert!(prepares_karaoke_line(4, 4, Some(2)));
        assert!(prepares_karaoke_line(5, 4, Some(2)));
        assert!(prepares_karaoke_line(2, 4, Some(2)));
        assert!(!prepares_karaoke_line(3, 4, Some(2)));
        assert!(!prepares_karaoke_line(6, 4, Some(2)));
    }

    #[test]
    fn active_verse_size_adds_two_pixels() {
        assert_eq!(active_verse_size(px(20.)), px(22.));
    }

    #[test]
    fn a_late_first_word_uses_the_whole_lead_in() {
        let words = vec![
            LyricsWord {
                start: Duration::from_millis(1500),
                end: Duration::from_millis(1900),
                text: "first".to_owned(),
            },
            LyricsWord {
                start: Duration::from_millis(2000),
                end: Duration::from_millis(2400),
                text: "second".to_owned(),
            },
        ];

        assert_eq!(
            karaoke_window(Duration::from_millis(1000), &words, 0),
            (Duration::from_millis(1000), Duration::from_millis(2000))
        );
    }

    #[test]
    fn a_finished_line_stays_past_during_a_gap() {
        let line = LyricsLine {
            start: Duration::from_secs(2),
            end: Some(Duration::from_secs(5)),
            text: "line".to_owned(),
            romanized: None,
            words: None,
            secondary: Vec::new(),
            voice: Voice::Lead,
        };

        assert!(line_has_passed(&line, Duration::from_secs(8)));
    }
}
