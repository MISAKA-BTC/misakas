//! **Liveness audit (lane lifecycle, 2026-10-22) — measurement, not a fix.** How the per-object acceptance rehearsal and the
//! per-block fold scale with the number of rows the PALW chain state holds.
//!
//! `palw_v2_apply_one_object_retained_v1` is what the block-acceptance filter (`palw_accept.rs`, past `palw_audit_2026_09_11`), the
//! mempool's H-1 gate and the template's re-check call ONCE PER CARRIED OBJECT; `TransitionBuilder::try_new` copies the whole parent
//! state (`NativeTransitionReservationV1::copy_parent_v1` → `NativeRetainedV1::try_from_clone_v1` + `value_requests_v1`) before the
//! object is even looked at. The object used here is refused at its first lookup (`ModelLineRetired` of a line that does not exist),
//! so everything measured is the fixed per-object overhead an attacker's refused object costs the node.
//!
//! Run: `cargo test --release -p kaspa-consensus-core liveness_lifecycle_v1 -- --nocapture`
use super::*;
use std::time::Instant;

fn params() -> PalwStateParamsV2 {
    match crate::config::params::palw_t12_shipped_params().palw_consensus_mode {
        crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.state,
        _ => unreachable!(),
    }
}

fn world(n: u64) -> PalwChainStateV2 {
    let mut s = PalwChainStateV2::genesis();
    let class = Hash64::from_u64_word(0xC1A5_0001);
    for i in 0..n {
        let mut c = palw_claim_template_v1(
            class,
            PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(9 + (i % 64)), 0)),
            1_000 + i,
            0,
            1_000,
        );
        c.pwu = 900;
        // Terminal rows awaiting retirement: no deadline, no index entry, nothing the sweeps would touch — only the full-state
        // copy and the frontier's `claims.iter()` see them.
        c.phase = PalwClaimPhaseV2::Final { final_daa: 1_000 + i };
        c.accepted_blue_score = 1_000 + i;
        c.accepted_block = Hash64::from_u64_word(0xB10C_0000 + i);
        s.set_claim_for_tests(Hash64::from_u64_word(0x5EC0_0000 + i), c);
    }
    s
}

fn time<T>(reps: u32, mut f: impl FnMut() -> T) -> f64 {
    let start = Instant::now();
    for _ in 0..reps {
        std::hint::black_box(f());
    }
    start.elapsed().as_secs_f64() * 1e6 / reps as f64
}

#[test]
fn liveness_lifecycle_v1_per_object_rehearsal_and_empty_fold_scale_with_state_rows() {
    let params = params();
    eprintln!(
        "t12 shipped: claim_retirement_daa={} window_court={} window_bind={} epoch_length={}",
        params.claim_retirement_daa, params.window_court, params.window_bind, params.epoch_length
    );
    let object = PalwConsensusObjectV2::ModelLineRetired { line_id: Hash64::from_u64_word(0xDEAD), signature: vec![] };
    let extras = PalwTransitionExtrasV1::default();
    let mut rows = Vec::new();
    for n in [1_000u64, 10_000, 50_000] {
        let state = world(n);
        let ctx = PalwBlockContextV2 { block: Hash64::from_u64_word(0xB10C), daa_score: 1_000_000, blue_score: 1_000_000, subsidy: 0 };
        // The refused object: proves the work happened before the refusal.
        let refused = palw_v2_apply_one_object_retained_v1(&state, &params, &ctx, &object, false, false, false, false, &extras);
        assert!(refused.is_err(), "the probe object must be refused (it names no line)");
        let reps = if n >= 50_000 { 5 } else { 20 };
        let per_object_us = time(reps, || {
            palw_v2_apply_one_object_retained_v1(&state, &params, &ctx, &object, false, false, false, false, &extras).is_err()
        });
        let empty_fold_us = time(reps, || apply_palw_transition_v2(&state, &params, &ctx, &[], None).map(|_| ()));
        let clone_us = time(reps, || state.clone());
        eprintln!(
            "N={n:>6} claims: per-object rehearsal (refused) {per_object_us:>10.0} us | plain clone {clone_us:>10.0} us | empty-block fold {empty_fold_us:>10.0} us"
        );
        rows.push((n, per_object_us, empty_fold_us));
    }
    // Growth order: 50x the rows must cost well over 10x per object (linear in |state|), i.e. the per-object cost is NOT bounded
    // independently of the state size.
    let (n0, o0, f0) = rows[0];
    let (n2, o2, f2) = rows[2];
    eprintln!("growth N {n0}->{n2} ({}x): per-object x{:.1}, empty fold x{:.1}", n2 / n0, o2 / o0.max(1.0), f2 / f0.max(1.0));
    assert!(o2 / o0.max(1.0) > 10.0, "per-object rehearsal cost grows linearly with the state's rows");
}

// ---------------------------------------------------------------------------------------------------------------------------------
// studio/t3-liveness (2026-10-22, measurement only): the same three probes at 1k / 10k / 50k / 200k / 1M claim rows, with a separate
// warm-up and median / p95 / max over repeated runs, for the growth order (L-LC-01/02). `#[ignore]` so it never rides a normal
// suite run: `cargo test --release -p kaspa-consensus-core --lib studio_t3_lifecycle -- --ignored --nocapture --test-threads=1`.
// `STUDIO_T3_SIZES=1000,10000` and `STUDIO_T3_RUNS=20` override the sizes and the run count.
// ---------------------------------------------------------------------------------------------------------------------------------

fn studio_stats(mut v: Vec<f64>) -> (f64, f64, f64) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = if v.len() % 2 == 1 { v[v.len() / 2] } else { (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2.0 };
    // Nearest-rank p95.
    let rank = ((0.95 * v.len() as f64).ceil() as usize).clamp(1, v.len());
    (med, v[rank - 1], *v.last().unwrap())
}

fn studio_samples<T>(warmup: u32, runs: u32, mut f: impl FnMut() -> T) -> Vec<f64> {
    for _ in 0..warmup {
        std::hint::black_box(f());
    }
    (0..runs)
        .map(|_| {
            let start = Instant::now();
            std::hint::black_box(f());
            start.elapsed().as_secs_f64() * 1e6
        })
        .collect()
}

#[test]
#[ignore = "studio/t3-liveness measurement; run explicitly with --ignored --release"]
fn studio_t3_lifecycle_rehearsal_clone_fold_vs_claim_rows() {
    let params = params();
    let object = PalwConsensusObjectV2::ModelLineRetired { line_id: Hash64::from_u64_word(0xDEAD), signature: vec![] };
    let extras = PalwTransitionExtrasV1::default();
    let sizes: Vec<u64> = std::env::var("STUDIO_T3_SIZES")
        .ok()
        .map(|s| s.split(',').map(|x| x.trim().parse().unwrap()).collect())
        .unwrap_or_else(|| vec![262, 1_000, 10_000, 50_000, 200_000, 1_000_000]);
    let runs_override: Option<u32> = std::env::var("STUDIO_T3_RUNS").ok().map(|s| s.parse().unwrap());
    let ctx = PalwBlockContextV2 { block: Hash64::from_u64_word(0xB10C), daa_score: 1_000_000, blue_score: 1_000_000, subsidy: 0 };
    let mut rows = Vec::new();
    for n in sizes {
        let build = Instant::now();
        let state = world(n);
        let build_ms = build.elapsed().as_secs_f64() * 1e3;
        let refused = palw_v2_apply_one_object_retained_v1(&state, &params, &ctx, &object, false, false, false, false, &extras);
        assert!(refused.is_err(), "the probe object must be refused (it names no line)");
        let runs = runs_override.unwrap_or(if n >= 1_000_000 { 10 } else { 20 });
        let warmup = 2;
        let per_object = studio_stats(studio_samples(warmup, runs, || {
            palw_v2_apply_one_object_retained_v1(&state, &params, &ctx, &object, false, false, false, false, &extras).is_err()
        }));
        let clone = studio_stats(studio_samples(warmup, runs, || state.clone()));
        let fold =
            studio_stats(studio_samples(warmup, runs, || apply_palw_transition_v2(&state, &params, &ctx, &[], None).map(|_| ())));
        println!(
            "STUDIO_T3_LC N={n} runs={runs} warmup={warmup} build_ms={build_ms:.1} \
             per_object_us(med/p95/max)={:.1}/{:.1}/{:.1} clone_us={:.1}/{:.1}/{:.1} empty_fold_us={:.1}/{:.1}/{:.1}",
            per_object.0, per_object.1, per_object.2, clone.0, clone.1, clone.2, fold.0, fold.1, fold.2
        );
        rows.push((n as f64, per_object.0, clone.0, fold.0));
        drop(state);
    }
    // Growth order: least-squares slope of log(median) on log(N) per probe over N >= 1k (1.0 = linear); N=262 is t12 today.
    for (name, pick) in [("per_object", 0usize), ("clone", 1), ("empty_fold", 2)] {
        let pts: Vec<(f64, f64)> =
            rows.iter().filter(|r| r.0 >= 1_000.0).map(|r| (r.0.ln(), [r.1, r.2, r.3][pick].max(1e-3).ln())).collect();
        let k = pts.len() as f64;
        let (sx, sy) = pts.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
        let (mx, my) = (sx / k, sy / k);
        let num: f64 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
        let den: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
        println!("STUDIO_T3_LC_SLOPE {name} log-log slope = {:.3}", num / den.max(1e-12));
    }
}
