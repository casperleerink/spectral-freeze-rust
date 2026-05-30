use nih_plug::prelude::*;
use spectral_freeze::SpectralFreezePlugin;

fn main() {
    if !nih_export_standalone_with_args::<SpectralFreezePlugin, _>(standalone_args()) {
        std::process::exit(1);
    }
}

fn standalone_args() -> Vec<String> {
    let mut args: Vec<String> = std::env::args().collect();

    #[cfg(target_os = "macos")]
    apply_macos_audio_defaults(&mut args);

    args
}

#[cfg(target_os = "macos")]
fn apply_macos_audio_defaults(args: &mut Vec<String>) {
    if has_flag(args, "--help") || has_flag(args, "-h") {
        return;
    }

    let has_sample_rate = has_option(args, "--sample-rate", "-r");
    let has_period_size = has_option(args, "--period-size", "-p");
    let has_audio_layout = has_option(args, "--audio-layout", "-l");

    if has_sample_rate && has_period_size && has_audio_layout {
        return;
    }

    let output_device_name = option_value(args, "--output-device");
    let Some(defaults) = preferred_coreaudio_defaults(output_device_name.as_deref()) else {
        return;
    };

    if !has_sample_rate {
        args.push("--sample-rate".to_string());
        args.push(defaults.sample_rate.to_string());
    }
    if !has_period_size {
        args.push("--period-size".to_string());
        args.push(defaults.period_size.to_string());
    }
    if !has_audio_layout && defaults.output_channels == 1 {
        args.push("--audio-layout".to_string());
        args.push("2".to_string());
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct AudioDefaults {
    output_channels: u16,
    sample_rate: u32,
    period_size: u32,
}

#[cfg(target_os = "macos")]
fn preferred_coreaudio_defaults(output_device_name: Option<&str>) -> Option<AudioDefaults> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::host_from_id(cpal::HostId::CoreAudio).ok()?;
    let output_device = if let Some(name) = output_device_name {
        host.output_devices()
            .ok()?
            .find(|device| device.name().as_deref().map(|n| n == name).unwrap_or(false))?
    } else {
        host.default_output_device()?
    };

    let default_sample_rate = output_device
        .default_output_config()
        .ok()
        .map(|config| config.sample_rate().0);
    let configs: Vec<_> = output_device.supported_output_configs().ok()?.collect();

    for output_channels in [2, 1] {
        for config in configs
            .iter()
            .filter(|config| config.channels() == output_channels)
        {
            let cpal::SupportedBufferSize::Range { min, max } = config.buffer_size() else {
                continue;
            };
            let period_size = 512.clamp(*min, *max);
            let sample_rate = [
                default_sample_rate.unwrap_or(0),
                48_000,
                44_100,
                config.min_sample_rate().0,
            ]
            .into_iter()
            .find(|rate| {
                *rate > 0
                    && (config.min_sample_rate().0..=config.max_sample_rate().0).contains(rate)
            })?;

            return Some(AudioDefaults {
                output_channels,
                sample_rate,
                period_size,
            });
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn has_flag(args: &[String], long: &str) -> bool {
    args.iter().skip(1).any(|arg| arg == long)
}

#[cfg(target_os = "macos")]
fn has_option(args: &[String], long: &str, short: &str) -> bool {
    args.iter().skip(1).any(|arg| {
        arg == long
            || arg == short
            || arg.starts_with(&format!("{long}="))
            || arg.starts_with(short)
    })
}

#[cfg(target_os = "macos")]
fn option_value(args: &[String], long: &str) -> Option<String> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == long {
            return iter.next().cloned();
        }
        if let Some(value) = arg.strip_prefix(&format!("{long}=")) {
            return Some(value.to_string());
        }
    }
    None
}
