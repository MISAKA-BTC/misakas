#!/usr/bin/env bash
set -euo pipefail
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=2
core=(--config profile.dev.package.kaspa-consensus-core.opt-level=0 --config profile.dev.package.kaspa-consensus-core.debug=0)
cargo test -p misaka-palw-positive-contract --test case_journal -- --test-threads=2
cargo test -p kaspa-consensus-core --lib positive_journal "${core[@]}" -- --test-threads=2
cargo test -p kaspa-consensus-core --lib native_route_scanner_rejects_duplicate_and_unsorted_positive_keys_before_collection "${core[@]}" -- --test-threads=1
cargo test -p misaka-palw-tir-exec --features node --test positive_court_a4_integration a6_journal_replay "${core[@]}" -- --test-threads=2
cargo test -p kaspa-consensus --lib positive_journal "${core[@]}" --config profile.dev.package.kaspa-consensus.opt-level=0 --config profile.dev.package.kaspa-consensus.debug=0 -- --test-threads=2
python3 docs/maintenance/check_docs.py
