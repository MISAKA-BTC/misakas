# ADR-v2-0032 — 検証抽選・beacon・認証receipt・paid lock乱数

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 4本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0006](../rfc-v2/0006-panels-and-probabilistic-verification.md) / [RFC-v2-0010](../rfc-v2/0010-dag-consensus-and-native-settlement.md)

## 1. この文書の役割

検査の担当割当、post-commit乱数、receiptと各種認証objectの意味を整理する。

## 2. 統合した判断の読み方

### 2.1

challenge samplingは再検査のschedulerであり、抽選や多数票そのものが算術有罪を決めるわけではない。[旧ADR-0028](../adr/0028-palw-challenge-sampling-protocol.md)

### 2.2

FamilyCertified／ClassLaneCertifiedやextension manifestの証拠を、claim単位のGate Cと区別する。名称がcertificationでも対象命題と必要な検査は同じではない。[旧ADR-0075](../adr/0075-certification-is-a-consensus-object.md) [旧ADR-0108](../adr/0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md)

### 2.3

paid claimのlock乱数は後続seal commitment等の採択記録に従い、bootstrap seedと通常sourceを分ける。claimを固定する前の乱数を後から固定されるstatementの検査に流用しない。[旧ADR-0185](../adr/0185-paid-lock-randomness-bootstrap-and-paid-sources.md)

## 3. 維持する境界・後続改定

古いcredit gateやclass certificationの記録を、後続SecurityCertifiedの無条件な計算証明に読み替えない。適用先・source・版の差は原文のまま残す。

## 4. 隣接する判断との分担

算術検査は[ADR-v2-0033](0033-probabilistic-checks-and-exact-court.md)、ticket側のanchorは[ADR-v2-0028](0028-attempt-ticket-and-seed-anchor.md)、G14は[ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0028-palw-challenge-sampling-protocol.md](../adr/0028-palw-challenge-sampling-protocol.md) | L1–L436 | L24–L42 |
| [0075-certification-is-a-consensus-object.md](../adr/0075-certification-is-a-consensus-object.md) | L1–L375 | L17–L26 |
| [0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md](../adr/0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md) | L1–L521 | L23–L43 |
| [0185-paid-lock-randomness-bootstrap-and-paid-sources.md](../adr/0185-paid-lock-randomness-bootstrap-and-paid-sources.md) | L1–L33 | L3–L8 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0028-palw-challenge-sampling-protocol"></a>

<details>
<summary>0028-palw-challenge-sampling-protocol.md — 原文 L1–L436</summary>

[原文を開く](../adr/0028-palw-challenge-sampling-protocol.md) · SHA-256: `18edab2ff7a9d65bc02dae81095d679796d8bd55f5a95564d54594de480de7d0`

<!-- ADR_SOURCE_BEGIN:0028-palw-challenge-sampling-protocol.md -->
~~~~~~~~markdown
# ADR-0028: PALW challenge sampling — a scheduler for re-execution, never a verdict

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


> **Status amendment (2026-09-18, ADR-0133):** the windows below are stated against the 120 s block cadence of the
> day, and the scheduling fabric they describe is not the V2 lineage's. On V2 verification is its own clock —
> the receipt window (600 DAA), a class's verification window in spans, the class-local gate and the registry's
> capacity rule (ADR-0133, ADR-0135) — and **no claim has to verify inside a block interval**. Verification V2
> (S1, segmented replay) is fenced at testnet-11's DAA 6,100 (ADR-0133 §11.1). The 120 s figure that survives is
> the PALW anchor cadence (ADR-0021), not a claim's deadline.

Status: **Accepted (architecture), with the §4e remedy set SUPERSEDED on the V2 lineage.**
Activates nothing; devnet / shadow / zero-credit envelope
unchanged. This ADR fixes the scheduling fabric that ADR-0027 left open: who re-executes which
job, when duties and windows open and close, what the randomness binds — and what it is
forbidden to decide. Promoted from Proposed on 2026-08-16 after a numeric review against the
real network parameters: the first draft's 48 h challenge window exceeded both pruning horizons
(30 h at 0.1 bps, 38 h on the 120 s net) — windows are now pruning-constrained, they derive a
credited-job ceiling (§3), and the funding mechanics are concrete (§4a–4e).
Date: 2026-08-16
Relates to: ADR-0027 (premises P1–P3, the refutation model this ADR schedules work for),
ADR-0026 §4/§5 (the PRF-positions flow demoted there is given its surviving role here),
[`misaka-palw-slash-protocol-design-v0.1.md`](../design/palw/verification/misaka-palw-slash-protocol-design-v0.1.md)
(§9 dynamic inputs, §10 DA, §17 objective offenses, §18 economics, §23 deadlines),
[`palw-full-logits-trace-v2-design.md`](../design/palw/execution/palw-full-logits-trace-v2-design.md) (§10 economics,
§12 gates, §13 staging, §15 prohibited claims),
[`palw-legs-capture-measurement-2026-08-15.md`](../evidence/palw/measurements/palw-legs-capture-measurement-2026-08-15.md)
(its §6 is the opening seam this ADR's §5 consumes), `consensus/core/src/vlt.rs` (`select_verifiers` — the assignment
shape adopted in §2), ADR-0012 (superseded commit-reveal sortition — deliberately **not**
revived; see "What this ADR deliberately does not decide").

## Premises carried forward

P1 (no BFT), P2 (no challenge-randomness dependence), P3 (slash-terminal) are inherited from
ADR-0027 unchanged. This ADR adds their corollary for scheduling:

> **Randomness may schedule work, spread load, and price risk. It may never make an unchecked
> job safe.** Any rule of the form "job X was not drawn, therefore X may be credited without a
> replay" reintroduces exactly the dependence P2 forbids: the moment prediction or grinding of
> the draw becomes possible, the unchecked set becomes the attacker's choice.

One placement matters more than everything below: **the load-bearing randomness in PALW sits at
job origination, not at challenge time.** Compute jobs are self-originated (no orderer); what
prevents precomputation and replay-of-known-work is the `execution_seed`'s binding to recent
chain state — a work-freshness property the PALW seed path already enforces. Challenge-side
randomness, by contrast, is pure scheduling. Confusing the two is how "sampling" ends up
carrying security it cannot have.

## What sampling still does, once it cannot convict

| Role | Status under P1–P3 |
| --- | --- |
| Decide a verdict (fraud / no fraud) | **Forbidden.** Verdicts come from unilateral refutation only (ADR-0027 §1). |
| Decide which jobs may skip verification | **Forbidden.** §1 below: every credited job is fully re-executed. |
| Assign the funded re-execution duty (who replays job C) | Legitimate — §2. Determinism and class-scoping are what matter; unpredictability is hardening. |
| Open and close windows (duty, challenge, answer) | Legitimate — §3. Deadlines make non-response an *objective* offense. |
| Size redundancy and price risk (`q`, fees, bonds) | Legitimate — §4. `q` is the redundancy of the honest-existence assumption. |
| Audit answerability and DA (which leaves to demand) | Legitimate — §5, on the landed opening seam. Proves *answerable*, never *correct*. |

## Decision

### 1. Every credited job is fully re-executed; attestation allocates credit, refutation decides fraud

The crediting gate for a PALW job commitment `C` (the committed root — bare v2 logits root or
the execution-commitment composite, per the class's registered form):

```
credit(C) ⟺   window W_challenge(C) has closed
            ∧ ≥ 1 assigned re-executor published a bonded attestation whose
                independently recomputed root equals C's committed root
            ∧ no accepted refutation against C
```

(An attestation signs `C`'s root, so a refutation of `C` covers every attestation of it — the
signers are then slashed by `signature ∧ refutation`, with no separate refutation object.)

Three readings, to keep this compatible with ADR-0027's "PASS is absence of refutation":

* An **attestation is not a verdict.** It is a bonded, slashable claim "my replay produced this
  root" — it *allocates credit*, which P1 permits, and it re-grounds false-PASS slashing as
  `signature ∧ later refutation` (v0.1 §18.2 `V-C1` as amended). Nobody is slashed for silence.
* **Absence of attestations ⇒ credit 0, never "shrink the panel and mint"** (v2 design §10,
  adopted verbatim). The hash floor keeps the chain alive; PALW credit simply does not accrue.
* The **verification tax is stated, not hidden**: one primary execution plus `q` funded replays
  per credited job — `≥ (1+q)×` compute per credit. "Light verification" remains a prohibited
  claim (v2 design §15). Sampling exists to decide *who* pays that tax and *when it is due*,
  not to lower it.

### 2. Assignment: the `select_verifiers` ticket, adopted as the duty lottery

The repo already has the right shape — `select_verifiers` in `vlt.rs`: a deterministic,
class-scoped, executor-excluded ticket ranking. It is adopted for v2 with a new domain key and
its inputs renamed to the v2 vocabulary:

```
ticket(v) = H( PALW_V2_ASSIGNMENT_DOMAIN ‖ C ‖ executor_id ‖ anchor(C) ‖ v )
eligible(v) ⟺ bonded ∧ registered in class(C) ∧ not frozen ∧ v ≠ executor
panel(C)   = the q lowest tickets over eligible(v)      (validator_id tie-break, as today)
anchor(C)  = selected-chain block hash at daa(C) + Δ_bind
```

What this construction is required to provide — and what it is not:

* **Determinism.** Every node derives the same panel from chain state alone. A duty must be
  checkable ("v was assigned and did not answer") for no-show to be an objective offense.
* **Class scoping.** Cross-class assignment refutes honest work — the measured failure the
  class mechanism exists to prevent (`select_verifiers`' own doc-comment). Unchanged.
* **Executor independence.** The executor is excluded and cannot unilaterally pick its panel;
  grinding `anchor(C)` or its own bonded identity buys scheduling influence at bond-scale cost.
* **NOT unpredictability.** Under P2 the design must stay sound even if the panel is known at
  commit time. It is: an assigned re-executor replays *fully*, so there are no unchecked
  positions to hide in; and a fully corrupt panel cannot finalize a lie, because the panel is a
  *funding* device, not an exclusivity device — the challenge window is permissionless, and a
  refutation from anyone stands (§4). Anchor unpredictability is retained as hardening only.

**Reorg rule.** Assignment is a chain-scoped fact: a reorg that replaces the anchor block
recomputes panels on the new selected chain, and duties re-anchor with their windows.
Attestations and refutations are statements about `C` — they remain valid in any chain that
contains `C`'s commitment; only duty, deadline and bounty bookkeeping move. `Δ_bind` is a
**settling offset, not a finality bound**: duties may derive from a young anchor because every
offense that references the anchor (no-show, deadline) is prosecuted only after the anchor is
final (§3's `finality < W_challenge` rule) — an assignee that acted on an anchor later reorged
away simply sees its duty re-drawn, and nothing is chargeable against the vanished draw.

> **Amended by ADR-0034 §7 (2026-08-16).** Once receipts carry the three routing keys, the
> eligibility predicate above becomes binding-aware: family and family-version equality, band
> capacity, a Merkle ready proof against the verifier's committed `ready_binding_root`, slot,
> bond, TTL, reputation-floor and control-domain conjuncts join `bonded ∧ ¬frozen ∧ ≠ executor`
> (`routed_candidate_eligible_v1`, its own domain key — the assignment-twin discipline). A
> panel that lapses `W_replay` without the required on-time attestations is **re-drawn wider**
> at the escalation anchor; original no-shows keep their objective offense. Everything else in
> this section — determinism, executor independence, NOT-unpredictability, the reorg rule —
> and §4's `q ≥ 2`, funding split, no-show pricing and permissionless window are unchanged.

### 3. Windows: DAA-denominated, stall-tolerant, pruning-constrained — and they cap the credited job

All deadlines are DAA-score offsets (v0.1 §23, including "DAA stalls ⇒ deadlines stall").
Wall-clock intent is primary; the DAA denominator is per-network. Both PALW parameter sets are
shown (`new_deci_bps`: 10 s blocks; `new_two_minute_bps`: the 120 s public PALW testnet,
decided from fleet replay measurements — see the runbook's "Why 120 s").

| Window | Intent | 0.1 bps | 120 s net | Meaning of expiry |
| --- | --- | --- | --- | --- |
| `Δ_bind` | ~20 min | 120 DAA | 10 DAA | anchor fixed; panel and duties derivable |
| `W_replay` | ~1 h | 360 DAA | 30 DAA | assigned re-executor must attest or refute; silence = objective no-show (v0.1 §17) — *provided the input was available (below)* |
| `W_answer` | ~1 h | 360 DAA | 30 DAA | committed material must be opened; silence = objective DA offense |
| `W_round` | ~1 h | 360 DAA | 30 DAA | non-response at a bisection rung = objective offense (`M-O3`) |
| `W_challenge` | ~24 h | 8_640 DAA | 720 DAA | permissionless refutation window; `credit(C)` evaluated at close |

The rules the defaults are one solution of:

```
W_replay, W_answer, W_round ≥ κ · p99_cold_replay(class, credited ceiling)     κ ≥ 3
    (an answer or a rung response may cost a replay — a shorter window would make
     honest silence indistinguishable from withholding, which no objective offense may do)
W_challenge ≥ W_replay + L_bisect · W_round + margin                           L_bisect ≈ 20
finality_duration < W_challenge
    (anchor-referencing offenses prosecute only after the anchor is final — 12 h here)
W_challenge + prosecution slack < pruning horizon
    (30 h at 0.1 bps; 38 h on the 120 s net, where the prunality lower bound binds)
```

**Pruning is the binding constraint, and it caps the credited job, not just the window.** The
first draft of this ADR defaulted `W_challenge` to 48 h — wrong on both networks, caught in
review against the real parameters. At 24 h the ladder budget is 21 h with 3 h of margin: thin,
and priced — every stalled rung is itself an objective offense, so a withholding miner converts
to conviction long before the window is consumed. Response windows of 1 h at `κ = 3` require
`p99_cold_replay ≤ 20 min`; the fleet measurement behind the 120 s block time (12–26 s per
16-decode job ⇒ ≈ 0.75–1.6 s per decode token on the slowest host) derives a **credited-job
ceiling of ≈ 512 decode tokens**. The v2 format ceiling (4 095) stands unchanged — but a class
may only *credit* jobs whose measured p99 fits its registered windows inside the pruning
horizon. A bigger credited job requires a longer window, which requires a longer pruning
horizon or pruning-surviving carriage (Consequences) — a parameter decision taken explicitly,
never a silent stretch. Every default here is a Stage-1 placeholder that MUST be re-derived
from the per-class measurement before any slash activates; shipping the placeholder as if
measured is a §15-class violation.

**Duty precondition — input availability.** `W_replay`'s clock presumes the assignee can obtain
the job input (the prompt token ids behind `prompt_token_ids_hash`). An assignee that cannot
posts a fee-bonded input objection; the miner must serve the input within `W_answer` or the
duty converts into the miner's `DATA_WITHHOLDING` offense. A no-show is never chargeable
against an assignee holding an unanswered input objection.

Two consequences worth stating plainly: PALW credit is **latent by construction** — at least
`W_challenge` behind the chain tip (delayed settlement, v0.1 §20, unchanged); and the checkpoint
interval chosen at class registration is not free — it is the worst-case *localization* replay
inside a dispute, so registration must publish interval and p99 together.

### 4. `q`, funding, and the inequality that must hold before any reward exists

`q` is the redundancy of ADR-0027's honest-existence assumption — **how many independent
parties are funded and obliged to replay `C`**, not how many positions are sampled (that
meaning is dead). Defaults and rules:

* `q = 2` at Stage 1–2: one for liveness (a single assignee down must not strand a job
  uncredited), one because redundancy is the only lever the panel has. `q` scales per job by
  the v0.1 §9.2 inputs (credit at stake, bond, operator reputation, recent mismatch rate) —
  raised `q` raises the tax, which is the honest trade.
* **The panel funds promptness; the permissionless window carries the security.** Even
  `P(no honest assignee) = b^q` under adversarial fraction `b` is deliberately NOT the safety
  argument — leaning on it would be a probabilistic-majority argument through the back door.
  A corrupt panel can delay credit (no honest attestation ⇒ no credit) but cannot mint a lie
  safely: attesting a false root is `signature ∧ refutation` slashable the moment anyone —
  panel or not — replays and refutes.

**4a. The replay fee is an issuance split, not a fee market.** Self-originated jobs have no
orderer and no execution fee to tax, so the fee attaches to the only event that exists — the
credit event:

```
issuance(C) = (1 + q · ρ_v) · base(C)
  miner:                                     base(C)
  each on-time assigned attester
  whose replayed root matches:               ρ_v · base(C)
  ρ_v = measured replay/primary cost ratio of the class (≈ 1.0; published at registration)
```

Late attestations, non-matching roots and voluntary (unassigned) attestations earn nothing.
Unchecked jobs pay nobody, because unchecked jobs are never credited (§1). §1's `(1+q)×`
verification tax is therefore *literally the emission schedule* — visible in issuance,
not hidden in a market.

**4b. An attestation is assumption of liability, not proof of independent work.** The
verifier's dilemma is priced, not pretended away: a rubber-stamper who co-signs the miner's
published root without replaying earns the same fee at zero cost — and stakes its bond on a
root it never checked. Replaying is the attester's own risk management, dominant when
`S_a · P(fraud) · P_refute > c_replay`; at current magnitudes (bond 20 000 BILI against minutes
of CPU) that inequality is slack by orders of magnitude, and it self-stabilizes: were
rubber-stamping common, fraud would start paying, `P(fraud)` would rise, and replaying would
become dominant again. What the fee actually buys is **capacity** — hardware-hours standing
ready — not per-replay willingness; collusive stamping silently lowers the *real* replay rate — which
is exactly what §6's Stage-0 `P_check` telemetry exists to expose — but never makes a false
root safe, because the refutation right is permissionless.

**4c. No-show is priced against griefing.** No-show is an objective offense (v0.1 §17) with a
slash floor of a large multiple of the forgone fee (placeholder: `≥ 100 · ρ_v · base`,
Stage-1-measured like every number here).

> **Amendment 2026-08-16 (B15 simulation, `docs/palw-economic-parameters-2026-08-16.md`):**
> at live parameters the placeholder is **uncollectible** — `100 · ρ_v · base` = 444 562 BILI
> against a 20 000 BILI bond, 22× more than exists to slash. The floor is therefore
> `min(100 · ρ_v · base, bond)`: below ~222 k BILI bonds, **the bond IS the floor**. The
> griefing inequality still holds at the cap (2 × 20 000 BILI of slash against a one-orphan
> gain of 4 445.62 BILI ⇒ ROI 0.11), so the multiplier only becomes meaningful once bonds grow. The asymmetry is deliberate: a panel that strands a
job costs the miner one orphan-equivalent (re-mine), while costing the no-show pair two
slashes — targeted verifier griefing has negative return, and a panel that is merely *down*
loses fees and a bounded slash, not its base bond.

**4d. The challenger economy is rivalrous by construction.** The refutation bounty stays
capped at 49 % of collected slash under [ADR-0032's 2026-10-10 amendment](0032-palw-fee-bond-escrow.md#palw-challenger-share-amendment--2026-10-10),
with `B_cap` retained in the credit-overlay flow: the reliable challengers are
*competitors*. A rival miner who refutes removes competing credit AND collects the bounty, so
`P_check` rests on rivalry, not altruism; §5's fee-bonded audit calls are the paid probing
market for everyone else. (This is also why §2 tolerates a predictable panel: grinding a rival
*off* the panel removes their fee, never their refutation right.)

**4e. 旧 overlay mint の rate cap・subsidy 比率 — 不採用。**

PALW が block production を担う構成に変わり、hash block に追加する独立した `base(C)` mint がなくなったため、旧 per-job cap と subsidy 比率の調整は継続しない。現在の class block budget は [ADR-0045](0045-palw-class-economy-on-chain.md)、bond の aggregate exposure は [ADR-0042](0042-palw-mainnet-candidate-ruleset.md) Decision 6 に従う。

### 5. The audit layer: opening calls as the DA heartbeat — answerable, not correct

The landed opening seam (`PalwLegsOpeningCallV1` / `PalwLegsOpeningAnswerV1` /
`check_legs_opening_answer_v1`, worker `v2-legs-open`) becomes the transport of a sampled,
permissionless **answerability audit**:

```
auditor posts an opening call for C   (fee-bonded tx; leaves chosen by PRF(R), R = H(C ‖ anchor);
                                       ≤ PALW_LEGS_MAX_REQUESTED_OPENINGS per call, rate-capped per C)
  → the operator answers within W_answer (model-free verifiable, answerable by ANY class member)
  → silence past W_answer = objective DA offense; the call and its silence are both on-chain facts
```

What this layer proves and what it cannot: an answer proves the committed tree is *openable*
and its material *available* — membership under the committed root, exactly what the seam's
checker adjudicates. It does **not** prove the values are correct; per-leaf membership says
nothing about the rest of the tree, which is why predictability of `PRF(R)` is harmless here
and why this layer may never feed a slash for *computation* (that is §1's replay + ADR-0027's
refutation). Two further rules keep it honest:

* **Audit calls are priced as replays.** Answering regenerates the row by re-execution unless
  the operator retained material (retention is a local choice the seam already permits), so an
  uncompensated call is a DoS primitive. The call fee compensates the answer at replay cost:
  paid out to the operator on a valid answer, returned to the auditor — with the DA bounty on
  top — on proven silence.
* **Interval spot-checks stay diagnostic.** With checkpoint state bytes served under DA (they
  verify against `state_root`, which binds `state_layout_id ‖ bytes`), a checker can load
  checkpoint `k`, replay one interval, and compare — a cheap drift probe. Divergence found this
  way feeds `ClassContradictionCertificateV1` (freeze, fail-safe) and the §12 statistics; it
  never slashes without the one-step refutation. The ADR-0026 carve-out stands: tolerant
  comparison exists only in this non-slashing layer.

### 6. Stage mapping — what each stage newly requires from this ADR

Stages are ADR-0027 §6's; this table adds the sampling-protocol prerequisites. Nothing here
activates; each promotion is an explicit decision record (v2 design §13).

```
Stage 0 Shadow       assignment + windows computed and logged only; measure P_check,
                     p99_cold_replay per class, no-show and inclusion latency
                     — these measurements ARE the §12 gate artifacts
Stage 1 Objective    duties real; objective offenses only (no-show, W_answer silence,
                     equivocation, deadline, DA); credit still zero; defaults of §3
                     replaced by measured values
Stage 2 Bounded      credit(C) gate of §1 live with q ≥ 2; refutation may slash WorkBond;
                     requires: ExecutionStepRefutationV1 landed (computational conviction),
                     registration-time inequality of §4 enforced
Stage 3 Full         wider exposure; requires the second independent reference
                     implementation (v0.1 §29 gate 1) and a Stage-2 soak with zero
                     unexplained ClassContradiction freezes
```

## What this ADR deliberately does not decide

* **Fee and bounty magnitudes.** §4 fixes attachment points and inequalities; numbers come from
  the economic simulation gate (v2 design §13 step 5).
* **Job-side prompt/seed policy.** Origination binding is the PALW seed path's, already
  enforced; nothing here alters it.
* **A randomness beacon.** ADR-0012's commit-reveal stays superseded. Under P2 a beacon would
  be a solution to a dependency this design refuses to have; the selected-chain anchor hash is
  sufficient for scheduling, and hardening beyond it buys nothing load-bearing.
* **Mainnet.** Separate ADR, separate activation, after Stage 3 (v2 design §13 step 7).
  > **Resolved 2026-08-17 (ADR-0036).** A parallel Accepted ADR — ADR-0041, on the non-ancestral
  > `main-backup-8107bfb-20260807` snapshot — decided mainnet ships PALW active from a new v4
  > genesis. It is **not** in contradiction with this clause once its layers are separated:
  > ADR-0041 decides only the *land* shape (a new genesis-active identity, with credit / accept /
  > mint still OFF behind gates), which is this lineage's Stage 0. ADR-0036 records the settlement:
  > the live `palw_credit` lineage governs; ADR-0041's *mechanism* (`palw_spam` / `palw_algo4_accept`)
  > is superseded (it does not exist on the live tree); its two surviving conclusions — mainnet PALW
  > needs a **new network identity** (the current `MAINNET_PARAMS` can never carry it: both window
  > presets fail `finality_depth < W_challenge` at 10 BPS) and the *land → accept → mint* separation —
  > are carried into the future parameterized mainnet ADR. That ADR still comes after Stage 3, and
  > may not be signed while any of the 2026-08-16/17 audit's 9 blockers is open.

## Assumptions that remain (stated so they can be attacked)

1. **1-of-N honest replay, funded** — inherited from ADR-0027, now with the funding mechanism
   (§4) that makes it enforceable rather than aspirational.
2. **Censorship-resistant inclusion within `W_challenge`** — inherited; §3 turns "sizing it is
   open work" into attackable defaults with a measurement obligation.
3. **Honest measurement.** `p99_cold_replay` and `P_check` are published, reproducible
   artifacts; a class that games its p99 shrinks its own dispute windows and self-refutes on
   the first real dispute.
4. **Grinding never reduces `P_check`.** An executor who grinds anchors or bonded identities
   can shape who holds the duty — including seating a colluding panel that delays credit or
   co-signs a lie for the fee — but the lie's safety still reduces to the permissionless
   window: grinding touches neither the fullness of a replay (nothing to hide in) nor anyone's
   right to refute. Stated as the invariant it is: grinding moves fees and timing, never
   detection. If a concrete grinding path that lowers `P_check` is found, it attacks §2 and
   this ADR must be amended.

## Consequences

* **New objects to land (consensus-inert first, like everything before them):** the v2
  assignment evaluator (a domain-keyed `select_verifiers` twin), the duty/no-show bookkeeper
  over DAA windows, on-chain envelopes for attestation, opening call and answer (the wire
  bodies exist in `palw_legs`; what is missing is their chain carriage), and the telemetry that
  measures `P_check`, inclusion latency and no-show rates in shadow.
  > **Chain carriage designed as ADR-0029 (2026-08-16, Proposed):** two stages — Stage 0 on
  > native-subnetwork payloads (zero node changes, watcher over the RPC acceptance stream,
  > telemetry-grade only) and Stage 1 on dedicated subnetwork ids under the fork's existing
  > DNS-overlay discipline (offense-grade). Same Borsh bodies in both. Its mass audit found
  > one object that does not fit any transaction: the bare-v2 logits-event refutation
  > (≈ 0.99 MB vs 0.5 MB block mass) — legs-composite classes refute through 8 KB activation
  > rows and are unaffected, and bare-v2 classes are gated out of Stage 2 until chunked
  > evidence carriage lands.
  > **Stage-0 landed (2026-08-16, `consensus/core/src/palw_schedule.rs`, consensus-inert):**
  > the assignment twin (`select_replay_panel_v1`, eligibility rule in the function, domain
  > uniqueness tested against every PALW family AND the VLT sortition key), the §3 window
  > parameters with `validate()` enforcing this ADR's inequality set against the real
  > `BlockrateParams` — including a regression test pinning the 48 h draft failure on both
  > networks — and the shadow ledger (`PalwShadowLedgerV1`): pure-function duty
  > classification, the §1 credit gate evaluated as shadow (late refutations counted as
  > credited-and-refuted, the P3 tail metric), counts and nearest-rank percentiles only.
  > The worker gained `--mode v2-replay-bench` (fresh-load runs, shared percentile
  > convention, κ·p99 fit against both networks' defaults, non-zero exit on root drift).
  > First tool-validation numbers on the dev host: Metal D=512 p99 ≈ 5.5 s, CPU-aarch64
  > D=512 p99 ≈ 8.2 s — both fit trivially; the registered numbers must come from the
  > fleet, whose 0.75–1.6 s/token measurement remains the operative sizing basis.
  >
  > **Fleet-measured (2026-08-16, all four t10 hosts,
  > `docs/palw-stage0-fleet-replay-bench-2026-08-16.md`):** D=512 p99 37.3–90.7 s,
  > 59–165 ms/token, worst κ·p99 = 272 s — every host fits `w_replay` = 1 h with ≥ 13×
  > margin, and the cross-host logits roots are identical 4/4 at both depths (the pairwise
  > class property, measured). The old 0.75–1.6 s/token basis was F16; the pinned Q4 artifact
  > is ~10× faster, so the ≈ 512 credited ceiling is conservative and a registration-time
  > re-derivation may raise it — by this ADR's own rule, not by edit.
* **Carriage must outlive headers.** `credit(C)` is evaluated at `W_challenge` close and
  offenses prosecute after anchor finality, so `C`'s commitment record and the duty anchor must
  live in pruning-surviving state (as bond records already do) — or every window must close
  inside the pruning horizon with slack, which is what the §3 defaults do. The chain-carriage
  design (future work) inherits this as a hard constraint, not a preference.
* **Registration grows the published numbers per class:** `p99_cold_replay` at the credited
  ceiling, the credited ceiling itself (window-derived, ≤ the format ceiling), `ρ_v`, and the
  checkpoint interval's localization cost — plus the class budget and bond-exposure limits referenced in §4e. The §12 checklist gains:
  `[ ] windows re-derived from measured p99`, `[ ] credited ceiling derived from windows and
  the pruning horizon`, `[ ] P_check measured in shadow`, `[ ] no-show/inclusion telemetry
  published`.
* **ADR-0026 §5 closes.** The dynamic-`q` section's question ("how many challenges") has its
  successor: `q` counts funded replays (§4), the PRF flow schedules audits that decide nothing
  (§5), and the minimum-`f` parameter is gone with the sampling security model.
* **The opening seam acquires its consumer.** `v2-legs-open`'s refusal property — never answer
  for a root you cannot reproduce — is exactly what makes §5's silence offense fair: an honest
  operator can always answer for an honest commitment, so unanswerability past `W_answer` is
  evidence about the commitment or its DA, not about luck.

## Mission alignment amendment — 2026-10-07

challenge samplingの検出率は、客観的convictionの成立とは別に報告する。選ばれたchallengerだけに証拠閲覧を限定しない。発見済みfaultの証明受理を再抽選やPanel承認に依存させない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

確率的検出・全constraint coverage・exact courtに、bond/DAAによるclaim発行機会と総経済効果の上限を追加する。計算省略による発行速度優位で枠を増やさない。
容量倍率の拡大をcheckerのsoundness改善と同一視せず、全claimのerror composition、監視不在、同一seedの相関と期限内の証拠包含・徴収率を再評価する。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0028-palw-challenge-sampling-protocol.md -->

</details>

<a id="source-0075-certification-is-a-consensus-object"></a>

<details>
<summary>0075-certification-is-a-consensus-object.md — 原文 L1–L375</summary>

[原文を開く](../adr/0075-certification-is-a-consensus-object.md) · SHA-256: `95ff48a3dc100d0801ee22e66026768cb36f6f4fbec5f6ddc8cd7a797eeecfc0`

<!-- ADR_SOURCE_BEGIN:0075-certification-is-a-consensus-object.md -->
~~~~~~~~markdown
# ADR-0075: Certification is a consensus object

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** PROPOSED (2026-09-02), implemented on branch `palw-adr0073-fp-weight` for the
Relaunch 5e re-genesis (the state version and the genesis free-prompt set move, so the fingerprint
moves). Written against the goal "the public testnet accepts free-prompt claims on the real
models, and a model's certification no longer needs a code change".
**Builds on:** ADR-0069 (end-to-end adjudicability: weight is the price of a certified family),
ADR-0074 Decision 6 (the free-prompt lane bears weight only on a certified class), ADR-0054
(permissionless admission), ADR-0042 Decisions 7/8 (lifecycle objects ride transactions).
**Amends:** ADR-0069 Decisions 2 and 5 (the certified set is no longer only the build's), ADR-0069
Decision 6 (a weightless entrant is seated by an object, not by a re-genesis), ADR-0074 Decision 6
(the free-prompt-certified set has a chain half).

> **Landed (index reconciliation, 2026-09-02).** "PROPOSED … implemented on branch" is the
> drafting state; the implementation is on `main` as the Relaunch 5e build (`ff943fe1` the objects,
> `49e5e1da` the mainnet rules of §7, `1b49fcaa` Decision 14's chunked carriage, `654b57f1` the
> re-seed on seating from [ADR-0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md)
> Decision 4), state version 16, fingerprints re-pinned for 5e. On mainnet's genesis card: §7 names
> the model-root constants because the assembly *can* pin model tiers; the decided route
> (Decision 8, ADR-0076 §4) is floor-only at genesis with every model arriving by registration →
> drill → binding, and mainnet ships `PalwConsensusMode::Disabled` until the card is set. Map:
> [`README.md`](README.md).

> **Security amendment appended (2026-09-02)** — see the last section: the permissionless lane's griefing budget — chunk groups carry a deposit, grading is priced before it is performed, the card's keys are the operators' own, and the tooling reads genesis ∪ chain.

## 1. The problem: a certificate lived in the binary

ADR-0069 made weight the price of adjudicability: a class holds a share only if some family this
build has drilled end to end covers every kernel the class reaches, and the set of such families
is pinned by `court_e2e_root` at genesis. ADR-0074 Decision 6 did the same for the free-prompt
lane with a genesis-frozen set of class ids. Both sets are compile-time values. Adding a model's
certification — or the free-prompt lane of a model the network already runs — therefore meant a
code change, a fingerprint move and a rolling re-genesis of every host. On 5d the free-prompt set
holds the floor alone: `execute_free_prompt` runs on the registered A16 graph and on QWEN36 once
this branch lands its path, and the transition refuses both with `FreePromptLaneUncertified`
because nothing on the chain can say otherwise.

The court itself never needed the binary's word. `certify_e2e_family_v1` and
`certify_e2e_free_prompt_lane_v1` grade recorded fault vectors with no backend in scope (ADR-0069
Decision 5 made the evidence exportable for exactly this reason). What was missing was a place on
the chain for the evidence to be graded, and a rule for what the grade unlocks.

## 2. Decisions

**Decision 1 — Two lifecycle objects, carried by ordinary transactions.**
`FamilyCertified { evidence }` carries a family's drill (`PalwCertificationEvidenceV1`: the
attempt-lane `PalwE2eDrillEvidenceV1` or the free-prompt-lane
`PalwE2eFreePromptDrillEvidenceV1`). `ClassLaneCertified { class_id, lane, profile }` binds a
registered class to a lane. Both ride the lifecycle subnetwork (0x4b) like a court move; both are
permissionless and unsigned. The evidence is objective and the class binding is checked against
the class's own profile hash, so a signature would authenticate nothing; the transaction fee is
the rent, and a drill's evidence (hundreds of kilobytes at the RC fixtures' size) rides one
standard-mass transaction.

**Decision 2 — The court grades; nothing else vouches.** The transition re-runs the shipped
grader on the carried vectors and records only the family the grader returns. A drill whose
planted faults the court cannot convict, whose honest run it convicts, whose vectors are about
another graph, or whose free-prompt questions are not its own, is refused
(`CertificationRefused`) and records nothing. A refused object is a dropped carrier: the block
stands (ADR-0042 Decision 5's "reads as nothing" is logged, not silent). The grading work per
object is bounded by the court's own step ceilings and by `PALW_CERTIFICATION_MAX_VECTORS` (32).

**Decision 3 — The chain's certified sets are state.** `PalwChainStateV2` gains
`certified_families` (attempt lane) and `fp_certified_families` (free-prompt lane), keyed by
family digest, and `fp_certified_classes`, keyed by class id. All three are in the state root and
the carriage, written through delta entries that revert (`PALW_STATE_V2_VERSION` 15 → 16). A
family already recorded for a lane is refused a second time (`FamilyAlreadyCertified`).

**Decision 4 — Every gate reads genesis ∪ chain.** `family_certified_for_weight_v2(root, genesis,
chain, reachable)` checks the root against the genesis set only — the chain's families are chain
history, each replayed from the evidence that produced it — and searches both. The registration
share rule in the processor, `verify_class_admission_v3` (v2 is the same gate with the chain set
empty) and the free-prompt arm (`params.fp_certified_classes ∪ state.fp_certified_classes`) all
read the union. `court_e2e_root` keeps its meaning: the set the network committed to at birth.

**Decision 5 — A class is bound by kernel coverage, in both lanes.** `ClassLaneCertified` is
accepted when the class is Active, `profile.shape_profile_id() == class_id` (a class id IS its
profile's id, as `verify_class_admission_v2` already requires), and `reachable_kernels_v1(profile)
⊆ kernel_ids` of some family the chain certified for that lane. Free-prompt lane: the class is
recorded in `fp_certified_classes` and its commitments are admitted from then on. Attempt lane: a
class holding no share is seated at `min_grantable_share_permille` through the same share table a
registration uses (largest-remainder donation, the base reserve, the pending-share ceiling) —
ADR-0069 Decision 6's "earns cadence once some build certifies a backend for it" as an object. A
class already holding a share is refused (`ClassAlreadyWeighted`); a class no chain family covers
is refused (`NoCertifiedFamilyCovers`). The transition holds no profiles, which is why the object
carries one and why the free-prompt record is per class.

**Decision 6 — The genesis free-prompt set is derived by the same rule.** The RC free-prompt
families are the floor, PALW-QWEN36 and PALW-QWEN25-A16, each drilled on the fixture graph its
attempt-lane certificate was drilled on with a caller's prompt instead of the anchor's
(`rc_free_prompt_evidence_v1`). `palw_rc_fp_certified_class_ids_v1()` is every RC catalog class
some such family covers — the floor, the QWEN36 graph-v3 class and the A16 graph-v2 class — so the
5e genesis admits free-prompt claims on the real models without an on-chain object, and every
later class takes the on-chain route.

**Decision 7 — The route has tooling.** `palw-certify drill --family --lane --out` writes a
`FamilyCertified` object from this build's drills (graded locally first, so a drill the chain
would refuse never leaves the machine); `palw-certify bind --model-id --lane --out` writes a
`ClassLaneCertified` for a catalog class; `misaka-cli palw submit-object --object --yes` funds
and broadcasts either, sizing the fee from the carrier's own compute mass.

## 3. What does not change

The attempt-lane genesis set and `court_e2e_root`; registration (signature, admission carriage,
exposure); the court and its cost ceilings; carriage codecs; the SDK's registration preflight,
which still prices against the build's set — a registrant whose family is certified only on chain
must read the chain (the processor requires exactly the share the union implies).

## 4. Consequences, and what is deliberately not here

* A family, once certified, stays. There is no revocation object: a class that misbehaves is
  frozen by contradiction (`ClassFrozen`), as before, and the family's certificate says only that
  the court CAN convict it — which remains true.
* Grading in the transition costs every node CPU per object; the vector cap, the standard-mass
  ceiling and the fee bound it. Spam is a paid way to make every node run a drill grader once.
* A fixture drill certifies kernels, not weights (ADR-0069 Decision 2). This is unchanged, and it
  is why the on-chain object for a new model tier is a fixture-sized file rather than a model.
* The SDK preflight and `misaka-palw-gateway` do not read the chain's sets yet; the processor
  does, so a mismatch surfaces as a refused registration with the share the chain wanted named in
  the error.

## 5. Invariants the tests hold

* `certification_objects_carry_a_family_onto_the_chain_and_seat_its_class` (base0): on real
  drills, a family enters through evidence and only through evidence that grades; tampered
  evidence is refused; a class binds by its own profile; a free-prompt commitment refused before
  the binding is admitted after it; a weightless class is seated at the floor by the attempt-lane
  binding; every delta reverts.
* `the_rc_free_prompt_set_is_the_one_this_build_drilled` and
  `the_rc_free_prompt_classes_are_the_covered_ones` (base0): the genesis set is the drills, field
  for field, and the class set is the covered catalog.
* `a_chain_certified_class_takes_fp_commitments_under_the_genesis_gate`,
  `a_class_is_bound_to_a_chain_family_by_its_own_profile_and_kernel_coverage`,
  `evidence_the_court_refuses_certifies_nothing` (consensus-core): the gates, the refusals, the
  state root, the carriage and the revert.

## 6. The mainnet route for a model this build never pinned (Decision 8)

Mainnet ships PALW off today (`PalwConsensusMode::Disabled`); the bundle it will activate is
built by the same `palw_v2_params_on_base` every RC network uses, so nothing here is a testnet
special case. For a stranger with a new model, the route is three ordinary transactions and no
code change:

1. **Register weightless.** `kaspad --palw-register-class <model id>` (or the SDK) builds the
   class from its registered graph and registers it at 0‰ when no family the build or the chain
   certified covers its kernels. The registration message is signed over the share the OBJECT
   carries — a weightless registration used to be unsignable because the panel signed the floor
   unconditionally; that is fixed with this ADR.
2. **Post the covering family's drill.** `palw-certify drill --model-id <model id> --lane attempt
   --out f.obj` picks the RC family whose kernel set contains every kernel the model's graph
   reaches (`covering_rc_family_v1`), runs that family's fixture drill, grades it locally, and
   writes a `FamilyCertified`; `misaka-cli palw submit-object --object f.obj --yes` carries it.
   The test `every_catalog_class_on_a_registered_graph_is_covered_by_a_drillable_family` holds
   that every class the catalog can express on a registered graph has such a family on both
   lanes. A model whose graph reaches a kernel no shipped family drills is a new architecture,
   and needs a build whose court serves it — that boundary is ADR-0069 Decision 2's, not this
   ADR's.
3. **Bind the class.** `palw-certify bind --model-id <model id> --lane attempt --out b.obj` and
   submit it: the class is seated at the floor share and is weight-bearing from the next epoch
   boundary. `--lane fp` the same way admits its free-prompt claims.

If the family was certified on chain BEFORE the registration, the processor requires the floor
share at registration (ADR-0049 Decision H); `PalwRegistrationTermsV2` now carries the chain's
certified families, so the SDK and the panel price that registration correctly without an RPC of
their own. `a_model_this_build_never_pinned_is_seated_through_the_chain_alone` runs the route on
the Qwen3.5-2B graph-v3 class, which no RC set names.

## 7. Mainnet: the rules of operation (Decisions 9–13)

Mainnet's PALW is born the way every RC is born: the pinned genesis card
(`PALW_MAINNET_GENESIS_BONDS`, `PALW_MAINNET_GENESIS_ARTIFACT_ROOT`,
`PALW_MAINNET_QWEN36_ARTIFACT_ROOT`, `PALW_MAINNET_QWEN25_A16_ARTIFACT_ROOT`) assembled by the same
`palw_v2_params_on_base` the RC networks use, so the certification rules are the ruleset's, not a
mainnet special case (ADR-0042 Decision 11: the last RC and mainnet share one ruleset id, built
from one git tag). The card is empty until real operator keys exist; while it is, mainnet is the
hash-only network it is today, byte for byte. This section decides how the objects operate once
it is not.

**Decision 9 — Who may submit, and how much.** Anyone. A `FamilyCertified` or
`ClassLaneCertified` is an ordinary transaction under the standard mass ceiling, paying the
standard relay fee; no bond, no signature, no allow-list — the court grades the evidence and the
class's own profile hash grades the binding, so a gatekeeper would add nothing but a gatekeeper.
Two bounds hold the cost: at most `PALW_CERTIFICATION_MAX_VECTORS` (32) fault vectors per object,
and at most `PALW_CERTIFICATION_MAX_PER_BLOCK` (2) `FamilyCertified` objects graded per block,
counted in transaction order by the acceptance walk — the third is dropped and the block stands,
so a stranger's evidence can make a block slower to validate by two drills, never invalid.

**Decision 10 — Revocation.** There is no revocation object, on purpose. A class that misbehaves
is frozen by a contradiction certificate (`ClassFrozen`), which removes its weight and refuses its
claims; its certification record stays as history and does nothing. A *family* certificate can
only be wrong if the court that graded it is wrong, and a court defect is a ruleset change: under
ADR-0042 Decision 11 that is a new network identity, born with an empty chain set, on which every
family re-certifies through the same objects. Nothing on the old identity is edited.

**Decision 11 — Upgrade.** Within one identity, a family's coverage grows by posting a second
drill whose kernel set is a superset (a different digest, a second record; the class binding
finds whichever covers it). Across identities, everything re-certifies — the genesis set of the
new identity is derived by the coverage rule from the drills the new court grades
(`palw_rc_fp_certified_class_ids_v1`), and the chain set starts empty. A class's own profile never
changes (its id IS its profile), so a class never needs re-binding on one identity.

**Decision 12 — Expiry.** None by time. A certification is bound to the network identity that
recorded it and lives with the class record: a class reclaimed to Dormant and re-registered under
the same id keeps its free-prompt certification (same profile, same kernels) and takes its share
from the registration rules, which read genesis ∪ chain. What DOES lapse is everything, on a
ruleset change — the wipe policy below.

**Decision 13 — State version 16 and the migration that is not one.** Mainnet has no PALW state
today; a carded mainnet is born at version 16. Testnet-11 moves 15 → 16 by the Relaunch 5e
re-genesis, as every ruleset change has: a node that starts this build over a datadir written by
another version refuses at boot with the reason and the remedy (wipe, resync from peers announcing
this build's fingerprint), because `PalwStateCarriageV2::into_state` accepts only its own version
and the P2P handshake already refuses the old fingerprint. No in-place migration exists and none
is planned: a chain-certified set is chain history of one identity.

**Deployment and verification.** `docs/mainnet-palw-certification-runbook.md`: the card, the
build from the final RC tag, the simultaneous validator swap (every validator on one build —
a mixed fleet forks at the first certification object), and the verification ladder (devnet
rehearsal of the three transactions, the announce fingerprint, the first `FamilyCertified` on the
live chain read back from every validator's `[palw-lifecycle]` line).

**Decision 14 — An object too large for one carrier rides in chunks, reassembled in state.**
Measured on the RC drills: the A16 fixture's attempt drill is 82 KB, the QWEN36 fixture's 248 KB
(183 KB of it the profile repeated in every refutation's binding), the floor's 310 KB. A block
carries at most `max_block_mass / TRANSIENT_BYTE_TO_MASS_FACTOR` ≈ 125 KB of transaction bytes,
and a standard transaction is bounded the same way, so most drills do not fit one carrier and a
single-carrier rule would have made "permissionless" mean "only small graphs". `ObjectChunk {
group, index, count, bytes }` is a lifecycle object: `group` is the digest of the whole object's
bytes (`palw_object_chunk_group_id_v1`), each part is at most `PALW_OBJECT_CHUNK_MAX_BYTES`
(100,000), a group spans at most `PALW_OBJECT_CHUNK_MAX_COUNT` (8) parts. The chain keeps the
parts IN STATE (`pending_chunks`, in the state root, carried and reverted like every collection)
and, in the block that completes a group, hashes the assembly, decodes it, requires a
`FamilyCertified`, and applies it through the same arm a directly carried one goes through —
which is why the per-block grading cap counts a completing chunk as a certification. The table
holds at most `PALW_OBJECT_CHUNK_MAX_GROUPS` (8) half-assembled groups; a group nobody completes
within `PALW_OBJECT_CHUNK_TTL_DAA` (4,000) is evicted, in key order, when the room is needed. A
part that duplicates, a part under another count, an assembly that does not hash to its group,
or a chunked object of any other kind is refused and the block stands. `palw-certify drill`
writes the chunks beside the whole object and `misaka-cli palw submit-object` carries them as one
chained burst, each carrier funded from the previous one's change.

## Security amendment (2026-09-02) — the permissionless lane's griefing budget

**SA-1 — A chunk group's opener rents the SLOT it takes.** `ObjectChunk` groups are unsigned and
permissionless; eight pending groups × 800,000 bytes live in the state root for up to 4,000 DAA. A
griefer can hold all eight with junk for ~5.5 days at the price of carriage and block every honest
drill. Rule: past `Params::palw_certification_rent`, a chunk that OPENS a group must be carried by
a transaction paying `palw_object_chunk_group_rent_v1()` — the carriage price of everything a slot
may ever hold (`PALW_OBJECT_CHUNK_MAX_BYTES × PALW_OBJECT_CHUNK_MAX_COUNT` = 800,000 mass ⇒
10,000,000 sompi), flat, whatever `count` the opener declares. A chunk that merely extends an open
group pays nothing: the row is already paid for, and charging it would fall on the carrier that
completes. The opener is refused before the group reaches the state root.

*The price is the slot and not the declared `count`, deliberately.* The resource is one of eight
rows, and a group declaring two parts denies it exactly as completely as one declaring eight — so a
`count`-scaled price puts the griefer's optimum at `count = 2`, holding a row for a quarter of the
reasoned price and for LESS than Decision 14's own measured drills (248 KB ⇒ 3 parts, 310 KB ⇒ 4)
pay for the identical row. A rent an honest opener pays more of than a griefer is not a rent.

*What ships is a PRICE, where this amendment first asked for a refundable DEPOSIT.* Junk pays, and
it pays before the state root is touched; honesty is not refunded. A refund needs the chain to
remember which outpoint backs which group — a field on `PalwPendingChunksV2`, i.e. a
`PALW_STATE_V2_VERSION` bump, which is hashed into every state root and into `consensus_params_id`
and therefore re-mints every V2 network whether or not the fence is armed. That is the one thing an
activation may not do, so the deposit belongs to the next state-version bump, with the lock (the
group's outpoint joins `palw_v2_locked_bond_outpoints`) and the forfeit (it joins
`palw_v2_bond_burn_obligations`). Until then the honest/junk asymmetry this amendment named as its
point is the part that is missing, and the flat slot price is what stands in for it.

**SA-2 — Grading is priced before it is performed.** Two `FamilyCertified` are graded per block
whether or not they pass; a refused one still costs every validator up to 32 court re-executions.
Rule: past the same fence, a `FamilyCertified` whose carrier paid less than
`palw_certification_min_fee_v1(vector_count)` is **dropped ungraded and the block stands**, and it
spends none of the block's two grading slots. The price is derived from the vector count (compute
mass, as `misaka palw submit-object` already sizes it) rather than from the object's bytes, because
the count is a field the submitter writes and nothing ties it to how large the object is.

*Dropped, where this amendment first said the accepting block "is invalid".* The security property
is the same — nothing underpaid is ever graded — and the difference is who dies for a stranger's
transaction. Admission on the 0x4b band is stateless, so an underpaid carrier relays and mines
freely, and a template builder selects by fee without consulting this filter; making the accepting
block invalid is audit M-01's shape exactly, with honest miners producing the invalid blocks.

*And the block's cap counts court work, not chunk arithmetic.* Past the fence, a chunk charges one
of the two grading slots only when it actually assembles into a decodable `FamilyCertified` — the
same predicate the price is read from. Counting "completes a group" instead let sixty bytes
(`ObjectChunk { group: <fresh>, index: 5, count: 1 }`, or a one-byte payload that decodes to
nothing) burn a slot the transition was going to refuse anyway, and two of them per block dropped
every genuine certification the block carried.

*The rent is burned, not paid to the accepting miner.* Both prices are transaction fees, and a fee
goes to the coinbase of the block that accepts it — so a miner, who needs no stranger's cooperation
to get a carrier into a block, recovered ~90% of whatever he had just been charged and the claim
that the CPU is "bounded by fees the griefer actually paid" was false for the cheapest adversary
either rule has. Past the fence, `calculate_utxo_state` withholds `palw_object_rent_ceiling_v1` of
each certification carrier's fee from the block reward by don't-mint — the mechanism the C-08 bond
burn and the §F service share already use — capped at the fee, so carriage above the rent is still
the miner's and objects no rule prices (court moves, free-prompt commitments) burn nothing. The
residual, stated plainly: the worst case is still two graded objects × 32 vectors = 64 court
re-executions per block, and past the fence that block costs its author 20,000,000 sompi (0.2 BILI)
destroyed. The bound on validator CPU per block is `PALW_CERTIFICATION_MAX_PER_BLOCK ×
PALW_CERTIFICATION_MAX_VECTORS`, a constant this binary enforces; what the rent buys is that the
bound cannot be reached for free, by a miner or by anyone else.

**SA-3 — The card's keys are the operators' own**, generated on their hosts; only public rows enter
`params.rs` (the testnet-11 practice); a card with two rows sharing an operator id is refused by the
genesis gate.

**SA-4 — The SDK preflight and the gateway read genesis ∪ chain** (the open item above), because a
tool that reports a chain-certified class as uncertified will be "fixed" by an operator with a flag,
and flags that override chain facts are how a rolling re-genesis starts.

**SA-5 — and the slot is spent by a certification the grader ACCEPTED** (audit 2026-09-02, its own
fence `palw_chunk_cap_charge`). SA-2 above decides what a block's grading may be charged FOR; this
decides WHEN the charge lands, and the two were found separately and are fenced separately because
each is sound alone.

`PALW_CERTIFICATION_MAX_PER_BLOCK` is charged in the acceptance rehearsal, before the transition
runs, so the rehearsal's notion of "completes a group" has to BE the transition's or the cap is
spendable by objects that complete nothing. It was not: the rehearsal asked only whether a lone
chunk declared `count == 1`, so `ObjectChunk { group: anything, index: anything, count: 1, bytes:
eight bytes of nothing }` — sixty bytes at an ordinary fee — was charged a grading and then refused
by the transition, and two per block dropped every genuine `FamilyCertified` that block carried.
Certification is how a family earns the right to bear weight, so a block-cheap way to starve it is a
block-cheap way to keep an honest class weightless.

The rehearsal now runs the transition's own completion test — every part present, the assembly
hashing to the declared `group`, and the object it decodes to being a `FamilyCertified` — and spends
the cap only on a chunk that passes it. Requiring `index < count` alone is **not** that rule and
buys nothing: `index: 0, count: 1` was always available at the identical sixty bytes and starves the
identical cap, so a guard that asks only the index rule deletes a strictly dominated variant. Past
the fence the SLOT is charged in the apply's `Ok` arm, to an object the court actually graded and
accepted, and what an object costs BEFORE it is graded is the fault vectors it asks the court to
walk, against a per-block work budget. Behind a bare `Option<ForkActivation>`
(`palw_chunk_cap_charge`, `None` on every shipped preset), because which objects a block accepts
decides its state root.

*How the two fences compose.* The rent's predicate (`palw_v2_graded_vector_count`) and D14's
(`palw_chunk_completes_a_certification_v1`) both assemble and decode; D14's additionally demands the
whole hash to the declared group id, so anything D14 calls a completion the rent calls one too. The
slot gate is therefore never tighter than the work gate, and no object is charged a grading it was
not also priced for. Arming the rent WITHOUT D14 leaves one corner open — a chunk whose assembled
bytes decode to a `FamilyCertified` but do not hash to its declared group id is priced and charged a
slot, and the transition then refuses it — so the two are armed together or the rent is armed
knowing that.

## ADR-0144 alignment (2026-09-21)

The remaining gap — `misaka-palw-gateway` still cannot read the chain's certified set — is the
localhost `/v1` surface [ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md)
P1 puts in scope (and ADR-0077 Decision 3 already required). It is not a GPU marketplace and it
does not wait on ADR-0101. The SDK half is already closed.

## Mission alignment amendment — 2026-10-07

certificateのchain object化は維持する。新profileのcertificate受入にはpublic prosecutionのcoverage/evidenceを追加し、registrant自身が正直なcaptureから組み立てたrefutationの存在だけで新gateをPASSにしない。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0075-certification-is-a-consensus-object.md -->

</details>

<a id="source-0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote"></a>

<details>
<summary>0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md — 原文 L1–L521</summary>

[原文を開く](../adr/0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md) · SHA-256: `a273232bc2a6921de48bf235586b7ff68e5e11e89edc076f24a4a2327ee04cec`

<!-- ADR_SOURCE_BEGIN:0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md -->
~~~~~~~~markdown
# ADR-0108 — An extension is a manifest the verifier recomputes, and a receipt is evidence, not a vote

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


> **2026-10-06 future-profile boundary:** manifest recomputation establishes canonical identity,
> dependencies and admission/resource facts; it is not mandatory whole-inference recomputation.
> [ADR0172](0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) permits only approved
> versioned kernels and bounded plans, not manifest-uploaded executable verifiers. New large-model
> claim correctness follows [ADR0171](0171-probabilistic-constraint-checks-and-court-on-dispute.md)'s
> encoded probabilistic checks and bounded exact court on dispute. These decisions change no legacy
> manifest verifier or active consensus rule merely by documentation.

* Status: PROPOSED 2026-09-11, at the operator's request ("このような拡張をパーミッションレスで行える
  基盤を作成するADRを作成してから実装して"). **Decisions 1–9 IMPLEMENTED the same day** (§9),
  consensus-inert: no consensus object, acceptance rule, fence, parameter or fingerprint moves. A
  fleet takes it by an ordinary rebuild, and a node that never takes it is not partitioned from one
  that did.
* Builds on: [0046](0046-palw-v2-consensus-object-carriage.md) (an identity is derived, never
  declared), [0054](0054-palw-share-follows-production.md) / [0056](0056-palw-permissionless-class-admission-and-share-economy.md)
  (admission is a deterministic gate, not a vote), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  (a class the build never heard of is served from the registration the chain carries — behind a seal
  the operator arms), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) Decision 5 (a drill's
  evidence is readable by anyone), [0075](0075-certification-is-a-consensus-object.md)
  (certification is two lifecycle objects any transaction can carry), [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md)
  Decisions 3 and 5 and its SA-5 (a transformer is named by its manifest, a consumer recomputes, and a
  manifest nobody can fetch is no derivation), [0072](0072-the-ticket-is-the-execution.md) SA-2
  (the fork-id gate: a scheduled height is compared, never a claim about it), and the doctrine that
  consensus changes ship as an activation and never as a re-genesis.
* Answers the operator's 2026-09-11 assessment (§1): "the verification platform is mostly built; the
  extension platform is half built; what is missing is SOURCE → canonical form, shared across the
  kinds of thing a person can bring."
* Amends nothing in consensus. Adds one crate, one CLI verb with five forms, and one document format.
* Supersedes nothing.

## 0. The sentence this ADR is

**A person who made something — with a model, a converter, a drill, or by hand — writes ONE manifest
that says what it is, what it depends on, and what recomputing it must produce; any node's verifier
recomputes it and answers with exactly one of three classifications — *expressible now* (the chain
already has the object; admission is permissionless and the manifest says which object), *a node
extension* (the chain never sees it; a node that lacks the code answers "unverifiable here", never
"valid"), or *a ruleset change* (a fence; the verifier says what fingerprint the arming build would
print and whether arming it is a flag day, and activation stays a coordinated release); and a receipt
of that recomputation is evidence someone else can reproduce, signed by whoever ran it, that no
consensus path reads and no count of which admits anything.** The chain keeps naming identities it can
recompute. The manifest is how a stranger learns what an identity means. Nothing about the generator
of the thing — a person, Claude, GPT, Kimi — is recorded, asked, or trusted.

## 1. What exists, measured on `main` `40ac431b` (2026-09-11)

The operator's assessment, checked against the tree. Each row names the primitive that makes it true.

| What a person brings | The chain's entrance (on `main`) | Verified by | Permissionless? |
|---|---|---|---|
| A class of an existing architecture (weights + profile) | `PalwConsensusObjectV2::ClassRegistered` | `verify_class_admission_v6` — `class_id = H(profile)`, kernels, court cost, canonical job, PWU all recomputed | **chain: yes**; node: the SDK's compile-time lineage table decides what this build can *serve*; ADR-0067's `with_chain_classes_v1` arm is sealed by default (§1.2) |
| A family certification (drill evidence) | `FamilyCertified { evidence }`, `ClassLaneCertified { class_id, lane, profile }` | `certify_e2e_family_v1` / `certify_e2e_free_prompt_lane_v1` — the transition grades the vectors, records only what the grader returns | **yes** (`palw-certify drill\|bind`, `misaka palw submit-object`) |
| A model line, version, proposal, evaluation | `ModelLineFounded`, `ModelVersionPublished`, `ModelProposalPosted`, `ModelEvaluationPosted` | the fold | **yes past `palw_model_lines`**, which every shipped preset leaves `None` |
| An artifact derived from a model's answer | `DerivedArtifactV1 { object, signature }` | `misaka_palw_derive::verify` — grammar, transformer, artifact hash recomputed; consensus reads `kind != 0` and nothing else | **yes on chain**; verifiable only where the transformer's manifest is published (ADR-0078 SA-5, `registry::manifest_is_published`) |
| Court evidence | `CourtOpened` … `CourtAttnRootClaimed` | the court | yes, by construction |
| A new context width inside the ladder | a `ClassRegistered` whose profile carries the width (`n_ctx` is a field of `PalwShapeProfileV3`, so it is inside the id) | the same gate, at the ladder `palw_admission_shape_at_v1` derives from `Params` at the height | **yes** |
| A new kernel, a new court, a new state transition, a width past the ladder | a fence — 29 `palw_*` fields of `Params`, every one `Option<ForkActivation>` (`palw_fences_v1`) | `consensus_params_id` / `consensus_identity_id` / `consensus_schedule_id`, the fork-id gate | **no** — a release, and this week measured what that costs (§1.3) |

Five things the table shows are true at once:

1.1 **IDENTITY / COMMIT / VERIFY / ADMIT exist, per kind, and all four recompute.** A class id is a
hash of its profile; a family id is what the court's grader returns; a transformer id is a hash of its
manifest; a derived artifact carries the hashes a consumer recomputes. Nothing on this list is a
registry a person appends to. That is the property this ADR must keep.

1.2 **What is sealed is sealed for a reason, not for lack of code.** `PalwClassSdk::resolve_chain_registered`
can serve a class the build's tables never heard of, from the registration the chain carries, and
refuses unconditionally until `with_chain_classes_v1()` is called — "arm it deliberately once the
operator accepts interpreted execution" (ADR-0067 Decision 5). The seal exists because an unpaid
panel seat is cheap to buy and a registrant who can seat itself judges its own class. This ADR does
not lift it (Decision 8).

1.3 **A fence is a flag day whether or not it is armed.** Adding a `Params` field moves
`consensus_params_id` for every preset that sets it (ADR-0095's field, once written, moved
testnet-11's printed fingerprint `060e3597…` → `ecbdbc22…` on 2026-09-11); arming it at a genesis
height moves `consensus_identity_id`, which the handshake refuses on; scheduling it at a future height
is compared by the fork-id gate the moment the schedule is announced — the third flag day cut every
un-upgraded node the day the fleet restarted, at DAA ≈ 2,241 against a fence at 2,400. None of this is
a defect: it is what a rule change *is*. It is why no manifest may ever "activate" anything (Decision 6).

1.4 **SOURCE → canonical form is the missing step, and it is missing per kind.** `palw-class inspect`
reads an artifact file; `palw-certify drill` runs a fixture drill; `palw-derive` reads a transformer by
name; a context width is a number an operator passes to `bind --n-ctx`. Each entrance takes its own
input in its own shape, and none of them takes "here is a thing I made, its identity, what it depends
on, and what recomputing it must give" as one document. A person with a new thing has to know which
tool, which flag, and which vocabulary before the first refusal tells them anything.

1.5 **No entrance says which of the three things it is.** `verify_class_admission_v6` refuses a profile
whose kernel this build lacks (`reachable_kernels_v1` finds nothing to price) with the same shape of
answer it gives a profile that is malformed. A person cannot tell "your file is wrong" from "this build
cannot judge this" from "this needs a release". Every refusal names a field — the repository's
discipline holds — but the *tier* is not a field.

## 2. The boundary, stated as three tiers

```text
                     fixed verifier: this build's consensus-core + SDK + derive
                                          │
        ┌─────────────────────────────────┼─────────────────────────────────┐
        │                                 │                                 │
   A. EXPRESSIBLE NOW              B. NODE EXTENSION                C. RULESET CHANGE
   the chain has the object;       the chain never sees it;        a fence; accept/reject or a
   this build recomputes it        a build must carry the code     state root moves
        │                                 │                                 │
   permissionless admission        permissionless on chain,        a coordinated release:
   (existing object, existing      verifiable only where the       ADR-0072 SA-2's gate, ADR-0105
   gate, existing fee)             code is; SA-5 says publish      §7's notice, one host at a time
```

* **A** is a class whose kernels this build prices, a certification this build's court grades, a
  transformer this build ships, a derivation over one, a line or version past its fence, a context
  width the ladder admits.
* **B** is a transformer this build does not ship (the chain admits its id; a consumer without the
  code says *unverifiable here*), a model architecture with no lineage in this build (the chain admits
  the registration if the kernels price; a node without the lineage cannot serve it — ADR-0067's arm
  is exactly this case for one family), a new derived kind. **B is permissionless at the chain and
  reproducible where the code is published; it is never "valid" on a machine that could not run it.**
* **C** is a new kernel, a new court, a width past the ladder, a new object, a changed transition.
  **C is never permissionless, and this ADR says so in the report rather than pretending otherwise.**
  A candidate can be *described* (Decision 6) and *reproduced* (Decision 4); it is activated by a
  release that every peer must carry, because a validator cannot verify semantics it does not have.

The user's instinct — "追加物を無限にpermissionlessにするのではなく、固定された verification language で
表現できる範囲を十分広くする" — is this diagram. The platform's job is to make A wide and B honest, and
to make C's cost visible before anyone pays it.

## 3. Decisions

### Decision 1 — One manifest, canonical, with a derived identity

`PalwExtensionManifestV1` is a JSON document. Its canonical form is RFC 8785 (JCS) as
`misaka_palw_derive::canon_json::canonicalize_json` already implements it — sorted keys, no duplicate
keys (refused before any parser could choose), no lone surrogates, shortest-form numbers. Its id is

```text
extension_id = BLAKE2b-512( key = "misaka-palw/extension-manifest/v1", canonical bytes )
```

the same shape `transformer_id_v1` has. Two people who write the same manifest write the same id; a
manifest whose bytes were reordered by an editor has the same id; a manifest with a field changed has
a different one.

```jsonc
{
  "manifest": "misaka-palw/extension-manifest/v1",
  "kind": "model-class" | "context-profile" | "family-certification" | "lane-certification"
        | "derived-transformer" | "ruleset-candidate",
  "name": "a name the person chose — never an identity",
  "network": "testnet-11",
  "source":   { "digest": "<64 hex>", "note": "what made it; opaque; never checked" },
  "artifact": { "root": "<128 hex>", "bytes": 1795427276, "path": "qwen25-a16.palwart" },
  "requires": { "ruleset_id": "<64 hex consensus_params_id>",
                "fences": { "palw_kary_court": "active" },
                "kernel_ids": ["<128 hex>", ...] },
  "declares": { "object_id": "<128 hex — the identity this kind derives>",
                "capabilities": ["attempt", "fp"],
                "resource_bounds": { "memory_bytes": 2147483648 } },
  "verification": { "vectors": "<path or inline, kind-specific>",
                    "expected": { "artifact_root": "<128 hex>", "family_id": "<128 hex>" } },
  "admission": { "object": "ClassRegistered" }
}
```

**The identity is derived, never declared** (ADR-0046, restated for manifests). `declares.object_id`
is what the person *claims*; the verifier recomputes the kind's identity from the kind's own input —
`H(profile)` for a class, the grader's family id for a certification, `H(manifest bytes)` for a
transformer — and a mismatch is refused **by field name**, never averaged, never taken on trust.
`source.digest` is the only field the verifier never checks: it is the person's own record of what
they started from, and this ADR states in §6 that nothing downstream may ever read it as evidence.

`requires.ruleset_id` is the `consensus_params_id` the manifest was written against. A verifier on
another ruleset does not refuse — it reports the mismatch and verifies anyway — because a manifest
written before a fingerprint moved (§1.3 happened twice this week) is not thereby wrong.

### Decision 2 — Every answer names its tier

`verify_extension_v1` returns `PalwExtensionReportV1`, and the report's `classification` is one of:

```text
Expressible   { admission_object, would_be_refused: Option<field-naming reason> }
NodeExtension { missing: what this build lacks, by name — a transformer, a lineage, a kernel id }
RulesetChange { fences: [(name, requested height, this build's value)],
                would_print: { params_id, identity_id, schedule_id },
                flag_day: bool, reason }
Refused       { field, reason }
```

**There is no bare `PASS`.** A verifier that could not run the thing says `NodeExtension`, and a
report that says `Expressible` says *which* object it would be. `Refused` is for a manifest that is
wrong about itself — an id that does not recompute, a bound that is zero, a vector whose expected root
the recomputation does not produce. A limit of this build is not a verdict on the manifest
(the repository's own rule: a limit is not a verdict).

### Decision 3 — Verification has three depths, and the report says which it reached

```text
Structural   any machine: canonical form, field bounds (§6 SA-1), the id, the kind's own
             identity recomputed from what the manifest carries INLINE (a profile, a
             transformer manifest, a fence list)
Vectors      the declared vectors re-run with this build: a transformer over its DSL vectors,
             a drill's fault vectors through certify_e2e_family_v1, a class profile through the
             admission gate at the named shape
Full         the same computation the chain's transition applies, over the bytes the person holds:
             the artifact file read and its root recomputed (the SDK's pairing), the certification
             evidence graded, the registration object built and the gate asked
```

A machine that holds no 34 GiB artifact stops at Structural or Vectors and the report says
`depth_reached: Vectors`, `depth_requested: Full`, `stopped_at: "artifact.path not readable"`. That is
not a failure; it is the sentence the operator's Mac-versus-GPU case asked for.

### Decision 4 — A receipt is evidence of reproduction, signed by whoever reproduced it — and it is not a vote

`PalwExtensionReceiptV1` is a canonical JSON document carrying the manifest's `extension_id`, the
report (classification, depth, every check by name with `pass | fail | skipped(reason)`, the
recomputed roots), the verifier's `ruleset_id` and crate version, a timestamp the verifier chose, and
optionally an ML-DSA-87 signature by the verifier's key over the receipt's canonical bytes, under the
context `misaka-palw/extension-receipt/v1`. That context is a constant of the extension crate and is
**deliberately not added to the bundle's `signature_contexts_root`**: that root is a consensus set
(`palw_signature_contexts_v2`, armed at genesis), and adding a member moves the fingerprint — a
receipt signs nothing the chain ever verifies, so the chain's set has no reason to know it.
`receipt_id = H(canonical bytes without the signature)`.

What a receipt is for: two receipts from two builds that disagree about the same manifest are a bug
report with a preimage attached. A person who cannot run the Full depth can read receipts from people
who could, and can check that those people signed what they claim.

What a receipt is **not**, stated as a rule the tests hold (§7 I-9): **no code path in `kaspad`,
`kaspa-consensus`, `kaspa-consensus-core` or the SDK reads a receipt; no count of receipts changes any
admission, activation, fence, seat, price or share.** A hundred receipts saying `Expressible` do not
admit a class; the gate does, exactly once, when the object is carried. A hundred receipts saying a
ruleset candidate reproduces do not arm a fence; a release does. This is the difference between
reproducibility and a Sybil vote, and it is the whole reason the chain can afford to let anyone verify.

### Decision 5 — Preflight and submit go through the objects that already exist; nothing new rides

`preflight` = `verify` at depth Full, plus the shape the chain would judge at a height: `Params` at
the node's current DAA (`palw_admission_shape_at_v1`: which court, which ladder, which prompt-ids
form), the object it would build, what it costs (a carrier fee for a certification; a registration's
share and slash value from the network's terms), and the refusal it would meet — **before any fee**.

`submit` builds the existing object and files it:

| kind | object built | how |
|---|---|---|
| `model-class`, `context-profile` | `ClassRegistered` via `PalwClassSdk::build_post_genesis_registration` | needs the registrant's bond key (the same object `kaspad --palw-register-class` files in-process); the CLI signs with its own key and files through `submit-object` |
| `family-certification` | `FamilyCertified` | `submit-object`, unsigned (the fee is the rent, ADR-0075) |
| `lane-certification` | `ClassLaneCertified` | `submit-object` |
| `derived-transformer` | nothing — a transformer is not a chain object; each derivation over it rides per claim (`DerivedArtifactV1`) | `submit` refuses, naming `admission.object: none` |
| `ruleset-candidate` | nothing — a fence is a release | `submit` refuses, naming the tier |

No manifest, receipt, or extension id is written to the chain. The chain keeps naming what it can
recompute (a class id, a family id, a transformer id); the manifest is the document a stranger fetches
to learn what that id means, published wherever the artifact is (Decision 9).

### Decision 6 — A ruleset candidate is described and costed; it is never activated by a manifest

`kind: ruleset-candidate` names fences and heights: `{"palw_heartbeat_transparent": 12000}` or
`{"palw_context_ladder": "genesis"}`. The verifier takes this build's `Params` for the named network,
sets the fences as asked, and reports what the arming build **would print** — `consensus_params_id`,
`consensus_identity_id`, `consensus_schedule_id`, the fence schedule — beside what this build prints;
whether the identity moves (arming at genesis) or only the fingerprint and schedule (a future height);
and whether the build refuses the combination (`validate_palw_v2`, which refuses a fence its ruleset
cannot carry — ADR-0096's decode constraint, ADR-0105's rule without the heartbeat lane). A fence name
this build does not have is reported as such: the candidate needs a build before it needs a height.

That is the whole of what a manifest may do to a rule: say what arming it costs. Activation is ADR-0105
§7's recipe — schedule the height in the preset, rebuild, and give the notice the fork-id gate makes
necessary. A candidate's receipts are reproducibility evidence for the *release discussion*; the
release is what activates.

### Decision 7 — A context width is a class inside the ladder and a ruleset candidate outside it

`kind: context-profile` is `model-class` with the width named: the manifest carries a profile (or a
registered class id and a width), the verifier projects the profile at that width and asks the
admission gate at the network's shape. Inside the ladder the answer is `Expressible { ClassRegistered }`
— the same object, a different id, because `n_ctx` is inside the id. Outside it, the gate's refusal
is translated into `RulesetChange { fences: [palw_context_ladder …] }` with the height field left for
the person to fill, and the report says which wall (the ladder, the court's cost shape, the DA form)
the width hits first. ADR-0097's fit walls and ADR-0103's held context live on branches as of this
writing; this kind maps onto whatever walls the build carries, by asking the gate rather than by
restating them.

### Decision 8 — The chain-class seal stays; the manifest tells the operator it is there

`resolve_chain_registered` stays sealed behind `with_chain_classes_v1()` for ADR-0067 Decision 5's
reason. A `model-class` report says, beside `Expressible`, whether **serving** the class on this build
needs the arm (`serving: { in_build_table: false, needs_chain_classes_arm: true }`), so a person learns
before registering that their class will be registered, judged, and — until an operator arms the seal
— served by nobody. Lifting the seal is an economics decision (paid seats) this ADR does not make.

### Decision 9 — Publication and discovery are off-chain, and the id is what makes that safe

A manifest is published beside its artifact — a file, a URL, a content-addressed store; this ADR does
not choose. What makes that safe is that nothing depends on *where*: the id is a hash of the bytes,
the identities inside it recompute, and a manifest fetched from anywhere is either the one whose id
was quoted or refused. The verifier **never follows a reference on its own** (§6 SA-2): every byte it
reads is a path the person handed it.

## 4. The CLI

```text
misaka palw extension inspect   <manifest>              what it is, its id, its tier by structure alone
misaka palw extension verify    <manifest> [--depth structural|vectors|full] [--receipt-out <file>]
                                                        [--key-file <path> | --key-stdin]  recompute; write a receipt
misaka palw extension preflight <manifest>              verify at Full + the chain's shape at the node's DAA
misaka palw extension submit    <manifest> --key-file <path> [--bond <txid:index>] [--yes]
                                                        build the existing object and file it (dry-run unless --yes)
misaka palw extension receipt-verify <receipt> [--manifest <file>]
                                                        the signature, the id, the ruleset it was made on
```

The key is named the way every keyed `misaka` command names it — a 0600 seed file or stdin, never
an argument or the environment (ADR-0063 SA-1); `--bond` is the registrant's producer bond, required
for a class because the registration is signed by that bond's key.

Exit codes distinguish the tiers so a script can branch: expressible-and-would-be-admitted at the
depth asked for `0`; refused (the manifest is wrong about itself, or expressible but the chain would
refuse it — already registered) `20`; node extension (unverifiable here) `21`; ruleset change `22`;
a depth not reached because the machine lacks the bytes `23`. The draft of this section proposed
2/3/4/5; `2` is clap's argument-error code and 3–5 were already `NETWORK_MISMATCH`, `CONNECTION` and
`NODE_NOT_SYNCED` in `misaka-cli`'s `exit` module, so the tier codes took the first free block at
20. Every non-zero exit prints the field or the missing thing by name. The CLI's own codes still
apply around them: `1` for a plain error (a missing `--bond`, a transformer that has nothing to
file), `3` for a manifest whose network is not `--network`, `4` for a node that cannot be reached.

## 5. What this costs

A manifest is small (bounded, §6 SA-1). Structural verification is microseconds. Vectors is what the
vectors cost — a drill's fault vectors through the court are seconds; a transformer over its DSL
vectors is milliseconds. Full is the artifact: the SDK's pairing reads the whole file once (1.7 GiB
dense in seconds, the 34 GiB mmap tier in minutes on a cold disk). Nothing here adds a byte to a block,
a millisecond to a transition, or a field to `Params`.

## 6. Security amendments, stated before the build

* **SA-1 — bounds.** A manifest is at most 1 MiB canonical; at most 4,096 vectors; at most 64 fences;
  every hex field exactly 64 or 128 characters; every path relative and free of `..`. A manifest over a
  bound is refused before it is parsed further. A bound exceeded is no report.
* **SA-2 — the verifier follows nothing.** No URL, no fetch, no "the manifest says the artifact is
  here". Every byte comes from a path the person passed on the command line, and a path the manifest
  names is resolved relative to the manifest's own directory and refused if it escapes it.
* **SA-3 — a receipt names the ruleset it was made on**, and `receipt-verify` refuses to call a receipt
  made on another `ruleset_id` a receipt for this one. A receipt from a devnet is not evidence about
  testnet-11 (the same object can be Expressible on one and RulesetChange on the other).
* **SA-4 — the signature context is its own.** `misaka-palw/extension-receipt/v1` is used for
  receipts and nothing else, and it is not in the chain's context set (Decision 4); a signature under
  another context does not verify, so a receipt cannot be replayed as an attestation, a bond message,
  or anything else ML-DSA signs in this system — and nothing signed for the chain verifies as a receipt.
* **SA-5 — no aggregation, anywhere.** No struct in consensus, node, SDK or CLI holds a *set* of
  receipts. `receipt-verify` takes one. This is the mechanical form of Decision 4.
* **SA-6 — `submit` signs with the operator's key, never one the manifest names.** A manifest carries
  no key material and no signature; a field that looks like one is refused as unknown.
* **SA-7 — `source.digest` is inert.** It is carried for the person and is not an input to any check,
  any id, or any report field other than its own echo. A build that ever reads it as evidence has
  changed this ADR.

## 7. Invariants the tests hold

* **I-1** The same manifest with keys reordered, whitespace changed, or written by another serializer
  has the same canonical bytes and the same `extension_id`; a duplicate key is refused; a changed field
  changes the id.
* **I-2** A manifest whose `declares.object_id` is not what the kind recomputes is `Refused` naming
  `declares.object_id`, for every kind.
* **I-3** Every field bound in SA-1 is enforced at the boundary, with the offending field named, before
  any kind-specific work.
* **I-4** `model-class`: the RC's own genesis rows (the SDK's built-in ledger) verify `Expressible {
  ClassRegistered }` and the report says they are already registered; a profile whose kernels this
  build's vocabulary cannot price is `RulesetChange` naming the kernel (a new kernel is a release,
  ADR-0067); a profile no certified family covers is `Expressible` and the report says *weightless*
  (ADR-0069 Decision 5); an artifact container no lineage of this build sniffs is `NodeExtension`
  naming the lineage; the serving flag says whether the class is in the build's table.
* **I-5** `family-certification`: the shipped attempt-lane drill evidence verifies to the pinned
  family id at depth Vectors; one flipped vector is `Refused` with the grader's own error name.
* **I-6** `derived-transformer`: a manifest naming a transformer this build ships verifies at depth
  Vectors, and one naming the previous published tree's id resolves through
  `PRIOR_SOURCE_TREES_SHA256_HEX`; a name this build lacks is `NodeExtension`; a widened ceiling is a
  different id (ADR-0078 SA-2, re-pinned here).
* **I-7** `ruleset-candidate`: a dormant fence at a future height → params id and schedule id move,
  identity does not, `flag_day: true` with the reason "the fork-id gate compares heights"; the same
  fence at genesis → identity moves; an unknown fence name → the report says a build is needed; a
  combination `validate_palw_v2` refuses is reported with its refusal.
* **I-8** A receipt round-trips: sign, canonicalize, verify; a receipt whose report byte changed after
  signing fails; a receipt made on another ruleset is refused by `receipt-verify` naming `ruleset_id`;
  an unsigned receipt verifies as unsigned and says so.
* **I-9** No aggregation: the workspace has no dependency edge from `kaspad`, `kaspa-consensus`,
  `kaspa-consensus-core`, `misaka-palw-sdk` or `misaka-palw-derive` to the extension crate (`cargo
  tree -i misaka-palw-extension` names only the CLI), so nothing that decides anything can read a
  receipt. Recorded in §9 with the measured output.
* **I-10** The verifier never opens a path outside the manifest's directory, and never a URL.

## 8. What is deliberately not decided, and what is not verified

* **A `service-descriptor` kind.** The operator's list has it; `main` has no chain object for a
  service (ADR-0101's "anyone serves" lives on a branch). Reserved as a name, refused as unknown until
  there is an object for it to name.
* **Registration terms over RPC.** `PalwRegistrationTermsV2` (registered class ids, registered
  artifact roots, chain-certified families) has no RPC on `main`; `kaspad --palw-register-class` reads
  it in-process. `preflight` therefore judges against the network's genesis terms and the fences at the
  node's DAA, and its report carries `chain_terms: "genesis only — the node exposes no terms RPC"`. A
  `getPalwRegistrationTerms` RPC is the natural next step and is not built here.
* **Sharded seats (ADR-0099) and held context (ADR-0103)** are on branches; the `context-profile` kind
  asks the gate this build has, and will ask the walls those ADRs add when they land, without a change
  here.
* **Lifting the chain-class seal** (Decision 8) waits on paid seats.
* **Not verified:** a Full-depth run over the 34 GiB Qwen3.6 mmap tier on a fleet host (the 1.7 GiB
  dense tier was measured — §9); a `submit` of a `model-class` against a live network (the
  object-building path is the SDK's, already exercised by `kaspad --palw-register-class`; the CLI
  wiring is exercised in dry-run, and every node-reaching path was exercised only as far as the
  connection refusal).

## 9. Number hygiene and implementation record

`docs/adr/README.md` on `main` at `40ac431b` says the next free number is 0108 (0099–0104 and
0106–0107 are resident on other branches; 0105 landed here). This ADR takes **0108**. A concurrent
claimant renumbers the later writer. **The next free number is 0109.**

Implementation, 2026-09-11, on `feat/adr-0108-extension-envelope`, on top of `main` `40ac431b`.

**What was built.** One crate, `misaka-palw-extension` (`manifest.rs`, `report.rs`, `verify.rs`,
`kinds/{model_class,certification,derived_transformer,ruleset_candidate}.rs`, `receipt.rs`); one CLI
module, `misaka/apps/cli/src/palw_extension.rs`, behind `misaka palw extension` with the five verbs of §4;
one operator guide, [`docs/palw-extension-envelope.md`](../reference/palw/palw-extension-envelope.md), with a
runnable example manifest per kind in [`docs/extension-manifests/`](../reference/manifests/extensions/).
Consensus-inert as promised: nothing under `consensus/`, `kaspad/`, `protocol/` or `rpc/` moved, and
the only edit outside the new crate and its CLI module is one summary line in `palw_fp.rs`'s
`submit-object` so a registration prints as a line rather than as its whole `Debug`.

**Measured, 2026-09-11 (this Mac, `cargo test` debug builds).**

* **Tests: 23 in the crate, all passing** — 15 in `tests/invariants.rs` (one per I-1…I-10, plus the
  three extra cases §7 implies: a kernel outside the vocabulary, a container no lineage sniffs, a
  class no family covers), 6 in `tests/doc_examples.rs` (every example manifest in `docs/`, so a
  stale example is a failing test), 2 unit tests (every fence of `palw_fences_v1` can be set by name;
  the class probe answers as the SDK's `preflight_admission` does, row for row, over the whole
  ledger). `cargo test -p misaka-cli`: **97 passing**, unchanged.
* **I-9, the `cargo tree -i` this ADR requires** — nothing that decides anything can read a receipt:

  ```console
  $ cargo tree -i misaka-palw-extension
  misaka-palw-extension v1.1.0 (…/misaka-palw-extension)
  └── misaka-cli v1.1.0 (…/misaka-cli)
  ```

  The CLI is the only edge in. A test in the crate re-reads the six deciders' manifests
  (`kaspad`, `consensus`, `consensus/core`, the SDK, derive, base0) and fails if any names this crate.
* **Gates.** `cargo clippy -p misaka-palw-extension -p misaka-cli --tests --no-deps -- -D warnings`
  clean; `cargo check --workspace --exclude misaka-palw-worker` clean (1 m 13 s);
  `sh scripts/ci-gates.sh --group fast` all green; `rustfmt --edition 2024 --check` clean on all 15
  touched `.rs` files; `python3 scripts/ci/check-repin-enumeration.py` unchanged at `main`'s baseline of
  7 unclassified files (79 in-scope files, 294 occurrences) — no literal in this work needs re-pinning,
  because every hex value in the tests and the examples is recomputed by the build rather than written
  down.
* **A Full-depth run over real weights**, the case Decision 3 exists for: the published
  `qwen25-1.5b-a16.palwart` (1,795,427,276 bytes), `context-profile` at `n_ctx` 24 —
  **43.5 s, 5.8 GB peak RSS**, root recomputed and matched, 1,589,424 pwu per inference. (For scale,
  `palw-class inspect` over the same file is 66.8 s because it pairs every row of the lineage; the
  verifier pairs the one class the manifest names.) The 34 GiB mmap tier remains unmeasured (§8).
* **The CLI, in dry run**, on the six example manifests: `inspect` 0; `verify` 0 for the
  context-profile at Vectors, the family certification, the transformer; 20 for the floor
  (already registered) and the lane binding (see below); 23 for `--depth full` with no artifact
  present; 22 for the candidate. `submit` refuses a transformer (1, naming `admission.object: none`)
  and a candidate (22, naming the tier), refuses a class with no `--bond` (1), and cuts the 754,377-byte
  family drill into 8 `ObjectChunk` carriers before reaching the node. Everything node-reaching was
  exercised only to the connection refusal (4): there was no local node.
* **Receipts**: signed with an ML-DSA-87 key, the id is over the unsigned bytes (signing does not move
  it), a changed report byte fails by name, another ruleset is refused naming `verifier.ruleset_id`,
  and a signature under any other context does not verify.

**Three things the implementation learned from the tree, which changed the design.**

1. **A lane binding's covering family must be one the CHAIN certified.** `ClassLaneCertified` searches
   `state.certified_family_records(lane)`, which only `FamilyCertified` objects write; the
   compile-time RC set is not in it, and testnet-11's genesis carries no `FamilyCertified` at all
   (measured: 3 `ClassRegistered` + 8 `BondRegistered`). So every lane binding is
   `NoCertifiedFamilyCovers` until someone files the family first, and the report says so and names
   the drill that fixes it. The attempt lane additionally refuses a class that already holds a share
   (`ClassAlreadyWeighted`) — the floor holds 1000‰, so the obvious example would have been refused
   twice. The example is the graph-v5@512 row's free-prompt lane instead.
2. **The chain does not refuse a second class over registered weights.** Nothing in the
   `ClassRegistered` transition reads `registered_artifact_roots`; that is the SDK's candidate rule
   (the 2026-08-28 mispairing). Reporting it as a refusal would have been a limit rendered as a
   verdict, so it is a named check and the tier stays *expressible* — which matters exactly for
   Decision 7's case, since the inventory root does not depend on `n_ctx` and a new width therefore
   registers weights the chain already holds.
3. **A court-capable row registers its inventory root, not the artifact digest.** The chain arm
   (`dense_artifact_by_registered_root`) accepts either; Full depth computes both and says which
   matched, because a manifest that names the wrong one is refused against its own file and the
   cheapest way to learn the right value is the refusal that prints both.

**Exit codes.** §4's draft proposed 2/3/4/5; those were taken (clap's argument error, and
`NETWORK_MISMATCH` / `CONNECTION` / `NODE_NOT_SYNCED` in `misaka-cli`'s `exit` module), so the tiers
took 20–23. §4 carries the real numbers.

**Not done here, and deliberately:** the `getPalwRegistrationTerms` RPC (§8) — the library already
takes live terms through `PalwExtensionEnvV1::chain_terms` and every report says which terms it
judged against; the chain-class seal (Decision 8); a `service-descriptor` kind (§8).

## Mission alignment amendment — 2026-10-07

manifest/receiptはevidenceでありvoteではないという原則を、非Panel bondのproof受理にも適用する。extension readinessのjury多数決でpublic prosecution gateを省略しない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md -->

</details>

<a id="source-0185-paid-lock-randomness-bootstrap-and-paid-sources"></a>

<details>
<summary>0185-paid-lock-randomness-bootstrap-and-paid-sources.md — 原文 L1–L33</summary>

[原文を開く](../adr/0185-paid-lock-randomness-bootstrap-and-paid-sources.md) · SHA-256: `b85e913e2e12aa69e3767f3b97141b2354c3efe5004d14a58b2c256a13c0553f`

<!-- ADR_SOURCE_BEGIN:0185-paid-lock-randomness-bootstrap-and-paid-sources.md -->
~~~~~~~~markdown
# ADR-0185 — paid claim の lock 乱数:後続 seal の commitment、最初の lock のみ固定 seed、承認済み paid claim を beacon source に

- **Decision:** Accepted — ユーザー決定 2026-10-17(F1・F2・bootstrap、ADR-0181 の例外変更経路で記録)。
- **Scope:** RFC-0007 paid claim(`palw_claim_challenge_v2`)の checker 抽選の乱数源と beacon source の範囲。PALW 合意方式・既存数値は変更しない。
- **Implementation:** `integ/validation-r2`(`8c5124c6a`、`7962446f3`)。`PalwClaimChallengePolicyV2::bootstrap_seed`(dormant、probe と全 preset で `None`)、`misaka_palw_challenge::paid_opt_in::collect_paid_seal_reveal_seed_v6`、`paid_claim_seal_anchor_v2` / `paid_claim_bootstrap_anchor_v2`、`palw_exec_v2_credit_approval_v1`。
- **Verification:** 単体試験のみ(challenge 6/6、core 41/41 と bootstrap 3/3)。実ノード試験・drill は未実施。
- **Activation:** なし。fence は全 preset で `None`、t12 identity 不変。
- **Related:** RFC-0007、[検証記録](../audit/palw/design-reviews/consolidated-validation-2026-10-15.md) §1・§4・§5、PRINCIPLES §8(乱数としての commitment / VRF は可、計算証明は不使用)。

## 背景

RFC07 adapter 以降、beacon event を出せるのは選ばれた checker が承認した paid claim の Final だけで、paid claim の lock は Final を持つ source を要した。新しいチェーンでは最初の event を作れず、lock 後の source も無限に後続の承認を要した(検証記録 F1)。

## 決定

1. paid claim の lock 乱数は、対象 claim の commit より後に seal された claim の commitment(seal 時に内容と salt が固定、seal 期間後に公開)から作る。source は Final を要しない。承認済み Final は報酬・credit 側でのみ要求する。
2. seal 由来の seed が得られず、承認済み paid Final もまだ無い最初の lock に限り、genesis / 年次フォークで pin した固定 seed を使う。固定 seed で lock した claim は抽選が予測可能なため EXEC credit を得ない(event は出す)。
3. 承認済み paid claim を beacon source に含め、paid job の opt-in source 登録を許す(同じ policy 下のみ)。

4. onboarding / conformance beacon の窓(`PALW_ONBOARDING_POLICY_WINDOW_DAA_V1`)を、paid source claim の署名済み寿命(source 窓を含む)以上に広げる。120 → 1,024 DAA(暫定値)。120 では paid source の Final が窓に入らず beacon が UNAVAILABLE になった(検証記録 §6)。ユーザー決定 2026-10-17。

## grinding が効かない理由

対象は全 source の seal より先に commit する。seal は内容と salt を固定し、公開は seal 期間後。source 集合は seal 期間の終わりで閉じ、対象自身の producer・base job・相互 source 組は除外される。残る手段は reveal の保留だけで、それは attempt 全体の veto(deposit 没収)になり、結果の選択にはならない。前提:混ぜた source のうち少なくとも 1 つが正直。

## 帰結

pin する seed の値は network ごとのユーザー決定。fence 有効化は ADR-0181 の手続きに従う。

## 追記 2026-10-18(ユーザー決定)

1. **v4/v5 opt-in beacon と Panel v5 beacon も seal-reveal で読む。** 承認済み paid source の自身の lifetime には同じ policy の source window と審査期間が含まれるため、target の `final_by` までに Final になることは構造上ない。そこで F1 と同じ方法 — target の commitment 後に seal された opt-in source の salted commitment を seal window 後に開き、source の Final を待たない — で beacon を導出する(`collect_opt_in_seal_reveal_beacon_v6`)。withheld / forfeited の mixed source は veto、`k` 未満は UNAVAILABLE。適用は chain の route 上の v4/v5 collector すべて(`opt_in_seal_reveal_v6`)。v4/v5 自体が dormant な `palw_beacon_source_opt_in_v1` / `palw_beacon_source_fee_v1` の後にしか存在しない。node・Panel history・SDK の fresh verifier は同じ述語を読む。
2. **`PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1` を導出値にする**(200 → binding window + 遅延 + 収集 window(≥ paid source lifetime)+ depth + evidence deadline + challenge window = 1,208 DAA)。pass 後の DA 喪失による eligibility 喪失の規則はそのまま有効になる。
~~~~~~~~
<!-- ADR_SOURCE_END:0185-paid-lock-randomness-bootstrap-and-paid-sources.md -->

</details>

