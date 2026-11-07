//! **Lane G14-K × typed roots (RFC-0004 Part II): the kernel service's typed pipeline on R4X's harness** — a memory step lie
//! convicted before Final and after Final, an honest memory claim left alone, through exactly the `misaka_palw_remote::pursuit`
//! functions kaspad's kernel-route verifier calls (`kernel_consumer_ledger_v1` → `kernel_class_model_root_v1` (the rule program's
//! commitments: the only root the verifier's model is matched by) → `kernel_check_typed_v1` (no producer value read) →
//! `plan_kernel_pursuit_on_v1` → `kernel_step_sends_v1`). Typed classes are OPV-only and their eligibility is derived through the rule
//! program's component (R4X's harness names it through the processor's `cfg(test)` seam), so — like the segmented half — this is
//! V-node(harness), not the actual service.
use super::*;
use misaka_palw_remote::pursuit::{
    KernelFilingV1, KernelGoalV1, KernelPursuitOutcomeV1, KernelPursuitStepV1, KernelPursuitV1, KernelSendV1, kernel_check_typed_v1,
    kernel_class_model_root_v1, kernel_consumer_ledger_v1, kernel_step_sends_v1, plan_kernel_pursuit_on_v1,
};

/// The service's typed pipeline, one tick minus the carrier: `None` when its own check finds nothing to do.
fn tick_objects(net: &mut Net, card: usize, claim: &Digest, model: &MapParams) -> Option<(KernelPursuitStepV1, Vec<Obj>)> {
    let route = net.api().expect("the route");
    let ledger = kernel_consumer_ledger_v1(&route, None, None).expect("the rows rebuild");
    let row = ledger.claims.get(claim)?;
    let root = kernel_class_model_root_v1(&ledger, &row.class_binding_id).expect("a memory class is checked with one model");
    assert_eq!(root, ParamCommitmentsV1::of(model).root(), "the model is matched by the REGISTERED rule commitments");
    let goal = if row.convicted {
        KernelGoalV1::Demand(vec![(0, 0)]) // the plan answers `Done` from the rows
    } else {
        kernel_check_typed_v1(&ledger, claim, model, [0x4B; 64]).expect("the model-holding verifier concludes")?
    };
    let bond = net.bond(card);
    let synced = net.collateral(card).saturating_sub(route.chunk_deposits_of_v1(&bond));
    let pursuit = KernelPursuitV1 { claim: *claim, bond, goal, synced_collateral: Some(synced) };
    let filing = match &pursuit.goal {
        KernelGoalV1::Prove(proof) => {
            Some(net.route(card, &K::FileProof { accuser: pursuit.accuser(), claim: *claim, proof: proof.clone() }))
        }
        KernelGoalV1::Demand(_) => None,
    };
    let f =
        filing.as_ref().map(|signed| KernelFilingV1 { signed, chunk_cap: misaka_palw_remote::pursuit::KERNEL_SERVICE_CHUNK_CAP_V1 });
    let step = plan_kernel_pursuit_on_v1(&route, &ledger, &pursuit, f.as_ref()).expect("the plan");
    let sends = kernel_step_sends_v1(&step, &pursuit, f.as_ref()).expect("the sends");
    let objects = sends
        .into_iter()
        .map(|s| match s {
            KernelSendV1::Route(K::FileProof { .. }) => filing.clone().expect("a filing"),
            KernelSendV1::Route(k) => net.route(card, &k),
            KernelSendV1::Chunk(_) => panic!("a memory filing fits one carrier"),
        })
        .collect();
    Some((step, objects))
}

/// Tick until the pursuit ends (sending what each step sends; the seal's delay waited out by heartbeats).
async fn pursue(net: &mut Net, card: usize, claim: &Digest, model: &MapParams) -> KernelPursuitStepV1 {
    for _ in 0..16 {
        let (step, objects) = tick_objects(net, card, claim, model).expect("the lie is found");
        if !objects.is_empty() {
            net.send(objects.into_iter().map(|o| (card, o)).collect()).await;
        } else if let KernelPursuitStepV1::AwaitSeal { file_from_daa, .. } = step {
            net.beat_to(file_from_daa).await;
        } else if matches!(step, KernelPursuitStepV1::Done(_)) {
            return step;
        } else {
            panic!("unexpected step: {step:?}");
        }
    }
    panic!("the typed pursuit did not end");
}

/// **A memory step lie, convicted before Final and (a second one) after Final, by the service's typed pipeline; an honest claim
/// checks and nothing is sent.** The first job runs from M0 (public: the verifier's own model), the later ones from the chain's
/// carried post-state — no producer value is read anywhere.
#[tokio::test]
async fn g14k_the_service_core_convicts_memory_step_lies_pre_and_post_final_and_leaves_an_honest_claim_alone() {
    kaspa_core::log::try_init_logger("warn");
    let mut m = MemWorld::new().await;
    let model = m.fx.params.clone();
    let outsider = 6;
    let (s_at, n_at) = matmul_of(&m.fx.program);
    // ---- an honest claim: nothing to send; it reaches Final and moves the line ----
    let job1 = m.job(vec![vec![3, 17, 9], vec![5, 2]]).await;
    let p1 = m.produce(&job1, 0, &m.m0(), |_, _| {});
    let c1 = m.net.commit(0, SpecClaimV1::Memory(p1.claim.clone())).await;
    assert!(tick_objects(&mut m.net, outsider, &c1, &model).is_none(), "the honest claim checks: nothing to send");
    m.net.final_of(&c1).await;
    let root1 = m.head();
    // ---- a lie before Final (from the chain's carried pre-state) ----
    let job2 = m.job(vec![vec![4, 4], vec![8, 1, 6]]).await;
    let pre2 = m.chain_head();
    let lie = m.produce(&job2, 4, &pre2, |i, t| {
        if i == 1 {
            bump(&mut t.values[0][s_at][n_at], 1)
        }
    });
    let c2 = m.net.commit(4, SpecClaimV1::Memory(lie.claim.clone())).await;
    let end = pursue(&mut m.net, outsider, &c2, &model).await;
    assert_eq!(end, KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
    let l = m.net.ledger();
    assert!(l.claims[&c2].convicted && matches!(l.claims[&c2].life.state, ClaimStateV1::Convicted { .. }), "convicted before Final");
    assert_eq!(m.head(), root1, "a convicted claim never moves the line");
    // ---- a lie that reached Final unprosecuted, then convicted inside the liability horizon (the line rolls back) ----
    let job3 = m.job(vec![vec![2, 9], vec![7, 7, 1]]).await;
    let lie3 = m.produce(&job3, 5, &pre2, |i, t| {
        if i == 0 {
            bump(&mut t.values[1][s_at][n_at], 1)
        }
    });
    let c3 = m.net.commit(5, SpecClaimV1::Memory(lie3.claim.clone())).await;
    m.net.final_of(&c3).await;
    assert_eq!(m.head(), *lie3.claim.step_roots.last().unwrap(), "nobody prosecuted in the window: the lie moved the line");
    let end = pursue(&mut m.net, outsider, &c3, &model).await;
    assert_eq!(end, KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
    assert!(m.net.ledger().claims[&c3].convicted, "convicted after Final");
    assert_eq!(m.head(), root1, "the line rolled back to the head it had");
    assert!(tick_objects(&mut m.net, outsider, &c1, &model).is_none(), "the honest claim is still left alone");
    m.net.assert_replays().await;
}
