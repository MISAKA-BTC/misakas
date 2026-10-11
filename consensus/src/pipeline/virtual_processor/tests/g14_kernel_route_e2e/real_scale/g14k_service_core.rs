//! **Lane G14-K on K2-TIR-v4 (segmented, OPV): the kernel service's own pipeline, at the canonical harness.**
//!
//! kaspad's kernel-route verifier (`kaspad::palw_panel::palw_kernel_verifier`) runs on the real service for K2-TIR v1/v2 classes
//! (`kaspad/src/palw_service_kernel_e2e.rs`). A segmented class cannot be registered on kaspad's test network: v4/v5 classes take
//! OPV only, and OPV eligibility is DERIVED (onboarding conformance passed under a Panel-independent beacon — codex's lane), with the
//! consensus crate's `cfg(test)` eligibility seam out of reach of kaspad's tests. So the service's segmented half is exercised here,
//! on the canonical node harness (mempool → template → block fold), through EXACTLY the functions the service calls, and nothing a
//! test supplies:
//!
//! * the ledger is the consumer's view (`pursuit::kernel_consumer_ledger_v1`) of op 211's rows;
//! * the token stream is rebuilt from the chain: the job's prompt from the `PostPromptTile` objects the blocks carry, each
//!   authenticated against the job's committed root (`pursuit::kernel_tiled_prompt_v1`, `kernel_segmented_tokens_v1`) — never the
//!   producer's;
//! * the served positions are the `Respond` parts the blocks carry, reassembled from the generic `ObjectChunk` lane
//!   (`pursuit::kernel_reassemble_objects_v1`, `KernelBlockMaterialV1`);
//! * the finding is `pursuit::kernel_check_segmented_v1` over the verifier's own replay (`seg_detect::prepare_reexecution_v1`);
//! * the step is `pursuit::plan_kernel_pursuit_on_v1`, the objects `pursuit::kernel_step_sends_v1`, signed deterministically.
//!
//! Level: **V-node(harness)**, not V-node(svc) — stated in the completion matrix.
use super::super::recovery::{carried_objects, det_chunk, det_route, synced_collateral};
use super::*;
use misaka_palw_remote::pursuit::{
    KERNEL_SERVICE_CHUNK_CAP_V1, KernelBlockMaterialV1, KernelFilingV1, KernelGoalV1, KernelPursuitOutcomeV1, KernelPursuitStepV1,
    KernelPursuitV1, KernelSendV1, kernel_check_segmented_v1, kernel_consumer_ledger_v1, kernel_reassemble_objects_v1,
    kernel_segmented_tokens_v1, kernel_step_sends_v1, kernel_tiled_prompt_v1, plan_kernel_pursuit_on_v1,
};

/// **The service's segmented pipeline as a struct**: the card (identity), the registered model, and this process's replay cache.
struct ServiceCore {
    card: usize,
    model: MapParams,
    replay: std::cell::RefCell<Option<(Digest, misaka_palw_kernel::seg_detect::OwnReplayV1)>>,
}

impl ServiceCore {
    fn new(card: usize, model: &MapParams) -> Self {
        Self { card, model: model.clone(), replay: std::cell::RefCell::new(None) }
    }

    /// The service's tick, minus the carrier: `(step, the objects it sends)`, or `None` when its own check finds nothing.
    fn tick_objects(&self, net: &Net, claim: &Digest) -> Option<(KernelPursuitStepV1, Vec<Obj>)> {
        let route = net.api().expect("the route");
        let ledger = kernel_consumer_ledger_v1(&route, None, None).expect("the rows rebuild");
        let row = ledger.claims.get(claim)?;
        let view = ledger.seg_claim_view_v1(claim).expect("a segmented claim");
        let objects = kernel_reassemble_objects_v1(carried_objects(net));
        let prompt = kernel_tiled_prompt_v1(&objects, &row.job_id, view.job.prompt_len, &view.job.prompt_root);
        let tokens = kernel_segmented_tokens_v1(&view, prompt.as_deref()).expect("the prompt is public in full on the chain");
        {
            let mut cache = self.replay.borrow_mut();
            if cache.as_ref().is_none_or(|(c, _)| c != claim) {
                let art = |j: u16, l: Option<u16>| self.model.tensors.get(&(j, l)).cloned();
                *cache =
                    Some((*claim, misaka_palw_kernel::seg_detect::prepare_reexecution_v1(&view.context(), &art, &tokens).unwrap()));
            }
        }
        let served = KernelBlockMaterialV1::from_objects(&objects, claim, &view);
        let goal = if row.convicted {
            KernelGoalV1::Demand(vec![(0, 0)]) // the plan answers `Done` from the rows
        } else {
            let cache = self.replay.borrow();
            kernel_check_segmented_v1(&view, &cache.as_ref().unwrap().1, &served, &self.model, &tokens).expect("the search runs")?
        };
        let pursuit = KernelPursuitV1 {
            claim: *claim,
            bond: net.bond(self.card),
            goal,
            synced_collateral: Some(synced_collateral(net, self.card)),
        };
        let filing = match &pursuit.goal {
            KernelGoalV1::Prove(proof) => {
                Some(det_route(net, self.card, &K::FileProof { accuser: pursuit.accuser(), claim: *claim, proof: proof.clone() }))
            }
            KernelGoalV1::Demand(_) => None,
        };
        let f = filing.as_ref().map(|signed| KernelFilingV1 { signed, chunk_cap: KERNEL_SERVICE_CHUNK_CAP_V1 });
        let step = plan_kernel_pursuit_on_v1(&route, &ledger, &pursuit, f.as_ref()).expect("the plan");
        let sends = kernel_step_sends_v1(&step, &pursuit, f.as_ref()).expect("the sends");
        let objects = sends
            .into_iter()
            .map(|s| match s {
                KernelSendV1::Route(K::FileProof { .. }) => filing.clone().expect("a filing"),
                KernelSendV1::Route(k) => det_route(net, self.card, &k),
                KernelSendV1::Chunk(c) => det_chunk(net, self.card, c),
            })
            .collect();
        Some((step, objects))
    }

    /// One tick: plan and send (the seal's delay waited out by heartbeats, as the chain's clock would).
    async fn tick(&self, net: &mut Net, claim: &Digest) -> Option<KernelPursuitStepV1> {
        let (step, objects) = self.tick_objects(net, claim)?;
        if !objects.is_empty() {
            let card = self.card;
            net.send(objects.into_iter().map(|o| (card, o)).collect()).await;
        } else if let KernelPursuitStepV1::AwaitSeal { file_from_daa, .. } = step {
            net.beat_to(file_from_daa).await;
        }
        Some(step)
    }
}

/// The producer (card 0) answers every open demand on `lie` (its obligation) — unless it is withholding.
async fn answer_all(net: &mut Net, program: &TirProgramV1, lie: &SegClaim) -> usize {
    let ledger = net.ledger();
    let mut answers = Vec::new();
    for ((c, _, p), _) in ledger.demands.iter().filter(|((c, _, _), _)| *c == lie.id) {
        let progress = ledger.seg_progress.get(&(*c, 0, *p));
        for i in 0..position_parts_v1(program, *p).unwrap().len() as u32 {
            if progress.is_some_and(|g| g.is_served(i)) {
                continue;
            }
            let part = position_part_v1(program, *p, &lie.values[*p as usize], lie.c.position_path(*p).1, i).unwrap();
            let respond = K::Respond { claim: lie.id, stage: 0, position: *p, bytes: borsh::to_vec(&part).unwrap() };
            answers.push((0usize, net.route(0, &respond)));
        }
    }
    let n = answers.len();
    if n > 0 {
        net.send_all(answers).await;
    }
    n
}

/// **A segmented lie, every other bond colluding by silence, convicted through the service's pipeline**: the verifier rebuilds the
/// prompt from the tiles on chain, its own replay diverges from the committed segment roots, it demands (adaptive search), reads the
/// producer's `Respond` parts back from the blocks, localizes the element, seals and files — conviction. An HONEST twin of the same
/// job shape finds nothing: the service sends nothing at all.
#[tokio::test]
async fn g14k_the_service_core_convicts_a_segmented_lie_via_demands_and_respond_chunks_and_ignores_the_honest_twin() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let outsider = w.net.chain.bonds.len() - 1;
    let prompt: Vec<u32> = (0..1300u32).map(|i| (i * 11 + 3) % 32).collect();
    let honest_job = w.tiled_job(&prompt).await;
    let honest = w.claim(0, honest_job, &prompt, None).await;
    let job = w.tiled_job(&prompt).await;
    let (_, s, n) = matmul_at(&w.f.program, 0);
    let lie = w.claim(0, job, &prompt, Some((1200, s, n))).await;
    let program = w.f.program.clone();
    let core = ServiceCore::new(outsider, &w.f.params);
    assert!(core.tick_objects(&w.net, &honest.id).is_none(), "the honest twin checks: nothing to send");
    let mut demands = 0;
    let mut end = None;
    for _ in 0..96 {
        match core.tick(&mut w.net, &lie.id).await.expect("the lie is found") {
            KernelPursuitStepV1::Demand { .. } => demands += 1,
            KernelPursuitStepV1::AwaitAnswer { .. } => {
                assert!(answer_all(&mut w.net, &program, &lie).await > 0, "an open demand is owed parts");
            }
            step @ (KernelPursuitStepV1::Done(_)
            | KernelPursuitStepV1::TooLate { .. }
            | KernelPursuitStepV1::WouldBeDismissed { .. }
            | KernelPursuitStepV1::Refused { .. }
            | KernelPursuitStepV1::Stranded { .. }) => {
                end = Some(step);
                break;
            }
            _ => {}
        }
    }
    assert_eq!(end, Some(KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted)), "the pursuit ends convicted");
    assert!(demands >= 1, "the search demanded positions");
    assert!(w.net.ledger().claims[&lie.id].convicted);
    assert!(!w.net.ledger().claims[&honest.id].convicted, "the honest twin untouched");
    assert!(core.tick_objects(&w.net, &honest.id).is_none(), "and still nothing to send for it");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay of the service-core conviction");
}

/// **A segmented lie whose producer withholds every demanded answer**: the service's pipeline demands, awaits, sends nothing more —
/// and at the counted deadline the claim is the producer's availability default (`Done(Defaulted)`), never a conviction.
#[tokio::test]
async fn g14k_the_service_core_takes_a_withheld_segmented_answer_to_the_da_default() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let outsider = w.net.chain.bonds.len() - 1;
    let prompt: Vec<u32> = (0..1300u32).map(|i| (i * 7 + 1) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let (_, s, n) = matmul_at(&w.f.program, 0);
    let lie = w.claim(0, job, &prompt, Some((1200, s, n))).await;
    let core = ServiceCore::new(outsider, &w.f.params);
    let slashed_before = w.net.slashed(0);
    let mut end = None;
    for _ in 0..400 {
        match core.tick(&mut w.net, &lie.id).await.expect("the lie is found") {
            KernelPursuitStepV1::AwaitAnswer { deadline_daa, .. } => {
                assert!(w.net.daa() <= deadline_daa + 2, "the chain defaults the demand at its deadline");
                let next = w.net.daa() + 1;
                w.net.beat_to(next).await; // the producer stays silent
            }
            step @ KernelPursuitStepV1::Done(_) => {
                end = Some(step);
                break;
            }
            KernelPursuitStepV1::Demand { .. } => {}
            other => panic!("unexpected step: {other:?}"),
        }
    }
    assert_eq!(end, Some(KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Defaulted)), "the objective outcome is the DA default");
    let ledger = w.net.ledger();
    let row = &ledger.claims[&lie.id];
    assert!(!row.convicted, "withholding is never a conviction");
    assert!(matches!(row.life.state, ClaimStateV1::Unavailable { producer_defaulted: true, .. }), "{:?}", row.life.state);
    assert!(w.net.slashed(0) > slashed_before, "the default is charged to the producer's real bond");
    assert_eq!(w.net.kernel_reserved(outsider), 0, "the demand bonds returned");
}
