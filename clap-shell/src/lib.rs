use dsp::{ProcessParams, SpectralFreeze, LATENCY_SAMPLES, PARAMS, SPECTRUM_DISPLAY_BINS};
use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, CentralPanel, Color32, Pos2, Rect, RichText, Stroke, Vec2},
    EguiState,
};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

const SIDECHAIN_STEREO: &[NonZeroU32] = &[new_nonzero_u32(2)];
const SIDECHAIN_MONO: &[NonZeroU32] = &[new_nonzero_u32(1)];
const AUX_INPUT_NAMES: &[&str] = &["Sidechain"];

pub struct SpectralFreezePlugin {
    params: Arc<SpectralFreezeParams>,
    dsp: SpectralFreeze,
    spectrum: Arc<SpectrumAtomics>,
}

#[derive(Params)]
pub struct SpectralFreezeParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "freeze"]
    pub freeze: BoolParam,
    #[id = "filter"]
    pub filter: FloatParam,
    #[id = "scBoost"]
    pub sc_boost: FloatParam,
    #[id = "scFreqSmoothing"]
    pub sc_freq_smoothing: FloatParam,
    #[id = "organic"]
    pub organic: FloatParam,
}

struct SpectrumAtomics {
    bins: [AtomicU32; SPECTRUM_DISPLAY_BINS],
}

impl Default for SpectrumAtomics {
    fn default() -> Self {
        Self {
            bins: std::array::from_fn(|_| AtomicU32::new(0.0_f32.to_bits())),
        }
    }
}

impl SpectrumAtomics {
    fn store(&self, snapshot: [f32; SPECTRUM_DISPLAY_BINS]) {
        for (atom, value) in self.bins.iter().zip(snapshot) {
            atom.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    fn load(&self) -> [f32; SPECTRUM_DISPLAY_BINS] {
        std::array::from_fn(|i| f32::from_bits(self.bins[i].load(Ordering::Relaxed)))
    }
}

impl Default for SpectralFreezePlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(SpectralFreezeParams::default()),
            dsp: SpectralFreeze::default(),
            spectrum: Arc::new(SpectrumAtomics::default()),
        }
    }
}

impl Default for SpectralFreezeParams {
    fn default() -> Self {
        let pct = Arc::new(|value: f32| format!("{}%", (value * 100.0).round() as i32));
        let db = Arc::new(|value: f32| format!("+{value:.1} dB"));

        Self {
            editor_state: EguiState::from_size(760, 540),
            freeze: BoolParam::new(PARAMS[0].name, PARAMS[0].default >= 0.5),
            filter: FloatParam::new(
                PARAMS[1].name,
                PARAMS[1].default,
                FloatRange::Linear { min: PARAMS[1].min, max: PARAMS[1].max },
            )
            .with_step_size(0.001)
            .with_value_to_string(pct.clone()),
            sc_boost: FloatParam::new(
                PARAMS[2].name,
                PARAMS[2].default,
                FloatRange::Linear { min: PARAMS[2].min, max: PARAMS[2].max },
            )
            .with_step_size(0.01)
            .with_unit(" dB")
            .with_value_to_string(db),
            sc_freq_smoothing: FloatParam::new(
                PARAMS[3].name,
                PARAMS[3].default,
                FloatRange::Linear { min: PARAMS[3].min, max: PARAMS[3].max },
            )
            .with_step_size(0.001)
            .with_value_to_string(pct.clone()),
            organic: FloatParam::new(
                PARAMS[4].name,
                PARAMS[4].default,
                FloatRange::Linear { min: PARAMS[4].min, max: PARAMS[4].max },
            )
            .with_step_size(0.001)
            .with_value_to_string(pct),
        }
    }
}

impl SpectralFreezePlugin {
    fn current_params(&self) -> ProcessParams {
        ProcessParams {
            freeze: self.params.freeze.value(),
            filter: self.params.filter.value(),
            sc_boost_db: self.params.sc_boost.value(),
            sc_freq_smoothing: self.params.sc_freq_smoothing.value(),
            organic: self.params.organic.value(),
        }
    }
}

impl Plugin for SpectralFreezePlugin {
    const NAME: &'static str = "Spectral Freeze Rust";
    const VENDOR: &'static str = "Learning";
    const URL: &'static str = "https://example.invalid/spectral-freeze";
    const EMAIL: &'static str = "support@example.invalid";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: SIDECHAIN_STEREO,
            names: PortNames {
                main_input: Some("Input"),
                main_output: Some("Output"),
                aux_inputs: AUX_INPUT_NAMES,
                ..PortNames::const_default()
            },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: SIDECHAIN_MONO,
            names: PortNames {
                main_input: Some("Input"),
                main_output: Some("Output"),
                aux_inputs: AUX_INPUT_NAMES,
                ..PortNames::const_default()
            },
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let spectrum = self.spectrum.clone();
        let egui_state = params.editor_state.clone();

        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, _| {},
            move |egui_ctx, setter, _state| {
                let _ = egui_state.as_ref();
                CentralPanel::default().show(egui_ctx, |ui| {
                    draw_editor(ui, setter, &params, &spectrum);
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
        let main_channels = audio_io_layout
            .main_output_channels
            .or(audio_io_layout.main_input_channels)
            .map(|n| n.get() as usize)
            .unwrap_or(2);
        let sidechain_channels = audio_io_layout
            .aux_input_ports
            .first()
            .map(|n| n.get() as usize)
            .unwrap_or(0);
        self.dsp.prepare(buffer_config.sample_rate, main_channels, sidechain_channels);
        context.set_latency_samples(LATENCY_SAMPLES);
        true
    }

    fn reset(&mut self) {
        self.dsp.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let params = self.current_params();
        let sidechain = aux.inputs.first_mut().map(|buffer| buffer.as_slice());
        self.dsp.process_block(buffer.as_slice(), sidechain.as_deref(), params);
        self.spectrum.store(self.dsp.processed_spectrum_snapshot());
        ProcessStatus::Normal
    }
}

impl ClapPlugin for SpectralFreezePlugin {
    const CLAP_ID: &'static str = "com.learning.spectral-freeze-rust";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("STFT phase-vocoder spectral freeze with magnitude filtering and sidechain spectral enhancement");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for SpectralFreezePlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"SpectralFrzRust2";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Tools,
        Vst3SubCategory::Stereo,
    ];
}

fn draw_editor(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &SpectralFreezeParams,
    spectrum: &SpectrumAtomics,
) {
    let bg = Color32::from_rgb(16, 16, 22);
    let panel = Color32::from_rgb(26, 26, 34);
    let panel2 = Color32::from_rgb(34, 34, 44);
    let border = Color32::from_rgb(45, 45, 58);
    let accent = Color32::from_rgb(124, 196, 255);
    let accent_dim = Color32::from_rgb(77, 127, 168);
    let fg = Color32::from_rgb(232, 232, 240);
    let fg_dim = Color32::from_rgb(138, 138, 154);

    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, bg);

    ui.vertical_centered(|ui| {
        ui.add_space(22.0);
        ui.label(RichText::new("SPECTRAL FREEZE").size(20.0).strong().color(fg));
        ui.label(RichText::new("STFT · PHASE VOCODER · MAGNITUDE FILTER").size(11.0).color(fg_dim));
        ui.add_space(28.0);

        draw_spectrum(ui, spectrum.load(), panel, border, accent, accent_dim, fg_dim);
        ui.add_space(22.0);
        draw_freeze_button(ui, setter, &params.freeze, accent, panel2, border, fg_dim, bg);
        ui.add_space(22.0);

        draw_knob_row(ui, setter, params, accent, panel2, border, fg, fg_dim);
    });
}

fn draw_spectrum(
    ui: &mut egui::Ui,
    values: [f32; SPECTRUM_DISPLAY_BINS],
    panel: Color32,
    border: Color32,
    accent: Color32,
    accent_dim: Color32,
    fg_dim: Color32,
) {
    let desired = Vec2::new(ui.available_width().min(680.0), 168.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 16.0, panel.gamma_multiply(0.85));
    painter.rect_stroke(rect, 16.0, Stroke::new(1.0, border), egui::StrokeKind::Outside);

    let header = Rect::from_min_max(rect.min + Vec2::new(16.0, 10.0), Pos2::new(rect.max.x - 16.0, rect.min.y + 30.0));
    painter.text(header.left_top(), egui::Align2::LEFT_TOP, "PROCESSED / OUTPUT SPECTRUM", egui::FontId::monospace(10.0), fg_dim);
    painter.text(header.right_top(), egui::Align2::RIGHT_TOP, "POST FREEZE · FILTER · ORGANIC · SC", egui::FontId::monospace(10.0), accent_dim);

    let canvas = Rect::from_min_max(rect.min + Vec2::new(16.0, 42.0), rect.max - Vec2::new(16.0, 14.0));
    painter.rect_filled(canvas, 12.0, Color32::from_rgb(12, 12, 18));
    for i in 1..4 {
        let y = canvas.top() + canvas.height() * i as f32 / 4.0;
        painter.line_segment([Pos2::new(canvas.left(), y), Pos2::new(canvas.right(), y)], Stroke::new(1.0, Color32::from_white_alpha(9)));
    }

    let gap = 2.0;
    let bar_w = ((canvas.width() - gap * (values.len() - 1) as f32) / values.len() as f32).max(1.0);
    for (i, v) in values.iter().enumerate() {
        let v = v.clamp(0.0, 1.0);
        let h = (v * canvas.height()).max(1.0);
        let x = canvas.left() + i as f32 * (bar_w + gap);
        let bar = Rect::from_min_size(Pos2::new(x, canvas.bottom() - h), Vec2::new(bar_w, h));
        painter.rect_filled(bar, 1.5, accent.linear_multiply(0.10 + 0.85 * v));
    }
}

fn draw_freeze_button(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    param: &BoolParam,
    accent: Color32,
    panel2: Color32,
    border: Color32,
    fg_dim: Color32,
    bg: Color32,
) {
    let frozen = param.value();
    let text = if frozen { "FROZEN" } else { "FREEZE" };
    let button = egui::Button::new(RichText::new(text).strong().size(14.0).color(if frozen { bg } else { fg_dim }))
        .fill(if frozen { accent } else { panel2 })
        .stroke(Stroke::new(1.0, if frozen { accent } else { border }))
        .corner_radius(24.0)
        .min_size(Vec2::new(136.0, 42.0));
    if ui.add(button).clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, !frozen);
        setter.end_set_parameter(param);
    }
}

fn draw_knob_row(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &SpectralFreezeParams,
    accent: Color32,
    panel2: Color32,
    border: Color32,
    fg: Color32,
    fg_dim: Color32,
) {
    let row_width = ui.available_width().min(680.0);
    let tile_width = 132.0;
    let row_height = 148.0;
    let gap = ((row_width - tile_width * 4.0) / 3.0).max(0.0);

    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(row_width, row_height), egui::Sense::hover());
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(row_rect), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            draw_knob(ui, setter, &params.filter, "FILTER", "pct", accent, panel2, border, fg, fg_dim);
            draw_knob(ui, setter, &params.organic, "ORGANIC", "pct", accent, panel2, border, fg, fg_dim);
            draw_knob(ui, setter, &params.sc_boost, "SC BOOST", "db", accent, panel2, border, fg, fg_dim);
            draw_knob(ui, setter, &params.sc_freq_smoothing, "SC SMOOTH", "pct", accent, panel2, border, fg, fg_dim);
        });
    });
}

fn draw_knob(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    param: &FloatParam,
    label: &str,
    kind: &str,
    accent: Color32,
    panel2: Color32,
    border: Color32,
    fg: Color32,
    fg_dim: Color32,
) {
    // Give each knob a fixed tile. `vertical_centered()` inside a wrapped row can
    // consume the full remaining row width in egui, which pushed later knobs off
    // the bottom/right of the plugin window.
    ui.allocate_ui_with_layout(
        Vec2::new(132.0, 148.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.label(RichText::new(label).size(11.0).color(fg_dim));
            let size = Vec2::splat(98.0);
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
            if response.drag_started() {
                setter.begin_set_parameter(param);
            }
            if response.dragged() {
                let range = param.modulated_normalized_value();
                let sensitivity = if ui.input(|i| i.modifiers.shift) { 0.0005 } else { 0.004 };
                let dy = ui.input(|i| i.pointer.delta().y);
                let next = (range - dy * sensitivity).clamp(0.0, 1.0);
                setter.set_parameter_normalized(param, next);
            }
            if response.drag_stopped() {
                setter.end_set_parameter(param);
            }
            if response.double_clicked() {
                setter.begin_set_parameter(param);
                setter.set_parameter(param, param.default_plain_value());
                setter.end_set_parameter(param);
            }

            let painter = ui.painter();
            let center = rect.center();
            let radius = rect.width() * 0.5 - 8.0;
            let norm = param.modulated_normalized_value();
            draw_arc(painter, center, radius, -135.0, 135.0, Stroke::new(4.0, border));
            draw_arc(painter, center, radius, -135.0, -135.0 + norm * 270.0, Stroke::new(4.0, accent));
            painter.circle_filled(center, radius - 12.0, panel2);
            painter.circle_stroke(center, radius - 12.0, Stroke::new(1.0, border));
            let angle = (-135.0 + norm * 270.0 - 90.0).to_radians();
            let p1 = center + Vec2::new(angle.cos(), angle.sin()) * (radius - 24.0);
            let p2 = center + Vec2::new(angle.cos(), angle.sin()) * (radius - 14.0);
            painter.line_segment([p1, p2], Stroke::new(2.0, accent));

            let value = param.value();
            let text = match kind {
                "pct" => format!("{}%", (value * 100.0).round() as i32),
                "db" => format!("+{value:.1} dB"),
                _ => format!("{value:.2}"),
            };
            ui.label(RichText::new(text).monospace().size(14.0).color(fg));
        },
    );
}

fn draw_arc(painter: &egui::Painter, center: Pos2, radius: f32, start_deg: f32, end_deg: f32, stroke: Stroke) {
    let steps = 48.max(((end_deg - start_deg).abs() / 6.0) as usize);
    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let deg = start_deg + (end_deg - start_deg) * t - 90.0;
        let rad = deg.to_radians();
        points.push(center + Vec2::new(rad.cos(), rad.sin()) * radius);
    }
    painter.add(egui::Shape::line(points, stroke));
}

nih_export_clap!(SpectralFreezePlugin);
nih_export_vst3!(SpectralFreezePlugin);
