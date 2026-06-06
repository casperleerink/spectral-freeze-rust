# Plan: Legato Mono Spectral Instrument Interaction

## Goal

Change the instrument interaction model so Spectral Freeze behaves like a playable monophonic spectral instrument:

- a fresh note starts immediately from its assigned freeze spectrum,
- legato notes morph between freeze spectra,
- releasing the latest note returns to the previous still-held note,
- releasing the final note fades to silence,
- glide is a legato transition control, not the core attack/release mechanism.

No backward compatibility is required. Prefer replacing old behavior over layering compatibility paths.

## Non-goals

- Do not reintroduce polyphonic voices.
- Do not reintroduce ADSR decay/sustain.
- Do not reintroduce sidechain, sidechain parameters, aux input buses, or post-FFT output FX.
- Do not make glide happen from silence on fresh detached notes.
- Do not expose separate magnitude/phase glide controls in this pass.

## Chosen Interaction Model

Use a newest-held-note priority stack.

### No notes held

- Output is silent after release finishes.
- No active target is required.
- The last spectral state may remain internally, but it should not cause a new detached note to glide from stale material.

### Note A on from silence

- Resolve note A to its pad and assigned freeze item.
- Seed the mono spectral state immediately from A.
- Open the amplitude gate with Attack.
- Do **not** glide from silence or from stale previous material.

### Note B on while A is still held

- Push B onto the held-note stack.
- B becomes the active target.
- Keep the gate open.
- Glide the existing spectral state from A toward B.

### Note B off while A is still held

- Remove B from the held-note stack.
- A becomes the active target again because it is now the newest remaining held note.
- Keep the gate open.
- Glide the existing spectral state from B back toward A.

### Final note off

- Remove the final note from the stack.
- Close the amplitude gate with Release.
- Do not force spectral magnitudes to glide to zero; amplitude release handles silence.

### Detached note after silence

If no notes are held and the gate has closed, a later note-on should seed directly from that note's freeze spectrum.

This prevents unexpected morphs from old material after a pause.

## Parameter Model

Current technical controls:

- `Mag Glide`
- `Phase Glide`
- `Organic`

Desired musical controls:

- `Attack`
- `Release`
- `Glide`
- `Organic`

### Glide decision

Prefer **one public Glide parameter** for now.

Reasoning:

- Users usually do not know or care whether a transition is magnitude glide or phase/frequency glide.
- The musical concept is: "how long does it take to morph from one held note's freeze to another?"
- A single Glide knob is easier to play and easier to automate.
- Internally we can still apply different derived coefficients to magnitude and phase advance if needed.

Suggested internal mapping:

```rust
mag_glide_s = glide_s;
phase_glide_s = glide_s * PHASE_GLIDE_RATIO;
```

Start with:

```rust
PHASE_GLIDE_RATIO = 1.0
```

If one-knob glide feels smeary or unstable, tune the internal ratio later, for example:

```rust
PHASE_GLIDE_RATIO = 0.5 // phase/frequency catches up faster
```

Only expose separate controls later if listening tests prove one Glide is insufficient.

Alternative descriptive two-knob naming, if needed later:

- `Shape Glide` for magnitudes / spectral envelope.
- `Motion Glide` for phase advance / spectral movement/pitch motion.

But do not expose both in this pass.

## New Instrument Params

Replace current instrument params with:

```rust
pub const PARAM_ATTACK: usize = 0;
pub const PARAM_RELEASE: usize = 1;
pub const PARAM_GLIDE: usize = 2;
pub const PARAM_INSTRUMENT_ORGANIC: usize = 3;

pub struct InstrumentProcessParams {
    pub attack_s: f32,
    pub release_s: f32,
    pub glide_s: f32,
    pub organic: f32,
}
```

Suggested ranges/defaults:

- Attack: `0.0..=5.0 s`, default `0.008 s`
- Release: `0.0..=10.0 s`, default `0.180 s`
- Glide: `0.0..=5.0 s`, default `0.250 s`
- Organic: `0.0..=1.0`, default unchanged

## DSP State Changes

Replace boolean-only held pad tracking with an ordered held-note stack.

Add:

```rust
struct HeldNote {
    pad: usize,
    item_index: usize,
    note: u8,
    channel: u8,
    velocity: f32,
}
```

In `MonoSpectralEngine` keep:

```rust
held_notes: Vec<HeldNote>,
sustained_notes: Vec<HeldNote>, // or a release/sustain flag per held note
active_target: Option<HeldNote>,
gate: bool,
amp: f32,
```

Implementation can be simpler than this if sustain is represented cleanly, but newest-held-note priority must be preserved.

## Note-On Behavior

In `FreezeInstrument::note_on()` / `MonoSpectralEngine::note_on()`:

1. Map MIDI note to pad.
2. Resolve pad assignment to freeze item.
3. Remove any existing stack entry for the same `(note, channel)` to avoid duplicates.
4. Determine whether this is a fresh articulation:
   - no currently held/sustained notes, and
   - gate is closed or amp is near zero.
5. Push the new held note to the stack.
6. Set it as active target.
7. If fresh articulation:
   - seed spectral state directly from target freeze,
   - clear output FIFO if needed to avoid stale overlap-add tails,
   - open gate using Attack.
8. If legato:
   - do not seed,
   - keep current spectral state,
   - let spectral state glide toward new target using Glide.

## Note-Off Behavior

In `FreezeInstrument::note_off()` / `MonoSpectralEngine::note_off()`:

1. Remove matching `(note, channel)` from physical held stack.
2. If sustain pedal is down, keep the note alive as sustained.
3. Else recompute active target:
   - newest remaining held/sustained note becomes target,
   - if no notes remain, close gate using Release.
4. If target changes to a previous note, do not seed; glide back to that target.

## Sustain Behavior

Sustain should preserve released notes until the pedal is lifted.

Suggested behavior:

- sustain down: note-off marks note as no longer physically held but does not remove it from active priority stack.
- sustain up: remove all notes that are not physically held.
- after cleanup:
  - newest remaining note becomes target and glides there,
  - or gate closes if no notes remain.

## Render Behavior

In `render_frame()`:

- Use active target item if present.
- Derive both magnitude and phase-advance glide from one public `glide_s`.
- Continue current Organic-in-spectrum processing.
- Keep continuous phase during legato transitions.

If no active target:

- Do not render new spectral frames, or render silence.
- Existing FIFO may drain under Release.

## UI Changes

Update desktop bottom panel:

- remove `Mag Glide`
- remove `Phase Glide`
- add `Attack`
- add `Release`
- add `Glide`
- keep `Organic`

Suggested panel title:

```text
Expression
```

or

```text
Motion
```

## Files To Update

- `dsp/src/instrument.rs`
  - params
  - held-note stack
  - note-on/off/sustain behavior
  - gate smoothing uses Attack/Release params
  - one public Glide maps internally to magnitude/phase glide

- `desktop-shell/src/params.rs`
  - replace mag/phase glide params with attack/release/glide

- `desktop-shell/src/plugin.rs`
  - update `current_params()`

- `desktop-shell/src/ui/bottom_panel.rs`
  - update knobs/labels

- `dsp/src/tests.rs`
  - update param expectations
  - add interaction tests

- `docs/instrument-mono-spectral-state.md`
  - document legato mono model

## Tests To Add

### Fresh note seeds directly

- Trigger note A from silence.
- Verify spectral output corresponds to A without requiring glide time from silence.

### Legato note-on glides to new target

- Hold A.
- Press B before releasing A.
- Verify output transitions toward B, not layered with A.

### Releasing latest note returns to previous held note

- Hold A.
- Press B.
- Release B while A is still held.
- Verify active target becomes A and output transitions back toward A.

### Final release closes gate

- Press A.
- Release A.
- Verify output decays to silence according to Release.

### Detached note does not glide from stale state

- Press A.
- Release A and wait for silence.
- Press B later.
- Verify B seeds directly instead of slowly morphing from A.

### Sustain preserves released notes

- Press A.
- Sustain down.
- Release A.
- Verify gate remains open.
- Sustain up.
- Verify gate closes if no physical notes remain.

## Validation

Run:

```bash
cargo fmt
cargo test -p dsp
cargo check
```

Manual checks:

- A on: direct A attack.
- A hold, B on: morph A to B.
- B off while A held: morph B to A.
- A off: release to silence.
- A off, wait, B on: direct B attack, no stale glide.
