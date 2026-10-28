//! **Lane ARM-LEGACY (2026-10-27): stage (d) for the legacy route families** — the held-DA default (b) and the honest twin (c) across
//! a node restart (same database), a reorg and a full IBD / pruned import, on the parent's validated isolated G14 network
//! (LG14-A + LG14-B + option (b) armed at genesis through normal `Params` validation).
//!
//! The conviction (a) across restart / reorg / IBD / pruned import is the parent's `lg14a_the_pursuit_survives_a_node_restart` and
//! `g14r_resume` cases (same fold rules) plus the per-block replay roots asserted at the end of
//! `legacy_v2_attempt_full_collusion_outside_bond_convicts[_with_court_capacity]`.
//!
//! Silence is a DA default here, never a computation conviction (`court_convicted` / `executor_refuted` stay false). No cryptographic
//! computation proof of any kind is used (PRINCIPLES §8).
use super::*;
use kaspa_consensus_core::BlockHash;
use kaspa_database::{create_temp_db, prelude::ConnBuilder};

fn withhold(unit: &PalwDaUnitV1) -> bool {
    matches!(unit, PalwDaUnitV1::LegacyHeldV2(_))
}

/// Reserve and read the binding (the producer answers it), then send the reserved demand of the step tree's root node (tag 157).
/// Returns the verifier and the open session's deadline.
async fn reserve_bind_and_demand(lg: &mut Lg, produced: &Produced) -> (Verifier, u64) {
    let id = produced.claim_id;
    let newcomer = lg.bond(NEWCOMER);
    let mut v = Verifier::check(lg, NEWCOMER, id);
    for _ in 0..8 {
        if v.binding.is_some() {
            break;
        }
        v.step(lg, produced, &withhold).await;
    }
    let deadline = demand_root(lg, &v, id).await;
    assert!(lg.view(id).record.is_some_and(|r| r.live.contains_key(&newcomer)), "reserved");
    (v, deadline)
}

/// Send the reserved held demand of the root node; returns the session's deadline.
async fn demand_root(lg: &mut Lg, v: &Verifier, id: Hash64) -> u64 {
    let newcomer = lg.bond(NEWCOMER);
    let binding = v.binding.clone().expect("the binding, read off the producer's authenticated answer");
    let height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(binding.step_leaf_count);
    let unit = PalwLegacyHeldUnitV2::StepNode { level: height, index: 0 };
    let object = v.demand(lg, PalwLegacyProbeV1::HeldNode { unit });
    assert!(matches!(object, Obj::LegacyHeldDemandedV2 { .. }), "tag 157");
    lg.send(vec![(NEWCOMER, object)]).await;
    lg.view(id)
        .sessions
        .iter()
        .find(|(accuser, session)| *accuser == newcomer && session.units.contains(&PalwDaUnitV1::LegacyHeldV2(unit)))
        .map(|(_, session)| session.deadline_daa)
        .expect("the reserved held session is open")
}

/// The producer stays silent on the held unit past its deadline (it answers everything else); asserts the objective DA default.
async fn silent_to_default(lg: &mut Lg, produced: &Produced, deadline: u64) {
    let id = produced.claim_id;
    let producer_before = lg.collateral(EXECUTOR);
    lg.beat_to(deadline + 2).await;
    for _ in 0..4 {
        if lg.view(id).da_defaulted {
            break;
        }
        lg.producer_answers(produced, &withhold).await;
        lg.beat(1).await;
    }
    let view = lg.view(id);
    assert!(view.da_defaulted, "the objective DA default");
    assert!(
        matches!(
            view.phase,
            PalwClaimPhaseV2::Voided { reason: kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::ProducerWithholding, .. }
        ),
        "voided ProducerWithholding: {:?}",
        view.phase
    );
    assert!(!view.court_convicted && !view.executor_refuted, "silence is never a presumed computation fault");
    assert!(lg.collateral(EXECUTOR) < producer_before, "the withholding producer is charged");
    assert!(lg.view(id).record.is_none(), "the default released the reservation");
    assert_eq!(lg.exposure(NEWCOMER), 0, "the outsider is made whole");
}

/// Stop the node and reopen it over the same database; asserts the sink, the PALW tip and every delta row survived.
fn restart(lg: Lg, db: &std::sync::Arc<kaspa_database::prelude::DB>) -> Lg {
    let sink = lg.chain.sink();
    let root = lg.chain.tip_state().1.state_root();
    let deltas: Vec<(BlockHash, Hash64)> =
        chain_blocks(&lg.chain, sink).iter().map(|b| (b.header.hash, root_at(&lg.chain, b.header.hash))).collect();
    let Lg { chain, config, bundle, premine, floats, funding, domain, backend, newcomer, nonce, rnd, _keep } = lg;
    let (simulated_time, heartbeat_nonce) = (chain.ctx.simulated_time, chain.nonce_for_reopen());
    drop(chain);
    let mut resumed = config.clone();
    resumed.process_genesis = false;
    let (sender, receiver) = async_channel::unbounded();
    let second = TestConsensus::with_db(db.clone(), &resumed, sender);
    let chain = t12_reopened_chain(second, &resumed, &bundle, simulated_time, heartbeat_nonce);
    let mut keep = _keep;
    keep.push(Box::new(receiver));
    let lg = Lg { chain, config, bundle, premine, floats, funding, domain, backend, newcomer, nonce, rnd, _keep: keep };
    assert_eq!(lg.chain.sink(), sink, "the sink survived the restart");
    assert_eq!(lg.chain.tip_state().1.state_root(), root, "the PALW tip, off disk");
    for (hash, want) in deltas {
        assert_eq!(root_at(&lg.chain, hash), want, "the delta row of {hash} survived");
    }
    lg
}

/// A fresh node replaying `lg`'s selected chain from genesis reaches the same root at every block.
async fn ibd_replays_the_same_roots(lg: &Lg) -> T12Chain {
    let z = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    for block in chain_blocks(&lg.chain, lg.chain.sink()) {
        let hash = block.header.hash;
        arrive(&z, block, "the first node's block").await;
        assert_eq!(root_at(&z, hash), root_at(&lg.chain, hash), "the IBD node's root at {hash}");
    }
    z
}

/// A node importing the PALW carriage at `p` (on `lg`'s chain) and taking the blocks after it; returns it at `lg`'s sink.
async fn pruned_import_at(lg: &Lg, p: BlockHash) -> T12Chain {
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    let all = chain_blocks(&lg.chain, lg.chain.sink());
    let k = all.iter().position(|b| b.header.hash == p).expect("P is on the chain");
    assert!(k + 1 < all.len(), "a block after P");
    let importer = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    for b in &all[..=k] {
        arrive(&importer, b.clone(), "a block through the pruning point").await;
    }
    arrive(&importer, Block::from_header_arc(all[k + 1].header.clone()), "T's header").await;
    let vp = lg.chain.vp();
    vp.capture_pruning_point_palw_state(p).expect("exact PALW snapshot capture succeeds");
    let wire = borsh::to_vec(&vp.pruning_point_palw_state(p).expect("servable")).expect("serializes");
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&wire).expect("the wire bytes decode");
    {
        let ivp = importer.vp();
        let mut store = ivp.palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("no PALW tip");
        for blk in std::iter::once(importer.config.params.genesis.hash).chain(all[..=k].iter().map(|b| b.header.hash)) {
            store.delete_delta_for_tests(blk).expect("no delta row at or below the pruning point");
        }
    }
    importer.establish_pruning_import_target(p);
    importer.vp().import_pruning_point_palw_state(p, carriage).expect("the served carriage installs");
    assert_eq!(importer.tip_state().1.state_root(), root_at(&lg.chain, p), "the imported state is P's");
    for blk in &all[k + 1..] {
        arrive(&importer, blk.clone(), "a block after P").await;
    }
    assert_eq!(importer.tip_state().1.state_root(), lg.chain.tip_state().1.state_root(), "the importer reaches the same tip root");
    importer
}

/// **(b)+(d) restart, IBD, pruned import:** the node stops with the held session open (before its deadline), reopens over the same
/// database, and the silence still ends in the objective DA default; a full-IBD node and a node importing at the block that holds
/// the open session both reach the same roots and the same `ProducerWithholding` verdict.
#[tokio::test]
async fn arm_legacy_d_held_da_default_across_restart_ibd_and_pruned_import() {
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut lg = g14_lg_over(false, |c: &Config| TestConsensus::with_db(db.clone(), c, sender));
    lg._keep.push(Box::new(receiver));
    let (produced, _, _) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    let (_v, deadline) = reserve_bind_and_demand(&mut lg, &produced).await;
    let session_block = lg.chain.sink();
    assert!(lg.daa() < deadline, "stopped with the session open");
    let mut lg = restart(lg, &db);
    silent_to_default(&mut lg, &produced, deadline).await;
    let z = ibd_replays_the_same_roots(&lg).await;
    let zl = lg.on_chain(z, lg.funding.clone());
    assert!(zl.view(id).da_defaulted && !zl.view(id).court_convicted, "the IBD node's verdict: DA default, no conviction");
    let i = pruned_import_at(&lg, session_block).await;
    let il = lg.on_chain(i, lg.funding.clone());
    assert!(il.view(id).da_defaulted && !il.view(id).court_convicted, "the pruned importer's verdict: DA default, no conviction");
}

/// **(b)+(d) reorg:** after the reservation and binding, `A` carries the held demand; a node `Z` follows `A`, then reorgs onto a
/// heavier branch `B` forked just before the demand (inside the shallow-tie window). Whatever stands on `Z`'s tip, the outsider's
/// demand (re-sent if the reorg dropped it) runs to its deadline on the new branch and the silence ends in the same objective DA
/// default — never a computation conviction.
#[tokio::test]
async fn arm_legacy_d_held_da_default_after_a_reorg_off_the_demand() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = g14_lg();
    let (produced, _, _) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    let newcomer = lg.bond(NEWCOMER);
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    for _ in 0..8 {
        if v.binding.is_some() {
            break;
        }
        v.step(&mut lg, &produced, &withhold).await;
    }
    assert!(v.binding.is_some(), "the binding read");
    let fork = lg.chain.sink();
    demand_root(&mut lg, &v, id).await;

    let z = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    for block in chain_blocks(&lg.chain, lg.chain.sink()) {
        arrive(&z, block, "A's block").await;
    }
    let mut b = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    let up_to_fork = chain_blocks(&lg.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for block in up_to_fork {
        arrive(&b, block, "a block up to the fork").await;
    }
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = lg.ttpb();
    let a_work = lg.chain.ctx.consensus.get_block(lg.chain.sink()).unwrap().header.blue_work;
    let mut b_blocks = Vec::new();
    while b.ctx.consensus.get_block(b.sink()).unwrap().header.blue_work <= a_work {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    let shallow = kaspa_consensus_core::palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_DAA_V1;
    assert!(b.daa_of(b.sink()) - b.daa_of(fork) <= shallow, "B stays inside the real shallow-tie window");
    for block in &b_blocks {
        arrive(&z, block.clone(), "B's block").await;
    }
    assert_eq!(z.sink(), b.sink(), "Z reorgs onto B");
    let mut z = z;
    z.ctx.simulated_time = z.ctx.simulated_time.max(b.ctx.simulated_time);
    let mut zl = lg.on_chain(z, lg.funding.clone());
    assert!(zl.view(id).record.is_some_and(|r| r.live.contains_key(&newcomer)), "the reservation predates the fork");
    let open = zl
        .view(id)
        .sessions
        .iter()
        .find(|(accuser, s)| *accuser == newcomer && s.units.iter().any(withhold))
        .map(|(_, s)| s.deadline_daa);
    eprintln!("[arm-legacy d reorg] held session on Z's tip after the reorg: {open:?}");
    let deadline = match open {
        Some(d) => d,
        None => demand_root(&mut zl, &v, id).await,
    };
    silent_to_default(&mut zl, &produced, deadline).await;
}

/// **(c)+(d) the honest twin, untouched:** an honest licensed attempt on the same armed network reaches `Final` at its deadline with
/// its vesting row and safe weight; across a restart, a full IBD and a pruned import at its `Final` block it stays `Final` with the
/// same row, nothing reserved, no session, no offence, and no collateral moved from the producer or any seat.
#[tokio::test]
async fn arm_legacy_d_honest_twin_untouched_across_restart_ibd_and_pruned_import() {
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut lg = g14_lg_over(false, |c: &Config| TestConsensus::with_db(db.clone(), c, sender));
    lg._keep.push(Box::new(receiver));
    let (produced, seats, licensed) = licensed_claim(&mut lg, false).await;
    let id = produced.claim_id;
    let producer_before = lg.collateral(EXECUTOR);
    let seat_before: Vec<(usize, u64)> = seats.iter().map(|c| (*c, lg.collateral(*c))).collect();
    let deadline = lg.chain.tip_state().1.deadline_of(&id).expect("a licensed claim owes its Final deadline");
    assert!(deadline >= licensed);
    lg.beat_to(deadline + 1).await;
    let final_block = lg.chain.sink();
    lg.beat(2).await;
    let check = |l: &Lg, who: &str| {
        let (_, state) = l.chain.tip_state();
        assert!(matches!(state.claim(&id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Final { .. })), "{who}: Final");
        assert!(state.vesting_row(&id).is_some(), "{who}: its vesting row");
        let view = l.view(id);
        assert!(view.record.is_none() && view.sessions.is_empty(), "{who}: nothing reserved, no session");
        assert!(!view.da_defaulted && !view.court_convicted && !view.executor_refuted, "{who}: no verdict");
    };
    check(&lg, "A");
    let lg = restart(lg, &db);
    check(&lg, "after the restart");
    assert_eq!(lg.collateral(EXECUTOR), producer_before, "the honest producer is not charged");
    for (card, before) in &seat_before {
        assert_eq!(lg.collateral(*card), *before, "card {card}: the honest seat is not charged");
    }
    let z = ibd_replays_the_same_roots(&lg).await;
    check(&lg.on_chain(z, lg.funding.clone()), "the IBD node");
    let i = pruned_import_at(&lg, final_block).await;
    check(&lg.on_chain(i, lg.funding.clone()), "the pruned importer");
}
