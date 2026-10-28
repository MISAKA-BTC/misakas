//! **ADR-0152 §4-quater P-1 against kaspa's own depth interlocks, the identity's normalisation, and
//! V2(d) on testnet-12's real classes** (adopted from the second review's probe of
//! `feat/t12-class-verify-deadline`, with its id pins left to `palw_offence_attribution_is_t12_only`,
//! which re-pins them on every change that moves testnet-12).
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_class_verify_p1_on_t12 -- --nocapture`

#[path = "rcore_common.rs"]
mod common;
use common::*;
use kaspa_consensus_core::config::params::{
    ForkActivation, devnet_shipped_params, mainnet_shipped_params, palw_class_verify_reff_span_daa_v1, palw_rc_shipped_params,
    palw_t12_shipped_params, palw_v2_claim_lattice_daa_v1, palw_v2_pruning_depth_for_lattice_v1, palw_v2_pruning_depth_v1,
};
use kaspa_consensus_core::palw_class_verify_deadline_v1::{PALW_CLASS_VERIFY_LONG_D_DAA_V1, PalwClaimVerifyShapeV1};
use kaspa_consensus_core::palw_state_v2::{palw_class_needs_measured_row_v1, palw_panel_holds_to_final_v1};

/// **P-1's 74,920 meets every interlock kaspa puts on a pruning depth**: finality 600, the pruning
/// consistency predicate (`k < P mod F < F − k`, `pruning.rs`), the anticone finalization depth
/// under it; the lattice is `2(600 + 16,000) + 1,200 + 3,000 + R_eff`'s span 37,520 with no rounding
/// bump; the short window is armed from genesis (the fence's prerequisite); and re-deriving the depths
/// on the shipped params is a no-op. Every other preset keeps the fence dormant and testnet-11 its
/// 12,002.
#[test]
fn p1_the_t12_depth_meets_kaspas_interlocks() {
    for (name, p) in [("t12 from NetworkId", t12()), ("t12 shipped", palw_t12_shipped_params())] {
        let b = bundle(&p);
        let (pd, fd, k) = (p.pruning_depth(), p.finality_depth(), p.ghostdag_k() as u64);
        println!(
            "{name}: pruning {pd} finality {fd} k {k} merge {} mergeset {} anticone {} P mod F {} steps {}",
            p.merge_depth(),
            p.mergeset_size_limit(),
            p.anticone_finalization_depth(),
            pd % fd,
            pd.div_ceil(fd)
        );
        assert_eq!(pd, 74_920, "{name}");
        assert_eq!(fd, 600, "{name}");
        assert!(k < pd % fd && pd % fd < fd - k, "{name}: pruning.rs's assert_pruning_depth_consistency");
        assert!(p.anticone_finalization_depth() < pd, "{name}");
        let lattice = palw_v2_claim_lattice_daa_v1(&b, p.palw_da_court, p.palw_class_verify_deadline);
        assert_eq!(lattice, 74_920);
        assert_eq!(palw_class_verify_reff_span_daa_v1(&b.state), 37_520);
        assert_eq!(2 * (600 + 16_000) + 1_200 + 3_000 + 37_520, 74_920, "the lattice's terms");
        assert_eq!(palw_v2_pruning_depth_for_lattice_v1(&p.blockrate, &b, lattice), 74_920, "no rounding bump");
        assert_eq!(palw_v2_pruning_depth_v1(&p.blockrate, &b, p.palw_da_court, p.palw_class_verify_deadline), 74_920);
        assert_eq!(p.palw_short_challenge_window, Some(ForkActivation::always()), "{name}: the short window from genesis");
        p.validate_palw_v2().expect("validates");
        let re = p.clone().with_palw_v2_depths(&b);
        assert_eq!(re.blockrate.pruning_depth, 74_920, "{name}: with_palw_v2_depths is idempotent");
        assert_eq!(re.consensus_params_id(), p.consensus_params_id(), "{name}");
    }
    for (name, p) in [("t11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params()), ("mainnet", mainnet_shipped_params())] {
        println!("{name}: pruning {} fence {:?}", p.pruning_depth(), p.palw_class_verify_deadline);
        assert_eq!(p.palw_class_verify_deadline, None, "{name}");
    }
    assert_eq!(palw_rc_shipped_params().pruning_depth(), 12_002);
    // The depth back to the fence-off derivation moves the params and identity ids and not the
    // schedule (no height moved).
    let t12 = palw_t12_shipped_params();
    let mut back = t12.clone();
    let b = bundle(&back);
    back.blockrate.pruning_depth = palw_v2_pruning_depth_v1(&back.blockrate, &b, back.palw_da_court, None);
    assert_eq!(back.blockrate.pruning_depth, 12_002);
    assert_ne!(back.consensus_params_id(), t12.consensus_params_id(), "the depth is in the params id");
    assert_ne!(back.consensus_identity_id(), t12.consensus_identity_id(), "and in the identity");
    assert_eq!(back.consensus_schedule_id(), t12.consensus_schedule_id(), "and not in the schedule");
}

/// **The identity's DA-court normalisation with the class-verify fence on**: a build that SCHEDULES
/// the court at a height and one that does not share an identity (the depth is re-derived with the
/// same fence), and a genesis court does not.
#[test]
fn p1_identity_normalisation_with_the_fence_on() {
    let t12 = t12();
    let b = bundle(&t12);
    let mut unscheduled = t12.clone();
    unscheduled.palw_da_court = None;
    unscheduled.blockrate.pruning_depth =
        palw_v2_pruning_depth_v1(&unscheduled.blockrate, &b, None, unscheduled.palw_class_verify_deadline);
    let mut scheduled = t12.clone();
    scheduled.palw_da_court = Some(ForkActivation::new(50_000));
    scheduled.blockrate.pruning_depth =
        palw_v2_pruning_depth_v1(&scheduled.blockrate, &b, scheduled.palw_da_court, scheduled.palw_class_verify_deadline);
    println!(
        "fence on: depth without DA court {} / with a scheduled one {}",
        unscheduled.blockrate.pruning_depth, scheduled.blockrate.pruning_depth
    );
    assert_eq!(scheduled.blockrate.pruning_depth, 74_920);
    assert_eq!(unscheduled.consensus_identity_id(), scheduled.consensus_identity_id(), "a scheduled court is not yet a rule");
    assert_ne!(t12.consensus_identity_id(), unscheduled.consensus_identity_id(), "a genesis court is");
}

/// **V2(d) refuses no honest launch claim** (the review's L3), on both lanes, at launch and past the
/// 2M flag day: no testnet-12 class has a claim whose own `D` passes 120 without the room holding the
/// class to Final — the 2M row is held (C7), so even past its flag day V2(d) never refuses a 2M claim.
#[test]
fn v2d_refuses_no_launch_class_claim() {
    for p in [t12(), t12_2m_open()] {
        let (sp, s) = (bundle(&p).state, genesis_state(&p));
        let classes = genesis_classes(&p);
        let floor = classes[0].0;
        for (class, leaves, _, _) in &classes {
            let profile = bundle(&p).genesis_objects.iter().find_map(|o| match o {
                PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(carriage), .. } if class_id == class => {
                    Some(carriage.profile.clone())
                }
                _ => None,
            });
            let profile = profile.or_else(|| {
                (*class == floor).then(|| {
                    kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
                        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
                    )
                    .expect("BASE-0's profile")
                })
            });
            let Some(profile) = profile else { panic!("{class}: no profile") };
            let published = edited(&sp, &s, |carriage| {
                carriage.fp_work_profiles.insert(*class, Box::new(profile.clone()));
            });
            let held = palw_panel_holds_to_final_v1(&sp, &published, class);
            let nm = palw_class_needs_measured_row_v1(&sp, &published, class);
            for shape in [
                PalwClaimVerifyShapeV1::Attempt,
                PalwClaimVerifyShapeV1::FreePrompt { work_leaves: 1 },
                PalwClaimVerifyShapeV1::FreePrompt { work_leaves: *leaves },
                PalwClaimVerifyShapeV1::FreePrompt { work_leaves: u64::from(u32::MAX) },
            ] {
                let d = sp.claim_verify_daa_v1(&published, class, shape, 1_000);
                let refused_by_v2d = d > PALW_CLASS_VERIFY_LONG_D_DAA_V1 && !held;
                println!(
                    "class {class}: n_ctx {} held {held} nm {nm} {shape:?}: D {d} -> V2(d) refuses {refused_by_v2d}",
                    profile.n_ctx
                );
                assert!(!refused_by_v2d, "{class}: V2(d) would refuse an honest launch claim");
            }
        }
    }
}
