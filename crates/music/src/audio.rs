use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use anyhow::{Context as _, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::source::{SeekError, UniformSourceIterator};
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source};

use crate::spectrum::{Spectrum, Tap};

pub const RAMP: Duration = Duration::from_millis(25);
const BUFFER: Duration = Duration::from_millis(50);

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

pub struct Output {
    sink: Arc<rodio::Player>,
    volume: Volume,
    device: String,
    failed: Arc<AtomicBool>,
    _stream: MixerDeviceSink,
}

impl Output {
    pub fn open(volume: Volume, spectrum: Spectrum) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("no audio output device")?;

        let default = device
            .default_output_config()
            .map_err(|error| anyhow::anyhow!("cannot read the output config: {error}"))?;

        let device_name = ident(&device);
        log::info!(
            "sink: using {} at {} Hz, {} channels, {}",
            device_name,
            default.sample_rate(),
            default.channels(),
            default.sample_format()
        );

        let format = default.sample_format();
        let frames = (BUFFER.as_secs_f64() * default.sample_rate() as f64).round() as u32;
        let failed = Arc::new(AtomicBool::new(false));
        let stream_failed = failed.clone();
        let builder = DeviceSinkBuilder::default()
            .with_device(device)
            .with_config(&default.config())
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

        let applied = volume.get();
        let tap = spectrum.attach(default.sample_rate(), default.channels());
        let (sink, source) = rodio::Player::new();
        stream.mixer().add(
            output_source(
                source,
                volume.clone(),
                applied,
                default.channels(),
                default.sample_rate(),
            )
            .with_tap(tap),
        );

        Ok(Self {
            sink: Arc::new(sink),
            volume,
            device: device_name,
            failed,
            _stream: stream,
        })
    }

    pub fn sink(&self) -> &Arc<rodio::Player> {
        &self.sink
    }

    pub fn set_volume(&self, gain: f32) {
        self.volume.set(gain);
    }

    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    pub fn changed(&self) -> bool {
        cpal::default_host()
            .default_output_device()
            .map(|device| ident(&device))
            .is_none_or(|device| device != self.device)
    }
}

fn output_source<I: Source>(
    input: I,
    volume: Volume,
    initial: f32,
    channels: u16,
    rate: u32,
) -> SmoothGain<UniformSourceIterator<I>> {
    let source = UniformSourceIterator::new(
        input,
        NonZero::new(channels).unwrap(),
        NonZero::new(rate).unwrap(),
    );
    SmoothGain::new(source, volume, initial, RAMP)
}

pub struct SmoothGain<I> {
    input: I,
    volume: Volume,
    tap: Option<Tap>,

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
        if let Some(tap) = self.tap.as_mut() {
            tap.push(output);
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
    channels: u64,
}

impl<I: Source> Trimmed<I> {
    pub fn new(input: I, skip: Duration, take: Option<Duration>) -> Self {
        let rate = input.sample_rate().get() as u64;
        let channels = input.channels().get() as u64;
        let lane = rate * channels;
        let samples = |span: Duration| frame_samples(span, rate, channels);

        Self {
            head: samples(skip),
            body: take.map(samples),
            emitted: 0,
            primed: false,
            lane,
            channels,
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
        let emitted = frame_samples(position, self.lane / self.channels, self.channels);
        let target = Duration::from_secs_f64(emitted as f64 / self.lane as f64);
        self.input.try_seek(target.saturating_add(self.offset()))?;
        self.primed = true;
        self.emitted = emitted;
        Ok(())
    }
}

fn frame_samples(span: Duration, rate: u64, channels: u64) -> u64 {
    let frames = (span.as_nanos() * u128::from(rate) + 500_000_000) / 1_000_000_000;
    frames.min(u128::from(u64::MAX / channels)) as u64 * channels
}

fn ident(device: &cpal::Device) -> String {
    device
        .id()
        .map(|id| id.to_string())
        .unwrap_or_else(|_| "unknown".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;

    fn buffer(channels: u16, rate: u32, data: Vec<f32>) -> SamplesBuffer {
        SamplesBuffer::new(
            NonZero::new(channels).unwrap(),
            NonZero::new(rate).unwrap(),
            data,
        )
    }

    #[test]
    fn fractional_trim_preserves_channel_frames() {
        for channels in [2u16, 6] {
            let data = (0..1000)
                .flat_map(|_| (0..channels).map(f32::from))
                .collect();
            let input = buffer(channels, 44_100, data);
            let mut trimmed = Trimmed::new(
                input,
                Duration::from_millis(5),
                Some(Duration::from_millis(5)),
            );
            assert_eq!(trimmed.head, 221 * u64::from(channels));
            let samples: Vec<_> = trimmed.by_ref().collect();
            assert_eq!(samples.len(), 221 * usize::from(channels));
            for frame in samples.chunks_exact(usize::from(channels)) {
                assert_eq!(frame, (0..channels).map(f32::from).collect::<Vec<_>>());
            }
            trimmed.try_seek(Duration::from_millis(1)).unwrap();
            assert_eq!(trimmed.emitted, 44 * u64::from(channels));
            assert_eq!(trimmed.count(), (221 - 44) * usize::from(channels));
        }
    }

    #[test]
    fn trim_handles_zero_and_extreme_durations_without_partial_frames() {
        assert_eq!(frame_samples(Duration::ZERO, 44_100, 2), 0);
        assert_eq!(frame_samples(Duration::MAX, 192_000, 6) % 6, 0);
    }

    #[test]
    fn output_preserves_pitch_across_sample_rates_and_channel_counts() {
        for (rate, channels) in [(48_000, 1), (44_100, 2)] {
            let data = (0..rate / 10)
                .flat_map(|frame| {
                    let sample = (std::f32::consts::TAU * 3000. * frame as f32 / rate as f32).sin();
                    std::iter::repeat_n(sample, channels as usize)
                })
                .collect();
            let output =
                output_source(buffer(channels, rate, data), Volume::new(1.), 1., 2, 48_000);
            assert_eq!(output.channels().get(), 2);
            assert_eq!(output.sample_rate().get(), 48_000);
            let stereo: Vec<_> = output.take(4096).collect();
            assert_eq!(stereo.len(), 4096);
            let mut bins: Vec<_> = stereo
                .chunks_exact(2)
                .map(|frame| {
                    assert!((frame[0] - frame[1]).abs() < 0.0001);
                    rustfft::num_complex::Complex32::new((frame[0] + frame[1]) / 2., 0.)
                })
                .collect();
            rustfft::FftPlanner::new()
                .plan_fft_forward(bins.len())
                .process(&mut bins);
            let peak = (1..bins.len() / 2)
                .max_by(|&a, &b| bins[a].norm_sqr().total_cmp(&bins[b].norm_sqr()))
                .unwrap();
            assert_eq!(peak, 128, "3 kHz must remain 3 kHz at 48 kHz");
        }
    }
}
