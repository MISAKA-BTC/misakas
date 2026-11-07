# ADR-v2-0007 — EVM／EXEC実行lane・anchor・gas budget

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 6本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0007](../rfc-v2/0007-claims-exec-and-answer-accounting.md) / [RFC-v2-0010](../rfc-v2/0010-dag-consensus-and-native-settlement.md)

## 1. この文書の役割

EVM実行、EXEC lane、TxPermit／WorkSliceとchain anchorの役割を統合する。

## 2. 統合した判断の読み方

### 2.1

Selected-parent EVM laneの基礎と、foldを正本にしてEVMを読取り・操作の窓にする判断を対応づける。[旧ADR-0020](../adr/0020-selected-parent-evm-lane.md) [旧ADR-0089](../adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)

### 2.2

EXECはREAL等からのanchorにより到達する実行領域であり、親選択・clock・fork-choice weightと同じものとして扱わない。gasの増加はlaneのbudgetとpermitに結び付ける。[旧ADR-0125](../adr/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md) [旧ADR-0139](../adr/0139-the-execution-lanes-gas-is-one-budget-a-round.md) [旧ADR-0168](../adr/0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md)

### 2.3

旧algo-11 work-slice案を復活させず、EXEC_TXとEXEC_SLICEで既存EXEC classを共有する後続方針を読む。TxPermitとslice会計は別、同じ仕事をroot／prefix／sliceで重複計上しない。[旧ADR-0169](../adr/0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md)

## 3. 維持する境界・後続改定

旧algo-11の試験結果は置換後EXECの実装完了証拠ではない。旧ADR-0169の安定したファイル名は互換参照として保持する。

## 4. 隣接する判断との分担

仕事量と対価は[ADR-v2-0026](0026-canonical-work-and-answer-accounting.md)、claim依存は[ADR-v2-0048](0048-claim-dependencies-and-prefix-origin.md)、合意への寄与は[ADR-v2-0006](0006-fork-choice-participation-and-settlement.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0020-selected-parent-evm-lane.md](../adr/0020-selected-parent-evm-lane.md) | L1–L171 | L14–L24 |
| [0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md](../adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) | L1–L574 | L24–L39 |
| [0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md](../adr/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md) | L1–L432 | L17–L33 |
| [0139-the-execution-lanes-gas-is-one-budget-a-round.md](../adr/0139-the-execution-lanes-gas-is-one-budget-a-round.md) | L1–L113 | L12–L16 |
| [0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md](../adr/0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md) | L1–L311 | 定型のStatus宣言なし（本文を参照） |
| [0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md](../adr/0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md) | L1–L34 | L12–L12 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0020-selected-parent-evm-lane"></a>

<details>
<summary>0020-selected-parent-evm-lane.md — 原文 L1–L171</summary>

[原文を開く](../adr/0020-selected-parent-evm-lane.md) · SHA-256: `b6586849aeafeacd070f747249cac3580f7f24fa9bd1712b100610084b248732`

<!-- ADR_SOURCE_BEGIN:0020-selected-parent-evm-lane.md -->
~~~~~~~~markdown
# ADR-0020: Selected-Parent EVM Execution Lane on L1

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

## Status
Accepted & implemented through design **v0.4 (mergeset delayed acceptance)**; **ACTIVATED ON
TESTNET 2026-06-11**. All build phases are on `pr-19-s5f-…`: P0–P3 (types/executor/state), M10
(acceptance executor + hot path), P4 (UTXO↔EVM bridge: deposit-lock → claim → credit → F002
withdraw → synthetic UTXO), §15 (template + wire), §16 (EVM mempool + RPC + indexes), §14 (EVM-tx
P2P relay, protocol 100→101 back-compat), plus the N0 optimization pass (O1 sender cache, O2
parallel admission, O3 Evm reuse, O9 lane KPIs). Proven live first on a 3-node devnet-EVM mesh,
then on testnet: `TESTNET_PARAMS.evm_activation_daa_score = 0` (genesis-active) via a 3-host
re-genesis cutover — the testnet genesis hash is unchanged (`cf4c48fe…`), and post-activation
headers carry `EVM_HEADER_VERSION` (v2), so pre-EVM nodes and the old chain are version-isolated
in both directions. Devnet is likewise genesis-active; **mainnet/simnet stay inert (`u64::MAX`)**.

> **Tree reconciliation (2026-09-05, recorded by [ADR-0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) §2).** "The node binary remains
> secp-free by default; EVM is an opt-in (`--features evm`) build" no longer describes the tree:
> `kaspad/Cargo.toml:93` is `default = ["evm"]` since 2026-08-21 and a non-`evm` build refuses an
> EVM-active network (`EvmLaneRequiresEvmBuild`). F001 WMISAKA is declared and never deployed; F002
> is a call-frame intercept (`misaka/evm/core/src/withdraw.rs`), not a stateless precompile. Everything
> else below stands. Map: [`README.md`](README.md).

> **Design superseded by v0.4** — the unified design doc
> [`docs/misaka-evm-design-v0.4.md`](../design/evm/misaka-evm-design-v0.4.md) replaces the v0.3 immediate-execution
> model with **mergeset delayed acceptance** (B's own payload is executed by its selected child), adds
> `evm_payload_hash` as a second header commitment, 5-class skip semantics, payload-miner fee routing,
> two-stage caps, and a non-decreasing timestamp clamp. The v0.4 migration deltas are listed in
> design §21; the sections below are the v0.2/v0.3 freeze, kept as the historical record of the
> consensus-safety analysis.

Source design: `MISAKA_Kaspa_L1_Selected_Parent_EVM_Design_v0.2_Audit_Revised.docx` +
`..._v0.3_DEX_Uniswap_Addendum.docx`. This ADR is the code-grounded freeze of that design against the
current kaspa-pq tree. The v0.2 audit elevated 6 consensus-safety rules and **collapsed the header to a
single `evm_commitment_root`** (the four-root v0.1 layout is gone — see "Version gating").

Adds header fields and a block-body payload → a hard fork, but **version-gated** so every existing
v0/v1 genesis hash and block identity is byte-for-byte unchanged (see §"Version gating"). Interacts
with the PQ-only invariant ([ADR-0019](0019-mldsa87-migration.md)): the EVM lane is a **separate
signature domain** that reintroduces secp256k1/ECDSA, isolated behind the `evm` cargo feature so the
default node build stays secp-free.

---

## Context

We want to run the Ethereum EVM as part of L1 consensus — no external bridge, no L2 sequencer —
while keeping the Kaspa/MISAKA DAG consensus and UTXO ledger intact. The core tension is that the
EVM is a **global mutable state machine** (inherently sequential), whereas a DAG accepts many blocks
in parallel and reorganizes its virtual selected chain frequently. Running every blue/accepted
block's EVM txs would force a re-ordering/replay problem on every virtual change.

## Decision — Selected-Parent EVM Lane

The EVM parent of a DAG block `B` is its GHOSTDAG **`selected_parent(B)`** — not its full direct-parent
set, and not the current virtual selected parent:

```
EVM_PARENT(B) = selected_parent(B)
EVM_STATE(B)  = EXEC(EVM_STATE(EVM_PARENT(B)), system_deposits(B), evm_txs(B), env(B))
```

Consequences of this single rule:
- `B`'s EVM result is an **append-only function of `B` alone** — computed once at block validation,
  stored by `block_hash`, and **never re-executed** on a virtual reorg.
- A virtual change only moves a **canonical EVM head pointer** (`latest_unsafe` / `safe` / `finalized`);
  no `execute_evm` / `revert_evm` on the hot path.
- EVM txs are canonical only when their block enters the selected-parent chain; UTXO txs keep their
  existing DAG-inclusive acceptance. This asymmetry is intentional (design §3.3).
- UTXO ↔ EVM value moves via in-consensus **system deposit / withdraw** side-effects, conserving the
  combined native-coin supply (design §6/§7).
- RPC separates `latest` / `safe` / `finalized` heads; USDC/CEX-grade use targets `finalized`.

Trade-off (accepted): EVM throughput tracks the single selected-parent chain, **not** DAG parallelism.

---

## Frozen parameters (P0)

| Item | Value | Notes |
|---|---|---|
| `EVM_HEADER_VERSION` | `2` | `constants.rs`. Must exceed genesis v0 and `BLOCK_VERSION`=1. Never lower. |
| `EVM_CHAIN_ID` | `0x4D534B` ("MSK") | `evm/mod.rs`. Distinct from all public Ethereum nets; mainnet id chosen at launch. |
| EVM fork | revm `SpecId::SHANGHAI` (pinned P2) | London+ baseline runs Uniswap v2/v3 + current-solc contracts (design §19.2); Cancun/EIP-1153 (v4) is a deliberate later fork. Never auto-follows upstream; bump = hard fork. |
| `EVM_NATIVE_SCALE` | `10^10` | sompi (8 dec) → wei (18 dec). Withdrawals must be exact multiples. |
| `EVM_GENESIS_STATE_ROOT` | `keccak256(rlp(()))` empty-trie root (`56e81f17…b421`) | The P2 executor asserts an empty block reproduces it. |
| Header preimage suffix (v2+ only) | `evm_payload_hash(64)` then `evm_commitment_root(64)` | **TWO** keyed BLAKE2b-512 roots, appended in that order after `pruning_point` (design v0.4 §4.1/§4.3 — superseded the v0.2 single-root layout). Frozen byte order. |
| EVM commitment domains | `b"EvmPayload64"` · `b"EvmCommitment64"` | `EvmPayload64` keys the block's raw `EvmExecutionPayload` (→ `evm_payload_hash`); `EvmCommitment64` keys the body-side `EvmExecutionHeader` (state/tx/receipts/system-ops/withdrawals/deposit-claim roots, gas, basefee, logs bloom, evm_number, evm_timestamp_sec, burn accumulator → `evm_commitment_root`). The earlier `MISAKA_EVM_COMMITMENT_V2` domain is retired. |
| Subnetwork ids | `0x20` deposit, `0x21` withdraw-claim (reserved), `0x22` admin (reserved) | `subnets.rs`. |
| DB store prefixes | `201`–`210` | `database/registry.rs` (`EvmHeader`…`EvmBlockHashMap`). |
| Withdraw precompile | `0x…F002` (`MISAKA_WITHDRAW`) | `evm/mod.rs`. |
| Activation | `Params::evm_activation_daa_score` | **testnet = 0 (genesis-active since 2026-06-11)**, devnet = 0; mainnet/simnet = `u64::MAX` (inert). Activation gates header v2 (`EVM_HEADER_VERSION`) — see Status. |

**Circular-dependency rule (design §4.2):** the current L1 block hash and current EVM block hash are
**not** inputs to the EVM execution environment (the header hash already commits to the EVM result).
`blockhash`/`prevrandao` derive from `selected_parent` ancestry only.

---

## Version gating (the load-bearing correctness property)

The single `evm_commitment_root` field is **always present** in the `Header` struct (defaulting to
zero) but enters the header-hash preimage **only when `header.version >= EVM_HEADER_VERSION`**
(`hashing::header::write_header_preimage`). Because genesis headers are v0 and live mined blocks are
v1 (both `< 2`), their preimage — and all three digests (legacy-32, identity-64, pre-PoW-64) — is
byte-identical to the pre-EVM protocol. `consensus-core::config::genesis::test_genesis_hashes` stays
green **with no constant changes**. (Mirrors the `merkle::*_pre_crescendo` version-gating precedent.)

On-disk: the consensus header is **bincode**-serialized (not borsh) via `database::access`. Adding
fields changes that layout, so `LATEST_DB_VERSION` is bumped `6 → 7` and old-shape DBs are rejected at
open time (clean resync, per [ADR-0001](0001-network-isolation.md)) rather than migrated.

---

## PQ-only reconciliation

revm pulls in secp256k1 (ecrecover + secp precompiles), which conflicts with the secp-free node
guarantee enforced by `scripts/ci/pq-ci-guard.sh`. Resolution: the EVM **types** are always compiled and
are secp-free; the **executor** (revm) lands behind the `evm` cargo feature (default OFF). The default
`kaspad` build stays secp-free; an `--features evm` build opts into the EVM lane and secp. The EVM lane
is a separate signature domain from native UTXO ML-DSA-87 (design §1.2/§16). A PQ-EVM (no secp) is
explicitly out of scope.

---

## Implementation roadmap (design §17)

| Phase | Scope | Status |
|---|---|---|
| **P0** | Spec freeze (this ADR) | **Done** |
| **P1** | Consensus types: `EvmH256`/`EvmExecutionHeader`/`EvmExecutionPayload` + deposit/withdraw ops; single `evm_commitment_root` + version-gated preimage; block `evm_payload`; subnets; store prefixes; `evm_activation_daa_score`; body rule; `evm` feature declared | **Done** |
| P2 | revm `SpecId::SHANGHAI` executor behind `evm` (parent state root → keccak state/tx/receipts roots); deterministic env (number/ts-clamp/prevrandao/EIP-1559 basefee); deposit-claim credit; F002 withdraw precompile; commitment matches `EvmExecutionHeader.commitment_root()`; differential tests | **Done** (live on testnet-10) |
| P3 | EVM stores (201–210), multi-root state backend, canonical heads (no-replay on virtual change), pruning/GC | **Done** (store-side; standalone GC/pruning of EVM rows still pending) |
| P4 | Deposit (subnet 0x20) extraction from acceptance data; withdraw precompile; UTXO-diff materialization; combined supply-invariant tests | **Done** — deposit-lock → `submitEvmDepositClaim` → refund + F002 withdraw all live (incl. the claim-retry + same-generation TOCTOU fix, commit `51ece4d`) |
| P5 | EVM txpool, template builder (EVM roots + withdrawals in utxo_commitment), EIP-1559 basefee | **Done** — own-payload EVM mempool + claim queue + template fold |
| P6 | `eth_*` JSON-RPC, logs, subscriptions, `safe`/`finalized` tags; wire EVM data through gRPC/p2p/RPC | **In progress** — feature-gated Ethereum JSON-RPC adapter (`rpc/eth`, `--features evm`): identity/chain/state/`eth_call`/`estimateGas` done; `eth_sendRawTransaction`/receipts/blocks + logs/subscriptions WIP |
| P7 | Security/audit: DoS, state bloat, supply, reorg, RPC consistency | **Done** — external EVM-bridge audit (F1–F6) remediated (commit `74d5442`); state-bloat GC remains the open item |

### P1 surface (implemented)
- `crypto/hashes`: `EvmH256` (32-byte Ethereum H256, mirrors `Hash`).
- `consensus-core`: `constants::EVM_HEADER_VERSION`; `evm` module (`EvmExecutionPayload`,
  `EvmExecutionHeader`, deposit/withdraw op types, `EvmAddress`, `EvmBloom`, frozen constants); single
  `Header::evm_commitment_root` + `with_evm_commitment`; version-gated `write_header_preimage`;
  `Block`/`MutableBlock.evm_payload` +
  `with_evm_payload`; subnet ids + `is_evm_overlay`; `Params::evm_activation_daa_score` +
  `is_evm_active`; `RuleError::NonEmptyEvmPayloadBeforeActivation`.
- `consensus`: body-isolation rule `check_evm_payload` (pre-EVM header ⇒ empty payload);
  `LATEST_DB_VERSION 6→7`.
- `database`: store prefixes 201–210.
- Cargo `evm` feature declared (empty) on `kaspad`/`kaspa-consensus`/`kaspa-consensus-core`.

**Deferred to P2 (intentional):** non-zero EVM data is **not** carried over gRPC/p2p/RPC in P1 — the
convert layer round-trips the zero/empty P1 values via defaults; the wire extension lands with the
executor that first produces non-zero values, so wire + execution are tested together.

---

## Consequences
- A new hard fork; existing nets' genesis hashes are provably unchanged (version gate).
- Live non-EVM nets resync once (DB version bump).
- The node binary remains secp-free by default; EVM is an opt-in (`--features evm`) build.
- EVM TPS is bounded by the selected-parent lane, not DAG width.
~~~~~~~~
<!-- ADR_SOURCE_END:0020-selected-parent-evm-lane.md -->

</details>

<a id="source-0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand"></a>

<details>
<summary>0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md — 原文 L1–L574</summary>

[原文を開く](../adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) · SHA-256: `9dd6dbf5cce911b31b093b6122cb85e4c8074c43ce376462dbd852d39f972a00`

<!-- ADR_SOURCE_BEGIN:0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md -->
~~~~~~~~markdown
# ADR-0089 — the fold is the truth; the EVM is its window and its hand

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


## 2026-10-09 改定 — Positionと市場の対象を固定する

[ADR-0175](0175-registered-models-are-permanently-immutable.md)を適用する。市場の`line_id`は固定されたmodel registration IDであり、Position、reserve、seed、reward buybackとEVMの参照先を改善版へ付け替えない。改善版は自身の登録・検証責任・報酬資格・独立AMMを持つ。既存市場の残高、価格、virtual reserveや実reserveを新登録へ継承・自動移動させない。新規登録が他モデルの市場を変更してはならない。以下に残るversion移行への期待は旧設計の記録であり、新規則では権利を与えない。

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Amended a third time (2026-10-01, implemented the same day behind the dormant fence `palw_model_virtual_v1`).** [0162](0162-the-pair-opens-on-a-virtual-reserve.md): past the fence the AMM window's `market()` answers a twelfth word (the row's own virtual reserve), `constants()`' third word is the virtual reserve again, `quoteSell`'s first word is the gross MSK out, and the writer reverts a seed after the market's first trade (`SeedAfterTrade()`) and a buy of a line whose class does not trade (`ClassNotEligible()`); a seed before the class's approval is taken.
>
> **Amended again (2026-09-06, design first).** [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md): the AMM window's `market()` (Decision 2) gains two words at its end — `buybackSompi`, `retiredUnits` — the BILI the mining reward has put into the pair and the positions the chain holds for good; every earlier word keeps its offset. The move itself is the fold's, on no lane: no action, no event.
>
> **Amended (2026-09-05, implemented the same day).** [ADR-0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) adds a third action to the writer (`3`, the seed — `msg.value` is the seed, at least 100,000 MSK), `seed()` and the `Seeded` event to the facade, `SeedTooSmall()`, the settlement's `action` in place of `is_buy`, and puts the least seed in `constants()` where the virtual reserve was; `decimals()` is 0. Decisions 3, 5 and 6 read with that in mind.

**Status:** PROPOSED 2026-09-05, design only (no implementation yet). Requested by the operator
on 2026-09-05, on top of ADR-0087 and ADR-0088: Model Positions are not to be "a mere copy of
ERC-20" (ERC-20 の単なるコピーにせず); `ModelPosition`, `ModelRegistry` and `ModelAMM` are to be
**native primitives of the MISAKA EVM** — the thing that is MISAKA's own reason to exist
(Misaka 独自の存在意義); the current ERC is to be reshaped into an EVM design specialised for
trading model positions; and Hyperliquid's HyperEVM is the reference.
**Builds on:** ADR-0020 (the selected-parent EVM lane — live on testnet-10, testnet-11 and
devnet from genesis, inert on mainnet), ADR-0022 (the EVM snapshot at the pruning point),
ADR-0023 (Lane 2; per-lane precompile profile; the frozen security labels), ADR-0087 (the market
is a balance in the state fold; implemented behind `palw_model_market`), ADR-0088 (lines, versions,
roles, usage; the market keyed by line), ADR-0059 (carve, never mint), `docs/misaka-evm-design-v0.4.md` §3.2 (system
ops), §8 (fees), §9 (the UTXO ⇄ EVM bridge), §17 (invariants).
**Amends:** ADR-0087 Decision 3 (two more ways to make the same two moves — from the EVM),
Decision 8 (what a participant reads gains the EVM's surfaces); design v0.4 §9.1's supply
invariant (it names the market's sinks and the writer's escrow). ADR-0087 Decision 5 (no
transfer) is not amended: this ADR is the proof that the EVM cannot manufacture one.

## 0. The sentence this ADR is

The PALW state fold holds every fact about a model — its class, its line and its owner, the
versions in force and how each was used, its curve, every position — and that is where they stay. The EVM gets **three windows**
(precompiles that return the fold's rows as facts) and **one hand** (a system contract whose
calls become fold actions, applied after the block), and every class gets an address at which
the ERC-20 *read* surface works and the ERC-20 *transfer* surface does not exist. That is what
HyperEVM did with an order book. MISAKA does it with a model registry — lines, their versions,
their owners, their usage — and a curve: things no other EVM can read as consensus facts, because
no other chain holds them as consensus.

## 1. The reference: what HyperEVM built (read 2026-09-05, Hyperliquid's own docs)

| HyperEVM (Hyperliquid) | what it settled |
|---|---|
| "The HyperEVM consists of EVM blocks built as part of Hyperliquid's execution, inheriting all security from HyperBFT consensus." Cancun without blobs; chain id 999 / 998; HYPE is gas; base **and** priority fees burned. | The EVM is not a second chain and has no bridge trust: its blocks are part of L1 execution. |
| **Read precompiles** from `0x0000000000000000000000000000000000000800` upward (`L1Read.sol`: position 0x800, spotBalance 0x801, vaultEquity 0x802, withdrawable 0x803, delegations 0x804, delegatorSummary 0x805, markPx 0x806, oraclePx 0x807, spotPx 0x808, l1BlockNumber 0x809, perpAssetInfo 0x80a, spotInfo 0x80b, tokenInfo 0x80c, tokenSupply 0x80d, bbo 0x80e, accountMarginSummary 0x80f, coreUserExists 0x810). "The values are guaranteed to match the latest HyperCore state at the time the EVM block is constructed." Gas `2000 + 65 × (input_len + output_len)`; invalid inputs "return an error and consume all gas passed into the precompile call frame". | Core state is read **synchronously**, as a fact of the block, with no oracle in between. |
| **CoreWriter** at `0x3333333333333333333333333333333333333333`, `sendRawAction(bytes)`: byte 1 = encoding version, bytes 2–4 = action id, the rest = ABI data; action ids 1 limit order … 17 outcome operation; "burns ~25,000 gas before emitting a log", a basic call "~47000"; "order actions and vault transfers sent from CoreWriter are delayed onchain for a few seconds" — "to prevent any potential latency advantages for using HyperEVM to bypass the L1 mempool". | Core state is written **asynchronously**, by an action the core executes after the block, with no return value in the EVM and a deliberate delay. |
| **System addresses**: each token's is `0x20` followed by zeros and the token index big-endian (`0x20000000000000000000000000000000000000c8` for index 200); HYPE's is `0x2222222222222222222222222222222222222222`. EVM → Core is an ERC-20 `transfer` to the system address; Core → EVM is a system transaction calling `transfer(recipient, amount)` on the *linked* contract. "A token's system address must have the total non-system balance on the other side." "There are currently no checks that the system address has sufficient supply or that the contract is a valid ERC20, so be careful when sending funds." | An asset crosses by moving to a known address; linking is explicit; the other side's supply is the system address's balance — and the docs say plainly what is not checked. |
| **Dual blocks**: fast blocks every 1 s at 3M gas, slow blocks every 1 min at 30M gas, interleaved in one increasing sequence of EVM block numbers; a user opts in with `{"type": "evmUserModify", "usingBigBlocks": true}`; the mempool "accepts only the next 8 nonces for each address" and prunes after a day. | Throughput and latency are allocated separately. |
| The same 20-byte address is the account on both sides. | Identity is shared, so bridging is a transfer to a system address and nothing more. |

Sources: `hyperliquid.gitbook.io/hyperliquid-docs/for-developers/hyperevm` (overview),
`…/hyperevm/interacting-with-hypercore` (precompiles, CoreWriter, action table),
`…/hyperevm/hypercore-less-than-greater-than-hyperevm-transfers` (system addresses, linking),
`…/hyperevm/dual-block-architecture`. Read on 2026-09-05; numbers move with their releases and
are cited here for the shape, not the values.

## 2. What exists here

* **The MISAKA EVM is ADR-0020's lane, and it is live on the PALW network.** revm pinned to
  `SpecId::SHANGHAI`; chain id `0x4D534B`; `EVM_NATIVE_SCALE = 10^10` (sompi → wei);
  `EVM_GAS_LIMIT = 30,000,000` per chain block, `MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK = 128 KiB`;
  one EVM block per selected-chain block (`evm_number(B) = evm_number(selected_parent(B)) + 1`);
  header fields `evm_payload_hash` and `evm_commitment_root`, version-gated (`EVM_HEADER_VERSION
  = 2`). `Params::evm_activation_daa_score` is `0` on `TESTNET_PARAMS` and `DEVNET_PARAMS`,
  `u64::MAX` on mainnet and simnet — and `TESTNET11_PARAMS` is `..TESTNET_PARAMS`
  (`consensus/core/src/config/params.rs:4951`), so testnet-11, the PALW RC, runs the EVM lane from genesis, and the fleet
  builds it (`contrib/local-desktop-join/scripts/misaka-desktop-node.sh:405`: `cargo build --release -p kaspad --features evm`) —
  and since 2026-08-21 so does every build: `kaspad/Cargo.toml:93` is `default = ["evm"]`
  ("the EVM lane is part of the node, not a bolt-on"), a non-`evm` build refuses an EVM-active
  network at boot (`ConfigError::EvmLaneRequiresEvmBuild`, `kaspad/src/daemon.rs:133`), and ADR-0020's
  "secp-free by default" and ADR-0087 §1's "an optional, non-default feature" are both stale
  against the tree. The lane is still a separate secp256k1 signature domain (ADR-0023's label
  for it: `CLASSICAL-ECC` / `ETH-COMPATIBLE`); an EVM tx is not a Kaspa transaction but a raw
  EIP-2718 envelope in `Block.evm_payload.transactions`, recovered to a 20-byte address by
  `TxEnvelope::recover_signer` (`misaka/evm/core/src/tx.rs:196-213`), reaching a block through
  `eth_sendRawTransaction` → `evm_mempool` → the payload.
* **The EVM already has system addresses and a bridge, and one registration seam.**
  `register_all_misaka_precompiles(handler, f003_active)` (`misaka/evm/core/src/precompiles.rs:23`)
  is the single place consensus and `eth_call` install MISAKA's handlers, so the two cannot
  diverge. `0x…F001` `WMISAKA_ADDRESS` is declared and deployed nowhere (the genesis state root
  is the empty trie); `0x…F003` ML-DSA-87 verify exists (`F003_VERIFY_GAS = 500,000`) and is
  inert on every network; `0x…F002` `MISAKA_WITHDRAW` is live and is **not a precompile but a
  call-frame intercept** — `misaka/evm/core/src/withdraw.rs:1-8`: "the stateless precompile ABI
  receives only `(input, gas_limit, ctx)` and cannot see `msg.sender` / `msg.value`". It costs
  `F002_WITHDRAW_GAS = 9,000`, journals a `MisakaWithdraw(address,uint256,bytes)` log, and the
  executor scans the block's committed logs into `WithdrawOp`s that `apply_evm_bridge_effects`
  (`consensus/src/processes/evm/mod.rs:211-258`) turns into a synthetic UTXO output under the
  domain `MISAKA_EVM_SYNTHETIC_OUTPOINT_V2`, *in the accepting block's own UTXO diff*; user
  faults revert the tx and the block stands. That log-scan → op → diff path is the only channel
  today by which the EVM writes anything outside itself. Deposits are two-step: an
  `EVM_DEPOSIT_LOCK` output (a push-and-drop no-op over an ML-DSA P2KH refund script,
  `crypto/txscript/src/script_class.rs:295-311`) on subnet `0x20`, then a producer-selected `DepositClaim {
  deposit_outpoint, evm_address, amount_sompi, claim_tip_sompi }` system op in a block's
  payload — the only `EvmSystemOp` kind there is — verified against `selected_parent(B)`'s
  UTXO view, crediting `(amount − claim_tip) × EVM_NATIVE_SCALE`. System ops run before user
  txs (v0.4 §3.2), at most 256 claims / 64 KiB / 10M system gas per EVM block (§9.2). Supply:
  `UTXO_balances + EVM_deposit_locks + EVM_native_balances + burned == issued` (§9.1), checked
  in O(1) by the `evm_total_native_balance` accumulator.
* **The order inside one chain block is fixed and is the order this ADR needs.** For a
  selected-chain candidate `B`, `calculate_utxo_state_relatively` runs `calculate_utxo_state`,
  then `evm_chain_context_step` (claims, mergeset acceptance execution, `evm_commitment_root`,
  and the bridge's UTXO side-effects folded into the block's context *before*
  `verify_expected_utxo_state`), then the UTXO verification, then the PALW transition
  (`apply_palw_transition_v6`) — `consensus/src/pipeline/virtual_processor/processor.rs:1435` before `:1625`. The EVM's result for `B`
  is a function of `B`'s parents and `B`'s system ops, never of `B`'s own fold (v0.4 I2, I9);
  the fold for `B` may therefore take the EVM's output as an input without a cycle.
* **The market is in the fold, implemented.** ADR-0087: `PalwModelMarketV1`, position rows
  keyed by `(class_id, holder: Hash64)`, `holder` = the payout payload of an ML-DSA-87 key
  (the identity a bond pays); objects `ModelBuy` / `ModelSell` on carrier transactions; a buy
  pays the class's sink (`OP_RETURN ‖ "MSKMDL01" ‖ class_id`, unspendable); a sell writes a
  `PalwPayoutV2` the coinbase honours; the rows enter the state root only when non-empty; the
  market opens at its first buy; `palw_model_market: Option<ForkActivation>`, `None` everywhere.
  ADR-0088 keys the market by *line* (a class, an owner, a name; the founding line's key is the
  class id) and adds, per line, the owner/developer/maintainer roles, the versions with their
  declared hashes and chain-counted usage, proposals and declared evaluations — all fold rows,
  all bond-signed on the UTXO side.
* **Two key domains, no shared address.** A fold identity is a 64-byte hash of an ML-DSA-87
  key; an EVM account is a 20-byte secp256k1 address. Hyperliquid's "same address on both
  sides" does not exist here and cannot be made to: ADR-0023 FD-SUPPLY calls a move from the
  PQ domain into Lane 2 "a security downgrade" that wallets must display.
* **Prior art that is not revived, and a coupling that does not exist.** The Token Program
  of 2026-08-10/11 (`consensus/core/src/token.rs`: an SPL-style `TokenAccount { balance, nonce }`
  ledger inside consensus, ML-DSA-87 transfer and burn, subnets `0x30`/`0x31`) is in the tree
  behind inert fences (`TokenParams::INERT` on every network) and never touched the EVM — its
  own design rejected "an EVM predeploy" (案 C) in favour of the consensus ledger. This ADR adds
  no ledger: the ledger is ADR-0087's, and the EVM gets views of it. And today there is no
  coupling at all: no `palw` symbol under `kaspa-evm/`, `consensus/core/src/evm/`,
  `consensus/src/processes/evm/` or `rpc/eth/`, and no `Evm` symbol in any of the 74
  `palw_*.rs` files; the one place the two meet is `misaka-palw-derive`'s runner, which executes
  model-written initcode "on an ephemeral, isolated state … never against the chain's EVM
  state and never inside the node process". The fold's rows do travel with a pruned node: the
  pruning-point snapshot flow serves a `PruningPointPalwStateMessage` beside the EVM's
  (`protocol/flows/src/v8/request_pruning_point_snapshots.rs:13`), so a window opened after pruned IBD has something
  to look at.

## 3. The requirement, and what "native" has to mean

The operator's three words name three things the fold already holds: the **registry** (class
rows; lines, versions, roles, usage — ADR-0056/0067/0088), the **AMM** (the curve, the reserve,
the fees — ADR-0087 Decisions 2–4), the **position** (a balance keyed by holder — ADR-0087
Decision 1).
"Native primitives of the EVM" can mean two things, and only one of them is right:

* **Move the truth into the EVM** — system contracts holding the reserve, the positions and
  the class table in keccak-MPT state, with Solidity-shaped rules. Rejected: the lane is
  `u64::MAX` on mainnet, so the market would not exist where the EVM does not ("optional
  feature" is no longer the reason — the lane is a default build since 2026-08-21 — the network
  fence is); its keys are secp256k1, so every position would leave the PQ domain;
  ADR-0087's M1–M8 and ADR-0088's L1–L10 are fold invariants proven against fold objects, and a
  version, a role change, a transfer and a certification are ML-DSA-87 bond signatures that no
  EVM account can produce; and a second copy of the class table is the two-products split
  ADR-0077 R0 exists to close.
* **Keep the truth in the fold and make the EVM its window and its hand** — the HyperEVM
  shape: precompiles that return fold rows, a writer whose calls become fold actions applied
  after the block, and per-class addresses at which a wallet's ERC-20 reader works. "Native"
  then means what it means on HyperEVM: implemented by the node's executor, no bytecode, the
  truth elsewhere. This is what the ADR decides.

And the ERC is reshaped, not copied: the position's token face keeps ERC-20's *read* half
(`name`, `symbol`, `decimals`, `totalSupply`, `balanceOf`, the events) and replaces its
*transfer* half with the curve (`price`, `quoteBuy`, `quoteSell`, `buy`, `sell`). It answers
`supportsInterface(ERC-20) == false` on purpose. An ERC-20 tool that only reads works; one that
transfers fails at the selector, loudly, and no contract can be written around it (§4 D6).

## 4. Decisions

**Decision 1 — Four system addresses in MISAKA's own range, and a facade family.** Following
`0x…F001/F002/F003`:

| address | name | kind |
|---|---|---|
| `0x…F010` | `ModelRegistry` | read precompile |
| `0x…F011` | `ModelAMM` | read precompile (quotes are pure functions of a fold row) |
| `0x…F012` | `ModelPosition` | read precompile |
| `0x…F013` | `ModelWriter` | the hand: `sendAction(bytes)`, escrow account — a call-frame intercept, F002's shape |
| `0x4d50 ‖ blake2b(b"misaka-evm/model-position-facade/v1" ‖ line_id)[..18]` | the line's **MRC-20 facade** | a native contract at a derived address (Decision 3): reads as a precompile, writes as an intercept |

Two mechanisms, chosen by what a call needs to see. The three reads and the facade's view
selectors are stateless precompiles installed through `register_all_misaka_precompiles` with a
handle on the selected parent's fold rows — input in, bytes out. The writer and the facade's
`buy`/`sell` are call-frame intercepts, because "the stateless precompile ABI … cannot see
`msg.sender` / `msg.value`" (`misaka/evm/core/src/withdraw.rs:1-8`) and Decisions 4–5 need both.

Hyperliquid's `0x0800…` and `0x2000…`/`0x3333…` ranges are not reused, so a contract ported
from HyperEVM cannot silently call the wrong thing. The facade's 2-byte prefix (`MP`) is
Hyperliquid's `0x20 ‖ token index` idea with a hash where an index would be a free field.
`ModelRegistry.facadeOf(line_id)` and `lineOf(address)` map both ways; an address with the
prefix that names no line behaves as an empty account. A line is ADR-0088's unit of the market —
a class, an owner, a name — and the founding line of a class has the class id as its line id.

**Decision 2 — What a read returns is the fold at the EVM block's selected parent, stated as
Hyperliquid states it.** Every read precompile returns rows of `fold(selected_parent(B))` — the
post-state of the selected parent's transition — because that is the one fold state that is a
function of `B`'s parents alone (v0.4 I2/I9). The guarantee, in the reference's words with the
referent changed: *the values are guaranteed to match the fold's state at the selected parent
of the EVM block.* A read of a class that does not exist, a window that was never opened or a
holder that has no row returns the zero value (not an error) — the zero is itself a fact.
Malformed input reverts and consumes the frame's gas, Hyperliquid's rule. Gas: `2,000 + 65 ×
(input_len + output_len)`, the reference's schedule taken as the starting point and re-measured
against `EVM_GAS_LIMIT` before the fence is armed (§5).

`ModelRegistry` (ADR-0056/0067/0075/0088 rows; every 64-byte id is two `bytes32` words): `classCount()`, `classAt(i)`, `classRow(class)` → `(status, sharePermille, budgetBlocks, canonicalLeaves, isBase, registrantPayload, registeredDaa)`, `certified(class, lane)`, `rootsInForce(class)` → the count and `rootInForceAt(class, i)`; `lineCount()`, `lineAt(i)`, `linesOf(class)` → count and `lineOfClassAt(class, i)`, `line(line)` → `(class, ownerPayload, developerPayload, maintainerPayload, current, versionsPublished, previewCount, contributorPermille, status, nameHash)`, `version(line, n)` → `(root, parent, adoptedFrom, runtimeHash, datasetCommitment, trainingConfigHash, notesHash, publishedDaa, publishedByPayload, status, untilDaa)`, `usage(line, n)` → `(attemptClaims, fpClaims, workLeaves, firstUsedDaa, lastUsedDaa)`, `evaluationCount(line, n)` and `evaluation(line, n, i)` → `(evaluatorId, scorePermille, reportHash, byPayload, postedDaa, isLinesOwn)`, `proposalCount(line)` and `proposal(line, i)` → `(proposalId, root, noteHash, byPayload, postedDaa, adoptedIn)`, `facadeOf(line)`, `lineOf(address)`, `chainDaa()` (the selected parent's DAA score — the fold's clock, the analogue of `l1BlockNumber`). Declared values are returned as declared and labelled so in the Solidity interface; nothing in the EVM treats a score as a fact.

`ModelAMM` (ADR-0087 rows and arithmetic, keyed by line): `market(line)` → `(openedDaa,
mskReserve, positionUnits, soldUnits, burned, ownerPaid, contributorPaid, closedToBuys, exists)`,
`price(line)` (sompi per position), `quoteBuy(line, mskIn)` → `(unitsOut, burn, leg, net,
priceAfter)`, `quoteSell(line, unitsIn)` → `(mskOut, burn, leg, net, priceAfter)`, `constants()`
→ `(supplyUnits, unitsPerPosition, virtualSompi, burnPermille, legPermille)`. The quotes call the
same `palw_model_buy_quote_v1` / `palw_model_sell_quote_v1` the fold calls, so a quote is the
fold's arithmetic and not a re-implementation of it. A quote is a quote against the selected
parent's row; Decision 7 says what a trade actually gets.

`ModelPosition`: `balanceOf(line, holderId)` for any 64-byte holder id, `balanceOfAddress
(line, address)` for the EVM namespace (Decision 7), `totalSupply(line)`, `sold(line)`,
`holderIdOf(address)`.

**Decision 3 — The MRC-20 facade: ERC-20's read half, the curve in place of its transfer half.**
At each line's facade address the executor implements, natively, the interface below; there
is no bytecode, `EXTCODESIZE` returns 0 and `EXTCODEHASH` the empty hash, exactly as for a
precompile, and the registry is how a caller learns that the address is live.

```solidity
interface IMRC20 /* MISAKA Request for Comment 20: a model position */ {
    // ---- ERC-20's read half, kept ----
    function name() external view returns (string memory);      // "MISAKA Model Position <8 hex of line id>"
    function symbol() external view returns (string memory);    // "MP-<8 hex>"
    function decimals() external view returns (uint8);          // 6: one position is 10^6 units (ADR-0087 D1)
    function totalSupply() external view returns (uint256);     // PALW_MODEL_SUPPLY_UNITS_V1, fixed at opening
    function balanceOf(address holder) external view returns (uint256); // EVM-namespace units only (D8)
    // ---- the curve, in place of the transfer half ----
    function lineId() external view returns (bytes32, bytes32); // the line, 64 bytes as two words
    function circulating() external view returns (uint256);     // sold_units, every namespace
    function price() external view returns (uint256);           // sompi per position
    function quoteBuy(uint256 mskInSompi) external view returns (uint256 unitsOut, uint256 priceAfter);
    function quoteSell(uint256 unitsIn) external view returns (uint256 mskOutSompi, uint256 priceAfter);
    function buy(uint256 minUnitsOut) external payable;         // = ModelWriter action 1 (D6); msg.value is the gross leg
    function sell(uint256 unitsIn, uint256 minMskOutSompi) external; // = ModelWriter action 2 (D6)
    // ---- settlement events, emitted at the settling block (D7), never at the call ----
    event Bought(address indexed holder, uint256 mskIn, uint256 unitsOut, uint256 priceAfter);
    event Sold(address indexed holder, uint256 unitsIn, uint256 mskOut, uint256 priceAfter);
    event Refused(address indexed holder, uint8 actionId, uint256 amount, bytes32 reason);
    // ---- absent by definition: transfer, transferFrom, approve, allowance, Transfer, Approval ----
    //      those four selectors revert NonTransferable(); supportsInterface(0x36372b07 /* ERC-20 */) == false
}
```

`decimals() == 6` and `totalSupply()` in units keep ADR-0087's arithmetic on the wire
unchanged; `msg.value` is in wei and must be a multiple of `EVM_NATIVE_SCALE` (the F002 rule),
so the fold's sompi leg is exact. `supportsInterface(type(IMRC20).interfaceId) == true`. A
facade for a line whose market is `closed_to_buys` (ADR-0087 D7; a retired line, ADR-0088 D6)
reverts `buy` at the call and still queues `sell`.

**Decision 4 — There is no `Transfer` because there is no transfer, and the EVM cannot build
one.** ADR-0087 Decision 5 is a fold fact: no object moves units between holders. On the EVM
that fact has to survive two constructions that would rebuild a transfer from the outside —
the wrapper (a contract buys positions and issues its own transferable shares) and the
delegate (a contract sells on a holder's behalf). Both die on one rule: **a holder is the
signing account and only the signing account** — `ModelWriter` and every facade write refuse a
call unless `msg.sender == tx.origin` and `EXTCODESIZE(msg.sender) == 0`
(`NotAnAccount`), so no contract ever holds or moves a position, and a position's only
counterparty is the curve. The cost is stated: no contract wallet, no multisig, no account
abstraction can hold a position on this lane; EIP-7702 is not in Shanghai and, if a later fork
admits it, the code-size clause is what keeps the rule. Hyperliquid lets contracts hold core
balances and act through CoreWriter; MISAKA cannot, because ADR-0087's constraint is that a
position never passes between persons, and a contract that holds one is a person-shaped hole
in that constraint.

**Decision 5 — `ModelWriter` is CoreWriter's shape with two actions.** `sendAction(bytes data)
payable`: byte 1 = encoding version (`1`), bytes 2–4 = action id big-endian, the rest = ABI:

| id | action | ABI | value |
|---|---|---|---|
| 1 | buy | `(bytes32 lineA, bytes32 lineB, uint256 minUnitsOut)` | `msg.value` = the gross BILI leg, wei, a multiple of `EVM_NATIVE_SCALE` |
| 2 | sell | `(bytes32 lineA, bytes32 lineB, uint256 unitsIn, uint256 minMskOutSompi)` | 0 |
| 3–255 | reserved | — | — |

No action founds a line, publishes a version, sets a role, registers a class or certifies a
family: those are ML-DSA-87 bond signatures on the UTXO side (ADR-0056, 0075, 0088), and an EVM
account has no bond. The call burns `PALW_EVM_WRITER_GAS_V1 = 25,000` gas before emitting
`ActionQueued(address indexed account, uint8 actionId, bytes data)` (Hyperliquid's ~25,000 and
~47,000 for a basic call, taken as the starting schedule; F002's in-tree figure is 9,000). The
block's **action list** is built the way F002's withdrawals are: the executor scans the
committed logs of the accepted txs for `ActionQueued` from `0x…F013` and decodes each into a
`PalwEvmMarketActionV1` in log order — a reverted tx leaves no log and therefore no action, and
a forged log from any other address is not the writer's. A buy's `msg.value` is held in the
writer's own account (`0x…F013`) — the **escrow** — until the settling block; a sell holds
nothing, because the units are the fold's and are debited there. At most
`PALW_EVM_MARKET_ACTIONS_PER_BLOCK_V1 = 128` actions per EVM block: the 129th reverts at the
call. Malformed data, an unknown line, `msg.value` not a multiple of the scale, or a sell of
zero revert at the call — the same "user-input fault ⇒ tx revert, block valid" class as F002.
`facade.buy` / `facade.sell` are the two actions with the line filled in by the address, and
nothing else: one write path, two doors.

**Decision 6 — The hand moves after the block, in the fold, and settles one block later.**

| step | where | what happens |
|---|---|---|
| **emit** | `EVM(B)` — the EVM block of chain block `B`, executing the accepted payloads of `B`'s mergeset | the writer validates the call, escrows a buy's value, appends `PalwEvmMarketActionV1 { seq, account, action, line, amount, min }` to the block's **action list**, emits `ActionQueued` |
| **apply** | `fold(B)` — `apply_palw_transition_v6` gains the action list as an input | after every carrier-borne object of `B` (ADR-0087's `ModelBuy`/`ModelSell` in acceptance order), the EVM actions in `seq` order: each is quoted on the row *as it then stands*, the `min` is checked, the row and the holder's units move (a buy credits `evm_holder(account)`, a sell debits it) or the action is **refused** with a reason; the outcome is written to the delta as `PalwEvmSettlementV1 { seq, account, line, outcome: Filled { units, sompi, priceAfter } \| Refused { reason } }` — a state-root collection that enters the root only when non-empty (ADR-0087's rule) |
| **settle** | `EVM(C)`, `C = the selected child of B` — `evm_chain_context_step(C)` runs system ops before user txs (v0.4 §3.2) | a second `EvmSystemOp` kind, `MarketSettle { seq, account, line, outcome }`: the producer of `C` puts `fold(B)`'s settlement list into `C`'s payload `system_ops` in `seq` order, and validation requires the set to equal the list derived from the selected parent exactly — a missing, extra or reordered op disqualifies `C` as a bad claim does — so v0.4 I2 (the EVM result is a function of the parents and the system ops) holds unchanged and `system_ops_root` commits them. A filled buy **burns the escrow** (writer balance −= gross) and `apply_evm_bridge_effects` materialises the line's **sink output** (`OP_RETURN ‖ MSKMDL01 ‖ line`, value = gross) into `C`'s own UTXO diff under a new outpoint domain `MISAKA_EVM_MARKET_SINK_V1` — F002's mechanism with the sink script in place of the withdrawal script; a refused buy **refunds the escrow** to `account`; a filled sell **credits** `account` with `net × EVM_NATIVE_SCALE`, a `DepositClaim`-shaped credit with no lock and no tip; each emits `Bought` / `Sold` / `Refused` from the facade in a system receipt at index 0 of `C`'s EVM block |
| **visible** | `EVM(C)` and after | precompiles at `C` read `fold(B)`, which already holds the moved row: a position bought at `B` is readable, and settled, one chain block later |

Order and price: carriers first, then EVM actions, so an EVM trade is always quoted after
the block's UTXO-side trades — the MISAKA form of Hyperliquid's "delayed onchain for a few
seconds", and for the same reason: the EVM is not a faster door to the same book. `min`
protections refuse, never partially fill (ADR-0087 M5), and a refusal costs the caller its gas
and nothing else. The escrow's shape is what makes refusal safe: the value never reached the
fold, so a refused buy is a refund inside the EVM at `C`, not a coinbase payout.

**Decision 7 — Two holder namespaces, and neither reaches the other.** An EVM account's holder
id is `evm_holder_v1(chain_id, address) = blake2b_512_keyed(b"misaka-palw/model-market/holder/evm/v1", chain_id ‖ address)`,
a `Hash64` in the same `holder` field ADR-0087 keys positions by. A position bought from the
EVM is sold from the EVM; one bought by a carrier is sold by a carrier; no object moves units
between an ML-DSA holder and an EVM holder, because that would be the transfer ADR-0087 D5
forbids, in a costume. Payouts differ by namespace: a carrier holder's sell is a `PalwPayoutV2`
the coinbase honours (unchanged); an EVM holder's sell is a settlement credit at `C`
(Decision 6), and **the coinbase never emits it** — the fold's payout table gains
`PalwPayoutKindV2::EvmCredit { address }` beside the payload kind, and the coinbase skips that
kind while the EVM step consumes it. The security label of an EVM-held position is ADR-0023's
`CLASSICAL-ECC`, and `getPalwModelPositions` returns it per row so a wallet shows it; the
fold's own positions keep the PQ domain. The owner's and an adopted contributor's legs (ADR-0087 D4, ADR-0088 D8) are unchanged by
which door the trade came through: they are fold payouts to bonds' payloads.

**Decision 8 — Supply, stated with the new pools in it.** Design v0.4 §9.1 becomes

```
UTXO_balances + EVM_deposit_locks + EVM_native_balances (the writer's escrow included) + burned == issued
```

with two consequences the fold's books make visible: a filled EVM buy moves value from
`EVM_native` (the escrow) to `UTXO_balances` (a dead sink output) at `C`, so between `B` and `C`
the fold's `msk_reserve` says "received" one block before the sink exists — a one-block
receivable, the same lag a carrier's escrowed reward has before its coinbase; and a filled
EVM sell raises `EVM_native` by `net` at `C` and `issued` by the same, exactly as the coinbase
raises `issued` for a carrier holder's sell — the sink outputs on the UTXO side are dead by
script, and ADR-0087 M2 (`Σ msk_in = reserve + Σ payouts + burned + registrant + author`) is
the identity that says the re-issue never exceeds what was sunk. The
`evm_total_native_balance` accumulator (v0.4 §4.2) takes the escrow burn and the credits as
system-op deltas, so the O(1) check stays O(1).

**Decision 9 — The fence, and where it can be armed.** `palw_model_evm: Option<ForkActivation>`
on the params, top level, bare; `validate_palw_v2` refuses it unless `palw_model_market` is
armed at or before it and `evm_activation_daa_score ≤` it — on mainnet the lane is `u64::MAX`,
so the market's EVM face waits for ADR-0020's mainnet activation and says so; on testnet-11 and
devnet both preconditions hold today. Below the fence the four addresses and the facades are
empty accounts (the F003 idiom: "below the fence the handler is not registered, so a call is
byte-identical to calling an empty account"), the writer accepts nothing, the transition takes
an empty action list, and the fingerprint moves only where the flag is set. The lane's own
fences are bare DAA scores (`evm_*_activation_daa_score`, five of them); this one is the PALW
idiom's `Option<ForkActivation>` because `for_each_fence` visits it, and the executor reads the
same value through `Params::palw_model_evm_active_at(daa)`, so there is one switch, not two. No state-root
version bump: the settlement list and the EVM-namespace rows enter the root only when non-empty
(ADR-0087's implementation rule; ADR-0088 D11).

**Decision 10 — What HyperEVM has that this ADR does not take, and why.**
* *The asset crossing by system address.* A position never crosses: the facade has no supply,
  holds none, and links to nothing — there is no "total non-system balance on the other side"
  to keep, and so nothing that "is not checked". Only BILI crosses, and it crosses by the bridge
  that exists (deposit lock/claim, F002, and now the escrow/settlement pair).
* *Contracts as core actors.* Refused by Decision 4.
* *Dual blocks.* The EVM block cadence is the selected chain's, and the chain's interval is
  `header.bits`' (ADR-0071 as amended); a market action is one small tx, and the registry's
  writes do not run in the EVM.
* *The same address on both sides.* Impossible across ML-DSA-87 and secp256k1 (§2); Decision 7
  is the honest replacement.
* *Priority fees burned.* v0.4 §8 pays them to the payload miner; unchanged.

**Decision 11 — Lane 1 is the same primitives with post-quantum keys.** ADR-0023's Lane 1
(`PQ-AUTHENTICATED`) shares one engine with Lane 2 and differs by "auth profile, chain/lane
domain, precompile policy, and bridge policy" (acceptance condition 3). The four addresses and
the facade family are the model-market entry of both lanes' precompile profiles; a Lane 1
holder id is `evm_holder_v1(lane 1's chain id, its ML-DSA-derived address)` — a third
namespace, PQ-labelled, still never crossing. The writer's action list and the fold's
settlement list are FD-XLANE's outbox and inbox, one anchor apart, which is why this ADR needs
no synchronous cross-lane call and adds none.

**Decision 12 — What ships beside the node.** `misaka/evm/contracts/misaka-model/` with `IMisakaModelRegistry.sol`,
`IMisakaModelAMM.sol`, `IMisakaModelPosition.sol`, `IMRC20.sol`, `IMisakaModelWriter.sol` and
the address constants — the `L1Read.sol` role; `eth_call` against the precompiles and facades
works from any Shanghai toolchain at chain id `0x4D534B` with no MISAKA-specific RPC; receipts
of the settling block carry the facade events under the standard log format
(`docs/evm-differences-from-ethereum.md`'s rule: the receipts *root* is MISAKA's, the log
fields are standard); `getPalwModelMarket` gains `evm_namespace_units`, `getPalwModelPositions`
gains the label; `misaka palw model-buy|model-sell` gain `--via evm` for the operator's own
account; the explorer's Model Market page shows one curve with two doors.

## 5. What this costs, stated before it is measured

* **Gas, per action (user-paid):** a basic writer call ≈ 47,000 (Hyperliquid's measured figure
  for the same shape: ~25,000 burned + the call and the log; F002 charges 9,000 for a
  comparable intercept, so the 25,000 is a ceiling to measure down from), so 128 actions ≈ 6.0M
  of the block's 30M. A facade `balanceOf` read ≈ `2,000 + 65 × (36 + 32)` = 6,420; a `quoteBuy` ≈
  `2,000 + 65 × (100 + 192)` ≈ 21,000.
* **System gas, per settlement (nobody pays; capped):** 25,000 per settlement op, the deposit
  claim's figure; 128 × 25,000 = 3.2M, and 256 claims × 25,000 = 6.4M, together 9.6M under the
  existing `MAX_SYSTEM_GAS_PER_EVM_BLOCK` = 10M — which is how 128 was chosen.
* **Fold:** O(1) per action, after the carriers; the quote is the existing pure function.
* **State:** one settlement row (≈ 120 bytes) per action, dropped when `C` consumes it; one
  position row per (class, EVM holder), ADR-0087's ≈ 150 bytes.
* **Bytes on the wire:** an action is one EVM tx (≈ 200 bytes) in a 128 KiB payload; no
  carrier, no chunk group.
* **Latency:** emit at `B`, applied at `fold(B)`, settled and visible at `C` — one chain block,
  which at the RC's cadence is the interval `bits` sets. The reference's "a few seconds" is a
  design choice made for the same reason; here it is one block for a structural one.
* **The fee table is ADR-0087's, unchanged**, with ADR-0088 D8's owner/contributor split of
  the leg: 5 % burned, 1 % to the line's owner (and its adopted contributor), 94 % net, 12 %
  round trip before slippage — from either door.

## 6. Security — the four principles, checked before it is built

*A free field is a free draw; silence is not a verdict; weight is what certification buys; the
chain never takes the host's word* (README §"Security amendments").

| # | attack | what stops it, and the residual |
|---|---|---|
| A1 | **A wrapper**: a contract buys positions and sells its own transferable shares. | Decision 4: a contract cannot be a holder (`NotAnAccount`). Residual: off-chain custody of a key is outside every protocol; stated, not closed. |
| A2 | **Reentrancy** through a precompile or the writer. | Reads are pure functions of the selected parent's fold and make no calls; the writer makes no calls and returns nothing; settlement runs as system ops before any user tx (v0.4 §3.2). Nothing re-enters. |
| A3 | **Front-running the curve inside a block**: an accepting miner orders EVM txs to trade ahead of a buy. | The accepting block's EVM order is the mergeset's canonical acceptance order, not the miner's choice of sequence; `min` protections refuse; carriers trade first by rule, so the EVM door is never the faster one (Decision 6). Residual: ordinary EIP-1559 priority competition among EVM txs, as on every EVM. |
| A3′ | **Occupying the action budget without outbidding** (mainnet audit 2026-09-06, M-16/L-6). | A3's "ordinary EIP-1559 priority competition" is true of the MEMPOOL — `mining/src/evm_mempool.rs` caps a sender at 256 txs and selects the template by effective tip — and is NOT true across payload blocks: `processor.rs` sources the executed set from `consensus_ordered_mergeset` (selected parent first, then ascending blue work), so an attacker who mines, or who buys placement in the selected parent's payload, reaches the writer without outbidding anyone. A refused sell costs only gas (Decision 6 puts the position check in the fold, deliberately), so the budget was fillable cheaply by one account. Bounded by `MAX_MARKET_ACTIONS_PER_EVM_BLOCK_PER_ACCOUNT` = 16: eight distinct accounts must cooperate to fill a block. Residual, stated: eight cooperating accounts still can, at a cost of gas ×128 per block sustained. |
| A4 | **Escrow stranding**: value enters the writer and no settlement ever comes. | Every action in `EVM(B)`'s list has an outcome in `fold(B)` (filled or refused) and every outcome has a system op in `EVM(C)`; a chain block with a non-empty list and a child that omits the ops is `CommitmentMismatch` → disqualified. The F002 residual-locking test (`misaka/evm/core/src/executor.rs:1417-1427`) is the template: "no value stuck", asserted; `apply_evm_bridge_effects` is where the sink lands, beside the withdrawals. Residual: a reorg that drops `B` drops the escrow with the block, as it drops a withdrawal — the existing reversed-diff path. |
| A5 | **A fake `tx.origin`**: an account that is a contract under a future fork (EIP-7702). | The code-size clause in Decision 4; the fork that admits 7702 must re-decide it explicitly. |
| A6 | **Gas griefing**: fill the block with actions that will be refused. | Each costs its caller the full call gas; refusals are free for the fold (O(1)) and for the settling block (a refund op under the system-gas cap). |
| A7 | **Supply**: mint through the credit path. | An EVM credit is written only by the fold from a filled sell, whose `net` came off the reserve ADR-0087 M2 bounds; the coinbase skips `EvmCredit`; the accumulator check (Decision 8) is O(1) and every node runs it. |
| A8 | **The chain takes the host's word**: a precompile answering from RPC state. | A read is served from the consensus store's fold row for the selected parent — the same rows `evm_commitment_root` is computed over; an executor answering from anywhere else produces a different commitment and is disqualified. |
| A9 | **Cross-namespace laundering**: buy on the EVM, sell on the UTXO side to move BILI across the bridge without the bridge. | No object crosses namespaces (Decision 7); the only BILI path is the bridge's own, and a sell pays the namespace it was bought in. |
| A10 | **Security downgrade by default**: a wallet routing a PQ holder into the EVM door. | Positions never move between doors; a wallet cannot route what does not move. ADR-0023 FD-SUPPLY's display rule applies to the *label* of an EVM-held position, which `getPalwModelPositions` carries. |
| A11 | **Weight**: an EVM action bearing on fork choice. | It bears on nothing but a market row: no share, no budget, no ticket, no blue work (ADR-0087 §2's "a position grants nothing but the right to sell it back"). |

## 7. Invariants the tests must hold

* **E1 (window).** For every EVM block `B` and every precompile call, the returned rows equal
  `fold(selected_parent(B))`'s; two nodes with the same store answer byte-identically; a call
  below the fence is an empty-account call.
* **E2 (one hand).** The only writes the EVM can cause in the fold are the two actions; a
  property test over the writer's decoder finds no third; the facade's `buy`/`sell` reach the
  same code path as `sendAction` with ids 1 and 2.
* **E3 (no transfer, from either side).** ADR-0087 M3 holds over the object set *and* the
  action set; the facade's four ERC-20 transfer selectors revert; `supportsInterface(ERC-20)`
  is false; a call with `msg.sender != tx.origin` or non-empty code is refused.
* **E4 (order).** In `fold(B)`, every carrier-borne market object applies before every EVM
  action, and EVM actions apply in `seq` order; a golden trace with both kinds fixes the prices.
* **E5 (settlement is total).** `|actions(B)| == |settlements(B)| == |settlement ops(C)|`; a
  filled buy's escrow burn equals its sink output's value; a refused buy's refund equals its
  escrow; a filled sell's credit equals `net × EVM_NATIVE_SCALE`.
* **E6 (supply).** Decision 8's identity holds at every chain block, including across `B`/`C`
  with the receivable stated; the accumulator equals the sum of EVM balances.
* **E7 (namespaces).** No fold transition changes an ML-DSA holder's units by an EVM action or
  an EVM holder's units by a carrier object; `evm_holder_v1` is injective on (chain id,
  address) up to the hash.
* **E8 (ADR-0087 M1–M8 and ADR-0088 L1–L10 hold unchanged** with EVM-originated moves in the
  recorded sequences.
* **E9 (the fence).** Fingerprint unchanged where `None`; arming without `palw_model_market` or
  on a lane-inert preset is refused at validation; a chain with the fence armed and no action
  ever emitted has the same state root as one without the fence.
* **E10 (reorg).** Dropping `B` drops its action list, its settlement rows and its escrow with
  the block's reversed diff; re-adding it reproduces them.
* **E11 (bounds).** The 129th action in a block reverts; system gas per EVM block never exceeds
  the cap with claims and settlements together.
* **E12 (labels).** Every EVM-namespace row returned over RPC carries `CLASSICAL-ECC`.

## 8. Order of work

1. Fold: the action input to `apply_palw_transition_v6`, `evm_holder_v1`, the settlement
   collection, `PalwPayoutKindV2::EvmCredit`, E4–E8.
2. Executor: the three read precompiles, the writer with escrow, the facade family, the
   settlement system ops with their receipt and logs, E1–E3, E5, E11.
3. Processor: the action list carried from `evm_chain_context_step(B)` into `fold(B)`, and the
   settlement ops derived for `C`; the commitment covers both; E10.
4. Params: the fence and its two preconditions; the fingerprint pin; E9.
5. RPC/CLI/explorer; the Solidity interfaces; the wallet label; E12.
6. Devnet drill (both lanes of the market on one class: carrier buy, EVM buy, EVM sell, refused
   buy, reorg across `B`/`C`); then testnet-11 by activation.

## 9. Implementation record (2026-09-05, `palw-adr0088-0089-impl`)

**Landed — §8 items 1–5 (fold, executor, processor, params, RPC/CLI and the Solidity
interfaces); item 6 (the devnet drill) is not.** The fold half is commit `bf4ed1d8`; the rest is
the branch's closing commit.

| where | what |
|---|---|
| `consensus/core/src/evm/model_market.rs` | Decision 1's addresses (`0x…F010`–`0x…F013` through `system_address`) and `facade_address_v1` (`0x4d50` ‖ 18 bytes of BLAKE2b over the line id); `evm_holder_v1(chain_id, address)` (Decision 7); `PalwEvmMarketActionV1` / `PalwEvmSettlementV1` with the refusal codes; `PalwEvmMarketFencesV1`; the view `PalwEvmViewV1` (classes, lines, versions, proposals, evaluations, markets, positions — flattened rows the doors answer from and nothing else); `synthetic_market_sink_txid`; the bounds (128 actions an EVM block, 25,000 gas a settlement, 25,000 gas a writer call). |
| `palw_state_v2.rs` | `evm_settlements` on the state (root only when non-empty; carriage tail `0x89`); `PalwTransitionExtrasV1 { evm_market_active, evm_actions }`; slot 1c′ drains the selected parent's settlement list, slot 3c applies the block's actions after every carrier-borne market object (E4) through `model_buy_v1` / `model_sell_v1(.., pay_net_via_coinbase = false)` — an EVM buy is a position under the EVM holder id, an EVM sell's net is the settlement row and never a coinbase payout; a refused action is a settlement with a reason, never a fault of the block; `evm_view_v1(chain_id, base_class_id)`. |
| `config/params.rs`, `palw_mode_v2.rs` | `palw_model_evm: Option<ForkActivation>`; `validate_palw_v2` refuses it before `palw_model_market` (`ModelEvmBeforeMarket`) and on a preset whose EVM lane is inert (`ModelEvmOnInertLane`); `palw_evm_market_fences_at(daa)`; pinned in the fingerprint; `None` on every preset. |
| `consensus/core/src/evm/mod.rs` | `EvmSystemOp::MarketSettle(PalwEvmSettlementV1)`; `EvmExecutionResult.market_actions` (not committed — the fold recomputes nothing from it; the settlements it produces are). |
| `misaka/evm/core/src/model_market.rs` | The intercept: `MarketHandlers` wrapping `handler.execution.call` (the F002 seam's shape) — the three read doors decode the ABI and answer from the view; the facade family answers ERC-20's read half, refuses its transfer half with `NonTransferable`, and routes `buy`/`sell` to the writer's path; the writer charges its gas, refuses a static frame, a caller that is not `tx.origin` or has code (Decision 4), a dormant market, an unknown line, a market closed to buys, a value that is not whole sompi, the 129th action, then moves the escrow caller → writer in the tx's journal and emits `ActionQueued` — the two unwind together on a revert; `decode_action_log` (strict: address, topics, lengths, zero padding) and `settlement_log` (`Bought` / `Sold` / `Refused` from the line's facade). |
| `misaka/evm/core/src/executor.rs`, `precompiles.rs`, `sim.rs`, `trace.rs` | `EvmBlockInput.market: EvmMarketInput { palw_view, fences, expected_settlements, chain_id }`; the `MarketSettle` ops validated equal, in order and in count, to the selected parent's list (E5) and applied before any user tx — a filled buy burns the writer's escrow, a refused buy reroutes it to the account, a filled sell credits the net — their events in ONE system receipt at index 0 (user receipt indices unchanged, `accepted_tx_count` counts user txs); the committed `ActionQueued` logs scanned into `market_actions` in log order; Decision 8's accumulator `+ credited − burned`; `register_all_misaka_precompiles(handler, f003_active, market)`; `EthCallEnv { palw_view, market_fences, chain_id_for_holders }` so `eth_call` registers the same doors. |
| `consensus/src/processes/evm/mod.rs`, `virtual_processor/processor.rs`, `body_validation_in_isolation.rs` | `evm_chain_context_step` builds the view and the fences from the selected parent's fold and the block's DAA, hands the parent's settlement list as `expected_settlements`, validates the payload's ops (`validate_evm_market_settlements`), materialises a filled buy's escrow as a sink output (`apply_evm_market_effects`, `synthetic_market_sink_txid`) and feeds `market_actions` into `PalwTransitionExtrasV1.evm_actions`; the EVM pre-execution pipeline is bypassed while the fence is armed (a block's fold input is its own); the template builder pushes the selected parent's `MarketSettle` ops and mirrors the sink effects; the system-op cap split (claims ≤ 256, settlements ≤ 128, E11). |
| `kaspad/src/eth_rpc.rs`, `consensus/core/src/api/mod.rs`, `consensus/mod.rs`, `session.rs` | `palw_evm_view_v1()` — the tip's view and the fences at the tip's DAA — set on the head `EthCallEnv`; a historical block's env carries no view (recorded below). |
| `misaka/evm/contracts/misaka-model/` | `MisakaModelAddresses.sol`, `IMisakaModelRegistry.sol`, `IMisakaModelAMM.sol`, `IMisakaModelPosition.sol`, `IMRC20.sol`, `IMisakaModelWriter.sol`, a README — 64-byte ids as two `bytes32` words; compiled with solc 0.8.28. |

**Tests.** `palw_state_v2::tests::evm_market` (4): a buy from the EVM is a position in the EVM
namespace and a settlement the child carries (E2, E7); a sell from the EVM credits through its
settlement and never the coinbase (E2, E6); a refused action is a settlement and not a fault of the
block (E5); the settlement tail rides the carriage only while a list waits (E10's revert side).
`evm::model_market` (3): the addresses are where the ADR puts them, the holder id binds the chain
and the account (E7), the view answers by id and by facade (E1). `kaspa_evm::model_market` (4):
selectors and the interface id excluding ERC-20 (E3), the action record round-trips through the
log, `sendAction` calldata decodes strictly, the settlement log names the facade and the outcome.
`kaspa_evm::executor` (2): the writer escrows and the block lists its actions in order — a buy, a
sell, a torn buy reverted, and the same calls finding an empty account while the fence is dormant
(E2, E9, E11's revert shape); a settlement block burns, refunds and credits exactly the parent's
list, the system receipt at index 0 ahead of the user's, the supply identity across a deposit, a
burn, a credit and a basefee burn (E5, E6), and a short, altered or unexpected list refused whole.
The params test pins E9. E4 and E8 hold by the fold's slot order and by ADR-0087's and ADR-0088's
suites running unchanged over the same state (1,868 green). E10's forward side (dropping `B`) and
E12 are not tested — see below.

**What the implementation taught.** (1) `PalwPayoutKindV2::EvmCredit` (§8 item 1) was not needed:
the settlement row *is* the credit, and the executor pays it at `C`; a payout kind would have been
a second copy of the same number. (2) The settlement list has to live in the fold's state for one
block (not only in `B`'s result) so that `C`'s validation, `C`'s template and a node that replays
`B` from its delta all read the same list. (3) The action list must be scanned from the *committed*
logs, not from the writer's own counter — a tx that queued and then failed leaves no log, and the
counter is resynchronised from the scan at every tx. (4) The escrow moves inside the tx's journal,
so a revert after the queue (out of gas in the same frame is impossible, but the shape is kept)
unwinds both the value and the log together.

**The devnet drill (§8 item 6), run 2026-09-05.** `scripts/drills/misaka-palw-model-market-devnet-e2e.sh`:
six fixture-PoW producers under the devnet's public-seed bonds, the three fences armed from genesis
(`--palw-model-devnet=0`), the floor-only ruleset (`--palw-devnet-floor-only`) and the bridge's
DNS-finality gate waived (`--evm-bridge-devnet-unpaused`) — three private-devnet-only `kaspad`
flags this drill added. Run 8 passed 12/12: the founding line read through `getPalwModelLine`; a
carrier buy of 5 MSK folded on every node (467,801,333 units); an EVM account funded through
`EVM_DEPOSIT_LOCK` + claim (30 MSK); an EVM buy of 3 MSK queued in `B` and settled in `C` into
278,585,835 units the position window returned; a refused buy (an impossible floor) refunded its
escrow in `C` and moved no position; an EVM sell of one position debited exactly 10^6 units and
credited 954,000 sompi net (94 % of the 1,015,085 gross the curve gave); six nodes returned one
row, one balance, one position; a partition {0,1} | {2,3,4,5} in which the minority queued and
settled a 2 MSK buy across its own `B`/`C` while the majority outgrew it (+2 vs +8), and a heal
after which all six agreed again — on a row that carried the buy, re-included and re-settled on
the winning chain (E10: the losing `B`/`C` dropped with their escrow, the action found its way back
through the mempool); no node logged a settlement mismatch, a dropped market object or a panic.
Seven earlier runs died on the harness and on ADR-0087's sink (its §7 records the defect); the
drill's own traps are in the script's comments.

**Not landed.** A historical `eth_call` sees the market's doors closed — the fold is kept only at
the tip and a past view would need a replay this RPC does not run (`eth_rpc.rs` says so at the
site); the `CLASSICAL-ECC` label (E12) — the fold keys a position by holder id alone and cannot
tell an EVM-derived id from an ML-DSA one, so the RPC has nothing to label with; the wallet label;
the explorer; arming on testnet-11.

## 10. What is deliberately not decided

* The gas schedule's final numbers (`2,000 + 65·n`, 25,000, 47,000) — Hyperliquid's are the
  starting values; MISAKA's are set from a measurement against `EVM_GAS_LIMIT` before arming.
* Whether ADR-0088's *declared* rows (evaluations, dataset and config hashes) should be served
  by the precompile at all, or left to the explorer; the registry's shape carries them either
  way, labelled as declarations.
* A Lane 1 (PQ-EVM) writer that can carry bond-signed objects (a version, a role change) —
  possible only where the lane's native auth *is* the bond's key (ADR-0023 O-03), and its own ADR.
* Whether a contract wallet could ever hold a position under a stricter rule (for instance, a
  wallet whose only owner is one EOA and whose code is a known hash) — the operator's legal
  reading of 交換業 decides whether "held by a contract" is "held by a person", and nothing
  technical does.
* The facade's `name()`/`symbol()` text beyond the shape above.

## 11. Number hygiene

This is ADR-0089. The README's next free number was 0089 after 0088's row; it becomes 0090 with
this row. First written on `docs/adr-0089-evm-model-primitives`; revised with ADR-0088's
2026-09-05 rewrite on `palw-adr0088-0089-impl`, where 0087 → 0088 → 0089 read in one line.
~~~~~~~~
<!-- ADR_SOURCE_END:0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md -->

</details>

<a id="source-0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time"></a>

<details>
<summary>0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md — 原文 L1–L432</summary>

[原文を開く](../adr/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md) · SHA-256: `ec76b956e6402a3f6f6a0441b96aba9da29e7f0dd9cd606ff5855eb3eab4c8a9`

<!-- ADR_SOURCE_BEGIN:0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md -->
~~~~~~~~markdown
# ADR-0125 — The execution lane is a second lane inside the cadence, and it widens one permit at a time

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: **IMPLEMENTED 2026-09-17** on `feat/palw-exec-lane-and-validator-retirement` (from `main` at
  `6fdf6ba7`, after ADR-0124), including the stage table (§7.2), equivocation evidence and relay
  de-duplication (§7.3), the RPC read and the operator's status line (§7.4), and the devnet drill
  (§7.1). **testnet-11 schedules it at DAA 7,101 at one permit per one-second round**. `6001` is the
  bundle's retained logical label in historical audit and test material, not a second activation
  height; `None` on every other shipped preset.
* Operator's request: "testnet では BPS 1 から実装して最終目標を BPS 10 とする", with the design
  quoted in §1.
* Builds on: [0060](0060-the-liveness-doctrine.md) / [0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)
  (a lane with its own algorithm id, a constant target that never touches `bits`, zero subsidy),
  [0072](0072-the-ticket-is-the-execution.md) (an attempt's execution commitment is a value nobody
  re-rolls for free), [0075](0075-certification-is-a-consensus-object.md) (a certified family is chain
  state), [0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md) (what a second lane does to a
  bonded block at `ghostdag_k = 1`), [0107](0107-a-share-grows-on-work-that-reached-final.md) (`Final`
  is what is counted), [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md).
* Amends nothing: no DAG parameter, no PALW window and no cadence moves (§8 says why an earlier draft
  thought they had to). Supersedes nothing.

## 0. The sentence this ADR is

**Transactions get a fast lane without the PALW chain getting a fast clock. Every second is a round;
a round's permits go to bonds whose attempts reached `Final` in the previous scheduler span, in
quotas proportional to the compute those attempts certified and capped at 45 % per security domain,
one permit an operator, no domain in two consecutive rounds. A permit is a round block: a light, fee-only block that is never
a selected parent, never blue and never counted in the DAA score, so the chain's GHOSTDAG, windows,
retargets and depths are exactly those of the DAG without it. A merging chain block accepts a round
block's transactions when its parent state grants the permit, and pays the fees to the bond's
payout. 1 BPS is one permit a round; 10 BPS is ten, carried when enough security domains and
operators fill them (SA-4).**

## 1. What the operator asked, in the operator's words

> 120 秒の PALW Anchor あたり 1200 個の Execution Slot にすれば … PALW のユーザー向け実行レイヤーを
> 10 BPS まで持っていけます。PALW 推論そのものを 10 BPS で実行するわけではありません。
>
> 1200 blocks すべてに「新しい PALW work」を付けない。… execution block 自体は 追加 PWU = 0、追加 PALW
> reward = 0。
>
> 10 BPS なら「10 本 × 1 BPS」の発想が良い。… slot interval を 1 秒 → 100 ms へ縮めるのではなく、1 秒
> round の並列幅を 1 → 10 へ増やす。
>
> 同一 security domain 連続禁止 … model_id ではなく security_domain_id で判定。同一 operator 最大 1/10。
>
> claim E の Credit = epoch E+1 から使用。
>
> testnet では BPS 1 から実装して最終目標を BPS 10 とする。
>
> 現在の方式は beacon はもうないのでは

The design text proposed the DNS epoch beacon `R_E` as the seed. That overlay depends on validators
and is being retired; this lane reads no beacon (Decision 3).

## 2. What exists, and what refuses the obvious design

* **The 120-second cadence is frozen.** `validate_palw_v2` refuses a `ConsensusV2` params set whose
  `target_time_per_block`, `ghostdag_k`, mergeset limit, merge depth, coinbase maturity or sample rates
  are not the two-minute set. Depths are plain `u64`s in `BlockrateParams`, not `ForkedParam`s.
* **`ghostdag_k = 1`.** A block a second on the chain's own DAG would colour every bonded block red
  (ADR-0105's incident at eight heartbeats), advance the DAA score 120 times faster than the PALW
  windows assume, and put a stale attempt outside merge depth before it could be merged.
* **Block production has no beacon.** The attempt ticket is `H(execution_commitment_v3(attempt,
  execution_anchor))`; the ADR-0074 walk serves the free-prompt receipt lane only.
* **A header hash is not seed material.** A header's nonce re-rolls it for free.

So the lane's blocks cannot be chain blocks. They are blocks beside the chain.

## 3. Decisions

**Decision 1 — a round block is a sidecar: algo 10, never a selected parent, always red, outside
the DAA set.** `POW_ALGO_ID_PALW_ROUND_V1 = 10` is admitted only past `palw_execution_lane`'s
activation, ORed into the header gate as the heartbeat is. Its target is the constant `2⁻¹⁶`
(`PALW_ROUND_WORK_LOG2`), its tag the hash lanes' tag, its level zero, its blue work zero, and it is not
a priced difficulty row. GHOSTDAG never selects it and colours it red before any k-cluster walk
(`GhostdagManager::is_round_block`); the window manager treats it as outside the DAA set whatever its
blue score, so it is neither counted in a DAA score nor sampled into a difficulty or median-time
window. Its coinbase declares zero subsidy and carries no outputs; its `palw_state_root` is zero (the
header gate requires it). `mergeset_size_limit` counts only the chain's own blocks; round blocks have
their own bound (Decision 6). It is never a sink candidate, never virtual's selected parent and never
the headers' selected tip.

**Decision 2 — a round block hangs from an anchor on the chain.** A block that is not a round block
names at least one parent that is not one, and its selected parent is the heaviest of those. A round
block names at most one: that one is its anchor (its selected parent) when it moves the lane onto a
newer chain block; with none, it extends the lane from its round parents and its selected parent is
the heaviest of their anchors. After GHOSTDAG every round parent's anchor must lie on the block's
selected chain (`check_round_lane_mergeset`).

The consequence is the property the lane rests on: **a round block's past brings no chain block into
any mergeset that its anchor's past had not already brought.** By induction a round block's chain
ancestors are its anchor's past, and the anchor is a chain ancestor of every block that names the
round block; so a chain block's mergeset restricted to chain blocks, its reachability among chain
blocks, its blue score, blue work, DAA score, windows, merge-depth root and finality point are those
of the same block in the DAG with every round block removed. A producer anchors at the sink's
selected parent, which the next chain block can name beside the round tips.

**Decision 3 — a round is a second, and its permits come from the previous span's finalized
attempts.** `round(t) = (t − genesis_timestamp) / 1000`; a span is `schedule_span_daa` of DAA.
When an attempt claim reaches `Final`, the fold records `(domain, bond, operator, claim id, execution
root, credit)` in the span of the finalizing block (`record_round_final`). The credit is the compute
the attempt certified — the operator's "Credit 量 = canonical compute": its exposure pwu (one canonical
inference of its class, the number its exposure and its ADR-0124 price are read on), capped at
ADR-0124 Decision 6's unit, the dearest weight-bearing model class's (`palw_execution_credit_v1`). At the first block of a later span
those finals become the next span's schedule and leave (`rotate_round_lane`,
`palw_execution_schedule_v1`):

* the seed is `H(span ‖ count ‖ (claim id ‖ execution root)*)` over the finals in claim-id order —
  changing it costs an inference that reaches `Final`; no beacon and no header enters it;
* a domain's credits are the sum of its finals' credits, its quota is its share capped at `450 ‰`,
  water-filled in exact integers (`palw_execution_quotas_v1`: one domain holds the lane, two split it evenly, three or more
  are capped and re-divided), listing at most 32 domains and 64 bonds a domain;
* the domains are split into two parity groups as evenly as a greedy split makes them
  (`palw_execution_parities_v1`); a domain holds permits only in rounds of its parity, so no domain
  holds permits in two consecutive rounds at any width.

A round's permits are drawn from the schedule alone (`palw_execution_permits_v1`): for each index,
a domain of the round's parity with room under `⌈width / 3⌉` is picked in proportion to its quota by
`H(seed ‖ round ‖ index)`, and within it the bond with the lowest `H(seed ‖ round ‖ bond)` among those
whose operator holds no permit yet. Credits earned in span `s` are spendable only in `s + 1`.

**Decision 4 — the permit travels signed in the header.** A round block's `palw_commitment` is a
`PXR1` envelope `{ version, network domain, round, permit index, bond, pubkey, signature }`. The
header stage checks its shape, that its round is the header timestamp's, and the ML-DSA-87 signature
over `H(network ‖ pre-PoW hash ‖ timestamp ‖ nonce ‖ round ‖ index ‖ bond)` under the carried key
(`palw_carriage_stateless_v1`). The nonce is signed: the envelope is inside the block identity and
outside the PoW pre-image, so a signature that did not cover it could be re-announced under any
re-solved nonce by anyone.

**Decision 5 — the merging block decides the permit, from its parent state.** For each round block in
its mergeset, a chain block grants the permit when (`palw_round_verdicts_v1`):

* the anchor's span is the merging block's span or the one before (the two the fold keeps);
* that span's schedule grants `(round, permit index)` to the envelope's bond at the lane's width;
* the bond is registered, `Active`, and its key is the envelope's;
* the round block's coinbase names the bond's registered payout;
* the permit is not already in the ledger.

A round block without its permit is merged and coloured like any red, and none of its transactions is
accepted. The permits a block grants are recorded by its fold (`record_round_permits`) under
`(span, round) → bit per index`; the ledger and the schedules older than the span before the block's
are dropped at the span boundary. The template, the virtual state and every validating node read
one parent state and give one answer.

**Decision 6 — the lane's mergeset rule and its fees.** A mergeset holds at most `max_per_mergeset`
round blocks, at most `permits_per_round` of one round, one per `(round, index)`, round blocks of at
most 64 bonds, and a round block merges only rounds older than its own
(`palw_execution_mergeset_rule_v1`). A merging block pays each permitted round block's accepted fees to
the script its coinbase names, aggregated per script in order of first appearance, after the red lump
— never to the merging miner, never through the ADR-0018 carve and never into a validator pool. The
coinbase output cap widens by the 64 payees where the lane is configured. The lane mints nothing.

**Decision 7 — widening is a stage table, and a span keeps the width it opens with.** The lane opens
at `permits_per_round` (1 on the first stage) and `widenings` lists up to nine later stages, each a
height and a wider round, up to 10 (`PALW_EXEC_MAX_PERMITS_PER_ROUND_V1`) — enough to go from 1 BPS to
10 one permit at a time. A span's width is the widest stage in force at the DAA score the span opens
with (`width_of_span`), so the draw, the permit index and the per-round mergeset bound read one number
for a whole span, and history validates at the width it was produced at. Stages only widen, so the
header's per-round bound — the width at the block's own span — bounds every span a mergeset reaches
back to; a template and virtual's parents use the width at their anchor, never wider than what the
block is judged by. Each widening's height is a fence (scheduled, normalised out of the identity with
its width, named to the fork-id gate as `palw_execution_lane_widening_1..9`); its width is reported in
the schedule id.

**Decision 8 — the chain merges the lane, and a node produces it.** Virtual offers round tips as
parents after the chain's own candidates, newest round first, keeping one parent slot for them, taking
a tip only while its anchor is on virtual's selected chain and the mergeset still passes both bounds
(`palw_add_round_parents`). `round_adapt_block_template` turns the mining manager's template into a
round block (parents per Decision 2, recomputed GHOSTDAG, DAA score, bits, median time and pruning
point, the round's timestamp, zero-subsidy coinbase to the payout, empty EVM payload).
`kaspad --palw-round-lane` (with `--palw-producer-key` and `--palw-producer-bond`) asks
`palw_round_view_v1` each second whether the bond holds an unused permit, and if it does builds,
solves, signs and submits the round block.

## 4. What is built

* `consensus/core/src/palw_execution_lane_v1.rs` — rounds, spans, domains, quotas, parities, the
  schedule, the draw, the envelope and its signing message, the mergeset rule, the producer's view;
  integer-only (a test holds it to that).
* `pow_layer0.rs` / `consensus/pow` — algo 10, its constant target, its tag arm, its predicates.
* `config/params.rs` — `PalwExecutionLaneV1 { activation, permits_per_round, max_per_mergeset,
  schedule_span_daa }` at every fence site; `validate_palw_v2` refuses an unrunnable shape.
* GHOSTDAG, windows and DAA, the header processor (gate, envelope, parent shape, anchor rule,
  mergeset rule, selected tip), the body processor (zero subsidy), the PALW state (finals, schedules,
  ledger — rooted and carried only once written, so no state version moves), the virtual processor
  (verdicts, skipped transactions, payouts, sink search, virtual parents, round view and template),
  the consensus API and session, `kaspad`'s round producer.
* §7.2: `PalwExecWideningV1` and the stage table in `config/params.rs` (validation, hashing, fence
  visitor, fence names, fork-id and ruleset-candidate probes); every width reader in the header, the
  verdict, virtual's parents, the round template and the round view.
* §7.3: `PalwExecEquivocationV1` (`palw_execution_lane_v1.rs`), the object and its acceptance and fold
  (`round_equivocations`, delta 48, carriage tail `0xA8`), the verdict's refusal of a burned permit,
  `protocol/flows/src/palw_round_relay.rs` (announce one block a permit, queue the evidence), the
  panel's filing, and the round producer's persist-before-sign record.
* §7.4: `getPalwRoundLane` (op 181; wRPC and gRPC; `PalwExecLaneStatusV1` through the consensus API;
  `nextRoundPermits`), `misaka palw round-lane`, the lane line in `misaka mining status` (the blocks a
  second the schedule carries, beside the width), and one chain-walk log line per chain block that
  merges the lane.
* The compute credit: `palw_execution_credit_v1`, `PalwExecFinalV1::credit`, the fold's
  `work_price_unit` (shared with ADR-0124's work price).
* §7.1: `kaspad --palw-execution-lane-devnet=activation,width,span[,daa:width…]` and
  `scripts/drills/misaka-palw-round-lane-devnet-drill.sh`.
* Tests: the rules module (including the evidence's own proofs); the state (`adr0125_*` in
  `palw_state_v2.rs`: a finalized attempt schedules the next span, a permit is accepted once, spans
  are dropped, the delta and the carriage round-trip, nothing is written below the fence, a permit
  signed twice burns once and slashes the floor, a reorg across a span boundary reverts the schedule
  and the ledger exactly, a `Final` credits the compute it certified capped at the unit); a domain's
  quota follows that compute and not the count of finals; the stage table (`palw_execution_lane_stage_tests`); the pipeline
  (`adr0125_*` in the virtual processor's tests: round blocks hang beside the chain and never move it;
  a chain block naming only round blocks, a permit twice and a forged envelope are refused; a merging
  block grants exactly the permits its parent state schedules; a widening holds for whole spans; two
  real signed round blocks for one permit are evidence the chain accepts once); the coinbase (payouts
  aggregate after the lump and fund no pool); the relay memory; the producer's round record; the CLI's
  lane line; the devnet flag's parser.

## 5. Security amendments

* **SA-1 — the schedule costs work to move.** Its seed and its credits are finalized attempts; a
  fabricated one is a claim the panel and the court void and slash.
* **SA-2 — a permit holder can equivocate, only on its own permit, and it costs the floor.** ML-DSA
  signatures are randomised and the envelope is outside the PoW pre-image, so a holder can publish
  several blocks for one permit; a stranger cannot sign one. One mergeset accepts at most one of them
  and the ledger accepts the permit once. A node announces only the first block it holds for a
  `(round, index, bond)` (`PalwRoundRelayV1`); a second, different signed block is stored, not
  announced, and becomes evidence a funded panel files as `RoundPermitEquivocated` (object tag 46).
  The chain admits it when the named span's schedule grants that permit to that bond and both
  signatures verify under the bond's REGISTERED key; the fold then burns the permit (no merging block
  grants it again) and slashes the registry's collateral floor, once per permit, within the two spans
  the ledger keeps. A producer records each round before signing it, so a restart cannot sign one
  permit twice.
* **SA-3 — the anchor rule is what makes the lane invisible to the chain** (Decision 2). Without it a
  round block could carry a chain block into a mergeset through a block that is never a selected
  parent.
* **SA-4 — the width is a ceiling, and the domains fill it.** A domain holds at most `⌈width / 3⌉`
  permits of a round and only rounds of its parity, and an operator one permit a round, so a round
  holds at most `⌈width / 3⌉` per domain of its parity group and one per operator. At width 10 one
  domain carries at most 2 blocks a second, two carry 4, three 6; ten needs at least six domains that
  the parity split puts three and three, and ten operators in each round. With one live domain every
  other round is empty: the parity rule doing its job. `getPalwRoundLane` answers the permits of this
  round and the next (`nextRoundPermits`), and the operator's lane line leads with the blocks a second
  those two carry — never the width, which is a request and not a rate.
* **SA-5 — a permit is earned per finalized attempt.** Only bonds with a `Final` attempt in the span
  are listed; `N` permits in one round need `N` operators.
* **SA-6 — a round block is never a chain block,** so its UTXO commitment, accepted-id root, overlay
  root and EVM commitment are never read; the template writes zeros and an empty EVM payload.
* **SA-7 — integers only.**
* **SA-8 — the attempt lane's domain is the class unless a free-prompt family certifies it.** The
  attempt lane keeps no per-class family row, so a class certified only by genesis is its own domain.
* **SA-9 — no validator is involved.** Nothing in the lane reads a beacon, a bond view of the DNS
  overlay or a validator pool, and its fees are excluded from the ADR-0018 carve.
* **SA-10 — compute, not frequency, earns the lane.** A class that finalizes more attempts of a
  lighter inference earns no more of the lane than the compute they certified; a class that bears no
  weight is credited at most the unit, so declaring a dear inference buys no lane share, as it buys no
  pay (ADR-0124 Decision 6). A claim under `MaxPerAttempt` is credited the pwu it claimed, which is
  also the exposure it is slashable for.

## 6. What does not change

The attempt lane, its targets, budgets and subsidy; the receipt lane; the heartbeat; fork choice;
every DAG parameter, depth and PALW window; every network until its fence is armed.

## 7. What remains

1. **Built** — the devnet drill (`scripts/drills/misaka-palw-round-lane-devnet-drill.sh`, four nodes: an
   attempt reaches `Final`, the next span is scheduled, round blocks are produced for held permits,
   chain blocks grant them, a granted round block carries fee-paying transactions). A reorg across a
   span boundary is pinned at the state layer (`adr0125_a_reorg_across_a_span_boundary_…`); the drill
   does not partition its network. The run's result is recorded in §9.
2. **Built** — the stage table (Decision 7).
3. **Built** — relay de-duplication and the equivocation slash (SA-2).
4. **Built** — `getPalwRoundLane`, `misaka palw round-lane` and `misaka mining status`'s lane line.
   An explorer reads op 181; misakascan lives in its own repository.
5. **Scheduled** — testnet-11 at `PALW_RC_PALW_UPGRADE_FENCE_DAA` = 6,001 (the operator's height,
   2026-09-17, at DAA ≈5,773): `{ width 1, no widening, max_per_mergeset 600, schedule_span_daa 5 }` (30 when first pinned as
   `4787b92a…`; ADR-0130 shortened the span and named the scheduler rule set the same day, re-pinning to
   `ab4e7b9c…`), one execution block a second, with ADR-0124, ADR-0126, ADR-0128 and ADR-0130 at the same
   height. The first widening is its own height when the operator names it.

## 8. Corrections

* **The first draft seeded rounds from the ADR-0074 beacon.** Main's block production reads no
  beacon; the operator said so. Replaced by finalized attempts.
* **The second draft moved the DAG's parameters** — `ForkedParam` for `ghostdag_k`, the mergeset limit
  and the depths, and every PALW window scaled 120× — so round blocks could be chain blocks. Replaced:
  round blocks are outside the chain, so nothing the chain reads moves.
* **A round block first had to name exactly one chain parent.** Two round blocks of one anchor could
  then never be parent and child — naming the anchor beside a round tip that already hangs from it
  names an ancestor of a parent. A round block may now extend the lane from round parents alone.
* **The schedule first credited every `Final` as one.** A light class that finalized more often
  out-scheduled a heavy one — testnet-11's three classes in one span counted 183 / 367 / 450 with the
  floor at the cap — the imbalance ADR-0124 corrected for pay. A `Final` now credits the compute it
  certified (450 / 450 / 100 for the same span; `a_domains_quota_follows_the_compute_its_finals_certified_not_their_count`).
* **The operator's status line first printed the width as the lane's BPS.** One domain at width 2
  carries half a block a second; the line now reports what the schedule carries.
* **The permit ledger first dropped rounds below the newest merged round.** A holder of a future
  round's permit could then publish early and make every honest round block of the rounds between
  stale. The ledger is keyed by `(span, round)` and kept for two spans instead.

## 9. The drill, run (2026-09-17)

Four nodes on one host (Apple silicon, 12 cores, three builds running beside it), `--devnet
--palw-devnet-floor-only --palw-execution-lane-devnet=0,2,30`: the lane open from genesis at two
permits a round, 30-DAA spans, and one security domain — the floor class — so SA-4 applies: its
permits fall in even rounds only, at most one a round (width 2 caps a domain at one). Binaries were built
from this branch before the compute credit (§8) and before the chain walk named native transactions.

| step | result | evidence |
|---|---|---|
| 0 | the lane armed on the nodes' ruleset | `getPalwRoundLane` `armed: true` at DAA 0, 08:53 JST |
| 1 | attempts reached `Final` in span 3 and span 4 was scheduled from them | DAA 120 at 10:09; the step took 76 minutes at 1.2–2.4 DAA a minute, which is why a step now waits on chain progress (`STALL_WAIT`) and a stalled run can `ATTACH=1` |
| 2 | a node produced a round block for a permit its bond held | `[palw-round-producer] 1 round blocks produced (latest round 41216982)`, 10:09:42 |
| 3 | chain blocks merged round blocks and granted their permits | 10:09:48: 3 round blocks, 3 permits, 4 transactions accepted from them; later merges of 6, 1, 51 and 3 |
| 4 | **partly established** — see below | 16 transactions accepted from permitted round blocks after the first payment (8 at 10:09:59, 7 in the 51-block merge at 10:11:44, 1 at 10:11:50); the recipient's balance went 0 → 3.00000003 BILI (three of five payments applied when read) |
| 5 | two nodes reported one lane | node-1 and node-3: span 4, width 2, one domain |

**What step 4 did not prove.** Round blocks carried fee-paying transactions — the chain walk accepted
them from granted permits and paid their fees to the permits' payouts — but the lane also carries the
nodes' own PALW carriers, and the script accepted the first lane line with any transaction, which was
written two seconds *before* the first payment. The balance moved, but a chain block could have carried
those payments. So the drill proved the lane carries transactions, not that it carried *the*
payments. Fixed the same day: the chain walk's lane line now counts native transactions apart from
carriers and names the first four, and step 4 requires one of the sent transaction ids in a lane line
written after the sends. That run belongs to the release build that carries ADR-0126 (revised),
ADR-0127 and ADR-0128, and its result is appended here when it is taken.

**Run 2 — 2026-09-17, 15:48–16:58 JST, release binaries built from `f4ea3ed2`** (ADR-0124–0128,
ADR-0130 M1/M2 with its two-boundary span delay, the ADR-0131/0132/0133 shadows; before ADR-0134's
deletions and ADR-0135, which the unit and integration suites cover, and before the heights moved —
devnet schedules no fence, so the move does not reach this drill). Same host, four fixture nodes,
`--palw-execution-lane-devnet` at width 2 and 10-DAA spans, `STEP_WAIT` 4 h / `STALL_WAIT` 15 min.

| step | result | evidence |
|---|---|---|
| 0 | the lane armed on the nodes' ruleset | `armed, opens at DAA 0`, 06:48Z |
| 1 | attempts reached `Final` and span 12 was scheduled from them two boundaries later (ADR-0130) | `0.5 BPS scheduled (width 2) · span 12 · … · 2 finals toward span 14`, 16:55 JST; 67 minutes at ≈1.7 DAA a minute (the devnet challenge window is 100 DAA) |
| 2 | a node produced a round block for a permit its bond held | `[palw-round-producer] 1 round blocks produced (latest round 41241332)`, 16:55:32 |
| 3 | chain blocks merged round blocks and granted their permits | 16:56:56: chain block `e617a77e…` merged 43 round blocks, 43 permits granted, 5 transactions accepted from them (0 native) |
| 4 | **established** — a granted round block carried a sent payment | five payments sent (`d7b978c9…`, `e0745580…`, `eb2ee4ea…`, `31fd64aa…`, `b8cddeaf…`); 16:57:19, after the sends: chain block `570284ce…` merged 11 round blocks, 1 transaction accepted, **1 native: `d7b978c9…`**, the first sent id, named by the chain walk's lane line; recipient balance 0 → 400,000,009 sompi |
| 5 | two nodes reported one lane | node-1 and node-3: span 12, width 2, one domain; 16 lane lines across the fleet, 127 round-block production reports |

`PASS` at 07:58:37Z (`scripts/drills/misaka-palw-round-lane-devnet-drill.sh`, exit 0). What the first run
could not show — that the lane carried *the* payments rather than only carriers — is now shown by the
rule §8 added: the lane line counts native transactions apart from carriers and names them, and the
step accepts only a line written after the sends that names a sent id. One observation, not a defect
of the lane: the per-draw storage line (ADR-0112 Decision 8) prints once a draw, and a fixture draw
takes 240 ms, so a node log grew by 12,460 such lines in 50 minutes; on the fleet a draw takes seconds
to minutes and the line is rare. Left as is.

## Mission alignment amendment — 2026-10-07

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。


## Semantic mergeset amendment - 2026-10-07

**Accepted and implemented on `pre`.** This is a classification-layer amendment to ADR-0125, not adoption or withdrawal of ADR-0168's separate canonical-execution proposal. Algo-10 rounds remain raw GHOSTDAG reds, never selected parents, and contribute no additional blue score, blue work or DAA. Permit acceptance and fee payouts keep their existing rules. BASE-0 and heartbeat remain separate under the revised ADR-0165; `rcore/consensus-accounting-v2` remains excluded. No consensus fence, colouring change, emission change, anchor trailer or new rooted accounting is introduced.

Raw `mergeset_reds` mixes execution rounds and genuine GHOSTDAG reds. PALW consumers must separate their meaning through a common header-derived classifier, independently of a round's permit verdict. A refused round remains an execution round.

**Merge depth remains a raw consensus rule:** it still checks rounds as well as ordinary raw reds. Valid extra rounds do not move the score-derived merge-depth root; stale rounds can still violate the existing bound. Pruning, IBD, anticone counting and serialized consensus data retain their current rules.

### Common semantic classifier

`consensus/core/src/palw_mergeset_v1.rs` supplies `ClassifiedMergesetV1` with private partitions and the accessors:

```text
blues()         ordinary raw blues, including the selected parent
rounds()        execution rounds, regardless of permit verdict
genuine_reds() ordinary raw reds, excluding every execution round
```

`classify_palw_mergeset_v1` resolves these partitions through a header-derived round predicate. `try_classify_palw_mergeset_v1` propagates missing-header errors. A round predicate reads algo 10 from immutable headers, or uses the round set already resolved by the existing lane acceptance processor under its configured activation. It must never use an acceptance verdict: a refused round is still a round.

The classifier reads the whole mergeset, preserves the relative input order of each partition, and does not mutate, sort or serialize consensus data. Classification of blues is defensive; valid configured GHOSTDAG data never colours a round blue. Both internal `GhostdagData` and externally exposed `ExternalGhostdagData` provide wrappers over this same core classifier.

### Consumers

* **Round discovery:** header post-PoW validation and round template discovery scan `unordered_mergeset_without_selected_parent()`, identify the round header id, and retain the existing activation, envelope, permit-width and anchor rules. Finding rounds does not mean finding reds.
* **Coinbase:** normal red rewards and the validator pool use `genuine_reds()`. Round fees use `rounds()` and retain their existing full-fee payout aggregation. Ordinary blue outputs, entitled-red/V4 outputs, the red lump, round payout outputs, validator outputs and the inclusion bounty keep their prior ordering and amounts. The round payout order remains first appearance in the raw mergeset. Refused permits still accept no transaction and earn no fee.
* **PALW state inputs:** `PalwTransitionExtrasV1.merged_reds` contains genuine reds only. Its attempt-colour decisions are unchanged because execution rounds never carry an attempt. Entitlement, receipt payout and operator-anchor consumers use classified ordinary members.
* **Lane watch:** the shared classifier supplies round counts separately from merged attempt work. Execution rounds still do not clear the dead-attempt-lane alarm. Missing member headers stop the observation rather than being guessed to represent ordinary reds.
* **Node block view:** accepted rounds keep `laneClass=EXEC`; refused rounds and rounds without a retained verdict use `laneClass=ROUND`. `exec.verdict` and `exec.refusal` carry acceptance separately. `laneClass=RED` is reserved for ordinary GHOSTDAG reds.

Raw mergeset access remains valid inside GHOSTDAG/store code, reachability, pruning/IBD, merge-depth validation, and legacy RPC field conversion. PALW semantic consumers must use the common classifier or a complete, colour-independent discovery iterator. Use `member`/`header` while a member can be a round; use `red` only after genuine-red classification. Tests may inspect raw reds to pin the unchanged representation.

`python3 scripts/ci/check-palw-mergeset-access.py` audits the PALW semantic consumer files for direct raw-red access. Consensus/store and legacy RPC compatibility accesses remain outside this semantic-only check.

### RPC and explorer

`mergeSetBluesHashes` and `mergeSetRedsHashes` retain their existing raw meaning, including rounds in the latter. Verbose block data additionally exposes:

```json
{
  "palwMergeView": {
    "roundBlocks": ["<round block hash>"],
    "genuineRedBlocks": ["<ordinary red hash>"]
  }
}
```

The view is derived from member headers, independent of local permit telemetry. It is absent when any necessary header is unavailable, so clients never mistake an incomplete empty list for a complete classification. JSON clients tolerate the additive field; gRPC uses a new optional message at field 21 and retains all existing field numbers. The existing wRPC binary serializer is unchanged; binary-decoded or older values have no derived view. Clients that need the view use JSON or the updated gRPC schema. Never infer “genuine red” from a raw-red list alone when the derived view is absent.

The explorer shows execution rounds as **E / ROUND**, with `accepted`, `permit refused` or `verdict unknown` separately. An older node's `laneClass=RED` cannot turn an algo-10 block into a RED row. Ordinary chain/blue blocks show **C-BLUE**, while **HEARTBEAT** and **BASE-0** retain distinct lane names. **RED** means a confirmed non-round GHOSTDAG red; off-chain blocks without a classification remain **unclassified**. Block details show the derived round and genuine-red counts, or an explicit unavailable value on older/pruned nodes. No structural E-BLUE display is introduced.

### Validation

Regression coverage uses mixed genuine-red / round fixtures, not round-only assertions:

1. Actual GHOSTDAG still puts rounds in raw reds and never selects one as parent.
2. Classification places rounds exclusively in `rounds()` and ordinary reds exclusively in `genuine_reds()`, preserving order.
3. Mixed coinbases keep normal red rewards and validator-pool amounts separate from round fee payouts, including zero-fee/refused rounds.
4. Increasing valid rounds leaves DAA, blue score/work and the merge-depth root unchanged. The existing merge-depth check still applies to raw members.
5. Normal red payout processing grows only when ordinary reds are added; extra rounds only affect their own fee outputs.
6. RPC round-trips retain raw hashes and semantic partitions, older/binary values remain decodable, and missing headers produce no misleading view.
7. Explorer regression cases keep refused rounds out of the RED filter and keep heartbeat and BASE-0 distinct.
~~~~~~~~
<!-- ADR_SOURCE_END:0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md -->

</details>

<a id="source-0139-the-execution-lanes-gas-is-one-budget-a-round"></a>

<details>
<summary>0139-the-execution-lanes-gas-is-one-budget-a-round.md — 原文 L1–L113</summary>

[原文を開く](../adr/0139-the-execution-lanes-gas-is-one-budget-a-round.md) · SHA-256: `f68c087780cb1de3bb91fd6916e7765407e4e29def84845f0c95d0125217fc94`

<!-- ADR_SOURCE_BEGIN:0139-the-execution-lanes-gas-is-one-budget-a-round.md -->
~~~~~~~~markdown
# ADR-0139 — O13 decided: the execution lane's gas throughput scales with the lane, one budget a round

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`; **built**. Rides
`palw_execution_lane` (testnet-11 DAA 6,001): below the lane the cap is what a chain block always
had; past it the cap is a function of the round blocks the chain block merges. Decides the open
item O13 of `docs/misaka-evm-design-v0.4.md` (§14.3: "per-second G_limit derivation + measured
propagation, frozen before activation").

## 1. The question

ADR-0125 gives transactions a fast lane — one execution round a second, one permit a round on
testnet-11 — "without the PALW chain getting a fast clock". Round blocks reach the chain only
through the chain block that merges them, and the EVM accepts user gas per **chain block**:
`MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK = EVM_GAS_LIMIT = 30 M`, executed under one 30 M env
(`misaka/evm/core/src/env.rs`). With chain blocks every 120 seconds, "1 BPS" of round blocks was 1 BPS
of **scheduling** and 30 M gas per 120 s of **throughput** — 250 k gas a second, whatever the
lane's width. The operator asked which one the lane means.

## 2. Decision

**Both.** A round buys gas: every DISTINCT permitted round among the round blocks a chain block
merges adds `EVM_ROUND_GAS_BUDGET_V1 = 3,000,000` to that chain block's accepted-user-gas cap, on
top of the base, under a ceiling of `EVM_CHAIN_BLOCK_GAS_CEILING_V1 = 30 M + 120 × 3 M = 390 M`
(one anchor's worth of rounds). `evm_user_gas_cap_v1(permitted_rounds)` is the one spelling.

* **Deterministic in consensus data, never in a clock.** The rounds are the `round` fields of the
  permit uses the merging block's own verdicts record (`PalwRoundVerdictsV1::uses`, the same list the
  PALW fold writes); a round index is derived from the round block's header timestamp against the
  genesis, and the round block is consensus data. The node's wall clock enters nowhere.
* **Committed.** The cap is the EVM header's `gas_limit`, so the commitment root covers it: a block
  that claims another cap does not reconstruct, and a block re-executed from its stores (the
  reconstruct driver) runs under the cap it was validated with, read from its committed header.
* **Never below the base.** `user_gas_cap.max(MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK)`: the system
  ops (deposit claims, market settlements) that fit before still fit. The per-transaction bound
  (`gas_limit ≤ MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK`, `tx.rs`) is unchanged.
* **The template computes what validation will compute** (`evm_template_fields`): the distinct
  permitted rounds of the round blocks the template merges, from the tip state's verdicts.
* The base-fee controller keeps `EVM_GAS_LIMIT / 2` as its target: fee dynamics are per chain block
  as before; the lane widens the ceiling, not the target.

## 3. Why 3 M a round — the benchmark

`o13_bench_execution_gas_per_second` (`kaspa-evm`, `--ignored --release`), on the build host
(Apple silicon, in-memory `CacheDB`), 2026-09-18:

| workload | gas | time | gas/s | bytes/gas |
|---|---|---|---|---|
| 1,000 signed EIP-1559 transfers | 21.0 M | 0.121 s | 174 M | 0.0050 |
| 200 calls of 40 fresh SSTOREs (+ deploy) | 181.5 M | 0.030 s | 6.0 G | 0.0002 |

* **Execution** is not the bound: even a host 5× slower with a disk-backed flat state runs 30 M+
  gas a second; 3 M a round is a tenth of that.
* **Propagation** is not the bound: at 0.005 bytes/gas a full round is 15 KB, a full ceiling block
  ~2 MB at the anchor cadence.
* **State growth is the bound.** A fresh account or storage slot costs ~20 k gas and writes ~100
  bytes of state; 3 M gas a second is ≤ 15 KB/s of new state, **~1.3 GB a day at full load** —
  what a testnet host absorbs. The transfer workload (fresh accounts at 21 k gas each) is exactly
  this worst case. Raising the budget is a number, not a rule; the state-growth line is the one to
  re-measure first.

## 3a. What a block at the ceiling actually costs

Sizing the budget from gas/s says the ceiling is affordable. It does not say what one block AT the
ceiling costs, and the ceiling is the number to be wary of, so it was measured directly rather than
extrapolated. `o13_bench_ceiling_block_apply_and_reorg` (`kaspa-evm`, `--ignored --release`), same
host, 2026-09-18: one chain block filled to 389,991,000 gas with 18,571 transfers to fresh addresses
— the cheapest gas per transaction, so the most transactions the ceiling admits, and the worst case
for state growth. n = 30.

| phase | min | p50 | p95 | max |
|---|---|---|---|---|
| execute the 390 M gas | 2,166 ms | 2,205 ms | 3,635 ms | 3,680 ms |
| commit (`state_root` + snapshot) | 13.5 ms | 14.3 ms | 25.4 ms | 28.2 ms |
| **apply (seed + execute + commit)** | **2,180 ms** | **2,219 ms** | **3,653 ms** | **3,701 ms** |
| reseed a CacheDB from the 18,572-account post-state | 1.3 ms | 1.6 ms | 2.5 ms | 6.4 ms |

The p95 and max columns are from a host that was compiling at the same time; the p50 is the quiet
figure. Percentiles over a deterministic workload measure host noise, not tail behaviour of the
rule, so the spread between p50 and max IS the load sensitivity, and it is 1.7×.

* **Against the slot.** The anchor cadence is 120 s. A block at the absolute ceiling takes 1.8 % of
  one slot quiet, 3.1 % under load. A host ten times slower still spends under a third of a slot.
* **Against a reorg.** A reorg re-seeds from the pre-block snapshot and re-executes the replacement:
  about `D × (apply + reseed)` for D blocks. The reseed is 1.6 ms at this state size and is O(N) in
  the whole account set, not in the block — the `state_cost` bench is the one that tracks it as N
  grows. At the 30-block merge depth, thirty CEILING blocks re-execute in ~67 s, inside one slot.
* **Against propagation.** A ceiling block carries 1.97 MB of transactions, 0.0050 bytes/gas.
* **Against the disk.** It leaves 18,572 new accounts and 1.71 MB of new state, 0.0044 bytes/gas.
  One ceiling block every 120 s is **1.23 GB a day**, which is §3's estimate measured rather than
  derived. This remains the binding cost, and it is the line to re-measure before the budget moves.
* **Against memory.** Peak RSS of the whole test process, holding all 18,571 signed transactions and
  the executor at once, was 944 MB. The state itself is small: the post-block snapshot is 1.71 MB.

## 4. Pinned

`the_cap_is_the_base_below_the_lane_and_one_budget_a_permitted_round_under_the_ceiling`
(consensus-core `evm`): base below the lane, base with no merged round, one budget a round, the
ceiling at 120 rounds and beyond, saturating. The executor's own suites (77) and the consensus EVM
suites (43) run under the threaded cap. No fingerprint moves: the cap rides the lane's fence and
the EVM header's commitment.

`o13_bench_ceiling_block_apply_and_reorg` also asserts the two things §3a's numbers rest on: that the
block really filled the ceiling, and that every transfer created a fresh account, so the state-growth
figure is the worst case and not an average.
~~~~~~~~
<!-- ADR_SOURCE_END:0139-the-execution-lanes-gas-is-one-budget-a-round.md -->

</details>

<a id="source-0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor"></a>

<details>
<summary>0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md — 原文 L1–L311</summary>

[原文を開く](../adr/0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md) · SHA-256: `745ca0959b0ece5a6b8e0ab21ad298082e0463fccca8f87cb8c0df011bec043d`

<!-- ADR_SOURCE_BEGIN:0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md -->
~~~~~~~~markdown
# ADR-0168 — An execution block is a third class, and the chain reaches it through an anchor, never a parent

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **RFC8 integration precedence — 2026-10-08:** For the unified EXEC lane, [revised RFC-0008](../rfc/0008-palw-claim-backed-consensus-blocks.md) governs the current direction: `EXEC_TX` and `EXEC_SLICE` share a chain-independent class with zero raw/PALW fork-choice weight, blue score and DAA contribution. §10.3's positive round `safe_weight` pool and `REAL_ROUND`/E-BLUE naming are earlier proposals, not requirements for this lane; root weight is not redistributed to EXEC. Parent/anchor hygiene and bounded lane acceptance remain design references. Earlier branch implementation/status records below do not claim these features are shipped in main or activated.

* Historical Status (superseded for the unified RFC8 EXEC lane by the amendment above): **PROPOSED 2026-10-03, REVISED TWICE (2026-10-03 14:00, user): §10 is the design — a Round coloring class generalising ADR-0105's `LaneColoring`, rounds first-class and transparent to consensus accounting, one weight budget a claim. §§1–9 keep the incident, the parent hygiene, the anchor, the closure, the window and the state rows; where §2 Decision 1 says "weight 0 class" §10 governs.** Target: the next fence after DAA 5,300; dormant on this branch.
  `palw_exec_class_v1`, **dormant on every shipped preset**. See §9 for what is built and what is not.
* Operator's request (2026-10-03): a third canonical class EXEC beside BLUE and RED; consensus parents separate from
  execution-lane heads; acceptance by an anchoring blue block's root inside a window; EXEC never advances the DAA;
  block class and lane health in RPC.
* Builds on: [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
  (the lane, its permits and its round blocks), [0139](0139-the-execution-lanes-gas-is-one-budget-a-round.md),
  [0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md).
* Amends: ADR-0125 Decisions 1, 2 and 8 *past the fence only* (round blocks stop being parents of chain blocks).
  Supersedes nothing.

## 0. The sentence this ADR is

**A round block is canonical and weightless: it is never a parent, never red and never counted; a chain block
accepts the lane by committing, in its own coinbase, the heads it covers and the Merkle root over the blocks they
newly bring, and the blocks outside the two-span window are dropped instead of wedging anybody.**

## 1. The incident

From 2026-10-01 10:12 JST node b0's round producer made 0 round blocks: 10,106 submissions were refused with "block
is violating bounded merge depth" (`~/Downloads/MISAKA-wt-b/lanes/evidence/multi-block-gap-1003.md`). One round block
(`7c0bb084…`, DAA 2,817) had never been merged; `round_adapt_block_template` chose round **parents** from the DAG
with no merge-depth test, every later round block named it, and `check_bounded_merge_depth` refused each of them. A
tip nobody could merge stayed a tip forever, so the wedge was self-perpetuating and survived restarts. The cause is
structural: while round blocks are GHOSTDAG parents, the lane's liveness is hostage to the chain's mergeset rules.
(Lane P2's int-10.7 closes the symptom node-side. This ADR removes the coupling.)

## 2. Decisions

**Decision 1 — EXEC is a class, not a colour.** Past the fence an accepted execution-lane block is EXEC: canonical
(its transactions are accepted when the anchoring block's parent state grants its permit, its fees are paid), never
red, with **fork-choice weight 0 and DAA contribution 0**. It never carries blue work: one real-model claim must not
become 120 units of consensus weight. (A future *bounded* useful-work weight, capped per claim, is a separate fence and
out of scope; nothing here forecloses it.) The class is a function of the header (algo 10 with the lane open) and,
for "accepted", of the chain's anchored set; block queries report `blue | exec | red`.

**Decision 2 — consensus parents are consensus tips only.** Past the fence a block that is not a round block names no
round block as a direct parent (`BadRoundLaneParents`). The chain's GHOSTDAG, mergeset, merge-depth, k-cluster and
DAA windows therefore never see a round block, and a stale lane head cannot reach a template. A round block keeps its
ADR-0125 shape (at most one chain parent — its anchor — and any round parents), so a lane is still a chain of round
blocks hanging from the chain.

**Decision 3 — the anchor trailer.** A chain block that wants the lane accepted ends its coinbase payload (the end of
`extra_data`) with

```
… ‖ version u8 ‖ n_heads u8 ‖ heads[n]·64 ‖ count u32 ‖ root·64 ‖ body_len u16 ‖ "PXA1"
```

(1 ≤ n ≤ 8, heads strictly ascending). The coinbase is inside `hash_merkle_root`, which is in the pre-PoW header, so
the anchor is committed before the grind and nobody can re-wrap a solved block. **No header field moves**: stored
headers, the P2P and RPC header encodings and the block id of every existing block are unchanged — which is why this is
a trailer and not a header field (a new header field would have been a stored-format break that an in-place flag day
cannot carry; the 2026-10-03 analysis is in §7). Magic-terminated bytes are an anchor and a malformed one is refused;
anything else is the miner's.

**Decision 4 — what an anchor covers.** The *closure* of its heads through round-parent edges
(`palw_exec_closure_v1`), stopping at

* a block the parent state already anchored (`exec_anchored`): covered once, never walked past;
* a block **outside the window** — its anchor's span is not this block's span or the one before (the verdict's own
  two spans, the ones the fold keeps schedules and permit ledgers for): **dropped**, and the walk does not go past it;
* a block whose anchor is not on the anchoring block's selected chain: dropped.

A head outside the window therefore contributes nothing and invalidates nothing; the lane **resumes from its latest
anchored checkpoint** (a round block names the checkpoint, not the stale head). Everything else the walk reaches is
covered, in canonical `(round, permit index, hash)` order. `count` is the number covered and `root` is the Merkle root
over their leaves (`H(leaf ‖ hash ‖ round ‖ index)`, odd node promoted, count bound into the root, the empty set has a
root of its own). The anchoring block's validation recomputes the closure from its parent state and refuses a wrong
`count` or `root` (`BadExecutionRoot`, the block is disqualified from the chain like any commitment fault).

**Decision 5 — acceptance.** Where today a merging block's mergeset reds supply the round blocks that
`palw_round_verdicts_v1` judges, past the fence the *covered set* supplies them: appended after the mergeset in
canonical order, judged by the same verdict (permit in the parent state's schedule, bond active and key match, payout
match, permit unused), their transactions accepted in the same loop and their fees paid by the same coinbase rule.
The fold records the covered blocks (`exec_anchored`, state delta 111) and the chain's record of the lane
(`exec_lane`, delta 112). Only a block **on the selected chain** anchors: an anchor in a side branch is judged when
(if) that branch becomes the chain. Reorgs revert the deltas exactly.

**Decision 6 — the clock.** EXEC blocks never advance the DAA score and never carry the tick (ADR-0125 already kept
algo 10 outside the DAA set; this ADR keeps it so). Lane RS's rule — one real-model attempt per clock slot carries
the tick — is unchanged and independent.

**Decision 7 — bounds.** ≤ 8 heads per anchor; ≤ `max_per_mergeset` (the lane's own bound, 600 on testnet-12) covered
blocks per anchor, the walk stopping at the bound (`TooManyLeaves`); the state holds only blocks inside two spans
(`record_exec_anchor` drops older entries); a trailer adds ≤ 650 bytes to a coinbase. A hostile lane costs at most
`max_leaves + 1` visits plus one probe per boundary block it reaches.

**Decision 8 — dependencies.** An anchor names blocks the validating node must hold. At the body stage a block whose
anchor heads lack a body is `MissingParents(heads)` — retryable, never invalid — and the orphan and in-flight
dependency logic treat the heads as parents. The IBD enumeration lists a chain block's covered blocks beside its
mergeset (§5).

## 3. Threat analysis

* **Fork-weight amplification.** The reason for Decision 1. A covered block adds nothing to blue score, blue work or
  the DAA score, and is never in a mergeset, so no number of round blocks moves fork choice. The only weight a claim
  buys is what it bought before (its attempt block). *Residual:* none; a bounded useful-work weight is a new fence.
* **Anchoring withholding.** A producer that wins chain blocks can decline to anchor a rival's lane, or anchor only
  its own. Cost to the lane: a round block's transactions wait for the next anchoring block, and are lost to the
  lane after two spans. Mitigations: any chain block may anchor any lane head in the window, so an honest block
  anchors it a block later; round-block producers are paid only when covered, so withholding is a visible, attributable
  loss (`getPalwRoundLane` reports `rejected_since` and the reason); the user's transactions are also in the mempool
  and are included by ordinary blocks. *Residual:* a 100 % miner can censor the lane, as it can censor anything.
* **DAG bloat / tips.** Round blocks hang beside the chain and are not parents, so they remain DAG tips until pruned
  and a lane head that is abandoned stays a tip. They are excluded from virtual-parent selection and the sink (they
  already were), and `body_tips` is pruned by the pruning point as before. *Residual:* a hostile producer with a permit
  can leave one tip per permit per round; permits are scheduled and bounded (ADR-0125 SA-4).
* **Header size.** Zero: the anchor rides the coinbase. The coinbase grows ≤ 650 bytes per anchoring block; its mass
  counts against the block.
* **Anchor grinding / equivocation.** The anchor is pre-PoW (coinbase in the merkle root), so a producer cannot grind
  it after solving, and a third party cannot swap it. A permit holder who signs two round blocks for one permit is
  still ADR-0125 SA-2's evidence; the covered set holds at most one per `(round, index)` through the permit ledger.
* **Stale-head wedge.** A stale head is *dropped*, never an error: the closure stops at it. The producer builds on the
  lane's anchored checkpoint when its head is stale (`stale` in the health read). The incident of §1 cannot recur by
  this path.
* **Validation cost.** The closure is bounded by Decision 7 and reads headers the node already holds; no state is read
  per leaf beyond `exec_anchored` membership and the permit verdict ADR-0125 already runs.
* **Reorg determinism.** The closure is a function of the anchoring block's coinbase, headers and its selected
  parent's state; the fold writes through deltas that revert exactly (test).
* **Missing data on a syncing node.** See Decision 8 and §5; the failure mode is "wait and fetch", never "disqualify".

## 4. What does not change

Every DAG parameter; the attempt lane, receipt lane and heartbeat; ADR-0125's permits, schedules, equivocation
evidence and fees (the verdict is the same function over a different set); the header, its hash and its stored
shape; every network until its fence is armed. The fence's four places (`Option<ForkActivation>` field,
`for_each_fence`, Some-only writes in `consensus_params_id` and `consensus_schedule_id`, the `never()` collapse),
its reader and `validate_palw_exec_class_v1` (ConsensusV2; `palw_execution_lane` and
`palw_lane_accept_parents_first` open at or below it).

## 5. Pipeline map

| Stage | Change |
|---|---|
| Header, pre-GHOSTDAG | a non-round block past the fence names no round parent |
| Body, isolation | a round block's coinbase carries no trailer; a trailer is well-formed |
| Body, in context | an anchor's heads have bodies (`MissingParents` otherwise) |
| Virtual, chain candidate | closure from the coinbase + parent state; `count`/`root` verified; the covered set joins the verdict, the acceptance loop and the coinbase payouts; fold records it |
| Template | no round parents; the anchor trailer is computed from the lane's heads by the same closure |
| Round producer | extends its anchored checkpoint when its head is stale |
| IBD / orphans | a chain block's covered blocks are listed with it; heads count as dependencies |
| RPC | block class; `getPalwRoundLane` health |

## 6. Spec text

`docs/spec/palw/18-exec-class.md` carries the normative rules (trailer layout, closure, window, root, verdict
inputs, state rows).

## 7. Why not a header field, why not parents

* *A header field* (`execution_anchors`) is the cleanest wire shape and was the first design. It changes the stored
  header encoding (`HeaderWithBlockLevel` is bincode, field-ordered), the P2P and RPC header messages and the
  header-hash preimage, and testnet-12 upgrades in place; the previous header field (`palw_commitment`) arrived with
  a re-genesis. A coinbase trailer has the same commitment properties (pre-PoW, in the block id) with none of that.
* *Round blocks as red parents with an EXEC colour* fixes the weight and the merge-depth refusal but keeps the
  coupling: a lane head is still a candidate parent, still counted in the mergeset bound and still a tip the
  template must choose among.

## 8. Rollout

Dormant everywhere on this branch. Arming is a flag day: the fence height H, with `palw_execution_lane` and
`palw_lane_accept_parents_first` at or below it. Below H nothing changes, byte for byte; at H a template stops naming
round parents and starts anchoring. A node that has not upgraded refuses the first anchoring block (its coinbase is
valid to it, but the round blocks are never merged by it) and diverges in UTXO state at the first accepted lane
transaction, so this is a coordinated fence like every other.

## 9. Status at the time of writing

See the lane report; this section is updated with the final state of the branch.

## 10. The design (2026-10-03 14:00): a Round coloring class, transparent to consensus accounting (option a2)

The operator's design, and a correction of this ADR's first revision: a real-model round block (`REAL_ROUND`, algo 10)
is a **first-class DAG block** — a vertex, gossiped, canonical, shown E-BLUE — that is **transparent to consensus
accounting**. ADR-0105's incident was never "blue real-model blocks"; it was slow bonded attempts landing behind
heartbeats and being coloured RED by `k = 1`. ADR-0105 answered it by making colouring *lane-aware* (`LaneColoring`:
Classic / Weighted / Heartbeat), and ADR-0105 §11 (F1, 2026-09-25) bounded that transparency to candidates hanging from
the merging block's own selected chain. This ADR adds one more class to that mechanism and reuses its code.

### 10.1 The class and what "transparent" means

`LaneColoring::Round`, chosen from the candidate's own header (algo 10 with the lane open **and** this fence active at
the candidate's own DAA score — the key ADR-0105 uses, fixed before any block that merges the candidate is coloured, so
no block's colouring depends on the merging block's own GHOSTDAG output). A Round candidate in a mergeset is **coloured
blue when it is canonical (§10.2) and is otherwise red as any block** — but a blue Round, and the Round's contribution to
every number a *normal* block is judged by, is nil:

| quantity | Round's effect |
|---|---|
| normal blocks' k anticone budget | **none** — a Round is not counted in any non-Round candidate's blue anticone, and does not enlarge its recorded count (ADR-0105's "invisible" rule, and its "a heartbeat must not turn a bonded block red through a third block" rule, applied to the Round) |
| `blue_score` | **+0** — a blue Round is recorded in the mergeset's blues but not counted in the score |
| raw `blue_work` | **+0** — `palw_lane_blue_work_v1` gets an arm returning zero for Round |
| DAA score / windows | **+0** — rounds were already outside the DAA set (ADR-0125) and stay so |
| merge-depth / finality / pruning clocks | **none** — all are blue-score clocks, which a Round does not advance |
| PALW `safe_weight` | **+ its allocated share of `W_claim`** (§10.3), and only that |

So `k` is not changed, no clock is changed, and a Round can neither colour a normal block red nor make a normal block
deeper. What it *is* is a canonical block: its transactions are accepted, its fees are paid, it is blue in the
explorer's sense, and it is a DAG vertex that peers gossip and sync as a block (§10.5).

### 10.2 Canonical iff

A Round is blue in a merging block's colouring iff **all** of:

1. **valid claim** — the claim the permit's bond earned exists in the parent state and is `Final` (ADR-0125 Decision 3);
2. **hangs from the merging block's own selected chain** — ADR-0105 §11's `hangs_from_the_merging_chain`, reused as is
   (the Round's anchor is the merging block's selected parent or a selected-chain ancestor at most `merge_depth` blue score
   below it). A Round hanging from another branch is coloured `Classic`, i.e. red there: this is what closes the
   private-branch borrowing double spend for rounds exactly as F1 closed it for heartbeats;
3. **round index in the permitted range** — the permit's `(span, round, index)` is granted by the schedule (≤ 120 a
   `Final`, ADR-0125 / quanta);
4. **unique** — `(claim_id, round_index)` is not already in the ledger of the merging block's parent state;
5. **budget not exceeded** — the claim's round pool still has a share to give (§10.3);
6. **inside the window** — anchor span within the two spans the fold keeps, and inside the merge-depth floor ADR-0105 §5.1
   derives (a Round whose walk has to pass further back is red, and the stale-lane wedge of §1 is a red tip, not a refused
   parent);
7. **parent and anchor not stale** — via the shared merge-depth predicate of `rcore/int-10-p4` (`bc1c57de9`), the same
   function the template uses to choose parents, so a producer cannot build a Round the validator refuses for staleness.

Items 1, 3, 4 and 5 read the parent state; the colouring walk is a header-stage function with no state. The class is
therefore decided in two steps with one answer: the **structural** part (2, 6, 7: positions in the DAG) is coloured by
GHOSTDAG at the header stage and is what makes a Round "blue" in the DAG's own data; the **state** part (1, 3, 4, 5) is
decided by the merging block's parent state, exactly as ADR-0125 decides a permit today, and a Round that fails it is
merged blue-or-red as the structure says but is credited nothing, its transactions are not accepted, and its fees are not
paid (the ADR-0125 "red without its permit" outcome, unchanged). Blue-by-structure therefore never depends on state a
header-only node lacks, and acceptance never depends on colour alone.

### 10.3 The weight budget

Fork-choice weight is PALW state weight (`safe_weight`, the comparator's keys), so the budget lives in the fold:

* at `Final` a claim is worth `W_claim`, its certified useful work, as today;
* the attempt block keeps `W_attempt = W_claim − R`, and the rounds' pool is `R = ⌊W_claim · ρ / 1000⌋` (`ρ` a fence
  companion value; `ρ = 0` makes rounds weightless and trivially respects the budget);
* each canonical round adds `⌊R / N⌋` to `safe_weight` and records `(claim_id, round_index)`; `N` is the number of
  tickets the claim minted (≤ 120); shares never granted (an unspent ticket, the division's remainder) are never created:
  **`W_attempt + Σ round shares ≤ W_claim` for 120 rounds or one**. (A CCU-proportional split is the alternative; it is
  deferred because it needs each round's compute, which the lane does not record, and the equal split is already a bound.)
* ADR-0152 J-1's per-bond cap applies to the sum.

### 10.4 Why colouring-side transparency, and not (c)

ADR-0105 §4 weighed (c) — choose the selected parent by `blue_work + own work` — and rejected it: it moves every
ordering in consensus that treats blue work as the heavier chain (`find_selected_parent`, the sink search, the
`pick_virtual_parents` assertion, the pruning proof, IBD, the DNS reorg gate), it must keep the mergeset in blue-work
order for the colouring loop, and it cannot be fenced because the selected parent determines the mergeset that the fence's
DAA score is read from. Giving a Round weight by changing *whose work counts in blue work or in the selected parent*
would be (c) again, with a producer-controlled number in it. (a2) takes the other path ADR-0105 took for (d):

* the **colouring** changes (one more class in a function that already has three, keyed on the candidate's own DAA score,
  fenced, with the pruning proof's build and validate carrying it through `with_level` as before);
* the **ordering does not** — raw `blue_work` gains 0 from a Round, so every comparison of blue work in consensus reads
  what it read; `find_selected_parent` and the sink are untouched, and a Round is never a selected parent (it never was);
* **weight lives where fork choice already reads PALW work** (state, `safe_weight`), so it needs no header term the
  header stage could not verify, and it is bounded by a budget in the fold rather than by a clock.

Why that is safe against the three failure shapes the first revision named: (1) a Round does not enter any normal block's
anticone count (the Heartbeat/Weighted rule), so `k = 1` cannot colour an attempt red because of it; (2) blue score and
raw blue work are untouched, so merge depth, finality and pruning are as long as before; (3) a Round is never a selected
parent, so the lane cannot become the chain, and a stale lane is a red Round tip — a verdict about a block, not a
refusal of the next parent.

### 10.5 Lane hygiene and acceptance (what §§2–9 contribute)

* A chain block names no round block as a parent past the fence **only in the sense of §2 Decision 2 when the Round is not
  canonical for it** — under (a2) a canonical Round *is* a parent-reachable vertex, and §2's separation is retained for
  the one thing it is for: **a stale or foreign Round must never reach parent selection**, which is the shared predicate of
  §10.2(7). Which of the two carriers (a Round named as a parent by a chain block inside the merge-depth window, as ADR-0125
  does, or the anchor trailer of §2 Decision 3) the implementation keeps is decided at implementation by the stale-tip
  tests; the trailer is kept in this branch because it already solves dependency and IBD enumeration without a header
  field. *(open item, §10.7)*
* The closure, window, `exec_anchored` ledger and `exec_lane` record of §§2–5 are the **ledger of canonical Rounds**:
  `(claim_id, round_index)` uniqueness (§10.2(4)) is `exec_anchored`'s key space extended with the claim.
* **Clock.** Rounds never advance the DAA and never carry the tick; lane RS's rule (one real attempt or fallback per slot)
  is untouched.

### 10.6 A later fraud verdict

Fork choice is deterministic: **a verdict never changes the weight of a block already folded.** The Rounds stay in
history; the claim's useful-work credit and its reward (attempt and rounds alike) are voided and the producer is slashed
by the ordinary void path. The attacker's gain is the fork weight its claim carried while it stood: at most
`J1_cap × ⌈challenge window ÷ span⌉` claims in flight, each worth at most `W_claim` **in total across attempt and Rounds** —
so Rounds multiply it by 1, not 120. The figures are computed from the shipped params when the fence is built (§10.7).

### 10.7 RPC, status and open items

* **RPC.** `blockKind`: `REAL` for attempts, `REAL_ROUND` for Rounds; `blockClass` stays; the explorer groups
  `REAL_ROUND` under their claim, rendered E-BLUE (canonical).
* **Built (dormant) on `rcore/exec-class`:** parent/anchor hygiene (§2), the closure, window and DoS bounds, the state
  rows at deltas 111/112 and tail `0xF1`, the RPC health and `blockClass`, the drill. **Unbuilt:** `LaneColoring::Round` and
  its `palw_lane_blue_work_v1` arm, `hangs_from_the_merging_chain` reuse for Rounds, the budget fold and `ρ`, the
  `(claim_id, round_index)` ledger row, `blockKind`, and the property tests: weight of a claim with 120 Rounds == with 1 ==
  `W_claim`; reorg determinism; Rounds never outweigh an honest chain beyond the budget; Rounds never move the DAA, the
  blue score or raw blue work; a Round never changes a normal block's colour; the attacker-gain table.
* **Open:** the carrier choice of §10.5; whether ADR-0105 §11's `merge_depth` bound is the right anchor bound for a lane
  that produces a block a second (it was derived for a block every 12 s).
* **Go/no-go for DAA 5,300: no-go.** The design moved after the freeze's inputs; this arms at the next fence.
~~~~~~~~
<!-- ADR_SOURCE_END:0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md -->

</details>

<a id="source-0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission"></a>

<details>
<summary>0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md — 原文 L1–L34</summary>

[原文を開く](../adr/0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md) · SHA-256: `989739e6d91bb40974cdd7c9a4f59e82f699d14f8a2c5d8a2ed388e49cd3e0af`

<!-- ADR_SOURCE_BEGIN:0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md -->
~~~~~~~~markdown
# ADR-0169 — RFC8 uses the existing EXEC lane; the former algo-11 design is retired

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: SUPERSEDED DIRECTION, 2026-10-08. Stable filename retained for existing links.

The former algo-11 design body has been deleted. Its previously reported test results, suite totals and
unrun cases remain in the [v0 test record](../evidence/palw/implementation-records/rfc-0008-v0-test-record.md). Those results describe the
`rfc8/claim-backed-blocks` branch of 2026-10-03/04; they do not establish current main's implementation or
validate the replacement EXEC design. No old fence is activated by this change.

[Revised RFC-0008](../rfc/0008-palw-claim-backed-consensus-blocks.md) and
[EXEC integration spec v1](../design/palw/execution/rfc-0008-implementation-spec.md) govern new RFC8 work:

- REAL anchors an ordinary or long useful-work claim under the current REAL qualification rules.
- EXEC_TX and EXEC_SLICE share the existing chain-independent EXEC class; neither becomes a selected parent,
  contributes raw/PALW fork-choice weight or advances the DAA/clock.
- TxPermit and WorkSlice authorization/accounting are separate. Slice work settles once at root claim Final;
  root-prefix and slice ranges cannot duplicate credit, rewards or execution-schedule credit.
- Current main's heartbeat, BASE-0, 120-second cadence and liveness structure remain the baseline.
- ADR-0173/RFC14 public prosecution, evidence retention and liability apply to every slice/boundary.
  Panel=0 still requires RFC14 and RFC15's separate completion/activation gates.
- New root/slice statements bind [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)'s
  common post-commit policy. Each slice's evidence is fixed before its corresponding future source window;
  a seed exposed at root-open cannot test later freely chosen statements. Carriers never multiply beacon sources.

This is a design/documentation update. The replacement runtime has not been implemented or activated by it.
~~~~~~~~
<!-- ADR_SOURCE_END:0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md -->

</details>

