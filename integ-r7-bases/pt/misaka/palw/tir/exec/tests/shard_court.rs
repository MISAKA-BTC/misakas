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
use kaspa_consensus_core::palw_step_leg::{
    PalwStepRangeOpeningV1, step_merkle_range_siblings_v1, step_merkle_root_v1, step_tile_leaf_hash_v1,
};
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_tir_court_v1::{
    PalwTirBuilderMeterV1, PalwTirBuilderWorkLimitsV1, PalwTirCourtRulesV1, build_tir_dissect_bottom_bounded_v1,
    build_tir_dissect_bottom_v1, build_tir_dissect_round_bounded_v1, build_tir_dissect_round_v1, build_tir_root_claim_bounded_v1,
    build_tir_root_claim_v1, check_tir_dissect_bottom_v1, check_tir_root_claim_v1, tir_root_claim_finalizes_to_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirDissectPhaseV1, PalwTirDissectSiteV1, PalwTirFoldV1,
    PalwTirRootClaimV1, palw_tir_dissect_site_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::{PalwTirLeafKindV1, palw_tir_execution_root_v1};
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_exec::node::{
    TirArtifactV1, TirBackendV1, TirCaptureV1, TirClassRunnerV1, TirEvidenceV1, TirFileMirrorV1, TirRunInputsV1, TirShardHoldingV1,
    cell_runs_capped_v1, fetch_shard_params_v1, tir_court_builder_limits_v1, tir_dissect_choice_v1, tir_public_court_cells_v1,
    tir_shard_geometry_over_v1,
};
use std::sync::Arc;

const SESSION: Hash64 = Hash64::from_bytes([0x6D; 64]);
fn rules() -> PalwTirCourtRulesV1 {
    let court = PalwCourtParamsV2::with_cost_ceilings(
        1 << 26,
        10,
        2,
        DEFAULT_MAX_CLOSE_BYTES,
        DEFAULT_MAX_TERMINAL_MACS,
        DEFAULT_MAX_OPERAND_COUNT,
    )
    .unwrap();
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

// The exact conservative planner used by the actual public-court consumer. Answers
// are authenticated producer runs, not a verifier's separately replayed prefix.
fn public_court_inputs(holding: &TirShardHoldingV1, capture: &TirCaptureV1, named: u64) -> (TirCaptureV1, TirRunInputsV1, usize) {
    let cells = tir_public_court_cells_v1(&holding.space, &capture.binding.job_context, named, holding.shard).unwrap();
    let leaf = holding.space.leaf_at(&capture.binding.job_context, named).unwrap();
    let PalwTirLeafKindV1::Commit { occurrence, .. } = leaf.kind else { panic!("commit") };
    let cell = cells.last().unwrap();
    assert_eq!(cell.occ, occurrence as usize..occurrence as usize + 1);
    assert_eq!(cell.positions, 0..leaf.position + 1);
    let hashes: Vec<_> = capture
        .leaves
        .iter()
        .map(|l| step_tile_leaf_hash_v1(&capture.binding.job_context.context_hash(), &holding.class_id, l))
        .collect();
    let mut inputs = TirRunInputsV1::default();
    let mut bytes = 0;
    for (first, count, _) in
        cells.iter().flat_map(|cell| cell_runs_capped_v1(&holding.space, &capture.binding.job_context, cell, 65_536).unwrap())
    {
        let first = first as usize;
        let end = first + count as usize;
        assert!(
            holding.space.leaf_at(&capture.binding.job_context, (end - 1) as u64).unwrap().position <= leaf.position,
            "the public court never fetches future positions"
        );
        let range = PalwStepRangeOpeningV1 {
            first_leaf_index: first as u64,
            leaf_hashes: hashes[first..end].to_vec(),
            siblings: step_merkle_range_siblings_v1(&hashes, first, count as usize).unwrap(),
        };
        inputs
            .add_run(
                capture.binding.step_leaf_count,
                &capture.binding.step_merkle_root,
                &range,
                &capture.leaves[first..end],
                &capture.binding.job_context,
                &holding.class_id,
                1 << 26,
            )
            .unwrap();
        // Measure the actual stripped DA disclosure bytes rather than just lane bytes.
        let mut binding = capture.binding.clone();
        kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut binding);
        bytes += borsh::object_length(&kaspa_consensus_core::palw_tir_court_v1::PalwTirStepRunDisclosureV1 {
            binding,
            preimages: capture.leaves[first..end].to_vec(),
            range,
        })
        .unwrap();
    }
    assert!(inputs.leaves() < capture.binding.step_leaf_count as usize, "one occurrence cannot acquire the whole job");
    let mut fold = capture.clone();
    fold.leaves.clear();
    (fold, inputs, bytes)
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

// Global histories are written by pre and may be read by post. The public court's
// closure must include those authenticated units across a different occurrence.
fn global_post_history() -> (String, misaka_palw_tir::TirProgramV1, misaka_palw_tir::MapParams) {
    use misaka_palw_tir::{DType, Ref as R, Tensor, TensorType, builder::ProgramBuilder};
    let mut pb = ProgramBuilder::new(16, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
    let emb = pb.param("embed", DType::I8, &[16, 4], false);
    let hist = pb.hist_state("global.rows", DType::I8, &[4], 5, false);
    let carry = TensorType::fixed(DType::I8, &[4]);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(emb, R::Input(0), 0, 0);
        b.hist_append(hist, x);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", vec![carry.clone()]);
        let x = b.cast(R::CarryIn(0), DType::I8);
        b.finish(&[x])
    };
    let (post, logits) = {
        let mut b = pb.block("post", vec![carry]);
        let rows = b.cast(R::State(hist), DType::I32);
        let rows = b.commit(rows);
        let sum = b.reduce_sum(rows, 0, DType::I32);
        let sum = b.reshape_fixed(sum, &[4]);
        let logits = b.commit(sum);
        let R::Node(node) = logits else { unreachable!() };
        (b.finish(&[]), node)
    };
    let program = node_common::flat(pb.finish(pre, vec![layer, layer], post, logits));
    let mut params = misaka_palw_tir::MapParams::default();
    params.tensors.insert((0, None), Tensor::new(DType::I8, vec![16, 4], (0..64).map(|i| i % 11 - 5).collect()).unwrap());
    ("global-pre-history-post-reduction".into(), program, params)
}

fn exact_builder_limit(meter: PalwTirBuilderMeterV1) -> PalwTirBuilderWorkLimitsV1 {
    PalwTirBuilderWorkLimitsV1 {
        evaluation: misaka_palw_tir::demand::DemandLimits { max_elements: meter.work.elements, max_terms: meter.work.terms },
        max_target_requests: meter.target_requests,
        max_source_reads: meter.source_reads,
    }
}

fn record_builder_maximum(maximum: &mut PalwTirBuilderMeterV1, meter: PalwTirBuilderMeterV1) {
    maximum.work.elements = maximum.work.elements.max(meter.work.elements);
    maximum.work.terms = maximum.work.terms.max(meter.work.terms);
    maximum.target_requests = maximum.target_requests.max(meter.target_requests);
    maximum.source_reads = maximum.source_reads.max(meter.source_reads);
}

#[test]
fn proven_partial_rows_play_honest_and_forged_history_courts_for_every_corpus_site() {
    let rules = rules();
    let dir = std::env::temp_dir().join(format!("tir-exec-partial-court-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut models, mut sites, mut reductions, mut maximum_close, mut maximum_root, mut maximum_round) = (0, 0, 0, 0, 0, 0);
    let (mut maximum_court_leaves, mut maximum_court_da_bytes) = (0, 0);
    let mut maximum_builder = PalwTirBuilderMeterV1::default();
    for (k, (name, program, params)) in node_common::programs().into_iter().chain(std::iter::once(global_post_history())).enumerate() {
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
            if leaf.position != last && leaf.position != last.saturating_sub(1) {
                continue;
            }
            let Some(site) = palw_tir_dissect_site_v1(full.space(), &intervals, &leaf) else { continue };
            let PalwTirLeafKindV1::Commit { occurrence, .. } = leaf.kind else { panic!("dissected commit") };
            let shard = parts
                .iter()
                .position(|part| {
                    kaspa_consensus_core::palw_tir_shard_v1::palw_tir_shard_occurrences_v1(part, program.schedule.layers.len())
                        .contains(&(occurrence as usize))
                })
                .unwrap() as u16;
            let partial = fetch_shard_params_v1(&class, class_id, artifact_root, 2, shard, &TirFileMirrorV1(path.clone())).unwrap();
            let (fold, inputs, da_bytes) = public_court_inputs(&partial, &capture, index);
            maximum_court_leaves = maximum_court_leaves.max(inputs.leaves());
            maximum_court_da_bytes = maximum_court_da_bytes.max(da_bytes);
            let root = partial.root_claim(&capture, index, &rules).unwrap_or_else(|e| panic!("{name} leaf {index}: {e}"));
            let runner = TirClassRunnerV1::new(&partial.space, &partial.plan, &partial.params, class_id).unwrap();
            let store = TirEvidenceV1::dense(
                &runner,
                &capture.binding,
                &capture.prompt,
                &capture.logits_rows,
                &capture.generated,
                &capture.leaves,
                &partial,
                rules.prompt_form,
                rules.max_step_leaf_count,
            )
            .unwrap();
            assert_eq!(
                root,
                build_tir_root_claim_v1(&capture.binding, index, &store, &rules).unwrap(),
                "bounded node and historical root builder produce the same object"
            );
            let (metered_root, meter) =
                build_tir_root_claim_bounded_v1(&capture.binding, index, &store, &rules, tir_court_builder_limits_v1(&rules)).unwrap();
            assert_eq!(metered_root, root);
            if meter.source_reads > 0 {
                let mut insufficient = exact_builder_limit(meter);
                insufficient.max_source_reads -= 1;
                assert!(
                    build_tir_root_claim_bounded_v1(&capture.binding, index, &store, &rules, insufficient).is_err(),
                    "root source reads are shared across reductions, finalize and every closure pass"
                );
            }
            record_builder_maximum(&mut maximum_builder, meter);
            assert_eq!(
                build_tir_root_claim_bounded_v1(&capture.binding, index, &store, &rules, exact_builder_limit(meter)).unwrap().0,
                root
            );
            let mut insufficient = exact_builder_limit(meter);
            insufficient.max_target_requests -= 1;
            assert!(
                build_tir_root_claim_bounded_v1(&capture.binding, index, &store, &rules, insufficient).is_err(),
                "root target bound is shared across full reductions, finalize and closure"
            );
            assert_eq!(root, full.root_claim(&honest.material, index, &rules).unwrap(), "{name}: partial root is canonical");
            assert_eq!(check_tir_root_claim_v1(&root, index, &rules).unwrap(), site);
            maximum_root = maximum_root.max(borsh::object_length(&root).unwrap());
            let mut phase = PalwTirDissectPhaseV1::open(SESSION, index, &site, &root, 2, 0, 10).unwrap();
            let mut daa = 1;
            while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
                let round = partial.dissect_round(&capture, &phase, &rules).unwrap();
                let (metered, meter) = build_tir_dissect_round_bounded_v1(
                    &capture.binding,
                    &phase,
                    partial.space.layout.h_tile,
                    &store,
                    &rules,
                    tir_court_builder_limits_v1(&rules),
                )
                .unwrap();
                assert_eq!(metered, round);
                if meter.source_reads > 0 {
                    let mut insufficient = exact_builder_limit(meter);
                    insufficient.max_source_reads -= 1;
                    assert!(
                        build_tir_dissect_round_bounded_v1(
                            &capture.binding,
                            &phase,
                            partial.space.layout.h_tile,
                            &store,
                            &rules,
                            insufficient
                        )
                        .is_err(),
                        "source read ceiling includes every child and reduction, with supplied/cache hits charged"
                    );
                }
                assert_eq!(
                    build_tir_dissect_round_v1(&capture.binding, &phase, partial.space.layout.h_tile, &store, &rules).unwrap(),
                    round
                );
                record_builder_maximum(&mut maximum_builder, meter);
                assert_eq!(
                    build_tir_dissect_round_bounded_v1(
                        &capture.binding,
                        &phase,
                        partial.space.layout.h_tile,
                        &store,
                        &rules,
                        exact_builder_limit(meter)
                    )
                    .unwrap()
                    .0,
                    round
                );
                let mut insufficient = exact_builder_limit(meter);
                insufficient.max_target_requests -= 1;
                assert!(
                    build_tir_dissect_round_bounded_v1(
                        &capture.binding,
                        &phase,
                        partial.space.layout.h_tile,
                        &store,
                        &rules,
                        insufficient
                    )
                    .is_err(),
                    "round target budget is not reset for every child/reduction"
                );
                if meter.work.terms > 0 || meter.work.elements > 0 {
                    let mut insufficient = exact_builder_limit(meter);
                    if meter.work.terms > 0 {
                        insufficient.evaluation.max_terms -= 1;
                    } else {
                        insufficient.evaluation.max_elements -= 1;
                    }
                    assert!(
                        build_tir_dissect_round_bounded_v1(
                            &capture.binding,
                            &phase,
                            partial.space.layout.h_tile,
                            &store,
                            &rules,
                            insufficient
                        )
                        .is_err(),
                        "whole-round evaluated work cap is exact"
                    );
                }
                assert_eq!(round, full.dissect_round(&honest.material, &phase, &rules).unwrap(), "{name}: same child partials");
                assert_eq!(
                    partial.dissect_round_from_runs(&fold, &inputs, &phase, &rules).unwrap(),
                    round,
                    "{name}: one occurrence/prefix closes the same honest round"
                );
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
            let (metered, meter) =
                build_tir_dissect_bottom_bounded_v1(&capture.binding, &phase, &store, &rules, tir_court_builder_limits_v1(&rules))
                    .unwrap();
            assert_eq!(metered, bottom);
            assert_eq!(
                build_tir_dissect_bottom_bounded_v1(&capture.binding, &phase, &store, &rules, exact_builder_limit(meter)).unwrap().0,
                bottom
            );
            if meter.source_reads > 0 {
                let mut insufficient = exact_builder_limit(meter);
                insufficient.max_source_reads -= 1;
                assert!(
                    build_tir_dissect_bottom_bounded_v1(&capture.binding, &phase, &store, &rules, insufficient).is_err(),
                    "bottom source reads refuse at the exact whole-move boundary"
                );
            }
            assert_eq!(build_tir_dissect_bottom_v1(&capture.binding, &phase, &store, &rules).unwrap(), bottom);
            record_builder_maximum(&mut maximum_builder, meter);
            assert_eq!(bottom, full.dissect_bottom(&honest.material, &phase, &rules).unwrap());
            assert_eq!(
                partial.dissect_bottom_from_runs(&fold, &inputs, &phase, &rules).unwrap(),
                bottom,
                "{name}: same honest bottom over requested committed inputs"
            );
            assert_eq!(check_tir_dissect_bottom_v1(&phase, &bottom, index, &rules), Err(PalwStepRefuteError::NoFaultFound));
            assert!(
                partial.dissect_bottom_from_runs(&fold, &TirRunInputsV1::default(), &phase, &rules).is_err(),
                "missing committed inputs cannot create a public bottom"
            );
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
                let (forged_fold, forged_inputs, _) = public_court_inputs(&partial, &forged, index);
                let mut phase = PalwTirDissectPhaseV1::open(SESSION, index, &site, &lie, 2, 0, 10).unwrap();
                // A lying reduction can round to the same committed output tile, hence
                // the same step root. Construct a guaranteed different authenticated root
                // for this negative control while preserving those admissible lie cases.
                let mut foreign = capture.clone();
                foreign.leaves[index as usize].values_le[0] ^= 1;
                reroot(&mut foreign, partial.class_id);
                assert_ne!(foreign.binding.step_merkle_root, capture.binding.step_merkle_root);
                foreign.leaves.clear();
                assert!(
                    partial.dissect_round_from_runs(&foreign, &inputs, &phase, &rules).is_err(),
                    "honest inputs under another step root cannot be reused for a re-rooted capture"
                );
                let mut daa = 1;
                while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
                    let truth = partial.dissect_round(&forged, &phase, &rules).unwrap();
                    assert_eq!(truth, full.dissect_round(&forged_material, &phase, &rules).unwrap());
                    assert_eq!(partial.dissect_round_from_runs(&forged_fold, &forged_inputs, &phase, &rules).unwrap(), truth);
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
                assert_eq!(partial.dissect_bottom_from_runs(&forged_fold, &forged_inputs, &phase, &rules).unwrap(), bottom);
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
            let mut borrowed_prompt = capture.clone();
            borrowed_prompt.prompt[0] = (borrowed_prompt.prompt[0] + 1) % program.token_bound;
            assert!(
                partial.named_leaf_close(&borrowed_prompt, index, &rules).is_err(),
                "matching root fields do not authenticate a borrowed prompt"
            );
            let mut borrowed_trace = capture.clone();
            borrowed_trace.logits_rows[0][0] ^= 1;
            assert!(
                partial.named_leaf_close(&borrowed_trace, index, &rules).is_err(),
                "matching trace root fields do not authenticate rows"
            );
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
        "RFC06_PARTIAL_COURT models={models} sites={sites} forged_reductions={reductions} max_root_bytes={maximum_root} max_round_bytes={maximum_round} max_close_bytes={maximum_close} max_court_run_leaves={maximum_court_leaves} max_court_da_bytes={maximum_court_da_bytes} max_builder={maximum_builder:?}"
    );
    assert!(models >= 3 && sites >= models && reductions >= sites, "history corpus must be exercised");
}
