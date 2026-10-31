# ADR-v2-0012 — 状態root・pruning proof・snapshot同期

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 3本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0012](../rfc-v2/0012-authenticated-state-storage-and-ibd.md)

## 1. この文書の役割

pruning後も検証可能な状態と、そのcommitment・importの関係をまとめる。

## 2. 統合した判断の読み方

### 2.1

pruning point以後に必要なEVM等の補助stateをheader commitmentと照合して転送・importする判断を保存する。旧overlay snapshotは退役方針との適用境界を残す。[旧ADR-0022](../adr/0022-pruned-ibd-evm-overlay-snapshot.md)

### 2.2

pruning proofのheader検証は、抜き取りに置き換える旧案ではなく全件検証を維持し、費用は再利用・並列化等で下げる方向を継承する。[旧ADR-0041](../adr/0041-palw-pruning-proof-verification.md)

### 2.3

PALW V2 rootがcommitする親側stateと、当該blockのobject適用後stateを区別し、challengeとrootの循環を防ぐ。[旧ADR-0043](../adr/0043-palw-v2-state-root-ordering.md)

## 3. 維持する境界・後続改定

ADR-0041のsampling案は明示撤回部分として[ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md)に登録する。歴史的なPoW headerの検査方式を新row-streamの健全性証拠に流用しない。

## 4. 隣接する判断との分担

UTXO集合は[ADR-v2-0003](0003-utxo-accumulator-and-commitment.md)、新state map／row streamは[ADR-v2-0049](0049-authenticated-state-map-and-row-stream.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0022-pruned-ibd-evm-overlay-snapshot.md](../adr/0022-pruned-ibd-evm-overlay-snapshot.md) | L1–L240 | L12–L14 |
| [0041-palw-pruning-proof-verification.md](../adr/0041-palw-pruning-proof-verification.md) | L1–L449 | L12–L18 |
| [0043-palw-v2-state-root-ordering.md](../adr/0043-palw-v2-state-root-ordering.md) | L1–L213 | L12–L19 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0022-pruned-ibd-evm-overlay-snapshot"></a>

<details>
<summary>0022-pruned-ibd-evm-overlay-snapshot.md — 原文 L1–L240</summary>

[原文を開く](../adr/0022-pruned-ibd-evm-overlay-snapshot.md) · SHA-256: `e1412de18a73b20159d70cd57db031d08669c2b9632bbabaa9caf96fd6a8f649`

<!-- ADR_SOURCE_BEGIN:0022-pruned-ibd-evm-overlay-snapshot.md -->
~~~~~~~~markdown
# ADR-0022 — Pruned-IBD support for the EVM lane and the DNS/PoS-v2 overlay

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


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
~~~~~~~~
<!-- ADR_SOURCE_END:0022-pruned-ibd-evm-overlay-snapshot.md -->

</details>

<a id="source-0041-palw-pruning-proof-verification"></a>

<details>
<summary>0041-palw-pruning-proof-verification.md — 原文 L1–L449</summary>

[原文を開く](../adr/0041-palw-pruning-proof-verification.md) · SHA-256: `da3d00802a32b077eefa89ed585a3c11ee285532b45a1807f79f59c9770a19c6`

<!-- ADR_SOURCE_BEGIN:0041-palw-pruning-proof-verification.md -->
~~~~~~~~markdown
# ADR-0041: PALW pruning-proof verification — exhaustive and amortised, not sampled

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: **Landed.** Activates nothing on a shipped preset. Decision 1′ (the resident verification
agent, opt-in behind `MISAKA_PALW_AGENT=1`) and Decision 2 (bounded concurrency, `MISAKA_PALW_CONCURRENCY`,
default 1) are landed and measured; Decision 3 (the header cap) and Decision 4 (cheap checks before
the inference) were already landed. Decision 1 (sampling) is withdrawn as unsound.
Governs how a node validates a pruning-point proof and a trusted set on a network whose PoW is a
PALW inference (`pow_palw_activation` / `pow_palw_ollama_activation` active — testnet-11 and devnet
today).

> **Not the ADR-0041 that ADR-0036 supersedes.** The `main-backup-8107bfb-20260807` snapshot
> carries a *different*, non-ancestral ADR-0041 (`palw_spam` / `palw_algo4_accept` /
> `palw_compute_work_scale` / qwen-8.0 `mint.rs`), whose mechanism
> [ADR-0036](0036-palw-mainnet-activation-model.md) Decision 2 declines to port. That number was
> released to the live lineage by ADR-0036 Decision 1. This ADR is the live-lineage 0041 and has
> nothing to do with it.

Date: 2026-08-18
Relates to: ADR-0007/0008 (the Layer-0 512-bit PoW the proof headers carry), ADR-0038/0039 (PALW
is the consensus work; there is no hash lane), the pruning-proof machinery in
`consensus/src/processes/pruning_proof/` (`build.rs`, `validate.rs`, `apply.rs`, `mod.rs`), the
Layer-1 finalizer `consensus/pow/src/palw.rs`, and the 2026-08-18 PALW audit finding H1.

## Context — one inference per proof header does not scale, and the block interval cannot fix it

A pruning-point proof lets a fresh node accept the current pruning point without replaying history.
It carries, per block level, the headers in `future(root) ∩ past(tip)` for a root at blue-depth
`≈ 2·pruning_proof_m` below the level tip (`build.rs:353`, `validate.rs:291`). The validator checks
each header's PoW: `calc_block_level_check_pow_layer0` at `validate.rs:205`, and again per
trusted-set block at `apply.rs:69`.

On a hash network that check is microseconds. On a PALW network it is **one full LLM inference** —
`StateLayer0::check_pow_layer0` → algo 4 → `native::tag_for_seed`, which spawns `palw-worker
--mode verify` (`palw.rs:257-260`). Every such call is serialized process-globally on a single
`SPAWN_GATE` mutex (`palw.rs:186`), and each spawn re-reads and SHA-256s the whole 1.28 GiB pinned
GGUF. Measured on the fleet: ~12 s per header on an EPYC core, ~26 s on the slowest host.

Two consequences, one for honesty and one for safety.

**Honest cost.** The proof's own size formula is `estimate_proof_unique_size ≈ (⌊log₂(history/m)⌋ +
1)·m` (`mod.rs:283`). At testnet-11 (`m = 1000`, ~262,800 blocks/year at 120 s) an honest
one-year proof is ≈ 9,000 distinct headers. At 12 s serialized that is **30 hours** of stalled
IBD; 33 hours on the slowest host. The tag cache does not save this: it is keyed by seed, most
proof headers have distinct seeds, and it clears wholesale at `TAG_CACHE_MAX = 8_192`
(`palw.rs:178, 211`), below the honest distinct-header count — so it thrashes rather than
deduplicates.

**The block interval is the wrong lever, and lengthening it makes two other things worse.** Proof
size is `Θ(m·log(history))` — logarithmic in history length, which the interval scales *linearly*.
Computed from the code's own formula:

| interval | 1-year blocks | proof headers | verify @12 s |
|---|---|---|---|
| 120 s | 262,800 | ~9,000 | 30.0 h |
| 300 s | 105,120 | ~7,000 | 23.3 h |
| 600 s | 52,560 | ~6,000 | 20.0 h |
| 3600 s | 8,760 | ~4,000 | 13.3 h |

Five minutes buys a **22 %** reduction, not a fix; halving the proof would take squaring the
interval. And lengthening the interval shrinks the pruning horizon (`pruning_depth` 1,144 → 892 at
120 s → 300 s), which tightens the per-class DAA fold's memory bound (ADR-0038/this branch) and
changes finality depth — costs paid network-wide to remove 7 hours from a one-time sync. The
interval is the steady-state load lever (one 12 s inference per 120 s block ≈ 10 %); it is not the
burst lever.

**Safety cost (audit H1).** Nothing caps the *number* of headers. The pre-loop checks are on the
level count only (`validate.rs:104`), the wire limit is `P2P_MAX_MESSAGE_SIZE = 1 GiB`, and header
PoW is public — so a peer can replay the network's own historical headers into a level and buy the
victim tens to hundreds of hours of serialized inference for a few MB of bandwidth. Header PoW is
public, so "the padding must be valid PoW" is not a barrier: the network's real headers are valid.

## The decision

**Pruning-proof PoW verification stays EXHAUSTIVE.** The cost is removed by amortising the
per-header overhead and by parallelism, not by checking fewer headers — sampling was drafted here and
withdrawn as unsound (below). Four decisions, in order of how much each changes the numbers.

### Decision 1 — WITHDRAWN: sampling the PoW checks is UNSOUND in this proof structure

PoW header 検証の sampling は実装しない。一つの未検証 header だけで任意に大きい work を主張でき、少数 sampling では重い偽 chain を十分に排除できないため。検証は全件のままとする。

### Decision 1′ — Amortise the per-header cost (this is the real lever)

The 12 s is not 12 s of inference. Each tag spawns a fresh `palw-worker` process which re-reads and
SHA-256s the whole 1,280,835,840-byte pinned GGUF and reloads the model, then runs an inference the
module itself quotes at ~1–3 s. The worker's own source already names the fix and the reason it was
deferred rather than dismissed: *"Costs a full read of the 1.2 GB file per job process; the
persistent agent (P1) amortizes it, correctness does not wait for it"* (`misaka/palw/worker/src/
main.rs:419`).

A persistent verification agent — one process, model resident, pin checked once at startup, seeds
fed over a framed protocol — removes the SHA-256, the model load and the spawn from the per-header
cost, leaving the inference. That is a constant-factor win of roughly 4–6× at the fleet's measured
numbers, and it costs NO security argument: the tag is a pure function of the seed, every header is
still verified in full, and the artifact pin is still the full read (once, not per header).

The pin's integrity is what makes this safe to amortise and must not be weakened in the process: the
audit trail records a v1 `(path, size, mtime)` cache that let any same-sized model pass the pin, on
the consensus PoW path (`main.rs:422-431`). The persistent agent re-reads and re-digests at startup
and holds the model immutably for its lifetime; it must not reopen the path, and a model file that
changes underneath it must be a hard failure, not a re-read.

#### Implementation 2026-08-18: LANDED and measured

Measured on this machine with the real pinned model
(`Qwen3.5-2B-Q4_K_M.gguf`, `--n-predict 16`), and the numbers settle the design question:

| | measured |
|---|---|
| one-shot `--mode verify`: model load | **7.44 s** |
| one-shot: the inference itself | **0.18 s** |
| resident agent: model load (warm page cache) | 0.28 s |
| resident agent: the inference itself | 0.16 s |

**Overhead is ~97 % of a one-shot verification.** Decision 1′ is not a marginal optimisation; the
inference is a rounding error next to the artifact read and the model load.

The equivalence the whole design rests on was also **proved empirically, not argued**: a resident
agent holding the model produced a projection document for its first job that is **byte-identical to
the one-shot document, in every field** — including all five the PoW tag is derived from
(`output_commitment`, `gemm_trace_root`, `operation_schedule_commitment`, `prefill_tokens`,
`decode_tokens`). A resident model computes the same tag.

**The blocker was self-inflicted and is now resolved.** Running a second job on one context failed —
`shim_ctx` read damaged afterwards (`lctx` = `0x8078` or `0x0`, `n_vocab` = 0). The cause was one
line: production `execute` closes the context it opens, and the extraction that produced a
context-taking `execute_on_context` took the close with it. Every job freed its CALLER'S context and
the next job read freed memory. Fixed by giving the close to the owner: `execute` opens, delegates
and closes; `execute_on_context` never closes.

Two hypotheses were raised before that and both were disproved by measurement. They are recorded
because each looked convincing and each would have sent an implementer somewhere useless:

* *"`execute` corrupts the heap."* No. Two independent `execute()` calls in one process are clean; a
  probe reading `shim_n_vocab` around every shim call of one job showed the struct intact throughout
  (`n_vocab = 248320`); Guard Malloc over the unmodified one-shot found no overflow.
* *"It is the Metal backend's resource lifecycle"* — suggested by an exit-time
  `ggml-metal-device.m:657: GGML_ASSERT([rsets->data count] == 0)`. No: a CPU-only llama tree was
  configured and built out of tree (`-DGGML_METAL=OFF -DGGML_BLAS=OFF -DBUILD_SHARED_LIBS=OFF`,
  linked via `MISAKA_PALW_CPU=1`, leaving the pinned Metal build untouched) and the same failure
  occurred with no Metal device in the process. That assertion was the probe leaking a context — a
  second symptom of the same self-inflicted bug.

**The equivalence is reproduced, not claimed.** Three jobs on ONE context with
`shim_reset_context` between them (backend synchronize, then `llama_memory_clear`) produce
projections byte-identical to each other AND to a fresh one-shot process, across all five fields the
PoW tag derives from. The one-shot's own output is unchanged by the refactor, byte for byte.

**Both halves are now landed.** `palw-worker --mode pow-agent` holds the model and serves jobs over
newline-delimited JSON; `kaspa-pow`'s `native::resident` is the validator-side child that drives it.
Measured on this machine, 12 distinct seeds, `--n-predict 128`:

| | per seed (median) | 12 seeds |
|---|---|---|
| one-shot, a process each | 3.28 s | 39.2 s |
| resident agent | **0.57 s** | 9.6 s (2.70 s of it the one model load) |

**5.7× per seed** on that machine. The ratio does **not** transfer to the fleet, and the reason is
the whole point of the section below.

#### Fleet measurement 2026-08-18 (`misaka-ibm`, 8-vCPU KVM EPYC @ 2.0 GHz, **CPU class**)

The dev machine runs the **Metal** class (`03a3c66c…`); the fleet runs the **CPU** class
(`0fc67b41…`). Those are different determinism classes, and the cost structure inverts between them.
Per header, one-shot, measured with the node's own workload stopped:

| | dev (Metal class) | fleet (CPU class) |
|---|---|---|
| pin: SHA-256 of the 1.28 GiB artifact | — | **1.56 s** |
| model load | 2.70 s | 3.08 s |
| the inference itself | 0.18–0.57 s | **13.9–16.0 s** |
| one-shot total | ~3.3 s | **~18.6–20.7 s** |
| resident agent (inference only) | 0.57 s | **13.9–16.0 s** |
| **speedup from Decision 1′** | **5.7×** | **~1.3×** |

The pin is not free and is not in the "model loaded" figure: `pinned_model_path_v2` SHA-256s the
whole artifact on **every spawn**, before `execute` starts its clock. The agent still removes it,
along with the model load — 4.6 s of the fleet's ~19 s.

But on the CPU class the **inference dominates**: 16 s of a ~20 s header. The 97 %-overhead premise
this ADR opened with was measured on a GPU-class host where the inference is a rounding error; on
the CPU class the overhead is ~23 %, so amortising it buys ~1.3×, not ~5×. Decision 1′ is still a
real win and still costs no security argument — it is simply a much smaller one exactly where the
fleet lives.

Four design points, each of which is a way this could have been got wrong:

* **One reader.** The agent returns the same projection document the one-shot prints, and both are
  read by one function (`native::tag_from_doc`). A tag that depended on which transport delivered
  the document would be a consensus bug; the way not to have one is not to have a second parser.
* **Marked lines, not length-prefixed frames.** llama.cpp and ggml are third-party code sharing the
  child's stdout. One stray byte desynchronises a length-prefixed stream silently and permanently;
  with a marker, noise is skipped instead. The v2 compute path can use Borsh frames because its
  worker exits after one job — a resident one cannot.
* **The agent is an accelerator, never an authority.** It runs only under `MISAKA_PALW_AGENT=1`, and
  every failure — spawn, handshake, timeout, a frame out of order, a child that died — falls back to
  the one-shot path that ships today. So it can change how fast a tag arrives and not which tag, and
  it cannot wedge a node the old path would have synced. A test pins the sharpest case: with the
  agent enabled and the worker binary missing, the caller still gets the same `PalwUnavailable`,
  naming the same path.
* **A dead child costs a delay, not a tag.** A resident process can be OOM-killed between two seeds,
  and the validator finds out by writing to a pipe whose far end is gone — one `SIGPIPE` away from
  killing the node instead of returning an error. Tested by killing the agent mid-run: the next seed
  still produces the right tag.

Two measured facts worth recording because both are counter-intuitive:

* **The per-seed entropy is `gemm_trace_root`, not the generated text.** Across distinct seeds the
  greedy OUTPUT is the same generic continuation, so `output_commitment`, `decode_tokens` and the
  schedule commitment are all constant; the tag varies because the digest of the full logits of
  every decode call varies. Anyone testing this path who asserts distinctness on the output will
  write a vacuous test — the first version of ours was exactly that.
* **The overhead is not the inference.** At 0.57 s served versus 3.28 s spawned, ~83 % of a one-shot
  verification on a WARM machine is still setup. The 97 % figure above was the cold case.

What remains of the 30 h → 45 min claim is Decision 2 (concurrency), which is deliberately untouched
here: the agent runs inside the existing `SPAWN_GATE`, so exactly one inference is in flight either
way and the two paths above were measured under the same policy.

Three suspects were eliminated along the way and are recorded so nobody re-investigates them: the
worker's own heap handling, a buffer overflow, and the Metal backend.

### Decision 2 — Parallelise verification during IBD — **landed, and it buys far less than drafted**

The `SPAWN_GATE` mutex serialised every inference in the process, which is right for steady-state
block validation (one at a time, low load) and wrong for a sync burst. It is now a counting
semaphore whose permit count is `MISAKA_PALW_CONCURRENCY` (default **1**, i.e. exactly the old
behaviour), the resident agent is a pool that grows to that count, and the two proof loops feed it:

* `validate.rs` computes each level's header PoW in **bounded batches**. The walk stays strictly
  sequential — every line after the PoW checks mutates stores whose order *is* the validation — and
  errors stay in header order, so nothing about what is accepted changes. The batch is bounded, not
  whole-level, because a PoW-derived error can stop the walk: up to `batch − 1` inferences may be
  spent on headers the walk never reaches, and that waste must be a constant rather than a level.
* `apply.rs` batches both of its sites (the trusted set, and the level recompute for every distinct
  proof header). Neither loop has an early exit once its shape gate has passed, so there is **no**
  speculation there at all — the same work, in less wall clock. Hoisting the gate out of the walk
  also means a proof that fails it now aborts having written nothing to the headers store, where
  before it had written every header up to the bad one.

**The drafted "8-way concurrency ≈ 8×" is not supported by measurement, and on the fleet host
concurrency is worse than useless.** Two hosts, both with `CPU_THREADS = 4` so N workers occupy 4N
threads.

**Fleet (`misaka-ibm`, 8 vCPU, CPU class), node stopped for the measurement:**

| N | threads | inference per job | throughput | vs serial |
|---|---|---|---|---|
| 1 | 4/8 | 13.9 s | 0.072 /s | 1.00× |
| 2 | 8/8 | **73.6 s** | 0.027 /s | **0.38×** |

Two concurrent inferences make each one **5.3× slower** and cut aggregate throughput to **0.38× —
2.6× WORSE than running them one after another**. N = 3 and N = 4 did not finish inside a 400 s cap.
This is not oversubscription: 2 workers × 4 threads is exactly the 8 available cores, `vmstat`
reports **`st = 0`** (the hypervisor is not taking the CPU), and the guest has one NUMA node. Cores
are not the scarce resource. Memory bandwidth is — and on a KVM guest that bandwidth is shared with
co-tenants, which CPU-steal accounting does not show.

**Dev (12-core M-series, Metal class),** N resident agents, 6 jobs each:

| agents | threads | jobs/s | speedup | per-job latency |
|---|---|---|---|---|
| 1 | 4/12 | 1.17 | 1.00× | 0.86 s |
| 2 | 8/12 | 1.62 | 1.39× | 1.24 s (1.44×) |
| 3 | 12/12 | 2.06 | **1.77×** | 1.46 s (1.70×) |
| 4 | 16/12 | 2.07 | 1.77× | 1.93 s (2.26×) |
| 6 | 24/12 | 2.07 | 1.77× | 2.90 s (3.38×) |

Throughput **saturates at 1.77×** and adding agents past that buys exactly nothing — beyond 3, every
extra agent only lengthens everyone's latency in proportion.

The binding constraint is **not cores**, and the table says so: at 2 agents only 8 of 12 cores are
busy, yet per-job latency is already 1.44× the solo figure. If cores were the limit, two agents
would not slow each other at all. What is shared and saturated is memory bandwidth — batch-1 decode
streams the entire ~1.28 GiB weight set per token, so concurrent models compete for the memory bus
rather than for arithmetic. (Consistent with the numbers and with how batch-1 inference is known to
behave; not separately confirmed with a bandwidth counter.)

Even there it saturates at 1.77×, and past 3 agents every extra one only lengthens everyone's
latency. The binding constraint is the same on both hosts and it is **not cores**: at 2 agents on the
dev box only 8 of 12 cores are busy, yet per-job latency is already 1.44× solo. If cores were the
limit, two agents would not slow each other at all. What is shared and saturated is memory
bandwidth — batch-1 decode streams the entire ~1.28 GiB weight set per token, so concurrent models
compete for the memory bus rather than for arithmetic. (Consistent with every number above; not
separately confirmed with a bandwidth counter, which a KVM guest largely cannot read anyway.)

Three consequences worth stating plainly:

* **`CPU_THREADS = 4` is pinned by the determinism class**, so a host cannot trade worker count
  against threads per worker to dodge this — the shape string is part of the runtime identity.
* **The guess that a server would scale better was wrong.** It was the honest thing to expect and the
  measurement refuted it: the 8-vCPU KVM EPYC scales *negatively* where the laptop at least reached
  1.77×. Nothing about "server hardware" should be assumed here.
* **Therefore the default stays 1, and raising it requires a measurement, not a policy.** The
  reproducer is in-tree (`palw_agent_concurrency`, and the fleet sweep this section records). A host
  that has not been measured must be assumed to be the `misaka-ibm` case, where `N = 2` would make a
  sync 2.6× slower.

### Decision 3 — Hard header cap, enforced before any inference

Independent of sampling, the validator caps the total header slots it will consider, derived from
its OWN params — never from the proof's claimed `daa_score`. This closes the amplification (audit
H1): a proof larger than any honest builder could produce is rejected before a single worker is
spawned. The cap is generous by design — its job is to bound the pathological case (a 1 GiB message
of junk), not to be tight; tightness is what sampling provides. The interim cap this ADR ships is
`(max_block_level + 1) · 2 · pruning_proof_m` total header slots, the builder's own per-level
working-set capacity (`build.rs:197, 366`) times the level count. A proof exceeding it is refused
as oversized, not run.

The same discipline applies to the trusted set (`apply.rs`): its length is capped before the loop
that runs one inference per block.

### Decision 4 — Cheap checks gate the inference — **already landed**

Every check in the per-header loop that does **not** consume the PoW result runs first, so a header
that fails a shape, level, or ancestry rule never buys an inference. This extends the P0-3 ordering
fix (already applied to the ordinary header path at `header_processor/processor.rs:318`) to the
proof paths — `validate.rs`'s loop and both of `apply.rs`'s.

Both were already in this shape when this ADR's implementation started, under the P0-1/P0-2 audit
work: `check_proof_header_shape` runs immediately before every `calc_block_level_*` call on those
paths. Nothing was needed here.

The batching in Decision 2 had to be built so as not to quietly undo it, which is the part worth
knowing: the batch scan applies the same gate and **stops at the first header that fails it**, so a
header the walk will reject is never pulled into a batch, and neither is anything behind it. A batch
that ignored the gate would have re-introduced exactly the amplification this decision removes.

### Live corroboration, and a bound this ADR does not actually enforce

While measuring the above, the same host turned out to be running the scenario for real: two
testnet-11 nodes (`palw-soak-node`, `palw-ibd-join`), both on the PALW preset, one syncing from
scratch since 00:31. Its own `[ibd-perf]` counter reports per-header validation time, which on this
network is the inference. **n = 1,242 headers**, one sync:

| | ibd-join (syncing) | soak (steady state) |
|---|---|---|
| p25 | 14.3 s | — |
| **median** | **19.8 s** | **35.0 s** |
| p75 | 32.5 s | — |
| mean | 29.1 s | 50.3 s |
| max | **303.5 s** | — |

The mode is 10–15 s (375 of 1,242 samples) with a long right tail. That floor is the same number the
controlled measurement produced with everything else stopped (13.9 s), and that agreement is the
corroboration: **an uncontended header costs ~12–14 s on this host; everything above it is
contention.**

**The contention is self-inflicted, and `MISAKA_PALW_CONCURRENCY` does not prevent it.** The
semaphore is per PROCESS. Two PALW node processes on one host are two concurrent inferences no
matter what the knob says — precisely the configuration measured at 0.38× throughput. The effect is
in the table: the syncing node's median is 40 % above its own uncontended floor, and the soak node,
competing with a node that never pauses, sits at a 35 s median and a 50 s mean.

Decision 2's bound was therefore real but **incomplete**: it bounded a process, and the resource it
protects is the host. **Now closed.** `MISAKA_PALW_LEASE_DIR` points every PALW process on a machine
at one directory of slot files, and a permit additionally requires an exclusive `flock` on one of
`MISAKA_PALW_CONCURRENCY` of them — so the bound covers the host.

`flock` rather than a PID file or a named POSIX semaphore for one reason: **the kernel releases it
when the holder dies.** A crashed node must not permanently consume a slot, and both alternatives
leak one. A lease directory that cannot be used warns once and leaves the per-process bound in
place — a performance control failing open, deliberately, because refusing to validate over a lock
directory would wedge the node. Opt-in, like everything else here; the default is unchanged.

The mechanism is unit-tested in CI without a model: `flock(2)` associates a lock with the open file
DESCRIPTION rather than the process ("An attempt to lock the file using one of these file descriptors
may be denied by a lock that the calling process has already placed via another file descriptor"), so
threads holding independent descriptions contend exactly as separate processes do. The test asserts
both directions — never more than N holders, and that N is actually reached, so a lease that
degraded into a global mutex would fail it.

### The number that decides whether a network is syncable at all

Neither this ADR nor its 30-hour headline ever stated the ratio that actually matters: **how fast a
node syncs versus how fast the chain grows.**

Measured on testnet-11, same host, same window: the syncing node processed **1,235 headers in
7 h 34 m ≈ 163 headers/hour**, while the chain produced blocks at ~90 s intervals over the sampled
window ≈ **40 blocks/hour**.

**The margin is ~4×**, and would be ~6.5× if the node did not share its host (259 headers/hour at the
uncontended 13.9 s). A node a day behind needs about six hours to catch up.

That margin, not the absolute hours, is the property to defend. It is roughly
`block_interval / per_header_cost`, and below 1 **no node can ever sync and the network is closed to
new participants**. It falls if the block interval shortens, if the model gets slower, if the host
gets busier, or if a second PALW node is co-located. Two of those four are configuration choices
being made on this host today.

## Where this leaves the numbers, honestly

Exhaustive verification stays. The honest one-year testnet-11 proof is ~9,000 headers, and the three
landing decisions compose multiplicatively rather than changing the exponent:

Everything below is now measured **on the fleet host itself** (`misaka-ibm`, CPU class, its node
stopped), not composed from a dev machine's ratios:

| | per header | 9,000 headers |
|---|---|---|
| one-shot, serial — what ships | ~18.6–20.7 s | **46–52 h** |
| + resident agent (Decision 1′, **1.3×** measured) | ~13.9–16.0 s | **35–40 h** |
| + concurrency (Decision 2, **0.38×** measured — refused) | — | worse; stays at N = 1 |

**The headline is 46–52 h → 35–40 h.** Not 30 h → 45 min, and not 30 h → 3 h. Both of this ADR's
earlier numbers were wrong in the same direction, and for the same reason: they were composed from a
GPU-class dev machine where the inference is a rounding error, applied to a fleet baseline that was
itself an estimate. On the CPU class the inference *is* the cost — 16 s of a ~20 s header — so
amortising the fixed 4.6 s buys 1.3×, and running two at once costs 2.6×.

Note the baseline moved too: this ADR opened with "~12 s per header" and the measured figure is
~19 s quiet, ~30 s while the node is also validating. A from-genesis sync of a year of
testnet-11 history on this host class is a **multi-day** operation, and the honest lever left is not
in this ADR — it is the cost of one inference, i.e. the model and the class.

45 minutes for a one-time sync of a year of history is a different proposition from 30 hours, and it
is reached without weakening any security argument. It is still `O(m·log(history))` inferences, so it
grows with history logarithmically and with the model's cost linearly — if the model gets slower,
this gets worse, and the mitigation is the agent's throughput, not the protocol.

The interim cap (Decision 3) is orthogonal to all of this: it bounds the ATTACKER's amplification,
not the honest cost. It landed first because it has zero liveness risk.

## Consequences

* IBD on a PALW network goes from ~46–52 h to ~35–40 h on the measured fleet host with Decision 1′
  enabled, with NO change to what is accepted — every header is still verified in full. Decision 2
  is landed but must stay at its default of 1 there, because 2-way concurrency measured 2.6× WORSE
  than serial. Both of this ADR's earlier headlines (45 min, then 3 h) were composed from a
  GPU-class dev machine and did not survive contact with the fleet.
* The remaining cost is the inference itself, which no scheduling decision can amortise. If a
  multi-day from-genesis sync is unacceptable, the levers are the model, the class, or shipping a
  trusted checkpoint — none of which are in this ADR's scope.
* No consensus rule changes; no preset changes; the fingerprint does not move. Every change here is
  a local sync policy.
* Nothing here rests on proof acceptance being a local decision, because nothing here samples. That
  property was load-bearing for the withdrawn Decision 1 and is now merely true.
* The remaining exposure is throughput, not soundness: a node whose verification agent is slow syncs
  slowly. That is a operational property with a visible symptom, unlike a false-accept probability,
  which is why exhaustive-and-amortised is preferred over sampled-and-fast.
~~~~~~~~
<!-- ADR_SOURCE_END:0041-palw-pruning-proof-verification.md -->

</details>

<a id="source-0043-palw-v2-state-root-ordering"></a>

<details>
<summary>0043-palw-v2-state-root-ordering.md — 原文 L1–L213</summary>

[原文を開く](../adr/0043-palw-v2-state-root-ordering.md) · SHA-256: `17ed2f04ccbd3907c279f598c2bde040182ee13d52bea8d7b163d9b8e00cfa88`

<!-- ADR_SOURCE_BEGIN:0043-palw-v2-state-root-ordering.md -->
~~~~~~~~markdown
# ADR-0043 — PALW V2 state-root hash ordering (no challenge↔commitment cycle)

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


- **Status:** Accepted (2026-08-20)
- **Relates to:** ADR-0042 Decision 5 (which required this record before any header commits a
  state root: "the ADR that lands it must spell out a hash ordering with no
  challenge↔commitment cycle — don't implement by vibes"), Decision 3 (the attempt transcript),
  Decision 9 (the comparator), Decision 11 (the ruleset fingerprint).
- **Lands with:** PR-03 (`consensus/core/src/palw_state_v2.rs`). The header FIELD that carries
  the root arrives with the V2 header format in PR-10; this ADR fixes what the bytes will mean
  so nothing about them is decided by the PR that wires them.

## 1. What a V2 header commits, and why there is no cycle

A V2 header at block `B` commits

```
B.palw_state_root  =  state_root( PalwChainStateV2 after applying B's SELECTED PARENT )
```

that is, the **parent-side** state: the result of applying the selected-parent chain up to and
including `parent(B)`, and nothing of `B` itself. `B`'s own attempt and accepted objects are
applied by [`apply_palw_transition_v2`] to *produce* the state that `B`'s children commit.

The cycle analysis, term by term:

```
challenge(B)   = H(network_domain ‖ pre_pow_hash(B) ‖ timestamp ‖ nonce ‖ class ‖ bond)   (ADR-0042 D3a)
root(B)        = H(challenge ‖ class ‖ bond ‖ trace_root ‖ output_root ‖ pwu)
attempt_id(B)  = H(canonical unsigned attempt)
state_root(B)  = f(state after parent(B))
```

- `challenge(B)` takes **no state root** as input, so the attempt does not depend on any value
  the attempt itself could move.
- `state_root(B)` is fully determined before `B`'s attempt exists (it is a function of the
  parent chain), so the header may carry both the attempt and the state root with neither
  hashing the other's output.
- The claim that `B`'s attempt creates enters the state committed by `B`'s **children** — the
  same parent-side pattern as the UTXO commitment, and the reason a verifier can check
  `B.palw_state_root` before ever validating `B`'s own PALW content.

Rejected alternative, for the record: committing the *post-B* state ("this block's own effects
included") would make the header's committed root depend on the attempt, whose challenge is
domain-separated over header fields adjacent to that commitment — auditing that no path closes
into a cycle then has to be redone every time either transcript grows a field. Parent-side
commitment makes the acyclicity structural rather than reviewed-per-change.

## 2. The root derivation, frozen

> **Amended 2026-08-22 (M-02), extended 2026-08-23 (ADR-0051).** The listing below is the
> **version-6** preimage,
> replacing the version-1 listing this section froze at authorship. Between the two, four changes
> extended the preimage and this ADR did not follow; the external audit named the drift (M-02)
> and it is settled here, before the ruleset id froze, so the golden vectors are fixed against
> the right text. One of the four (`retired_safe_weight`) had ALSO skipped the version bump §2's
> own change rule demands — version 5 supplies it. The amended listing is not prose any more:
> `the_state_root_preimage_is_exactly_the_adr_0043_list` (in `palw_state_v2.rs`) reimplements
> this section from its text and holds it equal to the production root, and
> `every_primary_datum_moves_the_root` refuses a primary field that is persisted but unhashed.

Implemented in `PalwChainStateV2::state_root` (domain
`misaka-palw/state-v2/state-root/v1`, keyed BLAKE2b-512), version constant
`PALW_STATE_V2_VERSION = 7`:

```
H( version_le(2)                                                            # = 7
 ‖ root("bonds")  ‖ root("reserved_exposure") ‖ root("classes") ‖ root("class_targets")
 ‖ root("class_shares")
 ‖ epoch_budgets_tag(1) [‖ borsh(epoch_budgets)]
 ‖ root("receipt_targets")
 ‖ root("capabilities") ‖ root("claims")
 ‖ root("pending_payouts")
 ‖ root("panels") ‖ root("court_sessions")
 ‖ root("epoch_counters") ‖ root("receipt_epoch_counters")
 ‖ safe_weight_le(16) ‖ retired_safe_weight_le(16) ‖ bounded_immature_le(16)
 ‖ safe_frontier_blue_score_le(8) ‖ safe_frontier(64)
 ‖ last_point_tag(1) [‖ borsh(last_point)] )
```

Both option fields use the same tag rule: `0x00` for absent, `0x01 ‖ borsh(value)` for present.
`epoch_budgets` and `last_point` are single records, not collections, so they are serialized
raw under the state-root domain rather than through a labeled collection root.

Provenance of every item past the version-1 listing — each is primary data for the stated
reason, and "derivable caches stay out" (§3) still holds:

| item | added by | why it is primary |
|---|---|---|
| `class_shares` | ADR-0045 Decision 3 (`79245ef0`) | the cadence share table is granted by registration and conserved by transition — not derivable from the class set |
| `epoch_budgets` | ADR-0045 Decision 2 (`79245ef0`, blocks-not-pwu per `d4890a78`) | derived once at the epoch boundary from the boundary's own facts, then constant — a later state cannot re-derive it |
| `receipt_targets` | ADR-0044 / FP-03 (`5b53e8b7`) | the receipt lane's per-class retarget slot, the mirror of `class_targets` |
| `receipt_epoch_counters` | ADR-0044 / FP-03 (`5b53e8b7`) | the receipt lane's census, the mirror of `epoch_counters` |
| `pending_payouts` | ADR-0042 Decision 10 escrow (`1a4bedb5`, version 4) | a released escrow waiting for the next coinbase — a miner must not be able to pay a queue nobody else has |
| `retired_safe_weight` | launch blockers §8, terminal-claim retirement (`bb62f1fc`) | the certified weight of retired claims; not derivable precisely because the claims it summarizes are gone |
| `classes[…].terms.runtime_pins` | ADR-0051 step 6 | the runtime, model and tokenizer a black-box class is held to. A node with no on-chain pins could only check its worker against itself |
| `classes[…].terms` | ADR-0051 (two execution families) | which family verifies a class — and therefore whether a dispute about it can end in a conviction — plus the panel it draws. Not derivable from the graph: a black-box class and an adjudicable one can have the same shape |

Chain-block identity throughout the state — `PalwBlockContextV2::block`, `safe_frontier`, a
claim's `accepted_block`, the state-book keys — is the codebase's `BlockHash` (**`Hash64`**,
flipped in PR-9.5e / ADR-0008). This is also why `PalwCandidateOrderV1::candidate` is a
`Hash64`: the comparator's tie-break IS the candidate's block hash, no widening layer between
them.

Each collection root (domain `misaka-palw/state-v2/collection/v1`) is

```
H( len(label) ‖ label ‖ count_le(8) ‖ ( len(key) ‖ borsh(key) ‖ len(record) ‖ borsh(record) )* )
```

over the collection's key order — `BTreeMap` order, which for every key type in the state is a
comparison of fixed-width byte arrays and integers, identical on every ISA. Length prefixes keep
adjacent entries from bleeding into one another; the label keeps two same-shaped collections
from colliding.

**Change rule:** adding, removing, or reordering a field or collection — or changing any
record's Borsh shape — is a consensus change and takes a new version constant, an amendment to
the §2 listing above, and new golden vectors (`the_version_7_state_root_golden_vectors`); the
domain strings stay at `/v1` because the version inside the preimage is the separator. There is
no "compatible" evolution of a hash preimage. The rule was violated once — `retired_safe_weight`
entered without a bump — which is why the correspondence is now a test rather than a sentence.

**And the test has already caught one.** ADR-0051 added `terms` to `PalwClassStateV2`, which moves
the class collection's record encoding and therefore every root;
the golden vectors failed on the first run after that change, before the preimage move could ship
as a silent fork — and again when step 6 added `runtime_pins` to those terms. Versions 5 → 6 → 7
are those two catches. Two bumps in one day is the change rule working, not thrashing: each is a
real preimage move that would otherwise have been silent.

## 3. What the root deliberately does not cover

- **Parameters** (β, windows, epoch length): per-network constants inside the atomic bundle,
  committed by the ruleset fingerprint (ADR-0042 Decision 11). Re-hashing them per block would
  say nothing the fingerprint has not already said.
- **Indices** (`deadlines`, `unresolved`, `open_courts_by_claim`): derivable caches. They are
  rebuilt on every load and cross-checked against primary data; hashing them would freeze an
  implementation detail into consensus.

## 4. The carriage, and the None-root rule

`PalwStateCarriageV2` (domain `misaka-palw/state-v2/carriage/v1`) is the Borsh snapshot of the
primary data that the pruning proof carries (ADR-0042 Decision 5), digested as
`H(len ‖ borsh(carriage))`. Loading rebuilds all indices and refuses a snapshot whose derivable
facts disagree with its primary data.

Self-consistency is necessary, not sufficient: a claim's `reserved` / `immature_contribution`
snapshots are the accounting basis by design, so a tamper that adjusts `pwu` and nothing
derivable from it is *coherent*. What catches it is the committed root. Therefore:
**`into_state(…, expected_root = None)` is legal only for a node's own trusted disk; a
peer-supplied carriage MUST be loaded against the root the chain committed.** PR-08's IBD wiring
inherits this rule as written.

## 5. The frontier, since the comparator orders by it first

`safe_frontier` advances to `(blue_score, block)` of the chain point being applied exactly when,
at the end of that application, **no unresolved claim exists** — and holds still otherwise. It
is monotone by construction and observed lazily at block boundaries. This is the first key of
`compare_palw_candidates_v1` (ADR-0042 Decision 9): a private fork can pile up attempts, but its
claims cannot mature, so from the fork point its frontier never moves again.

## 6. Number hygiene

This is ADR-0043; 0042 is the last committed. Ties with a concurrent 0043 resolve per ADR-0036
Decision 5 (this file's content stays, the later writer renumbers).

## Amendment 2026-09-02 — version 17 (ADR-0078): the derivation table

ADR-0078 Decision 4 adds one collection, `derived_artifacts` — the derivations of free-prompt
claims, keyed `(claim, transformer)` (`PalwDerivedKeyV1`), valued by names and hashes only
(`PalwDerivedRowV1`: derived id, grammar id, kind, DSL hash, artifact hash, artifact size,
accepted DAA). It is primary data: a derivation is a statement the executor signed and the chain
accepted, not derivable from any other table. `PALW_STATE_V2_VERSION` moves 16 → 17. The listing
this amendment restates is the WHOLE current preimage, because the text above stopped at version 7
while the code went on to 16 (registration exposure and class walks — ADR-0056; the certification
tables — ADR-0075), and a listing that is not the whole list is not a listing:

```
H( version_le(2)                                                            # = 17
 ‖ root("bonds")  ‖ root("reserved_exposure") ‖ root("classes") ‖ root("class_targets")
 ‖ root("class_shares")
 ‖ root("registration_exposure") ‖ root("class_walks")                      # ADR-0056 D3, D5
 ‖ epoch_budgets_tag(1) [‖ borsh(epoch_budgets)]
 ‖ root("receipt_targets")
 ‖ root("capabilities") ‖ root("claims")
 ‖ root("pending_payouts")
 ‖ root("certified_families") ‖ root("fp_certified_families")               # ADR-0075 D3
 ‖ root("fp_certified_classes")                                             # ADR-0075 D5
 ‖ root("pending_chunks")                                                   # ADR-0075 D14
 ‖ root("derived_artifacts")                                                # ADR-0078 D4
 ‖ root("panels") ‖ root("court_sessions")
 ‖ root("epoch_counters") ‖ root("receipt_epoch_counters")
 ‖ safe_weight_le(16) ‖ retired_safe_weight_le(16) ‖ bounded_immature_le(16)
 ‖ safe_frontier_blue_score_le(8) ‖ safe_frontier(64)
 ‖ last_point_tag(1) [‖ borsh(last_point)] )
```

| item | added by | why it is primary |
|---|---|---|
| `registration_exposure`, `class_walks` | ADR-0056 Decisions 3 and 5 | the registry's own exposure ledger and the per-class streak counters — accumulators the transition maintains and the consistency check re-derives |
| `certified_families`, `fp_certified_families`, `fp_certified_classes` | ADR-0075 Decisions 3 and 5 | what the court graded on this chain, and which classes it seated — chain history of one identity |
| `pending_chunks` | ADR-0075 Decision 14 | half-assembled objects, kept in state so every node completes a group in the same block |
| `derived_artifacts` | ADR-0078 Decision 4 | a signed, accepted statement of what was made from a claim's answer; retired with the claim |

`the_state_root_preimage_is_exactly_the_adr_0043_list` restates this listing and holds it equal to
`PalwChainStateV2::state_root`; `the_version_17_state_root_golden_vectors` pins both roots.
~~~~~~~~
<!-- ADR_SOURCE_END:0043-palw-v2-state-root-ordering.md -->

</details>

