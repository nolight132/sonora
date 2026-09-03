use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{Context, Entity, EventEmitter, Task};
use music::{
    HostCommand, HostCommands, HostRepeat, HostState, MusicApi, RemoteDevice, RemoteState,
    RemoteTransport, RemoteUpdates, Track,
};

use crate::{AppSettings, Io, Outcome, Playback, Session, SessionEvent, Toasts, join};

const RETRY: Duration = Duration::from_secs(20);
const TICK: Duration = Duration::from_millis(500);
/// Least time between two state publishes. Spotify rate limits the endpoint, and a burst of
/// changes (a skip loads a track, which moves the queue, which moves the position) is one edit
/// as far as another device is concerned.
const SETTLE: Duration = Duration::from_millis(750);
/// Backed off to this after the endpoint refuses, rather than hammering it.
const BACKOFF: Duration = Duration::from_secs(10);
/// How far the position may drift from what another device would extrapolate before it is worth
/// telling them. Anything smaller is just the clock running.
const DRIFT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RemoteEvent {
    Engaged,
    Released,
}

/// What another device is playing, and the wire to command it. Sonora announces itself as an
/// observer, so it appears to nobody's device list and only ever watches and commands.
pub struct Remotes {
    session: Entity<Session>,
    settings: Entity<AppSettings>,
    playback: Option<Entity<Playback>>,
    io: Io,
    transport: Option<Arc<dyn RemoteTransport>>,
    state: RemoteState,
    at: Instant,
    target: Option<String>,
    track: Option<Track>,
    resolved: Option<String>,
    watch: Option<Task<()>>,
    resolve: Option<Task<()>>,
    command: Option<Task<()>>,
    tick: Option<Task<()>>,
    orders: Option<Task<()>>,
    pending: VecDeque<HostCommand>,
    drain: Option<Task<()>>,
    settle: Option<Task<()>>,
    publish: Option<Task<()>>,
    hosting: bool,
    reported: Option<HostState>,
    reported_at: Option<Instant>,
    hold: Option<Instant>,
}

impl EventEmitter<RemoteEvent> for Remotes {}

impl Remotes {
    pub fn new(
        session: Entity<Session>,
        settings: Entity<AppSettings>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&session, |this, _, event, cx| match event {
            SessionEvent::SignedIn => this.watch(cx),
            // a reconnect hands out a new librespot session, so the old dealer is gone
            SessionEvent::Reconnected => {
                this.watch = None;
                this.transport = None;
                this.watch(cx);
            }
            SessionEvent::SignedOut => this.forget(cx),
            SessionEvent::LocalChanged => {}
        })
        .detach();

        let mut remotes = Self {
            session,
            settings,
            playback: None,
            io,
            transport: None,
            state: RemoteState::default(),
            at: Instant::now(),
            target: None,
            track: None,
            resolved: None,
            watch: None,
            resolve: None,
            command: None,
            tick: None,
            orders: None,
            pending: VecDeque::new(),
            drain: None,
            settle: None,
            publish: None,
            hosting: false,
            reported: None,
            reported_at: None,
            hold: None,
        };
        remotes.watch(cx);
        remotes
    }

    /// Wired after construction, because `Playback` needs `Remotes` to exist first.
    pub fn bind(&mut self, playback: Entity<Playback>, cx: &mut Context<Self>) {
        // deferred: `carry_out` updates `Playback`, whose notify lands while it is borrowed
        cx.observe(&playback, |this, _, cx| this.schedule_report(cx))
            .detach();
        self.playback = Some(playback);
    }

    /// Whether another device handed playback to Sonora.
    pub fn hosting(&self) -> bool {
        self.hosting
    }

    /// Whether the Connect watch is up, so a device list is worth showing at all.
    pub fn reachable(&self) -> bool {
        self.transport.is_some()
    }

    /// Every device Spotify knows about except Sonora itself.
    pub fn devices(&self) -> &[RemoteDevice] {
        &self.state.devices
    }

    pub fn active(&self) -> Option<&RemoteDevice> {
        self.state.active_device()
    }

    /// The device Sonora is standing in for, once the user picked one.
    pub fn engaged(&self) -> Option<&RemoteDevice> {
        let target = self.target.as_deref()?;
        self.state.device(target).filter(|device| device.active)
    }

    pub fn track(&self) -> Option<&Track> {
        self.track.as_ref()
    }

    pub fn playing(&self) -> bool {
        self.state.playing
    }

    pub fn buffering(&self) -> bool {
        self.state.buffering
    }

    pub fn duration(&self) -> Duration {
        match self.state.duration.is_zero() {
            true => self
                .track
                .as_ref()
                .map(|track| track.duration)
                .unwrap_or_default(),
            false => self.state.duration,
        }
    }

    /// The cluster only pushes about once a second, so the reported position is carried
    /// forward by hand between pushes.
    pub fn position(&self) -> Duration {
        let total = self.duration();
        let live = match self.state.playing {
            true => self.state.position + self.at.elapsed(),
            false => self.state.position,
        };
        match total.is_zero() {
            true => live,
            false => live.min(total),
        }
    }

    pub fn volume(&self) -> Option<f32> {
        self.engaged().and_then(|device| device.volume)
    }

    fn watch(&mut self, cx: &mut Context<Self>) {
        let Some(factory) = self.session.read(cx).remotes() else {
            return self.forget(cx);
        };
        if self.watch.is_some() {
            return;
        }

        let io = self.io.clone();
        let playable = self.settings.read(cx).connect_hosting();
        self.watch = Some(cx.spawn(async move |this, cx| {
            loop {
                let started = join(io.spawn({
                    let factory = factory.clone();
                    async move { factory.watch(playable).await }
                }))
                .await;

                match started {
                    Ok(remote) => {
                        let music::Remote {
                            transport,
                            updates,
                            commands,
                        } = remote;
                        if this
                            .update(cx, |this, cx| {
                                this.transport = Some(transport);
                                this.listen(commands, cx);
                                cx.notify();
                            })
                            .is_err()
                        {
                            return;
                        }
                        pump(updates, &this, cx).await;
                    }
                    Err(error) => log::warn!("remotes: cannot watch for devices: {error:#}"),
                }

                if this
                    .update(cx, |this, cx| {
                        this.transport = None;
                        this.orders = None;
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
                cx.background_executor().timer(RETRY).await;
            }
        }));
    }

    /// Acts on what another device asked. Sonora only obeys once it has been handed playback,
    /// so a stray command cannot hijack a local session.
    fn obey(&mut self, command: HostCommand, cx: &mut Context<Self>) {
        if self.playback.is_none() {
            return;
        }
        if matches!(command, HostCommand::Take(_)) {
            self.hosting = true;
            self.target = None;
            self.tick = None;
        } else if !self.hosting {
            return;
        }
        cx.notify();

        // Driving `Playback` here would re-enter this entity — its transport reads `Remotes` —
        // so orders are queued and carried out on the next tick, outside this update. A burst
        // of skips must not cancel itself, hence a queue rather than one task.
        self.pending.push_back(command);
        if self.drain.is_some() {
            return;
        }
        self.drain = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::ZERO).await;
                let next = this.update(cx, |this, _| this.pending.pop_front());
                match next {
                    Ok(Some(command)) => {
                        if this
                            .update(cx, |this, cx| this.carry_out(command, cx))
                            .is_err()
                        {
                            return;
                        }
                    }
                    Ok(None) => {
                        this.update(cx, |this, _| this.drain = None).ok();
                        return;
                    }
                    Err(_) => return,
                }
            }
        }));
    }

    fn carry_out(&mut self, command: HostCommand, cx: &mut Context<Self>) {
        let Some(playback) = self.playback.clone() else {
            return;
        };

        match command {
            HostCommand::Take(handover) => {
                playback.update(cx, |playback, cx| playback.adopt_handover(handover, cx))
            }
            HostCommand::Play => {
                if !matches!(playback.read(cx).state(), crate::PlaybackState::Playing) {
                    playback.update(cx, |playback, cx| playback.resume(cx));
                }
            }
            HostCommand::Pause => {
                if matches!(
                    playback.read(cx).state(),
                    crate::PlaybackState::Playing | crate::PlaybackState::Loading
                ) {
                    playback.update(cx, |playback, cx| playback.pause(cx));
                }
            }
            HostCommand::Next => playback.update(cx, |playback, cx| playback.next(cx)),
            HostCommand::Previous => playback.update(cx, |playback, cx| playback.previous(cx)),
            HostCommand::Seek(at) => playback.update(cx, |playback, cx| playback.seek(at, cx)),
            HostCommand::Volume(level) => {
                playback.update(cx, |playback, cx| playback.set_volume(level, cx))
            }
            HostCommand::Repeat(repeat) => playback.update(cx, |playback, cx| {
                playback.set_repeat(
                    match repeat {
                        HostRepeat::Off => crate::Repeat::Off,
                        HostRepeat::Context => crate::Repeat::All,
                        HostRepeat::Track => crate::Repeat::One,
                    },
                    cx,
                )
            }),
            HostCommand::Shuffle(on) => self.set_shuffle(on, cx),
            HostCommand::Enqueue(id) => self.enqueue(id, cx),
            HostCommand::Resign => self.stand_down(cx),
        }
        cx.notify();
    }

    fn set_shuffle(&mut self, on: bool, cx: &mut Context<Self>) {
        let queue = crate::Sonora::global(cx).queue.clone();
        if queue.read(cx).shuffle() != on {
            queue.update(cx, |queue, cx| queue.toggle_shuffle(cx));
        }
    }

    fn enqueue(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(playback) = self.playback.clone() else {
            return;
        };
        let Some(client) = self.session.read(cx).client() else {
            return;
        };

        let io = self.io.clone();
        self.command = Some(cx.spawn(async move |_, cx| {
            let found = join(io.spawn(async move { client.track(&id).await })).await;
            match found {
                Ok(track) => {
                    playback.update(cx, |playback, cx| playback.enqueue(track, cx));
                }
                Err(error) => log::warn!("remotes: cannot queue a remote request: {error:#}"),
            }
        }));
    }

    /// Stops hosting and tells Spotify so, which is what moves the phone's "playing on" back
    /// off Sonora. Playback itself is left alone.
    pub fn stand_down(&mut self, cx: &mut Context<Self>) {
        if !self.hosting {
            return;
        }
        self.hosting = false;
        self.reported = None;
        self.reported_at = None;
        self.hold = None;
        self.publish = None;

        let Some(transport) = self.transport.clone() else {
            return;
        };
        let io = self.io.clone();
        self.command = Some(cx.spawn(async move |_, _| {
            if let Err(error) = join(io.spawn(async move { transport.resign().await })).await {
                log::warn!("remotes: cannot hand playback back: {error:#}");
            }
        }));
        cx.notify();
    }

    /// Publishes what Sonora is playing, so every other device's UI follows it. Skipped when
    /// nothing changed, because each call is a network round trip.
    fn schedule_report(&mut self, cx: &mut Context<Self>) {
        if !self.hosting || self.settle.is_some() {
            return;
        }
        // Always through a timer, never inline: this runs inside a `Playback` update, which
        // `report` reads back.
        let wait = self
            .hold
            .map(|until| until.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::ZERO);
        self.settle = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            this.update(cx, |this, cx| {
                this.settle = None;
                this.report(cx);
            })
            .ok();
        }));
    }

    /// Whether the state moved in a way another device cannot work out for itself. Position
    /// travels as a timestamp others extrapolate from, so the clock ticking is not news.
    fn worth_telling(&self, state: &HostState) -> bool {
        let Some(last) = self.reported.as_ref() else {
            return true;
        };
        let since = self
            .reported_at
            .map(|at| at.elapsed())
            .unwrap_or(Duration::ZERO);
        let expected = match last.playing {
            true => last.position + since,
            false => last.position,
        };
        if state.position.abs_diff(expected) > DRIFT {
            return true;
        }
        // everything but the position
        HostState {
            position: state.position,
            ..last.clone()
        } != *state
    }

    fn report(&mut self, cx: &mut Context<Self>) {
        if !self.hosting {
            return;
        }
        let Some(playback) = self.playback.clone() else {
            return;
        };
        let Some(transport) = self.transport.clone() else {
            return;
        };

        let state = playback.read(cx).hosted(cx);
        if !self.worth_telling(&state) {
            return;
        }
        self.reported = Some(state.clone());
        self.reported_at = Some(Instant::now());
        self.hold = Some(Instant::now() + SETTLE);

        let io = self.io.clone();
        self.publish = Some(cx.spawn(async move |this, cx| {
            let sent = join(io.spawn(async move { transport.publish(&state).await })).await;
            if let Err(error) = sent {
                log::warn!("remotes: cannot publish the player state: {error:#}");
                this.update(cx, |this, cx| {
                    // forget what was said, so the next attempt sends the whole state again
                    this.reported = None;
                    this.hold = Some(Instant::now() + BACKOFF);
                    this.schedule_report(cx);
                })
                .ok();
            }
        }));
    }

    /// Pumps what other devices ask of Sonora. Held as a task so it dies with the watch.
    fn listen(&mut self, mut commands: Box<dyn HostCommands>, cx: &mut Context<Self>) {
        self.orders = Some(cx.spawn(async move |this, cx| {
            while let Some(command) = commands.next().await {
                if this.update(cx, |this, cx| this.obey(command, cx)).is_err() {
                    return;
                }
            }
        }));
    }

    fn forget(&mut self, cx: &mut Context<Self>) {
        let released = self.target.take().is_some();
        self.watch = None;
        self.resolve = None;
        self.command = None;
        self.tick = None;
        self.drain = None;
        self.settle = None;
        self.pending.clear();
        self.transport = None;
        self.state = RemoteState::default();
        self.track = None;
        self.resolved = None;
        if released {
            cx.emit(RemoteEvent::Released);
        }
        cx.notify();
    }

    fn apply(&mut self, state: RemoteState, cx: &mut Context<Self>) {
        let moved = self.state.active != state.active;
        self.state = state;
        self.at = Instant::now();

        if moved
            && let Some(target) = self.target.clone()
            && self.state.active.as_deref() != Some(target.as_str())
        {
            self.target = None;
            cx.emit(RemoteEvent::Released);
        }

        self.resolve_track(cx);
        self.beat(cx);
        cx.notify();
    }

    /// The cluster pushes only when something changes, so a remote track would otherwise sit
    /// on one position for seconds. This ticks the shown clock forward the way the local
    /// engine's position events do.
    fn beat(&mut self, cx: &mut Context<Self>) {
        let wanted = self.state.playing && self.target.is_some();
        if !wanted {
            self.tick = None;
            return;
        }
        if self.tick.is_some() {
            return;
        }

        self.tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let running = this.update(cx, |this, cx| {
                    cx.notify();
                    this.state.playing && this.target.is_some()
                });
                match running {
                    Ok(true) => {}
                    _ => return,
                }
            }
        }));
    }

    fn resolve_track(&mut self, cx: &mut Context<Self>) {
        let wanted = self.state.track.clone();
        if wanted == self.resolved {
            return;
        }
        self.resolved = wanted.clone();

        let Some(id) = wanted else {
            self.track = None;
            self.resolve = None;
            return;
        };
        let Some(client) = self.session.read(cx).client() else {
            return;
        };

        let io = self.io.clone();
        self.resolve = Some(cx.spawn(async move |this, cx| {
            let found = join(io.spawn(async move { track(client, &id).await })).await;
            this.update(cx, |this, cx| {
                match found {
                    Ok(track) => this.track = Some(track),
                    Err(error) => log::warn!("remotes: cannot read the remote track: {error:#}"),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Moves playback to `id` and starts standing in for it. `resume` is what Sonora is
    /// playing locally, so picking a device carries the track across.
    pub fn engage(&mut self, id: &str, resume: Option<(String, Duration)>, cx: &mut Context<Self>) {
        let Some(transport) = self.transport.clone() else {
            return Toasts::show(Outcome::Failed, "toast-remote-unavailable", cx);
        };
        let Some(device) = self.state.device(id).cloned() else {
            return;
        };

        self.target = Some(device.id.clone());
        cx.emit(RemoteEvent::Engaged);
        self.beat(cx);
        cx.notify();

        let active = self.state.active.clone();
        let io = self.io.clone();
        let name = device.name.clone();
        self.command = Some(cx.spawn(async move |this, cx| {
            let moved = join(io.spawn(async move {
                match resume {
                    Some((track, at)) => transport.play_track(&device.id, &track, at).await,
                    None => transport.transfer(active.as_deref(), &device.id).await,
                }
            }))
            .await;

            if let Err(error) = moved {
                log::warn!("remotes: cannot move playback to {name}: {error:#}");
                this.update(cx, |this, cx| {
                    this.target = None;
                    cx.emit(RemoteEvent::Released);
                    cx.notify();
                    Toasts::about(Outcome::Failed, "toast-remote-failed", name, cx);
                })
                .ok();
            }
        }));
    }

    /// Stops standing in for the remote device. It keeps playing; Sonora just stops showing
    /// and driving it.
    pub fn release(&mut self, cx: &mut Context<Self>) {
        if self.target.take().is_none() {
            return;
        }
        self.tick = None;
        cx.emit(RemoteEvent::Released);
        cx.notify();
    }

    pub(crate) fn play(&mut self, cx: &mut Context<Self>) {
        self.state.playing = true;
        self.at = Instant::now();
        self.beat(cx);
        self.send(
            |transport, target| async move { transport.play(&target).await },
            cx,
        );
    }

    pub(crate) fn pause(&mut self, cx: &mut Context<Self>) {
        self.state.position = self.position();
        self.state.playing = false;
        self.at = Instant::now();
        self.tick = None;
        self.send(
            |transport, target| async move { transport.pause(&target).await },
            cx,
        );
    }

    pub(crate) fn next(&mut self, cx: &mut Context<Self>) {
        self.send(
            |transport, target| async move { transport.next(&target).await },
            cx,
        );
    }

    pub(crate) fn previous(&mut self, cx: &mut Context<Self>) {
        self.send(
            |transport, target| async move { transport.previous(&target).await },
            cx,
        );
    }

    pub(crate) fn seek(&mut self, position: Duration, cx: &mut Context<Self>) {
        self.state.position = position;
        self.at = Instant::now();
        self.send(
            move |transport, target| async move { transport.seek(&target, position).await },
            cx,
        );
    }

    pub(crate) fn set_volume(&mut self, level: f32, cx: &mut Context<Self>) {
        if let Some(device) = self
            .target
            .clone()
            .and_then(|id| self.state.devices.iter_mut().find(|held| held.id == id))
            && device.volume.is_some()
        {
            device.volume = Some(level.clamp(0., 1.));
        }
        self.send(
            move |transport, target| async move { transport.set_volume(&target, level).await },
            cx,
        );
    }

    fn send<Make, Sent>(&mut self, make: Make, cx: &mut Context<Self>)
    where
        Make: FnOnce(Arc<dyn RemoteTransport>, String) -> Sent + Send + 'static,
        Sent: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let Some(transport) = self.transport.clone() else {
            return;
        };
        let Some(target) = self.target.clone() else {
            return;
        };

        let io = self.io.clone();
        self.command = Some(cx.spawn(async move |_, _| {
            if let Err(error) = join(io.spawn(make(transport, target))).await {
                log::warn!("remotes: the device refused a command: {error:#}");
            }
        }));
        cx.notify();
    }
}

async fn pump(
    mut updates: Box<dyn RemoteUpdates>,
    remotes: &gpui::WeakEntity<Remotes>,
    cx: &mut gpui::AsyncApp,
) {
    while let Some(state) = updates.next().await {
        if remotes
            .update(cx, |this, cx| this.apply(state, cx))
            .is_err()
        {
            return;
        }
    }
}

async fn track(client: Arc<dyn MusicApi>, id: &str) -> anyhow::Result<Track> {
    client.track(id).await
}
