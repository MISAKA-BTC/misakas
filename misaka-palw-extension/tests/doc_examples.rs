//! **The example manifests under `docs/extension-manifests/` verify as the guide says they do.**
//! A document nobody runs drifts; these are run. Each example is copied into a temporary
//! directory beside whatever file it names, so the copy in `docs/` stays exactly what a reader
//! downloads.

use std::path::{Path, PathBuf};

use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_state_v2::{PalwCertificationEvidenceV1, PalwCertifiedLaneV1, PalwConsensusObjectV2};
use misaka_palw_base0::e2e_drill::{PalwRcFamilyV1, rc_attempt_evidence_v1};
use misaka_palw_derive::ids::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1, transformer_id};
use misaka_palw_derive::registry::{grammar_by_name, transformer_by_name};
use misaka_palw_extension::{PalwExtensionClassificationV1 as C, PalwExtensionDepthV1 as D, PalwExtensionEnvV1, verify_extension_v1};

fn docs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("the workspace root").join("docs").join("extension-manifests")
}

fn env() -> PalwExtensionEnvV1 {
    PalwExtensionEnvV1::genesis(NetworkId::with_suffix(NetworkType::Testnet, 11))
}

/// The values the examples must carry, printed when an example is missing or stale so the person
/// fixing the document has them without a second run.
fn values() -> String {
    let params: kaspa_consensus_core::config::params::Params = NetworkId::with_suffix(NetworkType::Testnet, 11).into();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!() };
    let floor = misaka_palw_base0::classes::canonical_class_by_model_id_v1(&bundle.court, "PALW-BASE-0/rc").unwrap();
    let floor_root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().unwrap();
    let projected = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v1(24).unwrap();
    let music = transformer_by_name("music/smf/v1").unwrap();
    let grammar = grammar_by_name(music.manifest().grammar).unwrap();
    let answer = std::fs::read(docs_dir().join("lead.json")).unwrap_or_default();
    let canonical = grammar.canonicalize(&answer).unwrap_or_default();
    let dsl_hash = dsl_hash_v1(&grammar_id_v1(grammar.name()), &canonical);
    let artifact = music.run(&canonical).map(|a| (artifact_hash_v1(&a.bytes).to_string(), a.bytes.len())).unwrap_or_default();
    format!(
        "floor class_id {}\nfloor root {}\nA16 row projected at n_ctx 24 class_id {}\nbase0 family id {}\nmusic/smf/v1 id {}\nlead.json dsl_hash {dsl_hash} artifact_hash {} bytes {}\nruleset {}",
        floor.class_id(),
        floor_root,
        projected.shape_profile_id(),
        PalwRcFamilyV1::Base0.family_id(),
        transformer_id(&music.manifest()),
        artifact.0,
        artifact.1,
        params.consensus_params_id()
    )
}

fn example(name: &str) -> Vec<u8> {
    std::fs::read(docs_dir().join(name)).unwrap_or_else(|e| panic!("docs/extension-manifests/{name}: {e}\n{}", values()))
}

fn stage(names: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for name in names {
        std::fs::write(dir.path().join(name), example(name)).unwrap();
    }
    dir
}

#[test]
fn the_model_class_example_is_the_floor_already_registered_at_full_depth() {
    let dir = stage(&["model-class.json"]);
    let report =
        verify_extension_v1(&example("model-class.json"), dir.path(), &env(), D::Full).unwrap_or_else(|e| panic!("{e}\n{}", values()));
    assert!(
        matches!(&report.classification, C::Expressible { already_registered: true, .. }),
        "{}\n{}",
        report.classification.summary(),
        values()
    );
    assert_eq!(report.depth_reached, D::Full);
}

#[test]
fn the_context_profile_example_is_a_new_width_expressible_and_served_only_with_the_arm() {
    let dir = stage(&["context-profile.json"]);
    let report = verify_extension_v1(&example("context-profile.json"), dir.path(), &env(), D::Full)
        .unwrap_or_else(|e| panic!("{e}\n{}", values()));
    match &report.classification {
        C::Expressible { already_registered, serving, would_be_refused, .. } => {
            assert!(!already_registered, "{}", values());
            assert_eq!(*would_be_refused, None);
            assert!(serving.as_ref().unwrap().needs_chain_classes_arm);
        }
        other => panic!("{}\n{}", other.summary(), values()),
    }
    assert_eq!(report.depth_reached, D::Vectors, "no artifact file beside the example");
    assert_eq!(report.stopped_at.as_deref(), Some("artifact.path not readable"));
    // The width is new; the WEIGHTS are the ones the graph-v5@512 row already registered, so the
    // example carries that root and the report warns rather than refuses (the chain admits a second
    // class over registered weights — only the SDK's candidate rule never builds one).
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.name == "registration.new_weights"
                && matches!(c.outcome, misaka_palw_extension::PalwExtensionOutcomeV1::Fail(_))),
        "{:?}",
        report.checks
    );
}

#[test]
fn the_family_certification_example_verifies_once_the_reader_has_run_the_drill() {
    let dir = stage(&["family-certification.json"]);
    // The guide's recipe: `palw-certify drill --family base0 --lane attempt --out base0-attempt.borsh`.
    let evidence = rc_attempt_evidence_v1(PalwRcFamilyV1::Base0).unwrap();
    let object = PalwConsensusObjectV2::FamilyCertified { evidence: Box::new(PalwCertificationEvidenceV1::Attempt(evidence)) };
    std::fs::write(dir.path().join("base0-attempt.borsh"), borsh::to_vec(&object).unwrap()).unwrap();
    let report = verify_extension_v1(&example("family-certification.json"), dir.path(), &env(), D::Vectors)
        .unwrap_or_else(|e| panic!("{e}\n{}", values()));
    assert!(
        matches!(&report.classification, C::Expressible { would_be_refused: None, .. }),
        "{}\n{}",
        report.classification.summary(),
        values()
    );
    // Without the file, the report says unverifiable here — and nothing else.
    let bare = stage(&["family-certification.json"]);
    let report = verify_extension_v1(&example("family-certification.json"), bare.path(), &env(), D::Vectors).unwrap();
    assert!(matches!(report.classification, C::NodeExtension { .. }));
}

/// The graph-v5@512 row is registered at genesis and is not in the genesis-frozen free-prompt
/// set, so binding its free-prompt lane is a real next step on testnet-11 — and against the genesis
/// state it is refused until a family the CHAIN certified for that lane covers it. The report
/// names that refusal and the command that removes it.
#[test]
fn the_lane_certification_example_names_the_family_the_chain_must_certify_first() {
    let dir = stage(&["lane-certification.json"]);
    let court =
        kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
            .unwrap();
    // What `palw-certify bind --model-id Qwen/Qwen2.5-1.5B/graph-v5@512 --lane fp` writes.
    let profile =
        misaka_palw_base0::e2e_drill::catalog_profile_by_model_id_v1(&court, misaka_palw_base0::classes::A16_GRAPH_V5_MODEL_ID)
            .expect("the graph-v5@512 row is in this build's catalogs");
    let object = PalwConsensusObjectV2::ClassLaneCertified {
        class_id: profile.shape_profile_id(),
        lane: PalwCertifiedLaneV1::FreePrompt,
        profile: Box::new(profile),
    };
    std::fs::write(dir.path().join("v5-512-fp-lane.borsh"), borsh::to_vec(&object).unwrap()).unwrap();
    let report = verify_extension_v1(&example("lane-certification.json"), dir.path(), &env(), D::Vectors)
        .unwrap_or_else(|e| panic!("{e}\n{}", values()));
    match &report.classification {
        C::Expressible { admission_object, would_be_refused: Some(why), .. } => {
            assert_eq!(admission_object, "ClassLaneCertified");
            assert!(why.starts_with("NoCertifiedFamilyCovers"), "{why}");
            assert!(why.contains("palw-certify drill --family a16-v5 --lane fp"), "the refusal names the step that removes it: {why}");
        }
        other => panic!("{}\n{}", other.summary(), values()),
    }
    assert_eq!(report.depth_reached, D::Vectors);
    assert_eq!(report.recomputed.get("covering_family").map(String::as_str), Some("PALW-QWEN25-A16-V5"));
}

#[test]
fn the_derived_transformer_example_reruns_its_vector() {
    let dir = stage(&["derived-transformer.json", "lead.json"]);
    let report = verify_extension_v1(&example("derived-transformer.json"), dir.path(), &env(), D::Vectors)
        .unwrap_or_else(|e| panic!("{e}\n{}", values()));
    assert!(matches!(&report.classification, C::Expressible { .. }), "{}\n{}", report.classification.summary(), values());
    assert_eq!(report.recomputed.get("vectors_run").map(String::as_str), Some("1"), "{}", values());
}

#[test]
fn the_ruleset_candidate_example_is_a_flag_day_and_never_expressible() {
    let dir = stage(&["ruleset-candidate.json"]);
    let report = verify_extension_v1(&example("ruleset-candidate.json"), dir.path(), &env(), D::Full)
        .unwrap_or_else(|e| panic!("{e}\n{}", values()));
    assert!(
        matches!(&report.classification, C::RulesetChange { flag_day: true, .. }),
        "{}\n{}",
        report.classification.summary(),
        values()
    );
}
