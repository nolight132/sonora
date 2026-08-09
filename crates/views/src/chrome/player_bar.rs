// SPDX-License-Identifier: GPL-3.0-or-later

use router::{Destination, Link, navigate};
use std::time::Duration;
use ui::ActiveTheme as _;

use gpui::prelude::*;
use gpui::{
    Context, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    Render, ScrollHandle, SharedString,
};
use gpui::{Window, div, px};
use i18n::t;
use input::ToggleFullscreen;
use state::{Library, Playback, PlaybackState, Queue, Repeat, Sonora};
use ui::{
    Artwork, Button, InlineLink, InlineLinks, Popup, Room, Scrollbar, Scrubber, ScrubberState,
    clock,
};

use crate::chrome::SidebarRight;
use crate::shared::menu::ItemMenu;

const SEEK_MAX: f32 = 560.;
const VOLUME_WIDTH: f32 = 110.;
const VOLUME_TIGHT: f32 = 72.;
const CLOCK_SHORT: f32 = 3.4;
const CLOCK_LONG: f32 = 5.4;
const STEP: f32 = 0.004;

pub(crate) struct PlayerBar {
    playback: Entity<Playback>,
    queue: Entity<Queue>,
    library: Entity<Library>,
    sidebar_right: Option<Entity<SidebarRight>>,
    track_menu: ItemMenu,
    context_menu: Option<(spotify::Track, Point<Pixels>)>,
    seek: ScrubberState,
    volume: ScrubberState,
    pending: Option<f32>,
    over_seek: Option<f32>,
    over_volume: Option<f32>,
    muted: Option<f32>,
}

impl PlayerBar {
    #[allow(dead_code)]
    pub fn new(playback: Entity<Playback>, queue: Entity<Queue>, cx: &mut Context<Self>) -> Self {
        Self::build(playback, queue, None, cx)
    }

    pub(crate) fn with_sidebar_right(
        playback: Entity<Playback>,
        queue: Entity<Queue>,
        sidebar_right: Entity<SidebarRight>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::build(playback, queue, Some(sidebar_right), cx)
    }

    fn build(
        playback: Entity<Playback>,
        queue: Entity<Queue>,
        sidebar_right: Option<Entity<SidebarRight>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let library = Sonora::global(cx).library.clone();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        cx.observe(&queue, |_, _, cx| cx.notify()).detach();
        cx.observe(&library, |_, _, cx| cx.notify()).detach();
        if let Some(sidebar_right) = &sidebar_right {
            cx.observe(sidebar_right, |_, _, cx| cx.notify()).detach();
        }

        let playlist_scrollbar = cx.new(|_| {
            Scrollbar::new(ScrollHandle::new())
                .always_visible()
                .track_inset(px(4.))
        });

        Self {
            playback,
            queue,
            library,
            sidebar_right,
            track_menu: ItemMenu::new(playlist_scrollbar),
            context_menu: None,
            seek: ScrubberState::new("seek"),
            volume: ScrubberState::new("volume"),
            pending: None,
            over_seek: None,
            over_volume: None,
            muted: None,
        }
    }

    fn open_context_menu(
        &mut self,
        track: spotify::Track,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.track_menu.reset();
        self.context_menu = Some((track, position));
        cx.notify();
    }

    fn commit_seek(&mut self, cx: &mut Context<Self>) {
        let Some(fraction) = self.pending.take() else {
            return;
        };
        self.playback
            .update(cx, |playback, cx| playback.seek_fraction(fraction, cx));
    }

    fn hover(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let pad = cx.theme().metrics.pad;
        let seek = self.seek.hovered(event.position, pad);
        let volume = self.volume.hovered(event.position, pad);

        if moved(self.over_seek, seek) || moved(self.over_volume, volume) {
            self.over_seek = seek;
            self.over_volume = volume;
            cx.notify();
        }
    }

    fn transport(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(self.shuffle(cx))
            .child(self.previous(cx))
            .child(self.toggle(cx))
            .child(self.next(cx))
            .child(self.repeat(cx))
    }

    fn shuffle(&self, cx: &mut Context<Self>) -> Button {
        let theme = *cx.theme();
        let on = self.queue.read(cx).shuffle();

        Button::new("shuffle")
            .ghost()
            .small()
            .icon("icons/shuffle.svg")
            .tooltip_above("player-shuffle")
            .tint(match on {
                true => theme.primary,
                false => theme.muted_foreground,
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.queue.update(cx, |queue, cx| queue.toggle_shuffle(cx));
            }))
    }

    fn repeat(&self, cx: &mut Context<Self>) -> Button {
        let theme = *cx.theme();
        let repeat = self.playback.read(cx).repeat();

        Button::new("repeat")
            .ghost()
            .small()
            .icon(match repeat {
                Repeat::One => "icons/repeat-one.svg",
                _ => "icons/repeat.svg",
            })
            .tooltip_above(match repeat {
                Repeat::Off => "player-repeat",
                Repeat::All => "player-repeat-all",
                Repeat::One => "player-repeat-one",
            })
            .tint(match repeat {
                Repeat::Off => theme.muted_foreground,
                _ => theme.primary,
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.playback
                    .update(cx, |playback, cx| playback.cycle_repeat(cx));
            }))
    }

    fn sound(&self, width: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let empty = theme.muted_foreground.opacity(0.3);
        let level = self.playback.read(cx).volume();
        let bubble = self.over_volume.map(|at| (at, percent(at)));
        let restore = self.muted.unwrap_or(0.7);

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .child(
                Button::new("volume")
                    .ghost()
                    .small()
                    .icon(volume_icon(level))
                    .tooltip_above(match level <= 0.001 {
                        true => "player-unmute",
                        false => "player-mute",
                    })
                    .tint(theme.muted_foreground)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let wanted = match level <= 0.001 {
                            true => restore,
                            false => 0.,
                        };
                        this.muted = match wanted {
                            0. => Some(level),
                            _ => None,
                        };
                        this.playback
                            .update(cx, |playback, cx| playback.set_volume(wanted, cx));
                    })),
            )
            .child(
                div().w(width).flex_none().child(
                    Scrubber::new(&self.volume, level)
                        .colors(theme.progress_bar, empty, theme.foreground)
                        .when_some(bubble, |this, (at, text)| this.bubble(at, text))
                        .on_move(cx.listener(|this, fraction: &f32, _, cx| {
                            let level = *fraction;
                            this.muted = None;
                            this.playback
                                .update(cx, |playback, cx| playback.set_volume(level, cx));
                        })),
                ),
            )
    }

    fn previous(&self, cx: &mut Context<Self>) -> Button {
        let enabled = self.queue.read(cx).has_previous();

        Button::new("previous")
            .ghost()
            .small()
            .icon("icons/skip-back.svg")
            .tooltip_above("player-previous")
            .disabled(!enabled)
            .on_click(cx.listener(|this, _, _, cx| {
                this.playback
                    .update(cx, |playback, cx| playback.previous(cx));
            }))
    }

    fn next(&self, cx: &mut Context<Self>) -> Button {
        let enabled = self.queue.read(cx).has_next();

        Button::new("next")
            .ghost()
            .small()
            .icon("icons/skip-forward.svg")
            .tooltip_above("player-next")
            .disabled(!enabled)
            .on_click(cx.listener(|this, _, _, cx| {
                this.playback.update(cx, |playback, cx| playback.next(cx));
            }))
    }

    fn queue_button(&self, cx: &mut Context<Self>) -> Option<Button> {
        let sidebar_right = self.sidebar_right.as_ref()?;
        let open = sidebar_right.read(cx).is_open();

        Some(
            Button::new("toggle-queue")
                .ghost()
                .hoverless()
                .small()
                .icon("icons/list.svg")
                .tooltip_above("queue-title")
                .selected(open)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(sidebar_right) = &this.sidebar_right {
                        sidebar_right.update(cx, |panel, cx| panel.toggle(cx));
                    }
                })),
        )
    }

    #[allow(dead_code)]
    fn fullscreen_button(&self) -> Button {
        Button::new("toggle-fullscreen")
            .ghost()
            .small()
            .icon("icons/maximize.svg")
            .tooltip_above("player-fullscreen")
            .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleFullscreen), cx))
    }

    fn toggle(&self, cx: &mut Context<Self>) -> Button {
        let state = self.playback.read(cx).state();
        let playing = matches!(state, PlaybackState::Playing);
        let idle = matches!(state, PlaybackState::Idle | PlaybackState::Failed(_));

        let (id, icon, tooltip) = if playing {
            ("pause", "icons/pause.svg", "play-pause")
        } else {
            ("play", "icons/play.svg", "play-resume")
        };

        Button::new(id)
            .ghost()
            .small()
            .icon(icon)
            .tooltip_above(tooltip)
            .disabled(idle)
            .on_click(cx.listener(|this, _, _, cx| {
                this.playback
                    .update(cx, |playback, cx| playback.toggle_play(cx));
            }))
    }

    fn like(&self, track: Option<spotify::Track>, cx: &mut Context<Self>) -> Button {
        let theme = *cx.theme();
        let id = track.as_ref().and_then(|track| track.id.as_deref());
        let library = self.library.read(cx);
        let saved = id.is_some_and(|id| library.saved(id));

        Button::new("toggle-liked-track")
            .ghost()
            .backgroundless()
            .small()
            .icon(match saved {
                true => "icons/heart-filled.svg",
                false => "icons/heart.svg",
            })
            .tooltip_above(match saved {
                true => "menu-remove-from-library",
                false => "menu-add-to-library",
            })
            .tint(match saved {
                true => theme.primary,
                false => theme.muted_foreground,
            })
            .disabled(id.is_none())
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(track) = track.clone() {
                    this.library
                        .update(cx, |library, cx| library.toggle(track, cx));
                }
            }))
    }

    fn now_playing(&self, room: bool, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let artwork = ui::snapped(theme.metrics.row, window);
        let artists = theme.text(ui::Text::Small);
        let track = self.playback.read(cx).track().cloned();
        let cover = track.as_ref().and_then(|track| track.cover.clone());
        let like = self.like(track.clone(), cx);

        div()
            .flex()
            .items_center()
            .gap_3()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .id("now-playing-artwork")
                    .when_some(
                        track.as_ref().and_then(|track| track.album_id.clone()),
                        |this, album| this.link(Destination::Album(album.into())),
                    )
                    .child(Artwork::new(cover).size(artwork)),
            )
            .when(room, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .justify_center()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .min_w_0()
                                .items_center()
                                .gap_1()
                                .child(match &track {
                                    Some(track) => {
                                        let context_track = track.clone();
                                        div()
                                            .id("now-playing-track")
                                            .when_some(track.album_id.clone(), |this, album_id| {
                                                this.hover(|style| style.underline())
                                                    .link(Destination::Album(album_id.into()))
                                            })
                                            .on_mouse_down(
                                                MouseButton::Right,
                                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                                    window.prevent_default();
                                                    this.open_context_menu(
                                                        context_track.clone(),
                                                        event.position,
                                                        cx,
                                                    );
                                                }),
                                            )
                                            .child(SharedString::from(track.name.clone()))
                                            .min_w_0()
                                            .truncate()
                                    }
                                    None => div()
                                        .id("now-playing-album")
                                        .child(t!("player-nothing-playing"))
                                        .min_w_0()
                                        .text_color(muted)
                                        .truncate(),
                                })
                                .child(like),
                        )
                        .when_some(track.clone(), |this, track| {
                            this.child(
                                InlineLinks::new(
                                    "now-playing-artist",
                                    track.artist_refs.into_iter().map(|artist| {
                                        InlineLink::new(artist.name, artist.id.map(Into::into))
                                    }),
                                    track.artists,
                                    muted,
                                )
                                .text_size(artists)
                                .truncate()
                                .on_click(|id, cx| {
                                    navigate(Destination::Artist(id), cx);
                                }),
                            )
                        }),
                )
            })
    }
}

fn moved(before: Option<f32>, after: Option<f32>) -> bool {
    match (before, after) {
        (Some(before), Some(after)) => (before - after).abs() > STEP,
        (before, after) => before.is_some() != after.is_some(),
    }
}

fn volume_icon(level: f32) -> &'static str {
    match level {
        level if level <= 0.0001 => "icons/volume-x.svg",
        level if level < 0.25 => "icons/volume.svg",
        level if level < 0.5 => "icons/volume-1.svg",
        _ => "icons/volume-2.svg",
    }
}

fn percent(fraction: f32) -> SharedString {
    t!("player-percent", value = (fraction * 100.).round())
}

impl Render for PlayerBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let empty = muted.opacity(0.3);
        let span = Room::of(window.viewport_size().width);
        let stacked = !span.fits(Room::Roomy);
        let height = match stacked {
            true => ui::snapped(theme.metrics.player_bar + theme.metrics.pad * 3., window),
            false => ui::snapped(theme.metrics.player_bar, window),
        };
        let clock_text = theme.text(ui::Text::Tiny);

        let show_track = span.fits(Room::Snug);

        let playback = self.playback.read(cx);
        let seekable = playback.track().is_some();
        let progress = self.pending.unwrap_or_else(|| playback.progress());
        let elapsed = playback.position();
        let total = playback
            .track()
            .map(|track| track.duration)
            .unwrap_or(Duration::ZERO);
        let clock_width = clock_text
            * match total.as_secs() >= 3600 {
                true => CLOCK_LONG,
                false => CLOCK_SHORT,
            };

        let seek_bubble = self
            .over_seek
            .or(self.pending)
            .map(|at| (at, clock(total.mul_f32(at))));

        let clock_label = |value: Duration, align_end: bool| {
            div()
                .child(clock(value))
                .w(clock_width)
                .flex_none()
                .whitespace_nowrap()
                .text_size(clock_text)
                .text_color(muted)
                .when_else(align_end, |this| this.text_right(), |this| this.text_left())
        };

        let seek = div()
            .flex()
            .items_center()
            .gap_2()
            .w_full()
            .child(clock_label(elapsed, true))
            .child(
                div().flex_1().min_w_0().child(
                    Scrubber::new(&self.seek, progress)
                        .colors(theme.progress_bar, empty, theme.foreground)
                        .enabled(seekable)
                        .when_some(seek_bubble, |this, (at, text)| this.bubble(at, text))
                        .on_move(cx.listener(|this, fraction: &f32, _, cx| {
                            this.pending = Some(*fraction);
                            cx.notify();
                        }))
                        .on_release(
                            cx.listener(|this, _: &MouseUpEvent, _, cx| this.commit_seek(cx)),
                        ),
                ),
            )
            .child(clock_label(total, false))
            .into_any_element();

        let base = div()
            .flex()
            .w_full()
            .h(height)
            .flex_none()
            .px_5()
            .when(stacked, |this| this.py_2())
            .when(!theme.transparent, |this| this.bg(theme.secondary))
            .border_t_1()
            .border_color(theme.border)
            .on_mouse_move(cx.listener(Self::hover));

        let context_menu = self.context_menu.clone().map(|(track, position)| {
            Popup::new(position, self.track_menu.for_track(&track, cx)).on_close(cx.listener(
                |this, _, _, cx| {
                    this.context_menu = None;
                    cx.notify();
                },
            ))
        });

        let content = match stacked {
            true => base
                .flex_col()
                .justify_center()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .w_full()
                        .child(self.now_playing(show_track, window, cx))
                        .child(self.transport(cx)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .w_full()
                        .child(div().flex_1().min_w_0().child(seek))
                        .child(self.sound(px(VOLUME_TIGHT), cx))
                        .when_some(self.queue_button(cx), |this, button| this.child(button)),
                ),
            false => base
                .items_center()
                .gap_4()
                .child(self.now_playing(show_track, window, cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .flex_1()
                        .min_w_0()
                        .max_w(px(SEEK_MAX))
                        .child(self.transport(cx))
                        .child(seek),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_2()
                        .flex_1()
                        .min_w_0()
                        .child(self.sound(px(VOLUME_WIDTH), cx))
                        .when_some(self.queue_button(cx), |this, button| this.child(button)),
                ),
        };

        content.when_some(context_menu, |this, menu| this.child(menu))
    }
}
