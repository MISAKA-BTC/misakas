//! **RFC-0007 Part II, CP2: the seat-local algebraic check is sound on tiny classes.**
//!
//! Every test runs a producer — honest, or one that changes ONE node's value as it computes it and
//! then computes everything downstream honestly from the lie (the strongest liar: its witness and
//! its committed rows agree with each other everywhere but at the lie) — and a seat that holds no
//! weight matrix, only its sketches. What is held:
//!
//! * an honest witness is accepted, and the two producers (typed backend, reference walk) serve it
//!   byte for byte alike;
//! * a lie in an accumulator, a narrowed output, a routed expert, an attention row, a token — and,
//!   over hundreds of random lies at random nodes, every lie that changes the claim — is refused;
//!   a seat accepts only the honest claim;
//! * a tamperer who knows the vector passes, and one who does not is caught with probability
//!   `1 − 1/p` (shown at a toy prime, where `1/p` is observable);
//! * each node is checked over the moduli its refined interval needs, and a node checked over one
//!   too few lets an error of exactly that prime through.

use std::collections::BTreeSet;

use misaka_palw_tir::{DType, Prim, Tensor};
use misaka_palw_tir_exec::{TirParams, TirPlan};
use misaka_palw_tir_sketch::fixture::{TirSketchFixtureV1, dense_moe_v1, wide_v1};
use misaka_palw_tir_sketch::geom::TirCheckGeomV1;
use misaka_palw_tir_sketch::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const CLASS: [u8; 64] = [0x51; 64];
const JOB_ID: [u8; 32] = [0x7A; 32];

fn job() -> TirSketchJobV1 {
    TirSketchJobV1 { prompt: vec![3, 17, 5, 29, 11], decode: 4 }
}

fn served() -> TirCheckPolicyV1 {
    TirCheckPolicyV1 { act_act: TirActActPolicyV1::Served, weight_min_k: 0 }
}

fn recompute() -> TirCheckPolicyV1 {
    TirCheckPolicyV1 { act_act: TirActActPolicyV1::Recompute, weight_min_k: 0 }
}

struct Class {
    fx: TirSketchFixtureV1,
    plan: TirPlan,
    analysis: TirSketchAnalysisV1,
}

fn class(fx: TirSketchFixtureV1) -> Class {
    let plan = TirPlan::compile(&fx.program).expect("the fixture is a program in normal form");
    let analysis = TirSketchAnalysisV1::of(&fx.program);
    Class { fx, plan, analysis }
}

impl Class {
    fn produce(&self, policy: &TirCheckPolicyV1, tamper: &mut dyn FnMut(&TirTamperSiteV1, &mut Tensor)) -> TirWitnessV1 {
        tir_witness_produce_v1(&self.plan, &self.analysis, &self.fx.params, &job(), policy, tamper).expect("the producer runs")
    }

    fn honest(&self, policy: &TirCheckPolicyV1) -> TirWitnessV1 {
        self.produce(policy, &mut |_, _| {})
    }

    /// The params a seat holds: what the checker reads, never a sketched weight.
    fn held(&self, policy: &TirCheckPolicyV1) -> misaka_palw_tir::MapParams {
        self.fx.params_only(&self.analysis.held_params(&self.fx.program, 64, policy))
    }

    fn param_names(&self, set: &BTreeSet<u16>) -> BTreeSet<String> {
        set.iter().map(|j| self.fx.program.params[*j as usize].name.clone()).collect()
    }

    /// One seat's verdict on a witness, with its own secret and (optionally) forced moduli.
    fn seat_check(
        &self,
        secret: u8,
        policy: &TirCheckPolicyV1,
        moduli: Option<&[TirSketchModulusV1]>,
        w: &TirWitnessV1,
    ) -> Result<TirCheckReportV1, Box<TirCheckFailureV1>> {
        let keys = TirSeatSketchSecretV1::from_bytes([secret; 32]).keys(&CLASS, 1);
        let store =
            TirSketchStoreV1::build_with(&self.plan, &self.analysis, &self.fx.params, &keys, moduli).expect("the store builds");
        let held = self.held(policy);
        let mut checker = TirSketchCheckerV1::new(&self.plan, &self.analysis, &store, &keys, &held, *policy).expect("a checker");
        if let Some(m) = moduli {
            checker = checker.with_fresh_moduli(m);
        }
        checker.check(&job(), &JOB_ID, w)
    }

    fn check(&self, policy: &TirCheckPolicyV1, w: &TirWitnessV1) -> Result<TirCheckReportV1, Box<TirCheckFailureV1>> {
        self.seat_check(0x33, policy, None, w)
    }

    fn block_named(&self, name: &str) -> u8 {
        self.fx.program.blocks.iter().position(|b| b.name == name).expect("a block of the fixture") as u8
    }

    /// The occurrence a block runs at (the first one).
    fn occurrence_of(&self, block: u8) -> u16 {
        self.plan.occurrences.iter().position(|(b, _)| *b == block).expect("scheduled") as u16
    }

    fn matmuls(&self, block: u8, pred: impl Fn(&TirMatMulKindV1) -> bool) -> Vec<u16> {
        self.analysis.blocks[block as usize].matmuls.iter().filter(|s| pred(&s.kind)).map(|s| s.node).collect()
    }
}

/// A tamper that adds `delta` to element `elem` of node `node` of occurrence `occ` at position `pos`.
fn add_at(pos: u32, occ: u16, node: u16, elem: usize, delta: i128) -> impl FnMut(&TirTamperSiteV1, &mut Tensor) {
    move |s, v| {
        if s.pos == pos && s.occurrence == occ && s.node == node {
            v.data[elem] += delta;
        }
    }
}

fn is_weight(k: &TirMatMulKindV1) -> bool {
    matches!(k, TirMatMulKindV1::Weight { .. })
}

fn fault(r: Result<TirCheckReportV1, Box<TirCheckFailureV1>>) -> TirCheckFailureV1 {
    *r.expect_err("the seat refuses")
}

#[test]
fn the_analysis_sketches_every_weight_and_leaves_activation_products_to_the_policy() {
    let c = class(dense_moe_v1(1));
    let (dense, moe) = (c.block_named("dense"), c.block_named("moe"));
    let kinds = |b: u8| c.analysis.blocks[b as usize].matmuls.iter().map(|s| s.kind).collect::<Vec<_>>();
    let left_static =
        |k: &TirMatMulKindV1| matches!(k, TirMatMulKindV1::Weight { side: TirSideV1::Left, source: TirWeightSourceV1::Static(_) });
    let routed = |k: &TirMatMulKindV1| {
        matches!(k, TirMatMulKindV1::Weight { side: TirSideV1::Left, source: TirWeightSourceV1::Routed { idx_rank: 1, .. } })
    };
    // dense: q, k, v projections, Q·Kᵀ, P·V, o, gate, up, down.
    let d = kinds(dense);
    assert_eq!(d.len(), 9);
    assert_eq!(d.iter().filter(|k| left_static(k)).count(), 7);
    assert_eq!(d.iter().filter(|k| **k == TirMatMulKindV1::ActAct).count(), 2, "Q·Kᵀ and P·V");
    // moe: router, the three routed expert products, the combine, the shared expert's four.
    let m = kinds(moe);
    assert_eq!(m.iter().filter(|k| routed(k)).count(), 3, "gate, up and down are gathered by the committed TopK");
    assert_eq!(m.iter().filter(|k| left_static(k)).count(), 5, "router + shared gate/up/down + shared gate scalar");
    assert_eq!(m.iter().filter(|k| **k == TirMatMulKindV1::ActAct).count(), 1, "the combine");
    // The seat holds norms, narrowings, tables and the embedding — never a weight matrix.
    let held = c.param_names(&c.analysis.held_params(&c.fx.program, 64, &served()));
    let sketched = c.param_names(&c.analysis.sketched_params(&c.fx.program, 64, &served()));
    for w in [
        "blk.attn_q.w",
        "blk.attn_o.w",
        "blk.ffn_down.w",
        "blk.gate_exps.w",
        "blk.up_exps.w",
        "blk.down_exps.w",
        "blk.router.w",
        "output.w",
    ] {
        assert!(sketched.contains(w) && !held.contains(w), "{w} is sketched, not held");
    }
    for p in ["tok_embd", "blk.attn_norm.g", "blk.attn_q.m", "rope.cos_hi", "output.m"] {
        assert!(held.contains(p), "{p} is held");
    }
}

#[test]
fn both_producers_serve_the_same_witness() {
    for fx in [dense_moe_v1(2), wide_v1(2)] {
        let c = class(fx);
        let params = TirParams::from_map(&c.plan, &c.fx.params).expect("complete params");
        for policy in [served(), recompute()] {
            let exec = tir_witness_capture_v1(&c.plan, &c.analysis, &params, &job(), &policy).expect("the typed backend runs");
            assert_eq!(exec, c.honest(&policy), "typed backend and reference walk serve one witness");
        }
    }
}

#[test]
fn an_honest_witness_is_accepted_by_a_seat_that_holds_no_weight_matrix() {
    for fx in [dense_moe_v1(3), wide_v1(3)] {
        let c = class(fx);
        for policy in [served(), recompute()] {
            let w = c.honest(&policy);
            let r = c.check(&policy, &w).unwrap_or_else(|f| panic!("{policy:?}: {f:?}"));
            assert_eq!(r.commit_root, w.commit_root);
            assert!(r.weight_checks > 0 && r.avoided_macs > 0);
            assert_eq!(r.positions, job().positions());
            if policy == served() {
                assert!(r.fresh_checks > 0 || c.fx.program.blocks.iter().all(|b| b.name != "dense"));
            } else {
                assert_eq!(r.fresh_checks, 0);
            }
        }
        // Another seat, another secret, the same verdict.
        let w = c.honest(&served());
        assert!(c.seat_check(0x99, &served(), None, &w).is_ok());
    }
}

#[test]
fn an_off_by_one_in_an_accumulator_is_caught_by_its_own_check() {
    let c = class(dense_moe_v1(4));
    let dense = c.block_named("dense");
    let occ = c.occurrence_of(dense);
    for (i, node) in c.matmuls(dense, is_weight).into_iter().enumerate() {
        let pos = (i as u32) % job().positions();
        let w = c.produce(&served(), &mut add_at(pos, occ, node, 0, 1));
        let f = fault(c.check(&served(), &w));
        assert_eq!((f.pos, f.occurrence, f.node), (pos, occ, Some(node)), "the lie is found at its node: {f:?}");
        assert!(matches!(f.fault, TirCheckFaultV1::Freivalds { .. }), "{f:?}");
    }
}

#[test]
fn a_changed_narrowed_output_is_caught() {
    let c = class(dense_moe_v1(5));
    let dense = c.block_named("dense");
    let occ = c.occurrence_of(dense);
    let b = &c.fx.program.blocks[dense as usize];
    // The narrowed codes the MLP's down projection reads: its activation operand is a reshape of
    // the narrowing's last clamp. (A lie in the q codes can vanish in RoPE's rounding: then the
    // claim is honest, and accepting it is right.)
    let down = *c.matmuls(dense, is_weight).last().expect("the down projection");
    let misaka_palw_tir::Ref::Node(reshape) = b.nodes[down as usize].inputs[1] else { panic!("a reshaped activation") };
    let misaka_palw_tir::Ref::Node(narrowed) = b.nodes[reshape as usize].inputs[0] else { panic!("the narrowed codes") };
    assert!(matches!(b.nodes[narrowed as usize].prim, Prim::Clamp { .. }) && b.nodes[narrowed as usize].out.dtype == DType::I16);
    let honest = c.honest(&served());
    for pos in [0, 4, 7] {
        let w = c.produce(&served(), &mut add_at(pos, occ, narrowed, 1, 1));
        assert_ne!(w, honest, "the lie reaches the served products");
        let f = fault(c.check(&served(), &w));
        assert_eq!(f.pos, pos, "{f:?}");
        assert!(
            matches!(f.fault, TirCheckFaultV1::Freivalds { .. } | TirCheckFaultV1::CommitMismatch { .. }),
            "found by the next product's check or the committed rows: {f:?}"
        );
    }
}

#[test]
fn a_swapped_routed_expert_is_caught() {
    let c = class(dense_moe_v1(6));
    let moe = c.block_named("moe");
    let occ = c.occurrence_of(moe);
    let b = &c.fx.program.blocks[moe as usize];
    let topk = b.nodes.iter().position(|n| matches!(n.prim, Prim::TopK { .. })).expect("the router's TopK") as u16;
    for pos in 0..job().positions() {
        // Swap the first selected expert for the lowest unselected one, keeping the set sorted.
        let w = c.produce(&served(), &mut |s, v| {
            if s.pos == pos && s.occurrence == occ && s.node == topk {
                let chosen: Vec<i128> = v.data.clone();
                let other = (0..4).find(|e| !chosen.contains(e)).expect("an unselected expert");
                let mut set = vec![other, chosen[1]];
                set.sort();
                v.data = set;
            }
        });
        let f = fault(c.check(&served(), &w));
        assert_eq!((f.pos, f.occurrence), (pos, occ), "{f:?}");
        assert!(matches!(f.fault, TirCheckFaultV1::Freivalds { .. } | TirCheckFaultV1::CommitMismatch { .. }), "{f:?}");
    }
}

#[test]
fn a_changed_attention_row_is_caught() {
    let c = class(dense_moe_v1(7));
    let dense = c.block_named("dense");
    let occ = c.occurrence_of(dense);
    let b = &c.fx.program.blocks[dense as usize];
    let act = c.matmuls(dense, |k| *k == TirMatMulKindV1::ActAct);
    let pv = act[1];
    // The served attention output (P·V), one lane, checked with a fresh vector.
    let w = c.produce(&served(), &mut add_at(5, occ, pv, 2, 1));
    let f = fault(c.check(&served(), &w));
    assert_eq!((f.pos, f.node), (5, Some(pv)), "{f:?}");
    assert!(matches!(f.fault, TirCheckFaultV1::Freivalds { .. }));
    // Recomputed instead: the next product reads the lie and is refused (a lie large enough to
    // survive the narrowing by 2^15 that follows; a smaller one is absorbed, and the claim is honest).
    let w = c.produce(&recompute(), &mut add_at(5, occ, pv, 2, 1 << 20));
    assert_ne!(w, c.honest(&recompute()));
    let f = fault(c.check(&recompute(), &w));
    assert_eq!(f.pos, 5, "{f:?}");
    // A key row appended to the history (committed), changed at position 2 and read at every later one.
    let krow = b.nodes.iter().position(|n| matches!(n.prim, Prim::HistAppend { .. })).expect("the key history") as u16;
    let Prim::HistAppend { .. } = b.nodes[krow as usize].prim else { unreachable!() };
    let misaka_palw_tir::Ref::Node(row) = b.nodes[krow as usize].inputs[0] else { panic!("a committed row node") };
    let w = c.produce(&served(), &mut add_at(2, occ, row, 0, 1));
    let f = fault(c.check(&served(), &w));
    assert_eq!(f.pos, 2, "{f:?}");
}

#[test]
fn a_selected_token_that_is_not_the_argmax_is_refused() {
    let c = class(dense_moe_v1(8));
    let mut w = c.honest(&served());
    let p = job().prompt.len();
    w.generated[1] = (w.generated[1] + 1) % 32;
    w.tokens[p + 1] = w.generated[1];
    let f = fault(c.check(&served(), &w));
    assert!(
        matches!(f.fault, TirCheckFaultV1::Token { .. } | TirCheckFaultV1::Freivalds { .. } | TirCheckFaultV1::CommitMismatch { .. }),
        "{f:?}"
    );
}

#[test]
fn a_witness_out_of_its_interval_or_out_of_shape_is_refused_before_any_check() {
    let c = class(dense_moe_v1(9));
    let honest = c.honest(&served());
    // An i64 accumulator of an A16 projection holds at most 2^7·2^15·K: i64::MAX is outside it.
    let mut w = honest.clone();
    w.steps[1].values[0].value.data[0] = i64::MAX as i128;
    assert!(matches!(fault(c.check(&served(), &w)).fault, TirCheckFaultV1::WitnessOutOfRange { .. }));
    let mut w = honest.clone();
    w.steps[1].values.remove(3);
    assert!(matches!(fault(c.check(&served(), &w)).fault, TirCheckFaultV1::WitnessShape(_)));
    let mut w = honest.clone();
    let extra = w.steps[1].values[0].clone();
    w.steps[1].values.push(misaka_palw_tir_sketch::witness::TirWitnessValueV1 { node: extra.node + 1, ..extra });
    assert!(matches!(fault(c.check(&served(), &w)).fault, TirCheckFaultV1::WitnessShape(_)));
    // A recomputing seat refuses a served activation product it did not ask for.
    assert!(matches!(fault(c.check(&recompute(), &honest)).fault, TirCheckFaultV1::WitnessShape(_)));
}

/// **The property**: over random lies — any position, any node, any element, deltas from ±1 to
/// wide ones — a seat accepts only a witness whose claim is the honest one, and refuses every lie
/// in a served product.
#[test]
fn a_seat_accepts_only_the_honest_claim_and_refuses_every_lie_in_a_served_product() {
    let c = class(dense_moe_v1(10));
    let honest = c.honest(&served());
    let mut rng = ChaCha8Rng::seed_from_u64(0xC0FFEE);
    let (mut changed, mut caught_served) = (0, 0);
    let mut cases = 0;
    while cases < 240 {
        let pos = rng.gen_range(0..job().positions());
        let occ = rng.gen_range(0..c.plan.occurrences.len() as u16);
        if occ as usize == c.plan.occurrences.len() - 1 && pos + 1 < job().prompt.len() as u32 {
            continue;
        }
        let block = c.plan.occurrences[occ as usize].0;
        let nodes = c.fx.program.blocks[block as usize].nodes.len();
        // One case in four lies in a served product; the rest anywhere.
        let products: Vec<u16> = c.analysis.blocks[block as usize].matmuls.iter().map(|s| s.node).collect();
        let node = if !products.is_empty() && rng.gen_range(0..4) == 0 {
            products[rng.gen_range(0..products.len())]
        } else {
            rng.gen_range(0..nodes as u16)
        };
        let elems = c.fx.program.blocks[block as usize].nodes[node as usize].out.elements_at((pos + 1) as u64) as usize;
        let elem = rng.gen_range(0..elems.max(1));
        let delta: i128 = match rng.gen_range(0..4) {
            0 => 1,
            1 => -1,
            2 => rng.gen_range(2..1000),
            _ => -(1i128 << rng.gen_range(10..30)),
        };
        let mut t = add_at(pos, occ, node, elem, delta);
        let Ok(w) = tir_witness_produce_v1(&c.plan, &c.analysis, &c.fx.params, &job(), &served(), &mut t) else {
            continue; // the lie left its dtype: the producer cannot even run it
        };
        cases += 1;
        let claim_changed = w.commit_root != honest.commit_root || w.generated != honest.generated;
        let served_lie = c.analysis.witnessed(&c.fx.program, block, node, 1, &served());
        let verdict = c.check(&served(), &w);
        if claim_changed {
            changed += 1;
            assert!(verdict.is_err(), "a changed claim is refused (pos {pos}, occ {occ}, node {node}, delta {delta})");
        }
        if served_lie && w != honest {
            caught_served += 1;
            assert!(verdict.is_err(), "a lie in a served product is refused (pos {pos}, occ {occ}, node {node})");
        }
        if verdict.is_ok() {
            assert!(
                !claim_changed && w.steps.iter().zip(&honest.steps).all(|(a, b)| a.values == b.values),
                "accepted only when nothing served changed"
            );
        }
    }
    assert!(changed > 40 && caught_served > 30, "the sample exercised both: {changed} changed claims, {caught_served} served lies");
}

/// A tamperer who KNOWS the vector builds an error orthogonal to it and passes; the same error
/// against another seat's vector is caught. Soundness is the secrecy of the vector, nothing else.
#[test]
fn a_tamperer_who_knows_the_vector_passes_and_one_who_does_not_is_caught() {
    let md = TirSketchModulusV1::P61;
    let (a, b) = (misaka_palw_tir::TensorType::fixed(DType::I8, &[6, 4]), misaka_palw_tir::TensorType::fixed(DType::I16, &[4, 1]));
    let g = TirCheckGeomV1::new(TirSideV1::Left, &a, &b, 1, 0);
    let w: Vec<i128> = (0..24).map(|i| (i * 37 % 255) - 127).collect();
    let x: Vec<i128> = (0..4).map(|i| i * 1000 - 1500).collect();
    let out: Vec<i128> = (0..6).map(|r| (0..4).map(|t| w[r * 4 + t] * x[t]).sum()).collect();
    for seat in 0..50u8 {
        let mine = TirSeatSketchSecretV1::from_bytes([seat; 32]).keys(&CLASS, 0).site_vector(1, 2, md, g.v_len());
        let theirs = TirSeatSketchSecretV1::from_bytes([seat.wrapping_add(1); 32]).keys(&CLASS, 0).site_vector(1, 2, md, g.v_len());
        // e[0] = v[1], e[1] = −v[0]: Σ v·e = 0 mod p.
        let mut forged = out.clone();
        forged[0] += mine[1] as i128;
        forged[1] -= mine[0] as i128;
        let pass = |v: &[u64]| {
            let s = g.sketch(&w, v, md);
            g.lhs(&forged, v, md) == g.rhs(&x, &mut |_| Some(&s[..]), md).unwrap()
        };
        assert!(pass(&mine), "the forger who knows v passes");
        assert!(!pass(&theirs), "the same forgery fails another seat's v");
    }
}

/// At a toy prime the `1/p` is observable: random nonzero errors pass at that rate, and no more.
/// At `P61` none of the same number passes.
#[test]
fn random_errors_pass_a_toy_prime_at_one_in_p_and_never_pass_p61() {
    let (a, b) = (misaka_palw_tir::TensorType::fixed(DType::I8, &[8, 4]), misaka_palw_tir::TensorType::fixed(DType::I16, &[4, 1]));
    let g = TirCheckGeomV1::new(TirSideV1::Left, &a, &b, 1, 0);
    let w: Vec<i128> = (0..32).map(|i| (i * 53 % 255) - 127).collect();
    let x: Vec<i128> = vec![3, -7, 11, 13];
    let out: Vec<i128> = (0..8).map(|r| (0..4).map(|t| w[r * 4 + t] * x[t]).sum()).collect();
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let trials = 20_000;
    for (md, lo, hi) in [(TirSketchModulusV1::toy(101), 140, 260), (TirSketchModulusV1::P61, 0, 0)] {
        let mut passes = 0;
        for t in 0..trials {
            let v = TirSeatSketchSecretV1::from_bytes([(t % 251) as u8; 32]).keys(&CLASS, t as u64).site_vector(0, 0, md, g.v_len());
            let s = g.sketch(&w, &v, md);
            let mut bad = out.clone();
            // A nonzero error, not a multiple of the toy prime, on random elements.
            for e in bad.iter_mut() {
                if rng.gen_bool(0.5) {
                    *e += rng.gen_range(1..100);
                }
            }
            if bad == out {
                bad[0] += 1;
            }
            if g.lhs(&bad, &v, md) == g.rhs(&x, &mut |_| Some(&s[..]), md).unwrap() {
                passes += 1;
            }
        }
        assert!(
            (lo..=hi).contains(&passes),
            "p = {}: {passes} of {trials} passed (expected {} ± 60)",
            md.p(),
            trials / md.p() as usize
        );
    }
}

#[test]
fn each_node_takes_the_moduli_its_refined_interval_needs() {
    let c = class(wide_v1(11));
    let keys = TirSeatSketchSecretV1::from_bytes([1; 32]).keys(&CLASS, 0);
    let store = TirSketchStoreV1::build(&c.plan, &c.analysis, &c.fx.params, &keys).expect("builds");
    let wide = c.block_named("wide");
    let occ = c.occurrence_of(wide);
    let mm = c.matmuls(wide, is_weight);
    assert_eq!(mm.len(), 2);
    let moduli = |n: u16| store.get(occ, n).expect("sketched").moduli.clone();
    assert_eq!(moduli(mm[0]), vec![TirSketchModulusV1::P61, TirSketchModulusV1::P64], "the i64 product of an i32-wide activation");
    assert_eq!(moduli(mm[1]), TirSketchModulusV1::LADDER_V1.to_vec(), "the i128 product of i64 operands");
    let post = c.block_named("post");
    let head = c.matmuls(post, is_weight)[0];
    assert_eq!(store.get(c.occurrence_of(post), head).unwrap().moduli, vec![TirSketchModulusV1::P61], "an i32 head");
    assert_eq!((store.stats().wide_sites, store.stats().widest_sites), (1, 1));
    // The dense + MoE class: every weight product is narrow enough for one Mersenne prime.
    let d = class(dense_moe_v1(11));
    let store = TirSketchStoreV1::build(&d.plan, &d.analysis, &d.fx.params, &keys).expect("builds");
    assert_eq!((store.stats().wide_sites, store.stats().widest_sites), (0, 0));
    assert!(store.stats().sites > 0 && store.stats().entries < store.stats().weight_elements, "{:?}", store.stats());
}

/// **The hole a missing rung leaves**: an `i64` accumulator raised by exactly `P61` (and everything
/// downstream computed from it) passes a seat that checks it over `P61` alone — a false claim
/// accepted — and is refused over the two moduli its interval needs.
#[test]
fn an_error_of_exactly_p61_passes_one_modulus_and_not_two() {
    let c = class(wide_v1(12));
    let wide = c.block_named("wide");
    let occ = c.occurrence_of(wide);
    let acc64 = c.matmuls(wide, is_weight)[0];
    let honest = c.honest(&served());
    let p61 = TirSketchModulusV1::P61.p() as i128;
    let w = c.produce(&served(), &mut add_at(3, occ, acc64, 5, p61));
    assert_ne!(w.commit_root, honest.commit_root, "the lie changes the claim");
    let r = c.seat_check(0x44, &served(), Some(&[TirSketchModulusV1::P61]), &w);
    assert!(r.is_ok(), "one rung too few: the false claim is ACCEPTED ({r:?})");
    let f = fault(c.seat_check(0x44, &served(), None, &w));
    assert_eq!((f.pos, f.node), (3, Some(acc64)));
    assert_eq!(f.fault, TirCheckFaultV1::Freivalds { modulus: TirSketchModulusV1::P64.p() }, "the second rung catches it");
}

#[test]
fn the_seat_reads_fewer_field_terms_than_a_recompute_reads_multiply_adds() {
    let c = class(dense_moe_v1(13));
    let w = c.honest(&served());
    let r = c.check(&served(), &w).expect("honest");
    // At these toy widths the margin is small; the measurement tool gives it at real ones.
    assert!(r.check_terms < r.avoided_macs * 2, "{r:?}");
    let (elements, bytes) = w.served();
    assert_eq!((elements, bytes), (r.served_elements, r.served_bytes));
}

/// A seat that recomputes the short weight products (`K < 17`: the o projection, the expert down
/// projections, the shared expert's) holds exactly those weights, is served only the long ones,
/// and still accepts the honest claim and refuses a lie in any product it was served.
#[test]
fn a_seat_that_recomputes_short_products_holds_only_their_weights() {
    let c = class(dense_moe_v1(14));
    let pol = TirCheckPolicyV1 { act_act: TirActActPolicyV1::ServedFrom { k: 4 }, weight_min_k: 17 };
    let held = c.param_names(&c.analysis.held_params(&c.fx.program, 64, &pol));
    for w in ["blk.attn_o.w", "blk.gate_exps.w", "blk.down_exps.w", "output.w"] {
        assert!(held.contains(w), "{w}: K ≤ 16, recomputed from a held weight");
    }
    assert!(!held.contains("blk.ffn_down.w"), "K = 24: served and sketched");
    let w = c.honest(&pol);
    let r = c.check(&pol, &w).expect("the honest claim");
    assert!(r.exact_macs > 0 && r.weight_checks > 0);
    let dense = c.block_named("dense");
    let down = *c.matmuls(dense, is_weight).last().expect("the down projection");
    let w = c.produce(&pol, &mut add_at(2, c.occurrence_of(dense), down, 3, 1));
    let f = fault(c.check(&pol, &w));
    assert_eq!((f.pos, f.node), (2, Some(down)));
}


// ---- §II.8: row-block sketches ------------------------------------------------------------------------------------------------

impl Class {
    /// A seat check whose store sketches every weight in `blocks` free-axis blocks.
    fn blocked_check(&self, blocks: usize, w: &TirWitnessV1) -> Result<TirCheckReportV1, Box<TirCheckFailureV1>> {
        let keys = TirSeatSketchSecretV1::from_bytes([0x33; 32]).keys(&CLASS, 1);
        let store = TirSketchStoreV1::build_blocked(&self.plan, &self.analysis, &self.fx.params, &keys, blocks).expect("the store builds");
        let held = self.held(&served());
        TirSketchCheckerV1::new(&self.plan, &self.analysis, &store, &keys, &held, served()).expect("a checker").check(&job(), &JOB_ID, w)
    }
}

/// The blocks sum to the whole sketch, on every geometry the fixtures have (static left and right, routed), at every modulus.
#[test]
fn the_row_block_sketches_sum_to_the_whole_sketch() {
    for fx in [dense_moe_v1(4), wide_v1(4)] {
        let c = class(fx);
        let keys = TirSeatSketchSecretV1::from_bytes([0x33; 32]).keys(&CLASS, 1);
        let one = TirSketchStoreV1::build_blocked(&c.plan, &c.analysis, &c.fx.params, &keys, 1).unwrap();
        let many = TirSketchStoreV1::build_blocked(&c.plan, &c.analysis, &c.fx.params, &keys, 3).unwrap();
        let mut seen = 0;
        for (occ, &(block, _)) in c.plan.occurrences.iter().enumerate() {
            for site in &c.analysis.blocks[block as usize].matmuls {
                let (Some(a), Some(b)) = (one.get(occ as u16, site.node), many.get(occ as u16, site.node)) else { continue };
                assert_eq!(a.s, b.s, "the whole sketch is the sum of its blocks (occurrence {occ}, node {})", site.node);
                assert!(b.blocks >= 1 && (b.blocks == 1 || b.s_blocks.iter().all(|m| m.len() == b.blocks)));
                seen += 1;
            }
        }
        assert!(seen > 0);
    }
}

/// An honest witness passes at any block count; a lie in one accumulator is refused, and the failure names the blocks whose own check fails.
#[test]
fn a_failed_check_names_the_failing_blocks_and_an_honest_witness_passes_at_any_block_count() {
    let c = class(dense_moe_v1(4));
    let dense = c.block_named("dense");
    let occ = c.occurrence_of(dense);
    let honest = c.honest(&served());
    for blocks in [1usize, 2, 3, 8] {
        assert!(c.blocked_check(blocks, &honest).is_ok(), "{blocks} blocks: an honest witness passes");
    }
    for node in c.matmuls(dense, is_weight) {
        let w = c.produce(&served(), &mut add_at(0, occ, node, 0, 1));
        for blocks in [1usize, 2, 3, 8] {
            let f = fault(c.blocked_check(blocks, &w));
            assert_eq!((f.pos, f.occurrence, f.node), (0, occ, Some(node)), "{f:?}");
            assert!(matches!(f.fault, TirCheckFaultV1::Freivalds { .. }), "{f:?}");
            assert!(!f.blocks.is_empty(), "{blocks} blocks: the failure names a block: {f:?}");
            assert!(f.blocks.iter().all(|b| (*b as usize) < blocks.max(1)), "{f:?}");
        }
    }
}

/// The count and the transfer bound: `ceil(bytes / F)` blocks, each standing for at most `F` bytes; the RFC's worked head.
#[test]
fn the_block_count_and_the_fetch_bound() {
    assert_eq!(tir_block_count_v1(0), 1);
    assert_eq!(tir_block_count_v1(TIR_BLOCK_FETCH_CAP_BYTES_V1), 1);
    assert_eq!(tir_block_count_v1(TIR_BLOCK_FETCH_CAP_BYTES_V1 + 1), 2);
    // Qwen2.5-1.5B's head: 151,936 rows of 1,536 i16 weights = 466.8 MB: 223 blocks of at most 2 MiB (the RFC's B = 256 is the nearest power).
    let head = 151_936u64 * 1_536 * 2;
    let b = tir_block_count_v1(head) as u64;
    assert!(head.div_ceil(b) <= TIR_BLOCK_FETCH_CAP_BYTES_V1, "no block exceeds the cap");
    assert!(b * 1_536 * 8 < 3_300_000, "the extra sketch is about 3 MB ({} B)", b * 1_536 * 8);
}
