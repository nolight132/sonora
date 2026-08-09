// SPDX-License-Identifier: GPL-3.0-or-later

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Entity, FontWeight, MouseButton, Pixels, Point, RenderOnce,
    SharedString, Window, div, px,
};
use router::{Destination, navigate};
use spotify::Album;
use state::{Origin, Playback, PlaybackState};
use ui::{ActiveTheme as _, Card, Text};

pub(crate) const CARD_MIN: Pixels = px(130.);
pub(crate) const CARD_MAX: Pixels = px(190.);
const CARD_GAP: Pixels = px(32.);

type ContextMenu = Rc<dyn Fn(Album, Point<Pixels>, &mut App)>;
type LoadArt = Rc<dyn Fn(usize) -> bool>;
type LayoutListener = Rc<dyn Fn(Vec<Bounds<Pixels>>, &mut App)>;

#[derive(Clone, Copy)]
pub(crate) struct CardLayout {
    pub(crate) columns: usize,
    pub(crate) card: Pixels,
    gap: Pixels,
}

impl CardLayout {
    fn new(available: Pixels) -> Self {
        let available = available.max(CARD_MIN);
        let columns = (((available + CARD_GAP) / (CARD_MIN + CARD_GAP))
            .floor()
            .max(1.)) as usize;
        let count = columns as f32;
        let spread = available - CARD_GAP * (count - 1.);
        let card = (spread / count).min(CARD_MAX).floor();
        let gap = match columns > 1 {
            true => ((available - card * count) / (count - 1.)).floor(),
            false => Pixels::ZERO,
        };

        Self { columns, card, gap }
    }
}

#[derive(IntoElement)]
pub(crate) struct CardGrid {
    layout: CardLayout,
    cards: Vec<AnyElement>,
}

impl CardGrid {
    pub(crate) fn new(available: Pixels) -> Self {
        Self {
            layout: CardLayout::new(available),
            cards: Vec::new(),
        }
    }

    pub(crate) fn layout(available: Pixels) -> CardLayout {
        CardLayout::new(available)
    }

    pub(crate) fn children(mut self, cards: impl IntoIterator<Item = AnyElement>) -> Self {
        self.cards.extend(cards);
        self
    }
}

impl RenderOnce for CardGrid {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .w_full()
            .gap_x(self.layout.gap)
            .children(self.cards)
    }
}

#[derive(IntoElement)]
pub(crate) struct AlbumGrid {
    id: &'static str,
    layout: CardLayout,
    albums: Vec<(usize, Album)>,
    playback: Entity<Playback>,
    load_art: LoadArt,
    on_context: Option<ContextMenu>,
    on_layout: Option<LayoutListener>,
}

impl AlbumGrid {
    pub(crate) fn new(
        id: &'static str,
        available: Pixels,
        albums: impl IntoIterator<Item = (usize, Album)>,
        playback: Entity<Playback>,
    ) -> Self {
        Self {
            id,
            layout: CardLayout::new(available),
            albums: albums.into_iter().collect(),
            playback,
            load_art: Rc::new(|_| true),
            on_context: None,
            on_layout: None,
        }
    }

    pub(crate) fn layout(available: Pixels) -> CardLayout {
        CardLayout::new(available)
    }

    pub(crate) fn load_art_when(mut self, load: impl Fn(usize) -> bool + 'static) -> Self {
        self.load_art = Rc::new(load);
        self
    }

    pub(crate) fn on_context(
        mut self,
        listener: impl Fn(Album, Point<Pixels>, &mut App) + 'static,
    ) -> Self {
        self.on_context = Some(Rc::new(listener));
        self
    }

    pub(crate) fn on_layout(
        mut self,
        listener: impl Fn(Vec<Bounds<Pixels>>, &mut App) + 'static,
    ) -> Self {
        self.on_layout = Some(Rc::new(listener));
        self
    }
}

impl RenderOnce for AlbumGrid {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            id,
            layout,
            albums,
            playback,
            load_art,
            on_context,
            on_layout,
        } = self;
        let cards = albums.into_iter().map(|(index, album)| {
            let context = album.clone();
            let card = album_card(
                id,
                index,
                album,
                playback.clone(),
                layout.card,
                load_art(index),
                cx,
            );
            let Some(listener) = on_context.clone() else {
                return card;
            };

            div()
                .id(SharedString::from(format!("{id}-context-{index}")))
                .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    listener(context.clone(), event.position, cx);
                })
                .child(card)
                .into_any_element()
        });

        div()
            .flex()
            .flex_wrap()
            .w_full()
            .gap_x(layout.gap)
            .gap_y_6()
            .children(cards)
            .when_some(on_layout, |grid, listener| {
                grid.on_children_prepainted(move |bounds, _, cx| listener(bounds, cx))
            })
    }
}

fn album_card(
    id: &'static str,
    index: usize,
    album: Album,
    playback: Entity<Playback>,
    width: Pixels,
    load_art: bool,
    cx: &App,
) -> AnyElement {
    let theme = *cx.theme();
    let cover = match load_art {
        true => album.cover_large.clone().or_else(|| album.cover.clone()),
        false => None,
    };
    let artists = crate::shared::cells::artist_links(
        SharedString::from(format!("{id}-artist-{index}")),
        album.artist_refs.clone(),
        album.artists.clone(),
        theme.muted_foreground,
    )
    .text_size(theme.text(Text::Small))
    .truncate();
    let origin = Origin::Album(album.id.clone());
    let state = playback.read(cx).playing_from(&origin);
    let playing = matches!(state, Some(PlaybackState::Playing));
    let played = album.id.clone();
    let opened = SharedString::from(album.id);

    Card::new((id, index), SharedString::from(album.name))
        .tile(width)
        .art_radius(theme.radius)
        .cover(cover)
        .weight(FontWeight::SEMIBOLD)
        .flat()
        .underline()
        .bare_meta(div().child(artists))
        .play(playing, move |_, _, cx| {
            playback.update(cx, |playback, cx| match &state {
                Some(PlaybackState::Playing) => playback.pause(cx),
                Some(PlaybackState::Paused) => playback.resume(cx),
                _ => playback.play_album(&played, cx),
            });
        })
        .press(move |_, _, cx| navigate(Destination::Album(opened.clone()), cx))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_stays_within_its_bounds() {
        for width in (160..2400).step_by(3) {
            let layout = CardLayout::new(px(width as f32));
            assert!(layout.card <= CARD_MAX, "{width} yielded {:?}", layout.card);
            assert!(layout.card >= CARD_MIN, "{width} yielded {:?}", layout.card);
        }
    }

    #[test]
    fn a_row_never_outgrows_the_space_it_was_given() {
        for width in (160..2400).step_by(3) {
            let available = px(width as f32);
            let layout = CardLayout::new(available);
            let count = layout.columns as f32;
            let used = layout.card * count + layout.gap * (count - 1.);
            assert!(used <= available, "{width} packed {used:?}");
        }
    }

    #[test]
    fn cards_never_touch() {
        for width in (160..2400).step_by(3) {
            let layout = CardLayout::new(px(width as f32));
            if layout.columns > 1 {
                assert!(layout.gap >= CARD_GAP, "{width} yielded {:?}", layout.gap);
            }
        }
    }

    #[test]
    fn slack_goes_to_the_gaps_once_the_cards_are_capped() {
        let layout = CardLayout::new(CARD_MAX * 2. + CARD_GAP * 2.);

        assert_eq!(layout.card, CARD_MAX);
        assert!(layout.gap > CARD_GAP);
    }

    #[test]
    fn a_single_column_has_no_gap() {
        let layout = CardLayout::new(CARD_MIN);
        assert_eq!(layout.gap, Pixels::ZERO);
    }
}
