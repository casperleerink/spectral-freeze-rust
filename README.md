# Spectral Freeze Rust Port

Rust port of the JUCE [spectral-freeze](https://github.com/casperleerink/spectral-freeze) plugin.

## Workspace

- `dsp/` — host-agnostic spectral instrument DSP.
- `desktop-shell/` — `nih-plug` CLAP/VST3/standalone wrapper with an egui instrument UI.

## Build and verify

For fast standalone development with auto rebuild/relaunch:

```sh
brew install watchexec
./scripts/dev-standalone-watch.sh
```

Pass standalone audio options after the script if needed:

```sh
./scripts/dev-standalone-watch.sh --sample-rate 48000 --period-size 512
```

For release/package validation:

```sh
cargo test --workspace
cargo run -p xtask -- bundle spectral-freeze --release
```

Outputs:

- CLAP: `target/bundled/Spectral Freeze.clap`
- VST3: `target/bundled/Spectral Freeze.vst3`
- Standalone: `target/bundled/Spectral Freeze` (or `target/bundled/Spectral Freeze.app` on macOS)

## Versioning and releases

All shipped native formats use the Rust workspace semver version in `Cargo.toml`.

Bump it with:

```sh
node scripts/set-version.mjs X.Y.Z
```

Then verify and tag:

```sh
cargo test -p dsp -p spectral-freeze
cargo run -p xtask -- bundle spectral-freeze --release

git commit -am "Release vX.Y.Z"
git tag vX.Y.Z
git push origin main vX.Y.Z
```

Release automation:

- `.github/workflows/plugin-release.yml` builds CLAP, VST3, and standalone bundles for macOS universal, Windows x64, and Linux x64, uploads CI artifacts, and attaches zip files on `v*` tags.
