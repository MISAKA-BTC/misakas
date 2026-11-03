//! SDK construction/preflight only. Native carrier acceptance is tested separately.
use kaspa_consensus_core::{
    Hash64,
    config::params::{ForkActivation, Params, palw_t12_shipped_params},
    palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2},
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwRegistrationTermsV2},
    palw_tir_admission_v1::palw_tir_post_genesis_registration_v1,
    palw_tir_class_v1::{PalwTirLayoutV1, palw_tir_class_registration_message_v1},
    palw_tir_v1::PalwTirFenceV1,
};
use misaka_palw_sdk::{
    PalwTirClassEntryV1,
    lineages::tir::TirLineageV1,
    tir_registration::{build_tir_registration_v1, tir_registration_message_v1, tir_registration_preflight_v1},
};
use misaka_palw_tir::{
    DType, Ref, TensorType,
    builder::ProgramBuilder,
    program::{HISTORY_BOUND_V1_SMALL, LOGITS_I64_Q24_SCHEME_V2},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

const AT: u64 = 1000;
struct Fixture {
    entry: PalwTirClassEntryV1,
    dir: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn fixture(dtype: DType) -> Fixture {
    let mut p = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let embed = p.param("embed", DType::I8, &[4, 4], false);
    let mut pre = p.block("pre", vec![]);
    let row = pre.gather(embed, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[row]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let bounded = post.clamp(Ref::CarryIn(0), 0, 63, DType::I32);
    post.commit(bounded);
    let out = if dtype == DType::I32 {
        bounded
    } else {
        let x = post.cast(bounded, dtype);
        post.commit(x);
        x
    };
    let Ref::Node(logits) = out else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![], post, logits);
    program.logits_scheme_id = if dtype == DType::I64 {
        LOGITS_I64_Q24_SCHEME_V2
    } else {
        kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_bytes()
    };
    misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
    let layout = PalwTirLayoutV1 {
        version: 1,
        max_context: 64,
        checkpoint_interval: 2,
        h_tile: 2,
        commit_tiles: vec![4; program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).count()],
        state_tiles: vec![],
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("palw-sdk-dispatch-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("class.palwtir");
    misaka_palw_tir_artifact::write_container_v1(
        &path,
        &program,
        borsh::to_vec(&layout).unwrap(),
        [18; 64],
        "{\"model_id\":\"dispatch-fixture\"}".into(),
        &mut |j, layer| {
            assert_eq!((j, layer), (0, None));
            Ok((0..16).map(|n| n as u8).collect())
        },
    )
    .unwrap();
    Fixture { entry: TirLineageV1::open_entry(&path).unwrap(), dir }
}
fn params(version: u16) -> Params {
    let mut params = palw_t12_shipped_params();
    let mut fence = PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT));
    fence.court_version = version;
    params.palw_tir_v1 = Some(fence);
    params.sync_palw_tir_v1();
    params
}
fn bundle(params: &Params) -> &PalwConsensusParamsV2 {
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { unreachable!() };
    bundle
}
fn terms() -> PalwRegistrationTermsV2 {
    PalwRegistrationTermsV2 {
        min_grantable_share_permille: 0,
        slash_value_per_pwu: 1,
        initial_target: 1 << 100,
        registered_class_ids: vec![],
        registered_artifact_roots: vec![],
        chain_certified_families: vec![],
    }
}
fn bond() -> PalwBondKeyV2 {
    PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
        transaction_id: kaspa_consensus_core::tx::TransactionId::from_bytes([7; 64]),
        index: 0,
    })
}
fn build(params: &Params, entry: &PalwTirClassEntryV1, daa: u64) -> Result<PalwConsensusObjectV2, String> {
    build_tir_registration_v1(params, bundle(params), entry, &terms(), AT + 10, bond(), vec![], daa)
}

#[test]
fn existing_sdk_builder_uses_trusted_wide_rules_and_preserves_tag61_message() {
    let fixture = fixture(DType::I64);
    let params = params(2);
    params.validate_palw_v2().unwrap();
    let object = build(&params, &fixture.entry, AT).unwrap();
    assert_eq!(borsh::to_vec(&object).unwrap()[0], 61);
    tir_registration_preflight_v1(&params, bundle(&params), &object, AT, &[]).unwrap();
    let PalwConsensusObjectV2::ClassRegisteredTirV1 {
        class_id,
        artifact_root,
        pwu_rule,
        slash_value_per_pwu,
        initial_target,
        share_permille,
        activation_daa,
        admission,
    } = &object
    else {
        unreachable!()
    };
    assert_eq!(*class_id, fixture.entry.class_id());
    assert_eq!(admission.class, *fixture.entry.class);
    assert_eq!(admission.canonical, fixture.entry.canonical_context_v2(fixture.entry.class.program.len()).unwrap());
    assert_eq!(admission.canonical.tokenizer_id, Hash64::default());
    let domain = Hash64::from_u64_word(23);
    assert_eq!(
        tir_registration_message_v1(domain, &object),
        Some(palw_tir_class_registration_message_v1(
            domain,
            *class_id,
            *share_permille,
            *activation_daa,
            &admission.registrant_bond,
            *artifact_root,
            *slash_value_per_pwu,
            *initial_target,
            pwu_rule,
            &admission.canonical,
            &admission.class
        ))
    );
}

#[test]
fn wide_sdk_does_not_select_from_peer_scheme_or_inactive_unknown_rules() {
    let fixture = fixture(DType::I64);
    for version in [1, 3] {
        assert!(build(&params(version), &fixture.entry, AT).is_err());
    }
    assert!(build(&params(2), &fixture.entry, AT - 1).is_err());
    let mut dormant = params(2);
    dormant.palw_tir_v1 = None;
    dormant.sync_palw_tir_v1();
    assert!(build(&dormant, &fixture.entry, AT).is_err());
    let mut stale = fixture.entry.clone();
    stale.canonical_job.0 += 1;
    assert!(build(&params(2), &stale, AT).is_err());
}

#[test]
fn narrow_sdk_carriers_stay_byte_identical_for_every_legacy_committable_dtype() {
    for dtype in [DType::I8, DType::I16, DType::I32, DType::Idx] {
        let fixture = fixture(dtype);
        let legacy_params = params(1);
        let entry = &fixture.entry;
        let expected = palw_tir_post_genesis_registration_v1(
            entry.class.as_ref().clone(),
            entry.canonical_context(),
            entry.artifact_root,
            0,
            terms().initial_target,
            terms().slash_value_per_pwu,
            AT + 10,
            bond(),
            vec![],
            bundle(&legacy_params).court.max_step_leaf_count(),
        )
        .unwrap();
        let actual = build(&legacy_params, entry, AT).unwrap();
        assert_eq!(borsh::to_vec(&actual).unwrap(), borsh::to_vec(&expected).unwrap());
        let court2 = params(2);
        assert_eq!(borsh::to_vec(&build(&court2, entry, AT).unwrap()).unwrap(), borsh::to_vec(&expected).unwrap());
    }
}
