#!/usr/bin/env bash
# Private A0/A1/A2/A3 r1 integration; no public-network activation.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
cargo test --locked -p misaka-palw-positive-contract
cargo test --locked -p misaka-palw-tir-exec --features positive-panel \
    --test positive_check --test positive_check_hardening --test positive_contract --test positive_panel_r1 --test positive_record
cargo test --locked -p misaka-palw-remote --features positive-panel \
    --test positive_panel_r1 --test positive_panel_sources_r1 --test positive_panel_record_r1
cargo test --locked -p misaka-palw-remote --features positive-panel --lib positive_panel_sources::
cargo test --locked -p misaka-palw-remote --features positive-panel --lib miner::positive_v2
cargo test --locked -p misaka-palw-remote --lib pursuit::
