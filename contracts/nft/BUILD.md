# Reproducible build manifest — MisakaNFT721Immutable

Source verification for the MISAKA NFT template. Pin every input, rebuild from a
clean checkout, and compare the bytecode hash below. Any mismatch means the
deployed contract does not correspond to this source.

## Pinned inputs

| Input | Pin |
|---|---|
| solc | `0.8.28` (`0.8.28+commit.7893614a`) — pinned in `foundry.toml` |
| OpenZeppelin Contracts | `v5.0.2` (commit `dbb6104ce834628e473d2173bbc9d47f81a9eec3`) |
| forge-std | `v1.9.4` |
| evm_version | `shanghai` |
| optimizer | enabled, 200 runs |
| bytecode_hash / cbor_metadata | `none` / `false` (no trailing metadata → deterministic) |

## Expected artifact (clean build, 2026-06-22)

| Artifact | Value |
|---|---|
| creation bytecode keccak256 | `0x15b5683e97ee55230e59d28aea660c3e682289f84f1599e7990674381d85ee90` |
| runtime bytecode keccak256 | `0x311a8cd197c423f32706ab36871428bb8c8b6c32fe947902088b0215a483d70d` |
| runtime size | 5689 bytes (< EIP-170 24576) |
| forge test | 30/30 (15 core + 14 hardening + 1 invariant) |

> The hashes change if ANY pinned input changes. Re-running `build.sh` on a clean
> checkout must reproduce them exactly; record the new values here on any
> intentional bump (and note the reason).

## Rebuild + verify

```
cd contracts/nft
./build.sh            # installs the exact tags, builds, prints the hashes, runs tests
```

`build.sh` fails loudly if the creation/runtime keccak does not match the table above.

## On-chain deploy manifest (fill in per deployment)

Record for each production deploy so the audit trail is complete:

- network / EVM chain id: `0x4D534B` (MISAKA)
- deployer address:
- contract address:
- constructor args: name, symbol, maxSupply, baseURI, manifestHash, collectionURI, admin, minter, royaltyReceiver, royaltyBps
- admin / minter / royalty receiver (and whether admin is a multisig/timelock)
- `finishMinting()` tx (if the edition is sealed)
