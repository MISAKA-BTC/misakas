# ADR-0056: Permissionless class admission, and the share economy that survives it

**Status:** Accepted and **implemented** (2026-08-27). Decisions 1, 2 and 7 were already the
shipped design; Decisions 3 and 5 landed with the state version 8 → 9 this ADR predicted, and
Decision 6 is a policy the gate keeps by not deduplicating. **Decision 4 is withdrawn** in favour
of ADR-0054 Decision 1, which answers the same question with a proportional step instead of a
streak — see that decision for the measurement that settled it.

**Renumbered** from 0054 at the merge: ADR-0054 and ADR-0055 were taken by two branches whose
documents were written earlier the same day.
**Date:** 2026-08-27
**Depends on:** ADR-0038 (PALW is the consensus work), ADR-0039 (the floor; no weight without a
complete catalog), ADR-0045 (the class economy: one share table, conservation, donation),
ADR-0049 Decision H (the post-genesis admission carriage), ADR-0053 (one execution family).

---

> **Amended (index reconciliation, 2026-09-02).** Three statements below are stale. (1) The
> constants table's `min_base_class_share_permille = 300‰` is [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md)'s
> 20‰ since Relaunch 5. (2) Item 7's "the mid-epoch budget defect is also closed" is contradicted by
> [ADR-0053](0053-palw-one-execution-family.md) (the fix was reverted on `main`) and by the shipped
> transition: a class activating mid-epoch has no attempt-lane budget until the next boundary
> (`ensure_epoch_budgets`), a known gap kept for state-root compatibility. (3) Decision 5 still says
> "share is earned per Decision 4"; Decision 4 is withdrawn and the share walk is
> [ADR-0054](0054-palw-share-follows-production.md) Decision 1. Registration itself is gated further
> by [ADR-0069](0069-e2e-adjudicability-is-the-price-of-weight.md) (an uncertified family registers
> weightless) and [ADR-0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) (a bond
> is earned). Map: [`README.md`](README.md).

## Context

testnet-11 registers three classes at genesis. The question this ADR answers is the next one:
**who may add the fourth, by what rule, and what stops a thousand of them?**

Two failure shapes bound the design space:

* **A permissioned registry** ("the foundation approves models") reintroduces the authority PALW
  exists to remove. The chain's whole verification story is that a claim is checked by
  *recomputation*, not by *reputation*; a model list curated by identity would be the one
  component whose correctness is somebody's word.
* **A free registry** is an attack surface. Every registration takes one permille of cadence from
  every incumbent (ADR-0045 Decision 3's donation), and share once granted currently has **no
  path back**: no decay, no reclamation, no growth either. A flooder with one bond could register
  hundreds of dead classes, dilute every working class toward the 1‰ grant floor, and the table
  would stay that way forever. Nothing about the flood is even dishonest — each registration
  passes every technical check.

The resolution is the repository's standing pattern: **make the gate arithmetic and the economics
self-limiting**, so that "who may register?" has the same answer as "who may produce a block?" —
anyone, at a price denominated in the same collateral as everything else, under rules every
validator recomputes independently.

### What already holds (measured, not planned)

The technical half of permissionless admission is **already the shipped design**:

1. **The gate is deterministic and total.** `verify_class_admission_v2`
   (`consensus/core/src/palw_class_admission_v2.rs`) checks, from the carried material alone:
   the profile validates; `class_id == shape_profile_id` (identity is *derived*, never chosen);
   every reachable kernel is in the court catalog (A4); the logits commitment scheme is one this
   build adjudicates; the worst-case step space fits the ladder; the three court-cost ceilings
   hold under `derive_court_cost_v1` — the same function the genesis mint uses (one derivation,
   two doors); the canonical job fits the registered context in the enumeration's own form; and
   the declared `pwu_per_inference` equals the **counted** step-leaf count, so a registration
   cannot multiply its own weight by self-report.
2. **The object carries its own evidence.** ADR-0049 Decision H: a post-genesis `ClassRegistered`
   rides with `PalwClassAdmissionCarriageV2` — the full shape profile and the canonical job —
   because a running chain has no catalog to look the entrant up in. Genesis registrations are
   checked by `verify_palw_genesis_v2` against the catalog the ruleset id commits to instead.
3. **The registrant is a bond, and signs.** The acceptance layer
   (`virtual_processor/processor.rs`, the `ClassRegistered` arm) requires an **Active** bond and
   an ML-DSA-87 signature over `palw_class_registration_message_v2(network, class_id, share,
   activation_daa, bond)`. Not a permission system: the smallest answer to "who", denominated in
   the collateral every other authority on this chain is denominated in.
4. **The entrant's share is pinned.** A post-genesis entrant joins at exactly
   `min_grantable_share_permille()` (1‰) — a registrant that could name its own permille would be
   donating itself a slice of every incumbent's cadence.
5. **Donation conserves and floors.** `granted_share_table_v2`: incumbents are scaled by largest
   remainder (total order on ties, so two nodes cannot disagree about a residue permille),
   Σ = 1000‰ is a construction, and a donation that would push any incumbent below the 1‰ grant
   floor is refused (`DonationBreaksGrantFloor`).
6. **Activation is a soak, and a clock.** `activation_daa` registers the class *weightless*
   (`PalwClassStatusV2::Registered`): in the registry, in the catalog, fully adjudicable, holding
   no share, its attempts refused — until the named DAA score, when the transition grants the
   pending share. The flip is a clock, not an object: nobody submits it, so there is nothing to
   forge.
7. **Idle share does not strangle producers.** `derive_epoch_budgets_v2` uses the *census*
   denominator (ADR-0045 Decision 2): a class's budget is its share over the **competing** set of
   the closed epoch, so a table full of idle entrants slows nobody who is actually producing.
   (This closed defect H1; the mid-epoch re-derivation defect is also closed — budgets re-derive
   whenever the share table grows under them.)
8. **Freezing is evidence, not opinion.** A class leaves by `ClassFrozen` only with a structural
   contradiction certificate, and the floor may never be frozen (ADR-0039 W6′).

What does **not** exist today — and what this ADR decides — is the economics around that gate:
registration has no cost beyond a transaction fee, share has no path up, no path down, and no
reclamation. Decisions 3–6 add exactly those, and nothing else.

---

## Decision 1 — The constitution: admission is arithmetic, and only arithmetic

**Implemented.** A class joins this network if and only if a `ClassRegistered` object passes the
deterministic gate above. There is no allowlist, no vote, no identity check, and none may be
added to this path. "The MISAKA foundation approved it" is not a condition; "every validator
independently recomputed the same admission facts" is the only one.

Two corollaries, stated so they cannot be un-stated quietly:

* **No BFT vote on models.** Validators do not vote on a registration; they *validate* it, the
  way they validate any transaction. Agreement is a consequence of determinism, not of a quorum.
* **Governance, if it ever exists, is a separate layer with a smaller job.** A community that
  wants to *refuse* a technically admissible model may only do so the way it changes any other
  consensus rule: a ruleset change, visible in the fingerprint, adopted by upgrade. This ADR
  reserves no hook for it inside the gate, deliberately — a hook is a permission system with
  extra steps.

## Decision 2 — The kernel boundary: what needs a binary, and what does not

**Implemented.** A model whose graph reaches only catalogued kernels, commits a supported logits
scheme, and fits the ladder and the cost ceilings **registers without any software change** —
weights, profile, carriage, signature, done. A model that needs a new kernel, a new commitment
scheme, or a deeper ladder **cannot** be admitted by any transaction: the gate refuses it by
name, because a court that cannot re-execute a step must never accept work committed under it.

The upgrade path for that case is the one this repository has already walked three times (the
Qwen3.6 descriptors, the A16 rope/mul-elem pair, the tile floor): grow the court catalog in the
validator binary, which **moves the ruleset id**, which is a coordinated network upgrade visible
at the handshake — and *then* the model registers through the unchanged gate. Deployment and
consensus are different events; conflating them ("everyone updated, so the model is adopted") is
refused by construction, because the registration object still has to ride a block and pass the
gate afterwards.

## Decision 3 — Registration exposure: entry is priced in bonded collateral

**Implemented.** The transition's `ClassRegistered` arm reserves; `ClassFrozen` and reclamation
release; `verify_admission_v2`'s ceiling reads the sum of both ledgers.

A registration reserves **`REGISTRATION_EXPOSURE_SOMPI`** against the registrant's bond for as
long as the class it registered is `Registered` or `Active`, released when the class is
reclaimed (Decision 5) or frozen. The reservation rides the same exposure accounting that prices
a claim's bind window, and the same ceiling (`max_exposure_ratio_permille`) bounds it — so a bond
can hold at most `⌊collateral · ratio / REGISTRATION_EXPOSURE⌋` live registrations *minus* what
its claims are using, and flooding the registry competes with producing blocks for the same
capital.

The constant is a bundle parameter (inside the ruleset id), sized like the claim exposure is
sized: high enough that a thousand dead classes cost a thousand bonds' worth of idle collateral,
low enough that one honest model costs one operator nothing they don't already hold. It is a
**reservation, not a burn**: honest registrants get it back at retirement, so the price of entry
is the *time value* of collateral, and the price of spam is that time value multiplied by a
number the spammer chose.

*Why not a burn?* A burn punishes the honest entrant exactly as much as the flooder and creates a
money sink whose size is a policy knob. A reservation punishes only *holding many dead classes
simultaneously*, which is precisely the attack, and it composes with Decision 5: reclamation
frees the spammer's collateral only by also freeing the share they squatted.

## Decision 4 — Withdrawn: the share walk is ADR-0054's, not this one's

**Withdrawn 2026-08-27 at the merge, in favour of ADR-0054 Decision 1.** Two branches wrote a
share-movement rule the same afternoon. They agree on everything that matters — the signal is a
class's own FILLED BUDGET and nothing else, because jobs are self-originated on this network and
every other "demand" signal is manufacturable by the class's own operators at no cost; the step is
bounded per epoch; the floor is the reservoir and conservation is a construction rather than an
assertion. This one is not kept, and the reason is a measurement rather than a preference.

This decision's step was a STREAK: *n* consecutive epochs at or above a fill threshold buys one
permille. ADR-0054's is proportional: a class that filled its budget takes
`max(1‰, share × g / 1000)` at every qualifying boundary. On a minimum-share class the streak rule
has no reachable input at all — the grant floor is defined so that `expected = 1` for every
entrant on every network, and ADR-0045 Decision 2's budget then caps the same class at one block
per epoch, so its only two states are "produced 0" (the retarget skips it, audit H1) and
"produced 1" (`expected == observed`, an exact no-op). Measured on a two-class chain carrying the
real `PALW-QWEN36` class over four epochs in both states: the target never moved. A rule keyed to
those states is a rule with no input on exactly the class that most needs to grow.

The proportional step is also the faster one where speed is the point: an entrant reaches a few
percent of cadence in a fortnight instead of months, which is the difference between a public
network with two model tiers and a public network with two demonstrations attached.

**What this ADR still owns of the economy:** Decision 3 (a live registration is EXPOSURE against
its registrant's bond, so registering a thousand classes costs a thousand reservations) and
Decision 5 below (a class that produces nothing for `reclaim_epochs` gives the seat back). Those
two are what make admission permissionless without making it free, and ADR-0054 explicitly leaves
both undecided. The floor's protected permille, which all three lines arrived at independently, is
one field — `min_base_class_share_permille`, the audit's name, because its version landed first
and closes a measured critical — and every path that can move a permille checks it.

## Decision 5 — Reclamation: dead classes give the network back

**Implemented** as `apply_class_reclamation`, in transition slot 2d — at the same boundary and off
the same closed epoch's census as ADR-0054's growth step, and immediately after it, because a class
the growth rule just decayed and a class that has been silent for `reclaim_epochs` are different
facts and only the second takes a class out of the table. The status flip is written exactly as
`activate_due_classes` writes activation — a clock, not an object.

A class that produced **zero** blocks for `RECLAIM_EPOCHS` consecutive epochs transitions
`Active → Dormant`:

* its share returns to the incumbents by the inverse of the registration donation (largest
  remainder, same total order);
* its registrant's `REGISTRATION_EXPOSURE` is released (Decision 3);
* it **stays in the registry and the catalog** — Dormant is not Frozen: its past claims remain
  adjudicable, disputes against them still run, and its artifact root still names it;
* admission refuses its attempts, exactly as it refuses a `Registered`-not-yet-active class.

A Dormant class returns by a fresh `ClassRegistered` for the same `class_id` — the one case where
`DuplicateClass` does not refuse — carrying a fresh signature, a fresh exposure reservation, a
fresh activation soak, and entering again at the 1‰ floor. Nothing about its history transfers:
share is earned per Decision 4, from the bottom, both times.

Together with Decision 3 this closes the flood: a spammer's collateral stays locked exactly as
long as the squatted permille stays out of the table, and both revert on the same clock. The
attack's steady state is *paying rent on nothing*.

## Decision 6 — Duplicates are priced, not policed

**Implemented by absence, and now decided.** The same
`artifact_root` under a different profile is a **different class** — legitimately: a geometry
upgrade (a wider context after a court-format improvement, a different tile budget) is exactly a
new profile over the same weights, and this repository has already done it twice. The gate does
not deduplicate by weights, and must not: it cannot distinguish an upgrade from a copy, and
trying would make the artifact root a lock instead of a name.

What bounds copy-spam is Decisions 3–5: every copy reserves its own exposure, enters at 1‰,
must *produce* to keep it, and is reclaimed when it does not. A hundred copies of a working model
are a hundred rents; the table converges on the copies that someone actually runs — which is the
only definition of "the real one" a permissionless network can afford.

## Decision 7 — What the chain does not judge

**Implemented, by absence — kept deliberately.** The gate checks *adjudicability*, never
*quality*. A model that passes every check and generates noise is admissible; a model that would
win benchmarks but reaches one uncatalogued kernel is not. Text quality is the operators' market
(they choose what to run and what to pay for); the chain's only promise is that whatever runs
can be re-executed, disputed, and slashed. Confusing the two would put a benchmark inside
consensus, and a benchmark is an oracle.

---

## The attack table

| Attack | What stops it | Decision |
|---|---|---|
| Register hundreds of dead classes to dilute incumbents | Each reserves bonded exposure while alive; zero production reclaims share and only then frees the collateral — steady state is rent on nothing | 3 + 5 |
| Name your own share at entry | Acceptance pins entrants to the 1‰ grant floor | (shipped) |
| Grow share by fake demand | The only growth signal is filled budget, which costs full production price to fake; one bounded step per epoch | ADR-0054 D1 |
| Squat share without producing | Decay to the floor, then reclamation to Dormant, by clock | ADR-0054 D1 + 5 |
| Register someone else's class id | `class_id == shape_profile_id`, derived not declared | (shipped) |
| Overstate `pwu_per_inference` to inflate weight | Declared value must equal the counted step-leaf count | (shipped) |
| Register a class the court cannot try | Coverage, scheme, ladder and cost gates refuse by name | (shipped) |
| Sneak a new kernel in by transaction | Impossible by construction: the catalog is in the binary, growing it moves the ruleset id | 2 |
| Freeze a competitor | `ClassFrozen` needs a structural contradiction certificate; the floor is unfreezable | (shipped) |
| Dilute the floor below liveness | `min_base_class_share_permille` bounds every donation path | 3 + ADR-0054 D2 |
| Flood-register to slow incumbents' *budgets* | Census denominator: idle share is excluded from competing budgets | (shipped, H1) |
| Re-register a reclaimed class to reset its slate | Allowed — and it re-enters at 1‰ with fresh exposure and a fresh soak; there is no slate to reset | 5 |

## Consequences

* Adding a fourth model to testnet-11 is, today, a `ClassRegistered` transaction from any active
  bond — the gate is live end to end (`the_a16_dense_class_passes_the_admission_gate` and the
  acceptance arm are its tests). What ships with Decisions 3–5 is the guarantee that the
  *thousandth* registration is as harmless as the fourth.
* **The constants this network chose, and why each is derived rather than picked:**

  | Parameter | RC value | Where it comes from |
  |---|---|---|
  | `REGISTRATION_EXPOSURE_SOMPI` | 40,000 | A minimum bond's ceiling is `400,000 × 500‰ = 200,000`, so a smallest-possible bond holds **five** live registrations and no claims. A hundred dead classes needs twenty minimum bonds, idle. |
  | `RECLAIM_EPOCHS` | 12 | Reclamation takes the WHOLE share, so it is deliberately far slower than ADR-0054's step. One block in twelve epochs is not what this rule is about. |
  | `min_base_class_share_permille` | 300‰ | Leaves 700‰ for a busy registry while keeping the class every node can run at nearly a third of the cadence. One field, checked by growth, decay and grants alike — see Decision 4. |

* Two bundle parameters and state version 8 → 9 entered the ruleset id together; landing them was
  a re-mint, like every ruleset move on this line. The shipped testnet-11 value is re-derived over
  the whole merged ruleset rather than carried from any one branch, so no number from this ADR's
  drafting survives as a pin.
* **What the implementation added that the ADR text did not anticipate:** a class with NO budget
  for the closed epoch is not measured at all — it could not have filled a ceiling nobody gave it
  (the mid-epoch activation case), and counting that as a decay would punish a class for the
  boundary's own arithmetic.
* The share table becomes a slow, legible instrument: one bounded step per class per epoch
  boundary, every move derivable by every observer from on-chain state alone.
* Nothing in this ADR adds an authority. The list of things that can move a permille after it
  lands: a signed registration (in), the epoch clock (up, down, back). Every one is validated by
  recomputation.
