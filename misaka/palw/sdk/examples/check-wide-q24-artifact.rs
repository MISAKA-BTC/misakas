//! Real-checkpoint wide output exercise: committed run, bounded capture, cells and both court doors.
use kaspa_consensus_core::palw_tir_court_v1::*;
use kaspa_consensus_core::palw_tir_step_v1::*;
use kaspa_consensus_core::{palw_logits_q24_v2 as wide, palw_step_refute::PalwStepRefuteError};
use misaka_palw_tir_exec::node::wide_q24::TirWideCaptureV2;
use misaka_palw_tir_exec::node::*;
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: check-wide-q24-artifact <artifact> <calibration.json> <fresh-report.json>".into());
    }
    let art = TirArtifactV1::open(Path::new(&args[0]))?;
    let class = art.class()?;
    let (root, _) = art.inventory_root()?;
    let id = class.class_id(&root);
    let space = PalwTirStepSpaceV1::new(&class)?;
    let public = misaka_palw_sdk::runtime_pack::build::network("testnet-12")?;
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(public_bundle) = &public.palw_consensus_mode else {
        unreachable!()
    };
    assert!(
        misaka_palw_sdk::tir_layout::tir_class_admission_offline_v1(&public, public_bundle, &class, root).is_err(),
        "public preset must not admit wide profile"
    );
    let runner = TirClassRunnerV1::new_wide_q24_v2(&space, art.plan(), art.params(), id)?;
    assert!(TirClassRunnerV1::new(&space, art.plan(), art.params(), id).is_err());
    let calib: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let prompt: Vec<u32> =
        calib["sequences"][0].as_array().ok_or("calibration sequences")?.iter().take(4).map(|x| x.as_u64().unwrap() as u32).collect();
    let mut ctx = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, id, (prompt.len() as u32, 2))
        .ok_or("context")?;
    let form = kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat;
    ctx.prompt_token_ids_hash = kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(form, &prompt)?;
    let mut leaves = Vec::new();
    let run = runner.run_wide_q24_v2(&ctx, &prompt, u64::MAX, &mut |l| leaves.push(l.preimage.clone()))?;
    let max = run.logits_rows.iter().flatten().map(|x| x.unsigned_abs()).max().ok_or("logits")?;
    assert!(max > i32::MAX as u64, "this is the actual large-logit case");
    let binding = PalwTirStepBindingV1 {
        version: PALW_TIR_STEP_BINDING_VERSION_V1,
        job_context: ctx.clone(),
        class: class.clone(),
        artifact_root: root,
        full_logits_trace_root: run.trace_root,
        step_leaf_count: run.leaf_count,
        step_merkle_root: run.step_merkle_root,
        committed_execution_root: run.execution_root,
    };
    let capture = TirWideCaptureV2 {
        binding,
        prompt: prompt.clone(),
        logits_rows: run.logits_rows.clone(),
        generated: run.generated.clone(),
        leaves,
    };
    let bytes = capture.encode();
    assert_eq!(TirWideCaptureV2::decode(&bytes)?, capture);
    assert!(TirCaptureV1::decode(&bytes).is_err());
    capture.authenticate(u64::MAX)?;
    let event = capture.event(0, 0).ok_or("event")?;
    let vocab = capture.logits_rows[0].len();
    event.authenticate(&ctx, &run.trace_root, vocab, u64::MAX)?;
    let mut bad = event.clone();
    bad.lanes[0] ^= 1;
    assert!(bad.authenticate(&ctx, &run.trace_root, vocab, u64::MAX).is_err());
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: 1 << 26,
        prompt_form: form,
        limits: misaka_palw_tir::demand::DemandLimits { max_elements: 1 << 26, max_terms: 1 << 26 },
    };
    let i = capture.leaves.iter().position(|l| l.version == 2).ok_or("wide leaf")?;
    let tree = TirStepTreeV1::full(&run.leaf_hashes);
    let opening = tree.opening(i as u64).ok_or("opening")?;
    let leaf = space.leaf_at(&ctx, i as u64).ok_or("leaf")?;
    let PalwTirLeafKindV1::Commit { first_element, .. } = leaf.kind else { return Err("not output".into()) };
    let evt = capture.event(0, (first_element / 4096) as u32).ok_or("output event")?;
    assert_eq!(
        check_tir_logits_consistency_wide_q24_v2(&capture.binding, &opening, &capture.leaves[i], &evt, &rules),
        Err(PalwStepRefuteError::NoFaultFound)
    );
    let store = TirEvidenceV1::dense_wide_q24_v2(&runner, &capture, &art, form, rules.max_step_leaf_count)?;
    let honest = store.cone_refutation(i as u64, &rules)?;
    assert_eq!(check_tir_cone_refutation_v1(&honest, &rules), Err(PalwStepRefuteError::NoFaultFound));
    // Commit one incorrect wide lane. The court must recompute it, not merely reject its hash.
    let mut forged = capture.clone();
    forged.leaves[i].values_le[0] ^= 1;
    let hashes: Vec<_> = forged
        .leaves
        .iter()
        .map(|p| kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx.context_hash(), &id, p))
        .collect();
    forged.binding.step_merkle_root =
        kaspa_consensus_core::palw_step_leg::step_merkle_root_capped_v1(&hashes, rules.max_step_leaf_count)?;
    forged.binding.committed_execution_root =
        palw_tir_execution_root_v1(&ctx.context_hash(), &run.trace_root, &id, run.leaf_count, &forged.binding.step_merkle_root);
    let forged_store = TirEvidenceV1::dense_wide_q24_v2(&runner, &forged, &art, form, rules.max_step_leaf_count)?;
    let refute = forged_store.cone_refutation(i as u64, &rules)?;
    assert!(check_tir_cone_refutation_v1(&refute, &rules).is_ok());
    // Decode fault: bind an incorrect ID to a new trace commitment and open the real winning lane.
    let mut bad_decode = capture.clone();
    let correct = bad_decode.generated[0];
    bad_decode.generated[0] = (correct + 1) % vocab as u32;
    bad_decode.binding.full_logits_trace_root =
        wide::trace_root(&ctx, &bad_decode.logits_rows, &bad_decode.generated).ok_or("bad trace")?;
    bad_decode.binding.committed_execution_root = palw_tir_execution_root_v1(
        &ctx.context_hash(),
        &bad_decode.binding.full_logits_trace_root,
        &id,
        run.leaf_count,
        &run.step_merkle_root,
    );
    let c = bad_decode.event(0, bad_decode.generated[0] / 4096).ok_or("committed event")?;
    let b = bad_decode.event(0, correct / 4096).ok_or("beat event")?;
    assert!(
        check_tir_decode_wide_q24_v2(
            &bad_decode.binding,
            &c,
            &b,
            correct as usize,
            &kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY,
            &rules
        )
        .is_ok()
    );
    // A complete CPU cell uses the same committed leaves at 8 bytes only for final logits.
    let inputs = TirCaptureInputsV1::new(&capture.leaves, &ctx, &id, &run.step_merkle_root, rules.max_step_leaf_count)?;
    let cell = TirCellV1 { shard: 0, occ: 0..space.occurrences().len(), positions: 0..(prompt.len() as u32 + 1) };
    let mut tokens = prompt.clone();
    tokens.extend(&run.generated);
    let req = TirCellRequestV1 {
        space: &space,
        plan: art.plan(),
        params: art.params(),
        class_id: id,
        ctx: &ctx,
        cell: &cell,
        tokens: &tokens,
        inputs: &inputs,
        fused: false,
    };
    let mut cpu =
        misaka_palw_tir_exec::cellstep_v2::CpuCellStepperV2(misaka_palw_tir_exec::TirExecutor::new(art.plan(), art.params())?);
    assert!(matches!(
        misaka_palw_tir_exec::node::cell::verify_cell_stepping_wide_q24_v2(&req, &mut cpu),
        TirCellVerdictV1::Verified { .. }
    ));
    let report = serde_json::json!({"class_id":id.to_string(),"max_context":class.layout.max_context,"max_abs_q24":max,"max_abs_natural_units":max as f64/(1u64<<24) as f64,"wide_capture_roundtrip":true,"old_capture_refuses":true,"event_authentication":true,"altered_event_rejected":true,"honest_arithmetic_acquitted":true,"incorrect_arithmetic_convicted":true,"honest_trace_consistency_acquitted":true,"incorrect_decode_convicted":true,"wide_cell_verified":true,"public_preset_refuses":true,"public_network":false,"claim_certified":false});
    let out = std::fs::OpenOptions::new().write(true).create_new(true).open(&args[2])?;
    serde_json::to_writer_pretty(out, &report)?;
    println!("{report}");
    Ok(())
}
