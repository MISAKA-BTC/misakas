//! **testnet-12's IR flag day: RFC-0002 PALW-TIR v1 at DAA 2,000** (the user's decision of 2026-09-28).
//!
//! * **the list** — [`PALW_T12_TIR_FLAG_DAY_FENCES_V1`] is `palw_tir_v1` alone, at a height no other
//!   fence of testnet-12 uses (750, 1,000, 1,300 and 1,700 are the earlier flag days'); its name is apart
//!   from the P0a line's `PALW_T12_POST_LAUNCH_FENCES_V4`, which stays dormant and off this flag day
//!   (the user's direction: hybrids go the IR way);
//! * **the release** — testnet-12 as the DAA-2,000 release ships it (`palw_t12_release_v4_params`: the
//!   shipped ruleset before the DAA-3,600 flag day, int-8's ids) IS the DAA-1,700 release
//!   (`palw_t12_release_v3_params`, int-7's three ids exactly, pinned in `palw_tir_fences_are_dormant.rs`)
//!   with that list armed at 2,000: the params and schedule ids move, the identity does not, and no other
//!   fence moves;
//! * **the value** — `PalwTirFenceV1::testnet12_v1`: this build's primitive set and court version at
//!   testnet-12's IR ceilings (`max_cone_work` 2^16), mirrored on the V2 bundle;
//! * **the fork id** — an int-7 node is kept below 2,000 in both directions and refused from 2,000;
//! * **R-core+** — `palw_rcore_plus` (and the k-ary court, and A-2's declaration) are in force at or
//!   below the flag day on testnet-12, and the flag day is refused on a ruleset where R-core+ comes later;
//! * **the drill** — a salted drill carries the release's 2,000, and `--palw-drill-tir-at` moves the one
//!   fence to a low height and nothing else.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_tir_flag_day_t12`

use kaspa_consensus_core::config::drill::{PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1, palw_drill_tir_fence_at_v1};
use kaspa_consensus_core::config::params::{
    PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_DAA,
    PALW_T12_TIR_FLAG_DAY_DAA, PALW_T12_TIR_FLAG_DAY_FENCES_V1, Params, palw_t12_drill_params_v1, palw_t12_release_v3_params,
    palw_t12_release_v4_params, palw_t12_release_v5_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_tir_v1::{PALW_T12_TIR_CEILINGS_V1, PALW_TIR_COURT_VERSION_V1, PalwTirFenceV1, palw_tir_prim_set_id_v1};

/// The flag day's height.
const FLAG_DAY: u64 = 2_000;

/// A salted drill (any legal salt: the claims below are relational).
fn drill_salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x5a; PALW_DRILL_SALT_LEN_V1]).expect("a legal salt")
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn fence_at(p: &Params, name: &str) -> Option<u64> {
    p.palw_fences_v1().iter().find(|(n, _)| *n == name).and_then(|(_, f)| *f).map(|f| f.daa_score())
}

/// `p` with the IR flag day's list at `at` (`None`: dormant), through each entry's own `set`.
fn with_list(mut p: Params, at: Option<u64>) -> Params {
    for f in PALW_T12_TIR_FLAG_DAY_FENCES_V1 {
        (f.set)(&mut p, at.map(ForkActivation::new));
    }
    p
}

#[test]
fn the_ir_flag_day_is_palw_tir_v1_alone_at_2000_a_height_no_other_fence_uses() {
    assert_eq!(PALW_T12_TIR_FLAG_DAY_DAA, Some(FLAG_DAY), "the flag day's height");
    let names: Vec<&str> = PALW_T12_TIR_FLAG_DAY_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(names, ["palw_tir_v1"], "the IR alone: no P0a / C5 fence rides this flag day");
    for f in PALW_T12_POST_LAUNCH_FENCES_V1.iter().chain(PALW_T12_POST_LAUNCH_FENCES_V2).chain(PALW_T12_POST_LAUNCH_FENCES_V3) {
        assert_ne!(f.name, "palw_tir_v1", "on no earlier list");
    }
    // The DAA-3,600 release (int-10): the shipped ruleset arms the int-11 list on top of it (`palw_t12_flag_day_int11.rs`).
    let shipped = palw_t12_release_v5_params();
    for (name, fence) in shipped.palw_fences_v1() {
        if name != "palw_tir_v1" {
            assert_ne!(fence.map(|f| f.daa_score()), Some(FLAG_DAY), "{name}: 2,000 is the IR flag day's alone");
        }
    }
    let schedule = shipped.fence_schedule_v1();
    for earlier in [750, 1_000, 1_300, 1_700] {
        assert!(schedule.contains(&earlier), "{earlier}: an earlier flag day, still scheduled ({schedule:?})");
    }
    let day_3600 = PALW_T12_TIR_FENCE2_DAA.expect("the DAA-3,600 flag day");
    assert_eq!(schedule.last(), Some(&day_3600), "the DAA-3,600 flag day is the schedule's last height ({schedule:?})");
    assert_eq!(schedule[schedule.len() - 2], FLAG_DAY, "and the IR flag day the one before it ({schedule:?})");
}

#[test]
fn testnet12_ships_the_ir_flag_day_armed_at_2000_over_the_daa1700_release() {
    // The release's three ids are int-7's (`7aba8dd57`, the fleet's DAA-1,700 release) — pinned in
    // `palw_tir_fences_are_dormant.rs` (`palw_t12_release_v3_params`).
    let release = palw_t12_release_v3_params();
    let (rp, ri, rs) = ids(&release);
    assert_eq!(fence_at(&release, "palw_tir_v1"), None, "dormant on the release");
    release.validate_palw_v2().expect("the release validates");

    // testnet-12 as the DAA-2,000 release ships it: the shipped ruleset before the DAA-3,600 flag day.
    let shipped = palw_t12_release_v4_params();
    let armed = with_list(release.clone(), Some(FLAG_DAY));
    armed.validate_palw_v2().expect("the list at 2,000 validates over the release");
    assert_eq!(ids(&shipped), ids(&armed), "the DAA-2,000 release arms the list at 2,000 over the DAA-1,700 release");
    shipped.validate_palw_v2().expect("testnet-12 as the DAA-2,000 release validates");
    // …and what a node runs today is that release with the DAA-3,600 flag day's list on top (its own tests).
    let node = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    assert_eq!(ids(&node), ids(&palw_t12_shipped_params()), "the node's door");
    assert_ne!(ids(&node).0, ids(&shipped).0, "which the DAA-3,600 flag day moves");
    assert_eq!(ids(&node).1, ids(&shipped).1, "and not the identity");

    let (sp, si, ss) = ids(&shipped);
    assert_ne!(sp, rp, "the ruleset names the IR");
    assert_ne!(ss, rs, "the schedule names 2,000");
    assert_eq!(si, ri, "the identity does not move");

    // No other fence moved.
    let before = release.palw_fences_v1();
    for ((name, now), (was_name, was)) in shipped.palw_fences_v1().into_iter().zip(before) {
        assert_eq!(name, was_name);
        if name == "palw_tir_v1" {
            assert_eq!(now.map(|f| f.daa_score()), Some(FLAG_DAY));
        } else {
            assert_eq!(now, was, "{name}: moved by the IR flag day");
        }
    }

    // The value: this build's ids at testnet-12's IR ceilings, mirrored on the bundle.
    let fence = shipped.palw_tir_v1_fence().expect("armed on testnet-12");
    assert_eq!(fence, PalwTirFenceV1::testnet12_v1(ForkActivation::new(FLAG_DAY)));
    assert_eq!(fence.prim_set_id, palw_tir_prim_set_id_v1());
    assert_eq!(fence.court_version, PALW_TIR_COURT_VERSION_V1);
    assert_eq!(fence.ceilings, PALW_T12_TIR_CEILINGS_V1);
    assert_eq!(fence.ceilings.max_cone_work, 1 << 16, "admission work 2^16");
    assert_eq!(fence.ceilings.max_program_bytes, 88_000);
    let PalwConsensusMode::ConsensusV2(bundle) = &shipped.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    assert_eq!(bundle.state.tir_from_daa(), Some(FLAG_DAY), "the fold's mirror");
    assert!(!shipped.palw_tir_v1_active_at(FLAG_DAY - 1) && shipped.palw_tir_v1_active_at(FLAG_DAY));

    // `set(None)` gives the release back, to the id; `set(Some(never()))` keeps the identity.
    assert_eq!(ids(&with_list(shipped.clone(), None)), ids(&release), "set(None): the release");
    let mut never = shipped.clone();
    for f in PALW_T12_TIR_FLAG_DAY_FENCES_V1 {
        (f.set)(&mut never, Some(ForkActivation::never()));
    }
    never.validate_palw_v2().expect("never() is dormant");
    assert_eq!(ids(&never).1, ri, "never(): the identity");
}

#[test]
fn the_fork_id_keeps_an_int7_node_below_2000_and_refuses_it_from_2000() {
    let old = palw_t12_release_v3_params();
    let new = palw_t12_release_v4_params();
    for daa in [1_700, 1_900, FLAG_DAY - 1] {
        let o = fork_id_v1(&old, daa);
        let n = fork_id_v1(&new, daa);
        assert_eq!(n.next, FLAG_DAY, "the new build announces 2,000 next");
        assert!(!evaluate_fork_id_v1(&new, daa, o.fired.as_bytes().as_slice(), o.next).refuses(), "new keeps old at {daa}");
        assert!(!evaluate_fork_id_v1(&old, daa, n.fired.as_bytes().as_slice(), n.next).refuses(), "old keeps new at {daa}");
    }
    for daa in [FLAG_DAY, FLAG_DAY + 1, 2_500] {
        let o = fork_id_v1(&old, daa);
        let n = fork_id_v1(&new, daa);
        assert!(evaluate_fork_id_v1(&new, daa, o.fired.as_bytes().as_slice(), o.next).refuses(), "new refuses old at {daa}");
        assert!(evaluate_fork_id_v1(&old, daa, n.fired.as_bytes().as_slice(), n.next).refuses(), "old refuses new at {daa}");
    }
}

#[test]
fn rcore_plus_the_kary_court_and_a2_are_in_force_below_the_flag_day() {
    let shipped = palw_t12_shipped_params();
    let in_force = |f: Option<ForkActivation>| f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= FLAG_DAY);
    assert!(in_force(shipped.palw_rcore_plus), "palw_rcore_plus at or below 2,000: {:?}", shipped.palw_rcore_plus);
    assert!(in_force(shipped.palw_kary_court), "palw_kary_court at or below 2,000: {:?}", shipped.palw_kary_court);
    assert!(shipped.palw_audit_2026_09_11.is_some(), "A-2 declared");
    shipped.validate_palw_tir_v1().expect("the IR's own refusals pass on testnet-12");

    // The IR flag day on a ruleset whose R-core+ comes later, or never, is refused — by name.
    for rcore in [Some(ForkActivation::new(FLAG_DAY + 1)), Some(ForkActivation::never()), None] {
        let mut p = shipped.clone();
        p.palw_rcore_plus = rcore;
        let e = p.validate_palw_tir_v1().expect_err("R-core+ after the IR flag day");
        assert!(format!("{e:?}").contains("palw_rcore_plus"), "{rcore:?}: {e:?}");
    }
    // …and at the flag day itself it is enough.
    let mut same = shipped.clone();
    same.palw_rcore_plus = Some(ForkActivation::new(FLAG_DAY));
    same.validate_palw_tir_v1().expect("R-core+ at the flag day's own height");
    // The k-ary court likewise.
    let mut late = shipped;
    late.palw_kary_court = Some(ForkActivation::new(FLAG_DAY + 1));
    let e = late.validate_palw_tir_v1().expect_err("the k-ary court after the IR flag day");
    assert!(format!("{e:?}").contains("palw_kary_court"), "{e:?}");
}

#[test]
fn a_drill_carries_2000_and_the_drill_flag_moves_the_ir_alone() {
    let salt = drill_salt();
    let drill = palw_t12_drill_params_v1(&salt);
    assert_eq!(fence_at(&drill, "palw_tir_v1"), Some(FLAG_DAY), "the drill runs the release's schedule");
    let mut moved = drill.clone();
    let moves = palw_drill_tir_fence_at_v1(&mut moved, 20).expect("a free low height");
    assert_eq!(moves.len(), 1);
    assert_eq!((moves[0].name, moves[0].was, moves[0].at), ("palw_tir_v1", Some(FLAG_DAY), 20));
    moved.validate_palw_v2().expect("the moved drill validates");
    assert!(moved.palw_tir_v1_active_at(20) && !moved.palw_tir_v1_active_at(19));
    for ((name, now), (_, was)) in moved.palw_fences_v1().into_iter().zip(drill.palw_fences_v1()) {
        if name != "palw_tir_v1" {
            assert_eq!(now, was, "{name}: the drill flag moves the IR alone");
        }
    }
    assert_eq!(ids(&moved).1, ids(&drill).1, "the drill's identity does not move");
}
