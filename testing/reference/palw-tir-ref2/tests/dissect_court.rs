//! 04b §9.5 against the first implementation's full IR court, as a black box: a real step leg built
//! by its fail-closed builder from this crate's run, a real binding and evidence store, the root
//! claim built and admitted by the court (`build_tir_root_claim_v1`, `check_tir_root_claim_v1`),
//! its rounds (`build_tir_dissect_round_v1`, the phase) and its bottom (`build_tir_dissect_bottom_v1`,
//! `check_tir_dissect_bottom_v1`), for honest executors and for executors that commit a forged tile
//! and lie about one reduction's total; compared, move by move, with this crate's §9.5.

mod common;

use std::collections::BTreeMap;

use common::bridge::{Outcome, catch_any, first_decode};
use common::demsrc::*;
use common::progen::{R, pick};
use kaspa_consensus_core as cc;
use misaka_palw_tir as first;
use misaka_palw_tir_ref2 as ref2;
use rand::{Rng, SeedableRng};
use ref2::admit::ranges;
use ref2::build::{ProgBuilder, fixed, ty};
use ref2::codec::encode;
use ref2::demand::{Ctx, Limits, Target, eval_demanded};
use ref2::dissect::{
    RootClaim, Site, Values, Verdict, admit_root, bottom, closure, cut, finalize, history_tiles, partials, positions, site,
};
use ref2::eval::Params;
use ref2::{DType, Dim, Prim, Program, Ref};

const LIM: Limits = Limits::UNLIMITED;
const W_ROUND: u64 = 1 << 40;

fn scale() -> usize {
    std::env::var("TIR_REF2_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(1)
}

/// The attention layer of `dissect_differential` (no params, no token read inside the layer).
fn attn(rng: &mut R, nh: u32, dh: u32, window: u32, layers: usize, extra: bool) -> Program {
    let hb = 1u32 << 18;
    let tb = 16u32;
    let d = nh * dh;
    let mut b = ProgBuilder::new(hb, tb);
    let emb: Vec<i128> = (0..(tb * d) as usize).map(|_| rng.gen_range(-60i128..=60)).collect();
    let e_c = b.konst(DType::I16, &[tb, d], &emb);
    let hk = b.hist_state("k", DType::I16, &[nh, dh], window, true);
    let hv = b.hist_state("v", DType::I16, &[nh, dh], window, true);
    let pre = b.block("pre", vec![]);
    let x0 = b.node(pre, Prim::Gather { axis: 0, batch_dims: 0 }, &[Ref::Const(e_c), Ref::Input(0)], fixed(DType::I16, &[d]), true);
    b.carry_out(pre, &[x0]);
    let lay = b.block("layer", vec![fixed(DType::I16, &[d])]);
    let mut proj = |b: &mut ProgBuilder, commit: bool| {
        let w = b.konst(DType::I8, &[d, d], &(0..(d * d) as usize).map(|_| rng.gen_range(-3i128..=3)).collect::<Vec<_>>());
        let xr = b.node(lay, Prim::Reshape, &[Ref::CarryIn(0)], fixed(DType::I16, &[1, d]), false);
        let mm = b.node(lay, Prim::MatMul, &[Ref::Node(xr), Ref::Const(w)], fixed(DType::I32, &[1, d]), false);
        let cl = b.node(lay, Prim::Clamp { lo: -90, hi: 90 }, &[Ref::Node(mm)], fixed(DType::I16, &[1, d]), false);
        b.node(lay, Prim::Reshape, &[Ref::Node(cl)], fixed(DType::I16, &[nh, dh]), commit)
    };
    let q = proj(&mut b, false);
    let k_row = proj(&mut b, true);
    let v_row = proj(&mut b, true);
    let q3 = b.node(lay, Prim::Reshape, &[Ref::Node(q)], fixed(DType::I16, &[nh, 1, dh]), false);
    let kk = b.node(
        lay,
        Prim::HistAppend { state: hk },
        &[Ref::Node(k_row)],
        ty(DType::I16, &[Dim::H, Dim::Fixed(nh), Dim::Fixed(dh)]),
        false,
    );
    let vv = b.node(
        lay,
        Prim::HistAppend { state: hv },
        &[Ref::Node(v_row)],
        ty(DType::I16, &[Dim::H, Dim::Fixed(nh), Dim::Fixed(dh)]),
        false,
    );
    let kt = b.node(
        lay,
        Prim::Transpose { perm: vec![1, 2, 0] },
        &[Ref::Node(kk)],
        ty(DType::I16, &[Dim::Fixed(nh), Dim::Fixed(dh), Dim::H]),
        false,
    );
    let vt = b.node(
        lay,
        Prim::Transpose { perm: vec![1, 0, 2] },
        &[Ref::Node(vv)],
        ty(DType::I16, &[Dim::Fixed(nh), Dim::H, Dim::Fixed(dh)]),
        false,
    );
    let sc =
        b.node(lay, Prim::MatMul, &[Ref::Node(q3), Ref::Node(kt)], ty(DType::I32, &[Dim::Fixed(nh), Dim::Fixed(1), Dim::H]), false);
    let m = b.node(lay, Prim::ReduceMax { axis: 2 }, &[Ref::Node(sc)], fixed(DType::I32, &[nh, 1, 1]), false);
    let diff = b.node(lay, Prim::Sub, &[Ref::Node(sc), Ref::Node(m)], ty(DType::I64, &[Dim::Fixed(nh), Dim::Fixed(1), Dim::H]), false);
    let e = b.node(lay, Prim::IntExp, &[Ref::Node(diff)], ty(DType::I32, &[Dim::Fixed(nh), Dim::Fixed(1), Dim::H]), false);
    let s = b.node(lay, Prim::ReduceSum { axis: 2 }, &[Ref::Node(e)], fixed(DType::I64, &[nh, 1, 1]), false);
    let s1 = b.node(lay, Prim::Clamp { lo: 1, hi: 1 << 50 }, &[Ref::Node(s)], fixed(DType::I64, &[nh, 1, 1]), false);
    let o = b.node(lay, Prim::MatMul, &[Ref::Node(e), Ref::Node(vt)], fixed(DType::I64, &[nh, 1, dh]), false);
    let mut num = o;
    if extra {
        let vs = b.node(lay, Prim::ReduceSum { axis: 1 }, &[Ref::Node(vt)], fixed(DType::I32, &[nh, 1, dh]), false);
        num = b.node(lay, Prim::Add, &[Ref::Node(o), Ref::Node(vs)], fixed(DType::I64, &[nh, 1, dh]), false);
    }
    let div = b.node(
        lay,
        Prim::Div { rule: ref2::Rounding::Floor },
        &[Ref::Node(num), Ref::Node(s1)],
        fixed(DType::I64, &[nh, 1, dh]),
        false,
    );
    let out = b.node(lay, Prim::Clamp { lo: -30000, hi: 30000 }, &[Ref::Node(div)], fixed(DType::I16, &[nh, 1, dh]), true);
    let y = b.node(lay, Prim::Reshape, &[Ref::Node(out)], fixed(DType::I16, &[d]), false);
    let r = b.node(lay, Prim::Add, &[Ref::CarryIn(0), Ref::Node(y)], fixed(DType::I32, &[d]), false);
    let c = b.node(lay, Prim::Clamp { lo: -30000, hi: 30000 }, &[Ref::Node(r)], fixed(DType::I16, &[d]), true);
    b.carry_out(lay, &[c]);
    let post = b.block("post", vec![fixed(DType::I16, &[d])]);
    let lg = b.node(post, Prim::Cast, &[Ref::CarryIn(0)], fixed(DType::I32, &[d]), true);
    b.schedule(pre, &vec![lay; layers], post, lg);
    b.finish()
}

/// One executor's execution, committed: the class, the space, the job, every leaf with its values,
/// the leg's leaf hashes and root, the binding and the evidence store.
struct Execution {
    binding: cc::palw_tir_step_v1::PalwTirStepBindingV1,
    leaves: Vec<(cc::palw_tir_step_v1::PalwTirLeafV1, cc::palw_step_leg::PalwStepTileLeafV1)>,
    hashes: Vec<cc::Hash64>,
    prompt: Vec<u32>,
}

impl cc::palw_tir_court_v1::PalwTirEvidenceStoreV1 for Execution {
    fn step_leaf(&self, index: u64) -> Option<cc::palw_step_leg::PalwStepTileLeafV1> {
        self.leaves.get(index as usize).map(|(_, p)| p.clone())
    }
    fn step_opening(&self, index: u64) -> Option<cc::palw_step_leg::PalwStepOpeningV1> {
        cc::palw_step_leg::step_opening_v1(&self.hashes, index).ok()
    }
    fn step_range_siblings(&self, first: u64, count: u64) -> Option<Vec<cc::Hash64>> {
        cc::palw_step_leg::step_merkle_range_siblings_v1(&self.hashes, first as usize, count as usize).ok()
    }
    fn param_opening(&self, _leaf: u32) -> Option<cc::palw_artifact::PalwArtifactOpeningV1> {
        None
    }
    fn prompt_token_ids(&self) -> Option<Vec<u32>> {
        Some(self.prompt.clone())
    }
    fn prompt_ids_opening(&self, _tile: u32) -> Option<cc::palw_prompt_ids_v1::PalwPromptIdsOpeningV1> {
        None
    }
    fn decode_pin(&self) -> Option<cc::palw_step_refute::PalwDecodeTokenPinV1> {
        None
    }
}

fn layout(p: &Program, positions: u32, h_tile: u32, tile_len: u32) -> cc::palw_tir_class_v1::PalwTirLayoutV1 {
    let mut commit_tiles = Vec::new();
    for blk in &p.blocks {
        for n in &blk.nodes {
            if n.commit {
                commit_tiles.push(tile_len);
            }
        }
    }
    let state_tiles = p.states.iter().map(|s| s.shape.iter().product::<u32>().max(4)).collect();
    cc::palw_tir_class_v1::PalwTirLayoutV1 {
        version: cc::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1,
        max_context: positions,
        checkpoint_interval: 1,
        h_tile,
        commit_tiles,
        state_tiles,
    }
}

/// The values of one leaf from this crate's run (`forge`: a replacement for one commit tile).
fn leaf_values(
    p: &Program,
    m: &Model,
    leaf: &cc::palw_tir_step_v1::PalwTirLeafV1,
    forge: &BTreeMap<(u64, u32, u16, u64), Vec<i128>>,
) -> Result<Vec<i128>, String> {
    use cc::palw_tir_step_v1::PalwTirLeafKindV1 as K;
    let a = leaf.position as u64;
    let n = leaf.value_count as u64;
    match leaf.kind {
        K::Commit { occurrence, node, first_element, .. } => {
            if let Some(v) = forge.get(&(a, occurrence, node, first_element)) {
                return Ok(v.clone());
            }
            let v = m.nodes.get(&(a, occurrence, node)).ok_or(format!("no commit {a} {occurrence} {node}"))?;
            Ok(v[first_element as usize..(first_element + n) as usize].to_vec())
        }
        K::State { state, layer, first_element, .. } => {
            let v = m.states.get(&(a + 1, state, layer.map(|l| l as u32))).ok_or(format!("no state after {a}"))?;
            Ok(v[first_element as usize..(first_element + n) as usize].to_vec())
        }
        K::HistTile { state, layer, first_lane, row_lanes, first_position, .. } => {
            let (occ, src) = m.hist_src[&(state, layer.map(|l| l as u32))];
            let row = |r: u64| -> Result<Vec<i128>, String> {
                let v = match src {
                    HistIn::Node(k) => m.nodes.get(&(r, occ, k)),
                    HistIn::Carry { occ, node } => m.nodes.get(&(r, occ, node)),
                };
                v.cloned().ok_or(format!("no row at {r}"))
            };
            let rows = n / row_lanes as u64;
            let mut out = Vec::new();
            for t in 0..rows {
                let r = row(first_position as u64 + t)?;
                out.extend_from_slice(&r[first_lane as usize..(first_lane + row_lanes as u64) as usize]);
            }
            let _ = p;
            Ok(out)
        }
    }
}

fn execute(
    p: &Program,
    m: &Model,
    tokens: &[u64],
    h_tile: u32,
    tile_len: u32,
    forge: &BTreeMap<(u64, u32, u16, u64), Vec<i128>>,
) -> Result<(Execution, cc::palw_tir_step_v1::PalwTirStepSpaceV1, cc::palw_v2::PalwJobContextV2), String> {
    let positions = tokens.len() as u32;
    let class = cc::palw_tir_class_v1::PalwTirClassV1 {
        version: cc::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1,
        program: encode(p),
        layout: layout(p, positions, h_tile, tile_len),
        tokenizer_id: cc::Hash64::from_u64_word(3),
    };
    let space = cc::palw_tir_step_v1::PalwTirStepSpaceV1::new(&class).map_err(|e| format!("space {e:?}"))?;
    let artifact_root = cc::Hash64::from_u64_word(11);
    let class_id = class.class_id(&artifact_root);
    let prompt: Vec<u32> = tokens.iter().map(|&t| t as u32).collect();
    // The yardstick context of the class at (prefill = positions, decode = 1), with this prompt.
    let mut jc =
        cc::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class_id, (positions, 1)).ok_or("no canonical context")?;
    jc.prompt_token_ids_hash = cc::palw_v2::prompt_token_ids_hash_v2(&prompt);
    // The binding's job-context shape wants n_ctx = prefill + decode (found by probing
    // verify_tir_binding_v1; the canonical context carries the layout's max_context).
    jc.max_context_tokens = positions + 1;
    let mut builder =
        cc::palw_tir_step_v1::PalwTirStepLegBuilderV1::new(&space, &jc, class_id, 1 << 30).map_err(|e| format!("builder {e:?}"))?;
    let mut leaves = Vec::new();
    while let Some(leaf) = builder.next_leaf() {
        let vals = leaf_values(p, m, &leaf, forge)?;
        let pre = cc::palw_tir_step_v1::palw_tir_leaf_preimage_v1(&leaf, &vals).map_err(|e| format!("preimage {e:?}"))?;
        builder.push(&vals).map_err(|e| format!("push {e:?} at {leaf:?}"))?;
        leaves.push((leaf, pre));
    }
    let (count, root) = builder.finish().map_err(|e| format!("finish {e:?}"))?;
    let ctx_hash = jc.context_hash();
    let hashes: Vec<cc::Hash64> =
        leaves.iter().map(|(_, pre)| cc::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &class_id, pre)).collect();
    let my_root = cc::palw_step_leg::step_merkle_root_v1(&hashes).map_err(|e| format!("root {e:?}"))?;
    if my_root != root {
        return Err("the leaf hashes do not rebuild the builder's root".into());
    }
    let flt = cc::Hash64::from_u64_word(12);
    let binding = cc::palw_tir_step_v1::PalwTirStepBindingV1 {
        version: cc::palw_tir_step_v1::PALW_TIR_STEP_BINDING_VERSION_V1,
        job_context: jc.clone(),
        class,
        artifact_root,
        full_logits_trace_root: flt,
        step_leaf_count: count,
        step_merkle_root: root,
        committed_execution_root: cc::palw_tir_step_v1::palw_tir_execution_root_v1(&ctx_hash, &flt, &class_id, count, &root),
    };
    cc::palw_tir_step_v1::verify_tir_binding_v1(&binding, 1 << 30).map_err(|e| format!("binding {e:?}"))?;
    Ok((Execution { binding, leaves, hashes, prompt }, space, jc))
}

fn rules() -> cc::palw_tir_court_v1::PalwTirCourtRulesV1 {
    cc::palw_tir_court_v1::PalwTirCourtRulesV1 {
        max_step_leaf_count: 1 << 30,
        prompt_form: cc::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        limits: first::demand::DemandLimits { max_elements: u64::MAX, max_terms: u64::MAX },
    }
}

#[derive(Default)]
struct Tally {
    games: usize,
    outcomes: BTreeMap<String, usize>,
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
}

fn to_claim(v: &Values) -> cc::palw_tir_dissect_v1::PalwTirRangeClaimV1 {
    cc::palw_tir_dissect_v1::PalwTirRangeClaimV1 { partials: v.clone() }
}

#[test]
fn the_court_on_long_h_attention() {
    let mut t = Tally::default();
    let n = 12 * scale() as u64;
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0xC047_0000 + seed);
        let nh = rng.gen_range(1..=2);
        let dh = rng.gen_range(1..=3);
        let window = pick(&mut rng, &[8u32, 16, 1 << 18]);
        let extra = rng.gen_bool(0.4);
        let layers = rng.gen_range(1..=2);
        let prog = attn(&mut rng, nh, dh, window, layers, extra);
        let Outcome::Ok(fp) = first_decode(&encode(&prog)) else { continue };
        let _ = fp;
        let len = rng.gen_range(6..=24usize);
        let tokens: Vec<u64> = (0..len).map(|_| rng.gen_range(0..16)).collect();
        let m = Model::from_run(&prog, &Params::new(), &tokens, &|_| true).expect("the run");
        let h_tile = pick(&mut rng, &[1u32, 2, 4, 8]);
        let tile_len = pick(&mut rng, &[4u32, 8, 64]);
        let honest = match execute(&prog, &m, &tokens, h_tile, tile_len, &BTreeMap::new()) {
            Ok(x) => x,
            Err(e) => {
                t.disagree("execution", format!("seed {seed}: {e}"));
                continue;
            }
        };
        let intervals = ranges(&prog).unwrap();
        // Every dissected commit leaf of the last position.
        let last = (len - 1) as u32;
        let leaves = honest.1.leaves_of_position(&honest.2, last);
        for leaf in leaves {
            let cc::palw_tir_step_v1::PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } = leaf.kind else { continue };
            let ctx = Ctx { pos: last as u64, occ: occurrence };
            let Ok(Some(st)) = site(&prog, &intervals, ctx, node, h_tile as u64) else { continue };
            let tile: Vec<u64> = (first_element..first_element + leaf.value_count as u64).collect();
            let tag = format!("seed {seed} leaf {} {ctx:?} node {node} tile {first_element}+{}", leaf.index, tile.len());
            play(&mut t, &tag, &prog, &m, &tokens, &st, &tile, leaf.index, &honest.0, h_tile, tile_len, &mut rng);
        }
    }
    println!(
        "the court on long-H attention: {} games, outcomes {:?}, {} disagreements",
        t.games,
        t.outcomes,
        t.disagreements.values().map(|d| d.0).sum::<usize>()
    );
    for (k, (n, ex)) in &t.disagreements {
        println!("  DISAGREE [{k}] ×{n}");
        for e in ex {
            println!("      {e}");
        }
    }
    assert!(t.disagreements.is_empty());
}

/// The honest game and a forged-tile lie in each reduction, on the court and on this crate.
#[allow(clippy::too_many_arguments)]
fn play(
    t: &mut Tally,
    tag: &str,
    prog: &Program,
    m: &Model,
    tokens: &[u64],
    st: &Site,
    tile: &[u64],
    narrowed: u64,
    honest: &Execution,
    h_tile: u32,
    tile_len: u32,
    rng: &mut R,
) {
    let rules = rules();
    // The court builds the honest executor's root claim; compare with this crate's.
    let root_f = match catch_any(|| cc::palw_tir_court_v1::build_tir_root_claim_v1(&honest.binding, narrowed, honest, &rules)) {
        Ok(Ok(r)) => r,
        other => {
            t.disagree("court root build", format!("{tag}: {other:?}"));
            return;
        }
    };
    let mut all_t: Values = Vec::new();
    for (i, &r) in st.reductions.iter().enumerate() {
        let els: Vec<u64> = (0..st.counts[i]).collect();
        all_t.push(eval_demanded(prog, &Target::Node { ctx: st.ctx, node: r }, &els, &mut Mine(m), LIM).unwrap().0);
    }
    let full = RootClaim { elements: st.counts.iter().map(|&k| (0..k).collect()).collect(), totals: all_t.clone() };
    let l: Vec<Vec<u64>> =
        closure(prog, st, tile, &full, &mut Mine(m), LIM).unwrap().iter().map(|s| s.iter().copied().collect()).collect();
    let root = RootClaim {
        totals: l.iter().enumerate().map(|(i, li)| li.iter().map(|&e| all_t[i][e as usize]).collect()).collect(),
        elements: l,
    };
    let theirs_l: Vec<Vec<u64>> = root_f.elements.iter().map(|x| x.iter().map(|&e| e as u64).collect()).collect();
    if theirs_l != root.elements || root_f.totals.partials != root.totals {
        t.disagree(
            "honest root claim",
            format!("{tag}: ref2 {:?}/{:?} court {:?}/{:?}", root.elements, root.totals, theirs_l, root_f.totals.partials),
        );
        return;
    }
    let mut games: Vec<(Option<(usize, usize, i128)>, u8)> = vec![(None, 0)];
    for i in (0..st.reductions.len()).filter(|&i| !root.elements[i].is_empty()) {
        for kind in [0u8, 0, 1, 2] {
            let e = rng.gen_range(0..root.elements[i].len());
            games.push((Some((i, e, pick(rng, &[1i128, -1, 5, 1 << 20]))), kind));
        }
    }
    for (lie, kind) in games {
        let arity = pick(rng, &[2u8, 2, 4, 8]) as u64;
        t.games += 1;
        // The executor's claim and, for a liar, its forged execution (the tile its totals finalize to).
        let mut claim = root.clone();
        let mut exec_store: Option<Execution> = None;
        if let Some((i, e, d)) = lie {
            claim.totals[i][e] += d;
            for j in (i + 1)..st.reductions.len() {
                if let Ok(v) = partials(prog, st, &claim, (0, st.h), &mut Mine(m), LIM) {
                    claim.totals[j] = v[j].clone();
                }
            }
            let Ok((forged, _)) = finalize(prog, st, tile, &claim, &mut Mine(m), LIM) else { continue };
            if kind == 2 {
                // Unforged: the honest execution, lying totals.
                exec_store = None;
            } else {
                let mut forge = BTreeMap::new();
                forge.insert((st.ctx.pos, st.ctx.occ, st.node, tile[0]), forged.clone());
                match execute(prog, m, tokens, h_tile, tile_len, &forge) {
                    Ok((x, _, _)) => exec_store = Some(x),
                    Err(e) => {
                        // A forged value outside the node's dtype cannot be committed at all.
                        *t.outcomes.entry(format!("forged tile uncommittable: {}", e.split(' ').next().unwrap_or(""))).or_default() +=
                            1;
                        continue;
                    }
                }
            }
        }
        let exec = exec_store.as_ref().unwrap_or(honest);
        let committed: Vec<i128> = match lie {
            Some(_) if kind != 2 => finalize(prog, st, tile, &claim, &mut Mine(m), LIM).unwrap().0,
            _ => tile.iter().map(|&e| m.nodes[&(st.ctx.pos, st.ctx.occ, st.node)][e as usize]).collect(),
        };
        // The root claim: the court's builder gives the carriage; the executor states its totals.
        let mut root_c = match catch_any(|| cc::palw_tir_court_v1::build_tir_root_claim_v1(&exec.binding, narrowed, exec, &rules)) {
            Ok(Ok(r)) => r,
            other => {
                t.disagree("court root build (forged)", format!("{tag} {lie:?}: {other:?}"));
                continue;
            }
        };
        root_c.elements = claim.elements.iter().map(|x| x.iter().map(|&e| e as u32).collect()).collect();
        root_c.totals = to_claim(&claim.totals);
        let mine_admit = admit_root(prog, st, tile, &committed, &claim, &mut Mine(m), LIM);
        let court_admit = catch_any(|| cc::palw_tir_court_v1::check_tir_root_claim_v1(&root_c, narrowed, &rules));
        let fsite = match (&mine_admit, court_admit) {
            (Ok(()), Ok(Ok(s))) => s,
            (Err(_), Ok(Err(_))) => {
                *t.outcomes.entry("root claim refused by both".into()).or_default() += 1;
                continue;
            }
            (a, b) => {
                t.disagree("root claim admission", format!("{tag} {lie:?}: ref2 {a:?} court {b:?}"));
                continue;
            }
        };
        // Rounds at arity 2, the court building the honest-given-root partials; the liar's round
        // folds to its claim with the lie carried by the last child.
        let sid = cc::Hash64::from_u64_word(99);
        let mut phase =
            match cc::palw_tir_dissect_v1::PalwTirDissectPhaseV1::open(sid, narrowed, &fsite, &root_c, arity as u8, 0, W_ROUND) {
                Ok(p) => p,
                Err(e) => {
                    t.disagree("phase open", format!("{tag}: {e:?}"));
                    continue;
                }
            };
        let (mut first_t, mut count, mut disputed) = (0u64, history_tiles(st), claim.totals.clone());
        let mut daa = 1;
        let mut ok = true;
        while count > 1 {
            let children = cut(first_t, count, arity);
            let built = catch_any(|| cc::palw_tir_court_v1::build_tir_dissect_round_v1(&exec.binding, &phase, h_tile, exec, &rules));
            let mut claims: Vec<Values> = Vec::new();
            for &(cf, c) in &children {
                claims.push(partials(prog, st, &claim, positions(st, cf, c), &mut Mine(m), LIM).unwrap());
            }
            match built {
                Ok(Ok(r)) if r.children.iter().map(|c| c.partials.clone()).collect::<Vec<_>>() == claims => {}
                other => t.disagree("court round build", format!("{tag} {lie:?}: ref2 {claims:?} court {other:?}")),
            }
            let court_eval = claims.clone();
            if let Some((i, e, _)) = lie
                && kind != 1
            {
                let last = claims.len() - 1;
                match st.folds[i] {
                    ref2::dissect::Fold::Sum => {
                        let have: i128 = claims.iter().map(|c| c[i][e]).sum();
                        claims[last][i][e] += disputed[i][e] - have;
                    }
                    ref2::dissect::Fold::Max => {
                        let want = disputed[i][e];
                        let have = claims.iter().map(|c| c[i][e]).max().unwrap();
                        if want > have {
                            claims[last][i][e] = want;
                        } else {
                            for c in claims.iter_mut() {
                                c[i][e] = c[i][e].min(want);
                            }
                        }
                    }
                }
            }
            let mine_r = ref2::dissect::check_round(st, &claim, &disputed, &claims, children.len());
            let msg = cc::palw_tir_dissect_v1::PalwTirDissectRoundV1 { version: 1, children: claims.iter().map(to_claim).collect() };
            let court_r = phase.apply_round(&msg, daa, W_ROUND);
            if mine_r.is_ok() != court_r.is_ok() {
                t.disagree("round", format!("{tag} {lie:?}: ref2 {mine_r:?} court {court_r:?}"));
                ok = false;
                break;
            }
            if mine_r.is_err() {
                *t.outcomes.entry("round refused by both".into()).or_default() += 1;
                ok = false;
                break;
            }
            daa += 1;
            let choice = (0..claims.len()).find(|&x| claims[x] != court_eval[x]).unwrap_or(claims.len() - 1);
            let cm = cc::palw_tir_dissect_v1::PalwTirDissectChoiceV1 {
                version: 1,
                session_id: sid,
                round: phase.round(),
                child: choice as u8,
            };
            if let Err(e) = phase.apply_choice(&cm, daa, W_ROUND) {
                t.disagree("choice", format!("{tag}: {e:?}"));
                ok = false;
                break;
            }
            daa += 1;
            (first_t, count) = children[choice];
            disputed = claims[choice].clone();
        }
        if !ok {
            continue;
        }
        // The bottom: the court builds the carriage and grades it.
        let range = positions(st, first_t, count);
        let mine_v = bottom(prog, st, &claim, range, &disputed, &mut Mine(m), LIM);
        let court_v = catch_any(|| {
            let b = cc::palw_tir_court_v1::build_tir_dissect_bottom_v1(&exec.binding, &phase, exec, &rules)?;
            Ok::<_, cc::palw_tir_court_v1::PalwTirEvidenceErrorV1>(cc::palw_tir_court_v1::check_tir_dissect_bottom_v1(
                &phase, &b, narrowed, &rules,
            ))
        });
        let court = match court_v {
            Ok(Ok(Ok(v))) => match v.fault {
                cc::palw_step_leg::PalwStepFaultV1::ComputationMismatch { value_index } => {
                    Ok(Verdict::Mismatch { value_index: value_index as usize })
                }
                other => Err(format!("{other:?}")),
            },
            Ok(Ok(Err(cc::palw_step_refute::PalwStepRefuteError::NoFaultFound))) => Ok(Verdict::Defeated),
            other => Err(format!("{other:?}")),
        };
        match (&mine_v, &court) {
            (Ok(a), Ok(b)) if a == b => {
                let key = match (a, lie) {
                    (Verdict::Defeated, None) => "honest: challenger defeated at the bottom".to_string(),
                    (Verdict::Mismatch { value_index }, Some((i, e, _))) => {
                        let at: usize = claim.elements[..i].iter().map(|l| l.len()).sum::<usize>() + e;
                        if *value_index == at {
                            "a lie in reduction convicted at its value".into()
                        } else {
                            format!("a lie convicted elsewhere ({value_index} vs {at})")
                        }
                    }
                    other => format!("unexpected {other:?}"),
                };
                *t.outcomes.entry(key).or_default() += 1;
            }
            _ => t.disagree("bottom", format!("{tag} {lie:?}: ref2 {mine_v:?} court {court:?}")),
        }
    }
}

#[allow(dead_code)]
fn unused() -> (f32, Option<()>) {
    let _ = pick::<u8>;
    (0.0, None)
}

/// Finding H1 on the court itself: the honest executor's own root claim, built by the court's
/// builder for a tile that reads one of the cone's two maxima, then graded by the court.
#[test]
fn h1_on_the_court() {
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
    let tokens: Vec<u64> = vec![3, 1, 4, 1, 5, 2];
    let m = Model::from_run(&prog, &Params::new(), &tokens, &|_| true).unwrap();
    let (exec, space, jc) = execute(&prog, &m, &tokens, 2, 4, &BTreeMap::new()).expect("execution");
    let rules = rules();
    for leaf in space.leaves_of_position(&jc, 5) {
        let cc::palw_tir_step_v1::PalwTirLeafKindV1::Commit { node, first_element, .. } = leaf.kind else { continue };
        if node != cat {
            continue;
        }
        let built = cc::palw_tir_court_v1::build_tir_root_claim_v1(&exec.binding, leaf.index, &exec, &rules);
        let graded = built.as_ref().map(|r| cc::palw_tir_court_v1::check_tir_root_claim_v1(r, leaf.index, &rules).map(|_| "admitted"));
        println!(
            "H1 on the court: tile {first_element}..{}: the court builds elements {:?}; the court grades it {:?}",
            first_element + leaf.value_count as u64,
            built.as_ref().map(|r| r.elements.clone()),
            graded
        );
    }
}
