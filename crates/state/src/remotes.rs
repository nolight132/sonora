use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{Context, Entity, EventEmitter, Task};
use music::{MusicApi, RemoteDevice, RemoteState, RemoteTransport, RemoteUpdates, Track};

use crate::{Io, Outcome, Session, SessionEvent, Toasts, join};

const RETRY: Duration = Duration::from_secs(20);
const TICK: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RemoteEvent {
    Engaged,
    Released,
}

/// What another device is playing, and the wire to command it. Sonora announces itself as an
/// observer, so it appears to nobody's device list and only ever watches and commands.
pub struct Remotes {
    session: Entity<Session>,
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
}

impl EventEmitter<RemoteEvent> for Remotes {}

impl Remotes {
    pub fn new(session: Entity<Session>, io: Io, cx: &mut Context<Self>) -> Self {
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
        };
        remotes.watch(cx);
        remotes
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
        self.watch = Some(cx.spawn(async move |this, cx| {
            loop {
                let started = join(io.spawn({
                    let factory = factory.clone();
                    async move { factory.watch().await }
                }))
                .await;

                match started {
                    Ok((transport, updates)) => {
                        this.update(cx, |this, cx| {
                            this.transport = Some(transport);
                            cx.notify();
                        })
                        .ok();
                        pump(updates, &this, cx).await;
                    }
                    Err(error) => log::warn!("remotes: cannot watch for devices: {error:#}"),
                }

                if this
                    .update(cx, |this, cx| {
                        this.transport = None;
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

    fn forget(&mut self, cx: &mut Context<Self>) {
        let released = self.target.take().is_some();
        self.watch = None;
        self.resolve = None;
        self.command = None;
        self.tick = None;
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
