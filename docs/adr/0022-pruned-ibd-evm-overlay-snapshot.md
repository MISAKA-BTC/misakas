# ADR-0022 — Pruned-IBD support for the EVM lane and the DNS/PoS-v2 overlay

Status: **Accepted (implementing)** — 2026-06-15
Supersedes: none. Depends on: ADR-0020 (Selected-Parent EVM Lane), ADR-0018 (PoS-v2
economics), DNS v3 Canonical Lagged Anchor, ADR-0007 Phase 3 (BLAKE2b-SHA3 PoW).

## 1. Problem

A fresh node joins via **headers-proof IBD** (`IbdType::DownloadHeadersProof`): it
downloads a pruning proof + the pruning point's anticone, imports the pruning point's
**L1 UTXO set**, then downloads block bodies forward and lets the virtual processor
validate them.

`VirtualStateProcessor::import_pruning_point_utxo_set` imports **only** the L1 UTXO
multiset (verified against `header.utxo_commitment`) plus the pruning point's
`StatusUTXOValid`. It does **not** import:

* the pruning point's **EVM execution state** (`evm_header_store[pp]`,
  `evm_state_store[pp]`, `CanonicalEvmHeads.finalized`), nor
* the **DNS/PoS-v2 overlay state** as of the pruning point (`stake_bonds_store`,
  `reserve_balance_store[pp]`, the live `epoch_accumulator` tallies, and the
  `rewarded_epochs_store` window).

On networks where the EVM lane and/or the overlay are genesis-active
(testnet/devnet have `evm_activation_daa_score = 0`; **all four** nets have
`dns_params = Some(..)`), the first post-pruning chain block fails validation:

* `resolve_virtual` → `evm_chain_context_step` → `evm_execute_acceptance_with_parent`
  finds no `evm_header_store[pp]`, treats the pruning point as the *implicit
  EVM-genesis parent* (empty snapshot, `evm_number = 1`), re-executes the child
  against the wrong base, and the recomputed `evm_commitment_root` mismatches the
  header → `EvmValidateError::CommitmentMismatch` → `StatusDisqualifiedFromChain`.
* Even with EVM fixed, coinbase `c == v` fails next: `verify_expected_utxo_state`
  recomputes validator rewards / reserve drip / deferred quality bonus from empty
  overlay state.

Disqualification is inherited by all descendants → `0 valid chain blocks` → the node
loops, unable to extend the canonical chain. (This is independent of, and downstream
of, the earlier `accepted_txs_of_chain_block` empty-acceptance panic fix.)

**Forcing full sync is not an option:** a fresh node has no
`highest_known_syncer_chain_hash`, so `determine_ibd_type` can only choose
`DownloadHeadersProof`; and serving peers have *pruned the historical bodies*, so
there is nothing to full-sync from. Pruned IBD is the only way new nodes can join,
so it must import the auxiliary state.

## 2. Decision

At the pruning point, transfer and import the auxiliary consensus state that
post-pruning blocks read, and make it **trustlessly verifiable** by committing to it
in the L1 header.

* **EVM state** is already committed: `Header::evm_commitment_root` commits to the
  pruning point's `EvmExecutionHeader`, whose `state_root` is the keccak-MPT root of
  the full account state. The EVM snapshot is therefore verified against existing
  header fields — **no new header field, no re-genesis for EVM**.
* **Overlay state** is *not* committed in the L1 header today. We add a new committed
  header field **`overlay_commitment_root`** that commits to the canonical
  **OverlaySnapshot** as-of each block. This is a hashing change → **re-genesis on all
  four networks** (chosen deliberately over forward-only verification; see §7).

New P2P messages stream both snapshots during headers-proof IBD; consensus import
functions verify and persist them before block bodies are processed.

## 3. The OverlaySnapshot and its commitment

### 3.1 Definition

`OverlaySnapshot(B)` is the minimal, complete set of overlay rows required to
validate the selected-chain descendants of `B` without access to `past(B)`:

| Component | Source store | Why a descendant needs it |
|---|---|---|
| `bonds: Vec<StakeBondRecord>` | `stake_bonds_store` (prefix 196) | seed for `initial_active_bond_view()`; reward attestation resolution |
| `reserve_balance: u64` | `reserve_balance_store[B]` (prefix 200) | `balance_after(selected_parent=B)` drives the security-reserve drip → coinbase |
| `epoch_tallies: Vec<(u64, EpochTally)>` | `epoch_accumulator_store` (prefix 198) | deferred quality-bonus payouts reference `EpochTally(E − lag)` |
| `rewarded_window: Vec<(BlockHash, Vec<(TransactionOutpoint, u64)>)>` | `rewarded_epochs_store` (prefix 197) | cross-block reward uniqueness over `reward_uniqueness_window_blocks` |

The `epoch_tallies` set covers every epoch whose deferred payout can still land in
`(B, B + finalization]`; the `rewarded_window` set covers the selected-chain blocks in
`(B − reward_uniqueness_window_blocks, B]`. Both windows are bounded
(`reward_uniqueness_window_blocks = 600`, `epoch_length_blocks = 100`) and far shorter
than `pruning_depth`, so on a live node near the sink they never reach the pruning
point — the windows only cross `pp` during the post-import catch-up of `(pp, pp+window]`.

### 3.2 Canonical encoding (FROZEN)

All components are serialized with **borsh** in a canonical, sorted order:

* `bonds` sorted by `bond_outpoint` (txid bytes, then index).
* `epoch_tallies` sorted by epoch; `EpochTally.included` already sorted by
  `validator_id`.
* `rewarded_window` sorted by block hash; each block's `(outpoint, epoch)` list sorted.

`OverlaySnapshot::commitment_preimage()` = borsh of the sorted struct.
`overlay_commitment_root(B)` = `blake2b_512_keyed(MISAKA_OVERLAY_COMMITMENT_CONTEXT,
preimage)` → `Hash64`, where `MISAKA_OVERLAY_COMMITMENT_CONTEXT = b"OverlayCommit64"`
(distinct from `EvmCommitment64` / `EvmPayload64`).

The empty snapshot (genesis: no bonds, reserve 0, no tallies, no rewarded rows) hashes
to a fixed constant `OVERLAY_EMPTY_COMMITMENT`.

### 3.3 Where it is computed and verified

`overlay_commitment_root(B)` commits to overlay state **after** applying `B`'s
selected-chain effects — exactly the same "post-state" convention as
`utxo_commitment` and `evm_commitment_root`:

* **Construction:** the block-template builder fills `header.overlay_commitment_root`
  from the virtual overlay state (consensus computes it; miners/validators just carry
  it, like `utxo_commitment`).
* **Validation (`c == v`):** `verify_expected_utxo_state` recomputes the snapshot from
  the data it already gathers per chain block (bond view, reserve, epoch tallies,
  rewarded keys) and checks the digest equals `header.overlay_commitment_root`,
  yielding a new `RuleError::BadOverlayCommitment`. A mismatch disqualifies the block
  like any other `c == v` failure.

The first implementation recomputes the snapshot directly from the per-block overlay
data (cheap on the current validator-sparse networks). An incremental bond-set MuHash
+ windowed MuHashes are a future optimization (§9) and do not change the committed
value.

## 4. Header change

Add `overlay_commitment_root: Hash64` to `consensus_core::header::Header`, appended to
the canonical preimage in `hashing::header::write_header_preimage` **after** the EVM
conditional block. Because the overlay is genesis-active on every network, the field is
committed **unconditionally** (for all header versions); there is no pre-overlay era to
gate against. Builder `Header::with_overlay_commitment` mirrors `with_evm_commitment`;
`new_finalized` / `from_precomputed_hash` default it to `Hash64::default()` and the
template/genesis paths set it explicitly. Frozen byte order (hard fork to change):
`… pruning_point, [v2+: evm_payload_hash, evm_commitment_root], overlay_commitment_root`.

## 5. P2P / IBD protocol

New messages (proto field IDs in the 60+ range, protocol-version gated):

* `RequestPruningPointEvmState { pruning_point }` →
  `PruningPointEvmStateChunk { evm_header?, account_pairs[] }` … `DonePruningPointEvmStateChunks`
  (header in the first chunk; account snapshot streamed in bounded chunks).
* `RequestPruningPointOverlayState { pruning_point }` →
  `PruningPointOverlayStateChunk { … }` … `DonePruningPointOverlayStateChunks`
  (bonds + reserve + epoch tallies + rewarded window, chunked).

Both follow the existing `RequestPruningPointUtxoSet` streaming pattern
(`v7/request_pruning_point_utxo_set.rs`), with a `RequestNext…Chunk` back-pressure ack
every `IBD_BATCH_SIZE`.

### 5.1 Import sequence (headers-proof IBD)

Spliced into the `IbdType::DownloadHeadersProof` arm, after `sync_new_utxo_set`
commits the L1 UTXO set and **before** body processing reaches `pp`'s children:

1. headers proof + anticone + trusted data (unchanged)
2. `sync_new_utxo_set` → `import_pruning_point_utxo_set` (unchanged; resolves virtual to `[pp]`)
3. **`sync_pruning_point_evm_state`** → `import_pruning_point_evm_state(pp, evm_header, snapshot)`
4. **`sync_pruning_point_overlay_state`** → `import_pruning_point_overlay_state(pp, snapshot)`
5. `sync_missing_block_bodies` → virtual processor validates `(pp, tip]` (now succeeds)

### 5.2 Verification on import

* EVM: `evm_header.commitment_root() == headers_store[pp].evm_commitment_root`
  (pure, secp-free) **and** keccak-MPT state root of the imported snapshot
  `== evm_header.state_root` (via `kaspa-evm`, `--features evm`). Then write
  `evm_header_store[pp]`, `evm_state_store[pp]`, and set `CanonicalEvmHeads.finalized = pp`.
* Overlay: `OverlaySnapshot::commitment_root() == headers_store[pp].overlay_commitment_root`.
  Cross-check each bond's `bond_outpoint` exists in the imported UTXO set with the
  recorded `amount`. Then write `stake_bonds_store`, `reserve_balance_store[pp]`,
  `epoch_accumulator_store`, and `rewarded_epochs_store` window rows.

### 5.3 Protocol-version fallback

Bump `PROTOCOL_VERSION`. If a peer below the required version is the only syncer on an
EVM/overlay-active network, headers-proof IBD returns a clean `ProtocolError` (the node
declines rather than building a chain it cannot validate). This is also the
defense-in-depth guard that prevents the looping/broken state during rollout.

## 6. Non-goals / unchanged

* No change to block-validation rules for blocks a node can already process; this is an
  IBD-bootstrap path plus one new committed header field.
* The unbounded growth of the EVM stores (they are **never pruned** today — confirmed:
  no `evm_*_store.delete_batch` in `pruning_processor`) is **out of scope** here and
  tracked separately.

## 7. Alternatives considered

* **Forward-only verification (no header commitment, no re-genesis):** import the
  overlay snapshot from the peer and rely on the existing coinbase `c == v` check of
  the first post-`pp` blocks to reject a wrong snapshot. Cheaper and avoids re-genesis,
  but the overlay seed is only *eventually* verified and a malformed-but-quiescent
  snapshot can be accepted briefly. **Rejected** in favor of the committed-root design
  for full trustlessness at the pruning point (matches how `utxo_commitment` /
  `evm_commitment_root` already work).

## 7a. Reward-split change (bundled with the re-genesis)

**Supply is unchanged: 30B total = 15B premine + 15B additional issuance** (an
earlier 40B/25B variant was reverted — total emission stays 15B over 20 years at
5%/yr). The re-genesis only **rebalances the existing block subsidy toward
validators**: `subsidy_validator_bps` **2500 → 3000** (validator share 25% → 30%),
with `subsidy_worker_base_bps` **6700 → 6200** absorbing the 5pt and
`subsidy_worker_inclusion_bps` kept at 800. Result per block subsidy: worker 70%
(62% base + 8% inclusion), validator 30%.

Rationale: validators provide the stake dimension of the 2-D DNS reorg defense —
the security backbone that replaces the abandoned ASIC-resistance (ADR-0007 Phase
3). Strengthening the validator incentive (without inflating supply) helps attract
the stake-depth that makes deep reorgs of DNS-confirmed anchors infeasible. The
EVM lane is executed by every node (not validators), so EVM does not justify a
larger validator share — the security role does. Applied to both the
`PRODUCTION_DNS_PARAMS` and `GENESIS_ACTIVE_DNS_PARAMS` full `fee_split` (the
bootstrap split is unchanged). Consensus rule change (coinbase split); ships with
the re-genesis; does not itself change the genesis hash (split is not a genesis
input).

## 8. Re-genesis impact

Adding `overlay_commitment_root` to the preimage changes every block hash, including
genesis (which commits `OVERLAY_EMPTY_COMMITMENT`). All four networks re-genesis:
new genesis block hashes; `utxo_commitment` and premine are unchanged (overlay empty at
genesis); coinbase marker is bumped to isolate the old chain; the startup
genesis-mismatch guard (`Consensus::new` asserts `past_pruning_points[0] == config
genesis`) is retained. EVM/UTXO commitments at genesis are unchanged.

## 9. Future work

* Incremental bond-set MuHash + windowed MuHashes for the epoch/rewarded components, so
  `overlay_commitment_root` is `O(mutations)` per block at mainnet bond-set scale
  (current direct recompute is fine for the validator-sparse testnet/devnet).
* Prune the EVM stores (bounded retention) and ship only the pruning point's snapshot.
