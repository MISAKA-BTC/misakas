//! **lane: rcore/f1-forkchoice-attacks — the deep-reorg strict-economic-win fence, armed and
//! measured on testnet-12's own params.**
//!
//! The fence `palw_reorg_strict_economic_win` ships DORMANT (`None`) on every preset. This suite
//! proves the four fingerprint properties a Some-only fence must have (the 2026-09-18 lesson
//! `a-some-only-fence-needs-its-never-collapse` and `a-fence-at-a-scheduled-height-is-invisible-to-
//! the-fork-id`): arming it at a FUTURE height moves the params id and the schedule id but NOT the
//! consensus identity id (so a build that merely schedules it stays a peer of every un-upgraded
//! node), a scheduled `never()` collapses to the dormant identity, and the armed height registers
//! in the fork-id gate. The RULE the fence gates — an all-economic deep-reorg tie keeps the
//! incumbent instead of the candidate hash — is unit-tested in `palw_fork_authority_v2`, and the
//! processor chain that crosses the fence with DAA progress asserted lives in the pipeline suite
//! (`fcattack_probes`).
//!
//! Run: cargo test -p kaspa-consensus-core --test reorg_strict_win_fence -- --nocapture

use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

/// A height that is NOT already on testnet-12's schedule, so arming there registers a NEW fence in
/// the fork-id gate (the `a-fence-at-a-scheduled-height-is-invisible-to-the-fork-id` rule: an
/// existing scheduled height would not change the fork id).
const ARM_AT: u64 = 9_100_001;

#[test]
fn shipped_testnet_12_leaves_the_fence_dormant() {
    let t12 = t12();
    assert_eq!(t12.palw_reorg_strict_economic_win, None, "the fence ships dormant on testnet-12");
    // And it is listed by palw_fences_v1 (which is what feeds the fork-id gate).
    assert!(
        t12.palw_fences_v1().iter().any(|(name, _)| *name == "palw_reorg_strict_economic_win"),
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
    armed.palw_reorg_strict_economic_win = Some(ForkActivation::new(ARM_AT));

    // Arming a FUTURE height is a real, validatable ruleset.
    armed.validate_palw_v2().expect("arming the fence alone is a runnable ruleset");

    // The params id (the full fingerprint) MOVES — the Some-only write fires.
    assert_ne!(
        dormant.consensus_params_id(),
        armed.consensus_params_id(),
        "arming the fence moves the params fingerprint"
    );
    // The schedule id MOVES — for_each_fence writes the armed height.
    assert_ne!(
        dormant.consensus_schedule_id(),
        armed.consensus_schedule_id(),
        "arming the fence moves the schedule id"
    );
    // The identity id does NOT move — a scheduled (future) fence is normalised to never() and the
    // never()-collapse drops it, so a build that merely schedules it is a peer of an un-upgraded
    // one. This is the whole point of the fence machinery, and the property the 2026-09-18 partition
    // was about.
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
    // The `a-some-only-fence-needs-its-never-collapse` property, stated at the level it matters:
    // the IDENTITY id. `consensus_identity_id` normalises every armed height to never() and then
    // runs `normalize_values_a_scheduled_fence_drags_with_it`, which must drop `Some(never())` to
    // `None` — otherwise the Some-only fingerprint write turns the normalised never() into
    // "palw_reorg_strict_economic_win" + u64::MAX while a build without the field writes nothing,
    // the identities split, and the fleet partitions on deploy day over a height nobody reached.
    // So a build that carries `Some(never())` and one that carries `None` must share an identity.
    let dormant = t12();
    let mut never = t12();
    never.palw_reorg_strict_economic_win = Some(ForkActivation::never());
    assert_eq!(
        dormant.consensus_identity_id(),
        never.consensus_identity_id(),
        "the never()-collapse holds: Some(never()) has the dormant identity"
    );
    // The schedule id is a report, not a gate; the bare fence has no companion value, so an armed
    // never() (u64::MAX) is written by `for_each_fence` exactly as the absent case's u64::MAX —
    // equal. (The raw params_id fingerprint of an un-normalised Some(never()) does differ, which is
    // harmless: the identity above is what the handshake compares, and it collapses.)
    assert_eq!(dormant.consensus_schedule_id(), never.consensus_schedule_id(), "Some(never()) schedule id == dormant");
}

#[test]
fn armed_at_genesis_is_a_real_rule_difference_that_separates_identities() {
    // `Some(always())` (active at DAA 0) is NOT normalised away — a rule that is live at genesis is
    // a genuine identity difference. This guards against the fence being accidentally inert.
    let dormant = t12();
    let mut at_genesis = t12();
    at_genesis.palw_reorg_strict_economic_win = Some(ForkActivation::always());
    assert_ne!(
        dormant.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "a fence active at genesis separates identities"
    );
}
