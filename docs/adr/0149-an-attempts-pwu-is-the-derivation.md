# ADR-0149 — An attempt's pwu is the derivation, so the weight reads it directly

**Status:** IMPLEMENTED behind `Params::palw_canonical_work`, one of the three fences of the ADR-0145
economic bundle (`validate_palw_v2` arms them at one height or not at all). Dormant on every preset.

This is the 2026-09-19 reward audit's **F1** — *fork-choice weight is a number the registrant
chooses* — closed the way ADR-0145 I1 asks: the chain derives the value and refuses a claim whose
declared value differs, rather than correcting it silently.

**On the history.** The rule in §3 landed on 2026-09-20 inside commit `59931c38`, whose message
describes the ratio re-pricing of §2 instead: two sessions were working in one worktree and a
`git add` of one swept up the other's uncommitted change. The history is unpushed and was left as
it is by agreement; this ADR is the record of what that commit's code does. §5 and §6 — two defects
in that code, found while writing its tests — land with this ADR.

---

## 1. What F1 was

`claim.pwu` carries 90–99 % of fork-choice weight. Below the fence it is
`expected_attempts(class target) × pwu_per_inference`, and the second factor is the registrant's:
under `DerivedV1` it is the step-leaf count the registration declared, under `MaxPerAttempt` any
number the producer likes up to a cap the registrant also chose. The audit measured the result as
24,572× weight per executed MAC-equivalent across the space of legal declarations.

## 2. Why re-pricing the weight was not enough

The first repair left `claim.pwu` alone and re-priced the weight: `claim.pwu × derived / declared`,
so the declared factor cancels and `expected_attempts × derived_per_draw` remains. The numbers are
right — the counterexample properties `59931c38` un-ignored measure exactly that product — and the
rule still had three holes:

* **The declaration stayed an input everywhere except the weight.** It was on the wire, in the claim
  record, in the exposure reservation, in the producer's headroom prediction and in every explorer
  and RPC reader. Each of those is a place for the next finding.
* **A cancellation holds only while its two factors agree at two chain points.** The weight is priced
  at a claim's `Final` and re-derived at its retirement and at every consistency check; the ratio is
  safe only because `pwu_rule` is written once and re-registration forces the same graph. That is an
  argument a later change can break without touching this code.
* **`MaxPerAttempt` has no factor to divide by.** "Any number up to the cap" is not a per-inference
  cost, so the ratio re-priced it by a number that was never what the producer chose.

## 3. Decision

For an attempt at or past the fence's height:

1. **One legal pwu, and it is the chain's.** Admission item 6 requires
   `attempt.pwu == palw_attempt_derived_pwu_v1(effective class target, derived draw)` — the expected
   attempts a win costs at the target, times the derived work of the draw the class really runs
   (`palw_canonical_per_draw_v1`, the registry row's `economic_ccu_per_claim`). On both rule forms.
   Any other value is `PwuClaimNotDerived { claimed, derived }`; a class the chain holds no derived
   draw for is `PwuUnderivable { class }` — refused by name, never priced on the declared basis
   instead.
2. **The weight is the pwu.** `palw_claim_canonical_weight_v1` returns `claim.pwu` for an attempt
   accepted past the fence. There is nothing to re-price: the claim carries the derivation.
3. **The producer is handed that pwu.** `palw_producer_facts_v3` returns the value item 6 accepts and
   the exposure the fold will reserve for it, so a producer never builds an attempt its own chain
   refuses and never mispredicts its own headroom. A class with no derived draw has no facts past
   the fence: the producer holds rather than mining into a refusal.
4. **The reservation is one inference's derived work in the collateral unit**
   (`palw_exposure_pwu_v3`) on both rule forms. Past the fence a `MaxPerAttempt` claim's pwu is the
   derived work of a WIN; what a defaulting producer owes is one inference, as it always was.

Item 6 is checked at the attempt's own point, like the `DerivedV1` equality it generalises
(ADR-0045 Decision 1): the fold applies merged attempts at later points, where the target may have
moved, and the target that counts an attempt's executions is the one it faced.

## 4. What stays

Below the fence every reading is byte-identical. `palw_claim_canonical_pwu_v1` stays in
`palw_canonical_work_v1.rs` as the ratio's statement and the subject of its own tests; the fold
reads it nowhere. No parameter was added and no serialized structure changed, so no fingerprint on
any network moves (`palw_the_release_did_not_move` pins them).

## 5. The floor before its row

The registry writes a class's row at its first span boundary, and a chain block's attempt is
admitted against its **parent's** state. `validate_palw_v2` lets the bundle arm at the registry's own
height — and arming the economy at a network's birth is exactly that. Then the first blocks past the
fence are admitted against a state that holds no row at all, item 6 finds no derived draw for any
class, the floor's included, and every attempt is `PwuUnderivable`. The producers, reading the same
state, hold. A network whose only block type is the attempt stops there; on one that runs a heartbeat
miner the heartbeat lane restarts it an hour later and walks the DAA to the next span boundary, where
the registry finally steps. Either way the fence stops the chain it was built to cross, and the unit
tests could not see it, because every fixture that armed the fence inserted the rows by hand.

**The floor is priced on the draw its row will carry.** `PalwChainStateV2::palw_attempt_per_draw_v1`
reads the class's row, and — only for the floor, only past the fence and only while the floor has no
row — the floor's entry in the table the registry reads (`genesis_works`), which
`step_model_registry` copies into the row verbatim. The node resolves that entry for the admission
(`PalwEpochBudgetFencesV1::base_known_draw`) and the producer; the fold reads it from its own
registry input. So the gate that admits the floor's first row-less attempt, the producer that built
it and the ledger that reserves for it read one number, and the row the registry later writes
carries the same number: the claim is never re-priced, and the next block, reading the row, derives
the same pwu (`the_floor_produces_on_the_first_blocks_past_a_bundle_armed_at_the_registrys_height`).

Only the floor, because it is the one class whose production cannot wait: every other class is
refused its row-less window already, by the work target, which arms at the registry's height and
prices a model class from its row. The exposure basis takes the same reading
(`palw_exposure_basis_v2`): a basis that needed the floor's row would reserve the floor's first
row-less attempt on its claimed pwu — the derived work of a win — and the ceiling and the ledger
would part at the very block the fence is crossed.

**What this does not extend to.** A compute-priced free-prompt commitment in that window is still
refused by name (`FreePromptNetworkQuantumUnknown`) and dropped, which costs its committer a fee: the
lane's pooled receipt target is seeded from the floor's row too, and the window is at most one span.

## 6. Merged attempts

A merged blue's attempt is admitted twice: by the node's pre-check (`palw_v2_merged_works`, with the
block's fences) and by the fold's own re-run against its live state. The re-run built its fences with
`..Default::default()` — `canonical_work_daa: None`, the declared rule — while the pre-check passed
the height. Past the fence no attempt could satisfy both: every merged blue's work was skipped,
claimed by nobody, paid to nobody and weighing nothing, on any network with parallel blocks. The
re-run now takes the height and the floor's draw
(`a_merged_attempt_past_the_fence_is_folded_on_the_rule_its_pre_check_admitted_it_on`).

The defect is older than §3 by one fence: the canonical-work fence's exposure half already had the
re-run measure the declared basis while the fold reserved the derived one, which is the audit's
finding (a) — the ceiling and the ledger pricing one claim differently — for merged work.

## 7. Tests

* `palw_admission_v2::tests::past_the_canonical_fence_the_one_legal_pwu_is_the_rows_derivation` —
  `DerivedV1`: the declared basis below, the row's derivation past, the declaration refused past,
  a row-less class `PwuUnderivable` with or without the floor's known draw.
* `palw_state_v2::tests::adr0135::attempt_pwu_is_the_derivation`:
  * `the_floor_produces_on_the_first_blocks_past_a_bundle_armed_at_the_registrys_height` — §5
    through the producer, the gate, the fold mid-span, the registry's boundary and the next block;
  * `a_merged_attempt_past_the_fence_is_folded_on_the_rule_its_pre_check_admitted_it_on` — §6;
  * `past_the_fence_a_final_weighs_exactly_the_derivation_it_was_admitted_on` — the rule end to end,
    `MaxPerAttempt` floor, from the gate to `safe_weight`.
* The counterexamples — F1's decode declaration and re-tiled row, and I1's 24,572× — run in
  `palw_reward_properties_v1` against `fork_weight_past_the_bundle_v1`, which models this rule.

---

## Implementation (2026-09-21)

ADR-0146's search reports 1.000000× — P4 does not forbid this fence. It stays `None` on every
shipped preset. Choosing a height is a different commit (0144 item 0).
