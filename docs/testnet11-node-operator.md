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

The current Testnet-11 fingerprint is (the build that schedules the DAA 6,701 flag day; the 7,000 release printed `ae1d6162…` and is refused from 6,700, the moved height it does not schedule):

```text
d4161a86544fb03ce6f343184dd80e73b930f6665da409c5981e8cfa32786cf2
```

The fence schedule this build prints on start:

```text
1150, 1900, 2150, 2400, 3500, 4000, 6300, 6301, 6400, 6501, 6900, 2125000
```

**6,700** is the compatibility boundary (no PALW rule fires there; it exists so the fleet is on one
binary first), **6,701** is the one PALW upgrade day, **6,800** carries ADR-0133's Verification V2
and the whole-artifact possession proof, **6,901** retires the compute overlay, **6,900** is the
market's least seed. The first four moved up 300 on 2026-09-18, when holding the release to fix the
DAA clock took the tip past the boundary they had been pinned to. What each one changes:
[docs/testnet11-6701-upgrade-announcement.md](testnet11-6701-upgrade-announcement.md).

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
