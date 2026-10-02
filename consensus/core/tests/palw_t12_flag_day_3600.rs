//! **testnet-12's DAA-3,600 flag day: `palw_tir_fence2` ALONE** (the user's decision of 2026-10-01, one release;
//! the coordinator's of the same day, the court window stays dormant on testnet-12; `rcore/int-10`).
//!
//! * **the list** — [`PALW_T12_TIR_FENCE2_FENCES_V1`] is `palw_tir_fence2`, armed at DAA 3,600
//!   ([`PALW_T12_TIR_FENCE2_DAA`]), a height no other fence of testnet-12 uses (750, 1,000, 1,300, 1,700 and
//!   2,000 are the earlier flag days'); `palw_model_court_window` is armed nowhere
//!   ([`PALW_T12_MODEL_COURT_WINDOW_DAA`] is `None`);
//! * **the baseline** — [`palw_t12_release_v4_params`] (the shipped ruleset with that list dormant) IS int-8's
//!   ruleset, `4ca695b98`, the fleet's: its params id is the fingerprint the fleet prints today
//!   (`3db42ea6…`), pinned here by value, so the fork-id comparison and the "only this fence moved it" twin
//!   keep a baseline that cannot drift with this release's re-pin;
//! * **the release** — testnet-12 as the DAA-3,600 release ships it ([`palw_t12_release_v5_params`]: since int-11 the
//!   shipped ruleset also arms the int-11 list at 5,300, which `palw_t12_flag_day_int11.rs` holds) IS that baseline with fence2
//!   armed at 3,600: the params and schedule ids move, the identity does not, and no other fence moves (the court window
//!   stays `None`);
//! * **the fork id** — an int-8 node is kept below 3,600 in both directions and refused from 3,600;
//! * **the prerequisites** — `palw_tir_v1` at or below `palw_tir_fence2`; and, for a ruleset that arms the dormant
//!   window, `palw_kary_court` at or below it, each refused by name otherwise;
//! * **what a model would get from the dormant window** — the court window a registered class derives from its own
//!   shape, at testnet-12's court: finite, never below the network window, monotone in the history, bounded by the
//!   court's own structure (the numbers are printed, for the review). That testnet-12 charges the held clock, so
//!   the window is the network window for every admissible class, is proven in
//!   `palw_t12_court_window_changes_no_admission`.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_t12_flag_day_3600`

use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_MODEL_COURT_WINDOW_DAA, PALW_T12_MODEL_COURT_WINDOW_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1,
    PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_DAA, PALW_T12_TIR_FENCE2_FENCES_V1,
    PALW_T12_TIR_FLAG_DAY_DAA, Params, palw_t12_release_v3_params, palw_t12_release_v4_params, palw_t12_release_v5_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// The flag day's height.
const FLAG_DAY: u64 = 3_600;

/// int-8's consensus params id (`4ca695b98`, the fingerprint the fleet prints and `deploy-int8-phaseh`'s
/// `EXPECT_FP`), split so that the re-pin tool does not read it as a pin of the shipped ruleset: it must NOT move
/// when this release is re-pinned — that is what the baseline test below is for.
fn int8_params_id() -> String {
    concat!("3db42ea638f3c4274f326b4049aa1ef8", "2408cb044c03f4ccf52848446a77702a").to_owned()
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn fence_at(p: &Params, name: &str) -> Option<u64> {
    p.palw_fences_v1().iter().find(|(n, _)| *n == name).and_then(|(_, f)| *f).map(|f| f.daa_score())
}

/// `p` with the flag day's list at `at` (`None`: dormant), through each entry's own `set`.
fn with_list(mut p: Params, at: Option<u64>) -> Params {
    for f in PALW_T12_TIR_FENCE2_FENCES_V1 {
        (f.set)(&mut p, at.map(ForkActivation::new));
    }
    p
}

#[test]
fn the_flag_day_is_fence2_alone_at_3600_a_height_no_other_fence_uses() {
    assert_eq!(PALW_T12_TIR_FENCE2_DAA, Some(FLAG_DAY), "the flag day's height (the user's decision of 2026-10-01)");
    let names: Vec<&str> = PALW_T12_TIR_FENCE2_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(names, ["palw_tir_fence2"], "the DAA-3,600 list is fence2 ALONE (the coordinator's decision of 2026-10-01)");
    // The court window has a list of its own, and no height: armed nowhere.
    assert_eq!(PALW_T12_MODEL_COURT_WINDOW_FENCES_V1.iter().map(|f| f.name).collect::<Vec<_>>(), ["palw_model_court_window"]);
    assert_eq!(PALW_T12_MODEL_COURT_WINDOW_DAA, None, "the model court window is dormant on testnet-12");
    // On no earlier list.
    for f in PALW_T12_POST_LAUNCH_FENCES_V1.iter().chain(PALW_T12_POST_LAUNCH_FENCES_V2).chain(PALW_T12_POST_LAUNCH_FENCES_V3) {
        assert!(f.name != "palw_tir_fence2" && f.name != "palw_model_court_window", "{} is on an earlier list", f.name);
    }
    let shipped = palw_t12_release_v5_params();
    assert_eq!(fence_at(&shipped, "palw_model_court_window"), None, "armed nowhere on the shipped ruleset");
    for (name, fence) in shipped.palw_fences_v1() {
        if name != "palw_tir_fence2" {
            assert_ne!(fence.map(|f| f.daa_score()), Some(FLAG_DAY), "{name}: 3,600 is this flag day's alone");
        }
    }
    let schedule = shipped.fence_schedule_v1();
    for earlier in [750, 1_000, 1_300, 1_700, PALW_T12_TIR_FLAG_DAY_DAA.expect("the IR flag day")] {
        assert!(schedule.contains(&earlier), "{earlier}: an earlier flag day, still scheduled ({schedule:?})");
    }
    assert_eq!(schedule.last(), Some(&FLAG_DAY), "the DAA-3,600 flag day is the schedule's last height ({schedule:?})");
}

#[test]
fn the_release_v4_baseline_is_int8s_ruleset_to_the_id() {
    let baseline = palw_t12_release_v4_params();
    assert_eq!(baseline.consensus_params_id().to_string(), int8_params_id(), "the baseline is the fleet's int-8 ruleset");
    assert_eq!(fence_at(&baseline, "palw_tir_fence2"), None);
    assert_eq!(fence_at(&baseline, "palw_model_court_window"), None);
    assert_eq!(fence_at(&baseline, "palw_tir_v1"), PALW_T12_TIR_FLAG_DAY_DAA, "the IR flag day is the baseline's");
    baseline.validate_palw_v2().expect("the int-8 ruleset validates on this build");
    // …and it is int-7's with the IR flag day armed (the chain of baselines is unbroken).
    let int7 = palw_t12_release_v3_params();
    let mut armed = int7.clone();
    for f in kaspa_consensus_core::config::params::PALW_T12_TIR_FLAG_DAY_FENCES_V1 {
        (f.set)(&mut armed, Some(ForkActivation::new(PALW_T12_TIR_FLAG_DAY_DAA.expect("the IR flag day"))));
    }
    assert_eq!(ids(&baseline), ids(&armed), "int-8 = int-7 + the IR flag day at 2,000");
}

#[test]
fn testnet12_ships_the_flag_day_armed_at_3600_over_the_int8_release() {
    let baseline = palw_t12_release_v4_params();
    let shipped = palw_t12_release_v5_params();
    shipped.validate_palw_v2().expect("testnet-12 as shipped validates");
    assert_eq!(
        ids(&shipped),
        ids(&with_list(baseline.clone(), Some(FLAG_DAY))),
        "testnet-12 as shipped arms fence2 at 3,600 over int-8"
    );
    assert_eq!(
        ids(&Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))),
        ids(&palw_t12_shipped_params()),
        "the node's door is testnet-12 as shipped (with the int-11 flag day armed on top of this release)"
    );
    let (bp, bi, bs) = ids(&baseline);
    let (sp, si, ss) = ids(&shipped);
    assert_ne!(sp, bp, "the ruleset names the fence");
    assert_ne!(ss, bs, "the schedule names 3,600");
    assert_eq!(si, bi, "the identity does not move: a future height is not an identity");
    // No other fence moved — the court window least of all.
    for ((name, now), (was_name, was)) in shipped.palw_fences_v1().into_iter().zip(baseline.palw_fences_v1()) {
        assert_eq!(name, was_name);
        if name == "palw_tir_fence2" {
            assert_eq!(now.map(|f| f.daa_score()), Some(FLAG_DAY), "{name}");
            assert_eq!(was, None, "{name} is dormant on the int-8 baseline");
        } else {
            assert_eq!(now, was, "{name}: moved by the DAA-3,600 flag day");
        }
    }
    // The rule is live from 3,600 and not below it; fence2's bundle mirror follows; the court window is live nowhere.
    assert!(!shipped.palw_tir_fence2_active_at(FLAG_DAY - 1) && shipped.palw_tir_fence2_active_at(FLAG_DAY));
    assert!(!shipped.palw_model_court_window_active_at(0) && !shipped.palw_model_court_window_active_at(u64::MAX - 1));
    let PalwConsensusMode::ConsensusV2(bundle) = &shipped.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    assert_eq!(bundle.state.tir_fence2_from_daa(), Some(FLAG_DAY), "fence2's fold mirror");
    // `set(None)` gives int-8 back, to the id; `set(Some(never()))` keeps the identity and is dormant.
    assert_eq!(ids(&with_list(shipped.clone(), None)), ids(&baseline), "set(None): the int-8 ruleset");
    let mut never = shipped.clone();
    for f in PALW_T12_TIR_FENCE2_FENCES_V1 {
        (f.set)(&mut never, Some(ForkActivation::never()));
    }
    never.validate_palw_v2().expect("never() is dormant");
    assert_eq!(ids(&never).1, bi, "never(): the identity");
    assert!(!never.palw_tir_fence2_active_at(u64::MAX - 1));
}

#[test]
fn the_fork_id_keeps_an_int8_node_below_3600_and_refuses_it_from_3600() {
    let old = palw_t12_release_v4_params();
    let new = palw_t12_release_v5_params();
    for daa in [2_000, 3_000, FLAG_DAY - 1] {
        let o = fork_id_v1(&old, daa);
        let n = fork_id_v1(&new, daa);
        assert_eq!(n.next, FLAG_DAY, "the new build announces 3,600 next");
        assert!(!evaluate_fork_id_v1(&new, daa, o.fired.as_bytes().as_slice(), o.next).refuses(), "new keeps old at {daa}");
        assert!(!evaluate_fork_id_v1(&old, daa, n.fired.as_bytes().as_slice(), n.next).refuses(), "old keeps new at {daa}");
    }
    for daa in [FLAG_DAY, FLAG_DAY + 1, 4_500] {
        let o = fork_id_v1(&old, daa);
        let n = fork_id_v1(&new, daa);
        assert!(evaluate_fork_id_v1(&new, daa, o.fired.as_bytes().as_slice(), o.next).refuses(), "new refuses old at {daa}");
        assert!(evaluate_fork_id_v1(&old, daa, n.fired.as_bytes().as_slice(), n.next).refuses(), "old refuses new at {daa}");
    }
}

#[test]
fn the_flag_day_holds_its_prerequisites_by_name() {
    let shipped = palw_t12_release_v5_params();
    let refused_by = |p: &Params, needle: &str, why: &str| {
        let e = p.validate_palw_v2().expect_err(why);
        assert!(format!("{e:?}").contains(needle), "{why}: {e:?}");
    };
    // `palw_tir_fence2` needs `palw_tir_v1` at or below it.
    let ir = PALW_T12_TIR_FLAG_DAY_DAA.expect("the IR flag day");
    let mut early = shipped.clone();
    (PALW_T12_TIR_FENCE2_FENCES_V1[0].set)(&mut early, Some(ForkActivation::new(ir - 1)));
    refused_by(&early, "palw_tir_v1", "fence2 below the IR fence");
    let mut at_ir = shipped.clone();
    (PALW_T12_TIR_FENCE2_FENCES_V1[0].set)(&mut at_ir, Some(ForkActivation::new(ir)));
    at_ir.validate_palw_v2().expect("fence2 at the IR fence's own height");
    let mut no_ir = shipped.clone();
    no_ir.palw_tir_v1 = None;
    refused_by(&no_ir, "palw_tir", "fence2 with no IR fence at all");
    // The dormant window's prerequisite, held for a ruleset that arms it: `palw_model_court_window` needs
    // `palw_kary_court` at or below it (`always()` on testnet-12).
    assert_eq!(shipped.palw_kary_court, Some(ForkActivation::always()), "testnet-12's k-ary court is in force from genesis");
    for kary in [Some(ForkActivation::new(FLAG_DAY + 1)), Some(ForkActivation::never()), None] {
        let mut p = shipped.clone();
        p.palw_kary_court = kary;
        // The other fences that read the k-ary court (the IR fence's own prerequisite among them) may answer
        // first on a ruleset this broken: all that is asserted here is that it does not validate.
        assert!(p.validate_palw_v2().is_err(), "{kary:?}: a ruleset with no k-ary court at the flag day does not validate");
    }
    // The window armed on the int-8 baseline (a drill's `--palw-drill-model-court-at` does this) validates.
    let mut armed = palw_t12_release_v4_params();
    (PALW_T12_MODEL_COURT_WINDOW_FENCES_V1[0].set)(&mut armed, Some(ForkActivation::new(FLAG_DAY)));
    armed.validate_palw_v2().expect("the dormant window, armed over the baseline, validates");
    // The model-window check ON ITS OWN (`validate_palw_model_court_window`): on a ruleset whose genesis registers a
    // fused row the k-ary court's own refusals answer first, so the rule is asked directly.
    let window_with = |kary: Option<ForkActivation>, at: u64| {
        let mut p = shipped.clone();
        p.palw_kary_court = kary;
        p.palw_model_court_window = Some(ForkActivation::new(at));
        p.validate_palw_model_court_window()
    };
    for (kary, at, ok) in [
        (Some(ForkActivation::always()), FLAG_DAY, true),
        (Some(ForkActivation::new(FLAG_DAY)), FLAG_DAY, true),
        (Some(ForkActivation::new(FLAG_DAY - 100)), FLAG_DAY, true),
        (Some(ForkActivation::new(FLAG_DAY + 1)), FLAG_DAY, false),
        (Some(ForkActivation::never()), FLAG_DAY, false),
        (None, FLAG_DAY, false),
    ] {
        let verdict = window_with(kary, at);
        assert_eq!(verdict.is_ok(), ok, "kary {kary:?}, window {at}: {verdict:?}");
        if !ok {
            let why = format!("{:?}", verdict.expect_err("refused"));
            assert!(why.contains("palw_model_court_window is armed before palw_kary_court"), "{why}");
        }
    }
    let mut dormant_window = shipped.clone();
    dormant_window.palw_kary_court = None;
    for window in [None, Some(ForkActivation::never())] {
        dormant_window.palw_model_court_window = window;
        dormant_window.validate_palw_model_court_window().expect("a dormant window needs no k-ary court");
    }
    // `never()` is dormant and needs nothing.
    let mut dormant = palw_t12_release_v4_params();
    dormant.palw_model_court_window = Some(ForkActivation::never());
    dormant.validate_palw_v2().expect("never() needs nothing");
}

/// **What a registered model gets**, at testnet-12's own court: the minimum complete dispute window of its shape.
/// Finite, never below the network window, non-decreasing in the history, and bounded by the court's structure
/// (`(2·(ladder + history) + terminal + 1)·turn_deadline + the assembly reserve + 1`) — there is no shared
/// ceiling, and none is needed: a hostile shape cannot push it past what the structure allows. The numbers are
/// printed (`--nocapture`) for `docs/design/palw/t12-int10-court-window-review.md`.
#[test]
fn a_models_window_is_finite_floored_at_the_network_window_and_bounded_by_the_courts_structure() {
    use kaspa_consensus_core::palw_class_admission_v2::palw_court_window_for_history_v1;
    use kaspa_consensus_core::palw_court_v2::palw_court_params_held_at_v2;
    let shipped = palw_t12_release_v5_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &shipped.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let network = bundle.state.window_court();
    // The held fence is armed on testnet-12 from genesis, and admission charges the NETWORK's regime: no class's
    // window carries the leaf ladder's rounds (palw_class_admission_v2: "no class's window is charged the ladder").
    assert!(shipped.palw_held_context_active_at(0) && shipped.palw_held_context_active_at(FLAG_DAY), "held from genesis");
    // testnet-12 is a held network (its 2M row): the held derivation is the only one that has an admissible arity here.
    for held in [true] {
        let court = palw_court_params_held_at_v2(bundle, true, held).expect("testnet-12's court derives an arity");
        let (ladder, turn, terminal) =
            (u64::from(court.bisection_rounds()), court.turn_deadline_daa(), u64::from(court.terminal_rounds()));
        let reserve = kaspa_consensus_core::palw_context_ladder::palw_close_assembly_daa_v1(court.max_close_chunks());
        // 64 history rounds is the most `⌈log_k(positions/tile)⌉` can be at any arity ≥ 2 over a u64 space.
        let ceiling = (2 * (ladder + 64) + terminal + 1) * turn + reserve + 1;
        println!(
            "WINDOW held={held}: network {network}, turn {turn}, ladder rounds {ladder}, terminal {terminal}, arity {}, reserve {reserve}, structural ceiling {ceiling}",
            court.dissection_arity()
        );
        let mut last = 0u64;
        for history in [1u64, 16, 512, 4_096, 32_768, 131_072, 1 << 20, 2_000_000, 1 << 32, 1 << 48, u64::MAX] {
            for tile in [1u32, 16, 64, 1_024] {
                let w = palw_court_window_for_history_v1(network, true, held, &court, history, tile).expect("finite for every shape");
                assert!(w >= network, "never below the network window ({history}/{tile}: {w})");
                assert!(w <= ceiling.max(network), "bounded by the court's structure ({history}/{tile}: {w} > {ceiling})");
                if tile == 16 {
                    assert!(w >= last, "non-decreasing in the history ({history}: {w} < {last})");
                    last = w;
                    println!("WINDOW held={held} history={history} tile=16 -> {w}");
                }
            }
        }
        // At testnet-12's held court the window exceeds the network's only for absurd shapes: every history up to 2^32
        // positions at tile 16 derives the network window itself (the fence would change no admission there).
        for history in [512u64, 32_768, 131_072, 1 << 20, 2_000_000, 1 << 32] {
            assert_eq!(palw_court_window_for_history_v1(network, true, held, &court, history, 16), Ok(network), "{history}");
        }
        assert!(palw_court_window_for_history_v1(network, true, held, &court, 1 << 48, 16).expect("finite") > network);
        // History zero (no dissected cone) and a zero tile: the network window, and a refusal that names the shape.
        assert_eq!(palw_court_window_for_history_v1(network, true, held, &court, 0, 16), Ok(network));
        assert!(palw_court_window_for_history_v1(network, true, held, &court, 100, 0).is_err(), "a zero tile has no finite bound");
        // Inactive: the network window whatever the shape.
        assert_eq!(palw_court_window_for_history_v1(network, false, held, &court, u64::MAX, 1), Ok(network));
    }
}
