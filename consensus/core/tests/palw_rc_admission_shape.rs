//! **The admission shape on the preset that SHIPS, not the one that drills.**
//!
//! `e5651de0` made `Params::palw_context_ladder` load-bearing: `palw_admission_shape_at_v1` now
//! derives the ladder half from that fence. Its own test builds `devnet_shipped_params()` — and
//! devnet is the one shipped preset that ARMS the ladder (`only_devnet_arms_the_context_ladder`).
//!
//! So the drill validates the graph-v5@512 registration under `ladder: Some(..)` and testnet-11
//! registers it under `ladder: None`. Same class, different admission shape. **A green devnet
//! drill is evidence about devnet**, and the thing a launch turns on is the registration under the
//! RC preset.
//!
//! This asks the RC preset directly. It is a separate file rather than an addition to
//! `admission_shape_tests` because that module arrived in `e5651de0` and belongs to the session
//! writing it; two writers in one module is how a merge conflict is manufactured at a freeze.

use kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// **The RC preset arms the court and leaves the ladder dormant — and the shape says both.**
///
/// Not a restatement of the fence values: it asserts that the shape the PREFLIGHT computes for the
/// row the genesis registers is the one the fences imply, on the preset that ships. If FG's arming
/// of `palw_kary_court` ever regresses, the court half goes `None` here and a fused row is refused
/// by name — which is the failure this whole branch exists to keep visible.
#[test]
fn the_rc_preset_admission_shape_is_court_armed_and_ladder_dormant() {
    let rc = kaspa_consensus_core::config::params::palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &rc.palw_consensus_mode else {
        panic!("the RC preset ships a ConsensusV2 bundle");
    };
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_graph_v5_profile_v1().expect("the graph-v5 profile derives");

    let shape = palw_admission_shape_at_v1(&rc, bundle, &profile, 0).expect("the RC court has a shape at genesis");

    // The court half: FG arms `palw_kary_court` because a genesis that registers a fused row must.
    let court = shape.court.expect("the RC preset arms palw_kary_court — FG registers a fused row and validate_palw_v2 requires it");
    let derived = kaspa_consensus_core::palw_court_v2::palw_court_params_at_v2(bundle, true).expect("the armed court derives");
    assert_eq!(court.dissection_arity, derived.dissection_arity(), "the arity is the DERIVED one, not a literal");
    assert_eq!(court.window_court_daa, bundle.state.window_court());
    assert_eq!(court.prompt_ids_form, rc.palw_prompt_ids_form_at(0), "the ids form is the preset's, not a hardcoded MerkleV1");

    // The ladder half: dormant on t11. This is the half that differs from devnet, and it is the
    // reason a green devnet drill does not speak for this preset.
    assert!(
        rc.palw_context_ladder.is_none(),
        "the RC preset armed palw_context_ladder — if that is deliberate the fingerprint moved and \
         this cut's re-pin set is wrong; if not, it is an arming nobody decided"
    );
    assert!(shape.ladder.is_none(), "the ladder half must follow its fence: dormant on t11, so no rules");

    println!(
        "RC admission shape @ daa 0: court arity {} {:?} ids window {} | ladder {}",
        court.dissection_arity,
        court.prompt_ids_form,
        court.window_court_daa,
        if shape.ladder.is_some() { "Some" } else { "None (fence dormant)" }
    );
}

/// **The admission itself, under t11's shape — the claim the announcement rests on.**
///
/// The sibling test above measures the SHAPE and says so. This asks the gate. They are different
/// assertions and conflating them is what §1 did when it read FG's genesis-assembly result as an
/// admission-gate result: the genesis route computes a row's ladder rules from the bundle
/// *regardless of any fence*, so "admits and prices at 2^26 dormant" was never about the gate.
///
/// **Three routes each look like they cover this and none does** (5b's enumeration, and it is the
/// clearest statement of why a well-tested thing can be untested):
///
/// * the genesis route bypasses the fence entirely;
/// * `classes.rs`'s `armed_rulesets` passes `Some(rules)` for BOTH presets, so it never asks with
///   `None` — that is my own helper and it never occurred to me that it could not express the
///   case that ships;
/// * `e5651de0`'s `admission_shape_tests` runs on devnet, the one shipped preset that arms the
///   ladder.
///
/// So this asks `verify_class_admission_v6` with exactly what `palw_admission_shape_at_v1` returns
/// for the RC preset — court armed, ladder dormant — using the registration object out of the
/// genesis set rather than one built here. **5b is asserting the same thing through the SDK
/// preflight; this goes through `consensus/core`.** Two paths, two authors, one claim: if they
/// disagree, that disagreement is worth more than either result.
///
/// **THE RULE THIS MIRRORS LIVES AT `consensus/src/pipeline/virtual_processor/processor.rs`, in
/// the `ClassRegistered` arm** — it derives the court from `palw_kary_court_active_at`, the ladder
/// from `palw_context_ladder_at(daa).then(...)`, and calls `verify_class_admission_v6` with both.
/// That is the ACCEPTANCE path, so this is not a test of the panel's opinion: it is a test of the
/// rule the chain applies. `e5651de0` did not touch the processor — it made the preflight agree
/// with a rule that was already there, and the earlier disagreement between them is what hid it.
///
/// **If someone changes the processor's ladder derivation, this test must change with it or it
/// becomes a false red** — it would keep asking with `ladder: None` after the chain stopped. That
/// is a real hazard and this paragraph is the only thing guarding against it, so: the assertion
/// below is valid exactly while the processor derives the ladder from the fence. A fix that makes
/// the processor compute rules from the bundle regardless (the way the GENESIS route already does)
/// is a legitimate repair and would require rewriting this test, not deleting it.
///
/// Conversely, a fix that repairs only `palw_admission_shape_at_v1` would turn this test and 5b's
/// green **while the chain still refuses** — a green suite over a closed door. Any candidate fix
/// should be read against the processor arm before it is believed.
#[test]
fn the_fused_row_admits_under_t11s_own_shape_with_the_ladder_dormant() {
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

    let rc = kaspa_consensus_core::config::params::palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &rc.palw_consensus_mode else {
        panic!("the RC preset ships a ConsensusV2 bundle");
    };
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_graph_v5_profile_v1().expect("the graph-v5 profile derives");
    let class_id = profile.shape_profile_id();

    // The registration the CHAIN holds, not one assembled here — the whole point is to ask about
    // the object that ships.
    let registration = bundle
        .genesis_objects
        .iter()
        .find(|o| matches!(o, PalwConsensusObjectV2::ClassRegistered { class_id: id, .. } if *id == class_id))
        .unwrap_or_else(|| panic!("the RC genesis does not register {class_id} — FG is not in this tree"));
    let carriage = match registration {
        PalwConsensusObjectV2::ClassRegistered { admission: Some(a), .. } => a,
        _ => panic!("the fused row's registration carries no admission carriage"),
    };

    let shape = palw_admission_shape_at_v1(&rc, bundle, &profile, 0).expect("the RC court has a shape at genesis");
    assert!(shape.ladder.is_none(), "this test is about the DORMANT ladder; the RC preset has armed it");

    let admitted = kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v6(
        bundle,
        &carriage.profile,
        &carriage.canonical,
        registration,
        &kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1(),
        &[],
        shape.ladder.clone(),
        shape.court,
        rc.palw_fp_decode_rules.is_some(),
    );

    let entry = admitted.unwrap_or_else(|e| {
        panic!(
            "the row testnet-11 REGISTERS does not admit under testnet-11's own shape \
             (court {:?}, ladder None): {e:?}. The devnet drill passes because devnet arms the \
             ladder; this is the preset that ships.",
            shape.court
        )
    });
    println!(
        "t11 shape admission: class {} admitted, close {} B, {} terminal MACs",
        &entry.class_id.to_string()[..16],
        entry.court_cost.max_close_bytes,
        entry.court_cost.max_terminal_macs
    );
}
