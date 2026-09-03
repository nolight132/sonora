use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use rodio::Source;
use rodio_tap::{
    FrequencyBin, TapReader, Transform, Visualizer as Analyzer, VisualizerConfig as AnalyzerConfig,
    TOP_FREQUENCY_48K,
};

const BASS: (f32, f32) = (18., 140.);
const MIDS: (f32, f32) = (140., 850.);
const HIGHS: (f32, f32) = (850., TOP_FREQUENCY_48K);

#[derive(Clone, Copy, Debug, Default)]
pub struct Pulse {
    pub peak: f32,
    pub rms: f32,
    pub bass: f32,
    pub mids: f32,
    pub highs: f32,
}

#[derive(Clone)]
pub struct Visualizer {
    current: Arc<Mutex<Option<Arc<TapReader<2>>>>>,
    peak: Arc<AtomicU32>,
    rms: Arc<AtomicU32>,
    bass: Arc<AtomicU32>,
    mids: Arc<AtomicU32>,
    highs: Arc<AtomicU32>,
}

impl Visualizer {
    pub fn new() -> Self {
        Self {
            current: Arc::new(Mutex::new(None)),
            peak: Arc::new(AtomicU32::new(0)),
            rms: Arc::new(AtomicU32::new(0)),
            bass: Arc::new(AtomicU32::new(0)),
            mids: Arc::new(AtomicU32::new(0)),
            highs: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn wrap<S>(&self, source: S) -> rodio_tap::TapAdapter<S, 2>
    where
        S: Source + Send + 'static,
        S::Item: cpal::Sample + Send + 'static,
        f32: cpal::FromSample<S::Item>,
    {
        let (reader, adapter) = TapReader::<2>::new(source);
        if let Ok(mut slot) = self.current.lock() {
            *slot = Some(reader);
        }
        adapter
    }

    pub fn pulse(&self) -> Pulse {
        Pulse {
            peak: load(&self.peak),
            rms: load(&self.rms),
            bass: load(&self.bass),
            mids: load(&self.mids),
            highs: load(&self.highs),
        }
    }

    pub fn clear(&self) {
        store(&self.peak, 0.);
        store(&self.rms, 0.);
        store(&self.bass, 0.);
        store(&self.mids, 0.);
        store(&self.highs, 0.);
    }

    pub async fn listen(&self) {
        let current = Arc::clone(&self.current);
        let visualizer = self.clone();
        let config = AnalyzerConfig {
            period: Duration::from_millis(33),
            transform: Transform::FourierCustom(vec![
                FrequencyBin::new(BASS.0, BASS.1),
                FrequencyBin::new(MIDS.0, MIDS.1),
                FrequencyBin::new(HIGHS.0, HIGHS.1),
            ]),
            normalize_by_fft_size: true,
            emit_before_fft_window_full: true,
            ..Default::default()
        };

        Analyzer::<2>::run_with_frame_reader_async(
            move || current.lock().ok().and_then(|slot| slot.clone()),
            config,
            move |channels, _| visualizer.store(fold(channels)),
        )
        .await
    }

    fn store(&self, pulse: Pulse) {
        store(&self.peak, pulse.peak);
        store(&self.rms, pulse.rms);
        store(&self.bass, pulse.bass);
        store(&self.mids, pulse.mids);
        store(&self.highs, pulse.highs);
    }
}

impl Default for Visualizer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Visualizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Visualizer").finish_non_exhaustive()
    }
}

fn fold(channels: &[rodio_tap::ChannelSpectrum]) -> Pulse {
    if channels.is_empty() {
        return Pulse::default();
    }

    let n = channels.len() as f32;
    let mean = |value: f32| value / n;
    let band = |index: usize| {
        mean(
            channels
                .iter()
                .map(|channel| channel.bins.get(index).copied().unwrap_or(0.))
                .sum::<f32>(),
        )
    };

    Pulse {
        peak: mean(channels.iter().map(|channel| channel.peak).sum::<f32>()).clamp(0., 1.),
        rms: mean(channels.iter().map(|channel| channel.rms).sum::<f32>()).clamp(0., 1.),
        bass: squash(band(0)),
        mids: squash(band(1)),
        highs: squash(band(2)),
    }
}

fn squash(magnitude: f32) -> f32 {
    (magnitude / (magnitude + 0.08)).clamp(0., 1.)
}

fn store(slot: &AtomicU32, value: f32) {
    slot.store(value.clamp(0., 1.).to_bits(), Ordering::Relaxed);
}

fn load(slot: &AtomicU32) -> f32 {
    f32::from_bits(slot.load(Ordering::Relaxed)).clamp(0., 1.)
}
