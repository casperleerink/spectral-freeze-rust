# Mono Spectral-State Instrument

Spectral Freeze is now a monophonic spectral-state instrument. Captured freezes are spectral attractors, not independent voices.

## Model

- The instrument owns one persistent spectral body.
- MIDI pads select a target freeze from the existing 16 pad assignments.
- Note-on changes the current target and opens a simple smoothed gate.
- Note-off closes the gate when no pads remain held, with sustain pedal support.
- The current magnitude and phase-advance arrays glide toward the target freeze per FFT hop.
- Phase remains continuous across target changes.

## Parameters

- **Mag Glide**: seconds for current magnitudes to move toward the selected target.
- **Phase Glide**: seconds for per-bin phase advance to move toward the selected target.
- **Organic**: integrated directly into the mono spectral-state render hop.

ADSR controls were intentionally removed. Amplitude uses fixed short attack smoothing and fixed release smoothing so the first prototype centers spectral motion rather than envelope design.

## Organic placement

Organic now acts before the instrument iFFT:

1. The mono engine updates `current_mag` and `current_phase_advance` toward the selected target.
2. It advances phase continuously.
3. Organic applies banded magnitude instability, phase instability, spectral smoothing, and residual/noise energy directly to that spectrum.
4. The result is mirrored, inverse-FFT'd, lightly saturated, windowed, and overlap-added.

This keeps the graph to one spectral synthesis stage instead of rendering to audio and then re-analysing it with a post-FFT effect.

## Preserved source material

The existing captured freeze format remains the spectral source format:

- target magnitudes,
- stored phase,
- measured phase advance,
- per-item filter threshold.

The filter threshold is applied to target magnitudes before glide smoothing, so filtered bins fade in or out through the mono spectral state.
