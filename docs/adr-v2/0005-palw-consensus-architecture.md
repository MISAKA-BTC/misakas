# ADR-v2-0005 — PALW合意構造・実行family・旧PoWからの移行

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 9本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0010](../rfc-v2/0010-dag-consensus-and-native-settlement.md)

## 1. この文書の役割

PoW・credit overlayからPALW合意へ至る構造の判断をまとめ、旧方式の復活を防ぐ。

## 2. 統合した判断の読み方

### 2.1

PALWを付加的なcreditだけでなく合意上のworkへ接続する経緯、非同期Jobとblockの関係、BASEを含むblock生産の構造を区別して読む。[旧ADR-0037](../adr/0037-palw-async-job-state-machine.md) [旧ADR-0038](../adr/0038-palw-is-the-consensus-work.md) [旧ADR-0039](../adr/0039-palw-only-block-production.md)

### 2.2

二つのADR-0026は別文書として保存する。一方はtestnet-11の構成記録、他方はruntimeを分離した検証architectureであり、同じ番号を根拠に上書きしない。[旧ADR-0026（current-palw-testnet11-architecture）](../adr/0026-current-palw-testnet11-architecture.md) [旧ADR-0026（palw-v2-runtime-separated-verification）](../adr/0026-palw-v2-runtime-separated-verification.md)

### 2.3

Family M撤回後の単一family、atomicなruleset有効化、PALW-only／BFT validatorを使わない判断を後続の適用範囲で参照する。[旧ADR-0053](../adr/0053-palw-one-execution-family.md) [旧ADR-0042](../adr/0042-palw-mainnet-candidate-ruleset.md) [旧ADR-0180](../adr/0180-palw-only-consensus-no-bft-validators.md)

## 3. 維持する境界・後続改定

ADR-0007のlayered PoWと過去networkの有効規則を一律に削除済みとはしない。旧hash-floor／credit-only案の撤回は[ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md)で管理し、旧claim・旧networkへの適用を残す。

## 4. 隣接する判断との分担

選択規則は[ADR-v2-0006](0006-fork-choice-participation-and-settlement.md)、livenessは[ADR-v2-0009](0009-heartbeat-floor-and-liveness.md)、既存networkからのactivationは[ADR-v2-0011](0011-release-activation-and-flag-days.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0007-layered-pow.md](../adr/0007-layered-pow.md) | L1–L391 | L12–L13, L261–L261 |
| [0026-current-palw-testnet11-architecture.md](../adr/0026-current-palw-testnet11-architecture.md) | L1–L64 | L15–L15 |
| [0026-palw-v2-runtime-separated-verification.md](../adr/0026-palw-v2-runtime-separated-verification.md) | L1–L379 | L15–L33 |
| [0037-palw-async-job-state-machine.md](../adr/0037-palw-async-job-state-machine.md) | L1–L349 | L15–L19, L94–L101 |
| [0038-palw-is-the-consensus-work.md](../adr/0038-palw-is-the-consensus-work.md) | L1–L806 | L17–L20 |
| [0039-palw-only-block-production.md](../adr/0039-palw-only-block-production.md) | L1–L391 | L15–L20 |
| [0042-palw-mainnet-candidate-ruleset.md](../adr/0042-palw-mainnet-candidate-ruleset.md) | L1–L678 | L15–L16 |
| [0053-palw-one-execution-family.md](../adr/0053-palw-one-execution-family.md) | L1–L323 | L15–L18 |
| [0180-palw-only-consensus-no-bft-validators.md](../adr/0180-palw-only-consensus-no-bft-validators.md) | L1–L26 | L5–L11 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0007-layered-pow"></a>

<details>
<summary>0007-layered-pow.md — 原文 L1–L391</summary>

[原文を開く](../adr/0007-layered-pow.md) · SHA-256: `9c9353c588efeae4e327da3fe04a07de7239b5223ced1dead61a0c0ad812b400`

<!-- ADR_SOURCE_BEGIN:0007-layered-pow.md -->
~~~~~~~~markdown
# ADR-0007: Layered PoW (Layer 0 quantum-resistant finalizer + Layer 1 ASIC-hard tag)

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: Accepted (Phase 1 design freeze; PR-8.1 — PR-8.3 land the foundations). **Still governing;
not contradicted by PALW.**

> **Where PALW sits in this frame (added 2026-08-20).** PALW is a Layer-1 `algo_id` variant
> (`algo_id = 4`), which is the extension point this ADR already specifies — including the rule
> that matters most downstream: **switching is a hard cut-off, two `algo_id` values never coexist
> on one network.** That rule is why
> [ADR-0039](0039-palw-only-block-production.md) can decide "no hash lane on any network" without
> inventing a mixed-algo difficulty relation, and why
> [ADR-0036](0036-palw-mainnet-activation-model.md) Decision 4's hash floor had to be superseded
> rather than layered on. Layer 0 is unchanged on every lineage. Nothing in this ADR is retracted.
Date: 2026-05-28
Supersedes: —
Depends on: [ADR-0001](0001-network-isolation.md), [ADR-0003](0003-lthash-utxo-accumulator.md)

## Context

The upstream Kaspa PoW reduces to a single Keccak-family hash
(`cSHAKE256("ProofOfWorkHash")` and `cSHAKE256("HeavyHash")` —
[crypto/hashes/src/hashers.rs](../../crypto/hashes/src/hashers.rs)),
with the resulting 32-byte digest compared against a 256-bit target
derived from the header's compact `bits` field. Block work is a
192-bit accumulator (`kaspa-math::Uint192`).

That design conflates three concerns that kaspa-pq wants to keep
separable:

1. **The PoW comparison domain.** The size of the integer the miner
   solves against. Upstream uses 256 bits, which is "safe enough"
   under current symmetric-hash assumptions but does not give a
   comfortable post-quantum margin against future Grover-style
   speedups on the underlying hash.
2. **The ASIC-resistance function.** The actual heavy computation
   the miner runs. Upstream uses Keccak; kaspa-pq Phase 1 inherits
   that exactly, and a future hard-fork is expected to swap it for
   an ASIC-hard variant (Argon2d, Argon2id, RandomX-like,
   Cuckoo-like, …).
3. **The work accounting width.** The size of the integer that
   accumulates block work into `BlueWorkType` and feeds GHOSTDAG /
   DNS WorkScore. Upstream's `Uint192` is convenient but does not
   leave headroom for the 512-bit target.

Conflating these makes either side of the swap dangerous: changing
ASIC resistance silently re-derives the work comparison domain,
and any change to the work comparison domain requires re-deriving
the entire ASIC stack. kaspa-pq splits them at the spec level so
each layer can move on its own hard-fork schedule.

## Decision

kaspa-pq PoW is a **two-layer** construction:

```
Layer 1  : L1_tag = AsicHardFn_v{algo_id}(pre_pow_hash, timestamp, bits, nonce)
                    ↓ length-prefixed
Layer 0  : pow_512 = BLAKE2b-512(
                       key   = "kaspa-pq-pow-v1",
                       input = network_id || algo_id ||
                               pre_pow_hash || timestamp || bits || nonce ||
                               len(L1_tag) || L1_tag
                     )
Accept   : Uint512::from_le_bytes(pow_512)
             <= Uint512::from_compact_target_bits_512(bits)
```

Constants:

- `POW_FINALIZER_DOMAIN = b"kaspa-pq-pow-v1"`
- `POW_FINALIZER_BYTES = 64`
- `POW_ALGO_ID_KHEAVYHASH = 1`   (kaspa-pq Phase 1 only)
- `BlueWorkType = Uint576`        (Phase 1)
- DAA internal arithmetic = `Uint640`
- `PowTargetType = Uint512`
- `PowWorkType = Uint512`

### Layer 0 — quantum-resistant finalizer (consensus-critical)

Frozen in Phase 1; never changes without an additional hard-fork
ADR. Properties:

- **Family:** BLAKE2b (HAIFA-like construction). Deliberately
  different from the Keccak family used by Layer 1, so a structural
  weakness in one family does not propagate through both halves of
  the same PoW.
- **Output width:** 64 bytes = 512 bits. Compared against a 512-bit
  target.
- **Keyed:** `b"kaspa-pq-pow-v1"` as the BLAKE2b key. Matches the
  existing `crypto/hashes/src/hashers.rs` BLAKE2b family that
  already uses `.key($domain_sep)` for kaspa-style domain
  separation (e.g. `MuHashFinalizeHash` keyed with
  `b"MuHashFinalize"`).
- **Self-delimiting:** the input embeds `network_id` plus an
  explicit `len(L1_tag)` byte-length prefix so adding a new
  `algo_id` variant cannot collide with a previous variant's
  encoding.

### Layer 1 — ASIC-resistance tag (`algo_id`-driven)

Identified by an 8-bit `algo_id` carried in the header (added in
PR-8.4). Phase 1 ships `algo_id = 1 = POW_ALGO_ID_KHEAVYHASH`, a
direct re-export of the upstream `cSHAKE256("HeavyHash")` 32-byte
tag. **No claim of ASIC-resistance change at Phase 1.**

Future `algo_id` values (`= 2`, `= 3`, …) are introduced by their
own hard-fork ADRs and may choose Argon2d, Argon2id, RandomX-like,
Cuckoo-like, etc. Switching is a **hard cut-off**: two `algo_id`
values are never simultaneously valid at the same `daa_score`, and
mixed-algo difficulty arithmetic is not part of this design.

### Width chain

```
header.bits  (compact 32-bit)
       ↓ from_compact_target_bits_512
target_512  (Uint512)
       ↓ pow finalize / verify
pow_512     (Uint512)
       ↓ work = floor(2^512 / (target_512 + 1))
work_576    (Uint576)
       ↓ DAA window aggregation
daa_acc_640 (Uint640)
```

The work accumulator is one machine word wider than the target so
that a window of 2^64 maximum-work blocks still fits without
overflow (a deliberately impossible upper bound, but cheap to keep
on the safe side).

### Difficulty lift from upstream's 256-bit world

For any historical 256-bit target `target_256` derived from the
upstream `bits` field:

```
target_512 = target_256 << 256
```

Under the ideal uniform-hash model, this preserves block-finding
probability exactly:

```
Pr[X_512 ≤ target_256 << 256]
  = (target_256 << 256) / 2^512
  = target_256 / 2^256
  = Pr[X_256 ≤ target_256]
```

So if a fork-activation rule is ever needed to lift from upstream
Kaspa difficulty to kaspa-pq Phase 1 difficulty, this is the
preserving map.

### `BlueWorkType` width choice

Phase 1 picks `Uint576` as a **safe ceiling**. The minimal-safe
width is governed by the smallest target the consensus rules will
accept. If a future ADR adds `min_target ≥ 2^256` as a consensus
rule, single-block work is bounded above by `2^256` and
`BlueWorkType = Uint384` becomes safe. Until that ADR lands,
`Uint576` is the chosen width and is not optimised.

## Public claim discipline

External messaging about kaspa-pq Phase 1 PoW must be precise to
avoid over-promising ASIC resistance. Templates:

**Acceptable:**

> kaspa-pq introduces a Layered PoW. Phase 1 establishes the
> quantum-resistant PoW domain — a 512-bit target, a BLAKE2b-512
> finalizer, and `Uint512`/`Uint576` work accounting. Layer 1
> remains the upstream kHeavyHash-compatible function
> (`algo_id = 1`), so ASIC resistance is maintained at the current
> level. ASIC-hard Layer 1 variants are scheduled for Phase 2 and
> beyond via separate hard-fork ADRs.

**Unacceptable:**

> Layered PoW makes kaspa-pq ASIC-resistant.

The second sentence is wrong at Phase 1 — it conflates Layer 0 and
Layer 1. Repeat: Phase 1 ships `algo_id = 1` only, and that
function has the same ASIC profile as the upstream Keccak-based
kHeavyHash.

## Consequences

### Positive

- Quantum-resistance and ASIC-hardness become independent
  hard-fork knobs. Phase 1 can ship without committing to a
  specific ASIC-hard algorithm choice.
- Two different hash families on either side of the PoW reduces
  the blast radius of any single-family structural break.
- DNS WorkScore (the PoS-PoW two-axis finality overlay) cleanly
  binds to Layer 0's `BlueWorkType`. The ASIC-resistance dial
  moves under Layer 1 without affecting WorkScore.
- A future `algo_id` switch (e.g. to Argon2id) only re-derives the
  tag → finalizer input section; the rest of the validator,
  RPC, mempool, and storage stack is unchanged.

### Negative

- One extra hash per block (BLAKE2b-512 over a small input). The
  cost is negligible against the cost of the Layer 1 function.
- Block-header width grows by 1 byte (`pow_algo_id`).
- `BlueWorkType` becomes 72 bytes (Uint576) rather than 24 bytes
  (Uint192). RocksDB rows that store work values grow accordingly.
- DAA arithmetic moves from 256/192-bit ladders to 512/576/640-bit
  ladders. The math is straight-line but does need
  `Uint512` / `Uint576` / `Uint640` types added to `kaspa-math`.

### Neutral

- The minimum target floor is left unset in Phase 1. If a future
  ADR adds `min_target ≥ 2^256`, `BlueWorkType` can be tightened
  to `Uint384`.

## Implementation order (Phase 8 PR sequence)

1. **PR-8.1: ADR-0007 (this).** No code, just the design freeze.
2. **PR-8.2: kaspa-math.** Add `Uint512`, `Uint576`, `Uint640`,
   each via the existing `construct_uint!` macro. Add
   `compact_target_bits_512` helper symmetric with the existing
   `compact_target_bits`.
3. **PR-8.3: consensus/core/src/pow_layer0.rs.** Self-contained
   module exposing `POW_FINALIZER_DOMAIN`, `POW_FINALIZER_BYTES`,
   `POW_ALGO_ID_KHEAVYHASH`, `pow_finalizer_blake2b_512`,
   `lift_target_256_to_512`, `calc_work_512`. Unit tests for the
   difficulty-lift identity and the BLAKE2b-512 finalizer
   determinism.
4. **PR-8.4 (deferred): Header field.** Add `pow_algo_id: u8` to
   `Header`. Recompute genesis hashes (4 networks). Consensus-
   breaking — handled in its own PR so the change is reviewable in
   isolation.
5. **PR-8.5 (deferred): BlueWorkType cascade.** Swap the type
   alias `BlueWorkType = Uint192` → `Uint576`. Cascades through
   header serialization, RPC types, GHOSTDAG data, DAA, and
   downstream consumers (~50 files).
6. **PR-8.6 (deferred): Validation wiring.** Connect
   `pow_finalizer_blake2b_512` into the consensus PoW check; route
   the L1 tag through `pow_algo_id`-driven dispatch.

PRs 8.4 – 8.6 are intentionally separate from PR-8.1 – 8.3 so the
Layer 0 design + math + module can land first and the
consensus-breaking changes can be reviewed and rolled out on a
known good base.

## Phase 2 (deferred): `pow_algo_id` wire support (audit H-04)

**Status: planned; NOT required for the Phase-1 single-algo launch.**

### Why it is safe to defer (the Phase-1 posture)

At Phase 1 only `POW_ALGO_ID_KHEAVYHASH = 1` is admitted, and the rule is already
enforced on the **live** path: `validate_header_in_isolation` calls
`check_algo_id_phase1(header.pow_algo_id)` (→ `RuleError::UnknownPowAlgoId`) for
every ordinary and trusted-IBD header, and pruning-proof import enforces the same
rule (`PruningProofUnknownPowAlgoId`). The field is **internal-only**: it is not
carried on the P2P or RPC wire, so every node reconstructs `pow_algo_id = 1` from
`POW_ALGO_ID_KHEAVYHASH` when decoding a header (`protocol/p2p/src/convert/header.rs`,
the `rpc-core` / `rpc-grpc-core` header models) — there is no `new_finalized`-bypassing
borsh/serde header decode on any network receive path — and binds the locally-recomputed
identity hash to it. So there is no wire field through which a peer can inject a deviating
value, and no mixed-`algo_id` regime: **no Phase-1 consensus-split risk** (audit H-04:
validation present + on the live path; the only residual is forward-compatibility, below).

### Phase-2 work (release-blocker for the `algo_id ≥ 2` hard fork)

Introducing an ASIC-hard Layer-1 variant (`algo_id = 2, …`) is a **hard fork** and MUST
land all of the following together, gated on a new `pow_algo_id_phase2_activation_daa_score`:

1. **P2P wire.** Add `uint32 powAlgoId` to `BlockHeader` in `protocol/p2p/proto/p2p.proto`;
   regenerate the protowire. Plumb it through `From<(HeaderFormat,&Header)>` (send) and
   `Header::try_from` (read — drop the hardcoded `POW_ALGO_ID_KHEAVYHASH` default) in
   `protocol/p2p/src/convert/header.rs`.
2. **RPC wire.** Add the field to `RpcBlockHeader` in `rpc/grpc/core/proto/rpc.proto`
   (regenerate) and to the `rpc-core` header models; carry it through
   `rpc/grpc/core/src/convert/header.rs` and the `rpc-core` `TryFrom`s (replace the
   hardcoded default). Keep `submit_block` / block-template round-tripping the real value.
3. **Consensus rule.** Replace `check_algo_id_phase1` with a height-aware check: below
   `pow_algo_id_phase2_activation_daa_score` admit only `1`; at/above admit `{1, 2}`. Apply
   the same rule in pruning-proof validation.
4. **PoW dispatch.** Route `header.pow_algo_id` to the L1 algorithm in `consensus/pow` (the
   `StateLayer0` finalizer + the L1 tag are already `pow_algo_id`-aware; wire the dispatch to
   the new variant's verifier).
5. **No re-genesis required** — the field already exists on `Header` and is in the
   identity-hash preimage with value `1`; only the wire transport and the rule's admitted set
   change. Existing `algo_id = 1` headers stay valid.
6. **Tests.** A header carrying `pow_algo_id = 2` below activation is rejected; at/above
   activation it validates and dispatches to the L1 verifier; a P2P + RPC round-trip preserves
   the field (no longer defaulted).

Until this lands, `pow_algo_id` MUST remain `1` everywhere and the `check_algo_id_phase1`
gate MUST stay enforced.

## Phase 2 — Argon2id (`algo_id = 2`): LANDED, then SUPERSEDED

Phase 2 shipped the memory-hard **Argon2id 16 MiB** Layer-1 (`POW_ALGO_ID_ARGON2ID = 2`,
`pow_layer0::argon2id_l1_tag_v1`) on testnet/mainnet to compress the GPU↔ASIC gap. It worked,
but exposed a structural cost: a memory-hard PoW is **symmetric** — the verifier pays (a large
fraction of) the miner's per-hash cost. Measured on the weakest mesh host, one Argon2id-16 MiB
header verification is ~48 ms (~20 H/s/core), which makes **PoW verification the IBD / catch-up
bottleneck**: a node syncing the header chain spends the majority of its CPU re-running Argon2id.

The `algo_id = 2` code (constant, `argon2id_l1_tag_v1`, the `StateLayer0` dispatch arm) is kept
present-but-dormant so historical pruning proofs spanning the Phase-2 era still verify
(`check_algo_id_known` admits `{1, 2, 3}`); no live network selects it.

## Phase 3 — compute-only BLAKE2b-512 ∥ SHA3-512 (`algo_id = 3`): ACTIVE

To remove the catch-up bottleneck, Phase 3 replaces Argon2id with a **compute-only** Layer-1 tag
(`POW_ALGO_ID_BLAKE2B_SHA3 = 3`, `pow_layer0::blake2b_sha3_l1_tag_v1`):

```text
half_b = BLAKE2b-512(key = "kaspa-pq-l1-blake2b-sha3-v1", netid_len||netid || pre_pow_hash64 || nonce_le)   // 64 B
half_s = SHA3-512(        "kaspa-pq-l1-blake2b-sha3-v1" || netid_len||netid || pre_pow_hash64 || nonce_le)   // 64 B
l1_tag = half_b || half_s                                                                                    // 128 B
```

The 128-byte tag (≤ `POW_L1_TAG_MAX_BYTES = 256`) feeds the **unchanged** Layer-0 BLAKE2b-512
finalizer with `algo_id = 3`, which mixes a second BLAKE2b-512 over the whole preimage — so the
accepted digest depends on every tag byte and a miner cannot skip the SHA3 half. Per-nonce work is
`2×BLAKE2b-512 + 1×SHA3-512` (all compute-only), giving ~10^4× faster verification (~µs/header).

### The trade-off (explicit and accepted)

A compute-only PoW is **not** memory-hard: GPU/FPGA/ASIC acceleration is possible — Phase 3 gives up
the kHeavyHash/Argon2id-era ASIC-resistance goal. This is acceptable in this fork because PoW is no
longer the sole security pillar: the **two-dimensional (PoW × stake) DNS finality overlay**
(ADR-0009) gates reorgs on stake-confirmed anchors, so a pure-PoW majority cannot rewrite confirmed
history. The decision optimizes for sync/verification cost (a real operational pain) over PoW
egalitarianism (already softened by the overlay). The two hash families are concatenated as a
cryptographic hedge: a break in one of BLAKE2b-512 / SHA3-512 still leaves the Layer-0 finalizer's
comparison intact.

### Activation & re-genesis

- Gated on `Params::pow_blake2b_sha3_activation` (renamed from `pow_argon2id_activation`):
  `always()` on testnet/mainnet (`algo_id = 3` from genesis), `never()` on devnet/simnet (stay
  kHeavyHash). `required_algo_id(active)` → `3` when active, else `1`; `check_algo_id` enforces the
  exact id per header DAA score (single-algo invariant, no mixed-`algo_id` DAG).
- **Re-genesis with a clean genesis-hash break.** Genesis is PoW-exempt and still declares
  `algo_id = 1`, so the algo switch *alone* would leave the genesis hash byte-identical and only
  invalidate *post-genesis* Argon2id blocks. That is unsafe: a node restarted **without wiping its
  DB** would silently resume the old Argon2id chain (`factory.rs` reopens an existing consensus with
  `process_genesis=false`) and graft `algo_id = 3` blocks onto an `algo_id = 2` history, splitting
  from freshly-genesised nodes. So the re-genesis bumps each live net's coinbase with a Phase-3
  relaunch marker (`"-bs3"`, mirroring TN11's `11,1`), which **changes the genesis hash** of
  `GENESIS` (mainnet) and `TESTNET_GENESIS` (testnet-10). Devnet/simnet stay kHeavyHash and are
  untouched; `TESTNET11_GENESIS` is vestigial (no active params) and untouched.
- **Startup genesis-mismatch guard.** `Consensus::new` now asserts, when opening an existing DB
  (`process_genesis=false`), that the DB's recorded genesis (`past_pruning_points_store[0]` — written
  at genesis processing, never pruned, the proof anchor) equals the configured `genesis.hash`. With
  the marker bump an un-wiped node's stored genesis ≠ the new configured genesis, so it **refuses to
  start** with a "wipe to re-genesis" message instead of silently resuming the old chain. (This
  mirrors the invariant the pruning processor already asserts at runtime.)
- **Operational re-genesis runbook.** Stop ALL nodes; delete every consensus data dir AND the
  explorer DB; rebuild from this commit; relaunch. The guard makes a forgotten wipe a loud refusal,
  not a silent split.
- **Difficulty**: the inherited genesis `bits` are Argon2id-era and ~10^4× too easy for the fast
  hash; they are kept as the easy launch floor (self-correcting — the DAA ramps difficulty to the
  hash-rate equilibrium `D ≈ aggregate-H/s ÷ BPS` within the first `MIN_DIFFICULTY_WINDOW_SIZE`
  blocks under **un-throttled** mining, and never stalls). Re-bench with `pq-miner --bench-secs`
  (now measures BLAKE2b-SHA3) on launch hardware and pre-set `bits` near equilibrium to skip the
  initial instamine ramp (re-genesises the `hash` constant — recompute via `gen_kaspa_pq_genesis_hashes`).
- The P2P/RPC wire already carries `pow_algo_id` as a generic `u8`/`u32` (Phase-2 wire support
  landed), so `algo_id = 3` round-trips with no transport change.

## References

- [ADR-0001 — Network isolation](0001-network-isolation.md)
  (kaspa-pq is a fresh chain; the difficulty-lift identity is a
  documentation aid, not a migration path from a live mainline).
- [ADR-0003 — LtHash16_1024](0003-lthash-utxo-accumulator.md)
  (the LtHash empty-state finalize uses the same keyed-BLAKE2b
  family that the Layer 0 finalizer uses, modulo output width).
- RFC 7693 (BLAKE2). FIPS 202 (SHA-3 family — alternative
  considered for the Layer 0 finalizer; rejected in favour of
  BLAKE2b-512 for implementation symmetry with the existing
  kaspa-pq hash stack).
~~~~~~~~
<!-- ADR_SOURCE_END:0007-layered-pow.md -->

</details>

<a id="source-0026-current-palw-testnet11-architecture"></a>

<details>
<summary>0026-current-palw-testnet11-architecture.md — 原文 L1–L64</summary>

[原文を開く](../adr/0026-current-palw-testnet11-architecture.md) · SHA-256: `8344d7711ac72da9d4bb4d3f0871528212377803520163086251a93916faa9fa`

<!-- ADR_SOURCE_BEGIN:0026-current-palw-testnet11-architecture.md -->
~~~~~~~~markdown
# ADR-0026: Current PALW architecture on testnet-11

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: Accepted

## Decision

`testnet-11` is the only current public test network. PALW consensus has one
canonical path:

```text
Producer / miner → Claim → Future DAG anchor → Panel selection → Panel verification
```

The only PALW protocol roles are:

| Role | Responsibility |
| --- | --- |
| Producer / miner | Produces the computation claim and its receipt/claim transaction. |
| Panel verifier | Deterministically selected from the future anchor and verifies the claim. |

Peer discovery is an operator-facing concern only. External discovery health
and node-local state must not affect block validity, finality, panel selection,
reward settlement, or chain choice.

## Scope boundary

Only the path shown above is normative. No external signal, node-local state, or
legacy side path may participate in current consensus.

Current panel selection uses the future DAG anchor directly. It must not read
external discovery state, historical committee state, local time, or unordered
map iteration. For identical claim, producer, anchor, runtime class, candidate set,
and panel size, every node must derive the same panel.

Model-cost tables, receipt validation, token-cost accounting, and `rho_micro`
calculation may remain when required by current PALW claim/verification
semantics; they are computation data only.

## Compatibility boundary

There are no compatibility RPCs for retired protocol surfaces, and no current
code may read or mutate their stores. New code must use the PALW names
`future_anchor`, `panel`, `claim`, and `producer`.

## Mission alignment amendment — 2026-10-07

この番号の二つのfilenameは別系統の記録である。旧testnet-11 architectureのPanel/mint/finality記述とV2 runtime-separated設計を現在の実装として混同しない。将来のPALW訴追に関しては本amendmentを優先する。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0026-current-palw-testnet11-architecture.md -->

</details>

<a id="source-0026-palw-v2-runtime-separated-verification"></a>

<details>
<summary>0026-palw-v2-runtime-separated-verification.md — 原文 L1–L379</summary>

[原文を開く](../adr/0026-palw-v2-runtime-separated-verification.md) · SHA-256: `8a700163a20c1a455ccaebe8f738b2a8243f7dae697858be79ba798eafbcddf6`

<!-- ADR_SOURCE_BEGIN:0026-palw-v2-runtime-separated-verification.md -->
~~~~~~~~markdown
# ADR-0026: PALW v2 verification architecture — borrow Ambient's shape, strengthen the proof

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Accepted (architecture). Restored in full 2026-08-26; PROMOTED by ADR-0038 from credit
machinery to L1 machinery.** ADR-0051 walked this ADR's thesis back for one capped family — accepting
a tolerant proof model by name — and [ADR-0053](0053-palw-one-execution-family.md) withdrew that
family, so the exception is gone: take Ambient's architecture, refuse its proof model, build
exact-within-pinned-class, everywhere. The operating envelope as first written — devnet, shadow
mode, consensus-visible zero-credit only — was the envelope of the *credit-overlay* lineage, where
PALW rode beside a hash-produced block. On the V2 lineage
[ADR-0038](0038-palw-is-the-consensus-work.md) inverts the layering and
[ADR-0039](0039-palw-only-block-production.md) removes the hash path, so this ADR's verification
architecture is what *produces* blocks rather than what credits them; its activation is governed by
[ADR-0042](0042-palw-mainnet-candidate-ruleset.md)'s one atomic ruleset, not by the §12 ladder
alone. Every activation gate in the v2 design §12 still stands; this ADR moves none of them.
Date: 2026-08-15
Relates to: ADR-0007 (layered PoW), ADR-0021 (superseded PALW PoW record — kept as history),
ADR-0024 (verified-LLM token-weighted BFT),
[`palw-full-logits-trace-v2-design.md`](../design/palw/execution/palw-full-logits-trace-v2-design.md) (operative
safety model), [`ambient-pol-binary-audit-2026-08-15.md`](../audit/palw/ambient-pol-binary-audit-2026-08-15.md)
(the evidence this ADR rests on), the detailed / VPS-canonical-worker / secure-OTA design set,
`consensus/core/src/palw_v2.rs` (frozen preimages).

## Thesis

**Adopt Ambient's implementation architecture; do not adopt Ambient's proof-security model.** The
runtime/verification separation, the multi-runtime adapter surface, the Merkle-commit →
post-commit-challenge → recompute flow, asynchronous verification and bond/slashing are the right
shape and MISAKA takes them. The parts that make Ambient's *proof* weaker than PALW needs — a
tolerant (RBO / p95-band) comparison, single-token-as-sufficient framing, logits-only-when-final
proof material, LStake-only validator selection, and a runtime-embedded closed verifier — MISAKA
replaces with an exact-within-pinned-class comparison over a deeper trace (logits + activations +
GEMM), a dynamically-sized challenge count, a diversified validator set, and an open kernel.

## Context

### Where the v2 scheme stands

The PALW v2 execution scheme (informally "algo 2"; canonically `PALW_EXECUTION_ALGO_ID_V2`, a
PALW-internal namespace value that is **not** the header `pow_algo_id = 2`) commits one pinned-LLM
execution to a `full_logits_sequence_root`: each decode step's full logits vector
(n_vocab = 248 320, f32) becomes a `job_context_hash`-bound event hash, the ordered events a
domain-separated Merkle root, and that root a keyed outer hash binding job, network, runtime,
class and token budget. Implemented so far, all consensus-inert: the worker's `v2-job` path
(token-ID envelopes, exact decode, fail-closed non-finite handling, build-measured
`RuntimeManifestV2`), golden-vector boot gating, `misaka-palw-agent` Phase A, kaspad
`--compute-endpoint`, and `PalwCapabilityDeclarationV2` (v2 design §16).

The honesty baseline (v2 design §4) is load-bearing everywhere below: a trace root is an **audited
commitment, not a cryptographic proof**. Any claimant can announce an arbitrary root; only
canonical replay by independent bonded verifiers, plus bonds/slashing/challenge-windows/maturity,
makes a false one costly. Acceptance security is economic, and reduces to `P_detect · S > G`.

### Ambient, now measured (not inferred)

The `ambient-xyz/llama.cpp-ambient-bin` `macos-arm64` binary was fetched via Git-LFS and audited
(strings + exports, diffed against upstream llama.cpp; see the companion audit doc). This replaces
the earlier survey's inferences with facts:

- **Verification runs inside the llama.cpp fork.** The Ambient `llama-server` exposes
  `/ambient/v1/inference/verify` and `/…/token-data`, does the recompute, and checks the Merkle
  proof itself. The compute path is source-unreleased but **runtime-embedded** — a verifier must
  run Ambient's build. (Survey hypothesis C confirmed; B refuted for the compute path.)
- **The comparison is tolerant, not exact.** Acceptance is governed by `logit_min_prob` (score
  only logits above a softmax floor), `logit_min_rbo_score` (a Rank-Biased-Overlap ranking-
  similarity score on top logits), `mlp_output_p95_abs_diff` (a p95 absolute-difference *band* on
  MLP-layer outputs), and `num_mlp_failure_threshold` (a bounded per-layer failure allowance).
  Ambient absorbs hardware FP divergence in the *comparison*.
- **Ambient already captures intermediate activations.** `ambient_capture_layers` and the MLP-
  output metric mean the proof material spans logits **and** selected MLP-layer activations.
- **The challenge is teacher-forced.** `ambient_forced_tokens` fixes the token sequence so the
  verifier recomputes a challenged position without sampling drift.
- **Merkle + HDF5 + SGLang-compatible capture.** The proof is a Merkle commitment; captured
  tensors travel as HDF5 with SGLang-compatible export, so the *capture format* is runtime-
  portable even though the *comparison code* is not. On-chain `VerificationState`
  (`merkle_root`, `assigned_verifiers`, `assigned_verifiers_token_ranges`, `verified_tokens`,
  `output_hash`) maps onto the server's per-request `start_token`/`end_token` window and verdict
  counters.

Two facts reframe the earlier ADR draft. First, Ambient and MISAKA give **opposite answers to the
same problem** (hardware FP divergence): Ambient fuzzes the comparison; MISAKA pins the class and
compares exactly. Second, Ambient's tolerant comparison is only safe *because* their verification
is asynchronous and optimistic — off the chain-validity path, where a fuzzy verdict triggers a
probabilistic slash rather than a fork. That coupling — comparison strictness ↔ where the verdict
is consumed — is the hinge of this ADR.

## Decision

### 1. Borrow Ambient's architecture; keep the scheme kernel out of the runtime

Adopt, on evidence: (a) **runtime/verification separation** — vLLM / llama.cpp / TensorRT-LLM /
ROCm behind a Runtime Adapter API, none of them containing consensus logic; (b) a **runtime-
portable capture format** so a vLLM or SGLang miner and a llama.cpp verifier interoperate on the
same captured tensors (Ambient's HDF5/SGLang-export precedent); (c) **Merkle commitment**;
(d) **post-commit random challenge**; (e) **asynchronous verification** off the block-production
path; (f) **bond + slashing**; (g) pinned **model/runtime/tokenizer profiles**.

But where Ambient compiles the verifier into its llama.cpp fork, MISAKA does not. Every domain key
(`misaka-palw/<name>/v2`), preimage and canonical encoding is single-sourced in
`consensus/core/src/palw_v2.rs`, golden-frozen, runtime-independent; runtimes are replaceable
adapters *beneath* the kernel. Ambient's own build is the cautionary case: a consensus-critical
comparison living inside an inference runtime makes "the runtime updated" a consensus event — the
blockchain equivalent of `npm update` toppling the chain. The three layers are frozen:

```
runtime adapters (vLLM / llama.cpp / TensorRT / …)   ← replaceable, no consensus logic
        │  per-decode logits + selected activations + GEMM trace
        ▼
PALW Canonicalizer (CanonicalLogitsV1)               ← §3, deterministic integer form
        ▼
PALW scheme kernel (palw_v2.rs, golden-frozen)       ← single source of preimages/domains
        ▼
node: capability / certificate / challenge / slash   ← consumes verdicts, computes none
```

### 2. Prove deeper than logits — logits + activations + GEMM trace

Ambient treats logits as the computation's fingerprint and (via MLP capture) reaches one layer
deeper. PALW commits deeper still, because the property MISAKA must enforce is not "the claimant
knows the right logits" but "the claimant actually performed the pinned, high-cost computation."
Hashing a final result — `proof = H(Model(x))` — only binds knowledge of the result; the v2 scheme
already moved past this, and this ADR fixes the target as a per-token tuple:

```
T_i = H( Canonicalize(final_logits_i)
       ‖ Project(activation_i @ pinned layers)      // e.g. L8, L16, L24, L32
       ‖ Project(gemm_trace_i @ pinned ops) )
TokenRoot = MerkleRoot(T_0 … T_n)                    // domain-separated, per palw_v2.rs
```

The GEMM-trace leg is the leg Ambient does not have and is where PALW is strictly stronger:
a challenge can address `(token, layer, matrix-tile)`, so predicting final logits alone is
insufficient. Which layers/ops are pinned is a `shape_profile_id`-bound constant; changing it is a
new scheme version. (This is the `full_logits_sequence_root` ambition of the detailed design made
explicit as the challenge surface; the current Land code commits logits events — activations and
GEMM legs are staged additions under the same Merkle discipline, not a new mechanism.)

### 3. Exactness inside a pinned class — never a tolerance in the slashing verdict

This is the deliberate inverse of Ambient's RBO/p95 model, and it is forced by *where PALW's
verdict is consumed*. A fuzzy comparison means `Validator A → pass, Validator B → fail` on the
same input; Ambient survives that because a fuzzy verdict there is a probabilistic slash off the
critical path. PALW's slashing quorum must be **deterministic** — every honest verifier of a class
must reach the identical verdict — or the slash itself forks. Therefore:

- **A hardware/software backend is a determinism class.** Measured: Metal root `ba3b9994…` ≠ CPU
  root `d04672dc…` (same job); F16 x86 agreed 8/8 seeds but arm-vs-x86 only 7/8, the miss an
  argmax flip on a prefill batch-GEMM **near-tie**; the registry Q8_0 artifact split even EPYC vs
  Broadwell (4/8). Divergence concentrates at near-ties — exactly where any ε-band is undecidable,
  and exactly what RBO is built to paper over.
- **Within a class: full 64-byte equality.** Across classes: **no comparison** — a cross-class
  mismatch refutes nothing (same output, different trace is the expected physics). A replay refutes
  only within the declared `runtime_class_id`.
- Divergence is absorbed in **class membership** (exact pinned artifact), not in the comparator.
  Class membership is manifest-hash exact, never a label — the golden-set gate that compared a
  class *label* let a different build pass (`f9ab6ab`), and a class is a claim about *pairs* of
  hosts, so nothing with fewer than two measured sides is a class.
- A tolerant comparator is not banned from the system — it is banned from the **binding verdict**.
  A soft cross-class RBO/p95 signal may run in a *diagnostic, non-slashing* audit tier (anomaly
  detection, class-drift alarms). It never mints, never slashes, never gates a block. Any proposal
  to make ε binding is a new `trace_scheme_id`, a new ADR, and re-activation from zero-credit.

CanonicalLogitsV1 (v2 design §3, §8) is what makes within-class exactness reachable: hash the
canonical little-endian integer/byte form under a pinned FP environment (RNE, no FTZ/DAZ drift, no
fast-math, no FMA contraction) — `hash(canonical_tensor)`, never `hash(raw_float_tensor)`.

> **Amended by ADR-0027 §2:** the class definition is upgraded from pairwise agreement to
> conformance against a canonical reference implementation (soft-float/integer), so disputes are
> adjudicable by any node without class membership. Exactness itself is also re-grounded: under
> the no-BFT premise a tolerance verdict could only be settled by a vote, so exact-within-class is
> forced, not merely preferred.

### 4. Commit → post-commit challenge → recompute → quorum (the flow, made unpredictable)

Adopt Ambient's central move — decide *what to check after the commitment is fixed* — and bind the
selection to future chain randomness so the miner cannot know the audited positions at commit time:

```
C  = MerkleRoot(TokenRoot, activation_root, trace_root)          // published at commit
R  = H( C ‖ future_DAG_block_hash ‖ epoch_randomness )           // unknowable at commit
challenge_set = PRF(R) → { (token, layer, tile) … }              // q positions, §6
verifier: teacher-force the committed token IDs, recompute the challenged positions,
          compare exactly within the verifier's own runtime_class_id
verdict:  deterministic quorum over independent bonded verifiers of that class
```

Teacher-forcing is borrowed (Ambient's `ambient_forced_tokens`) — it removes sampling drift so a
mid-sequence position is deterministically recomputable. **The KV-cache caveat is a hard measurement
requirement, not a footnote:** "verify one token" is not "one token of FLOPs." A fresh verifier
holding no KV cache must prefill the forced prefix to reach a challenged position, so every replay-
cost number in the §10 inequality MUST be measured from a **cold, no-KV verifier**, per class, at
p99 — the "one-token verification is cheap" claim is prohibited (v2 design §15 extends to cover it).

> **Amended by ADR-0027 §1/§4:** challenge-position unpredictability is no longer a security
> dependency. The binding dispute path is a challenger-chosen first-divergence refutation
> (re-execute → name the step → one-step check under canonical reference arithmetic); the
> PRF-positions flow above survives only as an optional coverage sampler that decides nothing and
> never feeds a slash.

### 5. Dynamic challenge count `q`, derived from `P_detect · S > G` — never fixed at 1

Ambient's "verify one token" framing is the single weakest security claim, and PALW must not copy
its cardinality. For a fault touching a fraction `f` of positions, `P_detect = 1 − (1 − f)^q`.
A whole-output fault (`f = 1`) needs one challenge; a 1 %-localized fault (`f = 0.01`) is caught
with probability ≈1 % at `q = 1` and only ≈9.6 % at `q = 10`. So `q` is not a constant: it is sized
per job from bond `S`, expected cheating gain `G`, job value, computation amount and operator
reputation, so that `P_detect · S > G` holds for the *smallest plausible* `f` the scheme intends to
resist. The published §10 economics carry the `q`-sizing rule and its assumed minimum `f`; shipping
a fixed `q` (especially `q = 1`) is a rejected design.

> **Amended by ADR-0027 §3:** under the no-BFT / no-challenge-randomness premises the sampling
> form of `P_detect` is replaced by funded full re-execution (f-independent: one honest replay
> catches any deviation), and `q` changes meaning from "positions sampled" to "independent
> re-executors funded." The inequality keeps its shape; `P_detect` becomes an incentive property
> measured, never assumed.

### 6. Verification is asynchronous; PALW never gates block validity, and never on LStake alone

The BlockDAG must not stall on inference. Block validity stays `valid permanent hash PoW AND (PALW
certificate absent OR valid under its activation stage)` — the permanent hash floor is retained.
PALW runs as an overlay: commit on-DAG, challenge after future randomness resolves, verify,
settle, and slash **retrospectively** (Ambient's asynchronous/optimistic posture, which is
precisely what lets §3's exact verdict live off the critical path). Fork-choice/DNS/finality
weight is reached only through the staged ladder, never at once:

```
Stage 0  ShadowSidecar / zero-credit         (current envelope)
Stage 1  PALW rewards only
Stage 2  PALW → validator/DNS weight, WITH a weight cap
Stage 3  deeper security integration, post external audit + soak
```

And validator selection is **diversified**, not LStake-only. Ambient's LStake (influence ∝ past
verified work) invites a self-reinforcing loop: more PALW → more stake → more likely selected →
verify one's own class → more PALW. MISAKA gates selection on
`VRF ⊕ bond ⊕ independent-identity ⊕ PALW-reputation`, with executor self-verification excluded and
operator-aggregation/concentration caps (v2 design §10). If independent bonded credentials fall
below threshold, the committee is **not** shrunk to mint — PALW credit goes to zero.

### 7. Open kernel; self-originated jobs (no auction); tokenizer outside the runtime

- **Open verification kernel — the inverse of Ambient's binary-only verifier.** Preimage layouts
  (golden-frozen), the comparison rule (§3), challenge selection (§4), `q`-sizing (§5), and the
  bond/slash/maturity/window parameters plus **measured** per-class cold-verifier costs (§10) are
  all published *before* any non-zero credit. This is structural, not a preference: acceptance is
  economic, so `P_detect · S > G` is unauditable if challenge selection, the comparator or `S` are
  hidden — and MISAKA's own algo-5 history shows the failure mode (an output-text commitment that
  looked sound until measurement found a 1-distinct constant, forgeable without model execution;
  publication + measurement killed it in one cycle, a private spec would have preserved it).
  A vendor-supplied closed verifier also permissions the committee to the vendor and makes slashing
  on a closed component's word ungovernable. The domain keys are public keyed-BLAKE2b separations,
  not MAC secrets — obscurity buys this scheme nothing and would only gate verifiers.
- **Self-originated jobs, no auction tier.** PALW seeds are chain-derived (v2 design §9); there is
  no orderer, job market or job fee in scope, so Ambient's entire auction stack (`auction-*`) has
  no MISAKA equivalent and none is planned. This is also why security here does **not** depend on
  user query demand.
- **Tokenizer is a separately-pinned component.** The worker takes token IDs only;
  `tokenizer_id`/`tokenizer_sha256` pin it. A tokenizer drift is an identity change, not a runtime
  patch (Ambient's standalone `tokenizer` repo is convergent precedent).

### 8. Distribution: per-class signed artifact bundle, public source

Adopt Ambient's release *shape* (date-versioned, per-backend, verification-enabled runtime
artifacts — the `llama-2026.07.21.17-…` pattern) as the class unit: one signed bundle per class
containing the bit-identical static worker, its `RuntimeManifestV2`, the full-length golden set,
the GGUF/tokenizer pins and the launch verifier, under the secure-OTA role model (distribution and
activation separate). Unlike Ambient, **no part is binary-only**: full worker/patch source and
`patchset_root` stay public so reproducible-build checking is a public audit path — but bit-identity
of the *distributed* artifact is the *membership* rule (per-host builds silently minted divergent
artifacts once already: the ggml-OpenMP delta, `2825d99`). A class registration MUST reject any
manifest still carrying `"unpinned"` fields.

## Borrow / do not borrow (summary)

| Borrow from Ambient | Do **not** borrow |
|---|---|
| Runtime ↔ verification separation | Verifier compiled into the runtime fork |
| Multi-runtime adapters (vLLM / llama.cpp / …) | Logits-only proof material |
| Runtime-portable capture format (HDF5/SGLang-style) | Tolerant RBO / p95-band comparison in the binding verdict |
| Merkle commitment + post-commit challenge | Fixed 1-token verification |
| Teacher-forced recompute | "one-token verify = one-token FLOPs" costing |
| Random validator/range assignment | LStake-only validator selection |
| Asynchronous / retrospective slashing | PALW result gating block validity |
| Bond + slashing; pinned model/runtime profiles | Security that depends on user query demand |
| Release-artifact shape (per-backend, versioned) | Unspecified hardware tolerance |

## What the audit could not establish

Even with the binary: the exact Merkle-leaf preimage and canonical tensor byte-layout, the RBO
depth/weighting, Ambient's default threshold values (hidden behind format strings), whether cross-
backend verification shares thresholds, and everything in the unreleased Solana programs (challenge
selection, committee sampling, bond/slash amounts, LStake accounting). No binary was decompiled;
only exported symbols and static strings were read. The three x64 tarballs were not downloaded
(the macOS object settles the scheme). These unknowns do not affect any decision above — each rests
on MISAKA's own measurements and requirements and stands even if every remaining guess about
Ambient is wrong.

## Consequences

* **A GPU/vLLM path is class + adapter work, not scheme work.** A new adapter must supply the per-
  decode logits/activation/GEMM surface into the unchanged kernel preimages, FP-environment probes
  around model load, a build-measured manifest with zero `"unpinned"` fields, golden
  generation/self-test, and the envelope identity gate. GPU class granularity (per driver? per GPU
  generation?) is a measurement campaign before any registration; CUDA is not presumed batch-
  invariant.
* **Activation + GEMM legs are the next Land increment** under the existing Merkle discipline in
  `palw_v2.rs` — new leaf kinds and `shape_profile_id` pins, not a new mechanism; consensus stays
  inert until their own gates pass.
  > **Land status (2026-08-15, `consensus/core/src/palw_legs.rs`, consensus-inert):** the
  > activation and checkpoint legs are implemented as a new scheme family
  > (`misaka-palw/execution-commitment/v1`) wrapping the frozen v2 logits root; leg-era contexts
  > keep `trace_scheme_id = v2`, and which commitment form a class produces is a registry fact.
  > **§2's `Project(·)` is amended to exact canonical rows**: under ADR-0027 the legs feed
  > openings and one-step recomputation — a projection is neither an input state nor cheaper to
  > refute, and the hash already is the compression. The v2 fail-closed rule extends to
  > activations (a committed non-finite value is a refutable fault); checkpoint leaves chain
  > from a job-bound genesis, which moves v0.1 §17.2 `M-C4` "broken checkpoint ancestry" into
  > ADR-0027's *objective* column (two adjacent openings that do not chain convict — no
  > recomputation, no jury). Deliberately opaque until registration-time measurement:
  > `tap_semantics_id`, `state_layout_id`, tap-layer values, the interval. The GEMM/tile leg is
  > absent by design (blocked on step-function pinning; adding it is a new scheme version), and
  > worker capture of the legs is the next runtime increment — the schema froze first so capture
  > has one layout to hit.
  >
  > **Capture landed (2026-08-15, `misaka-palw-worker`, still consensus-inert):** the worker taps
  > `l_out-<il>` (post-block residual stream) and commits `llama_state_seq_get_data` checkpoints,
  > streaming both into the schema's own builder so no structural fault is constructible. The two
  > opaque identities are now *declared* (they become network facts only at registration), and
  > the increment's gate was measured rather than assumed: llama.cpp accepts a capture callback
  > only at context creation and it changes ggml's split-compute granularity, so **capture had to
  > be shown logits-neutral** — the legs bind the frozen v2 root, and a capturing executor whose
  > logits moved would disagree with a non-capturing verifier about an honest execution. Measured
  > 4/4 golden jobs unmoved on both the Metal and CPU classes, roots reproducible across runs,
  > 0 of 592 captured rows all-zero. A backend that fails this gate is a distinct determinism
  > class, not a bug to suppress. Evidence and run recipe:
  > `docs/palw-legs-capture-measurement-2026-08-15.md`.
  >
  > **Opening production landed 2026-08-16 (same doc, §6):** commitments are now answerable —
  > `v2-legs-open` re-executes and opens named leaves, refusing any root it cannot reproduce
  > (openings come from re-execution, so the class can answer for its honest members and nobody
  > can open a fraudulent tree), and `check_legs_opening_answer_v1` adjudicates answers
  > model-free. Measured commit → open-in-a-fresh-process → verify on both classes, with both
  > refusals (tampered commitment, tampered answer) held. The challenge *sampling* protocol
  > remains the future ADR above.
* **The challenge/`q`/randomness protocol and the cold-verifier cost model are new design work**
  (future ADR): future-randomness binding point, reorg handling, job-commit deadline, `q`-sizing
  formula with its assumed minimum `f`, and per-class p99 cold-prefill costs — all published before
  the first non-zero credit, as §12 gate items.
  > **ADR-0028 (2026-08-16, Accepted):** binding point and reorg rule (anchor at
  > `daa(C) + Δ_bind`, chain-scoped duties), DAA-denominated windows with measured-p99 sizing —
  > pruning-constrained, which caps the *credited* job size below the format ceiling —
  > `q` re-defined as funded-replay redundancy (its minimum-`f` parameter is gone with the
  > sampling security model), the replay fee as an issuance split `(1 + q·ρ_v)·base`, and the
  > opening seam as the DA-audit transport. Randomness schedules; it never decides — per
  > ADR-0027 P2.
* **Any tolerance proposal is a new scheme version.** There is no ε parameter in the binding path.
* ADR-0021 stays the historical header-PoW record and points here; the v2 design doc stays the
  operative safety model and gate list; the binary-audit doc is the evidence of record. This ADR
  adds no activation and changes no consensus behavior.

## Mission alignment amendment — 2026-10-07

この番号の二つのfilenameは別系統の記録である。旧testnet-11 architectureのPanel/mint/finality記述とV2 runtime-separated設計を現在の実装として混同しない。将来のPALW訴追に関しては本amendmentを優先する。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0026-palw-v2-runtime-separated-verification.md -->

</details>

<a id="source-0037-palw-async-job-state-machine"></a>

<details>
<summary>0037-palw-async-job-state-machine.md — 原文 L1–L349</summary>

[原文を開く](../adr/0037-palw-async-job-state-machine.md) · SHA-256: `54891a7da15767700724c9d98dc0ec8e81c6022c442294aa7a203710b9b5faba`

<!-- ADR_SOURCE_BEGIN:0037-palw-async-job-state-machine.md -->
~~~~~~~~markdown
# ADR-0037: PALW asynchronous job state machine — hash-floor direction withdrawn

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Superseded in part by ADR-0038 (same day).** Decision 1 (hash-PoW as the primary
consensus work; PALW never block-critical) is **reversed** by ADR-0038 — it secured the chain by
making the chain's thesis optional. Decisions 2–9 (state machine, binding, panels, court seating,
classes, mint hygiene, P_check exclusion, registry/freeze) are **carried forward** into ADR-0038
Decision G and remain normative through it. Read ADR-0038 first.

Original status: Accepted (architecture decision). Activates nothing today; binds every future
value-bearing PALW deployment. Adopted from the 2026-08-17 design review that cross-examined the
2026-08-16/17 mainnet-readiness audit (9 blockers, 15 high; the consumer layer, not the arithmetic,
is what fails) against Ambient's published auction/escrow structure.

Date: 2026-08-17
Supersedes: **ADR-0028's mainnet mechanism** (the block-coupled challenge-window walk as the
*mainnet* credit mechanism — the window walk remains the *testnet soak* mechanism and Stage-0→3
instrument). Amends **ADR-0036 Decision 4** only by adding an exit criterion to the TN11/devnet
single-algo exemption (below). Everything else in ADR-0036 (lineage, namespace, "new identity
required", land→accept→mint separation) stands and is assumed here.
Relates to: ADR-0021 (algo-4 PoW — retired from the value path by this ADR), ADR-0026 (Ambient
survey: borrow-architecture/strengthen-proof — this ADR is that borrowing, made precise),
ADR-0027 (no-BFT unilateral fraud proofs — unchanged, becomes Layer 3's admission rule),
ADR-0030–0033 (the arithmetic court and credit gate — unchanged, re-seated as Layer 3 and the
Layer-2 consumer), ADR-0034 (routing — unchanged), ADR-0035 (public testnet — unchanged).

## Context

Three measured facts force this ADR:

1. **The audit's NO-GO is about the consumer layer, not the arithmetic.** The exact-bit court
   (step space → leg → refutation → bisect → one-primitive CPU adjudication, SoftFloat second
   implementation, `Unadjudicable` three-way verdict) survived adversarial review. What failed is
   everything that connects arithmetic results to chain state and mint: fail-open lookups,
   payee-by-pubkey-hash, unbounded coinbase append, history re-scans, and `panic!` on runtime
   absence.

2. **PALW today is block-critical.** `required_algo_id` returns one mandatory id and
   `check_algo_id` rejects every other (`consensus/core/src/pow_layer0.rs`); there is no
   mixed-algo difficulty arithmetic; `calc_block_level_check_pow_layer0` panics on
   `PalwUnavailable` (`consensus/pow/src/lib.rs`). A single inference-runtime fault was a
   chain-liveness fault. The proposed permanent hash floor was later withdrawn by ADR-0039.

3. **PALW evidence is reconstructed, not stored.** `compute_palw_credit_outputs`
   (`consensus/src/pipeline/virtual_processor/processor.rs`) re-walks the selected-parent chain
   across the whole challenge horizon on every decided block, re-discovering commitments,
   attestations and refutations from raw carriage records. That couples PALW timing to block
   cadence (the reason the audit found 10 BPS "structurally impossible"), makes every consumer
   re-derive facts (each re-derivation is a fresh fail-open opportunity), and cannot express
   escrow, deadlines, claims, or dispute state at all.

What Ambient demonstrates and MISAKA adopts is *structural*, not cryptographic: one state object
per job (escrow, fixed payees, deadlines, claim bitmap, selected verifiers), monotone status
transitions, verification asynchronous to finality, bonded disputes with replacement panels.
The exclusions that survive are q-of-n committees as final truth, tolerance-based comparison as
a slash basis, service/admin per-job overrides, and self-declared capacity in the safety argument.
The original exclusion of PALW block production was withdrawn by ADR-0038.

## Decision 1 — Three layers; PALW is never block-critical on a value-bearing network

**Withdrawn / 不採用。** hash PoW を本体、PALW を任意の credit overlay にする案は、有用計算を consensus work にする目的と一致しないため。[ADR-0038](0038-palw-is-the-consensus-work.md) が置き換える。Decision 2 以降の job state・escrow・court 規則は後続 ADR の修正範囲を除き維持する。

## Decision 2 — Jobs are state, not history

The per-block horizon walk of `compute_palw_credit_outputs` is retired for value networks.
Each job is a pruning-surviving consensus state object:

```rust
struct PalwJobStateV3 {
    job_id: Hash32,
    job_context_hash: Hash32,
    model_band_id: Hash32,
    execution_class_id: Hash32,
    requester_outpoint: Outpoint,
    executor_bond_outpoint: Outpoint,
    executor_pubkey: Mldsa87PublicKey,
    commitment_root: Hash32,
    trace_root: Hash32,
    output_root: Hash32,
    commitment_anchor_hash: Hash32,
    eligible_set_snapshot_id: Hash32,
    selected_verifier_bond_outpoints: Vec<Outpoint>,
    status: PalwJobStatus,
    deadlines: PalwDeadlines,
    user_escrow_amount: Sompi,
    max_inflation_credit: Sompi,
    verdict: PalwVerdict,
    reward_claimed_bitmap: u32,
}
```

Transitions are monotone and closed:

```
Open → Committed → PanelSelected → ProvisionalAccepted | ProvisionalRejected
     → ChallengeWindow ─ no dispute → FinalizedAccepted | FinalizedRejected
                       └ dispute   → Disputed → Adjudicating
                                                 → Convicted | NoFaultFound | Unadjudicable
```

Three rules the audit found violated, now structural:

* **A well-formed refutation locks; it never deletes.** A refutation transaction opens dispute
  state and freezes the reward; it is not an erase instruction. (I8)
* **Only exact primitive conviction or an objective no-show/deadline violation can destroy a
  reward or bond.** (I9) Committee disagreement alone cannot.
* **`Unadjudicable` slashes no one** — not the miner, not the panel, not (by default) the
  challenger. It zeroes the job's inflation credit, refunds escrow by the predetermined rule,
  and puts the execution class on the auto-freeze path, because reaching `Unadjudicable` proves
  the class's catalog-completeness claim was false. Re-activation requires a new class version
  and re-audit. Chain liveness is untouched. (I10)

## Decision 3 — Identity and signatures are fully bound, verified at every consumer entry

```
job_id = H("MISAKA/PALW/JOB/V3" || network_id || request_txid || request_output_index
           || requester_nonce || model_band_id)
```

One commitment per `job_id`, first-accepted-wins, enforced in consensus state — `committed_root`
alone is not an identity (it cannot distinguish same-root distinct jobs, duplicated carriage,
replay). Commitment and attestation signatures (ML-DSA-87) bind the full context:

```
commit_message  = H("MISAKA/PALW/COMMIT/V3" || network_id || job_id || job_context_hash
                    || execution_class_id || executor_bond_outpoint
                    || commitment_root || trace_root || output_root)
attest_message  = H("MISAKA/PALW/ATTEST/V3" || network_id || job_id || job_context_hash
                    || execution_class_id || verifier_bond_outpoint
                    || sample_indices || observed_roots || verdict)
```

Signatures are verified at carriage admission **and re-checked (or provably cache-carried) at the
credit-consumer entry**. "Another layer will verify it" is not an accepted design state — that
phrase is what produced the audit's fail-opens. (I5)

## Decision 4 — Panel selection: future anchor, real snapshot, dual deadline

`select_replay_panel_v1` (`consensus/core/src/palw_schedule.rs`) is kept; its *inputs* are fixed.
The caller may no longer hardcode eligibility:

```
panel_seed = H("MISAKA/PALW/PANEL/V3" || network_id || job_id || commitment_root
               || future_anchor_block_hash || eligible_set_snapshot_root)
```

The anchor is a block finalized *after* the commitment; the eligible set is the snapshot at the
anchor; candidates must be `Active` bonds of the exact `execution_class_id`, not frozen, not the
executor, deduplicated by `bond_outpoint` and (best-effort) operator root; `Pending / Unbonding /
Slashed` are excluded. Deadlines are dual, so one saturated mergeset cannot evaporate a replay
window:

```
action_allowed = current_daa ≥ anchor_daa + min_daa_delta
               ∧ past_median_time ≥ anchor_mtp + min_seconds
```

## Decision 5 — Sampled verification is the fast path, never the final ruling

Fast path (adopted from Ambient, role-limited): future randomness picks sample positions; q-of-n
same-class validators recompute; agreement yields `ProvisionalAccepted`. Initial shape
(simulation subject, not a mainnet constant): n=3/q=2 ordinary jobs, n=5/q=3 large jobs. An
attestation carries `job_id`, class id, sample indices, sampled checkpoint roots, observed
output/token roots, verifier bond outpoint, signature — never a bare success count.

Slow path (MISAKA's differentiator, already landed as ADR-0030–0033 machinery): on root
disagreement only, bisect token checkpoint → layer checkpoint → step-leg → kernel invocation →
primitive. The full node's final ruling needs no model and no GPU:

```
verify Merkle proofs → execute one bounded primitive on CPU (vendored SoftFloat / fixed
integer semantics, no host libm) → compare exact bits
```

For every *active* class, catalog coverage of reachable kernels must be 100% — 90% coverage is an
invitation for the remaining 10%. `Unadjudicable`-on-gap is the enforcement (Decision 2).

## Decision 6 — Two-tier hardware taxonomy; classes qualify by calibration, not self-declaration

User-facing discovery uses four bands: `CPU / METAL / CUDA / ROCM`. Consensus uses neither the
band nor "same backend" — it uses an exact environment hash:

```
ExecutionClassId = H(model_band_id || backend_family || weights_root || tokenizer_root
    || runtime_source_commit || runtime_binary_hash || compiler_id || compiler_flags
    || kernel_plan_root || math_profile || libm_build_id || driver_runtime_profile
    || fma_mode || ftz_daz_mode || quantization_profile || shape_profile
    || catalog_root || reference_arithmetic_version)
```

`ModelBandId` (what quality the requester buys) and `ExecutionClassId` (what environment
consensus can adjudicate) are distinct fields and both appear in the execution receipt. Validator
onboarding is `Unregistered → bond → Probation → deterministic calibration jobs → Qualified →
activation delay → Active`; only calibration-passing validators enter panels. **Cross-class
results are telemetry, never a slash basis** (I11) — this generalizes the measured
CPU-class rule ("cross-class refutes an honest receipt") into policy. Exact cross-class
compatibility, if ever demonstrated, arrives as a distinct `CompatibilityGroupId`, not as an
assumption.

## Decision 7 — Mint is a carve of scheduled subsidy, never an append

```
total_coinbase_outputs ≤ scheduled_block_subsidy + transaction_fees          (I6, I15)
palw_reward_block ≤ palw_block_budget      palw_reward_epoch ≤ palw_epoch_budget
hash_reward_block ≥ permanent_hash_reward_floor
```

`compute_palw_credit_outputs` is re-shaped from an unbounded `Vec<TransactionOutput>` producer
into a budgeted, deterministic batch:

```rust
fn compute_palw_credit_outputs(credit_index: &PalwFinalizedCreditIndex, block_budget: Sompi,
    max_outputs: usize, current_daa: u64) -> Result<PalwCreditBatch, PalwCreditError>;

struct PalwCreditBatch { outputs: Vec<TransactionOutput>,
    consumed_credit_ids: Vec<Hash32>, consumed_budget: Sompi }
```

Credit records are consumed in the pinned order `(finalized_daa, job_id)` — prefix-mandatory up
to `max_outputs`/budget, so miners cannot censor or reorder payees without producing an invalid
coinbase. Payees resolve **only** from the exact payout script recorded on
`executor_bond_outpoint` / `verifier_bond_outpoint` (the B14 rule, now universal — never a
`validator_pubkey_hash` lookup). (I3, I4)

Three pools never mix: **user escrow** (paid from ordinary UTXOs; winner + panel + refund + fee ≤
escrow), **PALW inflation credit** (block/epoch budget within scheduled emission), **slash bonds**
(compensation, challenger bounty, burn). One commingled "PALW reward wallet" is how a system
loses the ability to explain its own balances.

## Decision 8 — `P_check` and self-declared capacity are out of the safety argument

Whether a validator really replayed is unobservable in telemetry, so no safety inequality may
contain a telemetry-derived `P_check`. The binding limits are bond-exposure caps, enforced in
consensus state:

```
outstanding_executor_credit ≤ executor_bond × executor_leverage_limit
outstanding_attested_credit ≤ verifier_bond × verifier_leverage_limit
```

plus per-class rate state (`last_credited_daa_by_class`, `credited_amount_this_epoch`,
`active_unfinalized_exposure`) actually checked at credit time. Per-identity limits are fairness
aids only — Sybil-splittable — so **the real valve is the global epoch budget** (Decision 7).
This is the structural fix for the measured `max_leverage` 11,655× violation.

## Decision 9 — On-chain class registry; freeze halts credit, not the chain; no per-job override

`PalwExecutionClassState` (status ∈ `Inactive/Probation/Active/Frozen/Deprecated`, manifest and
artifact roots, committee shape, `activation_epoch`, `freeze_reason`) lives in pruning-surviving
chain state, not compile-time fork parameters. `class_frozen` is consulted on every path: job
admission, commitment, panel selection, attestation admission, provisional finalization, credit
generation. Freeze semantics: new jobs rejected, new credit halted, existing disputes continue,
base chain continues. Governance may freeze a class; **it may not touch an individual job's
verdict or payout** (I13) — Ambient's service-finalize override is explicitly not imported.

Artifacts (weights, tokenizer, template, runtime binary+source, compiler+flags, kernel catalog,
math/driver profile, reference arithmetic) are content-hash-pinned and re-derived from actual
bytes at startup; a manifest mismatch refuses the class. Self-attested flags
(`libm_transcribed: true` style) and CWD-cache shortcuts are banned — the B8 `libm_arithmetic_digest`
and the B15 always-recompute GGUF gate (both landed 2026-08-17) are the pattern.

## Decision 10 — Reconciliation and the 10 BPS question

* ADR-0028's ladder (Stage 0→3) remains the *qualification process*; its block-coupled window
  walk remains the *soak instrument*. Its **mainnet mechanism** is superseded by Decisions 2–9.
* ADR-0036's frame (live lineage governs; new identity required; land→accept→mint; floor binds
  mainnet) is unchanged; its testnet exemption gains the Decision-1 exit criterion.
* Because Layer 2 is asynchronous, `W_challenge`, payout finality, BlockDAG finality, and block
  interval become **independent parameters**. The audit's "10 BPS is structurally impossible"
  conclusion applied to the block-coupled design; it is *re-opened, not resolved*, for this
  architecture. The mainnet-parameter ADR must re-derive all four separately — copying any
  existing preset into the new identity is forbidden.

## Activation stages (value identity)

```
M0 Hash-only            parsing on, classes inactive, PALW cap 0, consensus influence 0
M1 User-escrow only     escrow jobs + committee verification; inflation 0; ordering impact 0
M2 Capped PALW credit   after the full drill list*; tiny epoch cap; hash floor reward intact
M3 Consensus weight     separate ADR; capped; PALW weight can never override the hash chain
```

*M2 preconditions: signature-forgery, duplicate-credit, budget-invariant, pruned-IBD,
reorg-determinism, worker-crash, class-freeze drill, bonded-dispute drill, exact primitive
conviction, `Unadjudicable`-no-slash, third-party bonded operators, external review.
**Initial mainnet stops at M2.** Rewarding useful work and dying when useful work fails are
different properties; the second is an uncompensated liability.

## Invariants (release-blocking; reviews check these, not features)

```
I1  Hash-PoW blocks build and validate with zero PALW
I2  PALW runtime failure never causes panic / fork / block rejection
I3  A job credits at most once
I4  Every payee resolves from an exact bond_outpoint script
I5  Every PALW signature binds network, job, context, class, bond
I6  Payouts never exceed escrow or emission budget
I7  Missing/pruned data is never treated as empty
I8  A well-formed refutation alone never destroys credit
I9  Slash requires exact conviction or objective no-show
I10 Unadjudicable slashes no one
I11 Class mismatch is never a slash basis
I12 Class freeze never halts the hash chain
I13 Governance cannot alter an individual verdict or payout
I14 State root is identical across reorg, IBD, and pruning
I15 Total emission never exceeds the public schedule
```

## Implementation order and current status (2026-08-17)

* **Withdrawn tracks:** PALW を任意 overlay にする Track A と hash floor の導入は、有用計算が consensus work を担う目的に合わないため継続しない（ADR-0038/0039）。worker failure の hardening は別の有効な変更として維持する。

* **Track C — state machine + mint: one change set.** `PalwJobStateV3`, consumer-entry signature
  checks, exact-outpoint payees, `job_id` dedup, future-anchor panels, `job_context_hash`
  binding, on-chain rate state, block/epoch budgets, `PalwCreditBatch`, bonded dispute state,
  freeze state. Partial activation is forbidden — landing B1 without B3 (or B2 without B5)
  preserves a fail-open with better paperwork.
* **Track D — the court (longest path):** step-leg capture → capture neutrality → quant
  GEMV/GEMM, SoftMax, RoPE, GDN catalogs → `ExecutionStepRefutation` carriage → bisection state
  machine → primitive CPU adjudication. Much is landed (ADR-0030–0033); the gate to M2 is 100%
  reachable-kernel coverage per active class.

## What this ADR does not decide

Mainnet parameters (ADR-0036's "does not decide" stands, now including the four decoupled
timing parameters); concrete n/q, bonds, leverage limits, budgets (soak/simulation outputs);
M3. The former hash-floor migration is withdrawn by ADR-0038/0039.

## Summary

job state・escrow・budget・objective court は後続 ADR の修正に従って維持する。hash floor 上の任意 PALW overlay に移す旧方針は継続しない。

## Mission alignment amendment — 2026-10-07

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0037-palw-async-job-state-machine.md -->

</details>

<a id="source-0038-palw-is-the-consensus-work"></a>

<details>
<summary>0038-palw-is-the-consensus-work.md — 原文 L1–L806</summary>

[原文を開く](../adr/0038-palw-is-the-consensus-work.md) · SHA-256: `349530a80ae555c13ba49a9e494eff23011daf593980f6e00f4865dc0ab48ea2`

<!-- ADR_SOURCE_BEGIN:0038-palw-is-the-consensus-work.md -->
~~~~~~~~markdown
# ADR-0038: PALW is the consensus work — sampled verification and a receipt-licensed weight ramp

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Accepted (architecture decision), amended.** Supersedes **ADR-0037 Decision 1** (the
layer inversion below) while carrying forward ADR-0037's machinery (Decisions 2–9) re-seated
under the new layer assignment. Everything else in ADR-0036 (lineage, new-identity requirement,
land→accept→mint separation, no per-job override) stands.

> **Amended 2026-08-17 by ADR-0039** (`0039-palw-only-block-production.md`): invariants **W4 and
> W6 are superseded**, and Decisions B and E are amended, to remove the last two hash paths to
> consensus participation — the anti-stall floor as a block-production path, and `spam_hash_work`
> as a fork-choice weight term. The hash floor is replaced by a portable integer-only
> `PALW-BASE-0` class held permanently Active; total PALW unavailability halts the chain loudly
> rather than producing hash blocks. The layer inversion, sampled verification, the court,
> per-class DAA and bonded producers are unchanged.

Date: 2026-08-17
Relates to: ADR-0021 (algo-4 — its *lottery shape* is kept, its *verification shape* is replaced),
ADR-0026/0027/0028 (court + fraud proofs + sampling — promoted from credit machinery to L1
machinery), ADR-0030–0033 (the arithmetic court — unchanged), ADR-0034 (classes/bands — promoted
to consensus difficulty domains), ADR-0037 (the state machine and mint hygiene — carried,
re-seated), docs/palw-mainnet-readiness-audit (the 9 blockers — dispositioned in §Audit closure).

## Context — why ADR-0037 Decision 1 was wrong

ADR-0037 made hash-PoW the primary consensus work and PALW an asynchronous reward subsystem.
That resolves every liveness blocker — by abandoning the point. If hash is the security resource,
LLM compute is a subsidy program: the chain is exactly as secure with PALW switched off, miners
treat inference as an optional side business, and "useful AI compute as Sybil resistance" is a
marketing sentence, not a mechanism. The project's thesis requires the scarce resource that
orders blocks to BE the inference.

The audit's actual finding was narrower than "PALW cannot be L1". It was: **the current
implementation makes every full node execute the same giant LLM runtime to validate any block,
and panics when that runtime is absent.** The fatal coupling is in *verification*, not in
*production*. The fix is to change who verifies and how — not to demote the work.

What the current `algo_id = 4` already gets right (measured, `consensus/pow/src/lib.rs`):
the lottery shape. `seed = (network_id, pre_pow_hash, timestamp, nonce)`; one deterministic
pinned-LLM inference per nonce; the 200-byte tag feeds the BLAKE2b-512 finalizer; the digest
compares against `bits`. That is already "one canonical inference = one lottery ticket,
header-bound, non-transferable, progress-free". It is kept verbatim. What is replaced is the
verifier: full nodes stop re-running inference, ever.

## Decision A — The layer inversion

```
MISAKA value network
  PALW  = the consensus work (~90–99% of effective weight in normal operation)
  Hash  = header binding, commitments, randomness, tie-break
          (hash production / weight floor withdrawn by ADR-0039)
```

Block production requires:

```
valid_palw_block =
    valid_header
  ∧ spam_hash(header, nonce) < spam_target            (cheap, always CPU-checkable)
  ∧ header carries palw_commitment_root                (trace/output Merkle root)
  ∧ executor references an Active bond of an Active ExecutionClass
  ∧ palw_ticket < class_target                         (the algo-4 lottery, kept)
  ∧ well-formed carriage (sizes, class id, signature length)
```

None of these require any full node to run an LLM. `palw_ticket` is *checked* against the
committed root and header binding; whether the root is an honest inference is decided by
Decision C's sampled verification and court, under Decision B's weight ramp. A full node's
validation surface is: hashes, Merkle roots, signatures, and (in disputes) one bounded CPU
primitive.

**Bonded producers are mandatory.** This is not optional decoration; §New-risk 1 shows the
design is unsound without it. A PALW block names its executor's `bond_outpoint`; conviction
slashes it. Permissionless entry = post a bond, exactly the existing 20k-class bond flow.

## Implementation status — Decision A, 2026-08-18

Decision A's predicate is implemented, tested clause by clause, and **called by the block pipeline
on every block**. It is inert on every shipped network: `Params::palw_block_commitment` is `None`
everywhere, and below that fence the rule is the pre-ADR one (a PALW header's `palw_commitment`
must be empty). testnet-11's consensus fingerprint is unchanged, so a node carrying this peers with
one that does not.

| conjunct | where | state |
|---|---|---|
| `valid_header` | existing pipeline | — |
| `spam_hash < spam_target` | existing Layer-0 PoW | — |
| header carries `palw_commitment_root` | `pow_layer0::check_palw_commitment_shape`, fenced | **done** |
| executor references an Active bond | `PalwBlockCommitmentV1::validate_executor_bond_v1` (W8) | **done** |
| `palw_ticket < class_target` | `palw_pwu::palw_ticket_admits_v1` | **done** (see the correction below) |
| well-formed carriage | `validate_shape` + `validate_against_class_v1` | **done** |
| all six, one call | `kaspa_pow::palw_admission::check_palw_block_admission_v1` | **done** |
| the call | `verify_expected_utxo_state` | **wired** |
| the former V1 class target | V1 class resolver removed; V2 uses its own class-state fold | **retired** |

**Correction to this ADR's own text.** Decision A writes the lottery clause as
`palw_ticket < class_target`. The implementation admits on `<=`, and the ADR text is the outlier:
`palw_pwu::palw_expected_attempts_v1` computes `2^128 / (target + 1)` — it counts `target + 1`
admitting values, and its comment says so ("`target == u128::MAX` is the easiest possible target:
every ticket admits", which is false under `<`) — and the Layer-0 PoW beside it already admits on
`pow_512 <= target_512`. Under `<` the work formula is wrong by `(target+1)/target`, a factor of 2
at `target == 1`, i.e. worst exactly where difficulty is highest. **The clause above should read
`palw_ticket <= class_target`.**

**A framing this ADR does not need.** The class target was expected to require a per-block store.
It does not, and a store would be wrong: `palw_facts::block_pwu_v1`'s doc already recorded that the
only legal source is a fold over the BLOCK's own selected-parent chain, because a store answers
about the reading node's virtual tip and a target that depends on where the tip points is not a
fact about the chain being weighed. The resolver folds from the class's registered `boot_target`;
with no block yet declaring a class, that fold returns `boot_target`, which is the definition rather
than a placeholder.

### What Decision A still needs, in dependency order

1. **A producer that builds the commitment.** `Header::with_palw_commitment` exists and nothing
   calls it. It cannot be the node's template builder: the commitment covers the trace and output
   roots of the WINNING inference, which only the miner has after it wins. `misaminer` already
   drives the PALW lottery (`mine_palw_sequential`), so this belongs there. Two links do not exist,
   and the first is not the one it looks like:

   * **The roots must be OPENABLE — and the v2 projection's are. CORRECTION.**
     `PalwBlockCommitmentV1::trace_root` is documented as "Merkle root of the execution trace
     checkpoints (what samplers open)". This entry previously said no such root existed, on the
     evidence of the v1 worker's `gemm_trace_root` — `keyed64(".../trace-root/v1", all events)`,
     one flat digest with nothing to open — and concluded that **Decision A's producer depends on
     Decision C's proof material**. That conclusion was wrong, and it was wrong because it read the
     layer-0 v1 artifact instead of the v2 projection the worker actually publishes.

     `misaka-palw-worker` sets `full_logits_trace_root` from
     `PalwTraceCommitmentV2::full_logits_sequence_root`, and that is
     `full_logits_trace_root_v2(context, summary, trace_event_merkle_root_v2(ordered_event_hashes))`
     — a domain-separated binary Merkle tree with index-bound leaves and odd nodes promoted
     unchanged. Openable by construction, and the worker holds the leaves.

     One piece genuinely was missing: the tree had no **opening API**. `trace_event_opening_v2` and
     `trace_event_opening_root_v2` now mirror `palw_step_leg`'s construction rather than inventing a
     second convention. The count is bound by the OUTER root, not by the opening verifier — two
     counts can imply the same path for a given index, so the verifier cannot be where that is
     enforced, and the test says so where a reader would otherwise assume it.

     The original warning still holds for the artifact it was about: surfacing `gemm_trace_root`
     into a commitment would mint blocks no sampler can challenge, making every dispute over them
     `Unadjudicable` — rejected but unslashed, the A4 hole in a new place. The producer must take
     the v2 projection's root, not the layer-0 one.
   * **The signature needs bond key material the mining path is not given.** The commitment carries
     an ML-DSA-87 signature over the bond; that key is the validator seed held by
     `kaspa-pq-validator`, while `misaminer` holds only a BIP39-derived payout keypair. Closing
     this is a security-boundary choice — put the validator key on the mining rig, or have the
     sidecar sign on the miner's behalf behind an authorization rule — not a wiring task.
2. **The fence, on a network.** One field. Gated entirely on (1): installed while no producer
   builds commitments, every block fails admission.

### Finality that eroded with nothing but time, and the property that catches it

The two window fixes below both answer "which carriage counts". This one answers "**as of when is a
party accountable**", and it needed no carriage at all.

`receipt_is_authentic_v1` resolved the filer's bond at `pov_daa` — the evaluating node's tip.
`effective_bond_status` is one-way: once `pov` passes a filer's unbond request the bond reads
`Unbonding` forever. So a receipt **stopped being authentic as its filer left**, a matured block
dropped back below quorum, and its pwu left `safe(C)`. Nothing about the chain changed; only the
clock did. And it is steerable — file, let the block mature, then unbond to demote it. The accused
in both conviction arms and the challenger behind a bisect `Open` were resolved the same way and are
fixed with it.

The right moment is the one the party ACTED at: the DAA its carriage was accepted. This is the
asymmetry the credit path already states — *"a refutation accepted after the window still convicts,
but does not revoke credit"* — later facts punish, they do not revoke. The weight path now agrees
with it.

**The property this belongs to, which ADR-0039 asks for before any wiring**: with the carriage
fixed, advancing the point of view must never move a block out of `Final`. `safe(C)` governs IBD and
the deep-reorg bound, so pwu that leaves it is finality handed back. That test now exists, with the
control that keeps it from becoming "bonds are never checked": a filer that had already unbonded
*before* it filed still counts for nothing.

**And the fix was half-applied, in the direction that mattered more.** Resolving the accused's
record at the accepted DAA did not reach the adjudicators' own re-check, which both conviction arms
were still handing `pov_daa`. So the same erosion ran the other way: a landed conviction **voids** a
block's PALW weight, and reading at `pov` meant the void *lifted* once the accused's bond aged out
of `Active`. A block a proof said was wrong regained full weight by nothing but time passing, and
the accused steered it — get convicted, unbond, be un-convicted. Both arms now judge at the DAA the
accusation was accepted, and the property is pinned in both directions: with the carriage fixed,
advancing the point of view moves a block neither out of `Final` nor out of `Voided`.

The lesson is the one the three window arms already taught, restated for a second axis: a rule about
*when* is written once per call site, so it is verified by sweeping every call site at once — never
at the site being edited.

### The third unbounded carriage arm: a late `Open` un-matured a `Final` block

Found by asking the same question of every arm after the receipt asymmetry above. `dispute_is_open_v1`
did not bound the `Open`'s acceptance DAA at all, and `ramp_stage_v1` returns `Provisional` on an open
dispute **before** it looks at the window — correct for a dispute opened inside the window and still
running, catastrophic for one opened after it.

So one bonded `Open` filed against an already-`Final` block demoted it, and `safe(C)` — the weight
that governs IBD and the deep-reorg bound — **lost pwu it had already accumulated**. An accumulated
finality weight that can go down is this ADR's own "mutable-weight forkchoice" critical, reachable
for the price of a single carriage record and available to anyone holding any active bond.

Only the `Open` is bounded. The session's later moves are meant to run past the window — that is
what `prosecution_slack` is for — so the ladder replay is untouched.

**Three arms, one question, three different answers before today.** Convictions were bounded,
receipts were not, and the `Open` was not. Each was written at a different time and each looked
locally reasonable. The lesson worth more than the three fixes: when a rule says "within the window",
every arm that reads carriage needs the bound written into it, and the arms must be checked
together rather than as they are added.

### Decision C's assigned duty, and the window asymmetry found while wiring it

**The defect first, because it is the part that mattered.** The resolver bounded convictions by the
challenge window (W5: a conviction accepted after the close is telemetry, never a weight fact) and
did not bound receipts at all. Late evidence therefore counted **for** a block and never against it.

The consequence is not a fairness complaint. Quorum is a threshold on the receipt count, so a block
that missed quorum inside its window could be **topped up to `Final` afterwards**, at a moment of
the topper-up's choosing — hold receipts back, then raise an old branch's weight when a competing
branch appears. Bounding retroactive weight changes is the entire purpose of a challenge window.
Both arms now share one boundary, `accepted_daa <= accepted + w_challenge`, which is also the DAA
one-off `weight_facts_v1` treats as still-inside. No existing test changed behaviour, which is what
made the asymmetry survivable for as long as it did.

**The duty accounting** (`panel_duty_v1`) is the fold this was found in. It answers who defaulted,
and three of its rules go the opposite way from the quorum count beside it:

* **The duty deadline is not the challenge window.** The schedule already defines one —
  `delta_bind + w_replay`, "attest or refute within this many DAA of the anchor", which
  `PalwScheduleParamsV1::validate` holds strictly shorter than `w_challenge`. Measuring a no-show
  against `w_challenge` would let an assigned seat wait and see what everyone else filed before
  committing. The quorum count deliberately keeps the LONGER window: a late receipt is still
  evidence someone replayed and agreed, so discarding it costs liveness for nothing. One carriage
  row can therefore count toward quorum **and** be a no-show, and the test that pins this asserts
  both on the same row. Carrying `PalwScheduleParamsV1` in the resolver input instead of two loose
  windows is what makes that inequality enforceable rather than remembered.
* **The duty deadline is not the challenge window.** The schedule already defines one —
  `delta_bind + w_replay`, "attest or refute within this many DAA of the anchor", which
  `PalwScheduleParamsV1::validate` holds strictly shorter than `w_challenge`. Measuring a no-show
  against `w_challenge` would let an assigned seat wait and see what everyone else filed before
  committing. The quorum count deliberately keeps the LONGER window: a late receipt is still
  evidence someone replayed and agreed, so discarding it costs liveness for nothing. One carriage
  row can therefore count toward quorum **and** be a no-show, and the test that pins this asserts
  both on the same row. Carrying `PalwScheduleParamsV1` in the resolver input instead of two loose
  windows is what makes that inequality enforceable rather than remembered.
* **Any verdict discharges.** Quorum counts `Match` only, because only agreement licenses. A seat
  that replayed and filed `Mismatch` did its duty and disagreed — reusing the quorum filter would
  make the honest dissent Decision C exists to collect into the offence it punishes.
* **`Pending` is a distinct value, not an empty set.** Mid-window nobody has defaulted yet, and the
  dangerous answer is not a wrong name but a `Closed { no_shows: [] }` a caller reads as "nobody
  did" — or a full one it reads as "everybody did", which turns a slow network into a mass slash.
* **An inactive bond is not excused.** A slashed seat collects a no-show on top of its slash. That
  is a real cost, accepted because the alternative is worse: excusing inactive bonds makes unbonding
  an exit from assigned duty, so a seat that dislikes what it is about to find can withdraw instead
  of filing. Double-punishing is unfair; a purchasable exemption is unsound.

The consequence is deliberately not attached. This computes who defaulted; what it costs is a
slash-path decision, and separating them is what let the accounting be tested against a case no live
slash path exists to exercise — including the node that cannot verify signatures, which correctly
reports the *entire panel* in default and must never act on it.

### Decision C's freeze clause — implemented, and the block was not where it looked

I10 — "`Unadjudicable` slashes NOBODY and freezes the class" — is implemented. It was recorded here
an hour earlier as blocked on chain-point-scoped class state; that was wrong, and the correction is
worth keeping because the same mistake is available for every other piece of Decision C.

Two things were missing. The first made the second look impossible.

* **The verdict was discarded at the carriage boundary.**
  `adjudicate_step_conviction_carriage_v1` flattened `PalwStepRefuteError::Unadjudicable` into
  `StepConvictionNotProven(String)`, so a fact about the accused CLASS's coverage was
  indistinguishable from a fact about the challenger's evidence — both an `Err` the live path
  skipped. `PalwCarriageError::StepUnadjudicable` is now its own variant.
* **The freeze does not need a store, and must not have one.** `class_is_frozen_v1` folds over the
  carriage on the chain being evaluated. The class-state store's module records why a row is
  dangerous — it answers about the reading node's virtual tip, so two nodes with different sink
  histories disagree about a coinbase — and its rule that "a seed writer must arrive together with
  per-chain-point scoping, never before it" is satisfied by having no row at all. The moves ARE
  carriage on the chain, so the walk is chain-scoped by construction. Same shape as
  `dispute_is_open_v1` and as the class target above.

The decision is `outcome_freezes_class_v1`, tested exhaustively over outcomes: a landed conviction
slashes and does not freeze, every other failure is the challenger's problem, and only a coverage
gap freezes.

**Reaching `Unadjudicable` end to end is now covered, and covering it found the rule stopping one
call short of doing anything.** `palw_step_refute` already built such a refutation for its own
`unknown_kernel_is_unadjudicable`; it is exposed as `unadjudicable_refutation` and carried into a
conviction. With it, `resolve_block_facts_v1` was filling the field named
`dispute_open_or_unadjudicable` with the dispute half ALONE — so a block with a full receipt quorum
and a live coverage gap against it matured to `Final` and entered `safe(C)`. A block nothing can be
held to, counted as finality. I10 had been implemented as far as the function that computes it.

Both terms are bounded by the block's own challenge window, so adding the second cannot demote a
matured block: `Final` requires `pov` past the window close, and a freeze record must be accepted at
or before it. The same fixture also un-vacuums the reorg suite's "unadjudicable conviction" arm,
which until now held a carriage that fails on its opening path — the shape of a conviction, and none
of the rule.

**And wiring it exposed a second gap the same fixture proved: the freeze was not scoped to a
class.** `class_frozen_before_close_v1` froze on any `Unadjudicable` conviction on the chain,
whatever class's execution it refuted. A coverage gap is a fact about ONE catalog, and manufacturing
one is free — `Unadjudicable` slashes nobody — so an attacker registers a class with an uncatalogued
kernel, refutes its own execution in it, and every block on the network stops maturing, at the price
of one carriage record with no bond at risk. `runtime_class_id` and `execution_class_id` are one
namespace (`PalwClassRegistrationV1` maps the first onto the second), so the comparison is direct.

The fixture is what proved it: `unadjudicable_refutation`'s job context names a different class from
the resolver fixtures' default, so the first version of the test froze a block of class `0xC1` with
a gap in class `6` — and passed. The scenario now runs under the refuted class, receipts and all,
with a gap-in-another-class case beside it.

**The lesson for the rest of Decision C**: "this needs chain state" has meant "this needs a store"
twice in this ADR's implementation, and both times the honest answer was a fold over the block's own
chain. Reach for the store only when a fold provably cannot answer.

**And the freeze had the same unbounded shape the three carriage arms did**, found by running
ADR-0039's finished suite against it rather than by reading it again. `class_is_frozen_v1` walked
every conviction the point of view had reached, with no window bound — so once wired it would have
been a *broader* retroactive demotion weapon than the late-`Open` fixed as exactly that: a freeze is
a fact about the CLASS, so one coverage gap surfacing at any later DAA pulls every matured block of
that class back to `Provisional` at once. That is ADR-0039 §3e's "a conviction can never rewrite
safe weight" rewritten wholesale.

It is now `class_frozen_before_close_v1`, bounded by the same window every other arm carries, with
the bound in the NAME because it must be read before the function is wired — it is per-block and it
is not "is the class frozen right now". The scope rule is split out as `freeze_record_is_in_scope_v1`
and tested exhaustively, for the same reason `outcome_freezes_class_v1` was split out: no fixture
here reaches `Unadjudicable` end to end, so a bound left inline would be an untested bound.

What the bound leans on is what the conviction path already leans on — ADR-0039's residual
assumption 1, an honest party reachable within every challenge window. A gap surfacing inside the
window pins those blocks at `Provisional` forever; one surfacing afterwards stops the class going
FORWARD, which is the store-backed view's job on the panel and mint paths, fail-closed.

Decisions B, C and D remain unimplemented beyond the pure arithmetic already in
`palw_weight`, `palw_facts` and `palw_class_daa`. Decision C in particular is the panel assignment,
receipt collection and challenge-window machinery, which is the bulk of the remaining work.

## Decision B — Acceptance now, weight later: the ramp

Block acceptance and PALW finality are split. An admitted block enters the DAG immediately;
its *weight* matures:

```
weight(B) = pwu(B) × ramp(B)
            (hash-work addition withdrawn by ADR-0039 W4′)

ramp(B) = 0        at admission                     (provisional)
        = ρ_r      once ≥ k assigned receipts land  (receipt-licensed; ρ_r ≈ 1)
        = 1        at PALW-final
        = 0        forever, if convicted before PALW-final (retroactive void)

PALW-final(B) ⟺ challenge window W_challenge passed with no surviving refutation
```

* After PALW-final, weight is immutable — conviction can no longer void it (finality means
  finality; the window is sized so that cannot happen against a live watcher, §Assumptions).
* Fork choice (GHOSTDAG blue work) reads `weight(·)`; reorg semantics are unchanged, only the
  work scalar matures over a bounded horizon. Weight mutation is **monotone within a branch
  except the single conviction edge**, and every mutation is recomputable from DAG data alone —
  two nodes with the same DAG compute the same weights (ADR-0037 I14 carried).
* **DAG-native receipts:** verification receipts are ordinary carriage riding *successor*
  blocks (`verify(B, sample #i)` in C, D, …), so verification is chain activity, receipt
  inclusion is fee-earning, and the ramp is computable from the DAG with no side channel.

The honest cost, stated: full-weight finality lags by `W_challenge`. `W_challenge`,
BlockDAG finality, payout finality and block interval are four independent parameters
(ADR-0037 Decision 10 carried); 10 BPS constrains none of them structurally, because
verification is off the admission path.

## Decision C — Verification: assigned sampling is the alarm, the court is the truth

* **Assigned, mandatory sampling.** The ADR-0028 lottery (`select_replay_panel_v1`, future
  anchor, ADR-0037 Decision 4 inputs) assigns a per-block panel from the block's class.
  Assigned duty is objective: attest-or-no-show, no-show is an offense (bond-relevant).
  Panels sample positions derived from post-anchor randomness (layer/token/GEMM coordinates),
  recompute, and file bonded receipts. **Receipts are claims, not truth** — an attester who
  receipts a sample the court later convicts is convictable on their own signed roots
  (rubber-stamping is a bonded bet, this is the P_check answer carried from ADR-0037
  Decision 8).
* **Permissionless refutation.** Anyone bonded may file a refutation regardless of panel
  membership (ADR-0027 unchanged). The security statement of the fast path is 1-of-N honest
  *watcher*, not q-of-n honest *committee*: quorum accelerates the ramp; it never becomes
  final truth.
* **The court is unchanged** (ADR-0030–0033): checkpoint → layer → step-leg → kernel →
  primitive; the full node re-executes ONE catalogued primitive on CPU (vendored
  SoftFloat / fixed integer semantics) against Merkle-proven operands. GPU-less VPS
  adjudication, exact bits, three verdicts. `Unadjudicable` semantics (ADR-0037 Decision 2:
  no slash, class freeze) now apply to *block work*: the block's pwu voids (ramp → 0 if not
  yet final), nobody is slashed, the class freezes.

## Decision D — Multi-class difficulty: per-class DAA, static PWU only inside a class

The class taxonomy (ADR-0034) is promoted from a routing concern to a **consensus difficulty
domain**, and difficulty is measured in PALW Work Units:

* **Intra-class:** `pwu(class)` is a static, canonical work score derived from normative
  FLOPs, memory traffic, token count, context length and quantization class — never measured
  wall-clock (an RTX 5090 second and a 2019-GPU second are different amounts of nothing).
  The derivation is registered with the class and frozen.
* **Inter-class:** class ratios are NOT hand-priced. Every hand-set cross-class coefficient
  is a standing arbitrage: the mispriced-cheap class absorbs all miners and the multi-class
  resilience this ADR buys dies as monoculture. Instead **each Active class runs its own DAA**
  (the multi-algo-chain construction — Myriad/DigiByte-style — with per-class targets), each
  targeting its share of block cadence. Real miner economics then price the classes
  continuously, and no committee maintains a coefficient table.
* **Class failure:** the surviving Active classes absorb a failed class's share under the class rules.
* **Hash anti-stall floor — 不採用。** 有用計算を hash mining で迂回する誘因と混合 difficulty の負担があるため、[ADR-0039](0039-palw-only-block-production.md) で撤回した。後続の [ADR-0060](0060-the-liveness-doctrine.md) の emergency heartbeat は別の liveness 規則である。

## Decision E — What hash still does

Hash の block-production right と追加 fork-choice weight は採用しない。有用計算を hash mining で迂回できるため（[ADR-0039](0039-palw-only-block-production.md)）。header/transaction identity、commitment、signature input、randomness と既存 tie-break の hash 利用は維持する。

## Decision F — Fork choice, IBD, and the fabrication problem

PALW work is **not self-authenticating** — that is the deep difference from hash-PoW and the
one place this design must not hand-wave. A fabricated commitment root costs zero GPU work;
only verification distinguishes it from real work. Therefore:

* **Weight is receipt-licensed or finality-licensed, never claim-licensed.** A block's pwu
  enters fork-choice weight only at `ramp ≥ ρ_r` (bonded receipts from the assigned panel —
  ML-DSA signatures a private-fork attacker cannot forge for validators they don't control)
  or at PALW-final *on the receipt-covered chain*. An attacker's private fork can fabricate
  its own blocks but not the panel receipts of bonded validators it doesn't own, so its
  fabricated pwu never matures; it earns no hash-work fallback weight (ADR-0039 W4′).
* **IBD:** a syncing node validates headers, spam hashes, carriage shape, receipt coverage
  and refutation absence — all DAG-objective; it never re-runs inference. Deep history is
  additionally fenced by the existing pruning-depth rule and the VLT finality overlay
  (ADR-0009/0024 lineage), which was built for exactly this class of deep-reorg refusal.
* Within the unbonding period, receipt keys are slashable; the long-range variant (keys that
  already unbonded re-signing an alternate past) is fenced by pruning depth + VLT, the same
  two fences the chain already relies on for stake.

## Decision G — What survives from ADR-0037, re-seated

| ADR-0037 piece | Fate here |
| --- | --- |
| Job state machine (`palw_job_state.rs`, I8/I9/I10 as theorems) | **Carried.** The lifecycle now describes a *block's PALW work*: Provisional → ChallengeWindow → Disputed → court verdicts. The landed module is the spine unchanged; `Open/Committed/PanelSelected` map to production-side states. |
| Court (ADR-0030–0033) | Carried verbatim; promoted to L1. |
| Identity/signature binding (Decision 3 domains) | Carried; commit message binds the header context. |
| Future-anchor panels, dual deadlines (Decision 4) | Carried; now assigns *block* panels. |
| Class registry, freeze, six-path gate (Decision 9) | Carried; freeze now also removes the class from the difficulty domain set (Decision D). |
| Mint hygiene: subsidy carve, budgets, exact bond-outpoint payees, three-pool separation (Decision 7) | Carried for *rewards*. Block subsidy funds class production and receipt fees under subsequent rules; still never exceeds schedule (I6/I15). |
| P_check exclusion, leverage caps (Decision 8) | Carried; receipts are bonded bets (Decision C). |
| No admin per-job/per-block override | Carried (I13). |
| **Decision 1 (hash floor as primary; PALW never block-critical)** | **Superseded.** Inverted by Decisions A–E. |
| M0–M3 staging | Reshaped: M-stages now stage *class count, receipt quorum k, ρ_r, and the PALW share of subsidy*, not "PALW off→on". A soak network runs the full shape from its genesis. |

### What actually blocks Decisions B and C: the carriage store cannot name a chain

Both remaining wirings need the same input — `PalwResolverInputV1::carriage`, specified as *"carriage
records accepted on THE CHAIN BEING EVALUATED, within the challenge horizon"*. The store that holds
those records cannot answer it.

`PalwCarriageRecord` is `{ kind, accepted_daa_score, body }` — precisely the `(u8, u64, Vec<u8>)`
tuple the resolver takes, which is what makes the mistake inviting — and it carries **no accepting
block**. A DAA score is not a chain identifier; two competing branches both have them. A wirer who
reaches for `PalwCarriageStore::all()` therefore mixes evidence from branches the node reorged away
from into the weight of a block on the branch it kept, and nothing about the call looks wrong.

This is the fourth appearance of one defect family in this ADR's implementation. The class target,
the class freeze, and the dispute walk were each rewritten away from a store for it, and the
recurring lesson recorded above — *"reach for the store only when a fold provably cannot answer"* —
was written from the first three. This is the case where the fold's cost is real rather than
imagined, so it is a decision rather than a repetition:

| | correctness | cost |
| --- | --- | --- |
| Fold over the chain path's accepted transactions | correct by construction, like every other PALW fact | the walk re-runs per candidate chain |
| **Add the accepting block hash to the row**, filter by chain membership — **CHOSEN 2026-08-19** | correct, and cheap to read | changes the stored row format |

**Resolved: the row names its block.** `PalwCarriageRecord::accepted_block`, schema v2. The reindex
cost priced into this decision turned out to be machinery that already existed —
`PALW_CARRIAGE_SCHEMA_VERSION` discards rows written under an older layout and re-sweeps history on
the next start, so it is automatic rather than an operator step.

Reading it needs reachability, which the store does not have, so the filter lives on the virtual
processor as `palw_carriage_on_chain_v1`: a row belongs to this chain iff its `accepted_block` is
`chain_tip` or a selected-parent-chain ancestor of it. Asked, not guessed from a number.

**What this does not solve, stated because the name invites the assumption.** `stage_palw_carriages`
inserts for `chain_path.added` and deletes for `chain_path.removed`, so the store holds rows for the
APPLIED chain. `accepted_block` lets a reader exclude a stale row; it cannot conjure a row for a
candidate chain that was never applied. Weighing an unapplied candidate still requires walking that
candidate's own accepted transactions. So this closes the correctness question for the chain a node
has, and Decision B's fork choice — which compares against chains it does not have — carries the
fold's cost for that comparison whatever this store does.

### The producer's signing seam — the last thing that was not "write a function"

Decision A's producer was blocked on a key, not on logic. The bonded ML-DSA-87 key lives in the
validator sidecar; `misaminer` holds only its BIP39 payout key, so the miner cannot sign the
commitment its own block carries. `ValidatorKey::sign_palw_block_commitment_v1` closes that, and its
SHAPE is the security argument rather than a detail of it.

**It is not a "sign these bytes" call, and must never become one.** A sidecar that signs a digest
handed to it by another process has given that process the key: the digest of a stake attestation,
of a precommit, or of a transaction input is bytes like any other, so a compromised — or merely
buggy — miner could obtain a signature that slashes this bond or spends its funds, and nothing in
the request would look wrong. Two properties prevent it, both structural:

1. **The digest is derived inside the signer from a typed commitment.** The caller passes the
   payload and the attempt it was mined under; `PalwBlockCommitmentV1::message` recomputes what gets
   signed. No input to the method can express "an attestation".
2. **The context is `PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT`**, disjoint from the attestation,
   precommit and transaction contexts. ML-DSA binds the context into the signature, so even a digest
   that collided with an attestation message yields a signature no attestation verifier accepts.
   The test asserts the negative directly — the same message under the attestation and precommit
   contexts does not verify.

The signer shape-checks first, in `sign_precommit`'s spirit: signing what consensus will reject only
burns an attempt, and doing it silently makes a misconfigured miner look broken. What it deliberately
does NOT check is that `executor_bond_outpoint` is a bond this key backs — the sidecar does not hold
the bond registry, and a signature over a foreign bond simply fails at admission, because the
registry resolves the key from the bond rather than from the commitment. That failure is a rejected
block, not a loss.

### External audit, 2026-08-19: NO-GO — and what the status table below does not mean

An independent audit of this commit returned **NO-GO with ten independent activation blockers**
(`docs/palw-critical-audit-2026-08-19-ja.md`). Four were re-verified on this side by running the
code the auditor could not run, and all four hold. **Three of them are lines written the same day as
the status table below**, and a fourth invalidates a safety claim made in that day's commits:

| | |
| --- | --- |
| **P0-1** commitment is not bound to the PoW ticket | one PoW solution → unlimited distinct block identities |
| **P0-2** the commitment signature is never verified at admission | W8 becomes "name any Active bond", not "hold a bond" |
| **P0-3** a fresh tip can never resolve | PALW fork choice never fires; every contest falls back to blue work |
| **P0-4** candidate weight depends on the reading node's sink | equal DAGs, different weights — the W3 violation, i.e. partition |
| **P0-6** conviction/equivocation verified under the receipt context | a bond is slashed while its block keeps its weight |
| **P0-7** executor exclusion compares an outpoint to a key hash | the producer sits on its own verification panel |
| P0-5, P0-8, P0-9, P0-10 | two fork-choice authorities; a court that cannot adjudicate; an incomplete ladder; unbounded bond exposure |

**So read the table below as "each decision has a code path", never as "PALW may be switched on".**
The five fences stay `None`. The audit's own conclusion is the operative one: these do not close with
small patches, because admission, fork choice, court and economics have to be closed together — and
enabling one fence at a time is specifically what must not be done, since each defect is currently
masked by another (every node still re-runs the LLM, which is itself a W1 violation, and that is
what makes fake-root grinding expensive today).

**The methodological finding matters as much as the list.** Decisions A–H were reported implemented
and tested on 2026-08-19, with 932+207 tests passing and zero warnings, and the same day's code
contained four P0s. Tests that pass are not evidence of soundness at a seam the tests do not model;
this ADR's own record of four consensus defects found by reading rather than by testing (the late
receipt, the late `Open`, the unbonding erosion, the carriage chain-scope) says the same thing from
the other direction. **Activation requires an audit that clears, not a suite that passes.**

### Implementation status — Decisions A–D and H, 2026-08-19

Every decision now has a code path. What each rests on is stated rather than implied:

| | state |
| --- | --- |
| **A** — admission predicate | wired into the pipeline; the producer's signing seam exists (`sign_palw_block_commitment_v1`) |
| **B** — acceptance now, weight later | block weight → chain weight → **fork choice**, via the `order_tips_v1` seam |
| **C** — assigned sampling | panel drawn from chain state (candidates → anchor → seed); duty accounting; I10 freeze |
| **D** — per-class DAA | the class's share is **looked up by class id**, not assumed to be the cadence |
| **H** — 120 s cadence | frozen, and refused at `Params` construction rather than at sync time |

**Dormant on every shipped network, and that is a fact about the presets rather than a structural
guarantee.** Five fences — `palw_credit`, `palw_block_commitment`, `palw_schedule`, `palw_ramp`,
`palw_fork_choice` — are `None` on every preset, each is Some-only in the fingerprint, and
`absent_schedule_and_ramp_fences_do_not_move_the_fingerprint` pins that. With them off,
`palw_tip_weights_v1` returns before touching a store and the sink heap is byte-identical to the
blue-work heap it replaced.

**Two things this does NOT make true**, because the same sentence would otherwise be read as
"activate it":

* The bisection ladder still affords 2^10 steps at both presets, and nothing has checked that
  against the pinned model's real `step_leaf_count`. The floor arithmetic says it is exceeded at a
  few tens of tokens. Decision H keeps the *option* of closing that open; it does not close it.
* Nothing has run this on a network. Every claim above is a claim about code and its tests.

### The ladder gap, measured — and the prerequisite nobody has built

Decision H's open item is now closed as a question and open as a problem: **the bisection ladder
cannot reach the pinned model's step space at any shipped preset**, and this is pinned by
`the_pinned_model_needs_a_deeper_ladder_than_any_preset_affords`.

Both presets afford `2^10 = 1 024` steps. The pruning horizon caps them at `2^12` (deci-bps) and
`2^17` (120 s). Against the pinned geometry — `MODEL_LAYER_COUNT = 24`, `MODEL_HIDDEN_DIM = 2048` —
the floor of the plausible envelope (3 nodes per layer, whole-row tiles, 16 tokens) is **1 152
leaves**, already past the affordable space; the middle of it is `2^16`–`2^18`. A step-level fraud
is therefore **unprosecutable at these parameters**: the terminal opening and the conviction land
past `w_challenge` and are discarded, so the ladder convicts nobody and the honest challenger spends
its rungs for nothing.

The test asserts the envelope rather than a number, and the reason it must is the larger finding.

**No `PalwShapeProfileV3` has ever been built for the pinned model.** Every profile in the
repository is a test fixture — `palw_step_leg`'s, `palw_step_refute`'s, `palw_adversarial`'s, and
`palw_registry::fleet_registration()`'s, whose doc says it is "built from the REAL fleet measurement
(slowest host, 2026-08-16)" and which carries `layer_count: 4, hidden_dim: 16, ffn_dim: 16,
tile_len: 16`. The real model is 24 × 2048.

That profile is what defines **what a step IS**. Without it:

* the step space cannot be measured, only bounded — hence the envelope above;
* **A4 cannot be evaluated at all.** "The court's kernel catalog is 100% of reachable kernels per
  Active class" is a statement about the reachable set, and the reachable set is enumerated from the
  profile. A catalog-coverage certificate over a 4-layer, 16-wide fixture certifies nothing about
  Qwen3.5-2B;
* a second class cannot be registered, because registration carries a profile;
* the court's convictions are defined against a graph the network does not run.

**This is the prerequisite for three of the four layers a mainnet needs** (the ladder, multi-class,
and the numeric parameters that depend on step counts). It is not a gap in this ADR's decisions — A
through H are implemented and tested — it is a missing description of the model to the court, and it
must be derived from the pinned llama.cpp graph rather than from anyone's knowledge of transformer
architecture: a profile that is *plausible* but not *what the runtime does* convicts honest
producers on every step it gets wrong.

## Decision H — Block cadence is FROZEN at one block per 120 seconds (testnet and mainnet)

**Confirmed, 2026-08-19. Not a tuning parameter and not a launch-window choice: every PALW network
carrying value — testnet-11 and mainnet — targets a 120-second block interval.** The deci-bps
(10-second) preset stays in the tree for tests and must never be installed on such a network.

Two independent constraints force it, and each alone is sufficient.

### 1. Sync headroom — below 1× the network is permanently closed to new nodes

```text
headroom = block interval ÷ per-header verification cost
```

Below 1× a joining node falls further behind with every block it verifies and can never finish.
This is not slow sync; it is a network that cannot admit a participant again, ever.

Measured on the reference x86-64 CPU class host against the pinned Qwen3.5-2B:

| cadence | per-header cost | headroom | |
| --- | --- | --- | --- |
| **120 s (this decision)** | 15.7 s (clean) | **3.0–5.4×** | converges |
| 120 s, node co-located with another PALW process | 37–65 s (measured 2026-08-19) | 1.8–3.2× | converges, no margin |
| 10 s (deci-bps preset) | 15.7 s | 0.64× | **never converges** |
| 10 BPS (0.1 s) | 15.7 s | **0.0064×** | never converges |

The 10-second preset is already below 1× at the pinned model on the reference host. That is the
measurement that closes the question — the faster presets are not aggressive, they are outside the
feasible set for this class.

Decision A's W1 (full nodes never run the LLM) removes the per-header inference from this cost and
will widen the margin substantially. It does **not** license a faster cadence on its own, because of
the second constraint — and until W1 is wired, the running code has no margin to spend.

### 2. Ladder depth — a shorter window cannot prosecute step fraud

`affordable_ladder_rounds_v1` is `(w_challenge − w_replay − after·w_round) / (per_rung·w_round)`, and
both shipped presets afford **10 rounds = 2^10 = 1,024 steps**. Raising `w_challenge` is bounded by
the pruning horizon, which caps **deci-bps at 12 rounds (2^12)** and the **120-second preset at 17
(2^17)** — a 32× difference in the step space that can be walked to a terminal index before the
challenge window closes. A fraud deeper than that is unprosecutable: the terminal opening and the
conviction land past `w_challenge` and are discarded.

Only the 120-second preset has room to grow the ladder toward a realistic model's step space. A
faster cadence forecloses the court permanently, and no amount of implementation fixes it.

**Open item, tracked separately**: 2^10 has not been checked against the pinned 2B model's actual
`step_leaf_count`, and the floor arithmetic says it is exceeded at a few tens of tokens. The cadence
decision above is what keeps the *option* of closing that gap open; it does not close it.

### Consequences

* `PalwScheduleParamsV1::stage1_defaults_two_minute_bps` is the only preset admissible on a value
  network. `stage1_defaults_deci_bps` is test-only.
* Emission is the rate-preserving 120-second table (4445.62 BILI/block).
* Any future proposal to shorten the interval must first re-measure headroom on the then-current
  class and re-derive the affordable ladder depth. Neither is a review comment; both are numbers.
* Enforced as `PalwScheduleParamsV1::validate_for_value_network_v1`, which checks the cadence
  **before** the window arithmetic. That order is deliberate and is itself asserted: run the windows
  first and an operator who shortened the interval is told the pruning-depth inequality failed —
  true, and it reads as "widen a window", which is the one repair that cannot work here.

## Invariants v2 (release-blocking; supersede ADR-0037's I1/I2/I12, carry the rest)

```
W1  A full node validates every block, and adjudicates every dispute, with no LLM
    runtime, no GPU, and no model artifacts beyond Merkle-proven operands
W2  One PALW ticket costs one canonical inference, bound to (network, header, nonce);
    tickets are non-transferable and non-replayable across headers
W3  weight(B) is a pure function of the DAG; equal DAGs ⟹ equal weights everywhere
W4  SUPERSEDED by ADR-0039 W4′. (Was: unverified pwu never exceeds spam-hash backbone
    influence in fork choice — meaningless once ADR-0039 removes the backbone from
    weight(B). Replaced by two derived weights: safe = MATURE only, live = bounded
    published work above the safe frontier, one ordered fork choice over both.)
W5  PALW-final weight is immutable; pre-final conviction voids exactly the convicted
    block's pwu, nothing else
W6  SUPERSEDED by ADR-0039 W6′. (Was: with zero Active classes, the anti-stall floor
    produces degraded blocks and nothing halts the chain. ADR-0039 removes the hash
    floor: a portable integer-only Base PALW class carries the degradation instead, and
    a total PALW outage halts the chain loudly rather than producing hash blocks. The
    "no input halts the chain" clause does not survive — it was only obtainable by
    keeping a non-PALW production path.)
W7  Class freeze removes a difficulty domain and redistributes cadence; it never
    invalidates already-final weight
W8  Producer and attester accountability is bonded: no bond, no block; no bond, no receipt
I3–I11, I13–I15 of ADR-0037 carry unchanged (credit-once, exact payees, full binding,
budget ceilings, missing≠empty, refutation-locks, conviction-only slash, no-Unadjudicable
slash, no cross-class slash, no overrides, deterministic state, emission schedule)
```

## Audit closure — do the criticals recur?

Disposition of the 2026-08-16/17 blocker classes under this design:

| Blocker class (audit) | Disposition |
| --- | --- |
| Every full node depends on one giant runtime; `panic!` on `PalwUnavailable`/`PalwWorkerFailed` | **Eliminated by construction** (W1, W6): full nodes never invoke a runtime. The panic sites die with the re-verification path itself, not by wrapping them. |
| Exclusive `algo_id = 4` = single point of chain death | **Eliminated** (Decision D): N Active classes are N independent production paths. Note: NOT by demoting PALW — by multiplying classes. *(Amended by ADR-0039: the "+ anti-stall floor" term is withdrawn. Survivability rests entirely on class multiplicity plus a permanently-Active portable Base class; with zero Active classes the chain halts loudly by design. The blocker stays eliminated — N independent PALW production paths is the load-bearing half — but the residual case is now a deliberate halt, not a hash lane.)* |
| 100 ms block interval physically incompatible with 37–91 s replay | **Eliminated** (Decision B): replay is off the admission path; it constrains `W_challenge` only. The `finality_depth < W_challenge` preset failure becomes a parameter re-derivation, not a wall. |
| Consumer layer fail-open ×10 (lookup collisions, missing-as-empty, unverified carriage at credit entry) | **Not addressed by this ADR's shape — addressed by the carried Track-C machinery** (ledger state instead of horizon re-walks, verified-entry types, I5/I7). These blockers were never about the layer assignment and would recur in ANY design if Track C is skipped. |
| Payee by `validator_pubkey_hash`; unbounded coinbase append | Carried fix (Decision G: exact bond-outpoint payees, budgeted batch — I4/I6). |
| Artifact pinning (B8 libm, B15 GGUF CWD bypass) | Landed 2026-08-17, carried. Full nodes need it never; *miners and attesters* need it for class identity. |
| Gate-ledger through-line false | Process rule carried from ADR-0036 Decision 3. |

**Verdict: none of the audited criticals recurs — two of the seven classes are closed by this
ADR's structure, and the rest are closed by machinery this ADR carries forward. But the design
introduces three NEW critical-class risks, closed as follows; a review that only re-checks the
old nine would miss them.**

## New risks introduced by PALW-as-L1, and their closures

1. **Fabrication economics (the killer).** With optimistic default-accept, a fake commitment
   root costs ~0 and, unrefuted, would finalize: fabricated pwu for free, repeated forever.
   *Closure:* bonded producers (Decision A) + mandatory assigned sampling with near-certain
   coverage (Decision C) + receipt-licensed weight (W4). Expected value of fabrication is
   `−bond × P(conviction)` with `P(sample) ≈ 1` by assignment; unsampled fabricated work never
   outweighs the backbone. **Without bonds this design is unsound; "pure permissionless
   PALW mining" is not on the table and the ADR says so explicitly.**
2. **Mutable-weight fork choice.** A weight that changes after admission is a new consensus
   surface (the audit taught what those cost). *Closure:* W3 (weight is a DAG-pure function),
   W5 (single, monotone-except-conviction mutation, bounded horizon `W_challenge`), and the
   rule that PALW-final weight is immutable. This is the change set's hardest correctness
   target and gets its own adversarial suite before any value network (reorg-equivalence
   tests: same DAG ⟹ same virtual, across receipt/conviction orderings).
3. **Class monoculture via mispricing.** *Closure:* per-class DAA (Decision D) — prices are
   discovered, not maintained. Residual: a class whose real cost collapses (new hardware)
   gains share until its DAA catches up — bounded by adjustment latency, same exposure every
   multi-algo chain carries.

## Residual assumption set (what a signer of this ADR accepts)

```
A1  ≥1 honest, bonded watcher per Active class is live within every W_challenge
    (1-of-N optimistic assumption; panels make coverage assigned, not hoped-for)
A2  The bonded receipt set is not majority-corrupt within the unbonding period
    (deep-reorg fenced by pruning depth + VLT beyond it)
A3  Class PWU derivations are canonical and frozen; per-class DAA absorbs pricing error
A4  The court's kernel catalog is 100% of reachable kernels per Active class
    (Track D gate unchanged — Unadjudicable-on-gap + freeze is the enforcement)
A5  Hash-based production / weight assumptions are withdrawn by ADR-0039.
```

## What this ADR does not decide

Numeric parameters: ρ_r, k, W_challenge, per-class initial targets, the
PWU derivation formula and bond sizes. All are soak/simulation outputs
(ADR-0036 "does not decide" carried). The TN11/devnet migration path (they already run the
single-class ancestor of this shape; the delta is commitment-root headers + receipts + ramp)
is a soak-planning decision. M3 (PALW weight in VLT/BFT voting) remains a separate ADR.

## Summary

ADR-0037 secured the chain by making PALW optional. That was the wrong fix for the right
blockers. The right fix keeps inference as the scarce resource that orders blocks — the
algo-4 lottery already had the correct shape — and replaces the one fatal coupling: full
nodes verifying by re-execution. Verification becomes: assigned bonded sampling as the alarm,
the existing exact-bit court as the truth, weight that matures with evidence, classes that
fail independently. The former hash-floor production and weight paths are withdrawn by ADR-0039.

## Mission alignment amendment — 2026-10-07

PALWがconsensus workを担うという設計を維持し、そのworkが影響を持つ前提として公開訴追可能性を追加する。二重weightやhash/LLM価格の過去記録を、本目標で変更したとは扱わない。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

PALWを正当な有用計算として要求する方針に、producer資本・期間に基づく確定consensus creditの上限を追加する。canonical計算量/PWUの真の記録と付与するweight配分を分離する。
将来の容量拡大で、暫定capだけを維持してFinal時に未予約full weightを加算する経路は改定対象である。Final/retired/reversalと全fork-choice/settlement readerで同じ予算を使用し、旧claimは旧rulesetのまま完了する。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0038-palw-is-the-consensus-work.md -->

</details>

<a id="source-0039-palw-only-block-production"></a>

<details>
<summary>0039-palw-only-block-production.md — 原文 L1–L391</summary>

[原文を開く](../adr/0039-palw-only-block-production.md) · SHA-256: `b3d9b788895bafba54ef21ade4a58f444cec5bb8e5bf58b53082155a0bb4d67a`

<!-- ADR_SOURCE_BEGIN:0039-palw-only-block-production.md -->
~~~~~~~~markdown
# ADR-0039: PALW-only block production — a Base class instead of a hash floor, and a two-weight fork choice

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Proposed, with Decision 5 and its 2026-08-17 amendment SUPERSEDED IN PART by
ADR-0045.** Activates nothing. Supersedes **ADR-0038 W4 and W6** (restated below as W4′ and W6′)
and amends its Decision B and Decision E. Everything else in ADR-0038 — the layer inversion,
sampled verification, the court, per-class DAA, bonded producers — stands. Decisions 1–4 and 6 of
this ADR stand unchanged; Decision 5's epoch cap is re-denominated by
[ADR-0045](0045-palw-class-economy-on-chain.md) Decision 2 — see the banners on that decision.

Date: 2026-08-17
Relates to: ADR-0038 (the layer inversion this completes), ADR-0036 (lineage and the mainnet
identity requirement), ADR-0027/0028 (refutation and sampling), ADR-0030–0033 (the arithmetic
court, and the reason the Base class comes first), ADR-0034 (classes and bands),
`docs/palw-mainnet-readiness-audit-2026-08-16-ja.md` (the re-audit whose blockers 1–4 this ADR
answers), branch `palw-only-v4` commits `7cc6b08`, `b385803`, `92f563d`, `04c2650`.

## Context — what "PALW is the consensus work" has to mean

ADR-0038 made PALW the primary consensus work but kept a hash **anti-stall floor** that produces
blocks when every class is unavailable (W6), and kept `spam_hash_work` as a permanent additive
term in `weight(B)` (W4). Both are hash paths to the two things that define consensus
participation: **the right to produce a block**, and **weight in fork choice**. While either
exists, "the scarce resource that orders blocks is inference" is conditional.

Three uses of hashing are *not* in question and are retained: block and transaction identity,
Merkle commitments, artifact and model pinning, and compressing signature inputs. Removing those
would discard half the machinery of a blockchain for no security gain. What this ADR removes is
narrower and exact:

1. `H(header) < target` as a **block-production right**;
2. the hash floor as an **emergency block-production path**;
3. cheap hash as a **fork-choice weight** term.

## The impossibility, stated rather than finessed

These three cannot hold simultaneously:

```
A. Only PALW confers the right to produce a block.
B. Blocks are produced even when zero nodes can execute PALW.
C. There is no hash / PoS / BFT fallback production path.
```

If no party can execute PALW, there are no PALW proofs, and no rule can conjure a block from
their absence. ADR-0038 chose B and C by keeping a hash floor, which costs A. **This ADR chooses
A and C, and pays for it with B.** The guarantee therefore reads:

> While at least one genesis-authorized PALW class can produce certified work, the chain
> continues. When none can, the chain **halts loudly**. Validators do not manufacture economic
> blocks without PALW work.

That is a real cost and it should be signed for deliberately, not discovered later. The purpose
of Decision 1 is to make the antecedent overwhelmingly likely to hold.

## Decision 1 — The floor is a CLASS, not a hash: `PALW-BASE-0`

Mainnet genesis authorizes, alongside the accelerated classes, a portable base class:

* executes on **any** general-purpose CPU, scalar code path only;
* **no** GPU, CUDA, ROCm or Metal dependency;
* **no `libm` and no floating point** — fixed-point/integer arithmetic throughout;
* no external Ollama, no external llama.cpp, no external GGUF path: the model, tokenizer and
  kernel graph are pinned as a mainnet artifact;
* two independent implementations agreeing bit-for-bit before it may carry weight;
* **carries a non-zero share (≈ 5 %) in normal operation** and is exercised continuously;
* may receive up to 100 % of future share at an epoch boundary when other classes die.

The always-on requirement is the load-bearing half. A standby path that runs at 0 % share is a
path that has never run, and it will fail the first time it is needed — the failure mode is not
hypothetical, it is the ordinary history of untested failover.

Indicative shape, at most ~5 classes:

| Class | Role |
| --- | --- |
| CUDA | NVIDIA-class accelerated PALW |
| ROCm | AMD-class accelerated PALW |
| Metal | Apple-silicon PALW |
| CPU-OPT | AVX2 / AVX-512 / NEON |
| **CPU-BASE** | scalar integer implementation — always live, final survivor |

Degradation is then class redistribution, never a change of algorithm:

```
CUDA dies            → share to ROCm / Metal / CPU-*
all GPU classes die  → share to CPU-OPT / CPU-BASE
optimized CPU dies   → CPU-BASE alone, slower
```

**No hash block is produced at any step.**

### 1a. The Base class is built FIRST, not last — and this is the strongest argument for it

The natural reading of a "final survivor" is "the thing we finish last". That ordering is exactly
wrong here, for a reason the original proposal did not claim and which this ADR adopts as
normative:

> **`PALW-BASE-0` is the only class in which conviction is reachable in the near term.**

Integer-only means no `libm`, no FMA contraction, no transcendentals, no denormal behaviour — so
the ADR-0030–0033 kernel catalog can actually reach 100 % coverage for it. Every accelerated
class is blocked on transcribing quantized matmul, softmax and RoPE, which today leave the
catalog resolving 6 of 17 op kinds and a lie in the ops that carry the computation terminating
`Unadjudicable` (re-audit). Under optimistic verification, **conviction is the only thing standing
between a fabricated block and full weight**. Therefore:

* **No class may carry fork-choice weight until its catalog coverage is complete** (ADR-0038 A4's
  gate, made a weight precondition rather than only a credit precondition).
* `PALW-BASE-0` is the first class to satisfy it, so it is the first class that may carry weight,
  and it should be built and soaked first.

This also removes the `libm` false-conviction hazard from the floor entirely: an integer class
cannot have two honest implementations disagree by one ulp, so the failure that would slash an
honest producer (and, under the ramp, void weight retroactively) is unrepresentable there.

## Decision 2 — W6′ (supersedes W6): PALW-only liveness

> Mainnet shall not recognize hash-only work, hash-floor work, or any non-PALW block-production
> path. Every weighted block MUST carry a valid certificate for a genesis-authorized PALW class
> whose catalog coverage is complete.
>
> At least one portable, integer-only Base PALW class MUST remain Active with non-zero target
> share at all times.
>
> When an accelerated class becomes unavailable, its future epoch share SHALL be redistributed
> among other Active classes through finalized class-health transitions.
>
> If all accelerated classes are unavailable, the Base class MAY receive 100 % of future share,
> and the target block rate MAY decrease.
>
> If no genesis-authorized class can produce certified work, the network SHALL halt loudly.
> Validators MUST NOT manufacture economic blocks without PALW work.

### 2a. Degraded mode — what actually helps, stated correctly

Reducing target BPS in degraded mode is right, but **not for the reason usually given**: it does
not improve the security ratio. An attacker's advantage is their work over honest work, and
lowering the block rate scales both. Slowing production buys ordering stability and less work
wasted on orphans; it does not make a weakened chain harder to attack.

The levers that do change the ratio, or bound the damage:

* **suppress emission** in proportion to real work — do not pay a full subsidy for a fraction of
  the security;
* **extend the challenge window** — more wall-clock for a watcher to refute, against a smaller
  and slower adversary set;
* **extend or suspend finality for high-value operations** — bridge, mint, registry updates —
  while ordinary UTXO transfer continues;
* **raise the share threshold** at which a class may re-enter, so a flapping class cannot
  oscillate the domain.

Indicative only, to be measured, never shipped as-derived: `10 BPS → 2 BPS → 0.2–1 BPS`.

> **Throughput is the thing to spend in a degradation. Difficulty is not.**

## Decision 3 — W4′ (supersedes W4): two derived weights, one fork choice

### 3a. The constraint that decides the representation

The re-audit's first blocker is a mechanical fact, verified on this branch: a maturing weight
scalar **cannot** be `header.blue_work`. `blue_work` sits inside the pre-PoW preimage, is
miner-declared, and is validated by **exact equality** against the recomputed GHOSTDAG value
(`consensus/src/pipeline/header_processor/post_pow_validation.rs:49`); the pruning proof
(`processes/pruning_proof/{build,apply,validate}.rs`), difficulty and window machinery all read
it. A value that matures after the fact cannot be a value that is fixed under a PoW commitment
and re-derived identically by a pruning proof.

The resolution is not to make `blue_work` mutable. It is to **stop asking it to carry PALW
weight at all**:

* `header.blue_work` remains exactly what it is today — an immutable, PoW-covered accumulator —
  and pruning proofs, difficulty and window machinery are **unchanged**. Under Decision 4 it
  ceases to confer any production right; it is a structural accumulator, not a security budget.
* **PALW weight is never serialized into a header.** It is derived per node from DAG data, and
  the determinism obligation (W3) applies to the derivation.

### 3b. The two weights

```
safe weight(B)   = Σ pwu over MATURE ancestors only
                   (MATURE = receipt-licensed ∧ window closed ∧ no conviction ∧ no open dispute)

live weight(B)   = safe weight(frontier) + Σ bounded pwu over PUBLISHED, bonded,
                   not-yet-mature descendants of the frontier
```

* **safe weight** governs IBD, deep-reorg bounds and economic finality. A private fork cannot
  accumulate it, because it cannot obtain receipts from honest assigned verifiers while private —
  this is what kills fabricated forks, and it is why `ramp_stage_v1` was corrected (commit
  `04c2650`) so that a closed window without a receipt quorum does **not** finalize.
* **live weight** governs tip selection only, and only above the safe frontier. Its purpose is to
  stop assigned verifiers holding an honest chain hostage: withholding receipts must delay
  *maturity*, never *production*.

### 3c. One fork choice, not two

Two independently-consulted weights would be two answers to "what is the tip", which is a
partition waiting to happen. The rule is a single ordered procedure:

```
1. Take the highest safe-weight block — the SAFE FRONTIER.
2. Among its descendants only, select by live weight.
3. Ties break as today.
```

This is the familiar "finalized checkpoint, then fork choice above it" shape. Below the frontier
nothing is reorganizable by live weight; above it, live weight cannot outrank a matured branch.

### 3d. Determinism obligations (W3, restated as requirements on the derivation)

Every one of these is a partition if violated:

* No local wall-clock, no receipt **arrival** order, no peer-observation order may enter. Every
  threshold — receipt inclusion deadline, challenge deadline, weight activation, conviction
  cutoff — is evaluated at a **blue-score or finalized-epoch boundary**, never at "when I saw it".
* `live weight`'s bound on immature pwu must be a fixed fraction fixed at registration, so two
  nodes that disagree about nothing cannot disagree about the bound.
* A pruned node must reach the same answer as an archival node. Facts the derivation needs must
  live in pruning-surviving state, or the derivation must be defined to a bounded horizon that
  fits inside the pruning horizon. **Reading absent data as "nothing" is forbidden** — a missing
  fact is an error, never a permissive zero.
* GHOSTDAG's k-cluster blue-set computation continues to use `blue_work`; it is about DAG
  structure, not work magnitude. Only tip/virtual selection consults PALW weight. Affected site
  to change deliberately: the header-selected-tip store's ordering
  (`header_processor/processor.rs:416`).

### 3e. Retroactive void, bounded

A conviction before maturity voids that block's pwu (W5, unchanged). Because safe weight counts
only MATURE work, a conviction can never rewrite safe weight — it can only prevent work from
entering it. Retroactive void therefore acts on live weight alone, inside the challenge window,
above the safe frontier. This is what keeps "mutable weight" from meaning "mutable history".

## Decision 4 — The ticket is not a hash puzzle

The criterion, adopted verbatim as the test any future ticket construction must pass:

> **Does trying one new ticket require one new PALW execution?**

`H(header) < target` fails it: a candidate costs a hash. The construction landed on this branch
(commit `b385803`) passes it, by binding the tag to the exact attempt —
`challenge = H(network ‖ pre_pow_hash ‖ timestamp ‖ nonce ‖ class ‖ bond_outpoint)`, with the
tag derived from the trace under that challenge. Changing the nonce changes the challenge and
owes a new inference; a commitment cannot be replayed onto another attempt, header, class or
executor.

Two refinements the ADR fixes normatively:

**4a. A final hash is mandatory, and does not make this hash-based production.** Deriving the
ticket from `H(domain ‖ trace_accumulator ‖ challenge ‖ class_id)` is not a hash lottery, because
`H` is an *extractor* over a value that costs an inference to produce: grinding `H` a trillion
times yields one candidate, since the accumulator does not change. Hashing is what makes the
ticket uniform; the inference is what makes it scarce.

**4b. A bare linear accumulator MUST NOT be the ticket.** A rolling accumulator of the form
`A_{j+1} = α_j·A_j + β_j·Q(S_j) mod p` makes `A_m` a **linear combination** of the per-step
states. Any freedom in a late `S_j` — a quantization boundary, a controllable trailing token,
padding — lets an attacker *solve a linear equation* for a value landing `A_m` under target,
with no re-execution. That defeats the Decision 4 criterion directly. The accumulator may be a
useful internal digest; it may not be the ticket without a final cryptographic hash over it.

## Decision 5 — Per-class share, epoch caps, and how caps are enforced

Static cross-class PWU coefficient tables are rejected (ADR-0038 Decision D, retained): any
mispriced class becomes a standing arbitrage and multi-class collapses into a monoculture, which
is the failure the multi-class design exists to prevent. Price is discovered per class by
independent DAA; PWU stays static **within** a class, derived from normative operation counts.

> **旧 weight/PWU 建て epoch cap は不採用。** 成熟で変わる weight は admission の固定基準にならず、PWU 建て cap は重い class を自身の cadence share より下に制限するため。share の params 固定も登録・成長を表せないため置き換えた。[ADR-0045](0045-palw-class-economy-on-chain.md) Decision 2/3 の block-count budget と chain-state share を採用する。

cap は producing block 自身の selected-chain class counter に対して判定し、merger を罰しない。この目的と、share を自己申告で与えない規則は維持する。

### Decision 5 amendment (2026-08-17) — the clause as written cannot be enforced at admission

旧 PWU 建て inequality の実装計画は継続しない。class 間の PWU 比較が誤価格と starvation を再導入するため。V2 の enforcement は ADR-0045 の凍結された block-count budget に従う。

## Decision 6 — Bonded is not permissioned

ADR-0038's phrasing that "pure permissionless PALW mining is not achievable" is withdrawn as
misleading. A bond that anyone may lock by protocol is **bonded permissionless mining**: the bond
is collateral that makes a false commitment expensive, not a licence that someone grants.

What remains prohibited, and what a reviewer should check for: administrator approval,
allowlists, manual registry vetting, or any validator holding a veto over who may produce.

Relay may bound in-flight candidates per bond outpoint (one, or a small fixed number, of
immature candidates). That is what replaces the hash anti-spam floor for flood control, and it is
an identity-neutral rule.

## What this ADR does not decide

* **The PWU derivation.** It carries 90–99 % of weight and is today a self-declared `u64` checked
  only for non-zero (re-audit blocker 6). Its normative derivation is required before any class
  carries weight, and is its own decision record.
* **Catalog completion** for the accelerated classes, and the second implementations.
* **Mainnet parameters** — identity, genesis, window set, share table, `ρ_r`, `k`, bonds. Those
  are soak outputs (ADR-0036), and this ADR deliberately fixes only shapes and inequalities.
* **Whether the accumulator of 4b is used at all internally.** Only its use as a bare ticket is
  refused.

## Consequences

* **A required adversarial suite before any value network.** The theorem to prove is:
  *equal DAGs ⟹ equal weights*, invariant under receipt arrival order, conviction observation
  order, pruning point, and IBD start height. ADR-0038 named mutable-weight fork choice as its
  own hardest correctness target; this makes that a gate rather than an intention.

  **Status — two of the four invariances are now under test, and the suite has two axes because
  the attacks do.** `palw_facts`'s `once_decided_a_block_stays_decided` sweeps the point of view
  across every boundary the fixtures have, each point seeing only the carriage it has reached: a
  block's classification, once decided, is **permanent and unique**. `Final` and `Voided` are the
  decisions; below them the stage may move freely.

  That looser half is §3e itself, not slack. An earlier draft asserted a monotone rank
  `Provisional < ReceiptLicensed < Final` and forbade exactly what this section permits — it failed
  the moment a real coverage gap became visible mid-window, which is retroactive void acting on live
  weight, above the safe frontier, as designed. The correction is worth recording: the invariant a
  suite asserts can be *stronger* than the ADR and still be wrong.
  `carriage_accepted_after_the_window_changes_nothing` holds the clock and appends one record of
  every kind past the window, in both directions from both a matured base and one short of quorum.
  `resolution_does_not_depend_on_walk_order` covers arrival order.

  Both axes carry `panel_duty_v1` with them. It is not weight, but it names the seats a slash path
  would charge, so a duty set that drifts with the point of view means two nodes charging different
  validators for the same block — and its rule is stricter than the stage's: `Pending` to exactly
  one `Closed` answer, never moving again.

  The second axis is not redundancy. Removing the late-`Open` bound leaves *every* scenario in the
  point-of-view sweep `Provisional` at every point of view — constant, monotone, and passing. Each
  of the five defects fixed on this branch was re-introduced against the finished suite and each
  is caught by exactly one of the two axes.

  **The bridge between the two layers is now asserted, not assumed.** The block layer pins that a
  block only moves up the ladder; fork choice compares `(safe, live)` over a chain. Those connect
  only if advancing any block's stage moves both weights monotonically —
  `advancing_a_blocks_stage_never_lowers_either_weight`, over every ordered pair of rungs. `live` is
  the half that can surprise: a maturing block leaves the immature pool for `safe`, so its
  contribution jumps from `β·pwu` to `pwu`, which is non-decreasing *precisely because* `β <= 1000‰`
  — the reason `PalwChainWeightParamsV1::validate` rejects a larger bound. Above it a block would
  lower its own chain's live weight by maturing, and a chain could be reorged away by the very act
  of its work being verified. Monotone alone is also satisfied by a table whose rungs are all
  identical, so strictness is asserted with it.

  **Still open, and not provable at this level:** pruning point and IBD start height. Both are
  claims about a DAG rather than about a derivation over one block's carriage, so they need the
  wiring first — which is the honest reason they are not here rather than an omission.
* **Code sites this ADR commits to touching:** the header-selected-tip ordering
  (`header_processor/processor.rs:416`), virtual/tip selection in the virtual processor, and a
  new derivation module for the two weights. `blue_work`, pruning proof, difficulty and window
  machinery are deliberately **not** touched.
* **`spam_hash_work` leaves `weight(B)`.** With the term removed, an unlicensed block's weight is
  its bounded live pwu, not a hash quantity — and W4′ replaces W4's "never exceeds spam-hash
  backbone influence", which is meaningless once there is no backbone.
* **The Base class becomes a build-order commitment**, not only a design one: it is the first
  class to complete, the first to carry weight, and the reference against which the accelerated
  classes' catalogs are judged.
* **The halt is real.** Operators must be told plainly that a total PALW outage stops the chain,
  and that this is chosen. The alternative — a hash path that can produce economic blocks — is the
  thing the whole design refuses.

## Residual assumptions (what signing this accepts)

1. At least one Base-class operator is reachable and honest within every challenge window.
2. Two independent Base implementations can be built and kept bit-identical — the integer-only
   constraint is what makes this tractable.
3. Catalog coverage is achievable to 100 % for the Base class's op set.
4. The bonded producer set is large enough that "bonded permissionless" is not permissioned in
   practice. Today it is four hosts under one administrator; that is a launch precondition, not a
   property of the design.
5. Class-health transitions are observed identically by every node, because they are finalized
   facts — a class-death judgment that differs between nodes is a partition.

## Mission alignment amendment — 2026-10-07

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0039-palw-only-block-production.md -->

</details>

<a id="source-0042-palw-mainnet-candidate-ruleset"></a>

<details>
<summary>0042-palw-mainnet-candidate-ruleset.md — 原文 L1–L678</summary>

[原文を開く](../adr/0042-palw-mainnet-candidate-ruleset.md) · SHA-256: `208f479f390b687616edec42cdd3475a0189a7ac8931971045b52b4ac1082997`

<!-- ADR_SOURCE_BEGIN:0042-palw-mainnet-candidate-ruleset.md -->
~~~~~~~~markdown
# ADR-0042: The PALW mainnet-candidate ruleset — one atomic activation, one fork choice, one fingerprint

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Proposed.** Activates nothing, moves no fence, changes no shipped preset. It is the
engineering spec the `palw-v2` branch implements before any public PALW-RC or mainnet activation.

Date: 2026-08-20
Branch: `palw-v2` (cut from `palw-only-v4` @ `26469061`, which carries the external NO-GO audit).
Audit baseline: `9cfcbf99` / `docs/palw-critical-audit-2026-08-19-ja.md` (10 P0s + 2 blockers).

Relates to / builds on:

- **ADR-0035** (public PALW testnet = testnet-11, continued; class pin at the door) — TN11 stays
  what it is. This ADR does **not** re-genesis it.
- **ADR-0036** (mainnet activation model; new network identity required; hash floor SUPERSEDED;
  120 s cadence; params are soak outputs) — this ADR is the "fill-in the ruleset" spec ADR-0036
  §"What this ADR does not decide" promises, minus the measured parameters.
- **ADR-0038** (PALW is the consensus work; invariants W1–W8; Decisions A–H) — this ADR keeps every
  invariant and turns the audit's 12 activation conditions into a single ruleset.
- **ADR-0039** (`PALW-BASE-0` instead of a hash floor; two-weight fork choice; ticket-not-hash;
  per-class DAA; bonded permissionless) — carried in full; §3d determinism obligations are load-bearing.
- **ADR-0040** (`PALW-BASE-0` integer arithmetic), **ADR-0041** (pruning-proof verification).

Supersedes: **nothing's decisions.** It replaces one *mechanism* — the five independent activation
fences — with one atomic bundle, and it names the state, admission, court and fork-choice shapes
those fences would have switched on piecemeal.

---

> **Network table amended (index reconciliation, 2026-09-02).** The "two-network split" table
> below lists testnet-11 as the legacy algo-4 soak (`PalwConsensusMode::LegacyTn11`) and the PALW-RC
> as a separate network (suffix 12). Since then testnet-12 was retired into 11: `--netsuffix=11`
> resolves to `palw_rc_shipped_params` — the RC bundle, `ConsensusV2` — and `--netsuffix=12` is
> refused by name; the `LegacyTn11` lane survives only as the unrouted `TESTNET11_PARAMS` constant.
> The table's RC(n+1) rule is what testnet-11 has lived by (Relaunch 2 through 5e). Later ADRs on
> top of this ruleset: Decision 3a's tag now expands `execution_commitment_v3`
> ([ADR-0072](0072-the-ticket-is-the-execution.md)); Decision 7's no-show verdict is an abstention
> on armed presets ([ADR-0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) D4) and
> its proof-carrying half is [ADR-0062](0062-data-availability-court.md) (Proposed); Decision 11's
> "RC == mainnet by tag" is the route [ADR-0075](0075-certification-is-a-consensus-object.md) §7
> relies on, with mainnet shipping `Disabled` until its card is set. Map: [`README.md`](README.md).

## Context — why an RC, and not five `Some`s

The shipped presets set `palw_credit`, `palw_fork_choice`, `palw_schedule`, `palw_ramp` and
`palw_block_commitment` to `None`, and that is the whole of today's dormancy. The field docs say so
in as many words: dormancy is "a fact about the presets, not a structural one"
(`config/params.rs`), and `palw_fork_choice`'s own doc records that setting it "partitions the
network without altering fork choice." Five booleans, individually flippable, is a machine that
*invites* being half-activated. The external NO-GO audit (2026-08-19) found ten P0s that become live
"if you flip the fences on the current code" — block forgery from one PoW solution, bond
impersonation, a fork choice that never engages, node-to-node divergence on one DAG, an
unadjudicable court, and unbounded immature work per bond.

Three of those P0s (P0-3, P0-6, P0-7) were written the same day they were audited, and a fourth
(P0-2) was being fixed in the worktree while the audit was recorded. That discovery rate is the
argument against incremental fence-flipping: **the risk is not any single fence, it is that a human
flips them in the wrong order, or one at a time, and ships a chain that forks.** The fix is
structural, not procedural.

So the RC is not "TN11 with fences on." It is a separate network, brought up from a new genesis, on
a ruleset that is byte-identical to the one mainnet will ship — the only differences permitted are
network identity, genesis/allocation, address prefix, ports/seeds, and faucet. Everything that
decides consensus is the same, and that sameness is *checkable by machine* (Decision 11).

### The two-network split this ADR assumes

| Network | Role | Same ruleset as mainnet? |
|---|---|---|
| **testnet-11** | Legacy algo-4 PoW soak (ADR-0035); runtime determinism + operator drills | No — `PalwConsensusMode::LegacyTn11` |
| **PALW Devnet** | Short windows, forced fraud, fast state-machine/court iteration | No |
| **PALW Chaosnet** | Multi-host, partitions, fault injection; private | Nearly, but not public |
| **PALW-RC(n)** | Public mainnet candidate | **Yes** |
| **Mainnet** | Ships the last RC's canonical ruleset bytes | **Yes** |

A public RC that changes any consensus rule is **re-genesised as RC(n+1)**, never continued. A
testnet forked repeatedly is not "the same as mainnet," whatever its age.

---

## Decision 1 — One atomic activation bundle, not five fences

The five `Option` fences are replaced, on the V2 lineage, by a single mode enum. A network is in
exactly one PALW mode, and `ConsensusV2` carries *all* of the ruleset or none of it.

```rust
pub enum PalwConsensusMode {
    Disabled,                              // hash networks; PALW machinery inert
    LegacyTn11(LegacyPalwParams),          // ADR-0035 algo-4 soak, unchanged
    ConsensusV2(PalwConsensusParamsV2),    // the RC / mainnet ruleset
}

pub struct PalwConsensusParamsV2 {
    pub protocol_version: u16,
    pub algorithm_id: u8,                  // POW_ALGO_ID_PALW_COMMITTED_V2 (Decision 3)
    pub cadence: PalwCadenceParams,        // frozen 120 s (ADR-0038 H)
    pub class_catalog_root: Hash,          // registered classes incl. PALW-BASE-0
    pub court_catalog_root: Hash,          // adjudicable primitive set
    pub bond: PalwBondParams,
    pub exposure: PalwExposureParams,      // Decision 6
    pub panel: PalwPanelParams,            // Decision 7
    pub windows: PalwWindowParams,         // bind / receipt / challenge / prosecution
    pub fork_choice: PalwForkChoiceParams, // Decision 9
    pub class_daa: PalwClassDaaParams,
    pub rewards: PalwRewardParams,         // Decision 10
}
```

`ConsensusV2` is validated **once, at construction**, and a node refuses to start unless every
invariant below holds. There is no path that switches on "fork choice" without "commitment," or
"schedule" without "credit," because there are no longer separate switches.

Startup invariants (all must hold, or the node does not boot):

```
BASE-0 exists in the class catalog and holds non-zero target share
BASE-0 court coverage == 100% (every reachable primitive is adjudicable)
window_court  >=  worst-case court duration ((ceil(log2(max_step_leaves)) + terminal) × turn deadline)   [amended A1]
bond withdrawal delay  >  bind + receipt + challenge + prosecution + reorg margin   (Decision 6)
the class share table SATISFIES the exposure/starvation inequality (ADR-0039 D5e: no StarvedClass)
a live exposure ceiling is set                                                       (Decision 6)
reward maturity  >  the full liability period                                        (Decision 10)
the safe/live fork-choice comparator is present and total                            (Decision 9)
protocol_version, algorithm_id, and both catalog roots are the ones the fingerprint commits to
```

This is the plan's central move: **PALW V2 can be enabled only as one bundle whose invariants a
node checks before it dials a peer.** ADR-0039 D5e already refuses a starved share table at startup;
this generalizes that gate to the whole ruleset.

---

## Decision 2 — The block state machine, and why a fresh tip is weighable (closes P0-3)

Every PALW block moves through one lattice. Weight is a function of the state, and the state only
advances.

```
Provisional ──future anchor──▶ PanelBound ──quorum receipt──▶ ReceiptLicensed
                                                                   │        │
                                                       no dispute  │        │ fraud / withholding / timeout
                                                                   ▼        ▼
                                                                 Final    Voided ──▶ Slash
```

| State | live weight | safe weight |
|---|---|---|
| Provisional | bounded immature pwu (β·pwu, β ≤ 1000‰) | 0 |
| PanelBound | bounded immature pwu | 0 |
| ReceiptLicensed | bounded immature pwu | 0 |
| Final | full pwu | full pwu |
| Voided | 0 | 0 |

The audit's P0-3 is a structural cycle: the old weigher required a *future* panel anchor
(`accepted_daa + delta_bind`) to weigh a block, but a fresh tip has no block after it, so its anchor
is always `None`, so every live tip is `UnresolvedBlock`, so PALW weight is never consulted and fork
choice silently falls back to blue work. **The fix is that a fresh commitment is weighable
immediately as `Provisional`, deterministically, with no panel.** The panel is derived later, from
the anchor once it exists, and it gates only the `PanelBound → ReceiptLicensed → Final` transitions.

This is consistent with ADR-0039 §3e/§Consequences: a block's classification, once it reaches
`Final`/`Voided`, is permanent and unique; below that the stage may move freely, and
`advancing_a_block's_stage_never_lowers_either_weight` (β ≤ 1000‰ makes the `live→safe` jump
non-decreasing). ADR-0042 adds only the near end of the ladder — that a panel-less block is
`Provisional`, not `Unresolved`.

---

## Decision 3 — `PalwAttemptEnvelopeV2`, a new algo id, and identity by `attempt_id` (closes P0-1)

### 3a. The commitment binds to the PoW, or the PoW is free

P0-1: the live PoW path (`calculate_pow_layer0` over `pre_pow_hash, timestamp, nonce, network_id`)
does not consume the commitment root, while `PalwBlockCommitmentV1::l1_tag_bytes` — which *would*
bind it — is never called on the live path. So one PoW solution mints unlimited distinct block
identities by swapping the commitment. V2 makes the binding mandatory and total:

```
challenge        = H(network_domain ‖ pre_pow_hash ‖ timestamp ‖ nonce ‖ class_id ‖ bond_outpoint)
commitment_root  = H(challenge ‖ class_id ‖ bond_outpoint ‖ trace_root ‖ output_root ‖ pwu)
PoW L1 tag       = Expand(commitment_root)              // the finalizer CONSUMES this, not (pph,ts,nonce)
```

This is ADR-0039 Decision 4 made non-optional on the live path: one new ticket costs one new
inference (W2), and a commitment cannot be replayed onto another attempt, header, class or executor.
A consensus test fixes that **mutating any one bit of the commitment fails the PoW** (the exact
property `l1_tag_bytes`'s own unit tests assert, now enforced where blocks are validated).

### 3b. The unsigned attempt and its envelope

```rust
pub struct PalwAttemptUnsignedV2 {
    pub version: u16,
    pub network_domain: Hash,
    pub challenge: Hash,
    pub class_id: PalwClassId,
    pub executor_bond: TransactionOutpoint,
    pub executor_pubkey: MlDsa87PublicKey,   // MUST equal the bond record's key (Decision 6)
    pub operator_id: Hash,                    // registered at bond time; panel dedup (Decision 7)
    pub artifact_root: Hash,                  // MUST equal the class's registered root
    pub trace_root: Hash,
    pub output_root: Hash,
    pub pwu: u64,
}
pub struct PalwAttemptEnvelopeV2 {
    pub attempt: PalwAttemptUnsignedV2,
    pub signature: MlDsa87Signature,
}
```

### 3c. Block identity is `attempt_id`, not the raw signature — **deferred with cause (Amendments §A2)**

`block_id` incorporates `attempt_id = H(canonical(PalwAttemptUnsignedV2))`, **not** the signature
bytes. ML-DSA-87 signatures are not guaranteed unique, so folding raw signature bytes into the
identity would re-open malleability under the disguise of a fix: a second valid signature over the
same message would yield a second block id. The signature is verified as a *witness* at admission;
identity is `canonical header ‖ attempt_id`.

> **Amended at implementation (2026-08-20):** as written, this clause hands a third party a
> zero-cost censorship primitive and MUST NOT land naively — flip one signature bit and the block
> keeps its id but fails admission, so the first-seen mutated copy poisons the honest block's id
> in every known-invalid cache. The tree keeps raw-carrier-bytes identity (a mutated copy is a
> DIFFERENT id that dies alone; only the key holder can mint valid-signature siblings, and those
> deduplicate at the claim, `claim_id = attempt_id`). 3c lands only together with a
> mutated-witness path that rejects without caching id-invalidity. See §A2 for the full analysis.

> **Re-decided (2026-08-21), after the signature became verified.** The P0 fix (an attempt's
> ML-DSA-87 signature is verified on the live path) was read as removing this deferral's reason.
> It does not. The reason §A2 gives is the CENSORSHIP path, and verification is what arms it: an
> unverified witness could not fail admission, so the cache-poisoning needed the check to exist.
> **3c's identity half stays deferred, on the same precondition** — a pipeline path that rejects
> a witness-mutated carrier without marking the block id invalid.
>
> What P0 did change is the residual on the identity the tree keeps. Before it, ANY third party
> could mint valid-signature siblings from one solved PoW. Now only the bond holder can, and it
> can mint as many as it likes at one hedged signature each — which §A2 asserts is bounded ("DAG
> spam ..., never a second paid claim") and which nothing measured. It is measured now:
> `palw_v2_a_bond_holders_own_resignature_buys_a_block_but_never_a_second_claim` re-signs one
> attempt with different hedging randomness, gets a second valid block, and gets exactly one
> claim — because `claim_id = attempt_id` and the signature is deliberately outside it.
>
> Nothing enforceable was left on the table. A deterministic signature is not checkable from a
> public key; folding the signature into the PoW digest would cost one ML-DSA-87 signature per
> nonce of the class-ticket search; and excluding it from block identity IS 3c, whose blocker is
> the paragraph above. The byte-level malleability that would otherwise sit on top of the hedging
> is already closed: `decode_wire` refuses trailing bytes and `validate_shape_v2` pins the
> signature length.

### 3d. A new algo id, so no old node re-interprets a V2 block

V2 uses `POW_ALGO_ID_PALW_COMMITTED_V2`, distinct from `POW_ALGO_ID_PALW_LLM` (4) and
`POW_ALGO_ID_PALW_OLLAMA` (5). A pre-V2 node cannot mistake a V2 block for legacy algo-4. Because the
RC is a new genesis, no compatibility shim is owed.

---

## Decision 4 — The full node runs no model (closes W1 / the runtime half of P0-8)

A full node validates and adjudicates **without any LLM.** It checks:

```
canonical encoding · signature · attempt_id · ticket · bond state · class state
· PWU · exposure · panel · receipt · court proof · safe/live transitions
```

It never runs Qwen / Llama / Ollama / vLLM, never loads a model file, never re-executes an inference.
This is not a convention — it is a **compile-time boundary**:

- the `misakad` / `consensus` crates carry **no dependency** on any model-runtime crate;
- inference lives only in sidecars: `palw-worker` (execute, produce trace), `palw-signer`
  (bond key + anti-equivocation journal), `palw-watchtower` (re-execute, detect fraud, prosecute);
- **CI forbids** a `consensus`→runtime import edge (a test that greps the dependency graph, not a
  code comment);
- the "no runtime → `panic!`" paths are deleted. A full node without a model is the normal case, not
  a fault. (ADR-0036's `PalwWorkerFailed` retry belongs to the *producer* sidecar, not the validator.)

---

## Decision 5 — Candidate-scoped PALW state, and an authenticated commitment for pruning (closes P0-4, half of P0-5)

P0-4: today the weigher reads the node's current-sink mutable stores (`bond_view`, carriage,
capability), so two nodes with the same DAG but different applied history weigh the same candidate
differently — a permanent partition, and a direct violation of W3 (equal DAGs ⇒ equal weights).

V2 derives **every** weight input from the candidate chain point, never from the node's sink:

```rust
pub struct PalwChainStateV2 {
    pub state_root: Hash,
    pub bonds: BondStateRoot,
    pub reserved_exposure: ExposureStateRoot,   // Decision 6
    pub classes: ClassStateRoot,
    pub class_targets: ClassTargetRoot,         // per-class DAA
    pub capabilities: CapabilityStateRoot,
    pub claims: ClaimStateRoot,
    pub panels: PanelStateRoot,
    pub court_sessions: CourtStateRoot,
    pub safe_weight: U256,
    pub live_weight: U256,
    pub safe_frontier: Hash,
    pub epoch_counters: EpochCounterRoot,
}

fn apply_palw_transition(
    parent_state: &PalwChainStateV2,
    accepted_objects: &[PalwConsensusObject],
    current_attempt: &PalwAttemptEnvelopeV2,
) -> Result<PalwChainStateV2>;
```

The transition is built from the **same candidate-chain context and deterministic acceptance
ordering as UTXO validation** — never a separate "PALW-ish timeline." The invariant (ADR-0039 §3d,
made a gate here):

```
same genesis · same DAG · same block bodies · same evidence
    ⟹ identical palw_state_root, safe/live weight, and selected tip
```

independent of: block/evidence arrival order, prior sink, DB insertion order, restart count,
archival-vs-pruned, IBD start point, thread count, or ISA (x86-64/arm64). "Reading absent data as
nothing" is forbidden — a missing fact is an error, never a permissive zero.

For pruning (ADR-0041), the PALW state root, class state, bond exposure, unresolved court sessions,
safe frontier and unresolved claims are carried in the pruning proof. When a new header commits a
state root, the ADR that lands it **must spell out a hash ordering with no challenge↔commitment
cycle** — this is the one place the plan flags as "don't implement by vibes."

---

## Decision 6 — Admission split, and per-bond exposure (closes P0-2, P0-10)

Admission is two phases. **Stateless** (no chain state): version + canonical encoding, size bounds,
`attempt_id` recompute, signature verification **with the public key inside the commitment**, ticket
recompute, target check, domain/network separator. **Stateful** (candidate-chain snapshot):

```
1. bond outpoint is Active at the candidate chain point
2. the bond record's pubkey  ==  the commitment's executor_pubkey     ← closes P0-2
3. operator_id matches the bond registration
4. class is Active (not frozen)
5. artifact_root == the class's registered root
6. PWU is consistent with the class rules
7. within the class epoch budget (ADR-0039 D5, as a predicate on the PRODUCING block's own
   selected-chain class production — never the broken mergeset formulation of D5c)
8. within the bond's exposure ceiling                                  ← closes P0-10
9. the bond is not in retirement / withdrawal wait
10. no equivocation by the same bond
```

P0-2: today admission checks the signature's *length* and that the named bond is *Active*, but never
that the bond's holder *authorized* anything — so W8 ("no bond, no block") degrades to "write any
Active bond's name." Verifying the ML-DSA-87 signature under the bond record's key, before the
ticket check, restores it. (A fix along these exact lines is already in flight on `palw-only-v4`;
this ADR is where it becomes part of the atomic ruleset rather than a lone patch.)

P0-10: `Provisional`/`ReceiptLicensed` carry positive live weight, but nothing bounds how many
immature claims one bond backs, so a fake-root grinder can stack many claims before the first slash
lands — collateral is levered many times over. V2 reserves exposure per bond as a consensus fact:

```
reserved_exposure(bond)  =  Σ immature_claim.pwu × slash_value_per_pwu[class]
reserved_exposure(bond)  ≤  slashable_collateral(bond) × max_exposure_ratio
```

reserved at commitment admission (or at live-weight eligibility), released only on `Final`,
`Voided`, or `timeout`. Relay in-flight limits (ADR-0039 D6) stay as *supplementary* DoS control,
never the economic bound. And withdrawal is delayed past the whole liability period (the startup
invariant in Decision 1) so a bond cannot commit fraud and leave before it is provable.

---

## Decision 7 — Panel, data availability, and no-show (closes the wiring half of P0-7, DA half of P0-8)

The panel module (`palw_job_panel.rs`) already excludes the executor and dedups by bond outpoint;
P0-7 is the **caller** in the virtual processor passing `executor_bond_outpoint.transaction_id` as
the exclusion id, while candidates carry `validator_pubkey_hash` — different namespaces, so the
executor is never actually excluded, and `operator_root` is always `None`. V2 wires the caller to the
bond-registry-resolved identity:

- exclude executor's **bond**, executor's **validator key hash**, and executor's **operator_id**;
- `operator_id` is a **required** bond-registration field, DERIVED from an operator key
  (`palw_operator_id_v2(key)`), and every identity must carry the bundle's `min_collateral_sompi`
  — so splitting X collateral across bonds manufactures at most `⌊X / min_collateral⌋` panel
  identities. **Sybil is bounded, not prevented** (Amendments §A3); the original wording ("does
  not manufacture extra panel seats") overclaimed;
- an assigned seat that no-shows past its deadline is slashed / loses collateral, derived
  chain-scoped so reorg and replay reach the same verdict.

Trace data availability: the full trace is not on-chain, but the commitment binds
`trace_manifest ‖ chunk_merkle_root ‖ chunk_count ‖ retention_deadline`. A producer that fails to
serve a requested opening/chunk by the deadline **defaults**: claim void, bond slash — so silence
can never pin a block at `Provisional` forever. The four states — data obligation, receivability,
`unavailable` receipt, producer withholding — are kept distinct so a panel member is never punished
for data the *producer* hid.

---

## Decision 8 — The BASE-0 court is complete and proof-carrying (closes P0-8, P0-9)

At RC, the **only** class that carries weight is `PALW-BASE-0` (ADR-0039 D1a). An accelerated class
that is registered but has `coverage < 100%` or no independent verifier gets **share 0**.

P0-8: today the court fetches raw model rows from a `PalwWeightOracleV1`, but the full-node
production oracle `PalwNoWeightsV1` returns `None` for every row, so every arithmetic dispute is
`Unadjudicable`; swapping in a real oracle only where a node happens to hold the artifact makes
adjudication depend on local files — a partition. V2 makes the **evidence proof-carrying**: a fraud
proof includes the disputed step, opcode, pre/post-state roots, input operands, weight row,
quantization parameters, RoPE table slice, and Merkle proofs to the class artifact root and the
trace root. The full node re-runs one primitive on the CPU from the proof alone. `Unadjudicable` is
**removed as a reachable outcome** for BASE-0's op set.

P0-9: bisection is completed on both soundness and liveness —

1. session id includes the attempt id and both bonds;
2. the midpoint state is proven to lie inside the trace commitment (no responder-chosen interval);
3. each turn has a DAA-boundary deadline;
4. challenger timeout ⇒ default loss; responder timeout ⇒ default loss;
5. interval length 1 ⇒ terminal one-step adjudication (a terminal *opening* exists);
6. data-withholding ⇒ default;
7. round count is derived from trace length, **not fixed at 10**:

```
rounds  =  ceil(log2(max_step_leaf_count)) + terminal/opening rounds
```

and it is an **activation condition** that, for every registered class, the worst-case terminal
verdict fits inside the COURT window (Decision 1's amended `window_court ≥ worst-case court
duration` — see Amendments §A1: an open court suspends finality, so the court races its own
window, never the challenge window). The committed ladder-gap test on this branch
(`palw_schedule.rs`, commit `d1891333`) already measures that a 10-round ladder cannot reach the
pinned model; V2 sizes the ladder to the measured trace instead, and `a7be964e` closes the loop
against the catalog: `PalwCourtParamsV2::max_step_leaf_count` must cover
`PalwClassCatalogV2::max_step_leaf_count()`, so understating the ladder to shrink `window_court`
contradicts the catalog the same bundle commits to.

---

## Decision 9 — One fork-choice authority (closes the other half of P0-5)

A single pure comparator orders PALW candidates, and **every** chain-selection site calls it:

```rust
fn compare_palw_candidates(a: &PalwChainStateV2, b: &PalwChainStateV2) -> Ordering;
// 1. safe frontier   2. safe weight   3. live weight among same-frontier descendants   4. tie-break
// live_total = safe_weight + bounded immature weight  (so maturing never lowers the total)
```

Sites that MUST use it: virtual canonical tip, IBD-complete tip, pruning point, finality/deep-reorg
gate, restart recovery, sync-peer chain comparison.

P0-5: the header processor passes `palw = None` for both candidate and previous tip
(`header_processor/processor.rs:436-449` / ADR-0039 §3d names `:416` as the site), so the
header-selected-tip store stays blue-work-ordered even after the virtual sink starts using PALW
weight — two canonical-chain views inside one node. V2 forbids a *second* authority. A header-only
processor that lacks bodies and PALW state MAY use blue work as a **download-ordering hint**, but
that store is renamed `header_download_hint` (not `header_selected_tip`) and is never consulted for
chain authority, pruning, or finality.

**Amendment (audit 2026-09-02) — a site with no virtual sink is ordered at its imported tip.**
"IBD-complete tip" is a site with no sink to order: a STAGING consensus is built
`skip_adding_genesis`, so it holds no virtual state row and its `get_sink()` answers the all-zero
hash. Handing that to the comparator produced no order at all — and **no order is not an
abstention**. Both consumers refuse what they cannot weigh, deliberately, so that blue work cannot
install a chain through IBD; so a comparator with nothing to compare refused every headers-proof
sync on a ConsensusV2 network, against every peer, permanently, and the pruning-point PALW import
placed one line earlier "so the challenger becomes a real value" was dead work.

The rule is therefore stated positively: **a consensus is ordered at the deepest block whose PALW
state it has verified.** For a running node that is the virtual sink. For a staging consensus
mid-IBD it is the pruning point whose carriage it has just checked against the witness child's
header, which is the block the PALW tip row names — weighing it further forward is not available,
because the deltas the walk needs are written by virtual processing and staging has not run any.
This does not reintroduce the download hint as an authority: the fallback fires only where the
sink is not a block at all, and what it reads is the verified tip row. A tip snapshot that does
not load is not a weighing point either — a state fault is not a standing.

---

## Decision 10 — Reward is not spendable before `Final` (closes reward-before-void)

A `Provisional` block can still become `Voided`, so its PALW reward must not be spendable
immediately. V2 escrows it:

```
block accepted → reward escrow → claim reaches Final → spendable
```

Escrow is preferred over a fixed long coinbase maturity because each claim reaches `Final` at a
different point; a single maturity constant is either too short (spends voided reward) or needlessly
long for the common case. PALW reward is a **carve of the fixed subsidy** (ADR-0038 Decision G /
ADR-0035 §5's 62 % worker share shape), never an addition to it — the schedule is never exceeded
(I6/I15).

---

## Decision 11 — The ruleset fingerprint, committed to genesis (RC == mainnet, by hash)

RC-equals-mainnet is not a promise a human keeps; it is a hash a node checks.

```
palw_ruleset_id = H(
    protocol_version ‖ canonical_consensus_params ‖ class_catalog_root ‖ court_catalog_root
    ‖ trace_format_version ‖ signature_contexts ‖ fork_choice_version
)
```

`palw_ruleset_id` is committed to genesis. **The last RC and mainnet share the same
`palw_ruleset_id`.** Network identity lives in the challenge's `network_domain` (Decision 3a), so a
testnet block still cannot be replayed on mainnet even with an identical ruleset id. The P2P
handshake exchanges `network_id`, `palw_ruleset_id`, `class_catalog_root`, `court_catalog_root` and
drops a mismatched peer early. The mainnet binary is built from the **same git tag** as the final RC,
changing only network-scoped constants, and it reads the RC's canonical ruleset bytes rather than a
human re-typing parameters.

---

## The release gate — the audit's 12 conditions, as one checklist

No PALW-RC genesis and no mainnet activation until **all** hold. Each maps to the external audit's
"Activation の最低条件" and to a decision above.

| # | Condition | Decision | Closes |
|---|---|---|---|
| 1 | Ticket binding: commitment mutation ⇒ PoW failure | 3a | P0-1 |
| 2 | Signer authentication: commitment signature verified under the bond key | 6 | P0-2 |
| 3 | Fresh-tip semantics: a panel-less block is `Provisional`, deterministically | 2 | P0-3 |
| 4 | Candidate-chain purity: all facts from the candidate point, no sink reads | 5 | P0-4 |
| 5 | Single fork-choice authority across virtual/header/IBD/pruning/finality | 9 | P0-5 |
| 6 | Typed signature APIs (receipt/commitment/attestation/court contexts split) | 3, 6 | P0-6 |
| 7 | Proof-carrying court: operands + artifact proofs in the evidence | 8 | P0-8 |
| 8 | Complete bisection: midpoint verify, terminal opening, defaults, bond binding, depth | 8 | P0-9 |
| 9 | Collateral accounting: per-bond immature exposure ceiling | 6 | P0-10 |
| 10 | Panel enforcement: correct exclusion id, operator dedup, no-show penalty | 7 | P0-7 |
| 11 | Class lifecycle: Active/frozen/coverage, per-class retarget, redistribution, epoch budget | 1, 5 | W blockers |
| 12 | Determinism suite: prior sink / restart / IBD start / pruning point / store order invariant | 5 | P0-4 |

Additional (from the audit's §追加): **W1 runtime separation** (Decision 4) and **reward escrow**
(Decision 10) are release-blocking too.

---

## Implementation order — PR-00 … PR-10

Intermediate PRs are **never** enabled on a public preset. The public profile is added only in the
last PR.

| PR | Content | Done when |
|---|---|---|
| **PR-00** | this ADR, threat model, P0-reproducing red-test spec | the attack tests are red on current impl |
| **PR-01** | `PalwAttemptEnvelopeV2`, new algo id, canonical hash transcript | every field mutation invalidates the ticket |
| **PR-02** | full-node runtime-free admission | consensus build carries no model dependency (CI edge test) |
| **PR-03** | `PalwChainStateV2`, delta, state root | equal-DAG differential test passes |
| **PR-04** | bond-signature / class / PWU / epoch / exposure admission | foreign bond & over-exposure rejected |
| **PR-05** | worker / signer sidecars, anti-equivocation journal | fail-closed before a double-sign |
| **PR-06** | Provisional / panel / receipt / DA / no-show | a fresh tip is always weighable |
| **PR-07** | BASE-0 court, terminal bisection, default rules | intentional fraud is adjudicable |
| **PR-08** | safe/live fork choice, IBD, pruning unified | all subsystems pick the same tip |
| **PR-09** | per-class DAA, lifecycle, reward escrow | class stop/resume is deterministic |
| **PR-10** | atomic params, ruleset id, new genesis, public profile | PALW-RC boots from genesis |

**The next file to touch is not the five `None`s in `params.rs`.** It is PR-01. The public profile
is PR-10, and only after PR-00…PR-09 close.

---

## Amendments (2026-08-20, implementation-time)

Three clauses changed when the implementation tested them. Each is amended in place above and
recorded here with the reasoning, so the spec a reader audits is the spec the tree implements.

### A1 — The court races `window_court`, not the challenge window (Decision 1, Decision 8)

Original invariant: `challenge window > worst-case court duration`. That presumes finality RACES
an open court — that a claim could reach `Final` while its dispute is still being bisected, so the
challenge window had to be long enough to contain the whole prosecution. The implemented state
machine is stronger: **an open court session suspends `ReceiptLicensed → Final` outright**, so a
challenged claim cannot finalize early no matter how the windows compare, and what actually needs
bounding is the court against its own deadline. The binding inequality is therefore
`window_court ≥ (ceil(log2(max_step_leaf_count)) + terminal_rounds) × turn_deadline_daa`,
enforced at bundle construction (`PalwCourtParamsV2`, Decision 8's formula, overflow = refusal)
and cross-checked against the class catalog's real worst case (`verify_against_catalog`,
`a7be964e`). Implementing the original inequality was attempted and withdrawn: it added no safety
(the suspension already provides it) and taxed every honest, unchallenged claim with the
worst-case court duration for nothing.

### A2 — Decision 3c must not land without a mutated-witness path

With identity = `attempt_id` (signature excluded), any third party can flip one bit of a carried
signature and relay the result: same block id, invalid witness. The first-seen mutated copy fails
admission and the id lands in known-invalid caches — after which the HONEST block, arriving
second with the same id, is refused unseen. One flipped bit censors one block, network-wide, at
zero cost. (Bitcoin met this exact shape in its mutated-block/witness-malleation handling: reject
the copy without caching invalidity for the id.)

The tree therefore keeps **raw-carrier-bytes identity** for the `palw_commitment` field: a
mutated copy is a *different* id that dies alone at admission; valid-signature siblings of one
attempt are mintable only by the bond holder (ML-DSA-87 signing is hedged), share one PoW ticket,
and deduplicate at the claim (`claim_id = attempt_id`), so self-malleation buys DAG spam bounded
by the holder's own ban exposure, never a second paid claim. The PoW side of 3c is fully in force
— the signature is outside `attempt_id`, outside `commitment_root_v2`, and moves no digest
(`palw_v2_commitment_mutation_invalidates_pow` pins this). 3c's identity half lands only together
with a pipeline path that rejects witness-mutated carriers WITHOUT marking the block id invalid.

### A3 — Decision 7's Sybil guarantee, restated as the bound it is

Original wording: requiring `operator_id` at bond registration means "splitting collateral across
bonds does not manufacture extra panel seats." It does — up to a bound. What the implementation
provides (`4596a644`): `operator_id` is derived from an operator KEY, and every distinct identity
must independently carry `min_collateral_sompi`. Splitting X collateral therefore yields at most
`⌊X / min_collateral⌋` panel identities. That is a real economic floor per seat, and it is the
honest claim: **Sybil-bounded, not Sybil-proof.** Panel quorum math and slash sizing must assume
an adversary holds every identity their collateral can fund.

### A4 — the free panel re-roll is priced on BOTH lanes, and only one of them was

Decision 7's panel is drawn from an anchor the claim cannot choose, so there is nothing to shop
for at mining time. What remained was the *re-roll*: a producer that dislikes its drawn panel lets
the bind window lapse and commits again. On the ATTEMPT lane that is already expensive, and
measurably so (`abandoning_a_panel_costs_a_block_its_reward_and_its_epoch_budget`): the claim is a
BLOCK, so a re-roll is another solved PoW, the abandoned block's reward is forfeit, and the
class's epoch budget was spent at acceptance and is never refunded.

**All three of those costs rest on the same fact, and the free-prompt lane does not have it.** An
ADR-0044 commitment rides a transaction, not a block. Measured on the merged tree: after a
`BindTimeout` the reservation was released in full, no counter moved, no bond was debited, and the
next commitment was accepted in the very next block — a redraw priced at one transaction fee,
indefinitely repeatable.

The fix keeps an abandoned free-prompt claim's collateral RESERVED for
`PalwStateParamsV2::fp_abandon_hold_daa` after the void. Nothing is confiscated — declining to
bind is not an offence — the reservation is delayed, and every sompi returns when the span
elapses. What that buys is a denominator: N concurrent redraws need N × the reservation, so the
redraw rate is bounded by collateral. That is deliberately the same currency §A3's Sybil bound
speaks, so the two compose — an adversary's identity count and its redraw rate are both bounded by
the same X, and neither can be traded for the other.

`fp_abandon_hold_daa = 0` is the pre-FP configuration and leaves the attempt lane exactly as it
was, which is what every attempt-only fixture runs at.

---

## What this ADR does not decide

- **Measured mainnet parameters** — window sizes, share table, ρ_r, k, bonds, `base(C)` — are soak
  outputs (ADR-0036), filled into `PalwConsensusParamsV2` after Chaosnet + RC, in a later ADR.
- **The PWU derivation** (ADR-0039's open item) — still its own record; V2 requires it before any
  class carries weight.
- **Accelerated-class catalogs and second implementations** — BASE-0 is the only weight-bearing
  class at RC.
- **Operator items** — seeds, ports, public entry, faucet — are ADR-0035 §6 shaped and per-network.

## Number hygiene

This is ADR-0042; 0041 is the last committed. ADR-0036 records a same-day 0035/0036 collision from
two parallel sessions; if a concurrent branch also claims 0042, the tie is broken by keeping this
file's content and renumbering the later writer, per ADR-0036 Decision 5.

## Mission alignment amendment — 2026-10-07

atomic ruleset/fingerprintの規則は維持する。新しいpublic access、court受理、dispute-completeness gateは、それぞれ旧claimのreplayと明示的なversioned移行を必要とし、この文書で旧高さを付け替えない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0042-palw-mainnet-candidate-ruleset.md -->

</details>

<a id="source-0053-palw-one-execution-family"></a>

<details>
<summary>0053-palw-one-execution-family.md — 原文 L1–L323</summary>

[原文を開く](../adr/0053-palw-one-execution-family.md) · SHA-256: `33e4bf54ede94edfa12f03b068ae4763d1d51ce4174fffd8a1f3f541be08c3c4`

<!-- ADR_SOURCE_BEGIN:0053-palw-one-execution-family.md -->
~~~~~~~~markdown
# ADR-0053: One execution family — Family M is withdrawn, and the court is not optional

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Accepted.** Supersedes ADR-0051. Removes the Metal/GGUF execution family, the family
concept, the per-class panel, the runtime pins and the `misaka-palw-metal` crate. Moves
`PALW_STATE_V2_VERSION` (7 → 8) and the ruleset id, so testnet-11's consensus fingerprint moves and
every network re-mints.

Date: 2026-08-26

Relates to: ADR-0051 (**superseded**), ADR-0026 (whose thesis 0051 walked back for one family and
which is hereby restored in full), ADR-0038/0039 (PALW is the consensus work; no weight without a
complete catalog), ADR-0040 (the integer family that is now the only one), ADR-0045 (the share
table — a row of which 0051 reserved for this family), ADR-0049 (the adjudication contract the
withdrawn family deliberately never entered), ADR-0052 (`PALW-QWEN36`, the measurement that
removed 0051's motive), ADR-0034 (routing — its `Metal` family becomes reserved here).

---

## Context — the motive expired, and then the mechanisms turned out not to exist

ADR-0051 was an honest response to a real measurement. On 2026-08-22, making Qwen2.5-1.5B a
MISAKA-arithmetic class cost a canonical IR projection, an arithmetic canonicalization, a per-model
converter and a 1.7 GiB re-quantized artifact — and the result still produced a degraded argmax
(`[11,11,11,11]`), because static PTQ into BASE-0's int8 stream lost the model. The same checkpoint
in native GGUF ran at full quality at ~40 tok/s. From that, 0051 concluded that the lane whose job
is UX must ride on an existing runtime, commit to what the inference *said*, and verify by tolerant
replay — accepting, by name and capped at half the economy, a proof model that can never convict.

**That premise is dead.** ADR-0052 and the work that landed with it put Qwen3.6-35B-A3B's forty
layers — thirty GatedDeltaNet arms, ten gated-attention arms, a 256-expert mixture — through the
integer runtime, generating real text, with a kernel catalog covering **100 %** of what the graph
can reach and an adjudicator whose every arm calls the same function the engine calls. The model
the black box existed to serve is adjudicable without the black box. Whatever Family M was worth,
it is not worth it against a court-adjudicable class that runs the same checkpoint.

That alone would justify a re-scoping. What justifies a **withdrawal** is what an audit of the
shipped code found underneath ADR-0051's three safety claims.

### The 500‰ cap was never written

Decision 1's whole safety argument is that Family M's non-convictable work is bounded at half the
share table, "so the half that can convict a liar stays in charge of the tie".

`PalwClassAdmissionError::FamilyShareCap` exists as an error variant. **It has zero construction
sites.** Nothing in admission, in the genesis loader, in the share table or in the class DAA ever
computes a per-family sum or compares one to a cap. A Family-M class registered at any permille the
share rules allowed, and the bound that made the family acceptable was a doc comment.

This is the failure mode this repository has now recorded four times: *a gate that never fires is
more dangerous than no gate*, because the design reasons as though it fired.

It is currently harmless for one reason nobody designed: the acceptance gate forces a post-genesis
entrant's share to EQUAL `min_grantable_share_permille`, so no registration can reach a permille
worth capping. The cap is not load-bearing because a different rule happens to stand where it would
have stood.

### The per-class panel was checked and then ignored

Decision 6 called per-class panel parameters "the one structural consensus change this ADR
requires": a class could register `(seats, quorum)` thinner than the network's, floored by
`min_class_panel` in the bundle, so a Metal class could license with two seats instead of six.

Two things were true of the implementation:

1. **It was not scoped to the family.** `verify_class_admission_v2` calls `terms.panel_params(…)`
   at the top of the function, *before* any family dispatch. `min_class_panel: (2, 2)` shipped in
   `palw_fp_devnet_bundle_v3`, which every preset's params are built from. So on every shipped
   network, a **deterministic** class could also register a two-seat panel. A field added to let a
   black-box family exist quietly lowered the licensing bar for the family that can convict.
2. **It changed nothing anyway.** The panel a claim is actually bound with comes from
   `derive_panel_v2(state, panel_params, …)` in the virtual processor, and `panel_params` there is
   the node's bundle-global `PalwPanelParamsV2`. The class's registered `(seats, quorum)` never
   reaches it. A class admitted at 2-of-2 was still bound a 5-seat panel.

**These two failures are opposite, and filing them together would produce the wrong fix.**
`FamilyShareCap` is declared and never constructed — it *cannot fire*. `min_class_panel` is
enforced exactly as written — it *fires and lets the thing through*. One is a missing check; the
other is a working check whose rule is wrong. A reader who reaches for the same remedy for both
will fix neither. (The distinction is misaka-testnet-ed's, from an independent audit of the same
two mechanisms on the mainline and on the deployed t11 build.)

A ruleset field that weakened a gate and altered no behaviour is not a feature with a bug in it. It
is a schema cost paid for nothing, twice: `PALW_STATE_V2_VERSION` went 5 → 6 → 7 in a single day to
carry `terms` and then `runtime_pins`, for a family that never produced a block.

### The admission arm skipped every check that makes a class prosecutable

This is the strongest line in the case. `verify_class_admission_v2` dispatched a non-adjudicable
class to `palw_metal_class_admission_v1`, which:

* ran `validate_geometry` **instead of** `validate_shape`;
* skipped the **ADR-0038 A4 coverage gate** entirely — the gate whose whole purpose is refusing a
  class whose disputes would come back `Unadjudicable`, "rejected but unslashed, the hole a forger
  farms";
* skipped the **ladder-depth** check (`DeeperThanTheLadder`) and the **ADR-0049 Decision C court
  cost ceilings** (`CourtCostExceedsCeiling`);
* and wrote a `PalwClassCatalogEntryV2` whose `reachable_kernels` was `Default::default()` — the
  **empty set**. The class catalog root, which is inside the ruleset id, committed to a class the
  court knows nothing about.

Every one of those checks was written to refuse exactly the object this arm constructed. The
justification was internally coherent — "running them would refuse every Family-M class for failing
to be something it never claimed" — and that is precisely the argument that should have ended the
proposal rather than opened an arm around it. A class that cannot be prosecuted is not a class with
a different verification scheme. It is the thing the gates are for.

### And it could not have run — for a reason no operator could fix

The obvious version of this is **wrong**, and an earlier draft of this ADR published it: "the fleet
holds no Apple Silicon, so a family whose seats replay on Metal has nobody to seat." What
testnet-11 actually registered is not a Metal class. `682756bc…` is the **Linux CPU** build of the
pinned llama.cpp — commit `28f1f623` records the measurement ("the Linux CPU class is live at
682756bc…"), and the deployed worker reports `accelerate:false, avx2:true`. The family's identity
is its pins, not its silicon, so a CPU build qualifies; four fleet hosts carry that exact worker.
Replay capacity existed.

It still never produced a block on any chain, and the reason is a **fixed point in the class
identity** rather than a hardware shortage:

1. every Family-M execution goes through `MetalBackend::run_worker`, which invokes the worker at
   `--mode v2-legs-job`;
2. the deployed worker (built 2026-08-14) does not implement that mode. It refuses — measured still
   refusing, about twice a second, on 2026-08-26;
3. the repo's worker DOES implement it, and that is exactly why it cannot be deployed:
   `worker_binary_sha256` is inside the preimage of `PalwRuntimeManifestV2::manifest_hash()`, so a
   rebuild is a different runtime and `check_runtime_identity` refuses it for a class pinning the
   old manifest;
4. and the class id is the shape profile id, which does not move when the binary does — so
   registering the repaired worker collides with the class already there:
   `PalwStateV2Error::DuplicateClass`.

No configuration, rebuild or restart closes that loop. Only a re-mint does. A family whose sole
deployed class cannot be repaired without re-minting the network is not a family having an outage;
it is one whose identity scheme has no repair path — a structural argument for withdrawal, where
"we own no Macs" was a contingent one that a reader could answer by buying a Mac.

Two claims from the earlier draft are corrected rather than carried:

* **the panel arithmetic.** Quorum is **3 of 5**, not 5 of 5 (`PALW_V2_PANEL_SEATS = 5`,
  `PALW_V2_PANEL_QUORUM = 3`), and a drawable panel needs `SEATS + 1` distinct operators because a
  panel excludes the executor. Both are true of every class on the network, not of this family, so
  neither carries weight here;
* **the budget observation.** A class holding share while producing nothing was measured lying
  about its epoch budget for 33 hours — but the mid-epoch budget fix was **reverted on main**
  (`b57adc83`, "the mid-epoch budget fix rewrites history, so it cannot ship to a running chain"),
  and the class now reports `share=1 budget=1` and still produces nothing. It stands as a
  historical observation, not as a present-tense claim.

The economic point survives both corrections, in its narrow form: a share granted to a class that
produces nothing is not idle, because per-class DAA and epoch budgets reason about expected
production.

## Decision 1 — There is one execution family, and it is not a value

A class is verified one way: pinned integer arithmetic, a graph projected from a canonical IR,
disputes ending in the ADR-0049 court. This is **not** recorded as a single-variant enum. The type
`PalwExecutionFamilyV1` (the verification scheme, `consensus/core/src/palw_backend.rs` — distinct
from ADR-0034's routing family of the same name) and its `is_court_adjudicable()` are deleted.

Every registered class is court-adjudicable **by construction**, which is a stronger statement than
a flag that says so: a flag has an arm, and the arm the withdrawn family needed was the one that
skipped the coverage gate. A future second family is a new ADR that re-derives the whole question,
not a variant somebody adds to a live enum.

## Decision 2 — `PalwClassTermsV2` is deleted, and the class record shrinks

Gone from the chain: `terms.family`, `terms.runtime_pins` (`PalwRuntimePinsV2`), and
`terms.panel_seats` / `terms.panel_quorum`. Gone from the bundle: `min_class_panel` and the
`min_panel_seats` / `min_panel_quorum` accessors; gone from `PalwRegistrationTermsV2`, the two
fields a registrant read them through.

`PALW_STATE_V2_VERSION` goes **7 → 8**. A shrinking record forks a chain exactly as loudly as a
growing one, and ADR-0043 §2's change rule applies unchanged: the preimage moves, so the version
moves, and `the_version_8_state_root_golden_vectors` is where that is declared. The spec-side
second implementation of the state root moves with it — the correspondence is a round trip or it is
nothing.

## Decision 1a — What "one gate" does and does not claim, at genesis

Decision 1 is a statement about `verify_class_admission_v2`, the POST-GENESIS path: there is one
arm, and every registration that reaches it runs the whole of `validate_shape`, the ADR-0038 A4
coverage walk, the ladder-depth check and the ADR-0049 Decision C court-cost ceilings. Removing the
family removed the second arm; it did not make the genesis path identical to that one, and this
section says so rather than letting a reader assume it.

A **genesis** registration carries `admission: None`, because the ruleset id already commits to a
catalog describing the class. `verify_palw_genesis_v2` therefore checks the registration against
`PalwClassCatalogV2` rather than against a carried profile, and `verify_against_catalog` gates:

* the catalog is the one the ruleset root commits to;
* BASE-0 is present (the liveness floor is registered);
* **A4 coverage, for every entry** — so a genesis class whose graph reaches an uncatalogued kernel
  is refused, exactly as a post-genesis one is;
* **ladder depth** — `court.max_step_leaf_count() >= catalog.max_step_leaf_count()`.

It does **not** check the court-cost ceilings. `derive_court_cost_v1` appears nowhere in the
genesis path, so `max_opening_bytes` / `max_terminal_macs` / `max_operand_count` bound a
post-genesis entrant and not a class minted into a genesis. A class can therefore be seated at
block zero whose cheapest prosecutable step costs more than the ruleset allows — coverage-clean,
ladder-deep enough, and unpolicable, which is the exact condition ADR-0049 Decision C named.

Two further facts about the genesis path, stated so nobody has to rediscover them:

* the catalog's numbers (`reachable_kernels`, the two leaf counts) are **minted, not re-derived**.
  With no carried profile there is no graph to walk at load, so coverage is checked against the set
  the catalog asserts. Whoever mints a genesis is trusted to have derived it from the profile —
  which is why the mint path and the admission path must build entries with the same function, and
  why `verify_class_admission_v2` returns "what a genesis catalog would have held for this class";
* this is a **pre-existing** hole, not one this ADR opens. It is recorded here because Decision 1's
  "there is no arm that skips it" is true of the gate it names and would be false as a claim about
  every path a class can enter by. The fix — carrying the derived cost in the catalog entry so the
  bundle can check it the way it already checks depth — belongs to the class-catalog work, not to
  withdrawing a family.

## Decision 3 — One panel: the network's

Panel seats and quorum are bundle-global again. A class does not get to be licensed by fewer
operators than the network decided, and the network's decision is inside its ruleset id, where a
registrant cannot reach it.

## Decision 4 — `--palw-register-class` survives, pointed at a class the court can judge

ADR-0049 Decision H's post-genesis registration path is kept, because the thing it carries was
never family-specific: a profile, a canonical job, a bond's signature. What is removed is the only
builder it had — `family_m_post_genesis_registration_v1`, which could construct exactly the one
black-box class its own crate pinned.

Its replacement, `palw_post_genesis_registration_v1` (in `palw_class_admission_v2.rs`, beside the
gate that reads what it builds), is a generalization rather than a port: a deterministic class's
identity is its graph, which the caller already holds, so it builds a registration for **any**
profile. `kaspad --palw-register-class` now registers the class of the converted artifact the node
loaded, matched by shape against the build's own class registry. An operator who put an artifact on
disk has already said which class this node is for; a second declaration is a second place for them
to disagree.

Three things stay out of the caller's hands, for the reasons the withdrawn builder already gave:
the **share** (an entrant joins at the ruleset's minimum grantable permille), the **class id** (it
is the profile's id — a class IS its graph), and **`pwu_per_inference`** (counted from the canonical
job, so a declared value can only fail).

## Decision 5 — ADR-0034's `Metal` routing family becomes reserved

The routing family (`palw_routing.rs`, hardware) is a different type from the withdrawn
verification family, and ADR-0034 is not otherwise touched. But `Metal` was admissible there for
one reason — the withdrawn family was going to route to it, and a tolerant verifier does not need
Apple floats to be reproducible. Nothing routes there now.

A Metal binding today would have to be a deterministic-integer class executing on Apple GPUs. There
is not one line of GPU kernel in this tree, and no vendor guarantees the float reproducibility such
a class would have to be adjudicated against. `family_is_reserved_v1` therefore covers `Metal`
alongside `Cuda` and `Rocm`: it returns the way they do, through the ADR-0027 falsifiable
conformance campaign, not through a code edit. The discriminant stays `1` because it is on the wire.

## Decision 6 — What is deleted, in full

| | |
|---|---|
| `misaka-palw-metal` (crate, 1,538 lines) | `MetalBackend`, `CAT-M-0001`, the GGUF catalog, the pins, the workspace + `kaspad` dependency |
| `PalwExecutionFamilyV1` / `is_court_adjudicable()` | `consensus/core/src/palw_backend.rs`; `fn family()` leaves the backend trait |
| `PalwClassTermsV2`, `PalwRuntimePinsV2` | `consensus/core/src/palw_state_v2.rs`, and `terms` from `PalwClassStateV2` and `ClassRegistered` |
| `PalwClassAdmissionError::{FamilyShareCap, PanelTerms}` | the dead cap and the panel check |
| `palw_metal_class_admission_v1` | the arm that skipped `validate_shape`, A4, the ladder and the ceilings |
| `min_class_panel` + accessors, `PalwRegistrationTermsV2::min_panel_{seats,quorum}` | the ruleset field and its readers |
| `PalwProducerFactsV2::terms`, `PalwSeatDutyV2::terms`, `PalwCourtDutyV2::terms`, `PalwDisputableClaimV2::terms` | the family a producer, a seat and a challenger each had to dispatch on |
| `--palw-metal-worker`, `PalwBackendRegistry`'s family match | the CLI flag and the node-side dispatch |

`misaka-palw-worker` (the pinned llama.cpp build) is **kept**. It is not Family M; it is the runtime
the ADR-0034 capability probe interrogates, and no crate depends on it. Its build-time message and
the README no longer claim it serves a registered class, because it no longer does.

## Consequences

* **Every network re-mints.** The ruleset id moves (`min_class_panel` left the bundle) and the state
  root moves (version 8), so testnet-11's consensus fingerprint moves:
  `a708dc4a…` → `a1284a00…`. This is a coordinated upgrade — the fingerprint is what peers compare
  at the handshake, so old and new builds will not agree. Testnet-11 is the only preset that moves,
  being the only one carrying a PALW V2 bundle.
* **ADR-0026 is restored in full.** Its thesis — take Ambient's architecture, refuse its proof
  model, build exact-within-pinned-class — no longer has an exception carved into it.
* **The UX question is re-opened honestly.** ADR-0051 existed because a usable model in the
  deterministic family looked unaffordable. It is now affordable but not free: ADR-0052's class runs
  and is adjudicable, and its fidelity and speed are engineering work, not a proof-model problem.
  If a future measurement says a tolerant family is needed after all, it needs a new ADR that
  answers what 0051 did not: who enforces the cap, what the seats' hardware distribution actually
  is, and which gate refuses a class whose disputes cannot terminate.

## What this does NOT walk back

| | |
|---|---|
| Canonical IR / ADR-0049 | Untouched. The machinery of every class. |
| BASE-0 floor | Untouched. Liveness anchor, derived artifact, court intact. |
| ADR-0034 routing/bands | Kept, minus one admissible family (Decision 5). |
| ADR-0044 free-prompt lane | Untouched. The UX carrier is orthogonal to which family serves it. |
| ADR-0049 Decision H post-genesis registration | Kept, with a better builder (Decision 4). |
| ADR-0045 share table | Kept. What is removed is a cap that was never implemented, not the table. |

## Reading order for a reviewer

1. ADR-0051 — the proposal this withdraws; read Decisions 1, 4 and 6 against the Context above.
2. `consensus/core/src/palw_class_admission_v2.rs` — one gate, no second arm.
3. ADR-0052 — the class that removed 0051's motive.
4. `consensus/core/src/palw_backend.rs` — the seam that survived losing the thing it was built for.

## Mission alignment amendment — 2026-10-07

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0053-palw-one-execution-family.md -->

</details>

<a id="source-0180-palw-only-consensus-no-bft-validators"></a>

<details>
<summary>0180-palw-only-consensus-no-bft-validators.md — 原文 L1–L26</summary>

[原文を開く](../adr/0180-palw-only-consensus-no-bft-validators.md) · SHA-256: `96d22678d1fb7c078622b596812ac803f3704b13247071521e0a6965b92ea809`

<!-- ADR_SOURCE_BEGIN:0180-palw-only-consensus-no-bft-validators.md -->
~~~~~~~~markdown
# ADR-0180 — PALW のみの合意:BFT の validator を使わない

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

* **Status:** Accepted(ユーザー決定 2026-10-16)。RFC-0012 の採用理由を記録する。
* **Related:**
  * [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md):PALW のみの合意と native EVM 決済。DNS validator・attestation・precommit・DNS-final・stake による reorg 拒否権を退役する。
  * fence `palw_dns_retirement_v1`:DNS-BFT の退役。RFC-0012 の方式 (b)。
  * [ADR-0178](0178-fork-choice-heals-a-partition-on-bonded-participation.md):bond 参加による fork choice。
  * [PRINCIPLES](../PRINCIPLES.md)。
* **Supersedes(合意の役割として):** ADR-0009〜0018 の DNS validator・確率的 finality・validator 報酬・stake attestation の系統。過去の記述は履歴として残す。

## 決定

**MISAKA は、BFT を使わない新しい proof として PALW を提唱する。** 合意は PoW(DAG)と PALW の有用計算・bond・公開証拠による裁定で成り立ち、BFT の validator 集合に finality や reorg の権限を与えない。

**validator を無くした理由:**
- **validator による先行者利益を無くすため。** BFT の validator 集合は、早く参加した者や初期の割当てを持つ者が、継続的に finality の権限と報酬を握る構造になりやすい。
- **運営母体を無くすため。** validator の選定・鍵・運用を担う主体は、実質的な運営母体になる。その主体に合意の権限が集まることを避ける。
- **より分散された構造を求めるため。** 参加は bond と実際の計算(PALW)によって誰にでも開かれる。権限は特定の集合ではなく、公開された規則と証拠に置く。

## 帰結

- DNS-BFT は `palw_dns_retirement_v1` で退役する。新しい規則は DNS-BFT に依存しない。t12 の履歴を読むための legacy reader は残す。
- 合意 credit の認証(RFC-0018 Gate C / `SecurityCertified`)では、抽選された検証者の承認を、正直な割合という明示した前提の下で用いる。抽選は claim ごとに行い、固定の validator 集合に権限を与えるものではない。
- 計算の正しさは、確率的な検出、公開証拠による局所化、決定論的な裁定、経済的な抑止で担保する(PRINCIPLES)。暗号学的な計算証明は使わない(PRINCIPLES §8)。
~~~~~~~~
<!-- ADR_SOURCE_END:0180-palw-only-consensus-no-bft-validators.md -->

</details>

