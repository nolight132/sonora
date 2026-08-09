// SPDX-License-Identifier: GPL-3.0-or-later

mod sink;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use librespot_core::{Session, SpotifyUri};
use librespot_playback::config::{AudioFormat, Bitrate, PlayerConfig};
use librespot_playback::mixer::NoOpVolume;
use librespot_playback::player::{Player, PlayerEvent};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::sink::{BlazingSink, Flush, Volume};

const TRACK_PREFIX: &str = "spotify:track:";

#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    pub normalisation: bool,
    pub gapless: bool,
    pub position_interval: Duration,
    pub gain: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioEvent {
    Loading(Duration),
    Playing(Duration),
    Paused(Duration),
    Position(Duration),
    Ended,
    Unavailable,
}

pub struct AudioEvents(UnboundedReceiver<PlayerEvent>);

impl AudioEvents {
    pub async fn next(&mut self) -> Option<AudioEvent> {
        loop {
            if let Some(event) = translate(self.0.recv().await?) {
                return Some(event);
            }
        }
    }
}

pub struct Engine {
    player: Arc<Player>,
    volume: Volume,
    flush: Flush,
    gapless: bool,
}

impl Engine {
    pub fn start(session: Session, config: EngineConfig) -> (Self, AudioEvents) {
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
        let player = Player::new(player_config, session, Box::new(NoOpVolume), move || {
            BlazingSink::boxed(AudioFormat::default(), sink_flush, sink_volume)
        });

        let events = AudioEvents(player.get_player_event_channel());
        let engine = Self {
            player,
            volume,
            flush,
            gapless: config.gapless,
        };
        (engine, events)
    }

    pub fn load(&self, track_id: &str) -> Result<()> {
        self.begin(track_id, true)
    }

    pub fn segue(&self, track_id: &str) -> Result<()> {
        self.begin(track_id, !self.gapless)
    }

    fn begin(&self, track_id: &str, cut: bool) -> Result<()> {
        let uri = track_uri(track_id)?;

        if cut {
            self.flush.request();
        }
        self.player.load(uri, true, 0);
        Ok(())
    }

    pub fn preload(&self, track_id: &str) -> Result<()> {
        self.player.preload(track_uri(track_id)?);
        Ok(())
    }

    pub fn play(&self) {
        self.player.play();
    }

    pub fn pause(&self) {
        self.player.pause();
    }

    pub fn seek(&self, position: Duration) {
        self.flush.request();
        self.player.seek(position.as_millis() as u32);
    }

    pub fn set_gain(&self, gain: f32) {
        self.volume.set(gain);
    }

    pub fn is_broken(&self) -> bool {
        self.player.is_invalid()
    }
}

fn track_uri(track_id: &str) -> Result<SpotifyUri> {
    SpotifyUri::from_uri(&format!("{TRACK_PREFIX}{track_id}"))
        .with_context(|| format!("{track_id} is not a track id"))
}

fn translate(event: PlayerEvent) -> Option<AudioEvent> {
    let millis = |position_ms: u32| Duration::from_millis(position_ms as u64);

    match event {
        PlayerEvent::Loading { position_ms, .. } => Some(AudioEvent::Loading(millis(position_ms))),
        PlayerEvent::Playing { position_ms, .. } => Some(AudioEvent::Playing(millis(position_ms))),
        PlayerEvent::Paused { position_ms, .. } => Some(AudioEvent::Paused(millis(position_ms))),
        PlayerEvent::PositionChanged { position_ms, .. }
        | PlayerEvent::PositionCorrection { position_ms, .. } => {
            Some(AudioEvent::Position(millis(position_ms)))
        }
        PlayerEvent::Stopped { .. } | PlayerEvent::EndOfTrack { .. } => Some(AudioEvent::Ended),
        PlayerEvent::Unavailable { .. } => Some(AudioEvent::Unavailable),
        _ => None,
    }
}
