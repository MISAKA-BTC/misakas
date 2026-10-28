//! **`params`'s `palw_model_registry_fence_tests` module**, moved out of `params.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).
#![cfg(test)]

use super::*;

/// **ADR-0135: dormant on every shipped preset; arming it moves the identity and the schedule.**
#[test]
fn adr0135_the_registry_fence_is_dormant_everywhere_and_arms_by_height() {
    for (name, preset) in [("devnet", devnet_shipped_params()), ("mainnet", MAINNET_PARAMS.clone())] {
        assert!(preset.palw_model_registry.is_none(), "{name}: the registry is not scheduled");
        assert!(!preset.palw_model_registry_at(u64::MAX - 1), "{name}: never active while dormant");
    }
    // testnet-11 arms it on the 6,001 flag day (the operator's decision after the devnet drill).
    let shipped = palw_rc_shipped_params();
    assert_eq!(shipped.palw_model_registry, Some(ForkActivation::new(PALW_RC_PALW_UPGRADE_FENCE_DAA)));
    assert!(
        !shipped.palw_model_registry_at(PALW_RC_PALW_UPGRADE_FENCE_DAA - 1)
            && shipped.palw_model_registry_at(PALW_RC_PALW_UPGRADE_FENCE_DAA)
    );
    let mut rc = shipped.clone();
    rc.palw_model_registry = None;
    rc.palw_verification_v2 = None;
    rc.palw_verification_s3 = None;
    rc.palw_verification_s2 = None;
    rc.palw_readiness_v2 = None;
    rc.palw_economic_payout = None;
    rc.palw_work_target = None;
    rc.set_palw_single_lottery(None);
    assert!(rc.palw_fences_v1().iter().any(|(name, fence)| *name == "palw_model_registry" && fence.is_none()));
    let mut never = rc.clone();
    never.palw_model_registry = Some(ForkActivation::never());
    assert!(!never.palw_model_registry_at(u64::MAX - 1), "never is never active");
    assert_eq!(never.consensus_identity_id(), rc.consensus_identity_id(), "and a never-armed registry is one identity with dormant");

    // Armed above the flag day (the economy and the lane are in force by then): identity and
    // schedule move, and the fork-id gate names the height.
    let mut armed = rc.clone();
    let height = PALW_RC_PALW_UPGRADE_FENCE_DAA + 1_000;
    armed.palw_model_registry = Some(ForkActivation::new(height));
    armed.validate_palw_v2().expect("armed above the economy and the lane");
    assert_ne!(armed.consensus_params_id(), rc.consensus_params_id(), "arming moves the identity");
    assert!(armed.fence_schedule_v1().contains(&height), "and the schedule names the height");
    assert!(!armed.palw_model_registry_at(height - 1) && armed.palw_model_registry_at(height));

    // Below the economy's height the registry has nothing to read: refused.
    let mut early = rc.clone();
    early.palw_model_registry = Some(ForkActivation::new(PALW_RC_PALW_UPGRADE_FENCE_DAA - 1));
    let refusal = early.validate_palw_v2().expect_err("the registry cannot precede the panel economy");
    assert!(format!("{refusal:?}").contains("palw_model_registry"), "{refusal:?}");
}

/// **ADR-0132 S: the single lottery is dormant on mainnet and devnet; on testnet-11 it is
/// scheduled at [`PALW_RC_ANCHOR_CLOCK_FENCE_DAA`] with the anchor clock and the cursor.**
/// Armed at or above the work target it moves the identity and the schedule; without the work
/// target at or below it is refused — with the network draw gone, `W` is the only rule left
/// holding the cadence.
#[test]
fn adr0132_the_single_lottery_fence_is_dormant_everywhere_arms_by_height_and_needs_the_work_target() {
    use crate::palw_economic_payout_v1::PALW_ECONOMIC_PAYOUT_DEVNET_V1;
    for (name, preset) in [("devnet", devnet_shipped_params()), ("mainnet", MAINNET_PARAMS.clone())] {
        assert!(preset.palw_single_lottery.is_none(), "{name}: the single lottery is not scheduled");
        assert!(!preset.palw_single_lottery_at(u64::MAX - 1), "{name}: never active while dormant");
    }
    // **testnet-11 arms it at [`PALW_RC_ANCHOR_CLOCK_FENCE_DAA`]**, with `palw_anchor_clock` and
    // ADR-0142's clock cursor, because past the anchor clock an attempt block postponed the
    // next heartbeat and the drill froze at its own flag day. It is not on 7,101: that height
    // already committed vanilla `daaScore` values.
    let shipped = palw_rc_shipped_params();
    assert_eq!(shipped.palw_single_lottery, Some(ForkActivation::new(PALW_RC_ANCHOR_CLOCK_FENCE_DAA)));
    assert_eq!(shipped.palw_anchor_clock, Some(ForkActivation::new(PALW_RC_ANCHOR_CLOCK_FENCE_DAA)));
    assert_eq!(shipped.palw_clock_cursor, Some(ForkActivation::new(PALW_RC_ANCHOR_CLOCK_FENCE_DAA)));
    let mut rc = shipped.clone();
    rc.set_palw_single_lottery(None);
    // Verification V2 (6,100) follows the registry; this test moves the registry past it.
    rc.palw_verification_v2 = None;
    rc.palw_verification_s3 = None;
    rc.palw_verification_s2 = None;
    rc.palw_readiness_v2 = None;
    assert!(rc.palw_fences_v1().iter().any(|(name, fence)| *name == "palw_single_lottery" && fence.is_none()));
    let mut never = rc.clone();
    never.set_palw_single_lottery(Some(ForkActivation::never()));
    assert!(!never.palw_single_lottery_at(u64::MAX - 1), "never is never active");
    assert_eq!(never.consensus_identity_id(), rc.consensus_identity_id(), "a never-armed single lottery is one identity with dormant");

    let height = PALW_RC_PALW_UPGRADE_FENCE_DAA + 1_000;
    let mut armed = rc.clone();
    armed.palw_model_registry = Some(ForkActivation::new(height));
    armed.palw_economic_payout =
        Some(PalwEconomicPayoutV1 { activation: ForkActivation::new(height), ..PALW_ECONOMIC_PAYOUT_DEVNET_V1 });
    armed.palw_work_target = Some(ForkActivation::new(height));
    // ADR-0142: the single lottery arms the anchor clock with it, and the anchor clock may not
    // be armed without the clock cursor at or below it.
    armed.palw_clock_cursor = Some(ForkActivation::new(height));
    armed.set_palw_single_lottery(Some(ForkActivation::new(height + 100)));
    armed.validate_palw_v2().expect("armed above the work target, with the cursor below it");
    assert_ne!(armed.consensus_params_id(), rc.consensus_params_id(), "arming moves the identity");
    assert!(armed.fence_schedule_v1().contains(&(height + 100)), "and the schedule names the height");
    assert!(!armed.palw_single_lottery_at(height + 99) && armed.palw_single_lottery_at(height + 100));
    let mut same = armed.clone();
    same.set_palw_single_lottery(Some(ForkActivation::new(height)));
    same.validate_palw_v2().expect("the work target's own height is at or below");

    // testnet-11 arms the work target on the 6,001 flag day (`wip/arm-6001-registry-payout`),
    // so the base that lacks it is built here rather than read from the preset.
    let mut alone = rc.clone();
    alone.palw_work_target = None;
    alone.set_palw_single_lottery(None);
    alone.set_palw_single_lottery(Some(ForkActivation::new(height)));
    let refusal = alone.validate_palw_v2().expect_err("the single lottery cannot precede the work target");
    assert!(format!("{refusal:?}").contains("palw_single_lottery"), "{refusal:?}");
    let mut early = armed.clone();
    early.set_palw_single_lottery(Some(ForkActivation::new(height - 1)));
    assert!(early.validate_palw_v2().is_err(), "below the work target it is refused");
}

/// **ADR-0137: the work target is dormant on every shipped preset; armed beside the registry
/// and the payout it moves the identity and the schedule; alone, or above only one of them, it
/// is refused; `never` is dormant.**
/// **The operator's rule (2026-09-18): DAA 6,001 is the one PALW upgrade flag day.** Every
/// consensus change of the upgrade — the model registry, the economic payout, the work target
/// (and with it the rooted W), the single lottery, the short challenge window, and the 6,001
/// fences that were already scheduled — fires at 6,001 together; DAA 6,000 is only the
/// compatibility boundary (the held regime and the audit's deep fixes, which the deployed
/// release scheduled at 7,000), so the new binary runs on the live chain before either. The
/// table below is exhaustive: a fence the preset names that is not in it fails the test, so a
/// new fence has to say which day it belongs to.
#[test]
fn t11_daa_6000_is_the_compatibility_boundary_and_6001_the_one_palw_upgrade_flag_day() {
    use crate::palw_mode_v2::PalwConsensusMode;
    use crate::palw_state_v2::PALW_SHORT_CHALLENGE_WINDOW_DAA_V1;
    use std::collections::{BTreeMap, BTreeSet};
    let rc = palw_rc_shipped_params();
    rc.validate_palw_v2().expect("the shipped preset validates");
    let by_name: BTreeMap<&str, Option<u64>> =
        rc.palw_fences_v1().into_iter().map(|(name, fence)| (name, fence.map(|f| f.daa_score()))).collect();

    const UPGRADE_6001: &[&str] = &[
        "palw_model_registry",
        "palw_economic_payout",
        "palw_work_target",
        "palw_short_challenge_window",
        "palw_panel_economy",
        "palw_work_priced_reward",
        "palw_execution_lane",
        "palw_overlay_carve",
        "dns_bft_gate",
    ];
    const COMPATIBILITY_6000: &[&str] = &["palw_held_context", "palw_shard_court", "palw_fp_da_pins", "palw_audit_2026_09_11_deep"];
    // The heights come from the constants, not from literals: the flag day has moved twice
    // (7,000 → 6,000 on 2026-09-17, 6,000 → 6,300 on 2026-09-18 when the tip reached it while
    // the release was held), and a test that spells the number out has to be edited each time
    // — which is the edit most likely to be made without re-reading what it pins.
    let later: &[(&str, u64)] = &[
        ("palw_verification_v2", PALW_RC_VERIFICATION_V2_FENCE_DAA),
        ("palw_readiness_v2", PALW_RC_VERIFICATION_V2_FENCE_DAA),
        ("palw_execution_lane_span_short", PALW_RC_EXECUTION_SPAN_SHORT_FENCE_DAA),
        ("palw_compute_overlay_retired", PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA),
        ("palw_model_seed_v2", PALW_RC_MODEL_SEED_V2_FENCE_DAA),
        ("palw_single_lottery", PALW_RC_ANCHOR_CLOCK_FENCE_DAA),
        ("palw_anchor_clock", PALW_RC_ANCHOR_CLOCK_FENCE_DAA),
        ("palw_clock_cursor", PALW_RC_ANCHOR_CLOCK_FENCE_DAA),
        ("palw_execution_quanta", PALW_RC_EXECUTION_QUANTA_FENCE_DAA),
        ("palw_seat_gate_possession", PALW_RC_SEAT_GATE_POSSESSION_FENCE_DAA),
        ("palw_class_receipt_window", PALW_RC_CLASS_RECEIPT_WINDOW_FENCE_DAA),
        ("palw_objective_offence", PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA),
        ("palw_verification_s3", PALW_RC_VERIFICATION_S3_FENCE_DAA),
        ("palw_verification_s2", PALW_RC_VERIFICATION_S2_FENCE_DAA),
    ];
    for name in UPGRADE_6001 {
        assert_eq!(by_name.get(name).copied().flatten(), Some(PALW_RC_PALW_UPGRADE_FENCE_DAA), "{name}: on the flag day");
    }
    for name in COMPATIBILITY_6000 {
        assert_eq!(by_name.get(name).copied().flatten(), Some(PALW_RC_AUDIT_DEEP_FENCE_DAA), "{name}: at the compatibility boundary");
    }
    for (name, height) in later {
        assert_eq!(by_name.get(name).copied().flatten(), Some(*height), "{name}: its own later day");
    }
    // Exhaustive: everything else the preset names fires below the boundary or never.
    for (name, height) in &by_name {
        let classified = UPGRADE_6001.contains(name) || COMPATIBILITY_6000.contains(name) || later.iter().any(|(n, _)| n == name);
        if !classified {
            assert!(
                height.is_none_or(|h| h < PALW_RC_AUDIT_DEEP_FENCE_DAA),
                "{name} fires at {height:?}: a fence at or past the boundary has to be classified in this table"
            );
        }
    }

    // The four points: 6000-1 / 6000 / 6001 / 6001+1.
    let active_at = |daa: u64| -> BTreeSet<&str> {
        rc.palw_fences_v1().into_iter().filter(|(_, f)| f.is_some_and(|f| f.is_active(daa))).map(|(n, _)| n).collect()
    };
    let (boundary, day) = (PALW_RC_AUDIT_DEEP_FENCE_DAA, PALW_RC_PALW_UPGRADE_FENCE_DAA);
    let (at_5999, at_6000, at_6001, at_6002) = (active_at(boundary - 1), active_at(boundary), active_at(day), active_at(day + 1));
    assert_eq!(
        at_6000.difference(&at_5999).copied().collect::<BTreeSet<_>>(),
        COMPATIBILITY_6000.iter().copied().collect::<BTreeSet<_>>(),
        "6,000 fires the compatibility boundary and nothing of the upgrade"
    );
    assert_eq!(
        at_6001.difference(&at_6000).copied().collect::<BTreeSet<_>>(),
        UPGRADE_6001.iter().copied().collect::<BTreeSet<_>>(),
        "6,001 fires the whole upgrade at once"
    );
    // ADR-0143's fence stands unarmed (the 2026-09-19 audit), so nothing fires between the
    // upgrade day and ADR-0133's.
    assert_eq!(at_6002, at_6001, "and nothing else fires until ADR-0133's day");
    assert_eq!(
        active_at(PALW_RC_VERIFICATION_V2_FENCE_DAA).difference(&at_6001).copied().collect::<Vec<_>>(),
        vec!["palw_readiness_v2", "palw_verification_v2"],
        "6,100 is ADR-0133's own day: segmented licensing and the whole-artifact possession proof together"
    );
    assert!(
        !rc.palw_verification_v2_at(PALW_RC_VERIFICATION_V2_FENCE_DAA - 1)
            && rc.palw_verification_v2_at(PALW_RC_VERIFICATION_V2_FENCE_DAA)
    );
    assert!(
        !rc.palw_readiness_v2_at(PALW_RC_VERIFICATION_V2_FENCE_DAA - 1) && rc.palw_readiness_v2_at(PALW_RC_VERIFICATION_V2_FENCE_DAA)
    );
    for name in UPGRADE_6001 {
        assert!(!at_6000.contains(name), "{name} is not active at 6,000");
    }

    // The accessors the code reads agree with the table.
    for daa in [boundary - 1, boundary] {
        assert!(!rc.palw_model_registry_at(daa) && rc.palw_economic_payout_at(daa).is_none(), "registry/payout off at {daa}");
        assert!(!rc.palw_work_target_at(daa) && !rc.palw_single_lottery_at(daa), "work target/single lottery off at {daa}");
        assert!(!rc.palw_short_challenge_window_at(daa), "short window off at {daa}");
    }
    for daa in [day, day + 1] {
        assert!(rc.palw_model_registry_at(daa) && rc.palw_economic_payout_at(daa).is_some(), "registry/payout on at {daa}");
        assert!(rc.palw_work_target_at(daa), "work target on at {daa}");
        assert!(!rc.palw_single_lottery_at(daa), "the single lottery waits for DAA {PALW_RC_ANCHOR_CLOCK_FENCE_DAA}, at {daa} too");
        assert!(rc.palw_short_challenge_window_at(daa), "short window on at {daa}");
    }
    for daa in [PALW_RC_ANCHOR_CLOCK_FENCE_DAA, PALW_RC_ANCHOR_CLOCK_FENCE_DAA + 1] {
        assert!(rc.palw_single_lottery_at(daa) && rc.palw_anchor_clock_at(daa), "clock/lottery on at {daa}");
    }
    // The bundle's copy of the short window follows the fence, not a constant.
    let PalwConsensusMode::ConsensusV2(bundle) = &rc.palw_consensus_mode else { panic!("testnet-11 runs V2") };
    assert_eq!(bundle.state.window_challenge_at(boundary), bundle.state.window_challenge());
    assert_eq!(bundle.state.window_challenge_at(day), PALW_SHORT_CHALLENGE_WINDOW_DAA_V1);
    // The schedule the fork id advertises: 6,000 and 6,001 as neighbours, then 6,201.
    let schedule = rc.fence_schedule_v1();
    assert!(schedule.windows(2).any(|w| w == [boundary, day]), "{schedule:?}");
    assert!(
        schedule.contains(&PALW_RC_VERIFICATION_V2_FENCE_DAA)
            && schedule.contains(&PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA)
            && schedule.contains(&PALW_RC_ANCHOR_CLOCK_FENCE_DAA)
            && schedule.contains(&PALW_RC_EXECUTION_QUANTA_FENCE_DAA)
            && schedule.contains(&PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA)
            && !schedule.contains(&7_000),
        "{schedule:?}: 7,000 is nobody's height any more"
    );
    // No part of the bundle may fire without the rest: a preset with one member moved is refused
    // or, where the order rules allow it, visibly a different identity.
    // The single lottery is no longer on this day (ADR-0142), so the member dropped here is one
    // that still is; the point of the assertion is unchanged.
    let mut partial = rc.clone();
    partial.palw_work_target = None;
    assert_ne!(partial.consensus_params_id(), rc.consensus_params_id(), "dropping one member moves the fingerprint");
    let mut early = rc.clone();
    early.palw_clock_cursor = Some(ForkActivation::new(boundary));
    early.set_palw_single_lottery(Some(ForkActivation::new(boundary)));
    assert!(early.validate_palw_v2().is_err(), "the single lottery cannot precede the work target");
    // ...and with the work target below it, it is still refused without ADR-0142's cursor.
    let mut no_cursor = rc.clone();
    no_cursor.set_palw_single_lottery(Some(ForkActivation::new(day)));
    assert!(no_cursor.validate_palw_v2().is_err(), "the anchor clock cannot be armed without the clock cursor");
    let mut early = rc.clone();
    early.palw_work_target = Some(ForkActivation::new(6_000));
    assert!(early.validate_palw_v2().is_err(), "the work target cannot precede the registry and the payout");
    let mut drifted = rc.clone();
    drifted.palw_short_challenge_window = Some(ForkActivation::new(6_002));
    assert!(drifted.validate_palw_v2().is_err(), "the fence and the bundle's copy may not disagree");
}

#[test]
fn adr0137_the_work_target_fence_is_dormant_everywhere_arms_by_height_and_is_refused_alone() {
    use crate::palw_economic_payout_v1::PALW_ECONOMIC_PAYOUT_DEVNET_V1;
    for (name, preset) in [("devnet", devnet_shipped_params()), ("mainnet", MAINNET_PARAMS.clone())] {
        assert!(preset.palw_work_target.is_none(), "{name}: the work target is not scheduled");
        assert!(!preset.palw_work_target_at(u64::MAX - 1), "{name}: never active while dormant");
    }
    // testnet-11 arms it beside the registry and the payout on the 6,001 flag day: ADR-0137's
    // fifth finding forbids a gap between the share rule and the rule that stops reading shares.
    let shipped = palw_rc_shipped_params();
    assert_eq!(shipped.palw_work_target, Some(ForkActivation::new(PALW_RC_PALW_UPGRADE_FENCE_DAA)));
    assert!(
        !shipped.palw_work_target_at(PALW_RC_PALW_UPGRADE_FENCE_DAA - 1)
            && shipped.palw_work_target_at(PALW_RC_PALW_UPGRADE_FENCE_DAA)
    );
    shipped.validate_palw_v2().expect("armed beside the registry and the payout it ships with");
    let mut rc = shipped.clone();
    rc.palw_work_target = None;
    rc.set_palw_single_lottery(None);
    assert!(rc.palw_fences_v1().iter().any(|(name, fence)| *name == "palw_work_target" && fence.is_none()));
    // The counterfactuals below arm it above the flag day, so the base carries neither the
    // registry nor the payout: with them at 6,001 every height above is "at or below" and the
    // refusals this test is about could not fire.
    rc.palw_model_registry = None;
    rc.palw_verification_v2 = None;
    rc.palw_verification_s3 = None;
    rc.palw_verification_s2 = None;
    rc.palw_readiness_v2 = None;
    rc.palw_economic_payout = None;
    let mut never = rc.clone();
    never.palw_work_target = Some(ForkActivation::never());
    assert!(!never.palw_work_target_at(u64::MAX - 1), "never is never active");
    assert_eq!(never.consensus_identity_id(), rc.consensus_identity_id(), "a never-armed work target is one identity with dormant");

    let height = PALW_RC_PALW_UPGRADE_FENCE_DAA + 1_000;
    let mut armed = rc.clone();
    armed.palw_model_registry = Some(ForkActivation::new(height));
    armed.palw_economic_payout =
        Some(PalwEconomicPayoutV1 { activation: ForkActivation::new(height), ..PALW_ECONOMIC_PAYOUT_DEVNET_V1 });
    armed.palw_work_target = Some(ForkActivation::new(height + 10));
    armed.validate_palw_v2().expect("armed above the registry and the payout");
    assert_ne!(armed.consensus_params_id(), rc.consensus_params_id(), "arming moves the identity");
    assert!(armed.fence_schedule_v1().contains(&(height + 10)), "and the schedule names the height");
    assert!(!armed.palw_work_target_at(height + 9) && armed.palw_work_target_at(height + 10));
    let mut same_height = armed.clone();
    same_height.palw_work_target = Some(ForkActivation::new(height));
    same_height.validate_palw_v2().expect("the same height as the registry and the payout is at or below");

    let mut alone = rc.clone();
    alone.palw_work_target = Some(ForkActivation::new(height));
    let refusal = alone.validate_palw_v2().expect_err("the work target cannot precede the registry and the payout");
    assert!(format!("{refusal:?}").contains("palw_work_target"), "{refusal:?}");
    let mut without_payout = rc.clone();
    without_payout.palw_model_registry = Some(ForkActivation::new(height));
    without_payout.palw_work_target = Some(ForkActivation::new(height));
    assert!(without_payout.validate_palw_v2().is_err(), "the registry alone is not enough: W₀ needs a rate");
    let mut early = armed.clone();
    early.palw_work_target = Some(ForkActivation::new(height - 1));
    assert!(early.validate_palw_v2().is_err(), "below the registry and the payout it is refused");
}

/// **ADR-0132 Upgrade C: dormant on every shipped preset; armed beside the registry it moves the
/// identity and the schedule and its numbers reach the fingerprint; alone, or with a zero rate
/// or an inverted share, it is refused.**
#[test]
fn adr0132_the_economic_payout_fence_is_dormant_everywhere_arms_by_height_and_is_refused_alone() {
    use crate::palw_economic_payout_v1::PALW_ECONOMIC_PAYOUT_DEVNET_V1;
    for (name, preset) in [("devnet", devnet_shipped_params()), ("mainnet", MAINNET_PARAMS.clone())] {
        assert!(preset.palw_economic_payout.is_none(), "{name}: the payout is not scheduled");
        assert!(preset.palw_economic_payout_at(u64::MAX - 1).is_none(), "{name}: never active while dormant");
    }
    // testnet-11 arms it beside the registry on the 6,001 flag day, at the calibrated numbers.
    let shipped = palw_rc_shipped_params();
    let armed_t11 = shipped.palw_economic_payout.expect("testnet-11 schedules the payout");
    assert_eq!(
        (armed_t11.activation, armed_t11.rate_sompi_per_giga, armed_t11.panel_share_alpha_permille),
        (ForkActivation::new(PALW_RC_PALW_UPGRADE_FENCE_DAA), 900_000_000, 100)
    );
    assert_eq!(
        (armed_t11.panel_share_min_permille, armed_t11.panel_share_max_permille, armed_t11.cap_utilization_max_permille),
        (100, 300, 800)
    );
    shipped.validate_palw_v2().expect("the shipped card validates with both armed");
    let mut rc = shipped.clone();
    rc.palw_model_registry = None;
    rc.palw_verification_v2 = None;
    rc.palw_verification_s3 = None;
    rc.palw_verification_s2 = None;
    rc.palw_readiness_v2 = None;
    rc.palw_economic_payout = None;
    rc.palw_work_target = None;
    rc.set_palw_single_lottery(None);
    assert!(rc.palw_fences_v1().iter().any(|(name, fence)| *name == "palw_economic_payout" && fence.is_none()));
    let mut never = rc.clone();
    never.palw_economic_payout = Some(PalwEconomicPayoutV1 { activation: ForkActivation::never(), ..PALW_ECONOMIC_PAYOUT_DEVNET_V1 });
    assert!(never.palw_economic_payout_at(u64::MAX - 1).is_none(), "never is never active");
    assert_eq!(never.consensus_identity_id(), rc.consensus_identity_id(), "a never-armed payout is one identity with dormant");

    let height = PALW_RC_PALW_UPGRADE_FENCE_DAA + 1_000;
    let mut armed = rc.clone();
    armed.palw_model_registry = Some(ForkActivation::new(height));
    armed.palw_economic_payout =
        Some(PalwEconomicPayoutV1 { activation: ForkActivation::new(height), ..PALW_ECONOMIC_PAYOUT_DEVNET_V1 });
    armed.validate_palw_v2().expect("armed beside the registry, above the economy");
    assert_ne!(armed.consensus_params_id(), rc.consensus_params_id(), "arming moves the identity");
    assert!(armed.fence_schedule_v1().contains(&height), "and the schedule names the height");
    assert!(armed.palw_economic_payout_at(height - 1).is_none() && armed.palw_economic_payout_at(height).is_some());
    let mut dearer = armed.clone();
    dearer.palw_economic_payout.as_mut().unwrap().rate_sompi_per_giga += 1;
    assert_ne!(dearer.consensus_params_id(), armed.consensus_params_id(), "the rate is in the fingerprint");

    let mut alone = rc.clone();
    alone.palw_economic_payout = armed.palw_economic_payout;
    let refusal = alone.validate_palw_v2().expect_err("the payout cannot precede the registry whose work it prices");
    assert!(format!("{refusal:?}").contains("palw_economic_payout"), "{refusal:?}");
    let mut zero = armed.clone();
    zero.palw_economic_payout.as_mut().unwrap().rate_sompi_per_giga = 0;
    assert!(format!("{:?}", zero.validate_palw_v2().expect_err("a zero rate pays nothing")).contains("zero rate"));
    let mut inverted = armed.clone();
    inverted.palw_economic_payout.as_mut().unwrap().panel_share_min_permille = 400;
    assert!(inverted.validate_palw_v2().is_err(), "a floor above the ceiling is refused");
}
