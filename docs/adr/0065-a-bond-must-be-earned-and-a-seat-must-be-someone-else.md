# ADR-0065 — A bond must be earned, and a failure is not a verdict

*(Titled "…and a seat must be someone else" when drafted. D3 was withdrawn against the code — the
seat draw already dedups by operator, and "someone else" turned out to be uncheckable. The title now
names what survived.)*

Status: **D1 and D4 LANDED (dormant); D2 restated; D3 and D5 decided** (2026-08-30). Closes the
CRITICAL recorded in the two addenda to `docs/palw-mainnet-audit-2026-08-30.md`. **Mainnet
blocker.** Consensus-affecting, but **no re-mint**: both landed rules sit behind top-level
`ForkActivation` fences, `None` on every preset, so no shipped fingerprint moves and either can be
armed by rolling deploy. The earlier "fingerprint move and a re-mint on every network" in
§Consequences was written before the placement was worked out and is corrected there.

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

**D2 — ~~Frontier provenance.~~ UNIMPLEMENTABLE AS WRITTEN. Restated below.**

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
* **D2b — a single-chain anchor that is not the DAA.** The property D2 actually wanted is *the bond
  was visible before this branch diverged*, and the only branch-local quantity an attacker cannot
  advance privately is the frontier itself. "A bond may be seated only if it was registered at or
  before the branch's own safe frontier" is that property, inside the pure fold. It is **not
  implementable without new state**: `safe_frontier_blue_score` is a blue score and `registered_daa`
  is a DAA score, and comparing them is unsound. Recording either a registration blue score on the
  bond or a DAA on the frontier changes the state root, which is the re-mint D1 and D4 were
  deliberately shaped to avoid. **Left open with the cost stated rather than half-built.**

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

## Two operational rules that fall out, worth keeping even after the code is fixed

1. **A conviction verdict must log its reason.** The panel files `Unavailable` with the claim id and
   nothing else, which is why a third of the chain's claims were being convicted unnoticed. A verdict
   against another party should never be cheaper to emit than to explain.
2. **No single host may hold quorum-many seats**, independent of any rule change.
