//! **Nothing the 2026-09-19/20 reward work added is live, and this is where that is checked.**
//!
//! The re-audit's most important sentence is not a finding: every fence this work adds is `None` on
//! every shipped preset, so testnet-11 behaves exactly as it did before any of it. That claim is
//! easy to make and easy to be wrong about — a fence that reaches the fingerprint changes network
//! identity on deploy day whether or not it is armed, and a fence that is armed by accident changes
//! consensus. So it is pinned to the number itself.
//!
//! If a change is meant to be dormant and this file goes red, the change is not dormant. If a
//! change is meant to ARM, this file is the one to update deliberately, in the commit that arms it,
//! with the new height and the new fingerprint written down together.

use kaspa_consensus_core::config::params::{
    palw_rc_shipped_params, MAINNET_PARAMS, PALW_RC_ANCHOR_CLOCK_FENCE_DAA, PALW_RC_EXECUTION_QUANTA_FENCE_DAA,
    PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA,
};

/// **The shipped release's fingerprint, moved 2026-09-21 by ADR-0133 §7's seat gate at DAA 8,100
/// and §11.3's class receipt window at DAA 8,160.**
///
/// Previous (`8e1970dd…`) was ADR-0133 S3 at 8,600 and S2 at 8,700, over execution quanta at 7,800
/// and ADR-0144 §9 at 8,500. A scheduled future fence writes Some-only into the params id and the
/// schedule id; the identity does not move (the fence normalises out until it fires), which is why
/// only two of the three constants below change with this pair.
///
/// **Re-pinned 2026-09-23 for the FIFTH certified family** (`PALW-QWEN36-V6`, `7681c203`), and this is
/// the one change on the branch that moves testnet-11's IDENTITY as well as its ruleset: the root of
/// the certified family set (`court_e2e_root`) sits inside every RC bundle, so both ids below move
/// while the schedule id does not (no height was added). Deliberate, not dormant — testnet-11 is
/// superseded by testnet-12 and its chain past DAA 7,219 is unsyncable by a correct validator
/// (`docs/testnet-12-regenesis-2026-09-23.md`), so no node on this build is meant to rejoin it. The
/// params-id pin in `config::params` (`shipped_presets_have_pinned_fingerprints`) moved in the same
/// commit; this file was missed then and caught by the KV track's full run.
/// Previous: params `79b49c238c46b0d97ab9b46d79fd5f85f8b50da623921a53f0af361515d50640`,
/// identity `12e975effe2ef067e039c07b1af4199b7c4122068da7ccc2dda989cf3f4ec4d2`.
///
/// **Re-pinned again 2026-09-23 for the economic audit's state schema v21** (`b5c5f324`, merged in
/// `5d90a18f`): `PALW_STATE_V2_VERSION` and the unconditionally-hashed `settled_attempt_finals` sit in
/// every V2 bundle's state, so both ids move on every V2 preset while the schedule id does not (the
/// audit's fence is `None` here, and Some-only). The params-id pin in `config::params` moved in
/// `cca52e92`; this file was missed there. Previous: params `33bdff0b…`, identity `ca11f05d…`.
///
/// **And once more for ADR-0152's v22 skeleton** (`PALW_STATE_V2_VERSION` 21 -> 22, hashed into the
/// V2 arm): the params and identity ids move with the version alone — `palw_rcore_plus` and C7's list
/// are Some-only / non-empty-only and absent here — and the schedule id does not move. Previous:
/// params `c99bb4f4…`, identity `19dbdbb8…`.
const T11_CONSENSUS_PARAMS_ID: &str = "bd633ce933974d4134676efbdaf46b269dc2fb78f007e0907479aabd4d743f29";
const T11_CONSENSUS_IDENTITY_ID: &str = "44cb8fd729e9575a6e3b1e72c466b8abce4b9ecd81bb556685c9ba225487117f";
/// **Re-pinned 2026-09-23 for ADR-0151's `palw_economic_safety`, and only this one of the three.**
///
/// `consensus_schedule_id` writes every score `for_each_fence` visits, and a `None` Some-only fence
/// is visited through a `u64::MAX` sentinel — so ADDING a fence moves this id on every preset,
/// dormant or not. The other two did not move and that is the point: `consensus_params_id` (the
/// ruleset a node announces) and `consensus_identity_id` (what two nodes must share to peer) are
/// Some-only in the sense that matters, so testnet-11 peers and folds exactly as it did.
///
/// This id is explicitly NOT a gate — it exists so a mismatch can be reported precisely — which is
/// why a new dormant fence may move it and why re-pinning is the whole remedy.
const T11_CONSENSUS_SCHEDULE_ID: &str = "5a1d8d5679e0e8d7e9022255668fd5d4b3e4c8a6c367acf3882c6a3d480d8b64";
const MAINNET_CONSENSUS_PARAMS_ID: &str = "badaa8e90f14ef0074048d6b18660864855be8ab854d0ecb01dfbb62171538e1";

#[test]
fn the_shipped_release_fingerprint_did_not_move() {
    let rc = palw_rc_shipped_params();
    assert_eq!(rc.consensus_params_id().to_string(), T11_CONSENSUS_PARAMS_ID, "the ruleset a node announces");
    assert_eq!(rc.consensus_identity_id().to_string(), T11_CONSENSUS_IDENTITY_ID, "the identity two nodes must share to peer");
    assert_eq!(rc.consensus_schedule_id().to_string(), T11_CONSENSUS_SCHEDULE_ID, "the schedule the operator log names");
    assert_eq!(
        MAINNET_PARAMS.consensus_params_id().to_string(),
        MAINNET_CONSENSUS_PARAMS_ID,
        "and mainnet, which this work must not have touched at all"
    );
}

/// **Every fence the reward work added is dormant on every preset**, read at runtime rather than
/// off the base constants — the process error that produced a false CRITICAL in the first round.
#[test]
fn every_fence_the_reward_work_added_is_dormant_everywhere() {
    use kaspa_consensus_core::config::params::{DEVNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS};
    for (net, p) in [
        ("shipped release", palw_rc_shipped_params()),
        ("mainnet", MAINNET_PARAMS),
        ("testnet", TESTNET_PARAMS),
        ("simnet", SIMNET_PARAMS),
        ("devnet", DEVNET_PARAMS),
    ] {
        for (name, fence) in [
            ("palw_canonical_work", p.palw_canonical_work),
            ("palw_admission_independence", p.palw_admission_independence),
            ("palw_fp_derived_work", p.palw_fp_derived_work),
        ] {
            assert_eq!(fence.map(|f| f.daa_score()), None, "{net}: {name} must be dormant until a height is chosen");
        }
    }
}

/// **And the bundle is what an arming build has to satisfy**, so the height cannot be chosen for
/// one fence in isolation later. The shipped release does not satisfy it — it arms none of the
/// three and `palw_artifact_root_ownership` is commented out on its card — which is exactly why a
/// build that wants the economy has to change more than one line.
#[test]
fn arming_one_fence_of_the_bundle_on_the_release_is_refused() {
    use kaspa_consensus_core::config::params::ForkActivation;
    let rc = palw_rc_shipped_params();
    let registry = rc.palw_model_registry.expect("the shipped release arms the registry").daa_score();

    let mut one = rc.clone();
    one.palw_canonical_work = Some(ForkActivation::new(registry + 1_000));
    let refusal = one.validate_palw_v2().expect_err("one fence of the bundle, on the release, is refused");
    assert!(format!("{refusal:?}").contains("arm together or not at all"), "{refusal:?}");

    let mut three = rc.clone();
    for f in [&mut three.palw_canonical_work, &mut three.palw_admission_independence, &mut three.palw_fp_derived_work] {
        *f = Some(ForkActivation::new(registry + 1_000));
    }
    let refusal = three.validate_palw_v2().expect_err("and three without ADR-0143 is still refused");
    assert!(format!("{refusal:?}").contains("palw_artifact_root_ownership"), "{refusal:?}");

    three.palw_artifact_root_ownership = Some(ForkActivation::new(registry));
    three.validate_palw_v2().expect("with ADR-0143 at or below it, the bundle assembles");
}

/// **ADR-0144 P4 is satisfied, so 0147–0149 may arm — and the shipped release still does not.**
/// The search in `palw_arbitrage_search_v1` reports 1.000000× weight-per-MAC across the four
/// shipped classes. That is the gate 0146 asked for. This file still pins every bundle fence
/// `None`: choosing a height is a different commit from knowing the height would be legal.
#[test]
fn p4_does_not_forbid_the_bundle_and_the_release_still_does_not_arm_it() {
    use kaspa_consensus_core::palw_arbitrage_search_v1::{
        palw_arbitrage_search_shipped_classes_v1, PALW_ARBITRAGE_BOUND_ARITHMETIC_V1,
    };
    let bound = palw_arbitrage_search_shipped_classes_v1();
    assert_eq!(bound.weight_per_mac_spread, PALW_ARBITRAGE_BOUND_ARITHMETIC_V1);
    assert!(bound.scalar_is_arithmetic_only, "collapsing traffic into the scalar would be a coefficient, which P4 forbids");
    let rc = palw_rc_shipped_params();
    assert_eq!(rc.palw_canonical_work, None);
    assert_eq!(rc.palw_admission_independence, None);
    assert_eq!(rc.palw_fp_derived_work, None);
}

/// **Where the new denominator reaches, and where it does not** — a self-red-team of the item 6
/// fix, pinned so the reasoning survives the commit that made it.
///
/// `work_price_unit_at` feeds two callers, and moving it from "a MAX over the registered classes"
/// to "the chain's work target" is a large change in magnitude. That is safe only because of where
/// the two callers sit:
///
/// * **The work-priced escrow is not the payer past the bundle.** The fold picks the escrow as
///   `economics.priced_reward(..)` when a claim carries an ADR-0132 snapshot and falls back to
///   `work_priced_escrow` only when it does not. `validate_palw_v2` requires `palw_work_target`
///   armed for the bundle, and the work target in turn requires `palw_economic_payout` at or below
///   ITSELF — so every claim accepted past the bundle carries a snapshot, and the fallback is
///   unreachable there. The denominator's magnitude therefore does not move anybody's pay.
/// * **The execution lane's credit is a CLAMP, not a ratio.** `palw_execution_credit_v1` is
///   `exposure_pwu.min(unit)`, and under the old unit — a max over the same classes — the clamp
///   was a no-op for every `DerivedV1` class by construction. A larger unit keeps it a no-op.
///   What changes is only that a class whose single draw exceeds a whole block's compute budget is
///   now clamped at that budget, which is the honest answer rather than a regression.
///
/// If a later branch makes the work-priced escrow reachable past the bundle, this test goes red
/// and the magnitude question has to be answered again rather than inherited.
#[test]
fn the_work_price_denominator_cannot_reach_the_payer_past_the_bundle() {
    let fold = include_str!("../src/palw_state_v2.rs");
    let body = &fold[..fold.find("\n#[cfg(test)]").expect("the tests follow the fold")];
    assert!(
        body.contains("None if self.extras.work_priced_reward_active => self.work_priced_escrow(claim)"),
        "the work-priced escrow is still only the fallback for a claim with no economics snapshot"
    );

    let params = include_str!("../src/config/params.rs");
    assert!(
        params.contains("palw_work_target is armed without palw_model_registry and palw_economic_payout armed at or below"),
        "and the work target still requires the payout at or below it, which is what makes the snapshot always present"
    );
    assert!(
        params.contains("economic bundle is armed without palw_model_registry and palw_work_target armed at ONE height"),
        "and the bundle still requires the work target, which is what carries the payout with it"
    );

    // The lane's credit is a clamp, so a larger unit cannot reduce anyone's credit.
    use kaspa_consensus_core::palw_execution_lane_v1::palw_execution_credit_v1;
    let draw = 83_102_171_136u64; // one draw of the shipped dense row, in MAC-equivalents
    let old_unit = 9_000_776u64; // the live MAX over declared per-inference values
    let big_unit = 306_000_000_000u64; // a block's worth of compute, the order W sits at
    assert_eq!(palw_execution_credit_v1(draw, big_unit), draw, "a unit above the draw does not clamp it");
    assert_eq!(palw_execution_credit_v1(old_unit, big_unit), old_unit, "nor a smaller measure");
    assert_eq!(palw_execution_credit_v1(draw, old_unit), old_unit, "the OLD unit is what clamped");
}

/// **The DAA-clock trio sits past the 2026-09-21 BASE-0 stall**, so a bonded attempt parent
/// no longer buys an hour of heartbeat silence once the fence fires. Lottery, clock and
/// cursor stay one height (`validate_palw_v2`).
#[test]
fn the_daa_clock_fence_is_past_the_stalled_tip() {
    use kaspa_consensus_core::config::params::ForkActivation;
    let rc = palw_rc_shipped_params();
    assert_eq!(PALW_RC_ANCHOR_CLOCK_FENCE_DAA, 7_780);
    assert!(PALW_RC_ANCHOR_CLOCK_FENCE_DAA > 7_680, "past the 2026-09-21 BASE-0 stall at DAA 7,680");
    assert!(PALW_RC_ANCHOR_CLOCK_FENCE_DAA > 7_732, "past the second BASE-0 stall at DAA 7,732");
    assert_eq!(rc.palw_anchor_clock, Some(ForkActivation::new(PALW_RC_ANCHOR_CLOCK_FENCE_DAA)));
    assert_eq!(rc.palw_clock_cursor, Some(ForkActivation::new(PALW_RC_ANCHOR_CLOCK_FENCE_DAA)));
    assert_eq!(rc.palw_single_lottery, Some(ForkActivation::new(PALW_RC_ANCHOR_CLOCK_FENCE_DAA)));
    assert!(!rc.palw_anchor_clock_at(PALW_RC_ANCHOR_CLOCK_FENCE_DAA - 1));
    assert!(rc.palw_anchor_clock_at(PALW_RC_ANCHOR_CLOCK_FENCE_DAA));
    assert!(!rc.palw_single_lottery_at(PALW_RC_ANCHOR_CLOCK_FENCE_DAA - 1));
    assert!(rc.palw_single_lottery_at(PALW_RC_ANCHOR_CLOCK_FENCE_DAA));
}

/// **ADR-0144 §9 rides a later unused height**, so already-committed Valid receipts are not
/// retroactively locked. Mainnet stays unset. Identity does not move (scheduled fence).
#[test]
fn the_objective_offence_fence_is_past_the_clock() {
    use kaspa_consensus_core::config::params::ForkActivation;
    let rc = palw_rc_shipped_params();
    assert_eq!(PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA, 8_500);
    assert!(PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA > PALW_RC_ANCHOR_CLOCK_FENCE_DAA);
    assert_eq!(rc.palw_objective_offence, Some(ForkActivation::new(PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA)));
    assert!(!rc.palw_objective_offence_at(PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA - 1));
    assert!(rc.palw_objective_offence_at(PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA));
    assert_eq!(MAINNET_PARAMS.palw_objective_offence, None);
}

/// **Spend-once execution quanta ride DAA 7,800**, ~40 DAA past the live tip, so a rolling fleet
/// can arm the mint before the first Final lands. Mainnet stays unset.
#[test]
fn the_execution_quanta_fence_is_past_the_clock() {
    use kaspa_consensus_core::config::params::ForkActivation;
    let rc = palw_rc_shipped_params();
    assert_eq!(PALW_RC_EXECUTION_QUANTA_FENCE_DAA, 7_800);
    assert!(PALW_RC_EXECUTION_QUANTA_FENCE_DAA > PALW_RC_ANCHOR_CLOCK_FENCE_DAA);
    assert!(PALW_RC_EXECUTION_QUANTA_FENCE_DAA < PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA);
    assert_eq!(rc.palw_execution_quanta, Some(ForkActivation::new(PALW_RC_EXECUTION_QUANTA_FENCE_DAA)));
    assert!(!rc.palw_execution_quanta_at(PALW_RC_EXECUTION_QUANTA_FENCE_DAA - 1));
    assert!(rc.palw_execution_quanta_at(PALW_RC_EXECUTION_QUANTA_FENCE_DAA));
    assert_eq!(MAINNET_PARAMS.palw_execution_quanta, None);
}

/// **ADR-0133 S3 then S2 ride later unused heights**, so live licensing stays S1 coverage until
/// the operator crosses them. Mainnet stays unset.
#[test]
fn the_verification_s3_and_s2_fences_are_past_s1_and_the_lock_ledger() {
    use kaspa_consensus_core::config::params::{
        ForkActivation, PALW_RC_VERIFICATION_S2_FENCE_DAA, PALW_RC_VERIFICATION_S3_FENCE_DAA, PALW_RC_VERIFICATION_V2_FENCE_DAA,
    };
    let rc = palw_rc_shipped_params();
    assert_eq!(PALW_RC_VERIFICATION_V2_FENCE_DAA, 7_200);
    assert_eq!(PALW_RC_VERIFICATION_S3_FENCE_DAA, 8_600);
    assert_eq!(PALW_RC_VERIFICATION_S2_FENCE_DAA, 8_700);
    assert!(PALW_RC_VERIFICATION_S3_FENCE_DAA > PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA);
    assert!(PALW_RC_VERIFICATION_S2_FENCE_DAA > PALW_RC_VERIFICATION_S3_FENCE_DAA);
    assert_eq!(rc.palw_verification_v2, Some(ForkActivation::new(PALW_RC_VERIFICATION_V2_FENCE_DAA)));
    assert_eq!(rc.palw_verification_s3, Some(ForkActivation::new(PALW_RC_VERIFICATION_S3_FENCE_DAA)));
    assert_eq!(rc.palw_verification_s2, Some(ForkActivation::new(PALW_RC_VERIFICATION_S2_FENCE_DAA)));
    assert!(!rc.palw_verification_s3_at(PALW_RC_VERIFICATION_S3_FENCE_DAA - 1));
    assert!(rc.palw_verification_s3_at(PALW_RC_VERIFICATION_S3_FENCE_DAA));
    assert!(!rc.palw_verification_s2_at(PALW_RC_VERIFICATION_S2_FENCE_DAA - 1));
    assert!(rc.palw_verification_s2_at(PALW_RC_VERIFICATION_S2_FENCE_DAA));
    assert_eq!(MAINNET_PARAMS.palw_verification_s3, None);
    assert_eq!(MAINNET_PARAMS.palw_verification_s2, None);
    assert_eq!(rc.palw_kimi_k3, None, "Kimi stays fenced; fingerprint is Some-only");
    assert_eq!(MAINNET_PARAMS.palw_kimi_k3, None);
}
