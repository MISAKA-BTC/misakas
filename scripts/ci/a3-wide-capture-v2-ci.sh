#!/usr/bin/env bash
# Explicit bounded PALWTRC2 data API; no native dispatch or Hello2 activation.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
cargo test --locked -p misaka-palw-tir-exec --features positive-panel \
    --test wide_capture_v2 --test capture_codec --test run_wide_q24_v2 -- --nocapture
cargo clippy --locked -p misaka-palw-tir-exec --features positive-panel --lib --no-deps -- -D warnings
