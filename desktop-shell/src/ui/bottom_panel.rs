use super::{controls::draw_param_slider, theme::UiTheme};
use crate::params::SpectralFreezeParams;
use crate::state::{InstrumentState, Selection};
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{self, RichText, Stroke};

pub(super) fn draw_bottom_panel(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &SpectralFreezeParams,
    state: &mut InstrumentState,
    theme: &UiTheme,
) {
    egui::Frame::NONE
        .fill(theme.panel)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(12.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.label(
                RichText::new("Contextual Filter + ADSR + Organic + SC")
                    .strong()
                    .color(theme.fg),
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
                    .color(theme.fg_dim),
            );
            ui.painter().rect_stroke(
                ui.min_rect(),
                12.0,
                Stroke::new(0.0, theme.panel2),
                egui::StrokeKind::Outside,
            );
        });
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
                state.mark_audio_state_changed();
            }
        }
        Selection::Pad(pad) => {
            if let Some(idx) = state.pad_assignments[pad] {
                if let Some(item) = state.pool.get_mut(idx) {
                    item.filter = value;
                    state.mark_audio_state_changed();
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
