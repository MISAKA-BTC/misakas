//! **Lane G14-R on K2-TIR-v4 (segmented, OPV): the adaptive public prosecution RESUMES across a node restart between the producer's
//! `Respond` parts, a pruned import after a partial answer set, a reorg that removes the verifier's demand, and a full IBD mid-search.**
//!
//! The verifier ([`SegPursuer`]) keeps nothing but its card and the registered model. Each tick it reads the segmented record through
//! the read API, re-runs its own replay (recomputed by every new process), assembles every position the chain's blocks serve (the
//! `Respond` parts, authenticated against the committed segment roots), runs the adaptive search
//! (`seg_detect::check_claim_by_reexecution_v1`, deterministic in that material) and asks `misaka_palw_remote::pursuit` for the step.
use super::super::canonical::{Rpc, agree, pruned_import_node, sync};
use super::super::recovery::{carried_route_objects, dag_ibd_node, det_chunk, det_route, reorged_node};
use super::*;
use misaka_palw_kernel::seg_detect::{OwnReplayV1, check_claim_by_reexecution_v1, prepare_reexecution_v1};
use misaka_palw_remote::pursuit::{
    KernelFilingV1, KernelGoalV1, KernelPursuitOutcomeV1, KernelPursuitStepV1, KernelPursuitV1, plan_kernel_pursuit_v1,
};
use std::cell::RefCell;

/// The segmented record op 210 serves for `claim` on `net`, as the outsider's verifier reads it.
fn record_on(net: &Net, claim: &Digest) -> misaka_palw_kernel::seg_ledger::SegClaimViewV1 {
    let api = net.api().expect("the read API serves the route");
    let read = api.claim_read_v1(claim).expect("the rows rebuild").expect("the claim is served");
    let header: EvidenceHeaderV1 = borsh::from_slice(&read.record_header).expect("the class header");
    SegmentedClaimRecordV1::from_bytes(&read.public_record).expect("the record").view(&header).expect("the record is the claim's")
}

/// Every position of `claim` the node's chain serves in full: its `Respond` parts read from the blocks, assembled and authenticated
/// against the committed segment roots (a position whose parts do not all ride yet is not material).
fn served_from_blocks(net: &Net, claim: &Digest, view: &misaka_palw_kernel::seg_ledger::SegClaimViewV1) -> FromBlocks {
    let mut positions = BTreeMap::new();
    // Every position some `Respond` of the claim answers — carried whole, or in the generic `ObjectChunk`s a large part rides
    // (reassembled exactly as `served_parts_from_blocks` reads them).
    let mut seen = std::collections::BTreeSet::new();
    let mut groups: BTreeMap<Vec<u8>, (u8, BTreeMap<u8, Vec<u8>>)> = BTreeMap::new();
    let mut objects = Vec::new();
    for o in super::super::recovery::carried_objects(net) {
        match o {
            Obj::ObjectChunk { group, index, count, bytes } => {
                let entry = groups.entry(borsh::to_vec(&group).unwrap()).or_insert_with(|| (count, BTreeMap::new()));
                entry.1.insert(index, bytes);
                if entry.1.len() == entry.0 as usize {
                    let whole: Vec<u8> = entry.1.values().flatten().copied().collect();
                    if let Ok(o) = borsh::from_slice::<Obj>(&whole) {
                        objects.push(o);
                    }
                }
            }
            o => objects.push(o),
        }
    }
    for o in objects {
        if let Obj::KernelRouteV1 { bytes, .. } = o
            && let Ok(K::Respond { claim: c, position, .. }) = K::decode(&bytes)
            && c == *claim
        {
            seen.insert(position);
        }
    }
    // G14-S: a hiding value is served as leaf hashes beside the assembled position (`assemble_position_full_v1`).
    let mut hidden = BTreeMap::new();
    for p in seen {
        let parts = served_parts_from_blocks(net, claim, p);
        if let Some((a, leaves)) =
            misaka_palw_kernel::seg_da::assemble_position_full_v1(&view.program, &view.segment_roots, view.positions, p, &parts)
        {
            positions.insert(p, a);
            hidden.insert(p, leaves);
        }
    }
    FromBlocks { positions, hidden, read: Cell::new(0) }
}

/// **A public verifier of segmented claims that keeps nothing across a restart**: its own replay is recomputed by every new instance
/// (a pure function of the public record and the registered model, cached within one process only).
struct SegPursuer {
    card: usize,
    params: MapParams,
    own: RefCell<Option<(Digest, OwnReplayV1)>>,
}

impl SegPursuer {
    fn new(card: usize, params: &MapParams) -> SegPursuer {
        SegPursuer { card, params: params.clone(), own: RefCell::new(None) }
    }

    fn derive(&self, net: &Net, claim: &Digest, tokens: &[u32]) -> (PalwKernelRouteStateV1, KernelPursuitV1) {
        let (state, _) = Rpc::of(&net.chain).route();
        let ledger = state.ledger().expect("rows");
        let goal = match ledger.claims.get(claim) {
            Some(row) if !row.convicted => {
                let view = record_on(net, claim);
                let params = self.params.clone();
                let art = move |j: u16, l: Option<u16>| params.tensors.get(&(j, l)).cloned();
                let mut own = self.own.borrow_mut();
                if own.as_ref().is_none_or(|(c, _)| c != claim) {
                    *own = Some((*claim, prepare_reexecution_v1(&view.context(), &art, tokens).expect("the own replay")));
                }
                let (_, replay) = own.as_ref().unwrap();
                let chain = served_from_blocks(net, claim, &view);
                let r = check_claim_by_reexecution_v1(&view.context(), &replay.roots, replay.decode_mismatch, &chain, &art, tokens);
                match r.finding {
                    SegFindingV1::Fault(f) => KernelGoalV1::Prove(ProsecutionV1::Segmented(f.to_bytes())),
                    SegFindingV1::Demand(ps) => KernelGoalV1::Demand(ps.into_iter().map(|p| (0, p)).collect()),
                    other => panic!("the search stopped: {other:?}"),
                }
            }
            _ => KernelGoalV1::Demand(vec![(0, 0)]),
        };
        (
            state,
            KernelPursuitV1 {
                claim: *claim,
                bond: net.bond(self.card),
                goal,
                synced_collateral: Some(super::super::recovery::synced_collateral(net, self.card)),
            },
        )
    }

    fn plan(&self, net: &Net, claim: &Digest, tokens: &[u32]) -> KernelPursuitStepV1 {
        let (state, pursuit) = self.derive(net, claim, tokens);
        let filing = match &pursuit.goal {
            KernelGoalV1::Prove(proof) => {
                Some(det_route(net, self.card, &K::FileProof { accuser: pursuit.accuser(), claim: *claim, proof: proof.clone() }))
            }
            _ => None,
        };
        let f = filing.as_ref().map(|o| KernelFilingV1 { signed: o, chunk_cap: 50_000 });
        plan_kernel_pursuit_v1(&state, &pursuit, f.as_ref()).expect("the plan reads the rows")
    }

    /// One tick: plan and act. The producer's answers are not this verifier's: `AwaitAnswer` returns without mining.
    async fn tick(&self, net: &mut Net, claim: &Digest, tokens: &[u32]) -> KernelPursuitStepV1 {
        let (state, pursuit) = self.derive(net, claim, tokens);
        let filing = match &pursuit.goal {
            KernelGoalV1::Prove(proof) => {
                Some(det_route(net, self.card, &K::FileProof { accuser: pursuit.accuser(), claim: *claim, proof: proof.clone() }))
            }
            _ => None,
        };
        let f = filing.as_ref().map(|o| KernelFilingV1 { signed: o, chunk_cap: 50_000 });
        let step = plan_kernel_pursuit_v1(&state, &pursuit, f.as_ref()).expect("the plan reads the rows");
        let card = self.card;
        match &step {
            KernelPursuitStepV1::Seal { seal, .. } => {
                let o = det_route(net, card, &K::SealProof { accuser: pursuit.accuser(), claim: *claim, seal: *seal });
                net.send(vec![(card, o)]).await;
            }
            KernelPursuitStepV1::AwaitSeal { file_from_daa, .. } => net.beat_to(*file_from_daa).await,
            KernelPursuitStepV1::FileWhole { .. } => {
                net.send(vec![(card, filing.unwrap())]).await;
            }
            KernelPursuitStepV1::FileChunks { missing, .. } => {
                let chunks = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_chunks_v1(
                    filing.as_ref().unwrap(),
                    net.bond(card),
                    kaspa_consensus_core::palw_kernel_route_v1::PalwKernelChunkTargetV1::Claim(*claim),
                    50_000,
                )
                .unwrap();
                let send =
                    chunks.into_iter().filter(|c| missing.contains(&c.index)).map(|c| (card, det_chunk(net, card, c))).collect();
                net.send(send).await;
            }
            KernelPursuitStepV1::Demand { stage, position, .. } => {
                let o = det_route(
                    net,
                    card,
                    &K::FileDemand { demander: pursuit.accuser(), claim: *claim, stage: *stage, position: *position },
                );
                net.send(vec![(card, o)]).await;
            }
            _ => {}
        }
        step
    }
}

/// **The producer serves** (its obligation; card 0): at most `limit` of the parts still owed on the claim's open demands, in
/// position and part order, through the mempool and the template. Returns how many it sent.
async fn producer_serves(net: &mut Net, program: &TirProgramV1, lie: &SegClaim, limit: usize) -> usize {
    let ledger = net.ledger();
    let mut answers = Vec::new();
    for ((c, _, p), _) in ledger.demands.iter().filter(|((c, _, _), _)| *c == lie.id) {
        let progress = ledger.seg_progress.get(&(*c, 0, *p));
        for i in 0..position_parts_v1(program, *p).unwrap().len() as u32 {
            if progress.is_some_and(|g| g.is_served(i)) || answers.len() >= limit {
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

/// Ticks the verifier (the producer answering every `AwaitAnswer`) until the pursuit ends; at most `cap` ticks.
async fn pursue_to_end(sp: &SegPursuer, net: &mut Net, program: &TirProgramV1, lie: &SegClaim, cap: usize) -> KernelPursuitStepV1 {
    for _ in 0..cap {
        match sp.tick(net, &lie.id, &lie.tokens).await {
            KernelPursuitStepV1::AwaitAnswer { .. } => {
                assert!(producer_serves(net, program, lie, usize::MAX).await > 0, "an open demand is owed parts");
            }
            step @ (KernelPursuitStepV1::Done(_)
            | KernelPursuitStepV1::TooLate { .. }
            | KernelPursuitStepV1::WouldBeDismissed { .. }
            | KernelPursuitStepV1::Refused { .. }
            | KernelPursuitStepV1::Stranded { .. }) => return step,
            _ => {}
        }
    }
    panic!("the segmented pursuit reached no end in {cap} ticks");
}

/// Ticks until every position of the current round is demanded (the plan turns to `AwaitAnswer`).
async fn demand_round(sp: &SegPursuer, net: &mut Net, lie: &SegClaim) -> u64 {
    for _ in 0..32 {
        match sp.tick(net, &lie.id, &lie.tokens).await {
            KernelPursuitStepV1::Demand { .. } => {}
            KernelPursuitStepV1::AwaitAnswer { deadline_daa, .. } => return deadline_daa,
            other => panic!("a demand round: {other:?}"),
        }
    }
    panic!("the round never closed");
}

/// **Restart between `Respond` parts, then a pruned import after a partial answer set** (K2-TIR-v4 at 8,192 positions, OPV, the
/// producer and every other bond colluding by silence). The verifier demands its first round; the producer serves ONE part; the node
/// is restarted over its database; a new verifier plans `AwaitAnswer` (the demands are on disk: none re-filed); the producer serves
/// one more part; then a node imports the pruning-point carriage after that partial answer set, and the verifier — now on it — carries
/// the adaptive search on to a conviction (the producer answering there). The two nodes agree.
#[tokio::test]
async fn g14r_k2s_restart_between_respond_parts_then_a_pruned_import_resume_the_search_to_a_conviction() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut w = SegWorld::with(|c| TestConsensus::with_db(db.clone(), c, sender)).await;
    w.net._keep.push(Box::new(receiver));
    let outsider = w.net.chain.bonds.len() - 1;
    let prompt: Vec<u32> = (0..1300u32).map(|i| (i * 13 + 5) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let (_, s, n) = matmul_at(&w.f.program, 0);
    let lie = w.claim(0, job, &prompt, Some((1200, s, n))).await;
    let program = w.f.program.clone();

    let deadline = demand_round(&SegPursuer::new(outsider, &w.f.params), &mut w.net, &lie).await;
    let demands = carried_route_objects(&w.net, outsider, &lie.id, "FileDemand");
    assert!(demands >= 1);
    assert_eq!(producer_serves(&mut w.net, &program, &lie, 1).await, 1, "one part of the answer set");
    let mut w = w.restart(db.clone());
    let sp = SegPursuer::new(outsider, &w.f.params);
    let step = sp.plan(&w.net, &lie.id, &lie.tokens);
    assert!(
        matches!(step, KernelPursuitStepV1::AwaitAnswer { deadline_daa, .. } if deadline_daa <= deadline)
            || matches!(step, KernelPursuitStepV1::Demand { .. }),
        "after the restart the open demands are read off disk: {step:?}"
    );
    if matches!(step, KernelPursuitStepV1::AwaitAnswer { .. }) {
        assert_eq!(carried_route_objects(&w.net, outsider, &lie.id, "FileDemand"), demands, "none re-filed");
    }
    producer_serves(&mut w.net, &program, &lie, 1).await;
    let mut node = pruned_import_node(&mut w.net).await;
    let sp = SegPursuer::new(outsider, &w.f.params);
    let end = pursue_to_end(&sp, &mut node, &program, &lie, 64).await;
    assert_eq!(end, KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
    assert!(node.ledger().claims[&lie.id].convicted, "convicted on the pruned-import node");
    sync(&node, &mut w.net).await;
    agree(&w.net, &node, "the pruned-import node that finished the search");
}

/// **A reorg removes the verifier's first demand; a full IBD node finishes the search.** `Z` follows `A`, where the verifier demanded
/// its first position, then reorgs onto a heavier branch without it: the verifier on `Z` re-derives the same demand (its search is a
/// function of the public material, which `B` lacks too) and files it again there. After the producer answers that round on `Z`, a
/// node syncing every block of `Z` from genesis takes over and the verifier carries the search to the conviction on it.
#[tokio::test]
async fn g14r_k2s_a_reorg_removes_the_demand_and_an_ibd_node_finishes_the_search() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let outsider = w.net.chain.bonds.len() - 1;
    let prompt: Vec<u32> = (0..1300u32).map(|i| (i * 17 + 2) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let (_, s, n) = matmul_at(&w.f.program, 0);
    let lie = w.claim(0, job, &prompt, Some((1200, s, n))).await;
    let program = w.f.program.clone();
    let fork = w.net.chain.sink();
    let sp = SegPursuer::new(outsider, &w.f.params);
    let KernelPursuitStepV1::Demand { position: first, .. } = sp.tick(&mut w.net, &lie.id, &lie.tokens).await else {
        panic!("the search opens with a demand")
    };
    assert!(w.net.ledger().demands.contains_key(&(lie.id, 0, first)));

    // The funding is the virtual UTXO set's: A's demand carrier sits in the virtual's merge set.
    let mut zn = reorged_node(&w.net, fork, w.net.funding.clone()).await;
    assert!(!zn.ledger().demands.contains_key(&(lie.id, 0, first)), "B's tip holds no demand");
    let step = sp.plan(&zn, &lie.id, &lie.tokens);
    assert!(matches!(step, KernelPursuitStepV1::Demand { position, .. } if position == first), "re-derived: {step:?}");
    // The re-sent demand rides with A's merged one: a demand by the same bond is a free join, never a second bond.
    demand_round(&sp, &mut zn, &lie).await;
    producer_serves(&mut zn, &program, &lie, usize::MAX).await;

    // The DAG holds A's merged block off Z's selected chain: the syncing node receives the whole DAG.
    let mut node = dag_ibd_node(&zn).await;
    let sp = SegPursuer::new(outsider, &w.f.params);
    let end = pursue_to_end(&sp, &mut node, &program, &lie, 64).await;
    assert_eq!(end, KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
    sync(&node, &mut zn).await;
    agree(&zn, &node, "the IBD node that finished the search");
    assert!(zn.ledger().claims[&lie.id].convicted);
}
