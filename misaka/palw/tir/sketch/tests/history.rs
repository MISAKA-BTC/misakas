//! **Per-row history sketches** (RFC-0007 Part II §II.3, `misaka_palw_tir_sketch::history`): an honest `P·V` and `Q·Kᵀ` pass, a tampered
//! one fails (and passes a toy prime exactly `1/p` of the time), a sliding window evicts exactly, and the per-row sketch equals the
//! window-wide one it replaces.
//!
//! Run: `cargo test -p misaka-palw-tir-sketch --test history`

use misaka_palw_tir_sketch::{TirHistorySketchV1, TirSeatSketchSecretV1, TirSketchModulusV1};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const D: usize = 16;

fn rows(rng: &mut ChaCha8Rng, n: usize, lim: i64) -> Vec<Vec<i64>> {
    (0..n).map(|_| (0..D).map(|_| rng.gen_range(-lim..=lim)).collect()).collect()
}

fn pv(p: &[i64], v: &[Vec<i64>]) -> Vec<i64> {
    (0..D).map(|d| p.iter().zip(v).map(|(w, row)| w * row[d]).sum()).collect()
}

fn qk(q: &[i64], k: &[Vec<i64>]) -> Vec<i64> {
    k.iter().map(|row| q.iter().zip(row).map(|(a, b)| a * b).sum()).collect()
}

fn sketch(secret: u8, m: TirSketchModulusV1) -> (TirHistorySketchV1, misaka_palw_tir_sketch::TirSketchKeysV1) {
    let keys = TirSeatSketchSecretV1::from_bytes([secret; 32]).keys(&[3u8; 64], 7);
    let h = TirHistorySketchV1::new(&keys, &[9u8; 32], 2, 5, m, D);
    (h, keys)
}

#[test]
fn an_honest_attention_step_passes_and_a_changed_value_does_not() {
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    for round in 0..40 {
        let n = 1 + (round % 23);
        let (k, v) = (rows(&mut rng, n, 3_000), rows(&mut rng, n, 3_000));
        let (mut h, keys) = sketch(round as u8 + 1, TirSketchModulusV1::P61);
        for (kr, vr) in k.iter().zip(&v) {
            h.append(&keys, kr, vr);
        }
        assert_eq!(h.retained(), n);
        let p: Vec<i64> = (0..n).map(|_| rng.gen_range(-200..=200)).collect();
        let q: Vec<i64> = (0..D).map(|_| rng.gen_range(-3_000..=3_000)).collect();
        let (out, scores) = (pv(&p, &v), qk(&q, &k));
        assert!(h.check_pv(&p, &out), "honest P·V (round {round})");
        assert!(h.check_qk(&q, &scores), "honest Q·Kᵀ (round {round})");
        // One lane off by one: caught.
        let mut bad = out.clone();
        bad[round % D] += 1;
        assert!(!h.check_pv(&p, &bad), "a P·V off by one in one lane");
        let mut bad = scores.clone();
        bad[round % n] -= 1;
        assert!(!h.check_qk(&q, &bad), "a score off by one");
        // A shape that is not the retained window's.
        assert!(!h.check_pv(&p[..n - 1], &out), "a shape that is not the retained window's");
        assert!(!h.check_qk(&q[..D - 1], &scores));
    }
}

/// The error is the PROMISED one: an error orthogonal to the secret vector passes (so the secret is the whole defence), and over a
/// toy prime a random error passes about `1/p` of the time.
#[test]
fn soundness_is_the_secrecy_of_the_vector_and_the_rate_is_one_over_p() {
    let p_toy = 101u64;
    let m = TirSketchModulusV1::toy(p_toy);
    let mut rng = ChaCha8Rng::seed_from_u64(5);
    let n = 6;
    let (k, v) = (rows(&mut rng, n, 40), rows(&mut rng, n, 40));
    let p: Vec<i64> = (0..n).map(|_| rng.gen_range(-9..=9)).collect();
    let out = pv(&p, &v);
    let trials = 20_000;
    let mut passed = 0;
    for t in 0..trials {
        // A fresh secret each trial: the same tampered output meets a new σ.
        let keys2 = TirSeatSketchSecretV1::from_bytes([(t / 250) as u8 + 1; 32]).keys(&[(t % 7) as u8; 64], t as u64);
        let mut h = TirHistorySketchV1::new(&keys2, &[(t % 255) as u8; 32], t as u16, 5, m, D);
        for (kr, vr) in k.iter().zip(&v) {
            h.append(&keys2, kr, vr);
        }
        let mut bad = out.clone();
        bad[0] += 1 + (t as i64 % 50);
        if h.check_pv(&p, &bad) {
            passed += 1;
        }
    }
    // 1/p of 20,000 is 198; the bound is generous at both ends.
    assert!((130..=270).contains(&passed), "a toy-prime check passed {passed} of {trials} tampered outputs (expected ≈ {})", trials / p_toy as usize);
    // At P61 none passes.
    let (mut h, keys) = sketch(1, TirSketchModulusV1::P61);
    for (kr, vr) in k.iter().zip(&v) {
        h.append(&keys, kr, vr);
    }
    let mut bad = out.clone();
    bad[0] += 1;
    assert!(!h.check_pv(&p, &bad));
}

/// **The per-row sketch is the window-wide sketch**: `Σ_h p[h]·S_V[h]` equals `Σ_d σ[d]·(Σ_h p[h]·V_h[d])` — the identity the saving
/// rests on — and a sliding window that evicts its oldest row equals a fresh sketch of the rows that remain.
#[test]
fn a_sliding_window_equals_a_fresh_sketch_of_the_window() {
    let mut rng = ChaCha8Rng::seed_from_u64(9);
    let total = 14;
    let window = 5;
    let (k, v) = (rows(&mut rng, total, 2_000), rows(&mut rng, total, 2_000));
    let (mut sliding, keys) = sketch(4, TirSketchModulusV1::P61);
    for (i, (kr, vr)) in k.iter().zip(&v).enumerate() {
        sliding.append(&keys, kr, vr);
        if sliding.retained() > window {
            assert!(sliding.evict_oldest(&k[i - window]));
        }
        let lo = (i + 1).saturating_sub(window);
        let (kw, vw) = (&k[lo..=i], &v[lo..=i]);
        let p: Vec<i64> = (0..kw.len()).map(|_| rng.gen_range(-50..=50)).collect();
        let q: Vec<i64> = (0..D).map(|_| rng.gen_range(-2_000..=2_000)).collect();
        assert!(sliding.check_pv(&p, &pv(&p, vw)), "P·V over the window at {i}");
        assert!(sliding.check_qk(&q, &qk(&q, kw)), "Q·Kᵀ over the window at {i}");
        let mut bad = qk(&q, kw);
        bad[0] += 1;
        assert!(!sliding.check_qk(&q, &bad), "a tampered score over the window at {i}");
    }
    assert_eq!(sliding.retained(), window);
    assert_eq!(sliding.next_pos(), total as u32, "positions are absolute");
    assert!(!{
        let (mut empty, _) = sketch(1, TirSketchModulusV1::P61);
        empty.evict_oldest(&k[0])
    }, "nothing to evict");
}

/// Wide moduli: the sketch works over every rung of the ladder.
#[test]
fn every_rung_of_the_ladder_checks() {
    let mut rng = ChaCha8Rng::seed_from_u64(11);
    let n = 7;
    let (k, v) = (rows(&mut rng, n, 1 << 20), rows(&mut rng, n, 1 << 20));
    for m in TirSketchModulusV1::LADDER_V1 {
        let (mut h, keys) = sketch(8, m);
        for (kr, vr) in k.iter().zip(&v) {
            h.append(&keys, kr, vr);
        }
        let p: Vec<i64> = (0..n).map(|_| rng.gen_range(-1_000..=1_000)).collect();
        let q: Vec<i64> = (0..D).map(|_| rng.gen_range(-(1 << 20)..=1 << 20)).collect();
        assert!(h.check_pv(&p, &pv(&p, &v)), "P·V over {m:?}");
        assert!(h.check_qk(&q, &qk(&q, &k)), "Q·Kᵀ over {m:?}");
        let mut bad = qk(&q, &k);
        bad[3] += 1;
        assert!(!h.check_qk(&q, &bad), "a tampered score over {m:?}");
    }
}

#[test]
fn nothing_prints_a_sketch_value() {
    let (mut h, keys) = sketch(0xAB, TirSketchModulusV1::P61);
    h.append(&keys, &[1; D], &[2; D]);
    let text = format!("{h:?}");
    assert_eq!(text, format!("TirHistorySketchV1(1 rows of {D}, ..)"));
}
