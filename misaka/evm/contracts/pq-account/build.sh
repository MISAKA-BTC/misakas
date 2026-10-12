#!/usr/bin/env bash
# Reproducible build + test for the MISAKA PQ-Rooted EVM Smart Account (PREA P0-2).
# Installs the EXACT pinned dependency (forge-std), builds with the pinned solc
# (foundry.toml), and runs the Foundry tests. No OpenZeppelin dependency.
set -euo pipefail
cd "$(dirname "$0")"

FORGE_STD_TAG="v1.9.4"

command -v forge >/dev/null || { echo "forge not found (install Foundry: https://getfoundry.sh)"; exit 1; }

echo ">> installing pinned deps"
forge install "foundry-rs/forge-std@${FORGE_STD_TAG}" --no-git

echo ">> clean build (solc pinned in foundry.toml)"
forge build --force

echo ">> test"
forge test -vvv

echo ">> bytecode hashes (record in this file when the source freezes)"
JQ='jq -r'
for c in MisakaPqSmartAccount; do
  CREATION=$(${JQ} '.bytecode.object' "out/${c}.sol/${c}.json")
  RUNTIME=$(${JQ} '.deployedBytecode.object' "out/${c}.sol/${c}.json")
  echo "  ${c} creation keccak:  $(cast keccak "${CREATION}")"
  echo "  ${c} runtime  keccak:  $(cast keccak "${RUNTIME}")"
done
