#!/usr/bin/env bash
# Source-only typed/legacy bridge; no public profile, wire or activation.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
cargo test --locked -p misaka-palw-tir-exec --features positive-panel \
    --test positive_typed_adapter --test positive_typed_capture \
    --test positive_full_relation --test positive_panel_r1 --test positive_record -- --nocapture
cargo test --locked -p misaka-palw-tir-exec --features positive-panel \
    --lib node::positive_typed_adapter::budget::
cargo clippy --locked -p misaka-palw-tir-exec --features positive-panel --lib --no-deps -- -D warnings
