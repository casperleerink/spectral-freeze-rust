use super::{theme::UiTheme, waveform::draw_waveform};
use crate::source::loader;
use crate::state::{EditorRuntime, InstrumentState, Selection};
use dsp::capture_freeze_from_audio;
use nih_plug_egui::egui::{self, Color32, RichText, Stroke};

pub(super) fn draw_source_panel(
    ui: &mut egui::Ui,
    runtime: &mut EditorRuntime,
    state: &mut InstrumentState,
    theme: &UiTheme,
) {
    let panel_width = ui.available_width();

    egui::Frame::NONE
        .fill(theme.panel)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(12.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_min_width((panel_width - 24.0).max(0.0));
            ui.set_height(170.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Source Files / Load WAV")
                        .strong()
                        .color(theme.fg),
                );
                if ui
                    .add_enabled(
                        runtime.pending_source_rx.is_none(),
                        egui::Button::new("Load WAV").fill(theme.panel2),
                    )
                    .clicked()
                {
                    loader::open_dialog(runtime);
                }
                let mut audition = runtime.audition_enabled;
                if ui.checkbox(&mut audition, "Audition Monitor").changed() {
                    runtime.audition_enabled = audition;
                    runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
                }
                if ui
                    .add_enabled(
                        runtime.source.is_some(),
                        egui::Button::new("Capture").fill(theme.accent.gamma_multiply(0.35)),
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
                            state.mark_audio_state_changed();
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
            ui.label(RichText::new(source_label).size(11.0).color(theme.fg_dim));
            if let Some(status) = &runtime.file_status {
                ui.label(RichText::new(status).size(11.0).color(theme.accent));
            }
            if let Some(err) = &runtime.file_error {
                ui.label(
                    RichText::new(err)
                        .size(11.0)
                        .color(Color32::from_rgb(255, 130, 130)),
                );
            }
            draw_waveform(ui, runtime, state, theme);
        });
}
