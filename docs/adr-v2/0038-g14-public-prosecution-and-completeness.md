# ADR-v2-0038 — G14公開訴追・adjudicability・裁定保証の前提

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 7本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0014](../rfc-v2/0014-court-master-contract.md)

## 1. この文書の役割

G14がどの条件で外部者の訴追を成立させるか、その保証と限界を一つの入口にする。

## 2. 統合した判断の読み方

### 2.1

Panelの追加投票に依存しない公開訴追、認証されたopeningによる局所化、Nodeによる決定論的裁定を中心に置く。静的adjudicabilityと実際に証拠を取得して到達するE2E能力を区別する。[旧ADR-0027](../adr/0027-palw-slash-unilateral-fraud-proofs.md) [旧ADR-0049](../adr/0049-palw-adjudication-contract.md) [旧ADR-0069](../adr/0069-e2e-adjudicability-is-the-price-of-weight.md) [旧ADR-0173（public-verifier-dispute-completeness-is-misaka-purpose）](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)

### 2.2

capacity、最低担保、保持期間、対象class、nativeの公開filingとFinal後の責任、D2／X3のarm recordを個別条件として保存する。[旧ADR-0183](../adr/0183-g14-court-capacity-panel-floor-retention-detection.md) [旧ADR-0187](../adr/0187-g14-native-public-filing-and-post-final-mining-accusation.md) [旧ADR-0188](../adr/0188-d2-x3-arm-records-composition-and-pause-credit.md)

### 2.3

正しい登録modelを保有・取得できる独立verifierという前提を省略しない。G14はその前提の下での追及能力であり、署名・hash・モデル年次指定が計算実行の無条件証明になるとはしない。[旧ADR-0173（public-verifier-dispute-completeness-is-misaka-purpose）](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) [旧ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)

## 3. 維持する境界・後続改定

旧ADR-0173のモデル取得保証部分はADR-0177で撤回された範囲を除外して読む。統合は未解決の実装・供給・経済安全性を証明済みにするものではない。

## 4. 隣接する判断との分担

仕様の正本は[RFC-v2-0014](../rfc-v2/0014-court-master-contract.md)。通常検査は[ADR-v2-0033](0033-probabilistic-checks-and-exact-court.md)、DAは[ADR-v2-0039](0039-data-availability-court-and-panel-da.md)、長文脈とserviceは[ADR-v2-0041](0041-long-context-court-decomposition.md)・[ADR-v2-0042](0042-court-service-and-versioned-clock.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0027-palw-slash-unilateral-fraud-proofs.md](../adr/0027-palw-slash-unilateral-fraud-proofs.md) | L1–L309 | L15–L29 |
| [0049-palw-adjudication-contract.md](../adr/0049-palw-adjudication-contract.md) | L1–L438 | L15–L17 |
| [0069-e2e-adjudicability-is-the-price-of-weight.md](../adr/0069-e2e-adjudicability-is-the-price-of-weight.md) | L1–L498 | L15–L28 |
| [0173-public-verifier-dispute-completeness-is-misaka-purpose.md](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) | L1–L232 | L14–L21, L217–L217 |
| [0183-g14-court-capacity-panel-floor-retention-detection.md](../adr/0183-g14-court-capacity-panel-floor-retention-detection.md) | L1–L23 | L3–L8 |
| [0187-g14-native-public-filing-and-post-final-mining-accusation.md](../adr/0187-g14-native-public-filing-and-post-final-mining-accusation.md) | L1–L59 | L3–L8 |
| [0188-d2-x3-arm-records-composition-and-pause-credit.md](../adr/0188-d2-x3-arm-records-composition-and-pause-credit.md) | L1–L93 | L3–L14 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0027-palw-slash-unilateral-fraud-proofs"></a>

<details>
<summary>0027-palw-slash-unilateral-fraud-proofs.md — 原文 L1–L309</summary>

[原文を開く](../adr/0027-palw-slash-unilateral-fraud-proofs.md) · SHA-256: `150f66dda612712eacecc4c0131a16bd8403ddf32a2d985a7c1af9a708b4e807`

<!-- ADR_SOURCE_BEGIN:0027-palw-slash-unilateral-fraud-proofs.md -->
~~~~~~~~markdown
# ADR-0027: PALW-S — unilateral fraud proofs; no BFT, no challenge randomness, slash-terminal

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Accepted (architecture); PROMOTED by ADR-0038 from credit machinery to L1 machinery.**
Operating envelope as written: devnet / shadow / zero-credit. No slashing is enabled by this ADR; it
fixes what a slash may ever rest on — and that rule is unchanged by the promotion. What changed is
what a slash *bounds*: on the V2 lineage block production is bonded
([ADR-0039](0039-palw-only-block-production.md) Decision 6), so the aggregate-leverage half of
§4's `max_leverage ≤ 1.0` is enforced per bond by
[ADR-0042](0042-palw-mainnet-candidate-ruleset.md) Decision 6 (`reserved_exposure`), not by a
per-job credit price.
Date: 2026-08-15
Relates to: ADR-0026 (v2 verification architecture — amended here in §3/§4/§5),
[`misaka-palw-slash-protocol-design-v0.1.md`](../design/palw/verification/misaka-palw-slash-protocol-design-v0.1.md)
(the input specification this ADR adopts and amends),
[`palw-full-logits-trace-v2-design.md`](../design/palw/execution/palw-full-logits-trace-v2-design.md) (§10 economics,
§12 gates), `consensus/core/src/vlt.rs` (`ComputeFraudKind` — the existing enforcement of the same
rule at the VLT layer).

## Premises (given, not derived)

- **P1 — no BFT dependence.** No honest-majority committee may be the source of truth for a
  verdict. A vote may schedule work or allocate rewards; it may never decide that a computation
  was wrong.
- **P2 — no challenge-randomness dependence.** No slash outcome may depend on a hash-derived
  challenge position being unpredictable or unbiasable. *(Read as: the unpredictability of the
  challenge is not a security assumption. Hash functions remain in use as commitment primitives —
  Merkle openings, domain-separated preimages — which is a different dependency. If the intended
  reading was stronger, this section is the one to correct.)*
- **P3 — fraud terminates in slash.** The final consequence of fraud is bond movement. Not chain
  invalidity, not a reorg, not a cryptographic soundness claim.

These premises are not a restriction added on top of a working design — they **select** the design.
The rest of this ADR is what they force.

## What the premises force

**P1 ⇒ every slashable offense must be objectively refutable from published material by a single
party.** This is already the enforced rule one layer down: `ComputeFraudKind` slashes only on
`ContradictoryVerification` (one verifier, two signatures that cannot both be true) and explicitly
refuses to slash on `ForgedReceipt` — "I re-ran it and got something else" is not checkable without
re-running it, and consensus cannot re-run it. PALW-S generalizes that rule rather than carving an
exception for it.

**P1 also deletes "false PASS by majority" as a concept.** A PASS is not a positive finding of
correctness; it is **the absence of a refutation within the window**. Nobody is ever slashed for
failing to find fraud (unprovable). A party that *chooses* to sign a positive attestation stakes a
bond against later refutation — that remains objective, because the slash evidence is
`signature ∧ refutation`, not a jury's opinion.

**P2 ⇒ the disputed position is chosen adversarially by the challenger, not drawn by a PRF.**
Sampling exists to catch a cheater who does not know where you will look; if unpredictability is
not assumable, sampling has no security to offer and is demoted to an incentive/coverage device.
What replaces it is a challenger who **re-executes and points at the first divergence**.

**P2 also removes an entire failure surface**: anchor-reorg fragility (v0.1 §8.2), commitment
grinding over favorable challenges (§8.3, §26.4), and the requirement that a "finalized" anchor
exist before a challenge may be issued. None of it is load-bearing any more.

**P1 + P2 ⇒ exactness is mandatory, not preferred.** ADR-0026 §3 chose exact-within-class over a
tolerance band on measurement grounds. Under P1 that choice becomes structural: a tolerance
verdict ("close enough?") is not objectively checkable by one party, so it can only be settled by
a vote — which is precisely the BFT dependence P1 forbids. **A tolerant comparator and a BFT-free
slash cannot coexist.**

**P3 ⇒ un-refuted fraud is a credit error, not a chain fault.** The worst case of a fraud nobody
checks is wrongly-minted credit, which is bounded by exposure caps and the staged ladder (v0.1
§20.2) — never by rolling back the DAG.

## Decision

### 1. Direct refutation is the primary path; bisection is the DA-degraded fallback

Any deviation from the pinned execution implies **at least one locally-refutable step**: at the
first divergent step, the committed input still equals the honest input (the prefix matched) while
the committed output does not equal the step function applied to it. So a challenger who
re-executes does not need to search — it locates the first divergence and names it.

```
challenger re-executes the job under the pinned profile
  → first index i where its state ≠ the miner's committed state
  → submits ExecutionStepRefutationV1 { proof_id, step_index i,
        opening(committed input state at i), opening(committed output state at i) }
  → every node recomputes ONE step from the opened input under canonical reference semantics (§2)
  → if output ≠ committed output: the miner is slashed. If it matches: the challenger is slashed.
```

One round, one bounded check, no vote. **Bisection is retained only for the degraded case** where
the miner has withheld intermediate states so the challenger cannot name a step with openings: the
interactive ladder forces disclosure incrementally (≈log₂(steps) ≈ 20 rounds at 10⁶ steps), each
round objectively checkable, terminating in the same one-step check. Non-response at any rung is
an objective offense (v0.1 §17.1 `M-O3`), so withholding is not an escape — it is a faster loss.

This makes **data availability load-bearing for the BFT-free property**, not merely an anti-
griefing nicety: v0.1 §10's DA manifest, retention and multi-source failure evidence are adopted
in full, and `DATA_WITHHOLDING` keeps its objective status.

### 2. Adjudication semantics: one step, canonical reference arithmetic, reproduced by every node

The one-step check must be reproducible by **every node on heterogeneous hardware**, which native
float cannot promise. It is therefore defined in **canonical reference arithmetic** — deterministic
software IEEE-754 (soft-float) or the profile's integer form (v0.1 §5.2 `CanonicalTensorV1`), with
pinned operation order, no FMA contraction, RNE, no fast-math. Cost is not an obstacle at this
granularity: one decode step of a 2B model is ~4·10⁹ FLOPs, while one pinned GEMM tile (e.g. 64³)
is ~5·10⁵ — four orders below — and a 10–100× soft-float penalty on one tile is still negligible
beside ordinary block validation. **The claim is 10¹⁵ ops; the adjudication is 10⁵–10⁷.** That
ratio is the entire reason a BFT-free dispute is affordable.

This upgrades the class definition of ADR-0026 §3. A determinism class stops being *"these hosts
agreed pairwise"* and becomes:

```
runtime_class_id conforms  ⟺  it reproduces the canonical reference implementation
                              bit-exactly over the conformance corpus
```

An absolute reference replaces pairwise agreement; conformance is then transitive, and the
adjudicator needs no class membership of its own. Honesty about the cost: a class conforms only if
its reduction order is pinned (true of the current CPU profile by construction; **hard on GPUs**,
which reorder reductions via split-K/atomics unless deterministic kernels are mandated). A backend
that cannot match the reference cannot be admitted — that is a clean admission criterion, and it
is expected to exclude some hardware.

### 3. What replaces `P_detect`: one funded honest re-execution, and f-independence

v0.1 §9 sizes bonds from `P_detect = 1-(1-f)^q`, and v0.1 §31.1 correctly names its own fatal
weakness: an attacker who makes `f` arbitrarily small makes sampling arbitrarily weak. Under
direct refutation that parameter disappears:

```
sampling model      P_detect = 1-(1-f)^q            ← adversary minimizes f
refutation model    P_detect = P(at least one honest full re-execution + timely inclusion)
                             ← f-independent: a full re-executor finds ANY deviation
```

A single mis-multiplied tile is caught with the same certainty as a wholly fabricated trace. The
price is stated plainly and must not be hidden later: **the honest verification tax is one full
replay per checked job, so a checked job costs ≥2× primary execution.** This is the honest
counterpart of the "one-token verification is cheap" claim already banned in the v2 design §15;
per-class cold, no-KV, p99 replay cost is what the §10 economics must carry.

`q` survives with a changed meaning: not *how many positions to sample* but **how many independent
re-executors to fund**, i.e. the redundancy of the 1-of-N assumption. v0.1 §9.2's dynamic inputs
(reward, bond, gain, model size, length, reputation, recent mismatch) apply unchanged to that
quantity, and v0.1 §9.1's tiers become funding tiers. The inequality keeps its shape, with
`P_detect` now an incentive property rather than a combinatorial one:

```
P_check · S_eff ≥ λ · G_max,   λ ≥ 2.0 initially,   P_check measured, never assumed
```

### 4. Amendments to the v0.1 specification

| v0.1 mechanism | Verdict under P1/P2 | Replacement |
|---|---|---|
| §12.3 primary quorum 4-of-5 / 5-of-7 | BFT — may not convict | verifiers are watchers; no vote is counted |
| §16.3 appeal jury 9-of-13 | BFT | one-step check under §2; no jury exists |
| §15.2 `ComputationFaultCertificateV1` (quorum bitmap + aggregate sigs) | BFT | `ExecutionStepRefutationV1` (§1): one step, openings, objectively checkable |
| §8 future-anchor challenge randomness as security | P2 violation | challenger-chosen position; randomness kept only as an optional coverage sampler that decides nothing |
| §9 `P_detect = 1-(1-f)^q` for bond sizing | f-minimizable | §3: funded full re-execution, f-independent |
| §14.3 "primary quorum + appeal ⇒ FAIL_COMPUTE" | BFT | FAIL only via §1 refutation; no quorum state |
| §13 verifier commit-reveal | needed only because votes were counted | optional hygiene for the attestation market; not a verdict input |
| §18.2 `V-C1` false PASS | keep — but re-grounded | slashed on `signature ∧ later refutation`, not on a jury's overturn |
| §18.2 `V-C2` false FAIL | keep — re-grounded | challenger loses the one-step check ⇒ ChallengeBond slashed |
| §19.2 statistical circuit-breaker triggers | may not slash; may freeze | §5: objective freeze trigger; statistics stay advisory |

Adopted from v0.1 **unchanged**: the four-bond model (§4) including `max_leverage ≤ 1.0` and
unbonding ≥ dispute lifetime; DA rules (§10); the objective offense taxonomy (§17.1, §17.3 O-codes,
§17.4); reward/bond separation and correlated-offense caps (§18.1, §24.2); `slash_id` idempotency
(§24.1); slash distribution with the challenger share below half (§18.4, as amended to 49% by
[ADR-0032, 2026-10-10](0032-palw-fee-bond-escrow.md#palw-challenger-share-amendment--2026-10-10)); delayed settlement and
credit caps (§20); DAA-score-denominated deadlines with the "DAA stalls ⇒ deadlines stall" rule
(§23); the prohibitions (§27); and the test plan (§28), whose canonicalization matrix now doubles
as the reference-conformance corpus of §2.

### 5. Freeze is permissive, slash is strict

Freezing takes no property, so its trigger may be **permissive**; slashing transfers property, so
its trigger must be **objective**. A statistical anomaly may therefore freeze a profile but may
never slash anyone. To keep even the freeze off a governance vote, one objective trigger is
introduced:

```
ClassContradictionCertificateV1 = two hosts, same runtime_class_id, same golden set,
  same job envelope, two signed and differing roots
    ⇒ objectively contradictory: the class claim itself is refuted
    ⇒ profile/class freeze (fail-safe), bonds frozen not released, no slash
```

The rest of v0.1 §19 applies as written: new slashes stop, bonds freeze rather than release
(an attacker must not escape through the emergency exit, an honest operator must not be
liquidated by it), PALW credit and DNS weight stop accruing, and **the BlockDAG and the hash PoW
floor keep running**. A corrected profile is a new `profile_id`; profiles are never overwritten.

### 6. Slash-terminal settlement (P3)

No PALW outcome — pass, fail, dispute, freeze — touches block validity, fork choice, or a past
DAG. Block validity remains `valid permanent hash PoW AND (PALW certificate absent OR valid under
its activation stage)`. A refuted proof moves bonds and revokes provisional credit; it never
re-orders history. The v0.1 §21 ladder is adopted with its stage semantics restated under this
model:

```
Stage 0 Shadow          record refutations, slash nothing; measure P_check, replay cost, DA
Stage 1 Objective slash objective offenses only (opening/equivocation/deadline/DA/bond misuse)
Stage 2 Bounded         §1 refutation may slash WorkBond; BaseBond capped 5-10%
Stage 3 Full            repeat-offense BaseBond escalation; wider DNS weight; hash floor stays
```

Stage 1's "computation mismatch does not slash BaseBond" is not a temporary conservatism here — it
is the same rule as P1, applied while reference conformance (§2) is still being proven.

## Assumptions that remain (stated so they can be attacked)

1. **1-of-N honest re-execution, funded.** At least one independent party actually re-executes a
   given job. This is an incentive property; if nobody is paid to check, nothing is checked. It
   replaces the honest-majority assumption with an honest-*existence* assumption — weaker, but not
   free.
2. **Censorship-resistant inclusion inside the window.** A single refutation must reach the chain.
   Under BFT this was diluted across a committee; here it is the critical liveness assumption.
   Mitigations: windows sized in DAA score with the stall rule, multiple submission routes, and a
   window long enough for the degraded bisection ladder (≈20 rounds × response window — materially
   longer than v0.1 §23's 30-minute dispute window; sizing it is open work).
   *ADR-0028 §3 (Accepted) supplies the sizing: `W_challenge ≥ W_replay + 20·W_round`, every
   response window sized at replay cost (never shorter), the whole ladder constrained inside
   the pruning horizon — which caps the CREDITED job size (~512 decode tokens on the measured
   fleet), with every default a placeholder until the per-class p99 measurement replaces it.*
3. **Reference conformance of the miner's class.** If a class silently stops matching the
   reference, honest miners are refutable. §5's freeze is the fail-safe, and §2's corpus is what
   makes drift detectable before it is punitive.
4. **DA sufficiency.** Enough of the trace must be retrievable to name a step; otherwise the
   dispute degrades to bisection and, ultimately, to an objective non-response offense.

Not assumed anywhere: honest majority, unpredictable challenges, cryptographic soundness of the
commitment as a proof of computation.

## Consequences

* **New consensus objects to design**: `ExecutionStepRefutationV1`, the bisection ladder
  messages, `ClassContradictionCertificateV1`, and a step-check verifier over canonical reference
  arithmetic. `ComputationFaultCertificateV1` from v0.1 §15.2 is **not** implemented.
  *Land status (2026-08-15, `consensus/core/src/palw_slash.rs`, consensus-inert):*
  `PalwClassContradictionCertificateV1` (§5) is implemented with its equivocation/divergence
  adjudication; Merkle openings against the v2 event commitment
  (`trace_event_opening_root_v1`, derivation-based shape — an opening cannot supply its own
  promote pattern) and two **structural** unilateral refutations are implemented:
  `PalwTraceSummaryRefutationV1` (the committed root's transparent preimage breaks the pinned
  exact-decode schedule) and `PalwTraceEventRefutationV1` (an opened event's preimage carries
  non-finite logits, a wrong row length, or a wrong vocab). All preimage reuse is frozen by
  equivalence tests against `palw_v2`; the arithmetic `ExecutionStepRefutationV1` remains blocked
  on the activation/checkpoint legs and the reference evaluator, as designed.
* **`ComputeFraudKind` gains a provable computation kind for the first time.** The memory-of-record
  rule — `ForgedReceipt` can never slash because it is uncheckable — stands for *unaided claims*.
  A refutation that carries openings and resolves to one reference-arithmetic step is checkable, so
  it is the adjudication that `FailedChallenge`/`ForgedReceipt` always lacked. Wiring it is future
  work behind the same staged gates; nothing in this ADR enables it.
* **The step function must be pinned at tile granularity** (`shape_profile_id`): which operator,
  which tile shape, which reduction order. This extends ADR-0026 §2's activation/GEMM legs from
  "committed" to "adjudicable", and is a prerequisite for Stage 2.
* **Reference implementation becomes a deliverable**: a soft-float/integer canonical evaluator,
  independently implemented **twice** (v0.1 §29 gate 1) — two implementations agreeing is evidence
  about the *specification*, which is a different and legitimate use of agreement than a jury
  deciding a fact.
  *Land status (2026-08-15, `consensus/core/src/palw_reference.rs`, consensus-inert):* the
  canonical arithmetic v1 is implemented as pure-integer soft-float (binary32 add/sub/mul/neg,
  pinned k-ascending dot, GEMM-over-dot) — no hardware float in the normative path, so MXCSR/FPCR
  state cannot reach a result; NaN payloads (the one legitimately nondeterministic IEEE surface)
  canonicalize to `0x7FC00000`; `reference_arithmetic_ruleset_id_v1` pins the rule text for
  future `shape_profile_id` binding. Verified against the hardware FPU as a clean-environment
  oracle: full special-value matrix, 200k random pairs, 100k tie-neighborhood pairs, 100k
  subnormal-range pairs, dot/GEMM cross-checks, an order witness and an FMA-discrimination
  witness, in debug AND release. This is one implementation plus an in-crate oracle — the §29
  gate-1 *independent second implementation* (e.g. a Berkeley-SoftFloat cross-build) remains
  open. Transcendentals (exp/sqrt/div — needed for softmax/norm steps) are deliberately absent
  pending their own canonical pinning; the step *function* remains unpinned, as above.
* **GPU admission is now falsifiable**: a backend is admitted iff it matches the reference
  bit-exactly with pinned reductions. Expect some hardware to fail this and stay out.
* ADR-0026 §3 (class definition), §4 (challenge randomness) and §5 (`q` semantics) are amended by
  §2/§3 above; the amendment notes are carried in that ADR so the two cannot drift apart.

## Mission alignment amendment — 2026-10-07

unilateral fraud proofという原則を中核目標へ昇格する。「bounded stepがある」は証拠取得・局所化の完成を含まない。fused/recurrent/held regimeも外部参加者のbounded proof generationを確認し、旧credit-onlyや旧quorum由来slashの記述を今後の制約としない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

共通の採掘上限を経済的抑止へ追加する。有効な計算不正証拠は客観裁定・報酬失効・徴収可能担保へのslashへ進めるが、claim量産・上限到達・ローカル検査failureだけで有罪にしない。
同じbondの残存責任を保持し、自己共謀reporter還流と共有担保枯渇を控除したnet損失を評価する。早期conviction/voidで消費済みの採掘枠を即時返還しない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0027-palw-slash-unilateral-fraud-proofs.md -->

</details>

<a id="source-0049-palw-adjudication-contract"></a>

<details>
<summary>0049-palw-adjudication-contract.md — 原文 L1–L438</summary>

[原文を開く](../adr/0049-palw-adjudication-contract.md) · SHA-256: `91dbc5d2d0133cedf525838e0d54dfc4b58ff41b4279c78a43b296e1e7c38549`

<!-- ADR_SOURCE_BEGIN:0049-palw-adjudication-contract.md -->
~~~~~~~~markdown
# ADR-0049: The adjudication contract — what a court opens, and the bound that makes it model-size-independent

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Proposed.** Activates nothing, registers no class, and changes no proof-of-work. It
specifies what a refutation may ask for, what the court may open to answer it, and the admission
gate that makes both bounded by a class's own geometry rather than by its model's size.

Date: 2026-08-21

Relates to: ADR-0038 (PALW is the consensus work; assumption **A4**, catalog coverage, is the
clause this ADR corrects), ADR-0030 (the step space and shape profile), ADR-0040 (`PALW-BASE-0`,
whose all-`int8` operands are the reason Decision A's defect stayed invisible), ADR-0042 (the
mainnet-candidate ruleset, whose court parameters gain four companions here), ADR-0046 (chain
carriage and its size expectations, which this ADR is what makes achievable),
`consensus/core/src/palw_step_refute.rs`, `consensus/core/src/palw_artifact.rs`,
`consensus/core/src/palw_class_admission_v2.rs`.

**Two premises this ADR does not touch, and is written to protect.** Block production stays a PALW
lottery — hash-function-independent, algo-6, one solved attempt per block. And the free-prompt lane
stays: a miner earns by running a *useful* inference for a real prompt. Every decision below exists
because those two premises require a court whose cost does not grow with the model, and the court
does not have one today.

---

> **Status as of the index reconciliation (2026-09-02).** The line above still says "Proposed …
> activates nothing"; the body records otherwise and later ADRs built on it. Decision E is
> implemented (`DecodeToken` close arm, 2026-08-22); Decision F landed (the canonical IR, 2026-08-26;
> made unviolatable for interpreted classes by [ADR-0067](0067-classes-are-chain-data-kernels-are-the-build.md));
> Decision C ships as `max_close_bytes = 80 KiB` and the RC identity gate (§Amendment); Decision H's
> registration carriage is live on testnet-11 and is the path [ADR-0056](0056-palw-permissionless-class-admission-and-share-economy.md)
> and [ADR-0075](0075-certification-is-a-consensus-object.md) extend (a weightless entrant, seated by
> objects). Decision G's tied-head inventory is still the recorded open item. The carriage ADR this
> file cites as ADR-0046 is [0046](0046-palw-v2-consensus-object-carriage.md) (renamed from a
> colliding 0045 on 2026-09-02). Map: [`README.md`](README.md).

## Context — the central claim was measured and does not hold

ADR-0038 W1 says a full node adjudicates every dispute while holding no model. The mechanism is
proof-carrying evidence: a refuter opens the operands it used against the class's registered
`artifact_root`, and the node recomputes one step. `PalwNoWeightsV1` exists as a production type
precisely so that "a node with no weights" is the normal case rather than a degradation.

An external audit of the 2026-08-21 snapshot reported that this does not hold in the code, and
every load-bearing part of that report reproduces on the integration branch:

**The terminal adjudication opens the whole weight matrix.** `palw_step_refute.rs:459`:

```rust
let wanted = out_dim.checked_mul(x.len()).ok_or(PalwStepRefuteError::Unadjudicable)?;
```

`out_dim × in_len` is the entire matrix, and `:479`/`:494` reject an operand of any other length.
The adjudicator then recomputes every output row and compares one tile at the end. Measured against
real geometry:

| matrix | whole-matrix opening | the tile actually disputed |
|---|---|---|
| BASE-0 `output` 4096 × 256 | 1.0 MiB | 16 KiB at `tile_len` 64 |
| Qwen2.5-1.5B `unembed` 151,936 × 1,536 | ~223 MiB | 192 KiB at `tile_len` 128 |

ADR-0046 sizes a court close well under 152 KB. The gap is two to three orders of magnitude, and it
scales with the model — which is the one thing the design promised it would not do.

**The operand API disagrees with itself about its unit.** The trait says
(`palw_step_refute.rs:617`) *"Little-endian raw bytes of `elements` values … in the tensor's own
dtype"*, so `elements` counts VALUES. The production oracle returns `elements` BYTES
(`palw_artifact.rs:187`, `operand.bytes[..elements as usize]`). The two coincide for exactly one
dtype width, and `PALW-BASE-0` is `int8` throughout — so the defect is invisible in the only class
that exists. `Rescale`, `Requantize` and `RopeTable` read multi-byte parameters through this same
oracle, so the first class with a wider operand adjudicates nothing and reports it as
`InputSetNotCanonical` rather than as the API mismatch it is. The test double `FixedRow` returns the
whole row regardless of the requested size, which is why no test sees it.

**Coverage passes on a class with unadjudicable steps.** `palw_step_refute.rs:394`:

```rust
if coord.call_index != 0 {
    return Err(PalwStepRefuteError::Unadjudicable);
}
```

Embedding is adjudicable at prefill only, and the reason given is correct: a decode token is not in
the prompt, so a challenger naming it freely would convict an honest producer. But BASE-0's own
canonical job is prefill 8 / **decode 4** (`palw_base0_profile.rs:442`). So the liveness floor
itself reaches steps its court refuses — while `verify_catalog_coverage_v1` reports 100 %, because
it compares *kernel ids* and every kernel BASE-0 reaches is catalogued.

**And four hand-written descriptions of one computation disagree.** The engine narrows after
`RmsNorm`, after each projection, after `RopeTable` and after each residual add; the profile's node
tables and the court's adjudicator carry only the un-narrowed op; the artifact inventory names
`attn_norm.weight`, `ffn_norm.weight` and `output_norm.weight`, which neither the engine nor the
court ever reads. Today that is inert because no worker commits real step legs. The moment one does,
the court recomputes a different value than the producer committed — which is a false conviction of
an honest producer, the one failure this court may never have.

The through-line is a single mistake made four times: **the coverage gate answers "is this kernel in
the catalog", and the question that decides whether a network works is "can the court close, on every
coordinate a class can reach, inside a bounded opening".**

---

## Decision A — operands are addressed in bytes, and the canonical encoding is pinned

`PalwWeightOracleV1::weight_row(tensor, layer, row_start, elements)` becomes

```
operand_bytes(tensor_name, layer, byte_offset: u64, byte_len: u32) -> Option<Vec<u8>>
```

Byte-addressed on both sides, with no dtype arithmetic anywhere in the oracle. Each catalogued
kernel's descriptor pins the canonical byte encoding of the parameters it reads, so
`byte_len` is a function of the node and its dtype rather than a number two implementations derive
separately.

*Why bytes and not "dtype + element count".* Either would remove the ambiguity. Bytes remove it in
the layer that is easiest to get wrong: a Merkle opening proves BYTES, so an oracle that speaks
bytes needs no conversion between what it proves and what it returns, and there is no second place
for the conversion to be written differently.

`FixedRow` and every other test double must honour `byte_len` exactly. A double that returns more
than it was asked for is the reason this defect had no failing test, and a double that ignores its
arguments is not a double of anything.

## Decision B — the terminal adjudication is tile-local

A refutation already names a coordinate `(call_index, node_slot, position, tile_index)`. The court
must open, and must recompute, only what that coordinate depends on:

* **operands:** the weight rows the tile's own outputs reduce over — `tile_len × in_len` values,
  never `out_dim × in_len`;
* **recomputation:** the tile's output lane only. Not every row followed by a comparison of one.

For a reduction over the input (`MatMulQuant`, `DotI8`) the tile's outputs are a contiguous slice of
output channels, so the opening is a contiguous row range and the Merkle path count is bounded by
the tile, not by the matrix.

This is the decision that makes ADR-0038 W1 true rather than aspirational, and it is what keeps a
court close inside the carriage budget ADR-0046 assumed.

## Decision C — admission bounds the court from the class's own geometry

Coverage PASS stops being sufficient for admissible. `verify_class_admission_v2` derives four
numbers from the shape profile and refuses a class that exceeds the ruleset's ceiling for any:

| bound | derived from | why it must be checked at admission |
|---|---|---|
| max opening bytes, per refutation and per court close | widest tile × its `in_len` × dtype bytes, plus Merkle paths | a proof that cannot ride a transaction is a dispute nobody can raise |
| max terminal MACs | the same tile's reduction length | terminal adjudication is a full node's own CPU cost, on peer-supplied input |
| max operand count | the node's `input_refs` and its weight operand | bounds deserialization work before any arithmetic runs |
| max Merkle path count | inventory depth × operand count | the same, for proof verification |

Each ceiling joins `PalwCourtParamsV2` and therefore `palw_ruleset_id_v2`, beside
`max_step_leaf_count`. They are frozen with the network for the same reason it is: a class deeper or
heavier than the ceiling cannot join a running chain, so the ceiling must be chosen once, at genesis,
for every class the network ever intends to admit.

*This is deliberately the same shape as the existing ladder gate.* `max_step_leaf_count` bounds how
many rounds a dispute takes; these bound how much each round costs. A ruleset that fixed one and left
the other free was bounding the number of steps in a walk without bounding the length of a step.

### Amendment, 2026-08-26 — the ceilings are numbers now, and the metric they are on changed

Decision C shipped as three fields on `PalwCourtParamsV2` with generous defaults, gated nowhere. The
road map called the result "the four cost ceilings … not yet gated", beside the ladder, under *the
two decisions that expire*. Choosing them turned out to require fixing what they measure first.

**What was measured, and what it should have been.** `derive_court_cost_v1` returned the artifact
opening — the weight bytes of the widest tile. A close is not its weight bytes. Assembled from real
executions of the shipped floor, one close weighed **750,716 bytes at a 64/64 job** against a derived
32,768: the difference is the disputed step's own leaves, and above all the KV history, which an
attention matmul reads at every position up to its own. A second arm was invisible the same way: a
decode-position gather must carry every logits row so the court can recompute
`base0_logits_trace_root_v1`, so it costs `decode_tokens × vocabulary × 4`. Both grow with the job,
and nothing charges an attempt more for a longer job, so **an attacker picks the job length** — the
rule `worst_case_step_leaf_count_v1` already states, applied to cost.

So the field is `max_close_bytes` and it counts what `palw_court_v2::arithmetic_close_bytes_v2`
counts on a real object: opened payload plus every Merkle path element proving it, artifact side and
step side alike. The ADR's own table said as much ("per refutation and per court close … plus Merkle
paths"); the implementation was narrower than the sentence.

**The fourth ceiling is the byte ceiling.** "Max Merkle path count" is not a separate field: a path
element is 64 bytes and both sides count it in the byte total, so a count would refuse exactly the
set the byte bound already refuses. Three fields, four bounds.

**The number, derived rather than chosen.** A close rides one `SUBNETWORK_ID_PALW_LIFECYCLE`
transaction — no chunked-evidence path carries a `PalwConsensusObjectV2` — and transient mass is
`size × 4` against a 480,000 standard limit, so a relayable transaction is 120,000 bytes. Allowing
18,000 for a carrier and the measured ~1.2× encoding overhead gives ceiling ≤ 85,000;
`DEFAULT_MAX_CLOSE_BYTES` is **80 KiB**. `assemble_palw_rc_identity_v2` gate 6 refuses an RC identity
carrying any other value, exactly as gate 5 does for the ladder.

**What it forecloses: nothing that was ever carriable.** Qwen2.5-1.5B's cheapest possible close is
1,220,368 bytes at any tile length and any context, because one row of a 128,256-token vocabulary is
513,024 bytes on its own — four times the largest standard transaction. A larger ceiling would not
buy that class; it would admit a class whose disputes nobody could raise, which is what Decision C
exists to refuse. Carrying a model at that scale needs an **openable** logits commitment (Decision E
says "O(1) in vocabulary"; the root is a flat hash over every row) and a per-layer slice of the
checkpoint the KV history arrives in. Both are code, not a bigger number in a genesis.

**What it cost the floor.** `PALW_RC_BASE0_GEOMETRY` was `vocab_size` 4,096 / `n_ctx` 512, whose
worst close is megabytes — the network's own liveness floor was coverage-clean, ladder-deep enough
and unprosecutable at its longest jobs. It is `1,024` / `12` now, chosen by the rule *the largest
pair whose worst close stays under 80% of the ceiling* (61,040 of 81,920), so that a later
leg-format change cannot make the floor inadmissible on its own network. The canonical job — 8
prefill, 4 decode — is unchanged.

**ADR-0030 §3's checkpoint anchor does not change this**, and the reason is worth recording: a
checkpoint covers a decode call, so a dispute at call `c` needs the one covering `c − 1`. None exists
at a prefill position or at the first decode call — which is exactly where the worst job's worst step
is. The anchor makes late-decode disputes cheap; the bound is over the coordinate that has no anchor.
A first draft of this amendment bounded the anchored form and understated the floor by 2×, which
`the_derived_close_cost_bounds_a_real_one` caught by assembling a real close and comparing.

## Decision D — coverage is over reachable coordinates, not over kernel ids

A class is adjudicable **iff every reachable `(call_index, node_slot, position, tile_index)`
adjudicates.** The kernel-id set remains necessary and stops being sufficient.

`verify_catalog_coverage_v1` keeps its job — it answers "can this build execute this kernel at all" —
and gains a companion that asks the question A4 actually needs: enumerate the coordinate classes a
profile reaches (prefill and decode calls, every node slot, the tile shapes each produces) and require
an adjudicating arm for each. C-04 is exactly what this catches: every kernel id is catalogued and a
whole call class is refused.

## Decision E — decode is adjudicable, by challenging the argmax rather than proving it

The `call_index != 0` refusal was right about the danger and wrong to stop there: on the free-prompt
lane the **generated text is the product**, so a class whose decode steps cannot be adjudicated cannot
carry the lane the network exists to sell.

The producer commits, per decode position, the logits vector's Merkle root — it already computes the
vector. The decode token is `argmax_lowest(logits)`, the tie rule the engine already pins. The court
never verifies the argmax, which would cost the whole vocabulary. It adjudicates a **challenge** to
it, and a challenge is one index:

> A challenger names `j` and opens `logits[j]`. The token is refuted iff
> `logits[j] > logits[token]`, or `logits[j] == logits[token] && j < token`.

One opening, one comparison, independent of vocabulary size. The embedding step at a decode position
then adjudicates against a token the chain has pinned, and the challenger who names a token freely is
refuted by the same opening rather than believed.

*Proving is expensive and refuting is cheap, so refute.* That is the same asymmetry the whole dispute
model rests on, applied to the one place it had not been.

> **Implemented 2026-08-22 — integer-first, both halves.** The selection rule is one pinned
> function, `base0_decode_token_select_v1` (argmax, ties to the LOWEST index), which the engine's
> decode loop and the court both call — the engine's `argmax_lowest` delegates to it, so
> "selected" cannot mean two things. On-chain refutation is the new close arm
> `PalwCourtVerdictProofV2::DecodeToken { binding, pin, position }` with fault
> `PalwStepFaultV1::DecodeTokenMismatch { position }` (evidence kind 6): the pin carries the
> integer class's logits rows and generated ids, authenticated by recomputing
> `base0_logits_trace_root_v1` against the binding the claim's own `execution_root` pins — the
> evidence is the commitment itself, no artifact opening involved. For BASE-0 the "one index"
> economy is unnecessary: its vocabulary is small by construction, so the pin carries whole rows
> (bounded by the court's opening-byte ceiling at the cost gate) and the court runs the whole
> argmax. A `Float32` class is refused by name — its per-position openings arrive with the class
> that needs them (Gate 3).
>
> The same pin closes the **integer-leg dispatch**: `full_logits_trace_root` is one header slot
> with two occupants, and the step-refutation decode check used to recompute only the v2
> event-tree root, so a BASE-0 decode-embed gather could never authenticate its generated ids —
> 4 of the floor's 914 leaves ended `Unadjudicable`. The check now dispatches on the class's
> registered `PalwStepLaneV1`, and the sweep
> (`the_court_convicts_no_leaf_of_an_honest_execution`) demands **914/914 adjudicated, zero
> unadjudicable** — a new hole fails the test by name. Money path verified end to end
> (`palw_v2_a_lying_decode_token_convicts_through_the_court_close`): the lying claim voids as
> `CourtFraud` and its bond is slashed; the honest one survives the same close with its stake.
> Mutation-checked in both directions: reverting the dispatch reddens the sweep, and re-tying the
> selection rule to the highest index reddens the tie test.

## Decision F — the engine, the profile, the adjudicator and the inventory are projections of one description

Decisions B and D require the engine's tile boundaries and the court's tile boundaries to be the same
object, not two objects that agree. Four independently authored descriptions of one computation is
how the narrowing steps came to exist in one and not the others.

Each class carries one canonical execution description. The shape profile, the engine's op sequence,
the adjudicator's node table and the artifact inventory are each **generated** from it, and a golden
test asserts the four projections agree — node for node, tile for tile, tensor for tensor.

Until the generator exists, the interim obligation is narrower and immediate: **no worker may commit a
step leg for a class whose profile does not name every narrowing the engine performs.** A commitment
the court cannot reproduce is a false conviction waiting for its first dispute.

> **Landed (2026-08-26, `09bd647f` / `df63a916` / `de1b67c1`).** The generator exists and the interim
> obligation is a gate rather than a rule.
>
> `misaka/palw/base0/src/plan.rs` compiles `BASE0_LAYER_IR` — and now `BASE0_PRE_IR` and
> `BASE0_POST_IR` — into the engine's op sequence: which step runs, in what order, reading which
> earlier step, against which operand, producing how many values. The kernels are untouched, because
> that arithmetic is the class; what is generated is what the divergences were actually made of.
> **No kernel is called in `engine.rs` any more.** `misaka/palw/base0/src/operands.rs` is the one
> binding from an IR tensor name to an artifact's bytes, keyed the way the court asks (template name
> + layer index), and the engine and the inventory both read through it. Both classes' profiles project
> every table — neither file writes a `PalwStepNodeV1`.
>
> Three properties now fall out of the IR instead of being declared beside it: a step's output TYPE
> (a narrowing emits codes, everything else accumulators — so feeding a Qk accumulator to a code
> kernel is a compile-time refusal), where the cache is WRITTEN (from the node roles, both halves
> committed together), and where the health diagnostics are measured.
>
> The interim obligation is enforced at `base0_execute_for_attempt_v1`: `base0_check_graph_v1` runs
> before the first token, at `kv_len` 1 **and** 2, because at one position a per-head width and a
> per-layer one are the same number. A class whose declared graph is not the graph this engine
> performs produces nothing.
>
> **What building the generator found, and it was live.** The inventory held its own copy of the
> name-to-field mapping, and one entry was already wrong: `attn_q.requant` served the tensor-wide
> `layer.requant[0]` unconditionally, while the engine narrows through the per-channel table whenever
> the artifact carries one — which is where a projection bias lives, in each channel's `zero`. Every
> Qwen2.5 member carries one. The failure is not a refusal: `palw_step_refute.rs:719` asks for
> `9 × channels` bytes, finds a nine-byte row, and **cycles it across every channel** — so the court
> recomputes an honest step from parameters the producer never applied and convicts. Silently, and
> only for classes with a bias, which is not the floor. Fixed by construction: the inventory serves
> what the resolver returns.
>
> Byte-identical where it had to be. Both classes' `shape_profile_id` are unmoved — the floor's
> `c185df95…` is in the RC genesis and a live testnet-12 — `the_engine_matches_its_golden_trace`
> stands, and the floor's pinned `artifact_root` does not move (the floor has no per-channel table).
>
> **And it is not slower**, which was the open question: this engine is on the block-production path.
> Measured A/B against the pre-change tree at the RC geometry, release, 160 forward passes each, twice:
> **1.69–1.72 ms/token against 1.74–1.75 ms/token**, about 3% faster. Dispatching a step costs a row
> copy that reading a local variable did not, and it is dominated by the projections — while the loop
> this replaced computed the value-weighted attention dot products **twice**, once into `attn` and
> once into the captured row, which the plan does once because the graph declares one node there.
>
> The golden test Decision F asks for is
> `the_four_projections_agree_and_a_real_execution_agrees_with_them`, at two positions, over all three
> tables, plus the live rows an execution produced. Its teeth are
> `a_divergence_the_width_check_cannot_see_is_named`: five mutations of the profile — wrong operand,
> wrong kernel, wrong input, wrong cache role, short table — each reported by slot and field. The
> three the old shape guard could not see are the first three, and each is a court that recomputes an
> honest step and convicts the producer.

## Decision G — the canonical artifact inventory, and one meaning for "class id"

`artifact_root` is the Merkle root over a canonical inventory manifest, one leaf per operand row.
Each entry carries:

```
tensor name · layer · dtype · shape · byte_offset · byte_len · quantization record · order index
```

and the manifest is refused for any of: a duplicate entry, a missing tensor the profile names, an
overlapping byte range, a byte of the artifact no entry covers, or a non-canonical order. "Every byte
is covered exactly once, in one order" is what makes an opening's absence meaningful.

> **Open, found by landing Decision F (2026-08-26): a tied-head class has no canonical inventory.**
>
> Qwen2.5 ties its embeddings, so its lm_head reads `token_embd.weight` and no `output.weight` exists.
> `base0_inventory_v1` always emits the head as a tiled `output.weight`, so a tied class's inventory
> carries a tensor no step can open — and the tensor its head *does* read is carried in the wrong row
> shape. A gather emits one row per token id (`d_model` bytes); a `MatMulQuant` opening asks for
> `tile_len × d_model` bytes at a tile offset; `operand_bytes` serves only an exact
> `(name, layer, row_start)` match at exactly the requested length. So the head adjudicates
> `Unadjudicable` at every tile, and `qwen25_admissible_geometry_v1` searches upward from `tile_len`
> 64 — the one width where a gather row and a matmul tile coincide is never chosen.
>
> This is a Decision G question, not a Decision F one: one tensor cannot carry both row shapes
> without overlapping rows, which the canonical layout refuses on purpose. Deciding it needs a rule —
> a second row family under a derived name, or a head that opens through the gather's coordinates —
> and that is a rule about what an opening addresses. What Decision F bought is that the class now
> says so: `a_tied_head_class_is_named_by_the_check_rather_than_registering_quietly` reports it as
> `RowNobodyOpens { "output.weight" }` instead of the class registering quietly and finding out at
> its first dispute.

This also settles the two things called a class id. **`execution_class_id` is the shape profile id** —
a class is its graph, which is what the chain already keys on. The artifact digest is
`artifact_root` and is a separate value with a separate job: the graph says what is computed, the root
says what it is computed against. A flat hash of a whole artifact is neither, because nothing can be
opened against it.

## Decision H — post-genesis class registration is allowed, at the minimum share, gated by Decision C

Three policies coexist today: the lifecycle carriage refuses `ClassRegistered` outright
(`palw_lifecycle_objects_v2.rs:104`), `verify_class_admission_v2` would admit at the minimum grantable
share, and the state machine implements a weightless activation clock. Consensus does not benefit from
three answers.

The carriage's objection is the correct one and it is a statement about *checking*, not about
*forbidding*: a class entering a live chain moves the share table and brings its own `pwu_rule`, and
nothing checked either. Decision C is that check. So:

**a class may register post-genesis, at `min_grantable_share_permille`, iff it passes coverage over
coordinates (D), the four cost bounds (C), and the derived-pwu rule the genesis loader already
enforces.** The carriage's refusal is replaced by that gate rather than removed, and the object gains
the shape profile the gate needs — which it must carry anyway, because nothing else on a running chain
can tell the court what the class computes.

---

## Consequences

* **The 1-tile claim becomes true, and must be re-measured before it is repeated.** Every statement
  of the form "a full node adjudicates one tile without holding the model" is unsupported until
  Decision B lands and the opening sizes are measured on real geometry.
* **Four ruleset parameters are added, and freeze with the network.** Choosing them is a genesis
  decision with the same expiry as the ladder: too small forecloses classes, too large admits a
  proof nobody can verify in time.
* **Existing golden values move.** The operand API change (A) and the inventory (G) both change what
  is hashed. Nothing is registered yet, which is why this is the moment.
* **`FixedRow`-style doubles must be re-written before they can witness anything.** A test double
  that ignores the size it was asked for cannot detect a size defect, and one did not.
* **The audit's NO-GO stands until B, C and D land.** A class can be coverage-PASS, ladder-admissible
  and still have no closeable court, which is precisely the state measured today.

## What this ADR does not decide

* **The residual amplification.** Whether `BASE-0`'s residual add gains an amplifying `Rescale` is a
  separate decision, pending its own measurement; it changes the arithmetic, not the adjudication
  contract.
* **Qwen3.6's MoE / GatedDeltaNet / SSM primitives.** New ops need their own accumulator and state
  bounds, which is an ADR of its own. This one is a precondition for it: a new op set is not worth
  specifying against an adjudication contract that does not close.
* **Performance.** Nothing here is a throughput decision, and no bound below may be relaxed for one.

## Mission alignment amendment — 2026-10-07

adjudication contractはcourt kernel・material manifestだけでなく、普通のpublic bondが公開取得・局所化・terminal filingを完結できるcontractへ強める。契約したmaterialのhashだけを、証拠が取得できることの証明としない。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0049-palw-adjudication-contract.md -->

</details>

<a id="source-0069-e2e-adjudicability-is-the-price-of-weight"></a>

<details>
<summary>0069-e2e-adjudicability-is-the-price-of-weight.md — 原文 L1–L498</summary>

[原文を開く](../adr/0069-e2e-adjudicability-is-the-price-of-weight.md) · SHA-256: `4c48dadb0765704bba4459b8adc7c23c1a126f72b9486f17e95900c65d506954`

<!-- ADR_SOURCE_BEGIN:0069-e2e-adjudicability-is-the-price-of-weight.md -->
~~~~~~~~markdown
# ADR-0069 — End-to-end adjudicability is the price of weight

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **ACCEPTED — IMPLEMENTED (2026-09-01; reviewed and amended 2026-09-02).** Landed on
`palw-adr0068-phase2` in `cfe1feeb` (the certificate, the root, the gate, the zero-share state),
`1ac8f3f3` (drill and grade split into two verbs), `d2e63039`/`62da1a61` (the model tiers certify
and take their cadence back), `f23f1348` (the genesis invariant test can see genesis classes),
`758c1568` (certification buys weight, not existence), and the two review fixes recorded in
"What landed". Written against the ADR-0068 launch audit, whose central
finding was that Relaunch 5's genesis share table hands **97.8% of cadence** (QWEN36 489‰ +
QWEN25-A16 489‰) to families that carry no court responder — `supports_court()` is `false` for
both, `bisect_prefix_state`/`refutation_for_index` take the trait's defaults, and a producer that
never runs the model cannot be convicted (audit F2/F6/F8). Builds on ADR-0039 (a class is
weightless until its kernel catalog closes), ADR-0049 (the adjudication contract), ADR-0067
(classes are chain data; only kernels are the build), and ADR-0054/0056 (share follows production;
permissionless admission). Consistent with the standing doctrine that consensus changes ship by
activation, never by re-genesis.

> **Amended by [ADR-0075](0075-certification-is-a-consensus-object.md) (2026-09-02).** Decisions
> 2 and 5 assumed the certified set is the build's (`palw_rc_court_e2e_root_v1`, pinned). ADR-0075
> makes certification a consensus object — `FamilyCertified` (a family drill re-graded by the court
> in the transition) and `ClassLaneCertified` (a class bound to a lane) — so the set the weight
> gate reads is genesis ∪ chain state (`certified_families`, `fp_certified_families`,
> `fp_certified_classes`) and a new family is certified by objects, not by a fingerprint move.
> Decision 6 (registration stays permissionless; weight requires certification) is unchanged and
> is the route ADR-0075 §7 spells out for mainnet. [ADR-0073](0073-real-demand-work-bears-the-weight.md)
> narrows a certificate's scope to the lane it was drilled on. Map: [`README.md`](README.md).

> **Security amendment appended (2026-09-02) — Decision 7.** The review's open item ("a weightless class's block still adds pwu") is a fork-choice hole with shipped numbers behind it; the last section closes it: an uncertified family's blocks weigh nothing in either chain weight.

## 1. The goal, stated as a test

PALW's one claim is that its blocks are paid for by *actual* LLM inference. The claim is only as
strong as the chain's ability to **convict** a producer who did not do the work. Today that ability
is asserted, not enforced:

* `supports_court()` is consulted in exactly two node-local places — `kaspad/src/palw_producer.rs:601`
  (it prints a warning and keeps producing) and `kaspad/src/palw_panel.rs:1785` (it buckets a stall
  reason). It appears in **no** consensus rule. A family can answer `true` or `false` and the chain
  pays it the same.
* The static conformance battery (`misaka-palw-sdk`'s `check_lineage_v1`) proves the *profile* is a
  walkable step space whose every kernel the adjudicator re-executes. It never instantiates a
  backend, so it cannot see that the backend cannot actually **play** the court.

So the chain grants fork-choice weight to families it cannot prosecute. The test this ADR wants the
chain to pass:

> **No family carries fork-choice weight unless this build can take that family's own backend,
> on a real anchor, all the way through a dispute — `execute`, `verify_material`, `bisect_prefix_state`,
> `refutation_for_index`, close — and have the close convict a planted fault and acquit the honest run.**

Weightless registration stays permissionless. **Weight** is the thing that must be earned by being
certifiably convictable.

## 2. What is already true (measured, not planned)

The design is half-built, which is what makes this an ADR and not a research program.

* **The static half exists and is unforgeable by construction.** `verify_catalog_coverage_v1` /
  `verify_profile_coverage_v1` (`consensus/core/src/palw_catalog_coverage.rs`) prove per-kernel and
  per-node-shape adjudicability, and their certificate is constructible *only* through the sealed
  constructor (`PalwReachableKernelSetV1`'s private `_sealed` field) — so "we checked coverage" and
  "a certificate exists" are one fact. The result is committed as the bundle's `court_catalog_root`
  — this build's adjudicable primitive set as one hash (`palw_catalog_coverage.rs:173`). This is the
  exact mechanism ADR-0067 uses to make adjudication a property of the build that two honest nodes
  compute identically.
* **BASE-0 already carries the end-to-end drill.** `misaka/palw/base0/src/backend.rs`:
  `execute_with_injected_fault(job, prompt, leaf)` (:189) produces *guilty* material; then for the
  honest and the guilty run alike, `refutation_for_index` opens (:528, :568) and
  `bisect_prefix_state` before and after the fault leaf shows the two executions agree *before* and
  differ *at* it (:575-579). This is precisely the certification vector — for one family.
* **The gap is three-fold.** (a) Nothing requires that drill of anyone. (b) For QWEN36 and
  QWEN25-A16 the court methods are the trait defaults (`consensus/core/src/palw_backend.rs:176,193,205`
  return `None`/`false`/`Err`); `bisect_prefix_state` is implemented in exactly one place in the
  whole tree (BASE-0, `backend.rs:239`). (c) Admission cannot even *express* weightlessness:
  `granted_share_table_v2` refuses a zero grant by construction — `min_grantable_share_permille` is
  at least 1 (`palw_class_admission_v2.rs:38-42`) — so "register it weightless" is, in today's code,
  a fiction: every registered class already gets ≥ 1‰.

## 3. Decision

### Decision 1 — Two adjudicability properties, named apart

**Static adjudicability** — the profile is a walkable step space whose every reachable kernel and
every node shape the build re-executes. Already defined and already gated (§2, first bullet).

**End-to-end adjudicability** — a real backend, given a real anchor, plays a real dispute to a
conviction. Not defined anywhere today.

Weight requires **both**. Registration and liveness require only the static one (ADR-0039's
"admissible for liveness, weightless").

### Decision 2 — E2E certification is a build fact, committed like the catalog

Mirror `court_catalog_root` exactly. Introduce `court_e2e_root` (name provisional): the hash of the
set of **family descriptors** this build has certified end-to-end, where a certificate is
constructible *only* by running the drill of Decision 3 to a conviction — a sealed constructor, the
same pattern as `PalwReachableKernelSetV1`. Two honest nodes agree on the root because both compute
it from the same build.

This keeps ADR-0067 intact: the **class** (its profile and artifact) remains chain data; the
**build** remains the authority on adjudication — previously over kernels, now over the whole court
turn. The `court_e2e_root` rides the bundle beside `court_catalog_root`.

### Decision 3 — The certification drill, defined

For a family `F` and its canonical job, `F` certifies iff, for a **covering leaf set** `L`:

1. `execute` yields honest material and its four roots.
2. `execute_with_injected_fault(job, prompt, ℓ)` yields guilty material for every `ℓ ∈ L`.
3. For the honest run and every guilty run, `refutation_for_index(material, ℓ)` opens (`Ok`), and
   `bisect_prefix_state(material, i)` is a true prefix commitment — two runs that agree through
   index `i` return the same state at `i`, and two that first differ at `ℓ` return equal states for
   `i ≤ ℓ` and unequal for `i = ℓ+1`.
4. The **actual court** resolves it: `adjudicate_court_close_v2` → `check_step_refutation_v1`
   convicts each guilty run at its planted leaf and acquits the honest run. Not a re-implementation
   of the court — the drill drives the shipped adjudicator.
5. Everything step 4 reads is *available*: the retained material carries the per-step tiles and the
   binding, **or** the checkpoint leg re-captures them (`Base0CheckpointCaptureV1::push_chunks`
   against `next_geometry`), and either way the close fits under the carriage close ceiling
   (tiled, never flat — a Qwen-scale logits row is ≈ 993 KiB against an 80 KiB ceiling).

The passing vectors from a certification run **are** the family's regression test and its
certification evidence — the same artifact serves both, as BASE-0's drill already does.

`L` must be a *covering* set: it must include a leaf in every table the profile declares
(`pre` / `gdn` / `attn` / `post`) and at least one prefill and one decode position. A fault at a
leaf `L` omits is a step the family could diverge on unconvicted, so a smaller `L` is a smaller
guarantee and the certificate must record the `L` it vouched for (as coverage records the reachable
set it vouched for, not the catalog snapshot that covered it — `palw_catalog_coverage.rs:129`).

### Decision 4 — The graph must not lie about the engine (ADR-0049 Decision F, enforced)

A family certifies only if the narrowings its engine actually executes equal the narrowings its
graph declares. This needs, per engine, a `plan()` and a `check_graph` — the counterpart BASE-0 has
in `base0_check_graph_v1` and A16 lacks (`A16Engine` has no `plan()`; its `pre` table performs an
**undeclared requant** that lifts the embedding onto the A16 stream — `misaka/palw/base0/src/legs.rs:113-119`).
The drill of Decision 3 would catch an undeclared step anyway (the bisection diverges at it), but a
graph check names it at build time instead of at a conviction, and turns "the class misdescribes its
engine" from a live-fire discovery into a compile error.

### Decision 5 — The admission gate grants weight only to certified families

Two changes, both local:

1. **Make weightlessness expressible.** Permit a genuine zero-share admitted state — a `share = 0`
   grant, or a `liveness_only` flag on the registration — removing the `min_grantable ≥ 1` fiction
   for uncertified families. A weightless class still produces blocks, is still gossiped and stored,
   still contributes to liveness; it simply earns no cadence and cannot dilute the certified share
   table.
2. **Gate the weight-bearing grant.** `verify_class_admission_v2` / `granted_share_table_v2` refuse
   a nonzero grant unless the registered `PalwRegistrationTermsV2.family` is present in this build's
   `court_e2e_root` set. The judging material is already on chain — `family` is a registration term
   — so this is a set-membership check, not a new object.

### Decision 6 — Permissionlessness and the doctrine are preserved

Anyone still registers any model with a bond (ADR-0054/0056). Certification is **not** a maintainer's
signature; it is a mechanical property of a build, so a third party who ships weights, an adjudicable
backend, and the passing vectors certifies a family without asking anyone — exactly as ADR-0067
intends. Turning a family weight-bearing later is an **activation** (the `court_e2e_root` moves the
way `court_catalog_root` does), never a re-genesis (standing doctrine).

## 4. What this fixes, and what it costs

**Fixes.** Audit F6/F8 directly — the unconvictable 97.8% can no longer hold weight, so a producer
that never runs the model earns nothing whether or not anyone disputes it. It also drains the pool
two other findings drank from: the clone-flood (F4) that moved cadence onto model-free classes gains
nothing if those classes can't hold weight without certifying a real backend, and the round-0
silence acquittal (F2) stops paying, because silence on an uncertified family's dispute defends a
weightless claim.

*Amended on review (2026-09-02): the F4 sentence overclaims.* Certification generalises over the
reachable KERNEL SET (§"What landed", Decision 2), so a clone that declares a certified family's
graph over a root nobody holds is covered by that family's certificate and joins at
`min_grantable` like any prosecutable class. What such a clone cannot do is produce — no node can
serve it — so it earns nothing and ADR-0054's reclamation returns its seat; the dilution it costs
meanwhile is ADR-0054's problem, not this gate's. The sentence stands for clones of *uncertified*
families, which is the case the audit measured.

**Costs.** The drill is CI-time and already exists for one family. The root is one hash on the
bundle. The admission change is a membership test plus a zero-share state. The real work is
per-family — making QWEN36 and A16 actually certify — and that work is already scoped by ADR-0067's
A16 items (the checkpoint leg, the rung methods, the graph check) and by the manual that accompanies
this ADR (`docs/misaka-palw-model-adjudicability-guide-v0.1-ja.md`).

## 5. Considered and rejected

* **Trust `supports_court()`.** Rejected: it is node-local and lie-able, and the audit shows it is
  already `false` where it matters while cadence flows regardless. A build fact committed into the
  ruleset root is the only form two honest nodes cannot disagree about.
* **Forbid registration of uncertifiable families outright.** Rejected: it breaks ADR-0039's
  deliberate "admissible for liveness, weightless" state and the permissionless test — a family must
  be able to exist and produce while its adjudication is still being built. The gate belongs on
  *weight*, not on *existence*.
* **Certify off-chain with a signed attestation.** Rejected: it reintroduces the central party
  ADR-0067 exists to remove. The build, not a signer, is the authority.
* **Charge the accuser for an unanswerable dispute (the pre-audit behaviour).** Rejected already by
  ADR-0067's `rearm_after_unanswered_opening`; noted here because it is the failure this gate
  replaces — the old chain made prosecuting an uncconvictable class a guaranteed loss, which is why
  the classes shipped unprosecuted.

## 6. Invariants to verify at each step

1. **No unearned weight.** For every Active class, `share > 0 ⇒ family ∈ court_e2e_root`. A test
   over the shipped genesis table and over a synthetic post-genesis registration.
   *Amended on review (2026-09-02):* "weight" here is CADENCE — the share table and the epoch
   budget it derives. It is not the fork-choice `pwu` a block carries: a weightless class still
   produces its one floor block per epoch (`derive_epoch_budgets_v2` floors every budget at one),
   and that block's claim contributes `pwu` to `safe_weight` when it reaches `Final`, exactly as
   any other. Fork choice orders by the safe frontier first, so the contribution is a tiebreaker,
   but §1's sentence "no family carries fork-choice weight" is stronger than what is enforced;
   what is enforced is that an unprosecutable family cannot take a slice of the cadence, and
   cannot hold more than the liveness floor's one block. Making the floor block's `pwu` zero for
   a zero-share class would close the remainder and is left as a follow-up, recorded rather than
   implied.
2. **The covering set is covering.** The drill fails if `L` omits any declared table or omits a
   prefill or a decode position — asserted as a *difference* (a class that passes with a full `L`
   must fail with an `L` missing one table), so a collapsed `L` cannot pass silently.
3. **E2E ⊆ catalog.** `court_e2e_root`'s families are a subset of what `court_catalog_root` covers —
   you cannot be end-to-end certified for a kernel you cannot even catalog.
4. **Weightless is still chain data.** A `share = 0` class round-trips through carriage, IBD and
   pruning byte-identically to a weight-bearing one; only the grant differs.
5. **Genesis honours the gate.** Relaunch 5's share table assigns weight only to families in the
   shipped `court_e2e_root` — which, until A16 certifies, is `{BASE-0}`. QWEN36 and QWEN25-A16
   register weightless until their certification lands, then take weight by activation.
   *As landed:* the zeroing happened (`a1ea9a4b`) and was reversed the same day (`62da1a61`) on
   the terms this invariant states — both model tiers certified (their step spaces adjudicate
   leaf by leaf, ADR-0070), the shipped `court_e2e_root` is the three-family set
   `{BASE-0, QWEN36, QWEN25-A16}`, and the genesis table is 489‰/489‰/22‰. The invariant is
   asserted by `the_shipped_genesis_grants_weight_only_to_certified_families`, which reads the
   real `Params` a node boots with; `verify_palw_genesis_v2` itself does not run the weight gate
   (a genesis registration carries no admission carriage), so at genesis the invariant is a
   build fact held by that test, exactly as `court_catalog_root`'s coverage is.

## What landed

Everything the tracking list named, and two things the review found it had not said.

* **Decision 2 — `court_e2e_root` and the sealed certificate.** `consensus/core/src/palw_e2e_adjudicability.rs`:
  `PalwE2eCertificateV1` (private `_sealed`, no `BorshDeserialize`), `PalwE2eFamilyV1` (the
  descriptor: family id, drilled class id, reachable kernel set, covering), `palw_court_e2e_root_of_v1`
  (sorted digests, count-prefixed). The root rides `PalwConsensusParamsV2::court_e2e_root` beside
  `court_catalog_root`, is refused unset by `validate`, and is PINNED for the RC networks
  (`palw_rc_court_e2e_root_v1`) rather than read from the process registry — a value that depended
  on whether a drill had run when the params were assembled would be an order-dependent identity.
  The set consensus actually reads is `palw_rc_certified_families_v1`, derived from profiles every
  node holds plus the pinned facts only a drill measures (drilled class id, convicted leaves); the
  base0 crate's `pin_tests` assert this build's drill reproduces it family by family and root for
  root. *Generalisation is by kernel set:* a class may hold weight iff ONE certified family's
  drilled kernel set contains the class's reachable set — never the union, since a class is served
  by one backend and two certificates stitched together vouch for a graph nobody ran.
* **Decision 3 — the drill, and its two verbs.** `misaka/palw/base0/src/e2e_drill.rs`:
  `drill_family_evidence_v1` (needs the model: honest run, one planted fault per (table, call
  class) the profile reaches, both refutations through the same prover, the prefix rung around
  the fault, the operand openings, and the malformed-material arm — nine truncations/extensions
  of the family's own honest material through `verify_material`, none of which may read `Matches`)
  and `certify_e2e_family_v1` (needs only the shipped adjudicator: re-runs
  `check_execution_step_refutation_v1` over the recorded objects, requiring `NoFaultFound` on the
  honest side and a conviction on the guilty side at every leaf, then scores the covering). The
  split is what lets a family whose weights are tens of gigabytes be drilled once, where the
  weights are, and graded anywhere — `PalwE2eDrillEvidenceV1` is borsh on purpose, the
  certificate is not. The model tiers are drilled on fixture geometries that reach the same
  kernel sets as the production classes (measured: 23 for the hybrid, 12 for the dense tier).
* **Decision 4 — the graph must not lie.** Enforced on the evidence rather than trusted to the
  capture: `GraphMisdescribesTheEngine` refuses a binding whose committed step space is a
  different size from the declared graph's enumeration. The A16 requant the ADR names is declared
  by the corrected class (`qwen25_a16_profile_v2`; ADR-0070).
* **Decision 5 — weight, and only weight, is gated.** `verify_class_admission_v2` takes the
  certified set as an argument and requires it to hash to the bundle's root (a pure function whose
  input is still not the caller's opinion); a nonzero share on an uncovered class is
  `NotEndToEndCertified`. `granted_share_table_v2` admits a zero grant, exempts an already-zero
  incumbent from the donor floor (without it a chain could hold at most ONE weightless class), and
  `derive_epoch_budgets_v2` floors every budget at one block so a weightless class still produces.
  The acceptance path (`processor.rs`) fixes an entrant's share as a function of its own graph:
  `min_grantable` if some certified family covers it, `0` otherwise — exactly one value either
  way, so a registrant still cannot choose (Decision H). The SDK's preflight and registration
  builder derive the same value, so a node reports "weightless" rather than "refused" for a family
  nobody has drilled.
* **Decision 6 — permissionless in, still permissionless.** `758c1568`: the first cut of the gate
  and Decision H's "exactly `min_grantable`" together closed the door on every uncertified family
  (measured: 44 catalogued kernels, 37 drilled, so a class reaching one of the other 7 could
  neither take weight nor register). The zero-share entrant is that door.
* **The round-0 silence exemption** (audit M2-5, Gate 3-3) reads the same fact: a claim whose
  class holds share is on a certified family and its silence is a default like any other; a
  weightless claim keeps the exemption, because the reason for it stands.
* **The node.** `kaspad` registers the build's certified families at boot, logs the set, and warns
  when its root is not the network's (the handshake is what enforces it — the root is inside
  `consensus_params_id`).

### Found on review and closed (2026-09-02)

* **The growth rule was a second door into weight.** ADR-0054's `derive_class_share_growth_v1`
  steps by `max(1‰, share × g / 1000)`, and a weightless class's budget is floored at one block —
  so producing that block read as "filled its allowance" and a zero share stepped to 1‰ at the
  next boundary, then grew 25 % per epoch, on the shipped ruleset (`class_growth_permille = 250`).
  The admission gate never saw it, and with `class_shares > 0` selecting the round-0 conviction,
  the honest producer of an unprosecutable family became slashable for a silence it could not
  break. Reproduced at the pure rule and at the transition; closed at the rule (a zero share is
  skipped by the growth arm — zero is a certification state, not a small share), with a
  transition-level test beside a share-bearing control that must still grow.
* **The certifier did not bind the vectors to the evidence's graph.** The kernel set, the covering
  and `drilled_class_id` were read off `evidence.profile`; the verdicts off each refutation's own
  binding; nothing compared the two beyond the leaf count, which a kernel-id change does not move.
  A drill of one graph could therefore be filed as evidence for another and mint a certificate
  vouching for kernels no court re-executed. The consensus path never consumed a certificate, so
  the chain was not exposed; the mechanism's own promise (§2, "unforgeable by construction") was.
  Closed: every vector's binding and job context must name the evidence's class id
  (`VectorIsAboutAnotherGraph`), with a test that files the floor's real vectors under a same-shaped
  sibling graph and is refused by name.

### Still open

* A weightless class's floor block still carries its claim's `pwu` into `safe_weight` (invariant 1,
  amended). Pricing that block at zero `pwu` for a zero-share class would make §1's sentence
  literally true; it is a fork-choice rule change and is not done here.
* The ADR-0067 sidecar fence (`--palw-chain-classes`) and the deployment of the ruleset this ADR
  moved are operator decisions, recorded in `docs/palw-step-space-deployment-notes.md` and the
  Relaunch 5 runbook.

## Security amendment (2026-09-02) — the open item is a fork-choice hole, closed as Decision 7

The review note left one item open: "a weightless class's block still adds pwu to fork choice;
pricing it at zero is an undone fork-choice change". Read against the shipped tree it is not a
rounding matter.

**The attack, from shipped numbers.** (1) Registration is permissionless; a family whose kernels are
all in the catalog but not all drilled registers at share 0 (Decision 5). (2) A post-genesis
entrant's `initial_target` is the base class's live target (the processor's M2-12 rule: "the
entrant's difficulty is the chain's"); on testnet-11 5e the floor's seed is `MAX/12,663`, so the
entrant's block claims `expected_draws ≈ 12,663`. (3) `pwu_per_inference` is the class's own counted
leaves, admitted up to `PALW_STEP_MAX_LEAVES = 2²²`; the hybrid tier already declares 2,685,360. One
such block's `pwu = expected_draws × per_inference ≈ 12,663 × 4.19×10⁶ ≈ 5.3×10¹⁰`, against
≈ 4.4×10⁶ for an honest QWEN36 block and ≈ 2.1×10⁹ for a whole class's epoch. (4) A share-0 class
still has an epoch budget of one block. (5) For an uncertified family a fabricated execution
commitment cannot be convicted — that is what "uncertified" means — so each draw costs one hash,
not one inference; 12,663 hashes is nothing. (6) The panel that licenses it is drawn from bonds that
declared capability for the class (ADR-0071 Decision 3 — a signature and nothing else); for a class
only its registrant can run, those are the registrant's own bonds, at 400,000 sompi each. One
fabricated block per epoch then carries ≈ 8× the fork-choice weight the entire honest network
produces in that epoch, in `safe(C)` once its own panel licenses it to `Final` — the weight the
IBD and deep-reorg gates read.

**Decision 7 — an uncertified family's blocks weigh nothing.** A block whose class's family is not
attempt-lane certified at that block's chain point (genesis ∪ chain, ADR-0075 Decision 4)
contributes `pwu = 0` to both `safe` and `live`, whatever its ramp stage.

*Where, corrected 2026-09-02 at implementation.* This paragraph first said "in `chain_weights_v1`'s
inputs", which is the right rule at the wrong address, and an implementation that believed it
closed the defect while leaving the live network open. `chain_weights_v1` is the V1 path and, on a
V2 network, only a SEARCH ORDER — `palw_tip_weights_v1` says so in its own comment
(`processor.rs:9822`). The authority is the state fold: `palw_state_v2.rs:2819` builds
`PalwCandidateOrderV1::new(safe_frontier_blue_score, safe_weight, bounded_immature, candidate)`,
and those three are hashed into the state root. So the predicate belongs at **both** addresses, and
on the fold it belongs at every accumulation site — `safe_weight` at a claim's `Final`,
`bounded_immature` at each recomputation, and `retired_safe_weight` when a claim retires — through
ONE helper (`palw_class_bears_weight_v2`), because `assert_internal_consistency` re-derives
`safe_weight == retired_safe_weight + Σ Final` and a second spelling of the rule makes the fold
refuse states it has just built. A consequence worth stating: the free-prompt lane's own
`safe_weight` contribution is a separate decision, not a corollary, and must be taken explicitly.

The weightless→bearing crossing has no path through the share-growth walk in any case:
`derive_class_share_growth_v1` refuses to grow a share-0 class (ADR-0069's own review fix — a zero
share is a certification state, not a small one), so a class becomes weight-bearing only by being
granted share, never by drifting into it. The block is otherwise unchanged: it advances DAA, it is paid its budgeted subsidy
(ADR-0039's "admissible for liveness"), its claim runs the same lattice. Weight is what
certification buys — Decision 5's sentence, applied to the quantity it was written about. This is a
fork-choice rule and moves the ruleset id: it ships in the next ruleset move and is a mainnet
precondition.

**Where the rule lives — this text said `chain_weights_v1`'s inputs, which is right as a rule and
wrong as a location.** `chain_weights_v1` is fed by `palw_facts::resolve_block_weight_v1`, and on a
`ConsensusV2` network the heap that reads it is a SEARCH ORDER, not the authority — the processor's
own `palw_tip_weights_v1` says so. The authority is the V2 **state fold**: `PalwChainStateV2` keeps
`safe_weight` and `bounded_immature` as running totals, hands both to `PalwCandidateOrderV1::new`,
and hashes both into `palw_state_root`.

**Where the rule BINDS, counted honestly — an earlier draft of this paragraph said "three places,
all through the one helper `palw_class_bears_weight_v2`" and that was false for two of the three.**
It binds in exactly two, and both are the V2 state fold: the claim's `immature_contribution` at
creation (the `live` half) and `safe_weight` at the claim's `Final` (the half the IBD and deep-reorg
gates read), the second through `palw_claim_safe_contribution_v2`, which is the helper's only
wrapper. The predicate implemented is the class's **granted share**, which admission makes a sound
proxy for certification in the one direction that matters — `verify_class_admission_v3` refuses
`share > 0` to a family that is not certified end to end, so `share > 0 ⇒ certified`.

The V1 class resolver is removed: its credit overlay is not used on V2. V2 admission and certification checks above remain in force.

Two consequences are residuals rather than choices, and are pinned by tests rather than left to be
discovered. **(i)** The safe half is priced at the finalizing block, so a claim accepted while its
class was weightless and finalized after the class took cadence is paid in full; freezing the
decision on the claim record is a claim-encoding change, hence a state-version bump and a re-mint.
**(ii)** Because of (i), `retired_safe_weight` must NOT re-ask the question at the retirement block —
it accumulates `claim.pwu`, the most the retired claims could have carried — and the internal
consistency identity `safe_weight == retired + Σ Final` therefore becomes an upper bound while the
fence is armed. Asking twice instead makes the two totals drift apart permanently, which shows up
as every pruning-point import being refused for a state that obeyed the rule exactly.

**(iii) The free-prompt half of the same retirement was not a residual, it was a hole, and the fence
carries its repair.** A free-prompt claim's spent quanta really do move `safe_weight`
(`apply_receipt_spend` adds `pwu / quanta` per spend), but `retire_claim` folded only `Attempt`
claims, so a retiring free-prompt claim with any spent quantum left that weight in `safe_weight`
with nothing left to re-derive it. The live fold never notices — it does not run the check — but
`PalwStateCarriageV2::into_state_v2` does, so the node **refuses its own durable tip on the next
restart** (`load_tip` → `CarriageInconsistent`), and every peer importing that pruning-point
snapshot is refused with it. Reachable on any ruleset with `claim_retirement_daa > 0`; the shipped
RC sets it to `WINDOW_COURT` = 3000, so it is a live rule and not a theoretical one. Unlike (ii)
this amount is EXACT — the free-prompt lane is not priced by Decision 7 at the spend or in the
re-derivation, so `per_quantum × |spent|` is the same expression on both sides and no share table is
asked at either point. It is fenced with Decision 7 rather than shipped bare because
`retired_safe_weight` is hashed into `palw_state_root`, so the repair moves the root of any block
that retires a spent free-prompt claim. **Dormant, the hole is still open**, and arming Decision 7
closes it — for retirements that have not happened yet. See (v): it closes nothing behind itself.

**(iv) What the armed bound is worth, and what it is not.** It stops a snapshot from claiming more
safe weight than its own LIVE claims could have justified. It does not bound `retired_safe_weight`,
which is a carriage-supplied scalar with no re-derivation in either fence position, and it does not
tie `class_shares` to any certificate — the only checks there are that the key set matches the
non-`Registered`/non-`Dormant` classes and that the table sums to 1000‰, so a peer may declare its
own uncertified class `Active` at 1000‰ and be weight-bearing by this fold's own predicate. Nor is
the `state_root` check in `into_state_v2` the authority an earlier note claimed: `palw_state_root`
is not checked at header validation, and in a pruned IBD the witness header carrying the expected
root arrives from the same peer as the carriage, so the check authenticates carriage-against-header,
not carriage-against-rule. What stands behind an imported weight is the headers-proof and PoW race
for that child. Closing either hole needs the weight-bearing decision frozen on the claim and a
certification field the share table can be checked against — both claim/class encoding changes,
hence a `PALW_STATE_V2_VERSION` bump and a re-mint.

**(v) Arming is a fix going forward and NOT a recovery, so the fence is genesis-only and the code
refuses anything else.** A chain that retired a spent free-prompt claim while the fence was dormant
carries the stranded weight in `safe_weight` with the claim that would re-derive it already gone.
Arming later cannot repair that, and the reason is not a missing feature: `retired_safe_weight` was
never credited, so `safe_ceiling` sits BELOW `safe_weight` and the armed `<=` bound refuses the
state for the same arithmetic reason the dormant equality does; the fold carries both scalars past
the activation untouched, so every later state inherits the refusal; and a node holding such a tip
cannot start at all (`load_tip` → `CarriageInconsistent`), so it never reaches the activation height
to be healed by it — nor can it fetch a healthy one, since the peers' pruning-point carriage carries
the same scalars. All three positions are measured by
`arming_the_fence_does_not_repair_a_chain_that_already_stranded_weight`
(`consensus/core/tests/palw_adr0069_d7_fold.rs`). Repairing such a chain means writing a state BELOW
the fence differently, which moves roots already committed — that is a re-mint, not an activation.
Therefore: **arm `palw_uncertified_weightless` at the genesis of a fresh relaunch, or leave it
dormant.** `Params::validate_palw_v2` refuses an activation above `genesis.daa_score` on every
preset and every lineage, so a node configured with a rolling activation height does not start
(`ConfigBuilder::build` panics) — an operator following this ADR cannot reach the wedged state by
doing what it says. The cost of arming at genesis is stated in (ii) and (iv) and is paid for the
network's whole life: the import-side identity is a bound, not an equality, from block 0.
*And what genesis-only permanently costs* (measured 2026-09-03, `params.rs:1030`). Two consequences
follow from the refusal above, and neither is optional:

* **A chain that launched without it can only be fixed by a re-genesis.** That is why testnet-11's
  Relaunch is the mitigation for 5e rather than one option among several, and why the relaunch's
  schedule is a security property and not only a delivery one.
* **On mainnet this is a one-way door.** Mainnet ships `PalwConsensusMode::Disabled`; the day its
  genesis card is set, either this fence is armed in that genesis or the network can never carry
  Decision 7 at all — because the standing doctrine forbids a mainnet re-genesis, and this rule
  refuses every other arming. So "arm it at mainnet genesis" is not a deployment preference, it is
  the only moment the option exists. A carded mainnet without it is a mainnet where an uncertified
  family's fabricated block outweighs the honest network permanently.

Two companions, so Decision 7 is not the only wall: **(a)** a `BondCapabilityDeclared` set is
bounded and priced (ADR-0071's security amendment); **(b)** ADR-0076 §8's field is pinned to the
base target by the processor today — recorded there — and seating (ADR-0076 Decision 4) re-prices it.

Invariants: **E1** a block of a share-0 or uncertified class contributes 0 to both chain weights at
every ramp stage; **E2** the same block's subsidy and DAA advance are unchanged; **E3** a class that
becomes certified mid-chain gains weight only for blocks after the certifying object's chain
point — never retroactively (a retroactive gain would let a certification reorder history).

## ADR-0144 alignment (2026-09-21)

The remaining hole this ADR still names — a zero-share class's floor block still adds pwu to fork
choice — is **not** closed by giving that block more synthetic weight, a new share, or a special
PALW-work unit. [ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) §6
item 5: shrink Attempt; do not grow it. E1–E3 stand. The floor block's leftover pwu is closed by
making synthetic work the less attractive fallback, then replacing its beacon role, not by a new
weight-bearing exception.

## Mission alignment amendment — 2026-10-07

将来のE2E certificateは、producerと共有するcaptureなしのfresh non-seat prosecutionを証明対象に含める。既存family drillのsealed certificateは旧範囲の実装記録として保持し、ordinary public-bond completenessへ自動的に拡張しない。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0069-e2e-adjudicability-is-the-price-of-weight.md -->

</details>

<a id="source-0173-public-verifier-dispute-completeness-is-misaka-purpose"></a>

<details>
<summary>0173-public-verifier-dispute-completeness-is-misaka-purpose.md — 原文 L1–L232</summary>

[原文を開く](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) · SHA-256: `6d0d70756a07758e18e577008af4ba443e52ea904fde2055a3081884351bfeac`

<!-- ADR_SOURCE_BEGIN:0173-public-verifier-dispute-completeness-is-misaka-purpose.md -->
~~~~~~~~markdown
# ADR-0173 — MISAKA's purpose: public-verifier dispute completeness

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**トークンの表示名は Misaka、ticker は BILI。** `misaka` 系アドレスプレフィックスと既存のprotocol/CLI/API識別子は維持する（[ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

* **Status:** Accepted as the project's design mandate at the user's request, 2026-10-07. Implementation and activation remain pending. This decision assigns no wire id, consensus fingerprint, activation height or deployment.
* **Purpose:** 正しい固定modelを保有/取得できたPanel外のpublic bondが、producer秘密状態なしにclaim固有の認証証拠から不正をlocalizeし客観的convictionへ到達する。モデル入手自体はchainが管理・保証しない（ADR177）。
* **Design path:** [RFC-0014](../rfc/0014-panel-independent-fraud-prosecution.md) specifies the implementation work and completion gates. [RFC-0015](../rfc/0015-panel-free-permissionless-verification.md) specifies a separately gated, deferred Panel=0 mode.
* **Complements:** [ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md)'s useful local-inference purpose, [ADR-0171](0171-probabilistic-constraint-checks-and-court-on-dispute.md)'s verification direction and [ADR-0172](0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)'s kernel-only extension boundary.
* **Audit:** [All-RFC/ADR alignment report](../evidence/adr/0173-mission-alignment-audit-2026-10-07.md). The report records document versions, dispositions and the limits of this documentation change.
* **Amendment, 2026-10-10:** 旧Seeder/TRDC/FPR可用性方針は、後続の[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)で撤回した。従来の整合チェックは履歴で、新配分の経済/実装証拠ではない。
* **Further amendment, 2026-10-10:** 旧Seeder/TRDC/FPR可用性方針は、後続の[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)で撤回した。従来の整合チェックは履歴で、新配分の経済/実装証拠ではない。
* **Amendment, 2026-10-14 (user decision, [PRINCIPLES §0](../PRINCIPLES.md)):** MISAKAの**最終Goal**は「モデルを非公開にしておくことが経済的に不利になる仕組み」であり、不正の完全排除ではない。本ADRの公開検証者による不正追及の完結は、その下の制度要件として維持する: **登録モデルが1人にでも共有され、正しいPanel（正直な検証者）を建てられれば、claim固有の公開・認証済み証拠から不正を局所化し、客観的なcourtで立証してslashできる。** 誰にも共有されない閉鎖モデル（`p = 0`）の不正は排除を保証せず、ADR-0176のbond単位の上限で被害を有界化する残余リスクとする。検出確率の下限と期待利益が負であることは有効化gateではなく評価指標である。

## 1. MISAKAの中核目標

**後続改定:** 以下の公開検証目標は、検証者が正しい登録modelを得られた条件で成立する。
model配布は合意の管理・強制対象にせず、旧無条件取得保証をADR177で撤回する。

MISAKAは、利用者がローカルで実行する有用なLLM計算を、公開に検証可能なワークとして扱うP2P基盤を目指す。その安全性の中核目標は、**固定Panelの多数派を信用しなくても、普通のpublic bondを持つ独立参加者が、公開され認証された証拠だけで不正を特定し、合意規則が検証するconvictionとproducerへの責任追及まで完結できること**である。

Panelが不正なclaimを一度licenseしても、その多数決で計算上の矛盾が正当化されてはならない。ライセンスは処理状態であり、算術的な正しさの最終根拠ではない。Panelの正直な多数派、特定のgenesis operator、producerと同じプロセスやローカルディスクを、安全性の前提に置かない。

これは設計目標の採択であり、現在のコードがこの条件をすべて満たしたという宣言ではない。「public verifierが実際に不正を発見して証明できる」と「正直な参加者が存在すれば必ずすべての不正を即座に発見する」は別の性質である。

## 2. 用語と適用範囲

| 用語 | この決定での意味 |
| --- | --- |
| 普通のpublic bond | 公開されたidentity・bond・maturity・collateral規則を満たす参加者。Panel非所属でも参加可能で、genesis allowlist、operator anchorの特権、モデル所有者の承認を必要としない。無資源・無担保での無制限な訴追を意味しない。 |
| public authenticated material（ADR177のscope） | claimのinput/state/trace/output/witness等の有限な認証証拠。modelはoff-chainで得た固定rootのbytesを前提にし、model weightsの取得を強制する公開義務にはしない。hash/成功署名だけでは計算検証を代替しない。 |
| producerの秘密状態を使わない | producerだけのcapture、未公開FOLD prefix、KV cache、tile preimage、RAM、ファイル、秘密鍵、内部prover API、producerインスタンスへの参照を訴追の前提にしない。検証者自身の計算・メモリ・秘密乱数は使えるが、convictionは公開検証可能でなければならない。 |
| localize | 全体の異常や検査失敗を、認証された入力・境界・状態に拘束された有界の不正箇所とterminal proofへ落とすこと。leaf番号の報告だけでは完了しない。 |
| objective conviction | 任意の合意検証ノードが、規範的なcourt規則と証拠から同じ有罪結果を計算すること。Panel投票、LLM回答の良し悪し、ローカルHTTP失敗を有罪の根拠にしない。 |
| public-verifier dispute completeness | 許可されたclass/profileの不正について、外部参加者が異常を発見した後、認証された公開証拠から局所化と客観的裁定を有界に完結でき、必要証拠の非開示でその経路を永久に塞げない性質。 |

目標は各承認済みKernel・VerificationPlan・task/context・privacy profileに適用する。任意の未対応モデル、任意の自然言語回答の真偽、未定義のfaultまで裁定できるという意味ではない。登録者が不正箇所を検査対象から外すことも認めない。

## 3. 規範的決定

### D1 — 訴追の権利はPanelへの選出と独立

普通のpublic bondは、Panelに選ばれずとも証拠を取得し、検査し、規範的なdemand/accusation/court/terminal proofを提出できなければならない。Panel bind・receipt・seat schedulingは通常処理の担当割当であり、外部検証者の証拠アクセスや有効なfraud proofの受理条件ではない。

現行のoperator-anchor特権やseat専用APIは移行対象として扱う。新しい設計に永続的な安全性前提として持ち込まない。Sybil対策の公開bond条件や有界の訴追費用は維持するが、少数の指定operatorによる許可と混同しない。

### D2 — 必要な証拠を、producerの善意に依存させない

登録・claimの新しいprofileは、必要証拠のcommitment、取得単位、認証、応答責任、保持期間、deadline、最大bytes/workを定義しなければならない。公開openingから、検証者自身の状態で異なる出力を発見し、**producerが実際にcommitした異なる値**を開いて裁定できることが必要である。正直な検証者のreplay値で、不正producerのopeningを代用しない。

全captureを全ノードに複製する必要はない。ローカル保管・streaming・Merkle/FOLD opening・第三者DAは利用できる。ただし、必要箇所が公開に取得できない場合には、有界の認証された非開示手続で解決できなければならない。「captureは家にある」ことを、producerの私的状態にしかアクセスできない訴追設計の理由にしない。

旧`PanelDa`/`claim_readers_v2`の「producer・bound seats・すでに開かれたcourtのchallengerだけが読める」という条件は、この目標を満たす新しい報酬profileの根拠にはならない。courtを開くために不正の発見が必要なのに、発見に必要な入力をcourt開始前に読めない循環を残さない。公開証拠で裁定できない秘密入力profileは、別の明示的に承認された公開検証可能な証明方式が同等の局所化・裁定・非開示条件を満たすまで、新しい報酬経路で有効にしない。ZK等の方式をこのADRで採用済みとは扱わない。

### D3 — 検出・局所化・裁定・回収を別々に証明

新しい受入試験は次の経路を、独立したpublic bondで連続して成立させる。

```text
公開されたclass / claim / authenticated material
  → 外部public verifierによる不正の発見
  → commitmentに拘束された有界の局所化
  → exact courtによるobjective conviction
  → claim void / 規則上のFinal reversal
  → producerの回収可能な担保・escrowへのslash
```

算術的不正については、必要証拠が開示されればその算術的不正を裁定できることが必要である。必要証拠を非開示にする攻撃については、認証されたdemand、責任あるresponse/disclosure、規範的なdeadline/default裁定の経路を持つことが必要である。非開示のconvictionを、未証明の算術的不正のconvictionと呼び換えない。単なるping失敗・切断・verifier不在をproducer fraudと扱わない。

検査失敗、fault ledger、leaf番号、court kernelの実装、`supports_court()`、既存のfamily certificateのいずれか一つだけで、この連続した性質が証明されたとは扱わない。whole-job replayを使う場合も、公開の入力から始まり宣言した最悪時資源内に収まる必要がある。巨大な無界replayを隠れた最終手段にしない。

### D4 — 多数派のlicenseは有効な客観的証拠を覆せない

新設計では、Panel多数派または全席が悪意でも、客観的な証拠が提出されれば、それを再度Panelに投票させたり、Panelの許可待ちにしたりしない。有効なproofの処理を無関係なopen courtで封じない。重複提出・競合court・reorgは、一度限りの決済と原子的なstate/escrow/lock更新で扱う。

`ReceiptLicensed`と`Final`は、証明不能なclaimを真実に変えない。Final後の責任期間、証拠保持、proof受理、担保解放、報酬回収・reversalを整合させる。無期限の訴追を約束するのでなく、設計で定めた有界期間内の公平な取得・提出・包含機会を確保する。

producerへのconvictionと、false `Valid`へのseat責任は別に証明する。Panel seatをslashする場合は、そのseatが署名したscheme/scope/evidenceの主張と客観的な偽りの関係を示す。別scopeの正しい部分検査に署名したseatを、claim全体が不正だったことだけで自動的に有罪にしない。

### D5 — 報酬・weightの前提をdispute completenessまで強める

新しいclass/profile/kernel/planのmineability・報酬・consensus weightの有効化には、この外部訴追経路の成立を必要条件とする。登録、source fidelity、kernel catalogへの収載、独立したseat readiness、market開設、通常の正直claimのFinalは、いずれも代わりにならない。

受入証拠は、そのprofileが許す最大context・shape・recurrence・MoE・state boundary・dissection/close量と、対応する全fault種類を対象にする。小さい既知fixtureや一つのfamilyの成功だけを、canonical大規模profile全体の証明にしない。未対応の局所化・opening・裁定は、missing capabilityとして報告し、その新しい報酬profileを閉じたままにする。

これは将来のversioned gateである。文書変更によって既存ネットワークのclassを失効させたり、過去のcertificateを再解釈したりしない。旧claimは旧規則で再生し、移行条件を別途指定する。

### D6 — 検出の確率と、発見後の客観性を混同しない

[ADR-0171](0171-probabilistic-constraint-checks-and-court-on-dispute.md)の全constraintを対象とする承認済み検査、明示的なsoundness composition、post-commit乱数、residual errorの方針を維持する。courtは発見されなかった不正の確率を消さない。「1 honest seatが存在する」「quorumを集める」「少数のraw trace leafをsampleする」だけを、全不正の検出保証にしない。

外部参加者が異常を実際に発見し、そのprofileの訴追資源を用意し、規範的期間内に有効な証拠を包含できる条件の下で、その後の局所化・convictionを多数派の善意に依存させない。チェーン全体の検閲耐性・clock・DA・経済的監視継続性も別の必要条件であり、「一人で安全」の一言で省略しない。

外部public verifierは、producerとの共有秘密、選ばれたseatだけの秘密preprocessing、監査者の単なる主張を必須にしてはならない。private sketch等を通常処理の最適化として使う場合も、外部検査・局所化・公開court証拠への別の完結した経路を持つ必要がある。

### D7 — Panel=0は完成した基盤の上でのみ検討・有効化

Panelは移行中の担当分散・通常の検査・receipt・経済的監視供給を担い得る。目標はその正直な多数派への依存を除くことであり、このADRはPanelを今すぐ削除する決定ではない。

**RFC-0014の全completion gatesが実環境で成立するまで、RFC-0015のPanel=0は有効にしてはならない。** 成立した後も、RFC-0015固有のpending/maturity、positive verification、DA、exposure、fork-choice、監視の誘因、負荷・migration試験と明示的activationが必要である。`panel_seats=0`への設定変更、quorumの空集合成立、silent acceptanceでは代替できない。ADR-0061のzero-seat **genesis**は、固定Panelによる検証を廃止するPanel=0とは別の意味である。

Panel=0の方向は、固定Panelをpermissionless verifierとobjective fraud proofへ移行すること。検証自体をなくしたり、challengeがなければ必ず正しいと扱ったりすることではない。

### D8 — Post-commit challengeを外部参加者も完全再構成する（2026-10-08）

新しいKernel/model/claim/EXEC slice/public-check経路は、[RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)の唯一のpost-commit protocolをbindする。ordinary outsiderは公開のpolicy、statement/prover-message commitment、将来のqualifying PALW useful workとFinal/settlement provenanceから、同じsource順序・seed・query/vector・各interactive transcriptを再構成できなければならない。producer/Panelから配布されたseedを信頼しない。

beaconはcommit済みstatementのchallengeを決めるだけで、semantic/constraint/court/public-material/resource completenessやG14を代替しない。モデル登録はStatic Admission → Beacon Conformance → Active Eligibilityに分け、RegisteredDormantやsampled conformance PASSを報酬資格と混同しない。Kernel作者が選んだfixtureだけの一致もKernel soundnessの証明にはならない。

sourceはconsuming challengeと独立に有効な将来canonical workに限定し、candidate self-source・循環verification・free header reroll・committee/DNS/BFT beaconを排除する。複数Final workのmixだけでunbiasedと主張せず、source unpredictability、grinding、withholding、retry/reorgのboundsを立証する。source不足はpending/既定deadlineで扱い、heartbeat・BASE-0・取引・chainを止めたり弱めたりしない。

同じseedを多数watcherが検査しても独立試行とは数えない。mismatchは認証localization・exact courtへ進め、probabilistic failureだけでconvict/slashしない。有効なexact fraud proofを出すために新beacon待ちを要求しない。challenge再現・公的material・objective outcomeの全経路をG14に追加し、RFC15のPanel=0はその達成後も別gate/activationを要する。この追記は実装・activationの宣言ではない。

### D9 — 永久不変のモデル同一性と入手への合意不介入（2026-10-10後続改定）

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を適用する。
**チェーンはモデルを入手できるかどうかに一切干渉しない。** 登録は自由、登録後のModel ID/root、
weights・tokenizer/config・実行仕様は永久不変とする。取得者は認証piece/shardと全量復元rootを照合する。
claimは同じ固定Model ID/root/specをbindし、root一致とminer計算の正当性を区別する。

MISAKA Torrent採用・専用SeederとSeeder報酬の概念を廃止する。一般的なP2P/mirror/共有は任意の
off-chain運用とし、合意によるSeeder登録・選出・取得監査・
PoR/lease/TRDCを撤回する。モデル非公開・全peer停止・第三者への提供拒否だけでclaim/reward/weightを止めず、
bondをslashしない。モデル取得成功を合意が保証するという旧AND契約も撤回する。
任意のinfohash/manifest bindingを登録する場合は固定rootとの対応を不変にし、必須の取得経路にしない。

### D10 — モデル別拘束miner元本とcoinbase配分（2026-10-10後続改定）

model別の適格拘束元本を`C_{b,m}`、`S_m=sum_b C_{b,m}`とし、同じbondのmodel割当合計は
実効拘束元本以下にする。claim予約件数・rho・root/slice/rider・鍵分割で元本を増幅しない。
model ownerの市場seed/Position/AMMやPanel/Seeder担保も自動算入しない。
`A_m=f(S_m)`、正の分母では`R_m=R_PALW*A_m/sum_j A_j`として既定の総coinbase予算内で配分する。
分母0はmodel配分0、整数残余と未使用枠を規範化する。資本預託だけでは報酬を払わず、有効claimとFinalを要求する。

公開参加が資本と配分倍率を大幅に増やし、閉鎖自己資本より圧倒的に有利になることを経済設計の目標にする。
同額`S_m`なら自己/外部資本の配分は同じであり、所有者の独立性をウォレット数から認定しない。
倍率曲線、競合・自己増資・Sybil/集中・閉鎖時の検出率と既存minerのcapを含め、利益比較で立証する。
既存minerの実収入増や公開が常に利益最大になることは未保証である。

モデル予算と個別bondのQ/B/R/F上限・共通DAA拘束を同時に適用する（ADR176）。
model配分変更だけでblock頻度、DAA、難易度、fork choiceを変えず、Final/courtと全writer/readerを別に実証する。

### D11 — FPR撤回と条件付きpublic prosecution（2026-10-10後続改定）

旧FPR三段階、モデル全量公開、配布default/重大Slash、代替Seeder、公開費予約、
availability起因CHALLENGED-weight停止を撤回する。モデル配布を合意のcourtへ持ち込まない。
claim固有の認証state/trace/output/witnessの適格要求・保持・客観的計算不正/defaultは維持するが、
weight-file/range、全量列挙や反復要求をモデル取得の強制へ転用しない（ADR177 D2）。
必要なoperandの認証と許可unit/累積scopeを実装前に確定する。

G14は、verifierが正しい固定modelを実際に得られた条件で公開証拠からlocalize/convictする能力を検証する。
**全model所有者が提供を拒否しても第三者が必ず取得できるという旧保証は撤回する。**
モデル未取得は計算不正・qualification停止・Slashの根拠ではない。実効検出`p=0`の場合も経済評価へ含める。
適格なclaim裁定が未解決ならそのFinal規則を適用するが、配布不能だけの停止へ読み替えない。
Panel=0、倍率式・経済優位・新会計/weight・court scopeは未実装/未立証であり、別activationを要する。

### D12 — Bondが時間内の採掘能力と総経済・consensus creditを制限する

[ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を、確率的検証・公開prosecutionと
組み合わせるPALWの追加必須前提とする。モデル計算はbondで制限された枠を使うための正当な仕事であり、
同額bond・同DAA期間・同倍率なら高速偽造でclaim発行能力、ブロック、報酬、Final後の確定weightの上限を増やさない。
claim容量を増やすときは一件の権利を細分化/配分し、bond当たりの総影響を維持する。
受理時予約、共通`d + W`までの早期回復禁止、全報酬/weight経路の帰属、残存責任の担保保持を要求する。

Final前のweight capだけでは成立せず、Final/retired/reversalと全settlement readerまで同じ予算を適用する。
不正反証で報酬失効と適用規則上の担保損失を執行するが、同じ共有担保をclaimごとに回収可能として重複計上しない。
実効検出・徴収率と計算費用を測定し、同じ上限だけで正直な計算の経済優位を証明したと扱わない。
この決定はproducer採掘予算に適用し、public challengerの別責任・費用を消さない。

## 4. RFC/ADR間の優先順位

1. 将来のPALW設計で、Panel-only evidence、honest-majority arithmetic authority、producer-private prosecution、kernel-only adjudicability certificate、無条件のPanel=0と本ADRが衝突した場合は、**本ADRとRFC-0014/0015の厳しいcompletion/activation条件を優先する**。
2. ADR-0172の承認済みversioned Kernel + 宣言的VerificationPlanを維持する。任意のverifier upload、model/fraud-proof VM、TEE validity trust、BFT orchestrator/PKI/committee authorityを代替依存として復活させない。ML-DSA/Hash64/UTXO、既存EVMやclock/fork-choiceの個別規則は、別の改定なしに変更しない。
3. ADR-0144の利用者の有用なローカル推論を対象にする目的は維持し、安全性の中核要件として本ADRを追加する。modelの回答の品質・source fidelity・実行の一致・不正の訴追可能性は別々に示す。
4. 過去のStatus、measurement、実装記録、fenceは履歴として保持する。該当文書の先頭bannerと末尾amendmentが、今後の設計における読み替えを指定する。「Implemented」と書かれた旧bodyだけから、本目標の達成を推論しない。
5. モデル入手の合意不介入とmodel拘束miner元本によるcoinbase配分はADR177を優先する。旧Seeder/Torrent/PoR/監査/lease/TRDC/FPR gateと配布起因weight停止を撤回し、固定登録identityとclaim固有courtのscopeを維持する。
6. public prosecutionのモデル取得前提はD11/ADR177 D7の条件付き保証へ改定する。全保有者拒否時の取得保証と閉鎖時の正の検出率は宣言しない。取得失敗だけで資格停止/SlashやFinal延長を起動しない。
7. D12とADR-0176を全将来PALW採掘経路の共通前提とする。class/work量・claim数の増加から未予約の報酬・block・Final weightを得る経路を改定し、既存実装や旧claimの記録を新保証の完成証拠としない。

## 5. 完成の証拠

RFC-0014のgatesを、少なくとも次の条件で満たすまで本目標を「完了」と呼ばない。

* fresh process・別bond・Panel非所属で始め、producerの内部オブジェクト、captureディレクトリ、既知fault位置を渡さず、公開したclass/claim/materialから不正の発見とlocalizationを行う。
* 正しいmodelを得たverifierで、Panel多数派および全席の偽`Valid`でlicenseしたclaim、served but falseな認証trace/state/FOLD、偽output、held/recurrent/checkpoint境界、fused tile、prompt/weights/identity mismatch、claim固有証拠の非開示を対象にする。正直なclaimにfalse convictionが出ないことも確認する。
* 実際の公開transport、bond admission、demand、court、terminal transition、producerの回収可能なslash、必要なseat責任まで確認する。liar自身の`ServedView`を外部取得の代用品にしたfixtureをこのgateの成功証拠にしない。
* bytes、work、rounds、deadline、保持・再取得、in-flight load、証拠包含、正しいnon-fault応答、duplicate、Final/reversal、reorg/IBD/pruning、activationを宣言した上限で確認する。
* 9B-8kや対応を主張するlong-contextなど、実際のcanonical最大profileで測る。証明可能性と回収可能額・監視インセンティブを区別し、不足担保を文書上のslash額で埋めたことにしない。
* RFC14 §16.8のnon-interference/元本/配分試験、§16.10のweight/規則E、§16.11の公開/閉鎖ECON・MEASを満たす。取得監査を資格gateへ戻さず、条件付き検出率と現実の計算/通信費を測る。
* ADR177の許可claim証拠unitと累積scopeを検証し、モデル取得要求を裁定から排除する。全peer停止でも同一chain/proofの合意結果が変わらず、正しいmodelを持つfresh verifierがclaimを裁定できることを確認する。
* ADR-0176の同額bond/同期間/同倍率での正直な計算対高速偽造、容量拡大時の総量保存、Final weight全経路、共通拘束、分割/再登録/期間境界、徴収可能損失と経済評価を満たす。

## 6. 確認したコードの位置づけ

確認基準は公開`main`の`43f0bcb362d37cba79414940f3fdd368d40d3377`。ローカルworkspaceのruntimeは別の旧HEADであり、この文書群の取り込みはruntime更新を意味しない。

| コード上の経路 | この目標に対する位置づけ |
| --- | --- |
| `palw_court_v2.rs`のcourt opening、`palw_offence_attribution_v1.rs`のExecutorRefuted | 客観的証拠を扱う土台。すべての公開profileで外部参加者がその証拠を作れる証明ではない。 |
| `palw_state_v2.rs`のconviction funnel、licensed/final claimのvoid/reversal、producerのslash、PanelFalseValid | license後の責任追及経路の土台。proof生成、期間、回収可能な額、署名scopeと合わせて評価する必要がある。 |
| `palw_operator_da` | Panel外の検査・DA accusationの土台。開示された自己整合的な偽traceの算術convictionまで、DA accusationだけで完了したとは扱わない。 |
| `palw_filer_held` / `palw_filer_held_e2e` | canonical held-8kのfresh verifierで、dense量、FOLD prefix、committed fused tileに残るgapを記録する。内部producerインスタンスによるserved materialの代用は完成証拠にならない。 |
| class/TIR admission、現行Panel basisとseat lock | 静的なcost/court検査や現行receipt条件である。新しいpublic-verifier completeness gateやPanel=0を文書だけで追加したことにはならない。 |

このADRは必要な実装の優先順位と受入条件を固定する。コード変更、試験によるgate充足、deploymentは別の作業である。

## 改定 — 2026-10-12（Panel=1でもnon-seatの公開訴追を維持：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §4。

通常のreward-bearing modeを1 claim 1 seat（Panel=1）とする場合でも、non-seatの証拠取得・裁定権限を制限しない。Panel署名を計算の真実の最終根拠にしない。producerと唯一のseatが共謀した場合、必要なモデルを保有したnon-seatが公開証拠から反証でき、Panel署名で妨害されないこと（受入試験P04）を要求する。外部の実検査が一切ない場合の未検出リスクは残余リスクとして明示し、`p = 1` と報告しない（P05）。

## ユーザー方針 2026-10-15 による改定

* 中核目標(非Panelの公開bondによる客観的conviction)は維持する。これに加え、**合意creditは公開告発の機会が経過した`SecurityCertified`のclaimだけが持つ**。G14の公開告発window(異議・convictionなしの経過)は認証の必要条件である。
* `p = 0`(閉鎖モデル)の残余リスクは経済的な損失に限られ、合意finalityにはならない。

**ユーザー方針(原文):**「この攻撃を塞ぐ方針で進める。DNS-BFTはRFCにある通り廃止したので使用できない。」— ここでの攻撃は、閉鎖モデルまたは自前Panelのモデルの未認証claimが`Final`に達し、safe frontier・safe weight・live total・settlement depth・rule E participation・settled anchorを進めて、不正な合意またはsettlement finalityを得ること。補足判断(同日):「抽選した検証者の承認を認証の柱にしている。これは承認にするということは前提にするのでそのままでいい。」

* **`Final`と`SecurityCertified`は別状態である。** `Final`は手続きの完了(利用者払いescrowの経済的settlementは既存規則どおり)。合意credit(safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来の報酬leg)は`SecurityCertified`のclaimだけが生む([RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md) §7、Gate C)。
* **認証は前提に基づく。** claim固定後の乱数でglobal poolから抽選した検証者のm-of-k肯定承認と、G14公開告発windowの異議なし経過で成立する。「抽選されたverifier stakeのうち正直な割合が閾値以上」を明示的な前提とし、無条件の数学的証明とは扱わない。暗号学的な計算証明は使わない(PRINCIPLES §8)。認証済みでもG14で反証されたclaimはvoidされ、合意creditは取り消される。
* **DNS-BFTは廃止済みであり、安全性にも収束にも使わない。**
* **genesisモデルにも認証の免除はない。70Bモデルは、独立した検証(抽選検証者による再実行)が成立するまで合意creditを持たない。**
* 本改定は追記であり、上の本文は履歴として残す。実装は休眠(fence `palw_independently_work_certified_v1`、全presetで`None`、有効化は別途のユーザー判断)。
~~~~~~~~
<!-- ADR_SOURCE_END:0173-public-verifier-dispute-completeness-is-misaka-purpose.md -->

</details>

<a id="source-0183-g14-court-capacity-panel-floor-retention-detection"></a>

<details>
<summary>0183-g14-court-capacity-panel-floor-retention-detection.md — 原文 L1–L23</summary>

[原文を開く](../adr/0183-g14-court-capacity-panel-floor-retention-detection.md) · SHA-256: `1b44afbca30f920d11b27c4d9e0b41b999e85dccf83c99f809092ed527dadbf5`

<!-- ADR_SOURCE_BEGIN:0183-g14-court-capacity-panel-floor-retention-detection.md -->
~~~~~~~~markdown
# ADR-0183 — G14 裁定容量・Panel 下限・裁定保持期間・検出方針(ユーザー決定の記録)

- **Decision:** Accepted — ユーザー決定 2026-10-16〜17 の実装記録(ADR-0181 の手続きによる記録)。
- **Scope:** G14 の裁定容量、Panel の最低担保、court を要する class の trace 保持期間、検出方針の claim gate。
- **Implementation:** branch `integ/g14-court-capacity-r2`(dormant fence。全 preset で None、t12 の identity は不変)。
- **Verification:** targeted test(`g14_court_capacity`、`g13_panel_liability`、`g14_detection_gate`、node `lg14a_legacy_filer_e2e`、`g14_kernel_route_e2e`)。全体 suite は未実行。
- **Activation:** なし。各 fence は測定記録と別途の有効化決定を要する。
- **Related:** [G14 完全性証明 §11](../audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md)、[ADR-0181](0181-design-freeze-and-controlled-change.md)。

## 背景と決定

1. **裁定容量(`palw_court_capacity_v1`)。** 1 手の予算は別枠にする。court を要する claim には、最初の 1 行分の予約 pool を claim の寿命中 1 回だけ割り当てる。受理数は hard-deadline bucket ごとに M = ⌊reserve/2⌋ 件で上限を設け、超えた claim は skip 可能な `CourtClaimAdmissionFull` で受理しない。H より 1 service window 以上前に受理されたのに裁定されなかった行は無報酬とする(Final 後は Final を取り消し、vesting を burn する)。
2. **Panel の最低担保。** G-13 が有効な間は、最低 seat 担保を 240,000 MSK とする(測定した健全時の最小値 238,119 MSK に基づく)。draw、bind、room、activation、seating はすべて同じ下限関数を読む。
3. **trace 保持期間。** court を要する(fused-attention)V2 Attempt は、licence から 4W + W_disclose 以上の保持を約束する(t12 では 14,568 DAA)。約束しない Attempt は報酬対象にしない。
4. **検出方針(`palw_detection_policy_v1`)。** claim gate は、その claim の実際の保持期間に対する裁定予算を読む。公開検証されていない kernel claim の種類(pipeline、typed Composite/Retrieval)は受理しない。

## 採用理由・代替案

いずれもユーザー決定である。値は測定記録から導出した。決定にない値は作っていない。

## 帰結

暗号学的な計算証明は使っていない。ピーク時の Panel 容量は未解決である(ピーク時に必要な担保は 1,696,051 MSK)。
~~~~~~~~
<!-- ADR_SOURCE_END:0183-g14-court-capacity-panel-floor-retention-detection.md -->

</details>

<a id="source-0187-g14-native-public-filing-and-post-final-mining-accusation"></a>

<details>
<summary>0187-g14-native-public-filing-and-post-final-mining-accusation.md — 原文 L1–L59</summary>

[原文を開く](../adr/0187-g14-native-public-filing-and-post-final-mining-accusation.md) · SHA-256: `0d6d4c12d124430a21f5019e1ea794227e27dcc622d31e9749e8054f0d91c504`

<!-- ADR_SOURCE_BEGIN:0187-g14-native-public-filing-and-post-final-mining-accusation.md -->
~~~~~~~~markdown
# ADR-0187 — native 経路の G14:枠なしの公開告発、期限切れ seal の解放、mining claim の Final 後の追及

- **Decision:** Accepted — ユーザー決定 2026-10-17:G-N2 の実装値(Final 後の demand は生涯 8・同時 2、attestor の lock は seat-lock 関係を claim の reserved exposure に当てた額、責任期間は他 lane と同じ式)をそのまま採用する。最終の drill・測定で問題が出た場合は見直す。
- **Scope:** native funded v11・text29 の告発枠(G-N1)、owning39 mining claim の Final 後の追及(G-N2)、担保不足での有罪(G-14 / T-N13)。
- **Implementation:** branch `integ/g14-court-capacity-r2`。各 native lane の既存の休眠 fence の下(全 preset で `None`)。testnet-12 の identity は不変。
- **Verification:** v11 の node 試験 5 件、v39 mining court の fold 試験 4 件(G14 完全性証明 §13)。text29 と v39 の実 job の試験は harness が無く未実行。
- **Activation:** なし。各 native lane は測定記録と有効化の判断を要する。
- **Related:** [G14 完全性証明 §13](../audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md)、[ADR-0183](0183-g14-court-capacity-panel-floor-retention-detection.md)、[ADR-0181](0181-design-freeze-and-controlled-change.md)、[RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md)。

## 背景と決定

1. **枠なしの公開告発(v11 action 19、text29 action 21)。** 自己完結の filing は、producer の operator 以外の Active bond が、seal・ticket・owner 席なしで出せる。作業料は前払いで、誤りなら没収する。producer の予約は使わない。検証と有罪は、seal の reveal と同じ関数を通る。commit-reveal は無いので、写しが先に入れば finder share はその提出者に渡る。producer は同じように有罪になる。
2. **期限切れ seal の解放(v11 action 20)。** reveal 期限を過ぎ、protected DA を 1 度も強制していない seal は、job の terminal 前に席を返す。未使用の guarantee は返還しない。protected DA を強制した seal は席を保つ。これで job あたりの protected 回答は 2 × 4 のままになる。
3. **mining claim の Final 後の追及(v39 action 80)。**
   - 資料:component の資料を claim の責任期間まで保持する(collection 53 を拒否)。責任期間 = 受理 + 認証の寿命 + receipt の寿命 + prosecution window。
   - 告発:自己完結の filing、または Final 後の demand(claim あたり生涯 8 件・同時 2 件、回答期限は既存の `da_answer`)。
   - 未回答:default は有罪とする。
   - 有罪の効果:producer の reserved exposure の徴収(残高まで)、receipt・safe weight・certification の取消、Reproduced attestor の lock の charge。
   - Gate C:開いている demand は pending である。
4. **attestor の lock。** native mining claim に Reproduced と attest した bond は、seat-lock 関係(ADR-0151 D1/D4)を claim の reserved exposure と quorum k に当てた額を lock する。
5. **veto。** RFC-0018 U-6 は採用しない(`veto = 0`)。

## 採用理由・代替案

- G-N1 の監査の選択肢は 3 つ:期限切れ seal の解放、枠なしの自己完結 filing、枠の価格による増加。前の 2 つを採った。後の 1 つは経済値の変更になる。
- producer の前払い回答上限(`check_native_protected_capacity`)は変えていない。生涯の上限を外す案は、producer の義務を無限にするので採らなかった。
- G-N2 の監査は「negative veto の人数で客観裁定を代用しない」としている。veto 0 で、verified filing と default だけで裁く。

## 値(2026-10-17 ユーザー採用。drill・測定で問題が出れば見直す)

| 値 | 現状 | 根拠 |
|---|---|---|
| Final 後の demand 数(8 / 同時 2) | 採用 | protected seal の 2 × 4 に合わせた。測定していない |
| attestor の lock 額 | 採用(seat-lock 関係 × reserved exposure) | 既存の関係の適用。gain の代わりに exposure を使っている |
| 責任期間 | 採用(既存の値の和) | 新しい値は導入していない |
| 試験の horizon(challenge 300 など) | 試験だけ | probe の値は node 試験に短すぎる |

## 追記(2026-10-18):text29 の料金式の欠陥

- text29 の `paid_restore` の二次の料金式は、block あたりの court work 上限を常に超え、text29 は到達不能だった。同じ単価の線形の式に直した。
- Fund は、paged29 の同時義務が固定の `seal_ttl` の下で物理 service を超えるため、まだ成立しない。値の判断を待つ(G14 完全性証明 §13.5)。

## 追記(2026-10-18、ユーザー決定):carriage の period

- Final27 の予約 scheduler(`NativeCarriageServiceV22::reserved_scheduler_v30`)の carriage period を 2 DAA から 1 DAA にした。物理 service は 2 倍になる。
- `seal_ttl`、締切、責任期間は変えていない。Final27 が有効な所だけに効く(全 preset で休眠)。Final27 の外の probe service は period 2 のまま。
- text29 と v39 は同じ service を使うので、両方の Fund の同時義務の検査に効く。

## 帰結

- 暗号学的な計算証明は使っていない。
- 残る限界:ordinary lane と discovery ticket の先取り(価格だけで抑えている)。text29 と v39 の実 job での試験。node 水準の担保不足(正規の経路では作れない)。

## 追記(2026-10-18、ユーザー決定):v39 の Master・mining の料金は運ぶ bytes で計る

- v39 の Master(65、66、67〜70 の metadata upload)、mining(73、75)、component の Fund の restore 料金から、固定の envelope(65,536 byte、66 の 131,072 byte と component ごとの 32,768 byte、prior component ごとの 16,384 byte、finalized の 32,768 byte)を外した。action が実際に運ぶ・restore する bytes(登録 body、source、book、post)だけを数える。
- block あたりの court work 上限(2^30、通常 2^29)と単価(16,384 / 8,192 work/byte)は変えていない。休眠の `palw_native_execution_v39` / `palw_native_mining_v39` の中だけで効く。
- これで fixture の Master 65 は 97,027,072 work 程度になり、通常の admission の上限に入る(G14 完全性証明 §13.7 の表の前提が変わった)。
- Master の audit(`derive_native_master_final_audit_v39`)の restore extent の再計算も同じ式にした。
~~~~~~~~
<!-- ADR_SOURCE_END:0187-g14-native-public-filing-and-post-final-mining-accusation.md -->

</details>

<a id="source-0188-d2-x3-arm-records-composition-and-pause-credit"></a>

<details>
<summary>0188-d2-x3-arm-records-composition-and-pause-credit.md — 原文 L1–L93</summary>

[原文を開く](../adr/0188-d2-x3-arm-records-composition-and-pause-credit.md) · SHA-256: `3b9a7ff50d55343778bbfb6807af1e28768463017c54e8d91cca2a72909fb5c3`

<!-- ADR_SOURCE_BEGIN:0188-d2-x3-arm-records-composition-and-pause-credit.md -->
~~~~~~~~markdown
# ADR-0188 — D2・X3 の休眠 fence を G14 の arm-readiness record で閉じ、X3 の期限を窓の合成で定める

- **Decision:** Accepted — ユーザー決定 2026-10-18 の内容を実装・記録する: X3 の期限は `prepare = window_bind + window_receipt`、`observe = window_challenge_at(activation)`、`resolve = B_disclose + B_court + Δ`（`Δ = B_cold_fetch + B_check + B_localize + B_carrier + B_reorg_slack`）、`hard = accepted + prepare + observe + resolve`。Δ の各項と `c_retain` は policy の名前付き入力で、0 は未測定として拒否する（既定値にしない）。
- **Scope:** `palw_court_model_scope_v1`（D2）と `palw_claim_hard_deadline_v1`（X3）の refusal。X3 の econ 接続（`C_upper` と beacon source の material 項）。X3 の DA pause 返還（`resume_claim_after_da_session_v2`）。D2 の readiness object の tag（230）。
- **Implementation:** branch `codex-free/d2-x3-finish`（起点 `release/t12-16000` 17cbdee9e）。
  - `PalwG14GatedFenceV1::{CourtModelScopeV1, ClaimHardDeadlineV1}` を追加し、無条件 refusal を `validate_palw_g14_fence_record_in_v1` の record gate に置き換えた。`PALW_G14_ARMING_RECORDS_V1` は**空のまま**（record を追加していない）。requirements は `PALW_D2_ARMING_REQUIREMENTS_V1` / `PALW_X3_ARMING_REQUIREMENTS_V1`。
  - X3 は `palw_x3_unmeasured_terms_v1` が空でない policy を record の有無に関係なく refuse する。spans は `palw_x3_composition_refusal_v1` で窓から読んだ合成と一致しなければ refuse する（activation で窓を読み直す）。
  - `palw_x3_composed_policy_v1` で合成 policy を作る。`candidate_from_horizons_v1` は未測定の B_* を 0 のまま残す（未承認の候補）。
  - X3 の claim には DA pause credit を与えない（`palw_x3_pause_credit_v1`）。旧 claim は pause をそのまま返す。
  - econ: `palw_minimum_price_c_upper_x3_v1`（`palw_econ_contracts_v1`）と `PalwBeaconSourceCostBoundsV1::minimum_gross_fee_x3`（`palw_beacon_source_fee_v1`）が retention の価格を `C_upper` と material 項に入れる。`c_retain = 0` で拡張が正なら UNKNOWN として refuse する。
- **Tag:** R-D2-READY は tag **230**（`PALW_A2_TAG_ALLOCATIONS_V1` に 2026-10-15 から予約済みの行）を確定した。kind の codec と A-2 kind-table の行は未着手。
- **Verification:** 下の「テスト結果」を参照。node-level の REQ-X3-1〜5 の block flow テストと、restart / reorg の replay 同一性は**未実施**（2026-10-18 の再確認でも未着手）。R-D2-COVER は 2026-10-18 に導出して試験を追加した（下の「R-D2-COVER」）。tag 230 の codec は実装せず、Proposed の選択肢を下に書いた。
- **Activation:** なし。`PALW_G14_ARMING_RECORDS_V1` は空で、D2 と X3 は全 real network で refuse される。X3 の policy 値（B_cold_fetch・B_check・B_localize・B_carrier・B_reorg_slack・c_retain）は**ユーザー承認と測定が必要で、未決**。
- **Related:** [mainnet-spec-register §3.1.1](../design/palw/integration/mainnet-spec-register-2026-10-12.md)、[RFC-0014 §7.2](../rfc/0014-panel-independent-fraud-prosecution.md)、[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)（D2 の出典）、[ADR-0187](0187-g14-native-public-filing-and-post-final-mining-accusation.md)。

## 背景と決定

1. **arm の形。** 他の G14 fence と同じく、armed の高さで pinned record（network・高さ・evidence・review）が無ければ refuse する。record の一覧は空。record を pin するのは審査された変更だけで、この ADR は値を入れない。
2. **X3 の合成。** 期限を窓と Δ から組む。窓の変更は合成を通じて期限に反映され、activation で `Params` の窓を読み直して食い違えば refuse する。Δ は「測定されるまで 0 にできない」項として扱う。
3. **X3 の pause。** X3 の期限は d0 で固定される（REQ-X3-3）。DA session の pause を返すと claim の anchor が後ろに動き、期限の延長になるため、X3 claim は 0 とする。旧 claim は既存どおり。
4. **R-D2-READY。** tag 230 を確定する。kind 自体は codec を伴うので、この ADR の範囲では未実装。

## 採用理由・代替案

- record gate は既存の G14 fence と同じ経路にした。新しい承認の仕組みを作らず、空の一覧で全 network を refuse し続けられる。
- Δ を `resolve` に含めたのは、REQ-T12-3 の `B_remaining` と合成式を一致させるため。Δ を 0 にして既定に残す案は採らない（未測定を無料とみなすことになる）。
- X3 の pause 返還を credit 0 にしたのは、d0 固定（REQ-X3-3）を優先するため。代替の「pause を上限付きで返す」は、期限延長の上限を別に定める必要があり、この ADR では決めない。

## テスト結果（2026-10-18、`CARGO_INCREMENTAL=0`、`-j 4`、branch `codex-free/d2-x3-finish`）

- `--lib palw_ruling_fences_v1`: 16 件 PASS（新規 3 件を含む: `d2_and_x3_are_refused_without_a_record_and_admitted_exactly_at_the_records_network_and_height`、`x3_refuses_an_unmeasured_policy_term_even_with_a_record`、`x3_spans_are_the_composition_of_the_windows_read_at_activation`）。
- `--lib` の `palw_court_model_scope_routes_v1`: 5 件を含めて PASS。
- `--test palw_ruling_fences_2026_10_12`: 3 件 PASS。`--test t12_repin_values`: 1 件 PASS。
- `--test g14_close_gates`: 12 件 PASS（`testnet12_identity_did_not_move`・`armable_g14_fences_refuse_without_a_measured_record` を含む）。`--test g14_close_legacy`: 8 件 PASS。`--test g14_court_capacity`: 15 件 PASS（1 件 ignored）。
- `--lib` 全体で 8 件が FAIL（4,612 PASS）: `fork_id_v1` の 2 件、`palw_lifecycle_objects_v2::tests::a2u` の 3 件、`palw_native_resource_v1` の 1 件、`palw_state_v2::tests` の 2 件。**base（17cbdee9e）でも同じ 8 件が FAIL（4,609 PASS）を確認済み（2026-10-18）。本ブランチが原因ではないため修正していない。**
- 未実施: node-level（block flow）の REQ-X3-1〜5 試験、restart / reorg 再生の同一性、R-D2-COVER、node-level の D2 full-collusion 試験、R-D2-READY の codec。
- econ（2026-10-18、commit 4cc05d5ec）: `x3_gross_floor_*` 2 件・`c_upper_x3_*` 2 件 PASS（`c_retain` 0 の正の延長は UNKNOWN、overflow・未測定 class も拒否）。

## R-D2-COVER（2026-10-18）— IR / pipeline / evaluation finalize の読み取り集合

**結論:** finalize が読む inventory leaf の正確な集合は、opening 時には導出していない。導出できるのは finalize の評価を実行した後だけである。

- **何が決まるか。** IR の評価では `palw_tir_court_v1` の `param()` が、読んだ byte を持つ inventory leaf を `used.params` に入れる。どの byte を読むかは program と carried step の値で決まる（添字が step のデータに依存する）。よって集合は program だけの関数ではない。
- **opening の検査。** `palw_d2_named_leaf_operands_admit_v1` は `TirParams(None)` と `TirParams(Single)`（multiproof が登録 root に届くこと）を受理し、読み取り集合との一致は見ない。`EvalCone` の params は `CarriedUnchecked` で素通し。
- **厳密な関門。** root claim の finalize の Step 8（`check_canonical`: `used.params.len() != r.params.opened_count()` で拒否、`palw_tir_court_v1.rs`）。不足する leaf は被告が自分の copy から補うしかない。
- **露出の上限。** 被告が補う leaf は、その 1 件の告発の finalize が読む相異なる leaf に限られる。各 leaf は class の inventory の 1 piece 全体で、数は `PalwTirInventoryIndexV1::leaf_count()` で上から抑えられる。挑戦者が公開を怠った場合、この露出は挑戦者の告発の費用になる（棄却された claim では挑戦者が負担）。
- **残余（閉じていない）。** 課題は「被告に model byte を強制しない」の原則と、この露出の関係である。opening で読み取り集合を導出する（評価を一度走らせて検証する）案は未実装。費用は告発 1 件あたり finalize 1 回で、有界。採否は設計判断。
- **試験:** `r_d2_cover_the_opening_does_not_derive_the_finalize_read_set_and_the_finalize_is_the_exact_gate`（`palw_court_model_scope_routes_v1/tests.rs`）が上の 2 点（opening は空の carriage を受理、厳密な関門は finalize の Step 8）を固定する。

## R-D2-READY — tag 230 の codec（Proposed、ユーザー判断待ち）

tag 230 の wire 形は設計判断のため実装しない。選択肢:

- **案 A（per-leaf digest 列）。** 新 object `SeatReadinessDigest { beacon: Hash64, class_id: Hash64, seat: PalwBondKeyV2, span: u64, digests: Vec<(u32 leaf_index, Hash64 digest)> }`。digest は `palw_d2_readiness_digest_v1` の値。borsh。`digests` の長さは draw の上限で有界（上限は policy の名前付き値、未決）。A-2 への影響: kind 表に tag 230 の行を 1 つ追加し、fence 未満は「decode 不能と同じ扱い」（判定せず通す、ADR の既存規則）。fingerprint と A-2 pin が変わるので、pin の再測定が要る。休眠（`palw_court_model_scope_v1` 配下）なので、arm まで網の挙動は変わらない。
- **案 B（Merkle 根 1 個）。** `SeatReadinessRoot { beacon, class_id, seat, span, root: Hash64, leaf_count: u32 }` の 1 個だけを置き、反証は leaf index と Merkle path を伴う。object は小さいが、反証に path の codec（sibling 列の上限と順序の規則）が増える。A-2 への影響は案 A と同じ行数、追加の pin は path codec 分。
- **案 C（既存の readiness proof の tag 47/50/93 を使う）。** 不採用。readiness proof は model bytes を被告に出させる（D2 の目的と反する）。

**ユーザーに決めてほしいこと:** A か B か。反証の 1 件あたりの費用（B は path 分だけ重い）と、digest 列の上限（draw 数）の値。どちらも arm 前の判断で、今は実装しない。

## node-level 試験（2026-10-19、`consensus/src/pipeline/virtual_processor/tests/lg14a_legacy_filer_e2e.rs`）

- **arm の形:** D2 と X3 は test config だけで arm する（`lg14a_config_with(armed, d2, x3)`、`sync_palw_2026_10_12_ruling_fences_v1`）。X3 の policy は `unapproved_probe_at`（probe 値、test 限定）の `resolve_daa` を **composition の値 `palw_claim_pursuit_budget_v1` に置き換える**（下の発見 1）。preset には入れない。
- **PASS:** `x3_req_1_2_3_the_hard_deadline_is_fixed_at_acceptance_on_the_node`（期限は accepted + 合成値、min、後続ブロックで不変）、`x3_req_4_a_pursuit_starts_only_inside_its_budget_on_the_node`（許容 start は accepted から cutoff まで、cutoff+1 以後は reservation なし）、`x3_the_fixed_deadline_survives_a_restart_and_replays`（再起動後も期限・tip・delta 行が一致、fresh node が同じ root に再生）、`d2_a_reorg_undoes_and_returns_the_reservation_exactly`（D2 の reorg、旧 legacy シナリオを D2 arm で実行）。`cargo test -p kaspa-consensus --lib lg14a_legacy_filer_e2e`: **29 passed, 3 ignored**（2026-10-19、commit a9e81e94f の後）。
- **BLOCKED（無視フラグ付きで記録）:**
  - `d2_a_full_collusion_on_a_new_claim_is_convicted_before_final_on_the_node`、`d2_the_pursuit_survives_a_restart`（R-D2-NODE）: 新 claim の held StepLeaf の告発は mempool で拒否される（ADR-0177 D2 の「モデル bytes を強制しない」）。replacement route（`palw_d2_one_move_on_claim_part_v1` など）は node の admission に配線されていない。配線は consensus 経路の実装で、この session の範囲を超える。
  - ~~R-X3-HOLD~~ **解消（2026-10-19、commit 0bc3f67b5）:** 規則の不具合ではなく、試験の前提の誤りだった。X3 では、claim の Final を止めるのは開いている DA session だけである（DL-1 の X3 行は `open_sessions() > 0` のときに期限を None にする）。session のない reservation は、有効期限 `min(提案, hard)` を超えて Final を止めない（REQ-X3-2）。試験を 2 本に分けた。`x3_req_5_reaching_the_deadline_alone_never_finals_an_unresolved_claim_on_the_node` は、reserved の held DA session を開いて producer が答えない場合に、hard を過ぎても Final にならず DA default（Voided）に至ることを確かめる。`x3_req_5b_a_reservation_without_a_session_finals_at_its_effective_deadline` は、session のない reservation の claim が hard 以下で Final になることを確かめる。試験環境では X3 と LG14-B（held DA）を一緒に武装する。`lg14a_legacy_filer_e2e`: **31 passed, 2 ignored**（R-D2-NODE の 2 件）。
- **発見 1（期限の composition）:** probe の `resolve_daa`（200）は `B_disclose + B_court + Δ` より短く、どの pursuit も許可されない（`palw_claim_pursuit_budget_v1` は `window_court` と disclose window を含む）。registers の「X3 と L の窓」の未解決項と同じ。値の再測定が arm の前提。
- **発見 2（held claim の期限）:** X3 を arm すると、node 上の held claim の bind 期限は legacy の `None` ではなく hard に従う（REQ-X3-2 の min）。試験は legacy 値（None）を固定する。

## 残り（この ADR の範囲外、または未実施）

- node-level の BLOCKED 1 件（R-D2-NODE：filer が D2 claim に KernelWitness を要求し、fold が claim-part 経路で裁定する変更が要る）。X3 の reorg 再生（pre-cutoff の reservation と session を使うシナリオは未作成）。
- tag 230 の codec と A-2 kind-table 行（上の選択）。
- X3 の Δ（5 項）と `c_retain` の測定・承認（未決のまま）。


## R-D2-READY B案の隔離実装承認 — 2026-10-21

利用者は本chatの選択回答で **B（Merkle rootと認証path）** を選択した。tag 230の小さいroot objectと、反証時のleaf index・認証pathについて詳細仕様化・隔離実装を進める。上のA/B未選択という記述は当時の履歴である。

この選択はdraw上限の値、本番採択・新しいモデル審査・モデル係数の変更・武装・配備の承認ではない。未決のdraw上限を仮値や0で代用して受付成功にしない。既存wire/tag・t12同一性を保持し、codec、既存digest refutationとの接続、native/resource preflight、A-2とfence境界の扱いを別々に検証する。新codecの追加だけをR-D2/G14完成と報告しない。

### Bのcodec詳細候補（2026-10-21、隔離・限定検証済み）

`palw_court_model_scope_routes_v1::readiness_root`で、reserved tag230と固定fieldのroot/countを独立にencode/decodeする。これは既存の`PalwConsensusObjectV2`へ登録したkindではない。tag/fieldsの切断・末尾を拒否し、root objectに可変長配列を持たせない。seat認証署名・native recordを持たないDTOのdecodeを、ready認定・slash・モデル審査の権限へ変換しない。

反証はinventory leaf index、既存keyed digest、bottom-up sibling列を持つ。drawの正規順序は昇順・重複なしとし、そのordinalに対するtree pathを検査する。leaf hashはbeacon/class/seat/span/index/digestを新しいデータ同一性domainへ束縛する。内部nodeは既存artifact treeの関数を使い、odd nodeは複製せずpromoteする。pathの正確な長さ・順序と全消費を検査する。u32 countから導ける構造上の最大深さ32を、可変長割当前に検査する。32はdraw数の採択値ではない。

native authorityから得るexpected context/drawと、明示したnonzero draw上限が必要であり、未決上限はNoneとして拒否する。認証pathがrootへ届いた後にだけ、検証者自身の登録artifact openingを既存`palw_d2_readiness_refute_v1`へ渡す。openingの別座標・改変・登録root不一致はrefute成立にせず、正しいdigestは既存のDigestHolds、登録byteと異なる認証済みdigestだけはSeatRefutedを返す。結果はpure predicateであり金融効果を適用しない。

全体の残件は、nativeのliteral seat authority/署名・drawの合意導出と上限の採択、実fee/資本とリソースpreflight、永続state/反証受付/qualified金融効果、A-2/dispatcher/fence境界への接続、実nodeでの敵対的検証である。既存のA-2 pinやt12同一性をこの部品に合わせてrepinしない。既存のモデル二階層を変更せず、model登録/昇格/利用資格への審査に転用しない。

[限定検証の証拠と残件](../evidence/palw/g14-completion-2026-10-21/readiness-root.md)：D2選択8件、現ソースのt12同一性9件・有効化境界5件が成功。rootだけでは隠されたdigest/pathを復元できないため、供給経路を[RFC-0033](../rfc/0033-d2-readiness-root-opening-availability.md) / [ADR-0200](0200-d2-readiness-root-opening-availability.md)で別途具体化し、C1/C2の選択を確認中。nativeのready/金融効果は未実装のままである。
~~~~~~~~
<!-- ADR_SOURCE_END:0188-d2-x3-arm-records-composition-and-pause-credit.md -->

</details>

