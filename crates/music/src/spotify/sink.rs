use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use librespot_playback::audio_backend::{Sink, SinkError, SinkResult};
use librespot_playback::convert::Converter;
use librespot_playback::decoder::AudioPacket;
use librespot_playback::{NUM_CHANNELS, SAMPLE_RATE};

use crate::Visualizer;
use crate::audio::{Output, Volume};

const QUEUED_CHUNKS: usize = 26;
const DRAIN_POLL: Duration = Duration::from_millis(10);

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

pub struct BlazingSink {
    output: Output,
    flush: Flush,
}

impl BlazingSink {
    pub fn open(
        flush: Flush,
        volume: Volume,
        visualizer: Option<Visualizer>,
    ) -> Result<Self, SinkError> {
        let output = Output::open(volume, visualizer)
            .map_err(|error| SinkError::ConnectionRefused(error.to_string()))?;
        output.sink().pause();

        Ok(Self { output, flush })
    }

    pub fn boxed(flush: Flush, volume: Volume, visualizer: Option<Visualizer>) -> Box<dyn Sink> {
        match Self::open(flush, volume, visualizer) {
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
        self.output.sink().play();
        Ok(())
    }

    fn stop(&mut self) -> SinkResult<()> {
        self.output.sink().pause();
        Ok(())
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        if self.flush.take() {
            self.output.sink().clear();
            self.output.sink().play();
        }

        let samples = packet
            .samples()
            .map_err(|error| SinkError::OnWrite(error.to_string()))?;
        let samples = converter.f64_to_f32(samples);

        self.output.sink().append(rodio::buffer::SamplesBuffer::new(
            NUM_CHANNELS as cpal::ChannelCount,
            SAMPLE_RATE,
            &*samples,
        ));

        while self.output.sink().len() > QUEUED_CHUNKS {
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
