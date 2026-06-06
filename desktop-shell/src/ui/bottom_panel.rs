use super::{controls::draw_param_knob, filter_panel::selection_help, theme::UiTheme};
use crate::params::SpectralFreezeParams;
use crate::state::InstrumentState;
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
                RichText::new("Spectral Motion + Organic")
                    .strong()
                    .color(theme.fg),
            );
            ui.horizontal_wrapped(|ui| {
                draw_param_knob(ui, setter, &params.mag_glide, "Mag Glide", theme);
                draw_param_knob(ui, setter, &params.phase_glide, "Phase Glide", theme);
                draw_param_knob(ui, setter, &params.organic, "Organic", theme);
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
