const PROCESSOR_NAME = "com.learning.spectral-freeze";
const RING_HEADER_I32S = 2;
const RING_READ = 0;
const RING_WRITE = 1;
const EVENT_U32S = 2;

export const descriptor = {
  identifier: PROCESSOR_NAME,
  name: "Spectral Freeze",
  vendor: "Learning",
  version: "0.2.0",
  apiVersion: "2.0.0",
  isInstrument: false,
  hasAudioInput: true,
  hasAudioOutput: true,
  keywords: ["spectral", "freeze", "stft", "effect"],
};

export class SpectralFreezeWamNode extends AudioWorkletNode {
  constructor(audioContext, options) {
    const { wasmBytes, parameterRing, mainChannels = 2, sidechainChannels = 0, maxBlock = 128 } = options;
    super(audioContext, PROCESSOR_NAME, {
      numberOfInputs: sidechainChannels > 0 ? 2 : 1,
      numberOfOutputs: 1,
      outputChannelCount: [mainChannels],
      processorOptions: { wasmBytes, parameterRing, mainChannels, sidechainChannels, maxBlock },
    });

    this.parameterInfo = options.parameterInfo;
    this.parameterRing = parameterRing;
    this.ringI32 = new Int32Array(parameterRing);
    this.ringU32 = new Uint32Array(parameterRing);
    this.ringCapacity = (this.ringU32.length - RING_HEADER_I32S) / EVENT_U32S;
    this.valueFloat = new Float32Array(1);
    this.valueBits = new Uint32Array(this.valueFloat.buffer);
  }

  getParameterInfo() {
    return this.parameterInfo;
  }

  setParameterValue(parameterIdOrIndex, value) {
    const index = typeof parameterIdOrIndex === "number"
      ? parameterIdOrIndex
      : this.parameterInfo.findIndex((parameter) => parameter.id === parameterIdOrIndex);
    if (index < 0) throw new Error(`Unknown Spectral Freeze parameter: ${parameterIdOrIndex}`);

    let read = Atomics.load(this.ringI32, RING_READ);
    let write = Atomics.load(this.ringI32, RING_WRITE);
    const next = (write + 1) % this.ringCapacity;
    if (next === read) {
      // Drop the oldest parameter event instead of using postMessage on the
      // realtime path. Hosts normally write at UI rates so this is exceptional.
      read = (read + 1) % this.ringCapacity;
      Atomics.store(this.ringI32, RING_READ, read);
    }

    this.valueFloat[0] = value;
    const slot = RING_HEADER_I32S + write * EVENT_U32S;
    this.ringU32[slot] = index;
    this.ringU32[slot + 1] = this.valueBits[0];
    Atomics.store(this.ringI32, RING_WRITE, next);
  }
}

export class SpectralFreezeWam {
  static descriptor = descriptor;

  static async create(audioContext, options = {}) {
    const wasmUrl = options.wasmUrl ?? new URL("../../target/wasm32-unknown-unknown/release/spectral_freeze_wam.wasm", import.meta.url);
    const processorUrl = options.processorUrl ?? new URL("./SpectralFreezeWamProcessor.js", import.meta.url);
    const wasmBytes = await fetch(wasmUrl).then((response) => {
      if (!response.ok) throw new Error(`Failed to fetch ${wasmUrl}: ${response.status}`);
      return response.arrayBuffer();
    });

    await audioContext.audioWorklet.addModule(processorUrl);

    const parameterInfo = options.parameterInfo ?? await readParameterInfoFromWasm(wasmBytes);
    const parameterRing = createParameterRing(options.parameterRingCapacity ?? 1024);

    const node = new SpectralFreezeWamNode(audioContext, {
      wasmBytes,
      parameterRing,
      parameterInfo,
      mainChannels: options.mainChannels ?? 2,
      sidechainChannels: options.sidechainChannels ?? 0,
      maxBlock: options.maxBlock ?? 128,
    });

    for (const [index, parameter] of parameterInfo.entries()) {
      node.setParameterValue(index, parameter.default);
    }

    return node;
  }

  // WAM hosts use the manifest to render their own controls. This module is
  // intentionally headless: there is no createGui() method.
}

function createParameterRing(capacity) {
  const slots = RING_HEADER_I32S + capacity * EVENT_U32S;
  return new SharedArrayBuffer(slots * Uint32Array.BYTES_PER_ELEMENT);
}

async function readParameterInfoFromWasm(wasmBytes) {
  const { instance } = await WebAssembly.instantiate(wasmBytes, {});
  const { memory, sf_parameter_manifest_ptr, sf_parameter_manifest_len } = instance.exports;
  const ptr = sf_parameter_manifest_ptr();
  const len = sf_parameter_manifest_len();
  const bytes = new Uint8Array(memory.buffer, ptr, len);
  return JSON.parse(new TextDecoder().decode(bytes));
}

export default SpectralFreezeWam;
