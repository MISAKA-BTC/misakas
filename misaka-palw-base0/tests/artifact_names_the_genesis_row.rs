//! **Does the class an ARTIFACT names equal the class GENESIS registers?**
//!
//! Every other test in this area builds both ends of the pairing from one source of truth and
//! checks they match — which they do, necessarily, because one call made both. The pairing that
//! decides whether the network works is between two INDEPENDENT productions of the same value:
//!
//! * what a **file on disk** says, through the route `palw-certify bind --artifact` and the panel
//!   take — the header's `max_position`, `a16_row_for_artifact_shape_v1`, its projection;
//! * what the **genesis catalog** ships — `a16_graph_v5_row_v1`, the row 5f registers.
//!
//! Nothing forces those to agree. If they disagree, `bind` writes a `ClassLaneCertified` naming a
//! class the chain will never hold, the chain accepts the object because it is well-formed, and
//! the lane stays closed — the operator's first symptom is a free-prompt refusal that reads like
//! the context-width wall. That is the failure this file exists to make loud, and it has already
//! happened once: a registration that took its width from a catalog row while the artifact said
//! something else.
//!
//! **The header route is exercised twice**, because they answer different questions: from the
//! declared shape (always available, so the check always runs) and from the real `.palwart` when
//! `MISAKA_PALW_ARTIFACT` names one. The second is the one that would catch a converter whose
//! header stopped matching what this build believes it writes.

use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use misaka_palw_base0::classes::*;

fn court() -> PalwCourtParamsV2 {
    PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("the shipped court params project")
}

/// **The class id an artifact names must be the class id genesis registers.**
///
/// Asserted on `shape_profile_id` and not on `n_ctx`: two rows can both be 512 and be different
/// classes, which is the whole reason a width in a model id was not enough on its own.
#[test]
fn the_row_an_artifact_names_is_the_row_genesis_registers() {
    let genesis = a16_graph_v5_row_v1().expect("the graph-v5 dense row projects");
    let genesis_id = genesis.profile.shape_profile_id();

    let derived = a16_row_for_artifact_shape_v1(&court(), &genesis.artifact_shape, None)
        .expect("the genesis row's own artifact shape names an A16 row");

    assert_eq!(
        derived.n_ctx, genesis.profile.n_ctx,
        "the artifact route derived n_ctx {} and genesis registers {}",
        derived.n_ctx, genesis.profile.n_ctx
    );
    assert_eq!(
        derived.profile.shape_profile_id(),
        genesis_id,
        "SAME WIDTH, DIFFERENT CLASS.\n  \
         genesis  ({}) projects {}\n  \
         artifact (header max_position {}) projects {}\n\
         Both are n_ctx {}. `palw-certify bind --artifact` would name the second and genesis \
         registers the first, so the ClassLaneCertified names a class the chain will never hold: \
         it is accepted, the lane stays closed, and the first symptom is a free-prompt refusal \
         that reads like the width wall.",
        genesis.model_id,
        genesis_id,
        genesis.artifact_shape.max_position,
        derived.profile.shape_profile_id(),
        derived.n_ctx
    );
}

/// The same equality against the REAL file, which is the only version that can catch a converter
/// whose header stopped saying what this build believes it writes. Skips loudly: a header check
/// that quietly did not run is worth less than no check.
#[test]
fn the_shipped_artifact_names_the_row_genesis_registers() {
    let Ok(path) = std::env::var("MISAKA_PALW_ARTIFACT") else {
        println!("SKIPPED: set MISAKA_PALW_ARTIFACT to the shipped .palwart. No real header was checked.");
        return;
    };
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let artifact = misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes).unwrap_or_else(|e| panic!("{path}: {e:?}"));
    let genesis = a16_graph_v5_row_v1().expect("the graph-v5 dense row projects");

    assert_eq!(
        artifact.shape.max_position, genesis.artifact_shape.max_position,
        "the shipped artifact's rotary span is {} and the genesis row was projected at {} — the \
         converter and the catalog disagree about what this family's weights can execute",
        artifact.shape.max_position, genesis.artifact_shape.max_position
    );
    let derived =
        a16_artifact_row_v1(&court(), &artifact, None).unwrap_or_else(|e| panic!("the shipped artifact names no A16 row: {e}"));
    assert_eq!(
        derived.profile.shape_profile_id(),
        genesis.profile.shape_profile_id(),
        "the shipped artifact at {path} names class {} and genesis registers {}",
        derived.profile.shape_profile_id(),
        genesis.profile.shape_profile_id()
    );
}
