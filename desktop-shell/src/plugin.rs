use crate::params::SpectralFreezeParams;
use crate::state::{EditorRuntime, PadActivityAtomics};
use crate::ui::draw_editor;
use dsp::{CapturedFreeze, FreezeInstrument, InstrumentProcessParams, PAD_COUNT, pad_note};
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui::CentralPanel};
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

const SIDECHAIN_STEREO: &[NonZeroU32] = &[new_nonzero_u32(2)];
const SIDECHAIN_MONO: &[NonZeroU32] = &[new_nonzero_u32(1)];
const AUX_INPUT_NAMES: &[&str] = &["Sidechain"];

pub struct SpectralFreezePlugin {
    params: Arc<SpectralFreezeParams>,
    instrument: FreezeInstrument,
    audition: FreezeInstrument,
    runtime: Arc<Mutex<EditorRuntime>>,
    activity: Arc<PadActivityAtomics>,
    previous_mouse_gates: [bool; PAD_COUNT],
    cached_mouse_gates: [bool; PAD_COUNT],
    cached_audition_enabled: bool,
    cached_audition_item: Option<Arc<CapturedFreeze>>,
    cached_audition_revision: u64,
    cached_editor_frame_generation: u64,
    cached_pool: Vec<CapturedFreeze>,
    cached_pad_assignments: [Option<usize>; PAD_COUNT],
    cached_audio_state_revision: u64,
    last_audition_revision: u64,
    last_editor_frame_generation: u64,
    blocks_since_editor_frame: u32,
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
            cached_mouse_gates: [false; PAD_COUNT],
            cached_audition_enabled: false,
            cached_audition_item: None,
            cached_audition_revision: 0,
            cached_editor_frame_generation: 0,
            cached_pool: Vec::new(),
            cached_pad_assignments: [None; PAD_COUNT],
            cached_audio_state_revision: u64::MAX,
            last_audition_revision: 0,
            last_editor_frame_generation: 0,
            blocks_since_editor_frame: u32::MAX,
        }
    }
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
    const NAME: &'static str = "Spectral Freeze";
    const VENDOR: &'static str = "Casper Leerink";
    const URL: &'static str = "https://github.com/casperleerink/spectral-freeze-rust";
    const EMAIL: &'static str = "casper@cleerink.com";
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
        self.cached_mouse_gates = [false; PAD_COUNT];
        self.cached_audition_enabled = false;
        self.cached_audition_item = None;
        self.cached_audition_revision = 0;
        self.cached_editor_frame_generation = 0;
        self.cached_pool.clear();
        self.cached_pad_assignments = [None; PAD_COUNT];
        self.cached_audio_state_revision = u64::MAX;
        self.last_audition_revision = 0;
        self.last_editor_frame_generation = 0;
        self.blocks_since_editor_frame = u32::MAX;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let params = self.current_params();

        if let Ok(runtime) = self.runtime.try_lock() {
            self.cached_mouse_gates = runtime.mouse_pad_gates;
            self.cached_audition_enabled = runtime.audition_enabled;
            self.cached_audition_item = runtime.audition_item.clone();
            self.cached_audition_revision = runtime.audition_revision;
            self.cached_editor_frame_generation = runtime.editor_frame_generation;
        }
        if let Ok(state) = self.params.instrument_state.try_lock() {
            if state.audio_revision != self.cached_audio_state_revision
                || state.pool.len() != self.cached_pool.len()
                || state.pad_assignments != self.cached_pad_assignments
            {
                self.cached_pool = state.pool.clone();
                self.cached_pad_assignments = state.pad_assignments;
                self.cached_audio_state_revision = state.audio_revision;
            }
        }

        let mouse_gates = self.cached_mouse_gates;
        let audition_enabled = self.cached_audition_enabled;
        let audition_item = self.cached_audition_item.clone();
        let audition_revision = self.cached_audition_revision;
        let editor_frame_generation = self.cached_editor_frame_generation;

        if editor_frame_generation != self.last_editor_frame_generation {
            self.last_editor_frame_generation = editor_frame_generation;
            self.blocks_since_editor_frame = 0;
        } else {
            self.blocks_since_editor_frame = self.blocks_since_editor_frame.saturating_add(1);
        }
        let editor_recently_active = self.blocks_since_editor_frame < 100;

        for pad in 0..PAD_COUNT {
            if mouse_gates[pad] && !self.previous_mouse_gates[pad] {
                self.instrument.note_on(
                    pad_note(pad),
                    0,
                    1.0,
                    &self.cached_pool,
                    &self.cached_pad_assignments,
                );
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
                        &self.cached_pool,
                        &self.cached_pad_assignments,
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
            .process_block(&mut main, sidechain.as_deref(), params, &self.cached_pool);

        if audition_enabled && editor_recently_active {
            if let Some(item) = audition_item {
                let pool = std::slice::from_ref(item.as_ref());
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
    const CLAP_ID: &'static str = "com.cleerink.spectral-freeze";
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
    const VST3_CLASS_ID: [u8; 16] = *b"SpectralFreeze01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
        Vst3SubCategory::Stereo,
    ];
}
