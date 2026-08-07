// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{Div, MouseButton, Stateful};

use crate::{Destination, navigate};

pub trait Link: Sized {
    fn link(self, to: Destination) -> Self;
}

impl Link for Stateful<Div> {
    fn link(self, to: Destination) -> Self {
        self.cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, _, cx| navigate(to.clone(), cx))
    }
}
