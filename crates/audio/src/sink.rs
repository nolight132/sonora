// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait};
use librespot_playback::audio_backend::{Sink, SinkError, SinkResult};
use librespot_playback::config::AudioFormat;
use librespot_playback::convert::Converter;
use librespot_playback::decoder::AudioPacket;
use librespot_playback::{NUM_CHANNELS, SAMPLE_RATE};
use rodio::source::SeekError;
use rodio::{OutputStream, OutputStreamBuilder, Source};

const QUEUED_CHUNKS: usize = 26;
const DRAIN_POLL: Duration = Duration::from_millis(10);
const GAIN_RAMP_DURATION: Duration = Duration::from_millis(25);

#[derive(Clone, Default)]
pub struct Flush(Arc<AtomicBool>);

impl Flush {
    pub fn request(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    fn take(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
}

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

struct SmoothGain<I> {
    input: I,
    volume: Volume,

    current: f32,
    target: f32,
    step: f32,

    frames_left: u32,
    ramp_frames: u32,

    channel: u16,
    channels: u16,
}

impl<I: Source> SmoothGain<I> {
    fn new(input: I, volume: Volume, initial: f32, duration: Duration) -> Self {
        let channels = input.channels();
        let ramp_frames = (duration.as_secs_f64() * input.sample_rate() as f64)
            .round()
            .max(1.0) as u32;

        Self {
            input,
            volume,
            current: initial,
            target: initial,
            step: 0.0,
            frames_left: 0,
            ramp_frames,
            channel: 0,
            channels,
        }
    }
}

impl<I: Source> Iterator for SmoothGain<I> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.input.next()?;

        if self.channel == 0 {
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
        if self.channel == self.channels {
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

pub struct BlazingSink {
    sink: Arc<rodio::Sink>,
    _volume: Volume,
    _stream: OutputStream,
    flush: Flush,
}

impl BlazingSink {
    pub fn open(format: AudioFormat, flush: Flush, volume: Volume) -> Result<Self, SinkError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| SinkError::ConnectionRefused("no output device".to_owned()))?;

        log::info!(
            "sink: using {}",
            device.name().unwrap_or_else(|_| "unknown".to_owned())
        );

        let default = device
            .default_output_config()
            .map_err(|error| SinkError::InvalidParams(error.to_string()))?;
        let config = device
            .supported_output_configs()
            .map_err(|error| SinkError::InvalidParams(error.to_string()))?
            .find(|config| config.channels() == NUM_CHANNELS as cpal::ChannelCount)
            .and_then(|config| {
                config
                    .try_with_sample_rate(cpal::SampleRate(SAMPLE_RATE))
                    .or_else(|| config.try_with_sample_rate(default.sample_rate()))
            })
            .unwrap_or(default);

        let sample_format = output_sample_format(format, config.sample_format());
        let mut stream = OutputStreamBuilder::default()
            .with_device(device)
            .with_config(&config.config())
            .with_sample_format(sample_format)
            .open_stream()
            .map_err(|error| SinkError::ConnectionRefused(error.to_string()))?;
        stream.log_on_drop(false);

        let applied = volume.get();

        let (sink, source) = rodio::Sink::new();

        stream.mixer().add(SmoothGain::new(
            source,
            volume.clone(),
            applied,
            GAIN_RAMP_DURATION,
        ));

        let sink = Arc::new(sink);
        sink.pause();

        Ok(Self {
            sink,
            _volume: volume,
            _stream: stream,
            flush,
        })
    }

    pub fn boxed(format: AudioFormat, flush: Flush, volume: Volume) -> Box<dyn Sink> {
        match Self::open(format, flush, volume) {
            Ok(sink) => Box::new(sink),
            Err(error) => {
                log::error!("sink: cannot open an output device: {error}");
                Box::new(Silence)
            }
        }
    }
}

impl Sink for BlazingSink {
    fn start(&mut self) -> SinkResult<()> {
        self.sink.play();
        Ok(())
    }

    fn stop(&mut self) -> SinkResult<()> {
        self.sink.pause();
        Ok(())
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        if self.flush.take() {
            self.sink.clear();
            self.sink.play();
        }

        let samples = packet
            .samples()
            .map_err(|error| SinkError::OnWrite(error.to_string()))?;
        let samples = converter.f64_to_f32(samples);

        self.sink.append(rodio::buffer::SamplesBuffer::new(
            NUM_CHANNELS as cpal::ChannelCount,
            SAMPLE_RATE,
            &*samples,
        ));

        while self.sink.len() > QUEUED_CHUNKS {
            std::thread::sleep(DRAIN_POLL);
        }
        Ok(())
    }
}

struct Silence;

impl Sink for Silence {
    fn write(&mut self, _packet: AudioPacket, _converter: &mut Converter) -> SinkResult<()> {
        Ok(())
    }
}

fn output_sample_format(input: AudioFormat, device: cpal::SampleFormat) -> cpal::SampleFormat {
    if cfg!(target_os = "windows") {
        device
    } else {
        match input {
            AudioFormat::F64 => cpal::SampleFormat::F64,
            AudioFormat::F32 => cpal::SampleFormat::F32,
            AudioFormat::S32 => cpal::SampleFormat::I32,
            AudioFormat::S24 | AudioFormat::S24_3 => cpal::SampleFormat::I24,
            AudioFormat::S16 => cpal::SampleFormat::I16,
        }
    }
}

#[cfg(test)]
mod tests {
    use librespot_playback::config::AudioFormat;

    use super::output_sample_format;

    #[test]
    fn chooses_the_sample_format_for_the_platform() {
        let selected = output_sample_format(AudioFormat::F32, cpal::SampleFormat::I16);

        #[cfg(target_os = "windows")]
        assert_eq!(selected, cpal::SampleFormat::I16);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(selected, cpal::SampleFormat::F32);
    }
}
