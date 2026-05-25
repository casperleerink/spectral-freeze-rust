//! Host-agnostic DSP for Spectral Freeze.
//!
//! This crate contains the complete audio algorithm. The native CLAP/VST3 shell
//! and the headless WAM shell both call into this processor and consume the same
//! parameter manifest below. Runtime allocation is limited to [`SpectralFreeze::prepare`]
//! and construction; [`SpectralFreeze::process_block`] performs no heap allocation.

use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::f32::consts::PI;
use std::sync::Arc;

pub const FFT_ORDER: usize = 11;
pub const FFT_SIZE: usize = 1 << FFT_ORDER;
pub const HOP_SIZE: usize = FFT_SIZE / 4;
pub const NUM_BINS: usize = FFT_SIZE / 2 + 1;
pub const MAG_HISTORY_SIZE: usize = 4;
pub const FREEZE_PHASE_JITTER_RADIANS: f32 = 0.004;
pub const ORGANIC_AM_BANDS: usize = 12;
pub const SPECTRUM_DISPLAY_BINS: usize = 96;
pub const LATENCY_SAMPLES: u32 = FFT_SIZE as u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Bool,
    Float,
}

#[derive(Clone, Copy, Debug)]
pub struct ParamInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ParamKind,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub unit: &'static str,
}

pub const PARAM_FREEZE: usize = 0;
pub const PARAM_FILTER: usize = 1;
pub const PARAM_SC_BOOST: usize = 2;
pub const PARAM_SC_FREQ_SMOOTHING: usize = 3;
pub const PARAM_ORGANIC: usize = 4;

pub const PARAMS: [ParamInfo; 5] = [
    ParamInfo {
        id: "freeze",
        name: "Freeze",
        kind: ParamKind::Bool,
        min: 0.0,
        max: 1.0,
        default: 0.0,
        unit: "",
    },
    ParamInfo {
        id: "filter",
        name: "Filter",
        kind: ParamKind::Float,
        min: 0.0,
        max: 1.0,
        default: 0.0,
        unit: "%",
    },
    ParamInfo {
        id: "scBoost",
        name: "SC Boost",
        kind: ParamKind::Float,
        min: 0.0,
        max: 18.0,
        default: 9.0,
        unit: " dB",
    },
    ParamInfo {
        id: "scFreqSmoothing",
        name: "SC Freq Smooth",
        kind: ParamKind::Float,
        min: 0.0,
        max: 1.0,
        default: 0.25,
        unit: "%",
    },
    ParamInfo {
        id: "organic",
        name: "Organic",
        kind: ParamKind::Float,
        min: 0.0,
        max: 1.0,
        default: 0.0,
        unit: "%",
    },
];

/// Static JSON generated from the manifest above for the WAM JS layer.
pub const PARAMETER_MANIFEST_JSON: &str = r#"[
  {"id":"freeze","name":"Freeze","kind":"bool","min":0.0,"max":1.0,"default":0.0,"unit":""},
  {"id":"filter","name":"Filter","kind":"float","min":0.0,"max":1.0,"default":0.0,"unit":"%"},
  {"id":"scBoost","name":"SC Boost","kind":"float","min":0.0,"max":18.0,"default":9.0,"unit":" dB"},
  {"id":"scFreqSmoothing","name":"SC Freq Smooth","kind":"float","min":0.0,"max":1.0,"default":0.25,"unit":"%"},
  {"id":"organic","name":"Organic","kind":"float","min":0.0,"max":1.0,"default":0.0,"unit":"%"}
]"#;

#[derive(Clone, Copy, Debug)]
pub struct ProcessParams {
    pub freeze: bool,
    pub filter: f32,
    pub sc_boost_db: f32,
    pub sc_freq_smoothing: f32,
    pub organic: f32,
}

impl Default for ProcessParams {
    fn default() -> Self {
        Self {
            freeze: PARAMS[PARAM_FREEZE].default >= 0.5,
            filter: PARAMS[PARAM_FILTER].default,
            sc_boost_db: PARAMS[PARAM_SC_BOOST].default,
            sc_freq_smoothing: PARAMS[PARAM_SC_FREQ_SMOOTHING].default,
            organic: PARAMS[PARAM_ORGANIC].default,
        }
    }
}

impl ProcessParams {
    pub fn from_values(values: [f32; 5]) -> Self {
        Self {
            freeze: values[PARAM_FREEZE] >= 0.5,
            filter: values[PARAM_FILTER],
            sc_boost_db: values[PARAM_SC_BOOST],
            sc_freq_smoothing: values[PARAM_SC_FREQ_SMOOTHING],
            organic: values[PARAM_ORGANIC],
        }
        .clamped()
    }

    pub fn clamped(self) -> Self {
        Self {
            freeze: self.freeze,
            filter: clamp(self.filter, 0.0, 1.0),
            sc_boost_db: clamp(self.sc_boost_db, 0.0, 18.0),
            sc_freq_smoothing: clamp(self.sc_freq_smoothing, 0.0, 1.0),
            organic: clamp(self.organic, 0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct JuceRandom {
    seed: u64,
}

impl JuceRandom {
    fn new(seed: u64) -> Self {
        Self { seed: seed & 0x0000_ffff_ffff_ffff }
    }

    #[inline]
    fn next_int(&mut self) -> u32 {
        // Matches juce::Random::nextInt(): a 48-bit LCG with Java-style
        // constants, returning the top 32 bits after the update.
        self.seed = self.seed.wrapping_mul(0x5deece66d).wrapping_add(11) & 0x0000_ffff_ffff_ffff;
        (self.seed >> 16) as u32
    }

    #[inline]
    fn next_float(&mut self) -> f32 {
        let result = self.next_int() as f32 / (u32::MAX as f32 + 1.0);
        result.min(1.0 - f32::EPSILON)
    }

    #[inline]
    fn bipolar(&mut self) -> f32 {
        self.next_float() * 2.0 - 1.0
    }
}

struct FreezeState {
    frozen_mag: [f32; NUM_BINS],
    frozen_phase: [f32; NUM_BINS],
    frozen_phase_advance: [f32; NUM_BINS],
    last_analysis_phase: [f32; NUM_BINS],
    smoothed_phase_advance: [f32; NUM_BINS],
    has_last_analysis_phase: bool,
    mag_history: [[f32; NUM_BINS]; MAG_HISTORY_SIZE],
    mag_history_write: usize,
    mag_history_count: usize,
    was_frozen: bool,
    has_frozen_frame: bool,
}

impl Default for FreezeState {
    fn default() -> Self {
        Self {
            frozen_mag: [0.0; NUM_BINS],
            frozen_phase: [0.0; NUM_BINS],
            frozen_phase_advance: [0.0; NUM_BINS],
            last_analysis_phase: [0.0; NUM_BINS],
            smoothed_phase_advance: [0.0; NUM_BINS],
            has_last_analysis_phase: false,
            mag_history: [[0.0; NUM_BINS]; MAG_HISTORY_SIZE],
            mag_history_write: 0,
            mag_history_count: 0,
            was_frozen: false,
            has_frozen_frame: false,
        }
    }
}

struct OrganicAmState {
    value: [f32; ORGANIC_AM_BANDS],
    target: [f32; ORGANIC_AM_BANDS],
    hop_counter: usize,
}

impl Default for OrganicAmState {
    fn default() -> Self {
        Self {
            value: [0.0; ORGANIC_AM_BANDS],
            target: [0.0; ORGANIC_AM_BANDS],
            hop_counter: 0,
        }
    }
}

struct StftChannelState {
    input_fifo: Box<[f32; FFT_SIZE]>,
    output_fifo: Box<[f32; FFT_SIZE]>,
    spectrum: Box<[Complex32; FFT_SIZE]>,
    fifo_pos: usize,
    samples_seen: usize,
}

impl StftChannelState {
    fn new() -> Self {
        Self {
            input_fifo: Box::new([0.0; FFT_SIZE]),
            output_fifo: Box::new([0.0; FFT_SIZE]),
            spectrum: Box::new([Complex32::new(0.0, 0.0); FFT_SIZE]),
            fifo_pos: 0,
            samples_seen: 0,
        }
    }

    fn reset(&mut self) {
        self.input_fifo.fill(0.0);
        self.output_fifo.fill(0.0);
        self.spectrum.fill(Complex32::new(0.0, 0.0));
        self.fifo_pos = 0;
        self.samples_seen = 0;
    }

    #[inline]
    fn push_sample_and_pop_output(&mut self, input: f32) -> f32 {
        self.input_fifo[self.fifo_pos] = input;
        let output = self.output_fifo[self.fifo_pos];
        self.output_fifo[self.fifo_pos] = 0.0;
        self.fifo_pos = (self.fifo_pos + 1) % FFT_SIZE;
        if self.samples_seen < FFT_SIZE {
            self.samples_seen += 1;
        }
        output
    }

    fn copy_input_frame_to_spectrum(&mut self) {
        for i in 0..FFT_SIZE {
            self.spectrum[i] = Complex32::new(self.input_fifo[(self.fifo_pos + i) % FFT_SIZE], 0.0);
        }
    }

    fn overlap_add_scratch_to_output(&mut self) {
        for i in 0..FFT_SIZE {
            self.output_fifo[(self.fifo_pos + i) % FFT_SIZE] += self.spectrum[i].re;
        }
    }
}

struct ChannelState {
    stft: StftChannelState,
    freeze: FreezeState,
    organic_am: OrganicAmState,
    rng: JuceRandom,
}

impl ChannelState {
    fn new(seed: u32) -> Self {
        let mut rng = JuceRandom::new(seed as u64);
        let mut organic_am = OrganicAmState::default();
        for target in &mut organic_am.target {
            *target = rng.bipolar();
        }
        Self {
            stft: StftChannelState::new(),
            freeze: FreezeState::default(),
            organic_am,
            rng,
        }
    }

    fn reset(&mut self) {
        self.stft.reset();
        self.freeze = FreezeState::default();
        self.organic_am.value.fill(0.0);
        self.organic_am.hop_counter = 0;
        for target in &mut self.organic_am.target {
            *target = self.rng.bipolar();
        }
    }
}

struct SidechainState {
    input_fifo: Box<[f32; FFT_SIZE]>,
    spectrum: Box<[Complex32; FFT_SIZE]>,
    fifo_pos: usize,
}

impl SidechainState {
    fn new() -> Self {
        Self {
            input_fifo: Box::new([0.0; FFT_SIZE]),
            spectrum: Box::new([Complex32::new(0.0, 0.0); FFT_SIZE]),
            fifo_pos: 0,
        }
    }

    fn reset(&mut self) {
        self.input_fifo.fill(0.0);
        self.spectrum.fill(Complex32::new(0.0, 0.0));
        self.fifo_pos = 0;
    }
}

pub struct SpectralFreeze {
    channels: Vec<ChannelState>,
    sc_channels: Vec<SidechainState>,
    main_channel_count: usize,
    sidechain_channel_count: usize,
    window: Box<[f32; FFT_SIZE]>,
    window_gain: f32,
    phase_advance: Box<[f32; NUM_BINS]>,
    sc_latest_mag: Box<[f32; NUM_BINS]>,
    sc_smoothed_mag: Box<[f32; NUM_BINS]>,
    sc_retention_per_hop: f32,
    master_hop_counter: usize,
    forward_fft: Arc<dyn Fft<f32>>,
    inverse_fft: Arc<dyn Fft<f32>>,
    processed_spectrum: [f32; SPECTRUM_DISPLAY_BINS],
    spectrum_bucket_starts: [usize; SPECTRUM_DISPLAY_BINS],
    spectrum_bucket_ends: [usize; SPECTRUM_DISPLAY_BINS],
}

impl Default for SpectralFreeze {
    fn default() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let forward_fft = planner.plan_fft_forward(FFT_SIZE);
        let inverse_fft = planner.plan_fft_inverse(FFT_SIZE);

        let mut this = Self {
            channels: Vec::new(),
            sc_channels: Vec::new(),
            main_channel_count: 0,
            sidechain_channel_count: 0,
            window: Box::new([0.0; FFT_SIZE]),
            window_gain: 1.0,
            phase_advance: Box::new([0.0; NUM_BINS]),
            sc_latest_mag: Box::new([0.0; NUM_BINS]),
            sc_smoothed_mag: Box::new([0.0; NUM_BINS]),
            sc_retention_per_hop: 0.65,
            master_hop_counter: 0,
            forward_fft,
            inverse_fft,
            processed_spectrum: [0.0; SPECTRUM_DISPLAY_BINS],
            spectrum_bucket_starts: [1; SPECTRUM_DISPLAY_BINS],
            spectrum_bucket_ends: [2; SPECTRUM_DISPLAY_BINS],
        };
        this.prepare(44_100.0, 2, 0);
        this
    }
}

impl SpectralFreeze {
    pub fn prepare(&mut self, sample_rate: f32, main_channels: usize, sidechain_channels: usize) {
        self.main_channel_count = main_channels;
        self.sidechain_channel_count = sidechain_channels;

        self.channels.clear();
        self.channels.reserve(main_channels);
        for ch in 0..main_channels {
            let seed = 0x5f37_59df_u32 ^ (ch as u32 + 1).wrapping_mul(0x9e37_79b9);
            self.channels.push(ChannelState::new(seed));
        }

        self.sc_channels.clear();
        self.sc_channels.reserve(sidechain_channels);
        for _ in 0..sidechain_channels {
            self.sc_channels.push(SidechainState::new());
        }

        for n in 0..FFT_SIZE {
            self.window[n] = 0.5 - 0.5 * (2.0 * PI * n as f32 / FFT_SIZE as f32).cos();
        }

        let mut cola_sum = 0.0;
        let mut k = 0;
        while k * HOP_SIZE < FFT_SIZE {
            let w = self.window[k * HOP_SIZE];
            cola_sum += w * w;
            k += 1;
        }
        self.window_gain = if cola_sum > 0.0 { cola_sum.recip() } else { 1.0 };

        for k in 0..NUM_BINS {
            self.phase_advance[k] = 2.0 * PI * k as f32 * HOP_SIZE as f32 / FFT_SIZE as f32;
        }

        self.sc_latest_mag.fill(0.0);
        self.sc_smoothed_mag.fill(0.0);
        self.processed_spectrum.fill(0.0);
        self.master_hop_counter = 0;

        let safe_sample_rate = if sample_rate > 0.0 { sample_rate } else { 44_100.0 };
        self.sc_retention_per_hop = (-(HOP_SIZE as f32 / (safe_sample_rate * 0.75))).exp();

        self.precompute_spectrum_buckets();
    }

    pub fn reset(&mut self) {
        for ch in &mut self.channels {
            ch.reset();
        }
        for ch in &mut self.sc_channels {
            ch.reset();
        }
        self.sc_latest_mag.fill(0.0);
        self.sc_smoothed_mag.fill(0.0);
        self.processed_spectrum.fill(0.0);
        self.master_hop_counter = 0;
    }

    pub fn main_channel_count(&self) -> usize { self.main_channel_count }
    pub fn sidechain_channel_count(&self) -> usize { self.sidechain_channel_count }

    /// Process planar audio in place. `main` contains the main input copied into
    /// the output buffers. `sidechain`, when present, is read-only sidechain input.
    pub fn process_block(
        &mut self,
        main: &mut [&mut [f32]],
        sidechain: Option<&[&mut [f32]]>,
        params: ProcessParams,
    ) {
        let params = params.clamped();
        let num_samples = main.first().map_or(0, |ch| ch.len());
        let main_channels = main.len().min(self.channels.len());
        let sidechain_channels = sidechain
            .map(|sc| sc.len().min(self.sc_channels.len()))
            .unwrap_or(0);
        let run_sidechain = sidechain_channels > 0 && params.sc_boost_db > 0.0;

        for n in 0..num_samples {
            for ch in 0..main_channels {
                let state = &mut self.channels[ch];
                main[ch][n] = state.stft.push_sample_and_pop_output(main[ch][n]);
            }

            if run_sidechain {
                if let Some(sc) = sidechain {
                    for ch in 0..sidechain_channels {
                        let state = &mut self.sc_channels[ch];
                        state.input_fifo[state.fifo_pos] = sc[ch][n];
                        state.fifo_pos = (state.fifo_pos + 1) % FFT_SIZE;
                    }
                }
            }

            self.master_hop_counter += 1;
            if self.master_hop_counter >= HOP_SIZE {
                self.master_hop_counter = 0;

                if run_sidechain {
                    self.analyse_sidechain_hop(sidechain_channels);
                }

                for ch in 0..main_channels {
                    self.process_channel_frame(ch, run_sidechain, params);
                    self.channels[ch].stft.overlap_add_scratch_to_output();
                }
            }
        }
    }

    pub fn processed_spectrum_snapshot(&self) -> [f32; SPECTRUM_DISPLAY_BINS] {
        self.processed_spectrum
    }

    fn precompute_spectrum_buckets(&mut self) {
        let min_bin = 1.0_f32;
        let max_bin = (NUM_BINS - 1) as f32;
        for i in 0..SPECTRUM_DISPLAY_BINS {
            let start_norm = i as f32 / SPECTRUM_DISPLAY_BINS as f32;
            let end_norm = (i + 1) as f32 / SPECTRUM_DISPLAY_BINS as f32;
            let start = ((max_bin / min_bin).powf(start_norm) * min_bin).floor() as usize;
            let end = ((max_bin / min_bin).powf(end_norm) * min_bin).ceil() as usize;
            let start = start.clamp(1, NUM_BINS - 1);
            let end = end.clamp(start + 1, NUM_BINS);
            self.spectrum_bucket_starts[i] = start;
            self.spectrum_bucket_ends[i] = end;
        }
    }

    fn analyse_sidechain_hop(&mut self, sidechain_channels: usize) {
        self.sc_latest_mag.fill(0.0);
        for ch in 0..sidechain_channels {
            let state = &mut self.sc_channels[ch];
            for i in 0..FFT_SIZE {
                state.spectrum[i] = Complex32::new(
                    state.input_fifo[(state.fifo_pos + i) % FFT_SIZE] * self.window[i],
                    0.0,
                );
            }
            self.forward_fft.process(state.spectrum.as_mut_slice());
            for k in 0..NUM_BINS {
                self.sc_latest_mag[k] += state.spectrum[k].norm();
            }
        }

        for k in 0..NUM_BINS {
            self.sc_smoothed_mag[k] = self.sc_retention_per_hop * self.sc_smoothed_mag[k]
                + (1.0 - self.sc_retention_per_hop) * self.sc_latest_mag[k];
        }
    }

    fn process_channel_frame(&mut self, ch: usize, apply_sidechain: bool, params: ProcessParams) {
        let state = &mut self.channels[ch];
        state.stft.copy_input_frame_to_spectrum();

        let fifo_primed = state.stft.samples_seen >= FFT_SIZE;
        let capture_edge = params.freeze && fifo_primed && (!state.freeze.was_frozen || !state.freeze.has_frozen_frame);
        let run_analysis = !params.freeze || capture_edge || !state.freeze.has_frozen_frame;

        if run_analysis {
            apply_window(state.stft.spectrum.as_mut(), self.window.as_ref());
            self.forward_fft.process(state.stft.spectrum.as_mut_slice());
            record_analysis_frame(&mut state.freeze, state.stft.spectrum.as_ref(), self.phase_advance.as_ref());
        }

        if capture_edge {
            capture_freeze_frame(&mut state.freeze, state.stft.spectrum.as_ref());
        }

        if params.freeze && state.freeze.has_frozen_frame {
            resynthesise_frozen_frame(
                &mut state.freeze,
                &mut state.organic_am,
                &mut state.rng,
                state.stft.spectrum.as_mut(),
                params.organic,
            );
        }

        apply_magnitude_threshold_filter(state.stft.spectrum.as_mut(), params.filter);
        apply_organic_spectral_processing(
            state.stft.spectrum.as_mut(),
            &mut state.rng,
            params.organic,
            params.filter,
        );

        if apply_sidechain {
            apply_sidechain_enhancement(
                state.stft.spectrum.as_mut(),
                self.sc_smoothed_mag.as_ref(),
                params.sc_boost_db,
                params.sc_freq_smoothing,
            );
        }

        publish_processed_spectrum_from(
            state.stft.spectrum.as_ref(),
            &self.spectrum_bucket_starts,
            &self.spectrum_bucket_ends,
            &mut self.processed_spectrum,
        );
        rebuild_conjugate_mirror(state.stft.spectrum.as_mut());
        self.inverse_fft.process(state.stft.spectrum.as_mut_slice());
        normalize_inverse_fft(state.stft.spectrum.as_mut());
        apply_organic_saturation(state.stft.spectrum.as_mut(), params.organic);
        apply_synthesis_window(
            state.stft.spectrum.as_mut(),
            self.window.as_ref(),
            self.window_gain,
        );

        state.freeze.was_frozen = params.freeze;
    }

}

fn publish_processed_spectrum_from(
    spectrum: &[Complex32; FFT_SIZE],
    bucket_starts: &[usize; SPECTRUM_DISPLAY_BINS],
    bucket_ends: &[usize; SPECTRUM_DISPLAY_BINS],
    processed_spectrum: &mut [f32; SPECTRUM_DISPLAY_BINS],
) {
    let mut peak = 0.0;
    let mut bucket_mags = [0.0_f32; SPECTRUM_DISPLAY_BINS];
    for i in 0..SPECTRUM_DISPLAY_BINS {
        let mut mag = 0.0;
        for bin in bucket_starts[i]..bucket_ends[i] {
            let bin_mag = spectrum[bin].norm();
            if bin_mag > mag {
                mag = bin_mag;
            }
        }
        bucket_mags[i] = mag;
        if mag > peak {
            peak = mag;
        }
    }

    if peak < 1.0e-7 {
        for bin in processed_spectrum {
            *bin *= 0.85;
        }
        return;
    }

    const FLOOR_DB: f32 = -72.0;
    for (i, mag) in bucket_mags.iter().enumerate() {
        let relative_db = gain_to_decibels(*mag / peak, FLOOR_DB);
        let target = clamp((relative_db - FLOOR_DB) / -FLOOR_DB, 0.0, 1.0);
        let previous = processed_spectrum[i];
        processed_spectrum[i] = if target > previous {
            0.55 * previous + 0.45 * target
        } else {
            0.88 * previous + 0.12 * target
        };
    }
}

fn record_analysis_frame(state: &mut FreezeState, spectrum: &[Complex32; FFT_SIZE], phase_advance: &[f32; NUM_BINS]) {
    let slot = &mut state.mag_history[state.mag_history_write];
    for k in 0..NUM_BINS {
        let c = spectrum[k];
        let phase = c.im.atan2(c.re);
        slot[k] = c.norm();

        if state.has_last_analysis_phase {
            let mut deviation = phase - state.last_analysis_phase[k] - phase_advance[k];
            while deviation > PI {
                deviation -= 2.0 * PI;
            }
            while deviation < -PI {
                deviation += 2.0 * PI;
            }
            let measured_advance = phase_advance[k] + deviation;
            state.smoothed_phase_advance[k] = 0.65 * state.smoothed_phase_advance[k] + 0.35 * measured_advance;
        } else {
            state.smoothed_phase_advance[k] = phase_advance[k];
        }
        state.last_analysis_phase[k] = phase;
    }

    state.has_last_analysis_phase = true;
    state.mag_history_write = (state.mag_history_write + 1) % MAG_HISTORY_SIZE;
    state.mag_history_count = (state.mag_history_count + 1).min(MAG_HISTORY_SIZE);
}

fn capture_freeze_frame(state: &mut FreezeState, spectrum: &[Complex32; FFT_SIZE]) {
    let count = state.mag_history_count.max(1);
    let inv_count = 1.0 / count as f32;
    for k in 0..NUM_BINS {
        let mut sum = 0.0;
        for h in 0..count {
            sum += state.mag_history[h][k];
        }
        let c = spectrum[k];
        state.frozen_mag[k] = sum * inv_count;
        state.frozen_phase[k] = c.im.atan2(c.re);
        state.frozen_phase_advance[k] = state.smoothed_phase_advance[k];
    }
    state.has_frozen_frame = true;
}

fn resynthesise_frozen_frame(
    state: &mut FreezeState,
    organic_am: &mut OrganicAmState,
    rng: &mut JuceRandom,
    spectrum: &mut [Complex32; FFT_SIZE],
    organic_amt: f32,
) {
    if organic_amt > 0.0 {
        organic_am.hop_counter += 1;
        if organic_am.hop_counter >= 8 {
            organic_am.hop_counter = 0;
            for target in &mut organic_am.target {
                *target = rng.bipolar();
            }
        }
        for b in 0..ORGANIC_AM_BANDS {
            organic_am.value[b] += 0.08 * (organic_am.target[b] - organic_am.value[b]);
        }
    }

    for k in 0..NUM_BINS {
        let mut phase = state.frozen_phase[k]
            + state.frozen_phase_advance[k] * (1.0 + rng.bipolar() * organic_amt * 0.035)
            + rng.bipolar() * (FREEZE_PHASE_JITTER_RADIANS + organic_amt * 0.18);
        if phase > PI {
            phase -= 2.0 * PI;
        } else if phase < -PI {
            phase += 2.0 * PI;
        }
        state.frozen_phase[k] = phase;

        let band_pos = k as f32 * ORGANIC_AM_BANDS as f32 / NUM_BINS as f32;
        let band0 = clamp(band_pos as f32, 0.0, (ORGANIC_AM_BANDS - 1) as f32) as usize;
        let band1 = (band0 + 1).min(ORGANIC_AM_BANDS - 1);
        let frac = band_pos - band0 as f32;
        let band_am = organic_am.value[band0] * (1.0 - frac) + organic_am.value[band1] * frac;
        let mag = state.frozen_mag[k]
            * (1.0 + band_am * organic_amt * 0.28)
            * (1.0 + rng.bipolar() * organic_amt * 0.06);
        spectrum[k] = Complex32::from_polar(mag, phase);
    }
}

fn apply_window(spectrum: &mut [Complex32; FFT_SIZE], window: &[f32; FFT_SIZE]) {
    for i in 0..FFT_SIZE {
        spectrum[i].re *= window[i];
    }
}

fn normalize_inverse_fft(spectrum: &mut [Complex32; FFT_SIZE]) {
    let scale = 1.0 / FFT_SIZE as f32;
    for sample in spectrum.iter_mut() {
        *sample *= scale;
    }
}

fn apply_synthesis_window(spectrum: &mut [Complex32; FFT_SIZE], window: &[f32; FFT_SIZE], gain: f32) {
    for i in 0..FFT_SIZE {
        spectrum[i].re *= window[i] * gain;
        spectrum[i].im = 0.0;
    }
}

fn apply_magnitude_threshold_filter(spectrum: &mut [Complex32; FFT_SIZE], filter_amt: f32) {
    if filter_amt <= 0.0 {
        return;
    }
    let mut max_mag = 0.0;
    for c in spectrum.iter().take(NUM_BINS) {
        let mag = c.norm();
        if mag > max_mag {
            max_mag = mag;
        }
    }
    if max_mag <= 0.0 {
        return;
    }
    let threshold = max_mag * filter_amt * filter_amt;
    for c in spectrum.iter_mut().take(NUM_BINS) {
        if c.norm() < threshold {
            *c = Complex32::new(0.0, 0.0);
        }
    }
}

fn rebuild_conjugate_mirror(spectrum: &mut [Complex32; FFT_SIZE]) {
    for k in 1..(FFT_SIZE / 2) {
        spectrum[FFT_SIZE - k] = spectrum[k].conj();
    }
    spectrum[0].im = 0.0;
    spectrum[FFT_SIZE / 2].im = 0.0;
}

fn apply_organic_spectral_processing(
    spectrum: &mut [Complex32; FFT_SIZE],
    rng: &mut JuceRandom,
    organic_amt: f32,
    filter_amt: f32,
) {
    if organic_amt <= 0.0 {
        return;
    }

    let smooth_amt = organic_amt * (0.30 + 0.60 * filter_amt);
    let mut mag = [0.0_f32; NUM_BINS];
    let mut phase = [0.0_f32; NUM_BINS];
    let mut shaped_mag = [0.0_f32; NUM_BINS];

    let mut peak = 0.0;
    for k in 0..NUM_BINS {
        mag[k] = spectrum[k].norm();
        phase[k] = spectrum[k].im.atan2(spectrum[k].re);
        if mag[k] > peak {
            peak = mag[k];
        }
    }
    if peak <= 1.0e-9 {
        return;
    }

    for k in 0..NUM_BINS {
        let far_left = mag[k.saturating_sub(2)];
        let left = mag[k.saturating_sub(1)];
        let mid = mag[k];
        let right = mag[(k + 1).min(NUM_BINS - 1)];
        let far_right = mag[(k + 2).min(NUM_BINS - 1)];
        shaped_mag[k] = (1.0 - smooth_amt) * mid
            + smooth_amt * (0.08 * far_left + 0.22 * left + 0.40 * mid + 0.22 * right + 0.08 * far_right);
    }

    let residual_level = organic_amt * organic_amt * (0.0007 + 0.0025 * filter_amt) * peak;
    for k in 0..NUM_BINS {
        let local_env = 0.5 * shaped_mag[k] / peak + 0.5;
        let noise_mag = residual_level * local_env * (0.4 + 0.6 * rng.next_float());
        let noise_phase = rng.next_float() * 2.0 * PI;
        spectrum[k] = Complex32::from_polar(shaped_mag[k], phase[k])
            + Complex32::from_polar(noise_mag, noise_phase);
    }
}

fn apply_organic_saturation(spectrum: &mut [Complex32; FFT_SIZE], organic_amt: f32) {
    if organic_amt <= 0.0 {
        return;
    }

    let drive = 1.0 + organic_amt * 4.0;
    let makeup = drive.tanh().recip();
    let wet = organic_amt * 0.60;
    let mut input_energy = 0.0;
    let mut output_energy = 0.0;

    for c in spectrum.iter_mut() {
        let dry = c.re;
        input_energy += dry * dry;
        let sat = (dry * drive).tanh() * makeup;
        c.re = dry + wet * (sat - dry);
        output_energy += c.re * c.re;
    }

    if input_energy > 1.0e-12 && output_energy > 1.0e-12 {
        let compensation = (input_energy / output_energy).sqrt();
        for c in spectrum.iter_mut() {
            c.re *= compensation;
        }
    }
}

fn apply_sidechain_enhancement(
    spectrum: &mut [Complex32; FFT_SIZE],
    smoothed_mag: &[f32; NUM_BINS],
    boost_db: f32,
    freq_smoothing: f32,
) {
    if boost_db <= 0.0 {
        return;
    }

    let mut sc_peak: f32 = 0.0;
    let mut main_peak: f32 = 0.0;
    for k in 0..NUM_BINS {
        sc_peak = sc_peak.max(smoothed_mag[k]);
        main_peak = main_peak.max(spectrum[k].norm());
    }
    if sc_peak <= 1.0e-9 || main_peak <= 1.0e-9 {
        return;
    }

    let mut raw_mask = [0.0_f32; NUM_BINS];
    let mut mask = [0.0_f32; NUM_BINS];
    let inv_sc_peak = sc_peak.recip();
    let inv_main_peak = main_peak.recip();
    const GAMMA: f32 = 1.25;
    const PRESENCE_THRESHOLD: f32 = 0.004;
    const PRESENCE_FULL: f32 = 0.05;

    for k in 0..NUM_BINS {
        let main_norm = spectrum[k].norm() * inv_main_peak;
        let sc_norm = smoothed_mag[k] * inv_sc_peak;
        let sc_match = clamp(sc_norm, 0.0, 1.0).powf(GAMMA);
        let main_presence = smoothstep((main_norm - PRESENCE_THRESHOLD) / (PRESENCE_FULL - PRESENCE_THRESHOLD));
        raw_mask[k] = sc_match * main_presence;
    }

    let a = clamp(freq_smoothing, 0.0, 1.0);
    for k in 0..NUM_BINS {
        let left = raw_mask[k.saturating_sub(1)];
        let mid = raw_mask[k];
        let right = raw_mask[(k + 1).min(NUM_BINS - 1)];
        mask[k] = (1.0 - a) * mid + a * (0.25 * left + 0.5 * mid + 0.25 * right);
    }

    let max_boost = decibels_to_gain(clamp(boost_db, 0.0, 18.0));
    for k in 0..NUM_BINS {
        let shaped = smoothstep(mask[k]);
        let boost_gain = 1.0 + (max_boost - 1.0) * shaped;
        spectrum[k] *= boost_gain;
    }
}

#[inline]
fn clamp(x: f32, min: f32, max: f32) -> f32 {
    x.max(min).min(max)
}

#[inline]
fn smoothstep(x: f32) -> f32 {
    let x = clamp(x, 0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

#[inline]
fn decibels_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

#[inline]
fn gain_to_decibels(gain: f32, minus_infinity_db: f32) -> f32 {
    if gain > 0.0 {
        20.0 * gain.log10()
    } else {
        minus_infinity_db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_matches_constants() {
        assert_eq!(PARAMS.len(), 5);
        assert_eq!(PARAMS[PARAM_FREEZE].id, "freeze");
        assert_eq!(PARAMS[PARAM_SC_FREQ_SMOOTHING].default, 0.25);
        assert!(PARAMETER_MANIFEST_JSON.contains("scFreqSmoothing"));
    }

    #[test]
    fn silence_stays_silent() {
        let mut processor = SpectralFreeze::default();
        processor.prepare(44_100.0, 2, 0);
        let mut left = vec![0.0_f32; 4096];
        let mut right = vec![0.0_f32; 4096];
        let mut channels: [&mut [f32]; 2] = [&mut left, &mut right];
        processor.process_block(&mut channels, None, ProcessParams::default());
        assert!(channels.iter().flat_map(|ch| ch.iter()).all(|sample| sample.abs() < 1.0e-6));
    }

    #[test]
    fn impulse_passes_after_latency() {
        let mut processor = SpectralFreeze::default();
        processor.prepare(48_000.0, 1, 0);
        let mut mono = vec![0.0_f32; FFT_SIZE * 4];
        mono[0] = 1.0;
        let mut channels: [&mut [f32]; 1] = [&mut mono];
        processor.process_block(&mut channels, None, ProcessParams::default());
        let energy: f32 = channels[0].iter().map(|x| x.abs()).sum();
        assert!(energy > 0.1, "expected overlap-add output energy, got {energy}");
    }

    #[test]
    fn organic_saturation_compensates_its_own_gain() {
        let mut frame = [Complex32::new(0.0, 0.0); FFT_SIZE];
        for (i, sample) in frame.iter_mut().enumerate() {
            sample.re = (2.0 * PI * 440.0 * i as f32 / 44_100.0).sin() * 0.2;
        }
        let before = (frame.iter().map(|x| x.re * x.re).sum::<f32>() / frame.len() as f32).sqrt();

        apply_organic_saturation(&mut frame, 1.0);

        let after = (frame.iter().map(|x| x.re * x.re).sum::<f32>() / frame.len() as f32).sqrt();
        assert!((after - before).abs() <= before * 0.01, "saturation changed RMS: before={before}, after={after}");
    }

    #[test]
    fn organic_macro_does_not_raise_output_level() {
        fn render_rms(organic: f32) -> f32 {
            let mut processor = SpectralFreeze::default();
            processor.prepare(44_100.0, 1, 0);
            let mut mono = vec![0.0_f32; FFT_SIZE * 16];
            for (i, sample) in mono.iter_mut().enumerate() {
                *sample = (2.0 * PI * 440.0 * i as f32 / 44_100.0).sin() * 0.2;
            }
            let mut channels: [&mut [f32]; 1] = [&mut mono];
            processor.process_block(&mut channels, None, ProcessParams { organic, ..Default::default() });
            let stable = &channels[0][FFT_SIZE * 2..];
            (stable.iter().map(|x| x * x).sum::<f32>() / stable.len() as f32).sqrt()
        }

        let dry = render_rms(0.0);
        let organic = render_rms(1.0);
        assert!(organic <= dry * 1.05, "organic raised output level: dry={dry}, organic={organic}");
        assert!(organic >= dry * 0.80, "organic over-compensated output level: dry={dry}, organic={organic}");
    }

    fn sine_buffer(freq: f32, amp: f32, sample_rate: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| (2.0 * PI * freq * i as f32 / sample_rate).sin() * amp)
            .collect()
    }

    fn sine_projection(samples: &[f32], freq: f32, sample_rate: f32, offset: usize) -> f32 {
        let mut sin_sum = 0.0;
        let mut cos_sum = 0.0;
        for (i, sample) in samples.iter().enumerate() {
            let phase = 2.0 * PI * freq * (i + offset) as f32 / sample_rate;
            sin_sum += *sample * phase.sin();
            cos_sum += *sample * phase.cos();
        }
        2.0 * (sin_sum * sin_sum + cos_sum * cos_sum).sqrt() / samples.len() as f32
    }

    #[test]
    fn freeze_produces_bounded_audio() {
        let mut processor = SpectralFreeze::default();
        processor.prepare(44_100.0, 1, 0);
        let mut mono = sine_buffer(440.0, 0.25, 44_100.0, FFT_SIZE * 8);
        let mut channels: [&mut [f32]; 1] = [&mut mono];
        processor.process_block(&mut channels, None, ProcessParams { freeze: true, ..Default::default() });
        assert!(channels[0].iter().all(|x| x.is_finite() && x.abs() < 8.0));
    }

    #[test]
    fn freeze_holds_tone_after_input_stops() {
        let sample_rate = 44_100.0;
        let mut processor = SpectralFreeze::default();
        processor.prepare(sample_rate, 1, 0);

        let mut prime = sine_buffer(440.0, 0.2, sample_rate, FFT_SIZE * 3);
        let mut channels: [&mut [f32]; 1] = [&mut prime];
        processor.process_block(&mut channels, None, ProcessParams::default());

        let mut capture = sine_buffer(440.0, 0.2, sample_rate, HOP_SIZE * 2);
        let mut channels: [&mut [f32]; 1] = [&mut capture];
        processor.process_block(&mut channels, None, ProcessParams { freeze: true, ..Default::default() });

        let mut silent = vec![0.0_f32; FFT_SIZE * 6];
        let mut channels: [&mut [f32]; 1] = [&mut silent];
        processor.process_block(&mut channels, None, ProcessParams { freeze: true, ..Default::default() });

        let held = &channels[0][FFT_SIZE * 2..];
        let rms = (held.iter().map(|x| x * x).sum::<f32>() / held.len() as f32).sqrt();
        assert!(rms > 0.01, "frozen output disappeared after input stopped, rms={rms}");
    }

    #[test]
    fn silent_sidechain_matches_no_sidechain() {
        let sample_rate = 44_100.0;
        let len = FFT_SIZE * 8;
        let input = sine_buffer(440.0, 0.2, sample_rate, len);

        let mut no_sc_processor = SpectralFreeze::default();
        no_sc_processor.prepare(sample_rate, 1, 0);
        let mut no_sc = input.clone();
        let mut no_sc_channels: [&mut [f32]; 1] = [&mut no_sc];
        no_sc_processor.process_block(&mut no_sc_channels, None, ProcessParams::default());

        let mut sc_processor = SpectralFreeze::default();
        sc_processor.prepare(sample_rate, 1, 1);
        let mut with_sc = input;
        let mut silent_sc = vec![0.0_f32; len];
        let mut with_sc_channels: [&mut [f32]; 1] = [&mut with_sc];
        let sc_channels: [&mut [f32]; 1] = [&mut silent_sc];
        sc_processor.process_block(&mut with_sc_channels, Some(&sc_channels), ProcessParams::default());

        for (a, b) in no_sc_channels[0].iter().zip(with_sc_channels[0].iter()) {
            assert!((a - b).abs() < 1.0e-6, "silent sidechain changed output: {a} vs {b}");
        }
    }

    #[test]
    fn sidechain_boosts_matching_frequency() {
        let sample_rate = 44_100.0;
        let len = FFT_SIZE * 24;
        let mut main: Vec<f32> = (0..len)
            .map(|i| {
                let t = i as f32 / sample_rate;
                0.12 * (2.0 * PI * 440.0 * t).sin() + 0.04 * (2.0 * PI * 880.0 * t).sin()
            })
            .collect();
        let mut sidechain = sine_buffer(880.0, 0.4, sample_rate, len);

        let mut processor = SpectralFreeze::default();
        processor.prepare(sample_rate, 1, 1);
        let mut main_channels: [&mut [f32]; 1] = [&mut main];
        let sc_channels: [&mut [f32]; 1] = [&mut sidechain];
        processor.process_block(
            &mut main_channels,
            Some(&sc_channels),
            ProcessParams { sc_boost_db: 18.0, sc_freq_smoothing: 0.25, ..Default::default() },
        );

        let start = FFT_SIZE * 6;
        let analysed = &main_channels[0][start..];
        let a440 = sine_projection(analysed, 440.0, sample_rate, start);
        let a880 = sine_projection(analysed, 880.0, sample_rate, start);
        assert!(a880 / a440 > 0.45, "sidechain did not boost matched 880 Hz enough: 440={a440}, 880={a880}");
    }

    #[test]
    fn freeze_recaptures_on_second_rising_edge() {
        let sample_rate = 44_100.0;
        let mut processor = SpectralFreeze::default();
        processor.prepare(sample_rate, 1, 0);

        let mut first = sine_buffer(330.0, 0.2, sample_rate, FFT_SIZE * 5);
        let mut channels: [&mut [f32]; 1] = [&mut first];
        processor.process_block(&mut channels, None, ProcessParams { freeze: true, ..Default::default() });

        let mut unfreeze = sine_buffer(880.0, 0.2, sample_rate, FFT_SIZE * 5);
        let mut channels: [&mut [f32]; 1] = [&mut unfreeze];
        processor.process_block(&mut channels, None, ProcessParams::default());

        let mut second = sine_buffer(880.0, 0.2, sample_rate, FFT_SIZE * 8);
        let mut channels: [&mut [f32]; 1] = [&mut second];
        processor.process_block(&mut channels, None, ProcessParams { freeze: true, ..Default::default() });

        let start = FFT_SIZE * 4;
        let analysed = &channels[0][start..];
        let a330 = sine_projection(analysed, 330.0, sample_rate, start);
        let a880 = sine_projection(analysed, 880.0, sample_rate, start);
        assert!(a880 > a330 * 2.0, "second freeze edge did not recapture new tone: 330={a330}, 880={a880}");
    }

    #[test]
    fn sidechain_can_be_enabled_while_freeze_is_already_on() {
        let sample_rate = 44_100.0;
        let len = FFT_SIZE * 10;
        let mut processor = SpectralFreeze::default();
        processor.prepare(sample_rate, 1, 1);
        let mut main = sine_buffer(440.0, 0.2, sample_rate, len);
        let mut sidechain = sine_buffer(440.0, 0.2, sample_rate, len);
        let mut main_channels: [&mut [f32]; 1] = [&mut main];
        let sc_channels: [&mut [f32]; 1] = [&mut sidechain];
        processor.process_block(
            &mut main_channels,
            Some(&sc_channels),
            ProcessParams { freeze: true, sc_boost_db: 9.0, ..Default::default() },
        );
        let stable = &main_channels[0][FFT_SIZE * 4..];
        let rms = (stable.iter().map(|x| x * x).sum::<f32>() / stable.len() as f32).sqrt();
        assert!(rms > 0.01, "freeze+sidechain startup produced no tone, rms={rms}");
    }
}
