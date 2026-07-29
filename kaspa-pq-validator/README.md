# kaspa-pq-validator

The **single-host validator sidecar** for kaspa-pq (ADR-0011). A standalone process that
connects to a co-located `kaspad` over a local `127.0.0.1` wRPC (borsh) endpoint and, once
its stake bond is active, attests to the selected-chain anchor each epoch — signing with an
ML-DSA-65 validator key, funding a `StakeAttestationShard` transaction from a UTXO at its
own address, and submitting it.

This is the **production-recommended** deployment shape. The *integrated* alternative
(`kaspad --enable-validator …`, ADR-0010) runs the same logic in-process; both share the
signing core (`kaspa-pq-validator-core`). The sidecar lets you restart the validator
without taking down consensus, isolate the validator key by file mode, and run `--dry-run`.

## Build

```bash
cargo build --release -p kaspa-pq-validator    # target/release/kaspa-pq-validator
```

## Subcommands

### `keygen` — create a validator key

```bash
kaspa-pq-validator keygen --out /etc/kaspa-pq/validator.mldsa --network testnet
```

Generates a fresh ML-DSA-87 key, writes the 32-byte seed (hex, mode `0600`), and prints the
`validator_id` + funding address. **Only the validator key is produced** — per ADR-0011
key separation, the owner / withdrawal key is created separately (`kaspa-pq` wallet) and
**must not** live on the validator host.

### `run` — the validator daemon

```bash
kaspa-pq-validator run \
    --node-rpc 127.0.0.1:27210 \
    --validator-key /etc/kaspa-pq/validator.mldsa \
    --stake-bond <txid_hex>:<index> \
    --signed-epoch-db /var/lib/kaspa-pq/validator.testnet-200.state \
    --network testnet-200
```

Walks the ADR-0011 state machine (`NodeNotSynced → BondNotFound → BondPending → Active → …`)
and attests while the bond is active. Without all three of `--validator-key`,
`--stake-bond`, `--signed-epoch-db`, it runs **observe-only** (no signing). `--dry-run`
signs + self-verifies locally but never submits. `Slashed` is a fatal, non-zero exit.

> **`--node-rpc` port is network-dependent** — it must match the node's **wRPC Borsh** port:
> mainnet `27110`, **testnet `27210`** (the live network is testnet-200), devnet `27610`. If you start the node with
> `--rpclisten-borsh=default`, the port is chosen automatically for that network — point
> `--node-rpc` at the same value.

### `status` — one-shot health check

```bash
kaspa-pq-validator status --node-rpc 127.0.0.1:27210 --stake-bond <txid_hex>:<index> --network testnet-200
```

### `palw-payload` — strict PALW lifecycle artifacts

For the PALW presets (`testnet-200`, `testnet-110`, and `devnet-111`), this command builds the exact raw Borsh
payload consumed by `palw-submit`. Inputs that cross operator boundaries use versioned JSON; payload
outputs are never JSON-wrapped.

A miner first supplies canonical, batch-unbound leaves (`batch_id` zero, indices `0..n`) as:

```json
{"schema":"misaka.palw.leaf-set.v1","leaves":[/* PalwPublicLeafV1 objects */]}
```

Build the manifest and its content-id-restamped leaf set, then build every reported chunk:

```bash
kaspa-pq-validator palw-payload batch-manifest \
  --network testnet-110 --leaves-file leaves.unbound.json \
  --registration-epoch 123 --descriptor-root <hash64> --audit-policy-id <hash64> \
  --out manifest.borsh --restamped-leaves-out leaves.batch.json

kaspa-pq-validator palw-payload leaf-chunk \
  --network testnet-110 --manifest-file manifest.borsh --leaves-file leaves.batch.json \
  --chunk-index 0 --out chunk-0.borsh
```

The manifest builder derives model/runtime ids and the checked leaf-bond sum from the leaves, derives
all windows from network consensus parameters, re-runs manifest admission, reconstructs every chunk,
and verifies every Merkle opening. A mismatched epoch/model/runtime/root/bond sum is refused. Submit
the manifest during its declared registration epoch, wait for selected-chain inclusion, then submit
all chunks in separate stages.

Audit facts must come from a synced node, not from an assembler-authored file. The RPC returns the
complete canonical, selection-relevant provider view frozen at the audit DAA: all rows created by the
snapshot, plus any later-created row explicitly named by a leaf (needed for verifier-identical
producer/operator exclusions), with later unbond/slash stamps rewound. It refuses an oversized raw
registry before deriving this view; it never truncates. Export one round:

```bash
kaspa-pq-validator palw-payload audit-facts \
  --network testnet-110 --node-rpc 127.0.0.1:27210 \
  --batch-id <batch_id> --audit-beacon-epoch 125 --out audit-facts.json
```

Each selected auditor evaluates the beacon-selected sample independently, then signs only after its
own synced node returns the identical frozen round — seed, manifest/leaves, selection-relevant
provider view, committee, parameters, and sample included. The live sink may advance harmlessly; a
pre-snapshot fork or selection change is refused:

```bash
kaspa-pq-validator palw-payload audit-vote \
  --network testnet-110 --node-rpc 127.0.0.1:27210 --facts-file audit-facts.json \
  --validator-key auditor.key --auditor-bond <txid:index> --verdict pass \
  --checked-leaf-bitmap-root <hash64> --passed-leaf-count 16 \
  --rejected-leaf-bitmap-root <hash64> --out auditor-1.vote.borsh

kaspa-pq-validator palw-payload certificate \
  --network testnet-110 --node-rpc 127.0.0.1:27210 --facts-file audit-facts.json \
  --vote-file auditor-1.vote.borsh --vote-file auditor-2.vote.borsh \
  --out certificate.borsh
```

The assembler verifies every ML-DSA-87 signature against the selected bond, counts PASS stake against
the full selected slate, re-derives the round inputs, runs the stateless consensus payload validator,
and refuses fork-drifting or provider-omitting facts by querying its node again. V2 votes sign both
`passed_leaf_count` and `rejected_leaf_bitmap_root`; the assembler derives those values from the votes
and rejects any disagreement, so it cannot repackage a valid vote set under another summary. It takes
`certificate_epoch` from that fresh live query rather than the older file cursor. All artifacts use
create-new writes (mode `0600` on Unix);
existing files/symlinks are not replaced, and the manifest is removed if its paired leaf-set write
fails.

Operational limits are deliberately fail-closed: audit-facts accepts at most 1,024 provider records
(reading at most 1,025 to detect overflow) and the RPC refuses JSON above 16 MiB. Exceeding either
limit is a public-network activation blocker requiring a protocol/operator capacity decision. This
tooling does **not** supply receipt DA,
anti-spam economics, pruning snapshots, or proof that an auditor actually possessed off-chain receipt
chunks; `checked_leaf_bitmap_root` remains the auditor's evidence commitment. The two summary fields
are now quorum-attested, not independently re-derived by consensus from receipt evidence. DA,
challenge/fraud evidence, and payout/slashing policy must therefore still be closed separately before
a valuable/public network is enabled.

This is a V2-only cutover. Legacy V1 votes, certificates, and pruning bundles are rejected; operators
must coordinate a re-genesis/new network identity, reset old datadirs, and regenerate all audit
artifacts rather than replaying an existing V1 shared-testnet history.

## Requirements & safety (ADR-0011)

- **Same host** as the node; the node's RPC bound to `127.0.0.1` only (firewall it on all
  public interfaces).
- The node must run **`--utxoindex`** — the sidecar's funding lookup uses `getUtxosByAddresses`.
- **One key, one host.** The same validator key MUST NOT run on a second host concurrently
  (the single biggest equivocation/slashing risk). Back up the `signed-epoch-db`.
- Downtime is **not** slashable; only equivocation is.

## systemd

Two units linked by `Requires=` (see ADR-0011 §"Systemd reference units"); the validator
`ExecStart` is `kaspa-pq-validator run --node-rpc 127.0.0.1:<port> …`.

## Smoke test (simnet)

Verifies the wRPC round-trip of the validator RPCs against a real node. Start a simnet
node and query it:

```bash
# 1. node (separate terminal); --utxoindex enables the funding lookup
kaspad --simnet --utxoindex --rpclisten-borsh=127.0.0.1:27510 --appdir=/tmp/kpq-smoke

# 2. one-shot status (exercises getServerInfo + getStakeBond)
kaspa-pq-validator status --node-rpc 127.0.0.1:27510 --stake-bond "$(printf 'ab%.0s' {1..64}):0"
#   node_network: simnet
#   node_synced:  false
#   bond:         abab…ab:0 (not found in the registry)   # simnet has no overlay → correct

# 3. observe-only daemon (state machine against the live node)
kaspa-pq-validator run --node-rpc 127.0.0.1:27510 --network simnet
#   [kaspa-pq-validator] connected: network=simnet synced=false version=1.1.0
#   [kaspa-pq-validator] status=NodeNotSynced (virtual_daa=0)
```

The full reward-bearing path (active bond → sign → fund → submit a shard tx) requires an
overlay-active network (devnet) with an activated `StakeBond`; that end-to-end run is the
remaining integration test (shared with the in-process validator's deferred e2e).
