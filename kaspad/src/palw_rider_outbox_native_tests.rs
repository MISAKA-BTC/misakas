//! Synthetic native rows restored with the real public carriage/factory APIs.
//! No live store, signature approval, or campaign acceptance is claimed.
use super::*;
use kaspa_consensus_core::{config::params::ForkActivation, palw_reader_router_v2::*, palw_state_v2::*, palw_step::*};
fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
pub(crate) fn native_rider_fixture() -> (Arc<Owner>, PalwAttemptEnvelopeV2, kaspa_consensus_core::palw_v2::PalwJobContextV2, Hash64) {
    let mut profile =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .unwrap();
    profile.layer_count = 1;
    profile.full_attention_interval = 0;
    let node = |desc: &str, width, refs: &[u16], name: &str| PalwStepNodeV1 {
        op_kind: PalwStepOpKindV1::MatMulQuant,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: name.into(),
        weight_dtypes: vec![],
        out_len: PalwStepOutLenV1::Fixed { elements: width },
        tile_len: 4,
        kernel_semantics_id: kaspa_consensus_core::palw_step::kernel_semantics_id_v1(desc),
        input_refs: refs.to_vec(),
    };
    use kaspa_consensus_core::palw_step_refute::{
        KDESC_Q36_MATMUL_GROUPED, KDESC_Q36_MOE_COMBINE, KDESC_Q36_ROUTER_TOPK, KDESC_Q36_SIGMOID,
    };
    profile.gdn_nodes = vec![
        node(KDESC_Q36_SIGMOID, 16, &[PALW_STEP_INPUT_LAYER_IN], "score"),
        node(KDESC_Q36_ROUTER_TOPK, 4, &[0], "router"),
        node(KDESC_Q36_MATMUL_GROUPED, 16, &[0], "test.routed"),
        node(KDESC_Q36_MOE_COMBINE, 8, &[2, 1], "combine"),
    ];
    let class = profile.shape_profile_id();
    let artifact = h(77);
    let bond = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(5), 0));
    let state_params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, class, 4, 1000, 100, 1000, 0).unwrap();
    let context = |block, daa| PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: daa, subsidy: 0 };
    let objects = vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: class,
            artifact_root: artifact,
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        PalwConsensusObjectV2::BondRegistered {
            bond,
            pubkey: vec![7],
            operator_pubkey: vec![8],
            collateral: 1000,
            payout_payload: h(9),
            capable_classes: Default::default(),
            signature: vec![],
        },
    ];
    let (state, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params, &context(1, 1), &objects, None).unwrap();
    let mut envelope = rider_fixture_v2(h(0), 0);
    envelope.attempt.version = kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION;
    envelope.attempt.class_id = class;
    envelope.attempt.artifact_root = artifact;
    envelope.attempt.executor_bond = bond.0;
    envelope.attempt.operator_id = palw_operator_id_v2(&[8]);
    envelope.attempt.trace_retention_daa = 100;
    let lead = attempt_id_v2(&envelope.attempt);
    let (state, _) = apply_palw_transition_v2(&state, &state_params, &context(2, 10), &[], Some(&envelope)).unwrap();
    let mut carriage = PalwStateCarriageV2::from_state(&state);
    carriage.classes.get_mut(&class).unwrap().registrant_bond = Some(bond);
    carriage.fp_work_profiles.insert(class, Box::new(profile.clone()));
    let mut params = kaspa_consensus_core::config::params::devnet_shipped_params();
    params.palw_offence_attribution = None;
    params.palw_memory_routing_court_v2 = Some(ForkActivation::new(1));
    params.palw_capacity_multi_claim = Some(ForkActivation::new(1));
    let net = b"rider-owner-test";
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(net, Some(params.genesis.hash));
    let mut route = kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1(domain, h(3)),
        None,
        Default::default(),
    );
    let base = profile.pre_nodes.len() as u32;
    let row = PalwReaderRouterRowV2 {
        declaration: PalwReaderRouterDeclarationV2 {
            version: 2,
            class_id: class,
            artifact_root: artifact,
            bindings: vec![
                PalwReaderRouterBindingV2 { reader_slot: base + 2, router_slot: base + 1 },
                PalwReaderRouterBindingV2 { reader_slot: base + 3, router_slot: base + 1 },
            ],
        },
        registrant: bond,
        bound_from_daa: 1,
    };
    route.aux.insert((PALW_READER_ROUTER_TABLE_V2, class.as_byte_slice().to_vec()), borsh::to_vec(&row).unwrap());
    carriage.kernel_route = Some(route);
    let state = carriage.into_state(&state_params, None).unwrap();
    let mut header = kaspa_consensus_core::header::Header::from_precomputed_hash(h(2), vec![]);
    header.daa_score = 10;
    let mut job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, 1, 1);
    job.network_id = net.to_vec();
    job.job_id = kaspa_consensus_core::palw_capacity_s567_v1::palw_rider_job_anchor_v1(&lead, 0);
    let owner = Owner::from_native_rider_v2(
        Arc::new(NativeRetainedV1::try_from_clone_v1(&state).unwrap()),
        &params,
        &header,
        11,
        &job,
        bond,
        lead,
        0,
        100,
        net,
        4,
    )
    .unwrap()
    .unwrap();
    envelope.attempt.network_domain = domain;
    envelope.attempt.challenge = kaspa_consensus_core::palw_capacity_s567_v1::palw_rider_challenge_v1(&lead, 0);
    (Arc::new(owner), envelope, job, lead)
}
