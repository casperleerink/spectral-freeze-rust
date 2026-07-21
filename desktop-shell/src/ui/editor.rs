use super::{
    bottom_panel::draw_bottom_panel, filter_panel::draw_filter_panel, pad_grid::draw_pad_grid,
    pool_panel::draw_pool_panel, source_panel::draw_source_panel, theme::UiTheme,
};
use crate::params::SpectralFreezeParams;
use crate::source::loader;
use crate::state::{EditorRuntime, InstrumentState, PadActivityAtomics};
use dsp::capture_freeze_from_audio;
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{self, Vec2};
use std::sync::{Arc, Mutex};

pub(crate) fn draw_editor(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &SpectralFreezeParams,
    runtime: &Arc<Mutex<EditorRuntime>>,
    activity: &PadActivityAtomics,
) {
    let theme = UiTheme::default();

    ui.painter().rect_filled(ui.max_rect(), 0.0, theme.bg);
    ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);

    let mut runtime = runtime.lock().unwrap();
    runtime.editor_frame_generation = runtime.editor_frame_generation.wrapping_add(1);
    let mut state = params.instrument_state.lock().unwrap();
    loader::poll(&mut runtime, &mut state);
    sync_runtime_source_metadata(&mut runtime, &mut state);
    refresh_audition(&mut runtime, &state);

    egui::Frame::NONE
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal_top(|ui| {
                    let spacing = ui.spacing().item_spacing.x;
                    let filter_width = 178.0;
                    let source_width = (ui.available_width() - filter_width - spacing).max(360.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(source_width, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| draw_source_panel(ui, &mut runtime, &mut state, &theme),
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(filter_width, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| draw_filter_panel(ui, &mut state, &theme),
                    );
                });

                ui.columns(2, |columns| {
                    draw_pool_panel(&mut columns[0], &mut runtime, &mut state, &theme);
                    draw_pad_grid(
                        &mut columns[1],
                        &mut runtime,
                        &mut state,
                        activity.load(),
                        &theme,
                    );
                });

                draw_bottom_panel(ui, setter, params, &mut state, &theme);
            });
        });

    draw_drag_ghost(ui.ctx(), &runtime, &state, &theme);

    refresh_audition(&mut runtime, &state);
}

/// Floating label that follows the pointer while a pool item is dragged, since
/// the OS cursor can't be changed on macOS and the drop needs a clear affordance.
fn draw_drag_ghost(
    ctx: &egui::Context,
    runtime: &EditorRuntime,
    state: &InstrumentState,
    theme: &UiTheme,
) {
    let Some(idx) = runtime.drag_pool_item else {
        return;
    };
    let Some(item) = state.pool.get(idx) else {
        return;
    };
    let Some(pos) = ctx.pointer_latest_pos() else {
        return;
    };

    egui::Area::new(egui::Id::new("pool-drag-ghost"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + Vec2::new(14.0, 12.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme.panel2)
                .stroke(egui::Stroke::new(1.0, theme.accent))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(egui::RichText::new(&item.name).size(11.0).color(theme.fg));
                    ui.label(
                        egui::RichText::new("Drop on a pad")
                            .size(9.0)
                            .color(theme.fg_dim),
                    );
                });
        });
}

fn sync_runtime_source_metadata(runtime: &mut EditorRuntime, state: &mut InstrumentState) {
    if let Some(source) = &runtime.source {
        state.source_path = Some(source.path.to_string_lossy().to_string());
        state.source_sample_rate = source.sample_rate;
        let len = source.len_samples();
        if len > 0 {
            state.source_cursor_sample = state.source_cursor_sample.min(len - 1);
        }
    }
}

fn refresh_audition(runtime: &mut EditorRuntime, state: &InstrumentState) {
    if !runtime.audition_enabled {
        if runtime.audition_item.is_some() {
            runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
        }
        runtime.audition_item = None;
        return;
    }
    let Some(source) = &runtime.source else {
        if runtime.audition_item.is_some() {
            runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
        }
        runtime.audition_item = None;
        return;
    };

    let source_path = source.path.to_string_lossy();
    let needs_capture = match &runtime.audition_item {
        None => true,
        Some(item) => {
            item.cursor_sample != state.source_cursor_sample
                || item.source_path.as_deref() != Some(source_path.as_ref())
        }
    };

    if needs_capture {
        runtime.audition_item = capture_freeze_from_audio(
            &source.channels,
            source.sample_rate,
            state.source_cursor_sample,
            Some(&source_path),
            state.contextual_filter,
        )
        .map(Arc::new);
        runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
    } else if let Some(item) = &runtime.audition_item
        && (item.filter - state.contextual_filter).abs() > 1.0e-6
    {
        // Filter-only change: swap the item without bumping the revision so
        // the audition voice keeps sounding instead of restarting per tick.
        let mut updated = (**item).clone();
        updated.filter = state.contextual_filter;
        runtime.audition_item = Some(Arc::new(updated));
    }
}
