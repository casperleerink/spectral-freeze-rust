use nih_plug::prelude::{FloatParam, Param, ParamSetter};
use nih_plug_egui::egui;

pub(super) fn draw_param_slider(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    param: &FloatParam,
    label: &str,
) {
    let mut normalized = param.modulated_normalized_value();
    let response = ui.add(
        egui::Slider::new(&mut normalized, 0.0..=1.0)
            .text(label)
            .show_value(false),
    );
    if response.drag_started() {
        setter.begin_set_parameter(param);
    }
    if response.changed() {
        setter.set_parameter_normalized(param, normalized);
    }
    if response.drag_stopped() {
        setter.end_set_parameter(param);
    }
    response.on_hover_text(param.to_string());
}
