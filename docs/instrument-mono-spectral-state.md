# Mono Spectral-State Instrument

Spectral Freeze is a playable monophonic spectral instrument. Captured freezes are spectral targets for one persistent spectral body, not independent voices.

## Interaction model

The instrument uses a newest-held-note priority stack.

- A note-on from silence resolves the pad assignment, seeds the spectral state directly from that freeze, clears stale overlap-add output, and opens the gate with **Attack**.
- A legato note-on pushes a new target onto the stack, keeps the gate open, and morphs the existing spectral state toward the new freeze with **Glide**.
- Releasing the newest note removes it from the stack. If older notes remain held or sustained, the newest remaining note becomes the target and the spectral state glides back to it.
- Releasing the final note closes the amplitude gate with **Release**. Spectral magnitudes are not forced to glide to zero.
- A later detached note after silence seeds directly from its target freeze, so old spectral material does not create unexpected morphs after a pause.

Sustain pedal support preserves released notes while the pedal is down. Pedal-up removes notes that are no longer physically held, then either returns to the newest remaining note or closes the gate.

## Parameters

- **Attack**: amplitude gate opening time, in seconds.
- **Release**: amplitude gate closing time, in seconds.
- **Glide**: legato spectral morph time, in seconds. Internally this drives both magnitude and phase-advance smoothing.
- **Organic**: integrated directly into the mono spectral-state render hop.

Glide is only a legato transition control. Fresh detached notes seed immediately from their assigned freeze instead of gliding from silence or stale material.

## Organic placement

Organic acts before the instrument iFFT:

1. The mono engine updates `current_mag` and `current_phase_advance` toward the active target.
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
