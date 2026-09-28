//! 04b §9.5 differential: this crate's H dissection against the first implementation's, called as a
//! black box through public API only — `misaka_palw_tir::demand::eval_demanded_range` (§9.5.2),
//! and `kaspa_consensus_core::palw_tir_dissect_v1` (the cone's reductions, the site from a step
//! space, the claim check, the obligations, the value bound, and the dissection phase that
//! fold-checks rounds and moves on choices).
//!
//! Programs: random long-`H` attention layers (one to three heads, windows up to `2^18`, a max, a sum
//! and a value product over `H`, optionally a fourth reduction, a committed max, several layers),
//! and random programs of this crate's generator that have a dissected tile. Parties: an honest
//! responder, and liars — a false total in each reduction in turn, with the tile forged to match
//! (so the root claim is admitted) or left honest; rounds that fold to the lie (the lie carried in
//! the last, the first or a random child, or spread), or that do not fold; totals outside their
//! bound; element lists short, long or unsorted — against a challenger that names the first child
//! whose claim differs from the court's evaluation.
//!
//! Compared on every move: the root claim admitted or refused; every round admitted or refused (a lie
//! caught at a fold is caught at the same round); the children's ranges; the bottom's range and
//! verdict (the same `value_index`). Scale with `TIR_REF2_CASES`.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::bridge::{Outcome, catch_any, first_decode};
use common::demsrc::*;
use common::progen::{GenCfg, R, gen_program, pick};
use kaspa_consensus_core as cc;
use misaka_palw_tir as first;
use misaka_palw_tir_ref2 as ref2;
use rand::{Rng, SeedableRng};
use ref2::admit::ranges;
use ref2::build::{ProgBuilder, fixed, ty};
use ref2::codec::encode;
use ref2::demand::{Ctx, Limits, Question, Target, eval_demanded};
use ref2::dissect::{
    Fold, RootClaim, Site, Values, Verdict, admit_root, bottom, check_round, closure, cut, finalize, history_tiles, obligations,
    partials, positions, round_bytes, site, value_bound,
};
use ref2::eval::Params;
use ref2::{DType, Dim, Prim, Program, Ref};

fn scale() -> usize {
    std::env::var("TIR_REF2_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(1)
}

const LIM: Limits = Limits::UNLIMITED;
const W_ROUND: u64 = 1 << 40;

struct Prepared {
    prog: Program,
    fp: first::program::TirProgramV1,
    info: first::validate::ProgramInfo,
    ivs_first: Vec<Vec<first::interval::Interval>>,
}

fn prepare(prog: &Program) -> Option<Prepared> {
    ref2::normal_form::check(prog).ok()?;
    ranges(prog).ok()?;
    let Outcome::Ok(fp) = first_decode(&encode(prog)) else { return None };
    let info = catch_any(|| first::validate::validate(&fp)).ok()?.ok()?;
    let ivs_first = catch_any(|| first::interval::analyze_ranges(&fp)).ok()?.ok()?;
    Some(Prepared { prog: prog.clone(), fp, info, ivs_first })
}

// ---------------------------------------------------------------- long-H attention programs

struct Attn {
    nh: u32,
    dh: u32,
    window: u32,
    layers: usize,
    commit_m: bool,
    extra: bool,
    select_o2: bool,
    many: usize,
}

/// pre embeds the token; each layer attends over a per-layer K/V history of `window` rows: scores,
/// their max `m`, `e = IntExp(scores − m)`, `s = Σ e`, `o = e · V`, `out = Clamp(o / max(s, 1))`
/// committed (the dissected tile), residual carry; post clamps the carry as logits.
fn attn_program(rng: &mut R, a: &Attn) -> Program {
    let hb = 1u32 << 18;
    let tb = 16u32;
    let d = a.nh * a.dh;
    let mut b = ProgBuilder::new(hb, tb);
    let small = |rng: &mut R, n: usize, r: i128| -> Vec<i128> { (0..n).map(|_| rng.gen_range(-r..=r)).collect() };
    let emb = small(rng, (tb * d) as usize, 60);
    let e_c = b.konst(DType::I16, &[tb, d], &emb);
    let hk = b.hist_state("k", DType::I16, &[a.nh, a.dh], a.window, true);
    let hv = b.hist_state("v", DType::I16, &[a.nh, a.dh], a.window, true);
    let pre = b.block("pre", vec![]);
    let x0 = b.node(pre, Prim::Gather { axis: 0, batch_dims: 0 }, &[Ref::Const(e_c), Ref::Input(0)], fixed(DType::I16, &[d]), true);
    b.carry_out(pre, &[x0]);
    let lay = b.block("layer", vec![fixed(DType::I16, &[d])]);
    let proj = |b: &mut ProgBuilder, rng: &mut R, commit: bool| {
        let w = b.konst(DType::I8, &[d, d], &(0..(d * d) as usize).map(|_| rng.gen_range(-3i128..=3)).collect::<Vec<_>>());
        let xr = b.node(lay, Prim::Reshape, &[Ref::CarryIn(0)], fixed(DType::I16, &[1, d]), false);
        let mm = b.node(lay, Prim::MatMul, &[Ref::Node(xr), Ref::Const(w)], fixed(DType::I32, &[1, d]), false);
        let cl = b.node(lay, Prim::Clamp { lo: -90, hi: 90 }, &[Ref::Node(mm)], fixed(DType::I16, &[1, d]), false);
        b.node(lay, Prim::Reshape, &[Ref::Node(cl)], fixed(DType::I16, &[a.nh, a.dh]), commit)
    };
    let commit_q = rng.gen_bool(0.3);
    let q = proj(&mut b, rng, commit_q);
    let q3 = b.node(lay, Prim::Reshape, &[Ref::Node(q)], fixed(DType::I16, &[a.nh, 1, a.dh]), false);
    let k_row = proj(&mut b, rng, true);
    let v_row = proj(&mut b, rng, true);
    let kk = b.node(lay, Prim::HistAppend { state: hk }, &[Ref::Node(k_row)], ty(DType::I16, &[Dim::H, Dim::Fixed(a.nh), Dim::Fixed(a.dh)]), false);
    let vv = b.node(lay, Prim::HistAppend { state: hv }, &[Ref::Node(v_row)], ty(DType::I16, &[Dim::H, Dim::Fixed(a.nh), Dim::Fixed(a.dh)]), false);
    let kt = b.node(lay, Prim::Transpose { perm: vec![1, 2, 0] }, &[Ref::Node(kk)], ty(DType::I16, &[Dim::Fixed(a.nh), Dim::Fixed(a.dh), Dim::H]), false);
    let vt = b.node(lay, Prim::Transpose { perm: vec![1, 0, 2] }, &[Ref::Node(vv)], ty(DType::I16, &[Dim::Fixed(a.nh), Dim::H, Dim::Fixed(a.dh)]), false);
    let sc = b.node(lay, Prim::MatMul, &[Ref::Node(q3), Ref::Node(kt)], ty(DType::I32, &[Dim::Fixed(a.nh), Dim::Fixed(1), Dim::H]), false);
    let m = b.node(lay, Prim::ReduceMax { axis: 2 }, &[Ref::Node(sc)], fixed(DType::I32, &[a.nh, 1, 1]), a.commit_m);
    let diff = b.node(lay, Prim::Sub, &[Ref::Node(sc), Ref::Node(m)], ty(DType::I64, &[Dim::Fixed(a.nh), Dim::Fixed(1), Dim::H]), false);
    let e = b.node(lay, Prim::IntExp, &[Ref::Node(diff)], ty(DType::I32, &[Dim::Fixed(a.nh), Dim::Fixed(1), Dim::H]), false);
    let e_used = if a.select_o2 {
        // O-2: a Select whose condition carries H, with a value operand that depends on m.
        let cmp = b.node(lay, Prim::Compare { cmp: ref2::Cmp::Eq }, &[Ref::Node(sc), Ref::Node(m)], ty(DType::I8, &[Dim::Fixed(a.nh), Dim::Fixed(1), Dim::H]), false);
        let mb = b.node(lay, Prim::Clamp { lo: 0, hi: 1 << 24 }, &[Ref::Node(m)], fixed(DType::I32, &[a.nh, 1, 1]), false);
        b.node(lay, Prim::Select, &[Ref::Node(cmp), Ref::Node(mb), Ref::Node(e)], ty(DType::I32, &[Dim::Fixed(a.nh), Dim::Fixed(1), Dim::H]), false)
    } else {
        e
    };
    let mut sums = vec![b.node(lay, Prim::ReduceSum { axis: 2 }, &[Ref::Node(e_used)], fixed(DType::I64, &[a.nh, 1, 1]), false)];
    for _ in 1..a.many.max(1) {
        sums.push(b.node(lay, Prim::ReduceSum { axis: 2 }, &[Ref::Node(e_used)], fixed(DType::I64, &[a.nh, 1, 1]), false));
    }
    let mut s = sums[0];
    for &x in &sums[1..] {
        s = b.node(lay, Prim::Add, &[Ref::Node(s), Ref::Node(x)], fixed(DType::I64, &[a.nh, 1, 1]), false);
    }
    let sc1 = b.node(lay, Prim::Clamp { lo: 1, hi: 1 << 50 }, &[Ref::Node(s)], fixed(DType::I64, &[a.nh, 1, 1]), false);
    let o = b.node(lay, Prim::MatMul, &[Ref::Node(e_used), Ref::Node(vt)], fixed(DType::I64, &[a.nh, 1, a.dh]), false);
    let mut num = o;
    if a.extra {
        // A fourth reduction: the plain sum of V over H, added to the product.
        let vs = b.node(lay, Prim::ReduceSum { axis: 1 }, &[Ref::Node(vt)], fixed(DType::I32, &[a.nh, 1, a.dh]), false);
        num = b.node(lay, Prim::Add, &[Ref::Node(o), Ref::Node(vs)], fixed(DType::I64, &[a.nh, 1, a.dh]), false);
    }
    let div = b.node(lay, Prim::Div { rule: ref2::Rounding::Floor }, &[Ref::Node(num), Ref::Node(sc1)], fixed(DType::I64, &[a.nh, 1, a.dh]), false);
    let out = b.node(lay, Prim::Clamp { lo: -30000, hi: 30000 }, &[Ref::Node(div)], fixed(DType::I16, &[a.nh, 1, a.dh]), true);
    let y = b.node(lay, Prim::Reshape, &[Ref::Node(out)], fixed(DType::I16, &[d]), false);
    let r = b.node(lay, Prim::Add, &[Ref::CarryIn(0), Ref::Node(y)], fixed(DType::I32, &[d]), false);
    let c = b.node(lay, Prim::Clamp { lo: -30000, hi: 30000 }, &[Ref::Node(r)], fixed(DType::I16, &[d]), true);
    b.carry_out(lay, &[c]);
    let post = b.block("post", vec![fixed(DType::I16, &[d])]);
    let lg = b.node(post, Prim::Cast, &[Ref::CarryIn(0)], fixed(DType::I32, &[d]), true);
    b.schedule(pre, &vec![lay; a.layers], post, lg);
    b.finish()
}

// ---------------------------------------------------------------- the first implementation's site

/// A step space and a job context for a run of `positions` positions, every commit point tiled at
/// `tile_lens` (by `(block, node)`, default the node's element count at the window).
fn space(pp: &Prepared, positions: u32, h_tile: u32, tile_len: u32) -> Result<(cc::palw_tir_step_v1::PalwTirStepSpaceV1, cc::palw_v2::PalwJobContextV2), String> {
    let p = &pp.prog;
    let mut commit_tiles = Vec::new();
    for blk in p.blocks.iter() {
        for n in &blk.nodes {
            if n.commit {
                commit_tiles.push(tile_len);
            }
        }
    }
    let state_tiles = p
        .states
        .iter()
        .map(|s| match s.kind {
            ref2::StateKind::Fixed { .. } => s.shape.iter().product::<u32>().max(4),
            ref2::StateKind::Hist { .. } => s.shape.iter().product::<u32>().max(4),
        })
        .collect();
    let layout = cc::palw_tir_class_v1::PalwTirLayoutV1 {
        version: cc::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1,
        max_context: positions,
        checkpoint_interval: 1,
        h_tile,
        commit_tiles,
        state_tiles,
    };
    let sp = cc::palw_tir_step_v1::PalwTirStepSpaceV1::from_program(pp.fp.clone(), pp.info.clone(), layout).map_err(|e| format!("{e:?}"))?;
    let h = cc::Hash64::default();
    let ctx = cc::palw_v2::PalwJobContextV2 {
        version: 2,
        network_id: vec![],
        job_id: h,
        job_nullifier: h,
        assignment_id: h,
        execution_seed: [0; 32],
        model_profile_id: h,
        runtime_manifest_hash: h,
        runtime_class_id: h,
        shape_profile_id: h,
        trace_scheme_id: h,
        cu_ruleset_id: h,
        tokenizer_id: h,
        prompt_token_ids_hash: h,
        declared_prefill_tokens: positions,
        exact_decode_tokens: 1,
        max_context_tokens: positions,
    };
    Ok((sp, ctx))
}

fn first_site(
    pp: &Prepared,
    sp: &cc::palw_tir_step_v1::PalwTirStepSpaceV1,
    jc: &cc::palw_v2::PalwJobContextV2,
    ctx: Ctx,
    n: u16,
    first_el: u64,
) -> Result<Option<(cc::palw_tir_dissect_v1::PalwTirDissectSiteV1, u64, u32)>, String> {
    let leaves = catch_any(|| sp.leaves_of_position(jc, ctx.pos as u32)).map_err(|e| format!("panic {e}"))?;
    let leaf = leaves.into_iter().find(|l| {
        matches!(l.kind, cc::palw_tir_step_v1::PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
            if occurrence == ctx.occ && node == n && first_element == first_el)
    });
    let Some(leaf) = leaf else { return Err(format!("no commit leaf for {ctx:?} node {n} element {first_el}")) };
    let (index, count) = (leaf.index, leaf.value_count);
    let s = catch_any(|| cc::palw_tir_dissect_v1::palw_tir_dissect_site_v1(sp, &pp.ivs_first, &leaf)).map_err(|e| format!("panic {e}"))?;
    Ok(s.map(|s| (s, index, count)))
}

fn same_site(mine: &Site, theirs: &cc::palw_tir_dissect_v1::PalwTirDissectSiteV1) -> Result<(), String> {
    let folds: Vec<bool> = mine.folds.iter().map(|f| *f == Fold::Max).collect();
    let tf: Vec<bool> = theirs.folds.iter().map(|f| matches!(f, cc::palw_tir_dissect_v1::PalwTirFoldV1::Max)).collect();
    let mb: Vec<(i128, i128)> = mine.bounds.iter().map(|b| (b.lo, b.hi)).collect();
    let tb: Vec<(i128, i128)> = theirs.bounds.iter().map(|b| (b.lo, b.hi)).collect();
    if mine.reductions != theirs.reductions
        || folds != tf
        || mb != tb
        || mine.counts != theirs.counts
        || mine.h != theirs.history_positions as u64
        || mine.h_tile != theirs.tile_positions as u64
        || theirs.node != mine.node
        || theirs.ctx.pos as u64 != mine.ctx.pos
        || theirs.ctx.occurrence as u32 != mine.ctx.occ
    {
        return Err(format!("site: ref2 {mine:?} first {theirs:?}"));
    }
    Ok(())
}

// ---------------------------------------------------------------- the tally

#[derive(Default)]
struct Tally {
    sites: usize,
    games: usize,
    moves: usize,
    caught: BTreeMap<String, usize>,
    disagreements: BTreeMap<String, (usize, Vec<String>)>,
}

impl Tally {
    fn disagree(&mut self, kind: &str, detail: String) {
        let e = self.disagreements.entry(kind.to_string()).or_default();
        e.0 += 1;
        if e.1.len() < 4 {
            e.1.push(detail);
        }
    }
    fn report(&self, name: &str) {
        println!(
            "{name}: {} sites, {} games, {} moves compared, outcomes {:?}, {} disagreements",
            self.sites,
            self.games,
            self.moves,
            self.caught,
            self.disagreements.values().map(|d| d.0).sum::<usize>()
        );
        for (k, (n, ex)) in &self.disagreements {
            println!("  DISAGREE [{k}] ×{n}");
            for e in ex {
                println!("      {e}");
            }
        }
    }
}

// ---------------------------------------------------------------- the game

/// A model whose node answers for the site's supplied reductions come from a claim.
fn with_claim(m: &Model, st: &Site, root: &RootClaim) -> Model {
    let mut x = m.clone();
    for (i, &r) in st.reductions.iter().enumerate() {
        let map: BTreeMap<u64, i128> = root.elements[i].iter().copied().zip(root.totals[i].iter().copied()).collect();
        x.claimed.insert((st.ctx.pos, st.ctx.occ, r), map);
    }
    x
}

/// The first implementation's evaluations of a claim, through its range evaluation: the finalize
/// (with every reduction but `n` supplied), the closure of reads to a fixpoint, and a range's
/// partials. `Err` carries the class of a failed evaluation.
fn first_finalize(pp: &Prepared, st: &Site, tile: &[u64], root: &RootClaim, m: &Model) -> Result<(Vec<i128>, BTreeSet<(usize, u64)>), String> {
    let mm = with_claim(m, st, root);
    let last = st.reductions.len() - 1;
    if st.reductions[last] == st.node {
        let mut reads = BTreeSet::new();
        let mut v = Vec::new();
        for &e in tile {
            reads.insert((last, e));
            match root.elements[last].binary_search(&e) {
                Ok(k) => v.push(root.totals[last][k]),
                Err(_) => return Err("Missing".into()),
            }
        }
        return Ok((v, reads));
    }
    let sup: Vec<u16> = st.reductions.clone();
    let run = first_range(&pp.fp, &pp.info, st.ctx, st.node, tile, &sup, None, &mm, LIM);
    match run.res {
        DRes::Ok(v, _) => Ok((v, supplied_reads(st, &run.asked))),
        other => Err(format!("{other:?}")),
    }
}

fn supplied_reads(st: &Site, asked: &BTreeSet<Question>) -> BTreeSet<(usize, u64)> {
    asked
        .iter()
        .filter_map(|q| match *q {
            Question::Node { ctx, node, i } if ctx == st.ctx => st.reductions.iter().position(|&r| r == node).map(|j| (j, i)),
            _ => None,
        })
        .collect()
}

fn first_closure(pp: &Prepared, st: &Site, tile: &[u64], root: &RootClaim, m: &Model) -> Result<Vec<BTreeSet<u64>>, String> {
    let mm = with_claim(m, st, root);
    let (_, reads) = first_finalize(pp, st, tile, root, m)?;
    let mut c: Vec<BTreeSet<u64>> = vec![BTreeSet::new(); st.reductions.len()];
    let mut work: Vec<(usize, u64)> = Vec::new();
    for (j, e) in reads {
        if c[j].insert(e) {
            work.push((j, e));
        }
    }
    while let Some((i, e)) = work.pop() {
        let r = st.reductions[i];
        let sup: Vec<u16> = st.reductions.iter().copied().filter(|&x| x != r).collect();
        let run = first_range(&pp.fp, &pp.info, st.ctx, r, &[e], &sup, Some((0, 1)), &mm, LIM);
        if !matches!(run.res, DRes::Ok(..)) {
            return Err(format!("probe {r}[{e}]: {:?}", run.res));
        }
        for (j, f) in supplied_reads(st, &run.asked) {
            if c[j].insert(f) {
                work.push((j, f));
            }
        }
    }
    Ok(c)
}

fn first_partials(pp: &Prepared, st: &Site, root: &RootClaim, range: (u64, u64), m: &Model) -> Result<Values, String> {
    let mm = with_claim(m, st, root);
    let mut out = Vec::new();
    for (i, &r) in st.reductions.iter().enumerate() {
        let sup: Vec<u16> = st.reductions.iter().copied().filter(|&x| x != r).collect();
        let run = first_range(&pp.fp, &pp.info, st.ctx, r, &root.elements[i], &sup, Some(range), &mm, LIM);
        match run.res {
            DRes::Ok(v, _) => out.push(v),
            other => return Err(format!("{other:?}")),
        }
    }
    Ok(out)
}

fn to_range_claim(v: &Values) -> cc::palw_tir_dissect_v1::PalwTirRangeClaimV1 {
    cc::palw_tir_dissect_v1::PalwTirRangeClaimV1 { partials: v.clone() }
}

fn dummy_carriage(pp: &Prepared) -> cc::palw_tir_court_v1::PalwTirConeRefutationV1 {
    let h = cc::Hash64::default();
    let job_context = cc::palw_v2::PalwJobContextV2 {
        version: 2,
        network_id: vec![],
        job_id: h,
        job_nullifier: h,
        assignment_id: h,
        execution_seed: [0; 32],
        model_profile_id: h,
        runtime_manifest_hash: h,
        runtime_class_id: h,
        shape_profile_id: h,
        trace_scheme_id: h,
        cu_ruleset_id: h,
        tokenizer_id: h,
        prompt_token_ids_hash: h,
        declared_prefill_tokens: 1,
        exact_decode_tokens: 1,
        max_context_tokens: 1,
    };
    let layout = cc::palw_tir_class_v1::PalwTirLayoutV1 {
        version: 1,
        max_context: 1,
        checkpoint_interval: 1,
        h_tile: 1,
        commit_tiles: vec![],
        state_tiles: vec![],
    };
    let coord = cc::palw_step::PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 };
    cc::palw_tir_court_v1::PalwTirConeRefutationV1 {
        binding: cc::palw_tir_step_v1::PalwTirStepBindingV1 {
            version: 1,
            job_context,
            class: cc::palw_tir_class_v1::PalwTirClassV1 { version: 1, program: encode(&pp.prog), layout, tokenizer_id: h },
            artifact_root: h,
            full_logits_trace_root: h,
            step_leaf_count: 0,
            step_merkle_root: h,
            committed_execution_root: h,
        },
        output_opening: cc::palw_step_leg::PalwStepOpeningV1 { leaf_index: 0, leaf_hash: h, siblings: vec![] },
        output_preimage: cc::palw_step_leg::PalwStepTileLeafV1 { version: 1, coord, value_count: 0, values_le: vec![] },
        operands: cc::palw_step_refute::PalwStepInputRowV1 { preimages: vec![], run_siblings: vec![] },
        params: vec![],
        prompt_token_ids: vec![],
        prompt_ids_openings: vec![],
        decode_tokens: None,
    }
}

/// How the responder plays.
#[derive(Clone, Copy, Debug)]
enum Play {
    Honest,
    /// A false total at `(i, e)`, the tile forged to match; rounds fold to it with the lie carried
    /// by the child at `place` (usize::MAX: the last, a random one otherwise).
    Consistent { i: usize, e: usize, delta: i128, place: Place },
    /// A false total, forged tile, rounds that are honest-given-the-totals (they do not fold).
    NonFolding { i: usize, e: usize, delta: i128 },
    /// A false total with the honest tile.
    Unforged { i: usize, e: usize, delta: i128 },
    /// A total outside its reduction's bound.
    OutOfBound { i: usize, e: usize },
    /// One element dropped from, or added to, a list, or a list reversed.
    ShortList { i: usize },
    LongList { i: usize },
    Unsorted { i: usize },
}

#[derive(Clone, Copy, Debug)]
enum Place {
    Last,
    First,
    Random,
    Spread,
}

/// The responder's children at a round: the honest-given-root partials, adjusted so that they fold
/// to the disputed claim for `(i, e)` (the lie placed per `place`).
fn fold_to(st: &Site, honest: &mut [Values], disputed: &Values, i: usize, e: usize, place: Place, rng: &mut R) {
    let n = honest.len();
    let want = disputed[i][e];
    match st.folds[i] {
        Fold::Sum => {
            let have: i128 = honest.iter().map(|c| c[i][e]).sum();
            let gap = want - have;
            if gap == 0 {
                return;
            }
            match place {
                Place::Last => honest[n - 1][i][e] += gap,
                Place::First => honest[0][i][e] += gap,
                Place::Random => {
                    let k = rng.gen_range(0..n);
                    honest[k][i][e] += gap
                }
                Place::Spread => {
                    let part = gap / n as i128;
                    for c in honest.iter_mut() {
                        c[i][e] += part;
                    }
                    honest[n - 1][i][e] += gap - part * n as i128;
                }
            }
        }
        Fold::Max => {
            let have = honest.iter().map(|c| c[i][e]).max().unwrap();
            if want > have {
                let k = match place {
                    Place::Last | Place::Spread => n - 1,
                    Place::First => 0,
                    Place::Random => rng.gen_range(0..n),
                };
                honest[k][i][e] = want;
            } else if want < have {
                for c in honest.iter_mut() {
                    if c[i][e] > want {
                        c[i][e] = want;
                    }
                }
            }
        }
    }
}

fn flat_index(root: &RootClaim, i: usize, e: usize) -> usize {
    root.elements[..i].iter().map(|l| l.len()).sum::<usize>() + e
}

/// One game on both implementations; returns where it ended ("root refused", "round r refused",
/// "bottom: mismatch at k" / "bottom: defeated").
#[allow(clippy::too_many_arguments)]
fn game(
    t: &mut Tally,
    tag: &str,
    pp: &Prepared,
    m: &Model,
    st: &Site,
    fsite: &cc::palw_tir_dissect_v1::PalwTirDissectSiteV1,
    tile: &[u64],
    honest_root: &RootClaim,
    honest_tile: &[i128],
    play: Play,
    k: u64,
    rng: &mut R,
) {
    t.games += 1;
    let what = format!("{tag} {play:?} arity {k}");
    // The responder's root claim and the committed tile.
    let mut root = honest_root.clone();
    let mut tile_vals = honest_tile.to_vec();
    match play {
        Play::Honest => {}
        Play::Consistent { i, e, delta, .. } | Play::NonFolding { i, e, delta } | Play::Unforged { i, e, delta } => {
            root.totals[i][e] += delta;
            // A consistent liar restates every later reduction as the evaluation given its totals
            // (r_j reads only r_k, k < j), so that its only false statement is (i, e).
            for j in (i + 1)..st.reductions.len() {
                match partials(&pp.prog, st, &root, (0, st.h), &mut Mine(m), LIM) {
                    Ok(v) => root.totals[j] = v[j].clone(),
                    Err(_) => break,
                }
            }
            if !matches!(play, Play::Unforged { .. })
                && let Ok((v, _)) = finalize(&pp.prog, st, tile, &root, &mut Mine(m), LIM)
            {
                tile_vals = v;
            }
        }
        Play::OutOfBound { i, e } => root.totals[i][e] = st.bounds[i].hi + 1,
        Play::ShortList { i } => {
            root.elements[i].pop();
            root.totals[i].pop();
        }
        Play::LongList { i } => {
            if let Some(extra) = (0..st.counts[i]).find(|x| !root.elements[i].contains(x)) {
                let pos = root.elements[i].partition_point(|&x| x < extra);
                root.elements[i].insert(pos, extra);
                root.totals[i].insert(pos, 0);
            } else {
                return;
            }
        }
        Play::Unsorted { i } => {
            if root.elements[i].len() < 2 {
                return;
            }
            root.elements[i].reverse();
            root.totals[i].reverse();
        }
    }
    // The root claim: admitted or refused, on both.
    t.moves += 1;
    let mine = admit_root(&pp.prog, st, tile, &tile_vals, &root, &mut Mine(m), LIM);
    let els32: Vec<Vec<u32>> = root.elements.iter().map(|l| l.iter().map(|&x| x as u32).collect()).collect();
    let check = catch_any(|| cc::palw_tir_dissect_v1::palw_tir_dissect_check_claim_v1(fsite, &els32, &to_range_claim(&root.totals)));
    let theirs: Result<(), String> = match check {
        Err(p) => Err(format!("PANIC {p}")),
        Ok(Err(e)) => Err(format!("{e:?}")),
        Ok(Ok(())) => match first_finalize(pp, st, tile, &root, m) {
            Err(e) => Err(format!("finalize {e}")),
            Ok((v, _)) if v != tile_vals => Err("finalize differs from the tile".into()),
            Ok(_) => match first_closure(pp, st, tile, &root, m) {
                Err(e) => Err(format!("closure {e}")),
                Ok(c) if c.iter().map(|s| s.iter().copied().collect::<Vec<_>>()).collect::<Vec<_>>() != root.elements => {
                    Err("closure differs".into())
                }
                Ok(_) => Ok(()),
            },
        },
    };
    if mine.is_ok() != theirs.is_ok() {
        let lens: Vec<usize> = root.elements.iter().map(|l| l.len()).collect();
        // Finding H1: the first implementation refuses an empty element list, which the text
        // requires when the tile reads none of a reduction's elements (step 5: L = the closure).
        let h1 = mine.is_ok() && lens.contains(&0) && theirs.as_ref().is_err_and(|e| e.contains("ascending, distinct and inside"));
        let kind = if h1 { "H1: an honest root claim with an empty list refused by first" } else { "root claim" };
        t.disagree(kind, format!("{what}: ref2 {mine:?} first {theirs:?} (list lengths {lens:?}, counts {:?})", st.counts));
        return;
    }
    if mine.is_err() {
        *t.caught.entry("root claim refused".into()).or_default() += 1;
        return;
    }
    // The phase, on the first implementation.
    let froot = cc::palw_tir_dissect_v1::PalwTirRootClaimV1 {
        version: 1,
        elements: els32.clone(),
        totals: to_range_claim(&root.totals),
        finalize: Box::new(dummy_carriage(pp)),
    };
    let sid = cc::Hash64::from_u64_word(7);
    let mut phase = match catch_any(|| cc::palw_tir_dissect_v1::PalwTirDissectPhaseV1::open(sid, 0, fsite, &froot, k as u8, 0, W_ROUND)) {
        Ok(Ok(ph)) => ph,
        other => {
            t.disagree("phase open", format!("{what}: {other:?}"));
            return;
        }
    };
    let t_h = history_tiles(st);
    if phase.round_budget() != ref2::dissect::round_budget(t_h, k) {
        t.disagree("round budget", format!("{what}: ref2 {} first {}", ref2::dissect::round_budget(t_h, k), phase.round_budget()));
    }
    let (mut first_t, mut count, mut disputed) = (0u64, t_h, root.totals.clone());
    let mut round = 0u32;
    let mut daa = 1u64;
    while count > 1 {
        let children = cut(first_t, count, k);
        let theirs_children: Vec<(u64, u64)> = phase.child_ranges();
        if theirs_children != children {
            t.disagree("children", format!("{what} round {round}: ref2 {children:?} first {theirs_children:?}"));
            return;
        }
        // The responder's round.
        let mut claims: Vec<Values> = Vec::new();
        for &(cf, cc_) in &children {
            match partials(&pp.prog, st, &root, positions(st, cf, cc_), &mut Mine(m), LIM) {
                Ok(v) => claims.push(v),
                Err(e) => {
                    t.disagree("partials fail", format!("{what}: {e:?}"));
                    return;
                }
            }
        }
        // The court's own evaluation of the children (the challenger's yardstick), on both.
        let court = claims.clone();
        for (x, &(cf, cc_)) in children.iter().enumerate() {
            match first_partials(pp, st, &root, positions(st, cf, cc_), m) {
                Ok(v) if v == court[x] => {}
                other => t.disagree("partials", format!("{what} round {round} child {x}: ref2 {:?} first {other:?}", court[x])),
            }
        }
        if let Play::Consistent { i, place, .. } = play {
            for e in 0..root.elements[i].len() {
                fold_to(st, &mut claims, &disputed, i, e, place, rng);
            }
        }
        t.moves += 1;
        let mine_r = check_round(st, &root, &disputed, &claims, children.len());
        let msg = cc::palw_tir_dissect_v1::PalwTirDissectRoundV1 { version: 1, children: claims.iter().map(to_range_claim).collect() };
        let theirs_r = catch_any(|| {
            let mut ph = phase.clone();
            let r = ph.apply_round(&msg, daa, W_ROUND);
            (r, ph)
        });
        let (theirs_ok, ph2) = match theirs_r {
            Ok((r, ph)) => (r.map_err(|e| format!("{e:?}")), ph),
            Err(p) => {
                t.disagree("first panics", format!("{what}: {p}"));
                return;
            }
        };
        if mine_r.is_ok() != theirs_ok.is_ok() {
            t.disagree("round", format!("{what} round {round}: ref2 {mine_r:?} first {theirs_ok:?}"));
            return;
        }
        if mine_r.is_err() {
            *t.caught.entry(format!("round {round} refused")).or_default() += 1;
            return;
        }
        phase = ph2;
        daa += 1;
        // The challenger names the first child whose claim differs from the court's evaluation.
        let choice = (0..claims.len()).find(|&x| claims[x] != court[x]).unwrap_or_else(|| rng.gen_range(0..claims.len()));
        t.moves += 1;
        let cm = cc::palw_tir_dissect_v1::PalwTirDissectChoiceV1 { version: 1, session_id: sid, round: phase.round(), child: choice as u8 };
        match catch_any(|| {
            let mut ph = phase.clone();
            let r = ph.apply_choice(&cm, daa, W_ROUND);
            (r, ph)
        }) {
            Ok((Ok(()), ph)) => phase = ph,
            other => {
                t.disagree("choice", format!("{what} round {round}: {other:?}"));
                return;
            }
        }
        daa += 1;
        (first_t, count) = children[choice];
        disputed = claims[choice].clone();
        round += 1;
    }
    // The bottom.
    let range = positions(st, first_t, count);
    match phase.terminal_range() {
        Some((a, b)) if (a as u64, b as u64) == range => {}
        other => t.disagree("terminal range", format!("{what}: ref2 {range:?} first {other:?}")),
    }
    t.moves += 1;
    let mine_v = bottom(&pp.prog, st, &root, range, &disputed, &mut Mine(m), LIM);
    let theirs_v = first_partials(pp, st, &root, range, m).map(|court| {
        let fc: Vec<i128> = court.into_iter().flatten().collect();
        let fx: Vec<i128> = disputed.iter().flatten().copied().collect();
        match fc.iter().zip(fx.iter()).position(|(a, b)| a != b) {
            Some(k) => Verdict::Mismatch { value_index: k },
            None => Verdict::Defeated,
        }
    });
    match (&mine_v, &theirs_v) {
        (Ok(a), Ok(b)) if a == b => {
            let key = match a {
                Verdict::Defeated => "bottom: challenger defeated".to_string(),
                Verdict::Mismatch { value_index } => {
                    let expected = match play {
                        Play::Consistent { i, e, .. } => Some(flat_index(&root, i, e.min(root.elements[i].len() - 1))),
                        _ => None,
                    };
                    if expected.is_some_and(|x| x != *value_index) && !matches!(play, Play::Consistent { place: Place::Spread, .. }) {
                        format!("bottom: mismatch elsewhere than the lie ({value_index} vs {expected:?})")
                    } else {
                        "bottom: executor convicted at the lie".to_string()
                    }
                }
            };
            *t.caught.entry(key).or_default() += 1;
        }
        _ => t.disagree("bottom", format!("{what}: ref2 {mine_v:?} first {theirs_v:?}")),
    }
}

/// Every dissected commit tile of the last positions of a run, with games on both implementations.
fn exercise(t: &mut Tally, rng: &mut R, tag: &str, pp: &Prepared, tokens: &[u64], plays_per_site: usize) {
    exercise_with(t, rng, tag, pp, &Params::new(), tokens, plays_per_site)
}

fn exercise_with(t: &mut Tally, rng: &mut R, tag: &str, pp: &Prepared, params: &Params, tokens: &[u64], plays_per_site: usize) {
    let p = &pp.prog;
    let Some(m) = Model::from_run(p, params, tokens, &|_| true) else { return };
    let intervals = ranges(p).unwrap();
    let positions_n = tokens.len() as u64;
    let occs = occurrences(p);
    for pos in positions_n.saturating_sub(2)..positions_n {
        for (o, &(b, _)) in occs.iter().enumerate() {
            for (n, node) in p.blocks[b].nodes.iter().enumerate() {
                if !node.commit || ref2::dissect::cone_reductions(p, b, n as u16).is_empty() {
                    continue;
                }
                let ctx = Ctx { pos, occ: o as u32 };
                let n = n as u16;
                // The layout's rules (Phase F): h_tile a power of two in [1, 4096], tile_len in [4, 2^16].
                let h_tile = pick(rng, &[1u64, 1, 2, 4, 8, 16]);
                let e_count = ref2::tensor::count(&node.out.extents(history_h(p, b, pos)));
                let tile_len = rng.gen_range(4..=64u64) as u32;
                let first_el = rng.gen_range(0..e_count.div_ceil(tile_len as u64)) * tile_len as u64;
                let tile: Vec<u64> = (first_el..(first_el + tile_len as u64).min(e_count)).collect();
                let st = match site(p, &intervals, ctx, n, h_tile) {
                    Ok(Some(s)) => s,
                    other => {
                        t.disagree("ref2 site", format!("{tag}: {other:?}"));
                        continue;
                    }
                };
                t.sites += 1;
                // The first implementation's site, from a step space over the same layout.
                let (sp, jc) = match space(pp, positions_n as u32, h_tile as u32, tile_len) {
                    Ok(x) => x,
                    Err(e) => {
                        t.disagree("step space", format!("{tag}: {e}"));
                        continue;
                    }
                };
                let fsite = match first_site(pp, &sp, &jc, ctx, n, first_el) {
                    Ok(Some((s, _, count))) => {
                        if count as usize != tile.len() {
                            t.disagree("tile length", format!("{tag}: ref2 {} first {count}", tile.len()));
                        }
                        s
                    }
                    other => {
                        t.disagree("first site", format!("{tag} {ctx:?} node {n}: {other:?}"));
                        continue;
                    }
                };
                if let Err(e) = same_site(&st, &fsite) {
                    t.disagree("site", format!("{tag}: {e}"));
                    continue;
                }
                // The honest root claim.
                let mut all_t: Values = Vec::new();
                for (i, &r) in st.reductions.iter().enumerate() {
                    let els: Vec<u64> = (0..st.counts[i]).collect();
                    let (v, _) = eval_demanded(p, &Target::Node { ctx, node: r }, &els, &mut Mine(&m), LIM).unwrap();
                    all_t.push(v);
                }
                let full = RootClaim { elements: st.counts.iter().map(|&k| (0..k).collect()).collect(), totals: all_t.clone() };
                let c = match closure(p, &st, &tile, &full, &mut Mine(&m), LIM) {
                    Ok(c) => c,
                    Err(e) => {
                        t.disagree("honest closure", format!("{tag}: {e:?}"));
                        continue;
                    }
                };
                let l: Vec<Vec<u64>> = c.iter().map(|s| s.iter().copied().collect()).collect();
                if l.iter().map(|x| x.len()).sum::<usize>() > ref2::dissect::MAX_VALUES {
                    continue;
                }
                let root = RootClaim { totals: l.iter().enumerate().map(|(i, li)| li.iter().map(|&e| all_t[i][e as usize]).collect()).collect(), elements: l };
                let honest_tile: Vec<i128> = tile.iter().map(|&e| m.nodes[&(pos, o as u32, n)][e as usize]).collect();
                // The closure on the first implementation's arithmetic.
                match first_closure(pp, &st, &tile, &root, &m) {
                    Ok(fc) if fc.iter().map(|s| s.iter().copied().collect::<Vec<_>>()).collect::<Vec<_>>() == root.elements => {}
                    other => t.disagree("closure", format!("{tag}: ref2 {:?} first {other:?}", root.elements)),
                }
                let kk = pick(rng, &[2u64, 2, 4, 8, 64]);
                let tagx = format!("{tag} {ctx:?} node {n} tile {first_el}+{} h_tile {h_tile}", tile.len());
                game(t, &tagx, pp, &m, &st, &fsite, &tile, &root, &honest_tile, Play::Honest, kk, rng);
                let listed: Vec<usize> = (0..st.reductions.len()).filter(|&i| !root.elements[i].is_empty()).collect();
                if listed.len() < st.reductions.len() {
                    *t.caught.entry("sites with a reduction whose closure is empty".into()).or_default() += 1;
                }
                for _ in 0..plays_per_site {
                    if listed.is_empty() {
                        break;
                    }
                    let i = pick(rng, &listed);
                    let e = rng.gen_range(0..root.elements[i].len());
                    let delta = pick(rng, &[1i128, -1, 2, 1000, -77, 1 << 20]);
                    let place = pick(rng, &[Place::Last, Place::First, Place::Random, Place::Spread]);
                    let play = match rng.gen_range(0..10) {
                        0..=4 => Play::Consistent { i, e, delta, place },
                        5 => Play::NonFolding { i, e, delta },
                        6 => Play::Unforged { i, e, delta },
                        7 => Play::OutOfBound { i, e },
                        8 => Play::ShortList { i },
                        _ => {
                            if rng.gen_bool(0.5) {
                                Play::LongList { i }
                            } else {
                                Play::Unsorted { i }
                            }
                        }
                    };
                    let kk = pick(rng, &[2u64, 2, 4, 8, 64]);
                    game(t, &tagx, pp, &m, &st, &fsite, &tile, &root, &honest_tile, play, kk, rng);
                }
                // A lie in each reduction in turn, carried in the last child.
                for &i in &listed {
                    let e = rng.gen_range(0..root.elements[i].len());
                    game(
                        t,
                        &tagx,
                        pp,
                        &m,
                        &st,
                        &fsite,
                        &tile,
                        &root,
                        &honest_tile,
                        Play::Consistent { i, e, delta: 1, place: Place::Last },
                        2,
                        rng,
                    );
                }
            }
        }
    }
}

fn history_h(p: &Program, b: usize, pos: u64) -> u64 {
    match ref2::normal_form::block_window(p, b).ok().flatten() {
        Some(w) => (pos + 1).min(w as u64),
        None => 1,
    }
}

#[test]
fn attention_long_h() {
    let mut t = Tally::default();
    let n = 24 * scale() as u64;
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0xD155_0000 + seed);
        let a = Attn {
            nh: rng.gen_range(1..=3),
            dh: rng.gen_range(1..=3),
            window: pick(&mut rng, &[4u32, 7, 16, 33, 1 << 18]),
            layers: rng.gen_range(1..=2),
            commit_m: rng.gen_bool(0.3),
            extra: rng.gen_bool(0.4),
            select_o2: false,
            many: 1,
        };
        let prog = attn_program(&mut rng, &a);
        let Some(pp) = prepare(&prog) else {
            t.disagree("attention program refused", format!("seed {seed}"));
            continue;
        };
        let len = rng.gen_range(6..=40usize);
        let tokens: Vec<u64> = (0..len).map(|_| rng.gen_range(0..16)).collect();
        exercise(&mut t, &mut rng, &format!("attn seed {seed}"), &pp, &tokens, 6);
    }
    t.report("long-H attention programs");
    assert!(t.disagreements.is_empty());
}

/// Random programs of this crate's generator that have a dissected tile, over long runs.
#[test]
fn random_programs_long_h() {
    let mut t = Tally::default();
    let (mut tried, mut used) = (0, 0);
    let mut seed = 0u64;
    while used < 60 * scale() && tried < 20_000 * scale() {
        tried += 1;
        seed += 1;
        let mut rng = R::seed_from_u64(0xD157_0000 + seed);
        let g = gen_program(&mut rng, GenCfg { range_safe: true, max_nodes: 30, ..GenCfg::default() });
        let p = &g.prog;
        let dissected = (0..p.blocks.len())
            .any(|b| p.blocks[b].nodes.iter().enumerate().any(|(n, x)| x.commit && !ref2::dissect::cone_reductions(p, b, n as u16).is_empty()));
        if !dissected {
            continue;
        }
        let Some(pp) = prepare(p) else { continue };
        let len = rng.gen_range(5..=24usize);
        let bound = (p.token_bound as u64).clamp(1, 1000);
        let tokens: Vec<u64> = (0..len).map(|_| rng.gen_range(0..bound)).collect();
        if Model::from_run(p, &g.params, &tokens, &|_| true).is_none() {
            continue;
        }
        used += 1;
        exercise_with(&mut t, &mut rng, &format!("gen seed {seed}"), &pp, &g.params, &tokens, 4);
    }
    t.report(&format!("random programs with a dissected tile ({used} programs of {tried} generated)"));
    assert!(t.disagreements.keys().all(|k| k.starts_with("H1")), "disagreements beyond finding H1");
}

/// The cone-level functions on every commit point of random programs and attention variants: the
/// reductions, the obligations O-1 to O-3, the value bound and the round bytes.
#[test]
fn cone_functions() {
    let mut t = Tally::default();
    let mut checked = 0usize;
    let mut broken: BTreeMap<String, usize> = BTreeMap::new();
    let mut progs: Vec<(String, Program)> = Vec::new();
    for seed in 0..(300 * scale() as u64) {
        let mut rng = R::seed_from_u64(0xC0FE_0000 + seed);
        progs.push((format!("gen {seed}"), gen_program(&mut rng, GenCfg { range_safe: true, max_nodes: 30, ..GenCfg::default() }).prog));
        if seed % 5 == 0 {
            let a = Attn {
                nh: rng.gen_range(1..=2),
                dh: rng.gen_range(1..=2),
                window: 8,
                layers: 1,
                commit_m: rng.gen_bool(0.5),
                extra: rng.gen_bool(0.5),
                select_o2: rng.gen_bool(0.5),
                many: if rng.gen_bool(0.3) { rng.gen_range(14..=18) } else { 1 },
            };
            progs.push((format!("attn {seed}"), attn_program(&mut rng, &a)));
        }
    }
    for (name, prog) in progs {
        if ref2::normal_form::check(&prog).is_err() {
            continue;
        }
        let Outcome::Ok(fp) = first_decode(&encode(&prog)) else { continue };
        for (b, blk) in prog.blocks.iter().enumerate() {
            for (n, node) in blk.nodes.iter().enumerate() {
                if !node.commit {
                    continue;
                }
                checked += 1;
                let n16 = n as u16;
                let fb = &fp.blocks[b];
                let mine_r = ref2::dissect::cone_reductions(&prog, b, n16);
                let theirs_r = catch_any(|| cc::palw_tir_dissect_v1::palw_tir_cone_reductions_v1(fb, n16));
                if Ok(mine_r.clone()) != theirs_r {
                    t.disagree("reductions", format!("{name} b{b} n{n}: ref2 {mine_r:?} first {theirs_r:?}"));
                }
                let mine_o = obligations(&prog, b, n16);
                let theirs_o = catch_any(|| cc::palw_tir_dissect_v1::palw_tir_dissect_obligations_v1(fb, n16));
                let ok_first = matches!(theirs_o, Ok(Ok(())));
                if mine_o.is_empty() != ok_first {
                    t.disagree("obligations", format!("{name} b{b} n{n}: ref2 {mine_o:?} first {theirs_o:?}"));
                } else if let Ok(Err(e)) = &theirs_o {
                    let s = format!("{e:?}");
                    let kind = s.split([' ', '{', '(']).next().unwrap_or("").to_string();
                    *broken.entry(kind.clone()).or_default() += 1;
                    let in_set = mine_o.iter().any(|o| match o {
                        ref2::dissect::Obligation::TooManyReductions(_) => kind == "TooManyReductions",
                        ref2::dissect::Obligation::DataDependentRead { node } => s.contains(&format!("node: {node},")),
                        ref2::dissect::Obligation::TotalCarriesH { node } => s.contains(&format!("node: {node} ")),
                    });
                    if !in_set {
                        t.disagree("obligation named", format!("{name} b{b} n{n}: ref2 {mine_o:?} first {s}"));
                    }
                }
                if !mine_r.is_empty() {
                    for tile_len in [1u32, 3, 64] {
                        let mv = value_bound(&prog, b, n16, tile_len as u64);
                        let fv = catch_any(|| cc::palw_tir_dissect_v1::palw_tir_dissect_value_bound_v1(fb, n16, tile_len));
                        if Ok(mv) != fv {
                            // Finding H2: the first implementation's V falls below the text's formula
                            // (and below the real closure: `value_bound_against_closures`).
                            let kind = if fv.as_ref().is_ok_and(|&f| f < mv) { "H2: first's value bound below the text's (and the real closure)" } else { "value bound" };
                            t.disagree(kind, format!("{name} b{b} n{n} tile_len {tile_len}: ref2 {mv} first {fv:?}"));
                        }
                    }
                }
            }
        }
    }
    for k in [2u8, 4, 8, 64] {
        for mm in [1usize, 3, 16] {
            for v in [1u64, 100, 4096] {
                let mine = round_bytes(k as u64, mm as u64, v);
                let theirs = cc::palw_tir_dissect_v1::palw_tir_dissect_round_bytes_v1(k, mm, v);
                if mine != theirs {
                    t.disagree("round bytes", format!("k {k} m {mm} V {v}: ref2 {mine} first {theirs}"));
                }
            }
        }
    }
    println!("cone functions: {checked} commit points, obligations broken (first's names) {broken:?}");
    t.report("cone functions");
    assert!(t.disagreements.keys().all(|k| k.starts_with("H2")), "disagreements beyond finding H2");
}


#[test]
#[ignore]
fn investigate_seed() {
    let seed: u64 = std::env::var("SEED").unwrap_or("67".into()).parse().unwrap();
    let mut rng = R::seed_from_u64(0xD157_0000 + seed);
    let g = gen_program(&mut rng, GenCfg { range_safe: true, max_nodes: 30, ..GenCfg::default() });
    let p = &g.prog;
    for (b, blk) in p.blocks.iter().enumerate() {
        for (n, x) in blk.nodes.iter().enumerate() {
            let reds = ref2::dissect::cone_reductions(p, b, n as u16);
            println!(
                "b{b} n{n:<3} {} {:<40} ins {:?} out {:?}{}",
                if x.commit { "C" } else { " " },
                format!("{:?}", x.prim),
                x.inputs,
                x.out.shape,
                if x.commit && !reds.is_empty() { format!("  reductions {reds:?}") } else { String::new() }
            );
        }
    }
    println!("schedule {:?}", p.schedule);
}

/// Finding H1 on a hand-built layer: two maxima over the history, broadcast and concatenated;
/// a tile of the first half reads only the first maximum.
#[test]
fn h1_empty_list_probe() {
    let mut b = ProgBuilder::new(1 << 18, 8);
    let hk = b.hist_state("k", DType::I16, &[2], 8, true);
    let pre = b.block("pre", vec![]);
    let x = b.node(pre, Prim::Broadcast, &[Ref::Input(0)], fixed(DType::Idx, &[2]), false);
    let xc = b.node(pre, Prim::Clamp { lo: 0, hi: 100 }, &[Ref::Node(x)], fixed(DType::I16, &[2]), true);
    b.carry_out(pre, &[xc]);
    let lay = b.block("layer", vec![fixed(DType::I16, &[2])]);
    let k = b.node(lay, Prim::Clamp { lo: -50, hi: 50 }, &[Ref::CarryIn(0)], fixed(DType::I16, &[2]), true);
    let kk = b.node(lay, Prim::HistAppend { state: hk }, &[Ref::Node(k)], ty(DType::I16, &[Dim::H, Dim::Fixed(2)]), false);
    let kt = b.node(lay, Prim::Transpose { perm: vec![1, 0] }, &[Ref::Node(kk)], ty(DType::I16, &[Dim::Fixed(2), Dim::H]), false);
    let q1 = b.konst(DType::I8, &[1, 2], &[1, 2]);
    let q2 = b.konst(DType::I8, &[1, 2], &[-3, 1]);
    let s1 = b.node(lay, Prim::MatMul, &[Ref::Const(q1), Ref::Node(kt)], ty(DType::I32, &[Dim::Fixed(1), Dim::H]), false);
    let m1 = b.node(lay, Prim::ReduceMax { axis: 1 }, &[Ref::Node(s1)], fixed(DType::I32, &[1, 1]), false);
    let s2 = b.node(lay, Prim::MatMul, &[Ref::Const(q2), Ref::Node(kt)], ty(DType::I32, &[Dim::Fixed(1), Dim::H]), false);
    let m2 = b.node(lay, Prim::ReduceMax { axis: 1 }, &[Ref::Node(s2)], fixed(DType::I32, &[1, 1]), false);
    let b1 = b.node(lay, Prim::Broadcast, &[Ref::Node(m1)], fixed(DType::I32, &[1, 4]), false);
    let b2 = b.node(lay, Prim::Broadcast, &[Ref::Node(m2)], fixed(DType::I32, &[1, 4]), false);
    let cat = b.node(lay, Prim::Concat { axis: 1 }, &[Ref::Node(b1), Ref::Node(b2)], fixed(DType::I32, &[1, 8]), true);
    let c = b.node(lay, Prim::Clamp { lo: -1000, hi: 1000 }, &[Ref::CarryIn(0)], fixed(DType::I16, &[2]), true);
    b.carry_out(lay, &[c]);
    let post = b.block("post", vec![fixed(DType::I16, &[2])]);
    let lg = b.node(post, Prim::Cast, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
    b.schedule(pre, &[lay], post, lg);
    let prog = b.finish();
    let pp = prepare(&prog).expect("normal form and ranges");
    let tokens: Vec<u64> = vec![3, 1, 4, 1, 5, 2];
    let m = Model::from_run(&prog, &Params::new(), &tokens, &|_| true).unwrap();
    let ctx = Ctx { pos: 5, occ: 1 };
    let st = site(&prog, &ranges(&prog).unwrap(), ctx, cat, 2).unwrap().unwrap();
    let (sp, jc) = space(&pp, 6, 2, 4).unwrap();
    for first_el in [0u64, 4] {
        let tile: Vec<u64> = (first_el..first_el + 4).collect();
        let (fsite, _, _) = first_site(&pp, &sp, &jc, ctx, cat, first_el).unwrap().unwrap();
        let mut all_t = Vec::new();
        for &r in &st.reductions {
            all_t.push(eval_demanded(&prog, &Target::Node { ctx, node: r }, &[0], &mut Mine(&m), LIM).unwrap().0);
        }
        let full = RootClaim { elements: vec![vec![0], vec![0]], totals: all_t.clone() };
        let c = closure(&prog, &st, &tile, &full, &mut Mine(&m), LIM).unwrap();
        let l: Vec<Vec<u64>> = c.iter().map(|s| s.iter().copied().collect()).collect();
        let root = RootClaim { totals: l.iter().enumerate().map(|(i, li)| li.iter().map(|&e| all_t[i][e as usize]).collect()).collect(), elements: l.clone() };
        let committed: Vec<i128> = tile.iter().map(|&e| m.nodes[&(5, 1, cat)][e as usize]).collect();
        let mine = admit_root(&prog, &st, &tile, &committed, &root, &mut Mine(&m), LIM);
        let els32: Vec<Vec<u32>> = l.iter().map(|x| x.iter().map(|&e| e as u32).collect()).collect();
        let theirs = cc::palw_tir_dissect_v1::palw_tir_dissect_check_claim_v1(&fsite, &els32, &to_range_claim(&root.totals));
        println!("H1 probe: tile {first_el}..{} reads lists {l:?}: ref2 {mine:?}, first's claim check {theirs:?}", first_el + 4);
    }
}

/// §9.5.2's refusals of a malformed range request, on both implementations.
#[test]
fn range_request_refusals() {
    let mut rng = R::seed_from_u64(0x9A9E);
    let a = Attn { nh: 1, dh: 2, window: 16, layers: 1, commit_m: false, extra: false, select_o2: false, many: 1 };
    let prog = attn_program(&mut rng, &a);
    let pp = prepare(&prog).unwrap();
    let tokens: Vec<u64> = (0..6).map(|i| i % 16).collect();
    let m = Model::from_run(&prog, &Params::new(), &tokens, &|_| true).unwrap();
    let ctx = Ctx { pos: 5, occ: 1 };
    let b = prog.schedule.layers[0] as usize;
    let reds: Vec<u16> = (0..prog.blocks[b].nodes.len() as u16).filter(|&i| ref2::demand::reduces_over_h(&prog, b, i as usize)).collect();
    let r = reds[0];
    let other = (0..prog.blocks[b].nodes.len() as u16).find(|&i| !reds.contains(&i)).unwrap();
    let n = prog.blocks[b].nodes.len() as u16;
    let h = 6u64;
    let cases: Vec<(&str, u16, Vec<u16>, Option<(u64, u64)>)> = vec![
        ("the target supplied", r, vec![r], None),
        ("a supplied index that is no node", r, vec![n + 3], None),
        ("a range on a node that does not reduce over H", other, vec![], Some((0, 1))),
        ("from = to", r, vec![], Some((2, 2))),
        ("from > to", r, vec![], Some((3, 2))),
        ("to > H", r, vec![], Some((0, h + 1))),
        ("the whole history", r, vec![], Some((0, h))),
        ("one index", r, vec![], Some((h - 1, h))),
    ];
    for (what, target, sup, range) in cases {
        let a = mine_range(&prog, ctx, target, &[0], &sup, range, &m, LIM);
        let f = first_range(&pp.fp, &pp.info, ctx, target, &[0], &sup, range, &m, LIM);
        println!("range request, {what}: ref2 {:?} / first {:?}", a.res, f.res);
        assert_eq!(a.res, f.res, "{what}");
    }
}

#[test]
#[ignore]
fn investigate_value_bound() {
    let seed: u64 = std::env::var("SEED").unwrap_or("445".into()).parse().unwrap();
    let blk: usize = std::env::var("BLK").unwrap_or("1".into()).parse().unwrap();
    let node: u16 = std::env::var("NODE").unwrap_or("16".into()).parse().unwrap();
    let mut rng = R::seed_from_u64(0xC0FE_0000 + seed);
    let p = gen_program(&mut rng, GenCfg { range_safe: true, max_nodes: 30, ..GenCfg::default() }).prog;
    let (cone_nodes, _) = ref2::admit::cone(&p, blk, node as usize);
    for (n, x) in p.blocks[blk].nodes.iter().enumerate() {
        let inc = cone_nodes.contains(&(n as u16));
        println!(
            "b{blk} n{n:<3} {}{} {:<36} ins {:?} out {:?}",
            if x.commit { "C" } else { " " },
            if inc { "*" } else { " " },
            format!("{:?}", x.prim),
            x.inputs,
            x.out.shape
        );
    }
    let fp = match first_decode(&encode(&p)) {
        Outcome::Ok(f) => f,
        _ => panic!(),
    };
    for tl in [1u64, 3, 64] {
        println!(
            "tile_len {tl}: ref2 V {} first V {:?}; reductions {:?}",
            value_bound(&p, blk, node, tl),
            cc::palw_tir_dissect_v1::palw_tir_dissect_value_bound_v1(&fp.blocks[blk], node, tl as u32),
            ref2::dissect::cone_reductions(&p, blk, node)
        );
    }
}

/// The value bound against the real closure sizes, where the two bounds differ.
#[test]
#[ignore]
fn value_bound_against_closures() {
    let mut rows = Vec::new();
    for seed in 0..(300 * scale() as u64) {
        let mut rng = R::seed_from_u64(0xC0FE_0000 + seed);
        let g = gen_program(&mut rng, GenCfg { range_safe: true, max_nodes: 30, ..GenCfg::default() });
        let p = &g.prog;
        let Outcome::Ok(fp) = first_decode(&encode(p)) else { continue };
        if ref2::normal_form::check(p).is_err() || ranges(p).is_err() {
            continue;
        }
        for (b, blk) in p.blocks.iter().enumerate() {
            for (n, node) in blk.nodes.iter().enumerate() {
                if !node.commit || ref2::dissect::cone_reductions(p, b, n as u16).is_empty() {
                    continue;
                }
                for tl in [1u64, 3, 64] {
                    let mv = value_bound(p, b, n as u16, tl);
                    let fv = cc::palw_tir_dissect_v1::palw_tir_dissect_value_bound_v1(&fp.blocks[b], n as u16, tl as u32);
                    if mv == fv {
                        continue;
                    }
                    // The real closures: every tile of this commit point at the last position of a run.
                    let occs = occurrences(p);
                    let Some(o) = occs.iter().position(|&(bb, _)| bb == b) else { continue };
                    let len = 7usize;
                    let tokens: Vec<u64> = (0..len as u64).map(|i| i % (p.token_bound as u64).max(1)).collect();
                    let Some(m) = Model::from_run(p, &g.params, &tokens, &|_| true) else { continue };
                    let ctx = Ctx { pos: len as u64 - 1, occ: o as u32 };
                    let Ok(Some(st)) = site(p, &ranges(p).unwrap(), ctx, n as u16, 1) else { continue };
                    let e_count = ref2::tensor::count(&node.out.extents(history_h(p, b, ctx.pos)));
                    let mut all_t: Values = Vec::new();
                    for (i, &r) in st.reductions.iter().enumerate() {
                        let els: Vec<u64> = (0..st.counts[i]).collect();
                        all_t.push(eval_demanded(p, &Target::Node { ctx, node: r }, &els, &mut Mine(&m), LIM).unwrap().0);
                    }
                    let full = RootClaim { elements: st.counts.iter().map(|&k| (0..k).collect()).collect(), totals: all_t };
                    let mut worst = 0usize;
                    let mut start = 0;
                    while start < e_count {
                        let tile: Vec<u64> = (start..(start + tl).min(e_count)).collect();
                        if let Ok(c) = closure(p, &st, &tile, &full, &mut Mine(&m), LIM) {
                            worst = worst.max(c.iter().map(|s| s.len()).sum());
                        }
                        start += tl;
                    }
                    rows.push(format!("seed {seed} b{b} n{n} tile_len {tl}: ref2 V {mv}, first V {fv}, the largest real closure {worst}"));
                }
            }
        }
    }
    for r in &rows {
        println!("{r}");
    }
}
