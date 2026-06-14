#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
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
