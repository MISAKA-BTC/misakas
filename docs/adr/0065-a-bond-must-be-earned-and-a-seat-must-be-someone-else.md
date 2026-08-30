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

**Liveness side, measured on testnet-11.** `derive_panel_v2` draws seats per **bond**, not per
operator (`min_active_validators` was fixed to dedup by `validator_pubkey_hash`; the panel draw
never was). The shipped panel is `seat_count = 5, quorum = 3`, and **one host runs three seats** —
exactly quorum. 443 of ~1,265 claims (35 %) ended in `ProducerDefaulted`, each voiding ≈2,756 MSK.

## What the live chain taught that the code review could not

`Unavailable` **is not an abstention — it is a conviction.** `palw_panel_v2`'s own header: *the two
quorums license OPPOSITE transitions.* A `Valid` quorum licenses `ReceiptLicensed`; an
`Unavailable` quorum licenses `ProducerDefaulted`.

And availability fails **per claim, not per seat**: when a seat cannot evaluate a claim, every seat
in the same situation fails on the same claims. On testnet-11 the producer loads three class
artifacts and four of the five seats hold only two, so a whole class's claims are unevaluable to
everyone except the producer's co-located panel. The three seats on one host vote `Unavailable`
together, reach quorum unaided, and convict a producer that is serving correctly.

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
* D3 raises the operator floor: a panel of 5 distinct operators plus a producer needs 6 real
  operators, which is what `derive_panel_v2` already refuses to fake. That is a liveness cost paid
  deliberately, and it is the same cost ADR-0064 names for a recovered chain.

## Two operational rules that fall out, worth keeping even after the code is fixed

1. **A conviction verdict must log its reason.** The panel files `Unavailable` with the claim id and
   nothing else, which is why a third of the chain's claims were being convicted unnoticed. A verdict
   against another party should never be cheaper to emit than to explain.
2. **No single host may hold quorum-many seats**, independent of any rule change.
