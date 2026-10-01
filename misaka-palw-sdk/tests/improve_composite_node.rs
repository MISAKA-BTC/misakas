//! **RFC-0004 A10: a composite candidate served from its parent's artifact and its adapter section**
//! (§6.3, §6.7) — the node's side of a LoRA candidate.
//!
//! The parent and the candidate are lowered, calibrated and materialised as the converter does it
//! (`palw-tir-fidelity`, `--adapter --parent-stats`), declared under a layout, and written as
//! PALWTIR1 containers; `palw-class composite --parent-class … --section-out` writes the candidate's
//! adapter section (`PALWTIRS`: params `P..` alone). Then, as a node loads them — the parent first,
//! then the section (`TirLineageV1`):
//!
//! * the section opens over the held parent: the reference is the chain's (`parent_class`,
//!   `parent_root`, `adapter_root`, `P`), the artifact root the composite root, and the class the
//!   candidate's — its program, layout and tokenizer, its id over the composite root;
//! * every param instance it serves is the full candidate's, byte for byte, and its inventory has
//!   the full candidate's leaves in their order (its root is no commitment; the composite root is);
//! * it runs the candidate exactly: the same logits and ids as the full container;
//! * its court carriage is made under the sub-roots (the parent's leaves under the parent root, the
//!   adapter's rebased under the adapter root; never one multiproof against one root), and its
//!   honest cone closes acquit through them on both sides of the split;
//! * a section whose parent is not held, a tampered adapter root, a parent root the file does not
//!   have, and a section opened at another `P` are each refused.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_improve_composite_v1::PalwTirCompositeRefV1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step_refute::{PALW_LOGITS_TILE_LANES, PalwStepRefuteError, tiled_logits_scheme_id_v1};
use kaspa_consensus_core::palw_tir_artifact_v1::PalwTirTensorSourceV1;
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_court_v1::{PalwTirParamOpeningV1, check_tir_cone_refutation_v1};
use kaspa_hashes::Hash64;
use misaka_palw_sdk::lineages::tir::TirLineageV1;
use misaka_palw_sdk::tir_composite::{tir_composite_derive_v1, tir_composite_section_write_v1};
use misaka_palw_sdk::{PalwModelLineageV1, PalwWeightResidencyV1};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_artifact::{PalwTirContainerV1, write_container_v1};
use misaka_palw_tir_exec::node::{TirArtifactV1, TirParamOpenerV1};
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::{IntParams, LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::{Checkpoint, Overlay};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures")
}

/// A scratch directory of its own, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("palw-composite-node-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The parent and the candidate, as `palw-tir-fidelity` and `palw-tir-fidelity --adapter
/// --parent-stats` convert them: programs, materialised params, and `P`.
fn converted(adapter: &str) -> (TirProgramV1, IntParams, TirProgramV1, IntParams, u32) {
    let ad_dir = fixtures().join("hf-lora").join(adapter);
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(ad_dir.join("logits.json")).unwrap()).unwrap();
    let base_dir = fixtures().join("hf").join(meta["base"].as_str().unwrap());
    let cfg = std::fs::read_to_string(base_dir.join("config.json")).unwrap();
    let opts = LowerOpts { max_window: Some(64), ..LowerOpts::default() };
    let parent = fidelity::prepare(&cfg, &opts).expect("the parent");
    let (cand, p) = fidelity::prepare_candidate(&parent, &std::fs::read_to_string(ad_dir.join("adapter_config.json")).unwrap(), &opts)
        .expect("the candidate");
    let ck = Checkpoint::open(&base_dir).expect("checkpoint");
    let ad = Checkpoint::open(&ad_dir.join("adapter_model.safetensors")).expect("adapter");
    let (pf, _) = ParamStore::from_source(&parent.hl, &parent.binding, &ck).expect("parent params");
    let (cf, _) = ParamStore::from_source(&cand.hl, &cand.binding, &Overlay { base: &ck, over: &ad }).expect("candidate params");
    let calib = fidelity::random_sequences(parent.hl.vocab, 4, 32, 11);
    let quiet = |_: usize, _: usize| {};
    let (lp, lc) = (Resident(Arc::new(pf)), Resident(Arc::new(cf)));
    let stats_p = fidelity::calibrate(&parent.hl, &lp, &calib, &quiet).expect("parent calibration");
    let mat_p = materialise(&parent.lowered, &parent.hl, &lp, &stats_p, &QuantPolicy::default(), &quiet).expect("parent artifact");
    let own = fidelity::calibrate(&cand.hl, &lc, &calib, &quiet).expect("candidate calibration");
    let stats = fidelity::candidate_stats(&stats_p, own);
    let mat_c = materialise(&cand.lowered, &cand.hl, &lc, &stats, &QuantPolicy::default(), &quiet).expect("candidate artifact");
    (parent.lowered.program.clone(), mat_p.params, cand.lowered.program.clone(), mat_c.params, p as u32)
}

/// The program under the tiled logits scheme (the class a node registers).
fn tiled(program: &TirProgramV1) -> TirProgramV1 {
    let mut p = program.clone();
    p.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    TirProgramV1::decode_canonical(&p.encode()).expect("still canonical")
}

/// A small declared layout: the logits node at the tiled scheme's 4,096 lanes, every other commit
/// node in 64-lane tiles, two-position checkpoints.
fn layout(program: &TirProgramV1) -> PalwTirLayoutV1 {
    let mut commit_tiles = Vec::new();
    for (bi, b) in program.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if n.commit {
                let logits = bi == program.schedule.post as usize && ni == program.logits as usize;
                commit_tiles.push(if logits { PALW_LOGITS_TILE_LANES as u32 } else { 64 });
            }
        }
    }
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: 16,
        checkpoint_interval: 2,
        h_tile: 4,
        commit_tiles,
        state_tiles: program.states.iter().map(|_| 4).collect(),
    }
}

/// A PALWTIR1 container of `program` under its layout, the tensors from `params`.
fn write(path: &Path, program: &TirProgramV1, params: &IntParams, meta: serde_json::Value) {
    write_container_v1(path, program, borsh::to_vec(&layout(program)).unwrap(), [0x61; 64], meta.to_string(), &mut |j, l| {
        params.tensors.get(&(j, l)).map(|t| t.le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
    })
    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

fn court() -> PalwCourtParamsV2 {
    PalwCourtParamsV2::new(kaspa_consensus_core::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT, 4, 2).expect("a court")
}

fn serve(adapter: &str) {
    let (parent_program, parent_params, candidate_program, candidate_params, p) = converted(adapter);
    let (parent_program, candidate_program) = (tiled(&parent_program), tiled(&candidate_program));
    let dir = Scratch::new(adapter);
    let (pp, cp, sp) = (dir.0.join("parent.palwtir"), dir.0.join("candidate.palwtir"), dir.0.join("candidate.palwtirs"));
    write(&pp, &parent_program, &parent_params, serde_json::json!({ "model_id": "test/parent" }));
    write(&cp, &candidate_program, &candidate_params, serde_json::json!({ "model_id": "test/candidate", "composite": { "p": p } }));

    // The node loads the parent; the tool writes the section against the parent's class.
    let lineage = TirLineageV1::new();
    lineage.load(&pp, PalwWeightResidencyV1::PageCache).expect("the parent loads");
    let parent = lineage.tir_classes().remove(0);
    let (pc, cc) = (PalwTirContainerV1::open(&pp).unwrap(), PalwTirContainerV1::open(&cp).unwrap());
    let composite = tir_composite_derive_v1(&pc, &cc, Some(parent.class_id())).expect("a composite of the parent");
    tir_composite_section_write_v1(&cc, &composite, &sp).expect("the section");
    assert!(
        std::fs::metadata(&sp).unwrap().len() < std::fs::metadata(&cp).unwrap().len(),
        "{adapter}: the section is the adapter's alone"
    );

    // The section loads over the held parent.
    lineage.load(&sp, PalwWeightResidencyV1::PageCache).expect("the section loads over its parent");
    let entry = lineage.tir_classes().into_iter().find(|e| e.artifact.composite_ref().is_some()).expect("the composite class");
    let r = *entry.artifact.composite_ref().unwrap();
    assert_eq!(
        r,
        PalwTirCompositeRefV1 {
            parent_class: parent.class_id(),
            parent_root: parent.artifact_root,
            adapter_root: composite.adapter_root,
            p
        },
        "{adapter}: the chain's reference"
    );
    assert_eq!(Some(entry.artifact_root), composite.artifact_root(), "{adapter}: the composite root");
    let full = TirLineageV1::open_entry(&cp).expect("the full candidate");
    assert_eq!(entry.class, full.class, "{adapter}: the candidate's program, layout and tokenizer");
    assert_eq!(entry.class_id(), full.class.class_id(&r.artifact_root()), "{adapter}: its id over the composite root");
    assert_eq!(entry.model_id, "test/candidate");

    // Every instance it serves is the full candidate's; its inventory is the full candidate's leaves.
    for (j, layers) in misaka_palw_tir_artifact::param_instances_v1(&candidate_program).into_iter().enumerate() {
        for l in layers {
            let j = j as u16;
            assert_eq!(entry.artifact.tensor_bytes(j, l), full.artifact.tensor_bytes(j, l), "{adapter}: param {j} at {l:?}");
        }
    }
    let (ct, ft) = (entry.artifact.inventory_tree().unwrap(), full.artifact.inventory_tree().unwrap());
    assert_eq!(ct.leaves(), ft.leaves(), "{adapter}: the same leaves, in the same order");
    assert_eq!(entry.artifact.inventory_root(), Ok((r.artifact_root(), ft.leaf_count())));
    assert_eq!(composite.parent_leaves + composite.adapter_leaves, ft.leaf_count());

    // The carriage: under the sub-roots only.
    let split = composite.parent_leaves;
    let sides = |leaves: &[u32]| match entry.artifact.param_carriage(leaves) {
        Some(PalwTirParamOpeningV1::Composite(o)) => {
            assert_eq!(o.artifact, r);
            (o.parent.is_some(), o.adapter.is_some())
        }
        other => panic!("{adapter}: {leaves:?}: a composite carriage, got {other:?}"),
    };
    assert_eq!(sides(&[0, 1]), (true, false));
    assert_eq!(sides(&[split, split + 1]), (false, true));
    assert_eq!(sides(&[split - 1, split]), (true, true));
    assert!(entry.artifact.param_multiproof(&[0]).is_none(), "{adapter}: no one-root multiproof of a composite");
    for leaf in [0, split - 1, split, ft.leaf_count() - 1] {
        let (a, b) = (entry.artifact.param_opening(leaf).unwrap(), full.artifact.param_opening(leaf).unwrap());
        assert_eq!(a.operand, b.operand, "{adapter}: leaf {leaf}'s operand, read for an evaluation");
    }
    assert!(
        matches!(full.artifact.param_carriage(&[0, split]), Some(PalwTirParamOpeningV1::Single(_))),
        "a one-root class carries one multiproof"
    );

    // The run: the candidate, computed from the two files exactly as from its own container.
    let form = PalwPromptIdsFormV1::Flat;
    let backend = TirLineageV1::backend(&entry, &court(), form).expect("the composite's backend");
    let full_backend = TirLineageV1::backend(&full, &court(), form).expect("the candidate's backend");
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    let anchor = Hash64::from_bytes([0xA7; 64]);
    let (ctx, prompt_usize) = backend.job_for_anchor(anchor).expect("the composite's job");
    let (full_ctx, full_prompt) = full_backend.job_for_anchor(anchor).expect("the candidate's job");
    assert_eq!(prompt_usize, full_prompt, "{adapter}: one anchor, one prompt");
    let prompt: Vec<u32> = prompt_usize.iter().map(|t| *t as u32).collect();
    let (a, b) = (backend.retain(&ctx, &prompt).unwrap(), full_backend.retain(&full_ctx, &prompt).unwrap());
    assert_eq!((&a.logits_rows, &a.generated), (&b.logits_rows, &b.generated), "{adapter}: the composite computes the candidate");

    // The court: honest cone closes acquit through the sub-roots, on both sides of the split.
    let run = backend.execute(&ctx, &prompt_usize).expect("the composite runs");
    let rules = backend.court_rules(&court());
    let n = a.leaf_hashes.len() as u64;
    let (mut parent_side, mut adapter_side, mut acquitted) = (0, 0, 0);
    for leaf in (0..n).step_by((n as usize / 48).max(1)) {
        let refutation = match backend.cone_close(&run.material, leaf, &rules) {
            Ok(PalwCourtVerdictProofV2::TirCone { refutation }) => refutation,
            Ok(_) => panic!("{adapter}: leaf {leaf}: a cone close"),
            Err(e) => {
                eprintln!("{adapter}: leaf {leaf}: no close: {e}");
                continue;
            }
        };
        match &refutation.params {
            PalwTirParamOpeningV1::Composite(o) => {
                parent_side += usize::from(o.parent.is_some());
                adapter_side += usize::from(o.adapter.is_some());
            }
            PalwTirParamOpeningV1::None => {}
            PalwTirParamOpeningV1::Single(_) => {
                panic!("{adapter}: leaf {leaf}: a composite class never carries one root's multiproof")
            }
        }
        assert!(
            matches!(check_tir_cone_refutation_v1(&refutation, &rules), Err(PalwStepRefuteError::NoFaultFound)),
            "{adapter}: leaf {leaf}: an honest close acquits"
        );
        acquitted += 1;
    }
    assert!(
        acquitted > 8 && parent_side > 0 && adapter_side > 0,
        "{adapter}: {acquitted} closes, {parent_side} parent / {adapter_side} adapter"
    );

    // The evaluation executor (RFC-0004 A10) on the held classes: the candidate served from its two
    // files runs an evaluation job exactly as its own container does — the same stage roots, step
    // root, ids and score — and a seat holding either class replays the other's claim of the same
    // work; the parent runs the same job as a subject of its own.
    {
        use kaspa_consensus_core::palw_improve_eval_v1::{PalwEvalModeV1, PalwEvalStageParamsV1};
        use kaspa_consensus_core::palw_improve_state_v1::{PalwEvalSubjectV1, PalwScoringKindV1};
        use misaka_palw_sdk::improve::PalwImproveEvalTaskV1;
        use misaka_palw_sdk::improve_eval::{PalwEvalHeldV1, PalwEvalSeatJudgmentV1, palw_eval_run_v1, palw_eval_seat_judge_v1};
        let task_of = |held: &PalwEvalHeldV1, kind, mode: PalwEvalModeV1, reference: Vec<u32>, params| {
            let subject = PalwEvalSubjectV1::Candidate(held.class_id);
            let job = kaspa_consensus_core::palw_improve_eval_v1::PalwEvalJobV1 {
                line_id: Hash64::from_bytes([0x11; 64]),
                epoch: 3,
                item: 7,
                subject,
                kind,
                mode: mode.clone(),
            };
            PalwImproveEvalTaskV1 {
                line_id: job.line_id,
                epoch: 3,
                item: 7,
                subject,
                subject_class: held.class_id,
                kind,
                mode,
                job_id: job.id(),
                prompt_ids: vec![1, 5, 9],
                reference_ids: reference,
                params,
            }
        };
        let (held_comp, held_full, held_parent) =
            (PalwEvalHeldV1::from_entry(&entry).unwrap(), PalwEvalHeldV1::from_entry(&full).unwrap(), PalwEvalHeldV1::from_entry(&parent).unwrap());
        let seed = kaspa_consensus_core::palw_improve_state_v1::palw_improve_eval_seed_v1(&Hash64::from_bytes([0x33; 64]), 7);
        let generate = |h: &PalwEvalHeldV1| {
            task_of(
                h,
                PalwScoringKindV1::ExactMatch,
                PalwEvalModeV1::Generate { seed, max_new: 3, stop_ids: vec![] },
                vec![],
                PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 },
            )
        };
        let (wc, wf) = (
            palw_eval_run_v1(&held_comp, &generate(&held_comp)).expect("the composite generates"),
            palw_eval_run_v1(&held_full, &generate(&held_full)).expect("the full candidate generates"),
        );
        assert_eq!(wc.execution.claim, wf.execution.claim, "{adapter}: the composite computes the candidate: one step tree, one answer");
        assert_eq!(wc.generated().len(), 3);
        assert_ne!(wc.binding.committed_execution_root, wf.binding.committed_execution_root, "the class is in the execution root");
        let forced = |h: &PalwEvalHeldV1| {
            task_of(
                h,
                PalwScoringKindV1::RefLogLik,
                PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::from_bytes([0x44; 64]) },
                wc.generated().to_vec(),
                PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 12 },
            )
        };
        let (fc, ff) = (
            palw_eval_run_v1(&held_comp, &forced(&held_comp)).expect("the composite scores"),
            palw_eval_run_v1(&held_full, &forced(&held_full)).expect("the full candidate scores"),
        );
        assert_eq!(fc.tail.score, ff.tail.score, "{adapter}: the same log-likelihood from the two files as from one");
        let fp = palw_eval_run_v1(&held_parent, &forced(&held_parent)).expect("the parent scores the same reference");
        assert_ne!(fc.tail.score, fp.tail.score, "{adapter}: the adapter moves the likelihood: the candidate is not its parent");
        // Claims check, and a seat holding the class replays them to Valid.
        let facts = misaka_palw_sdk::improve_eval::PalwEvalClaimFactsV1 {
            network_domain: Hash64::from_bytes([0x10; 64]),
            executor_bond: kaspa_consensus_core::config::premine::premine_outpoint(2),
            executor_pubkey: vec![7; 16],
            operator_id: Hash64::from_bytes([0x12; 64]),
            anchor_block: Hash64::from_bytes([0x13; 64]),
            anchor_daa: 99,
            prompt_ids_form: form,
            trace_retention_daa: 5_000,
        };
        for (work, held) in [(&wc, &held_comp), (&fc, &held_comp), (&fp, &held_parent)] {
            let claim = misaka_palw_sdk::improve_eval::palw_eval_claim_v1(work, held, &facts).expect("a claim the chain's check admits");
            assert_eq!(
                palw_eval_seat_judge_v1(held, &claim.commitment, &claim.prompt, &claim.tail),
                PalwEvalSeatJudgmentV1::Valid,
                "{adapter}: a seat holding the class replays the claim"
            );
        }
    }

    // Refusals.
    let stranger = tir_composite_derive_v1(&pc, &cc, Some(Hash64::from_bytes([0x3c; 64]))).unwrap();
    let orphan = dir.0.join("orphan.palwtirs");
    tir_composite_section_write_v1(&cc, &stranger, &orphan).unwrap();
    let e = lineage.load(&orphan, PalwWeightResidencyV1::PageCache).unwrap_err();
    assert!(e.contains("is not held"), "{adapter}: a section of a parent this node does not hold: {e}");
    let e = TirArtifactV1::open_composite(&pp, &sp, &PalwTirCompositeRefV1 { adapter_root: Hash64::from_bytes([1; 64]), ..r }, None)
        .err()
        .expect("a tampered adapter root");
    assert!(e.contains("adapter section roots"), "{adapter}: {e}");
    let e = TirArtifactV1::open_composite(&pp, &sp, &PalwTirCompositeRefV1 { parent_root: Hash64::from_bytes([2; 64]), ..r }, None)
        .err()
        .expect("a parent root the file does not have");
    assert!(e.contains("the parent roots to"), "{adapter}: {e}");
    assert!(TirArtifactV1::open_composite(&pp, &sp, &PalwTirCompositeRefV1 { p: p + 1, ..r }, None).is_err(), "another P");
    eprintln!(
        "{adapter}: P {p}, parent {} leaves, adapter {} leaves; {acquitted} closes acquitted ({parent_side} parent-side, {adapter_side} adapter-side)",
        composite.parent_leaves, composite.adapter_leaves
    );
}

#[test]
fn a_llama_rank_16_candidate_is_served_from_its_parents_artifact_and_its_section() {
    serve("llama_r16");
}

#[test]
fn a_mistral_q_v_candidate_is_served_from_its_parents_artifact_and_its_section() {
    serve("mistral_qv_r8");
}
