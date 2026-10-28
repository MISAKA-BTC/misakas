//! **RFC-0002 Phase F, step F4: the step space of an IR class, in closed form, equals the walk.**
//!
//! Over every program of `consensus-vectors/tir-v1/programs`, several layouts (tile lengths,
//! checkpoint intervals, history tiles, state tiles) and a sweep of jobs, an independent position-by-
//! position enumeration written here from the design's rules is compared, leaf by leaf, with the
//! step space's closed-form count, `leaf_at`, `leaf_index` and `leaves_of_position`. Then a real run
//! of each program is committed through the fail-closed builder and its binding verifies — and
//! stops verifying when any part of it moves.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step::PalwStepCoordinateV1;
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1,
};
use kaspa_consensus_core::palw_tir_step_v1::{
    PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirLeafKindV1, PalwTirStepBindingV1, PalwTirStepLegBuilderV1, PalwTirStepSpaceV1,
    palw_tir_execution_root_v1, palw_tir_lane_values_v1, palw_tir_lanes_le_v1, verify_tir_binding_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use misaka_palw_tir::program::{Ref, StateKind};
use misaka_palw_tir::{DType, Dim, Interpreter, MapParams, RunState, Tensor, TirProgramV1};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn programs() -> Vec<(String, Vec<u8>, MapParams, Vec<u32>)> {
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
            let bytes = unhex(v["program_borsh_hex"].as_str().unwrap());
            let program = TirProgramV1::decode_canonical(&bytes).expect("canonical");
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
            (v["name"].as_str().unwrap().to_string(), bytes, params, tokens)
        })
        .chain(std::iter::once(h_program()))
        .collect()
}

/// A program whose committed nodes CARRY `H` — a windowed history summed per position — so the
/// rising and the flat halves of the closed form (`min(a + 1, W)` below and past the window) are both
/// exercised, with the per-layer history of three layer occurrences.
fn h_program() -> (String, Vec<u8>, MapParams, Vec<u32>) {
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::{Ref as R, TensorType};
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let emb = pb.param("embed", DType::I8, &[16, 4], false);
    let hist = pb.hist_state("rows", DType::I8, &[4], 5, true);
    let carry = TensorType::fixed(DType::I8, &[4]);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(emb, R::Input(0), 0, 0);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", vec![carry.clone()]);
        let window = b.hist_append(hist, R::CarryIn(0));
        let wide = b.cast(window, DType::I32);
        let wide = b.commit(wide);
        let sum = b.reduce_sum(wide, 0, DType::I32);
        let sum = b.reshape_fixed(sum, &[4]);
        let y = b.clamp(sum, -128, 127, DType::I8);
        b.finish(&[y])
    };
    let (post, logits) = {
        let mut b = pb.block("post", vec![carry]);
        let l = b.cast(R::CarryIn(0), DType::I32);
        let l = b.commit(l);
        let R::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let program = pb.finish(pre, vec![layer, layer, layer], post, logits);
    let bytes = program.encode();
    TirProgramV1::decode_canonical(&bytes).expect("the H program is canonical");
    let mut params = MapParams::default();
    let data: Vec<i128> = (0..64).map(|i| ((i * 37 + 11) % 256) as i128 - 128).collect();
    params.tensors.insert((0, None), Tensor::new(DType::I8, vec![16, 4], data).unwrap());
    ("h-window-sum".to_string(), bytes, params, vec![1, 5, 9, 3, 14, 0, 7])
}

fn committed_count(p: &TirProgramV1) -> usize {
    p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum()
}

/// A layout whose tile lengths vary by node (`4 + (k·seed) % 6`), so ragged and multi-tile nodes both occur.
fn layout(p: &TirProgramV1, seed: u32, c: u32, h: u32, max_context: u32) -> PalwTirLayoutV1 {
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context,
        checkpoint_interval: c,
        h_tile: h,
        commit_tiles: (0..committed_count(p) as u32).map(|k| 4 + (k.wrapping_mul(seed) % 6)).collect(),
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
        trace_scheme_id: kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1(),
        cu_ruleset_id: z,
        tokenizer_id: z,
        prompt_token_ids_hash: z,
        declared_prefill_tokens: prefill,
        exact_decode_tokens: decode,
        max_context_tokens: 1024,
    }
}

/// The design's enumeration, written out position by position: `(call, slot, position, tile, values)`.
fn walk(p: &TirProgramV1, l: &PalwTirLayoutV1, prefill: u32, decode: u32) -> Vec<(u32, u32, u32, u32, u32)> {
    let info = misaka_palw_tir::validate::validate(p).unwrap();
    let occ = p.occurrences();
    let bases = p.occurrence_slot_bases();
    let slots: u32 = occ.iter().map(|(b, _)| p.blocks[*b as usize].nodes.len() as u32).sum();
    let mut tiles = BTreeMap::new();
    let mut k = 0;
    for (bi, b) in p.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if n.commit {
                tiles.insert((bi, ni), l.commit_tiles[k]);
                k += 1;
            }
        }
    }
    let uses = |b: u8, j: u16| {
        p.blocks[b as usize].nodes.iter().any(|n| {
            n.inputs.contains(&Ref::State(j))
                || matches!(n.prim, misaka_palw_tir::Prim::StateWrite { state } | misaka_palw_tir::Prim::HistAppend { state } if state == j)
        })
    };
    let mut fixed = Vec::new();
    let mut hist = Vec::new();
    for (j, s) in p.states.iter().enumerate() {
        let layers: Vec<Option<u16>> = if s.per_layer {
            p.schedule.layers.iter().enumerate().filter(|(_, b)| uses(**b, j as u16)).map(|(l, _)| Some(l as u16)).collect()
        } else {
            vec![None]
        };
        let elements: u32 = s.shape.iter().product();
        for _ in layers {
            match s.kind {
                StateKind::Fixed { .. } => fixed.push((elements, l.state_tiles[j])),
                StateKind::Hist { .. } => hist.push((elements, l.state_tiles[j])),
            }
        }
    }
    let positions = prefill + decode - 1;
    let mut out = Vec::new();
    for a in 0..positions {
        let (call, position) = if a < prefill { (0, a) } else { (a - prefill + 1, 0) };
        for (i, (b, _)) in occ.iter().enumerate() {
            if i == occ.len() - 1 && a + 1 < prefill {
                continue;
            }
            let h = info.blocks[*b as usize].window.map(|w| (a + 1).min(w)).unwrap_or(1);
            for (ni, n) in p.blocks[*b as usize].nodes.iter().enumerate() {
                if !n.commit {
                    continue;
                }
                let e: u32 = n
                    .out
                    .shape
                    .iter()
                    .map(|d| {
                        if *d == Dim::H {
                            h
                        } else if let Dim::Fixed(x) = d {
                            *x
                        } else {
                            1
                        }
                    })
                    .product();
                let t = tiles[&(*b as usize, ni)];
                for tile in 0..e.div_ceil(t) {
                    out.push((call, bases[i] + ni as u32, position, tile, (e - tile * t).min(t)));
                }
            }
        }
        if (a + 1) % l.checkpoint_interval == 0 {
            for (k, (e, t)) in fixed.iter().enumerate() {
                for tile in 0..e.div_ceil(*t) {
                    out.push((call, slots + k as u32, position, tile, (e - tile * t).min(*t)));
                }
            }
        }
        if (a + 1) % l.h_tile == 0 {
            for (k, (e, t)) in hist.iter().enumerate() {
                for tile in 0..e.div_ceil(*t) {
                    out.push((call, slots + (fixed.len() + k) as u32, position, tile, (e - tile * t).min(*t) * l.h_tile));
                }
            }
        }
    }
    out
}

#[test]
fn the_closed_form_is_the_walk_on_every_program_layout_and_job() {
    let mut compared = 0usize;
    for (name, bytes, _, _) in programs() {
        let program = TirProgramV1::decode_canonical(&bytes).unwrap();
        for (seed, c, h) in [(1u32, 1u32, 1u32), (7, 2, 2), (5, 3, 4), (3, 4, 1), (11, 5, 8)] {
            let l = layout(&program, seed, c, h, 16);
            let class = PalwTirClassV1 {
                version: PALW_TIR_CLASS_VERSION_V1,
                program: bytes.clone(),
                layout: l.clone(),
                tokenizer_id: Hash64::from_bytes([1; 64]),
            };
            let space = PalwTirStepSpaceV1::new(&class).unwrap_or_else(|e| panic!("{name}: {e}"));
            for (prefill, decode) in [(1u32, 1u32), (1, 4), (3, 1), (5, 3), (2, 7), (9, 8)] {
                let ctx = job(prefill, decode, Hash64::from_bytes([2; 64]));
                let expect = walk(&program, &l, prefill, decode);
                let count = space.leaf_count_capped(&ctx, u64::MAX).unwrap();
                assert_eq!(count as usize, expect.len(), "{name} seed {seed} C {c} h {h} job ({prefill}, {decode}): the count");
                let mut by_position: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
                for a in 0..prefill + decode - 1 {
                    by_position.extend(
                        space
                            .leaves_of_position(&ctx, a)
                            .iter()
                            .map(|x| (x.coord.call_index, x.coord.node_slot, x.coord.position, x.coord.tile_index, x.value_count)),
                    );
                }
                assert_eq!(by_position, expect, "{name}: leaves_of_position is the walk");
                for (i, want) in expect.iter().enumerate() {
                    let leaf = space.leaf_at(&ctx, i as u64).unwrap_or_else(|| panic!("{name}: leaf {i}"));
                    let got =
                        (leaf.coord.call_index, leaf.coord.node_slot, leaf.coord.position, leaf.coord.tile_index, leaf.value_count);
                    assert_eq!(&got, want, "{name}: leaf {i}");
                    assert_eq!(space.leaf_index(&ctx, &leaf.coord), Some(i as u64), "{name}: leaf_index inverts leaf_at at {i}");
                    compared += 1;
                }
                assert!(space.leaf_at(&ctx, count).is_none(), "{name}: nothing past the last leaf");
                let bogus = [
                    PalwStepCoordinateV1 { call_index: 0, node_slot: u32::MAX, position: 0, tile_index: 0 },
                    PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: prefill, tile_index: 0 },
                    PalwStepCoordinateV1 { call_index: decode, node_slot: 0, position: 0, tile_index: 0 },
                    PalwStepCoordinateV1 { call_index: 1, node_slot: 0, position: 1, tile_index: 0 },
                ];
                for coord in bogus {
                    assert_eq!(space.leaf_index(&ctx, &coord), None, "{name}: {coord:?} names no leaf");
                }
            }
        }
    }
    assert!(compared > 10_000, "{compared} leaves compared");
    let with_h = programs()
        .iter()
        .map(|(_, bytes, _, _)| TirProgramV1::decode_canonical(bytes).unwrap())
        .filter(|p| p.blocks.iter().any(|b| b.nodes.iter().any(|n| n.commit && n.out.has_h())))
        .count();
    assert!(with_h >= 1, "some committed node carries H, so the floor-sum half of the closed form is exercised");
}

#[test]
fn a_layout_that_is_not_the_programs_is_refused() {
    let (_, bytes, _, _) = programs().into_iter().find(|p| p.0.contains("fixed")).expect("the fixed-state program");
    let program = TirProgramV1::decode_canonical(&bytes).unwrap();
    let ok = layout(&program, 1, 2, 2, 16);
    let class = |l: PalwTirLayoutV1| PalwTirClassV1 {
        version: 1,
        program: bytes.clone(),
        layout: l,
        tokenizer_id: Hash64::from_bytes([1; 64]),
    };
    assert!(PalwTirStepSpaceV1::new(&class(ok.clone())).is_ok());
    let refused = |edit: &dyn Fn(&mut PalwTirLayoutV1)| {
        let mut l = ok.clone();
        edit(&mut l);
        PalwTirStepSpaceV1::new(&class(l)).is_err()
    };
    assert!(refused(&|l| l.commit_tiles.push(4)), "one tile per committed node");
    assert!(refused(&|l| {
        l.commit_tiles.pop();
    }));
    assert!(refused(&|l| l.commit_tiles[0] = 3), "a tile below the step leg's minimum");
    assert!(refused(&|l| l.commit_tiles[0] = 65_537), "…or past its maximum");
    assert!(refused(&|l| l.h_tile = 3), "h_tile is a power of two");
    assert!(refused(&|l| l.h_tile = 8192), "…of at most 4,096");
    assert!(refused(&|l| l.checkpoint_interval = 0));
    assert!(refused(&|l| l.max_context = 0));
    assert!(refused(&|l| l.max_context = program.history_bound + 1), "at most the program's history bound");
    assert!(refused(&|l| l.state_tiles.push(4)), "one state tile per state");
    assert!(refused(&|l| l.version = PALW_TIR_LAYOUT_VERSION_V1 + 1));
    let space = PalwTirStepSpaceV1::new(&class(ok)).unwrap();
    assert!(space.job_shape(&job(0, 1, Hash64::from_bytes([2; 64]))).is_err(), "no prompt position");
    assert!(space.job_shape(&job(1, 0, Hash64::from_bytes([2; 64]))).is_err(), "no token");
    assert!(space.job_shape(&job(10, 8, Hash64::from_bytes([2; 64]))).is_err(), "17 positions past max_context 16");
}

#[test]
fn the_lanes_are_four_bytes_and_idx_is_unsigned() {
    assert_eq!(palw_tir_lanes_le_v1(DType::I8, &[-1, 127]).unwrap(), [255, 255, 255, 255, 127, 0, 0, 0]);
    assert_eq!(palw_tir_lanes_le_v1(DType::Idx, &[4_294_967_295]).unwrap(), [255, 255, 255, 255]);
    assert!(palw_tir_lanes_le_v1(DType::I8, &[128]).is_err(), "a value outside the dtype is unbuildable");
    assert!(palw_tir_lanes_le_v1(DType::I64, &[0]).is_err(), "i64 is never committed");
    assert_eq!(palw_tir_lane_values_v1(DType::I16, &[255, 255, 255, 255]).unwrap(), vec![-1]);
    assert_eq!(palw_tir_lane_values_v1(DType::Idx, &[255, 255, 255, 255]).unwrap(), vec![4_294_967_295]);
    assert_eq!(palw_tir_lane_values_v1(DType::I8, &[0, 1, 0, 0]).unwrap(), vec![256], "read without judging: TIR-33 judges");
    assert!(palw_tir_lane_values_v1(DType::I32, &[0, 0, 0]).is_err());
}

/// The honest values of every leaf of a run, from the reference evaluator: commit points from the
/// step's commit records, checkpoints from the run state after the position, history tiles from the
/// rows each `HistAppend` appended (its input: a committed node or a carry-in, NF-20).
fn honest_values(space: &PalwTirStepSpaceV1, ctx: &PalwJobContextV2, params: &MapParams, tokens: &[u32]) -> Vec<Vec<i128>> {
    let p = &space.program;
    let interp = Interpreter::new(p).unwrap();
    let mut state = RunState::default();
    let positions = ctx.declared_prefill_tokens + ctx.exact_decode_tokens - 1;
    let mut rows: BTreeMap<(u16, Option<u16>), Vec<Vec<i128>>> = BTreeMap::new();
    let mut out = Vec::new();
    for a in 0..positions {
        let token = tokens[a as usize % tokens.len()];
        let step = interp.step(params, &mut state, token).unwrap();
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
                    rows.entry((j, if p.states[j as usize].per_layer { *layer } else { None })).or_default().push(row);
                }
            }
        }
        for leaf in space.leaves_of_position(ctx, a) {
            let n = leaf.value_count as usize;
            out.push(match leaf.kind {
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
                    let all = &rows[&(j, layer)];
                    (first_position..=a)
                        .flat_map(|t| all[t as usize][first_lane as usize..first_lane as usize + row_lanes as usize].to_vec())
                        .collect()
                }
            });
        }
    }
    out
}

#[test]
fn an_honest_run_commits_through_the_builder_and_its_binding_verifies() {
    for (name, bytes, params, tokens) in programs() {
        let program = TirProgramV1::decode_canonical(&bytes).unwrap();
        let class = PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: bytes.clone(),
            layout: layout(&program, 7, 2, 2, 16),
            tokenizer_id: Hash64::from_bytes([1; 64]),
        };
        let artifact_root = Hash64::from_bytes([0xA7; 64]);
        let class_id = class.class_id(&artifact_root);
        let space = PalwTirStepSpaceV1::new(&class).unwrap();
        let ctx = job(3, 3, class_id);
        let values = honest_values(&space, &ctx, &params, &tokens);
        let mut builder = PalwTirStepLegBuilderV1::new(&space, &ctx, class_id, u64::MAX).unwrap();
        for v in &values {
            builder.push(v).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        assert!(builder.next_leaf().is_none());
        let (count, root) = builder.finish().unwrap();
        let logits_root = Hash64::from_bytes([0x10; 64]);
        let binding = PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: ctx.clone(),
            class: class.clone(),
            artifact_root,
            full_logits_trace_root: logits_root,
            step_leaf_count: count,
            step_merkle_root: root,
            committed_execution_root: palw_tir_execution_root_v1(&ctx.context_hash(), &logits_root, &class_id, count, &root),
        };
        verify_tir_binding_v1(&binding, u64::MAX).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(verify_tir_binding_v1(&binding, count - 1).is_err(), "{name}: past the ladder");
        let mut moved = binding.clone();
        moved.step_leaf_count += 1;
        assert!(verify_tir_binding_v1(&moved, u64::MAX).is_err(), "{name}: a count that is not canonical");
        let mut moved = binding.clone();
        moved.artifact_root = Hash64::from_bytes([0xA8; 64]);
        assert!(verify_tir_binding_v1(&moved, u64::MAX).is_err(), "{name}: other weights are another class");
        let mut moved = binding.clone();
        moved.class.layout.checkpoint_interval = 3;
        assert!(verify_tir_binding_v1(&moved, u64::MAX).is_err(), "{name}: another layout is another class");
        let mut moved = binding.clone();
        moved.step_merkle_root = Hash64::from_bytes([0x11; 64]);
        assert!(verify_tir_binding_v1(&moved, u64::MAX).is_err(), "{name}: the root is committed");

        // Fail-closed: a value outside its leaf's dtype, and a short leaf, are unbuildable.
        let mut builder = PalwTirStepLegBuilderV1::new(&space, &ctx, class_id, u64::MAX).unwrap();
        let first = builder.next_leaf().unwrap();
        let mut wide = values[0].clone();
        wide[0] = first.dtype.max_value() + 1;
        assert!(builder.push(&wide).is_err(), "{name}");
        assert!(builder.push(&values[0][1..]).is_err(), "{name}");
        assert!(builder.finish().is_err(), "{name}: an incomplete leg has no root");
    }
}
