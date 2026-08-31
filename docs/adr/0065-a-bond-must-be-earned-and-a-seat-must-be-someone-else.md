# ADR-0065 — A bond must be earned, and a failure is not a verdict

*(Titled "…and a seat must be someone else" when drafted. D3 was withdrawn against the code — the
seat draw already dedups by operator, and "someone else" turned out to be uncheckable. The title now
names what survived.)*

Status: **D1, D2 and D4 LANDED (all dormant); D3 and D5 decided** (2026-08-30). **D1 made
ARMABLE 2026-08-31** — see the correction below. Closes the CRITICAL recorded in the two addenda to
`docs/palw-mainnet-audit-2026-08-30.md`. **Mainnet blocker.** Consensus-affecting, but **no
re-mint for the RULES**: both landed rules sit behind top-level `ForkActivation` fences, `None` on
every preset, so no shipped fingerprint moves and either can be armed by rolling deploy. The earlier
"fingerprint move and a re-mint on every network" in §Consequences was written before the placement
was worked out and is corrected there. **Growing the registry so D1 can be armed at all is a
separate change and it IS a re-mint** — testnet-11 Relaunch 4, below.

## The single root

Three facts about a bond, all verified in the tree:

* registration gates on `min_collateral_sompi` alone — **400,000 sompi (0.004 MSK)**, refundable;
* **`write_bond(key, None)` has no callers** — a bond never leaves the registry, retired or not;
* **`registered_daa` is written and read by no consensus gate anywhere** — no maturity, no soak.

So a bond is cheap, permanent and instantly usable. Everything below is that one fact, seen from
two sides.

**Safety side.** A holder of one bond can fork from any point at or after registering it, fold
sybil `BondRegistered` objects into the fork's own blocks, seat panels from them, self-license and
grow `safe_frontier` privately. `palw_fork_choice`'s stated invariant — *a fork nobody could see
collects no receipts, so it has no frontier* — is already false.

**Liveness side, measured on testnet-11.** The shipped panel is `seat_count = 5, quorum = 3`, and
**one host runs three seats** — exactly quorum. 443 of ~1,265 claims (35 %) ended in
`ProducerDefaulted`, each voiding ≈2,756 MSK.

That host holds three seats *legitimately*: three bonds, three distinct `operator_id`s, three
distinct bond keys. The draw is working as designed (see D3). Cheap, permanent, instantly-usable
bonds plus a free identity namespace mean **quorum is something one party can simply hold**.

## What the live chain taught that the code review could not

`Unavailable` **is not an abstention — it is a conviction.** `palw_panel_v2`'s own header: *the two
quorums license OPPOSITE transitions.* A `Valid` quorum licenses `ReceiptLicensed`; an
`Unavailable` quorum licenses `ProducerDefaulted`.

And availability fails **per claim, not per seat**: when a seat cannot obtain a claim's material,
every seat in the same situation fails on the same claims. The three seats on one host vote
`Unavailable` together, reach quorum unaided, and convict a producer that is serving correctly.

The mechanism was measured, and the first diagnosis was wrong in a way worth keeping. It looked like
a configuration gap — the producer loaded three class artifacts, host C's seats held two. That gap
was real and was closed; **the convictions continued** at ~30 % from every remote seat while the
producer's co-located panel stayed at 0 %. The cause is the **material transport**: a seat with no
material issues a gossip pull (`request_palw_material`, one per 25 DAA), waits half the receipt
window, and signs `Unavailable`. The co-located panel never needs the pull, which is exactly why it
never accuses. Roughly a third of pulls do not deliver, and **neither side logs a send, an answer, or
a timeout**.

So the conviction rate is a measurement of relay loss wearing a fraud verdict's clothes — and the
config fix not stopping it is the evidence that this is structural rather than operational.

**The general shape: a verifier's own missing dependency is submitted as evidence against the
accused.** Correlated failure plus a positive-conviction verdict plus co-located quorum is enough to
convict without a single dishonest actor — which is exactly the property a court must not have.

## Decisions

**D1 — Seat maturity. LANDED, dormant.** A bond may be drawn for a panel only if it was registered
at or before `anchor_daa - bond_maturity_daa`. This is the rule `registered_daa` was evidently
recorded for; the field already exists, so it adds no state.

Three things the implementation had to decide, none of which this ADR anticipated:

* **The clock is the claim's ANCHOR, not the binding block.** A panel is accepted only if it equals
  the derived one exactly, so the draw must stay a pure function of the claim. Resolving the fence
  at the binding block's DAA would make the derived panel change from block to block around the
  fence height, and a `PanelBound` that missed the block it was assembled for would be refused as a
  mismatch rather than accepted late. The subtraction lives in one function
  (`palw_seat_maturity_floor_v1`) that both the assembler and the acceptance layer call — two
  subtractions that could differ by one is a node proposing panels its own peers refuse.
* **The parameter is `Option<{ForkActivation, u64}>` at TOP LEVEL of `Params`, and the window is
  never visited by `for_each_fence`.** The bundle was the obvious home and is the wrong one:
  `palw_ruleset_id_v2` is a bare borsh hash over the whole bundle and `for_each_fence` never
  descends into it, so a bundle field moves `consensus_identity_id` and every old/new pair fails
  the handshake — a deploy-day partition on testnet-11, the only shipped preset carrying a bundle.
  And within the top-level field, only the activation may be visited: `consensus_identity_id`
  normalises every visited value to `0` or `u64::MAX`, so a visited *duration* would make two
  builds shipping different windows fingerprint identically, peer, and then seat different panels.
  That is `inactivity_leak_daa`'s recorded failure, and the window is hashed raw into
  `consensus_params_id` instead.
* **The liveness trap below is now refused, not documented.** See the Consequences entry.

**D2 — ~~Frontier provenance.~~ Unimplementable as written; RESTATED AND LANDED as a
comparison-site rule (dormant).**

The sentence was: *a `safe_frontier` advance requires receipts whose panels were drawn from bonds
mature relative to the fork point, not merely relative to the branch's own tip; without this, D1 is
defeated by rooting the fork later.* Two things are wrong with it.

**Wrong 1 — the rule cannot live where it was put.** `safe_frontier` is written in the pure fold
(`apply_palw_transition_v4` step 5, `palw_state_v2.rs:3670/3679`; the only other writer,
`:5557-5558`, replays the fold's own recorded pair when walking a chain path). The fold's entire
input is `(parent, params, admission, ctx, objects, own work, merged work)` — one chain, no second
branch, no ancestor. And the result is hashed into `state_root`, which the next block's header pins
and every node re-checks. So a frontier whose value depended on a fork point would depend on which
competing branch a node happens to hold, and two nodes would compute different roots for the same
block. **D2 as written is a chain split, not merely awkward.**

**Wrong 2 — the causal claim is false.** `registered_daa` is written from `ctx.daa_score` at the
fold and the comparison DAA is the branch's own; both are branch-local, so the fork point never
enters either side. D1 is not defeated by *rooting the fork later* — it is defeated by *extending
the fork by `bond_maturity_daa`*, which is work the attacker has to do. That is the cost D1 was
supposed to impose, and it does impose it.

**The restatement, in two parts, because they cover different attacks.**

* **D2a — a comparison-site rule, at the one site that has a fork point.** `dns_reorg_outcome`
  (`processor.rs:8784`) holds both the candidate and `prev_sink` in one consensus instance and the
  file already has an O(log) selected-chain ancestor finder it uses on the DNS path
  (`chain_common_ancestor_within`, `:8424`). A deep reorg may be refused when the challenger's
  frontier advance rests on bonds registered after the common ancestor. **This does not cover
  IBD**: the IBD commit site compares two independent consensus instances
  (`protocol/flows/src/ibd/flow.rs:1912-1913`) with no shared reachability, so there is no ancestor
  to compute. Say so in whatever ships it; a gate that silently covers one of two entry points is
  worse than one that admits its scope.
* **D2b — ~~a single-chain anchor that is not the DAA.~~ Withdrawn: D2a needs no anchor.** The
  worry was that "registered after the fork point" required comparing a blue score to a DAA score,
  which is unsound, and that fixing it meant new state and a re-mint. **It requires neither.** The
  bond registry is append-only (D5), so along any chain bonds only accumulate, and therefore
  *registered after the fork point* is exactly *present in the challenger's registry and absent
  from the ancestor's* — a set difference between two materialized states. No units are compared,
  `registered_daa` is never read, and no state changes. That is what D2a implements.

**What landed for D2a**, behind a top-level bare `Option<ForkActivation>` (`None` on every preset):
at the deep-reorg gate, when the challenger's registry holds bonds the ancestor's does not and a
panel that bound on the challenger's branch seated a quorum of them, the reorg is refused
(`FrontierProvenanceViolation`). Panels are read off the DELTAS rather than the challenger's tip,
because `retire_claim` deletes a claim and its panel while `safe_frontier` never retreats — so the
frontier can stand on a panel the tip no longer holds.

The fast path is a proof rather than a shortcut: a panel holds at most `minted` new seats, so a
quorum of them needs `minted >= quorum`, and the threshold used is the strictly smaller
`seat_count - quorum` that the majority invariant implies. It therefore scans sometimes when it
need not and never skips when it must, checked exhaustively over every legal panel shape.

**Three abstentions, deliberate.** No fork point within the horizon, a state this node cannot
materialize, or a missing delta all pass with a warning rather than refusing. A veto that cannot
name a fork point is the permanent-partition shape the gate beside it already learned to escape.

**And the threshold is derived, not configured.** A value that sits beside a fence is normalised
out of `consensus_identity_id`, so two builds scheduling one height with different thresholds would
peer and then disagree — the hazard D1 carries permanently. A bare fence has nothing to coordinate.

Note also that `PalwCandidateOrderV1` is four scalars (`palw_fork_choice.rs:38-53`), so any
provenance a comparison-site rule wants has to widen that seam or be computed beside it.

**And with D1 armed, most of what D2 was reaching for is already paid for.** A fork that mints its
own sybil bonds cannot seat them until the fork itself has advanced `bond_maturity_daa` — that is
the cost, and it is imposed by a single-chain rule with no fork point in it. What D2 leaves over is
the bond **pre-registered on the honest chain** and held as a standing, retroactive option: it is
mature at the fork point, so it is mature relative to it, and D2 *as written* would not have caught
it either. That residual is a **pricing** question (D6 and the collateral floor), not a provenance
one — which is worth saying plainly, because the two were run together in the original draft and
the provenance half is the expensive one.

**D3 — ~~A seat is a distinct operator.~~ WITHDRAWN: already implemented, and it does not deliver what
the title claims.** Correcting this ADR against the code before implementing it:

* `derive_panel_v2` **already draws one seat per operator** — `palw_panel_v2.rs:216-228` tickets every
  eligible bond, sorts, and skips any bond whose `operator_id` is already seated.
* Registration **already enforces one key, one bond** — `palw_state_v2.rs:4632` refuses a
  `DuplicateBondKey`, and its comment (audit M2-19) names this exact attack: *"splitting collateral
  across N registrations of the SAME key manufactured N independent seat tickets, and a 3-of-5 panel
  is a permanent quorum for whoever holds three of them."*

So the dedup this ADR asked for is not missing. **The property it wanted is not obtainable by dedup at
all**, and that is the finding: `operator_id` is `palw_operator_id_v2(operator_pubkey)`
(`palw_state_v2.rs:4642`) over a key the registrant chooses freely and separately from the bond key.
One party mints as many operator identities as it wants. Host C demonstrates it — three seats, three
bonds, three operator keys, one machine, all perfectly within the rules.

**"Distinct operator" is not a checkable predicate**, for the same reason silence is not one
(ADR-0064 Fact A): nothing on-chain distinguishes two identities from two people. Deduping harder
cannot fix a namespace that is free to enter. The only real levers are the ones that cost something
(D1, and the collateral floor) and the one that stops a failure being read as guilt (D4). An ADR that
demands an uncheckable property produces exactly the kind of gate that looks like a defence and
enforces nothing — which is the failure this whole audit line keeps finding.

**D4 — `Unavailable` must be evidence of a refusal, not of a failure. LANDED, dormant.** A seat that
cannot evaluate a claim for reasons on its own side must **abstain**, not convict.

**The "minimum viable form" proposed here — a seat demonstrates it holds the class artifact root —
is unsound, for the same reason D3 was withdrawn.** `artifact_root` is a public consensus field
(`palw_state_v2.rs:1116/1667`, and it is served over public RPC), so a seat holding nothing presents
the same 64 bytes as one holding the model. Worse, the honest-node half of that test already ships:
`kaspad/src/palw_panel.rs` answers `Incapable` when the class backend does not resolve, and the
measured convictions come from seats that DO hold the class and did not receive bytes.

**And no positive proof of refusal is constructible in this tree.** The material pull carries no
seat identity, nonce or signature; the serve carries no producer signature; the serve path returns
a silent `None` on four separate conditions including a node-global 10-second per-claim throttle;
and the producer broadcasts its material exactly once, immediately before submitting its block —
the "periodically while its claim is unresolved" in the flow context has no implementation. There
is nothing for a rule to check.

**So what landed is the weakening, and it is the honest one.** Past
`Params::palw_unavailable_abstains`, an `Unavailable` counts toward **neither** quorum and is
**never charged as a dissent** — the treatment `Incapable` already gets. The claim then falls to
the redraw-then-`ReceiptTimeout` path, which voids the escrow and slashes nobody. The verdict is
KEPT rather than deleted: a seat must still have a way to say it got nothing, and pushing those
seats into silence would trade a fact the chain can see for one it provably cannot.

D4 does **not** reduce to "route more cases into `Incapable`". `palw_seat_may_plead_incapable_v2`
refuses that plea for the base class — the class producing most testnet-11 claims — and the refusal
poisons the whole receipt set rather than dropping the one receipt.

Two consequences of the landed shape worth stating plainly:

* **`ProducerDefaulted` becomes unlicensable past the fence**, because the `Unavailable` quorum is
  its only constructor. The acceptance cross-check refuses it automatically once the tally can no
  longer return `ProducerUnavailable`, and the transition arm refuses it too rather than trusting
  that filter. A producer that genuinely withholds still gets nothing — no `Final`, no payout, its
  own escrow destroyed at `ReceiptTimeout` — so withholding remains self-punishing. What is lost is
  the *slash*, and provable withholding belongs in the DA court (ADR-0062), where the demand is on
  chain and absence cannot be forged by presence.
* **The fence is top-level and not in the bundle**, and here that is load-bearing rather than tidy:
  this rule has to reach a network that is convicting honest producers *now*, by rolling deploy. A
  bundle placement would refuse every old/new handshake instead of peering with a warning.

**D5 — The registry IS append-only, and that is now a decision.** Verified: `write_bond`'s `None`
branch has no callers anywhere — every call passes `Some`. It is deliberate and it must stay. A
retirement is a *status transition*, not a deletion (`Active → Retiring { since_daa }`), and the
record is the only thing that proves the withdrawal delay has elapsed
(`palw_bond_collateral_is_locked_v2` reads `since_daa`). Removing the row would make a retired bond
indistinguishable from one that never existed, which is exactly what the retirement path must not
do.

So "permanent" in the attack means the *registration* is permanent — not that a retired bond keeps
its powers. `palw_bond_may_take_work_v2` requires `Active`, so a `Retiring` bond takes no seats and
no work. The attacker's advantage is that it need never retire, and what prices that is D1 and the
collateral floor, not deletion.

**D6 — Re-pricing alone is not a remedy.** Raising `min_collateral_sompi` does not close this:
permanence (D5) and the absence of maturity (D1) are what make one purchase a standing, retroactive
option. Re-price if desired, but never in place of D1–D4.

## Consequences

* **No re-mint, and no shipped fingerprint moves.** Both landed rules are top-level
  `Option<ForkActivation>` fences left `None` on every preset, so an old build and a new one keep
  one `consensus_identity_id` and stay peers; arming either is a rolling deploy. The earlier
  "fingerprint move and a re-mint on every network" at the top of this section assumed a bundle
  placement, which turned out to be the thing that would have partitioned testnet-11 on deploy day.
* **`min_slash_permille_of_escrow` is NOT what makes a default cost the producer its bond — the
  earlier note here was wrong and the harm is live.** That parameter is read in exactly one place,
  admission item 9's collateral-backs-the-escrow check. The `ProducerDefaulted` arm calls
  `void_and_slash`, which is `void_claim` *followed by* `slash_bond(claim.bond, claim.reserved)`;
  its own comment says "this void takes the stake, unlike the two timeouts". So each of
  testnet-11's 443 defaults debited the producer's bond on top of voiding the escrow, and the
  licensing arm's `slash_dissenting_seats` charged every un-fed seat that filed `Unavailable` on a
  claim the panel went on to license. **Read that bond's `collateral` and `slashed` before
  anything else**: the debit is capped per event, so a long enough run drives it under
  `min_collateral_sompi`, after which `palw_bond_may_take_work_v2` stops it taking work at all —
  the chain refusing its own only producer for an offence it did not commit.
* **D1's liveness price is now refused rather than remembered.** `derive_panel_v2` fails closed on a
  short draw (`InsufficientEligibleBonds`), so a claim that cannot seat a panel never binds and
  voids at `BindTimeout`. The shipped genesis registers exactly `PALW_V2_PANEL_SEATS + 1` bonds and
  the draw excludes the executor — **zero slack** — and every genesis bond carries
  `registered_daa = genesis.daa_score`. A fence armed before its own window has elapsed therefore
  makes all of them immature simultaneously: no panel anywhere, `safe_frontier` pinned at 0,
  pruning never starting, every block's worker carve burned. This ADR's answer was "arm it after
  the network has bonds to spare", which is something an operator has to remember;
  `Params::validate_palw_v2` now refuses the configuration outright, and refuses a zero window
  besides, because a gate that cannot refuse anything is the failure this whole audit line keeps
  finding.
* **D4 is the one that stops present harm**, and unlike D1 and D2 it is not defeated by a free
  identity namespace. It should ship first.

### What an adversarial review of the landed code established, and none of it is fixable by editing D4

* **Past the fence, seat accountability is gone entirely.** `slash_dissenting_seats` has two call
  sites; the `ProducerDefaulted` one becomes unreachable and the licensing one skips `Withheld`, and
  `slash_silent_seats` was already a no-op. So no seat's receipt behaviour is punishable by
  anything. That is the honest position — this chain cannot observe silence, and it cannot tell a
  lost fetch from a refusal — but it should be stated rather than discovered: **a seat that files
  nothing, files `Unavailable` forever, or lies `Valid` pays the same, which is zero.** Seat
  accountability needs receipts that ride the chain independently of a concluding object; until one
  exists there is nothing to charge on.
* **The redraw is weaker than D4's fallback assumes, for a reason D4 cannot reach.** Two defects,
  one fixed here and one structural. Fixed: the seat service keyed its "already answered" set by
  CLAIM, so every seat that answered a first panel silently skipped the second — the redraw dealt
  new seats and then muted any that had sat before. Structural: `derive_panel_v2` re-randomises only
  the ticket ORDER (by `anchor_block`), so on a registry holding exactly `seat_count + 1` bonds —
  the shipped genesis — the second panel is the identical seat SET. **A redraw only helps a network
  with spare bonds.** With none, D4's fallback is simply the timeout, which is still the right
  outcome (escrow void, nobody slashed) but is not the second opinion the redraw was built for.
* **D1's arming guard is a genesis-only check and cannot see the live registry.** It proves the
  bonds that exist at genesis are mature when the fence fires; it cannot prove the same of a
  REPLACEMENT. On the zero-slack shipped registry, if one non-executor bond leaves eligibility — the
  operator retires it, or the escrow slash drives it under `min_collateral_sompi` — the replacement
  is unseatable for a whole `bond_maturity_daa`, and a short draw is no panel: every claim voids at
  `BindTimeout` and the frontier stops for the window's duration. Without D1 the same replacement
  restores panels in the next block. **So D1 must not be armed on a network running at
  `seat_count + 1` bonds.** This was written as an operational precondition the config check cannot
  enforce. It is now BOTH enforced and satisfied — see the correction below.
* **Arming D1 is a coordinated change, even though the handshake permits a rolling one.** Two
  operators who schedule it at the same height with different windows keep one
  `consensus_identity_id` and are told only by a warning. Deliberate — it is the standing rule for a
  value inert until its fence fires — but it means the window has to be agreed out of band.

## Two operational rules that fall out, worth keeping even after the code is fixed

1. **A conviction verdict must log its reason.** The panel files `Unavailable` with the claim id and
   nothing else, which is why a third of the chain's claims were being convicted unnoticed. A verdict
   against another party should never be cheaper to emit than to explain.
2. **No single host may hold quorum-many seats**, independent of any rule change.


## Correction, 2026-08-31 — the rule was enforceable and unarmable at the same time

The caveat above says D1 "must not be armed on a network running at `seat_count + 1` bonds" and
files that under things a config check cannot enforce. Both halves were wrong in the same direction.

**It IS enforceable.** `validate_palw_v2` reads the genesis registry — the objects are in `Params`
— so the count is knowable at boot, before a peer is dialed. The guard now refuses an armed
`palw_bond_maturity` on a bundle registering fewer than
`palw_fp_devnet_v3::palw_v2_maturity_armable_bonds_v1()` bonds, and `ConfigBuilder::build` panics on
a failing validate. An operator cannot arm the halt by remembering wrong.

**And enforcing it exposed the real defect: nothing this build shipped could satisfy it.**
`PALW_RC_GENESIS_BONDS` held exactly `seat_count + 1 = 6` cards, so the guard refused every shipped
preset. D1 was a rule that could only be obeyed by minting a new genesis — enforcement existed and
arming did not, which is the "gate that never fires" shape this ADR line keeps finding, wearing the
opposite mask: a gate that fires on everything.

**The bar, derived rather than chosen.** `seat_count + 3`:

| term | why |
|---|---|
| `seat_count` | the panel itself |
| `+1` | the executor, excluded by bond, operator and key |
| `+1` | one seat departing — the case D1's window makes expensive, since the replacement is immature for a whole window |
| `+1` | margin, so the first departure does not leave the network one retirement from a halt |

The strict minimum is `seat_count + 2`; the third is slack, and it is named as slack in the code. An
earlier derivation reached the same number by counting "the replacement's own immaturity" as a
fourth term, which double-counts: the replacement's immaturity *is* the departure's gap.

**What changed.** `PALW_RC_GENESIS_BONDS` grew from six cards to eight, with two real ML-DSA-87
key pairs generated for the purpose. This is a genesis change — two collateral outputs and two fee
floats enter the premine, two `BondRegistered` objects enter `genesis_objects` — so the premine
commitment, both genesis hashes and the testnet-11 `consensus_params_id` all move together:

* testnet-11 genesis `d2789338…` → `572f80c0…`, `utxo_commitment` `670b1125…` → `7f3142f2…`
* `consensus_params_id` `f3bf86b4…` → `4f89ec82…` — and then to `5ccdd684…` when this merged with
  the free-prompt quantum drop (1,000 → 100) on the same day. Recorded because the intermediate
  value never shipped: a fingerprint is a function of the whole ruleset, so two re-mints landing
  together produce a third figure rather than either of theirs.
* payload marker `11,3` → `11,4` ("Relaunch 4")

**testnet-11 must be re-minted and every host must wipe its datadir.** Nothing in this build accepts
the Relaunch-3 chain; an un-wiped node hits the startup genesis-mismatch guard rather than silently
resuming, which is the intended behaviour and the reason the marker is bumped.

**Two tests hold the claim**, because "armable" has two halves and the first alone is the failure
mode this ADR keeps cataloguing:

* `arming_bond_maturity_needs_a_registry_with_a_spare_seat` — the config gate accepts the fence on
  the shipped preset, refuses it on the same preset trimmed to `seat_count + 1`, and still refuses
  it one card short of the bar.
* `the_shipped_registry_draws_under_an_armed_maturity_fence_even_after_a_seat_leaves` — the shipped
  genesis objects are applied through the real transition, a claim is bound under a real card, and
  the panel draws with the floor on; then a `BondRetireRequested` removes a seat and it still draws.
  The same departure on a `seat_count + 1` registry returns `InsufficientEligibleBonds`. A fence
  that validates and then starves every draw would pass the first test and fail this one.

**What is still not enforced, stated plainly.** The guard reads the GENESIS registry, and that is
a real limitation rather than a theoretical one: a `BondRegistered` carrier IS admitted on a live
V2 chain once it locks the collateral it declares (`virtual_processor/processor.rs:4922`), so live
registries genuinely do grow. A network that grew past the bar post-genesis still cannot arm D1 if
its genesis was small; a network whose genesis was large can arm it after the live registry has
shrunk below the bar. `validate_palw_v2` has no chain access, so this is the limit of a config-time
check, not an oversight — but it means the guard is conservative in one direction and stale in the
other. Both failures land on the same runtime behaviour, which fails closed: a short draw is
`InsufficientEligibleBonds`, no panel, and the claim voids at `BindTimeout`. **The reason to ship
the registry at `seat_count + 3` anyway is that the genesis size is the only one the config gate
can see, and a halted chain produces no blocks — so no repair carrier can land on the network that
most needs one.**

**Mitigated, 2026-08-31: the stall now says what it is.** The gap above cannot be closed by a
check — `validate_palw_v2` has no chain access, and a runtime refusal would be a consensus rule
resolved from local state, which is a chain split. What was fixable is that the failure was
*silent*. When the live registry falls under the bar, `derive_panel_v2_with_maturity` returns
`InsufficientEligibleBonds`, `palw_v2_derived_panel_bindings` skips the claim with a bare
`else { continue }`, every claim voids at `BindTimeout` and `safe_frontier` stops — and no line
anywhere named the maturity fence. From the outside that is indistinguishable from a producer
outage, which is the wrong thing to go and investigate.

`VirtualStateProcessor::palw_warn_if_maturity_outruns_the_registry` now reports it. **Log only** —
no refusal, no fence, no return value, no fingerprint movement — with two severities, because the
two situations need different responses:

* **below `seat_count`** — no claim can seat a panel, whoever its executor is;
* **at exactly `seat_count`** — every claim from a still-eligible bond fails, and only a claim whose
  own bond has itself left eligibility can still bind;
* **below `palw_v2_maturity_armable_bonds_v1()`** — draws still work but the margin is gone, and the
  next retirement or slash stalls the chain for a full maturity window.

Three bands rather than two, because exactly three are provable and the middle one is where a
plausible message goes wrong. The draw's three exclusions collapse to one: `pubkey` uniqueness is
enforced at registration (`palw_state_v2.rs`, the `DuplicateBondKey` arm), so the key clause can
only match the executor's own bond, which the bond clause already matched. The exclusions therefore
remove exactly ONE operator when the executor's operator is among the eligible, and ZERO when it is
not — and it is not whenever the executor's own bond is immature, `Retiring`, or under the
collateral floor, each of which a claim can enter *after* it was created (claim creation checks
`Retiring` at admission and never re-checks, and imposes no maturity requirement at all).

So at exactly `seat_count` a panel is still drawable for such a claim, and an earlier draft of this
warning said "no panel can be drawn" over a state where one demonstrably was.
`a_claim_whose_own_operator_has_left_can_still_seat_a_panel` measures both edges of that band rather
than arguing them. The alarm threshold did not move — `seat_count + 1` is still the point where
claims from healthy bonds start failing — but a diagnostic may not claim more than its count knows.

The count comes from `palw_panel_v2::palw_seatable_operators_v1`, which applies **the same two
predicates the draw does** (`palw_bond_may_take_work_v2` and the maturity comparison) rather than
restating them — a diagnostic that could disagree with the rule it describes is the failure this
ADR line keeps cataloguing, one layer out.
`the_seatable_counter_agrees_with_the_draw_it_warns_about` holds the two together across every
maturity floor and panel size, so the counter cannot quietly stop matching. It counts distinct
OPERATORS, because the draw seats one bond per operator, and it deliberately does not apply the
draw's executor exclusions: the bar it is compared against already prices the executor as one of
its four terms, so excluding it twice would fire the warning a seat early.

It is rate-limited to once per `window_bind` — the interval over which a claim actually times out,
so the operator hears once per cohort of claims the shortfall costs rather than once per block —
and it returns before touching the header store or the registry whenever the fence is dormant,
which is every shipped preset.
