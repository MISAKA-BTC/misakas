//! Actual fold/reward consumer tests. Draw/attestation rows use the existing test
//! seam; certification verdict, retirement, payout and replay are production code.
//! This is not the remaining node/pruning G14 acceptance drill.
use super::*;
use crate::palw_rfc0018_v1::PalwRfc0018MirrorV1;
use crate::palw_security_certification_v1::*;

fn rules() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 100, 10, 20, 50, 1000, h64(1), 4, 1000, 100, 1000, 0)
        .unwrap()
        .with_worker_carve_permille(620)
        .unwrap()
        .with_claim_retirement_daa(1)
        .unwrap()
        .with_claim_local_settlement_from_daa(Some(100))
        .with_rfc0018_v1(Some(PalwRfc0018MirrorV1 {
            gate_c_from_daa: Some(99),
            certification: PalwSecurityCertificationPolicyV1 {
                m: 1,
                k: 1,
                veto: 0,
                anchor_delay_spans: 1,
                attest_window_daa: 1,
                prosecution_window_daa: 1,
                certify_by_daa: 20,
                verifier_maturity_daa: 0,
                verifier_min_collateral_sompi: 1,
                draws_per_epoch: 64,
                epoch_daa: 100,
            },
            ..Default::default()
        }))
}
fn extra() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        claim_local_rule_context_v1: Some(([11; 32], [12; 32])),
        kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
            network_domain: h64(10),
            ruleset_digest: h64(11),
            chain_genesis: h64(12),
            ..Default::default()
        }),
        ..Default::default()
    }
}
fn step(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    n: u64,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    env: Option<&PalwAttemptEnvelopeV2>,
    x: &PalwTransitionExtrasV1,
) -> (PalwChainStateV2, PalwStateDeltaV2) {
    let point = PalwBlockContextV2 { subsidy: 1000, ..ctx(n, daa, n) };
    let out = apply_palw_transition_v2_with_extras(s, p, &point, objects, env, false, false, false, false, x)
        .unwrap_or_else(|e| panic!("DAA {daa}: {e:?}"));
    out.0.assert_internal_consistency(p).unwrap_or_else(|e| panic!("DAA {daa} consistency: {e:?}"));
    assert_eq!(apply_delta_v2(s, &out.1, p).unwrap().state_root(), out.0.state_root());
    assert_eq!(revert_delta_v2(&out.0, &out.1, p).unwrap().state_root(), s.state_root());
    out
}
fn row(s: &PalwChainStateV2, id: Hash64) -> PalwClaimCertificationRowV1 {
    (*super::super::palw_claim_local_rewards_v1::retained_certification_v1(s, id).unwrap().unwrap()).clone()
}
fn bind(id: Hash64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::PanelBound {
        claim: id,
        anchor: h64(77),
        seats: vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: op_id(21) }],
    }
}
fn license(id: Hash64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seat_says(true) }
}
fn world(all: bool) -> (PalwStateParamsV2, PalwChainStateV2, [Hash64; 3]) {
    world_at(all, 100)
}
fn world_at(all: bool, first: u64) -> (PalwStateParamsV2, PalwChainStateV2, [Hash64; 3]) {
    // The old-cohort case isolates certification-time payment, with retirement
    // disabled. Its released scheduling rules are deliberately left untouched.
    let p = if first < 100 { rules().with_claim_retirement_daa(0).unwrap() } else { rules() };
    let mut objects = register_class_and_bond();
    objects.push(seat_bond_reg(2, 1000));
    let (mut s, _) = step(&PalwChainStateV2::genesis(), &p, 1, first - 1, &objects, None, &extra());
    let mut ids = [ZERO_HASH64; 3];
    for i in 0..3 {
        let mut e = attempt(40, i as u64 + 1);
        e.attempt.execution_root = h64(if i == 1 { 601 } else { 600 });
        e.attempt.trace_retention_daa = 1000;
        ids[i] = attempt_id_v2(&e.attempt);
        s = step(&s, &p, i as u64 + 2, first + i as u64, &[], Some(&e), &extra()).0;
        assert!(s.claim(&ids[i]).is_some());
    }
    let chosen: Vec<_> = ids.iter().enumerate().filter(|(i, _)| all || *i > 0).map(|(_, id)| *id).collect();
    s = step(&s, &p, 5, 103, &chosen.iter().copied().map(bind).collect::<Vec<_>>(), None, &extra()).0;
    s = step(&s, &p, 6, 104, &chosen.iter().copied().map(license).collect::<Vec<_>>(), None, &extra()).0;
    s = step(&s, &p, 7, 125, &[], None, &extra()).0;
    let mut x = extra();
    for id in chosen {
        assert!(matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
        let mut r = row(&s, id);
        r.anchor = Some((1, [7; 64]));
        r.draws = vec![crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond_key(2))];
        r.drawn_daa = 126;
        r.deadline_daa = 127;
        r.attestations = vec![crate::palw_model_eligibility_v1::PalwEligibilityAttestationRowV1 {
            verifier: r.draws[0],
            verdict: crate::palw_model_eligibility_v1::PalwEligibilityVerdictV1::Reproduced,
            transcript: [9; 64],
            attested_daa: 126,
        }];
        let mut k = vec![CERTIFICATION_CLAIM_PREFIX_V1];
        k.extend_from_slice(&palw_certification_id_v1(&id));
        x.exec_v2_test_kernel_rows.push((PALW_SECURITY_CERTIFICATION_TABLE_V1, k, Some(borsh::to_vec(&r).unwrap())));
    }
    s = step(&s, &p, 8, 126, &[], None, &x).0;
    s = step(&s, &p, 9, 128, &[], None, &extra()).0;
    for id in ids.iter().skip(if all { 0 } else { 1 }) {
        assert!(row(&s, *id).outcome.unwrap().0.certifies());
        assert_eq!(row(&s, *id).held_reward, 620);
    }
    (p, s, ids)
}
#[test]
fn rfc23s2_reward_independent_claim_pays_while_dependency_is_held_then_denied() {
    let (p, s, [a, b, c]) = world(false);
    let collateral = s.bond(&bond_key(1)).unwrap().collateral;
    let (s, _) = step(&s, &p, 10, 148, &[], None, &extra());
    assert!(s.claim(&b).is_none(), "independent B retires without A");
    assert!(s.claim(&a).is_some());
    assert!(s.claim(&c).is_some());
    assert_eq!(s.pending_payouts.get(&palw_certification_payout_key_v1(&b)).map(|p| p.amount), Some(620));
    assert!(!s.pending_payouts.contains_key(&palw_certification_payout_key_v1(&c)));
    assert_eq!(row(&s, c).held_reward, 620);
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s)).unwrap();
    crate::palw_native_resource_v1::preflight_native_carriage_v1(bytes.as_slice(), bytes.len() as u64).unwrap();
    let restored =
        borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state_v3(&p, Some(s.state_root()), false, None).unwrap();
    assert_eq!(restored.state_root(), s.state_root());
    let (s, _) = step(&restored, &p, 11, 149, &[], None, &extra());
    assert!(!s.pending_payouts.contains_key(&palw_certification_payout_key_v1(&b)), "no second B payment");
    let (s, _) = step(&s, &p, 12, 201, &[], None, &extra());
    assert!(!s.pending_payouts.contains_key(&palw_certification_payout_key_v1(&c)));
    assert_eq!(row(&s, c).held_reward, 0, "A timeout denies C's unpaid amount");
    assert!(row(&s, c).outcome.unwrap().0.certifies(), "dependency denial does not falsify raw Gate C approval");
    assert_eq!(s.bond(&bond_key(1)).unwrap().collateral, collateral, "no fraud slash for a dependency timeout");
}
#[test]
fn rfc23s2_reward_retired_source_history_releases_once_and_clears_hold_hint() {
    let (p, s, [a, b, c]) = world(true);
    let (s, _) = step(&s, &p, 10, 148, &[], None, &extra());
    assert!(s.claim(&a).is_none());
    assert!(s.claim(&b).is_none());
    let first = s.pending_payouts.get(&palw_certification_payout_key_v1(&c)).map(|p| p.amount).unwrap_or(0);
    let (s, _) = step(&s, &p, 11, 149, &[], None, &extra());
    let second = s.pending_payouts.get(&palw_certification_payout_key_v1(&c)).map(|p| p.amount).unwrap_or(0);
    assert_eq!(first + second, 620, "source removal order never strands or doubles C's payment");
    let (s, _) = step(&s, &p, 12, 150, &[], None, &extra());
    assert!(s.claim(&c).is_none());
    assert!(!s.pending_payouts.contains_key(&palw_certification_payout_key_v1(&c)));
}

#[test]
fn rfc23s2_reward_preexisting_cohort_keeps_its_released_payment_terms() {
    let (_p, s, [a, b, c]) = world_at(true, 99);
    assert_eq!(s.claim(&a).unwrap().accepted_daa, 99);
    assert_eq!(
        s.pending_payouts.get(&palw_certification_payout_key_v1(&a)).map(|p| p.amount),
        Some(620),
        "old cohort's Gate C reward stays immediate across the new owner fence"
    );
    for id in [b, c] {
        assert!(!s.pending_payouts.contains_key(&palw_certification_payout_key_v1(&id)));
        assert_eq!(row(&s, id).held_reward, 620);
    }
}

#[test]
fn rfc23s2_reward_source_conviction_denies_consumer_without_an_extra_slash() {
    let (p, s, [a, b, c]) = world(false);
    let x = extra();
    let mut builder = TransitionBuilder::new(&s, &p, false, false, false, false, &x);
    let source = builder.state.claim(&a).unwrap().clone();
    // Existing conviction writer seam. Objective proof/filer admission is not
    // exercised here and remains part of the node acceptance work.
    builder.void_and_slash_at(a, &source, 129, PalwVoidReasonV2::CourtFraud, None).unwrap();
    let after_source = builder.state.bond(&bond_key(1)).unwrap().collateral;
    assert!(after_source < s.bond(&bond_key(1)).unwrap().collateral);
    let consumer = builder.state.claim(&c).unwrap().clone();
    for _ in 0..2 {
        super::super::palw_security_certification_fold_v1::try_release_claim_local_reward_v1(&mut builder, 129, &c, &consumer, false)
            .unwrap();
        assert_eq!(builder.state.bond(&bond_key(1)).unwrap().collateral, after_source);
        assert_eq!(row(&builder.state, c).held_reward, 0);
        assert!(row(&builder.state, c).outcome.unwrap().0.certifies());
        assert!(!builder.state.pending_payouts.contains_key(&palw_certification_payout_key_v1(&c)));
    }
    assert_eq!(row(&builder.state, b).held_reward, 620, "unrelated B's unpaid amount is unchanged");
}

#[test]
fn rfc23s2_reward_claim_journal_rejects_a_forged_retirement_phase() {
    use crate::palw_claim_local_v1::*;
    let (p, s, [a, _, _]) = world(true);
    let (s, _) = step(&s, &p, 10, 148, &[], None, &extra());
    let key = palw_claim_liability_frame_key_v1(ctx(10, 148, 10).block);
    let raw = &s.kernel_route.as_ref().unwrap().aux[&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())];
    let f = palw_decode_claim_liability_frame_v1(raw).unwrap();
    f.validate_context_v1().unwrap();
    let mut forged = (*f).clone();
    let event = forged
        .native_changes
        .iter_mut()
        .find(|e| e.owner == PalwClaimLocalRowOwnerV1::ChainClaims && e.key.as_slice() == a.as_byte_slice() && e.new.is_none())
        .unwrap();
    let mut old: PalwClaimStateV2 = borsh::from_slice(event.old.as_ref().unwrap()).unwrap();
    assert!(matches!(old.phase, PalwClaimPhaseV2::Final { .. }));
    old.phase = PalwClaimPhaseV2::Voided { voided_daa: 148, reason: PalwVoidReasonV2::CourtFraud };
    event.old = Some(borsh::to_vec(&old).unwrap());
    assert!(forged.validate_context_v1().is_err(), "removal provenance is bound to the actual row event");
}

#[test]
fn rfc23s2_reward_historical_gate_c_preserves_lazy_prefence_and_fail_closed_rules() {
    use crate::palw_rfc0018_v1::{PalwGateCRuleV1, palw_work_certified_from_row_v1};
    let c = palw_claim_template_v1(h64(1), bond_key(1), 100, 2, 1000);
    assert!(palw_work_certified_from_row_v1(&params(), &c, || panic!("legacy must not inspect the new row")));
    let p = rules();
    let mut m = *p.rfc0018_v1().unwrap();
    m.gate_c_rule = PalwGateCRuleV1::FailClosedV1;
    let p = p.with_rfc0018_v1(Some(m));
    assert!(!palw_work_certified_from_row_v1(&p, &c, || panic!("fail-closed must not inspect the new row")));
}

#[test]
fn rfc23s2_reward_native_horizon_alone_never_authenticates_unresolved_inputs() {
    use super::super::palw_claim_local_rewards_v1::{RewardReadinessV1, reward_readiness_v1};
    use crate::palw_work_slice_v2::{PalwWorkPrefixStageV2, PalwWorkRootPhaseV2, PalwWorkRootV2};
    let (p, mut s, [_, b, _]) = world(false);
    assert_eq!(reward_readiness_v1(&s, &p, 129, b, true).unwrap(), RewardReadinessV1::Ready);
    // Isolate the unsupported bridge using a closed, zero-value native receipt.
    // This is a reader fixture, not native job/court admission coverage.
    s.exec_v2_rewards.insert(
        b,
        crate::palw_work_slice_v2::PalwExecV2RewardRowV1 {
            final_daa: 125,
            liability_until: 126,
            allocation_sompi: 0,
            legs: BTreeMap::new(),
        },
    );
    assert_eq!(reward_readiness_v1(&s, &p, 129, b, true).unwrap(), RewardReadinessV1::Hold(u64::MAX));

    // A root's cached Verified prefix cannot stand in for the native Claim row
    // and selected receipt approval. Even when its chain liability has elapsed,
    // absent evidence stays held.
    let root = PalwWorkRootV2 {
        class_id: h64(20),
        canonical_job_id: h64(21),
        job_work_id: h64(22),
        kernel_version: 1,
        plan_root: h64(23),
        total_work: 1,
        boundaries: vec![0, 1],
        initial_state_root: h64(24),
        evidence_policy_root: h64(25),
        root_bond: bond_key(1),
        extra_executors: Vec::new(),
        executor_exposure: 0,
        anchor: ctx(1, 100, 1).block,
        opened_daa: 100,
        expiry_daa: 200,
        next_index: 0,
        last_state_root: h64(24),
        accepted_work: 0,
        verified_work: 0,
        pending: 0,
        phase: PalwWorkRootPhaseV2::Open,
        prefix_claim: h64(901),
        prefix: PalwWorkPrefixStageV2::Verified { verified_daa: 101, route_reward: 0 },
    };
    s.exec_v2.roots.insert(b, root);
    assert_eq!(reward_readiness_v1(&s, &p, 129, b, false).unwrap(), RewardReadinessV1::Hold(u64::MAX));
}

#[test]
fn rfc23s2_reward_missing_certification_row_preserves_the_retry_clock() {
    let (p, s, [_, _, c]) = world(false);
    let (s, _) = step(&s, &p, 10, 148, &[], None, &extra());
    let x = extra();
    let mut builder = TransitionBuilder::new(&s, &p, false, false, false, false, &x);
    // Fault injection only: an absent row supplies neither payment authority nor
    // permission to discard a previously held claim.
    builder
        .write_kernel_row(PALW_SECURITY_CERTIFICATION_TABLE_V1, certification_claim_key_v1(&palw_certification_id_v1(&c)), None)
        .unwrap();
    let claim = builder.state.claim(&c).unwrap().clone();
    assert_eq!(
        super::super::palw_security_certification_fold_v1::try_release_claim_local_reward_v1(&mut builder, 1001, &c, &claim, true)
            .unwrap(),
        Some(1001)
    );
    assert_eq!(super::super::palw_claim_local_rewards_v1::hold_hint_v1(&builder.state, c, &claim), Some(1001));
    assert!(builder.state.claim(&c).is_some());
    assert!(!builder.state.pending_payouts.contains_key(&palw_certification_payout_key_v1(&c)));
}
#[test]
fn rfc23s2_claim_local_settlement_read_settles_independent_b_while_a_is_unresolved() {
    use super::super::{PalwClaimLocalSettlementV1 as S, palw_claim_local_settlement_v1 as read};
    // A (root 600) never bound: unresolved. B (root 601) independent. C shares A's root.
    let (p, s, [a, b, c]) = world(false);
    assert!(!matches!(s.claim(&a).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "A stays unresolved");
    // Inside B's own liability window: held by B's window, not by A.
    assert!(matches!(read(&s, &p, b, 129).unwrap(), S::Held { .. }));
    assert!(matches!(read(&s, &p, c, 129).unwrap(), S::Held { .. }));
    // Past B's own window B settles although A is still unresolved; C still waits on A.
    assert_eq!(read(&s, &p, b, 1001).unwrap(), S::Settled, "an unrelated unresolved claim never blocks B");
    assert!(matches!(read(&s, &p, c, 1001).unwrap(), S::Held { .. }), "C actually depends on A");
    assert!(!matches!(read(&s, &p, a, 1001).unwrap(), S::Settled));
    // B's answer is the same whether or not A ever resolves.
    let (p2, s2, [_, b2, _]) = world(true);
    assert_eq!(read(&s2, &p2, b2, 1001).unwrap(), S::Settled);
    // Unknown id: no answer, never a guess.
    assert_eq!(read(&s, &p, h64(0xDEAD), 1001).unwrap(), S::NotRetained);
    // Below / without the stage-2 fence: Dormant (callers keep palw_settlement_v1 unchanged).
    assert_eq!(read(&s, &p, b, 99).unwrap(), S::Dormant);
    let off = p.clone().with_claim_local_settlement_from_daa(None);
    assert_eq!(read(&s, &off, b, 1001).unwrap(), S::Dormant);
    // The read is pure: no state change.
    let root = s.state_root();
    let _ = read(&s, &p, c, 1001).unwrap();
    assert_eq!(s.state_root(), root);
}
