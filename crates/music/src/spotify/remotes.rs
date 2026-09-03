use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use futures::StreamExt as _;
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use librespot_core::Session;
use librespot_core::dealer::protocol::Message;
use librespot_protocol::connect::{
    Capabilities, Cluster as ClusterMessage, ClusterUpdate, Device, DeviceInfo, MemberType,
    PutStateReason, PutStateRequest, SetVolumeCommand,
};
use librespot_protocol::devices::DeviceType;
use librespot_protocol::player::{ContextPlayerOptions, PlayerState, Suppressions};
use protobuf::{EnumOrUnknown, Message as _, MessageField};
use tokio::sync::mpsc;

use crate::{RemoteDevice, RemoteKind, RemoteState, RemoteTransport, TRACK_PREFIX};

const CONNECTION_ID: HeaderName = HeaderName::from_static("x-spotify-connection-id");
const CONNECTIONS: &str = "hm://pusher/v1/connections/";
const CLUSTER: &str = "hm://connect-state/v1/cluster";
const CONNECTION_HEADER: &str = "Spotify-Connection-Id";
const SPIRC_VERSION: &str = "3.2.6";
const VOLUME_MAX: f32 = u16::MAX as f32;
const CONNECTION_WAIT: Duration = Duration::from_secs(20);
const DRIFT_LIMIT: i64 = 60_000;

pub struct Watcher {
    session: Session,
    connection_id: String,
}

impl Watcher {
    /// Announces Sonora as an observer-only Connect device and starts the dealer, so the
    /// cluster pushes arrive. Nothing here can play; it only watches and commands.
    pub async fn start(session: Session) -> Result<(Self, Remotes)> {
        let connections = session
            .dealer()
            .listen_for(CONNECTIONS, |message: Message| {
                message
                    .headers
                    .get(CONNECTION_HEADER)
                    .cloned()
                    .ok_or_else(|| {
                        librespot_core::Error::failed_precondition("no connection id header")
                    })
            })
            .map_err(|error| anyhow!("cannot listen for a connection id: {error}"))?;

        let clusters = session
            .dealer()
            .listen_for(CLUSTER, Message::from_raw)
            .map_err(|error| anyhow!("cannot listen for the cluster: {error}"))?;

        session
            .dealer()
            .start()
            .await
            .map_err(|error| anyhow!("cannot start the dealer: {error}"))?;

        let mut connections = connections;
        let connection_id = tokio::time::timeout(CONNECTION_WAIT, connections.next())
            .await
            .context("Spotify sent no connection id")?
            .context("the connection stream ended")?
            .map_err(|error| anyhow!("cannot read the connection id: {error}"))?;
        session.set_connection_id(&connection_id);

        let watcher = Self {
            session,
            connection_id,
        };
        let first = watcher.announce().await?;
        Ok((watcher, Remotes::new(clusters, first)))
    }

    async fn announce(&self) -> Result<RemoteState> {
        let response = self
            .session
            .spclient()
            .put_connect_state_request(&observer(&self.session))
            .await
            .map_err(|error| anyhow!("cannot announce the device: {error}"))?;
        let cluster =
            ClusterMessage::parse_from_bytes(&response).context("cannot read the cluster")?;
        Ok(read(&cluster, self.session.device_id()))
    }

    fn headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(CONNECTION_ID, HeaderValue::from_str(&self.connection_id)?);
        Ok(headers)
    }

    async fn command(&self, target: &str, command: serde_json::Value) -> Result<()> {
        let body = serde_json::to_vec(&serde_json::json!({ "command": command }))?;
        let mut headers = self.headers()?;
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let endpoint = format!(
            "/connect-state/v1/player/command/from/{}/to/{target}",
            self.session.device_id()
        );
        self.session
            .spclient()
            .request(&Method::POST, &endpoint, Some(headers), Some(&body))
            .await
            .map_err(|error| anyhow!("the device refused the command: {error}"))?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl RemoteTransport for Watcher {
    fn device_id(&self) -> String {
        self.session.device_id().to_owned()
    }

    async fn refresh(&self) -> Result<RemoteState> {
        self.announce().await
    }

    async fn play(&self, target: &str) -> Result<()> {
        self.command(target, serde_json::json!({ "endpoint": "resume" }))
            .await
    }

    async fn pause(&self, target: &str) -> Result<()> {
        self.command(target, serde_json::json!({ "endpoint": "pause" }))
            .await
    }

    async fn next(&self, target: &str) -> Result<()> {
        self.command(target, serde_json::json!({ "endpoint": "skip_next" }))
            .await
    }

    async fn previous(&self, target: &str) -> Result<()> {
        self.command(target, serde_json::json!({ "endpoint": "skip_prev" }))
            .await
    }

    async fn seek(&self, target: &str, position: Duration) -> Result<()> {
        let at = position.as_millis() as u64;
        self.command(
            target,
            serde_json::json!({ "endpoint": "seek_to", "value": at, "position": at }),
        )
        .await
    }

    async fn set_volume(&self, target: &str, level: f32) -> Result<()> {
        let mut command = SetVolumeCommand::new();
        command.volume = (level.clamp(0., 1.) * VOLUME_MAX).round() as i32;
        let endpoint = format!(
            "/connect-state/v1/connect/volume/from/{}/to/{target}",
            self.session.device_id()
        );
        self.session
            .spclient()
            .request_with_protobuf(&Method::PUT, &endpoint, Some(self.headers()?), &command)
            .await
            .map_err(|error| anyhow!("the device refused the volume: {error}"))?;
        Ok(())
    }

    async fn play_track(&self, target: &str, track_id: &str, at: Duration) -> Result<()> {
        let uri = format!("{TRACK_PREFIX}{track_id}");
        self.command(
            target,
            serde_json::json!({
                "endpoint": "play",
                "context": {
                    "uri": uri,
                    "url": format!("context://{uri}"),
                },
                "play_origin": { "feature_identifier": "harmony" },
                "options": { "seek_to": at.as_millis() as u64 },
            }),
        )
        .await
    }

    /// Hands the current playback to `target`. Spotify only accepts a transfer whose `from` is
    /// the device that holds playback, and refuses one carrying restore options, so the body
    /// stays empty and the device restores its own state.
    async fn transfer(&self, active: Option<&str>, target: &str) -> Result<()> {
        let from = active.unwrap_or(target);
        self.session
            .spclient()
            .transfer(from, target, None)
            .await
            .map_err(|error| anyhow!("cannot move playback to the device: {error}"))?;
        Ok(())
    }

    async fn release(&self) -> Result<()> {
        self.session
            .spclient()
            .delete_connect_state_request()
            .await
            .map_err(|error| anyhow!("cannot withdraw the device: {error}"))?;
        Ok(())
    }
}

pub struct Remotes {
    first: Option<RemoteState>,
    updates: mpsc::UnboundedReceiver<RemoteState>,
    _pump: tokio::task::JoinHandle<()>,
}

impl Remotes {
    fn new(
        clusters: librespot_core::dealer::manager::BoxedStreamResult<ClusterUpdate>,
        first: RemoteState,
    ) -> Self {
        let (sender, updates) = mpsc::unbounded_channel();
        let me = first.own_id.clone();
        let _pump = tokio::spawn(async move {
            let mut clusters = clusters;
            while let Some(update) = clusters.next().await {
                let Ok(update) = update else { continue };
                let Some(cluster) = update.cluster.into_option() else {
                    continue;
                };
                if sender.send(read(&cluster, &me)).is_err() {
                    break;
                }
            }
        });

        Self {
            first: Some(first),
            updates,
            _pump,
        }
    }
}

#[async_trait::async_trait]
impl crate::RemoteUpdates for Remotes {
    async fn next(&mut self) -> Option<RemoteState> {
        if let Some(first) = self.first.take() {
            return Some(first);
        }
        self.updates.recv().await
    }
}

fn read(cluster: &ClusterMessage, own_id: &str) -> RemoteState {
    let active = match cluster.active_device_id.is_empty() {
        true => None,
        false => Some(cluster.active_device_id.clone()),
    };

    let mut devices: Vec<RemoteDevice> = cluster
        .device
        .iter()
        .filter(|(id, device)| id.as_str() != own_id && device.can_play)
        .map(|(id, device)| RemoteDevice {
            id: id.clone(),
            name: device.name.clone(),
            kind: kind(device),
            volume: match volume_disabled(device) {
                true => None,
                false => Some(device.volume as f32 / VOLUME_MAX),
            },
            active: Some(id) == active.as_ref(),
        })
        .collect();
    devices.sort_by(|left, right| left.name.cmp(&right.name));

    let player = &cluster.player_state;
    let track = player
        .track
        .as_ref()
        .and_then(|track| track.uri.strip_prefix(TRACK_PREFIX))
        .map(str::to_owned);

    let playing = player.is_playing && !player.is_paused;

    RemoteState {
        own_id: own_id.to_owned(),
        devices,
        active,
        track,
        playing,
        buffering: player.is_buffering,
        position: position(cluster, playing),
        duration: Duration::from_millis(player.duration.max(0) as u64),
        shuffle: player
            .options
            .as_ref()
            .map(|options| options.shuffling_context)
            .unwrap_or_default(),
    }
}

/// `position_as_of_timestamp` is where the track stood at `player_state.timestamp`, which is
/// server time and older than the push. The cluster's own `server_timestamp_ms` says how much
/// older, so the two together give the position at the moment Spotify assembled the state.
fn position(cluster: &ClusterMessage, playing: bool) -> Duration {
    let player = &cluster.player_state;
    let at = Duration::from_millis(player.position_as_of_timestamp.max(0) as u64);
    if !playing {
        return at;
    }

    let since = cluster
        .server_timestamp_ms
        .checked_sub(player.timestamp)
        .filter(|drift| (0..DRIFT_LIMIT).contains(drift))
        .unwrap_or_default();
    at + Duration::from_millis(since as u64)
}

fn kind(device: &DeviceInfo) -> RemoteKind {
    match device.device_type.enum_value_or_default() {
        DeviceType::COMPUTER => RemoteKind::Computer,
        DeviceType::SMARTPHONE | DeviceType::TABLET => RemoteKind::Phone,
        DeviceType::TV | DeviceType::CAST_VIDEO | DeviceType::GAME_CONSOLE => RemoteKind::Screen,
        DeviceType::AUTOMOBILE => RemoteKind::Car,
        _ => RemoteKind::Speaker,
    }
}

fn volume_disabled(device: &DeviceInfo) -> bool {
    device
        .capabilities
        .as_ref()
        .map(|capabilities| capabilities.disable_volume)
        .unwrap_or_default()
}

fn observer(session: &Session) -> PutStateRequest {
    let device_info = DeviceInfo {
        can_play: false,
        volume: u16::MAX as u32,
        name: crate::REMOTE_NAME.to_owned(),
        device_id: session.device_id().to_owned(),
        device_type: EnumOrUnknown::new(DeviceType::COMPUTER),
        device_software_version: env!("CARGO_PKG_VERSION").to_owned(),
        spirc_version: SPIRC_VERSION.to_owned(),
        client_id: session.client_id(),
        capabilities: MessageField::some(Capabilities {
            can_be_player: false,
            is_controllable: false,
            is_observable: true,
            needs_full_player_state: true,
            gaia_eq_connect_id: true,
            supports_gzip_pushes: true,
            command_acks: true,
            hidden: true,
            volume_steps: 64,
            supported_types: vec!["audio/track".into(), "audio/episode".into()],
            ..Default::default()
        }),
        ..Default::default()
    };

    PutStateRequest {
        member_type: EnumOrUnknown::new(MemberType::CONNECT_STATE),
        put_state_reason: EnumOrUnknown::new(PutStateReason::NEW_DEVICE),
        device: MessageField::some(Device {
            device_info: MessageField::some(device_info),
            player_state: MessageField::some(PlayerState {
                session_id: session.session_id(),
                playback_speed: 1.,
                is_system_initiated: true,
                options: MessageField::some(ContextPlayerOptions::new()),
                suppressions: MessageField::some(Suppressions::new()),
                ..Default::default()
            }),
            ..Default::default()
        }),
        client_side_timestamp: now_ms(),
        ..Default::default()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default()
}
