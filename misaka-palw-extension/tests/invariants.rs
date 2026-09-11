//! **ADR-0108 §7, one test per invariant.** Every fixture is written into a temporary directory
//! beside a manifest that names it, so SA-2's rule (a path resolves inside the manifest's
//! directory) is exercised by every test that reads a file, and every hex value is computed by the
//! build under test — nothing here is a literal a re-genesis would have to re-pin.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_e2e_adjudicability::PalwE2eDrillEvidenceV1;
use kaspa_consensus_core::palw_state_v2::{PalwCertificationEvidenceV1, PalwCertifiedLaneV1, PalwConsensusObjectV2};
use kaspa_hashes::Hash64;
use misaka_palw_base0::e2e_drill::{PalwRcFamilyV1, rc_attempt_evidence_v1};
use misaka_palw_derive::ids::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1, transformer_id};
use misaka_palw_derive::registry::{PRIOR_SOURCE_TREES_SHA256_HEX, transformer_by_name};
use misaka_palw_extension::{
    PalwExtensionClassificationV1 as C, PalwExtensionDepthV1 as D, PalwExtensionEnvV1, PalwExtensionError, PalwExtensionReceiptV1,
    PalwExtensionReportV1, PalwParsedManifestV1, extension_id_v1, sign_receipt_v1, verify_extension_v1, verify_receipt_v1,
};

// ---------------------------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------------------------

fn t11() -> NetworkId {
    NetworkId::with_suffix(NetworkType::Testnet, 11)
}

fn env() -> PalwExtensionEnvV1 {
    PalwExtensionEnvV1::genesis(t11())
}

fn params() -> Params {
    t11().into()
}

/// The floor's class id and registered root on testnet-11 — read from the network's genesis, so
/// the test cannot agree with a stale copy of either.
fn floor() -> (Hash64, Hash64) {
    let params = params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("testnet-11 ships a ConsensusV2 bundle");
    };
    let court = bundle.court;
    let entry = misaka_palw_base0::classes::canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor is a row");
    let class_id = entry.class_id();
    let root = bundle
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id: id, artifact_root, .. } if *id == class_id => Some(*artifact_root),
            _ => None,
        })
        .expect("the floor is in testnet-11's genesis");
    (class_id, root)
}

fn hex64(h: &Hash64) -> String {
    h.to_string()
}

fn zeros(n: usize) -> String {
    "0".repeat(n)
}

/// The floor family's attempt-lane drill, once per process: the same evidence `palw-certify drill
/// --family base0 --lane attempt` writes.
fn base0_evidence() -> &'static PalwE2eDrillEvidenceV1 {
    static EVIDENCE: OnceLock<PalwE2eDrillEvidenceV1> = OnceLock::new();
    EVIDENCE.get_or_init(|| rc_attempt_evidence_v1(PalwRcFamilyV1::Base0).expect("the floor drills on every build"))
}

fn write_object(dir: &Path, name: &str, object: &PalwConsensusObjectV2) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, borsh::to_vec(object).expect("borsh")).expect("write");
    path
}

fn model_class_manifest(class_id: &str, root: &str, extra: &str) -> String {
    format!(
        r#"{{
  "manifest": "misaka-palw/extension-manifest/v1",
  "kind": "model-class",
  "name": "the floor, by its ledger row",
  "network": "testnet-11",
  "source": {{ "note": "PALW-BASE-0/rc" }},
  "artifact": {{ "root": "{root}" }},
  "declares": {{ "object_id": "{class_id}", "capabilities": ["attempt", "fp"] }},
  "verification": {{ "model_id": "PALW-BASE-0/rc"{extra} }},
  "admission": {{ "object": "ClassRegistered" }}
}}"#
    )
}

fn family_manifest(family_id: &str, object: &str) -> String {
    format!(
        r#"{{
  "manifest": "misaka-palw/extension-manifest/v1",
  "kind": "family-certification",
  "name": "the floor's attempt-lane drill",
  "network": "testnet-11",
  "declares": {{ "object_id": "{family_id}" }},
  "verification": {{ "object_path": "{object}", "lane": "attempt" }},
  "admission": {{ "object": "FamilyCertified" }}
}}"#
    )
}

fn lane_manifest(class_id: &str, object: &str) -> String {
    format!(
        r#"{{
  "manifest": "misaka-palw/extension-manifest/v1",
  "kind": "lane-certification",
  "name": "the floor's attempt lane",
  "network": "testnet-11",
  "declares": {{ "object_id": "{class_id}" }},
  "verification": {{ "object_path": "{object}", "lane": "attempt" }},
  "admission": {{ "object": "ClassLaneCertified" }}
}}"#
    )
}

/// The corpus's smallest `music/v1` answer (the same bytes `misaka-palw-derive`'s own tests derive).
const MUSIC_ANSWER: &[u8] = br#"{"v":1,"ppq":480,"tempo_us_per_quarter":500000,"time_signature":[4,4],
        "tracks":[{"name":"lead","channel":0,"program":0,
                   "notes":[{"pitch":60,"velocity":100,"onset":0,"duration":480}]}]}"#;

/// What this build's `music/smf/v1` makes of the answer: `(dsl_hash, artifact_hash, bytes)`.
fn music_vector() -> (Hash64, Hash64, u64) {
    let transformer = transformer_by_name("music/smf/v1").expect("shipped");
    let grammar = misaka_palw_derive::registry::grammar_by_name(transformer.manifest().grammar).expect("shipped");
    let canonical = grammar.canonicalize(MUSIC_ANSWER).expect("the corpus answer canonicalizes");
    let dsl_hash = dsl_hash_v1(&grammar_id_v1(grammar.name()), &canonical);
    let artifact = transformer.run(&canonical).expect("the corpus answer derives");
    (dsl_hash, artifact_hash_v1(&artifact.bytes), artifact.bytes.len() as u64)
}

fn transformer_manifest(name: &str, id: &str, vectors: &str, transformer_extra: &str) -> String {
    format!(
        r#"{{
  "manifest": "misaka-palw/extension-manifest/v1",
  "kind": "derived-transformer",
  "name": "the shipped MIDI writer",
  "network": "testnet-11",
  "declares": {{ "object_id": "{id}" }},
  "transformer": {{ "name": "{name}"{transformer_extra} }},
  "verification": {{ "vectors": [{vectors}] }},
  "admission": {{ "object": "none" }}
}}"#
    )
}

fn ruleset_manifest(fences: &str) -> String {
    format!(
        r#"{{
  "manifest": "misaka-palw/extension-manifest/v1",
  "kind": "ruleset-candidate",
  "name": "a candidate",
  "network": "testnet-11",
  "requires": {{ "fences": {{ {fences} }} }},
  "declares": {{ "object_id": "{}" }},
  "admission": {{ "object": "none" }}
}}"#,
        zeros(128)
    )
}

fn verify(manifest: &str, dir: &Path, depth: D) -> PalwExtensionReportV1 {
    verify_extension_v1(manifest.as_bytes(), dir, &env(), depth).unwrap_or_else(|e| panic!("boundary refusal: {e}"))
}

fn refused_field(report: &PalwExtensionReportV1) -> &str {
    match &report.classification {
        C::Refused { field, .. } => field,
        other => panic!("expected Refused, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// I-1 — canonical form and the id
// ---------------------------------------------------------------------------------------------

#[test]
fn i1_the_same_manifest_in_any_spelling_has_one_id_and_a_changed_field_another() {
    let (class_id, root) = floor();
    let a = model_class_manifest(&hex64(&class_id), &hex64(&root), "");
    // The same document: keys reordered, whitespace changed, written by another hand.
    let b = format!(
        r#"{{"admission":{{"object":"ClassRegistered"}},"verification":{{"model_id":"PALW-BASE-0/rc"}},
           "declares":{{"capabilities":["attempt","fp"],"object_id":"{}"}},"artifact":{{"root":"{}"}},
           "source":{{"note":"PALW-BASE-0/rc"}},"network":"testnet-11","name":"the floor, by its ledger row",
           "kind":"model-class","manifest":"misaka-palw/extension-manifest/v1"}}"#,
        hex64(&class_id),
        hex64(&root)
    );
    let pa = PalwParsedManifestV1::parse(a.as_bytes()).unwrap();
    let pb = PalwParsedManifestV1::parse(b.as_bytes()).unwrap();
    assert_eq!(pa.canonical_bytes, pb.canonical_bytes, "two spellings of one document canonicalize to one byte string");
    assert_eq!(pa.extension_id, pb.extension_id);
    assert_eq!(pa.extension_id, extension_id_v1(&pa.canonical_bytes));
    // The id is the keyed digest over `len ‖ bytes`, as the ADR states it.
    let mut state = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/extension-manifest/v1").to_state();
    state.update(&(pa.canonical_bytes.len() as u64).to_le_bytes());
    state.update(&pa.canonical_bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    assert_eq!(
        pa.extension_id,
        Hash64::from_bytes(out),
        "the id is BLAKE2b-512 keyed by the manifest domain over the canonical bytes"
    );

    // A changed field is a different id.
    let c = a.replace("the floor, by its ledger row", "the floor, renamed");
    assert_ne!(PalwParsedManifestV1::parse(c.as_bytes()).unwrap().extension_id, pa.extension_id);

    // A duplicate key is refused before any parser could choose.
    let d = a.replacen("\"name\":", "\"name\": \"first\", \"name\":", 1);
    match PalwParsedManifestV1::parse(d.as_bytes()) {
        Err(PalwExtensionError::Field { field, reason }) => {
            assert_eq!(field, "manifest");
            assert!(reason.to_ascii_lowercase().contains("duplicate"), "{reason}");
        }
        other => panic!("a duplicate key must be refused: {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// I-2 — declares.object_id is recomputed, for every kind
// ---------------------------------------------------------------------------------------------

#[test]
fn i2_a_declared_object_id_the_kind_does_not_recompute_is_refused_by_that_field_for_every_kind() {
    let dir = tempfile::tempdir().unwrap();
    let (class_id, root) = floor();
    let wrong = hex64(&Hash64::from_u64_word(0xBAD));

    // model-class
    let report = verify(&model_class_manifest(&wrong, &hex64(&root), ""), dir.path(), D::Structural);
    assert_eq!(refused_field(&report), "declares.object_id");

    // context-profile (a projection at a width)
    let projected = format!(
        r#"{{"manifest":"misaka-palw/extension-manifest/v1","kind":"context-profile","name":"w","network":"testnet-11",
           "artifact":{{"root":"{}"}},"declares":{{"object_id":"{wrong}"}},
           "verification":{{"project":"a16-context-row","n_ctx":16}},"admission":{{"object":"ClassRegistered"}}}}"#,
        zeros(128)
    );
    let report = verify(&projected, dir.path(), D::Structural);
    assert_eq!(refused_field(&report), "declares.object_id");

    // family-certification
    let evidence = base0_evidence().clone();
    let object = PalwConsensusObjectV2::FamilyCertified { evidence: Box::new(PalwCertificationEvidenceV1::Attempt(evidence)) };
    write_object(dir.path(), "family.borsh", &object);
    let report = verify(&family_manifest(&wrong, "family.borsh"), dir.path(), D::Structural);
    assert_eq!(refused_field(&report), "declares.object_id");

    // lane-certification
    let profile = base0_evidence().profile.clone();
    let lane = PalwConsensusObjectV2::ClassLaneCertified { class_id, lane: PalwCertifiedLaneV1::Attempt, profile: Box::new(profile) };
    write_object(dir.path(), "lane.borsh", &lane);
    let report = verify(&lane_manifest(&wrong, "lane.borsh"), dir.path(), D::Structural);
    assert_eq!(refused_field(&report), "declares.object_id");

    // derived-transformer
    let report = verify(&transformer_manifest("music/smf/v1", &wrong, "", ""), dir.path(), D::Structural);
    assert_eq!(refused_field(&report), "declares.object_id");

    // ruleset-candidate has no object id to recompute (nothing rides); its declared id is inert
    // and its tier is always a ruleset change — asserted so a later reading of I-2 does not
    // expect a refusal the kind cannot give.
    let report = verify(&ruleset_manifest(r#""palw_heartbeat_transparent": 9000000"#), dir.path(), D::Structural);
    assert!(matches!(report.classification, C::RulesetChange { .. }));
}

// ---------------------------------------------------------------------------------------------
// I-3 — SA-1's bounds, at the boundary, naming the field
// ---------------------------------------------------------------------------------------------

#[test]
fn i3_every_bound_is_enforced_at_the_boundary_naming_the_field() {
    let (class_id, root) = floor();
    let ok = model_class_manifest(&hex64(&class_id), &hex64(&root), "");
    assert!(PalwParsedManifestV1::parse(ok.as_bytes()).is_ok());
    let field_of = |doc: String| -> String {
        match PalwParsedManifestV1::parse(doc.as_bytes()) {
            Err(PalwExtensionError::Field { field, .. }) => field,
            Ok(_) => panic!("must be refused at the boundary"),
            Err(other) => panic!("{other}"),
        }
    };

    // Over 1 MiB canonical: no report.
    let big = ok.replace("\"note\": \"PALW-BASE-0/rc\"", &format!("\"note\": \"{}\"", "x".repeat((1 << 20) + 1)));
    assert_eq!(field_of(big), "manifest");
    // Hex fields exactly 64 or 128.
    assert_eq!(field_of(ok.replace(&hex64(&root), &hex64(&root)[..127])), "artifact.root");
    assert_eq!(field_of(ok.replace(&hex64(&class_id), &format!("{}zz", &hex64(&class_id)[..126]))), "declares.object_id");
    assert_eq!(field_of(ok.replace("\"note\": \"PALW-BASE-0/rc\"", "\"digest\": \"abc\"")), "source.digest");
    // Paths: relative and free of `..`.
    assert_eq!(
        field_of(ok.replace(
            &format!("\"root\": \"{}\"", hex64(&root)),
            &format!("\"root\": \"{}\", \"path\": \"../weights.palwart\"", hex64(&root))
        )),
        "artifact.path"
    );
    assert_eq!(
        field_of(ok.replace(
            &format!("\"root\": \"{}\"", hex64(&root)),
            &format!("\"root\": \"{}\", \"path\": \"/etc/passwd\"", hex64(&root))
        )),
        "artifact.path"
    );
    // Too many vectors. A vector is two 128-hex hashes, so 4,097 of them are over 1 MiB before
    // they are over 4,096 — the byte bound refuses the document first, by design ("refused before
    // it is parsed further"); the count bound behind it is asserted on the shape directly.
    let vector = format!(
        r#"{{"dsl_path":"v.json","expected_dsl_hash":"{}","expected_artifact_hash":"{}","expected_artifact_bytes":1}}"#,
        zeros(128),
        zeros(128)
    );
    let vectors = std::iter::repeat_n(vector.clone(), 4_097).collect::<Vec<_>>().join(",");
    assert_eq!(field_of(transformer_manifest("music/smf/v1", &zeros(128), &vectors, "")), "manifest");
    let mut shape =
        PalwParsedManifestV1::parse(transformer_manifest("music/smf/v1", &zeros(128), &vector, "").as_bytes()).unwrap().manifest;
    shape.verification.vectors = std::iter::repeat_n(shape.verification.vectors[0].clone(), 4_097).collect();
    match shape.check_bounds() {
        Err(PalwExtensionError::Field { field, .. }) => assert_eq!(field, "verification.vectors"),
        other => panic!("{other:?}"),
    }
    // Too many fences.
    let fences = (0..65).map(|i| format!("\"palw_fence_{i}\": 1")).collect::<Vec<_>>().join(",");
    assert_eq!(field_of(ruleset_manifest(&fences)), "requires.fences");
    // SA-6: a key that looks like key material or a signature is refused as unknown, wherever it is.
    assert_eq!(field_of(ok.replace("\"note\": \"PALW-BASE-0/rc\"", "\"note\": \"x\", \"signature\": \"00\"")), "source.signature");
    assert_eq!(field_of(ok.replace("\"admission\": {", "\"seed_hex\": \"00\", \"admission\": {")), "seed_hex");
    // Any other unknown field is refused too.
    assert_eq!(field_of(ok.replace("\"admission\": {", "\"activate\": true, \"admission\": {")), "manifest");
    // The reserved kind.
    let reserved = ok.replace("\"kind\": \"model-class\"", "\"kind\": \"service-descriptor\"");
    match PalwParsedManifestV1::parse(reserved.as_bytes()) {
        Err(PalwExtensionError::Field { field, reason }) => {
            assert_eq!(field, "kind");
            assert!(reason.contains("reserved"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    // A float is not a canonical number (RFC 8785 as this tree spells it).
    assert_eq!(field_of(ok.replace("\"admission\": {", "\"admission\": {\"weight\": 1.5, ")), "manifest");
}

// ---------------------------------------------------------------------------------------------
// I-4 — model-class
// ---------------------------------------------------------------------------------------------

#[test]
fn i4_the_floor_is_expressible_and_already_registered_and_served_from_the_table() {
    let dir = tempfile::tempdir().unwrap();
    let (class_id, root) = floor();
    let report = verify(&model_class_manifest(&hex64(&class_id), &hex64(&root), ""), dir.path(), D::Full);
    match &report.classification {
        C::Expressible { admission_object, would_be_refused, serving, weightless, already_registered } => {
            assert_eq!(admission_object, "ClassRegistered");
            assert!(*already_registered, "the floor is in testnet-11's genesis");
            assert!(would_be_refused.as_deref().is_some_and(|w| w.contains("already registered")), "{would_be_refused:?}");
            assert!(!weightless, "the floor's family covers the floor");
            let serving = serving.as_ref().expect("a class reports serving");
            assert!(serving.in_build_table && !serving.needs_chain_classes_arm);
        }
        other => panic!("{other:?}"),
    }
    // Full: the derived root was recomputed from nothing on disk and matched.
    assert_eq!(report.depth_reached, D::Full);
    assert_eq!(report.stopped_at, None);
    assert_eq!(report.recomputed.get("artifact_root").map(String::as_str), Some(hex64(&root).as_str()));
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.name == "artifact.root" && matches!(c.outcome, misaka_palw_extension::PalwExtensionOutcomeV1::Pass))
    );
    assert_eq!(report.ruleset_id_this_build, params().consensus_params_id().to_string());
    assert!(report.chain_terms.starts_with("genesis only"));

    // The same class under another root: different weights, refused by the root's field.
    let report = verify(&model_class_manifest(&hex64(&class_id), &hex64(&Hash64::from_u64_word(0x77)), ""), dir.path(), D::Structural);
    assert_eq!(refused_field(&report), "artifact.root");
}

#[test]
fn i4_a_kernel_outside_the_vocabulary_is_a_ruleset_change_naming_the_kernel() {
    let dir = tempfile::tempdir().unwrap();
    let mut profile = base0_evidence().profile.clone();
    let stranger = Hash64::from_u64_word(0x5741_4E47_4552);
    profile.pre_nodes[0].kernel_semantics_id = stranger;
    let class_id = profile.shape_profile_id();
    let doc = format!(
        r#"{{"manifest":"misaka-palw/extension-manifest/v1","kind":"model-class","name":"a new kernel","network":"testnet-11",
           "artifact":{{"root":"{}"}},"declares":{{"object_id":"{}"}},
           "verification":{{"profile_borsh_hex":"{}","canonical_job":[8,4]}},"admission":{{"object":"ClassRegistered"}}}}"#,
        hex64(&Hash64::from_u64_word(0x1)),
        hex64(&class_id),
        faster_hex::hex_string(&borsh::to_vec(&profile).unwrap())
    );
    let report = verify(&doc, dir.path(), D::Vectors);
    match &report.classification {
        C::RulesetChange { reason, flag_day, .. } => {
            assert!(reason.contains(&hex64(&stranger)), "the kernel is named: {reason}");
            assert!(reason.contains("release"), "{reason}");
            assert!(*flag_day);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn i4_an_artifact_container_no_lineage_sniffs_is_a_node_extension_naming_the_lineage() {
    let dir = tempfile::tempdir().unwrap();
    // A ledger row that needs a file, pointed at bytes whose magic nothing in this build claims.
    let params = params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!() };
    let a16 = misaka_palw_base0::classes::canonical_class_by_model_id_v1(&bundle.court, "Qwen/Qwen2.5-1.5B").expect("the A16 row");
    let a16_root = bundle
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id: id, artifact_root, .. } if *id == a16.class_id() => {
                Some(*artifact_root)
            }
            _ => None,
        })
        .unwrap_or(Hash64::from_u64_word(0xA16));
    std::fs::write(dir.path().join("weights.bin"), b"STRANGE1 and then some bytes that are not a container").unwrap();
    let doc = format!(
        r#"{{"manifest":"misaka-palw/extension-manifest/v1","kind":"model-class","name":"a16","network":"testnet-11",
           "artifact":{{"root":"{}","path":"weights.bin"}},"declares":{{"object_id":"{}"}},
           "verification":{{"model_id":"Qwen/Qwen2.5-1.5B"}},"admission":{{"object":"ClassRegistered"}}}}"#,
        hex64(&a16_root),
        hex64(&a16.class_id())
    );
    let report = verify(&doc, dir.path(), D::Full);
    match &report.classification {
        C::NodeExtension { missing } => assert!(missing[0].starts_with("lineage for container `STRANGE1`"), "{missing:?}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(report.depth_reached, D::Vectors, "the gate answered; only the bytes could not be judged");

    // And a machine that holds no artifact at all stops at Vectors, says so, and is not refused.
    let doc = doc.replace(",\"path\":\"weights.bin\"", "");
    let report = verify(&doc, dir.path(), D::Full);
    assert!(matches!(report.classification, C::Expressible { .. }), "{:?}", report.classification);
    assert_eq!(report.depth_reached, D::Vectors);
    assert_eq!(report.stopped_at.as_deref(), Some("artifact.path not given"));
}

#[test]
fn i4_a_class_no_certified_family_covers_is_expressible_and_weightless() {
    // A hybrid graph: the floor with one node's kernel swapped for a catalogued kernel of another
    // family that can still serve the node — every kernel is in the vocabulary, no single family's
    // drill covers the set. Searched rather than written down, so the test says which swap it found
    // and fails loudly if this build has none.
    use kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1;
    use kaspa_consensus_core::palw_e2e_adjudicability::{family_certified_for_weight_v2, palw_rc_certified_families_v1};
    let dir = tempfile::tempdir().unwrap();
    let params = params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!() };
    let base = base0_evidence().profile.clone();
    let catalogued = kaspa_consensus_core::palw_step_refute::catalogued_kernel_ids_v1();
    let families = palw_rc_certified_families_v1();
    let base_set = reachable_kernels_v1(&base);
    let mut found = None;
    'search: for table in 0..4 {
        let count = match table {
            0 => base.pre_nodes.len(),
            1 => base.gdn_nodes.len(),
            2 => base.attn_nodes.len(),
            _ => base.post_nodes.len(),
        };
        for slot in 0..count {
            for kernel in catalogued.iter().filter(|k| !base_set.contains(k)) {
                let mut candidate = base.clone();
                let node = match table {
                    0 => &mut candidate.pre_nodes[slot],
                    1 => &mut candidate.gdn_nodes[slot],
                    2 => &mut candidate.attn_nodes[slot],
                    _ => &mut candidate.post_nodes[slot],
                };
                node.kernel_semantics_id = *kernel;
                if candidate.validate_shape().is_err() {
                    continue;
                }
                if kaspa_consensus_core::palw_catalog_coverage::verify_profile_coverage_v1(&candidate).is_err() {
                    continue;
                }
                let reachable = reachable_kernels_v1(&candidate);
                if family_certified_for_weight_v2(bundle.court_e2e_root, &families, &[], &reachable).expect("priced").is_none() {
                    found = Some((candidate, table, slot, *kernel));
                    break 'search;
                }
            }
        }
    }
    let Some((profile, table, slot, kernel)) = found else {
        panic!(
            "this build has no catalogued kernel that can stand in for a floor node without a family covering the result — I-4's weightless case needs another fixture"
        );
    };
    eprintln!("weightless hybrid: table {table} slot {slot} kernel {kernel}");
    let class_id = profile.shape_profile_id();
    let doc = format!(
        r#"{{"manifest":"misaka-palw/extension-manifest/v1","kind":"model-class","name":"a hybrid","network":"testnet-11",
           "artifact":{{"root":"{}"}},"declares":{{"object_id":"{}"}},
           "verification":{{"profile_borsh_hex":"{}","canonical_job":[8,4]}},"admission":{{"object":"ClassRegistered"}}}}"#,
        hex64(&Hash64::from_u64_word(0x2)),
        hex64(&class_id),
        faster_hex::hex_string(&borsh::to_vec(&profile).unwrap())
    );
    let report = verify(&doc, dir.path(), D::Vectors);
    match &report.classification {
        C::Expressible { weightless, serving, already_registered, .. } => {
            assert!(*weightless, "no family covers the hybrid, so it registers weightless (ADR-0069 Decision 5)");
            assert!(!*already_registered);
            let serving = serving.as_ref().unwrap();
            assert!(
                !serving.in_build_table && serving.needs_chain_classes_arm,
                "a class outside the table needs the arm (Decision 8)"
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(report.depth_reached, D::Vectors, "the gate admitted it at share 0: {:?}", report.checks);
}

// ---------------------------------------------------------------------------------------------
// I-5 — family-certification
// ---------------------------------------------------------------------------------------------

#[test]
fn i5_the_floors_drill_verifies_to_its_pinned_family_id_and_a_flipped_vector_is_refused_in_the_graders_words() {
    let dir = tempfile::tempdir().unwrap();
    let evidence = base0_evidence().clone();
    let family_id = evidence.family_id;
    assert_eq!(family_id, PalwRcFamilyV1::Base0.family_id(), "the drill names the pinned floor family");
    let object = PalwConsensusObjectV2::FamilyCertified { evidence: Box::new(PalwCertificationEvidenceV1::Attempt(evidence.clone())) };
    write_object(dir.path(), "base0-attempt.borsh", &object);
    let report = verify(&family_manifest(&hex64(&family_id), "base0-attempt.borsh"), dir.path(), D::Vectors);
    match &report.classification {
        C::Expressible { admission_object, would_be_refused, .. } => {
            assert_eq!(admission_object, "FamilyCertified");
            assert_eq!(*would_be_refused, None);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(report.depth_reached, D::Vectors);
    assert_eq!(report.recomputed.get("family_id").map(String::as_str), Some(hex64(&family_id).as_str()));
    let graded = evidence.clone();
    let expected_digest = kaspa_consensus_core::palw_e2e_adjudicability::certify_e2e_family_v1(&graded).unwrap().family_digest;
    assert_eq!(report.recomputed.get("family_digest").map(String::as_str), Some(hex64(&expected_digest).as_str()));

    // One flipped vector: the grader's own error, in its own words.
    let mut tampered = evidence;
    tampered.vectors[0].leaf_index = tampered.vectors[0].leaf_index.wrapping_add(1);
    let grader_says = PalwCertificationEvidenceV1::Attempt(tampered.clone()).grade().expect_err("a flipped vector does not grade");
    let object = PalwConsensusObjectV2::FamilyCertified { evidence: Box::new(PalwCertificationEvidenceV1::Attempt(tampered)) };
    write_object(dir.path(), "tampered.borsh", &object);
    let report = verify(&family_manifest(&hex64(&family_id), "tampered.borsh"), dir.path(), D::Vectors);
    match &report.classification {
        C::Refused { field, reason } => {
            assert_eq!(field, "verification.object_path");
            assert!(reason.contains(&grader_says.to_string()), "the refusal carries the grader's words: {reason} / {grader_says}");
        }
        other => panic!("{other:?}"),
    }
    // Structural alone never claims what only the grader can say: it reaches Structural, not Vectors.
    let report = verify(&family_manifest(&hex64(&family_id), "tampered.borsh"), dir.path(), D::Structural);
    assert!(matches!(report.classification, C::Expressible { .. }));
    assert_eq!(report.depth_reached, D::Structural);
}

/// **The transition's other two `FamilyCertified` rules, reported in its order.** Too many vectors
/// is the manifest's object being wrong for every chain (refused by the field); a family the chain
/// already certified is a fact about THIS chain, which only live terms can show.
#[test]
fn i5_a_family_over_the_vector_bound_is_refused_and_an_already_certified_one_would_be_refused() {
    let dir = tempfile::tempdir().unwrap();
    let evidence = base0_evidence().clone();
    let family_id = evidence.family_id;
    let max = kaspa_consensus_core::palw_state_v2::PALW_CERTIFICATION_MAX_VECTORS;
    // One vector over the bound, each stripped of its operand openings so the object stays inside
    // the 1.6 MB a chunked object can ride in — the carriage rule is checked first (an object no
    // carrier can take never reaches the transition), and this test is about the one behind it.
    let mut small = evidence.vectors[0].clone();
    small.operand_openings.clear();
    let mut stuffed = evidence.clone();
    stuffed.vectors = std::iter::repeat_n(small, max + 1).collect();
    let object = PalwConsensusObjectV2::FamilyCertified { evidence: Box::new(PalwCertificationEvidenceV1::Attempt(stuffed)) };
    write_object(dir.path(), "stuffed.borsh", &object);
    let report = verify(&family_manifest(&hex64(&family_id), "stuffed.borsh"), dir.path(), D::Structural);
    match &report.classification {
        C::Refused { field, reason } => {
            assert_eq!(field, "verification.object_path");
            assert!(reason.contains("TooManyDrillVectors"), "{reason}");
        }
        other => panic!("{other:?}"),
    }

    // Live terms that already hold the family: expressible, and the chain would refuse a second.
    let object = PalwConsensusObjectV2::FamilyCertified { evidence: Box::new(PalwCertificationEvidenceV1::Attempt(evidence.clone())) };
    write_object(dir.path(), "family.borsh", &object);
    let graded = PalwCertificationEvidenceV1::Attempt(evidence).grade().unwrap();
    let params = params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!() };
    let mut terms = misaka_palw_extension::genesis_registration_terms_v1(bundle).expect("genesis terms");
    terms.chain_certified_families = vec![graded];
    let live = PalwExtensionEnvV1 { network_id: t11(), daa_score: None, chain_terms: Some(terms) };
    let report =
        verify_extension_v1(family_manifest(&hex64(&family_id), "family.borsh").as_bytes(), dir.path(), &live, D::Vectors).unwrap();
    match &report.classification {
        C::Expressible { would_be_refused: Some(why), .. } => assert!(why.starts_with("FamilyAlreadyCertified"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(report.chain_terms.starts_with("the chain's terms as the caller read them"), "{}", report.chain_terms);
}

/// **`ClassLaneCertified`, in the transition's order**: the class must be on chain, a family the
/// chain certified for the lane must cover it, and the attempt lane seats only a weightless class.
#[test]
fn i5_a_lane_binding_reports_the_chains_refusals_in_the_transitions_order() {
    let dir = tempfile::tempdir().unwrap();
    let (class_id, _) = floor();
    let profile = base0_evidence().profile.clone();
    assert_eq!(profile.shape_profile_id(), class_id, "the floor's drill runs the floor's own graph");
    let lane =
        PalwConsensusObjectV2::ClassLaneCertified { class_id, lane: PalwCertifiedLaneV1::Attempt, profile: Box::new(profile.clone()) };
    write_object(dir.path(), "lane.borsh", &lane);

    // At the genesis state: no chain-certified family covers it, and the floor already holds a share.
    let report = verify(&lane_manifest(&hex64(&class_id), "lane.borsh"), dir.path(), D::Vectors);
    match &report.classification {
        C::Expressible { would_be_refused: Some(why), .. } => {
            assert!(why.starts_with("NoCertifiedFamilyCovers"), "the transition checks coverage first: {why}");
            assert!(why.contains("palw-certify drill --family base0 --lane attempt"), "{why}");
            assert!(why.contains("and after that, ClassAlreadyWeighted"), "{why}");
        }
        other => panic!("{other:?}"),
    }

    // Live terms whose chain-certified set covers it: only the lane's own rule is left.
    let graded = PalwCertificationEvidenceV1::Attempt(base0_evidence().clone()).grade().unwrap();
    let params = params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!() };
    let mut terms = misaka_palw_extension::genesis_registration_terms_v1(bundle).expect("genesis terms");
    terms.chain_certified_families = vec![graded];
    let live = PalwExtensionEnvV1 { network_id: t11(), daa_score: None, chain_terms: Some(terms) };
    let report =
        verify_extension_v1(lane_manifest(&hex64(&class_id), "lane.borsh").as_bytes(), dir.path(), &live, D::Vectors).unwrap();
    match &report.classification {
        C::Expressible { would_be_refused: Some(why), .. } => assert!(why.starts_with("ClassAlreadyWeighted"), "{why}"),
        other => panic!("{other:?}"),
    }

    // A class nobody registered: MissingClass comes first.
    let mut stranger = profile;
    stranger.n_ctx += 1;
    let stranger_id = stranger.shape_profile_id();
    let lane = PalwConsensusObjectV2::ClassLaneCertified {
        class_id: stranger_id,
        lane: PalwCertifiedLaneV1::Attempt,
        profile: Box::new(stranger),
    };
    write_object(dir.path(), "stranger.borsh", &lane);
    let report = verify(&lane_manifest(&hex64(&stranger_id), "stranger.borsh"), dir.path(), D::Structural);
    match &report.classification {
        C::Expressible { would_be_refused: Some(why), .. } => assert!(why.starts_with("MissingClass"), "{why}"),
        C::Refused { field, reason } => panic!("the widened floor must stay a valid graph for this test: {field}: {reason}"),
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// I-6 — derived-transformer
// ---------------------------------------------------------------------------------------------

#[test]
fn i6_a_shipped_transformer_verifies_at_vectors_a_prior_trees_id_resolves_a_stranger_is_a_node_extension_and_a_widened_ceiling_is_another_id()
 {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("lead.json"), MUSIC_ANSWER).unwrap();
    let (dsl_hash, artifact_hash, bytes) = music_vector();
    let vector = format!(
        r#"{{"dsl_path":"lead.json","expected_dsl_hash":"{}","expected_artifact_hash":"{}","expected_artifact_bytes":{bytes}}}"#,
        hex64(&dsl_hash),
        hex64(&artifact_hash)
    );
    let current = transformer_by_name("music/smf/v1").unwrap().manifest();
    let current_id = transformer_id(&current);

    // This build's id, by name: Vectors re-run and matched.
    let report = verify(&transformer_manifest("music/smf/v1", &hex64(&current_id), &vector, ""), dir.path(), D::Vectors);
    assert!(
        matches!(&report.classification, C::Expressible { admission_object, .. } if admission_object == "none"),
        "{:?}",
        report.classification
    );
    assert_eq!(report.depth_reached, D::Vectors);
    assert_eq!(report.recomputed.get("resolved_tree").map(String::as_str), Some("current"));
    assert_eq!(report.recomputed.get("vectors_run").map(String::as_str), Some("1"));

    // The previous published tree's id resolves through PRIOR_SOURCE_TREES_SHA256_HEX.
    let mut prior = current.clone();
    prior.source_tree_sha256 = PRIOR_SOURCE_TREES_SHA256_HEX[0];
    let prior_id = transformer_id(&prior);
    let report = verify(&transformer_manifest("music/smf/v1", &hex64(&prior_id), &vector, ""), dir.path(), D::Vectors);
    assert!(matches!(report.classification, C::Expressible { .. }), "{:?}", report.classification);
    assert!(report.recomputed.get("resolved_tree").is_some_and(|t| t.starts_with("prior ")), "{:?}", report.recomputed);

    // A name this build lacks: unverifiable here, never valid.
    let report = verify(&transformer_manifest("music/smf/v9-not-shipped", &hex64(&current_id), &vector, ""), dir.path(), D::Vectors);
    match &report.classification {
        C::NodeExtension { missing } => assert!(missing[0].contains("music/smf/v9-not-shipped"), "{missing:?}"),
        other => panic!("{other:?}"),
    }

    // The full form with a widened ceiling: a different id (ADR-0078 SA-2), so the declared one is
    // refused by name — and the full form with the shipped ceilings hashes to the shipped id.
    let full = |max_dsl_bytes: u64| {
        format!(
            r#", "grammar": "{}", "discipline": "{}", "writer": "{}", "source_tree_sha256": "{}", "kind": {}, "max_dsl_bytes": {max_dsl_bytes}, "max_artifact_bytes": {}, "max_steps": {}"#,
            current.grammar,
            current.discipline.as_str(),
            current.writer,
            current.source_tree_sha256,
            current.kind,
            current.max_artifact_bytes,
            current.max_steps
        )
    };
    let report = verify(
        &transformer_manifest("music/smf/v1", &hex64(&current_id), &vector, &full(current.max_dsl_bytes)),
        dir.path(),
        D::Vectors,
    );
    assert!(matches!(report.classification, C::Expressible { .. }), "{:?}", report.classification);
    let report = verify(
        &transformer_manifest("music/smf/v1", &hex64(&current_id), &vector, &full(current.max_dsl_bytes + 1)),
        dir.path(),
        D::Vectors,
    );
    assert_eq!(refused_field(&report), "declares.object_id");
    // A zero ceiling is refused by its own field, before any id is compared.
    let report = verify(&transformer_manifest("music/smf/v1", &hex64(&current_id), &vector, &full(0)), dir.path(), D::Vectors);
    assert_eq!(refused_field(&report), "transformer.max_dsl_bytes");

    // A vector whose expected artifact the recomputation does not produce is refused by index and field.
    let wrong = vector.replace(&hex64(&artifact_hash), &hex64(&Hash64::from_u64_word(0x9)));
    let report = verify(&transformer_manifest("music/smf/v1", &hex64(&current_id), &wrong, ""), dir.path(), D::Vectors);
    assert_eq!(refused_field(&report), "verification.vectors[0].expected_artifact_hash");
    // A vector this machine does not hold is a depth stop, not a verdict.
    let report = verify(
        &transformer_manifest("music/smf/v1", &hex64(&current_id), &vector.replace("lead.json", "elsewhere.json"), ""),
        dir.path(),
        D::Vectors,
    );
    assert!(matches!(report.classification, C::Expressible { .. }));
    assert_eq!(report.depth_reached, D::Structural);
    assert_eq!(report.stopped_at.as_deref(), Some("verification.vectors[0].dsl_path not readable"));
}

// ---------------------------------------------------------------------------------------------
// I-7 — ruleset-candidate
// ---------------------------------------------------------------------------------------------

#[test]
fn i7_a_dormant_fence_at_a_height_moves_the_params_and_schedule_ids_and_at_genesis_the_identity() {
    let dir = tempfile::tempdir().unwrap();
    let this = params();
    assert!(this.palw_heartbeat_transparent.is_none(), "testnet-11 leaves palw_heartbeat_transparent dormant");
    let this_params = this.consensus_params_id().to_string();
    let this_identity = this.consensus_identity_id().to_string();
    let this_schedule = this.consensus_schedule_id().to_string();

    let report = verify(&ruleset_manifest(r#""palw_heartbeat_transparent": 9000000"#), dir.path(), D::Full);
    match &report.classification {
        C::RulesetChange { fences, would_print, flag_day, reason } => {
            assert_eq!(fences.len(), 1);
            assert_eq!(
                (fences[0].name.as_str(), fences[0].requested.as_str(), fences[0].this_build.as_str()),
                ("palw_heartbeat_transparent", "9000000", "absent")
            );
            let wp = would_print.as_ref().expect("a candidate prints");
            assert_ne!(wp.params_id, this_params, "the params id moves");
            assert_ne!(wp.schedule_id, this_schedule, "the schedule id moves");
            assert_eq!(wp.identity_id, this_identity, "the identity does not move for a future height");
            assert!(wp.fence_schedule.contains(&9_000_000));
            assert!(*flag_day);
            assert!(reason.contains("the fork-id gate"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    // What the arming build would print is what a `Params` with that fence set prints.
    let mut arming = this.clone();
    arming.palw_heartbeat_transparent = Some(ForkActivation::new(9_000_000));
    assert_eq!(report.recomputed.get("arming.params_id").map(String::as_str), Some(arming.consensus_params_id().to_string().as_str()));

    let report = verify(&ruleset_manifest(r#""palw_heartbeat_transparent": "genesis""#), dir.path(), D::Full);
    match &report.classification {
        C::RulesetChange { would_print, flag_day, reason, .. } => {
            assert_ne!(would_print.as_ref().unwrap().identity_id, this_identity, "arming at genesis moves the identity");
            assert!(*flag_day);
            assert!(reason.contains("identity moves"), "{reason}");
        }
        other => panic!("{other:?}"),
    }

    // A fence this build does not have: a build before a height.
    let report = verify(&ruleset_manifest(r#""palw_no_such_fence": 12000"#), dir.path(), D::Full);
    match &report.classification {
        C::RulesetChange { fences, reason, .. } => {
            assert_eq!(fences[0].this_build, "absent");
            assert!(
                reason.contains("this build has no fence `palw_no_such_fence`: the candidate needs a build before it needs a height"),
                "{reason}"
            );
        }
        other => panic!("{other:?}"),
    }

    // A combination validate_palw_v2 refuses (ADR-0089 Decision 9: the EVM face before the
    // market) is reported with its refusal.
    let report = verify(&ruleset_manifest(r#""palw_model_evm": 100, "palw_model_market": 200"#), dir.path(), D::Full);
    match &report.classification {
        C::RulesetChange { reason, .. } => {
            assert!(reason.contains("validate_palw_v2") && reason.contains("ModelEvmBeforeMarket"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
    // Never Expressible: a candidate that names this build's own values is still a ruleset change.
    let report = verify(&ruleset_manifest(r#""palw_kary_court": "genesis""#), dir.path(), D::Full);
    match &report.classification {
        C::RulesetChange { flag_day, would_print, .. } => {
            assert!(!flag_day, "nothing would print differently");
            assert_eq!(would_print.as_ref().unwrap().params_id, this_params);
        }
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// I-8 — the receipt
// ---------------------------------------------------------------------------------------------

#[test]
fn i8_a_receipt_round_trips_a_changed_byte_fails_another_ruleset_is_refused_and_unsigned_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let (class_id, root) = floor();
    let report = verify(&model_class_manifest(&hex64(&class_id), &hex64(&root), ""), dir.path(), D::Vectors);
    let ruleset = report.ruleset_id_this_build.clone();
    let key = kaspa_pq_validator_core::ValidatorKey::from_seed([7u8; 32]);

    // Unsigned: verifies as unsigned, and says so.
    let unsigned = PalwExtensionReceiptV1::issue(report.clone(), 1_757_000_000_000);
    let bytes = unsigned.canonical_json().unwrap();
    let verdict = verify_receipt_v1(&bytes, Some(&ruleset)).unwrap();
    assert!(!verdict.signed);
    assert!(verdict.note.starts_with("unsigned"), "{}", verdict.note);
    assert_eq!(verdict.receipt_id, unsigned.receipt_id().unwrap().to_string());
    assert_eq!(verdict.tier, "expressible");

    // Signed: sign, canonicalize, verify — and the id is over the bytes WITHOUT the signer fields.
    let mut signed = unsigned.clone();
    sign_receipt_v1(&mut signed, &key).unwrap();
    assert_eq!(signed.receipt_id().unwrap(), unsigned.receipt_id().unwrap(), "the signature is outside the id");
    let signed_bytes = signed.canonical_json().unwrap();
    let verdict = verify_receipt_v1(&signed_bytes, Some(&ruleset)).unwrap();
    assert!(verdict.signed);
    assert_eq!(verdict.signer_pubkey_hex.as_deref(), Some(faster_hex::hex_string(key.public_key()).as_str()));
    // Reordered by another serializer: the same receipt.
    let value: serde_json::Value = serde_json::from_slice(&signed_bytes).unwrap();
    let pretty = serde_json::to_vec_pretty(&value).unwrap();
    assert!(verify_receipt_v1(&pretty, Some(&ruleset)).unwrap().signed);

    // A report byte changed after signing: the signature fails, by name.
    let tampered =
        String::from_utf8(signed_bytes.clone()).unwrap().replace("\"depth_reached\":\"vectors\"", "\"depth_reached\":\"full\"");
    assert_ne!(tampered, String::from_utf8(signed_bytes.clone()).unwrap());
    match verify_receipt_v1(tampered.as_bytes(), Some(&ruleset)) {
        Err(PalwExtensionError::Field { field, .. }) => assert_eq!(field, "signature_hex"),
        other => panic!("{other:?}"),
    }

    // SA-3: made on another ruleset — refused naming verifier.ruleset_id, signature or not.
    match verify_receipt_v1(&signed_bytes, Some(&"f".repeat(64))) {
        Err(PalwExtensionError::Field { field, reason }) => {
            assert_eq!(field, "verifier.ruleset_id");
            assert!(reason.contains("SA-3"), "{reason}");
        }
        other => panic!("{other:?}"),
    }

    // SA-4: a signature under any other context does not verify as a receipt.
    let mut other_context = unsigned.clone();
    let message = other_context.unsigned_canonical_bytes().unwrap();
    let signature = key.sign_with_context(&message, kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT);
    other_context.signer_pubkey_hex = Some(faster_hex::hex_string(key.public_key()));
    other_context.signature_hex = Some(faster_hex::hex_string(&signature));
    match verify_receipt_v1(&other_context.canonical_json().unwrap(), Some(&ruleset)) {
        Err(PalwExtensionError::Field { field, .. }) => assert_eq!(field, "signature_hex"),
        other => panic!("{other:?}"),
    }
    // And the receipt context is not a member of the chain's context set (Decision 4).
    for set in [
        kaspa_consensus_core::palw_mode_v2::PALW_V2_SIGNATURE_CONTEXTS,
        kaspa_consensus_core::palw_mode_v2::PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V2,
    ] {
        assert!(
            !set.contains(&misaka_palw_extension::PALW_EXTENSION_RECEIPT_MLDSA87_CONTEXT),
            "the receipt context must stay out of the chain's context set"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// I-9 — no aggregation: nothing that decides anything can read a receipt
// ---------------------------------------------------------------------------------------------

#[test]
fn i9_the_crate_depends_on_nothing_that_decides_and_nothing_that_decides_depends_on_it() {
    // The cheap tripwire: this crate's own manifest never names the node or the consensus crate,
    // so the only edge INTO it can be the CLI's (the measured `cargo tree -i` is in ADR-0108 §9).
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    for forbidden in ["kaspad", "kaspa-consensus.workspace", "kaspa-consensus =", "kaspa-consensus\""] {
        assert!(!manifest.contains(forbidden), "misaka-palw-extension must not depend on `{forbidden}`");
    }
    // And the reverse direction, read from the workspace manifests of the crates that decide.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for decider in ["kaspad", "consensus", "consensus/core", "misaka-palw-sdk", "misaka-palw-derive", "misaka-palw-base0"] {
        let text = std::fs::read_to_string(root.join(decider).join("Cargo.toml")).unwrap();
        assert!(
            !text.contains("misaka-palw-extension"),
            "{decider}/Cargo.toml names the extension crate — a decider must not read a receipt"
        );
    }
    // SA-5, mechanically: no public type of this crate holds a set of receipts.
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/receipt.rs")).unwrap();
    assert!(!src.contains("Vec<PalwExtensionReceiptV1>"), "no set of receipts, anywhere");
}

// ---------------------------------------------------------------------------------------------
// I-10 — the verifier never opens a path outside the manifest's directory, and never a URL
// ---------------------------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn i10_a_path_that_escapes_the_manifests_directory_is_refused_by_name_and_so_is_a_url() {
    let outside = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let evidence = base0_evidence().clone();
    let family_id = evidence.family_id;
    let object = PalwConsensusObjectV2::FamilyCertified { evidence: Box::new(PalwCertificationEvidenceV1::Attempt(evidence)) };
    let real = write_object(outside.path(), "family.borsh", &object);
    // A symlink inside the directory pointing outside it: resolved, seen to escape, refused.
    std::os::unix::fs::symlink(&real, dir.path().join("link.borsh")).unwrap();
    match verify_extension_v1(family_manifest(&hex64(&family_id), "link.borsh").as_bytes(), dir.path(), &env(), D::Vectors) {
        Err(PalwExtensionError::Field { field, reason }) => {
            assert_eq!(field, "verification.object_path");
            assert!(reason.contains("outside the manifest's directory"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    // `..` and an absolute path are refused before anything is resolved.
    for path in ["../family.borsh", "/etc/hostname", "sub/../../x"] {
        match verify_extension_v1(family_manifest(&hex64(&family_id), path).as_bytes(), dir.path(), &env(), D::Vectors) {
            Err(PalwExtensionError::Field { field, .. }) => assert_eq!(field, "verification.object_path", "{path}"),
            other => panic!("{path}: {other:?}"),
        }
    }
    // A URL is not a path.
    match verify_extension_v1(
        family_manifest(&hex64(&family_id), "https://example.invalid/family.borsh").as_bytes(),
        dir.path(),
        &env(),
        D::Vectors,
    ) {
        Err(PalwExtensionError::Field { field, reason }) => {
            assert_eq!(field, "verification.object_path");
            assert!(reason.contains("URL"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    // The same object inside the directory verifies — the rule is about the directory, not the file.
    write_object(dir.path(), "family.borsh", &object);
    let report = verify(&family_manifest(&hex64(&family_id), "family.borsh"), dir.path(), D::Vectors);
    assert!(matches!(report.classification, C::Expressible { .. }));
    // And a file that is simply not there is a depth stop that says "unverifiable here".
    let report = verify(&family_manifest(&hex64(&family_id), "missing.borsh"), dir.path(), D::Vectors);
    assert!(matches!(report.classification, C::NodeExtension { .. }), "{:?}", report.classification);
    assert_eq!(report.stopped_at.as_deref(), Some("verification.object_path not readable"));
}
