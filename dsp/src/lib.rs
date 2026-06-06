//! Host-agnostic DSP for Spectral Freeze.
//!
//! This crate contains the complete audio algorithm. The native CLAP/VST3 shell
//! and the headless WAM shell both call into this processor and consume the same
//! parameter manifests. Runtime allocation is limited to construction and
//! `prepare()` calls; audio `process_block()` methods perform no heap allocation.

mod constants;
mod instrument;
mod params;
mod processor;
mod random;
mod state;
mod stft;

pub use constants::*;
pub use instrument::*;
pub use params::*;
pub use processor::SpectralFreeze;

pub(crate) fn clamp(x: f32, min: f32, max: f32) -> f32 {
    x.max(min).min(max)
}

#[cfg(test)]
mod tests;
