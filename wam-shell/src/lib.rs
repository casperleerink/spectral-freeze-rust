//! Headless Web Audio Module (WAM) DSP binary.
//!
//! This crate intentionally uses plain `extern "C"` exports. There is no
//! `wasm-bindgen` boundary on the audio thread; the AudioWorklet loads the
//! `.wasm` with `WebAssembly.instantiate()` and calls these functions directly.

use dsp::{PARAMETER_MANIFEST_JSON, PARAMS, ProcessParams, SpectralFreeze};
use std::slice;

const MAX_CHANNELS: usize = 2;
const DEFAULT_MAX_BLOCK: usize = 128;

pub struct WasmProcessor {
    dsp: SpectralFreeze,
    params: [f32; 3],
    main_channels: usize,
    max_block: usize,
    main_buffers: [Vec<f32>; MAX_CHANNELS],
}

impl WasmProcessor {
    fn new(sample_rate: f32, main_channels: usize, max_block: usize) -> Self {
        let main_channels = main_channels.clamp(1, MAX_CHANNELS);
        let max_block = max_block.max(DEFAULT_MAX_BLOCK);
        let mut dsp = SpectralFreeze::default();
        dsp.prepare(sample_rate, main_channels);
        Self {
            dsp,
            params: PARAMS.map(|p| p.default),
            main_channels,
            max_block,
            main_buffers: std::array::from_fn(|_| vec![0.0; max_block]),
        }
    }

    fn process(&mut self, input_ptr: *const f32, output_ptr: *mut f32, frames: usize) -> i32 {
        if input_ptr.is_null() || output_ptr.is_null() || frames > self.max_block {
            return 0;
        }

        let input = unsafe { slice::from_raw_parts(input_ptr, self.main_channels * frames) };
        let output = unsafe { slice::from_raw_parts_mut(output_ptr, self.main_channels * frames) };

        for ch in 0..self.main_channels {
            let src = &input[ch * frames..(ch + 1) * frames];
            self.main_buffers[ch][..frames].copy_from_slice(src);
        }

        let params = ProcessParams::from_values(self.params);

        match self.main_channels {
            1 => {
                let mut main: [&mut [f32]; 1] = [&mut self.main_buffers[0][..frames]];
                self.dsp.process_block(&mut main, params);
            }
            2 => {
                let (left, rest) = self.main_buffers.split_at_mut(1);
                let mut main: [&mut [f32]; 2] = [&mut left[0][..frames], &mut rest[0][..frames]];
                self.dsp.process_block(&mut main, params);
            }
            _ => return 0,
        }

        for ch in 0..self.main_channels {
            let dst = &mut output[ch * frames..(ch + 1) * frames];
            dst.copy_from_slice(&self.main_buffers[ch][..frames]);
        }

        1
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn sf_create(
    sample_rate: f32,
    main_channels: u32,
    max_block: u32,
) -> *mut WasmProcessor {
    Box::into_raw(Box::new(WasmProcessor::new(
        sample_rate,
        main_channels as usize,
        max_block as usize,
    )))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sf_destroy(ptr: *mut WasmProcessor) {
    if !ptr.is_null() {
        unsafe {
            drop(Box::from_raw(ptr));
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sf_reset(ptr: *mut WasmProcessor) {
    if let Some(processor) = unsafe { ptr.as_mut() } {
        processor.dsp.reset();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sf_set_parameter(ptr: *mut WasmProcessor, index: u32, value: f32) -> i32 {
    let Some(processor) = (unsafe { ptr.as_mut() }) else {
        return 0;
    };
    let index = index as usize;
    if index >= processor.params.len() {
        return 0;
    }
    let info = PARAMS[index];
    processor.params[index] = value.clamp(info.min, info.max);
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sf_get_parameter(ptr: *mut WasmProcessor, index: u32) -> f32 {
    let Some(processor) = (unsafe { ptr.as_ref() }) else {
        return 0.0;
    };
    processor.params.get(index as usize).copied().unwrap_or(0.0)
}

/// Process planar f32 buffers laid out as `[channel][frame]`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sf_process(
    ptr: *mut WasmProcessor,
    input_ptr: *const f32,
    output_ptr: *mut f32,
    frames: u32,
) -> i32 {
    let Some(processor) = (unsafe { ptr.as_mut() }) else {
        return 0;
    };
    processor.process(input_ptr, output_ptr, frames as usize)
}

#[unsafe(no_mangle)]
pub extern "C" fn sf_parameter_count() -> u32 {
    PARAMS.len() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn sf_parameter_manifest_ptr() -> *const u8 {
    PARAMETER_MANIFEST_JSON.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn sf_parameter_manifest_len() -> usize {
    PARAMETER_MANIFEST_JSON.len()
}

#[unsafe(no_mangle)]
pub extern "C" fn sf_latency_samples() -> u32 {
    dsp::LATENCY_SAMPLES
}

#[unsafe(no_mangle)]
pub extern "C" fn sf_max_block_size(ptr: *const WasmProcessor) -> u32 {
    if let Some(processor) = unsafe { ptr.as_ref() } {
        processor.max_block as u32
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn sf_alloc_f32(len: usize) -> *mut f32 {
    let mut buffer = Vec::<f32>::with_capacity(len);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sf_free_f32(ptr: *mut f32, len: usize) {
    if !ptr.is_null() {
        unsafe {
            drop(Vec::from_raw_parts(ptr, 0, len));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_processor_processes_silence() {
        let mut processor = WasmProcessor::new(48_000.0, 2, 128);
        let input = vec![0.0_f32; 256];
        let mut output = vec![1.0_f32; 256];
        assert_eq!(
            processor.process(input.as_ptr(), output.as_mut_ptr(), 128),
            1
        );
        assert!(output.iter().all(|x| *x == 0.0));
    }
}
