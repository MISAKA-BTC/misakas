//! **RFC-0026 §10 — the T01–T15 integration acceptance campaign at node level** (lane r5-t-campaign, 2026-10-22).
//!
//! Every test here runs on the kernel-route harness of the parent module (`g14_kernel_route_e2e`): objects go through the node's
//! mempool, its own block template and the chain block's fold (`Net::send`), funded by real Full-UTXO carriers signed with ML-DSA-87
//! (`Net::carrier` / `sign_spend`), every route object signed by the bond the fold names (`Net::route`). Verifiers are built only
//! from the node's read API (`World::fresh`). No fold-only shortcut: nothing below calls a ledger method directly to change state.
//!
//! The T table (docs/design/palw/integration/g14-active-completion-ledger-2026-10-21.md §"RFC-0026 T01–T15 completion audit") asks
//! for more than this base carries. Where a scenario needs an object of a PARALLEL lane that is not on base `64bc87dcd` — the paid
//! public ingress object (r5-f03-ingress), the shared-court op / F02 wire (r5-f02-wire), escrow settlement paths
//! (r5-escrow-settlement), position terminal objects (r5-p2-terminal), dependency leases / relay authority (r5-clock-deps-relay),
//! funded original admission (r5-original-admission) — the test is written against the INTENDED API, carries a
//! `// DEPENDS ON lane X` comment, is `#[ignore = "needs lane X merge"]`, and its body compiles only with the consensus crate feature
//! `g14-t-campaign-merged` (so this crate keeps building before the merge). Central verification: merge the lanes, fix the API names,
//! build with `--features g14-t-campaign-merged`, run with `--include-ignored`.
//!
//! Each active test covers the PART of its T item the base can carry and says which part is pending; none of them claims the whole
//! T item (RFC-0026 §8.1: a component is not the contract).
//!
//! Scale: T11 reads `G14_T11_N` (default 3; the audit's full run is 10,000) and T12 reads `G14_T12_N` (default 3).
use super::*;

mod t01_t03;
mod t04_t05;
mod t06_t07;
mod t08_t09;
mod t10_t11;
mod t12_t13;
mod t14;
mod t15;
mod value_gates;

/// A scaled campaign parameter: `var` from the environment, else `default` (never zero).
pub(super) fn campaign_n(var: &str, default: usize) -> usize {
    std::env::var(var).ok().and_then(|v| v.parse::<usize>().ok()).filter(|n| *n > 0).unwrap_or(default)
}

/// The DAA of every block of the selected chain after the first `skip`, in order.
pub(super) fn chain_daas_after(net: &Net, skip: usize) -> Vec<u64> {
    chain_blocks(&net.chain, net.chain.sink()).into_iter().skip(skip).map(|b| b.header.daa_score).collect()
}

pub(super) fn chain_len(net: &Net) -> usize {
    chain_blocks(&net.chain, net.chain.sink()).len()
}

/// **A hostile carrier**: `object` (already forged by the caller) offered to the node's mempool from `card`'s funding chain. If the
/// pool refuses it the funding chain is rolled back (the carrier never existed) and `false` is returned; if the pool takes it, the
/// node's template must carry it and the next block folds it, and `true` is returned. Either way the caller asserts no effect.
pub(super) async fn offer_forged(net: &mut Net, card: usize, object: Obj) -> bool {
    let saved = net.funding[card].clone();
    let tx = net.carrier(card, &object);
    if net.mempool(&tx).is_err() {
        net.funding[card] = saved;
        let ttpb = net.ttpb();
        net.chain.heartbeat(ttpb, Vec::new()).await;
        return false;
    }
    let ttpb = net.ttpb();
    let carrying = net.chain.heartbeat(ttpb, vec![tx.clone()]).await;
    assert!(carrying.transactions.iter().any(|t| t.id() == tx.id()), "a pool-accepted carrier is carried by the template");
    net.chain.heartbeat(ttpb, Vec::new()).await;
    true
}

/// A lying (or honest) claim by `producer` covered by EVERY seat of its interim assignment; returns it with its seats.
pub(super) async fn covered_claim(w: &mut World, producer: usize, lie: bool) -> (Claim, Vec<usize>) {
    let job = w.job().await;
    let claim = w.claim(producer, &job, lie).await;
    let seats = w.seats(&claim.id);
    w.cover(&claim.id).await;
    (claim, seats)
}

/// The number of genesis harness cards (bonded actors) on this network.
pub(super) fn cards(w: &World) -> usize {
    w.net.chain.bonds.len()
}

/// The pending-lane gate: an `#[ignore]`d campaign test whose body is not compiled on this base panics with the lane it waits for.
#[allow(unused_macros)]
macro_rules! pending_lane {
    ($lane:literal) => {
        panic!(concat!(
            "DEPENDS ON lane ",
            $lane,
            ": merge it, then build the consensus crate with --features g14-t-campaign-merged and run with --include-ignored"
        ))
    };
}
pub(super) use pending_lane;

/// **Studio T4 (2026-10-22)**: the gate of an `#[ignore]`d campaign test whose lanes ARE merged on `g14/integration-r7` but whose
/// intended API does not exist there and whose path is refused on a node until a value is adopted. Its intended body is kept under
/// the OFF feature `g14-t-campaign-intended-api`; run with `--ignored`, the test panics with the real blocker (a BLOCKED row, never
/// a PASS). The value gates themselves are asserted by `value_gates::t_campaign_value_gated_paths_are_refused_in_a_node_build`.
#[allow(unused_macros)]
macro_rules! blocked_on {
    ($why:literal) => {
        panic!(concat!("BLOCKED (lanes merged on g14/integration-r7; intended API absent / value not adopted): ", $why))
    };
}
#[allow(unused_imports)]
pub(super) use blocked_on;
