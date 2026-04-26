# Spectral Freeze Rust Port

Rust port of the JUCE `../spectral-freeze` plugin.

## Workspace

- `dsp/` — pure host-agnostic DSP and the single parameter manifest.
- `clap-shell/` — `nih-plug` CLAP/VST3 wrapper with an egui GUI matching the original layout.
- `wam-shell/` — headless WAM-oriented `wasm32-unknown-unknown` `cdylib` plus a thin JS AudioWorklet layer.

## Build and verify

```sh
cargo test --workspace
cargo build -p spectral-freeze-wam --target wasm32-unknown-unknown --release
cargo nih-plug bundle spectral-freeze-clap --release
```

Outputs:

- CLAP: `target/bundled/spectral-freeze-clap.clap`
- VST3: `target/bundled/spectral-freeze-clap.vst3`
- WAM wasm: `target/wasm32-unknown-unknown/release/spectral_freeze_wam.wasm`

The WAM is intentionally headless: `wam-shell/js/SpectralFreezeWamNode.js` exposes `getParameterInfo()` and does not implement `createGui()`. Parameter updates are written to a `SharedArrayBuffer` ring buffer and drained by the AudioWorklet processor.

A minimal browser smoke page is in `examples/wam-test/index.html`.
