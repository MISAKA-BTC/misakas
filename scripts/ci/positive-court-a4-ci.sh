#!/usr/bin/env bash
set -euo pipefail
A4_REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$A4_REPO_ROOT"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$A4_REPO_ROOT/target/a4-court-ci}"
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
# Fresh Rust source build. Optional ROCKSDB_LIB_DIR may point to an existing
# compatible native RocksDB archive; no consensus Rust rlib is substituted.
cargo test -p misaka-palw-positive-contract -p misaka-palw-positive-accounting --offline --locked
cargo test -p misaka-palw-tir-exec --features node --offline --locked \
    --test positive_check --test positive_check_hardening --test positive_contract \
    --test positive_court_a4_integration --test positive_court_a4_admission --test positive_court_a4_native \
    --config 'profile.dev.package.kaspa-consensus-core.opt-level=0'
cargo test -p kaspa-consensus-core --lib palw_positive_court_v2::coordinator --offline --locked \
    --config 'profile.dev.package.kaspa-consensus-core.opt-level=0'
cargo test -p kaspa-consensus-core --lib palw_court_shared --offline --locked \
    --config 'profile.dev.package.kaspa-consensus-core.opt-level=0'
python3 docs/maintenance/check_docs.py
