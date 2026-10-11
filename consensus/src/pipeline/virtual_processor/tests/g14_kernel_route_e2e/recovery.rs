//! **Lane G14-R: the kernel route's public prosecution RESUMES (K2-TIR v1/v2 single-program classes; v4 is `real_scale/g14r_seg.rs`).**
//!
//! The verifier here is a [`Pursuer`]: identity (a bond's card), the registered model it acquired off-chain (ADR-0177), and NOTHING
//! else. Every tick it reads the route through op 211 (the RPC ops' own builders and JSON wire form, [`Rpc`]), rebuilds the ledger,
//! re-runs its deterministic check of the claim against public material, and asks `misaka_palw_remote::pursuit` what the chain still
//! needs. Its carriers are signed deterministically (FIPS 204, `rnd = 0^32`), so a re-built carrier is the SAME bytes. So each case
//! below stops the verifier — or the node under it — at a point of the pursuit and shows the pursuit reaching the same objective
//! outcome from chain state alone, with nothing sent twice that the chain charges twice:
//!
//! | recovery point | how | where it stops |
//! |---|---|---|
//! | verifier restart | a new [`Pursuer`] (no state carried) | after the seal, after each chunk of a 3-chunk filing, after conviction |
//! | node restart | the node reopened over the same database | after the seal and the first chunk |
//! | reorg | a heavier branch from before the seal; then the seal's branch out-works it | seal and partial filing removed / re-added |
//! | full IBD | a node replaying every block from genesis | after the seal and a partial filing; after a demand was served |
//! | pruned import | a node installing the PALW carriage at a pruning point | after a demand (the producer then stays silent) |
//! | deadline | the verifier down until its resume bound; past the pre-Final bound; past liability | G14-B: the bound is honoured |
//!
//! The fences are armed without their validation, exactly as in the parent module: nothing here can run on a network.
use super::canonical::{Rpc, agree, ibd_node, pruned_import_node, sync};
use super::*;
use kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2;
use misaka_palw_remote::pursuit::{
    KernelFilingV1, KernelGoalV1, KernelPursuitOutcomeV1, KernelPursuitStepV1, KernelPursuitV1, PURSUIT_DETERMINISTIC_RND_V1,
    kernel_pursuit_resume_bound_v1, opener_groups_for_claim_v1, plan_kernel_pursuit_v1,
};

// ---- deterministic carriers -----------------------------------------------------------------------------------------------------

/// A `KernelRouteV1` signed DETERMINISTICALLY by card `card` (the pursuit's carriers: a rebuilt carrier is the same bytes).
pub(super) fn det_route(net: &Net, card: usize, object: &K) -> Obj {
    let bytes = object.encode();
    let signer = net.bond(card);
    let message = palw_kernel_route_message_v1(net.domain, &signer, &bytes);
    let key = TestConsensus::palw_v2_registry_keypair(card as u64);
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &key.signing_key,
        message.as_byte_slice(),
        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
        PURSUIT_DETERMINISTIC_RND_V1,
    )
    .expect("ML-DSA-87 signs")
    .as_ref()
    .to_vec();
    Obj::KernelRouteV1 { bytes, signer, signature }
}

/// A route chunk (tag 113) signed deterministically by its opener, card `card`.
pub(super) fn det_chunk(net: &Net, card: usize, chunk: PalwKernelChunkV1) -> Obj {
    let message = palw_kernel_chunk_message_v1(net.domain, &chunk);
    let key = TestConsensus::palw_v2_registry_keypair(card as u64);
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &key.signing_key,
        message.as_byte_slice(),
        PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1,
        PURSUIT_DETERMINISTIC_RND_V1,
    )
    .expect("ML-DSA-87 signs")
    .as_ref()
    .to_vec();
    Obj::KernelRouteChunkV1 { chunk: Box::new(chunk), signature }
}

/// Every carrier object the node's selected chain holds (what a fresh verifier reads from the blocks).
pub(super) fn carried_objects(net: &Net) -> Vec<Obj> {
    chain_blocks(&net.chain, net.chain.sink())
        .iter()
        .flat_map(|b| b.transactions.iter())
        .filter(|tx| tx.subnetwork_id == kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE)
        .filter_map(|tx| borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload).ok())
        .map(|p| p.object)
        .collect()
}

/// How many times each chunk index of `group` (opened by `card`) rides the node's chain.
pub(super) fn carried_chunks(net: &Net, card: usize, group: Hash64) -> BTreeMap<u8, usize> {
    let opener = net.bond(card);
    let mut out = BTreeMap::new();
    for o in carried_objects(net) {
        if let Obj::KernelRouteChunkV1 { chunk, .. } = o
            && chunk.opener == opener
            && chunk.group == group
        {
            *out.entry(chunk.index).or_insert(0) += 1;
        }
    }
    out
}

/// How many `(object name)` route objects signed by `card` naming `claim` ride the chain (seals, demands, whole filings).
pub(super) fn carried_route_objects(net: &Net, card: usize, claim: &Digest, name: &str) -> usize {
    let signer = net.bond(card);
    carried_objects(net)
        .into_iter()
        .filter(|o| match o {
            Obj::KernelRouteV1 { bytes, signer: s, .. } if *s == signer => K::decode(bytes).is_ok_and(|k| {
                k.name() == name
                    && match &k {
                        K::SealProof { claim: c, .. } | K::FileProof { claim: c, .. } | K::FileDemand { claim: c, .. } => c == claim,
                        _ => false,
                    }
            }),
            _ => false,
        })
        .count()
}

/// **The verifier's own bond, as the fold will sync it** (its V2 collateral less the route's chunk deposits it holds — the
/// non-kernel reservations a verifier's bond carries here): what it reads of its own bond (op 202's bond row) before a dry run.
pub(super) fn synced_collateral(net: &Net, card: usize) -> u64 {
    let bond = net.bond(card);
    let collateral = net.chain.tip_state().1.bond(&bond).expect("the verifier's bond").collateral;
    collateral.saturating_sub(net.api().map_or(0, |r| r.chunk_deposits_of_v1(&bond)))
}

// ---- the pursuer: identity and model, no state ----------------------------------------------------------------------------------

/// **A public verifier that keeps nothing between ticks.** `chunks` > 1 forces its filing through the route's chunk lane in that many
/// parts (the cap derived from the filing's own size, so every rebuild derives the same cap).
#[derive(Clone)]
pub(super) struct Pursuer {
    pub(super) card: usize,
    pub(super) model: MapParams,
    pub(super) chunks: usize,
}

impl Pursuer {
    pub(super) fn new(card: usize, model: &MapParams, chunks: usize) -> Pursuer {
        Pursuer { card, model: model.clone(), chunks }
    }

    /// The route as op 211 serves it at `node`'s tip, and the pursuit re-derived from it: the verifier's own check of `claim` against
    /// the public material `da` (and whatever the chain serves) with its registered model.
    pub(super) fn derive(&self, node: &Net, claim: &Digest, da: &Da) -> (PalwKernelRouteStateV1, KernelPursuitV1) {
        let (state, root) = Rpc::of(&node.chain).route();
        let fresh = Fresh::from_api(&state, root, 0x5A);
        let goal = match fresh.ledger.claims.get(claim) {
            Some(row) if !row.convicted => match fresh.check(*claim, da, &self.model) {
                OutsiderFindingV1::Prosecute(proof) => KernelGoalV1::Prove(proof),
                OutsiderFindingV1::Demand(positions) => KernelGoalV1::Demand(positions),
                OutsiderFindingV1::Clean => panic!("these cases pursue lies"),
            },
            // Nothing to check (no such claim at this tip, or already convicted): the plan answers from the rows.
            _ => KernelGoalV1::Demand(vec![(0, 0)]),
        };
        (
            state,
            KernelPursuitV1 {
                claim: *claim,
                bond: node.bond(self.card),
                goal,
                synced_collateral: Some(synced_collateral(node, self.card)),
            },
        )
    }

    fn filing(&self, node: &Net, pursuit: &KernelPursuitV1) -> Option<(Obj, usize)> {
        let KernelGoalV1::Prove(proof) = &pursuit.goal else { return None };
        let o = det_route(node, self.card, &K::FileProof { accuser: pursuit.accuser(), claim: pursuit.claim, proof: proof.clone() });
        let len = borsh::to_vec(&o).unwrap().len();
        let cap = if self.chunks > 1 { len.div_ceil(self.chunks) } else { 50_000 };
        Some((o, cap))
    }

    /// The step the chain still needs, from `node`'s rows alone.
    pub(super) fn plan(&self, node: &Net, claim: &Digest, da: &Da) -> KernelPursuitStepV1 {
        let (state, pursuit) = self.derive(node, claim, da);
        let filing = self.filing(node, &pursuit);
        let f = filing.as_ref().map(|(o, cap)| KernelFilingV1 { signed: o, chunk_cap: *cap });
        plan_kernel_pursuit_v1(&state, &pursuit, f.as_ref()).expect("the plan reads the rows")
    }

    /// **One tick**: plan, then do it — sending at most `chunk_limit` of a filing's missing chunks (a crash mid-filing is a tick that
    /// sent fewer than it planned). Returns the step it planned.
    pub(super) async fn tick(&self, node: &mut Net, claim: &Digest, da: &Da, chunk_limit: usize) -> KernelPursuitStepV1 {
        let (state, pursuit) = self.derive(node, claim, da);
        let filing = self.filing(node, &pursuit);
        let f = filing.as_ref().map(|(o, cap)| KernelFilingV1 { signed: o, chunk_cap: *cap });
        let step = plan_kernel_pursuit_v1(&state, &pursuit, f.as_ref()).expect("the plan reads the rows");
        let card = self.card;
        match &step {
            KernelPursuitStepV1::Seal { seal, .. } => {
                let o = det_route(node, card, &K::SealProof { accuser: pursuit.accuser(), claim: *claim, seal: *seal });
                node.send(vec![(card, o)]).await;
            }
            KernelPursuitStepV1::AwaitSeal { file_from_daa, .. } => node.beat_to(*file_from_daa).await,
            KernelPursuitStepV1::FileWhole { .. } => {
                let (o, _) = filing.expect("a Prove pursuit files");
                node.send(vec![(card, o)]).await;
            }
            KernelPursuitStepV1::FileChunks { missing, .. } => {
                let (o, cap) = filing.expect("a Prove pursuit files");
                let chunks = palw_kernel_chunks_v1(&o, node.bond(card), PalwKernelChunkTargetV1::Claim(*claim), cap).expect("chunks");
                let send: Vec<(usize, Obj)> = chunks
                    .into_iter()
                    .filter(|c| missing.contains(&c.index))
                    .take(chunk_limit)
                    .map(|c| (card, det_chunk(node, card, c)))
                    .collect();
                node.send(send).await;
            }
            KernelPursuitStepV1::Demand { stage, position, .. } => {
                let o = det_route(
                    node,
                    card,
                    &K::FileDemand { demander: pursuit.accuser(), claim: *claim, stage: *stage, position: *position },
                );
                node.send(vec![(card, o)]).await;
            }
            KernelPursuitStepV1::AwaitAnswer { .. } | KernelPursuitStepV1::Recheck { .. } => {
                let next = node.daa() + 1;
                node.beat_to(next).await;
            }
            _ => {}
        }
        step
    }

    /// Ticks until the pursuit is over (or reports it cannot act); at most `cap` ticks.
    pub(super) async fn run(&self, node: &mut Net, claim: &Digest, da: &Da, cap: usize) -> KernelPursuitStepV1 {
        for _ in 0..cap {
            let step = self.tick(node, claim, da, usize::MAX).await;
            if matches!(
                step,
                KernelPursuitStepV1::Done(_)
                    | KernelPursuitStepV1::TooLate { .. }
                    | KernelPursuitStepV1::WouldBeDismissed { .. }
                    | KernelPursuitStepV1::Refused { .. }
                    | KernelPursuitStepV1::Stranded { .. }
            ) {
                return step;
            }
        }
        panic!("the pursuit of a claim reached no end in {cap} ticks: {:?}", self.plan(node, claim, da));
    }

    /// **Seal, then wait out the seal's precedence delay** (a tick that seals, then the ticks that plan `AwaitSeal`).
    pub(super) async fn seal_and_wait(&self, node: &mut Net, claim: &Digest, da: &Da) {
        let step = self.tick(node, claim, da, usize::MAX).await;
        assert!(matches!(step, KernelPursuitStepV1::Seal { .. }), "the pursuit opens with its seal: {step:?}");
        while matches!(self.plan(node, claim, da), KernelPursuitStepV1::AwaitSeal { .. }) {
            self.tick(node, claim, da, usize::MAX).await;
        }
    }

    /// The filing's group id (the chunk lane's) as this verifier would build it from `node`'s rows.
    pub(super) fn group(&self, node: &Net, claim: &Digest, da: &Da) -> Hash64 {
        let (_, pursuit) = self.derive(node, claim, da);
        let (o, cap) = self.filing(node, &pursuit).expect("a Prove pursuit");
        palw_kernel_chunks_v1(&o, node.bond(self.card), PalwKernelChunkTargetV1::Claim(*claim), cap).expect("chunks")[0].group
    }
}

pub(super) fn missing_of(step: &KernelPursuitStepV1) -> Vec<u8> {
    match step {
        KernelPursuitStepV1::FileChunks { missing, .. } => missing.clone(),
        other => panic!("a chunked filing step was expected: {other:?}"),
    }
}

/// A Panel-covered lying claim (every seat colluding) and the first bonded card outside its Panel.
pub(super) async fn covered_lie(w: &mut World) -> (Claim, usize) {
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let card = w.outsiders(&lie, &seats, 1)[0];
    (lie, card)
}

/// The settled end of a chunked conviction: one conviction, every chunk index carried exactly once, no group or deposit left.
pub(super) fn assert_convicted_once(net: &Net, card: usize, claim: &Digest, group: Hash64, chunks: usize) {
    assert!(net.ledger().claims[claim].convicted, "convicted");
    let carried = carried_chunks(net, card, group);
    assert_eq!(carried.len(), chunks, "every chunk rode: {carried:?}");
    assert!(carried.values().all(|n| *n == 1), "no chunk rode twice: {carried:?}");
    let route = net.api().unwrap();
    assert!(opener_groups_for_claim_v1(&route, &net.bond(card), claim).is_empty(), "the group assembled");
    assert_eq!(route.chunk_deposits_of_v1(&net.bond(card)), 0, "no chunk deposit left held");
    assert!(net.owed(card) > 0, "the accuser's share is queued once");
}

// ---- 1. the verifier restarts between every step -------------------------------------------------------------------------------

/// **K2-TIR-v2, Panel-licensed, every seat colluding: the verifier is restarted (a new process, nothing carried) after the seal and
/// after each chunk of a three-chunk filing.** Each restart re-derives the step the chain still needs: the seal is never re-sealed
/// (and the in-flight duplicate a crash would have re-sent is a no-op on the ledger: the seal keeps its DAA), each chunk rides once,
/// one group and one deposit, the conviction lands once, before Final, and inside the resume bound G14-B's carriage leaves.
#[tokio::test]
async fn g14r_kernel_v2_panel_the_pursuit_resumes_after_a_verifier_restart_at_every_step() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let (lie, card) = covered_lie(&mut w).await;
    let da = lie.published(&w.fx, &[]);
    let floor = match w.net.claim_state(&lie.id) {
        ClaimStateV1::ProbabilisticPass { window_end_daa, .. } => window_end_daa,
        s => panic!("passed: {s:?}"),
    };

    // Process 1: seals, then dies.
    Pursuer::new(card, &w.fx.params, 3).seal_and_wait(&mut w.net, &lie.id, &da).await;
    let kid = w.net.kid(card);
    let sealed_at = w.net.ledger().proof_seals[&(lie.id, kid)].daa;

    // Process 2: the seal is on chain; it plans the whole filing, sends one chunk, dies.
    let p2 = Pursuer::new(card, &w.fx.params, 3);
    let group = p2.group(&w.net, &lie.id, &da);
    assert_eq!(missing_of(&p2.plan(&w.net, &lie.id, &da)), vec![0, 1, 2], "no re-seal: the chain holds it");
    p2.tick(&mut w.net, &lie.id, &da, 1).await;
    // The duplicate a crash with the seal in flight would re-send: an identical re-seal changes nothing.
    let KernelGoalV1::Prove(proof) = p2.derive(&w.net, &lie.id, &da).1.goal else { panic!("a proof") };
    let reseal = det_route(&w.net, card, &K::SealProof { accuser: kid, claim: lie.id, seal: proof_seal_v1(&lie.id, &kid, &proof) });
    w.net.send(vec![(card, reseal)]).await;
    assert_eq!(w.net.ledger().proof_seals[&(lie.id, kid)].daa, sealed_at, "the re-sent seal kept its first DAA");

    // Process 3: chunk 0 is on chain under the same group; it sends chunk 1, dies.
    let p3 = Pursuer::new(card, &w.fx.params, 3);
    assert_eq!(p3.group(&w.net, &lie.id, &da), group, "deterministic carriers: the same group after a restart");
    assert_eq!(missing_of(&p3.plan(&w.net, &lie.id, &da)), vec![1, 2]);
    p3.tick(&mut w.net, &lie.id, &da, 1).await;

    // Process 4: one chunk left; inside its resume bound; completes the filing.
    let p4 = Pursuer::new(card, &w.fx.params, 3);
    assert_eq!(missing_of(&p4.plan(&w.net, &lie.id, &da)), vec![2]);
    let ledger = w.net.ledger();
    let (pre_final, within) = kernel_pursuit_resume_bound_v1(&ledger, &ledger.claims[&lie.id], 1);
    let now = w.net.daa();
    assert!(pre_final.is_some_and(|b| now <= b) && within.is_some_and(|b| now <= b), "inside the resume bound: {now} {pre_final:?}");
    let end = p4.run(&mut w.net, &lie.id, &da, 8).await;
    assert_eq!(end, KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
    assert_convicted_once(&w.net, card, &lie.id, group, 3);
    assert_eq!(carried_route_objects(&w.net, card, &lie.id, "SealProof"), 2, "the seal and the one duplicate a crash re-sent");

    // Process 5: nothing more to do; and the lie never reaches Final.
    assert_eq!(
        Pursuer::new(card, &w.fx.params, 3).plan(&w.net, &lie.id, &da),
        KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted)
    );
    w.net.beat_to(floor + 2).await;
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Convicted { .. }), "{:?}", w.net.claim_state(&lie.id));
    let z = w.net.replay().await;
    w.net.assert_same(&z, "a node replaying the resumed pursuit");
}

// ---- 2. the node restarts mid-filing ------------------------------------------------------------------------------------------

/// **A real node restart between the first and second chunk** (the node reopened over the same database): the route's rows come
/// off disk with the seal and the half-built group, and a verifier with no state finishes the same group.
#[tokio::test]
async fn g14r_kernel_v2_the_pursuit_resumes_after_a_node_restart_mid_filing() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over(|c| TestConsensus::with_db(db.clone(), c, sender));
    net._keep.push(Box::new(receiver));
    let mut w = World::on(net).await;
    let (lie, card) = covered_lie(&mut w).await;
    let da = lie.published(&w.fx, &[]);
    let p = Pursuer::new(card, &w.fx.params, 3);
    p.seal_and_wait(&mut w.net, &lie.id, &da).await;
    let group = p.group(&w.net, &lie.id, &da);
    p.tick(&mut w.net, &lie.id, &da, 1).await;
    let (sink, root) = (w.net.chain.sink(), w.net.chain.tip_state().1.state_root());

    let mut w = w.restart(db.clone());
    assert_eq!((w.net.chain.sink(), w.net.chain.tip_state().1.state_root()), (sink, root), "the tip off disk");
    let p = Pursuer::new(card, &w.fx.params, 3);
    assert_eq!(missing_of(&p.plan(&w.net, &lie.id, &da)), vec![1, 2], "the half-built group off disk");
    assert_eq!(p.run(&mut w.net, &lie.id, &da, 8).await, KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
    assert_convicted_once(&w.net, card, &lie.id, group, 3);
    let z = w.net.replay().await;
    w.net.assert_same(&z, "a node replaying across the restart");
}

// ---- 3. a reorg removes the partial filing, then the merge re-adds it ------------------------------------------------------------

/// **Node `Z` follows `A`, then reorgs onto a heavier branch `B` that never saw the last chunk** — inside the shipped
/// strict-economic-win rule's shallow-tie window (an all-economic tie reorgs only when the incumbent side spans ≤ 2 DAA, so the
/// reorged-out step is the last carrier: carrying block + folding block). On `B`'s tip the half-built group is gone and the plan
/// re-derives the whole filing (`[0, 1, 2]`). In a DAG the losing branch is not discarded: `Z`'s next block merges `A`'s block, whose
/// chunk carrier conflicts with nothing, so the part comes back — and the plan re-derives again from the new tip (`[1, 2]`), never
/// from a cache. The verifier's funding is the virtual UTXO set's (`A`'s change: its carrier sits in the virtual's merge set). The
/// filing completes once: one conviction, one group, no deposit left held.
#[tokio::test]
async fn g14r_kernel_v2_a_reorg_removes_the_last_chunk_the_merge_re_adds_it_and_the_pursuit_re_derives_each_time() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let (lie, card) = covered_lie(&mut w).await;
    let da = lie.published(&w.fx, &[]);
    let p = Pursuer::new(card, &w.fx.params, 3);
    p.seal_and_wait(&mut w.net, &lie.id, &da).await;
    let fork = w.net.chain.sink();
    p.tick(&mut w.net, &lie.id, &da, 1).await;
    assert_eq!(missing_of(&p.plan(&w.net, &lie.id, &da)), vec![1, 2], "A: the seal and chunk 0");

    let mut zn = reorged_node(&w.net, fork, w.net.funding.clone()).await;
    assert!(zn.ledger().proof_seals.contains_key(&(lie.id, zn.kid(card))), "the seal is below the fork");
    assert!(opener_groups_for_claim_v1(&zn.api().unwrap(), &zn.bond(card), &lie.id).is_empty(), "B's tip holds no group");
    assert_eq!(missing_of(&p.plan(&zn, &lie.id, &da)), vec![0, 1, 2], "the pursuit re-derives from B's tip: the whole filing");
    // Z's next block merges A's block (its chunk carrier conflicts with nothing).
    let next = zn.daa() + 1;
    zn.beat_to(next).await;
    let after_merge = missing_of(&p.plan(&zn, &lie.id, &da));
    eprintln!("[g14r] after Z's next block the plan reads {after_merge:?} (A's chunk merged back: {})", after_merge == vec![1, 2]);
    assert!(after_merge == vec![1, 2] || after_merge == vec![0, 1, 2], "{after_merge:?}");
    assert_eq!(p.run(&mut zn, &lie.id, &da, 12).await, KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
    assert!(zn.ledger().claims[&lie.id].convicted);
    let route = zn.api().unwrap();
    assert!(opener_groups_for_claim_v1(&route, &zn.bond(card), &lie.id).is_empty(), "the group assembled");
    assert_eq!(route.chunk_deposits_of_v1(&zn.bond(card)), 0, "no chunk deposit left held");
    assert!(zn.owed(card) > 0, "the accuser's share is queued");
    let z = dag_ibd_node(&zn).await;
    zn.assert_same(&z.chain, "a node syncing the DAG the pursuit finished on");
}

/// **Every block of `net`'s DAG below its sink, parents first** (a node whose history merged a losing branch holds blocks off its
/// selected chain: a syncing peer receives them too). Ordered by blue score — a parent's is always lower than its child's.
pub(super) fn dag_blocks(net: &Net) -> Vec<Block> {
    let genesis = net.config.params.genesis.hash;
    let (mut seen, mut stack, mut out) = (std::collections::BTreeSet::new(), vec![net.chain.sink()], Vec::new());
    while let Some(h) = stack.pop() {
        if h == genesis || !seen.insert(h) {
            continue;
        }
        let block = net.chain.ctx.consensus.get_block(h).expect("the node holds its DAG");
        stack.extend(block.header.direct_parents().iter().copied());
        out.push(block);
    }
    out.sort_by_key(|b| (b.header.blue_score, b.header.hash));
    out
}

/// A node that syncs `main`'s whole DAG from genesis (an IBD that also receives the merged, off-chain blocks).
pub(super) async fn dag_ibd_node(main: &Net) -> Net {
    let z = t12_genesis_chain(&main.config, &main.bundle, &main.premine, &main.floats);
    for b in dag_blocks(main) {
        arrive(&z, b, "a block of the first node's DAG").await;
    }
    let mut node = main.on_chain(z);
    node.chain.ctx.simulated_time = main.chain.ctx.simulated_time;
    agree(main, &node, "the DAG-IBD node");
    node
}

/// **Node `Z`**: replays `main`, then takes a heavier branch `B` mined from `fork` that carries nothing, and reorgs onto it. Its
/// actors' funding is `funding_at_fork` (B spent none of their outputs).
pub(super) async fn reorged_node(main: &Net, fork: kaspa_hashes::Hash64, funding_at_fork: Premine) -> Net {
    let z = main.replay().await;
    let mut zn = main.on_chain(z);
    zn.chain.ctx.simulated_time = main.chain.ctx.simulated_time;
    let mut b = t12_genesis_chain(&main.config, &main.bundle, &main.premine, &main.floats);
    let up = chain_blocks(&main.chain, fork);
    let fork_timestamp = up.last().unwrap().header.timestamp;
    for blk in up {
        arrive(&b, blk, "a block up to the fork").await;
    }
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = main.ttpb();
    let a_work = main.chain.ctx.consensus.get_block(main.chain.sink()).unwrap().header.blue_work;
    let mut b_blocks = Vec::new();
    while b.ctx.consensus.get_block(b.sink()).unwrap().header.blue_work <= a_work {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    for blk in &b_blocks {
        arrive(&zn.chain, blk.clone(), "B's block").await;
    }
    assert_eq!(zn.chain.sink(), b.sink(), "B out-works A: Z reorgs onto B");
    zn.chain.ctx.simulated_time = zn.chain.ctx.simulated_time.max(b.ctx.simulated_time);
    zn.funding = funding_at_fork;
    zn
}

// ---- 4. full IBD: a node started after the seal and a partial filing ----------------------------------------------------------

/// **A node that syncs every block from genesis after the verifier sealed and sent one chunk**: the verifier, now on that node (its
/// previous node gone), reads the same rows and finishes the same group; the two nodes end on one root.
#[tokio::test]
async fn g14r_kernel_v2_an_ibd_node_started_after_the_seal_and_a_partial_filing_finishes_the_same_pursuit() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let (lie, card) = covered_lie(&mut w).await;
    let da = lie.published(&w.fx, &[]);
    let p = Pursuer::new(card, &w.fx.params, 3);
    let group = p.group(&w.net, &lie.id, &da);
    p.seal_and_wait(&mut w.net, &lie.id, &da).await;
    p.tick(&mut w.net, &lie.id, &da, 1).await;
    let mut node = ibd_node(&w.net).await;
    assert_eq!(missing_of(&p.plan(&node, &lie.id, &da)), vec![1, 2], "the IBD node serves the seal and the half-built group");
    assert_eq!(p.run(&mut node, &lie.id, &da, 8).await, KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
    assert_convicted_once(&node, card, &lie.id, group, 3);
    sync(&node, &mut w.net).await;
    agree(&w.net, &node, "the IBD node that finished the pursuit");
    assert!(w.net.ledger().claims[&lie.id].convicted);
}

/// **A node that syncs every block after the producer SERVED a demanded position**: the withheld value was demanded by the verifier on
/// its old node and served on chain; on the IBD node the verifier's re-check reads the served position off the rows (public since its
/// service), finds the lie and files. No second demand rides: the position is public.
#[tokio::test]
async fn g14r_kernel_v2_an_ibd_node_started_after_the_served_answer_convicts_from_it() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let (lie, card) = covered_lie(&mut w).await;
    let da = lie.published(&w.fx, &[lie.at]);
    let p = Pursuer::new(card, &w.fx.params, 1);
    let step = p.tick(&mut w.net, &lie.id, &da, usize::MAX).await;
    assert!(
        matches!(step, KernelPursuitStepV1::Demand { stage: 0, position, .. } if position == lie.at.0),
        "the withheld value is demanded: {step:?}"
    );
    w.serve(0, &lie, lie.at.0).await;
    let mut node = ibd_node(&w.net).await;
    let step = p.plan(&node, &lie.id, &da);
    assert!(matches!(step, KernelPursuitStepV1::Seal { .. }), "the served value convicts: {step:?}");
    assert_eq!(p.run(&mut node, &lie.id, &da, 8).await, KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
    assert_eq!(carried_route_objects(&node, card, &lie.id, "FileDemand"), 1, "one demand, never re-filed");
    sync(&node, &mut w.net).await;
    agree(&w.net, &node, "the IBD node");
}

// ---- 5. pruned import: a node started after the demand ------------------------------------------------------------------------

/// **OPV (no Panel): a node that installs the PALW carriage at a pruning point AFTER the verifier's demand, then the blocks after
/// it.** The verifier on it plans `AwaitAnswer` with the demand's own deadline (no re-demand: the demand row lists it once), the
/// producer stays silent, and at the deadline the chain itself defaults the claim; the verifier reads `Defaulted`.
#[tokio::test]
async fn g14r_kernel_v2_opv_a_pruned_import_node_started_after_the_demand_reads_the_default() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::opv().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let card = 2;
    let da = lie.published(&w.fx, &[lie.at]);
    let p = Pursuer::new(card, &w.fx.params, 1);
    assert!(matches!(p.tick(&mut w.net, &lie.id, &da, usize::MAX).await, KernelPursuitStepV1::Demand { .. }));
    let mut node = pruned_import_node(&mut w.net).await;
    let KernelPursuitStepV1::AwaitAnswer { deadline_daa, .. } = p.plan(&node, &lie.id, &da) else {
        panic!("the imported rows hold the open demand: {:?}", p.plan(&node, &lie.id, &da))
    };
    let kid = node.kid(card);
    let demand = node.ledger().demands[&(lie.id, 0, lie.at.0)].clone();
    assert_eq!(demand.demanders.iter().filter(|(b, _)| *b == kid).count(), 1, "demanded once");
    node.beat_to(deadline_daa).await;
    assert_eq!(p.plan(&node, &lie.id, &da), KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Defaulted));
    assert!(matches!(node.claim_state(&lie.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }));
    assert_eq!(carried_route_objects(&node, card, &lie.id, "FileDemand"), 1, "never re-filed");
    sync(&node, &mut w.net).await;
    agree(&w.net, &node, "the pruned-import node");
}

// ---- 6. deadlines: the resume bound is honoured -------------------------------------------------------------------------------

/// **G14-B: how long may the verifier be down?** Three lies of one Panel world, each sealed, then the verifier stops:
/// * it comes back AT its pre-Final resume bound (the last DAA its remaining carriage still lands before Final): it convicts before
///   Final, and the claim never finalizes;
/// * it comes back after the claim's Final, inside liability: it convicts after Final (the liability's slash);
/// * it comes back after the liability horizon: the plan is `TooLate`, nothing is sent and nothing is charged.
#[tokio::test]
async fn g14r_kernel_v2_a_resumed_pursuit_meets_its_deadline_inside_the_resume_bound_and_sends_nothing_past_it() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    // (a) down until the pre-Final bound.
    let (lie, card) = covered_lie(&mut w).await;
    let da = lie.published(&w.fx, &[]);
    let p = Pursuer::new(card, &w.fx.params, 1);
    assert!(matches!(p.tick(&mut w.net, &lie.id, &da, usize::MAX).await, KernelPursuitStepV1::Seal { .. }));
    let ledger = w.net.ledger();
    let (pre_final, _) = kernel_pursuit_resume_bound_v1(&ledger, &ledger.claims[&lie.id], 1);
    let bound = pre_final.expect("a pre-Final bound");
    w.net.beat_to(bound).await;
    assert!(w.net.daa() >= bound && w.net.daa() <= bound + 1, "resumed at the bound: {} vs {bound}", w.net.daa());
    assert_eq!(p.run(&mut w.net, &lie.id, &da, 4).await, KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Convicted { .. }), "before Final: {:?}", w.net.claim_state(&lie.id));

    // (b) down past Final, inside liability.
    let (lie2, card2) = covered_lie(&mut w).await;
    let da2 = lie2.published(&w.fx, &[]);
    let p2 = Pursuer::new(card2, &w.fx.params, 1);
    assert!(matches!(p2.tick(&mut w.net, &lie2.id, &da2, usize::MAX).await, KernelPursuitStepV1::Seal { .. }));
    let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&lie2.id) else { panic!("passed") };
    w.net.beat_to(window_end_daa + 1).await;
    assert!(matches!(w.net.claim_state(&lie2.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&lie2.id));
    let ledger = w.net.ledger();
    let (pre, within) = kernel_pursuit_resume_bound_v1(&ledger, &ledger.claims[&lie2.id], 1);
    assert!(pre.is_none() && within.is_some_and(|b| w.net.daa() <= b), "{pre:?} {within:?}");
    assert_eq!(p2.run(&mut w.net, &lie2.id, &da2, 4).await, KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
    assert!(w.net.ledger().claims[&lie2.id].convicted && matches!(w.net.claim_state(&lie2.id), ClaimStateV1::Final { .. }));

    // (c) down past liability: nothing to send.
    let (lie3, card3) = covered_lie(&mut w).await;
    let da3 = lie3.published(&w.fx, &[]);
    let p3 = Pursuer::new(card3, &w.fx.params, 1);
    assert!(matches!(p3.tick(&mut w.net, &lie3.id, &da3, usize::MAX).await, KernelPursuitStepV1::Seal { .. }));
    let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&lie3.id) else { panic!("passed") };
    w.net.beat_to(window_end_daa + 1).await;
    let until = w.net.ledger().claims[&lie3.id].liability_until.expect("Final sets the horizon");
    w.net.beat_to(until + 1).await;
    let (collateral, objects) = (w.net.collateral(card3), carried_objects(&w.net).len());
    let step = p3.run(&mut w.net, &lie3.id, &da3, 2).await;
    assert!(matches!(step, KernelPursuitStepV1::TooLate { .. }), "{step:?}");
    assert_eq!((w.net.collateral(card3), carried_objects(&w.net).len()), (collateral, objects), "nothing sent, nothing charged");
}
