//! Actual signed EXEC admission/selection/retirement, with the private eight-card
//! Gate C profile. This is a lifecycle campaign, not a public activation record.
//! It inherits the conformance harness's pre-genesis public-artifact availability
//! fixture and private parameter policy. `no_seam` disables its native class
//! admission/trace-lottery bypass; no selected-approval row is injected.
use super::*;
use kaspa_consensus_core::palw_security_certification_v1::*;
use kaspa_consensus_core::palw_state_v2::{
    PALW_COMPACT_CLAIM_SOURCE_PREFIX_V1, PalwClaimLocalSettlementV1,
    palw_claim_local_recorded_settlement_v1,
};

// The established CERT lane's TEST-ONLY m=5/k=3 profile. It exercises actual
// bonded-pool selection and signatures; it is not the public 75/41 policy.
const POLICY: PalwSecurityCertificationPolicyV1 = PalwSecurityCertificationPolicyV1 {
    m: 5, k: 3, veto: 0, anchor_delay_spans: 1, attest_window_daa: 600,
    prosecution_window_daa: 40, certify_by_daa: 4_000,
    verifier_maturity_daa: 0, verifier_min_collateral_sompi: 1,
    draws_per_epoch: 64, epoch_daa: 1_000_000,
};

fn profile(params: &mut kaspa_consensus_core::config::params::Params) {
    params.palw_model_tier_v1 = Some(kaspa_consensus_core::palw_model_tier_v1::PalwModelTierFenceV1::at(ForkActivation::new(0)));
    params.sync_palw_model_tier_v1();
    params.palw_independently_work_certified_v1 = Some(ForkActivation::new(0));
    params.sync_palw_rfc0018_v1();
    params.palw_unresolved_bound_v1 = Some(ForkActivation::new(0));
    params.sync_palw_unresolved_bound_v1();
    params.palw_claim_local_settlement_v1 = Some(ForkActivation::new(0));
    params.sync_palw_claim_local_settlement_v1();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else { panic!("V2") };
    let mirror = kaspa_consensus_core::palw_rfc0018_v1::PalwRfc0018MirrorV1 {
        certification: POLICY, ..*bundle.state.rfc0018_v1().expect("actual Gate C mirror")
    };
    // Frozen before genesis. Never shorten a live claim's promise to fit a test.
    let retirement = bundle.state.fp_abandon_hold_daa().saturating_add(1).max(1_024);
    bundle.state = bundle.state.clone().with_rfc0018_v1(Some(mirror)).with_claim_retirement_daa(retirement).unwrap();
    assert!(params.validate_palw_claim_local_settlement_v1().is_err(), "public stage 2 still requires measured activation evidence");
}

fn chain_cert(net: &Net, claim: Hash64) -> Option<PalwClaimCertificationRowV1> {
    net.api()?.aux_row(PALW_SECURITY_CERTIFICATION_TABLE_V1, &certification_claim_key_v1(&palw_certification_id_v1(&claim)))
}

/// The ordinary native Panel has licensed this honest execution already. The
/// *separate* Gate C decision is made by its actual drawn bonds, through signed
/// carrier transactions; neither matching replay nor Final stands in for it.
async fn certify_root(cw: &mut Cw, s: &Session) {
    use misaka_palw_kernel::model_eligibility_v1::{EligibilityAttestationV1, EligibilityVerdictV1, ELIGIBILITY_ATTESTATION_VERSION_V1};
    let claim = s.real.claim;
    let row = chain_cert(&cw.net, claim).expect("Final committed a real Gate C subject");
    assert!(row.outcome.is_none(), "Final alone cannot certify");
    for round in 0..64 {
        if chain_cert(&cw.net, claim).is_some_and(|r| r.anchor.is_some()) { break; }
        let ttpb = cw.net.ttpb();
        cw.net.chain.attempt(5 + round % 3, ttpb, Vec::new(), &|_| true).await;
        cw.net.beat_to(cw.net.daa() + 8).await;
    }
    let row = chain_cert(&cw.net, claim).unwrap();
    assert!(row.anchor.is_some(), "actual later selected-chain seed required");
    assert_eq!(row.draws.len(), POLICY.m as usize);
    // Commit the actual honest public execution's claim/output and selected
    // prefix/slice receipts into each verifier's transcript. This authenticates
    // their decision under the honest-verifier premise, not a computation proof.
    let transcript = misaka_palw_kernel::hash::object_id(b"misaka/test/exec-retired-verifier/v1", &(claim, s.real.run.clone(), &row.draws));
    let mut attestations = Vec::new();
    for card in 0..8 {
        let verifier = cw.net.kid(card);
        if !row.draws.contains(&verifier) { continue; }
        let attestation = EligibilityAttestationV1 {
            version: ELIGIBILITY_ATTESTATION_VERSION_V1, network: cw.net.domain.as_bytes(),
            exam: palw_certification_id_v1(&claim), verifier,
            verdict: EligibilityVerdictV1::Reproduced, transcript, claim_mismatches: Vec::new(),
        };
        attestations.push((card, cw.net.route(card, &K::AttestEligibilityExamV1 { attestation })));
    }
    let subject_before=cw.net.chain.tip_state().1.claim(&claim).unwrap().clone();
    cw.net.send(attestations).await;
    let deadline = cw.net.daa() + POLICY.prosecution_window_daa + 200;
    while chain_cert(&cw.net, claim).unwrap().outcome.is_none() {
        assert!(cw.net.daa() < deadline, "selected approvals must be decided by actual blocks");
        cw.net.beat_to(cw.net.daa() + 1).await;
    }
    assert!(chain_cert(&cw.net, claim).unwrap().outcome.unwrap().0.certifies());
    let state=cw.net.chain.tip_state().1;
    assert_eq!(state.claim(&claim),Some(&subject_before),"actual selected approval is a sidecar-only effect, not a rewritten base claim");
    let point=*state.last_point().unwrap();
    let _held=kaspa_consensus_core::palw_state_v2::palw_claim_local_owned_checkpoint_v1(&state,&cw.net.bundle.state,&point).unwrap().unwrap();
    let checkpoint=kaspa_consensus_core::palw_state_v2::palw_claim_local_checkpoint_snapshot_v2(&state).unwrap().unwrap();
    let key=certification_claim_key_v1(&palw_certification_id_v1(&claim));
    let mut approval_frame=None;
    for ((table,key_frame),bytes) in &state.kernel_route_state_v1().unwrap().aux {
        if *table!=kaspa_consensus_core::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1||key_frame.len()!=65||key_frame[0]!=b'F'{continue}
        let frame=kaspa_consensus_core::palw_claim_local_v1::palw_decode_claim_liability_frame_v1(bytes).unwrap();
        if frame.native_changes.iter().any(|change|change.owner==kaspa_consensus_core::palw_claim_local_v1::PalwClaimLocalRowOwnerV1::KernelRoute&&change.table==PALW_SECURITY_CERTIFICATION_TABLE_V1&&change.key==key&&change.new.as_ref().is_some_and(|raw|borsh::from_slice::<PalwClaimCertificationRowV1>(raw).is_ok_and(|row|row.outcome.is_some_and(|(outcome,_)|outcome.certifies())))) {
            assert!(!frame.native_changes.iter().any(|change|change.owner==kaspa_consensus_core::palw_claim_local_v1::PalwClaimLocalRowOwnerV1::ChainClaims&&change.key.as_slice()==claim.as_byte_slice()),"this regression needs genuine approval without a base-claim write");
            if approval_frame.as_ref().is_none_or(|(blue,_)|*blue<frame.point.blue_score){approval_frame=Some((frame.point.blue_score,kaspa_consensus_core::palw_claim_local_v1::palw_liability_material_id_v1(bytes)));}
        }
    }
    let (_,material)=approval_frame.expect("real selected signatures and prosecution tick produced an actual certifying sidecar journal");
    use kaspa_consensus_core::palw_claim_local_v1::PalwClaimLocalWorkRefV1 as W;
    let source=&checkpoint.records[&W::ChainClaim{claim}];
    assert!(source.certified&&!source.liability_closed,"actual approval is retained through the open material duty");
    assert!(source.material.contains(&material),"an old unapproved birth frame is not the selected-approval context");
    // A's genuine sidecar decision can change dependent C's eligibility, but
    // does not make C or an unrelated B the owner of A's approval material.
    for work in [W::ExecSession{root:s.real.claim},W::KernelClaim{claim:Hash64::from_bytes(s.prefix.claim.id)}] {
        let record=&checkpoint.records[&work];
        if record.accepting_block!=source.accepting_block {
            assert!(!record.material.contains(&material),"unrelated/dependent work must reach A through the actual edge, not acquire A's latest frame");
        }
    }
}

#[tokio::test]
async fn rfc23s2_no_seam_exec_retirement_compact_source_restart_and_replay() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut cw = active_world_over_with_params(
        VerificationModeV1::OptimisticPublicVerification,
        |config| TestConsensus::with_db(db.clone(), config, sender), profile,
    ).await;
    cw.net._keep.push(Box::new(receiver));
    let tier = kaspa_consensus_core::palw_model_tier_v1::palw_model_tier_v1(&cw.net.chain.tip_state().1, &cw.net.bundle.state, &cw.v2_class);
    assert_eq!(tier, kaspa_consensus_core::palw_model_tier_v1::PalwModelTierV1::Registered);
    assert_eq!(kaspa_consensus_core::palw_model_tier_v1::palw_model_tier_coefficient_permille_v1(
        cw.net.bundle.state.model_tier_v1(), tier, cw.net.daa()), 300,
        "the legacy capability fixture does not review or promote a registered model's fixed tier");
    cw.no_seam = true;
    let mut s = opened_with_lifetime(&mut cw, false, |_| false, |_, _, _| Vec::new(), 2_048).await;
    for (index, slice) in s.slices.iter().enumerate() {
        s.lane.push(carry_slice(&mut cw, s.real.claim, index as u32, &slice.claim).await);
    }
    // Retain the actual material capability only while examining its checkpoint;
    // do not keep a second historical workspace alive during subsequent mining.
    let source_birth = {
        let before = cw.net.chain.tip_state().1;
        let point = before.last_point().unwrap();
        let _checked = kaspa_consensus_core::palw_state_v2::palw_claim_local_owned_checkpoint_v1(&before, &cw.net.bundle.state, point).unwrap().expect("actual material checkpoint");
        let checkpoint=kaspa_consensus_core::palw_state_v2::palw_claim_local_checkpoint_snapshot_v2(&before).unwrap().unwrap();
        checkpoint.records[&kaspa_consensus_core::palw_claim_local_v1::PalwClaimLocalWorkRefV1::KernelClaim { claim: Hash64::from_bytes(s.prefix.claim.id) }].accepting_block
    };
    let (final_daa, _) = finalize_native_root(&mut cw, &s).await;
    let source = cw.net.chain.tip_state().1.claim(&s.real.claim).unwrap().clone();
    assert!(matches!(source.phase, PalwClaimPhaseV2::Final { final_daa: daa } if daa == final_daa));
    certify_root(&mut cw, &s).await;
    let current = cw.net.chain.tip_state().1;
    let params = &cw.net.bundle.state;
    let material_until = current.exec_v2_liability_until_v1(params, &s.real.claim).unwrap().expect("actual EXEC promise");
    let end = material_until.max(source.trace_retention_daa).max(final_daa + params.claim_retirement_daa()) + 3;
    eprintln!("rfc23s2/real EXEC: Final={final_daa}, materialUntil={material_until}, retirementTarget={end}");
    cw.net.beat_to(end).await;
    let state = cw.net.chain.tip_state().1;
    assert!(state.claim(&s.real.claim).is_none(), "actual deadline retirement must run");
    assert_eq!(palw_claim_local_recorded_settlement_v1(&state, &cw.net.bundle.state, s.real.claim, &source, cw.net.daa()).unwrap(), PalwClaimLocalSettlementV1::Settled,
        "selected complete retired EXEC must not be permanently held");
    let mut key = vec![PALW_COMPACT_CLAIM_SOURCE_PREFIX_V1];
    key.extend_from_slice(s.real.claim.as_byte_slice());
    assert!(state.kernel_route_state_v1().unwrap().aux.contains_key(&(kaspa_consensus_core::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, key.clone())), "actual source became compact standing");
    let mut kernel_key = vec![kaspa_consensus_core::palw_state_v2::PALW_COMPACT_KERNEL_ORIGIN_PREFIX_V1];
    kernel_key.extend_from_slice(&s.prefix.claim.id);
    let route = state.kernel_route_state_v1().unwrap();
    let table = kaspa_consensus_core::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
    assert!(route.aux.contains_key(&(table, kernel_key)), "closed actual paid kernel keeps compact native DOB");
    let mut frame_key=vec![b'F'];frame_key.extend_from_slice(source_birth.as_byte_slice());
    assert!(!route.aux.contains_key(&(table,frame_key)), "closed kernel DOB must not pin its whole historical carriage forever");
    let root = state.state_root();
    let recovered = cw.net.restart(db.clone());
    assert_eq!(recovered.chain.tip_state().1.state_root(), root);
    assert_eq!(palw_claim_local_recorded_settlement_v1(&recovered.chain.tip_state().1, &recovered.bundle.state, s.real.claim, &source, recovered.daa()).unwrap(), PalwClaimLocalSettlementV1::Settled);
    let replay = replay_with_lane(&recovered, &s.lane).await;
    recovered.assert_same(&replay, "actual selected EXEC retired-source compact cold restart and lane replay");
}
