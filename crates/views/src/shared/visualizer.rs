use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, EntityId, Window};
use music::Spectrum;

const EASE: f32 = 0.35;

#[derive(Default)]
struct State {
    shown: Vec<f32>,
    armed: bool,
    visible: bool,
}

#[derive(Clone, Default)]
pub struct VisualizerDrive {
    state: Rc<RefCell<State>>,
}

impl VisualizerDrive {
    pub fn levels(&self) -> Vec<f32> {
        self.state.borrow().shown.clone()
    }

    pub fn show(&self, watch: EntityId, spectrum: Spectrum, window: &mut Window) {
        let mut state = self.state.borrow_mut();
        state.visible = true;
        if state.armed {
            return;
        }
        state.armed = true;
        drop(state);

        let drive = self.clone();
        window.on_next_frame(move |window, cx| drive.step(watch, spectrum, window, cx));
    }

    pub fn hide(&self) {
        self.state.borrow_mut().visible = false;
    }

    fn step(&self, watch: EntityId, spectrum: Spectrum, window: &mut Window, cx: &mut App) {
        {
            let mut state = self.state.borrow_mut();
            if !state.visible {
                state.armed = false;
                return;
            }

            let target = spectrum.bands();
            if state.shown.len() != target.len() {
                state.shown = target.clone();
            }
            for (shown, target) in state.shown.iter_mut().zip(&target) {
                *shown += (target - *shown) * EASE;
            }
        }

        cx.notify(watch);
        let drive = self.clone();
        window.on_next_frame(move |window, cx| drive.step(watch, spectrum, window, cx));
    }
}
