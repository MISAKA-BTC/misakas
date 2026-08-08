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
    shallow_pruning::{BLOCKS_TO_PRUNE, write_shallow_pruning_params},
    utils::wait_for,
};
use kaspa_addresses::Address;
use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_consensus::params::SIMNET_PARAMS;
use kaspa_grpc_client::GrpcClient;
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
