use std::time::Duration;

use gpui::{Context, Entity, Task};
use music::{ConnectHandle, ConnectStatus, PlaybackConfig};

use crate::playback::gain;
use crate::{AppSettings, Io, Outcome, Playback, Session, SessionEvent, SessionState, Toasts};

const DEVICE_NAME: &str = "Sonora";
const POSITION_INTERVAL: Duration = Duration::from_secs(1);
const SPOTIFY: &str = "spotify";

pub struct Connect {
    session: Entity<Session>,
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    io: Io,
    handle: Option<Box<dyn ConnectHandle>>,
    events: Option<Task<()>>,
    connected: bool,
}

impl Connect {
    pub fn new(
        session: Entity<Session>,
        playback: Entity<Playback>,
        settings: Entity<AppSettings>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&session, |this, _, event, cx| match event {
            SessionEvent::SignedIn | SessionEvent::SignedOut | SessionEvent::Reconnected => {
                this.sync(cx)
            }
            SessionEvent::LocalChanged => {}
        })
        .detach();
        cx.observe(&settings, |this, _, cx| this.sync(cx)).detach();

        let mut connect = Self {
            session,
            playback,
            settings,
            io,
            handle: None,
            events: None,
            connected: false,
        };
        connect.sync(cx);
        connect
    }

    /// Whether a remote (typically a phone) has an active Spotify Connect session with
    /// Sonora right now, as opposed to Sonora merely advertising and waiting for one.
    pub fn connected(&self) -> bool {
        self.connected
    }

    fn wanted(&self, cx: &Context<Self>) -> bool {
        self.settings.read(cx).spotify_connect()
            && self.session.read(cx).provider_slug() == Some(SPOTIFY)
            && matches!(self.session.read(cx).state(), SessionState::SignedIn(_))
    }

    fn sync(&mut self, cx: &mut Context<Self>) {
        match (self.wanted(cx), self.handle.is_some()) {
            (true, false) => self.start(cx),
            (false, true) => self.stop(cx),
            _ => {}
        }
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        let Some(provider) = self.session.read(cx).provider() else {
            return;
        };
        let settings = self.settings.read(cx);
        let config = PlaybackConfig {
            normalisation: settings.normalisation(),
            gapless: settings.gapless(),
            position_interval: POSITION_INTERVAL,
            gain: gain(settings.volume()),
        };
        let runtime = self.io.handle();

        let Some((handle, mut events)) = provider.connect(runtime, DEVICE_NAME.to_owned(), config)
        else {
            return;
        };
        self.handle = Some(handle);

        self.connected = false;
        self.events = Some(cx.spawn(async move |this, cx| {
            while let Some(status) = events.recv().await {
                if this
                    .update(cx, |this, cx| this.on_status(status, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        cx.notify();
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        self.handle = None;
        self.events = None;
        self.connected = false;
        cx.notify();
    }

    fn on_status(&mut self, status: ConnectStatus, cx: &mut Context<Self>) {
        match status {
            ConnectStatus::Connected | ConnectStatus::Paused => {
                self.connected = true;
            }
            ConnectStatus::Playing => {
                self.connected = true;
                self.playback.update(cx, |playback, cx| playback.pause(cx));
                Toasts::show(Outcome::Done, "toast-connect-active", cx);
            }
            ConnectStatus::Disconnected => {
                self.connected = false;
            }
            ConnectStatus::Unavailable => {
                log::warn!("connect: cannot advertise Sonora on the network");
                Toasts::show(Outcome::Failed, "toast-connect-unavailable", cx);
                return self.stop(cx);
            }
            ConnectStatus::Advertising => {}
        }
        cx.notify();
    }
}
