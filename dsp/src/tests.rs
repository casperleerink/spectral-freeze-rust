use super::*;
use crate::processor::apply_organic_saturation;
use rustfft::num_complex::Complex32;
use std::f32::consts::PI;

#[test]
fn manifest_matches_constants() {
    assert_eq!(PARAMS.len(), 3);
    assert_eq!(PARAMS[PARAM_FREEZE].id, "freeze");
    assert_eq!(PARAMS[PARAM_ORGANIC].id, "organic");
}

#[test]
fn silence_stays_silent() {
    let mut processor = SpectralFreeze::default();
    processor.prepare(44_100.0, 2);
    let mut left = vec![0.0_f32; 4096];
    let mut right = vec![0.0_f32; 4096];
    let mut channels: [&mut [f32]; 2] = [&mut left, &mut right];
    processor.process_block(&mut channels, ProcessParams::default());
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
    processor.prepare(48_000.0, 1);
    let mut mono = vec![0.0_f32; FFT_SIZE * 4];
    mono[0] = 1.0;
    let mut channels: [&mut [f32]; 1] = [&mut mono];
    processor.process_block(&mut channels, ProcessParams::default());
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
        processor.prepare(44_100.0, 1);
        let mut mono = vec![0.0_f32; FFT_SIZE * 16];
        for (i, sample) in mono.iter_mut().enumerate() {
            *sample = (2.0 * PI * 440.0 * i as f32 / 44_100.0).sin() * 0.2;
        }
        let mut channels: [&mut [f32]; 1] = [&mut mono];
        processor.process_block(
            &mut channels,
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
    processor.prepare(44_100.0, 1);
    let mut mono = sine_buffer(440.0, 0.25, 44_100.0, FFT_SIZE * 8);
    let mut channels: [&mut [f32]; 1] = [&mut mono];
    processor.process_block(
        &mut channels,
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
    processor.prepare(sample_rate, 1);

    let mut prime = sine_buffer(440.0, 0.2, sample_rate, FFT_SIZE * 3);
    let mut channels: [&mut [f32]; 1] = [&mut prime];
    processor.process_block(&mut channels, ProcessParams::default());

    let mut capture = sine_buffer(440.0, 0.2, sample_rate, HOP_SIZE * 2);
    let mut channels: [&mut [f32]; 1] = [&mut capture];
    processor.process_block(
        &mut channels,
        ProcessParams {
            freeze: true,
            ..Default::default()
        },
    );

    let mut silent = vec![0.0_f32; FFT_SIZE * 6];
    let mut channels: [&mut [f32]; 1] = [&mut silent];
    processor.process_block(
        &mut channels,
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
fn freeze_recaptures_on_second_rising_edge() {
    let sample_rate = 44_100.0;
    let mut processor = SpectralFreeze::default();
    processor.prepare(sample_rate, 1);

    let mut first = sine_buffer(330.0, 0.2, sample_rate, FFT_SIZE * 5);
    let mut channels: [&mut [f32]; 1] = [&mut first];
    processor.process_block(
        &mut channels,
        ProcessParams {
            freeze: true,
            ..Default::default()
        },
    );

    let mut unfreeze = sine_buffer(880.0, 0.2, sample_rate, FFT_SIZE * 5);
    let mut channels: [&mut [f32]; 1] = [&mut unfreeze];
    processor.process_block(&mut channels, ProcessParams::default());

    let mut second = sine_buffer(880.0, 0.2, sample_rate, FFT_SIZE * 8);
    let mut channels: [&mut [f32]; 1] = [&mut second];
    processor.process_block(
        &mut channels,
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
    instrument.prepare(sample_rate, 1);
    instrument.note_on(FIRST_PAD_MIDI_NOTE, 0, 1.0, &pool, &assignments);

    let mut block = vec![0.0_f32; FFT_SIZE * 6];
    let mut channels: [&mut [f32]; 1] = [&mut block];
    instrument.process_block(
        &mut channels,
        InstrumentProcessParams {
            mag_glide_s: 0.0,
            phase_glide_s: 0.0,
            ..Default::default()
        },
        &pool,
    );
    let rms = (channels[0].iter().map(|x| x * x).sum::<f32>() / channels[0].len() as f32).sqrt();
    assert!(rms > 0.001, "assigned pad note produced silence, rms={rms}");
    assert!(instrument.active_pads()[0]);

    instrument.note_off(FIRST_PAD_MIDI_NOTE, 0, InstrumentProcessParams::default());
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
    instrument.prepare(sample_rate, 1);
    let params = InstrumentProcessParams::default();

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
