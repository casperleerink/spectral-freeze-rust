use super::{
    bottom_panel::draw_bottom_panel, pad_grid::draw_pad_grid, pool_panel::draw_pool_panel,
    source_panel::draw_source_panel, theme::UiTheme,
};
use crate::params::SpectralFreezeParams;
use crate::source::loader;
use crate::state::{EditorRuntime, InstrumentState, PadActivityAtomics};
use dsp::capture_freeze_from_audio;
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{self, RichText, Vec2};
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
    let mut state = params.instrument_state.lock().unwrap();
    loader::poll(&mut runtime, &mut state);
    sync_runtime_source_metadata(&mut runtime, &mut state);
    refresh_audition(&mut runtime, &state);

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("SPECTRAL FREEZE INSTRUMENT")
                    .size(19.0)
                    .strong()
                    .color(theme.fg),
            );
            ui.label(
                RichText::new("WAV → FREEZE POOL → 16 MIDI PADS")
                    .size(11.0)
                    .color(theme.fg_dim),
            );
        });

        draw_source_panel(ui, &mut runtime, &mut state, &theme);

        ui.horizontal(|ui| {
            draw_pool_panel(ui, &mut runtime, &mut state, &theme);
            draw_pad_grid(ui, &mut runtime, &mut state, activity.load(), &theme);
        });

        draw_bottom_panel(ui, setter, params, &mut state, &theme);
    });

    refresh_audition(&mut runtime, &state);
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
    let next = capture_freeze_from_audio(
        &source.channels,
        source.sample_rate,
        state.source_cursor_sample,
        Some(&source.path.to_string_lossy()),
        state.contextual_filter,
    );
    let changed = match (&runtime.audition_item, &next) {
        (Some(a), Some(b)) => {
            a.cursor_sample != b.cursor_sample
                || (a.filter - b.filter).abs() > 1.0e-6
                || a.source_path != b.source_path
        }
        (None, None) => false,
        _ => true,
    };
    if changed {
        runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
    }
    runtime.audition_item = next;
}
