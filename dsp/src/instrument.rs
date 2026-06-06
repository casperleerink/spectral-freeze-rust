use crate::clamp;
use crate::constants::*;
use crate::params::{
    PARAM_ORGANIC, PARAM_SC_BOOST, PARAM_SC_FREQ_SMOOTHING, PARAMS, ParamInfo, ParamKind,
    ProcessParams,
};
use crate::processor::{
    SpectralFreeze, apply_synthesis_window, normalize_inverse_fft, rebuild_conjugate_mirror,
};
use crate::random::JuceRandom;
use crate::stft::{calculate_window_gain, fill_hann_window, phase_advance_for_bin};
use rustfft::{Fft, FftPlanner, num_complex::Complex32};
use std::f32::consts::PI;
use std::sync::Arc;

pub const PAD_COUNT: usize = 16;
pub const MAX_INSTRUMENT_VOICES: usize = 16;
pub const FIRST_PAD_MIDI_NOTE: u8 = 36; // C1 through D#2

pub const PARAM_ATTACK: usize = 0;
pub const PARAM_DECAY: usize = 1;
pub const PARAM_SUSTAIN: usize = 2;
pub const PARAM_RELEASE: usize = 3;
pub const PARAM_INSTRUMENT_ORGANIC: usize = 4;
pub const PARAM_INSTRUMENT_SC_BOOST: usize = 5;
pub const PARAM_INSTRUMENT_SC_FREQ_SMOOTHING: usize = 6;

pub const INSTRUMENT_PARAMS: [ParamInfo; 7] = [
    ParamInfo {
        id: "attack",
        name: "Attack",
        kind: ParamKind::Float,
        min: 0.0,
        max: 5.0,
        default: 0.010,
        unit: " s",
    },
    ParamInfo {
        id: "decay",
        name: "Decay",
        kind: ParamKind::Float,
        min: 0.0,
        max: 5.0,
        default: 0.100,
        unit: " s",
    },
    ParamInfo {
        id: "sustain",
        name: "Sustain",
        kind: ParamKind::Float,
        min: 0.0,
        max: 1.0,
        default: 1.0,
        unit: "%",
    },
    ParamInfo {
        id: "release",
        name: "Release",
        kind: ParamKind::Float,
        min: 0.0,
        max: 10.0,
        default: 0.250,
        unit: " s",
    },
    ParamInfo {
        id: "organic",
        name: "Organic",
        kind: ParamKind::Float,
        min: 0.0,
        max: 1.0,
        default: PARAMS[PARAM_ORGANIC].default,
        unit: "%",
    },
    ParamInfo {
        id: "scBoost",
        name: "SC Boost",
        kind: ParamKind::Float,
        min: 0.0,
        max: 18.0,
        default: PARAMS[PARAM_SC_BOOST].default,
        unit: " dB",
    },
    ParamInfo {
        id: "scFreqSmoothing",
        name: "SC Freq Smooth",
        kind: ParamKind::Float,
        min: 0.0,
        max: 1.0,
        default: PARAMS[PARAM_SC_FREQ_SMOOTHING].default,
        unit: "%",
    },
];

#[derive(Clone, Copy, Debug)]
pub struct InstrumentProcessParams {
    pub attack_s: f32,
    pub decay_s: f32,
    pub sustain: f32,
    pub release_s: f32,
    pub organic: f32,
    pub sc_boost_db: f32,
    pub sc_freq_smoothing: f32,
}

impl Default for InstrumentProcessParams {
    fn default() -> Self {
        Self {
            attack_s: INSTRUMENT_PARAMS[PARAM_ATTACK].default,
            decay_s: INSTRUMENT_PARAMS[PARAM_DECAY].default,
            sustain: INSTRUMENT_PARAMS[PARAM_SUSTAIN].default,
            release_s: INSTRUMENT_PARAMS[PARAM_RELEASE].default,
            organic: INSTRUMENT_PARAMS[PARAM_INSTRUMENT_ORGANIC].default,
            sc_boost_db: INSTRUMENT_PARAMS[PARAM_INSTRUMENT_SC_BOOST].default,
            sc_freq_smoothing: INSTRUMENT_PARAMS[PARAM_INSTRUMENT_SC_FREQ_SMOOTHING].default,
        }
    }
}

impl InstrumentProcessParams {
    pub fn clamped(self) -> Self {
        Self {
            attack_s: clamp(self.attack_s, 0.0, 5.0),
            decay_s: clamp(self.decay_s, 0.0, 5.0),
            sustain: clamp(self.sustain, 0.0, 1.0),
            release_s: clamp(self.release_s, 0.0, 10.0),
            organic: clamp(self.organic, 0.0, 1.0),
            sc_boost_db: clamp(self.sc_boost_db, 0.0, 18.0),
            sc_freq_smoothing: clamp(self.sc_freq_smoothing, 0.0, 1.0),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FrozenChannelData {
    pub mag: Vec<f32>,
    pub phase: Vec<f32>,
    pub phase_advance: Vec<f32>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CapturedFreeze {
    pub name: String,
    pub source_path: Option<String>,
    pub source_sample_rate: f32,
    pub cursor_sample: usize,
    pub cursor_time_seconds: f32,
    pub filter: f32,
    pub channels: Vec<FrozenChannelData>,
}

impl CapturedFreeze {
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }
}

pub fn note_to_pad(note: u8) -> Option<usize> {
    if (FIRST_PAD_MIDI_NOTE..FIRST_PAD_MIDI_NOTE + PAD_COUNT as u8).contains(&note) {
        Some((note - FIRST_PAD_MIDI_NOTE) as usize)
    } else {
        None
    }
}

pub fn pad_note(pad: usize) -> u8 {
    FIRST_PAD_MIDI_NOTE + pad.min(PAD_COUNT - 1) as u8
}

pub fn note_label(note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = note as i16 / 12 - 2;
    format!("{}{}", NAMES[note as usize % 12], octave)
}

pub fn capture_freeze_from_audio(
    source_channels: &[Vec<f32>],
    source_sample_rate: f32,
    cursor_sample: usize,
    source_path: Option<&str>,
    filter: f32,
) -> Option<CapturedFreeze> {
    if source_channels.is_empty() || source_sample_rate <= 0.0 {
        return None;
    }

    let max_len = source_channels.iter().map(Vec::len).max().unwrap_or(0);
    if max_len == 0 {
        return None;
    }

    let safe_cursor = cursor_sample.min(max_len - 1);
    let frame_start = safe_cursor.saturating_sub(FFT_SIZE / 2);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let mut window = [0.0_f32; FFT_SIZE];
    fill_hann_window(&mut window);

    let mut channels = Vec::with_capacity(source_channels.len().min(2));
    let mut scratch = [Complex32::new(0.0, 0.0); FFT_SIZE];
    for src in source_channels.iter().take(2) {
        let mut mag_sum = vec![0.0_f32; NUM_BINS];
        let mut phase = vec![0.0_f32; NUM_BINS];
        let mut phase_advance = vec![0.0_f32; NUM_BINS];
        let mut last_phase = vec![0.0_f32; NUM_BINS];
        let mut has_last_phase = false;

        for frame_idx in 0..MAG_HISTORY_SIZE {
            let lookback_hops = MAG_HISTORY_SIZE - 1 - frame_idx;
            let analysis_start = frame_start.saturating_sub(lookback_hops * HOP_SIZE);
            for i in 0..FFT_SIZE {
                let sample = src.get(analysis_start + i).copied().unwrap_or(0.0);
                scratch[i] = Complex32::new(sample * window[i], 0.0);
            }
            fft.process(&mut scratch);

            for k in 0..NUM_BINS {
                let c = scratch[k];
                let bin_phase_advance = phase_advance_for_bin(k);
                let bin_phase = c.im.atan2(c.re);
                mag_sum[k] += c.norm();

                if has_last_phase {
                    let mut deviation = bin_phase - last_phase[k] - bin_phase_advance;
                    while deviation > PI {
                        deviation -= 2.0 * PI;
                    }
                    while deviation < -PI {
                        deviation += 2.0 * PI;
                    }
                    let measured_advance = bin_phase_advance + deviation;
                    phase_advance[k] = 0.65 * phase_advance[k] + 0.35 * measured_advance;
                } else {
                    phase_advance[k] = bin_phase_advance;
                }

                last_phase[k] = bin_phase;
                phase[k] = bin_phase;
            }
            has_last_phase = true;
        }

        let history_gain = (MAG_HISTORY_SIZE as f32).recip();
        let mag = mag_sum.into_iter().map(|m| m * history_gain).collect();
        channels.push(FrozenChannelData {
            mag,
            phase,
            phase_advance,
        });
    }

    let cursor_time_seconds = safe_cursor as f32 / source_sample_rate;
    let file_name = source_path
        .and_then(|p| std::path::Path::new(p).file_name())
        .and_then(|p| p.to_str())
        .unwrap_or("audio.wav");
    let name = format!("{file_name} @ {}", format_time(cursor_time_seconds));

    Some(CapturedFreeze {
        name,
        source_path: source_path.map(ToOwned::to_owned),
        source_sample_rate,
        cursor_sample: safe_cursor,
        cursor_time_seconds,
        filter: clamp(filter, 0.0, 1.0),
        channels,
    })
}

fn format_time(seconds: f32) -> String {
    let total_ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let minutes = total_ms / 60_000;
    let secs = (total_ms / 1000) % 60;
    let millis = total_ms % 1000;
    format!("{minutes:02}:{secs:02}.{millis:03}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnvStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

struct InstrumentVoice {
    active: bool,
    pad_index: usize,
    item_index: usize,
    note: u8,
    channel: u8,
    velocity: f32,
    env_stage: EnvStage,
    env_level: f32,
    release_start_level: f32,
    phase: Vec<Box<[f32; NUM_BINS]>>,
    output_fifo: Vec<Box<[f32; FFT_SIZE]>>,
    spectrum: Box<[Complex32; FFT_SIZE]>,
    fifo_pos: usize,
    hop_counter: usize,
    rng: JuceRandom,
}

impl InstrumentVoice {
    fn new(output_channels: usize, seed: u32) -> Self {
        Self {
            active: false,
            pad_index: 0,
            item_index: 0,
            note: FIRST_PAD_MIDI_NOTE,
            channel: 0,
            velocity: 0.0,
            env_stage: EnvStage::Idle,
            env_level: 0.0,
            release_start_level: 0.0,
            phase: (0..output_channels)
                .map(|_| Box::new([0.0; NUM_BINS]))
                .collect(),
            output_fifo: (0..output_channels)
                .map(|_| Box::new([0.0; FFT_SIZE]))
                .collect(),
            spectrum: Box::new([Complex32::new(0.0, 0.0); FFT_SIZE]),
            fifo_pos: 0,
            hop_counter: HOP_SIZE,
            rng: JuceRandom::new(seed as u64),
        }
    }

    fn prepare_channels(&mut self, output_channels: usize) {
        if self.phase.len() != output_channels {
            self.phase = (0..output_channels)
                .map(|_| Box::new([0.0; NUM_BINS]))
                .collect();
            self.output_fifo = (0..output_channels)
                .map(|_| Box::new([0.0; FFT_SIZE]))
                .collect();
        }
        self.clear_buffers();
    }

    fn clear_buffers(&mut self) {
        for fifo in &mut self.output_fifo {
            fifo.fill(0.0);
        }
        self.spectrum.fill(Complex32::new(0.0, 0.0));
        self.fifo_pos = 0;
        self.hop_counter = HOP_SIZE;
    }

    fn start(
        &mut self,
        pad_index: usize,
        item_index: usize,
        item: &CapturedFreeze,
        note: u8,
        channel: u8,
        velocity: f32,
    ) {
        self.active = true;
        self.pad_index = pad_index;
        self.item_index = item_index;
        self.note = note;
        self.channel = channel;
        self.velocity = clamp(velocity, 0.0, 1.0);
        self.env_stage = EnvStage::Attack;
        self.env_level = 0.0;
        self.release_start_level = 0.0;
        self.clear_buffers();
        if !item.channels.is_empty() {
            for out_ch in 0..self.phase.len() {
                let src_ch = out_ch.min(item.channels.len() - 1);
                for k in 0..NUM_BINS {
                    self.phase[out_ch][k] =
                        item.channels[src_ch].phase.get(k).copied().unwrap_or(0.0);
                }
            }
        }
    }

    fn release(&mut self, params: InstrumentProcessParams) {
        if self.active && self.env_stage != EnvStage::Release {
            if params.release_s <= 0.0 || self.env_level <= 1.0e-5 {
                self.stop();
            } else {
                self.env_stage = EnvStage::Release;
                self.release_start_level = self.env_level;
            }
        }
    }

    fn stop(&mut self) {
        self.active = false;
        self.env_stage = EnvStage::Idle;
        self.env_level = 0.0;
        self.clear_buffers();
    }

    fn next_envelope(&mut self, sample_rate: f32, params: InstrumentProcessParams) -> f32 {
        match self.env_stage {
            EnvStage::Idle => 0.0,
            EnvStage::Attack => {
                if params.attack_s <= 0.0 {
                    self.env_level = 1.0;
                    self.env_stage = EnvStage::Decay;
                } else {
                    self.env_level += 1.0 / (params.attack_s * sample_rate).max(1.0);
                    if self.env_level >= 1.0 {
                        self.env_level = 1.0;
                        self.env_stage = EnvStage::Decay;
                    }
                }
                self.env_level
            }
            EnvStage::Decay => {
                if params.decay_s <= 0.0 {
                    self.env_level = params.sustain;
                    self.env_stage = EnvStage::Sustain;
                } else {
                    self.env_level -=
                        (1.0 - params.sustain) / (params.decay_s * sample_rate).max(1.0);
                    if self.env_level <= params.sustain {
                        self.env_level = params.sustain;
                        self.env_stage = EnvStage::Sustain;
                    }
                }
                self.env_level
            }
            EnvStage::Sustain => {
                self.env_level = params.sustain;
                self.env_level
            }
            EnvStage::Release => {
                if params.release_s <= 0.0 {
                    self.stop();
                    return 0.0;
                }
                self.env_level -=
                    self.release_start_level / (params.release_s * sample_rate).max(1.0);
                if self.env_level <= 1.0e-5 {
                    self.stop();
                    0.0
                } else {
                    self.env_level
                }
            }
        }
    }

    fn render_frame(
        &mut self,
        item: &CapturedFreeze,
        window: &[f32; FFT_SIZE],
        window_gain: f32,
        inverse_fft: &Arc<dyn Fft<f32>>,
    ) {
        if item.channels.is_empty() {
            return;
        }
        for out_ch in 0..self.output_fifo.len() {
            self.spectrum.fill(Complex32::new(0.0, 0.0));
            let src_ch = out_ch.min(item.channels.len() - 1);
            let channel = &item.channels[src_ch];
            let max_mag = channel.mag.iter().copied().fold(0.0_f32, f32::max);
            let threshold = max_mag * item.filter * item.filter;
            for k in 0..NUM_BINS {
                let mag = channel.mag.get(k).copied().unwrap_or(0.0);
                let phase_advance = channel.phase_advance.get(k).copied().unwrap_or(0.0);
                let mut phase = self.phase[out_ch][k]
                    + phase_advance
                    + self.rng.bipolar() * FREEZE_PHASE_JITTER_RADIANS;
                if phase > PI {
                    phase -= 2.0 * PI;
                } else if phase < -PI {
                    phase += 2.0 * PI;
                }
                self.phase[out_ch][k] = phase;
                self.spectrum[k] = if mag >= threshold {
                    Complex32::from_polar(mag, phase)
                } else {
                    Complex32::new(0.0, 0.0)
                };
            }
            rebuild_conjugate_mirror(self.spectrum.as_mut());
            inverse_fft.process(self.spectrum.as_mut_slice());
            normalize_inverse_fft(self.spectrum.as_mut());
            apply_synthesis_window(self.spectrum.as_mut(), window, window_gain);
            let fifo = &mut self.output_fifo[out_ch];
            for i in 0..FFT_SIZE {
                fifo[(self.fifo_pos + i) % FFT_SIZE] += self.spectrum[i].re;
            }
        }
    }
}

pub struct FreezeInstrument {
    sample_rate: f32,
    output_channels: usize,
    window: Box<[f32; FFT_SIZE]>,
    window_gain: f32,
    voices: Vec<InstrumentVoice>,
    sustain_down: bool,
    sustained_pads: [bool; PAD_COUNT],
    output_fx: SpectralFreeze,
    inverse_fft: Arc<dyn Fft<f32>>,
}

impl Default for FreezeInstrument {
    fn default() -> Self {
        let mut this = Self {
            sample_rate: 44_100.0,
            output_channels: 2,
            window: Box::new([0.0; FFT_SIZE]),
            window_gain: 1.0,
            voices: Vec::new(),
            sustain_down: false,
            sustained_pads: [false; PAD_COUNT],
            output_fx: SpectralFreeze::default(),
            inverse_fft: FftPlanner::<f32>::new().plan_fft_inverse(FFT_SIZE),
        };
        this.prepare(44_100.0, 2, 0);
        this
    }
}

impl FreezeInstrument {
    pub fn prepare(&mut self, sample_rate: f32, output_channels: usize, sidechain_channels: usize) {
        self.sample_rate = if sample_rate > 0.0 {
            sample_rate
        } else {
            44_100.0
        };
        self.output_channels = output_channels.max(1);
        fill_hann_window(self.window.as_mut());
        self.window_gain = calculate_window_gain(self.window.as_ref());

        if self.voices.len() != MAX_INSTRUMENT_VOICES {
            self.voices = (0..MAX_INSTRUMENT_VOICES)
                .map(|i| InstrumentVoice::new(self.output_channels, 0x51f0_0000 ^ i as u32))
                .collect();
        }
        for voice in &mut self.voices {
            voice.prepare_channels(self.output_channels);
        }
        self.sustain_down = false;
        self.sustained_pads = [false; PAD_COUNT];
        self.output_fx
            .prepare(self.sample_rate, self.output_channels, sidechain_channels);
    }

    pub fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.stop();
        }
        self.sustain_down = false;
        self.sustained_pads = [false; PAD_COUNT];
        self.output_fx.reset();
    }

    pub fn note_on(
        &mut self,
        note: u8,
        channel: u8,
        velocity: f32,
        pool: &[CapturedFreeze],
        assignments: &[Option<usize>; PAD_COUNT],
    ) {
        let Some(pad) = note_to_pad(note) else {
            return;
        };
        let Some(item_index) = assignments[pad] else {
            return;
        };
        let Some(item) = pool.get(item_index) else {
            return;
        };
        for voice in &mut self.voices {
            if voice.active && voice.pad_index == pad {
                voice.stop();
            }
        }
        let slot = self
            .voices
            .iter()
            .position(|v| !v.active)
            .unwrap_or(pad % self.voices.len());
        self.voices[slot].start(pad, item_index, item, note, channel, velocity);
        self.sustained_pads[pad] = false;
    }

    pub fn note_off(&mut self, note: u8, channel: u8, params: InstrumentProcessParams) {
        let Some(pad) = note_to_pad(note) else {
            return;
        };
        if self.sustain_down {
            self.sustained_pads[pad] = true;
            return;
        }
        for voice in &mut self.voices {
            if voice.active
                && voice.pad_index == pad
                && voice.note == note
                && voice.channel == channel
            {
                voice.release(params.clamped());
            }
        }
    }

    pub fn set_sustain(&mut self, down: bool, params: InstrumentProcessParams) {
        if self.sustain_down && !down {
            let params = params.clamped();
            for pad in 0..PAD_COUNT {
                if self.sustained_pads[pad] {
                    for voice in &mut self.voices {
                        if voice.active && voice.pad_index == pad {
                            voice.release(params);
                        }
                    }
                }
                self.sustained_pads[pad] = false;
            }
        }
        self.sustain_down = down;
    }

    pub fn active_pads(&self) -> [bool; PAD_COUNT] {
        let mut active = [false; PAD_COUNT];
        for voice in &self.voices {
            if voice.active && voice.pad_index < PAD_COUNT {
                active[voice.pad_index] = true;
            }
        }
        active
    }

    pub fn process_block(
        &mut self,
        main: &mut [&mut [f32]],
        sidechain: Option<&[&mut [f32]]>,
        params: InstrumentProcessParams,
        pool: &[CapturedFreeze],
    ) {
        self.process_block_inner(main, sidechain, params, pool, true);
    }

    pub fn process_block_additive(
        &mut self,
        main: &mut [&mut [f32]],
        params: InstrumentProcessParams,
        pool: &[CapturedFreeze],
    ) {
        self.process_block_inner(main, None, params, pool, false);
    }

    fn process_block_inner(
        &mut self,
        main: &mut [&mut [f32]],
        sidechain: Option<&[&mut [f32]]>,
        params: InstrumentProcessParams,
        pool: &[CapturedFreeze],
        clear_outputs: bool,
    ) {
        let params = params.clamped();
        let num_samples = main.first().map_or(0, |ch| ch.len());
        let channels = main.len().min(self.output_channels);
        if clear_outputs {
            for ch in main.iter_mut() {
                ch.fill(0.0);
            }
        }
        for n in 0..num_samples {
            for voice in &mut self.voices {
                if !voice.active {
                    continue;
                }
                let Some(item) = pool.get(voice.item_index) else {
                    voice.stop();
                    continue;
                };
                if voice.hop_counter >= HOP_SIZE {
                    voice.render_frame(
                        item,
                        self.window.as_ref(),
                        self.window_gain,
                        &self.inverse_fft,
                    );
                    voice.hop_counter = 0;
                }
                let env = voice.next_envelope(self.sample_rate, params) * voice.velocity;
                for ch in 0..channels {
                    main[ch][n] += voice.output_fifo[ch][voice.fifo_pos] * env;
                    voice.output_fifo[ch][voice.fifo_pos] = 0.0;
                }
                voice.fifo_pos = (voice.fifo_pos + 1) % FFT_SIZE;
                voice.hop_counter += 1;
            }
        }

        if clear_outputs && (params.organic > 0.0 || params.sc_boost_db > 0.0) {
            self.output_fx.process_block(
                main,
                sidechain,
                ProcessParams {
                    freeze: false,
                    filter: 0.0,
                    sc_boost_db: params.sc_boost_db,
                    sc_freq_smoothing: params.sc_freq_smoothing,
                    organic: params.organic,
                },
            );
        }
    }
}
