//! Host-agnostic instrument DSP for Spectral Freeze.
//!
//! This crate contains the audio algorithm shared by the native CLAP/VST3 shell
//! and standalone app. Runtime allocation is limited to construction and
//! `prepare()` calls; audio `process_block()` methods perform no heap allocation.

mod constants;
mod instrument;
mod params;
mod random;
mod spectral;
mod state;
mod stft;

pub use constants::*;
pub use instrument::*;
pub use params::*;

pub(crate) fn clamp(x: f32, min: f32, max: f32) -> f32 {
    x.max(min).min(max)
}

#[cfg(test)]
mod tests;
