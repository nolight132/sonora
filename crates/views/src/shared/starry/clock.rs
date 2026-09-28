use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gpui::{App, EntityId, Window};

/// The frames the stage's own motion needs: the turn of the record, the sweep
/// of the sheen, the drift of the particles. None of it depends on sound being
/// made, and once playback stops nothing else in fullscreen asks for frames at
/// all — the spectrum drive parks and the view goes quiet — so a stage that
/// rode along with playback would freeze mid-turn the moment it was paused.
/// This asks for frames on the stage's behalf, from inside the frame it was
/// last given, and wakes the view to redraw. Call it every render; it only
/// ever has one loop running, and it stops the moment `live` goes false.
/// How many frames the loop may run on without the view having rendered. A
/// render refreshes the allowance, so while the stage is on screen the loop
/// never runs dry; once the view stops asking — it was closed, or the stage
/// was switched off — it winds down on its own instead of spinning forever on
/// a view nobody is watching.
const GRACE: u32 = 3;

#[derive(Clone)]
pub(crate) struct Drive {
    armed: Rc<Cell<bool>>,
    beats: Rc<Cell<u32>>,
}

impl Default for Drive {
    fn default() -> Self {
        Self {
            armed: Rc::new(Cell::new(false)),
            beats: Rc::new(Cell::new(0)),
        }
    }
}

impl Drive {
    pub(crate) fn run(&self, watch: EntityId, live: bool, window: &mut Window) {
        self.beats.set(match live {
            true => GRACE,
            false => 0,
        });
        if !live || self.armed.replace(true) {
            return;
        }
        let drive = self.clone();
        window.on_next_frame(move |window, cx| drive.step(watch, window, cx));
    }

    fn step(&self, watch: EntityId, window: &mut Window, cx: &mut App) {
        let beats = self.beats.get();
        if beats == 0 {
            self.armed.set(false);
            return;
        }
        self.beats.set(beats - 1);
        cx.notify(watch);
        let drive = self.clone();
        window.on_next_frame(move |window, cx| drive.step(watch, window, cx));
    }
}

/// How long the particle field takes to fade in or out, as a time constant:
/// it covers about two thirds of the way in this long, and is all but there
/// after three times as much. Not a share per frame, so it lasts the same on
/// any display. Raise it for a slower drift in and out.
const FADE: f32 = 1.6;

/// What the clock hands back for one frame.
pub(crate) struct Pose {
    /// Seconds the music has been playing: the record's turn.
    pub(crate) turn: f32,
    /// How much of the particle field is there, from none of it to all of it.
    /// It eases out once the music stops and back in once it starts — the
    /// drift belongs to the sound, not to the window being open.
    pub(crate) presence: f32,
}

/// How long the record has been turning, and nothing else: the clock only
/// banks time while sound is being made, so pausing parks it mid-turn — no
/// rewind, no coasting — and resuming picks the turn up from where it stood.
/// Call it once per render; it remembers when it was last called and adds the
/// gap only when `playing`. The cap keeps a long stall — a suspended window,
/// a hitch in the loop — from lurching the record forward.
#[derive(Default)]
pub(crate) struct Clock {
    played: Cell<f32>,
    presence: Cell<f32>,
    last: Cell<Option<Instant>>,
}

impl Clock {
    pub(crate) fn tick(&self, playing: bool) -> Pose {
        let now = Instant::now();
        if let Some(last) = self.last.replace(Some(now)) {
            let gone = now.duration_since(last).as_secs_f32().min(0.1);
            if playing {
                self.played.set(self.played.get() + gone);
            }
            let target = match playing {
                true => 1.,
                false => 0.,
            };
            let rate = 1. - (-gone / FADE).exp();
            let presence = self.presence.get();
            self.presence.set(presence + (target - presence) * rate);
        }
        Pose {
            turn: self.played.get(),
            presence: self.presence.get(),
        }
    }

    /// Whether anything on the stage still moves: the record turning, or the
    /// field still fading. Once a paused stage has gone still there is nothing
    /// left to draw, and the frame loop may wind down.
    pub(crate) fn moving(&self, playing: bool) -> bool {
        playing || self.presence.get() > 0.01
    }
}
