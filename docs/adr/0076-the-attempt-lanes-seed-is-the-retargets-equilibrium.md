# ADR-0076: The attempt lane's seed is the retarget's own equilibrium

**Status:** ACCEPTED (2026-09-02), implemented for the Relaunch 5e re-genesis (every preset's
fingerprint moves). Written against a measurement, not a design review: testnet-11 Relaunch 5d ran
its first hour with the liveness floor at 249 blocks and both model tiers at **zero**, against a
share table that allots them 489‰ each.
**Builds on:** ADR-0038 Decision D (per-class difficulty domains; wall-clock never enters fork
choice), ADR-0045 Decision 1 (`pwu_per_inference` is the canonical job's counted step leaves),
ADR-0071 Decision 1 as amended (block interval is `calculate_difficulty_bits`' job; the per-class
retarget only redistributes share), ADR-0072 (the ticket is the execution — one canonical
inference is one draw).
**Makes reachable:** ADR-0075 (a stranger's model, certified on-chain, has to be able to produce
once it is seated).

## 1. What was measured

The three classes testnet-11 registers, with the share each actually holds after the genesis
grants dilute, and the counted work of one canonical inference:

| class | share | `pwu_per_inference` | 5d seed |
|---|---|---|---|
| PALW-BASE-0 `f1c5635c…` | 22‰ | 7,708 | `MAX/2` |
| PALW-QWEN36 `5bd9ae3d…` | 489‰ | 2,685,360 | `MAX/2` |
| PALW-QWEN25-A16 `71bbb755…` | 489‰ | 1,589,424 | `MAX/2` |

One seed for all three, because the model rows were built from `(slash_value, initial_target)`
read off the floor's own registration — one tuple standing for "the floor's economics", of which
the first half really is per-network by rule (one slash value for the network, because it sets
weight-per-collateral) and the second half is per-class by construction.

With one seed, a class's block rate is its DRAW rate. The floor completes a canonical inference in
~50 ms on the fleet's x86; Qwen3.6 takes ~9 s. So the floor drew ~180 tickets for every one the
hybrid drew, at identical odds, and took ~98.5 % of the blocks — which is what the live chain
reported: 249 floor blocks, zero from either tier, in the first hour.

Predicted from the same arithmetic before it was checked: at genesis `bits` (~1/20) the three
classes offer 10.15 hits/s of which the floor is 10.0 — a 30 blocks/min start burst that is 98.5 %
floor. The chain's own first minute: 27 blocks, all floor.

## 2. Why no retarget repairs it

Both correction paths decline, each for a correct local reason:

* `retarget_over_span_v1` expects `share_c · total` and divides by observed. A class that produced
  **nothing** is skipped — silence is not evidence of trying, and easing on silence would let a
  registrant buy cadence with patience.
* `converge_idle_target_v1` (ADR-0071 Decision 1 as amended) moves an idle class toward
  `floor_price`, the hardest target any class that actually produced is paying. Seeded AT the
  floor's price, an idle tier is already there: `current_target >= floor_price` returns unchanged.

A shared seed is therefore not a slow start. It is a permanent one, and the module's own doc had
already named the shape — "a class whose target is too hard to win even one block per epoch stays
too hard forever".

## 3. Decision 1 — the seed is `share · pwu`, against one pinned scale

`retarget_over_span_v1` converges to `observed_c / total = share_c`. A class produces
`r_c · P_bits · T_c / 2^128` blocks per second, so the fixed point is `T_c ∝ share_c / r_c`.

`r_c` — inferences per second — is the one term consensus may not measure: a wall-clock second is
a fact about a host, and ADR-0038 Decision D refuses to let one reach fork choice. What consensus
holds instead is `pwu_per_inference`, the normative step-leaf count of the class's canonical job,
frozen at registration. A producer working at any fixed rate completes inferences in inverse
proportion to it, so `r_c ∝ 1/w_c` is the only hardware-free reading of the same quantity:

```text
    T_c = MAX · (share_c · w_c) / PALW_ATTEMPT_TARGET_UNIT_SHARE_PWU_V1
```

Each class is priced from **its own two numbers** against one pinned scale — never against the
other classes — so a table's seeds do not move when a class joins or leaves it, a network that
registers only the floor prices that floor exactly as a three-class network would, and a stranger
registering post-genesis is priced by the rule that priced the incumbents.

The counted-work proxy is not exact: the measured wall-clock ratio between the floor and the
hybrid is 180×, the counted-work ratio is 348×. The seed is therefore within a factor of ~1.9 of
the equilibrium, which one closed span of the retarget removes — as against the nine days of
×4-per-epoch crawling the shared seed required, and the permanent stall it actually produced.

## 4. Decision 2 — the scale is spent on headroom

Only ratios of class targets carry meaning: the aggregate interval is `bits`' job and the two
retargets are wired so they cannot fight over one cadence. The absolute scale is therefore free,
and free means it should be spent on the thing scaling can still get wrong.

A class seeded at `u128::MAX` has no headroom: no class dearer per unit of share can ever be
seeded above it, and no retarget can lift it, because a target may not exceed the ticket space.
Anchoring the table's dearest class AT the ceiling would make a network's genesis the permanent
upper bound on every model a stranger could bring later — which is the door ADR-0075 opens.

`PALW_ATTEMPT_TARGET_UNIT_SHARE_PWU_V1 = 2^31`, and what testnet-11 gets for it:

| class | `share · pwu` | 5e seed |
|---|---|---|
| PALW-QWEN36 | 1,313,141,040 | `MAX / 1.63` |
| PALW-QWEN25-A16 | 777,228,336 | `MAX / 2.76` |
| PALW-BASE-0 | 169,576 | `MAX / 12,663` |

The dearest class draws about every other ticket, the ceiling stays 1.63× away, and the floor sits
7,744× below the hybrid — the ratio their shares and their counted work require.

**Mainnet is the case this decision is really for.** Mainnet mints floor-only, and every model
class it will hold arrives afterwards by registration. A floor left at `MAX/2` leaves a factor of
two above it and no model tier could ever be seeded into room that is not there. Seeded, a
floor-only network's floor sits at `MAX/278` and the room above it is the 278× a tier needs.

## 5. Decision 3 — the seed is written at genesis assembly, against the EFFECTIVE share table

A registration's declared `share_permille` is not the share it holds: a grant funds an entrant by
scaling every incumbent, so the floor declares 1000‰ and holds 22‰, and the hybrid declares 957‰
to hold 489‰ after the dense tier dilutes it. Pricing against the declared figures is a 23× error
on the floor alone.

So the genesis assembly FOLDS its own registration list through `apply_palw_transition_v2` — the
same call `assemble_palw_rc_identity_v2` uses to prove a list applies — and seeds each row from
the table that fold produces. A second, hand-rolled spelling of the dilution would be a second
place for it to drift.

The seed lands in the `ClassRegistered` object, so it is inside `consensus_params_id`: a node
built before this change and a node built after it announce different fingerprints and refuse each
other at the handshake, rather than agreeing on a name and disagreeing about which class may
produce.

## 5b. Decision 4 — a class being SEATED is a class being priced

ADR-0075 seats a weightless class by an object: it registers holding no share, and a
`ClassLaneCertified` on the attempt lane grants it `min_grantable_share_permille` once the chain
has graded the drill that covers it. Its registration was priced for the cadence it held then —
none — so granting the share without re-seeding would leave the one number that decides how often
it may produce entirely unrelated to the share just granted, in either direction. A stranger who
declared `MAX/2` would sit thousands of times easier than the floor it is joining; one who declared
conservatively would sit locked out, with no retarget able to reach it, because an idle class only
converges toward a price a PRODUCING class pays and it produces nothing.

So the grant writes the price with the seat: `attempt_target_seed_v1(granted share, the class's own
`pwu_per_inference`)`, from the table the transition has just written. This is the clause that makes
ADR-0075's permissionless path end in a class that can actually produce, rather than one that holds
a share and no way to use it.

## 6. What this does NOT change

**Not the weight split.** Weight per second is `w_c · r_c · P_bits`, in which `T_c` cancels:
`pwu(B) = expected_attempts(T_c) · w_c` rises exactly as fast as the block count falls. A seed
moves the BLOCK COUNT split — cadence, which is what the share table allots — and leaves fork
choice where it was. `pwu` magnitude still may not be read as a cross-class price.

**Not collateral.** A claim reserves `palw_exposure_pwu_v1 = pwu_per_inference`, with no target in
it, and the genesis bind-window gate reads the same target-free quantity. `GENESIS_CLASS_TARGET` in
`palw_fp_devnet_v3` survives as what it always arithmetically was for that call — a 2× margin on
derived collateral — and is re-documented rather than moved, because moving it would re-size every
shipped registry to chase a factor that is not in the reservation it funds.

**Not the receipt lane.** The receipt lane seeds at `PALW_RECEIPT_TARGET_SEED_V1` and has since
the 5e-prep commit that decoupled the two: a receipt draw is one quantum of real demand, whose
supply has nothing to do with how fast a class runs its canonical job.

## 7. Consequences

* Every V2 preset's fingerprint moves. Relaunch 5e is a wipe, like every relaunch on this chain.
* `PALW_RC_FLOOR_DERIVED_PWU` moves 15,416 → 97,606,404 (12,664 expected executions × 7,708). A
  floor block weighing six thousand times more is the identity in §6 working, not a regression.
* A devnet floor is seeded at `MAX/278` instead of `MAX/2`: ~279 draws per block where a drill
  used to need two. At the floor's ~50 ms inference that is ~14 s per block on a network whose
  `bits` is otherwise trivial.
* The first-block cadence lands near target instead of bursting: at genesis `bits` the three
  classes now offer ~0.137 hits/s against 10.15 before, i.e. ~0.41 blocks/min against the 0.5/min
  the cadence set asks for. The 500-block start burst that ADR-0071's record measured against a
  264-block span window does not happen.

## 8. What is deliberately left open

A post-genesis registrant still DECLARES its own `initial_target`, and the transition still writes
it verbatim at registration. Nothing refuses a value the rule would not have produced — a free
field, which this codebase has twice found to be a free draw.

Decision 4 removes the reachable half of that: a class registering weightless is re-priced the
moment it is granted a share, so a declared value buys nothing on the ADR-0075 path. What remains
is a registrant that takes a share at registration and declares its own seed, and the closure is a
one-line equality at the transition — the derived seed is computable by the registrant (its share
is `min_grantable_share_permille`, a ruleset constant, and its `pwu` is counted from the carriage
it already carries), so there is no fixed point to solve. It is left out of this change only
because it invalidates every fixture in the tree that constructs a `ClassRegistered` with a chosen
target, and Relaunch 5e is a deployment rather than a refactor.
