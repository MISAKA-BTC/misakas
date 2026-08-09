//! End-to-end proof that the chain-participation work behaves over real p2p.
//!
//! Everything else about this change is tested against structures in memory. That is where the
//! policy lives, but it is not where the incident happened: testnet-22 was two real nodes, a real
//! handshake, and a real IBD. These tests run actual `kaspad` daemons, connect them over TCP, and
//! assert on what the node tells the outside world — because that is what a validator and a miner
//! act on.
//!
//! **What these do not cover, and why.** None of them drives a headers-proof IBD, because a node
//! cannot be made to run one at this scale. A fresh simnet node orphans the peer's tip, resolves
//! the missing roots from a block locator, and walks forward — measured, not assumed: with 1500
//! blocks mined ahead of it, the follower logs `Orphaned 19 ... Unorphaned 19` and never enters
//! IBD. Reaching `DownloadHeadersProof` needs the peer's chain to be a pruning depth ahead, which
//! at simnet parameters is far more blocks than a test can mine.
//!
//! The way through is to shorten the pruning depth so the leader actually prunes — see
//! `common::shallow_pruning`. With that, a joining node has to run a real IBD, and the tests below
//! exercise `IbdRunning` end to end over TCP.
//!
//! Still not covered end to end: `Quarantined`, the commit barrier's refusals, and the challenger
//! proof exchange. Those need two peers offering genuinely different chains at pruning depth, which
//! means two independently-mined histories rather than one shared one — a harness of its own. They
//! remain covered by unit tests over the same code.

use crate::common::{
    daemon::Daemon,
    laggy_link::{LaggyLink, WAN_DELAY},
    shallow_pruning::{BLOCKS_TO_PRUNE, write_shallow_pruning_params},
    utils::wait_for,
};
use kaspa_addresses::Address;
use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_consensus::params::SIMNET_PARAMS;
use kaspa_grpc_client::GrpcClient;
use kaspa_p2p_flows::flowcontext::{recovery_trace, verification_trace};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspad_lib::args::Args;
use std::time::Duration;

/// Enough blocks that the follower has real work to do, but not so many the test drags.
const BLOCKS_TO_MINE: usize = 20;
const TOTAL_FD_LIMIT: i32 = 10;

fn gated_args() -> Args {
    Args {
        simnet: true,
        unsafe_rpc: true,
        disable_upnp: true,
        // Needed to mine at all from genesis on simnet: a fresh node's sink is old, so it is not
        // "nearly synced" and submitBlock would be refused for reasons unrelated to this work.
        // Note this bypasses the submitBlock gate ONLY — it is not on the validator path and cannot
        // reach the participation gate, which is the separation these tests rely on.
        enable_unsynced_mining: true,
        // The gate is off by default on simnet — a peerless node has no branch to overlook. These
        // tests have two nodes and mean to exercise it, so they ask for it explicitly.
        enforce_chain_participation: true,
        ..Default::default()
    }
}

/// kaspa-pq is PQ-only: a coinbase pay address must be ML-DSA-87 P2PKH (a 64-byte BLAKE2b-512
/// pubkey hash). The legacy secp256k1 `PubKey` form is rejected by the script-class check
/// (ADR-0019 §8). Where the coins go is irrelevant here, so a fixed hash is fine.
fn miner_address(daemon: &Daemon) -> Address {
    Address::new(daemon.network.into(), kaspa_addresses::Version::PubKeyHashMlDsa87, &[0; 64])
}

/// Mine `count` blocks into `client`, returning the resulting virtual DAA score.
///
/// Submitted directly rather than through the shared `mine_block` helper, which waits on
/// block-added notifications this test has no listeners for.
async fn mine(client: &GrpcClient, address: &Address, count: usize) -> u64 {
    for _ in 0..count {
        let template = client.get_block_template(address.clone(), vec![]).await.unwrap();
        client.submit_block(template.block, false).await.unwrap();
    }
    client.get_block_dag_info().await.unwrap().virtual_daa_score
}

async fn connect(from: &GrpcClient, to_port: u16) {
    from.add_peer(format!("127.0.0.1:{to_port}").try_into().unwrap(), true).await.unwrap();
    let check = from.clone();
    wait_for(
        50,
        200,
        move || {
            let client = check.clone();
            async move { !client.get_connected_peer_info().await.unwrap().peer_info.is_empty() }
        },
        "the nodes never connected",
    )
    .await;
}

/// A node that catches up through ordinary relay must NOT be held back.
///
/// This is the false-positive direction, and it is the one that takes a network down. The gate
/// closes on IBD — on wholesale adoption of a peer's chain — not on receiving blocks. A follower
/// that joins, orphans the tip, resolves the roots and walks forward has not adopted anyone's
/// chain; it validated every block itself. Gating that would stop honest nodes from mining, which
/// is a worse outcome than the bug being fixed and a much easier one to ship by accident.
///
/// Asserted over real p2p, at real DAG parity with the peer it caught up from.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_node_that_caught_up_by_relay_is_not_held_back() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let mut leader = Daemon::new_random_with_args(gated_args(), TOTAL_FD_LIMIT);
    let leader_client = leader.start().await;
    let address = miner_address(&leader);

    let target = mine(&leader_client, &address, BLOCKS_TO_MINE).await;
    assert!(target >= BLOCKS_TO_MINE as u64, "the leader did not build a chain to catch up to");
    assert!(leader_client.get_sync_status().await.unwrap(), "the leader mined its own chain and should not be held back");

    let mut follower = Daemon::new_random_with_args(gated_args(), TOTAL_FD_LIMIT);
    let follower_client = follower.start().await;
    connect(&follower_client, leader.p2p_port).await;

    let check = follower_client.clone();
    wait_for(
        100,
        300,
        move || {
            let client = check.clone();
            async move { client.get_block_dag_info().await.unwrap().virtual_daa_score >= target }
        },
        "the follower never caught up to the leader",
    )
    .await;

    // Caught up by relay, so nothing was adopted wholesale and nothing should be withheld. All
    // three RPCs must agree — they are one definition now, and a validator polls the weakest.
    assert!(follower_client.get_sync_status().await.unwrap(), "a node that caught up by relay was held back anyway");
    assert!(follower_client.get_server_info().await.unwrap().is_synced, "getServerInfo disagreed with getSyncStatus");
    assert!(follower_client.get_info().await.unwrap().is_synced, "getInfo disagreed with getSyncStatus");
    assert!(
        follower_client.get_block_template(address.clone(), vec![]).await.unwrap().is_synced,
        "a node that caught up by relay would not hand out a mineable template"
    );

    follower.shutdown();
    leader.shutdown();
}

/// A node that never syncs from anyone has nothing to review, and must not be held back.
///
/// The counterpart to the tests above: a gate that closes when it should not is a node that can
/// never mine, which is a worse failure than the one being fixed and a much easier one to ship by
/// accident.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_node_that_never_synced_is_not_held_back() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let mut solo = Daemon::new_random_with_args(gated_args(), TOTAL_FD_LIMIT);
    let client = solo.start().await;
    let address = miner_address(&solo);

    // No peer, no IBD, so the gate never closes.
    let score = mine(&client, &address, 5).await;
    assert!(score >= 5, "a node with nothing to review could not mine");
    assert!(client.get_sync_status().await.unwrap(), "a node that never synced from anyone was held back anyway");
    assert!(
        client.get_block_template(address.clone(), vec![]).await.unwrap().is_synced,
        "a node with nothing to review refused to hand out a mineable template"
    );

    solo.shutdown();
}

/// Peers that disagree about consensus rules must not become peers at all.
///
/// testnet-22 forked partly because an older build presented a handshake indistinguishable from a
/// correct one, so the two peered and each became a valid IBD source for the other. Here the two
/// daemons differ in a consensus parameter while answering the same network name — the exact shape
/// that used to connect — and must not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn peers_running_different_consensus_rules_do_not_connect() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let mut normal = Daemon::new_random_with_args(gated_args(), TOTAL_FD_LIMIT);
    let normal_client = normal.start().await;

    // Same network name, different rules. `--override-params-file` is exactly how this happens for
    // real — it is the supported way to run a network with adjusted consensus parameters, and
    // nothing before this change stopped such a node from peering with an unadjusted one.
    let overrides = std::env::temp_dir().join(format!("misaka-e2e-override-{}.json", std::process::id()));
    std::fs::write(&overrides, format!(r#"{{"timestamp_deviation_tolerance": {}}}"#, SIMNET_PARAMS.timestamp_deviation_tolerance + 1))
        .unwrap();

    let mut divergent_args = gated_args();
    divergent_args.override_params_file = Some(overrides.to_string_lossy().into_owned());
    let mut divergent = Daemon::new_random_with_args(divergent_args, TOTAL_FD_LIMIT);
    let divergent_client = divergent.start().await;

    divergent_client.add_peer(format!("127.0.0.1:{}", normal.p2p_port).try_into().unwrap(), true).await.unwrap();

    // Give it long enough that a successful handshake would certainly have completed.
    tokio::time::sleep(Duration::from_secs(5)).await;

    assert!(
        divergent_client.get_connected_peer_info().await.unwrap().peer_info.is_empty(),
        "a node running different consensus rules connected anyway; it could then have become an IBD source and forked the \
         network, which is how testnet-22 happened"
    );
    assert!(
        normal_client.get_connected_peer_info().await.unwrap().peer_info.is_empty(),
        "the correctly-configured node accepted a peer with different consensus rules"
    );

    divergent.shutdown();
    normal.shutdown();
    let _ = std::fs::remove_file(&overrides);
}

/// Probe: confirm the shallow preset actually makes the pruning point leave genesis.
///
/// The IBD tests below are only meaningful if it does — that is the whole mechanism by which a
/// fresh node stops being able to unorphan its way forward.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_shallow_preset_advances_the_pruning_point() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let overrides = write_shallow_pruning_params("probe");
    let mut args = gated_args();
    args.override_params_file = Some(overrides.to_string_lossy().into_owned());

    let mut node = Daemon::new_random_with_args(args, TOTAL_FD_LIMIT);
    let client = node.start().await;
    let address = miner_address(&node);

    let genesis = client.get_block_dag_info().await.unwrap().pruning_point_hash;
    let score = mine(&client, &address, BLOCKS_TO_PRUNE).await;
    let info = client.get_block_dag_info().await.unwrap();
    println!("PROBE score={} pruning_point_moved={} pp={}", score, info.pruning_point_hash != genesis, info.pruning_point_hash);
    assert_ne!(info.pruning_point_hash, genesis, "the shallow preset did not move the pruning point off genesis");

    node.shutdown();
    let _ = std::fs::remove_file(&overrides);
}

/// Set up a leader whose pruning point has left genesis, so a joining node must run a real IBD.
///
/// Returns the leader, its client, the pay address, and the override file to clean up. Both daemons
/// must be given the SAME override file: differing consensus params would — correctly — stop them
/// peering, which is a different test.
async fn pruned_leader(tag: &str) -> (Daemon, GrpcClient, Address, std::path::PathBuf, u64) {
    let overrides = write_shallow_pruning_params(tag);
    let mut args = gated_args();
    args.override_params_file = Some(overrides.to_string_lossy().into_owned());

    let mut leader = Daemon::new_random_with_args(args, TOTAL_FD_LIMIT);
    let client = leader.start().await;
    let address = miner_address(&leader);

    let genesis_pp = client.get_block_dag_info().await.unwrap().pruning_point_hash;
    let score = mine(&client, &address, BLOCKS_TO_PRUNE).await;
    let pp = client.get_block_dag_info().await.unwrap().pruning_point_hash;
    assert_ne!(pp, genesis_pp, "the leader never pruned, so a follower would unorphan instead of running an IBD");

    (leader, client, address, overrides, score)
}

async fn joined_follower(overrides: &std::path::Path, leader_p2p_port: u16, target: u64) -> (Daemon, GrpcClient) {
    let mut args = gated_args();
    args.override_params_file = Some(overrides.to_string_lossy().into_owned());
    let mut follower = Daemon::new_random_with_args(args, TOTAL_FD_LIMIT);
    let client = follower.start().await;
    connect(&client, leader_p2p_port).await;

    let check = client.clone();
    wait_for(
        200,
        600,
        move || {
            let c = check.clone();
            async move { c.get_block_dag_info().await.unwrap().virtual_daa_score >= target }
        },
        "the follower never synced the leader's chain",
    )
    .await;
    (follower, client)
}

/// A node that adopted a peer's chain wholesale must not call itself synced.
///
/// This is the property everything else rests on. The in-process validator, the external validator
/// and every miner take `is_synced` as permission to act, and testnet-22 is what happens when a node
/// grants that permission for a chain nothing compared. Asserted through the same RPC a real
/// validator polls, on a node that really did run an IBD against a real peer over TCP.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_node_that_ran_an_ibd_does_not_report_itself_synced() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let (mut leader, leader_client, _address, overrides, target) = pruned_leader("no-synced").await;
    assert!(leader_client.get_sync_status().await.unwrap(), "the leader mined its own chain and should not be held back");

    let (mut follower, follower_client) = joined_follower(&overrides, leader.p2p_port, target).await;

    // At the leader's DAA score, and still refusing to say it is synced. Being at the tip is not the
    // same as having established that the tip is the right one.
    //
    // What rules out an unrelated cause is the control test below:
    // `a_node_that_caught_up_by_relay_is_not_held_back` reaches the same DAG parity through the same
    // RPCs on the same harness and reports synced=TRUE. The only difference between them is whether
    // an IBD ran. (The logs agree: "IBD started with peer" → "Chain participation held:
    // state=ibd-running" → "IBD with peer ... completed successfully".)
    assert!(
        !follower_client.get_server_info().await.unwrap().is_synced,
        "the follower reported is_synced=true after an IBD; a validator polling this would attest on a chain nothing compared"
    );
    assert!(!follower_client.get_sync_status().await.unwrap(), "getSyncStatus disagreed with getServerInfo");
    assert!(!follower_client.get_info().await.unwrap().is_synced, "getInfo disagreed with getServerInfo");

    follower.shutdown();
    leader.shutdown();
    let _ = std::fs::remove_file(&overrides);
}

/// The gate must reach the block template too, or a miner would build on the unreviewed chain while
/// the validator sat out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_node_that_ran_an_ibd_does_not_advertise_a_mineable_template() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let (mut leader, _leader_client, address, overrides, target) = pruned_leader("no-template").await;
    let (mut follower, follower_client) = joined_follower(&overrides, leader.p2p_port, target).await;

    let template = follower_client.get_block_template(address.clone(), vec![]).await.unwrap();
    assert!(
        !template.is_synced,
        "the follower handed out a template marked synced while its chain was still under review; miners honour this flag"
    );

    follower.shutdown();
    leader.shutdown();
    let _ = std::fs::remove_file(&overrides);
}

/// Probe: two independently mined chains, both pruned, offered to one fresh node.
///
/// This is the testnet-22 shape and the precondition for every remaining assertion. Two leaders
/// that never met each other mine past pruning depth on identical rules, so a joining node is
/// offered two genuinely different histories rather than two views of one. Confirms the setup is
/// reachable before anything is asserted about which one wins.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_independent_pruned_chains_can_be_offered_to_one_node() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let overrides = write_shallow_pruning_params("two-chains");

    // Branch B: the one that will be synced first, and is the lighter of the two.
    let (mut b, b_client, _b_addr, b_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE).await;
    // Branch A: independently mined, never connected to B, and deliberately heavier.
    let (mut a, a_client, _a_addr, a_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE + 1500).await;

    let b_info = b_client.get_block_dag_info().await.unwrap();
    let a_info = a_client.get_block_dag_info().await.unwrap();
    println!(
        "PROBE b_score={} b_pp={} a_score={} a_pp={} different_chains={}",
        b_score,
        b_info.pruning_point_hash,
        a_score,
        a_info.pruning_point_hash,
        a_info.pruning_point_hash != b_info.pruning_point_hash
    );
    assert_ne!(
        a_info.pruning_point_hash, b_info.pruning_point_hash,
        "the two leaders converged on one history, so there is no partition to test"
    );
    assert!(a_score > b_score, "branch A must be the heavier one for the switch to be the correct outcome");

    // A fresh node meets B first, then A — the incident's ordering.
    let mut args = gated_args();
    args.override_params_file = Some(overrides.to_string_lossy().into_owned());
    let mut follower = Daemon::new_random_with_args(args, TOTAL_FD_LIMIT);
    let follower_client = follower.start().await;
    connect(&follower_client, b.p2p_port).await;
    follower_client.add_peer(format!("127.0.0.1:{}", a.p2p_port).try_into().unwrap(), true).await.unwrap();

    // Whatever it settles on, it must not be reporting itself synced while two histories are open.
    tokio::time::sleep(Duration::from_secs(20)).await;
    let peers = follower_client.get_connected_peer_info().await.unwrap().peer_info.len();
    let synced = follower_client.get_sync_status().await.unwrap();
    println!("PROBE follower peers={peers} synced={synced}");
    assert!(!synced, "the follower called itself synced while two conflicting histories were on offer");

    follower.shutdown();
    a.shutdown();
    b.shutdown();
    let _ = std::fs::remove_file(&overrides);
}

/// Mine an independent branch on the shared shallow preset, past its pruning depth.
async fn pruned_branch(overrides: &std::path::Path, blocks: usize) -> (Daemon, GrpcClient, Address, u64) {
    let mut args = gated_args();
    args.override_params_file = Some(overrides.to_string_lossy().into_owned());
    let mut node = Daemon::new_random_with_args(args, TOTAL_FD_LIMIT);
    let client = node.start().await;
    let address = miner_address(&node);

    let genesis_pp = client.get_block_dag_info().await.unwrap().pruning_point_hash;
    let score = mine(&client, &address, blocks).await;
    assert_ne!(
        client.get_block_dag_info().await.unwrap().pruning_point_hash,
        genesis_pp,
        "branch never pruned, so a follower would unorphan instead of running an IBD"
    );
    (node, client, address, score)
}

/// Accumulated work at a branch's sink — the quantity chain selection actually uses.
///
/// The DAA score is not it. A branch mined quickly at low difficulty can outscore one mined slowly
/// at high difficulty while carrying less work, and a node choosing between them will correctly
/// prefer the second. Measured on the VPS fixture, where the branch with 1300 MORE DAA score had
/// 25% LESS blue work: every round run against it was asserting the wrong proposition, and the node
/// refusing to switch was right.
async fn branch_blue_work(client: &GrpcClient) -> kaspa_consensus_core::BlueWorkType {
    let sink = client.get_block_dag_info().await.unwrap().sink;
    client.get_block(sink, false).await.unwrap().header.blue_work
}

/// Assert the premise every one of these tests rests on: two genuinely different histories, and the
/// one called "heavy" is the one a correct node should prefer.
///
/// Checked rather than assumed, because it was assumed once and was false.
async fn assert_fixture_premise(light: &GrpcClient, heavy: &GrpcClient) {
    let light_pp = light.get_block_dag_info().await.unwrap().pruning_point_hash;
    let heavy_pp = heavy.get_block_dag_info().await.unwrap().pruning_point_hash;
    assert_ne!(light_pp, heavy_pp, "the two branches share a pruning point, so they are not two histories");

    let light_work = branch_blue_work(light).await;
    let heavy_work = branch_blue_work(heavy).await;
    assert!(
        heavy_work > light_work,
        "the branch this fixture calls heavy has LESS accumulated work than the light one \
         (heavy={heavy_work} light={light_work}). A node preferring the light branch would be \
         correct, so nothing this test asserts about convergence would mean anything."
    );
}

/// The whole point, end to end: which chain a node adopts must not depend on who relayed first.
///
/// Two leaders that never met mine independently past pruning depth on identical rules, one heavier
/// than the other. A fresh node is connected to both at once, so which of them wins the IBD latch is
/// a genuine race — exactly the race that decided testnet-22. The node must end up on the heavier
/// chain either way: if it raced onto the heavier one, it stays; if it raced onto the lighter one, it
/// has to verify the other's pruning proof and hand the latch over.
///
/// E2E-A: a stronger chain found DURING the first IBD must win, without Bootstrap Recovery.
///
/// Split from the combined scenario deliberately. This half exercises only the candidate
/// coordinator — summary, proof, comparison, reservation, handoff — and never crosses a pruning
/// point, because nothing has been committed yet. If this fails, the problem is in the coordinator
/// and looking at recovery would be looking in the wrong place.
///
/// Both leaders stop mining before the follower joins, so candidate ids cannot drift under the
/// reservation while it is being redeemed.
#[ignore = "passes; opt-in because it takes ~6 minutes — run with --include-ignored"]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_a_a_stronger_chain_found_during_ibd_wins() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");
    recovery_trace::clear();

    let overrides = write_shallow_pruning_params("e2e-a");
    let (mut light, light_client, _l, _light_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE).await;
    let (mut heavy, heavy_client, _h, heavy_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE + 2500).await;
    let light_pp = light_client.get_block_dag_info().await.unwrap().pruning_point_hash;
    let heavy_pp = heavy_client.get_block_dag_info().await.unwrap().pruning_point_hash;
    assert_fixture_premise(&light_client, &heavy_client).await;

    // Mining has stopped on both: the tips are now fixed, so a reservation cannot be invalidated by
    // the chain moving under it.
    let mut args = gated_args();
    args.override_params_file = Some(overrides.to_string_lossy().into_owned());
    let mut follower = Daemon::new_random_with_args(args, TOTAL_FD_LIMIT);
    let follower_client = follower.start().await;
    follower_client.add_peer(format!("127.0.0.1:{}", light.p2p_port).try_into().unwrap(), true).await.unwrap();
    follower_client.add_peer(format!("127.0.0.1:{}", heavy.p2p_port).try_into().unwrap(), true).await.unwrap();

    let check = follower_client.clone();
    let settled = tokio::time::timeout(Duration::from_secs(240), async move {
        loop {
            if check.get_block_dag_info().await.unwrap().virtual_daa_score >= heavy_score {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .unwrap_or(false);

    let info = follower_client.get_block_dag_info().await.unwrap();
    println!("{}", recovery_trace::diagnosis(recovery_trace::RecoveryStage::CandidateCommitted));
    println!(
        "E2E-A outcome settled={settled} score={} on_heavy={} on_light={}",
        info.virtual_daa_score,
        info.pruning_point_hash == heavy_pp,
        info.pruning_point_hash == light_pp
    );

    follower.shutdown();
    heavy.shutdown();
    light.shutdown();
    let _ = std::fs::remove_file(&overrides);

    assert_eq!(info.pruning_point_hash, heavy_pp, "the follower did not end up on the heavier chain");
}

/// E2E-B: a stronger chain found AFTER a provisional commit must be adopted via Bootstrap Recovery.
///
/// Only worth investigating once E2E-A passes: this half additionally requires crossing the
/// provisional pruning point, which is the part that needs a permit.
#[ignore = "passes; opt-in because it takes ~6 minutes — run with --include-ignored"]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_b_bootstrap_recovery_crosses_a_provisional_pruning_point() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");
    recovery_trace::clear();

    let overrides = write_shallow_pruning_params("e2e-b");
    let (mut light, light_client, _l, light_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE).await;
    let (mut heavy, heavy_client, _h, heavy_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE + 2500).await;
    let light_pp = light_client.get_block_dag_info().await.unwrap().pruning_point_hash;
    let heavy_pp = heavy_client.get_block_dag_info().await.unwrap().pruning_point_hash;
    assert_fixture_premise(&light_client, &heavy_client).await;

    // Sync the lighter chain to completion FIRST, so it is provisionally committed and its pruning
    // point becomes the boundary a permit has to cross.
    let mut args = gated_args();
    args.override_params_file = Some(overrides.to_string_lossy().into_owned());
    let mut follower = Daemon::new_random_with_args(args, TOTAL_FD_LIMIT);
    let follower_client = follower.start().await;
    connect(&follower_client, light.p2p_port).await;

    let check = follower_client.clone();
    wait_for(
        200,
        600,
        move || {
            let c = check.clone();
            async move { c.get_block_dag_info().await.unwrap().virtual_daa_score >= light_score }
        },
        "the follower never synced the lighter chain",
    )
    .await;
    let provisional = follower_client.get_block_dag_info().await.unwrap().pruning_point_hash;
    assert_eq!(provisional, light_pp, "the follower did not provisionally adopt the lighter chain");
    assert!(!follower_client.get_sync_status().await.unwrap(), "should still be withholding participation");

    // Only now offer the heavier one.
    follower_client.add_peer(format!("127.0.0.1:{}", heavy.p2p_port).try_into().unwrap(), true).await.unwrap();

    let check = follower_client.clone();
    let settled = tokio::time::timeout(Duration::from_secs(240), async move {
        loop {
            if check.get_block_dag_info().await.unwrap().virtual_daa_score >= heavy_score {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .unwrap_or(false);

    let info = follower_client.get_block_dag_info().await.unwrap();
    println!("{}", recovery_trace::diagnosis(recovery_trace::RecoveryStage::RecoveryPermitGranted));
    println!(
        "E2E-B outcome settled={settled} score={} on_heavy={} still_on_light={}",
        info.virtual_daa_score,
        info.pruning_point_hash == heavy_pp,
        info.pruning_point_hash == light_pp
    );

    follower.shutdown();
    heavy.shutdown();
    light.shutdown();
    let _ = std::fs::remove_file(&overrides);

    assert_eq!(info.pruning_point_hash, heavy_pp, "bootstrap recovery did not adopt the heavier chain");
}

/// How many times a mainnet-qualifying scenario must hold. One green run of a concurrent system is
/// an anecdote; the interesting failures are the ones that need a particular interleaving.
const QUALIFYING_REPETITIONS: usize = 3;

/// Run E2E-A's scenario once over a delayed link, returning whether the node landed on the heavier
/// chain. Shared by the repetition harness so each round is genuinely independent — fresh daemons,
/// fresh data directories, fresh link.
async fn handoff_round_over_wan(tag: &str) -> bool {
    let overrides = write_shallow_pruning_params(tag);
    let (mut light, _light_client, _l, _ls) = pruned_branch(&overrides, BLOCKS_TO_PRUNE).await;
    let (mut heavy, heavy_client, _h, heavy_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE + 2500).await;
    let heavy_pp = heavy_client.get_block_dag_info().await.unwrap().pruning_point_hash;

    // Both peers are reached through delayed links, so neither is privileged by being closer.
    let light_link = LaggyLink::spawn(light.p2p_port, WAN_DELAY).await;
    let heavy_link = LaggyLink::spawn(heavy.p2p_port, WAN_DELAY).await;

    let mut args = gated_args();
    args.override_params_file = Some(overrides.to_string_lossy().into_owned());
    let mut follower = Daemon::new_random_with_args(args, TOTAL_FD_LIMIT);
    let follower_client = follower.start().await;
    follower_client.add_peer(format!("127.0.0.1:{}", light_link.local_port).try_into().unwrap(), true).await.unwrap();
    follower_client.add_peer(format!("127.0.0.1:{}", heavy_link.local_port).try_into().unwrap(), true).await.unwrap();

    let check = follower_client.clone();
    let _ = tokio::time::timeout(Duration::from_secs(600), async move {
        loop {
            if check.get_block_dag_info().await.unwrap().virtual_daa_score >= heavy_score {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await;

    let landed = follower_client.get_block_dag_info().await.unwrap().pruning_point_hash == heavy_pp;
    follower.shutdown();
    heavy.shutdown();
    light.shutdown();
    let _ = std::fs::remove_file(&overrides);
    landed
}

/// Run E2E-B's scenario once over a delayed link: adopt the lighter chain first, then meet the
/// heavier one. Returns whether recovery replaced it.
async fn recovery_round_over_wan(tag: &str) -> bool {
    let overrides = write_shallow_pruning_params(tag);
    let (mut light, light_client, _l, light_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE).await;
    let (mut heavy, heavy_client, _h, heavy_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE + 2500).await;
    let heavy_pp = heavy_client.get_block_dag_info().await.unwrap().pruning_point_hash;
    let light_pp = light_client.get_block_dag_info().await.unwrap().pruning_point_hash;

    let light_link = LaggyLink::spawn(light.p2p_port, WAN_DELAY).await;
    let heavy_link = LaggyLink::spawn(heavy.p2p_port, WAN_DELAY).await;

    let mut args = gated_args();
    args.override_params_file = Some(overrides.to_string_lossy().into_owned());
    let mut follower = Daemon::new_random_with_args(args, TOTAL_FD_LIMIT);
    let follower_client = follower.start().await;
    follower_client.add_peer(format!("127.0.0.1:{}", light_link.local_port).try_into().unwrap(), true).await.unwrap();

    let check = follower_client.clone();
    let adopted = tokio::time::timeout(Duration::from_secs(600), async move {
        loop {
            if check.get_block_dag_info().await.unwrap().virtual_daa_score >= light_score {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .unwrap_or(false);
    assert!(adopted, "the follower never adopted the lighter chain over a delayed link");
    assert_eq!(follower_client.get_block_dag_info().await.unwrap().pruning_point_hash, light_pp);
    assert!(!follower_client.get_sync_status().await.unwrap(), "should still be withholding participation");

    follower_client.add_peer(format!("127.0.0.1:{}", heavy_link.local_port).try_into().unwrap(), true).await.unwrap();
    let check = follower_client.clone();
    let _ = tokio::time::timeout(Duration::from_secs(600), async move {
        loop {
            if check.get_block_dag_info().await.unwrap().virtual_daa_score >= heavy_score {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await;

    let recovered = follower_client.get_block_dag_info().await.unwrap().pruning_point_hash == heavy_pp;
    follower.shutdown();
    heavy.shutdown();
    light.shutdown();
    let _ = std::fs::remove_file(&overrides);
    recovered
}

/// Mainnet gate: the pre-commit handoff must hold repeatedly, over a delayed link.
///
/// Reports every round rather than stopping at the first failure — "two of three" and "none of
/// three" are different diagnoses, and stopping early throws that away.
#[ignore = "mainnet qualification: ~30 minutes"]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mainnet_gate_handoff_holds_repeatedly_over_a_delayed_link() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let mut results = Vec::new();
    for round in 0..QUALIFYING_REPETITIONS {
        recovery_trace::clear();
        let landed = handoff_round_over_wan(&format!("wan-handoff-{round}")).await;
        println!("MAINNET-GATE handoff round={round} landed_on_heavy={landed}");
        if !landed {
            println!("{}", recovery_trace::diagnosis(recovery_trace::RecoveryStage::CandidateCommitted));
        }
        results.push(landed);
    }
    println!("MAINNET-GATE handoff results={results:?}");
    assert!(results.iter().all(|r| *r), "the pre-commit handoff did not hold in every round: {results:?}");
}

/// Mainnet gate: bootstrap recovery must hold repeatedly, over a delayed link.
#[ignore = "mainnet qualification: ~40 minutes"]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mainnet_gate_recovery_holds_repeatedly_over_a_delayed_link() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let mut results = Vec::new();
    for round in 0..QUALIFYING_REPETITIONS {
        recovery_trace::clear();
        let recovered = recovery_round_over_wan(&format!("wan-recovery-{round}")).await;
        println!("MAINNET-GATE recovery round={round} recovered={recovered}");
        if !recovered {
            println!("{}", recovery_trace::diagnosis(recovery_trace::RecoveryStage::RecoveryPermitGranted));
        }
        results.push(recovered);
    }
    println!("MAINNET-GATE recovery results={results:?}");
    assert!(results.iter().all(|r| *r), "bootstrap recovery did not hold in every round: {results:?}");
}

/// Rounds the randomized soak runs. Three greens proved nothing on their own — this work has
/// already produced a green run that was luck — so the release gate is a soak, not a sample.
const SOAK_ROUNDS: usize = 20;

/// Mainnet gate: the property must hold under randomized network conditions, repeatedly.
///
/// Everything that varied between the passing and failing rounds so far was timing, so timing is
/// what gets randomized: latency across a wide band, which peer is offered first, how long before
/// the second appears, and whether the link breaks and heals mid-flight.
///
/// The two chains are mined ONCE and reused. What varies between rounds is the follower's
/// experience of the network, which is the thing under test; re-mining 8700 blocks per round would
/// buy nothing and cost hours.
///
/// Each round is seeded from its index, so a failure replays exactly rather than being a story
/// about a bad afternoon.
#[ignore = "mainnet soak: hours; run explicitly"]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mainnet_soak_randomized_fault_injection() {
    use rand::{Rng, SeedableRng, rngs::StdRng};

    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let overrides = write_shallow_pruning_params("soak");
    let (mut light, light_client, address, light_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE).await;
    let (mut heavy, heavy_client, _h, heavy_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE + 2500).await;
    let light_pp = light_client.get_block_dag_info().await.unwrap().pruning_point_hash;
    let heavy_pp = heavy_client.get_block_dag_info().await.unwrap().pruning_point_hash;
    assert_fixture_premise(&light_client, &heavy_client).await;
    assert!(heavy_score > light_score);

    // Each round is reproducible from its seed, so make that reproducibility reachable: SOAK_SEEDS=2
    // replays the one failing round without paying for the nineteen that already passed. The full
    // sweep is still what the gate runs; this is for the loop between a failure and its fix.
    let rounds: Vec<usize> = match std::env::var("SOAK_SEEDS") {
        Ok(list) => list.split(',').filter_map(|s| s.trim().parse().ok()).collect(),
        Err(_) => (0..SOAK_ROUNDS).collect(),
    };
    assert!(!rounds.is_empty(), "SOAK_SEEDS was set but named no rounds");

    let mut failures: Vec<String> = Vec::new();
    for round in rounds.iter().copied() {
        let mut rng = StdRng::seed_from_u64(round as u64);
        // A band wide enough to cover a LAN and a bad intercontinental hop, drawn per round.
        let lo = rng.gen_range(5u64..=60);
        let hi = lo + rng.gen_range(20u64..=440);
        let delay = Duration::from_millis(lo)..Duration::from_millis(hi);
        let heavy_first: bool = rng.r#gen();
        let second_peer_delay_ms = rng.gen_range(0u64..=20_000);
        let cut_link: bool = rng.gen_bool(0.3);

        recovery_trace::clear();
        verification_trace::clear();
        let light_link = LaggyLink::spawn(light.p2p_port, delay.clone()).await;
        let heavy_link = LaggyLink::spawn(heavy.p2p_port, delay.clone()).await;

        let mut args = gated_args();
        args.override_params_file = Some(overrides.to_string_lossy().into_owned());
        let mut follower = Daemon::new_random_with_args(args, TOTAL_FD_LIMIT);
        let follower_client = follower.start().await;

        let (first_port, second_port) =
            if heavy_first { (heavy_link.local_port, light_link.local_port) } else { (light_link.local_port, heavy_link.local_port) };
        follower_client.add_peer(format!("127.0.0.1:{first_port}").try_into().unwrap(), true).await.unwrap();
        tokio::time::sleep(Duration::from_millis(second_peer_delay_ms)).await;
        follower_client.add_peer(format!("127.0.0.1:{second_port}").try_into().unwrap(), true).await.unwrap();

        // A partition that heals: the node must recover from it, not merely survive it.
        if cut_link {
            tokio::time::sleep(Duration::from_secs(5)).await;
            heavy_link.cut();
            tokio::time::sleep(Duration::from_secs(10)).await;
            heavy_link.heal();
        }

        let check = follower_client.clone();
        let target = heavy_score;
        let settled = tokio::time::timeout(Duration::from_secs(420), async move {
            loop {
                if check.get_block_dag_info().await.unwrap().virtual_daa_score >= target {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .unwrap_or(false);

        let info = follower_client.get_block_dag_info().await.unwrap();
        let on_heavy = info.pruning_point_hash == heavy_pp;
        let on_light = info.pruning_point_hash == light_pp;
        println!(
            "SOAK round={round} delay={lo}..{hi}ms heavy_first={heavy_first} second_after={second_peer_delay_ms}ms \
             cut={cut_link} settled={settled} on_heavy={on_heavy} on_light={on_light}"
        );

        // The property: never the wrong branch. Landing on the heavier one is convergence; landing
        // on the lighter one is the bug this whole effort exists to prevent.
        if on_light {
            failures.push(format!("round {round}: settled on the LIGHTER branch (seed={round})"));
            println!("{}", recovery_trace::diagnosis(recovery_trace::RecoveryStage::CandidateCommitted));
            // Why verification never happened, from the ring that costs nothing to fill. The stage
            // counts say a nomination did not become a proof request; this says which of the seven
            // ways that can happen actually happened.
            println!("{}", verification_trace::dump());
        } else if !on_heavy {
            failures.push(format!("round {round}: settled on neither branch (seed={round})"));
        }

        follower.shutdown();
        let _ = address;
    }

    heavy.shutdown();
    light.shutdown();
    let _ = std::fs::remove_file(&overrides);

    println!("SOAK complete: {} failures out of {}", failures.len(), rounds.len());
    assert!(failures.is_empty(), "randomized soak found failures:\n{}", failures.join("\n"));
}

/// The points in recovery at which the process is killed, one per round.
///
/// Chosen to bracket each irreversible step rather than to sample evenly: what matters is whether
/// the node comes back safe when it died holding something it had not finished — a proof it had not
/// checked, a reservation it had not redeemed, a staging area it had not committed, a chain it had
/// committed but not verified to the tip.
const RESTART_POINTS: [recovery_trace::RecoveryStage; 7] = [
    // Discovery: a chain has been described but nothing checked.
    recovery_trace::RecoveryStage::SummaryReceived,
    // A proof is outstanding — the lease is held and the slot is spent.
    recovery_trace::RecoveryStage::ProofRequestSent,
    // Evidence in hand, decision not yet made.
    recovery_trace::RecoveryStage::ProofValidated,
    // The latch is promised to a chain that has not had it yet.
    recovery_trace::RecoveryStage::PreferredCandidateReserved,
    // Mid-staging: headers are being validated against a chain not yet adopted.
    recovery_trace::RecoveryStage::IbdStartedForPreferredCandidate,
    // The adoption permit exists but the swap has not happened.
    recovery_trace::RecoveryStage::RecoveryPermitGranted,
    // The chain has been swapped in and the sync that justified it may not have finished.
    recovery_trace::RecoveryStage::CandidateCommitted,
];

/// Kill the node partway through recovery and require that what comes back is still safe.
///
/// Every other test here restarts nothing, so every one of them proves a property of a process that
/// ran to completion. A validator does not get that guarantee. It gets power cuts, OOM kills, and
/// operators typing `systemctl restart` at the least convenient moment — and the one moment that
/// matters most is while the node is holding a chain it has not finished checking.
///
/// The safety property is NOT "it always converges". Restarting mid-IBD is deliberately
/// unrecoverable: an interrupted IBD leaves the active consensus in a state nothing can vouch for,
/// so the node comes back QUARANTINED and stays there until an operator looks at it. Converging
/// would mean guessing, and guessing is what this whole effort exists to stop.
///
/// What must hold at every restart point, without exception:
///   - it never comes back participating, whatever it was doing when it died;
///   - it never ends up mining or attesting on the lighter chain;
///   - if it is not quarantined, it converges to the heavier chain on its own.
#[ignore = "restart fault injection: ~25 minutes; run explicitly"]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_node_killed_partway_through_recovery_comes_back_safe() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let overrides = write_shallow_pruning_params("restart");
    let (mut light, light_client, _l, light_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE).await;
    let (mut heavy, heavy_client, _h, heavy_score) = pruned_branch(&overrides, BLOCKS_TO_PRUNE + 2500).await;
    let light_pp = light_client.get_block_dag_info().await.unwrap().pruning_point_hash;
    let heavy_pp = heavy_client.get_block_dag_info().await.unwrap().pruning_point_hash;
    assert_fixture_premise(&light_client, &heavy_client).await;

    let mut failures: Vec<String> = Vec::new();
    for kill_at in RESTART_POINTS {
        recovery_trace::clear();

        // Lighter chain first, so the node has something provisional to lose.
        let mut args = gated_args();
        args.override_params_file = Some(overrides.to_string_lossy().into_owned());
        let mut follower = Daemon::new_random_with_args(args, TOTAL_FD_LIMIT);
        let follower_client = follower.start().await;
        follower_client.add_peer(format!("127.0.0.1:{}", light.p2p_port).try_into().unwrap(), true).await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        follower_client.add_peer(format!("127.0.0.1:{}", heavy.p2p_port).try_into().unwrap(), true).await.unwrap();

        // Wait for the moment being tested, but never forever: if recovery never reaches this stage
        // the round has nothing to say about restarts, and saying so beats a timeout that reads like
        // a restart bug.
        let reached = tokio::time::timeout(Duration::from_secs(300), async {
            loop {
                if recovery_trace::furthest_stage().is_some_and(|s| s >= kill_at) {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        })
        .await
        .unwrap_or(false);
        if !reached {
            failures.push(format!("{kill_at:?}: recovery never reached this stage, so the restart was never exercised"));
            follower.shutdown();
            continue;
        }

        // The kill. Not a graceful drain — the point is what survived on disk.
        let mut follower = follower.restarted(TOTAL_FD_LIMIT);
        let follower_client = follower.start().await;

        // First thing after coming back, before it has had any chance to re-sync: it must not be
        // telling miners and validators to go ahead. This is the assertion a persisted gate exists
        // for; an in-memory one would read `synced` here.
        if follower_client.get_sync_status().await.unwrap() {
            failures.push(format!("{kill_at:?}: reported synced immediately after restart — the gate did not survive"));
        }

        follower_client.add_peer(format!("127.0.0.1:{}", light.p2p_port).try_into().unwrap(), true).await.unwrap();
        follower_client.add_peer(format!("127.0.0.1:{}", heavy.p2p_port).try_into().unwrap(), true).await.unwrap();

        let check = follower_client.clone();
        let settled = tokio::time::timeout(Duration::from_secs(300), async move {
            loop {
                if check.get_block_dag_info().await.unwrap().virtual_daa_score >= heavy_score {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .unwrap_or(false);

        let info = follower_client.get_block_dag_info().await.unwrap();
        let on_heavy = info.pruning_point_hash == heavy_pp;
        let on_light = info.pruning_point_hash == light_pp;
        let participating = follower_client.get_sync_status().await.unwrap();
        println!("RESTART at={kill_at:?} settled={settled} on_heavy={on_heavy} on_light={on_light} participating={participating}");

        // The one rule with no exceptions: never acting on the lighter chain.
        if on_light && participating {
            failures.push(format!("{kill_at:?}: came back PARTICIPATING on the lighter chain"));
            println!("{}", recovery_trace::diagnosis(recovery_trace::RecoveryStage::CandidateCommitted));
        } else if !on_heavy && !participating {
            // Quarantined, or still working. Safe either way — it is not acting on anything.
            println!("  (held back rather than converged, which is the safe outcome, not a pass)");
        } else if !on_heavy {
            failures.push(format!("{kill_at:?}: participating on neither branch"));
        }

        follower.shutdown();
    }

    heavy.shutdown();
    light.shutdown();
    let _ = std::fs::remove_file(&overrides);
    let _ = light_score;

    println!("RESTART complete: {} failures out of {}", failures.len(), RESTART_POINTS.len());
    assert!(failures.is_empty(), "restart fault injection found failures:\n{}", failures.join("\n"));
}
