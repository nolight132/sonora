// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, ElementId, Entity, FontWeight, Hsla, MouseButton, Pixels, Point,
    Render, ScrollHandle, SharedString, Window, div, px,
};
use i18n::t;
use router::{Destination, navigate};
use spotify::Track;
use ui::Input;

use crate::chrome::Chrome;
use crate::shared::menu::ItemMenu;
use state::{Hit, Kind, Playback, Search};
use ui::ActiveTheme as _;
use ui::{Card, Popup, Room, Scrollbar, Scroller, Text, Theme, VAST, clock, eyebrow};

use crate::shared::cells;
use crate::shared::tracks::{PlaybackStatus, playback_status};

enum Press {
    Song(Track),
    Artist(String),
    Album(String),
}

pub(crate) struct SearchView {
    input: Entity<Input>,
    search: Entity<Search>,
    playback: Entity<Playback>,
    playback_status: PlaybackStatus,
    songs: Entity<Scrollbar>,
    artists: Entity<Scrollbar>,
    albums: Entity<Scrollbar>,
    mixed: Entity<Scrollbar>,
    track_menu: ItemMenu,
    context_menu: Option<(Track, Point<Pixels>)>,
}

impl SearchView {
    pub(crate) fn new(
        search: Entity<Search>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| Input::new("search-placeholder", cx).icon("icons/search.svg"));

        cx.observe(&input, |this, input, cx| {
            let query = input.read(cx).text().to_owned();
            this.search.update(cx, |search, cx| search.ask(&query, cx));
        })
        .detach();

        cx.observe(&search, |this, _, cx| {
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

        let asked = input.read(cx).text().to_owned();
        search.update(cx, |search, cx| search.ask(&asked, cx));

        let playlist_scrollbar = cx.new(|_| {
            Scrollbar::new(ScrollHandle::new())
                .always_visible()
                .track_inset(px(4.))
        });

        Self {
            input,
            search,
            playback,
            playback_status: current_playback,
            songs: cx.new(|_| Scrollbar::new(ScrollHandle::new())),
            artists: cx.new(|_| Scrollbar::new(ScrollHandle::new())),
            albums: cx.new(|_| Scrollbar::new(ScrollHandle::new())),
            mixed: cx.new(|_| Scrollbar::new(ScrollHandle::new())),
            track_menu: ItemMenu::new(playlist_scrollbar),
            context_menu: None,
        }
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    fn pressed(
        &self,
        target: Press,
        cx: &Context<Self>,
    ) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, _, _, cx| match &target {
            Press::Song(track) => this
                .playback
                .update(cx, |playback, cx| playback.play_radio(track, cx)),
            Press::Artist(id) => navigate(Destination::Artist(id.clone().into()), cx),
            Press::Album(id) => navigate(Destination::Album(id.clone().into()), cx),
        })
    }

    fn subtitle(&self, hit: &Hit, place: usize, compact: bool, theme: &Theme) -> AnyElement {
        let links = |id: String, artists, fallback| {
            cells::artist_links(id, artists, fallback, theme.muted_foreground)
                .truncate()
                .into_any_element()
        };

        match (compact, hit) {
            (false, Hit::Song(track)) => links(
                format!("song-artist-{place}"),
                track.artist_refs.clone(),
                track.artists.clone(),
            ),
            (false, Hit::Album(album)) => links(
                format!("album-artist-{place}"),
                album.artist_refs.clone(),
                album.artists.clone(),
            ),
            _ => meta(hit, compact).into_any_element(),
        }
    }

    fn row(&self, hit: &Hit, place: usize, compact: bool, cx: &Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let meta = self.subtitle(hit, place, compact, &theme);

        match hit {
            Hit::Song(track) => {
                let tint = match track.id.is_some() && track.id == self.playback_status.0 {
                    true => theme.primary,
                    false => theme.foreground,
                };
                let track = track.clone();
                let view = cx.entity().downgrade();
                Card::new(("song", place), track.name.clone())
                    .cover(track.cover.clone())
                    .tint(tint)
                    .meta(meta)
                    .when(track.explicit, |card| card.explicit())
                    .trailing(
                        div()
                            .flex_none()
                            .w(cells::TRAILING)
                            .whitespace_nowrap()
                            .text_right()
                            .text_size(theme.text(Text::Small))
                            .text_color(theme.muted_foreground)
                            .child(clock(track.duration)),
                    )
                    .press(self.pressed(Press::Song(track.clone()), cx))
                    .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                        window.prevent_default();
                        let Some(view) = view.upgrade() else {
                            return;
                        };
                        view.update(cx, |this, cx| {
                            this.track_menu.reset();
                            this.context_menu = Some((track.clone(), event.position));
                            cx.notify();
                        });
                    })
                    .into_any_element()
            }
            Hit::Artist(artist) => {
                let card = Card::new(("artist", place), artist.name.clone())
                    .cover(artist.cover.clone())
                    .circle()
                    .meta(meta);
                match &artist.id {
                    Some(id) => card.press(self.pressed(Press::Artist(id.clone()), cx)),
                    None => card,
                }
                .into_any_element()
            }
            Hit::Album(album) => {
                Card::new(ElementId::Name(album.id.clone().into()), album.name.clone())
                    .cover(album.cover.clone())
                    .meta(meta)
                    .press(self.pressed(Press::Album(album.id.clone()), cx))
                    .into_any_element()
            }
        }
    }

    fn best(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = *cx.theme();
        let hit = self.search.read(cx).best()?;
        let (kind, title, artists, target) = match hit {
            Hit::Song(track) => (
                Kind::Song,
                track.name.clone(),
                track.artist_refs.clone(),
                Some(Press::Song(track.clone())),
            ),
            Hit::Artist(artist) => (
                Kind::Artist,
                artist.name.clone(),
                Vec::new(),
                artist.id.clone().map(Press::Artist),
            ),
            Hit::Album(album) => (
                Kind::Album,
                album.name.clone(),
                album.artist_refs.clone(),
                Some(Press::Album(album.id.clone())),
            ),
        };

        let song = match hit {
            Hit::Song(track) => Some(track.clone()),
            _ => None,
        };
        let view = cx.entity().downgrade();
        let card = Card::new("best", title)
            .art(theme.metrics.cover * 0.45)
            .cover(cover(hit).clone())
            .when(matches!(kind, Kind::Artist), Card::circle)
            .eyebrow(noun(kind))
            .size(Text::Title)
            .weight(FontWeight::BOLD)
            .bare_meta(
                cells::artist_links(
                    "best-artist-link",
                    artists,
                    meta(hit, false),
                    theme.muted_foreground,
                )
                .text_size(theme.text(Text::Small))
                .truncate(),
            )
            .flat()
            .gap_4()
            .p_3()
            .bg(theme.secondary)
            .when_some(target, |card, target| card.press(self.pressed(target, cx)))
            .when_some(song, |card, track| {
                card.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    window.prevent_default();
                    let Some(view) = view.upgrade() else {
                        return;
                    };
                    view.update(cx, |this, cx| {
                        this.track_menu.reset();
                        this.context_menu = Some((track.clone(), event.position));
                        cx.notify();
                    });
                })
            })
            .into_any_element();

        Some(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap_2()
                .child(eyebrow(t!("search-best-match"), cx).pb_1())
                .child(card)
                .into_any_element(),
        )
    }

    fn failure(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let reason = self.search.read(cx).error()?.to_owned();

        Some(
            div()
                .flex_none()
                .text_color(cx.theme().danger)
                .child(reason)
                .into_any_element(),
        )
    }

    fn panel(
        &self,
        id: &'static str,
        bar: &Entity<Scrollbar>,
        rows: Vec<AnyElement>,
        cx: &Context<Self>,
    ) -> AnyElement {
        if rows.is_empty() {
            return div()
                .flex_none()
                .text_color(cx.theme().muted_foreground)
                .child(t!("search-no-matches"))
                .into_any_element();
        }

        div()
            .flex_1()
            .min_h_0()
            .child(
                Scroller::new(id, bar)
                    .child(div().flex().flex_col().gap_1().pl_3().pr_3().children(rows)),
            )
            .into_any_element()
    }

    fn section(
        &self,
        id: &'static str,
        bar: &Entity<Scrollbar>,
        title: SharedString,
        rows: Vec<AnyElement>,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_1()
            .child(div().pl_3().child(eyebrow(title, cx).pb_1()))
            .child(self.panel(id, bar, rows, cx))
            .into_any_element()
    }

    fn column(&self, kind: Kind, cx: &Context<Self>) -> AnyElement {
        let (id, bar, title) = match kind {
            Kind::Song => ("search-songs", &self.songs, t!("search-songs")),
            Kind::Artist => ("search-artists", &self.artists, t!("search-artists")),
            Kind::Album => ("search-albums", &self.albums, t!("search-albums")),
        };

        let rows = self
            .search
            .read(cx)
            .of(kind)
            .enumerate()
            .map(|(place, hit)| self.row(hit, place, false, cx))
            .collect();

        self.section(id, bar, title, rows, cx)
    }

    fn everything(&self, cx: &Context<Self>) -> AnyElement {
        let mut places = [0; 3];
        let rows = self
            .search
            .read(cx)
            .hits()
            .iter()
            .map(|hit| {
                let slot = match hit {
                    Hit::Song(_) => 0,
                    Hit::Artist(_) => 1,
                    Hit::Album(_) => 2,
                };
                let place = places[slot];
                places[slot] += 1;
                self.row(hit, place, true, cx)
            })
            .collect();

        self.section("search-all", &self.mixed, t!("search-results"), rows, cx)
    }
}

fn cover(hit: &Hit) -> Option<String> {
    match hit {
        Hit::Song(track) => track.cover.clone(),
        Hit::Artist(artist) => artist.cover.clone(),
        Hit::Album(album) => album.cover.clone(),
    }
}

fn meta(hit: &Hit, compact: bool) -> SharedString {
    match hit {
        Hit::Song(track) => tagged(Kind::Song, &track.artists, compact),
        Hit::Album(album) => tagged(Kind::Album, &album.artists, compact),
        Hit::Artist(artist) => match compact {
            true => noun(Kind::Artist),
            false => held(artist.saved),
        },
    }
}

fn tagged(kind: Kind, value: &str, compact: bool) -> SharedString {
    if !compact {
        return SharedString::from(value.to_owned());
    }

    let noun = noun(kind);
    t!("search-tagged", kind = &noun, value = value)
}

fn held(saved: usize) -> SharedString {
    match saved {
        0 => noun(Kind::Artist),
        count => t!("search-saved", count = count),
    }
}

fn noun(kind: Kind) -> SharedString {
    match kind {
        Kind::Song => t!("kind-song"),
        Kind::Artist => t!("kind-artist"),
        Kind::Album => t!("kind-album"),
    }
}

fn divider(color: Hsla) -> impl IntoElement {
    div().flex_none().w(px(1.)).bg(color)
}

impl Render for SearchView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let pad = theme.metrics.inset;
        let width = cells::content_width(window, pad * 2., cx);
        let stacked = !Chrome::room(window, cx).fits(Room::Wide);
        let inset = match width > VAST {
            true => (width - VAST) / 2.,
            false => Pixels::ZERO,
        };
        let asked = !self.search.read(cx).query().trim().is_empty();
        let context_menu = self.context_menu.clone().map(|(track, position)| {
            Popup::new(position, self.track_menu.for_track(&track, cx)).on_close(cx.listener(
                |this, _, _, cx| {
                    this.context_menu = None;
                    cx.notify();
                },
            ))
        });

        let results = match stacked {
            true => self.everything(cx),
            false => div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(self.column(Kind::Song, cx))
                .child(divider(theme.border))
                .child(self.column(Kind::Artist, cx))
                .child(divider(theme.border))
                .child(self.column(Kind::Album, cx))
                .into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .gap_6()
            .px(pad + inset)
            .pt(pad)
            .pb(pad)
            .child(div().flex().flex_none().child(self.input.clone()))
            .children(self.failure(cx))
            .children(self.best(cx))
            .when(asked, |this| this.child(results))
            .when_some(context_menu, |this, menu| this.child(menu))
    }
}
