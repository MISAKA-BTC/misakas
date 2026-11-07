//! **The IR court's test fixture**, shared by `palw_tir_court.rs` and `palw_tir_dissect.rs`: every
//! golden program run as a job (a prompt, then greedily generated tokens fed back) under a layout
//! with ragged multi-tile commit points, two-position checkpoints and two-row history tiles, its
//! logits committed under the tiled scheme and its params as a TIR inventory; a store over any
//! commitment of its leaves; and the helpers that re-point a carriage at another commitment.
#![allow(dead_code, unused_imports)]

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
    check_tir_logits_consistency_v1, palw_tir_leaf_interval_v1, palw_tir_step_node_parts_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::{
    PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirLeafKindV1, PalwTirLeafV1, PalwTirStepBindingV1, PalwTirStepSpaceV1,
    palw_tir_execution_root_v1, palw_tir_leaf_preimage_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use misaka_palw_tir::demand::{DemandLimits, hist_row_node_v1};
use misaka_palw_tir::interval::{Interval, analyze_ranges};
use misaka_palw_tir::{DType, Interpreter, MapParams, RunState, Tensor, TirProgramV1};

pub fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

/// The golden programs: name, program, params, tokens.
pub fn programs() -> Vec<(String, TirProgramV1, MapParams, Vec<u32>)> {
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

pub struct TensorSrc<'a>(pub &'a MapParams);
impl PalwTirTensorSourceV1 for TensorSrc<'_> {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.tensors.get(&(param, layer)).map(|t| Cow::Owned(t.to_le_bytes()))
    }
}

pub const PREFILL: u32 = 4;
pub const DECODE: u32 = 3;
pub const RULES: PalwTirCourtRulesV1 =
    PalwTirCourtRulesV1 { max_step_leaf_count: 1 << 26, prompt_form: PalwPromptIdsFormV1::Flat, limits: DemandLimits::UNLIMITED };

/// A class, a job and its honest run.
pub struct Fixture {
    pub name: String,
    pub space: PalwTirStepSpaceV1,
    pub class: PalwTirClassV1,
    pub ctx: PalwJobContextV2,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub ops: Vec<PalwArtifactOperandV1>,
    pub prompt: Vec<u32>,
    pub generated: Vec<u32>,
    pub rows: Vec<Vec<i32>>,
    pub leaves: Vec<PalwTirLeafV1>,
    pub values: Vec<Vec<i128>>,
    /// The program's proven intervals — `None` for a program the range analysis refuses (no such
    /// class is ever admitted; the golden state programs are evaluator vectors, not classes).
    pub intervals: Option<Vec<Vec<Interval>>>,
    /// Whether every generated token is the greedy selection over its row.
    pub greedy: bool,
}

/// One committed execution: the leaf preimages, their hashes, the trace, the binding.
#[derive(Clone)]
pub struct Execution {
    pub preimages: Vec<PalwStepTileLeafV1>,
    pub hashes: Vec<Hash64>,
    pub rows: Vec<Vec<i32>>,
    pub generated: Vec<u32>,
    pub binding: PalwTirStepBindingV1,
}

pub fn layout(p: &TirProgramV1, positions: u32) -> PalwTirLayoutV1 {
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

pub fn fixture(name: String, program: TirProgramV1, params: MapParams, tokens: Vec<u32>) -> Fixture {
    fixture_with(name, program, params, tokens, PREFILL, DECODE)
}

/// [`fixture`] with a job of `prefill` prompt tokens and `decode` generated ones — a longer history
/// for the dissection's chain runs (RFC-0002 F7).
pub fn fixture_with(name: String, program: TirProgramV1, params: MapParams, tokens: Vec<u32>, prefill: u32, decode: u32) -> Fixture {
    fixture_tiled(name, program, params, tokens, prefill, decode, 4096)
}

/// [`fixture_with`] with the logits node tiled at `logits_tile` lanes (a divisor of the tiled
/// scheme's 4,096: spec 04b §10.3's carriable-close rule lets a class tile its logits finer).
pub fn fixture_tiled(
    name: String,
    mut program: TirProgramV1,
    params: MapParams,
    tokens: Vec<u32>,
    prefill: u32,
    decode: u32,
    logits_tile: u32,
) -> Fixture {
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let bytes = program.encode();
    let program = TirProgramV1::decode_canonical(&bytes).expect("still canonical under the tiled scheme");
    let positions = prefill + decode - 1;
    let prompt: Vec<u32> = (0..prefill as usize).map(|i| tokens[i % tokens.len()]).collect();
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
        let token = if a < prefill { prompt[a as usize] } else { generated[(a - prefill) as usize] };
        let step = interp.step(&params, &mut state, token).expect("an honest step");
        let mut c: BTreeMap<(usize, u16), Tensor> = BTreeMap::new();
        for r in step.commits {
            c.insert((occ_of(r.block, r.layer), r.node), r.value);
        }
        commits.push(c);
        after.push(state.clone());
        if a + 1 >= prefill {
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
    let mut lay = layout(&program, positions);
    let logits_index = program
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(bi, b)| b.nodes.iter().enumerate().filter(|(_, n)| n.commit).map(move |(ni, _)| (bi, ni)))
        .position(|(bi, ni)| bi == program.schedule.post as usize && ni == program.logits as usize)
        .expect("the logits node commits");
    lay.commit_tiles[logits_index] = logits_tile;
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
        declared_prefill_tokens: prefill,
        exact_decode_tokens: decode,
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
pub fn fixtures() -> Vec<Fixture> {
    let all: Vec<Fixture> = programs().into_iter().map(|(n, p, params, t)| fixture(n, p, params, t)).collect();
    let admissible: Vec<Fixture> = all.into_iter().filter(|f| f.intervals.is_some()).collect();
    assert_eq!(admissible.len(), 5, "the five corpus models are admissible");
    admissible
}

/// Lanes as committed: `i32` little-endian, `idx` as `u32` — written raw, so a test can commit a
/// lane outside its node's dtype (the builder would refuse to).
pub fn raw_lanes(dtype: DType, values: &[i128]) -> Vec<u8> {
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
    pub fn commit(&self, values: &[Vec<i128>], rows: &[Vec<i32>], generated: &[u32]) -> Execution {
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

    pub fn honest(&self) -> Execution {
        self.commit(&self.values, &self.rows, &self.generated)
    }

    pub fn interval(&self, leaf: &PalwTirLeafV1) -> Interval {
        palw_tir_leaf_interval_v1(&self.space, self.intervals.as_ref().expect("admissible"), leaf).expect("interval")
    }
}

pub struct Store<'a> {
    pub f: &'a Fixture,
    pub x: &'a Execution,
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
    fn step_node(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
        palw_tir_step_node_parts_v1(&self.x.hashes, level, index)
    }
    fn row_node(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
        kaspa_consensus_core::palw_tir_court_v1::palw_tir_row_node_parts_v1(&self.f.ctx, &self.x.rows, level, index)
    }
}

pub fn refute(f: &Fixture, x: &Execution, leaf: u64) -> PalwTirConeRefutationV1 {
    build_tir_cone_refutation_v1(&x.binding, leaf, &Store { f, x }, &RULES).unwrap_or_else(|e| panic!("{}: leaf {leaf}: {e}", f.name))
}

/// Re-point a refutation at another commitment of the same leaves (its openings and run siblings).
pub fn rebind(r: &mut PalwTirConeRefutationV1, x: &Execution) {
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
