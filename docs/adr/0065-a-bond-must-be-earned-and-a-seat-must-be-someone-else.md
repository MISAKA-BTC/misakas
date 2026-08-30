# ADR-0065 — A bond must be earned, and a failure is not a verdict

*(Titled "…and a seat must be someone else" when drafted. D3 was withdrawn against the code — the
seat draw already dedups by operator, and "someone else" turned out to be uncheckable. The title now
names what survived.)*

Status: **Proposed** (2026-08-30). Closes the CRITICAL recorded in the two addenda to
`docs/palw-mainnet-audit-2026-08-30.md`. **Mainnet blocker.** Consensus-affecting: fingerprint move
and a re-mint.

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

**D1 — Seat maturity.** A bond may be drawn for a panel only if `daa - registered_daa ≥
bond_maturity_daa`. This is the rule `registered_daa` was evidently recorded for; the field already
exists, so it adds no state.

**D2 — Frontier provenance.** A `safe_frontier` advance requires receipts whose panels were drawn
from bonds mature **relative to the fork point**, not merely relative to the branch's own tip.
Without this, D1 is defeated by rooting the fork later.

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

**D4 — `Unavailable` must be evidence of a refusal, not of a failure.** A seat that cannot evaluate
a claim for reasons on its own side (no artifact for the class, no fetch) must **abstain**, not
convict. Only a seat that reached the producer and was refused may file `Unavailable`. Minimum
viable form: a seat must demonstrate it *could* have evaluated the class — it holds the class
artifact root — before its `Unavailable` counts toward quorum.

**D5 — Decide whether the registry is append-only.** `write_bond(key, None)` having no callers is
either a leak or a deliberate choice, and it is undocumented. "Permanent" is load-bearing in the
attack, so it must be a decision, not an accident.

**D6 — Re-pricing alone is not a remedy.** Raising `min_collateral_sompi` does not close this:
permanence (D5) and the absence of maturity (D1) are what make one purchase a standing, retroactive
option. Re-price if desired, but never in place of D1–D4.

## Consequences

* Fingerprint move and a re-mint on every network. Stage with ADR-0064 if that ships in the same
  cycle, so operators re-mint once.
* **Do not raise `min_slash_permille_of_escrow` from 0 before D4.** The audit already flagged
  admission item 9; the measured default rate adds the larger reason — enabling slashing over this
  rate would slash honest bonds at roughly one claim in three.
* **D1 has a liveness price that must be paid deliberately.** `derive_panel_v2` refuses a short draw
  (`InsufficientEligibleBonds`, `palw_panel_v2.rs:229`), so raising the bar for eligibility can stop
  every claim binding. On a chain with barely more eligible bonds than `seat_count`, a maturity
  window means **no panel can be drawn until the window elapses** — and a freshly re-minted network
  starts with exactly that shortage. D1 must therefore measure maturity from a base the genesis
  bonds already satisfy, or ship dormant behind a fence armed after the network has bonds to spare.
* **D4 is the one that stops present harm**, and unlike D1 and D2 it is not defeated by a free
  identity namespace. It should ship first.

## Two operational rules that fall out, worth keeping even after the code is fixed

1. **A conviction verdict must log its reason.** The panel files `Unavailable` with the claim id and
   nothing else, which is why a third of the chain's claims were being convicted unnoticed. A verdict
   against another party should never be cheaper to emit than to explain.
2. **No single host may hold quorum-many seats**, independent of any rule change.
