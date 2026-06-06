# PLAN: Transform Spectral Freeze into a Monophonic Spectral-State Instrument

## Intent

This is **not** a production migration and this project does **not** need backward compatibility with the current instrument behavior. Do not preserve old behavior merely because it exists. Delete, replace, or rewrite architecture freely when it makes the experimental instrument clearer, simpler, or more expressive.

Rework the instrument away from the current polyphonic 16-pad freeze/drum-rack model and toward a monophonic, continuously evolving spectral instrument.

The new core idea:

> The instrument owns one persistent spectral state. MIDI notes and parameters do not spawn independent voices; they steer that spectral state toward captured freeze targets.

This should make the instrument more expressive and more open-ended. The captured FFT data becomes source material / spectral attractors, while the performance layer becomes about movement, glide, instability, and spectral transformation.

The first prototype should stay intentionally small: replace polyphonic playback with one monophonic spectral engine and add only the initial glide behavior needed to morph between freeze targets. This should be a direct transformation, not an added mode layered on top of the old design.

Future experiments can then add more spectral parameters that act directly on magnitude, phase, phase advance, or bin movement.

---

## Current codebase understanding

### Project structure

Important files:

- `dsp/src/instrument.rs` — current MIDI instrument, captured freeze format, pad mapping, polyphonic voice renderer.
- `dsp/src/processor.rs` — original live spectral freeze effect and global spectral output processing helpers.
- `dsp/src/state.rs` — STFT/freeze/effect state types used by `SpectralFreeze`.
- `dsp/src/constants.rs` — FFT constants.
- `dsp/src/params.rs` — effect parameter manifest and `ProcessParams`.
- `desktop-shell/src/plugin.rs` — NIH-plug CLAP/VST3 shell, MIDI/event routing, audition routing.
- `desktop-shell/src/params.rs` — host parameters for the current instrument.
- `desktop-shell/src/state.rs` — persisted pool/pad state and editor runtime state.
- `desktop-shell/src/ui/*` — egui editor panels.
- `docs/instrument-v1-spec.md` — current v1 pad-bank design spec.

### Current captured freeze data

`dsp/src/instrument.rs` defines the useful spectral source material:

```rust
pub struct FrozenChannelData {
    pub mag: Vec<f32>,
    pub phase: Vec<f32>,
    pub phase_advance: Vec<f32>,
}

pub struct CapturedFreeze {
    pub name: String,
    pub source_path: Option<String>,
    pub source_sample_rate: f32,
    pub cursor_sample: usize,
    pub cursor_time_seconds: f32,
    pub filter: f32,
    pub channels: Vec<FrozenChannelData>,
}
```

This should be kept. It already contains the data needed for a spectral-attractor engine:

- target magnitudes per bin,
- starting/stored phase per bin,
- measured/smoothed phase advance per bin,
- per-item filter value.

### Current capture path

`capture_freeze_from_audio()` in `dsp/src/instrument.rs`:

- extracts an FFT window around the waveform cursor,
- averages magnitude over `MAG_HISTORY_SIZE`,
- stores latest phase,
- estimates phase advance from recent analysis frames,
- stores up to two source channels.

This can remain unchanged for the first monophonic prototype.

### Current MIDI/pad model

Constants in `dsp/src/instrument.rs`:

```rust
pub const PAD_COUNT: usize = 16;
pub const FIRST_PAD_MIDI_NOTE: u8 = 36;
```

`note_to_pad()` maps MIDI notes C1 through D#2 onto the 16 pads. This is still useful as a first target-selection mechanism.

For the first monophonic prototype, keep:

- 16 pad assignments,
- pool items,
- note-to-pad mapping,
- mouse pad gates.

But reinterpret pads as **spectral target selectors**, not independent voices.

### Current polyphonic renderer

The current instrument is built around:

```rust
pub const MAX_INSTRUMENT_VOICES: usize = 16;

pub struct FreezeInstrument {
    ...
    voices: Vec<InstrumentVoice>,
    sustain_down: bool,
    sustained_pads: [bool; PAD_COUNT],
    output_fx: SpectralFreeze,
    inverse_fft: Arc<dyn Fft<f32>>,
}
```

Each `InstrumentVoice` owns independent playback state:

- active flag,
- pad/item/note/channel,
- velocity,
- ADSR envelope stage,
- per-channel phase arrays,
- per-channel output FIFOs,
- spectrum scratch,
- FIFO position,
- hop counter,
- RNG.

On note-on, `FreezeInstrument::note_on()` finds an assigned pool item, stops any existing voice for the same pad, then starts a free voice slot.

On each sample in `process_block_inner()`, active voices are summed:

```rust
main[ch][n] += voice.output_fifo[ch][voice.fifo_pos] * env;
```

This is the main behavior to remove/replace.

### Current per-voice spectral synthesis

`InstrumentVoice::render_frame()` currently:

1. Reads magnitudes directly from the assigned `CapturedFreeze`.
2. Advances per-bin phase using that freeze's stored `phase_advance`.
3. Applies small phase jitter.
4. Applies the item's threshold-like `filter` by zeroing bins below `max_mag * filter^2`.
5. Builds a complex spectrum from `mag` and `phase`.
6. Rebuilds conjugate mirror.
7. Runs inverse FFT.
8. Normalizes and applies synthesis Hann window.
9. Overlap-adds into the voice's FIFO.

This code is the best starting point for the mono engine. The key change is that it should no longer read `channel.mag[k]` directly as the output magnitude. Instead, it should update a persistent `current_mag[k]` toward a target magnitude, then render from `current_mag[k]`.

### Current global output processing

`FreezeInstrument` owns:

```rust
output_fx: SpectralFreeze
```

After instrument voices are summed, `process_block_inner()` optionally calls `output_fx.process_block()` when Organic or sidechain boost is active.

This can remain initially. It is already a global post-process stage. Later, Organic may be rethought as a direct part of the mono spectral-state engine rather than an output effect.

### Current host parameters

`dsp/src/instrument.rs` currently exposes instrument params:

- Attack
- Decay
- Sustain
- Release
- Organic
- SC Boost
- SC Freq Smooth

`desktop-shell/src/params.rs` mirrors these as NIH-plug `FloatParam`s.

For the new instrument, ADSR is not the conceptual center. The first new control should be some form of spectral glide. There are two possible implementation paths:

1. Fast prototype: repurpose one or more ADSR parameters internally while renaming later.
2. Cleaner prototype: replace `attack/decay/sustain/release` with new spectral controls in `INSTRUMENT_PARAMS`, `InstrumentProcessParams`, and `desktop-shell/src/params.rs`.

Because compatibility is not important, prefer the cleaner prototype.

### Current UI

The editor is still organized around:

- Source waveform/load/audition,
- contextual filter,
- freeze pool,
- 4x4 pad grid,
- bottom ADSR + Organic + SC panel.

Relevant files:

- `desktop-shell/src/ui/editor.rs`
- `desktop-shell/src/ui/bottom_panel.rs`
- `desktop-shell/src/ui/pad_grid.rs`
- `desktop-shell/src/ui/pool_panel.rs`
- `desktop-shell/src/ui/source_panel.rs`
- `desktop-shell/src/ui/filter_panel.rs`

For the first mono prototype, the UI can mostly stay intact. The bottom panel should be renamed away from `ADSR + Organic + SC` and show the new spectral controls.

### Current audition path

`desktop-shell/src/plugin.rs` has two `FreezeInstrument`s:

- `instrument` for MIDI/mouse pad playback,
- `audition` for editor audition monitor.

Audition currently creates a one-item pool and triggers pad 0 on a separate `FreezeInstrument` when the audition item changes.

This can continue to work if `FreezeInstrument` remains the public type and `note_on/process_block_additive/reset` keep their signatures. Internally, it can become monophonic.

---

## Target architecture

### High-level model

Replace the multi-voice renderer with one persistent mono spectral engine.

Conceptually:

```text
Captured freezes = stored spectral attractors
MIDI note-on     = choose target attractor
Parameters       = affect how the current spectral state moves / behaves
Output           = resynthesis of the current spectral state
```

The sound should not be restarted on every note unless explicitly desired. A note changes the target of the spectral body.

### Proposed DSP state

Inside `dsp/src/instrument.rs`, replace `InstrumentVoice`/`Vec<InstrumentVoice>` with something like:

```rust
struct MonoSpectralEngine {
    active: bool,
    gate: bool,
    target_pad: Option<usize>,
    target_item_index: Option<usize>,
    last_note: u8,
    last_channel: u8,
    velocity: f32,

    held_pads: [bool; PAD_COUNT],
    sustain_down: bool,

    current_mag: Vec<Box<[f32; NUM_BINS]>>,
    current_phase: Vec<Box<[f32; NUM_BINS]>>,
    current_phase_advance: Vec<Box<[f32; NUM_BINS]>>,

    amp: f32,
    output_fifo: Vec<Box<[f32; FFT_SIZE]>>,
    spectrum: Box<[Complex32; FFT_SIZE]>,
    fifo_pos: usize,
    hop_counter: usize,
    rng: JuceRandom,
}
```

Notes:

- `current_mag` is the core new state.
- `current_phase_advance` allows the motion/frequency character of the target freeze to glide too.
- `current_phase` stays continuous between targets.
- `held_pads` preserves enough note-off/gate behavior for monophonic playing without independent voices.
- `amp` can be a simple gate smoother for now instead of full ADSR.

### Proposed `FreezeInstrument`

```rust
pub struct FreezeInstrument {
    sample_rate: f32,
    output_channels: usize,
    window: Box<[f32; FFT_SIZE]>,
    window_gain: f32,
    engine: MonoSpectralEngine,
    output_fx: SpectralFreeze,
    inverse_fft: Arc<dyn Fft<f32>>,
}
```

Remove or stop using:

- `MAX_INSTRUMENT_VOICES`,
- `InstrumentVoice`,
- `EnvStage`,
- per-voice ADSR,
- polyphonic summing,
- per-pad sustained release arrays as voice release state.

`PAD_COUNT`, `FIRST_PAD_MIDI_NOTE`, `note_to_pad()`, `pad_note()`, `note_label()`, `CapturedFreeze`, and capture code should stay.

---

## First prototype behavior

### MIDI note-on

1. Map note to pad with `note_to_pad()`.
2. Look up assigned pool item.
3. If valid:
   - set `target_pad`,
   - set `target_item_index`,
   - set `velocity`,
   - mark pad held,
   - open the gate,
   - if this is the first ever target / current spectrum is silent, optionally initialize `current_mag`, `current_phase`, and `current_phase_advance` from the target to avoid a long fade from silence.
4. Do **not** clear FIFO or reset phase on every note-on.

This is the architectural shift: note-on changes a target, not a voice.

### MIDI note-off

1. Map note to pad.
2. If sustain is down, keep the pad held.
3. Otherwise clear the held state for that pad.
4. If no pads are held, close the gate.

No independent release voices.

### Sustain pedal

Simplify sustain:

- sustain down: note-offs do not close the gate,
- sustain up: if no physical pads/notes remain held, close the gate.

Implementation may need separate `held_pads` and `sustained_pads` only if exact sustain semantics are wanted. For the first experiment, simple sustain behavior is enough.

### Active pad display

`FreezeInstrument::active_pads()` should return only the current target/held pad activity.

Possible first behavior:

- active if pad is currently held, or
- active if pad is the current target and gate/output is still audible.

Prefer: show `target_pad` while `amp` is above a small threshold, plus held pads.

### Per-sample process

For each sample:

1. If `hop_counter >= HOP_SIZE`, render one spectral frame from the mono engine.
2. Pop samples from the mono output FIFO into `main`.
3. Apply simple amplitude smoothing/gate.
4. Advance FIFO and hop counters.

No loop over voices.

### Per-hop render

Given the current target item:

1. For each output channel, choose matching source channel from `CapturedFreeze`.
2. Compute target magnitudes from `channel.mag`, applying item filter.
3. Move `current_mag[ch][k]` toward target magnitude.
4. Move `current_phase_advance[ch][k]` toward target phase advance.
5. Advance `current_phase[ch][k]` by the current phase advance plus optional tiny jitter.
6. Build spectrum from `current_mag` and `current_phase`.
7. Rebuild conjugate mirror.
8. Inverse FFT.
9. Normalize, synthesis-window, overlap-add into output FIFO.

The key new logic is:

```rust
current_mag[k] += mag_glide_coeff[k] * (target_mag[k] - current_mag[k]);
current_phase_advance[k] += phase_glide_coeff[k] * (target_adv[k] - current_phase_advance[k]);
```

For the first version, `mag_glide_coeff[k]` can be the same for every bin.

---

## Initial parameter set

Replace the ADSR controls with spectral-state controls.

### First mandatory controls

#### `magGlide`

Controls how quickly magnitudes move toward the target freeze.

Suggested range:

- 0.0 to 5.0 seconds, or
- 0.0 to 1.0 normalized mapped exponentially to a time range.

Because this is a host parameter, a linear seconds range is simplest initially.

Interpretation:

- 0 ms: almost immediate target changes,
- 100–500 ms: playable spectral glides,
- multiple seconds: slow spectral drifting.

#### `phaseGlide`

Controls how quickly per-bin phase advance moves toward the target freeze's `phase_advance`.

Suggested range:

- 0.0 to 5.0 seconds.

This may be subtler than magnitude glide, but it prepares the architecture for phase/frequency-bank expression.

### Keep initially

- `organic`
- `scBoost`
- `scFreqSmoothing`

These can continue to run as post-processing through `output_fx`.

### Optional simple gate controls

Because ADSR is being removed, the mono instrument needs at least a minimal amplitude behavior.

For the first prototype, hardcode:

- gate attack smoothing: e.g. 5–10 ms,
- gate release smoothing: e.g. 100–250 ms.

Avoid adding more parameters until the spectral glide sound is evaluated.

### Suggested new `InstrumentProcessParams`

```rust
pub struct InstrumentProcessParams {
    pub mag_glide_s: f32,
    pub phase_glide_s: f32,
    pub organic: f32,
    pub sc_boost_db: f32,
    pub sc_freq_smoothing: f32,
}
```

Update:

- `INSTRUMENT_PARAMS`,
- parameter constants,
- `desktop-shell/src/params.rs`,
- `SpectralFreezePlugin::current_params()`,
- bottom panel labels.

---

## Glide coefficient calculation

Glide is updated once per rendered FFT hop, not every sample.

For a time constant in seconds:

```rust
fn glide_coeff(time_s: f32, sample_rate: f32) -> f32 {
    if time_s <= 0.0 {
        1.0
    } else {
        1.0 - (-(HOP_SIZE as f32) / (time_s * sample_rate)).exp()
    }
}
```

This gives stable exponential smoothing independent of sample rate and hop size.

Use separate coefficients for:

- magnitude glide,
- phase-advance glide.

---

## Filter behavior

Current per-item filter is a threshold:

```rust
threshold = max_mag * item.filter * item.filter
if mag >= threshold { mag } else { 0.0 }
```

For the first prototype, keep this behavior, but apply it to the target magnitude before smoothing:

```rust
target_mag = if raw_mag >= threshold { raw_mag } else { 0.0 };
current_mag += coeff * (target_mag - current_mag);
```

This allows filtered-out bins to fade away instead of disappearing instantly.

Later, filter could become another continuous spectral-state parameter.

---

## Implementation steps

### Step 1: Update the design/spec docs

- Keep `docs/instrument-v1-spec.md` as historical if desired.
- Add or update documentation describing the new mono spectral-state instrument.
- This `PLAN.md` is the starting implementation plan.

### Step 2: Change DSP params

In `dsp/src/instrument.rs`:

- remove `PARAM_ATTACK`, `PARAM_DECAY`, `PARAM_SUSTAIN`, `PARAM_RELEASE`,
- add `PARAM_MAG_GLIDE`, `PARAM_PHASE_GLIDE`,
- update `INSTRUMENT_PARAMS` length and entries,
- update `InstrumentProcessParams` and `Default`/`clamped()`.

In `desktop-shell/src/params.rs`:

- replace NIH-plug params `attack`, `decay`, `sustain`, `release` with `mag_glide`, `phase_glide`,
- update formatting/parsing as seconds,
- keep Organic/SC params.

In `desktop-shell/src/plugin.rs`:

- update `current_params()`.

In `desktop-shell/src/ui/bottom_panel.rs`:

- rename panel from `ADSR + Organic + SC` to something like `Spectral Motion + Organic + SC`,
- show `Mag Glide` and `Phase Glide` knobs.

### Step 3: Replace voice storage with mono engine

In `dsp/src/instrument.rs`:

- remove or stop using `InstrumentVoice`, `EnvStage`, `MAX_INSTRUMENT_VOICES`, and `voices: Vec<InstrumentVoice>`.
- create `MonoSpectralEngine` with persistent spectral arrays and FIFO.
- move useful pieces of `InstrumentVoice::render_frame()` into `MonoSpectralEngine::render_frame()`.

Important: no note-on should call `clear_buffers()` unless the entire instrument is reset or channel count changes. Continuous state is the point.

### Step 4: Rewrite `FreezeInstrument::prepare/reset`

`prepare()` should:

- update sample rate and channel count,
- fill Hann window and calculate window gain,
- prepare mono engine channel arrays for output channel count,
- prepare `output_fx`,
- reset sustain/gate state if appropriate.

`reset()` should:

- clear mono engine spectral state,
- clear FIFOs,
- close gate,
- reset `output_fx`.

### Step 5: Rewrite note handling

`note_on()`:

- map note to pad,
- resolve assignment to pool item,
- set mono target,
- mark held pad,
- open gate.

`note_off()`:

- clear held pad or respect sustain,
- close gate if no notes remain held/sustained.

`set_sustain()`:

- simple global sustain behavior.

### Step 6: Rewrite processing loop

`process_block_inner()` should:

- optionally clear outputs,
- for each sample:
  - render a hop when due,
  - pop mono FIFO into outputs with gate amplitude,
  - zero FIFO slot,
  - advance FIFO/hop counters,
- run global `output_fx` if `clear_outputs` and Organic/SC are active.

`process_block_additive()` should still add audition output into existing main buffers, so preserve `clear_outputs = false` behavior.

### Step 7: Update activity reporting

`active_pads()` should use the mono engine's held/target state instead of active voices.

### Step 8: Build and test

Run:

```bash
cargo fmt
cargo test
cargo check
```

Then test manually in the desktop shell/plugin:

- load WAV,
- capture several freezes,
- assign to pads,
- hold one pad,
- play another pad and confirm the spectrum glides instead of layering,
- adjust `Mag Glide`,
- adjust `Phase Glide`,
- verify audition still works,
- verify Organic/SC still compile and do not break output.

---

## Future spectral experiments

These are intentionally not part of the first implementation, but the new monophonic spectral-state architecture should make them easy to add.

### Per-bin glide spread

Make glide coefficient depend on bin index.

Examples:

- lows arrive first, highs trail,
- highs arrive first, lows bloom later,
- midrange moves fastest while edges smear.

Possible parameter:

```text
Spread: -1.0..1.0
negative = high-to-low
positive = low-to-high
```

### Randomized bin inertia

Each bin has its own glide multiplier, either fixed per target or slowly evolving.

Sound goal:

- cloudy transitions,
- uneven spectral melting,
- less linear crossfade behavior.

### Energy-aware glide

Make strong bins and weak bins move at different speeds.

Examples:

- strong partials snap into place, noisy residuals trail,
- weak/airy bins respond quickly while body lags.

### Phase drift

Add controlled random drift to phase per bin.

Potential control:

```text
Drift: 0..1
```

Low drift = stable frozen oscillator bank.  
High drift = vapor, chorus, unstable spectral mist.

### Phase attraction

Let current phase be weakly attracted to target stored phase.

This should be experimental because direct phase interpolation can cause wrapping artifacts, but it may create interesting locking/unlocking behavior.

### Spectral bend / theremin control

Apply a continuous bend curve to `current_phase_advance`.

Examples:

- pitch bend scales all phase advances,
- mod wheel bends high bins more than lows,
- nonlinear bend stretches/compresses spectral spacing.

This is one of the most theremin-like ideas.

### Velocity mapping

Use velocity to affect:

- output energy,
- magnitude glide speed,
- brightness / filter threshold,
- phase drift amount,
- transient spectral excitation.

### Mod wheel / pitch bend mapping

Potential mappings:

- pitch bend = spectral bend / phase advance scale,
- mod wheel = phase drift or spectral spread,
- aftertouch = spectral energy or glide acceleration.

### Morphing between neighboring targets

Instead of MIDI selecting one target, arrange freeze pool/pads as positions in a continuous space.

Examples:

- keyboard note chooses interpolation between adjacent freeze targets,
- pitch bend moves between neighboring targets,
- XY pad chooses blend of four spectral attractors.

### Spectral residual/noise injection

Add controlled noise/residual energy shaped by current magnitude envelope.

Could be related to the existing `organic` behavior, but integrated directly into the mono engine.

### Freeze-target memory

Allow the current spectrum to retain traces of previous targets even after a new note arrives.

Ideas:

- slow decay floor per bin,
- spectral smear buffer,
- recency-weighted target blend,
- delayed bins that follow older targets.

---

## Guiding principle

Avoid preserving old polyphonic behavior unless it directly helps exploration. Do not add compatibility branches, legacy modes, or abstractions whose only purpose is to keep the previous pad-bank instrument alive.

This codebase is an experimental instrument, not a compatibility product. The priority is to make the DSP architecture simple and expressive:

```text
one spectral body
many possible gestures
captured freezes as attractors
parameters as spectral forces
```
