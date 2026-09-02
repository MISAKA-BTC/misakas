# ADR-0069 — End-to-end adjudicability is the price of weight

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
* **BASE-0 already carries the end-to-end drill.** `misaka-palw-base0/src/backend.rs`:
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
**undeclared requant** that lifts the embedding onto the A16 stream — `misaka-palw-base0/src/legs.rs:113-119`).
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
* **Decision 3 — the drill, and its two verbs.** `misaka-palw-base0/src/e2e_drill.rs`:
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
contributes `pwu = 0` to both `safe` and `live`, whatever its ramp stage. The block is otherwise
unchanged: it advances DAA, it is paid its budgeted subsidy (ADR-0039's "admissible for liveness"),
its claim runs the same lattice. Weight is what certification buys — Decision 5's sentence, applied
to the quantity it was written about. This is a fork-choice rule and moves the ruleset id: it ships
in the next ruleset move and is a mainnet precondition.

**Where the rule lives — this text said `chain_weights_v1`'s inputs, which is right as a rule and
wrong as a location.** `chain_weights_v1` is fed by `palw_facts::resolve_block_weight_v1`, and on a
`ConsensusV2` network the heap that reads it is a SEARCH ORDER, not the authority — the processor's
own `palw_tip_weights_v1` says so. The authority is the V2 **state fold**: `PalwChainStateV2` keeps
`safe_weight` and `bounded_immature` as running totals, hands both to `PalwCandidateOrderV1::new`,
and hashes both into `palw_state_root`. So Decision 7 is priced in three places, all through the one
helper `palw_class_bears_weight_v2`: the V1 resolver (the search order), the claim's
`immature_contribution` at creation (the `live` half), and `safe_weight` at the claim's `Final` (the
half the IBD and deep-reorg gates read). The predicate implemented is the class's **granted share**,
which admission makes a sound proxy for certification in the one direction that matters —
`verify_class_admission_v3` refuses `share > 0` to a family that is not certified end to end, so
`share > 0 ⇒ certified`.

Two consequences are residuals rather than choices, and are pinned by tests rather than left to be
discovered. **(i)** The safe half is priced at the finalizing block, so a claim accepted while its
class was weightless and finalized after the class took cadence is paid in full; freezing the
decision on the claim record is a claim-encoding change, hence a state-version bump and a re-mint.
**(ii)** Because of (i), `retired_safe_weight` must NOT re-ask the question at the retirement block —
it accumulates `claim.pwu`, the most the retired claims could have carried — and the internal
consistency identity `safe_weight == retired + Σ Final` therefore becomes an upper bound while the
fence is armed. Asking twice instead makes the two totals drift apart permanently, which shows up
as every pruning-point import being refused for a state that obeyed the rule exactly.

Two companions, so Decision 7 is not the only wall: **(a)** a `BondCapabilityDeclared` set is
bounded and priced (ADR-0071's security amendment); **(b)** ADR-0076 §8's field is pinned to the
base target by the processor today — recorded there — and seating (ADR-0076 Decision 4) re-prices it.

Invariants: **E1** a block of a share-0 or uncertified class contributes 0 to both chain weights at
every ramp stage; **E2** the same block's subsidy and DAA advance are unchanged; **E3** a class that
becomes certified mid-chain gains weight only for blocks after the certifying object's chain
point — never retroactively (a retroactive gain would let a certification reorder history).
