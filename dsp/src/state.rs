use crate::constants::*;

pub(crate) struct OrganicAmState {
    pub(crate) value: [f32; ORGANIC_AM_BANDS],
    pub(crate) target: [f32; ORGANIC_AM_BANDS],
    pub(crate) hop_counter: usize,
}

impl Default for OrganicAmState {
    fn default() -> Self {
        Self {
            value: [0.0; ORGANIC_AM_BANDS],
            target: [0.0; ORGANIC_AM_BANDS],
            hop_counter: 0,
        }
    }
}

pub(crate) struct OrganicScratch {
    pub(crate) mag: [f32; NUM_BINS],
    pub(crate) phase: [f32; NUM_BINS],
    pub(crate) shaped_mag: [f32; NUM_BINS],
}

impl Default for OrganicScratch {
    fn default() -> Self {
        Self {
            mag: [0.0; NUM_BINS],
            phase: [0.0; NUM_BINS],
            shaped_mag: [0.0; NUM_BINS],
        }
    }
}
