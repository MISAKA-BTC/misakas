# ADR-0093 — The court can try a fused row; the responder is what is missing

* Status: PROPOSED 2026-09-06 (design only). **§6 steps 2–4 IMPLEMENTED 2026-09-11, together**
  (§9): both A16 families answer a fused dissection — the root claim, every round, the
  challenger's choice and the bottom's close — with every number computed by the court's own
  kernels, drilled on real captures before anything relies on it (§7's condition). Step 5 is
  settled by a fact the design did not have: the exemption it would narrow was never armed on any
  preset, and is now pinned unarmed. Step 6 — the fleet upgrade that makes testnet-11's dense
  graph-v5 producers defendable — is the operator's. **Amended 2026-09-11 (§10):** Decision 6
  (admission refuses an undissectable fused class past its own fence, `None` everywhere) and
  Decision 7 (a forged FOLD is bottomed from the challenger's honest prefix and the root claim's
  own tile — no row served by the accused) are built; §9's straddling finding is corrected.
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

## 9. As built (2026-09-11)

On `feat/adr-0099-sharded-seat`, after ADR-0102, in one piece — §7 asked that no half ship.

**Decision 1, smaller than designed: a family reads, the court's kernels compute.** The trait
verb is `attn_site_evidence(material, narrowed, carried_prompt)`, not `attn_tile_claim`: a
family returns what its capture COMMITTED about the site — the binding, the opened output tile
and query row, the four registered narrowings opened against the class root, the head's query
slice and the layer's K and V series, and the bottom's evidence (the anchor checkpoint with every
chunk of its state, or the cache-write rows) — and `palw_attn_responder_v1` computes the root
(`a16_attn_root_claim_v1`), every range (`a16_attn_tile_triple_v1` folded by `palw_attn_fold_v1`),
the challenger's choice and the bottom, the same calls `check_attn_dissect_bottom_v1` makes.
Decision 4's hazard — statistics "close but not exact" that convict their own producer — has no
family code to live in. `attn_tile_claim` is retired. `supports_dissection` is true where the
verb exists AND the class's fused sites can be dissected (`palw_fused_sites_are_dissectable_v1`);
`has_fused_site` says whether there is anything to dissect.

**Whose rows.** The evidence is read from the capture's OWN committed rows — a dense capture's
retained tiles, or a fold re-executed and required to reproduce its own root — so a challenger's
bottom opens the accused's commitments, a forged output tile included; the checkpoint leg comes
from a re-execution (the per-position cadence retains none) and the anchor's state from the
seat's recompute kernels, each refused unless it roots to the capture's own binding. Openings
are walked off `PalwStepMerkleTreeV1`, the step tree built once and pinned path-for-path against
`step_merkle_path_v1` (a bottom and its rows are ~a thousand paths over millions of leaves).

**Decision 3, the panel.** Four moves in `kaspad/src/palw_panel.rs`, before the ladder's arms: the
responder's root claim at a fused terminal (the arity the ruleset derives, signed under the
claim's key) and its rounds; the challenger's choice — the first child its OWN execution's
recompute does not reproduce, and silence when every child does (an honest disclosure has no
winning choice); and the bottom's close, assembled from the ACCUSED capture (pulled when not
held), filed by a party only when the verdict is its own win. Evidence is cached per session and
capture; the court dedup keys give the dissection its own round space (`court_move_round_v1`), so
a round that restarts at 0 is never taken for a ladder move already sent. The court duty view
carries the phase and the class's fused bit (`PalwCourtDutyV2::{dissection, fused_class}`).

**Decision 2 / step 5, settled by a fact.** `palw_court_responder_coverage` — the mercy this ADR
would narrow — is `None` on every preset: it was never armed. So there is nothing to narrow, and
the kaspad pin that was to go red "the day a responder exists" now says the opposite thing it
should: the panel builds all four moves, and the exemption stays unarmed on every preset. The
consequence is the one §4 feared, now answerable: on a chain whose k-ary court is armed (the RC
base and devnet, graph-v5 in genesis), a fused terminal's silence is a conviction, and before
this build no binary could file the root claim — an honest dense graph-v5 producer brought to a
fused leaf lost its reservation by silence. A node running this build answers; one that does not
still cannot, which is why step 6 is a fleet upgrade. The producer now refuses to underwrite a
fused claim it could not defend (`has_fused_site && !supports_dissection` where the k-ary court is
armed), as it already refused one the DA court could default.

**Found on the way.**

* **A fused tile wider than a head is undissectable, and admission never asked.** The fusion
  inherits the tile of the attention-table node it replaces, which the hybrid tables budget per
  geometry: at the fuzz corpus's tiny geometry (the fixture the hybrid test executes) that is the
  whole 64-lane row over 16-lane heads, so its graph-v5 fused leaf is refused by the court as
  `FusedTileStraddlesHeads`, and admission (which checks the query slice's tile, not the
  output's) does not refuse such a class. Graph-v6 (ADR-0102) cuts that one node's row at the
  head, so its fused leaves are dissectable at every geometry by construction; a backend whose
  class fails the check reports `supports_dissection() == false` and is not produced under where
  the k-ary court is armed. The admission refusal is §10's Decision 6.
  *Corrected 2026-09-11:* this bullet first said the tile was "512 on every hybrid geometry",
  generalizing from the fixture. Probed over the shipped geometries (2B and 35B-A3B: 8 lanes;
  27B: 4 lanes; every `n_ctx` from 8 to 131,072), the graph-v5 fused tile is inside its 256-lane
  head — so the shipped graph-v5 hybrid rows ARE dissectable and their backends say so; what
  graph-v6's head-width tile buys is that the property no longer depends on the budget.
* **The gap guard read the wrong half of a file.** It cut "production" at the first
  `#[cfg(test)]`, and `qwen25_a16_backend.rs` carries a test-only helper a thousand lines above its
  trait impl — so the guard could never have gone red for that family. It now cuts at the first
  test module and asserts the trait impl is inside what it reads.

**Tests** (§5): invariant 2 and 3 on REAL captures —
`a_real_fused_capture_answers_its_dissection_exactly_and_a_forged_row_is_convicted` (the dense
graph-v5 row executed; both layers, both heads, every court tile of a two-tile history acquitted
from the anchor; a negative control in which one weight-carrying input code moved breaks the
fold) and `a_real_hybrid_fused_capture_answers_its_dissection_exactly_and_a_forged_row_is_convicted`
(graph-v6 through the composed anchor; graph-v5's refusal pinned by name); through the CHAIN —
`the_parties_moves_from_evidence_acquit_the_honest_and_convict_the_forged_through_the_chain`
(ADR-0082's drill with every hand-written claim replaced by the evidence functions); invariant 4 —
the forged arms, where the least lie that finalizes to a forged tile is followed into the child it
hides in and convicted at that bottom; invariant 1 — `a_family_takes_the_dissections_turn_with_both_verbs_or_neither`
and the producer guard.

**Not done.**

* A live devnet drill of the dissection: the panel's arms are compile-checked, source-pinned and
  built from functions the tests play, but no node has yet filed a root claim on a running chain.
  The classic ladder in front of it is carrier-bound on one host and needs six panel seats, here
  six holders of the 1.8 GB A16 artifact — a fleet drill.
* **A forged claim whose capture is a FOLD cannot yet be bottomed.** The free-prompt lane retains a
  fold (no rows), and the evidence reads the accused's rows by re-executing — which reproduces the
  honest execution, not the forged one, so the evidence refuses by name ("a folded capture keeps
  no rows, and its re-execution is not the same execution"). The attempt lane retains dense
  captures and is unaffected. The missing piece is the bottom's twin of ADR-0085's close from
  served intervals: the accused's rows around the one tile, served with their paths.
* ~~The admission refusal for classes whose fused sites cannot be dissected~~ — §10, Decision 6.
* Step 6.

## 10. Amended 2026-09-11: Decisions 6 and 7

**Decision 6 — admission refuses a fused class no dissection can try, past its own fence.**
`Params::palw_fused_dissectable` (`None` on every preset; `Some`-only in the fingerprint, the fence
schedule and the fork-id probe, so no shipped fingerprint moves). Past it,
`verify_class_admission_v8` refuses a fused class whose output tile is not inside one head —
`FusedTileStraddlesHeads { tile_len, d_head }`, the output half of
`palw_fused_sites_are_dissectable_v1` (`palw_fused_output_tiles_are_one_heads_v1`), the predicate
the backends' `supports_dissection` already reads, so the gate and the backend are one spelling. A
genesis row is judged likewise when the fence is armed from genesis
(`palw_genesis_holds_undissectable_fused_class_v1` in `validate_palw_v2`). It is a fence and not a
fix because a live chain may already hold such a class. No shipped row fails it — the dense graph-v5
row's fused tile is 8 lanes over 128, the hybrid graph-v5's 8 (2B, 35B-A3B) or 4 (27B) over 256 at
every `n_ctx` probed, graph-v6's the head itself — so the class it exists for is a REGISTRANT's
graph: the dense row with its fused tile widened to two heads, which the gate admitted before this
fence and the court then refuses to dissect. Pinned both ways
(`an_undissectable_fused_class_is_refused_by_its_fence_alone`,
`the_dissectable_fused_fence_is_dormant_visible_when_armed_and_judges_genesis_from_genesis`).

**Decision 7 — a forged fold is bottomed from the challenger's honest prefix and the root claim's
own tile; the accused serves nothing.** The hole was worse than §9 said: at `Terminal` the move is
a close the accused never files against itself, so the whole-session backstop ends an unclosed
session on the CHALLENGER's side — a bottom the challenger cannot assemble acquits the forger. A
served-interval bottom (ADR-0085's twin, as §9 guessed) would not close it, because a forger need
not serve. What the challenger holds is enough without the accused:

* the court narrowed to the FIRST leaf the challenger could not reproduce (its choice rule is the
  first child its own execution does not reproduce), so every committed leaf before the disputed
  one is a leaf the challenger's own re-execution computes;
* the accused's root claim — which it must file, or its silence convicts — carries the committed
  output tile at that leaf, opened against the accused's own root.

`PalwStepPrefixTreeV1` rebuilds the accused's tree over `[0, i]` from exactly those two: nodes wholly
inside the prefix from the prefix's leaves, the node just right of it at each level from the right
siblings on `i`'s own path, every left sibling on that path compared with the prefix's node as it
is built. Every row a bottom opens (the query row, the cache rows, the output tile) is at or before
`i`, and the leaves after `i` — which a fold does not keep and a forger need not reveal — are never
needed. The evidence refuses by name unless the rebuilt root IS the binding's. The checkpoint leg is
the fold's own retained leaves (`base0_checkpoint_leg_of_retention_v1`, now the one spelling of
"which retention a fold's leg has", shared with the interval lane's anchor). The trait verb takes the
tile (`attn_site_evidence(…, accused_out_tile)`); the panel's close reads it off the chain
(`attn_root_out_tiles_from_chain_v1` over `walk_accepted_lifecycle_objects_v1`: the accepted
`CourtAttnRootClaimed` for the session, walking the selected chain's accepted lifecycle transactions
down to the session's own opening height) and keeps only a tile whose opening proves against the
claim's root at the narrowed leaf. Nothing on chain moves.

Tests: `the_prefix_and_the_last_leafs_path_open_every_leaf_before_it_against_the_accused_root` and
`a_prefix_that_is_not_the_committed_one_is_refused_or_roots_elsewhere` (every leaf count to 40,
every `i`, every `j`, a forged suffix); `a_forged_fold_is_bottomed_from_the_honest_prefix_and_the_root_claims_tile`
(dense tier: one forged execution retained both ways — the fold alone refused by name, the fold with
the tile equal to the dense capture's evidence opening for opening, a tile that is not the accused's
refused, the least lie convicted from the fold's evidence); the same equality on the hybrid tier
inside its fused test; `the_walk_visits_what_the_chain_accepted_newest_first_and_stops_at_the_height`
(the chain read on a mock chain); the production wiring pinned in the panel's source test.

**Corrected.** §9 said the attempt lane (dense) was unaffected. It is for the drill's forgery (a
committed tile altered, the execution itself honest) and not in general: the evidence reads the
checkpoint leg from a re-execution, and a forger whose downstream execution follows its lie commits
different checkpoint leaves after the disputed call — the re-execution's leg does not root to the
binding, the evidence refuses by name, and the backstop acquits. A fold carries its checkpoint
leaves (Decision 7 reads them); the dense retention (`Base0RetainedMaterialV1`) carries tiles, rows,
ids and, under the per-position cadence, no chunks — no leaves. The fix has Decision 7's shape: the
dense retention carries its checkpoint leaves (a material version `verify_material` checks against
the binding's checkpoint root, so a producer cannot serve the capture without them), or the
accused's anchor is filed on chain with its root claim. **Not built.**

**Still not done:** the dense retention's checkpoint leaves (above); a live devnet drill; step 6.
