use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::audio::{Output, RAMP, SmoothGain, Volume};
use crate::subsonic::client::SubsonicClient;
use crate::{PlaybackConfig, PlaybackEvent, PlaybackEvents, PlaybackFactory, Player};

const POLL: Duration = Duration::from_millis(20);

enum Command {
    Load { id: String, at: Option<Duration> },
    Preload { id: String },
    Play,
    Pause,
    Seek(Duration),
    Gain(f32),
}

pub struct Factory {
    client: SubsonicClient,
}

impl Factory {
    pub fn new(client: SubsonicClient) -> Self {
        Self { client }
    }
}

impl PlaybackFactory for Factory {
    fn start(&self, config: PlaybackConfig) -> (Box<dyn Player>, Box<dyn PlaybackEvents>) {
        let (commands, command_rx) = unbounded_channel();
        let (events, event_rx) = unbounded_channel();
        let client = self.client.clone();
        let spawned = std::thread::Builder::new()
            .name("subsonic-playback".to_owned())
            .spawn(move || run(client, config, command_rx, events));
        if let Err(error) = spawned {
            log::error!("playback: cannot spawn subsonic engine thread: {error}");
        }
        (Box::new(Engine { commands }), Box::new(Events(event_rx)))
    }
}

struct Engine {
    commands: UnboundedSender<Command>,
}

impl Player for Engine {
    fn load(&self, track_id: &str, _seamless: bool) -> Result<()> {
        self.commands
            .send(Command::Load {
                id: track_id.to_owned(),
                at: None,
            })
            .context("cannot reach subsonic playback engine")
    }

    fn load_paused_at(&self, track_id: &str, at: Duration) -> Result<()> {
        self.commands
            .send(Command::Load {
                id: track_id.to_owned(),
                at: Some(at),
            })
            .context("cannot reach subsonic playback engine")
    }

    fn preload(&self, track_id: &str) -> Result<()> {
        self.commands
            .send(Command::Preload {
                id: track_id.to_owned(),
            })
            .context("cannot reach subsonic playback engine")
    }

    fn play(&self) {
        self.commands.send(Command::Play).ok();
    }

    fn pause(&self) {
        self.commands.send(Command::Pause).ok();
    }

    fn seek(&self, position: Duration) {
        self.commands.send(Command::Seek(position)).ok();
    }

    fn set_gain(&self, gain: f32) {
        self.commands.send(Command::Gain(gain)).ok();
    }
}

struct Events(UnboundedReceiver<PlaybackEvent>);

#[async_trait]
impl PlaybackEvents for Events {
    async fn next(&mut self) -> Option<PlaybackEvent> {
        self.0.recv().await
    }
}

#[derive(Clone)]
struct Loaded {
    data: Arc<Vec<u8>>,
    duration: Option<Duration>,
}

struct Slot {
    id: String,
    length: Option<Duration>,
    envelope: Volume,
    gain: f32,
}

impl Slot {
    fn mute(&self) {
        self.envelope.set(0.0);
    }

    fn unmute(&self) {
        self.envelope.set(self.gain);
    }
}

enum Kind {
    Play,
    Ahead,
}

struct Fetched {
    epoch: u64,
    id: String,
    kind: Kind,
    result: Result<Loaded>,
}

fn run(
    client: SubsonicClient,
    config: PlaybackConfig,
    commands: UnboundedReceiver<Command>,
    events: UnboundedSender<PlaybackEvent>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            log::error!("playback: cannot build subsonic engine runtime: {error}");
            return;
        }
    };
    runtime.block_on(engine_loop(client, config, commands, events));
}

async fn engine_loop(
    client: SubsonicClient,
    config: PlaybackConfig,
    mut commands: UnboundedReceiver<Command>,
    events: UnboundedSender<PlaybackEvent>,
) {
    let output = match Output::open(Volume::new(config.gain)) {
        Ok(output) => output,
        Err(error) => {
            log::error!("playback: cannot open audio output: {error:#}");
            return;
        }
    };
    let sink = output.sink().clone();
    sink.pause();

    let (fetched, mut arrivals) = unbounded_channel::<Fetched>();
    let mut ticker = tokio::time::interval(POLL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let report_every = (config.position_interval.as_millis() / POLL.as_millis()).max(1) as u32;
    let mut ticks = 0u32;

    let mut playing = false;
    let mut autostart = true;
    let mut hold: Option<Duration> = None;
    let mut epoch = 0u64;
    let mut pending: Option<u64> = None;
    let mut inflight: Option<tokio::task::AbortHandle> = None;
    let mut current: Option<Slot> = None;
    let mut queued: Option<Slot> = None;
    let mut ahead: Option<(String, Loaded)> = None;
    let mut prev_len = 0usize;

    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break };
                match command {
                    Command::Load { id, at } => {
                        if at.is_none() && current.as_ref().is_some_and(|slot| slot.id == id) {
                            playing = true;
                            autostart = true;
                            if let Some(slot) = &current {
                                slot.unmute();
                                if let Some(length) = slot.length {
                                    events.send(PlaybackEvent::Length(length)).ok();
                                }
                            }
                            sink.play();
                            events.send(PlaybackEvent::Playing(sink.get_pos())).ok();
                            continue;
                        }
                        epoch += 1;
                        if let Some(handle) = inflight.take() {
                            handle.abort();
                        }
                        let cached = ahead
                            .take_if(|(cached, _)| *cached == id)
                            .map(|(_, loaded)| loaded);
                        pending = None;
                        if cached.is_none() {
                            ahead = None;
                            pending = Some(epoch);
                            inflight = Some(spawn(&client, id.clone(), epoch, Kind::Play, &fetched));
                        }
                        events.send(PlaybackEvent::Loading(at.unwrap_or_default())).ok();
                        silence(&sink, current.as_ref()).await;
                        current = None;
                        queued = None;
                        playing = false;
                        autostart = at.is_none();
                        hold = at;
                        prev_len = 0;
                        let Some(loaded) = cached else { continue };
                        match begin(&sink, &id, &loaded, autostart, hold.take()) {
                            Ok(slot) => {
                                announce(&events, &slot, autostart, at.unwrap_or_default());
                                prev_len = sink.len();
                                current = Some(slot);
                                playing = autostart;
                            }
                            Err(error) => {
                                log::warn!("playback: cannot decode subsonic track {id}: {error:#}");
                                events.send(PlaybackEvent::Unavailable).ok();
                            }
                        }
                    }
                    Command::Preload { id } => {
                        let known = current.as_ref().is_some_and(|slot| slot.id == id)
                            || queued.as_ref().is_some_and(|slot| slot.id == id)
                            || ahead.as_ref().is_some_and(|(cached, _)| *cached == id);
                        if known || current.is_none() {
                            continue;
                        }
                        spawn(&client, id, epoch, Kind::Ahead, &fetched);
                    }
                    Command::Play => {
                        autostart = true;
                        if let Some(slot) = &current {
                            sink.play();
                            slot.unmute();
                            playing = true;
                            events.send(PlaybackEvent::Playing(sink.get_pos())).ok();
                        }
                    }
                    Command::Pause => {
                        autostart = false;
                        playing = false;
                        let position = sink.get_pos();
                        if let Some(slot) = &current {
                            slot.mute();
                            await_drain(&sink).await;
                            sink.pause();
                        }
                        events.send(PlaybackEvent::Paused(position)).ok();
                    }
                    Command::Seek(position) => match &current {
                        None if hold.is_some() => hold = Some(position),
                        None => {}
                        Some(slot) => {
                            slot.mute();
                            await_drain(&sink).await;
                            if let Err(error) = sink.try_seek(position) {
                                log::warn!("playback: cannot seek subsonic track: {error}");
                            }
                            if playing {
                                slot.unmute();
                            }
                            events.send(PlaybackEvent::Position(sink.get_pos())).ok();
                        }
                    },
                    Command::Gain(level) => output.set_volume(level),
                }
            }
            arrival = arrivals.recv() => {
                let Some(Fetched { epoch: at, id, kind, result }) = arrival else { break };
                if at != epoch {
                    continue;
                }
                match kind {
                    Kind::Play => {
                        if pending != Some(at) {
                            continue;
                        }
                        pending = None;
                        inflight = None;
                        let at = hold.take();
                        match result.and_then(|loaded| begin(&sink, &id, &loaded, autostart, at)) {
                            Ok(slot) => {
                                announce(&events, &slot, autostart, at.unwrap_or_default());
                                prev_len = sink.len();
                                current = Some(slot);
                                playing = autostart;
                            }
                            Err(error) => {
                                log::warn!("playback: cannot load subsonic track {id}: {error:#}");
                                events.send(PlaybackEvent::Unavailable).ok();
                            }
                        }
                    }
                    Kind::Ahead => {
                        let Ok(loaded) = result else {
                            continue;
                        };
                        if current.is_some() && queued.is_none() {
                            match append(&sink, &id, &loaded, false) {
                                Ok(slot) => {
                                    queued = Some(slot);
                                    prev_len = sink.len();
                                }
                                Err(error) => {
                                    log::warn!("playback: cannot decode subsonic preload {id}: {error:#}")
                                }
                            }
                        }
                        ahead = Some((id, loaded));
                    }
                }
            }
            _ = ticker.tick() => {
                let len = sink.len();
                ticks += 1;
                if current.is_some() && playing && len < prev_len {
                    ticks = 0;
                    events.send(PlaybackEvent::Ended).ok();
                    current = queued.take();
                    ahead = None;
                    playing = current.is_some();
                    match &current {
                        Some(slot) => {
                            if let Some(length) = slot.length {
                                events.send(PlaybackEvent::Length(length)).ok();
                            }
                            events.send(PlaybackEvent::Position(sink.get_pos())).ok();
                        }
                        None => log::debug!("playback: subsonic track ended with nothing queued"),
                    }
                } else if playing && ticks >= report_every {
                    ticks = 0;
                    events.send(PlaybackEvent::Position(sink.get_pos())).ok();
                }
                prev_len = len;
            }
        }
    }
}

fn spawn(
    client: &SubsonicClient,
    id: String,
    epoch: u64,
    kind: Kind,
    fetched: &UnboundedSender<Fetched>,
) -> tokio::task::AbortHandle {
    let client = client.clone();
    let fetched = fetched.clone();
    tokio::spawn(async move {
        let result = fetch(&client, &id).await;
        fetched
            .send(Fetched {
                epoch,
                id,
                kind,
                result,
            })
            .ok();
    })
    .abort_handle()
}

async fn silence(sink: &rodio::Sink, slot: Option<&Slot>) {
    let Some(slot) = slot else {
        sink.clear();
        return;
    };
    slot.mute();
    await_drain(sink).await;
    sink.clear();
}

async fn await_drain(sink: &rodio::Sink) {
    if sink.is_paused() {
        return;
    }
    tokio::time::sleep(RAMP).await;
}

fn begin(
    sink: &rodio::Sink,
    id: &str,
    loaded: &Loaded,
    start: bool,
    at: Option<Duration>,
) -> Result<Slot> {
    sink.clear();
    let slot = append(sink, id, loaded, true)?;
    if let Some(at) = at
        && let Err(error) = sink.try_seek(at)
    {
        log::warn!(
            "playback: cannot start subsonic track {id} at {}s: {error}",
            at.as_secs()
        );
    }
    match start {
        true => sink.play(),
        false => sink.pause(),
    }
    Ok(slot)
}

fn append(sink: &rodio::Sink, id: &str, loaded: &Loaded, fade: bool) -> Result<Slot> {
    let envelope = Volume::new(1.0);
    let initial = match fade {
        true => 0.0,
        false => 1.0,
    };
    let source = decode(loaded.data.clone())?;
    sink.append(SmoothGain::new(source, envelope.clone(), initial, RAMP));
    let _ = id;
    Ok(Slot {
        id: id.to_owned(),
        length: loaded.duration,
        envelope,
        gain: 1.0,
    })
}

fn announce(
    events: &UnboundedSender<PlaybackEvent>,
    slot: &Slot,
    playing: bool,
    position: Duration,
) {
    if let Some(length) = slot.length {
        events.send(PlaybackEvent::Length(length)).ok();
    }
    let event = match playing {
        true => PlaybackEvent::Playing(position),
        false => PlaybackEvent::Paused(position),
    };
    events.send(event).ok();
}

async fn fetch(client: &SubsonicClient, id: &str) -> Result<Loaded> {
    let (bytes, duration) = client.stream_bytes(id).await?;
    Ok(Loaded {
        data: Arc::new(bytes),
        duration: Some(duration),
    })
}

fn decode(data: Arc<Vec<u8>>) -> Result<impl rodio::Source + Send + 'static> {
    let length = data.len() as u64;
    rodio::Decoder::builder()
        .with_data(Cursor::new(Bytes(data)))
        .with_byte_len(length)
        .with_seekable(true)
        .build()
        .context("cannot decode subsonic audio")
}

struct Bytes(Arc<Vec<u8>>);

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
