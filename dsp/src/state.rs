use crate::constants::*;
use crate::random::JuceRandom;
use rustfft::num_complex::Complex32;

pub(crate) struct FreezeState {
    pub(crate) frozen_mag: [f32; NUM_BINS],
    pub(crate) frozen_phase: [f32; NUM_BINS],
    pub(crate) frozen_phase_advance: [f32; NUM_BINS],
    pub(crate) last_analysis_phase: [f32; NUM_BINS],
    pub(crate) smoothed_phase_advance: [f32; NUM_BINS],
    pub(crate) has_last_analysis_phase: bool,
    pub(crate) mag_history: [[f32; NUM_BINS]; MAG_HISTORY_SIZE],
    pub(crate) mag_history_write: usize,
    pub(crate) mag_history_count: usize,
    pub(crate) was_frozen: bool,
    pub(crate) has_frozen_frame: bool,
}

impl Default for FreezeState {
    fn default() -> Self {
        Self {
            frozen_mag: [0.0; NUM_BINS],
            frozen_phase: [0.0; NUM_BINS],
            frozen_phase_advance: [0.0; NUM_BINS],
            last_analysis_phase: [0.0; NUM_BINS],
            smoothed_phase_advance: [0.0; NUM_BINS],
            has_last_analysis_phase: false,
            mag_history: [[0.0; NUM_BINS]; MAG_HISTORY_SIZE],
            mag_history_write: 0,
            mag_history_count: 0,
            was_frozen: false,
            has_frozen_frame: false,
        }
    }
}

pub(crate) struct OrganicAmState {
    pub(crate) value: [f32; ORGANIC_AM_BANDS],
    pub(crate) target: [f32; ORGANIC_AM_BANDS],
    pub(crate) hop_counter: usize,
}

impl Default for OrganicAmState {
    fn default() -> Self {
        Self {
            value: [0.0; ORGANIC_AM_BANDS],
            target: [0.0; ORGANIC_AM_BANDS],
            hop_counter: 0,
        }
    }
}

pub(crate) struct StftChannelState {
    pub(crate) input_fifo: Box<[f32; FFT_SIZE]>,
    pub(crate) output_fifo: Box<[f32; FFT_SIZE]>,
    pub(crate) spectrum: Box<[Complex32; FFT_SIZE]>,
    pub(crate) fifo_pos: usize,
    pub(crate) samples_seen: usize,
}

impl StftChannelState {
    pub(crate) fn new() -> Self {
        Self {
            input_fifo: Box::new([0.0; FFT_SIZE]),
            output_fifo: Box::new([0.0; FFT_SIZE]),
            spectrum: Box::new([Complex32::new(0.0, 0.0); FFT_SIZE]),
            fifo_pos: 0,
            samples_seen: 0,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.input_fifo.fill(0.0);
        self.output_fifo.fill(0.0);
        self.spectrum.fill(Complex32::new(0.0, 0.0));
        self.fifo_pos = 0;
        self.samples_seen = 0;
    }

    #[inline]
    pub(crate) fn push_sample_and_pop_output(&mut self, input: f32) -> f32 {
        self.input_fifo[self.fifo_pos] = input;
        let output = self.output_fifo[self.fifo_pos];
        self.output_fifo[self.fifo_pos] = 0.0;
        self.fifo_pos = (self.fifo_pos + 1) % FFT_SIZE;
        if self.samples_seen < FFT_SIZE {
            self.samples_seen += 1;
        }
        output
    }

    pub(crate) fn copy_input_frame_to_spectrum(&mut self) {
        for i in 0..FFT_SIZE {
            self.spectrum[i] = Complex32::new(self.input_fifo[(self.fifo_pos + i) % FFT_SIZE], 0.0);
        }
    }

    pub(crate) fn overlap_add_scratch_to_output(&mut self) {
        for i in 0..FFT_SIZE {
            self.output_fifo[(self.fifo_pos + i) % FFT_SIZE] += self.spectrum[i].re;
        }
    }
}

pub(crate) struct ChannelState {
    pub(crate) stft: StftChannelState,
    pub(crate) freeze: FreezeState,
    pub(crate) organic_am: OrganicAmState,
    pub(crate) rng: JuceRandom,
}

impl ChannelState {
    pub(crate) fn new(seed: u32) -> Self {
        let mut rng = JuceRandom::new(seed as u64);
        let mut organic_am = OrganicAmState::default();
        for target in &mut organic_am.target {
            *target = rng.bipolar();
        }
        Self {
            stft: StftChannelState::new(),
            freeze: FreezeState::default(),
            organic_am,
            rng,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.stft.reset();
        self.freeze = FreezeState::default();
        self.organic_am.value.fill(0.0);
        self.organic_am.hop_counter = 0;
        for target in &mut self.organic_am.target {
            *target = self.rng.bipolar();
        }
    }
}

pub(crate) struct SidechainState {
    pub(crate) input_fifo: Box<[f32; FFT_SIZE]>,
    pub(crate) spectrum: Box<[Complex32; FFT_SIZE]>,
    pub(crate) fifo_pos: usize,
}

impl SidechainState {
    pub(crate) fn new() -> Self {
        Self {
            input_fifo: Box::new([0.0; FFT_SIZE]),
            spectrum: Box::new([Complex32::new(0.0, 0.0); FFT_SIZE]),
            fifo_pos: 0,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.input_fifo.fill(0.0);
        self.spectrum.fill(Complex32::new(0.0, 0.0));
        self.fifo_pos = 0;
    }
}
