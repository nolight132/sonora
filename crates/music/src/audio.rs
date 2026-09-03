use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use anyhow::{Context as _, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::source::SeekError;
use rodio::{OutputStream, OutputStreamBuilder, Source};

pub const RAMP: Duration = Duration::from_millis(25);

#[derive(Clone)]
pub struct Volume(Arc<AtomicU32>);

impl Volume {
    pub fn new(gain: f32) -> Self {
        Self(Arc::new(AtomicU32::new(gain.to_bits())))
    }

    pub fn set(&self, gain: f32) {
        self.0.store(gain.to_bits(), Ordering::Relaxed);
    }

    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

pub struct Output {
    sink: Arc<rodio::Sink>,
    volume: Volume,
    _stream: OutputStream,
}

impl Output {
    pub fn open(volume: Volume) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("no audio output device")?;

        let default = device
            .default_output_config()
            .map_err(|error| anyhow::anyhow!("cannot read the output config: {error}"))?;

        log::info!(
            "sink: using {} at {} Hz, {} channels, {}",
            device.name().unwrap_or_else(|_| "unknown".to_owned()),
            default.sample_rate().0,
            default.channels(),
            default.sample_format()
        );

        let format = default.sample_format();
        let builder = OutputStreamBuilder::default()
            .with_device(device)
            .with_config(&default.config())
            .with_sample_format(format);
        let mut stream = builder
            .open_stream()
            .map_err(|error| anyhow::anyhow!("cannot open the audio output: {error}"))?;
        stream.log_on_drop(false);

        let applied = volume.get();
        let (sink, source) = rodio::Sink::new();
        stream
            .mixer()
            .add(SmoothGain::new(source, volume.clone(), applied, RAMP));

        Ok(Self {
            sink: Arc::new(sink),
            volume,
            _stream: stream,
        })
    }

    pub fn sink(&self) -> &Arc<rodio::Sink> {
        &self.sink
    }

    pub fn set_volume(&self, gain: f32) {
        self.volume.set(gain);
    }
}

pub struct SmoothGain<I> {
    input: I,
    volume: Volume,

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

    fn resync(&mut self) {
        let channels = self.input.channels().max(1);
        let rate = self.input.sample_rate().max(1);
        if channels == self.channels && rate == self.rate {
            return;
        }

        self.channels = channels;
        self.rate = rate;
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

    fn channels(&self) -> u16 {
        self.input.channels()
    }

    fn sample_rate(&self) -> u32 {
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
        let lane = (input.sample_rate() as u64) * (input.channels().max(1) as u64);
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

    fn channels(&self) -> u16 {
        self.input.channels()
    }

    fn sample_rate(&self) -> u32 {
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
