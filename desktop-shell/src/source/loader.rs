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

pub(crate) fn open_dialog(runtime: &mut EditorRuntime, parent_ns_view: Option<usize>) {
    if runtime.pending_source_rx.is_some() {
        return;
    }

    let (tx, rx) = mpsc::channel();
    runtime.pending_source_rx = Some(rx);
    runtime.file_error = None;
    runtime.file_status = Some("Opening WAV picker…".to_string());

    #[cfg(not(target_os = "macos"))]
    let _ = parent_ns_view;

    #[allow(unused_mut)]
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title("Load WAV")
        .add_filter("WAV audio", &["wav"]);
    // Without a parent, rfd attaches the panel to the host's main window
    // (e.g. Ableton's), which puts it behind the floating plugin window.
    // Parenting it to our own view opens it as a sheet on the plugin window.
    #[cfg(target_os = "macos")]
    if let Some(parent) = parent_ns_view.and_then(macos::ParentWindow::new) {
        dialog = dialog.set_parent(&parent);
    }
    // Create the future here on the GUI thread: on macOS this is the AppKit
    // main thread, and rfd resolves the parent view into an NSWindow while
    // building the future. Doing that on a worker thread would touch AppKit
    // off-main and could race an editor teardown. The worker only polls.
    let future = dialog.pick_file();

    thread::spawn(move || {
        let picked = pollster::block_on(future);
        let result = picked.map(|handle| {
            let path = handle.path().to_owned();
            load_wav(&path)
        });
        let _ = tx.send(result);
    });
}

#[cfg(target_os = "macos")]
mod macos {
    use raw_window_handle::{
        AppKitDisplayHandle, AppKitWindowHandle, DisplayHandle, HandleError, HasDisplayHandle,
        HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle,
    };
    use std::ffi::c_void;
    use std::ptr::NonNull;

    pub(super) struct ParentWindow(NonNull<c_void>);

    impl ParentWindow {
        pub(super) fn new(ns_view: usize) -> Option<Self> {
            NonNull::new(ns_view as *mut c_void).map(Self)
        }
    }

    impl HasWindowHandle for ParentWindow {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let handle = RawWindowHandle::AppKit(AppKitWindowHandle::new(self.0));
            Ok(unsafe { WindowHandle::borrow_raw(handle) })
        }
    }

    impl HasDisplayHandle for ParentWindow {
        fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
            let handle = RawDisplayHandle::AppKit(AppKitDisplayHandle::new());
            Ok(unsafe { DisplayHandle::borrow_raw(handle) })
        }
    }
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
