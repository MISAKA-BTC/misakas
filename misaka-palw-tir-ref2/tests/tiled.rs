//! **RFC-0013 §5: the tiled evaluation equals the whole-tensor evaluation, and its residency is bounded by a tile.**
//!
//! * random programs (this crate's generator): `step` and `run` through a [`TiledParams`] with tiles of 1, 2, 3, 5 and 10⁶ elements give the
//!   same `Result` — logits, commits, the next run state, and every refusal — as the whole-`Params` evaluator;
//! * a Qwen-shaped decoder (this crate's own builder, scaled down): the same, over several positions, with **strict** tiling (a param larger
//!   than a tile may only be read by a primitive with a tiled form — the bound as a guarantee), the multiply-accumulate count equal to the
//!   analytic one the whole path makes, every tiled param element read exactly once per use, and the peak residency within
//!   `max(tile, widest row)`;
//! * a source that fails mid-run: the refusal reaches the caller, class and reason, never a silent zero.
//!
//! Run: `cargo test -p misaka-palw-tir-ref2 --test tiled`

mod common;

use common::progen::{GenCfg, R, gen_program};
use misaka_palw_tir_ref2 as ref2;
use rand::{Rng, SeedableRng};
use ref2::eval::{Params, initial_state, run, step};
use ref2::tiled::{MapRowSource, RowSource, TileReport, TiledParams, tile_elems_bound};
use ref2::{Class, DType, Prim, Program, Ref, Res, Tensor};

fn rest_elems(shape: &[u32]) -> u64 {
    shape.iter().skip(1).map(|&d| d as u64).product::<u64>().max(1)
}

/// The widest row of any declared param: the floor under every tile.
fn widest_row(p: &Program) -> u64 {
    p.params.iter().map(|d| rest_elems(&d.shape)).max().unwrap_or(1)
}

#[test]
fn tiled_evaluation_equals_whole_evaluation_on_random_programs() {
    let mut rng = R::seed_from_u64(0x0013_2026);
    let (mut compared, mut refused, mut lazy_nodes) = (0usize, 0usize, 0u64);
    for case in 0..250 {
        let g = gen_program(&mut rng, GenCfg::default());
        for tile in [1u64, 2, 3, 5, 1_000_000] {
            let source = MapRowSource(&g.params);
            let tiled = TiledParams::new(&source, tile);
            let (mut whole_state, mut tiled_state) = (initial_state(&g.prog), initial_state(&g.prog));
            for position in 0..3 {
                let token = rng.gen_range(0..g.prog.token_bound) as u64;
                let expected = step(&g.prog, &g.params, &whole_state, token);
                let got = step(&g.prog, &tiled, &tiled_state, token);
                assert_eq!(got, expected, "case {case} tile {tile} position {position}");
                compared += 1;
                match expected {
                    Ok((_, next)) => {
                        whole_state = next.clone();
                        tiled_state = next;
                    }
                    Err(_) => {
                        refused += 1;
                        break;
                    }
                }
            }
            let r = tiled.report();
            lazy_nodes += r.tiles;
            // The residency bound: a tile is at most max(tile, the widest row), and a whole load at most one tile — or it is a param no
            // tiled primitive reads, which the meter names.
            assert!(r.peak_tile_elems <= tile.max(widest_row(&g.prog)), "case {case} tile {tile}: {r:?}");
        }
    }
    eprintln!("random programs: {compared} steps compared, {refused} refused, {lazy_nodes} tiles read");
    assert!(compared > 1_000 && refused > 3, "both values and refusals were compared: {compared} steps, {refused} refused");
    assert!(lazy_nodes > 100, "the tiled path actually ran ({lazy_nodes} tiles)");
}

#[test]
fn a_whole_run_through_tiles_equals_the_run_it_replaces() {
    let mut rng = R::seed_from_u64(0x0013_2027);
    let mut ok = 0;
    for case in 0..120 {
        let g = gen_program(&mut rng, GenCfg { range_safe: true, ..GenCfg::default() });
        let tokens: Vec<u64> = (0..4).map(|_| rng.gen_range(0..g.prog.token_bound) as u64).collect();
        let expected = run(&g.prog, &g.params, &tokens);
        for tile in [1u64, 4] {
            let source = MapRowSource(&g.params);
            let tiled = TiledParams::new(&source, tile);
            assert_eq!(run(&g.prog, &tiled, &tokens), expected, "case {case} tile {tile}");
        }
        if expected.is_ok() {
            ok += 1;
        }
    }
    eprintln!("whole runs: {ok} of 120 succeeded end to end");
    assert!(ok > 3, "{ok} runs succeeded end to end");
}

// ------------------------------------------------------------------------------------------------------------------
// A Qwen-shaped decoder, scaled down.
// ------------------------------------------------------------------------------------------------------------------

fn small_qwen() -> Program {
    let s = common::qwen::Shape {
        layers: 3,
        d: 12,
        q_heads: 2,
        kv_heads: 1,
        head: 6,
        ffn: 20,
        vocab: 37,
        window: 64,
        commit_attention: true,
    };
    common::qwen::build(&s)
}

/// Small deterministic params: every dtype's values stay far from its limits, so the run succeeds and the comparison is of values.
fn small_params(rng: &mut R, p: &Program) -> Params {
    let mut params = Params::new();
    let mut occ = vec![(p.schedule.pre, None)];
    for (l, &b) in p.schedule.layers.iter().enumerate() {
        occ.push((b, Some(l as u32)));
    }
    occ.push((p.schedule.post, None));
    for (b, layer) in occ {
        for n in &p.blocks[b as usize].nodes {
            for r in &n.inputs {
                if let Ref::Param(j) = *r {
                    let d = &p.params[j as usize];
                    let key = (j, if d.per_layer { layer } else { None });
                    if params.contains_key(&key) {
                        continue;
                    }
                    let shape: Vec<u64> = d.shape.iter().map(|&x| x as u64).collect();
                    let (lo, hi) = match d.dtype {
                        DType::I8 => (-3, 3),
                        DType::I16 => (-300, 300),
                        _ => (-1000, 1000),
                    };
                    let n: u64 = shape.iter().product();
                    let data = (0..n).map(|_| rng.gen_range(lo..=hi)).collect();
                    params.insert(key, Tensor { dtype: d.dtype, shape, data });
                }
            }
        }
    }
    params
}

/// The multiply-accumulates the whole evaluator makes in one step for MatMuls with a param operand (`B = [K, N]`, an `[1, K]` activation):
/// `N · K` per node, per occurrence.
fn analytic_param_macs(p: &Program) -> u128 {
    let mut occ = vec![p.schedule.pre];
    occ.extend(p.schedule.layers.iter().copied());
    occ.push(p.schedule.post);
    let mut total = 0u128;
    for b in occ {
        for n in &p.blocks[b as usize].nodes {
            if let (Prim::MatMul, [_, Ref::Param(j)]) = (&n.prim, n.inputs.as_slice()) {
                let d = &p.params[*j as usize];
                assert_eq!(d.shape.len(), 2, "this decoder's weights are [K, N]");
                total += d.shape[0] as u128 * d.shape[1] as u128;
            }
        }
    }
    total
}

#[test]
fn a_decoder_runs_through_tiles_strictly_with_the_same_logits_the_same_work_and_a_bounded_residency() {
    let prog = small_qwen();
    let mut rng = R::seed_from_u64(0x0013_2028);
    let params = small_params(&mut rng, &prog);
    let tokens = [3u64, 17, 0, 36, 8];
    let whole = run(&prog, &params, &tokens).expect("the small decoder runs");
    assert!(whole.iter().all(|o| !o.logits.data.is_empty()));

    let total_param_elems: u64 = params.values().map(|t| t.data.len() as u64).sum();
    let widest = widest_row(&prog);
    // The norm gains are read by an elementwise `Mul`, which has no tiled form: they are the largest params loaded whole.
    let gain = 12u64;
    let smallest_weight = prog
        .blocks
        .iter()
        .flat_map(|b| b.nodes.iter())
        .filter_map(|n| match (&n.prim, n.inputs.as_slice()) {
            (Prim::MatMul, [_, Ref::Param(j)]) => Some(prog.params[*j as usize].shape.iter().map(|&d| d as u64).product::<u64>()),
            _ => None,
        })
        .min()
        .expect("the decoder has weights");
    for tile in [1u64, 5, 12, 64, 240, 1_000_000] {
        let source = MapRowSource(&params);
        // Strict (from a tile that holds a gain): a param larger than a tile may be read only by MatMul or Gather. If any other primitive
        // loaded one whole, this would be an Err. Below that the gains load whole and the meter says so.
        let strict = tile >= gain;
        let tiled = if strict { TiledParams::new(&source, tile).strict() } else { TiledParams::new(&source, tile) };
        let got = run(&prog, &tiled, &tokens).unwrap_or_else(|e| panic!("tile {tile}: {e}"));
        assert_eq!(got, whole, "tile {tile}: logits and every commit, position by position");
        let r: TileReport = tiled.report();

        // Sizing. The work is the whole path's: one product per (output element, contraction index), per matmul with a param operand.
        // A param of at most one tile is multiplied whole by the ordinary path, so the tiled count is the whole count exactly when EVERY
        // weight is larger than a tile, and a part of it otherwise — never more.
        let analytic = analytic_param_macs(&prog) * tokens.len() as u128;
        assert!(r.macs <= analytic, "tile {tile}: {} products against the {analytic} the whole path makes", r.macs);
        if tile < smallest_weight {
            assert_eq!(r.macs, analytic, "tile {tile}: the same multiply-accumulates the whole evaluator makes");
        }

        // The tiling bound. Every tile ≤ max(tile, one row); whole loads (the gains) ≤ max(tile, a gain); the peak is their max.
        assert!(r.peak_tile_elems <= tile.max(widest), "tile {tile}: {r:?}");
        assert!(r.whole_peak_elems <= tile.max(gain), "tile {tile}: {r:?}");
        if strict {
            assert!(r.whole_peak_elems <= tile, "tile {tile}: strict, so every whole load fit in a tile — {r:?}");
        }
        assert!(r.peak_param_elems() <= tile.max(widest).max(gain));
        if tile < 1_000 {
            assert!(
                r.peak_param_elems() * 4 < total_param_elems,
                "tile {tile}: the evaluation never held the model ({r:?} of {total_param_elems})"
            );
        }
        for d in &prog.params {
            assert!(tile_elems_bound(rest_elems(&d.shape), tile) <= tile.max(rest_elems(&d.shape)));
        }
    }

    // Each tiled param element is read exactly once per use: with the widest tile the elements read are the sum over every use of a
    // MatMul's whole param and a Gather's rows (one row per position).
    let source = MapRowSource(&params);
    let tiled = TiledParams::new(&source, 1_000_000);
    run(&prog, &tiled, &tokens).unwrap();
    let r = tiled.report();
    let mut per_step = 0u64;
    let mut occ = vec![prog.schedule.pre];
    occ.extend(prog.schedule.layers.iter().copied());
    occ.push(prog.schedule.post);
    for b in occ {
        for n in &prog.blocks[b as usize].nodes {
            match (&n.prim, n.inputs.as_slice()) {
                (Prim::MatMul, [_, Ref::Param(j)]) => {
                    per_step += prog.params[*j as usize].shape.iter().map(|&d| d as u64).product::<u64>()
                }
                (Prim::Gather { .. }, [Ref::Param(j), _]) => per_step += rest_elems(&prog.params[*j as usize].shape),
                _ => {}
            }
        }
    }
    // (A param of at most one tile is loaded whole and not counted as tiled: at 10⁶ nothing is tiled.)
    assert_eq!(r.tiles, 0, "at a tile larger than every param nothing is read in tiles");
    let source = MapRowSource(&params);
    let tiled = TiledParams::new(&source, 1);
    run(&prog, &tiled, &tokens).unwrap();
    let r = tiled.report();
    assert_eq!(r.elems_read, per_step * tokens.len() as u64, "every element of a tiled param is read exactly once per use");
    assert!(r.peak_tile_elems <= widest, "a tile of one element is held to one row: {r:?}");
}

#[test]
fn a_source_that_fails_mid_run_stops_the_run_with_its_own_refusal() {
    struct FailAfter<'a> {
        inner: MapRowSource<'a>,
        left: std::cell::Cell<u32>,
    }
    impl RowSource for FailAfter<'_> {
        fn shape(&self, i: u16, l: Option<u32>) -> Res<Option<(DType, Vec<u64>)>> {
            self.inner.shape(i, l)
        }
        fn rows(&self, i: u16, l: Option<u32>, r0: u64, rows: u64) -> Res<Tensor> {
            if self.left.get() == 0 {
                return Err(ref2::TirError::new(Class::Missing, "injected source I/O failure"));
            }
            self.left.set(self.left.get() - 1);
            self.inner.rows(i, l, r0, rows)
        }
    }
    let prog = small_qwen();
    let mut rng = R::seed_from_u64(0x0013_2029);
    let params = small_params(&mut rng, &prog);
    for allowed in [0u32, 1, 7, 50] {
        let source = FailAfter { inner: MapRowSource(&params), left: std::cell::Cell::new(allowed) };
        let tiled = TiledParams::new(&source, 8);
        let e = run(&prog, &tiled, &[1, 2, 3]).expect_err("the source failed, so the run cannot have succeeded");
        assert_eq!((e.class, e.reason.as_str()), (Class::Missing, "injected source I/O failure"), "after {allowed} reads");
    }
    // An absent param is `Missing`, as in the whole path.
    let mut missing = params.clone();
    let key = *missing.keys().find(|(j, _)| prog.params[*j as usize].name == "wq").unwrap();
    missing.remove(&key);
    let source = MapRowSource(&missing);
    let tiled = TiledParams::new(&source, 8);
    let e = run(&prog, &tiled, &[1]).unwrap_err();
    assert_eq!(e.class, Class::Missing);
    assert_eq!(run(&prog, &missing, &[1]).unwrap_err().class, Class::Missing, "the whole path says the same");
}
