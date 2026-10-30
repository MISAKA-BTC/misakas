//! **RFC-0015 §13.1 (negative gate tests) and §13.3 (`ACTIVATION_ALLOWED`, an AND contract) as code** — the activation gate record of
//! `palw_panel_free_v1` (`kaspa_consensus_core::palw_panel_free_gate_v1`).
//!
//! Every case builds an otherwise complete record and breaks exactly one thing that §13.1 names (an operator-only proof, a private
//! endpoint, the canonical held/fused gaps, a borrowed trace answering DA, a Final during a non-seat pursuit, unverified resources or
//! inclusion, a missing class/plan record or a mismatched network identity, missing or bypassed Block / Reward / Final-weight caps or
//! common DAA hold), and asserts the gate refuses it by name. The node's own validation is asserted to refuse even the complete
//! record: this binary pins none (`PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1` is empty; the 2026-10-12 amendment, N5, defers Panel=0).
//!
//! Test source only (written 2026-10-15; not run in this change — no cargo invocation was allowed). Nothing here is a PASS record.

#![allow(warnings)]
use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_bond_budget_v1::{PalwBondBudgetFenceV1, PalwModelBondAllocationFenceV1};
use kaspa_consensus_core::palw_mode_v2::PalwModeV2Error;
use kaspa_consensus_core::palw_panel_free_gate_v1::{
    PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1, PALW_PANEL_FREE_GATE_RECORD_VERSION_V1, PalwGateVerdictV1, PalwPanelFreeConjunctV1 as C,
    PalwPanelFreeGateEntryV1, PalwPanelFreeGateRecordV1, PalwPanelFreeGateRefusalV1 as R, PalwPanelFreeRewardedProfilesV1,
    palw_panel_free_allocation_policy_digest_v1, palw_panel_free_terms_digest_v1,
};
use kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1;
use kaspa_hashes::Hash64;

const AT: u64 = 20_000;

/// testnet-12's params with every fence a conjunct is mechanically checked against armed at `AT` (validation bypassed: this is the
/// shape a future release would have to have, not a network).
fn coupled_params() -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(AT));
    p.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1::at(ForkActivation::new(AT)));
    p.palw_model_bond_allocation_v1 = Some(PalwModelBondAllocationFenceV1::at(ForkActivation::new(AT)));
    p.palw_court_model_scope_v1 = Some(ForkActivation::new(AT));
    p.palw_fork_choice_rule_e_v1 = Some(ForkActivation::new(AT));
    p.palw_staged_public_court_v1 = Some(ForkActivation::new(AT));
    p.palw_verifier_pay_v1 = Some(ForkActivation::new(AT));
    // Optional rewarded families are enabled explicitly by each gate case.
    p.palw_staged_tir_route_v1 = None;
    p.palw_staged_float_constraint_v1 = None;
    p
}

fn fence() -> PalwPanelFreeFenceV1 {
    PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(AT), Vec::new())
}

/// A record every conjunct of which is PASS with its own evidence and review digests, bound to `p` and `f`.
fn complete_record(p: &Params, f: &PalwPanelFreeFenceV1) -> PalwPanelFreeGateRecordV1 {
    PalwPanelFreeGateRecordV1 {
        version: PALW_PANEL_FREE_GATE_RECORD_VERSION_V1,
        genesis: p.genesis.hash,
        activation_daa: AT,
        terms: palw_panel_free_terms_digest_v1(f),
        bond_budget_policy: p.palw_bond_budget_v1.as_ref().unwrap().policy.digest(),
        allocation_policy: palw_panel_free_allocation_policy_digest_v1(&p.palw_model_bond_allocation_v1.as_ref().unwrap().policy),
        profiles: PalwPanelFreeRewardedProfilesV1::default(),
        entries: C::ALL
            .iter()
            .enumerate()
            .map(|(i, c)| PalwPanelFreeGateEntryV1 {
                conjunct: *c,
                verdict: PalwGateVerdictV1::Pass,
                evidence: Hash64::from_u64_word(1_000 + i as u64),
                review: Hash64::from_u64_word(2_000 + i as u64),
            })
            .collect(),
    }
}

/// The gate's verdict on `r` with `r` itself pinned (so every check but the pin is exercised).
fn verdict(p: &Params, f: &PalwPanelFreeFenceV1, r: &PalwPanelFreeGateRecordV1) -> Result<(), R> {
    r.verify_v1(p, f, &[r.digest().as_bytes()])
}

fn with_entry(mut r: PalwPanelFreeGateRecordV1, c: C, edit: impl FnOnce(&mut PalwPanelFreeGateEntryV1)) -> PalwPanelFreeGateRecordV1 {
    edit(r.entries.iter_mut().find(|e| e.conjunct == c).expect("every conjunct is listed"));
    r
}

#[test]
fn rfc0015_gate_this_binary_pins_no_record_and_refuses_even_a_complete_one() {
    assert!(PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1.is_empty());
    let (p, f) = (coupled_params(), fence());
    let r = complete_record(&p, &f);
    assert_eq!(verdict(&p, &f, &r), Ok(()), "the record is complete: only the pin is missing");
    assert_eq!(r.verify_v1(&p, &f, PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1), Err(R::NotPinned));
    let mut armed = p.clone();
    let mut gated = f.clone();
    gated.activation_gate = Some(r);
    armed.palw_panel_free_v1 = Some(gated);
    assert_eq!(armed.validate_palw_panel_free_v1(), Err(PalwModeV2Error::Invalid(R::NotPinned.reason())));
    assert!(armed.validate_palw_v2().is_err());
    // And without a record the refusal says so (no operator flag reaches the fence's value).
    let mut bare = p.clone();
    bare.palw_panel_free_v1 = Some(fence());
    assert!(
        matches!(bare.validate_palw_panel_free_v1(), Err(PalwModeV2Error::Invalid(m)) if m.contains("no authenticated activation gate record"))
    );
}

/// §13.1, one bullet a case: the deliberately broken build is refused.
#[test]
fn rfc0015_gate_each_section_13_1_defect_is_refused_by_name() {
    let (p, f) = (coupled_params(), fence());
    let full = || complete_record(&p, &f);
    let fail = |c: C, v: PalwGateVerdictV1| with_entry(full(), c, |e| e.verdict = v);

    // * operator-only proofs / insufficient public bond qualification or API → G14 is not PASS.
    assert_eq!(
        verdict(&p, &f, &fail(C::G14PublicFraudProsecutionComplete, PalwGateVerdictV1::Fail)),
        Err(R::NotPass(C::G14PublicFraudProsecutionComplete))
    );
    // * a producer heap / drill instance / private endpoint is needed → G14 UNKNOWN is never PASS.
    assert_eq!(
        verdict(&p, &f, &fail(C::G14PublicFraudProsecutionComplete, PalwGateVerdictV1::Unknown)),
        Err(R::NotPass(C::G14PublicFraudProsecutionComplete))
    );
    // * the canonical held/fused cap / prefix / committed-tile gaps remain → a rewarded profile's dispute gate fails.
    assert_eq!(
        verdict(&p, &f, &fail(C::RewardedProfileDisputeResourceGates, PalwGateVerdictV1::Fail)),
        Err(R::NotPass(C::RewardedProfileDisputeResourceGates))
    );
    // * a garbage / borrowed trace answering DA ends the prosecution → RFC-0014 claim-specific prosecution fails.
    assert_eq!(
        verdict(&p, &f, &fail(C::Rfc14ClaimSpecificProsecutionAndRuleE, PalwGateVerdictV1::Fail)),
        Err(R::NotPass(C::Rfc14ClaimSpecificProsecutionAndRuleE))
    );
    // * Final during a non-seat pursuit, or a direct proof refused for an open court → the lifecycle/court/Final conjunct fails.
    assert_eq!(
        verdict(&p, &f, &fail(C::LifecycleCourtFinalImplementation, PalwGateVerdictV1::Fail)),
        Err(R::NotPass(C::LifecycleCourtFinalImplementation))
    );
    // * maximum profile, cold material time, resource / inclusion bounds unverified → UNKNOWN.
    assert_eq!(
        verdict(&p, &f, &fail(C::PublicMaterialWatcherInclusionAssumptions, PalwGateVerdictV1::Unknown)),
        Err(R::NotPass(C::PublicMaterialWatcherInclusionAssumptions))
    );
    // * the class/plan completion record is missing → the record does not cover every conjunct.
    let mut missing = full();
    missing.entries.retain(|e| e.conjunct != C::RewardedProfileDisputeResourceGates);
    assert_eq!(verdict(&p, &f, &missing), Err(R::Coverage));
    // * the staged upgrade's network identity does not match → another genesis, another height, other terms.
    let mut other_net = full();
    other_net.genesis = Hash64::from_u64_word(7);
    assert_eq!(verdict(&p, &f, &other_net), Err(R::WrongNetwork));
    let mut other_height = full();
    other_height.activation_daa = AT + 1;
    assert_eq!(verdict(&p, &f, &other_height), Err(R::WrongActivation));
    let mut other_terms = fence();
    other_terms.economics.reservation_per_claim += 1;
    assert_eq!(verdict(&p, &other_terms, &full()), Err(R::WrongTerms), "a record reviewed for other numbers authorizes none");
    // * Block / Reward / Final-weight caps or the common DAA hold unimplemented or undetermined → UNKNOWN conjunct, or the budget
    //   engine not armed at or below, armed above, or with another policy than the reviewed one.
    assert_eq!(
        verdict(&p, &f, &fail(C::ClassIndependentCapsAndCommonDaaHold, PalwGateVerdictV1::Unknown)),
        Err(R::NotPass(C::ClassIndependentCapsAndCommonDaaHold))
    );
    let mut no_budget = p.clone();
    no_budget.palw_bond_budget_v1 = None;
    assert_eq!(verdict(&no_budget, &f, &full()), Err(R::FenceNotArmedAtOrBelow("palw_bond_budget_v1")));
    let mut late_budget = p.clone();
    late_budget.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1::at(ForkActivation::new(AT + 1)));
    assert_eq!(verdict(&late_budget, &f, &full()), Err(R::FenceNotArmedAtOrBelow("palw_bond_budget_v1")));
    let mut never_budget = p.clone();
    never_budget.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1::at(ForkActivation::never()));
    assert_eq!(verdict(&never_budget, &f, &full()), Err(R::FenceNotArmedAtOrBelow("palw_bond_budget_v1")));
    let mut other_budget = p.clone();
    other_budget.palw_bond_budget_v1.as_mut().unwrap().policy.window_daa += 1;
    assert_eq!(verdict(&other_budget, &f, &full()), Err(R::BondBudgetPolicy), "another W is another reviewed economics");
}

/// The other §13.3 conjuncts: each is required, none can be skipped, and the mechanical couplings name their fence.
#[test]
fn rfc0015_gate_every_conjunct_and_every_coupled_fence_is_required() {
    let (p, f) = (coupled_params(), fence());
    for c in C::ALL {
        for v in [PalwGateVerdictV1::Fail, PalwGateVerdictV1::Unknown] {
            assert_eq!(verdict(&p, &f, &with_entry(complete_record(&p, &f), c, |e| e.verdict = v)), Err(R::NotPass(c)), "{c:?} {v:?}");
        }
        let mut gone = complete_record(&p, &f);
        gone.entries.retain(|e| e.conjunct != c);
        assert_eq!(verdict(&p, &f, &gone), Err(R::Coverage), "{c:?} missing");
    }
    // N5: a record that has everything but the user's ruling lifting the 2026-10-12 deferral is refused.
    assert_eq!(
        verdict(
            &p,
            &f,
            &with_entry(complete_record(&p, &f), C::PanelZeroDeferralLiftedByRuling, |e| e.verdict = PalwGateVerdictV1::Unknown)
        ),
        Err(R::NotPass(C::PanelZeroDeferralLiftedByRuling))
    );
    type Unset = fn(&mut Params);
    let couplings: [(&str, Unset); 7] = [
        ("palw_probabilistic_constraints_v1", |q| q.palw_probabilistic_constraints_v1 = None),
        ("palw_bond_budget_v1", |q| q.palw_bond_budget_v1 = None),
        ("palw_model_bond_allocation_v1", |q| q.palw_model_bond_allocation_v1 = None),
        ("palw_court_model_scope_v1", |q| q.palw_court_model_scope_v1 = None),
        ("palw_fork_choice_rule_e_v1", |q| q.palw_fork_choice_rule_e_v1 = None),
        ("palw_staged_public_court_v1", |q| q.palw_staged_public_court_v1 = None),
        ("palw_verifier_pay_v1", |q| q.palw_verifier_pay_v1 = None),
    ];
    for (name, unset) in couplings {
        let mut q = p.clone();
        unset(&mut q);
        assert_eq!(verdict(&q, &f, &complete_record(&p, &f)), Err(R::FenceNotArmedAtOrBelow(name)), "{name}");
    }
    let mut other_alloc = p.clone();
    other_alloc.palw_model_bond_allocation_v1.as_mut().unwrap().policy.epoch_daa += 1;
    assert_eq!(verdict(&other_alloc, &f, &complete_record(&p, &f)), Err(R::AllocationPolicy));
    // An unarmed fence (`never()`) has no activation to authorize.
    let never = PalwPanelFreeFenceV1::interim_v1(ForkActivation::never(), Vec::new());
    assert_eq!(verdict(&p, &never, &complete_record(&p, &f)), Err(R::WrongActivation));
}

/// The record's own form: version, order, unique non-zero evidence and review digests.
#[test]
fn rfc0015_gate_the_record_form_is_strict() {
    let (p, f) = (coupled_params(), fence());
    let mut v = complete_record(&p, &f);
    v.version += 1;
    assert_eq!(verdict(&p, &f, &v), Err(R::Version));
    let mut swapped = complete_record(&p, &f);
    swapped.entries.swap(0, 1);
    assert_eq!(verdict(&p, &f, &swapped), Err(R::Coverage));
    let mut twice = complete_record(&p, &f);
    let first = twice.entries[0].clone();
    twice.entries[1] = first;
    assert_eq!(verdict(&p, &f, &twice), Err(R::Coverage));
    let c = C::IndependentAdversarialNodeE2eRecoveryMigration;
    assert_eq!(verdict(&p, &f, &with_entry(complete_record(&p, &f), c, |e| e.evidence = Hash64::default())), Err(R::Evidence(c)));
    assert_eq!(verdict(&p, &f, &with_entry(complete_record(&p, &f), c, |e| e.review = Hash64::default())), Err(R::Evidence(c)));
    assert_eq!(verdict(&p, &f, &with_entry(complete_record(&p, &f), c, |e| e.review = e.evidence)), Err(R::Evidence(c)));
    let reused = Hash64::from_u64_word(1_000); // the first conjunct's evidence
    assert_eq!(verdict(&p, &f, &with_entry(complete_record(&p, &f), c, |e| e.evidence = reused)), Err(R::Evidence(c)));
}

/// The record is identity, Some-only: a fence without one hashes exactly as before this change; with one, the record's digest is
/// part of both fingerprints.
#[test]
fn rfc0015_gate_the_record_is_identity_some_only() {
    let p = palw_t12_shipped_params();
    let id = |f: PalwPanelFreeFenceV1| {
        let mut q = p.clone();
        q.palw_panel_free_v1 = Some(f);
        (q.consensus_params_id(), q.consensus_schedule_id())
    };
    let bare = fence();
    assert!(bare.activation_gate.is_none(), "no value this binary builds carries a record");
    let mut gated = fence();
    gated.activation_gate = Some(complete_record(&coupled_params(), &fence()));
    assert_ne!(id(bare.clone()), id(gated.clone()));
    let mut other = gated.clone();
    other.activation_gate.as_mut().unwrap().entries[0].evidence = Hash64::from_u64_word(9_999);
    assert_ne!(id(gated), id(other), "the record's content is hashed");
    assert_eq!(id(bare.clone()), id(bare), "deterministic");
    assert!(!p.palw_panel_free_gated_v1(), "testnet-12 carries no fence, hence no record");
}

/// The staged TIR / Float32 routes are required exactly when the reviewed rewarded profile set includes their family (lead default
/// 2026-10-15): absent families need no route; a present family without its route armed at or below is refused; the profile set is
/// part of the record's identity.
#[test]
fn rfc0015_gate_the_staged_g14_routes_follow_the_rewarded_profile_set() {
    let (p, f) = (coupled_params(), fence());
    assert_eq!(verdict(&p, &f, &complete_record(&p, &f)), Ok(()), "no TIR / Float32 family rewarded: neither route required");
    let mut tir = complete_record(&p, &f);
    tir.profiles.tir_required_family = true;
    assert_eq!(verdict(&p, &f, &tir), Err(R::FenceNotArmedAtOrBelow("palw_staged_tir_route_v1")));
    let mut with_tir = p.clone();
    with_tir.palw_staged_tir_route_v1 = Some(ForkActivation::new(AT));
    assert_eq!(verdict(&with_tir, &f, &tir), Ok(()));
    let mut late = p.clone();
    late.palw_staged_tir_route_v1 = Some(ForkActivation::new(AT + 1));
    assert_eq!(verdict(&late, &f, &tir), Err(R::FenceNotArmedAtOrBelow("palw_staged_tir_route_v1")));
    let mut float = complete_record(&p, &f);
    float.profiles.float32_constraint = true;
    assert_eq!(verdict(&p, &f, &float), Err(R::FenceNotArmedAtOrBelow("palw_staged_float_constraint_v1")));
    let mut with_float = p.clone();
    with_float.palw_staged_float_constraint_v1 = Some(ForkActivation::new(AT));
    assert_eq!(verdict(&with_float, &f, &float), Ok(()));
    assert_ne!(tir.digest(), complete_record(&p, &f).digest(), "the profile set is bound in the record");
}
