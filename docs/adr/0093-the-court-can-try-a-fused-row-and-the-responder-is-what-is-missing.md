# ADR-0093 — The court can try a fused row; the responder is what is missing

* Status: PROPOSED 2026-09-06 (design only — §7 is explicit that nothing here is implemented, and
  why shipping half of it would be worse than shipping none)
* Builds on: [0082](0082-the-close-is-flat-in-the-context.md) Decisions 2 and 3 (the k-ary history
  dissection and its arity), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md)
  (adjudicability is the price of weight),
  [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) §7 (which names this as the
  one thing it deliberately did not decide).
* Amends: nothing. It supplies the half of ADR-0082 Decision 2 that was specified and never built.
* Supersedes nothing.

## 0. The sentence this ADR is

Every move of the fused-attention dissection exists in consensus and every piece of its arithmetic
is written twice — once to fold and once to check — but **no shipped binary can produce the first
move**, so on a fused class an honest producer is convicted by silence and a dishonest one is
never convicted at all. This ADR specifies the producer, and says why it is a backend verb over
one history tile rather than a new court.

## 1. What the audit found, and what it did not

The 2026-09-06 mainnet audit's C-2 (with H-5) reads: *a stranger convicts any dense-tier producer
with no evidence.* The mechanism is not a missing rule. It is a missing party.

* `PalwConsensusObjectV2::CourtAttnRootClaimed`, `CourtAttnDissected` and `CourtAttnChildChosen`
  are consensus objects with acceptance rules, signatures and fold arithmetic
  (`palw_state_v2.rs:3003`, `:3032`, and the variant after them).
* `palw_attn_dissect_v1`'s arithmetic is complete in both directions: `palw_attn_fold_v1` composes
  `k` children into their parent and `palw_attn_fold_check_v1` refuses a composition that does not
  reproduce it, with `palw_attn_child_ranges_v1` splitting a range the same way on both sides.
* The arity is derived, not chosen — `palw_court_params_at_v2` reads the widest registered site and
  `palw_court_arity_v1` returns the first arity that fits the clock and the carrier, or refuses
  (ADR-0092 Decision 3).
* And `git grep` finds **no construction of `PalwAttnRangeClaimV1` outside the fold and the test
  modules**. The only production producer is the fold, which needs children before it can make a
  parent.

The consequence is stated in the tree's own words at `palw_state_v2.rs:7800-7812`: on a fused site
an `Arithmetic` whole-row close is refused by `check_close_cost_v2` and an `AttnDissection` close is
refused `NoDissection`, so *every* terminal leaf of a fused class is undefendable. The
2026-09-06 audit fixes stopped the conviction, behind a fence and in the direction that does not
burn an honest producer's collateral. They did not make the court work.

## 2. What the responder actually has to compute

Less than it looks, and that is the whole reason this ADR is worth writing rather than deferring.

`PalwAttnRangeClaimV1` is three fields — `max: i32`, `exp_sum: i64`, `v_acc: Vec<i64>` — the running
statistics of an online softmax over a range of key positions. `palw_attn_fold_v1` composes them.
So a responder that can produce the triple for **one history tile** can produce every claim in the
dissection by folding, at every rung, without the backend knowing what a court is:

```
tile claims (from the backend, per PALW_ATTN_HISTORY_TILE_V4 positions)
        │  palw_attn_fold_v1
        ▼
   child claims  ──►  parent claim  ──►  …  ──►  root claim
```

That is the decision this ADR exists to take: **the backend's obligation is one tile, and the
court's shape is the fold's.** A backend does not implement a dissection; it answers what its own
attention computed over a contiguous run of positions, which is a quantity a streaming attention
kernel already forms and discards.

## 3. Decisions

**Decision 1 — the responder's backend obligation is a single verb over one history tile.**

```rust
/// The online-softmax statistics this family's attention formed over `[first, first + count)`
/// of the disputed site's key positions. `None` by default: a family that has not implemented
/// it cannot take a dissection's turn, and `supports_court` must say so.
fn attn_tile_claim(
    &self,
    material: &[u8],
    site: &crate::palw_attn_court_v1::PalwAttnBottomSiteV1,
    first: u64,
    count: u64,
) -> Option<crate::palw_attn_dissect::PalwAttnRangeClaimV1>;
```

`PalwAttnBottomSiteV1` is the site type the court already derives — `palw_court_v2::palw_attn_dispute_site_v2`
builds it, and it carries the disputed leaf's own coordinate, the head's slice within a cache row
and the job's declared prefill, so the backend is told *which* attention it is being asked about and
never has to choose. The lanes the root claim reports (`head`, `lane_first`, `lane_count`) and the
history width (`history_positions` = `kv_len` at the disputed position) are read off the same site,
which is what stops the responder and the challenger describing two different rows.

Defaulted to `None`, exactly as `disclose_trace_event` is defaulted to an error, and for the reason
the 2026-09-06 audit's C-5 repair established: a family that cannot answer must be visible as one
before a court is armed over it, not after it has been convicted.

**Decision 2 — when a responder exists, the mercy that excuses its silence must be narrowed, and
that is a second activation.** What the 2026-09-06 audit actually landed for C-2/H-5 is not an
assembly refusal: `palw_court_responder_coverage` gates an arm in the fold (`palw_state_v2.rs:8287`)
that, past the fence, ends a fused-terminal session which `owes_the_dissection_opening` without
convicting or fining anybody — `rearm_after_unanswered_opening`. It is mercy for a move *no party
in the tree can make*.

The moment `attn_tile_claim` ships for a family, that sentence stops being true of it, and mercy
that outlives its reason is indistinguishable from a court that cannot convict. So this ADR's
landing has a second half: the arm must ask whether **this claim's class** has a responder, not
whether the release has none. Two consequences, both deliberate:

* it is a consensus-validity change in the convicting direction, so it is its own fence and its own
  height — never a silent narrowing riding the responder's release;
* until it is armed, a family that CAN answer is still excused if it does not, which is a worse
  place to stop than either end. §6's order of work puts it last for that reason, and §5's
  invariant 3 is what says the responder works before anyone relies on it.

**And `supports_court()` must be SPLIT, not widened.** A first draft of this ADR said it should
come to mean both verbs. It must not: it is one boolean over two unrelated turns, it already means
"this family can disclose and take an arithmetic turn" after the C-5 repair gave all three families
`disclose_trace_event`, and `kaspad/src/palw_producer.rs:735` and `palw_panel.rs:2675` branch on it.
Widening it before any family implements `attn_tile_claim` would flip every family to `false` and
report a disclosure gap that does not exist. So the dissection turn gets its own predicate,
`supports_dissection()`, defaulted `false` and true exactly where `attn_tile_claim` is implemented.
Two turns, two answers.

**Decision 3 — the panel files the moves; the backend never sees a session.** The panel's court arm
gains: on an accusation at a fused site, fold the tile claims into the root and file
`CourtAttnRootClaimed` with the derived arity, the binding, the out tile and the operand openings;
on each `CourtAttnChildChosen`, split the named child's range with `palw_attn_child_ranges_v1`,
fold each part, and file `CourtAttnDissected`. Both signed ML-DSA-87 under the CLAIM's bond key, as
the objects require.

**Decision 4 — a wrong tile claim must be as expensive as no claim, and the design must not make
it cheaper.** A responder that files claims which do not fold to its own root convicts itself by
`palw_attn_fold_check_v1` without any execution being replayed. That is the existing rule and this
ADR does not soften it. The consequence for the implementer is the reason §7 refuses to ship this
half-built: an attention kernel instrumented to emit *approximately* the right statistics is worse
than one that emits none, because the family then answers and loses.

## 4. What this buys, and what it does not

It closes C-2's second half: a fused class becomes defendable, so a court over it is a court and not
a clock. It does **not** close C-2's first half by itself — `palw_kary_court` is `always()` on
`palw_rc_base_params` and on devnet with the graph-v5 row in genesis, so those chains are exposed
until either this responder ships to every seat or the fence is scheduled off. That sequencing is an
operator decision and ADR-0092 §7 already records it.

## 5. Invariants the tests must hold

1. A family whose `attn_tile_claim` is `None` reports `supports_court() == false`, and an assembly
   arming a k-ary court over it is refused — the same shape as the C-5 coverage fence.
2. For every registered fused class, the tile claims fold to the root the responder files:
   `palw_attn_fold_check_v1` accepts the responder's own tree at every rung. A drill vector, not a
   fixture — the audit's own lesson is that a court drilled on an n_ctx-32 toy geometry cannot
   exercise a width-dependent defect.
3. An honest responder wins: a full session played against a correct producer ends `NoFaultFound`,
   at the widest registered site, inside `window_court` at the derived arity.
4. A dishonest producer loses at the rung where its claim stops folding, and the challenger's
   choice reaches that rung in `ceil(log_arity(positions/tile))` rounds and no more.

## 6. Order of work

1. The trait verb and its `None` default, plus `supports_dissection()` beside `supports_court()` and
   a test that pins every shipped family at `false` today — small, behaviour-neutral, and it makes
   the gap visible in the type system rather than in a comment. Decision 2's narrowing of the fold's
   mercy arm is NOT here; it is step 5.
2. `attn_tile_claim` for one family, with invariant 2's drill vector. The floor class first: it is
   the one whose arithmetic is integer end to end.
3. The panel's two arms, against that family.
4. The remaining families, each with its own drill vector.
5. Decision 2's narrowing — the mercy arm asks whether this claim's class has a responder — behind
   its own fence and at its own height, once every registered family answers.
6. Only then, the operator decision in §4.

## 7. Why this ADR ships no code

Because the failure mode of a half-built responder is worse than the failure mode of none. Today a
fused class cannot answer, and the audit's fixes make that a refusal rather than a conviction. A
family that answers with statistics that are close but not exact convicts itself under Decision 4 —
`palw_attn_fold_check_v1` is exact integer arithmetic and does not care why the numbers disagree —
and it does so while holding the claim's collateral. So the order in §6 is not a preference: each
step must be drilled before the next, and the first family's drill vector is what says whether the
kernel instrumentation is exact.

## 8. Number hygiene

0093 is free: 0092 is the highest in `docs/adr/`, and `git grep -n "ADR-0093"` finds no citation.
Next free number after this one is 0094.
