// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::{Pixels, px};

pub const ALWAYS: Pixels = Pixels::ZERO;
pub const SNUG: Pixels = px(420.);
pub const ROOMY: Pixels = px(620.);
pub const WIDE: Pixels = px(740.);
pub const VAST: Pixels = px(1180.);

pub const MIN_CONTENT: Pixels = px(200.);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Room {
    Tight,
    Snug,
    Roomy,
    Wide,
    Vast,
}

impl Room {
    pub fn of(width: Pixels) -> Self {
        match width {
            width if width >= VAST => Self::Vast,
            width if width >= WIDE => Self::Wide,
            width if width >= ROOMY => Self::Roomy,
            width if width >= SNUG => Self::Snug,
            _ => Self::Tight,
        }
    }

    pub fn fits(self, step: Room) -> bool {
        self >= step
    }
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::{ROOMY, Room, SNUG, VAST, WIDE};

    #[test]
    fn classifies_every_step() {
        assert_eq!(Room::of(px(0.)), Room::Tight);
        assert_eq!(Room::of(SNUG - px(1.)), Room::Tight);
        assert_eq!(Room::of(SNUG), Room::Snug);
        assert_eq!(Room::of(ROOMY), Room::Roomy);
        assert_eq!(Room::of(WIDE), Room::Wide);
        assert_eq!(Room::of(VAST), Room::Vast);
    }

    #[test]
    fn steps_are_ordered() {
        assert!(Room::of(VAST).fits(Room::Roomy));
        assert!(Room::of(ROOMY).fits(Room::Roomy));
        assert!(!Room::of(SNUG).fits(Room::Roomy));
    }
}
