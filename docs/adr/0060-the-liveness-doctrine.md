# ADR-0060: The liveness doctrine — time is permissionless, weight is bonded, finality is an overlay

- Status: Accepted (doctrine); implementation staged — see §9. The one operator-tunable decision
  is the finality leak's time constant (§6).
- Date: 2026-08-30
- Depends on: ADR-0038 (PALW is consensus work), ADR-0045/0046 (class economy; carriage),
  ADR-0054 (share follows production; permissionless admission), ADR-0056 (share economy),
  ADR-0058 (merged work is counted), ADR-0059 (the 10B premine cap)
- Amends: ADR-0038's pure-PALW production stance and testnet-11's published "there is no hash
  lane to fall back to" doctrine — a *bounded, near-weightless* hash lane re-enters, as the
  chain's clock and nothing else.

## 1. The failure family, measured

Every liveness failure this project has actually had is one defect wearing four costumes:

* **The exposure wedge** (testnet-12, block 600): the only bonds filled their exposure
  ceilings; releasing a claim requires DAA to advance; DAA advances only when blocks are
  produced; only those bonds could produce. Held forever, by arithmetic.
* **The floor-producer stall** (testnet-11 operations): with no floor producer running, the
  DAA stops and cannot recover on its own — the chain waits for an operator.
* **The quarantine runaway** (chain-participation switch counter): a refusal gate whose
  predicate feeds itself, with no reset path — a synced node becomes permanently unable to
  participate.
* **The virtual wedge** (testnet-10, 81 hours): the DNS reorg gate refused every heavier
  candidate; nothing inside the chain could expire the refusal until a TTL was retrofitted.

And the bootstrap variant: bonds are required to produce, bond registration rides a
transaction, a transaction needs a produced block — so a chain born with zero bonds can never
produce block 1 (this is why ADR-0059 still ships six genesis seats).

The shared root: **the chain's clock — DAA advancement, timeout sweeping, gate expiry — was a
hostage of the very actors that were stuck.** Bonds can die (slashing, exhaustion, exit,
court error); a clock must not.

## 2. The doctrine

> **Time is supplied by a permissionless lane. Weight is supplied by bonds. Finality is
> supplied by validators. None of the three can stop the other two, and every refusal gate
> decays on the clock that always runs.**

Concretely: the accountability apparatus (bonds, panels, courts, quorums) exists to make
*weight and irreversibility* expensive to fake. It must never be what *time* depends on,
because accountability implies killability. A hash proof is the one kind of work that is
self-verifying — no claim, no panel, no court, therefore no bond — so it is the one lane that
may run permissionless, and the only thing it is allowed to sell is time.

## 3. Decision 1 — the heartbeat lane (new)

A bondless hash lane, re-enabling the existing BLAKE2b-512 ∥ SHA3-512 Layer-1 (`algo_id = 3`
— the lane testnet-11 deliberately switched off) under four bounds:

1. **Bondless, claimless.** Admission requires a valid hash proof and nothing else: no bond,
   no PALW claim, no escrow, no lifecycle objects. There is nothing to slash because there is
   nothing to lie about.
2. **Near-weightless.** A heartbeat block contributes a fixed constant `ε` to blue work,
   *independent of its difficulty* — an explicit, named exception to ADR-0045's DerivedV1
   work equality. Consequence: any amount of hash power loses fork choice to a single bonded
   PALW block; among heartbeat-only branches (total collapse), `ε × n` still orders longer
   chains first. An ASIC farm pointed at this lane buys cadence-capped, weightless blocks —
   i.e. nothing.
3. **Cadence-capped.** Nominal share 33‰ of cadence (one block per hour at the 120 s
   cadence, ≈ 24/day), held in the on-chain share table like any class and counted through
   the mergeset per ADR-0058 (the machinery built for slow classes is exactly what a 1/hour
   lane needs).
4. **Fee-only.** No subsidy, no worker/escrow split (there is no claim to escrow against).
   The 25B supply of ADR-0059 is untouched to the sompi. Running a heartbeat miner costs one
   CPU thread; it is a public good in calm weather and self-rewarding in a crisis, when every
   queued transaction's fee rides it.

Heartbeat blocks are otherwise ordinary blocks: they carry transactions — which is the entire
point, because bond registration, unbond, collateral funding and lifecycle transactions are
what ride them when no bonded lane is alive.

## 4. Decision 2 — the emergency ramp (new)

24 blocks/day is a heartbeat, not an ambulance. The shipped lifecycle windows sum to 6,000
DAA (600 bind + 600 receipt + 1,200 challenge + 3,000 court + 600 abandon-hold): at full
cadence that horizon sweeps in ≈ 8.3 days, but on the heartbeat alone it would take **250
days** — and the per-class epoch retarget cannot help, because an epoch is 1,000 DAA, which
at 24/day is 41 days away. The ramp therefore keys on **timestamps, not epochs**:

* The heartbeat's *target rate* is a deterministic step function of bonded-lane silence,
  measured as `now − timestamp(last block from any bonded lane)` over chain-observable data:
  nominal **1/hour**; silence > 1 h → **1/10 min**; silence > 6 h → **1/120 s** (full
  cadence). When a bonded lane produces again, the rate steps back down the same ladder.
* Inside the chosen rate, per-block difficulty follows an ASERT-style schedule on parent
  timestamps, so the lane holds its rate through hash-rate swings without waiting for an
  epoch boundary.

With the ramp, a total-collapse recovery sweeps timeouts at near-normal speed: the bind
window in ~20 hours, the full exposure horizon in ~8–9 days — instead of 25 and 250 days.

Timestamp manipulation is bounded by the existing median-time-past and future-drift rules,
and blunted by Decision 1.2: easing this lane's difficulty wins weightless blocks at a capped
rate and nothing else.

## 5. Decision 3 — producer-bond self-healing is now unconditional (mostly landed)

The producer side of the doctrine is largely built; what it lacked was the clock. On record,
as one list:

* Collateral is **derived, not declared** (`palw_v2_collateral_for_claim_lifetime_v1`, the
  "+1" lesson of the block-600 wedge), and re-derived from the dearest registered class.
* Exposure releases on timeout sweeps; **with Decision 1, sweeps run on a clock no bond can
  stop** — this is what turns "the wedged party must produce the releasing block" from a
  deadlock into a delay.
* Share follows production (ADR-0054), merged work is counted (ADR-0058), admission is
  permissionless (ADR-0054/0056), post-genesis bond registration rides an ordinary
  transaction (the carrier), and the post-genesis collateral floor is deliberately thin
  (`min_collateral_sompi` = 400,000 sompi): the economic re-entry door is always open, and
  with Decision 1 there is always a block for the re-entry transaction to ride.

No new mechanism is needed on this layer. (A collateral top-up path — extending headroom
without re-registering — remains a nice-to-have, deferred.)

## 6. Decision 4 — the finality inactivity leak (new; overlay-scoped)

Wherever the finality overlay (VLT) is enabled: a validator that has not attested for
**T_leak** — proposed at the DAA-equivalent of **7 days**, measured on the always-advancing
clock — is **excluded from the quorum denominator**. Its bond is *not* burned; it re-enters
by attesting again under the normal (re)bond rules. The remaining active set therefore
re-forms quorum on its own, and finality self-heals instead of halting until an operator
re-bonds by hand.

**The trade-off, stated plainly:** a partition longer than T_leak can produce two finalized
histories — each side leaks the other side's validators and each re-forms a local quorum.
This is the same price Ethereum's inactivity leak pays, and we accept it for the same
reason: the alternative (finality halts indefinitely on quorum loss) reproduces exactly the
manual-recovery operational pattern this doctrine exists to end. Two mitigations stand:
equivocation slashing still burns any validator that signs both sides, and T_leak is long
enough that a partition must persist for a week before uniqueness is at risk. T_leak is the
one parameter this ADR leaves to the operator.

Two clauses restated as binding, not new: the overlay **never gates production** (a finality
stall leaves Decisions 1–3 untouched), and the minority-cannot-finalize safety rule of the
overlay is unchanged — the leak shrinks the denominator slowly and deterministically; it
never lets a present minority finalize *now*.

VLT is currently dormant on testnet-11, so this decision creates no re-mint pressure; it
binds the overlay whenever and wherever it is next enabled.

## 7. Decision 5 — refusal gates decay (partially landed)

Two kinds of refusal exist, and the doctrine treats them oppositely:

* **Identity refusals** — wrong genesis, wrong consensus fingerprint, wrong ruleset — are
  *correct forever*. No TTL; these are what re-mints and version bumps are for.
* **State refusals** — quarantines, vetoes, holds, bans, candidate exclusions — every one
  MUST carry a decay measured in wall-clock or heartbeat DAA (a clock the refused party
  cannot be blamed for stalling), plus a logged reason naming the gate. A gate whose refusal
  can feed its own predicate is a wedge with a fuse of unknown length; the TTL is the fuse
  you chose on purpose.

Inventory at drafting time:

| gate | status |
|---|---|
| DNS reorg veto | TTL landed (`dns_veto_ttl_daa_score`; the t10 wedge-release retrofit) |
| chain-participation switch counter | **runaway measured, reset path absent; fix branch exists (`fix/chain-switch-counter-runaway`) — must be verified closed; as it stands it violates this doctrine** |
| pruned-IBD panic → quarantine | verify against doctrine (newcomer-path fixes landed) |
| producer hold (sweep cursor) | verify against doctrine |
| peer/mempool bans | already TTL'd (upstream behaviour) |

New gates inherit the rule at review time: *state refusal ⇒ decay + reason, or it does not
merge.*

## 8. What this deliberately does not decide

* **Removing the genesis bond seats.** Decision 1 makes a zero-seat genesis viable for the
  first time (heartbeat blocks carry the first bond registrations), which would complete
  ADR-0059's simplification: genesis = main wallet + community, nothing else. That is a
  follow-up amendment — the boot gate would become "heartbeat lane present OR ≥ 6 seats" —
  taken separately so this doctrine stays mechanism, not migration.
* **Right-sizing the genesis collateral outputs** (measured demand 3,223.07 MSK/seat against
  the 100M/seat currently locked) — separate change, same re-mint window.
* **Mainnet finality topology** — the leak's parameters for a mainnet validator set are a
  launch-time decision.

## 9. Implementation staging

1. **Heartbeat + ramp** (Decisions 1–2): consensus change — admission, per-class DAA
   exception, ε-work in GHOSTDAG accounting, fee-only coinbase path, share-table entry.
   Ships with a `PALW_STATE_V2_VERSION` bump and rides a re-mint; the ADR-0059 10B-cap
   re-mint is the natural vehicle if sequenced together.
2. **TTL audit** (Decision 5): mostly node-local; the switch-counter closure is the one
   consensus-adjacent item and is verified, not assumed.
3. **Inactivity leak** (Decision 4): overlay change, dormant until VLT is next enabled;
   carries its own version bump then.

Each stage declares itself by version bump — this project's own law: a rule change not
declared by a version bump forks the network silently.

## 10. The degradation ladder

| state | production | finality | recovery path |
|---|---|---|---|
| normal | all bonded PALW lanes + 33‰ heartbeat | overlay (where enabled) | — |
| validators dead | untouched | probabilistic only | leak re-forms quorum (≤ T_leak + margin) |
| producers dead | heartbeat, ramped to full cadence | continues | timeouts sweep ≈ normal speed → permissionless re-bond → PALW relights |
| everything dead | heartbeat alone | probabilistic only | both of the above, in order — **no re-mint, ever, for liveness** |

The honest ledger of what this costs: up to 33‰ of blocks in calm weather (and temporarily
more in a crisis) are hash work rather than useful inference — the minimal concession that
buys the property that the network survives its producers, its validators, and its own
court's mistakes.
