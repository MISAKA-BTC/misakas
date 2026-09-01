//! **The certification drill** (ADR-0069 Decision 3) — one harness, any family.
//!
//! `backend.rs`'s tests already ran this shape for BASE-0: an honest execution, a planted fault,
//! both sides opened through the SAME prover, the bisection agreeing before the fault and differing
//! at it, and the shipped adjudicator asked which way it reads. What they could not do is make the
//! result mean anything to the chain — a passing test is evidence to a person reading CI, and the
//! launch audit's finding was that the chain was paying 97.8% of its cadence to families for which
//! no such test existed at all.
//!
//! So the shape moves out of the tests and becomes a function over
//! [`PalwExecutionBackendV1`]. Nothing here is family-specific: it drives the three court verbs the
//! trait declares, and a family that has not implemented them fails at the first one rather than
//! producing a certificate for a court it cannot play. That is the whole design — the drill does
//! not know or care which model it is drilling, and it cannot be passed by declaring anything.
//!
//! # The covering set, chosen rather than assumed
//!
//! [`kaspa_consensus_core::palw_e2e_adjudicability::PalwE2eCoveringV1`] requires a fault in every
//! table the profile declares and in both call classes, and it is the CERTIFIER that scores that —
//! this module only picks candidates. The two use one classifier
//! ([`kaspa_consensus_core::palw_e2e_adjudicability::table_of_slot_v1`]) for the reason the rest of
//! this tree keeps recording: two descriptions of one computation are free to disagree, and here
//! they would disagree about how much of a step space a certificate vouches for.

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_e2e_adjudicability::{
    PalwE2eCertificateV1, PalwE2eDrillEvidenceV1, PalwE2eError, PalwE2eFaultVectorV1, certify_e2e_family_v1, table_of_slot_v1,
};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, PalwStepTableV1, canonical_step_coordinates};
use kaspa_hashes::Hash64;

/// Why a family could not be drilled. Separated from [`PalwE2eError`] — which is the CERTIFIER's
/// verdict on evidence — because these are failures to produce evidence at all, and the two have
/// different readers: a family that cannot open a refutation has not implemented a court, while a
/// family whose guilty run is acquitted has implemented one that does not work.
#[derive(Debug)]
pub enum PalwDrillError {
    /// The backend refused a verb the drill needs. Carries the backend's own message, because for
    /// the families that have not implemented the court this is the trait's default text and
    /// naming it is how an operator learns which half is missing.
    Backend { what: &'static str, why: String },
    /// No leaf could be found for some part of the step space, so no covering set exists to drill.
    /// A real answer about the profile rather than an error in the harness: a graph whose post
    /// table is never reached at a decode call has a step space with no such coordinate.
    NoCandidate { what: &'static str },
    /// The evidence was produced and the shipped court refused to certify it.
    Certify(PalwE2eError),
}

impl std::fmt::Display for PalwDrillError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Backend { what, why } => write!(f, "the backend cannot {what}: {why}"),
            Self::NoCandidate { what } => write!(f, "the profile offers no drillable leaf for {what}"),
            Self::Certify(e) => write!(f, "the court refused to certify the drill: {e}"),
        }
    }
}

/// One leaf the drill will plant a fault at, and why it was chosen.
struct Candidate {
    leaf: u64,
    table: PalwStepTableV1,
    decode: bool,
}

/// **Drill one family to a certificate.**
///
/// `anchor` decides the job, exactly as it does in production — the drill runs the canonical job
/// the chain would have asked for rather than one chosen to be easy. `artifact_root` is the class's
/// registered root, and the operand openings a close carries must prove against it: a drill that
/// proved its weight rows against some root of its own would certify a court reading weights the
/// chain never pinned.
///
/// The returned certificate is sealed — see [`certify_e2e_family_v1`] — so holding one IS the fact
/// that the shipped adjudicator convicted every planted fault and acquitted the honest run.
pub fn drill_family_v1(
    family_id: Hash64,
    backend: &dyn PalwExecutionBackendV1,
    profile: &PalwShapeProfileV3,
    artifact_root: Hash64,
    anchor: Hash64,
) -> Result<PalwE2eCertificateV1, PalwDrillError> {
    // **Refused up front, by the family's own declaration.** A backend whose court verbs are the
    // trait defaults would fail at `refutation_for_index` below with a less legible message; asking
    // first means a family that has not built a court is told that, rather than told that one of
    // its openings did not verify.
    if !backend.supports_court() {
        return Err(PalwDrillError::Backend {
            what: "take a court's turn",
            why: "the backend declares no court (supports_court() is false)".to_string(),
        });
    }

    let (job, prompt) = backend.job_for_anchor(anchor).map_err(|why| PalwDrillError::Backend { what: "derive a job", why })?;
    let honest = backend.execute(&job, &prompt).map_err(|why| PalwDrillError::Backend { what: "execute", why })?;

    // **Candidates: one leaf per (table, call class) the profile actually reaches.**
    //
    // Walked in the step space's own enumeration rather than computed, so the drill cannot disagree
    // with `canonical_step_coordinates` about which leaf is which — the same reason
    // `base0_anchored_ladder_v1` walks instead of doing arithmetic on the space.
    let leaf_count = kaspa_consensus_core::palw_step::step_leaf_count(profile, &job)
        .map_err(|e| PalwDrillError::Backend { what: "count its own step space", why: format!("{e:?}") })?;
    let mut candidates: Vec<Candidate> = Vec::new();
    for leaf in 0..leaf_count {
        let Some(coord) = canonical_step_coordinates(profile, &job, leaf) else { continue };
        let Some(table) = table_of_slot_v1(profile, coord.node_slot) else { continue };
        let decode = coord.call_index > 0;
        if candidates.iter().any(|c| c.table == table && c.decode == decode) {
            continue;
        }
        // A leaf is only drillable if the capture HOLDS it — `execute_with_injected_fault` corrupts
        // a tile the material carries, and a coordinate the capture never filled has none. Asked of
        // the backend rather than assumed, because which leaves a family retains is a family fact.
        if backend.refutation_for_index(&honest.material, leaf).is_err() {
            continue;
        }
        candidates.push(Candidate { leaf, table, decode });
    }
    if candidates.is_empty() {
        return Err(PalwDrillError::NoCandidate { what: "any table at any call class" });
    }

    let mut vectors = Vec::with_capacity(candidates.len());
    for candidate in &candidates {
        let leaf = candidate.leaf;
        // The fraud a court is FOR: a producer that ran a wrong execution and committed to it
        // honestly. Its material verifies against its own claim, so no seat check sees it.
        let guilty = backend
            .execute_with_injected_fault(&job, &prompt, leaf)
            .map_err(|why| PalwDrillError::Backend { what: "plant a drill fault", why })?;

        // **Both sides through ONE prover.** A prover only the challenger could run would be a
        // prover that decides the verdict, so the honest party's own close and the challenger's
        // come from the same call — which is also what makes the acquittal below meaningful.
        let honest_refutation = backend
            .refutation_for_index(&honest.material, leaf)
            .map_err(|why| PalwDrillError::Backend { what: "open an honest refutation", why })?;
        let guilty_refutation = backend
            .refutation_for_index(&guilty.material, leaf)
            .map_err(|why| PalwDrillError::Backend { what: "open a refutation for the planted fault", why })?;

        // The weight rows the close carries — asked of the adjudicator through the backend's
        // recording oracle, never enumerated here. A second enumeration on the prover side would be
        // a second opinion about which operands a step reads, and it would diverge in the direction
        // where an honest producer cannot close.
        let operand_openings = backend
            .operand_openings_for(&guilty_refutation)
            .map_err(|why| PalwDrillError::Backend { what: "open the operand rows its own close reads", why })?;

        let prefix = |material: &[u8], at: u64| -> Result<Hash64, PalwDrillError> {
            backend.bisect_prefix_state(material, at).ok_or(PalwDrillError::Backend {
                what: "answer at a rung",
                why: format!("bisect_prefix_state returned None at index {at}"),
            })
        };
        vectors.push(PalwE2eFaultVectorV1 {
            leaf_index: leaf,
            honest: honest_refutation,
            guilty: guilty_refutation,
            operand_openings,
            honest_prefix: (prefix(&honest.material, leaf)?, prefix(&honest.material, leaf + 1)?),
            guilty_prefix: (prefix(&guilty.material, leaf)?, prefix(&guilty.material, leaf + 1)?),
        });
    }

    // **The seat's verb, pointed at a stranger's bytes.**
    //
    // `verify_material` is reachable by anyone: material is gossiped, and no bond stands behind a
    // message. The launch audit found a family that answered a malformed one by fabricating an
    // empty logits row, tiling it into a zero-leaf tree, and hitting an `.expect` — every seat that
    // read the message died, and a claim with no seats never licenses and never reaches a court.
    //
    // The inputs are family-agnostic on purpose: truncations and extensions of the family's OWN
    // honest material, plus bytes that are not the format at all. A truncation is the interesting
    // case, because it is how a decoder produces a value that parses and does not cohere — the
    // shape the audit's finding took. Surviving is most of the assertion (a panic takes the
    // process, not the branch); the rest is that nothing here is ever `Matches`, because a seat
    // that certified a stranger's fragment would be worse than one that crashed on it.
    let malformed = malformed_variants_v1(&honest.material);
    let mut malformed_inputs_refused = 0u32;
    for bytes in &malformed {
        let claim = kaspa_consensus_core::palw_backend::PalwClaimRootsV1 {
            execution_root: honest.execution_root,
            trace_root: honest.trace_root,
            anchor,
        };
        if backend.verify_material(bytes, claim) == kaspa_consensus_core::palw_backend::PalwMaterialVerdictV1::Matches {
            return Err(PalwDrillError::Backend {
                what: "refuse a malformed material",
                why: format!("a {}-byte fragment of this family's own material verified as Matches", bytes.len()),
            });
        }
        malformed_inputs_refused += 1;
    }

    certify_e2e_family_v1(&PalwE2eDrillEvidenceV1 {
        family_id,
        profile: profile.clone(),
        artifact_root,
        vectors,
        malformed_inputs_refused,
    })
    .map_err(PalwDrillError::Certify)
}

/// Malformed inputs derived from one honest material, in the shapes a gossiped message actually
/// takes: nothing, not-the-format, a prefix that stops mid-structure, and a body with something
/// appended. Truncation is the one that matters — it is how a length-prefixed decoder yields a
/// value that parses and does not cohere.
fn malformed_variants_v1(honest: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![Vec::new(), b"not material at all".to_vec()];
    // A spread of prefixes rather than one: different cut points stop inside different fields, and
    // which field a decoder is lenient about is exactly what is being probed.
    for fraction in [1usize, 2, 3, 4, 8, 16, 32] {
        let cut = honest.len() / fraction;
        if cut < honest.len() {
            out.push(honest[..cut].to_vec());
        }
    }
    let mut extended = honest.to_vec();
    extended.push(0);
    out.push(extended);
    out
}

/// **The floor's own certificate, computed once per process.**
///
/// The drill is a real execution — it runs the model and the court several times — so it is done
/// once and cached, and every caller that needs "is BASE-0 certified on this build" gets the same
/// answer without paying for it again.
///
/// `Err` is kept rather than collapsed to `None`: a build whose floor stops certifying has a
/// specific thing wrong with it, and a node that logs "the floor is not certified" without saying
/// why is a node whose operator cannot act.
pub fn base0_certificate_v1() -> Result<&'static PalwE2eCertificateV1, &'static PalwDrillError> {
    static CERT: std::sync::OnceLock<Result<PalwE2eCertificateV1, PalwDrillError>> = std::sync::OnceLock::new();
    CERT.get_or_init(drill_base0_v1).as_ref()
}

/// The floor's family id — a build-level name, hashed under this module's own construction so it
/// cannot collide with a class id or an artifact root.
pub fn base0_family_id_v1() -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/e2e-family-id/v1").to_state();
    h.update(b"PALW-BASE-0");
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn drill_base0_v1() -> Result<PalwE2eCertificateV1, PalwDrillError> {
    use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;

    let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
        .map_err(|e| PalwDrillError::Backend { what: "read the shipped court params", why: format!("{e:?}") })?;
    let entry = crate::classes::canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc")
        .ok_or(PalwDrillError::NoCandidate { what: "the floor's own catalog entry" })?;
    let root = crate::rc::palw_rc_base0_artifact_root_v1()
        .map_err(|e| PalwDrillError::Backend { what: "derive the floor's artifact root", why: format!("{e:?}") })?;
    let resolved = crate::classes::resolve_class_v1(&court, entry.class_id(), root, &[])
        .map_err(|e| PalwDrillError::Backend { what: "resolve the floor", why: format!("{e:?}") })?;
    let profile = resolved.profile.clone();
    let backend = crate::backend::Base0Backend::new(resolved);
    // A fixed anchor, so the certificate is a function of the build rather than of when it ran —
    // `court_e2e_root` is a consensus identity and a drill that used a clock would move it.
    drill_family_v1(base0_family_id_v1(), &backend, &profile, root, Hash64::from_u64_word(0x0E2E_D8111))
}

/// **Register every family this build can certify, once.**
///
/// Called by the node before anything reads `court_e2e_root` or the admission gate — see
/// `kaspa_consensus_core::palw_e2e_adjudicability::register_certified_family_v1`. Idempotent and
/// order-free.
///
/// Returns what it registered, so a caller can log the set rather than the root alone: an operator
/// debugging "why does my class get no weight" needs the family list, and a 64-byte hash does not
/// answer that question.
pub fn register_builtin_certified_families_v1() -> Vec<&'static str> {
    let mut registered = Vec::new();
    if let Ok(certificate) = base0_certificate_v1() {
        kaspa_consensus_core::palw_e2e_adjudicability::register_certified_family_v1(certificate);
        registered.push("PALW-BASE-0");
    }
    registered
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The floor certifies, end to end, through the shipped court** (ADR-0069 invariant 5's
    /// premise).
    ///
    /// This is the test that decides whether ANY class can hold weight on this build: the admission
    /// gate grants a nonzero share only to a class whose kernels a certified family covers, and
    /// BASE-0 is the family the shipped genesis table depends on. A regression here does not fail
    /// quietly — it makes the network unable to fund its own liveness floor, which is why the
    /// assertion below is on the certificate's own contents rather than merely on `is_ok`.
    #[test]
    fn the_floor_certifies_end_to_end() {
        let certificate = match base0_certificate_v1() {
            Ok(c) => c,
            Err(e) => panic!("the floor must certify on every build that ships it: {e}"),
        };
        let covering = &certificate.family.covering;
        assert!(covering.pre, "a fault in the pre table was convicted");
        assert!(covering.attn, "and one in the layer table");
        assert!(covering.post, "and one in the post table, which is where the token is decided");
        assert!(covering.prefill && covering.decode, "and in both call classes");
        assert!(covering.convicted_leaves >= 4, "one leaf per declared table and call class, at least: {covering:?}");

        // The certificate vouches for the kernels the drilled graph actually walks — read off the
        // profile by the certifier, so a drill cannot widen its own promise.
        assert!(!certificate.family.kernel_ids.is_empty());
        assert!(
            certificate.family.kernel_ids.is_subset(&kaspa_consensus_core::palw_step_refute::catalogued_kernel_ids_v1()),
            "ADR-0069 invariant 3: end-to-end certified implies catalogued"
        );
    }

    /// **The registry is what the admission gate reads, and registering is what fills it.**
    ///
    /// Asserted as a DIFFERENCE around the registration, because "the floor's kernels are
    /// certified" is exactly the fact that decides whether the shipped genesis table can grant
    /// weight — and it must be false before any drill has run, or the gate would be certifying by
    /// default, which is the fail-open this ADR exists to close.
    #[test]
    fn registering_the_floor_is_what_lets_its_kernels_hold_weight() {
        use kaspa_consensus_core::palw_e2e_adjudicability::family_certified_for_kernels_v1;

        let certificate = base0_certificate_v1().expect("the floor certifies");
        let kernels = certificate.family.kernel_ids.clone();

        register_builtin_certified_families_v1();
        let found = family_certified_for_kernels_v1(&kernels).expect("the floor's own kernels are covered once it is registered");
        assert_eq!(found.family_id, base0_family_id_v1());

        // And a kernel no family drilled is still refused — the set is a real membership test, not
        // a switch that opens once anything registers.
        let mut alien = kernels.clone();
        alien.insert(Hash64::from_u64_word(0xA11E7));
        assert!(
            family_certified_for_kernels_v1(&alien).is_none(),
            "a class reaching a kernel no drill walked must not inherit the floor's certificate"
        );
    }

    /// **ADR-0069 invariant 1/5: no unearned weight on the network this build ships.**
    ///
    /// Reads the real `Params` a node boots with, walks the classes its genesis registers, and
    /// requires that every one holding a nonzero share is covered by a family this build certified
    /// end to end. This is the test the launch audit's central finding needed and nothing had:
    /// coverage passed for all three shipped classes, so every existing gate was green while 97.8%
    /// of cadence sat on two families whose court methods are the trait's defaults.
    ///
    /// A weightless class is fine and is the point — it registers, produces and counts for
    /// liveness. What this refuses is a class that takes cadence away from work the court can
    /// check, in exchange for work it cannot.
    #[test]
    fn the_shipped_genesis_grants_weight_only_to_certified_families() {
        use kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1;
        use kaspa_consensus_core::palw_e2e_adjudicability::family_certified_for_kernels_v1;
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

        register_builtin_certified_families_v1();
        let params: kaspa_consensus_core::config::params::Params =
            kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 11).into();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("testnet-11 ships a ConsensusV2 bundle");
        };

        let mut unearned: Vec<(Hash64, u16)> = Vec::new();
        for object in &bundle.genesis_objects {
            let PalwConsensusObjectV2::ClassRegistered { class_id, share_permille, admission, .. } = object else { continue };
            if *share_permille == 0 {
                continue; // weightless: registered, produces, earns no cadence. Exactly the state the ADR adds.
            }
            // The profile is what names the kernels. A genesis registration carries it in its
            // admission carriage; the floor's is the one this crate derives, and a class whose
            // profile this build cannot even see is certainly not one it has drilled.
            let reachable = match admission.as_ref() {
                Some(carriage) => reachable_kernels_v1(&carriage.profile),
                None if *class_id == bundle.base_class_id => {
                    let court = kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(
                        kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES,
                        4,
                        2,
                    )
                    .expect("shipped court");
                    let entry = crate::classes::canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor");
                    let root = crate::rc::palw_rc_base0_artifact_root_v1().expect("pinned");
                    let resolved = crate::classes::resolve_class_v1(&court, entry.class_id(), root, &[]).expect("the floor resolves");
                    reachable_kernels_v1(&resolved.profile)
                }
                None => {
                    unearned.push((*class_id, *share_permille));
                    continue;
                }
            };
            if family_certified_for_kernels_v1(&reachable).is_none() {
                unearned.push((*class_id, *share_permille));
            }
        }

        assert!(
            unearned.is_empty(),
            "ADR-0069 invariant 1: these genesis classes hold weight this build cannot prosecute — they must register \
             weightless (share 0) until a backend for them certifies: {unearned:?}"
        );
    }

    /// **A family that declares no court cannot be drilled into one.**
    ///
    /// The Qwen3.6 backend is the shipped example — its court methods are the trait's defaults —
    /// and the drill must refuse it at the declaration rather than produce a thin certificate from
    /// whatever the defaults return. This is the test that keeps `supports_court()` honest in the
    /// only direction that matters: a family cannot certify by lying downward.
    #[test]
    fn a_family_with_no_court_is_refused_rather_than_thinly_certified() {
        let artifact = std::sync::Arc::new(crate::qwen36::test_fixture(4, 8));
        let geometry = crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4);
        let profile = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(geometry).expect("the fixture geometry projects");
        let backend = crate::qwen36_backend::Qwen36Backend::new(
            artifact,
            "Qwen3.6-fixture",
            (4, 2),
            profile.shape_profile_id(),
            b"misaka-palw-test".to_vec(),
        );
        let err = match drill_family_v1(Hash64::from_u64_word(0x36), &backend, &profile, Hash64::from_u64_word(0xA7), Hash64::from_u64_word(1))
        {
            Err(e) => e,
            Ok(_) => panic!("a backend whose court verbs are the trait defaults must not certify"),
        };
        assert!(matches!(err, PalwDrillError::Backend { what: "take a court's turn", .. }), "{err}");
    }
}

#[cfg(test)]
mod pin_tests {
    /// **The RC networks pin their `court_e2e_root`, and this build must reproduce it.**
    ///
    /// The root is consensus identity: it decides who may hold weight, so two nodes that computed
    /// it differently would disagree about who gets paid. Pinning it in the bundle and checking the
    /// build against the pin is the same discipline `court_catalog_root` follows — a build whose
    /// court can play a different set of families than the network agreed to must refuse to start,
    /// not quietly join and diverge.
    ///
    /// Order-independent on purpose: a root read out of a process-global registry would depend on
    /// whether a drill had run yet when the params were assembled, which is not a property a
    /// consensus identity may have.
    #[test]
    fn the_pinned_rc_e2e_root_is_what_this_build_certifies() {
        super::register_builtin_certified_families_v1();
        let built = kaspa_consensus_core::palw_e2e_adjudicability::palw_court_e2e_root_v1();
        assert_eq!(
            built,
            kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_court_e2e_root_v1(),
            "this build certifies a different family set than the RC networks pin"
        );
    }
}
