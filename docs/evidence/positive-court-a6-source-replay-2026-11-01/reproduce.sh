#!/usr/bin/env bash
set -euo pipefail
# Run in the repository at source-snapshot.json code_commit. Set a suitable
# CARGO_TARGET_DIR externally; no native activation or live funding is invoked.
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=2
core=(--config profile.dev.package.kaspa-consensus-core.opt-level=0 --config profile.dev.package.kaspa-consensus-core.debug=0)
cargo test -p kaspa-consensus-core --lib palw_positive_common_source_replay_v2 "${core[@]}" -- --test-threads=2
cargo test -p kaspa-consensus-core --test positive_source_v2 "${core[@]}" -- --test-threads=2
cargo test -p misaka-palw-tir-exec --features node --test positive_court_a4_integration a6_journal_replay "${core[@]}" -- --test-threads=2
python3 docs/maintenance/check_docs.py
