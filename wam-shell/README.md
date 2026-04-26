# spectral-freeze-wam

Headless Web Audio Module build of Spectral Freeze.

This package ships:

- `dist/spectral_freeze_wam.wasm` — Rust DSP compiled to `wasm32-unknown-unknown`
- `dist/SpectralFreezeWamProcessor.js` — AudioWorklet processor that loads the wasm directly
- `dist/SpectralFreezeWamNode.js` — main-thread node wrapper
- `dist/index.js` — package entry point

The WAM is intentionally headless: hosts should call `getParameterInfo()` and render controls themselves. There is no `createGui()`.

## Usage

```js
import SpectralFreezeWam from "spectral-freeze-wam";

const audioContext = new AudioContext();
const node = await SpectralFreezeWam.create(audioContext);

source.connect(node).connect(audioContext.destination);

node.setParameterValue("freeze", 1);
node.setParameterValue("organic", 0.5);

console.log(node.getParameterInfo());
```

If your bundler needs explicit asset URLs:

```js
const node = await SpectralFreezeWam.create(audioContext, {
  wasmUrl: new URL("spectral-freeze-wam/dist/spectral_freeze_wam.wasm", import.meta.url),
  processorUrl: new URL("spectral-freeze-wam/dist/SpectralFreezeWamProcessor.js", import.meta.url),
});
```

## SharedArrayBuffer requirement

Parameter updates use a `SharedArrayBuffer` ring buffer, not `postMessage`, so browser pages generally need cross-origin isolation headers:

```http
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```
