use nih_plug::prelude::*;
use spectral_freeze::SpectralFreezePlugin;

fn main() {
    if !nih_export_standalone::<SpectralFreezePlugin>() {
        std::process::exit(1);
    }
}
