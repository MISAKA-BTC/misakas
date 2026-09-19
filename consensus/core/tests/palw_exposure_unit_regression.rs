//! **The exposure ceiling and the fold's ledger price one claim with one expression.**
//!
//! Regression for the 2026-09-19 re-audit's finding (a), which two independent agents reached
//! separately. The fold's ledger write had been repointed onto ADR-0145's derived work while the
//! admission ceiling still measured the registrant's declared leaves, so past `palw_canonical_work`
//! the gate would admit an attempt the ledger then reserved 2,809×–12,533× against. The next
//! attempt trips `ExposureCeilingExceeded`, and on the floor class — the one every node must be
//! able to produce on — that is the chain stopping for being used. It was not reachable on any
//! shipped preset, because the fence is dormant everywhere; it would have fired on the flag day.
//!
//! Two things had to be true and only one was: the two sites must run ONE expression, and that
//! expression must not change the unit the collateral was posted in. `palw_exposure_pwu_v3` over
//! `PalwExposureBasisV1` does both — see its doc for why the floor class is the normaliser and why
//! the normaliser is derived rather than chosen.

use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5;
use kaspa_consensus_core::palw_economic_compute_v1::{PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1};
use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_A16_GRAPH_V5_N_CTX, qwen25_a16_graph_v5_canonical_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwClassStateV2, PalwClassStatusV2, PalwExposureBasisV1, PalwPwuRuleV2, palw_exposure_pwu_v1, palw_exposure_pwu_v3,
};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1};
use kaspa_hashes::Hash64;

/// The shipped genesis registry's value (palw_genesis_v2.rs:497).
const SLASH_VALUE_PER_PWU: u64 = 5;

fn class_of(pwu_per_inference: u64) -> PalwClassStateV2 {
    PalwClassStateV2 {
        artifact_root: Hash64::default(),
        slash_value_per_pwu: SLASH_VALUE_PER_PWU,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
        status: PalwClassStatusV2::Active,
        registered_daa: 0,
        registrant_bond: None,
        fused_attention: false,
    }
}

fn shipped_ladder() -> u64 {
    let rc = kaspa_consensus_core::config::params::palw_rc_shipped_params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &rc.palw_consensus_mode else {
        panic!("the shipped release params are a ConsensusV2 bundle");
    };
    bundle.court.max_step_leaf_count()
}

fn declared_leaves(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u64 {
    let job = rc_job_context(profile, canonical.0, canonical.1);
    step_leaf_count_capped_v1(profile, &job, shipped_ladder()).expect("the shipped row counts")
}

fn one_draw_mac_eq(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    let job = rc_job_context(profile, canonical.0, canonical.1);
    palw_attempt_economic_compute_v1(profile, &job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("the shipped row walks")
}

#[test]
fn the_ceiling_and_the_ledger_price_one_claim_with_one_expression() {
    let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the shipped floor builds");
    let dense = palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).expect("the shipped @512 row builds");

    // The basis is the FLOOR class's own two measures of one draw, read off the same chain rows
    // every node holds — `PalwChainStateV2::palw_exposure_basis_v1` builds exactly this pair.
    let floor_declared = declared_leaves(&floor, PALW_RC_BASE0_CANONICAL);
    let floor_drawn = one_draw_mac_eq(&floor, PALW_RC_BASE0_CANONICAL) as u64;
    let basis = PalwExposureBasisV1 { base_declared: floor_declared, base_canonical: floor_drawn };

    for (name, profile, canonical) in [
        ("BASE-0 floor", floor.clone(), PALW_RC_BASE0_CANONICAL),
        ("Qwen2.5-A16 @512", dense, qwen25_a16_graph_v5_canonical_v1()),
    ] {
        let leaves = declared_leaves(&profile, canonical);
        let drawn = one_draw_mac_eq(&profile, canonical) as u64;
        let class = class_of(leaves);
        // The claimed pwu is irrelevant on the `DerivedV1` arm; every face ignores it.
        let claimed_pwu = 1_234_567u64;

        let declared = palw_exposure_pwu_v1(&class, claimed_pwu);
        let dormant = palw_exposure_pwu_v3(&class, claimed_pwu, None, None);
        let armed = palw_exposure_pwu_v3(&class, claimed_pwu, Some(drawn), Some(basis));
        let unnormalised = drawn;

        // 1. Below the fence — every shipped preset — the new face is the old one byte for byte.
        assert_eq!(dormant, declared, "{name}: a dormant fence moves no reservation");

        // 2. A half-applied unit is what produced the divergence, so a derived draw with no basis
        //    to normalise it against keeps the declared basis rather than mixing the two.
        assert_eq!(
            palw_exposure_pwu_v3(&class, claimed_pwu, Some(drawn), None),
            declared,
            "{name}: no basis, no change of unit"
        );

        // 3. The floor class is EXACTLY unchanged past the fence. It is the normaliser, and it is
        //    also where liveness lives: every node must be able to produce on it.
        if name == "BASE-0 floor" {
            assert_eq!(armed, declared, "the floor class normalises to itself, exactly");
        }

        // 4. The bug, stated as a bound. Raw derived work inflated this class's reservation by the
        //    ratio below; normalised, it moves only by this class's work RELATIVE to the floor,
        //    which is what invariant (viii) actually asks for.
        let raw_inflation = unnormalised / declared.max(1);
        let real_move = (armed as f64) / (declared as f64);
        println!(
            "{name:18} declared={declared:>10}  raw_derived={unnormalised:>14} ({raw_inflation:>6}x)  normalised={armed:>10} ({real_move:>6.2}x)"
        );
        assert!(
            real_move < 10.0,
            "{name}: the normalised reservation moves by real relative work ({real_move}x), not by the change of unit ({raw_inflation}x)"
        );
        assert!(armed >= declared / 2, "{name}: and it does not collapse either");
    }
}

/// **Both call sites name `palw_exposure_pwu_v3`, and neither computes the quantity itself.**
///
/// The divergence was not a wrong number, it was a SECOND expression — the ceiling kept the old one
/// when the ledger was repointed. A behavioural test cannot see a third copy appearing next week,
/// so this reads the two files. It is the same guard shape the fold already uses for the court arms.
#[test]
fn neither_the_ceiling_nor_the_ledger_carries_its_own_copy_of_the_expression() {
    let gate = include_str!("../src/palw_admission_v2.rs");
    let fold = include_str!("../src/palw_state_v2.rs");
    let fold_body = &fold[..fold.find("\n#[cfg(test)]").expect("the tests follow the fold")];

    assert!(gate.contains("palw_exposure_pwu_v3(class, attempt.pwu, canonical_draw, exposure_basis)"), "the ceiling runs the shared expression");
    assert!(
        fold_body.contains("palw_exposure_pwu_v3(class, attempt.pwu, canonical_draw, exposure_basis)"),
        "and so does the ledger write, over the same two inputs"
    );
    assert!(
        !gate.contains("palw_exposure_pwu_v1(class, attempt.pwu)"),
        "the ceiling no longer keeps a declared-basis copy — that copy IS the finding"
    );
}
