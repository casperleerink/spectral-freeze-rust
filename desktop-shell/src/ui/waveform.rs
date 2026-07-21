use super::theme::UiTheme;
use crate::source::loader;
use crate::state::{EditorRuntime, InstrumentState};
use nih_plug_egui::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use std::path::PathBuf;

pub(super) fn draw_waveform(
    ui: &mut egui::Ui,
    runtime: &mut EditorRuntime,
    state: &mut InstrumentState,
    theme: &UiTheme,
) {
    let desired = Vec2::new(ui.available_width(), 118.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click_and_drag());
    let painter = ui.painter();
    painter.rect_filled(rect, 8.0, theme.panel2.gamma_multiply(0.75));
    painter.rect_stroke(
        rect,
        8.0,
        Stroke::new(1.0, theme.border),
        egui::StrokeKind::Outside,
    );

    let pointer_over_waveform = ui
        .ctx()
        .pointer_hover_pos()
        .is_some_and(|pos| rect.contains(pos));
    let hovered_file_count = ui.ctx().input(|input| input.raw.hovered_files.len());
    if pointer_over_waveform && hovered_file_count > 0 {
        painter.rect_stroke(
            rect.shrink(2.0),
            8.0,
            Stroke::new(2.0, theme.accent),
            egui::StrokeKind::Outside,
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Drop WAV to load",
            egui::FontId::proportional(14.0),
            Color32::WHITE,
        );
    }

    let dropped_paths: Vec<PathBuf> = ui.ctx().input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect()
    });
    if pointer_over_waveform && !dropped_paths.is_empty() {
        if let Some(path) = dropped_paths
            .into_iter()
            .find(|path| loader::is_wav_path(path))
        {
            loader::load_path(runtime, path);
        } else {
            runtime.file_error = Some("Drop a .wav file on the waveform".to_string());
        }
    }

    if let Some(source) = &runtime.source {
        let len = source.len_samples().max(1);
        if (response.clicked() || response.dragged()) && response.interact_pointer_pos().is_some() {
            let pos = response.interact_pointer_pos().unwrap();
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            state.source_cursor_sample = ((len - 1) as f32 * t).round() as usize;
            runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
        }

        let cols = rect.width().max(1.0) as usize;
        for x in 0..cols {
            let start = x * len / cols;
            let end = ((x + 1) * len / cols).max(start + 1).min(len);
            let mut peak = 0.0_f32;
            for ch in &source.channels {
                for sample in &ch[start.min(ch.len())..end.min(ch.len())] {
                    peak = peak.max(sample.abs());
                }
            }
            let x_pos = rect.left() + x as f32;
            let y0 = rect.center().y - peak * rect.height() * 0.45;
            let y1 = rect.center().y + peak * rect.height() * 0.45;
            painter.line_segment(
                [Pos2::new(x_pos, y0), Pos2::new(x_pos, y1)],
                Stroke::new(1.0, theme.accent.gamma_multiply(0.65)),
            );
        }

        let cursor_x =
            rect.left() + rect.width() * (state.source_cursor_sample as f32 / len as f32);
        painter.line_segment(
            [
                Pos2::new(cursor_x, rect.top()),
                Pos2::new(cursor_x, rect.bottom()),
            ],
            Stroke::new(2.0, Color32::WHITE),
        );
        painter.text(
            rect.left_top() + Vec2::new(10.0, 8.0),
            egui::Align2::LEFT_TOP,
            "click/scrub cursor · drop WAV to load",
            egui::FontId::monospace(10.0),
            theme.fg_dim,
        );
    } else {
        painter.text(
            rect.center() - Vec2::new(0.0, 16.0),
            egui::Align2::CENTER_CENTER,
            "Drop a WAV file to show waveform",
            egui::FontId::proportional(14.0),
            theme.fg_dim,
        );

        let button_rect =
            Rect::from_center_size(rect.center() + Vec2::new(0.0, 18.0), Vec2::new(104.0, 28.0));
        if ui
            .put(
                button_rect,
                egui::Button::new("Load WAV").fill(theme.panel2),
            )
            .clicked()
        {
            // Stored by our vendored egui-baseview so dialogs can be parented
            // to the plugin window (macOS only; None elsewhere).
            let ns_view = ui
                .ctx()
                .data(|data| data.get_temp::<usize>(egui::Id::new("egui_baseview_ns_view")));
            loader::open_dialog(runtime, ns_view);
        }
    }
}
