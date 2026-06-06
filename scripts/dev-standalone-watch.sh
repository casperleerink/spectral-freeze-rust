#!/usr/bin/env bash
set -euo pipefail

if ! command -v watchexec >/dev/null 2>&1; then
  echo "error: watchexec is required for auto rebuild/relaunch." >&2
  echo "Install it with: brew install watchexec" >&2
  exit 127
fi

exec watchexec \
  --restart \
  --exts rs,toml \
  --ignore target \
  --watch Cargo.toml \
  --watch Cargo.lock \
  --watch dsp \
  --watch desktop-shell \
  -- \
  cargo run -p spectral-freeze --bin spectral_freeze -- "$@"
