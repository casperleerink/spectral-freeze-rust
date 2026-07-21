use super::theme::UiTheme;
use crate::state::{EditorRuntime, InstrumentState};
use dsp::{PAD_COUNT, format_time, note_label, pad_note};
use nih_plug_egui::egui::{self, Align2, Color32, FontId, RichText, Stroke, Vec2};

pub(super) fn draw_pad_grid(
    ui: &mut egui::Ui,
    runtime: &mut EditorRuntime,
    state: &mut InstrumentState,
    active: [bool; PAD_COUNT],
    theme: &UiTheme,
) {
    let panel_width = ui.available_width();
    let source_auditioning = runtime.audition_enabled;
    let panel_fill = if source_auditioning {
        theme.panel.gamma_multiply(0.72)
    } else {
        theme.panel
    };
    let text_color = if source_auditioning {
        theme.fg_dim
    } else {
        theme.fg
    };

    egui::Frame::NONE
        .fill(panel_fill)
        .stroke(Stroke::new(
            1.0,
            if source_auditioning {
                theme.border.gamma_multiply(0.7)
            } else {
                theme.border
            },
        ))
        .corner_radius(12.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_min_width((panel_width - 20.0).max(0.0));
            ui.set_height(270.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Play Pads").strong().color(text_color));
                    if source_auditioning {
                        ui.label(
                            RichText::new("Sound Select active")
                                .size(10.0)
                                .color(theme.fg_dim),
                        );
                    }
                });
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
                                draw_pad(ui, runtime, state, active, theme, pad, pad_size);
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

#[allow(clippy::too_many_arguments)]
fn draw_pad(
    ui: &mut egui::Ui,
    runtime: &mut EditorRuntime,
    state: &mut InstrumentState,
    active: [bool; PAD_COUNT],
    theme: &UiTheme,
    pad: usize,
    pad_size: Vec2,
) {
    let source_auditioning = runtime.audition_enabled;
    let assigned = state.pad_assignments[pad].and_then(|i| state.pool.get(i));
    let is_drop_target = runtime.drag_pool_item.is_some();

    let (rect, response) = ui.allocate_exact_size(pad_size, egui::Sense::click());
    let hovered_drop = is_drop_target && response.contains_pointer();
    let playing = active[pad] || runtime.mouse_pad_gates[pad];

    let fill = if playing {
        theme.accent
    } else if hovered_drop {
        theme.accent.gamma_multiply(0.35)
    } else if is_drop_target {
        theme.panel2.linear_multiply(1.35)
    } else if assigned.is_some() {
        theme.panel2
    } else {
        theme.panel2.gamma_multiply(0.55)
    };
    let fill = if source_auditioning && !playing {
        fill.gamma_multiply(0.72)
    } else {
        fill
    };
    let stroke = if hovered_drop {
        Stroke::new(2.0, theme.accent)
    } else if assigned.is_some() {
        Stroke::new(1.0, theme.accent.gamma_multiply(0.45))
    } else {
        Stroke::new(1.0, theme.border)
    };

    let (header_color, name_color, time_color) = if playing {
        (Color32::BLACK, Color32::BLACK, Color32::BLACK)
    } else if source_auditioning {
        (theme.fg_dim.gamma_multiply(0.8), theme.fg_dim, theme.fg_dim)
    } else {
        (theme.fg_dim, theme.fg, theme.accent.gamma_multiply(0.85))
    };

    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 8.0, fill);
    painter.rect_stroke(rect, 8.0, stroke, egui::StrokeKind::Inside);
    painter.text(
        rect.left_top() + Vec2::new(8.0, 6.0),
        Align2::LEFT_TOP,
        format!("Pad {} · {}", pad + 1, note_label(pad_note(pad))),
        FontId::proportional(9.0),
        header_color,
    );
    match assigned {
        Some(item) => {
            painter.text(
                rect.left_top() + Vec2::new(8.0, 21.0),
                Align2::LEFT_TOP,
                short_name(base_name(&item.name)),
                FontId::proportional(11.0),
                name_color,
            );
            painter.text(
                rect.left_top() + Vec2::new(8.0, 38.0),
                Align2::LEFT_TOP,
                format!("@ {}", format_time(item.cursor_time_seconds)),
                FontId::monospace(9.0),
                time_color,
            );
        }
        None => {
            painter.text(
                rect.left_top() + Vec2::new(8.0, 21.0),
                Align2::LEFT_TOP,
                if is_drop_target { "Drop here" } else { "Empty" },
                FontId::proportional(11.0),
                theme.fg_dim.gamma_multiply(0.8),
            );
        }
    }

    runtime.mouse_pad_gates[pad] =
        response.is_pointer_button_down_on() && state.pad_assignments[pad].is_some();
    if response.contains_pointer()
        && ui.input(|i| i.pointer.any_released())
        && let Some(idx) = runtime.drag_pool_item.take()
        && idx < state.pool.len()
    {
        state.pad_assignments[pad] = Some(idx);
        state.mark_audio_state_changed();
    }
    response.context_menu(|ui| {
        if ui.button("Clear pad").clicked() {
            state.pad_assignments[pad] = None;
            state.mark_audio_state_changed();
            ui.close_menu();
        }
    });
}

/// The pool item name is "file.wav @ mm:ss.mmm"; the pad shows the file part
/// on its own line and the timestamp separately.
fn base_name(name: &str) -> &str {
    name.split(" @ ").next().unwrap_or(name)
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
