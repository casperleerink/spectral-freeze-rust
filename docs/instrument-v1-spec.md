# Spectral Freeze Instrument v1 Spec

## Goal

Replace the current audio-effect workflow with a true MIDI instrument workflow for CLAP/VST3. The instrument lets users load source audio, find interesting spectral freeze moments, collect them in a pool, assign them to a 16-pad bank, and play those pads from MIDI like a drum rack.

Native CLAP/VST3 is the v1 target. Keep DSP/data structures host-agnostic where practical so a WAM version can be revisited later.

## Product Model

- True instrument, not timeline-bound audio effect.
- Empty bank by default.
- WAV loading only for v1.
- No source audio is embedded in plugin state.
- Captured spectral freeze data is embedded/saved, so projects still play if source files move.
- Source file paths and cursor positions are saved as metadata where useful.

## Main Workflow

1. User clicks **Load Audio File** and selects a WAV file.
2. UI shows the source waveform/cursor area.
3. User clicks/scrubs the waveform cursor.
4. If **Audition Monitor** is enabled, the plugin continuously sounds the frozen result at the cursor.
5. User adjusts the contextual **Filter** for the current audition.
6. User clicks **Capture**.
7. A new item is added to the **Freeze Pool** only.
8. User drags a pool item onto a pad.
9. MIDI notes or mouse-held pad clicks play assigned pads.

## UI Layout

Single-screen layout, not tabs:

```text
┌────────────────────────────────────────────┐
│ Source Files / Load WAV                    │
│ Waveform + cursor + Audition toggle        │
├──────────────────────┬─────────────────────┤
│ Freeze Pool          │  4×4 Pad Grid        │
│ captured moments     │  MIDI playable       │
│ scrollable list      │                     │
├──────────────────────┴─────────────────────┤
│ Contextual Filter + ADSR + Organic + SC    │
└────────────────────────────────────────────┘
```

No spectrum display for v1.

## Source Files

- V1 import: **Load WAV** button/file dialog.
- Drag/drop file import can come later.
- Source files are not embedded in state.
- Missing source files should show a warning, but captured pool items remain playable.
- Stereo behavior should preserve current channel behavior: stereo files capture/play stereo spectral data where possible.

## Audition Monitor

- Editor-only sound-design helper.
- Defaults off when editor opens.
- Not saved as active playback state.
- Only sounds while editor is open and Audition Monitor is enabled.
- Sounds the current waveform cursor freeze continuously.
- Moving the cursor updates/replaces the auditioned frozen sound immediately.
- Raw source playback is not the default behavior.

## Capture

- Capture immediately commits the current audition state into a Freeze Pool item.
- Captured item should sound the same when played, except for velocity, envelope, and global processing.
- Captured item stores:
  - spectral freeze data
  - source file metadata/path
  - cursor time/sample metadata
  - auto-generated name, e.g. `vocal.wav @ 00:12.438`
  - item Filter value
- Capture does **not** auto-assign to a pad.

## Freeze Pool

- Unlimited/scrollable list for v1.
- List item shows auto-generated name; optionally show filter value.
- Items can be deleted.
- Deleting an item clears any pads referencing it.
- Items are not auditionable by clicking in v1.
- Selecting an item allows editing its stored Filter value.
- Pads reference pool items live, so editing a pool item’s Filter updates all pads assigned to that item.

## Contextual Filter

- UI/state-only, not host-automatable in v1.
- One contextual Filter control:
  - waveform/cursor selected: controls current audition and next capture
  - pool item selected: edits that item’s stored Filter
  - assigned pad selected: selects/edits its underlying pool item
- New captures store the current audition Filter value.

## Pad Grid

- 16 pads, 4×4 grid.
- Default MIDI mapping: C1 through D#2.
- Pads show:
  - assigned/unassigned state
  - item name/short name
  - MIDI note label
  - active/playing highlight
- Drag Freeze Pool item onto pad to assign/replace.
- Same pool item may be assigned to multiple pads.
- Dropping on an assigned pad replaces immediately.
- Pad clear action exists, e.g. small `×` or context/right-click.
- No pad-to-pad rearranging in v1.
- Mouse interaction:
  - mouse down on assigned pad = note-on/trigger
  - mouse up = note-off/release
  - pad click also selects the underlying pool item for Filter editing

## MIDI / Voice Behavior

- Instrument is not pitched/chromatic.
- Each MIDI note triggers one pad slot.
- No pitch transposition.
- Polyphonic across pads.
- Cap active voices simply, e.g. 16 active voices.
- Retriggering the same pad/note restarts/chokes that pad’s current voice rather than layering another copy.
- Note-off enters release.
- Sustain pedal CC64 is supported:
  - pedal down holds note-offs
  - pedal up releases held notes
- Ignore pitch bend, mod wheel, aftertouch, etc. in v1.

## Envelope

Global ADSR, host-automatable:

- Attack: 0–5s, default 10ms
- Decay: 0–5s, default 100ms
- Sustain: 0–100%, default 100%
- Release: 0–10s, default 250ms

Notes sustain indefinitely while held, then release on note-off.

## Global Processing / Parameters

Keep these global controls from the current effect where applicable:

- Organic: global, same default as current effect (0%)
- SC Boost: global, host-automatable
- SC Smooth: global, host-automatable

Sidechain should behave like a built-in global spectral effect/enhancement stage on the instrument output rather than per-pad sidechain processing.

Omit global output/gain for v1. No automatic loudness normalization in v1.

## Preset / Project State

Save with plugin state:

- Freeze Pool items including embedded spectral data and item Filter values
- Pad assignments referencing pool items
- global ADSR/Organic/SC parameter values
- source path/cursor metadata where useful

Do not embed full source audio files.

No separate bank import/export for v1.

## Out of Scope for v1

- Non-WAV file support
- Web/WAM UI implementation
- Spectrum display
- Spectrogram view
- Factory/demo items
- Tempo/sync features
- Pad rearranging
- MIDI learn / automatable pad assignments
- Host automation for contextual Filter
- Loudness normalization
- Export/import bank files
- Raw source playback as primary audition path
