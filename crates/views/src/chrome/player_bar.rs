use router::{Destination, Link, navigate};
use std::time::Duration;
use ui::ActiveTheme as _;

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, Render, ScrollWheelEvent, SharedString,
};
use gpui::{Window, div, px};
use i18n::t;
use input::{ToggleFullscreen, ToggleLyrics, ToggleQueue};
use state::{AppSettings, Lyrics, Playback, Queue, SideTab, Sonora};
use ui::{
    Artwork, Button, ExplicitBadge, InlineLink, InlineLinks, Popup, Room, Scrollbar, Scrubber,
    ScrubberState, Text, clock,
};

use crate::chrome::SidebarRight;
use crate::shared::menus::ItemMenu;
use crate::shared::transport::{NOTCH, like, moved, percent, transport, volume_icon};

const SEEK_MAX: f32 = 560.;
const VOLUME_WIDTH: f32 = 110.;
const VOLUME_TIGHT: f32 = 72.;
const CLOCK_SHORT: f32 = 3.4;
const CLOCK_LONG: f32 = 5.4;

fn compact_lyrics_line(lyrics: &music::Lyrics, position: Duration) -> Option<SharedString> {
    let music::Lyrics::Synced { lines } = lyrics else {
        return None;
    };
    let line = lines
        .get(music::lyrics::active(lines, position)?)?
        .text
        .trim();
    (!line.is_empty()).then(|| SharedString::from(line.to_owned()))
}

fn compact_lyrics_available(
    viewport_height: Pixels,
    title_height: Pixels,
    player_height: Pixels,
    cover_size: Pixels,
) -> bool {
    viewport_height - title_height - player_height < cover_size * 2.
}

pub(crate) struct PlayerBar {
    playback: Entity<Playback>,
    queue: Entity<Queue>,
    lyrics: Entity<Lyrics>,
    settings: Entity<AppSettings>,
    track_menu: ItemMenu,
    context_menu: Option<(music::Track, Point<Pixels>)>,
    seek: ScrubberState,
    volume: ScrubberState,
    pending: Option<f32>,
    over_seek: Option<f32>,
    over_volume: Option<f32>,
    volume_held: bool,
    muted: Option<f32>,
}

impl PlayerBar {
    pub fn new(playback: Entity<Playback>, queue: Entity<Queue>, cx: &mut Context<Self>) -> Self {
        let library = Sonora::global(cx).library.clone();
        let lyrics = Sonora::global(cx).lyrics.clone();
        let settings = Sonora::global(cx).settings.clone();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        cx.observe(&queue, |_, _, cx| cx.notify()).detach();
        cx.observe(&library, |_, _, cx| cx.notify()).detach();
        cx.observe(&lyrics, |_, _, cx| cx.notify()).detach();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();

        let me = cx.entity_id();
        let playlist_scrollbar = cx.new(|_| Scrollbar::inset().watching(me));

        Self {
            playback,
            queue,
            lyrics,
            settings,
            track_menu: ItemMenu::new(playlist_scrollbar),
            context_menu: None,
            seek: ScrubberState::new("seek"),
            volume: ScrubberState::new("volume"),
            pending: None,
            over_seek: None,
            over_volume: None,
            volume_held: false,
            muted: None,
        }
    }

    fn open_context_menu(
        &mut self,
        track: music::Track,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.track_menu.reset(cx);
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

    fn turn_volume(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(window.line_height()).y;
        if delta == Pixels::ZERO {
            return;
        }
        cx.stop_propagation();

        let notch = match delta > Pixels::ZERO {
            true => NOTCH,
            false => -NOTCH,
        };
        let level = (self.playback.read(cx).volume() + notch).clamp(0., 1.);
        self.muted = None;
        self.playback
            .update(cx, |playback, cx| playback.set_volume(level, cx));
    }

    fn sound(&self, width: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let empty = theme.muted_foreground.opacity(0.3);
        let level = self.playback.read(cx).volume();
        let showing = self.over_volume.is_some() || self.volume_held;
        let bubble = showing.then(|| (level, percent(level)));
        let restore = self.muted.unwrap_or(0.7);

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .on_scroll_wheel(cx.listener(Self::turn_volume))
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
                            this.volume_held = true;
                            this.muted = None;
                            this.playback
                                .update(cx, |playback, cx| playback.set_volume(level, cx));
                        }))
                        .on_release(cx.listener(|this, _: &MouseUpEvent, _, cx| {
                            this.volume_held = false;
                            cx.notify();
                        })),
                ),
            )
    }

    fn side_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let settings = self.settings.read(cx);
        let open = settings.sidebar_right_open();
        let tab = settings.sidebar_right_tab();

        let button = move |id: &'static str, icon: &'static str, hint: &'static str, side| {
            let showing = open && tab == side;

            Button::new(id)
                .ghost()
                .small()
                .icon(icon)
                .tooltip_above(hint)
                .selected(showing)
                .tint(match showing {
                    true => theme.foreground,
                    false => theme.muted_foreground,
                })
                .on_click(move |_, window, cx| {
                    let action: Box<dyn gpui::Action> = match side {
                        SideTab::Queue => Box::new(ToggleQueue),
                        SideTab::Lyrics => Box::new(ToggleLyrics),
                    };
                    window.dispatch_action(action, cx);
                })
        };

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .child(button(
                "player-lyrics",
                "icons/mic-vocal.svg",
                "lyrics-title",
                SideTab::Lyrics,
            ))
            .child(button(
                "player-queue",
                "icons/list-music.svg",
                "queue-title",
                SideTab::Queue,
            ))
            .into_any_element()
    }

    fn fullscreen_button(&self) -> Button {
        Button::new("toggle-fullscreen")
            .ghost()
            .small()
            .icon("icons/maximize.svg")
            .tooltip_above("player-fullscreen")
            .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleFullscreen), cx))
    }

    fn now_playing(&self, room: bool, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let artwork = ui::snapped(theme.metrics.row, window);
        let artists = theme.text(ui::Text::Small);
        let track = self.playback.read(cx).track().cloned();
        let cover = track.as_ref().and_then(|track| track.cover.clone());
        let explicit = track.as_ref().is_some_and(|track| track.explicit);
        let like = like(track.clone(), cx);

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
                    .when_some(track.clone(), |this, context| {
                        this.on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                window.prevent_default();
                                this.open_context_menu(context.clone(), event.position, cx);
                            }),
                        )
                    })
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
                                .when(explicit, |this| {
                                    this.child(div().flex_none().child(ExplicitBadge::new()))
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

impl PlayerBar {
    fn base_height(window: &Window, cx: &gpui::App) -> Pixels {
        let theme = *cx.theme();
        match !Room::of(window.viewport_size().width).fits(Room::Roomy) {
            true => ui::snapped(theme.metrics.player_bar + theme.metrics.pad * 3., window),
            false => ui::snapped(theme.metrics.player_bar, window),
        }
    }

    fn compact_line(&self, window: &Window, cx: &gpui::App) -> Option<SharedString> {
        let theme = *cx.theme();
        if !compact_lyrics_available(
            window.viewport_size().height,
            theme.metrics.title_bar,
            Self::base_height(window, cx),
            theme.metrics.cover,
        ) {
            return None;
        }
        let lyrics = self.lyrics.read(cx);
        let position = self.playback.read(cx).live_position();
        compact_lyrics_line(&lyrics.current()?.lyrics, position)
    }

    pub(crate) fn height(&self, window: &Window, cx: &gpui::App) -> Pixels {
        let theme = *cx.theme();
        Self::base_height(window, cx)
            + self
                .compact_line(window, cx)
                .map_or(Pixels::ZERO, |_| theme.metrics.list_row)
    }
}

impl Render for PlayerBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let empty = muted.opacity(0.3);
        let span = Room::of(window.viewport_size().width);
        let stacked = !span.fits(Room::Roomy);
        let height = self.height(window, cx);
        let compact_line = self.compact_line(window, cx);
        let clock_text = theme.text(ui::Text::Tiny);

        let show_track = span.fits(Room::Snug);
        let sides = match SidebarRight::available(window) {
            true => Some(self.side_buttons(cx)),
            false => None,
        };

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
            .h(height
                - compact_line
                    .as_ref()
                    .map_or(Pixels::ZERO, |_| theme.metrics.list_row))
            .flex_none()
            .px_5()
            .when(stacked, |this| this.py_2())
            .when(!theme.transparent, |this| this.bg(theme.secondary))
            .when(compact_line.is_none(), |this| {
                this.border_t_1().border_color(theme.border)
            })
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
                        .child(transport(&self.playback, &self.queue, false, cx)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .w_full()
                        .child(div().flex_1().min_w_0().child(seek))
                        .children(sides)
                        .child(self.sound(px(VOLUME_TIGHT), cx))
                        .child(self.fullscreen_button()),
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
                        .child(transport(&self.playback, &self.queue, false, cx))
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
                        .children(sides)
                        .child(self.sound(px(VOLUME_WIDTH), cx))
                        .child(self.fullscreen_button()),
                ),
        };

        div()
            .flex()
            .flex_col()
            .w_full()
            .h(height)
            .when_some(compact_line, |this, line| {
                this.child(
                    div()
                        .id("compact-lyrics")
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .h(theme.metrics.list_row)
                        .px_5()
                        .border_t_1()
                        .border_color(theme.border)
                        .when(!theme.transparent, |this| this.bg(theme.secondary))
                        .text_size(theme.text(Text::Body))
                        .child(
                            div()
                                .w_full()
                                .min_w_0()
                                .truncate()
                                .text_center()
                                .child(line),
                        ),
                )
            })
            .child(content)
            .when_some(context_menu, |this, menu| this.child(menu))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use music::{Lyrics, LyricsLine, Voice};

    use super::*;

    fn line(start: u64, text: &str) -> LyricsLine {
        LyricsLine {
            start: Duration::from_secs(start),
            end: None,
            text: text.to_owned(),
            romanized: None,
            words: None,
            secondary: Vec::new(),
            voice: Voice::Lead,
        }
    }

    #[test]
    fn compact_lyrics_follow_the_current_synced_line() {
        let lyrics = Lyrics::Synced {
            lines: Arc::from([line(2, "first"), line(5, "second")]),
        };

        assert_eq!(compact_lyrics_line(&lyrics, Duration::from_secs(1)), None);
        assert_eq!(
            compact_lyrics_line(&lyrics, Duration::from_secs(2)).as_deref(),
            Some("first")
        );
        assert_eq!(
            compact_lyrics_line(&lyrics, Duration::from_secs(5)).as_deref(),
            Some("second")
        );
    }

    #[test]
    fn compact_lyrics_ignore_plain_and_empty_lines() {
        assert_eq!(
            compact_lyrics_line(&Lyrics::plain("plain"), Duration::ZERO),
            None
        );
        assert_eq!(
            compact_lyrics_line(
                &Lyrics::Synced {
                    lines: Arc::from([line(0, "  ")]),
                },
                Duration::ZERO,
            ),
            None
        );
    }

    #[test]
    fn compact_lyrics_use_the_height_left_above_the_player() {
        let title = px(36.);
        let player = px(76.);
        let cover = px(140.);

        assert!(!compact_lyrics_available(
            title + player + cover * 2.,
            title,
            player,
            cover
        ));
        assert!(compact_lyrics_available(
            title + player + cover * 2. - px(1.),
            title,
            player,
            cover
        ));
    }
}
