# ADR-0028: PALW challenge sampling — a scheduler for re-execution, never a verdict

Status: **Proposed (draft for review).** Activates nothing; devnet / shadow / zero-credit
envelope unchanged. This ADR fixes the scheduling fabric that ADR-0027 left open: who
re-executes which job, when duties and windows open and close, what the randomness binds — and
what it is forbidden to decide.
Date: 2026-08-16
Relates to: ADR-0027 (premises P1–P3, the refutation model this ADR schedules work for),
ADR-0026 §4/§5 (the PRF-positions flow demoted there is given its surviving role here),
[`misaka-palw-slash-protocol-design-v0.1.md`](../misaka-palw-slash-protocol-design-v0.1.md)
(§9 dynamic inputs, §10 DA, §17 objective offenses, §18 economics, §23 deadlines),
[`palw-full-logits-trace-v2-design.md`](../palw-full-logits-trace-v2-design.md) (§10 economics,
§12 gates, §13 staging, §15 prohibited claims),
[`palw-legs-capture-measurement-2026-08-15.md`](../palw-legs-capture-measurement-2026-08-15.md)
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

**Reorg rule.** Assignment is a chain-scoped fact: a reorg deeper than `Δ_bind` recomputes
panels on the new selected chain, and duties re-anchor with their windows. Attestations and
refutations are statements about `C` — they remain valid in any chain that contains `C`'s
commitment; only duty, deadline and bounty bookkeeping move. `Δ_bind` must exceed the ordinary
merge-depth reorg envelope so re-anchoring is the exception, not the steady state.

### 3. Windows: DAA-denominated, stall-tolerant, sized from measured p99 — with defaults stated as defaults

All deadlines are DAA-score offsets (v0.1 §23, including "DAA stalls ⇒ deadlines stall").
Wall-clock examples below assume the 0.1-bps PALW network parameters (one block per 10 s).

| Window | Opens | Closes (default) | Meaning of expiry |
| --- | --- | --- | --- |
| `W_bind` | `daa(C)` | `+ Δ_bind = 120 DAA` (~20 min) | anchor fixed; panel and duties derivable |
| `W_replay` | anchor | `+ 360 DAA` (~1 h) | assigned re-executor must attest or refute; silence = objective no-show (v0.1 §17) |
| `W_challenge` | `daa(C)` | `+ 17_280 DAA` (~48 h) | permissionless refutation window; `credit(C)` evaluated at close |
| `W_answer` | opening call included | `+ 360 DAA` (~1 h) | committed material must be opened; silence = objective DA offense |
| `W_round` | bisection rung | `+ 360 DAA` per rung | non-response at any rung = objective offense (`M-O3`) |

Sizing rule, not numerology: `W_replay ≥ κ · p99_cold_replay(class, job ceiling)` with `κ ≥ 3`,
where `p99_cold_replay` is the **measured** cold, no-KV, per-class replay cost at the job-size
ceiling — the same §12 gate item ADR-0026 already demands ("measured replay capacity fits the
challenge window at p99"). `W_answer` and `W_round` are sized by the SAME rule, not shorter:
answering an opening call regenerates the row by re-execution unless material was retained, and
a bisection rung can demand state the operator must replay to reach — a response window shorter
than a replay would make honest silence indistinguishable from withholding, which no objective
offense may do. `W_challenge` must additionally fit the degraded ladder:
`≥ W_replay + L_bisect · W_round` with `L_bisect ≈ 20` — the defaults above put that sum at
~21 h, leaving ~27 h of margin, closing ADR-0027's "sizing it is open work" with a number that
can be attacked. Every default in the table is a Stage-1 placeholder that MUST be re-derived
from the per-class measurement before any slash activates; shipping the placeholder as if
measured is a §15-class violation.

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
* **Funding attaches to the credit event, not the job.** Self-originated jobs have no orderer
  and no execution fee; the replay fee for the panel and the challenger bounty (≤ 10 % of
  slash, v0.1 §18.4) are priced into PALW credit issuance. A no-show forfeits the fee and is
  an objective offense; fees for unchecked jobs are never paid because unchecked jobs are
  never credited (§1).
* The pre-reward inequality (v2 design §10, ADR-0027 §3) is restated with this ADR's terms and
  becomes a **registration-time check**:

```
P_check · S_eff ≥ λ · G_max        λ ≥ 2.0
  P_check = P(≥1 honest full replay AND timely inclusion within W_challenge)
            — measured from Stage-0/1 refutation drills, never assumed
  S_eff   = slashable bond reachable by refutation      (max_leverage ≤ 1.0:
            credit mintable within one unbonding period must not exceed S_eff —
            under P3 deterrence is economic, so this inequality is load-bearing)
  G_max   = credit mintable from the dishonest commitment
```

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

## Assumptions that remain (stated so they can be attacked)

1. **1-of-N honest replay, funded** — inherited from ADR-0027, now with the funding mechanism
   (§4) that makes it enforceable rather than aspirational.
2. **Censorship-resistant inclusion within `W_challenge`** — inherited; §3 turns "sizing it is
   open work" into attackable defaults with a measurement obligation.
3. **Honest measurement.** `p99_cold_replay` and `P_check` are published, reproducible
   artifacts; a class that games its p99 shrinks its own dispute windows and self-refutes on
   the first real dispute.
4. **Grinding buys scheduling only.** An executor who grinds anchors or identities can shape
   *who* checks and *when* — never *whether* checking can catch it (full replay) nor *whether*
   anyone may (permissionless window). If a concrete grinding path to more than scheduling is
   found, it attacks §2 and this ADR must be amended.

## Consequences

* **New objects to land (consensus-inert first, like everything before them):** the v2
  assignment evaluator (a domain-keyed `select_verifiers` twin), the duty/no-show bookkeeper
  over DAA windows, on-chain envelopes for attestation, opening call and answer (the wire
  bodies exist in `palw_legs`; what is missing is their chain carriage), and the telemetry that
  measures `P_check`, inclusion latency and no-show rates in shadow.
* **Registration grows two published numbers per class:** `p99_cold_replay` at the job ceiling
  and the checkpoint interval's localization cost — plus the §4 inequality check. The §12
  checklist gains: `[ ] windows re-derived from measured p99`, `[ ] P_check measured in shadow`,
  `[ ] no-show/inclusion telemetry published`.
* **ADR-0026 §5 closes.** The dynamic-`q` section's question ("how many challenges") has its
  successor: `q` counts funded replays (§4), the PRF flow schedules audits that decide nothing
  (§5), and the minimum-`f` parameter is gone with the sampling security model.
* **The opening seam acquires its consumer.** `v2-legs-open`'s refusal property — never answer
  for a root you cannot reproduce — is exactly what makes §5's silence offense fair: an honest
  operator can always answer for an honest commitment, so unanswerability past `W_answer` is
  evidence about the commitment or its DA, not about luck.
