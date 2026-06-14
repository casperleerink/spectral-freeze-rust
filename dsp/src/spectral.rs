use crate::constants::*;
use crate::random::JuceRandom;
use crate::state::OrganicScratch;
use rustfft::num_complex::Complex32;
use std::f32::consts::PI;

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
    // Higher wet mix so the tanh harmonics actually land in the timbre.
    let wet = organic_amt * 0.85;
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
        // Only partially correct the energy change. Full compensation makes the
        // saturation purely timbral (no level movement), which reads as "nothing
        // happening"; letting ~40% of the natural drive-induced level change
        // through keeps the effect alive without runaway gain.
        let compensation = (input_energy / output_energy).sqrt();
        let compensation = 1.0 + (compensation - 1.0) * 0.6;
        for c in spectrum.iter_mut() {
            c.re *= compensation;
        }
    }
}
