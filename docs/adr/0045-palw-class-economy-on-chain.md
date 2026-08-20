# ADR-0045: The class economy is chain state — derived PWU, block-denominated epoch budgets, and the registration-granted share table

Status: **Accepted (implemented on the V2 lineage; consensus-inert — no shipped preset carries
`ConsensusV2`).**
Date: 2026-08-20
Relates to: ADR-0038 Decision D (per-class difficulty domains), ADR-0039 Decision 5 and its
2026-08-17 amendment (the four defects and the blocked enforcement point), ADR-0042 Decisions
1/5/6 (atomic bundle, candidate-scoped state, admission list), ADR-0043 (state-root ordering),
`consensus/core/src/palw_pwu.rs` (the derivation identity and `check_pwu_claim_v1`),
`consensus/core/src/palw_class_daa.rs` (the V1 budget derivation and its `StarvedClass`
refusal), `consensus/core/src/palw_state_v2.rs`, `consensus/core/src/palw_admission_v2.rs`,
`consensus/core/src/palw_mode_v2.rs`, threat model rows H3 and "Per-class DAA / lifecycle".

## Thesis

Three numbers decide whether many classes can share one chain without collapsing into a
monoculture or wedging: what one block's work is **worth** (pwu), how many blocks a class may
put into an epoch (the Decision 5 cap), and who holds what fraction of cadence (the share
table). Before this ADR the first was a self-declared value under a ceiling, the second was
frozen at ruleset-mint time in a currency that structurally starves heavy classes, and the
third was a params constant no chain event could move — so registering a second model was a
release, not a transaction. Each of the three is a fact about the chain, and this ADR moves
each to the altitude where the chain can check it:

```text
pwu       — derived, per candidate point:  claim == palw_pwu_v1(class_target, pwu_per_inference)
budget    — derived, per epoch boundary:   ⌊tol · E · s_c / (1000 · denom_c)⌋ blocks, frozen in state
shares    — granted, per registration:     conserved to 1000‰ by donation arithmetic, rooted
```

## Decision 1 — pwu has exactly one legal value (the ADR-0039 derivation item, V2 face)

`PalwPwuRuleV2` gains its promised second variant:

```rust
enum PalwPwuRuleV2 {
    MaxPerAttempt(u64),                    // pre-derivation scaffolding, unchanged
    DerivedV1 { pwu_per_inference: u64 },  // this decision
}
```

Under `DerivedV1`, admission item 6 is **equality, not a bound**: the attempt's `pwu` must equal
`palw_pwu_v1(class_target, pwu_per_inference)` where `class_target` is the class's target **at
the candidate chain point** — the exact identity `palw_pwu.rs` states and the V1 lineage already
enforces via `check_pwu_claim_v1`. The ADR-0039 amendment's objection to an admission-time rule
— "the only altitude at which a header is validated has no legal source for a class's DAA
target" — is void on the V2 lineage: the class target is rooted, candidate-scoped chain state
(PR-03), which is the altitude admission already reads bonds and classes from. Neither factor is
a miner input, so any other value is a mistake or a weight-inflation attempt, and both are
`PwuClaimNotDerived`.

`pwu_per_inference` must be ≥ 1 at registration (`ZeroPwuPerInference`), and is normatively the
**counted step-leaf count of the class's canonical inference** — the same number the court's
bisection ladder must walk, which is why `PalwCourtParamsV2::max_step_leaf_count` is its
network-wide ceiling. The genesis loader that verifies `class_catalog_root` preimages must
verify the declared count against the catalog's counted one — the same landing H2's still-open
note already assigns `max_step_leaf_count` to. One currency for weight and adjudication: work
that cannot be walked cannot be worth anything.

`MaxPerAttempt` survives for fixtures and pre-derivation nets; ADR-0042's "the derivation is
required before any class carries weight" is now dischargeable — a value network registers only
`DerivedV1` classes, and the register says so.

## Decision 2 — the epoch budget's currency is the block

The 2026-08-17 amendment elected pwu as the cap's currency (defect (a)) and then derived, in
defect (e), that the election cannot work across epochs: with `W_e = L · Σ s_k · pwu_k`, the
share cancels out of its own inequality and what remains is `tol · mean(pwu) ≥ pwu_c` — a
comparison of pwu magnitudes across classes, which is the cross-class price this fork rejects
everywhere else, and which caps every above-mean class below its own cadence. The starvation is
the **currency's** fault, not the tolerance's. This decision supersedes the election:

**The budget is denominated in blocks.**

```text
budget_c(e) = ⌊ tol‰ · E · s_c / (1000 · denom_c) ⌋      blocks, for epoch e
```

* **Nothing the pwu currency enforced is lost.** Targets move only at epoch boundaries (PR-09)
  and Decision 1 makes per-block pwu a constant of (class, epoch) — so within any epoch the two
  caps are the same cap, entry for entry. They differ only across epochs, which is exactly where
  the pwu currency manufactures defect (e).
* **Every defect of the amendment stays closed.** (a) a block count is even more immutable than
  pwu; (b) the budget is frozen at the boundary, in rooted state (below); (c) it is charged to
  the producing block's own class counter along its own selected chain — the accumulator the
  V2 state already carries — and never to a merger; (d) dissolves outright: no pwu magnitude
  crosses a class boundary anywhere in the cap; (e) `budget_c ≥ expected_c ⟺ tol ≥ 1000‰`,
  which is the tolerance floor — **there is no configuration in which a class is capped below
  its own cadence**, so `StarvedClass` has nothing left to refuse in V2.
* **The cap still bounds what it exists to bound.** The threat is a transiently mis-tuned DAA
  flooding the DAG — a block-count, bandwidth and verification-load quantity. The weight side
  needs no cap once pwu is derived: a too-easy target mints many blocks that each weigh
  proportionally less, so a class's Σpwu per epoch tracks the compute it actually spent
  regardless of tuning; inflating per-block pwu requires tightening one's own target and paying
  the inferences (bounded per boundary by `max_factor`).

**Boundary freezing.** Crossing into epoch `e` derives every eligible class's budget from the
boundary's own facts and writes them into rooted state (`PalwEpochBudgetsV2 { epoch_index,
budget_blocks }`). Admission reads the snapshot; for the crossing block itself (whose apply has
not happened yet) it derives from the parent state — the same pure function over the same
inputs, so the stored snapshot and the crossing block's own admission cannot disagree. The
epoch's budgets are constant for the epoch, whatever registrations land mid-epoch: a class
registered mid-epoch enters the table at the next boundary.

**The denominator is the H1 census.** `denom_c = Σ shares over the competing set`: the closed
epoch's producers among unfrozen share-bearing classes; a non-producer that re-enters is
measured against the set plus itself; an empty census (fresh chain, gap epochs) competes
everyone unfrozen. This is the retarget's own absence rule — audit H1 — applied to the second
consumer of the same census: a class whose permille sits idle must not strangle the classes
that are actually producing, and a cap that ignored absence would reintroduce H1's unbounded
walk as a hard refusal instead of a slow one.

**Grant floor.** A share is grantable only if `tol · E · s ≥ 10⁶` — the smallest share whose
worst-case budget (`denom = 1000‰`) is still ≥ 1 block. Derived budgets are therefore never
zero, mid-flight, by construction; the V1 derivation's `ZeroBudget` refusal becomes a
registration-time property instead of an epoch-time cliff. The tolerance is fenced
`[1000‰, PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE]` — the V1 ceiling, reused.

The V1 pwu-denominated derivation (`class_epoch_budgets_v1`) stands as the V1 lineage's record
of the amendment; the V2 lineage does not call it.

## Decision 3 — the share table is chain state, granted at registration, conserved by arithmetic

`class_shares: BTreeMap<class_id, u16>` joins `PalwChainStateV2`'s primary data, rooted and
carriage-moved like everything else the root commits to. The params table dissolves:
`PalwClassDaaV2Params` is deleted, and `PalwStateParamsV2` carries what actually is a network
constant — `base_class_id`, `class_daa_max_factor`, `budget_tolerance_permille`. The bundle
names `base_class_id` too, and the startup gate requires the two to agree (the C5 pattern: the
value the id commits to must be the value the machine enforces).

* **The first registration funds the floor.** The first `ClassRegistered` on a chain must be
  the base class, at exactly 1000‰ — the liveness floor exists before anything else does, and
  ADR-0039 W6′'s "may never be absent, may never be zero" becomes a property of the only
  sequence of objects the machine accepts.
* **Every later registration pays for its seat.** `ClassRegistered` carries `share_permille`;
  the entrant's share is donated proportionally by every incumbent — largest-remainder,
  class-id order, the V1 freeze arithmetic reused as arithmetic — so `Σ = 1000‰` holds at every
  mutation by construction, not by a boot-time assertion. Refused: an entrant below the grant
  floor, or a donation that would push any incumbent (the base class included) below it.
* **Freeze and unfreeze move no share.** A frozen class's permille stays in the table and out
  of the census; both consumers (retarget expectation, budget denominator) renormalize over who
  actually competes. This supersedes the register's "redistribution on freeze" expectation
  deliberately: a freeze that moved permille would hand every unfreeze a burst and make a
  temporary status a table mutation, and the census already answers absence at the only two
  places absence is measured.
* **No self-declaration.** Decision 5's "enters at a share set by finalized class-health
  transition rather than by self-declaration" is honored structurally: the share rides the same
  authorized registration object the class itself does — whoever may register a class may fund
  it, and nobody else may move a permille at all.

**Startup gate movement.** Deleted from the bundle gate: table-sums-to-1000, base-share-nonzero,
and both share↔budget coherence loops — their subjects no longer exist as params. What replaces
them is stronger: conservation and the floor hold at **every transition**, exercised by the
differential suite, instead of once at boot. `PALW_STATE_V2_VERSION` bumps 1 → 2: the root
preimage gains `class_shares` and `epoch_budgets` in declared-field order (ADR-0043's rule for
a consensus change to the root: a new version, never a silent re-reading).

## What this ADR does not decide

* **Automatic share re-allocation from class health.** An idle class's permille sits; the
  census routes around it. Decaying it on-chain is a future object with its own authorization
  story.
* **Permanent class removal** and where its share returns.
* **The counting tool** that produces `pwu_per_inference` for accelerated classes; BASE-0's
  count lands with the catalog loader, beside `max_step_leaf_count`, which already owes the
  same check.
* **Reward's relation to shares.** Reward stays claim-scoped (Decision 10); nothing here touches
  the escrow.

## Consequences

* Defect (e) stops being a tuning problem: no share table, tolerance, or pwu spread can cap a
  class below its own cadence, and the proof is one line (`tol ≥ 1000‰`).
* Registering a model becomes a chain event: a `ClassRegistered` object with a funded share,
  admissible mid-flight, producing from the next epoch boundary — not a params release.
* The ruleset id changes shape (fields moved and removed). Permitted: nothing shipped carries
  `ConsensusV2`, and Decision 11's promise is about RC == mainnet, both of which mint after
  this ADR.
* New named tests land with the implementation: the derivation-equality attack
  (`u64::MAX` claim refused, off-by-one refused, drift across a retarget boundary re-derived),
  budget frozen under mid-epoch registration, the (B+1)-th block refused while a sibling class
  still admits, donation conservation under permutation, and the H1-census denominator for a
  sole producer.
