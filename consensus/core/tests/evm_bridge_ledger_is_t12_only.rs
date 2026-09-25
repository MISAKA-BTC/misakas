//! **The bridge ledger is testnet-12's alone, and it is dormant everywhere else.**
//!
//! `Params::evm_bridge_ledger_activation_daa_score` decides which EVM transactions an accepting block
//! executes (a draw the backed supply cannot cover is a class-2 skip) and makes the committed
//! `evm_total_native_balance` a checked ledger, so it is a consensus fence. It is `0` on testnet-12
//! and `u64::MAX` (inert) on every other preset, and it is written FINITE-ONLY in every writer —
//! `consensus_params_id` and the `for_each_fence` walk under the identity and schedule ids — so
//! testnet-11, devnet and mainnet fingerprint byte-identically to the build before the field existed.
//! Those three are pinned to their numbers by `palw_clock_floor_is_t12_only.rs`
//! (`testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_floor`); this file pins the
//! relations.
//!
//! Run: `cargo test -p kaspa-consensus-core --test evm_bridge_ledger_is_t12_only -- --nocapture`

use kaspa_consensus_core::config::params::{
    Params, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_shipped_params,
};

fn inert_twin(p: &Params) -> Params {
    let mut twin = p.clone();
    twin.evm_bridge_ledger_activation_daa_score = u64::MAX;
    twin
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

/// testnet-12 at the parent (`8e1ce34a`) with its offence-attribution fence taken away — the value
/// `palw_offence_attribution_is_t12_only` pinned there as `T12_BEFORE_THE_ATTRIBUTION`.
const T12_AT_THE_PARENT_WITHOUT_THE_ATTRIBUTION: (&str, &str, &str) = (
    "f5eee4746137d0e40a58fd5e99bfdc47738800550ad313811be7b1eed185821b",
    "75666d42d34312c76691de9d1f876465e58502e48d38bde1aa3d7cce0906e7c9",
    "75282ad229d5f8617aa5b3f5b818ddab9defdc893f1276c6a079c080af7debff",
);

/// **The ledger is the only thing this change moved on testnet-12.** Take it away (and the
/// attribution fence, whose pin is the one number recorded at the parent) and testnet-12's ruleset,
/// identity and schedule are exactly the parent's.
///
/// ADR-0152 §4-quater's `palw_class_verify_deadline` landed beside the ledger (merged after the
/// parent), so it is taken away too — the fence cleared, its state-params mirror synced, and the
/// pruning depth re-derived without it (its P-1 lattice, 74,920, back to the parent's 12,002), as a
/// build without the fence derives it. Its own "only thing that moved" pin is
/// `palw_offence_attribution_is_t12_only`'s `T12_BEFORE_THE_ATTRIBUTION_AND_THE_DEADLINE`.
#[test]
fn the_ledger_is_the_only_thing_that_moved_testnet12() {
    let mut parent = inert_twin(&palw_t12_shipped_params());
    parent.palw_class_verify_deadline = None;
    parent.sync_palw_class_verify_deadline();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &parent.palw_consensus_mode else {
        panic!("testnet-12 is ConsensusV2")
    };
    parent.blockrate.pruning_depth =
        kaspa_consensus_core::config::params::palw_v2_pruning_depth_v1(&parent.blockrate, bundle, parent.palw_da_court, None);
    assert_eq!(parent.blockrate.pruning_depth, 12_002, "the parent's depth");
    println!("testnet-12 at the parent (the ledger and the deadline fence taken away): {:?}", ids(&parent));
    parent.palw_offence_attribution = None;
    // ADR-0152-adjacent: the Activation Pool landed after this pin was taken; it is taken away
    // here too, and `palw_activation_pool_is_t12_only` pins what it moves.
    parent.palw_activation_pool = None;
    // The readiness-V2 horizon (user decision 2026-09-25, readiness capacity option (a)) landed after
    // it too; taken away as well (its mirror synced), and `palw_readiness_horizon_is_t12_only` pins
    // what it moves.
    parent.palw_readiness_v2_max_age_spans = None;
    parent.sync_palw_readiness_v2_max_age_spans();
    let now = ids(&parent);
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_AT_THE_PARENT_WITHOUT_THE_ATTRIBUTION);
}

/// Armed from genesis on testnet-12; inert on every other shipped preset.
#[test]
fn the_ledger_is_armed_on_testnet12_and_nowhere_else() {
    assert_eq!(palw_t12_shipped_params().evm_bridge_ledger_activation_daa_score, 0, "testnet-12 arms it at genesis");
    for (name, p) in [("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params()), ("mainnet", mainnet_shipped_params())]
    {
        assert_eq!(p.evm_bridge_ledger_activation_daa_score, u64::MAX, "{name}: the ledger must be inert");
    }
}

/// testnet-12's ruleset names the ledger: a node without it announces another ruleset, and — the
/// fence being in force from block one — cannot share testnet-12's identity.
#[test]
fn the_ledger_moves_testnet12s_fingerprint_and_identity() {
    let t12 = palw_t12_shipped_params();
    let twin = inert_twin(&t12);
    assert_ne!(t12.consensus_params_id(), twin.consensus_params_id(), "testnet-12's ruleset names the ledger");
    assert_ne!(t12.consensus_identity_id(), twin.consensus_identity_id(), "in force from block one: two identities");
    assert_ne!(t12.consensus_schedule_id(), twin.consensus_schedule_id(), "and the schedule the operator log prints names it");
    println!("testnet-12 params id with the ledger {} / inert twin {}", t12.consensus_params_id(), twin.consensus_params_id());
}

/// On a network that does not arm it, an inert ledger is absence, and scheduling it at a future
/// height is a rollout, not a partition: the identity two nodes must share to peer stays the same,
/// while the params id and the schedule id — the ones an operator reads — say a height is coming.
#[test]
fn on_testnet11_an_inert_ledger_is_absence_and_a_future_height_is_not_yet_a_rule() {
    let t11 = palw_rc_shipped_params();
    let mut scheduled = t11.clone();
    scheduled.evm_bridge_ledger_activation_daa_score = 9_000_000;
    assert_eq!(scheduled.consensus_identity_id(), t11.consensus_identity_id(), "a future height is not yet a rule");
    assert_ne!(scheduled.consensus_params_id(), t11.consensus_params_id(), "the ruleset a node announces names the height");
    assert_ne!(scheduled.consensus_schedule_id(), t11.consensus_schedule_id(), "and so does the schedule");
    assert!(scheduled.fence_schedule_v1().contains(&9_000_000), "the height is one this build changes the rules at");
    assert!(!t11.fence_schedule_v1().contains(&9_000_000));
}
