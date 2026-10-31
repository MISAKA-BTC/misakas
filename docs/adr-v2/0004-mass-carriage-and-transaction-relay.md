# ADR-v2-0004 — Mass・DoS上限・合意object搬送・再送

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 4本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0007](../rfc-v2/0007-claims-exec-and-answer-accounting.md) / [RFC-v2-0009](../rfc-v2/0009-remote-client-and-sdk.md) / [RFC-v2-0010](../rfc-v2/0010-dag-consensus-and-native-settlement.md)

## 1. この文書の役割

transactionの資源価格とPALW objectの搬送・再送を、受理時の責務としてまとめる。

## 2. 統合した判断の読み方

### 2.1

Massはbyte・script・sigopの費用を分け、署名方式の変更後の実測・上限と整合させる。初期のML-DSA-65用定数をML-DSA-87へ無条件に流用しない。[旧ADR-0005](../adr/0005-mass-policy.md) [旧ADR-0019](../adr/0019-mldsa87-migration.md)

### 2.2

登録は対応するcollateralと、裁定objectは対応する証拠と結び付けて搬送する。subnetwork、codec、hash domainと適用版を保存する。[旧ADR-0029](../adr/0029-palw-chain-carriage.md) [旧ADR-0046](../adr/0046-palw-v2-consensus-object-carriage.md)

### 2.3

未収録transactionの再announceと、合意上の一回だけの受理・効果を分ける。再送で金融効果やclaimを増やさない。[旧ADR-0115](../adr/0115-a-pending-transaction-is-announced-until-it-lands.md)

## 3. 維持する境界・後続改定

ADR-0029の段階案とADR-0046のV2搬送を時系列で読む。本文のDraftは統合によって公開wireとして採択されない。

## 4. 隣接する判断との分担

受理後のclaim意味論は[ADR-v2-0027](0027-claim-lifecycle-and-liability.md)、資金契約は[ADR-v2-0037](0037-fee-escrow-and-role-consent.md)、Court固有のmaterial入口は[ADR-v2-0045](0045-terminal-material-and-readiness.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0005-mass-policy.md](../adr/0005-mass-policy.md) | L1–L267 | L12–L15 |
| [0029-palw-chain-carriage.md](../adr/0029-palw-chain-carriage.md) | L1–L273 | L15–L27 |
| [0046-palw-v2-consensus-object-carriage.md](../adr/0046-palw-v2-consensus-object-carriage.md) | L1–L201 | L12–L19 |
| [0115-a-pending-transaction-is-announced-until-it-lands.md](../adr/0115-a-pending-transaction-is-announced-until-it-lands.md) | L1–L86 | L14–L24 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0005-mass-policy"></a>

<details>
<summary>0005-mass-policy.md — 原文 L1–L267</summary>

[原文を開く](../adr/0005-mass-policy.md) · SHA-256: `4a673df180c3d9524db66488410e0160ef39bf5c275d4fa89b529960cc767d1c`

<!-- ADR_SOURCE_BEGIN:0005-mass-policy.md -->
~~~~~~~~markdown
# ADR-0005: Mass / DoS policy for ML-DSA P2PKH transactions

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: Accepted — **PR-19-S7 (Phase 7) recalibrated `mass_per_sig_op = 10_000`
for ML-DSA-87** (Phase 1 froze the shape; the earlier ML-DSA-65 PoC set 6000)
Date: 2026-05-28 (rev 2026-05-31)
Supersedes: —

## PR-19-S7 (Phase 7) recalibration — ML-DSA-87 (supersedes the ML-DSA-65 result below)

The kaspa-pq signature migrated from ML-DSA-65 to **ML-DSA-87** (ADR-0019).
ML-DSA-87 `verify` is meaningfully slower, so the sigop weight was re-measured
and raised. Same harness (`crypto/txscript/benches/bench.rs`, now over
`ml_dsa_87::verify`), same reference hardware (Apple Silicon arm64), same
"pin against the slowest variant" rule:

| Primitive | Variant | Median |
|---|---|---|
| `secp256k1::schnorr::Signature::verify` | (single impl) | **12.74 µs** |
| `libcrux_ml_dsa::ml_dsa_87::verify`     | default (NEON/AVX2 multiplexed) | **63.88 µs** |
| `libcrux_ml_dsa::ml_dsa_87::portable::verify` | portable, no SIMD | **76.52 µs** |

Ratios:

- ML-DSA-87 default / Schnorr = 63.88 / 12.74 = **5.01×**
- ML-DSA-87 portable / Schnorr = 76.52 / 12.74 = **6.01×** ← slowest

Calibration formula (unchanged):

```
1000 (upstream mass_per_sig_op)  ×  6.01 (slowest ratio)  ×  1.59 (safety)  =  9548  →  10_000
```

→ kaspa-pq `mass_per_sig_op = 10_000` (rounded up to the nearest 1000, matching
the ML-DSA-65 convention; effective safety factor becomes 10000 / (1000 × 6.01)
= **1.66 ≥ 1.5**), locked across all four `*_PARAMS` constants in
[consensus/core/src/config/params.rs](../../consensus/core/src/config/params.rs).
The earlier ML-DSA-65 value of 6000 encoded a 6.0× multiplier (3.78 × 1.59),
which is *below* the bare ML-DSA-87 portable ratio (6.01×) — i.e. ML-DSA-87 with
6000 had **no** safety margin, which is why the bump was required.

Wallet-side consequence: raising the sigop weight shrinks the tx-generator's
per-relay input batches (≈16 → ≈10 inputs on testnet-10), so the
`kaspa-wallet-core` generator tests were re-recalibrated in the same PR.

Pre-mainnet reinforcement (unchanged intent): re-run the bench on the production
low-end reference image and compare its portable median against 76.52 µs; bump
again if it exceeds.

## (historical) Phase 6 calibration result — ML-DSA-65 PoC, superseded by the above

## Phase 6 calibration result

Measured on the reference hardware (Apple Silicon arm64) via
`crypto/txscript/benches/bench.rs`. The bench exposes three variants so
the calibration can pin against the slowest:

| Primitive | Variant | Median |
|---|---|---|
| `secp256k1::schnorr::Signature::verify` | (single impl) | **12.71 µs** |
| `libcrux_ml_dsa::ml_dsa_65::verify`     | default (NEON multiplexed) | **40.75 µs** |
| `libcrux_ml_dsa::ml_dsa_65::portable::verify` | portable, no SIMD | **48.02 µs** |

Ratios:

- ML-DSA default / Schnorr = 40.75 / 12.71 = **3.21×**
- ML-DSA portable / Schnorr = 48.02 / 12.71 = **3.78×** ← slowest

The kaspa-pq policy calibrates against the slowest variant so that
no-SIMD low-end reference platforms remain safely budgeted:

```
1000 (upstream mass_per_sig_op)  ×  3.78 (slowest ratio)  ×  1.59 (safety)  ≈  6000
```

→ kaspa-pq `mass_per_sig_op = 6000`, locked in
[consensus/core/src/config/params.rs](../../consensus/core/src/config/params.rs)
across all four `*_PARAMS` constants. Reinforcement on a true low-end
cloud instance (e.g. a single-vCPU bursty VM with no SIMD enabled in
the OS) before mainnet launch may further tighten — re-run the bench
above on the production reference image and compare its portable
median against 48.02 µs; bump if it exceeds.

## Context

The upstream `Params` defaults (verified in
[consensus/core/src/config/params.rs](../../consensus/core/src/config/params.rs)):

```text
max_signature_script_len: 10_000
max_script_public_key_len: 10_000
mass_per_tx_byte:          1
mass_per_script_pub_key_byte: 10
mass_per_sig_op:           1000
max_block_mass:            500_000
```

These constants were tuned for Schnorr-secp256k1 inputs (~64-byte
signatures, ~32-byte public keys) and ECDSA-secp256k1 inputs (~71-byte
signatures, ~33-byte public keys). The 1000-mass-per-sigop weight was
calibrated against the cost of a secp256k1 verify.

ML-DSA-65 changes both the size and the cost of a "sigop":

- Signature ~3309 bytes (+1 sighash byte).
- Public key 1952 bytes.
- A full signatureScript is ~5267 bytes.
- A full input (outpoint + script + sequence) is ~5319 bytes.
- ML-DSA-65 `verify` is meaningfully more expensive than
  secp256k1 Schnorr `verify`, in the high-tens-of-µs range
  depending on platform and AVX2 availability.

Two failure modes if we ship upstream defaults unchanged:

1. **Byte-mass underweight on inputs.** A block packed with
   ML-DSA P2PKH spends has ~94× more byte mass per input than the
   upstream secp256k1 case. Even at `mass_per_tx_byte = 1`, this
   eats `max_block_mass` quickly — but not quickly enough to bound
   the verify time per block.
2. **Sigop-mass underweight on inputs.** A naive attacker constructs
   a block with as many ML-DSA verifies as fit, then verify time
   blows past the 100ms block budget.

## Decision

Phase 1 freezes the **shape** of the mass policy; the **values** are
measured and frozen in Phase 6.

### Frozen shape

- The existing `mass = byte_mass + script_pub_key_mass + sigop_mass`
  formula is preserved.
- A new `mass_per_sig_op` value is calibrated for ML-DSA-65 verify.
- `MAX_SCRIPT_ELEMENT_SIZE` is widened to `4096`. Keeping the
  upstream `520` is incompatible with a 3310-byte signature push.
- `max_signature_script_len = 10_000` is preserved as a first
  approximation. It admits exactly one ML-DSA-65 P2PKH input with
  slack. Multi-sig signatureScripts are not yet allowed (see
  [ADR-0002](0002-mldsa65-p2pkh.md)).
- `max_script_public_key_len = 10_000` is preserved.

### Calibration formula

```
new_mass_per_sig_op =
    ceil(
        old_mass_per_sig_op
          * median_mldsa65_verify_time
          / median_schnorr_verify_time
          * safety_factor
    )
```

- `old_mass_per_sig_op = 1000`.
- `median_*_verify_time` measured on devnet/simnet on at least two
  reference hardware classes (one "modern desktop", one "low-end
  cloud instance").
- `safety_factor ≥ 1.5`.

### Pre-verify rejection rules

Before any ML-DSA `verify` call, the script engine **must** reject:

- public-key item whose length ≠ 1952,
- signature item whose length ∉ {3309, 3310},
- non-canonical push (the upstream "minimal push" rule applies
  unchanged),
- script-element length > `MAX_SCRIPT_ELEMENT_SIZE`,
- script size > `MAX_SCRIPTS_SIZE`,
- stack depth > `MAX_STACK_SIZE`.

Rejection here is cheap. The cost we are budgeting against is the
ML-DSA `verify` itself; any failure that does not actually exercise
`verify` must not consume sigop-mass.

### SigCache shape

`Mldsa65SigCacheKey` (see `kaspa-pq-spec.md` §7) is mandatory. The
cache must not hold raw 1952-byte / 3309-byte material by value.

## Consequences

### Positive

- The mass formula keeps the same overall structure as upstream, so
  the existing fee-estimation, block-template, and mempool-eviction
  logic continues to work with only the constants changed.
- Pre-verify length and shape checks bound the cost of a
  malformed-tx flood, because they reject without entering ML-DSA.

### Negative

- We cannot ship a number for `mass_per_sig_op` from Phase 1; the
  Phase 6 benchmark gates the value. Until then, the PoC runs with
  upstream `1000` and we accept that the simnet is dramatically
  over-budget on verify cost. This is only acceptable because the
  PoC simnet is not adversarial.
- A single calibrated value will not be optimal across all
  hardware. We pick the lowest common denominator (slowest of
  the reference platforms) so that the mass cap protects honest
  validators on slow boxes.

### Neutral

- The block-template builder will likely produce blocks with fewer
  inputs than the byte budget alone would allow, because the new
  sigop weight will dominate. This is intentional.

## Alternatives considered

1. **Keep `mass_per_sig_op = 1000`.** Rejected: under-budgets ML-DSA
   verify by an order of magnitude.
2. **Use a separate "pq sigop" counter with its own cap, in addition
   to byte and script-pubkey mass.** Rejected for the PoC: adds a
   third axis to the mass formula and a new acceptance condition
   in the validator. Revisit if Phase 6 finds that the byte-budget
   and sigop-budget cannot be reconciled with a single
   `mass_per_sig_op`.
3. **Drop `mass_per_sig_op` and price purely by byte mass.** Rejected:
   verify time and tx size are correlated but not perfectly, and the
   correlation breaks under adversarial inputs (e.g. an input that
   pre-fails the cheap length checks would be over-priced).

## Implementation notes for Phase 6

Bench harness location: `consensus/core/benches/mldsa65_verify.rs`
(to be created in Phase 6). It must:

- Use Criterion or equivalent stable-noise bench harness.
- Report median, p95, p99 for `verify`.
- Run with and without AVX2 / NEON, recording the slower number as
  the calibration baseline.
- Run on at least one "low-end" reference image.

Outputs:

- Phase 6 PR sets `mass_per_sig_op` in
  [consensus/core/src/config/params.rs](../../consensus/core/src/config/params.rs).
- Phase 6 PR re-runs `max_block_mass` headroom math and tightens
  if needed.
- Phase 6 PR updates `kaspa-pq-spec.md` §6 with the locked-in values.

## Acceptance criteria (Phase 6)

1. `mass_per_sig_op` is set from a measured ratio with safety
   factor ≥ 1.5.
2. Mempool admission survives a flood of 10× normal traffic in
   malformed ML-DSA signatures without exceeding O(traffic)
   verify-time.
3. A full-tx ML-DSA verify is exercised exactly once per accepted
   input (no double-verify due to a sigcache miss path).
4. Block-template generation latency stays inside the
   per-block budget at the calibrated `mass_per_sig_op`.

## References

- Upstream [consensus/core/src/config/params.rs](../../consensus/core/src/config/params.rs).
- [ADR-0002 — ML-DSA-65 P2PKH](0002-mldsa65-p2pkh.md).
- `libcrux-ml-dsa` 0.0.9 docs.rs (verify portable/AVX2 split).
~~~~~~~~
<!-- ADR_SOURCE_END:0005-mass-policy.md -->

</details>

<a id="source-0029-palw-chain-carriage"></a>

<details>
<summary>0029-palw-chain-carriage.md — 原文 L1–L273</summary>

[原文を開く](../adr/0029-palw-chain-carriage.md) · SHA-256: `fabb7c65451f3adf213519bc2d3e5e4abae4aaf6999fc2fb6e11b8ce6ff821fa`

<!-- ADR_SOURCE_BEGIN:0029-palw-chain-carriage.md -->
~~~~~~~~markdown
# ADR-0029: PALW chain carriage — the objects ride the rails the fork already built

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Proposed (draft for review).** Activates nothing. This ADR decides how ADR-0028's
objects — job commitments, attestations, opening calls and answers, refutations — become
on-chain facts, in two stages whose first requires **zero node changes** and realizes the
Stage-0 telemetry (`P_check`, no-show, inclusion latency) as measurements of real chain events.
Date: 2026-08-16
Relates to: ADR-0028 (the scheduling fabric this carries; its Consequences named this gap),
ADR-0027 (evidence objects and DA rules), ADR-0026 §12 (the gate artifacts Stage 0 fills),
ADR-0009/0016/0017 (the DNS overlay carriage discipline this reuses),
`consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs`
(`check_transaction_subnetwork` — the admission gate), `consensus/core/src/dns_finality.rs`
(`dns_tx_kind`, the stateless validators), `consensus/src/pipeline/virtual_processor/processor.rs`
(the capability store's accept/revert/backfill walk — the Stage-1 template),
`consensus/core/src/palw_legs.rs` / `palw_slash.rs` / `palw_v2.rs` (the wire bodies, all frozen).

## Facts this design stands on (verified in code, 2026-08-16)

1. **The fork has an established carriage discipline, live on t10 today**: one dedicated
   20-byte `SUBNETWORK_ID_*` per payload kind, a Borsh body in `tx.payload`, **stateless**
   validation at admission (`dns_tx_kind` → per-kind validator → `InvalidDnsOverlayPayload`),
   **stateful** checks in the virtual processor's credit walk, and a store with
   accept/revert-on-reorg plus a horizon-bounded history backfill
   (`compute_capabilities_with_ids_from_accepted_txs` + `verified_capability`). Eleven kinds
   already ride this way, including the v1 compute lifecycle.
2. **An unknown subnetwork id is rejected at admission** (`SubnetworksDisabled`). New ids are
   a coordinated release — the token band's comment states the pattern explicitly. Therefore
   new-id carriage cannot reach today's deployed fleet.
3. **A native-subnetwork transaction may carry a payload today** — asserted by the isolation
   test (`tx.payload = vec![0]` → `Ok`), with **no payload inspection at admission and no
   payload rule in mempool standardness**. Mass is the only bound.
4. **The budgets**: `mass_per_tx_byte = 1`, `max_block_mass = 500_000`, standard-tx mass cap
   `480_000` (`MAXIMUM_STANDARD_TRANSACTION_MASS`, this fork's value). ML-DSA-87 signature
   = 4 627 B.
5. **The observation surface exists**: virtual-chain-changed RPC with
   `include_accepted_transaction_ids` — added and removed chain blocks with their accepted
   transactions. A watcher can maintain a selected-chain view of carried objects without any
   node change.
6. **One evidence object does not fit**: a bare-v2 logits-event refutation carries a full
   vocab row — `248 320 × 4 B ≈ 0.99 MiB` (the number `palw_slash` itself documents) —
   exceeding both the standard cap and the block mass. §6 refuses to hand-wave this.

## Premises

P1–P3 and the ADR-0028 corollary are inherited. Two carriage-specific rules join them:

* **A payload a deployed node rejects at admission is not carriage, it is a fork.** Never
  retrofit new versions or shapes into an already-deployed subnetwork's stateless validator —
  a deployed validator that rejects the new version keeps the tx out of blocks entirely. New
  shapes get new ids, shipped together with their validators (the version trap, named).
* **Offense-grade facts require offense-grade carriage.** An objective offense (no-show,
  `W_answer` silence) may only be grounded in objects every consensus-running node indexed
  identically — i.e. Stage-1 subnetwork carriage. Stage-0 native carriage produces
  *measurements*, never offense evidence against third parties.

## Decision

### 1. Two stages, one body format

| | Stage 0 (now, zero release) | Stage 1 (coordinated release) |
| --- | --- | --- |
| Vehicle | native subnetwork, payload = `"MPALW2" ‖ kind u8 ‖ borsh body` | one new `SUBNETWORK_ID_PALW_*` per kind |
| Admission | none (native payloads are opaque to consensus) | stateless per-kind validator, `InvalidDnsOverlayPayload` on failure |
| Indexing | external watcher over the RPC acceptance stream | in-node store: accept/revert/backfill, the capability-store walk verbatim |
| Grounds | telemetry only (§12 artifacts) | duties, deadlines, objective offenses; later the §1 credit gate |
| Who can act on it | the fleet drill | every consensus-running node |

**The Borsh bodies are identical in both stages.** Migration is a change of address, not of
format: the Stage-0 magic envelope is dropped and the body moves onto its subnetwork id.
Bodies never embed their carriage (no subnetwork id, no magic inside the signed material), so
a Stage-0 object and its Stage-1 twin hash and verify identically.

### 2. The five kinds and their bodies

Every body reuses a frozen wire object; carriage adds only identity and binding — never a
second copy of anything the inner object already commits to (the dual-source rule).

```
kind 0x01  PalwCommitmentCarriageV1 {
             version u16,
             envelope: PalwJobEnvelopeV2,          // full input: replays are self-contained,
                                                   // so ADR-0028 §3's input-DA objection is
                                                   // unreachable for carried jobs
             committed_form: u8,                   // 0 = bare v2 root, 1 = execution composite
             committed_root: Hash64,
             binding: Option<PalwLegsBindingV1>,   // required iff composite: the transparent
                                                   // preimage refuters open against
             validator_id: Hash64, bond_outpoint: TransactionOutpoint,
             signature: Vec<u8>,                   // ML-DSA-87 over a carriage-domain digest
           }
kind 0x02  PalwAttestationCarriageV1 {
             version u16,
             commitment_root: Hash64,              // what §1's credit gate matches against
             attestation: PalwExecutionAttestationV1,  // palw_slash, unchanged — its message
                                                   // golden 9fb7e41e… stays the signed digest
             attester_id: Hash64, bond_outpoint: TransactionOutpoint,
           }
kind 0x03  PalwOpeningCallCarriageV1   { version u16, call: PalwLegsOpeningCallV1 }
kind 0x04  PalwOpeningAnswerCarriageV1 { version u16, call_tx_id: TransactionId,
                                         answer: PalwLegsOpeningAnswerV1 }
kind 0x05  PalwRefutationCarriageV1    { version u16, evidence: enum {
                                           Legs(PalwLegsRefutationV1),
                                           Summary(PalwTraceSummaryRefutationV1),
                                           Event(chunked — see §6),
                                         } }
```

Rules carried over from the fork's own precedents:

* **Evidence carriers declare no outputs** (kinds 0x05, and 0x03 if a fee-bond ever pays out
  at `(tx_id, 0)`) — the slashing/challenge/precommit-evidence rule, adopted so the Stage-2
  reporter-reward slot is never a retrofit.
* **Dedup**: store rows are keyed by carrying `tx_id` (revert-friendly, the capability-store
  key); logical identity is first-accepted-wins — `committed_root` for commitments,
  `(commitment_root, attester_id)` for attestations, `call_tx_id` for answers. `commit_daa`,
  `attest_daa`, call and answer times are the acceptance DAA of the first accepted carrier on
  the selected chain — the same clock `PalwShadowLedgerV1` already consumes.
* **Reorg**: Stage 1 inherits the accept/revert walk; Stage 0's watcher mirrors it in
  userspace from the RPC stream's removed/added chain blocks. ADR-0028 §2's re-anchor rule
  applies unchanged.

### 3. Mass budget — every kind sized against the real constants

At `mass_per_tx_byte = 1` against the 480 000 standard cap (block cap 500 000):

| object | size (est.) | fits? |
| --- | --- | --- |
| commitment, bare form | ≈ 7.8 KB (envelope ≈ 2.9 K + identity + 4 627 B sig) | ✓ |
| commitment, composite form | ≈ 9.1 KB (+ binding ≈ 1.3 K) | ✓ |
| attestation | ≈ 5.0 KB | ✓ |
| opening call (≤ 16 coordinates) | ≈ 3.3 KB | ✓ |
| opening answer, 16 activation openings | ≈ 152 KB (16 × ≈ 9.4 K + binding) | ✓ |
| legs refutation (one activation leaf) | ≈ 15 KB | ✓ |
| summary/schedule refutation | ≈ 2–3 KB | ✓ |
| **bare-v2 logits-event refutation** | **≈ 0.99 MB** | **✗ — twice the block mass** |

Consequently the **carriage cap for openings is 16 per call** (wire cap stays 64;
`PALW_LEGS_MAX_REQUESTED_OPENINGS` is unchanged — a request wanting more splits into several
calls, each with its own `W_answer`). The cap buys margin, not bare feasibility: 16 × ≈ 9.4 KB
≈ 152 KB is 3.2× under the standard cap, while the wire-cap 64 (~600 KB) exceeds it outright
and even 48 (~451 KB) would ride with no room for anything else in the transaction or the
block it shares.

### 4. Time semantics

An object's protocol time is **the acceptance DAA score of its first accepted carrier on the
selected chain** — observable at Stage 0 through the RPC acceptance stream, stored at Stage 1
exactly as `accepted_daa_score` already is for capabilities. Mempool time, broadcast time and
block timestamps are not protocol facts. `W_answer(call)` runs from the call carrier's
acceptance to the answer carrier's acceptance; inclusion latency — the ADR-0028 assumption-2
telemetry — is `acceptance_daa − first_broadcast` as measured by the submitting watcher (the
broadcast half is watcher-local by nature and is labeled as such in artifacts).

### 5. Stage 0 realization — the drill that fills the §12 artifacts

No node changes. Three pieces, all external:

```
submitter  (fleet hosts)   every N minutes: run a v2 job (bench path), wrap the result as
                           kind 0x01, submit via RPC as a native-subnetwork tx with the magic
                           payload; panels derived per select_replay_panel_v1 over the four
                           bonded validators; assigned hosts replay and submit kind 0x02
watcher    (host B)        subscribes virtual-chain-changed with accepted tx ids; maintains
                           the selected-chain object view (apply added / roll back removed);
                           classifies duties against job_schedule_v1; feeds
                           PalwShadowLedgerV1::observe_job
reporter   (host B)        publishes the ledger report per class — jobs, creditable, on-time
                           matches, no-shows, mismatches, attest/refutation inclusion
                           latency, replay p99 — THE §12 artifacts, now from chain events
```

Drill discipline: induced negatives are part of the schedule (a host stays silent on an
assigned duty once per session; one job is answered past `W_answer`) so the no-show and
late columns are exercised, not vacuously zero. A deliberately mismatched attestation may be
drilled ONLY against a job whose **envelope carries a dedicated drill `network_id`** — the
attestation itself carries no network id by design, but everything signed about a job is
bound through its `job_context_hash`, so the drill namespace scopes the contradiction. A
signed mismatch on the production namespace is `ClassContradictionCertificateV1` material
and must never be manufactured.

Fees: plain transaction fees from the fleet wallet. Fee-bonded calls (ADR-0028 §5's DoS
pricing) need escrow and are **deferred to Stage 1**; at Stage 0 the caller and answerer are
both ours, so the audit-call fee question does not arise.

### 6. The object that does not fit, and what that forces

A bare-v2 logits-event refutation cannot ride any single transaction on these parameters
(0.99 MB vs 0.5 MB block mass). The design refuses both easy outs — raising our own devnet's
mass caps (a dispute layer that only works on tuned parameters is self-deception) and
DA-by-reference for adjudication bytes (every node must recompute the check from the
transaction alone; fetching evidence at validation time is not consensus-safe). What remains:

1. **Legs-carrying classes are unaffected.** Their computational divergence localizes through
   activation rows (8 KB — fits with 30× margin), which is now an *additional, measured*
   argument for the composite commitment form beyond ADR-0026 §2's: **the legs are what make
   refutations carriageable.**
2. **Bare-v2 classes get chunked evidence carriage at Stage 1**: a 3-chunk reassembly
   envelope (`evidence_id ‖ chunk i/n ‖ bytes`, ≈ 331 KB per chunk, each standard-mass-legal;
   the refutation adjudicates only when all chunks of `evidence_id` are accepted; `W_round`
   applies to the last chunk). This is deliberately not designed further here — it is gated
   work, and the gate is stated as a rule:

> **No Stage 2 (slash-bearing) operation for a class whose registered commitment form is
> bare-v2 until chunked logits-evidence carriage has landed and been drilled.** Composite-form
> classes are not blocked by this.

### 7. What this ADR deliberately does not decide

* **Fee-bond escrow** for opening calls and the challenger bounty plumbing (ADR-0028 §4/§5) —
  needs the bond-UTXO covenant discipline; Stage-1 design.
* **The credit gate's consensus wiring** (Stage 2): this ADR carries the facts the gate will
  read; it does not implement the gate.
* **Subnetwork id values** — assigned at the Stage-1 coordinated release alongside the token
  band's, not reserved here.
* **DA-layer serving of checkpoint state bytes** (interval spot-checks, ADR-0028 §5) — rides
  the existing v0.1 §10 DA manifest rules when that layer is wired; nothing here blocks it.

## Assumptions that remain (stated so they can be attacked)

1. **Native-payload admission stays open.** Stage 0 rides an existing behavior; if a future
   release tightens native-payload rules, Stage 0 carriage moves to Stage 1 early. The
   watcher would detect this as submission rejections, not as silent data loss.
2. **The RPC acceptance stream is faithful** — same-node trust only: each watcher trusts the
   node it queries, which is the trust a validator already places in its own node.
3. **Standardness relay**: payload-bearing native txs relay through today's mempool (no
   payload rule exists to stop them); if relay policy changes, direct-to-miner submission on
   our own fleet still lands drill objects — degraded, and visible in inclusion latency.
4. **Watcher determinism**: two watchers over the same chain produce identical ledgers —
   guaranteed by construction (acceptance DAA + first-accepted-wins + the pure classification
   functions of `palw_schedule`), and drilled by running watchers on two hosts and diffing
   reports.

## Consequences

* **New Land module next** (consensus-inert, like everything before it): the magic envelope +
  the five carriage bodies + caps + encode/decode + golden tests in `consensus/core`
  (`palw_carriage.rs`), reusing the frozen inner objects untouched. Then the watcher/submitter
  binaries (out-of-node), then the drill runbook.
  > **Landed (2026-08-16, `consensus/core/src/palw_carriage.rs`, on the unified branch,
  > consensus-inert):** magic + five bodies + the 16-opening carriage cap + stateless
  > validators + the Stage-0 extractor (`palw_carriages_from_accepted_txs`, the
  > capability-extractor twin: native + magic + decode + validate). Composite commitments are
  > **recomputed, not trusted** — the binding must derive from the carried envelope and rebuild
  > the committed root from parts; the attester dedup identity is equality-enforced against the
  > inner signer; the refutation enum ships without a logits-event variant per §6. Five
  > full-payload goldens pinned; domains uniqueness-tested across every PALW family and the
  > VLT sortition key. Still missing toward Stage 0 running: the watcher/submitter binaries
  > and the drill runbook.
* **ADR-0028's Stage-0 telemetry becomes realizable**: `P_check`, no-show and inclusion
  latency stop being columns the ledger cannot fill. The §12 checklist items this feeds:
  `[ ] P_check measured in shadow`, `[ ] no-show/inclusion telemetry published`.
* **The composite commitment form gains a second justification** (§6): legs make refutations
  fit the chain. Registration guidance: new classes should register the composite form.
* **The version trap is now a stated rule** — the next person who wants to "just add a
  version" to a deployed validator has a sentence to collide with.

## Mission alignment amendment — 2026-10-07

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0029-palw-chain-carriage.md -->

</details>

<a id="source-0046-palw-v2-consensus-object-carriage"></a>

<details>
<summary>0046-palw-v2-consensus-object-carriage.md — 原文 L1–L201</summary>

[原文を開く](../adr/0046-palw-v2-consensus-object-carriage.md) · SHA-256: `8e245fb614ac8598ecd7d19b4449adf80ab7189f164c01ea9c895f3cd547a96e`

<!-- ADR_SOURCE_BEGIN:0046-palw-v2-consensus-object-carriage.md -->
~~~~~~~~markdown
# ADR-0046 — PALW V2 consensus-object carriage: the registrations ride their collateral, the verdicts ride their evidence

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


- **Status:** Accepted (2026-08-20)
- **Relates to:** ADR-0042 (the ruleset this feeds; its Decision 5 state machine consumes
  `PalwConsensusObjectV2`), ADR-0043 (the state root those objects move), ADR-0029 (the V1
  carriage whose Stage-1 shape this reuses), ADR-0009/0010 (the stake-bond rail this mirrors
  for bonds).
- **Lands with:** the acceptance seam (`consensus/src`) and the RC genesis loader. The wire
  types and stateless validation land now in `consensus/core/src/palw_carriage_v2.rs`,
  dormant — no shipped preset admits the band.

## Problem

`apply_palw_transition_v2` consumes a `Vec<PalwConsensusObjectV2>` per chain block, and nothing
defines where those objects come from. Ten variants exist; a network cannot run on the two the
genesis can seed. The V1 carriage (ADR-0029) answers the same question for the *evidence* lane
(commitments, attestations, openings, refutations); the V2 *state* lane needs its own answer,
and it is not the same answer, because three of the objects are not "signed statements someone
published" — they are UTXO events (bonds), derivations (panels), or the absence of an event
(defaults).

## Decision 1 — one subnetwork id per kind, band 0x50, Borsh body, no magic

ADR-0029 Stage 1, verbatim: the subnetwork id IS the address, the payload is the Borsh body of
one wire struct, bodies never embed their carriage, one object per transaction. The V2 state
band starts at 0x50 (0x46–0x4F stay reserved for V1 evidence growth):

```
0x50  SUBNETWORK_ID_PALW_V2_BOND          PalwBondCarriageV2         → BondRegistered
0x51  SUBNETWORK_ID_PALW_V2_RETIRE        PalwRetireCarriageV2       → BondRetireRequested
0x52  SUBNETWORK_ID_PALW_V2_BIND          PalwBindCarriageV2         → PanelBound
0x53  SUBNETWORK_ID_PALW_V2_RECEIPTS      PalwReceiptsCarriageV2     → ReceiptLicensed | ProducerDefaulted
0x54  SUBNETWORK_ID_PALW_V2_COURT_OPEN    PalwCourtOpenCarriageV2    → CourtOpened
0x55  SUBNETWORK_ID_PALW_V2_COURT_CLOSE   PalwCourtCloseCarriageV2   → CourtClosed
```

Four variants have **no wire kind**, deliberately:

- `ClassRegistered` / `ClassFrozen` / `ClassUnfrozen` — **genesis-only at RC.** BASE-0 is the
  only weight-bearing class (ADR-0042 Decision 8) and it is registered by the genesis loader,
  which verifies the class catalog against its committed root (`verify_against_catalog`) before
  producing the objects. A runtime class-lifecycle path is a governance decision; when a
  governance ADR wants one, it allocates its own kind rather than inheriting an unguarded one.
- The transition's **timeout edges** (`BindTimeout`, `ReceiptTimeout`, retirement completion,
  challenge-window expiry) — never carried. They are functions of the deadline index and
  `ctx.daa_score`, derived inside `apply_palw_transition_v2`; an object claiming "a timeout
  happened" would be a second source for a fact the state already knows.

## Decision 2 — two validation layers with two different failure meanings

The fork's own precedents, adopted as a pair:

- **Stateless shape** (decodability, version, sizes, signature lengths, internal signature
  checks that need no chain state) runs at transaction **isolation** validation. Failure is a
  transaction rule error — the `InvalidDnsOverlayPayload` shape
  (`tx_validation_in_isolation.rs`) — and a block carrying a malformed carrier is invalid.
  Malformation is always authored; there is nothing honest to protect.
- **Stateful admission** (does the bond exist, is the panel's anchor mature, does the quorum
  hold at this candidate chain point) runs at **acceptance**, against the candidate chain's
  own state. Failure means the carrier is **skipped — unaccepted, never block-invalidating** —
  exactly how a double-spend or a non-releasable stake-bond spend is treated today
  (`utxo_validation.rs`: "SKIPPED (not accepted, not muhashed)"). Two honest carriers can race
  (two retires for one bond, two binds for one claim); the second is stale by the first's
  effect, and a block builder must not be invalidated by mempool timing it cannot see.

The consequence that makes the walk total: **the object list a chain block hands to
`apply_palw_transition_v2` is the accepted-carrier list.** Acceptance filtered everything the
state machine would refuse, so a transition error on tx-carried objects is unreachable — the
only transition input that can still fail is the header-carried attempt, whose failure is the
block's own (ADR-0042 disqualification seam), not a carrier's.

## Decision 3 — the bond IS its collateral output

`SUBNETWORK_ID_PALW_V2_BOND` mirrors the stake-bond rail (`StakeBondPayload`, live on t10/t11):

- The carrier transaction's **output 0 is the collateral**. The bond key is
  `(carrier_txid, 0)` and `collateral` is **output 0's value, read, not declared** — a declared
  amount beside a real output is a second source that can lie
  (`BondOutputValueMismatch` is the V1 error this design makes unrepresentable).
- The payload carries the two keys and one signature:

```rust
PalwBondCarriageV2 {
    version:          u16,
    executor_pubkey:  Vec<u8>,   // ML-DSA-87, 2592 B — the key attempts are signed under
    operator_pubkey:  Vec<u8>,   // ML-DSA-87, 2592 B — the identity panels dedup on
    operator_signature: Vec<u8>, // ML-DSA-87 over H(domain ‖ executor_pubkey), operator's key
}
```

- **The operator co-signs the adoption of the executor key.** Without it, anyone could register
  bonds attributing executors to a victim operator (forced attribution: the victim's identity
  collects strangers' offenses). The signed message is the executor key alone — deliberately
  replayable, because the blessing is per-relationship, not per-bond: replaying it costs the
  replayer fresh collateral and gives the operator another bond it already consented to back.
- **Funding is the executor-side authorization**, as on the stake rail: no executor signature
  at registration, because a bond whose executor key the registrant does not hold is a bond
  that can never produce a valid attempt (every attempt is signed over `attempt_id` under that
  key) — it can only lose its own collateral.
- **Withdrawal discipline** (the spend-gate rule, landing with the acceptance seam):
  the bond outpoint is unspendable while its record is not fully retired; once retired, the
  release transaction may pay the owner at most `remaining = collateral − slashed`, and must
  carry one output of exactly the slashed total to the canonical PALW burn script
  (`PALW_V2_BOND_BURN_SPK`, provably unspendable, bytes pinned with the gate). The slashed
  part must not be expressible as fee — fee is miner revenue, and a producer who mines can
  otherwise slash itself into its own coinbase.

`SUBNETWORK_ID_PALW_V2_RETIRE` is the executor's own act:
`PalwRetireCarriageV2 { version, bond: TransactionOutpoint, executor_signature }`, the
signature over `H(domain ‖ bond outpoint)` under the bond record's executor key (checked at
acceptance, where the record is). Replay against an already-retiring bond is a stateful skip.

## Decision 4 — panels are derived, receipts are counted, courts carry their proof

- **`PALW_V2_BIND` carries `{claim, anchor}` and NOTHING else.** The seats are what
  `derive_panel_v2` computes from the candidate state and the anchor; acceptance derives them
  and hands the transition a `PanelBound` whose seat list is the derivation's. Carrying seats
  on the wire would be a second source for a pure function's output — the dual-source rule
  ADR-0029 already enforces for bodies. Binding is permissionless: the carrier needs no
  signature, because a correct bind is a fact about the chain, not a claim about the sender
  (`validate_panel_bound_v2` is the acceptance check; a wrong anchor or a dead claim is a
  skip).
- **`PALW_V2_RECEIPTS` carries `{claim, receipts}` and declares no direction.** Acceptance runs
  `validate_receipt_quorum_v2` (every receipt signature under the seat bond's registry key,
  `signed_daa` windows, `Unavailable` particulars, one answer per seat) and the quorum it
  returns IS the direction: `Licensed` maps to `ReceiptLicensed`, `ProducerUnavailable` to
  `ProducerDefaulted` — the two are provably disjoint (`2·quorum > seat_count`), so nothing is
  left for a declaration to add except a chance to contradict the derivation. Both objects hand
  the transition the seat-verdict set, because the transition prices the seats the quorum
  refuted (audit C5's both-directions slash).
- **`PALW_V2_COURT_OPEN` carries `{claim, challenger_bond, space, space_size,
  challenger_signature}`** — the signature over
  `H(domain ‖ claim ‖ challenger_bond ‖ space ‖ space_size)` under the challenger bond's
  executor key: a challenge names the stake that backs it (ADR-0042 Decision 8: refuted
  challengers are slashed) AND the dispute shape it commits to, so a relayer cannot re-mount
  one signature onto a different bisection space. `session_id` is **derived** — it is
  `court_session_id_v2(claim, claim.trace_root, executor_bond, challenger_bond, space,
  space_size)`, whose extra inputs come from the candidate state, which is why the carrier does
  not carry it (`validate_court_opened_v2` is the acceptance check; a lapsed window, a missing
  challenger or a self-challenge is a skip).
- **`PALW_V2_COURT_CLOSE` carries `{session_id, proof}` and NO verdict.**
  `adjudicate_court_close_v2` returns the only verdict the proof supports (the terminal
  arithmetic adjudication of ADR-0030/0033, C3's `check_execution_root_binding` included), and
  the transition is handed `CourtClosed` with THAT verdict — never one the object announced. A
  proof that does not adjudicate is a skip. Timeout defaults are NOT carried (Decision 1) — a
  session nobody closes is closed by the deadline sweep.

## Decision 5 — order is acceptance order

The object list of a chain block is the concatenation of its accepted carriers' objects in
**canonical acceptance order**: mergeset order, then intra-block transaction index — the exact
sequence acceptance data already fixes for every node. No PALW-specific ordering exists; a
second ordering would be a second consensus.

The header-carried attempt is **not part of this list** — it is the transition's separate
`attempt` argument, applied after the block's objects (registrations in a block admit attempts
in that block's own children at the earliest, since admission runs against the parent-side
state per ADR-0043).

## Mass budget (against the 480,000 standard cap, mass_per_tx_byte = 1)

| carrier | dominant terms | est. bytes | fits |
| --- | --- | --- | --- |
| bond | 2 × 2592 pk + 4627 sig | ≈ 9.9 K | ✓ |
| retire | 4627 sig + outpoint | ≈ 4.8 K | ✓ |
| bind | two hashes | ≈ 0.2 K | ✓ |
| receipts (5 seats) | 5 × (4627 sig + verdict) | ≈ 24 K | ✓ |
| court open | 4627 sig + hashes | ≈ 4.9 K | ✓ |
| court close | terminal one-step proof | court-capped (≪ ADR-0029's 152 K answer bound) | ✓ |

## What this ADR does not decide

- The **bytes of `PALW_V2_BOND_BURN_SPK`** and the spend gate's wiring — they land with the
  acceptance seam, which owns UTXO rules.
- The **court proof's future proof classes** — `PalwCourtVerdictProofV2` is the court module's
  own enum (today: `Arithmetic { refutation, operand_openings }`); the CLOSE carrier presents
  whatever variants that enum grows, and acceptance verifies them through
  `adjudicate_court_close_v2`. This ADR fixes the carriage, not the proof taxonomy.
- Any **runtime class lifecycle** — a governance ADR allocates its own kinds.
- **Relay/mempool policy** for the band (standardness, RBF-adjacent races) — policy, not
  consensus; the skip semantics above make every policy outcome safe.

## Number hygiene

This record briefly shipped as ADR-0045 and was renumbered the same day, by its own rule.
0043 was claimed hours earlier by a parallel session (`palw-v2-state-root-ordering`), 0044 is
committed on the `palw-freeprompt-v3` branch (free-prompt PALW), and 0045 — checked against
committed branches at numbering time, then claimed at 17:56 by a THIRD parallel lane
(`palw-cross-class-v1`, `palw-class-economy-on-chain`) nineteen minutes before this record's
first commit at 18:15. ADR-0036 Decision 5 breaks the tie by timestamp: the later writer
renumbers, and that is this record — now 0046. (Two commits on this branch, `aebb5b24` and
`c1425406`, say "ADR-0045" in their immutable messages; they mean this record.)
~~~~~~~~
<!-- ADR_SOURCE_END:0046-palw-v2-consensus-object-carriage.md -->

</details>

<a id="source-0115-a-pending-transaction-is-announced-until-it-lands"></a>

<details>
<summary>0115-a-pending-transaction-is-announced-until-it-lands.md — 原文 L1–L86</summary>

[原文を開く](../adr/0115-a-pending-transaction-is-announced-until-it-lands.md) · SHA-256: `11f16ebce4015e634b0aa7172a7e2cf41cdef30a2e2c060b8b88dc8a5533cb55`

<!-- ADR_SOURCE_BEGIN:0115-a-pending-transaction-is-announced-until-it-lands.md -->
~~~~~~~~markdown
# ADR-0115 — A pending transaction is announced until it lands

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

* Status: PROPOSED 2026-09-11, from the operator's report ("また購入してもトランザクションやチャートに反映されてない
  原因は" / "何回購入しても balance が減らない") and their standing instruction to put liveness first.
  **IMPLEMENTED the same day, consensus-inert**: node policy only — no validity rule, no `Params`
  field, no fingerprint moves. A node takes it by a rebuild and restart; a node without it still
  validates every block the others make.
* Builds on: [0020](0020-selected-parent-evm-lane.md) (the EVM lane: a payload is carried by a block and
  executed when the chain accepts it), [0072](0072-the-ticket-is-the-execution.md) (a producer's template
  is fixed for the whole draw), [0109](0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md)
  (the same failure for deposit claims, fixed there by making every producer claim unasked).
* Amends: kaspa-pq EVM Lane v0.4 §14.2 (EVM tx gossip) and §16 (mempool retention).
* Supersedes nothing.

## 0. The sentence this ADR is

**A pending EVM transaction is announced again — to a peer when it connects, to every peer every five
minutes, and whenever a client sends it again — and is kept for a day, so a transaction reaches the
producer that will carry it however late that producer arrives, and is dropped only when it has
executed or could not.**

## 1. What was measured (2026-09-11, testnet-11)

Two joins from `0x0aec…8d79` (nonce 0 and 1, 10,000 MSK each) were sent to the explorer node during a
fleet rollout and never executed:

* Across all 1,546 EVM blocks of testnet-11, **no EVM transaction had ever been included** (only deposit
  claims, which ADR-0109 carries without relay).
* The explorer node held both (`misaka_getEvmTxStatus`: pending, `includedIn: []`), and so did the four
  `.113` pool slots — their block templates carried both — but the slots win about **one draw in
  38,570** and had produced nothing that run.
* The producers that actually win (ibm node0/node1, 5.104 seat2) were restarting when the transactions
  were announced (their connections to the explorer date from 10:08:50–10:21:34Z). The announcement is
  sent **once**, at admission, to whoever is connected; nothing announces it again; so they never held
  the transactions (`misaka_getEvmTxStatus` on ibm node0: `unknown`).
* The pool's TTL was **one hour**. With a chain block every 15–60 minutes and templates fixed per draw,
  that is shorter than an ordinary wait. Both transactions expired unexecuted at ≈11:40Z, and the wallet
  and the site noticed nothing: the balance never moved and each retry looked like the first one failing.

## 2. Decisions

1. **The relay has its own clock.** The P2P service ticks `FlowContext::evm_relay_tick` every
   `EVM_RELAY_TICK` (15 s) until shutdown. Every tick flushes the EVM spread's queue (it used to be pumped
   only when a block arrived); every `EVM_REANNOUNCE_EVERY_TICKS` ticks (5 min), while the node is nearly
   synced, the pool is maintained (Decision 3) and **every pending hash is announced again** to every
   EVM-relay peer. A peer requests only the hashes it lacks, so an announcement it already heard costs
   32 bytes.
2. **A peer that connects hears the pool once, at once.** After the handshake launches its flows, a peer
   on protocol ≥ `PROTOCOL_VERSION_EVM_RELAY` is sent the pool's hashes (in inv messages of at most 512).
   A peer that is still syncing ignores them; Decision 1 reaches it minutes later.
3. **The pool is kept true without a template.** `MiningManager::maintain_evm_pool` expires past the TTL
   and prunes what the chain executed (a nonce below the sender's committed nonce) — what every template
   build already did, now also done by a node that builds no template (the explorer and RPC nodes, where
   wallets send). A failed state read prunes nothing.
4. **A client that sends a pending transaction again is asking for it to travel.** `submit_rpc_evm_transaction`
   announces a `Duplicate` before reporting it (the eth RPC still answers it with the hash, as before).
5. **The TTL is a day** (`EVM_MEMPOOL_TX_TTL_SECS = 86 400`). Executed transactions leave by Decision 3;
   the count, byte and per-sender caps bound the pool; the TTL bounds only what can never execute.

## 3. What it does not do

It does not make a block come sooner: a transaction still waits for a producer that holds it to start a
draw and win it, and on testnet-11 that is tens of minutes. Nor does it change what a producer selects
or what the chain accepts. The site and MISAKA Wallet (the other half of the same report) show a pending
order as pending, keep its signed bytes and send them again while its nonce is free — which, with
Decision 4, also makes the node announce it again.

## 4. Tests

* `maintain_prunes_what_executed_and_keeps_what_is_pending` — pending kept, executed pruned without a
  template, nothing pruned on a failed state read; `evm_pending_hashes` is what the relay announces.
* `ttl_expiry_and_removal_keep_accounting_consistent` — the TTL is a day; `hashes` lists every pending tx.
* kaspa-mining 103/0 (with `evm`), kaspa-p2p-flows 108/0. Not drilled end to end on two nodes (a
  funded EVM sender on a local devnet was not set up); the relay decisions are glue over the tested pool
  calls and the existing spread.
~~~~~~~~
<!-- ADR_SOURCE_END:0115-a-pending-transaction-is-announced-until-it-lands.md -->

</details>

