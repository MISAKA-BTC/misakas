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
    let (_, mut program, params, _) = fixture::programs().into_iter().find(|(n, ..)| n == "moe-top2-shared").expect("the MoE model");
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let ops = palw_tir_inventory_operands_v1(&program, &fixture::TensorSrc(&params)).expect("the inventory");
    let root = artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("a root");
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: fixture::layout(&program, 64),
        tokenizer_id: Hash64::from_bytes([0x70; 64]),
    };
    (class, root)
}

/// The registration a registrant makes: the chain's target and slash value, weightless, active now.
fn registration(chain: &Chain, registrant: PalwBondKeyV2, daa: u64) -> PalwConsensusObjectV2 {
    let (class, root) = class();
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
