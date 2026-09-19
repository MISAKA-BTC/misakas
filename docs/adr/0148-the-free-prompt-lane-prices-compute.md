# ADR-0148 — The free-prompt lane prices compute, and prices it the same for every class

**Status:** IMPLEMENTED behind the ADR-0145 economic bundle: `Params::palw_canonical_work` sets the
UNIT (a free-prompt claim accepted at or past it is priced in compute), `Params::palw_fp_derived_work`
the DERIVATION the price reads. The bundle rule arms them at one height, and a commitment that meets
the unit without the derivation is refused by name (`FreePromptComputeWithoutDerivation`). Dormant on
every preset.

This is the 2026-09-19 audit's **F5** on the lane ADR-0144 makes the product, and the part of **F1**
that lived in that lane. ADR-0145 derived the lane's work from the class's graph (F2) but left it in
step leaves; this puts it in the unit the attempt lane's weight and pay are already in past the same
fence, and takes the class out of the price.

---

## 1. What priced a free-prompt claim, and why none of it was the compute

Below the fence a free-prompt claim's reward was decided by four things:

| | what it was | whose it was |
|---|---|---|
| credited work | step LEAVES of the run | a count of activations — cost counts weights |
| the quantum | `canonical_leaves / quanta_per_canonical_job` of the claim's OWN class | the class registrant's declaration |
| the quanta | `min(⌊credited / quantum⌋, 64)` | a ceiling: every long prompt, every wide model paid 64 quanta |
| the odds | the class's own receipt target, walked on the class's own census against its share | the class's usage, and its share |

So the price of a unit of compute was a function of the model (leaves per MAC-eq spread 6.8x across
the live classes, monotone in width — the widest paid least), of what the registrant declared, of
how long the prompt was, and of how much the class was used. Measured over three shipped graphs, one
identical run's expected wins per unit of compute differed by more than 2x between models; a short
run of the dense row earned zero quanta outright.

## 2. Decision

For a free-prompt claim accepted at or past the canonical-work fence:

1. **The credit is compute.** `fp_derive_credited_compute_v1` — ADR-0145's canonical work of the run
   the commitment states (`prompt_tokens` positions, `decode_tokens_executed` generated), NEW
   positions only (the class's paid prefix as the reused prefix), in the provisional scalar the
   attempt lane's weight is derived in. The declared `work_leaves` must still equal the graph's leaf
   count — leaves are what the court walks — but nothing is paid in them.
2. **One network quantum.** `Q = floor draw / quanta_per_canonical_job`: the liveness floor's derived
   draw (its registry row, frozen when the registry opened it) over the ruleset's existing divisor.
   Genesis's graph, nobody's registration, and no new constant.
3. **Quanta scale the ODDS, not the credit.** `n = min(⌊C/Q⌋, 2^16)` quanta of `⌊C/n⌋` each
   (`claim.pwu = n × ⌊C/n⌋`, uniform, as the consistency check and the validator crate require). A
   quantum carrying `c` wins with `pooled × c / Q`, saturating at certainty, so a claim's expected wins
   are `pooled / MAX × C / Q` however it is split. The 2^16 cap bounds how many BLOCKS one claim can
   ever win; it no longer bounds credited work. (The old refusal to scale a target was about declared
   work, where a producer who chose its leaf count chose its odds; past the fence the compute is the
   chain's derivation and the expectation is linear in it, so there is no shape to grind toward.)
4. **One receipt target for the lane.** The pooled target (`fp_pooled_receipt_key_v1`, a keyed hash no
   class id can reach) is opened by the first compute-priced commitment from the floor's own receipt
   target — the network quantum is one floor quantum's share of the floor's job, so the floor keeps
   the odds it had — and walked at each epoch boundary on the receipt lane's WHOLE output against its
   slice of the census. The per-class receipt targets are frozen past the fence: they serve only the
   leaves-era claims still draining, and walking them on a census the pooled target also answers to
   would be two controllers on one lane. No class has a target, a share or a census of its own in the
   compute era, so nothing a class does moves another's odds.
5. **The price ceiling.** The pooled target is clamped to `palw_work_ticket_target_v1(Q, W)`: a
   quantum never gets better odds than a forward of the same compute at `W`, because `W₀ = escrow /
   rate_max` is the most the network pays for a unit of compute (ADR-0137). This is also the flood
   guard: a quiet epoch eases the pooled target, and without a ceiling one large claim could win
   thousands of receipt blocks before the retarget's ×4-an-epoch clamp caught up. At the ceiling a
   quantum carrying `c` is drawn exactly as a forward of `c` is — the two lanes price one unit of
   compute identically.
6. **A spend weighs one network quantum.** Expected weight is `pooled / MAX × C`: proportional to the
   compute and to nothing about the class. (Weighing a spend by its own quantum would square the
   compute into the weight.) The spend is counted in the pooled census. The spend, the retirement and
   the consistency re-derivation read one function, `palw_fp_spend_weight_v1`, by the claim's era.
7. **The reservation is the claimed compute** in the floor's collateral unit (the exposure basis the
   attempt lane uses) at the network's slash price. Registration already pins every class's
   `slash_value_per_pwu` to the floor's (`SlashValueNotTheNetworks`), so the price is the network's
   in both eras; what changed is that the quantity is compute.

## 3. Paid prompt rows: kept past the claim, for a bounded time

ADR-0145 §6's cache rule reads the prompts a class has already been paid to evaluate. The re-audit
made those rows outlive their claims (so a retired claim's prefill is not sold twice) and did not
update the consistency check, which still required every row's CLAIM to be live: on the first
free-prompt retirement past the fence, every node would have refused its own tip and every pruning
import would have failed. And a row kept for ever would put every prompt ever mined into consensus
state.

Past this ADR a row carries `expires_daa`, epoch-aligned, `PALW_FP_PROMPT_ROW_KEEP_EPOCHS_V1 = 100`
epochs past its acceptance epoch; the first block of an epoch sweeps expired rows (delta-borne, so a
reorg across the sweep restores them); the reading ignores an expired row whatever the sweep has done;
and the consistency check requires every row to be filed under a held class and not past its expiry.
The re-sale that survives is one per window per prefix, by a producer that held the prefix's KV state
for the whole window; the state the rule costs is throughput times the window.

## 4. What this does not do, and why

* **Weight is not lane-neutral.** An attempt block weighs about `W` (its expected attempts times one
  draw), a receipt block one network quantum. The FP lane's weight is the eligible fraction of its
  work, which is conservative — an attacker is not handed a cheaper path to weight — but when the
  attempt lane shrinks (ADR-0144 §6 item 5) the FP lane's weight per unit of work will have to rise
  to `W` per block, which needs the spend-time `W` recorded per spend. Not done here; named.
* **Under the double draw the FP lane may pay up to the network-draw factor more per unit of compute
  than the attempt lane**, because its ceiling is `W` and an attempt block's compute is `W · draws`.
  Intended: ADR-0144's measurement was that the useful lane carried none of the economy, and the
  ceiling bounds how far the difference can go.
* **Cold versus cached is still not distinguishable.** An honest cold re-run of a paid prompt inside
  the window is credited its generation only — the conservative direction, as ADR-0145 §6 records.
* **The producer's finder** returns only winning quanta for compute-era claims (up to 2^16 tickets a
  claim would otherwise be listed); leaves-era claims keep their losing rows.

## 5. Tests

`palw_state_v2::tests::fp_compute_pricing`:

* `a_unit_of_compute_has_the_same_odds_whichever_model_ran_it` — floor, dense row, hybrid: one run's
  expected wins per MAC-eq agree to 0.2 % in the compute era; the leaves era spreads them past 2x.
* `a_compute_era_claim_reads_no_number_its_registrant_wrote` — a re-tiled class declaring a tenth of
  the job: leaves era prices the same prompt differently, compute era byte-identically.
* `the_lane_has_one_receipt_target_and_a_registration_moves_nobodys_odds`.
* `a_spend_weighs_one_network_quantum_and_retires_exactly_that` — through the fold and both
  consistency checks (the retirement is the durable-row bug's regression).
* `a_paid_prompt_row_outlives_its_claim_for_its_retention_and_no_longer` — survival past retirement,
  replay inside and after the window, the sweep at exactly the expiry block, the reorg revert.
* `the_pooled_target_walks_on_the_lanes_output_and_never_past_the_price_ceiling`.

## 6. Addendum (2026-09-20): the entrance prices with the ledger's expression

The lane moved to compute; its entrance did not. A gateway sized a commitment's exposure with
`misaka_palw_fp_submit::fp_claim_exposure_v1` — quanta of the class's leaves times the slash rate —
and the rail's watcher checked the bond's room with the same number, while the fold reserved the
claim's COMPUTE in the floor's collateral unit. For the floor the two agree by construction; for a
wider model the reservation is several times the leaves figure (the spread §1 measured, now on the
exposure side). A bond with room for the leaves figure and not for the reservation got a commitment
written, carried and paid for, and then refused at the transition as `FreePromptExposureCeiling`.

**One expression, asked for.** The fold's pricing — the derivation, the prefix accounting, the
network quantum, the quanta, the pwu and the reservation, for both eras — is now one pure function
of the state, `palw_fp_commitment_price_v1`, and the fold calls it. A node answers
`GetPalwFreePromptPrice` (op 187, wRPC and gRPC) with the same function over its tip at the
virtual's DAA, plus the bond's room by the fold's own two terms (`palw_fp_bond_room_v1`). The gateway
asks after the job ran and before the commitment is written, and uses the chain's reservation,
quanta and room; a refusal the fold would make is a `commit_refusal` by name. The rail's watcher
asks before it pays a carrier fee and gives up on a job the chain would refuse. Both ask on a
connection opened for the one question, because a node older than the op closes the WebSocket on
it, and both fall back to the leaves figure there.

Test: `fp_compute_pricing::the_entrance_prices_a_commitment_exactly_as_the_fold_reserves_it` — both
eras, two classes: the quoted quanta, pwu and reservation equal the claim the fold writes one block
later, the bond's room moves by exactly the quote, and a lying leaf count is refused by both with
the same error.
