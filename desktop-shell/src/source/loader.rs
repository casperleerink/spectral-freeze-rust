use super::wav::load_wav;
use crate::state::{EditorRuntime, InstrumentState, LoadedSource, Selection};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;

pub(crate) fn poll(runtime: &mut EditorRuntime, state: &mut InstrumentState) {
    let pending = runtime
        .pending_source_rx
        .as_ref()
        .and_then(|rx| match rx.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Some(Err(
                "WAV loader stopped before returning a file".to_string(),
            ))),
        });

    if let Some(result) = pending {
        runtime.pending_source_rx = None;
        runtime.file_status = None;
        match result {
            Some(Ok(source)) => apply_loaded_source(runtime, state, source),
            Some(Err(err)) => runtime.file_error = Some(err),
            None => {}
        }
    }
}

fn apply_loaded_source(
    runtime: &mut EditorRuntime,
    state: &mut InstrumentState,
    source: LoadedSource,
) {
    state.source_path = Some(source.path.to_string_lossy().to_string());
    state.source_sample_rate = source.sample_rate;
    state.source_cursor_sample = 0;
    state.selection = Selection::Waveform;
    runtime.source = Some(source);
    runtime.file_error = None;
    runtime.audition_revision = runtime.audition_revision.wrapping_add(1);
}

pub(crate) fn open_dialog(runtime: &mut EditorRuntime) {
    if runtime.pending_source_rx.is_some() {
        return;
    }

    let (tx, rx) = mpsc::channel();
    runtime.pending_source_rx = Some(rx);
    runtime.file_error = None;
    runtime.file_status = Some("Opening WAV picker…".to_string());

    thread::spawn(move || {
        let picked = pollster::block_on(
            rfd::AsyncFileDialog::new()
                .set_title("Load WAV")
                .add_filter("WAV audio", &["wav"])
                .pick_file(),
        );
        let result = picked.map(|handle| {
            let path = handle.path().to_owned();
            load_wav(&path)
        });
        let _ = tx.send(result);
    });
}

pub(crate) fn load_path(runtime: &mut EditorRuntime, path: PathBuf) {
    if runtime.pending_source_rx.is_some() {
        runtime.file_error = Some("Already loading a WAV file".to_string());
        return;
    }

    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("WAV file")
        .to_string();
    let (tx, rx) = mpsc::channel();
    runtime.pending_source_rx = Some(rx);
    runtime.file_error = None;
    runtime.file_status = Some(format!("Loading {label}…"));

    thread::spawn(move || {
        let result = load_wav(&path);
        let _ = tx.send(Some(result));
    });
}

pub(crate) fn is_wav_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_wav_path_is_case_insensitive() {
        assert!(is_wav_path(Path::new("loop.wav")));
        assert!(is_wav_path(Path::new("LOOP.WAV")));
        assert!(!is_wav_path(Path::new("loop.aif")));
        assert!(!is_wav_path(Path::new("loop")));
    }
}
