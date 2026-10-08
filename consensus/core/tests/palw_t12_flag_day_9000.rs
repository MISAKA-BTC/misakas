//! **testnet-12's int-13 flag day, DAA 9,000: the code-change-only fences that waited on a height** (release line `release/t12-daa9000`,
//! the coordinator's brief of 2026-10-08).
//!
//! * **the list** — [`PALW_T12_INT13_FENCES_V1`] arms, at ONE height ([`PALW_T12_INT13_DAA`], 9,000), `palw_audit_1004_v1` (the
//!   2026-10-04 audit's consensus fixes), `palw_gen_range_twin_v1` (the range twin that sizes a generative class's closes),
//!   `palw_model_court_window` (the per-model court window, which on the held clock moves no verdict) and `palw_receipt_spend_v4`
//!   (RFC-0009 stage C). Tier 1: each one's prerequisites are already in force on testnet-12 (genesis rules, or the int-11 list's
//!   `palw_gen_v1` at 5,300). A height no other fence uses: the fork id names heights, not fences;
//! * **the baseline** — [`palw_t12_release_v6_params`] (the shipped ruleset with the list dormant) IS the int-12 release (the DAA-5,300
//!   flag day, the fleet's): its params, identity and schedule ids are pinned here by value, so this release's re-pin cannot move them;
//! * **the release** — testnet-12 as shipped IS that baseline with the list at 9,000: params and schedule ids move, the identity does
//!   not, and no other fence moves;
//! * **the fork id** — an int-12 node is kept below 9,000 in both directions and refused from 9,000;
//! * **the prerequisites** — each entry's, refused by name where it is out of order; and the list as a whole needs the int-11 list below it;
//! * **the drill** — `--palw-drill-int13-at` ([`palw_drill_int13_at_v1`]) moves exactly the list on a salted drill chain, on a copy,
//!   after the flag days (and the int-11 list) it needs below it;
//! * **the next entry is one line** — a list entry added later is armed through the same `set`, and the checks below read the list's
//!   own names, so adding one changes `LIST` and the re-pin, nothing else.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_t12_flag_day_9000`

use kaspa_consensus_core::config::drill::{
    PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1, palw_drill_int11_at_v1, palw_drill_int13_at_v1, palw_drill_model_court_window_at_v1,
    palw_drill_post_launch_fences_at_v1, palw_drill_post_launch_fences_v2_at_v1, palw_drill_post_launch_fences_v3_at_v1,
    palw_drill_tir_fence2_at_v1, palw_drill_tir_fence_at_v1,
};
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_INT11_FENCES_V1, PALW_T12_INT11_FLAG_DAY_DAA, PALW_T12_INT11_RHO100_DAA, PALW_T12_INT11_RHO100_FENCES_V1,
    PALW_T12_INT11_RHO250_DAA, PALW_T12_INT11_RHO250_FENCES_V1, PALW_T12_INT11_RHO1000_DAA, PALW_T12_INT11_RHO1000_FENCES_V1,
    PALW_T12_INT13_DAA, PALW_T12_INT13_FENCES_V1, PALW_T12_MODEL_COURT_WINDOW_DAA, PALW_T12_POST_LAUNCH_FENCES_V1,
    PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_FENCES_V1, PALW_T12_TIR_FLAG_DAY_FENCES_V1, Params,
    palw_t12_arm_int11_flag_day_at_v1, palw_t12_arm_int13_flag_day_at_v1, palw_t12_drill_params_v1, palw_t12_launch_params_v1,
    palw_t12_release_v1_params, palw_t12_release_v2_params, palw_t12_release_v3_params, palw_t12_release_v4_params,
    palw_t12_release_v5_params, palw_t12_release_v6_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// The flag day's height.
const H: u64 = 9_000;

/// The names of the list, in the order the entries' prerequisites need. **Adding an entry to the release is one line here, one line in
/// `PALW_T12_INT13_FENCES_V1`, and the re-pin.**
const LIST: [&str; 4] = ["palw_audit_1004_v1", "palw_gen_range_twin_v1", "palw_model_court_window", "palw_receipt_spend_v4"];

/// The int-12 release's params id (the DAA-5,300 flag day, the fleet's `5ee7fd8e…`), split so that the re-pin tool does not read it as a
/// pin of the shipped ruleset: it must NOT move when THIS release is re-pinned — that is what the baseline test below is for.
fn int12_params_id() -> String {
    concat!("5ee7fd8ee019968cf52929b844cf9ddf", "b1aad500842a89cf04bced8ba4edefb6").to_owned()
}

/// int-12's identity id (the launch release's: no flag day since launch has moved the identity), split for the same reason.
fn int12_identity_id() -> String {
    concat!("5de80e64b63572de0cbf1a09679034e3", "a1765166e8249d3a88f8e29891215bb5").to_owned()
}

/// int-12's schedule id (`1678e073…`: 750, 1,000, 1,300, 1,700, 2,000, 3,600, 5,300, 5,395, 5,490, 5,585), split for the same reason.
fn int12_schedule_id() -> String {
    concat!("1678e07359f6727e96224041450a3b1d", "2aadcf8acd6bb6db0c277ff4d401c9b9").to_owned()
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn fence_at(p: &Params, name: &str) -> Option<u64> {
    p.palw_fences_v1().iter().find(|(n, _)| *n == name).and_then(|(_, f)| *f).map(|f| f.daa_score())
}

fn salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x5c; PALW_DRILL_SALT_LEN_V1]).expect("a legal salt")
}

/// `p` with the flag day's list at `at` (`None`: dormant), through each entry's own `set`.
fn with_list(mut p: Params, at: Option<u64>) -> Params {
    palw_t12_arm_int13_flag_day_at_v1(&mut p, at);
    p
}

fn entry(name: &str) -> &'static kaspa_consensus_core::config::params::PalwPostLaunchFenceV1 {
    PALW_T12_INT13_FENCES_V1.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("{name} is on the list"))
}

fn refused(p: &Params) -> Option<String> {
    p.validate_palw_v2().err().map(|e| format!("{e:?}"))
}

#[test]
fn the_flag_day_is_the_tier_one_list_at_9000_a_height_no_other_fence_uses() {
    assert_eq!(PALW_T12_INT13_DAA, Some(H), "the int-13 flag day's height");
    let names: Vec<&str> = PALW_T12_INT13_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(names, LIST, "the int-13 list, in the order its prerequisites need");
    // On no earlier flag day's list — the court window has a list of its own (a drill's and a later release's), which holds the same entry.
    for earlier in PALW_T12_POST_LAUNCH_FENCES_V1
        .iter()
        .chain(PALW_T12_POST_LAUNCH_FENCES_V2)
        .chain(PALW_T12_POST_LAUNCH_FENCES_V3)
        .chain(PALW_T12_TIR_FLAG_DAY_FENCES_V1)
        .chain(PALW_T12_TIR_FENCE2_FENCES_V1)
        .chain(PALW_T12_INT11_FENCES_V1)
        .chain(PALW_T12_INT11_RHO100_FENCES_V1)
        .chain(PALW_T12_INT11_RHO250_FENCES_V1)
        .chain(PALW_T12_INT11_RHO1000_FENCES_V1)
    {
        assert!(!LIST.contains(&earlier.name), "{} is on an earlier list", earlier.name);
    }
    // The tier-2 and undecided fences are NOT on it: the user decided nothing about the first two, the rest are other lanes'.
    for left_out in [
        "palw_tir_only_v1",
        "palw_model_virtual_v1",
        "palw_dns_retirement_v1",
        "palw_exec_payload_v2",
        "palw_permissionless_panel_v1",
        "palw_probabilistic_constraints_v1",
        "palw_panel_free_v1",
    ] {
        assert!(!LIST.contains(&left_out), "{left_out} is not tier 1");
    }
    assert_eq!(PALW_T12_MODEL_COURT_WINDOW_DAA, None, "the court window's own list stays heightless: the int-13 list arms its entry");

    let shipped = palw_t12_shipped_params();
    for name in LIST {
        assert_eq!(fence_at(&shipped, name), Some(H), "{name} arms at 9,000");
    }
    // A height no other fence uses.
    for (name, fence) in shipped.palw_fences_v1() {
        if !LIST.contains(&name) {
            assert_ne!(fence.map(|f| f.daa_score()), Some(H), "{name}: 9,000 is the int-13 list's alone");
        }
    }
    let schedule = shipped.fence_schedule_v1();
    for earlier in [750, 1_000, 1_300, 1_700, 2_000, 3_600] {
        assert!(schedule.contains(&earlier), "{earlier}: an earlier flag day, still scheduled ({schedule:?})");
    }
    assert_eq!(
        &schedule[schedule.len() - 5..],
        [PALW_T12_INT11_FLAG_DAY_DAA.unwrap(), PALW_T12_INT11_RHO100_DAA.unwrap(), PALW_T12_INT11_RHO250_DAA.unwrap(), PALW_T12_INT11_RHO1000_DAA.unwrap(), H],
        "the int-11 flag day's four heights then 9,000 are the schedule's last ({schedule:?})"
    );
    assert_eq!(schedule.iter().filter(|h| **h == H).count(), 1, "one height for the whole list");
}

#[test]
fn the_release_v6_baseline_is_int12s_ruleset_to_the_id() {
    let baseline = palw_t12_release_v6_params();
    let (params, identity, schedule) = ids(&baseline);
    assert_eq!(params, int12_params_id(), "the baseline is the fleet's int-12 ruleset");
    assert_eq!(identity, int12_identity_id(), "…its identity");
    assert_eq!(schedule, int12_schedule_id(), "…and its schedule");
    for name in LIST {
        assert_eq!(fence_at(&baseline, name), None, "{name} is dormant on the int-12 baseline");
    }
    assert_eq!(fence_at(&baseline, "palw_gen_v1"), PALW_T12_INT11_FLAG_DAY_DAA, "the int-11 list is the baseline's");
    baseline.validate_palw_v2().expect("the int-12 ruleset validates on this build");
    // …and it is int-10's with the int-11 flag day armed (the chain of baselines is unbroken).
    let mut armed = palw_t12_release_v5_params();
    palw_t12_arm_int11_flag_day_at_v1(&mut armed, PALW_T12_INT11_FLAG_DAY_DAA);
    assert_eq!(ids(&armed), (params, identity, schedule), "int-12 = int-10 + the int-11 flag day at 5,300");
    // Every older baseline carries the list dormant too.
    for (name, p) in [
        ("palw_t12_launch_params_v1", palw_t12_launch_params_v1()),
        ("palw_t12_release_v1_params", palw_t12_release_v1_params()),
        ("palw_t12_release_v2_params", palw_t12_release_v2_params()),
        ("palw_t12_release_v3_params", palw_t12_release_v3_params()),
        ("palw_t12_release_v4_params", palw_t12_release_v4_params()),
        ("palw_t12_release_v5_params", palw_t12_release_v5_params()),
    ] {
        for entry in LIST {
            assert_eq!(fence_at(&p, entry), None, "{name}: {entry} is dormant");
        }
        p.validate_palw_v2().unwrap_or_else(|e| panic!("{name} validates: {e}"));
    }
}

#[test]
fn testnet12_ships_the_list_armed_at_9000_over_the_int12_release() {
    let baseline = palw_t12_release_v6_params();
    let shipped = palw_t12_shipped_params();
    shipped.validate_palw_v2().expect("testnet-12 as shipped validates");
    assert_eq!(ids(&shipped), ids(&with_list(baseline.clone(), Some(H))), "testnet-12 as shipped arms the list at 9,000 over int-12");
    assert_eq!(ids(&Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))), ids(&shipped), "the node's door");
    let (bp, bi, bs) = ids(&baseline);
    let (sp, si, ss) = ids(&shipped);
    assert_ne!(sp, bp, "the ruleset names the fences");
    assert_ne!(ss, bs, "the schedule names 9,000");
    assert_eq!(si, bi, "the identity does not move: a future height is not an identity");
    assert_eq!(shipped.genesis.hash, baseline.genesis.hash, "the genesis does not move");
    // No other fence moved.
    for ((name, now), (was_name, was)) in shipped.palw_fences_v1().into_iter().zip(baseline.palw_fences_v1()) {
        assert_eq!(name, was_name);
        if LIST.contains(&name) {
            assert_eq!(was, None, "{name} is dormant on the int-12 baseline");
            assert_eq!(now.map(|f| f.daa_score()), Some(H), "{name} is armed at 9,000");
        } else {
            assert_eq!(now, was, "{name}: moved by the int-13 flag day");
        }
    }
    // `set(None)` gives int-12 back, to the id; `set(Some(never()))` keeps the identity and is dormant.
    assert_eq!(ids(&with_list(shipped.clone(), None)), ids(&baseline), "set(None): the int-12 ruleset");
    let mut never = shipped.clone();
    for f in PALW_T12_INT13_FENCES_V1.iter().rev() {
        (f.set)(&mut never, Some(ForkActivation::never()));
    }
    never.validate_palw_v2().expect("never() is dormant");
    assert_eq!(ids(&never).1, bi, "never(): the identity");
    assert_ne!(ids(&never).0, sp, "never(): not the armed release");
    // Each entry alone is a different ruleset (each is a fence of the params id, none is shadowed by another).
    for name in LIST {
        let mut without = shipped.clone();
        (entry(name).set)(&mut without, None);
        assert_ne!(ids(&without).0, sp, "{name}: the params id names it");
        without.validate_palw_v2().unwrap_or_else(|e| panic!("the list without {name} still validates: {e}"));
    }
}

#[test]
fn the_rules_are_live_from_9000_and_not_below() {
    let shipped = palw_t12_shipped_params();
    assert!(!shipped.palw_audit_1004_active_at(H - 1) && shipped.palw_audit_1004_active_at(H), "audit 1004");
    assert!(!shipped.palw_gen_range_twin_active_at(H - 1) && shipped.palw_gen_range_twin_active_at(H), "the range twin");
    assert!(!shipped.palw_model_court_window_active_at(H - 1) && shipped.palw_model_court_window_active_at(H), "the court window");
    assert!(!shipped.palw_receipt_spend_v4_active_at(H - 1) && shipped.palw_receipt_spend_v4_active_at(H), "V4 receipt redemption");
    use kaspa_consensus_core::palw_tir_close_range_v1::PalwTirCloseTwinV1;
    assert_eq!(shipped.palw_gen_close_twin_at(H - 1), PalwTirCloseTwinV1::Element);
    assert_eq!(shipped.palw_gen_close_twin_at(H), PalwTirCloseTwinV1::Range);
    // The bundle's mirrors, which the fold reads, follow the fences.
    let PalwConsensusMode::ConsensusV2(bundle) = &shipped.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    assert_eq!(bundle.state.audit_1004_from_daa(), Some(H), "audit 1004's fold mirror");
    assert_eq!(bundle.state.gen_range_twin_from_daa(), Some(H), "the range twin's fold mirror");
    // The baseline answers none of them at any height.
    let baseline = palw_t12_release_v6_params();
    for daa in [0, H - 1, H, u64::MAX - 1] {
        assert!(!baseline.palw_audit_1004_active_at(daa) && !baseline.palw_gen_range_twin_active_at(daa), "{daa}");
        assert!(!baseline.palw_model_court_window_active_at(daa) && !baseline.palw_receipt_spend_v4_active_at(daa), "{daa}");
    }
    let PalwConsensusMode::ConsensusV2(b) = &baseline.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    assert_eq!((b.state.audit_1004_from_daa(), b.state.gen_range_twin_from_daa()), (None, None), "no mirror on the baseline");
    // The int-11 rules the range twin stands on are in force at 9,000 (and before it, from 5,300).
    assert!(shipped.palw_gen_v1_active_at(H) && shipped.palw_gen_v1_active_at(5_300));
}

#[test]
fn the_fork_id_keeps_an_int12_node_below_9000_and_refuses_it_from_9000() {
    let old = palw_t12_release_v6_params();
    let new = palw_t12_shipped_params();
    for daa in [5_585, 7_000, H - 1] {
        let o = fork_id_v1(&old, daa);
        let n = fork_id_v1(&new, daa);
        assert_eq!(n.next, H, "the new build announces 9,000 next at {daa}");
        assert!(!evaluate_fork_id_v1(&new, daa, o.fired.as_bytes().as_slice(), o.next).refuses(), "new keeps old at {daa}");
        assert!(!evaluate_fork_id_v1(&old, daa, n.fired.as_bytes().as_slice(), n.next).refuses(), "old keeps new at {daa}");
    }
    for daa in [H, H + 1, 12_000] {
        let o = fork_id_v1(&old, daa);
        let n = fork_id_v1(&new, daa);
        assert!(evaluate_fork_id_v1(&new, daa, o.fired.as_bytes().as_slice(), o.next).refuses(), "new refuses old at {daa}");
        assert!(evaluate_fork_id_v1(&old, daa, n.fired.as_bytes().as_slice(), n.next).refuses(), "old refuses new at {daa}");
    }
    assert!(fork_id_gate_fences_v1(&new).contains(&H) && !fork_id_gate_fences_v1(&old).contains(&H), "the fork id gates on 9,000");
    // A build that arms only part of the list is a different ruleset by the params id (the handshake's fingerprint), even where the fork
    // id — which names heights — cannot tell the two apart.
    let mut partial = old.clone();
    for f in &PALW_T12_INT13_FENCES_V1[..PALW_T12_INT13_FENCES_V1.len() - 1] {
        (f.set)(&mut partial, Some(ForkActivation::new(H)));
    }
    assert_ne!(ids(&partial).0, ids(&new).0, "a partial list is another ruleset");
}

#[test]
fn the_list_holds_its_prerequisites_by_name() {
    let baseline = palw_t12_release_v6_params();
    let at = |h: u64| Some(ForkActivation::new(h));
    let armed = with_list(baseline.clone(), Some(H));
    armed.validate_palw_v2().expect("the list as a whole validates over int-12");

    // palw_audit_1004_v1 needs the economic-safety bundle and the model registry at or below it.
    let audit = |edit: &dyn Fn(&mut Params)| {
        let mut p = armed.clone();
        edit(&mut p);
        // The fence's own validator, so another fence's earlier refusal cannot answer for it.
        p.validate_palw_audit_1004_v1().err().map(|e| format!("{e:?}")).unwrap_or_default()
    };
    assert!(audit(&|p| p.palw_economic_safety = None).contains("palw_audit_1004_v1"), "audit 1004 without economic safety");
    assert!(audit(&|p| p.palw_economic_safety = at(H + 1)).contains("palw_audit_1004_v1"), "…or with it later");
    assert!(audit(&|p| p.palw_model_registry = None).contains("palw_audit_1004_v1"), "without the registry");
    assert!(audit(&|p| p.palw_model_registry = at(H + 1)).contains("palw_audit_1004_v1"), "…or with it later");
    assert!(audit(&|p| p.palw_audit_1004_v1 = at(H + 5)).contains("mirror"), "a fence whose bundle mirror is unsynced");

    // palw_gen_range_twin_v1 needs palw_gen_v1 at or below it (5,300): refused below it, with it off, and over the baseline before int-11.
    let mut below_gen = armed.clone();
    (entry("palw_gen_range_twin_v1").set)(&mut below_gen, at(5_299));
    assert!(refused(&below_gen).is_some_and(|e| e.contains("palw_gen_range_twin_v1")), "the range twin below palw_gen_v1");
    let mut at_gen = armed.clone();
    (entry("palw_gen_range_twin_v1").set)(&mut at_gen, at(5_300));
    at_gen.validate_palw_v2().expect("the range twin at palw_gen_v1's own height");
    let mut no_gen = armed.clone();
    no_gen.palw_gen_v1 = None;
    assert!(refused(&no_gen).is_some(), "the range twin with no generative fence at all");
    let over_int10 = with_list(palw_t12_release_v5_params(), Some(H));
    assert!(
        refused(&over_int10).is_some_and(|e| e.contains("palw_gen_range_twin_v1")),
        "the list needs the int-11 list below it: over the int-10 baseline it is refused by name"
    );

    // palw_model_court_window needs palw_kary_court at or below it (always() on testnet-12).
    assert_eq!(armed.palw_kary_court, Some(ForkActivation::always()), "testnet-12's k-ary court is in force from genesis");
    for kary in [at(H + 1), Some(ForkActivation::never()), None] {
        let mut p = armed.clone();
        p.palw_kary_court = kary;
        // Other fences that read the k-ary court may answer first on a ruleset this broken; the window's own validator is asked directly.
        let why = p.validate_palw_model_court_window().expect_err("the window without a k-ary court at or below it");
        assert!(format!("{why:?}").contains("palw_model_court_window is armed before palw_kary_court"), "{kary:?}: {why:?}");
        assert!(p.validate_palw_v2().is_err(), "{kary:?}: the ruleset does not validate");
    }

    // palw_receipt_spend_v4 needs palw_audit_2026_09_11 and palw_audit_2026_09_23 at or below it (genesis rules on testnet-12).
    for (what, strip) in [
        ("palw_audit_2026_09_11", (|p: &mut Params| p.palw_audit_2026_09_11 = None) as fn(&mut Params)),
        ("palw_audit_2026_09_11 later", |p| p.palw_audit_2026_09_11 = Some(ForkActivation::new(H + 1))),
        ("palw_audit_2026_09_23", |p| p.palw_audit_2026_09_23 = None),
        ("palw_audit_2026_09_23 later", |p| p.palw_audit_2026_09_23 = Some(ForkActivation::new(H + 1))),
    ] {
        let mut p = armed.clone();
        strip(&mut p);
        let why = p.validate_palw_receipt_spend_v4().expect_err(what);
        assert!(format!("{why:?}").contains("palw_receipt_spend_v4"), "{what}: {why:?}");
    }
    let mut no_v2 = kaspa_consensus_core::config::params::TESTNET11_PARAMS;
    no_v2.palw_receipt_spend_v4 = at(H);
    assert!(no_v2.validate_palw_receipt_spend_v4().is_err(), "a ruleset with no V2 bundle");

    // Every entry alone, at any height from its prerequisites' on, validates over int-12 (the list has no order among its entries).
    for name in LIST {
        let mut one = baseline.clone();
        (entry(name).set)(&mut one, at(H));
        one.validate_palw_v2().unwrap_or_else(|e| panic!("{name} alone at 9,000 over int-12: {e}"));
    }
}

/// A drill ruleset with the flag days the int-13 list needs below it, as the drill's frozen order moves them — through the int-11 list.
fn drill_with_the_earlier_flag_days_low() -> Params {
    let mut p = palw_t12_drill_params_v1(&salt());
    palw_drill_post_launch_fences_at_v1(&mut p, 6).expect("--palw-drill-fence-at=6");
    palw_drill_post_launch_fences_v2_at_v1(&mut p, 10).expect("--palw-drill-fence2-at=10");
    palw_drill_post_launch_fences_v3_at_v1(&mut p, 14).expect("--palw-drill-fence3-at=14");
    palw_drill_tir_fence_at_v1(&mut p, 20).expect("--palw-drill-tir-at=20");
    palw_drill_tir_fence2_at_v1(&mut p, 24).expect("--palw-drill-tir2-at=24");
    palw_drill_int11_at_v1(&mut p, 28).expect("--palw-drill-int11-at=28");
    p
}

#[test]
fn the_drill_crosses_the_whole_list_at_a_low_height_and_moves_nothing_else() {
    let before = drill_with_the_earlier_flag_days_low();
    // The drill ruleset arms the release's list at 9,000 like every ruleset: the move is a MOVE.
    assert_eq!(fence_at(&before, "palw_audit_1004_v1"), Some(H));
    let mut moved = before.clone();
    let moves = palw_drill_int13_at_v1(&mut moved, 60).expect("a salted drill crosses the int-13 flag day low");
    assert_eq!(moves.iter().map(|m| m.name).collect::<Vec<_>>(), LIST, "exactly the list, in its order");
    for m in &moves {
        assert_eq!((m.at, m.was), (60, Some(H)), "{} moves from the release's height to 60", m.name);
    }
    moved.validate_palw_v2().expect("the result validates");
    for ((name, was), (_, now)) in before.palw_fences_v1().iter().zip(moved.palw_fences_v1().iter()) {
        if LIST.contains(name) {
            assert_eq!(now.map(|f| f.daa_score()), Some(60), "{name}");
        } else {
            assert_eq!(was, now, "{name} did not move");
        }
    }
    assert!(!moved.palw_audit_1004_active_at(59) && moved.palw_audit_1004_active_at(60));
    assert!(!moved.palw_gen_range_twin_active_at(59) && moved.palw_gen_range_twin_active_at(60));
    assert!(!moved.palw_model_court_window_active_at(59) && moved.palw_model_court_window_active_at(60));
    assert!(!moved.palw_receipt_spend_v4_active_at(59) && moved.palw_receipt_spend_v4_active_at(60));
    assert_eq!(ids(&moved).1, ids(&before).1, "a future height: the identity is the drill's");
    // Refusals leave the ruleset as it came.
    for (at, why) in [(0, "genesis"), (u64::MAX, "never")] {
        let mut p = before.clone();
        assert!(palw_drill_int13_at_v1(&mut p, at).is_err(), "{why}");
        assert_eq!(ids(&p), ids(&before), "{why}: untouched");
    }
    let mut same = before.clone();
    let why = palw_drill_int13_at_v1(&mut same, 24).expect_err("another fence's height (the second IR fence's)");
    assert!(why.contains("fork id"), "{why}");
    assert_eq!(ids(&same), ids(&before), "refused untouched");
    // A move to the height the release already has moves nothing.
    let mut nothing = before.clone();
    let why = palw_drill_int13_at_v1(&mut nothing, H).expect_err("the release's own height moves nothing");
    assert!(why.contains("move nothing"), "{why}");
    // The range twin needs palw_gen_v1 below it: with the int-11 list still at 5,300 a low int-13 is refused, by name, untouched.
    let mut no_int11 = palw_t12_drill_params_v1(&salt());
    palw_drill_post_launch_fences_at_v1(&mut no_int11, 6).unwrap();
    palw_drill_post_launch_fences_v2_at_v1(&mut no_int11, 10).unwrap();
    palw_drill_post_launch_fences_v3_at_v1(&mut no_int11, 14).unwrap();
    palw_drill_tir_fence_at_v1(&mut no_int11, 20).unwrap();
    palw_drill_tir_fence2_at_v1(&mut no_int11, 24).unwrap();
    let was = ids(&no_int11);
    let why = palw_drill_int13_at_v1(&mut no_int11, 60).expect_err("the range twin below palw_gen_v1");
    assert!(why.contains("palw_gen_range_twin_v1"), "{why}");
    assert_eq!(ids(&no_int11), was, "refused untouched");
    // Public testnet-12's genesis and another network: never.
    let mut public = palw_t12_shipped_params();
    assert!(palw_drill_int13_at_v1(&mut public, 60).is_err(), "public testnet-12's genesis");
    let mut mainnet = kaspa_consensus_core::config::params::mainnet_shipped_params();
    assert!(palw_drill_int13_at_v1(&mut mainnet, 60).is_err(), "another network");
}

#[test]
fn the_per_fence_court_window_flag_moves_its_entry_and_keeps_the_rest_of_the_list_armed() {
    // `--palw-drill-model-court-at` is the one per-fence flag of the list: it moves the window alone and the rest stays at 9,000.
    let mut p = drill_with_the_earlier_flag_days_low();
    palw_drill_model_court_window_at_v1(&mut p, 36).expect("--palw-drill-model-court-at=36");
    assert_eq!(fence_at(&p, "palw_model_court_window"), Some(36));
    for name in ["palw_audit_1004_v1", "palw_gen_range_twin_v1", "palw_receipt_spend_v4"] {
        assert_eq!(fence_at(&p, name), Some(H), "{name} stays at the release's height");
    }
    p.validate_palw_v2().expect("validates");
}

/// **The release's fence table, printed for the review and the report** (`--nocapture`): every testnet-12 fence with a height past genesis,
/// by height, the three ids of the shipped ruleset and of the int-12 baseline, and the fork id's schedule.
#[test]
fn print_the_release_fence_table() {
    let shipped = palw_t12_shipped_params();
    let baseline = palw_t12_release_v6_params();
    let mut rows: Vec<(u64, &str)> = shipped
        .palw_fences_v1()
        .into_iter()
        .filter_map(|(name, f)| f.map(|f| (f.daa_score(), name)))
        .filter(|(at, _)| *at != 0 && *at != u64::MAX)
        .collect();
    rows.sort();
    println!("FENCE TABLE (testnet-12 as shipped; heights past genesis):");
    for (at, name) in &rows {
        let note = if baseline.palw_fences_v1().iter().any(|(n, f)| n == name && f.map(|f| f.daa_score()) == Some(*at)) { "" } else { "  <- int-13" };
        println!("  {at:>6}  {name}{note}");
    }
    let (sp, si, ss) = ids(&shipped);
    let (bp, bi, bs) = ids(&baseline);
    println!("SHIPPED  params {sp}\n         identity {si}\n         schedule {ss}");
    println!("INT-12   params {bp}\n         identity {bi}\n         schedule {bs}");
    println!("SCHEDULE {:?}", shipped.fence_schedule_v1());
    assert!(rows.iter().any(|(at, name)| *at == H && *name == "palw_audit_1004_v1"));
}
