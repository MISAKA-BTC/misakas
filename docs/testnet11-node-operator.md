# Running a Testnet-11 node

Verified against current `main` on 2026-09-20.

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
  --rpclisten-borsh=default
```

DNS seeding is the default bootstrap path for testnet-11. If DNS is blocked or a private
environment needs an explicit bootstrap, resolve a seeder for that invocation and pass its IP,
for example `SEEDER_IP=$(dig +short A seeder1.misakascan.com | tail -n1)` followed by
`--addpeer="$SEEDER_IP:26311"`. The peer flag currently accepts IP addresses, not hostnames; do
not persist a resolved IP as the network's peer because seeder answers are operational and may
change.

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

The current Testnet-11 fingerprint is (the build that shortens the execution span at 7,300; previous `137b9c50…`):

```text
400403b8431082c9464d7326c3c11f77425ef3dbc41110f85a0dd28cb6f5f2d8
```

**This number moved on 2026-09-20 and the one it replaced is not wrong, it is older.** The execution
lane's schedule span shortens 5 DAA → 1 DAA at 7,300; ADR-0130's f+2 future-seed delay is kept.
What did **not** move is the identity two nodes compare at the handshake, so a node on `137b9c50…`
still peers with this one. It will be refused from DAA 7,300, where the span length changes — which
is why every node must carry this build before that height.

The fence schedule this build prints on start:

```text
1150, 1900, 2150, 2400, 3500, 4000, 6900, 7100, 7101, 7200, 7300, 7301, 8000, 2125000
```

**7,100** activates the held regime and deep-audit fixes. **7,101** is the one PALW upgrade day
(the internal `6001` bundle), including ADR-0125's 1-BPS execution lane. **7,200** carries
ADR-0133's Verification V2 and the whole-artifact possession proof, **7,300** shortens the
execution span, **7,301** retires the compute overlay, and **6,900** is the market's least seed.
What each one changes:
[docs/testnet11-7101-upgrade-announcement.md](testnet11-7101-upgrade-announcement.md).

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

An external hash miner cannot produce Testnet-11 blocks. Follow [testnet11-join-mining.md](testnet11-join-mining.md); the ADR-0122 wizard starts `kaspad --palw-produce --palw-round-lane` with the required Bond and class data.

## Execution-lane diagnosis

At DAA 7,101 and above, the consensus rule accepts ADR-0125 round blocks at one permit per
one-second round. That does not guarantee a round block every second: a bond must first be scheduled
from finalized attempts. `misaka mining setup` now starts `kaspad --palw-round-lane` with that
bond and its producer key so earned permits can be spent.

Past DAA 7,300 the scheduler still needs a Final and one future seed span, but a span is 1 DAA
(~120 s) rather than 5, so capacity that exists can rise within about four minutes.

For a one-minute window, query `getPalwRoundLane` (or `misaka --network testnet-11 palw round-lane`)
to confirm scheduled permits, count `[palw-round-producer]` logs for produced blocks, then count
`[palw-round-lane]` logs for merged/granted/accepted blocks. No permit indicates the
scheduler/finalized-attempt path; a permit without production indicates producer configuration; a
produced block without merge indicates peer acceptance or anchor merging. Merges absent from an
Explorer point to its indexer/display path, not the consensus lane.
