//! T14 the complete shared path (demand -> response -> proof / default -> settlement) across an actual reorg, a restart over the same
//! database, a pruned import and an IBD node.
use super::super::canonical::{agree, ibd_node, pruned_import_node, sync};
use super::*;

/// **T14a: restart over the same database in the MIDDLE of the shared path.** The demand is open when the node stops; the reopened
/// node loads the open session off disk, the producer's response and the outsider's proof are folded by the restarted node, and a
/// node replaying the whole chain agrees root for root.
#[tokio::test]
async fn t14_shared_path_survives_a_restart_with_an_open_demand() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over(|c| TestConsensus::with_db(db.clone(), c, sender));
    net._keep.push(Box::new(receiver));
    let mut w = World::on(net).await;
    let (lie, seats) = covered_claim(&mut w, 0, true).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    w.demand(outsider, &lie.id, lie.at.0).await;
    let open = w.net.ledger().demands[&(lie.id, 0, lie.at.0)].clone();
    let (sink, root) = (w.net.chain.sink(), w.net.chain.tip_state().1.state_root());

    let mut w = w.restart(db.clone());
    assert_eq!((w.net.chain.sink(), w.net.chain.tip_state().1.state_root()), (sink, root), "the stopped node's tip, off disk");
    assert_eq!(w.net.ledger().demands[&(lie.id, 0, lie.at.0)], open, "the open demand survived the restart");
    w.serve(0, &lie, lie.at.0).await;
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[lie.at]), 0x14);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "the restarted node completes the shared path");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "a node replaying across the restart");
}

/// **T14b: a reorg across the shared path's decisive object.** From a fork with the demand open, branch A carries the producer's
/// response (answered) and branch B — heavier — does not, so on B the deadline defaults the producer. A node following A then B, and a
/// node following B then A, each end on the winning branch's exact state: the default and its penalty appear with B and vanish with
/// A, the response row the reverse; nothing of a reorged-out branch survives.
///
/// Studio T4 (2026-10-22): the first version forked 6 DAA before the deadline and let A run 8+ blocks past it; the node then refused
/// the heavier bare B (`palw_reorg_strict_economic_win`: an all-economic tie deeper than `PALW_REORG_SHALLOW_TIE_DAA_V1` = 2 DAA
/// keeps the incumbent — the deep-reorg rule, "the heavier B wins" failed). A reorg between these two branches carries no economic
/// difference (frontier/safe/live 0 on both), so it is only decidable by GHOSTDAG inside the shallow window. The fork is therefore
/// taken at `deadline − 2`: A's response folds no later than the deadline (the last lawful block, see T07), so A's incumbent side spans at
/// most 2 DAA; B' (B up to the deadline block, whose tick defaults the producer) spans exactly 2. Each direction is exercised on its
/// own node with a shallow incumbent and a heavier challenger, and the deep tie (B spanning 3 DAA) is asserted to keep its incumbent
/// against a heavier A.
#[tokio::test]
async fn t14_shared_path_reorgs_between_answered_and_defaulted_branches_exactly() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let pol = w.policy();
    let (lie, seats) = covered_claim(&mut w, 0, true).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    w.demand(outsider, &lie.id, lie.at.0).await;
    let deadline = w.net.ledger().demands[&(lie.id, 0, lie.at.0)].deadline_daa;
    let before = w.net.collateral(0);
    w.net.beat_to(deadline - 2).await;
    assert_eq!(w.net.daa(), deadline - 2, "the fork is two DAA before the deadline");
    let fork = w.net.chain.sink();
    let fork_len = chain_blocks(&w.net.chain, fork).len();
    let z = w.net.replay().await;
    let zn = w.net.on_chain(z); // follows A, then B
    let z = w.net.replay().await;
    let zm = w.net.on_chain(z); // follows B, then A

    // ---- A: the producer answers; the response folds at or before the deadline (a heartbeat may not advance the DAA) ----
    w.serve(0, &lie, lie.at.0).await;
    assert!(w.net.daa() <= deadline, "A's response folds no later than the deadline (its last lawful block): {}", w.net.daa());
    assert!(w.net.ledger().served.contains_key(&(lie.id, 0, lie.at.0)), "A: answered");
    assert_eq!(w.net.collateral(0), before, "A: answered, no penalty");
    let after_fork = |net: &Net| -> Vec<Block> { chain_blocks(&net.chain, net.chain.sink()).into_iter().skip(fork_len).collect() };
    let a_short = after_fork(&w.net);

    // ---- B: from the fork, never sees the response: the deadline block's tick defaults the producer. B runs one DAA past the
    // deadline (so that, as an incumbent, it spans 3 DAA — deeper than the shallow window) and is heavier than A ----
    let b = t12_genesis_chain(&w.net.config, &w.net.bundle, &w.net.premine, &w.net.floats);
    let up_to_fork = chain_blocks(&w.net.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for blk in up_to_fork {
        arrive(&b, blk, "a block up to the fork").await;
    }
    let mut b = b;
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = w.net.ttpb();
    let mut b_blocks: Vec<Block> = Vec::new();
    while b.daa_of(b.sink()) <= deadline || b_blocks.len() <= a_short.len() {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    // B' = B up to its first block at the deadline: spans exactly 2 DAA above the fork, and its tick has defaulted the producer.
    let k0 = b_blocks.iter().position(|blk| blk.header.daa_score >= deadline).expect("B reaches the deadline");
    assert_eq!(b_blocks[k0].header.daa_score, deadline, "B' ends at the deadline block");

    // ---- node zn: incumbent A (<= 2 DAA above the fork), then B (heavier): B wins inside the shallow window ----
    for blk in &a_short {
        arrive(&zn.chain, blk.clone(), "A's block").await;
    }
    assert_eq!(zn.chain.sink(), w.net.chain.sink(), "zn follows A first");
    for blk in &b_blocks {
        arrive(&zn.chain, blk.clone(), "B's block").await;
    }
    assert_eq!(zn.chain.sink(), b.sink(), "the heavier B wins");
    assert!(
        matches!(zn.claim_state(&lie.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
        "on B the producer defaulted: {:?}",
        zn.claim_state(&lie.id)
    );
    assert!(!zn.ledger().served.contains_key(&(lie.id, 0, lie.at.0)), "A's response is gone");
    assert_eq!(zn.collateral(0), before - pol.default_penalty, "B's penalty applies");

    // ---- A is extended until it is heavier than the whole of B ----
    while after_fork(&w.net).len() <= b_blocks.len() {
        w.net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    let a_long = after_fork(&w.net);

    // ---- node zm: incumbent B' (2 DAA, defaulted), then the heavier A: back on A, identically ----
    for blk in &b_blocks[..=k0] {
        arrive(&zm.chain, blk.clone(), "B's block").await;
    }
    assert_eq!(zm.chain.sink(), b_blocks[k0].header.hash, "zm follows B' first");
    assert!(
        matches!(zm.claim_state(&lie.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
        "on B's deadline block the producer defaulted: {:?}",
        zm.claim_state(&lie.id)
    );
    assert_eq!(zm.collateral(0), before - pol.default_penalty);
    for blk in &a_long {
        arrive(&zm.chain, blk.clone(), "A's block").await;
    }
    w.net.assert_same(&zm.chain, "Z back on A");
    assert!(zm.ledger().served.contains_key(&(lie.id, 0, lie.at.0)), "the response is back");
    assert_eq!(zm.collateral(0), before, "the penalty vanished with B");

    // ---- the deep tie keeps its incumbent: on zn, B spans 3 DAA; the heavier A does not move the sink (blue work does not decide) ----
    assert!(b.daa_of(b.sink()) > deadline, "B spans more than the shallow window above the fork");
    for blk in &a_long {
        if zn.chain.ctx.consensus.get_block(blk.header.hash).is_err() {
            arrive(&zn.chain, blk.clone(), "A's later block").await;
        }
    }
    assert_eq!(zn.chain.sink(), b.sink(), "an all-economic tie deeper than the shallow window keeps the incumbent");
}

/// **T14c: a pruned import and a fresh IBD node in the middle of the shared path.** A node that joins from a pruning point taken while
/// the demand is open, and a node that syncs the whole chain, each continue the path themselves: the producer's response and the
/// outsider's proof are folded through the PRUNED node's own mempool and template; the main node and the IBD node take its blocks;
/// all three agree root for root, and the conviction is identical everywhere.
#[tokio::test]
async fn t14_shared_path_continues_on_a_pruned_import_node_and_an_ibd_node() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let (lie, seats) = covered_claim(&mut w, 0, true).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    w.demand(outsider, &lie.id, lie.at.0).await;
    let mut pruned = pruned_import_node(&mut w.net).await;
    assert!(pruned.ledger().demands.contains_key(&(lie.id, 0, lie.at.0)), "the open demand came through the carriage");
    let mut ibd = ibd_node(&w.net).await;

    // The pruned node carries the rest of the path.
    let bytes = lie.position(&w.fx, lie.at.0, |_| {});
    let o = pruned.route(0, &K::Respond { claim: lie.id, stage: 0, position: lie.at.0, bytes });
    pruned.send(vec![(0, o)]).await;
    let api = pruned.api().expect("route");
    let fresh = Fresh::from_api(&api, api.ledger_root(), 0x1C);
    let OutsiderFindingV1::Prosecute(proof) = fresh.check(lie.id, &lie.published(&w.fx, &[lie.at]), &w.fx.params) else {
        panic!("the pruned node's read API is enough to prosecute")
    };
    let o = pruned.route(outsider, &K::FileProof { accuser: pruned.kid(outsider), claim: lie.id, proof });
    pruned.send(vec![(outsider, o)]).await;
    assert!(pruned.ledger().claims[&lie.id].convicted, "convicted by the pruned node's own fold");

    sync(&pruned, &mut w.net).await;
    sync(&pruned, &mut ibd).await;
    // Studio T4 (2026-10-22): the first run compared with `pruned.assert_same(..)`, which walks the PRUNED node's chain to genesis and
    // reads a delta row for every block — rows a pruned import does not hold at or below its pruning point (KeyNotFound). The
    // pruned node is compared the way the harness's own pruned-import tests compare it (`canonical::agree`: sink, state root, kernel
    // route rows, and every delta row it holds), plus its complete virtual UTXO set; the two full nodes keep the full comparison.
    agree(&w.net, &pruned, "main after the pruned node's blocks");
    agree(&ibd, &pruned, "the IBD node after the pruned node's blocks");
    w.net.assert_same(&ibd.chain, "main vs the IBD node after the pruned node's blocks");
    assert_eq!(
        pruned.chain.ctx.consensus.get_virtual_parents(),
        w.net.chain.ctx.consensus.get_virtual_parents(),
        "same virtual parents"
    );
    assert_eq!(Net::utxos_of(&pruned.chain), Net::utxos_of(&w.net.chain), "the pruned node: same complete virtual UTXO set");
    assert!(w.net.ledger().claims[&lie.id].convicted && ibd.ledger().claims[&lie.id].convicted);
}

/// **T14 (legacy pruning regression)**: the specific LG14-A legacy pruned-import regression named in the completion ledger must pass
/// unchanged — it is the existing node test
/// `lg14a_legacy_filer_e2e::g14r_resume::g14r_lg14a_a_pruned_import_after_the_reservation_and_binding_carries_the_pursuit_to_the_conviction`;
/// this campaign runs it by name (central verification: `cargo test -p kaspa-consensus g14r_lg14a_a_pruned_import_after`), it is not
/// duplicated here.
///
/// **T14 (full shared original)**: the same three crossings for the native shared-original path — the shared-court op, the paid
/// ingress owner rows, the escrow settlement rows and the position terminal rows all ride the pruning carriage and reorg exactly.
// MERGED on g14/integration-r7 (studio T4) — the remaining blocker is the #[ignore] reason; was: DEPENDS ON lane r5-f02-wire, r5-f03-ingress, r5-escrow-settlement, r5-p2-terminal.
#[tokio::test]
#[ignore = "blocked on values V2+V3+V4+V8 and a node-level shared original (lanes merged on r7)"]
async fn t14_native_shared_original_rows_survive_reorg_restart_pruning_and_pruned_ibd() {
    #[cfg(feature = "g14-t-campaign-intended-api")]
    {
        let mut w = World::new().await;
        let (lie, _) = covered_claim(&mut w, 0, true).await;
        let mut pruned = pruned_import_node(&mut w.net).await;
        let a = w.net.chain.tip_state().1;
        let b = pruned.chain.tip_state().1;
        assert_eq!(a.palw_shared_duty_v1(&lie.id), b.palw_shared_duty_v1(&lie.id));
        assert_eq!(a.palw_paid_ingress_owners_v1(&lie.id), b.palw_paid_ingress_owners_v1(&lie.id));
        assert_eq!(a.palw_original_escrow_outcome_v1(&lie.id), b.palw_original_escrow_outcome_v1(&lie.id));
        assert_eq!(a.palw_position_terminal_v1(&lie.id), b.palw_position_terminal_v1(&lie.id));
        let _ = &mut pruned;
    }
    #[cfg(not(feature = "g14-t-campaign-intended-api"))]
    blocked_on!(
        "none of the four row kinds (shared-court rows under a CourtSharedContext, paid-ingress reservations, funded escrow rows, position-terminal rows) can be created on a node (no production CourtSharedContext installer; V2, V3, V4, V8 typed-missing); intended palw_shared_duty_v1 / palw_paid_ingress_owners_v1 / palw_original_escrow_outcome_v1 / palw_position_terminal_v1 do not exist"
    );
}
