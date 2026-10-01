//! **RFC-0007 Part IV.1: random leaf audits from openings alone, on a fixture claim.**
//!
//! An auditor holds no model. It names a committed tile, recomputes it with the court's demand
//! evaluator from openings — committed rows of the claim, artifact rows, tokens — and compares.
//! What is held, and what is measured:
//!
//! * an honest claim passes every audit of every tile (no false alarm);
//! * a claim whose producer fabricated a fraction `q` of its tiles (and computed everything
//!   downstream honestly from them — the strongest fabricator) is caught by `m` uniform audits at
//!   the rate `1 − (1 − q)^m`, within binomial bounds;
//! * a single lied tile is found by about `m/N` of `m`-audit rounds;
//! * the bytes an audit opens (printed with `--nocapture`).

use misaka_palw_tir::{DType, Tensor};
use misaka_palw_tir_exec::TirPlan;
use misaka_palw_tir_sketch::audit::{TirAuditTallyV1, TirAuditTileV1, tir_audit_tiles_v1, tir_leaf_audit_v1};
use misaka_palw_tir_sketch::fixture::{TirSketchFixtureV1, dense_moe_v1};
use misaka_palw_tir_sketch::*;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const TILE: usize = 8;

fn job() -> TirSketchJobV1 {
    TirSketchJobV1 { prompt: vec![3, 17, 5, 29, 11], decode: 4 }
}

fn policy() -> TirCheckPolicyV1 {
    TirCheckPolicyV1 { act_act: TirActActPolicyV1::Recompute, weight_min_k: 0 }
}

struct Claim {
    fx: TirSketchFixtureV1,
    plan: TirPlan,
    analysis: TirSketchAnalysisV1,
}

fn claim(seed: u64) -> Claim {
    let fx = dense_moe_v1(seed);
    let plan = TirPlan::compile(&fx.program).expect("a program");
    let analysis = TirSketchAnalysisV1::of(&fx.program);
    Claim { fx, plan, analysis }
}

impl Claim {
    fn produce(&self, tamper: &mut dyn FnMut(&TirTamperSiteV1, &mut Tensor)) -> TirWitnessV1 {
        tir_witness_produce_v1(&self.plan, &self.analysis, &self.fx.params, &job(), &policy(), tamper).expect("the producer runs")
    }

    /// Whether `tile` of `w` is a tile whose committed values differ from the honest ones AT ITS
    /// OWN NODE — the tiles a fabricator wrote rather than computed.
    fn audit(&self, w: &TirWitnessV1, t: TirAuditTileV1, tally: &mut TirAuditTallyV1) -> bool {
        tir_leaf_audit_v1(&self.plan, w, &self.fx.params, t, TILE, tally).expect("an audit runs")
    }
}

/// A fabricator: every committed node of the `(pos, occurrence)` units in `units` gets a small
/// random change (inside its dtype), and the producer computes on from it.
fn fabricate<'a>(c: &'a Claim, units: &'a [(u32, u16)], seed: u64) -> impl FnMut(&TirTamperSiteV1, &mut Tensor) + 'a {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    move |s, v| {
        let node = &c.fx.program.blocks[s.block as usize].nodes[s.node as usize];
        if !node.commit || v.dtype == DType::Idx || !units.contains(&(s.pos, s.occurrence)) {
            return;
        }
        for x in v.data.iter_mut() {
            let d: i128 = rng.gen_range(1..50) * if rng.gen_bool(0.5) { 1 } else { -1 };
            *x = (*x + d).clamp(-32767, 32767);
        }
    }
}

#[test]
fn an_honest_claim_passes_every_leaf_audit_and_an_audit_opens_little() {
    let c = claim(21);
    let w = c.produce(&mut |_, _| {});
    let tiles = tir_audit_tiles_v1(&c.plan, &w, TILE);
    assert!(tiles.len() > 100, "{} tiles", tiles.len());
    let (mut total, mut most, mut openings) = (0u64, 0u64, 0u64);
    for t in &tiles {
        let mut tally = TirAuditTallyV1::default();
        assert!(c.audit(&w, *t, &mut tally), "an honest tile passes: {t:?}");
        let b = tally.value_bytes(TILE);
        total += b;
        most = most.max(b);
        openings += tally.openings();
    }
    let n = tiles.len() as u64;
    eprintln!(
        "leaf audits of an honest fixture claim: {n} tiles; per audit {} bytes of opened values on average, {most} at most, {:.1} openings (Merkle paths) on average",
        total / n,
        openings as f64 / n as f64
    );
}

/// `m` uniform audits find a fabrication of a fraction `q` of the tiles at `1 − (1 − q)^m`.
#[test]
fn random_audits_catch_a_fabricated_fraction_at_one_minus_one_minus_q_to_the_m() {
    let c = claim(22);
    let honest = c.produce(&mut |_, _| {});
    let units: Vec<(u32, u16)> =
        (0..job().positions()).flat_map(|p| (0..c.plan.occurrences.len() as u16).map(move |o| (p, o))).collect();
    let mut rng = ChaCha8Rng::seed_from_u64(5);
    for q_units in [0.1f64, 0.3] {
        let mut chosen = units.clone();
        chosen.shuffle(&mut rng);
        chosen.truncate(((units.len() as f64) * q_units).round() as usize);
        let w = c.produce(&mut fabricate(&c, &chosen, 9));
        let tiles = tir_audit_tiles_v1(&c.plan, &w, TILE);
        // The tiles the fabricator wrote: those of the chosen units' committed nodes.
        let lied: Vec<bool> = tiles.iter().map(|t| !c.audit(&w, *t, &mut TirAuditTallyV1::default())).collect();
        let q = lied.iter().filter(|x| **x).count() as f64 / tiles.len() as f64;
        assert!(q > 0.0 && w != honest);
        for m in [1usize, 4, 16] {
            let trials = 400;
            let mut caught = 0;
            for _ in 0..trials {
                if (0..m).any(|_| lied[rng.gen_range(0..tiles.len())]) {
                    caught += 1;
                }
            }
            let expect = 1.0 - (1.0 - q).powi(m as i32);
            let rate = caught as f64 / trials as f64;
            let sd = (expect * (1.0 - expect) / trials as f64).sqrt();
            eprintln!(
                "fabricated {:.1} % of tiles, {m} audits: caught {:.1} % (1 − (1 − q)^m = {:.1} %)",
                100.0 * q,
                100.0 * rate,
                100.0 * expect
            );
            assert!((rate - expect).abs() <= 5.0 * sd + 0.01, "q {q:.3}, m {m}: {rate:.3} against {expect:.3}");
        }
    }
}

/// One lied tile among `N`: an `m`-audit round finds it about `m/N` of the time — the reason the mesh
/// is a sensor and not the floor.
#[test]
fn a_single_lied_tile_is_found_at_about_m_over_n() {
    let c = claim(23);
    let honest = c.produce(&mut |_, _| {});
    let tiles = tir_audit_tiles_v1(&c.plan, &honest, TILE);
    // Lie in one committed tile of the dense layer's attention residual at the last position.
    let target = *tiles.iter().rfind(|t| t.occurrence == 1 && t.pos == job().positions() - 1).expect("a tile");
    let w = c.produce(&mut |s, v| {
        if s.pos == target.pos && s.occurrence == target.occurrence && s.node == target.node {
            let i = target.tile * TILE;
            v.data[i] = (v.data[i] + 7).clamp(-32767, 32767);
        }
    });
    assert!(!c.audit(&w, target, &mut TirAuditTallyV1::default()), "the audit of that tile finds it");
    let mut rng = ChaCha8Rng::seed_from_u64(77);
    let (m, trials) = (8usize, 2_000);
    let caught = (0..trials).filter(|_| (0..m).any(|_| tiles[rng.gen_range(0..tiles.len())] == target)).count();
    let expect = 1.0 - (1.0 - 1.0 / tiles.len() as f64).powi(m as i32);
    eprintln!(
        "one lied tile of {}: {m} audits caught it {:.2} % of the time (≈ m/N = {:.2} %)",
        tiles.len(),
        100.0 * caught as f64 / trials as f64,
        100.0 * expect
    );
    assert!((caught as f64 / trials as f64 - expect).abs() < 0.03);
}
