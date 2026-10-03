//! **RFC-0006: a seat verifies a cell from committed boundary rows and only its shard's weights** (feature `node`).
//!
//! For every golden program, the corpus models and a program whose committed nodes carry `H`, under several layouts and jobs:
//!
//! * **honest**: the job runs once as a producer would (`TirClassRunnerV1`), every leaf is captured, and every cell of every
//!   layer partition (two and three shards) × every position cut at the layout's alignment `lcm(C, h_tile)` verifies — each from
//!   an executor that holds ONLY its occurrences' params (a cell that read another shard's weight would be `Missing`);
//! * **a pipelined producer** (one executor per shard, the shards' carry handed on) commits exactly what the whole-schedule
//!   producer commits, leaf for leaf — the cell machinery is the program's function;
//! * **a lie inside a cell** (a commit leaf, re-rooted) is found by the cell that holds it, at that leaf;
//! * **a consistent lie at a boundary row** (the pipelined producer lies in the carry it hands the next shard and computes
//!   downstream honestly from the lie) passes the DOWNSTREAM cell and is found by the UPSTREAM one — the detection lemma's first
//!   consequence;
//! * a wrong generated id is a token fault of the last shard's cell; an input the source lacks is `Unavailable`; a cell over runs
//!   (`TirRunInputsV1`, the `TirStepRun` form) verifies the same and refuses a run that does not reach the root.
#![cfg(feature = "node")]

mod node_common;

use std::collections::BTreeMap;
use std::ops::Range;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step_leg::{
    PalwStepRangeOpeningV1, PalwStepTileLeafV1, step_merkle_range_siblings_v1, step_merkle_root_v1, step_tile_leaf_hash_v1,
};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_consensus_core::palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirStepSpaceV1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use misaka_palw_tir::{MapParams, Prim, TirProgramV1};
use misaka_palw_tir_exec::node::{
    CpuKernelBackendV1, KernelBackendV1, TirCaptureInputsV1, TirCellRequestV1, TirCellV1, TirCellVerdictV1, TirClassRunnerV1,
    TirRunInputsV1, cell_runs_v1,
};
use misaka_palw_tir_exec::{NoSink, StepSink, TirExecutor, TirParams, TirPlan};
use node_common::{job, layout, programs};

struct Setup {
    space: PalwTirStepSpaceV1,
    class_id: Hash64,
    ctx: PalwJobContextV2,
    tokens: Vec<u32>,
    leaves: Vec<PalwStepTileLeafV1>,
    root: Hash64,
    n_occ: usize,
}

fn setup(name: &str, program: &TirProgramV1, params: &MapParams, seed: u32, c: u32, h: u32, prefill: u32, decode: u32) -> Setup {
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: layout(program, seed, c, h, 64),
        tokenizer_id: Hash64::from_bytes([1; 64]),
    };
    let class_id = class.class_id(&Hash64::from_bytes([0xA7; 64]));
    let space = PalwTirStepSpaceV1::new(&class).unwrap_or_else(|e| panic!("{name}: {e}"));
    let prompt: Vec<u32> = (0..prefill).map(|i| (i * 5 + 3) % program.token_bound).collect();
    let ctx = job(program, prefill, decode, class_id, &prompt, kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat);
    let plan = TirPlan::compile(program).unwrap();
    let tparams = TirParams::from_map(&plan, params).unwrap();
    let runner = TirClassRunnerV1::new(&space, &plan, &tparams, class_id).unwrap();
    let mut leaves = Vec::new();
    let run =
        runner.run(&ctx, &prompt, u64::MAX, false, &mut |l| leaves.push(l.preimage.clone())).unwrap_or_else(|e| panic!("{name}: {e}"));
    let mut tokens = prompt.clone();
    tokens.extend(run.generated.iter().copied());
    Setup { n_occ: space.occurrences().len(), space, class_id, ctx, tokens, root: run.step_merkle_root, leaves }
}

/// A partition of the layers into `s` contiguous shards, as occurrence ranges: `pre` with the first, `post` with the last.
fn shards(n_occ: usize, s: usize) -> Vec<Range<usize>> {
    let layers = n_occ - 2;
    if s > layers.max(1) {
        return vec![0..n_occ];
    }
    let cuts: Vec<usize> = (0..=s).map(|i| 1 + layers * i / s).collect();
    (0..s).map(|i| (if i == 0 { 0 } else { cuts[i] })..(if i + 1 == s { n_occ } else { cuts[i + 1] })).collect()
}

fn segments(positions: u32, g: u32, k: u32) -> Vec<Range<u32>> {
    let mut cuts: Vec<u32> = (0..=k).map(|j| (positions * j / k) / g * g).collect();
    *cuts.last_mut().unwrap() = positions;
    cuts.dedup();
    cuts.windows(2).map(|w| w[0]..w[1]).collect()
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// The params of exactly one shard: every instance an occurrence of `occ` reads (and no other).
fn shard_params<'a>(plan: &TirPlan, all: &MapParams, program: &TirProgramV1, occ: &Range<usize>) -> TirParams<'a> {
    let mut only = MapParams::default();
    for o in occ.clone() {
        let (block, layer) = plan.occurrences[o];
        for n in &program.blocks[block as usize].nodes {
            for r in &n.inputs {
                if let misaka_palw_tir::Ref::Param(j) = r {
                    let l = if program.params[*j as usize].per_layer { layer } else { None };
                    if let Some(t) = all.tensors.get(&(*j, l)) {
                        only.tensors.insert((*j, l), t.clone());
                    }
                }
            }
        }
    }
    TirParams::from_map_lenient(plan, &only).unwrap()
}

fn verify(
    s: &Setup,
    plan: &TirPlan,
    program: &TirProgramV1,
    params: &MapParams,
    cell: &TirCellV1,
    leaves: &[PalwStepTileLeafV1],
    root: &Hash64,
    tokens: &[u32],
) -> TirCellVerdictV1 {
    let inputs = TirCaptureInputsV1::new(leaves, &s.ctx, &s.class_id, root, u64::MAX).expect("an authenticated capture");
    let sp = shard_params(plan, params, program, &cell.occ);
    let req = TirCellRequestV1 {
        space: &s.space,
        plan,
        params: &sp,
        class_id: s.class_id,
        ctx: &s.ctx,
        cell,
        tokens,
        inputs: &inputs,
        fused: false,
    };
    CpuKernelBackendV1.verify_cell(&req).expect("the CPU backend runs every cell")
}

const LAYOUTS: [(u32, u32, u32, u32, u32); 3] = [(7, 2, 2, 5, 4), (3, 1, 4, 6, 3), (11, 4, 2, 4, 6)];

#[test]
fn every_cell_of_every_partition_verifies_from_its_boundary_rows_and_its_shards_weights() {
    let (mut cells, mut leaves_checked) = (0usize, 0u64);
    for (name, program, params) in programs() {
        let plan = TirPlan::compile(&program).unwrap();
        for (seed, c, h, prefill, decode) in LAYOUTS {
            let s = setup(&name, &program, &params, seed, c, h, prefill, decode);
            let job_positions = prefill + decode - 1;
            let g = c / gcd(c, h) * h;
            for n_shards in [2usize, 3] {
                for occ in shards(s.n_occ, n_shards) {
                    for k in [1u32, 2] {
                        for positions in segments(job_positions, g, k) {
                            let cell = TirCellV1 { shard: 0, occ: occ.clone(), positions: positions.clone() };
                            let v = verify(&s, &plan, &program, &params, &cell, &s.leaves, &s.root, &s.tokens);
                            match v {
                                TirCellVerdictV1::Verified { leaves, .. } => leaves_checked += leaves,
                                other => panic!("{name} (C {c}, h {h}, {n_shards} shards) cell {occ:?} x {positions:?}: {other:?}"),
                            }
                            cells += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!("{cells} cells verified, {leaves_checked} leaves recomputed");
    assert!(cells > 200);
}

/// The pipelined producer: one executor per shard (each holding only its params), the carry handed on at the boundaries, an
/// optional lie injected in the carry handed to shard `lie.0` at position `lie.1`. Returns every leaf's preimage in step order.
fn pipelined(
    s: &Setup,
    plan: &TirPlan,
    program: &TirProgramV1,
    params: &MapParams,
    cuts: &[Range<usize>],
    lie: Option<(usize, u32)>,
) -> Vec<PalwStepTileLeafV1> {
    use kaspa_consensus_core::palw_step::PalwStepCoordinateV1;
    let job = s.space.job_shape(&s.ctx).unwrap();
    let ctx_hash = s.ctx.context_hash();
    let sps: Vec<TirParams> = cuts.iter().map(|occ| shard_params(plan, params, program, occ)).collect();
    let mut execs: Vec<TirExecutor> =
        cuts.iter().zip(&sps).map(|(occ, sp)| TirExecutor::new_cell(plan, sp, occ.clone()).unwrap()).collect();
    for e in execs.iter_mut() {
        e.set_hist_tail(s.space.layout.h_tile as usize);
    }
    let mut out: Vec<PalwStepTileLeafV1> = Vec::new();
    for a in 0..job.positions {
        let listed = s.space.leaves_of_position(&s.ctx, a);
        let (call, position) = job.call_position(a);
        let mut by_slot: BTreeMap<(u32, u32), (u32, Vec<u8>)> = BTreeMap::new();
        struct Sink<'t> {
            tile_len: &'t BTreeMap<u32, usize>,
            tiles: &'t mut BTreeMap<(u32, u32), (u32, Vec<u8>)>,
        }
        impl StepSink for Sink<'_> {
            fn node(&mut self, v: &misaka_palw_tir_exec::NodeValue<'_>) {
                if !v.commit {
                    return;
                }
                let t = self.tile_len[&v.slot];
                let n = v.data.len();
                for (tile, from) in (0..n).step_by(t).enumerate() {
                    let count = t.min(n - from);
                    let lanes: Vec<u8> = v.data.to_i128s()[from..from + count]
                        .iter()
                        .flat_map(|x| {
                            if v.dtype == misaka_palw_tir::DType::Idx { (*x as u32).to_le_bytes() } else { (*x as i32).to_le_bytes() }
                        })
                        .collect();
                    self.tiles.insert((v.slot, tile as u32), (count as u32, lanes));
                }
            }
        }
        let mut tile_len = BTreeMap::new();
        for (o, (block, _)) in s.space.occurrences().iter().enumerate() {
            for (ni, node) in program.blocks[*block as usize].nodes.iter().enumerate() {
                if node.commit {
                    tile_len.insert(
                        s.space.node_slot(o, ni as u16).unwrap(),
                        s.space.commit_tile_len(*block, ni as u16).unwrap() as usize,
                    );
                }
            }
        }
        let mut carry: Vec<Vec<i128>> = Vec::new();
        for (i, occ) in cuts.iter().enumerate() {
            let end = if job.runs_post(a) { occ.end } else { occ.end.min(s.n_occ - 1) };
            if end <= occ.start {
                continue;
            }
            if let Some((shard, pos)) = lie
                && shard == i
                && pos == a
                && let Some(first) = carry.first_mut().and_then(|c| c.first_mut())
            {
                *first = if *first == 5 { 6 } else { 5 };
                // The producer COMMITS the lie as the boundary row it hands on (the next shard's seat reads it from the leaf).
                let prev = cuts[i].start - 1;
                let (block, _) = s.space.occurrences()[prev];
                let node = program.blocks[block as usize].carry_out[0];
                let slot = s.space.node_slot(prev, node).unwrap();
                let lanes = &mut by_slot.get_mut(&(slot, 0)).expect("the carry-out is committed").1;
                lanes[0..4].copy_from_slice(&(*first as i32).to_le_bytes());
            }
            let token = s.tokens[a as usize];
            carry = execs[i]
                .step_cell(token, occ.start..end, &carry, &mut Sink { tile_len: &tile_len, tiles: &mut by_slot })
                .unwrap_or_else(|e| panic!("shard {i} at position {a}: {e}"));
        }
        let c_int = s.space.layout.checkpoint_interval;
        let h_t = s.space.layout.h_tile as usize;
        for leaf in &listed {
            let (values_le, count) = match leaf.kind {
                PalwTirLeafKindV1::Commit { .. } => {
                    by_slot.get(&(leaf.coord.node_slot, leaf.coord.tile_index)).cloned().map(|(c, l)| (l, c)).expect("a commit tile")
                }
                PalwTirLeafKindV1::State { instance, first_element, .. } => {
                    assert!((a + 1) % c_int == 0);
                    let inst = &s.space.fixed_instances()[instance as usize];
                    let owner = cuts.iter().position(|occ| shard_writes(&s.space, occ, inst.state, inst.layer)).expect("a writer");
                    let v = execs[owner].fixed_value(inst.state, inst.layer).unwrap();
                    let vals = v.to_i128s();
                    let lanes: Vec<u8> = vals[first_element as usize..first_element as usize + leaf.value_count as usize]
                        .iter()
                        .flat_map(|x| {
                            if inst.dtype == misaka_palw_tir::DType::Idx {
                                (*x as u32).to_le_bytes()
                            } else {
                                (*x as i32).to_le_bytes()
                            }
                        })
                        .collect();
                    (lanes, leaf.value_count)
                }
                PalwTirLeafKindV1::HistTile { instance, first_lane, row_lanes, .. } => {
                    let inst = &s.space.hist_instances()[instance as usize];
                    let owner = cuts.iter().position(|occ| shard_writes(&s.space, occ, inst.state, inst.layer)).expect("a writer");
                    let tail = execs[owner].hist_tail(inst.state, inst.layer).unwrap();
                    let mut lanes = Vec::new();
                    for row in tail.iter().skip(tail.len() - h_t) {
                        for x in &row[first_lane as usize..first_lane as usize + row_lanes as usize] {
                            lanes.extend_from_slice(&x.to_le_bytes());
                        }
                    }
                    (lanes, leaf.value_count)
                }
            };
            out.push(PalwStepTileLeafV1 {
                version: 1,
                coord: PalwStepCoordinateV1 {
                    call_index: call,
                    node_slot: leaf.coord.node_slot,
                    position,
                    tile_index: leaf.coord.tile_index,
                },
                value_count: count,
                values_le,
            });
        }
        let _ = ctx_hash;
    }
    out
}

fn shard_writes(space: &PalwTirStepSpaceV1, occ: &Range<usize>, state: u16, layer: Option<u16>) -> bool {
    occ.clone().any(|o| {
        let (block, ol) = space.occurrences()[o];
        space.program.blocks[block as usize]
            .nodes
            .iter()
            .any(|n| matches!(n.prim, Prim::StateWrite { state: s } | Prim::HistAppend { state: s } if s == state))
            && (layer.is_none() || layer == ol)
    })
}

#[test]
fn a_pipelined_producer_commits_what_the_whole_schedule_producer_commits() {
    let mut programs_run = 0;
    for (name, program, params) in programs() {
        let plan = TirPlan::compile(&program).unwrap();
        let s = setup(&name, &program, &params, 7, 2, 2, 5, 4);
        for n_shards in [2usize, 3] {
            let cuts = shards(s.n_occ, n_shards);
            let leaves = pipelined(&s, &plan, &program, &params, &cuts, None);
            assert_eq!(leaves.len(), s.leaves.len(), "{name}: leaf count");
            assert_eq!(leaves, s.leaves, "{name} ({n_shards} shards): the pipelined leaves are the producer's");
        }
        programs_run += 1;
    }
    assert!(programs_run >= 10);
}

fn first_cell_with_a_boundary(
    s: &Setup,
    plan: &TirPlan,
    program: &TirProgramV1,
    params: &MapParams,
) -> Option<(Vec<Range<usize>>, u32)> {
    let _ = (plan, program, params);
    let cuts = shards(s.n_occ, 2);
    (cuts.len() == 2).then_some((cuts, 0))
}

#[test]
fn a_lie_inside_a_cell_is_found_by_that_cell_at_that_leaf_and_a_consistent_boundary_lie_by_the_upstream_cell() {
    let (mut inside, mut boundary) = (0usize, 0usize);
    for (name, program, params) in programs() {
        if program.schedule.layers.len() < 2 {
            continue;
        }
        let plan = TirPlan::compile(&program).unwrap();
        let s = setup(&name, &program, &params, 7, 2, 2, 5, 4);
        let Some((cuts, _)) = first_cell_with_a_boundary(&s, &plan, &program, &params) else { continue };
        let job_positions = 5 + 4 - 1;
        let cell_of = |shard: usize| TirCellV1 { shard: shard as u16, occ: cuts[shard].clone(), positions: 0..job_positions };
        // ---- a lie INSIDE a cell: one committed leaf of shard 1 that is not a boundary row ----
        let boundary_leaf = |leaf: &kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafV1| match leaf.kind {
            PalwTirLeafKindV1::Commit { occurrence, node, .. } => {
                let o = occurrence as usize;
                let (block, _) = s.space.occurrences()[o];
                o + 1 == cuts[1].start && program.blocks[block as usize].carry_out.contains(&node)
            }
            _ => false,
        };
        let a = 3u32;
        let target = s.space.leaves_of_position(&s.ctx, a).into_iter().find(|l| {
            matches!(l.kind, PalwTirLeafKindV1::Commit { occurrence, .. } if cuts[1].contains(&(occurrence as usize)) && occurrence as usize != cuts[1].end - 1 || false)
                && !boundary_leaf(l)
                && !matches!(l.kind, PalwTirLeafKindV1::Commit { occurrence, .. } if occurrence as usize == cuts[1].start)
        });
        if let Some(target) = target {
            let mut lie = s.leaves.clone();
            lie[target.index as usize].values_le[0] ^= 1;
            let hashes: Vec<Hash64> = lie.iter().map(|l| step_tile_leaf_hash_v1(&s.ctx.context_hash(), &s.class_id, l)).collect();
            let root = step_merkle_root_v1(&hashes).unwrap();
            // The cell that holds the leaf finds it; the leaf is a commit point no later leaf reads, so shard 0 is untouched.
            let v1 = verify(&s, &plan, &program, &params, &cell_of(1), &lie, &root, &s.tokens);
            assert!(
                matches!(v1, TirCellVerdictV1::Faulted { leaf, .. } if leaf == target.index),
                "{name}: {v1:?} (leaf {})",
                target.index
            );
            let v0 = verify(&s, &plan, &program, &params, &cell_of(0), &lie, &root, &s.tokens);
            assert!(matches!(v0, TirCellVerdictV1::Verified { .. }), "{name}: shard 0 is not the lie's: {v0:?}");
            inside += 1;
        }
        // ---- a CONSISTENT lie at the boundary row: shard 1 computes honestly from the carry shard 0 hands it ----
        let lying = pipelined(&s, &plan, &program, &params, &cuts, Some((1, a)));
        if lying == s.leaves {
            continue; // the program's carry at this position is not a value the lie can move (a saturating clamp)
        }
        let hashes: Vec<Hash64> = lying.iter().map(|l| step_tile_leaf_hash_v1(&s.ctx.context_hash(), &s.class_id, l)).collect();
        let root = step_merkle_root_v1(&hashes).unwrap();
        // The downstream cell's inputs and outputs agree: it verifies.
        let down = verify(&s, &plan, &program, &params, &cell_of(1), &lying, &root, &s.tokens);
        assert!(
            matches!(down, TirCellVerdictV1::Verified { .. }),
            "{name}: the downstream cell is fed the lie and verifies: {down:?}"
        );
        // The upstream cell finds it at the boundary row it computed.
        let up = verify(&s, &plan, &program, &params, &cell_of(0), &lying, &root, &s.tokens);
        match up {
            TirCellVerdictV1::Faulted { leaf, position } => {
                assert_eq!(position, a, "{name}");
                assert!(
                    matches!(s.space.leaf_at(&s.ctx, leaf).map(|l| l.kind), Some(PalwTirLeafKindV1::Commit { occurrence, .. }) if occurrence as usize + 1 == cuts[1].start),
                    "{name}: the finding is a boundary row"
                );
            }
            other => panic!("{name}: the upstream cell must find the lie: {other:?}"),
        }
        boundary += 1;
    }
    eprintln!("{inside} lies inside a cell, {boundary} consistent boundary lies");
    assert!(inside >= 3 && boundary >= 8);
}

#[test]
fn a_wrong_generated_id_is_a_token_fault_and_a_missing_input_is_unavailable_and_runs_answer_like_a_capture() {
    let (name, program, params) = programs().into_iter().find(|(n, ..)| n == "corpus dense").unwrap();
    let plan = TirPlan::compile(&program).unwrap();
    let s = setup(&name, &program, &params, 7, 2, 2, 5, 4);
    let cuts = shards(s.n_occ, 2);
    let job_positions = 5 + 4 - 1;
    let last = TirCellV1 { shard: 1, occ: cuts[1].clone(), positions: 0..job_positions };
    // The id the position selects is the greedy selection over the committed logits; another id is the last shard's fault.
    let mut wrong = s.tokens.clone();
    wrong[6] = (wrong[6] + 1) % program.token_bound;
    let v = verify(&s, &plan, &program, &params, &last, &s.leaves, &s.root, &wrong);
    assert!(matches!(v, TirCellVerdictV1::TokenFault { position: 5 }), "{v:?}");
    // ---- over runs: the cell's runs as range openings, each walked to the root ----
    let hashes: Vec<Hash64> = s.leaves.iter().map(|l| step_tile_leaf_hash_v1(&s.ctx.context_hash(), &s.class_id, l)).collect();
    let cell = TirCellV1 { shard: 1, occ: cuts[1].clone(), positions: 0..job_positions };
    let runs = cell_runs_v1(&s.space, &s.ctx, &cell).unwrap();
    assert!(!runs.is_empty());
    let mut inputs = TirRunInputsV1::default();
    for (first, count, pre) in &runs {
        let (first, count) = (*first as usize, *count as usize);
        let opening = PalwStepRangeOpeningV1 {
            first_leaf_index: first as u64,
            leaf_hashes: hashes[first..first + count].to_vec(),
            siblings: step_merkle_range_siblings_v1(&hashes, first, count).unwrap(),
        };
        inputs
            .add_run(hashes.len() as u64, &s.root, &opening, &s.leaves[first..first + *pre], &s.ctx, &s.class_id, u64::MAX)
            .expect("a run that reaches the root");
    }
    let sp = shard_params(&plan, &params, &program, &cell.occ);
    let req = TirCellRequestV1 {
        space: &s.space,
        plan: &plan,
        params: &sp,
        class_id: s.class_id,
        ctx: &s.ctx,
        cell: &cell,
        tokens: &s.tokens,
        inputs: &inputs,
        fused: false,
    };
    let by_runs = CpuKernelBackendV1.verify_cell(&req).unwrap();
    let by_capture = verify(&s, &plan, &program, &params, &cell, &s.leaves, &s.root, &s.tokens);
    assert_eq!(by_runs, by_capture, "a cell over the runs it reads is the cell over the whole capture");
    assert!(matches!(by_runs, TirCellVerdictV1::Verified { .. }));
    assert!(inputs.leaves() < hashes.len(), "the runs are a part of the claim's tree ({} of {})", inputs.leaves(), hashes.len());
    // A run that does not reach the claim's root is refused whole.
    let mut bad = PalwStepRangeOpeningV1 {
        first_leaf_index: 0,
        leaf_hashes: hashes[0..4].to_vec(),
        siblings: step_merkle_range_siblings_v1(&hashes, 0, 4).unwrap(),
    };
    bad.leaf_hashes[1] = Hash64::from_bytes([9; 64]);
    assert!(TirRunInputsV1::default().add_run(hashes.len() as u64, &s.root, &bad, &[], &s.ctx, &s.class_id, u64::MAX).is_err());
    // A cell handed fewer runs than it reads is `Unavailable` at the first leaf it lacks.
    let mut partial = TirRunInputsV1::default();
    let (first, count, pre) = runs[0];
    let (f, c) = (first as usize, count as usize);
    let opening = PalwStepRangeOpeningV1 {
        first_leaf_index: first,
        leaf_hashes: hashes[f..f + c].to_vec(),
        siblings: step_merkle_range_siblings_v1(&hashes, f, c).unwrap(),
    };
    partial.add_run(hashes.len() as u64, &s.root, &opening, &s.leaves[f..f + pre], &s.ctx, &s.class_id, u64::MAX).unwrap();
    let req = TirCellRequestV1 {
        space: &s.space,
        plan: &plan,
        params: &sp,
        class_id: s.class_id,
        ctx: &s.ctx,
        cell: &cell,
        tokens: &s.tokens,
        inputs: &partial,
        fused: false,
    };
    assert!(matches!(CpuKernelBackendV1.verify_cell(&req).unwrap(), TirCellVerdictV1::Unavailable { .. }));
    let _ = NoSink;
}

/// **A seat's duty over a class backend** (what the node runs): each admissible program is written as a container, served by
/// [`TirBackendV1`], and a producer's honest dense capture is verified shard by shard from the plan's own geometry and
/// segment cuts — every shard and segment `Verified`, a moved boundary leaf found by the shard that computed it, a fold
/// (no preimages) refused by name, and the cuts of a duty (`tir_shard_cells_v1`) partition the job exactly.
#[test]
fn a_seat_duty_verifies_its_shards_cells_over_a_backends_capture() {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
    use misaka_palw_tir::interval::analyze_ranges;
    use misaka_palw_tir_exec::node::{TirArtifactV1, TirBackendV1, TirCaptureV1, tir_shard_cells_v1, tir_verify_capture_cells_v1};
    use std::sync::Arc;
    let dir = std::env::temp_dir().join(format!("tir-exec-cell-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut classes, mut cells_run) = (0usize, 0usize);
    for (k, (name, program, params)) in programs().into_iter().enumerate() {
        if analyze_ranges(&program).is_err() || program.params.is_empty() || program.schedule.layers.len() < 2 {
            continue;
        }
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
        let form = if k % 2 == 0 { PalwPromptIdsFormV1::Flat } else { PalwPromptIdsFormV1::MerkleV1 };
        let backend =
            TirBackendV1::new(name.clone(), artifact, root, canonical, form, 1 << 26).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (job, prompt) = backend.job_for_anchor(Hash64::from_bytes([0x3C ^ k as u8; 64])).unwrap();
        let outcome = backend.execute(&job, &prompt).unwrap();
        let positions = backend.space().job_shape(&job).unwrap().positions;
        let layers = program.schedule.layers.len();
        for (s_l, s_p) in [(2u16, 1u16), (2, 2)] {
            if usize::from(s_l) > layers {
                continue;
            }
            // The duty's cells partition the job: every shard's segments tile 0..positions once, whole shards cover the occurrences.
            let mut occ_seen = Vec::new();
            for shard in 0..s_l {
                let full = tir_shard_cells_v1(&backend, positions, shard, s_l, s_p, PalwSegmentMaskV2::full(s_p)).unwrap();
                let mut cut = 0u32;
                for c in &full {
                    assert_eq!(c.positions.start, cut, "{name}: segments are contiguous");
                    cut = c.positions.end;
                    let v = tir_verify_capture_cells_v1(&backend, &outcome.material, std::slice::from_ref(c), &mut CpuKernelBackendV1)
                        .unwrap();
                    assert!(matches!(v, TirCellVerdictV1::Verified { .. }), "{name} S_L {s_l} S_P {s_p} shard {shard}: {v:?}");
                    cells_run += 1;
                }
                if let Some(first) = full.first() {
                    occ_seen.push(first.occ.clone());
                }
                if positions > 0 && !full.is_empty() {
                    assert_eq!(cut, positions, "{name}: the segments end at the job's last position");
                }
            }
            assert_eq!(occ_seen.first().map(|r| r.start), Some(0));
            assert_eq!(occ_seen.last().map(|r| r.end), Some(layers + 2));
            for w in occ_seen.windows(2) {
                assert_eq!(w[0].end, w[1].start, "{name}: shards are contiguous in occurrences");
            }
            // The whole duty of one shard in one call equals its cells one by one.
            let whole = tir_shard_cells_v1(&backend, positions, 0, s_l, s_p, PalwSegmentMaskV2::full(s_p)).unwrap();
            let v = tir_verify_capture_cells_v1(&backend, &outcome.material, &whole, &mut CpuKernelBackendV1).unwrap();
            assert!(matches!(v, TirCellVerdictV1::Verified { .. }), "{name}: {v:?}");
        }
        // A leaf moved after commitment is not the capture's root: refused as no capture rather than a verdict.
        let mut tampered = TirCaptureV1::decode(&outcome.material).unwrap();
        tampered.leaves[1].values_le[0] ^= 1;
        let cells = tir_shard_cells_v1(&backend, positions, 0, 2, 1, PalwSegmentMaskV2::full(1)).unwrap();
        assert!(tir_verify_capture_cells_v1(&backend, &tampered.encode(), &cells, &mut CpuKernelBackendV1).is_err(), "{name}");
        // A fold has no preimages to read.
        let folding = TirBackendV1::new(name.clone(), backend.artifact().clone(), root, backend.canonical().clone(), form, 1 << 26)
            .unwrap()
            .with_dense_capture_bytes(0);
        let fold = folding.execute(&job, &prompt).unwrap();
        let why = tir_verify_capture_cells_v1(&folding, &fold.material, &cells, &mut CpuKernelBackendV1).unwrap_err();
        assert!(why.contains("fold"), "{name}: {why}");
        classes += 1;
    }
    eprintln!("{classes} classes, {cells_run} cells verified through the backend");
    assert!(classes >= 5);
}

/// **The drill's consistent boundary lie, through the producer's own fault door** (`set_tir_drill_boundary_lie_v1`, D-S3): the
/// capture a lying producer commits — its first carry-out of the boundary occurrence changed at one position, everything after
/// computed honestly from it — is found by the UPSTREAM shard's cells and passes the DOWNSTREAM shard's (RFC §2.1).
#[test]
fn the_drills_boundary_lie_is_found_upstream_and_passes_downstream() {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
    use misaka_palw_tir::interval::analyze_ranges;
    use misaka_palw_tir_exec::node::{
        TirArtifactV1, TirBackendV1, TirBoundaryLieV1, set_tir_drill_boundary_lie_v1, tir_shard_cells_v1, tir_verify_capture_cells_v1,
    };
    use std::sync::Arc;
    let dir = std::env::temp_dir().join(format!("tir-exec-lie-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut lied_classes, mut honest_classes) = (0usize, 0usize);
    for (k, (name, program, params)) in programs().into_iter().enumerate() {
        if analyze_ranges(&program).is_err() || program.params.is_empty() || program.schedule.layers.len() != 2 {
            continue;
        }
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
        let artifact = Arc::new(TirArtifactV1::open(&path).unwrap());
        let (root, _) = artifact.inventory_root().unwrap();
        let class = artifact.class().unwrap();
        let canonical =
            kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class.class_id(&root), (4, 3)).unwrap();
        let form = if k % 2 == 0 { PalwPromptIdsFormV1::Flat } else { PalwPromptIdsFormV1::MerkleV1 };
        let backend = TirBackendV1::new(name.clone(), artifact, root, canonical, form, 1 << 26).unwrap();
        let (job, prompt) = backend.job_for_anchor(Hash64::from_bytes([0x3C ^ k as u8; 64])).unwrap();
        let honest = backend.execute(&job, &prompt).unwrap();
        let positions = backend.space().job_shape(&job).unwrap().positions;
        // The first occurrence of shard 1 under a 2-shard plan: pre + layer 0 are shard 0.
        set_tir_drill_boundary_lie_v1(Some(TirBoundaryLieV1 { position: 3, boundary: 2 }));
        let lied = backend.execute_with_injected_fault(&job, &prompt, 0);
        set_tir_drill_boundary_lie_v1(None);
        let Ok(lied) = lied else { continue };
        if lied.execution_root == honest.execution_root {
            continue; // the carry the lie flips is not committed by this program at that position
        }
        let verdict = |shard: u16| {
            let cells = tir_shard_cells_v1(&backend, positions, shard, 2, 1, PalwSegmentMaskV2::full(1)).unwrap();
            tir_verify_capture_cells_v1(&backend, &lied.material, &cells, &mut CpuKernelBackendV1).unwrap()
        };
        match (verdict(0), verdict(1)) {
            (TirCellVerdictV1::Faulted { position, .. }, TirCellVerdictV1::Verified { .. }) => {
                assert_eq!(position, 3, "{name}: the upstream cell finds it at the lied position");
                lied_classes += 1;
            }
            // A lie that moves nothing downstream of the boundary row's own leaf (a clamp) is still found upstream; both shards
            // faulting means the program's later leaves were not recomputed from the lie — a defect of the injector.
            other => panic!("{name}: shard 0 must fault and shard 1 verify, got {other:?}"),
        }
        // The honest capture still verifies after the lie is switched off.
        assert!(matches!(
            tir_verify_capture_cells_v1(
                &backend,
                &honest.material,
                &tir_shard_cells_v1(&backend, positions, 0, 2, 1, PalwSegmentMaskV2::full(1)).unwrap(),
                &mut CpuKernelBackendV1
            )
            .unwrap(),
            TirCellVerdictV1::Verified { .. }
        ));
        honest_classes += 1;
    }
    eprintln!("{lied_classes} classes lied at a boundary and were found upstream");
    assert!(lied_classes >= 2 && honest_classes >= 2);
}

// ---------------------------------------------------------------------------------------------
// The out-of-process device (cellproc): a fake helper over a socket pair
// ---------------------------------------------------------------------------------------------

mod helper {
    use super::*;
    use misaka_palw_tir_exec::node::DeviceKernelBackendV1;
    use misaka_palw_tir_exec::{NodeValue, StepSink, TirCellStepperV1, TirDeviceV1, TirParams, TirPlan};
    use misaka_palw_tir_exec::{CpuDeviceV1, TirProcessDeviceV1, serve_cell_requests_v1};
    use std::ops::Range;
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;

    /// A fake helper: the cell server loop over `device`, on a thread, behind a socket pair.
    pub fn fake_helper(device: impl TirDeviceV1 + 'static, mirror: bool) -> TirProcessDeviceV1 {
        let (a, b) = UnixStream::pair().unwrap();
        std::thread::spawn(move || {
            let (mut r, mut w) = (b.try_clone().unwrap(), b);
            let _ = serve_cell_requests_v1(&mut r, &mut w, &device);
        });
        TirProcessDeviceV1::connect(Box::new(a.try_clone().unwrap()), Box::new(a), mirror).expect("the helper says hello")
    }

    /// A device that LIES: the CPU executor with the first committed value of every step moved by one.
    pub struct LyingDevice;
    struct Lying<'a>(Box<dyn TirCellStepperV1 + 'a>);
    struct Bent<'s>(&'s mut dyn StepSink, bool);
    impl StepSink for Bent<'_> {
        fn node(&mut self, v: &NodeValue<'_>) {
            if v.commit && !self.1 && !v.data.to_i128s().is_empty() {
                self.1 = true;
                let mut lanes = v.data.to_i128s();
                lanes[0] ^= 1;
                let buf = misaka_palw_tir_exec::Buf::from_i128s(v.dtype, &lanes);
                self.0.node(&NodeValue { data: buf.slice(), ..*v });
            } else {
                self.0.node(v);
            }
        }
    }
    impl TirCellStepperV1 for Lying<'_> {
        fn step_cell(&mut self, t: u32, occ: Range<usize>, c: &[Vec<i128>], s: &mut dyn StepSink) -> misaka_palw_tir::TirResult<Vec<Vec<i128>>> {
            self.0.step_cell(t, occ, c, &mut Bent(s, false))
        }
        fn logits_lanes(&self) -> Vec<i32> {
            self.0.logits_lanes()
        }
        fn fixed_lanes(&self, a: u16, b: Option<u16>, c: usize, d: usize, o: &mut Vec<u8>) -> Result<(), String> {
            self.0.fixed_lanes(a, b, c, d, o)
        }
        fn hist_tile_lanes(&self, a: u16, b: Option<u16>, c: usize, d: usize, e: usize, o: &mut Vec<u8>) -> Result<(), String> {
            self.0.hist_tile_lanes(a, b, c, d, e, o)
        }
    }
    impl TirDeviceV1 for LyingDevice {
        fn name(&self) -> String {
            "liar".into()
        }
        fn capacity_bytes(&self) -> Option<u64> {
            None
        }
        fn cell_stepper<'a>(
            &'a self,
            plan: &'a TirPlan,
            params: &'a TirParams<'a>,
            occ: Range<usize>,
        ) -> Result<Box<dyn TirCellStepperV1 + 'a>, String> {
            Ok(Box::new(Lying(CpuDeviceV1.cell_stepper(plan, params, occ)?)))
        }
    }

    pub fn backend(device: TirProcessDeviceV1) -> DeviceKernelBackendV1 {
        DeviceKernelBackendV1(Arc::new(device))
    }
}

#[test]
fn a_helper_process_device_answers_every_cell_as_the_cpu_does_and_a_lying_helper_is_caught_by_the_mirror() {
    use misaka_palw_tir_exec::node::verify_cell_v1;
    let (mut cells, mut refused_late, mut caught) = (0usize, 0usize, 0usize);
    for (name, program, params) in programs() {
        let plan = TirPlan::compile(&program).unwrap();
        let s = setup(&name, &program, &params, 7, 2, 2, 5, 4);
        let honest = helper::backend(helper::fake_helper(misaka_palw_tir_exec::CpuDeviceV1, true));
        let mut honest = honest;
        let liar = helper::fake_helper(helper::LyingDevice, true);
        let mut liar = helper::backend(liar);
        let job_positions = 5 + 4 - 1;
        for occ in shards(s.n_occ, 2) {
            let cell = TirCellV1 { shard: 0, occ: occ.clone(), positions: 0..job_positions };
            let inputs = TirCaptureInputsV1::new(&s.leaves, &s.ctx, &s.class_id, &s.root, u64::MAX).unwrap();
            let sp = shard_params(&plan, &params, &program, &occ);
            let req = TirCellRequestV1 { space: &s.space, plan: &plan, params: &sp, class_id: s.class_id, ctx: &s.ctx, cell: &cell, tokens: &s.tokens, inputs: &inputs, fused: false };
            let cpu = verify_cell_v1(&req);
            // The helper, mirrored: the CPU's verdict byte for byte.
            match honest.verify_cell(&req) {
                Ok(v) => assert_eq!(v, cpu, "{name} {occ:?}: the helper's verdict"),
                Err(_) => refused_late += 1,
            }
            // A lying helper never gets a verdict through: the mirror refuses, the node runs the CPU.
            match liar.verify_cell(&req) {
                Err(_) => caught += 1,
                Ok(v) => assert_eq!(v, cpu, "{name} {occ:?}: a lying helper's verdict equals the CPU's only if it lied about nothing the cell committed"),
            }
            cells += 1;
        }
    }
    eprintln!("{cells} cells; {refused_late} refused by an honest helper; {caught} lies caught by the mirror");
    assert!(cells >= 20 && caught >= 10);
}

#[test]
fn the_cell_wire_round_trips_and_refuses_hostile_frames() {
    use misaka_palw_tir_exec::cellproc::{CellParamBlobV1, CellRequestV1, CellResponseV1, CellValueV1};
    let reqs = vec![
        CellRequestV1::Hello { version: 1 },
        CellRequestV1::Open { program: vec![1, 2, 3], occ: (1, 3), params: vec![CellParamBlobV1 { param: 2, layer: Some(1), bytes: vec![9; 7] }] },
        CellRequestV1::Step { token: 5, occ: (1, 3), carry_in: vec![vec![1, -2, i128::MAX], vec![]] },
        CellRequestV1::Fixed { state: 1, layer: None, first: 0, n: 4 },
        CellRequestV1::Hist { state: 0, layer: Some(0), h_tile: 4, first_lane: 8, row_lanes: 2 },
        CellRequestV1::Close,
    ];
    for r in reqs {
        assert_eq!(CellRequestV1::decode(&r.encode()).unwrap(), r);
    }
    let resp = vec![
        CellResponseV1::Ready { version: 1, name: "wgpu/x".into(), capacity: Some(1 << 33) },
        CellResponseV1::Opened,
        CellResponseV1::Stepped {
            values: vec![CellValueV1 { slot: 3, block: 1, layer: Some(0), node: 7, dtype: 2, shape: vec![2, 4], lanes: vec![1, -1, 0, 5, 6, 7, 8, 9] }],
            carry: vec![vec![4, 5]],
            logits: vec![-3, 9],
        },
        CellResponseV1::Lanes(vec![1, 2, 3, 4]),
        CellResponseV1::Closed,
        CellResponseV1::Refused("no".into()),
    ];
    for r in resp {
        assert_eq!(CellResponseV1::decode(&r.encode()).unwrap(), r);
    }
    // Hostile: a length past the frame, trailing bytes, an unknown tag, a truncated frame.
    assert!(CellRequestV1::decode(&[1, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f]).is_err());
    let mut hello = CellRequestV1::Hello { version: 1 }.encode();
    hello.push(0);
    assert!(CellRequestV1::decode(&hello).is_err());
    assert!(CellRequestV1::decode(&[99]).is_err());
    assert!(CellResponseV1::decode(&[2, 1]).is_err());
    // A wrong version is refused by the server, by name.
    let (a, b) = std::os::unix::net::UnixStream::pair().unwrap();
    std::thread::spawn(move || {
        let (mut r, mut w) = (b.try_clone().unwrap(), b);
        let _ = misaka_palw_tir_exec::serve_cell_requests_v1(&mut r, &mut w, &misaka_palw_tir_exec::CpuDeviceV1);
    });
    let err = misaka_palw_tir_exec::TirProcessDeviceV1::connect_version_for_test(Box::new(a.try_clone().unwrap()), Box::new(a), 2);
    assert!(err.is_err());
}

/// **A capture from answered runs** (D-S4): the leaves a seat was answered, in chunks and out of order, make the capture the producer
/// held — leaves, logits rows and generated ids — and a false leaf, a gap or another class's binding is refused by name.
#[test]
fn a_capture_is_reassembled_from_answered_runs_and_refuses_a_false_leaf_or_a_gap() {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use misaka_palw_tir::interval::analyze_ranges;
    use misaka_palw_tir_exec::node::{TirArtifactV1, TirBackendV1, TirCaptureV1};
    use std::sync::Arc;
    let dir = std::env::temp_dir().join(format!("tir-exec-runs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut done = 0;
    for (k, (name, program, params)) in programs().into_iter().enumerate() {
        if analyze_ranges(&program).is_err() || program.params.is_empty() {
            continue;
        }
        let lay = layout(&program, 5, 2, 2, 64);
        let path = dir.join(format!("{}.palwtir", name.replace(' ', "-")));
        let mut tensor = |j: u16, l: Option<u16>| -> Result<Vec<u8>, String> {
            params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
        };
        misaka_palw_tir_artifact::write_container_v1(&path, &program, borsh::to_vec(&lay).unwrap(), [2; 64], name.clone(), &mut tensor).unwrap();
        let artifact = Arc::new(TirArtifactV1::open(&path).unwrap());
        let (root, _) = artifact.inventory_root().unwrap();
        let class = artifact.class().unwrap();
        let canonical = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class.class_id(&root), (4, 3)).unwrap();
        let backend = TirBackendV1::new(name.clone(), artifact, root, canonical, PalwPromptIdsFormV1::Flat, 1 << 26).unwrap();
        let (job, prompt) = backend.job_for_anchor(Hash64::from_bytes([0x3C ^ k as u8; 64])).unwrap();
        let honest = backend.execute(&job, &prompt).unwrap();
        let cap = TirCaptureV1::decode(&honest.material).unwrap();
        let mut binding = cap.binding.clone();
        binding.class.program = Vec::new(); // as an answer carries it
        let prompt32: Vec<u32> = prompt.iter().map(|x| *x as u32).collect();
        // Chunks of 17 leaves, last first.
        let mut runs: Vec<(u64, Vec<_>)> = cap.leaves.chunks(17).enumerate().map(|(i, c)| ((i * 17) as u64, c.to_vec())).collect();
        runs.reverse();
        let rebuilt = backend.capture_from_runs_v1(binding.clone(), prompt32.clone(), &runs).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(rebuilt.leaves, cap.leaves, "{name}");
        assert_eq!(rebuilt.logits_rows, cap.logits_rows, "{name}: the rows read back from the logits leaves");
        assert_eq!(rebuilt.generated, cap.generated, "{name}: the greedy ids");
        assert_eq!(rebuilt.binding.step_merkle_root, cap.binding.step_merkle_root);
        // A false leaf: refused (the root does not hold).
        let mut bad = runs.clone();
        bad[0].1[0].values_le[0] ^= 1;
        assert!(backend.capture_from_runs_v1(binding.clone(), prompt32.clone(), &bad).unwrap_err().contains("step root"), "{name}");
        // A gap.
        let mut gap = runs.clone();
        gap.remove(1);
        assert!(backend.capture_from_runs_v1(binding.clone(), prompt32.clone(), &gap).unwrap_err().contains("gap"), "{name}");
        done += 1;
    }
    assert!(done >= 5);
}

/// **A shard-only holder's fetch** (D-S6): the rows of ONE shard, each proven against the registered root, make exactly the params
/// the shard's cells read — the cells verify over them as over the whole class, a holder that lies about a row or opens another leaf is
/// refused by name, and the seat held a fraction of the class.
#[test]
fn a_shard_only_holder_fetches_its_rows_proves_each_and_verifies_its_cells() {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
    use misaka_palw_tir::interval::analyze_ranges;
    use misaka_palw_tir_exec::node::{
        TirArtifactV1, TirBackendV1, TirCaptureV1, TirFileMirrorV1, TirRowFetcherV1, fetch_shard_params_v1, tir_shard_cells_over_v1,
        tir_verify_capture_cells_over_v1,
    };
    use std::sync::Arc;
    struct Liar<'a>(&'a TirFileMirrorV1, u8);
    impl TirRowFetcherV1 for Liar<'_> {
        fn open_rows(&self, leaves: &[u32]) -> Result<Vec<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1>, String> {
            let mut o = self.0.open_rows(leaves)?;
            match self.1 {
                0 => o[0].operand.bytes[0] ^= 1,                    // a false row
                1 => o[0].leaf_index = o[0].leaf_index.wrapping_add(1), // another leaf
                _ => o.truncate(o.len() - 1),                        // fewer rows
            }
            Ok(o)
        }
    }
    let dir = std::env::temp_dir().join(format!("tir-exec-rows-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut done = 0;
    for (k, (name, program, params)) in programs().into_iter().enumerate() {
        if analyze_ranges(&program).is_err() || program.params.is_empty() || program.schedule.layers.len() < 2 {
            continue;
        }
        let lay = layout(&program, 5, 2, 2, 64);
        let path = dir.join(format!("{}.palwtir", name.replace(' ', "-")));
        let mut tensor = |j: u16, l: Option<u16>| -> Result<Vec<u8>, String> {
            params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
        };
        misaka_palw_tir_artifact::write_container_v1(&path, &program, borsh::to_vec(&lay).unwrap(), [2; 64], name.clone(), &mut tensor).unwrap();
        let artifact = Arc::new(TirArtifactV1::open(&path).unwrap());
        let (root, leaf_count) = artifact.inventory_root().unwrap();
        let class = artifact.class().unwrap();
        let class_id = class.class_id(&root);
        let canonical = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class_id, (4, 3)).unwrap();
        let backend = TirBackendV1::new(name.clone(), artifact, root, canonical, PalwPromptIdsFormV1::Flat, 1 << 26).unwrap();
        let (job, prompt) = backend.job_for_anchor(Hash64::from_bytes([0x3C ^ k as u8; 64])).unwrap();
        let outcome = backend.execute(&job, &prompt).unwrap();
        let capture = TirCaptureV1::decode(&outcome.material).unwrap();
        let positions = backend.space().job_shape(&job).unwrap().positions;
        let mirror = TirFileMirrorV1(path.clone());
        let mut fetched_fraction = Vec::new();
        for shard in 0..2u16 {
            let holding = fetch_shard_params_v1(&capture.binding.class, class_id, root, 2, shard, &mirror).unwrap_or_else(|e| panic!("{name} shard {shard}: {e}"));
            fetched_fraction.push((holding.leaves, u64::from(leaf_count)));
            assert!(holding.leaves < u64::from(leaf_count) || program.params.iter().all(|p| !p.per_layer), "{name}: a shard is a part of the class");
            let cells = tir_shard_cells_over_v1(&holding.space, positions, shard, 2, 1, PalwSegmentMaskV2::full(1)).unwrap();
            let v = tir_verify_capture_cells_over_v1(&holding.space, &holding.plan, &holding.params, class_id, false, &capture, &cells, &mut CpuKernelBackendV1).unwrap();
            assert!(matches!(v, TirCellVerdictV1::Verified { .. }), "{name} shard {shard}: {v:?}");
            // The shard's rows are exactly what the shard's cells read: the OTHER shard's cells do not run over them.
            let other = tir_shard_cells_over_v1(&holding.space, positions, 1 - shard, 2, 1, PalwSegmentMaskV2::full(1)).unwrap();
            let refused = tir_verify_capture_cells_over_v1(&holding.space, &holding.plan, &holding.params, class_id, false, &capture, &other, &mut CpuKernelBackendV1).unwrap();
            let program_has_layer_params = program.params.iter().any(|p| p.per_layer);
            if program_has_layer_params {
                assert!(matches!(refused, TirCellVerdictV1::Refused(_)), "{name}: another shard's cell needs rows this seat did not fetch: {refused:?}");
            }
        }
        // A lying holder: a false row, another leaf, fewer rows.
        for mode in 0..3u8 {
            let e = fetch_shard_params_v1(&capture.binding.class, class_id, root, 2, 0, &Liar(&mirror, mode)).err().expect("a lying holder is refused");
            eprintln!("{name}: mode {mode}: {e}");
        }
        // A wrong registered root: every row fails its path.
        assert!(fetch_shard_params_v1(&capture.binding.class, class_id, Hash64::from_bytes([1; 64]), 2, 0, &mirror).is_err());
        done += 1;
    }
    assert!(done >= 4);
}
