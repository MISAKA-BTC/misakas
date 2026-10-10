//! **Lane G14C — typed roots on the real node: derived eligibility (GAP-50) and node recovery (GAP-51).** R4X's harness, unchanged.
//!
//! * **GAP-50.** A typed class is admitted under OPV by its derived eligibility (`opv_spec_eligibility_v1`), never by naming it: a
//!   `Memory` class through its rule program's single-program registration over its weights (named here, and only it, through the
//!   processor's `cfg(test)` seam — that component's own onboarding is OPVB's, tested there), a `Retrieval` class never (no conformance
//!   statement for a snapshot exists: `SNAPSHOT_NOT_ONBOARDABLE`), a `Composite` only when every stage's component is. The specs here use
//!   fixtures no other test names, so the seam — a process-wide list — cannot leak another test's names into these verdicts.
//! * **GAP-51.** A memory lie nobody prosecuted in its window moves the line at Final; a node started afterwards from a pruning point
//!   finds it from its own reads and convicts it inside the liability horizon through its own mempool and template, and the line rolls
//!   back to the head it had. A lie in a composite's MODEL stage is convicted at that stage. The typed rows (tables 22–24, the line
//!   head) survive a real restart over the same database, and the line carries on after it.

use super::super::t12_round_lane_e2e::t12_reopened_chain;
use super::*;
use crate::pipeline::virtual_processor::processor::kernel_route_test_opv_eligible_v1;
use kaspa_consensus_core::palw_opv_bootstrap_v1::OpvClassFactsV1;

/// `memory_spec`'s shape over another `memory_v1` seed (a class no other test registers or seams).
fn memory_spec_of(seed: u64) -> (TirSketchFixtureV1, ComputationSpecV1) {
    let fx = memory_v1(seed);
    let w = weights(&fx);
    let slots = vec![MemorySlotV1 { param: (1, Some(0)), state: (0, Some(0)) }];
    let m0 = slot_commitments_v1(&slots, &w.param_commitments).unwrap();
    let root = MemoryRootV1 {
        extension: k2_tr_v1_descriptor().digest(),
        line: [0x5E; 64],
        initial_root: memory_root_v1(&slots, &m0),
        slots,
        max_steps: 8,
    };
    let spec = ComputationSpecV1 { version: 1, mode: OPV, roots: vec![TypedRootV1::WeightsV1(w), TypedRootV1::MemoryV1(root)] };
    (fx, spec)
}

/// The OPV id of the single-program registration a memory class's eligibility is derived from (its rule program over its weights).
fn component_of(fx: &TirSketchFixtureV1) -> Hash64 {
    let w = weights(fx);
    Hash64::from_bytes(OpvClassFactsV1::of_registration(w.descriptor, &w.program_bytes, &w.plan, &w.param_commitments).opv_id)
}

impl Net {
    /// Send a typed registration and report whether the class now stands (a refused registration writes nothing).
    async fn try_register(&mut self, spec: ComputationSpecV1) -> bool {
        let class = spec.class_id().unwrap();
        let o = self.spec(1, SpecObjectV1::RegisterClass { spec });
        self.send(vec![(1, o)]).await;
        let l = self.ledger();
        l.typed.classes.contains_key(&class) && l.opv.classes.contains(&class)
    }
}

/// **GAP-50: typed classes are OPV-eligible only through their components.**
#[tokio::test]
async fn r4x_g14c_typed_classes_are_eligible_only_through_their_components() {
    kaspa_core::log::try_init_logger("warn");
    let (fx_a, spec_a) = memory_spec_of(31);
    let (fx_b, spec_b) = memory_spec_of(32);
    for fx in [&fx_a, &fx_b] {
        kernel_route_test_attest_artifact_v1(Hash64::from_bytes(ParamCommitmentsV1::of(&fx.params).root()), 0);
    }
    // Only A's COMPONENT (its rule program's single-program registration) is eligible; no typed class id is named.
    kernel_route_test_opv_eligible_v1(component_of(&fx_a));
    let mut net = Net::new(true).await;
    assert!(net.try_register(spec_a.clone()).await, "a memory class whose rule program is eligible is admitted by derivation");
    assert!(!net.try_register(spec_b).await, "a memory class whose rule program is not eligible is refused");
    // A retrieval class nobody named: no conformance statement for a snapshot exists.
    assert!(!net.try_register(retrieval_spec(5)).await, "a retrieval class is never derived-eligible");
    // A composite whose tool stage is an unregistered retrieval class, and its model the eligible memory class's rule: refused.
    let stages = vec![
        CompositeStageV1 { component: retrieval_spec(6).class_id().unwrap(), input: StageInputV1::Query(QuerySourceV1::JobQuery) },
        CompositeStageV1 {
            component: model_spec().class_id().unwrap(),
            input: StageInputV1::Tokens {
                sources: vec![TokenSourceV1::JobPrompt, TokenSourceV1::StagePayloads { stage: 0 }],
                max_new_tokens: 3,
            },
        },
    ];
    let composite = ComputationSpecV1 {
        version: 1,
        mode: OPV,
        roots: vec![TypedRootV1::CompositeV1(CompositeRootV1 { extension: k2_tr_v1_descriptor().digest(), stages })],
    };
    assert!(!net.try_register(composite).await, "a composite with a stage that is not onboarded is refused");
    // The admitted memory class works as before: a job and an honest claim, Final with no Panel.
    let class = spec_a.class_id().unwrap();
    let job =
        MemoryJobV1 { class, pre_root: net.ledger().typed.lines[&class].head_root, chunks: vec![vec![3, 17, 9]], nonce: [1; 64] };
    net.post(SpecJobV1::Memory(job.clone())).await;
    let l = net.ledger();
    let SpecClassKindV1::Memory { rule, root, writers } = &l.typed.classes[&class].kind else { panic!() };
    let m0 = vec![fx_a.params.tensors[&(1, Some(0))].clone()];
    let p = produce_memory_v1(&class, rule, root, writers, &job, &net.kid(0), &fx_a.params, &m0, 2, |_, _| {}).unwrap();
    let c = net.commit(0, SpecClaimV1::Memory(p.claim.clone())).await;
    net.final_of(&c).await;
    net.assert_replays().await;
}

/// R4X's `Net` around another chain (a node started later): the same actors, keys and funding.
fn on_chain(net: &Net, chain: T12Chain) -> Net {
    let mut node = Net {
        chain,
        config: net.config.clone(),
        bundle: net.bundle.clone(),
        premine: net.premine.clone(),
        floats: net.floats.clone(),
        funding: net.funding.clone(),
        domain: net.domain,
        rnd: net.rnd,
    };
    node.chain.ctx.simulated_time = net.chain.ctx.simulated_time;
    node
}

/// **A node started from a pruning point** (as `g14_kernel_route_e2e/canonical.rs`'s): `main` mines one block past its sink `P`;
/// the node takes the blocks through `P`, installs the PALW state served for `P` (the typed rows ride tail `0xEC`), then the rest.
async fn pruned_node(main: &mut Net) -> Net {
    use kaspa_consensus_core::block::Block;
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    let p = main.chain.sink();
    let ttpb = main.ttpb();
    main.chain.heartbeat(ttpb, Vec::new()).await;
    let all = chain_blocks(&main.chain, main.chain.sink());
    let k = all.iter().position(|b| b.header.hash == p).unwrap();
    let importer = t12_genesis_chain(&main.config, &main.bundle, &main.premine, &main.floats);
    for b in &all[..=k] {
        arrive(&importer, b.clone(), "a block through the pruning point").await;
    }
    arrive(&importer, Block::from_header_arc(all[k + 1].header.clone()), "T's header").await;
    let vp = main.chain.vp();
    vp.capture_pruning_point_palw_state(p);
    let carriage: PalwStateCarriageV2 =
        borsh::from_slice(&borsh::to_vec(&vp.pruning_point_palw_state(p).expect("servable")).unwrap()).unwrap();
    {
        let ivp = importer.vp();
        let mut store = ivp.palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("no PALW tip");
        for blk in std::iter::once(importer.config.params.genesis.hash).chain(all[..=k].iter().map(|b| b.header.hash)) {
            store.delete_delta_for_tests(blk).expect("no delta row at or below the pruning point");
        }
    }
    importer.vp().import_pruning_point_palw_state(p, carriage).expect("the carriage installs");
    for blk in &all[k + 1..] {
        arrive(&importer, blk.clone(), "a block after P").await;
    }
    let node = on_chain(main, importer);
    assert_eq!(node.api(), main.api(), "the pruned node holds the typed rows");
    node
}

/// `from`'s blocks that `to` lacks arrive at `to`.
async fn sync(from: &Net, to: &mut Net) {
    for b in chain_blocks(&from.chain, from.chain.sink()) {
        if to.chain.ctx.consensus.get_block(b.header.hash).is_err() {
            arrive(&to.chain, b, "a peer's block").await;
        }
    }
    assert_eq!(to.chain.sink(), from.chain.sink());
    to.chain.ctx.simulated_time = to.chain.ctx.simulated_time.max(from.chain.ctx.simulated_time);
}

/// **GAP-51: a memory lie that reached Final is convicted from a node started afterwards, and the line rolls back.**
#[tokio::test]
async fn r4x_g14c_a_memory_lie_that_finalized_is_convicted_from_a_pruned_node_and_the_line_rolls_back() {
    kaspa_core::log::try_init_logger("warn");
    let mut m = MemWorld::new().await;
    let root0 = m.head();
    let job = m.job(vec![vec![3, 17, 9], vec![5, 2]]).await;
    let (s_at, n_at) = matmul_of(&m.fx.program);
    let lie = m.produce(&job, 4, &m.m0(), |i, t| {
        if i == 1 {
            bump(&mut t.values[0][s_at][n_at], 1)
        }
    });
    let before = m.net.collateral(4);
    let c = m.net.commit(4, SpecClaimV1::Memory(lie.claim.clone())).await;
    m.net.final_of(&c).await;
    assert_eq!(m.head(), *lie.claim.step_roots.last().unwrap(), "nobody prosecuted in the window: the lie moved the line");

    let mut node = pruned_node(&mut m.net).await;
    let fault = fault_of(fresh(&node, c, &Da::memory(&lie, Some(&m.m0())), &m.fx.params, &(), 0x44));
    assert!(matches!(fault, SpecFaultV1::MemoryStep { step: 1, .. }), "localised to step 1 from the node's own reads: {fault:?}");
    let outsider = 6;
    node.file(outsider, c, fault).await;
    sync(&node, &mut m.net).await;
    let l = m.net.ledger();
    assert!(l.claims[&c].convicted, "convicted after Final, inside the liability horizon");
    assert_eq!(m.head(), root0, "the line rolled back to the head it had before the lie");
    let economics = m.net.api().unwrap().header.opv.unwrap().economics;
    assert_eq!(m.net.collateral(4), before - economics.reservation_per_claim - economics.admission_fee);
    assert_eq!(m.net.owed(outsider), economics.reservation_per_claim * u64::from(l.policy.accuser_reward_permille) / 1000);
    let ttpb = m.net.ttpb();
    m.net.chain.heartbeat(ttpb, Vec::new()).await;
    sync(&m.net, &mut node).await;
    assert_eq!(node.api(), m.net.api(), "both nodes hold the same typed rows and line");
    m.net.assert_replays().await;
}

/// **GAP-51: a lie in a composite's MODEL stage is convicted at that stage** (the tool stage honest).
#[tokio::test]
async fn r4x_g14c_a_lie_in_the_model_stage_of_a_composite_is_convicted_at_that_stage() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::new(true).await;
    let tool = net.register(retrieval_spec(2)).await;
    let model = net.register(model_spec()).await;
    let class = net.register(composite_spec()).await;
    let data = corpus();
    let model_fx = wide128_v1(7);
    let l = net.ledger();
    let SpecClassKindV1::Retrieval { root } = &l.typed.classes[&tool].kind else { panic!() };
    let row = l.classes[&model].clone();
    let job = CompositeJobV1 { class, prompt: vec![3, 17], query: vec![2, -1, 1, 3], nonce: [11; 64] };
    let jid = net.post(SpecJobV1::Composite(job.clone())).await;
    let result = data.retrieve(root, &job.query).unwrap();
    let payloads: Vec<Vec<u32>> = result.iter().map(|e| data.items[e.id as usize].payload.clone()).collect();
    let prompt: Vec<u32> = job.prompt.iter().copied().chain(payloads.iter().flatten().copied()).collect();
    let (s_at, n_at) = matmul_of(&model_fx.program);
    let (model_stage, trace) = produce_model_stage_v1(&model, &row, &jid, 1, prompt, 2, &net.kid(0), &model_fx.params, 2, |t| {
        bump(&mut t.values[1][s_at][n_at], 1)
    })
    .unwrap();
    let tool_stage = StageClaimV1::Retrieval { query: job.query.clone(), result, payloads };
    let c = net
        .commit(
            0,
            SpecClaimV1::Composite(CompositeClaimV1 { job_id: jid, producer_bond: net.kid(0), stages: vec![tool_stage, model_stage] }),
        )
        .await;
    let mut da = Da::default();
    da.stage(1, 0, &trace);
    let artifact = vec![model_fx.params.clone(), model_fx.params.clone()];
    let fault = fault_of(fresh(&net, c, &da, &artifact, &Mirror(&data, 0..0), 0x62));
    assert!(!matches!(fault, SpecFaultV1::Retrieval { .. }), "the honest tool stage is not accused: {fault:?}");
    net.file(6, c, fault).await;
    assert!(net.ledger().claims[&c].convicted, "convicted at the model stage");
    net.assert_replays().await;
}

/// **GAP-51: the typed rows and the memory line survive a real restart over the same database, and the line carries on after it.**
#[tokio::test]
async fn r4x_g14c_the_memory_line_survives_a_node_restart_and_carries_on() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, _receiver) = async_channel::unbounded();
    let (config, bundle, premine, floats) = typed_config(true);
    let chain = t12_genesis_chain_on(TestConsensus::with_db(db.clone(), &config, sender), &config, &bundle, &premine, &floats);
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let mut net = Net { chain, config, bundle, premine, floats: floats.clone(), funding: floats, domain, rnd: 0 };
    net.beat_to(1).await;
    let class = net.register(memory_spec()).await;
    let mut m = MemWorld { net, fx: memory_fx(), class, jobs: 0 };
    let job = m.job(vec![vec![3, 17, 9]]).await;
    let p = m.produce(&job, 0, &m.m0(), |_, _| {});
    let c = m.net.commit(0, SpecClaimV1::Memory(p.claim.clone())).await;
    m.net.final_of(&c).await;
    let (sink, root, rows, head) = (m.net.chain.sink(), m.net.chain.tip_state().1.state_root(), m.net.api(), m.head());

    // ---- stop the node, open it again on the same database ----
    let MemWorld { net, fx, class, jobs } = m;
    let Net { chain, config, bundle, premine, floats, funding, domain, rnd } = net;
    let (simulated_time, nonce) = (chain.ctx.simulated_time, chain.nonce_for_reopen());
    drop(chain);
    let mut resumed = config.clone();
    resumed.process_genesis = false;
    let (sender, _receiver2) = async_channel::unbounded();
    let chain = t12_reopened_chain(TestConsensus::with_db(db, &resumed, sender), &resumed, &bundle, simulated_time, nonce);
    let net = Net { chain, config, bundle, premine, floats, funding, domain, rnd };
    let mut m = MemWorld { net, fx, class, jobs };
    assert_eq!((m.net.chain.sink(), m.net.chain.tip_state().1.state_root()), (sink, root), "the same tip and PALW root");
    assert_eq!(m.net.api(), rows, "the same typed rows");
    assert_eq!(m.head(), head, "the same line head");
    // The line carries on from the restored head.
    let pre = m.chain_head();
    let job2 = m.job(vec![vec![4, 4]]).await;
    assert_eq!(job2.pre_root, head);
    let p2 = m.produce(&job2, 5, &pre, |_, _| {});
    let c2 = m.net.commit(5, SpecClaimV1::Memory(p2.claim.clone())).await;
    m.net.final_of(&c2).await;
    assert_eq!(m.head(), *p2.claim.step_roots.last().unwrap(), "the line moved after the restart");
    m.net.assert_replays().await;
}
