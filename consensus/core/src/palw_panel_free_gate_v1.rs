//! **RFC-0015 §13.3 as code: the activation gate record of `palw_panel_free_v1`** (dormant; nothing here arms anything).
//!
//! RFC-0015 §13.3 states `ACTIVATION_ALLOWED` as a release AND contract, "not a field an operator's word makes true". This module is
//! that contract's only door: `Params::validate_palw_panel_free_v1` refuses an armed `palw_panel_free_v1` on every network that is not
//! an isolated calibration genesis unless the fence's value carries a [`PalwPanelFreeGateRecordV1`] that
//!
//! 1. lists **every** conjunct of §13.3 (plus the 2026-10-10 PESG addition and the 2026-10-12 N5 deferral lift) exactly once, each
//!    with verdict [`PalwGateVerdictV1::Pass`] (UNKNOWN is never PASS), a non-zero evidence digest and a non-zero independent review
//!    digest, no evidence or review digest shared between two conjuncts;
//! 2. binds the network (its genesis hash), the coordinated activation height (the fence's own), the fence's exact terms (deny-list,
//!    floor, GAP-70 switch, OPV window / budgets / economics / carrier) and the exact bond-budget and model-allocation policies armed
//!    beside it, so a record reviewed for one network, height or set of numbers authorizes no other;
//! 3. finds, in the same `Params`, every fence a conjunct is mechanically checkable against armed **at or below** the activation —
//!    the kernel route (`palw_probabilistic_constraints_v1`), ADR-0176's class-independent `Q/B/R/F` caps and common DAA hold
//!    (`palw_bond_budget_v1`, its policy valid), ADR-0177's allocation (`palw_model_bond_allocation_v1`) and court model scope
//!    (`palw_court_model_scope_v1`), and RFC-0014 / ADR-0178's rule E (`palw_fork_choice_rule_e_v1`);
//! 4. and — the authentication — whose digest is one of [`PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1`], a constant of this binary's
//!    source. **That list is empty.** A record can therefore only become acceptable through a reviewed code release that pins its
//!    digest; no configuration file, command-line flag or operator signature can make it so, and this binary accepts none.
//!
//! The record is a value of `PalwPanelFreeFenceV1` (so it is identity: hashed Some-only after the fence's value, never on any shipped
//! network, where the fence is `None`). It changes no wire format, no object kind, no state, and no testnet-12 params or schedule id.
//! The record is an attestation that evidence exists; it is not the evidence, and a pinned digest is not a proof of safety — it only
//! makes the arming an explicit, reviewable code change instead of a switch (RFC-0015 §1.2, §13.3; PESG: PASS / FAIL / UNKNOWN).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_panel_free_v1::PalwPanelFreeFenceV1;

/// The record layout this binary reads.
pub const PALW_PANEL_FREE_GATE_RECORD_VERSION_V1: u16 = 1;

/// **The record digests this binary accepts** (BLAKE2b-512 keyed by the record domain, [`PalwPanelFreeGateRecordV1::digest`]).
/// **Empty**: RFC-0015 is deferred (2026-10-12 amendment, user ruling N5) and no conjunct of §13.3 is evidenced. Adding a digest here
/// is the activation decision itself and needs every conjunct's evidence, its independent review and the user's ruling — see
/// `docs/design/palw/rfc-0006-0012-user-decisions-2026-10-15.md` §RFC15.
pub const PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1: &[[u8; 64]] = &[];

const PALW_PANEL_FREE_GATE_RECORD_DOMAIN_V1: &[u8] = b"misaka-palw/panel-free/activation-gate-record/v1";
const PALW_PANEL_FREE_TERMS_DOMAIN_V1: &[u8] = b"misaka-palw/panel-free/terms/v1";
const PALW_PANEL_FREE_ALLOCATION_POLICY_DOMAIN_V1: &[u8] = b"misaka-palw/panel-free/allocation-policy/v1";

/// One conjunct of RFC-0015 §13.3 `ACTIVATION_ALLOWED` (and the two later additions). The discriminant is the record's canonical order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwPanelFreeConjunctV1 {
    /// `G14_PUBLIC_FRAUD_PROSECUTION_COMPLETE` (§1.1, all ten conditions, on a real node, for every rewarded profile).
    G14PublicFraudProsecutionComplete = 0,
    /// `all rewarded profile dispute/resource gates` (§1.1 last paragraph, §5, §13.2 "largest admitted class").
    RewardedProfileDisputeResourceGates = 1,
    /// `new lifecycle/court/Final implementation complete` (§4, §6, §12).
    LifecycleCourtFinalImplementation = 2,
    /// `panel-free collateral/accounting review complete` (§8.1–§8.2).
    PanelFreeCollateralAccountingReview = 3,
    /// `class-independent bond claim / block / reward / Final-weight caps + common DAA hold complete` (§8.3–§8.4).
    ClassIndependentCapsAndCommonDaaHold = 4,
    /// `public material/watcher/inclusion operating assumptions supported` (§2, §6.3, §7).
    PublicMaterialWatcherInclusionAssumptions = 5,
    /// `ADR177 non-interference / capital / allocation / open-vs-closed ECON-MEAS gates complete` (§8.5).
    Adr177NonInterferenceCapitalAllocationEcon = 6,
    /// `RFC14 claim-specific prosecution / Rule E gates complete` (§9).
    Rfc14ClaimSpecificProsecutionAndRuleE = 7,
    /// `independent adversarial node E2E + recovery/migration PASS` (§13.2).
    IndependentAdversarialNodeE2eRecoveryMigration = 8,
    /// `separately coordinated network schedule` (§11.1).
    SeparatelyCoordinatedNetworkSchedule = 9,
    /// 2026-10-10 user instruction: the PALW probabilistic / economic security gate (four conditions, T1–T6, A–F, three
    /// counterexamples, damage bounding) is PASS (`docs/design/palw/probabilistic-economic-security-gate.md`).
    ProbabilisticEconomicSecurityGate = 10,
    /// 2026-10-12 amendment (N5): Panel=0 is deferred; a new user ruling must lift the deferral after the separate safety
    /// demonstration (watcher exit, Panel collusion, verification cost, maximum claim load, cold model fetch, fraud-to-court time).
    PanelZeroDeferralLiftedByRuling = 11,
}

impl PalwPanelFreeConjunctV1 {
    /// Every conjunct, in canonical order.
    pub const ALL: [Self; 12] = [
        Self::G14PublicFraudProsecutionComplete,
        Self::RewardedProfileDisputeResourceGates,
        Self::LifecycleCourtFinalImplementation,
        Self::PanelFreeCollateralAccountingReview,
        Self::ClassIndependentCapsAndCommonDaaHold,
        Self::PublicMaterialWatcherInclusionAssumptions,
        Self::Adr177NonInterferenceCapitalAllocationEcon,
        Self::Rfc14ClaimSpecificProsecutionAndRuleE,
        Self::IndependentAdversarialNodeE2eRecoveryMigration,
        Self::SeparatelyCoordinatedNetworkSchedule,
        Self::ProbabilisticEconomicSecurityGate,
        Self::PanelZeroDeferralLiftedByRuling,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::G14PublicFraudProsecutionComplete => "G14_PUBLIC_FRAUD_PROSECUTION_COMPLETE",
            Self::RewardedProfileDisputeResourceGates => "all rewarded profile dispute/resource gates",
            Self::LifecycleCourtFinalImplementation => "new lifecycle/court/Final implementation complete",
            Self::PanelFreeCollateralAccountingReview => "panel-free collateral/accounting review complete",
            Self::ClassIndependentCapsAndCommonDaaHold => "class-independent Q/B/R/F caps + common DAA hold complete",
            Self::PublicMaterialWatcherInclusionAssumptions => "public material/watcher/inclusion operating assumptions supported",
            Self::Adr177NonInterferenceCapitalAllocationEcon => "ADR177 non-interference/capital/allocation/open-vs-closed gates complete",
            Self::Rfc14ClaimSpecificProsecutionAndRuleE => "RFC14 claim-specific prosecution / Rule E gates complete",
            Self::IndependentAdversarialNodeE2eRecoveryMigration => "independent adversarial node E2E + recovery/migration PASS",
            Self::SeparatelyCoordinatedNetworkSchedule => "separately coordinated network schedule",
            Self::ProbabilisticEconomicSecurityGate => "PALW probabilistic/economic security gate PASS",
            Self::PanelZeroDeferralLiftedByRuling => "2026-10-12 N5 Panel=0 deferral lifted by user ruling",
        }
    }
}

/// A conjunct's verdict (PESG's three values). Only `Pass` satisfies; `Unknown` is never read as `Pass`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwGateVerdictV1 {
    Pass = 0,
    Fail = 1,
    Unknown = 2,
}

/// One conjunct's line of the record.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPanelFreeGateEntryV1 {
    pub conjunct: PalwPanelFreeConjunctV1,
    pub verdict: PalwGateVerdictV1,
    /// Digest of the evidence bundle (measurements, node E2E logs, analyses) the verdict rests on. Non-zero, unique in the record.
    pub evidence: Hash64,
    /// Digest of the independent review of that evidence. Non-zero, unique in the record, never equal to an evidence digest.
    pub review: Hash64,
}

/// **The activation gate record** (module doc). Borsh is its canonical form; [`Self::digest`] is what the binary pins.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPanelFreeGateRecordV1 {
    pub version: u16,
    /// The genesis hash of the one network this record is for.
    pub genesis: Hash64,
    /// The coordinated activation height (must be the fence's own).
    pub activation_daa: u64,
    /// [`palw_panel_free_terms_digest_v1`] of the fence's value.
    pub terms: Hash64,
    /// `PalwBondBudgetPolicyV1::digest` of the `palw_bond_budget_v1` policy armed beside it.
    pub bond_budget_policy: Hash64,
    /// [`palw_panel_free_allocation_policy_digest_v1`] of the `palw_model_bond_allocation_v1` policy armed beside it.
    pub allocation_policy: Hash64,
    /// One entry per conjunct, strictly ascending by conjunct.
    pub entries: Vec<PalwPanelFreeGateEntryV1>,
}

/// Why a record does not open the gate (each a fixed sentence, for `PalwModeV2Error::Invalid`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwPanelFreeGateRefusalV1 {
    Version,
    /// A conjunct missing, repeated or out of order.
    Coverage,
    /// A conjunct whose verdict is FAIL or UNKNOWN.
    NotPass(PalwPanelFreeConjunctV1),
    /// A zero evidence or review digest, or a digest used twice.
    Evidence(PalwPanelFreeConjunctV1),
    WrongNetwork,
    /// The fence is unarmed (`never()`) or its height is not the record's.
    WrongActivation,
    WrongTerms,
    /// A fence a conjunct is mechanically checked against is absent, `never()`, or armed above the activation.
    FenceNotArmedAtOrBelow(&'static str),
    /// The bond budget's policy is invalid or is not the reviewed one.
    BondBudgetPolicy,
    /// The allocation policy is invalid or is not the reviewed one.
    AllocationPolicy,
    /// The record is well formed but its digest is not pinned in this binary.
    NotPinned,
}

impl PalwPanelFreeGateRefusalV1 {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Version => "palw_panel_free_v1's activation gate record has an unknown version",
            Self::Coverage => {
                "palw_panel_free_v1's activation gate record must list every RFC-0015 §13.3 conjunct exactly once, in order (12)"
            }
            Self::NotPass(_) => "palw_panel_free_v1's activation gate record has a conjunct that is not PASS (UNKNOWN is never PASS)",
            Self::Evidence(_) => {
                "palw_panel_free_v1's activation gate record has a conjunct without a unique non-zero evidence and independent review digest"
            }
            Self::WrongNetwork => "palw_panel_free_v1's activation gate record was reviewed for another network (genesis)",
            Self::WrongActivation => "palw_panel_free_v1's activation gate record names another activation height than the fence",
            Self::WrongTerms => "palw_panel_free_v1's activation gate record was reviewed for other OPV terms than the fence carries",
            Self::FenceNotArmedAtOrBelow(_) => {
                "palw_panel_free_v1 needs palw_probabilistic_constraints_v1, palw_bond_budget_v1, palw_model_bond_allocation_v1, \
                 palw_court_model_scope_v1 and palw_fork_choice_rule_e_v1 armed at or below it (RFC-0015 §8.3–§8.5, §9, §13.3)"
            }
            Self::BondBudgetPolicy => {
                "palw_panel_free_v1's activation gate record was reviewed for another (or an invalid) palw_bond_budget_v1 policy"
            }
            Self::AllocationPolicy => {
                "palw_panel_free_v1's activation gate record was reviewed for another (or an invalid) palw_model_bond_allocation_v1 policy"
            }
            Self::NotPinned => {
                "palw_panel_free_v1's activation gate record is not pinned in this binary (PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1 is \
                 empty: RFC-0015 is deferred, 2026-10-12 N5)"
            }
        }
    }
}

fn keyed64(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for part in parts {
        s.update(&(part.len() as u64).to_le_bytes());
        s.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn is_zero(h: &Hash64) -> bool {
    h.as_byte_slice().iter().all(|b| *b == 0)
}

/// **The digest of the fence's value without its activation and without the record** — what a reviewed record binds: the deny-list,
/// the effective-bits floor, GAP-70's switch and the OPV terms (Borsh).
pub fn palw_panel_free_terms_digest_v1(fence: &PalwPanelFreeFenceV1) -> Hash64 {
    let mut denied = Vec::with_capacity(fence.denied_classes.len() * 64);
    for class in &fence.denied_classes {
        denied.extend_from_slice(class.as_byte_slice());
    }
    let terms = borsh::to_vec(&(fence.window, fence.budgets, fence.economics, fence.carrier)).expect("the terms serialize");
    keyed64(
        PALW_PANEL_FREE_TERMS_DOMAIN_V1,
        &[&denied, &fence.min_effective_bits.to_le_bytes(), &[fence.sampled_conformance_gates_reward as u8], &terms],
    )
}

/// The digest a record binds of the `palw_model_bond_allocation_v1` policy.
pub fn palw_panel_free_allocation_policy_digest_v1(policy: &crate::palw_bond_budget_v1::PalwModelAllocationPolicyV1) -> Hash64 {
    keyed64(PALW_PANEL_FREE_ALLOCATION_POLICY_DOMAIN_V1, &[&borsh::to_vec(policy).expect("a policy serializes")])
}

/// `Some(at)` armed at or below `activation`: present, not `never()`, and at or below it.
fn at_or_below(at: Option<ForkActivation>, activation: u64) -> bool {
    at.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= activation)
}

impl PalwPanelFreeGateRecordV1 {
    /// The record's identity: BLAKE2b-512 keyed by the record domain over its Borsh bytes.
    pub fn digest(&self) -> Hash64 {
        keyed64(PALW_PANEL_FREE_GATE_RECORD_DOMAIN_V1, &[&borsh::to_vec(self).expect("a record serializes")])
    }

    /// The record's own form: version, every conjunct once in order, every verdict PASS, every digest non-zero and unique.
    pub fn validate_form_v1(&self) -> Result<(), PalwPanelFreeGateRefusalV1> {
        if self.version != PALW_PANEL_FREE_GATE_RECORD_VERSION_V1 {
            return Err(PalwPanelFreeGateRefusalV1::Version);
        }
        if self.entries.len() != PalwPanelFreeConjunctV1::ALL.len()
            || self.entries.iter().zip(PalwPanelFreeConjunctV1::ALL).any(|(e, c)| e.conjunct != c)
        {
            return Err(PalwPanelFreeGateRefusalV1::Coverage);
        }
        let mut seen: Vec<&Hash64> = Vec::with_capacity(2 * self.entries.len());
        for e in &self.entries {
            if e.verdict != PalwGateVerdictV1::Pass {
                return Err(PalwPanelFreeGateRefusalV1::NotPass(e.conjunct));
            }
            if is_zero(&e.evidence) || is_zero(&e.review) || e.evidence == e.review || seen.contains(&&e.evidence) || seen.contains(&&e.review)
            {
                return Err(PalwPanelFreeGateRefusalV1::Evidence(e.conjunct));
            }
            seen.push(&e.evidence);
            seen.push(&e.review);
        }
        Ok(())
    }

    /// **The gate** against `params` and its armed `fence`, accepting only digests in `pins` (the node passes
    /// [`PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1`]; tests pass their own to exercise every other check). Checks in the order of the
    /// module doc; the pin last, so every structural and binding refusal is named before it.
    pub fn verify_v1(&self, params: &Params, fence: &PalwPanelFreeFenceV1, pins: &[[u8; 64]]) -> Result<(), PalwPanelFreeGateRefusalV1> {
        self.validate_form_v1()?;
        if self.genesis != params.genesis.hash {
            return Err(PalwPanelFreeGateRefusalV1::WrongNetwork);
        }
        if fence.activation == ForkActivation::never() || fence.activation.daa_score() != self.activation_daa {
            return Err(PalwPanelFreeGateRefusalV1::WrongActivation);
        }
        if self.terms != palw_panel_free_terms_digest_v1(fence) {
            return Err(PalwPanelFreeGateRefusalV1::WrongTerms);
        }
        let at = self.activation_daa;
        let budget = params.palw_bond_budget_v1.as_ref();
        let allocation = params.palw_model_bond_allocation_v1.as_ref();
        for (name, armed) in [
            ("palw_probabilistic_constraints_v1", at_or_below(params.palw_probabilistic_constraints_v1, at)),
            ("palw_bond_budget_v1", at_or_below(budget.map(|f| f.activation), at)),
            ("palw_model_bond_allocation_v1", at_or_below(allocation.map(|f| f.activation), at)),
            ("palw_court_model_scope_v1", at_or_below(params.palw_court_model_scope_v1, at)),
            ("palw_fork_choice_rule_e_v1", at_or_below(params.palw_fork_choice_rule_e_v1, at)),
        ] {
            if !armed {
                return Err(PalwPanelFreeGateRefusalV1::FenceNotArmedAtOrBelow(name));
            }
        }
        let budget = budget.expect("checked above");
        if budget.policy.validate().is_err() || budget.policy.digest() != self.bond_budget_policy {
            return Err(PalwPanelFreeGateRefusalV1::BondBudgetPolicy);
        }
        let allocation = allocation.expect("checked above");
        if allocation.policy.validate().is_err() || palw_panel_free_allocation_policy_digest_v1(&allocation.policy) != self.allocation_policy {
            return Err(PalwPanelFreeGateRefusalV1::AllocationPolicy);
        }
        let digest = self.digest();
        if !pins.iter().any(|pin| pin == digest.as_byte_slice()) {
            return Err(PalwPanelFreeGateRefusalV1::NotPinned);
        }
        Ok(())
    }
}

impl Params {
    /// Whether the Panel-free fence carries an activation gate record (the networks on which an OPV claim must be budgeted by
    /// `palw_bond_budget_v1` at its door: `palw_kernel_route_fold_v1` refuses an unbudgeted OPV claim there).
    pub fn palw_panel_free_gated_v1(&self) -> bool {
        self.palw_panel_free_v1.as_ref().is_some_and(|f| f.activation_gate.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_binary_pins_no_record() {
        assert!(PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1.is_empty(), "RFC-0015 is deferred (2026-10-12 N5): nothing is pinned");
    }

    #[test]
    fn the_conjunct_list_is_canonical_and_complete() {
        for (i, c) in PalwPanelFreeConjunctV1::ALL.iter().enumerate() {
            assert_eq!(*c as u8 as usize, i);
            assert!(!c.name().is_empty());
        }
    }
}
