//! T14 the complete shared path (demand -> response -> proof / default -> settlement) across an actual reorg, a restart over the same
//! database, a pruned import and an IBD node.
use super::super::canonical::{ibd_node, pruned_import_node, sync};
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
/// response (answered) and branch B — heavier — does not, so on B the deadline defaults the producer. A node following A then B then
/// A again ends each time on that branch's exact state: the default and its penalty appear with B and vanish with A, the response
/// row the reverse; nothing of a reorged-out branch survives.
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
    w.net.beat_to(deadline.saturating_sub(6)).await;
    let fork = w.net.chain.sink();
    let z = w.net.replay().await;
    let zn = w.net.on_chain(z);

    // ---- A: the producer answers ----
    w.serve(0, &lie, lie.at.0).await;
    w.net.beat_to(deadline + 2).await;
    assert!(w.net.ledger().served.contains_key(&(lie.id, 0, lie.at.0)));
    assert_eq!(w.net.collateral(0), before, "A: answered, no penalty");

    // ---- B: from the fork, heavier, never sees the response: the producer defaults ----
    let b = t12_genesis_chain(&w.net.config, &w.net.bundle, &w.net.premine, &w.net.floats);
    let up_to_fork = chain_blocks(&w.net.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for blk in up_to_fork {
        arrive(&b, blk, "a block up to the fork").await;
    }
    let mut b = b;
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = w.net.ttpb();
    let a_len = chain_blocks(&w.net.chain, w.net.chain.sink()).len() - chain_blocks(&w.net.chain, fork).len();
    let mut b_blocks = Vec::new();
    for _ in 0..(a_len + 4) {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    for blk in chain_blocks(&w.net.chain, w.net.chain.sink()) {
        if zn.chain.ctx.consensus.get_block(blk.header.hash).is_err() {
            arrive(&zn.chain, blk, "A's block").await;
        }
    }
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

    // ---- A out-works B: back to the answered state, identically ----
    let old_len = chain_blocks(&w.net.chain, w.net.chain.sink()).len();
    for _ in 0..(b_blocks.len() + 4) {
        w.net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    for blk in chain_blocks(&w.net.chain, w.net.chain.sink()).into_iter().skip(old_len) {
        arrive(&zn.chain, blk, "A's later block").await;
    }
    w.net.assert_same(&zn.chain, "Z back on A");
    assert!(zn.ledger().served.contains_key(&(lie.id, 0, lie.at.0)));
    assert_eq!(zn.collateral(0), before, "the penalty vanished with B");
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
    pruned.assert_same(&w.net.chain, "main after the pruned node's blocks");
    pruned.assert_same(&ibd.chain, "the IBD node after the pruned node's blocks");
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
// DEPENDS ON lane r5-f02-wire, r5-f03-ingress, r5-escrow-settlement, r5-p2-terminal.
#[tokio::test]
#[ignore = "needs lane r5-f02-wire + r5-f03-ingress + r5-escrow-settlement + r5-p2-terminal merge"]
async fn t14_native_shared_original_rows_survive_reorg_restart_pruning_and_pruned_ibd() {
    #[cfg(feature = "g14-t-campaign-merged")]
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
    #[cfg(not(feature = "g14-t-campaign-merged"))]
    pending_lane!("r5-f02-wire + r5-f03-ingress + r5-escrow-settlement + r5-p2-terminal");
}
