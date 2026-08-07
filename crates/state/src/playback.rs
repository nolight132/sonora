// SPDX-License-Identifier: GPL-3.0-or-later

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use audio::{AudioEvent, AudioEvents, Engine, EngineConfig};
use gpui::{Context, Entity, EventEmitter, Task};
use spotify::{SpotifyApi, Track};

type Fetch = Pin<Box<dyn Future<Output = Result<Vec<Track>>> + Send>>;

use crate::queue::Queue;
use crate::{AppSettings, Io, Session, SessionEvent, join};

const POSITION_INTERVAL: Duration = Duration::from_millis(500);
const SKIP_DEBOUNCE: Duration = Duration::from_millis(250);
const KEY_COOLDOWN: Duration = Duration::from_secs(6);
const TAPER_DB: f32 = 50.;

fn gain(level: f32) -> f32 {
    match level.clamp(0., 1.) {
        level if level <= 0. => 0.,
        level => 10f32.powf(TAPER_DB * (level - 1.) / 20.),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Idle,
    Playing,
    Paused,
    Loading,
    Failed(String),
}

pub enum PlaybackEvent {
    StartedPlayback,
    EndedPlayback,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

#[derive(Clone, PartialEq, Eq)]
pub enum Origin {
    Album(String),
    Playlist(String),
    Radio(String),
}

pub struct Playback {
    state: PlaybackState,
    origin: Option<Origin>,
    position: Duration,
    track: Option<Track>,
    engine: Option<Engine>,
    session: Entity<Session>,
    queue: Entity<Queue>,
    settings: Entity<AppSettings>,
    level: f32,
    normalisation: bool,
    repeat: Repeat,
    radio: bool,
    task: Option<Task<()>>,
    load: Option<Task<()>>,
    fetch: Option<Task<()>>,
    blocked_until: Option<Instant>,
}

impl EventEmitter<PlaybackEvent> for Playback {}

impl Playback {
    pub fn new(
        session: Entity<Session>,
        queue: Entity<Queue>,
        settings: Entity<AppSettings>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&session, |this, session, event, cx| match event {
            SessionEvent::SignedIn => {
                let Some(librespot) = session.read(cx).librespot() else {
                    return;
                };
                this.start_engine(librespot, cx);
            }
            SessionEvent::SignedOut => this.teardown(cx),
        })
        .detach();

        let level = settings.read(cx).volume();
        let normalisation = settings.read(cx).normalisation();

        Self {
            state: PlaybackState::Idle,
            origin: None,
            position: Duration::ZERO,
            track: None,
            engine: None,
            session,
            queue,
            settings,
            level,
            normalisation,
            repeat: Repeat::Off,
            radio: false,
            task: None,
            load: None,
            fetch: None,
            blocked_until: None,
        }
    }

    pub fn play(&mut self, track: &Track, cx: &mut Context<Self>) {
        self.load_after(track, Duration::ZERO, cx);
    }

    pub fn preload(&self, track: &Track) {
        let Some(engine) = self.engine.as_ref() else {
            return;
        };
        let Some(id) = track.id.as_deref() else {
            return;
        };
        if !track.playable || self.track.as_ref().and_then(|track| track.id.as_deref()) == Some(id)
        {
            return;
        }
        if let Err(error) = engine.preload(id) {
            log::warn!("playback: cannot preload {}: {error:#}", track.name);
        }
    }

    fn load_after(&mut self, track: &Track, debounce: Duration, cx: &mut Context<Self>) {
        if self.engine.is_none() {
            return;
        }
        let Some(id) = track.id.clone() else {
            return self.failed(format!("{} has no track id", track.name), cx);
        };
        if !track.playable {
            return self.failed(format!("{} is not available to stream", track.name), cx);
        }

        self.track = Some(track.clone());
        self.state = PlaybackState::Loading;
        self.position = Duration::ZERO;
        cx.notify();

        let wait = self
            .blocked_until
            .and_then(|until| until.checked_duration_since(Instant::now()))
            .unwrap_or_default()
            .max(debounce);

        self.load = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            this.update(cx, |this, cx| {
                let Some(engine) = this.engine.as_ref() else {
                    return;
                };
                if let Err(error) = engine.load(&id) {
                    this.failed(format!("{error:#}"), cx);
                }
            })
            .ok();
        }));
    }

    pub fn start(&mut self, tracks: Vec<Track>, index: usize, cx: &mut Context<Self>) {
        self.fetch = None;
        self.begin(tracks, index, None, cx);
    }

    pub fn play_radio(&mut self, seed: &Track, cx: &mut Context<Self>) {
        let Some(id) = seed.id.clone() else {
            return self.failed(format!("{} has no track id", seed.name), cx);
        };
        if !seed.playable {
            return self.failed(format!("{} is not available to stream", seed.name), cx);
        }

        let origin = Origin::Radio(id.clone());
        let seed = seed.clone();
        self.gather(origin, cx, move |client| {
            Box::pin(async move {
                let mut tracks = client.track_radio(&id).await?;
                tracks.retain(|track| track.id != seed.id && track.playable);
                fastrand::shuffle(&mut tracks);
                tracks.insert(0, seed);
                Ok(tracks)
            })
        });
    }

    pub fn play_album(&mut self, album: &str, cx: &mut Context<Self>) {
        let origin = Origin::Album(album.to_owned());
        let album = album.to_owned();
        self.gather(origin, cx, move |client| {
            Box::pin(async move { client.album_tracks(&album).await })
        });
    }

    pub fn play_playlist(&mut self, playlist: &str, cx: &mut Context<Self>) {
        let origin = Origin::Playlist(playlist.to_owned());
        let playlist = playlist.to_owned();
        self.gather(origin, cx, move |client| {
            Box::pin(async move { client.playlist_tracks(&playlist).await })
        });
    }

    pub fn origin(&self) -> Option<&Origin> {
        self.origin.as_ref()
    }

    pub fn playing_from(&self, origin: &Origin) -> Option<PlaybackState> {
        (self.origin.as_ref() == Some(origin)).then(|| self.state.clone())
    }

    fn begin(
        &mut self,
        tracks: Vec<Track>,
        index: usize,
        origin: Option<Origin>,
        cx: &mut Context<Self>,
    ) {
        let Some(track) = self
            .queue
            .update(cx, |queue, cx| queue.start(tracks, index, cx))
        else {
            return;
        };
        self.origin = origin;
        self.play(&track, cx);
    }

    fn gather<F>(&mut self, origin: Origin, cx: &mut Context<Self>, tracks: F)
    where
        F: FnOnce(Arc<dyn SpotifyApi>) -> Fetch + Send + 'static,
    {
        let Some(client) = self.session.read(cx).client() else {
            return;
        };

        let io = Io::global(cx);
        if !self.has_active_playback() {
            self.state = PlaybackState::Loading;
            cx.notify();
        }

        self.fetch = Some(cx.spawn(async move |this, cx| {
            let loaded = join(io.spawn(async move { tracks(client).await })).await;

            this.update(cx, |this, cx| match loaded {
                Ok(tracks) => this.begin(tracks, 0, Some(origin), cx),
                Err(error) if this.has_active_playback() => {
                    log::error!("playback: cannot load context: {error:#}");
                }
                Err(error) => this.failed(format!("{error:#}"), cx),
            })
            .ok();
        }));
    }

    pub fn next(&mut self, cx: &mut Context<Self>) {
        self.fetch = None;
        self.follow_queue(cx);
    }

    pub fn radio(&self) -> bool {
        self.radio
    }

    pub fn toggle_radio(&mut self, cx: &mut Context<Self>) {
        self.radio = !self.radio;
        cx.notify();
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    pub fn cycle_repeat(&mut self, cx: &mut Context<Self>) {
        self.repeat = match self.repeat {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        };
        cx.notify();
    }

    fn advance(&mut self, ended: Option<Track>, cx: &mut Context<Self>) {
        match self.repeat {
            Repeat::One => match ended {
                Some(track) => self.load_after(&track, Duration::ZERO, cx),
                None => self.next(cx),
            },
            Repeat::All if !self.queue.read(cx).has_next() => {
                self.fetch = None;
                if let Some(track) = self.queue.update(cx, |queue, cx| queue.rewind(cx)) {
                    self.load_after(&track, SKIP_DEBOUNCE, cx);
                }
            }
            _ if self.radio && !self.queue.read(cx).has_next() => {
                match ended.or_else(|| self.track.clone()) {
                    Some(seed) => self.extend_radio(&seed, cx),
                    None => self.next(cx),
                }
            }
            _ => self.next(cx),
        }
    }

    fn extend_radio(&mut self, seed: &Track, cx: &mut Context<Self>) {
        let (Some(id), Some(client)) = (seed.id.clone(), self.session.read(cx).client()) else {
            return self.next(cx);
        };

        let io = Io::global(cx);
        let heard = seed.id.clone();
        self.fetch = Some(cx.spawn(async move |this, cx| {
            let loaded = join(io.spawn(async move {
                let mut tracks = client.track_radio(&id).await?;
                tracks.retain(|track| track.id != heard && track.playable);
                fastrand::shuffle(&mut tracks);
                anyhow::Ok(tracks)
            }))
            .await;

            this.update(cx, |this, cx| match loaded {
                Ok(tracks) if !tracks.is_empty() => {
                    this.queue.update(cx, |queue, cx| {
                        for track in tracks {
                            queue.append(track, cx);
                        }
                    });
                    this.follow_queue(cx);
                }
                Ok(_) => log::warn!("playback: radio returned no tracks"),
                Err(error) => log::warn!("playback: cannot extend radio: {error:#}"),
            })
            .ok();
        }));
    }

    fn follow_queue(&mut self, cx: &mut Context<Self>) {
        let Some(track) = self.queue.update(cx, |queue, cx| queue.next(cx)) else {
            return;
        };
        self.load_after(&track, SKIP_DEBOUNCE, cx);
    }

    pub fn previous(&mut self, cx: &mut Context<Self>) {
        self.fetch = None;
        let Some(track) = self.queue.update(cx, |queue, cx| queue.previous(cx)) else {
            return;
        };
        self.load_after(&track, SKIP_DEBOUNCE, cx);
    }

    pub fn play_past(&mut self, index: usize, cx: &mut Context<Self>) {
        self.fetch = None;
        let Some(track) = self
            .queue
            .update(cx, |queue, cx| queue.play_past(index, cx))
        else {
            return;
        };
        self.load_after(&track, SKIP_DEBOUNCE, cx);
    }

    pub fn play_upcoming(&mut self, index: usize, cx: &mut Context<Self>) {
        self.fetch = None;
        let Some(track) = self
            .queue
            .update(cx, |queue, cx| queue.play_upcoming(index, cx))
        else {
            return;
        };
        self.load_after(&track, SKIP_DEBOUNCE, cx);
    }

    pub fn resume(&mut self, cx: &mut Context<Self>) {
        if let Some(engine) = self.engine.as_ref() {
            engine.play();
            cx.notify();
        }
    }

    pub fn pause(&mut self, cx: &mut Context<Self>) {
        if let Some(engine) = self.engine.as_ref() {
            engine.pause();
            cx.notify();
        }
    }

    pub fn toggle_play(&mut self, cx: &mut Context<Self>) {
        if self.state == PlaybackState::Playing {
            self.pause(cx);
        } else {
            self.resume(cx);
        }
    }

    pub fn seek(&mut self, position: Duration, cx: &mut Context<Self>) {
        if let Some(engine) = self.engine.as_ref() {
            engine.seek(position);
            self.position = position;
            cx.notify();
        }
    }

    pub fn seek_fraction(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let Some(total) = self
            .track
            .as_ref()
            .map(|track| track.duration)
            .filter(|total| !total.is_zero())
        else {
            return;
        };

        let position = Duration::from_secs_f32(total.as_secs_f32() * fraction.clamp(0., 1.));
        self.seek(position, cx);
    }

    pub fn state(&self) -> &PlaybackState {
        &self.state
    }

    pub fn position(&self) -> Duration {
        self.position
    }

    pub fn track(&self) -> Option<&Track> {
        self.track.as_ref()
    }

    pub fn progress(&self) -> f32 {
        let Some(total) = self.track.as_ref().map(|track| track.duration) else {
            return 0.;
        };
        if total.is_zero() {
            return 0.;
        }
        (self.position.as_secs_f32() / total.as_secs_f32()).clamp(0., 1.)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.state, PlaybackState::Loading)
    }

    fn has_active_playback(&self) -> bool {
        self.track.is_some()
            && matches!(
                self.state,
                PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Loading
            )
    }

    pub fn volume(&self) -> f32 {
        self.level
    }

    pub fn set_volume(&mut self, level: f32, cx: &mut Context<Self>) {
        self.level = level.clamp(0., 1.);
        self.settings
            .update(cx, |settings, cx| settings.set_volume(self.level, cx));
        if let Some(engine) = self.engine.as_ref() {
            engine.set_gain(gain(self.level));
        }
        cx.notify();
    }

    pub fn normalisation(&self) -> bool {
        self.normalisation
    }

    pub fn set_normalisation(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.normalisation == on {
            return;
        }
        self.normalisation = on;
        self.settings
            .update(cx, |settings, cx| settings.set_normalisation(on, cx));

        if self.engine.is_some() {
            let session = self.session.read(cx).librespot();
            if let Some(session) = session {
                self.start_engine(session, cx);
                return;
            }
        }
        cx.notify();
    }

    fn start_engine(&mut self, session: librespot_core::Session, cx: &mut Context<Self>) {
        let config = EngineConfig {
            normalisation: self.normalisation,
            position_interval: POSITION_INTERVAL,
            gain: gain(self.level),
        };
        let (engine, events) = Engine::start(session, config);

        self.listen(events, cx);
        self.engine = Some(engine);
        self.state = PlaybackState::Idle;
        self.position = Duration::ZERO;
        cx.notify();
    }

    fn listen(&mut self, mut events: AudioEvents, cx: &mut Context<Self>) {
        self.task = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                if this.update(cx, |this, cx| this.apply(event, cx)).is_err() {
                    break;
                }
            }
        }));
    }

    fn apply(&mut self, event: AudioEvent, cx: &mut Context<Self>) {
        match event {
            AudioEvent::Loading(position) => {
                self.state = PlaybackState::Loading;
                self.position = position;
            }
            AudioEvent::Playing(position) => {
                let started = self.state != PlaybackState::Playing;
                self.state = PlaybackState::Playing;
                self.position = position;
                if started {
                    cx.emit(PlaybackEvent::StartedPlayback);
                }
            }
            AudioEvent::Paused(position) => {
                self.state = PlaybackState::Paused;
                self.position = position;
            }
            AudioEvent::Position(position) => self.position = position,
            AudioEvent::Ended => {
                let ended = self.track.take();
                self.state = PlaybackState::Idle;
                self.position = Duration::ZERO;
                cx.emit(PlaybackEvent::EndedPlayback);
                self.advance(ended, cx);
            }
            AudioEvent::Unavailable => {
                let name = self.track.as_ref().map(|track| track.name.as_str());
                log::warn!(
                    "playback: {} failed to load, backing off {}s",
                    name.unwrap_or("?"),
                    KEY_COOLDOWN.as_secs()
                );
                self.blocked_until = Some(Instant::now() + KEY_COOLDOWN);
                self.state = PlaybackState::Idle;
                self.position = Duration::ZERO;
                self.track = None;
                cx.emit(PlaybackEvent::EndedPlayback);
            }
        }
        cx.notify();
    }

    fn teardown(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        self.load = None;
        self.fetch = None;
        self.blocked_until = None;
        self.engine = None;
        self.track = None;
        self.origin = None;
        self.state = PlaybackState::Idle;
        self.position = Duration::ZERO;
        cx.notify();
    }

    fn failed(&mut self, problem: String, cx: &mut Context<Self>) {
        log::error!("playback: {problem}");
        self.state = PlaybackState::Failed(problem);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::gain;

    #[test]
    fn never_amplifies_past_unity() {
        assert_eq!(gain(1.), 1.);
        for step in 0..=100 {
            assert!(gain(step as f32 / 100.) <= 1.);
        }
    }

    #[test]
    fn silences_a_closed_slider() {
        assert_eq!(gain(0.), 0.);
        assert_eq!(gain(-1.), 0.);
        assert_eq!(gain(2.), 1.);
    }

    #[test]
    fn rises_with_the_slider() {
        let mut last = gain(0.);
        for step in 1..=100 {
            let next = gain(step as f32 / 100.);
            assert!(next > last, "gain fell at {step}");
            last = next;
        }
    }

    #[test]
    fn halves_the_slider_to_the_taper_midpoint() {
        let expected = 10f32.powf(-super::TAPER_DB / 40.);
        assert!((gain(0.5) - expected).abs() < 1e-6);
    }
}
