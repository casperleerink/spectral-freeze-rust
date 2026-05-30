use crate::params::SpectralFreezeParams;
use crate::state::{EditorRuntime, PadActivityAtomics};
use crate::ui::draw_editor;
use dsp::{FreezeInstrument, InstrumentProcessParams, PAD_COUNT, pad_note};
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
    last_audition_revision: u64,
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
