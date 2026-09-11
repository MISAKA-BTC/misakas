//! **`family-certification` and `lane-certification`** (ADR-0108 Decision 5; ADR-0075): the object
//! `palw-certify drill|bind --out` wrote, graded here by the same court the transition grades it
//! with. The family id is what the grader returns (ADR-0075 Decision 2) — never read off the
//! evidence as a fact — and a lane binding's class id is its profile's hash.
//!
//! **What the chain would still refuse is reported in the transition's own order** (`palw_state_v2`,
//! the `FamilyCertified` and `ClassLaneCertified` arms): too many vectors, a grade that fails, a
//! family already certified; a class that is not there, a lane with no family the CHAIN certified
//! to cover it, a class the attempt lane cannot seat because it already holds a share. Judged at
//! the terms the verifier has — the genesis state unless a caller read live ones (ADR-0108 §8) —
//! and the report says which.

use kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1;
use kaspa_consensus_core::palw_state_v2::{
    PALW_CERTIFICATION_MAX_VECTORS, PALW_OBJECT_CHUNK_MAX_BYTES, PalwCertificationEvidenceV1, PalwCertifiedLaneV1,
    PalwConsensusObjectV2, palw_certification_min_fee_v1, palw_object_chunk_group_rent_v1, palw_object_chunks_v1,
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

/// `palw-certify`'s spelling of a lane.
fn lane_flag(lane: PalwCertifiedLaneV1) -> &'static str {
    match lane {
        PalwCertifiedLaneV1::Attempt => "attempt",
        PalwCertifiedLaneV1::FreePrompt => "fp",
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

fn expressible(admission_object: &str, would_be_refused: Option<String>) -> PalwExtensionClassificationV1 {
    PalwExtensionClassificationV1::Expressible {
        admission_object: admission_object.to_string(),
        would_be_refused,
        serving: None,
        weightless: false,
        already_registered: false,
    }
}

/// The chain refuses at the first rule that fails; the report says that one first and names the
/// ones behind it, so a person fixing the first is not surprised by the second.
fn first_then_rest(refusals: Vec<String>) -> Option<String> {
    let mut refusals = refusals.into_iter();
    let first = refusals.next()?;
    let rest: Vec<String> = refusals.collect();
    Some(if rest.is_empty() { first } else { format!("{first} — and after that, {}", rest.join(" — and after that, ")) })
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
    // ADR-0075 Decision 14: an object above one carrier's bytes rides as chunks; `submit` cuts it.
    match palw_object_chunks_v1(&object) {
        Ok(None) => cx.pass("carriage"),
        Ok(Some(chunks)) => {
            cx.record("carriage.chunks", chunks.len());
            cx.record("carriage.rent_per_chunk_sompi", palw_object_chunk_group_rent_v1());
            cx.pass("carriage");
            cx.skip(
                "carriage.single",
                format!(
                    "{} bytes is above one carrier's {PALW_OBJECT_CHUNK_MAX_BYTES}: it rides as {} ObjectChunk carriers (ADR-0075 Decision 14) — `submit` cuts them",
                    bytes.len(),
                    chunks.len()
                ),
            );
        }
        Err(e) => {
            return Ok(KindOutcomeV1::at(
                cx.refuse("verification.object_path", format!("the object cannot be carried: {e}")),
                Structural,
            ));
        }
    }
    let declared = parse_hash64("declares.object_id", &manifest.declares.object_id)?;
    let wanted_lane = match &manifest.verification.lane {
        Some(word) => match lane_of(word) {
            Some(lane) => Some(lane),
            None => {
                return Ok(KindOutcomeV1::at(cx.refuse("verification.lane", format!("`{word}` is not `attempt` or `fp`")), Structural));
            }
        },
        None => None,
    };
    let terms = cx.terms()?;
    let judged_at_genesis = cx.env.chain_terms.is_none();

    match (kind, &object) {
        (PalwExtensionKindV1::FamilyCertification, PalwConsensusObjectV2::FamilyCertified { evidence }) => {
            let lane = evidence.lane();
            let vectors = evidence.vector_count();
            cx.record("lane", lane);
            cx.record("vectors", vectors);
            if let Some(wanted) = wanted_lane
                && wanted != lane
            {
                return Ok(KindOutcomeV1::at(
                    cx.refuse("verification.lane", format!("the object is a {lane}-lane certification")),
                    Structural,
                ));
            }
            // The transition's first rule: the grading work one object may ask of every node.
            if vectors > PALW_CERTIFICATION_MAX_VECTORS {
                return Ok(KindOutcomeV1::at(
                    cx.refuse(
                        "verification.object_path",
                        format!("{vectors} fault vectors, over the {PALW_CERTIFICATION_MAX_VECTORS} one object may ask every node to grade (TooManyDrillVectors)"),
                    ),
                    Structural,
                ));
            }
            cx.pass("vectors.bound");
            cx.record("rent_sompi", palw_certification_min_fee_v1(vectors));
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
            if cx.depth == Structural {
                return Ok(KindOutcomeV1::at(expressible(expected_object, None), Structural));
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
            // The transition's last rule: one certification per (lane, digest).
            let digest = family.digest();
            let would_be_refused = if judged_at_genesis {
                cx.pass("chain.family_uncertified");
                None
            } else if lane == PalwCertifiedLaneV1::Attempt {
                if terms.chain_certified_families.iter().any(|f| f.digest() == digest) {
                    let why = format!("FamilyAlreadyCertified: the chain already certified digest {digest} for the attempt lane");
                    cx.fail("chain.family_uncertified", why.clone());
                    Some(why)
                } else {
                    cx.pass("chain.family_uncertified");
                    None
                }
            } else {
                cx.skip(
                    "chain.family_uncertified",
                    "the terms this verifier read carry the attempt lane's chain-certified families only",
                );
                None
            };
            let depth = if cx.depth == Vectors { Vectors } else { Full };
            Ok(KindOutcomeV1::at(expressible(expected_object, would_be_refused), depth))
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
            if let Err(e) = profile.validate_shape() {
                return Ok(KindOutcomeV1::at(
                    cx.refuse(
                        "verification.object_path",
                        format!("the profile is not a valid graph (CertificationProfileInvalid): {e}"),
                    ),
                    Structural,
                ));
            }
            let derived = profile.shape_profile_id();
            cx.record("class_id", derived);
            cx.record("n_ctx", profile.n_ctx);
            if derived != class_id {
                return Ok(KindOutcomeV1::at(
                    cx.refuse(
                        "verification.object_path",
                        format!(
                            "the object names class {class_id} but its profile hashes to {derived} (CertificationProfileIsNotTheClass)"
                        ),
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

            // The chain's rules, in the transition's order, at the terms this verifier has.
            let bundle = cx.bundle()?.clone();
            let reachable = reachable_kernels_v1(profile);
            cx.record("reachable_kernels", reachable.len());
            let drillable = covering_rc_family_v1(profile, lane);
            let genesis_share = bundle.genesis_objects.iter().find_map(|o| match o {
                PalwConsensusObjectV2::ClassRegistered { class_id: id, share_permille, .. } if *id == class_id => {
                    Some(*share_permille)
                }
                _ => None,
            });
            let mut refusals = Vec::new();
            // 1. MissingClass — the object binds a lane of a class; it cannot create one.
            if genesis_share.is_some() || terms.registered_class_ids.contains(&class_id) {
                cx.pass("chain.class_registered");
            } else {
                let why = format!(
                    "MissingClass: the chain must already carry {class_id} as an Active class — this object binds a lane of a class, it cannot create one (register it first: a model-class manifest)"
                );
                cx.fail("chain.class_registered", why.clone());
                refusals.push(why);
            }
            // 2. NoCertifiedFamilyCovers — a family the CHAIN certified for this lane (a
            //    FamilyCertified object) must cover every kernel; the compile-time set does not count.
            let chain_families = match (&cx.env.chain_terms, lane) {
                (None, _) => Some(Vec::new()),
                (Some(t), PalwCertifiedLaneV1::Attempt) => Some(t.chain_certified_families.clone()),
                (Some(_), PalwCertifiedLaneV1::FreePrompt) => None,
            };
            match chain_families {
                Some(families) => match families.iter().find(|f| reachable.is_subset(&f.kernel_ids)) {
                    Some(family) => {
                        cx.record("chain_covering_family", family.family_id);
                        cx.pass("chain.covering_family");
                    }
                    None => {
                        let first_step = match drillable {
                            Some(family) => format!(
                                "file `palw-certify drill --family {} --lane {}` as a family-certification first",
                                family.short_name_v1(),
                                lane_flag(lane)
                            ),
                            None => "no family this build can drill covers it either".to_string(),
                        };
                        let why = format!(
                            "NoCertifiedFamilyCovers: no family the chain certified for the {lane} lane covers the class's {} kernels{} — {first_step}",
                            reachable.len(),
                            if judged_at_genesis { " (judged at the genesis state, which certifies none)" } else { "" }
                        );
                        cx.fail("chain.covering_family", why.clone());
                        refusals.push(why);
                    }
                },
                None => cx.skip(
                    "chain.covering_family",
                    "the terms this verifier read carry the attempt lane's chain-certified families only",
                ),
            }
            // 3. The lane's own rule.
            match lane {
                PalwCertifiedLaneV1::Attempt => match genesis_share {
                    Some(share) if share > 0 => {
                        let why = format!(
                            "ClassAlreadyWeighted: the class holds a share from genesis ({share}‰ at registration) — the attempt lane seats a class that registered weightless (ADR-0069 Decision 6)"
                        );
                        cx.fail("chain.class_weightless", why.clone());
                        refusals.push(why);
                    }
                    Some(_) => cx.pass("chain.class_weightless"),
                    None => cx.skip("chain.class_weightless", "the class's current share is not in the terms this verifier can read"),
                },
                PalwCertifiedLaneV1::FreePrompt => {
                    if bundle.state.fp_certified_classes().is_some_and(|set| set.contains(&class_id)) {
                        cx.skip(
                            "chain.fp_lane_uncertified",
                            "the class is in the genesis-frozen free-prompt set already; a chain certification adds a record the gate does not need",
                        );
                    } else if judged_at_genesis {
                        cx.pass("chain.fp_lane_uncertified");
                    } else {
                        cx.skip(
                            "chain.fp_lane_uncertified",
                            "the chain's own free-prompt certifications are not in the terms this verifier can read",
                        );
                    }
                }
            }
            let would_be_refused = first_then_rest(refusals);
            if cx.depth == Structural {
                return Ok(KindOutcomeV1::at(expressible(expected_object, would_be_refused), Structural));
            }
            // Vectors: a family THIS BUILD can drill covers the kernels — without one, the class is
            // a graph this build's court does not serve, and only a build that does can say more.
            match drillable {
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
                            "a certified family covering the class's {} kernels on the {lane} lane — a court that serves the graph is a build (ADR-0069 Decision 2)",
                            reachable.len()
                        )),
                        Structural,
                    ));
                }
            }
            let depth = if cx.depth == Vectors { Vectors } else { Full };
            Ok(KindOutcomeV1::at(expressible(expected_object, would_be_refused), depth))
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
