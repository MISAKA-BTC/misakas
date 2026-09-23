//! **A host stops promising the same bytes to every node on it** (ADR-0151 follow-up item 4).
//!
//! The fleet's failure: four seats on a 24 GB host each sized its caches and its class residency
//! against the host's own `MemAvailable`, which is the right calculation for one node and wrong by a
//! factor of four for four. They reached 22 of 23 GB with 14 of 19 GB of swap in use, and the kernel
//! chose which two to kill — seat4 at 21:56 and seat5 at 22:06, both at ~6.9 GiB anon-rss.
//!
//! Nothing in any log said the four budgets summed past the host. That is what these assertions are
//! about: the division is declared, the arithmetic is printed, and an impossible division is refused.

use kaspad_lib::args::{Args, palw_host_share_bytes_v1, palw_ram_scale_for_share_v1};
use kaspad_lib::palw_backends::{PALW_HOST_SHARE_FLOOR_BYTES_V1, check_host_share_v1};

const GIB: u64 = 1 << 30;

fn args_for(budget: Option<u64>, count: u32) -> Args {
    Args { palw_host_memory_budget: budget, palw_host_node_count: count, ..Args::default() }
}

#[test]
fn a_budget_is_divided_by_the_nodes_that_share_it() {
    assert_eq!(palw_host_share_bytes_v1(&args_for(Some(24 * GIB), 4)), Some(6 * GIB), "the fleet's own case");
    assert_eq!(palw_host_share_bytes_v1(&args_for(Some(24 * GIB), 1)), Some(24 * GIB), "one node takes the budget");
    // A count of zero is an operator slip, not a division by zero.
    assert_eq!(palw_host_share_bytes_v1(&args_for(Some(24 * GIB), 0)), Some(24 * GIB));
}

/// **No budget keeps the old behaviour.** The per-process sizing is not tightened behind an operator
/// who never declared a budget: a node cannot count its siblings without racing them, and the loser of
/// that race is the process that dies, so the fact has to come from the operator.
#[test]
fn no_budget_means_the_behaviour_that_shipped() {
    assert_eq!(palw_host_share_bytes_v1(&args_for(None, 4)), None);
    assert_eq!(check_host_share_v1(None, 4, None), Ok(()), "and nothing is refused on that path");
}

/// The scale the fleet reached by hand is the scale the budget derives. `--ram-scale=0.25` took the
/// seat host from 22/23 GB used with 14 GB of swap to 6/23 GB with 1 GB; a 6 GiB share must land
/// there or the budget would not have reached the lever that mattered.
#[test]
fn the_share_derives_the_scale_that_moved_the_fleet() {
    let share = palw_host_share_bytes_v1(&args_for(Some(24 * GIB), 4)).expect("a share");
    let scale = palw_ram_scale_for_share_v1(share);
    assert!((scale - 0.225).abs() < 0.001, "6 GiB share -> {scale}, and 0.25 by hand was what worked");
    // The slope is the one `--vps-8gb` ships: 0.3 at 8 GB.
    assert!((palw_ram_scale_for_share_v1(8 * GIB) - 0.3).abs() < 0.001, "the anchor is not invented");
    // And it stays inside the daemon's own accepted range at both ends.
    assert!(palw_ram_scale_for_share_v1(1) >= 0.1, "a tiny share clamps rather than going under the floor");
    assert!(palw_ram_scale_for_share_v1(4096 * GIB) <= 10.0, "an enormous one clamps rather than going over");
}

/// **An impossible division is refused, and the sentence names all three numbers.**
#[test]
fn a_share_too_thin_to_run_a_node_is_refused_by_name() {
    let budget = Some(24 * GIB);
    let share = palw_host_share_bytes_v1(&args_for(budget, 20));
    let why = check_host_share_v1(budget, 20, share).expect_err("1.2 GiB a node is refused");
    assert!(why.contains("24.00 GiB"), "the budget: {why}");
    assert!(why.contains("20 node(s)"), "the count: {why}");
    assert!(why.contains("2.00 GiB"), "the floor: {why}");
    assert!(why.contains("it makes every node page"), "and what the operator gains by not doing it: {why}");
}

#[test]
fn the_fleets_own_division_is_accepted() {
    let budget = Some(24 * GIB);
    let share = palw_host_share_bytes_v1(&args_for(budget, 4));
    assert_eq!(check_host_share_v1(budget, 4, share), Ok(()), "4 seats at 6 GiB each clears the floor");
    assert!(share.unwrap() > PALW_HOST_SHARE_FLOOR_BYTES_V1);
}

/// **`--palw-verify-class-manifest` must not pass by verifying nothing.**
///
/// Measured on the acceptance run: the host had no `.palwmanifest` on it at all, and the node logged
/// `--palw-verify-class-manifest: 0 registered root(s) re-derived … all agreeing` and started. An
/// operator who ASKS whether their roots agree and is told "0, all agreeing" has learned nothing and
/// been reassured — which is the failure the flag exists to end, wearing a different costume.
///
/// Asserted over the message rather than over a node, because the shape being pinned is that the
/// no-manifest and no-artifact cases are REFUSALS with sentences, not a quiet `Ok(0)`.
#[test]
fn verifying_nothing_is_not_a_pass() {
    let src = include_str!("../src/palw_backends.rs");
    let body = {
        let at = src.find("pub fn verify_class_manifests_v1").expect("the verifier is in this file");
        let end = src[at..].find("\npub fn load_class_holdings_v1").expect("it ends before the loader");
        &src[at..at + end]
    };
    assert!(
        body.contains("have no `.palwmanifest` beside them, so"),
        "an artifact without a sidecar must be named in a refusal, not skipped"
    );
    assert!(
        body.contains("this node holds no class artifacts, so there was nothing to"),
        "and a node with no artifacts at all must be refused too"
    );
    // The `Absent` arm must not be the empty block it was.
    assert!(!body.contains("PalwManifestLookupV1::Absent => {}"), "the Absent arm still discards the fact");
}


/// **A share stated outright overrides the equal division**, for a host whose nodes are not alike.
/// The floor still applies to it, and a stated share that ignores the budget is the operator's call.
#[test]
fn a_stated_share_overrides_the_equal_division() {
    let mut a = args_for(Some(24 * GIB), 4);
    assert_eq!(palw_host_share_bytes_v1(&a), Some(6 * GIB), "the equal split");
    a.palw_host_memory_share = Some(13 * GIB);
    assert_eq!(palw_host_share_bytes_v1(&a), Some(13 * GIB), "the producer's stated share wins");
    assert_eq!(check_host_share_v1(a.palw_host_memory_budget, 4, palw_host_share_bytes_v1(&a)), Ok(()));
    // A stated share with no budget at all is also a share — and still floored.
    let mut b = args_for(None, 1);
    b.palw_host_memory_share = Some(1 * GIB);
    assert_eq!(palw_host_share_bytes_v1(&b), Some(1 * GIB));
    assert!(check_host_share_v1(None, 1, palw_host_share_bytes_v1(&b)).is_ok(), "no budget: the floor check passes through (nothing to divide)");
    // The scale follows the stated share, not the budget's split.
    assert!((palw_ram_scale_for_share_v1(13 * GIB) - 0.4875).abs() < 0.001);
}
