use super::{controls::draw_normalized_knob, theme::UiTheme};
use crate::state::{InstrumentState, Selection};
use nih_plug_egui::egui::{self, RichText, Stroke, Vec2};

pub(super) fn draw_filter_panel(ui: &mut egui::Ui, state: &mut InstrumentState, theme: &UiTheme) {
    let panel_width = ui.available_width();

    egui::Frame::NONE
        .fill(theme.panel)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(12.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_min_width((panel_width - 24.0).max(0.0));
            // This is the inner height. With the frame's 12px top/bottom margin,
            // 170px matches the source/waveform panel's outer height.
            ui.set_height(170.0);
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(RichText::new("Contextual Filter").strong().color(theme.fg));
                ui.label(
                    RichText::new(context_label(state))
                        .size(10.0)
                        .color(theme.fg_dim),
                );
                ui.add_space(4.0);

                let mut filter = current_context_filter(state);
                let value_text = format!("{:.0}%", filter * 100.0);
                let response = draw_normalized_knob(
                    ui,
                    "Filter",
                    &mut filter,
                    &value_text,
                    theme,
                    Vec2::new(86.0, 86.0),
                );
                if response.changed() {
                    set_context_filter(state, filter);
                }
                let _ = response
                    .on_hover_text("Drag up/down or left/right. Hold Shift for fine control.");

                ui.add_space(2.0);
                ui.label(
                    RichText::new(selection_help(state))
                        .size(9.0)
                        .color(theme.fg_dim),
                );
            });
        });
}

pub(super) fn current_context_filter(state: &InstrumentState) -> f32 {
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

pub(super) fn set_context_filter(state: &mut InstrumentState, value: f32) {
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
            if let Some(idx) = state.pad_assignments[pad]
                && let Some(item) = state.pool.get_mut(idx)
            {
                item.filter = value;
                state.mark_audio_state_changed();
            }
        }
    }
}

pub(super) fn selection_help(state: &InstrumentState) -> String {
    match state.selection {
        Selection::Waveform => "Audition + next capture".to_string(),
        Selection::Pool(idx) => format!("Pool item {} + assigned pads", idx + 1),
        Selection::Pad(pad) => format!("Pad {} pool item", pad + 1),
    }
}

fn context_label(state: &InstrumentState) -> &'static str {
    match state.selection {
        Selection::Waveform => "Audition monitor",
        Selection::Pool(_) => "Freeze pool",
        Selection::Pad(_) => "Pad assignment",
    }
}
