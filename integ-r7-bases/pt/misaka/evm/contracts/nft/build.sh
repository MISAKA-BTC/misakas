#!/usr/bin/env bash
# Reproducible build + source verification for MisakaNFT721Immutable.
# Installs the EXACT pinned dependencies, builds with the pinned solc, runs the
# test suite, and checks the bytecode keccak256 against BUILD.md. Fails on drift.
set -euo pipefail
cd "$(dirname "$0")"

OZ_TAG="v5.0.2"
FORGE_STD_TAG="v1.9.4"
EXPECT_CREATION="0x15b5683e97ee55230e59d28aea660c3e682289f84f1599e7990674381d85ee90"
EXPECT_RUNTIME="0x311a8cd197c423f32706ab36871428bb8c8b6c32fe947902088b0215a483d70d"

command -v forge >/dev/null || { echo "forge not found (install Foundry)"; exit 1; }
command -v cast  >/dev/null || { echo "cast not found (install Foundry)"; exit 1; }

echo ">> installing pinned deps"
rm -rf lib
forge install "foundry-rs/forge-std@${FORGE_STD_TAG}" --no-git
forge install "OpenZeppelin/openzeppelin-contracts@${OZ_TAG}" --no-git

echo ">> clean build (solc pinned in foundry.toml)"
forge build --force >/dev/null

echo ">> tests"
forge test

J=out/MisakaNFT721Immutable.sol/MisakaNFT721Immutable.json
CREATION=$(python3 -c "import json;print(json.load(open('$J'))['bytecode']['object'])" | cast keccak)
RUNTIME=$(python3 -c "import json;print(json.load(open('$J'))['deployedBytecode']['object'])" | cast keccak)

echo ">> creation keccak: $CREATION"
echo ">> runtime  keccak: $RUNTIME"
fail=0
[ "$CREATION" = "$EXPECT_CREATION" ] || { echo "!! creation bytecode MISMATCH (expected $EXPECT_CREATION)"; fail=1; }
[ "$RUNTIME"  = "$EXPECT_RUNTIME"  ] || { echo "!! runtime bytecode MISMATCH (expected $EXPECT_RUNTIME)"; fail=1; }
[ "$fail" = 0 ] && echo ">> OK: bytecode matches BUILD.md" || { echo ">> FAIL: bytecode drift — see BUILD.md"; exit 1; }
