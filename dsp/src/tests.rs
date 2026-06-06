use super::*;
use crate::processor::apply_organic_saturation;
use rustfft::num_complex::Complex32;
use std::f32::consts::PI;

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
    assert!(
        channels
            .iter()
            .flat_map(|ch| ch.iter())
            .all(|sample| sample.abs() < 1.0e-6)
    );
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
    assert!(
        energy > 0.1,
        "expected overlap-add output energy, got {energy}"
    );
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
    assert!(
        (after - before).abs() <= before * 0.01,
        "saturation changed RMS: before={before}, after={after}"
    );
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
        processor.process_block(
            &mut channels,
            None,
            ProcessParams {
                organic,
                ..Default::default()
            },
        );
        let stable = &channels[0][FFT_SIZE * 2..];
        (stable.iter().map(|x| x * x).sum::<f32>() / stable.len() as f32).sqrt()
    }

    let dry = render_rms(0.0);
    let organic = render_rms(1.0);
    assert!(
        organic <= dry * 1.05,
        "organic raised output level: dry={dry}, organic={organic}"
    );
    assert!(
        organic >= dry * 0.80,
        "organic over-compensated output level: dry={dry}, organic={organic}"
    );
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
    processor.process_block(
        &mut channels,
        None,
        ProcessParams {
            freeze: true,
            ..Default::default()
        },
    );
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
    processor.process_block(
        &mut channels,
        None,
        ProcessParams {
            freeze: true,
            ..Default::default()
        },
    );

    let mut silent = vec![0.0_f32; FFT_SIZE * 6];
    let mut channels: [&mut [f32]; 1] = [&mut silent];
    processor.process_block(
        &mut channels,
        None,
        ProcessParams {
            freeze: true,
            ..Default::default()
        },
    );

    let held = &channels[0][FFT_SIZE * 2..];
    let rms = (held.iter().map(|x| x * x).sum::<f32>() / held.len() as f32).sqrt();
    assert!(
        rms > 0.01,
        "frozen output disappeared after input stopped, rms={rms}"
    );
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
    sc_processor.process_block(
        &mut with_sc_channels,
        Some(&sc_channels),
        ProcessParams::default(),
    );

    for (a, b) in no_sc_channels[0].iter().zip(with_sc_channels[0].iter()) {
        assert!(
            (a - b).abs() < 1.0e-6,
            "silent sidechain changed output: {a} vs {b}"
        );
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
        ProcessParams {
            sc_boost_db: 18.0,
            sc_freq_smoothing: 0.25,
            ..Default::default()
        },
    );

    let start = FFT_SIZE * 6;
    let analysed = &main_channels[0][start..];
    let a440 = sine_projection(analysed, 440.0, sample_rate, start);
    let a880 = sine_projection(analysed, 880.0, sample_rate, start);
    assert!(
        a880 / a440 > 0.45,
        "sidechain did not boost matched 880 Hz enough: 440={a440}, 880={a880}"
    );
}

#[test]
fn sidechain_boost_compensates_output_level() {
    let sample_rate = 44_100.0;
    let len = FFT_SIZE * 24;
    let input: Vec<f32> = (0..len)
        .map(|i| {
            let t = i as f32 / sample_rate;
            0.55 * (2.0 * PI * 440.0 * t).sin() + 0.12 * (2.0 * PI * 880.0 * t).sin()
        })
        .collect();

    let mut dry_processor = SpectralFreeze::default();
    dry_processor.prepare(sample_rate, 1, 0);
    let mut dry = input.clone();
    let mut dry_channels: [&mut [f32]; 1] = [&mut dry];
    dry_processor.process_block(
        &mut dry_channels,
        None,
        ProcessParams {
            sc_boost_db: 0.0,
            ..Default::default()
        },
    );

    let mut boosted_processor = SpectralFreeze::default();
    boosted_processor.prepare(sample_rate, 1, 1);
    let mut boosted = input;
    let mut sidechain = sine_buffer(880.0, 0.8, sample_rate, len);
    let mut boosted_channels: [&mut [f32]; 1] = [&mut boosted];
    let sc_channels: [&mut [f32]; 1] = [&mut sidechain];
    boosted_processor.process_block(
        &mut boosted_channels,
        Some(&sc_channels),
        ProcessParams {
            sc_boost_db: 18.0,
            sc_freq_smoothing: 0.25,
            ..Default::default()
        },
    );

    let start = FFT_SIZE * 6;
    let dry_stable = &dry_channels[0][start..];
    let boosted_stable = &boosted_channels[0][start..];
    let dry_peak = dry_stable.iter().fold(0.0_f32, |peak, x| peak.max(x.abs()));
    let boosted_peak = boosted_stable
        .iter()
        .fold(0.0_f32, |peak, x| peak.max(x.abs()));
    let dry_rms = (dry_stable.iter().map(|x| x * x).sum::<f32>() / dry_stable.len() as f32).sqrt();
    let boosted_rms =
        (boosted_stable.iter().map(|x| x * x).sum::<f32>() / boosted_stable.len() as f32).sqrt();
    let dry_ratio = sine_projection(dry_stable, 880.0, sample_rate, start)
        / sine_projection(dry_stable, 440.0, sample_rate, start);
    let boosted_ratio = sine_projection(boosted_stable, 880.0, sample_rate, start)
        / sine_projection(boosted_stable, 440.0, sample_rate, start);

    assert!(
        boosted_ratio > dry_ratio * 1.5,
        "sidechain did not lift matched content: dry={dry_ratio}, boosted={boosted_ratio}"
    );
    assert!(
        boosted_peak <= dry_peak * 1.05,
        "sidechain overdrives output peak: dry={dry_peak}, boosted={boosted_peak}"
    );
    assert!(
        boosted_rms <= dry_rms * 1.05,
        "sidechain overdrives output rms: dry={dry_rms}, boosted={boosted_rms}"
    );
}

#[test]
fn freeze_recaptures_on_second_rising_edge() {
    let sample_rate = 44_100.0;
    let mut processor = SpectralFreeze::default();
    processor.prepare(sample_rate, 1, 0);

    let mut first = sine_buffer(330.0, 0.2, sample_rate, FFT_SIZE * 5);
    let mut channels: [&mut [f32]; 1] = [&mut first];
    processor.process_block(
        &mut channels,
        None,
        ProcessParams {
            freeze: true,
            ..Default::default()
        },
    );

    let mut unfreeze = sine_buffer(880.0, 0.2, sample_rate, FFT_SIZE * 5);
    let mut channels: [&mut [f32]; 1] = [&mut unfreeze];
    processor.process_block(&mut channels, None, ProcessParams::default());

    let mut second = sine_buffer(880.0, 0.2, sample_rate, FFT_SIZE * 8);
    let mut channels: [&mut [f32]; 1] = [&mut second];
    processor.process_block(
        &mut channels,
        None,
        ProcessParams {
            freeze: true,
            ..Default::default()
        },
    );

    let start = FFT_SIZE * 4;
    let analysed = &channels[0][start..];
    let a330 = sine_projection(analysed, 330.0, sample_rate, start);
    let a880 = sine_projection(analysed, 880.0, sample_rate, start);
    assert!(
        a880 > a330 * 2.0,
        "second freeze edge did not recapture new tone: 330={a330}, 880={a880}"
    );
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
        ProcessParams {
            freeze: true,
            sc_boost_db: 9.0,
            ..Default::default()
        },
    );
    let stable = &main_channels[0][FFT_SIZE * 4..];
    let rms = (stable.iter().map(|x| x * x).sum::<f32>() / stable.len() as f32).sqrt();
    assert!(
        rms > 0.01,
        "freeze+sidechain startup produced no tone, rms={rms}"
    );
}

#[test]
fn capture_embeds_spectral_data_and_metadata() {
    let sample_rate = 44_100.0;
    let source = vec![sine_buffer(440.0, 0.25, sample_rate, FFT_SIZE * 4)];
    let item =
        capture_freeze_from_audio(&source, sample_rate, FFT_SIZE, Some("/tmp/vocal.wav"), 0.25)
            .expect("capture should succeed");
    assert_eq!(item.channel_count(), 1);
    assert_eq!(item.channels[0].mag.len(), NUM_BINS);
    assert_eq!(item.channels[0].phase.len(), NUM_BINS);
    assert_eq!(item.channels[0].phase_advance.len(), NUM_BINS);
    assert_eq!(item.source_path.as_deref(), Some("/tmp/vocal.wav"));
    assert_eq!(item.cursor_sample, FFT_SIZE);
    assert!(item.name.contains("vocal.wav @"));
    assert!((item.filter - 0.25).abs() < 1.0e-6);
}

#[test]
fn captured_freeze_tracks_source_phase_advance() {
    let sample_rate = 44_100.0;
    let frequency = 440.0;
    let source = vec![sine_buffer(frequency, 0.3, sample_rate, FFT_SIZE * 8)];
    let item = capture_freeze_from_audio(&source, sample_rate, FFT_SIZE * 4, None, 0.0)
        .expect("capture should succeed");

    let dominant_bin = item.channels[0]
        .mag
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(idx, _)| idx)
        .unwrap();
    let expected = 2.0 * PI * frequency * HOP_SIZE as f32 / sample_rate;
    let actual = item.channels[0].phase_advance[dominant_bin];
    assert!(
        (actual - expected).abs() < 0.25,
        "captured phase advance should follow source pitch: bin={dominant_bin}, expected={expected}, actual={actual}"
    );
}

#[test]
fn instrument_note_triggers_assigned_pad_and_releases() {
    let sample_rate = 44_100.0;
    let source = vec![sine_buffer(440.0, 0.3, sample_rate, FFT_SIZE * 4)];
    let item = capture_freeze_from_audio(&source, sample_rate, FFT_SIZE, None, 0.0).unwrap();
    let pool = vec![item];
    let mut assignments = [None; PAD_COUNT];
    assignments[0] = Some(0);

    let mut instrument = FreezeInstrument::default();
    instrument.prepare(sample_rate, 1, 0);
    instrument.note_on(FIRST_PAD_MIDI_NOTE, 0, 1.0, &pool, &assignments);

    let mut block = vec![0.0_f32; FFT_SIZE * 6];
    let mut channels: [&mut [f32]; 1] = [&mut block];
    instrument.process_block(
        &mut channels,
        None,
        InstrumentProcessParams {
            attack_s: 0.0,
            release_s: 0.05,
            ..Default::default()
        },
        &pool,
    );
    let rms = (channels[0].iter().map(|x| x * x).sum::<f32>() / channels[0].len() as f32).sqrt();
    assert!(rms > 0.001, "assigned pad note produced silence, rms={rms}");
    assert!(instrument.active_pads()[0]);

    instrument.note_off(
        FIRST_PAD_MIDI_NOTE,
        0,
        InstrumentProcessParams {
            release_s: 0.0,
            ..Default::default()
        },
    );
    assert!(!instrument.active_pads()[0]);
}

#[test]
fn sustain_pedal_holds_note_off_until_released() {
    let sample_rate = 44_100.0;
    let source = vec![sine_buffer(440.0, 0.3, sample_rate, FFT_SIZE * 4)];
    let item = capture_freeze_from_audio(&source, sample_rate, FFT_SIZE, None, 0.0).unwrap();
    let pool = vec![item];
    let mut assignments = [None; PAD_COUNT];
    assignments[0] = Some(0);
    let mut instrument = FreezeInstrument::default();
    instrument.prepare(sample_rate, 1, 0);
    let params = InstrumentProcessParams {
        release_s: 0.0,
        ..Default::default()
    };

    instrument.note_on(FIRST_PAD_MIDI_NOTE, 0, 1.0, &pool, &assignments);
    instrument.set_sustain(true, params);
    instrument.note_off(FIRST_PAD_MIDI_NOTE, 0, params);
    assert!(
        instrument.active_pads()[0],
        "sustain pedal should hold note-off"
    );
    instrument.set_sustain(false, params);
    assert!(
        !instrument.active_pads()[0],
        "pedal up should release sustained note"
    );
}
