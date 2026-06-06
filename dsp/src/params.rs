use crate::clamp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Bool,
    Float,
}

#[derive(Clone, Copy, Debug)]
pub struct ParamInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ParamKind,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub unit: &'static str,
}

pub const PARAM_FREEZE: usize = 0;
pub const PARAM_FILTER: usize = 1;
pub const PARAM_ORGANIC: usize = 2;

pub const PARAMS: [ParamInfo; 3] = [
    ParamInfo {
        id: "freeze",
        name: "Freeze",
        kind: ParamKind::Bool,
        min: 0.0,
        max: 1.0,
        default: 0.0,
        unit: "",
    },
    ParamInfo {
        id: "filter",
        name: "Filter",
        kind: ParamKind::Float,
        min: 0.0,
        max: 1.0,
        default: 0.0,
        unit: "%",
    },
    ParamInfo {
        id: "organic",
        name: "Organic",
        kind: ParamKind::Float,
        min: 0.0,
        max: 1.0,
        default: 0.0,
        unit: "%",
    },
];

/// Static JSON generated from the manifest above for the WAM JS layer.
pub const PARAMETER_MANIFEST_JSON: &str = r#"[
  {"id":"freeze","name":"Freeze","kind":"bool","min":0.0,"max":1.0,"default":0.0,"unit":""},
  {"id":"filter","name":"Filter","kind":"float","min":0.0,"max":1.0,"default":0.0,"unit":"%"},
  {"id":"organic","name":"Organic","kind":"float","min":0.0,"max":1.0,"default":0.0,"unit":"%"}
]"#;

#[derive(Clone, Copy, Debug)]
pub struct ProcessParams {
    pub freeze: bool,
    pub filter: f32,
    pub organic: f32,
}

impl Default for ProcessParams {
    fn default() -> Self {
        Self {
            freeze: PARAMS[PARAM_FREEZE].default >= 0.5,
            filter: PARAMS[PARAM_FILTER].default,
            organic: PARAMS[PARAM_ORGANIC].default,
        }
    }
}

impl ProcessParams {
    pub fn from_values(values: [f32; 3]) -> Self {
        Self {
            freeze: values[PARAM_FREEZE] >= 0.5,
            filter: values[PARAM_FILTER],
            organic: values[PARAM_ORGANIC],
        }
        .clamped()
    }

    pub fn clamped(self) -> Self {
        Self {
            freeze: self.freeze,
            filter: clamp(self.filter, 0.0, 1.0),
            organic: clamp(self.organic, 0.0, 1.0),
        }
    }
}
