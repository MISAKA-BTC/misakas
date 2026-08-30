# ADR-0060: The liveness doctrine — time is permissionless, weight is bonded, finality is an overlay

- Status: Accepted as doctrine; **implemented 2026-08-30 and then SHIPPED OFF the same day** —
  the mainnet audit (`docs/palw-mainnet-audit-2026-08-30.md`) found four structural defects in
  Decisions 1–2 and one in Decision 4. `PALW_HEARTBEAT_LANE_ENABLED = false`,
  `inactivity_leak_daa: u64::MAX` on every preset. §12 records what must change before either
  turns on. Decisions 3 and 5 are unaffected and live. Originally: **implemented 2026-08-30** (`palw-genesis-10b-cap` branch, same re-mint
  train as ADR-0059) — see §11 for what landed and where the implementation amended this text.
  The one operator-tunable decision is the finality leak's time constant (§6).
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
produce block 1 (this is why ADR-0059 still ships genesis seats — six then, eight since
2026-08-31, so that ADR-0065 D1 can be armed).

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
3. **Cadence-capped.** Nominal 33‰ of cadence (one block per hour at the 120 s cadence,
   ≈ 24/day). *Amended at implementation:* the cap is NOT a share-table entry — the share
   table is class-granted chain state and the heartbeat is deliberately not a class — but a
   **slot rule** (at most one heartbeat per interval, measured against the POV's youngest
   heartbeat in chain order) plus the lane's **own windowed retarget** toward one block per
   interval, floored at ~2²⁴ hashes so sibling spam is never free. The slot rule is the hard
   cap; the retarget is the price.
4. **Fee-only.** No subsidy, no worker/escrow split (there is no claim to escrow against).
   The 25B supply of ADR-0059 is untouched to the sompi. Running a heartbeat miner costs one
   CPU thread; it is a public good in calm weather and self-rewarding in a crisis, when every
   queued transaction's fee rides it.

Heartbeat blocks are otherwise ordinary blocks: they carry transactions — which is the entire
point, because bond registration, unbond, collateral funding and lifecycle transactions are
what ride them when no bonded lane is alive.

### What implementation added to Decision 1

* **A heartbeat chain block commits the parent PALW state root** like the bonded lanes
  (`palw_state_root` hash-visible on algo-3; every historical algo-3 header carries the zero
  root, so no identity moved): the doctrine's own regime is days of heartbeat-only chain,
  which must not be days of uncommitted state. Closing that surfaced a pre-existing gap —
  nothing refused a stuffed `palw_state_root` on any non-committing lane (hash-invisible
  bytes = block-identity malleability, the `NonPalwHeaderCarriesPalwCommitment` class through
  the sibling field) — now refused at the door (`UncommittedPalwStateRoot`).
* **The evidence source is a chain-order walk, not the difficulty window.** The integration
  test caught the slot rule waving a second heartbeat through: the difficulty window is
  SAMPLED, so the newest blocks — exactly what the slot rule is about — can be absent from
  it. The rules read a bounded selected-parent-chain walk (`processes::heartbeat_evidence`),
  which also makes back-dating a heartbeat useless: the youngest heartbeat is the nearest by
  chain distance, whatever timestamp it stamped.
* **The miner is one flag**: `kaspad --palw-heartbeat-miner-address=<ML-DSA-87 addr>` runs
  the bondless in-node miner (template → `heartbeat_adapt_block_template` → slot wait →
  one-thread grind → submit). Fee-only, gated to ConsensusV2 networks.

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

*As implemented:* `DnsParams.inactivity_leak_daa` (in every preset's fingerprint) drives
`InactivityLeakViewV1`, which filters BOTH quorum denominators
(`total_active_stake_by_epoch`, `total_voting_weight_by_epoch`), wired through every tally
site (live, shadow, precommit duty, branch score). The leak's evidence is the same
signature-verified contribution set the numerator aggregates — "counted as present" and
"counted as voting" are one fact — and the baseline is the later of the last attestation and
each bond's activation, so a freshly bonded validator is in grace, never leaked on arrival.
Shipped values: testnet lineage 5,040 DAA (~7 days at 120 s), mainnet 6,048,000 (~7 days at
10 bps), devnet/simnet `u64::MAX` (off — drills assume a frozen set).

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

| gate | status (verified 2026-08-30) |
|---|---|
| DNS reorg veto | TTL landed (`dns_veto_ttl_daa_score`; the t10 wedge-release retrofit) |
| chain-participation switch counter | **root cause closed** (`e05a8699`, in this branch's ancestry: a refused switch no longer feeds the counter that refused it) and the `--clear-quarantine` operator override works. Residual: no automatic decay — a documented exception, not a gap: post-fix the counter counts only REAL adopted switches, auto-release would re-admit genuine flapping, and the override is the deliberate escape. |
| pruned-IBD panic → quarantine | newcomer-path fixes in ancestry (`fix/newcomer-join-panic`, `fix/newcomer-bond-path` merges) |
| producer hold (sweep cursor) | hold reasons are logged with their diagnosing numbers (throttled, never silent); the holds are economic states that decay on chain progress — which Decision 1 makes unconditional |
| peer/mempool bans | already TTL'd (upstream behaviour) |

New gates inherit the rule at review time: *state refusal ⇒ decay + reason, or it does not
merge.*

## 8. What this deliberately does not decide

* **Removing the genesis bond seats** and **right-sizing the genesis collateral** — *decided
  the same day as ADR-0061*: the gate now admits any registry size down to zero (Decision 1's
  heartbeat carries the first bond registrations), and the collateral outputs are 10,000 MSK
  per seat against the 3,223.07 MSK derived demand.
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

## 11. Implementation record (2026-08-30)

All three stages landed together on the ADR-0059 re-mint train rather than serially — the
fingerprint was moving anyway, and one coordinated wipe is cheaper than three:

* **D1 + D2**: `palw_heartbeat_v1` (the pure rules), `processes::heartbeat_evidence` (the
  chain-order walk), acceptance via `accepts_algo_id`, the lane's bits + slot in header
  validation, ε in GHOSTDAG at every proof level, zero-subsidy enforcement in the body rule
  AND the expected-coinbase (`own_subsidy` threaded through `expected_coinbase_transaction`),
  `heartbeat_adapt_block_template` (consensus API + session delegate) and the
  `--palw-heartbeat-miner-address` in-node miner. Declared by `PALW_STATE_V2_VERSION`
  12 → 13.
* **D4**: `DnsParams.inactivity_leak_daa` + `InactivityLeakViewV1` as described in §6.
* **D5**: inventory verified (§7 table); the `palw_state_root` malleability gap found and
  closed in passing.
* **Tests**: unit (ladder, slot, retarget, floor, ε, leak denominators, evidence builder) and
  a pipeline integration test (`palw_heartbeat_blocks_tick_the_clock_and_weigh_epsilon`) that
  mints, folds, weighs (ε exactly), slot-refuses, ramp-admits and subsidy-refuses heartbeat
  blocks through the real block pipeline — the test that caught the sampled-window defect
  before it shipped.
* All five preset fingerprints re-pinned (t11: `4486d9b1…`); golden state-root vectors moved
  to version 13.

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


## 12. What the audit changed (2026-08-30, same day)

The doctrine in §2 stands. This implementation of it does not, and the failures are worth as much
as the design was:

* **D1/D2 (heartbeat lane) — OFF.** It could price the bonded lane off its own chain permanently
  (heartbeat `bits` poison the global difficulty window: 0 bonded + 263 heartbeat rows demands
  33,554,432 work where the ambient V2 target demands 2); ε = 1 is half a bonded block rather than
  a millionth, because a V2 block's work IS 2; the slot rule bounds the chain but not the DAG, and
  the retarget structurally cannot rise above its floor; and the evidence walk terminated on row
  count rather than depth, so archival and pruned nodes could reject each other's blocks. The
  first of those is the doctrine's own failure mode, reintroduced by the doctrine's own remedy —
  which is the strongest argument in this file for writing the failure down instead of the intent.
* **D4 (inactivity leak) — DORMANT.** The evidence cannot express the rule: the attestation walk
  spans ~150 s on mainnet against a declared 7 days, so absence from a two-minute window read as
  seven days of silence. Wired into the branch comparison it let a candidate branch write its own
  denominator — a 2-of-12 branch scoring full credit where it used to score zero. It is now
  dormant, structurally excluded from the branch comparison, and correctly classified as a
  duration rather than an activation fence (as a fence its value was normalised away, so two
  builds disagreeing about the denominator would have peered).
* **D3, D5 — unaffected.** Producer self-healing and the refusal-decay inventory stand as landed.

**Consequence for ADR-0061.** A zero-seat genesis is still a valid genesis, but its BOOTSTRAP —
"heartbeat blocks carry the first bond registrations" — waits on the redesign above. Until then a
zero-seat network can be minted and cannot make its first block.

**What a correct heartbeat lane needs:** its price out of `header.bits` (the receipt lane's ticket
is the existing pattern), a work basis that is not the shared blue-work scale, DAG-wide slot
evidence, and a depth bound tied to `pruning_depth`.
