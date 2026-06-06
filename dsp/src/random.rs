#[derive(Clone, Copy, Debug)]
pub(crate) struct JuceRandom {
    seed: u64,
}

impl JuceRandom {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            seed: seed & 0x0000_ffff_ffff_ffff,
        }
    }

    #[inline]
    fn next_int(&mut self) -> u32 {
        // Matches juce::Random::nextInt(): a 48-bit LCG with Java-style
        // constants, returning the top 32 bits after the update.
        self.seed = self.seed.wrapping_mul(0x5deece66d).wrapping_add(11) & 0x0000_ffff_ffff_ffff;
        (self.seed >> 16) as u32
    }

    #[inline]
    pub(crate) fn next_float(&mut self) -> f32 {
        let result = self.next_int() as f32 / (u32::MAX as f32 + 1.0);
        result.min(1.0 - f32::EPSILON)
    }

    #[inline]
    pub(crate) fn bipolar(&mut self) -> f32 {
        self.next_float() * 2.0 - 1.0
    }
}
