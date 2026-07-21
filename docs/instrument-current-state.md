# Spectral Freeze Instrument Current State

Spectral Freeze is a playable monophonic spectral instrument. Users load WAV source material, capture spectral freeze moments into a Freeze Pool, assign those captured freezes to a 16-pad bank, and play the pads from MIDI or mouse interaction.

## Product model

- Native CLAP/VST3/standalone instrument.
- Empty bank by default.
- WAV loading only.
- Source audio files are not embedded in plugin state.
- Captured freeze data is embedded in plugin state, so projects remain playable if source files move.
- Pads reference Freeze Pool items by index.

## Main workflow

```text
Load WAV
→ move waveform cursor
→ optional Audition Monitor
→ Capture
→ Freeze Pool item
→ drag to Pad Grid
→ play pad from MIDI or mouse
→ Mono Spectral Engine output
```

## Captured freeze format

A `CapturedFreeze` stores the spectral source material used by the instrument:

- name,
- source path metadata,
- source sample rate,
- cursor sample/time metadata,
- per-channel magnitude bins,
- per-channel phase bins,
- per-channel measured phase-advance bins.

The source WAV samples themselves are not stored.

## Freeze Pool

The Freeze Pool is the collection of captured spectral moments.

- Capture adds a new item to the pool.
- Pool items can be deleted and dragged onto pads.
- Deleting a pool item clears or reindexes affected pad assignments.

## Pad Grid

The instrument has 16 pads mapped from C1 through D#2.

- Each pad is either empty or assigned to one Freeze Pool item.
- Multiple pads may reference the same pool item.
- MIDI note-on or mouse-down on an assigned pad triggers that pad.
- MIDI note-off or mouse-up releases it.
- Pads show active state from the audio thread.

## Mono spectral-state engine

The audio engine is monophonic at the spectral-body level: captured freezes are spectral targets for one persistent spectral body, not independent layered voices.

The interaction model uses newest-held-note priority:

- A note-on from silence resolves its pad assignment, seeds the spectral state directly from that captured freeze, clears stale overlap-add output, and opens the amplitude gate with Attack.
- A legato note-on pushes a new target onto the held-note stack, keeps the gate open, and morphs the current spectral state toward the new target with Glide.
- Releasing the newest note removes it from the stack. If older notes remain held or sustained, the newest remaining note becomes the target and the spectral state glides back to it.
- Releasing the final note closes the amplitude gate with Release. Spectral magnitudes are not forced to glide to zero.
- A later detached note after silence seeds directly from its target freeze, so stale spectral material does not create unexpected morphs after a pause.

Sustain pedal CC64 is supported: pedal-down preserves released notes, and pedal-up removes notes that are no longer physically held.

## Parameters

Global host-automatable expression parameters:

- **Attack**: amplitude gate opening time, seconds.
- **Release**: amplitude gate closing time, seconds.
- **Glide**: legato spectral morph time, seconds.
- **Organic**: spectral instability/noise/saturation amount.
- **Filter**: spectral magnitude threshold, applied globally to whatever is playing.

Filter is applied to target magnitudes before glide smoothing, so filtered bins fade in or out through the mono spectral state.

## Organic processing

Organic is integrated into the mono spectral render hop before the instrument iFFT:

1. The mono engine updates current magnitude and current phase-advance toward the active target.
2. It advances phase continuously.
3. Organic applies banded magnitude instability, phase instability, spectral smoothing, and residual/noise energy directly to that spectrum.
4. The spectrum is mirrored, inverse-FFT’d, lightly saturated, windowed, and overlap-added.

This keeps the graph to one spectral synthesis stage.

## Implementation map

- `dsp/src/instrument.rs` — capture format, capture-from-audio, pad mapping, `FreezeInstrument`, `MonoSpectralEngine`.
- `dsp/src/spectral.rs` — shared spectral synthesis helpers.
- `desktop-shell/src/plugin.rs` — CLAP/VST3/standalone bridge, MIDI/mouse event handling, audio-thread state cache, audition instrument.
- `desktop-shell/src/state.rs` — persisted instrument state and editor runtime state.
- `desktop-shell/src/source/` — WAV loading.
- `desktop-shell/src/ui/` — source panel, waveform, Freeze Pool, Pad Grid, expression controls.
