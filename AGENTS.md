# Spectral Freeze

This project is an audio instrument. It has a dsp module with all audio processing code and native shell modules for releasing it to VST3, CLAP, and standalone.

The instrument leans heave on spectral audio processing. The goal is for the user of the instrument to be able to create compositions with spectral audio source material while allowing them to control the audio on a spectral level.

## Project State

When refactoring or changing code. Do not maintain backwards compatibility. Assume I am the only one using this code. 

This does NOT mean to write unmaintainable code, high quality code is still the goal. It does mean that when changing the code you should favour the new goal state, not maintain the current codebase state, removing old code is highly encouraged.

If when a change is proposed you notice that some other part of the functionality or codebase is no longer in line with that goal, either remove it or ask me what to do about it rather than trying to fit it inside the new goal.


## Development Loop

Run standalone app with auto rebuild/relaunch after code changes:

```sh
./scripts/dev-standalone-watch.sh
```
