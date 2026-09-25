# 08 — Economics: supply, issuance, rewards, and the collateral design bar

> Status: normative draft for misaka-next. Rule IDs `ECON-R*`, invariant IDs `INV-ECON-*`.
> Citations `path:line` are at the t12 reference `a0af3c92` (see `../../PROVENANCE.md`) unless
> marked *pending* or *ADR-0152* (read from `docs/adr-0152-v31-postedits` @ `9ed1adce`).
> Figures marked *(model)* come from ADR-0152's scripts, not from code. `[unverified]` marks a
> statement not checked against code.

## 1. Purpose

This chapter answers two questions. **Where does MSK come from and where does it go?** — the
supply cap, the emission schedule, how a block's subsidy is split, how a Proof-of-LLM reward is
escrowed, vested and paid, and every path by which MSK is destroyed. And **is lying unprofitable?**
— for every claim, at every moment before the network can have caught a lie, the value an attacker
has already walked away with must not exceed the value consensus still holds against it. That
inequality, INV-ECON-01, is what the stake machinery of chapter 02 exists to make true, and this
chapter is where it is computed on real parameters.

The book's rule for this chapter: an economic claim is only as good as the consensus mechanism
behind each term. "Collateral covers the gain" is false if the collateral can be withdrawn, if the
reward it is compared with was already paid, if the gain is measured in a different unit from the
reservation, or if the fraud is never detected. Each of those failed at least once in t12; each has
a rule below.

## 2. Concepts and types

### 2.1 Supply

| quantity | value | where (t12) |
|---|---|---|
| genesis cap | exactly 10,000,000,000 MSK on every network | `consensus/core/src/config/premine.rs:48-53` |
| emission | 15,000,000,000 MSK over 20 years, 5%/year decay, stepped yearly, zero after year 20 | `consensus/src/processes/coinbase.rs:579-596` |
| maximum supply | 25,000,000,000 MSK (`MAX_SOMPI`) | `consensus/core/src/constants.rs:37-45` |
| unit | 1 MSK = 10⁸ sompi | `consensus/core/src/constants.rs:27` |

### 2.2 One block's subsidy

The subsidy is a function of the block's own DAA score and the network's block interval
(`consensus/src/processes/coinbase.rs:541-553`). On a 120 s network year 1 pays 4,445.62014 MSK a
block (`coinbase.rs:716-718`, `YEAR1_PER_BLOCK_TWO_MINUTE`), ≈ 1,169.1M MSK a year. On t12 it is
split (ADR-0126; `consensus/core/src/config/params.rs:15492-15493`, armed at genesis by the t12
pass at `params.rs:15929-15940`):

| part | share | per block, year 1 | goes to |
|---|---|---|---|
| validator pool | 20% | 889.12 MSK | DNS-overlay validators, stake-proportional (`consensus/src/pipeline/virtual_processor/utxo_validation.rs:1179-1183`) |
| worker inclusion | 8% | 355.65 MSK | the block that includes attestations; unspent remainder burned (`coinbase.rs:143-149`) |
| worker base = **escrow `E`** | 72% | **3,200.8465 MSK** | withheld from the producer; becomes the claim's reward (§2.4) |

The whole worker base is escrowed: a PoL producer is paid nothing at acceptance.

### 2.3 A claim's economic terms

| symbol | meaning | floor | 8k | 2M |
|---|---|---|---|---|
| `E` | escrowed reward (72% of the block subsidy) | 3,200.85 | 3,200.85 | 3,200.85 |
| `w` | weight reservation: the claim's fork-choice weight priced in the exposure unit | 0.1075 | 494.32 | 59,742.94 |
| `R` | realizable rights (execution quanta a Final can spend before a conviction) | 0.01 | 49.45 | 655.37 |
| `s` | buyback bound (ADR-0091 slice, 5% of `E` where a market pair is open; 0 at t12 genesis) | 0 | 0 | 0 |
| `G_res = w + R + s` | what a Final releases before its row matures | 0.12 | 543.77 | 60,398.31 |
| `G = E + G_res` | a claim's whole fraud gain | 3,200.96 | 3,744.62 | 63,599.16 |

MSK, t12 genesis rows (ADR-0152 §2, `E` re-derived here: 444,562,014,000 × 720‰ = 320,084,650,080
sompi). `w`, `R` are the ADR's fold-measured values [not re-derived].

### 2.4 Extracted and frozen: the two sides of the design bar

```rust
/// Value consensus still holds against a claim and can burn on a conviction.
/// Constructible ONLY from: an unpaid escrow, a reserved commitment, a live seat lock,
/// an unreleased vesting row. There is no constructor from free stake.
pub struct Frozen(Sompi);

/// Value that has left consensus control or had an irrevocable effect because of a claim:
/// a paid leg, an executed buyback, a spent right, and the priced value of its weight.
pub struct Extracted(Sompi);

pub struct EconMargin(i128);   // Σ Frozen − Σ Extracted
```

Action tiers (chapter 02 BOND-R10) are **not** `Frozen`: they come out of free stake that several
concurrent convictions may compete for, so they are deterrence, not recovery.

### 2.5 Detection

A lie is *detected* when an honest party files a proof consensus accepts inside the claim's
conviction horizon (the panel and court chapters). INV-ECON-01 is conditional on detection; the
probability that a lie is **undetectable** is bounded separately (INV-ECON-07) as a function of the
attacker's share of eligible stake.

## 3. Normative rules

**ECON-R1 (supply cap).** Genesis MUST mint exactly `GENESIS_CAP` = 10B MSK, carve-outs
(collateral, community, floats) included. No consensus path MAY create MSK except: genesis; the
scheduled subsidy of an eligible block; the release of previously withheld escrow through a vesting
row; a reporter reward carved from a collected slash; and a market or bridge payout backed
one-for-one by an earlier recorded burn or lock (a model market's reserve, a bridge ledger's
backing). Every other mint path is balanced by a ledger entry that removed the same amount earlier.
`genesis + Σ subsidy ≤ MAX_SUPPLY` is not left to follow from the schedule. The schedule only
bounds the subsidy *per paid claim*, so the cap is enforced two ways: by tying the number of paid
claims to the schedule (ECON-R16) and by a per-block check against a rooted minted counter
(ECON-R17).
*Because:* INV-ECON-02. *[Synthesis edit, review: the draft asserted the cap from a per-block
schedule indexed by `LocalDaa`, while paid work blocks per slot were bounded only by per-lane
buckets.]*

**ECON-R2 (subsidy by block, never by account).** A block's subsidy MUST be a function of its own
`BlockDaa` and of whether its attempt was admitted (ECON-R16), and nothing else. It is non-increasing
in DAA. Blocks of a non-work lane (heartbeat, round, receipt) MUST declare zero. No subsidy, bounty
or reward MAY be a function of how many accounts a staker holds.
*Because:* `heartbeat_clock_acceleration` (a clock lane must not mint), `bond_split_amplification`
(INV-BOND-01).

**ECON-R3 (PoL reward is a carve, escrowed at acceptance).** A claim's reward MUST be carved from
the subsidy of the block that carried it (`⌊subsidy · carve⌋`, carve ≤ worker base) and withheld
from that block's coinbase. It MUST NOT be added to the schedule.
*Because:* an escrow minted beside the subsidy exceeds the schedule; t12 learned this when every
finalized claim minted its carve on top of emission until the coinbase began withholding it
(ADR-0042 D10; `consensus/core/src/palw_reward_v2.rs:6-10`, `consensus/src/processes/coinbase.rs:150-167`).

**ECON-R4 (every withheld sompi resolves once).** Each withheld escrow MUST end in exactly one of:
a released vesting leg, a burn record, the panel reserve, or an executed buyback. The identity
`Σ withheld = Σ released + Σ live rows + Σ burned + Σ reserve credits + Σ buyback` MUST hold over
any chain segment.
*Because:* INV-ECON-03.

**ECON-R5 (the Final split).** A Final claim's reward MUST be split `producer / seat pool /
reserve`: the pool is a fixed permille of the reward (or the class's snapshotted share), each
credited seat receives `⌊pool / seats⌋`, and what uncredited seats would have received plus rounding
goes to the reserve, **never** to the producer.
*Because:* a producer that can starve its panel of credit and keep the difference has an incentive
to prefer silent seats.

**ECON-R6 (one supply ledger; no silent burns).** Every path that destroys MSK MUST write the
amount to one rooted supply ledger, by path. A transaction output that is unspendable by design
(for example a model-market sink) MUST be refused unless an object in the same transaction binds
it, and the binding object's fold MUST record its amount.
*Because:* `unbound_model_sink_output_burn`; INV-ECON-04.

**ECON-R7 (no early extraction).** Every economic leg a claim can yield — reward shares, buyback,
execution rights, fee rights — MUST stay in the claim's vesting row until the row is releasable
(chapter 02 BOND-R12). Nothing of a claim's value MAY leave consensus control before its conviction
horizon.
*Because:* it reduces INV-ECON-01 to the one term cash cannot cover (weight), and removes the
declared-price terms t12 had to guess (§6.4).

**ECON-R8 (what counts as recoverable).** INV-ECON-01's right-hand side MUST count only `Frozen`
values. Free stake, action tiers, reporter bonds, future income and the attacker's other claims
MUST NOT be counted.
*Because:* tiers are collected from a shared free half (t12 A-5: "unless earlier slashes consumed
it", ADR-0152 §3.5), so they are not guaranteed per claim.

**ECON-R9 (the design bar).** For every admitted class and every stage of a claim:
`Extracted(claim) ≤ Frozen(claim)` (INV-ECON-01). Cumulative claims are unlimited; only
concurrent unresolved risk is reserved. Every economic table for a class MUST state the normal-path
hold time, claims per month per minimum stake, and the **uncovered maximum loss at 1,000
concurrent claims on the minimum stake**, both given detection and if undetected.
*Because:* the operator's bar (`claim_collateral_design_bar`, memory; ADR-0152 §1.5 v1 decision 6).

**ECON-R10 (reporter reward).** A conviction MAY pay one reporter
`⌊r · max(0, collected − X)⌋` with `r ≤ 10%`, `X` the part of the debit that recovery needs, from
the collected debit only; burned vesting is never in the base; a forfeit that is not a conviction
pays nothing. The reward MUST go to the earliest matching commit–reveal.
*Because:* a reward from nominal amounts is a mint; an offender that reports itself must still lose
at least `(1 − r) · collected` (INV-ECON-08).

**ECON-R11 (economic admission of a class).** A class MAY be admitted, or kept open, only if
`admit_class_econ` passes: (a) INV-ECON-01 holds at its parameters on the minimum stake; (b) an
honest verifier can complete a proof inside its conviction horizon (class-derived deadlines);
(c) its weight is priced in the unit the ledger reserves (INV-ECON-09) and the fork-choice
chapter's bound on that weight applies; (d) its undetectable-fraud threshold (INV-ECON-07) is at
least the network target share `s_target`. Otherwise the class is closed, or conservative (held
to Final with a static in-flight cap) while the owner decides.
*Because:* t12's 2M row fails (b) and (c), and meets (a) only with its weight valued at `w` (§6.3).

**ECON-R12 (weight is a separate harm).** Cash collateral MUST NOT be assumed to cover a claim's
fork-choice effect beyond its priced weight `w`. The fork-choice chapter MUST bound what a set of
fraudulent verified claims can do to chain selection so that its harm is at most additive in `w`.
*Because:* one t12 2M Final buys raw fork weight worth 335,728,175.72 MSK, 3.36% of the genesis
cap and 5,620× its reservation (`consensus/core/src/config/premine.rs:120-128`).

**ECON-R13 (throughput caps bound exit-able value).** The unreleased-reward ceiling and the
release rate MUST be consensus parameters with pure-function derivations, so the value that can
leave in a window is bounded independently of stake.
*Because:* ADR-0152 T-3; the design bar trades bond caps for throughput caps.

**ECON-R14 (derived, never typed).** Every collateral figure written into a genesis (seat stake,
floors) MUST be the output of a pure function of class economics in the unit the runtime reserves,
re-derived by a test that fails the build on drift.
*Because:* t12's first genesis posted collateral in declared leaves while the runtime reserved in
normalised MAC-equivalents, 44.25× apart on the 2M row, and every producer held from genesis
(`consensus/core/src/config/premine.rs:107-116`).

**ECON-R15 (fees).** Transaction fees are paid at the block and are not escrowed. A non-work lane
is fee-only. No gain term MAY be priced by an operator-declared constant (a "fee ceiling"): the
term must be bounded by consensus or deferred by ECON-R7.
*Because:* t12 prices a stolen execution round at a declared 0.01 MSK
(`consensus/core/src/palw_economic_safety_v1.rs:165-179`), "a number nobody has measured".

**ECON-R16 (only admitted work is paid, at a rate tied to the schedule).** Subsidy MUST be paid for
an attempt iff that attempt is admitted on the paying chain (03 CLAIM-R2). This covers the chain
block's own attempt and each merged attempt admitted in mergeset order (11 BLK-R6). A lost-lottery
attempt, a refused attempt, a skipped merged attempt and every non-work block mint nothing. The
per-claim subsidy MUST be `per_claim(daa) = ⌊schedule_per_slot(month(daa)) / R_total⌋`, where
`R_total = Σ_lanes rate_lane` is the admissions per tick of all paid lane buckets at full rate
(03 §2.6). Over a whole chain, admissions are at most `B_total + R_total · LocalDaa(tip)` (03 §2.6),
and `LocalDaa(tip)` never exceeds the wall-clock slot count (INV-DAA-02). So the total subsidy is at
most the schedule's sum over elapsed slots plus `B_total · per_claim(0)`.
*Because:* `heartbeat_clock_acceleration` (a clock lane must not mint); `failed_lottery_blue_weight`
(a refused attempt must not be paid); INV-ECON-02. t12 enforces the payment half through
`palw_v2_unentitled_blues` (`consensus/src/pipeline/virtual_processor/processor.rs:5436-5591`). Its
own comment records a residual race in one mergeset (`:5540-5546`), which ECON-R16 closes by paying
from the admission result itself (11 §4 step 8).

**ECON-R17 (a rooted minted counter).** The supply ledger (ECON-R6) MUST carry a rooted
`minted_total`. A block MUST be invalid if its coinbase would carry `GENESIS_CAP + minted_total`
past `MAX_SUPPLY`. The schedule's total MUST be set so that the `B_total · per_claim(0)` surplus of
ECON-R16 fits under `MAX_SUPPLY`. *Because:* INV-ECON-02 as a per-block fact, not a simulation fact
(08 Q8-4).

## 4. Pure functions

### 4.1 Issuance

```rust
pub fn block_subsidy(daa: BlockDaa, lane: Lane, admitted: bool, schedule: &EmissionSchedule, r_total: u64) -> Sompi;
pub fn split_subsidy(subsidy: Sompi, split: &SubsidySplit) -> SubsidyParts; // {escrow, validator, inclusion, service}
pub fn escrow_carve(subsidy: Sompi, carve: Permille) -> Sompi;               // floor; ≤ worker base
```
`block_subsidy` is `⌊table[min(month(daa), 240)] / r_total⌋` for an admitted attempt of a work
lane, and 0 otherwise (ECON-R16). Here `month(daa) = daa · SLOT_MS / 1000 / SECONDS_PER_MONTH`.
`split_subsidy`: parts sum to `subsidy` exactly; the primary takes rounding (t12
`consensus/core/src/dns_finality.rs:3054-3063`). Properties:

* `block_subsidy` is non-increasing in `daa` and zero unless `admitted`;
* `Σ_{months} table[m] · SECONDS_PER_MONTH ≤ 15B MSK − B_total · per_claim(0)` (t12's test of the
  table is `coinbase.rs:679-703`);
* over any chain, `Σ minted ≤ Σ_{t ≤ LocalDaa(tip)} table[month(t)] + B_total · per_claim(0)`
  (ECON-R16);
* no block passes ECON-R17's counter check with a coinbase that exceeds `MAX_SUPPLY`.

`BlockDaa` is the block's own DAA on its branch, a `LocalDaa`. That is safe here and only here.
Subsidy is monotone non-increasing in DAA, so advancing a branch's DAA can only lower what its blocks
mint. Which attempts are paid at all is decided by admission (ECON-R16), and which chain is paid is
fork choice's decision.

### 4.2 The Final split

```rust
pub fn final_split(reward: Sompi, pool: Permille, seats: SeatCount, credited: SeatCount) -> FinalSplit;
```
`pool_amt = ⌊reward · pool⌋; producer = reward − pool_amt; per_seat = ⌊pool_amt / seats⌋;
paid = per_seat · min(credited, seats); reserve = pool_amt − paid`. Identity `producer + paid +
reserve = reward` (t12 `consensus/core/src/palw_panel_economy_v1.rs:243-257`).

### 4.3 Gain, forfeit and break-even

```rust
pub fn claim_gain(c: &ClaimEconRecord) -> Gain;                     // {escrow, weight: w, rights: R, buyback: s}
pub fn break_even_success(g: Sompi, forfeit: Sompi) -> Ratio;       // P* = F / (G + F)
pub fn fraud_ev(p_success: Ratio, g: Sompi, forfeit: Sompi) -> i128; // p·G − (1 − p)·F
```
A fake or lying claim that fails forfeits `F = w + E + rr` (S0′, chapter 02 BOND-R13); one that
succeeds undetected gains `G`. An attempt is EV-positive iff `p > P*`. On t12: `P*` = 0.500 (floor),
0.497 (8k), 0.497 (2M) at S0′ (ADR-0152 §4.3; reproduced: 0.49999, 0.49668, 0.49741).

### 4.4 The margin

```rust
pub fn frozen(c: &ClaimEconView) -> Frozen;
pub fn extracted(c: &ClaimEconView) -> Extracted;
pub fn econ_margin(c: &ClaimEconView) -> EconMargin;   // frozen − extracted, per claim
pub fn owner_margin(claims: &[ClaimEconView]) -> EconMargin; // Σ over an owner's concurrent unresolved claims
```
Per stage, under ECON-R7:

| stage | `Frozen` | `Extracted` |
|---|---|---|
| Unverified (accepted, not licensed) | unpaid `E` + commitment `w + E + rr` | priced immature weight (0 if the claim carries none) |
| Verified, escrow released | unpaid `E` + `w + rr` + Σ live locks | priced weight `w` |
| Verified, escrow held | unpaid `E` + `w + E + rr` + Σ live locks | `w` |
| Final, row unreleased | row (all legs) + Σ live locks | `w` |
| row released | — | everything (only reachable after the horizon) |

`owner_margin` is additive because every `Frozen` term is per claim (BOND-R8 locks and BOND-R12 rows
are keyed by claim); it is **not** additive in `Extracted` if the fork-choice harm of many claims is
super-additive, which ECON-R12 hands to the fork-choice chapter.

### 4.5 Undetectable-fraud threshold

```rust
pub fn stake_threshold(p_star: Ratio, honest: Sompi, criterion: DetectCriterion) -> Sompi;
```
Under chapter 02's with-replacement draw with 5 seats and attacker stake share `s`:
`P2 = s²` (the full seat and a segment's holder both attacker's), `P3 = P(Bin(5, s) ≥ 3)`,
`P5 = s⁵`. `stake_threshold` returns the least `S` with `P(S/(S+H)) ≥ P*`. With `H` = 7,512,505.68
MSK (t12's eight genesis seats) and `P*` = 0.5:

| criterion | share `s*` | `S*` (MSK) | t12's draw, best split (ADR-0152 §4.3) |
|---|---|---|---|
| P2, honest unserved seats file (design point) | 0.7071 | 18.14M | 17.29M (share 0.697) |
| P3, V1 door, no filing | 0.5000 | 7.51M | 6.63M (share 0.469) |
| P5, all five seats | 0.8706 | 50.52M | 50.83M (share 0.871) |

In misaka-next the threshold is a pure function of share, so it is stated and monitored as a share.

### 4.6 Admission and reporter reward

```rust
pub fn admit_class_econ(class: &ClassEcon, net: &NetEcon) -> Result<ClassAdmission, EconRefusal>;
pub fn reporter_reward(collected: Sompi, x: Sompi, r: Bps) -> Sompi;
pub fn supply_identity(ledger: &SupplyLedger) -> bool;
```
`admit_class_econ` returns `Open`, `Conservative { cap }` or `Refused(reason)` by ECON-R11 (a)–(d);
every reason names the failing inequality and its two sides.

## 5. Invariants upheld (full statements in 09-invariants.md)

* **INV-ECON-01** For every claim of an admitted class, at every point before its conviction horizon
  closes, `Extracted(claim) ≤ Frozen(claim)`, where `Frozen` counts only values consensus holds and
  can burn (unpaid escrow, reserved commitment, live locks of its signers, its unreleased vesting
  row) and `Extracted` counts every paid leg, executed buyback, spent right and the priced value of
  its weight. (Refines the seed "maximum guaranteed attacker gain before detection MUST NOT exceed
  guaranteed slashable collateral".) Test: `inv_econ_01_extracted_never_exceeds_frozen`.
* **INV-ECON-02** Genesis mints exactly 10B MSK and `genesis + Σ minted ≤ 25B MSK` on every branch;
  only admitted attempts are paid (ECON-R16) and every block passes the rooted counter check
  (ECON-R17). Test: `inv_econ_02_supply_never_exceeds_the_cap`.
* **INV-ECON-03** Each block's subsidy is split exactly; each withheld escrow resolves exactly once
  (ECON-R4). Test: `inv_econ_03_every_withheld_sompi_resolves_once`.
* **INV-ECON-04** Every destroyed sompi is recorded in the supply ledger by path.
  Test: `inv_econ_04_no_silent_burn`.
* **INV-ECON-05** A non-work block mints nothing; raising a branch's DAA never raises a block's
  subsidy. Test: `inv_econ_05_heartbeats_mint_nothing`.
* **INV-ECON-06** No leg of a claim's value leaves consensus control before its conviction horizon.
  Test: `inv_econ_06_no_leg_leaves_before_the_horizon`.
* **INV-ECON-07** For each admitted class and door, the success probability of an undetectable fraud
  is below `P*` whenever the attacker's share of eligible stake is below `s_target`.
  Test: `inv_econ_07_undetectable_fraud_is_ev_negative_below_target_share`.
* **INV-ECON-08** A reporter reward is strictly less than the debit it is carved from; reporting
  one's own conviction is never profitable. Test: `inv_econ_08_self_report_loses`.
* **INV-ECON-09** The reservation, the ceiling, the forfeit and the weight price of a claim read one
  number in one unit. Test: `inv_econ_09_one_unit_for_reservation_and_weight`.
* **INV-ECON-10** A class that fails `admit_class_econ` takes no claim.
  Test: `inv_econ_10_unadmitted_class_takes_no_claim`.

## 6. t12 reference

### 6.1 Supply and issuance, as t12 does it

* **Genesis.** One main-wallet UTXO pays for every carve-out; `checked_sub` against the cap fails the
  build on overflow (`consensus/core/src/config/premine.rs:752-756`), pinned by
  `every_network_genesis_mints_exactly_the_10b_cap` (`:781-793`). t12 genesis carries a 858M MSK
  community table (`:464-507`), eight genesis seats of 939,063.21001040 MSK each
  (`:155`, 7,512,505.68 MSK in all), and the main wallet the rest (≈ 9.13B MSK; computed, assuming
  one 100 MSK float per seat, `:598` [unverified for t12]).
* **Emission.** The table holds 20 yearly rates × 12 months then 0 (`consensus/src/processes/coinbase.rs:579-620`);
  the schedule is indexed by DAA × block interval (`:557-575`), so it is emission-neutral across
  interval changes and slows in wall time when DAA runs slower than target.
* **Heartbeats mint nothing.** A heartbeat (and a round block) must declare zero subsidy
  (`consensus/src/pipeline/body_processor/body_validation_in_context.rs:74-89`). Meets ECON-R2.
* **Paid attempts per DAA are a controller target, not a bound.** The schedule's test assumes one
  paid block per target block time (`blocks_per_month = SECONDS_PER_MONTH · 1000 / ttpb`,
  `consensus/src/processes/coinbase.rs:702`). On t12 the DAA advances only with heartbeat ticks
  (01 §6.1). The number of paid attempt blocks per DAA is what the `W` controller aims at
  (`expected = epoch_length × fp_attempt_share_permille / 1000`,
  `consensus/core/src/palw_state_v2.rs:22756`), plus the floor class. Merged blues that fail admission
  are not paid (`palw_v2_unentitled_blues`, `processor.rs:5436-5591`), with a stated residual race
  (`:5540-5546`). So INV-ECON-02 is **partial** at the reference: the table is bounded, but paid
  blocks per DAA are bounded only by the controller.
* **Escrow.** `apply_attempt` stores `escrowed_reward = worker_carve_v2(subsidy, carve)` for the
  claim (`consensus/core/src/palw_state_v2.rs:28456-28460`, `:26964-26972`); the coinbase withholds the
  same amount from the carrying block (`consensus/src/processes/coinbase.rs:150-167`). Meets ECON-R3.
* **Final.** `finalize_claim` prices the reward (work price or ADR-0132 snapshot), executes the
  buyback slice at Final, splits producer / pool / reserve and, past `palw_rcore_plus`, writes the
  legs into a vesting row instead of the payout queue (`consensus/core/src/palw_state_v2.rs:19200-19324`).
  The pool is 200‰ (`consensus/core/src/palw_panel_economy_v1.rs:56`) or the class snapshot's share.
* **Market and bridge.** A model-market sell's net is credited in the EVM or minted by the coinbase
  on the carrier lane from the market reserve that earlier buys burned into; an EVM withdrawal
  beyond the bridge ledger's backing is skipped [unverified here; leads: memory
  `unbound-model-sink-output-is-a-silent-burn`, `evm-bridge-ledger-fad522c6`].
* **Void.** A voided claim's escrow is forfeit and burned (`consensus/core/src/palw_reward_v2.rs:66-76`);
  `void_and_slash_at` also debits the bond `reserved + (E + rr) + action` for forfeiting reasons
  (`consensus/core/src/palw_state_v2.rs:18828-18908`).

### 6.2 Burn paths in t12

| path | recorded where | verdict |
|---|---|---|
| voided escrow | not minted; ADR-0152 V-3's test-only identity (`vesting_burned` counts row burns, not voids) | recorded only by the identity test [ADR-0152 V-3] |
| bond slash | `PalwBondStateV2.slashed` (`palw_state_v2.rs:3416-3420`); release spend must destroy it (`:18731-18745`) | recorded |
| vesting row burn | `vesting_counters.burned` (`palw_state_v2.rs:19948-19972`) | recorded |
| class registration burn, 1 MSK | through `slash_bond` into `slashed` (`palw_state_v2.rs:6003`, `:18747-18753`) | recorded |
| work-price remainder | "never named — never minted" (`palw_state_v2.rs:19227-19228`) | not recorded; closes only in the identity |
| service share, undistributed validator pool, unspent inclusion bounty | "burned by don't-mint" (`consensus/src/processes/coinbase.rs:138`, `:148`) | not recorded |
| model-market sink output with no binding object | accepted by output class once the market fence is active (`consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs:102-111`, `tx_validation_in_header_context.rs:95-110`); the only other reader checks object → output, not output → object (`consensus/core/src/palw_lifecycle_objects_v2.rs:572`) | **silent burn: real** (defect against ECON-R6) |

Burns are spread over several counters and some paths record nothing; there is no single supply
ledger. misaka-next's ECON-R6 makes the ledger one rooted map.

### 6.3 INV-ECON-01 on t12's real parameters

Per claim, MSK, t12's stake-weighted draw, `m = 3`, 13,000 MSK producer, seats of 130k or more.
Stage values are ADR-0152 §4.4's; the margins and the concurrency row are computed here from
them.

| | floor | 8k | 2M |
|---|---|---|---|
| pre-licence: extracted / frozen | 0 / `w + E` = 3,200.95 (+ unpaid `E`) | 0 / 3,695.17 (+ `E`) | 0 / 62,943.79 (+ `E`) |
| licence→Final, every seat served and `k ≥ 2`: frozen `w + Σlocks` (+ unpaid `E`) | 320.32 | 1,412.55 | n/a (C7 holds to Final: 129,702.01) |
| post-Final, row unmatured: extracted `G_res` / frozen row + 3·`lock_3` | 0.12 / 3,521.06 | 543.77 / 4,119.08 | 60,398.31 / 69,959.07 |
| **margin given detection** | **+3,520.94** | **+3,575.31** | **+9,560.76**, but precondition (vi) fails (weight is not worth `w`, §6.4) |
| concurrent claims per 13,000 MSK producer (`⌊6,500 / (w+E)⌋` unlicensed) | 2 | 1 | 0 (a 2M producer needs ≥ 125,887.58 MSK) |
| uncovered at 1,000 concurrent claims, **given detection** | 0 (reachable on 13k: one unlicensed plus ~1,000 licensed-and-released claims commit ≈ 3,308 MSK) | 0 (1,000 released claims commit 494,320 MSK: ≈ 0.99M MSK posted) | not reachable; not covered in the weight unit |
| uncovered per claim **if undetected** (`G`) | 3,200.96 | 3,744.62 | 63,599.16 |
| undetectable-lie threshold (Sybil stake, best split) | 17.29M (worst state 12.74M; 8.32M with a free redraw) | **6.63M at the reference** (P3: the `AttnFused` lie has no conviction route, so honest filing does not help); 17.16M once A-held lands (*pending*) | no honest verifier finishes inside the horizon; `c_2M = 1`, held to Final |
| verdict at `a0af3c92` | **holds** below 17.29M Sybil stake, given filing | **does not hold** above 6.63M until A-held (*pending*, `feat/t12-aheld-node`) | **does not hold** (ADR-0152 §4.4); closure at launch is *pending* (`feat/t12-class-verify-deadline`) |

Notes, with sources:
* The 8k row: an arithmetic lie in an `AttnFused` step leaf returns `NeedsDissection`, is refused as
  a DA unit and has no held-dissection conviction (ADR-0152 §4.2 #18). The attacker can choose to lie
  there, so honest seats can refuse to sign but cannot convict; the lie succeeds whenever the
  attacker holds a V1 quorum (P3). The 6.63M figure is ADR-0152 §4.3's P3 threshold (51 operators
  of 130k), applied here by that reasoning [derived]. The fix (void reason 8 `CourtHeldVerdict`,
  object 57) is in `git diff rcore/int-3...feat/t12-aheld-node`.
* The 2M row: full replay ≈ 9.7 days against a 3,000-DAA court window (≈ 4.2 days at 120 s/DAA),
  so precondition (iii) fails (ADR-0152 §4.1 (iii), T-2(b)); `c_2M = 1` held to Final (`consensus/core/src/palw_work_target_v1.rs:217-228`).
  Uncovered ≤ 63,599.16 MSK a claim; at ≈ 147 DAA a claim and 21,600 DAA a month that is ≈ 9.35M MSK
  a month (ADR-0152 §4.2 #18, *(model)*; reproduced: 21,600 / 147 × 63,599.16 = 9,345,183).
* Single t12 genesis community allocations exceed the floor-class threshold: eight of 100M MSK and
  one of 30M (`consensus/core/src/config/premine.rs:464-507`). The threshold is a statement about
  stake share, and the stake is concentrated (ADR-0152 §9.3 Q10).

### 6.4 Where t12 extracts before the horizon (divergences from ECON-R7)

* **Buyback at Final.** The ADR-0091 slice `s` (5% of `E`) executes at Final and is not vested
  (`palw_state_v2.rs:19243`, ADR-0152 V-2); it is priced into `G_res` and the lock takes `s` at
  its cap `5%·E` (ADR-0152 L-1, IA-3).
* **Execution rights mature at F + 1,200.** Quanta are spendable from `final + window_challenge`
  (`consensus/core/src/palw_economic_safety_v1.rs:95-113`), before the row's `F + 3,000`; rights that
  matured before a conviction are not revoked, and the round fees they earn are market-driven
  (ADR-0152 V-2b "Residual, named").
* **A declared price.** Those rights are priced at `PALW_T12_PERMIT_FEE_CEILING_SOMPI` = 0.01 MSK a
  round (`palw_economic_safety_v1.rs:179`); on the 2M row the residual is linear in 216,000 rounds
  (`:174-176`).
* **Weight in the wrong unit for fork choice.** `palw_max_fraud_gain_v1` values weight as
  `palw_fork_weight_sompi_v1(exposure_pwu, slash_value_per_pwu)` (`consensus/core/src/palw_panel_var_v1.rs:120-133`),
  the exposure unit; the fork weight a 2M Final actually inserts is 5,620× that
  (`consensus/core/src/config/premine.rs:120-128`).

misaka-next removes the first three by ECON-R7 (every leg vests) and hands the fourth to the
fork-choice chapter (ECON-R12). After ECON-R7, `Extracted` before the horizon is the priced weight
alone, and the post-Final margin is `row + Σlocks − w`: floor `3,200.85 + locks − 0.11`, 8k
`3,200.85 + locks − 494.32`, 2M `3,200.85 + locks − 59,742.94` (so a 2M class must keep locks ≥
56,542 MSK in total, or stay closed).

### 6.5 Honest capital efficiency (t12)

Normal path: anchor 20 DAA, licence 1–6 DAA after bind, Final at licence + 121, so the escrow term
is held 21–26 DAA when released at licence and 142–147 DAA when held to Final (ADR-0152 §1.1, §5).
A 13,000 MSK producer licenses 256 / 154 floor claims a month at 120 / 200 s per DAA with t11's
measured Withheld rate, 360 / 216 with the early redraw *(model)* (ADR-0152 T-2(d)). An honest reward
waits 3,147 DAA to mint and 3,747 to spend nominally, 7.3 and 8.7 days at the measured 200 s/DAA
(ADR-0152 V-4).

### 6.6 Pending deltas touching this chapter

* `feat/t12-activation-pool` (*pending*): a class Activation Pool (object tag 58) funded only by
  top-ups (conserved, "bounded by the row's `funded_sompi`"), a Frozen class's top-up folded into
  `withheld` rather than refunded, and a P2PKH-output requirement beside the pool carrier (the
  pool's fix round F6). No new issuance [pool payouts checked in the diff's
  `step_activation_pool_v1` doc only].
* `feat/t12-class-verify-deadline` (*pending*): 2M closed at launch until a measured row (U-D1).
* `feat/t12-aheld-node` (*pending*): the 8k attention conviction route.

## 7. Attacks this chapter defends against (detail in 10-attack-model.md)

* `undetected_coverage_lie` — hold both attesters of a segment (or a V1 quorum) and license a lie no
  honest party can refute. ECON-R11(d), INV-ECON-07.
* `private_fake_root_burst` (economic half) — make many fake-root attempts; each failure forfeits
  `w + E` (S0′), so the attempt is EV-negative below `P*`. ECON rules via chapter 02 BOND-R5/R13.
* `early_extraction` — spend buyback, rights or reward before a conviction can land. ECON-R7.
* `weight_unit_gap` — buy fork weight worth far more than the reservation. ECON-R12, INV-ECON-09.
* `collateral_unit_mismatch` (this chapter's earlier name: `wrong_unit_collateral`) — collateral
  posted in a unit the runtime does not reserve in. ECON-R14.
* `unbound_model_sink_output_burn` (earlier name: `silent_sink_burn`) — MSK destroyed with no
  record. ECON-R6.
* `self_report_capture` — an offender files its own conviction to recover the reporter reward.
  ECON-R10.
* `heartbeat_clock_acceleration` (issuance half) — heartbeats advance DAA but mint nothing; the
  subsidy is non-increasing in DAA. ECON-R2.
* `unattributable_2m_claims` (earlier name: `unmeasured_class_admission`) — admit a class whose
  verification cannot finish inside its horizon
  (the 2M row). ECON-R11.

## 8. Open questions for the project owner

**Q8-1. Vest every leg (ECON-R7)?** Options: (a) vest buyback and execution rights with the reward
(this draft); (b) keep t12's buyback at Final and rights at F + 1,200 and price them into `G`;
(c) (b) but with a measured fee bound instead of the 0.01 MSK ceiling.
Recommendation: **(a)**. It removes the only declared-price term from the invariant. Cost: the
market's buyback and the execution lane wait ~3,000 DAA longer.

**Q8-2. The network target share `s_target` (ECON-R11(d)).** Options: 1/3, 1/2, the P2 break-even
0.707. Recommendation: **1/3** for every door on the design point, stated as a share and monitored
on the live eligible stake. With-replacement draws give `P3(1/3) = 0.21` and `P2(1/3) = 0.11`, both
far below `P* ≈ 0.5`; a class or door that cannot meet 1/3 is conservative.

**Q8-3. Should action tiers count toward recovery?** Options: (a) never (ECON-R8, this draft);
(b) the first conviction's tier, from the reserved free half. Recommendation: **(a)**; tiers deter,
they do not recover.

**Q8-4. One supply ledger.** Options: (a) one rooted map `burned[path]` plus `minted[path]`, checked
each block against the coinbase; (b) keep per-subsystem counters and a test-only identity (t12).
Recommendation: **(a)**; it makes INV-ECON-03/04 per-block facts instead of simulation facts.

**Q8-5. The panel pool share.** t12 uses 200‰ for the floor and a compute-derived snapshot share for
model classes. Options: fixed 200‰; derived per class from verification versus production compute
(ADR-0132). Recommendation: **derived per class**, with the derivation a pure function of class
economics (ECON-R14) and the floor pinned at 200‰ until measured.

**Q8-6. The 2M row.** Options: (a) closed until a measured verification row and a fork-choice
bound on weight (ECON-R11, R12); (b) conservative with `c = 1` as at the t12 reference.
Recommendation: **(a)**, as t12's own pending decision U-D1 already chose.
