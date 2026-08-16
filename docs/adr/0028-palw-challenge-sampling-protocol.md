# ADR-0028: PALW challenge sampling — a scheduler for re-execution, never a verdict

Status: **Accepted (architecture).** Activates nothing; devnet / shadow / zero-credit envelope
unchanged. This ADR fixes the scheduling fabric that ADR-0027 left open: who re-executes which
job, when duties and windows open and close, what the randomness binds — and what it is
forbidden to decide. Promoted from Proposed on 2026-08-16 after a numeric review against the
real network parameters: the first draft's 48 h challenge window exceeded both pruning horizons
(30 h at 0.1 bps, 38 h on the 120 s net) — windows are now pruning-constrained, they derive a
credited-job ceiling (§3), and the funding mechanics are concrete (§4a–4e).
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

**Reorg rule.** Assignment is a chain-scoped fact: a reorg that replaces the anchor block
recomputes panels on the new selected chain, and duties re-anchor with their windows.
Attestations and refutations are statements about `C` — they remain valid in any chain that
contains `C`'s commitment; only duty, deadline and bounty bookkeeping move. `Δ_bind` is a
**settling offset, not a finality bound**: duties may derive from a young anchor because every
offense that references the anchor (no-show, deadline) is prosecuted only after the anchor is
final (§3's `finality < W_challenge` rule) — an assignee that acted on an anchor later reorged
away simply sees its duty re-drawn, and nothing is chargeable against the vanished draw.

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
`S_a · P(fraud) · P_refute > c_replay`; at current magnitudes (bond 20 000 MSK against minutes
of CPU) that inequality is slack by orders of magnitude, and it self-stabilizes: were
rubber-stamping common, fraud would start paying, `P(fraud)` would rise, and replaying would
become dominant again. What the fee actually buys is **capacity** — hardware-hours standing
ready — not per-replay willingness; collusive stamping silently lowers the *real* replay rate — which
is exactly what §6's Stage-0 `P_check` telemetry exists to expose — but never makes a false
root safe, because the refutation right is permissionless.

**4c. No-show is priced against griefing.** No-show is an objective offense (v0.1 §17) with a
slash floor of a large multiple of the forgone fee (placeholder: `≥ 100 · ρ_v · base`,
Stage-1-measured like every number here). The asymmetry is deliberate: a panel that strands a
job costs the miner one orphan-equivalent (re-mine), while costing the no-show pair two
slashes — targeted verifier griefing has negative return, and a panel that is merely *down*
loses fees and a bounded slash, not its base bond.

**4d. The challenger economy is rivalrous by construction.** The refutation bounty stays
capped at 10 % of slash (v0.1 §18.4) — deliberately not a living: the reliable challengers are
*competitors*. A rival miner who refutes removes competing credit AND collects the bounty, so
`P_check` rests on rivalry, not altruism; §5's fee-bonded audit calls are the paid probing
market for everyone else. (This is also why §2 tolerates a predictable panel: grinding a rival
*off* the panel removes their fee, never their refutation right.)

**4e. Admission is doubly capped, and both caps are registration-time checks:**

```
physical:   R_jobs · q ≤ Σ_v capacity_v(p99)
    a chain may not credit jobs faster than its class can replay them; at 120 s blocks
    and the 512-decode ceiling (p99 ≈ 10–20 min), every-block crediting with q = 2 needs
    ≈ 20 standing replay slots — a 4-host fleet credits sparser, or registers more capacity

economic:   P_check · S_eff ≥ λ · G_max        λ ≥ 2.0
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
  checkpoint interval's localization cost — plus the §4e caps. The §12 checklist gains:
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
