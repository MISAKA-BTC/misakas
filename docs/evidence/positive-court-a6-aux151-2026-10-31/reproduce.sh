#!/usr/bin/env bash
set -euo pipefail
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=2
opts=(--config profile.dev.package.kaspa-consensus-core.opt-level=0 --config profile.dev.package.kaspa-consensus-core.debug=0)
cargo test -p kaspa-consensus-core --lib palw_positive_rows_v2 "${opts[@]}" -- --test-threads=2
RUST_MIN_STACK=33554432 cargo test -p kaspa-consensus-core --lib palw_native_resource_v1 "${opts[@]}" -- --test-threads=2
cargo test -p kaspa-consensus-core --lib palw_kernel_route_v1 "${opts[@]}" -- --test-threads=2
cargo test -p misaka-palw-positive-contract
cargo test -p misaka-palw-positive-recovery -p kaspa-database "${opts[@]}" -- --test-threads=2
python3 docs/maintenance/check_docs.py
