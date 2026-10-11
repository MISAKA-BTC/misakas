//! RFC06 public partial-holder dissection, over the executable model corpus.
//! These fixtures measure the actual carried evidence and prove local constraint closure.
//! They do not certify population throughput, watcher economics or whole-claim soundness.
#![cfg(feature = "node")]
mod node_common;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_mode_v2::{
    DEFAULT_MAX_CLOSE_BYTES, DEFAULT_MAX_OPERAND_COUNT, DEFAULT_MAX_TERMINAL_MACS, PalwCourtParamsV2,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step_leg::{step_merkle_root_v1, step_tile_leaf_hash_v1};
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_tir_court_v1::{
    PalwTirCourtRulesV1, check_tir_dissect_bottom_v1, check_tir_root_claim_v1, tir_root_claim_finalizes_to_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirDissectPhaseV1, PalwTirDissectSiteV1, PalwTirFoldV1,
    PalwTirRootClaimV1, palw_tir_dissect_site_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::{PalwTirLeafKindV1, palw_tir_execution_root_v1};
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_exec::node::{
    TirArtifactV1, TirBackendV1, TirCaptureV1, TirClassRunnerV1, TirEvidenceV1, TirFileMirrorV1, TirShardHoldingV1,
    fetch_shard_params_v1, tir_dissect_choice_v1, tir_shard_geometry_over_v1,
};
use std::sync::Arc;

const SESSION: Hash64 = Hash64::from_bytes([0x6D; 64]);
fn rules() -> PalwTirCourtRulesV1 {
    let court =
        PalwCourtParamsV2::with_cost_ceilings(1 << 26, 10, 2, DEFAULT_MAX_CLOSE_BYTES, DEFAULT_MAX_TERMINAL_MACS, DEFAULT_MAX_OPERAND_COUNT).unwrap();
    PalwTirCourtRulesV1 {
        max_step_leaf_count: 1 << 26,
        prompt_form: PalwPromptIdsFormV1::Flat,
        limits: kaspa_consensus_core::palw_court_v2::palw_tir_court_limits_v1(&court),
    }
}

fn reroot(capture: &mut TirCaptureV1, class_id: Hash64) {
    let context = capture.binding.job_context.context_hash();
    let hashes: Vec<_> = capture.leaves.iter().map(|l| step_tile_leaf_hash_v1(&context, &class_id, l)).collect();
    capture.binding.step_merkle_root = step_merkle_root_v1(&hashes).unwrap();
    capture.binding.committed_execution_root = palw_tir_execution_root_v1(
        &context,
        &capture.binding.full_logits_trace_root,
        &class_id,
        capture.binding.step_leaf_count,
        &capture.binding.step_merkle_root,
    );
}

// Make a protocol-admissible false root: finalize its lying totals and commit the resulting
// tile. Totals which rounding absorbs are also valid lies and still have a false bottom.
fn lying_root(
    holding: &TirShardHoldingV1,
    capture: &TirCaptureV1,
    index: u64,
    root: &PalwTirRootClaimV1,
    site: &PalwTirDissectSiteV1,
    reduction: usize,
    rules: &PalwTirCourtRulesV1,
) -> Option<(TirCaptureV1, PalwTirRootClaimV1)> {
    let runner = TirClassRunnerV1::new(&holding.space, &holding.plan, &holding.params, holding.class_id).unwrap();
    let store = TirEvidenceV1::dense(
        &runner,
        &capture.binding,
        &capture.prompt,
        &capture.logits_rows,
        &capture.generated,
        &capture.leaves,
        holding,
        rules.prompt_form,
        rules.max_step_leaf_count,
    )
    .unwrap();
    let n = root.totals.partials[reduction].len();
    for e in [0, n / 2, n - 1] {
        for step in [1, 1 << 4, 1 << 8, 1 << 12, 1 << 16, 1 << 20, 1 << 24, 1 << 28, 1 << 32] {
            for delta in [step, -step] {
                let mut totals = root.totals.clone();
                totals.partials[reduction][e] += delta;
                if !(site.bounds[reduction].lo..=site.bounds[reduction].hi).contains(&totals.partials[reduction][e]) {
                    continue;
                }
                let Ok(tile) = tir_root_claim_finalizes_to_v1(&capture.binding, index, &root.elements, &totals, &store, rules) else {
                    continue;
                };
                let Ok(values) = tile.into_iter().map(i32::try_from).collect::<Result<Vec<_>, _>>() else { continue };
                let mut forged = capture.clone();
                forged.leaves[index as usize].values_le = values.iter().flat_map(|x| x.to_le_bytes()).collect();
                reroot(&mut forged, holding.class_id);
                let Ok(carriage) = holding.root_claim(&forged, index, rules) else { continue };
                let lying =
                    PalwTirRootClaimV1 { version: root.version, elements: root.elements.clone(), totals, finalize: carriage.finalize };
                if check_tir_root_claim_v1(&lying, index, rules).is_ok() {
                    return Some((forged, lying));
                }
            }
        }
    }
    None
}

fn pushed_children(
    mut truth: kaspa_consensus_core::palw_tir_dissect_v1::PalwTirDissectRoundV1,
    phase: &PalwTirDissectPhaseV1,
    site: &PalwTirDissectSiteV1,
) -> kaspa_consensus_core::palw_tir_dissect_v1::PalwTirDissectRoundV1 {
    for (k, fold) in site.folds.iter().enumerate() {
        for e in 0..phase.elements()[k].len() {
            let claimed = phase.claim().partials[k][e];
            match fold {
                PalwTirFoldV1::Sum => {
                    let sum: i128 = truth.children.iter().map(|c| c.partials[k][e]).sum();
                    truth.children[0].partials[k][e] += claimed - sum;
                }
                PalwTirFoldV1::Max => {
                    let maximum = truth.children.iter().map(|c| c.partials[k][e]).max().unwrap();
                    if claimed > maximum {
                        truth.children[0].partials[k][e] = claimed;
                    } else {
                        for child in &mut truth.children {
                            child.partials[k][e] = child.partials[k][e].min(claimed);
                        }
                    }
                }
            }
        }
    }
    truth
}

#[test]
fn proven_partial_rows_play_honest_and_forged_history_courts_for_every_corpus_site() {
    let rules = rules();
    let dir = std::env::temp_dir().join(format!("tir-exec-partial-court-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut models, mut sites, mut reductions, mut maximum_close, mut maximum_root, mut maximum_round) = (0, 0, 0, 0, 0, 0);
    for (k, (name, program, params)) in node_common::programs().into_iter().enumerate() {
        let Ok(intervals) = analyze_ranges(&program) else { continue };
        if program.schedule.layers.len() < 2 {
            continue;
        }
        let path = dir.join(format!("{k}.palwtir"));
        let lay = node_common::layout(&program, 5, 2, 2, 64);
        let mut tensor = |j, l| params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| "missing tensor".to_string());
        misaka_palw_tir_artifact::write_container_v1(
            &path,
            &program,
            borsh::to_vec(&lay).unwrap(),
            [2; 64],
            name.clone(),
            &mut tensor,
        )
        .unwrap();
        let artifact = Arc::new(TirArtifactV1::open(&path).unwrap());
        let (artifact_root, _) = artifact.inventory_root().unwrap();
        let class = artifact.class().unwrap();
        let class_id = class.class_id(&artifact_root);
        let canonical = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class_id, (4, 3)).unwrap();
        let full =
            TirBackendV1::new(name.clone(), artifact, artifact_root, canonical, PalwPromptIdsFormV1::Flat, rules.max_step_leaf_count)
                .unwrap();
        let (job, prompt) = full.job_for_anchor(Hash64::from_bytes([k as u8 + 1; 64])).unwrap();
        let honest = full.execute(&job, &prompt).unwrap();
        let capture = TirCaptureV1::decode(&honest.material).unwrap();
        let last = full.space().job_shape(&job).unwrap().positions - 1;
        let (parts, _) = tir_shard_geometry_over_v1(full.space(), 2).unwrap();
        let mut model_sites = 0;
        for index in 0..capture.binding.step_leaf_count {
            let leaf = full.space().leaf_at(&job, index).unwrap();
            if leaf.position != last {
                continue;
            }
            let Some(site) = palw_tir_dissect_site_v1(full.space(), &intervals, &leaf) else { continue };
            let PalwTirLeafKindV1::Commit { occurrence, .. } = leaf.kind else { panic!("dissected commit") };
            let shard = parts.iter().position(|part| part.contains(&(occurrence as usize))).unwrap() as u16;
            let partial = fetch_shard_params_v1(&class, class_id, artifact_root, 2, shard, &TirFileMirrorV1(path.clone())).unwrap();
            let root = partial.root_claim(&capture, index, &rules).unwrap_or_else(|e| panic!("{name} leaf {index}: {e}"));
            assert_eq!(root, full.root_claim(&honest.material, index, &rules).unwrap(), "{name}: partial root is canonical");
            assert_eq!(check_tir_root_claim_v1(&root, index, &rules).unwrap(), site);
            maximum_root = maximum_root.max(borsh::object_length(&root).unwrap());
            let mut phase = PalwTirDissectPhaseV1::open(SESSION, index, &site, &root, 2, 0, 10).unwrap();
            let mut daa = 1;
            while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
                let round = partial.dissect_round(&capture, &phase, &rules).unwrap();
                assert_eq!(round, full.dissect_round(&honest.material, &phase, &rules).unwrap(), "{name}: same child partials");
                maximum_round = maximum_round.max(borsh::object_length(&round).unwrap());
                phase.apply_round(&round, daa, 10).unwrap();
                assert_eq!(tir_dissect_choice_v1(&phase, &round), None, "honest children yield no allegation");
                let choice = PalwTirDissectChoiceV1 {
                    version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
                    session_id: SESSION,
                    round: phase.round(),
                    child: (round.children.len() - 1) as u8,
                };
                phase.apply_choice(&choice, daa + 1, 10).unwrap();
                daa += 2;
            }
            let bottom = partial.dissect_bottom(&capture, &phase, &rules).unwrap();
            assert_eq!(bottom, full.dissect_bottom(&honest.material, &phase, &rules).unwrap());
            assert_eq!(check_tir_dissect_bottom_v1(&phase, &bottom, index, &rules), Err(PalwStepRefuteError::NoFaultFound));
            let mut close = kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2::TirDissection { bottom: Box::new(bottom) };
            close.tir_strip_program_v1();
            let close_bytes = borsh::object_length(&close).unwrap();
            assert!(
                close_bytes as u64 <= DEFAULT_MAX_CLOSE_BYTES,
                "{name}: carried close {close_bytes} exceeds the actual byte ceiling"
            );
            maximum_close = maximum_close.max(close_bytes);
            for reduction in 0..site.reductions.len() {
                let (forged, lie) = lying_root(&partial, &capture, index, &root, &site, reduction, &rules)
                    .unwrap_or_else(|| panic!("{name} leaf {index} reduction {reduction}: no admissible lie"));
                let forged_material = forged.encode();
                let mut phase = PalwTirDissectPhaseV1::open(SESSION, index, &site, &lie, 2, 0, 10).unwrap();
                let mut daa = 1;
                while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
                    let truth = partial.dissect_round(&forged, &phase, &rules).unwrap();
                    assert_eq!(truth, full.dissect_round(&forged_material, &phase, &rules).unwrap());
                    let pushed = pushed_children(truth.clone(), &phase, &site);
                    phase
                        .apply_round(&pushed, daa, 10)
                        .unwrap_or_else(|e| panic!("{name} reduction {reduction}: liar's round must fold legally: {e}"));
                    let child = tir_dissect_choice_v1(&phase, &truth).expect("false range has a different child");
                    let choice = PalwTirDissectChoiceV1 {
                        version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
                        session_id: SESSION,
                        round: phase.round(),
                        child,
                    };
                    phase.apply_choice(&choice, daa + 1, 10).unwrap();
                    daa += 2;
                }
                let bottom = partial.dissect_bottom(&forged, &phase, &rules).unwrap();
                assert_eq!(bottom, full.dissect_bottom(&forged_material, &phase, &rules).unwrap());
                assert!(
                    check_tir_dissect_bottom_v1(&phase, &bottom, index, &rules).is_ok(),
                    "{name}: legal false reduction convicted at bottom"
                );
                reductions += 1;
            }
            // Bound/root corruption is evidence refusal, never a conviction.
            let mut corrupt = capture.clone();
            corrupt.leaves[index as usize].values_le[0] ^= 1;
            assert!(partial.dissect_bottom(&corrupt, &phase, &rules).is_err());
            let mut fold = capture.clone();
            fold.leaves.clear();
            assert!(partial.dissect_round(&fold, &phase, &rules).is_err());
            sites += 1;
            model_sites += 1;
        }
        if model_sites > 0 {
            models += 1;
            eprintln!("RFC06_PARTIAL_COURT model={name:?} sites={model_sites} held_shards=2 positions={}", last + 1);
        }
        std::fs::remove_file(&path).unwrap();
    }
    std::fs::remove_dir(&dir).unwrap();
    eprintln!(
        "RFC06_PARTIAL_COURT models={models} sites={sites} forged_reductions={reductions} max_root_bytes={maximum_root} max_round_bytes={maximum_round} max_close_bytes={maximum_close}"
    );
    assert!(models >= 3 && sites >= models && reductions >= sites, "history corpus must be exercised");
}
