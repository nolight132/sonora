use std::fmt::Write as _;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use futures::StreamExt as _;
use librespot_connect::{ConnectConfig, Spirc};
use librespot_core::authentication::Credentials;
use librespot_core::config::DeviceType;
use librespot_core::{Session, SessionConfig};
use librespot_discovery::Discovery;
use librespot_playback::config::{Bitrate, PlayerConfig};
use librespot_playback::mixer::{Mixer, MixerConfig, NoOpVolume};
use librespot_playback::player::{Player, PlayerEvent, PlayerEventChannel};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::audio::Volume;
use crate::spotify::auth::AuthConfig;
use crate::spotify::sink::{BlazingSink, Flush};
use crate::{ConnectStatus, PlaybackConfig};

pub struct Handle {
    shutdown: Option<oneshot::Sender<()>>,
}

impl Handle {
    fn stop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

impl crate::ConnectHandle for Handle {
    fn stop(&mut self) {
        self.stop();
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn start(
    runtime: tokio::runtime::Handle,
    auth: AuthConfig,
    name: String,
    config: PlaybackConfig,
) -> (Handle, crate::ConnectEvents) {
    let (status_tx, status_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    runtime.spawn(run(auth, name, config, status_tx, shutdown_rx));

    (
        Handle {
            shutdown: Some(shutdown_tx),
        },
        status_rx,
    )
}

struct ConnectMixer(Volume);

impl Mixer for ConnectMixer {
    fn open(_config: MixerConfig) -> Result<Self, librespot_core::Error>
    where
        Self: Sized,
    {
        Ok(Self(Volume::new(0.5)))
    }

    fn volume(&self) -> u16 {
        (self.0.get().clamp(0.0, 1.0) * u16::MAX as f32).round() as u16
    }

    fn set_volume(&self, volume: u16) {
        self.0.set(volume as f32 / u16::MAX as f32);
    }
}

type Session3 = (Spirc, JoinHandle<()>, JoinHandle<()>);

async fn run(
    auth: AuthConfig,
    name: String,
    config: PlaybackConfig,
    status: mpsc::UnboundedSender<ConnectStatus>,
    mut shutdown: oneshot::Receiver<()>,
) {
    let device_id = device_id(&name);
    let client_id = auth.client_id.clone();

    let mut discovery = match Discovery::builder(device_id.clone(), client_id.clone())
        .name(name.clone())
        .device_type(DeviceType::Speaker)
        .launch()
    {
        Ok(discovery) => discovery,
        Err(error) => {
            log::warn!("connect: cannot advertise on the network: {error}");
            let _ = status.send(ConnectStatus::Unavailable);
            return;
        }
    };
    let _ = status.send(ConnectStatus::Advertising);

    let mut session: Option<Session3> = None;

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            credentials = discovery.next() => {
                let Some(credentials) = credentials else {
                    log::warn!("connect: discovery stopped unexpectedly");
                    let _ = status.send(ConnectStatus::Unavailable);
                    break;
                };

                end(session.take());

                match connect(&client_id, &device_id, &name, config, credentials, status.clone()).await {
                    Ok(next) => session = Some(next),
                    Err(error) => {
                        log::warn!("connect: cannot start a connect session: {error:#}");
                        let _ = status.send(ConnectStatus::Disconnected);
                    }
                }
            }
        }
    }

    end(session.take());
    discovery.shutdown().await;
}

fn end(session: Option<Session3>) {
    let Some((spirc, spirc_task, monitor_task)) = session else {
        return;
    };
    let _ = spirc.shutdown();
    spirc_task.abort();
    monitor_task.abort();
}

async fn connect(
    client_id: &str,
    device_id: &str,
    name: &str,
    config: PlaybackConfig,
    credentials: Credentials,
    status: mpsc::UnboundedSender<ConnectStatus>,
) -> Result<Session3> {
    let session_config = SessionConfig {
        client_id: client_id.to_owned(),
        device_id: device_id.to_owned(),
        ..Default::default()
    };
    let session = Session::new(session_config, None);

    let player_config = PlayerConfig {
        bitrate: Bitrate::Bitrate320,
        normalisation: config.normalisation,
        gapless: config.gapless,
        ..Default::default()
    };

    let flush = Flush::default();
    let volume = Volume::new(config.gain);
    let sink_flush = flush.clone();
    let sink_volume = volume.clone();
    let player = Player::new(
        player_config,
        session.clone(),
        Box::new(NoOpVolume),
        move || BlazingSink::boxed(sink_flush, sink_volume),
    );

    let monitor = player.get_player_event_channel();
    let monitor_task = tokio::spawn(monitor_events(monitor, flush, status.clone()));

    let mixer: Arc<dyn Mixer> = Arc::new(ConnectMixer(volume));

    let connect_config = ConnectConfig {
        name: name.to_owned(),
        device_type: DeviceType::Speaker,
        ..Default::default()
    };

    let (spirc, task) = Spirc::new(connect_config, session, credentials, player, mixer)
        .await
        .context("cannot start the connect session")?;

    let _ = status.send(ConnectStatus::Connected);
    let spirc_task = tokio::spawn(task);

    Ok((spirc, spirc_task, monitor_task))
}

async fn monitor_events(
    mut events: PlayerEventChannel,
    flush: Flush,
    status: mpsc::UnboundedSender<ConnectStatus>,
) {
    while let Some(event) = events.recv().await {
        match event {
            PlayerEvent::Loading { .. } | PlayerEvent::Seeked { .. } => flush.request(),
            PlayerEvent::Playing { .. } => {
                let _ = status.send(ConnectStatus::Playing);
            }
            PlayerEvent::Paused { .. } | PlayerEvent::Stopped { .. } => {
                let _ = status.send(ConnectStatus::Paused);
            }
            _ => {}
        }
    }
}

fn device_id(name: &str) -> String {
    let digest = Sha256::digest(name.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}
