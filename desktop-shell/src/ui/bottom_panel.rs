use super::{controls::draw_param_knob, theme::UiTheme};
use crate::params::SpectralFreezeParams;
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{self, RichText, Stroke};

pub(super) fn draw_bottom_panel(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &SpectralFreezeParams,
    theme: &UiTheme,
) {
    egui::Frame::NONE
        .fill(theme.panel)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(12.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.label(RichText::new("Expression").strong().color(theme.fg));
            ui.horizontal_wrapped(|ui| {
                draw_param_knob(ui, setter, &params.attack, "Attack", theme);
                draw_param_knob(ui, setter, &params.release, "Release", theme);
                draw_param_knob(ui, setter, &params.glide, "Glide", theme);
                draw_param_knob(ui, setter, &params.organic, "Organic", theme);
                draw_param_knob(ui, setter, &params.filter, "Filter", theme);
            });
            ui.painter().rect_stroke(
                ui.min_rect(),
                12.0,
                Stroke::new(0.0, theme.panel2),
                egui::StrokeKind::Outside,
            );
        });
}
