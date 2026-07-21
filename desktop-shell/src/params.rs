use crate::state::InstrumentState;
use dsp::{
    INSTRUMENT_PARAMS, PARAM_ATTACK, PARAM_FILTER, PARAM_GLIDE, PARAM_INSTRUMENT_ORGANIC,
    PARAM_RELEASE,
};
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::{Arc, Mutex};

#[derive(Params)]
pub struct SpectralFreezeParams {
    #[persist = "editor-state"]
    pub(crate) editor_state: Arc<EguiState>,

    #[persist = "instrument-state"]
    pub(crate) instrument_state: Arc<Mutex<InstrumentState>>,

    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "release"]
    pub release: FloatParam,
    #[id = "glide"]
    pub glide: FloatParam,
    #[id = "organic"]
    pub organic: FloatParam,
    #[id = "filter"]
    pub filter: FloatParam,
}

impl Default for SpectralFreezeParams {
    fn default() -> Self {
        let seconds = Arc::new(|value: f32| format!("{value:.3} s"));
        let seconds_from_string = Arc::new(|text: &str| parse_unit_float(text, "s"));
        let pct = Arc::new(|value: f32| format!("{}%", (value * 100.0).round() as i32));
        let pct_from_string = Arc::new(|text: &str| {
            let value = parse_unit_float(text, "%")?;
            if text.trim().contains('%') {
                Some(value / 100.0)
            } else {
                Some(value)
            }
        });
        Self {
            editor_state: EguiState::from_size(980, 720),
            instrument_state: Arc::new(Mutex::new(InstrumentState::default())),
            attack: FloatParam::new(
                INSTRUMENT_PARAMS[PARAM_ATTACK].name,
                INSTRUMENT_PARAMS[PARAM_ATTACK].default,
                FloatRange::Linear { min: 0.0, max: 5.0 },
            )
            .with_step_size(0.001)
            .with_value_to_string(seconds.clone())
            .with_string_to_value(seconds_from_string.clone()),
            release: FloatParam::new(
                INSTRUMENT_PARAMS[PARAM_RELEASE].name,
                INSTRUMENT_PARAMS[PARAM_RELEASE].default,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            )
            .with_step_size(0.001)
            .with_value_to_string(seconds.clone())
            .with_string_to_value(seconds_from_string.clone()),
            glide: FloatParam::new(
                INSTRUMENT_PARAMS[PARAM_GLIDE].name,
                INSTRUMENT_PARAMS[PARAM_GLIDE].default,
                FloatRange::Linear { min: 0.0, max: 5.0 },
            )
            .with_step_size(0.001)
            .with_value_to_string(seconds)
            .with_string_to_value(seconds_from_string),
            organic: FloatParam::new(
                INSTRUMENT_PARAMS[PARAM_INSTRUMENT_ORGANIC].name,
                INSTRUMENT_PARAMS[PARAM_INSTRUMENT_ORGANIC].default,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.001)
            .with_value_to_string(pct.clone())
            .with_string_to_value(pct_from_string.clone()),
            filter: FloatParam::new(
                INSTRUMENT_PARAMS[PARAM_FILTER].name,
                INSTRUMENT_PARAMS[PARAM_FILTER].default,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.001)
            .with_value_to_string(pct)
            .with_string_to_value(pct_from_string),
        }
    }
}

fn parse_unit_float(text: &str, unit: &str) -> Option<f32> {
    text.trim()
        .trim_start_matches('+')
        .trim_end_matches(unit)
        .trim()
        .parse::<f32>()
        .ok()
}
