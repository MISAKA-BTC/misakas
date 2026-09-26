//! **lane: rcore/hf-pptake2 — the pruning-proof / IBD adoption strict-economic-win fence, armed and
//! measured on testnet-12's own params.**
//!
//! **Since the post-launch release (int-4; the user's decision of 2026-09-26) testnet-12 SHIPS this
//! fence armed at DAA 750** (`PALW_T12_POST_LAUNCH_FENCE_DAA`), with the rest of
//! `PALW_T12_POST_LAUNCH_FENCES_V1`. What is said below of "as shipped" / "the release" now holds of
//! the LAUNCH ruleset — `palw_t12_launch_params_v1()`, the shipped ruleset with the post-launch list
//! set back to dormant, byte for byte the `b8564b88…` release a node that has not upgraded runs — and
//! the tests judge this fence alone against it; the shipped (armed) ids are pinned in the release
//! constant, and the shipped ruleset is asserted to carry the fence at 750.
//!
//! The fence `palw_pruning_proof_strict_economic_win` ships DORMANT (`None`) on every preset. This
//! suite proves the four fingerprint properties a Some-only fence must have (the 2026-09-18 lessons
//! `a-some-only-fence-needs-its-never-collapse` and `a-fence-at-a-scheduled-height-is-invisible-to-
//! the-fork-id`): arming it at a FUTURE height moves the params id and the schedule id but NOT the
//! consensus identity id (so a build that merely schedules it stays a peer of every un-upgraded
//! node), a scheduled `never()` collapses to the dormant identity, and the armed height registers
//! in the fork-id gate. The RULE the fence gates — an all-economic IBD/pruning-proof-adoption tie
//! keeps the incumbent instead of the candidate hash — is unit-tested in `palw_fork_authority_v2`
//! (`pruning_proof_adoption_keeps_the_incumbent_on_an_all_economic_tie`), and the adoption gate it
//! wires into is `IbdFlow::validate_staging_palw_order`.
//!
//! Run: cargo test -p kaspa-consensus-core --test pruning_proof_strict_economic_fence -- --nocapture

use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_POST_LAUNCH_FENCE_DAA, Params, palw_t12_launch_params_v1, palw_t12_shipped_params,
};
use kaspa_consensus_core::network::{NetworkId, NetworkType};

/// testnet-12 as LAUNCHED (the post-launch list dormant): the ruleset each test arms this fence over.
fn t12() -> Params {
    palw_t12_launch_params_v1()
}

/// testnet-12 as THIS build ships it — the post-launch release, every fence of
/// `PALW_T12_POST_LAUNCH_FENCES_V1` at DAA 750 (re-pinned by `scripts/t12-repin.sh` with it; the launch
/// release `0e8ec984e` was `b8564b88…` / `5de80e64…` / `93da24cc…`, which `palw_t12_launch_params_v1()`
/// still hashes to): params, identity, schedule — the same pins `palw_clock_lead_cap_is_t12_only`'s
/// `T12_WITH_THE_CAP` holds.
// re-pin 2026-09-26 @762784f40e9b: the DAA-750 post-launch release gains its 13th fence, palw_lane_accept_parents_first, armed at DAA 750 with the rest (params + schedule move; identity, genesis, premine unchanged) (was 1274ac12…, ae8cc4b7…)
const T12_RELEASE: (&str, &str, &str) = (
    "dbbc9104a2ee754f0f053a6e1614118979fd2c3dc87cbe6bffcf6dcaf4bd59c9",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "7c652212ab5337bda9508deeee2d2e119331856fce0bd27897f19dd66e552397",
);

/// A height that is NOT already on testnet-12's schedule (and distinct from the deep-reorg lane's
/// probe height), so arming there registers a NEW fence in the fork-id gate (the
/// `a-fence-at-a-scheduled-height-is-invisible-to-the-fork-id` rule).
const ARM_AT: u64 = 9_200_001;

#[test]
fn shipped_testnet_12_arms_the_fence_at_750() {
    let shipped = palw_t12_shipped_params();
    assert_eq!(
        shipped.palw_pruning_proof_strict_economic_win,
        Some(ForkActivation::new(PALW_T12_POST_LAUNCH_FENCE_DAA)),
        "testnet-12 ships the fence armed at DAA 750, the post-launch release's height"
    );
    // The shipped build announces the post-launch release's ids (T12_RELEASE, re-pinned with it).
    let ids = (
        shipped.consensus_params_id().to_string(),
        shipped.consensus_identity_id().to_string(),
        shipped.consensus_schedule_id().to_string(),
    );
    assert_eq!((ids.0.as_str(), ids.1.as_str(), ids.2.as_str()), T12_RELEASE, "testnet-12 is the post-launch release, to the id");
    // And through the network-id path a node takes.
    assert_eq!(
        Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12)).consensus_params_id(),
        shipped.consensus_params_id(),
        "Params::from(testnet-12) is the shipped testnet-12"
    );
    // The launch ruleset leaves it dormant, and it is listed by palw_fences_v1 (which feeds the fork-id gate).
    let launch = t12();
    assert_eq!(launch.palw_pruning_proof_strict_economic_win, None, "the launch ruleset leaves the fence dormant");
    assert!(
        launch.palw_fences_v1().iter().any(|(name, _)| *name == "palw_pruning_proof_strict_economic_win"),
        "the fence is enumerated in palw_fences_v1"
    );
    assert!(
        !kaspa_consensus_core::fork_id_v1::fork_id_gate_fences_v1(&launch).contains(&ARM_AT),
        "a dormant fence adds no height to the fork-id gate"
    );
    assert!(
        kaspa_consensus_core::fork_id_v1::fork_id_gate_fences_v1(&shipped).contains(&PALW_T12_POST_LAUNCH_FENCE_DAA),
        "the shipped build gates the fork id on 750"
    );
    shipped.validate_palw_v2().expect("shipped testnet-12 validates with the fence armed");
    launch.validate_palw_v2().expect("the launch ruleset validates with the fence dormant");
}

#[test]
fn arming_moves_the_params_id_and_the_schedule_id_but_not_the_identity_id() {
    let dormant = t12();
    let mut armed = t12();
    armed.palw_pruning_proof_strict_economic_win = Some(ForkActivation::new(ARM_AT));

    // Arming a FUTURE height is a real, validatable ruleset.
    armed.validate_palw_v2().expect("arming the fence alone is a runnable ruleset");

    // The params id (the full fingerprint) MOVES — the Some-only write fires.
    assert_ne!(dormant.consensus_params_id(), armed.consensus_params_id(), "arming the fence moves the params fingerprint");
    // The schedule id MOVES — for_each_fence writes the armed height.
    assert_ne!(dormant.consensus_schedule_id(), armed.consensus_schedule_id(), "arming the fence moves the schedule id");
    // The identity id does NOT move — a scheduled (future) fence is normalised to never() and the
    // never()-collapse drops it, so a build that merely schedules it is a peer of an un-upgraded one.
    assert_eq!(
        dormant.consensus_identity_id(),
        armed.consensus_identity_id(),
        "arming the fence at a future height leaves the consensus identity unchanged"
    );
    // The armed height registers in the fork-id gate.
    assert!(
        kaspa_consensus_core::fork_id_v1::fork_id_gate_fences_v1(&armed).contains(&ARM_AT),
        "the armed height is in the fork-id gate"
    );
}

#[test]
fn a_scheduled_never_collapses_to_the_dormant_identity() {
    // The `a-some-only-fence-needs-its-never-collapse` property, at the IDENTITY id: a build that
    // carries `Some(never())` and one that carries `None` must share an identity, or the Some-only
    // fingerprint write turns the normalised never() into "palw_pruning_proof_strict_economic_win"
    // + u64::MAX while a build without the field writes nothing, and the fleet partitions on deploy
    // day over a height nobody reached.
    let dormant = t12();
    let mut never = t12();
    never.palw_pruning_proof_strict_economic_win = Some(ForkActivation::never());
    assert_eq!(
        dormant.consensus_identity_id(),
        never.consensus_identity_id(),
        "the never()-collapse holds: Some(never()) has the dormant identity"
    );
    for scheduled in [ForkActivation::never(), ForkActivation::new(1_000_000)] {
        let mut p = t12();
        p.palw_pruning_proof_strict_economic_win = Some(scheduled);
        assert_eq!(p.consensus_identity_id(), dormant.consensus_identity_id(), "{scheduled:?} is absence in the identity");
    }
}

#[test]
fn armed_at_genesis_is_a_real_rule_difference_that_separates_identities() {
    // `Some(always())` (active at DAA 0) is NOT normalised away — a rule live at genesis is a
    // genuine identity difference. This guards against the fence being accidentally inert.
    let dormant = t12();
    let mut at_genesis = t12();
    at_genesis.palw_pruning_proof_strict_economic_win = Some(ForkActivation::always());
    assert_ne!(
        dormant.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "a fence active at genesis separates identities"
    );
}

#[test]
fn the_two_strict_economic_fences_are_independent_heights() {
    // The deep-reorg fence (rcore/f1-forkchoice-attacks) and this one gate the SAME strict-economic
    // principle at two different sites (the virtual processor's deep-reorg gate, and the IBD staging
    // commit). They are separate fields, each with its own PALW_T12_POST_LAUNCH_FENCES_V1 entry, and
    // the release arms both at DAA 750; here: this lane's field exists, is dormant as launched, and
    // moves independently of the deep-reorg one.
    let t12 = t12();
    assert_eq!(t12.palw_pruning_proof_strict_economic_win, None);
    assert!(t12.palw_fences_v1().iter().any(|(n, _)| *n == "palw_pruning_proof_strict_economic_win"));
    let mut one = palw_t12_shipped_params();
    one.palw_pruning_proof_strict_economic_win = Some(ForkActivation::new(ARM_AT));
    assert_eq!(one.palw_reorg_strict_economic_win, Some(ForkActivation::new(PALW_T12_POST_LAUNCH_FENCE_DAA)), "the other one stays");
}
