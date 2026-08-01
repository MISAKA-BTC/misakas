# testnet-20 → testnet-21 migration

As of 2026-08-01, **`testnet-21` (`pcpb-palw`) is the only publicly operated MISAKA testnet**.
`testnet-20` remains compilable for operators still holding its ledger, but it has **no DNS
seeders and no public discovery**, and its history **cannot replay** under the current rules
(below). `testnet-200` and `testnet-10` remain deprecated/retired as before.

## Why testnet-20 was replaced

ADR-0045 D3-b (PCPB LeafV2, `docs/palw-pcpb-leaf-v2-wiring-design.md`) is a leaf-format
re-genesis train, and it invalidates testnet-20's mined history at three independent points:

1. **Leaf-chunk wire v3 is mandatory.** Historical blocks carry v2 chunks; `validate_leaf_chunk`
   refuses anything but v3 (a lenient parse is the §5.15.4 hole), so the carrying blocks
   themselves fail transaction isolation on replay.
2. **The leaf layout moved** (`PalwPublicLeafV1` 964 → 1189 bytes: `a_commit`, `a_commit_epoch`,
   `provider_snapshot_root`, `assignment_proof_root`, `dispatch_kind`). Stored leaves do not even
   decode to the same bytes.
3. **Batch identities re-derive.** `leaf_hash → leaf_root → content_id() == batch_id` all move
   with the layout, so every algo-4 header on the old chain names a batch id that no longer
   exists under the new derivation.

The PCPB windows (`palw_freshness_window_epochs` w=6 / `palw_snapshot_lag_epochs` k=2 /
`palw_post_commit_delta_epochs` Δ=2) are consensus params now — part of `consensus_identity_hash`
— which is exactly what the testnet-20 identity tripwire caught. Its documented option (b)
applies: **re-genesis onto a new suffix, never an in-place edit.** testnet-21 is that re-genesis:
the compute-registry (v5) shape with LeafV2 + clauses 11/12/13 live from block 0.

An operational note that made the timing easier, not harder: testnet-20's first four days had
already produced two minute-order forks whose dead branches can capture fresh syncs into
DNS-anchor wedges (see "the bystander wedge" below). A fresh net, synced from genesis under the
fixed reorg gate, has no such trap history.

## What changes for an operator

| | testnet-20 (old) | **testnet-21 (new)** |
|---|---|---|
| Select with | `--testnet --netsuffix=20` | `--testnet --netsuffix=21` |
| P2P port | `26521` | **`26531`** |
| RPC ports | gRPC `26210` / wRPC Borsh `27210` / JSON `28210` | unchanged (per-NetworkType) |
| Genesis | `compute-registry-palw` v5 | **`pcpb-palw` v5** (tag `misaka-pcpb-palw`) |
| Discovery | — (seeders moved) | `seeder1.misakascan.com`, `seeder3.misakascan.com` |
| PALW lane | active, pre-PCPB leaves | active, **LeafV2 + PCPB clauses 11/12/13 from block 0** |
| EVM lane | inert (`u64::MAX`) | **activates at DAA 6,500,000** — requires `--features evm` |

- **Fresh datadir required.** No state carries across a re-genesis; do not point a testnet-20
  datadir at testnet-21.
- `misaka` CLI defaults to `testnet-21` (CLI > `MISAKA_NETWORK` > config > default). A stale
  explicit `--network testnet-20` fails the node-network match check loudly rather than joining a
  deprecated net.
- `kaspa-pq-validator` artifact builds: `--network testnet-21`.
- MTP scoring scope moves to `testnet-21` (the network NAME is the scope key; old ledgers stay
  verifiable under their own names).
- **Producers**: the first mintable `registered_epoch` on testnet-21 is `k + Δ = 4` (design memo
  §10.2). Do not register batches before epoch 4 — the early epochs are structurally
  algo-4-empty, which is fail-closed, not a fault.

## EVM lane flag day (ADR-0020) — DAA 6,500,000

Set 2026-08-02. testnet-21 is the first MISAKA network to activate the selected-parent EVM lane
**mid-chain**; testnet-10 and devnet had it open from genesis, and every other preset keeps the
`u64::MAX` fence. Below the fence nothing changes: `is_evm_active` is false, both EVM header
commitments are consensus-forced to zero (`RuleError::NonZeroEvmHeaderFieldsBeforeActivation`),
and blocks already mined replay byte-identically. That is what makes opening the fence legal on a
live net under clause (a) of the `pcpb_palw_network_selection` tripwire — the identity digest
moves, the history does not. The new digest is
`77e4b552896f22da…2b3b977b`; `kaspad` logs it at startup and `getInfo` reports it as
`consensusParamsHash`.

**Every node operator must act before the fence.** Unlike the testnet-10 activation, there is no
version isolation to fall back on: testnet-21 headers are already Header-v5, which outranks
`EVM_HEADER_VERSION` in `check_header_version`, so a non-EVM node is *not* fenced out at header
validation — it would sync normally and then refuse to follow the chain from the fence onward,
losing a synced datadir at the worst possible moment.

1. **Replace your binary.** Either take the published release archive (built with the feature
   since this release) or rebuild:
   ```bash
   cargo build --release -p kaspad --bin kaspad --features evm
   ```
   A build without the feature now refuses to *start* on testnet-21 and prints this remedy, so
   there is no silent-misconfiguration path — but it does mean an un-upgraded node stops at its
   next restart rather than at the fence.
2. **Check the startup log.** An EVM-capable node prints
   `EVM lane (ADR-0020): ACTIVATES at DAA score 6500000 on testnet-21`. No line means no feature.
3. **Bound EVM state growth** with `--evm-storage-profile=compact` (flat backend authoritative,
   per-block 206 snapshot retired). Without it the node persists a full EVM state clone per
   block — `O(state × kept blocks)`. Node-local and consensus-neutral, so operators may choose
   differently without splitting.
4. **Expose the Ethereum JSON-RPC only if you want it**: `--evm-rpc-listen` (default port 8545).
   It is not needed for node, validator, mining, or PALW participation. Do not expose it publicly
   without understanding what it serves.

What activation does *not* turn on: the lane's four execution fences
(`evm_gas_pool_v2`, `evm_f002_withdraw_cap`, `evm_f003_mldsa_verify`, `evm_typed_receipt_root`)
all stay `u64::MAX`, so the v1 strict declared-gas executor runs and the F002/F003 precompile
variants stay inert. Each is a separate, independently gated decision.

**Lane rule (ships with the activation): the EVM payload rides algo-3 only.** From the fence on,
a block on the algo-4 (PALW replica) lane must carry an *empty* `evm_payload` — the algo-3 hash
floor is the only lane that may contribute EVM transactions and bridge deposit-claims. Algo-4
blocks still execute their mergeset and commit `evm_commitment_root` like every chain block, so
EVM state continuity is lane-blind; the restriction pins *inclusion* to the permanent lane so a
PALW-side halt (beacon grace exhaustion) can never stall EVM inclusion. Producers need no action:
the template path only assembles payloads on algo-3 templates. See ADR-0020 for the full
rationale.

One consequence worth stating plainly: the released `kaspad` now links k256 (pure-Rust secp256k1)
for EVM `ecrecover`, so it is no longer a fully secp-free binary. The UTXO/L1 signature domain is
unaffected — it remains ML-DSA-87-only — and k256 authorizes EVM-lane transactions and nothing
else. This is the scoped supersession ADR-0023 anticipated. The default (`cargo build` without
the feature) tree stays secp-free and `scripts/pq-ci-guard.sh` still enforces that.

## The bystander wedge (2026-08-01), and what this migration does about it

Reported independently by two operators (three reproductions, byte-identical anchor/sink) on
testnet-20: during IBD a node can momentarily place its sink on a dead fork branch, a DNS anchor
gets confirmed there within milliseconds (the stake window is dominated by the shared pre-fork
segment, so a dead branch still clears `required_stake_depth`), and the node then holds the dead
branch forever because the emergency-dominance escape needed an ABSOLUTE
`emergency_work_margin = 1e6` — ~500k blocks of work at testnet-20's CPU difficulty. Permanent
wedge; only a DB copy recovered it.

(Sample size, final: **three** operators — the reporter, Daifuku, tetsu31 — with byte-identical
anchor/sink/fork coordinates; 4 of 5 fresh-sync attempts trapped.)

Status of the three proposed fixes:

1. **Work-margin calibration — LANDED** (`6ff40d4`, 2026-08-01): the enforced margin is now
   difficulty-denominated (`emergency_work_margin_for` = `max_reorg_horizon_blocks` × the
   canonical tip's per-block work), reachable at any difficulty, and the per-preset absolute
   addend is pinned to ZERO by `dns_emergency_work_margin_absolute_addend_stays_zero`.
   **Wedged testnet-20 nodes un-wedge by restarting on a ≥ `6ff40d4` binary — a DB copy is no
   longer required, and there is no need to wait for the fork region to sink below the pruning
   point.** testnet-21 inherits this from birth, so even a confirmed dead-branch anchor is
   escaped as soon as the live branch out-works one reorg horizon at current difficulty
   (~minutes at 10 BPS).
2. **Anchor-confirm hold during IBD replay** (sink timestamp far behind wall clock) — open
   follow-up; defense in depth on top of (1) and (3).
3. **Anchor confirm requires the anchor's own epoch to be attested — LANDED** (2026-08-01,
   same-day follow-up): `DnsParams::require_anchor_attestation`, **`true` on testnet-21,
   genesis-effective**. The confirm predicate now additionally requires ≥1 credited attestation
   for the confirmable anchor's OWN epoch, at BOTH coordinates that classify confirmation (the
   reorg-gate `DnsState` advance and the PALW beacon/clause-6 `palw_dns_confirmation`) — a dead
   branch, whose window score rides the shared pre-fork segment while nobody attested ITS
   anchor, can never confirm at all. The trap from this report is thereby eradicated at the
   source, not merely escaped. Deployment note: the flag went live hours after the testnet-21
   genesis while the ledger provably held zero attestations and zero confirmed anchors, so
   replay of the pre-flag blocks is byte-identical under both values; on any net that HAS
   confirmed anchors this flip is re-genesis-only (legacy presets stay `false` — see the field
   doc for the full liveness analysis: Bootstrap unchanged, full outage already halts
   confirmation via the bounded window, a skipped epoch just delays the advance one epoch).

## Recurrence guards carried over from the 20-migration

Everything from `docs/testnet-20-migration.md` §guards still applies, re-pointed at testnet-21:
the live tripwire (`pcpb_palw_network_selection` pins thresholds + the whole
`consensus_identity_hash`, now including the PCPB windows), the startup identity log line /
`consensusParamsHash`, and the point-of-view provenance failures that are never persisted as
`StatusInvalid`. The tripwire's clause (c) (fork-choice-only fields may be edited with a re-pin,
per the `6ff40d4` precedent) is documented in the test itself.
