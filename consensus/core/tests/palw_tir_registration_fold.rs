//! **RFC-0002 Phase F, step F6 C(1): an IR class registration folds** — through the legacy
//! registration's own body, plus the `tir_classes` row — on testnet-12's fold with `palw_tir_v1`
//! armed; and below the fence it is refused by name.
//!
//! Every block is checked by the shared chain harness three ways: the delta re-applies and reverts,
//! and the child's carriage (with the new `0xC0` tail) reloads under its committed root.

#[path = "rcore_common.rs"]
mod rcore;
use rcore::*;

#[path = "palw_tir_fixture_common.rs"]
#[allow(unused)]
mod fixture;

use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, artifact_root_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwStateV2Error, palw_class_registration_buyer_v1, palw_object_is_tir_v1,
};
use kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1;
use kaspa_consensus_core::palw_tir_admission_v1::{palw_tir_class_record_v1, palw_tir_post_genesis_registration_v1};
use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_operands_v1;
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;

const AT: u64 = 1_100;

fn armed() -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
    p.sync_palw_tir_v1();
    p
}

/// The corpus's `moe-top2-shared` model as an IR class at 64 positions, and its inventory root.
fn class() -> (PalwTirClassV1, Hash64) {
    class_with(0x70)
}

/// [`class`] under tokenizer `[tokenizer; 64]` — another class id for the same program.
fn class_with(tokenizer: u8) -> (PalwTirClassV1, Hash64) {
    let (_, mut program, params, _) = fixture::programs().into_iter().find(|(n, ..)| n == "moe-top2-shared").expect("the MoE model");
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let ops = palw_tir_inventory_operands_v1(&program, &fixture::TensorSrc(&params)).expect("the inventory");
    let root = artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("a root");
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: fixture::layout(&program, 64),
        tokenizer_id: Hash64::from_bytes([tokenizer; 64]),
    };
    (class, root)
}

/// The registration a registrant makes: the chain's target and slash value, weightless, active now.
fn registration(chain: &Chain, registrant: PalwBondKeyV2, daa: u64) -> PalwConsensusObjectV2 {
    registration_of(chain, registrant, daa, class())
}

/// [`registration`] of a given class.
fn registration_of(
    chain: &Chain,
    registrant: PalwBondKeyV2,
    daa: u64,
    (class, root): (PalwTirClassV1, Hash64),
) -> PalwConsensusObjectV2 {
    let facts = PalwTirJobFactsV1::of_class(&class, class.class_id(&root)).expect("decodes");
    let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&class).expect("wide enough"));
    let (floor, _, target, slash) = genesis_classes(&chain.p)[0];
    let target = chain.s.class_target(&floor).map(|t| t.target).unwrap_or(target);
    palw_tir_post_genesis_registration_v1(
        class,
        canonical,
        root,
        0,
        target,
        slash,
        daa,
        registrant,
        vec![9; 16],
        bundle(&chain.p).court.max_step_leaf_count(),
    )
    .expect("the builder counts the canonical job")
}

#[test]
fn an_ir_registration_folds_past_the_fence_and_is_refused_below_it() {
    let mut chain = Chain::new(armed());
    chain.room = true;
    let (registrant, _, _) = floor_producer(&chain.p);
    let object = registration(&chain, registrant, AT);
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, artifact_root, admission, .. } = &object else { unreachable!() };
    let class_id = *class_id;
    assert!(palw_object_is_tir_v1(&object));
    assert_eq!(palw_class_registration_buyer_v1(&object), Some(registrant), "a bought registration: a slot and the burn");

    // Below the fence: refused by name, the second lock behind the acceptance walk's drop.
    let below = chain.try_fold(
        &chain.s,
        &ctx(0xCA_0000 + AT - 1, AT - 1, AT - 1, 0),
        &[object.clone()],
        PalwBlockWorkV3::None,
        Hash64::default(),
    );
    assert!(matches!(below, Err(PalwStateV2Error::TirRegistrationRefused(_))), "{below:?}");

    // At the fence: it folds (the harness re-applies, reverts and reloads the block).
    let collateral = chain.s.bond(&registrant).expect("the registrant").collateral;
    let root_before = chain.s.state_root();
    chain.step_at(AT, &[object.clone()], PalwBlockWorkV3::None, Hash64::default(), 0);
    let (record, _) = palw_tir_class_record_v1(&admission.class, artifact_root).expect("decodes");
    assert_eq!(chain.s.tir_class_v1(&class_id), Some(&record), "the record, derived as admission derives it");
    assert_eq!(record.facts.class_id, class_id);
    let class_state = chain.s.class(&class_id).expect("the class");
    assert_eq!(class_state.registrant_bond, Some(registrant));
    assert!(!class_state.fused_attention, "no dissected commit point is admitted before F7");
    assert!(chain.s.bond(&registrant).expect("the registrant").collateral < collateral, "the registration burn was taken");
    assert_ne!(chain.s.state_root(), root_before);
    if let Some(row) = chain.s.model_lifecycle(&class_id) {
        eprintln!("lifecycle: {:?}, work {:?}", row.state, row.work);
        assert!(row.work.ops_supported && row.work.verification_ccu > 0, "the registry work is the IR program's");
    }

    // The chain holds the program (every IR object references it by class). The root commits it
    // through `graph_ir_root`, not byte by byte; a carriage whose program does not hash to its row's
    // root is refused at load.
    assert_eq!(record.program.as_slice(), admission.class.program.as_slice(), "the chain holds the program");
    let empty = kaspa_consensus_core::palw_tir_admission_v1::PalwTirClassRecordV1 {
        program: std::sync::Arc::new(Vec::new()),
        ..record.clone()
    };
    assert_eq!(record.rooted_bytes_v1(), borsh::to_vec(&empty).unwrap(), "rooted without the program's bytes");
    assert_eq!(record.check_program_v1(), Ok(()));
    let carriage = PalwStateCarriageV2::from_state(&chain.s);
    let mut bent = carriage.clone();
    let row = bent.tir_classes.get_mut(&class_id).expect("the row rides the carriage");
    let mut other = row.program.as_ref().clone();
    let last = other.len() - 1;
    other[last] ^= 1;
    row.program = std::sync::Arc::new(other);
    assert!(bent.into_state(&chain.sp, None).is_err(), "a program that is not its graph_ir_root's is refused at load");
    assert!(carriage.into_state(&chain.sp, Some(chain.s.state_root())).is_ok());

    // A second registration of a live class is a duplicate, as for any class.
    let again =
        chain.try_fold(&chain.s, &ctx(0xCA_0000 + AT + 1, AT + 1, AT + 1, 0), &[object], PalwBlockWorkV3::None, Hash64::default());
    assert!(matches!(again, Err(PalwStateV2Error::DuplicateClass(id)) if id == class_id), "{again:?}");
}

#[test]
fn a_chain_with_no_ir_class_roots_and_carries_as_before() {
    // The table is empty below the fence (and on every shipped network): its root block and carriage
    // tail are absent, so the state and its carriage are the ones a build without them produces.
    let chain = Chain::new(armed());
    assert!(chain.s.tir_class_v1(&Hash64::from_bytes([1; 64])).is_none());
    let carriage = PalwStateCarriageV2::from_state(&chain.s);
    assert!(carriage.tir_classes.is_empty());
    let bytes = borsh::to_vec(&carriage).expect("encodes");
    let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("decodes");
    assert_eq!(back, carriage);
}

/// **One IR class registration a block** (`PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1`, the fold's second
/// lock behind the acceptance walk's by-name drop): two distinct IR classes in one block are refused
/// as `TirRegistrationsPerBlockExceeded`, whichever comes first; each folds alone, and the two land in
/// two consecutive blocks.
#[test]
fn a_second_ir_registration_in_one_block_is_refused_by_the_folds_second_lock() {
    use kaspa_consensus_core::palw_state_v2::PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1;
    assert_eq!(PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1, 1);
    let mut chain = Chain::new(armed());
    chain.room = true;
    let (registrant, _, _) = floor_producer(&chain.p);
    let a = registration_of(&chain, registrant, AT, class_with(0x70));
    let b = registration_of(&chain, registrant, AT, class_with(0x71));
    let id = |o: &PalwConsensusObjectV2| match o {
        PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, .. } => *class_id,
        _ => unreachable!(),
    };
    assert_ne!(id(&a), id(&b), "two classes");
    for pair in [[a.clone(), b.clone()], [b.clone(), a.clone()]] {
        let both = chain.try_fold(&chain.s, &ctx(0xCB_0000 + AT, AT, AT, 0), &pair, PalwBlockWorkV3::None, Hash64::default());
        assert!(
            matches!(both, Err(PalwStateV2Error::TirRegistrationsPerBlockExceeded { class, max: 1 }) if class == id(&pair[1])),
            "the second of the block is refused: {both:?}"
        );
    }
    for one in [&a, &b] {
        chain
            .try_fold(&chain.s, &ctx(0xCB_1000 + AT, AT, AT, 0), std::slice::from_ref(one), PalwBlockWorkV3::None, Hash64::default())
            .expect("one IR registration a block folds");
    }
    chain.step_at(AT, &[a.clone()], PalwBlockWorkV3::None, Hash64::default(), 0);
    chain.step_at(AT + 1, &[b.clone()], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(chain.s.tir_class_v1(&id(&a)).is_some() && chain.s.tir_class_v1(&id(&b)).is_some(), "one a block: both land");
}

/// **A registration past the second IR fence records the `Select`-arm credit** (`palw_tir_fence2`):
/// the registry's work row is `palw_tir_model_work_v2(.., true)` at or past the fence's height and the
/// release's vector below it — and for this class (six `Select`s) the two differ.
#[test]
fn a_registration_past_the_second_ir_fence_records_the_select_arm_credit() {
    use kaspa_consensus_core::palw_tir_work_v1::palw_tir_model_work_v2;
    let mut rows = Vec::new();
    for (fence2, min_select_arms) in [(AT, true), (AT + 50, false)] {
        let mut p = armed();
        p.palw_tir_fence2 = Some(ForkActivation::new(fence2));
        p.sync_palw_tir_fence2();
        p.validate_palw_v2().expect("the second IR fence at or past palw_tir_v1");
        let mut chain = Chain::new(p);
        chain.room = true;
        let (registrant, _, _) = floor_producer(&chain.p);
        let object = registration(&chain, registrant, AT);
        let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, admission, .. } = &object else { unreachable!() };
        let (class_id, admission) = (*class_id, admission.clone());
        chain.step_at(AT, &[object.clone()], PalwBlockWorkV3::None, Hash64::default(), 0);
        let program = admission.class.decode_program().expect("decodes");
        let want = palw_tir_model_work_v2(&program, &admission.canonical, min_select_arms).expect("a work row");
        let row = chain.s.model_lifecycle(&class_id).expect("the registry's row").work.clone();
        assert_eq!(row, want, "fence2 at {fence2}: the {} vector", if min_select_arms { "credited" } else { "release's" });
        rows.push(row);
    }
    assert_ne!(rows[0], rows[1], "the class's arm-only work is credited at the smaller arm past the fence");
    assert!(rows[0].verification_ccu <= rows[1].verification_ccu && rows[0].economic_ccu_per_claim <= rows[1].economic_ccu_per_claim);
}
