# ADR-0054: Permissionless class admission, and the share economy that survives it

**Status:** Accepted — Decisions 1, 2 and 7 are implemented today (citations inline); Decisions
3–6 are normative and not yet implemented, each with its enforcement point named.
**Date:** 2026-08-27
**Depends on:** ADR-0038 (PALW is the consensus work), ADR-0039 (the floor; no weight without a
complete catalog), ADR-0045 (the class economy: one share table, conservation, donation),
ADR-0049 Decision H (the post-genesis admission carriage), ADR-0053 (one execution family).

---

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

**Normative; not yet implemented.** Enforcement point: the acceptance arm above plus the
transition's `ClassRegistered` arm; accounting reuses the bond `reserved` field
(`PalwBondRecordV2.reserved`, `reserved_exposure`).

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

## Decision 4 — Share moves at epoch boundaries, by production, bounded

**Normative; not yet implemented.** Enforcement point: the epoch-boundary arm of
`apply_palw_transition_v2`, beside `derive_epoch_budgets_v2`, reading the same per-epoch
produced-blocks census that already exists.

The share table currently has no arithmetic to change after registration. This decision gives it
exactly one, and makes it boring:

* **Signal.** For each class, the closed epoch's `produced / budget` ratio — both numbers the
  state already carries. Nothing else: not fees, not demand claims, not votes. Jobs are
  self-originated on this network (there is no orderer), so any "demand" signal is trivially
  manufactured by the class's own operators; *filled budget* is the one signal that costs real
  inference, real bonded claims, and real carve escrow per block to fake, and whose fake is
  indistinguishable from the honest thing because it **is** the honest thing.
* **Step.** A class that filled ≥ `SHARE_RAISE_FILL_PERMILLE` of its budget for
  `SHARE_RAISE_EPOCHS` consecutive epochs gains **+1‰**. A class that filled
  < `SHARE_DECAY_FILL_PERMILLE` for `SHARE_DECAY_EPOCHS` consecutive epochs loses **−1‰**, never
  below the 1‰ grant floor. One permille per boundary per class, in either direction — the walk
  is bounded by the clock, so no epoch's outcome moves the table faster than everyone can see.
* **Funding.** Raises are funded by the decayers of the same boundary first, then pro rata from
  all incumbents by the same largest-remainder arithmetic as registration donation
  (`granted_share_table_v2`'s scaling, reused, not re-implemented). Conservation stays a
  construction: Σ = 1000‰ before and after every boundary.
* **The floor is protected.** `PALW-BASE-0`'s share never drops below
  `FLOOR_PROTECTED_PERMILLE` (a bundle parameter) by *any* donation — registration or
  epoch-boundary. ADR-0039 made the floor unfreezable; this makes it un-diluteable below the
  level where "every node can always produce" stays true in cadence and not only in principle.
  The floor does not *gain* by the production rule either: it is the fallback, not a competitor.

*Why this is sybil-neutral.* Share follows realized, collateralized production. An operator who
fills a class's budget with their own jobs at their own expense is not gaming the metric — they
are *being* the class's production, paying full price for it (inference cost + claim exposure +
worker carve escrow), at a growth rate capped at 1‰ per `SHARE_RAISE_EPOCHS`. The same money
spent on honest demand buys the same permille. There is nothing cheaper behind the metric,
which is the definition of a metric that cannot be gamed, only paid.

## Decision 5 — Reclamation: dead classes give the network back

**Normative; not yet implemented.** Enforcement point: the same epoch-boundary arm; the
transition writes the status flip exactly as `activate_due_classes` writes activation — a clock,
not an object.

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

**Normative (the policy is implemented implicitly today; this makes it a decision).** The same
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
| Grow share by fake demand | The only growth signal is filled budget, which costs full production price to fake; +1‰ per window, capped | 4 |
| Squat share without producing | Decay to the floor, then reclamation to Dormant, by clock | 4 + 5 |
| Register someone else's class id | `class_id == shape_profile_id`, derived not declared | (shipped) |
| Overstate `pwu_per_inference` to inflate weight | Declared value must equal the counted step-leaf count | (shipped) |
| Register a class the court cannot try | Coverage, scheme, ladder and cost gates refuse by name | (shipped) |
| Sneak a new kernel in by transaction | Impossible by construction: the catalog is in the binary, growing it moves the ruleset id | 2 |
| Freeze a competitor | `ClassFrozen` needs a structural contradiction certificate; the floor is unfreezable | (shipped) |
| Dilute the floor below liveness | `FLOOR_PROTECTED_PERMILLE` bounds every donation path | 4 |
| Flood-register to slow incumbents' *budgets* | Census denominator: idle share is excluded from competing budgets | (shipped, H1) |
| Re-register a reclaimed class to reset its slate | Allowed — and it re-enters at 1‰ with fresh exposure and a fresh soak; there is no slate to reset | 5 |

## Consequences

* Adding a fourth model to testnet-11 is, today, a `ClassRegistered` transaction from any active
  bond — the gate is live end to end (`the_a16_dense_class_passes_the_admission_gate` and the
  acceptance arm are its tests). What ships with Decisions 3–5 is the guarantee that the
  *thousandth* registration is as harmless as the fourth.
* Four new bundle parameters enter the ruleset id when Decisions 3–5 land:
  `REGISTRATION_EXPOSURE_SOMPI`, the four share-walk constants, `RECLAIM_EPOCHS`,
  `FLOOR_PROTECTED_PERMILLE`. Landing them is a re-mint, like every ruleset move on this line.
* The share table becomes a slow, legible instrument: at most ±1‰ per class per epoch boundary,
  every move derivable by every observer from on-chain state alone.
* Nothing in this ADR adds an authority. The list of things that can move a permille after it
  lands: a signed registration (in), the epoch clock (up, down, back). Every one is validated by
  recomputation.
