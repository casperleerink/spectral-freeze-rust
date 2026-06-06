use crate::constants::*;
use crate::params::ProcessParams;
use crate::random::JuceRandom;
use crate::state::{ChannelState, FreezeState, OrganicAmState, OrganicScratch};
use crate::stft::{calculate_window_gain, fill_hann_window, fill_phase_advance};
use rustfft::{Fft, FftPlanner, num_complex::Complex32};
use std::f32::consts::PI;
use std::sync::Arc;

pub struct SpectralFreeze {
    channels: Vec<ChannelState>,
    main_channel_count: usize,
    window: Box<[f32; FFT_SIZE]>,
    window_gain: f32,
    phase_advance: Box<[f32; NUM_BINS]>,

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
            main_channel_count: 0,
            window: Box::new([0.0; FFT_SIZE]),
            window_gain: 1.0,
            phase_advance: Box::new([0.0; NUM_BINS]),

            master_hop_counter: 0,
            forward_fft,
            inverse_fft,
            processed_spectrum: [0.0; SPECTRUM_DISPLAY_BINS],
            spectrum_bucket_starts: [1; SPECTRUM_DISPLAY_BINS],
            spectrum_bucket_ends: [2; SPECTRUM_DISPLAY_BINS],
        };
        this.prepare(44_100.0, 2);
        this
    }
}

impl SpectralFreeze {
    pub fn prepare(&mut self, sample_rate: f32, main_channels: usize) {
        self.main_channel_count = main_channels;

        self.channels.clear();
        self.channels.reserve(main_channels);
        for ch in 0..main_channels {
            let seed = 0x5f37_59df_u32 ^ (ch as u32 + 1).wrapping_mul(0x9e37_79b9);
            self.channels.push(ChannelState::new(seed));
        }

        fill_hann_window(self.window.as_mut());
        self.window_gain = calculate_window_gain(self.window.as_ref());
        fill_phase_advance(self.phase_advance.as_mut());

        let _safe_sample_rate = if sample_rate > 0.0 {
            sample_rate
        } else {
            44_100.0
        };
        self.processed_spectrum.fill(0.0);
        self.master_hop_counter = 0;

        self.precompute_spectrum_buckets();
    }

    pub fn reset(&mut self) {
        for ch in &mut self.channels {
            ch.reset();
        }
        self.processed_spectrum.fill(0.0);
        self.master_hop_counter = 0;
    }

    pub fn main_channel_count(&self) -> usize {
        self.main_channel_count
    }
    /// Process planar audio in place. `main` contains the main input copied into
    /// the output buffers.
    pub fn process_block(&mut self, main: &mut [&mut [f32]], params: ProcessParams) {
        let params = params.clamped();
        let num_samples = main.first().map_or(0, |ch| ch.len());
        let main_channels = main.len().min(self.channels.len());

        for n in 0..num_samples {
            for ch in 0..main_channels {
                let state = &mut self.channels[ch];
                main[ch][n] = state.stft.push_sample_and_pop_output(main[ch][n]);
            }

            self.master_hop_counter += 1;
            if self.master_hop_counter >= HOP_SIZE {
                self.master_hop_counter = 0;

                for ch in 0..main_channels {
                    self.process_channel_frame(ch, params);
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

    fn process_channel_frame(&mut self, ch: usize, params: ProcessParams) {
        let state = &mut self.channels[ch];
        state.stft.copy_input_frame_to_spectrum();

        let fifo_primed = state.stft.samples_seen >= FFT_SIZE;
        let capture_edge = params.freeze
            && fifo_primed
            && (!state.freeze.was_frozen || !state.freeze.has_frozen_frame);
        let run_analysis = !params.freeze || capture_edge || !state.freeze.has_frozen_frame;

        if run_analysis {
            apply_window(state.stft.spectrum.as_mut(), self.window.as_ref());
            self.forward_fft.process(state.stft.spectrum.as_mut_slice());
            record_analysis_frame(
                &mut state.freeze,
                state.stft.spectrum.as_ref(),
                self.phase_advance.as_ref(),
            );
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
            &mut state.organic_scratch,
            params.organic,
            params.filter,
        );

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

fn record_analysis_frame(
    state: &mut FreezeState,
    spectrum: &[Complex32; FFT_SIZE],
    phase_advance: &[f32; NUM_BINS],
) {
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
            state.smoothed_phase_advance[k] =
                0.65 * state.smoothed_phase_advance[k] + 0.35 * measured_advance;
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

pub(crate) fn normalize_inverse_fft(spectrum: &mut [Complex32; FFT_SIZE]) {
    let scale = 1.0 / FFT_SIZE as f32;
    for sample in spectrum.iter_mut() {
        *sample *= scale;
    }
}

pub(crate) fn apply_synthesis_window(
    spectrum: &mut [Complex32; FFT_SIZE],
    window: &[f32; FFT_SIZE],
    gain: f32,
) {
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

pub(crate) fn rebuild_conjugate_mirror(spectrum: &mut [Complex32; FFT_SIZE]) {
    for k in 1..(FFT_SIZE / 2) {
        spectrum[FFT_SIZE - k] = spectrum[k].conj();
    }
    spectrum[0].im = 0.0;
    spectrum[FFT_SIZE / 2].im = 0.0;
}

pub(crate) fn apply_organic_spectral_processing(
    spectrum: &mut [Complex32; FFT_SIZE],
    rng: &mut JuceRandom,
    scratch: &mut OrganicScratch,
    organic_amt: f32,
    filter_amt: f32,
) {
    if organic_amt <= 0.0 {
        return;
    }

    let smooth_amt = organic_amt * (0.30 + 0.60 * filter_amt);
    let mag = &mut scratch.mag;
    let phase = &mut scratch.phase;
    let shaped_mag = &mut scratch.shaped_mag;

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
            + smooth_amt
                * (0.08 * far_left + 0.22 * left + 0.40 * mid + 0.22 * right + 0.08 * far_right);
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

pub(crate) fn apply_organic_saturation(spectrum: &mut [Complex32; FFT_SIZE], organic_amt: f32) {
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

#[inline]
fn clamp(x: f32, min: f32, max: f32) -> f32 {
    x.max(min).min(max)
}

#[inline]
fn gain_to_decibels(gain: f32, minus_infinity_db: f32) -> f32 {
    if gain > 0.0 {
        20.0 * gain.log10()
    } else {
        minus_infinity_db
    }
}
