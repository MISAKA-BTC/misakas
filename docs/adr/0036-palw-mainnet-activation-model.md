# ADR-0036: PALW mainnet activation — lineage reconciliation and the model that governs

Status: **Proposed (governance decision).** Activates nothing, changes no code, moves no fence.
This ADR settles a *documentation* conflict the 2026-08-16/17 mainnet-readiness audit surfaced:
two Accepted, non-ancestral ADRs each describe "mainnet PALW", with different mechanisms, a
colliding "PALW / algo-4" name, and overlapping ADR-number spaces. It decides which lineage
governs and what carries forward. It deliberately does **not** decide mainnet parameters — those
come from the Stage-0→3 soak (see "What this ADR does not decide").

> **Numbered 0036, not 0035.** This was first drafted as ADR-0035 and renumbered the same day: a
> parallel session landed `0035-palw-public-testnet-strategy.md` (testnet-11 as the public PALW
> *testnet*) at commit `e4848d2` while this was being written. That collision — two same-day
> ADR-0035s on one lineage — is a live instance of exactly the number-hygiene problem this ADR
> settles for the mainnet/backup fork. The two are otherwise disjoint: ADR-0035 decides the public
> *testnet*, this ADR decides the *mainnet* activation model.

Date: 2026-08-17
Supersedes: **ADR-0041 as to mechanism** (and, by extension, the `palw_spam` / `palw_algo4_accept`
/ `palw_compute_work_scale` activation vocabulary of the `main-backup-8107bfb-20260807` snapshot,
ADRs 0039–0048). It **adopts** ADR-0041's two surviving conclusions (below).
Relates to: ADR-0026/0027/0028 (the four-stage credit ladder this lineage implements),
ADR-0033 (`palw_credit` gate), ADR-0034 (routing),
ADR-0035 (`0035-palw-public-testnet-strategy.md` — the public PALW *testnet* decision; distinct
from this *mainnet* one, and the reason this ADR is 0036),
`docs/palw-mainnet-readiness-audit-2026-08-16-ja.md` (the audit and its 9 blockers),
`docs/palw-class-activation-gate-status.md` (the §12 gate ledger, corrected the same day).

> **Landed later than written (2026-08-17).** This ADR was drafted in a worktree and never
> committed, so the lineage that then produced ADR-0037 and ADR-0038 branched without it — while
> ADR-0038's header states "Everything else in ADR-0036 … stands" and rests normative weight on
> decisions whose text did not exist in any branch. That dangling reference was itself a finding
> of the 2026-08-17 re-audit (blocker 12). It is committed here unchanged in substance, with one
> section added below reconciling it against the pivot that overtook it.

## Relationship to ADR-0037 and ADR-0038 (added 2026-08-17)

ADR-0038 re-asserts PALW as the primary consensus work and introduces a hash **anti-stall floor**
on the value network. Read carelessly, that looks like it overturns Decision 4 below, which
decided the hash floor binds the *mainnet identity* while TN11/devnet stay deliberately
single-algo. It does not — the two are about different questions, and both still hold:

* **Decision 4 answers "must mainnet be able to survive a PALW runtime failure?"** — yes, and that
  requirement is a hard gate on the new identity. ADR-0038's anti-stall floor is one *shape* of
  that survival, arrived at independently. The requirement and the mechanism agree.
* **Decision 4's other half — TN11/devnet stay single-algo — is unaffected** by ADR-0038, which
  legislates for the value network. A soak net that halts loudly is still the right failure mode
  there, and ADR-0038 does not claim otherwise.
* **What ADR-0038 *does* change is the meaning of Decision 2's "land → accept → mint" separation.**
  Under the credit-gate model, *land* meant "the lane exists with credit off". Under ADR-0038 a
  landed PALW block already carries consensus weight, so the separation is now
  *admit → license (receipts) → mature*. The principle — presence of the lane is not licence to
  mint — survives; the stages it names are ADR-0038 Decision B's, not the old ladder's.
* **Decision 2's new-network-identity requirement is strengthened, not weakened.** ADR-0038 makes
  PALW the block-production path, so the audit's finding that the current `MAINNET_PARAMS`
  identity cannot carry PALW (both window presets fail `finality_depth < W_challenge` at 10 BPS)
  becomes structural rather than parametric.

Nothing here reopens Decision 1 (the live `palw_credit` lineage governs) or Decision 2's
supersession of ADR-0041's mechanism. Where ADR-0038 and this ADR genuinely conflict in future,
ADR-0038 governs on *what the consensus work is*, and this ADR governs on *which lineage and
which network identity mainnet ships as* — they are orthogonal axes and should stay so.

## Context — two lineages, one name

The audit (H12) found two Accepted ADRs describing mainnet PALW that do not reference each other
and sit on branches with no ancestry relation:

| | **Live lineage (governs)** | **Historical snapshot** |
| --- | --- | --- |
| branch | `misakas` → `origin/main` + `palw-llm-pow-*` (canonical tip `palw-llm-pow-unified`) | `main-backup-8107bfb-20260807` |
| mainnet ADR | **ADR-0028** — mainnet is a separate ADR after Stage 3 | **ADR-0041** — mainnet ships PALW active from a new v4 genesis |
| ADR numbers | 0021, 0024–0034 | 0039–0048 |
| mechanism | `palw_credit` fence + `PalwCreditParamsV1` staged credit gate | `palw_spam` / `palw_algo4_accept` / `palw_compute_work_scale` + qwen-8.0 `mint.rs` 12-gate |
| mainnet params today | `palw_credit: None`, `pow_palw_activation: never()` | (its own params, not on the live tree) |

Established by measurement, not assertion:

* **The two lineages are non-ancestral.** `git merge-base --is-ancestor main-backup-8107bfb-20260807 <origin/main | unified | bps01>` → **NOT-ANCESTOR** for all three. Their merge-base is `2dd863c` (2026-07-16); the snapshot diverged and was not carried forward.
* **ADR-0041's mechanism does not exist on the live tree.** `palw_algo4_accept` / `palw_spam` appear nowhere in the live `consensus/core/src/config/params.rs`; the live tree uses `palw_credit` (the ADR-0028/0033 gate). Porting ADR-0041's mechanism would mean *replacing* the lineage's credit design, not merging.
* **ADR-0041 is narrower than its commit message.** It decides only the *land* shape of mainnet — a new v4 genesis, genesis-active lane (`palw_activation_daa_score = 0`), non-inert `palw_spam` — and explicitly keeps `palw_algo4_accept = false`, `palw_compute_work_scale = 0`, and mint `eligible=false / weight=0` behind 12 external gates. Its "genesis-active" is a *land* decision, not a *credit* decision, so it is **not** in contradiction with ADR-0028's "credit after Stage 3" once the two layers are separated.

The genuine conflicts are therefore: (a) a name/number collision between two codebases, (b) two
different mechanisms for the same idea, and (c) two parallel, mutually-unaware decisions about
the mainnet identity. Left unresolved, no one can write a coherent release plan, because
"the mainnet ADR" is ambiguous.

## Decision

1. **The live `palw_credit` lineage governs.** The four-stage credit ladder of
   ADR-0026/0027/0028 and the `PalwCreditParamsV1` gate of ADR-0033 are the mainnet PALW design.
   The `main-backup-8107bfb-20260807` snapshot and its ADRs 0039–0048 are **historical**: a
   parallel design that was not taken forward. They reserve no numbers on the live lineage
   (the live line is free to use 0035, 0036, … and is not bound by the snapshot's 0039–0048).

2. **ADR-0041 is superseded as to mechanism, and two of its conclusions are adopted.**
   * **Adopted — mainnet PALW requires a new network identity.** ADR-0041 reaches this from the
     Header-v4 anti-spam fence (public/value requires `genesis.version == 4` + genesis-active
     PALW, which a fence retrofit on the existing identity cannot satisfy) and from the measured
     one-time cost of retrofitting pruning depth onto a running chain. The audit reaches the
     **same conclusion independently** (H13): the current `MAINNET_PARAMS` identity cannot carry
     PALW, because at 10 BPS `finality_depth = 432_000` and **both** shipped window presets
     (`W_challenge` 8_640 and 720) fail `PalwScheduleParamsV1::validate`'s
     `finality_depth < W_challenge` rule, and a 100 ms block interval is physically incompatible
     with a 37–91 s replay. Two independent design threads converging is strong signal; this is
     adopted as a hard constraint on the future mainnet-parameter ADR.
   * **Adopted — the land → accept → mint separation.** ADR-0041's three stages map onto this
     lineage's ladder: *land* = Stage 0 (a genesis-active lane may exist with credit OFF);
     *accept* = the objective-slash stages; *mint* = Stage 2+ credit, gated by §12 and a separate
     activation decision. Genesis-active *presence of the lane* does not imply genesis-active
     *credit*.
   * **Not adopted — the mechanism.** `palw_spam` / `palw_algo4_accept` / `palw_compute_work_scale`
     / qwen-8.0 `mint.rs` are not this lineage's mechanism and are not ported. Any specific
     0039–0048 item the project still wants (e.g. the v4 anti-spam accumulator shape) is ported
     deliberately, item by item, as a new live-lineage ADR — never by adopting the snapshot
     wholesale.

3. **Mainnet activation is gated behind the full ladder, the §12 gate, and the audit's blockers.**
   ADR-0028's "separate ADR, separate activation, after Stage 3" stands. That future ADR may not
   be signed while any §12 gate item it depends on is unmet **or** while any of the 9
   mainnet-readiness blockers is open. The blockers are not on the current ladder and must be
   added to it — the ledger's through-line ("every unmet item is a fleet measurement, not a
   design gap") was false and is retracted.

4. **The hash floor (audit B7) — DECIDED 2026-08-17: the principle stands, and it binds the
   mainnet identity, not the testnets.** The v2 design's principle 1 ("永久 hash floor を残す") and
   principle 2 ("PALW 障害時は credit = 0 とし liveness を継続") are **retained, not retracted.**
   They are, today, contradicted by the code: `required_algo_id` returns one mandatory id,
   `check_algo_id` rejects every other, `pow_layer0.rs` states there is no mixed-`algo_id`
   difficulty arithmetic, and `calc_block_level_check_pow_layer0` `panic!`s on
   `PalwUnavailable | PalwWorkerFailed`. The resolution splits by network, because the two have
   genuinely different requirements:

   * **TN11 / devnet keep single-algo PALW, deliberately.** These are soak networks whose entire
     purpose is to run PALW as the real PoW and observe it. For them, **a loud halt is the correct
     failure mode** — strictly better than a silent fork, which is the failure a hash floor would
     be trading it for. Retrofitting a mixed-algo difficulty relation onto a running chain would
     itself require a re-genesis or fork, and would add a large new consensus surface to the very
     system under audit. No hash floor is added here, and this is now a recorded choice rather
     than an unexamined state.
   * **Mainnet MUST ship the permanent hash floor.** It is a hard gate on the new network identity
     Decision 2 already requires: `Valid block = valid permanent hash PoW AND (PALW certificate
     absent OR valid under its activation stage)` (v2 design §2.2, verbatim). A single inference
     runtime failure must degrade mainnet to `credit = 0` with hash ordering and liveness intact —
     never to a halt. The floor is therefore **designed as part of that identity**, where the
     difficulty relation between the two work functions can be specified from genesis instead of
     grafted on, and it may not be deferred past it.
   * **Failure-mode hardening lands now, on both.** `PalwUnavailable` (a missing worker/model — a
     permanent configuration fault) keeps failing loud, and the ADR-0035 boot calibration already
     turns it into a *startup* refusal on class-pinned nets, which is where it belongs.
     `PalwWorkerFailed` must **not** panic a node on a transient fault (a spawn failure or timeout
     under load is not a configuration error), and is given bounded retry before it is treated as
     permanent — see Consequences.

   The result: no safety principle is given up, mainnet cannot launch without the floor, and the
   testnets are not destabilized to buy a property they do not need.

5. **Namespace.** Because the snapshot is non-ancestral and historical, the live lineage owns the
   name "PALW / algo-4"; no rename is required on the live tree. This ADR is the record that the
   snapshot's use of the name and the 0039–0048 numbers is superseded, so future readers do not
   mistake `git show main-backup-…:docs/adr/0041-*.md` for a live decision.

## What this ADR does not decide

* **Mainnet parameters.** The new network identity's name, genesis, suffix/ports/seeds, the
  window preset (a 10-BPS-or-slower set that passes `validate`), the credited-job ceiling, `base(C)`
  as a fraction of subsidy, `q`, bonds, and `ρ_v` — all come from the Stage-0→3 soak and are the
  subject of a *later* ADR (the parameterized mainnet-activation ADR ADR-0028 promises). That ADR
  cannot be honestly drafted before the soak, because its parameters are the soak's outputs.
* **Whether to port any specific 0039–0048 item.** Adopted here are only ADR-0041's two
  conclusions in §Decision.2. Anything else from the snapshot is a separate, deliberate port.
* **The 9 blockers themselves.** They are fixed in code, not decided here; this ADR only binds the
  release decision to their closure.

## Consequences

* **ADR-0028's mainnet clause is now self-consistent.** It carries a pointer to this resolution
  (edited the same day), so its "after Stage 3" and ADR-0041's "genesis-active" no longer read as
  a contradiction.
* **The §12 gate ledger gains a "Wired?" column and corrected rows** (`palw-class-activation-gate-status.md`,
  same-day revision), so no future promotion mistakes a landed struct for a live consensus path —
  the specific failure mode that let the through-line be false.
* **Future ADR numbering on the live lineage is unblocked:** this is ADR-0036; 0037+ are free; the
  snapshot's 0039–0048 do not reserve anything here. (0035 is the public-testnet-strategy ADR that
  triggered this renumber — the concrete cost of the un-settled number space this ADR closes.)
* **The mainnet-parameter ADR has its constraints pre-recorded:** new identity (Decision 2),
  post-soak parameters (does-not-decide), hash-floor resolution (Decision 4), all 9 blockers closed
  and on the ladder (Decision 3). It is a fill-in-the-measured-values exercise on top of this
  governance frame, not a fresh design.
* **Landed with this ADR (2026-08-17), the two "live today" items of the audit's critical path:**
  * **libm is now part of the class identity (B8).** `PalwRuntimeManifestV2` gained
    `libm_identity` (diagnostic) and `libm_arithmetic_digest` (load-bearing) — a behavioural
    fingerprint of the resolved `expf`/`logf` over the frozen `PALW_LIBM_PROBE_V1` vector,
    measured through the same dynamic symbols llama.cpp resolves. Manifest version → **v3**: a v2
    manifest could not distinguish two libms, so its class claim was under-specified and must not
    compare equal to a v3 one. Behavioural rather than a build id on purpose — a build id moves on
    rebuilds that do not change arithmetic, while this moves iff the arithmetic moves. It does not
    replace ADR-0031's disassembly audit, which remains what licenses `libm_transcribed`.
    *Verified:* the consensus fingerprint does **not** move (the manifest is not a `Params` field);
    `kaspa-consensus-core` 663/663.
  * **The GGUF pin is no longer bypassable (B15).** The v1 model gate consulted a
    `.palw-gguf-sha.json` in the process CWD keyed on `path|size|mtime` and returned the cached
    digest on a match — and its one caller was `--mode verify`, *the mode block validation
    invokes*, putting the bypass on the consensus PoW path. v1 was folded into the always-recompute
    v2 gate (VPS design §4.4) rather than patched, so one policy remains and no second
    implementation can drift. Cost: one 1.2 GB hash per job process, amortized by the persistent
    agent. Operators should delete any stale `.palw-gguf-sha.json`; it is now inert.
  * **Transient worker faults no longer panic a node.** `run_worker_with_retry` gives
    `PalwWorkerFailed` bounded attempts with linear backoff before the caller's panic; a missing
    worker (`PalwUnavailable`) still fails immediately, since retry cannot fix configuration.
* **What is NOT closed by this ADR:** the hash floor itself is designed and implemented with the
  mainnet identity, not here; TN11/devnet remain single-algo, and a persistent runtime failure
  still halts them by design. That is now a recorded trade, and it is on the ladder rather than
  hidden in a doc-comment.
* **If the project instead wants the snapshot's design to govern,** that is a reversible choice —
  but it means adopting the `palw_spam` mechanism onto the live tree and superseding ADR-0026/0027/
  0028/0033, which is a far larger change than porting ADR-0041's two conclusions. This ADR records
  the smaller, evidence-backed default; reversing it is a deliberate act with its own ADR.
