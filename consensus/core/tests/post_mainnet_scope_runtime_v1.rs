//! RFC-0022: initial-mainnet closure at the public preset and actual fold doors.
//! These tests do not prohibit historical private codecs or arm postponed lanes.
//! Registration shells intentionally stop at the dormant-kind guard, before
//! program admission; a malformed-program error is not accepted as scope closure.
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{MAINNET_PARAMS, Params, mainnet_shipped_params};
use kaspa_consensus_core::palw_gen_class_v1::{
    OutputSpecV1, PalwGenAdmissionCarriageV1, PalwGenClassV1, PalwGenOffersV1, PalwGenProfileOffersV1,
};
use kaspa_consensus_core::palw_gen_v1::PalwGenProfileV1;
use kaspa_consensus_core::palw_improve_state_v1::PalwImprovementPoolFundingV1;
use kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::*;
use kaspa_consensus_core::tx::TransactionOutpoint;

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn point(daa: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(daa), daa_score: daa, blue_score: daa, subsidy: 0 }
}
fn reference_state_params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0).unwrap()
}
fn initial_state_params(p: &Params) -> PalwStateParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.clone(),
        // An unset genesis card leaves mainnet PALW disabled. Exercise its
        // equally dormant verified-half mirrors without manufacturing a card.
        _ => reference_state_params(),
    }
}
fn media_registration(profile: PalwGenProfileV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ClassRegisteredGenV1 {
        class_id: h(50 + profile as u64),
        artifact_root: h(11),
        slash_value_per_pwu: 1,
        pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1),
        initial_target: 1,
        share_permille: 1,
        activation_daa: 0,
        admission: Box::new(PalwGenAdmissionCarriageV1 {
            class: PalwGenClassV1 {
                version: 1,
                profile: profile as u8,
                pipeline: vec![],
                programs: vec![],
                layouts: vec![],
                output: OutputSpecV1 { kind: profile as u8, shape: vec![1, 1, 3], meta: vec![] },
                offers: PalwGenOffersV1 {
                    steps: vec![],
                    scalars: vec![],
                    max_prompt_tokens: 0,
                    max_negative_tokens: 0,
                    images: vec![],
                    max_source_tokens: 0,
                    source_token_floor: 0,
                    forced_prompt_prefix: vec![],
                    profile: PalwGenProfileOffersV1::None,
                },
                tokenizer_id: h(12),
            },
            registrant_bond: PalwBondKeyV2(TransactionOutpoint::new(Default::default(), 1)),
            signature: vec![],
        }),
    }
}
fn improvement_funding() -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ImprovementPoolFunded {
        payload: Box::new(PalwImprovementPoolFundingV1 { line_id: h(30), amount: 1, sink_index: 0 }),
    }
}
fn chunk(object: &PalwConsensusObjectV2) -> PalwConsensusObjectV2 {
    let bytes = borsh::to_vec(object).unwrap();
    PalwConsensusObjectV2::ObjectChunk { group: palw_object_chunk_group_id_v1(&bytes), index: 0, count: 1, bytes }
}

#[test]
fn initial_mainnet_public_presets_keep_postponed_lanes_closed_without_an_automatic_date() {
    for p in [MAINNET_PARAMS, mainnet_shipped_params()] {
        assert!(p.palw_gen_v1.is_none(), "shared media fence requires a separately reviewed initial-release scope");
        assert!(p.palw_fp_job_v5.is_none(), "image-carrying alternate FP ingress is not offered by the initial preset");
        assert!(p.palw_improvement_v1.is_none());
        let mirrors = initial_state_params(&p);
        for daa in [0, 18_400, u64::MAX - 1] {
            assert!(!mirrors.gen_active_at(daa));
            assert!(!mirrors.improve_active_at(daa));
        }
    }
}

#[test]
fn initial_mainnet_media_and_improvement_are_refused_at_direct_and_chunked_fold_doors() {
    for p in [MAINNET_PARAMS, mainnet_shipped_params()] {
        // Keep the common TIR path available: closure must not depend on turning
        // off all text/IR support together with the postponed media lane.
        let mirrors = initial_state_params(&p).with_tir_from_daa(Some(0));
        for object in [media_registration(PalwGenProfileV1::Image), media_registration(PalwGenProfileV1::Video), improvement_funding()]
        {
            let media = matches!(object, PalwConsensusObjectV2::ClassRegisteredGenV1 { .. });
            for (chunked, carried) in [(false, object.clone()), (true, chunk(&object))] {
                let parent = PalwChainStateV2::genesis();
                let root = parent.state_root();
                let error = apply_palw_transition_v2(&parent, &mirrors, &point(100), &[carried], None).unwrap_err();
                match (chunked, media, error) {
                    (false, true, PalwStateV2Error::GenObjectRefused(why)) => assert!(why.contains("before palw_gen_v1")),
                    (false, false, PalwStateV2Error::ImprovementObjectRefused { why, .. }) => {
                        assert!(why.contains("before palw_improvement_v1"))
                    }
                    (true, _, PalwStateV2Error::ChunkedObjectUndecodable(_) | PalwStateV2Error::ChunkedObjectKindNotAllowed) => {}
                    (_, _, other) => panic!("a different refusal does not prove scope closure: {other:?}"),
                }
                assert_eq!(parent.state_root(), root);
            }
        }
    }
}

#[test]
fn postponed_scope_keeps_text_registry_and_claim_authentication_doors_separate() {
    let p = reference_state_params().with_tir_from_daa(Some(0));
    let text = PalwConsensusObjectV2::ClassRegistered {
        class_id: h(1),
        artifact_root: h(11),
        slash_value_per_pwu: 1,
        pwu_rule: PalwPwuRuleV2::MaxPerAttempt(10),
        initial_target: 1,
        share_permille: 1000,
        activation_daa: 0,
        admission: None,
    };
    // Verified-half genesis registry operation, not post-genesis model approval.
    let (state, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &p, &point(1), &[text], None).unwrap();
    assert!(state.class(&h(1)).is_some());
    assert!(!p.gen_active_at(1) && !p.improve_active_at(1));
    // Receipt decoding stays available. It grants no authority by itself: the
    // native claim/approval gate still refuses a licence without a claim.
    let unauthenticated = PalwConsensusObjectV2::ReceiptLicensed { claim: h(90), receipts: vec![] };
    assert!(palw_lifecycle_object_may_ride_v2(&unauthenticated).is_ok());
    let root = state.state_root();
    assert!(apply_palw_transition_v2(&state, &p, &point(2), &[unauthenticated], None).is_err());
    assert_eq!(state.state_root(), root);
}

/// Exercise the actual canonical carriage reader used by cold/import paths,
/// then repeat the ordinary guards. This is not a physical DB or TCP campaign.
#[test]
fn postponed_scope_remains_closed_after_canonical_state_restore_and_empty_tick() {
    for network in [MAINNET_PARAMS, mainnet_shipped_params()] {
        let params = initial_state_params(&network).with_tir_from_daa(Some(0));
        let text_class = params.base_class_id();
        let text = PalwConsensusObjectV2::ClassRegistered {
            class_id: text_class,
            artifact_root: h(11),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(10),
            initial_target: 1,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        };
        let (accepted, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &params, &point(1), &[text], None).unwrap();
        let expected = accepted.state_root();
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&accepted)).unwrap();
        let decoded: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
        let restored = decoded.into_state_v3(&params, Some(expected), false, None).unwrap();
        assert_eq!(restored.state_root(), expected);
        assert!(restored.class(&text_class).is_some(), "restore preserves the ordinary registry");
        assert!(params.tir_active_at(18_400));
        assert!(!params.gen_active_at(18_400) && !params.improve_active_at(18_400));
        for object in [media_registration(PalwGenProfileV1::Image), media_registration(PalwGenProfileV1::Video), improvement_funding()]
        {
            let media = matches!(object, PalwConsensusObjectV2::ClassRegisteredGenV1 { .. });
            for (chunked, carried) in [(false, object.clone()), (true, chunk(&object))] {
                let error = apply_palw_transition_v2(&restored, &params, &point(18_400), &[carried], None).unwrap_err();
                match (chunked, media, error) {
                    (false, true, PalwStateV2Error::GenObjectRefused(why)) => assert!(why.contains("before palw_gen_v1")),
                    (false, false, PalwStateV2Error::ImprovementObjectRefused { why, .. }) => {
                        assert!(why.contains("before palw_improvement_v1"))
                    }
                    (true, _, PalwStateV2Error::ChunkedObjectUndecodable(_) | PalwStateV2Error::ChunkedObjectKindNotAllowed) => {}
                    (_, _, other) => panic!("restore must preserve scope refusal, not another failure: {other:?}"),
                }
                assert_eq!(restored.state_root(), expected, "refused calls publish no mutation");
            }
        }
        let (ticked, _) = apply_palw_transition_v2(&restored, &params, &point(18_400), &[], None).unwrap();
        let after = PalwStateCarriageV2::from_state(&ticked);
        assert!(after.gen_classes.is_empty());
        assert!(after.improvement_lines.is_empty() && after.improvement_epochs.is_empty());
        assert!(after.improvement_candidates.is_empty() && after.improvement_eval_jobs.is_empty());
        assert!(after.improvement_pools.is_empty() && after.improvement_pool_entries.is_empty());
        assert!(after.improvement_grants.is_empty() && after.improvement_earnings.is_empty());
        assert!(after.claims.is_empty() && after.pending_payouts.is_empty(), "empty tick creates no postponed credit or payout");
        let root = ticked.state_root();
        let again: PalwStateCarriageV2 = borsh::from_slice(&borsh::to_vec(&after).unwrap()).unwrap();
        assert_eq!(again.into_state_v3(&params, Some(root), false, None).unwrap().state_root(), root);
    }
}
