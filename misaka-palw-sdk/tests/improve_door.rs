//! **RFC-0004 A10 end to end against the chain's evaluation door (A6, 127058973)**: a claim the node's
//! executor builds — run on a held subject, assembled, signed and funded as the panel carries it — passes
//! every stateless rule the chain applies to a version-9 payload, at the three places it applies them:
//!
//! * the *isolation* door (mempool, block body, template): the payload decodes as an evaluation claim's,
//!   its FP stand-in passes every rule of FP Job V4, and the claim's own rules hold;
//! * the *header-context* door: nothing refuses it past `palw_improvement_v1` and `palw_fp_decode_rules`;
//! * the *acceptance walk*: it becomes the `FreePromptCommitted` object the fold's evaluation branch reads,
//!   its job and tail in `eval`, its signature verified under the key it carries.
//!
//! And a seat that derives the task from the claim's own job and tail (what the extractor hands the fold)
//! replays it to the claim's roots.

use std::borrow::Cow;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpDecodeRulesV1, PalwFpJobTailV1};
use kaspa_consensus_core::palw_improve_eval_v1::{
    PalwEvalJobV1, PalwEvalModeV1, PalwEvalStageParamsV1, palw_fp_eval_objects_from_accepted_txs_v1, palw_fp_eval_refusal_at_v1,
    palw_fp_eval_payload_decode_v1, palw_improve_eval_decode_config_v1, validate_palw_fp_commitment_tx_under_v6,
};
use kaspa_consensus_core::palw_improve_state_v1::{PalwEvalSubjectV1, PalwScoringKindV1, palw_improve_eval_seed_v1};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_root_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
use misaka_palw_sdk::improve::PalwImproveEvalTaskV1;
use misaka_palw_sdk::improve_eval::{
    PalwEvalClaimFactsV1, PalwEvalHeldV1, PalwEvalSeatJudgmentV1, palw_eval_claim_v1, palw_eval_run_v1, palw_eval_seat_judge_v1,
    palw_eval_task_of_claim_v1,
};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::interp::MapParams;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{DType, Ref, Tensor, TensorType, TirProgramV1};

/// A toy IR class: an `i8` embedding, two layers of a per-layer `i8 [4, 4]` matrix and `i64` multiplier,
/// and an `i16` head over 16 ids (the evaluation lane's own fixture).
fn program() -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let table = pb.param("embed.table", DType::I8, &[16, 4], false);
    let w = pb.param("blk.w", DType::I8, &[4, 4], true);
    let m = pb.param("blk.m", DType::I64, &[4], true);
    let head = pb.param("head.w", DType::I16, &[16, 4], false);
    let carry = vec![TensorType::fixed(DType::I32, &[4])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let row = b.gather(table, Ref::Input(0), 0, 0);
        let row = b.cast(row, DType::I32);
        b.finish(&[row])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let acc = b.matmul(w, x, DType::I64);
        let acc = b.reshape_fixed(acc, &[4]);
        let y = b.mul(acc, m, DType::I128);
        let y = b.shr(y, 20, misaka_palw_tir::Rounding::HalfAwayFromZero, DType::I128);
        let y = b.clamp(y, -30_000, 30_000, DType::I32);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let l = b.matmul(head, x, DType::I64);
        let l = b.reshape_fixed(l, &[16]);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let mut p = pb.finish(pre, vec![layer, layer], post, logits);
    p.logits_scheme_id.copy_from_slice(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
    p
}

fn params_of(p: &TirProgramV1) -> MapParams {
    let mut out = MapParams::default();
    for (j, inst) in kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_param_instances_v1(p).into_iter().enumerate() {
        let d = &p.params[j];
        for l in inst {
            let n: usize = d.shape.iter().map(|x| *x as usize).product();
            let data: Vec<i128> = (0..n)
                .map(|i| {
                    let v = ((i * 37 + j * 11 + l.map_or(0, |l| l as usize) * 5) % 200) as i128 - 100;
                    if d.dtype == DType::I64 { v.abs() * 9_000 + 1 } else { v }
                })
                .collect();
            out.tensors.insert((j as u16, l), Tensor::new(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), data).unwrap());
        }
    }
    out
}

struct Src<'a>(&'a MapParams);
impl PalwTirTensorSourceV1 for Src<'_> {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.tensors.get(&(param, layer)).map(|t| Cow::Owned(t.to_le_bytes()))
    }
}

fn held() -> PalwEvalHeldV1 {
    let program = program();
    let params = params_of(&program);
    let commits = program.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
    let layout = PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: 12,
        checkpoint_interval: 1,
        h_tile: 16,
        commit_tiles: vec![4; commits],
        state_tiles: vec![4; program.states.len()],
    };
    let (root, _) = palw_tir_inventory_root_v1(&program, &Src(&params)).unwrap();
    PalwEvalHeldV1::from_map(Hash64::from_bytes([0x22; 64]), root, Hash64::from_bytes([0x14; 64]), program, layout, params)
}

/// The shipped testnet-12 ruleset's free-prompt parameters, the form and the decode rules the door runs.
fn freeprompt() -> kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptParamsV3 {
    use kaspa_consensus_core::config::params::Params;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    let params = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    match params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.freeprompt,
        _ => panic!("testnet-12 runs the ConsensusV2 bundle"),
    }
}

#[test]
fn a_claim_the_node_builds_passes_the_chains_door_the_walk_and_a_seats_replay() {
    let key = kaspa_pq_validator_core::ValidatorKey::from_seed([0x61; 32]);
    let held = held();
    // The item's generation, under the policy's budget; the ExactMatch scoring is the fold's, at the key's reveal.
    let epoch_seed = Hash64::from_bytes([0x33; 64]);
    let seed = palw_improve_eval_seed_v1(&epoch_seed, 7);
    let mode = PalwEvalModeV1::Generate { seed, max_new: 4, stop_ids: vec![] };
    let subject = PalwEvalSubjectV1::Candidate(held.class_id);
    let job = PalwEvalJobV1 { line_id: Hash64::from_bytes([0x11; 64]), epoch: 3, item: 7, subject, kind: PalwScoringKindV1::ExactMatch, mode: mode.clone() };
    let task = PalwImproveEvalTaskV1 {
        line_id: job.line_id,
        epoch: 3,
        item: 7,
        subject,
        subject_class: held.class_id,
        kind: PalwScoringKindV1::ExactMatch,
        mode,
        job_id: job.id(),
        prompt_ids: vec![3, 5, 1],
        reference_ids: vec![],
        params: PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 },
    };
    let work = palw_eval_run_v1(&held, &task).expect("the run");

    // The executor's facts, as the panel reads them: the drill network's domain (the shipped t12 genesis here),
    // the bond's key, a recent anchor.
    let t12 = kaspa_consensus_core::config::params::Params::from(kaspa_consensus_core::network::NetworkId::with_suffix(
        kaspa_consensus_core::network::NetworkType::Testnet,
        12,
    ));
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(t12.net.to_string().as_bytes(), Some(t12.genesis.hash));
    let form = PalwPromptIdsFormV1::Flat;
    let facts = PalwEvalClaimFactsV1 {
        network_domain: domain,
        executor_bond: kaspa_consensus_core::config::premine::premine_outpoint(2),
        executor_pubkey: key.public_key().to_vec(),
        operator_id: Hash64::from_bytes([0x12; 64]),
        anchor_block: Hash64::from_bytes([0x13; 64]),
        anchor_daa: 99,
        prompt_ids_form: form,
        trace_retention_daa: 5_000,
    };
    let claim = palw_eval_claim_v1(&work, &held, &facts).expect("a claim the executor's own check admits");

    // Signed and funded as the panel carries it.
    let funding_outpoint = TransactionOutpoint::new(Hash64::from_bytes([0xF0; 64]).into(), 0);
    let funding = UtxoEntry::new(10_000_000, ScriptPublicKey::from_vec(0, vec![0x51]), 0, false);
    let tx = key
        .build_fp_eval_commitment_tx(claim.commitment.clone(), &claim.tail, claim.prompt.clone(), funding_outpoint, &funding, 1_000_000)
        .expect("signed and funded");
    assert_eq!(tx.subnetwork_id, SUBNETWORK_ID_PALW_FP_COMMITMENT);

    // 1. The isolation door: the evaluation door where the ruleset carries the fence; the FP door refuses the same bytes.
    let freeprompt = freeprompt();
    let cap = 1u64 << 32;
    for rules in [PalwFpDecodeRulesV1::Scheduled, PalwFpDecodeRulesV1::Active] {
        assert_eq!(
            validate_palw_fp_commitment_tx_under_v6(&tx.payload, false, form, cap, rules, true),
            Ok(()),
            "the evaluation door admits the claim under {rules:?}"
        );
    }
    assert!(
        validate_palw_fp_commitment_tx_under_v6(&tx.payload, false, form, cap, PalwFpDecodeRulesV1::Active, false).is_err(),
        "a build that does not carry the fence refuses the payload's bytes at its FP door"
    );
    // 2. The header-context door, at a height past both fences.
    assert_eq!(palw_fp_eval_refusal_at_v1(&tx.payload, true, true, true), None);
    assert!(palw_fp_eval_refusal_at_v1(&tx.payload, false, true, true).is_some(), "below palw_improvement_v1: refused by name");
    assert!(palw_fp_eval_refusal_at_v1(&tx.payload, true, false, true).is_some(), "below palw_fp_decode_rules: refused by name");
    // 3. The acceptance walk: the object the fold's evaluation branch reads.
    let extraction = palw_fp_eval_objects_from_accepted_txs_v1(
        std::slice::from_ref(&tx),
        domain,
        &freeprompt,
        false,
        |_| kaspa_consensus_core::palw_fp_objects_v3::PalwFpClassCapsV1 {
            step_ladder: cap,
            held: false,
            derived_work: kaspa_consensus_core::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Declared,
            logits_q24: true,
        },
        false,
        false,
        form,
        PalwFpDecodeRulesV1::Active,
        |pk, msg, sig, ctx| matches!(kaspa_txscript::verify_mldsa87_with_context(pk, msg, sig, ctx), Ok(true)),
    );
    assert!(extraction.skipped.is_empty(), "{:?}", extraction.skipped);
    let [carried] = &extraction.objects[..] else { panic!("one object") };
    let PalwConsensusObjectV2::FreePromptCommitted { claim: claim_id, class_id, work_leaves, eval, executor_pubkey, .. } = &carried.object else {
        panic!("a free-prompt commitment")
    };
    assert_eq!(*claim_id, claim.claim_id());
    assert_eq!(*class_id, held.class_id);
    assert_eq!(*work_leaves, work.execution.space.leaf_count());
    assert_eq!(executor_pubkey, key.public_key());
    let eval = eval.as_ref().expect("an evaluation claim carries its job and tail");
    assert_eq!(eval.job, job);

    // A seat derives the task from what the extractor hands the fold — the job and the tail — and replays it.
    let (payload, tail) = palw_fp_eval_payload_decode_v1(&tx.payload).expect("the payload decodes as an evaluation claim's");
    assert!(matches!(payload.commitment.job.tail, Some(PalwFpJobTailV1::Eval(_))));
    let derived = palw_eval_task_of_claim_v1(&payload.commitment, &payload.prompt_token_ids, &tail).expect("a task");
    assert_eq!((derived.job_id, derived.item, derived.epoch), (task.job_id, 7, 3));
    assert_eq!(palw_eval_seat_judge_v1(&held, &payload.commitment, &payload.prompt_token_ids, &tail), PalwEvalSeatJudgmentV1::Valid);
    let _ = (DecodeConfigV4::NOOP, palw_improve_eval_decode_config_v1(&[]));
}
