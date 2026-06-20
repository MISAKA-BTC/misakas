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
kaspa-pq-validator keygen --out /etc/kaspa-pq/validator.mldsa --network mainnet
```

Generates a fresh ML-DSA-65 key, writes the 32-byte seed (hex, mode `0600`), and prints the
`validator_id` + funding address. **Only the validator key is produced** — per ADR-0011
key separation, the owner / withdrawal key is created separately (`kaspa-pq` wallet) and
**must not** live on the validator host.

### `run` — the validator daemon

```bash
kaspa-pq-validator run \
    --node-rpc 127.0.0.1:27110 \
    --validator-key /etc/kaspa-pq/validator.mldsa \
    --stake-bond <txid_hex>:<index> \
    --signed-epoch-db /var/lib/kaspa-pq/validator-state.json
```

Walks the ADR-0011 state machine (`NodeNotSynced → BondNotFound → BondPending → Active → …`)
and attests while the bond is active. Without all three of `--validator-key`,
`--stake-bond`, `--signed-epoch-db`, it runs **observe-only** (no signing). `--dry-run`
signs + self-verifies locally but never submits. `Slashed` is a fatal, non-zero exit.

> **`--node-rpc` port is network-dependent** — it must match the node's **wRPC Borsh** port:
> mainnet `27110` (used above, matching the `mainnet` keygen), **testnet `27210`** (the live
> network — use this for testnet-10), devnet `27610`. If you start the node with
> `--rpclisten-borsh=default`, the port is chosen automatically for that network — point
> `--node-rpc` at the same value.

### `status` — one-shot health check

```bash
kaspa-pq-validator status --node-rpc 127.0.0.1:27110 --stake-bond <txid_hex>:<index>
```

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
