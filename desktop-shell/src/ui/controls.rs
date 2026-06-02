use super::theme::UiTheme;
use nih_plug::prelude::{FloatParam, Param, ParamSetter};
use nih_plug_egui::egui::{
    self, Align2, Color32, FontId, Pos2, Response, Sense, Shape, Stroke, Vec2,
};
use std::f32::consts::PI;

const KNOB_START_ANGLE: f32 = PI * 0.75;
const KNOB_END_ANGLE: f32 = PI * 2.25;

pub(super) fn draw_param_knob(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    param: &FloatParam,
    label: &str,
    theme: &UiTheme,
) {
    let mut normalized = param.modulated_normalized_value();
    let response = draw_normalized_knob(
        ui,
        label,
        &mut normalized,
        &param.to_string(),
        theme,
        Vec2::new(68.0, 82.0),
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
    let _ = response.on_hover_text(param.to_string());
}

pub(super) fn draw_normalized_knob(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    value_text: &str,
    theme: &UiTheme,
    desired_size: Vec2,
) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

    if response.dragged() {
        let motion = response.drag_motion();
        let fine = ui.input(|input| input.modifiers.shift);
        let speed = if fine { 0.002 } else { 0.006 };
        let next = (*value + (motion.x - motion.y) * speed).clamp(0.0, 1.0);
        if (next - *value).abs() > f32::EPSILON {
            *value = next;
            response.mark_changed();
        }
    }

    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let hovered = response.hovered() || response.dragged();
        let radius = (desired_size.x.min(desired_size.y) * 0.28).clamp(16.0, 28.0);
        let center = Pos2::new(rect.center().x, rect.top() + radius + 24.0);
        let track_stroke = Stroke::new(
            3.0,
            theme
                .border
                .gamma_multiply(if hovered { 1.35 } else { 1.0 }),
        );
        let value_stroke = Stroke::new(3.5, theme.accent);

        painter.text(
            Pos2::new(rect.center().x, rect.top() + 2.0),
            Align2::CENTER_TOP,
            label,
            FontId::proportional(10.0),
            theme.fg_dim,
        );
        painter.circle_filled(
            center,
            radius,
            theme
                .panel2
                .gamma_multiply(if hovered { 1.12 } else { 0.9 }),
        );
        painter.circle_stroke(center, radius, Stroke::new(1.0, theme.border));
        painter.add(Shape::line(
            arc_points(center, radius + 5.0, KNOB_START_ANGLE, KNOB_END_ANGLE, 28),
            track_stroke,
        ));
        let angle = egui::lerp(KNOB_START_ANGLE..=KNOB_END_ANGLE, *value);
        painter.add(Shape::line(
            arc_points(center, radius + 5.0, KNOB_START_ANGLE, angle, 20),
            value_stroke,
        ));

        let indicator_end = center + Vec2::angled(angle) * (radius * 0.72);
        painter.line_segment([center, indicator_end], Stroke::new(2.0, theme.fg));
        painter.circle_filled(center, 2.5, theme.fg);

        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 14.0),
            Align2::CENTER_CENTER,
            value_text,
            FontId::monospace(10.0),
            Color32::from_rgb(210, 220, 230),
        );
    }

    response
}

fn arc_points(center: Pos2, radius: f32, start: f32, end: f32, steps: usize) -> Vec<Pos2> {
    let steps = steps.max(1);
    (0..=steps)
        .map(|idx| {
            let t = idx as f32 / steps as f32;
            let angle = egui::lerp(start..=end, t);
            center + Vec2::angled(angle) * radius
        })
        .collect()
}
