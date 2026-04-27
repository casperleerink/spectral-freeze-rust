export type SpectralFreezeParameterId =
  | "freeze"
  | "filter"
  | "scBoost"
  | "scFreqSmoothing"
  | "organic";

export interface SpectralFreezeParameterInfo {
  id: SpectralFreezeParameterId | string;
  name: string;
  kind: "bool" | "float";
  min: number;
  max: number;
  default: number;
  unit: string;
}

export interface SpectralFreezeDescriptor {
  identifier: string;
  name: string;
  vendor: string;
  version: string;
  apiVersion: string;
  isInstrument: boolean;
  hasAudioInput: boolean;
  hasAudioOutput: boolean;
  keywords: string[];
}

export interface SpectralFreezeWamCreateOptions {
  wasmUrl?: string | URL | Request;
  processorUrl?: string | URL;
  parameterInfo?: SpectralFreezeParameterInfo[];
  parameterRingCapacity?: number;
  mainChannels?: number;
  sidechainChannels?: number;
  maxBlock?: number;
}

export interface SpectralFreezeWamNodeOptions {
  wasmBytes: ArrayBuffer;
  parameterRing: SharedArrayBuffer;
  parameterInfo: SpectralFreezeParameterInfo[];
  mainChannels?: number;
  sidechainChannels?: number;
  maxBlock?: number;
}

export declare const descriptor: SpectralFreezeDescriptor;

export declare class SpectralFreezeWamNode extends AudioWorkletNode {
  readonly parameterInfo: SpectralFreezeParameterInfo[];
  readonly parameterRing: SharedArrayBuffer;

  constructor(audioContext: BaseAudioContext, options: SpectralFreezeWamNodeOptions);

  getParameterInfo(): SpectralFreezeParameterInfo[];

  setParameterValue(parameterIdOrIndex: SpectralFreezeParameterId | string | number, value: number): void;
}

export declare class SpectralFreezeWam {
  static descriptor: SpectralFreezeDescriptor;

  static create(
    audioContext: BaseAudioContext,
    options?: SpectralFreezeWamCreateOptions,
  ): Promise<SpectralFreezeWamNode>;
}

export default SpectralFreezeWam;
