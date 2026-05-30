use crate::state::LoadedSource;
use std::path::Path;

pub(crate) fn load_wav(path: &Path) -> Result<LoadedSource, String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|err| format!("Failed to open WAV: {err}"))?;
    let spec = reader.spec();
    if spec.channels == 0 {
        return Err("WAV has no channels".to_string());
    }
    let channels = spec.channels as usize;
    let mut planar = vec![Vec::<f32>::new(); channels.min(2)];
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for (i, sample) in reader.samples::<f32>().enumerate() {
                let sample = sample.map_err(|err| format!("Failed to read WAV sample: {err}"))?;
                let ch = i % channels;
                if ch < planar.len() {
                    planar[ch].push(sample);
                }
            }
        }
        hound::SampleFormat::Int => {
            let denom =
                ((1_i64 << (spec.bits_per_sample.saturating_sub(1) as u32).min(30)) - 1) as f32;
            for (i, sample) in reader.samples::<i32>().enumerate() {
                let sample = sample.map_err(|err| format!("Failed to read WAV sample: {err}"))?
                    as f32
                    / denom.max(1.0);
                let ch = i % channels;
                if ch < planar.len() {
                    planar[ch].push(sample.clamp(-1.0, 1.0));
                }
            }
        }
    }
    if planar.iter().all(Vec::is_empty) {
        return Err("WAV contains no samples".to_string());
    }
    Ok(LoadedSource {
        path: path.to_owned(),
        sample_rate: spec.sample_rate as f32,
        channels: planar,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_wav_reads_stereo_int_file() {
        let path = std::env::temp_dir().join(format!(
            "spectral-freeze-load-wav-{}.wav",
            std::process::id()
        ));
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        {
            let mut writer = hound::WavWriter::create(&path, spec).unwrap();
            writer.write_sample::<i16>(16_384).unwrap();
            writer.write_sample::<i16>(-16_384).unwrap();
            writer.write_sample::<i16>(0).unwrap();
            writer.write_sample::<i16>(8_192).unwrap();
            writer.finalize().unwrap();
        }

        let loaded = load_wav(&path).unwrap();
        assert_eq!(loaded.sample_rate, 44_100.0);
        assert_eq!(loaded.channels.len(), 2);
        assert_eq!(loaded.channels[0].len(), 2);
        assert_eq!(loaded.channels[1].len(), 2);
        assert!(loaded.channels[0][0] > 0.4);
        assert!(loaded.channels[1][0] < -0.4);
        let _ = std::fs::remove_file(path);
    }
}
