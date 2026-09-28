//! **RFC-0002 Phase F, step F6: admission v10** — the corpus programs admitted and counted, the
//! registration builder's object admitted as built, and every mutation refused by its name.
//!
//! The classes are the golden corpus programs under the court fixture's layout (ragged commit tiles,
//! a 4,096-lane logits tile, two-position checkpoints) at a 64-position context, their logits under
//! the tiled scheme, their inventory roots real. The ruleset is testnet-12's with `palw_tir_v1` armed.

#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, artifact_root_v1};
use kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError as E;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2, PalwCourtParamsV2};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwPwuRuleV2};
use kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1;
use kaspa_consensus_core::palw_tir_admission_v1::{
    PALW_TIR_MAX_DISTINCT_TILES_V1, PalwTirAdmissionRulesV1, palw_tir_post_genesis_registration_v1, palw_tir_prim_kernel_ids_v1,
    palw_tir_registration_preflight_at_v1, verify_class_admission_v10,
};
use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_operands_v1;
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1;
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;

const AT: u64 = 1_000;
const CONTEXT: u32 = 64;

fn params() -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
    p.sync_palw_tir_v1();
    p
}

fn bundle(p: &Params) -> PalwConsensusParamsV2 {
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is a V2 network") };
    bundle.clone()
}

struct Case {
    name: String,
    class: PalwTirClassV1,
    artifact_root: Hash64,
}

/// Every corpus program as a class at [`CONTEXT`] positions.
fn cases() -> Vec<Case> {
    programs()
        .into_iter()
        .map(|(name, mut program, params, _)| {
            program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
            // A program with no params has an empty inventory, and no root (the fixture's rule).
            let artifact_root = if program.params.is_empty() {
                Hash64::from_bytes([0; 64])
            } else {
                let ops = palw_tir_inventory_operands_v1(&program, &TensorSrc(&params)).expect("the inventory");
                artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("a root")
            };
            let class = PalwTirClassV1 {
                version: PALW_TIR_CLASS_VERSION_V1,
                program: program.encode(),
                layout: layout(&program, CONTEXT),
                tokenizer_id: Hash64::from_bytes([0x70; 64]),
            };
            Case { name, class, artifact_root }
        })
        .collect()
}

/// The registration the builder makes for a case: the canonical job the formula names, weightless.
fn registration(case: &Case, share: u16) -> PalwConsensusObjectV2 {
    let class_id = case.class.class_id(&case.artifact_root);
    let facts = PalwTirJobFactsV1::of_class(&case.class, class_id).expect("decodes");
    let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&case.class).expect("wide enough"));
    palw_tir_post_genesis_registration_v1(
        case.class.clone(),
        canonical,
        case.artifact_root,
        share,
        1 << 100,
        1,
        AT + 10,
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
            transaction_id: kaspa_consensus_core::tx::TransactionId::from_bytes([7; 64]),
            index: 0,
        }),
        vec![9; 16],
        bundle(&params()).court.max_step_leaf_count(),
    )
    .expect("the builder counts the canonical job")
}

fn rules() -> PalwTirAdmissionRulesV1 {
    PalwTirAdmissionRulesV1::at(&params(), AT).expect("the fence is in force")
}

fn admit(b: &PalwConsensusParamsV2, r: &PalwTirAdmissionRulesV1, object: &PalwConsensusObjectV2) -> Result<(), E> {
    verify_class_admission_v10(b, r, object, &[], &[]).map(|_| ())
}

/// A case's object with `edit` applied to its carried class and the id re-derived — the count and
/// the canonical job left as the builder made them, so only the edited property differs.
fn with_class(case: &Case, edit: impl FnOnce(&mut PalwTirClassV1)) -> PalwConsensusObjectV2 {
    let mut object = registration(case, 0);
    if let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, artifact_root, admission, .. } = &mut object {
        edit(&mut admission.class);
        *class_id = admission.class.class_id(artifact_root);
        admission.canonical.shape_profile_id = *class_id;
    }
    object
}

fn edit_program(case: &Case, edit: impl FnOnce(&mut misaka_palw_tir::TirProgramV1)) -> PalwConsensusObjectV2 {
    let mut program = misaka_palw_tir::TirProgramV1::decode_canonical(&case.class.program).unwrap();
    edit(&mut program);
    let mut class = case.class.clone();
    class.program = program.encode();
    let class_id = class.class_id(&case.artifact_root);
    let PalwConsensusObjectV2::ClassRegisteredTirV1 {
        artifact_root,
        slash_value_per_pwu,
        pwu_rule,
        initial_target,
        activation_daa,
        admission,
        ..
    } = registration(case, 0)
    else {
        unreachable!()
    };
    let mut admission = admission;
    admission.class = class;
    admission.canonical.shape_profile_id = class_id;
    PalwConsensusObjectV2::ClassRegisteredTirV1 {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule,
        initial_target,
        share_permille: 0,
        activation_daa,
        admission,
    }
}

#[test]
fn the_corpus_is_admitted_counted_and_recorded() {
    let (p, r) = (params(), rules());
    let b = bundle(&p);
    eprintln!(
        "testnet-12's court: close {} B ({} chunks), terminal {} MACs, operands {}, ladder {}",
        b.court.max_close_bytes(),
        b.court.max_close_chunks(),
        b.court.max_terminal_macs(),
        b.court.max_operand_count(),
        b.court.max_step_leaf_count()
    );
    let mut admitted = 0;
    for case in cases() {
        let object = registration(&case, 0);
        let started = std::time::Instant::now();
        let verdict = verify_class_admission_v10(&b, &r, &object, &[], &[]);
        let took = started.elapsed();
        match verdict {
            Ok((entry, record)) => {
                eprintln!(
                    "{:>24}: admitted in {took:?}: canonical {} leaves, worst {}, worst close {} B, worst tile {} MACs",
                    case.name,
                    entry.canonical_step_leaf_count,
                    entry.max_step_leaf_count,
                    entry.court_cost.max_close_bytes,
                    entry.court_cost.max_terminal_macs
                );
                let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, pwu_rule, admission, .. } = &object else {
                    unreachable!()
                };
                assert_eq!(entry.class_id, *class_id);
                assert_eq!(record.facts.class_id, *class_id);
                assert_eq!(*pwu_rule, PalwPwuRuleV2::DerivedV1 { pwu_per_inference: entry.canonical_step_leaf_count });
                let space = PalwTirStepSpaceV1::new(&admission.class).unwrap();
                assert_eq!(space.leaf_count_capped(&admission.canonical, 1 << 40).unwrap(), entry.canonical_step_leaf_count);
                assert!(entry.canonical_step_leaf_count <= entry.max_step_leaf_count);
                assert!(!entry.reachable_kernels.is_empty() && entry.reachable_kernels.is_subset(&palw_tir_prim_kernel_ids_v1()));
                assert_eq!(record.facts, PalwTirJobFactsV1::of_class(&admission.class, *class_id).unwrap());
                assert_eq!(record.logits_tiles(), 1, "a small vocabulary is one tiled-logits tile");
                assert!(took.as_millis() < 2_000, "{}: admission is cheap", case.name);
                // The preflight is the same gate, read off the ruleset at the height.
                assert_eq!(palw_tir_registration_preflight_at_v1(&p, &b, &object, AT, &[]).map(|(e, _)| e), Ok(entry));
                admitted += 1;
            }
            // The two programs the range analysis cannot prove are refused by the program's own gate.
            Err(E::TirProgram(why)) => {
                eprintln!("{:>24}: refused, as the court fixture's range analysis refuses it: {why}", case.name)
            }
            Err(e) => panic!("{}: {e}", case.name),
        }
    }
    assert_eq!(admitted, 5, "the five corpus models");
}

#[test]
fn every_mutation_is_refused_by_its_name() {
    let (p, r) = (params(), rules());
    let b = bundle(&p);
    let cases = cases();
    let case = cases.iter().find(|c| c.name == "dense-gqa-2layer").expect("the dense model");
    assert_eq!(admit(&b, &r, &registration(case, 0)), Ok(()));

    // Below the fence the preflight names the fence.
    assert_eq!(
        palw_tir_registration_preflight_at_v1(&p, &b, &registration(case, 0), AT - 1, &[]).map(|_| ()),
        Err(E::TirNeedsItsFence)
    );
    let other = PalwConsensusObjectV2::PanelUnavailableQuorum { claim: Hash64::from_bytes([1; 64]), receipts: Vec::new() };
    assert!(verify_class_admission_v10(&b, &r, &other, &[], &[]).is_err_and(|e| e == E::NotARegistration));

    // The program's bytes, its primitive set, its scheme, its token bound, its history bound.
    let mut small = r;
    small.fence.ceilings.max_program_bytes = (case.class.program.len() - 1) as u32;
    assert!(matches!(admit(&b, &small, &registration(case, 0)), Err(E::CourtCostExceedsCeiling { what: "IR program bytes", .. })));
    let mut garbage = registration(case, 0);
    if let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = &mut garbage {
        admission.class.program.push(0);
    }
    assert!(matches!(admit(&b, &r, &garbage), Err(E::TirProgram(_))), "a trailing byte is not the canonical encoding");
    // Normal form requires the v1 primitive set's id, so a program naming another fails to decode;
    // a fence naming another set (which `validate_palw_tir_v1` refuses to arm) is refused by name.
    assert!(matches!(admit(&b, &r, &edit_program(case, |p| p.prim_set_id[0] ^= 1)), Err(E::TirProgram(_))));
    let mut other_set = r;
    other_set.fence.prim_set_id = Hash64::from_bytes([2; 64]);
    assert!(matches!(admit(&b, &other_set, &registration(case, 0)), Err(E::TirPrimSet { .. })));
    assert!(matches!(admit(&b, &r, &edit_program(case, |p| p.logits_scheme_id = [0; 64])), Err(E::TirLayout(_))));
    let vocab = {
        let p = misaka_palw_tir::TirProgramV1::decode_canonical(&case.class.program).unwrap();
        p.blocks[p.schedule.post as usize].nodes[p.logits as usize].out.elements_at(1) as u32
    };
    let narrow = edit_program(case, |p| p.token_bound = vocab - 1);
    assert!(
        matches!(admit(&b, &r, &narrow), Err(E::TirLayout(ref why)) if why.contains("token bound")),
        "{:?}",
        admit(&b, &r, &narrow)
    );
    let held = edit_program(case, |p| p.history_bound = misaka_palw_tir::program::HISTORY_BOUND_V1_HELD);
    assert!(matches!(admit(&b, &r, &held), Err(E::TirLayout(ref why)) if why.contains("held")), "{:?}", admit(&b, &r, &held));

    // The layout.
    let wide = with_class(case, |c| c.layout.max_context = r.fence.ceilings.max_context + 1);
    assert!(matches!(admit(&b, &r, &wide), Err(E::TirLayout(_))), "past the network's context");
    let mut short = registration(case, 0);
    if let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = &mut short {
        admission.class.layout.commit_tiles.pop();
    }
    assert!(matches!(admit(&b, &r, &short), Err(E::TirLayout(_))), "one tile per committed node");
    // More distinct commit tile lengths than admission runs `tir_admit_v1` for: a program with a
    // chain of ten committed adds, each tiled at its own length.
    let chain = {
        use misaka_palw_tir::builder::ProgramBuilder;
        use misaka_palw_tir::{DType, Ref, TensorType};
        let mut pb = ProgramBuilder::new(8, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
        let embed = pb.param("embed", DType::I8, &[8, 4], false);
        let head = pb.param("head", DType::I8, &[8, 4], false);
        let carry = vec![TensorType::fixed(DType::I32, &[4])];
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let x = b.gather(embed, Ref::Input(0), 0, 0);
            let mut x = b.cast(x, DType::I32);
            for _ in 0..10 {
                x = b.add(x, x, DType::I32);
                x = b.commit(x);
            }
            b.finish(&[x])
        };
        let (post, logits) = {
            let mut b = pb.block("post", carry);
            let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
            let l = b.matmul(head, x, DType::I64);
            let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
            let l = b.reshape_fixed(l, &[8]);
            let l = b.commit(l);
            let Ref::Node(i) = l else { unreachable!() };
            (b.finish(&[]), i)
        };
        let mut program = pb.finish(pre, vec![], post, logits);
        program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
        program
    };
    let ragged = Case {
        name: "chain".into(),
        class: PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: chain.encode(),
            layout: kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1 {
                // Ten chain tiles, then the logits at the tiled scheme's 4,096 lanes.
                commit_tiles: (0..10u32).map(|i| 4 + i).chain([4096]).collect(),
                ..layout(&chain, CONTEXT)
            },
            tokenizer_id: Hash64::from_bytes([0x70; 64]),
        },
        artifact_root: Hash64::from_bytes([0xA7; 64]),
    };
    assert!(PALW_TIR_MAX_DISTINCT_TILES_V1 < 11, "eleven distinct lengths");
    let object = registration(&ragged, 0);
    assert!(matches!(admit(&b, &r, &object), Err(E::TirLayout(ref why)) if why.contains("distinct")), "{:?}", admit(&b, &r, &object));
    let mut even = ragged;
    even.class.layout.commit_tiles = [4; 10].into_iter().chain([4096]).collect();
    assert_eq!(admit(&b, &r, &registration(&even, 0)), Ok(()), "the same program at one tile length is admitted");

    // The ceilings `tir_admit_v1` applies: cone work, position MACs, state bytes.
    for (what, edit) in [
        (
            "cone work",
            Box::new(|c: &mut PalwTirAdmissionRulesV1| c.fence.ceilings.max_cone_work = 3)
                as Box<dyn Fn(&mut PalwTirAdmissionRulesV1)>,
        ),
        ("position MACs", Box::new(|c: &mut PalwTirAdmissionRulesV1| c.fence.ceilings.max_macs_per_position = 1)),
        ("state bytes", Box::new(|c: &mut PalwTirAdmissionRulesV1| c.fence.ceilings.max_state_bytes = 1)),
    ] {
        let mut tight = r;
        edit(&mut tight);
        assert!(matches!(admit(&b, &tight, &registration(case, 0)), Err(E::TirExceeds { .. })), "{what}");
    }
    let mut tight = r;
    tight.fence.ceilings.max_peak_live_bytes = 1;
    assert!(matches!(admit(&b, &tight, &registration(case, 0)), Err(E::CourtCostExceedsCeiling { what: "IR peak live bytes", .. })));
    let mut tight = r;
    tight.fence.ceilings.max_unrolled_nodes = 1;
    assert!(matches!(admit(&b, &tight, &registration(case, 0)), Err(E::CourtCostExceedsCeiling { what: "IR unrolled nodes", .. })));

    // The court: its terminal MACs (or, for a history-reducing cone, the dissection this build
    // does not play yet), its evaluation work, its close bytes.
    let court = |close: u64, macs: u64| {
        let mut b = b.clone();
        b.court = PalwCourtParamsV2::with_cost_ceilings(b.court.max_step_leaf_count(), b.court.turn_deadline_daa(), 2, close, macs, 8)
            .unwrap();
        b
    };
    assert!(
        matches!(
            admit(&court(b.court.max_close_bytes(), 1), &r, &registration(case, 0)),
            Err(E::TirNeedsDissection { .. }
                | E::CourtCostExceedsCeiling {
                    what: "IR tile multiply-accumulates" | "IR cone evaluation work (tile and state replay)",
                    ..
                })
        ),
        "{:?}",
        admit(&court(b.court.max_close_bytes(), 1), &r, &registration(case, 0))
    );
    assert!(matches!(
        admit(&court(case.class.program.len() as u64, b.court.max_terminal_macs()), &r, &registration(case, 0)),
        Err(E::CourtCostExceedsCeiling { what: "IR close bytes", .. })
    ));

    // The canonical job, the count, the rule, the id, the share.
    let mut off = registration(case, 0);
    if let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = &mut off {
        admission.canonical.declared_prefill_tokens += 1;
    }
    assert!(matches!(admit(&b, &r, &off), Err(E::TirCanonicalNotTheFormula(_))));
    let mut named = registration(case, 0);
    if let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = &mut named {
        admission.canonical.network_id = b"testnet-12".to_vec();
    }
    assert!(matches!(admit(&b, &r, &named), Err(E::TirCanonicalNotTheFormula(_))), "the yardstick's identity fields are fixed");
    // A 15-position class has no canonical job: the formula needs max_context / 8 ≥ 2.
    let mut too_narrow = registration(case, 0);
    if let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, artifact_root, admission, .. } = &mut too_narrow {
        admission.class.layout.max_context = 15;
        *class_id = admission.class.class_id(artifact_root);
    }
    assert!(matches!(admit(&b, &r, &too_narrow), Err(E::TirCanonicalNotTheFormula(ref why)) if why.contains("narrow")));
    let mut counted = registration(case, 0);
    if let PalwConsensusObjectV2::ClassRegisteredTirV1 { pwu_rule, .. } = &mut counted {
        let PalwPwuRuleV2::DerivedV1 { pwu_per_inference } = pwu_rule else { unreachable!() };
        *pwu_per_inference += 1;
    }
    assert!(matches!(admit(&b, &r, &counted), Err(E::PwuPerInferenceMismatch { .. })));
    let mut capped = registration(case, 0);
    if let PalwConsensusObjectV2::ClassRegisteredTirV1 { pwu_rule, .. } = &mut capped {
        *pwu_rule = PalwPwuRuleV2::MaxPerAttempt(5);
    }
    assert_eq!(admit(&b, &r, &capped), Err(E::ClassIsNotDerived));
    let mut renamed = registration(case, 0);
    if let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, .. } = &mut renamed {
        *class_id = Hash64::from_bytes([1; 64]);
    }
    assert!(matches!(admit(&b, &r, &renamed), Err(E::TirClassIdIsNotDerived { .. })));
    // Weight: the RC's committed families cover no primitive, so a nonzero share is refused; a
    // family the chain certified over the primitives grants it.
    let rc = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    assert_eq!(
        verify_class_admission_v10(&b, &r, &registration(case, 1), &rc, &[]).map(|_| ()),
        Err(E::NotEndToEndCertified { share: 1 }),
        "no family certifies the primitives"
    );
    let family =
        kaspa_consensus_core::palw_e2e_adjudicability::PalwE2eFamilyV1 { kernel_ids: palw_tir_prim_kernel_ids_v1(), ..rc[0].clone() };
    assert_eq!(verify_class_admission_v10(&b, &r, &registration(case, 1), &rc, &[family]).map(|_| ()), Ok(()));
    assert!(
        verify_class_admission_v10(&b, &r, &registration(case, 1), &[], &[]).is_err(),
        "a certified set that is not the network's"
    );
}
