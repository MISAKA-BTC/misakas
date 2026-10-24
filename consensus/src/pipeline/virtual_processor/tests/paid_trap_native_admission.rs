//! Real native admission/walk/rehearsal boundaries, not an activated campaign.
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use crate::pipeline::virtual_processor::processor::PalwLocalTransitionFaultV1;
use kaspa_consensus_core::{
    config::{Config, params::ForkActivation},
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_route_policy_v1},
    palw_mesh_v1::PalwTrapRevealedV1,
    palw_mode_v2::PalwConsensusMode,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeHeapV1, NativeRetainedV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1,
    },
    palw_producer_v2::PalwObjectRehearsalV1 as Rehearsal,
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwStateCarriageV2, PalwStateV2Error},
};
use kaspa_hashes::Hash64;

#[tokio::test]
async fn paid_trap_native_admission_capacity_is_retry_not_dropped_semantic_object() {
    const MODE: &str = "MISAKA_TRAP_NATIVE_ADMISSION_MODE";
    let Ok(mode) = std::env::var(MODE) else {
        let name = format!("{}::paid_trap_native_admission_capacity_is_retry_not_dropped_semantic_object", module_path!());
        let name = name.split_once("::").unwrap().1;
        for mode in ["zero", "walk", "ample"] {
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", name, "--nocapture", "--test-threads=1"])
                    .env(MODE, mode)
                    .status()
                    .unwrap()
                    .success(),
                "child {mode}"
            );
        }
        return;
    };
    let (config, _, premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    // An isolated private mechanics world. Disable pre-object sweeps to target
    // the exact admission lookup after its independently proven retained copy.
    params.palw_audit_2026_09_11 = None;
    params.palw_audit_mesh_v1 = Some(ForkActivation::always());
    params.sync_palw_audit_mesh_v1();
    let config = Config::new(params);
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    let chain = t12_genesis_chain(&config, bundle, &premine, &floats);
    let (block, state) = chain.tip_state();
    let vp = chain.vp();
    let sp = bundle.state.clone().with_paid_claim_trap_from(Some(0)).with_audit_mesh_from_daa(Some(0));
    // Empty route: a genuine absent paid base has the ordinary mesh semantics.
    // No paid authority or model execution is invented by this fixture.
    let mut carriage = PalwStateCarriageV2::from_state(&state);
    carriage.kernel_route = Some(PalwKernelRouteStateV1::new(
        palw_kernel_route_policy_v1(Hash64::from_u64_word(7), Hash64::from_u64_word(8)),
        None,
        Default::default(),
    ));
    // Local synthetic fixture only: checked restoration rebuilds every index
    // and refuses inconsistent rows. No peer state/root is being authenticated.
    let state = carriage.into_state_v3(&sp, None, false, config.params.palw_canonical_work_daa()).unwrap();
    let point = PalwBlockContextV2 { block, daa_score: 10, blue_score: 1, subsidy: 0 };
    let object = Obj::TrapRevealedV1 {
        reveal: Box::new(PalwTrapRevealedV1 {
            setter_bond: chain.bonds[2],
            claim: Hash64::from_u64_word(13),
            fault_leaf: 0,
            tiles: 1,
            salt: [7; 32],
            signature: vec![0; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }),
    };
    let root = state.state_root();
    let clone_bytes = state
        .native_heap_v1("native retained payload")
        .add::<PalwChainStateV2>(
            NativeCoverageV1::Bounded {
                bytes: (std::mem::size_of::<NativeRetainedV1<PalwChainStateV2>>() + std::mem::size_of::<[usize; 3]>()) as u64,
            },
            "native retained Arc/owner",
        )
        .bytes()
        .unwrap();
    let limit = match mode.as_str() {
        "zero" => 0,
        "walk" => clone_bytes.checked_add(64).unwrap(),
        "ample" => 1 << 30,
        _ => panic!("mode"),
    };
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: limit, database_bytes: u64::MAX }));
    if mode == "zero" {
        assert!(vp.palw_v2_validate_objects_classified_v1(&state, &sp, &point, std::slice::from_ref(&object)).is_err());
        assert!(matches!(
            vp.palw_object_rehearsal_v1_on(&state, &sp, &point, &object),
            Rehearsal::Refused(PalwStateV2Error::NativeResource(_))
        ));
        let mut malformed = object.clone();
        let Obj::TrapRevealedV1 { reveal } = &mut malformed else { unreachable!() };
        reveal.tiles = 0;
        assert!(
            matches!(vp.palw_object_rehearsal_v1_on(&state, &sp, &point, &malformed), Rehearsal::NotAccepted(_)),
            "stateless semantic faults are independent of local capacity"
        );
        let dormant = bundle.state.clone().with_paid_claim_trap_from(None).with_audit_mesh_from_daa(Some(0));
        assert!(
            matches!(vp.palw_object_rehearsal_v1_on(&state, &dormant, &point, &object), Rehearsal::NotAccepted(_)),
            "dormant paid lookup does not request a key"
        );
    } else if mode == "walk" {
        // This proves the budget admits the earlier copy and refuses only the
        // subsequent 65-byte key while that copy remains alive. Without the
        // typed walk boundary this failure becomes Ok(0), silently dropping it.
        let copy = NativeRetainedV1::try_from_clone_v1(&state).unwrap();
        assert!(matches!(
            copy.paid_trap_target_checked_v1(&sp, &Hash64::from_u64_word(13), 10),
            Err(PalwStateV2Error::NativeResource(_))
        ));
        drop(copy);
        assert!(matches!(
            vp.palw_v2_accept_classified_for_tests(&state, &sp, &point, vec![object]),
            Err(PalwLocalTransitionFaultV1::Resource(_))
        ));
    } else {
        let why = vp.palw_v2_validate_objects_classified_v1(&state, &sp, &point, std::slice::from_ref(&object)).unwrap_err();
        assert!(why.to_string().contains("opens no commitment"));
        assert!(matches!(vp.palw_object_rehearsal_v1_on(&state, &sp, &point, &object), Rehearsal::NotAccepted(_)));
        assert_eq!(vp.palw_v2_accept_classified_for_tests(&state, &sp, &point, vec![object]).unwrap(), 0);
    }
    assert_eq!(state.state_root(), root);
}
