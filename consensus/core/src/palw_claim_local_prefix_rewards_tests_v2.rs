//! Candidate child of palw_claim_local_rewards_v1::prefix_rewards.
//! Real readiness/opaque admission/F1+S1/native E-budget readers, real ML-DSA
//! export/use signatures. Historical carriages/terminal S1 are fixture seams:
//! this does NOT establish normal FP admission, native paid source-export court,
//! A5 publication, cold IBD or a node reorg acceptance result.
use super::super::super::palw_claim_local_compact_sources_v1 as compact;
use super::super::super::palw_claim_local_prefix_reservation_v1 as reservations;
use super::*;
use crate::palw_claim_local_v1::prefix_v1::*;
use crate::palw_claim_local_v1::*;
use crate::palw_freeprompt_v3::{PalwFpPrefixStateV1, PalwFreePromptCommitmentV3};
use libcrux_ml_dsa::ml_dsa_87 as dsa;
fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn point(n: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(10_000 + n), daa_score: n, blue_score: n, subsidy: 0 }
}
fn claim(job: &PalwFreePromptCommitmentV3, p: PalwBlockContextV2) -> PalwClaimStateV2 {
    PalwClaimStateV2 {
        source: PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() },
        class_id: job.job.class_id,
        bond: PalwBondKeyV2(job.job.executor_bond),
        pwu: 1,
        accepted_daa: p.daa_score,
        rebound_daa: None,
        accepted_blue_score: p.blue_score,
        accepted_block: p.block,
        trace_root: job.trace_root,
        output_root: job.output_root,
        execution_root: job.execution_root,
        trace_chunk_count: job.trace_chunk_count,
        trace_retention_daa: job.trace_retention_daa,
        reserved: 1,
        immature_contribution: 0,
        escrowed_reward: 0,
        work_leaves: job.work_leaves,
        work_id: None,
        phase: PalwClaimPhaseV2::Provisional,
        rights_reserved: 0,
        job_identity: h(20_000),
        rcore: PalwClaimRcoreV1::default(),
    }
}
fn object(id: Hash64, c: &PalwClaimStateV2, job: &PalwFreePromptCommitmentV3, prefix: PalwFpPrefixStateV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::FreePromptCommitted {
        claim: id,
        class_id: c.class_id,
        bond: c.bond,
        executor_pubkey: job.job.executor_pubkey.clone(),
        work_leaves: c.work_leaves,
        prompt_token_ids_hash: job.job.prompt_token_ids_hash,
        prompt_tokens: job.job.prompt_tokens,
        prompt_token_ids: vec![],
        decode_tokens_executed: job.decode_tokens_executed,
        trace_root: c.trace_root,
        output_root: c.output_root,
        execution_root: c.execution_root,
        trace_chunk_count: c.trace_chunk_count,
        trace_retention_daa: c.trace_retention_daa,
        consumed_prefix_state: prefix,
        job_pin: c.job_identity,
        public_job: None,
        eval: None,
    }
}
fn install_birth(
    state: &mut PalwChainStateV2,
    params: &PalwStateParamsV2,
    job: &PalwFreePromptCommitmentV3,
    c: &PalwClaimStateV2,
    edge: PalwPrefixDependencyEnvelopeV1,
) -> Hash64 {
    let id = crate::palw_freeprompt_v3::fp_claim_id_v3(job);
    let p = PalwBlockContextV2 { block: c.accepted_block, daa_score: c.accepted_daa, blue_score: c.accepted_blue_score, subsidy: 0 };
    let prior = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    let mut current = prior.clone();
    current.last_point = Some(p);
    current.claims.insert(id, c.clone());
    let descriptor = edge.consumed.as_ref().map_or(PalwFpPrefixStateV1::genesis(c.class_id), |s| s.intent.prefix);
    let frame = PalwClaimLiabilityFrameV1 {
        point: p,
        network_identity: [11; 32],
        fired_forks: [12; 32],
        policy: PalwClaimLocalPolicyContextV1::from_rules_v1(params, p.daa_score),
        claims: [id].into_iter().collect(),
        native_changes: vec![PalwClaimLocalNativeChangeV1 {
            owner: PalwClaimLocalRowOwnerV1::ChainClaims,
            table: 0,
            key: id.as_byte_slice().to_vec(),
            old: None,
            new: Some(borsh::to_vec(c).unwrap()),
        }],
        prior_state: borsh::to_vec(&prior).unwrap(),
        current_state: borsh::to_vec(&current).unwrap(),
        accepted_objects: vec![object(id, c, job, descriptor)],
        origin_headers: Default::default(),
        inputs: {
            let mut inputs = vec![PalwClaimLocalInputV1::FreePromptNative {
                claim: id,
                job_pin: c.job_identity,
                public_job: Some(Box::new(job.clone())),
                eval: None,
            }];
            // Independent B has no prefix input, rather than an invalid empty
            // provenance envelope. prefix_use authenticates any present wrapper.
            if !edge.exports.is_empty() || edge.consumed.is_some() {
                inputs.push(PalwClaimLocalInputV1::PrefixDependency { claim: id, envelope: Box::new(edge) });
            }
            inputs
        },
    };
    frame.validate_context_v1().unwrap();
    state
        .kernel_route
        .as_mut()
        .unwrap()
        .aux
        .insert((PALW_CLAIM_LOCAL_TABLE_V1, palw_claim_liability_frame_key_v1(p.block).to_vec()), borsh::to_vec(&frame).unwrap());
    id
}
#[test]
fn rfc40_prefix_readiness_retired_s1_source_holds_exact_e_before_inclusive_h_and_preserves_denial() {
    let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(2), 4, 1000, 100, 1000, 0)
        .unwrap()
        .with_claim_local_settlement_from_daa(Some(0));
    let keys = dsa::generate_key_pair([91; 32]);
    let pk = keys.verification_key.as_ref().to_vec();
    let native = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(2, |j| j.executor_pubkey = pk.clone(), None);
    let source_job = native.frame.original_v1().clone();
    let a = crate::palw_freeprompt_v3::fp_claim_id_v3(&source_job);
    let a_born = claim(&source_job, point(10));
    let horizon = source_job.trace_retention_daa;
    assert!(horizon > 30);
    let mut state = PalwChainStateV2::genesis();
    state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h(11), h(12)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 30, burned: 0 },
    ));
    state.last_point = Some(point(30));
    let export = PalwPrefixExportIntentV1 {
        version: 1,
        network: source_job.job.network_domain,
        source_namespace: 1,
        source_nonce: h(91),
        base_commitment: a,
        producer_key_id: crate::mldsa87_primitives::mldsa87_key_id(&pk),
        ordinal: 0,
        prefix: PalwFpPrefixStateV1 { class_id: a_born.class_id, state_root: h(92), prefix_tokens: 2 },
        liability_until: horizon - 1,
        material_until: horizon,
    };
    let signed = PalwSignedPrefixExportV1 {
        signature: dsa::sign(&keys.signing_key, export.id_v1().as_byte_slice(), PREFIX_EXPORT_CONTEXT_V1, [92; 32])
            .unwrap()
            .as_ref()
            .to_vec(),
        intent: export.clone(),
    };
    install_birth(
        &mut state,
        &params,
        &source_job,
        &a_born,
        PalwPrefixDependencyEnvelopeV1 { inner: vec![1], exports: vec![signed], consumed: None },
    );
    let identity =
        state.claim_local_admission_context_v1(&params, &point(10), a).unwrap().unwrap().prefix_export_v1(0).unwrap().unwrap().1;
    let consumer_native = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(
        2,
        |j| {
            j.executor_pubkey = pk.clone();
            j.job_nonce = [93; 32];
        },
        None,
    );
    let consumer_job = consumer_native.frame.original_v1().clone();
    let c = crate::palw_freeprompt_v3::fp_claim_id_v3(&consumer_job);
    let c_born = claim(&consumer_job, point(20));
    let usage = PalwPrefixUseIntentV1 {
        version: 1,
        network: export.network,
        consumer_nonce: h(93),
        base_commitment: c,
        consumer_key_id: export.producer_key_id,
        source: identity,
        prefix: export.prefix,
        hard_deadline: horizon - 1,
        material_until: horizon,
    };
    let consumed = PalwSignedPrefixUseV1 {
        signature: dsa::sign(&keys.signing_key, usage.id_v1().as_byte_slice(), PREFIX_USE_CONTEXT_V1, [94; 32])
            .unwrap()
            .as_ref()
            .to_vec(),
        intent: usage,
    };
    install_birth(
        &mut state,
        &params,
        &consumer_job,
        &c_born,
        PalwPrefixDependencyEnvelopeV1 { inner: vec![1], exports: vec![], consumed: Some(consumed) },
    );
    let independent_native = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(
        2,
        |j| {
            j.executor_pubkey = pk.clone();
            j.job_nonce = [95; 32];
        },
        None,
    );
    let independent_job = independent_native.frame.original_v1().clone();
    let b_born = claim(&independent_job, point(21));
    assert_ne!(source_job.execution_root, independent_job.execution_root, "B has its own authenticated execution identity");
    let b = install_birth(
        &mut state,
        &params,
        &independent_job,
        &b_born,
        PalwPrefixDependencyEnvelopeV1 { inner: vec![1], exports: vec![], consumed: None },
    );
    // Actual typed reservation reader must find a real native budget claim row;
    // no unchecked E-only row or later raw claim's liability substitutes for it.
    use crate::palw_bond_budget_v1::*;
    let budget_id = reservations::reservation_id(&point(10), a);
    let policy = PalwBondBudgetPolicyV1::unapproved_probe_v1();
    let mut budget = PalwBondBudgetStateV1::new(0, &policy);
    budget
        .reserve_liable(
            &policy,
            1_000_000,
            budget_id,
            a_born.bond,
            None,
            10,
            PalwBudgetVectorV1 { claims: 1, ..PalwBudgetVectorV1::ZERO },
            1,
            PalwBudgetOriginV1::FreePrompt,
            true,
            &mut Vec::new(),
        )
        .unwrap();
    state.bond_budget = Some(budget);
    let mut reservation = PalwPrefixExportReservationV1 {
        source_claim: a,
        point: point(10),
        bond: a_born.bond,
        budget_id,
        liability_until: horizon - 1,
        material_until: horizon,
        liability_sompi: 1,
        convicted_daa: None,
    };
    let ekey = reservations::row_key(budget_id).to_vec();
    state.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, ekey.clone()), borsh::to_vec(&reservation).unwrap());
    // Terminal/permitted S1 is a retained closed-source seam. It intentionally
    // causes source() to skip live_source while a different birth occupies raw A.
    let closed = compact::PalwClosedClaimSourceV1 {
        version: 1,
        ruleset: crate::Hash::from_bytes([11; 32]),
        claim: a,
        birth: point(10),
        original: a_born.clone(),
        terminal_phase: PalwClaimPhaseV2::Final { final_daa: 11 },
        closed_at: point(12),
        material_until: horizon,
        winner: None,
        approval: vec![],
        permitted: true,
    };
    let mut skey = vec![b'S'];
    skey.extend_from_slice(a.as_byte_slice());
    state.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, skey), borsh::to_vec(&closed).unwrap());
    let newer = PalwClaimStateV2 { accepted_daa: 30, accepted_blue_score: 30, accepted_block: point(30).block, ..a_born.clone() };
    state.claims.insert(a, newer);
    assert_eq!(compact::recorded(&state, a, &a_born).unwrap(), Some(RewardReadinessV1::Ready));
    assert_eq!(
        source(&state, &params, horizon, a, &a_born, c, &c_born, false).unwrap(),
        RewardReadinessV1::Hold(horizon),
        "all-source wrapper also guards generic carrier readers of retired compact sources"
    );
    assert_eq!(
        super::super::exact_export_readiness_v1(&state, &params, horizon, a, &a_born).unwrap(),
        RewardReadinessV1::Hold(horizon),
        "Closed(A5) direct consumers must join this same exact-born guard"
    );
    for now in [horizon - 1, horizon] {
        assert_eq!(readiness(&state, &params, now, c, &c_born, c, &c_born, false).unwrap(), RewardReadinessV1::Hold(horizon));
    }
    assert_eq!(readiness(&state, &params, horizon + 1, c, &c_born, c, &c_born, false).unwrap(), RewardReadinessV1::Ready);
    assert_eq!(readiness(&state, &params, horizon, b, &b_born, b, &b_born, false).unwrap(), RewardReadinessV1::Ready);
    let unconvicted = state.clone();
    reservation.convicted_daa = Some(horizon - 1);
    state.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, ekey), borsh::to_vec(&reservation).unwrap());
    for now in [horizon - 1, horizon, horizon + 1] {
        assert_eq!(readiness(&state, &params, now, c, &c_born, c, &c_born, false).unwrap(), RewardReadinessV1::Denied);
        assert_eq!(readiness(&state, &params, now, b, &b_born, b, &b_born, false).unwrap(), RewardReadinessV1::Ready);
    }
    // Snapshot rollback seam only; actual reorg/delta/cold restoration remains
    // a separate normal node test, not a result claimed by this regression.
    state = unconvicted;
    assert_eq!(readiness(&state, &params, horizon, c, &c_born, c, &c_born, false).unwrap(), RewardReadinessV1::Hold(horizon));
    // Armed-era scope without retained F1 or strict A5 is unknown, using the
    // legitimate original FP accepting point (no fabricated RED FP tuple).
    let mut missing = state.clone();
    missing
        .kernel_route
        .as_mut()
        .unwrap()
        .aux
        .remove(&(PALW_CLAIM_LOCAL_TABLE_V1, palw_claim_liability_frame_key_v1(point(10).block).to_vec()));
    assert_eq!(
        super::super::exact_export_readiness_v1(&missing, &params, horizon, a, &a_born).unwrap(),
        RewardReadinessV1::Hold(u64::MAX)
    );
    // A source born before this activation could not have admitted E. Its
    // independent legacy S1 compatibility does not require a new F1 archive.
    let legacy_params = params.clone().with_claim_local_settlement_from_daa(Some(20));
    assert_eq!(
        super::super::exact_export_readiness_v1(&missing, &legacy_params, horizon, a, &a_born).unwrap(),
        RewardReadinessV1::Ready
    );
    let mut legacy = state.clone();
    legacy
        .kernel_route
        .as_mut()
        .unwrap()
        .aux
        .remove(&(PALW_CLAIM_LOCAL_TABLE_V1, palw_claim_liability_frame_key_v1(point(10).block).to_vec()));
    legacy.kernel_route.as_mut().unwrap().aux.remove(&(PALW_CLAIM_LOCAL_TABLE_V1, reservations::row_key(budget_id).to_vec()));
    assert_eq!(source(&legacy, &legacy_params, horizon, a, &a_born, c, &c_born, false).unwrap(), RewardReadinessV1::Ready);
    // An already authenticated compact denial stays Denied without hydrating
    // F1 and without needing to guess whether an armed E could be present.
    let mut denied = closed.clone();
    denied.permitted = false;
    let mut skey = vec![b'S'];
    skey.extend_from_slice(a.as_byte_slice());
    legacy.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, skey), borsh::to_vec(&denied).unwrap());
    assert_eq!(source(&legacy, &params, horizon, a, &a_born, c, &c_born, false).unwrap(), RewardReadinessV1::Denied);
}
