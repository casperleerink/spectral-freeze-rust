use dsp::{CapturedFreeze, PAD_COUNT};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum Selection {
    Waveform,
    Pool(usize),
    Pad(usize),
}

impl Default for Selection {
    fn default() -> Self {
        Self::Waveform
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct InstrumentState {
    pub(crate) pool: Vec<CapturedFreeze>,
    pub(crate) pad_assignments: [Option<usize>; PAD_COUNT],
    pub(crate) source_path: Option<String>,
    pub(crate) source_cursor_sample: usize,
    pub(crate) source_sample_rate: f32,
    pub(crate) contextual_filter: f32,
    pub(crate) selection: Selection,
}

impl Default for InstrumentState {
    fn default() -> Self {
        Self {
            pool: Vec::new(),
            pad_assignments: [None; PAD_COUNT],
            source_path: None,
            source_cursor_sample: 0,
            source_sample_rate: 44_100.0,
            contextual_filter: 0.0,
            selection: Selection::Waveform,
        }
    }
}

#[derive(Clone)]
pub(crate) struct LoadedSource {
    pub(crate) path: PathBuf,
    pub(crate) sample_rate: f32,
    pub(crate) channels: Vec<Vec<f32>>,
}

impl LoadedSource {
    pub(crate) fn len_samples(&self) -> usize {
        self.channels.iter().map(Vec::len).max().unwrap_or(0)
    }

    pub(crate) fn duration_seconds(&self) -> f32 {
        self.len_samples() as f32 / self.sample_rate.max(1.0)
    }
}

#[derive(Default)]
pub(crate) struct EditorRuntime {
    pub(crate) source: Option<LoadedSource>,
    pub(crate) file_error: Option<String>,
    pub(crate) file_status: Option<String>,
    pub(crate) pending_source_rx: Option<mpsc::Receiver<Option<Result<LoadedSource, String>>>>,
    pub(crate) audition_enabled: bool,
    pub(crate) audition_item: Option<CapturedFreeze>,
    pub(crate) audition_revision: u64,
    pub(crate) mouse_pad_gates: [bool; PAD_COUNT],
    pub(crate) drag_pool_item: Option<usize>,
}

pub(crate) struct PadActivityAtomics {
    pads: [AtomicBool; PAD_COUNT],
}

impl Default for PadActivityAtomics {
    fn default() -> Self {
        Self {
            pads: std::array::from_fn(|_| AtomicBool::new(false)),
        }
    }
}

impl PadActivityAtomics {
    pub(crate) fn store(&self, active: [bool; PAD_COUNT]) {
        for (atom, value) in self.pads.iter().zip(active) {
            atom.store(value, Ordering::Relaxed);
        }
    }

    pub(crate) fn load(&self) -> [bool; PAD_COUNT] {
        std::array::from_fn(|i| self.pads[i].load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_activity_round_trips_all_pads() {
        let activity = PadActivityAtomics::default();
        let active = std::array::from_fn(|idx| idx % 3 == 0);

        activity.store(active);

        assert_eq!(activity.load(), active);
    }

    #[test]
    fn loaded_source_reports_length_and_duration() {
        let source = LoadedSource {
            path: PathBuf::from("example.wav"),
            sample_rate: 4.0,
            channels: vec![vec![0.0; 4], vec![0.0; 6]],
        };

        assert_eq!(source.len_samples(), 6);
        assert_eq!(source.duration_seconds(), 1.5);
    }
}
