# 02 — Bonds: account stake, commitments, slashing, exit and vesting

> Status: normative draft for misaka-next. Rule IDs `BOND-R*`, invariant IDs `INV-BOND-*`.
> Citations `path:line` are at the t12 reference `a0af3c92` (see `../../PROVENANCE.md`) unless
> marked *pending* (a branch not merged into the reference) or *ADR-0152* (read from
> `docs/adr-0152-v31-postedits` @ `9ed1adce`). `[unverified]` marks a statement not checked
> against code.

## 1. Purpose

A Proof-of-LLM claim is cheap to make and expensive to check. Whoever makes one must therefore
have something to lose that the network can take if the claim turns out to be a lie. That
something is **stake**: MSK posted by an account and held by consensus. This chapter defines what
stake is, what it buys, how much of it a claim or a panel seat ties up and for how long, how a
conviction takes it, how an honest staker gets it back, and how an unpaid reward (a *vesting
row*) is held so that it too can be taken.

The chapter is organised around one question that t12 answered inconsistently: **could splitting
one stake into many accounts buy anything?** Identities are free (a new key costs nothing), so any
right that is granted *per account* rather than *per unit of stake* is a right an attacker can
multiply. The misaka-next answer is a single discipline (BOND-R2): every authority an account has
is a linear function of its stake, rounded against the account, with no constant term.

The economic side (how large a claim's gain is, how much must be recoverable, supply and issuance)
is chapter 08. The clocks (`LocalDaa`, `SafeDaa`, `ChainFinalizedDaa`) are chapter 01's. What a claim
*is* and how it moves `UnverifiedClaim → VerifiedClaim → FinalClaim` belongs to the claim and panel
chapters; this chapter states only the stake consequences of each state.

## 2. Concepts and types

### 2.1 The stake account

```rust
pub struct AccountId(Hash64);            // H(domain ‖ account public key); one key, one account
pub struct Sompi(u64);                   // 1 MSK = 10^8 sompi; checked arithmetic only
pub struct Permille(u16);                // 0..=1000

pub struct StakeAccount {
    pub id: AccountId,
    pub key: PublicKey,                  // signs claims, receipts, accusations
    pub posted: Sompi,                   // slashable, counts for capacity and draw weight
    pub unbonding: Vec<Unbonding>,       // slashable, counts for NOTHING else (BOND-R14)
    pub slashed_total: Sompi,            // audit trail; burned value (BOND-R11)
    pub payout: PayoutAddress,           // where released vesting legs are paid (fixed per row)
    pub registration: RegistrationIndex, // chain-assigned order; the draw sorts by it (BOND-R3)
    pub mature: Deadline<ChainFinal>,    // Deadline::after(registering block's LocalDaa, W_mature)
}
/// An exit request. The mark is the requesting block's LocalDaa (01 §2.6); the exit may complete
/// only when ChainFinalizedDaa passes it by D_exit (BOND-R14).
pub struct Unbonding { pub amount: Sompi, pub requested: LocalDaa, pub release: Deadline<ChainFinal> }
```

An account is **not** an identity in any safety argument. Nothing on chain distinguishes two
accounts of one owner from two owners (t12 recorded the same fact as ADR-0064 Fact A and withdrew
ADR-0065 D3 "a seat must be someone else" for it; `docs/adr/README.md:149`). Every rule below is
therefore stated in stake, never in account counts.

### 2.2 What stake is committed to

A stake account's `posted` value is partitioned, at every state, into:

| part | type | set by | released by | slashable by |
|---|---|---|---|---|
| **claim commitment** | `ClaimCommitment` | the account's own claim entering the lattice | the claim's next stage (BOND-R5) | the claim's forfeit (S0′/S1/S2) |
| **seat duty** | `SeatDuty` | a panel bind that names the account | Final, void, redraw | never (a duty is not a verdict) |
| **seat lock** | `SeatLock` | a `Valid` receipt the account signed | its conviction horizon (BOND-R8) | a false-`Valid` conviction (S4) |
| **accuser exposure** | `AccuserExposure` | an accusation or court the account opened | the session's close | a refuted accusation |
| **free stake** | `FreeStake` | everything else | — | action tiers (BOND-R10) |

`committed = Σ claim commitments + Σ max(duty, live lock) + registration exposure`, derived, never
stored twice. The partition has two ceilings (BOND-R6).

### 2.3 Value that is recoverable but is not stake

A **vesting row** (`VestingRow`) is a Final claim's reward, named to its payees but not yet paid.
It is *recoverable value*: a conviction burns it. It is *not* posted stake: it buys no capacity, no
draw weight and no exit (BOND-R12). Chapter 08 counts it in the recoverable side of INV-ECON-01
only because this chapter guarantees the three properties that make it count: frozen, burnable,
no escape.

### 2.4 What each claim state may do to stake

The claim chapter's typestate is the authority; this table is its stake projection.

| claim state | producer commitment | seats | vesting |
|---|---|---|---|
| `UnverifiedClaim` (accepted, not licensed) | `w + E + rr` reserved (full forfeit) | duty reserved | none |
| `VerifiedClaim` (licensed, every seat served, `basis_k ≥ 2`, class not conservative) | `w + rr` (escrow term released; the escrow itself is still unpaid) | duty + live locks | none |
| `VerifiedClaim` (any other licence) | `w + E + rr` to Final | duty + live locks | none |
| `FinalClaim` | 0 | locks live to the horizon | one row, frozen to the horizon |
| `Voided` | 0 after the forfeit rule ran | released | none (never written) |

`w` is the claim's weight reservation, `E` its escrowed reward, `rr` its reserved rights; chapter 08
defines them and `G = E + w + R + s`.

### 2.5 Authority of stake, in one place

| a stake account may | measured in | per unit of stake? |
|---|---|---|
| produce claims | commitment room `⌊ratio · posted⌋ − committed` | yes |
| sit on a panel seat | draw probability ∝ `posted` (with replacement, BOND-R3) | yes |
| accuse / open a court / file DA | the free half: `posted − max(committed, ratio · posted) − accuser` | yes |
| post reporter commitments | priced out of free stake per commitment (BOND-R15) | yes |
| receive issuance | **nothing**: issuance is per block, never per account (ch. 08) | n/a |
| win a lottery ticket | **nothing**: a ticket is a function of the execution (claim chapter) | n/a |

## 3. Normative rules

**BOND-R1 (stake is standing, claims only reserve).** A stake account MUST be long-lived slashable
stake that claims and validations reserve but never consume. A claim or seat MUST release its
reservation when it resolves (Final, void, redraw), so cumulative work per account is unbounded and
only *concurrent unresolved* risk is reserved.
*Because:* cumulative limits buy no safety (INV-ECON-01 is per unresolved claim) and cost honest
capital: under t12's option A a 13,000 MSK bond held two concurrent floor claims for 142–147 DAA
each, and the post-Final lock was priced on top of the same gain (ADR-0152 §1.1–§1.3).

**BOND-R2 (linearity; no per-account allowance).** Every quantity that grants an account authority
(commitment room, draw weight, seats, accusation room, reporter commitments, per-class in-flight
share) MUST be a superadditive function of stake, `f(a) + f(b) ≤ f(a + b)` for all `a, b`, so that
splitting never gains. Concretely: linear in `posted`, rounded **down**; no constant term; no
`max(1, ·)`; no ceiling division; no per-account cap (a cap `min(x, cap)` is subadditive and pays
splitting). Floors (minimum stake) are eligibility thresholds and MUST NOT grant anything beyond
the linear share.
*Because:* `bond_split_amplification`; INV-BOND-01, INV-BOND-02, INV-BOND-03.

**BOND-R3 (panel seats are drawn by stake, with replacement).** A panel of `n` seats for a claim
MUST be drawn as `n` independent draws, each selecting an eligible account with probability
`posted(a) / Σ posted`. The seed is 04 POL-R9's. The stake snapshot is 05 PANEL-R8's: the state
strictly before the claim's accepting block, fixed before any leaf of the seed ring exists. Accounts
are laid out on the cumulative interval in **chain-assigned registration order**, never by
`AccountId`, so an account key ground against a predicted seed moves nothing. An account drawn `j`
times holds `j` seats, one per seat index. It reserves `j` duties and signs `j` receipts, each naming
its seat index (05 PANEL-R12). The executor's account MUST be excluded from the population. There
MUST be no per-account weight cap and no "one seat per account" rule.
*Because:* sampling without replacement with one seat per account makes the attacker's seat count
depend on how its stake is split, and makes honest safety depend on how honest stake is split
(t12 §6.3); with replacement both depend on stake share alone (INV-BOND-02).

**BOND-R4 (eligibility is capital, read at the population snapshot).** An account is eligible for
a draw iff, on the population snapshot of 05 PANEL-R8:

* it is active;
* its registration is mature, meaning `mature.elapsed(ChainFinalizedDaa)` holds at the snapshot:
  `W_mature` has passed on the chain-final clock since the registering block;
* `posted ≥ role_floor(Seat, class)` (BOND-R10);
* its work room covers the seat's eligibility amount `max(duty_bind, lock_2)`.

Nothing done after the snapshot (registration, deposit, slash of another account) MAY move who is
drawn.
*Because:* grinding by re-registration after seeing or predicting the seed (t12 SW-8's closed retry
path, ADR-0152 §4.3). Maturity reads the chain-final clock so that a private branch's fast clock
cannot mature its own fresh accounts (01 §2.7).

**BOND-R5 (staged claim commitment).** A claim's commitment on its producer MUST be:
`w + E + rr` from acceptance until licence; `w + rr` after a licence whose recounted basis is
`basis_k ≥ 2` **and** every seat carried a `Valid` receipt **and** the claim was never redrawn
**and** its class is not conservative; otherwise `w + E + rr` to Final; `0` at Final and at every
void after the forfeit rule has run. A released escrow term MUST NOT be re-reserved, and the
release flag MUST be monotone.
*Because:* `private_fake_root_burst` — a fake-root claim is indistinguishable from an honest failure
until a panel speaks, so the pre-licence commitment is the price of an attempt (ch. 08 §4.3).

**BOND-R6 (two ceilings, one invariant).** At every gate: `committed + new ≤ ⌊ratio · posted⌋` for
work (`ratio` = 500‰), and `committed + accuser + new ≤ posted` for everything; an accusation MUST
fit `max(committed, ⌊ratio · posted⌋) + accuser + new ≤ posted`. One pure function computes both
rooms for every reader (fold, admission, draw, node, RPC).
*Because:* the uncommitted half is what action tiers are collected from (BOND-R10); t12 finding 17
and review M1 (one sompi backing two claims) (`consensus/core/src/palw_state_v2.rs:28502-28530`).

**BOND-R7 (a duty never pins more than the claim forfeits).** `seats · duty_bind ≤ commitment at
bind` for every class, with `duty_bind = min(max(λ_term, lock_2), ⌊commitment_at_bind / seats⌋)`.
A duty MUST NOT be slashable.
*Because:* `withholding_producer_uncharged`; INV-BOND-07. (t12 has exactly this function,
`palw_rcore_duty_bind_v1`, `consensus/core/src/palw_state_v2.rs:2904-2906`.)

**BOND-R8 (seat lock follows the recoverable horizon).** A `Valid` signer's lock MUST be priced by
the *recounted* basis `k' = max(basis_k, 2)` as `lock(G_res, k') + ⌈100‰ · E_v / k'⌉` (chapter 08
defines `G_res`, `E_v`), MUST be live until the claim's row matures (and follow any re-keying of
that row by a DA session), and MUST NOT decrease before Final.
*Because:* a post-Final catch must still reach the signers (t12 L-3: without it the 2M break-even
`q*post` rises from 0.37 to 9.86, ADR-0152 §3.4).

**BOND-R9 (one conviction funnel).** Every conviction MUST open by recording, for every account it
may charge, `C₀` (posted before its first debit) and whether that account's exit was shut; run its
legs through one `slash` function that returns what it actually took; and close by writing one
consumed-offence record carrying the nominal and the **collected** amounts. A forfeit that is not
a conviction (S0′, BOND-R13) writes no record and opens no reward.
*Because:* reporter rewards must be paid from what was collected, never the nominal tier (ch. 08
ECON-R10).

**BOND-R10 (action tiers do not shrink when stake is split).** A claim-bound action tier MUST be a
function of the claim only: `tier(kind, G) = m_kind · G` with `m_S2 = 1`, `m_S3 = m_S4 = 3`
(`m = 3`, the t12 multiple). An account MUST NOT be drawn for, or produce, a claim of class `c`
unless `posted ≥ role_floor(role, c) = max(F_role, 2 · 3 · G_c, 2 · (w_c + E_c + rr_c))`
(`F_prod` = 13,000 MSK, `F_seat` = 130,000 MSK), so both the claim's commitment and the largest tier
it can trigger fit, and the tier is always collectible from the uncommitted half that BOND-R6 keeps
free. Equivocation (no claim) takes `min(posted, 3 · G_eq)`.
*Because:* t12 prices tiers as `min(x‰ · C₀, 3G)`, so the same stake split into small accounts pays
less per conviction (`action_tier_dilution`, INV-BOND-08). The tiers are deterrence above
break-even; INV-ECON-01 does not rest on them (chapter 08 ECON-R8).

**BOND-R11 (slashed value is burned).** A debit MUST leave `posted` and MUST NOT be minted again,
except for the reporter reward (chapter 08 ECON-R10), which is carved from the collected debit.
Every debit MUST be recorded in the supply ledger (chapter 08 ECON-R6).
*Because:* redistribution to a counterparty creates an incentive to manufacture convictions.

**BOND-R12 (vesting rows: frozen, burnable, no escape).** At Final the claim's reward legs MUST be
written as one `VestingRow` keyed by the claim, with payees fixed at Final. A row MUST NOT be
transferable, spendable, counted as `posted`, counted in any room or draw weight, or used to meet
any floor. Every conviction binding the claim MUST burn the whole row while it exists. A row MUST
be released (moved to the payout queue) only when `vesting_releasable` holds (§4.7), which reads
`ChainFinalizedDaa` (01 §2.7), never `LocalDaa`, `SafeDaa` or the node's `FinalizedDaa`.
*Because:* `vesting_escape`, `second_clock_heartbeat_escape`; INV-BOND-05.

**BOND-R13 (what a failed claim forfeits).** A capacity void (no panel could bind, no capable
panel) and a first failed panel MUST NOT charge the producer. A second failed panel MUST forfeit the
claim's commitment at its stage (S0′) until attribution of every failure mode is proven (the DA/court
chapter); a conservative class keeps S0′ permanently. A proven fraud before Final (S2) forfeits the
commitment plus its tier; a conviction after Final (S3) burns the row plus its tier.
*Because:* `private_fake_root_burst` (fake roots fail silently), balanced against
`silent_quorum_griefing` (an honest producer on a silent panel is redrawn once for free).

**BOND-R14 (exit is bounded below by the conviction horizon).** A withdrawal MUST first move stake
to `unbonding`, where it is slashable and counts for nothing else. Unbonding stake MUST NOT leave the
account while any of: a claim commitment, duty or live lock names the account; any accuser exposure
is open; the account is payee of an unreleased vesting row; or the exit's `release` deadline
(`Deadline::<ChainFinal>::after(requested, D_exit)`) has not elapsed at the block's
`ChainFinalizedDaa`. `D_exit` MUST be at least `D_max + W_conviction` (03 CLAIM-R13), the longest
conviction horizon of anything the account signed or produced, both in `DaaSpan`.
*Because:* `lock_escape_before_conviction`; INV-BOND-04. Reading the chain-final clock also makes 07
FINAL-R12 structural. An exit completes only once its request lies at least `D_exit` below the
exiting chain's own finalized anchor, so every history the retired account could re-sign forks
below the finalized anchor of any node that followed that chain.

**BOND-R15 (reporter commitments are priced).** Each open reporter commitment MUST reserve
`reporter_bond` of free stake until it is revealed or expires. There MUST be no per-account count
allowance.
*Because:* t12 allows 64 open commitments per bond with no reservation
(`consensus/core/src/palw_state_v2.rs:967`), a per-account allowance (BOND-R2).

**BOND-R16 (no per-account escalation).** A penalty MUST NOT depend on how many earlier offences
the same account committed (strikes, recidivism multipliers, tombstones). Each offence MUST be
priced so that it is unprofitable on its own (INV-ECON-01).
*Because:* an account-keyed escalation is evaded by rotating accounts (`strike_evasion_by_split`),
and an honest operator hit by correlated faults is the only party it reliably punishes.

**BOND-R17 (ejection is a capital predicate).** An account whose `posted` falls below a floor MUST
lose exactly the authorities that floor gates (producing, sitting) and nothing else; there MUST be
no status flag, tombstone or identity ban. An in-place deposit (`StakeDeposit`) MUST restore them,
effective for draws whose population snapshot is past the depositing block's `ChainFinalizedDaa`
maturity (BOND-R4).
*Because:* identity bans are split-evadable; a re-registration route (t12's only route) churns
keys for no safety gain.

**BOND-R18 (the ledger is one function of rooted state).** `committed(account)` and
`claim_commitment(claim)` MUST be pure functions of rooted state and the reading point, re-derived
on load, and every stage write MUST move the ledger by `commitment(before) − commitment(after)` in
the same write.
*Because:* t12's two ledgers once disagreed (commitments reached 150% of collateral, finding 12;
ADR-0152 §1.3); a unit mismatch between carve and reservation wedged t12's first fleet
(`consensus/core/src/config/premine.rs:107-128`).

## 4. Pure functions

All functions below are pure: no storage, network, RPC or wall clock. `AccountView` is an
immutable view assembled by the state layer from rooted maps.

### 4.1 Claim commitment

```rust
pub fn claim_commitment(claim: &ClaimEconRecord, stage: ClaimStage, class: &ClassPolicy) -> Sompi;
```
```
full = claim.w + claim.escrow + claim.rr
match stage:
  Unverified | Bound                          -> full
  Verified { release }                        -> if release.escrow_released { full - claim.escrow } else { full }
  Final | Voided { forfeit_ran: true }        -> 0
```
`escrow_released` is set once, by `release_due(claim, licence, class)`:
`door ∈ {Quorum, Coverage} ∧ basis_k ≥ 2 ∧ served_mask == all ∧ rebound.is_none() ∧ !class.conservative`.
Properties: monotone non-increasing along the lattice; `release_due` never un-flips.

### 4.2 Committed and rooms

```rust
pub fn committed(a: &AccountView<'_>) -> Sompi;   // Σ claim commitments + Σ max(duty, live lock) + registration
pub fn work_room(posted: Sompi, committed: Sompi, accuser: Sompi, ratio: Permille) -> Sompi;
pub fn accuser_room(posted: Sompi, committed: Sompi, accuser: Sompi, ratio: Permille) -> Sompi;
```
```
ceiling      = floor(posted * ratio / 1000)
work_room    = min(ceiling ⊖ committed, posted ⊖ committed ⊖ accuser)       // ⊖ saturating
accuser_room = posted ⊖ max(committed, ceiling) ⊖ accuser
```
Property (split): for any `posted = p1 + p2` with committed and accuser split alike,
`work_room(p1,..) + work_room(p2,..) ≤ work_room(p1+p2,..)` (floor rounding).

### 4.3 The draw

```rust
pub fn draw_seats(seed: &DrawSeed, pop: &StakeSnapshot, n: SeatCount) -> Result<Panel, DrawRefusal>;
```
```
pop' = pop.eligible_excluding(executor)            // BOND-R4, in registration order (never AccountId)
T    = Σ posted over pop'                          // u128
if pop'.distinct_accounts() < MIN_DISTINCT or T < eligible_floor(pop.base)  -> Err
for i in 0..n:
    u_i  = u128_from(H(DRAW_DOMAIN ‖ seed ‖ i)) mod T        // rejection-sampled to remove modulo bias
    seat_i = the account whose cumulative interval [S_{a-1}, S_a) contains u_i
Panel { seats: [seat_0 .. seat_{n-1}] }             // seat index i = draw i; receipts name i
```
Properties: determinism; `P(seat_i = a) = posted(a)/T` exactly (up to rejection sampling);
**split invariance**: replacing account `a` by accounts `a1, a2` with `posted(a1)+posted(a2) =
posted(a)` leaves the distribution of "seats held by the owner of `a`" unchanged (INV-BOND-02).
`MIN_DISTINCT` (open question Q2-3) is a liveness floor, not a safety claim.

### 4.4 Duty, lock, tier

```rust
pub fn seat_duty(commitment_at_bind: Sompi, lock_2: Sompi, lambda_term: Sompi, n: SeatCount) -> Sompi;
pub fn seat_lock(g_res: Sompi, e_v: Sompi, basis_k: BasisK) -> Sompi;
pub fn action_tier(kind: OffenceKind, g: Sompi, g_eq: Sompi, posted: Sompi) -> Sompi;
pub fn role_floor(role: Role, class: &ClassEcon) -> Sompi; // max(F_role, 6 · G_c, 2 · (w_c + E_c + rr_c))
```
`seat_duty = min(max(lambda_term, lock_2), commitment_at_bind / n)`;
`seat_lock = lock_required(g_res, k') + ceil(100‰ · e_v / k')`, `k' = max(basis_k, 2)`.
`action_tier`: S2 → `G`; S3, S4 → `3·G`; Eq → `min(posted, 3·G_eq)`. Property: independent of
`posted` for every claim-bound kind (INV-BOND-08).

### 4.5 Forfeit and slash

```rust
pub fn forfeit(claim: &ClaimEconRecord, stage: ClaimStage, reason: VoidReason, failed_panels: u8, class: &ClassPolicy) -> Sompi;
pub fn apply_slash(a: &StakeAccount, amount: Sompi) -> (StakeAccount, Collected);
```
`apply_slash` debits `min(amount, posted + Σ unbonding)`, taking `posted` first and then unbonding
entries oldest first, so moving stake to unbonding never shields it; it increments `slashed_total`
and returns the debit. Property: `posted' + Σ unbonding' + collected = posted + Σ unbonding`.
`forfeit` is BOND-R13's table: `0` for capacity voids and a first failed panel; the stage's
commitment for a second failed panel (S0′); the commitment plus `tier(S2, G)` for a proven fraud
before Final.

### 4.6 Exit

```rust
pub fn exit_permitted(a: &AccountView<'_>, u: &Unbonding, chain_final: ChainFinalizedDaa) -> bool;
```
`true` iff no commitment/duty/live lock names `a`, `accuser(a) == 0`, `a` is payee of no unreleased
row, and `u.release.elapsed(chain_final)` (`u.release = Deadline::<ChainFinal>::after(u.requested,
D_exit)`). Takes `ChainFinalizedDaa`. Calling it with `LocalDaa`, `SafeDaa` or the node's
`FinalizedDaa` does not compile.

### 4.7 Vesting release

```rust
pub fn vesting_releasable(row: &VestingRow, chain_final: ChainFinalizedDaa, settled: LicenceCount, open_session: bool, depth: u64) -> bool;
```
`true` iff all three hold:

* `row.expiry.elapsed(chain_final)`. `expiry: Deadline<ChainFinal>` is `Deadline::after(final block's
  LocalDaa, horizon)` with `horizon ≥ W_conviction` (01 §2.6). A DA session may later re-key it,
  again with a `LocalDaa` mark.
* `settled − row.settled_at_final ≥ depth`.
* `!open_session`.

There is no DAA-only escape (open question Q2-4). Because `ChainFinalizedDaa ≤ SafeDaa`, a row never
releases before its claim's conviction window (a `Deadline<Safe>`, 03 §2.3) has closed. Property:
monotone in `chain_final` and `settled`; a released row is never re-frozen; a row is burnable at
every state before release.

## 5. Invariants upheld (full statements in 09-invariants.md)

* **INV-BOND-01** Splitting one stake into N accounts MUST NOT increase the aggregate steady-state
  issuance rate those accounts can earn, nor their aggregate concurrent claim capacity.
  Test: `inv_bond_01_split_does_not_raise_issuance_or_capacity`.
* **INV-BOND-02** The distribution of the number of panel seats controlled by an owner depends
  only on the owner's share of eligible stake, not on how the owner (or anyone else) partitions
  stake into accounts. Test: `inv_bond_02_seat_distribution_is_split_invariant`.
* **INV-BOND-03** For every account-level allowance `f`, `Σ f(parts) ≤ f(Σ parts)`.
  Test: `inv_bond_03_no_allowance_is_superlinear_in_accounts`.
* **INV-BOND-04** No stake leaves an account while anything it signed or produced is inside its
  conviction horizon. Test: `inv_bond_04_exit_waits_for_every_horizon`.
* **INV-BOND-05** A vesting row is burnable from Final until its release; its release reads
  `ChainFinalizedDaa` and the licence count, never `LocalDaa`, `SafeDaa` or the node's
  `FinalizedDaa`; it is never collateral.
  Test: `inv_bond_05_private_daa_does_not_release_vesting`.
* **INV-BOND-06** `committed + accuser ≤ posted` and `committed ≤ ratio · posted` after every
  block. Test: `inv_bond_06_one_invariant_at_every_gate`.
* **INV-BOND-07** `seats · duty ≤ commitment_at_bind` for every class (withholding amplification
  ≤ 1). Test: `inv_bond_07_duty_never_exceeds_forfeit`.
* **INV-BOND-08** The amount a conviction takes for one claim-bound offence does not decrease when
  the same stake is split across more accounts. Test: `inv_bond_08_tier_is_split_invariant`.
* **INV-BOND-09** Supply conservation of stake: `Σ posted + Σ unbonding + Σ slashed_total` changes
  only by deposits and completed exits. Test: `inv_bond_09_slashes_are_burned_and_counted`.

## 6. t12 reference

### 6.1 How t12 does it

* **Bond record.** `PalwBondStateV2` (`consensus/core/src/palw_state_v2.rs:3402`) carries
  `pubkey`, `operator_id`, `collateral` (net of slashes), `slashed`, `status`, `registered_daa`,
  `payout_payload`, `capable_classes`. Its own doc states the intent this chapter enforces:
  "splitting collateral across bonds must not manufacture extra panel seats" (`:3405-3413`).
* **Registration.** Refuses a duplicate bond, collateral below the floor, a reused bond key, and
  (with `palw_operator_id_unique`) a reused operator id (`consensus/core/src/palw_state_v2.rs:23860-23906`).
  The floor is the producer floor (`:2445`), 13,000 MSK on t12
  (`consensus/core/src/palw_fp_devnet_v3.rs:902`, chosen for t12 at `consensus/core/src/config/params.rs:16107-16117`);
  the seat floor is ten producer floors, 130,000 MSK (`consensus/core/src/palw_panel_economy_v1.rs:64-71`).
* **Staged commitment.** `palw_claim_commitment_v1` (`consensus/core/src/palw_state_v2.rs:2474-2500`)
  is BOND-R5's table; `full` is `palw_claim_bond_reservation_v1` (`:4128`).
* **One ledger and two gates.** `palw_bond_committed_v1` (`:2527-2548`) sums reserved exposure
  (own commitments and duties), registration exposure and each live lock's excess over its duty.
  `palw_rcore_gate_room_of_v1` (`:2601-2616`) is BOND-R6's pair of rooms. `apply_attempt` checks the
  producer floor (`:28369`), the class gate (`:28379`), the per-bond class share (`:28384`) and the
  ceiling on the joining state (`:28502-28530`).
* **The draw.** Past `palw_rcore_plus`: one entry per operator, weight = posted collateral in whole
  MSK capped at 1,000,000 MSK (`consensus/core/src/palw_panel_v2.rs:137`), key `L/W`, smallest keys
  sit, i.e. successive sampling **without** replacement, one seat per operator
  (`consensus/core/src/palw_panel_v2.rs:1713-1760`, `:1828-1860`); an 875‰ eligible-stake floor
  (`:143`, `:1793-1799`).
* **Action tiers.** `min(permille · C₀, 3·G)`: S1/S2 at 100‰, S3/S4 at 250‰, Eq at 1000‰
  (`consensus/core/src/palw_state_v2.rs:949-953`, `:3295-3316`).
* **Strikes.** One per bond per 1,000-DAA epoch; the third inside 7,500 DAA escalates
  (`:955-959`, `:3337-3348`).
* **Forfeit.** `void_and_slash_at` debits `reserved + (escrow + rr if the reason forfeits) + action`
  (`:18828-18908`); `BindTimeout` and `NoCapablePanel` forfeit nothing (`:18870`).
* **Slash.** `slash_bond` debits `min(amount, collateral)`, raises `slashed`, returns the debit
  (`:18719-18729`); a seat-side slash is capped at `min_collateral_sompi` per call (`:18711-18713`).
* **Exit.** `palw_bond_collateral_is_locked_v6` (`:2796-2821`): the v5 gate (DAA delay plus the
  second clock), plus committed > registration, an open court, or payee of an unmatured row. The
  DAA delay is 7,500 (`PALW_RC_WINDOWS_V1`, `consensus/core/src/palw_fp_devnet_v3.rs:165`, `:199`; selected
  for t12 at `consensus/core/src/config/params.rs:16102-16106`) plus the DA lattice when armed
  (`consensus/core/src/config/params.rs:2741-2748`); the second clock escapes to DAA-only after
  `2 · window_court` with no licence (`consensus/core/src/palw_state_v2.rs:2224-2230`).
* **Vesting.** `finalize_claim` writes one `PalwVestingRowV1` (`consensus/core/src/palw_state_v2.rs:19200-19324`,
  `:19892-19937`); `burn_vesting_row` deletes it and counts it burned (`:19948-19972`); maturity is
  the lock predicate over `(expiry_daa, settled_at_final)` (`consensus/core/src/palw_vesting_v1.rs:454-464`)
  and never during a licence halt (`:445-447`). `PALW_RCORE_VESTING_ROWS_LANDED_V1 = true`
  (`consensus/core/src/palw_state_v2.rs:2846`).

### 6.2 Per-bond quantities in t12 (the audit this chapter was asked for)

| quantity | t12 rule | split-invariant? | effect of splitting stake S into N bonds |
|---|---|---|---|
| work ceiling | `⌊C · 500‰⌋` (`palw_state_v2.rs:2609`) | yes (floor rounding) | none; rounding favours merging |
| producer floor | 13,000 MSK posted to produce (`:3376-3389`) | threshold | none beyond the linear share |
| issuance | per block; escrow carved from the producing block's own subsidy (`:28456-28460`) | yes | none (INV-BOND-01 holds for issuance) |
| per-class epoch budget | per **class**, not per bond (`:4317-4325`, `:28544-28558`) | yes | none |
| lottery ticket | `class_ticket_v3` = H(execution commitment) (`palw_attempt_v2.rs:581-589`) | yes | none: the bond is in the anchor, but a ticket still needs its own execution; fake roots are priced by BOND-R5, not by bond count |
| per-bond class share | `⌈c_class / 2⌉` unlicensed claims (`palw_state_v2.rs:12239`) | **no** (ceil, per bond) | two bonds fill any class's room; on 2M (`c = 1`) one bond holds the whole lane |
| panel seats | one seat per operator, w/o replacement, weight cap 1M MSK | **no** | 17.29M as 133 operators of 130k: P2 = 0.5003 (ADR-0152 §4.3); the same 17.29M as one operator: at most one seat, P2 = 0 (reproduced by Monte Carlo) |
| action tier | `min(x‰ · C₀, 3G)` | **no** | smaller bonds pay less: 2M S3/S4 per seat 32,500 MSK at 130k vs 190,797.47 at 939k; 2M `q*post` V1 0.3726 vs 0.0985 (ADR-0152 §4.3) |
| strike escalation | per bond (`:3337-3348`) | **no** | rotating three bonds never reaches the third strike |
| reporter commitments | 64 open per bond, no reservation (`:967`) | **no** | 64·N open commitments for free |
| seat slash cap | `min(amount, min_collateral)` per call (`:18711-18713`) | yes (a cap per call, not per account) | none; noted because the cap is independent of the seat's stake |

**Verdict on `bond_split_amplification` (t12): partial.** Splitting does **not** raise issuance,
lottery draws or concurrent capacity. It **does** raise panel influence (the dominant term in the
undetected-lie threshold), lowers the per-conviction slash, evades strike escalation, and
multiplies the per-bond class share and reporter allowance.

### 6.3 Why without-replacement is the wrong draw

Monte Carlo (60,000 draws, 8 honest operators of 939,063 MSK, 5 seats), probability that the
attacker holds both assigned attesters of a segment, `P2 = E[A(A−1)]/20`:

| attacker stake 17.29M as | t12 draw | misaka-next draw (BOND-R3) |
|---|---|---|
| 1 operator (weight capped at 1M, or uncapped) | 0.0000 | 0.486 |
| 17 operators of ~1M | ≈ 0.471 | 0.486 |
| 133 operators of 130k | 0.5004 (ADR: 0.5003) | 0.486 |

With replacement, `P2 = s²` for stake share `s` (here `s = 0.697`), whatever the split. The
attacker's best case barely moves (0.500 → 0.486), but the honest side stops mattering: under t12's
draw the *same* 7.51M of honest stake held as one operator drops the attack threshold from 17.29M
to 0.52M (ADR-0152 §4.3 sensitivity table); under BOND-R3 the threshold is `S*` with
`(S*/(S*+H))² = P*`, i.e. about 18.14M against `H` = 7.51M at `P* ≈ 0.5`, however `H` is held.
t12's 1,000,000 MSK cap exists to patch that dependence and to bound the room lever of one heavy
ready operator (`consensus/core/src/palw_panel_v2.rs:133-137`, SW-A5 and SW-A6). BOND-R3 removes
the first instead of patching it; the second (how much replay one heavy account can be drawn for)
remains a capacity question for the panel chapter.

### 6.4 Divergences (misaka-next ≠ t12)

| t12 | misaka-next | reason |
|---|---|---|
| bond = UTXO outpoint; append-only registry; no top-up (ADR-0152 §9.3 Q11) | account with deposits and unbonding (BOND-R14, BOND-R17) | key churn buys nothing when every rule is in stake |
| draw w/o replacement, one seat per operator, 1M cap | with replacement, no cap (BOND-R3) | INV-BOND-02 |
| operator-id uniqueness as a draw prerequisite (ADR-0152 SW-2) | not a safety input | identities are free |
| tiers `min(x‰·C₀, 3G)`, producer floor 13k for every class | tiers `m·G`, class floors (BOND-R10) | INV-BOND-08 |
| strikes per bond | none (BOND-R16) | split-evadable |
| `⌈c/2⌉` per-bond class share | removed; the class room and the commitment price bound lane occupancy (open Q2-2) | BOND-R2 |
| 64 free reporter commitments per bond | priced (BOND-R15) | BOND-R2 |
| vesting/exit on `LocalDaa` + licence count, DAA-only escape after `2·window_court` | `ChainFinalizedDaa` (+ licence count for vesting), no escape for vesting (open Q2-4) | `second_clock_heartbeat_escape`, chapter 01 |
| draw laid out by operator key, population cut at the anchor | laid out in registration order; population cut before the accepting block (BOND-R3, 05 PANEL-R8) | a key ground against a predicted seed moves nothing |
| collateral floored per class only via the genesis card (`config/premine.rs:73-155`) | floors are pure functions of class economics (BOND-R10) | the wrong-unit incident |

### 6.5 Defects and pending deltas

* **Per-bond allowances** (table 6.2 rows marked "no"): defects against BOND-R2.
* **The 8k held-attention gap.** At `a0af3c92` an arithmetic lie in an `AttnFused` step leaf of the
  8k row has no conviction route (ADR-0152 §4.2 #18); the fix (A-held, void reason 8
  `CourtHeldVerdict`, object tag 57) is *pending* on `feat/t12-aheld-node`
  (`git diff rcore/int-3...feat/t12-aheld-node -- consensus/core/src/palw_state_v2.rs`, hunk adding
  `CourtHeldVerdict`). Until it lands, S3/S4 cannot fire for that lie, so the stake behind an 8k
  licence does not back it. Chapter 08 §6.3 prices the consequence.
* **2M open at the reference.** The 2M row is C7 (held to Final, `c_2M = 1`,
  `consensus/core/src/palw_work_target_v1.rs:217-228`); its closure at launch (U-D1) is *pending* on
  `feat/t12-class-verify-deadline`.
* **Top-up.** None exists: the registry is append-only and a bond below the floor after one S0′
  can recover only by re-registering under a new key and operator id (ADR-0152 B-4, §9.3 Q11).

## 7. Attacks this chapter defends against (detail in 10-attack-model.md)

* `bond_split_amplification` — split stake to multiply per-account rights. BOND-R2, R3, R10, R15, R16.
* `undetected_coverage_lie` (this chapter's earlier name: `sybil_seat_capture`) — hold both
  attesters of a segment (or a quorum) to license an undetectable lie. BOND-R3, R4; threshold
  priced in chapter 08.
* `action_tier_dilution` — produce or sign from floor-sized accounts to minimise the slash. BOND-R10.
* `strike_evasion_by_split` — rotate accounts to dodge escalation. BOND-R16.
* `lock_escape_before_conviction` (earlier name: `withdraw_before_conviction`) — exit before a
  conviction lands. BOND-R14, INV-BOND-04.
* `vesting_escape` — move, spend or pledge an unmatured reward. BOND-R12.
* `second_clock_heartbeat_escape` (earlier name: `private_daa_vesting_release`) — advance a
  branch's DAA (heartbeats) so rows or exits mature early. BOND-R12, BOND-R14 (`ChainFinalizedDaa`).
* `withholding_producer_uncharged` (earlier name: `withholding_amplification`) — bind panels and
  serve nothing to pin other accounts' capital. BOND-R7.
* `post_anchor_grinding` — register or deposit after seeing the seed to enter a panel. BOND-R4.
* `private_fake_root_burst` (economic half) — priced by BOND-R5 and BOND-R13; lottery half in the
  claim chapter.

## 8. Open questions for the project owner

**Q2-1. Draw with replacement (BOND-R3).**
Options: (a) with replacement, no cap (this draft); (b) keep t12's without-replacement race and its
1M cap; (c) without replacement but seats ∝ stake by systematic sampling.
Recommendation: **(a)**. It makes INV-BOND-02 true by construction, keeps the attacker's best-case
`P2` within 3% of t12's (0.486 vs 0.500 at 17.29M), and removes the honest-weight-vector cliff (the
same honest stake held by fewer accounts lowers t12's threshold, down to 0.52M). It does not fix
concentration: eight single community allocations of 100M MSK at t12 genesis
(`consensus/core/src/config/premine.rs:464-507`) each exceed every threshold in chapter 08 §4.5,
which only a larger eligible honest stake addresses (chapter 08 Q8-2). Liveness cost: an honest
account holding two seats takes both offline at once; `MIN_DISTINCT` (Q2-3) bounds it.

**Q2-2. The per-class in-flight share.** t12's `⌈c/2⌉` per bond answered `dos_repro_3d` (one junk
claim closing the 2M lane).
Options: (a) remove it; lane occupancy is priced by the full commitment and forfeited at the second
failed panel; (b) a stake-proportional share `⌊c · posted/Σposted_ready⌋` (split-invariant, but
zero for small accounts); (c) keep a per-account share.
Recommendation: **(a)**, plus ch. 08's rule that a class whose single claim can close its lane is
conservative or closed. (c) violates BOND-R2.

**Q2-3. `MIN_DISTINCT`.** Minimum distinct eligible accounts for a draw to bind (a liveness floor).
Options: n (5), 2n, none. Recommendation: **n**, stated as liveness only.

**Q2-4. DAA-only escape for exits.** t12 releases locks and exits by DAA alone after `2·window_court`
with no licence (a halt otherwise freezes stake indefinitely; t12 measured up to 4.13M MSK,
`consensus/core/src/palw_state_v2.rs:2213-2221`).
Options: (a) no escape for vesting rows, escape for exits measured in `ChainFinalizedDaa`; (b) no
escape anywhere; (c) t12's escape on both.
Recommendation: **(a)**. A payee can wait; a staker frozen forever is a liveness failure. Measuring
the escape in `ChainFinalizedDaa` stops a private branch from triggering it.
*Synthesis note (09/00, OQ-4):* `ChainFinalizedDaa` advances only with safe anchors (07 FINAL-R2),
so in a licence halt it stops too, and an escape measured in it never fires during the halt it was
meant for. The consolidated question in `00-overview.md` §10 records the trade-off.

**Q2-5. Class floors (BOND-R10).** `role_floor = max(F_role, 6·G_c, 2·(w+E+rr))` at t12's genesis
values (chapter 08 §2.3): producers 19,205.8 MSK (floor class; t12: 13,000), 22,467.7 (8k),
381,595.0 (2M); seats 130,000 (floor and 8k, unchanged) and 381,595.0 (2M; t12: 130,000).
Options: (a) as drafted; (b) t12's `x‰·C₀` tiers with uniform floors (split-sensitive; t12 accepted
it, ADR-0152 D4: "the producer term stays at 25% × 13,000 = 3,250 for every class"); (c) reserve
the tier per claim at bind (capital-heavy: 3G per concurrent claim).
Recommendation: **(a)**. The floor-class producer floor rises by 48%, and in exchange no account
size pays less per offence than another. If the owner keeps 13,000 MSK for the floor class, take
(a) for seats and (b) for producers, and record that INV-BOND-08 then holds for seats only.

**Q2-6. Unbonding while slashable.** Should unbonding stake be slashed before posted stake?
Recommendation: **no** (posted first, §4.5), so unbonding cannot shield posted stake.
