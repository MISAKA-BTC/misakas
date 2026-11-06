#!/usr/bin/env bash
# RFC0043 data-only fixed-length Flat/greedy CPU run; no public activation.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
cargo test --locked -p misaka-palw-tir-exec --features positive-panel \
    --test run_wide_q24_v2 --test run_wide_idx_history --test run_wide_memory \
    --test node_leg --test capture_codec --test cell \
    --test executor_tree_memory --test retention_memory \
    --test positive_typed_capture --test positive_typed_adapter -- --nocapture
cargo test --locked -p misaka-palw-tir-exec --features positive-panel --lib node::memory::tests::
cargo clippy --locked -p misaka-palw-tir-exec --features positive-panel --lib --no-deps -- -D warnings
