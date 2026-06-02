use super::theme::UiTheme;
use crate::state::{EditorRuntime, InstrumentState, Selection};
use nih_plug_egui::egui::{self, Color32, RichText, Stroke, Vec2};

pub(super) fn draw_pool_panel(
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
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_min_width((panel_width - 20.0).max(0.0));
            ui.set_height(270.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Freeze Pool").strong().color(theme.fg));
                ui.label(
                    RichText::new(format!("{} items", state.pool.len()))
                        .size(10.0)
                        .color(theme.fg_dim),
                );
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .max_height(226.0)
                    .show(ui, |ui| {
                        let mut delete_idx = None;
                        for idx in 0..state.pool.len() {
                            let selected =
                                matches!(state.selection, Selection::Pool(i) if i == idx);
                            ui.horizontal(|ui| {
                                let label = format!(
                                    "{}  ·  F{}%",
                                    state.pool[idx].name,
                                    (state.pool[idx].filter * 100.0).round() as i32
                                );
                                let response =
                                    ui.add(
                                        egui::Button::new(RichText::new(label).color(
                                            if selected { Color32::BLACK } else { theme.fg },
                                        ))
                                        .fill(if selected { theme.accent } else { theme.panel2 })
                                        .truncate()
                                        .min_size(Vec2::new(
                                            (ui.available_width() - 26.0).max(0.0),
                                            24.0,
                                        ))
                                        .sense(egui::Sense::click_and_drag()),
                                    );
                                if response.clicked() {
                                    state.selection = Selection::Pool(idx);
                                }
                                if response.drag_started() {
                                    runtime.drag_pool_item = Some(idx);
                                }
                                if response.dragged() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                                }
                                if ui.small_button("×").clicked() {
                                    delete_idx = Some(idx);
                                }
                            });
                        }
                        if state.pool.is_empty() {
                            ui.label(
                                RichText::new("Capture freezes here. Drag items onto pads.")
                                    .color(theme.fg_dim),
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
                            state.mark_audio_state_changed();
                            state.selection = Selection::Waveform;
                        }
                    });
            });
        });
}
