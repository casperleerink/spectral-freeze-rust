# Spectral Freeze Rust Port

Rust port of the JUCE [spectral-freeze](https://github.com/casperleerink/spectral-freeze) plugin.

## Workspace

- `dsp/` — pure host-agnostic DSP and the single parameter manifest.
- `clap-shell/` — `nih-plug` CLAP/VST3 wrapper with an egui GUI matching the original layout.
- `wam-shell/` — headless WAM-oriented `wasm32-unknown-unknown` `cdylib` plus a thin JS AudioWorklet layer.

## Build and verify

```sh
cargo test --workspace
cargo build -p spectral-freeze-wam --target wasm32-unknown-unknown --release
cargo run -p xtask -- bundle spectral-freeze-clap --release
```

Outputs:

- CLAP: `target/bundled/spectral-freeze-clap.clap`
- VST3: `target/bundled/spectral-freeze-clap.vst3`
- WAM wasm: `target/wasm32-unknown-unknown/release/spectral_freeze_wam.wasm`

The WAM is intentionally headless: `wam-shell/js/SpectralFreezeWamNode.js` exposes `getParameterInfo()` and does not implement `createGui()`. Parameter updates are written to a `SharedArrayBuffer` ring buffer and drained by the AudioWorklet processor.

A minimal browser smoke page is in `examples/wam-test/index.html`.

## Versioning and releases

All shipped formats use the same semver version:

- Rust workspace package version in `Cargo.toml`
- WAM npm version in `wam-shell/package.json`
- WAM descriptor version in `wam-shell/descriptor.json`

Bump them together with:

```sh
node scripts/set-version.mjs X.Y.Z
```

Then verify and tag:

```sh
cargo test -p dsp -p spectral-freeze-wam
cargo run -p xtask -- bundle spectral-freeze-clap --release
cd wam-shell && npm run build && npm pack --dry-run

git commit -am "Release vX.Y.Z"
git tag vX.Y.Z
git push origin main vX.Y.Z
```

Release automation:

- `.github/workflows/npm-release.yml` builds and packs the WAM on pushes/PRs, then publishes to npm and attaches the tarball on `v*` tags.
- `.github/workflows/plugin-release.yml` builds CLAP and VST3 bundles for macOS universal, Windows x64, and Linux x64, uploads CI artifacts, and attaches zip files on `v*` tags.

npm publishing uses npm Trusted Publishing, so no `NPM_TOKEN` secret is required. Configure the npm package's trusted publisher for repository `casperleerink/spectral-freeze-rust` and workflow `.github/workflows/npm-release.yml`.

## Release notes

### WAM npm package 0.2.2

- Add TypeScript declarations for the public WAM API.
- Add JSDoc/checkJs type checking for the JavaScript WAM wrapper.

### WAM npm package 0.2.1

- Verify npm release automation for `spectral-freeze-wam`.
