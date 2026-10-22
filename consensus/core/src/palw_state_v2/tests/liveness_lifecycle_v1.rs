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
    eprintln!(
        "growth N {n0}->{n2} ({}x): per-object x{:.1}, empty fold x{:.1}",
        n2 / n0,
        o2 / o0.max(1.0),
        f2 / f0.max(1.0)
    );
    assert!(o2 / o0.max(1.0) > 10.0, "per-object rehearsal cost grows linearly with the state's rows");
}
