# Running a Testnet-11 node

Verified against current `main` on 2026-09-13.

## Roles

A normal full node validates and serves the chain without a model artifact, producer key or Bond. Extra roles are opt-in:

| role | entry point |
|---|---|
| sync / validate | `kaspad --testnet --netsuffix=11` |
| PALW producer | `misaka --network testnet-11 mining setup` |
| panel verifier | `misaka --network testnet-11 verifier setup` |
| DNS-finality validator | `misaka --network testnet-11 validator setup` |
| EVM history/RPC | see `misaka-evm-flat-backend-runbook-v0.1.md` |

## Build

```bash
git switch main
git pull --ff-only
cargo build --release -p kaspad -p misaka-cli
```

## Run

```bash
./target/release/kaspad \
  --testnet --netsuffix=11 \
  --utxoindex \
  --rpclisten-borsh=default \
  --addpeer=169.58.39.220:26311
```

DNS seeding is supported; the explicit peer is a reliable fallback.

## Ports

| interface | default |
|---|---:|
| P2P | 26311 |
| gRPC | 127.0.0.1:26210 |
| wRPC Borsh | 127.0.0.1:27210 |
| wRPC JSON | 127.0.0.1:28210 |
| EVM JSON-RPC | 127.0.0.1:8545 |

wRPC is disabled until its listener flag is supplied. Keep RPC on loopback unless access control and firewalling are intentional.

## Identity

The current Testnet-11 fingerprint is:

```text
ae1d61628da50c7becea62f0a8f08c8654d190c60b2e104df0010b121ba4d3d8
```

A different fingerprint or fork-id schedule is a ruleset mismatch, not an ordinary connectivity problem. Testnet-11 Relaunch 5f uses a different genesis from prior relaunches; old datadirs cannot be continued.

## Health

```bash
./target/release/misaka --network testnet-11 doctor
./target/release/misaka --network testnet-11 mining status
```

Also inspect the startup fingerprint, peer count, sync state, virtual DAA and recent errors.

## Resource profiles

`kaspad --help` exposes these current `--node-profile` values:

- `full`
- `bootstrap-pruned`
- `recovery-sync`
- `validator`
- `archive`
- `public-rpc`

Use the profile as the policy boundary; do not combine it with roles the profile explicitly rejects.

## Updating

1. Build current main.
2. Record the candidate binary hash.
3. Stop the service.
4. Back up the exact existing binary and service unit.
5. Install the candidate.
6. Start with the same current-5f appdir.
7. Verify hash, fingerprint, peers, sync and service health.

Do not wipe a healthy current-5f appdir for a routine binary update. Move only the chain datadir aside when recovery from an incompatible/retired chain is actually required.

## Producing blocks

An external hash miner cannot produce Testnet-11 blocks. Follow [testnet11-join-mining.md](testnet11-join-mining.md); the ADR-0122 wizard starts `kaspad --palw-produce` with the required Bond and class data.
