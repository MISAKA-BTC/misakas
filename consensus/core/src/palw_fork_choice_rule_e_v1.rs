//! **ADR-0175: rule E — the fork choice that heals a partition on bonded participation (`palw_fork_choice_rule_e_v1`).**
//!
//! Past the fence (read at the INCUMBENT's DAA, as every fork-choice fence here is) a non-extension candidate is weighed against
//! the incumbent by what each tip holds that the other does not — its **exclusive past** — and nothing else:
//!
//! 1. **participation** — the number of distinct bonds, registered in BOTH tips' registries, that signed an attempt the chain
//!    accepted as a claim in that exclusive past — once the incumbent's side of the fork spans at least
//!    [`PALW_RULE_E_PARTICIPATION_DEPTH_DAA_V1`] DAA;
//! 2. the economic keys `(safe frontier, safe weight, live total)` counted over the claims accepted in that exclusive past;
//! 3. where participation ties at no fewer than a third of the common bonds on each side, GHOSTDAG's order; otherwise strict-win's
//!    shallow-tie question; otherwise the incumbent.
//!
//! A claim both tips hold — accepted in their common past, wherever its licence, `Final` or void landed — counts for neither side,
//! so a transition that merely lands on one branch first decides nothing, and a public attempt an attacker merges into its own
//! branch cancels out. Claims are what the fold accepted, so a losing lottery draw and an unbonded header never count; a bond
//! minted on one side only is in one registry only and never counts.
//!
//! Exclusivity is a predicate the caller supplies: the processor asks reachability (a claim is exclusive to `c` iff its accepted
//! block is not in `s`'s past); the IBD flow, which holds two consensus instances, asks the claim-id set difference
//! ([`palw_rule_e_sides_by_claim_set_v1`]). Both are the same set whenever the claims involved are retained, which every claim
//! accepted inside the finality depth is (retirement is `Final + claim_retirement`, far deeper). One comparator, so the relay
//! path and the IBD path decide a pair alike.
//!
//! **Dormant**: `None` on every preset and in no flag-day list; hashed Some-only into `consensus_params_id` and
//! `consensus_schedule_id`, collapsed whole from `Some(never())`. Arming it is refused by [`Params::validate_palw_fork_choice_rule_e_v1`]
//! in this binary ([`PALW_FORK_CHOICE_RULE_E_ARMABLE_V1`]), and — the user's ordering rule — so is `palw_dns_retirement_v1` unless
//! rule E is armed at or below it: today the DNS BFT veto is the only layer bounding the stale-incumbent comparison rule E removes.

use crate::config::params::{ForkActivation, Params};
use crate::palw_fork_authority_v2::PalwDeepReorgV2;
use crate::palw_mode_v2::PalwModeV2Error;
use crate::palw_state_v2::{
    PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwFpPricingV1, PalwStateParamsV2,
    palw_claim_safe_contribution_v3, palw_fp_spend_weight_v1,
};
use crate::palw_weight_cap_v1::{PalwBondWeightIndexV1, palw_weight_cap_applies_v1, palw_weight_final_safe_v1};
use core::cmp::Ordering;
use kaspa_hashes::Hash64;
use std::collections::{BTreeMap, BTreeSet};

/// `W_p`: participation is consulted once the incumbent's side of the fork spans at least this many DAA. At least the longest
/// interval an active honest bond goes without an attempt (so its signature lies above the fork), and at most the licence delay
/// (testnet-12: anchor delay 20, the receipts one DAA later) — so no self-licensed claim can exist on a private branch before
/// participation outranks it.
pub const PALW_RULE_E_PARTICIPATION_DEPTH_DAA_V1: u64 = 20;

/// The most claims one side's exclusive past may hold for the node to weigh it. A side past it cannot be weighed — the reorg is
/// refused, the incumbent kept (fail closed). An honest exclusive past is bounded by the finality depth (testnet-12: 600 blue,
/// ≤ 300 DAA) at the attempt rate; this is orders of magnitude above that and bounds what a junk branch can make a node do.
pub const PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1: usize = 16_384;

/// The most non-extension candidates the sink search UTXO-validates after its first acceptable candidate (the rest of the heap is
/// left unweighed). Bounds the search a flood of light branches can cost a resolve.
pub const PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1: usize = 8;

/// This binary carries rule E's acceptance rule, but no network may arm it until the user assigns a height in the full-activation
/// release; `false` refuses every armed height.
pub const PALW_FORK_CHOICE_RULE_E_ARMABLE_V1: bool = false;

/// The refusal an armed rule E meets in this binary — named, so a test can tell it from every other refusal.
pub const PALW_FORK_CHOICE_RULE_E_UNARMABLE_V1: &str =
    "palw_fork_choice_rule_e_v1 cannot be armed in this binary: ADR-0175's rule E ships with the full-activation release, which assigns its height";

/// The ordering rule's refusals.
pub const PALW_DNS_RETIREMENT_NEEDS_RULE_E_V1: &str = "palw_dns_retirement_v1 needs palw_fork_choice_rule_e_v1 armed at or below it (ADR-0175: the DNS BFT veto is \
     the only layer bounding the stale-incumbent comparison until rule E replaces it)";
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
}

impl PalwChainStateV2 {
    /// **One side of rule E**: this state's claims that `exclusive` admits (its exclusive past against the other tip), weighed
    /// with the fold's own expressions — `palw_claim_safe_contribution_v3` and F-W's `palw_weight_final_safe_v1` for a `Final`
    /// attempt, the spent quanta for a free-prompt `Final`, `immature_contribution` for an old-rule live claim and F-W's capped
    /// per-bond term (over the exclusive claims only) for a new-rule one. `common_bond` says whether a bond is registered in both
    /// tips. `uncertified_weightless` and `canonical_work_daa` are the params' readings at this chain point, as the consistency
    /// check takes them.
    pub fn palw_rule_e_side_v1(
        &self,
        params: &PalwStateParamsV2,
        uncertified_weightless: bool,
        canonical_work_daa: Option<u64>,
        mut exclusive: impl FnMut(&Hash64, &PalwClaimStateV2) -> bool,
        common_bond: impl Fn(&PalwBondKeyV2) -> bool,
    ) -> Result<PalwRuleESideV1, PalwRuleEErrorV1> {
        let mut claims: Vec<&PalwClaimStateV2> = Vec::new();
        for (id, claim) in self.claims_iter() {
            if exclusive(id, claim) {
                if claims.len() == PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1 {
                    return Err(PalwRuleEErrorV1::TooManyExclusiveClaims);
                }
                claims.push(claim);
            }
        }
        let shares: BTreeMap<Hash64, u16> = self.class_shares_iter().map(|(k, v)| (*k, *v)).collect();
        let pricing = PalwFpPricingV1::of(params, canonical_work_daa);
        let mut bonds: BTreeSet<PalwBondKeyV2> = BTreeSet::new();
        let (mut safe, mut immature) = (0u128, 0u128);
        let mut capped: Vec<&PalwClaimStateV2> = Vec::new();
        for claim in &claims {
            if matches!(claim.source, PalwClaimSourceV2::Attempt) && common_bond(&claim.bond) {
                bonds.insert(claim.bond);
            }
            match &claim.phase {
                PalwClaimPhaseV2::Final { .. } => {
                    let weight = match &claim.source {
                        PalwClaimSourceV2::Attempt => {
                            let canonical = self.palw_claim_canonical_weight_v1(claim, canonical_work_daa);
                            let contribution = palw_claim_safe_contribution_v3(&shares, claim, uncertified_weightless, canonical);
                            palw_weight_final_safe_v1(params, claim, contribution)
                        }
                        PalwClaimSourceV2::FreePrompt { quanta, spent } => palw_fp_spend_weight_v1(self, claim, *quanta, &pricing)
                            .checked_mul(spent.len() as u128)
                            .ok_or(PalwRuleEErrorV1::Overflow)?,
                    };
                    safe = safe.checked_add(weight).ok_or(PalwRuleEErrorV1::Overflow)?;
                }
                PalwClaimPhaseV2::Voided { .. } => {}
                _ if palw_weight_cap_applies_v1(params, claim) => capped.push(claim),
                _ => immature = immature.checked_add(claim.immature_contribution).ok_or(PalwRuleEErrorV1::Overflow)?,
            }
        }
        if !capped.is_empty() {
            let index = PalwBondWeightIndexV1::of(params, capped.into_iter());
            immature = immature.checked_add(index.capped_total(self)).ok_or(PalwRuleEErrorV1::Overflow)?;
        }
        // The frontier, under the fold's resolved-prefix rule restricted to the exclusive claims.
        let resolved_through = claims
            .iter()
            .filter(|c| !c.phase.is_terminal())
            .map(|c| c.accepted_blue_score)
            .min()
            .map(|oldest| oldest.saturating_sub(1))
            .unwrap_or(u64::MAX);
        let frontier = claims
            .iter()
            .filter(|c| matches!(c.phase, PalwClaimPhaseV2::Final { .. }) && c.accepted_blue_score <= resolved_through)
            .map(|c| c.accepted_blue_score)
            .max()
            .unwrap_or(0);
        Ok(PalwRuleESideV1 {
            participation: bonds.len() as u32,
            safe_frontier_blue_score: frontier,
            safe_weight: safe,
            live_total: safe.checked_add(immature).ok_or(PalwRuleEErrorV1::Overflow)?,
            exclusive_claims: claims.len() as u32,
        })
    }
}

/// The bonds registered in both states — the only bonds whose attempts count as participation.
pub fn palw_rule_e_common_bonds_v1(a: &PalwChainStateV2, b: &PalwChainStateV2) -> BTreeSet<PalwBondKeyV2> {
    a.bonds_iter().map(|(k, _)| *k).filter(|k| b.bond(k).is_some()).collect()
}

/// The fewest participating bonds on EACH side for a participation tie to go to GHOSTDAG's order: a third of the common bonds,
/// rounded up, and at least one (a heartbeat-only period — zero on both sides — never reaches it).
pub fn palw_rule_e_even_split_min_v1(common_bonds: usize) -> u32 {
    (common_bonds as u32).div_ceil(3).max(1)
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

/// **Both sides from two states that share no DAG** (the IBD flow's staged chain against the local one): a claim is exclusive to a
/// side iff the other state holds no claim of the same id accepted in the same block.
pub fn palw_rule_e_sides_by_claim_set_v1(
    a: &PalwChainStateV2,
    b: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    uncertified_weightless: bool,
    canonical_work_daa: Option<u64>,
) -> Result<(PalwRuleESideV1, PalwRuleESideV1, u32), PalwRuleEErrorV1> {
    let common = palw_rule_e_common_bonds_v1(a, b);
    let shared = |other: &PalwChainStateV2| {
        move |id: &Hash64, claim: &PalwClaimStateV2| other.claim(id).is_none_or(|o| o.accepted_block != claim.accepted_block)
    };
    let side_a = a.palw_rule_e_side_v1(params, uncertified_weightless, canonical_work_daa, shared(b), |k| common.contains(k))?;
    let side_b = b.palw_rule_e_side_v1(params, uncertified_weightless, canonical_work_daa, shared(a), |k| common.contains(k))?;
    Ok((side_a, side_b, palw_rule_e_even_split_min_v1(common.len())))
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
        let mut p = TESTNET_PARAMS;
        let ids = (p.consensus_params_id(), p.consensus_schedule_id());
        p.palw_fork_choice_rule_e_v1 = Some(ForkActivation::never());
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
        assert_eq!(palw_rule_e_decide_v1(&even, &even, false, 3, || panic!("participation does not count"), || false), PalwDeepReorgV2::Refuse);
        assert_eq!(palw_rule_e_even_split_min_v1(8), 3);
        assert_eq!(palw_rule_e_even_split_min_v1(0), 1);
    }
}
