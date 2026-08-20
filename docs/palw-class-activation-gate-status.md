# PALW ClassActivationGate — status ledger and the Stage ladder's decision records

**Normative:** `docs/palw-full-logits-trace-v2-design.md` §12 (the twelve gate items) + §13
(the staged-rollout order) · ADR-0028 §6 additions (`P_check`, no-show telemetry) · ADR-0027
§6 / ADR-0026 the four-stage ladder · **Date opened:** 2026-08-16 ·
**Revised 2026-08-17** after the mainnet-readiness audit (`palw-mainnet-readiness-audit-2026-08-16-ja.md`):
a **Wired?** column added, rows 6/7/9/11/12 corrected, the through-line retracted, and §5 added.

This is the living record every Stage promotion (B16) references. No promotion is automatic:
each is an explicit entry in §3 below, and none may be signed while any gate item it depends
on is unmet. The gate list is quoted verbatim from §12; the status column is the load-bearing
part. **Nothing in this document activates anything** — it is the checklist a future
activation ADR must be able to point at.

> **Read the Wired? column first.** "met" in the Status column can mean *the mechanism exists and
> is unit-tested* without meaning *a live consensus path reads it*. The 2026-08-17 audit found the
> gap between those two is where the risk lives — 9 blockers, all in the consumer layer. The
> Wired? column names, per row, whether a live path actually consumes the mechanism.

## 1. The twelve gate items, with today's honest status

| # | Gate item (§12, verbatim) | Status | Wired? (live consensus path reads it) | Where |
| --- | --- | --- | --- | --- |
| 1 | at least 3 CPU microarchitectures tested | **partial (2)** | n/a — measurement | Broadwell + EPYC measured (`palw-stage0-fleet-replay-bench-2026-08-16.md`); cross-host root identical 4/4. A third μarch is unstarted — the honest gap. |
| 2 | at least 1,000 canonical prompts × 5 reruns/machine | **rerun half measured on aarch64; the 1,000-prompt corpus is still not started** | n/a — measurement | `palw-aarch64-class-determinism-2026-08-20.md`: all four registered golden jobs, 5 cold runs each, `roots_identical_across_runs = true` 4/4, and `v2-selftest` returns `status: pass` against the recorded `qwen35-2b-v2.cpu-aarch64.golden` set — so the build reproduces values recorded BEFORE it, not merely values it agrees with itself about. The remaining work is the CORPUS (1,000 canonical prompts), not the method. The 60-seed forgery audit (`palw-algo4-forgery-audit-2026-08-16.md`) still stands beside it. |
| 3 | full 64-byte equality; no prefix compare in the decision | **met** | **yes** — live `Hash64` compares | Every root compare in `palw_slash`/`palw_legs`/`palw_step_leg` is `Hash64` equality; adversarial attack 8 searches for and rejects any domain bridge. Audit confirmed: zero tolerant/prefix compares reach any slash path. |
| 4 | cold/warm/restart/concurrent/affinity/memory-pressure tests | **5 of 6 measured on aarch64; affinity is not measurable on this host** | n/a — measurement | `palw-aarch64-class-determinism-2026-08-20.md`: cold (process per run) MATCH ×5 per job, restart ×2 MATCH, concurrent ×4 (four cold workers at once) MATCH ×4, memory pressure (2 GB churned alongside) MATCH. **Affinity is NOT run and is not claimed:** macOS has no `sched_setaffinity`, and `thread_policy_set` affinity tags are a hint the OS ignores on Apple Silicon. That condition is meaningful on the Linux fleet and must be measured there. |
| 5 | ≥ 10,000 chain-derived seeds entropy/cost report | **not started (measurement); the PREMISE is now true on the free-prompt lane** | **yes on the FP lane** — `palw_fp_execution_seed_v3` is the only source | The report itself is still a fleet run. What changed 2026-08-20 is the premise the report would have been about. H7 was right: `PalwJobEnvelopeV2::execution_seed` is a free field carriage never inspects, so an entropy report on it would have measured a value its own producer chose. On the free-prompt lane the seed is no longer a field — `palw_fp_job_context_v3` DERIVES it from the job's chain anchor (network domain, class, anchor block, anchor DAA), and `job_nonce` is deliberately excluded because it is the one value a producer varies at will. `the_execution_seed_is_chain_bound_and_not_grindable` measures both halves: grinding the nonce (or the prompt, or the ceiling) does not move the seed, every chain fact does, and the binding reaches the execution root. **Corrected while closing it:** the free field is on `PalwJobEnvelopeV2` — the SUPERVISOR's job object for the replay/legs paths — not on the V2 attempt lane's own commitment. `PalwAttemptUnsignedV2` has no `execution_seed` at all; its chain binding is `challenge_v2(network_domain, pre_pow_hash, timestamp, nonce, class, bond)`, which the finalizer recomputes from the header's own position and refuses on mismatch (`PalwV2ChallengeMismatch`), so that lane was never the one H7 was about. **Still open:** the supervisor path's envelope, wherever a replay job's seed reaches `PalwJobContextV2::from_envelope`. |
| 6 | negative controls produce mismatch or a different class ID | **met at trace layer (unit); class-gate consensus-inert** | trace separation measured; the class-ID *gate* (ADR-0034 routing) is consensus-inert | Adversarial attacks 1-3, 5, 9-10 are these negative controls at the unit level. Cross-**class** separation IS measured: `gemm_trace_root` **0/61 matching** (x86 vs Metal, complete separation) — `palw-algo4-crosshost-determinism-2026-08-16.md:41`. **Correction (2026-08-17):** the earlier "cross-class divergence 47/61" citation was inverted — 47/61 is output-text *agreement*, 14/61 diverges; the trace layer (0/61), not the output text, is what binds the class. |
| 7 | exact artifacts and FP environment launch-verified | **met (mechanism)** — B8/B15 closed 2026-08-17 | **yes** — FP probes (8 sites, fail-closed), artifact pin always-recomputed, libm in the class id | FP env probes (MXCSR/FPCR RNE/FTZ=0/DAZ=0) real and fail-closed. **B15 closed:** the bypassable v1 gate (a `.palw-gguf-sha.json` in CWD, keyed on `path\|size\|mtime`, reached from `--mode verify` = the block-validation path) was folded into the always-recompute v2 gate — one policy, no cache. **B8 closed:** `PalwRuntimeManifestV2` v3 adds `libm_identity` + `libm_arithmetic_digest` (behavioural probe of the resolved `expf`/`logf`), so a libm change is now a different class id instead of a silent PoW-tag change. Consensus fingerprint unmoved; 663/663. Per-class launch verification remains a registration step. |
| 8 | minimum independent bonded credentials continuously available | **partial** | n/a — operational | A/B/C bonded 20k MSK, active (`t10-bond-registered-2026-08-15`); *continuity* unproven (a soak fact). **Independence** is the unstated gap: the bonded set is four VPSes under one administrator (audit low) — 1-of-N-honest with N = 1 trust domain. |
| 9 | measured replay capacity fits the challenge window at p99 | **partial** (window fits; ceiling not enforced) | **no** — the gate reads `credited_ceiling_tokens` only as `== 0`, never as a cap | Worst fleet κ·p99 = 272 s vs `w_replay` 1 h — ≥ 13× margin (measured, solid). But the credited ceiling is a 0/non-zero *switch*, not a cap: `exact_decode_tokens` is never compared to it (audit H4), `max_context_tokens` is self-referentially validated, and the registered p99 is not cross-checked against the ceiling (audit medium). |
| 10 | sustained zero-mismatch shadow / zero-credit soak | **not started** | n/a — measurement/soak | The Stage-0 drill binary + runbook are ready; the soak is a fleet run — but it cannot start *honestly* until the consumer-path blockers are fixed (a soak against a fail-open gate measures nothing), and the drill panel currently comes from an operator-written JSON roster, not chain state (audit medium). |
| 11 | adversarial test **and external review** completed | **partial (unit harness only)** | unit tests, **not a consensus path**; mint-path attacks absent | `palw_adversarial` (10 named attacks, permanent harness) covers the trace/step layer at the unit level — real and good. It does **not** cover the credit-gate consumer path (forged signature B1, duplicate commitment B2, off-class panel member H1, unadjudicated refutation B9). External review: not started. |
| 12 | emergency zero-credit rollback exercised | **mechanism MET on the V2 lineage, 2026-08-20; the EXERCISE is still a fleet run** | **yes on V2** — `PalwClassStatusV2::Frozen` is read by admission, the retarget and production; V1's per-block `class_frozen_before_close_v1` is separately live | The row's finding was right about the V1 lineage and is now superseded by the V2 one. `ClassFrozen` is a consensus object whose evidence is a `PalwClassContradictionCertificateV1` — two attestations on one job context disagreeing about what that job produced — so the freeze is OBJECTIVE (nobody decides it) and unforgeable against a class that is behaving. `check_class_contradiction_shape_v2` refuses evidence about another class (the liveness-floor attack: manufacture a contradiction in a disposable class, quote it at BASE-0), evidence whose attestations agree, and evidence binding a different job context; signatures are the acceptance layer's, the same split `BondRegistered` uses. **It is one-way by design:** the `ClassUnfrozen` variant is DELETED, because a chain-level unfreeze turns an objective permanent consequence into a temporary one — re-activation means a NEW class id with its own catalog entry and registration, which is the audit trail §2 asks for expressed as chain state. Measured by `a_class_freezes_only_on_a_contradiction_about_itself`. **What remains is §2's exercise**, on the fleet: induce a contradiction, confirm nothing is credited and no bond moved. |

### ADR-0028 §6 additions

| item | status |
| --- | --- |
| `P_check` measured in shadow | not started (Stage-0 drill run — the ledger computes it; needs live carriage) |
| no-show / inclusion telemetry published | not started (same run; `PalwShadowLedgerV1` produces the artifact) |

### The through-line (corrected 2026-08-17 — the prior claim was false)

**Retracted:** *"Every unmet item is a fleet measurement or soak, not a design gap."* The
2026-08-16/17 mainnet-readiness audit (ADR-0028 baseline) refuted it with **9 blockers, all in
the credit-gate consumer layer** — the code that reads carriage into the coinbase. The honest
through-line:

> The **format and arithmetic layers are done** and unit-tested (Layer 1 + carriage + schedule);
> the audit confirmed these are genuinely well-built. The **consumer layer that reads them into
> chain state is not** — it is fail-open in ~10 independent places: no PALW signature is verified
> anywhere in consensus (B1); no `committed_root` dedup (B2); credit is appended to the coinbase
> with no budget or per-block cap (B3); `min_credit_interval_daa` is enforced nowhere so the
> 11,655× leverage violation returns on activation (B4); the payee is resolved by a non-unique key
> over an unordered map, making the coinbase nondeterministic (B5); the gate's inputs are not a
> pure function of the block's own chain (B6); `algo_id=4` is the exclusive PoW with no hash floor
> and a `panic!` on runtime failure (B7); the class identity never pins `libm` (B8); and a
> well-formed refutation voids credit with no bond or adjudication (B9). The empiricism cannot
> start honestly until this wiring is fixed, because a soak against a fail-open gate measures
> nothing. See §5 and `palw-mainnet-readiness-audit-2026-08-16-ja.md`.

## 2. Emergency zero-credit rollback — the mechanism (gate item 12)

The rollback must (ADR-0027 §5, v0.1 §19): stop new credit from a class instantly, never
release a bond through the emergency exit (an attacker must not escape), never touch block
validity, fork choice, or any past block, and be reversible only by an explicit re-activation
from zero-credit.

> **⚠ Correction (2026-08-17 audit, H11): this mechanism is described but NOT built.**
> `class_active` / `class_frozen` do not exist in the codebase — the only occurrence is the
> `palw_credit.rs:60` doc-comment that *names* them. `ClassContradictionCertificateV1` exists as a
> struct in `palw_slash` (adjudicator + tests) but has **no carriage kind and no consumer**, so the
> objective-freeze path cannot reach the chain. The only working lever today is zeroing
> `credited_ceiling`, which is a `Params` edit hashed into the consensus fingerprint — a flag-day
> rebuild delivered over a possibly-halted chain, not a runtime off-switch. **A working emergency
> off-switch is a precondition of any fence-active network** (audit critical-path item 9): move the
> registration into pruning-surviving on-chain state, give the contradiction certificate a carriage
> kind and consumer, and wire a real `class_frozen` bit into both the gate and `select_replay_panel_v1`.
> The text below is the design intent, retained; it is not the current code.

**Mechanism** (design; NOT yet wired): a class's credit *should be* gated by a
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

**Status: BLOCKED, by design — and further from ready than this row claimed.** Requires
(ADR-0028 §6): `ExecutionStepRefutationV1` landed, the §4 registration inequality enforced,
`q ≥ 2`, AND — for any **bare-v2** class — chunked logits-evidence carriage landed **and drilled**
(ADR-0029 §6). Composite-v2 classes are not bare-v2-blocked.

> **⚠ Correction (2026-08-17 audit):**
> * `ExecutionStepRefutationV1` exists as a struct but is **unreachable end-to-end** — no Borsh
>   derive, no `PalwCarriedEvidenceV1` variant, no producer — and the kernel catalog resolves 6 of
>   17 op kinds, **excluding `MatMulQuant` / `MatMulF16` / `SoftMax` / RoPE**, so a lie in the ops
>   that carry the computation returns `Unadjudicable` (audit H10). Arithmetic conviction — the
>   literal Stage-2 prerequisite — is not yet a thing you can point at on chain.
> * `PalwScheduleParamsV1::validate` and `PalwClassRegistrationV1::validate` are implemented and
>   tested but have **no non-test caller** — installing `palw_credit = Some(..)` runs none of them
>   (audit H2). `q ≥ 2` is likewise enforced nowhere (only `q == 0` is rejected).
> * Chunked carriage (kind 0x06) landed but **cannot carry the bare-v2 logits refutation it exists
>   for** (its evidence enum has no logits-event variant), and every evidence it *can* carry is far
>   under the single-transaction cap (audit medium) — so the bare-v2 Stage-2 gate is not actually
>   closeable by it yet.

Additional hard gate: full §12 on a low-credit testnet (items 2, 5, 7, 9, 10, 11-external, 12 all
pending) **plus the 9 consumer-path blockers of §5**, which are not on this ladder and must be added.

### Stage 3 — Full (far)

**Status: FAR.** Requires the second independent reference implementation (**met at the v1 op
level** — `misaka-palw-reference2`, Berkeley SoftFloat; v2-op extension pending) and a Stage-2
soak with zero unexplained `ClassContradiction` freezes. Mainnet is a separate ADR + separate
activation + a demonstrated emergency rollback (§2's exercise), never folded into this ladder.

## 4. What changed today (2026-08-16) against this gate

Landed the mechanisms behind items 3, 6 (unit), 7, 9, 11-adversarial, 12-design, and the
Stage-2 bare-v2 carriage prerequisite. Re-derived the credited ceiling (item 9) from the
measured fleet numbers. *(This section's closing claim — "the arithmetic is done; the empiricism
is not" — was the through-line §1 now retracts. See §5.)*

## 5. The 2026-08-16/17 mainnet-readiness audit and the ADR-0041/0028 resolution

**Report:** `docs/palw-mainnet-readiness-audit-2026-08-16-ja.md` (ADR-0028 baseline, 15-agent
audit, refuted 0). **Verdict: NO-GO** — and, crucially, not merely because the staged ladder is
unfinished. The ladder *as written* does not schedule several of the defects found. **9 blockers,
all in the consumer layer** (the credit gate reading carriage into the coinbase); the format and
arithmetic layers are sound and were credited as such. The blockers, one line each:

1. **B1** — no PALW ML-DSA-87 signature is verified anywhere in consensus; the gate mints on unauthenticated objects.
2. **B2** — no `committed_root` dedup; one job credits once per carrying transaction.
3. **B3** — credit is appended to the coinbase with no budget and no per-block cap; pure additive issuance.
4. **B4** — `min_credit_interval_daa` (§4e's rate lever) is enforced nowhere; the 11,655× leverage violation returns on activation.
5. **B5** — payee resolved by a non-unique `validator_pubkey_hash` over an unordered `HashMap`; nondeterministic coinbase = permanent partition.
6. **B6** — the gate's inputs are not a pure function of the block's own chain (pruned acceptance reads as "nothing", the E2 spend gate reads virtual state).
7. **B7** — `algo_id=4` is the exclusive PoW with no hash floor, and header validation `panic!`s when the runtime is unavailable (contradicts the v2 design's principle 1/2). *Live on TN11/devnet.* → **DECIDED 2026-08-17, then REVERSED the same day.** ADR-0036 Decision 4 first made a permanent hash floor a hard gate on the mainnet identity; **ADR-0039 Decisions 1/2 (W6′) supersede that** — there is no hash floor on any network, mainnet included. Block production is PALW work everywhere, and the liveness floor is `PALW-BASE-0`: a portable integer-only class held permanently Active, whose catalog closes so it can be audited and convicted on any CPU. Total PALW unavailability halts the network **loudly, by design**, rather than degrading to hash ordering — the trade being that a hash lane which can always produce blocks is a permanent incentive to mine the lane instead of the work. Transient `PalwWorkerFailed` no longer panics (bounded retry); a persistent fault still does, and that is now the intended terminal behaviour rather than a gap awaiting a floor. **Remaining deliverable:** register `PALW-BASE-0` and hold it Active (its artifacts, its second implementation and its difficulty-domain share), NOT "implement the hash floor". The v2 design's principle 1/2 is superseded on this point.
8. **B8** — the class identity never pins `libm`, though ADR-0031 makes glibc `expf`/`logf` normative arithmetic inside the PoW tag. *Live divergence vector.* → **CLOSED 2026-08-17:** manifest v3 adds `libm_identity` + `libm_arithmetic_digest`. (B15's GGUF-pin bypass closed the same day — see row 7.)
9. **B9** — any well-formed refutation voids credit with no bond, no signature and no adjudication; a dust-tx griefing primitive.

Every ledger row above now carries a **Wired?** column so no future promotion mistakes a landed
struct for a live consensus path — the exact failure mode that let the old through-line be false.

### Mainnet activation model — ADR-0041 vs ADR-0028, resolved (ADR-0036, 2026-08-17)

The audit (H12) found two **Accepted, non-ancestral** ADRs describing mainnet PALW: **ADR-0028**
(this lineage, `palw_credit` staged gate) and **ADR-0041** (the `main-backup-8107bfb-20260807`
snapshot, `palw_spam` / `palw_algo4_accept` mechanism; merge-base `2dd863c`, 2026-07-16, not an
ancestor of `origin/main` or any live branch). **ADR-0036 settles it:**

* The live **`palw_credit` lineage governs**; the snapshot's ADRs 0039–0048 are historical and
  reserve no numbers here (this line is at ADR-0034; 0035+ are free).
* **ADR-0041's mechanism is superseded** (it does not exist on the live tree — porting it would
  *replace* ADR-0026/0027/0028/0033, not merge).
* **ADR-0041's two surviving conclusions are adopted** into the future parameterized mainnet ADR:
  (a) mainnet PALW needs a **new network identity** — the current `MAINNET_PARAMS` can never carry
  it (audit H13: both window presets fail `finality_depth < W_challenge` at 10 BPS), a conclusion
  ADR-0041 reached independently from the v4 anti-spam fence; and (b) the **land → accept → mint**
  separation, which maps onto this lineage's Stage 0 → 2 → 3 ladder.
* The **hash-floor question (B7) is a hard precondition** of any PALW-active mainnet and is flagged
  in ADR-0036 for that ADR to resolve (implement the floor, or amend the v2 design to delete the
  claim and register PALW as an unrecoverable liveness dependency — a decision, not a silent state).

ADR-0028's mainnet clause now carries a pointer to this resolution, so "after Stage 3" and
ADR-0041's "genesis-active" no longer read as a contradiction. The parameterized mainnet ADR
(identity, genesis, windows, `base(C)` fraction, bonds) still comes after the soak, and may not be
signed while any of the 9 blockers is open.
