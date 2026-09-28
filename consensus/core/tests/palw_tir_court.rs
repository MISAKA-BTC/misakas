//! **RFC-0002 Phase F, step F5's exit test: the IR court on real executions.**
//!
//! Every program of `consensus-vectors/tir-v1/programs` (the five corpus models and the two state
//! programs) is run as a job — a prompt, then greedily generated tokens fed back — under a layout
//! with ragged multi-tile commit points, two-position checkpoints and two-row history tiles, its
//! logits committed under the tiled scheme, its params committed as a TIR inventory. Then:
//!
//! * every leaf of every honest execution, refuted with the refutation the builder assembles, is
//!   ACQUITTED (the cone recomputes to the committed lanes);
//! * every single-lane forgery that stays inside its proven interval is CONVICTED as a computation
//!   mismatch at that lane, and every lane pushed outside it is convicted by PALW-TIR-33 — on the
//!   disputed leaf itself, and on an operand whichever leaf is disputed;
//! * a carriage that is not the canonical set (a unit dropped, a unit added, a pin or a prompt that
//!   is not read) is REFUSED, and a binding or a leaf whose own structure is wrong convicts from it;
//! * the logits-consistency accusation and the decode-token door convict what they must and acquit
//!   the honest run;
//! * mutated refutations never panic the court.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::PathBuf;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, open_artifact_leaf_v1};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, PalwPromptIdsOpeningV1};
use kaspa_consensus_core::palw_step_leg::{
    PalwStepFaultV1, PalwStepOpeningV1, PalwStepTileLeafV1, step_merkle_range_siblings_v1, step_merkle_root_v1, step_opening_v1,
    step_tile_leaf_hash_v1,
};
use kaspa_consensus_core::palw_step_refute::{
    PalwDecodeTokenPinV1, PalwStepRefuteError, PalwTiledDecodeTokensV1, base0_decode_token_select_v1, tiled_decode_pin_v1,
    tiled_logits_rows_root_v1, tiled_logits_scheme_id_v1, tiled_logits_trace_root_v1,
};
use kaspa_consensus_core::palw_tir_artifact_v1::{
    PalwTirTensorSourceV1, palw_tir_inventory_operands_v1, palw_tir_leaf_index_v1, palw_tir_visit_inventory_rows_v1,
};
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1,
};
use kaspa_consensus_core::palw_tir_court_v1::{
    PalwTirConeRefutationV1, PalwTirCourtRulesV1, PalwTirEvidenceStoreV1, PalwTirInventoryIndexV1, PalwTirLogitsConsistencyV1,
    PalwTirTraceLanesV1, build_tir_cone_refutation_v1, check_tir_cone_refutation_v1, check_tir_decode_token_tiled_v1,
    check_tir_logits_consistency_v1, palw_tir_leaf_interval_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::{
    PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirLeafKindV1, PalwTirLeafV1, PalwTirStepBindingV1, PalwTirStepSpaceV1,
    palw_tir_execution_root_v1, palw_tir_leaf_preimage_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use misaka_palw_tir::demand::{DemandLimits, hist_row_node_v1};
use misaka_palw_tir::interval::{Interval, analyze_ranges};
use misaka_palw_tir::{DType, Interpreter, MapParams, RunState, Tensor, TirProgramV1};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

/// The golden programs: name, program, params, tokens.
fn programs() -> Vec<(String, TirProgramV1, MapParams, Vec<u32>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v1/programs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("vectors")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let program = TirProgramV1::decode_canonical(&unhex(v["program_borsh_hex"].as_str().unwrap())).expect("canonical");
            let mut params = MapParams::default();
            for p in v["params"].as_array().unwrap() {
                let j = p["param"].as_u64().unwrap() as u16;
                let layer = p["layer"].as_u64().map(|l| l as u16);
                let d = &program.params[j as usize];
                let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
                params
                    .tensors
                    .insert((j, layer), Tensor::from_le_bytes(d.dtype, &shape, &unhex(p["le_hex"].as_str().unwrap())).unwrap());
            }
            let tokens = v["steps"].as_array().unwrap().iter().map(|s| s["token"].as_u64().unwrap() as u32).collect();
            (v["name"].as_str().unwrap().to_string(), program, params, tokens)
        })
        .collect()
}

struct TensorSrc<'a>(&'a MapParams);
impl PalwTirTensorSourceV1 for TensorSrc<'_> {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.tensors.get(&(param, layer)).map(|t| Cow::Owned(t.to_le_bytes()))
    }
}

const PREFILL: u32 = 4;
const DECODE: u32 = 3;
const RULES: PalwTirCourtRulesV1 =
    PalwTirCourtRulesV1 { max_step_leaf_count: 1 << 26, prompt_form: PalwPromptIdsFormV1::Flat, limits: DemandLimits::UNLIMITED };

/// A class, a job and its honest run.
struct Fixture {
    name: String,
    space: PalwTirStepSpaceV1,
    class: PalwTirClassV1,
    ctx: PalwJobContextV2,
    class_id: Hash64,
    artifact_root: Hash64,
    ops: Vec<PalwArtifactOperandV1>,
    prompt: Vec<u32>,
    generated: Vec<u32>,
    rows: Vec<Vec<i32>>,
    leaves: Vec<PalwTirLeafV1>,
    values: Vec<Vec<i128>>,
    /// The program's proven intervals — `None` for a program the range analysis refuses (no such
    /// class is ever admitted; the golden state programs are evaluator vectors, not classes).
    intervals: Option<Vec<Vec<Interval>>>,
    /// Whether every generated token is the greedy selection over its row.
    greedy: bool,
}

/// One committed execution: the leaf preimages, their hashes, the trace, the binding.
#[derive(Clone)]
struct Execution {
    preimages: Vec<PalwStepTileLeafV1>,
    hashes: Vec<Hash64>,
    rows: Vec<Vec<i32>>,
    generated: Vec<u32>,
    binding: PalwTirStepBindingV1,
}

fn layout(p: &TirProgramV1, positions: u32) -> PalwTirLayoutV1 {
    let mut tiles = Vec::new();
    let mut k = 0u32;
    for (bi, b) in p.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if !n.commit {
                continue;
            }
            let is_logits = bi == p.schedule.post as usize && ni == p.logits as usize;
            tiles.push(if is_logits { 4096 } else { 4 + (k * 7) % 6 });
            k += 1;
        }
    }
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: positions,
        checkpoint_interval: 2,
        h_tile: 2,
        commit_tiles: tiles,
        state_tiles: (0..p.states.len() as u32).map(|j| 4 + j % 3).collect(),
    }
}

fn fixture(name: String, mut program: TirProgramV1, params: MapParams, tokens: Vec<u32>) -> Fixture {
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let bytes = program.encode();
    let program = TirProgramV1::decode_canonical(&bytes).expect("still canonical under the tiled scheme");
    let positions = PREFILL + DECODE - 1;
    let prompt: Vec<u32> = (0..PREFILL as usize).map(|i| tokens[i % tokens.len()]).collect();
    // The honest run: prompt, then each selected token fed back.
    let interp = Interpreter::new(&program).expect("valid");
    let mut state = RunState::default();
    let occurrences = program.occurrences();
    let occ_of = |block: u8, layer: Option<u16>| -> usize {
        match layer {
            Some(l) => l as usize + 1,
            None if block == program.schedule.pre => 0,
            None => occurrences.len() - 1,
        }
    };
    let (mut commits, mut after) = (Vec::new(), Vec::new());
    let (mut generated, mut rows, mut greedy) = (Vec::new(), Vec::new(), true);
    for a in 0..positions {
        let token = if a < PREFILL { prompt[a as usize] } else { generated[(a - PREFILL) as usize] };
        let step = interp.step(&params, &mut state, token).expect("an honest step");
        let mut c: BTreeMap<(usize, u16), Tensor> = BTreeMap::new();
        for r in step.commits {
            c.insert((occ_of(r.block, r.layer), r.node), r.value);
        }
        commits.push(c);
        after.push(state.clone());
        if a + 1 >= PREFILL {
            let row: Vec<i32> = step.logits.data.iter().map(|v| *v as i32).collect();
            let mut pick = base0_decode_token_select_v1(&row) as u32;
            if pick >= program.token_bound {
                greedy = false;
                pick %= program.token_bound;
            }
            generated.push(pick);
            rows.push(row);
        }
    }
    // The class and its artifact.
    let lay = layout(&program, positions);
    let class =
        PalwTirClassV1 { version: PALW_TIR_CLASS_VERSION_V1, program: bytes, layout: lay, tokenizer_id: Hash64::from_bytes([3; 64]) };
    let (ops, artifact_root) = if program.params.is_empty() {
        (Vec::new(), Hash64::from_bytes([0xA7; 64]))
    } else {
        let ops = palw_tir_inventory_operands_v1(&program, &TensorSrc(&params)).expect("inventory");
        let leaves: Vec<Hash64> = ops.iter().map(artifact_leaf_v1).collect();
        let root = artifact_root_v1(&leaves).expect("root");
        (ops, root)
    };
    let class_id = class.class_id(&artifact_root);
    let z = Hash64::from_bytes([0u8; 64]);
    let ctx = PalwJobContextV2 {
        version: 2,
        network_id: b"testnet-12".to_vec(),
        job_id: Hash64::from_bytes([5; 64]),
        job_nullifier: z,
        assignment_id: z,
        execution_seed: [0u8; 32],
        model_profile_id: z,
        runtime_manifest_hash: z,
        runtime_class_id: z,
        shape_profile_id: class_id,
        trace_scheme_id: tiled_logits_scheme_id_v1(),
        cu_ruleset_id: z,
        tokenizer_id: class.tokenizer_id,
        prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&prompt),
        declared_prefill_tokens: PREFILL,
        exact_decode_tokens: DECODE,
        max_context_tokens: 64,
    };
    let space = PalwTirStepSpaceV1::new(&class).expect("the layout fits");
    let intervals = analyze_ranges(&space.program).ok();
    let mut leaves = Vec::new();
    let mut values = Vec::new();
    for a in 0..positions {
        for leaf in space.leaves_of_position(&ctx, a) {
            let n = leaf.value_count as usize;
            let v: Vec<i128> = match leaf.kind {
                PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } => {
                    commits[a as usize][&(occurrence as usize, node)].data[first_element as usize..first_element as usize + n].to_vec()
                }
                PalwTirLeafKindV1::State { state: j, layer, first_element, .. } => {
                    let s = &space.program.states[j as usize];
                    let shape: Vec<usize> = s.shape.iter().map(|d| *d as usize).collect();
                    let t = after[a as usize].fixed.get(&(j, layer)).cloned().unwrap_or_else(|| Tensor::zeros(s.dtype, &shape));
                    t.data[first_element as usize..first_element as usize + n].to_vec()
                }
                PalwTirLeafKindV1::HistTile { state: j, layer, first_lane, row_lanes, first_position, .. } => (first_position..=a)
                    .flat_map(|p| {
                        let (c, node) = hist_row_node_v1(&space.program, j, layer, p).expect("appends");
                        commits[p as usize][&(c.occurrence as usize, node)].data
                            [first_lane as usize..first_lane as usize + row_lanes as usize]
                            .to_vec()
                    })
                    .collect(),
            };
            leaves.push(leaf);
            values.push(v);
        }
    }
    Fixture { name, space, class, ctx, class_id, artifact_root, ops, prompt, generated, rows, leaves, values, intervals, greedy }
}

/// The admissible programs: the five corpus models (the range analysis proves them).
fn fixtures() -> Vec<Fixture> {
    let all: Vec<Fixture> = programs().into_iter().map(|(n, p, params, t)| fixture(n, p, params, t)).collect();
    let admissible: Vec<Fixture> = all.into_iter().filter(|f| f.intervals.is_some()).collect();
    assert_eq!(admissible.len(), 5, "the five corpus models are admissible");
    admissible
}

/// Lanes as committed: `i32` little-endian, `idx` as `u32` — written raw, so a test can commit a
/// lane outside its node's dtype (the builder would refuse to).
fn raw_lanes(dtype: DType, values: &[i128]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 4);
    for v in values {
        if dtype == DType::Idx {
            out.extend_from_slice(&(*v as u32).to_le_bytes());
        } else {
            out.extend_from_slice(&(*v as i32).to_le_bytes());
        }
    }
    out
}

impl Fixture {
    /// Commit `values` (honest or not), a trace of `rows` and `generated`, and bind it.
    fn commit(&self, values: &[Vec<i128>], rows: &[Vec<i32>], generated: &[u32]) -> Execution {
        let ctx_hash = self.ctx.context_hash();
        let preimages: Vec<PalwStepTileLeafV1> = self
            .leaves
            .iter()
            .zip(values)
            .map(|(leaf, v)| {
                let mut p = palw_tir_leaf_preimage_v1(leaf, &vec![0; leaf.value_count as usize]).expect("shape");
                p.values_le = raw_lanes(leaf.dtype, v);
                p
            })
            .collect();
        let hashes: Vec<Hash64> = preimages.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash, &self.class_id, p)).collect();
        let root = step_merkle_root_v1(&hashes).expect("root");
        let trace = tiled_logits_trace_root_v1(&self.ctx, rows, generated).expect("trace");
        let count = hashes.len() as u64;
        let binding = PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: self.ctx.clone(),
            class: self.class.clone(),
            artifact_root: self.artifact_root,
            full_logits_trace_root: trace,
            step_leaf_count: count,
            step_merkle_root: root,
            committed_execution_root: palw_tir_execution_root_v1(&ctx_hash, &trace, &self.class_id, count, &root),
        };
        Execution { preimages, hashes, rows: rows.to_vec(), generated: generated.to_vec(), binding }
    }

    fn honest(&self) -> Execution {
        self.commit(&self.values, &self.rows, &self.generated)
    }

    fn interval(&self, leaf: &PalwTirLeafV1) -> Interval {
        palw_tir_leaf_interval_v1(&self.space, self.intervals.as_ref().expect("admissible"), leaf).expect("interval")
    }
}

struct Store<'a> {
    f: &'a Fixture,
    x: &'a Execution,
}

impl PalwTirEvidenceStoreV1 for Store<'_> {
    fn step_leaf(&self, index: u64) -> Option<PalwStepTileLeafV1> {
        self.x.preimages.get(index as usize).cloned()
    }
    fn step_opening(&self, index: u64) -> Option<PalwStepOpeningV1> {
        step_opening_v1(&self.x.hashes, index).ok()
    }
    fn step_range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>> {
        step_merkle_range_siblings_v1(&self.x.hashes, first as usize, count as usize).ok()
    }
    fn param_opening(&self, leaf: u32) -> Option<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1> {
        open_artifact_leaf_v1(&self.f.ops, leaf)
    }
    fn prompt_token_ids(&self) -> Option<Vec<u32>> {
        Some(self.f.prompt.clone())
    }
    fn prompt_ids_opening(&self, _tile: u32) -> Option<PalwPromptIdsOpeningV1> {
        None
    }
    fn decode_pin(&self) -> Option<PalwDecodeTokenPinV1> {
        Some(PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 {
            rows_root: tiled_logits_rows_root_v1(&self.f.ctx, &self.x.rows)?,
            generated_token_ids: self.x.generated.clone(),
        }))
    }
}

fn refute(f: &Fixture, x: &Execution, leaf: u64) -> PalwTirConeRefutationV1 {
    build_tir_cone_refutation_v1(&x.binding, leaf, &Store { f, x }, &RULES).unwrap_or_else(|e| panic!("{}: leaf {leaf}: {e}", f.name))
}

/// Re-point a refutation at another commitment of the same leaves (its openings and run siblings).
fn rebind(r: &mut PalwTirConeRefutationV1, x: &Execution) {
    let index_of = |p: &PalwStepTileLeafV1| x.preimages.iter().position(|q| q.coord == p.coord).expect("a leaf") as u64;
    r.binding = x.binding.clone();
    r.output_opening = step_opening_v1(&x.hashes, r.output_opening.leaf_index).unwrap();
    r.output_preimage = x.preimages[r.output_opening.leaf_index as usize].clone();
    let indices: Vec<u64> = r.operands.preimages.iter().map(index_of).collect();
    r.operands.preimages = indices.iter().map(|i| x.preimages[*i as usize].clone()).collect();
    let mut runs = Vec::new();
    let mut k = 0;
    while k < indices.len() {
        let mut len = 1;
        while k + len < indices.len() && indices[k + len] == indices[k] + len as u64 {
            len += 1;
        }
        runs.push(step_merkle_range_siblings_v1(&x.hashes, indices[k] as usize, len).unwrap());
        k += len;
    }
    r.operands.run_siblings = runs;
}

#[test]
fn every_honest_leaf_is_acquitted() {
    let mut total = 0usize;
    let mut kinds = BTreeMap::new();
    for f in fixtures() {
        let x = f.honest();
        for (i, leaf) in f.leaves.iter().enumerate() {
            let r = refute(&f, &x, i as u64);
            let verdict = check_tir_cone_refutation_v1(&r, &RULES);
            assert_eq!(verdict, Err(PalwStepRefuteError::NoFaultFound), "{} leaf {i} {:?}", f.name, leaf.kind);
            let kind = match leaf.kind {
                PalwTirLeafKindV1::Commit { .. } => "commit",
                PalwTirLeafKindV1::State { .. } => "checkpoint",
                PalwTirLeafKindV1::HistTile { .. } => "history tile",
            };
            *kinds.entry(kind).or_insert(0usize) += 1;
            total += 1;
        }
    }
    assert_eq!(kinds.len(), 3, "commit tiles, checkpoints and history tiles all adjudicated");
    assert!(total > 900, "{total} leaves");
}

#[test]
fn every_single_lane_forgery_is_convicted() {
    let mut convicted = 0usize;
    for f in fixtures() {
        let stride = (f.leaves.len() / 150).max(1);
        for i in (0..f.leaves.len()).step_by(stride) {
            let leaf = &f.leaves[i];
            let lane = leaf.value_count as usize / 2;
            let v = f.values[i][lane];
            let iv = f.interval(leaf);
            let Some(forged) = [v + 1, v - 1].into_iter().find(|w| iv.contains(*w)) else { continue };
            let mut values = f.values.clone();
            values[i][lane] = forged;
            let x = f.commit(&values, &f.rows, &f.generated);
            let r = refute(&f, &x, i as u64);
            let verdict = check_tir_cone_refutation_v1(&r, &RULES).unwrap_or_else(|e| panic!("{} leaf {i}: {e}", f.name));
            assert_eq!(verdict.fault, PalwStepFaultV1::ComputationMismatch { value_index: lane as u32 }, "{} leaf {i}", f.name);
            convicted += 1;
        }
    }
    assert!(convicted > 500, "{convicted} forgeries convicted");
}

#[test]
fn a_lane_outside_its_proven_interval_convicts_by_palw_tir_33() {
    let mut on_output = 0usize;
    let mut on_operand = 0usize;
    for f in fixtures() {
        let honest = f.honest();
        // The first later leaf whose refutation carries each leaf as an operand.
        let mut first_reader: Vec<Option<usize>> = vec![None; f.leaves.len()];
        for j in 0..f.leaves.len() {
            for p in &refute(&f, &honest, j as u64).operands.preimages {
                let k = honest.preimages.iter().position(|q| q.coord == p.coord).unwrap();
                first_reader[k].get_or_insert(j);
            }
        }
        let stride = (f.leaves.len() / 60).max(1);
        for i in (0..f.leaves.len()).step_by(stride) {
            let leaf = &f.leaves[i];
            let iv = f.interval(leaf);
            let lane_max = if leaf.dtype == DType::Idx { u32::MAX as i128 } else { i32::MAX as i128 };
            if iv.hi >= lane_max {
                continue;
            }
            // The disputed leaf itself.
            let mut values = f.values.clone();
            values[i][0] = iv.hi + 1;
            let x = f.commit(&values, &f.rows, &f.generated);
            let mut r = refute(&f, &honest, i as u64);
            rebind(&mut r, &x);
            let verdict = check_tir_cone_refutation_v1(&r, &RULES).unwrap_or_else(|e| panic!("{} leaf {i}: {e}", f.name));
            assert_eq!(verdict.fault, PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: 0 }, "{} leaf {i}", f.name);
            on_output += 1;
            // As an operand of the first later leaf that reads it: convicted there, at THIS leaf.
            if let Some(j) = first_reader[i] {
                let mut r = refute(&f, &honest, j as u64);
                rebind(&mut r, &x);
                let verdict = check_tir_cone_refutation_v1(&r, &RULES).unwrap_or_else(|e| panic!("{} leaf {j}: {e}", f.name));
                assert_eq!(verdict.fault, PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: 0 });
                let expect = kaspa_consensus_core::palw_step_leg::step_refutation_evidence_id(
                    &x.binding.committed_execution_root,
                    5,
                    i as u64,
                    verdict.fault,
                );
                assert_eq!(verdict.evidence_id, expect, "{}: the evidence names the operand's own leaf", f.name);
                on_operand += 1;
            }
        }
    }
    assert!(on_output > 100 && on_operand > 50, "{on_output} on the output, {on_operand} on an operand");
}

#[test]
fn a_carriage_that_is_not_the_canonical_set_is_refused() {
    let not_canonical = |v: Result<_, PalwStepRefuteError>| matches!(v, Err(PalwStepRefuteError::InputSetNotCanonical(_)));
    let mut checked = 0;
    for f in fixtures() {
        let x = f.honest();
        // A leaf that reads step leaves, params, the prompt: the last commit tile of the first
        // decode position's `pre`.
        let target = (0..f.leaves.len())
            .rev()
            .find(|i| {
                let r = refute(&f, &x, *i as u64);
                !r.operands.preimages.is_empty() && (f.ops.is_empty() || !r.params.is_empty())
            })
            .expect("a leaf with operands");
        let honest = refute(&f, &x, target as u64);
        assert_eq!(check_tir_cone_refutation_v1(&honest, &RULES), Err(PalwStepRefuteError::NoFaultFound));

        // An operand dropped.
        let mut r = honest.clone();
        r.operands.preimages.pop();
        rebind(&mut r, &x);
        assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: an operand dropped", f.name);
        // An operand added: the earliest leaf the set does not hold.
        let mut r = honest.clone();
        let held: Vec<_> = r.operands.preimages.iter().map(|p| p.coord).collect();
        if let Some(extra) = (0..target).find(|i| !held.contains(&x.preimages[*i].coord)) {
            r.operands.preimages.push(x.preimages[extra].clone());
            r.operands.preimages.sort_by_key(|p| x.preimages.iter().position(|q| q.coord == p.coord));
            rebind(&mut r, &x);
            assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: an operand added", f.name);
        }
        // Operands out of order.
        if honest.operands.preimages.len() >= 2 {
            let mut r = honest.clone();
            r.operands.preimages.swap(0, 1);
            assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: out of order", f.name);
        }
        // A param dropped or added.
        if !honest.params.is_empty() {
            let mut r = honest.clone();
            r.params.pop();
            assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: a param dropped", f.name);
            let mut r = honest.clone();
            let held: Vec<u32> = r.params.iter().map(|o| o.leaf_index).collect();
            if let Some(extra) = (0..f.ops.len() as u32).find(|l| !held.contains(l)) {
                r.params.push(open_artifact_leaf_v1(&f.ops, extra).unwrap());
                r.params.sort_by_key(|o| o.leaf_index);
                assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: a param added", f.name);
            }
            // A param opening of the right leaf with a byte changed does not reach the root.
            let mut r = honest.clone();
            r.params[0].operand.bytes[0] ^= 1;
            assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: a param byte changed", f.name);
        }
        // A prompt or a pin the evaluation does not read; or one it reads, withheld.
        let mut r = honest.clone();
        if r.prompt_token_ids.is_empty() {
            r.prompt_token_ids = f.prompt.clone();
        } else {
            r.prompt_token_ids.clear();
        }
        assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: the prompt", f.name);
        let mut r = honest.clone();
        r.decode_tokens = match r.decode_tokens {
            Some(_) => None,
            None => Store { f: &f, x: &x }.decode_pin(),
        };
        assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: the decode pin", f.name);
        // The whole-list prompt on a Merkle network is refused by form.
        let merkle = PalwTirCourtRulesV1 { prompt_form: PalwPromptIdsFormV1::MerkleV1, ..RULES };
        let mut r = honest.clone();
        r.prompt_token_ids = f.prompt.clone();
        assert!(not_canonical(check_tir_cone_refutation_v1(&r, &merkle)), "{}: form", f.name);
        checked += 1;
    }
    assert_eq!(checked, 5);
}

/// A program the range analysis refuses is no class: nothing can be proven about its lanes, so its
/// leaves are unadjudicable — refused, nobody slashed — never judged on unproven arithmetic.
#[test]
fn a_program_without_proven_ranges_is_never_adjudicated() {
    let mut seen = 0;
    for (n, p, params, t) in programs() {
        let f = fixture(n, p, params, t);
        if f.intervals.is_some() {
            continue;
        }
        let x = f.honest();
        for i in [0, f.leaves.len() / 2, f.leaves.len() - 1] {
            assert_eq!(
                check_tir_cone_refutation_v1(&refute(&f, &x, i as u64), &RULES),
                Err(PalwStepRefuteError::Unadjudicable),
                "{}",
                f.name
            );
        }
        seen += 1;
    }
    assert_eq!(seen, 2, "the two golden state programs are evaluator vectors, not classes");
}

#[test]
fn the_binding_and_the_leaf_convict_from_their_own_structure() {
    for f in fixtures().into_iter().take(3) {
        let x = f.honest();
        let honest = refute(&f, &x, 5);
        let ctx_hash = f.ctx.context_hash();
        // A leaf count that is not the job's, bound consistently: convicted from the binding.
        let mut r = honest.clone();
        r.binding.step_leaf_count += 1;
        r.binding.committed_execution_root = palw_tir_execution_root_v1(
            &ctx_hash,
            &r.binding.full_logits_trace_root,
            &f.class_id,
            r.binding.step_leaf_count,
            &r.binding.step_merkle_root,
        );
        assert_eq!(check_tir_cone_refutation_v1(&r, &RULES).unwrap().fault, PalwStepFaultV1::StepLeafCountNotCanonical, "{}", f.name);
        // A job longer than the class's context.
        let mut long = f.class.clone();
        long.layout.max_context = PREFILL + DECODE - 2;
        let class_id = long.class_id(&f.artifact_root);
        let mut r = honest.clone();
        r.binding.class = long;
        r.binding.job_context.shape_profile_id = class_id;
        let ctx_hash = r.binding.job_context.context_hash();
        r.binding.committed_execution_root = palw_tir_execution_root_v1(
            &ctx_hash,
            &r.binding.full_logits_trace_root,
            &class_id,
            r.binding.step_leaf_count,
            &r.binding.step_merkle_root,
        );
        assert_eq!(check_tir_cone_refutation_v1(&r, &RULES).unwrap().fault, PalwStepFaultV1::JobExceedsClassContext, "{}", f.name);
        // A binding whose parts do not produce its root is about another execution: refused.
        let mut r = honest.clone();
        r.binding.full_logits_trace_root = Hash64::from_bytes([9; 64]);
        assert!(matches!(check_tir_cone_refutation_v1(&r, &RULES), Err(PalwStepRefuteError::InputSetNotCanonical(_))));
        // A committed leaf with a value count that is not its coordinate's: convicted structurally.
        let mut x2 = x.clone();
        x2.preimages[5].value_count += 1;
        x2.preimages[5].values_le.extend_from_slice(&[0; 4]);
        let x2 = {
            let hashes: Vec<Hash64> = x2.preimages.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash_of(&f), &f.class_id, p)).collect();
            let root = step_merkle_root_v1(&hashes).unwrap();
            let mut b = x2.binding.clone();
            b.step_merkle_root = root;
            b.committed_execution_root =
                palw_tir_execution_root_v1(&ctx_hash_of(&f), &b.full_logits_trace_root, &f.class_id, b.step_leaf_count, &root);
            Execution { hashes, binding: b, ..x2 }
        };
        let mut r = honest.clone();
        rebind(&mut r, &x2);
        assert_eq!(check_tir_cone_refutation_v1(&r, &RULES).unwrap().fault, PalwStepFaultV1::StepValueCountNotCanonical, "{}", f.name);
    }
}

fn ctx_hash_of(f: &Fixture) -> Hash64 {
    f.ctx.context_hash()
}

#[test]
fn the_logits_trace_and_the_decode_tokens_are_held_to_the_step_leaves() {
    let mut checked = 0;
    for f in fixtures() {
        let x = f.honest();
        let post = (f.space.occurrences().len() - 1) as u32;
        let logits_leaves: Vec<usize> = f
            .leaves
            .iter()
            .enumerate()
            .filter(|(_, l)| matches!(l.kind, PalwTirLeafKindV1::Commit { occurrence, node, .. } if occurrence == post && node == f.space.program.logits))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(logits_leaves.len(), DECODE as usize, "{}: one logits tile per selecting row", f.name);
        let accuse = |x: &Execution, leaf: usize| {
            let l = &f.leaves[leaf];
            let row = l.position + 1 - PREFILL;
            let pin = tiled_decode_pin_v1(&f.ctx, &x.rows, &x.generated, row, 0).expect("pin");
            PalwTirLogitsConsistencyV1 {
                binding: x.binding.clone(),
                step_opening: step_opening_v1(&x.hashes, leaf as u64).unwrap(),
                step_preimage: x.preimages[leaf].clone(),
                trace: PalwTirTraceLanesV1::Tiled {
                    generated_token_ids: x.generated.clone(),
                    row_root: pin.row_root,
                    row_opening: pin.row_opening,
                    tile_lanes: pin.committed_tile_lanes.clone(),
                    tile_opening: pin.committed_opening.clone(),
                },
            }
        };
        for &leaf in &logits_leaves {
            // Tile 0 of each row: the pin's committed tile is the one holding lane `generated`, which
            // is tile 0 for these vocabularies.
            assert_eq!(
                check_tir_logits_consistency_v1(&accuse(&x, leaf), &RULES),
                Err(PalwStepRefuteError::NoFaultFound),
                "{}",
                f.name
            );
            // The trace row forged at lane 1: the executor committed two different rows.
            let row = (f.leaves[leaf].position + 1 - PREFILL) as usize;
            let mut rows = f.rows.clone();
            let lane = 1 % rows[row].len();
            rows[row][lane] += 1;
            let forged = f.commit(&f.values, &rows, &f.generated);
            let verdict = check_tir_logits_consistency_v1(&accuse(&forged, leaf), &RULES).expect("convicted");
            assert_eq!(verdict.fault, PalwStepFaultV1::TirLogitsTraceMismatch { value_index: lane as u32 });
        }
        // The decode-token door: the honest greedy token stands against every lane; a token that is
        // not the greedy one falls to the lane that beats it.
        if f.greedy {
            let vocab = f.rows[0].len() as u32;
            for row in 0..DECODE {
                for beat in 0..vocab.min(8) {
                    let pin = tiled_decode_pin_v1(&f.ctx, &x.rows, &x.generated, row, beat).unwrap();
                    assert_eq!(check_tir_decode_token_tiled_v1(&x.binding, &pin, &RULES), Err(PalwStepRefuteError::NoFaultFound));
                }
            }
            if vocab > 1 {
                let mut generated = f.generated.clone();
                let honest_pick = generated[0];
                generated[0] = (honest_pick + 1) % vocab;
                let forged = f.commit(&f.values, &f.rows, &generated);
                let pin = tiled_decode_pin_v1(&f.ctx, &forged.rows, &forged.generated, 0, honest_pick).unwrap();
                let verdict = check_tir_decode_token_tiled_v1(&forged.binding, &pin, &RULES).expect("convicted");
                assert_eq!(verdict.fault, PalwStepFaultV1::DecodeTokenMismatch { position: 0 }, "{}", f.name);
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 5);
}

#[test]
fn the_inventory_index_is_the_inventory() {
    for (name, program, _, _) in programs() {
        let index = PalwTirInventoryIndexV1::new(&program).expect("an inventory");
        if program.params.is_empty() {
            assert_eq!(index.leaf_count(), 0);
            continue;
        }
        let mut rows = Vec::new();
        palw_tir_visit_inventory_rows_v1(&program, &mut |r| rows.push(r)).expect("rows");
        assert_eq!(rows.len() as u32, index.leaf_count(), "{name}");
        for (i, r) in rows.iter().enumerate() {
            assert_eq!(index.piece_of(i as u32), Some((r.param, r.layer, r.row_start, r.len)), "{name} leaf {i}");
            for byte in [r.row_start as u64, (r.row_start + r.len - 1) as u64, (r.row_start + r.len / 2) as u64] {
                assert_eq!(index.leaf_of(r.param, r.layer, byte), Some(i as u32), "{name}");
                assert_eq!(palw_tir_leaf_index_v1(&program, r.param, r.layer, byte), Some(i as u32), "{name}");
            }
        }
        assert_eq!(index.piece_of(index.leaf_count()), None);
    }
}

#[test]
fn hostile_refutations_never_panic_the_court() {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut decoded = 0usize;
    for f in fixtures() {
        let x = f.honest();
        for leaf in [0usize, f.leaves.len() / 2, f.leaves.len() - 1] {
            let bytes = borsh::to_vec(&refute(&f, &x, leaf as u64)).unwrap();
            for _ in 0..40 {
                let mut m = bytes.clone();
                for _ in 0..1 + next() % 4 {
                    let at = (next() % m.len() as u64) as usize;
                    m[at] ^= 1 << (next() % 8);
                }
                if let Ok(r) = borsh::from_slice::<PalwTirConeRefutationV1>(&m) {
                    let _ = check_tir_cone_refutation_v1(&r, &RULES);
                    decoded += 1;
                }
            }
        }
    }
    assert!(decoded > 100, "{decoded} mutated refutations adjudicated without a panic");
}
