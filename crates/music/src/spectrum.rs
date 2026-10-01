use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use std::time::Duration;

use rtrb::{PopError, RingBuffer};
use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;

const N_BANDS: usize = 32;
const FFT_SIZE: usize = 2048;
/// In samples of whatever the tap hears, so a stereo source gets four windows of slack: a
/// device fills its whole buffer in one go after a stall and anything past this is dropped.
const RING_CAPACITY: usize = FFT_SIZE * 16;
const MIN_FREQ: f32 = 100.;
const MAX_FREQ: f32 = 6_000.;
const GAIN: f32 = 8.;
/// The level past which a band is compressed towards the ceiling instead of cut at it. Loud
/// bands that are cut all sit at the same height, and neighbours there draw a flat crest.
const KNEE: f32 = 0.6;
const ATTACK: f32 = 0.9;
const DECAY: f32 = 0.12;
const IDLE_POLL: Duration = Duration::from_millis(4);

/// The band levels of one channel, published by the analyzer thread and read by the UI.
#[derive(Clone)]
struct Lane {
    bands: Arc<Vec<AtomicU32>>,
}

impl Lane {
    fn new() -> Self {
        Self {
            bands: Arc::new((0..N_BANDS).map(|_| AtomicU32::new(0)).collect()),
        }
    }

    fn read(&self) -> Vec<f32> {
        self.bands
            .iter()
            .map(|band| f32::from_bits(band.load(Ordering::Relaxed)))
            .collect()
    }

    fn set(&self, index: usize, value: f32) {
        self.bands[index].store(value.to_bits(), Ordering::Relaxed);
    }
}

/// The running spectrum of what is playing, one band set per channel. A mono source publishes
/// the same levels on both.
#[derive(Clone)]
pub struct Spectrum {
    left: Lane,
    right: Lane,
    /// Whether the taps hear the track before the user's volume rather than after it.
    absolute: Arc<AtomicBool>,
}

impl Spectrum {
    pub fn new() -> Self {
        Self {
            left: Lane::new(),
            right: Lane::new(),
            absolute: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Makes every tap on this spectrum hear the track before the user's volume, so the levels
    /// stop following it. The taps pick the change up at the next frame.
    pub fn set_absolute(&self, absolute: bool) {
        self.absolute.store(absolute, Ordering::Relaxed);
    }

    /// The left channel's bands.
    pub fn left(&self) -> Vec<f32> {
        self.left.read()
    }

    /// The right channel's bands.
    pub fn right(&self) -> Vec<f32> {
        self.right.read()
    }

    /// Both channels folded together, the louder of the two per band.
    pub fn bands(&self) -> Vec<f32> {
        let mut bands = self.left.read();
        for (band, right) in bands.iter_mut().zip(self.right.read()) {
            *band = band.max(right);
        }
        bands
    }

    /// Starts the analyzer thread and returns the tap that feeds it. The tap has to be told
    /// the format of what it is fed through `Tap::format` before the first sample, and again
    /// whenever that changes.
    pub fn attach(&self) -> Tap {
        let (producer, consumer) = RingBuffer::<f32>::new(RING_CAPACITY);
        let format = Arc::new(Format::default());
        let target = self.clone();
        let heard = format.clone();
        let spawned = std::thread::Builder::new()
            .name("spectrum".to_owned())
            .spawn(move || analyze(consumer, heard, target));
        if let Err(error) = spawned {
            log::error!("spectrum: cannot spawn analyzer thread: {error}");
        }
        Tap {
            producer,
            format,
            absolute: self.absolute.clone(),
        }
    }
}

impl Default for Spectrum {
    fn default() -> Self {
        Self::new()
    }
}

/// The rate and channel count of the samples in the ring, as the tap last declared them.
#[derive(Default)]
struct Format {
    rate: AtomicU32,
    channels: AtomicU16,
}

impl Format {
    fn read(&self) -> (u32, usize) {
        (
            self.rate.load(Ordering::Acquire).max(1),
            self.channels.load(Ordering::Acquire).max(1) as usize,
        )
    }
}

/// Where the samples go in. It sits on the source's side of the mixer, so what it hears is the
/// track's own rate and channel count, not the device's, and those can change with the track.
pub struct Tap {
    producer: rtrb::Producer<f32>,
    format: Arc<Format>,
    absolute: Arc<AtomicBool>,
}

impl Tap {
    /// Declares the format of the samples that follow. Call it before the first sample and on
    /// every change; the analyzer picks the change up at the next frame boundary.
    pub fn format(&self, rate: u32, channels: u16) {
        self.format.rate.store(rate, Ordering::Release);
        self.format.channels.store(channels, Ordering::Release);
    }

    /// Whether the tap wants the samples from before the user's volume is applied.
    pub fn absolute(&self) -> bool {
        self.absolute.load(Ordering::Relaxed)
    }

    pub fn push(&mut self, sample: f32) {
        self.producer.push(sample).ok();
    }
}

/// One channel's window of samples and the smoothing that follows it.
struct Side {
    lane: Lane,
    samples: [f32; FFT_SIZE],
    smoothed: [f32; N_BANDS],
}

impl Side {
    fn new(lane: Lane) -> Self {
        Self {
            lane,
            samples: [0.; FFT_SIZE],
            smoothed: [0.; N_BANDS],
        }
    }
}

fn hann_window() -> [f32; FFT_SIZE] {
    let mut window = [0f32; FFT_SIZE];
    for (i, value) in window.iter_mut().enumerate() {
        *value = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (FFT_SIZE - 1) as f32).cos();
    }
    window
}

fn band_edges(rate: f32) -> Vec<usize> {
    let nyquist = rate / 2.;
    let max_freq = MAX_FREQ.min(nyquist);
    let min_freq = MIN_FREQ.min(max_freq * 0.5).max(1.);
    let bin_hz = rate / FFT_SIZE as f32;

    (0..=N_BANDS)
        .map(|i| {
            let t = i as f32 / N_BANDS as f32;
            let freq = min_freq * (max_freq / min_freq).powf(t);
            ((freq / bin_hz) as usize).clamp(1, FFT_SIZE / 2 - 1)
        })
        .collect()
}

fn analyze(mut consumer: rtrb::Consumer<f32>, format: Arc<Format>, spectrum: Spectrum) {
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let window = hann_window();
    let (mut rate, mut channels) = format.read();
    let mut edges = band_edges(rate as f32);
    // One accumulator per side: a source with more than two channels folds its odd lanes left
    // and its even ones right, which is close enough for a visualizer.
    let mut sides = [Side::new(spectrum.left), Side::new(spectrum.right)];
    let mut filled = 0usize;
    let mut frame = vec![0f32; channels];
    let mut lane_index = 0usize;
    let mut buffer = vec![Complex32::default(); FFT_SIZE];

    loop {
        let sample = match consumer.pop() {
            Ok(sample) => sample,
            Err(PopError::Empty) if consumer.is_abandoned() => return,
            Err(PopError::Empty) => {
                std::thread::sleep(IDLE_POLL);
                continue;
            }
        };

        // A new track can bring a new format. Grouping the samples by the wrong channel
        // count stretches a window over several frames' worth of audio, so the levels move
        // a few times a second and every band lands on the wrong frequency.
        if lane_index == 0 {
            let heard = format.read();
            if heard != (rate, channels) {
                (rate, channels) = heard;
                edges = band_edges(rate as f32);
                frame = vec![0f32; channels];
                filled = 0;
            }
        }

        frame[lane_index] = sample;
        lane_index += 1;
        if lane_index < channels {
            continue;
        }
        lane_index = 0;

        for (index, side) in sides.iter_mut().enumerate() {
            let mut sum = 0.;
            let mut taken = 0usize;
            for sample in frame.iter().skip(index).step_by(2) {
                sum += sample;
                taken += 1;
            }
            side.samples[filled] = match taken {
                0 => frame.iter().sum::<f32>() / channels as f32,
                taken => sum / taken as f32,
            };
        }
        filled += 1;
        if filled < FFT_SIZE {
            continue;
        }
        filled = 0;

        for side in sides.iter_mut() {
            for ((slot, sample), weight) in buffer.iter_mut().zip(side.samples).zip(&window) {
                *slot = Complex32::new(sample * weight, 0.);
            }
            fft.process(&mut buffer);

            for (band, edge) in edges.windows(2).enumerate() {
                let lo = edge[0];
                let hi = edge[1].max(lo + 1);
                let magnitude = buffer[lo..hi]
                    .iter()
                    .map(|bin| bin.norm())
                    .fold(0f32, f32::max);
                let target = soften((magnitude * GAIN / (FFT_SIZE as f32 / 2.)).sqrt());
                let rate = match target > side.smoothed[band] {
                    true => ATTACK,
                    false => DECAY,
                };
                side.smoothed[band] += (target - side.smoothed[band]) * rate;
                side.lane.set(band, side.smoothed[band]);
            }
        }
    }
}

/// A band's level on a curve that is linear up to `KNEE` and eases towards 1 above it without
/// reaching it, so two loud bands still land at different heights.
fn soften(level: f32) -> f32 {
    match level <= KNEE {
        true => level,
        false => KNEE + (1. - KNEE) * ((level - KNEE) / (1. - KNEE)).tanh(),
    }
}
