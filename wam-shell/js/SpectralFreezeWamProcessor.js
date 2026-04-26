/* global AudioWorkletProcessor, registerProcessor, currentFrame, sampleRate */

const RING_HEADER_I32S = 2;
const RING_READ = 0;
const RING_WRITE = 1;
const EVENT_U32S = 2; // [parameterIndex, f32ValueBits]

class SpectralFreezeWamProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();

    const opts = options.processorOptions ?? {};
    if (!opts.wasmBytes) throw new Error("SpectralFreezeWamProcessor requires wasmBytes");

    const module = new WebAssembly.Module(opts.wasmBytes);
    const instance = new WebAssembly.Instance(module, {});
    this.exports = instance.exports;
    this.memory = this.exports.memory;

    this.mainChannels = Math.max(1, Math.min(2, opts.mainChannels ?? 2));
    this.sidechainChannels = Math.max(0, Math.min(2, opts.sidechainChannels ?? 0));
    this.maxBlock = opts.maxBlock ?? 128;
    this.frames = 128;

    this.processor = this.exports.sf_create(sampleRate, this.mainChannels, this.sidechainChannels, this.maxBlock);
    this.inputPtr = this.exports.sf_alloc_f32(this.mainChannels * this.maxBlock);
    this.outputPtr = this.exports.sf_alloc_f32(this.mainChannels * this.maxBlock);
    this.sidechainPtr = this.sidechainChannels > 0
      ? this.exports.sf_alloc_f32(this.sidechainChannels * this.maxBlock)
      : 0;

    this.inputHeap = new Float32Array(this.memory.buffer, this.inputPtr, this.mainChannels * this.maxBlock);
    this.outputHeap = new Float32Array(this.memory.buffer, this.outputPtr, this.mainChannels * this.maxBlock);
    this.sidechainHeap = this.sidechainPtr
      ? new Float32Array(this.memory.buffer, this.sidechainPtr, this.sidechainChannels * this.maxBlock)
      : null;
    this.silence = new Float32Array(this.maxBlock);

    this.paramRing = opts.parameterRing ?? null;
    if (this.paramRing) {
      this.ringI32 = new Int32Array(this.paramRing);
      this.ringU32 = new Uint32Array(this.paramRing);
      this.ringCapacity = (this.ringU32.length - RING_HEADER_I32S) / EVENT_U32S;
      this.valueBits = new Uint32Array(1);
      this.valueFloat = new Float32Array(this.valueBits.buffer);
    }

    this.port.onmessage = (event) => {
      // Non-realtime setup/teardown only. Parameter automation deliberately does
      // not use postMessage; it goes through the SAB ring drained in process().
      if (event.data?.type === "reset") this.exports.sf_reset(this.processor);
    };
  }

  drainParameterRing() {
    if (!this.paramRing) return;
    let read = Atomics.load(this.ringI32, RING_READ);
    const write = Atomics.load(this.ringI32, RING_WRITE);
    while (read !== write) {
      const slot = RING_HEADER_I32S + read * EVENT_U32S;
      const parameterIndex = this.ringU32[slot];
      this.valueBits[0] = this.ringU32[slot + 1];
      this.exports.sf_set_parameter(this.processor, parameterIndex, this.valueFloat[0]);
      read = (read + 1) % this.ringCapacity;
      Atomics.store(this.ringI32, RING_READ, read);
    }
  }

  copyInput(input, heap, channels, frames) {
    for (let ch = 0; ch < channels; ch += 1) {
      const src = input?.[ch] ?? this.silence;
      const base = ch * this.maxBlock;
      for (let i = 0; i < frames; i += 1) heap[base + i] = src[i] ?? 0;
    }
  }

  copyOutput(output, frames) {
    for (let ch = 0; ch < this.mainChannels; ch += 1) {
      const dst = output?.[ch];
      if (!dst) continue;
      const base = ch * this.maxBlock;
      for (let i = 0; i < frames; i += 1) dst[i] = this.outputHeap[base + i];
    }
  }

  process(inputs, outputs) {
    const output = outputs[0];
    const frames = output?.[0]?.length ?? 128;
    if (frames > this.maxBlock) return true;

    this.drainParameterRing();
    this.copyInput(inputs[0], this.inputHeap, this.mainChannels, frames);
    if (this.sidechainHeap) this.copyInput(inputs[1], this.sidechainHeap, this.sidechainChannels, frames);

    this.exports.sf_process(
      this.processor,
      this.inputPtr,
      this.outputPtr,
      this.sidechainHeap ? this.sidechainPtr : 0,
      frames,
    );
    this.copyOutput(output, frames);
    return true;
  }
}

registerProcessor("com.learning.spectral-freeze", SpectralFreezeWamProcessor);
