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

    let derived = a16_row_for_artifact_shape_v1(&court(), &genesis.artifact_shape, None, None)
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
        a16_artifact_row_v1(&court(), &artifact, None, None).unwrap_or_else(|e| panic!("the shipped artifact names no A16 row: {e}"));
    assert_eq!(
        derived.profile.shape_profile_id(),
        genesis.profile.shape_profile_id(),
        "the shipped artifact at {path} names class {} and genesis registers {}",
        derived.profile.shape_profile_id(),
        genesis.profile.shape_profile_id()
    );
}

/// **The refusals, exercised — because a route that silently picks is the defect this replaced.**
#[test]
fn an_ambiguous_header_refuses_and_a_model_id_resolves_it() {
    let genesis = a16_graph_v5_row_v1().expect("the graph-v5 dense row projects");
    let shape = &genesis.artifact_shape;

    // n_ctx 16: this build tables several dense rows at that width, and they are different
    // classes. The file cannot say which, so the route must not choose.
    match a16_row_for_artifact_shape_v1(&court(), shape, Some(16), None) {
        Err(A16ArtifactRowError::AmbiguousAtWidth { asked, model_ids }) => {
            assert_eq!(asked, 16);
            assert!(model_ids.len() > 1, "an ambiguity refusal that names one row is not an ambiguity");
            println!("AMBIGUOUS at 16: {model_ids:?}");
            // And the escape hatch resolves it, to that row and not to another.
            let picked = a16_row_for_artifact_shape_v1(&court(), shape, Some(16), Some(model_ids[0]))
                .expect("naming one of the rows resolves the ambiguity");
            assert_eq!(picked.model_id, model_ids[0]);
        }
        Err(e) => panic!("expected an ambiguity refusal at 16, got: {e}"),
        Ok(row) => panic!("n_ctx 16 resolved silently to {} — the route is picking again", row.model_id),
    }

    // A width the registry does not offer is refused rather than projected into a class that
    // nothing registers. 300 is inside the artifact's 512-position span, so this is not the
    // width-vs-span check.
    match a16_row_for_artifact_shape_v1(&court(), shape, Some(300), None) {
        Err(A16ArtifactRowError::NoRowAtWidth { asked, offered }) => {
            assert_eq!(asked, 300);
            assert!(!offered.is_empty(), "a refusal that lists no alternatives cannot be acted on");
            println!("NO ROW at 300; offered {offered:?}");
        }
        Err(e) => panic!("expected a no-row refusal at 300, got: {e}"),
        Ok(row) => panic!("n_ctx 300 produced {} — a width no row spells was projected", row.model_id),
    }
}

/// **The route no longer PROJECTS, so the third spelling cannot arise.**
///
/// Measured elsewhere: `palw-certify bind --n-ctx 16` produced `7a76d29b…` while the panel
/// registers `71bbb755…` — same geometry, same width, different graph, because the ladder-row path
/// applies the court-capable transform and the catalog path does not. A width and a projection do
/// not determine a graph either, unless it is the same projection.
///
/// Matching rather than projecting removes the question: whatever the caller asks for, the profile
/// returned is a catalog row's own, so there is no second projection to disagree with the first.
#[test]
fn the_route_returns_the_catalog_rows_own_profile_at_every_width() {
    let genesis = a16_graph_v5_row_v1().expect("the graph-v5 dense row projects");
    for model_id in ["Qwen/Qwen2.5-1.5B/graph-v2", "Qwen/Qwen2.5-1.5B/graph-v3"] {
        let Some(row) = canonical_class_by_model_id_v1(&court(), model_id) else { continue };
        let derived = a16_row_for_artifact_shape_v1(&court(), &genesis.artifact_shape, Some(row.profile.n_ctx), Some(model_id))
            .unwrap_or_else(|e| panic!("{model_id} at its own width: {e}"));
        assert_eq!(
            derived.profile.shape_profile_id(),
            row.profile.shape_profile_id(),
            "{model_id}: the route returned a profile that is not the catalog row's — it projected instead of matching, \
             which is how a third class id for one width comes into existence"
        );
        assert_eq!(derived.model_id, model_id);
    }
}

/// **The width-only route names the same class, and that is the route that was still projecting.**
///
/// `palw-certify bind` has three forms and only two of them were rewritten to match. `--n-ctx`
/// alone — the form for a machine where the 1.7 GiB artifact is somewhere else — still projected
/// `palw_a16_context_row_profile_v1`, so at 512 it produced a class the chain does not hold while
/// every check downstream passed: the projection reaches kernels the graph-v2 family covers, so
/// `covering_rc_family_v1` found a family, the object was well-formed, and the certificate named
/// the wrong graph in silence. The narrowest possible failure, behind the flag with the least
/// evidence attached to it.
///
/// This asserts on `shape_profile_id` at the genesis width, and the assertion is not vacuous: the
/// second half measures the projection this route used to return and requires it to be a
/// DIFFERENT id. If it were the same, the route could go back to projecting and this test would
/// still be green.
#[test]
fn the_width_only_route_names_the_row_genesis_registers() {
    let genesis = a16_graph_v5_row_v1().expect("the graph-v5 dense row projects");
    let width = genesis.profile.n_ctx;

    let (profile, model_id) =
        a16_ladder_row_v1(&court(), width, None).unwrap_or_else(|e| panic!("--n-ctx {width} names no A16 row: {e}"));
    assert_eq!(
        profile.shape_profile_id(),
        genesis.profile.shape_profile_id(),
        "SAME WIDTH, DIFFERENT CLASS. `bind --n-ctx {width}` names {} ({model_id}) and genesis registers {} ({}). \
         The width-only route is projecting again.",
        profile.shape_profile_id(),
        genesis.profile.shape_profile_id(),
        genesis.model_id
    );
    assert_eq!(model_id, genesis.model_id, "the width chose a row, but not the one genesis registers");

    // The projection this route used to return, kept here as the thing the equality above is
    // distinguishing itself FROM.
    let projected = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v1(width)
        .expect("the v1 ladder projection is still a valid graph at this width");
    assert_ne!(
        projected.shape_profile_id(),
        genesis.profile.shape_profile_id(),
        "the v1 projection and the genesis row are the same class at n_ctx {width}, so the assertion above proves \
         nothing and this route could project again undetected"
    );
}

/// **A width no row spells fails to bind rather than projecting one.** The old route answered
/// every width, because a projection always succeeds — which is exactly why a typo produced a
/// class id instead of an error.
#[test]
fn a_width_no_row_spells_refuses_and_says_what_is_offered() {
    match a16_ladder_row_v1(&court(), 300, None) {
        Err(A16ArtifactRowError::NoRowAtWidth { asked, offered }) => {
            assert_eq!(asked, 300);
            assert!(!offered.is_empty(), "a refusal that lists no alternatives cannot be acted on");
            assert!(
                offered.iter().any(|(m, _)| *m == a16_graph_v5_row_v1().expect("projects").model_id),
                "the offer must include the row genesis registers, or the operator cannot find it: {offered:?}"
            );
        }
        other => panic!("n_ctx 300 is spelled by no A16 row and must refuse, got {other:?}"),
    }
    // And a width TWO rows spell refuses too, naming both — the ambiguity is the point.
    match a16_ladder_row_v1(&court(), 16, None) {
        Err(A16ArtifactRowError::AmbiguousAtWidth { asked, model_ids }) => {
            assert_eq!(asked, 16);
            assert!(model_ids.len() > 1, "an ambiguity refusal that names one row is not an ambiguity");
        }
        other => panic!("n_ctx 16 is spelled by more than one A16 row and must refuse, got {other:?}"),
    }
}

/// **The artifact route resolves without a `--model-id` because of the TABLE, not because of the
/// code — so the table's shape is asserted here rather than left as a note.**
///
/// `a16_row_for_artifact_shape_v1` picks by width and refuses when two rows share one. Today the
/// A16 rows sit at n_ctx 16, 18, 16, 16 and 512, so the graph-v5 row is alone at its width and
/// `bind --artifact` lands on it with nothing else said. That is a property of the catalog, and a
/// sixth A16 row added at 512 would silently turn the working bind into an `AmbiguousAtWidth`
/// refusal — arriving as "certify suddenly refuses" on the day someone registers the row.
///
/// It fails loudly rather than binding the wrong class, so it is a liveness risk and not a safety
/// one. But it is the shape of thing this repository keeps discovering as prose in a card, and a
/// card cannot fail. This can: a row added at 512 for any reason turns this red at build time,
/// with the reason and the fix in the message.
///
/// If a second row at this width is ever genuinely wanted, the repair is not to delete this test —
/// it is to give `palw-certify`'s artifact form a way to disambiguate, because at that point the
/// header alone stops naming a class.
#[test]
fn the_genesis_row_is_the_only_a16_row_at_its_width() {
    let genesis = a16_graph_v5_row_v1().expect("the graph-v5 dense row projects");
    let width = genesis.profile.n_ctx;

    let at_width: Vec<(&str, u32)> = canonical_classes_v1(&court())
        .into_iter()
        .filter(|c| matches!(c.source, ArtifactSourceV1::ConvertedA16))
        .filter(|c| c.profile.n_ctx == width)
        .map(|c| (c.model_id, c.profile.n_ctx))
        .collect();

    assert_eq!(
        at_width.len(),
        1,
        "n_ctx {width} is now spelled by {} A16 rows: {at_width:?}. `palw-certify bind --artifact` \
         picks by width and REFUSES on ambiguity, so the shipped artifact stops naming a class the \
         moment a second row shares this width — the operator sees `AmbiguousAtWidth` where a bind \
         used to succeed. Either move the new row to its own width, or give the artifact form a way \
         to disambiguate; do not delete this test.",
        at_width.len()
    );
    assert_eq!(at_width[0].0, genesis.model_id, "the one row at this width is not the one genesis registers");
}

/// **The other end is the GENESIS OBJECT SET, and every test above this one got that wrong.**
///
/// This file's own header says: *"Every other test in this area builds both ends of the pairing
/// from one source of truth and checks they match — which they do, necessarily, because one call
/// made both."* Then every test above asserts the artifact route equals `a16_graph_v5_row_v1()`,
/// which is a **catalog** row — `canonical_classes_v1`, the same table `a16_row_for_artifact_shape_v1`
/// picks from. Both ends, one source. The header describes the defect and the body commits it.
///
/// The header is kept verbatim on purpose. Someone who reads it and then finds this is a better
/// lesson than a clean file.
///
/// **What the tree actually registers**, printed by `every_shipped_row_clears_the_derived_turn_deadline`:
///
///   f1c5635c…  BASE-0 floor
///   5bd9ae3d…  QWEN36 graph-v3
///   71bbb755…  dense A16 **graph-v2 at n_ctx 16**
///
/// `4277d84f…` — the graph-v5 512 row — is not among them. `params.rs` calls
/// `qwen25_a16_registration_v2` over `QWEN25_1_5B_A16`, whose `n_ctx` is 16, and no
/// `qwen25_a16_registration_v5` exists anywhere in the tree.
///
/// The absence has a second, better proof that does not depend on reading a list: the RC ships
/// `palw_kary_court` DORMANT, and `validate_palw_v2` refuses a genesis set containing a
/// fused-attention class under a dormant fence. A registered v5 row would make
/// `palw_rc_shipped_params()` panic and take every test that touches it. They pass. **The absence
/// is proved by a mechanism that had no idea it was being asked.**
///
/// That coupling is load-bearing for FG and the two halves are not separable: **registering the
/// v5 row without arming `palw_kary_court` in the same change panics the genesis.**
///
/// **This test is RED on this tree and that is correct.** It goes green the moment FG registers
/// the row. A red test that is true is worth more than a green one that is not, and until FG
/// lands `palw-certify bind --artifact` mints a `ClassLaneCertified` for a class the chain will
/// refuse with `MissingClass` — the lane stays closed and the first symptom is a free-prompt
/// refusal that reads like the width wall, which is the exact failure this file exists to prevent.
#[test]
fn the_artifact_names_a_class_the_shipped_genesis_actually_registers() {
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

    let params = kaspa_consensus_core::config::params::palw_rc_shipped_params();
    let bundle = match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b,
        other => panic!("the RC preset is not a ConsensusV2 bundle: {other:?}"),
    };
    let registered: Vec<String> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, .. } => Some(class_id.to_string()),
            _ => None,
        })
        .collect();
    assert!(!registered.is_empty(), "the RC genesis registers no class at all — this test is measuring the wrong bundle");

    let genesis_row = a16_graph_v5_row_v1().expect("the graph-v5 dense row projects");
    let named = a16_row_for_artifact_shape_v1(&court(), &genesis_row.artifact_shape, None, None)
        .expect("the artifact shape names an A16 row")
        .profile
        .shape_profile_id()
        .to_string();

    assert!(
        registered.contains(&named),
        "`palw-certify bind --artifact` names class {named}, and the shipped genesis registers \
         [{}]. A ClassLaneCertified for a class the chain does not hold is accepted as well-formed, \
         refused by apply_object with MissingClass, and shows up to the operator as a free-prompt \
         refusal that reads like the context-width wall.\n\
         \n\
         FG registers the graph-v5 512 row and this goes green then. FG must ALSO arm \
         `palw_kary_court` in the same change: validate_palw_v2 refuses a fused class under a \
         dormant fence, so registering the row alone panics the genesis assembly.",
        registered.join(", ")
    );
}

/// **The close of the row the genesis REGISTERS, priced under the bundle that registers it** —
/// the only derivation in this area that touches no re-armed ruleset and no literal.
///
/// Every other close measurement here builds a ruleset, arms it, registers the row into it and
/// prices the result. That reproduces the shipped court faithfully *when the helper is right*, and
/// on 2026-09-03 it was not: one field of three was a hardcoded `MerkleV1` between two read off
/// the bundle, so the number looked bundle-derived and was priced under a court
/// `validate_palw_v2` refuses to assemble. The figure moved 287 bytes when the field was fixed.
///
/// This asks the shipped params instead: take the profile out of the genesis object's own
/// admission carriage, take arity, window and ids form off the same bundle, and price it. Two
/// independent routes now agree at 83,175 B / one carrier, which is the agreement the single
/// route could not provide.
///
/// **There is no stored close to read.** The carriage carries `profile`, `canonical`,
/// `registrant_bond` and `signature`; the acceptance layer derives the price. So a byte count has
/// no existence apart from a court — which is why the assertion below names all three fields, and
/// why the public announcement quotes the CARRIER COUNT and a command rather than a number.
#[test]
fn the_registered_row_prices_at_one_carrier_under_the_bundle_that_registers_it() {
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
    let params = kaspa_consensus_core::config::params::palw_rc_shipped_params();
    let bundle = match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b,
        other => panic!("not v2: {other:?}"),
    };
    let mut priced = 0;
    println!("ids form at 0: {:?}", params.palw_prompt_ids_form_at(0));
    println!("bundle arity: {}  window_court: {}", bundle.court.dissection_arity(), bundle.state.window_court());
    for o in &bundle.genesis_objects {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, admission, .. } = o {
            let id = class_id.to_string();
            // Only a fused row carries one (FG: "a fused row must carry its graph"), so the floor
            // and the hybrid legitimately have none and are not priced here.
            let Some(a) = admission else {
                println!("GENESIS class {}… has NO admission carriage", &id[..16]);
                continue;
            };
            // The close is DERIVED, never declared — the carriage carries the profile and the
            // acceptance layer prices it. So this prices the registered profile under the shipped
            // bundle's own court, which is the number the chain implies.
            let kary = kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
                dissection_arity: bundle.court.dissection_arity(),
                prompt_ids_form: params.palw_prompt_ids_form_at(0),
                window_court_daa: bundle.state.window_court(),
            };
            let Some(rules) = kaspa_consensus_core::palw_context_ladder::palw_class_ladder_rules_for_court_v1(
                &a.profile,
                Some(kary),
                kaspa_consensus_core::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
            ) else {
                println!("GENESIS class {}… has no ladder rules", &id[..16]);
                continue;
            };
            let rows = kaspa_consensus_core::palw_class_admission_v2::derive_court_cost_rows_v1(&a.profile, rules.cost_shape)
                .unwrap_or_else(|e| panic!("the genesis registers class {}… at a profile that does not price: {e:?}", &id[..16]));
            let b = rows.first().expect("a priced row has a binding node");
            let chunks = kaspa_consensus_core::palw_mode_v2::palw_close_chunks_for_bytes_v1(b.close_bytes);
            println!(
                "GENESIS class {}… n_ctx {} arity {} {:?} ids window {} -> close {} B = {} carrier(s)",
                &id[..16],
                a.profile.n_ctx,
                kary.dissection_arity,
                kary.prompt_ids_form,
                kary.window_court_daa,
                b.close_bytes,
                chunks
            );
            // **One carrier is the claim that matters** — it is what the announcement states, and
            // it is true across every close figure this row has had today. A class whose close
            // needs more than one carrier cannot be prosecuted on a shipped build.
            assert_eq!(
                chunks,
                1,
                "the genesis registers class {}… whose close is {} B = {chunks} carriers at arity {}, {:?} ids, \
                 window {}. More than one carrier means the row cannot be prosecuted.",
                &id[..16],
                b.close_bytes,
                kary.dissection_arity,
                kary.prompt_ids_form,
                kary.window_court_daa
            );
            // And the byte figure, pinned so a silent move is visible — with the court beside it,
            // because this number has been three different correct values in one afternoon.
            assert_eq!(
                b.close_bytes, 83_175,
                "the registered row's close moved: {} B at arity {}, {:?} ids, window {}. Check WHICH of those \
                 three moved before re-pinning — the last time this number changed it was the ids form, chosen \
                 by nobody.",
                b.close_bytes, kary.dissection_arity, kary.prompt_ids_form, kary.window_court_daa
            );
            priced += 1;
        }
    }
    // Without this the whole test passes when the genesis registers nothing with a carriage —
    // which is precisely the state this file spent the day discovering it was in.
    assert!(priced >= 1, "no genesis class carried an admission carriage, so nothing was priced and this test proved nothing");
}
