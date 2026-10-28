//! **G14 lane closure on the real node (2026-10-16): the kernel route's Valid signers answer for a conviction (G-K1, test T-N11).**
//!
//! Every assigned seat signs a passing receipt for a lying claim; one bonded outsider convicts it from the read API and the public DA
//! alone. The conviction slashes the producer's reservation AND each covering seat's signer liability lock, pre- and post-Final; the
//! lock holds the seat's collateral (V2's committed ledger) to the claim's liability horizon, so the seat cannot exit around it; an
//! honest covered claim's locks are released at its horizon, uncharged.
//!
//! **No test seam on the acceptance path.** The class is registered over the production onboarding path — a V2 IR class, a bonded
//! refutable artifact binding (tag 104) that matures, then `RegisterClass` — never `kernel_route_test_attest_artifact_v1`; every
//! object is a signed carrier through the mempool, the node's template and the chain block's fold. The fences are the harness's
//! (armed at DAA 0 WITHOUT their validation, which refuses them on every network: G14 from genesis, user decision 2026-10-16).
use super::*;
use kaspa_consensus_core::palw_g14_lane_closure_v1::{
    PALW_G14_LANE_CLOSURE_TABLE_V1, palw_kernel_signer_lock_amount_v1, palw_kernel_signer_lock_key_v1,
};
use kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_QUORUM_V1;

/// **A Panel-licensed kernel class onboarded through the production path** (no attestation hook): card 1 registers the V2 IR class,
/// binds its artifact's kernel commitments (bonded, refutable), waits out the binding's window and registers the kernel class over the
/// now-attested root.
async fn onboarded_world(seed: u64) -> World {
    let f = onb_fixture(seed);
    let mut net = Net::over_cfg(kernel_config_onboarding(), TestConsensus::new);
    net.beat_to(1).await;
    let registrant = 1usize;
    let class_obj = net.v2_registration(&f, registrant, net.daa() + 30);
    let Obj::ClassRegisteredTirV1 { class_id: v2_class, .. } = &class_obj else { unreachable!() };
    let v2_class = *v2_class;
    net.send(vec![(registrant, class_obj)]).await;
    let kernel_root = Hash64::from_bytes(f.pc.root());
    let o = net.artifact_bound(registrant, v2_class, kernel_root);
    net.send(vec![(registrant, o)]).await;
    let matures = net.api().unwrap().artifact_binding_v1(&v2_class, &kernel_root).expect("the binding row").matures_daa;
    net.beat_to(matures).await;
    let d = k2_tir_v2_descriptor();
    let register = K::RegisterClass {
        descriptor: d.digest(),
        program_bytes: f.program.encode(),
        plan: f.plan.clone(),
        param_commitments: f.pc.clone(),
    };
    let o = net.route(registrant, &register);
    net.send(vec![(registrant, o)]).await;
    let class =
        *net.ledger().classes.keys().next().expect("the kernel class registered over the matured binding (no test attestation)");
    assert_eq!(net.api().unwrap().onboarding_attested_roots_v1(net.daa()), vec![kernel_root], "attested by the binding alone");
    World { net, fx: Fixture { program: f.program, params: f.params, plan: f.plan, pc: f.pc }, class, jobs: 0 }
}

/// The job poster (card 1, every `World::job`) pays a claim's Final reward out of its job escrow — a real slash of its bond. A seat
/// that is also the poster sees that spend beside its own lock; `paid` is the number of Finals its escrows paid in the interval.
fn poster_spend(w: &World, card: usize, paid: u64) -> u64 {
    if card == 1 { paid * w.policy().claim_reward } else { 0 }
}

/// `card`'s `(collateral, slashed)`.
fn money(w: &World, card: usize) -> (u64, u64) {
    (w.net.collateral(card), w.net.slashed(card))
}

/// Does the route hold `card`'s signer lock on `claim`?
fn locked(w: &World, card: usize, claim: &Digest) -> bool {
    let key = palw_kernel_signer_lock_key_v1(&w.net.kid(card), claim);
    w.net.api().expect("the route").aux.contains_key(&(PALW_G14_LANE_CLOSURE_TABLE_V1, key))
}

/// **T-N11: the kernel route's Valid signers are charged on conviction** — before Final, after Final inside the liability horizon,
/// and never on an honest claim (its locks are released at the horizon).
#[tokio::test]
async fn kernel_route_valid_signers_are_charged_on_conviction() {
    use kaspa_consensus_core::palw_state_v2::palw_bond_backs_live_duty_v1;
    kaspa_core::log::try_init_logger("warn");
    let mut w = onboarded_world(61).await;
    let lock = palw_kernel_signer_lock_amount_v1(w.policy().claim_reward, PALW_KERNEL_ROUTE_INTERIM_QUORUM_V1);
    assert!(lock > 0);

    // ---- 1. a lie, every seat colluding, convicted BEFORE Final ----
    let job = w.job().await;
    let producer_before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    assert_eq!(seats.len(), 3, "three distinct-operator seats");
    let before: Vec<(u64, u64)> = seats.iter().map(|s| money(&w, *s)).collect();
    let reserved_before: Vec<u128> = seats.iter().map(|s| w.net.kernel_reserved(*s)).collect();
    w.cover(&lie.id).await;
    for ((s, (collateral, _)), reserved) in seats.iter().zip(&before).zip(&reserved_before) {
        assert!(locked(&w, *s, &lie.id), "seat {s}: its passing receipt locked its signer liability");
        assert_eq!(w.net.kernel_reserved(*s), reserved + u128::from(lock), "seat {s}: V2's committed ledger holds the lock");
        assert_eq!(w.net.collateral(*s), *collateral, "seat {s}: a lock is not a charge");
        let state = w.net.chain.tip_state().1;
        assert!(palw_bond_backs_live_duty_v1(&state, &w.net.bond(*s), w.net.daa(), None), "seat {s}: the exit gate holds it");
    }
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let da = lie.published(&w.fx, &[]);
    let proof = w.prosecution(&lie.id, &da, 0x5B);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "the outsider convicted the covered lie");
    assert_eq!(w.net.collateral(0), producer_before - w.policy().claim_collateral, "the producer's reservation is slashed");
    for ((s, (collateral, slashed)), reserved) in seats.iter().zip(&before).zip(&reserved_before) {
        assert_eq!(w.net.collateral(*s), collateral - lock, "seat {s}: every Valid signer is charged its lock");
        assert_eq!(w.net.slashed(*s), slashed + lock, "seat {s}: as a real slash (burned at release)");
        assert!(!locked(&w, *s, &lie.id), "seat {s}: the lock is spent (charged once)");
        assert_eq!(w.net.kernel_reserved(*s), *reserved, "seat {s}: the lock is no longer held");
    }

    // ---- 2. a lie nobody prosecutes in its window: Final, then convicted inside the liability horizon ----
    let job2 = w.job().await;
    let late = w.claim(0, &job2, true).await;
    let seats2 = w.seats(&late.id);
    let before2: Vec<(u64, u64)> = seats2.iter().map(|s| money(&w, *s)).collect();
    w.cover(&late.id).await;
    let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&late.id) else { panic!("covered") };
    w.net.beat_to(window_end_daa).await;
    assert!(matches!(w.net.claim_state(&late.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&late.id));
    for s in &seats2 {
        assert!(locked(&w, *s, &late.id), "seat {s}: Final does not release a signer before the liability horizon");
    }
    let outsider2 = w.outsiders(&late, &seats2, 1)[0];
    let da = late.published(&w.fx, &[]);
    let proof = w.prosecution(&late.id, &da, 0x5C);
    w.proof(outsider2, &late.id, proof).await;
    assert!(w.net.ledger().claims[&late.id].convicted, "convicted after Final");
    for (s, (collateral, slashed)) in seats2.iter().zip(&before2) {
        let spend = poster_spend(&w, *s, 1);
        assert_eq!(w.net.collateral(*s), collateral - lock - spend, "seat {s}: charged after Final too");
        assert_eq!(w.net.slashed(*s), slashed + lock + spend);
        assert!(!locked(&w, *s, &late.id));
    }

    // ---- 3. an honest covered claim: its signers hold their locks to the horizon, then are released uncharged ----
    let job3 = w.job().await;
    let honest = w.claim(0, &job3, false).await;
    let seats3 = w.seats(&honest.id);
    let before3: Vec<(u64, u64)> = seats3.iter().map(|s| money(&w, *s)).collect();
    w.cover(&honest.id).await;
    let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&honest.id) else { panic!("covered") };
    w.net.beat_to(window_end_daa).await;
    let horizon = w.net.ledger().claims[&honest.id].liability_until.expect("liability runs");
    w.net.beat_to(horizon).await;
    for s in &seats3 {
        assert!(locked(&w, *s, &honest.id), "seat {s}: held through the last DAA a proof may convict at");
    }
    w.net.beat_to(horizon + 2).await;
    for (s, (collateral, slashed)) in seats3.iter().zip(&before3) {
        let spend = poster_spend(&w, *s, 1);
        assert!(!locked(&w, *s, &honest.id), "seat {s}: released after the horizon");
        assert_eq!(money(&w, *s), (collateral - spend, slashed + spend), "seat {s}: an honest signer is never charged");
    }

    // A second node replaying the blocks reaches the same roots, rows and deltas.
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}
