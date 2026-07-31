# testnet-200 → testnet-20 migration

As of 2026-07-30, **`testnet-20` is the only publicly operated MISAKA testnet**. `testnet-200`
remains compilable for operators still holding its ledger, but it has **no DNS seeders and no
public discovery**, and replaying its history is unsafe (below). `testnet-10` remains only as a
compatibility preset.

## Why testnet-200 was replaced

testnet-200's replay halted at DAA 388,500. Its DNS-finality thresholds (`required_work_depth`
100 / `required_stake_depth` 5000) were changed **mid-chain with no DAA activation gate**, and
those thresholds are consensus-relevant for IBD: they drive `is_dns_confirmed` →
`last_dns_confirmed_anchor` → the v4 `palw_beacon_seed`. A node replaying pre-change history under
the new thresholds confirms a DNS anchor at a different height and re-derives a *non-zero* beacon
seed for a block that committed the all-zero seed under the old thresholds. That is a provenance
rejection → `StatusInvalid` poison → the missing-parents dead loop.

For a young network the clean recovery is a re-genesis, and testnet-20 (`compute-registry-palw`)
already was one: a v5 genesis with the thresholds flat from block 0. Making it the public net is
therefore the recovery, not a second migration.

Recurrence is guarded: `DnsParams::required_work_depth` / `required_stake_depth` now carry an
explicit invariant that they are consensus-relevant for IBD replay and must not be changed
mid-chain without a DAA gate, and `kaspad` gained `--reset-invalid-marks` so a node already
poisoned by a stale `StatusInvalid` can recover without a full resync.

## What changed

| | testnet-200 (deprecated) | **testnet-20 (public)** |
|---|---|---|
| Network id / flags | `--testnet --netsuffix=200` | **`--testnet --netsuffix=20`** |
| Genesis | v4 (staging-mainnet PALW) | **v5** (`COMPUTE_REGISTRY_PALW_GENESIS`) |
| P2P port | 26511 | **26521** |
| RPC ports | 26210 / 27210 / 28210 | unchanged (26210 / 27210 / 28210) |
| DNS seeders | none (removed) | `seeder1.misakascan.com`, `seeder3.misakascan.com` |
| Compute Set registry | fenced off | **open from genesis** (`palw_compute_registry_activation_daa_score = 0`; band 0x40-0x44, Header v5 only) |
| Validator entry floor | 20,000,000 MSK bond, 3 validators | **10 MSK bond, 1 validator** |
| DNS confirmation | WorkDepth ≥ 100, StakeDepth ≥ 5000 | unchanged, and flat from block 0 |

PALW and algo-4 acceptance are genesis-active on both. Everything else on testnet-20 inherits the
staging-mainnet shape verbatim — real PoW, non-inert anti-spam, full-scale depths. The registry
rehearsal changes the *validator entry* economics only, because §17.3's validator quorum has to be
mining-fundable within minutes of a fresh re-genesis; 20 M-MSK floors on a ~2-MSK coinbase would
take on the order of 10 M blocks to fund.

The two public networks were deliberately given **separate P2P ports**. A shared default let a node
on another testnet suffix cross-handshake instead of failing fast, and because a node
self-advertises `default_p2p_port`, discovery would hand peers a port nobody was listening on.

## Operator cutover

testnet-200 state cannot be carried across — the genesis differs. Use a **fresh datadir**.

```sh
kaspad --testnet --netsuffix=20 --utxoindex --rpclisten-borsh=default
```

DNS discovery is automatic (the same seed names now resolve testnet-20). If it is unavailable:

```sh
kaspad --testnet --netsuffix=20 \
  --addpeer=95.111.236.186:26521 --utxoindex --rpclisten-borsh=default
```

Validators need a fresh `--signed-epoch-db` per network: reusing one across networks trips the
anti-equivocation guard on overlapping epoch numbers.

```sh
kaspa-pq-validator bond --node-rpc 127.0.0.1:27210 --validator-key val.seed \
  --amount 1000000000 --network testnet-20            # 10 MSK entry floor
kaspa-pq-validator run  --node-rpc 127.0.0.1:27210 --validator-key val.seed \
  --stake-bond <txid:index> --signed-epoch-db val.testnet-20.state --network testnet-20
```

## Bringing the PALW beacon up (done 2026-07-31)

A fresh testnet-20 node reports `palw_enabled: true` and a synced sink, but the beacon starts
**halted** — it needs at least one bonded, beacon-enabled validator, and after the 200→20 cutover
the running validator was still attached to the deprecated testnet-200 node. Symptom:

```
activation.derived_mode: halted        activation.buried_sample_count: 0
activation.newest_sample: epoch=none   dns_health: DegradedCertificateCensored
stake_depth: 0.000000000/0.000005000
```

The sequence that fixes it:

```sh
# 1. bond a validator (testnet-20 entry floor is 10 MSK; bond more for margin)
kaspa-pq-validator bond --node-rpc 127.0.0.1:<borsh> \
  --validator-key <seed> --amount 100MSK --network testnet-20

# 2. run kaspad with the in-node validator AND the beacon layered on it
kaspad --testnet --netsuffix=20 ... \
  --enable-validator --validator-mode=active \
  --validator-key=<seed> --stake-bond=<txid:index> \
  --enable-beacon
```

**Set the heartbeat below one epoch.** The validator's default heartbeat is 30 s, but an epoch is
100 DAA — about 27 s on a live testnet-20. At that ratio the service reliably lands the
beacon-COMMIT and misses the REVEAL (its window is one epoch), so the seed carries instead of
advancing and `activation.open` flaps to false with `buried_carry_run` above `grace_epochs`. The
log shows the signature clearly: a run of `submitted beacon-commit` lines with no matching
`beacon-reveal`. Fix:

```sh
KASPA_VALIDATOR_HEARTBEAT_SECS=3 kaspad ...
```

With a 3 s heartbeat, commit and reveal both land every epoch, and the state settles to:

```
activation.open: true          activation.buried_carry_run: 0
activation.derived_mode: healthy   dns_confirmed: true
activation.newest_sample: epoch=<current-2> seed=<changes every epoch>
```

Two other footguns worth writing down. Bond transactions aggregate funding UTXOs, and each
ML-DSA-87 input is ~4.6 kB — a 19-input bond exceeds the 480,000 transient-mass limit, so bond an
amount that needs ten-ish inputs rather than the maximum the 20-UTXO cap allows. And `pkill -f`
patterns that appear in your own remote command line will kill the SSH session running them.

## What beacon-derived machinery this unblocks

Until the beacon advances, everything seeded by it is unavailable: PALW audit-round seeds,
beacon-driven DA chunk sampling, and beacon-salted job challenges. Tooling that consumes the
beacon should fail closed rather than fall back to a carried seed — `misaka-palw-bridge` refuses
to issue challenges in exactly that state, and resumes once `activation.open` is true.
