#!/usr/bin/env bash
# Profile-aware Execution Gate (Delegates to Aegis Rust standalone CLI)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/tools/target/release/aegis"

if [ ! -f "$BIN" ]; then
  cargo build --release --manifest-path "$ROOT/tools/Cargo.toml" --quiet
fi

exec "$BIN" gate --root "$ROOT" "$@"
