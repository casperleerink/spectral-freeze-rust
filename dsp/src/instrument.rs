use crate::clamp;
use crate::constants::*;
use crate::params::{PARAM_ORGANIC, PARAMS, ParamInfo, ParamKind};
use crate::processor::{
    apply_organic_saturation, apply_organic_spectral_processing, apply_synthesis_window,
    normalize_inverse_fft, rebuild_conjugate_mirror,
};
use crate::random::JuceRandom;
use crate::state::{OrganicAmState, OrganicScratch};
use crate::stft::{calculate_window_gain, fill_hann_window, phase_advance_for_bin};
use rustfft::{Fft, FftPlanner, num_complex::Complex32};
use std::f32::consts::PI;
use std::sync::Arc;

pub const PAD_COUNT: usize = 16;
pub const FIRST_PAD_MIDI_NOTE: u8 = 36; // C1 through D#2

pub const PARAM_ATTACK: usize = 0;
pub const PARAM_RELEASE: usize = 1;
pub const PARAM_GLIDE: usize = 2;
pub const PARAM_INSTRUMENT_ORGANIC: usize = 3;

pub const INSTRUMENT_PARAMS: [ParamInfo; 4] = [
    ParamInfo {
        id: "attack",
        name: "Attack",
        kind: ParamKind::Float,
        min: 0.0,
        max: 5.0,
        default: 0.008,
        unit: " s",
    },
    ParamInfo {
        id: "release",
        name: "Release",
        kind: ParamKind::Float,
        min: 0.0,
        max: 10.0,
        default: 0.180,
        unit: " s",
    },
    ParamInfo {
        id: "glide",
        name: "Glide",
        kind: ParamKind::Float,
        min: 0.0,
        max: 5.0,
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
];

#[derive(Clone, Copy, Debug)]
pub struct InstrumentProcessParams {
    pub attack_s: f32,
    pub release_s: f32,
    pub glide_s: f32,
    pub organic: f32,
}

impl Default for InstrumentProcessParams {
    fn default() -> Self {
        Self {
            attack_s: INSTRUMENT_PARAMS[PARAM_ATTACK].default,
            release_s: INSTRUMENT_PARAMS[PARAM_RELEASE].default,
            glide_s: INSTRUMENT_PARAMS[PARAM_GLIDE].default,
            organic: INSTRUMENT_PARAMS[PARAM_INSTRUMENT_ORGANIC].default,
        }
    }
}

impl InstrumentProcessParams {
    pub fn clamped(self) -> Self {
        Self {
            attack_s: clamp(self.attack_s, 0.0, 5.0),
            release_s: clamp(self.release_s, 0.0, 10.0),
            glide_s: clamp(self.glide_s, 0.0, 5.0),
            organic: clamp(self.organic, 0.0, 1.0),
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

const PHASE_GLIDE_RATIO: f32 = 1.0;
const SILENCE_AMP: f32 = 1.0e-5;

fn glide_coeff(time_s: f32, sample_rate: f32) -> f32 {
    if time_s <= 0.0 || sample_rate <= 0.0 {
        1.0
    } else {
        1.0 - (-(HOP_SIZE as f32) / (time_s * sample_rate)).exp()
    }
}

fn one_pole_coeff(time_s: f32, sample_rate: f32) -> f32 {
    if time_s <= 0.0 || sample_rate <= 0.0 {
        1.0
    } else {
        1.0 - (-(1.0_f32) / (time_s * sample_rate)).exp()
    }
}

fn wrap_phase(phase: f32) -> f32 {
    (phase + PI).rem_euclid(2.0 * PI) - PI
}

#[derive(Clone, Debug)]
struct HeldNote {
    pad: usize,
    item_index: usize,
    note: u8,
    channel: u8,
    velocity: f32,
    physically_held: bool,
}

struct MonoSpectralEngine {
    gate: bool,
    held_notes: Vec<HeldNote>,
    active_target: Option<HeldNote>,
    sustain_down: bool,
    current_mag: Vec<Box<[f32; NUM_BINS]>>,
    current_phase: Vec<Box<[f32; NUM_BINS]>>,
    current_phase_advance: Vec<Box<[f32; NUM_BINS]>>,
    organic_am: Vec<OrganicAmState>,
    organic_scratch: Vec<OrganicScratch>,
    amp: f32,
    output_fifo: Vec<Box<[f32; FFT_SIZE]>>,
    spectrum: Box<[Complex32; FFT_SIZE]>,
    fifo_pos: usize,
    hop_counter: usize,
    rng: JuceRandom,
}

impl MonoSpectralEngine {
    fn new(output_channels: usize) -> Self {
        let mut this = Self {
            gate: false,
            held_notes: Vec::new(),
            active_target: None,
            sustain_down: false,
            current_mag: Vec::new(),
            current_phase: Vec::new(),
            current_phase_advance: Vec::new(),
            organic_am: Vec::new(),
            organic_scratch: Vec::new(),
            amp: 0.0,
            output_fifo: Vec::new(),
            spectrum: Box::new([Complex32::new(0.0, 0.0); FFT_SIZE]),
            fifo_pos: 0,
            hop_counter: HOP_SIZE,
            rng: JuceRandom::new(0x51f0_fade),
        };
        this.prepare_channels(output_channels);
        this
    }

    fn prepare_channels(&mut self, output_channels: usize) {
        if self.current_mag.len() != output_channels {
            self.current_mag = (0..output_channels)
                .map(|_| Box::new([0.0; NUM_BINS]))
                .collect();
            self.current_phase = (0..output_channels)
                .map(|_| Box::new([0.0; NUM_BINS]))
                .collect();
            self.current_phase_advance = (0..output_channels)
                .map(|_| Box::new([0.0; NUM_BINS]))
                .collect();
            self.output_fifo = (0..output_channels)
                .map(|_| Box::new([0.0; FFT_SIZE]))
                .collect();
            self.organic_am = (0..output_channels)
                .map(|_| OrganicAmState::default())
                .collect();
            self.organic_scratch = (0..output_channels)
                .map(|_| OrganicScratch::default())
                .collect();
        }
    }

    fn reset(&mut self) {
        self.gate = false;
        self.held_notes.clear();
        self.active_target = None;
        self.sustain_down = false;
        self.amp = 0.0;
        self.clear_spectral_state();
        for organic_am in &mut self.organic_am {
            organic_am.value.fill(0.0);
            organic_am.hop_counter = 0;
            for target in &mut organic_am.target {
                *target = self.rng.bipolar();
            }
        }
        self.clear_output_buffers();
    }

    fn clear_spectral_state(&mut self) {
        for ch in 0..self.current_mag.len() {
            self.current_mag[ch].fill(0.0);
            self.current_phase[ch].fill(0.0);
            self.current_phase_advance[ch].fill(0.0);
        }
        self.spectrum.fill(Complex32::new(0.0, 0.0));
    }

    fn clear_output_buffers(&mut self) {
        for fifo in &mut self.output_fifo {
            fifo.fill(0.0);
        }
        self.fifo_pos = 0;
        self.hop_counter = HOP_SIZE;
    }

    fn note_on(
        &mut self,
        pad: usize,
        item_index: usize,
        item: &CapturedFreeze,
        note: u8,
        channel: u8,
        velocity: f32,
    ) {
        self.held_notes
            .retain(|held| held.note != note || held.channel != channel);

        let fresh_articulation =
            self.held_notes.is_empty() && (!self.gate || self.amp <= SILENCE_AMP);
        let held = HeldNote {
            pad,
            item_index,
            note,
            channel,
            velocity: clamp(velocity, 0.0, 1.0),
            physically_held: true,
        };
        self.held_notes.push(held.clone());
        self.active_target = Some(held);
        self.gate = true;

        if fresh_articulation {
            self.seed_from_target(item);
            self.clear_output_buffers();
        }
    }

    fn note_off(&mut self, note: u8, channel: u8) {
        if self.sustain_down {
            for held in &mut self.held_notes {
                if held.note == note && held.channel == channel {
                    held.physically_held = false;
                }
            }
        } else {
            self.held_notes
                .retain(|held| held.note != note || held.channel != channel);
        }
        self.recompute_active_target();
    }

    fn set_sustain(&mut self, down: bool) {
        if self.sustain_down && !down {
            self.held_notes.retain(|held| held.physically_held);
            self.recompute_active_target();
        }
        self.sustain_down = down;
    }

    fn recompute_active_target(&mut self) {
        self.active_target = self.held_notes.last().cloned();
        self.gate = self.active_target.is_some();
    }

    fn active_pads(&self) -> [bool; PAD_COUNT] {
        let mut active = [false; PAD_COUNT];
        for held in &self.held_notes {
            if held.pad < PAD_COUNT {
                active[held.pad] = true;
            }
        }
        if self.gate {
            if let Some(target) = &self.active_target {
                if target.pad < PAD_COUNT {
                    active[target.pad] = true;
                }
            }
        }
        active
    }

    fn seed_from_target(&mut self, item: &CapturedFreeze) {
        if item.channels.is_empty() {
            return;
        }

        for out_ch in 0..self.current_mag.len() {
            let src_ch = out_ch.min(item.channels.len() - 1);
            let channel = &item.channels[src_ch];
            let max_mag = channel.mag.iter().copied().fold(0.0_f32, f32::max);
            let threshold = max_mag * item.filter * item.filter;

            for k in 0..NUM_BINS {
                let raw_mag = channel.mag.get(k).copied().unwrap_or(0.0);
                self.current_mag[out_ch][k] = if raw_mag >= threshold { raw_mag } else { 0.0 };
                self.current_phase[out_ch][k] = channel.phase.get(k).copied().unwrap_or(0.0);
                self.current_phase_advance[out_ch][k] = channel
                    .phase_advance
                    .get(k)
                    .copied()
                    .unwrap_or_else(|| phase_advance_for_bin(k));
            }
        }
    }

    fn render_frame(
        &mut self,
        item: &CapturedFreeze,
        sample_rate: f32,
        params: InstrumentProcessParams,
        window: &[f32; FFT_SIZE],
        window_gain: f32,
        inverse_fft: &Arc<dyn Fft<f32>>,
    ) {
        if item.channels.is_empty() {
            return;
        }

        let mag_coeff = glide_coeff(params.glide_s, sample_rate);
        let phase_coeff = glide_coeff(params.glide_s * PHASE_GLIDE_RATIO, sample_rate);
        let organic_amt = params.organic;

        for out_ch in 0..self.output_fifo.len() {
            self.spectrum.fill(Complex32::new(0.0, 0.0));
            let src_ch = out_ch.min(item.channels.len() - 1);
            let channel = &item.channels[src_ch];
            let max_mag = channel.mag.iter().copied().fold(0.0_f32, f32::max);
            let threshold = max_mag * item.filter * item.filter;

            if organic_amt > 0.0 {
                let organic_am = &mut self.organic_am[out_ch];
                organic_am.hop_counter += 1;
                if organic_am.hop_counter >= 8 {
                    organic_am.hop_counter = 0;
                    for target in &mut organic_am.target {
                        *target = self.rng.bipolar();
                    }
                }
                for b in 0..ORGANIC_AM_BANDS {
                    organic_am.value[b] += 0.08 * (organic_am.target[b] - organic_am.value[b]);
                }
            }

            for k in 0..NUM_BINS {
                let raw_mag = channel.mag.get(k).copied().unwrap_or(0.0);
                let target_mag = if raw_mag >= threshold { raw_mag } else { 0.0 };
                let target_phase_advance = channel
                    .phase_advance
                    .get(k)
                    .copied()
                    .unwrap_or_else(|| phase_advance_for_bin(k));

                self.current_mag[out_ch][k] +=
                    mag_coeff * (target_mag - self.current_mag[out_ch][k]);
                self.current_phase_advance[out_ch][k] +=
                    phase_coeff * (target_phase_advance - self.current_phase_advance[out_ch][k]);

                let phase_advance = self.current_phase_advance[out_ch][k]
                    * (1.0 + self.rng.bipolar() * organic_amt * 0.035);
                let phase = wrap_phase(
                    self.current_phase[out_ch][k]
                        + phase_advance
                        + self.rng.bipolar() * (FREEZE_PHASE_JITTER_RADIANS + organic_amt * 0.18),
                );
                self.current_phase[out_ch][k] = phase;

                let band_pos = k as f32 * ORGANIC_AM_BANDS as f32 / NUM_BINS as f32;
                let band0 = clamp(band_pos, 0.0, (ORGANIC_AM_BANDS - 1) as f32) as usize;
                let band1 = (band0 + 1).min(ORGANIC_AM_BANDS - 1);
                let frac = band_pos - band0 as f32;
                let band_am = self.organic_am[out_ch].value[band0] * (1.0 - frac)
                    + self.organic_am[out_ch].value[band1] * frac;
                let mag = (self.current_mag[out_ch][k]
                    * (1.0 + band_am * organic_amt * 0.28)
                    * (1.0 + self.rng.bipolar() * organic_amt * 0.06))
                    .max(0.0);
                self.spectrum[k] = Complex32::from_polar(mag, phase);
            }

            apply_organic_spectral_processing(
                self.spectrum.as_mut(),
                &mut self.rng,
                &mut self.organic_scratch[out_ch],
                organic_amt,
                item.filter,
            );
            rebuild_conjugate_mirror(self.spectrum.as_mut());
            inverse_fft.process(self.spectrum.as_mut_slice());
            normalize_inverse_fft(self.spectrum.as_mut());
            apply_organic_saturation(self.spectrum.as_mut(), organic_amt);
            apply_synthesis_window(self.spectrum.as_mut(), window, window_gain);
            let fifo = &mut self.output_fifo[out_ch];
            for i in 0..FFT_SIZE {
                fifo[(self.fifo_pos + i) % FFT_SIZE] += self.spectrum[i].re;
            }
        }
    }

    fn next_amp(&mut self, sample_rate: f32, params: InstrumentProcessParams) -> f32 {
        let target = if self.gate {
            self.active_target
                .as_ref()
                .map(|target| target.velocity)
                .unwrap_or(0.0)
        } else {
            0.0
        };
        let time_s = if target > self.amp {
            params.attack_s
        } else {
            params.release_s
        };
        let coeff = one_pole_coeff(time_s, sample_rate);
        self.amp += coeff * (target - self.amp);
        if target <= 0.0 && self.amp.abs() < SILENCE_AMP {
            self.amp = 0.0;
        }
        self.amp
    }
}

pub struct FreezeInstrument {
    sample_rate: f32,
    output_channels: usize,
    window: Box<[f32; FFT_SIZE]>,
    window_gain: f32,
    engine: MonoSpectralEngine,
    inverse_fft: Arc<dyn Fft<f32>>,
}

impl Default for FreezeInstrument {
    fn default() -> Self {
        let mut this = Self {
            sample_rate: 44_100.0,
            output_channels: 2,
            window: Box::new([0.0; FFT_SIZE]),
            window_gain: 1.0,
            engine: MonoSpectralEngine::new(2),
            inverse_fft: FftPlanner::<f32>::new().plan_fft_inverse(FFT_SIZE),
        };
        this.prepare(44_100.0, 2);
        this
    }
}

impl FreezeInstrument {
    pub fn prepare(&mut self, sample_rate: f32, output_channels: usize) {
        self.sample_rate = if sample_rate > 0.0 {
            sample_rate
        } else {
            44_100.0
        };
        self.output_channels = output_channels.max(1);
        fill_hann_window(self.window.as_mut());
        self.window_gain = calculate_window_gain(self.window.as_ref());
        self.engine.prepare_channels(self.output_channels);
        self.engine.reset();
    }

    pub fn reset(&mut self) {
        self.engine.reset();
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
        self.engine
            .note_on(pad, item_index, item, note, channel, velocity);
    }

    pub fn note_off(&mut self, note: u8, channel: u8, _params: InstrumentProcessParams) {
        if note_to_pad(note).is_none() {
            return;
        }
        self.engine.note_off(note, channel);
    }

    pub fn set_sustain(&mut self, down: bool, _params: InstrumentProcessParams) {
        self.engine.set_sustain(down);
    }

    pub fn active_pads(&self) -> [bool; PAD_COUNT] {
        self.engine.active_pads()
    }

    pub fn process_block(
        &mut self,
        main: &mut [&mut [f32]],
        params: InstrumentProcessParams,
        pool: &[CapturedFreeze],
    ) {
        self.process_block_inner(main, params, pool, true);
    }

    pub fn process_block_additive(
        &mut self,
        main: &mut [&mut [f32]],
        params: InstrumentProcessParams,
        pool: &[CapturedFreeze],
    ) {
        self.process_block_inner(main, params, pool, false);
    }

    fn process_block_inner(
        &mut self,
        main: &mut [&mut [f32]],
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
            if self.engine.hop_counter >= HOP_SIZE {
                if let Some(item_index) = self
                    .engine
                    .active_target
                    .as_ref()
                    .map(|target| target.item_index)
                {
                    if let Some(item) = pool.get(item_index) {
                        self.engine.render_frame(
                            item,
                            self.sample_rate,
                            params,
                            self.window.as_ref(),
                            self.window_gain,
                            &self.inverse_fft,
                        );
                    } else {
                        self.engine
                            .held_notes
                            .retain(|held| held.item_index != item_index);
                        self.engine.recompute_active_target();
                    }
                }
                self.engine.hop_counter = 0;
            }

            let amp = self.engine.next_amp(self.sample_rate, params);
            for ch in 0..channels {
                main[ch][n] += self.engine.output_fifo[ch][self.engine.fifo_pos] * amp;
                self.engine.output_fifo[ch][self.engine.fifo_pos] = 0.0;
            }
            self.engine.fifo_pos = (self.engine.fifo_pos + 1) % FFT_SIZE;
            self.engine.hop_counter += 1;
        }
    }
}
