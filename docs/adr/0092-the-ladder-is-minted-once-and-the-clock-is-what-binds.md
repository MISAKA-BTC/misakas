# ADR-0092 — The ladder is minted once, and the wall clock is what binds

* Status: ACCEPTED 2026-09-06 (design first, at the operator's word; §8 records the implementation)
* Builds on: [0049](0049-palw-adjudication-contract.md) Decision C (what one round may cost),
  [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) (adjudicability is the price of
  weight), [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) Decision 12
  (the `2^32` context ladder), [0080](0080-the-answer-is-long-the-verified-unit-is-short.md) (the
  verified unit is short), [0082](0082-the-close-is-flat-in-the-context.md) Decisions 1, 3, 6 and 9
  (the k-ary court, the close ceiling, the seat's window),
  [0084](0084-the-ids-ride-the-capture-stays-home.md) U-08 (the walkers take the ruleset's ladder).
* Amends: nothing yet — §4 proposes amendments to 0082 Decision 3 only if Decision 3 of this ADR
  is taken.
* Supersedes nothing.

## 0. The sentence this ADR is

**The ladder is a one-time consensus commitment; the Court window, not model size, is the binding
resource constraint.**

A model's width costs the court rounds, not leaves, and rounds are already logarithmic — so what
stops a bigger model is not the tree. It is that every round costs wall clock inside a window fixed
at mint, and `max_step_leaf_count` is inside the ruleset id and therefore **cannot be raised on a
running chain**. This ADR decides what a mainnet freezes at mint so that 7B and 1T are the same
protocol, and names where a future width problem is paid for.

## 1. What the operator asked, in the operator's words

> 大きい dense LLM も DA Court で扱えるようにするなら、「Court の上限を単純に引き上げる」のではなく、
> 巨大な推論を階層化して DA するのが一番きれいです。
> …
> モデルが大きくなっても、1回の dispute で Court が調べる量は一定 にする。
> …
> 「モデルサイズが大きくなるほど DA が破綻する」という現在の構造を、「モデルサイズが大きくなっても
> DA の対象が階層的に増えるだけ」に変えられます。

The goal is right and this ADR adopts it. What follows first is the part the operator could not
see from the audit report: **most of that structure is already shipped**, and saying so precisely is
what makes the remaining decision small enough to take.

## 2. What already exists, checked against the tree rather than against the ADRs

Read at `4fcce4b0`. Every claim here is a file and a line, because an ADR that restates a decision
already taken is the most expensive kind of document this project can write.

| the operator's step | where it already is |
|---|---|
| hierarchical chunk/subchunk tree | `palw_context_ladder.rs` (4,018 lines), `palw_bisect.rs` (1,993) |
| dissect down to one operation | `palw_attn_court_v1.rs` (2,882); the moves `CourtAttnRootClaimed` / `CourtAttnDissected` are consensus objects (`palw_state_v2.rs:3003, 3032`) |
| split the answer in time | ADR-0080's interval lane; `fp_interval.rs`, ADR-0077 Decision 8 |
| separate model from inference | ADR-0067, ADR-0078: `class_id` / `artifact_root` / `shape_profile_id` are bound separately from a job's roots |
| **court work ≈ constant in model size** | **already true**: `rounds = ceil(log2(max_step_leaf_count)) + terminal` (`palw_mode_v2.rs:270`) |
| **refuse at registration a class the court cannot try** | **already enforced**: `verify_class_admission_v5` demands *all three* of the close ceiling, the ladder depth and the court window, and names which one refused (`CourtCostExceedsCeiling` / `DeeperThanTheLadder` / `CourtWindowTooShort`) |

So the proposal's (1), (2), (3), (5) and (6) are not new work. They are the shipped design, arrived
at independently, which is a good sign about the design and a bad sign about the documentation:
nothing in `docs/adr/` states the invariant in one place, which is why it could be proposed as
missing.

Two things in the audit are genuinely open, and neither is a shape problem:

* **The responder does not exist.** The court can try a fused-attention dissection; no shipped
  binary constructs one. `palw_state_v2.rs:7805-7812` records that on a fused site an `Arithmetic`
  whole-row close is refused by `check_close_cost_v2` and an `AttnDissection` close is refused
  `NoDissection` — so every terminal leaf of a fused class is undefendable. That is audit finding
  C-2, it is a feature of the size of the close prover, and it is **not** what this ADR decides.
* **The walkers do not take the ruleset's ladder.** `PALW_STEP_MAX_LEAVES = 1 << 22` is the
  executor's constant; the RC ruleset's `max_step_leaf_count` is `1 << 26` and ADR-0077 D12's
  ladder is `1 << 32`. Audit H-4 / ADR-0084 U-08. It is an arming (`Params::palw_court_ladder`),
  not a design change.

## 3. The wall, and where it actually is

`PalwCourtParamsV2::worst_case_duration_with_history_daa` (`palw_mode_v2.rs:602-609`):

```
moves    = (bisection_rounds + history_dissection_rounds) * 2 + terminal_rounds + root_claim
duration = moves * turn_deadline_daa
```

Three consequences, in order of how much they bind:

1. **Leaves are cheap.** `bisection_rounds` is `ceil(log_arity(max_step_leaf_count))`, so at
   arity 2 **one doubling of the provisioned space costs one round** — that is the rule, and it is
   the only statement in this section that is not a generated number. The tree's own sizing table
   and `provisioning_the_whole_step_space_costs_four_rounds` already pin the RC's step from its
   floor to the whole step space in those terms. A model an order of magnitude wider than today's
   therefore costs rounds in the tens, not in the millions, and §5's generator is what says how
   many. Nothing about the leaf count is a wall.
2. **Rounds are not cheap, because each one is a `turn_deadline_daa` of wall clock, doubled for
   the two parties.** The duration must fit `window_court`. The RC already moved
   `court_turn_deadline` 60 → 42 for exactly this reason, and at ADR-0077 D12's `2^32` the worst
   case sits a few tens of DAA below the window. **`2^32` is not a leaf-count choice. It is the
   wall-clock ceiling at the RC's turn deadline and court window.**
3. **`max_step_leaf_count` is inside the ruleset id.** `palw_ruleset_id_v2` hashes the bundle, so
   raising the ladder on a running chain is a flag day, and
   `palw_class_admission_v2.rs:56-62` states the consequence in its own words: *"Unlike every other
   obstacle to adding a class later, this one cannot be repaired later: by the time the second
   class exists, the number is already inside the network's identity."*

That is the decision this ADR exists to take. Not "how do we make the court hierarchical" — it is —
but **what number a mainnet freezes at mint, given that it can never raise it, and what it does
when a model arrives that does not fit.**

## 4. Decisions

**Decision 1 — a mainnet mints its ladder at the top of the wall-clock budget, not at the width of
the models it has.** The card chooses `max_step_leaf_count` as the largest power of two whose
`worst_case_duration_with_history_daa` still fits `window_court` with a stated margin, and records
the margin. Rationale: rounds are logarithmic, so provisioning the whole reachable space costs a
handful of rounds; and the number cannot be raised later, so under-provisioning is the only
irreversible mistake available at mint.

**Decision 2 — the class-admission gate keeps all three refusals, and the ADR names them as one
invariant.** A class is adjudicable on this ruleset when, and only when, it satisfies the close
ceiling, the ladder depth and the court window simultaneously. This is already the code
(`verify_class_admission_v5`); Decision 2 makes it a decision so a future amendment cannot relax
one of the three without amending an ADR.

**Decision 3 — the arity is the protocol-level knob that trades dissection depth against per-round
Court cost, and its value is a measurement.** `palw_kary_rounds_v1(space, arity)` is
`ceil(log_arity(space))`, so raising the arity from 2 to `k` divides the round count by `log2(k)`
and raises what one round carries. **Fewer rounds is not the same as less cost**, and this ADR does
not assume it is: `rounds(k) × cost_per_round(k) < rounds(2) × cost_per_round(2)` is a claim about
this tree's close bytes and the seat's throughput, not an identity, and nothing here is entitled to
it until §5's generator has measured both sides. Decision 3 records only that the arity — not the
ladder — is the parameter where a future width problem is paid for, and that it is chosen at mint
because it sits inside the bundle. The value stays open; §7 says so. *This amends ADR-0082
Decision 3 only if the arity is later moved.*

**Decision 4 — a model too wide for the minted ladder is a new class on a new ruleset, not a raised
ceiling.** The honest shape, and the one the code already forces. A network that wants a wider model
than it minted for takes a flag day or a re-mint; it does not get to widen its ladder in place,
because the ladder is in its identity. Stating this stops the next author from proposing an
in-place raise and discovering the identity problem after the design is written.

**Decision 5 — the time-direction split is the free-prompt lane's, and the attempt lane does not
get one.** ADR-0080 already bounds the verified unit on the free-prompt lane by interval, where
splitting a long answer improves both what a seat can verify and what a user can be served.

The attempt lane is refused it **not because it cannot be done, but because it would change the
consensus semantics of an attempt**. ADR-0072's unit is one execution, one ticket, one draw;
segmenting an attempt into `segment 1 … segment n` makes the ticket a sequence, which changes what
is drawn, what is priced and what a claim is — a change to ADR-0072 and ADR-0074, not a Court
optimisation. This ADR declines it on those grounds and records the reason, so the next author
reaches the semantics question deliberately instead of discovering it after the design.

## 5. The arithmetic, to be produced and not transcribed

> **Numbers in this section are generated artifacts, not normative hand-maintained constants.**
> A reader who needs a value runs the generator; a reader who finds a value here that the generator
> does not produce has found a bug in this document, not a decision.

The table this ADR needs — provisioned leaves, arity, bisection rounds, worst-case duration, margin
against `window_court` — must come from `misaka-palw-base0/src/bin/base0-class-sizing.rs` extended
to print the duration and the margin, and be pinned by a test the way
`provisioning_the_whole_step_space_costs_four_rounds` pins the round count today. **No number in
this section is written by hand.** This project's own lesson is that a worked example with no test
behind it is a number that drifts, and ADR-0087 §4's price column drifted twice for exactly that
reason.

What the table must answer before a mainnet card is minted:

* the largest `max_step_leaf_count` that fits `window_court` at the card's `turn_deadline_daa`,
  at arity 2 and at whatever arity the card chooses;
* the worst-case leaf count of every class the card registers, against that number — the dense
  graph-v5 512 row's canonical job is 6,630,544 and its worst case 52,778,128, both already
  measured in `palw_qwen25_profile.rs:949`;
* the margin, stated as a percentage, so the next model's fit is a lookup rather than a rediscovery.

## 6. Invariants the tests must hold

1. `rounds(max_step_leaf_count, arity) * turn_deadline * 2 + terminal <= window_court` for every
   shipped preset — asserted, not commented.
2. Every registered class of every shipped preset satisfies all three admission bounds, with the
   margin printed on failure.
3. Raising `max_step_leaf_count` on a preset moves that preset's `consensus_params_id` — the
   irreversibility Decision 4 rests on, asserted rather than assumed.
4. The sizing table is generated, and the test that pins it fails if the generator's output moves.

## 7. What is deliberately not decided

* **The fused-attention responder.** Audit C-2. It is the missing half of a court that already
  exists, it is the size of the close prover, and it belongs to its own ADR or to ADR-0082's
  implementation record — not here.
* **Whether the RC's arity moves.** Decision 3 names the knob; the measurement decides the value.
* **Whether testnet-11's ladder is re-armed at a height.** ADR-0084 U-08 / audit H-4 is an arming
  of `Params::palw_court_ladder`, and its schedule is an operator decision.
* **Any number.** §5 says why.

## 8. Implementation record (2026-09-06)

Steps 1 and 2 of §9 are done, and §6's invariants are tests. What the generator then measured is
worth reading before Decisions 1 and 3 are taken, because it changes what they are choosing
between.

**The generator.** `misaka-palw-base0 --bin base0-class-sizing` gained an ADR-0092 section that
reads the shipped RC bundle — turn deadline, terminal rounds, arity, close chunks, close-assembly
reserve, `window_court` — and prints, for each (history positions × arity) pair, the widest
`max_step_leaf_count` whose worst-case prosecution still fits. The predicate is the shipped one,
`palw_attn_court_admits_row_v1`, so a ladder the table calls admissible is one class admission
admits. No figure is written into this document.

**What it found, and it is the reason Decision 3 matters more than it looked.** At zero history the
wall is a long way off; the ladder is bounded by the round count alone and the same ceiling is
reached at every arity. Once a dispute must also dissect a history, the history's rounds — not the
ladder's — are what spend the window, and at the shipped arity the admissible ladder falls steeply
with the context. The shipped ladder and the shipped arity are a legal pair, and at the dense row's
own context they are *exactly* a legal pair: the widest ladder that arity can prosecute at that
context is the one the RC froze, with no headroom above it. That is pinned by
`the_shipped_ladder_is_the_widest_the_shipped_arity_can_prosecute_at_its_own_context`, which fails
in both directions — if the ladder is raised without the arity or the window moving with it, and if
the pair silently gains room.

So Decision 3's knob is not a refinement. On this window, raising the arity is what makes a wider
ladder reachable *at a real context at all*, and the decision table is where the two numbers are
priced against one another. The value stays open, as §7 says: this measures the wall clock, and the
bytes one round carries are the close ceiling's half of the same question.

**Step 2 was already done.** `mainnet_card_base_v1` arms both `palw_context_ladder` and
`palw_court_ladder`; the second was the audit's M-15, a regression the merge of the two development
lines had created, and it landed with the 2026-09-06 audit fixes.

**§6's invariants**, in `consensus/core/tests/palw_adr0092_court_budget.rs`, four tests, each with
its negative control run and observed red before being recorded here:

| invariant | test |
|---|---|
| 1 — every shipped preset prosecutes its own ladder inside its own window | `every_shipped_preset_prosecutes_its_own_ladder_inside_its_own_window` |
| the shipped pair is at the wall, not below it | `the_shipped_ladder_is_the_widest_the_shipped_arity_can_prosecute_at_its_own_context` |
| Decision 3's knob is measured, not assumed | `a_higher_arity_buys_a_deeper_ladder_at_the_same_window` |
| 3 — Decision 4's irreversibility | `raising_the_ladder_moves_the_rulesets_fingerprint` |

Invariant 2 of §6 — every registered class against all three admission bounds — is not added here:
`verify_class_admission_v5` already enforces it at registration and the genesis loader already runs
it, so a separate assertion would restate the production path rather than check it. Recorded as a
deliberate omission rather than an oversight.

## 9. Order of work

1. Extend the sizing binary to print duration and margin; pin its output. (No consensus change.)
2. Arm `Params::palw_court_ladder` where U-08 requires it — audit H-4, already fenced, and a
   prerequisite for any of this being measurable on a class wider than `2^22`.
3. Fill §5's table from the generator, then take Decisions 1 and 3's values for the card.
4. The responder, under its own ADR.

## 10. Number hygiene

0092 is free: 0091 is the highest in `docs/adr/` at `4fcce4b0`, and `git grep -n "ADR-0092"` finds
no citation in code or docs. Next free number after this one is 0093.
