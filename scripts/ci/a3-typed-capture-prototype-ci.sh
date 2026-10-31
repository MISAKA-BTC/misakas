#!/usr/bin/env bash
# Local source-only typed capture. No wire assignment or native activation.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
cargo test --locked -p misaka-palw-tir-exec --features positive-panel \
    --test positive_typed_capture -- --nocapture
cargo test --locked -p misaka-palw-tir-exec --features positive-panel \
    --test positive_panel_r1 --test node_leg --test capture_codec --test court
cargo test --locked -p misaka-palw-tir-exec --features positive-panel --lib
cargo test --locked -p misaka-palw-remote --features positive-panel \
    --test positive_panel_record_r1
