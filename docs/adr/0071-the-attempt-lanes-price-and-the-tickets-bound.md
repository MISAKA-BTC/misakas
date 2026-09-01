# ADR-0071 — The attempt lane's price, the ticket's bound, and who may judge a class

Status: **PROPOSED (2026-09-01).** Written against the mainnet-premise audit of ADR-0068 Phase 2,
whose remaining findings do not fit inside a local fix: each one contradicts a decision some earlier
ADR made deliberately, and says so in that decision's own comment. Builds on ADR-0038 (PALW is the
consensus work), ADR-0045 (`DerivedV1`, the class economy), ADR-0054/0056 (share follows
production), ADR-0060 (liveness doctrine), ADR-0065 (a bond must be earned and a seat must be
someone else), ADR-0066 (the heartbeat lane out of header bits) and ADR-0067 (classes are chain
data, kernels are the build). Consistent with the standing doctrine that consensus changes ship by
activation, never by re-genesis.

## 1. Why these four are one ADR

The audit's premise was the user's: **a Qwen block's weight is given, and weight must not depend on
hash computation.** Measured against that premise, live fork choice already passes — the attempt
lane weighs a constant `1 << 20`, the receipt lane weighs zero, the heartbeat lane weighs
epsilon = 1, and V2 admits no other algorithm, so `calc_work(bits)` is unreachable on a running V2
chain. What does *not* pass is everything downstream of `bits`: the pruning proof priced history by
it (fixed, `cbe5c002`), the lottery still meters *tries* rather than *executions*, the class
difficulty feeds back through the same field, and a claim's collateral was priced on that difficulty
(fixed, this train).

The four items below are what remain. They share one shape: **a quantity that should describe
LLM work is derived from, or bounded by, the hash lottery** — and in three of the four the code
comment at the site states the coupling as an intentional choice. That makes them ADR material
rather than patches, and it makes them one ADR rather than four, because Decision 1 and Decision 2
move the same number in opposite directions and shipping either alone regresses the other.

## 2. What already landed, so the scope is honest

Recorded here because a reader arriving at this ADR needs to know which half of the audit is code
and which is proposal:

* **The pruning proof no longer prices history by `bits`.** `blue_work_diff` took
  `.max(self.level_work)` on the attempt-lane arm, so adopting a heavier history was bought with
  proof-of-work levels. Removed (`cbe5c002`), with a test asserting `level_work(1, 225)` is
  `attempt × 4096` — the inequality that made the `.max()` load-bearing. Level *attainment* still
  requires grinding, so history adoption is grinding-priced linearly rather than exponentially;
  Decision 1 is what removes the remainder.
* **A claim's collateral is no longer priced by difficulty.** `reserved` was
  `attempt.pwu × slash_value_per_pwu`, and under `DerivedV1` `attempt.pwu` is
  `expected_attempts(class_target) × pwu_per_inference` — so a class that retargeted harder reserved
  more against unchanged collateral and locked its own producers out for succeeding. On the floor
  class that is the chain stopping, because a refused attempt on a V2 network is
  `StatusDisqualifiedFromChain` and DAA only advances when blocks are produced. Now
  `palw_exposure_pwu_v1`: one inference's worth, at every site that prices it — admission's ceiling,
  both state writes, the producer's own headroom prediction, and the genesis bind-window gate.
* **The 120 s cadence is a set of fields, applied in one place.** `palw_v2_params_on_base` wrote
  `target_time_per_block` alone, leaving the DAG parameters and the DNS windows counted for the
  base's block rate. At 120 s an inherited `PRODUCTION_DNS_PARAMS.unbonding_period_blocks` states
  14 days and means about 46 years, and `bond_spend_gate` enforces it in consensus. Now
  `Params::with_two_minute_cadence` / `with_palw_v2_depths`, gated in `validate_palw_v2`.
* **The court's close binds the registered class.** `adjudicate_close_proof_v2` accepted a close
  whose `shape_profile` was not the class under judgement.
* **The GDN replay has a ceiling on both arms**, not one.

## 3. Decision 1 — The attempt lane's price comes off `header.bits`

**What is true today.** ADR-0066 took the *heartbeat* lane's price out of `bits`, and its comment
records exactly why: `bits` is the field the difficulty window averages, so a window of rows priced
by their own lane raises the global demand to that lane's price and no other block can re-enter.
The substitution lives in `consensus/pow/src/lib.rs` — the one place every PoW path goes through —
and it covers `POW_ALGO_ID_HEARTBEAT_V1` only. The attempt lane, `POW_ALGO_ID_PALW_COMMITTED_V2`,
still reads its target from `header.bits`, and `pre_pow_validation.rs` still enforces
`header.bits == expected_bits` against the window for it.

**Why that is the same defect.** The attempt lane's *weight* is already a constant, so the coupling
does not show up in fork choice. It shows up in **admission**: `bits` sets the class target, the
class target sets `expected_attempts`, and `expected_attempts` is the number of hash tries an
inference must be paired with. A network that wants "weight does not depend on hash computation"
cannot leave the *rate* of LLM work denominated in a hash difficulty that a window of blocks
feeds back into.

**Decision.** Extend the constant-target substitution in `consensus/pow/src/lib.rs` to
`POW_ALGO_ID_PALW_COMMITTED_V2`, and add the matching bits lock in `pre_pow_validation.rs` so a V2
header's `bits` is a fixed network value rather than a window output. The per-class lottery keeps
its own target — `class_ticket_v2` against `state.class_target`, which is chain state and not a
header field — so difficulty stays per-class and stays retargeted, but it stops riding the field
the global window averages.

**And the retarget's expectation must change with it**, or Decision 1 ships a regression.
`retarget_over_span_v1` computes `expected = share × census.total_daa_blocks / 1000` — a share of
the blocks that *actually happened*. When a class under-produces, the realized total falls with it,
the expectation falls in step, and the retarget cannot see the shortfall it exists to correct. The
expectation must be the **absolute** count the DAA span implies: `share × span / 1000`, where the
span is DAA distance and not a block census.

**The `observed == 0` skip is the same bug's other half.** `palw_state_v2`'s per-class retarget
loop skips a class that produced nothing in the span, on the stated grounds that easing its target
on a span it sat out would be an unbounded walk in the other direction. That is right for a class
that *sat out* and wrong for a class that was *locked out* — one whose share entitled it to blocks
it could not mine. With an absolute expectation the two are distinguishable for the first time
(`expected == 0` is "sat out"; `expected > 0 && observed == 0` is "locked out"), so the split lands
with this Decision and cannot land before it.

## 4. Decision 2 — The ticket is bound to executions, not to tries

**What is true today.** `palw_job_anchor_v1` hashes `(network domain, pre-pow hash, class, bond)`
and deliberately **not** the nonce. Its own doc states the reasoning: binding the nonce "would price
one full inference per PoW try", and job grinding by reshuffling the block "costs a full inference
per try, which is the price the design means to charge." The consequence is measured:
`palw_ticket_v1` is the first 16 bytes of the PoW digest, the producer sweeps
`NONCES_PER_TEMPLATE = 4,000,000` nonces against one template, and so **one inference buys four
million lottery tickets.**

**Why the existing mitigation is not this one.** `palw_admission_v2`'s epoch budget refuses a class
whose accepted blocks would exceed its share of the epoch, so hashing cannot move cadence *between*
classes. It can still move producer share *within* a class, and — the part that matters for the
premise — it means the quantity the chain calls "work" is metered in tries. A chain whose thesis is
"blocks are paid for by actual LLM inference" cannot have its lottery denominated in a unit the
inference does not produce.

**Decision.** Two changes that must ship together:

1. Add a **coarse nonce bucket** to `palw_job_anchor_v1`: include `nonce >> k`, so one inference
   covers exactly `2^k` nonces rather than an unbounded sweep. `k` is a network constant on the
   bundle, hence in `palw_ruleset_id_v2`, hence a value two nodes cannot disagree about.
2. **Decouple `palw_pwu_v1` from `palw_expected_attempts_v1`**: a block's pwu becomes
   `2^k × pwu_per_inference` — the work the producer actually had to do to hold that ticket — rather
   than `expected_attempts(target) × pwu_per_inference`, which is the work the *difficulty* implies.

`k` is the whole design surface. `k = 0` is one inference per nonce, which is the honest extreme and
almost certainly unaffordable. `k = 22` is today's behaviour to within a rounding. The number is an
economic choice about how much hash the network is willing to let one inference cover, and it should
be picked from a measurement of inference cost against hash cost on the shipped classes, not from
this document.

**Interaction with Decision 1, stated because it is the reason these are one ADR.** Decision 1
removes `expected_attempts` from the *header*'s price; Decision 2 removes it from the *pwu*.
Shipping Decision 2 alone leaves the class target still feeding the ticket rate through `bits`;
shipping Decision 1 alone leaves pwu deriving from a target that no longer moves with the lane. The
`DerivedV1` equality check in admission reads both, so the two must move in one activation.

## 5. Decision 3 — A panel seat must be able to run the class it judges

**What is true today.** `derive_panel_v2_with_maturity` draws seats by bond ticket, excluding the
executor's bond, operator and key, and filtering on collateral and maturity. It does **not** filter
on whether the drawn bond can execute the class under judgement — `PalwBondStateV2` carries no
capability declaration at all (`pubkey`, `operator_id`, `collateral`, `slashed`, `status`,
`registered_daa`, `payout_payload`, and nothing else).

The V1 job panel has exactly this filter and states the rule the V2 draw is missing: a bond with no
capability declaration is **excluded, never defaulted**, because "a validator that never declared
one cannot be assigned to replay a class it may not have, and assigning it anyway would manufacture
no-shows against honest operators."

**Why it bites now and did not before.** While one floor class held all the weight, every seat could
run every class by construction. ADR-0068 gives the model tiers 97.8% of cadence, and a 33 GiB
artifact is not something a seat holds by default — so a panel drawn blind to capability seats
validators who can only abstain, and a claim that cannot reach quorum voids. `palw_unavailable_abstains`
turns that into an abstention rather than a false conviction, which is correct and is not a
substitute: an abstaining panel still fails to license the claim.

**Decision.** Give `PalwBondStateV2` a declared capability set — the class ids whose artifacts the
operator has staked collateral on being able to run — set at registration and amendable by a
lifecycle object, and filter the V2 claim-lane draw on it exactly as the V1 job panel filters on
`runtime_class_id`. Undeclared is excluded, never defaulted.

This is a state-schema change, so it carries the usual freight: a registration object field, a
lifecycle amendment path, the genesis registry, carriage, IBD and pruning round-trips, and an
activation. It is named here rather than patched because a capability the chain does not record
cannot be filtered on, and inventing the record is a design decision about who attests to holding
an artifact and what it costs to lie.

**What it must not become.** A capability declaration is a claim, not a proof; the thing that makes
it expensive to lie is that a declared seat which cannot serve is a seat that gets convicted. The
declaration therefore has to be *binding on the declarer*, or it is a permission list with extra
steps — and a permission list is the central party ADR-0067 exists to remove.

## 6. Considered and rejected

* **Leave the attempt lane on `bits` because its weight is already constant.** Rejected: the premise
  the audit was run against is about the chain's *price*, not only its fork choice, and admission
  reads the difficulty the window produces. "The coupling is unreachable from fork choice" is a
  statement about today's lane set, which ADR-0066 already changed once.
* **Solve the ticket problem by lowering `NONCES_PER_TEMPLATE`.** Rejected: that is a node-local
  constant in `kaspad`, so it binds honest producers and nobody else. The bound has to be in the
  anchor, which is consensus.
* **Filter the panel by asking nodes at draw time whether they hold the artifact.** Rejected: the
  draw must be a pure function of chain state at the anchor, or two nodes seat different panels for
  one claim.
* **Ship Decision 3 as a node-local preference in the producer.** Rejected for the same reason —
  and because the duty accounting charges exactly the seats the consensus draw names, so a
  node-local filter changes who shows up without changing who is blamed.

## 7. Invariants to verify at each step

1. **No V2 header's price is read from `bits`.** A test that mutates `bits` on an otherwise valid
   attempt header and asserts the admitted set is unchanged.
2. **The retarget sees a shortfall it caused.** A class with a nonzero share that produces nothing
   across a span must ease; a class with a share too small to round up to one block must not.
   Asserted as a difference, so a collapsed expectation cannot pass both.
3. **One inference covers exactly `2^k` nonces.** Two attempts whose nonces differ above bit `k`
   must have different job anchors; two differing below it must share one.
4. **pwu is difficulty-free.** `palw_pwu_v1`'s output is invariant under the class target.
5. **An undeclared bond is never seated**, and a declared-but-silent seat is convicted rather than
   ignored — the property that makes the declaration binding.
6. **Activation, not re-genesis.** Each Decision moves `palw_ruleset_id_v2`; none moves a genesis
   hash.

## What landed

Nothing from §3-§5 — this is PROPOSED. §2 is the part of the same audit that shipped, listed so the
two are not confused.
