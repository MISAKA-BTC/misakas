# Running a Testnet-12 node

A normal full node validates and serves the chain without a model artifact, producer key or Bond.
The step-by-step for starting one — build, the `kaspad` command line, ports and the startup
fingerprint to check — is §1 and §3 of [`testnet12-join-mining.md`](testnet12-join-mining.md). This
page only maps the roles to their guides.

## Roles

Extra roles are opt-in:

| role | entry point | guide |
|---|---|---|
| sync / validate | `kaspad --testnet --netsuffix=12 --utxoindex --rpclisten-borsh=default` | [`testnet12-join-mining.md`](testnet12-join-mining.md) §3 |
| PALW producer | `misaka --network testnet-12 mining setup` | [`testnet12-join-mining.md`](testnet12-join-mining.md) §5–§6 |
| panel verifier | `misaka --network testnet-12 verifier setup` | [`testnet12-join-mining.md`](testnet12-join-mining.md) §5 |
| DNS-finality validator | `misaka --network testnet-12 validator setup` | [`validator-runbook.md`](validator-runbook.md) |
| EVM history/RPC | — | [`misaka-evm-flat-backend-runbook-v0.1.md`](misaka-evm-flat-backend-runbook-v0.1.md) |
| archive node | `kaspad --archival` | [`archival.md`](archival.md) |

`misaka` uses testnet-12 when no network is named; the examples name it anyway.

## Operations

Ports, updating without wiping the app dir, the one-process-per-bond rule, service health and
resource profiles are in [`wiki/Operations-Notes.md`](wiki/Operations-Notes.md). A watchdog that
notices a hung (not just exited) node is in [`node-liveness-probe.md`](node-liveness-probe.md).

`kaspad --help` lists the `--node-profile` values (`full`, `bootstrap-pruned`, `recovery-sync`,
`validator`, `archive`, `public-rpc`). The sync-only profiles reject `--archival`, `--utxoindex`,
`--enable-validator`, `--evm-rpc-listen` and `--unsaferpc`; do not work around a profile with
manual flags.

Keep RPC on loopback unless access control and firewalling are intentional. wRPC is disabled until
its listener flag (`--rpclisten-borsh` / `--rpclisten-json`) is supplied.

## Health

```bash
misaka --network testnet-12 doctor
misaka --network testnet-12 node dag-info
```

Also inspect the startup fingerprint, peer count, sync state, virtual DAA and recent errors. A
different fingerprint or fence schedule from the one in
[`testnet12-join-mining.md`](testnet12-join-mining.md) §3 is a ruleset mismatch, not a connectivity
problem: rebuild from the release rather than copying a value from an older runbook.

### More than one node on a host

Give each node its own `--appdir` and its own ports. `misaka` cannot tell which node a command means
when several `kaspad` processes of one network run on the host; name one with
`--appdir <the node's --appdir>` or `[advanced] appdir` in `~/.misaka/mining.toml`. One bond is
one process: never run the same producer key and bond on two nodes.

### If your node says quarantined

A node that has closed its chain-participation gate reports `participation_allowed=false`, and
`misaka mining status` shows `E-NET-PARTICIPATION`. If the node log says `quarantined`, restart once
with `--clear-quarantine` and then remove the flag again: it clears the persisted state on every boot
it is present for (ADR-0025).

## Producing blocks

An external hash miner cannot produce testnet-12 blocks. Production runs inside
`kaspad --palw-produce`; follow [`testnet12-join-mining.md`](testnet12-join-mining.md).

## Execution-lane diagnosis

The execution lane's round blocks need no flag: a node started with `--palw-producer-key` and
`--palw-producer-bond` spends the round permits the chain grants that bond (`--palw-round-lane` is
accepted and does nothing). A permit exists only once a bond has been scheduled from finalized
attempts, so a round block every round is not guaranteed.

For a one-minute window, run `misaka --network testnet-12 palw round-lane` (RPC `getPalwRoundLane`)
to confirm scheduled permits, count `[palw-round-producer]` logs for produced blocks, then count
`[palw-round-lane]` logs for merged/granted/accepted blocks. No permit points to the
scheduler/finalized-attempt path; a permit without production points to producer configuration; a
produced block without merge points to peer acceptance or anchor merging. Merges absent from an
Explorer point to its indexer/display path, not the consensus lane.
