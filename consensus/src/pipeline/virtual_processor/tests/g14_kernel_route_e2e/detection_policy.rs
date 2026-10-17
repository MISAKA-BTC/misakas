//! **G14 detection policy on the real node** (`palw_detection_policy_v1`; `docs/design/palw/g14-detection-policy-2026-10-17.md` §10).
//!
//! A producer commits a kernel-route claim whose trace lies at ONE position (one MatMul value at position 1; every other position is
//! the class's own computation) through signed carriers, the mempool, the node's template and the chain block's fold; every assigned
//! seat colludes. An honest checker under an ARMED detection policy (the fence value with this class's policy, the regime the seat
//! reads) draws its positions with the policy's `k` in the v2 domain — post-commit, bound to claim, class, job, policy and checker —
//! and checks the drawn positions against the class's reference semantics (`trace_v1` over the claim's public stream). Over many
//! independent anchors its detection rate is the hypergeometric value `k / N` for this single-position fault. The detecting checker
//! localises and files from public material alone; the chain convicts, slashes the real producer bond by the claim's reservation and
//! the claim never reaches `Final`: no reward is owed to its producer.
//!
//! Scope, stated: the kernel route's own admission does not consult the fence (the V2 lanes' claim gate does,
//! `g14_detection_gate.rs`); the policy here governs the checker's draw, which is what this test exercises on the node.
use super::*;
use kaspa_consensus_core::palw_detection_policy_v1::*;
use std::collections::BTreeSet;

fn kernel_policy(class: Hash64) -> PalwDetectionPolicyV1 {
    let mut p = palw_detection_genesis_design_v1(class, PalwDetectionRouteV1::Kernel, PalwIntervalCountRuleV1::PricedLeaves {
        leaves_per_interval: 1,
    });
    // Positions are sampled (the element courts check a position); the fault set is "at least 3 of the claim's positions", 90%.
    p.method = PalwInspectionMethodV1::IntervalReplay;
    p.localisation = PalwLocalisationV1::RootDescentElementCourt;
    p.fault_model = PalwFaultModelV1 { d_min_abs: 3, d_min_ppm: 0 };
    p.epsilon = PalwRatioV1::new(1, 10);
    p.resources.max_claim_replay_units = 64;
    p
}

#[tokio::test]
async fn g14_detection_policy_a_checker_under_an_armed_policy_finds_a_single_position_lie_at_its_rate_and_the_filing_convicts() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let producer_collateral = w.net.collateral(0);
    let producer_owed = w.net.owed(0);
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;

    // The class's reference computation over the claim's public stream: the ground truth of which positions lie.
    let ledger = w.net.ledger();
    let generated = greedy(&w.fx, &ledger, &w.class, &job.prompt, job.max_new_tokens as usize);
    let fed = generated.len() - 1;
    let stream: Vec<u32> = job.prompt.iter().chain(&generated[..fed]).copied().collect();
    let reference = trace_v1(&w.fx.program, &w.fx.params, &stream).expect("the reference trace");
    let n = lie.trace.values.len() as u32;
    let bad: BTreeSet<u32> = (0..n).filter(|p| reference.values[*p as usize] != lie.trace.values[*p as usize]).collect();
    assert_eq!(bad, BTreeSet::from([1]), "a single-position lie");

    // The armed policy and the plan for this claim (N = the claim's positions, priced one leaf each).
    let class = Hash64::from_bytes(w.class);
    let fence = PalwDetectionPolicyFenceV1 { activation: ForkActivation::new(0), policies: vec![kernel_policy(class)] };
    let PalwDetectionRegimeV1::Policy(policy) = palw_detection_regime_v1(Some(&fence), &class, 0, w.net.daa()) else {
        panic!("the armed fence governs this class")
    };
    let plan = policy
        .plan_v1(&PalwDetectionClaimFactsV1 {
            attempt_lane: false,
            work_leaves: u64::from(n),
            context_priced_leaves: Some(u64::from(n)),
            prompt_tokens: 0,
            decode_tokens_executed: 0,
            collateral_sompi: u128::from(producer_collateral),
        })
        .expect("the policy plans this claim");
    assert_eq!(plan.n, n);
    assert!(plan.k_total < n, "a sampled check, not a whole replay (k = {}, N = {n})", plan.k_total);

    // An honest outside checker (a bonded card that is neither the producer nor a seat), many independent post-commit anchors.
    let checker = w.outsiders(&lie, &seats, 1)[0];
    let binding = PalwDetectionDrawBindingV2 {
        network_domain: Hash64::from_u64_word(0x11),
        claim_id: Hash64::from_bytes(lie.id),
        class_id: class,
        job_id: Hash64::from_bytes(job.id()),
        policy_id: policy.id(),
        checker: PalwDetectionCheckerV1::PublicBond(Hash64::from_bytes(w.net.kid(checker))),
    };
    const TRIALS: u64 = 400;
    let mut hits = 0u64;
    let mut first_hit = None;
    for t in 0..TRIALS {
        let anchors = [PalwDetectionRoundAnchorV1 { block: Hash64::from_u64_word(0xA0_0000 + t), daa: w.net.daa() }];
        let (rounds, union) = palw_detection_draw_rounds_v2(&binding, w.net.daa() - 1, &anchors, plan.k_round, n).expect("the draw");
        assert_eq!(union.len() as u32, plan.k_total.min(n));
        let drawn = &rounds[0];
        // The check: each drawn position's committed values against the reference.
        if drawn.iter().any(|p| reference.values[*p as usize] != lie.trace.values[*p as usize]) {
            hits += 1;
            first_hit.get_or_insert(t);
        }
    }
    let p = f64::from(plan.k_total) / f64::from(n);
    let rate = hits as f64 / TRIALS as f64;
    let tol = 4.0 * (p * (1.0 - p) / TRIALS as f64).sqrt() + 1.0 / TRIALS as f64;
    eprintln!("node detection: N={n} k={} D=1: empirical {rate:.4}, analytic {p:.4}", plan.k_total);
    assert!((rate - p).abs() <= tol, "empirical {rate} vs analytic {p}");
    assert!(first_hit.is_some(), "some draw finds the lie");

    // The detecting checker localises and files from the read API and the public DA alone; the chain convicts.
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0xD7);
    w.proof(checker, &lie.id, proof).await;
    let pol = w.policy();
    assert!(w.net.ledger().claims[&lie.id].convicted, "convicted on the checker's filing");
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Convicted { .. }), "{:?}", w.net.claim_state(&lie.id));
    assert_eq!(w.net.collateral(0), producer_collateral - pol.claim_collateral, "the real producer bond pays the reservation");
    assert_eq!(w.net.owed(0), producer_owed, "no reward is owed to the producer for the convicted claim");
    w.net.beat_to(w.net.daa() + 200).await;
    assert!(!matches!(w.net.claim_state(&lie.id), ClaimStateV1::Final { .. }), "a convicted claim never reaches Final");
    assert_eq!(w.net.owed(0), producer_owed, "and never earns later");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}
