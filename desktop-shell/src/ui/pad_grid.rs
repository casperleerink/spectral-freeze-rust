use super::theme::UiTheme;
use crate::state::{EditorRuntime, InstrumentState, Selection};
use dsp::{PAD_COUNT, note_label, pad_note};
use nih_plug_egui::egui::{self, Color32, RichText, Stroke, Vec2};

pub(super) fn draw_pad_grid(
    ui: &mut egui::Ui,
    runtime: &mut EditorRuntime,
    state: &mut InstrumentState,
    active: [bool; PAD_COUNT],
    theme: &UiTheme,
) {
    let panel_width = ui.available_width();

    egui::Frame::NONE
        .fill(theme.panel)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(12.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_min_width((panel_width - 20.0).max(0.0));
            ui.set_height(270.0);
            ui.vertical(|ui| {
                let spacing = Vec2::new(6.0, 6.0);
                let pad_size = Vec2::new(
                    ((ui.available_width() - spacing.x * 3.0) / 4.0).max(48.0),
                    58.0,
                );

                egui::Grid::new("freeze-pad-grid")
                    .num_columns(4)
                    .spacing(spacing)
                    .show(ui, |ui| {
                        for row in 0..4 {
                            for col in 0..4 {
                                let pad = row * 4 + col;
                                let assigned =
                                    state.pad_assignments[pad].and_then(|i| state.pool.get(i));
                                let selected =
                                    matches!(state.selection, Selection::Pad(p) if p == pad);
                                let is_drop_target = runtime.drag_pool_item.is_some();
                                let fill = if active[pad] || runtime.mouse_pad_gates[pad] {
                                    theme.accent
                                } else if selected {
                                    Color32::from_rgb(82, 105, 128)
                                } else if is_drop_target {
                                    theme.panel2.linear_multiply(1.35)
                                } else if assigned.is_some() {
                                    theme.panel2
                                } else {
                                    theme.panel2.gamma_multiply(0.55)
                                };
                                let short = assigned
                                    .map(|i| short_name(&i.name))
                                    .unwrap_or_else(|| "Empty".to_string());
                                let text = format!(
                                    "Pad {}  {}\n{}",
                                    pad + 1,
                                    note_label(pad_note(pad)),
                                    short
                                );
                                let response = ui.add_sized(
                                    pad_size,
                                    egui::Button::new(RichText::new(text).size(11.0).color(
                                        if fill == theme.accent {
                                            Color32::BLACK
                                        } else {
                                            theme.fg
                                        },
                                    ))
                                    .truncate()
                                    .fill(fill)
                                    .stroke(Stroke::new(1.0, theme.border)),
                                );
                                runtime.mouse_pad_gates[pad] =
                                    response.is_pointer_button_down_on() && assigned.is_some();
                                if response.clicked() {
                                    state.selection = Selection::Pad(pad);
                                }
                                if response.contains_pointer()
                                    && ui.input(|i| i.pointer.any_released())
                                    && let Some(idx) = runtime.drag_pool_item.take()
                                    && idx < state.pool.len()
                                {
                                    state.pad_assignments[pad] = Some(idx);
                                    state.mark_audio_state_changed();
                                    state.selection = Selection::Pad(pad);
                                }
                                response.context_menu(|ui| {
                                    if ui.button("Clear pad").clicked() {
                                        state.pad_assignments[pad] = None;
                                        state.mark_audio_state_changed();
                                        ui.close_menu();
                                    }
                                });
                            }
                            ui.end_row();
                        }
                    });
                if ui.input(|i| i.pointer.any_released()) {
                    runtime.drag_pool_item = None;
                }
            });
        });
}

fn short_name(name: &str) -> String {
    const MAX: usize = 14;
    if name.chars().count() <= MAX {
        name.to_string()
    } else {
        let mut s = name.chars().take(MAX - 1).collect::<String>();
        s.push('…');
        s
    }
}
