//! **Lane P4 values (V12, R1–R7) — DECIDED, AWAITING INDEPENDENT VERIFICATION** (user delegation of value decisions, 2026-10-22;
//! evidence and economic audit: `docs/evidence/palw/studio-values-2026-10-22/values-p4-policy-rfc.md`).
//!
//! Only the values whose definition (RFC / ADR formula or a recorded user decision) needs no measurement that is still missing are
//! here. Everything else in lane P4 — the bond budget's per-day rates, the RFC-0006 draw/price values, the RFC-0007 resource limits,
//! source deposit/caps and error allocation, the RFC-0010 production beacon, every measured record (RFC-0016, RFC-0006/0008, TIR
//! frame) and the PALWQ361 reference windows — stays `None` / empty in its own module, for the reasons the evidence file states.
//!
//! **Nothing here arms a fence, pins an arming or measured record, or changes a shipped preset.** No preset reads these values; each
//! fence's validator still refuses its activation exactly as before (the tests below check that the refusal stands with the decided
//! value in place). The lead pins them after the independent verification.
//!
//! No value here is, or relies on, a cryptographic proof of computation (PRINCIPLES §8); the hash below is a nothing-up-my-sleeve
//! derivation of a public seed, not evidence of any computation. No per-model verification is introduced (PRINCIPLES §9).

use crate::palw_bond_budget_v1::{
    PALW_ALLOCATION_ALPHA_HALVES_DECIDED_V1, PALW_BOND_HOLD_DAA_PER_DAY_V1, PALW_MODEL_ALLOCATION_POLICY_VERSION_V3,
    PalwAllocationAverageV1, PalwAllocationCurveV1, PalwModelAllocationPolicyV1,
};
use misaka_palw_kernel::hash::{Digest, object_id};

/// The status every value in this module carries until the independent verification and the lead's pin.
pub const PALW_P4_VALUES_STATUS_V1: &str = "decided, awaiting independent verification";

/// **V12 / R1: the allocation epoch, DAA** — one day-equivalent (720 DAA): the coarsest epoch that divides the user's 7-day average
/// (5,040 DAA) into whole days, equal to the common 1-day hold of both Panel modes.
pub const PALW_MODEL_ALLOCATION_EPOCH_DAA_DECIDED_V1: u64 = PALW_BOND_HOLD_DAA_PER_DAY_V1;
/// **V12: seasoning, in full epochs** — 1, the minimum `validate` admits. The average is linear in capital × time, so a longer
/// seasoning buys no protection against borrowed capital (it never amplifies) and only delays an honest bond.
pub const PALW_MODEL_ALLOCATION_SEASONING_EPOCHS_DECIDED_V1: u32 = 1;
/// **V12: entries one capital assignment may name** — 16: all 8 genesis models plus 8 registered ones. Splitting capital over more
/// models never raises its weight under `α = 1.5` (`x^α + y^α ≤ (x+y)^α`), so the bound is a resource bound only.
pub const PALW_MODEL_ALLOCATION_MAX_MODELS_PER_BOND_DECIDED_V1: u16 = 16;

/// **V12 / R1: `palw_model_bond_allocation_v1`'s policy — DECIDED, AWAITING INDEPENDENT VERIFICATION.** `α = 1.5`, the 7-day average
/// of miner + claim-bound verifier capital and `P_m` on block opportunity and coinbase are the user's decisions (2026-10-14/16); the
/// epoch, seasoning and per-bond entry bound are this lane's (see the constants). The fence stays refused by its validator (ADR-0177
/// D6 and the user's pre-activation conditions are not values).
pub fn palw_model_allocation_policy_decided_v1() -> PalwModelAllocationPolicyV1 {
    PalwModelAllocationPolicyV1 {
        version: PALW_MODEL_ALLOCATION_POLICY_VERSION_V3,
        epoch_daa: PALW_MODEL_ALLOCATION_EPOCH_DAA_DECIDED_V1,
        seasoning_epochs: PALW_MODEL_ALLOCATION_SEASONING_EPOCHS_DECIDED_V1,
        curve: PalwAllocationCurveV1 { alpha_halves: PALW_ALLOCATION_ALPHA_HALVES_DECIDED_V1 },
        max_models_per_bond: PALW_MODEL_ALLOCATION_MAX_MODELS_PER_BOND_DECIDED_V1,
        average: Some(PalwAllocationAverageV1::decided_v1()),
    }
}

/// Domain of the pinned bootstrap seed's derivation.
pub const PALW_PAID_CLAIM_BOOTSTRAP_SEED_DOMAIN_V1: &[u8] = b"MISAKA/PALW/PAID-CLAIM-BOOTSTRAP-SEED/NETWORK/V1";

/// **R2: the network's pinned paid-claim bootstrap seed — DECIDED, AWAITING INDEPENDENT VERIFICATION** (user decision 2026-10-17:
/// until the first approved paid Final, checkers are drawn from a seed fixed at genesis or an annual fork). The value is derived, not
/// chosen: `H(domain, network domain)` with the network domain of [`crate::config::params::Params::palw_rfc0608_network_domain_v1`]
/// (network name + genesis hash). Nobody can pick a seed that favours chosen checker bonds, and the seed differs per network. It is
/// public and predictable by design: a bootstrap lock earns no EXEC credit, and its safety rests on the claim-level court and bond
/// slash alone (see the evidence file's audit).
pub fn palw_paid_claim_bootstrap_seed_decided_v1(network_domain: &Digest) -> Digest {
    object_id(PALW_PAID_CLAIM_BOOTSTRAP_SEED_DOMAIN_V1, network_domain)
}

impl crate::config::params::Params {
    /// [`palw_paid_claim_bootstrap_seed_decided_v1`] of this network (not read by any preset).
    pub fn palw_paid_claim_bootstrap_seed_decided_v1(&self) -> Digest {
        palw_paid_claim_bootstrap_seed_decided_v1(&self.palw_rfc0608_network_domain_v1())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hash64;
    use crate::config::params::{ForkActivation, palw_t12_shipped_params};
    use crate::palw_bond_budget_v1::*;
    use crate::palw_mode_v2::PalwModeV2Error;
    use crate::palw_state_v2::PalwBondKeyV2;
    use crate::tx::TransactionOutpoint;
    use std::collections::BTreeMap;

    const KAS: u64 = crate::constants::SOMPI_PER_KASPA;

    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([n; 64]), 0))
    }
    fn model(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn state() -> PalwBondBudgetStateV1 {
        PalwBondBudgetStateV1::new(0, &PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1())
    }

    #[test]
    fn p4_decided_allocation_policy_validates_and_carries_the_user_terms() {
        let p = palw_model_allocation_policy_decided_v1();
        p.validate().expect("the decided policy validates");
        assert_eq!(p.average_epochs(), Some(7), "7 day-equivalents over 1-day epochs");
        assert_eq!(p.curve.alpha_halves, 3, "alpha = 1.5");
        assert_eq!(p.epoch_daa, 720);
        assert_eq!(PALW_P4_VALUES_STATUS_V1, "decided, awaiting independent verification");
        // Refusals: every user term is pinned by validate, and the lane's terms keep their invariants.
        let refused = |f: &dyn Fn(&mut PalwModelAllocationPolicyV1)| {
            let mut q = palw_model_allocation_policy_decided_v1();
            f(&mut q);
            q.validate().is_err()
        };
        assert!(refused(&|q| q.curve.alpha_halves = 2), "alpha = 1 refused");
        assert!(refused(&|q| q.curve.alpha_halves = 4), "alpha = 2 refused");
        assert!(refused(&|q| q.average.as_mut().unwrap().average_days = 30), "the old 30-day average refused");
        assert!(refused(&|q| q.average.as_mut().unwrap().verifier_capital = false));
        assert!(refused(&|q| q.average.as_mut().unwrap().block_opportunity = false));
        assert!(refused(&|q| q.epoch_daa = 1_000), "an epoch that does not divide 5,040 DAA");
        assert!(refused(&|q| q.epoch_daa = 0));
        assert!(refused(&|q| q.seasoning_epochs = 0));
        assert!(refused(&|q| q.max_models_per_bond = 0));
        assert!(refused(&|q| q.average = None));
    }

    #[test]
    fn p4_the_allocation_fence_stays_refused_with_the_decided_policy() {
        let decided = palw_model_allocation_policy_decided_v1();
        // Without the budget at or below it: ADR-0177 D5.
        let mut alone = palw_t12_shipped_params();
        alone.palw_model_bond_allocation_v1 =
            Some(PalwModelBondAllocationFenceV1 { activation: ForkActivation::new(16_000), policy: decided.clone() });
        let PalwModeV2Error::Invalid(why) = alone.validate_palw_model_bond_allocation_v1().unwrap_err() else { panic!("Invalid") };
        assert!(why.contains("requires palw_bond_budget_v1"), "{why}");
        // With a (V3-form) budget below it: still refused — the non-value blockers stand, nothing is armed here.
        let mut both = alone.clone();
        both.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1 {
            activation: ForkActivation::new(16_000),
            policy: PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1(),
        });
        let PalwModeV2Error::Invalid(why) = both.validate_palw_model_bond_allocation_v1().unwrap_err() else { panic!("Invalid") };
        assert!(why.contains("cannot be armed"), "{why}");
        // A malformed policy is named first.
        let mut bad = both.clone();
        bad.palw_model_bond_allocation_v1.as_mut().unwrap().policy.epoch_daa = 1_000;
        let PalwModeV2Error::Invalid(why) = bad.validate_palw_model_bond_allocation_v1().unwrap_err() else { panic!("Invalid") };
        assert!(why.contains("policy is invalid"), "{why}");
        // The shipped t12 preset is untouched.
        assert!(palw_t12_shipped_params().palw_model_bond_allocation_v1.is_none());
    }

    #[test]
    fn p4_assignment_bound_admits_sixteen_models_and_refuses_seventeen() {
        let p = palw_model_allocation_policy_decided_v1();
        let mut j = Vec::new();
        let sixteen: Vec<(Hash64, u64)> = (1..=16u8).map(|m| (model(m), KAS)).collect();
        state().assign(&p, bond(1), &sixteen, 1, 16 * KAS, 0, |_| true, &mut j).expect("16 entries fit");
        let seventeen: Vec<(Hash64, u64)> = (1..=17u8).map(|m| (model(m), KAS)).collect();
        assert!(state().assign(&p, bond(1), &seventeen, 1, 17 * KAS, 0, |_| true, &mut j).is_err(), "17 entries refused");
    }

    /// Economic audit (V12): one epoch of borrowed capital is worth exactly its capital × one epoch over the whole average — the
    /// 7-day average never amplifies a short lock — and a newcomer reaches the incumbent's average after seasoning + 7 epochs.
    #[test]
    fn p4_seven_epoch_average_never_amplifies_borrowed_capital_and_a_newcomer_ramps_in_seven_epochs() {
        let p = palw_model_allocation_policy_decided_v1();
        let empty = BTreeMap::new();
        // Borrowed: bond 1 assigns 700 to A at epoch 0 (effective from epoch 2) and is Active for epoch 2 only.
        let mut s = state();
        let mut j = Vec::new();
        s.assign(&p, bond(1), &[(model(0xA), 700 * KAS)], 1, 700 * KAS, 0, |_| true, &mut j).unwrap();
        let mut integral: u128 = 0;
        for epoch in 0..20u64 {
            let active = epoch == 2;
            s.roll_epoch_v3(&p, epoch, epoch * p.epoch_daa, |_| (700 * KAS, active), &empty, |_, c| c, &mut j);
            let avg = s.model_row(&model(0xA)).map(|r| r.capital).unwrap_or(0);
            if (2..9).contains(&epoch) {
                assert_eq!(avg, (100 * KAS) as u128, "epoch {epoch}: one seventh of the borrowed capital");
            } else {
                assert_eq!(avg, 0, "epoch {epoch}");
            }
            integral += avg;
        }
        assert_eq!(integral, (700 * KAS) as u128, "Σ average = capital × epochs locked: no amplification");
        s.check_consistency().expect("consistent");

        // Newcomer: A holds 700 from epoch 0; B joins at epoch 10 with 700 (effective from 12): equal from epoch 12 + 6 = 18.
        let mut s = state();
        let mut j = Vec::new();
        s.assign(&p, bond(1), &[(model(0xA), 700 * KAS)], 1, 1_400 * KAS, 0, |_| true, &mut j).unwrap();
        for epoch in 0..30u64 {
            if epoch == 10 {
                s.assign(&p, bond(1), &[(model(0xA), 700 * KAS), (model(0xB), 700 * KAS)], 2, 1_400 * KAS, epoch, |_| true, &mut j)
                    .unwrap();
            }
            s.roll_epoch_v3(&p, epoch, epoch * p.epoch_daa, |_| (1_400 * KAS, true), &empty, |_, c| c, &mut j);
            let a = s.model_row(&model(0xA)).map(|r| r.capital).unwrap_or(0);
            let b = s.model_row(&model(0xB)).map(|r| r.capital).unwrap_or(0);
            assert!(b <= a, "epoch {epoch}: the newcomer never exceeds the incumbent");
            if epoch >= 18 {
                assert_eq!((a, b), ((700 * KAS) as u128, (700 * KAS) as u128), "epoch {epoch}: full span reached");
            }
        }
        let history = s.models.values().filter(|r| r.reserved_sompi == PALW_ALLOCATION_HISTORY_MARKER_V1).count();
        assert!(history <= 2 * 7, "history rows bounded by the 7-epoch span: {history}");
        s.check_consistency().expect("consistent");
    }

    #[test]
    fn p4_bootstrap_seed_is_derived_per_network_nonzero_and_accepted_by_the_policy() {
        let t12 = palw_t12_shipped_params();
        let seed = t12.palw_paid_claim_bootstrap_seed_decided_v1();
        assert_ne!(seed, [0; 64]);
        eprintln!("testnet-12 paid-claim bootstrap seed (decided): {}", misaka_palw_kernel::hash::hex(&seed));
        assert_eq!(seed, t12.palw_paid_claim_bootstrap_seed_decided_v1(), "deterministic");
        assert_ne!(seed, palw_paid_claim_bootstrap_seed_decided_v1(&[0; 64]), "bound to the network domain");
        let mainnet = crate::config::params::MAINNET_PARAMS.palw_paid_claim_bootstrap_seed_decided_v1();
        assert_ne!(seed, mainnet, "another network, another seed");
        // The paid-claim policy accepts it (and still refuses the zero seed); the fence stays refused by its own gates.
        let probe = crate::palw_claim_challenge_v2::PalwClaimChallengeFenceV2::unapproved_probe_at(ForkActivation::new(16_000));
        let with = crate::palw_claim_challenge_v2::PalwClaimChallengePolicyV2 { bootstrap_seed: Some(seed), ..probe.policy };
        with.validate().expect("a non-zero pinned seed validates");
        let mut armed = t12.clone();
        armed.palw_claim_challenge_v2 = Some(crate::palw_claim_challenge_v2::PalwClaimChallengeFenceV2 { policy: with, ..probe });
        assert!(armed.validate_palw_claim_challenge_v2().is_err(), "no review evidence / record: the fence stays refused");
        assert!(t12.palw_claim_challenge_v2.is_none(), "the shipped preset is untouched");
    }
}
