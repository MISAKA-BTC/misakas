//! Actual fold/reward consumer tests. Draw/attestation rows use the existing test
//! seam; certification verdict, retirement, payout and replay are production code.
//! This is not the remaining node/pruning G14 acceptance drill.
#![allow(warnings)]
use super::*;
#[path = "rfc37_native_row_roundtrip_v1.rs"]
mod rfc37_native_row_roundtrip_v1;
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
    world_with_roots(all, first, p, [600, 601, 600])
}
fn world_with_roots<const N: usize>(
    all: bool,
    first: u64,
    p: PalwStateParamsV2,
    roots: [u64; N],
) -> (PalwStateParamsV2, PalwChainStateV2, [Hash64; N]) {
    let mut objects = register_class_and_bond();
    objects.push(seat_bond_reg(2, 1000));
    let (mut s, _) = step(&PalwChainStateV2::genesis(), &p, 1, first - 1, &objects, None, &extra());
    let mut ids = [ZERO_HASH64; N];
    for i in 0..N {
        let mut e = attempt(40, i as u64 + 1);
        e.attempt.execution_root = h64(roots[i]);
        e.attempt.trace_retention_daa = 1000;
        ids[i] = attempt_id_v2(&e.attempt);
        s = step(&s, &p, i as u64 + 2, first + i as u64, &[], Some(&e), &extra()).0;
        assert!(s.claim(&ids[i]).is_some());
    }
    let chosen: Vec<_> = ids.iter().enumerate().filter(|(i, _)| all || *i > 0).map(|(_, id)| *id).collect();
    s = step(&s, &p, N as u64 + 2, 103, &chosen.iter().copied().map(bind).collect::<Vec<_>>(), None, &extra()).0;
    s = step(&s, &p, N as u64 + 3, 104, &chosen.iter().copied().map(license).collect::<Vec<_>>(), None, &extra()).0;
    s = step(&s, &p, N as u64 + 4, 125, &[], None, &extra()).0;
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
    s = step(&s, &p, N as u64 + 5, 126, &[], None, &x).0;
    s = step(&s, &p, N as u64 + 6, 128, &[], None, &extra()).0;
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
    super::super::palw_claim_local_checkpoint_snapshot_v2(s).unwrap().map(|cp| borsh::to_vec(&*cp).unwrap())
}

#[test]
fn rfc23s2_reference_descriptor_heartbeat_does_not_copy_immutable_material() {
    use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
    let (p, state, _) = world(false);
    let before = super::super::palw_claim_local_checkpoint_snapshot_v2(&state).unwrap().unwrap();
    let (next, delta) = step(&state, &p, 99, 129, &[], None, &extra());
    let after = super::super::palw_claim_local_checkpoint_snapshot_v2(&next).unwrap().unwrap();
    assert_eq!(before.material, after.material, "heartbeat has no new provenance/material event");
    assert_ne!(before.point, after.point);
    let descriptor = super::super::palw_claim_descriptor_key_v2();
    assert!(delta.entries.iter().any(|entry|matches!(entry,PalwDeltaEntryV2::KernelRouteRow{table,key,..}if *table==PALW_CLAIM_LOCAL_TABLE_V1&&key.as_slice()==descriptor)));
    assert!(!delta.entries.iter().any(|entry|matches!(entry,PalwDeltaEntryV2::KernelRouteRow{table,key,..}if *table==PALW_CLAIM_LOCAL_TABLE_V1&&key.first().is_some_and(|p|matches!(*p,b'M'|b'Q')))),
        "point advancement must not journal old/new copies of unchanged material");
}

#[test]
fn rfc23s2_reference_descriptor_missing_same_snapshot_blob_is_not_material() {
    let (_p, mut state, _) = world(false);
    let key = (crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, super::super::palw_claim_descriptor_key_v2().to_vec());
    let descriptor: super::super::PalwClaimDescriptorV2 = borsh::from_slice(&state.kernel_route.as_ref().unwrap().aux[&key]).unwrap();
    let id = descriptor.liability.expect("actual unresolved liability blob");
    let mut blob_key = vec![super::super::PALW_CLAIM_BLOB_PREFIX_V2];
    blob_key.extend_from_slice(id.as_byte_slice());
    state.kernel_route.as_mut().unwrap().aux.remove(&(key.0, blob_key));
    assert!(
        matches!(super::super::palw_claim_local_checkpoint_snapshot_v2(&state), Err(PalwStateV2Error::CarriageInconsistent(_))),
        "a valid descriptor/root is not availability of its missing literal data"
    );
}

#[test]
fn rfc23s2_reference_descriptor_retains_legacy_flat_v1_read_compatibility() {
    use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
    let (p, mut state, _) = world(false);
    let logical = super::super::palw_claim_local_checkpoint_snapshot_v2(&state).unwrap().unwrap();
    state.kernel_route.as_mut().unwrap().aux.retain(|(t, k), _| {
        *t != PALW_CLAIM_LOCAL_TABLE_V1 || !k.first().is_some_and(|p| super::super::palw_claim_local_descriptor_v2::is_prefix(*p))
    });
    state.kernel_route.as_mut().unwrap().aux.insert(
        (PALW_CLAIM_LOCAL_TABLE_V1, super::super::palw_claim_local_checkpoint_key_v1().to_vec()),
        borsh::to_vec(&*logical).unwrap(),
    );
    let recovered = super::super::palw_claim_local_checkpoint_snapshot_v2(&state).unwrap().unwrap();
    assert_eq!(*recovered, *logical, "V1 C bytes retain their original meaning");
    state.assert_internal_consistency(&p).unwrap();
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
    if s2.claim(&a).is_some()
        && !matches!(s2.claim(&a).unwrap().phase, PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. })
    {
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
    assert_eq!(
        (checkpoint_bytes(&back), back.safe_frontier(), back.state_root()),
        before,
        "a reorg restores row and frontier exactly"
    );
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
    let key = (PALW_CLAIM_LOCAL_TABLE_V1, super::super::palw_claim_descriptor_key_v2().to_vec());
    let mut cp = (*super::super::palw_claim_local_checkpoint_snapshot_v2(&forged).unwrap().unwrap()).clone();
    let work = PalwClaimLocalWorkRefV1::ChainClaim { claim: a };
    let ra = cp.records.get_mut(&work).unwrap();
    ra.voided = true;
    ra.liability_closed = true;
    let mut record_key = vec![super::super::PALW_CLAIM_RECORD_PREFIX_V2];
    record_key.extend(borsh::to_vec(&work).unwrap());
    forged.kernel_route.as_mut().unwrap().aux.insert(
        (PALW_CLAIM_LOCAL_TABLE_V1, record_key),
        borsh::to_vec(&super::super::PalwClaimRecordRowV2 { version: 2, record: ra.clone() }).unwrap(),
    );
    // Even a self-consistent physical descriptor is not computation/closure authority.
    let mut descriptor: super::super::PalwClaimDescriptorV2 =
        borsh::from_slice(&forged.kernel_route.as_ref().unwrap().aux[&key]).unwrap();
    (descriptor.records_root, descriptor.records_count) =
        super::super::palw_claim_local_descriptor_v2::root(&forged, super::super::PALW_CLAIM_RECORD_PREFIX_V2);
    forged.kernel_route.as_mut().unwrap().aux.insert(key.clone(), borsh::to_vec(&descriptor).unwrap());
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
        let shape =
            crate::palw_native_resource_v1::preflight_native_value_v1::<crate::palw_claim_local_v1::PalwClaimLiabilityFrameV1>(
                bytes.as_slice(),
                bytes.len() as u64,
            )
            .unwrap();
        historical = historical.checked_add(shape.logical_requests_v1()).unwrap();
        let frame =
            crate::palw_claim_local_v1::decode_claim_local_value_v1::<crate::palw_claim_local_v1::PalwClaimLiabilityFrameV1>(bytes)
                .unwrap();
        for carriage in [&frame.prior_state, &frame.current_state] {
            let shape =
                crate::palw_native_resource_v1::preflight_native_carriage_v1(carriage.as_slice(), carriage.len() as u64).unwrap();
            historical = historical.checked_add(shape.memory_requests_v1().unwrap()).unwrap();
        }
    }
    let memory = (32u64 << 20).checked_add(historical.checked_mul(4).unwrap()).unwrap();
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(memory).unwrap();
    let build = || super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&s, &p, &point);
    assert!(matches!(build(), Err(PalwStateV2Error::NativeResource(_))));
    assert!(
        matches!(
            super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&malformed, &p, &point),
            Err(PalwStateV2Error::NativeResource(_))
        ),
        "capacity refusal precedes decoding even a malformed retained frame"
    );
    drop(blocker);
    assert!(
        matches!(
            super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&malformed, &p, &point),
            Err(PalwStateV2Error::CarriageInconsistent(_))
        ),
        "with capacity available the original malformed-frame error remains distinct"
    );
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
        .args(["--exact", "palw_state_v2::tests::claim_local_rewards_v1::rfc23s2_checkpoint_resource_owner_child", "--nocapture"])
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
    let before =
        super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&s, &p, &point).unwrap().unwrap();
    assert!(before.records[&work].liability_closed);
    drop(before);
    // A checkpoint reader cannot infer admission facts from the remaining live row.
    let key = palw_claim_liability_frame_key_v1(ctx(3, 101, 3).block);
    assert!(s.kernel_route.as_mut().unwrap().aux.remove(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())).is_some());
    let after =
        super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&s, &p, &point).unwrap().unwrap();
    assert!(!after.records[&work].liability_closed, "missing birth material holds credit; it never supplies an irreversible anchor");
}

#[test]
fn rfc23s2_attempt_cannot_be_fabricated_as_a_native_fp_carrier_source() {
    use crate::palw_claim_local_v1::*;
    use crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1;
    let (p, mut s, [a, b, _]) = world(false);
    let carrier = h64(0xCA11);
    s.kernel_route
        .as_mut()
        .unwrap()
        .aux
        .insert((PALW_FP_WINNING_CARRIER_TABLE_V1, a.as_byte_slice().to_vec()), borsh::to_vec(&carrier).unwrap());
    let mut extras = crate::palw_state_v2::PalwTransitionExtrasV1::default();
    extras.carrier_input_spends.insert(b, vec![carrier, h64(0x0)]);
    extras.carrier_input_spends.insert(a, vec![carrier]);
    let edges = super::super::palw_claim_local_fold_v1::consumed_carrier_edges_v1(
        &s,
        super::super::palw_claim_local_fold_v1::carrier_spend_pairs_v1(&extras),
    );
    assert_eq!(edges, [(b, a)].into());
    // Projection equivalence only: these hints still cannot authenticate the
    // Attempt as a genuine FP source (the existing negative assertions below).
    super::super::palw_claim_local_carrier_lineage_v9::rebuild_legacy_index_v9(&mut s).unwrap();
    assert_eq!(
        super::super::palw_claim_local_carrier_lineage_v9::current_winner_claim_targeted_v17(&s, &s, carrier).unwrap(),
        Some(a)
    );
    let mut unknown = s.clone();
    unknown.claim_local_legacy_carriers_ready_v9 = false;
    assert!(
        matches!(
            super::super::palw_claim_local_carrier_lineage_v9::current_winner_claim_targeted_v17(&unknown, &unknown, carrier),
            Err(PalwStateV2Error::NativeResource(_))
        ),
        "unknown reverse coverage cannot become known-empty"
    );
    let mut ambiguous = s.clone();
    ambiguous
        .kernel_route
        .as_mut()
        .unwrap()
        .aux
        .insert((PALW_FP_WINNING_CARRIER_TABLE_V1, b.as_byte_slice().to_vec()), borsh::to_vec(&carrier).unwrap());
    super::super::palw_claim_local_carrier_lineage_v9::rebuild_legacy_index_v9(&mut ambiguous).unwrap();
    assert_eq!(
        super::super::palw_claim_local_carrier_lineage_v9::current_winner_claim_targeted_v17(&ambiguous, &ambiguous, carrier).unwrap(),
        None,
        "same ambiguity semantics as the old table projection"
    );
    let mut retired = s.clone();
    retired.claims.remove(&a);
    assert_eq!(
        super::super::palw_claim_local_fold_v1::consumed_carrier_edges_v1(&retired, [(b, carrier)],),
        [(b, a)].into(),
        "retirement must not erase a committed carrier dependency at new admission"
    );
    let born = s.claims.get(&b).unwrap().clone();
    let key = palw_claim_liability_frame_key_v1(born.accepted_block);
    let raw = s.kernel_route.as_ref().unwrap().aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())).unwrap().clone();
    let (mut frame, lease) = palw_decode_claim_liability_frame_v1(&raw).unwrap().into_parts_v1();
    frame.inputs.push(PalwClaimLocalInputV1::ConsumedCarrier { consumer: b, source: a });
    s.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec()), borsh::to_vec(&frame).unwrap());
    drop(lease);
    let origin = super::super::palw_claim_local_rewards_v1::origin_for(&s, &p, b, &born).unwrap().unwrap();
    let deps = super::super::palw_claim_local_rewards_v1::dependency_inputs_v1(&s, &p, b, &born, Some(&origin)).unwrap();
    assert!(
        !deps.contains(&PalwClaimLocalWorkRefV1::ChainClaim { claim: a }),
        "an injected raw tag8/167 cannot enlist an Attempt as an actual FP carrier source"
    );
    let readiness = super::super::palw_claim_local_rewards_v1::reward_readiness_v1;
    let before = readiness(&s, &p, 200, b, false).unwrap();
    assert_ne!(before, super::super::palw_claim_local_rewards_v1::RewardReadinessV1::Denied);
    let original = s.claims.get(&a).unwrap().phase.clone();
    s.claims.get_mut(&a).unwrap().phase = PalwClaimPhaseV2::Voided { voided_daa: 200, reason: PalwVoidReasonV2::CourtFraud };
    assert_eq!(
        readiness(&s, &p, 200, b, false).unwrap(),
        before,
        "an invented Attempt carrier must not make an independent consumer depend on its verdict"
    );
    s.claims.get_mut(&a).unwrap().phase = original;
    assert_eq!(readiness(&s, &p, 200, b, false).unwrap(), before, "restoring the source claim restores the consumer reward");
}

#[test]
fn rfc23s2_checkpoint_retired_conviction_denies_consumer_credit_without_rewriting_approval() {
    use crate::palw_claim_local_v1::*;
    let (p, s, [a, b, c]) = world(false);
    let x = extra();
    let mut builder = TransitionBuilder::new(&s, &p, false, false, false, false, &x);
    let source = builder.state.claim(&a).unwrap().clone();
    // Existing conviction/retirement writer seams; admission of a public
    // refutation and the retirement clock are covered by their own tests.
    builder.void_and_slash_at(a, &source, 129, PalwVoidReasonV2::CourtFraud, None).unwrap();
    let terminal = builder.state.claim(&a).unwrap().clone();
    builder.retire_claim(a, &terminal).unwrap();
    let point = ctx(10, 129, 10);
    super::super::palw_claim_local_fold_v1::retain_changed_claim_context_v1(
        &mut builder,
        &s,
        &point,
        &[],
        PalwBlockWorkV3::None,
        &[],
        ZERO_HASH64,
    )
    .unwrap();
    builder.state.set_last_point_for_tests(Some(point));
    assert!(!builder.state.claims.contains_key(&a));
    assert_eq!(
        super::super::palw_claim_local_rewards_v1::reward_readiness_v1(&builder.state, &p, 129, c, false).unwrap(),
        super::super::palw_claim_local_rewards_v1::RewardReadinessV1::Denied,
    );
    let checkpoint = super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&builder.state, &p, &point)
        .unwrap()
        .unwrap();
    let validated = checkpoint.validate(&point, &checkpoint.records, &checkpoint.liability_state).unwrap();
    let consumer = PalwClaimLocalWorkRefV1::ChainClaim { claim: c };
    assert!(!validated.credit_eligible(consumer), "retiring the convicted source must not restore its consumer's credit");
    assert!(!validated.irreversible_eligible(consumer));
    assert!(validated.credit_eligible(PalwClaimLocalWorkRefV1::ChainClaim { claim: b }), "independent B retains credit");
    assert!(row(&builder.state, c).outcome.unwrap().0.certifies(), "raw selected-verifier approval is unchanged");
}

#[test]
fn rfc23s2_carrier_dependency_closure_survives_source_retirement_and_keeps_independent_credit() {
    use super::super::palw_claim_local_rewards_v1::{RewardReadinessV1, reward_readiness_v1};
    use crate::palw_claim_local_v1::*;
    let (p, mut s, [a, b, c, d]) = world_with_roots(false, 100, rules(), [600, 601, 602, 603]);
    // Retain two actual carrier-spend edges at their consumer birth points. The
    // carrier-index extraction is covered separately; these frames exercise the
    // historical reader, including its production carriage authentication.
    for (consumer, source) in [(b, a), (c, b)] {
        let born = s.claims.get(&consumer).unwrap();
        let key = palw_claim_liability_frame_key_v1(born.accepted_block);
        let raw = s.kernel_route.as_ref().unwrap().aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())).unwrap();
        let (mut frame, lease) = palw_decode_claim_liability_frame_v1(raw).unwrap().into_parts_v1();
        frame.inputs.push(PalwClaimLocalInputV1::ConsumedCarrier { consumer, source });
        s.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec()), borsh::to_vec(&frame).unwrap());
        drop(lease);
    }
    assert!(
        matches!(reward_readiness_v1(&s, &p, 1001, c, false).unwrap(), RewardReadinessV1::Hold(_)),
        "C must follow B's carrier dependency to unresolved A despite three distinct execution roots"
    );
    assert_eq!(reward_readiness_v1(&s, &p, 1001, d, false).unwrap(), RewardReadinessV1::Ready);
    let x = extra();
    let mut builder = TransitionBuilder::new(&s, &p, false, false, false, false, &x);
    let source = builder.state.claim(&a).unwrap().clone();
    builder.void_and_slash_at(a, &source, 129, PalwVoidReasonV2::CourtFraud, None).unwrap();
    let terminal = builder.state.claim(&a).unwrap().clone();
    builder.retire_claim(a, &terminal).unwrap();
    let point = ctx(11, 129, 11);
    super::super::palw_claim_local_fold_v1::retain_changed_claim_context_v1(
        &mut builder,
        &s,
        &point,
        &[],
        PalwBlockWorkV3::None,
        &[],
        ZERO_HASH64,
    )
    .unwrap();
    builder.state.set_last_point_for_tests(Some(point));
    let after_source = builder.state.bond(&bond_key(1)).unwrap().collateral;
    assert_eq!(
        reward_readiness_v1(&builder.state, &p, 1001, c, false).unwrap(),
        RewardReadinessV1::Denied,
        "the A→B→C chain still denies C after A's live vertex retires"
    );
    assert_eq!(reward_readiness_v1(&builder.state, &p, 1001, d, false).unwrap(), RewardReadinessV1::Ready);
    assert_eq!(builder.state.bond(&bond_key(1)).unwrap().collateral, after_source, "dependency reads never slash consumers");
    let checkpoint = super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&builder.state, &p, &point)
        .unwrap()
        .unwrap();
    let validated = checkpoint.validate(&point, &checkpoint.records, &checkpoint.liability_state).unwrap();
    assert!(!validated.credit_eligible(PalwClaimLocalWorkRefV1::ChainClaim { claim: c }));
    assert!(validated.credit_eligible(PalwClaimLocalWorkRefV1::ChainClaim { claim: d }));
}

#[test]
fn rfc37_nested_kernel_generation_exports_the_exact_committed_state() {
    let (_p, s, _) = world(false);
    let mut wire = Vec::new();
    let len = crate::palw_direct_row_export_v1::write_direct_row_generation_v1(&s, b"native-envelope", &mut wire).unwrap();
    assert_eq!(wire.len() as u64, len);
    let accepted =
        crate::palw_row_stream_v1::palw_accept_row_stream_generation_v1(s.state_root(), &wire, std::io::sink(), len, None).unwrap();
    assert_eq!(accepted.import_suffix(), b"native-envelope");
}

#[test]
fn rfc23s2_owned_native_lifecycle_preserves_open_a_and_blocks_dependent_c_credit() {
    use crate::palw_claim_local_v1::PalwClaimLocalWorkRefV1 as W;
    let (p, mut state, [a, b, c]) = world(false);
    let point = *state.last_point().unwrap();
    let root = state.state_root();
    let mut view = super::super::palw_claim_local_owned_checkpoint_v1(&state, &p, &point).unwrap().unwrap();
    assert!(
        view.lifecycle_preserved_through_v1(point.blue_score),
        "actual open A material permits shared data lifecycle preservation"
    );
    assert!(!view.credit_eligible_v1(W::ChainClaim { claim: a }));
    assert!(
        !view.credit_eligible_v1(W::ChainClaim { claim: c }),
        "C's own selected approval cannot bypass its actual shared execution dependency on A"
    );
    assert!(view.credit_eligible_v1(W::ChainClaim { claim: b }), "independent B retains reversible credit");
    assert_eq!(state.state_root(), root);
    drop(view);
    let birth = state.claim(&a).unwrap().clone();
    let key = crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(birth.accepted_block);
    state.kernel_route.as_mut().unwrap().aux.remove(&(crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())).unwrap();
    assert!(
        super::super::palw_claim_local_owned_checkpoint_v1(&state, &p, &point).is_err(),
        "claim metadata and a remaining commitment cannot mint an EVM lifecycle capability after actual admission material is lost"
    );
}

#[test]
fn rfc23s2_legacy_actual_journal_source_survives_retirement_and_closes_at_first_maturity_block() {
    // Focused core lifecycle regression. Header execution identities and the
    // existing named-validator draw seam are supplied at their public fold
    // boundaries. No accepted claim, work id, approval or source row is patched.
    // This is not the actual multi-node producer/admission campaign.
    use crate::config::params::ForkActivation;
    use crate::palw_native_settlement_v1::{PalwDnsRetirementV1, PalwSettlementPolicyV1};
    use crate::palw_native39_settlement_checkpoint_v1::Native39SettlementFoldV1;
    let mut p = rules();
    p.base_class_id = h64(900);
    let ruleset = kaspa_hashes::Hash::from_bytes([29; 32]);
    let mut x = extra();
    x.audit_2026_09_23_active = true;
    x.canonical_work_daa = Some(0);
    x.native_fact_ruleset = Some(ruleset);
    x.native_fact_quantum_maturity_daa = Some(7);
    x.native39_settlement_checkpoint = Some(Native39SettlementFoldV1 {
        retirement: PalwDnsRetirementV1 {
            activation: ForkActivation::new(0),
            settlement: PalwSettlementPolicyV1 {
                settled_anchor_depth: 2,
                unique_mature_work: u128::MAX,
                max_operator_permille: 1000,
                max_class_permille: 1000,
            },
            legacy_evidence_horizon_daa: 1,
        },
        ruleset,
        evm_activation_daa: u64::MAX,
        fork_choice_commitment: None,
    });
    let run = |s: &PalwChainStateV2,
               n: u64,
               daa: u64,
               objects: &[PalwConsensusObjectV2],
               env: Option<&PalwAttemptEnvelopeV2>,
               extras: &PalwTransitionExtrasV1| {
        let point = PalwBlockContextV2 { subsidy: 1000, ..ctx(n, daa, n) };
        let work = env.map_or(PalwBlockWorkV3::None, PalwBlockWorkV3::Attempt);
        let (next, delta, skips) =
            apply_palw_transition_v7(s, &p, None, &point, objects, work, &[], h64(91000 + n), false, false, false, false, extras)
                .unwrap();
        assert!(skips.is_empty());
        assert_eq!(apply_delta_v2(s, &delta, &p).unwrap().state_root(), next.state_root());
        assert_eq!(revert_delta_v2(&next, &delta, &p).unwrap().state_root(), s.state_root());
        next
    };
    let mut registrations = register_class_and_bond();
    let mut floor = registrations[0].clone();
    if let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } = &mut floor {
        *class_id = p.base_class_id;
        *artifact_root = h64(901)
    } else {
        unreachable!()
    }
    if let PalwConsensusObjectV2::ClassRegistered { share_permille, .. } = &mut registrations[0] {
        *share_permille = 500
    } else {
        unreachable!()
    }
    registrations.insert(0, floor); // Genuine BASE first; later500 grant proportionally funds the nonBASE class.
    registrations.push(seat_bond_reg(2, 1000));
    let mut state = run(&PalwChainStateV2::genesis(), 1, 99, &registrations, None, &x);
    let mut a = attempt(40, 1);
    a.attempt.execution_root = h64(800);
    a.attempt.trace_retention_daa = 1000;
    let id = attempt_id_v2(&a.attempt);
    state = run(&state, 2, 100, &[], Some(&a), &x);
    assert_eq!(state.claim(&id).unwrap().work_id, Some(h64(91002)));
    state = run(&state, 3, 103, &[bind(id)], None, &x);
    state = run(&state, 4, 104, &[license(id)], None, &x);
    state = run(&state, 5, 125, &[], None, &x);
    let source = state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .iter()
        .find(|((t, _), _)| *t == LEGACY_PENDING_FACT_TABLE_V1)
        .expect("actual Final journal captured pending canonical source");
    let source = super::super::palw_legacy_pending_facts_v1::decode(&source.1).unwrap();
    assert_eq!(source.claim_id, id);
    assert_eq!(source.fact.work, 40);
    assert_eq!(source.material_until_daa, Some(1000));
    assert!(crate::palw_native_closed_facts_v1::read_native_closed_facts_v1(&state).unwrap().is_empty());
    let mut cert = row(&state, id);
    cert.anchor = Some((1, [7; 64]));
    cert.draws = vec![crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond_key(2))];
    cert.drawn_daa = 126;
    cert.deadline_daa = 127;
    cert.attestations = vec![crate::palw_model_eligibility_v1::PalwEligibilityAttestationRowV1 {
        verifier: cert.draws[0],
        verdict: crate::palw_model_eligibility_v1::PalwEligibilityVerdictV1::Reproduced,
        transcript: [9; 64],
        attested_daa: 126,
    }];
    let mut draw = x.clone();
    let mut cert_key = vec![CERTIFICATION_CLAIM_PREFIX_V1];
    cert_key.extend_from_slice(&palw_certification_id_v1(&id));
    draw.exec_v2_test_kernel_rows.push((PALW_SECURITY_CERTIFICATION_TABLE_V1, cert_key, Some(borsh::to_vec(&cert).unwrap())));
    state = run(&state, 6, 126, &[], None, &draw);
    state = run(&state, 7, 128, &[], None, &x);
    state = run(&state, 8, 148, &[], None, &x);
    assert!(state.claim(&id).is_none(), "actual original retires before trace maturity");
    let carriage = PalwStateCarriageV2::from_state(&state);
    let encoded = borsh::to_vec(&carriage).unwrap();
    crate::palw_native_resource_v1::preflight_native_carriage_v1(encoded.as_slice(), encoded.len() as u64)
        .expect("typed196 source cold-restore inventory");
    let mut malformed = carriage.clone();
    let route = malformed.kernel_route.as_mut().unwrap();
    let (key, body) =
        route.aux.iter().find(|((t, _), _)| *t == LEGACY_PENDING_FACT_TABLE_V1).map(|(k, v)| (k.clone(), v.clone())).unwrap();
    route.aux.remove(&key);
    route.aux.insert((key.0, key.1[..127].to_vec()), body);
    let malformed = borsh::to_vec(&malformed).unwrap();
    assert!(
        crate::palw_native_resource_v1::preflight_native_carriage_v1(malformed.as_slice(), malformed.len() as u64).is_err(),
        "196 key is exactly128 bytes"
    );
    state = carriage.into_state_v3(&p, Some(state.state_root()), false, Some(0)).unwrap();
    state = run(&state, 9, 999, &[], None, &x);
    assert!(crate::palw_native_closed_facts_v1::read_native_closed_facts_v1(&state).unwrap().is_empty());
    // Another actual acceptance makes this very block's frame newer than the
    // parent's LastPoint. Promotion must validate under the current fold point.
    let mut pending = attempt(40, 77);
    pending.attempt.execution_root = h64(801);
    pending.attempt.trace_retention_daa = 2000;
    state = run(&state, 10, 1000, &[], Some(&pending), &x);
    let closed = crate::palw_native_closed_facts_v1::read_native_closed_facts_v1(&state).unwrap();
    let (_, fact) = closed.iter().find(|(_, f)| f.origin.subject == id).expect("first actual maturity block promotes retired source");
    assert_eq!(fact.closed_at.daa_score, 1000);
    assert_eq!(fact.fact.work, 40);
    assert_eq!(fact.origin.ruleset, ruleset);
    assert_eq!(fact.origin.work, h64(91002));
    assert!(
        !state.kernel_route.as_ref().unwrap().aux.keys().any(|(t, _)| *t == LEGACY_PENDING_FACT_TABLE_V1),
        "closed195 decision releases heavy source"
    );
}

#[test]
fn rfc23s2_actual_carrier_after_old_source_frame_collection_uses_compact_standing() {
    // Core ingress/lifecycle regression with the existing graph-pricing and
    // named-draw fixture seams, not a multi-node producer/admission claim.
    use super::super::palw_claim_local_compact_sources_v1 as compact;
    use crate::palw_claim_local_v1::*;
    use crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1;
    let (base_params, base, class, _) = derived_work_chain();
    let p = base_params
        .with_claim_retirement_daa(1)
        .unwrap()
        .with_claim_local_settlement_from_daa(Some(110))
        .with_rfc0018_v1(rules().rfc0018_v1().cloned());
    let mut x = extra();
    x.fp_derived_work_daa = Some(0);
    x.native_fact_ruleset = Some(kaspa_hashes::Hash::from_bytes([39; 32]));
    let mut state = step(&base, &p, 3, 102, &[lane_certification(class)], None, &x).0;
    let id = h64(0xFC);
    let carrier = h64(0xCAFC);
    let prompt: Vec<u32> = (0..8).collect();
    let work = crate::palw_step::step_leaf_count_of_tokens_capped_v1(&derived_profile(), 8, 2, 1 << 26).unwrap();
    let mut object = derived_commit(0xFC, &prompt, 2, work);
    if let PalwConsensusObjectV2::FreePromptCommitted { trace_retention_daa, .. } = &mut object {
        *trace_retention_daa = 1000
    } else {
        unreachable!()
    }
    let mut ingress = x.clone();
    ingress.fp_answer_accounting_active = true;
    ingress.fp_claim_answers.insert(id, vec![17, 18]);
    ingress.fp_winning_carriers.insert(id, carrier);
    let (accepted, birth_delta) = step(&state, &p, 4, 110, &[object], None, &ingress);
    state = accepted;
    let birth = state.last_point.unwrap();
    let actual_winner=birth_delta.entries.iter().find(|e|matches!(e,PalwDeltaEntryV2::KernelRouteRow{table,key,..}if *table==PALW_FP_WINNING_CARRIER_TABLE_V1&&key.as_slice()==id.as_byte_slice())).unwrap();
    let original_root = state.state_root();
    assert!(
        super::super::palw_claim_local_carrier_lineage_v9::covers_native_winner_event_v21(&state, birth, actual_winner).unwrap(),
        "ordinary actually folded unwrapped FP winner is covered by its exact Source8/9 owner"
    );
    let mut foreign = birth;
    foreign.blue_score += 1;
    assert!(
        !super::super::palw_claim_local_carrier_lineage_v9::covers_native_winner_event_v21(&state, foreign, actual_winner).unwrap(),
        "same carrier bytes do not lend another accepting birth"
    );
    let cold = PalwStateCarriageV2::from_state(&state).into_state(&p, Some(original_root)).unwrap();
    assert!(super::super::palw_claim_local_carrier_lineage_v9::covers_native_winner_event_v21(&cold, birth, actual_winner).unwrap());
    assert_eq!(state.state_root(), original_root);

    assert_eq!(
        state.kernel_route.as_ref().unwrap().aux[&(PALW_FP_WINNING_CARRIER_TABLE_V1, id.as_byte_slice().to_vec())],
        borsh::to_vec(&carrier).unwrap()
    );
    use super::super::{palw_claim_local_carrier_lineage_v9 as lineage, palw_claim_local_source_origin_v8::SourceOriginV8};
    let independent = SourceOriginV8::for_claim(&state, &p, id, state.claim(&id).unwrap()).unwrap().unwrap();
    assert!(
        lineage::visit_legacy_consumer_facts_v9(&state, &p, &independent, |_, _| panic!("actual independent F1 has no carrier legs"))
            .unwrap(),
        "genuine complete-empty old inventory is distinct from missing"
    );
    state = step(&state, &p, 5, 111, &[bind(id)], None, &x).0;
    state = step(&state, &p, 6, 112, &[license(id)], None, &x).0;
    state = step(&state, &p, 7, 133, &[], None, &x).0;
    let mut cert = row(&state, id);
    cert.anchor = Some((1, [7; 64]));
    cert.draws = vec![crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond_key(2))];
    cert.drawn_daa = 134;
    cert.deadline_daa = 135;
    cert.attestations = vec![crate::palw_model_eligibility_v1::PalwEligibilityAttestationRowV1 {
        verifier: cert.draws[0],
        verdict: crate::palw_model_eligibility_v1::PalwEligibilityVerdictV1::Reproduced,
        transcript: [9; 64],
        attested_daa: 134,
    }];
    let mut draw = x.clone();
    let mut key = vec![CERTIFICATION_CLAIM_PREFIX_V1];
    key.extend_from_slice(&palw_certification_id_v1(&id));
    draw.exec_v2_test_kernel_rows.push((PALW_SECURITY_CERTIFICATION_TABLE_V1, key, Some(borsh::to_vec(&cert).unwrap())));
    state = step(&state, &p, 8, 134, &[], None, &draw).0;
    state = step(&state, &p, 9, 136, &[], None, &x).0;
    state = step(&state, &p, 10, 156, &[], None, &x).0;
    assert!(state.claim(&id).is_none());
    assert!(compact::read(&state, id).unwrap().is_some());
    let birth_key = (PALW_CLAIM_LOCAL_TABLE_V1, palw_claim_liability_frame_key_v1(h64(4)).to_vec());
    assert!(state.kernel_route.as_ref().unwrap().aux.contains_key(&birth_key), "full source retained during actual trace duty");
    state = step(&state, &p, 11, 1001, &[], None, &x).0;
    assert!(!state.kernel_route.as_ref().unwrap().aux.contains_key(&birth_key), "ended full birth frame collected");
    assert!(
        !lineage::visit_legacy_consumer_facts_v9(&state, &p, &independent, |_, _| panic!("missing old F1 cannot invent facts"))
            .unwrap(),
        "absent actual F1 is not complete-empty"
    );
    state = PalwStateCarriageV2::from_state(&state).into_state_v3(&p, Some(state.state_root()), false, None).unwrap();
    let consumer = h64(0xFD);
    let next_prompt: Vec<u32> = (30..38).collect();
    let mut object = derived_commit(0xFD, &next_prompt, 2, work);
    if let PalwConsensusObjectV2::FreePromptCommitted { trace_retention_daa, execution_root, .. } = &mut object {
        *trace_retention_daa = 2000;
        *execution_root = h64(0xAFD)
    } else {
        unreachable!()
    }
    let mut next = x.clone();
    next.fp_answer_accounting_active = true;
    next.fp_claim_answers.insert(consumer, vec![41, 42]);
    next.fp_winning_carriers.insert(consumer, h64(0xCAFD));
    next.carrier_input_spends.insert(consumer, vec![carrier]);
    state = step(&state, &p, 12, 1002, &[object], None, &next).0;
    let origin =
        super::super::palw_claim_local_rewards_v1::origin_for(&state, &p, consumer, state.claim(&consumer).unwrap()).unwrap().unwrap();
    assert!(
        origin.frame().inputs.contains(&PalwClaimLocalInputV1::ConsumedCarrier { consumer, source: id }),
        "actual input inventory records the old winning carrier"
    );
    assert_eq!(
        compact::carrier(&state, &origin, id).unwrap(),
        Some(super::super::palw_claim_local_rewards_v1::RewardReadinessV1::Ready)
    );
    let old_consumer = SourceOriginV8::for_claim(&state, &p, consumer, state.claim(&consumer).unwrap()).unwrap().unwrap();
    let mut count = 0;
    assert!(
        lineage::visit_legacy_consumer_facts_v9(&state, &p, &old_consumer, |fact, source| {
            count += 1;
            assert_eq!(fact.spent_txid(), carrier);
            assert_eq!(source.source(), id);
            assert_eq!(source.accepting_point(), independent.accepting_point());
            assert!(source.closed_source_proof().is_some());
            Ok(())
        })
        .unwrap()
    );
    assert_eq!(count, 1);
    let mut calls = 0;
    let stop = lineage::visit_legacy_consumer_facts_v9(&state, &p, &old_consumer, |_, _| {
        calls += 1;
        Err(PalwStateV2Error::CarriageInconsistent("stop actual legacy visitor".into()))
    });
    assert!(stop.is_err());
    assert_eq!(calls, 1, "callback refusal propagates immediately");

    assert!(
        state
            .kernel_route
            .as_ref()
            .unwrap()
            .aux
            .contains_key(&(PALW_CLAIM_LOCAL_TABLE_V1, palw_claim_liability_frame_key_v1(h64(12)).to_vec())),
        "live consumer keeps complete original frame"
    );
}

#[test]
fn rfc23s2_compact_frame_capacity_refusal_child() {
    if std::env::var("MISAKA_COMPACT_FRAME_CAPACITY").ok().as_deref() != Some("1") {
        return;
    }
    use crate::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1};
    let (p, state, _) = world(false);
    let mut x = extra();
    x.native_fact_ruleset = Some(kaspa_hashes::Hash::from_bytes([41; 32]));
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &x);
    let before = b.state.state_root();
    let entries = b.entries.len();
    let point = ctx(10, 148, 10);
    let memory = 1 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(memory).unwrap();
    assert!(matches!(
        super::super::palw_claim_local_compact_sources_v1::compact_and_collect_v1(&mut b, &point),
        Err(PalwStateV2Error::NativeResource(_))
    ));
    assert_eq!(b.entries.len(), entries);
    assert_eq!(b.state.last_point, state.last_point, "failed local construction restores actual point marker");
    drop(blocker);
    assert_eq!(b.state.state_root(), before);
}
#[test]
fn rfc23s2_compact_frame_capacity_refusal_is_local_and_rolls_back() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "palw_state_v2::tests::claim_local_rewards_v1::rfc23s2_compact_frame_capacity_refusal_child", "--nocapture"])
        .env("MISAKA_COMPACT_FRAME_CAPACITY", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "{} {}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"));
}

/// Fault fixture only: malformed committed metadata must not become a guessed
/// eligible/ineligible bond; local allocation failure must remain distinguishable.
#[test]
fn native_common_capital_panel_checked_capacity_child() {
    if std::env::var("MISAKA_CHECKED_PANEL_CAPITAL").ok().as_deref() != Some("1") {
        return;
    }
    use crate::palw_native_budget_binding_v1::{NATIVE_BUDGET_BINDING_TABLE_V1, native_budget_binding_producer_key_v1};
    use crate::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1};
    use crate::palw_panel_v2::{
        PalwPanelDrawPolicyV1, PalwPanelValidLockV1, PalwRcoreSeatFilterV1, PalwStakeDrawCensusV1, PalwSw8DrawPointV1,
    };
    let (p, mut state, _) = world(false);
    let bond = bond_key(2);
    state
        .kernel_route
        .as_mut()
        .unwrap()
        .aux
        .insert((NATIVE_BUDGET_BINDING_TABLE_V1, native_budget_binding_producer_key_v1(&bond, h64(123))), vec![0]);
    let lock = PalwPanelValidLockV1 {
        required: 1,
        now_daa: 128,
        settled_anchor_depth: None,
        window_court: p.window_court(),
        rcore: Some(PalwRcoreSeatFilterV1 {
            eligibility: 1,
            ceiling_permille: 500,
            resolved_locks_off_ceiling: false,
            accuser_reserve: 0,
            held_charge_floor: None,
            gain_scale: crate::palw_weight_cap_v1::PalwCapacityGainScaleV1::of(&p),
        }),
    };
    let point = PalwSw8DrawPointV1 {
        max_slot: u64::MAX,
        draw_daa: 128,
        policy: PalwPanelDrawPolicyV1::default(),
        bond_maturity_window: None,
        settled_anchor_depth: None,
        capability_proof: false,
        shard_licensing: false,
    };
    let x = extra();
    let builder = TransitionBuilder::new(&state, &p, false, false, false, false, &x);
    let memory = 1 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(memory).unwrap();
    assert!(matches!(
        palw_bond_collateral_is_locked_checked_v7(
            &state,
            &p,
            &bond,
            state.bond(&bond).unwrap(),
            128,
            p.withdrawal_delay_daa(),
            None,
            true
        ),
        Err(PalwStateV2Error::NativeResource(_))
    ));
    assert!(matches!(builder.open_conviction_v1(&[bond], 128), Err(PalwStateV2Error::NativeResource(_))));
    assert!(matches!(lock.admits_checked_v2(&state, &bond), Err(PalwStateV2Error::NativeResource(_))));
    assert!(matches!(lock.room_checked_v2(&state, &bond), Err(PalwStateV2Error::NativeResource(_))));
    assert!(matches!(
        PalwStakeDrawCensusV1::new_checked_v2(&state, 1, 1, &point, None, Some(lock.frame_v1())),
        Err(PalwStateV2Error::NativeResource(_))
    ));
    drop(blocker);
    assert!(
        matches!(lock.admits_checked_v2(&state, &bond), Err(PalwStateV2Error::CarriageInconsistent(_))),
        "committed malformed source is a typed error, not ordinary bond exclusion"
    );
}
#[test]
fn native_common_capital_panel_capacity_is_not_candidate_exclusion() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "palw_state_v2::tests::claim_local_rewards_v1::native_common_capital_panel_checked_capacity_child",
            "--nocapture",
        ])
        .env("MISAKA_CHECKED_PANEL_CAPITAL", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "{} {}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"));
}

#[test]
fn rfc23s2_frame_collection_keeps_actual_live_dependency_birth_material() {
    let (p, state, [a, _, c]) = world(false);
    let mut x = extra();
    x.native_fact_ruleset = Some(kaspa_hashes::Hash::from_bytes([45; 32]));
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &x);
    let point = *state.last_point().unwrap();
    super::super::palw_claim_local_compact_sources_v1::compact_and_collect_v1(&mut b, &point).unwrap();
    for id in [a, c] {
        let born = b.state.claim(&id).unwrap();
        let original = super::super::palw_claim_local_rewards_v1::origin_for(&b.state, &p, id, born)
            .unwrap()
            .expect("actual live liability birth survives collection");
        assert_eq!(original.original_claim().execution_root, born.execution_root);
    }
    assert!(
        matches!(
            super::super::palw_claim_local_rewards_v1::reward_readiness_v1(&b.state, &p, 1001, c, false).unwrap(),
            super::super::palw_claim_local_rewards_v1::RewardReadinessV1::Hold(_)
        ),
        "collection cannot turn unresolved A's dependent C into ready credit"
    );
    let restored = PalwStateCarriageV2::from_state(&b.state).into_state_v3(&p, Some(b.state.state_root()), false, None).unwrap();
    assert_eq!(restored.state_root(), b.state.state_root());
}

#[test]
fn rfc0038_readmitted_raw_claim_keeps_exact_old_winner_after_active_index_replacement() {
    rfc0038_exact_old_winner_lifecycle(false, false);
}
#[test]
fn rfc0038_legacy_s1_upgrades_before_raw_reuse_after_f1_collection() {
    rfc0038_exact_old_winner_lifecycle(true, false);
}
#[test]
fn rfc0038_actual_carrier_geometry_empty_multiple_reused_source_cold_native_inverse() {
    rfc0038_exact_old_winner_lifecycle(false, true);
}
fn rfc0038_exact_old_winner_lifecycle(legacy_without_a5: bool, observe_geometry: bool) {
    // Core ingress/lifecycle regression with the existing graph-pricing and
    // named-draw fixture seams, not a multi-node producer/admission claim.
    use super::super::palw_claim_local_compact_sources_v1 as compact;
    use crate::palw_claim_local_v1::*;
    use crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1;
    let (base_params, base, class, _) = derived_work_chain();
    let p = base_params
        .with_claim_retirement_daa(1)
        .unwrap()
        .with_claim_local_settlement_from_daa(Some(110))
        .with_rfc0018_v1(rules().rfc0018_v1().cloned());
    let mut x = extra();
    x.fp_derived_work_daa = Some(0);
    x.native_fact_ruleset = Some(kaspa_hashes::Hash::from_bytes([39; 32]));
    let mut state = step(&base, &p, 3, 102, &[lane_certification(class)], None, &x).0;
    let id = h64(0xFC);
    let carrier = h64(0xCAFC);
    let prompt: Vec<u32> = (0..8).collect();
    let work = crate::palw_step::step_leaf_count_of_tokens_capped_v1(&derived_profile(), 8, 2, 1 << 26).unwrap();
    let mut object = derived_commit(0xFC, &prompt, 2, work);
    if let PalwConsensusObjectV2::FreePromptCommitted { trace_retention_daa, .. } = &mut object {
        *trace_retention_daa = 1000
    } else {
        unreachable!()
    }
    let mut ingress = x.clone();
    ingress.fp_answer_accounting_active = true;
    ingress.fp_claim_answers.insert(id, vec![17, 18]);
    ingress.fp_winning_carriers.insert(id, carrier);
    let geometry_source_parent = observe_geometry.then(|| state.clone());
    state = step(&state, &p, 4, 110, &[object], None, &ingress).0;
    assert_eq!(
        state.kernel_route.as_ref().unwrap().aux[&(PALW_FP_WINNING_CARRIER_TABLE_V1, id.as_byte_slice().to_vec())],
        borsh::to_vec(&carrier).unwrap()
    );
    if observe_geometry {
        if assert_actual_winner_geometry_writer_v9(
            geometry_source_parent.as_ref().unwrap(),
            &state,
            &p,
            PalwBlockContextV2 { subsidy: 1000, ..ctx(4, 110, 4) },
            id,
            carrier,
            &ingress,
        ) {
            return;
        }
        if assert_actual_carrier_geometry_recapture_v9(
            &state,
            &state,
            &p,
            PalwBlockContextV2 { subsidy: 1000, ..ctx(4, 110, 4) },
            id,
            &ingress,
            0,
        ) {
            return;
        }
    }
    state = step(&state, &p, 5, 111, &[bind(id)], None, &x).0;
    state = step(&state, &p, 6, 112, &[license(id)], None, &x).0;
    state = step(&state, &p, 7, 133, &[], None, &x).0;
    let mut cert = row(&state, id);
    cert.anchor = Some((1, [7; 64]));
    cert.draws = vec![crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond_key(2))];
    cert.drawn_daa = 134;
    cert.deadline_daa = 135;
    cert.attestations = vec![crate::palw_model_eligibility_v1::PalwEligibilityAttestationRowV1 {
        verifier: cert.draws[0],
        verdict: crate::palw_model_eligibility_v1::PalwEligibilityVerdictV1::Reproduced,
        transcript: [9; 64],
        attested_daa: 134,
    }];
    let mut draw = x.clone();
    let mut key = vec![CERTIFICATION_CLAIM_PREFIX_V1];
    key.extend_from_slice(&palw_certification_id_v1(&id));
    draw.exec_v2_test_kernel_rows.push((PALW_SECURITY_CERTIFICATION_TABLE_V1, key, Some(borsh::to_vec(&cert).unwrap())));
    state = step(&state, &p, 8, 134, &[], None, &draw).0;
    state = step(&state, &p, 9, 136, &[], None, &x).0;
    state = step(&state, &p, 10, 156, &[], None, &x).0;
    assert!(state.claim(&id).is_none());
    assert!(compact::read(&state, id).unwrap().is_some());
    let birth_key = (PALW_CLAIM_LOCAL_TABLE_V1, palw_claim_liability_frame_key_v1(h64(4)).to_vec());
    assert!(state.kernel_route.as_ref().unwrap().aux.contains_key(&birth_key), "full source retained during actual trace duty");
    state = step(&state, &p, 11, 1001, &[], None, &x).0;
    assert!(!state.kernel_route.as_ref().unwrap().aux.contains_key(&birth_key), "ended full birth frame collected");
    state = PalwStateCarriageV2::from_state(&state).into_state_v3(&p, Some(state.state_root()), false, None).unwrap();
    // Actual core admission writer, terminal lifecycle, collection and second
    // admission. GateC draw and graph-pricing seams above are NOT a signed
    // TFO2/FPR2 node-admission or pruned-node integration claim.
    let old = (*compact::read(&state, id).unwrap().unwrap()).clone();
    if legacy_without_a5 {
        state.kernel_route.as_mut().unwrap().aux.retain(|(table, key), raw| {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 {
                return true;
            }
            if key.first() == Some(&b'A') && raw.get(..2) == Some(&[5, 0]) {
                return false;
            }
            if key.first() == Some(&b'I') {
                let root: crate::palw_claim_local_context_nodes_v2::ComponentRootV2 = borsh::from_slice(raw).unwrap();
                if root.spec.owner == 6 {
                    return false;
                }
            }
            true
        });
        assert!(
            super::super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(&state, id, &old.original)
                .unwrap()
                .is_none()
        );
        state = PalwStateCarriageV2::from_state(&state).into_state_v3(&p, Some(state.state_root()), false, None).unwrap();
    }
    // A retired claim's paid prompt/answer coverage outlives its trace/body.
    // Reuse is legal only after the actual accounting row's native expiry,
    // not merely after trace retention 1000 or full-frame collection 1001.
    let readmit_daa = state.fp_claim_prompts[&(class, bond_key(1), id)].expires_daa;
    assert_eq!(readmit_daa, palw_fp_prompt_row_expiry_v1(110, p.epoch_length));
    assert_eq!(
        state.fp_claim_prompts[&(class, bond_key(1), crate::palw_fp_answer_accounting_v1::palw_fp_answer_row_key_v1(&id))].expires_daa,
        readmit_daa
    );
    assert!(readmit_daa > 1002);
    assert_eq!(
        crate::palw_fp_answer_accounting_v1::palw_fp_answer_covered_positions_v1(
            state.fp_paid_sequences_of_v1(&class, readmit_daa),
            &prompt,
            &[17, 18]
        ),
        0
    );
    let new_trace_until = readmit_daa.checked_add(1000).unwrap();
    let consumer_trace_until = readmit_daa.checked_add(2000).unwrap();
    let before = state.clone();
    let mut object = derived_commit(0xFC, &prompt, 2, work);
    if let PalwConsensusObjectV2::FreePromptCommitted { trace_retention_daa, .. } = &mut object {
        *trace_retention_daa = new_trace_until
    } else {
        unreachable!()
    }
    let new_carrier = h64(0xDDFC);
    let mut ingress = x.clone();
    ingress.fp_answer_accounting_active = true;
    ingress.fp_claim_answers.insert(id, vec![17, 18]);
    ingress.fp_winning_carriers.insert(id, new_carrier);
    // Preserve the production price refusal that diagnosed candidate29: the
    // identical raw FP/answer still has no new work at the old fixture DAA.
    let too_early = PalwBlockContextV2 { subsidy: 1000, ..ctx(12, 1002, 12) };
    assert!(matches!(
        apply_palw_transition_v2_with_extras(
            &before,
            &p,
            &too_early,
            std::slice::from_ref(&object),
            None,
            false,
            false,
            false,
            false,
            &ingress
        ),
        Err(PalwStateV2Error::ZeroQuanta)
    ));
    let (readmitted, delta) = step(&before, &p, 12, readmit_daa, &[object], None, &ingress);
    assert!(readmitted.claim(&id).unwrap().pwu > 0, "real canonical pricing admits positive work only at coverage expiry");
    if observe_geometry {
        if assert_actual_winner_geometry_writer_v9(
            &before,
            &readmitted,
            &p,
            PalwBlockContextV2 { subsidy: 1000, ..ctx(12, readmit_daa, 12) },
            id,
            new_carrier,
            &ingress,
        ) {
            return;
        }
    }
    assert_eq!(
        readmitted.kernel_route.as_ref().unwrap().aux[&(PALW_FP_WINNING_CARRIER_TABLE_V1, id.as_byte_slice().to_vec())],
        borsh::to_vec(&new_carrier).unwrap()
    );
    assert_ne!(readmitted.claim(&id).unwrap().accepted_block, old.original.accepted_block);
    compact::validate_compact_source_winner_v5(&readmitted, &p, id, &old).unwrap();
    let raw = borsh::to_vec(&PalwStateCarriageV2::from_state(&readmitted)).unwrap();
    let cold: PalwStateCarriageV2 = borsh::from_slice(&raw).unwrap();
    let cold = cold.into_state_v3(&p, Some(readmitted.state_root()), false, None).unwrap();
    compact::validate_compact_source_winner_v5(&cold, &p, id, &old).unwrap();
    let mut contradictory = old.clone();
    contradictory.permitted = !old.permitted;
    assert!(
        compact::validate_compact_source_winner_v5(&cold, &p, id, &contradictory).is_err(),
        "S1 must match whole A5 body even with matching immutable winner"
    );
    assert_eq!(apply_delta_v2(&before, &delta, &p).unwrap().state_root(), cold.state_root());
    assert_eq!(revert_delta_v2(&cold, &delta, &p).unwrap().state_root(), before.state_root());
    let mut altered = old.clone();
    altered.winner = Some(h64(0xBAD));
    assert!(compact::validate_compact_source_winner_v5(&cold, &p, id, &altered).is_err(), "old winner must match actual A5 body");
    let mut missing = cold.clone();
    missing.kernel_route.as_mut().unwrap().aux.retain(|(t, k), _| *t != PALW_CLAIM_LOCAL_TABLE_V1 || k.first() != Some(&b'A'));
    assert!(
        compact::validate_compact_source_winner_v5(&missing, &p, id, &old).is_err(),
        "A5 absent never authorizes an unrelated current index"
    );
    // Actual new admission spends the OLD output after raw-ID re-admission,
    // then cold restore/replay must retain its exact old source point. This
    // tests provenance/lifecycle under the existing named-draw fixture seam;
    // it is not a claim of signed multi-node selected-verifier conformance.
    use super::super::palw_claim_local_carrier_lineage_v9 as lineage;
    assert_eq!(
        lineage::with_exact_source_v9(&cold, id, old.birth, carrier, |v| {
            assert_eq!(v.accepting_point(), old.birth);
            assert_eq!(v.winning_txid(), carrier);
            assert!(!lineage::is_exact_live_source_v9(&cold, &v));
            assert_eq!(v.closed_source_proof(), Some(&old));
            Ok(v.outcome())
        })
        .unwrap(),
        Some(lineage::CarrierOutcomeV9::Unresolved)
    );
    let consumer = h64(0xFE);
    let consumer_prompt: Vec<u32> = (50..58).collect();
    let mut consumer_object = derived_commit(0xFE, &consumer_prompt, 2, work);
    if let PalwConsensusObjectV2::FreePromptCommitted { trace_retention_daa, execution_root, .. } = &mut consumer_object {
        *trace_retention_daa = consumer_trace_until;
        *execution_root = h64(0xAFE)
    } else {
        unreachable!()
    }
    let mut spending = x.clone();
    spending.fp_answer_accounting_active = true;
    spending.fp_claim_answers.insert(consumer, vec![61, 62]);
    spending.fp_winning_carriers.insert(consumer, h64(0xCAFE));
    spending.carrier_input_spends.insert(consumer, vec![carrier]);
    let (spent, spent_delta) = step(&cold, &p, 13, readmit_daa + 1, &[consumer_object], None, &spending);
    let consumer_point = PalwBlockContextV2 { subsidy: 1000, ..ctx(13, readmit_daa + 1, 13) };
    let assert_facts = |state: &PalwChainStateV2| {
        let mut count = 0;
        assert!(
            lineage::visit_consumer_facts_v9(state, consumer, consumer_point, |fact, source| {
                count += 1;
                assert_eq!(fact.spent_txid(), carrier);
                assert_eq!(fact.source(), id);
                assert_eq!(fact.source_point(), old.birth);
                assert_eq!(source.accepting_point(), old.birth);
                assert_ne!(source.accepting_point().block, state.claim(&id).unwrap().accepted_block);
                Ok(())
            })
            .unwrap()
        );
        assert_eq!(count, 1, "one actual spent winning transaction has one born owner");
    };
    assert_facts(&spent);
    let projected =
        super::super::palw_claim_local_rewards_v1::dependency_inputs_v1(&spent, &p, consumer, spent.claim(&consumer).unwrap(), None)
            .unwrap();
    assert!(
        !projected.contains(&PalwClaimLocalWorkRefV1::ChainClaim { claim: id }),
        "old carrier owner must not enlist the later live raw-ID birth"
    );
    // A distinct actual admission spends the NEW winning carrier. It must
    // contribute the exact currently live source to the checkpoint graph.
    let fresh = h64(0xFF);
    let fresh_prompt: Vec<u32> = (70..78).collect();
    let mut object = derived_commit(0xFF, &fresh_prompt, 2, work);
    if let PalwConsensusObjectV2::FreePromptCommitted { trace_retention_daa, execution_root, .. } = &mut object {
        *trace_retention_daa = consumer_trace_until;
        *execution_root = h64(0xAFF)
    } else {
        unreachable!()
    }
    let mut ingress = x.clone();
    ingress.fp_answer_accounting_active = true;
    ingress.fp_claim_answers.insert(fresh, vec![81, 82]);
    ingress.fp_winning_carriers.insert(fresh, h64(0xCAFF));
    ingress.carrier_input_spends.insert(fresh, vec![new_carrier]);
    let (latest, _) = step(&spent, &p, 14, readmit_daa + 2, &[object], None, &ingress);
    let projected =
        super::super::palw_claim_local_rewards_v1::dependency_inputs_v1(&latest, &p, fresh, latest.claim(&fresh).unwrap(), None)
            .unwrap();
    assert!(projected.contains(&PalwClaimLocalWorkRefV1::ChainClaim { claim: id }));
    let point = PalwBlockContextV2 { subsidy: 1000, ..ctx(14, readmit_daa + 2, 14) };
    let checkpoint =
        super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&latest, &p, &point).unwrap().unwrap();
    assert!(
        checkpoint.records[&PalwClaimLocalWorkRefV1::ChainClaim { claim: fresh }]
            .inputs
            .contains(&PalwClaimLocalWorkRefV1::ChainClaim { claim: id })
    );
    drop(checkpoint);
    let mut no_frames = latest.clone();
    no_frames.kernel_route.as_mut().unwrap().aux.retain(|(t, k), _| *t != PALW_CLAIM_LOCAL_TABLE_V1 || k.first() != Some(&b'F'));
    let projected =
        super::super::palw_claim_local_rewards_v1::dependency_inputs_v1(&no_frames, &p, fresh, no_frames.claim(&fresh).unwrap(), None)
            .unwrap();
    assert!(
        projected.contains(&PalwClaimLocalWorkRefV1::ChainClaim { claim: id }),
        "actual scoped facts retain graph edges without F1"
    );
    let restored = rfc37_native_row_roundtrip_v1::roundtrip(&spent, &p);
    assert_facts(&restored);
    assert_facts(&apply_delta_v2(&cold, &spent_delta, &p).unwrap());
    assert_eq!(revert_delta_v2(&restored, &spent_delta, &p).unwrap().state_root(), cold.state_root());
    if observe_geometry {
        // Actual new consumer carries two distinct winning inputs for SAME raw
        // source at different births, plus one duplicate and one ordinary tx.
        // The native capture must retain two exact source scopes, not four
        // producer counts and not one current raw source alias.
        let multi = h64(0x101);
        let prompt: Vec<u32> = (90..98).collect();
        let mut object = derived_commit(0x101, &prompt, 2, work);
        if let PalwConsensusObjectV2::FreePromptCommitted { trace_retention_daa, execution_root, .. } = &mut object {
            *trace_retention_daa = consumer_trace_until;
            *execution_root = h64(0xA101)
        } else {
            unreachable!()
        }
        let mut multi_extras = x.clone();
        multi_extras.fp_answer_accounting_active = true;
        multi_extras.fp_claim_answers.insert(multi, vec![91, 92]);
        multi_extras.fp_winning_carriers.insert(multi, h64(0xCA101));
        multi_extras.carrier_input_spends.insert(multi, vec![carrier, new_carrier, new_carrier, h64(0xDADA)]);
        let (actual, _) = step(&latest, &p, 15, readmit_daa + 3, &[object], None, &multi_extras);
        if assert_actual_carrier_geometry_recapture_v9(
            &actual,
            &latest,
            &p,
            PalwBlockContextV2 { subsidy: 1000, ..ctx(15, readmit_daa + 3, 15) },
            multi,
            &multi_extras,
            2,
        ) {
            return;
        }
    }
    let mut same = cold.clone();
    let live = same.claims.get_mut(&id).unwrap();
    live.accepted_block = old.original.accepted_block;
    live.accepted_daa = old.original.accepted_daa;
    live.accepted_blue_score = old.original.accepted_blue_score;
    assert!(
        compact::validate_compact_source_winner_v5(&same, &p, id, &old).is_err(),
        "same-birth corrupt current index still refuses even with A5"
    );
}

// Regression boundary: actual core-admitted source/consumer + REAL native
// metadata release/recapture writers, not a signed node funding/admission proof.
fn assert_actual_carrier_geometry_recapture_v9(
    state: &PalwChainStateV2,
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    point: PalwBlockContextV2,
    consumer: Hash64,
    extras: &PalwTransitionExtrasV1,
    expected_facts: u64,
) -> bool {
    use super::super::{palw_claim_local_carrier_lineage_v9 as lineage, palw_claim_local_source_admission_v8 as source};
    let admission = source::for_birth_point_v8(state, consumer, point.block, point.daa_score, point.blue_score).unwrap().unwrap();
    let original = admission.original_claim().clone();
    let manifest = source::manifest_reference_v9(state, consumer, point).unwrap().unwrap();
    let mut b = TransitionBuilder::new(state, p, false, false, false, false, extras);
    // A metadata owner-boundary fixture only: normal semantic owner performs
    // release at actual source-duty collection. We exercise real delete+mint
    // writers here without claiming that collection occurred at this point.
    lineage::release_consumer_facts_v9(&mut b, consumer, point).unwrap();
    let before = b.state.clone();
    let first = b.entries.len();
    let failure = std::env::var("MISAKA_CARRIER_GEOMETRY_FAILURE").unwrap_or_default();
    if failure == "consumer-cardinality" || failure == "consumer-report" {
        lineage::arm_geometry_observer_failure_v9(if failure == "consumer-cardinality" { 1 } else { 2 });
        let refused = lineage::capture_consumer_facts_observed_v9(&mut b, parent, point, consumer, &original, manifest);
        assert!(
            matches!(refused, Err(PalwStateV2Error::NativeResource(_))),
            "actual observer-local demand must refuse AFTER successful capture"
        );
        assert!(lineage::geometry_observer_failure_hit_v9());
        assert_geometry_rollback_complete_v9(&b, &before, first);
        return true;
    }
    let report = lineage::capture_consumer_facts_observed_v9(&mut b, parent, point, consumer, &original, manifest).unwrap();
    assert_eq!(report.scope().consumer, consumer);
    assert_eq!(report.scope().accepting_point, point);
    assert_eq!(report.scope().source_manifest, manifest);
    assert_eq!(report.referenced_rows()[0].rows, expected_facts);
    assert_eq!(report.referenced_rows()[1].rows, expected_facts);
    assert_eq!(report.referenced_rows()[2].rows, 1, "even complete empty input list has an actual CONSUMER row");
    assert_eq!(report.scope().sealed_facts.bytes, report.referenced_rows()[2].encoded_value_bytes);
    assert!(report.local_observation_demand().report_wrapper_requests > 0);
    let input_counts = report.actual_input_cardinality().unwrap();
    if expected_facts == 0 {
        assert_eq!(input_counts.funded_input_occurrences + input_counts.accepted_input_occurrences, 0);
        assert_eq!(input_counts.distinct_spent_txids, 0)
    }
    if expected_facts == 2 {
        assert_eq!(input_counts.funded_input_occurrences, 0);
        assert_eq!(input_counts.accepted_input_occurrences, 4);
        assert_eq!(input_counts.distinct_spent_txids, 3)
    }

    let journal = report.actual_journal().unwrap();
    let canonical = journal.nbia.iter().map(|g| g.encoded_delta_bytes).sum::<u64>() + journal.literal_winner167.encoded_delta_bytes;
    assert_eq!(canonical, b.entries[first..].iter().map(|e| borsh::object_length(e).unwrap() as u64).sum::<u64>());
    let old = journal.nbia.iter().map(|g| g.old_encoded_value_bytes).sum::<u64>() + journal.literal_winner167.old_encoded_value_bytes;
    let new = journal.nbia.iter().map(|g| g.new_encoded_value_bytes).sum::<u64>() + journal.literal_winner167.new_encoded_value_bytes;
    assert_eq!(
        old,
        b.entries[first..]
            .iter()
            .map(|e| match e {
                PalwDeltaEntryV2::KernelRouteRow { old, .. } => old.as_ref().map_or(0, |r| r.len() as u64),
                _ => panic!("native capture aux journal only"),
            })
            .sum::<u64>()
    );
    assert_eq!(
        new,
        b.entries[first..]
            .iter()
            .map(|e| match e {
                PalwDeltaEntryV2::KernelRouteRow { new, .. } => new.as_ref().map_or(0, |r| r.len() as u64),
                _ => panic!("native capture aux journal only"),
            })
            .sum::<u64>()
    );
    let mut seen = 0;
    lineage::visit_observed_sources_v9(&b.state, &report, |fact, owner| {
        assert_eq!(fact.consumer(), consumer);
        assert_eq!(fact.consumer_point(), point);
        assert_eq!(fact.source_point(), owner.accepting_point());
        assert_eq!(fact.spent_txid(), owner.winning_txid());
        let winner =
            lineage::observe_sealed_winner_geometry_v9(&b.state, owner.source(), owner.accepting_point(), owner.winning_txid())?;
        assert_eq!(winner.source_scope(), (owner.source(), owner.accepting_point(), owner.winning_txid()));
        assert_eq!(winner.referenced_rows()[0].rows, 1);
        assert_eq!(winner.referenced_rows()[1].rows, 1);
        assert!(winner.actual_journal().is_none(), "cannot infer original source mint bytes from current state");
        seen += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(seen, expected_facts);
    let child = b.state.clone();
    let delta = PalwStateDeltaV2 { point, entries: b.entries[first..].to_vec() };
    assert_eq!(apply_delta_v2(&before, &delta, p).unwrap().state_root(), child.state_root());
    assert_eq!(
        revert_delta_v2(&child, &delta, p).unwrap().state_root(),
        before.state_root(),
        "actual native inverse restores every old byte role"
    );
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&child)).unwrap();
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
    let cold = carriage.into_state_v3(p, Some(child.state_root()), false, None).unwrap();
    let read = lineage::observe_sealed_consumer_geometry_v9(&cold, consumer, point).unwrap();
    assert_eq!(read.scope(), report.scope());
    assert_eq!(read.referenced_rows(), report.referenced_rows());
    assert_eq!(read.retention_reasons(), report.retention_reasons());
    assert!(read.actual_journal().is_none(), "cold sealed rows prove current canonical quantities, not old journal traffic");
    assert!(read.actual_input_cardinality().is_none(), "ordinary input list is not invented from surviving source-only facts");
    false
}

fn assert_actual_winner_geometry_writer_v9(
    parent: &PalwChainStateV2,
    accepted: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    point: PalwBlockContextV2,
    source: Hash64,
    winner: Hash64,
    extras: &PalwTransitionExtrasV1,
) -> bool {
    use super::super::{palw_claim_local_carrier_lineage_v9 as lineage, palw_claim_local_source_admission_v8 as source_archive};
    let view = source_archive::for_birth_point_v8(accepted, source, point.block, point.daa_score, point.blue_score).unwrap().unwrap();
    let original = view.original_claim().clone();
    assert!(!parent.claims.contains_key(&source));
    // Owner boundary fixture: use the actual successful admission's original,
    // but call the REAL Claim and winner writers under this fresh builder.
    // This is not an additional signed admission/funding proof.
    let mut b = TransitionBuilder::new(parent, p, false, false, false, false, extras);
    b.write_claim(source, Some(original)).unwrap();
    let before = b.state.clone();
    let first = b.entries.len();
    if std::env::var("MISAKA_CARRIER_GEOMETRY_FAILURE").ok().as_deref() == Some("winner-report") {
        lineage::arm_geometry_observer_failure_v9(2);
        let refused = lineage::write_actual_winner_observed_v9(&mut b, &point, source, winner);
        assert!(
            matches!(refused, Err(PalwStateV2Error::NativeResource(_))),
            "actual observer report demand refuses AFTER winner167 write"
        );
        assert!(lineage::geometry_observer_failure_hit_v9());
        assert_geometry_rollback_complete_v9(&b, &before, first);
        return true;
    }
    let report = lineage::write_actual_winner_observed_v9(&mut b, &point, source, winner).unwrap();
    assert_eq!(report.source_scope(), (source, point, winner));
    assert_eq!(report.referenced_rows()[0].rows, 1);
    assert_eq!(report.referenced_rows()[1].rows, 1);
    assert_eq!(report.referenced_rows()[2].rows, 0);
    let journal = report.actual_journal().unwrap();
    assert_eq!(journal.literal_winner167.writes, 1);
    let actual = journal.nbia.iter().map(|g| g.encoded_delta_bytes).sum::<u64>() + journal.literal_winner167.encoded_delta_bytes;
    assert_eq!(actual, b.entries[first..].iter().map(|e| borsh::object_length(e).unwrap() as u64).sum::<u64>());
    let after = b.state.clone();
    let delta = PalwStateDeltaV2 { point, entries: b.entries[first..].to_vec() };
    assert_eq!(apply_delta_v2(&before, &delta, p).unwrap().kernel_route, after.kernel_route);
    assert_eq!(revert_delta_v2(&after, &delta, p).unwrap().kernel_route, before.kernel_route);
    false
}

fn assert_geometry_rollback_complete_v9(b: &TransitionBuilder<'_>, before: &PalwChainStateV2, entries: usize) {
    assert_eq!(b.entries.len(), entries, "observer refusal must remove every appended metadata delta");
    assert!(b.state == *before, "observer refusal restores entire native state, no standing/result may escape");
    assert!(
        b.state.kernel_route.as_ref().unwrap().aux == before.kernel_route.as_ref().unwrap().aux,
        "includes every NBIA and literal167 old value"
    );
    assert_eq!(b.state.claim_local_legacy_carriers_ready_v9, before.claim_local_legacy_carriers_ready_v9);
    assert!(b.state.claim_local_legacy_carriers_v9 == before.claim_local_legacy_carriers_v9);
    assert!(b.state.material_contracts_by_member_v8 == before.material_contracts_by_member_v8);
}
#[test]
fn rfc0038_actual_carrier_geometry_postwrite_resource_failure_child() {
    if std::env::var("MISAKA_CARRIER_GEOMETRY_FAILURE").is_err() {
        return;
    }
    rfc0038_exact_old_winner_lifecycle(false, true);
    assert!(
        super::super::palw_claim_local_carrier_lineage_v9::geometry_observer_failure_hit_v9(),
        "must reach requested post-writer observer stage"
    );
}
#[test]
fn rfc0038_actual_carrier_geometry_postwrite_resource_rollback() {
    for mode in ["winner-report", "consumer-cardinality", "consumer-report"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "palw_state_v2::tests::claim_local_rewards_v1::rfc0038_actual_carrier_geometry_postwrite_resource_failure_child",
                "--nocapture",
            ])
            .env("MISAKA_CARRIER_GEOMETRY_FAILURE", mode)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "postwrite real-demand rollback {mode}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

