use dsp::{
    CapturedFreeze, FreezeInstrument, INSTRUMENT_PARAMS, InstrumentProcessParams, PAD_COUNT,
    PARAM_ATTACK, PARAM_DECAY, PARAM_INSTRUMENT_ORGANIC, PARAM_INSTRUMENT_SC_BOOST,
    PARAM_INSTRUMENT_SC_FREQ_SMOOTHING, PARAM_RELEASE, PARAM_SUSTAIN, capture_freeze_from_audio,
    note_label, pad_note,
};
use nih_plug::prelude::*;
use nih_plug_egui::{
    EguiState, create_egui_editor,
    egui::{self, CentralPanel, Color32, Pos2, RichText, Stroke, Vec2},
};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const SIDECHAIN_STEREO: &[NonZeroU32] = &[new_nonzero_u32(2)];
const SIDECHAIN_MONO: &[NonZeroU32] = &[new_nonzero_u32(1)];
const AUX_INPUT_NAMES: &[&str] = &["Sidechain"];

#[derive(Clone, Debug, Serialize, Deserialize)]
enum Selection {
    Waveform,
    Pool(usize),
    Pad(usize),
}

impl Default for Selection {
    fn default() -> Self {
        Self::Waveform
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct InstrumentState {
    pool: Vec<CapturedFreeze>,
    pad_assignments: [Option<usize>; PAD_COUNT],
    source_path: Option<String>,
    source_cursor_sample: usize,
    source_sample_rate: f32,
    contextual_filter: f32,
    selection: Selection,
}

impl Default for InstrumentState {
    fn default() -> Self {
        Self {
            pool: Vec::new(),
            pad_assignments: [None; PAD_COUNT],
            source_path: None,
            source_cursor_sample: 0,
            source_sample_rate: 44_100.0,
            contextual_filter: 0.0,
            selection: Selection::Waveform,
        }
    }
}

#[derive(Clone)]
struct LoadedSource {
    path: PathBuf,
    sample_rate: f32,
    channels: Vec<Vec<f32>>,
}

impl LoadedSource {
    fn len_samples(&self) -> usize {
        self.channels.iter().map(Vec::len).max().unwrap_or(0)
    }

    fn duration_seconds(&self) -> f32 {
        self.len_samples() as f32 / self.sample_rate.max(1.0)
    }
}

#[derive(Default)]
struct EditorRuntime {
    source: Option<LoadedSource>,
    file_error: Option<String>,
    audition_enabled: bool,
    audition_item: Option<CapturedFreeze>,
    audition_revision: u64,
    mouse_pad_gates: [bool; PAD_COUNT],
    drag_pool_item: Option<usize>,
}

struct PadActivityAtomics {
    pads: [AtomicBool; PAD_COUNT],
}

impl Default for PadActivityAtomics {
    fn default() -> Self {
        Self {
            pads: std::array::from_fn(|_| AtomicBool::new(false)),
        }
    }
}

impl PadActivityAtomics {
    fn store(&self, active: [bool; PAD_COUNT]) {
        for (atom, value) in self.pads.iter().zip(active) {
            atom.store(value, Ordering::Relaxed);
        }
    }

    fn load(&self) -> [bool; PAD_COUNT] {
        std::array::from_fn(|i| self.pads[i].load(Ordering::Relaxed))
    }
}

pub struct SpectralFreezePlugin {
    params: Arc<SpectralFreezeParams>,
    instrument: FreezeInstrument,
    audition: FreezeInstrument,
    runtime: Arc<Mutex<EditorRuntime>>,
    activity: Arc<PadActivityAtomics>,
    previous_mouse_gates: [bool; PAD_COUNT],
    last_audition_revision: u64,
}

#[derive(Params)]
pub struct SpectralFreezeParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[persist = "instrument-state"]
    instrument_state: Arc<Mutex<InstrumentState>>,

    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "sustain"]
    pub sustain: FloatParam,
    #[id = "release"]
    pub release: FloatParam,
    #[id = "organic"]
    pub organic: FloatParam,
    #[id = "scBoost"]
    pub sc_boost: FloatParam,
    #[id = "scFreqSmoothing"]
    pub sc_freq_smoothing: FloatParam,
}

impl Default for SpectralFreezePlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(SpectralFreezeParams::default()),
            instrument: FreezeInstrument::default(),
            audition: FreezeInstrument::default(),
            runtime: Arc::new(Mutex::new(EditorRuntime::default())),
            activity: Arc::new(PadActivityAtomics::default()),
            previous_mouse_gates: [false; PAD_COUNT],
            last_audition_revision: 0,
        }
    }
}

impl Default for SpectralFreezeParams {
    fn default() -> Self {
        let seconds = Arc::new(|value: f32| format!("{value:.3} s"));
        let seconds_from_string = Arc::new(|text: &str| parse_unit_float(text, "s"));
        let pct = Arc::new(|value: f32| format!("{}%", (value * 100.0).round() as i32));
        let pct_from_string = Arc::new(|text: &str| {
            let value = parse_unit_float(text, "%")?;
            if text.trim().contains('%') {
                Some(value / 100.0)
            } else {
                Some(value)
            }
        });
        let db = Arc::new(|value: f32| format!("+{value:.1} dB"));
        let db_from_string = Arc::new(|text: &str| parse_unit_float(text, "dB"));

        Self {
            editor_state: EguiState::from_size(980, 680),
            instrument_state: Arc::new(Mutex::new(InstrumentState::default())),
            attack: FloatParam::new(
                INSTRUMENT_PARAMS[PARAM_ATTACK].name,
                INSTRUMENT_PARAMS[PARAM_ATTACK].default,
                FloatRange::Linear { min: 0.0, max: 5.0 },
            )
            .with_step_size(0.001)
            .with_value_to_string(seconds.clone())
            .with_string_to_value(seconds_from_string.clone()),
            decay: FloatParam::new(
                INSTRUMENT_PARAMS[PARAM_DECAY].name,
                INSTRUMENT_PARAMS[PARAM_DECAY].default,
                FloatRange::Linear { min: 0.0, max: 5.0 },
            )
            .with_step_size(0.001)
            .with_value_to_string(seconds.clone())
            .with_string_to_value(seconds_from_string.clone()),
            sustain: FloatParam::new(
                INSTRUMENT_PARAMS[PARAM_SUSTAIN].name,
                INSTRUMENT_PARAMS[PARAM_SUSTAIN].default,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.001)
            .with_value_to_string(pct.clone())
            .with_string_to_value(pct_from_string.clone()),
            release: FloatParam::new(
                INSTRUMENT_PARAMS[PARAM_RELEASE].name,
                INSTRUMENT_PARAMS[PARAM_RELEASE].default,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            )
            .with_step_size(0.001)
            .with_value_to_string(seconds)
            .with_string_to_value(seconds_from_string),
            organic: FloatParam::new(
                INSTRUMENT_PARAMS[PARAM_INSTRUMENT_ORGANIC].name,
                INSTRUMENT_PARAMS[PARAM_INSTRUMENT_ORGANIC].default,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.001)
            .with_value_to_string(pct.clone())
            .with_string_to_value(pct_from_string.clone()),
            sc_boost: FloatParam::new(
                INSTRUMENT_PARAMS[PARAM_INSTRUMENT_SC_BOOST].name,
                INSTRUMENT_PARAMS[PARAM_INSTRUMENT_SC_BOOST].default,
                FloatRange::Linear {
                    min: 0.0,
                    max: 18.0,
                },
            )
            .with_step_size(0.01)
            .with_value_to_string(db)
            .with_string_to_value(db_from_string),
            sc_freq_smoothing: FloatParam::new(
                INSTRUMENT_PARAMS[PARAM_INSTRUMENT_SC_FREQ_SMOOTHING].name,
                INSTRUMENT_PARAMS[PARAM_INSTRUMENT_SC_FREQ_SMOOTHING].default,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.001)
            .with_value_to_string(pct)
            .with_string_to_value(pct_from_string),
        }
    }
}

fn parse_unit_float(text: &str, unit: &str) -> Option<f32> {
    text.trim()
        .trim_start_matches('+')
        .trim_end_matches(unit)
        .trim()
        .parse::<f32>()
        .ok()
}

impl SpectralFreezePlugin {
    fn current_params(&self) -> InstrumentProcessParams {
        InstrumentProcessParams {
            attack_s: self.params.attack.value(),
            decay_s: self.params.decay.value(),
            sustain: self.params.sustain.value(),
            release_s: self.params.release.value(),
            organic: self.params.organic.value(),
            sc_boost_db: self.params.sc_boost.value(),
            sc_freq_smoothing: self.params.sc_freq_smoothing.value(),
        }
    }
}

impl Plugin for SpectralFreezePlugin {
    const NAME: &'static str = "Spectral Freeze Instrument";
    const VENDOR: &'static str = "Learning";
    const URL: &'static str = "https://example.invalid/spectral-freeze";
    const EMAIL: &'static str = "support@example.invalid";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: SIDECHAIN_STEREO,
            names: PortNames {
                main_output: Some("Output"),
                aux_inputs: AUX_INPUT_NAMES,
                ..PortNames::const_default()
            },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: SIDECHAIN_MONO,
            names: PortNames {
                main_output: Some("Output"),
                aux_inputs: AUX_INPUT_NAMES,
                ..PortNames::const_default()
            },
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let runtime = self.runtime.clone();
        let activity = self.activity.clone();
        let egui_state = params.editor_state.clone();

        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, _| {},
            move |egui_ctx, setter, _state| {
                let _ = egui_state.as_ref();
                CentralPanel::default().show(egui_ctx, |ui| {
                    draw_editor(ui, setter, &params, &runtime, &activity);
                });
            },
        )
    }

    fn initialize(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        let output_channels = audio_io_layout
            .main_output_channels
            .map(|n| n.get() as usize)
            .unwrap_or(2);
        let sidechain_channels = audio_io_layout
            .aux_input_ports
            .first()
            .map(|n| n.get() as usize)
            .unwrap_or(0);
        self.instrument.prepare(
            buffer_config.sample_rate,
            output_channels,
            sidechain_channels,
        );
        self.audition
            .prepare(buffer_config.sample_rate, output_channels, 0);
        context.set_latency_samples(dsp::LATENCY_SAMPLES);
        true
    }

    fn reset(&mut self) {
        self.instrument.reset();
        self.audition.reset();
        self.previous_mouse_gates = [false; PAD_COUNT];
        self.last_audition_revision = 0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let params = self.current_params();

        let (mouse_gates, audition_enabled, audition_item, audition_revision) = {
            let runtime = self.runtime.lock().unwrap();
            (
                runtime.mouse_pad_gates,
                runtime.audition_enabled,
                runtime.audition_item.clone(),
                runtime.audition_revision,
            )
        };

        let state = self.params.instrument_state.lock().unwrap();

        for pad in 0..PAD_COUNT {
            if mouse_gates[pad] && !self.previous_mouse_gates[pad] {
                self.instrument
                    .note_on(pad_note(pad), 0, 1.0, &state.pool, &state.pad_assignments);
            } else if !mouse_gates[pad] && self.previous_mouse_gates[pad] {
                self.instrument.note_off(pad_note(pad), 0, params);
            }
        }
        self.previous_mouse_gates = mouse_gates;

        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn {
                    note,
                    channel,
                    velocity,
                    ..
                } if velocity > 0.0 => {
                    self.instrument.note_on(
                        note,
                        channel,
                        velocity,
                        &state.pool,
                        &state.pad_assignments,
                    );
                }
                NoteEvent::NoteOn { note, channel, .. }
                | NoteEvent::NoteOff { note, channel, .. } => {
                    self.instrument.note_off(note, channel, params);
                }
                NoteEvent::Choke { note, channel, .. } => {
                    self.instrument.note_off(
                        note,
                        channel,
                        InstrumentProcessParams {
                            release_s: 0.0,
                            ..params
                        },
                    );
                }
                NoteEvent::MidiCC { cc: 64, value, .. } => {
                    self.instrument.set_sustain(value >= 0.5, params);
                }
                _ => {}
            }
        }

        let sidechain = aux.inputs.first_mut().map(|buffer| buffer.as_slice());
        let mut main = buffer.as_slice();
        self.instrument
            .process_block(&mut main, sidechain.as_deref(), params, &state.pool);

        if audition_enabled && self.params.editor_state.is_open() {
            if let Some(item) = audition_item {
                let pool = [item];
                let assignments = [
                    Some(0),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                ];
                if audition_revision != self.last_audition_revision {
                    self.audition.reset();
                    self.audition
                        .note_on(pad_note(0), 0, 1.0, &pool, &assignments);
                    self.last_audition_revision = audition_revision;
                }
                self.audition
                    .process_block_additive(&mut main, params, &pool);
            }
        } else {
            self.audition.reset();
            self.last_audition_revision = audition_revision;
        }

        self.activity.store(self.instrument.active_pads());
        ProcessStatus::Normal
    }
}

impl ClapPlugin for SpectralFreezePlugin {
    const CLAP_ID: &'static str = "com.learning.spectral-freeze-instrument";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Spectral freeze pad-bank MIDI instrument");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

impl Vst3Plugin for SpectralFreezePlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"SpectralFrzInst1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
        Vst3SubCategory::Stereo,
    ];
}

fn draw_editor(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &SpectralFreezeParams,
    runtime: &Arc<Mutex<EditorRuntime>>,
    activity: &PadActivityAtomics,
) {
    let bg = Color32::from_rgb(15, 15, 20);
    let panel = Color32::from_rgb(25, 25, 34);
    let panel2 = Color32::from_rgb(35, 35, 46);
    let border = Color32::from_rgb(56, 56, 72);
    let accent = Color32::from_rgb(124, 196, 255);
    let fg = Color32::from_rgb(232, 232, 240);
    let fg_dim = Color32::from_rgb(146, 146, 164);

    ui.painter().rect_filled(ui.max_rect(), 0.0, bg);
    ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);

    let mut runtime = runtime.lock().unwrap();
    let mut state = params.instrument_state.lock().unwrap();
    sync_runtime_source_metadata(&mut runtime, &mut state);
    refresh_audition(&mut runtime, &state);

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("SPECTRAL FREEZE INSTRUMENT")
                    .size(19.0)
                    .strong()
                    .color(fg),
            );
            ui.label(
                RichText::new("WAV → FREEZE POOL → 16 MIDI PADS")
                    .size(11.0)
                    .color(fg_dim),
            );
        });

        draw_source_panel(
            ui,
            &mut runtime,
            &mut state,
            panel,
            panel2,
            border,
            accent,
            fg,
            fg_dim,
        );

        ui.horizontal(|ui| {
            draw_pool_panel(
                ui,
                &mut runtime,
                &mut state,
                panel,
                panel2,
                border,
                accent,
                fg,
                fg_dim,
            );
            draw_pad_grid(
                ui,
                &mut runtime,
                &mut state,
                activity.load(),
                panel,
                panel2,
                border,
                accent,
                fg,
                fg_dim,
            );
        });

        draw_bottom_panel(
            ui, setter, params, &mut state, panel, panel2, border, accent, fg, fg_dim,
        );
    });

    refresh_audition(&mut runtime, &state);
}

fn sync_runtime_source_metadata(runtime: &mut EditorRuntime, state: &mut InstrumentState) {
    if let Some(source) = &runtime.source {
        state.source_path = Some(source.path.to_string_lossy().to_string());
        state.source_sample_rate = source.sample_rate;
        let len = source.len_samples();
        if len > 0 {
            state.source_cursor_sample = state.source_cursor_sample.min(len - 1);
        }
    }
}

fn refresh_audition(runtime: &mut EditorRuntime, state: &InstrumentState) {
    if !runtime.audition_enabled {
        if runtime.audition_item.is_some() {
            runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
        }
        runtime.audition_item = None;
        return;
    }
    let Some(source) = &runtime.source else {
        if runtime.audition_item.is_some() {
            runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
        }
        runtime.audition_item = None;
        return;
    };
    let next = capture_freeze_from_audio(
        &source.channels,
        source.sample_rate,
        state.source_cursor_sample,
        Some(&source.path.to_string_lossy()),
        state.contextual_filter,
    );
    let changed = match (&runtime.audition_item, &next) {
        (Some(a), Some(b)) => {
            a.cursor_sample != b.cursor_sample
                || (a.filter - b.filter).abs() > 1.0e-6
                || a.source_path != b.source_path
        }
        (None, None) => false,
        _ => true,
    };
    if changed {
        runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
    }
    runtime.audition_item = next;
}

fn draw_source_panel(
    ui: &mut egui::Ui,
    runtime: &mut EditorRuntime,
    state: &mut InstrumentState,
    panel: Color32,
    panel2: Color32,
    border: Color32,
    accent: Color32,
    fg: Color32,
    fg_dim: Color32,
) {
    egui::Frame::NONE
        .fill(panel)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(12.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Source Files / Load WAV").strong().color(fg));
                if ui.add(egui::Button::new("Load WAV").fill(panel2)).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("WAV audio", &["wav"])
                        .pick_file()
                    {
                        match load_wav(&path) {
                            Ok(source) => {
                                state.source_path = Some(path.to_string_lossy().to_string());
                                state.source_sample_rate = source.sample_rate;
                                state.source_cursor_sample = 0;
                                state.selection = Selection::Waveform;
                                runtime.source = Some(source);
                                runtime.file_error = None;
                            }
                            Err(err) => runtime.file_error = Some(err),
                        }
                    }
                }
                let mut audition = runtime.audition_enabled;
                if ui.checkbox(&mut audition, "Audition Monitor").changed() {
                    runtime.audition_enabled = audition;
                    runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
                }
                if ui
                    .add_enabled(
                        runtime.source.is_some(),
                        egui::Button::new("Capture").fill(accent.gamma_multiply(0.35)),
                    )
                    .clicked()
                {
                    if let Some(source) = &runtime.source {
                        if let Some(item) = capture_freeze_from_audio(
                            &source.channels,
                            source.sample_rate,
                            state.source_cursor_sample,
                            Some(&source.path.to_string_lossy()),
                            state.contextual_filter,
                        ) {
                            state.pool.push(item);
                            state.selection = Selection::Pool(state.pool.len() - 1);
                        }
                    }
                }
            });

            let source_label = if let Some(source) = &runtime.source {
                format!(
                    "{} · {:.2}s · {} ch · {} Hz",
                    source
                        .path
                        .file_name()
                        .and_then(|p| p.to_str())
                        .unwrap_or("audio.wav"),
                    source.duration_seconds(),
                    source.channels.len(),
                    source.sample_rate.round() as u32
                )
            } else if let Some(path) = &state.source_path {
                format!("Missing source file: {path} (captured pool remains playable)")
            } else {
                "No source loaded. Projects start with an empty bank.".to_string()
            };
            ui.label(RichText::new(source_label).size(11.0).color(fg_dim));
            if let Some(err) = &runtime.file_error {
                ui.label(
                    RichText::new(err)
                        .size(11.0)
                        .color(Color32::from_rgb(255, 130, 130)),
                );
            }
            draw_waveform(ui, runtime, state, panel2, border, accent, fg_dim);
        });
}

fn draw_waveform(
    ui: &mut egui::Ui,
    runtime: &mut EditorRuntime,
    state: &mut InstrumentState,
    panel2: Color32,
    border: Color32,
    accent: Color32,
    fg_dim: Color32,
) {
    let desired = Vec2::new(ui.available_width(), 118.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click_and_drag());
    let painter = ui.painter();
    painter.rect_filled(rect, 8.0, panel2.gamma_multiply(0.75));
    painter.rect_stroke(
        rect,
        8.0,
        Stroke::new(1.0, border),
        egui::StrokeKind::Outside,
    );

    if let Some(source) = &runtime.source {
        let len = source.len_samples().max(1);
        if (response.clicked() || response.dragged()) && response.interact_pointer_pos().is_some() {
            let pos = response.interact_pointer_pos().unwrap();
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            state.source_cursor_sample = ((len - 1) as f32 * t).round() as usize;
            state.selection = Selection::Waveform;
            runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
        }

        let cols = rect.width().max(1.0) as usize;
        for x in 0..cols {
            let start = x * len / cols;
            let end = ((x + 1) * len / cols).max(start + 1).min(len);
            let mut peak = 0.0_f32;
            for ch in &source.channels {
                for sample in &ch[start.min(ch.len())..end.min(ch.len())] {
                    peak = peak.max(sample.abs());
                }
            }
            let x_pos = rect.left() + x as f32;
            let y0 = rect.center().y - peak * rect.height() * 0.45;
            let y1 = rect.center().y + peak * rect.height() * 0.45;
            painter.line_segment(
                [Pos2::new(x_pos, y0), Pos2::new(x_pos, y1)],
                Stroke::new(1.0, accent.gamma_multiply(0.65)),
            );
        }

        let cursor_x =
            rect.left() + rect.width() * (state.source_cursor_sample as f32 / len as f32);
        painter.line_segment(
            [
                Pos2::new(cursor_x, rect.top()),
                Pos2::new(cursor_x, rect.bottom()),
            ],
            Stroke::new(2.0, Color32::WHITE),
        );
        painter.text(
            rect.left_top() + Vec2::new(10.0, 8.0),
            egui::Align2::LEFT_TOP,
            "click/scrub cursor",
            egui::FontId::monospace(10.0),
            fg_dim,
        );
    } else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Load a WAV file to show waveform",
            egui::FontId::proportional(14.0),
            fg_dim,
        );
    }
}

fn draw_pool_panel(
    ui: &mut egui::Ui,
    runtime: &mut EditorRuntime,
    state: &mut InstrumentState,
    panel: Color32,
    panel2: Color32,
    border: Color32,
    accent: Color32,
    fg: Color32,
    fg_dim: Color32,
) {
    egui::Frame::NONE
        .fill(panel)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(12.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_width((ui.available_width() - 12.0) * 0.42);
            ui.set_height(270.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Freeze Pool").strong().color(fg));
                ui.label(
                    RichText::new(format!("{} items", state.pool.len()))
                        .size(10.0)
                        .color(fg_dim),
                );
            });
            egui::ScrollArea::vertical()
                .max_height(226.0)
                .show(ui, |ui| {
                    let mut delete_idx = None;
                    for idx in 0..state.pool.len() {
                        let selected = matches!(state.selection, Selection::Pool(i) if i == idx);
                        ui.horizontal(|ui| {
                            let label = format!(
                                "{}  ·  F{}%",
                                state.pool[idx].name,
                                (state.pool[idx].filter * 100.0).round() as i32
                            );
                            let response = ui.add(
                                egui::Button::new(RichText::new(label).color(if selected {
                                    Color32::BLACK
                                } else {
                                    fg
                                }))
                                .fill(if selected { accent } else { panel2 })
                                .min_size(Vec2::new(ui.available_width() - 26.0, 24.0)),
                            );
                            if response.clicked() {
                                state.selection = Selection::Pool(idx);
                            }
                            if response.drag_started() {
                                runtime.drag_pool_item = Some(idx);
                            }
                            if ui.small_button("×").clicked() {
                                delete_idx = Some(idx);
                            }
                        });
                    }
                    if state.pool.is_empty() {
                        ui.label(
                            RichText::new("Capture freezes here. Drag items onto pads.")
                                .color(fg_dim),
                        );
                    }
                    if let Some(idx) = delete_idx {
                        state.pool.remove(idx);
                        for assignment in &mut state.pad_assignments {
                            match *assignment {
                                Some(i) if i == idx => *assignment = None,
                                Some(i) if i > idx => *assignment = Some(i - 1),
                                _ => {}
                            }
                        }
                        state.selection = Selection::Waveform;
                    }
                });
        });
}

fn draw_pad_grid(
    ui: &mut egui::Ui,
    runtime: &mut EditorRuntime,
    state: &mut InstrumentState,
    active: [bool; PAD_COUNT],
    panel: Color32,
    panel2: Color32,
    border: Color32,
    accent: Color32,
    fg: Color32,
    _fg_dim: Color32,
) {
    egui::Frame::NONE
        .fill(panel)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(12.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_height(270.0);
            ui.label(RichText::new("4×4 Pad Grid").strong().color(fg));
            let pad_size = Vec2::new(112.0, 50.0);
            for row in 0..4 {
                ui.horizontal(|ui| {
                    for col in 0..4 {
                        let pad = row * 4 + col;
                        let assigned = state.pad_assignments[pad].and_then(|i| state.pool.get(i));
                        let selected = matches!(state.selection, Selection::Pad(p) if p == pad);
                        let fill = if active[pad] || runtime.mouse_pad_gates[pad] {
                            accent
                        } else if selected {
                            Color32::from_rgb(82, 105, 128)
                        } else if assigned.is_some() {
                            panel2
                        } else {
                            panel2.gamma_multiply(0.55)
                        };
                        let short = assigned
                            .map(|i| short_name(&i.name))
                            .unwrap_or_else(|| "Empty".to_string());
                        let text =
                            format!("Pad {}  {}\n{}", pad + 1, note_label(pad_note(pad)), short);
                        let response = ui.add(
                            egui::Button::new(
                                RichText::new(text).size(11.0).color(if fill == accent {
                                    Color32::BLACK
                                } else {
                                    fg
                                }),
                            )
                            .fill(fill)
                            .stroke(Stroke::new(1.0, border))
                            .min_size(pad_size),
                        );
                        runtime.mouse_pad_gates[pad] =
                            response.is_pointer_button_down_on() && assigned.is_some();
                        if response.clicked() {
                            state.selection = Selection::Pad(pad);
                        }
                        if response.hovered() && ui.input(|i| i.pointer.any_released()) {
                            if let Some(idx) = runtime.drag_pool_item.take() {
                                if idx < state.pool.len() {
                                    state.pad_assignments[pad] = Some(idx);
                                    state.selection = Selection::Pad(pad);
                                }
                            }
                        }
                        response.context_menu(|ui| {
                            if ui.button("Clear pad").clicked() {
                                state.pad_assignments[pad] = None;
                                ui.close_menu();
                            }
                        });
                    }
                });
            }
            if ui.input(|i| i.pointer.any_released()) {
                runtime.drag_pool_item = None;
            }
        });
}

fn draw_bottom_panel(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &SpectralFreezeParams,
    state: &mut InstrumentState,
    panel: Color32,
    panel2: Color32,
    border: Color32,
    _accent: Color32,
    fg: Color32,
    fg_dim: Color32,
) {
    egui::Frame::NONE
        .fill(panel)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(12.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.label(
                RichText::new("Contextual Filter + ADSR + Organic + SC")
                    .strong()
                    .color(fg),
            );
            ui.horizontal_wrapped(|ui| {
                let mut filter = current_context_filter(state);
                if ui
                    .add(egui::Slider::new(&mut filter, 0.0..=1.0).text("Filter"))
                    .changed()
                {
                    set_context_filter(state, filter);
                }
                draw_param_slider(ui, setter, &params.attack, "Attack");
                draw_param_slider(ui, setter, &params.decay, "Decay");
                draw_param_slider(ui, setter, &params.sustain, "Sustain");
                draw_param_slider(ui, setter, &params.release, "Release");
                draw_param_slider(ui, setter, &params.organic, "Organic");
                draw_param_slider(ui, setter, &params.sc_boost, "SC Boost");
                draw_param_slider(ui, setter, &params.sc_freq_smoothing, "SC Smooth");
            });
            ui.label(
                RichText::new(selection_help(state))
                    .size(10.0)
                    .color(fg_dim),
            );
            ui.painter().rect_stroke(
                ui.min_rect(),
                12.0,
                Stroke::new(0.0, panel2),
                egui::StrokeKind::Outside,
            );
        });
}

fn draw_param_slider(ui: &mut egui::Ui, setter: &ParamSetter, param: &FloatParam, label: &str) {
    let mut normalized = param.modulated_normalized_value();
    let response = ui.add(
        egui::Slider::new(&mut normalized, 0.0..=1.0)
            .text(label)
            .show_value(false),
    );
    if response.drag_started() {
        setter.begin_set_parameter(param);
    }
    if response.changed() {
        setter.set_parameter_normalized(param, normalized);
    }
    if response.drag_stopped() {
        setter.end_set_parameter(param);
    }
    response.on_hover_text(param.to_string());
}

fn current_context_filter(state: &InstrumentState) -> f32 {
    match state.selection {
        Selection::Waveform => state.contextual_filter,
        Selection::Pool(idx) => state
            .pool
            .get(idx)
            .map(|item| item.filter)
            .unwrap_or(state.contextual_filter),
        Selection::Pad(pad) => state.pad_assignments[pad]
            .and_then(|idx| state.pool.get(idx))
            .map(|item| item.filter)
            .unwrap_or(state.contextual_filter),
    }
}

fn set_context_filter(state: &mut InstrumentState, value: f32) {
    let value = value.clamp(0.0, 1.0);
    match state.selection {
        Selection::Waveform => state.contextual_filter = value,
        Selection::Pool(idx) => {
            if let Some(item) = state.pool.get_mut(idx) {
                item.filter = value;
            }
        }
        Selection::Pad(pad) => {
            if let Some(idx) = state.pad_assignments[pad] {
                if let Some(item) = state.pool.get_mut(idx) {
                    item.filter = value;
                }
            }
        }
    }
}

fn selection_help(state: &InstrumentState) -> String {
    match state.selection {
        Selection::Waveform => {
            "Filter edits current waveform audition and next capture".to_string()
        }
        Selection::Pool(idx) => format!(
            "Filter edits pool item {} and all pads assigned to it",
            idx + 1
        ),
        Selection::Pad(pad) => format!(
            "Pad {} selected; Filter edits its underlying pool item",
            pad + 1
        ),
    }
}

fn short_name(name: &str) -> String {
    const MAX: usize = 22;
    if name.chars().count() <= MAX {
        name.to_string()
    } else {
        let mut s = name.chars().take(MAX - 1).collect::<String>();
        s.push('…');
        s
    }
}

fn load_wav(path: &Path) -> Result<LoadedSource, String> {
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

nih_export_clap!(SpectralFreezePlugin);
nih_export_vst3!(SpectralFreezePlugin);
