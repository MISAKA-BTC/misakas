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
//! value in place). The 2026-10-22 pin is the parameter register
//! (`docs/governance/parameter-register.md`), not an activation height.
//!
//! No value here is, or relies on, a cryptographic proof of computation (PRINCIPLES §8); the hash below is a nothing-up-my-sleeve
//! derivation of a public seed, not evidence of any computation. No per-model verification is introduced (PRINCIPLES §9).

use crate::palw_bond_budget_v1::{
    PALW_ALLOCATION_ALPHA_HALVES_DECIDED_V1, PALW_BOND_HOLD_DAA_PER_DAY_V1, PALW_MODEL_ALLOCATION_POLICY_VERSION_V3,
    PalwAllocationAverageV1, PalwAllocationCurveV1, PalwModelAllocationPolicyV1,
};
use misaka_palw_kernel::hash::{Digest, object_id};

/// The status of this module after the 2026-10-22 parameter-register pin. Fences stay unarmed.
pub const PALW_P4_VALUES_STATUS_V1: &str = "pinned 2026-10-22 in the parameter register; fences unarmed";

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

// ---- 目安 (provisional, user 2026-10-22: 「パラメーターは目安で決めていい　方針をあとから変更にする」) ------------------------------
// Each value below is a conservative PROVISIONAL estimate from the best data available on 2026-10-22 (design values, code constants,
// the under-load Mac Studio M-C measurements on studio/values-p1-measure, the t12 passive pace 184 s/DAA). None is a measured record,
// none arms anything, and each names the trigger that revises it. Evidence: values-p4-policy-rfc.md §10.

/// 目安: the capital unit the bond budget's rates are quoted against — the testnet-12 minimum producer bond (13,000 KAS; ADR-0176
/// plan §3). Revise: when the target ruleset's minimum bond changes.
pub const PALW_BOND_BUDGET_CAPITAL_UNIT_PROVISIONAL_SOMPI_V1: u64 = 13_000 * crate::constants::SOMPI_PER_KASPA;
/// 目安: claims (and reward-bearing blocks, and Round rights) per capital unit per DAY — one per 10 nominal DAA slots (72 of 720).
/// At the measured t12 pace (184 s/DAA, ≈ 470 DAA/day) that is ≈ 15 % of a day's blocks per minimum bond, so one bond cannot take
/// most of the block opportunity. Revise: ADR-0176 Stage 1 (the reference miner's sustained honest rate) — TooLow if REAL stalls on
/// the cap, TooHigh if the honest miner uses < 80 % of it.
pub const PALW_BOND_BUDGET_DAILY_CLAIMS_PROVISIONAL_V1: u64 = 72;
/// 目安: the export cap, permille of the collateral still held — half of PESG §6's 510 bound, leaving `0.51 − 0.25 = 0.26·K` for the
/// saved-compute term in `slash loss 0.51·K > export + S` (PRINCIPLES §10.4-1). Revise: Stage 1's measured `S/K`.
pub const PALW_BOND_BUDGET_EXPORT_CAP_PROVISIONAL_PERMILLE_V1: u16 = 250;
/// 目安: `H_L`, the liability hold after a claim is terminal — 10,000 DAA, the longest court-clock v2 extension horizon
/// (`a + 10,000`, F07 / V9). Revise: X3's measured `Δ` (`hard = accepted + 5,520 + Δ`) — `H_L` must cover the hard deadline.
pub const PALW_BOND_BUDGET_LIABILITY_HOLD_PROVISIONAL_DAA_V1: u64 = 10_000;
/// 目安 (lane GC-DUTY, Lead 2026-10-28; ADR-0176 2026-10-15「枠不足で REAL が止まらない」): `F` per capital unit on the claims' OWN
/// weight scale. Was 1,000 — a mechanics-scale placeholder 10^8 below the live scale, under which every own attempt was skipped
/// (`UnsettledWeight`) and no seed anchor, Panel=1 seat or Final followed. Live measurement (testnet-12, DAA 16,693, read-only
/// `getPalwCapacityShadow`): heavy class Final weight 1.389e11 per claim (median = p99 = max; light class 4.33e6), peak 45 open
/// claims on one bond. Value = 4 (margin) × 72 (a unit's daily claims) × 1.39e11 ≈ 4.0e13. A forged claim still cannot raise F (the
/// cap is capital-linear and the claim's weight is the chain's). Revise: the heaviest Final weight per claim × 72 × 4 moves (a new
/// genesis/annual class, a weight-scale change), or ADR-0176 Stage 1's measured honest load.
pub const PALW_BOND_BUDGET_FINAL_WEIGHT_PER_UNIT_PROVISIONAL_V1: u64 = 40_000_000_000_000;
/// 目安 (lane GC-DUTY, Lead 2026-10-28): open (unsettled) claims per bond. Was one day's claims (72). Live: peak 45 open on one bond,
/// accept → Final median 236 DAA / max 292 while Final is slow. Value = 4 × 72 = 288. Revise: when the measured peak × 4 passes it.
pub const PALW_BOND_BUDGET_MAX_OPEN_CLAIMS_PROVISIONAL_V1: u32 = 288;

/// **V12-b 目安: `palw_bond_budget_v1`'s V3 policy.** Per-day rates per minimum bond: 72 claims, 72 reward blocks, 72 Round rights;
/// reward 0.25 of the unit per day (a day of issuance-bound reward never exceeds the export cap of the capital, so a whole day of
/// forged claims stays below the 0.51 collectable slash); Final weight 1,000 per unit (the mechanics scale — its value is `Unknown`);
/// open claims = one day's claims; burst and concurrent liability at the interim constants (72 DAA × 4, 1000 ‰). The validator still
/// refuses the fence (non-value blockers); the lead pins.
pub fn palw_bond_budget_policy_provisional_v1() -> crate::palw_bond_budget_v1::PalwBondBudgetPolicyV1 {
    use crate::palw_bond_budget_v1::*;
    let mut p = PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1();
    p.capital_unit_sompi = PALW_BOND_BUDGET_CAPITAL_UNIT_PROVISIONAL_SOMPI_V1;
    p.rho = 1;
    p.claims_per_unit = PALW_BOND_BUDGET_DAILY_CLAIMS_PROVISIONAL_V1;
    p.block_units_per_unit = PALW_BOND_BUDGET_DAILY_CLAIMS_PROVISIONAL_V1 * PALW_BUDGET_BLOCK_UNIT_V1;
    p.reward_per_unit_sompi =
        PALW_BOND_BUDGET_CAPITAL_UNIT_PROVISIONAL_SOMPI_V1 / 1_000 * PALW_BOND_BUDGET_EXPORT_CAP_PROVISIONAL_PERMILLE_V1 as u64;
    p.final_weight_per_unit = PALW_BOND_BUDGET_FINAL_WEIGHT_PER_UNIT_PROVISIONAL_V1;
    p.max_open_claims_per_bond = PALW_BOND_BUDGET_MAX_OPEN_CLAIMS_PROVISIONAL_V1;
    p.slice_rights_by_rho = false;
    p.round_rights = PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: PALW_BOND_BUDGET_DAILY_CLAIMS_PROVISIONAL_V1 };
    p.liability_hold_daa = PALW_BOND_BUDGET_LIABILITY_HOLD_PROVISIONAL_DAA_V1;
    p.export_cap_permille = PALW_BOND_BUDGET_EXPORT_CAP_PROVISIONAL_PERMILLE_V1;
    p.weight_value = PalwWeightValuePolicyV1::Unknown;
    p
}

/// **R2 目安: `PalwClaimChallengePolicyV2`'s resource limits, with the decided bootstrap seed.** 64 classes, 1,024 jobs, 1,008 live
/// claims (42 concurrent court sessions × 24: at the measured court-op times — ≤ 1.5 ms median per verify, M1 Max under load —
/// every live claim forged and detected still fits 24 rounds of the court cap), 8 enrollments per job, 1,000,000 stream blocks,
/// 1 round (the initial contract is NonInteractive, R7-RANDOM 4). The review / evidence digests stay zero (external review, not
/// values), so the fence stays refused. Revise: M-C on minimum hardware, the court session length in DAA, an interactive compiler.
pub fn palw_claim_challenge_policy_provisional_v1(
    params: &crate::config::params::Params,
) -> crate::palw_claim_challenge_v2::PalwClaimChallengePolicyV2 {
    let probe =
        crate::palw_claim_challenge_v2::PalwClaimChallengeFenceV2::unapproved_probe_at(crate::config::params::ForkActivation::never());
    crate::palw_claim_challenge_v2::PalwClaimChallengePolicyV2 {
        maximum_classes: 64,
        maximum_jobs: 1_024,
        maximum_live_claims: 42 * 24,
        maximum_enrollments_per_job: 8,
        maximum_stream_blocks: 1_000_000,
        maximum_rounds: 1,
        bootstrap_seed: Some(params.palw_paid_claim_bootstrap_seed_decided_v1()),
        ..probe.policy
    }
}

/// R2 目安: the opt-in source seal deposit `d_src` — econ B12's parity value `c_ab·T_w/T_ab − carrier` at the interim terms (28.6,
/// 49 % reporter share). Revise: when the beacon shape and admission fees are pinned.
pub const PALW_SOURCE_SEAL_DEPOSIT_PROVISIONAL_SOMPI_V1: u64 = 28_600 * (crate::constants::SOMPI_PER_KASPA / 1_000);
/// R2 目安: `N_max`, sampled attempts per seal window (S4) — the interim sealed shape's `R + 1 = 3`. Revise: a measured delay value.
pub const PALW_SOURCE_MAX_CONCURRENT_ATTEMPTS_PROVISIONAL_V1: u32 = 3;

/// **R2 / R3 目安: the work-beacon shape** (RFC-0007 §VI.2 `k, D, delay, retry`; the RFC-0010 Panel beacon uses the same). NOT an
/// approval: `approved_panel_beacon_policies_v1()` stays empty until the external bias review (§VI.8 item 2 / P0-10), which revises it.
/// `k = 3` distinct works (P(all three adversarial) = 1.6 % at f = 0.25); `D = 200` DAA (the upper end of the observed quiet-time
/// finality depth, 7–10 h at 184 s/DAA); anchor delay 2 and window 40 slots (the interim sealed shape econ B12 measured); retry 2
/// (the interim `R + 1 = 3`: an adaptive last mover picks among at most 3 outputs).
pub const PALW_WORK_BEACON_K_PROVISIONAL_V1: u32 = 3;
pub const PALW_WORK_BEACON_SETTLEMENT_DEPTH_PROVISIONAL_DAA_V1: u64 = 200;
pub const PALW_WORK_BEACON_ANCHOR_DELAY_PROVISIONAL_SLOTS_V1: u64 = 2;
pub const PALW_WORK_BEACON_WINDOW_PROVISIONAL_SLOTS_V1: u64 = 40;
pub const PALW_WORK_BEACON_RETRY_LIMIT_PROVISIONAL_V1: u32 = 2;

/// **R1 目安: RFC-0006 R6-DRAW / R6-PRICE** (dormant behind N3; values only). Retry bound 2; 1 class seat + 1 outsider per stratum;
/// at most 4 retained 100k populations (the mechanics already refuse a fifth); worst-cell public material 33,629 B (the largest court
/// object M-C measured: a position close with two ML-DSA-87 signatures); worst fresh-verifier time 134 ms (the largest per-verify
/// maximum under load, ≈ 100× the medians); per-shard court capacity 42 sessions (the court cap). Revise: release-hardware
/// measurements of a real sharded class.
pub const PALW_RFC0006_DRAW_RETRY_BOUND_PROVISIONAL_V1: u32 = 2;
pub const PALW_RFC0006_SEATS_PER_STRATUM_PROVISIONAL_V1: u32 = 1;
pub const PALW_RFC0006_OUTSIDERS_PER_STRATUM_PROVISIONAL_V1: u32 = 1;
pub const PALW_RFC0006_RETAINED_POPULATIONS_PROVISIONAL_V1: u32 = 4;
pub const PALW_RFC0006_WORST_CELL_BYTES_PROVISIONAL_V1: u64 = 33_629;
pub const PALW_RFC0006_WORST_CELL_VERIFY_PROVISIONAL_US_V1: u64 = 134_000;
pub const PALW_RFC0006_SHARD_COURT_CAPACITY_PROVISIONAL_V1: u32 = 42;

/// **R4 / R8 目安: RFC-0016's value set** — the calibration starting point (user decisions 2026-10-15) with ε = 0 and no entrant cap
/// (ADR-0179: no entrants under the tier fence) and the capacity safety factor at 0.5 (half the measured capacity is credited); the
/// swept probes kept. NOT a measured record: `PALW_RFC0016_MEASURED_RECORDS_V1` stays empty (the lead's consolidation pins). Revise:
/// RFC-0016 §3.6's three-stage derivation.
pub fn palw_rfc0016_values_provisional_v1() -> crate::palw_rfc0016_v1::PalwRfc0016ValuesV1 {
    crate::palw_rfc0016_v1::PalwRfc0016ValuesV1 {
        entrant_reserve_permille: 0,
        entrant_max_per_model_permille: 0,
        capacity_safety_permille: 500,
        ..crate::palw_rfc0016_v1::values::PALW_RFC0016_CALIBRATION_START_V1
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
        assert_eq!(PALW_P4_VALUES_STATUS_V1, "pinned 2026-10-22 in the parameter register; fences unarmed");
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
        // block_opportunity = false is the V+ form: the policy validates; the fence decides it against V+.
        assert!(!refused(&|q| q.average.as_mut().unwrap().block_opportunity = false));
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
        // block_opportunity = false without V+ at or below it would apply P_m to blocks nowhere: refused.
        let mut no_gate = both.clone();
        no_gate.palw_model_bond_allocation_v1.as_mut().unwrap().policy.average.as_mut().unwrap().block_opportunity = false;
        let PalwModeV2Error::Invalid(why) = no_gate.validate_palw_model_bond_allocation_v1().unwrap_err() else { panic!("Invalid") };
        assert!(why.contains("block_opportunity is false exactly"), "{why}");
        // With V+ at the same height, false is the one-gate form and only the missing approved value refuses it.
        let mut v_plus = no_gate.clone();
        v_plus.palw_model_block_rights_v1 = Some(ForkActivation::new(16_000));
        let PalwModeV2Error::Invalid(why) = v_plus.validate_palw_model_bond_allocation_v1().unwrap_err() else { panic!("Invalid") };
        assert!(why.contains("no approved policy is pinned"), "{why}");
        // ... and true under V+ is refused (two gates).
        let mut twice = both.clone();
        twice.palw_model_block_rights_v1 = Some(ForkActivation::new(16_000));
        let PalwModeV2Error::Invalid(why) = twice.validate_palw_model_bond_allocation_v1().unwrap_err() else { panic!("Invalid") };
        assert!(why.contains("block_opportunity is false exactly"), "{why}");
        // A malformed policy is named first.
        let mut bad = both.clone();
        bad.palw_model_bond_allocation_v1.as_mut().unwrap().policy.epoch_daa = 1_000;
        let PalwModeV2Error::Invalid(why) = bad.validate_palw_model_bond_allocation_v1().unwrap_err() else { panic!("Invalid") };
        assert!(why.contains("policy is invalid"), "{why}");
        // The shipped t12 preset is untouched.
        assert_eq!(palw_t12_shipped_params().palw_model_bond_allocation_v1.as_ref().map(|f| f.activation.daa_score()), Some(18_400));
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
    fn p4_provisional_policies_validate_and_their_fences_stay_refused() {
        let t12 = palw_t12_shipped_params();
        // V12-b: the V3 budget policy is well formed, and the fence is still refused by its non-value blockers.
        let b = palw_bond_budget_policy_provisional_v1();
        b.validate().expect("the provisional budget policy validates");
        let daily = palw_bond_hold_daily_caps_v1(&b, PALW_BOND_BUDGET_CAPITAL_UNIT_PROVISIONAL_SOMPI_V1);
        assert_eq!(daily.claims, 72);
        assert_eq!(daily.reward_sompi, PALW_BOND_BUDGET_CAPITAL_UNIT_PROVISIONAL_SOMPI_V1 / 4, "a day's reward = 0.25 of the capital");
        assert!(daily.reward_sompi < PALW_BOND_BUDGET_CAPITAL_UNIT_PROVISIONAL_SOMPI_V1 / 1_000 * 510, "below the 0.51 collectable");
        let mut armed = t12.clone();
        armed.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1 { activation: ForkActivation::new(16_000), policy: b.clone() });
        // Lane VAL 2026-10-26: the estimate is pinned in the approved list, so a refusal (if any) is never the missing value.
        assert!(crate::palw_bond_budget_v1::palw_bond_budget_approved_policy_digests_v1().contains(&b.digest()));
        if let Err(e) = armed.validate_palw_bond_budget_v1() {
            assert!(!format!("{e:?}").contains("no approved policy"), "{e:?}");
        }
        // Any other policy is still refused for the missing approval.
        let mut other = b.clone();
        other.claims_per_unit += 1;
        other.max_open_claims_per_bond += 1;
        if other.validate().is_ok() {
            assert!(!crate::palw_bond_budget_v1::palw_bond_budget_approved_policy_digests_v1().contains(&other.digest()));
        }
        let mut bad = b.clone();
        bad.export_cap_permille = 511;
        assert!(bad.validate().is_err());
        // R2: the provisional paid-claim policy validates; the fence stays refused (no review evidence).
        let c = palw_claim_challenge_policy_provisional_v1(&t12);
        c.validate().expect("the provisional paid-claim policy validates");
        assert_eq!(c.bootstrap_seed, Some(t12.palw_paid_claim_bootstrap_seed_decided_v1()));
        let mut armed = t12.clone();
        armed.palw_claim_challenge_v2 =
            Some(crate::palw_claim_challenge_v2::PalwClaimChallengeFenceV2 { activation: ForkActivation::new(16_000), policy: c });
        assert!(armed.validate_palw_claim_challenge_v2().is_err());
        // R4/R8: the provisional RFC-0016 set is no placeholder and conforms, but no measured record pins it.
        let v = palw_rfc0016_values_provisional_v1();
        assert!(!v.is_placeholder());
        crate::palw_rfc0016_v1::values::palw_rfc0016_calibration_conforms_v1(&v).expect("k = 1, ε = 0");
        assert!(!crate::palw_rfc0016_v1::values::palw_rfc0016_values_measured_v1(
            &v,
            crate::palw_rfc0016_v1::values::PALW_RFC0016_MEASURED_RECORDS_V1
        ));
        // R3: the beacon shape is not an approval.
        assert_eq!(
            crate::palw_panel_beacon_v1::approved_panel_beacon_policies_v1().len(),
            1,
            "the 2026-10-26 user-approved policy only"
        );
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
