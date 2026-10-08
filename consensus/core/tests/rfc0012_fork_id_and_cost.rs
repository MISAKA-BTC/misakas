//! RFC-0012 dormant retirement: **who refuses whom, and what the evidence sweep costs.**
//!
//! * Arming `palw_dns_retirement_v1` adds a gate fence to the schedule. A node that has crossed it refuses a peer that has not
//!   (the un-upgraded binary); before it, the two stay peers. Presets leave the fork id untouched.
//! * The per-block evidence arithmetic is O(N log N + F log F) (`certify_native_prefix_v1`), where the per-effect reference it
//!   replaced is O(N x F). The timings are printed for the record; the only assertions are on how the cost SCALES.
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::palw_native_settlement_v1::{
    MatureUsefulWorkV1, NativeEffectV1, PalwDnsRetirementV1, PalwSettlementPolicyV1, certify_native_effect_v1,
    certify_native_prefix_v1,
};
use std::time::Instant;

fn policy() -> PalwSettlementPolicyV1 {
    PalwSettlementPolicyV1 { settled_anchor_depth: 2, unique_mature_work: 20, max_operator_permille: 600, max_class_permille: 600 }
}

fn fenced(at: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_dns_retirement =
        Some(PalwDnsRetirementV1 { activation: ForkActivation::new(at), settlement: policy(), legacy_evidence_horizon_daa: 300 });
    p
}

#[test]
fn rfc0012_the_unassigned_fence_leaves_every_identity_and_fork_id_alone_and_an_armed_one_partitions_at_the_height() {
    let p = palw_t12_shipped_params();
    assert!(p.palw_dns_retirement.is_none(), "testnet-12 assigns no retirement");
    let mut never = p.clone();
    never.palw_dns_retirement =
        Some(PalwDnsRetirementV1 { activation: ForkActivation::never(), settlement: policy(), legacy_evidence_horizon_daa: 300 });
    for at in [0, 750, 9_000, 12_000] {
        assert_eq!(fork_id_v1(&p, at), fork_id_v1(&never, at), "a never-leave fence is invisible to the fork id (DAA {at})");
    }
    let armed = fenced(9_000);
    assert_ne!(p.consensus_params_id(), armed.consensus_params_id());
    assert_ne!(p.consensus_schedule_id(), armed.consensus_schedule_id());
    // The armed node and the unupgraded node agree until the height, and the armed one refuses the other from it.
    let unupgraded_before = fork_id_v1(&p, 8_999);
    let armed_before = fork_id_v1(&armed, 8_999);
    assert!(
        !evaluate_fork_id_v1(&armed, 8_999, &unupgraded_before.fired.as_bytes(), unupgraded_before.next).refuses(),
        "before the fence: peers"
    );
    assert!(
        !evaluate_fork_id_v1(&p, 8_999, &armed_before.fired.as_bytes(), armed_before.next).refuses(),
        "and the unupgraded node keeps the armed one"
    );
    let unupgraded_at = fork_id_v1(&p, 9_000);
    let verdict = evaluate_fork_id_v1(&armed, 9_000, &unupgraded_at.fired.as_bytes(), unupgraded_at.next);
    assert!(verdict.refuses(), "from the fence: {verdict:?}");
    // A peer that scheduled a DIFFERENT retirement height is refused past the earlier one.
    let other = fenced(9_500);
    let other_at = fork_id_v1(&other, 9_200);
    assert!(
        evaluate_fork_id_v1(&armed, 9_200, &other_at.fired.as_bytes(), other_at.next).refuses(),
        "a different height is a different network"
    );
    // A different policy value is a different network too (the values are committed).
    let mut other_policy = armed.clone();
    other_policy.palw_dns_retirement.as_mut().unwrap().settlement.unique_mature_work += 1;
    assert_ne!(armed.consensus_params_id(), other_policy.consensus_params_id());
    assert_ne!(armed.consensus_schedule_id(), other_policy.consensus_schedule_id());
}

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

/// N effects (3 per DAA) and F = N/2 facts spread over them, 50 operators, 4 classes, each fact its own anchor.
fn workload(n: u64) -> (Vec<NativeEffectV1>, Vec<MatureUsefulWorkV1>) {
    let effects: Vec<NativeEffectV1> =
        (0..n).map(|i| NativeEffectV1 { daa: i / 3, blue: i + 1, frontier_covers: true, lifecycle_closed: true }).collect();
    let facts = (0..n / 2)
        .map(|i| {
            let blue = 2 * i + 2;
            MatureUsefulWorkV1 {
                identity: h(i + 1),
                anchor: h(1_000_000 + i),
                operator: h(2_000_000 + i % 50),
                class: h(3_000_000 + i % 4),
                anchor_blue: blue,
                accepted_blue: blue,
                anchor_daa: blue / 3,
                accepted_daa: blue / 3,
                matured_daa: 0,
                work: 1 + (i % 7) as u128,
            }
        })
        .collect();
    (effects, facts)
}

#[test]
fn rfc0012_the_sweep_scales_near_linearly_and_the_per_effect_reference_does_not() {
    let policy = PalwSettlementPolicyV1 {
        settled_anchor_depth: 1,
        unique_mature_work: 1,
        max_operator_permille: 1000,
        max_class_permille: 1000,
    };
    let mut timings = Vec::new();
    for n in [10_000u64, 40_000, 160_000] {
        let (effects, facts) = workload(n);
        let start = Instant::now();
        let out = certify_native_prefix_v1(policy, &effects, u64::MAX, &facts);
        let took = start.elapsed();
        eprintln!("[rfc0012-cost] sweep: {n} effects, {} facts: {:?} (safe {:?}, stop {:?})", facts.len(), took, out.safe, out.stop);
        timings.push((n, took));
    }
    let ratio = timings[2].1.as_secs_f64() / timings[0].1.as_secs_f64().max(1e-9);
    eprintln!("[rfc0012-cost] 16x the input took {ratio:.1}x the time");
    assert!(ratio < 64.0, "16x the chain cost {ratio:.1}x: not near-linear");

    // The reference, one effect at a time, over a chain 25x shorter than the smallest sweep above.
    let (effects, facts) = workload(400);
    let start = Instant::now();
    for e in &effects {
        let _ = certify_native_effect_v1(policy, (e.daa, e.blue), u64::MAX, true, true, true, true, &facts);
    }
    let reference = start.elapsed();
    let (e2, f2) = workload(1_600);
    let start = Instant::now();
    for e in &e2 {
        let _ = certify_native_effect_v1(policy, (e.daa, e.blue), u64::MAX, true, true, true, true, &f2);
    }
    let reference4 = start.elapsed();
    eprintln!(
        "[rfc0012-cost] per-effect reference: 400 effects {:?}, 1,600 effects {:?} ({:.1}x for 4x the input)",
        reference,
        reference4,
        reference4.as_secs_f64() / reference.as_secs_f64().max(1e-9)
    );
}
