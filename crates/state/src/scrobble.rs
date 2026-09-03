use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{App, Context, Entity, SharedString, Task};
use music::Track;
use music::lastfm::{self, Play};
use tokio::task::AbortHandle;

use crate::playback::PlaybackEvent;
use crate::settings::Lastfm;
use crate::{AppSettings, Io, Playback, PlaybackState, join};

const WAIT: Duration = Duration::from_secs(300);
const FAILED: &str = "settings-lastfm-failed";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScrobbleState {
    Off,
    Linking,
    On(SharedString),
    Failed(&'static str),
}

pub struct Scrobbler {
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    io: Io,
    client: Option<Arc<lastfm::Scrobbler>>,
    state: ScrobbleState,
    play: Option<Play>,
    current: Option<String>,
    started: i64,
    scrobbled: bool,
    link: Option<Task<()>>,
    waiting: Option<AbortHandle>,
}

impl Scrobbler {
    pub fn new(
        playback: Entity<Playback>,
        settings: Entity<AppSettings>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&playback, |this, _, event, cx| match event {
            PlaybackEvent::StartedPlayback => this.begin(cx),
            PlaybackEvent::EndedPlayback => this.end(),
        })
        .detach();
        cx.observe(&playback, |this, _, cx| this.tick(cx)).detach();

        let mut scrobbler = Self {
            playback,
            settings,
            io,
            client: None,
            state: ScrobbleState::Off,
            play: None,
            current: None,
            started: 0,
            scrobbled: false,
            link: None,
            waiting: None,
        };
        scrobbler.rebuild(cx);
        scrobbler
    }

    pub fn state(&self) -> &ScrobbleState {
        &self.state
    }

    pub fn linked(&self) -> bool {
        matches!(self.state, ScrobbleState::On(_))
    }

    pub fn enabled(&self, cx: &App) -> bool {
        self.settings.read(cx).lastfm().enabled
    }

    pub fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |settings, cx| settings.set_scrobbling(enabled, cx));
        self.rebuild(cx);
    }

    pub fn link(&mut self, key: String, secret: String, cx: &mut Context<Self>) {
        let (key, secret) = (key.trim().to_owned(), secret.trim().to_owned());
        if key.is_empty() || secret.is_empty() {
            self.state = ScrobbleState::Failed(FAILED);
            return cx.notify();
        }

        self.stop_waiting();
        cx.open_url(&lastfm::authorize_url(&key));
        self.state = ScrobbleState::Linking;
        cx.notify();

        let waiting = self.io.spawn(async move {
            let token = lastfm::token(WAIT).await?;
            let (session, name) = lastfm::session(&key, &secret, &token).await?;
            Ok(Lastfm {
                key,
                secret,
                session,
                name,
                enabled: true,
            })
        });
        self.waiting = Some(waiting.abort_handle());

        self.link = Some(cx.spawn(async move |this, cx| {
            let linked = join(waiting).await;

            this.update(cx, |this, cx| {
                this.link = None;
                this.waiting = None;
                match linked {
                    Ok(account) => {
                        this.settings
                            .update(cx, |settings, cx| settings.set_lastfm(account, cx));
                        this.rebuild(cx);
                    }
                    Err(error) => {
                        log::warn!("scrobble: cannot link last.fm: {error:#}");
                        this.state = ScrobbleState::Failed(FAILED);
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn unlink(&mut self, cx: &mut Context<Self>) {
        self.stop_waiting();
        self.settings.update(cx, |settings, cx| {
            settings.set_lastfm(Lastfm::default(), cx)
        });
        self.rebuild(cx);
    }

    fn stop_waiting(&mut self) {
        self.link = None;
        if let Some(waiting) = self.waiting.take() {
            waiting.abort();
        }
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let account = self.settings.read(cx).lastfm().clone();
        self.client = (account.enabled && !account.session.is_empty()).then(|| {
            Arc::new(lastfm::Scrobbler::new(
                &account.key,
                &account.secret,
                &account.session,
            ))
        });
        self.state = match account.session.is_empty() {
            true => ScrobbleState::Off,
            false => ScrobbleState::On(SharedString::from(account.name)),
        };
        cx.notify();
    }

    fn begin(&mut self, cx: &mut Context<Self>) {
        let Some(track) = self.playback.read(cx).track() else {
            return;
        };
        let Some(play) = played(track) else {
            return;
        };
        let resumed = track.id.is_some() && self.current == track.id;
        if !resumed {
            self.current = track.id.clone();
            self.started = now();
            self.scrobbled = false;
        }
        self.play = Some(play.clone());

        let Some(client) = self.client.clone() else {
            return;
        };
        self.io.spawn(async move {
            if let Err(error) = client.now_playing(&play).await {
                log::warn!("scrobble: cannot report the current track: {error:#}");
            }
        });
    }

    fn end(&mut self) {
        self.play = None;
        self.current = None;
        self.scrobbled = false;
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.scrobbled {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(play) = self.play.clone() else {
            return;
        };
        let playback = self.playback.read(cx);
        if *playback.state() != PlaybackState::Playing || !play.earned(playback.position()) {
            return;
        }
        self.scrobbled = true;

        let at = self.started;
        self.io.spawn(async move {
            if let Err(error) = client.scrobble(&play, at).await {
                log::warn!("scrobble: cannot scrobble the track: {error:#}");
            }
        });
    }
}

fn played(track: &Track) -> Option<Play> {
    let artist = track
        .artist_refs
        .first()
        .map(|artist| artist.name.clone())
        .unwrap_or_else(|| track.artists.clone());
    if artist.is_empty() || track.name.is_empty() {
        return None;
    }

    Some(Play {
        artist,
        title: track.name.clone(),
        album: Some(track.album.clone()).filter(|album| !album.is_empty()),
        duration: track.duration,
    })
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}
