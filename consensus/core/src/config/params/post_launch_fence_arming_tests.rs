//! **`params`'s `post_launch_fence_arming_tests` module**, moved out of `params.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;

/// `palw_t12_launch_params_v1`: shipped params with the post-launch lists dormant. G14 at DAA
/// 17,400 stays armed there, so this is that constructor's hash (2026-10-28), not the 2026-09-25
/// launch bytes. Identity is still the launch identity.
const T12_LAUNCH_IDS: (&str, &str, &str) = (
    "0fb6444822f57e424a6617a013f502d29d8fd8acddd33cae43066c0d84041ef5",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "3299840cfad1ec9eb3a2c63c0a2355e7759c9dedc4799b5eeb9e1f51dc9f2ece",
);

fn entry(name: &str) -> &'static PalwPostLaunchFenceV1 {
    PALW_T12_POST_LAUNCH_FENCES_V1.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("{name} is listed"))
}

fn height(params: &Params, name: &str) -> Option<ForkActivation> {
    params.palw_fences_v1().into_iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("{name} is a fence")).1
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

/// **The phase-1 audit's LOW, under the armed release: whatever an entry was set to after pass 2
/// of `palw_t12_base_params` — before the bundle exists — the assembly arms the release's height
/// over it, every mirror included.** Setting any entry but lane A's on the base at another height
/// (500) and assembling is byte-identical (`Debug`) to the shipped release: no stale mirror of the
/// base's height survives (the registry-resilience mirror is re-made at 750). Lane A's needs the
/// genesis registry and refuses a bundle-less base loudly rather than leave the rule dormant.
#[test]
fn an_entry_set_after_pass_2_is_armed_at_the_release_height_by_the_assembly() {
    if PALW_RC_GENESIS_ARTIFACT_ROOT == crate::Hash64::from_bytes([0u8; 64]) {
        return;
    }
    let release = palw_t12_shipped_params();
    let other = Some(ForkActivation::new(500));
    // Lane A needs the genesis registry (below); bind-deadlock needs lane A at or below it, which a
    // bundle-less base cannot carry, so the assembly's own validation refuses it there (below).
    for fence in
        PALW_T12_POST_LAUNCH_FENCES_V1.iter().filter(|f| f.name != "palw_operator_anchor" && f.name != "palw_anchor_at_ceiling")
    {
        let mut base = palw_t12_base_params();
        (fence.set)(&mut base, other);
        let assembled = palw_t12_public_params_over_v1(base);
        assembled.validate_palw_v2().unwrap_or_else(|e| panic!("{}: set after pass 2, assembled: {e}", fence.name));
        assert_eq!(
            height(&assembled, fence.name),
            Some(ForkActivation::new(PALW_T12_POST_LAUNCH_FENCE_DAA)),
            "{}: the assembly arms the release's height",
            fence.name
        );
        assert_eq!(format!("{assembled:?}"), format!("{release:?}"), "{}: one ruleset, whatever the base carried", fence.name);
    }
    let mut base = palw_t12_base_params();
    let registry = entry("palw_registry_resilience");
    (registry.set)(&mut base, other);
    let assembled = palw_t12_public_params_over_v1(base);
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &assembled.palw_consensus_mode else { panic!("V2") };
    assert_eq!(
        bundle.state.registry_resilience_from_daa(),
        Some(PALW_T12_POST_LAUNCH_FENCE_DAA),
        "the fold's mirror follows the release's height into the bundle"
    );

    let lane_a = entry("palw_operator_anchor");
    let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut base = palw_t12_base_params();
        (lane_a.set)(&mut base, other);
    }));
    assert!(refused.is_err(), "lane A's entry refuses a bundle-less base (its operators are the genesis registry's)");
    let mut base = palw_t12_base_params();
    (lane_a.set)(&mut base, None);
    assert_eq!(base.palw_operator_anchor, None, "setting it dormant needs no registry");
    // Bind-deadlock alone on a bare base: the assembly validates before the release arms lane A,
    // and refuses it by name (the int-4 audit's LOW) — a loud startup failure, never a silent rule.
    let binder = entry("palw_anchor_at_ceiling");
    let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut base = palw_t12_base_params();
        (binder.set)(&mut base, other);
        palw_t12_public_params_over_v1(base)
    }));
    assert!(refused.is_err(), "bind-deadlock set on a bare base, without lane A, does not assemble");
}

/// **The SHIPPED testnet-12 ruleset arms every post-launch fence at exactly DAA 750 and validates**
/// — through `palw_t12_shipped_params` and through `Params::from(testnet-12)`, the path kaspad
/// takes. Every prerequisite among the fences holds at one height, each rule is live at 750 and not
/// at 749 (the bundle's mirrors included), lane A trusts the eight genesis bonds, the identity is
/// the launch release's (a scheduled fence is peer-compatible) while the params id, the schedule
/// and the fork id move, and the list set back is the LAUNCH release to the id
/// ([`palw_t12_launch_params_v1`]). Every other preset leaves every entry dormant.
#[test]
fn the_shipped_ruleset_arms_every_post_launch_fence_at_750_and_validates() {
    if PALW_RC_GENESIS_ARTIFACT_ROOT == crate::Hash64::from_bytes([0u8; 64]) {
        return;
    }
    const AT: u64 = PALW_T12_POST_LAUNCH_FENCE_DAA;
    assert_eq!(AT, 750, "the user's decision of 2026-09-26");
    let release = palw_t12_shipped_params();
    let from_net = Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
    let launch = palw_t12_launch_params_v1();
    release.validate_palw_v2().expect("the armed release is a runnable testnet-12 ruleset");
    launch.validate_palw_v2().expect("the launch release still validates");
    assert_eq!(ids(&from_net), ids(&release), "the network id's ruleset is the shipped one");
    for fence in PALW_T12_POST_LAUNCH_FENCES_V1 {
        assert_eq!(height(&release, fence.name), Some(ForkActivation::new(AT)), "{} at exactly 750", fence.name);
        assert_eq!(height(&from_net, fence.name), Some(ForkActivation::new(AT)), "{} at 750 via Params::from", fence.name);
        assert_eq!(height(&launch, fence.name), None, "{}: dormant on the launch release", fence.name);
        for preset in [palw_rc_shipped_params(), devnet_shipped_params(), mainnet_shipped_params()] {
            assert_eq!(height(&preset, fence.name), None, "{} is dormant on {}", fence.name, preset.net);
        }
    }
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &release.palw_consensus_mode else { panic!("V2") };
    assert_eq!(bundle.state.registry_resilience_from_daa(), Some(AT), "registry resilience's mirror");
    assert_eq!(bundle.state.final_lock_full_collateral_from_daa(), Some(AT), "V02 option (a)'s mirror");
    assert_eq!(bundle.state.final_lock_life_from_daa(), Some(AT), "V02 lock life's mirror");
    assert!(release.palw_panel_seed_execution_active_at(AT) && !release.palw_panel_seed_execution_active_at(AT - 1));
    assert!(release.palw_operator_anchor_active_at(AT) && !release.palw_operator_anchor_active_at(AT - 1));
    assert!(release.palw_model_sink_bound_active_at(AT) && !release.palw_model_sink_bound_active_at(AT - 1));
    assert_eq!(release.palw_heartbeat_transparent_same_chain_fence(), Some(ForkActivation::new(AT)));
    assert_eq!(release.palw_reorg_strict_economic_win, Some(ForkActivation::new(AT)));
    assert_eq!(
        (release.palw_bond_maturity_window_at(AT - 1), release.palw_bond_maturity_window_at(AT)),
        (None, Some(PALW_T12_BOND_MATURITY_WINDOW_DAA)),
        "D1's window from 750, not below"
    );
    assert!(release.palw_final_lock_full_collateral_active_at(AT) && !release.palw_final_lock_full_collateral_active_at(AT - 1));
    assert!(release.palw_final_lock_life_active_at(AT) && !release.palw_final_lock_life_active_at(AT - 1));
    assert!(release.palw_anchor_at_ceiling_active_at(AT) && !release.palw_anchor_at_ceiling_active_at(AT - 1));
    assert!(release.palw_slashing_evidence_utxo_genuine_at(AT) && !release.palw_slashing_evidence_utxo_genuine_at(AT - 1));
    assert!(release.palw_pruning_proof_strict_economic_win.is_some_and(|f| f.is_active(AT) && !f.is_active(AT - 1)));
    let genesis_bonds: Vec<_> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            crate::palw_state_v2::PalwConsensusObjectV2::BondRegistered { bond, .. } => Some(*bond),
            _ => None,
        })
        .collect();
    let mut operators = release.palw_operator_anchor.as_ref().expect("lane A").operators.to_vec();
    operators.sort();
    let mut expected = genesis_bonds.clone();
    expected.sort();
    assert_eq!((operators.len(), &operators), (8, &expected), "lane A trusts every genesis bond");

    // The ids: the identity is the launch release's, the params id and the schedule name the height.
    let (now, then) = (ids(&release), ids(&launch));
    println!("testnet-12 armed at {AT}: {now:?}; launch release: {then:?}");
    assert_eq!((then.0.as_str(), then.1.as_str(), then.2.as_str()), T12_LAUNCH_IDS, "the list is the whole difference");
    assert_eq!(now.1, then.1, "a scheduled fence keeps the identity: the two builds peer below 750");
    assert_ne!(now.0, then.0, "the params id names the heights");
    assert_ne!(now.2, then.2, "and so does the schedule");
    assert_eq!(release.genesis.hash, launch.genesis.hash, "the genesis does not move");
    let gate = crate::fork_id_v1::fork_id_gate_fences_v1(&release);
    assert!(gate.contains(&AT), "the fork id gates on 750 ({gate:?})");
    assert!(!crate::fork_id_v1::fork_id_gate_fences_v1(&launch).contains(&AT));
    // The schedule: 750, D1's 1,000, and — on this build — the second and third flag days' own
    // heights (`PALW_T12_POST_LAUNCH_FENCES_V2`/`_V3`), the IR flag day's
    // (`PALW_T12_TIR_FLAG_DAY_FENCES_V1`) and the DAA-3,600 flag day's
    // (`PALW_T12_TIR_FENCE2_FENCES_V1`), the int-11 flag day's four (H, ρ = 100's H + 95, ρ = 250's H + 190 and ρ = 1000's H + 285)
    // and the int-13 flag day's one (`PALW_T12_INT13_DAA`); the DAA-750 release as the fleet ran it is 750 and 1,000 alone.
    let mut expected_schedule = vec![AT, PALW_T12_BOND_MATURITY_WINDOW_DAA];
    expected_schedule.extend(PALW_T12_POST_LAUNCH_FENCE_V2_DAA);
    expected_schedule.extend(PALW_T12_POST_LAUNCH_FENCE_V3_DAA);
    expected_schedule.extend(PALW_T12_TIR_FLAG_DAY_DAA);
    expected_schedule.extend(PALW_T12_TIR_FENCE2_DAA);
    expected_schedule.extend(PALW_T12_INT11_FLAG_DAY_DAA);
    expected_schedule.extend(PALW_T12_INT11_RHO100_DAA);
    expected_schedule.extend(PALW_T12_POST_LAUNCH_FENCE_V4_DAA);
    expected_schedule.extend(PALW_T12_INT11_RHO250_DAA);
    expected_schedule.extend(PALW_T12_INT11_RHO1000_DAA);
    expected_schedule.extend(PALW_T12_INT13_DAA);
    expected_schedule.push(PALW_T12_G14_17000_DAA_V1);
    expected_schedule.sort_unstable();
    expected_schedule.dedup();
    assert_eq!(
        release.fence_schedule_v1(),
        expected_schedule,
        "750, D1's 1,000, then the second, third, IR, DAA-3,600, int-11, int-13 and G14 flag days"
    );
    // The DAA-3,600 flag day's one fence: dormant below it, in force from it, on the shipped ruleset only; the
    // model court window rides no flag day (the coordinator's decision of 2026-10-01).
    let flag_day = PALW_T12_TIR_FENCE2_DAA.expect("the DAA-3,600 flag day");
    assert_eq!(flag_day, 3_600, "the user's decision of 2026-10-01");
    assert!(!release.palw_tir_fence2_active_at(flag_day - 1) && release.palw_tir_fence2_active_at(flag_day), "fence2 from 3,600");
    assert_eq!(PALW_T12_MODEL_COURT_WINDOW_DAA, None, "the model court window has no list-of-its-own height");
    // The int-13 list carries the window at DAA 15,500 (user, 2026-10-25). There is no DAA-9,000 flag day.
    let int13 = PALW_T12_INT13_DAA.expect("the int-13 flag day");
    assert_eq!(int13, 15_500, "the user's decision of 2026-10-25");
    for height in [0, 1, 750, 5_300, 9_000, int13 - 1] {
        assert!(!release.palw_model_court_window_active_at(height), "the model-window rule is dormant at {height}");
    }
    for height in [int13, int13 + 1, u64::MAX / 2, u64::MAX - 1] {
        assert!(release.palw_model_court_window_active_at(height), "the model-window rule is in force at {height}");
    }
    assert!(
        !palw_t12_release_v6_params().palw_model_court_window_active_at(u64::MAX / 2),
        "…and dormant at every height on the int-12 release it is judged against"
    );
    // Armed explicitly at a test height (the way a drill or the future release arms it), the window moves with the list: dormant
    // below the height, in force from it.
    {
        const INT13_TEST_AT: u64 = 9_000;
        let mut armed = release.clone();
        palw_t12_arm_int13_flag_day_at_v1(&mut armed, Some(INT13_TEST_AT));
        assert!(
            !armed.palw_model_court_window_active_at(INT13_TEST_AT - 1) && armed.palw_model_court_window_active_at(INT13_TEST_AT),
            "the model-window rule is in force from the list's height once the list is armed"
        );
        assert!(armed.fence_schedule_v1().contains(&INT13_TEST_AT), "…and the list's height is on the schedule once armed");
    }
    let pre_flag_day = palw_t12_release_v4_params();
    assert!(!pre_flag_day.palw_tir_fence2_active_at(flag_day), "the pre-flag release keeps the release's IR rules");
    let mut pre_flag_schedule = expected_schedule.clone();
    // The pre-flag release (int-8) omits the DAA-3,600 flag day's height, the int-11 flag day's four and the int-13 flag day's one.
    pre_flag_schedule.retain(|height| {
        *height != flag_day
            && Some(*height) != PALW_T12_INT13_DAA
            && Some(*height) != PALW_T12_INT11_FLAG_DAY_DAA
            && Some(*height) != PALW_T12_INT11_RHO100_DAA
            && Some(*height) != PALW_T12_INT11_RHO250_DAA
            && Some(*height) != PALW_T12_INT11_RHO1000_DAA
    });
    assert_eq!(pre_flag_day.fence_schedule_v1(), pre_flag_schedule, "the pre-flag release omits only the later flag days' heights");
    assert_eq!(
        palw_t12_release_v1_params().fence_schedule_v1(),
        vec![AT, PALW_T12_BOND_MATURITY_WINDOW_DAA],
        "the DAA-750 release: 750, then D1's 1,000"
    );

    // 1,000 — `palw_bond_maturity`'s own height — is refused by name.
    let mut at_maturity = release.clone();
    for fence in PALW_T12_POST_LAUNCH_FENCES_V1 {
        (fence.set)(&mut at_maturity, Some(ForkActivation::new(1_000)));
    }
    let why = at_maturity.validate_palw_v2().expect_err("never at 1,000");
    assert!(format!("{why:?}").contains("palw_bond_maturity_early"), "{why:?}");
}

/// **The int-4 audit's LOW: `palw_anchor_at_ceiling` validates only with lane A
/// (`palw_operator_anchor`) at or below it.** On the launch ruleset alone, above lane A, and with
/// lane A dormant it is refused by name; at lane A's height (the release) or above it, legal.
#[test]
fn the_anchor_at_ceiling_fence_needs_the_operator_anchor_at_or_below_it() {
    if PALW_RC_GENESIS_ARTIFACT_ROOT == crate::Hash64::from_bytes([0u8; 64]) {
        return;
    }
    let refused = |p: &Params, why: &str| {
        let e = p.validate_palw_v2().expect_err(why);
        assert!(format!("{e:?}").contains("palw_anchor_at_ceiling is armed without palw_operator_anchor"), "{why}: {e:?}");
    };
    let mut alone = palw_t12_launch_params_v1();
    alone.palw_anchor_at_ceiling = Some(ForkActivation::new(900));
    refused(&alone, "lane A dormant");
    // The DAA-1,300 release: the capacity fences (1,700) need lane A too, and would answer first.
    let release = palw_t12_release_v2_params();
    let mut below = release.clone();
    below.palw_anchor_at_ceiling = Some(ForkActivation::new(PALW_T12_POST_LAUNCH_FENCE_DAA - 1));
    refused(&below, "one DAA below lane A");
    let mut lane_a_never = release.clone();
    (entry("palw_operator_anchor").set)(&mut lane_a_never, Some(ForkActivation::never()));
    refused(&lane_a_never, "lane A scheduled never");
    release.validate_palw_v2().expect("at lane A's own height: the release");
    let mut above = release.clone();
    above.palw_anchor_at_ceiling = Some(ForkActivation::new(PALW_T12_POST_LAUNCH_FENCE_DAA + 250));
    above.validate_palw_v2().expect("above lane A");
    let mut never = palw_t12_launch_params_v1();
    never.palw_anchor_at_ceiling = Some(ForkActivation::never());
    never.validate_palw_v2().expect("a never() fence is dormant and needs nothing");
}

/// **The int-4 audit's LOW: `consensus_schedule_id` names the two strict-economic-win fences** — on
/// testnet-12, where they are armed, each one moved alone moves the schedule id; on every preset that
/// leaves them `None` (all but testnet-12) nothing is written, so no other network's id moved (the
/// presets' own pins hold that; here: set to `None` is the launch schedule id, and a network that
/// never had them keeps its id when they are cleared again).
#[test]
fn the_schedule_id_names_the_two_strict_win_fences_on_testnet12_only() {
    if PALW_RC_GENESIS_ARTIFACT_ROOT == crate::Hash64::from_bytes([0u8; 64]) {
        return;
    }
    let release = palw_t12_shipped_params();
    for name in ["palw_reorg_strict_economic_win", "palw_pruning_proof_strict_economic_win"] {
        let mut moved = release.clone();
        (entry(name).set)(&mut moved, Some(ForkActivation::new(PALW_T12_POST_LAUNCH_FENCE_DAA + 1)));
        assert_ne!(moved.consensus_schedule_id(), release.consensus_schedule_id(), "{name}: its height is in the schedule id");
        assert_eq!(moved.consensus_identity_id(), release.consensus_identity_id(), "{name}: a scheduled height, not identity");
    }
    // ONE of the two at a height, the other dormant: `for_each_fence` visits them Some-only and
    // side by side, so unnamed the two schedules hash the same sequence — only the names tell the
    // operator log WHICH rule a peer schedules.
    let (mut a, mut b) = (release.clone(), release.clone());
    a.palw_reorg_strict_economic_win = Some(ForkActivation::new(800));
    a.palw_pruning_proof_strict_economic_win = None;
    b.palw_reorg_strict_economic_win = None;
    b.palw_pruning_proof_strict_economic_win = Some(ForkActivation::new(800));
    assert_eq!(a.fence_schedule_v1(), b.fence_schedule_v1(), "the same heights");
    assert_ne!(a.consensus_schedule_id(), b.consensus_schedule_id(), "named: which rule sits at the height is reported");
    for preset in [palw_rc_shipped_params(), devnet_shipped_params(), mainnet_shipped_params()] {
        assert_eq!(preset.palw_reorg_strict_economic_win, None, "{}", preset.net);
        assert_eq!(preset.palw_pruning_proof_strict_economic_win, None, "{}", preset.net);
    }
}
