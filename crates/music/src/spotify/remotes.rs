use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use futures::StreamExt as _;
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use librespot_core::dealer::manager::{Reply, RequestReply};
use librespot_core::dealer::protocol::{Command, Message, Request};
use librespot_core::{Session, SpotifyId};
use librespot_protocol::connect::{
    Capabilities, Cluster as ClusterMessage, ClusterUpdate, Device, DeviceInfo, MemberType,
    PutStateReason, PutStateRequest, SetVolumeCommand,
};
use librespot_protocol::devices::DeviceType;
use librespot_protocol::player::{
    ContextPlayerOptions, PlayOrigin, PlayerState, ProvidedTrack, Suppressions,
};
use librespot_protocol::transfer_state::TransferState;
use protobuf::{EnumOrUnknown, Message as _, MessageField};
use tokio::sync::mpsc;

use crate::{
    Handover, HostCommand, HostRepeat, HostState, RemoteDevice, RemoteKind, RemoteState,
    RemoteTransport, TRACK_PREFIX,
};

const CONNECTION_ID: HeaderName = HeaderName::from_static("x-spotify-connection-id");
const CONNECTIONS: &str = "hm://pusher/v1/connections/";
const CLUSTER: &str = "hm://connect-state/v1/cluster";
const CONNECTION_HEADER: &str = "Spotify-Connection-Id";
const SPIRC_VERSION: &str = "3.2.6";
const VOLUME_MAX: f32 = u16::MAX as f32;
const COMMANDS: &str = "hm://connect-state/v1/player/command";
/// Volume arrives on its own endpoint rather than as a player command.
const VOLUME: &str = "hm://connect-state/v1/connect/volume";
const CONNECTION_WAIT: Duration = Duration::from_secs(20);
const DRIFT_LIMIT: i64 = 60_000;

pub struct Watcher {
    session: Session,
    connection_id: String,
    playable: Mutex<bool>,
    /// The state last published, so a change can be layered onto it rather than rebuilt.
    published: Mutex<PlayerState>,
    hosting: Mutex<bool>,
    /// Sonora's own volume in Spotify's 0..=u16::MAX scale, so a phone's slider reads what the
    /// player bar shows instead of sitting at full.
    volume: Mutex<u32>,
}

impl Watcher {
    /// Announces Sonora to Spotify and starts the dealer, so the cluster pushes and any player
    /// commands arrive. `playable` decides whether Sonora offers itself as a playback target;
    /// a control-only watch stays out of every other device's list.
    pub async fn start(session: Session, playable: bool) -> Result<(Self, Remotes, Orders)> {
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

        // registered even when hidden: the handler cannot be added once the dealer is running
        let orders = session
            .dealer()
            .handle_for(COMMANDS)
            .map_err(|error| anyhow!("cannot listen for player commands: {error}"))?;

        let volumes = session
            .dealer()
            .listen_for(VOLUME, Message::from_raw)
            .map_err(|error| anyhow!("cannot listen for volume changes: {error}"))?;

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

        let published = Mutex::new(fresh(&session.session_id()));
        let watcher = Self {
            session,
            connection_id,
            playable: Mutex::new(playable),
            published,
            hosting: Mutex::new(false),
            volume: Mutex::new(u16::MAX as u32),
        };
        let first = watcher.announce().await?;
        Ok((
            watcher,
            Remotes::new(clusters, first),
            Orders::new(orders, volumes),
        ))
    }

    async fn announce(&self) -> Result<RemoteState> {
        let request = self.request(false);
        let response = self
            .session
            .spclient()
            .put_connect_state_request(&request)
            .await
            .map_err(|error| anyhow!("cannot announce the device: {error}"))?;
        let cluster =
            ClusterMessage::parse_from_bytes(&response).context("cannot read the cluster")?;
        Ok(read(&cluster, self.session.device_id()))
    }

    /// Re-announces Sonora with a different `playable` flag without restarting the dealer.
    /// The handlers stay registered; only the device announcement changes.
    pub async fn reannounce(&self, playable: bool) -> Result<RemoteState> {
        if let Ok(mut held) = self.playable.lock() {
            *held = playable;
        }
        self.announce().await
    }

    /// The announce body. `active` is what tells Spotify that Sonora, not the handing device,
    /// is now playing — a transfer stays stuck on "Connecting" until an active state arrives.
    fn request(&self, active: bool) -> PutStateRequest {
        let player = self
            .published
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default();
        let volume = self.volume.lock().map(|held| *held).unwrap_or_default();
        let playable = self.playable.lock().map(|held| *held).unwrap_or(true);

        PutStateRequest {
            member_type: EnumOrUnknown::new(MemberType::CONNECT_STATE),
            put_state_reason: EnumOrUnknown::new(match active {
                true => PutStateReason::PLAYER_STATE_CHANGED,
                false => PutStateReason::NEW_DEVICE,
            }),
            is_active: active,
            started_playing_at: match active {
                true => now_ms(),
                false => 0,
            },
            device: MessageField::some(Device {
                device_info: MessageField::some(info(&self.session, playable, volume)),
                player_state: MessageField::some(player),
                ..Default::default()
            }),
            client_side_timestamp: now_ms(),
            ..Default::default()
        }
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

    async fn publish(&self, state: &HostState) -> Result<()> {
        if let Ok(mut held) = self.published.lock() {
            *held = player_state(state, &self.session.session_id(), &held);
        }
        if let Ok(mut hosting) = self.hosting.lock() {
            *hosting = true;
        }

        if let Ok(mut held) = self.volume.lock() {
            *held = (state.volume.clamp(0., 1.) * VOLUME_MAX).round() as u32;
        }

        let request = self.request(true);
        self.session
            .spclient()
            .put_connect_state_request(&request)
            .await
            .map_err(|error| anyhow!("cannot publish the player state: {error}"))?;
        Ok(())
    }

    async fn resign(&self) -> Result<()> {
        let hosting = self
            .hosting
            .lock()
            .map(|mut held| std::mem::replace(&mut *held, false))
            .unwrap_or_default();
        if !hosting {
            return Ok(());
        }
        if let Ok(mut held) = self.published.lock() {
            *held = fresh(&self.session.session_id());
        }

        // `notify: false`, as librespot's own client does: a notified withdrawal tells every other
        // device Sonora is gone rather than merely idle.
        self.session
            .spclient()
            .put_connect_state_inactive(false)
            .await
            .map_err(|error| anyhow!("cannot hand playback back: {error}"))?;

        // Going inactive takes Sonora out of the picker, so it announces itself again as an idle
        // target. Without this, handing playback back here is indistinguishable from quitting.
        self.announce()
            .await
            .context("cannot offer Sonora again after handing playback back")?;
        Ok(())
    }

    async fn reannounce(&self, playable: bool) -> Result<RemoteState> {
        Watcher::reannounce(self, playable).await
    }
}

/// Incoming commands from whichever device is driving Sonora. Every request is answered, so a
/// phone never waits on an ack Sonora silently dropped.
pub struct Orders {
    orders: mpsc::UnboundedReceiver<HostCommand>,
    _pump: tokio::task::JoinHandle<()>,
}

impl Orders {
    fn new(
        mut requests: librespot_core::dealer::manager::BoxedStream<RequestReply>,
        mut volumes: librespot_core::dealer::manager::BoxedStreamResult<SetVolumeCommand>,
    ) -> Self {
        let (sender, orders) = mpsc::unbounded_channel();
        let _pump = tokio::spawn(async move {
            loop {
                tokio::select! {
                    request = requests.next() => {
                        let Some((request, reply)) = request else { return };
                        let answer = match translate(request) {
                            Some(command) => match sender.send(command) {
                                Ok(()) => Reply::Success,
                                Err(_) => return,
                            },
                            None => Reply::Success,
                        };
                        let _ = reply.send(answer);
                    }
                    // volume rides its own endpoint and expects no ack
                    volume = volumes.next() => {
                        let Some(Ok(volume)) = volume else { continue };
                        let level = (volume.volume.max(0) as f32 / VOLUME_MAX).clamp(0., 1.);
                        if sender.send(HostCommand::Volume(level)).is_err() {
                            return;
                        }
                    }
                }
            }
        });

        Self { orders, _pump }
    }
}

#[async_trait::async_trait]
impl crate::HostCommands for Orders {
    async fn next(&mut self) -> Option<HostCommand> {
        self.orders.recv().await
    }
}

/// Turns a dealer request into something `state` can act on without knowing the wire.
fn translate(request: Request) -> Option<HostCommand> {
    match request.command {
        Command::Transfer(transfer) => Some(HostCommand::Take(handover(transfer.data?))),
        Command::Play(play) => {
            let context = play.context.uri.clone();
            let at = play.options.seek_to.unwrap_or_default();
            let skip = play.options.skip_to.as_ref();
            // A playlist played from a phone names no track: it sends the context plus, at most,
            // an index. The first track is the sane default when neither is given.
            let track = skip
                .and_then(|skip| skip.track_uri.clone())
                .and_then(|uri| base62(&uri))
                .or_else(|| first_of(&play.context));
            Some(HostCommand::Take(Handover {
                track,
                index: skip
                    .and_then(|skip| skip.track_index)
                    .map(|index| index as usize),
                context,
                position: Duration::from_millis(at as u64),
                paused: play.options.initially_paused.unwrap_or_default(),
                shuffle: false,
            }))
        }
        Command::Resume(_) => Some(HostCommand::Play),
        Command::Pause(_) => Some(HostCommand::Pause),
        Command::SkipNext(_) => Some(HostCommand::Next),
        Command::SkipPrev(_) => Some(HostCommand::Previous),
        Command::SeekTo(seek) => Some(HostCommand::Seek(Duration::from_millis(seek.value as u64))),
        Command::SetShufflingContext(set) => Some(HostCommand::Shuffle(set.value)),
        Command::SetRepeatingContext(set) => Some(HostCommand::Repeat(match set.value {
            true => HostRepeat::Context,
            false => HostRepeat::Off,
        })),
        Command::SetRepeatingTrack(set) => Some(HostCommand::Repeat(match set.value {
            true => HostRepeat::Track,
            false => HostRepeat::Off,
        })),
        Command::AddToQueue(add) => base62(&add.track.uri).map(HostCommand::Enqueue),
        Command::SetQueue(_) | Command::SetOptions(_) | Command::UpdateContext(_) => None,
        Command::Unknown(ref json) => {
            log::debug!("remotes: an unhandled command arrived: {json}");
            None
        }
    }
}

/// Reads a transfer. The handing device leaves `ContextTrack::uri` empty and puts the identity
/// in the raw `gid` plus a `metadata` map, so both are tried.
fn handover(transfer: TransferState) -> Handover {
    let playback = transfer.playback.as_ref();
    let paused = playback.map(|held| held.is_paused()).unwrap_or_default();
    let current = playback.and_then(|held| held.current_track.as_ref());

    let track = current.and_then(|track| {
        if let Some(id) = track.uri.as_deref().and_then(base62) {
            return Some(id);
        }
        if let Some(id) = track.metadata.get("entity_uri").and_then(|uri| base62(uri)) {
            return Some(id);
        }
        SpotifyId::from_raw(track.gid())
            .ok()
            .and_then(|id| id.to_base62().ok())
    });

    Handover {
        track,
        index: None,
        context: transfer
            .current_session
            .as_ref()
            .and_then(|session| session.context.as_ref())
            .and_then(|context| context.uri.clone()),
        position: extrapolated(&transfer, paused),
        paused,
        shuffle: transfer
            .options
            .as_ref()
            .map(|options| options.shuffling_context())
            .unwrap_or_default(),
    }
}

/// `restore_position: "extrapolate"` asks the receiving device to advance the reported position
/// by however long the transfer took to arrive.
fn extrapolated(transfer: &TransferState, paused: bool) -> Duration {
    let Some(playback) = transfer.playback.as_ref() else {
        return Duration::ZERO;
    };
    let at = Duration::from_millis(playback.position_as_of_timestamp().max(0) as u64);
    if paused {
        return at;
    }

    let drift = (now_ms() as i64)
        .checked_sub(playback.timestamp())
        .filter(|drift| (0..DRIFT_LIMIT).contains(drift))
        .unwrap_or_default();
    at + Duration::from_millis(drift as u64)
}

/// The first track of a context, when one was handed over without naming a track. Spotify
/// usually sends the pages empty and expects the receiver to resolve them, so this only helps
/// when they came filled in.
fn first_of(context: &librespot_protocol::context::Context) -> Option<String> {
    context
        .pages
        .iter()
        .flat_map(|page| page.tracks.iter())
        .find_map(|track| {
            base62(&track.uri.clone().unwrap_or_default()).or_else(|| {
                SpotifyId::from_raw(track.gid())
                    .ok()
                    .and_then(|id| id.to_base62().ok())
            })
        })
}

fn base62(uri: &str) -> Option<String> {
    let id = uri.strip_prefix(TRACK_PREFIX)?;
    (!id.is_empty()).then(|| id.to_owned())
}

fn fresh(session_id: &str) -> PlayerState {
    PlayerState {
        session_id: session_id.to_owned(),
        playback_speed: 1.,
        is_system_initiated: true,
        play_origin: MessageField::some(PlayOrigin::new()),
        suppressions: MessageField::some(Suppressions::new()),
        options: MessageField::some(ContextPlayerOptions::new()),
        ..Default::default()
    }
}

/// Sonora's state in the shape every other device's UI reads.
fn player_state(state: &HostState, session_id: &str, held: &PlayerState) -> PlayerState {
    let uri = |id: &String| format!("{TRACK_PREFIX}{id}");
    let provided = |id: &String| ProvidedTrack {
        uri: uri(id),
        provider: "context".to_owned(),
        ..Default::default()
    };

    PlayerState {
        timestamp: now_ms() as i64,
        context_uri: state.context.clone().unwrap_or_default(),
        context_url: state
            .context
            .as_ref()
            .map(|context| format!("context://{context}"))
            .unwrap_or_default(),
        track: MessageField::from_option(state.track.as_ref().map(provided)),
        playback_id: held.playback_id.clone(),
        playback_speed: 1.,
        position_as_of_timestamp: state.position.as_millis() as i64,
        duration: state.duration.as_millis() as i64,
        is_playing: state.playing || state.buffering,
        is_paused: !state.playing,
        is_buffering: state.buffering,
        is_system_initiated: true,
        options: MessageField::some(ContextPlayerOptions {
            shuffling_context: state.shuffle,
            repeating_context: matches!(state.repeat, Some(HostRepeat::Context)),
            repeating_track: matches!(state.repeat, Some(HostRepeat::Track)),
            ..Default::default()
        }),
        suppressions: MessageField::some(Suppressions::new()),
        play_origin: held
            .play_origin
            .clone()
            .into_option()
            .map(MessageField::some)
            .unwrap_or_else(|| MessageField::some(PlayOrigin::new())),
        prev_tracks: state.past.iter().map(provided).collect(),
        next_tracks: state.upcoming.iter().map(provided).collect(),
        session_id: session_id.to_owned(),
        ..Default::default()
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

/// How Sonora describes itself. A control-only watch is `hidden` and cannot play, so it never
/// shows up in anyone's device list; a playable one advertises the commands it can honour.
fn info(session: &Session, playable: bool, volume: u32) -> DeviceInfo {
    DeviceInfo {
        can_play: playable,
        volume,
        name: crate::REMOTE_NAME.to_owned(),
        device_id: session.device_id().to_owned(),
        device_type: EnumOrUnknown::new(DeviceType::COMPUTER),
        device_software_version: env!("CARGO_PKG_VERSION").to_owned(),
        spirc_version: SPIRC_VERSION.to_owned(),
        client_id: session.client_id(),
        capabilities: MessageField::some(Capabilities {
            can_be_player: playable,
            is_controllable: playable,
            supports_transfer_command: playable,
            supports_command_request: playable,
            supports_playlist_v2: playable,
            supports_set_options_command: playable,
            hidden: !playable,
            is_observable: true,
            needs_full_player_state: true,
            gaia_eq_connect_id: true,
            supports_gzip_pushes: true,
            command_acks: true,
            volume_steps: 64,
            supported_types: vec!["audio/track".into(), "audio/episode".into()],
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default()
}
