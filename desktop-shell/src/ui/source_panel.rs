use super::{theme::UiTheme, waveform::draw_waveform};
use crate::state::{EditorRuntime, InstrumentState};
use dsp::capture_freeze_from_audio;
use nih_plug_egui::egui::{self, Color32, RichText, Stroke};

pub(super) fn draw_source_panel(
    ui: &mut egui::Ui,
    runtime: &mut EditorRuntime,
    state: &mut InstrumentState,
    theme: &UiTheme,
) {
    let panel_width = ui.available_width();
    let is_sound_select = runtime.audition_enabled;

    egui::Frame::NONE
        .fill(theme.panel)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(12.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_min_width((panel_width - 24.0).max(0.0));
            ui.set_height(170.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Mode").strong().color(theme.fg));
                if mode_button(ui, theme, "Sound Select", is_sound_select).clicked()
                    && !runtime.audition_enabled
                {
                    runtime.audition_enabled = true;
                    runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
                }
                if mode_button(ui, theme, "Play Pads", !is_sound_select).clicked()
                    && runtime.audition_enabled
                {
                    runtime.audition_enabled = false;
                    runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let capture_enabled = runtime.source.is_some();
                    if ui
                        .add_enabled(capture_enabled, capture_button(theme, capture_enabled))
                        .clicked()
                        && let Some(source) = &runtime.source
                        && let Some(item) = capture_freeze_from_audio(
                            &source.channels,
                            source.sample_rate,
                            state.source_cursor_sample,
                            Some(&source.path.to_string_lossy()),
                        )
                    {
                        state.pool.push(item);
                        state.mark_audio_state_changed();
                    }
                });
            });

            if let Some(source_label) = if let Some(source) = &runtime.source {
                Some(format!(
                    "{} · {:.2}s · {} ch · {} Hz",
                    source
                        .path
                        .file_name()
                        .and_then(|p| p.to_str())
                        .unwrap_or("audio.wav"),
                    source.duration_seconds(),
                    source.channels.len(),
                    source.sample_rate.round() as u32
                ))
            } else if let Some(path) = &state.source_path {
                Some(format!(
                    "Missing source file: {path} (captured pool remains playable)"
                ))
            } else {
                None
            } {
                ui.label(RichText::new(source_label).size(11.0).color(theme.fg_dim));
            }
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

fn capture_button(theme: &UiTheme, enabled: bool) -> egui::Button<'static> {
    let fill = if enabled {
        theme.accent.gamma_multiply(0.85)
    } else {
        theme.panel2.gamma_multiply(0.72)
    };
    let text_color = if enabled {
        Color32::BLACK
    } else {
        theme.fg_dim
    };
    egui::Button::new(
        RichText::new("Capture Freeze")
            .size(12.0)
            .strong()
            .color(text_color),
    )
    .fill(fill)
    .stroke(Stroke::new(
        1.0,
        if enabled { theme.accent } else { theme.border },
    ))
}

fn mode_button(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    label: &'static str,
    selected: bool,
) -> egui::Response {
    let fill = if selected {
        theme.accent.gamma_multiply(0.55)
    } else {
        theme.panel2.gamma_multiply(0.8)
    };
    let text_color = if selected {
        Color32::WHITE
    } else {
        theme.fg_dim
    };
    ui.add_sized(
        [104.0, 26.0],
        egui::Button::new(RichText::new(label).size(12.0).strong().color(text_color))
            .fill(fill)
            .stroke(Stroke::new(
                if selected { 1.25 } else { 1.0 },
                if selected { theme.accent } else { theme.border },
            )),
    )
}
