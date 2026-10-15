#!/usr/bin/env bash
# Profile-aware Execution Gate (delegates to the Aegis CLI). Builds the CLI on first use.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# The repository root is a Cargo workspace: the binary lands in its target directory, not in tools/target.
BIN="${CARGO_TARGET_DIR:-$ROOT/target}/release/aegis"

if [ ! -x "$BIN" ]; then
  cargo build --release --manifest-path "$ROOT/Cargo.toml" --quiet
fi

exec "$BIN" gate --root "$ROOT" "$@"
