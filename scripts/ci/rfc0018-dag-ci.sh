#!/usr/bin/env bash
# Consensus rules, real in-process DAG/restart/replay, and public-RPC driver contracts.
# Does not activate fences or stand in for real-process/inference acceptance.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
export RUST_MIN_STACK="${RUST_MIN_STACK:-67108864}"

cargo test --locked --offline -p kaspa-consensus-core --lib palw_rfc0018_v1
cargo test --locked --offline -p kaspa-consensus --lib rfc18 -- --test-threads=1
python3 -m unittest discover -s scripts/drills -p test_rfc0018_dag_drill.py
