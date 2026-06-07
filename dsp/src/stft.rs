use crate::constants::*;
use std::f32::consts::PI;

pub(crate) fn fill_hann_window(window: &mut [f32; FFT_SIZE]) {
    for (n, sample) in window.iter_mut().enumerate() {
        *sample = 0.5 - 0.5 * (2.0 * PI * n as f32 / FFT_SIZE as f32).cos();
    }
}

pub(crate) fn calculate_window_gain(window: &[f32; FFT_SIZE]) -> f32 {
    let mut cola_sum = 0.0;
    let mut k = 0;
    while k * HOP_SIZE < FFT_SIZE {
        let w = window[k * HOP_SIZE];
        cola_sum += w * w;
        k += 1;
    }

    if cola_sum > 0.0 {
        cola_sum.recip()
    } else {
        1.0
    }
}

#[inline]
pub(crate) fn phase_advance_for_bin(bin: usize) -> f32 {
    2.0 * PI * bin as f32 * HOP_SIZE as f32 / FFT_SIZE as f32
}
