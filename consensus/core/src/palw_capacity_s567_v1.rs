//! **ADR-0164 — stages 5, 6 and 7 of ADR-0160's staged ramp, the pure half** (testnet-12 only, behind the dormant
//! `Params::palw_capacity_emission_budget` (F-EM), `palw_capacity_multi_claim` (F-M1) and `palw_capacity_rho_breaker`
//! (F-K), plus the ready ρ = 250 / ρ = 1000 steps of F-L; the fold halves are `palw_state_v2.rs`'s).
//!
//! * **F-EM — a fixed per-DAA reward budget** ([`PALW_EMISSION_BLOCKS_PER_DAA_V1`] claim-bearing blocks' whole carves a DAA).
//!   A claim's escrow is a carve of the SUBSIDY of the block that carried it ("never an addition to the schedule"), so
//!   emission per DAA is the carve of the claim-bearing blocks of that DAA — and a lane that put ρ × the claims in a DAA
//!   put ρ × the carves in it. Past F-EM, an attempt claim's acceptance charges its block's whole carve ([`PALW_EMISSION_UNIT_MILLI_V1`])
//!   to a rooted ledger keyed by the accepting block's DAA and is refused ([`PalwEmissionRefusalV1`], non-fatal, the carve
//!   burned as any skipped attempt's) when the DAA's ledger would pass [`palw_emission_budget_milli_v1`]. A rider ([`PALW_RIDERS_MAX_V1`]
//!   of them to a lead) is paid out of its lead's carve and charges nothing: the budget — and so the emission — does not
//!   depend on the number of claims at all.
//! * **F-M1 — riders.** [`palw_rider_shares_v1`] splits a lead's subsidy into `1 + n` shares; the fold gives each rider's claim
//!   the carve of its share (the whole admission — work floor, reservation, escrow — runs at that share, so a rider is a claim of
//!   `1/(1+n)` of the lead's size) and takes exactly that carve out of the lead's escrow, so Σ escrow of a block's claims is
//!   the block's carve, to the sompi.
//! * **F-K — the lower-only breaker.** One rooted row ([`PalwRhoBreakerV1`]) of per-epoch counters and a level `ℓ ∈ 0..=7` into
//!   [`PALW_RHO_LADDER_V1`]; at the end of the first chain block at or past each aligned epoch boundary
//!   ([`palw_breaker_evaluate_v1`]) the epoch's counters are judged and the level steps DOWN one rung on a trip (to 0 on a severe
//!   one) and UP one rung only after two clean epochs, never past the ladder; the issuance tier is `min(step ρ, Λ[ℓ])`
//!   ([`palw_breaker_tier_v1`]), so it never exceeds the flag-day schedule.

use crate::Hash64;
use crate::palw_state_v2::PalwBondKeyV2;

// ---------------------------------------------------------------------------------------------
// The rooted ledger (one collection for all three fences)
// ---------------------------------------------------------------------------------------------

/// **A key of `PalwChainStateV2::capacity_ledger`** — the order is the variant order, so the emission days sort first (a prune is a
/// range scan below one DAA), then the riders' leads, then the breaker's single row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwCapacityLedgerKeyV1 {
    /// F-EM: the budget spent in the DAA.
    Emission(u64),
    /// F-M1: a lead claim that has taken its riders.
    RiderLead(Hash64),
    /// F-K: the breaker's row.
    Breaker,
    /// F-K: one bond's own breaker row (B1, B2) — present only while the bond's level is lowered or its epoch counted something.
    BondBreaker(PalwBondKeyV2),
}

/// **A row of the ledger.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwCapacityLedgerRowV1 {
    /// F-EM: milli-carves charged in the key's DAA.
    Emission { spent_milli: u64 },
    /// F-M1: the lead took `riders` riders at `daa` (the row leaves once the window past it has closed).
    Riders { riders: u16, daa: u64 },
    /// F-K.
    Breaker(PalwRhoBreakerV1),
    /// F-K: a bond's own row.
    Bond(PalwBondBreakerV1),
}

// ---------------------------------------------------------------------------------------------
// F-EM
// ---------------------------------------------------------------------------------------------

/// One claim-bearing block's whole carve, in the ledger's unit (milli-carves).
pub const PALW_EMISSION_UNIT_MILLI_V1: u64 = 1_000;

/// **The budget, in claim-bearing blocks a DAA.** testnet-12's lane measured 5.3 claims a DAA at the worst healthy mix
/// (DAA ~3,000, with the five external seats), so 16 is three times the live rate and binds only where the lane's retarget
/// has collapsed and a burst of blocks would otherwise mint a burst of carves. A change is a new fence.
pub const PALW_EMISSION_BLOCKS_PER_DAA_V1: u64 = 16;

/// **A DAA's budget** in milli-carves.
pub const fn palw_emission_budget_milli_v1() -> u64 {
    PALW_EMISSION_BLOCKS_PER_DAA_V1 * PALW_EMISSION_UNIT_MILLI_V1
}

/// Why F-EM refuses a claim (non-fatal for the block's own attempt: it is skipped and its carve burned).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEmissionRefusalV1 {
    pub spent_milli: u64,
    pub budget_milli: u64,
}

/// **Does the DAA's budget take one more claim-bearing block's carve?** `spent` is what the ledger holds for the accepting
/// block's DAA (0 for no row). A pure function of that number.
pub fn palw_emission_admits_v1(spent_milli: u64) -> Result<u64, PalwEmissionRefusalV1> {
    let budget_milli = palw_emission_budget_milli_v1();
    match spent_milli.checked_add(PALW_EMISSION_UNIT_MILLI_V1) {
        Some(after) if after <= budget_milli => Ok(after),
        _ => Err(PalwEmissionRefusalV1 { spent_milli, budget_milli }),
    }
}

// ---------------------------------------------------------------------------------------------
// F-M1
// ---------------------------------------------------------------------------------------------

/// **Riders a lead may take** — `n_max` of ADR-0160 §9.3.
pub const PALW_RIDERS_MAX_V1: usize = 64;

/// **The window a lead takes riders in**, DAA past its acceptance: inside the panel's anchor delay (20), so a rider's lead is still
/// `Provisional` and its escrow still the carve it was accepted with.
pub const PALW_RIDERS_WINDOW_DAA_V1: u64 = 8;

/// **The shares of a lead's subsidy among itself and `n` riders**: `(rider_share, lead_remainder)` with
/// `rider_share = ⌊subsidy / (1 + n)⌋` and `lead_remainder = subsidy − n × rider_share`.
pub fn palw_rider_shares_v1(subsidy: u64, riders: usize) -> Option<(u64, u64)> {
    let n = u64::try_from(riders).ok()?;
    let share = subsidy.checked_div(n.checked_add(1)?)?;
    let lead = subsidy.checked_sub(share.checked_mul(n)?)?;
    Some((share, lead))
}

/// The domain of a rider's challenge: it binds the rider to its lead and its index, in place of a PoW position (a rider has no
/// header; its price is the collateral every claim costs — slot, bucket, weight cap — never a nonce).
pub const PALW_RIDER_CHALLENGE_DOMAIN_V1: &[u8] = b"misaka-palw/capacity/rider-challenge/v1";

/// **The challenge a rider's attempt must carry**: `H(domain ‖ lead ‖ index)` (the same blake2b the attempt module keys with).
pub fn palw_rider_challenge_v1(lead: &Hash64, index: u32) -> Hash64 {
    let mut hasher = blake2b_simd::Params::new().hash_length(64).key(PALW_RIDER_CHALLENGE_DOMAIN_V1).to_state();
    hasher.update(lead.as_bytes().as_slice());
    hasher.update(&index.to_le_bytes());
    Hash64::from_bytes(hasher.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// **The job anchor of a rider** (the anchor its execution is derived under, recorded as the claim's job identity past
/// `palw_offence_attribution`): `H(domain ‖ "anchor" ‖ lead ‖ index)` — on-chain data alone, so the court can re-derive the job.
pub fn palw_rider_job_anchor_v1(lead: &Hash64, index: u32) -> Hash64 {
    let mut hasher = blake2b_simd::Params::new().hash_length(64).key(PALW_RIDER_CHALLENGE_DOMAIN_V1).to_state();
    hasher.update(b"anchor");
    hasher.update(lead.as_bytes().as_slice());
    hasher.update(&index.to_le_bytes());
    Hash64::from_bytes(hasher.finalize().as_bytes().try_into().expect("64 bytes"))
}

// ---------------------------------------------------------------------------------------------
// The verification supply behind L_ver, stepped with the tier
// ---------------------------------------------------------------------------------------------

/// **RFC-0006's seat-work factor, in milli**: layer sharding takes a claim's verification from about five full replays (a k = 2 panel plus
/// the audit and the court margin) to about two replay-equivalents, so the same seats license 5 / 2 = 2.5 times the claims in the receipt
/// window. The factor applies to `L_ver`'s `μ_floor` from the ρ = 250 step (the release that adds the sharding arms it at H, the step at
/// H + 190), and ONLY while the issuance tier is at or above 250: a breaker that has lowered the tier below it takes the factor back.
/// (RFC-0007's tally licensing cuts per-claim signature carriage from 14.4 KB to 66–330 B, which relieves `L_carry`, not `L_ver`; `L_carry`
/// at 64 licences a block already clears the network level, so it is not stepped.)
pub const PALW_VERIFY_SUPPLY_SHARDED_MILLI_V1: u64 = 2_500;

/// The tier from which the sharded supply counts.
pub const PALW_VERIFY_SUPPLY_TIER_V1: u32 = 250;

/// **The verification supply factor (milli) at an issuance tier**: 1,000 below ρ = 250, [`PALW_VERIFY_SUPPLY_SHARDED_MILLI_V1`] from it.
pub const fn palw_verify_supply_milli_v1(tier_rho: u32) -> u64 {
    if tier_rho >= PALW_VERIFY_SUPPLY_TIER_V1 { PALW_VERIFY_SUPPLY_SHARDED_MILLI_V1 } else { 1_000 }
}

// ---------------------------------------------------------------------------------------------
// The ladder and F-K
// ---------------------------------------------------------------------------------------------

/// **Λ — the ρ ladder** (ADR-0160 §8.1).
pub const PALW_RHO_LADDER_V1: [u32; 8] = [1, 10, 25, 50, 100, 250, 500, 1_000];

/// The top level: no lowering.
pub const PALW_BREAKER_TOP_LEVEL_V1: u8 = 7;

/// The aligned epoch, in DAA (`PALW_RCORE_STRIKE_EPOCH_DAA_V1`'s span).
pub const PALW_BREAKER_EPOCH_DAA_V1: u64 = 1_000;

/// K3 / K4: a ratio trips at one in twenty (5%) of at least [`PALW_BREAKER_MIN_COUNT_V1`] claims reaching their deadline.
pub const PALW_BREAKER_RATIO_DENOM_V1: u64 = 20;
/// The fewest claims a ratio metric needs to trip (and the fewest overdue audits K6 needs).
pub const PALW_BREAKER_MIN_COUNT_V1: u64 = 20;
/// K2: the shortfall's share of the nominal, one in ten, over at least [`PALW_BREAKER_K2_BONDS_V1`] distinct bonds.
pub const PALW_BREAKER_K2_DENOM_V1: u64 = 10;
pub const PALW_BREAKER_K2_BONDS_V1: usize = 3;
/// K6: a credited licence not audited within this many DAA of its licence is overdue.
pub const PALW_BREAKER_AUDIT_WINDOW_DAA_V1: u64 = 60;

/// **A claim reaching its deadline, counted by what happened to it.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwBreakerEventV1 {
    /// A licence landed (K3's denominator; K6's, if credited).
    Licensed { credited: bool },
    /// A claim voided for want of a quorum receipt (`ReceiptTimeout`, `UnavailableQuorum`, `NotReplayBacked`): K3's numerator.
    ReceiptVoid,
    /// A panel bound (K4's denominator).
    Bound,
    /// A claim voided for want of a panel (`BindTimeout`, `NoCapablePanel`): K4's numerator.
    BindVoid,
    /// A conviction closed: its nominal, what it collected, whether its bond had been forfeited whole already, whether the claim
    /// it binds held audit receipts, and the bond (K2 and K1′).
    Conviction { nominal: u64, collected: u64, forfeited_before: bool, audited: bool, accused: PalwBondKeyV2 },
}

/// **The breaker's row** — the level and the current epoch's counters. Rooted (`PalwCapacityLedgerKeyV1::Breaker`).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwRhoBreakerV1 {
    /// The epoch index (`daa / 1,000`) the counters belong to.
    pub epoch: u64,
    /// The level into [`PALW_RHO_LADDER_V1`] — the tier's cap.
    pub level: u8,
    /// Consecutive clean epochs at the current level (saturates at 2).
    pub clean_epochs: u8,
    /// K3.
    pub receipt_reached: u32,
    pub receipt_voids: u32,
    /// K4.
    pub bind_reached: u32,
    pub bind_voids: u32,
    /// K6's denominator: credited licences this epoch.
    pub credited_licensed: u32,
    /// K1′.
    pub false_audits: u32,
    /// K2.
    pub nominal: u64,
    pub collected: u64,
    /// The distinct bonds this epoch's convictions fell short on (the first three).
    pub short_bonds: Vec<PalwBondKeyV2>,
}

impl PalwRhoBreakerV1 {
    /// The row a breaker starts with, in `epoch`: top level, nothing counted.
    pub fn fresh_v1(epoch: u64) -> Self {
        Self {
            epoch,
            level: PALW_BREAKER_TOP_LEVEL_V1,
            clean_epochs: 0,
            receipt_reached: 0,
            receipt_voids: 0,
            bind_reached: 0,
            bind_voids: 0,
            credited_licensed: 0,
            false_audits: 0,
            nominal: 0,
            collected: 0,
            short_bonds: Vec::new(),
        }
    }

    /// The row with `event` counted (saturating: a counter never wraps).
    pub fn noted_v1(&self, event: PalwBreakerEventV1) -> Self {
        let mut next = self.clone();
        match event {
            PalwBreakerEventV1::Licensed { credited } => {
                next.receipt_reached = next.receipt_reached.saturating_add(1);
                if credited {
                    next.credited_licensed = next.credited_licensed.saturating_add(1);
                }
            }
            PalwBreakerEventV1::ReceiptVoid => {
                next.receipt_reached = next.receipt_reached.saturating_add(1);
                next.receipt_voids = next.receipt_voids.saturating_add(1);
            }
            PalwBreakerEventV1::Bound => next.bind_reached = next.bind_reached.saturating_add(1),
            PalwBreakerEventV1::BindVoid => {
                next.bind_reached = next.bind_reached.saturating_add(1);
                next.bind_voids = next.bind_voids.saturating_add(1);
            }
            PalwBreakerEventV1::Conviction { nominal, collected, forfeited_before, audited, accused } => {
                if audited {
                    next.false_audits = next.false_audits.saturating_add(1);
                }
                // A bond the whole-bond forfeiture already took has nothing left for a tier to collect: not a shortfall.
                if !forfeited_before {
                    next.nominal = next.nominal.saturating_add(nominal);
                    next.collected = next.collected.saturating_add(collected.min(nominal));
                    if collected < nominal && next.short_bonds.len() < PALW_BREAKER_K2_BONDS_V1 && !next.short_bonds.contains(&accused) {
                        next.short_bonds.push(accused);
                    }
                }
            }
        }
        next
    }
}

/// **What an epoch's judgement found.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwBreakerVerdictV1 {
    /// Every metric under half its threshold.
    Clean,
    /// Some metric over half, none tripped: the level holds, the clean count restarts.
    Hold,
    /// A metric tripped: one rung down.
    Trip,
    /// A severe metric (K1′, K6 at half): level 0.
    Severe,
}

/// `numerator ≥ reached / denom` (a ratio metric's trip), with the minimum count.
fn ratio_trips(numerator: u64, reached: u64, denom: u64) -> bool {
    reached >= PALW_BREAKER_MIN_COUNT_V1 && numerator.saturating_mul(denom) >= reached
}

/// **The judgement of one epoch's counters**, with `overdue` credited licences past their audit window (K6, counted by the
/// fold over the claims) — a pure function.
pub fn palw_breaker_verdict_v1(row: &PalwRhoBreakerV1, overdue: u64) -> PalwBreakerVerdictV1 {
    let credited = u64::from(row.credited_licensed);
    // Severe: K1′ always; K6 at half of the epoch's credited licences.
    if row.false_audits >= 1 || (overdue >= PALW_BREAKER_MIN_COUNT_V1 && overdue.saturating_mul(2) >= credited) {
        return PalwBreakerVerdictV1::Severe;
    }
    let shortfall = row.nominal.saturating_sub(row.collected);
    let k2 = row.nominal > 0
        && shortfall.saturating_mul(PALW_BREAKER_K2_DENOM_V1) >= row.nominal
        && row.short_bonds.len() >= PALW_BREAKER_K2_BONDS_V1;
    let k3 = ratio_trips(u64::from(row.receipt_voids), u64::from(row.receipt_reached), PALW_BREAKER_RATIO_DENOM_V1);
    let k4 = ratio_trips(u64::from(row.bind_voids), u64::from(row.bind_reached), PALW_BREAKER_RATIO_DENOM_V1);
    let k6 = overdue >= PALW_BREAKER_MIN_COUNT_V1 && overdue.saturating_mul(PALW_BREAKER_RATIO_DENOM_V1) >= credited;
    if k2 || k3 || k4 || k6 {
        return PalwBreakerVerdictV1::Trip;
    }
    // Clean: every ratio under half its threshold (the minimum counts need not be reached to be clean), K2 under half.
    let half = |numerator: u64, reached: u64| numerator.saturating_mul(PALW_BREAKER_RATIO_DENOM_V1 * 2) < reached.max(1) || numerator == 0;
    let clean = half(u64::from(row.receipt_voids), u64::from(row.receipt_reached))
        && half(u64::from(row.bind_voids), u64::from(row.bind_reached))
        && (overdue == 0 || overdue.saturating_mul(PALW_BREAKER_RATIO_DENOM_V1 * 2) < credited.max(1))
        && (shortfall == 0 || shortfall.saturating_mul(PALW_BREAKER_K2_DENOM_V1 * 2) < row.nominal.max(1));
    if clean { PalwBreakerVerdictV1::Clean } else { PalwBreakerVerdictV1::Hold }
}

/// **The ladder index of a ρ**: the highest `i` with `Λ[i] ≤ rho` (0 for a ρ under 1).
pub fn palw_breaker_index_of_rho_v1(rho: u32) -> u8 {
    PALW_RHO_LADDER_V1.iter().rposition(|rung| *rung <= rho).map_or(0, |i| i as u8)
}

/// **The breaker's step at an epoch boundary** — the row after judging `row`'s epoch against `step_rho` (the schedule's ρ at the
/// boundary block), moving to `new_epoch` with fresh counters. `skipped` is how many whole empty epochs lie between (each
/// counts clean). The level only goes DOWN on a trip (one rung below the tier in force, or 0 on a severe one) and UP one rung
/// after two consecutive clean epochs; it is capped by the top of the ladder, and [`palw_breaker_tier_v1`] caps the tier by the
/// schedule, so the breaker never raises ρ above what the flag days armed.
pub fn palw_breaker_evaluate_v1(row: &PalwRhoBreakerV1, step_rho: u32, overdue: u64, new_epoch: u64, skipped: u64) -> PalwRhoBreakerV1 {
    let tier = row.level.min(palw_breaker_index_of_rho_v1(step_rho));
    let (mut level, mut clean) = (row.level, row.clean_epochs);
    match palw_breaker_verdict_v1(row, overdue) {
        PalwBreakerVerdictV1::Severe => {
            level = 0;
            clean = 0;
        }
        PalwBreakerVerdictV1::Trip => {
            level = tier.saturating_sub(1);
            clean = 0;
        }
        PalwBreakerVerdictV1::Hold => clean = 0,
        PalwBreakerVerdictV1::Clean => clean = clean.saturating_add(1).min(2),
    }
    // The empty epochs between the judged one and the new one are clean epochs.
    clean = u64::from(clean).saturating_add(skipped).min(2) as u8;
    if clean >= 2 && level < PALW_BREAKER_TOP_LEVEL_V1 {
        level += 1;
        clean = 0;
    }
    // At the top of the ladder there is nothing to climb to, so there is nothing to count.
    if level >= PALW_BREAKER_TOP_LEVEL_V1 {
        clean = 0;
    }
    PalwRhoBreakerV1 { level, clean_epochs: clean, ..PalwRhoBreakerV1::fresh_v1(new_epoch) }
}

/// **One bond's breaker row** (ADR-0160 §8.3's per-bond metrics, which lower only the bond's OWN tier): B1 — its own
/// producer-attributable endings (a data-availability default, `ProducerWithholding`; never a receipt-deadline expiry, which is the
/// network's) are at least a fifth of at least ten of its claims reaching a deadline in the epoch; B2 — any conviction of the bond, which restarts it one rung below the tier in force. Rooted
/// (`PalwCapacityLedgerKeyV1::BondBreaker`).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBondBreakerV1 {
    pub epoch: u64,
    pub level: u8,
    pub clean_epochs: u8,
    pub reached: u32,
    pub voids: u32,
    pub convicted: bool,
}

/// B1: a fifth of the bond's claims reaching a deadline, of at least this many.
pub const PALW_BOND_BREAKER_MIN_COUNT_V1: u32 = 10;

impl PalwBondBreakerV1 {
    pub fn fresh_v1(epoch: u64) -> Self {
        Self { epoch, level: PALW_BREAKER_TOP_LEVEL_V1, clean_epochs: 0, reached: 0, voids: 0, convicted: false }
    }

    /// The row with one claim reaching its receipt deadline (`void`: it failed there) counted.
    pub fn reached_v1(&self, void: bool) -> Self {
        let mut next = self.clone();
        next.reached = next.reached.saturating_add(1);
        next.voids = next.voids.saturating_add(u32::from(void));
        next
    }

    /// The row with a conviction of the bond counted (B2).
    pub fn convicted_v1(&self) -> Self {
        Self { convicted: true, ..self.clone() }
    }

    /// **The bond's epoch step**: a conviction or B1 lowers one rung below the tier in force (`step_rho` is the schedule's ρ at the
    /// boundary; the network's own level is the network row's business), a clean epoch counts toward one rung up after two. `None`: the row
    /// is idle (top level, nothing counted) and leaves the ledger.
    pub fn evaluated_v1(&self, step_rho: u32, new_epoch: u64) -> Option<Self> {
        let tier = self.level.min(palw_breaker_index_of_rho_v1(step_rho));
        let b1 = self.reached >= PALW_BOND_BREAKER_MIN_COUNT_V1 && u64::from(self.voids) * 5 >= u64::from(self.reached);
        let (mut level, mut clean) = (self.level, self.clean_epochs);
        if self.convicted || b1 {
            level = tier.saturating_sub(1);
            clean = 0;
        } else if self.voids == 0 || u64::from(self.voids) * 10 < u64::from(self.reached) {
            clean = clean.saturating_add(1).min(2);
        } else {
            clean = 0;
        }
        if clean >= 2 && level < PALW_BREAKER_TOP_LEVEL_V1 {
            level += 1;
            clean = 0;
        }
        if level >= PALW_BREAKER_TOP_LEVEL_V1 {
            return None;
        }
        Some(Self { epoch: new_epoch, level, clean_epochs: clean, reached: 0, voids: 0, convicted: false })
    }
}

/// **The issuance tier**: `min(step ρ, Λ[level])` — `step_rho` untouched where the breaker holds no row.
pub fn palw_breaker_tier_v1(step_rho: u32, row: Option<&PalwRhoBreakerV1>) -> u32 {
    match row {
        None => step_rho,
        Some(row) => step_rho.min(PALW_RHO_LADDER_V1[usize::from(row.level.min(PALW_BREAKER_TOP_LEVEL_V1))]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_bytes([n; 64]), 0))
    }

    #[test]
    fn a_bond_breaker_lowers_on_a_conviction_or_its_own_voids_and_recovers_after_two_clean_epochs() {
        let row = PalwBondBreakerV1::fresh_v1(3).convicted_v1();
        let lowered = row.evaluated_v1(1_000, 4).expect("a conviction lowers");
        assert_eq!((lowered.level, lowered.epoch), (6, 4));
        let mut b1 = PalwBondBreakerV1::fresh_v1(3);
        for _ in 0..8 {
            b1 = b1.reached_v1(false);
        }
        for _ in 0..2 {
            b1 = b1.reached_v1(true);
        }
        assert_eq!(b1.evaluated_v1(100, 4).map(|r| r.level), Some(3), "2 of 10: B1, one rung under ρ = 100");
        let mut nine = PalwBondBreakerV1::fresh_v1(3);
        for _ in 0..8 {
            nine = nine.reached_v1(true);
        }
        assert_eq!(nine.evaluated_v1(100, 4).map(|r| r.level), None, "under ten claims: no metric, an idle row");
        let one = lowered.evaluated_v1(1_000, 5).expect("one clean epoch");
        assert_eq!((one.level, one.clean_epochs), (6, 1));
        assert_eq!(one.evaluated_v1(1_000, 6), None, "two clean epochs: back to the top, the row leaves");
    }

    #[test]
    fn the_verification_supply_steps_with_the_tier_and_435_becomes_1087() {
        assert_eq!([1, 100, 249, 250, 1_000].map(palw_verify_supply_milli_v1), [1_000, 1_000, 1_000, 2_500, 2_500]);
        assert_eq!(435 * palw_verify_supply_milli_v1(1_000) / 1_000, 1_087);
    }

    #[test]
    fn the_budget_is_sixteen_carves_a_daa_and_refuses_the_seventeenth() {
        assert_eq!(palw_emission_budget_milli_v1(), 16_000);
        let mut spent = 0;
        for _ in 0..16 {
            spent = palw_emission_admits_v1(spent).expect("inside the budget");
        }
        assert_eq!(spent, 16_000);
        assert_eq!(palw_emission_admits_v1(spent), Err(PalwEmissionRefusalV1 { spent_milli: 16_000, budget_milli: 16_000 }));
        assert!(palw_emission_admits_v1(u64::MAX).is_err(), "a corrupt ledger row is refused, never wrapped");
    }

    #[test]
    fn the_shares_of_a_subsidy_sum_to_the_subsidy_for_every_rider_count() {
        let subsidy = 444_562_014_000u64;
        for n in 0..=PALW_RIDERS_MAX_V1 {
            let (share, lead) = palw_rider_shares_v1(subsidy, n).expect("shares");
            assert_eq!(share * n as u64 + lead, subsidy, "n = {n}");
            assert!(lead >= share, "the lead keeps at least a rider's share, the remainder besides");
        }
        assert_eq!(palw_rider_shares_v1(0, 5), Some((0, 0)));
    }

    #[test]
    fn a_rider_is_bound_to_its_lead_and_its_index() {
        let lead = Hash64::from_bytes([7; 64]);
        assert_ne!(palw_rider_challenge_v1(&lead, 0), palw_rider_challenge_v1(&lead, 1));
        assert_ne!(palw_rider_challenge_v1(&lead, 0), palw_rider_challenge_v1(&Hash64::from_bytes([8; 64]), 0));
        assert_ne!(palw_rider_job_anchor_v1(&lead, 0), palw_rider_job_anchor_v1(&lead, 1));
        assert_ne!(palw_rider_job_anchor_v1(&lead, 0), palw_rider_challenge_v1(&lead, 0), "the two derivations are domain-separated");
    }

    #[test]
    fn the_tier_is_never_above_the_schedule_and_the_breaker_only_lowers_it() {
        assert_eq!(palw_breaker_tier_v1(100, None), 100);
        let mut row = PalwRhoBreakerV1::fresh_v1(5);
        assert_eq!(palw_breaker_tier_v1(100, Some(&row)), 100, "the top level lowers nothing");
        assert_eq!(palw_breaker_tier_v1(2_000, Some(&row)), 1_000, "the ladder's top");
        row.level = 4;
        assert_eq!(palw_breaker_tier_v1(25, Some(&row)), 25, "a level above the step never raises it");
        assert_eq!(palw_breaker_tier_v1(1_000, Some(&row)), 100);
        row.level = 0;
        assert_eq!(palw_breaker_tier_v1(1_000, Some(&row)), 1);
        assert_eq!([1, 9, 10, 99, 100, 249, 250, 999, 1_000, 5_000].map(palw_breaker_index_of_rho_v1), [0, 0, 1, 3, 4, 4, 5, 6, 7, 7]);
    }

    fn judged(row: &PalwRhoBreakerV1, step: u32, overdue: u64) -> PalwRhoBreakerV1 {
        palw_breaker_evaluate_v1(row, step, overdue, row.epoch + 1, 0)
    }

    #[test]
    fn a_trip_lowers_one_rung_below_the_tier_in_force_and_a_severe_one_goes_to_level_zero() {
        // K3: 30 claims reached their receipt deadline, 3 timed out (10% ≥ 5%).
        let mut row = PalwRhoBreakerV1::fresh_v1(5);
        for _ in 0..27 {
            row = row.noted_v1(PalwBreakerEventV1::Licensed { credited: true });
        }
        for _ in 0..3 {
            row = row.noted_v1(PalwBreakerEventV1::ReceiptVoid);
        }
        assert_eq!(palw_breaker_verdict_v1(&row, 0), PalwBreakerVerdictV1::Trip);
        // ρ 1000 in force at the top level: Λ index 7 → 6 (ρ 500).
        let next = judged(&row, 1_000, 0);
        assert_eq!((next.level, next.epoch, next.clean_epochs), (6, 6, 0));
        assert_eq!((next.receipt_reached, next.receipt_voids, next.credited_licensed), (0, 0, 0), "fresh counters");
        // The schedule at ρ 100 (index 4): the level steps to 3 (ρ 50) from the tier in force, not from the unused top.
        assert_eq!(judged(&row, 100, 0).level, 3);
        // K1′: one false audit → level 0.
        let audited = PalwRhoBreakerV1::fresh_v1(5).noted_v1(PalwBreakerEventV1::Conviction {
            nominal: 10,
            collected: 10,
            forfeited_before: false,
            audited: true,
            accused: bond(1),
        });
        assert_eq!(palw_breaker_verdict_v1(&audited, 0), PalwBreakerVerdictV1::Severe);
        assert_eq!(judged(&audited, 1_000, 0).level, 0);
    }

    #[test]
    fn k2_needs_three_distinct_bonds_and_a_tenth_of_the_nominal_and_a_forfeited_bond_does_not_count() {
        let short = |n: u8, forfeited: bool| PalwBreakerEventV1::Conviction {
            nominal: 1_000,
            collected: 400,
            forfeited_before: forfeited,
            audited: false,
            accused: bond(n),
        };
        let mut row = PalwRhoBreakerV1::fresh_v1(1);
        row = row.noted_v1(short(1, false)).noted_v1(short(1, false)).noted_v1(short(2, false));
        assert_eq!(palw_breaker_verdict_v1(&row, 0), PalwBreakerVerdictV1::Hold, "two distinct bonds: over half, not tripped");
        row = row.noted_v1(short(3, true));
        assert_eq!(row.short_bonds.len(), 2, "a bond the forfeiture already took is not a shortfall");
        row = row.noted_v1(short(3, false));
        assert_eq!(palw_breaker_verdict_v1(&row, 0), PalwBreakerVerdictV1::Trip);
    }

    #[test]
    fn the_level_rises_one_rung_only_after_two_clean_epochs_and_never_past_the_top() {
        let mut row = PalwRhoBreakerV1::fresh_v1(1);
        row.level = 3;
        let one = judged(&row, 1_000, 0);
        assert_eq!((one.level, one.clean_epochs), (3, 1), "one clean epoch: the level holds");
        let two = judged(&one, 1_000, 0);
        assert_eq!((two.level, two.clean_epochs), (4, 0), "two: one rung up");
        // Empty epochs in between are clean.
        assert_eq!(palw_breaker_evaluate_v1(&row, 1_000, 0, 9, 1).level, 4, "a judged clean epoch plus one empty one");
        let mut top = PalwRhoBreakerV1::fresh_v1(1);
        for _ in 0..10 {
            top = judged(&top, 1_000, 0);
        }
        assert_eq!(top.level, PALW_BREAKER_TOP_LEVEL_V1);
        // A held epoch (over half a threshold, under it) restarts the count.
        let mut warm = PalwRhoBreakerV1::fresh_v1(1);
        warm.level = 2;
        warm.clean_epochs = 1;
        for _ in 0..30 {
            warm = warm.noted_v1(PalwBreakerEventV1::Bound);
        }
        warm = warm.noted_v1(PalwBreakerEventV1::BindVoid); // 1 of 31: 3.2% — over half of 5%, under it
        assert_eq!(palw_breaker_verdict_v1(&warm, 0), PalwBreakerVerdictV1::Hold);
        assert_eq!(judged(&warm, 1_000, 0).clean_epochs, 0);
    }

    #[test]
    fn the_audit_service_metric_needs_twenty_overdue_credited_licences() {
        let mut row = PalwRhoBreakerV1::fresh_v1(1);
        for _ in 0..100 {
            row = row.noted_v1(PalwBreakerEventV1::Licensed { credited: true });
        }
        assert_eq!(palw_breaker_verdict_v1(&row, 19), PalwBreakerVerdictV1::Hold, "19 overdue: under the minimum count, over half the ratio");
        assert_eq!(palw_breaker_verdict_v1(&row, 20), PalwBreakerVerdictV1::Trip, "20 of 100 credited licences: ≥ 5%");
        assert_eq!(palw_breaker_verdict_v1(&row, 50), PalwBreakerVerdictV1::Severe, "half of them: level 0");
    }
}
