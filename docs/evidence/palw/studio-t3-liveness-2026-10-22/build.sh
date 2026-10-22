#!/usr/bin/env bash
# studio/t3-liveness: build the measurement test binaries (release). Run from the repo root.
# CARGO_TARGET_DIR defaults to ./target; the Mac Studio run used the shared /Users/wata/Downloads/MISAKA-testnet/target.
set -u
cd "$(git rev-parse --show-toplevel)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
OUT=docs/evidence/palw/studio-t3-liveness-2026-10-22/raw
mkdir -p "$OUT"
nice -n 10 cargo test --release -j4 -p kaspa-consensus-core \
  --lib \
  --test studio_t3_court_close_floor \
  --test palw_held_close_chunks \
  --test audit_liveness_court_close_replay \
  --test audit_liveness_court_fences \
  --no-run 2>&1 | tee "$OUT/build-$(uname -s)-$(date +%Y%m%dT%H%M%S).log"
echo "BUILD EXIT ${PIPESTATUS[0]}"
