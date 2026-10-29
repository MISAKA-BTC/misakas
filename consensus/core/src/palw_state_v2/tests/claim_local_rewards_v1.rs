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
    world_with(all, first, p)
}
fn world_with(all: bool, first: u64, p: PalwStateParamsV2) -> (PalwStateParamsV2, PalwChainStateV2, [Hash64; 3]) {
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
        if p.claim_local_settlement_active_at(first) {
            assert_eq!(row(&s, *id).held_reward, 620);
        }
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

// ---- RFC-0023 stage 2: the fold-derived checkpoint and its consumers (lane rfc23-stage2) ----------------------------------

fn checkpoint_bytes(s: &PalwChainStateV2) -> Option<Vec<u8>> {
    use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
    let key = super::super::palw_claim_local_checkpoint_key_v1();
    s.kernel_route.as_ref()?.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())).cloned()
}

#[test]
fn rfc23s2_checkpoint_row_is_written_each_block_and_binds_the_unresolved_set() {
    use crate::palw_claim_local_v1::*;
    let (p, s, [a, b, _c]) = world(false);
    let raw = checkpoint_bytes(&s).expect("past the fence every block writes the checkpoint");
    let cp = palw_decode_claim_checkpoint_v1(&raw).unwrap();
    assert_eq!(cp.point, s.last_point().copied().unwrap(), "the checkpoint is the state's own point");
    let ra = &cp.records[&PalwClaimLocalWorkRefV1::ChainClaim { claim: a }];
    assert!(!ra.final_claim && !ra.liability_closed, "A is open and stays bound, not disguised as resolved");
    assert!(!ra.material.is_empty() && ra.material.iter().all(|m| cp.material.contains_key(m)), "A's material travels in the row");
    assert!(!cp.liability_state.is_empty(), "the open set's claim/court state is bound");
    let rb = &cp.records[&PalwClaimLocalWorkRefV1::ChainClaim { claim: b }];
    assert!(rb.final_claim && rb.certified, "B is Final and SecurityCertified");
    // The reader validates the stored row against a recomputation, never on its own word.
    let status = super::super::palw_claim_local_checkpoint_status_v1(&s, &p, b).unwrap().unwrap();
    assert!(status.credit_eligible, "B's closure does not contain A");
    let status_a = super::super::palw_claim_local_checkpoint_status_v1(&s, &p, a).unwrap().unwrap();
    assert!(!status_a.credit_eligible && !status_a.irreversible);
}

#[test]
fn rfc23s2_checkpoint_frontier_passes_unresolved_a_when_independent_b_settles() {
    let (p, s, [a, b, c]) = world(false);
    let (a_blue, b_blue, b_daa) = {
        let (ca, cb) = (s.claim(&a).unwrap(), s.claim(&b).unwrap());
        (ca.accepted_blue_score, cb.accepted_blue_score, cb.accepted_daa)
    };
    assert!(a_blue < b_blue, "A is older than B");
    assert!(s.safe_frontier().0 < a_blue, "before B's liability ends nothing stands past A");
    // B retires (its own liability ends by the chain's retirement rule) while A is still unresolved.
    let (s2, _) = step(&s, &p, 10, 148, &[], None, &extra());
    assert!(s2.claim(&b).is_none() && s2.claim(&a).is_some() && s2.claim(&c).is_some());
    assert_eq!(s2.safe_frontier().0, b_blue, "past the fence B's irreversible credit moves the frontier past unresolved A");
    // Settlement reads that one frontier: an effect at B's DAA settles without waiting for A.
    let read = crate::palw_settlement_v1::palw_settlement_v1(&s2, &p, Some(b_daa), b_daa).unwrap();
    assert!(read.settled, "B's effects settle at B's own time");
    // C shares A's execution root: it is not irreversible and does not move the frontier.
    let st_c = super::super::palw_claim_local_checkpoint_status_v1(&s2, &p, c).unwrap().unwrap();
    assert!(!st_c.irreversible);
    // Fork choice's first key is the same frontier.
    assert_eq!(s2.candidate_order(ctx(10, 148, 10).block).safe_frontier_blue_score, b_blue);
}

#[test]
fn rfc23s2_checkpoint_below_the_fence_keeps_the_shared_frontier() {
    // Stage 1 supplies the resolved-prefix retirement floor. With both stage
    // fences off, historical F05 lets retirement pass A; stage 2 must preserve
    // that legacy history rather than retroactively repairing it here.
    let p = rules().with_unresolved_bound_from_daa(Some(100)).with_claim_local_settlement_from_daa(None);
    let (p, s, [a, b, _c]) = world_with(false, 100, p);
    assert!(checkpoint_bytes(&s).is_none(), "no row below the fence");
    let a_blue = s.claim(&a).unwrap().accepted_blue_score;
    let (s2, _) = step(&s, &p, 10, 148, &[], None, &extra());
    assert!(checkpoint_bytes(&s2).is_none());
    if s2.claim(&a).is_some() && !matches!(s2.claim(&a).unwrap().phase, PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. }) {
        assert!(s2.safe_frontier().0 < a_blue, "below the fence an unresolved A still holds the shared frontier");
    }
    let _ = b;
    // The fence armed later than this history: byte-identical to the unarmed rules.
    let later = rules().with_unresolved_bound_from_daa(Some(100)).with_claim_local_settlement_from_daa(Some(10_000));
    let (_, s_late, _) = world_with(false, 100, later.clone());
    assert_eq!(s_late.state_root(), s.state_root(), "a not-yet-reached fence changes nothing");
}

#[test]
fn rfc23s2_checkpoint_reorg_reverts_the_row_and_the_frontier() {
    let (p, s, _) = world(false);
    let before = (checkpoint_bytes(&s), s.safe_frontier(), s.state_root());
    let (s2, delta) = step(&s, &p, 10, 148, &[], None, &extra());
    assert_ne!(checkpoint_bytes(&s2), before.0, "the row moved with the block");
    let back = revert_delta_v2(&s2, &delta, &p).unwrap();
    assert_eq!((checkpoint_bytes(&back), back.safe_frontier(), back.state_root()), before, "a reorg restores row and frontier exactly");
    let again = apply_delta_v2(&back, &delta, &p).unwrap();
    assert_eq!(again.state_root(), s2.state_root());
}

#[test]
fn rfc23s2_checkpoint_pruned_import_reproduces_and_refuses_a_forged_row() {
    use crate::palw_claim_local_v1::*;
    let (p, s, [a, _, _]) = world(false);
    let (s, _) = step(&s, &p, 10, 148, &[], None, &extra());
    // The pruning-point carriage carries the row; the import rebuilds and checks it against the recomputation.
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s)).unwrap();
    let restored =
        borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state_v3(&p, Some(s.state_root()), false, None).unwrap();
    assert_eq!(restored.state_root(), s.state_root());
    assert_eq!(checkpoint_bytes(&restored), checkpoint_bytes(&s));
    assert_eq!(restored.safe_frontier(), s.safe_frontier());
    restored.assert_internal_consistency(&p).unwrap();
    // The next block folds identically on the imported state.
    let (n1, _) = step(&s, &p, 11, 149, &[], None, &extra());
    let (n2, _) = step(&restored, &p, 11, 149, &[], None, &extra());
    assert_eq!(n1.state_root(), n2.state_root());
    // A row that disguises unresolved A as closed is refused by the recomputation check.
    let mut forged = s.clone();
    let key = (PALW_CLAIM_LOCAL_TABLE_V1, super::super::palw_claim_local_checkpoint_key_v1().to_vec());
    let mut cp = (*palw_decode_claim_checkpoint_v1(&forged.kernel_route.as_ref().unwrap().aux[&key]).unwrap()).clone();
    let ra = cp.records.get_mut(&PalwClaimLocalWorkRefV1::ChainClaim { claim: a }).unwrap();
    ra.voided = true;
    ra.liability_closed = true;
    forged.kernel_route.as_mut().unwrap().aux.insert(key.clone(), borsh::to_vec(&cp).unwrap());
    assert!(forged.assert_internal_consistency(&p).is_err(), "a stored row is never its own authority");
    // And a missing row past the fence is refused, not read as "nothing open".
    let mut missing = s.clone();
    missing.kernel_route.as_mut().unwrap().aux.remove(&key);
    assert!(missing.assert_internal_consistency(&p).is_err());
}

#[test]
fn rfc23s2_checkpoint_resource_owner_child() {
    if std::env::var("MISAKA_CHECKPOINT_RESOURCE_OWNER").ok().as_deref() != Some("1") {
        return;
    }
    use crate::palw_native_resource_v1::{
        NativeHeapV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
    };
    let (p, s, _) = world(false);
    let point = *s.last_point().unwrap();
    let canonical = checkpoint_bytes(&s).unwrap();
    let mut malformed = s.clone();
    let mut frame_key = vec![0; 65];
    frame_key[0] = b'F';
    malformed.kernel_route.as_mut().unwrap().aux.insert((crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, frame_key), vec![0]);
    // The new historical reader holds authenticated birth frames while it
    // inspects their prior/current carriages. Size the fixture's available room
    // from those exact typed preflights, not their smaller encoded byte lengths.
    // Four complete sets bound the nested target/source/retirement readers;
    // the existing 32 MiB remains construction/small-row workspace. No network
    // or production capacity is changed by this test-only local quota.
    let mut historical = 0u64;
    for (_, bytes) in super::super::palw_claim_local_rewards_v1::frames(&s) {
        let shape = crate::palw_native_resource_v1::preflight_native_value_v1::<crate::palw_claim_local_v1::PalwClaimLiabilityFrameV1>(
            bytes.as_slice(), bytes.len() as u64,
        ).unwrap();
        historical = historical.checked_add(shape.logical_requests_v1()).unwrap();
        let frame = crate::palw_claim_local_v1::decode_claim_local_value_v1::<crate::palw_claim_local_v1::PalwClaimLiabilityFrameV1>(bytes).unwrap();
        for carriage in [&frame.prior_state, &frame.current_state] {
            let shape = crate::palw_native_resource_v1::preflight_native_carriage_v1(carriage.as_slice(), carriage.len() as u64).unwrap();
            historical = historical.checked_add(shape.memory_requests_v1().unwrap()).unwrap();
        }
    }
    let memory = (32u64 << 20).checked_add(historical.checked_mul(4).unwrap()).unwrap();
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(memory).unwrap();
    let build = || super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&s, &p, &point);
    assert!(matches!(build(), Err(PalwStateV2Error::NativeResource(_))));
    assert!(matches!(
        super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&malformed, &p, &point),
        Err(PalwStateV2Error::NativeResource(_))
    ), "capacity refusal precedes decoding even a malformed retained frame");
    drop(blocker);
    assert!(matches!(
        super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&malformed, &p, &point),
        Err(PalwStateV2Error::CarriageInconsistent(_))
    ), "with capacity available the original malformed-frame error remains distinct");
    let owner = build().unwrap().unwrap();
    assert_eq!(borsh::to_vec(&owner).unwrap(), canonical, "owner retains the exact checkpoint encoding");
    assert!(reserve_native_requests_v1(memory).is_err(), "the returned checkpoint keeps its demand live");
    let (value, lease) = owner.into_parts_v1();
    let lease = lease.expect("native logical admission is armed");
    let heap = value.native_heap_v1("checkpoint result");
    assert!(lease.memory_v1() >= heap.bytes().unwrap(), "construction quote also bounds the retained result");
    drop(value);
    drop(lease);
    assert!(reserve_native_requests_v1(memory).is_ok(), "dropping the owner returns all demand");
}

#[test]
fn rfc23s2_checkpoint_admits_before_build_and_retains_until_drop() {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "palw_state_v2::tests::claim_local_rewards_v1::rfc23s2_checkpoint_resource_owner_child",
            "--nocapture",
        ])
        .env("MISAKA_CHECKPOINT_RESOURCE_OWNER", "1")
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "checkpoint capacity child failed: {} {}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr),
    );
    assert!(String::from_utf8_lossy(&child.stdout).contains("1 passed"), "the child must actually run its resource test");
}


#[test]
fn rfc23s2_checkpoint_missing_birth_material_never_closes_an_independent_claim() {
    use crate::palw_claim_local_v1::*;
    let (p, mut s, [_, b, _]) = world(false);
    let point = ctx(10, 1001, 10);
    let work = PalwClaimLocalWorkRefV1::ChainClaim { claim: b };
    let before = super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&s, &p, &point)
        .unwrap().unwrap();
    assert!(before.records[&work].liability_closed);
    drop(before);
    // A checkpoint reader cannot infer admission facts from the remaining live row.
    let key = palw_claim_liability_frame_key_v1(ctx(3, 101, 3).block);
    assert!(s.kernel_route.as_mut().unwrap().aux.remove(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())).is_some());
    let after = super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&s, &p, &point)
        .unwrap().unwrap();
    assert!(!after.records[&work].liability_closed, "missing birth material holds credit; it never supplies an irreversible anchor");
}

#[test]
fn rfc23s2_spending_a_settled_carrier_output_depends_on_that_claim() {
    use crate::palw_claim_local_v1::*;
    use crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1;
    let (p, mut s, [a, b, _]) = world(false);
    let carrier = h64(0xCA11);
    s.kernel_route.as_mut().unwrap().aux.insert(
        (PALW_FP_WINNING_CARRIER_TABLE_V1, a.as_byte_slice().to_vec()),
        borsh::to_vec(&carrier).unwrap(),
    );
    let mut extras = crate::palw_state_v2::PalwTransitionExtrasV1::default();
    extras.carrier_input_spends.insert(b, vec![carrier, h64(0x0)]);
    extras.carrier_input_spends.insert(a, vec![carrier]);
    let edges = super::super::palw_claim_local_fold_v1::consumed_carrier_edges_v1(
        &s,
        super::super::palw_claim_local_fold_v1::carrier_spend_pairs_v1(&extras),
    );
    assert_eq!(edges, [(b, a)].into());
    let born = s.claims.get(&b).unwrap().clone();
    let key = palw_claim_liability_frame_key_v1(born.accepted_block);
    let raw = s.kernel_route.as_ref().unwrap().aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())).unwrap().clone();
    let (mut frame, lease) = palw_decode_claim_liability_frame_v1(&raw).unwrap().into_parts_v1();
    frame.inputs.push(PalwClaimLocalInputV1::ConsumedCarrier { consumer: b, source: a });
    s.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec()), borsh::to_vec(&frame).unwrap());
    drop(lease);
    let origin = super::super::palw_claim_local_rewards_v1::origin_for(&s, &p, b, &born).unwrap().unwrap();
    let deps = super::super::palw_claim_local_rewards_v1::dependency_inputs_v1(&s, b, &born, Some(&origin)).unwrap();
    assert!(deps.contains(&PalwClaimLocalWorkRefV1::ChainClaim { claim: a }));
    let readiness = super::super::palw_claim_local_rewards_v1::reward_readiness_v1;
    let before = readiness(&s, &p, 200, b, false).unwrap();
    assert_ne!(before, super::super::palw_claim_local_rewards_v1::RewardReadinessV1::Denied);
    let original = s.claims.get(&a).unwrap().phase.clone();
    s.claims.get_mut(&a).unwrap().phase = PalwClaimPhaseV2::Voided { voided_daa: 200, reason: PalwVoidReasonV2::CourtFraud };
    assert_eq!(
        readiness(&s, &p, 200, b, false).unwrap(),
        super::super::palw_claim_local_rewards_v1::RewardReadinessV1::Denied,
        "voiding the spent carrier's claim cancels the consumer reward"
    );
    s.claims.get_mut(&a).unwrap().phase = original;
    assert_eq!(readiness(&s, &p, 200, b, false).unwrap(), before, "restoring the source claim restores the consumer reward");
}
