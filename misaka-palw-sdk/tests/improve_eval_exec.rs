//! **RFC-0004's evaluation runs its subject stage on the node's executor — the reference's run, byte for
//! byte** (`palw_eval_run_v1`; `misaka_palw_tir_exec::stage`; `docs/design/palw/tir/runtime-residency.md`
//! §8).
//!
//! Every evaluation here is run twice, by [`palw_eval_run_v1`] (the subject stage on the typed executor
//! over the held weights, the scoring stages on the reference) and by [`palw_eval_run_reference_v1`]
//! (every stage on the reference interpreter), and the two works must be one: the pipeline's run (every
//! stage's outputs, commit points and `Fixed` states), every leaf's values and hashes, the claim's roots,
//! the stop, the binding and the tail. Generating and teacher-forced (with the likelihood's scoring
//! stages), on the toy class the A6 vectors and the door's walk use, and on Hugging Face fixtures lowered
//! as the converter lowers them — mixtures, a dense decoder and a hybrid with recurrent states — mapped
//! and held at their floor with every row-addressed param served by rows. Then batches: tasks over one
//! parent's weights stepped together by one hub ([`palw_eval_run_batch_v1`]) are each the run they are
//! alone, a resident mixture's batch reads its routed rows once rather than once per member, and a LoRA
//! candidate served from its adapter section over its held parent steps together with the parent.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kaspa_consensus_core::palw_improve_eval_v1::{PalwEvalModeV1, PalwEvalStageParamsV1};
use kaspa_consensus_core::palw_improve_state_v1::{PalwEvalSubjectV1, PalwScoringKindV1, palw_improve_eval_seed_v1};
use kaspa_consensus_core::palw_step_refute::{PALW_LOGITS_TILE_LANES, tiled_logits_scheme_id_v1};
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_root_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_hashes::Hash64;
use misaka_palw_sdk::improve::PalwImproveEvalTaskV1;
use misaka_palw_sdk::improve_eval::{
    PalwEvalHeldV1, PalwEvalSeatJudgmentV1, PalwEvalWorkV1, palw_eval_claim_v1, palw_eval_lockstep_width_v1, palw_eval_run_batch_v1,
    palw_eval_run_reference_v1, palw_eval_run_v1, palw_eval_seat_judge_v1,
};
use misaka_palw_sdk::lineages::tir::TirLineageV1;
use misaka_palw_sdk::{PalwModelLineageV1, PalwTirClassEntryV1, PalwWeightResidencyV1};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, TirProgramV1};
use misaka_palw_tir_exec::node::{TirArtifactV1, TirResidencyPolicyV1};
use misaka_palw_tir_exec::{TirTierRulesV1, TirTiersV1};

const CONTEXT: u32 = 32;
const ALL_ROWS: TirTierRulesV1 = TirTierRulesV1 { pin_below_bytes: 0 };

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("palw-sdk-eval-exec-{name}-{}", std::process::id()));
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

// ---------------------------------------------------------------------------------------------
// Tasks, and two works that must be one
// ---------------------------------------------------------------------------------------------

fn task(
    held: &PalwEvalHeldV1,
    item: u32,
    kind: PalwScoringKindV1,
    mode: PalwEvalModeV1,
    prompt: Vec<u32>,
    reference: Vec<u32>,
    params: PalwEvalStageParamsV1,
) -> PalwImproveEvalTaskV1 {
    let subject = PalwEvalSubjectV1::Candidate(held.class_id);
    let mut t = PalwImproveEvalTaskV1 {
        line_id: Hash64::from_bytes([0x11; 64]),
        epoch: 3,
        item,
        subject,
        subject_class: held.class_id,
        kind,
        mode,
        job_id: Hash64::default(),
        prompt_ids: prompt,
        reference_ids: reference,
        params,
    };
    t.job_id = t.job().id();
    t
}

fn generating(held: &PalwEvalHeldV1, item: u32, prompt: Vec<u32>, max_new: u32) -> PalwImproveEvalTaskV1 {
    let seed = palw_improve_eval_seed_v1(&Hash64::from_bytes([0x33; 64]), item);
    task(
        held,
        item,
        PalwScoringKindV1::ExactMatch,
        PalwEvalModeV1::Generate { seed, max_new, stop_ids: vec![] },
        prompt,
        vec![],
        PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 },
    )
}

fn forced(held: &PalwEvalHeldV1, item: u32, prompt: Vec<u32>, reference: Vec<u32>) -> PalwImproveEvalTaskV1 {
    task(
        held,
        item,
        PalwScoringKindV1::RefLogLik,
        PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::from_bytes([0x44; 64]) },
        prompt,
        reference,
        PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 12 },
    )
}

/// Two works are one run: the pipeline's every stage, every leaf, the roots, the stop, the binding, the tail.
fn assert_same(a: &PalwEvalWorkV1, b: &PalwEvalWorkV1, what: &str) {
    assert_eq!(a.execution.run, b.execution.run, "{what}: the pipeline's run (every output, commit point and state)");
    assert_eq!(a.execution.leaf_values, b.execution.leaf_values, "{what}: every leaf's values");
    assert_eq!(a.execution.leaf_hashes, b.execution.leaf_hashes, "{what}: every leaf's hash");
    assert_eq!(a.execution.claim, b.execution.claim, "{what}: the roots and the ids");
    assert_eq!(a.execution.stop, b.execution.stop, "{what}: the stop");
    assert_eq!(a.binding, b.binding, "{what}: the binding");
    assert_eq!(a.tail, b.tail, "{what}: the tail");
}

/// The task on the executor and on the reference: one work, returned.
fn both(held: &PalwEvalHeldV1, t: &PalwImproveEvalTaskV1, what: &str) -> PalwEvalWorkV1 {
    let exec = palw_eval_run_v1(held, t).unwrap_or_else(|e| panic!("{what}: the executor's run: {e}"));
    let reference = palw_eval_run_reference_v1(held, t).unwrap_or_else(|e| panic!("{what}: the reference's run: {e}"));
    assert_same(&exec, &reference, what);
    exec
}

// ---------------------------------------------------------------------------------------------
// The toy class (A6's fixture: the eval vectors', the door's)
// ---------------------------------------------------------------------------------------------

fn toy_program() -> TirProgramV1 {
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
    p.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    p
}

fn toy_params(p: &TirProgramV1, salt: usize) -> MapParams {
    let mut out = MapParams::default();
    for (j, inst) in kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_param_instances_v1(p).into_iter().enumerate() {
        let d = &p.params[j];
        for l in inst {
            let n: usize = d.shape.iter().map(|x| *x as usize).product();
            let data: Vec<i128> = (0..n)
                .map(|i| {
                    let v = ((i * 37 + j * 11 + salt * 13 + l.map_or(0, |l| l as usize) * 5) % 200) as i128 - 100;
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

fn toy_layout(p: &TirProgramV1) -> PalwTirLayoutV1 {
    let commits = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: 12,
        checkpoint_interval: 1,
        h_tile: 16,
        commit_tiles: vec![4; commits],
        state_tiles: vec![4; p.states.len()],
    }
}

fn toy(salt: usize) -> PalwEvalHeldV1 {
    let program = toy_program();
    let params = toy_params(&program, salt);
    let layout = toy_layout(&program);
    let (root, _) = palw_tir_inventory_root_v1(&program, &Src(&params)).unwrap();
    PalwEvalHeldV1::from_map(
        Hash64::from_bytes([0x22 + salt as u8; 64]),
        root,
        Hash64::from_bytes([0x14; 64]),
        program,
        layout,
        params,
    )
}

/// **The toy class's evaluations are the reference's on the executor**: generating and teacher-forced,
/// over five weights and two prompts; the claims check and a seat (on the executor) replays them Valid.
#[test]
fn the_toy_class_s_evaluations_are_the_reference_s_on_the_executor() {
    let facts = misaka_palw_sdk::improve_eval::PalwEvalClaimFactsV1 {
        network_domain: Hash64::from_bytes([0x10; 64]),
        executor_bond: kaspa_consensus_core::config::premine::premine_outpoint(2),
        executor_pubkey: vec![7; 16],
        operator_id: Hash64::from_bytes([0x12; 64]),
        anchor_block: Hash64::from_bytes([0x13; 64]),
        anchor_daa: 99,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        trace_retention_daa: 5_000,
    };
    for salt in 0..5 {
        let held = toy(salt);
        assert!(held.evaluates_on_executor_v1(), "the executor computes the toy class's subject stage");
        for prompt in [vec![3, 5, 1], vec![9]] {
            let what = format!("salt {salt}, prompt {prompt:?}");
            let g = both(&held, &generating(&held, 7, prompt.clone(), 4), &format!("{what}, generating"));
            assert_eq!(g.generated().len(), 4);
            let f = both(&held, &forced(&held, 7, prompt, vec![7, 2, 9, 9]), &format!("{what}, teacher-forced"));
            assert_eq!(f.tail.score.len(), 2, "(hi, lo)");
            for work in [&g, &f] {
                let claim = palw_eval_claim_v1(work, &held, &facts).expect("the claim checks");
                assert_eq!(
                    palw_eval_seat_judge_v1(&held, &claim.commitment, &claim.prompt, &claim.tail),
                    PalwEvalSeatJudgmentV1::Valid
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Lowered Hugging Face fixtures
// ---------------------------------------------------------------------------------------------

/// The fixture lowered, calibrated, materialised and declared as a class (`palw-tir-fidelity`, then
/// `palw-class declare-layout`): the declared container's path.
fn declared(fixture: &str, dir: &Path) -> Option<PathBuf> {
    use misaka_palw_tir_lower::float_ref::ParamStore;
    use misaka_palw_tir_lower::float_ref::stream::Resident;
    use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
    use misaka_palw_tir_lower::quant::QuantPolicy;
    use misaka_palw_tir_lower::weights::Checkpoint;
    use misaka_palw_tir_lower::{artifact, fidelity};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures/hf").join(fixture);
    if !root.join("model.safetensors").exists() {
        eprintln!("the {fixture} fixture is missing: skipped");
        return None;
    }
    let config = std::fs::read_to_string(root.join("config.json")).unwrap();
    let prep = fidelity::prepare(&config, &LowerOpts { max_window: Some(CONTEXT), ..Default::default() }).expect("lowered");
    let ck = Checkpoint::open(&root).unwrap();
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap();
    let loader = Resident(Arc::new(params));
    let calib = fidelity::random_sequences(prep.hl.vocab, 2, CONTEXT as usize, 7);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).unwrap();
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).unwrap();
    let lowered = dir.join(format!("{fixture}.palwtir"));
    let meta = serde_json::json!({ "calibrated_context": CONTEXT, "model_id": format!("test/{fixture}") });
    artifact::write(&lowered, &prep.lowered.program, &mat.params, [0u8; 64], meta).unwrap();
    let declared = dir.join(format!("{fixture}.class.palwtir"));
    let net = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &net.palw_consensus_mode else { panic!() };
    let choice = misaka_palw_sdk::tir_layout::TirLayoutChoiceV1 { max_context: Some(CONTEXT), ..Default::default() };
    misaka_palw_sdk::tir_layout::tir_declare_layout_v1(&net, bundle, &lowered, &declared, &choice, None).expect("declared");
    Some(declared)
}

/// The fixture's class held at its floor, every row-addressed param served by rows.
fn at_the_floor(mapped: &PalwTirClassEntryV1, path: &Path) -> (PalwTirClassEntryV1, Arc<TirArtifactV1>) {
    let a = TirTiersV1::of(&mapped.artifact.plan().program, ALL_ROWS).arithmetic();
    let art = Arc::new(TirArtifactV1::open_with_rules(path, TirResidencyPolicyV1::Bytes(a.floor_bytes), ALL_ROWS).unwrap());
    assert!(!art.is_mapped());
    (PalwTirClassEntryV1 { artifact: art.clone(), ..mapped.clone() }, art)
}

/// Prompt ids inside the fixture's vocabulary.
fn ids(held: &PalwEvalHeldV1, from: &[u32]) -> Vec<u32> {
    from.iter().map(|t| t % held.program.token_bound).collect()
}

/// **Lowered models evaluate the same on the executor, mapped and at their floor**: mixtures (their
/// experts routed through the residency's rows), a dense decoder and a hybrid with recurrent `Fixed`
/// states, generating and teacher-forced — the executor's work the reference's, and the resident
/// executor's the mapped one's.
#[test]
fn lowered_models_evaluate_the_same_on_the_executor_mapped_and_at_their_floor() {
    let dir = Scratch::new("lowered");
    let mut ran = Vec::new();
    for fixture in ["qwen3_moe", "mixtral", "olmoe", "llama", "qwen3_next"] {
        let Some(path) = declared(fixture, &dir.0) else { continue };
        let mapped = TirLineageV1::open_entry(&path).expect("mapped");
        let held = PalwEvalHeldV1::from_entry(&mapped).expect("an evaluation subject");
        assert!(held.evaluates_on_executor_v1(), "{fixture}: the executor computes the subject stage");
        let prompt = ids(&held, &[1, 5, 9, 2]);
        let reference = ids(&held, &[3, 8, 13]);
        let g = both(&held, &generating(&held, 7, prompt.clone(), 3), &format!("{fixture} mapped, generating"));
        let f = both(&held, &forced(&held, 7, prompt.clone(), reference.clone()), &format!("{fixture} mapped, teacher-forced"));
        let (resident, art) = at_the_floor(&mapped, &path);
        let rheld = PalwEvalHeldV1::from_entry(&resident).expect("the resident subject");
        let rg = palw_eval_run_v1(&rheld, &generating(&rheld, 7, prompt.clone(), 3)).expect("the floor generates");
        let rf = palw_eval_run_v1(&rheld, &forced(&rheld, 7, prompt, reference)).expect("the floor scores");
        assert_same(&rg, &g, &format!("{fixture} at its floor, generating"));
        assert_same(&rf, &f, &format!("{fixture} at its floor, teacher-forced"));
        let s = art.residency_stats().unwrap();
        assert!(s.gathered_rows > 0, "{fixture}: the embedding's rows were gathered: {s:?}");
        assert_eq!(s.whole_reads, 0, "{fixture}: nothing served by rows was read whole: {s:?}");
        ran.push(fixture);
    }
    eprintln!("evaluated on the executor and the reference: {ran:?}");
    assert!(ran.len() >= 4, "{ran:?}");
}

// ---------------------------------------------------------------------------------------------
// Batches
// ---------------------------------------------------------------------------------------------

/// **One item's tasks over one parent's weights, stepped together, are each the run alone**: the toy
/// class under three weights (in memory) and a lowered mixture held at its floor (the store's rows),
/// generating and teacher-forced; the mixture's batch reads its routed rows once where the members one
/// after another read them once each; a member of another class runs alone and fails as alone.
#[test]
fn a_batch_steps_together_and_each_member_is_its_run_alone() {
    // The toy class under three weights.
    let helds: Vec<PalwEvalHeldV1> = (0..3).map(toy).collect();
    let tasks: Vec<PalwImproveEvalTaskV1> = helds
        .iter()
        .enumerate()
        .map(|(i, h)| if i == 1 { forced(h, 7, vec![3, 5, 1], vec![7, 2, 9, 9]) } else { generating(h, 7, vec![3, 5, 1], 4) })
        .collect();
    let members: Vec<(&PalwEvalHeldV1, &PalwImproveEvalTaskV1)> = helds.iter().zip(&tasks).collect();
    let batch = palw_eval_run_batch_v1(&members);
    assert_eq!(batch.stepped_together, 3);
    for (i, (run, (h, t))) in batch.runs.iter().zip(&members).enumerate() {
        let alone = palw_eval_run_reference_v1(h, t).expect("alone");
        assert_same(run.as_ref().expect("a member's run"), &alone, &format!("toy member {i}"));
    }
    // A member whose task is another class's: it runs alone, refused as alone; the rest step together.
    let stranger = generating(&helds[0], 9, vec![2], 2);
    let mut odd = members.clone();
    odd[2] = (&helds[2], &stranger);
    let batch = palw_eval_run_batch_v1(&odd);
    assert_eq!(batch.stepped_together, 2);
    assert!(batch.runs[2].as_ref().is_err_and(|e| e.contains("subject")), "{:?}", batch.runs[2].as_ref().err());
    assert_same(
        batch.runs[0].as_ref().unwrap(),
        &palw_eval_run_reference_v1(&helds[0], &tasks[0]).unwrap(),
        "toy member 0 beside a stranger",
    );

    // A lowered mixture at its floor: four tasks, two generating and two teacher-forced, over three prompts.
    let dir = Scratch::new("batch");
    let Some(path) = declared("qwen3_moe", &dir.0) else { return };
    let mapped = TirLineageV1::open_entry(&path).expect("mapped");
    let (resident, art) = at_the_floor(&mapped, &path);
    let held = PalwEvalHeldV1::from_entry(&resident).unwrap();
    let prompt = ids(&held, &[1, 5, 9, 2, 6]);
    let tasks = [
        generating(&held, 7, prompt.clone(), 3),
        generating(&held, 8, ids(&held, &[4, 4, 1]), 3),
        forced(&held, 7, prompt.clone(), ids(&held, &[3, 8, 13])),
        forced(&held, 9, ids(&held, &[7]), ids(&held, &[2, 2])),
    ];
    let alone: Vec<PalwEvalWorkV1> = tasks.iter().map(|t| palw_eval_run_v1(&held, t).expect("alone")).collect();
    let members: Vec<(&PalwEvalHeldV1, &PalwImproveEvalTaskV1)> = tasks.iter().map(|t| (&held, t)).collect();
    let batch = palw_eval_run_batch_v1(&members);
    assert_eq!(batch.stepped_together, 4);
    for (i, (run, alone)) in batch.runs.iter().zip(&alone).enumerate() {
        assert_same(run.as_ref().expect("a member's run"), alone, &format!("qwen3_moe member {i}"));
    }
    for (t, alone) in tasks.iter().zip(&alone) {
        assert_same(&palw_eval_run_reference_v1(&held, t).unwrap(), alone, "qwen3_moe: the executor's run is the reference's");
    }
    // One item's tasks — one prompt, as every subject of an item has — one after another and in one batch:
    // the batch admits a layer's routed rows once for every member at the same position.
    let store = art.weight_store().unwrap().clone();
    let item = [
        forced(&held, 7, prompt.clone(), ids(&held, &[3, 8, 13])),
        forced(&held, 7, prompt.clone(), ids(&held, &[3, 8, 13])),
        generating(&held, 7, prompt.clone(), 3),
    ];
    let s0 = store.stats();
    for t in &item {
        palw_eval_run_v1(&held, t).expect("alone");
    }
    let s1 = store.stats();
    let members: Vec<(&PalwEvalHeldV1, &PalwImproveEvalTaskV1)> = item.iter().map(|t| (&held, t)).collect();
    let batch = palw_eval_run_batch_v1(&members);
    let s2 = store.stats();
    assert!(batch.runs.iter().all(|r| r.is_ok()) && batch.stepped_together == 3);
    let (sequential, together) = (s1.routed_bytes_read - s0.routed_bytes_read, s2.routed_bytes_read - s1.routed_bytes_read);
    eprintln!(
        "qwen3_moe at its floor, one item's three tasks: routed rows read {sequential} bytes one after another, {together} in one batch ({:?})",
        batch.served
    );
    // The members share the prompt's five positions (and two of them the reference's): the batch reads those
    // positions' rows once where the members one after another read them once each.
    assert!(
        together * 3 < sequential * 2,
        "the batch reads a shared position's rows once for its members: {together} vs {sequential}"
    );
    // The executor reads no served instance whole (the reference's runs above did: its door is whole tensors).
    assert_eq!(s2.whole_reads, s0.whole_reads, "the executor's runs read nothing served by rows whole");
    // The width a node would give such a batch: bounded by the routed capacity over one admission and the members' runs.
    let width = palw_eval_lockstep_width_v1(&held, 16, 1 << 40, 8);
    assert!((1..=8).contains(&width), "{width}");
    assert!(palw_eval_lockstep_width_v1(&held, 16, held.run_bytes_v1(16) * 2, 8) <= 2, "two runs' worth of spare holds two at most");
    assert_eq!(palw_eval_lockstep_width_v1(&held, 16, 0, 8), 1, "no spare: a run alone");
}

/// **A LoRA candidate served from its adapter section over its held parent steps together with the
/// parent** (RFC-0004 §6.3, §6.7): the parent within a budget, the section opened over it (one store), the
/// parent's and the candidate's evaluations of one item in one batch — each the reference's run — and the
/// candidate's the full candidate container's. A rank-16 Llama adapter and a Mistral q/v adapter over
/// a sliding window.
#[test]
fn a_composite_candidate_and_its_parent_step_together() {
    for adapter in ["llama_r16", "mistral_qv_r8"] {
        composite_and_parent(adapter);
    }
}

fn composite_and_parent(adapter: &str) {
    use misaka_palw_sdk::tir_composite::{tir_composite_derive_v1, tir_composite_section_write_v1};
    use misaka_palw_tir_artifact::PalwTirContainerV1;
    let dir = Scratch::new(adapter);
    let Some((pp, cp)) = lora_pair(adapter, &dir.0) else { return };
    let sp = dir.0.join("candidate.palwtirs");
    let mapped = TirLineageV1::new();
    mapped.load(&pp, PalwWeightResidencyV1::PageCache).expect("the parent loads");
    let parent = mapped.tir_classes().remove(0);
    let (pc, cc) = (PalwTirContainerV1::open(&pp).unwrap(), PalwTirContainerV1::open(&cp).unwrap());
    let composite = tir_composite_derive_v1(&pc, &cc, Some(parent.class_id())).expect("a composite of the parent");
    tir_composite_section_write_v1(&cc, &composite, &sp).expect("the section");
    let resident = TirLineageV1::new();
    let budget = PalwWeightResidencyV1::Bytes(1 << 40);
    resident.load(&pp, budget).expect("the parent within a budget");
    resident.load(&sp, budget).expect("the section over the held parent");
    let classes = resident.tir_classes();
    let p_entry = classes.iter().find(|e| e.artifact.composite_ref().is_none()).expect("the parent");
    let c_entry = classes.iter().find(|e| e.artifact.composite_ref().is_some()).expect("the candidate");
    let (hp, hc) = (PalwEvalHeldV1::from_entry(p_entry).unwrap(), PalwEvalHeldV1::from_entry(c_entry).unwrap());
    assert_eq!(hp.weights_root_v1(), hc.weights_root_v1(), "the candidate's weights are its parent's store");
    let full = PalwEvalHeldV1::from_entry(&TirLineageV1::open_entry(&cp).expect("the full candidate")).unwrap();
    for (name, tp, tc) in [
        ("generating", generating(&hp, 7, vec![1, 5, 9], 3), generating(&hc, 7, vec![1, 5, 9], 3)),
        ("teacher-forced", forced(&hp, 7, vec![1, 5, 9], vec![4, 2, 8]), forced(&hc, 7, vec![1, 5, 9], vec![4, 2, 8])),
    ] {
        let batch = palw_eval_run_batch_v1(&[(&hp, &tp), (&hc, &tc)]);
        assert_eq!(batch.stepped_together, 2, "{adapter} {name}");
        let (wp, wc) = (batch.runs[0].as_ref().expect("the parent's run"), batch.runs[1].as_ref().expect("the candidate's run"));
        assert_same(wp, &palw_eval_run_reference_v1(&hp, &tp).unwrap(), &format!("{adapter}: the parent, {name}"));
        assert_same(wc, &palw_eval_run_reference_v1(&hc, &tc).unwrap(), &format!("{adapter}: the candidate, {name}"));
        let tf =
            if name == "generating" { generating(&full, 7, vec![1, 5, 9], 3) } else { forced(&full, 7, vec![1, 5, 9], vec![4, 2, 8]) };
        let wf = palw_eval_run_v1(&full, &tf).unwrap();
        assert_eq!(wc.execution.claim, wf.execution.claim, "{adapter} {name}: the composite computes the full candidate");
        assert_eq!(wc.tail.score, wf.tail.score, "{adapter} {name}");
    }
}

/// The parent and a LoRA candidate as `palw-tir-fidelity` (`--adapter --parent-stats`) converts them,
/// written as PALWTIR1 containers under a small layout: their paths.
fn lora_pair(adapter: &str, dir: &Path) -> Option<(PathBuf, PathBuf)> {
    use misaka_palw_tir_lower::fidelity;
    use misaka_palw_tir_lower::float_ref::ParamStore;
    use misaka_palw_tir_lower::float_ref::stream::Resident;
    use misaka_palw_tir_lower::lower::{IntParams, LowerOpts, materialise};
    use misaka_palw_tir_lower::quant::QuantPolicy;
    use misaka_palw_tir_lower::weights::{Checkpoint, Overlay};
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures");
    let ad_dir = fixtures.join("hf-lora").join(adapter);
    if !ad_dir.join("adapter_model.safetensors").exists() {
        eprintln!("the {adapter} fixture is missing: skipped");
        return None;
    }
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(ad_dir.join("logits.json")).unwrap()).unwrap();
    let base_dir = fixtures.join("hf").join(meta["base"].as_str().unwrap());
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
    let tiled = |program: &TirProgramV1| {
        let mut q = program.clone();
        q.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
        TirProgramV1::decode_canonical(&q.encode()).expect("still canonical")
    };
    let layout = |program: &TirProgramV1| {
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
    };
    let write = |path: &Path, program: &TirProgramV1, params: &IntParams, meta: serde_json::Value| {
        misaka_palw_tir_artifact::write_container_v1(
            path,
            program,
            borsh::to_vec(&layout(program)).unwrap(),
            [0x61; 64],
            meta.to_string(),
            &mut |j, l| params.tensors.get(&(j, l)).map(|t| t.le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}")),
        )
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    };
    let (pp, cp) = (dir.join("parent.palwtir"), dir.join("candidate.palwtir"));
    write(&pp, &tiled(&parent.lowered.program), &mat_p.params, serde_json::json!({ "model_id": "test/parent" }));
    write(
        &cp,
        &tiled(&cand.lowered.program),
        &mat_c.params,
        serde_json::json!({ "model_id": "test/candidate", "composite": { "p": p } }),
    );
    Some((pp, cp))
}
