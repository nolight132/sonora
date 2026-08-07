// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::{Context, EventEmitter};

use crate::Destination;

pub enum NavigationEvent {
    Moved(Destination),
}

pub struct Navigation {
    trail: Vec<Destination>,
    at: usize,
}

impl EventEmitter<NavigationEvent> for Navigation {}

impl Navigation {
    pub fn new(start: Destination) -> Self {
        Self {
            trail: vec![start],
            at: 0,
        }
    }

    pub fn current(&self) -> Destination {
        self.trail[self.at].clone()
    }

    pub fn can_go_back(&self) -> bool {
        self.at > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.at + 1 < self.trail.len()
    }

    pub fn go(&mut self, destination: Destination, cx: &mut Context<Self>) {
        if self.current() == destination {
            return;
        }

        self.trail.truncate(self.at + 1);
        self.trail.push(destination);
        self.at = self.trail.len() - 1;
        self.arrive(cx);
    }

    pub fn back(&mut self, cx: &mut Context<Self>) {
        if !self.can_go_back() {
            return;
        }
        self.at -= 1;
        self.arrive(cx);
    }

    pub fn forward(&mut self, cx: &mut Context<Self>) {
        if !self.can_go_forward() {
            return;
        }
        self.at += 1;
        self.arrive(cx);
    }

    fn arrive(&mut self, cx: &mut Context<Self>) {
        cx.emit(NavigationEvent::Moved(self.current()));
        cx.notify();
    }
}
