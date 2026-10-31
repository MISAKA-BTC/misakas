//! **`params`'s `consensus_params_id_tests` module**, moved out of `params.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;

/// **The bond economics of finality, pinned (2026-08-30).**
///
/// Priced at the rate the token actually trades at, the quorum-corruption cost is
/// `ceil(2n/3) x min_bond` — a PRODUCT, and the count is the cheap half. These three
/// constants were re-derived together and mean nothing apart, so they are asserted together:
/// changing one without the others is the mistake this test exists to catch.
/// **The 2026-09-23 audit fence is armed at genesis or refused** (2026-09-24 DoS audit review):
/// testnet-12 arms it at DAA 0 and validates; the same card with the fence moved to any later
/// height is refused by name, because the anchor ring the fence's rules read would start empty
/// under a counter pre-fence `Final`s had already moved (the D1 floor then answers `Some(0)`).
///
/// Fails without the check: the moved card validates.
#[test]
fn the_2026_09_23_audit_fence_is_armed_at_genesis_or_not_at_all() {
    let t12 = Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
    assert_eq!(t12.palw_audit_2026_09_23.map(|f| f.daa_score()), Some(0), "testnet-12 arms it from genesis");
    t12.validate_palw_v2().expect("testnet-12 validates");
    let mut late = t12.clone();
    late.palw_audit_2026_09_23 = Some(ForkActivation::new(1_000));
    let refused = late.validate_palw_v2();
    assert!(
        matches!(refused, Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(why)) if why.contains("only be armed at genesis")),
        "{refused:?}"
    );
}

/// **ADR-0152 v2 F2's attribution fence is armed at genesis, on the four fences it convicts
/// through, or refused** — testnet-12 alone arms it, at DAA 0, and validates; the card with the
/// fence at a later height, or with any of the four taken away, is refused by name; `never()`
/// collapses to `None` so it cannot write bytes a build without the field never writes.
#[test]
fn the_offence_attribution_fence_is_genesis_only_and_rides_its_four_fences() {
    let t12 = Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
    assert_eq!(t12.palw_offence_attribution, Some(ForkActivation::always()), "testnet-12 arms it from genesis");
    assert!(t12.palw_offence_attribution_active_at(0));
    t12.validate_palw_v2().expect("testnet-12 validates with the fence");
    for (name, p) in
        [("mainnet", mainnet_shipped_params()), ("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())]
    {
        assert_eq!(p.palw_offence_attribution, None, "{name}: dormant");
        assert!(!p.palw_offence_attribution_active_at(u64::MAX), "{name}: never in force");
    }
    let refused_with = |edit: &dyn Fn(&mut Params), needle: &str| {
        let mut p = t12.clone();
        palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_G14_17000_FENCES_V1, None);
        // int-12: the RFC-0007 fences ride the 5,300 list and need `palw_objective_offence`; take them away first, so the
        // edit below meets the attribution fence's own refusal (the release's list is not what this test asks about).
        p.palw_capped_onboarding_v1 = None;
        p.sync_palw_capped_onboarding_v1();
        p.palw_audit_mesh_v1 = None;
        p.sync_palw_audit_mesh_v1();
        p.palw_witness_manifest_v1 = None;
        p.sync_palw_witness_manifest_v1();
        p.palw_verification_vertex_v1 = None;
        p.sync_palw_verification_vertex_v1();
        edit(&mut p);
        let refused = p.validate_palw_v2();
        assert!(matches!(&refused, Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(why)) if why.contains(needle)), "{refused:?}");
    };
    refused_with(
        &|p| p.palw_offence_attribution = Some(ForkActivation::new(1)),
        "palw_offence_attribution may only be armed at genesis",
    );
    refused_with(&|p| p.palw_objective_offence = None, "palw_offence_attribution is armed without");
    refused_with(&|p| p.palw_verification_v2 = Some(ForkActivation::new(5)), "palw_offence_attribution is armed without");
    refused_with(&|p| p.palw_economic_safety = None, "palw_offence_attribution is armed without");
    // ADR-0152 v3.1 F1: the prefill draw at genesis, which J5 derives every attempt context at.
    refused_with(&|p| p.palw_prefill_draw = None, "palw_offence_attribution is armed without palw_prefill_draw at genesis");
    refused_with(
        &|p| p.palw_prefill_draw = Some(ForkActivation::new(7)),
        "palw_offence_attribution is armed without palw_prefill_draw at genesis",
    );
    let mut without = t12.clone();
    without.palw_offence_attribution = None;
    let mut never_armed = t12.clone();
    never_armed.palw_offence_attribution = Some(ForkActivation::never());
    assert_eq!(
        never_armed.consensus_identity_id(),
        without.consensus_identity_id(),
        "Some(never()) is absence, or the collapse in normalize_values_a_scheduled_fence_drags_with_it is gone"
    );
    let mut scheduled = without.clone();
    scheduled.palw_offence_attribution = Some(ForkActivation::new(1_000));
    assert_eq!(scheduled.consensus_identity_id(), without.consensus_identity_id(), "a future height is not yet a rule");
    assert_ne!(scheduled.consensus_schedule_id(), without.consensus_schedule_id(), "but the schedule names it");
    assert_ne!(t12.consensus_identity_id(), without.consensus_identity_id(), "in force from block one separates identities");
    assert_ne!(t12.consensus_params_id(), without.consensus_params_id(), "testnet-12's ruleset names the fence");
    assert_ne!(t12.consensus_schedule_id(), without.consensus_schedule_id(), "and so does its schedule");
}

/// testnet-12's pruning depth (ADR-0152 §4-quater P-1, U-D3): the D_cap claim lattice, 74,920,
/// which the finality-offset rounding leaves as it is. testnet-11's stays 12,002.
const T12_PRUNING_DEPTH: u64 = 74_920;

/// **T-D7 (the fence part): ADR-0152 §4-quater's class-verify-deadline fence is armed at genesis,
/// over §11.3's receipt window and the derived free-prompt work, or refused** — testnet-12 alone
/// arms it, at DAA 0, with empty rows, mirrored into its bundle, and validates; every other preset
/// leaves it and its rows unset. Refused by name: a later height, §11.3 absent or later, the
/// derived work absent, rows without the fence, a row that prices nothing or overshoots D_cap, a
/// class named twice, a row before `palw_offence_attribution`, and a bundle whose mirrors disagree.
/// `never()` collapses to `None`; a future height moves only the schedule; the fence in force
/// moves testnet-12's identity, params and schedule ids; a non-empty row set moves the params id.
/// Independent of `palw_offence_attribution`: the fence validates with that fence taken away.
#[test]
fn the_class_verify_deadline_fence_is_genesis_only_over_its_prerequisites() {
    use crate::palw_class_verify_deadline_v1::PalwClassVerifyRowV1;
    const ROW0: PalwClassVerifyRowV1 = PalwClassVerifyRowV1 {
        class_id: crate::Hash64::from_bytes([7u8; 64]),
        activation_daa: 0,
        a_r_ps: 1_000_000_000_000,
        b_r_ps: 0,
        t_fixed_ms: 60_000,
        canonical_positions: 1_000,
        leaves_per_position: 10,
    };
    static ROW: [PalwClassVerifyRowV1; 1] = [ROW0];
    static TWICE: [PalwClassVerifyRowV1; 2] = [ROW0, ROW0];
    static FREE: [PalwClassVerifyRowV1; 1] = [PalwClassVerifyRowV1 { a_r_ps: 0, ..ROW0 }];
    static OVER: [PalwClassVerifyRowV1; 1] = [PalwClassVerifyRowV1 { a_r_ps: 1_000_000_000_000_000, ..ROW0 }];
    let t12 = Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
    assert_eq!(t12.palw_class_verify_deadline, Some(ForkActivation::always()), "testnet-12 arms it from genesis");
    assert!(t12.palw_class_verify_rows.is_empty(), "no measured row at launch: the 2M row is closed (U-D1)");
    assert!(t12.palw_class_verify_deadline_active_at(0));
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { panic!("a V2 network") };
    assert_eq!(bundle.state.class_verify_deadline_from_daa(), Some(0), "the bundle mirrors the fence");
    assert!(bundle.state.class_verify_rows().is_empty());
    t12.validate_palw_v2().expect("testnet-12 validates with the fence");
    for (name, p) in
        [("mainnet", mainnet_shipped_params()), ("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())]
    {
        assert_eq!(p.palw_class_verify_deadline, None, "{name}: dormant");
        assert!(p.palw_class_verify_rows.is_empty(), "{name}: no rows");
        assert!(!p.palw_class_verify_deadline_active_at(u64::MAX), "{name}: never in force");
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &p.palw_consensus_mode {
            assert_eq!(b.state.class_verify_deadline_from_daa(), None, "{name}: no mirror");
        }
    }
    // Each refusal is asked of the fence's own validator, so no unrelated rule of the edited
    // preset answers first; `validate_palw_v2` runs it (the first case, asked through both).
    let refused_with = |edit: &dyn Fn(&mut Params), needle: &str| {
        let mut p = t12.clone();
        edit(&mut p);
        let refused = p.validate_palw_class_verify_deadline_v1();
        let why = match &refused {
            Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(why)) => why.to_string(),
            Err(crate::palw_mode_v2::PalwModeV2Error::InvalidOwned(why)) => why.clone(),
            other => panic!("{needle}: {other:?}"),
        };
        assert!(why.contains(needle), "{needle}: {why}");
    };
    let later = |p: &mut Params| {
        p.palw_class_verify_deadline = Some(ForkActivation::new(1));
        p.sync_palw_class_verify_deadline();
    };
    refused_with(&later, "palw_class_verify_deadline may only be armed at genesis");
    let mut via_v2 = t12.clone();
    later(&mut via_v2);
    assert!(
        matches!(via_v2.validate_palw_v2(), Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(why)) if why.contains("palw_class_verify_deadline may only be armed at genesis")),
        "validate_palw_v2 runs the fence's validator"
    );
    refused_with(
        &|p| p.set_palw_class_receipt_window(Some(ForkActivation::new(5))),
        "palw_class_verify_deadline is armed without palw_class_receipt_window",
    );
    refused_with(&|p| p.palw_fp_derived_work = None, "palw_class_verify_deadline is armed without palw_fp_derived_work");
    refused_with(
        &|p| p.set_palw_short_challenge_window(None),
        "palw_class_verify_deadline is armed without palw_short_challenge_window",
    );
    refused_with(
        &|p| {
            p.palw_class_verify_deadline = None;
            p.palw_class_verify_rows = &ROW;
            p.sync_palw_class_verify_deadline();
        },
        "palw_class_verify_rows is non-empty without palw_class_verify_deadline armed",
    );
    refused_with(
        &|p| {
            p.palw_class_verify_deadline = None;
            p.sync_palw_class_verify_deadline();
            if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &mut p.palw_consensus_mode {
                b.state = b.state.clone().with_class_verify_deadline(Some(0), Vec::new());
            }
        },
        "the V2 bundle carries class-verify-deadline mirrors without palw_class_verify_deadline armed",
    );
    refused_with(&|p| p.palw_class_verify_rows = &ROW, "disagree with the V2 bundle's mirrors");
    for (rows, needle) in [
        (&TWICE[..], "names one class twice"),
        (&FREE[..], "measures a replay that costs nothing"),
        (&OVER[..], "canonical deadline exceeds D_cap"),
    ] {
        refused_with(
            &|p| {
                p.palw_class_verify_rows = rows;
                p.sync_palw_class_verify_deadline();
            },
            needle,
        );
    }
    // P-1 / E-11 / T-D8b: testnet-12 carries the horizon its claims can reach. Its pruning depth is
    // the claim lattice past the fence — `2(600 + D_cap) + 1,200 + 3,000 + R_eff span` — laid out as
    // every V2 depth is (`palw_v2_pruning_depth_for_lattice_v1`); a measured row validates on
    // testnet-12's own params; and one DAA short of the lattice `validate_palw_v2` refuses the
    // ruleset (K18), row or no row, since the lattice past the fence is the D_cap one.
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(t12_bundle) = &t12.palw_consensus_mode else { unreachable!() };
    let reff = palw_class_verify_reff_span_daa_v1(&t12_bundle.state);
    assert_eq!(reff, 2 * (600 + 16_000) + 120 + 3_000 + 1_200, "R_eff's span at D_cap: 37,520");
    let lattice = palw_v2_claim_lattice_daa_v1(t12_bundle, t12.palw_da_court, t12.palw_class_verify_deadline);
    assert_eq!(lattice, 2 * (600 + 16_000) + 1_200 + 3_000 + reff, "the D_cap lattice (U-D3)");
    assert_eq!(lattice, 74_920);
    assert_eq!(
        palw_v2_claim_lattice_daa_v1(t12_bundle, t12.palw_da_court, None),
        2 * (600 + 600) + 1_200 + 3_000 + 5_400,
        "below the fence the lattice is the one it always was"
    );
    assert_eq!(
        t12.blockrate.pruning_depth,
        palw_v2_pruning_depth_for_lattice_v1(&t12.blockrate, t12_bundle, lattice),
        "testnet-12's depth IS the derivation"
    );
    assert_eq!(t12.blockrate.pruning_depth, T12_PRUNING_DEPTH, "testnet-12's pruning depth, pinned");
    let mut rowed = t12.clone();
    rowed.palw_class_verify_rows = &ROW;
    rowed.sync_palw_class_verify_deadline();
    rowed.validate_palw_v2().expect("a measured row validates on testnet-12's own horizon");
    for with_row in [false, true] {
        let mut short = if with_row { rowed.clone() } else { t12.clone() };
        short.blockrate.pruning_depth = lattice - 1;
        let refused = short.validate_palw_v2();
        assert!(
            matches!(&refused, Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(why)) if why.contains("the claim lattice outlives the pruning horizon")),
            "one DAA short of the D_cap lattice (row {with_row}): {refused:?}"
        );
        short.blockrate.pruning_depth = lattice;
        short.validate_palw_v2().expect("at the lattice exactly");
    }
    // A measured row moves the params id (not the schedule); before `palw_offence_attribution` it
    // is refused (U-D8).
    assert_ne!(rowed.consensus_params_id(), t12.consensus_params_id(), "a row is a rule: the params id names it");
    assert_eq!(rowed.consensus_schedule_id(), t12.consensus_schedule_id(), "a row is not a fence height");
    refused_with(
        &|p| {
            p.palw_class_verify_rows = &ROW;
            p.palw_offence_attribution = None;
            p.palw_rcore_plus = None;
            p.palw_rcore_conservative_classes = &[];
            p.sync_palw_rcore_plus();
            p.sync_palw_class_verify_deadline();
        },
        "activates before palw_offence_attribution",
    );
    // Independent of `palw_offence_attribution`: with no row, the fence stands without it.
    let mut unattributed = t12.clone();
    unattributed.palw_offence_attribution = None;
    unattributed.palw_rcore_plus = None;
    unattributed.palw_rcore_conservative_classes = &[];
    unattributed.sync_palw_rcore_plus();
    unattributed.validate_palw_class_verify_deadline_v1().expect("the fence does not ride the attribution fence");
    // The ids: `never()` is absence; a future height is a schedule entry only; in force from
    // genesis it separates testnet-12's identity, params and schedule.
    let mut without = t12.clone();
    without.palw_class_verify_deadline = None;
    without.sync_palw_class_verify_deadline();
    without.validate_palw_class_verify_deadline_v1().expect("the fence-off twin is consistent");
    let mut never_armed = t12.clone();
    never_armed.palw_class_verify_deadline = Some(ForkActivation::never());
    assert_eq!(
        never_armed.consensus_identity_id(),
        without.consensus_identity_id(),
        "Some(never()) is absence, or the collapse in normalize_values_a_scheduled_fence_drags_with_it is gone"
    );
    let mut scheduled = without.clone();
    scheduled.palw_class_verify_deadline = Some(ForkActivation::new(1_000));
    assert_eq!(scheduled.consensus_identity_id(), without.consensus_identity_id(), "a future height is not yet a rule");
    assert_ne!(scheduled.consensus_schedule_id(), without.consensus_schedule_id(), "but the schedule names it");
    assert_ne!(t12.consensus_identity_id(), without.consensus_identity_id(), "in force from block one separates identities");
    assert_ne!(t12.consensus_params_id(), without.consensus_params_id(), "testnet-12's ruleset names the fence");
    assert_ne!(t12.consensus_schedule_id(), without.consensus_schedule_id(), "and so does its schedule");
    assert!(
        t12.palw_fences_v1()
            .iter()
            .any(|(name, fence)| *name == "palw_class_verify_deadline" && *fence == Some(ForkActivation::always())),
        "registered in the fence list fork_id_v1 and the schedule id walk"
    );
}

/// **ADR-0152 C7 on testnet-12 is the 2M row, pinned and derived** (U1): the literal
/// `PALW_T12_RCORE_CONSERVATIVE_CLASSES` the const preset carries is the 2M dense row's class id,
/// one of testnet-12's genesis held rows, and not the 8k row's.
#[test]
fn the_t12_c7_list_is_the_2m_row() {
    let derived = palw_t12_rcore_conservative_classes_v1();
    assert_eq!(
        derived.as_slice(),
        PALW_T12_RCORE_CONSERVATIVE_CLASSES.as_slice(),
        "the pinned C7 list is the 2M row's derived class id: {:?}",
        derived.iter().map(|h| h.to_string()).collect::<Vec<_>>()
    );
    let held = palw_t12_genesis_held_class_ids_v1();
    assert_eq!(held.len(), PALW_T12_GENESIS_HELD_ROWS.len(), "every held row derives");
    assert_eq!(held[1], PALW_T12_RCORE_CONSERVATIVE_CLASSES[0], "the second held row is the 2M row");
    assert_ne!(held[0], PALW_T12_RCORE_CONSERVATIVE_CLASSES[0], "the 8k row is released per SR-1, not held");
}

#[test]
fn the_active_stake_gate_is_the_product_of_the_two_floors() {
    let dns = PRODUCTION_DNS_PARAMS;
    // 2026-09-25 (user decision, the 2026-09-24 mainnet assumption): 6 validators x 20M MSK.
    assert_eq!(
        dns.min_bond_amount_sompi,
        20_000_000 * SOMPI_PER_KASPA,
        "per-validator collateral — a DNS vote's own floor, apart from the PALW producer floor again"
    );
    assert_eq!(dns.min_active_validators, 6, "at least six validators, each bonded 20M");
    // The two gates are ANDed, so the stake gate must state their product or it is dead
    // weight that reads as a second, weaker floor.
    assert_eq!(
        dns.min_active_stake_sompi,
        dns.min_active_validators as u64 * dns.min_bond_amount_sompi,
        "the stake gate is the product; raise either floor and this must move with it"
    );
}

/// **Every class the shipped genesis funds is a class its own registry can judge**
/// (ADR-0071 Decision 3).
///
/// Not a consensus gate, deliberately: ADR-0061 retired the "a genesis must seat a panel"
/// refusal because an under-seated genesis is a transitional state — the heartbeat carries
/// blocks, bonds arrive as transactions, and licensing begins when the seats do. A class no
/// seat declares is the same shape of transitional, and refusing it at genesis would re-impose
/// the rule that ADR retired.
///
/// What is NOT transitional is shipping a card whose model tiers hold 97.8% of cadence and
/// whose registry declares none of them. That is a network which produces tier blocks whose
/// every claim voids at `BindTimeout`, forever, with the escrow burning each time — so it is
/// asserted here, over the real assembly, rather than left to be discovered on a live chain.
#[test]
fn the_shipped_genesis_can_seat_a_panel_for_every_class_it_registers() {
    use crate::palw_mode_v2::PalwConsensusMode;
    use crate::palw_state_v2::PalwConsensusObjectV2 as Obj;
    let params = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("the RC is a V2 network") };

    let classes: Vec<crate::Hash64> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            Obj::ClassRegistered { class_id, .. } => Some(*class_id),
            _ => None,
        })
        .collect();
    assert!(classes.len() >= 2, "the shipped card registers the floor and at least one tier");

    // `derive_panel_v2` seats one bond per operator and excludes the executor, so a class needs
    // `PALW_V2_PANEL_SEATS + 1` distinct operators that declare it.
    let needed = crate::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1();
    for class_id in classes {
        let operators: std::collections::BTreeSet<crate::Hash64> = bundle
            .genesis_objects
            .iter()
            .filter_map(|o| match o {
                Obj::BondRegistered { operator_pubkey, capable_classes, .. } if capable_classes.contains(&class_id) => {
                    Some(crate::palw_state_v2::palw_operator_id_v2(operator_pubkey))
                }
                _ => None,
            })
            .collect();
        assert!(
            operators.len() >= needed,
            "class {class_id} is funded but only {} distinct operators declare it — it needs {needed}, and short of that every \
                 one of its claims voids at BindTimeout with its escrow burned",
            operators.len()
        );
    }
}

/// **The 120 s cadence substitution is the identity on the network that already runs it.**
///
/// This is the property that lets every V2 assembly apply it unconditionally: testnet-11 is
/// live and its `consensus_params_id` is pinned, so a rescale that moved even one of its
/// windows would be a flag day wearing a mainnet fix's clothes.
#[test]
fn the_two_minute_cadence_is_a_fixed_point_of_the_live_testnet_table() {
    assert_eq!(TESTNET_DNS_PARAMS.at_two_minute_cadence(), TESTNET_DNS_PARAMS);
    let after = TESTNET_PARAMS.clone().with_two_minute_cadence();
    let (a, b) = (&after.blockrate, &TESTNET_PARAMS.blockrate);
    assert_eq!(
        (a.target_time_per_block, a.ghostdag_k, a.mergeset_size_limit, a.merge_depth, a.coinbase_maturity, a.max_block_parents),
        (b.target_time_per_block, b.ghostdag_k, b.mergeset_size_limit, b.merge_depth, b.coinbase_maturity, b.max_block_parents),
    );
    assert_eq!((a.finality_depth, a.pruning_depth), (b.finality_depth, b.pruning_depth));
    assert_eq!(after.dns_params, TESTNET_PARAMS.dns_params);
    // Idempotent, which is also the form the `validate_palw_v2` gate asserts.
    let once = PRODUCTION_DNS_PARAMS.at_two_minute_cadence();
    assert_eq!(once.clone().at_two_minute_cadence(), once);
}

/// **What the substitution buys a 10 bps base, stated as the harm it removes.**
///
/// `bond_spend_gate` enforces `unbonding_period_blocks` in consensus, so inheriting the
/// production count at 120 s is not a cosmetic unit error — it is confiscation.
#[test]
fn production_dns_windows_state_decades_at_the_v2_cadence_until_rescaled() {
    const SECS: u64 = 120;
    const YEAR: u64 = 365 * 86_400;
    assert!(
        PRODUCTION_DNS_PARAMS.unbonding_period_blocks * SECS > 40 * YEAR,
        "the premise of this test is the defect it guards: {} blocks",
        PRODUCTION_DNS_PARAMS.unbonding_period_blocks
    );
    let rescaled = PRODUCTION_DNS_PARAMS.at_two_minute_cadence();
    assert_eq!(rescaled.evidence_window_blocks * SECS, 14 * 86_400, "14 days, exactly");
    assert_eq!(rescaled.unbonding_period_blocks, rescaled.evidence_window_blocks + rescaled.max_reorg_horizon_blocks);
    // The two relational invariants a PURE duration rescale breaks, which is why this is a
    // substitution and not an arithmetic: 25x headroom over a healthy anchor-to-tip distance,
    // and the gate horizon's 3x over the TTL.
    let healthy_anchor_distance = rescaled.attestation_lag_blue_score + rescaled.epoch_length_blocks;
    assert!(rescaled.dns_veto_ttl_daa_score >= 25 * healthy_anchor_distance);
    assert!(rescaled.dns_gate_horizon_blocks >= 3 * rescaled.dns_veto_ttl_daa_score);
    // Only the CLOCK moves — the security posture is the network's, not the cadence's.
    assert_eq!(rescaled.min_bond_amount_sompi, PRODUCTION_DNS_PARAMS.min_bond_amount_sompi);
    assert_eq!(rescaled.min_active_validators, PRODUCTION_DNS_PARAMS.min_active_validators);
    assert_eq!(rescaled.min_active_stake_sompi, PRODUCTION_DNS_PARAMS.min_active_stake_sompi);
    assert_eq!(rescaled.required_work_depth, PRODUCTION_DNS_PARAMS.required_work_depth);
    assert_eq!(rescaled.reward_params, PRODUCTION_DNS_PARAMS.reward_params);
}

/// **A 10 bps base carries more than a wrong clock, and the gate must say so for each part.**
///
/// Asserted as three separate breakages of a set that passes, because a gate that only reads
/// `target_time_per_block` agrees with exactly the bug it exists to catch — which is what it
/// did until the mainnet audit.
#[test]
fn a_v2_network_is_refused_for_each_half_of_a_ten_bps_cadence() {
    let rc = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11));
    rc.validate_palw_v2().expect("the shipped RC is a well-formed V2 network");
    assert!(matches!(rc.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)));

    // The DAG parameters, with the clock left correct — the exact shape the old gate passed.
    let mut ten_bps_dag = rc.clone();
    ten_bps_dag.blockrate.ghostdag_k = BlockrateParams::new::<10>().ghostdag_k;
    assert_eq!(ten_bps_dag.blockrate.target_time_per_block, 120_000, "the clock is still right, which is the point");
    assert!(ten_bps_dag.validate_palw_v2().is_err(), "a 10 bps k at a 120 s clock must be refused");

    // The DNS windows — block counts that state 14 days and mean about 46 years here.
    let mut ten_bps_windows = rc.clone();
    ten_bps_windows.dns_params = Some(PRODUCTION_DNS_PARAMS);
    assert!(ten_bps_windows.validate_palw_v2().is_err(), "10 bps DNS windows at a 120 s clock must be refused");

    // And the fork that selects the cadence: mainnet's Crescendo is a finite height away.
    let mut future_crescendo = rc.clone();
    future_crescendo.crescendo_activation = MAINNET_PARAMS.crescendo_activation;
    assert!(!MAINNET_PARAMS.crescendo_activation.is_active(0), "the premise: mainnet's Crescendo is not active at genesis");
    assert!(future_crescendo.validate_palw_v2().is_err(), "a V2 network may not spend its pre-fork life at 10 bps");
}

/// **A bond may not leave before the history it voted on is beyond dispute.**
///
/// The rule the codebase enforces is `U >= R + E` (reorg horizon + evidence window), and the
/// evidence window dominates it. What that buys, and what nothing stated, is the property an
/// operator actually needs: an equivocation is provable for many finality windows after the
/// vote, so unbonding can never outrun the dispute it would escape.
///
/// Asserted as a RATIO rather than a block count, because both sides are re-derived whenever
/// the cadence changes — the 120 s testnet re-sized every window in this file by hand, and a
/// future re-sizing that halved the unbond would satisfy `U >= R + E` while quietly taking
/// this away.
#[test]
fn unbonding_outlasts_many_finality_windows() {
    for (label, dns, blockrate) in
        [("mainnet", PRODUCTION_DNS_PARAMS, MAINNET_PARAMS.blockrate), ("testnet-11", TESTNET_DNS_PARAMS, TESTNET_PARAMS.blockrate)]
    {
        let finality = blockrate.finality_depth;
        assert!(
            dns.unbonding_period_blocks >= 2 * finality,
            "{label}: unbonding {} must outlast two finality windows ({finality} blocks each)",
            dns.unbonding_period_blocks
        );
        // The rule the code enforces, restated where the ratio is asserted so the two cannot
        // be satisfied separately.
        assert!(dns.unbonding_period_blocks >= dns.max_reorg_horizon_blocks + dns.evidence_window_blocks, "{label}: U >= R + E");
    }
}

/// The 120 s/block constants the PUBLIC TESTNET runs must match the same formulas `Bps<BPS>`
/// encodes, evaluated at λ = 1/120, and the ghostdag k passed to the shared constructor must
/// be the one `calculate_ghostdag_k` actually returns — the one value the const fn cannot
/// compute for itself.
#[test]
fn two_minute_bps_constants_match_formulas() {
    use crate::config::{bps::calculate_ghostdag_k, constants::consensus::*};
    let b = BlockrateParams::new_two_minute_bps();
    let lambda = 1.0f64 / 120.0;
    assert_eq!(b.target_time_per_block, 120_000);
    assert_eq!(b.ghostdag_k as u64, calculate_ghostdag_k(2.0 * NETWORK_DELAY_BOUND as f64 * lambda, GHOSTDAG_TAIL_DELTA));
    assert_eq!(b.past_median_time_sample_rate, 1);
    assert_eq!(b.difficulty_sample_rate, 1);
    assert_eq!(b.max_block_parents, 10);
    assert_eq!(b.mergeset_size_limit, 180);
    // Duration-scaled depths: 12 h finality, 1 h merge, 100 s maturity — the SAME wall clock
    // the 10-bps mainnet has, which is the whole point of scaling by duration.
    assert_eq!(b.finality_depth, FINALITY_DURATION / 120);
    assert_eq!(b.merge_depth, MERGE_DEPTH_DURATION / 120);
    assert_eq!(b.finality_depth * 120, FINALITY_DURATION, "12 h exactly");
    assert_eq!(b.coinbase_maturity, 1, "100 s of maturity rounds UP to one 120 s block");
    // Pruning: at this rate the prunality lower bound EXCEEDS the duration term, so the bound
    // wins — recompute both sides rather than trusting the constant.
    let lower_bound =
        b.finality_depth + b.merge_depth * 2 + 4 * b.mergeset_size_limit * b.ghostdag_k as u64 + 2 * b.ghostdag_k as u64 + 2;
    assert!(lower_bound > PRUNING_DURATION / 120, "at 120 s the bound, not the 30 h duration, sets pruning depth");
    assert_eq!(b.pruning_depth, lower_bound);
    // Pruning depth still buys MORE than the 30 h the duration term intends.
    assert!(b.pruning_depth * 120 > PRUNING_DURATION);
    // The public testnet actually runs this interval. Its PoW is deliberately NOT the Ollama
    // PALW algo any more: that flavor was measured forgeable without running the model (see
    // the preset's comment), so the preset falls back to Phase-3 BLAKE2b-SHA3 until the
    // logits-binding replacement lands. Asserted, so re-enabling it is a deliberate edit here
    // rather than a quiet flip of one activation.
    assert_eq!(TESTNET_PARAMS.blockrate.target_time_per_block, 120_000);
    assert!(!TESTNET_PARAMS.pow_palw_ollama_activation.is_active(u64::MAX - 1), "the forgeable flavor must stay off");
    assert!(TESTNET_PARAMS.pow_blake2b_sha3_activation.is_active(0), "and the sound Phase-3 PoW carries the chain meanwhile");
    assert_eq!(TESTNET_PARAMS.target_time_per_block_history().after(), 120_000);
    // Emission holds the 37.047 MSK/s rate: one 120 s block pays 120× the per-second value.
    assert_eq!(TESTNET_PARAMS.bps(), 1, "integer bps floors at 1; exact rate work uses ttpb");
}

/// The VLT windows the public testnet runs are the wall-clock equivalents of PRODUCTION's,
/// not PRODUCTION's block counts. A regression here is silent and expensive: inherited
/// unchanged, the "14-day" unbonding window would be ~46 years at 120 s/block.
#[test]
fn testnet_vlt_windows_preserve_production_wall_clock() {
    let t = TESTNET_DNS_PARAMS;
    let secs = TESTNET_PARAMS.blockrate.target_time_per_block / 1000;
    assert_eq!(secs, 120);
    // 14 days of evidence/unbonding, exactly (the security property, in wall clock).
    assert_eq!(t.evidence_window_blocks * secs, 14 * 86_400);
    // Unbonding = evidence + the reorg horizon, so ADR-0009's U ≥ R + E still holds.
    assert_eq!(t.unbonding_period_blocks, t.evidence_window_blocks + t.max_reorg_horizon_blocks);
    assert!(t.unbonding_period_blocks >= t.max_reorg_horizon_blocks + t.evidence_window_blocks);
    // The veto TTL keeps PRODUCTION's ~30× headroom over a healthy anchor-to-tip distance,
    // and the gate horizon stays 3× the TTL.
    let healthy_anchor_distance = t.attestation_lag_blue_score + t.epoch_length_blocks;
    assert!(t.dns_veto_ttl_daa_score >= 25 * healthy_anchor_distance);
    assert_eq!(t.dns_gate_horizon_blocks, 3 * t.dns_veto_ttl_daa_score);
    // Settlement must outlast a contested fork: horizon + TTL, with margin.
    assert!(t.coinbase_settlement_long_maturity_daa > t.dns_gate_horizon_blocks + t.dns_veto_ttl_daa_score);
    // The stake-score window stays 15 epochs — the ratio emergency_stake_margin is calibrated
    // against (ADR-0018 / the 2026-07-19 incident note).
    assert_eq!(t.stake_score_window_blue_score, 15 * t.attestation_epoch_length_blue_score);
    // Bridge staleness keeps at least PRODUCTION's 150 s — as tolerance PAST the confirmed
    // anchor's healthy distance below the tip, which is how the bridge reads it since
    // 2026-09-10 (`dns_finality_fresh_for_bridge`). Read as a DAA distance that had to contain
    // that distance, 2 could never be met: the anchor is at least lag + backoff = 3 blue down.
    assert!(t.bridge_finality_max_staleness_daa_score * secs >= 150);
}

/// The hand-evaluated 0.1-bps blockrate constants must match the same formulas `Bps<BPS>`
/// encodes, evaluated at λ = 0.1 (`Bps` itself cannot express a sub-integer rate). Guards the
/// spelled-out values in `new_deci_bps` against drift in the shared duration constants.
#[test]
fn deci_bps_constants_match_formulas() {
    use crate::config::{bps::calculate_ghostdag_k, constants::consensus::*};
    let b = BlockrateParams::new_deci_bps();
    let lambda = 0.1f64;
    assert_eq!(b.target_time_per_block, 10_000, "1000 / 0.1 ms");
    assert_eq!(b.ghostdag_k as u64, calculate_ghostdag_k(2.0 * NETWORK_DELAY_BOUND as f64 * lambda, GHOSTDAG_TAIL_DELTA));
    // Per-block sample rates floor at 1 (0.1 · interval < 1 for both windows).
    assert_eq!(b.past_median_time_sample_rate, 1);
    assert_eq!(b.difficulty_sample_rate, 1);
    // Formula floors: k/2 = 2 < 10 parents; 2k = 8 < 180 mergeset.
    assert_eq!(b.max_block_parents, 10);
    assert_eq!(b.mergeset_size_limit, 180);
    // Duration-scaled depths: λ · duration, exactly.
    assert_eq!(b.merge_depth as f64, lambda * MERGE_DEPTH_DURATION as f64);
    assert_eq!(b.finality_depth as f64, lambda * FINALITY_DURATION as f64);
    assert_eq!(b.coinbase_maturity as f64, lambda * COINBASE_MATURITY_SECONDS as f64);
    // Pruning: the prunality lower bound at these constants sits below the duration term,
    // so the duration term wins — recompute both sides to keep that claim honest.
    let lower_bound =
        b.finality_depth + b.merge_depth * 2 + 4 * b.mergeset_size_limit * b.ghostdag_k as u64 + 2 * b.ghostdag_k as u64 + 2;
    assert!(lower_bound <= b.pruning_depth, "prunality lower bound {lower_bound} must not exceed pruning depth");
    assert_eq!(b.pruning_depth as f64, lambda * PRUNING_DURATION as f64);
    // The devnet preset actually runs these params with PALW active from genesis.
    assert_eq!(DEVNET_PARAMS.blockrate.target_time_per_block, 10_000);
    // Unchanged by the shared-constructor refactor: the hand-written table this replaced.
    assert_eq!(b.merge_depth, 360);
    assert_eq!(b.finality_depth, 4_320);
    assert_eq!(b.pruning_depth, 10_800);
    assert_eq!(b.coinbase_maturity, 10);
    // ADR-0068 Phase 1: the devnet BASE carries no V1 PALW proof-of-work any more — the
    // shipped form (`Params::from(Devnet)`) is the ConsensusV2 drill ruleset, whose gate
    // refuses a V1 fence beside it. The base const keeps the deci-bps blockrate this test
    // is about; the shipped form overrides the cadence to the frozen 120 s.
    assert!(!DEVNET_PARAMS.pow_palw_activation.is_active(0));
    assert!(matches!(
        Params::from(NetworkId::new(NetworkType::Devnet)).palw_consensus_mode,
        crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)
    ));
    // And the integer-bps view floors at 1 (the raw division is 0 — the reason emission
    // consumes `target_time_per_block_history` and sizing consumers get the floored view).
    assert_eq!(DEVNET_PARAMS.bps(), 1);
    assert_eq!(DEVNET_PARAMS.target_time_per_block_history().after(), 10_000);
}

#[test]
fn a_different_rule_set_gets_a_different_fingerprint() {
    // The whole point of the handshake check: two nodes answering the same network name must
    // not be able to peer while disagreeing about block validity.
    let base = SIMNET_PARAMS;
    let mut tweaked = SIMNET_PARAMS;
    tweaked.ghostdag_k += 1;
    assert_ne!(base.consensus_params_id(), tweaked.consensus_params_id());
}

/// **A scheduled fence moves the params id and NOT the identity id** (audit M1-6).
///
/// This is the property the whole split exists for: two builds that agree on every rule and
/// differ only in when a future rule turns on must be able to peer. If this ever fails, the
/// handshake is back to partitioning the network at deploy time, and the standing "ship
/// consensus changes as an activation" policy is unusable again.
#[test]
fn scheduling_a_fence_moves_the_params_id_but_not_the_identity() {
    let today = MAINNET_PARAMS;
    let mut scheduled = MAINNET_PARAMS;
    // The shape of a real upgrade: a fence that is off today is armed at a future height.
    scheduled.evm_f003_mldsa_verify_activation_daa_score = 9_000_000;

    assert_ne!(
        today.consensus_params_id(),
        scheduled.consensus_params_id(),
        "the legacy fingerprint must still notice a schedule change — old peers rely on it"
    );
    assert_eq!(
        today.consensus_identity_id(),
        scheduled.consensus_identity_id(),
        "scheduling a fence changed the identity: upgrading nodes would be refused by every un-upgraded peer"
    );
    assert_ne!(
        today.consensus_schedule_id(),
        scheduled.consensus_schedule_id(),
        "the schedule id must name the difference the operator log reports"
    );

    // And a change to a rule that decides the validity of blocks already on the chain still
    // moves the identity — the gate must keep refusing exactly those.
    let mut different_rules = MAINNET_PARAMS;
    different_rules.max_tx_inputs += 1;
    assert_ne!(
        today.consensus_identity_id(),
        different_rules.consensus_identity_id(),
        "a real rule change must still be refused at the handshake"
    );
}

/// **The token overlay is removed, so a network that arms it does not start.** Its fences stay in
/// `DnsParams` — the struct is hashed whole into `consensus_params_id` — but nothing folds a
/// ledger or settles emission any more, and a fence that schedules rules no code enforces is
/// refused at construction rather than shipped. Each of the three fences is refused on its own,
/// on every preset as the node materialises it; the shipped presets leave all three dormant and
/// start.
#[test]
fn arming_the_removed_token_overlay_is_refused_at_startup() {
    let nets = [MAINNET_PARAMS.net, TESTNET_PARAMS.net, TESTNET11_PARAMS.net, SIMNET_PARAMS.net, DEVNET_PARAMS.net];
    let mut refused = 0usize;
    for net in nets {
        let shipped = Params::from(net);
        shipped.validate_palw_v2().unwrap_or_else(|e| panic!("{net}: the shipped preset starts: {e}"));
        if shipped.dns_params.is_none() {
            continue;
        }
        for fence in ["tkn_shadow_activation_daa_score", "tkn_activation_daa_score", "emission_activation_epoch"] {
            let mut armed = shipped.clone();
            let tkn = &mut armed.dns_params.as_mut().expect("checked above").tkn;
            match fence {
                "tkn_shadow_activation_daa_score" => tkn.tkn_shadow_activation_daa_score = 9_000_000,
                "tkn_activation_daa_score" => tkn.tkn_activation_daa_score = 9_000_000,
                _ => tkn.emission_activation_epoch = 0,
            }
            let error = armed.validate_palw_v2().expect_err(&format!("{net}: arming {fence} must not start"));
            assert!(error.to_string().contains("the token overlay is removed"), "{net}: {fence} refused for another reason: {error}");
            refused += 1;
        }
    }
    assert!(refused >= 3, "no preset carries an overlay, so the refusal was never exercised");
}

/// **VLT voting weight is removed, so a network that arms its weight fence does not start.**
/// The fence stays in `DnsParams.vlt` — the struct is hashed whole into `consensus_params_id` —
/// but nothing moves voting power onto verified compute any more. Refused at any height,
/// including genesis, on every preset as the node materialises it; the shipped presets leave
/// it at `u64::MAX` and start.
#[test]
fn arming_the_removed_vlt_voting_weight_is_refused_at_startup() {
    let nets = [MAINNET_PARAMS.net, TESTNET_PARAMS.net, TESTNET11_PARAMS.net, SIMNET_PARAMS.net, DEVNET_PARAMS.net];
    let mut refused = 0usize;
    for net in nets {
        let shipped = Params::from(net);
        shipped.validate_palw_v2().unwrap_or_else(|e| panic!("{net}: the shipped preset starts: {e}"));
        let Some(dns) = shipped.dns_params.as_ref() else {
            continue;
        };
        assert_eq!(dns.vlt.vlt_activation_daa_score, u64::MAX, "{net}: a shipped preset armed the removed weight fence");
        for height in [0, 9_000_000, u64::MAX - 1] {
            let mut armed = shipped.clone();
            armed.dns_params.as_mut().expect("checked above").vlt.vlt_activation_daa_score = height;
            let error = armed.validate_palw_v2().expect_err(&format!("{net}: a weight fence at {height} must not start"));
            assert!(error.to_string().contains("VLT voting weight is removed"), "{net}: refused for another reason: {error}");
            refused += 1;
        }
    }
    assert!(refused >= 3, "no preset carries an overlay, so the refusal was never exercised");
}

/// **ADR-0128 Decision 8: the BFT gate starts only where it can be counted.** On every preset as
/// the node materialises it: the shipped gate is `None`; a well-formed gate starts wherever the
/// pruning depth holds the walk and is refused, naming the walk, wherever it does not; and each
/// malformed number, and a network with no overlay at all, is refused for its own reason.
#[test]
fn the_bft_gate_refuses_at_startup_what_it_could_not_count() {
    let gate = |activation| DnsBftGateV1 { activation, t_leak_daa: 5_040, reentry_final_depth_daa: 200, min_retained_validators: 4 };
    let refused_for = |params: &Params, why: &str, what: &str| {
        let error = params.validate_palw_v2().expect_err(&format!("{what} must not start"));
        assert!(error.to_string().contains(why), "{what}: refused for another reason: {error}");
    };
    let nets = [MAINNET_PARAMS.net, TESTNET_PARAMS.net, TESTNET11_PARAMS.net, SIMNET_PARAMS.net, DEVNET_PARAMS.net];
    let mut started = 0usize;
    for net in nets {
        let shipped = Params::from(net);
        shipped.validate_palw_v2().unwrap_or_else(|e| panic!("{net}: the shipped preset starts: {e}"));
        let scheduled = (net == TESTNET11_PARAMS.net).then(|| gate(ForkActivation::new(PALW_RC_PALW_UPGRADE_FENCE_DAA)));
        assert_eq!(shipped.dns_bft_gate, scheduled, "{net}: only testnet-11 schedules the BFT gate, on its 6,001 flag day");

        let mut no_overlay = shipped.clone();
        no_overlay.dns_params = None;
        no_overlay.dns_bft_gate = Some(gate(ForkActivation::new(9_000_000)));
        assert!(no_overlay.dns_bft_gate_fence().is_none(), "{net}: answered only where the network runs an overlay");
        refused_for(&no_overlay, "runs no DNS overlay", &format!("{net}: a gate without an overlay"));

        let Some(dns) = shipped.dns_params.clone() else { continue };
        let walk = crate::dns_bft_v1::dns_bft_walk_blue_score_v1(&gate(ForkActivation::always()), &dns).expect("fits");
        assert_eq!(
            walk,
            dns.stake_score_window_blue_score + 5_040 + 200 + dns.attestation_epoch_length_blue_score + dns.attestation_lag_blue_score
        );
        for activation in [ForkActivation::always(), ForkActivation::new(9_000_000), ForkActivation::never()] {
            let mut armed = shipped.clone();
            armed.dns_bft_gate = Some(gate(activation));
            if shipped.blockrate.pruning_depth >= walk {
                armed.validate_palw_v2().unwrap_or_else(|e| panic!("{net}: a well-formed gate at {activation:?} starts: {e}"));
                assert_eq!(armed.dns_bft_gate_fence(), Some(gate(activation)));
                started += 1;
            } else {
                refused_for(&armed, "the pruning point would pass the evidence", &format!("{net}: a walk of {walk}"));
            }
            for (why, malformed) in [
                ("t_leak_daa is zero", DnsBftGateV1 { t_leak_daa: 0, reentry_final_depth_daa: 0, ..gate(activation) }),
                (
                    "reentry_final_depth_daa is not below t_leak_daa",
                    DnsBftGateV1 { reentry_final_depth_daa: 5_040, ..gate(activation) },
                ),
                ("min_retained_validators is below four", DnsBftGateV1 { min_retained_validators: 3, ..gate(activation) }),
            ] {
                let mut armed = shipped.clone();
                armed.dns_bft_gate = Some(malformed);
                refused_for(&armed, why, &format!("{net}: {malformed:?}"));
            }
        }
        // The pruning-depth refusal is exact at its boundary — on a lineage whose other rules do
        // not read the depth.
        if !matches!(shipped.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)) {
            let mut at_the_walk = shipped.clone();
            at_the_walk.dns_bft_gate = Some(gate(ForkActivation::new(9_000_000)));
            at_the_walk.blockrate.pruning_depth = walk;
            at_the_walk.validate_palw_v2().unwrap_or_else(|e| panic!("{net}: a pruning depth equal to the walk starts: {e}"));
            let mut one_short = at_the_walk.clone();
            one_short.blockrate.pruning_depth = walk - 1;
            refused_for(&one_short, "the pruning point would pass the evidence", &format!("{net}: a pruning depth one short"));
        }
    }
    assert!(started > 0, "no preset could arm a well-formed gate, so its acceptance was never exercised");
    // testnet-11 with ADR-0128 §5's planned numbers, measured on the preset as it materialises:
    // its overlay runs at the two-minute cadence (a 30 blue-score StakeScore window, epochs and a
    // lag of 2), so the walk is 30 + 5,040 + 200 + 2 + 2 = 5,274 against a pruning depth of
    // 12,002 — inside it. (The ADR's prose quotes 1,500 + 5,440 = 6,940 and 12,000, which
    // assume 100-blue-score epochs this preset does not run; the refusal reads the preset.)
    let rc = palw_rc_shipped_params();
    let rc_walk = crate::dns_bft_v1::dns_bft_walk_blue_score_v1(
        &gate(ForkActivation::new(PALW_RC_PALW_UPGRADE_FENCE_DAA)),
        rc.dns_params.as_ref().unwrap(),
    );
    assert_eq!((rc_walk, rc.blockrate.pruning_depth), (Some(5_274), 12_002), "testnet-11's walk and horizon, measured");
    let mut armed_rc = rc.clone();
    armed_rc.dns_bft_gate = Some(gate(ForkActivation::new(PALW_RC_PALW_UPGRADE_FENCE_DAA)));
    armed_rc.validate_palw_v2().expect("testnet-11 may schedule the gate with the planned numbers");
}

/// **ADR-0128 Decision 8: the gate is hashed Some-only, and scheduling it is a schedule.** Its
/// height and each of its three numbers move `consensus_params_id` and `consensus_schedule_id`;
/// none of them moves `consensus_identity_id` while the height has not fired (the whole option
/// collapses out of it), and a gate in force from genesis does, because that is a rule about
/// block 1. The height is on the schedule the fork id is derived from. Every shipped preset
/// leaves the gate `None` and writes nothing, which `shipped_presets_have_pinned_fingerprints`
/// pins.
#[test]
fn the_bft_gate_is_hashed_some_only_and_a_scheduled_gate_stays_a_schedule() {
    let gate = DnsBftGateV1 {
        activation: ForkActivation::new(9_000_000),
        t_leak_daa: 5_040,
        reentry_final_depth_daa: 200,
        min_retained_validators: 4,
    };
    for (name, base) in [("mainnet", MAINNET_PARAMS), ("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        let with = |g: Option<DnsBftGateV1>| {
            let mut p = base.clone();
            p.dns_bft_gate = g;
            p
        };
        let unset = with(None);
        let scheduled = with(Some(gate));
        for (what, other) in [
            ("scheduling it", with(None)),
            ("its height", with(Some(DnsBftGateV1 { activation: ForkActivation::new(9_000_001), ..gate }))),
            ("t_leak_daa", with(Some(DnsBftGateV1 { t_leak_daa: 5_041, ..gate }))),
            ("the re-entry depth", with(Some(DnsBftGateV1 { reentry_final_depth_daa: 201, ..gate }))),
            ("the validator floor", with(Some(DnsBftGateV1 { min_retained_validators: 5, ..gate }))),
        ] {
            assert_ne!(
                scheduled.consensus_params_id(),
                other.consensus_params_id(),
                "{name}: {what} must move the printed fingerprint"
            );
            assert_ne!(scheduled.consensus_schedule_id(), other.consensus_schedule_id(), "{name}: {what} must move the schedule id");
            assert_eq!(
                scheduled.consensus_identity_id(),
                other.consensus_identity_id(),
                "{name}: {what} moved the identity — a gate that has not fired must not partition the fleet at deploy"
            );
        }
        let dormant = with(Some(DnsBftGateV1 { activation: ForkActivation::never(), ..gate }));
        assert_eq!(dormant.consensus_identity_id(), unset.consensus_identity_id(), "{name}: `never()` is absence to the identity");
        let at_genesis = with(Some(DnsBftGateV1 { activation: ForkActivation::always(), ..gate }));
        assert_ne!(
            at_genesis.consensus_identity_id(),
            unset.consensus_identity_id(),
            "{name}: a gate in force from block 1 is a rule"
        );
        assert!(scheduled.fence_schedule_v1().contains(&9_000_000), "{name}: the height is on the schedule");
        assert!(crate::fork_id_v1::fork_id_gate_fences_v1(&scheduled).contains(&9_000_000), "{name}: and named to the fork-id gate");
        assert!(scheduled.palw_fences_v1().contains(&("dns_bft_gate", Some(ForkActivation::new(9_000_000)))));
        assert!(!unset.fence_schedule_v1().contains(&9_000_000));
    }
}

/// **The inactivity leak is removed, so a network that sets `palw_inactivity_leak` does not
/// start.** The field stays (it is hashed only when set, so an unset preset's ids are what they
/// were), but no code excludes a silent validator from a quorum denominator any more — a
/// scheduled, a genesis-active and a `never()` leak are all refused, on every preset as the
/// node materialises it, and the shipped presets leave it unset and start.
#[test]
fn arming_the_removed_inactivity_leak_is_refused_at_startup() {
    let nets = [MAINNET_PARAMS.net, TESTNET_PARAMS.net, TESTNET11_PARAMS.net, SIMNET_PARAMS.net, DEVNET_PARAMS.net];
    for net in nets {
        let shipped = Params::from(net);
        shipped.validate_palw_v2().unwrap_or_else(|e| panic!("{net}: the shipped preset starts: {e}"));
        assert!(shipped.palw_inactivity_leak.is_none(), "{net}: a shipped preset set the removed leak");
        if let Some(dns) = shipped.dns_params.as_ref() {
            assert_eq!(dns.inactivity_leak_daa, u64::MAX, "{net}: the retired leak constant moved");
        }
        for activation in [ForkActivation::always(), ForkActivation::new(9_000_000), ForkActivation::never()] {
            let mut armed = shipped.clone();
            armed.palw_inactivity_leak = Some(PalwInactivityLeakV1 { activation, t_leak_daa: 5_040, reentry_final_depth_daa: 720 });
            let error = armed.validate_palw_v2().expect_err(&format!("{net}: a leak at {activation:?} must not start"));
            assert!(error.to_string().contains("the inactivity leak is removed"), "{net}: refused for another reason: {error}");
        }
    }
}

/// **A fence that is ACTIVE AT GENESIS is a rule, not a schedule** (re-audit R-1).
///
/// This is the pair a mainnet rollout of audit M1-1 actually produces: one node on the build
/// that ships `bond_spend_gate_mergeset_activation_daa_score: 0`, one still on the build that
/// ships `u64::MAX`. They agree on genesis and on the network name, so nothing else separates
/// them — and they disagree from block 1 about whether a merge-blue bond spend is valid. The
/// first version of the identity split normalised BOTH values to the same sentinel, so the
/// handshake kept the peer and the two built different chains in silence. If this assertion
/// ever flips back to equal, that silent fork is back.
#[test]
fn a_genesis_active_fence_is_not_normalised_away() {
    let shipped = MAINNET_PARAMS;
    let mut pre_m1_1 = MAINNET_PARAMS;
    pre_m1_1.dns_params.as_mut().unwrap().bond_spend_gate_mergeset_activation_daa_score = u64::MAX;

    assert_eq!(shipped.genesis.hash, pre_m1_1.genesis.hash, "the fixture must differ ONLY in the fence");
    assert_ne!(
        shipped.consensus_identity_id(),
        pre_m1_1.consensus_identity_id(),
        "a fence that is in force at genesis on one side and never on the other is a RULE difference: \
             the handshake must refuse it, not keep the peer with a warning"
    );

    // The other direction of the same rule: an overlay that is live from block 1 and one that
    // is off are not the same chain either.
    let mut overlay_off = MAINNET_PARAMS;
    overlay_off.dns_params.as_mut().unwrap().dns_activation_daa_score = u64::MAX;
    assert_ne!(shipped.consensus_identity_id(), overlay_off.consensus_identity_id());
}

/// **ADR-0064's bootstrap fence, in BOTH positions — the test that would have caught the trap.**
///
/// The fence was placed at top level so two builds that disagree only about a FUTURE height stay
/// peers. That only holds because of one clause: `for_each_fence` rewrites an armed height to
/// `ForkActivation::never()` and leaves it `Some`, and `consensus_params_id` writes bytes for a
/// `Some` that an unset build never writes — so without the collapse in
/// `normalize_values_a_scheduled_fence_drags_with_it` the identities differ and the handshake
/// refuses. That is the deploy-day partition the placement exists to prevent, reproduced by the
/// mechanism meant to prevent it.
///
/// A `const false` switch no fixture exercises is how four earlier findings survived to audit,
/// so this asserts all three positions: unset, armed-in-the-future, and active-at-genesis.
#[test]
fn the_bootstrap_fence_separates_networks_only_when_it_is_in_force() {
    let shipped = MAINNET_PARAMS;
    assert!(shipped.palw_bootstrap_activation.is_none(), "every shipped preset must leave ADR-0064 dormant");

    // Armed at a future height: SAME network. Peers on old and new builds must keep talking,
    // because nothing about the rules in force differs yet.
    let mut armed = MAINNET_PARAMS;
    armed.palw_bootstrap_activation = Some(ForkActivation::new(9_000_000));
    assert_eq!(
        shipped.consensus_identity_id(),
        armed.consensus_identity_id(),
        "a fence scheduled for the future is a SCHEDULE difference, not a rule difference —              if this fails, the Some(never()) collapse is gone and arming the fence partitions the network"
    );

    // ...but it is still a real commitment, so the params id (which is not normalised) moves.
    assert_ne!(
        shipped.consensus_params_id(),
        armed.consensus_params_id(),
        "arming the fence must be visible in the fingerprint, or it is not a consensus commitment at all"
    );

    // Active at genesis: a DIFFERENT network. Here the rule really is in force on one side.
    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_bootstrap_activation = Some(ForkActivation::always());
    assert_ne!(
        shipped.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "a fence in force from block 1 on one side and absent on the other is a rule difference"
    );

    // And an explicitly-never fence is exactly absence — the same network, byte for byte.
    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_bootstrap_activation = Some(ForkActivation::never());
    assert_eq!(
        shipped.consensus_identity_id(),
        never_armed.consensus_identity_id(),
        "Some(never()) and None say the same thing about the rules and must hash the same"
    );
}

/// **ADR-0066 Decision 1's fence: the heartbeat lane, armable on a running network.**
///
/// The lane used to be `PALW_HEARTBEAT_LANE_ENABLED`, a `const bool`. That is not a fence, it
/// is a rebuild — and worse, a rebuild that moved no fingerprint, so two builds from the same
/// tree disagreed about which blocks are valid while presenting identical identities to each
/// other at the handshake. A silent fork with no observable.
///
/// Both halves are asserted: the fence behaves like every other fence in the identity, AND it
/// can actually be armed on the shipped V2 preset — which is the property `PALW_HEARTBEAT_LANE_ENABLED`
/// could never have, since the only way to turn it on was to ship a different binary.
#[test]
fn the_heartbeat_fence_is_armable_and_is_not_a_rebuild() {
    let shipped = MAINNET_PARAMS;
    assert!(shipped.palw_heartbeat.is_none(), "every shipped preset leaves the lane closed");
    assert!(!shipped.palw_heartbeat_lane_open_at(0));

    let armed = |activation: ForkActivation| {
        let mut p = MAINNET_PARAMS;
        p.palw_heartbeat = Some(PalwHeartbeatV1 {
            activation,
            work_log2: crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
            max_per_mergeset: crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
        });
        p
    };
    assert_eq!(
        shipped.consensus_identity_id(),
        armed(ForkActivation::new(9_000_000)).consensus_identity_id(),
        "scheduled ahead: old and new builds stay peers, which is the whole reason it is a fence"
    );
    // **ADR-0066 SA-4 lands here, not on the line above.** The lane's price rides the fence and
    // two operators can disagree about it, so the schedule id — what the `flow_context` warning
    // prints — has to be able to name it. Gating on it instead would partition the fleet at the
    // FIRST host of the rollout this file records around `PALW_RC_PHASE1_FENCE_DAA`.
    assert_ne!(
        shipped.consensus_schedule_id(),
        armed(ForkActivation::new(9_000_000)).consensus_schedule_id(),
        "the operator log must name a scheduled heartbeat — SA-4"
    );
    assert_ne!(
        shipped.consensus_params_id(),
        armed(ForkActivation::new(9_000_000)).consensus_params_id(),
        "…and visible, which is the whole reason it is not a const"
    );
    assert_ne!(
        shipped.consensus_identity_id(),
        armed(ForkActivation::always()).consensus_identity_id(),
        "in force on one side is a block-validity difference and must split the network"
    );
    assert_eq!(
        shipped.consensus_identity_id(),
        armed(ForkActivation::never()).consensus_identity_id(),
        "Some(never()) is absence, or the collapse is gone"
    );

    // **The PRICE is in the fingerprint and not in the fence visitor** — the `inactivity_leak_daa`
    // lesson applied ahead of time. Normalising it would make two builds arming different
    // prices fingerprint identically at every height, and they would then disagree about
    // whether a solved header is valid the moment the lane opened.
    let mut cheap = armed(ForkActivation::new(9_000_000));
    let Some(h) = cheap.palw_heartbeat.as_mut() else { unreachable!() };
    h.work_log2 = crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2 + 1;
    assert_ne!(
        armed(ForkActivation::new(9_000_000)).consensus_params_id(),
        cheap.consensus_params_id(),
        "a different lane price is a different rule and must be in the fingerprint"
    );

    // **Armable on the preset that actually runs the lane**, at any height — no rebuild.
    for height in [ForkActivation::always(), ForkActivation::new(1), ForkActivation::new(9_000_000)] {
        let mut rc = palw_rc_shipped_params();
        rc.palw_heartbeat = Some(PalwHeartbeatV1 {
            activation: height,
            work_log2: crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
            max_per_mergeset: crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
        });
        rc.validate_palw_v2().expect("the shipped V2 preset may open its clock lane by configuration");
        assert!(rc.palw_heartbeat_lane_open_at(9_000_001));
    }

    // And a price this binary cannot compute is refused at boot rather than at block one:
    // the fence names the number operators coordinate on, `StateLayer0` applies it, and two
    // sources for one value is how a rule gets announced instead of enforced.
    let mut wrong_price = palw_rc_shipped_params();
    wrong_price.palw_heartbeat = Some(PalwHeartbeatV1 {
        activation: ForkActivation::always(),
        work_log2: crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2 + 1,
        max_per_mergeset: crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
    });
    assert!(wrong_price.validate_palw_v2().is_err(), "a lane price the binary does not implement must not start");

    // A hash network never opens the lane however the fence is set — there is no PALW clock to
    // rescue there, and the mode half is folded into one predicate so no call site can forget it.
    let mut hash_net = MAINNET_PARAMS;
    hash_net.palw_heartbeat = Some(PalwHeartbeatV1 {
        activation: ForkActivation::always(),
        work_log2: crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
        max_per_mergeset: crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
    });
    assert!(!hash_net.palw_heartbeat_lane_open_at(0), "the lane is a V2 rule");
    assert!(hash_net.palw_heartbeat_lane_fence().is_none());
}

/// **ADR-0105's fence: dormant everywhere it ships, a schedule where it is scheduled, a rule
/// where it is in force from genesis — and refused where there is no heartbeat to be about.**
///
/// The four positions every bare fence is held to (unset, scheduled, at genesis, `never()`),
/// measured on testnet-11 itself because that is where arming it would be a flag day: the
/// scheduled build must peer with today's (identity unchanged) while announcing the height in
/// all three places an operator or a peer reads one — the printed fingerprint (the field a
/// previous fence was destructured into and never written to, 0448d955), the schedule id, and
/// the fork id's schedule.
#[test]
fn the_heartbeat_transparent_fence_is_dormant_scheduled_or_a_rule_and_never_hides() {
    let t11 = palw_rc_shipped_params();
    // ADR-0105 §5.1 derives testnet-11's window from this number — 13 recovery slots at two blue
    // heartbeats a slot, ~26 minutes, before an exempt draw is red again. If it moves, so does
    // that figure (the pipeline test `the_fence_keeps_a_slow_draw_blue_only_inside_the_merge_depth_window`
    // measures the boundary on a fixture with the same cadence).
    assert_eq!((t11.ghostdag_k(), t11.merge_depth()), (1, 30), "testnet-11: k = 1 at 120 s, merge depth one hour");
    for (name, preset) in [
        ("mainnet", Params::from(NetworkId::new(NetworkType::Mainnet))),
        ("testnet-10", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 10))),
        ("testnet-11", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11))),
        ("devnet", Params::from(NetworkId::new(NetworkType::Devnet))),
        ("simnet", Params::from(NetworkId::new(NetworkType::Simnet))),
    ] {
        assert!(preset.palw_heartbeat_transparent.is_none(), "{name}: ADR-0105 ships dormant on every preset");
        assert!(preset.palw_heartbeat_transparent_fence().is_none(), "{name}");
    }

    let at = |activation: ForkActivation| {
        let mut p = palw_rc_shipped_params();
        p.palw_heartbeat_transparent = Some(activation);
        p
    };
    let scheduled = at(ForkActivation::new(9_000_000));
    scheduled.validate_palw_v2().expect("testnet-11 may schedule it: its heartbeat lane is armed from genesis");
    assert_eq!(
        t11.consensus_identity_id(),
        scheduled.consensus_identity_id(),
        "a scheduled height keeps the identity, or the first upgraded host partitions at deploy"
    );
    assert_ne!(t11.consensus_params_id(), scheduled.consensus_params_id(), "the printed fingerprint must carry the height");
    assert_ne!(t11.consensus_schedule_id(), scheduled.consensus_schedule_id(), "the operator log must name the height");
    assert!(scheduled.fence_schedule_v1().contains(&9_000_000), "the fork id is derived from this schedule");
    assert!(
        crate::fork_id_v1::fork_id_gate_fences_v1(&scheduled).contains(&9_000_000),
        "and the fork-id gate refuses on it once it fires — it changes which blocks are blue"
    );
    assert_ne!(
        t11.consensus_identity_id(),
        at(ForkActivation::always()).consensus_identity_id(),
        "in force from block 1 on one side is a coloring difference and must split the network"
    );
    // `never()` is absence to the handshake. (The printed fingerprint still writes the present
    // `Some(never())`, as it does for every bare fence: only the identity collapses it.)
    assert_eq!(t11.consensus_identity_id(), at(ForkActivation::never()).consensus_identity_id(), "never() is absence");
    assert!(at(ForkActivation::never()).palw_heartbeat_transparent_fence().is_some_and(|f| !f.is_active(u64::MAX - 1)));

    // The accessor folds the lane in, and the lane folds the mode in.
    assert_eq!(scheduled.palw_heartbeat_transparent_fence(), Some(ForkActivation::new(9_000_000)));
    let mut no_lane = at(ForkActivation::always());
    no_lane.palw_heartbeat = None;
    assert!(no_lane.palw_heartbeat_transparent_fence().is_none(), "no heartbeat lane, nothing for the rule to be about");
    assert!(no_lane.validate_palw_v2().is_err(), "…and a node refuses to start rather than hash a fence that cannot fire");
    let mut hash_net = MAINNET_PARAMS;
    hash_net.palw_heartbeat_transparent = Some(ForkActivation::always());
    assert!(hash_net.palw_heartbeat_transparent_fence().is_none(), "a hash network has no heartbeat lane");
    assert!(hash_net.validate_palw_v2().is_err(), "…so arming it there is refused, not ignored");
    let mut hash_net_never = MAINNET_PARAMS;
    hash_net_never.palw_heartbeat_transparent = Some(ForkActivation::never());
    hash_net_never.validate_palw_v2().expect("never() is absence and is exempt, as for every bare fence");
}

/// **ADR-0068 Phase 1: the two values that closed ADR-0066's leftovers follow the exact fence
/// discipline the heartbeat established.** F3a's width bound rides the heartbeat fence as a
/// second locked value; F2's attempt-work constant is its own fence. For each: shipped unset,
/// armable by configuration on the shipped V2 preset, scheduled-ahead keeps the identity,
/// in-force splits it, the VALUE is in the fingerprint and never in the visitor, and a value
/// the binary does not implement refuses to start.
#[test]
fn the_attempt_work_fence_and_the_width_bound_follow_the_fence_discipline() {
    let shipped = MAINNET_PARAMS;
    assert!(shipped.palw_attempt_work.is_none(), "every shipped preset leaves the constant off");
    assert!(!shipped.palw_attempt_work_open_at(0));

    let armed = |activation: ForkActivation| {
        let mut p = MAINNET_PARAMS;
        p.palw_attempt_work = Some(PalwAttemptWorkV1 {
            activation,
            work_log2: crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2,
            ticket_bucket_log2: crate::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2,
        });
        p
    };
    assert_eq!(
        shipped.consensus_identity_id(),
        armed(ForkActivation::new(9_000_000)).consensus_identity_id(),
        "scheduled ahead: old and new builds stay peers"
    );
    // ADR-0066 SA-4, the heartbeat's rule for the heartbeat's reason: `work_log2` here is
    // `header.blue_work` on every attempt block, so two builds that schedule different values
    // must be TOLD apart even though they are not KEPT apart.
    assert_ne!(
        shipped.consensus_schedule_id(),
        armed(ForkActivation::new(9_000_000)).consensus_schedule_id(),
        "the operator log must name a scheduled attempt-work price — SA-4"
    );
    assert_ne!(
        shipped.consensus_params_id(),
        armed(ForkActivation::new(9_000_000)).consensus_params_id(),
        "…and visible, which is what makes it a fence and not a const"
    );
    assert_ne!(
        shipped.consensus_identity_id(),
        armed(ForkActivation::always()).consensus_identity_id(),
        "in force on one side is a block-validity difference and must split the network"
    );
    assert_eq!(
        shipped.consensus_identity_id(),
        armed(ForkActivation::never()).consensus_identity_id(),
        "Some(never()) is absence, or the collapse is gone"
    );

    // The WORK is in the fingerprint and not in the fence visitor — the same lesson, applied
    // to the same shape, for the third time.
    let mut cheap = armed(ForkActivation::new(9_000_000));
    let Some(w) = cheap.palw_attempt_work.as_mut() else { unreachable!() };
    w.work_log2 = crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2 + 1;
    assert_ne!(
        armed(ForkActivation::new(9_000_000)).consensus_params_id(),
        cheap.consensus_params_id(),
        "a different attempt work is a different rule and must be in the fingerprint"
    );

    // Armable on the preset that actually runs the lane; a value the binary does not
    // implement refuses to start — for the attempt work AND for the width bound.
    let mut rc = palw_rc_shipped_params();
    rc.palw_attempt_work = Some(PalwAttemptWorkV1 {
        activation: ForkActivation::new(1),
        work_log2: crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2,
        ticket_bucket_log2: crate::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2,
    });
    rc.validate_palw_v2().expect("the shipped V2 preset may re-price its attempt lane by configuration");
    assert!(rc.palw_attempt_work_open_at(2));

    let mut wrong_work = palw_rc_shipped_params();
    wrong_work.palw_attempt_work = Some(PalwAttemptWorkV1 {
        activation: ForkActivation::always(),
        work_log2: crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2 + 1,
        ticket_bucket_log2: crate::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2,
    });
    assert!(wrong_work.validate_palw_v2().is_err(), "an attempt work the binary does not implement must not start");

    let mut wrong_width = palw_rc_shipped_params();
    wrong_width.palw_heartbeat = Some(PalwHeartbeatV1 {
        activation: ForkActivation::always(),
        work_log2: crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
        max_per_mergeset: crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET + 1,
    });
    assert!(wrong_width.validate_palw_v2().is_err(), "a width bound the binary does not enforce must not start");

    // The width bound is in the fingerprint too — two builds bounding different widths must
    // not share one, or they disagree about a merging block's validity when the lane opens.
    let hb_armed = |max_per_mergeset: u64| {
        let mut p = MAINNET_PARAMS;
        p.palw_heartbeat = Some(PalwHeartbeatV1 {
            activation: ForkActivation::new(9_000_000),
            work_log2: crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
            max_per_mergeset,
        });
        p
    };
    assert_ne!(
        hb_armed(crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET).consensus_params_id(),
        hb_armed(crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET + 1).consensus_params_id(),
        "a different width bound is a different rule and must be in the fingerprint"
    );

    // And the mode is folded into the accessor: a hash network never re-prices a lane it
    // does not have.
    let mut hash_net = MAINNET_PARAMS;
    hash_net.palw_attempt_work = Some(PalwAttemptWorkV1 {
        activation: ForkActivation::always(),
        work_log2: crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2,
        ticket_bucket_log2: crate::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2,
    });
    assert!(!hash_net.palw_attempt_work_open_at(0), "the constant is a V2 rule");
    assert!(hash_net.palw_attempt_work_fence().is_none());
}

/// **ADR-0066 security amendment SA-4 — a value beside a fence is REPORTED, and the rollout it
/// rides on still rolls.**
///
/// SA-4 names a real hazard: `for_each_fence` normalises a scheduled height, the collapse then
/// drops the whole option, and the VALUE beside the fence went with it — so two operators who
/// schedule one rule at one height with different values peered, synced, and priced a lane
/// differently the moment it fired, and the only warning they got printed two IDENTICAL
/// schedule ids.
///
/// Where the amendment lands is a decision, because the two properties are contradictory for
/// any equality-compared fingerprint: `id(None) == id(Some(H, v))` is the rolling deploy, and
/// `id(Some(H, v1)) != id(Some(H, v2))` is the gate, and transitivity forbids both. The
/// decision is the one `palw_bond_maturity.window_daa` already made and documented — report,
/// do not gate — and it is made for two reasons this test pins one by one:
///
/// 1. The rolling deploy is not hypothetical. The re-pin log in this file records the live
///    testnet-11 rollout of 2026-09-01, which scheduled `palw_heartbeat` and
///    `palw_attempt_work` at DAA 5,000 with the tip at 1,746 and rolled the fleet host by host
///    *because* a scheduled fence normalises out of `consensus_identity_id`. Gating on the
///    value partitions that fleet at the first host to restart.
/// 2. The hazard is answered where it can be answered without that cost: `consensus_schedule_id`
///    is what the `flow_context` warning prints when two peers' params ids differ and their
///    identities agree, and before this amendment it was blind to precisely the disagreement it
///    is the only defence against.
///
/// So: the value must SEPARATE the schedule ids, must NOT separate the identities while the
/// fence is only scheduled, and must still separate the identities once the fence is in force
/// at genesis — where the two builds disagree about block 1 and the handshake is right to
/// refuse.
#[test]
fn a_value_beside_a_scheduled_fence_is_reported_and_the_rollout_still_rolls() {
    // The heartbeat's price, the amendment's named value (its first, the inactivity leak's
    // grace, is removed with the leak). On a hash preset the fence folds to `None` through the
    // mode condition, so this uses the V2 ruleset the fence is real on.
    let beat = |work_log2: u32, activation: ForkActivation| {
        let mut p = palw_rc_shipped_params();
        p.palw_heartbeat =
            Some(PalwHeartbeatV1 { activation, work_log2, max_per_mergeset: crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET });
        p
    };
    let mut no_beat = palw_rc_shipped_params();
    no_beat.palw_heartbeat = None;
    for height in [ForkActivation::always(), ForkActivation::new(9_000_000)] {
        assert_ne!(
            beat(crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2, height).consensus_schedule_id(),
            beat(crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2 + 1, height).consensus_schedule_id(),
            "work_log2 is the lane's price and the operator log must be able to name two prices"
        );
    }
    assert_eq!(
        beat(crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2, ForkActivation::new(9_000_000)).consensus_identity_id(),
        beat(crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2, ForkActivation::new(12_345_678)).consensus_identity_id(),
        "…while the SCHEDULE still normalises away: agreeing operators must still be able to roll"
    );
    assert_ne!(
        beat(crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2, ForkActivation::always()).consensus_identity_id(),
        beat(crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2 + 1, ForkActivation::always()).consensus_identity_id(),
        "a price already in force is a rule difference the handshake must refuse"
    );
    // **The ADR-0068 Phase-1 rollout, as an assertion.** Every host that restarts on the
    // scheduling build must still handshake with every host that has not.
    for price in [crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2, crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2 + 1] {
        assert_eq!(
            no_beat.consensus_identity_id(),
            beat(price, ForkActivation::new(9_000_000)).consensus_identity_id(),
            "scheduling the heartbeat must not disconnect the first upgraded host from the rest of the fleet"
        );
    }

    // And the attempt lane's constant work, the same shape for the same reason.
    let work = |work_log2: u32, activation: ForkActivation| {
        let mut p = palw_rc_shipped_params();
        p.palw_attempt_work = Some(PalwAttemptWorkV1 {
            activation,
            work_log2,
            ticket_bucket_log2: crate::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2,
        });
        p
    };
    let mut no_work = palw_rc_shipped_params();
    no_work.palw_attempt_work = None;
    for height in [ForkActivation::always(), ForkActivation::new(9_000_000)] {
        assert_ne!(
            work(crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2, height).consensus_schedule_id(),
            work(crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2 + 1, height).consensus_schedule_id(),
            "attempt blue work is `header.blue_work` on every attempt block — the log must name two values"
        );
    }
    assert_ne!(
        work(crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2, ForkActivation::always()).consensus_identity_id(),
        work(crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2 + 1, ForkActivation::always()).consensus_identity_id(),
        "blue work already in force is a rule difference the handshake must refuse"
    );
    for price in [crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2, crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2 + 1] {
        assert_eq!(
            no_work.consensus_identity_id(),
            work(price, ForkActivation::new(9_000_000)).consensus_identity_id(),
            "scheduling the attempt lane must not disconnect the first upgraded host from the rest of the fleet"
        );
    }
}

/// **ADR-0065 D2's fence — the comparison-site rule — in the same four positions, plus the
/// one property the other two fences do not have to prove: it can be armed unconditionally.**
///
/// D1 carries a precondition (`palw_bond_maturity` needs a registry with a spare seat) and is
/// therefore refusable at boot. D2 has none: it changes which SITE two branches are compared
/// at — the fork point rather than each branch's own tip — and that is a pure rule change with
/// no registry, no duration and no quantity behind it. So "armable" here means exactly that
/// `validate_palw_v2` accepts it on every shipped preset, at any height, which is worth
/// asserting precisely because there is no guard that could refuse it: a fence with no
/// precondition is one whose only failure mode is being wired wrong, and this is the wiring.
#[test]
fn the_frontier_provenance_fence_separates_networks_only_when_it_is_in_force() {
    let shipped = MAINNET_PARAMS;
    assert!(shipped.palw_frontier_provenance.is_none(), "every shipped preset must leave ADR-0065 D2 dormant");

    let mut scheduled = MAINNET_PARAMS;
    scheduled.palw_frontier_provenance = Some(ForkActivation::new(9_000_000));
    assert_eq!(
        shipped.consensus_identity_id(),
        scheduled.consensus_identity_id(),
        "scheduling D2 ahead must keep old and new builds peers — it is a reorg rule and it has to reach a \
             running network by rolling deploy"
    );
    assert_ne!(
        shipped.consensus_params_id(),
        scheduled.consensus_params_id(),
        "…and still be visible in the fingerprint, or two builds compare branches at different sites in silence"
    );

    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_frontier_provenance = Some(ForkActivation::always());
    assert_ne!(
        shipped.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "in force from block 1 on one side is a rule difference — the two disagree about which minted seats \
             a candidate branch may count"
    );

    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_frontier_provenance = Some(ForkActivation::never());
    assert_eq!(
        shipped.consensus_identity_id(),
        never_armed.consensus_identity_id(),
        "Some(never()) is absence, or the collapse is gone"
    );

    // Independent of the fences beside it — arming one must never read as arming another.
    let mut d2_and_d4 = MAINNET_PARAMS;
    d2_and_d4.palw_frontier_provenance = Some(ForkActivation::always());
    d2_and_d4.palw_unavailable_abstains = Some(ForkActivation::always());
    assert_ne!(d2_and_d4.consensus_identity_id(), at_genesis.consensus_identity_id(), "two fences, two identities");

    // **Armable on every shipped preset, at any height, with no precondition to satisfy.**
    // This is the half the D1 work showed can be missing while everything else looks right:
    // a rule can be wired, fingerprinted and enforced and still be one no shipped network can
    // turn on. For D2 there is nothing to satisfy, and that is the assertion.
    for (name, base) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        for height in [ForkActivation::always(), ForkActivation::new(1), ForkActivation::new(9_000_000)] {
            let mut armed = base.clone();
            armed.palw_frontier_provenance = Some(height);
            armed.validate_palw_v2().unwrap_or_else(|e| panic!("{name}: D2 must be armable at any height: {e}"));
        }
    }
    let mut armed_rc = palw_rc_shipped_params();
    armed_rc.palw_frontier_provenance = Some(ForkActivation::always());
    armed_rc.validate_palw_v2().expect("and on the V2 preset that actually runs the rule");
}

/// **ADR-0069 Decision 7's fence, in the same four positions — and it is the one whose
/// dormancy has to be checked on the ASSEMBLED rulesets too.**
///
/// Decision 7 changes fork choice: past it a block whose class holds no granted share
/// contributes zero pwu to `safe` and `live`. Two builds that disagree about whether that rule
/// is in force choose different tips from the same DAG, so the fence has to be visible in the
/// fingerprint while leaving the dormant presets byte-identical. That pair is what the first
/// two assertions are.
///
/// **What they are NOT is a licence to schedule one.** The fence also carries the free-prompt
/// retirement repair, which cannot heal a chain that already stranded weight, so the last loop
/// in this test pins the other half: armable at genesis, refused at every other height.
///
/// The RC ruleset is checked as well as the raw preset. `palw_rc_shipped_params` is what
/// testnet-11 actually runs, it arms ADR-0065 D4 from genesis, and a Decision 7 armed there by
/// accident would move `consensus_params_id` and refuse every node on the live build at the
/// handshake.
#[test]
fn the_uncertified_weightless_fence_separates_networks_only_when_it_is_in_force() {
    let shipped = MAINNET_PARAMS;
    assert!(shipped.palw_uncertified_weightless.is_none(), "every shipped preset must leave ADR-0069 D7 dormant");
    for (name, base) in [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS)] {
        assert!(base.palw_uncertified_weightless.is_none(), "{name}: a fork-choice rule may not ship armed by default");
    }
    // **DEVNET arms it, at genesis and only at genesis** (the 5f genesis card §1, rehearsed on
    // devnet first). "By default" is what the rule above forbids; this is a decision the card
    // records, and `validate_palw_v2` refuses the fence at any other height — so the assertion
    // is on the VALUE, not merely on presence. A scheduled D7 would change what a chain thinks
    // uncertified classes weighed in history it already accepted.
    assert_eq!(
        DEVNET_PARAMS.palw_uncertified_weightless,
        Some(ForkActivation::always()),
        "devnet arms ADR-0069 Decision 7 at genesis, and `always()` is the only height it may take"
    );
    devnet_shipped_params().validate_palw_v2().expect("the armed devnet assembles");
    assert_eq!(
        palw_rc_shipped_params().palw_uncertified_weightless,
        Some(ForkActivation::always()),
        "the assembled RC ruleset (testnet-11 Relaunch 5f) ARMS it at genesis too — stated on the preset \
             (`palw_rc_base_params`), where the card's table can be checked against it"
    );

    let mut scheduled = MAINNET_PARAMS;
    scheduled.palw_uncertified_weightless = Some(ForkActivation::new(9_000_000));
    assert_eq!(
        shipped.consensus_identity_id(),
        scheduled.consensus_identity_id(),
        "a fence not yet in force does not separate identities — the normalization property, pinned here even \
             though `validate_palw_v2` now refuses to START a scheduled D7 (see the loop at the end of this test): \
             the collapse is what keeps the DORMANT presets byte-identical, and it must not depend on the gate"
    );
    assert_ne!(
        shipped.consensus_params_id(),
        scheduled.consensus_params_id(),
        "…and still be visible in the fingerprint, or two builds price the same block differently in silence"
    );

    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_uncertified_weightless = Some(ForkActivation::always());
    assert_ne!(
        shipped.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "in force from block 1 on one side is a rule difference — the two disagree about what an uncertified \
             class's block is worth to fork choice"
    );

    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_uncertified_weightless = Some(ForkActivation::never());
    assert_eq!(
        shipped.consensus_identity_id(),
        never_armed.consensus_identity_id(),
        "Some(never()) is absence, or the collapse is gone"
    );

    // Independent of the fences beside it — arming one must never read as arming another.
    let mut d2_and_d7 = MAINNET_PARAMS;
    d2_and_d7.palw_uncertified_weightless = Some(ForkActivation::always());
    d2_and_d7.palw_frontier_provenance = Some(ForkActivation::always());
    assert_ne!(d2_and_d7.consensus_identity_id(), at_genesis.consensus_identity_id(), "two fences, two identities");

    // **Armable AT GENESIS on every shipped preset — and at no other height.**
    //
    // The earlier draft of this loop asserted the opposite ("D7 must be armable at any
    // height"), on D2's reasoning: the predicate is the granted share the chain already
    // records, so no preset can fail to provide a precondition. That is still true of the
    // WEIGHT RULE and false of the fence as a whole, because the fence also carries the
    // free-prompt retirement repair, and a repair has a precondition a schedule cannot meet:
    // that no retirement has already happened under the broken arithmetic.
    // `arming_the_fence_does_not_repair_a_chain_that_already_stranded_weight` measures all
    // three positions — the armed re-derivation of a stranded state, the dormant one, and a
    // block folded past the activation — and every one of them refuses. A scheduled arming is
    // therefore not a remedy that arrives late; it is a remedy that never arrives, on a chain
    // whose nodes are already refusing their own tips. So the configuration is refused at
    // construction (`ConfigBuilder::build` panics), which is the only way "arm it at genesis,
    // never as a rolling deploy" is a rule rather than a line in a runbook.
    for (name, base) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        let mut at_genesis = base.clone();
        at_genesis.palw_uncertified_weightless = Some(ForkActivation::always());
        at_genesis.validate_palw_v2().unwrap_or_else(|e| panic!("{name}: D7 must be armable at genesis: {e}"));

        let mut dormant = base.clone();
        dormant.palw_uncertified_weightless = Some(ForkActivation::never());
        dormant.validate_palw_v2().unwrap_or_else(|e| panic!("{name}: `Some(never())` is absence and must start: {e}"));

        for height in [ForkActivation::new(1), ForkActivation::new(9_000_000)] {
            let mut scheduled_arm = base.clone();
            scheduled_arm.palw_uncertified_weightless = Some(height);
            let refused = scheduled_arm.validate_palw_v2().expect_err(&format!(
                "{name}: a scheduled D7 arming must refuse to start — it cannot repair the stranding it inherits"
            ));
            assert!(
                format!("{refused}").contains("genesis-only"),
                "{name}: the refusal must say what the operator may do instead: {refused}"
            );
        }
    }
    let mut armed_rc = palw_rc_shipped_params();
    armed_rc.palw_uncertified_weightless = Some(ForkActivation::always());
    armed_rc.validate_palw_v2().expect("and on the V2 preset that actually runs the rule");
    let mut scheduled_rc = palw_rc_shipped_params();
    scheduled_rc.palw_uncertified_weightless = Some(ForkActivation::new(9_000_000));
    scheduled_rc
        .validate_palw_v2()
        .expect_err("…and the preset testnet-11 runs is the one this matters on: no rolling arming there either");
}

/// **ADR-0075 D14's fence, in the same four positions.**
///
/// The rule it arms changes which objects a block accepts — a chunk whose index falls outside
/// its own count no longer spends the block's certification cap — and therefore the state root.
/// So it has to reach a running network the way D2 and D4 do: scheduled ahead, old and new
/// builds still peers, and the difference visible in the fingerprint the operator log prints.
/// A bundle placement would move `palw_ruleset_id_v2`, which `for_each_fence` never descends
/// into, and every old/new pair would refuse the handshake outright.
#[test]
fn the_chunk_cap_charge_fence_separates_networks_only_when_it_is_in_force() {
    let shipped = MAINNET_PARAMS;
    assert!(shipped.palw_chunk_cap_charge.is_none(), "every shipped preset must leave ADR-0075 D14 dormant");

    let mut scheduled = MAINNET_PARAMS;
    scheduled.palw_chunk_cap_charge = Some(ForkActivation::new(9_000_000));
    assert_eq!(
        shipped.consensus_identity_id(),
        scheduled.consensus_identity_id(),
        "scheduling D14 ahead must keep old and new builds peers — it is an acceptance rule and it has to reach a \
             running network by rolling deploy"
    );
    assert_ne!(
        shipped.consensus_params_id(),
        scheduled.consensus_params_id(),
        "…and still be visible in the fingerprint, or two builds grade different object sets in silence"
    );

    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_chunk_cap_charge = Some(ForkActivation::always());
    assert_ne!(
        shipped.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "in force from block 1 on one side is a rule difference — the two accept different objects from one block"
    );

    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_chunk_cap_charge = Some(ForkActivation::never());
    assert_eq!(
        shipped.consensus_identity_id(),
        never_armed.consensus_identity_id(),
        "Some(never()) is absence, or the collapse is gone"
    );

    // Independent of the fences beside it — arming one must never read as arming another.
    let mut d14_and_d2 = MAINNET_PARAMS;
    d14_and_d2.palw_chunk_cap_charge = Some(ForkActivation::always());
    d14_and_d2.palw_frontier_provenance = Some(ForkActivation::always());
    assert_ne!(d14_and_d2.consensus_identity_id(), at_genesis.consensus_identity_id(), "two fences, two identities");

    // Armable on every shipped preset, at any height: the rule is the transition's own
    // `index < count` and there is no registry, duration or quantity behind it to satisfy.
    for (name, base) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        for height in [ForkActivation::always(), ForkActivation::new(1), ForkActivation::new(9_000_000)] {
            let mut armed = base.clone();
            armed.palw_chunk_cap_charge = Some(height);
            armed.validate_palw_v2().unwrap_or_else(|e| panic!("{name}: D14 must be armable at any height: {e}"));
        }
    }
    let mut armed_rc = palw_rc_shipped_params();
    armed_rc.palw_chunk_cap_charge = Some(ForkActivation::always());
    armed_rc.validate_palw_v2().expect("and on the V2 preset that actually grades chunks");
}

/// **ADR-0065 D4's fence, in the same four positions.**
///
/// A separate test rather than a loop over the two fences, because the reason this one is at
/// top level is different and stronger. ADR-0064's fence enables a capability nobody needs
/// until a chain has stopped; D4's REMOVES a rule that is convicting honest producers on a
/// live network right now, so it has to reach that network by rolling deploy. A bundle
/// placement would move `palw_ruleset_id_v2` — which `for_each_fence` never descends into —
/// and every old/new pair would fail the handshake outright instead of peering with a warning,
/// which for a fix that must be deployed while the chain runs is the difference between a
/// remedy and a second outage.
/// **The RC ruleset arms D4, and the preset it is built from does not** (ADR-0068 launch audit).
///
/// Two facts, and the pair is the point. Dormant, an `Unavailable` quorum needs no proof: three
/// seats that did not RECEIVE material sign what three withheld-from seats would sign, and the
/// claim voids with the producer's bond and the honest `Valid` signers' collateral. The measured
/// transport rate on testnet-11 put that within ordinary reach with no attacker present.
///
/// Asserted as a DIFFERENCE rather than as one `is_some()`: the fence lives outside the V2
/// bundle precisely so an un-armed build and an armed one stay peers, and a test that only
/// checked the armed side would pass just as happily if arming had been moved into the bundle
/// and taken the handshake with it.
#[test]
fn the_rc_ruleset_arms_the_unavailable_abstains_rule_from_genesis() {
    let rc = palw_rc_shipped_params();
    assert_eq!(
        rc.palw_unavailable_abstains,
        Some(ForkActivation::always()),
        "a new genesis has no running chain to keep peers with, so the honest form is armed at block one"
    );
    assert!(
        rc.palw_unavailable_abstains.is_some_and(|f| f.is_active(0)),
        "and it must be in force AT genesis — a fence that arms later leaves the first epochs slashing honest seats"
    );
    // The preset it is assembled from stays dormant: arming is the assembly's decision, which
    // is what keeps `for_each_fence` able to reach it on a running network.
    assert!(MAINNET_PARAMS.palw_unavailable_abstains.is_none(), "the raw preset is untouched");
}

/// **ADR-0071 SA-1..SA-4's fence, held to the D4 discipline exactly.**
///
/// Four properties, and each one is a way this could have shipped wrong: every preset leaves
/// it dormant so no shipped fingerprint moves; scheduling it for a future height keeps old and
/// new builds peers (which is what the TOP-LEVEL placement buys, and what a bundle placement
/// would have taken away); arming it at genesis is a real rule difference and separates
/// identities; and `Some(never())` is absence.
#[test]
fn the_capability_bound_fence_separates_networks_only_when_it_is_in_force() {
    for (name, preset) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(preset.palw_capability_bound.is_none(), "{name} must leave ADR-0071's capability bound dormant");
    }
    assert!(
        palw_rc_shipped_params().palw_capability_bound.is_none(),
        "and so must the RC assembly — SA-3 narrows the panel draw, so arming it is a measured decision and not a default"
    );
    // **…and a CARD states it from genesis, which is the other half of "dormant" meaning
    // something** (mainnet audit 2026-09-06, M-14). A fence every configuration leaves off is a
    // rule that ships as a comment: the SA-1 bound and the SA-2 price are unreachable at both
    // writers on every network this tree can build. A live chain reaches them by activation,
    // which is the operator's call; a fresh network is born with them, which is this function's.
    // The set-difference test below cannot see this — it compares armed sets and a fence dormant
    // on both sides is in neither — so it is asserted here, positively.
    assert_eq!(
        mainnet_card_fixture_v1(true).palw_capability_bound,
        Some(ForkActivation::always()),
        "a carded mainnet states ADR-0071 SA-1/SA-2/SA-4 from genesis"
    );

    let shipped = MAINNET_PARAMS;
    let mut armed = MAINNET_PARAMS;
    armed.palw_capability_bound = Some(ForkActivation::new(9_000_000));
    assert_eq!(
        shipped.consensus_identity_id(),
        armed.consensus_identity_id(),
        "scheduling it for a future height must keep old and new builds peers — the whole reason it is not in the V2 bundle"
    );
    assert_ne!(
        shipped.consensus_params_id(),
        armed.consensus_params_id(),
        "…while still being a visible commitment: a rule change nobody can observe in the fingerprint is how two builds fork \
             in silence"
    );

    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_capability_bound = Some(ForkActivation::always());
    assert_ne!(
        shipped.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "in force from block 1 on one side is a rule difference — the two disagree about which bonds may be seated"
    );

    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_capability_bound = Some(ForkActivation::never());
    assert_eq!(
        shipped.consensus_identity_id(),
        never_armed.consensus_identity_id(),
        "Some(never()) is absence, or the collapse in normalize_values_a_scheduled_fence_drags_with_it is gone"
    );

    // Independent of its neighbours: arming one must not be readable as arming the other.
    let mut d4 = MAINNET_PARAMS;
    d4.palw_unavailable_abstains = Some(ForkActivation::always());
    assert_ne!(
        at_genesis.consensus_params_id(),
        d4.consensus_params_id(),
        "the capability bound and D4 are different rules and must fingerprint differently"
    );
}

/// **The mode condition is folded in once**, so the three sites that ask (the fold, the
/// acceptance layer, the panel assembler) cannot each remember it differently. `capable_classes`
/// and the claim lane exist only under `ConsensusV2`; on a hash network there is nothing for
/// this rule to bound, so an armed fence there must read as off.
#[test]
fn the_capability_bound_needs_a_v2_network_to_mean_anything() {
    let mut hash_network = MAINNET_PARAMS;
    hash_network.palw_capability_bound = Some(ForkActivation::always());
    assert!(
        !hash_network.palw_capability_bound_at(0),
        "a hash-only network has no capability declarations, so the fence must resolve off however it is set"
    );

    let mut v2 = palw_rc_shipped_params();
    assert!(!v2.palw_capability_bound_at(0), "the shipped V2 assembly leaves it off");
    v2.palw_capability_bound = Some(ForkActivation::new(1_000));
    assert!(!v2.palw_capability_bound_at(999), "before the height");
    assert!(v2.palw_capability_bound_at(1_000), "and from it");
}

#[test]
fn the_unavailable_abstains_fence_separates_networks_only_when_it_is_in_force() {
    let shipped = MAINNET_PARAMS;
    assert!(shipped.palw_unavailable_abstains.is_none(), "every shipped preset must leave ADR-0065 D4 dormant");

    let mut armed = MAINNET_PARAMS;
    armed.palw_unavailable_abstains = Some(ForkActivation::new(9_000_000));
    assert_eq!(
        shipped.consensus_identity_id(),
        armed.consensus_identity_id(),
        "scheduling D4 for a future height must keep old and new builds peers — this is the whole reason              it is not in the V2 bundle"
    );
    assert_ne!(
        shipped.consensus_params_id(),
        armed.consensus_params_id(),
        "…while still being a visible commitment: a rule change nobody can observe in the fingerprint is              how two builds fork in silence"
    );

    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_unavailable_abstains = Some(ForkActivation::always());
    assert_ne!(
        shipped.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "in force from block 1 on one side is a rule difference — the two disagree about whether a              quorum of Unavailable takes the producer's stake"
    );

    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_unavailable_abstains = Some(ForkActivation::never());
    assert_eq!(
        shipped.consensus_identity_id(),
        never_armed.consensus_identity_id(),
        "Some(never()) is absence, or the collapse ahead of the dns_params early return is gone"
    );

    // The two fences are independent: arming one must not be readable as arming the other.
    let mut both = MAINNET_PARAMS;
    both.palw_bootstrap_activation = Some(ForkActivation::always());
    both.palw_unavailable_abstains = Some(ForkActivation::always());
    assert_ne!(both.consensus_identity_id(), at_genesis.consensus_identity_id(), "two fences, two identities");
}

/// **ADR-0083 Decision 1's fence keeps the D4 discipline** — the only discipline that lets a rule
/// reach the LIVE network it was measured on: dormant on every shipped preset and therefore
/// absent from the fingerprint, visible in `consensus_params_id` the moment it is scheduled,
/// peers-with-a-warning at a future height, and a change of identity only in force at genesis.
#[test]
fn the_priced_rows_fence_separates_networks_only_when_it_is_in_force() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_difficulty_priced_rows.is_none(), "{name} must leave ADR-0083's fence dormant");
        // Byte-identical to a build without the field: adding the wiring is not a flag day.
        let before = shipped.consensus_params_id();
        let mut probe = shipped.clone();
        probe.palw_difficulty_priced_rows = None;
        assert_eq!(before, probe.consensus_params_id(), "{name}: the Some-only write ran on a None");
        // …and absent from the schedule walk; arming adds exactly one height.
        let walk = |mut p: Params| {
            let mut seen = Vec::new();
            p.for_each_fence(&mut |score| seen.push(*score));
            seen
        };
        let mut armed = shipped.clone();
        armed.palw_difficulty_priced_rows = Some(ForkActivation::new(9_000_000));
        assert_eq!(walk(armed).len(), walk(shipped.clone()).len() + 1, "{name}: arming adds exactly its height to the walk");
    }
    let shipped = MAINNET_PARAMS;

    let mut armed = MAINNET_PARAMS;
    armed.palw_difficulty_priced_rows = Some(ForkActivation::new(9_000_000));
    assert_eq!(
        shipped.consensus_identity_id(),
        armed.consensus_identity_id(),
        "scheduling the rule for a future height must keep old and new builds peers — the live network is where it has to arrive"
    );
    assert_ne!(shipped.consensus_params_id(), armed.consensus_params_id(), "…and still be a visible commitment");
    assert_ne!(shipped.consensus_schedule_id(), armed.consensus_schedule_id(), "the operator log must name it");

    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_difficulty_priced_rows = Some(ForkActivation::always());
    assert_ne!(
        shipped.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "in force from block 1 is a rule difference — every header's bits past genesis may differ"
    );

    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_difficulty_priced_rows = Some(ForkActivation::never());
    assert_eq!(
        shipped.consensus_identity_id(),
        never_armed.consensus_identity_id(),
        "Some(never()) is absence, or the collapse ahead of the dns_params early return is gone"
    );
    // (`never()` still writes its sentinel into the raw params id — the collapse to absence is
    // the identity's, exactly as for every fence beside it; the pins above are `None`.)
}

/// **ADR-0075 SA-1/SA-2's rent fence keeps the D4 discipline**, which is the only discipline
/// that lets a rule reach a LIVE network: dormant on every shipped preset and therefore absent
/// from the fingerprint, visible in `consensus_params_id` the moment it is scheduled, and a
/// change of identity only when it is in force at genesis.
#[test]
fn the_certification_rent_fence_separates_networks_only_when_it_is_in_force() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_certification_rent.is_none(), "{name} must leave ADR-0075's rent fence dormant");
    }
    let shipped = MAINNET_PARAMS;

    let mut armed = MAINNET_PARAMS;
    armed.palw_certification_rent = Some(ForkActivation::new(9_000_000));
    assert_eq!(
        shipped.consensus_identity_id(),
        armed.consensus_identity_id(),
        "scheduling the rent for a future height must keep old and new builds peers — a live network is where this rule has to arrive"
    );
    assert_ne!(
        shipped.consensus_params_id(),
        armed.consensus_params_id(),
        "…and still be a visible commitment: a price nobody can read off the fingerprint is a silent fork"
    );
    assert_ne!(shipped.consensus_schedule_id(), armed.consensus_schedule_id(), "the operator log must name it");

    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_certification_rent = Some(ForkActivation::always());
    assert_ne!(
        shipped.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "in force from block 1 on one side is a rule difference — the two disagree about which objects a block may grade and which may open a chunk group"
    );

    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_certification_rent = Some(ForkActivation::never());
    assert_eq!(
        shipped.consensus_identity_id(),
        never_armed.consensus_identity_id(),
        "Some(never()) is absence, or the collapse ahead of the dns_params early return is gone"
    );

    // Independent of its neighbours: arming the rent must not read as arming D4.
    let mut d4 = MAINNET_PARAMS;
    d4.palw_unavailable_abstains = Some(ForkActivation::always());
    assert_ne!(d4.consensus_identity_id(), at_genesis.consensus_identity_id(), "two fences, two identities");
}

/// **ADR-0081 Decision 3's fence keeps the same discipline, and one more that is specific to
/// it**: arming this changes what every job on the network is CALLED, so a dormant fence has to
/// be indistinguishable from a build that never had the field, and an armed one has to be
/// visible in every commitment an operator can read.
#[test]
fn the_prompt_ids_merkle_fence_is_dormant_and_visible_the_moment_it_is_not() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_prompt_ids_merkle.is_none(), "{name} must leave ADR-0081 Decision 3's fence dormant");
        assert_eq!(
            shipped.palw_prompt_ids_form_at(0),
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            "{name} commits prompts flat at genesis"
        );
        assert_eq!(
            shipped.palw_prompt_ids_form_at(u64::MAX),
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            "{name} commits prompts flat at every height there is — the fence is the only way in"
        );
    }
    let shipped = MAINNET_PARAMS;

    let mut armed = MAINNET_PARAMS;
    armed.palw_prompt_ids_merkle = Some(ForkActivation::new(9_000_000));
    assert_ne!(
        shipped.consensus_params_id(),
        armed.consensus_params_id(),
        "a commitment nobody can read off the fingerprint is a silent fork"
    );
    assert_ne!(shipped.consensus_schedule_id(), armed.consensus_schedule_id(), "the operator log must name it");

    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_prompt_ids_merkle = Some(ForkActivation::always());
    assert_ne!(
        shipped.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "in force from block 1 on one side is a rule difference — the two disagree about what every job on the network is called"
    );

    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_prompt_ids_merkle = Some(ForkActivation::never());
    assert_eq!(
        shipped.consensus_identity_id(),
        never_armed.consensus_identity_id(),
        "Some(never()) is absence, or the collapse ahead of the dns_params early return is gone"
    );

    // Independent of its neighbours: arming this must not read as arming the rent fence.
    let mut rent = MAINNET_PARAMS;
    rent.palw_certification_rent = Some(ForkActivation::always());
    assert_ne!(rent.consensus_identity_id(), at_genesis.consensus_identity_id(), "two fences, two identities");
}

/// **The fence only answers on a `ConsensusV2` network.** The free-prompt lane exists nowhere
/// else, so there is no job whose prompt commitment this rule could re-form — and an armed
/// fence on a non-V2 preset must still answer `Flat` rather than select a form the network has
/// no jobs for.
/// **ADR-0082 Decision 3's fence is dormant on every shipped preset and visible the moment it
/// is not** — the same property the prompt-ids fence holds, for the same reason: arming it
/// changes what a court may do, so a dormant fence must be indistinguishable from a build
/// that never had the field, and an armed one must move every commitment an operator reads.
#[test]
fn the_kary_court_fence_is_dormant_and_visible_the_moment_it_is_not() {
    for (name, shipped) in [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS)] {
        assert!(shipped.palw_kary_court.is_none(), "{name} must leave ADR-0082 Decision 3's fence dormant");
        assert!(!shipped.palw_kary_court_active_at(u64::MAX), "{name}: a dormant fence is never active");
    }
    // **DEVNET arms it at genesis** (the 5f genesis card §1, rehearsed on devnet first): the
    // graph-v5 row is admitted and UNPROSECUTABLE without it. `always()` and not a schedule,
    // because a chain that acquired a dissection court mid-life would be two courts wearing
    // one ruleset id.
    assert_eq!(DEVNET_PARAMS.palw_kary_court, Some(ForkActivation::always()), "devnet arms ADR-0082 Decision 3's fence at genesis");
    // The BUNDLED devnet, not the preset literal: `DEVNET_PARAMS` carries
    // `PalwConsensusMode::Disabled`, and the mode condition is folded into every fence
    // accessor, so the fence only ANSWERS once the bundle is installed. That is the half a
    // preset-literal assertion cannot see.
    let armed_devnet = devnet_shipped_params();
    assert!(matches!(armed_devnet.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)));
    assert!(armed_devnet.palw_kary_court_active_at(0), "the bundled devnet plays the dissection court from genesis");

    // **The RC ARMS it too, and it is not a decision this test can restate.** This half read
    // "the RC card leaves the fence dormant", and that was true while the RC registered no
    // fused row. It registers ADR-0082's graph-v5 512 row now (5f genesis card §2), and
    // `validate_palw_v2` refuses to ASSEMBLE a ruleset that registers a fused row with a
    // dormant court — so on this preset the two are one fact and the assertion below is the
    // one that can be made: the fence is armed exactly when the set needs it.
    let shipped = palw_rc_shipped_params();
    assert!(
        shipped.palw_kary_court_active_at(0),
        "the RC genesis registers a fused-attention row, so its dissection court is armed from block one"
    );
    // The dormancy/visibility property itself, on a ruleset that has NOT armed it — the
    // class-free RC base with the fence taken back off (the preset STATES it since the 5f
    // cut; the property under test is what arming does to the identity, not whether the RC
    // arms), which is what the fence's own doc is about.
    let mut bare = palw_rc_base_params();
    bare.palw_kary_court = None;
    assert!(!bare.palw_kary_court_active_at(u64::MAX), "a ruleset with no fused row leaves the fence dormant");
    let mut armed = shipped.clone();
    armed.palw_kary_court = Some(ForkActivation::new(9_000_000));
    assert_ne!(shipped.consensus_params_id(), armed.consensus_params_id(), "arming the k-ary court must move the fingerprint");
    assert_ne!(shipped.consensus_schedule_id(), armed.consensus_schedule_id(), "the operator log must name it");
    assert!(armed.palw_kary_court_active_at(9_000_000));
    assert!(!armed.palw_kary_court_active_at(8_999_999));
    // A never-arming fence collapses to absence in the IDENTITY (the normalised commitment),
    // like every bare fence beside it; the raw params id is the un-normalised preimage.
    let mut never_armed = bare.clone();
    never_armed.palw_kary_court = Some(ForkActivation::never());
    assert_eq!(
        never_armed.consensus_identity_id(),
        bare.consensus_identity_id(),
        "a never-arming fence collapses to absence, like every bare fence beside it"
    );
    // The mode condition is folded in: outside ConsensusV2 the fence answers nothing.
    let mut legacy = MAINNET_PARAMS.clone();
    legacy.palw_kary_court = Some(ForkActivation::always());
    assert!(matches!(legacy.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::Disabled));
    assert_eq!(legacy.palw_kary_court_fence(), None);
    assert!(!legacy.palw_kary_court_active_at(u64::MAX));
}

/// **ADR-0082 Decision 3: an armed dissection court derives its arity at ASSEMBLY, on the
/// ruleset it will judge under.** Stream I made a failed derivation a refusal to judge; the
/// check in `validate_palw_v2` is the other half — a node must not start on a ruleset whose
/// court has no shape and find out at its first dispute.
///
/// What this test does NOT do is build a bundle the check refuses, and the reason is worth
/// writing down: `PalwConsensusParamsV2::validate` already refuses any window that cannot hold
/// the BINARY court's worst-case honest prosecution ("window_court does not fit the worst-case
/// honest prosecution"), so every bundle that reaches the arity check has a window the binary
/// ladder fits. The derivation can still answer `None` — its rounds include the dissection over
/// the widest REGISTERED fused site, which the binary bound never counted — but only for a
/// catalog with such a site, which no shipped ruleset carries and no hand-built fixture here
/// should pretend to. `palw_court_v2`'s own test pins that `None` is a refusal, never a
/// fallback to two; this one pins that the assembly reads the derivation at all, and that
/// arming the shipped devnet leaves it assemblable.
#[test]
fn an_armed_kary_court_derives_its_shape_at_assembly() {
    use crate::palw_mode_v2::PalwConsensusMode;
    let shipped = devnet_shipped_params();
    shipped.validate_palw_v2().expect("the shipped devnet assembles");
    let PalwConsensusMode::ConsensusV2(bundle) = &shipped.palw_consensus_mode else { panic!("a V2 bundle") };
    let mut armed = shipped.clone();
    armed.palw_kary_court = Some(ForkActivation::always());
    armed.validate_palw_v2().expect("the shipped ruleset's window holds its dispute under the armed fence");
    // The value the assembly accepted is the one the court will play: the derived arity, not
    // the bundle literal's 2.
    let derived = crate::palw_court_v2::palw_court_params_at_v2(bundle, true).expect("derives");
    assert_eq!(bundle.court.dissection_arity(), crate::palw_mode_v2::PALW_COURT_BINARY_ARITY_V1, "every preset literal ships 2");
    assert!(derived.dissection_arity() >= 2, "the armed court plays the derived arity, {}", derived.dissection_arity());
}

/// **Audit D H-1: an armed fence and a bundle that froze a different arity is refused at
/// assembly.**
///
/// `palw_court_params_at_v2` OVERWRITES `dissection_arity` with the derivation's answer, so
/// past the fence the field the ruleset froze (what `palw_ruleset_id_v2` hashes, what a genesis
/// tool writes) and the arity the court deals are two statements about one rule. Nothing
/// compared them: `validate_palw_v2` only checked that a derivation EXISTS. A chain in which
/// they differ admits a fused class priced for one court and prosecutes it in another.
#[test]
fn an_armed_kary_court_refuses_a_ruleset_that_froze_another_arity() {
    use crate::palw_mode_v2::PalwConsensusMode;
    let shipped = devnet_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &shipped.palw_consensus_mode else { panic!("a V2 bundle") };
    let derived = crate::palw_court_v2::palw_court_params_at_v2(bundle, true).expect("derives").dissection_arity();

    // A legal arity that is NOT the one this ruleset derives.
    let other = if derived == 2 { 4 } else { 2 };
    let mut mismatched = shipped.clone();
    mismatched.palw_kary_court = Some(ForkActivation::always());
    let PalwConsensusMode::ConsensusV2(b) = &mut mismatched.palw_consensus_mode else { panic!("a V2 bundle") };
    b.court = b.court.with_dissection_arity(other).expect("a power of two in 2..=64 is legal");
    let err = mismatched.validate_palw_v2().expect_err("a ruleset whose frozen arity is not its derived one must not assemble");
    assert!(format!("{err}").contains("frozen dissection_arity"), "the refusal must name the field: {err}");

    // Dormant, the frozen-arity rule is silent — but devnet's genesis set registers the
    // graph-v5 row (testnet-11's set since the ADR-0082 drill), so the OTHER door refuses the
    // ruleset first: a fused genesis row under a dormant court
    // (`a_fused_genesis_row_is_refused_while_the_kary_court_is_dormant`). The two refusals
    // are different rules with different words; this asserts the dormant fence stops the
    // arity rule from speaking, and that what speaks instead names the fused row.
    let mut dormant = mismatched.clone();
    dormant.palw_kary_court = None;
    let err = dormant.validate_palw_v2().expect_err("a fused genesis row under a dormant court is refused");
    let text = format!("{err}");
    assert!(!text.contains("frozen dissection_arity"), "a dormant fence must silence the arity rule: {text}");
    assert!(text.contains("fused-attention"), "the dormant refusal is the fused-row guard's, by name: {text}");
}

/// **Audit D H-3, the other door: a fused row MINTED at genesis is refused while the court is
/// dormant.**
///
/// `verify_class_admission_v5`'s `FusedAttentionNeedsTheKaryCourt` is the refusal that keeps a
/// class the court cannot try out of the catalog — and it only guards the permissionless
/// registration path. Genesis registrations are verified against the committed catalog and
/// never run the admission gate at all, so a relaunch could mint the dense graph-v5 row with
/// `palw_kary_court` dormant and nothing would refuse: the leaf would be prosecutable only at
/// design A's whole-row width, which is the price the class was never charged for. The walk is
/// the arity derivation's own (`palw_attn_widest_registered_site_v2`), so "does this ruleset
/// register a fused row" is answered once.
#[test]
fn a_fused_genesis_row_is_refused_while_the_kary_court_is_dormant() {
    use crate::palw_mode_v2::PalwConsensusMode;
    use crate::palw_state_v2::{PalwClassAdmissionCarriageV2, PalwConsensusObjectV2, PalwPwuRuleV2};

    let shipped = devnet_shipped_params();
    shipped.validate_palw_v2().expect("the shipped devnet assembles");

    // The smallest fused row this crate can project — a class id nothing else has, and the one
    // property the gate reads: an `AttnFused` node.
    let profile = crate::palw_qwen25_profile::qwen25_a16_profile_v5(crate::palw_qwen25_profile::PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 16,
        ffn_dim: 12,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 4,
        vocab_size: 64,
        n_ctx: 16,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    })
    .expect("the v5 projection is a valid profile");
    let canonical = crate::palw_v2::PalwJobContextV2 {
        version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"misaka-palw-rc".to_vec(),
        job_id: kaspa_hashes::Hash64::default(),
        job_nullifier: kaspa_hashes::Hash64::default(),
        assignment_id: kaspa_hashes::Hash64::default(),
        execution_seed: [0; 32],
        model_profile_id: kaspa_hashes::Hash64::default(),
        runtime_manifest_hash: kaspa_hashes::Hash64::default(),
        runtime_class_id: kaspa_hashes::Hash64::default(),
        shape_profile_id: profile.shape_profile_id(),
        trace_scheme_id: kaspa_hashes::Hash64::default(),
        cu_ruleset_id: kaspa_hashes::Hash64::default(),
        tokenizer_id: kaspa_hashes::Hash64::default(),
        prompt_token_ids_hash: kaspa_hashes::Hash64::default(),
        declared_prefill_tokens: 4,
        exact_decode_tokens: 2,
        max_context_tokens: profile.n_ctx,
    };
    let registration = PalwConsensusObjectV2::ClassRegistered {
        class_id: profile.shape_profile_id(),
        artifact_root: kaspa_hashes::Hash64::from_u64_word(0xA271FAC7),
        slash_value_per_pwu: 1,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 },
        initial_target: 1,
        share_permille: 0,
        activation_daa: 0,
        admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
            profile,
            canonical,
            registrant_bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_u64_word(0x0082_F5ED),
                index: 0,
            }),
            signature: Vec::new(),
        })),
    };

    let mut minted = shipped.clone();
    let PalwConsensusMode::ConsensusV2(bundle) = &mut minted.palw_consensus_mode else { panic!("a V2 bundle") };
    bundle.genesis_objects.push(registration);
    assert!(
        crate::palw_court_v2::palw_attn_widest_registered_site_v2(bundle).0 > 0,
        "the premise: this genesis set now registers a fused site"
    );

    // Armed — which is what the shipped devnet now is — the same genesis set assembles: the row
    // has a court, which is the whole rule.
    let mut armed = minted.clone();
    armed.palw_kary_court = Some(ForkActivation::always());
    armed.validate_palw_v2().expect("a fused genesis row is exactly what an armed k-ary court is for");

    // Dormant, it does not.
    let mut dormant = minted;
    dormant.palw_kary_court = None;
    let err = dormant.validate_palw_v2().expect_err("a fused genesis row under a dormant court must not assemble");
    assert!(format!("{err}").contains("fused-attention"), "the refusal must name the leaf: {err}");
}

/// **Audit D M-1 and M-2: a fence this build cannot carry may not be armed.**
///
/// Both announce a rule the code cannot reach. `palw_fp_decode_rules` arms a numerator the
/// transition answers `FreePromptDecodeLeavesUnavailable` for and a sampler no engine has,
/// while `ChainFacts::fp_decode_rules_armed` makes the gateway accept temperature jobs — a lane
/// that burns a full inference per refusal. (`palw_prompt_ids_merkle` used to sit in this
/// table for the same reason; since the private-prompts design of 2026-09-05 every writer and
/// reader takes the form, and its own rule — genesis-only, named in the ruleset id — is tested
/// in `the_prompt_ids_merkle_fence_is_genesis_only_and_named_in_the_ruleset_id`.) Refusing to
/// ASSEMBLE is the `palw_uncertified_weightless` shape: a configuration that reads as armed and
/// behaves as broken must not start.
#[test]
fn a_fence_whose_rule_this_build_cannot_reach_is_refused_at_assembly() {
    let shipped = devnet_shipped_params();
    shipped.validate_palw_v2().expect("the shipped devnet assembles");

    for (name, arm) in [
        // (`palw_fp_decode_rules` left this table with RFC-0001 §A: this build carries D10, D11
        // and V4 — `the_decode_rules_fence_arms_over_the_derivation_with_its_mirror` holds what
        // arming it still requires.)
        //
        // ADR-0096 Decisions 6–8: no job may carry a constraint, no court can try one, no
        // engine masks one.
        (
            "palw_fp_decode_constraint",
            (|p: &mut Params, a: ForkActivation| p.palw_fp_decode_constraint = Some(a)) as fn(&mut Params, ForkActivation),
        ),
    ] {
        for activation in [ForkActivation::always(), ForkActivation::new(9_000_000)] {
            let mut armed = shipped.clone();
            arm(&mut armed, activation);
            let err = armed.validate_palw_v2().expect_err("{name} must not assemble while its rule is unreachable");
            assert!(format!("{err}").contains(name), "the refusal must name the fence: {err}");
        }
        // `Some(never())` is absence and stays assemblable — the collapse every other fence has.
        let mut never_armed = shipped.clone();
        arm(&mut never_armed, ForkActivation::never());
        never_armed.validate_palw_v2().unwrap_or_else(|e| panic!("{name}: Some(never()) is absence, not an arming: {e}"));
    }
}

/// **RFC-0001 §A / ADR-0082 D10–D11: the decode-rules fence arms — ConsensusV2 only, over the
/// derivation at or below it, with its bundle mirror — and the M-1 refusal is gone.** Over the
/// bundled devnet (no derived work) it is refused naming the derivation; over testnet-12 (derived
/// work at genesis) the flag-day entry arms it at any height and the ruleset assembles; a mirror
/// out of step, or a height written into the bundle's serialized field, is refused by name; and
/// the entry is dormant on every shipped preset, so no shipped id moves.
#[test]
fn the_decode_rules_fence_arms_over_the_derivation_with_its_mirror() {
    // Over a network without the derivation: refused, naming it.
    let mut devnet = devnet_shipped_params();
    (PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1.set)(&mut devnet, Some(ForkActivation::new(9_000_000)));
    let err = format!("{}", devnet.validate_palw_v2().expect_err("no derivation"));
    assert!(err.contains("palw_fp_derived_work"), "{err}");

    // Over testnet-12 as the DAA-3,600 release ships it (the int-11 list, which arms the decode rules at its own height, is
    // dormant on it): the entry arms it, mirror and all, at the flag day's height.
    let shipped = palw_t12_release_v5_params();
    assert!(shipped.palw_fp_decode_rules.is_none(), "dormant on the int-10 release");
    assert_eq!(
        palw_t12_shipped_params().palw_fp_decode_rules,
        PALW_T12_INT11_FLAG_DAY_DAA.map(ForkActivation::new),
        "the int-11 list arms it at H on the shipped ruleset"
    );
    let mut armed = shipped.clone();
    (PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1.set)(&mut armed, Some(ForkActivation::new(1_500)));
    armed.validate_palw_v2().expect("testnet-12 carries the derivation at genesis, so the rules arm");
    assert!(armed.palw_fp_decode_rules_active_at(1_500) && !armed.palw_fp_decode_rules_active_at(1_499));
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &armed.palw_consensus_mode else { panic!("V2") };
    assert_eq!(bundle.state.fp_decode_rules_from_daa(), Some(1_500));
    assert!(bundle.state.fp_decode_rules_at(1_500) && !bundle.state.fp_decode_rules_at(1_499));
    // The mirror is borsh-skipped: the ruleset id — which the identity reads unnormalised — does
    // not move at deploy; the fence itself is what the params and schedule ids name.
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(shipped_bundle) = &shipped.palw_consensus_mode else { panic!("V2") };
    assert_eq!(crate::palw_mode_v2::palw_ruleset_id_v2(bundle), crate::palw_mode_v2::palw_ruleset_id_v2(shipped_bundle));
    assert_eq!(armed.consensus_identity_id(), shipped.consensus_identity_id(), "a scheduled fence keeps its peers until it fires");
    assert_ne!(armed.consensus_params_id(), shipped.consensus_params_id(), "armed, the fingerprint says so");
    assert_ne!(armed.consensus_schedule_id(), shipped.consensus_schedule_id(), "and the operator log names it");

    // The fence without its mirror, and a mirror without its fence: refused by name.
    let mut unmirrored = shipped.clone();
    unmirrored.palw_fp_decode_rules = Some(ForkActivation::new(1_500));
    let err = format!("{}", unmirrored.validate_palw_v2().expect_err("mirror out of step"));
    assert!(err.contains("sync_palw_fp_decode_rules"), "{err}");
    let mut orphan = armed.clone();
    orphan.palw_fp_decode_rules = None;
    let err = format!("{}", orphan.validate_palw_v2().expect_err("mirror without the fence"));
    assert!(err.contains("without palw_fp_decode_rules armed"), "{err}");
    // `Some(never())` is absence.
    let mut never = shipped.clone();
    (PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1.set)(&mut never, Some(ForkActivation::never()));
    never.validate_palw_v2().expect("Some(never()) is absence");
    // Setting it back to dormant is the shipped ruleset, byte for byte.
    let mut back = armed.clone();
    (PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1.set)(&mut back, None);
    assert_eq!(back.consensus_params_id(), shipped.consensus_params_id());
    // No shipped list carries it yet: arming is the integration's one-line addition.
    assert!(
        PALW_T12_POST_LAUNCH_FENCES_V2.iter().chain(PALW_T12_POST_LAUNCH_FENCES_V1).all(|f| f.name != "palw_fp_decode_rules"),
        "FP Job V4 is dormant on this branch"
    );
    assert!(
        shipped.palw_fences_v1().iter().any(|(name, _)| *name == PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1.name),
        "the entry names a real fence"
    );
}

/// **ADR-0100: the one-move court arms only over a bundle that commits to its signing context.**
/// Over the frozen set (every shipped preset) the fence is refused by name and by the set it
/// wants; over a genesis that states COMPLETE_V3 it assembles; over COMPLETE_V2 — a complete
/// set, but not the one that names the accusation's context — it is refused again. The
/// refusal that used to stand here ("the accusation is a type, not an object") is gone with
/// the reason for it, and this is what replaced it.
#[test]
fn the_shard_court_arms_only_over_a_bundle_that_commits_to_its_signing_context() {
    let shipped = devnet_shipped_params();
    shipped.validate_palw_v2().expect("the shipped devnet assembles");
    for activation in [ForkActivation::always(), ForkActivation::new(9_000_000)] {
        let mut armed = shipped.clone();
        armed.palw_shard_court = Some(activation);
        let err = format!("{}", armed.validate_palw_v2().expect_err("the frozen set does not commit to the accuse context"));
        assert!(err.contains("palw_shard_court") && err.contains("COMPLETE_V3"), "the refusal names the fence and the set: {err}");
    }
    let mut never = shipped.clone();
    never.palw_shard_court = Some(ForkActivation::never());
    never.validate_palw_v2().expect("Some(never()) is absence");

    let over = |root: crate::Hash64| -> Params {
        let mut p = shipped.clone();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode else {
            panic!("the shipped devnet is ConsensusV2")
        };
        bundle.signature_contexts_root = root;
        p.palw_signature_contexts_v2 = Some(ForkActivation::always());
        p.palw_shard_court = Some(ForkActivation::new(9_000_000));
        p
    };
    over(crate::palw_mode_v2::palw_v2_signature_contexts_root_v3())
        .validate_palw_v2()
        .expect("a genesis that states the V3 set carries the one-move court");
    let err =
        format!("{}", over(crate::palw_mode_v2::palw_v2_signature_contexts_root_v2()).validate_palw_v2().expect_err("V2 lacks it"));
    assert!(err.contains("palw_shard_court") && err.contains("COMPLETE_V3"), "{err}");
}

/// **ADR-0100 Decision 4: per-shard licensing is dormant everywhere, visible the moment it is
/// not, and arms only over V3 with the one-move court armed no later.**
#[test]
fn the_shard_licensing_fence_is_dormant_and_arms_only_over_v3_with_the_court() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_shard_licensing.is_none(), "{name} must leave ADR-0100 Decision 4 dormant");
        assert!(!shipped.palw_shard_licensing_active_at(u64::MAX), "{name}");
    }
    let shipped = devnet_shipped_params();
    assert!(shipped.palw_shard_licensing.is_none(), "the bundled devnet leaves it dormant too");
    let mut visible = shipped.clone();
    visible.palw_shard_licensing = Some(ForkActivation::new(9_000_000));
    assert_ne!(visible.consensus_params_id(), shipped.consensus_params_id(), "armed, the fingerprint says so");
    let mut never = shipped.clone();
    never.palw_shard_licensing = Some(ForkActivation::never());
    never.validate_palw_v2().expect("Some(never()) is absence");

    // Over the frozen set: refused, naming the set.
    let err = format!("{}", visible.validate_palw_v2().expect_err("the frozen set lacks the plan's context"));
    assert!(err.contains("palw_shard_licensing") && err.contains("COMPLETE_V3"), "{err}");

    let over_v3 = |court: Option<ForkActivation>, licensing: ForkActivation| -> Params {
        let mut p = shipped.clone();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode else {
            panic!("the shipped devnet is ConsensusV2")
        };
        bundle.signature_contexts_root = crate::palw_mode_v2::palw_v2_signature_contexts_root_v3();
        p.palw_signature_contexts_v2 = Some(ForkActivation::always());
        p.palw_shard_court = court;
        p.palw_shard_licensing = Some(licensing);
        p
    };
    // Over V3 without the court, or with the court armed later: refused, naming the court.
    for court in [None, Some(ForkActivation::new(9_000_001))] {
        let err = format!("{}", over_v3(court, ForkActivation::new(9_000_000)).validate_palw_v2().expect_err("no court"));
        assert!(err.contains("palw_shard_court"), "{err}");
    }
    // Over V3 with the court at or below: assembles.
    over_v3(Some(ForkActivation::new(9_000_000)), ForkActivation::new(9_000_000))
        .validate_palw_v2()
        .expect("the court at the same height");
    over_v3(Some(ForkActivation::always()), ForkActivation::new(9_000_000)).validate_palw_v2().expect("the court from genesis");
    // Outside ConsensusV2 the fence answers nothing.
    let mut legacy = MAINNET_PARAMS.clone();
    legacy.palw_shard_licensing = Some(ForkActivation::always());
    assert_eq!(legacy.palw_shard_licensing_fence(), None);
}

/// **ADR-0102: the per-token lift's fence is dormant everywhere, visible the moment it is not,
/// and the kernel it arms is not in the identity.** Shipping the kernel moves no preset's
/// fingerprint (`shipped_presets_have_pinned_fingerprints` is the pin); arming the fence does,
/// and a genesis that registers a class reaching the kernel needs the fence from genesis.
#[test]
fn the_token_lift_fence_is_dormant_visible_when_armed_and_its_kernel_is_outside_the_catalog_root() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_token_lift.is_none(), "{name} must leave ADR-0102 dormant");
        assert!(!shipped.palw_token_lift_active_at(u64::MAX), "{name}");
    }
    let shipped = devnet_shipped_params();
    assert!(shipped.palw_token_lift.is_none(), "the bundled devnet leaves it dormant too");
    let mut visible = shipped.clone();
    visible.palw_token_lift = Some(ForkActivation::new(9_000_000));
    assert_ne!(visible.consensus_params_id(), shipped.consensus_params_id(), "armed, the fingerprint says so");
    visible.validate_palw_v2().expect("the fence alone assembles: it admits a kernel, it needs no other rule");
    let mut never = shipped.clone();
    never.palw_token_lift = Some(ForkActivation::never());
    never.validate_palw_v2().expect("Some(never()) is absence");
    // The kernel is adjudicable and outside the root the bundle commits to.
    assert!(crate::palw_step_refute::fenced_kernel_ids_v1().is_disjoint(&crate::palw_step_refute::catalogued_kernel_ids_v1()));
    // A genesis that registers a v6 row needs the fence from genesis.
    let v6 = crate::palw_qwen36_profile::qwen36_artifact_row_profile_v6(crate::palw_qwen36_profile::PalwQwen36GeometryV1 {
        n_ctx: 512,
        ..crate::palw_qwen36_profile::QWEN35_2B
    })
    .expect("projects");
    let mut with_v6 = shipped.clone();
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut with_v6.palw_consensus_mode else {
        panic!("the shipped devnet is ConsensusV2")
    };
    let Some(template) = bundle
        .genesis_objects
        .iter()
        .find(|o| matches!(o, crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { admission: Some(_), .. }))
        .cloned()
    else {
        panic!("the bundled devnet registers a class with its admission carriage")
    };
    let crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { admission: Some(mut carriage), .. } = template.clone() else {
        unreachable!()
    };
    carriage.profile = v6;
    let crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule,
        initial_target,
        share_permille,
        activation_daa,
        ..
    } = template
    else {
        unreachable!()
    };
    bundle.genesis_objects.push(crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule,
        initial_target,
        share_permille,
        activation_daa,
        admission: Some(carriage),
    });
    assert!(crate::palw_class_admission_v2::palw_genesis_reaches_fenced_kernel_v1(bundle));
    let err = format!("{}", with_v6.validate_palw_v2().expect_err("a v6 genesis row with the fence dormant"));
    assert!(err.contains("palw_token_lift") && err.contains("ADR-0102"), "{err}");
    let mut later = with_v6.clone();
    later.palw_token_lift = Some(ForkActivation::new(1));
    let err = format!("{}", later.validate_palw_v2().expect_err("armed after genesis is still dormant at it"));
    assert!(err.contains("palw_token_lift"), "{err}");
    // Outside ConsensusV2 the fence answers nothing.
    let mut legacy = MAINNET_PARAMS.clone();
    legacy.palw_token_lift = Some(ForkActivation::always());
    assert_eq!(legacy.palw_token_lift_fence(), None);
}

/// **ADR-0093 Decision 6: the dissectable-fused fence is dormant everywhere, visible the moment
/// it is not, and armed from genesis it judges the genesis rows.** Every shipped genesis row
/// with a fused site is dissectable (the dense graph-v5 row: 8-lane tiles over 128-lane heads),
/// so the shipped devnet assembles with the fence armed from genesis; the same row with its
/// fused tile widened to two heads does not.
#[test]
fn the_dissectable_fused_fence_is_dormant_visible_when_armed_and_judges_genesis_from_genesis() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_fused_dissectable.is_none(), "{name} must leave ADR-0093 Decision 6 dormant");
        assert!(!shipped.palw_fused_dissectable_active_at(u64::MAX), "{name}");
    }
    let shipped = devnet_shipped_params();
    assert!(shipped.palw_fused_dissectable.is_none(), "the bundled devnet leaves it dormant too");
    let mut visible = shipped.clone();
    visible.palw_fused_dissectable = Some(ForkActivation::new(9_000_000));
    assert_ne!(visible.consensus_params_id(), shipped.consensus_params_id(), "armed, the fingerprint says so");
    visible.validate_palw_v2().expect("armed later, the fence assembles over any genesis");
    let mut never = shipped.clone();
    never.palw_fused_dissectable = Some(ForkActivation::never());
    never.validate_palw_v2().expect("Some(never()) is absence");
    let mut from_genesis = shipped.clone();
    from_genesis.palw_fused_dissectable = Some(ForkActivation::always());
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &from_genesis.palw_consensus_mode else {
        panic!("the shipped devnet is ConsensusV2")
    };
    assert!(!crate::palw_class_admission_v2::palw_genesis_holds_undissectable_fused_class_v1(bundle));
    from_genesis.validate_palw_v2().expect("the shipped genesis rows are dissectable");
    // A genesis row the fence exists to refuse: a fused row whose output tile is two heads.
    let mut two_heads = crate::palw_context_ladder::palw_a16_context_row_profile_v5(512).expect("projects");
    for node in two_heads.attn_nodes.iter_mut().filter(|n| n.op_kind == crate::palw_step::PalwStepOpKindV1::AttnFused) {
        node.tile_len = 2 * two_heads.attn_head_dim;
    }
    let mut with_two_heads = from_genesis.clone();
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut with_two_heads.palw_consensus_mode else { unreachable!() };
    let template = bundle
        .genesis_objects
        .iter()
        .find(|o| matches!(o, crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { admission: Some(_), .. }))
        .cloned()
        .expect("the bundled devnet registers a class with its admission carriage");
    let crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { admission: Some(mut carriage), .. } = template.clone() else {
        unreachable!()
    };
    carriage.profile = two_heads;
    let crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule,
        initial_target,
        share_permille,
        activation_daa,
        ..
    } = template
    else {
        unreachable!()
    };
    bundle.genesis_objects.push(crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule,
        initial_target,
        share_permille,
        activation_daa,
        admission: Some(carriage),
    });
    assert!(crate::palw_class_admission_v2::palw_genesis_holds_undissectable_fused_class_v1(bundle));
    let err = format!("{}", with_two_heads.validate_palw_v2().expect_err("armed from genesis over a two-head genesis row"));
    assert!(err.contains("palw_fused_dissectable") && err.contains("ADR-0093"), "{err}");
    // Armed past genesis, the genesis rows are the chain's history, not the fence's to judge.
    let mut later = with_two_heads.clone();
    later.palw_fused_dissectable = Some(ForkActivation::new(1));
    later.validate_palw_v2().expect("armed past genesis, genesis rows stand");
    // Outside ConsensusV2 the fence answers nothing.
    let mut legacy = MAINNET_PARAMS.clone();
    legacy.palw_fused_dissectable = Some(ForkActivation::always());
    assert_eq!(legacy.palw_fused_dissectable_fence(), None);
}

/// **ADR-0093 Decision 8: the anchored-root fence is dormant everywhere, visible the moment it is
/// not, and needs the k-ary court at or below it.**
#[test]
fn the_anchored_root_fence_is_dormant_visible_when_armed_and_needs_the_kary_court() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_attn_anchored_root.is_none(), "{name} must leave ADR-0093 Decision 8 dormant");
        assert!(!shipped.palw_attn_anchored_root_active_at(u64::MAX), "{name}");
    }
    let shipped = devnet_shipped_params();
    assert!(shipped.palw_attn_anchored_root.is_none(), "the bundled devnet leaves it dormant too");
    assert!(shipped.palw_kary_court.is_some(), "the bundled devnet runs the k-ary court");
    let mut visible = shipped.clone();
    visible.palw_attn_anchored_root = Some(ForkActivation::new(9_000_000));
    assert_ne!(visible.consensus_params_id(), shipped.consensus_params_id(), "armed, the fingerprint says so");
    visible.validate_palw_v2().expect("armed over the k-ary court, the fence assembles");
    let mut never = shipped.clone();
    never.palw_attn_anchored_root = Some(ForkActivation::never());
    never.validate_palw_v2().expect("Some(never()) is absence");
    let mut courtless = visible.clone();
    courtless.palw_kary_court = Some(ForkActivation::new(9_500_000));
    let err = format!("{}", courtless.validate_palw_v2().expect_err("an anchored root claim of a dissection not yet run"));
    assert!(err.contains("palw_attn_anchored_root") && err.contains("palw_kary_court"), "{err}");
    let mut legacy = MAINNET_PARAMS.clone();
    legacy.palw_attn_anchored_root = Some(ForkActivation::always());
    assert_eq!(legacy.palw_attn_anchored_root_fence(), None);
}

/// **ADR-0103: the held regime's fence is dormant everywhere, visible the moment it is not, and
/// arms only with the k-ary court and the one-move court under it — and, from genesis, only over
/// V4 and the tiled prompt ids** (past genesis the fence commits V4 itself and a held class its
/// own ids, ADR-0118). Invariant 10. Every precondition is refused by name with the others
/// satisfied, so each refusal is that condition's and no other's.
#[test]
fn the_held_context_fence_is_dormant_and_arms_only_over_v4_with_its_courts_and_tiled_ids() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_held_context.is_none(), "{name} must leave ADR-0103 dormant");
        assert!(!shipped.palw_held_context_active_at(u64::MAX), "{name}");
    }
    let shipped = devnet_shipped_params();
    assert!(shipped.palw_held_context.is_none(), "the bundled devnet leaves it dormant too");
    let mut visible = shipped.clone();
    visible.palw_held_context = Some(ForkActivation::new(9_000_000));
    assert_ne!(visible.consensus_params_id(), shipped.consensus_params_id(), "armed, the fingerprint says so");
    assert!(visible.fence_schedule_v1().contains(&9_000_000), "and the schedule names its height");
    let mut never = shipped.clone();
    never.palw_held_context = Some(ForkActivation::never());
    never.validate_palw_v2().expect("Some(never()) is absence");

    // A network that satisfies every precondition: V4 at genesis, the tiled ids, the courts.
    let held = |root: crate::Hash64, merkle: bool, shard_court: Option<u64>, held_at: u64| -> Params {
        let mut p = shipped.clone();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode else {
            panic!("the shipped devnet is ConsensusV2")
        };
        bundle.signature_contexts_root = root;
        if merkle {
            bundle.trace_format_version = crate::palw_mode_v2::PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS;
            p.palw_prompt_ids_merkle = Some(ForkActivation::always());
        }
        p.palw_signature_contexts_v2 = Some(ForkActivation::always());
        p.palw_shard_court = shard_court.map(ForkActivation::new);
        p.palw_panel_da = Some(ForkActivation::always());
        // ADR-0111 Decision 5: the DA court and the free-prompt pins, which the court's horizon
        // widens the pruning depth for.
        p.palw_da_court = Some(ForkActivation::always());
        p.palw_fp_da_pins = Some(ForkActivation::always());
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { unreachable!() };
        let bundle = bundle.clone();
        p = p.with_palw_v2_depths(&bundle);
        p.palw_held_context = Some(ForkActivation::new(held_at));
        p
    };
    let v4 = crate::palw_mode_v2::palw_v2_signature_contexts_root_v4();
    let ok = held(v4, true, Some(100), 200);
    assert!(ok.palw_kary_court.is_some(), "the bundled devnet arms the k-ary court");
    ok.validate_palw_v2().expect("V4, the tiled ids, and both courts at or below the fence");
    held(v4, true, Some(200), 200).validate_palw_v2().expect("the one-move court AT the fence's height is at or below it");
    // The one-move court over V4 is still the one-move court: V4 covers V3.
    let mut court_only = held(v4, true, Some(100), 200);
    court_only.palw_held_context = None;
    court_only.validate_palw_v2().expect("the one-move court arms over V4 as over V3");

    let refused = |p: Params| format!("{}", p.validate_palw_v2().expect_err("a precondition is missing"));
    // The set and the ids are a GENESIS's to state (ADR-0118 Decisions 1 and 3): from genesis a
    // bundle short of V4, or flat ids, is refused; past genesis the fence commits the V4
    // contexts itself and a held class commits its own Merkle ids, so neither is.
    let v3 = crate::palw_mode_v2::palw_v2_signature_contexts_root_v3();
    let err = refused(held(v3, true, Some(0), 0));
    assert!(err.contains("palw_held_context") && err.contains("COMPLETE_V4"), "the set: {err}");
    held(v3, true, Some(100), 200).validate_palw_v2().expect("ADR-0118 D1: past genesis the fence commits V4 itself");
    let err = refused(held(v4, false, Some(0), 0));
    assert!(err.contains("palw_held_context") && err.contains("flat"), "the ids: {err}");
    held(v4, false, Some(100), 200).validate_palw_v2().expect("ADR-0118 D3: past genesis a held class's ids are its own");
    let err = refused(held(v4, true, None, 200));
    assert!(err.contains("palw_held_context") && err.contains("palw_shard_court"), "the one-move court: {err}");
    let err = refused(held(v4, true, Some(300), 200));
    assert!(err.contains("palw_held_context") && err.contains("palw_shard_court"), "the court after the fence: {err}");
    let mut no_kary = held(v4, true, Some(100), 200);
    no_kary.palw_kary_court = None;
    let err = refused(no_kary);
    assert!(err.contains("palw_kary_court"), "the k-ary court: {err}");
    // ADR-0111 Decision 5: a seat that holds no capture demands a leaf through the DA court,
    // and the pins make the retention that demand fits inside a chain fact.
    let mut no_da_court = held(v4, true, Some(100), 200);
    no_da_court.palw_da_court = None;
    let err = refused(no_da_court);
    assert!(err.contains("palw_da_court") && err.contains("ADR-0111"), "the DA court: {err}");
    let mut late_da_court = held(v4, true, Some(100), 200);
    late_da_court.palw_da_court = Some(ForkActivation::new(300));
    let err = refused(late_da_court);
    assert!(err.contains("palw_da_court"), "the DA court after the fence: {err}");
    let mut no_pins = held(v4, true, Some(100), 200);
    no_pins.palw_fp_da_pins = None;
    let err = refused(no_pins);
    assert!(err.contains("palw_fp_da_pins") && err.contains("ADR-0111"), "the pins: {err}");
    let mut no_panel = held(v4, true, Some(100), 200);
    no_panel.palw_panel_da = None;
    let err = refused(no_panel);
    assert!(err.contains("palw_held_context") && err.contains("palw_panel_da"), "the ids off the commitment: {err}");
    let mut late_panel = held(v4, true, Some(100), 200);
    late_panel.palw_panel_da = Some(ForkActivation::new(201));
    let err = refused(late_panel);
    assert!(err.contains("palw_panel_da"), "PanelDa after the fence: {err}");

    // Outside ConsensusV2 the fence answers nothing.
    let mut legacy = MAINNET_PARAMS.clone();
    legacy.palw_held_context = Some(ForkActivation::always());
    assert_eq!(legacy.palw_held_context_fence(), None);
}

/// **ADR-0117: the one-forward draw is dormant on every base preset, Some-only in the identity,
/// and read at the attempt's own height** — and testnet-11 arms it on the audit's flag day
/// (4,000, operator 2026-09-11). Armed, the fingerprint and the schedule say so; the switch is
/// exact at its height; `Some(never())` is absence; and outside `ConsensusV2` there is no
/// attempt lane for it to change.
#[test]
fn the_prefill_draw_fence_is_dormant_and_named_when_armed() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_prefill_draw.is_none(), "{name} must leave ADR-0117 dormant");
        assert!(!shipped.palw_prefill_draw_active_at(u64::MAX), "{name}");
    }
    let shipped = devnet_shipped_params();
    assert!(shipped.palw_prefill_draw.is_none(), "the bundled devnet leaves it dormant too");
    let mut visible = shipped.clone();
    visible.palw_prefill_draw = Some(ForkActivation::new(9_000_000));
    assert_ne!(visible.consensus_params_id(), shipped.consensus_params_id(), "armed, the fingerprint says so");
    assert!(visible.fence_schedule_v1().contains(&9_000_000), "and the schedule names its height");
    assert!(!visible.palw_prefill_draw_active_at(8_999_999), "before its height the draw is the canonical job");
    assert!(visible.palw_prefill_draw_active_at(9_000_000), "at its height, one forward");
    visible.validate_palw_v2().expect("a bare fence has no precondition");
    let mut never = shipped.clone();
    never.palw_prefill_draw = Some(ForkActivation::never());
    never.validate_palw_v2().expect("Some(never()) is absence");
    let mut legacy = MAINNET_PARAMS.clone();
    legacy.palw_prefill_draw = Some(ForkActivation::always());
    assert_eq!(legacy.palw_prefill_draw_fence(), None, "outside ConsensusV2 there is no attempt lane to change");

    let t11 = palw_rc_shipped_params();
    assert_eq!(t11.palw_prefill_draw, Some(ForkActivation::new(PALW_RC_AUDIT_FENCE_DAA)), "testnet-11 arms it at 4,000");
    assert!(!t11.palw_prefill_draw_active_at(PALW_RC_AUDIT_FENCE_DAA - 1), "below 4,000 testnet-11 draws the canonical job");
    assert!(t11.palw_prefill_draw_active_at(PALW_RC_AUDIT_FENCE_DAA), "from 4,000, one forward");
    t11.validate_palw_v2().expect("testnet-11 with the draw fence armed validates");
}

/// **The V2 pruning horizon IS the derivation, and the inherited floor never binds** — the
/// premise `consensus_identity_id`'s normalisation rests on (2026-09-06).
///
/// `with_palw_v2_depths` raises the depth to `palw_v2_pruning_depth_v1` and never lowers it, so
/// a preset whose INHERITED depth is wider than the derivation keeps the inherited value — and
/// the identity's normalisation, which re-derives without the DA term, could not reproduce it.
/// Devnet is in that position (inherited 1,144 against a derivation of 834), which is why the
/// normalisation runs only where the fence is SCHEDULED. This is the guard on that: for every
/// preset that schedules the court, the shipped horizon must be the derivation exactly, so the
/// re-derivation reproduces what the previous build shipped.
#[test]
fn the_v2_pruning_depth_is_the_derivation_on_every_preset_that_schedules_the_court() {
    let mut checked = 0;
    for (name, preset) in [("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &preset.palw_consensus_mode else { continue };
        let Some(fence) = preset.palw_da_court.filter(|f| !f.is_active(0) && *f != ForkActivation::never()) else { continue };
        checked += 1;
        assert_eq!(
            preset.blockrate.pruning_depth,
            palw_v2_pruning_depth_v1(&preset.blockrate, bundle, Some(fence), preset.palw_class_verify_deadline),
            "{name}: the shipped horizon is not the derivation — the identity's normalisation cannot reproduce it"
        );
        // …and the value the normalisation lands on is the derivation WITHOUT the DA term,
        // which is what the previous release shipped.
        let without = palw_v2_pruning_depth_v1(&preset.blockrate, bundle, None, preset.palw_class_verify_deadline);
        assert!(without < preset.blockrate.pruning_depth, "{name}: scheduling the court must widen the horizon");
    }
    assert!(checked > 0, "no shipped preset schedules the DA court — this test would pass by finding nothing");
}

/// **A scheduled DA court moves the fingerprint and NOT the identity — horizon included**
/// (testnet-11's second flag day, 2026-09-06).
///
/// The whole rollout rests on this: two builds that differ only about a future height must
/// stay peers, or the fleet cannot be upgraded one host at a time. The DA court's fence drags a
/// value with it — the pruning horizon widens by `accuse + disclose` — and the horizon is not a
/// fence, so without the normalisation the scheduling build would announce a different network.
/// Four facts: the horizon really does widen, the identity does not move, the fingerprint and
/// the schedule id do, and a court armed at GENESIS separates identities as it must.
/// The attempt-lane e2e root the testnet-11 fleet's build carried: the four-family set (BASE-0,
/// QWEN36, QWEN25-A16, QWEN25-A16-V5), i.e. `PALW_RC_COURT_E2E_ROOT_BYTES` as pinned from
/// ADR-0082's fused graph until the fifth family landed on 2026-09-23.
const PALW_RC_FOUR_FAMILY_COURT_E2E_ROOT_BYTES: [u8; 64] = [
    0xe6, 0x49, 0xe7, 0xc0, 0x8d, 0xe4, 0xe1, 0xdc, 0x3a, 0xbc, 0x3c, 0xa3, 0xb5, 0xab, 0x8e, 0x50, 0x3f, 0x2d, 0x64, 0xb9, 0xfa,
    0x1e, 0x90, 0x9b, 0xfd, 0x7b, 0xaa, 0x18, 0xaf, 0xbf, 0xa6, 0x51, 0x97, 0x11, 0xf7, 0x72, 0x3b, 0xce, 0xa5, 0x35, 0x23, 0xbd,
    0xdc, 0xc8, 0xa5, 0x69, 0x0a, 0xd7, 0x0a, 0x9b, 0x3a, 0xa3, 0x9c, 0xe9, 0x8d, 0x07, 0xd9, 0xc0, 0x39, 0x1d, 0xee, 0x31, 0xde,
    0x4a,
];

#[test]
fn scheduling_the_da_court_moves_the_fingerprint_and_leaves_the_identity_alone() {
    let shipped = palw_rc_shipped_params();
    let fence = shipped.palw_da_court.expect("testnet-11 schedules the DA court");
    assert!(!fence.is_active(0), "…at a height, not at genesis");

    // The same ruleset with the court unscheduled: what the previous release shipped.
    let mut previous = shipped.clone();
    previous.palw_da_court = None;
    previous.palw_panel_da = None;
    previous.palw_model_market = None;
    previous.palw_model_lines = None;
    previous.palw_model_benefits = None;
    previous.palw_model_evm = None;
    // ADR-0114's fence (3500) was scheduled five flag days later, and ADR-0120's (6900) later still.
    previous.palw_model_leg_v2 = None;
    previous.palw_model_seed_v2 = None;
    // The pre-mainnet audit's fences were scheduled later still — 4000 shallow, with ADR-0117's
    // draw fence, and 6000 deep (7000 until 2026-09-17).
    previous.palw_audit_2026_09_11 = None;
    previous.palw_prefill_draw = None;
    previous.palw_audit_2026_09_11_deep = None;
    // …and the held regime's flag day shares 6000 (ADR-0118): the regime, the one-move court and
    // the retention pins arm there together, with the deep fence.
    previous.palw_held_context = None;
    previous.palw_shard_court = None;
    previous.palw_fp_da_pins = None;
    // Scheduled on the same flag day by the 2026-09-06 audit's H-4 (ADR-0084 U-08), so the
    // build the fleet is running does not carry it either.
    previous.palw_court_ladder = None;
    // …and the 6,001 flag day (ADR-0124, 0125, 0126, 0128), later again.
    palw_rc_clear_flag_day_6001_for_tests(&mut previous);
    // **The fleet build pinned the FOUR-family court** (`PALW-QWEN25-A16-V5` the last). The fifth
    // family (`PALW-QWEN36-V6`, 2026-09-23) moved `palw_rc_court_e2e_root_v1` and with it every RC
    // bundle, so the reconstruction states the root that build carried — the bytes the pin held
    // until that day — rather than reading the current pin and drifting with it.
    if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut previous.palw_consensus_mode {
        bundle.court_e2e_root = crate::Hash64::from_bytes(PALW_RC_FOUR_FAMILY_COURT_E2E_ROOT_BYTES);
    }
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &previous.palw_consensus_mode else { panic!("V2") };
    previous.blockrate.pruning_depth =
        palw_v2_pruning_depth_v1(&previous.blockrate, bundle, None, previous.palw_class_verify_deadline);

    // **The "previous" here WAS the build the testnet-11 fleet runs** (`71b35c25…`, the ADR-0083
    // flag day's), and this line pinned it until 2026-09-23. It cannot any more: the economic
    // audit raised `PALW_STATE_V2_VERSION` to 21, a compile-time constant hashed into every state
    // root and the bundle, so no tree past that commit can reconstruct a v20 fleet's ruleset by
    // clearing fields — the five-family court root above was the last difference that could be
    // restated. testnet-11 is superseded by testnet-12 and no node on this build rejoins it, so
    // what remains below is the property this test exists for, stated RELATIVELY within one
    // build: scheduling the court widens the horizon, leaves the identity alone, and moves the
    // fingerprint.
    // From here the claims are about the court FENCE: that scheduling it leaves the identity
    // alone. The family set's move is a separate and deliberate identity move (the 2026-09-23
    // re-pin note in `shipped_presets_have_pinned_fingerprints`), so the root is held equal to
    // the shipped one below — otherwise the assertion would measure the family set, not the fence.
    if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut previous.palw_consensus_mode {
        bundle.court_e2e_root = crate::palw_e2e_adjudicability::palw_rc_court_e2e_root_v1();
    }
    assert!(
        shipped.blockrate.pruning_depth > previous.blockrate.pruning_depth,
        "the premise: scheduling the court widens the horizon ({} vs {})",
        shipped.blockrate.pruning_depth,
        previous.blockrate.pruning_depth
    );
    assert_eq!(
        shipped.consensus_identity_id(),
        previous.consensus_identity_id(),
        "a build that schedules the court must peer with one that does not — the horizon is normalised with the fence"
    );
    assert_ne!(shipped.consensus_params_id(), previous.consensus_params_id(), "…and the fingerprint says the rule is coming");
    assert_ne!(shipped.consensus_schedule_id(), previous.consensus_schedule_id(), "…so the operator log names the height");

    // Armed at genesis, it is a rule in force now: the identity must separate.
    let mut at_genesis = shipped.clone();
    at_genesis.palw_da_court = Some(ForkActivation::always());
    assert_ne!(
        at_genesis.consensus_identity_id(),
        previous.consensus_identity_id(),
        "a court in force from block one is a different network, not a scheduled upgrade"
    );
}

/// **ADR-0062's state-version rule, resolved: a SCHEDULED court needs no version move; a
/// court armed at genesis on a chain that already has one does** (2026-09-06).
///
/// The ADR says "`PALW_STATE_V2_VERSION` must move in the same release that arms the fence",
/// and its Consensus-impact section says why: "Deploying is a re-mint". Both sentences were
/// written for an arming that changes how blocks that ALREADY EXIST are folded. A version move
/// cannot mean that on a live chain: the version is hashed into `state_root`, every header
/// commits `palw_state_root`, so moving it invalidates the chain from block one — the version
/// bump IS the re-mint, not a step towards one.
///
/// A fence scheduled at a future height changes nothing below it. Two builds fold every
/// existing block to the same root, which is what lets the fleet roll one host at a time; past
/// the height the folds differ and the un-upgraded node forks — visibly, because `palw_da_court`
/// now arms the fork-id gate (`fork_id_gate_fences_v1`), which is the mechanism that replaces
/// the version bump's warning. So the rule this test pins is the one that is actually true:
///
/// * armed at GENESIS on a preset with no history (a card) — no version move, nothing to re-fold;
/// * SCHEDULED on a live chain — no version move, and the gate must be armed;
/// * armed at genesis on a preset that carries history — impossible without a re-mint, and the
///   version must move with it.
#[test]
fn a_scheduled_da_court_needs_no_state_version_move_and_a_genesis_one_would() {
    const DA_COURT_AT_GENESIS_STATE_VERSION_FLOOR: u32 = 21;
    for (name, shipped) in [("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        let Some(fence) = shipped.palw_da_court.filter(|a| *a != ForkActivation::never()) else { continue };
        if fence.is_active(0) {
            assert!(
                u32::from(crate::palw_state_v2::PALW_STATE_V2_VERSION) >= DA_COURT_AT_GENESIS_STATE_VERSION_FLOOR,
                "{name} arms palw_da_court at GENESIS on a preset with history at state version {} — that is a re-mint, \
                     and ADR-0062 requires the version to move with it (floor {DA_COURT_AT_GENESIS_STATE_VERSION_FLOOR})",
                crate::palw_state_v2::PALW_STATE_V2_VERSION
            );
        } else {
            // Scheduled: the fold below the height is unchanged, so the version stays — and the
            // fork-id gate must be armed, because that is what tells an un-upgraded peer at the
            // height instead of letting it fork in silence.
            assert!(
                crate::fork_id_v1::fork_id_gate_fences_v1(&shipped).contains(&fence.daa_score()),
                "{name} schedules palw_da_court at {} and the fork-id gate does not name it: an un-upgraded seat would \
                     fork at the height with nothing but a log line",
                fence.daa_score()
            );
        }
    }
    // The card has no history, so it arms at genesis at the current version — the exempt case.
    let card = mainnet_card_base_v1(mainnet_v2_mint_base(), false);
    assert_eq!(card.palw_da_court, Some(ForkActivation::always()));
}

/// **The delay in force outlasts the data-availability court, on every preset that arms it**
/// (ADR-0062; mainnet audit 2026-09-05, "should fix" `palw_mode_v2.rs:938`).
///
/// The bundle's own delay is untouched, so the ruleset id and every identity are what they
/// would be without the fence — the property a rolling upgrade needs. What moves is the value
/// the fold resolves: below the fence the bundle's, past it the bundle's plus `accuse +
/// disclose`. The bundle's own interlock proves the first outlasts the arithmetic court; this
/// proves the second outlasts the DA court, which is the half that interlock cannot see.
///
/// Not a refusal test, deliberately: the resolver adds exactly the lattice the threshold adds,
/// so a `validate_palw_v2` arm for it could never fire — see the comment where that arm is not.
#[test]
fn the_withdrawal_delay_in_force_outlasts_the_data_availability_court() {
    let mut checked = 0;
    for (name, preset) in [("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &preset.palw_consensus_mode else { continue };
        let Some(fence) = preset.palw_da_court.filter(|a| *a != ForkActivation::never()) else { continue };
        checked += 1;
        let liability = bundle.liability_daa().expect("the windows do not overflow");
        let lattice = palw_v2_da_court_lattice_daa(Some(fence), &bundle.state);
        assert!(lattice > 0, "{name}: an armed DA court with no windows is a rule with no phase");

        // The premise: the bundle's own delay is INSIDE `liability + lattice`, so the
        // resolution is doing real work rather than restating a number that already held.
        assert!(
            bundle.bond.withdrawal_delay_daa() <= liability + lattice,
            "{name}: the bundle delay {} already outlasts liability {liability} + lattice {lattice}; this test would pass \
                 for a reason it is not testing",
            bundle.bond.withdrawal_delay_daa()
        );
        // Below the fence: the bundle's own delay, byte for byte — a live chain's history.
        assert_eq!(
            palw_v2_bond_withdrawal_delay_at_v1(bundle, Some(fence), 0),
            if fence.is_active(0) { bundle.bond.withdrawal_delay_daa() + lattice } else { bundle.bond.withdrawal_delay_daa() },
            "{name}: at genesis the resolver answers the fence's own state"
        );
        // Past it: the court's windows ride on top, and that outlasts the whole lattice.
        let above = palw_v2_bond_withdrawal_delay_at_v1(bundle, Some(fence), u64::MAX);
        assert_eq!(above, bundle.bond.withdrawal_delay_daa() + lattice);
        assert!(above > liability + lattice, "{name}: a bond cannot retire before a defaulted claim's verdict lands");
    }
    assert!(checked > 0, "no shipped preset arms the DA court — this test would pass by finding nothing");
}

/// **ADR-0081 Decision 3's fence is genesis-only, and the ruleset id says which form the
/// network hashes its prompts under** (private-prompts design, 2026-09-05).
///
/// Three refusals and one acceptance, each the negation of the other's premise: a height
/// above genesis (no reader of a `prompt_token_ids_hash` holds a job's anchor height), the
/// fence without the format (a ruleset id that does not say what every job is called), the
/// format without the fence (a ruleset id that claims a commitment nothing arms) — and the
/// assembly on a base that arms it, which writes the format itself and validates.
#[test]
fn the_prompt_ids_merkle_fence_is_genesis_only_and_named_in_the_ruleset_id() {
    use crate::palw_mode_v2::{PALW_V2_TRACE_FORMAT_VERSION, PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS};
    let shipped = devnet_shipped_params();
    shipped.validate_palw_v2().expect("the shipped devnet assembles");
    assert_eq!(shipped.palw_prompt_ids_form_v1(), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat);

    let mut scheduled = shipped.clone();
    scheduled.palw_prompt_ids_merkle = Some(ForkActivation::new(9_000_000));
    let err = scheduled.validate_palw_v2().expect_err("a height above genesis is refused");
    assert!(format!("{err}").contains("genesis"), "{err}");

    let mut fence_without_format = shipped.clone();
    fence_without_format.palw_prompt_ids_merkle = Some(ForkActivation::always());
    let err = fence_without_format.validate_palw_v2().expect_err("the fence without trace format 4 is refused");
    assert!(format!("{err}").contains("trace_format_version"), "{err}");

    let mut format_without_fence = shipped.clone();
    if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut format_without_fence.palw_consensus_mode {
        bundle.trace_format_version = PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS;
    }
    let err = format_without_fence.validate_palw_v2().expect_err("trace format 4 without the fence is refused");
    assert!(format!("{err}").contains("dormant"), "{err}");

    // `Some(never())` is absence and stays assemblable — the collapse every other fence has.
    let mut never_armed = shipped.clone();
    never_armed.palw_prompt_ids_merkle = Some(ForkActivation::never());
    never_armed.validate_palw_v2().expect("Some(never()) is absence, not an arming");

    // The assembly on a base that arms it: the format is written from the fence, in one place.
    let mut base = DEVNET_PARAMS;
    base.palw_prompt_ids_merkle = Some(ForkActivation::always());
    let armed = palw_v2_params_from_artifacts_on_base(base, PALW_RC_GENESIS_ARTIFACT_ROOT, palw_devnet_genesis_bonds_v1())
        .expect("a base that arms the Merkle commitment assembles");
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &armed.palw_consensus_mode else { panic!("V2") };
    assert_eq!(bundle.trace_format_version, PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS);
    assert_ne!(PALW_V2_TRACE_FORMAT_VERSION, PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS);
    assert_eq!(armed.palw_prompt_ids_form_v1(), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1);
    assert_eq!(armed.palw_prompt_ids_form_at(u64::MAX), armed.palw_prompt_ids_form_v1(), "genesis-only means height-free");
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(flat) = &shipped.palw_consensus_mode else { panic!("V2") };
    assert_ne!(
        crate::palw_mode_v2::palw_ruleset_id_v2(bundle),
        crate::palw_mode_v2::palw_ruleset_id_v2(flat),
        "the form is inside the ruleset id"
    );
}

/// **ADR-0145 §5/§6's derived-work fence is dormant on every shipped preset, and all four
/// places a fence has to reach are checked here.**
///
/// The four are a set, not a list: the field, the `for_each_fence` arm, the Some-only write
/// into the identity, and the `never()` collapse. Miss the collapse and a build that
/// normalises `Some(never())` writes bytes a build without the field never writes, so the two
/// refuse each other on deploy day over a height neither has reached. Miss the `for_each_fence`
/// arm and a re-pin silently leaves the fence where it was. Both are caught below.
///
/// The sharper reason this fence needs it: past it a free-prompt claim is paid for work the
/// chain DERIVES, so a node that disagreed about the fence would compute a different state
/// root from a block it had accepted.
#[test]
fn the_fp_derived_work_fence_is_dormant_and_visible_the_moment_it_is_not() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_fp_derived_work.is_none(), "{name} must leave ADR-0145 §5/§6 dormant");
        assert!(!shipped.palw_fp_derived_work_active_at(u64::MAX), "{name}: a dormant fence is never active");
    }
    // The ACTIVATION half needs a ConsensusV2 network — the fence folds the mode in, because
    // the derivation reads a class's graph out of chain state and a network with no class
    // state has none to read.
    let shipped = devnet_shipped_params();
    assert!(shipped.palw_fp_derived_work.is_none(), "the bundled devnet leaves it dormant too");
    let mut armed = shipped.clone();
    armed.palw_fp_derived_work = Some(ForkActivation::new(9_000_000));
    assert_ne!(shipped.consensus_params_id(), armed.consensus_params_id(), "arming derived work must move the fingerprint");
    assert_ne!(shipped.consensus_schedule_id(), armed.consensus_schedule_id(), "the operator log must name it");
    assert!(armed.palw_fp_derived_work_active_at(9_000_000));
    assert!(!armed.palw_fp_derived_work_active_at(8_999_999));
    // It assembles: unlike `palw_fp_decode_rules` above, the rule this fence arms is one this
    // build can reach — the transition derives the work, the walk skips a carrier that
    // disagrees, and a class that has published no graph is refused by a name that says what
    // to carry. A fence whose only reachable effect is a refusal is the mistake that one is
    // kept dormant by; this one is not that shape, and the assembly gate agrees.
    // It assembles only WITH its bundle: ADR-0145's three fences COMPOSE, so `validate_palw_v2`
    // refuses any one of them alone (see the bundle rule beside the ordering guards). Arming
    // one is therefore no longer the shape to test; arming the three together is.
    armed.validate_palw_v2().expect_err("one fence of the economic bundle, alone, does not assemble");
    // The bundle's preconditions cascade — registry, panel economy, execution lane, payout,
    // work target — and the SHIPPED RELEASE is the ruleset that already satisfies all of them,
    // so it is the honest base for "can an operator actually arm this". The bundled devnet
    // above leaves the registry dormant and would only prove somebody else's dependency.
    let release = palw_rc_shipped_params();
    let height = release.palw_model_registry.expect("the shipped release arms the registry").daa_score() + 1_000;
    let mut bundled = release.clone();
    bundled.palw_fp_derived_work = Some(ForkActivation::new(height));
    bundled.palw_canonical_work = Some(ForkActivation::new(height));
    bundled.palw_admission_independence = Some(ForkActivation::new(height));
    // ADR-0143, the bundle's other precondition — see the bundle rule's note on why a
    // relabelled copy is not a new model.
    bundled.palw_artifact_root_ownership =
        Some(ForkActivation::new(release.palw_model_registry.expect("the release arms the registry").daa_score()));
    bundled.validate_palw_v2().expect("the derived-work fence is armable by a build that carries its rule, with its bundle");
    assert!(bundled.palw_fp_derived_work_active_at(height));
    // The collapse, observable through `consensus_identity_id`, which is where the normalizer
    // runs (`consensus_params_id` hashes the raw field and never normalizes).
    let mut never_armed = shipped.clone();
    never_armed.palw_fp_derived_work = Some(ForkActivation::never());
    assert_eq!(
        never_armed.consensus_identity_id(),
        shipped.consensus_identity_id(),
        "Some(never()) is absence, or the collapse in normalize_values_a_scheduled_fence_drags_with_it is gone"
    );
    let mut at_genesis = shipped.clone();
    at_genesis.palw_fp_derived_work = Some(ForkActivation::always());
    assert_ne!(
        at_genesis.consensus_identity_id(),
        shipped.consensus_identity_id(),
        "in force from block 1 on one side is a rule difference — the two disagree about what a claim is paid for"
    );
    // And the fence's HEIGHT reaches `for_each_fence`: two builds scheduling it at different
    // heights must be two different fingerprints, or a re-pin would move nothing.
    let mut later = shipped.clone();
    later.palw_fp_derived_work = Some(ForkActivation::new(9_000_001));
    assert_ne!(armed.consensus_params_id(), later.consensus_params_id(), "the height is inside the fingerprint");
}

/// **ADR-0082 Decisions 10/11's fence is dormant on every shipped preset and visible the
/// moment it is not.** The property the two fences above hold, for a sharper reason: this one
/// decides what a free-prompt claim EARNS and which token a class may commit, so a node that
/// disagreed about it would compute a different state root from a block it accepted.
#[test]
fn the_fp_decode_rules_fence_is_dormant_and_visible_the_moment_it_is_not() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_fp_decode_rules.is_none(), "{name} must leave ADR-0082 Decisions 10/11 dormant");
        assert!(!shipped.palw_fp_decode_rules_active_at(u64::MAX), "{name}: a dormant fence is never active");
    }
    // The ACTIVATION half needs a ConsensusV2 network, because the fence folds the mode in and
    // every `Params` const on this branch is `Disabled` (the const is the base identity; what a
    // node runs is the bundled form). `devnet_shipped_params()` is that bundled form.
    let shipped = devnet_shipped_params();
    assert!(matches!(shipped.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)));
    assert!(shipped.palw_fp_decode_rules.is_none(), "the bundled devnet leaves it dormant too");
    let mut armed = shipped.clone();
    armed.palw_fp_decode_rules = Some(ForkActivation::new(9_000_000));
    assert_ne!(shipped.consensus_params_id(), armed.consensus_params_id(), "arming the decode rules must move the fingerprint");
    assert_ne!(shipped.consensus_schedule_id(), armed.consensus_schedule_id(), "the operator log must name it");
    assert!(armed.palw_fp_decode_rules_active_at(9_000_000));
    assert!(!armed.palw_fp_decode_rules_active_at(8_999_999));
    // The collapse is observable through `consensus_identity_id`, which is where the
    // normalizer runs (`consensus_params_id` hashes the raw field and never normalizes).
    let mut never_armed = shipped.clone();
    never_armed.palw_fp_decode_rules = Some(ForkActivation::never());
    assert_eq!(
        never_armed.consensus_identity_id(),
        shipped.consensus_identity_id(),
        "Some(never()) is absence, or the collapse in normalize_values_a_scheduled_fence_drags_with_it is gone"
    );
    let mut at_genesis = shipped.clone();
    at_genesis.palw_fp_decode_rules = Some(ForkActivation::always());
    assert_ne!(
        at_genesis.consensus_identity_id(),
        shipped.consensus_identity_id(),
        "in force from block 1 on one side is a rule difference — the two disagree about what a claim earns"
    );
    // Two fences, two fingerprints: arming ADR-0082's court says nothing about its economics.
    let mut kary_only = shipped.clone();
    kary_only.palw_kary_court = Some(ForkActivation::new(9_000_000));
    assert_ne!(kary_only.consensus_params_id(), armed.consensus_params_id(), "the two ADR-0082 fences are distinct");
    // The mode condition is folded in: outside ConsensusV2 the fence answers nothing.
    let mut legacy = MAINNET_PARAMS.clone();
    legacy.palw_fp_decode_rules = Some(ForkActivation::always());
    assert!(matches!(legacy.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::Disabled));
    assert_eq!(legacy.palw_fp_decode_rules_fence(), None);
    assert!(!legacy.palw_fp_decode_rules_active_at(u64::MAX));
}

/// **ADR-0145's canonical-work fence is dormant on every shipped preset, collapses from
/// `never()`, and is visible the moment it is not.**
///
/// The sharpest reason any fence in this file has: past it the UNIT of `safe_weight` changes
/// from step leaves to MAC-equivalents, and `safe_weight` is fork choice's second key. Two
/// nodes that disagreed about this fence would not disagree about a block — they would agree
/// about every block and disagree about which chain is heavier, which is the failure no
/// validity check catches.
///
/// The `never()` arm is the one that actually bites: the fingerprint writes this field
/// Some-only, so a normalised `Some(never())` left standing would write bytes a build without
/// the field never writes, and the two builds would refuse each other on deploy day over a
/// height neither has reached.
#[test]
fn the_canonical_work_fence_is_dormant_and_visible_the_moment_it_is_not() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_canonical_work.is_none(), "{name} must leave ADR-0145 dormant");
        assert!(!shipped.palw_canonical_work_at(u64::MAX), "{name}: a dormant fence is never active");
        assert_eq!(shipped.palw_canonical_work_daa(), None, "{name}: a dormant fence names no height");
    }
    let shipped = devnet_shipped_params();
    assert!(shipped.palw_canonical_work.is_none(), "the bundled devnet leaves it dormant too");
    assert!(palw_rc_shipped_params().palw_canonical_work.is_none(), "and so does the shipped release");

    let mut armed = shipped.clone();
    armed.palw_canonical_work = Some(ForkActivation::new(9_000_000));
    assert_ne!(shipped.consensus_params_id(), armed.consensus_params_id(), "arming the basis must move the fingerprint");
    assert_ne!(shipped.consensus_schedule_id(), armed.consensus_schedule_id(), "the operator log must name it");
    assert!(armed.palw_canonical_work_at(9_000_000));
    assert!(!armed.palw_canonical_work_at(8_999_999));
    assert_eq!(armed.palw_canonical_work_daa(), Some(9_000_000), "the accounting path reads the HEIGHT, not a flag");

    let mut never_armed = shipped.clone();
    never_armed.palw_canonical_work = Some(ForkActivation::never());
    assert_eq!(
        never_armed.consensus_identity_id(),
        shipped.consensus_identity_id(),
        "Some(never()) is absence, or the collapse in normalize_values_a_scheduled_fence_drags_with_it is gone"
    );
    assert_eq!(never_armed.palw_canonical_work_daa(), None, "never() names no height either");

    // Its own failure domain: this fence is not an alias of the two it sits beside, so a
    // release can schedule any one of them without dragging the others.
    let mut neighbour = shipped.clone();
    neighbour.palw_artifact_root_ownership = Some(ForkActivation::new(9_000_000));
    assert_ne!(neighbour.consensus_params_id(), armed.consensus_params_id(), "three fences, three fingerprints");

    // **The ordering guard fires.** The derived work is read from a class's registry row, so a
    // ruleset that arms the basis above the registry — or without it — is refused rather than
    // booted: a claim priced declared at its `Final` and derived at its retirement leaves a
    // permanent difference in `retired_safe_weight` that no fence position reconciles.
    // Read the registry's own height off the SHIPPED RELEASE rather than typing one in: the
    // registry sits on several fences of its own, and a hand-picked number would trip one of
    // those instead and make this test prove somebody else's rule. The bundled devnet leaves
    // the registry dormant, so the release preset is the one that can exercise this ordering.
    let shipped = palw_rc_shipped_params();
    let registry_daa = shipped.palw_model_registry.expect("the shipped release arms the registry").daa_score();
    // The bundle rule means these heights are probed with all three economic fences armed
    // together; arming `palw_canonical_work` alone is refused for its own separate reason.
    let bundled = |daa: u64| {
        let mut p = shipped.clone();
        p.palw_canonical_work = Some(ForkActivation::new(daa));
        p.palw_admission_independence = Some(ForkActivation::new(daa));
        p.palw_fp_derived_work = Some(ForkActivation::new(daa));
        // ADR-0143 is a precondition of the bundle and the shipped card leaves it commented
        // out, so a build that arms the economy has to arm it too — at the registry's height,
        // which is the lowest the other preconditions already sit at.
        p.palw_artifact_root_ownership = Some(ForkActivation::new(registry_daa));
        p
    };
    let refusal = bundled(registry_daa - 1).validate_palw_v2().expect_err("the basis cannot precede the rows it reads");
    assert!(format!("{refusal:?}").contains("palw_canonical_work"), "{refusal:?}");
    assert!(
        bundled(registry_daa).validate_palw_v2().is_ok(),
        "the same height is legal — the rows exist before any re-priced claim can"
    );
    assert!(bundled(registry_daa + 1_000).validate_palw_v2().is_ok(), "and so is any height above it");
    let mut alone = shipped.clone();
    alone.palw_canonical_work = Some(ForkActivation::new(registry_daa));
    let refusal = alone.validate_palw_v2().expect_err("and one fence of the bundle, alone, is refused");
    assert!(format!("{refusal:?}").contains("arm together or not at all"), "{refusal:?}");
}

/// **ADR-0096 Decisions 6–8's fence is dormant on every shipped preset and visible the
/// moment it is not** — the decode-rules property directly above, for its sharper reason:
/// this fence decides which token a class may commit and what bytes an answer renders to, so
/// a node that disagreed about it would replay a different decode from a block it accepted.
#[test]
fn the_fp_decode_constraint_fence_is_dormant_and_visible_the_moment_it_is_not() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_fp_decode_constraint.is_none(), "{name} must leave ADR-0096 Decisions 6-8 dormant");
        assert!(!shipped.palw_fp_decode_constraint_active_at(u64::MAX), "{name}: a dormant fence is never active");
    }
    let shipped = devnet_shipped_params();
    assert!(matches!(shipped.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)));
    assert!(shipped.palw_fp_decode_constraint.is_none(), "the bundled devnet leaves it dormant too");
    let mut armed = shipped.clone();
    armed.palw_fp_decode_constraint = Some(ForkActivation::new(9_000_000));
    assert_ne!(shipped.consensus_params_id(), armed.consensus_params_id(), "arming the constraint must move the fingerprint");
    assert_ne!(shipped.consensus_schedule_id(), armed.consensus_schedule_id(), "the operator log must name it");
    assert!(armed.palw_fp_decode_constraint_active_at(9_000_000));
    assert!(!armed.palw_fp_decode_constraint_active_at(8_999_999));
    let mut never_armed = shipped.clone();
    never_armed.palw_fp_decode_constraint = Some(ForkActivation::never());
    assert_eq!(
        never_armed.consensus_identity_id(),
        shipped.consensus_identity_id(),
        "Some(never()) is absence, or the collapse in normalize_values_a_scheduled_fence_drags_with_it is gone"
    );
    let mut at_genesis = shipped.clone();
    at_genesis.palw_fp_decode_constraint = Some(ForkActivation::always());
    assert_ne!(
        at_genesis.consensus_identity_id(),
        shipped.consensus_identity_id(),
        "in force from block 1 on one side is a rule difference — the two disagree about which token may be committed"
    );
    // Two fences, two fingerprints: the constraint does not require the sampler and does not
    // alias it (ADR-0096 Decision 8, last sentence).
    let mut rules_only = shipped.clone();
    rules_only.palw_fp_decode_rules = Some(ForkActivation::new(9_000_000));
    assert_ne!(rules_only.consensus_params_id(), armed.consensus_params_id(), "the sampler and the constraint are distinct fences");
    // The mode condition is folded in: outside ConsensusV2 the fence answers nothing.
    let mut legacy = MAINNET_PARAMS.clone();
    legacy.palw_fp_decode_constraint = Some(ForkActivation::always());
    assert!(matches!(legacy.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::Disabled));
    assert_eq!(legacy.palw_fp_decode_constraint_fence(), None);
    assert!(!legacy.palw_fp_decode_constraint_active_at(u64::MAX));
}

/// **ADR-0099 Decision 5's fence is dormant on every shipped preset and visible the moment it
/// is not** — the constraint fence's property directly above, for its reason: this fence
/// decides what convicts, so a node that disagreed about it would slash on an object another
/// node ignores.
#[test]
fn the_shard_court_fence_is_dormant_and_visible_the_moment_it_is_not() {
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        assert!(shipped.palw_shard_court.is_none(), "{name} must leave ADR-0099 Decision 5 dormant");
        assert!(!shipped.palw_shard_court_active_at(u64::MAX), "{name}: a dormant fence is never active");
    }
    let shipped = devnet_shipped_params();
    assert!(shipped.palw_shard_court.is_none(), "the bundled devnet leaves it dormant too");
    let mut armed = shipped.clone();
    armed.palw_shard_court = Some(ForkActivation::new(9_000_000));
    assert_ne!(shipped.consensus_params_id(), armed.consensus_params_id(), "arming the shard court must move the fingerprint");
    assert_ne!(shipped.consensus_schedule_id(), armed.consensus_schedule_id(), "the operator log must name it");
    assert!(armed.palw_shard_court_active_at(9_000_000));
    assert!(!armed.palw_shard_court_active_at(8_999_999));
    let mut never_armed = shipped.clone();
    never_armed.palw_shard_court = Some(ForkActivation::never());
    assert_eq!(never_armed.consensus_identity_id(), shipped.consensus_identity_id(), "Some(never()) is absence");
    let mut at_genesis = shipped.clone();
    at_genesis.palw_shard_court = Some(ForkActivation::always());
    assert_ne!(at_genesis.consensus_identity_id(), shipped.consensus_identity_id(), "in force from block 1 is a rule difference");
    let mut constraint_only = shipped.clone();
    constraint_only.palw_fp_decode_constraint = Some(ForkActivation::new(9_000_000));
    assert_ne!(
        constraint_only.consensus_params_id(),
        armed.consensus_params_id(),
        "the shard court and the constraint are distinct fences"
    );
    let mut legacy = MAINNET_PARAMS.clone();
    legacy.palw_shard_court = Some(ForkActivation::always());
    assert_eq!(legacy.palw_shard_court_fence(), None);
    assert!(!legacy.palw_shard_court_active_at(u64::MAX));
}

#[test]
fn the_prompt_ids_merkle_fence_answers_only_under_consensus_v2() {
    let mut legacy = MAINNET_PARAMS;
    legacy.palw_prompt_ids_merkle = Some(ForkActivation::always());
    assert!(matches!(legacy.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::Disabled));
    assert_eq!(legacy.palw_prompt_ids_merkle_fence(), None, "the mode condition is folded into the fence");
    assert_eq!(legacy.palw_prompt_ids_form_at(u64::MAX), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat);
}

/// **A dormant fence changes nothing an operator can observe — including the schedule id.**
///
/// `consensus_schedule_id` hashes every value `for_each_fence` visits, in sequence. An arm that
/// stands `u64::MAX` in for absence therefore puts eight bytes into that hash on every preset
/// that leaves the field `None`, and two builds whose schedules are IDENTICAL — one carrying
/// the field unset, one built before the field existed — print two different schedule ids. The
/// id exists to answer "do these two agree", and that is the answer it must not get wrong.
///
/// Measured on the walk itself, because "the same as a build without the field" is not a
/// `Params` this test can construct: a dormant fence must contribute NO value to the walk, and
/// arming it must contribute exactly one.
#[test]
fn a_dormant_rent_fence_is_absent_from_the_schedule_walk() {
    let walk = |mut p: Params| {
        let mut seen = Vec::new();
        p.for_each_fence(&mut |score| seen.push(*score));
        seen
    };
    for (name, shipped) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
    {
        let dormant = walk(shipped.clone());
        let mut armed = shipped.clone();
        armed.palw_certification_rent = Some(ForkActivation::new(9_000_000));
        assert_eq!(
            walk(armed).len(),
            dormant.len() + 1,
            "{name}: a dormant rent fence adds nothing to the walk the schedule id hashes; arming it adds exactly its height"
        );
    }

    // …and absence still cannot alias a present value, which is the job the sentinel was
    // doing. Two builds that schedule ONE rule each, at the same height, but not the same
    // rule, must not report the same schedule.
    let mut rent = MAINNET_PARAMS;
    rent.palw_certification_rent = Some(ForkActivation::new(4_242));
    let mut abstains = MAINNET_PARAMS;
    abstains.palw_unavailable_abstains = Some(ForkActivation::new(4_242));
    assert_ne!(
        rent.consensus_schedule_id(),
        abstains.consensus_schedule_id(),
        "a Some-only arm without a NAME would let one scheduled fence read as another"
    );
}

/// **ADR-0075 SA-3: the genesis gate refuses a card whose rows share an operator.**
///
/// `derive_panel_v2` seats one bond per operator id, so a duplicated row is one seat and one
/// dead collateral output — and every seat count this file gates on would be reading a registry
/// larger than the one the draw sees.
#[test]
fn a_card_with_two_rows_sharing_an_operator_is_refused_at_genesis() {
    use crate::palw_mode_v2::PalwConsensusMode;
    use crate::palw_state_v2::PalwConsensusObjectV2 as Obj;

    let params = palw_rc_shipped_params();
    params.validate_palw_v2().expect("the shipped card's operators are distinct");

    let mut doubled = params.clone();
    let PalwConsensusMode::ConsensusV2(bundle) = &mut doubled.palw_consensus_mode else { panic!("the RC is a V2 network") };
    let copy =
        bundle.genesis_objects.iter().find(|o| matches!(o, Obj::BondRegistered { .. })).expect("the card registers bonds").clone();
    // A DIFFERENT bond outpoint, the SAME operator — the shape a card assembled by copying a
    // row takes, and the one the seat counts cannot see.
    let copy = match copy {
        Obj::BondRegistered { pubkey, operator_pubkey, collateral, payout_payload, capable_classes, signature, .. } => {
            Obj::BondRegistered {
                bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(
                    crate::tx::TransactionId::from_u64_word(0xDEAD_BEEF),
                    0,
                )),
                pubkey,
                operator_pubkey,
                collateral,
                payout_payload,
                capable_classes,
                signature,
            }
        }
        other => panic!("{other:?}"),
    };
    bundle.genesis_objects.push(copy);
    let err = doubled.validate_palw_v2().expect_err("a duplicated operator is refused");
    assert!(format!("{err}").contains("operator id"), "the refusal names what it found: {err}");
}

/// **ADR-0062's fence: dormant everywhere, and dormant means byte-identical.**
///
/// The whole reason the DA court is a top-level `Option<ForkActivation>` rather than a bundle
/// field: a build that carries the rule and a build that predates it must be one network until
/// somebody arms it, and arming it in the future must still keep them peers.
#[test]
fn the_da_court_fence_is_dormant_on_every_shipped_preset_and_costs_nothing_while_it_is() {
    for (name, params) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("devnet", DEVNET_PARAMS), ("simnet", SIMNET_PARAMS)]
    {
        assert!(params.palw_da_court.is_none(), "{name}: every shipped preset must leave ADR-0062 dormant");
    }

    let shipped = MAINNET_PARAMS;
    let mut scheduled = MAINNET_PARAMS;
    scheduled.palw_da_court = Some(ForkActivation::new(9_000_000));
    assert_eq!(
        shipped.consensus_identity_id(),
        scheduled.consensus_identity_id(),
        "scheduling the DA court for a future height must keep old and new builds peers — the reason it is \
             not inside the V2 bundle"
    );
    assert_ne!(
        shipped.consensus_params_id(),
        scheduled.consensus_params_id(),
        "…while still being a visible commitment: a rule change nobody can observe in the fingerprint is how \
             two builds fork in silence"
    );
    assert_ne!(
        shipped.consensus_schedule_id(),
        scheduled.consensus_schedule_id(),
        "and the operator log must be able to NAME the disagreement"
    );

    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_da_court = Some(ForkActivation::always());
    assert_ne!(
        shipped.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "in force from block one is a real rule difference: one side accepts two object kinds the other refuses"
    );

    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_da_court = Some(ForkActivation::never());
    assert_eq!(
        shipped.consensus_identity_id(),
        never_armed.consensus_identity_id(),
        "Some(never()) is absence, or the collapse in normalize_values_a_scheduled_fence_drags_with_it is gone"
    );

    // Independent of its neighbours: arming one fence must not be readable as arming another.
    let mut other = MAINNET_PARAMS;
    other.palw_unavailable_abstains = Some(ForkActivation::always());
    assert_ne!(other.consensus_identity_id(), at_genesis.consensus_identity_id(), "two fences, two identities");
}

/// **ADR-0084 U-08's fence has the DA court's contract**: dormant on every shipped preset,
/// free while dormant (a scheduled fence keeps old and new builds peers), a real rule once in
/// force (one side adjudicates a graph-v5 close, the other refuses it by name), and
/// `Some(never())` is absence. And independent of its neighbours.
#[test]
fn the_court_ladder_fence_is_dormant_on_every_shipped_preset_and_costs_nothing_while_it_is() {
    for (name, params) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("devnet", DEVNET_PARAMS), ("simnet", SIMNET_PARAMS)]
    {
        assert!(params.palw_court_ladder.is_none(), "{name}: every shipped preset must leave ADR-0084 U-08 dormant");
        assert!(params.palw_court_ladder_fence().is_none(), "{name}: …and the accessor says so");
        assert!(!params.palw_court_ladder_active_at(u64::MAX), "{name}: never in force while dormant");
    }

    let shipped = MAINNET_PARAMS;
    let mut scheduled = MAINNET_PARAMS;
    scheduled.palw_court_ladder = Some(ForkActivation::new(9_000_000));
    assert_eq!(shipped.consensus_identity_id(), scheduled.consensus_identity_id(), "scheduled keeps the builds peers");
    assert_ne!(shipped.consensus_params_id(), scheduled.consensus_params_id(), "…and is a visible commitment");

    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_court_ladder = Some(ForkActivation::always());
    assert_ne!(shipped.consensus_identity_id(), at_genesis.consensus_identity_id(), "in force from block one is a rule");

    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_court_ladder = Some(ForkActivation::never());
    assert_eq!(shipped.consensus_identity_id(), never_armed.consensus_identity_id(), "Some(never()) is absence");

    let mut other = MAINNET_PARAMS;
    other.palw_da_court = Some(ForkActivation::always());
    assert_ne!(other.consensus_identity_id(), at_genesis.consensus_identity_id(), "two fences, two identities");
}

/// **ADR-0087 Decision 6's fence has the DA court's contract** and is independent of its
/// neighbours.
#[test]
fn the_model_market_fence_is_dormant_on_every_shipped_preset_and_costs_nothing_while_it_is() {
    for (name, params) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("devnet", DEVNET_PARAMS), ("simnet", SIMNET_PARAMS)]
    {
        assert!(params.palw_model_market.is_none(), "{name}: every shipped preset must leave ADR-0087 dormant");
        assert!(!params.palw_model_market_active_at(u64::MAX), "{name}: never in force while dormant");
    }
    let shipped = MAINNET_PARAMS;
    let mut scheduled = MAINNET_PARAMS;
    scheduled.palw_model_market = Some(ForkActivation::new(9_000_000));
    assert_eq!(shipped.consensus_identity_id(), scheduled.consensus_identity_id(), "scheduled keeps the builds peers");
    assert_ne!(shipped.consensus_params_id(), scheduled.consensus_params_id(), "…and is a visible commitment");
    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_model_market = Some(ForkActivation::always());
    assert_ne!(shipped.consensus_identity_id(), at_genesis.consensus_identity_id(), "in force from block one is a rule");
    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_model_market = Some(ForkActivation::never());
    assert_eq!(shipped.consensus_identity_id(), never_armed.consensus_identity_id(), "Some(never()) is absence");
    let mut other = MAINNET_PARAMS;
    other.palw_court_ladder = Some(ForkActivation::always());
    assert_ne!(other.consensus_identity_id(), at_genesis.consensus_identity_id(), "two fences, two identities");
}

/// **ADR-0162's fence is dormant on every shipped preset and moves no id while it is.** On every
/// preset — the four constants, testnet-11's schedule (which arms the market at 1,900) and
/// testnet-12's genesis ruleset (which arms it from genesis) — it is `None`, in force nowhere, and
/// `Some(never())` is absence: the identity and the fork id's schedule are the preset's own.
/// Scheduled at a height it keeps the builds peers until it (the identity does not move) while the
/// params id and the fork id's schedule show it. Its dependency on the market is folded in, and
/// past it the least seed is zero whatever ADR-0120 says; ADR-0114's schedule is untouched.
#[test]
fn the_virtual_reserve_fence_is_dormant_everywhere_and_moves_no_id_while_it_is() {
    use crate::palw_model_market_v1::{PALW_MODEL_SEED_MIN_SOMPI_V1, PALW_MODEL_SEED_MIN_SOMPI_V2};
    let presets = [
        ("mainnet", MAINNET_PARAMS),
        ("testnet", TESTNET_PARAMS),
        ("devnet", DEVNET_PARAMS),
        ("simnet", SIMNET_PARAMS),
        ("testnet-11", palw_rc_shipped_params()),
        ("testnet-12", palw_t12_shipped_params()),
    ];
    for (name, p) in presets {
        assert_eq!(p.palw_model_virtual_v1, None, "{name}: ADR-0162 ships dormant");
        assert_eq!(p.palw_model_virtual_v1_fence(), None, "{name}");
        assert!(!p.palw_model_virtual_v1_active_at(0) && !p.palw_model_virtual_v1_active_at(u64::MAX), "{name}");
        assert_eq!(p.palw_model_virtual_v1_from_at(u64::MAX), None, "{name}");
        assert!(p.palw_fences_v1().contains(&("palw_model_virtual_v1", None)), "{name}: the fence is listed");
        let mut never = p.clone();
        never.palw_model_virtual_v1 = Some(ForkActivation::never());
        assert_eq!(never.consensus_identity_id(), p.consensus_identity_id(), "{name}: Some(never()) is absence");
        assert_eq!(never.fence_schedule_v1(), p.fence_schedule_v1(), "{name}: …to the fork id too");
        let mut scheduled = p.clone();
        scheduled.palw_model_virtual_v1 = Some(ForkActivation::new(9_000_001));
        assert_eq!(scheduled.consensus_identity_id(), p.consensus_identity_id(), "{name}: scheduled keeps the builds peers");
        assert_ne!(scheduled.consensus_params_id(), p.consensus_params_id(), "{name}: …and is a visible commitment");
        assert!(scheduled.fence_schedule_v1().contains(&9_000_001), "{name}: the fork id gates at its height");
    }
    // The market's dependency is folded in: no market, no virtual reserve.
    let mut no_market = MAINNET_PARAMS;
    no_market.palw_model_virtual_v1 = Some(ForkActivation::always());
    assert_eq!(no_market.palw_model_virtual_v1_fence(), None, "meaningless without the market");
    // Over testnet-12's genesis market (ADR-0120's million in force from genesis): past the fence the
    // least seed is zero, ADR-0114's schedule does not move, and the EVM window is told the height.
    let mut armed = palw_t12_shipped_params();
    assert!(armed.palw_model_market_active_at(0) && armed.palw_model_seed_v2_active_at(0), "the premise: t12's market");
    armed.palw_model_virtual_v1 = Some(ForkActivation::new(5_000));
    assert_eq!(armed.palw_model_virtual_v1_fence(), Some(ForkActivation::new(5_000)));
    assert_eq!(armed.palw_model_seed_min_sompi_at(4_999), PALW_MODEL_SEED_MIN_SOMPI_V2, "ADR-0120 below it");
    assert_eq!(armed.palw_model_seed_min_sompi_at(5_000), 0, "ADR-0162 over ADR-0120 past it");
    assert_eq!((armed.palw_model_virtual_v1_from_at(4_999), armed.palw_model_virtual_v1_from_at(5_000)), (None, Some(5_000)));
    assert_eq!(armed.palw_model_fees_at(5_000), armed.palw_model_fees_at(4_999), "the fee schedule is ADR-0114's on both sides");
    assert_eq!(armed.palw_evm_market_fences_at(5_000).virtual_v1_from, Some(5_000));
    assert_eq!(armed.palw_evm_market_fences_at(4_999).virtual_v1_from, None);
    let mut t11 = palw_rc_shipped_params();
    assert_eq!(t11.palw_model_seed_min_sompi_at(0), PALW_MODEL_SEED_MIN_SOMPI_V1, "testnet-11 below its own seed fence");
    t11.palw_model_virtual_v1 = Some(ForkActivation::new(9_000));
    assert_eq!(t11.palw_model_seed_min_sompi_at(9_000), 0, "and the same precedence there");
}

/// **The 2026-09-23 audit fence is testnet-12's alone** (the 2026-09-23 Position route matrix).
/// Every fix that fence carries (P-B4's sell payee, P-B3's lifecycle gate, P-B1's refunds, …)
/// claims "inert on testnet-11" by reading this fence; this pins the premise at the params
/// level, over the networks as a node resolves them from their id, not only the constants: a
/// preset that armed it — even at `always()`, which the fork-id gate-set golden cannot tell from
/// a new height — fails here.
#[test]
fn the_2026_09_23_audit_fence_is_armed_on_testnet_12_and_nowhere_else() {
    for (name, params) in [
        ("testnet-11", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11))),
        ("testnet-11 shipped", palw_rc_shipped_params()),
        ("testnet-10", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 10))),
        ("devnet", Params::from(NetworkId::new(NetworkType::Devnet))),
        ("devnet shipped", devnet_shipped_params()),
        ("mainnet", Params::from(NetworkId::new(NetworkType::Mainnet))),
        ("mainnet shipped", mainnet_shipped_params()),
        ("simnet", Params::from(NetworkId::new(NetworkType::Simnet))),
    ] {
        assert!(params.palw_audit_2026_09_23.is_none(), "{name}: palw_audit_2026_09_23 must stay unset");
        assert!(params.palw_audit_2026_09_23_fence().is_none(), "{name}: no resolved fence");
        assert!(!params.palw_audit_2026_09_23_active_at(0), "{name}: not in force at genesis");
        assert!(!params.palw_audit_2026_09_23_active_at(u64::MAX), "{name}: never in force");
    }
    let t12 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    assert!(t12.palw_audit_2026_09_23_active_at(0), "testnet-12 arms it from genesis");
}

/// **testnet-12's DNS-finality set is mainnet's own, and testnet-11's did not move** (user
/// decisions 2026-09-24 and 2026-09-25): ≥ 6 validators, a 20,000,000 MSK bond each and
/// 120,000,000 MSK of active stake (production's validators × bond relation), the windows at t12's
/// own 120 s cadence (fourteen days of unbonding in wall time, not fourteen days of 10 bps blocks),
/// Decision A's 600-DAA coinbase long maturity, and a set `validate_palw_v2` and the v3 anchor
/// invariants accept. Since 2026-09-25 those four numbers ARE `PRODUCTION_DNS_PARAMS`' (mainnet
/// moved to them; its fingerprint re-pinned), so testnet-12 derives from production with only the
/// work depth substituted, and its set is the same one it was before the move.
#[test]
fn the_testnet_12_dns_set_is_mainnet_assumed_and_nobody_elses_moved() {
    use crate::constants::SOMPI_PER_KASPA;
    let t12 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    let dns = t12.dns_params.as_ref().expect("testnet-12 runs the DNS overlay");
    assert_eq!(dns, &PALW_T12_DNS_PARAMS);
    assert_eq!(dns.min_active_validators, 6);
    assert_eq!(dns.min_bond_amount_sompi, 20_000_000 * SOMPI_PER_KASPA);
    assert_eq!(dns.min_active_stake_sompi, 120_000_000 * SOMPI_PER_KASPA);
    assert_eq!(dns.min_active_stake_sompi, dns.min_bond_amount_sompi * dns.min_active_validators as u64, "validators × bond");
    assert_eq!(dns.coinbase_settlement_long_maturity_daa, 600, "Decision A: the DAA fallback stays");
    // The cadence fields are testnet-11's 120 s table, i.e. production's at two-minute cadence.
    let cadence = PRODUCTION_DNS_PARAMS.at_two_minute_cadence();
    for (name, got, want) in [
        ("epoch_length_blocks", dns.epoch_length_blocks, cadence.epoch_length_blocks),
        ("max_reorg_horizon_blocks", dns.max_reorg_horizon_blocks, cadence.max_reorg_horizon_blocks),
        ("evidence_window_blocks", dns.evidence_window_blocks, cadence.evidence_window_blocks),
        ("unbonding_period_blocks", dns.unbonding_period_blocks, cadence.unbonding_period_blocks),
        ("dns_gate_horizon_blocks", dns.dns_gate_horizon_blocks, cadence.dns_gate_horizon_blocks),
        ("dns_veto_ttl_daa_score", dns.dns_veto_ttl_daa_score, cadence.dns_veto_ttl_daa_score),
        ("attestation_epoch_length_blue_score", dns.attestation_epoch_length_blue_score, cadence.attestation_epoch_length_blue_score),
        ("attestation_lag_blue_score", dns.attestation_lag_blue_score, cadence.attestation_lag_blue_score),
        ("stake_score_window_blue_score", dns.stake_score_window_blue_score, cadence.stake_score_window_blue_score),
    ] {
        assert_eq!(got, want, "{name}");
    }
    // Fourteen days of evidence and unbonding in WALL time at testnet-12's block time.
    let fourteen_days_ms = 14 * 24 * 3_600 * 1_000;
    assert_eq!(t12.target_time_per_block(), 120_000);
    assert_eq!(dns.evidence_window_blocks * t12.target_time_per_block(), fourteen_days_ms);
    assert_eq!(dns.unbonding_period_blocks, dns.evidence_window_blocks + dns.max_reorg_horizon_blocks, "U = E + R");
    assert_eq!(dns.required_work_depth, TESTNET_DNS_PARAMS.required_work_depth, "reachable on a PALW chain");
    assert!(dns.dns_v3_params_consistent(), "the v3 anchor invariants hold");
    t12.validate_palw_v2().expect("testnet-12's ruleset, DNS set included, validates");

    // Mainnet carries the same four numbers (2026-09-25), and testnet-12 is production at the
    // two-minute cadence with ONE substitution, the reachable work depth.
    assert_eq!(PRODUCTION_DNS_PARAMS.min_active_validators, 6);
    assert_eq!(PRODUCTION_DNS_PARAMS.min_bond_amount_sompi, 20_000_000 * SOMPI_PER_KASPA);
    assert_eq!(PRODUCTION_DNS_PARAMS.min_active_stake_sompi, 120_000_000 * SOMPI_PER_KASPA);
    assert_eq!(PRODUCTION_DNS_PARAMS.coinbase_settlement_long_maturity_daa, 600, "Decision A on mainnet too");
    assert_eq!(
        PALW_T12_DNS_PARAMS,
        crate::dns_finality::DnsParams { required_work_depth: TESTNET_DNS_PARAMS.required_work_depth, ..cadence },
        "testnet-12 = production at 120 s, the work depth alone substituted"
    );
    assert_eq!(Params::from(NetworkId::new(NetworkType::Mainnet)).dns_params, Some(PRODUCTION_DNS_PARAMS));
    // Nobody else moved: testnet-11 keeps its table.
    let t11 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11));
    // (The materialized table carries the registered models' VLT cost table, which
    // `with_registered_models` attaches; every other field is the const's.)
    let mut t11_dns = t11.dns_params.clone().expect("testnet-11 runs the DNS overlay");
    t11_dns.vlt.model_cost_table = TESTNET_DNS_PARAMS.vlt.model_cost_table;
    assert_eq!(t11_dns, TESTNET_DNS_PARAMS, "testnet-11's DNS table did not move");
    assert_eq!(t11.dns_params.as_ref().map(|d| (d.min_active_validators, d.min_bond_amount_sompi)), Some((1, 10 * SOMPI_PER_KASPA)));
}

#[test]
fn staged_public_court_has_independent_dormant_schedule_and_block_clock() {
    let t12 = palw_t12_shipped_params();
    assert_eq!(t12.palw_staged_public_court_v1.map(|f| f.daa_score()), Some(18_400));
    for p in [MAINNET_PARAMS, TESTNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS, palw_rc_shipped_params()] {
        assert!(p.palw_staged_public_court_v1.is_none());
        assert!(!p.palw_staged_public_court_v1_active_at(u64::MAX));
        let mut future = p.clone();
        future.palw_staged_public_court_v1 = Some(ForkActivation::new(9_000_000));
        assert_eq!(p.consensus_identity_id(), future.consensus_identity_id());
        assert_ne!(p.consensus_params_id(), future.consensus_params_id());
        future.palw_staged_public_court_v1 = Some(ForkActivation::never());
        assert_eq!(p.consensus_params_id(), future.consensus_params_id());
    }
    let mut p = palw_t12_shipped_params();
    p.palw_staged_public_court_v1 = Some(ForkActivation::new(47));
    p.validate_palw_staged_public_court_v1().unwrap();
    assert!(!p.palw_staged_public_court_v1_active_at(46));
    assert!(p.palw_staged_public_court_v1_active_at(47));
    p.for_each_fence(&mut |height| *height = height.saturating_add(3));
    assert_eq!(p.palw_staged_public_court_v1.unwrap().daa_score(), 50);
    let p = Params { palw_staged_public_court_v1: Some(ForkActivation::new(1)), ..MAINNET_PARAMS };
    assert!(p.validate_palw_staged_public_court_v1().is_err());
}

#[test]
fn g14_semantics_and_commitment_scope_have_independent_dormant_rule_fences() {
    for p in [MAINNET_PARAMS, TESTNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS, palw_rc_shipped_params(), palw_t12_shipped_params()] {
        for scope in [false, true] {
            assert!(p.palw_class_semantics_v1.is_none());
            assert!(p.palw_court_commitment_scope_v1.is_none());
            let mut future = p.clone();
            let field = if scope { &mut future.palw_court_commitment_scope_v1 } else { &mut future.palw_class_semantics_v1 };
            *field = Some(ForkActivation::new(9_000_000));
            assert_eq!(p.consensus_identity_id(), future.consensus_identity_id());
            assert_ne!(p.consensus_params_id(), future.consensus_params_id());
            let mut never = p.clone();
            let field = if scope { &mut never.palw_court_commitment_scope_v1 } else { &mut never.palw_class_semantics_v1 };
            *field = Some(ForkActivation::never());
            assert_eq!(p.consensus_params_id(), never.consensus_params_id());
        }
    }
    let mut p = palw_t12_shipped_params();
    p.palw_class_semantics_v1 = Some(ForkActivation::new(31));
    p.palw_court_commitment_scope_v1 = Some(ForkActivation::new(47));
    p.validate_palw_class_semantics_v1().unwrap();
    p.validate_palw_court_commitment_scope_v1().unwrap();
    assert!(!p.palw_class_semantics_v1_active_at(30));
    assert!(p.palw_class_semantics_v1_active_at(31));
    assert!(!p.palw_court_commitment_scope_v1_active_at(46));
    assert!(p.palw_court_commitment_scope_v1_active_at(47));
    p.for_each_fence(&mut |height| *height = height.saturating_add(3));
    assert_eq!(p.palw_class_semantics_v1.unwrap().daa_score(), 34);
    assert_eq!(p.palw_court_commitment_scope_v1.unwrap().daa_score(), 50);
    let p = Params { palw_class_semantics_v1: Some(ForkActivation::new(1)), ..MAINNET_PARAMS };
    assert!(p.validate_palw_class_semantics_v1().is_err());
    let p = Params { palw_court_commitment_scope_v1: Some(ForkActivation::new(1)), ..MAINNET_PARAMS };
    assert!(p.validate_palw_court_commitment_scope_v1().is_err());
}

#[test]
fn immutable_models_have_an_independent_dormant_fence_and_commit_scheduled_rules() {
    for p in [MAINNET_PARAMS, TESTNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS, palw_rc_shipped_params(), palw_t12_shipped_params()] {
        assert!(p.palw_model_immutable_v1.is_none());
        assert!(!p.palw_model_immutable_v1_active_at(u64::MAX));
        let mut future = p.clone();
        future.palw_model_immutable_v1 = Some(ForkActivation::new(9_000_000));
        assert_eq!(p.consensus_identity_id(), future.consensus_identity_id());
        assert_ne!(p.consensus_params_id(), future.consensus_params_id());
        let mut never = p.clone();
        never.palw_model_immutable_v1 = Some(ForkActivation::never());
        assert_eq!(p.consensus_params_id(), never.consensus_params_id());
    }
    let mut p = palw_t12_shipped_params();
    p.palw_model_immutable_v1 = Some(ForkActivation::new(9_000_000));
    p.validate_palw_model_immutable_v1().unwrap();
    assert!(!p.palw_model_immutable_v1_active_at(8_999_999));
    assert!(p.palw_model_immutable_v1_active_at(9_000_000));
    p.palw_artifact_root_ownership = None;
    assert!(p.validate_palw_model_immutable_v1().is_err());
}

/// **ADR-0088 Decision 11's fence has the same contract as ADR-0087's** and is independent of it.
#[test]
fn the_model_lines_fence_is_dormant_on_every_shipped_preset_and_costs_nothing_while_it_is() {
    for (name, params) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("devnet", DEVNET_PARAMS), ("simnet", SIMNET_PARAMS)]
    {
        assert!(params.palw_model_lines.is_none(), "{name}: every shipped preset must leave ADR-0088 dormant");
        assert!(!params.palw_model_lines_active_at(u64::MAX), "{name}: never in force while dormant");
    }
    let shipped = MAINNET_PARAMS;
    let mut scheduled = MAINNET_PARAMS;
    scheduled.palw_model_lines = Some(ForkActivation::new(9_000_000));
    assert_eq!(shipped.consensus_identity_id(), scheduled.consensus_identity_id(), "scheduled keeps the builds peers");
    assert_ne!(shipped.consensus_params_id(), scheduled.consensus_params_id(), "…and is a visible commitment");
    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_model_lines = Some(ForkActivation::always());
    assert_ne!(shipped.consensus_identity_id(), at_genesis.consensus_identity_id(), "in force from block one is a rule");
    let mut never_armed = MAINNET_PARAMS;
    never_armed.palw_model_lines = Some(ForkActivation::never());
    assert_eq!(shipped.consensus_identity_id(), never_armed.consensus_identity_id(), "Some(never()) is absence");
    let mut market = MAINNET_PARAMS;
    market.palw_model_market = Some(ForkActivation::always());
    assert_ne!(
        market.consensus_identity_id(),
        at_genesis.consensus_identity_id(),
        "the market's fence and the registry's are two fences"
    );
}

/// **ADR-0089 Decision 9's fence: dormant everywhere, and refused without its two
/// preconditions** — the market armed at or before it, and the EVM lane active by then.
#[test]
fn the_model_evm_fence_is_dormant_and_needs_the_market_and_an_active_lane() {
    for (name, params) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("devnet", DEVNET_PARAMS), ("simnet", SIMNET_PARAMS)]
    {
        assert!(params.palw_model_evm.is_none(), "{name}: every shipped preset must leave ADR-0089 dormant");
        assert!(!params.palw_model_evm_active_at(u64::MAX), "{name}: never in force while dormant");
    }
    let mut alone = TESTNET_PARAMS;
    alone.palw_model_evm = Some(ForkActivation::new(1_000));
    assert!(
        matches!(alone.validate_palw_v2(), Err(crate::palw_mode_v2::PalwModeV2Error::ModelEvmBeforeMarket)),
        "an EVM face of a market that does not exist is refused"
    );
    let mut late_market = TESTNET_PARAMS;
    late_market.palw_model_market = Some(ForkActivation::new(2_000));
    late_market.palw_model_evm = Some(ForkActivation::new(1_000));
    assert!(matches!(late_market.validate_palw_v2(), Err(crate::palw_mode_v2::PalwModeV2Error::ModelEvmBeforeMarket)));
    let mut inert_lane = MAINNET_PARAMS;
    inert_lane.palw_model_market = Some(ForkActivation::new(1_000));
    inert_lane.palw_model_evm = Some(ForkActivation::new(1_000));
    assert!(
        matches!(inert_lane.validate_palw_v2(), Err(crate::palw_mode_v2::PalwModeV2Error::ModelEvmOnInertLane)),
        "mainnet's lane is u64::MAX: the face waits for ADR-0020's activation"
    );
    let mut scheduled = TESTNET_PARAMS;
    scheduled.palw_model_market = Some(ForkActivation::new(1_000));
    scheduled.palw_model_evm = Some(ForkActivation::new(1_000));
    assert!(!matches!(
        scheduled.validate_palw_v2(),
        Err(crate::palw_mode_v2::PalwModeV2Error::ModelEvmBeforeMarket | crate::palw_mode_v2::PalwModeV2Error::ModelEvmOnInertLane)
    ));
    let shipped = TESTNET_PARAMS;
    assert_eq!(shipped.consensus_identity_id(), scheduled.consensus_identity_id(), "scheduled keeps the builds peers");
    assert_ne!(shipped.consensus_params_id(), scheduled.consensus_params_id(), "…and is a visible commitment");
    let fences = scheduled.palw_evm_market_fences_at(1_000);
    assert_eq!(
        (fences.market_active, fences.lines_active, fences.evm_active),
        (scheduled.palw_model_market_active_at(1_000), false, scheduled.palw_model_evm_active_at(1_000))
    );
}

/// **ADR-0089 Decision 9: below the fence there is no window to build** (mainnet audit
/// 2026-09-06, M-13).
///
/// Decision 9 says the four addresses and the facades are empty accounts below the fence, and
/// `Consensus::palw_evm_view_v1` resolved these fences and then built the whole flattening
/// anyway, for every `eth_call` and `eth_estimateGas`, on every ConsensusV2 network — cheap
/// only because the tables happen to be empty.
///
/// The rule is `any_active()`, asserted here at the layer that decides it: on every shipped
/// preset, at every score, there is no window; and arming ANY ONE of the three — the market's
/// reads are ADR-0087's fence, the registry's F010–F012 are ADR-0088's, the facades are
/// ADR-0089's — is enough to need one.
#[test]
fn no_evm_window_exists_below_all_three_model_fences() {
    for (name, params) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("devnet", DEVNET_PARAMS), ("simnet", SIMNET_PARAMS)]
    {
        for daa in [0u64, 1, 1_000, 9_000_000, u64::MAX] {
            assert!(
                !params.palw_evm_market_fences_at(daa).any_active(),
                "{name} at {daa}: every shipped preset leaves all three model fences dormant, so no window is ever built"
            );
        }
    }

    // Each fence alone is enough — none of the three is subsumed by another. Armed on a
    // ConsensusV2 preset, because all three accessors fold the mode condition in: on a network
    // with no V2 bundle there is no market to have a window onto.
    // The RC preset is the one build in the tree that SCHEDULES all three, and it is the case
    // that matters: below its own activation it must have no window either, or a scheduled
    // build pays for a market that does not exist yet.
    let rc = palw_rc_shipped_params();
    if rc.palw_model_market.is_some() {
        let at = PALW_RC_DA_COURT_FENCE_DAA;
        assert!(!rc.palw_evm_market_fences_at(at - 1).any_active(), "a scheduled build builds no window before its own score");
        assert!(rc.palw_evm_market_fences_at(at).any_active(), "…and one from it");
    }

    let mut v2 = rc.clone();
    v2.palw_model_market = None;
    v2.palw_model_lines = None;
    v2.palw_model_evm = None;
    assert!(!v2.palw_evm_market_fences_at(u64::MAX).any_active(), "the dormant base of the three arms below");

    let mut market = v2.clone();
    market.palw_model_market = Some(ForkActivation::new(1_000));
    assert!(!market.palw_evm_market_fences_at(999).any_active(), "and not one block early");
    assert!(market.palw_evm_market_fences_at(1_000).any_active(), "the market's own reads need the window");

    let mut lines = v2.clone();
    lines.palw_model_lines = Some(ForkActivation::new(1_000));
    assert!(!lines.palw_evm_market_fences_at(999).any_active());
    assert!(lines.palw_evm_market_fences_at(1_000).any_active(), "the registry reads F010-F012 need it too");

    let mut evm = v2.clone();
    evm.palw_model_market = Some(ForkActivation::new(1_000));
    evm.palw_model_evm = Some(ForkActivation::new(1_000));
    assert!(evm.palw_evm_market_fences_at(1_000).any_active());

    // `any_active` is the disjunction and nothing more — spelled once so the consensus path and
    // the RPC path cannot answer it differently.
    for bits in 0..8u8 {
        let f = crate::evm::model_market::PalwEvmMarketFencesV1 {
            market_active: bits & 1 != 0,
            lines_active: bits & 2 != 0,
            evm_active: bits & 4 != 0,
            // ADR-0114 moves what a quote says, never whether a window exists; nor does ADR-0120.
            leg_v2_active: bits & 1 != 0,
            seed_v2_active: bits & 2 != 0,
            // Nor does the 2026-09-23 audit: it changes what `market().exists` means (P10).
            audit_2026_09_23_active: bits & 4 != 0,
            // Nor does ADR-0162: it changes what a quote and `constants()` say.
            virtual_v1_from: (bits & 1 != 0).then_some(1_000),
        };
        assert_eq!(f.any_active(), bits != 0, "one window iff some fence is in force");
    }
}

/// **ADR-0062 SA-6: the lattice with the DA phase in it, at every preset that has a lattice.**
///
/// A preset with no V2 bundle has no claim lattice at all, so the bound is vacuous there; the
/// ones that DO carry a bundle are checked by construction — `with_palw_v2_depths` raises the
/// pruning horizon to hold the lattice and `validate_palw_v2` refuses a set where it does not.
/// What this pins is that the DA term is (a) zero while the fence is dormant, so no shipped
/// preset's depths moved by this ADR existing, and (b) actually carried into both bounds once
/// the fence is armed, so arming it cannot silently produce a phase that outlives the horizon
/// its evidence is anchored in.
#[test]
fn the_da_court_phase_fits_the_pruning_horizon_at_every_preset_that_has_a_lattice() {
    let shipped = palw_rc_shipped_params();
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &shipped.palw_consensus_mode else {
        panic!("the RC preset must carry a V2 bundle for this bound to mean anything");
    };
    let bundle = bundle.clone();

    // (a) Dormant costs nothing — the same params, with and without the field considered.
    assert_eq!(palw_v2_da_court_lattice_daa(None, &bundle.state), 0, "a dormant fence adds no DAA to the lattice");
    let redriven = shipped.clone().with_palw_v2_depths(&bundle);
    assert_eq!(
        redriven.blockrate.pruning_depth, shipped.blockrate.pruning_depth,
        "re-deriving the depths of a dormant preset must reproduce them exactly, or this ADR moved a fingerprint"
    );

    // (b) Armed, the term is `accuse + disclose` and BOTH bounds carry it.
    let da = palw_v2_da_court_lattice_daa(Some(ForkActivation::always()), &bundle.state);
    assert_eq!(
        da,
        crate::palw_state_v2::palw_da_accuse_window_daa_v1(&bundle.state)
            + crate::palw_state_v2::palw_da_disclose_window_daa_v1(&bundle.state),
        "SA-6's term is the amendment's literal accuse + disclose"
    );
    assert!(da > 0, "an armed DA court that adds nothing to the lattice is a rule with no window");

    let mut armed = shipped.clone();
    armed.palw_da_court = Some(ForkActivation::always());
    // A preset whose depths were derived WITHOUT the DA term does not silently pass: arming is
    // a re-derivation, which is the honest statement that it is a re-mint and not a flag flip.
    // The bond delay needs no re-derivation: it is resolved at each block
    // (`palw_v2_bond_withdrawal_delay_at_v1`), which is what keeps the ruleset id unmoved while
    // the fence is scheduled.
    let re_derived = armed.clone().with_palw_v2_depths(&bundle);
    assert!(re_derived.blockrate.pruning_depth >= shipped.blockrate.pruning_depth, "arming the DA court may only widen the horizon");
    let lattice = 2 * (bundle.state.window_bind() + bundle.state.window_receipt())
        + bundle.state.window_challenge()
        + bundle.state.window_court()
        + da;
    assert!(
        lattice <= re_derived.blockrate.pruning_depth,
        "the DA phase outlives the pruning horizon: lattice {lattice} > pruning_depth {}",
        re_derived.blockrate.pruning_depth
    );
    re_derived.validate_palw_v2().expect("a re-derived armed preset must validate");

    // SA-3's own inequality, which `validate_palw_v2` proves past the fence: the disclose
    // window is at least twice the finality window, so a reorg across the deadline cannot flip
    // the verdict without a finality violation.
    assert!(
        crate::palw_state_v2::palw_da_disclose_window_daa_v1(&bundle.state) >= 2 * re_derived.blockrate.finality_depth,
        "W_disclose must be at least twice the finality window (SA-3)"
    );
    let mut narrow = re_derived.clone();
    narrow.blockrate.finality_depth = crate::palw_state_v2::palw_da_disclose_window_daa_v1(&bundle.state);
    assert!(narrow.validate_palw_v2().is_err(), "a preset whose finality window outruns the disclose window is refused");
}

/// **Every fence in the fingerprint is normalised, including the nested ones** (re-audit R-2).
///
/// `dns_params` enters `consensus_params_id` as one borsh blob, so `dns.vlt.*` and `dns.tkn.*`
/// are in the fingerprint just as much as the top-level scores are. The hand-written
/// normalisation list missed them, which meant scheduling the VLT shadow fork — the upgrade
/// `TESTNET_VLT_SHADOW_FORK_DAA_SCORE` exists for, and the one this file documents as the next
/// mainnet fork — still partitioned the network at deploy time. That is the exact defect the
/// identity split was written to remove.
/// **The PALW state version reaches the IDENTITY, not just the params id.**
///
/// This is what makes a re-mint a clean flag day rather than a contamination risk: a node on
/// the old ruleset and a node on the new one are refused at the handshake
/// (`flow_context` returns `WrongConsensusParams` on an identity mismatch), so an un-upgraded
/// peer cannot feed its chain to an upgraded one. It holds because the version is not a fence,
/// so `consensus_identity_id`'s normalisation leaves it alone — but "not a fence" is a property
/// of a list that changes, so it is asserted rather than assumed.
#[test]
fn the_palw_state_version_moves_the_identity_and_not_only_the_params_id() {
    let today = Params::from(crate::network::NetworkId::with_suffix(NetworkType::Testnet, 11));
    let identity = today.consensus_identity_id();
    let params_id = today.consensus_params_id();

    // Every fence is normalised out of the identity — that is M1-6's property, and it is what
    // makes this test meaningful rather than trivially true of everything.
    let mut scheduled = today.clone();
    scheduled.for_each_fence(&mut |score| {
        if *score == 0 {
            *score = 7_000_000;
        }
    });
    assert_ne!(scheduled.consensus_params_id(), params_id, "moving a fence is visible in the params id");

    // The version is NOT a fence, so it must survive into the identity. If a future edit routes
    // it through `for_each_fence`, two rulesets would peer while disagreeing about the state
    // root, and a re-mint would stop being a clean break.
    let mut relabelled = today.clone();
    relabelled.for_each_fence(&mut |score| *score = if *score == 0 { 0 } else { u64::MAX });
    assert_eq!(
        relabelled.consensus_identity_id(),
        identity,
        "normalising the fences by hand reproduces the identity — so the identity is exactly the fence-normalised params id"
    );
    assert_ne!(identity, params_id, "and testnet-11 schedules something, so the two ids are genuinely different values");
}

/// **The identity must survive the MATERIALIZED release cut, not the const one** (audit3 H1).
///
/// `scheduling_any_fence_leaves_the_identity_alone` below builds its VLT case from the raw
/// const, so `with_registered_models` never runs, both sides keep an empty model table, and the
/// assertion passes on a preset no node would ever report. What a node runs is the value that
/// went through `From<NetworkId>` — and there the table is installed the moment the fence stops
/// being `u64::MAX`, because the build gate refuses a scheduled fence over an empty table. The
/// four `ModelCostEntry`s are inside `DnsParams`, `consensus_params_id` hashes that as one
/// borsh blob, and the fence was normalised out of the identity while the entries it drags
/// along were not.
///
/// That is a deploy-time partition on the one upgrade this file documents as the next mainnet
/// fork: `flow_context` refuses on an identity mismatch, so the first operator to cut the
/// release cannot connect to a single un-upgraded peer, about a height that is still in the
/// future and about which the two agree completely.
///
/// This is the same trap `shipped_presets_have_pinned_fingerprints` records: pin what is
/// MATERIALIZED, because the const is a number no node ever reports.
#[test]
fn a_materialized_release_cut_keeps_the_identity_it_had() {
    const H: u64 = 9_000_000;
    let dormant = with_registered_models(MAINNET_PARAMS);
    assert_eq!(dormant.dns_params.unwrap().vlt.model_cost_table.len, 0, "the shipped preset is dormant and carries no table");

    let mut scheduled = MAINNET_PARAMS;
    scheduled.dns_params.as_mut().unwrap().vlt.vlt_shadow_activation_daa_score = H;
    let scheduled = with_registered_models(scheduled);
    assert_eq!(
        scheduled.dns_params.unwrap().vlt.model_cost_table.len,
        4,
        "materializing a scheduled shadow fence installs the registered models — that is the whole point of the function"
    );

    let dormant = with_registered_models(MAINNET_PARAMS);
    let scheduled = {
        let mut p = MAINNET_PARAMS;
        p.dns_params.as_mut().unwrap().vlt.vlt_shadow_activation_daa_score = H;
        with_registered_models(p)
    };
    assert_ne!(
        dormant.consensus_params_id(),
        scheduled.consensus_params_id(),
        "an old peer must still be able to SEE that the schedule changed"
    );
    assert_eq!(
        dormant.consensus_identity_id(),
        scheduled.consensus_identity_id(),
        "…and must still peer with it, because the two agree about every block either can produce today"
    );

    // The same for the token lane, whose fences cannot be armed without freezing the emission
    // constants that are TBD while it is inert.
    let mut tkn = MAINNET_PARAMS;
    {
        let d = tkn.dns_params.as_mut().unwrap();
        d.tkn.tkn_shadow_activation_daa_score = H;
        d.tkn.emission_epoch_budget_r0_atomic = 123_456_789;
    }
    let tkn = with_registered_models(tkn);
    assert_ne!(dormant.consensus_params_id(), tkn.consensus_params_id(), "the schedule is visible");
    assert_eq!(dormant.consensus_identity_id(), tkn.consensus_identity_id(), "and it is not a partition");
}

#[test]
fn scheduling_any_fence_leaves_the_identity_alone() {
    let today = MAINNET_PARAMS;
    const H: u64 = 9_000_000;

    // One case per fence that a release cut can realistically arm. Each must move the params id
    // (old peers still notice) and leave the identity alone (new peers stay peers).
    let mut cases: Vec<(&str, Params)> = Vec::new();
    let mut p = MAINNET_PARAMS;
    p.dns_params.as_mut().unwrap().vlt.vlt_shadow_activation_daa_score = H;
    cases.push(("vlt shadow (ADR-0024 step 3)", p));
    let mut p = MAINNET_PARAMS;
    p.dns_params.as_mut().unwrap().vlt.vlt_activation_daa_score = H;
    cases.push(("vlt weight (step 4)", p));
    let mut p = MAINNET_PARAMS;
    p.dns_params.as_mut().unwrap().tkn.tkn_activation_daa_score = H;
    cases.push(("tkn", p));
    let mut p = MAINNET_PARAMS;
    p.dns_params.as_mut().unwrap().tkn.emission_activation_epoch = 42;
    cases.push(("tkn emission epoch", p));
    let mut p = MAINNET_PARAMS;
    p.dns_params.as_mut().unwrap().mandatory_attestation_inclusion_daa_score = H;
    cases.push(("mandatory attestation inclusion", p));
    let mut p = MAINNET_PARAMS;
    p.dns_params.as_mut().unwrap().coinbase_settlement_consensus_activation_daa_score = H;
    cases.push(("coinbase settlement consensus", p));
    let mut p = MAINNET_PARAMS;
    p.evm_activation_daa_score = H;
    cases.push(("evm", p));
    let mut p = MAINNET_PARAMS;
    p.crescendo_activation = ForkActivation::new(H);
    cases.push(("crescendo", p));

    for (name, scheduled) in cases {
        assert_ne!(today.consensus_params_id(), scheduled.consensus_params_id(), "{name}: the legacy fingerprint must move");
        assert_eq!(
            today.consensus_identity_id(),
            scheduled.consensus_identity_id(),
            "{name}: scheduling this fence moved the IDENTITY, so rolling it out would disconnect every \
                 upgrading node from every un-upgraded one for the whole rollout — the defect M1-6 exists to remove"
        );
        assert_ne!(today.consensus_schedule_id(), scheduled.consensus_schedule_id(), "{name}: the operator log must name it");
    }
}

/// **ADR-0095's fence is inside the fingerprint and outside the identity** — the property
/// 14c453d1 broke on the first half and never touched on the second.
///
/// testnet-11 schedules `palw_model_benefits` at DAA 2,400. Until this was fixed the printed
/// fingerprint did not write it, so the build that crossed the fence on 2026-09-09 and the build
/// without it both printed `060e3597…`. Writing it must move the fingerprint (moving the height
/// moves it too) and must NOT move the identity: a scheduled fence is normalised to `None`
/// there, which writes nothing — so the fixed build and the one before it stay peers, and the
/// handshake reports a schedule difference instead of refusing.
#[test]
fn the_benefits_fence_is_inside_the_fingerprint_and_outside_the_identity() {
    let shipped = palw_rc_shipped_params();
    let fence = shipped.palw_model_benefits.expect("testnet-11 schedules ADR-0095's fence");
    assert_eq!(fence.daa_score(), PALW_RC_MODEL_BENEFITS_FENCE_DAA);

    let mut moved = shipped.clone();
    moved.palw_model_benefits = Some(ForkActivation::new(PALW_RC_MODEL_BENEFITS_FENCE_DAA + 1));
    let mut unscheduled = shipped.clone();
    unscheduled.palw_model_benefits = None;

    for (what, other) in [("moving the height by one", &moved), ("unscheduling it", &unscheduled)] {
        assert_ne!(shipped.consensus_params_id(), other.consensus_params_id(), "{what} must move the printed fingerprint");
        assert_eq!(
            shipped.consensus_identity_id(),
            other.consensus_identity_id(),
            "{what} moved the IDENTITY — a scheduled fence must stay a schedule difference, or the fix itself is a flag day"
        );
    }
    assert_ne!(shipped.consensus_schedule_id(), moved.consensus_schedule_id(), "the operator log must name the height");
}

/// **Every fence the visitor reaches is inside the fingerprint** — on every shipped preset, as
/// the node materialises it.
///
/// `consensus_params_id` is exhaustive over `Params` by destructuring, and that is exactly what
/// let 14c453d1 through: binding a field satisfies the compiler whether or not the body writes
/// it. This asks the question behaviourally instead. `for_each_fence` visits every fence (the
/// identity's normalisation depends on it reaching all of them); moving any ONE of them by one
/// step must move the fingerprint. A visit that changes nothing (the stand-in an absent
/// Some-only fence is visited through) is skipped by comparing the whole `Params`.
#[test]
fn every_fence_the_visitor_reaches_moves_the_fingerprint() {
    // testnet-11 materialises as the PALW-RC ruleset (`palw_rc_shipped_params`); suffix 12 is
    // retired into it and `From<NetworkId>` refuses it by design.
    let nets = [MAINNET_PARAMS.net, TESTNET_PARAMS.net, TESTNET11_PARAMS.net, SIMNET_PARAMS.net, DEVNET_PARAMS.net];
    let mut failures = Vec::new();
    for net in nets {
        let base = Params::from(net);
        let base_debug = format!("{base:?}");
        let base_id = base.consensus_params_id();
        let mut index = 0usize;
        let mut moved_count = 0usize;
        loop {
            let mut tweaked = base.clone();
            let mut visited = 0usize;
            tweaked.for_each_fence(&mut |score| {
                if visited == index {
                    *score = match *score {
                        u64::MAX => u64::MAX - 1,
                        s => s + 1,
                    };
                }
                visited += 1;
            });
            if index >= visited {
                break;
            }
            if format!("{tweaked:?}") != base_debug {
                moved_count += 1;
                if tweaked.consensus_params_id() == base_id {
                    let named = base
                        .palw_fences_v1()
                        .into_iter()
                        .zip(tweaked.palw_fences_v1())
                        .find(|(a, b)| a.1 != b.1)
                        .map(|(a, _)| a.0)
                        .unwrap_or("a non-PALW fence");
                    failures.push(format!("{net}: visit #{index} ({named}) moved and the fingerprint did not"));
                }
            }
            index += 1;
        }
        assert!(moved_count > 0, "{net}: the visitor changed nothing — the sweep would be vacuous");
    }
    assert!(failures.is_empty(), "fences outside the fingerprint:\n{}", failures.join("\n"));
}

/// The scheduling cases above only mean anything if the fences they arm are NOT already
/// genesis-active on this preset — arming a genesis-active fence somewhere else is a rule
/// change and SHOULD move the identity, which is the assertion in
/// `a_genesis_active_fence_is_not_normalised_away`. Pin which side of that line each fixture is
/// on, so a preset edit cannot quietly turn one test vacuous and the other wrong.
#[test]
fn the_scheduling_fixtures_are_on_the_side_of_the_line_they_claim() {
    let dns = MAINNET_PARAMS.dns_params.clone().unwrap();
    for (name, score) in [
        ("vlt shadow", dns.vlt.vlt_shadow_activation_daa_score),
        ("vlt weight", dns.vlt.vlt_activation_daa_score),
        ("tkn", dns.tkn.tkn_activation_daa_score),
        ("tkn emission epoch", dns.tkn.emission_activation_epoch),
        ("mandatory attestation inclusion", dns.mandatory_attestation_inclusion_daa_score),
        ("coinbase settlement consensus", dns.coinbase_settlement_consensus_activation_daa_score),
        ("evm", MAINNET_PARAMS.evm_activation_daa_score),
        ("crescendo", MAINNET_PARAMS.crescendo_activation.daa_score()),
    ] {
        assert_ne!(score, 0, "{name} is genesis-active on mainnet, so the scheduling case for it is wrong");
        assert_ne!(score, 9_000_000, "{name}: the fixture height must differ from the shipped value");
    }
    // ...and the ones the identity MUST separate really are in force from block 1.
    for (name, score) in [
        ("dns overlay", dns.dns_activation_daa_score),
        ("bond spend gate (audit M1-1)", dns.bond_spend_gate_mergeset_activation_daa_score),
        ("full reward split", dns.full_reward_split_daa_score),
        ("pos v2", dns.pos_v2_activation_daa_score),
    ] {
        assert_eq!(score, 0, "{name} is expected to be genesis-active on mainnet");
    }
}

/// Every genesis-active fence on the shipped mainnet preset is a RULE: turning any one of them
/// off must be refused at the handshake, not waved through as a schedule difference (re-audit
/// R-1). `a_genesis_active_fence_is_not_normalised_away` proves it for the two that audit M1-1
/// moved; this is the same property for the rest of them.
#[test]
fn turning_off_any_genesis_active_fence_moves_the_identity() {
    let today = MAINNET_PARAMS;
    let off = |mutate: fn(&mut crate::dns_finality::DnsParams)| {
        let mut p = MAINNET_PARAMS;
        mutate(p.dns_params.as_mut().unwrap());
        p.consensus_identity_id()
    };
    for (name, id) in [
        ("dns overlay", off(|d| d.dns_activation_daa_score = u64::MAX)),
        ("full reward split", off(|d| d.full_reward_split_daa_score = u64::MAX)),
        ("pos v2", off(|d| d.pos_v2_activation_daa_score = u64::MAX)),
        ("finality fee", off(|d| d.finality_fee_activation_daa_score = u64::MAX)),
        ("unbond authz mergeset", off(|d| d.unbond_authz_mergeset_activation_daa_score = u64::MAX)),
    ] {
        assert_ne!(today.consensus_identity_id(), id, "{name}: in force on one side and never on the other is a rule difference");
    }
}

#[test]
fn shipped_presets_have_pinned_fingerprints() {
    // Golden vectors. Any change to what goes into the fingerprint, or to how it is encoded,
    // breaks these — which is the point. It forces the change to be deliberate, and it lets an
    // operator tell whether two releases will peer before deploying one of them.
    //
    // If you are here because a preset legitimately changed: update the value, and understand
    // that nodes on the old build will no longer peer with this one. That is usually the
    // correct outcome. Make sure it is the intended one.
    //
    // Last moved when the Compute Token Program's `tkn: TokenParams` joined `DnsParams`
    // (design v0.1 §10, inert `u64::MAX` fences everywhere — same shape as the `vlt` and
    // settlement-fence appends before it: `dns_params` is hashed as its whole borsh encoding,
    // so every preset that carries an overlay moved at once). Deliberate: the same release
    // also admits the 0x30/0x31 token-op subnetworks, which older builds reject per-tx, so
    // shipping it is the next coordinated flag day, not a rolling update — exactly as it was
    // for the settlement/preference batch this note previously recorded.
    //
    // Report every preset rather than dying on the first. All four moved together on that
    // merge, and a first-failure assert showed one of them, which reads as a narrower change
    // than it was.
    //
    // PALW LLM PoW (this branch): all four moved again because `pow_palw_activation` entered
    // the hash (every preset writes its daa_score, so even the `never()` nets shift — same
    // mechanics as the TokenParams merge above). Devnet moved for three additional,
    // deliberate reasons: `always()` activation, the 0.1-bps `new_deci_bps` blockrate, and
    // the re-genesised trivial-bits genesis hash. Coordinated flag day, as before.
    //
    // And once more when `pow_palw_ollama_activation` entered the hash (the Phase-4b
    // Ollama-runtime algo, `always()` on testnet-10 — the fleet's runtime — and `never()`
    // elsewhere).
    //
    // **Moved 2026-09-02 by ADR-0072 — the ticket is the execution.** `PALW_ATTEMPT_V2_VERSION`
    // 5 → 6: both lotteries of an algo-6 header are drawn from `execution_commitment_v3`, which no
    // nonce inside the anchor's bucket moves, so a build on the old rule admits blocks this one
    // refuses at the class lottery and the two must not peer. The attempt version sits inside the
    // V2 bundle's ruleset id by design, so the rows carrying a bundle move (2: testnet-11, devnet)
    // and no other row does. Values taken from this test's own output on the fixed tree, twice,
    // never transcribed. Coordinated upgrade for testnet-11; no re-genesis.
    let changed: Vec<String> = [
        // main's 2026-08-14 row (`a1e6602e…`) is deliberately not taken: it pins a testnet
        // whose `dns_veto_ttl_daa_score` is 2_000, and this lineage keeps 120 for the reason
        // recorded at `TESTNET_DNS_PARAMS` — a 10 bps constant would be ~2.8 days at the frozen
        // 120 s cadence. The pin below is the merged tree's own value, checked by this test.
        // Re-pinned 2026-08-28 for audit M1-1: the mainnet bond spend gate now activates at
        // genesis, so the fingerprint moves. Legal exactly because mainnet has not launched —
        // no live node holds the old value, and the same change on a network WITH history
        // would be a partition (which is what M1-6 is about). Testnet's pin is unchanged in
        // the same edit, which is the check that this touched only the unlaunched preset.
        // **Re-pinned 2026-08-30 for the bond-economics pass.** `min_bond_amount_sompi`
        // 20M → 50M, `min_active_validators` 3 → 12, and `min_active_stake_sompi` restated as
        // their product. All three are overlay ADMISSION rules — they decide which StakeBond
        // enters the registry, and therefore which validator set exists, which StakeScore an
        // epoch has, and which anchors confirm. Two builds that disagree would accept
        // different bond sets and finalise different history, so the fingerprint moving is
        // the correct and necessary outcome: they refuse each other at the handshake instead
        // of at consensus. Mainnet has not launched, so nothing needs re-minting; the same
        // edit after launch would need an activation fence rather than a bare constant.
        // (testnet-11 overrides all three in `TESTNET_DNS_PARAMS`.)
        //
        // **ALL FIVE pins also moved 2026-08-30 by the 10B premine cap (ADR-0059)** — the
        // genesis is inside `Params` and every network's genesis identity moved (vault block
        // deleted, one 10B main wallet, t11's carve-outs; then Relaunch 4, marker 11,4) — **and
        // again by ADR-0060 Decision 4** (`DnsParams.inactivity_leak_daa` enters every
        // preset: ~7 days per live cadence, u64::MAX on devnet/simnet).
        //
        // **Union re-pin at the 2026-08-30 MERGE of the two lines** (bond economics ⊕ the
        // liveness doctrine ⊕ the 10B cap): both sides bumped `PALW_STATE_V2_VERSION` 12→13
        // and neither 13 ever shipped, so one 13 carries the union and every pin below is
        // the merged tree's own value — a fingerprint is a function of the whole ruleset,
        // never a sum of per-branch diffs.
        // **Re-pinned once more at the end of the 2026-08-30 AUDIT.** Four remediations move
        // these: the heartbeat lane ships off (`PALW_HEARTBEAT_LANE_ENABLED`), the inactivity
        // leak ships dormant AND stops being misfiled as an activation fence (it is a
        // duration — under the fence normalisation every value collapsed to one identity, so
        // two builds disagreeing about the finality denominator would have peered), the claim
        // lattice's redraw is now counted by the bond-withdrawal invariant and by
        // `MAX_CLAIM_EXPOSURE_DAA`, and a genesis bond declares the collateral its outpoint
        // actually holds. Derived once from the fixed tree, as always.
        // **Re-pinned 2026-08-30, MAINNET ONLY, for the single-bond-floor decision.**
        // `min_bond_amount_sompi` 50M -> 10,000 MSK and the derived `min_active_stake_sompi`
        // 600M -> 120,000, so that one number says what a bond costs on this network and it is
        // the one a PALW genesis seat already posts (ADR-0061). `dns_params` is hashed as one
        // borsh blob, so mainnet's fingerprint moves and nothing else does — testnet keeps
        // `TESTNET_DNS_PARAMS` and devnet/simnet keep `GENESIS_ACTIVE_DNS_PARAMS`, and their
        // pins below are unchanged, which is the check that this touched only what it meant to.
        // Legal exactly because MAINNET HAS NOT LAUNCHED: no live node holds the old value, so
        // there is nothing to re-mint and no partition to cause. The same edit after launch
        // would need an activation fence rather than a bare constant (audit M1-6).
        // **Re-pinned 2026-09-25, MAINNET ONLY: mainnet takes testnet-12's DNS numbers** (user
        // decision) — `min_active_validators` 12 -> 6, `min_bond_amount_sompi` 10,000 -> 20,000,000
        // MSK, `min_active_stake_sompi` 120,000 -> 120,000,000 MSK, `coinbase_settlement_long_maturity_daa`
        // 0 -> 600 (Decision A). `dns_params` is hashed whole; testnet keeps `TESTNET_DNS_PARAMS`,
        // which overrides all four, so its pin is unchanged. Previous: badaa8e9….
        ("mainnet", MAINNET_PARAMS, "eb866c61ca1a8ab58108be6cd1f39f951b582123472545575a5c7dbe0f1e5aa5"),
        // Moved by the bps01⊕iso unification (2026-08-16): the CPU pins are now the UNION of
        // the two facts the branches discovered separately — `single-variant` (bps01, by
        // disassembly) ∧ `no-openmp` (iso, by the Linux link error) in `CPU_BUILD_PROFILE`,
        // and the aarch64 class renamed to its honest `aarch64-dotprod` tag — and the
        // registered-model entries derive from those pins, so the materialized hash moved.
        // Only this preset: the other three do not carry the palw-lite registry.
        // NOTE this "testnet" is the PALW 120 s re-genesis net, NOT the live t10 — the live
        // t10's wedge TTL (`dns_veto_ttl_daa_score: 2_000`, 3addb8b) is deliberately NOT
        // carried here: a different genesis cannot join (or re-wedge) that network, and
        // live-t10 releases keep cutting from bps01/main.
        // Moved before that when the block interval was set to its final 120 s (2026-08-12) —
        // the blockrate, the DAA window duration and every VLT window are in the hash.
        // Moved before that by the t10 PALW re-genesis ("-palw" marker + trivial bits + 0.1-bps
        // blockrate + palw activation + the wall-clock-preserving DnsParams re-sizing) —
        // see docs/history/networks/testnet-10/testnet10-palw-rollout-runbook.md — and pinned MATERIALIZED (below) per
        // the 8208cd6 lesson, so the pre-merge values (`32cbf80f…` re-genesis-const /
        // `d07cb673…` shadow-materialized) were both superseded by that merge.
        ("testnet", TESTNET_PARAMS, "0d9cf361e02dea6d9e873014ff5e414c2e8e6879e8d705cde6c924a3a3f8dd88"),
        // The PALW staging net (gate-4 soak): differs from "testnet" in exactly the three
        // activation flips (hash lane off, PALW-4 on, Ollama off) + the TN11 genesis. Its own
        // pin proves the t10 row above did NOT move when this preset was added.
        // Moved TWICE by the 2026-08-20 public re-genesis, both times through the genesis
        // `utxo_commitment`: first the 347M MSK community allocation and the bumped relaunch
        // marker, then the 9B main wallet moving to the operator's `PALW_PUBLIC_MAIN_ADDRESS`
        // (the 40 vault UTXOs are untouched — this is the main wallet only). Only this row
        // moved either time: t10 keeps the shared testnet premine, and devnet/simnet/mainnet
        // are not testnet-11.
        // **testnet-11 is now the PALW-RC network** (`palw_rc_shipped_params`, routed at
        // `From<NetworkId>`), so this pin moved deliberately: `06285374…` was the algo-4
        // `LegacyTn11` lane, which was never published and is not running anywhere. The const
        // beside it is still the old one — the test materializes by NETWORK ID, which is what a
        // node reports, and that is the whole reason this test pins the materialized value.
        // **Moved 2026-08-26 by the court catalog growing** (ADR-0052). The RC params carry
        // `palw_court_catalog_root_v1()`, so the set of kernels the court can adjudicate is
        // part of this network's identity — which is the design: a node that cannot adjudicate
        // what its peers can is a node that will disagree about a conviction. `PALW-QWEN36`
        // added twenty-three integer descriptors (the A16 tier's nine and the hybrid graph's
        // fourteen), so the root moved and this pin moves with it.
        //
        // **Operationally this is a coordinated upgrade for testnet-11**: the fingerprint is
        // what peers compare at the handshake, so old and new builds will not agree. Nothing
        // about the floor class's arithmetic changed — only what else the court knows how to
        // re-execute.
        //
        // **Moved again, same day, by ADR-0053 withdrawing the second execution family.** Two
        // things inside the ruleset id changed: `min_class_panel` left the bundle (it existed
        // so a Metal class could license with two seats, and it applied to every class,
        // including deterministic ones, on every preset that shipped it), and the class record
        // lost `terms`, taking `PALW_STATE_V2_VERSION` from 7 to 8. Testnet-11 is the only
        // preset that moves because it is the only one carrying a PALW V2 bundle. Same
        // handshake consequence as above, and the same remedy: everyone upgrades together.
        // Moved 2026-08-26 (again, same day): `PalwShapeProfileV3` gained `logits_scheme_id`
        // — the class's logits commitment became part of its identity — so the floor's class
        // id, the catalog root and this fingerprint all moved together. The scheme binding is
        // what stops a class admitted at the tiled close price from committing flat
        // (unprosecutable work); a coordinated upgrade, riding the re-mint the ADR-0053 state
        // bump already forces.
        // Moved a third time, deliberately, when the court learned to READ a checkpoint:
        // `PALW_V2_TRACE_FORMAT_VERSION` 1 → 2, because a refutation may now carry an
        // attention step's KV history as one committed checkpoint instead of one opening per
        // cached position. A v1 court and a v2 court disagree about a verdict on the same
        // object, which is a fork — the version is what makes them refuse each other at
        // startup instead of finding out at consensus.
        // Moved again, deliberately, when BASE-0 registered its checkpoint state chunk map
        // (`palw_state_chunk_map`). `state_chunk_map_id` is a field of `PalwShapeProfileV3`,
        // the class id IS the profile id, and the class id is in the RC bundle — so filling in
        // the unregistered sentinel moves the ruleset. **This build and the one running on t11
        // are different networks**, and they find out at the handshake rather than at
        // consensus, which is the direction to fail in. Landing it needs an RC re-mint; no
        // other preset moved, because no other preset carries a PALW class.
        // Re-derived at the MERGE of the two lines above (2026-08-26): every move both trails
        // describe lands in one value, because a fingerprint is a function of the whole
        // ruleset and not a sum of diffs.
        //
        // And once more the same day: the class catalog gained `court_cost` — the derived
        // close price, carried so the BOOT gate enforces the ceilings a genesis-minted class
        // previously bypassed. The catalog root is inside the ruleset id, so asserting more
        // about a class moves the network's identity, as it must.
        //
        // And once more for the ceilings themselves, which is what that `court_cost` is a
        // price against: `PalwCourtParamsV2::max_close_bytes` is 80 KiB — derived from what a
        // standard transaction can carry rather than left at a generous default — and
        // `PALW_RC_BASE0_GEOMETRY` is `vocab_size` 1,024 / `n_ctx` 12, because at 4,096/512 the
        // floor's own closes were megabytes and no block could carry them. The geometry is the
        // class id, the class id is in the catalog root, and the catalog root is in the bundle.
        // And again for the sliced court (2026-08-26, late): ten descriptors renamed to their
        // sliced semantics, two minted (the lane-sliced silu, the head RMS norm), the tile
        // floor lowered to 8, and the class constants the whole-close derivation chose —
        // every one inside profile ids or the catalog, so the fingerprint moves once more on
        // the same re-mint.
        // And once more when the court learned RANGE OPENINGS (2026-08-27): a canonical row
        // rides one sibling set per contiguous run instead of one path per leaf, the metric
        // prices runs, and the derivation then CHOSE the hybrid class's constants — canonical
        // (7,2), n_ctx 8 — landing its worst close at 73,636 of the 81,920 carrier. The
        // two-class genesis assembles from this value on.
        // **And the move this whole line was for: testnet-11 now registers TWO classes.**
        // `PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT` is pinned, so `palw_rc_shipped_params`
        // assembles the two-class card — the catalog gains the Qwen3.6 entry, the genesis
        // object list gains its registration, and both are inside the ruleset id. A node on
        // the old value and a node on this one are on different networks and find out at the
        // handshake, which is the direction to fail in; joining needs the re-mint the whole
        // day's work already forced.
        // Moved again by the A16 dense tier joining the COURT (2026-08-27): two kernels the tier
        // had always performed and the catalog never named (its rotation and its elementwise
        // product), and the tile floor down to 4 so an 8,960-lane reduction can close. The
        // catalog root is inside the ruleset id, so teaching the court a new op moves the
        // network — the same coordinated upgrade as every other move on this line.
        // **THREE classes now.** The A16 dense tier joins the floor and the hybrid: the
        // Qwen2.5-1.5B artifact root is pinned, so `palw_rc_shipped_params` registers it in
        // genesis and the catalog gains its entry. Same handshake consequence as every other
        // move on this line, and the same remedy — everyone upgrades together.
        // **ADR-0056 Decisions 3 and 5: the class economy enters the ruleset.** Seven parameters
        // (the registration reservation, the four share-walk constants, the reclamation window
        // and the floor's protected permille) plus state version 8 -> 9, which the class
        // record's registrant bond, the `Dormant` status and the two new accumulators earn.
        // Every one of them decides who holds cadence, so every one is in the fingerprint.
        // Same coordinated upgrade as every other move on this line.
        // **simnet and devnet move for the first time on this line**, and the cause is the audit
        // batch's re-genesis: the test networks mint a 10B premine, which is a genesis input and
        // therefore a fingerprint input. Mainnet's is untouched, which is the point of a change
        // that only re-genesis networks nobody has value on.
        // **And once more for the HIGH ledger** (`fix/audit-batch-1`, 2026-08-27): fifteen
        // findings fixed, of which `PALW_ATTEMPT_V2_VERSION` 4 -> 5, the lane split and the
        // opening ceiling are ruleset fields. Their line derived 934ad9de before this merge;
        // the value below is the re-derivation over BOTH lines, because a fingerprint is a
        // function of the whole ruleset and never a sum of diffs.
        // And once more for the SHARE-RAISE path (ADR-0054, 2026-08-27): `PalwStateParamsV2`
        // gains `class_growth_permille` and `base_class_reserve_permille`, and the RC bundle
        // turns them on at 250‰ / 500‰. The state params are borsh-serialized into
        // `palw_ruleset_id_v2`, so a rule that lets a class's cadence share follow its own
        // production is a different network from one where a share is fixed at the grant
        // floor forever — which is exactly what a fingerprint is for. Lands on the same
        // re-mint as the class above; no other preset carries a PALW bundle, so no other
        // preset moves.
        // **Re-derived at the merge of those two lines (2026-08-27).** The three-class card and
        // ADR-0054's share rule landed on separate branches and both move this value, so the
        // pin here is NEITHER side's: a fingerprint is a function of the whole ruleset, not a
        // sum of diffs, and carrying one side's number through a merge would pin a network
        // nobody runs.
        // And once more when the model tiers were FUNDED at genesis rather than admitted at the
        // grant floor (2026-08-27): the hybrid and the dense tier hold 200‰ each and the floor
        // 600‰, which is three numbers inside the genesis object list and therefore inside the
        // bundle. ADR-0054 could have walked them there over about four weeks of production;
        // this is the network deciding to start where it means to be.
        // And once more when the genesis registry was sized to CARRY those tiers: a claim of the
        // hybrid class reserves 348x what a floor claim does, so bonds funded for the floor
        // admitted fifteen of them against a two-hundred-block allowance and then held. The
        // declared collateral is re-derived from the dearest class the card registers, and the
        // bind-window gate now measures that class instead of the base one — both inside the
        // bundle, both moving this value.
        // And once more for the audit batch: the liveness floor gained a reserve
        // (`min_base_class_share_permille`) after 999 individually-legal grants were shown to
        // take it to 1‰, and the test networks re-genesis at a 10B premine. Both are inside
        // the bundle, so both move this value — re-derived once at the end of the merge rather
        // than carried from either line.
        // **Re-derived once at the END of this merge**, over the union: the three-class card,
        // the HIGH ledger, and ADR-0054's share rule all move it, and the shipped value is the
        // ruleset's own function rather than any one branch's number.
        // And once more when the Qwen3.6 artifact root was RE-PINNED (2026-08-27): the first
        // pin was measured on an artifact that no longer exists and that no conversion
        // reproduces — three independent reconstructions (the pin commit's own converter,
        // HEAD on arm64/macOS, HEAD on x86-64/Linux) agree on one root from the pinned GGUF,
        // and it is not the pinned one. The root sits in the genesis registration and the
        // class catalog, both inside the ruleset id, so correcting it is a re-mint — which is
        // the honest shape: a network whose registered class nobody can obtain and one whose
        // class anyone can rebuild from the public GGUF must not wear one name.
        // ADR-0058 (merged work is counted): PALW_STATE_V2_VERSION 9 → 10 entered the
        // fingerprint, deliberately — deploying this build is a re-mint.
        (
            "testnet-11",
            TESTNET11_PARAMS, // Re-pinned 2026-08-28 with the audit fixes: the ruleset id now commits to every ML-DSA
            // context the acceptance layer verifies under (M2-23), and the transition's rules
            // changed in ways that alter which blocks are valid (M2-3, M2-5, M2-7, M2-8, M2-11,
            // M2-12, M2-16). A moved fingerprint is the CORRECT outcome of that — this project's
            // own lesson is that a rule change not declared by a version bump forks the network
            // silently. testnet-11 must be re-minted onto this build; mainnet has not launched.
            //
            // **Re-pinned 2026-08-29 for the audit3 remediation.** `PALW_STATE_V2_VERSION` 10 → 12,
            // which is hashed here directly. Two rules moved and neither changes a schema, so this
            // number is the ONLY thing that tells two builds they disagree: S-03 stops a class
            // awaiting activation from holding a live share (registrations that used to be
            // accepted are now refused, and the old path could panic `granted_share_table_v2`
            // outright), and S-04 makes the coinbase's entitlement filter ask the full admission
            // rather than a subset of it (merged blocks the chain refuses for epoch budget or
            // exposure are no longer paid; a merged receipt spend is no longer denied). Different
            // blocks are valid and different coinbases are valid; H4 additionally stops an
            // unanswered opening rung from slashing the accuser, and a bond's `slashed` is state.
            // So the fingerprint moves — and only this preset carried a PALW V2 bundle then.
            //
            // **Re-pinned 2026-08-30 for the bond-economics pass.** `PalwStateParamsV2` gains
            // `min_slash_permille_of_escrow`, the rule that a claim's collateral must be a
            // fraction of the reward it escrows. It ships DORMANT (zero) — testnet-11's own slash
            // value cannot satisfy any non-zero value, so switching it on is a mint-time decision
            // and not a deployment — yet the field is hashed, so the fingerprint moves exactly as
            // `registration_exposure_sompi` did. The same pass moved `PALW_STATE_V2_VERSION`
            // 12 → 13: a claim whose panel concludes nothing is REDRAWN once rather than voided
            // against its producer, and `PalwClaimStateV2` gained the `rebound_daa` that dates the
            // second panel's anchor — not root-schema-neutral, an old build cannot decode the state.
            //
            // **Moved again by ADR-0060 (the liveness doctrine), riding the same union 13:** a V2
            // network now accepts bondless algo-3 heartbeat blocks — new block validity (slot
            // rule, lane retarget, fee-only coinbase) and new fork-choice arithmetic (ε work) —
            // and every preset gained `DnsParams.inactivity_leak_daa` (Decision 4). **And by
            // ADR-0061:** the genesis collateral outputs shrink 0.1B → 10,000 MSK each (the
            // declared collateral is the DERIVED 3,223.07 MSK, so `palw_ruleset_id` is untouched)
            // and the zero-seat genesis gate change moves no bytes on a seated network.
            //
            // **This value is the union re-pin at the 2026-08-30 merge** — see the mainnet row.
            //
            // **Re-pinned again at the 2026-08-31 MERGE of two independent changes**, and it is
            // neither branch's value: a fingerprint is a function of the whole ruleset, never a
            // sum of per-branch diffs, so `4f89ec82…` (the registry growth) and `d29e5667…` (the
            // quantum drop) are both stale the moment the two meet. This is the merged tree's
            // own value.
            //
            // The two causes, because they are separate and each is a re-mint on its own:
            //
            // 1. **The genesis bond registry grew from six seats to eight**
            //    (`PALW_RC_GENESIS_BONDS`, Relaunch 4, marker 11,4). Two real ML-DSA-87 cards
            //    enter `genesis_objects` and two collateral + two fee-float outputs enter the
            //    premine, so the genesis, the premine commitment and the bundle all move
            //    together. The reason is ADR-0065 D1: at `seat_count + 1` the maturity fence
            //    could not be armed on ANY network this build ships, because the arming guard
            //    refuses a registry with no spare seat — enforcement existed and arming did
            //    not. At `seat_count + 3` the fence is a config change rather than a re-mint.
            //
            // 2. **The free-prompt quantum dropped 1,000 → 100** (and `pwu_per_quantum` 100 →
            //    10 with it, holding weight-per-CU constant). The quantum is inside the
            //    ConsensusV2 free-prompt bundle and therefore inside `palw_ruleset_id_v2`, so
            //    lowering it is a ruleset change. At 1,000 no registered class could reach a
            //    single quantum — BASE-0 tops out at 705 CU and QWEN25-A16 at 961 — so the
            //    lane priced every real prompt at zero draws.
            //
            // **Only testnet-11 moved.** The other presets install neither a bond registry nor
            // a free-prompt bundle, so the blast radius is exactly the network both changes
            // were for. **This is a re-genesis for t11 and every host must wipe its datadir**;
            // nothing in this build accepts the old chain.
            //
            // **Re-pinned again 2026-09-01 (ADR-0068 Phase 2 — the Relaunch 5 identity):**
            // 0533c8ee… → d7510c7a…, four decisions in one train: the floor's reserve
            // 500‰ → 20‰, genesis shares 489/489 (floor ends at its reserve — the
            // LLM-primary table), both fences armed FROM GENESIS (always), and drill
            // finding F4's pruning depth-consistency nudge now applied in the shared
            // assembly. **This is a re-genesis for t11 and every host must wipe**; devnet
            // moved with it (→ 3f13411b…) through the shared builder.
            //
            // **Re-pinned 2026-09-01 (ADR-0068 Phase 1): the clock arms.** 5ccdd684… →
            // 05df4e5e… (then → 0533c8ee… when F5's chain exemption re-versioned the width
            // rule's hash label, same deploy train) because `palw_rc_shipped_params` now schedules `palw_heartbeat`
            // (work_log2 24, max_per_mergeset 4) and `palw_attempt_work` (work_log2 20) at
            // DAA `PALW_RC_PHASE1_FENCE_DAA` (5,000; the tip was 1,746 when chosen). This is
            // NOT a re-genesis and NOT a wipe: a scheduled fence is normalised out of
            // `consensus_identity_id`, so this build and 5ccdd684-era builds handshake and
            // follow one chain — until the fence height, where an un-upgraded node stops
            // accepting new blocks (the attempt lane's declared blue work changes). Roll the
            // fleet before DAA 5,000.
            //
            // **Re-pinned once for two bodies of work that landed together** — a fingerprint
            // is not the sum of its diffs, so the merge of two re-pins is a third value and it
            // is computed here, after both.
            //
            // The launch audit's remediation: the model tiers' genesis shares go to zero
            // (weight follows the end-to-end certificate, ADR-0069), ADR-0065 D4 arms from
            // genesis so an `Unavailable` quorum can no longer slash honest seats for the
            // network's own packet loss, and the bundle gains `court_e2e_root`.
            //
            // And the model tiers' step spaces becoming adjudicable, which moved
            // `PALW_V2_TRACE_FORMAT_VERSION` 2 → 3 (the tiled decode-token pin and the
            // map-versioned checkpoint-anchor geometry — see the constant's own doc).
            //
            // The genesis is untouched by both; the RULESET id moves, so this build and
            // d7510c7a-era builds refuse each other at the handshake instead of disagreeing
            // about a court verdict at consensus. **Deploy is whole-fleet-together.**
            //
            // …and once more for ADR-0071: the ticket is bounded by executions (Decision 2),
            // seats declare what they can judge (Decision 3), and an idle class converges to
            // the incumbent price (Decision 1a). Every one of those is a rule two nodes must
            // agree on before they agree on a block, so the id moves and old builds refuse
            // this one at the handshake instead of diverging at a verdict.
            //
            // **And once more to UNDO Decision 1's target freeze.** Relaunch 5 shipped with
            // the attempt lane's Layer-0 target frozen and measured what that costs: the floor
            // produced ~50 blocks a minute against a 0.5/min target, flat, because
            // `retarget_over_span_v1` only redistributes cadence BETWEEN classes and a
            // one-class network is a deliberate no-op for it. Block interval is
            // `calculate_difficulty_bits`'s job, which the freeze had taken away.
            // **Deploy is whole-fleet-together.**
            //
            // **And once more because the A16 genesis card pinned the wrong ROOT FORM.**
            // `PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT` held the file's `artifact_digest()`;
            // the graph-v2 row the chain registers resolves by the A16 operand-inventory root
            // (`CanonicalClassV1::artifact_root`), so every A16 producer refused its own
            // artifact and the dense tier made zero blocks on Relaunch 5. Re-pinned to the
            // inventory root measured over the deployed file. Genesis hash unchanged.
            // …and once more for ADR-0076: the three classes no longer share one attempt
            // target. `initial_target` is a field of `ClassRegistered`, so the seeds are
            // inside the ruleset id — which is the property that keeps a 5d node and a 5e
            // node from agreeing on a name while disagreeing about which class may produce.
            // …and once more for ADR-0083 Decision 1, SCHEDULED at DAA 1150 on the live chain
            // (Relaunch 5f's flag day, 2026-09-04): the Some-only write of
            // `palw_difficulty_priced_rows` enters the fingerprint, the genesis does not move,
            // and the fence-normalised identity is unchanged — so a seat on the previous
            // build peers with this one until the height. Previous value, for the record:
            // 2222e054f87bed7a33e9c017f5403cd52070d0778776b5bd78143e7f82ff92b7.
            // **Re-pinned 2026-09-06** for testnet-11's second flag day: ADR-0062's court,
            // ADR-0077 D16's private prompt and ADR-0087/0088/0089's model market, all
            // scheduled at DAA 1,900, plus the pruning horizon the court's fence drags with it
            // (6,602 → 12,002). The identity is unchanged — `scheduling_the_da_court_moves_the_fingerprint_and_leaves_the_identity_alone`
            // — so the two builds agree about every block either can produce until the height.
            // They do NOT stay peers, and this comment used to say they would: the fork-id gate
            // on the PREVIOUS build is already armed by ADR-0083's fence at 1150, it sees
            // `next = 1900` on a schedule that has no 1900, and it sends the reject. Measured
            // 2026-09-06 — nine outside addresses cut off within minutes of the first restart.
            // See `fork_id_gate_fences_v1`. An un-upgraded seat must rebuild NOW, not by 1900.
            // Previous: 71b35c250d01598ee8925146e66e8200945503ce2de1030bfd167e799b2498e9 (the
            // ADR-0083 flag day, 2026-09-04).
            // **Re-pinned 2026-09-06, same flag day, one more fence.** `palw_court_ladder`
            // (ADR-0084 U-08 / the 2026-09-06 audit's H-4) joins the DAA 1,900 set: past it the
            // refutation walkers take the ruleset's `max_step_leaf_count` instead of the
            // executor's constant, so a dispute over the dense graph-v5 row can be tried at the
            // width the row was admitted at. The identity does not move — a scheduled fence is
            // normalised out of `consensus_identity_id`, which is what lets the fleet roll one
            // host at a time — and the schedule id does, which is what names the height in the
            // operator log. Previous: b511dd1e99b673c62f3023d3cc1e0f4bc48ca8888d535ed62190d907505de531.
            // **Re-pinned again the same day: the ladder moved to a height of its own.** It first rode
            // 1,900; `fork_id_v1` digests the fired HEIGHTS and never the fence set, so a build
            // carrying the ladder at 1,900 and one without it advertised the identical fork id
            // and would have peered and then disagreed past the height. Measured before moving
            // it: same fork id at 1,899, 1,900 and 2,500. Previous, and not to be deployed past
            // 1,900: ebd3b321d4ac68a719f39701af1bfa3931230f27cb77d9fd0e26a513a151ede4.
            // **Re-pinned 2026-09-11: the fingerprint finally carries ADR-0095's fence.**
            // 14c453d1 scheduled `palw_model_benefits` at DAA 2,400 and never wrote it, so the
            // build that crossed it and the one without it both printed 060e3597… — the third
            // flag day was invisible to the one check the README told operators to make. No
            // rule moved: the identity is unchanged (a scheduled fence normalises to `None`,
            // `the_benefits_fence_is_inside_the_fingerprint_and_outside_the_identity`), so a
            // node on this build and one on the build before it stay peers; the handshake
            // reports a schedule difference between them instead of refusing. Previous, which
            // every build from 14c453d1 to a5f1bdf7 prints:
            // 060e3597cd2950bc183b215b5ff87538e72dd788cab43829dca6bc72bcb5ac89.
            // **Re-pinned 2026-09-11 for ADR-0114's flag day**: the owner leg's fence scheduled
            // at DAA 3,500. The identity does not move (a scheduled fence normalises to `None`),
            // so the fleet rolls one host at a time and the fork-id gate keeps builds with and
            // without it peers until 3500 (`the_owner_leg_flag_day_keeps_every_current_node_until_3500`);
            // past 3500 every node must be on this build. Previous (every build through
            // e458683e): ecbdbc2222efcc2d32493f2349e7c17f983611697a5d10ffc7ae90458bac8ee5.
            // **Re-pinned again 2026-09-11 for the SECOND flag day** (the pre-mainnet audit,
            // `PALW_RC_AUDIT_FENCE_DAA` = 4,000): `palw_audit_2026_09_11` arms at 4,000, adding
            // one schedule entry (1150/1900/2150/2400/3500/4000/2125000). Only THIS fence arms on
            // testnet-11 — the audit's share-census fence (`palw_share_growth_final`) and
            // dense-court fences (`palw_fused_dissectable` / `palw_attn_anchored_root`) are
            // mainnet-card corrections and stay dormant here. Same rolling-restart contract as
            // 3,500: identity unmoved, the fork-id gate keeps builds with and without the fence
            // peers until 4,000, and every node must carry it past 4,000. Previous (ADR-0114's
            // owner-leg build): 02c7282b7541011344eabb1ce6cbe6544987e7b6a7fc132413fbfff0a6a37cfd.
            // **And once more before it shipped: ADR-0117's `palw_prefill_draw` rides the same
            // 4,000** (operator, 2026-09-11). The schedule's heights do not move (one height), the
            // fingerprint does (the fence is written by name). The audit fence alone (a8f00497,
            // never released) printed 09efd285a32f31c883edc5b9b869892b15895bf1194593c067556c94cf8fc694
            // and shares this build's fork id — see `PALW_RC_AUDIT_FENCE_DAA` for why it must not run.
            // Previous (the 4,000 release): 4300409bb7127c8ee57bad783e4610e37bbd3c7cc535279d729fda3341d666dd.
            // **Re-pinned 2026-09-12 for the 7,000 release** — the one re-pin of the build that
            // carries every fence it schedules: ADR-0120's `palw_model_seed_v2` at 6,900 (its own
            // height, so a build without it is refused there rather than parted from silently), and
            // at 7,000 the held regime (ADR-0118: `palw_held_context`, `palw_shard_court`,
            // `palw_fp_da_pins`, with ADR-0119's held ladder riding it) together with the audit's
            // DEEP findings (`palw_audit_2026_09_11_deep`) — schedule 1150/1900/2150/2400/3500/4000/
            // 6900/7000/2125000. The branches' own values were intermediate: held alone
            // c2e5ffbef5e0326ee117b0554225b0c97da52fbfcb47585632c01cb12236d7b0, deep alone
            // d6ebbcab7691aec574e983b2c8dd81a6bd1fc1d340e92a7efba2f7e305449bec, ADR-0120 alone
            // 7fa26c269a2c007f077b687eb3ce5cdf5bbd6dbfe263980688fbec5e0ace99e8, held + deep
            // 941300ddff87abfa0a85fb7e6f54d8a8e7fd5e0935c17b351e93e3a43c860bf9.
            // **Re-pinned 2026-09-17 (evening) when the operator moved the heights**: the held regime
            // and the deep audit 7,000 → 6,000, the flag day 7,001 → 6,001, ADR-0134's retirement
            // 7,201 → 6,201 ("時間がかかりすぎる", the tip at 5,798; ADR-0120's 6,900 stays) —
            // schedule 1150/1900/2150/2400/3500/4000/6000/6001/6201/6900/2125000, fingerprint
            // 135b6ee0…; the previous pin (dd805c9f…, the 7,001 schedule) was never deployed. The
            // deployed 7,000 release is refused from 6,000, a height it does not schedule
            // (`the_moved_heights_refuse_the_deployed_7000_release_from_6000_and_keep_it_below`).
            // **Re-pinned 2026-09-17 (morning) for the 7,001 flag day** (then `PALW_RC_FLAG_DAY_7001_FENCE_DAA`, the
            // operator's height, chosen at DAA ≈5,773): ADR-0124's `palw_panel_economy` and
            // `palw_work_priced_reward`, ADR-0125's `palw_execution_lane` (one permit a round, spans of
            // 30 DAA, 600 a mergeset), ADR-0126's `palw_overlay_carve` (2,000 bps, 720 ‰) and
            // ADR-0128's `dns_bft_gate` (5,040 / 200 / 4), all at 7,001 — schedule 1150/1900/2150/
            // 2400/3500/4000/6900/7000/7001/2125000. The identity does not move, so builds with and
            // without the set peer until 7,001 and refuse each other from it
            // (`the_6001_flag_day_keeps_the_7000_release_until_6001`); every node must carry it
            // before the height. Previous (the 7,000 release, 13520042):
            // ae1d61628da50c7becea62f0a8f08c8654d190c60b2e104df0010b121ba4d3d8.
            // **Re-pinned 2026-09-17 for ADR-0130, on the same 7,001 flag day — one re-pin for the
            // union of its two halves**, neither of which is the value below on its own:
            //
            // * the panel draw is one lottery entry per OPERATOR past `palw_panel_economy`, named
            //   in the hash as `palw_panel_draw/operator_ticket_v1` beside the fence's height
            //   (alone: 236cdb562c8945bd55089f1674c5817a19022ec8baf44535984af57a848605e7);
            //   ADR-0130's own fence, `palw_panel_exposure_floor`, is `None` here and writes nothing;
            // * the lane's spans drop from 30 DAA to 5 (a span's producers are public ten minutes
            //   ahead, not an hour) and the scheduler's rule set is named in the hash
            //   (`palw_execution_lane/scheduler_v2`: one parity an operator, participants fixed a
            //   span before a future anchor seeds them)
            //   (alone: a4df92dfa4b353e68310426710a24d58bff9628b2e52500077523adccae0387b).
            //
            // The identity is unmoved, as it is for every scheduled fence, so the rollout is still
            // host by host — but the fork id digests HEIGHTS, and both builds fire 7,001: a node on
            // the previous pin and one on this build advertise one fork id and would deal different
            // panels and run a different lane past the height, so the fleet must carry one of them.
            // Previous (the first 7,001 build, never deployed):
            // 4787b92a0e20065aac88f8258b581f415ace9f6093dfab35c345b48135269005.
            // **Re-pinned 2026-09-17 for ADR-0134** (`palw_compute_overlay_retired` at 7,201, its own
            // height): the schedule gains a height, so the fork-id gate separates this build from the
            // 7,001 union build (ab4e7b9c7e20d14cbadc0874312b8c6dff89ca66ed7e9be3af2f5523dc58b5f2, never
            // deployed) as well as from the 7,000 release.
            // **Re-pinned 2026-09-17 (night) for ADR-0135 Upgrade A and ADR-0132 Upgrade C on the same
            // 6,001 flag day** (`palw_model_registry` and `palw_economic_payout`, both `Some`-only in
            // the hash; the schedule's height set is unchanged, so the fork id does not move and a
            // node on the 135b6ee0… build is told apart by the fingerprint alone). The previous pin
            // (135b6ee0…) was not deployed.
            // **Re-pinned 2026-09-20 for ADR-0130's span-short at DAA 7,300** (`palw_execution_lane_span_short`,
            // 5 DAA → 1 DAA, f+2 kept): the schedule gains a height, so the fork-id gate separates this
            // build from `137b9c50…`. The identity does not move. Previous:
            // 137b9c50aac6c8aabb872519a48a8066bc14867d84b3a64e6bb081e094788fde.
            // **Re-pinned 2026-09-21: DAA clock 7,900 → 7,780** so the live BASE-0 tip at
            // 7,732 is not another hour of heartbeat silence. Identity unmoved.
            // Previous: 20bf662f012ecb226005f8b915fd744b440b5c37c876c6066163b74f10827eeb.
            // **Re-pinned 2026-09-21: ADR-0133 §7's seat gate at 8,100** — `required_ready_seats`
            // becomes the possession floor and the capacity it used to swallow is answered by
            // the in-flight cap and the admission rate, so the held 2M class is admitted rarely
            // instead of asking a seven-bond network for 9,992 ready seats. The schedule gains a
            // height; the identity does not move.
            // Previous: 8854b190978bbabd92381810a3183eb1c74864c97011b2adcda7ac714f625e23.
            // **Re-pinned 2026-09-23: the fifth RC family** (`PALW-QWEN36-V6`, the hybrid lineage's
            // fused, per-token-lift graph — the kernel set of every HELD hybrid row). The root of
            // the certified family set is inside every RC bundle, so testnet-11's fingerprint AND
            // identity move: no node on this build can rejoin testnet-11, which is superseded by
            // testnet-12 and unsyncable past DAA 7,219 by a correct validator in any case
            // (`docs/history/networks/testnet-12/testnet-12-regenesis-2026-09-23.md`). The previous pin was not deployed.
            // Previous: 79b49c238c46b0d97ab9b46d79fd5f85f8b50da623921a53f0af361515d50640.
            // **And again the same day, for the economic audit's state schema v21**
            // (`PALW_STATE_V2_VERSION` 20 -> 21, hashed into the bundle; `palw_audit_2026_09_23`
            // itself is Some-only and dormant here). Previous: 33bdff0b….
            // **And once more for ADR-0152's v22 skeleton** (`PALW_STATE_V2_VERSION` 21 -> 22,
            // hashed into the V2 arm): the one re-pin the version forces on every V2 preset.
            // `palw_rcore_plus` and C7's list are Some-only / non-empty-only and absent here, so
            // nothing else moved. Previous: c99bb4f4….
            "bd633ce933974d4134676efbdaf46b269dc2fb78f007e0907479aabd4d743f29",
        ),
        ("simnet", SIMNET_PARAMS, "63238ba10766c824ff6915484829b01eb4fc3c105665a7db2cf6b175bf870dfd"),
        // Re-pinned twice for ADR-0068 Phase 1: first when the drill network armed the
        // heartbeat and attempt-work fences (both ride `consensus_params_id`), then when
        // `Params::from(Devnet)` became the BUNDLED `devnet_shipped_params()` — ConsensusV2
        // over a zero-row registry (ADR-0061), the ruleset the live drill actually runs.
        // The genesis is untouched (nothing is carved), so only the fingerprint moves.
        // …and once more when the drill's pruning-consistency nudge raised devnet's
        // pruning_depth 10,800 → 10,805 (remainder k+1 of finality 600).
        // …and once more with the pair above: the audit remediation's `court_e2e_root` and
        // armed D4 fence, and `PALW_V2_TRACE_FORMAT_VERSION` 2 → 3. The drill preset shares
        // both through the same bundle builder.
        // …and once more for the 120 s cadence SET (mainnet audit item 6). `Params::from(Devnet)`
        // is a ConsensusV2 assembly, and the assembly used to write `target_time_per_block`
        // alone — so the drill network ran a 10 bps DAG (k, mergeset, merge depth, maturity)
        // and 10 bps DNS windows on a 120 s clock. `with_two_minute_cadence` sets the whole
        // set; devnet keeps its deliberately widened `max_block_parents: 64`. testnet-11 is
        // untouched because its base already carried the whole set — which is the property
        // that made this safe to apply unconditionally.
        // …and once more for ADR-0076. Devnet registers only the floor, and the seed rule
        // prices it from its own share and work like any other class: `MAX/278` where it used
        // to carry `MAX/2`. On a one-class network the target sets no cadence — the retarget
        // is a deliberate no-op there and the interval is `bits`' job — so what this buys is
        // the ROOM above the floor that a model tier registering later needs, which is the
        // same reason mainnet's floor-only mint is seeded rather than left at half the space.
        // …and once more on 2026-09-23 for the fifth RC family (`PALW-QWEN36-V6`): the drill
        // preset carries the RC court root through the same bundle builder, so it moves with it.
        // Previous: 61286b15588eb253b9a7f64997935c919b604b07d1973d4d2366084f06f524f5.
        // …and once more the same day for the economic audit's state schema v21. Previous: 3894d83e….
        // …and once more for ADR-0152's v22 skeleton (`PALW_STATE_V2_VERSION` 21 -> 22, the V2
        // arm's version hash; the R-core+ fence is absent here). Previous: 9acd42be….
        ("devnet", DEVNET_PARAMS, "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8"),
    ]
    .into_iter()
    .filter_map(|(name, params, expected)| {
        // MATERIALIZED, not the raw const. `with_registered_models` attaches the compute
        // profiles to a preset that has scheduled its VLT shadow fence, and it runs at every
        // `From<NetworkType/NetworkId> for Params` — so the const and the thing a node
        // actually runs are different values, and only the second one is what peers compare
        // at the handshake. Pinning the const would pin a number no node ever reports:
        // caught live, where a correctly-built release announced `5fabb683…` while this test
        // was green on `62e299b6…`.
        let actual = Params::from(params.net).consensus_params_id().to_string();
        (actual != expected).then(|| format!("  {name}: pinned {expected}, got {actual}"))
    })
    .collect();
    assert!(changed.is_empty(), "consensus fingerprint changed for {} preset(s):\n{}", changed.len(), changed.join("\n"));
}

/// **ADR-0150: the fingerprint sees the rule, and the handshake still lets a fleet roll out.**
///
/// The manifest speaks only for a ruleset this network can run and only above R1, so the two
/// halves are testable on the one preset that has such a ruleset: testnet-11, whose
/// `palw_readiness_v2` is scheduled and whose readiness rule is R2.
///
/// * a network that can run the changed rule fingerprints differently from one that cannot —
///   the property `1 << 20`'s silent redefinition did not have;
/// * their IDENTITIES are the same, because the identity normalises a scheduled fence to "not
///   yet" and the ruleset riding it drops out — which is what makes a rollout a rollout.
#[test]
fn a_rule_this_network_can_run_is_in_the_fingerprint_and_not_in_the_handshake() {
    let speaks = palw_rc_shipped_params();
    assert_eq!(
        speaks.palw_rule_manifest_gates_v1().readiness_v2,
        crate::palw_rule_manifest_v1::PalwGateStateV1::Scheduled,
        "the premise: testnet-11 schedules the rule that changed"
    );
    assert!(
        crate::palw_rule_manifest_v1::palw_rule_manifest_digest_for_gates_v1(&speaks.palw_rule_manifest_gates_v1()).is_some(),
        "and the manifest has something to say about it"
    );
    let mut silent = speaks.clone();
    silent.palw_readiness_v2 = None;
    assert_eq!(
        crate::palw_rule_manifest_v1::palw_rule_manifest_digest_for_gates_v1(&silent.palw_rule_manifest_gates_v1()),
        None,
        "a network that cannot run the rule hears nothing about it"
    );
    assert_ne!(
        speaks.consensus_params_id(),
        silent.consensus_params_id(),
        "a build that redefined a rule this network will run must not fingerprint like one that did not"
    );
    assert_eq!(
        speaks.consensus_identity_id(),
        silent.consensus_identity_id(),
        "…and must still peer with it until the height, or landing the rule partitions the fleet on deploy day"
    );
}

#[test]
fn the_encoding_cannot_be_confused_by_concatenation() {
    // Adjacent fields must not be able to trade digits. Moving a value from one field to the
    // next has to change the fingerprint, or two different rule sets could share one.
    let mut a = SIMNET_PARAMS;
    let mut b = SIMNET_PARAMS;
    a.max_tx_inputs = 10;
    a.max_tx_outputs = 20;
    b.max_tx_inputs = 1020;
    b.max_tx_outputs = 0;
    assert_ne!(a.consensus_params_id(), b.consensus_params_id());
}

#[test]
fn an_absent_dns_overlay_is_not_an_empty_one() {
    // `None` and a zero-length encoding must differ, or "no overlay" and "an overlay that
    // happens to encode short" would look like the same rule set.
    let mut with = SIMNET_PARAMS;
    let mut without = SIMNET_PARAMS;
    with.dns_params = Some(GENESIS_ACTIVE_DNS_PARAMS);
    without.dns_params = None;
    assert_ne!(with.consensus_params_id(), without.consensus_params_id());
}

#[test]
fn a_fork_activation_height_is_part_of_the_rules() {
    let base = TESTNET_PARAMS;
    let mut moved = TESTNET_PARAMS;
    moved.evm_activation_daa_score += 1;
    assert_ne!(base.consensus_params_id(), moved.consensus_params_id(), "a different activation schedule is a different rule set");
}

/// Re-audit high 2, pinned: **the registration checks now have a caller.**
///
/// `PalwClassRegistrationV1::validate` was implemented, well tested, and reachable from
/// nothing outside its own test module — installing `palw_credit = Some(..)` ran none of it.
/// This asserts both halves of the fix: every shipped preset passes (the fence is `None`, so
/// there is nothing to check and nothing to break), and a fence carrying a registration that
/// fails its own rules is refused rather than run.
#[test]
fn a_palw_fence_is_validated_before_a_node_runs_it() {
    for (name, params) in [
        ("mainnet", &MAINNET_PARAMS),
        ("testnet", &TESTNET_PARAMS),
        ("testnet11", &TESTNET11_PARAMS),
        ("devnet", &DEVNET_PARAMS),
        ("simnet", &SIMNET_PARAMS),
    ] {
        assert!(params.validate_palw_v1().is_ok(), "{name}: a fence-less preset has nothing to fail");
        assert!(params.palw_credit.is_none(), "{name}: no shipped preset installs a fence yet");
    }

    // A fence whose registration breaks its own canonicality rule must not be runnable.
    // Built from the registry's own fixture so the failure is the injected one, not a
    // half-built struct.
    let mut broken = SIMNET_PARAMS;
    let mut registration = crate::palw_registry::tests::fleet_registration();
    registration.pwu_per_inference = 0;
    broken.palw_credit = Some(crate::palw_credit::PalwCreditParamsV1 {
        registration,
        s_eff_sompi: 20_000 * 100_000_000,
        unbonding_period_blocks: 1_209_600,
        activation_daa: 0,
        class_daa: crate::palw_class_daa::PalwClassDaaParamsV1::stage1_defaults(),
    });
    assert!(broken.validate_palw_v1().is_err(), "an invalid registration must be refused, not run");
}

/// **ADR-0065 D1's liveness trap, closed as a rule rather than as a runbook note.**
///
/// The draw is fail-closed — a short panel is `InsufficientEligibleBonds`, so the claim never
/// binds and voids at `BindTimeout` — and the shipped genesis registers exactly
/// `seat_count + 1` bonds while the draw excludes the executor. Zero slack. Every genesis bond
/// carries `registered_daa = genesis.daa_score`, so a maturity fence armed before its own
/// window has elapsed makes ALL of them immature at once: no panel can be drawn on the whole
/// network, `safe_frontier` never leaves 0, and pruning never starts.
///
/// The ADR names this and offers "ship dormant behind a fence armed after the network has
/// bonds to spare", which is a thing an operator has to remember. `validate_palw_v2` refuses
/// the configuration instead, which is a thing nobody has to remember — and it is reachable:
/// `ConfigBuilder::build` (`config/mod.rs:287`) panics on a failing `validate_palw_v2` before
/// a peer is dialed, the same fail-loud stance the V1 fence already has. A guard that only
/// ever ran from its own unit test would be the gate-that-never-fires this ADR line keeps
/// finding.
#[test]
fn a_bond_maturity_fence_armed_inside_its_own_window_is_refused() {
    let v2 = palw_rc_shipped_params();
    assert!(matches!(v2.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)), "the fixture must be V2");
    assert!(v2.palw_bond_maturity.is_none(), "every shipped preset leaves ADR-0065 D1 dormant");
    assert!(v2.validate_palw_v2().is_ok(), "the shipped preset validates");

    // Armed at 500 with a 1,000-DAA window, on a genesis at DAA 0: at the moment the rule
    // comes into force, no bond in existence is old enough for it.
    let mut too_early = palw_rc_shipped_params();
    too_early.palw_bond_maturity = Some(PalwBondMaturityV1 { activation: ForkActivation::new(500), window_daa: 1_000 });
    assert!(too_early.validate_palw_v2().is_err(), "a fence that starves its own draw must not be runnable");

    // Armed past the window: legal — on a registry that can carry the fence at all. The
    // fixture is named for that property so this test keeps asserting the WINDOW rule and not
    // the registry rule beside it (`arming_bond_maturity_needs_a_registry_with_a_spare_seat`
    // owns that one). Since the genesis grew to `seat_count + 3` the two coincide, but they
    // are still two rules and a later shrink must not silently move this test's subject.
    let mut sound = rc_params_with_a_spare_seat();
    sound.palw_bond_maturity = Some(PalwBondMaturityV1 { activation: ForkActivation::new(1_000), window_daa: 1_000 });
    assert!(sound.validate_palw_v2().is_ok(), "armed at or past its own window, the genesis registry is mature");

    // A zero window is the rule switched off while claiming to be on — refused, because a
    // gate that cannot refuse anything is the failure mode this ADR line keeps finding.
    let mut zero = rc_params_with_a_spare_seat();
    zero.palw_bond_maturity = Some(PalwBondMaturityV1 { activation: ForkActivation::new(1_000), window_daa: 0 });
    assert!(zero.validate_palw_v2().is_err(), "a zero window is a fence with nothing behind it");

    // `never()` is not armed at all, so its window is nobody's business.
    let mut never = palw_rc_shipped_params();
    never.palw_bond_maturity = Some(PalwBondMaturityV1 { activation: ForkActivation::never(), window_daa: 1_000 });
    assert!(never.validate_palw_v2().is_ok(), "an unarmed fence starves nothing");
}

/// The shipped RC params, asserted to carry a registry that can arm D1.
///
/// This used to GROW the shipped registry with cloned rows, because the shipped one was the
/// bare `seat_count + 1` and could not carry the fence at all. `PALW_RC_GENESIS_BONDS` now
/// ships `seat_count + 3` real cards, so the fixture IS the shipped preset — and the assertion
/// is what keeps that true. If a later edit shrinks the registry back under the bar, the tests
/// built on this fixture say so here, rather than failing for the window rule they are about.
#[cfg(test)]
fn rc_params_with_a_spare_seat() -> Params {
    let shipped = palw_rc_shipped_params();
    assert!(
        rc_registered_bonds(&shipped) >= crate::palw_fp_devnet_v3::palw_v2_maturity_armable_bonds_v1(),
        "the shipped genesis must keep enough bonds to arm ADR-0065 D1"
    );
    shipped
}

/// The shipped RC params with the bond registry cut back to `n` registrations.
///
/// Surplus rows are dropped and nothing else is touched, so the result differs from the
/// shipped preset in exactly the quantity the D1 arming guard reads. Trimming rather than
/// cloning matters: the rows that remain are the real shipped registrations, so a refusal
/// here is the count being refused and not a synthetic row failing some other check.
#[cfg(test)]
fn rc_params_with_bonds_trimmed_to(n: usize) -> Params {
    use crate::palw_state_v2::PalwConsensusObjectV2;
    let mut trimmed = palw_rc_shipped_params();
    if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &mut trimmed.palw_consensus_mode {
        let mut kept = 0usize;
        b.genesis_objects.retain(|o| {
            if matches!(o, PalwConsensusObjectV2::BondRegistered { .. }) {
                kept += 1;
                kept <= n
            } else {
                true
            }
        });
    }
    assert_eq!(rc_registered_bonds(&trimmed), n, "the fixture must land on the count it was asked for");
    trimmed
}

/// How many bonds a preset registers at genesis.
#[cfg(test)]
fn rc_registered_bonds(p: &Params) -> usize {
    use crate::palw_state_v2::PalwConsensusObjectV2;
    match &p.palw_consensus_mode {
        crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => {
            b.genesis_objects.iter().filter(|o| matches!(o, PalwConsensusObjectV2::BondRegistered { .. })).count()
        }
        _ => 0,
    }
}

/// **PALW, actually on, over mainnet's own identity — the wiring exercised rather than argued.**
///
/// Mainnet ships hash-only, and the honest reason is that a genesis bond registry needs real
/// operator keys and a premine that carries their collateral. Neither can be minted from
/// inside this tree. What CAN be checked from inside the tree is everything between those two
/// constants and a running V2 mainnet, and if that is never exercised then "mainnet is wired
/// for PALW" is a claim about code nobody has run — the shape this audit line keeps finding.
///
/// So this assembles a mainnet-EQUIVALENT: mainnet's own base identity (net, genesis, cadence,
/// the production DNS overlay and its economics), a bond registry sized so ADR-0065 D1 is
/// armable, and a premine built exactly the way mainnet's real one would be — then runs the
/// same assembly and the same genesis gate a booting node runs.
///
/// Both positions, because the dormant half is the one that ships: mainnet as it stands is
/// asserted byte-identical first, so this cannot pass by having quietly turned PALW on.
/// **Mainnet has a path to the economy its own comment describes** (mainnet audit).
///
/// `mainnet_shipped_params` assembled through `palw_v2_params_from_artifacts_on_base` — the
/// builder that takes only the FLOOR's artifact root — while the comment beside the call
/// described ADR-0068's table as "{floor ≈ 20‰, model classes ≈ 980‰}". Those two statements
/// cannot both be true: a carded mainnet came out floor-only, the model tiers did not exist on
/// it, and the 980‰ had nowhere to go. The class-registering assembly was spelled for the RC
/// and only for the RC, so there was no way to say it for mainnet at all.
///
/// Asserted as the DIFFERENCE between the two builders on ONE base, because that difference is
/// the whole content of the defect — and then as the absence of any change to mainnet today,
/// because a fix that moved the shipped preset would be a far bigger event than the bug.
#[test]
fn the_class_registering_assembly_can_be_spelled_for_any_base_not_only_the_rc() {
    use crate::palw_mode_v2::PalwConsensusMode;
    use crate::palw_state_v2::PalwConsensusObjectV2;

    let bonds = || -> Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1> {
        PALW_RC_GENESIS_BONDS
            .iter()
            .map(|c| crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
                bond: crate::palw_state_v2::PalwBondKeyV2(crate::config::premine::premine_outpoint(c.premine_index)),
                pubkey: c.bond_pubkey.to_vec(),
                operator_pubkey: c.operator_pubkey.to_vec(),
                payout_payload: crate::Hash64::from_bytes(c.payout_payload),
            })
            .collect()
    };
    let classes_of = |p: &Params| -> usize {
        let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("V2") };
        bundle.genesis_objects.iter().filter(|o| matches!(o, PalwConsensusObjectV2::ClassRegistered { .. })).count()
    };

    // The floor alone — what mainnet used to get however its card was filled in.
    let floor_only = palw_v2_params_from_artifacts_on_base(palw_rc_base_params(), PALW_RC_GENESIS_ARTIFACT_ROOT, bonds())
        .expect("the floor-only assembly is the one that always worked");
    assert_eq!(classes_of(&floor_only), 1, "one class: the liveness floor, and nothing to be LLM-primary about");

    // The same base, the same floor, plus the tiers — through the variant that takes a base.
    let with_tiers = palw_v2_params_with_classes_on_base(
        palw_rc_base_params(),
        PALW_RC_GENESIS_ARTIFACT_ROOT,
        PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT,
        palw_rc_qwen25_a16_is_registered().then_some(PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT),
        bonds(),
    )
    .expect("the class-registering assembly works on a base it is handed");
    assert!(classes_of(&with_tiers) > classes_of(&floor_only), "the difference between the two builders IS the model tiers");

    // Mainnet's own spelling now exists, and is dormant exactly as its card is.
    assert!(!palw_mainnet_qwen36_is_registered(), "no tier is pinned for mainnet yet — this is a path, not a launch");
    assert!(!palw_mainnet_qwen25_a16_is_registered());
    assert_eq!(
        mainnet_shipped_params().consensus_params_id(),
        MAINNET_PARAMS.consensus_params_id(),
        "and with nothing pinned, mainnet is byte-identical to what it shipped as"
    );
}

/// **ADR-0075 on mainnet is the ruleset's, not a special case.** A mainnet-equivalent genesis
/// (the card stood in by synthetic bonds) assembles through `palw_v2_params_on_base` and so
/// carries the genesis free-prompt-certified set derived by kernel coverage — the floor, the
/// QWEN36 graph-v3 class and the A16 graph-v2 class — under the same `court_e2e_root` the RC
/// networks commit to; the chain half of every gate starts empty, as it must on a new identity.
/// **Every shipped genesis bond is paid at its own key's address, and a seat's funding
/// therefore is something that seat can sign for.**
///
/// The panel resolves its funding through `fee_script`, which for a bonded node returns that
/// bond's `payout_payload` as a script — read from the chain, not derived from the local key.
/// The genesis fee floats pay there, every carrier pays its change back there, and the panel
/// signs every one of those spends with its own ML-DSA-87 key. So a registry row whose payout
/// payload were NOT its bond key's own address would seat a node that can see its money and
/// never spend it: every carrier refused with `script ran, but verification failed`, forever,
/// which is the shape reported as issue #97 from the other direction.
///
/// `kaspad`'s pre-sign guard now refuses such a carrier by name instead of signing it, and
/// that guard is only safe while this holds — a row that broke it would turn a silent wedge
/// into a loud one but would still be a dead seat. The devnet rows below assert the same
/// thing about the derived seeds; this asserts it about the shipped cards, whose keys were
/// generated on eight different hosts and could disagree one row at a time.
///
/// The comment above `PALW_RC_GENESIS_BONDS` already claims it. Nothing ran that claim.
#[test]
fn every_shipped_genesis_bond_is_paid_at_its_own_keys_address() {
    assert!(!PALW_RC_GENESIS_BONDS.is_empty(), "there are cards to check, or this test is vacuous");
    for (n, card) in PALW_RC_GENESIS_BONDS.iter().enumerate() {
        let own: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(card.bond_pubkey).as_bytes();
        assert_eq!(
            own, card.payout_payload,
            "genesis bond card {n} is paid to an address its own bond key cannot spend — that seat can never \
                 fund a carrier"
        );
    }
}

/// **testnet-12's registry is testnet-11's cards 0–6 and a re-keyed card 7 — each paid at its own
/// key's address, one operator per seat.** The same property as the test above, over the table
/// the t12 card and premine actually read, plus the two facts the re-key must not break: the
/// seven carried cards are byte-identical to testnet-11's (the fleet's key files still sign for
/// them), and card 7 shares no key with any other card.
#[test]
fn testnet_12s_registry_is_seven_carried_cards_and_one_new_key() {
    assert_eq!(PALW_T12_GENESIS_BONDS.len(), PALW_RC_GENESIS_BONDS.len(), "the same eight seats");
    for (n, card) in PALW_T12_GENESIS_BONDS.iter().enumerate() {
        assert_eq!(card.premine_index as usize, n, "card {n} sits at its own premine index");
        let own: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(card.bond_pubkey).as_bytes();
        assert_eq!(own, card.payout_payload, "testnet-12 card {n} is paid at its own bond key's address");
        if n < 7 {
            let rc = &PALW_RC_GENESIS_BONDS[n];
            assert!(
                card.bond_pubkey == rc.bond_pubkey
                    && card.operator_pubkey == rc.operator_pubkey
                    && card.payout_payload == rc.payout_payload,
                "card {n} is carried from testnet-11 unchanged"
            );
        }
    }
    let seven = &PALW_T12_GENESIS_BONDS[7];
    assert!(seven.bond_pubkey != PALW_RC_GENESIS_BONDS[7].bond_pubkey, "card 7 carries a new bond key");
    assert!(seven.operator_pubkey != PALW_RC_GENESIS_BONDS[7].operator_pubkey, "card 7 carries a new operator key");
    let operators: std::collections::BTreeSet<&[u8]> = PALW_T12_GENESIS_BONDS.iter().map(|c| c.operator_pubkey).collect();
    let bonds: std::collections::BTreeSet<&[u8]> = PALW_T12_GENESIS_BONDS.iter().map(|c| c.bond_pubkey).collect();
    assert_eq!(operators.len(), 8, "one seat is one operator");
    assert_eq!(bonds.len(), 8, "no two seats sign with one key");
    assert!(seven.bond_pubkey.len() == 2592 && seven.operator_pubkey.len() == 2592, "ML-DSA-87 verification keys");
}

/// **Devnet's genesis bonds come from public seeds and seat a producing chain** (ADR-0075 §7):
/// six bonds, six operators, each key derivable from `palw_devnet_genesis_bond_seed_v1` by
/// anyone, each holding a fee float at its own address — and the assembled devnet passes the
/// startup gate a node runs, so `kaspad --devnet --palw-produce` under any of the six keys
/// makes blocks from genesis without a card of real keys.
#[test]
fn devnet_genesis_bonds_are_public_seeds_that_seat_a_producing_chain() {
    use crate::config::premine::{PALW_DEVNET_GENESIS_BONDS, palw_devnet_genesis_bond_seed_v1, premine_outpoint};
    use crate::palw_mode_v2::PalwConsensusMode;

    let bonds = palw_devnet_genesis_bonds_v1();
    assert_eq!(bonds.len(), PALW_DEVNET_GENESIS_BONDS as usize);
    assert!(bonds.len() >= crate::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1(), "enough seats for a panel");
    let operators: std::collections::BTreeSet<_> = bonds.iter().map(|b| b.operator_pubkey.clone()).collect();
    assert_eq!(operators.len(), bonds.len(), "one seat is one operator");
    for (n, bond) in bonds.iter().enumerate() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(palw_devnet_genesis_bond_seed_v1(n as u32));
        assert_eq!(bond.pubkey, kp.verification_key.as_ref(), "bond {n}'s key is the public seed's");
        let payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(kp.verification_key.as_ref()).as_bytes();
        assert_eq!(bond.payout_payload, crate::Hash64::from_bytes(payload), "and it is paid at its own address");
        assert_eq!(bond.bond.0, premine_outpoint(n as u32), "collateral sits at premine index {n}");
    }
    let params = devnet_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("devnet is V2") };
    let seated = bundle
        .genesis_objects
        .iter()
        .filter(|o| matches!(o, crate::palw_state_v2::PalwConsensusObjectV2::BondRegistered { .. }))
        .count();
    assert_eq!(seated, bonds.len(), "the bundle seats every public-seed bond at genesis");
    params.validate_palw_v2().expect("the startup gate accepts the bonded devnet");
    assert_eq!(params.genesis.hash, DEVNET_PARAMS.genesis.hash, "the bonded premine is what the pinned devnet genesis commits to");
}

#[test]
fn a_mainnet_equivalent_genesis_carries_the_certification_rules() {
    use crate::config::premine::{bonded_genesis_utxos, premine_outpoint};
    use crate::palw_e2e_adjudicability::{palw_rc_court_e2e_root_v1, palw_rc_fp_certified_class_ids_v1};
    use crate::palw_fp_devnet_v3::{palw_devnet_bond_registry_v1, palw_v2_maturity_armable_bonds_v1};
    use crate::palw_mode_v2::PalwConsensusMode;

    let n = palw_v2_maturity_armable_bonds_v1();
    let specs: Vec<_> = palw_devnet_bond_registry_v1(n)
        .into_iter()
        .enumerate()
        .map(|(i, mut spec)| {
            spec.bond = crate::palw_state_v2::PalwBondKeyV2(premine_outpoint(i as u32));
            spec
        })
        .collect();
    let money: Vec<(u32, [u8; 64])> =
        specs.iter().enumerate().map(|(i, spec)| (i as u32, *spec.payout_payload.as_byte_slice())).collect();
    let utxos = bonded_genesis_utxos(MAINNET_PARAMS.net, &money, std::iter::empty());
    let params = palw_v2_params_from_artifacts_on_base_with_utxos(mainnet_v2_mint_base(), PALW_RC_GENESIS_ARTIFACT_ROOT, specs, utxos)
        .expect("a mainnet-equivalent genesis assembles");
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("the mode is V2") };

    let certified = palw_rc_fp_certified_class_ids_v1();
    assert_eq!(certified.len(), 3, "the floor and both model tiers' registered graphs");
    assert_eq!(
        bundle.state.fp_certified_classes(),
        Some(&certified),
        "the genesis free-prompt gate on mainnet is the drilled, coverage-derived set — and it is present, not absent"
    );
    // This compares the bundle's root with the function the assembler set it FROM, so it
    // pins only that the assembler installs the pinned root and no caller can name another.
    // Whether THIS BUILD's court certifies the family set that pin names has two sources and
    // lives where the registry is: `misaka-palw-base0::e2e_drill::tests::
    // the_pinned_rc_e2e_root_is_what_this_build_certifies` (computed root vs the pin).
    assert_eq!(
        bundle.court_e2e_root,
        palw_rc_court_e2e_root_v1(),
        "the assembler installs the pinned attempt-lane e2e root and no caller names another (the build-vs-pin \
             gate is misaka-palw-base0's the_pinned_rc_e2e_root_is_what_this_build_certifies)"
    );
    assert!(
        crate::palw_state_v2::PalwChainStateV2::genesis()
            .chain_certified_families(crate::palw_state_v2::PalwCertifiedLaneV1::Attempt)
            .is_empty(),
        "a new identity's chain set starts empty (ADR-0075 Decision 10)"
    );
    assert_eq!(crate::palw_state_v2::PALW_CERTIFICATION_MAX_PER_BLOCK, 2, "Decision 9's per-block grading cap");
    params.validate_palw_v2().expect("the startup gate accepts the carded mainnet");
}

#[test]
fn a_mainnet_equivalent_genesis_actually_enables_palw() {
    use crate::config::premine::{MISAKA_PREMINE_CAP_SOMPI, bonded_genesis_utxos, premine_outpoint};
    use crate::palw_fp_devnet_v3::{palw_devnet_bond_registry_v1, palw_v2_maturity_armable_bonds_v1};
    use crate::palw_mode_v2::PalwConsensusMode;

    // 1. As shipped: hash-only, and the empty card list is why.
    assert!(matches!(MAINNET_PARAMS.palw_consensus_mode, PalwConsensusMode::Disabled), "mainnet ships PALW off");
    assert!(PALW_MAINNET_GENESIS_BONDS.is_empty(), "…because it has no genesis registry yet");
    assert_eq!(
        mainnet_shipped_params().consensus_params_id(),
        MAINNET_PARAMS.consensus_params_id(),
        "and routing mainnet through the assembly must not move one byte while the card is unset"
    );

    // 2. A registry over mainnet's OWN premine indices, sized so D1 is armable on it.
    let n = palw_v2_maturity_armable_bonds_v1();
    let specs: Vec<_> = palw_devnet_bond_registry_v1(n)
        .into_iter()
        .enumerate()
        .map(|(i, mut spec)| {
            // The outpoint IS the bond's identity, so it has to be the premine output that
            // actually holds the collateral — otherwise the genesis gate cannot resolve it.
            spec.bond = crate::palw_state_v2::PalwBondKeyV2(premine_outpoint(i as u32));
            spec
        })
        .collect();
    let money: Vec<(u32, [u8; 64])> =
        specs.iter().enumerate().map(|(i, spec)| (i as u32, *spec.payout_payload.as_byte_slice())).collect();
    let utxos = bonded_genesis_utxos(MAINNET_PARAMS.net, &money, std::iter::empty());

    // The cap is the one invariant a new carve can silently break, so check it here too.
    let total: u64 = utxos.values().map(|e| e.amount).sum();
    assert_eq!(total, MISAKA_PREMINE_CAP_SOMPI, "a bonded mainnet genesis still mints exactly 10B");

    // 3. The real assembly and the real genesis gate, on mainnet's base.
    let params = palw_v2_params_from_artifacts_on_base_with_utxos(mainnet_v2_mint_base(), PALW_RC_GENESIS_ARTIFACT_ROOT, specs, utxos)
        .expect("a mainnet-equivalent genesis assembles and passes verify_palw_genesis_v2");

    // 4. PALW is genuinely on, and it is still mainnet underneath.
    assert!(matches!(params.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)), "the mode is V2");
    assert_eq!(params.net, MAINNET_PARAMS.net, "…on mainnet's own identity");
    // NOT the shipped genesis HASH: carding is a re-mint (the seats' collateral enters the
    // premine) and the mint is at the V2 ambient target, so the block moves. What must survive
    // is the identity and the economics, which the assertions around this one hold.
    assert_eq!(params.genesis.timestamp, MAINNET_PARAMS.genesis.timestamp, "…built from mainnet's own genesis");
    params.validate_palw_v2().expect("the startup gate a node runs accepts it");
    assert_eq!(
        params.palw_consensus_mode.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2),
        Some(true),
        "the attempt lane is open, which is what 'PALW is enabled' means at block validity"
    );
    // The mainnet economics came along rather than being replaced by the RC's.
    assert_eq!(
        params.dns_params.as_ref().expect("mainnet carries the overlay").min_bond_amount_sompi,
        20_000_000 * SOMPI_PER_KASPA,
        "the production DNS overlay and its bond floor (20M MSK since 2026-09-25) survive the assembly"
    );

    // 5. And D1 is armable here — the property the shipped six-bond registry refuses.
    let mut armed = params.clone();
    armed.palw_bond_maturity = Some(PalwBondMaturityV1 { activation: ForkActivation::new(1_000), window_daa: 1_000 });

    armed.validate_palw_v2().expect("a registry with spare seats may arm the maturity fence");
}

/// **Mainnet's base as a CARDING CEREMONY would mint it**: at the ambient target.
///
/// Carding mainnet is a re-mint — the seats' collateral enters the premine, which moves the
/// genesis — so `bits` is a value that ceremony chooses. `MAINNET_GENESIS` carries the hash
/// lineage's `0x1f7fffff`, one compact exponent (256x) harder than the `0x207fffff` both PALW
/// presets mint at, because a hash chain wants a launch-hashrate ramp and a V2 chain is
/// throttled by the class lottery instead. `validate_palw_v2` now refuses the mismatch, so
/// these equivalents mint the way the ceremony must; before that gate they assembled at the
/// hash lineage's difficulty and nothing said a word.
#[cfg(test)]
fn mainnet_v2_mint_base() -> Params {
    // **The write that used to be here has moved into production** (mainnet audit 2026-09-06,
    // M-7). It performed exactly the mint `palw_v2_params_on_base` omitted, so twelve tests
    // asserted that a mainnet-shaped card assembles while the shipped assembly panicked on
    // the same input. The helper stays — the name says what the base is FOR at eleven call
    // sites — but it now hands the assembly the unmodified preset, so those twelve tests
    // exercise the production mint and fail the moment it is removed again.
    MAINNET_PARAMS
}

/// **A mainnet card, assembled the way a ceremony assembles one.**
///
/// The card is five `const`s a test cannot set, so this supplies the same five facts to
/// `mainnet_card_params_v1` — the function `mainnet_shipped_params` is now a wrapper over. The
/// artifact roots are the RC's, which is the ADR-0042 Decision 11 premise ("RC and mainnet
/// agree by hash"); the bonds are the devnet registry's fabricated cards over mainnet premine
/// indices, which is how `a_mainnet_equivalent_genesis_carries_the_certification_rules`
/// already builds a mainnet-shaped registry without inventing real operator keys.
#[cfg(test)]
fn mainnet_card_fixture_v1(dense: bool) -> Params {
    use crate::config::premine::{bonded_genesis_utxos, premine_outpoint};
    use crate::palw_fp_devnet_v3::{palw_devnet_bond_registry_v1, palw_v2_maturity_armable_bonds_v1};
    let specs: Vec<_> = palw_devnet_bond_registry_v1(palw_v2_maturity_armable_bonds_v1())
        .into_iter()
        .enumerate()
        .map(|(i, mut spec)| {
            spec.bond = crate::palw_state_v2::PalwBondKeyV2(premine_outpoint(i as u32));
            spec
        })
        .collect();
    let money: Vec<(u32, [u8; 64])> =
        specs.iter().enumerate().map(|(i, spec)| (i as u32, *spec.payout_payload.as_byte_slice())).collect();
    let utxos = bonded_genesis_utxos(MAINNET_PARAMS.net, &money, std::iter::empty());
    mainnet_card_params_v1(
        PALW_RC_GENESIS_ARTIFACT_ROOT,
        Some(PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT),
        dense.then_some(PALW_RC_GENESIS_QWEN25_A16_GRAPH_V5_ARTIFACT_ROOT),
        specs,
        utxos,
    )
    .expect("a mainnet card assembles")
}

/// **The mainnet card states testnet-12's carve, and a card that states another fails here**
/// (user decision 2026-09-25, ADR-0126). The 2026-09-25 mainnet-values review (LOW) found that
/// nothing read the card's carve: `worker_carve_permille` 720 → 620 left every consensus-core
/// test green. Validators 20 %, the worker base the lowered split leaves 72 %, and a claim's
/// escrow 720‰ of a 120 s block — 3,200.85 MSK, the G every ADR-0152 tier and the genesis-seat
/// sizing are built on — each exactly testnet-12's.
#[test]
fn the_mainnet_card_states_testnet_12_s_carve_and_escrows_3_200_85_msk_a_claim() {
    let card = mainnet_card_fixture_v1(false);
    let t12 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    let carve = card.palw_overlay_carve;
    assert_eq!(
        carve,
        Some(PalwOverlayCarveV1 { activation: ForkActivation::always(), subsidy_validator_bps: 2_000, worker_carve_permille: 720 }),
        "the card states the carve from genesis: validators 20 %, escrow 720‰"
    );
    assert_eq!(carve, t12.palw_overlay_carve, "testnet-12's carve, field for field");
    let carve = carve.unwrap();
    let dns = card.dns_params.as_ref().expect("a card runs the overlay");
    let lowered = palw_overlay_fee_split_at_v1(dns, Some(carve), dns.full_reward_split_daa_score).expect("the full split is staged");
    assert_eq!(
        (lowered.subsidy_validator_bps, lowered.subsidy_worker_base_bps),
        (2_000, 7_200),
        "the validator pool's subsidy share is 20 % and the worker base the lowered split leaves is 72 %"
    );
    let subsidy = palw_genesis_block_subsidy_sompi(&card);
    assert_eq!(subsidy, PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI, "a card's 120 s block pays what testnet-12's first block pays");
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &card.palw_consensus_mode else {
        panic!("a mainnet card is ConsensusV2")
    };
    assert_eq!(bundle.state.worker_carve_at(subsidy, carve.escrow_carve()), 320_084_650_080, "3,200.85 MSK a claim");
    assert_eq!(
        bundle.state.worker_carve_at(subsidy, None),
        275_628_448_680,
        "without the card's carve the bundle would escrow its own 620‰: 2,756.28 MSK"
    );
}

/// **Every ConsensusV2 ruleset this tree can assemble is minted at its own ambient target.**
///
/// The RULE, not the patch: it names no constant and no network-specific value, so it holds for
/// a network added tomorrow. `validate_palw_v2` refuses anything else, and until the mainnet
/// audit of 2026-09-06 the card assembly could not satisfy it at all — `mainnet_shipped_params()`
/// panicked, and no test assembled a card to find out.
///
/// Fails on the pre-patch tree at the mainnet rows: `mainnet_card_fixture_v1` returns
/// `Err("a ConsensusV2 genesis is minted at a target other than this network's ambient
/// maximum")` and the `expect` inside it panics.
#[test]
fn every_assembled_v2_ruleset_is_minted_at_its_ambient_target() {
    for (name, params) in [
        ("testnet-11", palw_rc_shipped_params()),
        ("devnet", devnet_shipped_params()),
        ("mainnet card (floor + hybrid)", mainnet_card_fixture_v1(false)),
        ("mainnet card (all three tiers)", mainnet_card_fixture_v1(true)),
    ] {
        assert!(
            matches!(params.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)),
            "{name}: the premise — this row must be a V2 network"
        );
        assert_eq!(
            params.genesis.bits,
            params.max_difficulty_target.compact_target_bits(),
            "{name}: a V2 network is throttled by the class lottery, not the hash target, so its genesis is minted at \
                 the ambient maximum (ADR-0066; the gate is validate_palw_v2)"
        );
        params.validate_palw_v2().expect(name);
    }
}

/// **The two shipped V2 presets do not move when the mint is imposed.**
///
/// The other half of the rule above, and the half a fingerprint pin cannot state: the write in
/// `palw_v2_params_on_base` is the IDENTITY on every network that has ever run V2, so no
/// genesis hash and no `consensus_params_id` moves. A future edit that made it conditional, or
/// that changed `MAX_DIFFICULTY_TARGET`, breaks this rather than silently re-genesising a live
/// chain.
#[test]
fn imposing_the_v2_mint_moves_no_live_preset() {
    assert_eq!(palw_rc_shipped_params().genesis.hash, crate::config::genesis::PALW_RC_GENESIS.hash);
    assert_eq!(devnet_shipped_params().genesis.hash, crate::config::genesis::DEVNET_GENESIS.hash);
    assert_eq!(crate::config::genesis::PALW_RC_GENESIS.bits, 0x207fffff);
    assert_eq!(crate::config::genesis::DEVNET_GENESIS.bits, 0x207fffff);
}

/// Every V2 fence a preset can arm, as `(name, armed)`.
///
/// **Derived, not restated** (mainnet audit 2026-09-06, L-3). This was a hand-written `vec![]`
/// of 25 entries — the one fence list in the tree the compiler did not police — and the
/// model-market branch added four fences to `Params` without joining it, with zero merge
/// conflict here, while the audit summary claimed the property "the next fence added cannot be
/// armed on one shipped network and forgotten on another". It reads `Params::palw_fences_v1`
/// now, whose destructure is exhaustive.
#[cfg(test)]
fn palw_v2_fence_table(p: &Params) -> Vec<(&'static str, bool)> {
    p.palw_fences_v1().into_iter().map(|(name, fence)| (name, fence.is_some())).collect()
}

/// **The median-time window and the future-time tolerance describe ONE clock** (ADR-0038
/// Decision H; mainnet audit 2026-09-06, L-1).
///
/// `MEDIAN_TIME_SAMPLED_WINDOW_SIZE` is `(2·T − 1)/PAST_MEDIAN_TIME_SAMPLE_INTERVAL` — a window
/// sized to span twice the tolerance at whatever interval separates two samples. At the V2
/// cadence the sample interval is one 120-second block, not 10 seconds, so the same 27 samples
/// span 54 minutes: the pair is a description of a clock, and the two halves disagree by 12.3x.
///
/// Clause 1 is the identity, re-read from the params rather than from the number 1,620, so it
/// expresses the relation and holds for a cadence chosen tomorrow. Clause 2 is the PINNED GAP:
/// both live V2 presets carry the hash lineage's 132 s against a 54-minute window, and
/// correcting them is a fingerprint move — a flag day or a re-mint, and the operator's call.
#[test]
fn the_v2_median_time_window_and_the_timestamp_tolerance_describe_one_clock() {
    let c = mainnet_card_fixture_v1(true);
    let seconds_per_sample = (c.past_median_time_sample_rate() * c.target_time_per_block() / 1000) as usize;
    assert_eq!(
        c.past_median_time_window_size,
        ((2 * c.timestamp_deviation_tolerance - 1) as usize).div_ceil(seconds_per_sample),
        "a card's window is a window FOR its own tolerance — the config/constants.rs definition, at this cadence"
    );

    // The pinned gap on the two networks that have actually run V2.
    for (name, p) in [("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        let modelled = palw_v2_timestamp_deviation_tolerance_v1(
            p.past_median_time_window_size,
            p.past_median_time_sample_rate(),
            p.target_time_per_block(),
        );
        assert_eq!(p.timestamp_deviation_tolerance, crate::config::constants::consensus::TIMESTAMP_DEVIATION_TOLERANCE);
        assert_eq!(modelled, 1_620, "{name}: 27 samples x 1 x 120 s / 2");
        assert_ne!(
            modelled, p.timestamp_deviation_tolerance,
            "KNOWN GAP: {name} runs a 54-minute median window against a {} s future-time bound. Correcting it is a \
                 fingerprint move and a flag day. If this starts failing, someone re-minted — update the pin deliberately.",
            p.timestamp_deviation_tolerance
        );
    }
}

/// **The fence table names every PALW fence `Params` carries** (mainnet audit 2026-09-06, L-3).
///
/// Deliberately NOT a hand-maintained count: a count is the defect one level up. The property
/// is that the table and the struct agree, and the compiler is what actually enforces it —
/// `palw_fences_v1`'s destructure is exhaustive, so a fence added to `Params` fails to BUILD
/// until it is named. This asserts the consequence, and names the four the DA-court/model-market
/// merge added to `Params` while the hand-written table sat unchanged with zero merge conflict.
#[test]
fn the_fence_table_names_every_palw_fence_on_params() {
    let names: std::collections::BTreeSet<&str> = palw_v2_fence_table(&MAINNET_PARAMS).into_iter().map(|(n, _)| n).collect();
    for (name, _) in MAINNET_PARAMS.palw_fences_v1() {
        assert!(names.contains(name), "{name} is a PALW fence on Params and the table does not name it");
    }
    for name in ["palw_court_ladder", "palw_model_market", "palw_model_lines", "palw_model_evm"] {
        assert!(names.contains(name), "{name}: one of the four the 2026-09-06 merge added");
    }
    // …and the table is a table OF the presets, so it answers per preset rather than per build.
    for (preset_name, preset) in [
        ("testnet-11", palw_rc_shipped_params()),
        ("devnet", devnet_shipped_params()),
        ("mainnet card", mainnet_card_fixture_v1(true)),
    ] {
        let table = palw_v2_fence_table(&preset);
        assert_eq!(table.len(), names.len(), "{preset_name}: one spelling, one length");
        for (name, armed) in table {
            assert_eq!(
                armed,
                preset.palw_fences_v1().into_iter().find(|(n, _)| *n == name).expect("named above").1.is_some(),
                "{preset_name}: the table's {name} must be the params' own answer"
            );
        }
    }
}

/// The names a params set arms, as a set — one spelling for the two assertions below.
#[cfg(test)]
fn card_armed_names(p: &Params) -> std::collections::BTreeSet<&'static str> {
    palw_v2_fence_table(p).into_iter().filter(|(_, on)| *on).map(|(n, _)| n).collect()
}

/// **A carded mainnet arms what testnet-11 arms, and the one difference is a derivation**
/// (mainnet audit, 2026-09-05).
///
/// `docs/adr/README.md` says a carded mainnet "assembles the RC's ruleset … with the same
/// fences armed from genesis". It did not, and the reason was a spelling: ADR-0069 D7,
/// ADR-0082 D3 and ADR-0083 D1 were literals on `palw_rc_base_params`, a base mainnet never
/// passes through, so a carded mainnet was born with the heartbeat lane armed and ADR-0083's
/// repair dormant — the measured 5f wedge, on a chain with value — and with
/// `palw_uncertified_weightless` dormant, which `validate_palw_v2` refuses to arm above
/// genesis, so that hole would have been the chain's for life.
///
/// Asserted as a SET DIFFERENCE rather than field by field: a comparison that has to be
/// extended by hand when a fence is added is a comparison that stops covering the fence nobody
/// remembered — which is the defect itself, one level up.
#[test]
fn a_carded_mainnet_arms_every_fence_testnet_11_arms() {
    use crate::config::premine::{bonded_genesis_utxos, premine_outpoint};
    use crate::palw_fp_devnet_v3::{palw_devnet_bond_registry_v1, palw_v2_maturity_armable_bonds_v1};

    // The mainnet-equivalent card of `a_mainnet_equivalent_genesis_actually_enables_palw`,
    // then the SAME tail `mainnet_shipped_params` runs: assemble, then arm.
    let specs: Vec<_> = palw_devnet_bond_registry_v1(palw_v2_maturity_armable_bonds_v1())
        .into_iter()
        .enumerate()
        .map(|(i, mut spec)| {
            spec.bond = crate::palw_state_v2::PalwBondKeyV2(premine_outpoint(i as u32));
            spec
        })
        .collect();
    let money: Vec<(u32, [u8; 64])> =
        specs.iter().enumerate().map(|(i, spec)| (i as u32, *spec.payout_payload.as_byte_slice())).collect();
    let utxos = bonded_genesis_utxos(MAINNET_PARAMS.net, &money, std::iter::empty());
    let assembled = palw_v2_params_from_artifacts_on_base_with_utxos(
        mainnet_card_base_v1(mainnet_v2_mint_base(), false),
        PALW_RC_GENESIS_ARTIFACT_ROOT,
        specs,
        utxos,
    )
    .expect("a mainnet-equivalent genesis assembles");
    let carded = palw_rc_arm_phase1(mainnet_certify_registered_classes_v1(assembled));

    // What the card STATES on its base (`mainnet_card_base_v1`), individually: the ten below
    // armed from genesis on every card, the k-ary court only when the dense tier is pinned.
    for (name, armed) in [
        ("palw_context_ladder", carded.palw_context_ladder),
        ("palw_court_ladder", carded.palw_court_ladder),
        ("palw_capability_bound", carded.palw_capability_bound),
        ("palw_receipt_rows_unpriced", carded.palw_receipt_rows_unpriced),
        ("palw_attempt_header_pins", carded.palw_attempt_header_pins),
        ("palw_fp_da_pins", carded.palw_fp_da_pins),
        ("palw_validator_payout_bounds", carded.palw_validator_payout_bounds),
        ("palw_certification_rent", carded.palw_certification_rent),
        ("palw_chunk_cap_charge", carded.palw_chunk_cap_charge),
        ("palw_prompt_ids_merkle", carded.palw_prompt_ids_merkle),
        ("palw_panel_da", carded.palw_panel_da),
        ("palw_da_court", carded.palw_da_court),
    ] {
        assert_eq!(armed, Some(ForkActivation::always()), "a card states {name} from genesis");
    }
    assert!(carded.palw_kary_court.is_none(), "…and the court only with the dense tier, which this floor-only card does not pin");

    let rc = palw_rc_shipped_params();
    let rc_armed: Vec<&'static str> = palw_v2_fence_table(&rc).into_iter().filter(|(_, on)| *on).map(|(n, _)| n).collect();
    let mainnet_armed: std::collections::BTreeSet<&'static str> =
        palw_v2_fence_table(&carded).into_iter().filter(|(_, on)| *on).map(|(n, _)| n).collect();
    let missing: Vec<&'static str> = rc_armed.iter().copied().filter(|n| !mainnet_armed.contains(n)).collect();

    // The legitimate differences, and each is legitimate for a stated reason rather than by
    // having been forgotten:
    //
    // * `palw_kary_court` is DERIVED from the class set — it is the court for a fused-attention
    //   row, this equivalent registers only the floor, and `validate_palw_v2` refuses the fence
    //   on a ruleset whose frozen arity does not match one it can derive. A carded mainnet that
    //   pins the dense tier states it on the base — see the test below.
    // * `palw_model_seed_v2` (ADR-0120) is the market's least seed, for the same reason: a card
    //   states what opening a pair costs when it states the market.
    // * `palw_model_leg_v2` (ADR-0114) is a fee schedule on the market those fences arm: a card
    //   states the market's economics when it states the market, and none does yet.
    // * `palw_model_benefits` (ADR-0095) follows the three below for the same reason and one
    //   more of its own: a membership is a promise a LINE makes, and a card that armed it from
    //   genesis would be arming a promise mechanism on a network whose lines do not exist yet.
    //   It is scheduled on testnet-11 at `PALW_RC_MODEL_BENEFITS_FENCE_DAA` and stated by no
    //   card, deliberately.
    // * The three model fences (ADR-0087 D6, ADR-0088 D11, ADR-0089 D9) are SCHEDULED on
    //   testnet-11, at `PALW_RC_DA_COURT_FENCE_DAA`, and a card states none of them. Arming a
    //   market, a registry and an EVM from block one on a network with value is the measured
    //   decision ADR-0087 D6 reserves for the operator, not a default a card takes; the mainnet
    //   row of `docs/adr/README.md`'s activation table records them as not armed. **This
    //   difference was invisible until the 2026-09-06 audit (L-3): the fence table was
    //   hand-written and the model branch never joined it, so a set comparison could not see a
    //   fence the table did not name.** If a card is ever meant to arm them, this is the line
    //   that has to be changed deliberately.
    // * `palw_prefill_draw` (ADR-0117) is armed on testnet-11 on the audit's flag day and stated
    //   by no card: which job a draw runs is a class decision a card makes with its classes,
    //   after the one-forward job has run on a live chain.
    // * `dns_bft_gate` and `palw_execution_lane` are testnet-11's 6,001 flag day (ADR-0128,
    //   ADR-0125) and no card states them yet: a card's gate numbers (its leak period at its own
    //   cadence) and its lane's width and stage table are genesis decisions the operator has not
    //   taken. (`palw_overlay_carve`, ADR-0126, rode the same day and left this list on
    //   2026-09-25: the user took testnet-12's numbers for mainnet, so a card states the 20 %
    //   validator share and the 720‰ escrow from genesis, as testnet-12 does, rather than
    //   minting a split the shared bundle builder would carry to every network.)
    // * `palw_held_context` and the one-move court `palw_shard_court` (ADR-0103, ADR-0100) are
    //   testnet-11's held regime, scheduled at `PALW_RC_HELD_FENCE_DAA` by ADR-0118. A mainnet
    //   that wants the regime MINTS it — `palw_held_context_mint_v1`, V4 and the tiled ids from
    //   genesis — and whether one does is a product decision no card has taken. (The regime's
    //   third fence, the retention pins, a card already states.)
    // * `palw_model_registry` and `palw_economic_payout` (ADR-0135 Upgrade A, ADR-0132 Upgrade C)
    //   ride testnet-11's 6,001 flag day; a card states the registry's globals and the payout's
    //   rate from genesis once the operator has calibrated them on the testnet.
    // * `palw_work_target` and `palw_short_challenge_window` (ADR-0137, ADR-0132 §7.6) ride the
    //   6,001 / 7,101 flag day; a card states them with the payout's rate.
    // * `palw_single_lottery`, `palw_anchor_clock` and `palw_clock_cursor` (ADR-0132 S, ADR-0138,
    //   ADR-0142) ride testnet-11's own [`PALW_RC_ANCHOR_CLOCK_FENCE_DAA`] day, past the live
    //   tip so committed `daaScore` values stay valid; a card states them with the work target.
    // * `palw_verification_v2` (ADR-0133 S1) rides testnet-11's own 6,100 day; a card states its
    //   verification rule when it states the registry.
    // * `palw_objective_offence` (ADR-0144 §9), `palw_execution_quanta` and
    //   `palw_seat_gate_possession` (ADR-0133 §7) are testnet-11's own later days, each past the
    //   DAA clock: a lock ledger's slashing amounts, a permit mint's rate, and a registry's
    //   choice to admit a heavy class rarely rather than refuse it are all calibrations the
    //   operator takes on the testnet first, and no card states them.
    // * `palw_execution_lane_span_short` (ADR-0130) is the lane's own latency day at 7,300; a
    //   card states one span, not a schedule that shortens it.
    assert_eq!(
        missing,
        vec![
            "dns_bft_gate",
            "palw_model_registry",
            "palw_economic_payout",
            "palw_work_target",
            "palw_single_lottery",
            "palw_short_challenge_window",
            "palw_verification_v2",
            "palw_verification_s3",
            "palw_verification_s2",
            "palw_readiness_v2",
            "palw_anchor_clock",
            "palw_clock_cursor",
            "palw_objective_offence",
            "palw_seat_gate_possession",
            "palw_class_receipt_window",
            "palw_execution_quanta",
            "palw_execution_lane",
            "palw_execution_lane_span_short",
            "palw_model_market",
            "palw_model_lines",
            "palw_model_benefits",
            "palw_model_leg_v2",
            "palw_model_seed_v2",
            "palw_model_evm",
            "palw_kary_court",
            "palw_shard_court",
            "palw_held_context",
            "palw_prefill_draw"
        ],
        "a carded mainnet must arm every fence testnet-11 arms except the four named above \
             (rc: {rc_armed:?}, mainnet: {mainnet_armed:?})"
    );
    // **And one exception that shows up in neither list, because neither side arms it.**
    // ADR-0130's `palw_panel_exposure_floor` is dormant on testnet-11 (the operator's direction
    // of 2026-09-17: `λ` stays OFF at the 6,001 flag day, and the shadow read that measures what
    // it would require is node-local) and stated by no card yet. The user chose mainnet's λ on
    // 2026-09-25 — 5, from the 5×–10× range ADR-0130 recorded — and testnet-12 arms it; a card
    // states it together with the R-core+ duty that reads it, which no card carries today.
    // Named here so a card that ever states it has to change this sentence.
    assert!(
        carded.palw_panel_exposure_floor.is_none() && rc.palw_panel_exposure_floor.is_none(),
        "ADR-0130's seat exposure floor is stated by neither testnet-11 nor a card (testnet-12 arms λ = 5)"
    );
    for name in ["palw_model_market", "palw_model_lines", "palw_model_evm"] {
        assert!(
            rc.palw_fences_v1().into_iter().any(|(n, fence)| n == name && fence.is_some_and(|f| f.daa_score() > 0)),
            "{name}: the RC SCHEDULES it at a height — a card that does not state it is behind a decision, not an \
                 oversight"
        );
    }
    // **The card's armed set, spelled out — and the document is downstream of it** (mainnet
    // audit 2026-09-06, L-4). Deliberately a hand-maintained list, unlike everything above it:
    // its job is not to catch a fence the compiler already catches (`palw_fences_v1` does that),
    // it is to make a change to what a CARD arms impossible to land without a human reading
    // this sentence — `docs/adr/README.md`'s activation-axis table is written FROM this set, and
    // nothing runs a document.
    let card_armed: std::collections::BTreeSet<&str> = card_armed_names(&carded);
    let expected: std::collections::BTreeSet<&str> = [
        "palw_attempt_header_pins",
        "palw_attempt_work",
        // The 2026-09-11 pre-mainnet audit's fences: a card arms both from genesis (correct
        // there), testnet-11 schedules the shallow one at PALW_RC_AUDIT_FENCE_DAA and the deep
        // one at PALW_RC_AUDIT_DEEP_FENCE_DAA.
        "palw_audit_2026_09_11",
        "palw_audit_2026_09_11_deep",
        "palw_capability_bound",
        "palw_certification_rent",
        "palw_chunk_cap_charge",
        // The lead cap (the 2026-09-25 mainnet-values review, HIGH): stated beside the 1,620 s
        // tolerance a card derives, as testnet-12 arms it; testnet-11 runs 132 s and leaves it off.
        "palw_clock_lead_cap",
        "palw_compute_overlay_retired",
        "palw_context_ladder",
        "palw_court_ladder",
        "palw_da_court",
        "palw_difficulty_priced_rows",
        "palw_epoch_boundary_budget",
        "palw_fp_da_pins",
        "palw_fp_ruleset_caps",
        "palw_heartbeat",
        // ADR-0126 (user decision 2026-09-25): a card is born with the 20 % validator share and
        // the 720‰ escrow testnet-12 runs; testnet-11 reaches it by its 6,001 flag day.
        "palw_overlay_carve",
        "palw_panel_da",
        // ADR-0124: a card is born paying its panel out of the claim's reward, its seats
        // holding exposure, and its claims priced by the compute they certify; testnet-11
        // reaches both by a flag day or not at all.
        "palw_panel_economy",
        "palw_prompt_ids_merkle",
        "palw_receipt_rows_unpriced",
        "palw_signature_contexts_v2",
        // B-2 (mainnet audit 2026-09-11): a card counts only Final work in the share census
        // (ADR-0107), armed here and dormant on testnet-11.
        "palw_share_growth_final",
        "palw_unavailable_abstains",
        "palw_uncertified_weightless",
        "palw_validator_payout_bounds",
        "palw_work_priced_reward",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        card_armed, expected,
        "the card's armed set changed. docs/adr/README.md's activation-axis table is written FROM this set; update it \
             in the same commit"
    );
    // The dense card is this set plus the court its own pinned row needs and the two fences
    // that complete that court (AC-D8: ADR-0093 D6/D8), and nothing else.
    let dense: std::collections::BTreeSet<&str> = card_armed_names(&mainnet_card_fixture_v1(true));
    assert_eq!(
        dense.difference(&expected).copied().collect::<Vec<_>>(),
        vec!["palw_attn_anchored_root", "palw_fused_dissectable", "palw_kary_court"],
        "pinning the dense tier states the k-ary court and the two fences (D6/D8) that complete it, and nothing else"
    );

    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &carded.palw_consensus_mode else {
        panic!("the equivalent is V2")
    };
    assert_eq!(
        crate::palw_court_v2::palw_attn_widest_registered_site_v2(bundle).0,
        0,
        "…and it is the floor-only case: no fused row is registered, so no dissection court is owed"
    );

    // The two the audit found, named individually as well — a set assertion that later grows a
    // permitted exception must not quietly take these with it.
    assert!(
        carded.palw_difficulty_priced_rows.is_some(),
        "ADR-0083: the heartbeat lane is armed from genesis on mainnet, so the retarget must count only \
             bits-priced rows or the clock prices the bonded lane off its own chain"
    );
    assert!(carded.palw_heartbeat.is_some(), "…and the lane that makes that necessary is indeed armed");
    assert_eq!(
        carded.palw_uncertified_weightless,
        Some(ForkActivation::always()),
        "ADR-0069 D7 is genesis-only: a mainnet minted without it can never acquire it"
    );
    carded.validate_palw_v2().expect("the startup gate a mainnet node runs accepts the armed card");
}

/// **The complete context set is a CARD's rule, and testnet-11's identity does not move for it**
/// (ADR-0042 Decision 11; mainnet audit 2026-09-06, M-8).
///
/// Both halves, because either alone is a different and wrong outcome: the card must actually
/// commit to the complete set, and every network that already exists must be byte-identical to
/// a build in which this fence was never added.
///
/// Clause (b) is the assertion `shipped_presets_have_pinned_fingerprints` cannot make — it has
/// no testnet-11 row, because that ruleset is a function of pinned genesis artifacts — and it is
/// the whole reason the complete set is fenced rather than appended to
/// `PALW_V2_SIGNATURE_CONTEXTS`.
#[test]
fn the_complete_context_set_is_carded_and_leaves_every_live_network_where_it_was() {
    use crate::palw_mode_v2::{
        PALW_V2_FROZEN_SIGNATURE_CONTEXTS_ROOT_HEX, PalwConsensusMode, palw_v2_signature_contexts_root,
        palw_v2_signature_contexts_root_v2,
    };

    // (a) Dormant on every shipped preset, and dormant is byte-identical to absent.
    for (name, preset) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("devnet", DEVNET_PARAMS), ("simnet", SIMNET_PARAMS)]
    {
        assert!(preset.palw_signature_contexts_v2.is_none(), "{name}: a shipped preset armed the complete context set");
        let before = preset.consensus_params_id();
        let mut probe = preset;
        probe.palw_signature_contexts_v2 = None;
        assert_eq!(before, probe.consensus_params_id(), "{name}: the Some-only write ran on a None");
    }

    // (b) THE LIVE NETWORKS. testnet-11 and devnet keep the frozen root, so their ruleset ids
    //     are unmoved.
    for (name, live) in [("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        assert!(live.palw_signature_contexts_v2.is_none(), "{name}: arming this re-mints a live chain");
        let PalwConsensusMode::ConsensusV2(bundle) = &live.palw_consensus_mode else {
            panic!("{name} is a ConsensusV2 network");
        };
        // Against the LITERAL golden, not against `palw_v2_signature_contexts_root()`: that
        // function moves with the array, so comparing to it agrees with any append and is the
        // shape of test M-8 was raised about. This is the value the live chains committed to.
        assert_eq!(
            bundle.signature_contexts_root.to_string(),
            PALW_V2_FROZEN_SIGNATURE_CONTEXTS_ROOT_HEX,
            "{name}'s committed context set moved: that re-mints a live chain's consensus_params_id, which is a \
                 coordinated flag day and not an edit"
        );
        assert_eq!(bundle.signature_contexts_root, palw_v2_signature_contexts_root(), "{name}: and this build agrees");
        assert_ne!(bundle.signature_contexts_root, palw_v2_signature_contexts_root_v2());
    }

    // (c) A card states it from genesis and its BUNDLE carries the complete root — the two
    //     halves `validate_palw_v2` refuses to separate, checked together so a card that armed
    //     the fence without the assembly writing the root could not pass.
    let carded = mainnet_card_fixture_v1(false);
    assert_eq!(carded.palw_signature_contexts_v2, Some(ForkActivation::always()), "a card states the complete set from genesis");
    let PalwConsensusMode::ConsensusV2(card_bundle) = &carded.palw_consensus_mode else {
        panic!("a carded mainnet is a ConsensusV2 network");
    };
    assert_eq!(card_bundle.signature_contexts_root, palw_v2_signature_contexts_root_v2(), "…and its bundle commits to it");
    assert!(carded.validate_palw_v2().is_ok());

    // (d) Neither half stands alone.
    let mut fence_only = carded.clone();
    if let PalwConsensusMode::ConsensusV2(b) = &mut fence_only.palw_consensus_mode {
        b.signature_contexts_root = palw_v2_signature_contexts_root();
    }
    assert!(fence_only.validate_palw_v2().is_err(), "an armed fence over the frozen root must be refused");
    let mut root_only = carded.clone();
    root_only.palw_signature_contexts_v2 = None;
    assert!(root_only.validate_palw_v2().is_err(), "the complete root without the fence must be refused");

    // (e) …and it is genesis-only: the meaning of a signature is not a function of a height no
    //     verifier of one holds.
    let mut scheduled = carded.clone();
    scheduled.palw_signature_contexts_v2 = Some(ForkActivation::new(5_000));
    assert!(scheduled.validate_palw_v2().is_err(), "the set of contexts a ruleset commits to is not a height");

    // (f) `never()` is absence, whole option and all — the collapse every bare fence keeps.
    //     Observable through `consensus_identity_id`, which is where the normalizer runs, and
    //     a spelled-out dormant testnet-11 is still testnet-11 to the startup gate.
    let rc = palw_rc_shipped_params();
    let mut spelled_out = rc.clone();
    spelled_out.palw_signature_contexts_v2 = Some(ForkActivation::never());
    assert_eq!(
        spelled_out.consensus_identity_id(),
        rc.consensus_identity_id(),
        "never() is absence, or the collapse in normalize_values_a_scheduled_fence_drags_with_it is gone"
    );
    assert!(spelled_out.validate_palw_v2().is_ok(), "…and a dormant spelling is still testnet-11");
}

/// **The liveness proof a card's arming cannot ship without** — the card's own registry, not
/// the RC's (ADR-0071 SA-3; mainnet audit 2026-09-06, M-14).
///
/// This is `palw_panel_v2::the_shipped_registry_still_draws_a_full_panel_with_the_capability_fence_armed`
/// asked of the network that actually arms the fence. A short draw is
/// `InsufficientEligibleBonds`, so every claim voids at `BindTimeout` with its escrow burned,
/// and a card's registry has zero slack by construction (`seat_count + 1` bonds, the executor
/// excluded). Arming a fence that empties the panel is worse than leaving it dormant, so the
/// arming and this assertion are one change.
#[test]
fn a_carded_registry_still_draws_a_full_panel_with_the_capability_fence_armed() {
    use crate::palw_mode_v2::PalwConsensusMode;
    use crate::palw_state_v2::{
        PalwBlockContextV2, PalwChainStateV2, apply_palw_transition_v2, palw_bond_may_judge_class_v3, palw_bond_may_take_work_v2,
    };

    let params = mainnet_card_fixture_v1(true);
    assert_eq!(params.palw_capability_bound, Some(ForkActivation::always()), "the premise: the card arms SA-3");
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("the card is V2") };
    let sp = bundle.state.clone();
    let genesis_ctx =
        PalwBlockContextV2 { block: params.genesis.hash, daa_score: params.genesis.daa_score, blue_score: 0, subsidy: 0 };
    let (booted, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &genesis_ctx, &bundle.genesis_objects, None)
        .expect("the card's genesis applies");

    let seats_needed = bundle.panel.seat_count() as usize;
    let min_collateral = sp.min_collateral_sompi();
    let classes: Vec<crate::Hash64> = booted.classes_iter().map(|(id, _)| *id).collect();
    assert_eq!(classes.len(), 3, "the card registers the floor and both model tiers");

    for class_id in classes {
        let count = |proof: bool| -> usize {
            let mut operators: Vec<crate::Hash64> = booted
                .bonds_iter()
                .filter(|(key, bond)| {
                    palw_bond_may_take_work_v2(bond, min_collateral)
                        && palw_bond_may_judge_class_v3(&booted, key, bond, &class_id, proof)
                })
                .map(|(_, bond)| bond.operator_id)
                .collect();
            operators.sort();
            operators.dedup();
            operators.len()
        };
        assert_eq!(
            count(false),
            count(true),
            "class {class_id:?}: the capability fence removed a seat from the CARD's genesis registry — every claim on \
                 that network would void at BindTimeout"
        );
        assert!(count(true) > seats_needed, "class {class_id:?}: a panel of {seats_needed} needs one spare for the executor");
    }

    // **SA-2's price is affordable at genesis**, which is the other way this arming could brick
    // a card: each bond declares the floor plus both tiers, so it reserves
    // 3 x PALW_CAPABILITY_EXPOSURE_SOMPI against its own collateral before it has done
    // anything. If a future card registers more tiers than a genesis bond can pay for, this is
    // the line that says so rather than a node that will not boot.
    for (_, bond) in booted.bonds_iter() {
        assert!(bond.capable_classes.len() <= crate::palw_state_v2::PALW_MAX_CAPABLE_CLASSES);
        assert!(
            crate::palw_state_v2::palw_bond_capability_exposure_v1(bond) <= bond.collateral as u128,
            "a genesis bond's declared capability must be affordable against its own collateral (SA-2)"
        );
    }
}

/// **A ruleset must be able to pay its own validator floor inside ONE coinbase** (mainnet
/// audit, 2026-09-06, H-3).
///
/// The §E deferred fan-out is (epochs this block finalizes) × (validators included in each),
/// and `epochs_finalized_at` returns one epoch per `epoch_length_blocks` of DAA a block
/// advanced — up to `mergeset_size_limit / epoch_length_blocks`. A network whose own
/// `min_active_validators` cannot be paid inside its own coinbase cap halts permanently the
/// first time a wide mergeset appears, and the block that would merge the wide tips away is
/// itself unbuildable. This is a startup-shaped property checked at BUILD time, deliberately
/// NOT a `validate_palw_v2` refusal: testnet-11 is live and dormant here, and a runtime gate
/// would stop it booting.
#[test]
fn a_carded_mainnet_can_pay_its_own_validator_floor_inside_one_coinbase() {
    use crate::config::premine::{bonded_genesis_utxos, premine_outpoint};
    use crate::palw_fp_devnet_v3::{palw_devnet_bond_registry_v1, palw_v2_maturity_armable_bonds_v1};

    // The same mainnet-equivalent card `a_carded_mainnet_arms_every_fence_testnet_11_arms`
    // builds — assembled, then armed.
    let specs: Vec<_> = palw_devnet_bond_registry_v1(palw_v2_maturity_armable_bonds_v1())
        .into_iter()
        .enumerate()
        .map(|(i, mut spec)| {
            spec.bond = crate::palw_state_v2::PalwBondKeyV2(premine_outpoint(i as u32));
            spec
        })
        .collect();
    let money: Vec<(u32, [u8; 64])> =
        specs.iter().enumerate().map(|(i, spec)| (i as u32, *spec.payout_payload.as_byte_slice())).collect();
    let utxos = bonded_genesis_utxos(MAINNET_PARAMS.net, &money, std::iter::empty());
    let assembled = palw_v2_params_from_artifacts_on_base_with_utxos(
        mainnet_card_base_v1(mainnet_v2_mint_base(), false),
        PALW_RC_GENESIS_ARTIFACT_ROOT,
        specs,
        utxos,
    )
    .expect("a mainnet-equivalent genesis assembles");
    let carded = palw_rc_arm_phase1(mainnet_certify_registered_classes_v1(assembled));
    assert_eq!(carded.palw_validator_payout_bounds, Some(ForkActivation::always()), "a card states the payout bounds");

    // The premise, in this ruleset's OWN numbers: the fan-out a single block can owe.
    let dns = carded.dns_params.as_ref().expect("the DNS overlay");
    let epochs_per_block = carded.blockrate.mergeset_size_limit / dns.epoch_length_blocks.max(1) + 1;
    let fanout = (dns.min_active_validators as u64).saturating_mul(epochs_per_block);
    let reserved = crate::palw_state_v2::PALW_V2_MAX_DEFERRED_VALIDATOR_PAYOUTS;
    assert!(
        reserved > 0 && fanout > reserved,
        "the premise: this card's own floor of {} validators over {epochs_per_block} finalizable epochs per block \
             wants {fanout} outputs against {reserved} reserved slots, which is why the fan-out has to be BOUNDED per \
             block rather than reserved for",
        dns.min_active_validators,
    );

    // …and the reserved slots do fit inside the cap the isolation guard applies past the fence.
    assert!(
        reserved + crate::palw_state_v2::PALW_V2_MAX_RESERVE_DRIP_PAYOUTS
            <= crate::palw_state_v2::PALW_V2_COINBASE_EXTRA_OUTPUTS_BOUNDED,
        "the two per-block fan-outs must sit inside the widened coinbase allowance"
    );

    // testnet-11 is live and leaves the fence dormant, so its fan-out is still unbounded
    // against a cap of `mergeset_size_limit + 1 + PALW_V2_COINBASE_EXTRA_OUTPUTS`. Pinned so
    // the gap can neither widen nor close in silence — closing it there is a flag day and must
    // be a decision, not a diff.
    let rc = palw_rc_shipped_params();
    assert!(rc.palw_validator_payout_bounds.is_none(), "testnet-11 leaves it dormant: arming it is the operator's call");
    assert!(rc.palw_validator_payout_bounds_fence().is_none(), "the reader is the one place the rule is decided");
    for p in [&MAINNET_PARAMS, &TESTNET_PARAMS, &SIMNET_PARAMS, &DEVNET_PARAMS] {
        assert!(p.palw_validator_payout_bounds.is_none(), "{}: a shipped preset states no payout-bounds fence", p.net);
    }
    assert!(devnet_shipped_params().palw_validator_payout_bounds.is_none(), "devnet inherits the dormant state too");
}

/// **A data-availability court may not be armed over a class that could not answer it**
/// (ADR-0062 SA-2; mainnet audit 2026-09-06, C-5).
///
/// SA-2's own sentence is "a class whose event cannot be disclosed inside the budget is refused
/// at admission rather than being undefendable afterwards", and the genesis route had no door:
/// a ruleset could arm the court over a class whose commitment scheme no disclosure form names,
/// which is a class every accusation defeats by silence for the price of one bond.
///
/// The rule under test is the RULESET's, not the patch's: it is asked of the same fact
/// `check_trace_event_disclosure_v1` refuses on — the registered `logits_scheme_id` — so a
/// class this gate admits is exactly a class that checker could be handed an answer for. Both
/// directions are pinned, because a gate that refuses everything is as wrong as one that
/// refuses nothing: every shipped ruleset must still assemble.
#[test]
fn arming_the_da_court_over_an_undisclosable_class_refuses_to_assemble() {
    use crate::Hash64;
    use crate::palw_mode_v2::PalwConsensusMode;
    use crate::palw_state_v2::PalwConsensusObjectV2;

    // **Nothing that ships stops shipping.** testnet-11 schedules the court over a class set
    // that is entirely flat-or-tiled, devnet arms no court at all, and a card that pins the
    // dense tier arms it from genesis over the fused row.
    let rc = palw_rc_shipped_params();
    assert!(rc.palw_da_court.is_some(), "testnet-11 schedules the data-availability court, so the rule engages there");
    rc.validate_palw_v2().expect("testnet-11 assembles");
    devnet_shipped_params().validate_palw_v2().expect("devnet assembles");
    let carded = palw_rc_arm_phase1(
        palw_v2_params_with_classes_on_base(
            mainnet_card_base_v1(mainnet_v2_mint_base(), true),
            PALW_RC_GENESIS_ARTIFACT_ROOT,
            PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT,
            Some(PALW_RC_GENESIS_QWEN25_A16_GRAPH_V5_ARTIFACT_ROOT),
            vec![],
        )
        .expect("a dense-pinned mainnet card assembles"),
    );
    assert_eq!(carded.palw_da_court, Some(ForkActivation::always()), "a card arms the court from genesis");
    carded.validate_palw_v2().expect("a dense-pinned card assembles");

    // **A class committing under a third scheme is refused, by name.** The mutation is the
    // honest one: the carriage's profile IS the class, so its id moves with it.
    let mut mutated = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &mut mutated.palw_consensus_mode else { panic!("V2") };
    let mut renamed = None;
    for object in bundle.genesis_objects.iter_mut() {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(carriage), .. } = object {
            carriage.profile.logits_scheme_id = Hash64::from_u64_word(0x5CE3);
            *class_id = carriage.profile.shape_profile_id();
            renamed = Some(*class_id);
            break;
        }
    }
    let renamed = renamed.expect("testnet-11 registers at least one class that carries its own profile");
    let err = mutated.validate_palw_v2().expect_err(
        "a ruleset arming the data-availability court over a class whose commitments no disclosure form names must \
             not assemble: every accusation against it would win by silence",
    );
    let text = format!("{err}");
    assert!(text.contains(&renamed.to_string()), "the refusal names the class it is about: {text}");
    assert!(text.contains("scheme no disclosure form names"), "…and says what is wrong with it: {text}");

    // **The gate reaches every row it can resolve, and it can resolve all of testnet-11's** —
    // otherwise the assertions above would be about a smaller set than they read as.
    let PalwConsensusMode::ConsensusV2(bundle) = &rc.palw_consensus_mode else { panic!("V2") };
    let resolved = crate::palw_e2e_adjudicability::palw_registered_class_profiles_v1(bundle);
    assert!(resolved.len() >= 2, "this test proves little on a genesis registering one class: {}", resolved.len());
    assert!(
        resolved.iter().all(|(_, profile)| profile.is_some()),
        "testnet-11's registered classes are all describable from the ruleset, so the rule above covers all of them"
    );
}

/// **THE LIMIT, pinned so it can neither widen nor close silently** (mainnet audit 2026-09-06,
/// C-5): the responder-coverage gate above judges the rows this ruleset can DESCRIBE, and a
/// genesis class whose profile lives only in the committed catalog is not one of them.
///
/// `validate_palw_v2` holds `class_catalog_root` — a commitment, not a preimage — so for a
/// genesis registration (`admission: None`) the only profile source is the typed catalog in
/// `palw_e2e_adjudicability`, which carries the three rows this tree ships and nothing else.
/// The designed repair refused such a class outright; that would have refused
/// `palw_rc_params` over any catalog of the caller's own, which is a real, booting network
/// whose classes may be perfectly disclosable. Refusing what cannot be judged is fail-closed
/// only where the judgment was possible.
///
/// This test asserts the residue rather than the fix: a class the ruleset cannot describe
/// assembles, and it is documented as covered by whoever supplied the catalog. The day
/// `validate_palw_v2` can reach the catalog preimage, this test goes red — which is the
/// trigger to close the gap rather than to re-derive this reasoning.
#[test]
fn a_catalog_class_this_ruleset_cannot_describe_is_not_refused_here() {
    use crate::Hash64;
    use crate::palw_mode_v2::PalwConsensusMode;
    use crate::palw_state_v2::PalwConsensusObjectV2;

    let mut orphaned = palw_rc_shipped_params();
    assert!(orphaned.palw_da_court.is_some(), "the gate this is a limit of is engaged on this ruleset");
    let PalwConsensusMode::ConsensusV2(bundle) = &mut orphaned.palw_consensus_mode else { panic!("V2") };
    let mut unknown = None;
    for object in bundle.genesis_objects.iter_mut() {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, admission: None, .. } = object {
            *class_id = Hash64::from_u64_word(0xDEAD_C1A5);
            unknown = Some(*class_id);
            break;
        }
    }
    let unknown = unknown.expect("testnet-11 registers at least one catalog class");
    let PalwConsensusMode::ConsensusV2(bundle) = &orphaned.palw_consensus_mode else { panic!("V2") };
    assert!(
        crate::palw_e2e_adjudicability::palw_registered_class_profiles_v1(bundle)
            .into_iter()
            .any(|(id, profile)| id == unknown && profile.is_none()),
        "the premise: this ruleset now registers a class it holds no profile for"
    );
    assert!(
        orphaned.validate_palw_v2().is_ok(),
        "a class this seam cannot describe is passed over, not refused — see the gate's own comment"
    );
}

/// **The court prosecutes at the ladder it admitted at — where the fence says so** (ADR-0084
/// U-08; mainnet audit, 2026-09-05).
///
/// On every shipped preset the bundle freezes `max_step_leaf_count = 2^26` and the walkers'
/// constant is `2^22`. `palw_court_ladder` is ADR-0084 §7.5's fence for that loosening — and it
/// is NOT `palw_context_ladder`, which after the 2026-09-06 merge is ADR-0077 Phase B's
/// admission-rule fence and nothing else.
///
/// **Every arming below is READ OFF the params, never handed in.** The version this replaces
/// passed a literal `true` for devnet and hand-wrote the fence onto a fixture for the card, so
/// it asserted a property of its own arguments: with the card arming the OTHER field it still
/// passed, while a carded mainnet prosecuted its dense row at `2^22` (mainnet audit 2026-09-06,
/// M-15).
#[test]
fn the_refutation_ladder_is_the_rulesets_only_past_the_court_ladder_fence() {
    use crate::palw_court_v2::palw_refutation_leaf_cap_v2;
    use crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;

    let court_of = |p: &Params| match &p.palw_consensus_mode {
        crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b.court,
        _ => panic!("V2"),
    };
    let rc = palw_rc_shipped_params();
    let devnet = devnet_shipped_params();
    assert!(
        court_of(&rc).max_step_leaf_count() > PALW_STEP_LEG_MAX_LEAVES,
        "the premise: the ruleset admits deeper than the walkers' constant ({} vs {PALW_STEP_LEG_MAX_LEAVES})",
        court_of(&rc).max_step_leaf_count()
    );

    // **testnet-11 SCHEDULES it** (2026-09-06 audit H-4 / ADR-0084 U-08), on the same flag day
    // as the DA court. Below the height the walkers keep the executor's constant — the court
    // every close filed so far was judged under, which is why this cannot simply be armed — and
    // at it they take the ruleset's own ladder. A live chain reaches a rule at a height.
    let t11_ladder = rc.palw_court_ladder.expect("testnet-11 schedules the court ladder");
    assert_eq!(
        t11_ladder,
        ForkActivation::new(PALW_RC_COURT_LADDER_FENCE_DAA),
        "the ladder has a height of its own — the fork-id gate digests heights, not fence sets"
    );
    assert!(!t11_ladder.is_active(PALW_RC_COURT_LADDER_FENCE_DAA - 1), "…dormant the block before it");
    assert!(t11_ladder.is_active(PALW_RC_COURT_LADDER_FENCE_DAA), "…in force at it");
    assert_eq!(
        palw_refutation_leaf_cap_v2(&court_of(&rc), t11_ladder.is_active(PALW_RC_COURT_LADDER_FENCE_DAA - 1)),
        PALW_STEP_LEG_MAX_LEAVES,
        "before the height the walkers take the executor's constant"
    );
    assert_eq!(
        palw_refutation_leaf_cap_v2(&court_of(&rc), t11_ladder.is_active(PALW_RC_COURT_LADDER_FENCE_DAA)),
        court_of(&rc).max_step_leaf_count(),
        "at the height they take the ruleset's, which is what closes U-08 on this chain"
    );
    // **devnet: the fence is DORMANT, and this pins the gap rather than papering it.** Devnet
    // arms `palw_context_ladder` from genesis and leaves `palw_court_ladder` unset, so it
    // admits the graph-v5 row at 2^26 and its court still walks at 2^22. Arming it here would
    // move devnet's pinned fingerprint, which is a re-mint and the operator's call; recording
    // it is not. If devnet is ever re-minted with the fence, this assertion is the line that
    // says so.
    assert!(devnet.palw_court_ladder.is_none(), "devnet leaves ADR-0084 U-08 dormant — a known, pinned gap");
    assert_eq!(
        palw_refutation_leaf_cap_v2(&court_of(&devnet), devnet.palw_court_ladder_active_at(0)),
        PALW_STEP_LEG_MAX_LEAVES,
        "devnet's court walks at the constant while its classes are admitted at {}",
        court_of(&devnet).max_step_leaf_count()
    );
    // …and the fence, when a network does arm it, hands over the ruleset's own ladder.
    assert_eq!(palw_refutation_leaf_cap_v2(&court_of(&devnet), true), court_of(&devnet).max_step_leaf_count());

    // **A carded mainnet, through the card's OWN base statement** — not a fixture with the
    // fence written onto it. `mainnet_card_fixture_v1` runs `mainnet_card_base_v1` and the same
    // tail `mainnet_shipped_params` runs, so this measures what a mint produces.
    let carded = mainnet_card_fixture_v1(true);
    assert_eq!(carded.palw_court_ladder, Some(ForkActivation::always()), "a card states ADR-0084 U-08's fence");
    assert_eq!(
        palw_refutation_leaf_cap_v2(&court_of(&carded), carded.palw_court_ladder_active_at(0)),
        court_of(&carded).max_step_leaf_count(),
        "a carded mainnet prosecutes the dense row at the ladder it admitted it at"
    );
    assert!(
        court_of(&carded).max_step_leaf_count() > PALW_STEP_LEG_MAX_LEAVES,
        "the premise: without the fence this card's court could not try the row it registers"
    );
}

/// **A mainnet card that pins the dense tier can state the court that tries it.**
///
/// Both arms, because the failing one is the shipped behaviour this fixes: with
/// `MAINNET_PARAMS`' `palw_kary_court: None`, a card pinning ADR-0082's graph-v5 row did not
/// ship a court it could not run — `mainnet_shipped_params` panicked at startup, and the card
/// had no field with which to say the word.
#[test]
fn the_mainnet_base_can_state_the_court_the_dense_tier_needs() {
    let dense = Some(PALW_RC_GENESIS_QWEN25_A16_GRAPH_V5_ARTIFACT_ROOT);
    let err = palw_v2_params_with_classes_on_base(
        mainnet_v2_mint_base(),
        PALW_RC_GENESIS_ARTIFACT_ROOT,
        PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT,
        dense,
        vec![],
    )
    .expect_err("mainnet's base leaves palw_kary_court dormant, and a fused row may not be registered without it");
    assert!(
        format!("{err:?}").contains("palw_kary_court"),
        "the refusal names the fence the card has to state, not something downstream: {err:?}"
    );

    let mut base = mainnet_v2_mint_base();
    base.palw_kary_court = Some(ForkActivation::always());
    let params =
        palw_v2_params_with_classes_on_base(base, PALW_RC_GENESIS_ARTIFACT_ROOT, PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT, dense, vec![])
            .expect("with the court stated, the dense tier assembles on mainnet's own base");
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("V2") };
    assert!(
        crate::palw_court_v2::palw_attn_widest_registered_site_v2(bundle).0 > 0,
        "…and the row it registers is the fused one the court exists for"
    );
    palw_rc_arm_phase1(params);
}

/// **AC-D8 (A-3): a dense card arms ADR-0093 Decisions 6 and 8 with the court they complete.**
///
/// The dense-tier card arms `palw_kary_court` from genesis; the court's bottom on the
/// per-position dense row needs Decision 8's anchored root claim, and Decision 6 refuses a
/// fused class no dissection can try. Both are free from genesis on a fresh card; the test
/// shows that first, then asserts the card states them.
#[test]
fn a_dense_card_arms_the_anchored_root_and_the_dissectable_fence_from_genesis() {
    let card = mainnet_card_fixture_v1(true);
    assert_eq!(card.palw_kary_court, Some(ForkActivation::always()), "the dense card runs the k-ary court from genesis");
    let mut armed = card.clone();
    armed.palw_attn_anchored_root = Some(ForkActivation::always());
    armed.palw_fused_dissectable = Some(ForkActivation::always());
    armed.validate_palw_v2().expect("both fences arm from genesis on the dense card");
    assert_eq!(
        card.palw_attn_anchored_root,
        Some(ForkActivation::always()),
        "the dense card must arm ADR-0093 Decision 8 (palw_attn_anchored_root) with the court"
    );
    assert_eq!(
        card.palw_fused_dissectable,
        Some(ForkActivation::always()),
        "the dense card must arm ADR-0093 Decision 6 (palw_fused_dissectable) with the court"
    );
}

/// **A carded mainnet runs the DAG shape testnet-11 runs, and what still differs is named**
/// (mainnet audit, 2026-09-05).
///
/// ADR-0042 Decision 11's promise is that the RC's ruleset IS mainnet's. Two quantities that
/// are functions of the 120 s cadence were not moved by `with_two_minute_cadence`, so a carded
/// mainnet ran a shape no V2 network has ever run: a 661-sample difficulty window (22 h of
/// memory, against the 8.8 h the constant was sized for) and 16 block parents at
/// `ghostdag_k = 1`, against 10. `palw_v2_params_on_base` now imposes both, in the same place
/// and for the same reason it imposes the cadence and re-derives the depths.
///
/// The residue is asserted rather than described, so a future divergence has to come here to
/// be explained.
#[test]
fn a_carded_mainnet_runs_the_v2_dag_shape_and_names_what_still_differs() {
    use crate::config::premine::{bonded_genesis_utxos, premine_outpoint};
    use crate::palw_fp_devnet_v3::{palw_devnet_bond_registry_v1, palw_v2_maturity_armable_bonds_v1};

    let specs: Vec<_> = palw_devnet_bond_registry_v1(palw_v2_maturity_armable_bonds_v1())
        .into_iter()
        .enumerate()
        .map(|(i, mut spec)| {
            spec.bond = crate::palw_state_v2::PalwBondKeyV2(premine_outpoint(i as u32));
            spec
        })
        .collect();
    let money: Vec<(u32, [u8; 64])> =
        specs.iter().enumerate().map(|(i, spec)| (i as u32, *spec.payout_payload.as_byte_slice())).collect();
    let utxos = bonded_genesis_utxos(MAINNET_PARAMS.net, &money, std::iter::empty());
    let m = palw_rc_arm_phase1(
        palw_v2_params_from_artifacts_on_base_with_utxos(mainnet_v2_mint_base(), PALW_RC_GENESIS_ARTIFACT_ROOT, specs, utxos)
            .expect("a mainnet-equivalent genesis assembles"),
    );
    let r = palw_rc_shipped_params();
    let d = devnet_shipped_params();

    let shape = |p: &Params| {
        (
            p.blockrate.target_time_per_block,
            p.blockrate.ghostdag_k,
            p.blockrate.max_block_parents,
            p.blockrate.mergeset_size_limit,
            p.blockrate.merge_depth,
            p.blockrate.coinbase_maturity,
            p.blockrate.past_median_time_sample_rate,
            p.blockrate.difficulty_sample_rate,
            p.difficulty_window_size,
            p.min_difficulty_window_size,
        )
    };
    assert_eq!(shape(&m), shape(&r), "a carded mainnet runs testnet-11's cadence-derived DAG shape");
    assert_eq!(shape(&d), shape(&r), "…and so does the devnet the drills run, which is what makes it a rehearsal");
    assert_eq!(
        (m.difficulty_window_size, m.blockrate.max_block_parents),
        (PALW_V2_DIFFICULTY_WINDOW_SIZE, BlockrateParams::new_two_minute_bps().max_block_parents),
        "the two the audit found, named individually: 264 samples (~8.8 h at 120 s) and 10 parents"
    );

    // What is left, deliberately. `max_block_level` is a property of the hash lineage's block
    // levels and its pruning proof, not a function of the cadence, so it is mainnet's own —
    // named here so that the difference is a decision on the record rather than a discovery
    // after the mint. The rest of mainnet's identity (net, genesis, seeders, the production DNS
    // overlay and its bond floor, EVM off) is deliberate and covered by the tests above.
    assert_eq!((m.max_block_level, r.max_block_level), (225, 250), "the one shape difference that is not the cadence's");
    assert_eq!(m.net, MAINNET_PARAMS.net, "…on mainnet's own identity");
    assert_eq!(m.genesis.timestamp, MAINNET_PARAMS.genesis.timestamp, "…built from mainnet's own genesis");
    assert_eq!(
        m.genesis.bits,
        m.max_difficulty_target.compact_target_bits(),
        "and minted at the ambient target, which is what `validate_palw_v2` now requires of a V2 genesis"
    );
}

/// **A V2 genesis minted above the ambient target is refused** (mainnet audit, 2026-09-05).
///
/// Both arms on one preset, because a gate that refused everything would also pass the refusal
/// half on its own: testnet-11 as shipped is the accepting arm, and the same ruleset with the
/// hash lineage's genesis `bits` is the refusing one.
#[test]
fn a_v2_genesis_minted_above_the_ambient_target_is_refused() {
    let rc = palw_rc_shipped_params();
    assert_eq!(
        rc.genesis.bits,
        rc.max_difficulty_target.compact_target_bits(),
        "the shipped V2 presets already mint at the ambient target — this gate names what they do"
    );
    rc.validate_palw_v2().expect("…so the shipped ruleset passes it");

    let mut harder = rc.clone();
    harder.genesis.bits = MAINNET_PARAMS.genesis.bits;
    assert_ne!(
        harder.genesis.bits,
        harder.max_difficulty_target.compact_target_bits(),
        "the premise: 0x1f7fffff is not the ambient target"
    );
    let e = harder.validate_palw_v2().expect_err("a V2 network minted at the hash lineage's difficulty is refused");
    assert!(
        format!("{e:?}").contains("ambient maximum"),
        "the refusal names what is wrong with the mint, not something downstream: {e:?}"
    );

    // And the same gate is why `mainnet_v2_mint_base` exists: mainnet's shipped genesis is the
    // one that fails it, which is the finding.
    assert_ne!(
        MAINNET_PARAMS.genesis.bits,
        MAINNET_PARAMS.max_difficulty_target.compact_target_bits(),
        "mainnet's shipped genesis is minted for the hash lineage, 256x above a V2 network's ambient target"
    );
}

/// **The receipt lane leaves the difficulty count where the fence says so** (mainnet audit,
/// 2026-09-05). Dormant on testnet-11 (a live chain, ADR-0083's path (a) is the operator's),
/// stated from genesis on a card — and refused anywhere its first fence is not in force.
#[test]
fn the_receipt_rows_fence_is_stated_on_a_card_and_needs_the_first_fence() {
    let rc = palw_rc_shipped_params();
    assert!(rc.palw_receipt_rows_unpriced.is_none(), "testnet-11 leaves it dormant: a scheduled height is the operator's call");
    assert_eq!(rc.palw_difficulty_priced_rows, Some(ForkActivation::new(1150)), "…on a chain whose first fence fired at 1150");

    // Scheduled after the first fence: legal. Before it, or with the first dormant: refused.
    let mut later = rc.clone();
    later.palw_receipt_rows_unpriced = Some(ForkActivation::new(2000));
    later.validate_palw_v2().expect("a second fence scheduled after the first is a legal schedule");
    let mut earlier = rc.clone();
    earlier.palw_receipt_rows_unpriced = Some(ForkActivation::new(1000));
    let e = earlier.validate_palw_v2().expect_err("the second fence cannot fire before the first");
    assert!(format!("{e:?}").contains("palw_receipt_rows_unpriced"), "{e:?}");
    let mut alone = rc.clone();
    alone.palw_difficulty_priced_rows = None;
    alone.palw_receipt_rows_unpriced = Some(ForkActivation::always());
    alone.validate_palw_v2().expect_err("the second fence cannot stand alone");

    // A card: both from genesis, and the fork-id schedule carries neither (genesis fences are
    // the identity's business, not the handshake's).
    let base = mainnet_card_base_v1(mainnet_v2_mint_base(), false);
    let carded = palw_rc_arm_phase1(
        palw_v2_params_from_artifacts_on_base(base, PALW_RC_GENESIS_ARTIFACT_ROOT, vec![]).expect("a zero-seat card assembles"),
    );
    assert_eq!(carded.palw_receipt_rows_unpriced, Some(ForkActivation::always()));
    assert_eq!(carded.palw_difficulty_priced_rows, Some(ForkActivation::always()));
    assert!(crate::fork_id_v1::fork_id_gate_fences_v1(&carded).is_empty(), "nothing scheduled: everything is at genesis");
    // testnet-11's own DA-court fence rides along on this fixture, which starts from the
    // shipped preset — the assertion is about the receipt fence's height being listed, not
    // about it being the only one.
    assert_eq!(
        crate::fork_id_v1::fork_id_gate_fences_v1(&later),
        vec![
            1150,
            PALW_RC_DA_COURT_FENCE_DAA,
            2000,
            PALW_RC_COURT_LADDER_FENCE_DAA,
            PALW_RC_MODEL_BENEFITS_FENCE_DAA,
            PALW_RC_MODEL_LEG_V2_FENCE_DAA,
            PALW_RC_AUDIT_FENCE_DAA,
            // One entry for 6,000: the deep-audit fence and the held regime share it (ADR-0118; 7,000 until 2026-09-17).
            // ADR-0120's 6,900, where the deployed release put it — now BELOW the moved flag day.
            PALW_RC_MODEL_SEED_V2_FENCE_DAA,
            PALW_RC_AUDIT_DEEP_FENCE_DAA,
            // One entry for 6,001: ADR-0124, 0125, 0126, 0128 and 0130 share it.
            PALW_RC_PALW_UPGRADE_FENCE_DAA,
            // ADR-0133 Verification V2's own day, a hundred past the flag day.
            PALW_RC_VERIFICATION_V2_FENCE_DAA,
            // ADR-0130 latency: the execution span shortens 5 DAA → 1 DAA.
            PALW_RC_EXECUTION_SPAN_SHORT_FENCE_DAA,
            // ADR-0134's retirement, after the flag day.
            PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA,
            // ADR-0138 / ADR-0142: the DAA clock, past the live tip.
            PALW_RC_ANCHOR_CLOCK_FENCE_DAA,
            // The days past the clock, in height order: the permit mint, ADR-0133 §7's seat
            // gate, §11.3's class receipt window, ADR-0144's lock ledger, and ADR-0133's S3
            // and S2.
            PALW_RC_EXECUTION_QUANTA_FENCE_DAA,
            PALW_RC_SEAT_GATE_POSSESSION_FENCE_DAA,
            PALW_RC_CLASS_RECEIPT_WINDOW_FENCE_DAA,
            PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA,
            PALW_RC_VERIFICATION_S3_FENCE_DAA,
            PALW_RC_VERIFICATION_S2_FENCE_DAA,
        ],
        "…and a schedule lists every scheduled gate fence's height"
    );
    // The two 6,000 fences ship in one release only while they are one height: moving either
    // constant alone would put a fence at a height the other's release does not schedule.
    assert_eq!(PALW_RC_HELD_FENCE_DAA, Some(PALW_RC_AUDIT_DEEP_FENCE_DAA), "the held regime and the deep audit are one flag day");
}

/// **The header-stage DA pins are stated on a card, dormant on every live chain, and cost
/// nothing while they are dormant** (mainnet audit, 2026-09-06 — C-1).
///
/// Three separate claims, because a bare fence can fail any one of them on its own:
/// `None` everywhere shipped, so no live chain's fingerprint moves and no header it already
/// holds is re-judged; `Some(never())` is absence, so a build that spells the dormant state the
/// long way fingerprints identically to one that omits the field; and arming it moves the
/// identity, so an armed build and an un-armed one cannot silently believe they run one rule.
/// The fourth claim is the fail-closed one: the pins are fields of a V2 attempt envelope, so
/// the fence is refused on a network that has no V2 bundle to derive them from.
#[test]
fn the_attempt_header_pins_fence_is_dormant_on_every_shipped_preset_and_visible_the_moment_it_is_not() {
    for p in [&MAINNET_PARAMS, &TESTNET_PARAMS, &SIMNET_PARAMS, &DEVNET_PARAMS] {
        assert!(p.palw_attempt_header_pins.is_none(), "{}: a shipped preset states no header-stage pin fence", p.net);
    }
    let rc = palw_rc_shipped_params();
    assert!(
        rc.palw_attempt_header_pins.is_none(),
        "testnet-11's stored history was judged with the DA pins asked only in the virtual processor; arming this \
             fence there is a decision and a flag day, not an inheritance"
    );
    assert!(devnet_shipped_params().palw_attempt_header_pins.is_none(), "devnet inherits the dormant state too");

    // `Some(never())` is absence — the D4 collapse, observable through `consensus_identity_id`,
    // which is where the normalizer runs (`consensus_params_id` hashes the raw field and never
    // normalizes).
    let mut spelled_out = rc.clone();
    spelled_out.palw_attempt_header_pins = Some(ForkActivation::never());
    assert_eq!(
        spelled_out.consensus_identity_id(),
        rc.consensus_identity_id(),
        "Some(never()) is absence, or the collapse in normalize_values_a_scheduled_fence_drags_with_it is gone"
    );

    // A FUTURE height must not split the mesh on deploy day: `for_each_fence` visits the fence,
    // so a scheduled height normalises away in the identity and shows up in the schedule the
    // operator log carries. That is the whole reason the field is visited rather than only
    // hashed.
    let mut scheduled = rc.clone();
    scheduled.palw_attempt_header_pins = Some(ForkActivation::new(9_000_000));
    assert_eq!(
        scheduled.consensus_identity_id(),
        rc.consensus_identity_id(),
        "an armed and an un-armed build must peer until the height fires (SA-2's deploy-day partition)"
    );
    assert_ne!(scheduled.consensus_params_id(), rc.consensus_params_id(), "…while the ruleset id still names the rule");
    assert_ne!(scheduled.consensus_schedule_id(), rc.consensus_schedule_id(), "…and the operator log names the height");

    // …and in force FROM GENESIS is a real rule difference, which must separate identities.
    let mut armed = rc.clone();
    armed.palw_attempt_header_pins = Some(ForkActivation::always());
    assert_ne!(armed.consensus_params_id(), rc.consensus_params_id(), "arming a validity fence moves the ruleset id");
    assert_ne!(armed.consensus_identity_id(), rc.consensus_identity_id(), "…and at genesis the handshake must see it");
    armed.validate_palw_v2().expect("the RC is ConsensusV2, so the fence has a lane to apply to");

    // Fail-closed: no V2 bundle, no pins to enforce.
    let mut laneless = TESTNET_PARAMS;
    laneless.palw_attempt_header_pins = Some(ForkActivation::always());
    let e = laneless.validate_palw_v2().expect_err("a fence that hashes and cannot fire is refused at assembly");
    assert!(format!("{e:?}").contains("palw_attempt_header_pins"), "{e:?}");
    // …and the dormant spelling is not a rule, so it is accepted there.
    let mut laneless_dormant = TESTNET_PARAMS;
    laneless_dormant.palw_attempt_header_pins = Some(ForkActivation::never());
    laneless_dormant.validate_palw_v2().expect("never() names no rule on any network");

    // A card states it from genesis, where it is free: no history was judged under the looser
    // rule. It is a PARTIAL answer to C-1 — see the field's doc and
    // `palw_attempt_v2::tests::an_unadmitted_attempt_header_is_still_a_priced_difficulty_row`.
    let base = mainnet_card_base_v1(mainnet_v2_mint_base(), false);
    let carded = palw_rc_arm_phase1(
        palw_v2_params_from_artifacts_on_base(base, PALW_RC_GENESIS_ARTIFACT_ROOT, vec![]).expect("a zero-seat card assembles"),
    );
    assert_eq!(carded.palw_attempt_header_pins, Some(ForkActivation::always()));
}

/// **The free-prompt DA pin is dormant on every shipped preset but testnet-11's held flag day,
/// and visible the moment it is armed** (ADR-0072 Decision 8, free-prompt half; mainnet audit
/// 2026-09-06 — M-3; ADR-0118 for the height).
///
/// The same four claims the header-stage fence above is held to, because a bare fence can fail
/// any one of them alone: `None` everywhere shipped, so no live chain's fingerprint moves and
/// no claim it already recorded is re-derived; `Some(never())` is absence; arming it separates
/// identities; and it is refused on a network with no V2 lane to write a claim record on.
#[test]
fn the_free_prompt_da_pin_fence_is_dormant_on_every_shipped_preset_and_visible_the_moment_it_is_not() {
    for p in [&MAINNET_PARAMS, &TESTNET_PARAMS, &SIMNET_PARAMS, &DEVNET_PARAMS] {
        assert!(p.palw_fp_da_pins.is_none(), "{}: a shipped preset states no free-prompt DA pin fence", p.net);
    }
    let rc = palw_rc_shipped_params();
    assert_eq!(
        rc.palw_fp_da_pins,
        PALW_RC_HELD_FENCE_DAA.map(ForkActivation::new),
        "testnet-11's stored claims recorded the retention their own producers wrote; deriving it there is a decision \
             and a flag day, not an inheritance — the held regime's (ADR-0118), which needs it at or below its height"
    );
    assert!(devnet_shipped_params().palw_fp_da_pins.is_none(), "devnet inherits the dormant state too");
    // Dormant means the fence reader says so as well, which is what the transition asks.
    assert_eq!(
        rc.palw_fp_da_pins_fence(),
        PALW_RC_HELD_FENCE_DAA.map(ForkActivation::new),
        "the reader is the one place the rule is decided"
    );

    let mut spelled_out = rc.clone();
    spelled_out.palw_fp_da_pins = Some(ForkActivation::never());
    assert_eq!(spelled_out.consensus_identity_id(), rc.consensus_identity_id(), "Some(never()) is absence");

    let mut scheduled = rc.clone();
    scheduled.palw_fp_da_pins = Some(ForkActivation::new(9_000_000));
    assert_eq!(
        scheduled.consensus_identity_id(),
        rc.consensus_identity_id(),
        "an armed and an un-armed build must peer until the height fires"
    );
    assert_ne!(scheduled.consensus_params_id(), rc.consensus_params_id(), "…while the ruleset id still names the rule");
    assert_ne!(scheduled.consensus_schedule_id(), rc.consensus_schedule_id(), "…and the operator log names the height");

    let mut armed = rc.clone();
    armed.palw_fp_da_pins = Some(ForkActivation::always());
    assert_ne!(armed.consensus_params_id(), rc.consensus_params_id(), "arming a state-root fence moves the ruleset id");
    assert_ne!(armed.consensus_identity_id(), rc.consensus_identity_id(), "…and at genesis the handshake must see it");
    armed.validate_palw_v2().expect("the RC is ConsensusV2, so the fence has a lane to apply to");
    assert_eq!(armed.palw_fp_da_pins_fence(), Some(ForkActivation::always()));

    // Fail-closed: no V2 bundle, no claim record to write an obligation onto.
    let mut laneless = TESTNET_PARAMS;
    laneless.palw_fp_da_pins = Some(ForkActivation::always());
    let e = laneless.validate_palw_v2().expect_err("a fence that hashes and cannot fire is refused at assembly");
    assert!(format!("{e:?}").contains("palw_fp_da_pins"), "{e:?}");
    let mut laneless_dormant = TESTNET_PARAMS;
    laneless_dormant.palw_fp_da_pins = Some(ForkActivation::never());
    laneless_dormant.validate_palw_v2().expect("never() names no rule on any network");

    // A card states it from genesis, beside the `palw_da_court` it is what makes meaningful.
    let base = mainnet_card_base_v1(mainnet_v2_mint_base(), false);
    let carded = palw_rc_arm_phase1(
        palw_v2_params_from_artifacts_on_base(base, PALW_RC_GENESIS_ARTIFACT_ROOT, vec![]).expect("a zero-seat card assembles"),
    );
    assert_eq!(carded.palw_fp_da_pins, Some(ForkActivation::always()));
    assert_eq!(carded.palw_da_court, Some(ForkActivation::always()), "the court it makes meaningful is armed with it");
}

/// **ADR-0124's two fences are scheduled on testnet-11's 6,001 flag day and dormant on every other
/// shipped preset, visible the moment they are set, meaningless without a V2 bundle, and stated
/// from genesis on a mainnet card.** A scheduled height moves the fingerprint and the schedule and
/// never the identity — an armed and an un-armed build peer until the height fires — and arming
/// from genesis moves the identity.
#[test]
fn adr0124_the_panel_economy_and_the_work_price_are_scheduled_on_testnet_11_and_stated_on_a_card() {
    for p in [&MAINNET_PARAMS, &TESTNET_PARAMS, &SIMNET_PARAMS, &DEVNET_PARAMS] {
        assert!(p.palw_panel_economy.is_none() && p.palw_work_priced_reward.is_none(), "{}: a shipped preset states neither", p.net);
    }
    let shipped = palw_rc_shipped_params();
    let flag_day = Some(ForkActivation::new(PALW_RC_PALW_UPGRADE_FENCE_DAA));
    assert_eq!(
        (shipped.palw_panel_economy_fence(), shipped.palw_work_priced_reward_fence()),
        (flag_day, flag_day),
        "testnet-11 pays seats and prices work from its 6,001 flag day"
    );
    assert!(shipped.palw_seat_economy_at(PALW_RC_PALW_UPGRADE_FENCE_DAA - 1).is_none(), "no seat floor below the height");
    assert!(shipped.palw_seat_economy_at(PALW_RC_PALW_UPGRADE_FENCE_DAA).is_some(), "and one from it");
    // The build before the flag day, which the scheduled and genesis variants below are compared with.
    let mut rc = shipped.clone();
    rc.palw_panel_economy = None;
    rc.palw_work_priced_reward = None;
    assert!(rc.palw_seat_economy_at(u64::MAX).is_none(), "no seat floor while the fence is dormant");
    assert!(devnet_shipped_params().palw_panel_economy.is_none() && devnet_shipped_params().palw_work_priced_reward.is_none());

    for field in [
        (|p: &mut Params, a: Option<ForkActivation>| p.palw_panel_economy = a) as fn(&mut Params, Option<ForkActivation>),
        |p: &mut Params, a: Option<ForkActivation>| p.palw_work_priced_reward = a,
    ] {
        let mut spelled_out = rc.clone();
        field(&mut spelled_out, Some(ForkActivation::never()));
        assert_eq!(spelled_out.consensus_identity_id(), rc.consensus_identity_id(), "Some(never()) is absence");

        let mut scheduled = rc.clone();
        field(&mut scheduled, Some(ForkActivation::new(9_000_000)));
        // The registry and the payout ride the economy's height; a moved economy moves them off.
        scheduled.palw_model_registry = None;
        scheduled.palw_verification_v2 = None;
        scheduled.palw_verification_s3 = None;
        scheduled.palw_verification_s2 = None;
        scheduled.palw_readiness_v2 = None;
        scheduled.palw_economic_payout = None;
        scheduled.palw_work_target = None;
        scheduled.set_palw_single_lottery(None);
        assert_eq!(
            scheduled.consensus_identity_id(),
            rc.consensus_identity_id(),
            "an armed and an un-armed build peer until the height"
        );
        assert_ne!(scheduled.consensus_params_id(), rc.consensus_params_id(), "…while the fingerprint names the rule");
        assert_ne!(scheduled.consensus_schedule_id(), rc.consensus_schedule_id(), "…and the operator log names the height");
        scheduled.validate_palw_v2().expect("armable on the RC");

        let mut armed = rc.clone();
        field(&mut armed, Some(ForkActivation::always()));
        assert_ne!(armed.consensus_identity_id(), rc.consensus_identity_id(), "at genesis the handshake must see it");
    }

    // Past the fence the seat floor is ten producer floors and the ceiling is the bundle's.
    let mut armed = rc.clone();
    armed.palw_panel_economy = Some(ForkActivation::always());
    let economy = armed.palw_seat_economy_at(0).expect("armed");
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &armed.palw_consensus_mode else { unreachable!() };
    assert_eq!(economy.panel_floor_sompi, bundle.state.min_collateral_sompi() * 10);
    assert_eq!(economy.max_exposure_ratio_permille, bundle.state.fp_max_exposure_ratio_permille());

    // Fail-closed: no V2 bundle, no panel to pay and no work to price.
    let mut hash_only = MAINNET_PARAMS;
    hash_only.palw_panel_economy = Some(ForkActivation::always());
    hash_only.palw_work_priced_reward = Some(ForkActivation::always());
    assert!(hash_only.palw_panel_economy_fence().is_none() && hash_only.palw_work_priced_reward_fence().is_none());

    // A mainnet card states both from genesis.
    let card = mainnet_card_base_v1(MAINNET_PARAMS, false);
    assert_eq!(card.palw_panel_economy, Some(ForkActivation::always()));
    assert_eq!(card.palw_work_priced_reward, Some(ForkActivation::always()));

    // And a card's producer floor is 13,000 MSK, so its seat floor is 130,000 MSK — the
    // operator's mainnet numbers — while the RC's bundle keeps its policy floor (0.004 MSK).
    let carded = mainnet_card_fixture_v1(false);
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(card_bundle) = &carded.palw_consensus_mode else { unreachable!() };
    assert_eq!(card_bundle.state.min_collateral_sompi(), crate::palw_fp_devnet_v3::PALW_MAINNET_MIN_COLLATERAL_SOMPI);
    assert_eq!(card_bundle.state.min_collateral_sompi(), 13_000 * crate::constants::SOMPI_PER_KASPA);
    assert_eq!(
        carded.palw_seat_economy_at(0).expect("a card states the economy").panel_floor_sompi,
        130_000 * crate::constants::SOMPI_PER_KASPA
    );
    assert_eq!(bundle.state.min_collateral_sompi(), crate::palw_fp_devnet_v3::PALW_POLICY_MIN_COLLATERAL_SOMPI);
    assert_eq!(bundle.state.min_collateral_sompi(), 400_000, "testnet-11's floor and fingerprint do not move");
}

/// **ADR-0130's seat exposure floor is dormant on every shipped preset AND on every card, refused
/// wherever it could not floor anything, and Some-only in both ids.**
///
/// A card states it nowhere on purpose, and that is the one exception this file's
/// carded-mainnet parity keeps by name: `λ` for a mainnet is the operator's open decision
/// (5–10× is the range under discussion), and a card that picked one would be minting an
/// economics constant nobody has chosen. testnet-11 leaves it dormant too — the operator's
/// direction of 2026-09-17 is that `λ` stays OFF at the 6,001 flag day.
#[test]
fn adr0130_the_seat_exposure_floor_is_dormant_everywhere_and_refused_where_it_cannot_floor() {
    for p in [&MAINNET_PARAMS, &TESTNET_PARAMS, &TESTNET11_PARAMS, &SIMNET_PARAMS, &DEVNET_PARAMS] {
        assert!(p.palw_panel_exposure_floor.is_none(), "{}: a shipped preset states no exposure floor", p.net);
    }
    let rc = palw_rc_shipped_params();
    assert!(rc.palw_panel_exposure_floor.is_none(), "testnet-11's 6,001 flag day arms the economy, not the floor");
    assert!(devnet_shipped_params().palw_panel_exposure_floor.is_none());
    for dense in [false, true] {
        assert!(
            mainnet_card_fixture_v1(dense).palw_panel_exposure_floor.is_none(),
            "a card states no λ: the multiple a mainnet seat reserves is the operator's open decision"
        );
    }
    assert!(rc.palw_panel_exposure_floor_fence().is_none(), "the reader is the one place the rule is decided");
    assert_eq!(rc.palw_panel_reward_multiple_permille_at(u64::MAX), 0, "no floor is no multiple, at any height");
    assert_eq!(
        rc.palw_seat_economy_at(PALW_RC_PALW_UPGRADE_FENCE_DAA).expect("the economy is armed there").reward_multiple_permille,
        0,
        "the draw past the economy prices a seat at the claim's exposure while the floor is dormant"
    );

    let armed = |activation: ForkActivation, reward_multiple_permille: u32| {
        let mut params = rc.clone();
        params.palw_panel_exposure_floor = Some(PalwPanelExposureFloorV1 { activation, reward_multiple_permille });
        params
    };
    let refusal = |params: &Params| match params.validate_palw_v2() {
        Err(why) => why.to_string(),
        Ok(()) => panic!("expected validate_palw_v2 to refuse {:?}", params.palw_panel_exposure_floor),
    };
    let flag_day = ForkActivation::new(PALW_RC_PALW_UPGRADE_FENCE_DAA);
    // A network with no V2 lane has no seat to floor.
    let mut hash_only = MAINNET_PARAMS;
    hash_only.palw_panel_exposure_floor = Some(PalwPanelExposureFloorV1 { activation: flag_day, reward_multiple_permille: 2_000 });
    assert!(refusal(&hash_only).contains("not ConsensusV2"), "{}", refusal(&hash_only));
    assert!(hash_only.palw_panel_exposure_floor_fence().is_none(), "…and nothing would answer it");
    // No economy: no duty row is ever written, so the floor would hash and reserve nothing.
    let mut no_economy = armed(flag_day, 2_000);
    no_economy.palw_panel_economy = None;
    assert!(refusal(&no_economy).contains("without palw_panel_economy"), "{}", refusal(&no_economy));
    let mut never_economy = armed(flag_day, 2_000);
    never_economy.palw_panel_economy = Some(ForkActivation::never());
    assert!(refusal(&never_economy).contains("without palw_panel_economy"), "{}", refusal(&never_economy));
    // Below the economy's own height, and the degenerate multiples.
    assert!(
        refusal(&armed(ForkActivation::new(PALW_RC_PALW_UPGRADE_FENCE_DAA - 1), 2_000)).contains("activates below"),
        "a floor that fires before any seat is on duty is refused"
    );
    assert!(refusal(&armed(flag_day, 0)).contains("zero"));
    assert!(
        refusal(&armed(flag_day, crate::palw_panel_economy_v1::PALW_PANEL_REWARD_MULTIPLE_MAX_PERMILLE_V1 + 1))
            .contains("past 100,000")
    );
    // What IS armable: the economy's own height, a later one, and the whole legal range.
    for (activation, multiple) in [
        (flag_day, 2_000u32),
        (ForkActivation::new(9_000_000), 5_000),
        (ForkActivation::new(9_000_000), 10_000),
        (flag_day, crate::palw_panel_economy_v1::PALW_PANEL_REWARD_MULTIPLE_MAX_PERMILLE_V1),
    ] {
        armed(activation, multiple).validate_palw_v2().expect("armable on the RC");
    }
    // `never()` is absence, and absence is refused by nothing — even where the economy is unset.
    let mut dormant = armed(ForkActivation::never(), 0);
    dormant.palw_panel_economy = None;
    dormant.palw_model_registry = None;
    dormant.palw_verification_v2 = None;
    dormant.palw_verification_s3 = None;
    dormant.palw_verification_s2 = None;
    dormant.palw_readiness_v2 = None;
    dormant.palw_economic_payout = None;
    dormant.palw_work_target = None;
    dormant.set_palw_single_lottery(None);
    dormant.validate_palw_v2().expect("a never() floor is not a floor");
}

/// **ADR-0130's floor is hashed Some-only, reported with its multiple, and normalised out of the
/// identity until it fires** — the contract every fence beside it keeps, so scheduling one is a
/// rolling deploy and arming one at genesis is a different network.
#[test]
fn adr0130_the_exposure_floor_is_hashed_some_only_and_scheduled_like_a_fence() {
    let rc = palw_rc_shipped_params();
    let armed = |activation: ForkActivation, reward_multiple_permille: u32| {
        let mut params = rc.clone();
        params.palw_panel_exposure_floor = Some(PalwPanelExposureFloorV1 { activation, reward_multiple_permille });
        params
    };
    let base = armed(ForkActivation::new(9_000_000), 2_000);
    let later = armed(ForkActivation::new(9_000_001), 2_000);
    let deeper = armed(ForkActivation::new(9_000_000), 5_000);
    for (what, other) in [("the height", &later), ("the multiple", &deeper), ("the fence", &rc)] {
        assert_ne!(base.consensus_params_id(), other.consensus_params_id(), "{what} reaches the fingerprint");
        assert_ne!(base.consensus_schedule_id(), other.consensus_schedule_id(), "{what} is reported in the schedule id");
        assert_eq!(
            base.consensus_identity_id(),
            other.consensus_identity_id(),
            "{what} of a height that has not fired leaves the identity — a rolling deploy, not a flag day"
        );
    }
    assert_eq!(armed(ForkActivation::never(), 2_000).consensus_identity_id(), rc.consensus_identity_id(), "never() is absence");
    // The fork-id gate reads it by name, and a scheduled height joins the schedule it digests.
    assert!(
        base.palw_fences_v1()
            .iter()
            .any(|(name, fence)| *name == "palw_panel_exposure_floor" && *fence == Some(ForkActivation::new(9_000_000))),
        "the named fence list the gate reads carries it"
    );
    assert!(rc.palw_fences_v1().iter().any(|(name, fence)| *name == "palw_panel_exposure_floor" && fence.is_none()));
    assert!(base.fence_schedule_v1().contains(&9_000_000) && !rc.fence_schedule_v1().contains(&9_000_000));

    // Armed at genesis on a card — where the economy is also genesis — it is a rule about block
    // one, and its multiple with it.
    let card = mainnet_card_fixture_v1(false);
    let carded = |multiple: u32| {
        let mut params = card.clone();
        params.palw_panel_exposure_floor =
            Some(PalwPanelExposureFloorV1 { activation: ForkActivation::always(), reward_multiple_permille: multiple });
        params
    };
    carded(5_000).validate_palw_v2().expect("a card may state a floor from genesis");
    assert_ne!(carded(5_000).consensus_identity_id(), card.consensus_identity_id(), "a floor in force at genesis parts networks");
    assert_ne!(carded(5_000).consensus_identity_id(), carded(10_000).consensus_identity_id(), "and so does its multiple");

    // The accessors: the multiple at a height, and the seat economy that carries it to the draw.
    let scheduled = armed(ForkActivation::new(9_000), 2_000);
    assert_eq!(scheduled.palw_panel_reward_multiple_permille_at(8_999), 0, "below the height, no floor");
    assert_eq!(scheduled.palw_panel_reward_multiple_permille_at(9_000), 2_000);
    assert_eq!(scheduled.palw_seat_economy_at(9_000).expect("armed").reward_multiple_permille, 2_000);
    assert_eq!(
        scheduled.palw_seat_economy_at(PALW_RC_PALW_UPGRADE_FENCE_DAA).expect("the economy is armed there").reward_multiple_permille,
        0,
        "the economy is in force and the floor is not: the draw prices a seat at the claim's exposure"
    );
    assert!(scheduled.palw_seat_economy_at(PALW_RC_PALW_UPGRADE_FENCE_DAA - 1).is_none(), "below the economy, no draw economy");
    // testnet-11's numbers, tied to the one function that prices a seat: at λ = 2 a seat of a
    // five-seat panel reserves twice the ~128 MSK it can be paid, not the ~0.40 MSK it risks today.
    let economy = scheduled.palw_seat_economy_at(9_000).unwrap();
    assert_eq!(economy.seat_exposure(13_426_800, 320_084_650_080, 5), 25_606_772_006);
    assert_eq!(rc.palw_seat_economy_at(9_000).unwrap().seat_exposure(13_426_800, 320_084_650_080, 5), 40_280_400);
}

/// **ADR-0107's share-growth fence is dormant on every shipped preset, visible the moment it is
/// not, and refused where the rule it names cannot read what it needs.** The four claims the
/// fence above is held to, plus the one that is this fence's own: the boundary counts the
/// closed epoch's `Final` claims off their records, so a network whose claims retire within an
/// epoch would refuse growth a class earned — refused at assembly, with the reason.
#[test]
fn the_share_growth_final_fence_is_dormant_everywhere_and_refused_where_it_cannot_read_the_epochs_finals() {
    for p in [&MAINNET_PARAMS, &TESTNET_PARAMS, &SIMNET_PARAMS, &DEVNET_PARAMS] {
        assert!(p.palw_share_growth_final.is_none(), "{}: a shipped preset states no ADR-0107 fence", p.net);
    }
    let rc = palw_rc_shipped_params();
    assert!(rc.palw_share_growth_final.is_none(), "testnet-11 grows on accepted blocks until a flag day says otherwise");
    assert!(devnet_shipped_params().palw_share_growth_final.is_none(), "devnet inherits the dormant state too");
    assert!(rc.palw_share_growth_final_fence().is_none(), "the reader is the one place the rule is decided");

    let mut spelled_out = rc.clone();
    spelled_out.palw_share_growth_final = Some(ForkActivation::never());
    assert_eq!(spelled_out.consensus_identity_id(), rc.consensus_identity_id(), "Some(never()) is absence");

    let mut scheduled = rc.clone();
    scheduled.palw_share_growth_final = Some(ForkActivation::new(9_000_000));
    assert_eq!(
        scheduled.consensus_identity_id(),
        rc.consensus_identity_id(),
        "an armed and an un-armed build must peer until the height fires"
    );
    assert_ne!(scheduled.consensus_params_id(), rc.consensus_params_id(), "…while the fingerprint names the rule");
    assert_ne!(scheduled.consensus_schedule_id(), rc.consensus_schedule_id(), "…and the operator log names the height");
    scheduled.validate_palw_v2().expect("testnet-11's claims outlive an epoch (3000 > 1000), so the rule can read its finals");

    let mut armed = rc.clone();
    armed.palw_share_growth_final = Some(ForkActivation::always());
    assert_ne!(armed.consensus_identity_id(), rc.consensus_identity_id(), "at genesis the handshake must see it");
    armed.validate_palw_v2().expect("armable on the RC");
    assert_eq!(armed.palw_share_growth_final_fence(), Some(ForkActivation::always()));

    // Fail-closed: no V2 bundle, no claims to count.
    let mut laneless = TESTNET_PARAMS;
    laneless.palw_share_growth_final = Some(ForkActivation::always());
    let e = laneless.validate_palw_v2().expect_err("a fence that hashes and cannot fire is refused at assembly");
    assert!(format!("{e:?}").contains("palw_share_growth_final"), "{e:?}");
    assert!(laneless.palw_share_growth_final_fence().is_none(), "and the reader never resolves it off V2");

    // Fail-closed: claims that retire within an epoch are gone before the boundary counts them.
    let mut short_lived = armed.clone();
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut short_lived.palw_consensus_mode else {
        unreachable!("the RC is ConsensusV2")
    };
    let epoch = bundle.state.epoch_length();
    bundle.state = bundle.state.clone().with_claim_retirement_daa(epoch).expect("retirement past the abandon hold");
    let e = short_lived.validate_palw_v2().expect_err("an epoch-long retirement loses the epoch's early finals");
    assert!(format!("{e:?}").contains("claim_retirement_daa"), "{e:?}");
}

/// **A card certifies the free-prompt lane of the classes it registers; the RC's pinned set
/// does not** (mainnet audit, 2026-09-05). The pinned set is computed from a typed list of
/// profiles — the graph-v2 dense projection among them — while the 5f card registers the
/// graph-v5 row, so on testnet-11 the 489‰ dense tier's free-prompt lane is uncertified from
/// genesis. Asserted here as the difference, so the defect is on the record until an on-chain
/// `ClassLaneCertified` closes it there.
#[test]
fn a_card_certifies_the_free_prompt_lane_of_what_it_registers_and_the_rc_set_does_not() {
    use crate::palw_e2e_adjudicability::palw_fp_certified_class_ids_of_registered_v1;
    use crate::palw_mode_v2::PalwConsensusMode;
    use crate::palw_state_v2::PalwConsensusObjectV2;

    // Every class the bundle registers, by id — the catalog classes carry no admission
    // carriage (ADR-0053) and are registered all the same.
    let registered_ids = |p: &Params| -> std::collections::BTreeSet<crate::Hash64> {
        let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("V2") };
        bundle
            .genesis_objects
            .iter()
            .filter_map(|o| match o {
                PalwConsensusObjectV2::ClassRegistered { class_id, .. } => Some(*class_id),
                _ => None,
            })
            .collect()
    };
    let certified = |p: &Params| -> std::collections::BTreeSet<crate::Hash64> {
        let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("V2") };
        bundle.state.fp_certified_classes().cloned().unwrap_or_default()
    };

    // testnet-11: the pinned set names a class the card does not register.
    let rc = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(rc_bundle) = &rc.palw_consensus_mode else { panic!("V2") };
    let pinned = certified(&rc);
    let derived = palw_fp_certified_class_ids_of_registered_v1(rc_bundle);
    let registered = registered_ids(&rc);
    assert!(derived.is_subset(&registered), "the derived set is a subset of what is registered, by construction");
    assert!(
        !pinned.is_subset(&registered),
        "the RC's pinned set names a class its card does not register — the defect, on the record"
    );
    assert_ne!(pinned, derived);
    assert_eq!(derived.len(), 3, "the floor, the hybrid and the registered dense row are all covered ({derived:?})");
    assert_eq!(registered.len(), 3, "…which is every class the card registers");

    // A card with the same tiers: what it certifies is what it registers.
    let base = mainnet_card_base_v1(mainnet_v2_mint_base(), true);
    let assembled = palw_v2_params_with_classes_on_base(
        base,
        PALW_RC_GENESIS_ARTIFACT_ROOT,
        PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT,
        Some(PALW_RC_GENESIS_QWEN25_A16_GRAPH_V5_ARTIFACT_ROOT),
        vec![],
    )
    .expect("the dense tier assembles on a card that states its court");
    let carded = palw_rc_arm_phase1(mainnet_certify_registered_classes_v1(assembled));
    let card_registered = registered_ids(&carded);
    let card_certified = certified(&carded);
    assert!(card_certified.is_subset(&card_registered), "a card certifies only classes it registers");
    assert_eq!(
        card_certified,
        palw_fp_certified_class_ids_of_registered_v1(match &carded.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(b) => b,
            _ => unreachable!(),
        })
    );
    assert!(card_certified.len() >= derived.len(), "…and the dense row it registers is covered too ({card_certified:?})");
}

/// **Half a mainnet card is refused by name.** The ruleset reads the artifact root and the
/// premine reads the bond registry; with only the second filled in, a genesis carves seat
/// collateral out of the 10B cap for a `Disabled` mainnet that seats nobody.
#[test]
fn a_half_filled_mainnet_card_is_refused_by_name() {
    assert!(mainnet_card_coherence_v1(false, 0).is_ok(), "mainnet as it ships: neither half");
    assert!(mainnet_card_coherence_v1(true, 0).is_ok(), "a zero-seat card is ADR-0061's, and legal");
    assert!(mainnet_card_coherence_v1(true, 8).is_ok(), "a whole card");
    let e = mainnet_card_coherence_v1(false, 8).expect_err("bonds without a ruleset is the incoherent half");
    assert!(e.contains("PALW_MAINNET_GENESIS_ARTIFACT_ROOT"), "the message names the missing constant: {e}");
    // And the shipped constants are coherent as they stand.
    assert!(
        mainnet_card_coherence_v1(
            PALW_MAINNET_GENESIS_ARTIFACT_ROOT != crate::Hash64::from_bytes([0u8; 64]),
            PALW_MAINNET_GENESIS_BONDS.len()
        )
        .is_ok(),
        "the card in this binary is coherent"
    );
}

/// **D1 may not be armed on a registry with no spare seat — and the shipped one now has them.**
///
/// The arming guard beside this one proves the GENESIS bonds are mature when the fence fires.
/// It can say nothing about a replacement, and that is the case that bites: one bond leaving
/// eligibility — retired, or slashed under `min_collateral_sompi` — makes every panel
/// undrawable for a full maturity window, because the replacement is itself immature. A short
/// draw is `InsufficientEligibleBonds`: no panel, so the claim voids at `BindTimeout`.
///
/// ADR-0065 recorded this as an instruction ("do not arm D1 on a network running at
/// `seat_count + 1`"). This asserts it is a rule instead — **and that the shipped preset obeys
/// it**, which is the half that took a genesis change. While `PALW_RC_GENESIS_BONDS` held the
/// bare `seat_count + 1`, arming D1 required minting a new genesis, so the fence was one no
/// running network could ever reach: enforcement existed and arming did not. Both positions
/// are asserted on one preset, because a guard that refused everything would also pass the
/// refusal half on its own.
#[test]
fn arming_bond_maturity_needs_a_registry_with_a_spare_seat() {
    use crate::palw_fp_devnet_v3::{palw_v2_maturity_armable_bonds_v1, palw_v2_min_genesis_bonds_v1};

    // 1. The shipped registry carries the spare seats, so the fence is armable AS SHIPPED —
    //    a config change on a running network, not a re-mint.
    let shipped = palw_rc_shipped_params();
    assert!(
        rc_registered_bonds(&shipped) >= palw_v2_maturity_armable_bonds_v1(),
        "the shipped genesis must be arm-ready: {} bonds registered, {} required",
        rc_registered_bonds(&shipped),
        palw_v2_maturity_armable_bonds_v1()
    );
    let mut armable = palw_rc_shipped_params();
    armable.palw_bond_maturity = Some(PalwBondMaturityV1 { activation: ForkActivation::new(1_000), window_daa: 1_000 });
    armable.validate_palw_v2().expect("the shipped genesis can arm ADR-0065 D1 without a re-mint");

    // 2. Cut back to the bare minimum that seats one panel and the same fence is refused, so
    //    the guard is a rule and not a formality the shipped numbers happen to satisfy. The
    //    unarmed trim validates first: what differs between the two lines below is the arming
    //    and nothing else.
    let mut bare = rc_params_with_bonds_trimmed_to(palw_v2_min_genesis_bonds_v1());
    assert!(bare.validate_palw_v2().is_ok(), "the trimmed registry is otherwise legal — only the arming differs");
    bare.palw_bond_maturity = Some(PalwBondMaturityV1 { activation: ForkActivation::new(1_000), window_daa: 1_000 });
    let err = bare.validate_palw_v2().expect_err("arming D1 on a spare-less registry must be refused");
    assert!(format!("{err}").contains("no spare seat"), "refused for the registry, not for something else: {err}");

    // 3. One short of the bar is still refused. The boundary, not a number far from it —
    //    otherwise an off-by-one in the guard would pass both cases above.
    let mut short = rc_params_with_bonds_trimmed_to(palw_v2_maturity_armable_bonds_v1() - 1);
    short.palw_bond_maturity = Some(PalwBondMaturityV1 { activation: ForkActivation::new(1_000), window_daa: 1_000 });
    assert!(short.validate_palw_v2().is_err(), "the bar is exact: one short of it must not arm");
}

/// ADR-0065 D1's fence, in the four identity positions — and the DURATION beside it, which is
/// the part that could have gone wrong silently.
#[test]
fn the_bond_maturity_window_is_in_the_fingerprint_and_not_in_the_fence_visitor() {
    let shipped = MAINNET_PARAMS;
    assert!(shipped.palw_bond_maturity.is_none());

    let mut armed = MAINNET_PARAMS;
    armed.palw_bond_maturity = Some(PalwBondMaturityV1 { activation: ForkActivation::new(9_000_000), window_daa: 1_000 });
    assert_eq!(shipped.consensus_identity_id(), armed.consensus_identity_id(), "scheduled, not in force: same network");
    assert_ne!(shipped.consensus_params_id(), armed.consensus_params_id(), "…and still a visible commitment");

    // **The window is in the FINGERPRINT — and, deliberately, not in the identity.** Say both
    // halves out loud, because the second is a live deployment hazard and an assertion that
    // only checked the first would read as though it were covered.
    let mut wider = MAINNET_PARAMS;
    wider.palw_bond_maturity = Some(PalwBondMaturityV1 { activation: ForkActivation::new(9_000_000), window_daa: 5_000 });
    assert_ne!(
        armed.consensus_params_id(),
        wider.consensus_params_id(),
        "a different maturity window is a different rule and must be in the fingerprint"
    );
    // …and the two are nevertheless ONE identity while the fence is only scheduled, because
    // `consensus_identity_id` rewrites the armed height to `never()` and the collapse then
    // takes the whole option with it. That is this file's own rule for a value that is inert
    // until its fence fires (see `normalize_values_a_scheduled_fence_drags_with_it`, audit3
    // H1) and it is what the VLT cost table already does — two builds scheduling different
    // tables at one height are the same network today.
    //
    // **The hazard it buys, stated so nobody has to rediscover it:** two operators who schedule
    // D1 at the same height with DIFFERENT windows peer normally, are told only by the
    // params-id warning in `flow_context`, and seat different panels the moment the fence
    // fires. The warning is the whole of the defence, so arming this fence is a coordinated
    // change even though the handshake permits a rolling one. If this assertion ever flips to
    // `assert_ne!`, arming D1 becomes a flag day instead — which is a defensible choice, but it
    // must be a chosen one.
    //
    // And the warning has to be able to SAY it: `consensus_schedule_id` is what that log line
    // prints, so the window is folded in there (and nowhere else). Asserted below, because a
    // defence that reports two identical ids for the two builds it exists to tell apart is not
    // a defence.
    assert_ne!(
        armed.consensus_schedule_id(),
        wider.consensus_schedule_id(),
        "the operator log must be able to name the difference it is the only warning about"
    );
    assert_eq!(
        armed.consensus_identity_id(),
        wider.consensus_identity_id(),
        "a scheduled fence's payload leaves the identity with it — deliberate, and the reason the params id must differ"
    );

    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_bond_maturity = Some(PalwBondMaturityV1 { activation: ForkActivation::always(), window_daa: 1_000 });
    assert_ne!(shipped.consensus_identity_id(), at_genesis.consensus_identity_id(), "in force at genesis is a rule difference");

    let mut never = MAINNET_PARAMS;
    never.palw_bond_maturity = Some(PalwBondMaturityV1 { activation: ForkActivation::never(), window_daa: 1_000 });
    assert_eq!(
        shipped.consensus_identity_id(),
        never.consensus_identity_id(),
        "never() is absence — the WHOLE option collapses, or the window's bytes survive into the identity"
    );
}

/// **ADR-0073 SA-1's fence, in the same four identity positions — and `k` beside it.**
///
/// The width is the half that could have gone wrong in silence: a `k` inside `for_each_fence`
/// would be normalised to `0`/`u64::MAX` and two builds folding three blocks and five would
/// fingerprint identically at every height, which is exactly the `inactivity_leak_daa` trap
/// this file already carries a scar from. So it is pinned OUT of the visitor and INTO both
/// `consensus_params_id` and `consensus_schedule_id`.
#[test]
fn the_beacon_fold_width_is_in_the_fingerprint_and_not_in_the_fence_visitor() {
    let fold = |score: u64, k: u8| {
        let mut p = MAINNET_PARAMS;
        p.palw_beacon_fold = Some(PalwBeaconFoldV1 { activation: ForkActivation::new(score), k });
        p
    };
    let shipped = MAINNET_PARAMS;
    assert!(shipped.palw_beacon_fold.is_none(), "every shipped preset leaves SA-1 dormant");

    let armed = fold(9_000_000, 3);
    assert_eq!(shipped.consensus_identity_id(), armed.consensus_identity_id(), "scheduled, not in force: same network");
    assert_ne!(shipped.consensus_params_id(), armed.consensus_params_id(), "…and still a visible commitment");

    // `k` is in the FINGERPRINT — a different fold is a different draw and therefore a
    // different rule set.
    let wider = fold(9_000_000, 5);
    assert_ne!(armed.consensus_params_id(), wider.consensus_params_id(), "a different fold width must be in the fingerprint");
    // …and it is in the operator log, which is the whole of the defence while the fence is
    // merely scheduled and the identity has collapsed the option away.
    assert_ne!(
        armed.consensus_schedule_id(),
        wider.consensus_schedule_id(),
        "the warning must be able to name the width the two builds disagree about"
    );
    assert_eq!(
        armed.consensus_identity_id(),
        wider.consensus_identity_id(),
        "a scheduled fence's payload leaves the identity with it — deliberate, and why the params id must differ"
    );

    // In force at genesis is a rule difference; `never()` is absence, whole option and all.
    let mut at_genesis = MAINNET_PARAMS;
    at_genesis.palw_beacon_fold = Some(PalwBeaconFoldV1 { activation: ForkActivation::always(), k: 3 });
    assert_ne!(shipped.consensus_identity_id(), at_genesis.consensus_identity_id(), "in force at genesis is a rule difference");
    let mut never = MAINNET_PARAMS;
    never.palw_beacon_fold = Some(PalwBeaconFoldV1 { activation: ForkActivation::never(), k: 3 });
    assert_eq!(
        shipped.consensus_identity_id(),
        never.consensus_identity_id(),
        "never() is absence — the WHOLE option collapses, or k's byte survives into the identity"
    );
    // And a dormant fence is byte-identical to the field not existing at all, on every preset:
    // this is what keeps adding SA-1 from being a flag day of its own.
    for (name, preset) in
        [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("devnet", DEVNET_PARAMS), ("simnet", SIMNET_PARAMS)]
    {
        assert!(preset.palw_beacon_fold.is_none(), "{name}: a shipped preset armed the beacon fold");
        let before = preset.consensus_params_id();
        let mut probe = preset;
        probe.palw_beacon_fold = None;
        assert_eq!(before, probe.consensus_params_id(), "{name}: the Some-only write ran on a None");
    }
}

/// **SA-1's floor is a gate, not a sentence in an ADR.** A fold narrower than three attempt
/// blocks does not bound the withholding bias the amendment exists to bound — at `k = 1` it is
/// the pre-SA-1 rule wearing a fence — so an armed fence naming one is refused at construction
/// rather than shipped as a rule that reads like one.
#[test]
fn an_armed_beacon_fold_may_not_undercut_the_amendments_floor() {
    let v2 = palw_rc_shipped_params();
    assert!(
        matches!(v2.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)),
        "the fixture must be V2, or this gate is never reached"
    );
    assert!(v2.validate_palw_v2().is_ok(), "the shipped RC params must start with the fence dormant");

    for k in 0..crate::palw_fp_beacon_v3::PALW_BEACON_FOLD_MIN_K_V1 {
        let mut narrow = v2.clone();
        narrow.palw_beacon_fold = Some(PalwBeaconFoldV1 { activation: ForkActivation::new(9_000_000), k });
        let err = narrow.validate_palw_v2().expect_err("k below the floor must be refused");
        assert!(format!("{err}").contains("SA-1's floor"), "refused for the width, not for something else: {err}");
    }
    // The floor itself, and anything above it, starts.
    for k in [crate::palw_fp_beacon_v3::PALW_BEACON_FOLD_MIN_K_V1, 4, 8] {
        let mut ok = v2.clone();
        ok.palw_beacon_fold = Some(PalwBeaconFoldV1 { activation: ForkActivation::new(9_000_000), k });
        assert!(ok.validate_palw_v2().is_ok(), "k = {k} is at or above the floor and must start");
    }
    // A `never()` fence is not armed, so its width is not held to the floor — the same stance
    // every other value-beside-a-fence check on this function takes.
    let mut dormant = v2.clone();
    dormant.palw_beacon_fold = Some(PalwBeaconFoldV1 { activation: ForkActivation::never(), k: 1 });
    assert!(dormant.validate_palw_v2().is_ok(), "an unarmed fence carries no rule to enforce");
}

/// ADR-0039 W4′ fence: OFF must be indistinguishable from the field not existing.
///
/// The fingerprint separates nodes at handshake, so a `None` fence that still wrote bytes
/// would split every shipped network from every prior build for a rule none of them run.
/// ADR-0038 Decision A: adding the commitment fence must be invisible to every network that
/// does not install it — otherwise the wiring is itself a flag day, and testnet-11 (live, and
/// the only PALW network) would fork on a binary upgrade.
///
/// The constant is the fingerprint testnet-11 is running under right now, read off the node.
/// The schedule and ramp fences are Some-only too, so installing neither leaves every shipped
/// preset's fingerprint exactly where it was.
///
/// The pinned-fingerprint test beside this one would catch a regression, but it would report it
/// as "the preset changed" — which sends a reader looking at the preset. This says what the
/// rule is: a fence nobody installed is not part of the rule set, so it must not be part of the
/// identity a node announces.
#[test]
fn absent_schedule_and_ramp_fences_do_not_move_the_fingerprint() {
    for params in [MAINNET_PARAMS, TESTNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS] {
        assert!(params.palw_schedule.is_none(), "a shipped preset installed a schedule fence");
        assert!(params.palw_ramp.is_none(), "a shipped preset installed a ramp fence");
    }
    // Byte-identical to a build without the fields: the Some-only writes never ran.
    let before = MAINNET_PARAMS.consensus_params_id();
    let mut probe = MAINNET_PARAMS;
    probe.palw_schedule = None;
    probe.palw_ramp = None;
    assert_eq!(probe.consensus_params_id(), before);
}

/// **A fence must not move a fingerprint that is not about fences.**
///
/// This used to pin what the LIVE algo-4 testnet-11 announced on 2026-08-18, as a guard that a
/// running network could still upgrade in place. That network no longer exists — suffix 11 is
/// the PALW-RC network now, minted fresh, and no node was running the old one on any host when
/// it moved. Pinning a dead chain's number would only assert that nobody re-minted, which is
/// the opposite of what happened deliberately.
///
/// What the test is actually for survives, and is what it now says: a preset that installs NO
/// fence must fingerprint identically to a build without the fence fields at all. That is the
/// property "adding the fence does not move anything" — and it is checked on mainnet, where a
/// moved fingerprint really would mean a chain with history could not upgrade.
#[test]
fn adding_the_commitment_fence_does_not_move_any_shipped_fingerprint() {
    // The RC network's own id is pinned by `shipped_presets_have_pinned_fingerprints`; here we
    // only demand that it is a value at all, so a preset that fails to build is still caught.
    let live: Params = crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 11).into();
    assert_ne!(live.consensus_params_id().to_string(), String::new(), "testnet-11 must build a fingerprint");
    assert!(live.palw_block_commitment.is_none());

    for (name, params) in [
        ("mainnet", &MAINNET_PARAMS),
        ("testnet", &TESTNET_PARAMS),
        ("testnet11", &TESTNET11_PARAMS),
        ("devnet", &DEVNET_PARAMS),
        ("simnet", &SIMNET_PARAMS),
    ] {
        assert!(params.palw_block_commitment.is_none(), "{name}: no shipped preset installs the fence");
        assert!(!params.is_palw_commitment_bound(0), "{name}: unfenced means the pre-ADR rule at every DAA");
        assert!(!params.is_palw_commitment_bound(u64::MAX), "{name}: and at every DAA, not just low ones");
    }

    // Installing it DOES move the fingerprint — a node that requires the commitment and one
    // that forbids it must not peer.
    let mut fenced = TESTNET11_PARAMS;
    fenced.palw_block_commitment = Some(crate::palw_block_commitment::PalwBlockCommitmentParamsV1 { activation_daa_score: 1_000 });
    assert_ne!(fenced.consensus_params_id(), TESTNET11_PARAMS.consensus_params_id());
    assert!(!fenced.is_palw_commitment_bound(999));
    assert!(fenced.is_palw_commitment_bound(1_000), "the fence binds AT its own daa score");
}

#[test]
fn an_absent_fork_choice_fence_does_not_move_the_fingerprint() {
    for (name, params) in [
        ("mainnet", &MAINNET_PARAMS),
        ("testnet", &TESTNET_PARAMS),
        ("testnet11", &TESTNET11_PARAMS),
        ("devnet", &DEVNET_PARAMS),
        ("simnet", &SIMNET_PARAMS),
    ] {
        assert!(params.palw_fork_choice.is_none(), "{name}: no preset installs the fence");
    }
    // Setting the fence DOES move the fingerprint — otherwise two nodes disagreeing about
    // fork choice could peer.
    let mut fenced = TESTNET_PARAMS;
    fenced.palw_fork_choice =
        Some(crate::palw_chain_weight::PalwChainWeightParamsV1 { penalty_sompi_per_pwu: 10, immature_bound_permille: 100 });
    assert_ne!(fenced.consensus_params_id(), TESTNET_PARAMS.consensus_params_id());

    // And a node refuses to start on it: the V1 fork choice read the validator overlay's bonds
    // and is removed, so the fence would name a rule nothing enforces.
    assert!(fenced.validate_palw_v1().is_err(), "a fork-choice fence is refused");
    let mut with_class = fenced;
    with_class.palw_credit = Some(crate::palw_credit::PalwCreditParamsV1 {
        registration: crate::palw_registry::tests::base0_registration(),
        s_eff_sompi: 20_000 * 100_000_000,
        unbonding_period_blocks: 1_209_600,
        activation_daa: 0,
        class_daa: crate::palw_class_daa::PalwClassDaaParamsV1::stage1_defaults(),
    });
    assert!(with_class.validate_palw_v1().is_err(), "a covered class does not make the removed fork choice runnable");
    with_class.palw_fork_choice = None;
    assert!(with_class.validate_palw_v1().is_err(), "nor is the V1 credit gate runnable on its own");
}

/// **ADR-0061 Decision 1: a zero-seat genesis is still a ConsensusV2 network.**
///
/// The gate stopped refusing an empty registry, but the preset assembly still keyed the whole
/// ruleset off the bond card — so emptying the card to mint the zero-seat genesis the ADR
/// describes handed back a hash-only chain with a 10B premine and no way to spend it, and no
/// gate anywhere would have said so (a fleet of identical binaries fingerprints identically
/// and peers fine). The assembly now falls back only when the ARTIFACT ROOT is unset, which is
/// the one input code cannot mint.
#[test]
fn an_empty_bond_registry_still_yields_a_consensus_v2_network() {
    // The shipped card is non-empty, so assemble the zero-seat case explicitly through the
    // same entry point the preset uses.
    let zero_seat = palw_rc_params_from_artifacts(PALW_RC_GENESIS_ARTIFACT_ROOT, Vec::new())
        .expect("a zero-seat registry assembles — the panel gate retired in ADR-0061");
    assert!(
        matches!(zero_seat.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)),
        "a network with no bonds yet is a V2 network whose producers have not arrived — not a hash chain"
    );
    // And the lanes that make the bootstrap possible are open on it: the heartbeat supplies
    // the blocks, the attempt lane is what the first registered bond will produce on.
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &zero_seat.palw_consensus_mode else { unreachable!() };
    assert!(bundle.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2), "the attempt lane must be open");
    // The clock lane follows its switch. While it is OFF a zero-seat network is a valid
    // GENESIS that cannot make its first block — the bootstrap half of ADR-0061 waits on the
    // heartbeat redesign, and saying so here keeps the two facts from being confused.
    // The clock lane is not a BUNDLE lane at all since ADR-0066 — it is `palw_heartbeat`, a
    // top-level fence, so a zero-seat network's bootstrap depends on arming that fence rather
    // than on rebuilding with a different constant. While the fence is unset, a zero-seat
    // network is a valid GENESIS that cannot make its first block; saying so here keeps the
    // two facts from being confused.
    assert!(!bundle.accepts_algo_id(crate::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID));
    assert!(!zero_seat.palw_heartbeat_lane_open_at(0), "and the fence ships unset");
    let mut clocked = zero_seat.clone();
    clocked.palw_heartbeat = Some(PalwHeartbeatV1 {
        activation: ForkActivation::always(),
        work_log2: crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
        max_per_mergeset: crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
    });
    assert!(clocked.palw_heartbeat_lane_open_at(0), "arming the fence opens it, with no rebuild");
    // Only an unset artifact root drops the bundle, because only that is unmintable by code.
    assert_ne!(PALW_RC_GENESIS_ARTIFACT_ROOT, crate::Hash64::from_bytes([0u8; 64]));
}

#[test]
fn identical_params_agree() {
    assert_eq!(SIMNET_PARAMS.consensus_params_id(), SIMNET_PARAMS.clone().consensus_params_id());
}

#[test]
fn every_shipped_preset_is_distinguishable() {
    let ids = [
        ("mainnet", MAINNET_PARAMS.consensus_params_id()),
        ("testnet", TESTNET_PARAMS.consensus_params_id()),
        ("simnet", SIMNET_PARAMS.consensus_params_id()),
        ("devnet", DEVNET_PARAMS.consensus_params_id()),
    ];
    for (i, (name_a, a)) in ids.iter().enumerate() {
        for (name_b, b) in ids.iter().skip(i + 1) {
            assert_ne!(a, b, "{name_a} and {name_b} must not share a fingerprint");
        }
    }
}

#[test]
fn an_overlay_rule_change_is_caught() {
    // testnet-22's shape: a build whose overlay behaviour differs while everything visible at
    // handshake looks identical. The fingerprint covers the whole struct, so it is caught.
    let base = TESTNET_PARAMS;
    let mut tweaked = TESTNET_PARAMS;
    if let Some(dns) = tweaked.dns_params.as_mut() {
        dns.dns_activation_daa_score += 1;
    } else {
        tweaked.dns_params = Some(GENESIS_ACTIVE_DNS_PARAMS);
    }
    assert_ne!(base.consensus_params_id(), tweaked.consensus_params_id());
}
