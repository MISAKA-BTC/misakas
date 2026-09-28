//! **The IR family drill on tiny classes** (feature `node`; RFC-0002 Phase F, F9's exit test and
//! D-F2's shape): each admissible program is written as a PALWTIR1 container, opened MAPPED, served
//! by [`TirBackendV1`], and drilled — every committed node, checkpoint and history tile the job
//! reaches, in both call classes, a planted lie convicted by the shipped court and an honest
//! executor acquitted, the ladder narrowing to the lie, a challenger's re-execution building the
//! accused's own refutation — until the certificate covers every primitive the program uses.

#![cfg(feature = "node")]

mod node_common;

use std::collections::BTreeSet;
use std::sync::Arc;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT;
use kaspa_consensus_core::palw_court_v2::{PalwCourtVerdictProofV2, check_close_cost_v2};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_tir_court_v1::{
    check_tir_cone_refutation_v1, check_tir_decode_token_flat_v1, check_tir_decode_token_tiled_v1,
};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_exec::node::{TirArtifactV1, TirBackendV1, TirCaptureV1, TirDrillCallV1, TirDrillUnitKindV1, tir_family_drill_v1};
use node_common::{layout, programs, tiled};

#[test]
fn the_drill_certifies_every_tiny_class() {
    let dir = std::env::temp_dir().join(format!("tir-exec-drill-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut classes, mut units) = (0usize, 0usize);
    let mut kinds = BTreeSet::new();
    let mut prims = BTreeSet::new();
    for (k, (name, program, params)) in programs().into_iter().enumerate() {
        if analyze_ranges(&program).is_err() || program.params.is_empty() {
            continue;
        }
        let program = if k % 2 == 0 { program } else { TirProgramV1::decode_canonical(&tiled(program).encode()).unwrap() };
        let form = if k % 3 == 0 { PalwPromptIdsFormV1::MerkleV1 } else { PalwPromptIdsFormV1::Flat };
        let lay = layout(&program, 5, 2, 2, 64);
        let path = dir.join(format!("{}.palwtir", name.replace(' ', "-")));
        let mut tensor = |j: u16, l: Option<u16>| -> Result<Vec<u8>, String> {
            params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
        };
        misaka_palw_tir_artifact::write_container_v1(
            &path,
            &program,
            borsh::to_vec(&lay).unwrap(),
            [2; 64],
            name.clone(),
            &mut tensor,
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let artifact = Arc::new(TirArtifactV1::open(&path).unwrap_or_else(|e| panic!("{name}: {e}")));
        let (root, _) = artifact.inventory_root().unwrap();
        let class = artifact.class().unwrap();
        let canonical =
            kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class.class_id(&root), (4, 3)).unwrap();
        let backend =
            TirBackendV1::new(name.clone(), artifact, root, canonical, form, 1 << 26).unwrap_or_else(|e| panic!("{name}: {e}"));
        // The rules the court's IR arm grades under: its ladder, the prompt form, its work limits.
        let court = PalwCourtParamsV2::new(PALW_RC_COURT_MAX_STEP_LEAF_COUNT, 4, 2).expect("the shipped court");
        let rules = backend.court_rules(&court);
        assert_ne!(rules.limits, DemandLimits::UNLIMITED);

        // A seat's check of the honest capture, and of captures that answer something else.
        let anchor = Hash64::from_bytes([0x3C ^ k as u8; 64]);
        let (job, prompt) = backend.job_for_anchor(anchor).unwrap();
        let outcome = backend.execute(&job, &prompt).unwrap();
        let claim = PalwClaimRootsV1 {
            execution_root: outcome.execution_root,
            trace_root: outcome.trace_root,
            anchor,
            attempt_draw: Some(false),
            output_root: Some(outcome.output_root),
            job_pin: None,
        };
        assert_eq!(backend.verify_material(&outcome.material, claim), PalwMaterialVerdictV1::Matches, "{name}");
        let other = PalwClaimRootsV1 { anchor: Hash64::from_bytes([0x77; 64]), ..claim };
        assert_eq!(backend.verify_material(&outcome.material, other), PalwMaterialVerdictV1::Mismatch, "{name}: another anchor");
        // A lie the producer committed to answers for ITS roots (that is how a challenger finds the
        // capture it closes from) and for no other; the replay is what tells it from the honest run.
        let forged = backend.execute_with_injected_fault(&job, &prompt, 3).unwrap();
        let forged_claim = PalwClaimRootsV1 { execution_root: forged.execution_root, ..claim };
        assert_eq!(backend.verify_material(&forged.material, forged_claim), PalwMaterialVerdictV1::Matches, "{name}: its own roots");
        assert_eq!(backend.verify_material(&forged.material, claim), PalwMaterialVerdictV1::Mismatch, "{name}: the honest roots");
        let replay = backend.execute_for_verdict(&job, &prompt).unwrap();
        assert_eq!(
            (replay.execution_root, replay.trace_root, replay.output_root),
            (outcome.execution_root, outcome.trace_root, Some(outcome.output_root))
        );
        assert_ne!(replay.execution_root, forged.execution_root, "{name}: the replay refuses the lie");
        // A capture whose leaf moved after it was committed answers for nothing.
        let mut tampered = TirCaptureV1::decode(&outcome.material).unwrap();
        tampered.leaves[1].values_le[0] ^= 1;
        assert_eq!(backend.verify_material(&tampered.encode(), claim), PalwMaterialVerdictV1::Mismatch, "{name}: a moved leaf");
        assert_eq!(backend.verify_material(b"not a capture", claim), PalwMaterialVerdictV1::Unverifiable);
        // Readiness: the drawn leaves open against the class root in one multiproof.
        let (root_now, leaf_count) = backend.artifact_root_and_leaf_count().unwrap();
        assert_eq!(root_now, root);
        let draw: Vec<u32> = (0..leaf_count).step_by(3).collect();
        let (r2, leaves, opened) = backend.artifact_readiness_material(&draw).unwrap();
        assert_eq!((r2, leaves.len() as u32), (root, leaf_count));
        let mut streamed = Vec::new();
        let (r3, n3, opened3) = backend.artifact_readiness_material_streamed_v1(&draw, &mut |l| streamed.push(l)).unwrap().unwrap();
        assert_eq!((r3, n3, &streamed, &opened3), (root, leaf_count, &leaves, &opened));
        let mut sorted = opened.clone();
        sorted.sort_by_key(|(i, _)| *i);
        let proof = kaspa_consensus_core::palw_artifact::palw_artifact_multiproof_v1(&leaves, &sorted).unwrap();
        kaspa_consensus_core::palw_artifact::verify_artifact_multiproof_v1(&proof, root).unwrap();

        // A FOLD of the same execution (no preimages): a seat's check re-executes it, its prefix
        // states are the dense capture's, and its leaves open as the executor's own.
        let folding = TirBackendV1::new(name.clone(), backend.artifact().clone(), root, backend.canonical().clone(), form, 1 << 26)
            .unwrap()
            .with_dense_capture_bytes(0);
        let fold = folding.execute(&job, &prompt).unwrap();
        assert!(!TirCaptureV1::decode(&fold.material).unwrap().is_dense(), "{name}: a fold");
        assert_eq!((fold.execution_root, fold.trace_root), (outcome.execution_root, outcome.trace_root));
        assert_eq!(folding.verify_material(&fold.material, claim), PalwMaterialVerdictV1::Matches, "{name}: the fold");
        let n = TirCaptureV1::decode(&outcome.material).unwrap().binding.step_leaf_count;
        for i in [0, n / 3, n / 2, n - 1] {
            assert_eq!(
                folding.bisect_prefix_state(&fold.material, i),
                backend.bisect_prefix_state(&outcome.material, i),
                "{name}: {i}"
            );
            let r = folding.cone_refutation(&fold.material, i, &rules).unwrap_or_else(|e| panic!("{name}: fold leaf {i}: {e}"));
            assert_eq!(r, backend.cone_refutation(&outcome.material, i, &rules).unwrap(), "{name}: fold leaf {i}");
        }

        // The close objects a CourtClosed carries: within the court's byte ceiling, and graded
        // by the IR court (cone and logits closes acquit the honest execution; the decode door
        // finds the honest token).
        for i in [0, n / 2, n - 1] {
            let close = backend.cone_close(&outcome.material, i, &rules).unwrap();
            assert!(close.is_tir_v1());
            check_close_cost_v2(&close, &court).unwrap_or_else(|e| panic!("{name}: close of leaf {i}: {e}"));
            let PalwCourtVerdictProofV2::TirCone { refutation } = &close else { unreachable!() };
            assert_eq!(check_tir_cone_refutation_v1(refutation, &rules), Err(PalwStepRefuteError::NoFaultFound));
        }
        let decode = backend.decode_token_close(&outcome.material, 0, 1).unwrap();
        check_close_cost_v2(&decode, &court).unwrap();
        let verdict = match &decode {
            PalwCourtVerdictProofV2::TirDecodeTokenTiled { binding, pin } => check_tir_decode_token_tiled_v1(binding, pin, &rules),
            PalwCourtVerdictProofV2::TirDecodeToken { binding, pin, position } => {
                check_tir_decode_token_flat_v1(binding, pin, *position)
            }
            _ => unreachable!(),
        };
        assert_eq!(verdict, Err(PalwStepRefuteError::NoFaultFound), "{name}: the decode door");

        let cert = tir_family_drill_v1(&backend, anchor, &rules).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(cert.holds(), "{name}: {:?} of {:?} covered", cert.covered_prims, cert.reachable_prims);
        let calls: BTreeSet<_> = cert.units.iter().map(|u| u.call).collect();
        assert_eq!(calls, [TirDrillCallV1::Prefill, TirDrillCallV1::Decode].into_iter().collect(), "{name}: both call classes");
        for u in &cert.units {
            kinds.insert(match u.kind {
                TirDrillUnitKindV1::Commit { .. } => "commit",
                TirDrillUnitKindV1::Checkpoint { .. } => "checkpoint",
                TirDrillUnitKindV1::HistTile { .. } => "history tile",
            });
        }
        prims.extend(cert.covered_prims.iter().copied());
        units += cert.units.len();
        classes += 1;
        std::fs::remove_file(&path).ok();
    }
    std::fs::remove_dir_all(&dir).ok();
    eprintln!("{classes} classes certified: {units} units drilled both ways ({kinds:?}); primitives covered: {prims:?}");
    assert!(classes >= 5, "{classes}");
    assert_eq!(kinds.len(), 3, "commit tiles, checkpoints and history tiles");
}
