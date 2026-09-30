//! **The node's step leg of an IR job equals Phase F's (F4), leaf for leaf** (feature `node`).
//!
//! For every golden program, the corpus programs of `misaka-palw-tir/tests` and a program whose
//! committed nodes carry `H`, under several layouts and jobs: the REFERENCE evaluator runs the job
//! (prompt, then argmax-selected tokens — `base0_decode_token_select_v1`), its values fill the
//! consensus fail-closed builder (`PalwTirStepLegBuilderV1`) in F4's order, and the typed backend's
//! producer (`TirClassRunnerV1`) — which checks every position's leaves against F4's enumeration as
//! it goes — must reproduce the leaf count, the step Merkle root, the generated tokens, the logits
//! trace root and the IR execution root.
#![cfg(feature = "node")]

mod node_common;

use std::collections::BTreeMap;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step_refute::{base0_decode_token_select_v1, base0_logits_trace_root_v1, flat_logits_scheme_id_v1};
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1,
};
use kaspa_consensus_core::palw_tir_step_v1::{
    PalwTirLeafKindV1, PalwTirStepLegBuilderV1, PalwTirStepSpaceV1, palw_tir_execution_root_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use misaka_palw_tir::program::Ref;
use misaka_palw_tir::{Interpreter, MapParams, RunState, Tensor, TirProgramV1};
use misaka_palw_tir_exec::node::TirClassRunnerV1;
use misaka_palw_tir_exec::{TirParams, TirPlan};
use node_common::programs;

fn layout(p: &TirProgramV1, seed: u32, c: u32, h: u32, max_context: u32) -> PalwTirLayoutV1 {
    let committed: usize = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum();
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context,
        checkpoint_interval: c,
        h_tile: h,
        commit_tiles: (0..committed as u32).map(|k| 4 + (k.wrapping_mul(seed) % 6)).collect(),
        state_tiles: p.states.iter().enumerate().map(|(j, _)| 4 + ((j as u32 * seed) % 3)).collect(),
    }
}

fn job(prefill: u32, decode: u32, class_id: Hash64) -> PalwJobContextV2 {
    let z = Hash64::from_bytes([0u8; 64]);
    PalwJobContextV2 {
        version: 2,
        network_id: b"testnet-12".to_vec(),
        job_id: Hash64::from_bytes([prefill as u8; 64]),
        job_nullifier: z,
        assignment_id: z,
        execution_seed: [decode as u8; 32],
        model_profile_id: z,
        runtime_manifest_hash: z,
        runtime_class_id: z,
        shape_profile_id: class_id,
        trace_scheme_id: flat_logits_scheme_id_v1(),
        cu_ruleset_id: z,
        tokenizer_id: z,
        prompt_token_ids_hash: z,
        declared_prefill_tokens: prefill,
        exact_decode_tokens: decode,
        max_context_tokens: 1024,
    }
}

struct Honest {
    values: Vec<Vec<i128>>,
    generated: Vec<u32>,
    rows: Vec<Vec<i32>>,
}

/// The job on the REFERENCE evaluator, and every leaf's values in F4's order: commit points from
/// the step's records, checkpoints from the run state after the position, history tiles from the
/// rows each `HistAppend` appended (its input: a committed node or a carry-in, NF-20).
fn honest(space: &PalwTirStepSpaceV1, ctx: &PalwJobContextV2, params: &MapParams, prompt: &[u32]) -> Honest {
    let p = &space.program;
    let interp = Interpreter::new(p).unwrap();
    let mut state = RunState::default();
    let prefill = ctx.declared_prefill_tokens;
    let positions = prefill + ctx.exact_decode_tokens - 1;
    let mut appended: BTreeMap<(u16, Option<u16>), Vec<Vec<i128>>> = BTreeMap::new();
    let (mut values, mut generated, mut rows) = (Vec::new(), Vec::new(), Vec::new());
    for a in 0..positions {
        let token = if a < prefill { prompt[a as usize] } else { generated[(a - prefill) as usize] };
        let step = interp.step(params, &mut state, token).unwrap();
        if a + 1 >= prefill {
            let row: Vec<i32> = step.logits.data.iter().map(|v| *v as i32).collect();
            generated.push(base0_decode_token_select_v1(&row) as u32);
            rows.push(row);
        }
        let mut commits: BTreeMap<(u8, Option<u16>, u16), Tensor> = BTreeMap::new();
        for c in &step.commits {
            commits.insert((c.block, c.layer, c.node), c.value.clone());
        }
        let occ = p.occurrences();
        for (i, (b, layer)) in occ.iter().enumerate() {
            for n in &p.blocks[*b as usize].nodes {
                if let misaka_palw_tir::Prim::HistAppend { state: j } = n.prim {
                    let row = match n.inputs[0] {
                        Ref::Node(k) => commits[&(*b, *layer, k)].data.clone(),
                        Ref::CarryIn(k) => {
                            let (pb, pl) = occ[i - 1];
                            commits[&(pb, pl, p.blocks[pb as usize].carry_out[k as usize])].data.clone()
                        }
                        _ => unreachable!("NF-20"),
                    };
                    appended.entry((j, if p.states[j as usize].per_layer { *layer } else { None })).or_default().push(row);
                }
            }
        }
        for leaf in space.leaves_of_position(ctx, a) {
            let n = leaf.value_count as usize;
            values.push(match leaf.kind {
                PalwTirLeafKindV1::Commit { block, layer, node, first_element, .. } => {
                    commits[&(block, layer, node)].data[first_element as usize..first_element as usize + n].to_vec()
                }
                PalwTirLeafKindV1::State { state: j, layer, first_element, .. } => {
                    let s = &p.states[j as usize];
                    let shape: Vec<usize> = s.shape.iter().map(|d| *d as usize).collect();
                    let v = state.fixed.get(&(j, layer)).cloned().unwrap_or_else(|| Tensor::zeros(s.dtype, &shape));
                    v.data[first_element as usize..first_element as usize + n].to_vec()
                }
                PalwTirLeafKindV1::HistTile { state: j, layer, first_lane, row_lanes, first_position, .. } => {
                    let all = &appended[&(j, layer)];
                    (first_position..=a)
                        .flat_map(|t| all[t as usize][first_lane as usize..first_lane as usize + row_lanes as usize].to_vec())
                        .collect()
                }
            });
        }
    }
    Honest { values, generated, rows }
}

#[test]
fn the_node_leg_equals_the_reference_through_the_consensus_builder() {
    let mut runs = 0usize;
    let mut leaves = 0u64;
    for (name, program, params) in programs() {
        let bytes = program.encode();
        let plan = TirPlan::compile(&program).unwrap_or_else(|e| panic!("{name}: {e}"));
        let tparams = TirParams::from_map(&plan, &params).unwrap_or_else(|e| panic!("{name}: {e}"));
        for (seed, c, h, prefill, decode) in [(7u32, 2u32, 2u32, 3u32, 3u32), (3, 1, 4, 5, 4), (5, 3, 1, 1, 6), (11, 4, 8, 6, 5)] {
            let class = PalwTirClassV1 {
                version: PALW_TIR_CLASS_VERSION_V1,
                program: bytes.clone(),
                layout: layout(&program, seed, c, h, 64),
                tokenizer_id: Hash64::from_bytes([1; 64]),
            };
            let artifact_root = Hash64::from_bytes([0xA7; 64]);
            let class_id = class.class_id(&artifact_root);
            let space = PalwTirStepSpaceV1::new(&class).unwrap_or_else(|e| panic!("{name}: {e}"));
            let ctx = job(prefill, decode, class_id);
            let prompt: Vec<u32> = (0..prefill).map(|i| (i * 5 + 3) % program.token_bound).collect();
            let honest = honest(&space, &ctx, &params, &prompt);
            let mut builder = PalwTirStepLegBuilderV1::new(&space, &ctx, class_id, u64::MAX).unwrap();
            for v in &honest.values {
                builder.push(v).unwrap_or_else(|e| panic!("{name}: {e}"));
            }
            let (count, root) = builder.finish().unwrap();
            let trace = base0_logits_trace_root_v1(&ctx, &honest.rows, &honest.generated);
            let runner = TirClassRunnerV1::new(&space, &plan, &tparams, class_id).unwrap();
            let mut seen = 0u64;
            let run = runner
                .run(&ctx, &prompt, u64::MAX, true, &mut |leaf| {
                    assert_eq!(leaf.index, seen);
                    seen += 1;
                })
                .unwrap_or_else(|e| panic!("{name} (seed {seed}, C {c}, h {h}, {prefill}+{decode}): {e}"));
            assert_eq!(run.leaf_count, count, "{name}: leaf count");
            assert_eq!(run.step_merkle_root, root, "{name} (seed {seed}): the step root");
            assert_eq!(run.generated, honest.generated, "{name}: generated tokens");
            assert_eq!(run.logits_rows, honest.rows, "{name}: logits rows");
            assert_eq!(run.trace_root, trace, "{name}: the logits trace");
            assert_eq!(
                run.execution_root,
                palw_tir_execution_root_v1(&ctx.context_hash(), &trace, &class_id, count, &root),
                "{name}: the execution root"
            );
            runs += 1;
            leaves += count;
        }
    }
    eprintln!("{runs} jobs, {leaves} leaves: node leg = reference leg");
    assert!(runs >= 40);
}

/// A PALWTIR1 container written, then opened MAPPED: the params are served in place, the streamed
/// inventory root is the consensus inventory over the same bytes, and a job run from the mapping
/// commits exactly what the in-memory params commit.
#[test]
fn a_mapped_container_runs_like_the_params_it_holds() {
    use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_root_v1};
    use misaka_palw_tir_exec::node::TirArtifactV1;
    use std::borrow::Cow;
    let dir = std::env::temp_dir().join(format!("tir-exec-node-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // (A program without params has no inventory and no root: the consensus inventory refuses it.)
    let mut ran = 0;
    for (name, program, params) in
        programs().into_iter().filter(|(n, p, _)| (n.starts_with("corpus") || n.contains("hist")) && !p.params.is_empty())
    {
        let lay = layout(&program, 7, 2, 4, 64);
        let path = dir.join(format!("{}.palwtir", name.replace(' ', "-")));
        let tensor = |j: u16, l: Option<u16>| -> Result<Vec<u8>, String> {
            params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
        };
        let mut tensor = tensor;
        misaka_palw_tir_artifact::write_container_v1(
            &path,
            &program,
            borsh::to_vec(&lay).unwrap(),
            [1; 64],
            String::new(),
            &mut tensor,
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let art = TirArtifactV1::open(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        // The inventory root, streamed from the mapping = the consensus inventory over the map.
        struct Map<'m>(&'m MapParams);
        impl PalwTirTensorSourceV1 for Map<'_> {
            fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
                self.0.tensors.get(&(param, layer)).map(|t| Cow::Owned(t.to_le_bytes()))
            }
        }
        let (root, count) = art.inventory_root().unwrap();
        assert_eq!((root, count), palw_tir_inventory_root_v1(&program, &Map(&params)).unwrap(), "{name}");
        // The class it declares, and a job from the mapping against a job from memory.
        let class = art.class().unwrap();
        assert_eq!(class.layout, lay);
        let class_id = class.class_id(&root);
        let space = PalwTirStepSpaceV1::new(&class).unwrap();
        let ctx = job(4, 4, class_id);
        let prompt: Vec<u32> = (0..4).map(|i| (i * 7 + 1) % program.token_bound).collect();
        let mapped =
            TirClassRunnerV1::new(&space, art.plan(), art.params(), class_id).unwrap().run(&ctx, &prompt, u64::MAX, true, &mut |_| {});
        let plan = TirPlan::compile(&program).unwrap();
        let tp = TirParams::from_map(&plan, &params).unwrap();
        let memory = TirClassRunnerV1::new(&space, &plan, &tp, class_id).unwrap().run(&ctx, &prompt, u64::MAX, true, &mut |_| {});
        assert_eq!(mapped.unwrap(), memory.unwrap(), "{name}: the mapped artifact commits what its params commit");
        std::fs::remove_file(&path).ok();
        ran += 1;
    }
    std::fs::remove_dir_all(&dir).ok();
    assert!(ran >= 5, "{ran} containers");
}

/// **Resume at every checkpoint** (design §2.6): a run records its resume points; from each —
/// as recorded, and cut to the rows it must carry — a fresh executor continues the job, and its
/// leaves from the point's index on, its logits rows and its tokens are the uninterrupted run's.
/// A point cut below that never continues into different leaves: it is refused or, when no later
/// window or tile reads the missing rows, it commits the same leaves.
#[test]
fn a_run_resumed_at_any_checkpoint_continues_leaf_for_leaf() {
    let (mut resumed, mut refused, mut leaves) = (0usize, 0usize, 0u64);
    for (name, program, params) in programs() {
        let bytes = program.encode();
        let plan = TirPlan::compile(&program).unwrap();
        let tparams = TirParams::from_map(&plan, &params).unwrap();
        for (seed, c, h, prefill, decode) in
            [(7u32, 2u32, 2u32, 3u32, 3u32), (3, 1, 4, 5, 4), (5, 3, 1, 1, 6), (11, 4, 8, 6, 5), (13, 3, 4, 2, 7)]
        {
            let class = PalwTirClassV1 {
                version: PALW_TIR_CLASS_VERSION_V1,
                program: bytes.clone(),
                layout: layout(&program, seed, c, h, 64),
                tokenizer_id: Hash64::from_bytes([1; 64]),
            };
            let class_id = class.class_id(&Hash64::from_bytes([0xA7; 64]));
            let space = PalwTirStepSpaceV1::new(&class).unwrap();
            let ctx = job(prefill, decode, class_id);
            let prompt: Vec<u32> = (0..prefill).map(|i| (i * 5 + 3) % program.token_bound).collect();
            let runner = TirClassRunnerV1::new(&space, &plan, &tparams, class_id).unwrap();
            let full = runner.run(&ctx, &prompt, u64::MAX, true, &mut |_| {}).unwrap();
            let (recorded, points) = runner.run_recording(&ctx, &prompt, u64::MAX, &mut |_| {}).unwrap();
            assert_eq!(recorded, full, "{name}: a recording run commits what a run commits");
            let positions = prefill + decode - 1;
            assert_eq!(points.len() as u32, positions / c, "{name}: one point per checkpoint position");
            for point in &points {
                let pos = point.position + 1;
                assert_eq!(pos % c, 0);
                let windows: Vec<usize> = space
                    .hist_instances()
                    .iter()
                    .map(|i| match program.states[i.state as usize].kind {
                        misaka_palw_tir::StateKind::Hist { window } => window as usize,
                        _ => unreachable!(),
                    })
                    .collect();
                let need: Vec<usize> = windows.iter().map(|w| (pos as usize).min(w - 1).max((pos % h) as usize)).collect();
                for (rows, n) in point.hist.iter().zip(&need) {
                    assert!(rows.len() >= *n, "{name}: a point with {} rows of the {n} it must carry", rows.len());
                }
                let cut = |extra: usize| {
                    let mut p = point.clone();
                    for (rows, n) in p.hist.iter_mut().zip(&need) {
                        let keep = n.saturating_sub(extra);
                        rows.drain(..rows.len() - keep);
                    }
                    p
                };
                let selected = pos.saturating_sub(prefill - 1) as usize;
                for p in [point.clone(), cut(0)] {
                    let mut seen = Vec::new();
                    let r = runner
                        .resume(&ctx, &prompt, u64::MAX, &p, &mut |leaf| seen.push(leaf.index))
                        .unwrap_or_else(|e| panic!("{name} (seed {seed}, C {c}, h {h}) at {}: {e}", point.position));
                    let first = r.first_index as usize;
                    assert_eq!(first as u64, space.running_total(&space.job_shape(&ctx).unwrap(), pos) as u64);
                    assert_eq!(r.leaf_hashes, full.leaf_hashes[first..], "{name} at {}: the leaves", point.position);
                    assert_eq!(seen, (first as u64..full.leaf_count).collect::<Vec<_>>(), "{name}: leaf indices");
                    assert_eq!(r.logits_rows, full.logits_rows[selected..], "{name}: logits rows");
                    assert_eq!(r.generated, full.generated, "{name}: tokens");
                    resumed += 1;
                    leaves += r.leaf_hashes.len() as u64;
                }
                if need.iter().any(|n| *n > 0) {
                    match runner.resume(&ctx, &prompt, u64::MAX, &cut(1), &mut |_| {}) {
                        Err(_) => refused += 1,
                        Ok(r) => assert_eq!(r.leaf_hashes, full.leaf_hashes[r.first_index as usize..], "{name}: a short point"),
                    }
                }
            }
            // A point with a token too many, or past the job's end, is refused.
            if let Some(point) = points.first() {
                let mut p = point.clone();
                p.generated.push(0);
                assert!(runner.resume(&ctx, &prompt, u64::MAX, &p, &mut |_| {}).is_err(), "{name}: an extra token");
                p = point.clone();
                p.position = positions;
                assert!(runner.resume(&ctx, &prompt, u64::MAX, &p, &mut |_| {}).is_err(), "{name}: a point past the end");
            }
        }
    }
    eprintln!("{resumed} resumed runs, {leaves} leaves; {refused} short points refused");
    assert!(resumed >= 200 && refused > 0, "{resumed} resumed, {refused} refused");
}
