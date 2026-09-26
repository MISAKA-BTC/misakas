//! **lane: rcore/hf-pptake2 — the pruning-proof / IBD adoption strict-economic-win fence, armed and
//! measured on testnet-12's own params.**
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

use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};

fn t12() -> Params {
    palw_t12_shipped_params()
}

/// The launch release's own ids (rcore/int-3 `0e8ec984e`, re-pin `9c717c16d`): `(params, identity,
/// schedule)` — the same pins the sibling fence suites hold. A dormant build must announce these
/// three unchanged, or the shipped fingerprint b8564b88… moved.
const T12_LAUNCH_RELEASE: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// A height that is NOT already on testnet-12's schedule (and distinct from the deep-reorg lane's
/// probe height), so arming there registers a NEW fence in the fork-id gate (the
/// `a-fence-at-a-scheduled-height-is-invisible-to-the-fork-id` rule).
const ARM_AT: u64 = 9_200_001;

#[test]
fn shipped_testnet_12_leaves_the_fence_dormant() {
    let t12 = t12();
    assert_eq!(t12.palw_pruning_proof_strict_economic_win, None, "the fence ships dormant on testnet-12");
    // A node update carrying the dormant fence IS the launch release in every id it announces —
    // params fingerprint, identity and schedule (the fence is hashed and visited SOME-ONLY) — so it
    // needs no re-pin and prints exactly what the release prints.
    let ids = (
        t12.consensus_params_id().to_string(),
        t12.consensus_identity_id().to_string(),
        t12.consensus_schedule_id().to_string(),
    );
    assert_eq!(
        (ids.0.as_str(), ids.1.as_str(), ids.2.as_str()),
        T12_LAUNCH_RELEASE,
        "the dormant build is the launch release, to the id (b8564b88… unmoved)"
    );
    // And through the network-id path a node takes.
    assert_eq!(
        Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12)).consensus_params_id(),
        t12.consensus_params_id(),
        "Params::from(testnet-12) is the shipped testnet-12"
    );
    // And it is listed by palw_fences_v1 (which is what feeds the fork-id gate), so the integrator's
    // PALW_T12_POST_LAUNCH_FENCES_V1 entry {name: "pruning_proof_strict_economic_win"} finds it.
    assert!(
        t12.palw_fences_v1().iter().any(|(name, _)| *name == "palw_pruning_proof_strict_economic_win"),
        "the fence is enumerated in palw_fences_v1"
    );
    // Dormant, so it contributes nothing to the fork-id gate.
    assert!(
        !kaspa_consensus_core::fork_id_v1::fork_id_gate_fences_v1(&t12).contains(&ARM_AT),
        "a dormant fence adds no height to the fork-id gate"
    );
    t12.validate_palw_v2().expect("shipped testnet-12 validates with the fence dormant");
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
    // commit). They are separate fields, so the integrator can arm each at DAA 500 with its own
    // PALW_T12_POST_LAUNCH_FENCES_V1 entry. Here we only assert this lane's field exists and is
    // dormant; the deep-reorg field arrives on its own branch and is armed alongside.
    let t12 = t12();
    assert_eq!(t12.palw_pruning_proof_strict_economic_win, None);
    assert!(t12.palw_fences_v1().iter().any(|(n, _)| *n == "palw_pruning_proof_strict_economic_win"));
}
