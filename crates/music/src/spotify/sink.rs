//! librespot's `Sink` over the shared paced output. librespot decodes on its own thread, so
//! every write here is finished audio; the pacing and the device watch live in `crate::sink`.

use std::time::Duration;

use librespot_playback::audio_backend::{Sink, SinkError, SinkResult};
use librespot_playback::convert::Converter;
use librespot_playback::decoder::AudioPacket;
use librespot_playback::{NUM_CHANNELS, SAMPLE_RATE};
use tokio::sync::mpsc::UnboundedSender;

use crate::audio::Chain;
use crate::sink::{Cue, Paced, packet, watch_for_output};

pub struct OutputSink {
    paced: Paced,
}

impl OutputSink {
    /// The sink librespot's player builder asks for. Without an output device it gets a silent
    /// one, so playback state still moves.
    pub fn boxed(cue: Cue, chain: Chain, changed: UnboundedSender<()>) -> Box<dyn Sink> {
        match Paced::open(cue.clone(), chain, changed.clone()) {
            Ok(paced) => Box::new(Self { paced }),
            Err(error) => {
                log::error!("sink: cannot open an output device: {error:#}");
                watch_for_output(changed);
                Box::new(Silence(cue))
            }
        }
    }
}

impl Sink for OutputSink {
    /// librespot decodes everything at `SAMPLE_RATE`, so this is the one place the output
    /// follows it, before the first packet of a play.
    fn start(&mut self) -> SinkResult<()> {
        self.paced
            .fit(SAMPLE_RATE, NUM_CHANNELS as u16)
            .and_then(|()| self.paced.play())
            .map_err(|error| SinkError::OnWrite(error.to_string()))
    }

    fn stop(&mut self) -> SinkResult<()> {
        self.paced.pause();
        Ok(())
    }

    fn write(&mut self, packeted: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        let samples = packeted
            .samples()
            .map_err(|error| SinkError::OnWrite(error.to_string()))?;
        let samples = converter.f64_to_f32(samples);
        let Some(samples) = packet(&samples, NUM_CHANNELS as u16, SAMPLE_RATE) else {
            return Ok(());
        };
        self.paced
            .write(samples)
            .and_then(|()| self.paced.drain())
            .map_err(|error| SinkError::OnWrite(error.to_string()))
    }
}

/// Swallows the audio when no output device opens. It still answers the cue, so playback state
/// moves on rather than waiting for sound that cannot come, and it takes as long over a packet
/// as the packet lasts: without the queue to pace it, the player would otherwise run a track
/// out at decoding speed and be sitting on the end of the queue by the time a device is back.
struct Silence(Cue);

impl Sink for Silence {
    fn write(&mut self, packeted: AudioPacket, _converter: &mut Converter) -> SinkResult<()> {
        if let Ok(samples) = packeted.samples() {
            std::thread::sleep(lasts(samples.len()));
        }
        self.0.admit();
        Ok(())
    }
}

/// How long `samples` of librespot's audio plays for, counted across every channel.
fn lasts(samples: usize) -> Duration {
    Duration::from_secs_f64(samples as f64 / f64::from(NUM_CHANNELS as u32 * SAMPLE_RATE))
}
