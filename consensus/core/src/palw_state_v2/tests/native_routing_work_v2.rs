//! Actual prospective factory over synthetic native rows, not chain/campaign evidence.
#![allow(warnings)]
use super::*;
use crate::palw_fp_execution_v3::*;
use crate::palw_freeprompt_v3::*;
use crate::palw_native_resource_v1::NativeRetainedV1;
use crate::palw_reader_router_v2::{PALW_READER_ROUTER_TABLE_V2, PalwNativeRoutingWorkAuthorityV2 as Owner, PalwReaderRouterRowV2};
use std::sync::Arc;

#[test]
fn native_fp_factory_refresh_allows_tip_progress_only_for_same_eligible_work() {
    let mut params = crate::config::params::TESTNET_PARAMS;
    params.palw_memory_routing_court_v2 = Some(crate::config::params::ForkActivation::new(1));
    let net = b"native-routing-test";
    let network = crate::palw_attempt_v2::palw_network_domain_v2_for(net, Some(params.genesis.hash));
    let (profile, declaration) = crate::palw_reader_router_v2::tests::profile_and_mapping(false);
    let class = profile.shape_profile_id();
    let executor = bond_key(1);
    let anchor = crate::header::Header::from_precomputed_hash(h64(101), vec![]);
    let mut fp = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: network,
        class_id: class,
        executor_bond: executor.0,
        executor_pubkey: vec![7; 4],
        operator_id: h64(8),
        anchor_block: anchor.hash,
        anchor_daa: anchor.daa_score,
        job_nonce: [1; 32],
        tokenizer_id: h64(9),
        prompt_token_ids_hash: h64(10),
        prompt_tokens: 2,
        decode_token_limit: 4,
        max_context_tokens: 64,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    };
    let canonical = crate::palw_base0_profile::rc_job_context(&profile, 2, 4);
    let facts = PalwFpClassFactsV3 {
        model_profile_id: canonical.model_profile_id,
        runtime_manifest_hash: canonical.runtime_manifest_hash,
        runtime_class_id: canonical.runtime_class_id,
        shape_profile_id: class,
        cu_ruleset_id: canonical.cu_ruleset_id,
    };
    let job = palw_fp_job_context_v3(&fp, &facts, &palw_fp_run_facts_for_executed_v1(&fp, 4), net).unwrap();
    let mut state = PalwChainStateV2::genesis();
    state.bonds.insert(
        executor,
        PalwBondStateV2 {
            pubkey: fp.executor_pubkey.clone(),
            operator_id: fp.operator_id,
            collateral: 100,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 0,
            payout_payload: h64(7),
            capable_classes: Default::default(),
        },
    );
    state.classes.insert(
        class,
        PalwClassStateV2 {
            artifact_root: declaration.artifact_root,
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1),
            status: PalwClassStatusV2::Active,
            registered_daa: 0,
            registrant_bond: Some(executor),
            fused_attention: false,
        },
    );
    state.fp_work_profiles.insert(class, Box::new(profile));
    state.class_semantics.insert(
        class,
        crate::palw_class_semantics_v1::PalwClassSemanticsV1::of_registration(class, declaration.artifact_root, &job, 0),
    );
    let mut route = crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(network, h64(3)),
        None,
        Default::default(),
    );
    route.aux.insert(
        (PALW_READER_ROUTER_TABLE_V2, class.as_byte_slice().to_vec()),
        borsh::to_vec(&PalwReaderRouterRowV2 { declaration, registrant: executor, bound_from_daa: 1 }).unwrap(),
    );
    state.kernel_route = Some(route);
    let make = |source: &PalwChainStateV2, word, daa, epoch, fp: &PalwFreePromptJobV3| {
        let mut source = source.clone();
        let mut header = crate::header::Header::from_precomputed_hash(h64(word), vec![]);
        header.daa_score = daa;
        source.last_point = Some(PalwBlockContextV2 { block: header.hash, daa_score: daa, blue_score: daa, subsidy: 0 });
        Owner::from_native_fp_v2(
            Arc::new(NativeRetainedV1::try_from_clone_v1(&source).unwrap()),
            &params,
            &header,
            &anchor,
            daa + 1,
            fp,
            &job,
            net,
            epoch,
        )
    };
    let original = make(&state, 201, 10, 4, &fp).unwrap().unwrap();
    let advanced = make(&state, 202, 11, 4, &fp).unwrap().unwrap();
    assert!(!original.same_work_v2(&advanced), "actual snapshots differ");
    assert!(original.same_work_identity_v2(&advanced), "normal forward progress preserves exact work");
    assert!(!original.same_work_identity_v2(&make(&state, 203, 12, 5, &fp).unwrap().unwrap()), "import never crosses");
    assert!(!advanced.same_work_identity_v2(&original), "clock cannot move backwards");
    fp.job_nonce[0] ^= 1;
    assert!(make(&state, 204, 12, 4, &fp).is_err(), "descriptor substitution fails actual context reconstruction");
    let mut frozen = state.clone();
    frozen.classes.get_mut(&class).unwrap().status = PalwClassStatusV2::Frozen { since_daa: 11 };
    fp.job_nonce[0] ^= 1;
    assert!(make(&frozen, 205, 12, 4, &fp).is_err(), "current native eligibility is rechecked");
}
