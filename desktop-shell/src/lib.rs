use nih_plug::prelude::*;

mod params;
mod plugin;
mod source;
mod state;
mod ui;

pub use plugin::SpectralFreezePlugin;

nih_export_clap!(SpectralFreezePlugin);
nih_export_vst3!(SpectralFreezePlugin);
