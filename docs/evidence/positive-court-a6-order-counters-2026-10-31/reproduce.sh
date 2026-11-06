#!/usr/bin/env bash
set -euo pipefail
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=2
cargo test -p kaspa-consensus-core --lib palw_positive_rows_v2 \
  --config profile.dev.package.kaspa-consensus-core.opt-level=0 \
  --config profile.dev.package.kaspa-consensus-core.debug=0 -- --test-threads=2
python3 docs/maintenance/check_docs.py
# Run generators in an isolated copy: the recorded full run exits 1 on the
# pre-existing NumericalManifestV1 PIN mismatch. Do not regenerate the real
# worktree or refresh that unrelated PIN to make this check appear successful.
