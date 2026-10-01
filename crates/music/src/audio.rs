use std::num::NonZero;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use anyhow::{Context as _, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::source::SeekError;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source};

use crate::equalizer::{Equalized, Equalizer};
use crate::spectrum::{Spectrum, Tap};

pub const RAMP: Duration = Duration::from_millis(25);
const BUFFER: Duration = Duration::from_millis(50);

/// The device stream every output in the process mixes into, so the sound server sees Sonora as
/// one client however many engines are running. It runs at the rate of whatever played last and
/// closes when the last output on it drops.
static SHARED: Mutex<Weak<Device>> = Mutex::new(Weak::new());

#[derive(Clone)]
pub struct Volume(Arc<AtomicU32>);

impl Volume {
    pub fn new(gain: f32) -> Self {
        Self(Arc::new(AtomicU32::new(gain.to_bits())))
    }

    pub fn set(&self, gain: f32) {
        self.0.store(gain.to_bits(), Ordering::Relaxed);
    }

    fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// What sits between the queue and the device: the equalizer, then the volume ramp, with the
/// spectrum tap listening at the end. Every engine builds one and hands it to the output.
pub struct Chain {
    pub volume: Volume,
    pub equalizer: Equalizer,
    pub spectrum: Spectrum,
}

/// One open stream on an output device, with the mixer the engines' chains play into.
struct Device {
    id: String,
    /// The rate the stream was opened for, whether or not the device could honour it.
    wanted: Option<u32>,
    rate: u32,
    failed: Arc<AtomicBool>,
    /// Taken when an output reopens the device at another rate, which closes the stream under
    /// every output still on it.
    stream: Mutex<Option<MixerDeviceSink>>,
}

impl Device {
    /// The shared stream on the default device, at `rate` when one is given. It opens a new
    /// stream when no output holds a healthy one there at that rate, closing the old stream on
    /// the same device first. A stream left on a device that is no longer the default stays with
    /// the outputs still using it until they reopen.
    fn shared(rate: Option<u32>) -> Result<Arc<Self>> {
        let device = cpal::default_host()
            .default_output_device()
            .context("no audio output device")?;
        let id = ident(&device);

        let mut shared = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
        let current = shared.upgrade().filter(|open| open.id == id);
        if let Some(open) = &current
            && open.live()
            && rate.is_none_or(|rate| open.fits(rate))
        {
            return Ok(open.clone());
        }
        if let Some(open) = current {
            open.close();
        }

        let open = Arc::new(Self::open(device, id, rate)?);
        *shared = Arc::downgrade(&open);
        Ok(open)
    }

    /// Opens the device at `rate` when it offers that rate and at its default otherwise.
    fn open(device: cpal::Device, id: String, rate: Option<u32>) -> Result<Self> {
        let default = device
            .default_output_config()
            .map_err(|error| anyhow::anyhow!("cannot read the output config: {error}"))?;
        let config = match rate {
            Some(rate) => at_rate(&device, &default, rate).unwrap_or_else(|| {
                log::info!("sink: the device does not offer {rate} Hz, resampling instead");
                default
            }),
            None => default,
        };

        log::info!(
            "sink: using {} at {} Hz, {} channels, {}",
            id,
            config.sample_rate(),
            config.channels(),
            config.sample_format()
        );

        let format = config.sample_format();
        let frames = (BUFFER.as_secs_f64() * config.sample_rate() as f64).round() as u32;
        let failed = Arc::new(AtomicBool::new(false));
        let stream_failed = failed.clone();
        let builder = DeviceSinkBuilder::default()
            .with_device(device)
            .with_config(&config.config())
            .with_buffer_size(cpal::BufferSize::Fixed(frames))
            .with_sample_format(format)
            .with_error_callback(move |error| match error {
                cpal::StreamError::BufferUnderrun => log::debug!("sink: buffer underrun"),
                error => {
                    log::warn!("sink: audio output failed: {error}");
                    stream_failed.store(true, Ordering::Release);
                }
            });
        let mut stream = builder
            .open_stream()
            .map_err(|error| anyhow::anyhow!("cannot open the audio output: {error}"))?;
        stream.log_on_drop(false);

        Ok(Self {
            id,
            wanted: rate,
            rate: config.sample_rate(),
            failed,
            stream: Mutex::new(Some(stream)),
        })
    }

    /// Whether a track at `rate` can play on this stream without a reopen. A rate the device
    /// turned down counts as fitting, so the output does not retry it on every track.
    fn fits(&self, rate: u32) -> bool {
        rate == self.rate || self.wanted == Some(rate)
    }

    /// Whether the stream is still open and has reported no error.
    fn live(&self) -> bool {
        !self.failed.load(Ordering::Acquire) && self.stream().is_some()
    }

    fn close(&self) {
        self.stream().take();
    }

    /// Puts one engine's chain into the mixer. A stream that has closed drops it.
    fn add(&self, source: impl Source + Send + 'static) {
        if let Some(stream) = self.stream().as_ref() {
            stream.mixer().add(source);
        }
    }

    fn stream(&self) -> MutexGuard<'_, Option<MixerDeviceSink>> {
        self.stream.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One engine's player on the shared device stream. Dropping it ends the player's source in the
/// mixer, and the stream closes once no output is left on it.
pub struct Output {
    chain: Chain,
    sink: Arc<rodio::Player>,
    device: Arc<Device>,
    /// Set when `fit` could not reopen the device, so the engine reports the output gone.
    broken: bool,
}

impl Output {
    /// Joins the stream on the default output device at whatever rate it runs and runs every
    /// sample through the equalizer and then the volume ramp before it reaches the mixer.
    pub fn open(chain: Chain) -> Result<Self> {
        let device = Device::shared(None)?;
        let sink = attach(&chain, &device);
        Ok(Self {
            chain,
            sink: Arc::new(sink),
            device,
            broken: false,
        })
    }

    pub fn sink(&self) -> &Arc<rodio::Player> {
        &self.sink
    }

    /// Whether a track at `rate` can go out as the output stands, without `fit` reopening it.
    pub fn fits(&self, rate: u32) -> bool {
        self.device.live() && self.device.fits(rate)
    }

    /// Moves this output onto a shared stream at `rate` unless it `fits` already, keeping the
    /// player paused if it was. The stream reopens when it runs at another rate, which closes
    /// it under the other engines. Whatever this output had queued is dropped and any clone of
    /// `sink` goes stale, so callers do this before a track and take `sink` again when it
    /// returns true.
    pub fn fit(&mut self, rate: u32) -> Result<bool> {
        if self.fits(rate) {
            return Ok(false);
        }
        let paused = self.sink.is_paused();
        let device = match Device::shared(Some(rate)) {
            Ok(device) => device,
            Err(error) => {
                self.broken = true;
                return Err(error);
            }
        };
        let sink = attach(&self.chain, &device);
        if paused {
            sink.pause();
        }
        self.sink = Arc::new(sink);
        self.device = device;
        self.broken = false;
        Ok(true)
    }

    pub fn set_volume(&self, gain: f32) {
        self.chain.volume.set(gain);
    }

    /// Whether what this output holds can no longer be heard: the stream reported an error,
    /// another engine reopened it at another rate, or `fit` failed. Every output on the stream
    /// sees an error. A fresh track recovers through `fit`, a paused one has to be reloaded.
    pub fn failed(&self) -> bool {
        self.broken || !self.device.live()
    }

    /// Whether the system's default device is no longer the one this output plays on.
    pub fn changed(&self) -> bool {
        cpal::default_host()
            .default_output_device()
            .map(|device| ident(&device))
            .is_some_and(|device| device != "unknown" && device != self.device.id)
    }
}

pub struct SmoothGain<I> {
    input: I,
    volume: Volume,
    tap: Option<Tap>,
    /// Whether the tap hears the samples from before the gain. It is read once per frame so
    /// every channel of a frame agrees.
    unscaled: bool,

    current: f32,
    target: f32,
    step: f32,

    ramp: Duration,
    frames_left: u32,
    ramp_frames: u32,

    channel: u16,
    channels: u16,
    rate: u32,
}

impl<I: Source> SmoothGain<I> {
    pub fn new(input: I, volume: Volume, initial: f32, ramp: Duration) -> Self {
        Self {
            input,
            volume,
            tap: None,
            unscaled: false,
            current: initial,
            target: initial,
            step: 0.0,
            ramp,
            frames_left: 0,
            ramp_frames: 1,
            channel: 0,
            channels: 0,
            rate: 0,
        }
    }

    pub fn with_tap(mut self, tap: Tap) -> Self {
        self.tap = Some(tap);
        self
    }

    fn resync(&mut self) {
        let channels = self.input.channels().get();
        let rate = self.input.sample_rate().get();
        if channels == self.channels && rate == self.rate {
            return;
        }

        self.channels = channels;
        self.rate = rate;
        if let Some(tap) = &self.tap {
            tap.format(rate, channels);
        }
        self.ramp_frames = (self.ramp.as_secs_f64() * rate as f64).round().max(1.0) as u32;
        self.frames_left = self.frames_left.min(self.ramp_frames);
    }
}

impl<I: Source> Iterator for SmoothGain<I> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.input.next()?;

        if self.channel == 0 {
            self.resync();
            self.unscaled = self.tap.as_ref().is_some_and(Tap::absolute);
            let requested = self.volume.get().max(0.0);

            if requested.to_bits() != self.target.to_bits() {
                self.target = requested;
                self.frames_left = self.ramp_frames;
                self.step = (self.target - self.current) / self.ramp_frames as f32;
            }

            if self.frames_left > 0 {
                self.current += self.step;
                self.frames_left -= 1;

                if self.frames_left == 0 {
                    self.current = self.target;
                }
            }
        }

        let output = sample * self.current;
        if let Some(tap) = self.tap.as_mut() {
            tap.push(match self.unscaled {
                true => sample,
                false => output,
            });
        }

        self.channel += 1;
        if self.channel >= self.channels {
            self.channel = 0;
        }

        Some(output)
    }
}

impl<I: Source> Source for SmoothGain<I> {
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }

    fn channels(&self) -> NonZero<u16> {
        self.input.channels()
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.input.try_seek(position)
    }
}

pub struct Trimmed<I> {
    input: I,
    head: u64,
    body: Option<u64>,
    emitted: u64,
    primed: bool,
    lane: u64,
}

impl<I: Source> Trimmed<I> {
    pub fn new(input: I, skip: Duration, take: Option<Duration>) -> Self {
        let lane = (input.sample_rate().get() as u64) * (input.channels().get() as u64);
        let samples = |span: Duration| (span.as_secs_f64() * lane as f64).round() as u64;

        Self {
            head: samples(skip),
            body: take.map(samples),
            emitted: 0,
            primed: false,
            lane,
            input,
        }
    }

    fn offset(&self) -> Duration {
        Duration::from_secs_f64(self.head as f64 / self.lane as f64)
    }
}

impl<I: Source> Iterator for Trimmed<I> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.primed {
            self.primed = true;
            for _ in 0..self.head {
                self.input.next()?;
            }
        }
        if self.body.is_some_and(|body| self.emitted >= body) {
            return None;
        }

        let sample = self.input.next()?;
        self.emitted += 1;
        Some(sample)
    }
}

impl<I: Source> Source for Trimmed<I> {
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }

    fn channels(&self) -> NonZero<u16> {
        self.input.channels()
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        match self.body {
            Some(body) => Some(Duration::from_secs_f64(body as f64 / self.lane as f64)),
            None => self
                .input
                .total_duration()
                .map(|whole| whole.saturating_sub(self.offset())),
        }
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.input.try_seek(position + self.offset())?;
        self.primed = true;
        self.emitted = (position.as_secs_f64() * self.lane as f64).round() as u64;
        Ok(())
    }
}

/// Whether the system has a default output device to play on. A device can be missing for as
/// long as it takes a headset to reconnect, so the engines ask again rather than give up.
pub fn available() -> bool {
    cpal::default_host().default_output_device().is_some()
}

fn ident(device: &cpal::Device) -> String {
    device
        .id()
        .map(|id| id.to_string())
        .unwrap_or_else(|_| "unknown".to_owned())
}

/// A fresh player whose chain plays into `device`.
fn attach(chain: &Chain, device: &Device) -> rodio::Player {
    let applied = chain.volume.get();
    let tap = chain.spectrum.attach();
    let (sink, source) = rodio::Player::new();
    let equalized = Equalized::new(source, chain.equalizer.clone());
    device.add(SmoothGain::new(equalized, chain.volume.clone(), applied, RAMP).with_tap(tap));
    sink
}

/// The device's config at `rate` in the default's channels and sample format, if the device
/// offers that combination.
fn at_rate(
    device: &cpal::Device,
    default: &cpal::SupportedStreamConfig,
    rate: u32,
) -> Option<cpal::SupportedStreamConfig> {
    if default.sample_rate() == rate {
        return Some(default.clone());
    }
    device
        .supported_output_configs()
        .ok()?
        .filter(|range| {
            range.channels() == default.channels()
                && range.sample_format() == default.sample_format()
        })
        .find_map(|range| range.try_with_sample_rate(rate))
}
