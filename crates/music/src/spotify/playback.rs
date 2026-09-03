use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use librespot_core::{Session, SpotifyUri};
use librespot_playback::config::{Bitrate, PlayerConfig};
use librespot_playback::mixer::NoOpVolume;
use librespot_playback::player::{Player, PlayerEvent};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::audio::Volume;
use crate::spotify::sink::{BlazingSink, Flush};
use crate::{
    PlaybackConfig, PlaybackEvent, PlaybackEvents, PlaybackFactory, Player as MusicPlayer,
};

const TRACK_PREFIX: &str = "spotify:track:";

pub struct Events(UnboundedReceiver<PlayerEvent>);

#[async_trait]
impl PlaybackEvents for Events {
    async fn next(&mut self) -> Option<PlaybackEvent> {
        loop {
            if let Some(event) = translate(self.0.recv().await?) {
                return Some(event);
            }
        }
    }
}

pub struct Factory(Session);

impl Factory {
    pub fn new(session: Session) -> Self {
        Self(session)
    }
}

impl PlaybackFactory for Factory {
    fn start(&self, config: PlaybackConfig) -> (Box<dyn MusicPlayer>, Box<dyn PlaybackEvents>) {
        let (engine, events) = Engine::start(self.0.clone(), config);
        (Box::new(engine), Box::new(events))
    }
}

pub struct Engine {
    player: Arc<Player>,
    volume: Volume,
    flush: Flush,
    gapless: bool,
}

impl Engine {
    fn start(session: Session, config: PlaybackConfig) -> (Self, Events) {
        let volume = Volume::new(config.gain);
        let flush = Flush::default();

        let player_config = PlayerConfig {
            bitrate: Bitrate::Bitrate320,
            normalisation: config.normalisation,
            gapless: config.gapless,
            position_update_interval: Some(config.position_interval),
            ..Default::default()
        };

        let sink_volume = volume.clone();
        let sink_flush = flush.clone();
        let sink_visualizer = config.visualizer.clone();
        let player = Player::new(player_config, session, Box::new(NoOpVolume), move || {
            BlazingSink::boxed(
                sink_flush.clone(),
                sink_volume.clone(),
                sink_visualizer.clone(),
            )
        });

        let events = Events(player.get_player_event_channel());
        let engine = Self {
            player,
            volume,
            flush,
            gapless: config.gapless,
        };
        (engine, events)
    }

    fn load(&self, track_id: &str, seamless: bool) -> Result<()> {
        let uri = track_uri(track_id)?;

        if !seamless || !self.gapless {
            self.flush.request();
        }
        self.player.load(uri, true, 0);
        Ok(())
    }

    fn load_paused_at(&self, track_id: &str, at: Duration) -> Result<()> {
        let uri = track_uri(track_id)?;

        self.flush.request();
        self.player.load(uri, false, at.as_millis() as u32);
        Ok(())
    }

    fn preload(&self, track_id: &str) -> Result<()> {
        self.player.preload(track_uri(track_id)?);
        Ok(())
    }

    fn play(&self) {
        self.player.play();
    }

    fn pause(&self) {
        self.player.pause();
    }

    fn seek(&self, position: Duration) {
        self.flush.request();
        self.player.seek(position.as_millis() as u32);
    }

    fn set_gain(&self, gain: f32) {
        self.volume.set(gain);
    }
}

impl MusicPlayer for Engine {
    fn load(&self, track_id: &str, seamless: bool) -> Result<()> {
        self.load(track_id, seamless)
    }

    fn load_paused_at(&self, track_id: &str, at: Duration) -> Result<()> {
        self.load_paused_at(track_id, at)
    }

    fn preload(&self, track_id: &str) -> Result<()> {
        self.preload(track_id)
    }

    fn play(&self) {
        self.play();
    }

    fn pause(&self) {
        self.pause();
    }

    fn seek(&self, position: Duration) {
        self.seek(position);
    }

    fn set_gain(&self, gain: f32) {
        self.set_gain(gain);
    }
}

fn track_uri(track_id: &str) -> Result<SpotifyUri> {
    SpotifyUri::from_uri(&format!("{TRACK_PREFIX}{track_id}"))
        .with_context(|| format!("{track_id} is not a track id"))
}

fn translate(event: PlayerEvent) -> Option<PlaybackEvent> {
    let millis = |position_ms: u32| Duration::from_millis(position_ms as u64);

    match event {
        PlayerEvent::Loading { position_ms, .. } => {
            Some(PlaybackEvent::Loading(millis(position_ms)))
        }
        PlayerEvent::Playing { position_ms, .. } => {
            Some(PlaybackEvent::Playing(millis(position_ms)))
        }
        PlayerEvent::Paused { position_ms, .. } => Some(PlaybackEvent::Paused(millis(position_ms))),
        PlayerEvent::PositionChanged { position_ms, .. }
        | PlayerEvent::PositionCorrection { position_ms, .. } => {
            Some(PlaybackEvent::Position(millis(position_ms)))
        }
        PlayerEvent::Stopped { .. } | PlayerEvent::EndOfTrack { .. } => Some(PlaybackEvent::Ended),
        PlayerEvent::Unavailable { denied: true, .. } => Some(PlaybackEvent::Refused),
        PlayerEvent::Unavailable { .. } => Some(PlaybackEvent::Unavailable),
        _ => None,
    }
}
