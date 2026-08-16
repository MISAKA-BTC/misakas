# PALW ClassActivationGate — status ledger and the Stage ladder's decision records

**Normative:** `docs/palw-full-logits-trace-v2-design.md` §12 (the twelve gate items) + §13
(the staged-rollout order) · ADR-0028 §6 additions (`P_check`, no-show telemetry) · ADR-0027
§6 / ADR-0026 the four-stage ladder · **Date opened:** 2026-08-16

This is the living record every Stage promotion (B16) references. No promotion is automatic:
each is an explicit entry in §3 below, and none may be signed while any gate item it depends
on is unmet. The gate list is quoted verbatim from §12; the status column is the load-bearing
part. **Nothing in this document activates anything** — it is the checklist a future
activation ADR must be able to point at.

## 1. The twelve gate items, with today's honest status

| # | Gate item (§12, verbatim) | Status | Where |
| --- | --- | --- | --- |
| 1 | at least 3 CPU microarchitectures tested | **partial (2)** | Broadwell + EPYC measured (`palw-stage0-fleet-replay-bench-2026-08-16.md`); cross-host root identical 4/4. A third μarch is unstarted — the honest gap. |
| 2 | at least 1,000 canonical prompts × 5 reruns/machine | **not started** | 60-seed forgery audit exists (`palw-algo4-forgery-audit-2026-08-16.md`); the 1000×5 corpus run is a fleet job, not yet run. |
| 3 | full 64-byte equality; no prefix compare in the decision | **met** | Every root compare in `palw_slash`/`palw_legs`/`palw_step_leg` is `Hash64` equality; adversarial attack 8 searches for and rejects any domain bridge. |
| 4 | cold/warm/restart/concurrent/affinity/memory-pressure tests | **partial** | Cold-process-per-run measured; `roots_identical_across_runs` true everywhere. The full matrix (affinity, memory-pressure) is unstarted. |
| 5 | ≥ 10,000 chain-derived seeds entropy/cost report | **not started** | The seed-binding (chain-bound `execution_seed`, grinding-closed) exists; the entropy/cost *report* is a fleet analysis run. |
| 6 | negative controls produce mismatch or a different class ID | **met (unit) / partial (fleet)** | Adversarial suite attacks 1-3, 5, 9-10 are exactly these negative controls at the unit level; cross-class-answer divergence measured at 47/61 seeds (`palw-algo4-forgery-audit-60-seeds`). |
| 7 | exact artifacts and FP environment launch-verified | **met (mechanism)** | GGUF sha256 + size pinned; worker probes MXCSR/FPCR (RNE/FTZ=0/DAZ=0) load-time; manifest build.rs pins CMakeCache + static-lib shas. Per-class launch verification is a registration step. |
| 8 | minimum independent bonded credentials continuously available | **partial** | A/B/C bonded 20k MSK, active (`t10-bond-registered-2026-08-15`); *continuity* is unproven (a soak fact). |
| 9 | measured replay capacity fits the challenge window at p99 | **met** | Worst fleet κ·p99 = 272 s vs `w_replay` 1 h — ≥ 13× margin. Credited ceiling re-derived from the measurement (`credited_ceiling_tokens_v1`, this session): the pinned Q4 class is format-bound, not window-bound. |
| 10 | sustained zero-mismatch shadow / zero-credit soak | **not started** | The Stage-0 drill binary + runbook are ready (`misaka-palw-shadow`, `palw-stage0-shadow-drill-runbook.md`); the soak is a fleet run. |
| 11 | adversarial test **and external review** completed | **partial** | Adversarial test: **met** — `palw_adversarial` (10 named attacks, permanent harness). External review: not started (needs the second reference impl finished + a review package). |
| 12 | emergency zero-credit rollback exercised | **design met, exercise pending** | Mechanism in §2 below; the exercise is a fleet drill. |

### ADR-0028 §6 additions

| item | status |
| --- | --- |
| `P_check` measured in shadow | not started (Stage-0 drill run — the ledger computes it; needs live carriage) |
| no-show / inclusion telemetry published | not started (same run; `PalwShadowLedgerV1` produces the artifact) |

### The through-line

Every **mechanism** the gate needs now exists and is tested at the unit level (Layer 1 + the
carriage/schedule pieces). Every **unmet** item is a *fleet measurement or soak*, not a design
gap — which is exactly the state the gate is meant to expose: the code cannot mint anything,
and the remaining work is empirical, on hardware, over time.

## 2. Emergency zero-credit rollback — the mechanism (gate item 12)

The rollback must (ADR-0027 §5, v0.1 §19): stop new credit from a class instantly, never
release a bond through the emergency exit (an attacker must not escape), never touch block
validity, fork choice, or any past block, and be reversible only by an explicit re-activation
from zero-credit.

**Mechanism** (design; wiring is Stage-2, B14): a class's credit is gated by a
`class_active ∧ ¬class_frozen` predicate read by the credit walk. Two independent off-switches:

* **Objective freeze** — the `ClassContradictionCertificateV1` trigger (two signed,
  non-matching roots for one job under one class = the class's own membership claim refuted).
  This is already an objective, carriage-borne fact; it sets `class_frozen` with no governance
  step. Bonds freeze, they do not release.
* **Registry zero-credit** — a class's registered `credited_ceiling` set to 0 (or `class_active`
  cleared) via the same registration path that set it, which is a coordinated-release action,
  not a runtime vote. `credit(C) = 0` for every job of a zero-ceiling class by the §1 gate's
  own arithmetic — no special-case code, the ceiling *is* the switch.

Neither path can release a frozen bond, touch a settled block, or reverse itself implicitly.
Re-activation re-runs the full §12 gate from zero-credit (§3's ladder), which is the audit
trail the "exercised" checkbox will point at.

**The exercise** (pending, fleet): on the Stage-0 drill network, (a) induce a
`ClassContradiction` on a dedicated mismatch namespace and confirm the shadow ledger flags the
freeze and credits nothing; (b) set a drill class's ceiling to 0 and confirm every subsequent
job scores `credit = 0` in the ledger; (c) confirm no bond moved in either case. The drill
binary's induced-negative flags (`--noshow-nth`, mismatch namespace) are the substrate.

## 3. Stage ladder — promotion decision records (B16)

The ladder is ADR-0026 §/ADR-0027 §6's four stages. Each promotion is an explicit record here;
promotion N requires every gate item its stage depends on to be **met** in §1.

### Stage 0 — Shadow / zero-credit (current)

**Status: ACTIVE (code); fleet run pending.** Carriage is consensus-inert (native subnetwork,
telemetry only). No credit, no offense evidence against third parties, no block-validity
effect. Entry criteria: none (this is the floor). What it produces: the §1 measurements that
turn "not started" rows into data.

*Gate dependency:* none. *Blocker to leaving:* items 1, 2, 4, 5, 10 (all fleet runs) plus the
Stage-1 carriage landing (B7/B8, in progress).

### Stage 1 — Objective slash (design landed, not promoted)

**Status: NOT PROMOTED.** Requires: dedicated subnetwork carriage (B7/B8) + the stateless
validators (landed format, wiring in progress) + gate items 3, 6, 7, 9 met (they are, at the
levels noted) + a Stage-0 soak with zero unexplained freezes (item 10, pending). Slashes only
objective/structural faults; the arithmetic `ExecutionStepRefutationV1` may convict but does
not yet gate credit. **This promotion cannot be signed until item 10's soak exists.**

### Stage 2 — Bounded (blocked)

**Status: BLOCKED, by design.** Requires (ADR-0028 §6): `ExecutionStepRefutationV1` landed
(**met** — `palw_step_refute`, catalog-scoped), the §4 registration inequality enforced
(`PalwScheduleParamsV1::validate`, met), `q ≥ 2`, AND — for any **bare-v2** class — the
chunked logits-evidence carriage landed **and drilled** (ADR-0029 §6). The carriage **landed**
this session (`palw_carriage` kind 0x06); the **drill** is a Stage-0 fleet run. Composite-v2
classes are not bare-v2-blocked. Additional hard gate: full §12 satisfied on a low-credit
testnet (items 2, 5, 10, 11-external, 12-exercise all pending).

### Stage 3 — Full (far)

**Status: FAR.** Requires the second independent reference implementation (**met at the v1 op
level** — `misaka-palw-reference2`, Berkeley SoftFloat; v2-op extension pending) and a Stage-2
soak with zero unexplained `ClassContradiction` freezes. Mainnet is a separate ADR + separate
activation + a demonstrated emergency rollback (§2's exercise), never folded into this ladder.

## 4. What changed today (2026-08-16) against this gate

Landed the mechanisms behind items 3, 6 (unit), 7, 9, 11-adversarial, 12-design, and the
Stage-2 bare-v2 carriage prerequisite. Re-derived the credited ceiling (item 9) from the
measured fleet numbers. The gate's shape is now clear: **the arithmetic is done; the
empiricism is not.**
