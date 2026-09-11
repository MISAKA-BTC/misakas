//! **`family-certification` and `lane-certification`** (ADR-0108 Decision 5; ADR-0075): the object
//! `palw-certify drill|bind --out` wrote, graded here by the same court the transition grades it
//! with. The family id is what the grader returns (ADR-0075 Decision 2) — never read off the
//! evidence as a fact — and a lane binding's class id is its profile's hash.

use kaspa_consensus_core::palw_state_v2::{
    PALW_OBJECT_CHUNK_MAX_BYTES, PalwCertificationEvidenceV1, PalwCertifiedLaneV1, PalwConsensusObjectV2,
};
use misaka_palw_base0::e2e_drill::covering_rc_family_v1;

use crate::manifest::{PalwExtensionError, PalwExtensionKindV1, parse_hash64};
use crate::report::{PalwExtensionClassificationV1, PalwExtensionDepthV1};
use crate::verify::{KindOutcomeV1, PALW_EXTENSION_MAX_FILE_BYTES, VerifyCx};

fn lane_of(word: &str) -> Option<PalwCertifiedLaneV1> {
    match word.trim().to_ascii_lowercase().as_str() {
        "attempt" => Some(PalwCertifiedLaneV1::Attempt),
        "fp" | "free-prompt" | "freeprompt" => Some(PalwCertifiedLaneV1::FreePrompt),
        _ => None,
    }
}

fn object_name(object: &PalwConsensusObjectV2) -> &'static str {
    match object {
        PalwConsensusObjectV2::FamilyCertified { .. } => "FamilyCertified",
        PalwConsensusObjectV2::ClassLaneCertified { .. } => "ClassLaneCertified",
        PalwConsensusObjectV2::ObjectChunk { .. } => {
            "ObjectChunk (a part — name the whole object palw-certify wrote beside the chunks)"
        }
        PalwConsensusObjectV2::ClassRegistered { .. } => "ClassRegistered",
        _ => "another lifecycle object",
    }
}

pub(crate) fn verify(cx: &mut VerifyCx<'_>) -> Result<KindOutcomeV1, PalwExtensionError> {
    use PalwExtensionDepthV1::{Full, Structural, Vectors};
    let manifest = cx.manifest();
    let kind = manifest.kind;
    let expected_object = kind.admission_object();

    let Some(object_path) = manifest.verification.object_path.clone() else {
        return Ok(KindOutcomeV1::at(
            cx.refuse(
                "verification.object_path",
                format!("a {kind} manifest names the borsh `{expected_object}` file `palw-certify` wrote"),
            ),
            Structural,
        ));
    };
    let Some((_, bytes)) = cx.read_named_file("verification.object_path", &object_path, PALW_EXTENSION_MAX_FILE_BYTES)? else {
        cx.fail("verification.object_path", format!("{object_path}: not readable on this machine"));
        return Ok(KindOutcomeV1::stopped(
            PalwExtensionClassificationV1::node_extension(format!(
                "the certification object verification.object_path ({object_path}) — not readable on this machine, so nothing here can be graded"
            )),
            Structural,
            "verification.object_path not readable",
        ));
    };
    cx.record("object_bytes", bytes.len());
    let object: PalwConsensusObjectV2 = match borsh::from_slice(&bytes) {
        Ok(object) => object,
        Err(e) => {
            return Ok(KindOutcomeV1::at(
                cx.refuse("verification.object_path", format!("not a borsh consensus object: {e}")),
                Structural,
            ));
        }
    };
    if bytes.len() > PALW_OBJECT_CHUNK_MAX_BYTES {
        cx.skip("carriage", format!("{} bytes is above one carrier's {PALW_OBJECT_CHUNK_MAX_BYTES}: submit the `.chunkN` files palw-certify wrote (ADR-0075 Decision 14)", bytes.len()));
    } else {
        cx.pass("carriage");
    }
    let declared = parse_hash64("declares.object_id", &manifest.declares.object_id)?;
    let wanted_lane = match &manifest.verification.lane {
        Some(word) => match lane_of(word) {
            Some(lane) => Some(lane),
            None => {
                return Ok(KindOutcomeV1::at(
                    cx.refuse("verification.lane", format!("`{word}` is not `attempt` or `fp`")),
                    Structural,
                ));
            }
        },
        None => None,
    };
    let terms = cx.terms()?;

    match (kind, &object) {
        (PalwExtensionKindV1::FamilyCertification, PalwConsensusObjectV2::FamilyCertified { evidence }) => {
            let lane = evidence.lane();
            cx.record("lane", lane);
            cx.record("vectors", evidence.vector_count());
            if let Some(wanted) = wanted_lane
                && wanted != lane
            {
                return Ok(KindOutcomeV1::at(
                    cx.refuse("verification.lane", format!("the object is a {lane}-lane certification")),
                    Structural,
                ));
            }
            // Structural: what the evidence NAMES. The grader is what makes it a family (Vectors).
            let named = match evidence.as_ref() {
                PalwCertificationEvidenceV1::Attempt(drill) => drill.family_id,
                PalwCertificationEvidenceV1::FreePrompt(drill) => drill.evidence.family_id,
            };
            cx.record("family_id_named", named);
            if named != declared {
                return Ok(KindOutcomeV1::at(
                    cx.refuse("declares.object_id", format!("the evidence names family {named}, not the declared {declared}")),
                    Structural,
                ));
            }
            cx.pass("declares.object_id");
            if let Some(expected) = manifest.verification.expected.get("family_id") {
                let expected = parse_hash64("verification.expected.family_id", expected)?;
                if expected != named {
                    return Ok(KindOutcomeV1::at(
                        cx.refuse("verification.expected.family_id", format!("the evidence names {named}")),
                        Structural,
                    ));
                }
                cx.pass("verification.expected.family_id");
            }
            let expressible = |would_be_refused: Option<String>| PalwExtensionClassificationV1::Expressible {
                admission_object: expected_object.to_string(),
                would_be_refused,
                serving: None,
                weightless: false,
                already_registered: false,
            };
            if cx.depth == Structural {
                return Ok(KindOutcomeV1::at(expressible(None), Structural));
            }
            // Vectors: the court grades the evidence, and only what it returns is the family.
            let family = match evidence.grade() {
                Ok(family) => family,
                Err(e) => {
                    return Ok(KindOutcomeV1::at(
                        cx.refuse("verification.object_path", format!("this build's court refuses the evidence: {e}")),
                        Structural,
                    ));
                }
            };
            cx.pass("grader");
            cx.record("family_id", family.family_id);
            cx.record("family_digest", family.digest());
            cx.record("drilled_class_id", family.drilled_class_id);
            cx.record("kernel_ids", family.kernel_ids.len());
            if family.family_id != declared {
                return Ok(KindOutcomeV1::at(
                    cx.refuse(
                        "declares.object_id",
                        format!("the grader returns family {}, not the declared {declared}", family.family_id),
                    ),
                    Vectors,
                ));
            }
            let digest = family.digest();
            let already = terms.chain_certified_families.iter().any(|f| f.digest() == digest);
            let would_be_refused =
                already.then(|| "this family digest is already chain-certified (per the terms the caller read)".to_string());
            let depth = if cx.depth == Vectors { Vectors } else { Full };
            Ok(KindOutcomeV1::at(expressible(would_be_refused), depth))
        }
        (PalwExtensionKindV1::LaneCertification, PalwConsensusObjectV2::ClassLaneCertified { class_id, lane, profile }) => {
            let lane = *lane;
            let class_id = *class_id;
            cx.record("lane", lane);
            if let Some(wanted) = wanted_lane
                && wanted != lane
            {
                return Ok(KindOutcomeV1::at(cx.refuse("verification.lane", format!("the object binds the {lane} lane")), Structural));
            }
            let derived = profile.shape_profile_id();
            cx.record("class_id", derived);
            cx.record("n_ctx", profile.n_ctx);
            if derived != class_id {
                return Ok(KindOutcomeV1::at(
                    cx.refuse(
                        "verification.object_path",
                        format!("the object names class {class_id} but its profile hashes to {derived}; the chain would refuse it"),
                    ),
                    Structural,
                ));
            }
            if derived != declared {
                return Ok(KindOutcomeV1::at(
                    cx.refuse("declares.object_id", format!("the profile hashes to {derived}, not to the declared {declared}")),
                    Structural,
                ));
            }
            cx.pass("declares.object_id");
            if let Some(expected) = manifest.verification.expected.get("class_id") {
                let expected = parse_hash64("verification.expected.class_id", expected)?;
                if expected != derived {
                    return Ok(KindOutcomeV1::at(
                        cx.refuse("verification.expected.class_id", format!("recomputed {derived}")),
                        Structural,
                    ));
                }
                cx.pass("verification.expected.class_id");
            }
            let in_genesis = cx
                .bundle()?
                .genesis_objects
                .iter()
                .any(|o| matches!(o, PalwConsensusObjectV2::ClassRegistered { class_id: id, .. } if *id == class_id));
            let registered = in_genesis || terms.registered_class_ids.contains(&class_id);
            let would_be_refused = (!registered)
                .then(|| format!("the chain must already carry {class_id} as an Active class — this object binds a lane of a class, it cannot create one"));
            let expressible = |would_be_refused: Option<String>| PalwExtensionClassificationV1::Expressible {
                admission_object: expected_object.to_string(),
                would_be_refused,
                serving: None,
                weightless: false,
                already_registered: false,
            };
            if cx.depth == Structural {
                return Ok(KindOutcomeV1::at(expressible(would_be_refused), Structural));
            }
            let reachable = kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(profile);
            cx.record("reachable_kernels", reachable.len());
            match covering_rc_family_v1(profile, lane) {
                Some(family) => {
                    cx.record("covering_family", family.name());
                    cx.pass("covering_family");
                }
                None => {
                    cx.fail(
                        "covering_family",
                        format!("no family this build can drill covers every kernel the class reaches on the {lane} lane"),
                    );
                    return Ok(KindOutcomeV1::at(
                        PalwExtensionClassificationV1::node_extension(format!(
                            "a certified family covering the class's {} kernels on the {lane} lane — the chain would refuse this with NoCertifiedFamilyCovers, and a court that serves the graph is a build (ADR-0069 Decision 2)",
                            reachable.len()
                        )),
                        Structural,
                    ));
                }
            }
            let depth = if cx.depth == Vectors { Vectors } else { Full };
            Ok(KindOutcomeV1::at(expressible(would_be_refused), depth))
        }
        (_, other) => Ok(KindOutcomeV1::at(
            cx.refuse(
                "verification.object_path",
                format!("is a {}, and a {kind} manifest names a {expected_object}", object_name(other)),
            ),
            Structural,
        )),
    }
}
