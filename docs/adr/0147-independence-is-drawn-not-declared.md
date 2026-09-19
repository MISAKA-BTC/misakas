# ADR-0147 — Independence is drawn, not declared

**Status:** IMPLEMENTED behind `Params::palw_admission_independence`, which is one of the three
fences of the ADR-0145 economic bundle (`validate_palw_v2` arms them at one height or not at all).
Dormant on every preset. Supersedes the identity test that shipped under the same fence on
2026-09-19 (`palw_bond_is_independent_of_registrant_v1`), which is deleted.

This is the admission half of ADR-0145 (I3, and §7's "independent admission") and the 2026-09-19
reward audit's F3: *a class can be judged entirely by its own registrant.*

---

## 1. Why the first repair was not a repair

A claim's panel is drawn from the bonds that may judge its class: under the registry, those with a
fresh possession proof of the class's artifact; below it, those that declared capability for it.
Both are acts a registrant performs for itself. For a model nobody else runs, the class's own
population is the registrant's fleet, and the panel is the registrant.

The first repair asked whether a seat's `operator_id`, `pubkey` and `payout_payload` differed from
the registrant's. Every one of those is a field the registrant writes into its own
`BondRegistered`. The re-audit showed it with an executable fixture: seven sybils paying seven
addresses were seven independent parties, the class left `Candidate` on them, and a quorum of them
licensed its claim — with the fence armed and no stranger anywhere on the chain.

**No comparison of registrant-written fields can do better**, because there is no beneficial
ownership on this chain to compare. What a registrant cannot write is the composition of a
population it did not choose. That is the whole decision.

## 2. Decision

### 2.1 The outsider seat

For a claim of a **bought** class (`registrant_bond` is `Some`) whose `accepted_daa` is at or past
the fence's height:

* the panel's **first seat** is drawn from the **network's base-class population** — every bond
  eligible to judge the liveness floor, under the claim's own executor exclusions, floor, headroom
  and registration cut — minus the registrant's own bond and operator, ranked by
  `H(outsider-ticket domain ‖ anchor ‖ claim ‖ operator_id)`, one entry per operator;
* the other `seat_count − 1` seats are drawn from the class's own population as before, without
  the outsider's operator;
* **no licence stands without the outsider's `Valid`** — on the V1 quorum arm, on the V2 coverage
  arm (which carried no independence check of any kind before this), and in the acceptance layer
  that assembles and validates both. A quorum of the class's own seats is a quorum the registrant
  may hold entirely; the outsider is a condition of the licence, not one voice in it.

An outsider that cannot judge the class says `Incapable`. The claim then voids at its receipt
deadline, which is where every unreachable quorum already ends. **The cost of a model the network
does not run lands on the claims of that model, never on the seat that declined to pretend.**

Genesis classes have no registrant and are never outsider-judged: their panels are the ones they
always had, and the shipped registry's zero slack (`seat_count + 1` bonds) is untouched.

### 2.2 The population is fixed before its randomness

The anchor's hash is a panel's randomness. Once it exists, a party can search operator keys offline
for a ticket below every eligible operator's — a few dozen hashes against a population of
twenty-seven — and register that bond before the panel binds. `palw_bond_maturity` closes this with
a window, **and it is dormant on every shipped preset**, so today every panel on every network is
open to it.

Past this fence, every seat of a governed claim must be held by a bond that some chain block
**before the anchor** registered (`registered_daa < anchor_daa`); the maturity window, where armed,
combines as the stricter. `a_bond_registered_after_the_anchor_cannot_grind_its_way_into_the_outsider_seat`
runs the grind: without the cut the ground bond takes the outsider seat, with it the bond sits
nowhere and the panel is the one the chain would have drawn without it.

This applies to the whole panel of every claim accepted past the fence, genesis classes included —
it is sortition hygiene, not independence, and it costs nothing a bond registered before the claim
does not already have.

### 2.3 The admission jury

A bought class opens `Candidate` (ADR-0145 §7) and leaves it only when a **jury the network drew**
finds a majority of itself holding the class:

* **population** — active bonds above the floor that serve the liveness floor, registered before
  the span whose anchor seeds the draw began, minus the registrant's bond and operator;
* **draw** — `seat_count` operators, one entry each, ranked by
  `H(jury domain ‖ H(class ‖ span ‖ anchor block ‖ anchor execution key) ‖ operator_id)`;
* **randomness** — the execution lane's seed anchor of the span before (ADR-0130): re-rolling it
  costs a winning inference, where a boundary block's own hash costs a header;
* **verdict** — a strict majority (`seat_count / 2 + 1`) of the jurors each hold a bond READY for the
  class by the registry's own five-clause predicate — the same one the draw's population uses, so
  "ready" means one thing;
* **ration** — one audit per `epoch_length / span_daa` spans, on the spans whose index is a multiple
  of it. Stateless: no row records the last audit, so none can be written or rewound to run another.
  No seed anchor in the span before is no audit; a short jury admits nothing.

A lottery that may be re-run every span is passed by waiting — at a five-DAA span, a registrant who
wins the jury one time in a thousand would be through in under an hour and a half. The rate the chain
can bound is how often the draw is taken.

### 2.4 One claim, one answer

The claim-level rule reads the fence's **height** against the **claim's own `accepted_daa`**, never
against the block reading it — the same construction `palw_canonical_work` uses and for the same
reason: the draw (resolved at the anchor), the binding, every licensing arm and every node replaying
them read one claim at different chain points. A claim accepted just below the fence is bound
without an outsider and licensed without one, even when its licence lands above the fence
(`a_licence_without_the_outsiders_valid_is_refused_on_every_licensing_arm`). The block-level rules
(`Candidate`, the jury, the refused cadence grant, the work-price filter) ask the same height at the
block. `PalwTransitionExtrasV1::admission_independence_daa` is the one field for both, so the two
readings cannot disagree about where the fence is.

### 2.5 What the stratified draw does not have

`palw_shard_licensing`'s per-shard draw seats no outsider, so an outsider-judged claim licensed by
parts would be judged by the population its registrant fills. `validate_palw_v2` refuses a build
arming this fence beside `palw_shard_licensing`, and the fold refuses a part licence of an
outsider-judged claim by name. Both are dormant on every preset.

## 3. The bound, stated at its real strength

Let `f` be the registrant's share of the network's floor population (its operators over all
operators serving the liveness floor).

| event | probability | per |
|---|---|---|
| the registrant holds a claim's outsider seat | `f` | claim |
| the registrant alone passes an admission audit | `P(Hyp(N, fN, s) ≥ ⌊s/2⌋+1)` ≈ `C(s,⌊s/2⌋+1)·f^(⌊s/2⌋+1)` for small `f` | audit period |

Measured on the fixtures (seven sybils against twenty honest operators, `s = 5`): the outsider was a
sybil at 98 of 400 anchors, 24.5 % (predicted 7/27 = 25.9 %); the registrant passed the jury alone at
343 and 366 of 4,000 seeds in two independent seed families, 8.6 % and 9.2 % (predicted
7,371/80,730 = 9.13 %); against two hundred honest operators, at 2 of 4,000 (predicted 0.036 %).
Under the rule this replaces, the same registrant passed **both** with probability one, for the price
of seven payout addresses.

**What the bound is.** Self-judgement is priced in the honest network's size, where it was a
constant six keys. **What it is not:** a party that IS a large share of the network is that share
(`a_registrant_that_is_the_whole_network_is_admitted_by_it` pins the `f = 1` case), and no rule that
reads only the chain can make a network out of nobody.

**The residual, named.** For a class nobody else runs, honest outsiders plead `Incapable` and its
claims void; the registrant's own licensed rate is `f`, and a claim licensed by a registrant-held
outsider was checked by nobody. That leak is bounded by `f` times the class's eligible volume, which
the lifecycle keeps small until verified use expands it — but it is not zero. What closes it is an
outsider that can check a claim WITHOUT holding the model: ADR-0133's segment-scoped receipts let a
seat attest one segment, which needs that segment's weights and nothing more. Its protocol half is
armed at testnet-11 6,100 and its runtime half is not built. The consensus rule here is unchanged by
it — only what an honest outsider can answer instead of `Incapable`.

## 4. What was removed

`palw_bond_is_independent_of_registrant_v1`, `palw_panel_has_independent_seat_v1`, the errors
`PanelWithoutIndependentSeat` and `QuorumWithoutIndependentSeat`, the observation field
`independent_ready_seats`, and the node assembler's identity check before proposing a panel (the
outsider is IN the derived panel, and the acceptance layer demands that panel exactly).
`admission_independence_active: bool` became `admission_independence_daa: Option<u64>`.

## 5. Tests

In `palw_state_v2::tests::adr0135::admission_independence`:

* `the_outsider_is_drawn_from_the_network_and_not_from_the_class` — below the rule every panel of a
  Kimi claim is five sybils; past it the outsider is a sybil at the network share, never the
  registrant or the producer, and the draw is identical under both payout layouts;
* `a_bond_registered_after_the_anchor_cannot_grind_its_way_into_the_outsider_seat` — §2.2, the grind
  run both ways;
* `a_licence_without_the_outsiders_valid_is_refused_on_every_licensing_arm` — V1 and V2, `Incapable`,
  and the claim-keyed era;
* `a_candidate_leaves_only_on_a_jury_the_network_drew` — through the fold, with the lane's real seed
  anchor: a minority seed keeps it out, a majority seed lets it in, the honest network holding the
  model lets it in, no anchor and off-schedule spans draw nothing;
* `the_registrant_passes_the_jury_alone_at_its_network_share_not_by_construction` — the
  hypergeometric bound over four thousand seeds, and payout-layout invariance;
* `a_registrant_that_is_the_whole_network_is_admitted_by_it` — the bound at `f = 1`;
* `a_registrant_paying_its_sybils_separately_no_longer_judges_its_own_class` — the re-audit's attack,
  re-run, now failing for the attacker;
* the three that survive from the first repair: the `Candidate` gate, genesis/dormant roots, and I4.

And `palw_reward_properties_v1::a_class_whose_owner_holds_every_capable_seat_cannot_be_admitted` —
ADR-0145 §8's property, un-ignored: declaring the class buys no juror, and the owner alone is admitted
at its network share.

## 6. Addendum (2026-09-20): a registration does not straddle the fence

Below `palw_admission_independence` a post-genesis registration must take the minimum grantable
share; past it, exactly 0‰. The gate reads the fence at the block that ACCEPTS the carrier. The
Studio economy drill, which arms the registry and this fence at one height, found the gap between
the two: the node registered the moment the registry opened, on terms the RPC had computed at the
sink's DAA (one block behind, pre-fence, 1‰), and the carrier landed past the fence, which dropped
it — "registers at 1‰; past palw_admission_independence a registration buys existence and not
cadence". The panel's own retry only rebuilt it 200 DAA later.

The registration terms are now resolved at the virtual's DAA, which is where a carrier sent now
lands, and a node does not build a registration within `PALW_REGISTRATION_LANDING_MARGIN_DAA_V1`
(10) DAA below a scheduled independence fence (`palw_registration_waits_for_fences_v2`); it builds
once the fence is in force, on the 0‰ terms the gate will apply. A carrier that still lands late is
caught by the panel's retry, as before. The CLI paths (`model add`, `extension submit`) read the
same terms; they have no landing-margin wait, which leaves a ten-DAA window in which a hand-filed
registration can be refused and its fee lost — named, not closed.
