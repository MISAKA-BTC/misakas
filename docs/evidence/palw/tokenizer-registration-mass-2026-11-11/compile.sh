#!/bin/sh
set -eu
base_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
dep_dir=/tmp/misaka-public-contract-test.fd7edK/debug/deps
exec rustc --edition 2024 -L "dependency=$dep_dir" \
    --extern "kaspa_consensus_core=$dep_dir/libkaspa_consensus_core-1ae7c6f5cf89b13b.rlib" \
    --extern "kaspa_txscript=$dep_dir/libkaspa_txscript-ac4138da362dae90.rlib" \
    --extern "kaspa_hashes=$dep_dir/libkaspa_hashes-809eb1c36e40821b.rlib" \
    --extern "libcrux_ml_dsa=$dep_dir/liblibcrux_ml_dsa-0b2424abe2d920a7.rlib" \
    "$base_dir/main.rs" -o "$base_dir/probe"
