#!/usr/bin/env bash
# A3 private EXACT_STREAM adapters; does not enable a native network ruleset.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

cargo test --locked -p misaka-palw-tir-exec --features node --test positive_court_roles --test panel_a3
cargo test --locked -p misaka-palw-remote --test panel_submission
cargo test --locked -p misaka-palw-remote --lib pursuit::
