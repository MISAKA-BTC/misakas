//! **ADR-0175: rule E — the fork choice weighs each tip's exclusive past, bonded participation first
//! (`palw_fork_choice_rule_e_v1`).**
//!
//! Past the fence (read at the INCUMBENT's DAA, as every fork-choice fence here is) a non-extension candidate is weighed against
//! the incumbent — and the sink search's candidates against one another — by what each tip holds that the other does not, its
//! **exclusive past**, and nothing else:
//!
//! 1. **participation** — the number of distinct executor bonds, registered in the two tips' COMMON PAST (the registry of their
//!    common selected-chain ancestor `F`), with an attempt claim in that exclusive past — once both tips stand at least
//!    [`PALW_RULE_E_PARTICIPATION_DEPTH_DAA_V1`] DAA above `F`;
//! 2. the economic keys `(safe frontier, safe weight, live total)` counted over the claims in that exclusive past;
//! 3. a tie: where participation counts and each side reaches a third of `F`'s bonds, GHOSTDAG's order; otherwise strict-win's
//!    shallow-tie question; otherwise the incumbent.
//!
//! **The exclusive past, exactly**: the claims a tip's own chain accepted ABOVE `F` (accepted blue score above `F`'s; chain blue
//! scores rise strictly) whose id the other tip's state does not hold. A claim both tips hold counts for neither side — a transition
//! that lands on one branch first decides nothing, and a public attempt one branch merges into its own past cancels — and a claim
//! accepted at or below `F` is common history. (It equals "carrying block outside the other tip's past" except where the other
//! tip's fold refused a merged attempt this tip accepted; that claim counts for this tip.) Claims are what the fold accepted, so a
//! losing lottery draw and an unbonded header never count; a bond registered after `F` never counts.
//!
//! **One computation, three callers.** Every claim is first reduced to a [`PalwRuleEClaimRecordV1`] (the fold's own price at the
//! tip), and a side is computed from records alone ([`palw_rule_e_side_from_records_v1`]). The virtual processor builds both tips'
//! records from its states and `F` from its chain; the IBD flow, which holds two consensus instances and no shared DAG, uses the
//! claim-set difference bounded by what both states still retain ([`palw_rule_e_sides_by_claim_set_v1`]); a header-verified
//! client reads the same records from the fork-choice leaf v2 ([`crate::palw_fork_choice_rule_e_leaf_v2`]). All three meet
//! [`palw_rule_e_decide_v1`] / [`palw_rule_e_order_v1`].
//!
//! **Dormant**: `None` on every preset and in no flag-day list; hashed Some-only into `consensus_params_id` and
//! `consensus_schedule_id`, collapsed whole from `Some(never())`. Arming it is refused by [`Params::validate_palw_fork_choice_rule_e_v1`]
//! in this binary ([`PALW_FORK_CHOICE_RULE_E_ARMABLE_V1`]), and — the user's ordering rule — so is `palw_dns_retirement_v1` unless
//! rule E is armed at or below it.

use crate::config::params::{ForkActivation, Params};
use crate::palw_fork_authority_v2::{PalwDeepReorgV2, PalwIbdCommitV2};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::{
    PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwFpPricingV1, PalwStateParamsV2,
    palw_claim_safe_contribution_v3, palw_fp_spend_weight_v1,
};
use crate::palw_weight_cap_v1::{
    palw_bond_weight_cap_v1, palw_staged_weight_v1, palw_weight_cap_applies_v1, palw_weight_final_safe_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use core::cmp::Ordering;
use kaspa_hashes::Hash64;
use std::collections::{BTreeMap, BTreeSet};

/// `W_p`: participation is consulted once BOTH tips stand at least this many DAA above their common chain ancestor (the lower
/// tip's span — for an incumbent facing a branch at least as long, its own history since the fork). At least the longest interval
/// an active honest bond goes without an attempt (so its signature lies above the fork), and at most the licence delay
/// (testnet-12: anchor delay 20, the receipts one DAA later) — so no self-licensed claim can exist in a branch's exclusive past
/// before participation outranks it.
pub const PALW_RULE_E_PARTICIPATION_DEPTH_DAA_V1: u64 = 20;

/// The most claims one side's exclusive past may hold for a node to weigh it. A side past it cannot be weighed: the reorg is
/// refused, the incumbent kept (fail closed). An honest exclusive past is bounded by the finality depth (testnet-12: 600 blue,
/// ≤ 300 DAA) at the attempt rate; this is orders of magnitude above that and bounds what a junk branch can make a node do.
pub const PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1: usize = 16_384;

/// The most candidates the sink search UTXO-validates and weighs AFTER its first acceptable one (GHOSTDAG's heaviest), highest
/// header-level participation first. The rest are left unweighed. Bounds what a flood of light branches can cost one resolve.
pub const PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1: usize = 8;

/// The most heap entries the search's continuation scores by header-level participation (headers and reachability only).
pub const PALW_RULE_E_MAX_SCORED_V1: usize = 256;

/// The most blocks one header-level participation score reads (the candidate's chain blocks above the fork and their mergesets).
/// An honest branch inside the finality depth (testnet-12: ≤ 300 DAA, a few blocks each) stays well below it.
pub const PALW_RULE_E_SCORE_WALK_V1: usize = 4_096;

/// The most selected-chain blocks a fork-span walk reads before it calls the fork deep. A tick holds a few chain blocks, so a
/// walk over `W_p` ticks stays far below it; past it the answer is the conservative one for the question asked.
pub const PALW_RULE_E_FORK_WALK_V1: usize = 4_096;

/// IBD only: how much earlier than one state a second state may have accepted the same claim (a block is merged within the merge
/// depth of the chain that accepts it, ≤ 30 blue score on testnet-12; this is twenty times that). A claim whose acceptance lies
/// closer than this to the other state's retirement horizon is not judged exclusive, since that state may hold and have retired it.
pub const PALW_RULE_E_IBD_RETENTION_MARGIN_DAA_V1: u64 = 600;

/// A peer's allowance of blocks below the merge-depth root at once: see [`PalwRuleEDeepRelayBudgetV1`].
pub const PALW_RULE_E_DEEP_RELAY_BURST_V1: u32 = 16;
/// One more block below the merge-depth root a peer may hand over per this many milliseconds.
pub const PALW_RULE_E_DEEP_RELAY_REFILL_MS_V1: u64 = 15_000;

/// This binary carries rule E's acceptance rule, but no network may arm it until the user assigns a height in the full-activation
/// release; `false` refuses every armed height.
pub const PALW_FORK_CHOICE_RULE_E_ARMABLE_V1: bool = false;

/// The refusal an armed rule E meets in this binary — named, so a test can tell it from every other refusal.
pub const PALW_FORK_CHOICE_RULE_E_UNARMABLE_V1: &str = "palw_fork_choice_rule_e_v1 cannot be armed in this binary: ADR-0175's rule E ships with the full-activation release, which assigns its height";

/// The ordering rule's refusals.
pub const PALW_DNS_RETIREMENT_NEEDS_RULE_E_V1: &str = "palw_dns_retirement_v1 needs palw_fork_choice_rule_e_v1 armed at or below it (ADR-0175: the DNS overlay's veto may retire only \
     once rule E decides deep reorgs)";
pub const PALW_RULE_E_ABOVE_DNS_RETIREMENT_V1: &str =
    "palw_fork_choice_rule_e_v1 is armed above palw_dns_retirement_v1 — rule E must be in force no later than the DNS veto retires";

impl Params {
    /// Whether rule E is in force at `daa_score` (the incumbent's DAA, at every reader).
    pub fn palw_fork_choice_rule_e_active_at(&self, daa_score: u64) -> bool {
        self.palw_fork_choice_rule_e_v1.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa_score))
    }

    /// **The ordering rule alone** (the user's, 2026-10-08): `palw_dns_retirement_v1` armed needs rule E armed at or below it.
    pub fn validate_palw_fork_choice_rule_e_order_v1(&self) -> Result<(), PalwModeV2Error> {
        let retirement = self.palw_dns_retirement.map(|r| r.activation).filter(|a| *a != ForkActivation::never());
        let rule_e = self.palw_fork_choice_rule_e_v1.filter(|a| *a != ForkActivation::never());
        match (retirement, rule_e) {
            (Some(_), None) => Err(PalwModeV2Error::Invalid(PALW_DNS_RETIREMENT_NEEDS_RULE_E_V1)),
            (Some(retirement), Some(rule_e)) if rule_e.daa_score() > retirement.daa_score() => {
                Err(PalwModeV2Error::Invalid(PALW_RULE_E_ABOVE_DNS_RETIREMENT_V1))
            }
            _ => Ok(()),
        }
    }

    /// **The fence's refusals**: the ordering rule, then any armed height while [`PALW_FORK_CHOICE_RULE_E_ARMABLE_V1`] is `false`.
    pub fn validate_palw_fork_choice_rule_e_v1(&self) -> Result<(), PalwModeV2Error> {
        self.validate_palw_fork_choice_rule_e_order_v1()?;
        match self.palw_fork_choice_rule_e_v1 {
            Some(f) if f != ForkActivation::never() && !PALW_FORK_CHOICE_RULE_E_ARMABLE_V1 => {
                Err(PalwModeV2Error::Invalid(PALW_FORK_CHOICE_RULE_E_UNARMABLE_V1))
            }
            _ => Ok(()),
        }
    }

    /// The V2 state params rule E weighs with — `None` off a ConsensusV2 network, where rule E has nothing to weigh.
    pub fn palw_rule_e_state_params_v1(&self) -> Option<&PalwStateParamsV2> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => Some(&bundle.state),
            _ => None,
        }
    }
}

/// One side's standing under rule E: what its exclusive past holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwRuleESideV1 {
    /// Distinct common bonds with an accepted attempt claim in the exclusive past.
    pub participation: u32,
    /// The accepted blue score of the deepest `Final` exclusive claim below the oldest unresolved exclusive claim (0: none).
    pub safe_frontier_blue_score: u64,
    /// The safe weight of the exclusive `Final` claims, priced as the fold prices it.
    pub safe_weight: u128,
    /// `safe_weight` plus the exclusive claims' bounded immature weight (F-W's per-bond cap applied over the exclusive set).
    pub live_total: u128,
    /// How many exclusive claims the side holds (for the cost bound and the report).
    pub exclusive_claims: u32,
}

impl PalwRuleESideV1 {
    pub fn economic(&self) -> (u64, u128, u128) {
        (self.safe_frontier_blue_score, self.safe_weight, self.live_total)
    }
}

/// Why a side could not be weighed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRuleEErrorV1 {
    /// The exclusive past holds more than [`PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1`] claims.
    TooManyExclusiveClaims,
    /// An arithmetic bound.
    Overflow,
    /// Not a ConsensusV2 network, or a state with no point to weigh at.
    NotWeighable,
}

/// A claim's phase as rule E reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwRuleEClaimStatusV1 {
    /// Any non-terminal phase.
    Live,
    Final,
    Voided,
}

/// **One claim, reduced to what rule E reads** — priced at the state it is read from, exactly as the fold prices it there. The
/// unit the node computes a side from and the unit a fork-choice leaf v2 commits, so the two cannot price a claim differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRuleEClaimRecordV1 {
    pub claim_id: Hash64,
    /// The blue score of the chain block that accepted it (the frontier's unit, and the "above `F`" test).
    pub accepted_blue_score: u64,
    /// The executor bond.
    pub bond: PalwBondKeyV2,
    /// An attempt claim (participation counts these only); a free-prompt claim otherwise.
    pub attempt: bool,
    pub status: PalwRuleEClaimStatusV1,
    /// `Final`: `palw_claim_safe_contribution_v3` under F-W's `palw_weight_final_safe_v1` for an attempt, the spent quanta's weight
    /// for a free-prompt claim. 0 otherwise.
    pub safe_weight: u128,
    /// `Live`: F-W's staged weight where the cap applies (`capped`), `immature_contribution` otherwise. 0 otherwise.
    pub live_weight: u128,
    /// The live weight goes through its bond's F-W cap.
    pub capped: bool,
    /// The bond's collateral at this state, where `capped` (the cap's input; `None`: no such bond, cap 0).
    pub bond_collateral: Option<u64>,
}

impl PalwChainStateV2 {
    /// **One claim's record at this state** — the fold's expressions: `palw_claim_safe_contribution_v3` and F-W's
    /// `palw_weight_final_safe_v1` for a `Final` attempt, the spent quanta for a free-prompt `Final`, F-W's staged weight for a live
    /// new-rule claim and `immature_contribution` for an old-rule one. `uncertified_weightless` and `canonical_work_daa` are the
    /// params' readings, taken once for every record of a comparison.
    pub fn palw_rule_e_record_v1(
        &self,
        params: &PalwStateParamsV2,
        uncertified_weightless: bool,
        canonical_work_daa: Option<u64>,
        shares: &BTreeMap<Hash64, u16>,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
    ) -> Result<PalwRuleEClaimRecordV1, PalwRuleEErrorV1> {
        let (status, safe_weight, live_weight, capped) = match &claim.phase {
            PalwClaimPhaseV2::Final { .. } => {
                let weight = match &claim.source {
                    PalwClaimSourceV2::Attempt => {
                        let canonical = self.palw_claim_canonical_weight_v1(claim, canonical_work_daa);
                        let contribution = palw_claim_safe_contribution_v3(shares, claim, uncertified_weightless, canonical);
                        palw_weight_final_safe_v1(params, claim, contribution)
                    }
                    PalwClaimSourceV2::FreePrompt { quanta, spent } => {
                        palw_fp_spend_weight_v1(self, claim, *quanta, &PalwFpPricingV1::of(params, canonical_work_daa))
                            .checked_mul(spent.len() as u128)
                            .ok_or(PalwRuleEErrorV1::Overflow)?
                    }
                };
                (PalwRuleEClaimStatusV1::Final, weight, 0, false)
            }
            PalwClaimPhaseV2::Voided { .. } => (PalwRuleEClaimStatusV1::Voided, 0, 0, false),
            _ if palw_weight_cap_applies_v1(params, claim) => {
                (PalwRuleEClaimStatusV1::Live, 0, palw_staged_weight_v1(params, claim), true)
            }
            _ => (PalwRuleEClaimStatusV1::Live, 0, claim.immature_contribution, false),
        };
        Ok(PalwRuleEClaimRecordV1 {
            claim_id,
            accepted_blue_score: claim.accepted_blue_score,
            bond: claim.bond,
            attempt: matches!(claim.source, PalwClaimSourceV2::Attempt),
            status,
            safe_weight,
            live_weight,
            capped,
            bond_collateral: if capped { self.bond(&claim.bond).map(|record| record.collateral) } else { None },
        })
    }

    /// **The records of this state's claims that `exclusive` admits** — one side's exclusive past. Fails past
    /// [`PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1`] (the caller fails closed).
    pub fn palw_rule_e_records_v1(
        &self,
        params: &PalwStateParamsV2,
        uncertified_weightless: bool,
        canonical_work_daa: Option<u64>,
        mut exclusive: impl FnMut(&Hash64, &PalwClaimStateV2) -> bool,
    ) -> Result<Vec<PalwRuleEClaimRecordV1>, PalwRuleEErrorV1> {
        let shares: BTreeMap<Hash64, u16> = self.class_shares_iter().map(|(k, v)| (*k, *v)).collect();
        let mut records = Vec::new();
        for (id, claim) in self.claims_iter() {
            if exclusive(id, claim) {
                if records.len() == PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1 {
                    return Err(PalwRuleEErrorV1::TooManyExclusiveClaims);
                }
                records.push(self.palw_rule_e_record_v1(params, uncertified_weightless, canonical_work_daa, &shares, *id, claim)?);
            }
        }
        Ok(records)
    }
}

/// **One side of rule E from its exclusive records** — the one place a side is computed, by the node and by a header-verified
/// client alike. `common_bond` says whether a bond counts toward participation (registered in the common past). Participation:
/// the distinct common bonds of attempt records; safe weight: the `Final` records' weights; live total: safe plus the uncapped
/// live weights plus, per bond, F-W's `min(Σ staged, W_cap(collateral))` over its capped live records; the frontier: the
/// deepest `Final` record below the oldest live one (the fold's resolved-prefix rule restricted to the records).
pub fn palw_rule_e_side_from_records_v1<'a>(
    records: impl IntoIterator<Item = &'a PalwRuleEClaimRecordV1>,
    common_bond: impl Fn(&PalwBondKeyV2) -> bool,
) -> Result<PalwRuleESideV1, PalwRuleEErrorV1> {
    let mut bonds: BTreeSet<PalwBondKeyV2> = BTreeSet::new();
    let (mut safe, mut immature, mut count) = (0u128, 0u128, 0usize);
    let mut staged: BTreeMap<PalwBondKeyV2, (u128, Option<u64>)> = BTreeMap::new();
    let mut oldest_live: Option<u64> = None;
    let mut finals: Vec<u64> = Vec::new();
    for record in records {
        count += 1;
        if count > PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1 {
            return Err(PalwRuleEErrorV1::TooManyExclusiveClaims);
        }
        if record.attempt && common_bond(&record.bond) {
            bonds.insert(record.bond);
        }
        match record.status {
            PalwRuleEClaimStatusV1::Final => {
                safe = safe.checked_add(record.safe_weight).ok_or(PalwRuleEErrorV1::Overflow)?;
                finals.push(record.accepted_blue_score);
            }
            PalwRuleEClaimStatusV1::Voided => {}
            PalwRuleEClaimStatusV1::Live => {
                oldest_live = Some(oldest_live.map_or(record.accepted_blue_score, |o| o.min(record.accepted_blue_score)));
                if record.capped {
                    let entry = staged.entry(record.bond).or_insert((0, record.bond_collateral));
                    entry.0 = entry.0.checked_add(record.live_weight).ok_or(PalwRuleEErrorV1::Overflow)?;
                } else {
                    immature = immature.checked_add(record.live_weight).ok_or(PalwRuleEErrorV1::Overflow)?;
                }
            }
        }
    }
    for (sum, collateral) in staged.values() {
        immature = immature
            .checked_add((*sum).min(collateral.map(palw_bond_weight_cap_v1).unwrap_or(0)))
            .ok_or(PalwRuleEErrorV1::Overflow)?;
    }
    let resolved_through = oldest_live.map(|oldest| oldest.saturating_sub(1)).unwrap_or(u64::MAX);
    let frontier = finals.into_iter().filter(|bs| *bs <= resolved_through).max().unwrap_or(0);
    Ok(PalwRuleESideV1 {
        participation: bonds.len() as u32,
        safe_frontier_blue_score: frontier,
        safe_weight: safe,
        live_total: safe.checked_add(immature).ok_or(PalwRuleEErrorV1::Overflow)?,
        exclusive_claims: count as u32,
    })
}

/// The bonds registered in both states — the IBD path's common past, where no fork block is known.
pub fn palw_rule_e_common_bonds_v1(a: &PalwChainStateV2, b: &PalwChainStateV2) -> BTreeSet<PalwBondKeyV2> {
    a.bonds_iter().map(|(k, _)| *k).filter(|k| b.bond(k).is_some()).collect()
}

/// The fewest participating bonds on EACH side for a participation tie to go to GHOSTDAG's order: a third of the common bonds,
/// rounded up, and at least one (a heartbeat-only period — zero on both sides — never reaches it).
pub fn palw_rule_e_even_split_min_v1(common_bonds: usize) -> u32 {
    u32::try_from(common_bonds).unwrap_or(u32::MAX).div_ceil(3).max(1)
}

/// Whether participation counts for a fork whose LOWER tip stands `lower_span_daa` DAA above the common chain ancestor.
pub fn palw_rule_e_participation_counts_v1(lower_span_daa: u64) -> bool {
    lower_span_daa >= PALW_RULE_E_PARTICIPATION_DEPTH_DAA_V1
}

/// [`palw_rule_e_participation_counts_v1`] from the two tips' and their common selected-chain ancestor's DAA scores (what a
/// header-verified client reads).
pub fn palw_rule_e_participation_counts_from_daas_v1(daa_a: u64, daa_b: u64, daa_fork: u64) -> bool {
    palw_rule_e_participation_counts_v1(daa_a.min(daa_b).saturating_sub(daa_fork))
}

/// The two sides of one pair and the even-split threshold — what every path hands [`palw_rule_e_decide_v1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRuleEPairV1 {
    pub a: PalwRuleESideV1,
    pub b: PalwRuleESideV1,
    pub even_split_min: u32,
}

/// **Both sides of rule E for two states**, each restricted to the claims its predicate admits AND the other state does not hold
/// (a claim both tips hold — by id — decides nothing, wherever and whenever each accepted it). `common_bond` and `common_bonds` are
/// the common past's registry (participation's bonds and the even split's `n`). Each side is priced at its own state, with the
/// weight fence read at that state's own point (`uncertified_weightless_a` / `_b`) — as a fork-choice leaf commits it. The one
/// place every node path builds a pair.
#[allow(clippy::too_many_arguments)]
pub fn palw_rule_e_sides_v1(
    a: &PalwChainStateV2,
    b: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    uncertified_weightless_a: bool,
    uncertified_weightless_b: bool,
    canonical_work_daa: Option<u64>,
    mut a_exclusive: impl FnMut(&Hash64, &PalwClaimStateV2) -> bool,
    mut b_exclusive: impl FnMut(&Hash64, &PalwClaimStateV2) -> bool,
    common_bond: impl Fn(&PalwBondKeyV2) -> bool,
    common_bonds: usize,
) -> Result<PalwRuleEPairV1, PalwRuleEErrorV1> {
    let records_a = a.palw_rule_e_records_v1(params, uncertified_weightless_a, canonical_work_daa, |id, claim| {
        b.claim(id).is_none() && a_exclusive(id, claim)
    })?;
    let records_b = b.palw_rule_e_records_v1(params, uncertified_weightless_b, canonical_work_daa, |id, claim| {
        a.claim(id).is_none() && b_exclusive(id, claim)
    })?;
    Ok(PalwRuleEPairV1 {
        a: palw_rule_e_side_from_records_v1(&records_a, &common_bond)?,
        b: palw_rule_e_side_from_records_v1(&records_b, &common_bond)?,
        even_split_min: palw_rule_e_even_split_min_v1(common_bonds),
    })
}

/// **The relay path's pair**: `a` and `b` with their common selected-chain ancestor's state `fork` (its registry is the common
/// past's) and blue score — each side's claims accepted above the fork that the other does not hold.
#[allow(clippy::too_many_arguments)]
pub fn palw_rule_e_sides_above_fork_v1(
    a: &PalwChainStateV2,
    b: &PalwChainStateV2,
    fork: &PalwChainStateV2,
    fork_blue_score: u64,
    params: &PalwStateParamsV2,
    uncertified_weightless_a: bool,
    uncertified_weightless_b: bool,
    canonical_work_daa: Option<u64>,
) -> Result<PalwRuleEPairV1, PalwRuleEErrorV1> {
    let above = |_: &Hash64, claim: &PalwClaimStateV2| claim.accepted_blue_score > fork_blue_score;
    palw_rule_e_sides_v1(
        a,
        b,
        params,
        uncertified_weightless_a,
        uncertified_weightless_b,
        canonical_work_daa,
        above,
        above,
        |k| fork.bond(k).is_some(),
        fork.bonds_iter().count(),
    )
}

/// **Rule E's history-free order** between two sides: participation first where it counts, then the economic keys.
pub fn palw_rule_e_order_v1(a: &PalwRuleESideV1, b: &PalwRuleESideV1, participation_counts: bool) -> Ordering {
    let economic = a.economic().cmp(&b.economic());
    if participation_counts { a.participation.cmp(&b.participation).then(economic) } else { economic }
}

/// **Rule E's deep-reorg decision**: may `challenger` replace `incumbent`? A strict win in [`palw_rule_e_order_v1`] allows, a loss
/// refuses; a tie goes to GHOSTDAG's order (`ghostdag_heavier`) where participation counts and both sides reach `even_split_min`,
/// else to strict-win's shallow-tie question (`shallow_ghostdag_win`), else keeps the incumbent. Each question is asked only on the
/// tie that needs it.
pub fn palw_rule_e_decide_v1(
    incumbent: &PalwRuleESideV1,
    challenger: &PalwRuleESideV1,
    participation_counts: bool,
    even_split_min: u32,
    ghostdag_heavier: impl FnOnce() -> bool,
    shallow_ghostdag_win: impl FnOnce() -> bool,
) -> PalwDeepReorgV2 {
    let allow = |yes: bool| if yes { PalwDeepReorgV2::Allow } else { PalwDeepReorgV2::Refuse };
    match palw_rule_e_order_v1(challenger, incumbent, participation_counts) {
        Ordering::Greater => PalwDeepReorgV2::Allow,
        Ordering::Less => PalwDeepReorgV2::Refuse,
        Ordering::Equal
            if participation_counts && incumbent.participation >= even_split_min && challenger.participation >= even_split_min =>
        {
            allow(ghostdag_heavier())
        }
        Ordering::Equal => allow(shallow_ghostdag_win()),
    }
}

/// **Both sides from two states that share no DAG** (the IBD flow's staged chain against the local one). A claim of one state is
/// exclusive iff the other holds no claim of its id AND the other could not have retired it. A state at DAA `D` still holds every
/// claim it accepted with `terminal + claim_retirement_daa > D`, and `terminal ≥` its own acceptance `≥` this state's acceptance
/// less the merge slack [`PALW_RULE_E_IBD_RETENTION_MARGIN_DAA_V1`] — so a claim with `accepted + claim_retirement_daa > D + slack`
/// would still be there. An older claim is neither side's: the two states cannot tell whether they share it. `a_daa`/`b_daa` are
/// the DAA scores the states stand at.
pub fn palw_rule_e_sides_by_claim_set_v1(
    a: &PalwChainStateV2,
    a_daa: u64,
    b: &PalwChainStateV2,
    b_daa: u64,
    params: &PalwStateParamsV2,
    uncertified_weightless_a: bool,
    uncertified_weightless_b: bool,
    canonical_work_daa: Option<u64>,
) -> Result<PalwRuleEPairV1, PalwRuleEErrorV1> {
    let retained_by = |other_daa: u64| {
        let retirement = params.claim_retirement_daa();
        move |_: &Hash64, claim: &PalwClaimStateV2| {
            retirement == 0
                || claim.accepted_daa.saturating_add(retirement) > other_daa.saturating_add(PALW_RULE_E_IBD_RETENTION_MARGIN_DAA_V1)
        }
    };
    let common = palw_rule_e_common_bonds_v1(a, b);
    palw_rule_e_sides_v1(
        a,
        b,
        params,
        uncertified_weightless_a,
        uncertified_weightless_b,
        canonical_work_daa,
        retained_by(b_daa),
        retained_by(a_daa),
        |k| common.contains(k),
        common.len(),
    )
}

/// **A consensus instance's standing, as the IBD flow hands it to rule E**: the state at the block the instance is weighed at
/// (its sink; a staging consensus's imported pruning point) and that block's DAA score.
#[derive(Clone, Debug)]
pub struct PalwRuleEWeighingV1 {
    pub state: PalwChainStateV2,
    pub block: Hash64,
    pub daa_score: u64,
}

/// **Rule E's IBD commit** — the same comparator as the relay path's gate, over the claim-set difference. Participation counts:
/// the headers-proof IBD runs only for a peer whose chain shares no block this node knows within a pruning depth, or across this
/// node's own pruning point under a recovery permit, so the fork is far deeper than `W_p` by the path's own entry conditions. A
/// tie keeps the incumbent: IBD never commits on blue work, not even at an even split (the relay path decides those once the
/// blocks arrive). An unweighable pair keeps the incumbent (the caller's fail-closed arm).
pub fn palw_rule_e_ibd_commit_v1(
    params: &Params,
    incumbent: &PalwRuleEWeighingV1,
    challenger: &PalwRuleEWeighingV1,
) -> Result<(PalwIbdCommitV2, PalwRuleEPairV1), PalwRuleEErrorV1> {
    let state_params = params.palw_rule_e_state_params_v1().ok_or(PalwRuleEErrorV1::NotWeighable)?;
    let weightless = |daa: u64| params.palw_uncertified_weightless.is_some_and(|f| f.is_active(daa));
    let pair = palw_rule_e_sides_by_claim_set_v1(
        &challenger.state,
        challenger.daa_score,
        &incumbent.state,
        incumbent.daa_score,
        state_params,
        weightless(challenger.daa_score),
        weightless(incumbent.daa_score),
        params.palw_canonical_work_daa(),
    )?;
    let decision = palw_rule_e_decide_v1(&pair.b, &pair.a, true, pair.even_split_min, || false, || false);
    let commit = match decision {
        PalwDeepReorgV2::Allow => PalwIbdCommitV2::Commit,
        PalwDeepReorgV2::Refuse => PalwIbdCommitV2::KeepIncumbent,
    };
    Ok((commit, pair))
}

/// **The relay's question under rule E: may a block below the virtual's merge-depth root be validated rather than skipped?** The
/// status quo skips every such block (it cannot be merged), so a node never holds — and its sink search never weighs — a lighter
/// branch longer than about the merge depth. Past the fence (read at this node's virtual DAA) a block that is not a heartbeat is
/// taken: an attempt block is what participation counts, and its missing ancestors come in as orphan roots, so the branch is
/// fetched down to the fork. A heartbeat below the root is still skipped — a heartbeat-only branch carries no participation, and
/// one that later carries an attempt is fetched then. The per-peer budget ([`PalwRuleEDeepRelayBudgetV1`]) bounds what a peer's
/// junk below the root can cost; the sink search's own bound ([`PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1`]) bounds what it costs a
/// resolve.
pub fn palw_rule_e_relay_weighs_below_merge_root_v1(params: &Params, virtual_daa: u64, pow_algo_id: u8) -> bool {
    params.palw_fork_choice_rule_e_active_at(virtual_daa) && pow_algo_id != crate::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1
}

/// **A peer's allowance of blocks below the merge-depth root** (rule E's relay): [`PALW_RULE_E_DEEP_RELAY_BURST_V1`] at once, and
/// one more per [`PALW_RULE_E_DEEP_RELAY_REFILL_MS_V1`]. An honest branch below the root needs its attempt blocks, a few a slot
/// from the whole network; the rest of it arrives as orphan roots, which the allowance does not meter.
#[derive(Clone, Copy, Debug)]
pub struct PalwRuleEDeepRelayBudgetV1 {
    tokens: u32,
    last_refill_ms: u64,
}

impl PalwRuleEDeepRelayBudgetV1 {
    pub fn new(now_ms: u64) -> Self {
        Self { tokens: PALW_RULE_E_DEEP_RELAY_BURST_V1, last_refill_ms: now_ms }
    }

    /// Take one block's allowance at `now_ms`; `false` when the peer has none left.
    pub fn take(&mut self, now_ms: u64) -> bool {
        let earned = now_ms.saturating_sub(self.last_refill_ms) / PALW_RULE_E_DEEP_RELAY_REFILL_MS_V1;
        if earned > 0 {
            self.tokens = self.tokens.saturating_add(u32::try_from(earned).unwrap_or(u32::MAX)).min(PALW_RULE_E_DEEP_RELAY_BURST_V1);
            self.last_refill_ms = self.last_refill_ms.saturating_add(earned.saturating_mul(PALW_RULE_E_DEEP_RELAY_REFILL_MS_V1));
        }
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};
    use crate::palw_native_settlement_v1::{PalwDnsRetirementV1, PalwSettlementPolicyV1};

    fn retirement(at: u64) -> PalwDnsRetirementV1 {
        PalwDnsRetirementV1 {
            activation: ForkActivation::new(at),
            settlement: PalwSettlementPolicyV1 {
                settled_anchor_depth: 2,
                unique_mature_work: 20,
                max_operator_permille: 1000,
                max_class_permille: 1000,
            },
            legacy_evidence_horizon_daa: 10,
        }
    }

    #[test]
    fn the_fence_is_dormant_everywhere_hashed_some_only_and_refused_when_armed() {
        for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
            assert_eq!(p.palw_fork_choice_rule_e_v1, None);
            assert!(!p.palw_fork_choice_rule_e_active_at(u64::MAX));
            p.validate_palw_fork_choice_rule_e_v1().unwrap();
        }
        let shipped = crate::config::params::palw_t12_shipped_params();
        assert_eq!(shipped.palw_fork_choice_rule_e_v1, None, "testnet-12 as shipped does not schedule it");
        assert!(shipped.palw_fences_v1().iter().any(|(n, f)| *n == "palw_fork_choice_rule_e_v1" && f.is_none()), "listed, dormant");
        let mut p = TESTNET_PARAMS;
        let (ids, identity) = ((p.consensus_params_id(), p.consensus_schedule_id()), p.consensus_identity_id());
        p.palw_fork_choice_rule_e_v1 = Some(ForkActivation::never());
        assert_eq!(p.consensus_identity_id(), identity, "Some(never()) collapses whole in the identity");
        p.validate_palw_fork_choice_rule_e_v1().unwrap();
        p.palw_fork_choice_rule_e_v1 = Some(ForkActivation::new(1_000));
        assert_eq!(p.validate_palw_fork_choice_rule_e_v1(), Err(PalwModeV2Error::Invalid(PALW_FORK_CHOICE_RULE_E_UNARMABLE_V1)));
        assert_ne!((p.consensus_params_id(), p.consensus_schedule_id()), ids, "an armed height would be a different network");
        assert!(p.palw_fork_choice_rule_e_active_at(1_000) && !p.palw_fork_choice_rule_e_active_at(999));
    }

    /// **The user's ordering rule, each case** (2026-10-08): retirement without rule E is refused; rule E above it is refused;
    /// rule E at or below it passes the ordering rule (and meets only the fence's own refusal in this binary).
    #[test]
    fn the_dns_retirement_needs_rule_e_at_or_below_it() {
        let mut p = TESTNET_PARAMS;
        p.palw_dns_retirement = Some(retirement(9_000));
        assert_eq!(p.validate_palw_fork_choice_rule_e_order_v1(), Err(PalwModeV2Error::Invalid(PALW_DNS_RETIREMENT_NEEDS_RULE_E_V1)));
        assert_eq!(p.validate_palw_fork_choice_rule_e_v1(), Err(PalwModeV2Error::Invalid(PALW_DNS_RETIREMENT_NEEDS_RULE_E_V1)));
        p.palw_fork_choice_rule_e_v1 = Some(ForkActivation::never());
        assert_eq!(p.validate_palw_fork_choice_rule_e_order_v1(), Err(PalwModeV2Error::Invalid(PALW_DNS_RETIREMENT_NEEDS_RULE_E_V1)));
        p.palw_fork_choice_rule_e_v1 = Some(ForkActivation::new(9_001));
        assert_eq!(p.validate_palw_fork_choice_rule_e_order_v1(), Err(PalwModeV2Error::Invalid(PALW_RULE_E_ABOVE_DNS_RETIREMENT_V1)));
        for at in [9_000u64, 8_999, 0] {
            p.palw_fork_choice_rule_e_v1 = Some(ForkActivation::new(at));
            p.validate_palw_fork_choice_rule_e_order_v1().unwrap_or_else(|e| panic!("rule E at {at} ≤ 9,000: {e:?}"));
            assert_eq!(
                p.validate_palw_fork_choice_rule_e_v1(),
                Err(PalwModeV2Error::Invalid(PALW_FORK_CHOICE_RULE_E_UNARMABLE_V1)),
                "past the ordering rule, only the fence's own refusal"
            );
        }
        // A retirement collapsed to `never()` asks nothing of rule E.
        p.palw_dns_retirement = Some(retirement(u64::MAX));
        p.palw_fork_choice_rule_e_v1 = None;
        p.validate_palw_fork_choice_rule_e_v1().unwrap();
    }

    /// **`validate_palw_v2` reaches the rule-E refusals on testnet-12**, and last: an armed retirement with no rule E is refused
    /// by the ordering rule's own name, and one with rule E at its height meets only the fence's refusal.
    #[test]
    fn validate_palw_v2_asks_the_ordering_rule_last() {
        let mut p = crate::config::params::palw_t12_shipped_params();
        p.validate_palw_v2().expect("testnet-12 as shipped validates");
        p.palw_dns_retirement = Some(retirement(9_000));
        assert_eq!(p.validate_palw_v2(), Err(PalwModeV2Error::Invalid(PALW_DNS_RETIREMENT_NEEDS_RULE_E_V1)));
        p.palw_fork_choice_rule_e_v1 = Some(ForkActivation::new(9_000));
        assert_eq!(p.validate_palw_v2(), Err(PalwModeV2Error::Invalid(PALW_FORK_CHOICE_RULE_E_UNARMABLE_V1)));
        p.palw_dns_retirement = None;
        assert_eq!(p.validate_palw_v2(), Err(PalwModeV2Error::Invalid(PALW_FORK_CHOICE_RULE_E_UNARMABLE_V1)));
    }

    fn side(participation: u32, frontier: u64, safe: u128, live: u128) -> PalwRuleESideV1 {
        PalwRuleESideV1 { participation, safe_frontier_blue_score: frontier, safe_weight: safe, live_total: live, exclusive_claims: 0 }
    }

    #[test]
    fn participation_outranks_the_economic_keys_once_it_counts_and_not_before() {
        let never = || -> bool { panic!("a strict order asks no tie question") };
        let (honest, colluder) = (side(5, 0, 0, 0), side(3, 9, 9_000, 9_000));
        assert_eq!(palw_rule_e_decide_v1(&honest, &colluder, true, 3, never, never), PalwDeepReorgV2::Refuse);
        assert_eq!(palw_rule_e_decide_v1(&colluder, &honest, true, 3, never, never), PalwDeepReorgV2::Allow);
        // Shallower than W_p the economic keys decide, as strict-win.
        assert_eq!(palw_rule_e_decide_v1(&honest, &colluder, false, 3, never, never), PalwDeepReorgV2::Allow);
        assert!(!palw_rule_e_participation_counts_v1(PALW_RULE_E_PARTICIPATION_DEPTH_DAA_V1 - 1));
        assert!(palw_rule_e_participation_counts_v1(PALW_RULE_E_PARTICIPATION_DEPTH_DAA_V1));
    }

    #[test]
    fn a_tie_goes_to_ghostdag_only_on_an_even_split_of_a_third_and_otherwise_to_the_shallow_question() {
        let even = side(4, 0, 0, 0);
        assert_eq!(palw_rule_e_decide_v1(&even, &even, true, 3, || true, || false), PalwDeepReorgV2::Allow);
        assert_eq!(palw_rule_e_decide_v1(&even, &even, true, 3, || false, || true), PalwDeepReorgV2::Refuse);
        // A heartbeat-only period: zero on both sides never reaches the split — the incumbent is kept unless shallow.
        let none = side(0, 0, 0, 0);
        assert_eq!(palw_rule_e_decide_v1(&none, &none, true, 3, || panic!("not an even split"), || false), PalwDeepReorgV2::Refuse);
        assert_eq!(palw_rule_e_decide_v1(&none, &none, true, 3, || panic!("not an even split"), || true), PalwDeepReorgV2::Allow);
        // Below W_p a tie is strict-win's.
        assert_eq!(
            palw_rule_e_decide_v1(&even, &even, false, 3, || panic!("participation does not count"), || false),
            PalwDeepReorgV2::Refuse
        );
        assert_eq!(palw_rule_e_even_split_min_v1(8), 3);
        assert_eq!(palw_rule_e_even_split_min_v1(0), 1);
    }

    /// The order is antisymmetric: what `a` wins against `b`, `b` loses against `a` — so the search's pairwise choice and the
    /// gate's answer cannot disagree about a pair.
    #[test]
    fn the_order_is_antisymmetric() {
        let sides = [side(0, 0, 0, 0), side(2, 0, 0, 0), side(2, 5, 0, 0), side(1, 9, 9, 9), side(2, 5, 7, 7)];
        for a in &sides {
            for b in &sides {
                for counts in [false, true] {
                    assert_eq!(palw_rule_e_order_v1(a, b, counts), palw_rule_e_order_v1(b, a, counts).reverse());
                }
            }
        }
    }

    /// The side from records: participation over common bonds' attempts only; a void weighs nothing; the frontier is the deepest
    /// `Final` below the oldest live record; F-W's per-bond cap applies to the summed staged weight of each bond's capped records.
    #[test]
    fn a_side_from_records_is_the_folds_arithmetic_over_the_exclusive_set() {
        use crate::tx::TransactionOutpoint;
        let bond = |i: u32| PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(3), i));
        let rec = |id: u64, bs: u64, b: u32, attempt: bool, status: PalwRuleEClaimStatusV1, safe: u128, live: u128, capped: bool| {
            PalwRuleEClaimRecordV1 {
                claim_id: Hash64::from_u64_word(id),
                accepted_blue_score: bs,
                bond: bond(b),
                attempt,
                status,
                safe_weight: safe,
                live_weight: live,
                capped,
                bond_collateral: if capped { Some(13_000 * 100_000_000) } else { None },
            }
        };
        use PalwRuleEClaimStatusV1::{Final, Live, Voided};
        let cap = palw_bond_weight_cap_v1(13_000 * 100_000_000);
        let records = [
            rec(1, 10, 1, true, Final, 100, 0, false),
            rec(2, 12, 2, true, Final, 50, 0, false),
            rec(3, 15, 3, true, Live, 0, 7, false),
            rec(4, 11, 4, true, Voided, 0, 0, false),
            rec(5, 16, 5, true, Live, 0, cap, true),
            rec(6, 17, 5, true, Live, 0, cap, true),
            rec(7, 18, 6, false, Live, 0, 3, false),
            rec(8, 19, 9, true, Live, 0, 0, false),
        ];
        let side = palw_rule_e_side_from_records_v1(&records, |k| *k != bond(9)).unwrap();
        assert_eq!(side.participation, 5, "bonds 1–5 (bond 6's record is free-prompt, bond 9 is not common)");
        assert_eq!(side.safe_weight, 150);
        assert_eq!(side.live_total, 150 + 7 + 3 + cap, "bond 5's two capped records meet one cap");
        assert_eq!(side.safe_frontier_blue_score, 12, "the deepest Final below the oldest live record (15)");
        assert_eq!(side.exclusive_claims, 8);
    }

    #[test]
    fn the_relay_weighs_attempts_below_the_merge_root_only_past_the_fence_and_the_budget_bounds_a_peer() {
        use crate::pow_layer0::{POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_PALW_COMMITTED_V2};
        let mut p = crate::config::params::palw_t12_shipped_params();
        // Unarmed: the status quo — every block below the root is skipped.
        assert!(!palw_rule_e_relay_weighs_below_merge_root_v1(&p, 10_000, POW_ALGO_ID_PALW_COMMITTED_V2));
        p.palw_fork_choice_rule_e_v1 = Some(ForkActivation::new(100));
        assert!(!palw_rule_e_relay_weighs_below_merge_root_v1(&p, 99, POW_ALGO_ID_PALW_COMMITTED_V2), "below the fence");
        assert!(palw_rule_e_relay_weighs_below_merge_root_v1(&p, 100, POW_ALGO_ID_PALW_COMMITTED_V2), "an attempt block, past it");
        assert!(!palw_rule_e_relay_weighs_below_merge_root_v1(&p, 100, POW_ALGO_ID_HEARTBEAT_V1), "a heartbeat is still skipped");
        // A peer flooding blocks below the root gets its burst, then one per refill.
        let mut budget = PalwRuleEDeepRelayBudgetV1::new(0);
        let at_once = (0..1_000).filter(|_| budget.take(0)).count();
        assert_eq!(at_once, PALW_RULE_E_DEEP_RELAY_BURST_V1 as usize);
        let over_an_hour = (1..=3_600u64).filter(|s| budget.take(s * 1_000)).count();
        assert_eq!(over_an_hour as u64, 3_600_000 / PALW_RULE_E_DEEP_RELAY_REFILL_MS_V1);
    }
}
