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
    PalwE2eCertificateV1, PalwE2eDrillEvidenceV1, PalwE2eError, PalwE2eFaultVectorV1, PalwE2eFreePromptCertificateV1,
    PalwE2eFreePromptDrillEvidenceV1, certify_e2e_family_v1, certify_e2e_free_prompt_lane_v1, table_of_slot_v1,
};
use kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3;
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

/// **Drill one family and record what it proved** — the half that needs the model.
///
/// Returns the evidence rather than a certificate so that the two halves can happen on different
/// machines: a family whose weights are tens of gigabytes drills once, wherever those weights are,
/// and exports these vectors. [`certify_e2e_family_v1`] grades them anywhere, with no artifact
/// present, because grading re-runs the adjudicator over recorded objects. That separation is what
/// keeps `court_e2e_root` a property of the BUILD rather than of which files a node happens to
/// hold — see `PalwE2eFaultVectorV1`'s note.
///
/// `anchor` decides the job, exactly as it does in production — the drill runs the canonical job
/// the chain would have asked for rather than one chosen to be easy. `artifact_root` is the class's
/// registered root, and the operand openings a close carries must prove against it: a drill that
/// proved its weight rows against some root of its own would certify a court reading weights the
/// chain never pinned.
///
/// The returned certificate is sealed — see [`certify_e2e_family_v1`] — so holding one IS the fact
/// that the shipped adjudicator convicted every planted fault and acquitted the honest run.
pub fn drill_family_evidence_v1(
    family_id: Hash64,
    backend: &dyn PalwExecutionBackendV1,
    profile: &PalwShapeProfileV3,
    artifact_root: Hash64,
    anchor: Hash64,
) -> Result<PalwE2eDrillEvidenceV1, PalwDrillError> {
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

    Ok(PalwE2eDrillEvidenceV1 { family_id, profile: profile.clone(), artifact_root, vectors, malformed_inputs_refused })
}

/// **Drill one family to a certificate** — [`drill_family_evidence_v1`] and then the grader.
///
/// The two are separate verbs because they run in different places for a family whose weights do
/// not fit in a build. Drilling needs the model; GRADING needs only the shipped adjudicator, so a
/// node with no artifact at all can certify from exported vectors and reach the same
/// `court_e2e_root`. For BASE-0 both happen here, because the floor is derived and needs no files.
pub fn drill_family_v1(
    family_id: Hash64,
    backend: &dyn PalwExecutionBackendV1,
    profile: &PalwShapeProfileV3,
    artifact_root: Hash64,
    anchor: Hash64,
) -> Result<PalwE2eCertificateV1, PalwDrillError> {
    let evidence = drill_family_evidence_v1(family_id, backend, profile, artifact_root, anchor)?;
    certify_e2e_family_v1(&evidence).map_err(PalwDrillError::Certify)
}

/// Malformed inputs derived from one honest material, in the shapes a gossiped message actually
/// takes: nothing, not-the-format, a prefix that stops mid-structure, and a body with something
/// appended. Truncation is the one that matters — it is how a length-prefixed decoder yields a
/// value that parses and does not cohere.
/// **The free-prompt twin of [`drill_family_evidence_v1`]** (ADR-0073 Decision 1f): the same
/// vectors, planted in a run over the USER's job.
///
/// The honest run is `execute_free_prompt`. The guilty run plants its fault under the job context
/// the honest run COMMITTED — read off its own capture, so the drill cannot quietly run a
/// different job — and both provers are handed the user's prompt. The malformed-material sweep
/// verifies under the free-prompt anchor, which is the job id. Every vector is filed with the
/// question it answered; the certifier checks that the bindings agree.
pub fn drill_free_prompt_evidence_v1(
    family_id: Hash64,
    backend: &dyn PalwExecutionBackendV1,
    profile: &PalwShapeProfileV3,
    artifact_root: Hash64,
    job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
    prompt_token_ids: &[u32],
) -> Result<kaspa_consensus_core::palw_e2e_adjudicability::PalwE2eFreePromptDrillEvidenceV1, PalwDrillError> {
    use kaspa_consensus_core::palw_e2e_adjudicability::{PalwE2eFreePromptDrillEvidenceV1, PalwE2eFreePromptQuestionV1};

    if !backend.supports_court() {
        return Err(PalwDrillError::Backend {
            what: "take a court's turn",
            why: "the backend declares no court (supports_court() is false)".to_string(),
        });
    }
    let prompt: Vec<usize> = prompt_token_ids.iter().map(|t| *t as usize).collect();
    let honest = backend
        .execute_free_prompt(job, &prompt)
        .map_err(|why| PalwDrillError::Backend { what: "execute the free prompt", why })?
        .outcome;
    let anchor = kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(job);
    let ctx = backend
        .capture_shape(&honest.material)
        .ok_or_else(|| PalwDrillError::Backend {
            what: "read the job context off its own capture",
            why: "capture_shape returned None".to_string(),
        })?
        .job_context;
    if ctx.job_id != anchor {
        return Err(PalwDrillError::Backend {
            what: "run the user's job",
            why: format!("the capture names job {} and the question is {anchor}", ctx.job_id),
        });
    }

    let leaf_count = kaspa_consensus_core::palw_step::step_leaf_count(profile, &ctx)
        .map_err(|e| PalwDrillError::Backend { what: "count its own step space", why: format!("{e:?}") })?;
    let mut candidates: Vec<Candidate> = Vec::new();
    for leaf in 0..leaf_count {
        let Some(coord) = canonical_step_coordinates(profile, &ctx, leaf) else { continue };
        let Some(table) = table_of_slot_v1(profile, coord.node_slot) else { continue };
        let decode = coord.call_index > 0;
        if candidates.iter().any(|c| c.table == table && c.decode == decode) {
            continue;
        }
        if backend.refutation_for_free_prompt_index(&honest.material, leaf, prompt_token_ids).is_err() {
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
        let guilty = backend
            .execute_with_injected_fault(&ctx, &prompt, leaf)
            .map_err(|why| PalwDrillError::Backend { what: "plant a drill fault", why })?;
        let honest_refutation = backend
            .refutation_for_free_prompt_index(&honest.material, leaf, prompt_token_ids)
            .map_err(|why| PalwDrillError::Backend { what: "open an honest refutation", why })?;
        let guilty_refutation = backend
            .refutation_for_free_prompt_index(&guilty.material, leaf, prompt_token_ids)
            .map_err(|why| PalwDrillError::Backend { what: "open a refutation for the planted fault", why })?;
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

    let mut malformed_inputs_refused = 0u32;
    for bytes in &malformed_variants_v1(&honest.material) {
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

    let questions = vec![PalwE2eFreePromptQuestionV1 { job: job.clone(), prompt_token_ids: prompt_token_ids.to_vec() }; vectors.len()];
    Ok(PalwE2eFreePromptDrillEvidenceV1 {
        evidence: PalwE2eDrillEvidenceV1 { family_id, profile: profile.clone(), artifact_root, vectors, malformed_inputs_refused },
        questions,
    })
}

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

/// The floor's own shipped class, resolved the way a node resolves it — the backend every RC
/// drill of the floor runs on, whichever lane.
fn floor_fixture_v1() -> Result<(crate::backend::Base0Backend, PalwShapeProfileV3, Hash64), PalwDrillError> {
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
    Ok((crate::backend::Base0Backend::new(resolved), profile, root))
}

fn drill_base0_v1() -> Result<PalwE2eCertificateV1, PalwDrillError> {
    let (backend, profile, root) = floor_fixture_v1()?;
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
    for (name, certificate) in
        [("PALW-BASE-0", base0_certificate_v1()), ("PALW-QWEN36", qwen36_certificate_v1()), ("PALW-QWEN25-A16", a16_certificate_v1())]
    {
        if let Ok(certificate) = certificate {
            kaspa_consensus_core::palw_e2e_adjudicability::register_certified_family_v1(certificate);
            registered.push(name);
        }
    }
    registered
}

/// The two model tiers' family ids, under the same construction the floor uses.
pub fn qwen36_family_id_v1() -> Hash64 {
    family_id_of("PALW-QWEN36")
}

pub fn a16_family_id_v1() -> Hash64 {
    family_id_of("PALW-QWEN25-A16")
}

fn family_id_of(name: &str) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/e2e-family-id/v1").to_state();
    h.update(name.as_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The hybrid tier's certificate, drilled on a FIXTURE geometry.**
///
/// The registered class is Qwen3.6 35B-A3B, whose weights are tens of gigabytes; drilling those at
/// startup would make `court_e2e_root` depend on which files a node happens to hold, which is the
/// environment-dependence the root is pinned to avoid. It does not need to: a certificate
/// generalises over the REACHABLE KERNEL SET, and measured, the production geometry and this
/// fixture reach exactly the same 23 kernels. What the drill proves — that this build's backend can
/// assemble a refutation, answer a rung and close, and that the shipped court convicts — is a
/// property of the code and the kernel arithmetic, not of how many layers are stacked.
///
/// **What it therefore does NOT prove, stated because a certificate that overclaims is worse than
/// none:** a fault that only appears at production dimensions. The GDN geometry ceiling found by
/// the ADR-0068 sweep (a `1<<24` head dimension underflowing a shift) is exactly that class of
/// defect, and a fixture drill would not have caught it. This certificate says the family is
/// PROSECUTABLE; it does not say the family is bug-free, and the sweep is what answers the second
/// question.
pub fn qwen36_certificate_v1() -> Result<&'static PalwE2eCertificateV1, &'static PalwDrillError> {
    static CERT: std::sync::OnceLock<Result<PalwE2eCertificateV1, PalwDrillError>> = std::sync::OnceLock::new();
    CERT.get_or_init(drill_qwen36_v1).as_ref()
}

/// The fixture graph the QWEN36 family is drilled on: `qwen36_dev_fixture(4, 8)` under the
/// registered (graph-v2+) profile — small enough to sweep every leaf, and reaching the same kernels
/// the 35B class reaches, which is what a family certificate is about.
fn qwen36_fixture_v1() -> Result<(crate::qwen36_backend::Qwen36Backend, PalwShapeProfileV3, Hash64), PalwDrillError> {
    use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_profile_v2};

    let artifact = std::sync::Arc::new(crate::qwen36::qwen36_dev_fixture(4, 8));
    let s = &artifact.shape;
    // The fixture's own shape, projected into the geometry the registered class is described by.
    // Read off the artifact rather than written out, so the two cannot describe different models.
    let geometry = PalwQwen36GeometryV1 {
        layer_count: s.n_layers() as u16,
        full_attention_interval: 4,
        hidden_dim: s.d_model as u32,
        attn_heads: s.n_heads as u16,
        attn_kv_heads: s.n_kv_heads as u16,
        attn_head_dim: s.head_dim as u32,
        rope_dims: s.rotary_dim as u16,
        rope_freq_base_bits: 0x4B18_9680,
        gdn_k_heads: s.linear_k_heads as u16,
        gdn_v_heads: s.linear_v_heads as u16,
        gdn_head_dim: s.linear_head_dim as u32,
        gdn_conv_kernel: s.conv_kernel as u16,
        n_experts: s.n_experts as u32,
        experts_per_token: s.experts_per_token as u32,
        moe_dim: s.moe_dim as u32,
        shared_dim: s.shared_dim as u32,
        attn_output_gate: if s.attn_output_gate() { 1 } else { 0 },
        vocab_size: s.vocab as u32,
        n_ctx: 8,
        n_threads: 1,
        rms_eps_q: s.eps_q,
        tile_len: 4,
    };
    let profile = qwen36_profile_v2(geometry)
        .map_err(|e| PalwDrillError::Backend { what: "project the fixture geometry", why: format!("{e:?}") })?;
    // **From the REGISTERED declaration, not the compiled table.** `supports_court` is true only
    // for a backend that holds both a plan and the profile it was planned from, because a court's
    // coordinates are the profile's — a backend running its own hardcoded graph could not place a
    // capture at the leaves the chain's class enumerates.
    // **The fixture's OWN inventory root.** A close's weight rows must prove against the root the
    // class registered, and the certifier checks exactly that — so the drill has to hand it the
    // root its own backend opens against, computed from the same artifact and profile the backend
    // holds. A value invented here produces evidence nothing can verify, which is the certifier
    // doing its job and the drill wasting a run.
    let root = crate::inventory::qwen36_inventory_v1(&artifact, &profile)
        .map_err(|e| PalwDrillError::Backend { what: "root its own fixture inventory", why: format!("{e:?}") })?
        .root();
    let backend = Qwen36BackendCtor::build(artifact, profile.clone())?;
    Ok((backend, profile, root))
}

fn drill_qwen36_v1() -> Result<PalwE2eCertificateV1, PalwDrillError> {
    let (backend, profile, root) = qwen36_fixture_v1()?;
    drill_family_v1(qwen36_family_id_v1(), &backend, &profile, root, Hash64::from_u64_word(0x0E2E_D836))
}

/// A named constructor so the two `from_registered_profile` failure modes read differently in a
/// log: a graph this build cannot serve is a different problem from an artifact it cannot open.
struct Qwen36BackendCtor;

impl Qwen36BackendCtor {
    fn build(
        artifact: std::sync::Arc<crate::qwen36::Qwen36ArtifactV1>,
        profile: PalwShapeProfileV3,
    ) -> Result<crate::qwen36_backend::Qwen36Backend, PalwDrillError> {
        crate::qwen36_backend::Qwen36Backend::from_registered_profile(artifact, b"misaka-palw-rc".to_vec(), profile, (4, 2))
            .map_err(|why| PalwDrillError::Backend { what: "serve its own registered graph", why })
    }
}

/// **The dense tier's certificate, drilled on a fixture** — same reasoning as the hybrid's, and the
/// same limitation. Measured: the production A16 geometry and this fixture reach the same 12
/// kernels.
///
/// The class this certifies is the one whose profile declares the four-byte state map. The map is
/// not a detail: `supports_court` is false without it, because a one-byte map cannot describe an
/// `i32` KV cache and a checkpoint taken under it would open to a state the producer never held.
pub fn a16_certificate_v1() -> Result<&'static PalwE2eCertificateV1, &'static PalwDrillError> {
    static CERT: std::sync::OnceLock<Result<PalwE2eCertificateV1, PalwDrillError>> = std::sync::OnceLock::new();
    CERT.get_or_init(drill_a16_v1).as_ref()
}

/// The fixture graph the QWEN25-A16 family is drilled on: a two-layer derived A16 store under the
/// corrected (graph-v2) profile.
fn a16_fixture_v1() -> Result<(crate::qwen25_a16_backend::Qwen25A16Backend, PalwShapeProfileV3, Hash64), PalwDrillError> {
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v2};

    let geometry = PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 8,
        ffn_dim: 8,
        attn_heads: 2,
        attn_kv_heads: 2,
        attn_head_dim: 4,
        vocab_size: 64,
        n_ctx: 32,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    };
    let shape = crate::artifact::Base0ShapeV1 {
        n_layers: geometry.layer_count as usize,
        n_heads: geometry.attn_heads as usize,
        n_kv_heads: geometry.attn_kv_heads as usize,
        d_head: geometry.attn_head_dim as usize,
        d_ff: geometry.ffn_dim as usize,
        vocab: geometry.vocab_size as usize,
        max_position: geometry.n_ctx as usize,
        ln_theta_gen_q: crate::artifact::LN_THETA_10000_GEN_Q,
        eps_q: 1,
    };
    let artifact = crate::artifact::Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
        .map_err(|e| PalwDrillError::Backend { what: "derive its fixture weights", why: format!("{e:?}") })?
        .with_a16_params(crate::engine_a16::derived_a16_store(&shape))
        .map_err(|e| PalwDrillError::Backend { what: "derive its A16 parameter store", why: format!("{e:?}") })?;
    let profile = qwen25_a16_profile_v2(geometry)
        .map_err(|e| PalwDrillError::Backend { what: "project the fixture geometry", why: format!("{e:?}") })?;
    // The fixture's own inventory root — see the hybrid tier's twin comment.
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile)
        .map_err(|e| PalwDrillError::Backend { what: "root its own fixture inventory", why: format!("{e:?}") })?
        .root();
    let backend = crate::qwen25_a16_backend::Qwen25A16Backend::from_registered_profile(
        std::sync::Arc::new(artifact),
        b"misaka-palw-rc".to_vec(),
        profile.clone(),
        (4, 2),
    )
    .map_err(|why| PalwDrillError::Backend { what: "serve its own registered graph", why })?;
    Ok((backend, profile, root))
}

fn drill_a16_v1() -> Result<PalwE2eCertificateV1, PalwDrillError> {
    let (backend, profile, root) = a16_fixture_v1()?;
    drill_family_v1(a16_family_id_v1(), &backend, &profile, root, Hash64::from_u64_word(0x0E2E_D825))
}

// ---------------------------------------------------------------------------------------------
// ADR-0075: the drills as evidence, and the free-prompt lane of every RC family
// ---------------------------------------------------------------------------------------------

/// The three families this build ships drills for, by the name their family id hashes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRcFamilyV1 {
    Base0,
    Qwen36,
    Qwen25A16,
}

impl PalwRcFamilyV1 {
    pub const ALL: [Self; 3] = [Self::Base0, Self::Qwen36, Self::Qwen25A16];

    pub fn name(self) -> &'static str {
        match self {
            Self::Base0 => "PALW-BASE-0",
            Self::Qwen36 => "PALW-QWEN36",
            Self::Qwen25A16 => "PALW-QWEN25-A16",
        }
    }

    pub fn family_id(self) -> Hash64 {
        family_id_of(self.name())
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "base0" | "palw-base-0" | "floor" => Some(Self::Base0),
            "qwen36" | "palw-qwen36" => Some(Self::Qwen36),
            "a16" | "qwen25-a16" | "palw-qwen25-a16" => Some(Self::Qwen25A16),
            _ => None,
        }
    }
}

/// **The attempt-lane drill of an RC family, as evidence** — the bytes a `FamilyCertified`
/// object carries (ADR-0075 Decision 1), graded on chain by the same court that grades them here.
/// Uncached: an export is a one-off; the certificate producers keep their own.
pub fn rc_attempt_evidence_v1(family: PalwRcFamilyV1) -> Result<PalwE2eDrillEvidenceV1, PalwDrillError> {
    match family {
        PalwRcFamilyV1::Base0 => {
            let (backend, profile, root) = floor_fixture_v1()?;
            drill_family_evidence_v1(family.family_id(), &backend, &profile, root, Hash64::from_u64_word(0x0E2E_D8111))
        }
        PalwRcFamilyV1::Qwen36 => {
            let (backend, profile, root) = qwen36_fixture_v1()?;
            drill_family_evidence_v1(family.family_id(), &backend, &profile, root, Hash64::from_u64_word(0x0E2E_D836))
        }
        PalwRcFamilyV1::Qwen25A16 => {
            let (backend, profile, root) = a16_fixture_v1()?;
            drill_family_evidence_v1(family.family_id(), &backend, &profile, root, Hash64::from_u64_word(0x0E2E_D825))
        }
    }
}

/// The user-chosen prompt each RC family's free-prompt lane is drilled with, and the decode
/// budget. Not the anchor's prompt: the lane's whole claim is that the court adjudicates the
/// prompt a USER handed the class. The QWEN36 fixture's context is eight tokens.
pub fn rc_free_prompt_question_v1(family: PalwRcFamilyV1) -> (Vec<u32>, u32) {
    match family {
        PalwRcFamilyV1::Base0 | PalwRcFamilyV1::Qwen25A16 => (vec![3, 5, 8, 13, 21], 2),
        PalwRcFamilyV1::Qwen36 => (vec![3, 5, 8, 13], 2),
    }
}

/// A free-prompt job for a drill: the class is the profile's, the prompt is `ids`, everything
/// the chain would fill in (anchor, bond, nonce) is a fixed constant so two drills of one family
/// are one drill.
pub fn fp_drill_job_v1(profile: &PalwShapeProfileV3, ids: &[u32], decode: u32) -> PalwFreePromptJobV3 {
    use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION};
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: Hash64::from_u64_word(0xD0),
        class_id: profile.shape_profile_id(),
        executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
        executor_pubkey: vec![0x11; 32],
        operator_id: Hash64::from_u64_word(0x0B),
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 1234,
        job_nonce: [0x5A; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(ids),
        prompt_tokens: ids.len() as u32,
        decode_token_limit: decode,
        max_context_tokens: profile.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
    }
}

/// **The free-prompt-lane drill of an RC family, as evidence** (ADR-0074 Decision 6, ADR-0075).
/// Same fixture graph as the attempt lane, a caller's prompt instead of the anchor's.
pub fn rc_free_prompt_evidence_v1(family: PalwRcFamilyV1) -> Result<PalwE2eFreePromptDrillEvidenceV1, PalwDrillError> {
    let (ids, decode) = rc_free_prompt_question_v1(family);
    match family {
        PalwRcFamilyV1::Base0 => {
            let (backend, profile, root) = floor_fixture_v1()?;
            let job = fp_drill_job_v1(&profile, &ids, decode);
            drill_free_prompt_evidence_v1(family.family_id(), &backend, &profile, root, &job, &ids)
        }
        PalwRcFamilyV1::Qwen36 => {
            let (backend, profile, root) = qwen36_fixture_v1()?;
            let job = fp_drill_job_v1(&profile, &ids, decode);
            drill_free_prompt_evidence_v1(family.family_id(), &backend, &profile, root, &job, &ids)
        }
        PalwRcFamilyV1::Qwen25A16 => {
            let (backend, profile, root) = a16_fixture_v1()?;
            let job = fp_drill_job_v1(&profile, &ids, decode);
            drill_free_prompt_evidence_v1(family.family_id(), &backend, &profile, root, &job, &ids)
        }
    }
}

/// **A class this build's catalogs can express, by model id** — the floor and the A16 rows live
/// in `canonical_classes_v1`, the Qwen36 rows in `qwen36_canonical_classes_v1`; a certification
/// tool must not care which. Rows whose profile does not project (a geometry deeper than the
/// ladder) are absent, as they are absent from registration.
pub fn catalog_profile_by_model_id_v1(
    court: &kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2,
    model_id: &str,
) -> Option<PalwShapeProfileV3> {
    if let Some(entry) = crate::classes::canonical_class_by_model_id_v1(court, model_id) {
        return Some(entry.profile);
    }
    crate::classes::qwen36_canonical_classes_v1().into_iter().find(|row| row.model_id == model_id).and_then(|row| row.profile().ok())
}

/// Every `(model id, profile)` the catalogs express, both tables.
pub fn catalog_profiles_v1(court: &kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2) -> Vec<(String, PalwShapeProfileV3)> {
    let mut out: Vec<(String, PalwShapeProfileV3)> =
        crate::classes::canonical_classes_v1(court).into_iter().map(|c| (c.model_id.to_string(), c.profile)).collect();
    for row in crate::classes::qwen36_canonical_classes_v1() {
        if let Ok(profile) = row.profile() {
            out.push((row.model_id.to_string(), profile));
        }
    }
    out
}

/// **Which RC family's drill certifies `profile`'s kernels for `lane`** (ADR-0075 Decision 5, at
/// the tool's side): the first pinned RC family whose kernel set contains every kernel the profile
/// reaches, read off the pinned sets rather than by drilling, so a `palw-certify drill --model-id`
/// knows which fixture to run before running it. `None` means no family this build ships can be
/// drilled for the graph — a new architecture, which needs a build that serves it (ADR-0069
/// Decision 2: a certificate is about kernels the court implements).
pub fn covering_rc_family_v1(
    profile: &PalwShapeProfileV3,
    lane: kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1,
) -> Option<PalwRcFamilyV1> {
    use kaspa_consensus_core::palw_e2e_adjudicability::{palw_rc_certified_families_v1, palw_rc_fp_certified_families_v1};
    use kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1;

    let reachable = kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(profile);
    let families = match lane {
        PalwCertifiedLaneV1::Attempt => palw_rc_certified_families_v1(),
        PalwCertifiedLaneV1::FreePrompt => palw_rc_fp_certified_families_v1(),
    };
    PalwRcFamilyV1::ALL
        .into_iter()
        .find(|family| families.iter().any(|f| f.family_id == family.family_id() && reachable.is_subset(&f.kernel_ids)))
}

fn drill_fp_v1(family: PalwRcFamilyV1) -> Result<PalwE2eFreePromptCertificateV1, PalwDrillError> {
    let evidence = rc_free_prompt_evidence_v1(family)?;
    certify_e2e_free_prompt_lane_v1(&evidence).map_err(PalwDrillError::Certify)
}

pub fn base0_fp_certificate_v1() -> Result<&'static PalwE2eFreePromptCertificateV1, &'static PalwDrillError> {
    static CERT: std::sync::OnceLock<Result<PalwE2eFreePromptCertificateV1, PalwDrillError>> = std::sync::OnceLock::new();
    CERT.get_or_init(|| drill_fp_v1(PalwRcFamilyV1::Base0)).as_ref()
}

pub fn qwen36_fp_certificate_v1() -> Result<&'static PalwE2eFreePromptCertificateV1, &'static PalwDrillError> {
    static CERT: std::sync::OnceLock<Result<PalwE2eFreePromptCertificateV1, PalwDrillError>> = std::sync::OnceLock::new();
    CERT.get_or_init(|| drill_fp_v1(PalwRcFamilyV1::Qwen36)).as_ref()
}

pub fn a16_fp_certificate_v1() -> Result<&'static PalwE2eFreePromptCertificateV1, &'static PalwDrillError> {
    static CERT: std::sync::OnceLock<Result<PalwE2eFreePromptCertificateV1, PalwDrillError>> = std::sync::OnceLock::new();
    CERT.get_or_init(|| drill_fp_v1(PalwRcFamilyV1::Qwen25A16)).as_ref()
}

pub fn rc_fp_certificate_v1(family: PalwRcFamilyV1) -> Result<&'static PalwE2eFreePromptCertificateV1, &'static PalwDrillError> {
    match family {
        PalwRcFamilyV1::Base0 => base0_fp_certificate_v1(),
        PalwRcFamilyV1::Qwen36 => qwen36_fp_certificate_v1(),
        PalwRcFamilyV1::Qwen25A16 => a16_fp_certificate_v1(),
    }
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

    /// **Every profile this build knows how to describe, by the class id it registers under.**
    ///
    /// A genesis registration carries no admission carriage — `admission: None` is the genesis
    /// FORM, because the carriage is what the post-genesis acceptance path reads — so the profile
    /// of a genesis-registered class is not in the object. It is in the build, in the same class
    /// tables the SDK dispatches on, and that is where this looks.
    fn known_profiles() -> std::collections::BTreeMap<Hash64, PalwShapeProfileV3> {
        let court =
            kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                .expect("shipped court");
        let mut out = std::collections::BTreeMap::new();
        for class in crate::classes::canonical_classes_v1(&court) {
            out.insert(class.class_id(), class.profile);
        }
        for class in crate::classes::qwen36_canonical_classes_v1() {
            if let (Some(id), Ok(profile)) = (class.class_id(), class.profile()) {
                out.insert(id, profile);
            }
        }
        out
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
    ///
    /// **"I cannot see the profile" and "it is not certified" are different answers, and this
    /// separates them.** The first version of this test did not: it resolved the profile only from
    /// the registration's admission carriage, which a GENESIS registration never carries, so every
    /// genesis model class fell into the uncertified bucket by construction. While those classes
    /// held no share the two answers coincided and nothing showed it — the blind spot was reachable
    /// only by giving the model tiers weight, which is the exact moment this test is supposed to
    /// speak. An unresolvable profile is now its own failure, because a registered class whose
    /// graph this build cannot even name is a class no node can serve.
    #[test]
    fn the_shipped_genesis_grants_weight_only_to_certified_families() {
        use kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1;
        use kaspa_consensus_core::palw_e2e_adjudicability::family_certified_for_kernels_v1;
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

        register_builtin_certified_families_v1();
        let known = known_profiles();
        let params: kaspa_consensus_core::config::params::Params =
            kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 11).into();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("testnet-11 ships a ConsensusV2 bundle");
        };

        let mut unearned: Vec<(Hash64, u16)> = Vec::new();
        let mut unresolvable: Vec<(Hash64, u16)> = Vec::new();
        for object in &bundle.genesis_objects {
            let PalwConsensusObjectV2::ClassRegistered { class_id, share_permille, admission, .. } = object else { continue };
            if *share_permille == 0 {
                continue; // weightless: registered, produces, earns no cadence. Exactly the state the ADR adds.
            }
            // The carriage where a post-genesis registration carries its graph; the build's own
            // tables where a genesis one does not.
            let reachable = match admission.as_ref() {
                Some(carriage) => reachable_kernels_v1(&carriage.profile),
                None => match known.get(class_id) {
                    Some(profile) => reachable_kernels_v1(profile),
                    None => {
                        unresolvable.push((*class_id, *share_permille));
                        continue;
                    }
                },
            };
            if family_certified_for_kernels_v1(&reachable).is_none() {
                unearned.push((*class_id, *share_permille));
            }
        }

        assert!(
            unresolvable.is_empty(),
            "these genesis classes hold weight and this build cannot even name their graphs — no node could serve them, \
             whatever their certification says: {unresolvable:?}"
        );
        assert!(
            unearned.is_empty(),
            "ADR-0069 invariant 1: these genesis classes hold weight this build cannot prosecute — they must register \
             weightless (share 0) until a backend for them certifies: {unearned:?}"
        );

        // **And the test can still fail.** Every assertion above passes vacuously on a genesis that
        // grants nobody weight, which is what this network looked like an hour ago — so the last
        // thing checked is that there was actually something to check.
        let weighted = bundle
            .genesis_objects
            .iter()
            .filter(|o| matches!(o, PalwConsensusObjectV2::ClassRegistered { share_permille, .. } if *share_permille > 0))
            .count();
        assert!(
            weighted >= 2,
            "the shipped genesis grants weight to {weighted} class(es) — this test proves nothing on a table of zeros"
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
        let err = match drill_family_v1(
            Hash64::from_u64_word(0x36),
            &backend,
            &profile,
            Hash64::from_u64_word(0xA7),
            Hash64::from_u64_word(1),
        ) {
            Err(e) => e,
            Ok(_) => panic!("a backend whose court verbs are the trait defaults must not certify"),
        };
        assert!(matches!(err, PalwDrillError::Backend { what: "take a court's turn", .. }), "{err}");
    }

    /// **The floor's free-prompt lane certifies through the shipped court** (ADR-0073 Decision
    /// 1f) — the same drill as the attempt lane's, over a job the user fixed — and the certifier
    /// refuses the substitutions that matter: a question the vectors were not about, and a
    /// question that is not its own. The RC free-prompt-certified set's floor entry is pinned
    /// from this very drill.
    #[test]
    fn the_floor_free_prompt_lane_certifies_and_a_swapped_question_is_refused() {
        use kaspa_consensus_core::palw_e2e_adjudicability::{
            PalwE2eError, certify_e2e_free_prompt_lane_v1, palw_court_e2e_root_of_v1, palw_rc_court_fp_e2e_root_v1,
            palw_rc_fp_certified_families_v1,
        };
        use kaspa_consensus_core::palw_freeprompt_v3::{
            PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3,
        };
        use kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2;
        use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

        let court =
            kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                .expect("the shipped court");
        let entry = crate::classes::canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor is registered");
        let root = crate::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
        let resolved = crate::classes::resolve_class_v1(&court, entry.class_id(), root, &[]).expect("the floor resolves");
        let profile = resolved.profile.clone();
        let backend = crate::backend::Base0Backend::new(resolved);

        let ids: Vec<u32> = vec![3, 5, 8, 13, 21];
        let job = PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(0xD0),
            class_id: profile.shape_profile_id(),
            executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
            executor_pubkey: vec![0x11; 32],
            operator_id: Hash64::from_u64_word(0x0B),
            anchor_block: Hash64::from_u64_word(0xA0),
            anchor_daa: 1234,
            job_nonce: [0x5A; 32],
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: prompt_token_ids_hash_v2(&ids),
            prompt_tokens: ids.len() as u32,
            decode_token_limit: 2,
            max_context_tokens: profile.n_ctx,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        };
        let drill = drill_free_prompt_evidence_v1(base0_family_id_v1(), &backend, &profile, root, &job, &ids)
            .expect("the floor drills its free-prompt lane");
        let certificate = certify_e2e_free_prompt_lane_v1(&drill).expect("the floor's free-prompt lane certifies");
        // **The RC set's floor entry IS this drill's family** (ADR-0074 Decision 6): the pinned
        // covering, kernel set and drilled class are what the shipped court certified here.
        let rc = palw_rc_fp_certified_families_v1();
        let pinned = rc
            .iter()
            .find(|f| f.family_id == base0_family_id_v1())
            .expect("the floor is among the free-prompt-certified families (ADR-0075 Decision 6 adds the model tiers)");
        assert_eq!(*pinned, certificate.family, "the RC entry is pinned from this drill, field for field");
        let attempt = base0_certificate_v1().expect("the floor's attempt certificate");
        assert_eq!(certificate.family.drilled_class_id, attempt.family.drilled_class_id, "one graph, whichever lane drilled it");
        assert_eq!(certificate.family.kernel_ids, attempt.family.kernel_ids, "…and one kernel set");
        assert!(drill.evidence.vectors.iter().all(|v| v.honest.prompt_token_ids == ids), "every prover was handed the user's prompt");

        // A question the vectors were not about: another prompt of the same length, hashed into
        // its own job. The vectors' bindings still name the job they actually ran.
        let other_ids: Vec<u32> = vec![4, 6, 9, 14, 22];
        let mut other_job = job.clone();
        other_job.prompt_token_ids_hash = prompt_token_ids_hash_v2(&other_ids);
        let mut swapped = drill.clone();
        for question in &mut swapped.questions {
            question.job = other_job.clone();
            question.prompt_token_ids = other_ids.clone();
        }
        assert!(
            matches!(certify_e2e_free_prompt_lane_v1(&swapped), Err(PalwE2eError::VectorIsNotAboutTheQuestion { .. })),
            "vectors about one job do not certify another"
        );
        // A question that is not its own (the ids do not hash to the job) is refused before any
        // vector is read.
        let mut not_own = drill.clone();
        not_own.questions[0].prompt_token_ids[0] ^= 1;
        assert!(matches!(certify_e2e_free_prompt_lane_v1(&not_own), Err(PalwE2eError::FreePromptQuestionNotItsOwn { .. })));

        assert_eq!(palw_rc_court_fp_e2e_root_v1(), palw_court_e2e_root_of_v1(&rc), "the root is the set's");
    }
}

#[cfg(test)]
mod certification_object_tests {
    use super::*;
    use kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1;
    use kaspa_consensus_core::palw_e2e_adjudicability::{
        palw_court_e2e_root_of_v1, palw_rc_court_fp_e2e_root_v1, palw_rc_fp_certified_class_ids_v1, palw_rc_fp_certified_families_v1,
    };
    use kaspa_consensus_core::palw_state_v2::{
        PalwBlockContextV2, PalwBondKeyV2, PalwCertificationEvidenceV1, PalwCertifiedLaneV1, PalwChainStateV2,
        PalwConsensusObjectV2 as Obj, PalwPwuRuleV2, PalwStateParamsV2, PalwStateV2Error, apply_palw_transition_v2, revert_delta_v2,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    /// **Every RC family's free-prompt lane drills, and the set the network pins is those drills**
    /// (ADR-0074 Decision 6, ADR-0075 Decision 6) — field for field, root for root.
    #[test]
    fn the_rc_free_prompt_set_is_the_one_this_build_drilled() {
        let committed = palw_rc_fp_certified_families_v1();
        let mut drilled = Vec::new();
        for family in PalwRcFamilyV1::ALL {
            let certificate =
                rc_fp_certificate_v1(family).unwrap_or_else(|e| panic!("{} drills its free-prompt lane: {e}", family.name()));
            drilled.push(certificate.family.clone());
        }
        assert_eq!(
            committed.len(),
            drilled.len(),
            "the network pins {} free-prompt families and this build drilled {}",
            committed.len(),
            drilled.len()
        );
        for want in &committed {
            let got = drilled
                .iter()
                .find(|f| f.family_id == want.family_id)
                .unwrap_or_else(|| panic!("this build drilled no free-prompt family {}", want.family_id));
            assert_eq!(got, want, "the drilled free-prompt family and the pinned one differ");
        }
        assert_eq!(palw_rc_court_fp_e2e_root_v1(), palw_court_e2e_root_of_v1(&drilled), "the free-prompt root is the drilled set's");
    }

    /// **The classes whose free-prompt lane the RC networks certify at genesis are exactly the
    /// RC classes some drilled free-prompt family covers** — the rule `ClassLaneCertified`
    /// applies on chain, applied at build time (ADR-0075 Decision 6).
    #[test]
    fn the_rc_free_prompt_classes_are_the_covered_ones() {
        use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
        use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_1_5B_A16, qwen25_a16_profile_v2};
        use kaspa_consensus_core::palw_qwen36_profile::{QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_profile_v2};

        let families = palw_rc_fp_certified_families_v1();
        let ids = palw_rc_fp_certified_class_ids_v1();
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor projects");
        let hybrid = qwen36_profile_v2(qwen36_geometry_artifact_eps(QWEN36_35B_A3B)).expect("the hybrid projects");
        let dense = qwen25_a16_profile_v2(QWEN25_1_5B_A16).expect("the dense tier projects");
        for (name, profile) in [("PALW-BASE-0", &floor), ("PALW-QWEN36 graph-v3", &hybrid), ("PALW-QWEN25-A16 graph-v2", &dense)] {
            let reachable = reachable_kernels_v1(profile);
            assert!(
                families.iter().any(|f| reachable.is_subset(&f.kernel_ids)),
                "{name}'s kernels are covered by no free-prompt-certified family"
            );
            assert!(ids.contains(&profile.shape_profile_id()), "{name} is covered but not in the free-prompt-certified class set");
        }
        assert_eq!(ids.len(), 3, "the three RC classes, and nothing else");
    }

    /// **Every class this build's catalog can express is certifiable without a code change**
    /// (ADR-0075, the mainnet route): for each catalog row on a registered graph (graph-v2/v3 —
    /// the v1 rows are the legacy graphs no certified family covers by design), some RC family's
    /// drill covers its kernels on BOTH lanes, so `palw-certify drill --model-id` always has a
    /// fixture to run.
    #[test]
    fn every_catalog_class_on_a_registered_graph_is_covered_by_a_drillable_family() {
        use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
        let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court");
        let mut checked = 0;
        for (model_id, profile) in catalog_profiles_v1(&court) {
            let legacy = model_id != "PALW-BASE-0/rc" && !model_id.contains("/graph-v");
            if legacy {
                continue;
            }
            for lane in [PalwCertifiedLaneV1::Attempt, PalwCertifiedLaneV1::FreePrompt] {
                let family = covering_rc_family_v1(&profile, lane);
                assert!(family.is_some(), "{model_id} has no drillable RC family for the {lane} lane");
            }
            checked += 1;
        }
        assert!(checked >= 5, "the floor, the A16 graph-v2 row and the three Qwen36 graph-v3 rows, checked {checked}");
    }

    /// **A model this build never pinned becomes weight-bearing through the chain alone**: the
    /// Qwen3.5-2B graph-v3 class (dense, one expert — not in any RC certified set) registers
    /// weightless, the QWEN36 family's drill is posted, the class is bound, and it holds the floor
    /// share. The same three transactions a stranger sends on mainnet.
    #[test]
    fn a_model_this_build_never_pinned_is_seated_through_the_chain_alone() {
        use kaspa_consensus_core::palw_e2e_adjudicability::{family_certified_for_weight_v1, palw_rc_certified_families_v1};
        use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;

        let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court");
        let entrant_profile = catalog_profile_by_model_id_v1(&court, "Qwen/Qwen3.5-2B/graph-v3").expect("catalog row");
        let entrant_class = entrant_profile.shape_profile_id();
        let reachable = reachable_kernels_v1(&entrant_profile);
        // Not in the build's committed set — that is the premise of the route.
        let genesis_covers = family_certified_for_weight_v1(
            kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_court_e2e_root_v1(),
            &palw_rc_certified_families_v1(),
            &reachable,
        );
        let covering = covering_rc_family_v1(&entrant_profile, PalwCertifiedLaneV1::Attempt).expect("a drillable family");
        assert_eq!(covering, PalwRcFamilyV1::Qwen36, "the dense Qwen3.5 graph reaches the QWEN36 family's kernels");

        let (a16, a16_profile, a16_root) = a16_fixture_v1().expect("the base class fixture");
        let base_class = a16_profile.shape_profile_id();
        let (canonical, _) = a16.job_for_anchor(h(0xF1)).expect("a canonical job");
        let base_leaves = kaspa_consensus_core::palw_step::step_leaf_count(&a16_profile, &canonical).expect("counts");
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, base_class, 4, 1000, 100, 800, 0).unwrap();
        let floor = params.min_grantable_share_permille();
        let registrations = vec![
            Obj::ClassRegistered {
                class_id: base_class,
                artifact_root: a16_root,
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: base_leaves },
                initial_target: u128::MAX,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            Obj::ClassRegistered {
                class_id: entrant_class,
                artifact_root: h(0x2B),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1_000 },
                initial_target: u128::MAX,
                share_permille: 0,
                activation_daa: 0,
                admission: None,
            },
        ];
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &params, &at(1, 100, 1), &registrations, None)
            .expect("a weightless registration is always admissible");
        assert_eq!(s1.class_share_permille(&entrant_class), Some(0));

        let bind = Obj::ClassLaneCertified {
            class_id: entrant_class,
            lane: PalwCertifiedLaneV1::Attempt,
            profile: Box::new(entrant_profile.clone()),
        };
        if genesis_covers.map(|f| f.is_none()).unwrap_or(true) {
            assert_eq!(
                apply_palw_transition_v2(&s1, &params, &at(2, 101, 2), std::slice::from_ref(&bind), None).unwrap_err(),
                PalwStateV2Error::NoCertifiedFamilyCovers { class: entrant_class, lane: PalwCertifiedLaneV1::Attempt },
                "before the family is posted there is nothing to bind to"
            );
        }
        let evidence = rc_attempt_evidence_v1(covering).expect("the covering family drills");
        let (s2, _) = apply_palw_transition_v2(
            &s1,
            &params,
            &at(2, 101, 2),
            &[Obj::FamilyCertified { evidence: Box::new(PalwCertificationEvidenceV1::Attempt(evidence)) }],
            None,
        )
        .expect("the court grades the drill");
        let (s3, _) = apply_palw_transition_v2(&s2, &params, &at(3, 102, 3), &[bind], None).expect("the entrant is seated");
        assert_eq!(s3.class_share_permille(&entrant_class), Some(floor), "weight-bearing at the floor");
        assert_eq!(s3.class_share_permille(&base_class), Some(1000 - floor));
    }

    /// **A drill too large for one carrier rides in chunks and certifies exactly as one that
    /// fits** (ADR-0075 Decision 14): the floor's attempt drill is ~310 KB, four chunks; the
    /// family is recorded in the block that completes the group, and nothing before it.
    #[test]
    fn a_drill_too_large_for_one_carrier_certifies_through_chunks() {
        use kaspa_consensus_core::palw_state_v2::{PALW_OBJECT_CHUNK_MAX_BYTES, palw_object_chunks_v1};

        let (a16, a16_profile, a16_root) = a16_fixture_v1().expect("the base class fixture");
        let base_class = a16_profile.shape_profile_id();
        let (canonical, _) = a16.job_for_anchor(h(0xF1)).expect("a canonical job");
        let base_leaves = kaspa_consensus_core::palw_step::step_leaf_count(&a16_profile, &canonical).expect("counts");
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, base_class, 4, 1000, 100, 800, 0).unwrap();
        let registration = Obj::ClassRegistered {
            class_id: base_class,
            artifact_root: a16_root,
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: base_leaves },
            initial_target: u128::MAX,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        };
        let (s0, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &params, &at(1, 100, 1), &[registration], None).unwrap();

        let evidence = rc_attempt_evidence_v1(PalwRcFamilyV1::Base0).expect("the floor drills");
        let object = Obj::FamilyCertified { evidence: Box::new(PalwCertificationEvidenceV1::Attempt(evidence)) };
        let bytes = borsh::to_vec(&object).unwrap().len();
        assert!(bytes > PALW_OBJECT_CHUNK_MAX_BYTES, "the floor's drill is {bytes} bytes — the case chunks exist for");
        let chunks = palw_object_chunks_v1(&object).expect("chunkable").expect("chunked");
        let mut state = s0;
        let mut daa = 101;
        for (i, chunk) in chunks.iter().enumerate() {
            let (next, _) = apply_palw_transition_v2(&state, &params, &at(daa, daa, daa), std::slice::from_ref(chunk), None)
                .unwrap_or_else(|e| panic!("chunk {i} of {}: {e}", chunks.len()));
            let recorded = next.chain_certified_families(PalwCertifiedLaneV1::Attempt).len();
            if i + 1 < chunks.len() {
                assert_eq!(recorded, 0, "nothing is certified before the group completes");
            } else {
                assert_eq!(recorded, 1, "the family is recorded in the block that completes the group");
                assert_eq!(
                    next.chain_certified_families(PalwCertifiedLaneV1::Attempt)[0].family_id,
                    PalwRcFamilyV1::Base0.family_id()
                );
            }
            state = next;
            daa += 1;
        }
    }

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn at(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: blue, subsidy: 0 }
    }

    /// **The whole on-chain path, on real drills** (ADR-0075): a family enters the state through
    /// its evidence and no other way; a class is bound to it by its own profile; a free-prompt
    /// commitment refused before the binding is admitted after it; and a weightless class whose
    /// attempt-lane family is certified later is seated at the floor. Every delta reverts.
    #[test]
    fn certification_objects_carry_a_family_onto_the_chain_and_seat_its_class() {
        let (a16, a16_profile, a16_root) = a16_fixture_v1().expect("the A16 fixture serves its registered graph");
        let (q36, q36_profile, q36_root) = qwen36_fixture_v1().expect("the QWEN36 fixture serves its registered graph");
        let a16_class = a16_profile.shape_profile_id();
        let q36_class = q36_profile.shape_profile_id();
        let bond_outpoint = TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 };
        let pubkey = vec![7u8; 4];

        let (a16_canonical, _) = a16.job_for_anchor(h(0xF1)).expect("the A16 fixture implies a canonical job");
        let a16_leaves = kaspa_consensus_core::palw_step::step_leaf_count(&a16_profile, &a16_canonical).expect("counts");
        let (q36_canonical, _) = q36.job_for_anchor(h(0xF2)).expect("the QWEN36 fixture implies a canonical job");
        let q36_leaves = kaspa_consensus_core::palw_step::step_leaf_count(&q36_profile, &q36_canonical).expect("counts");

        // A gated network: the genesis set names nobody, so only the chain can certify.
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, a16_class, 4, 1000, 100, 800, 0)
            .unwrap()
            .with_fp_quanta(8, 64)
            .unwrap()
            .with_fp_certified_classes(std::collections::BTreeSet::new());
        let floor = params.min_grantable_share_permille();
        let registrations = vec![
            Obj::ClassRegistered {
                class_id: a16_class,
                artifact_root: a16_root,
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: a16_leaves },
                initial_target: u128::MAX,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            // Registered WEIGHTLESS: no family covered it when it joined (ADR-0069 Decision 6).
            Obj::ClassRegistered {
                class_id: q36_class,
                artifact_root: q36_root,
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: q36_leaves },
                initial_target: u128::MAX,
                share_permille: 0,
                activation_daa: 0,
                admission: None,
            },
            Obj::BondRegistered {
                bond: PalwBondKeyV2(bond_outpoint),
                pubkey: pubkey.clone(),
                operator_pubkey: vec![21; 8],
                collateral: 100_000,
                payout_payload: h(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ];
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &params, &at(1, 100, 1), &registrations, None)
            .expect("the fixtures register");
        assert_eq!(s1.class_share_permille(&q36_class), Some(0), "the QWEN36 fixture joined weightless");

        // A real free-prompt run on the A16 fixture, committed the way the chain sees it.
        let (ids, decode) = rc_free_prompt_question_v1(PalwRcFamilyV1::Qwen25A16);
        let prompt: Vec<usize> = ids.iter().map(|t| *t as usize).collect();
        let mut job = fp_drill_job_v1(&a16_profile, &ids, decode);
        job.executor_bond = bond_outpoint;
        job.executor_pubkey = pubkey.clone();
        let run = a16.execute_free_prompt(&job, &prompt).expect("the A16 fixture runs a caller's prompt");
        let commit = Obj::FreePromptCommitted {
            claim: h(0xFC),
            class_id: a16_class,
            bond: PalwBondKeyV2(bond_outpoint),
            executor_pubkey: pubkey.clone(),
            work_leaves: run.facts.step_leaf_count,
            prompt_token_ids_hash: job.prompt_token_ids_hash,
            decode_tokens_executed: run.facts.decode_tokens_executed,
            trace_root: run.outcome.trace_root,
            output_root: run.outcome.output_root,
            execution_root: run.outcome.execution_root,
            trace_chunk_count: run.outcome.trace_chunk_count,
            trace_retention_daa: 999_999,
        };
        assert_eq!(
            apply_palw_transition_v2(&s1, &params, &at(2, 101, 2), std::slice::from_ref(&commit), None).unwrap_err(),
            PalwStateV2Error::FreePromptLaneUncertified(a16_class),
            "before any certification the gate holds"
        );

        // The family enters through its evidence — and only through evidence that grades.
        let evidence = rc_free_prompt_evidence_v1(PalwRcFamilyV1::Qwen25A16).expect("the A16 fixture drills its free-prompt lane");
        let family_object = Obj::FamilyCertified { evidence: Box::new(PalwCertificationEvidenceV1::FreePrompt(evidence.clone())) };
        let (s2, d2) = apply_palw_transition_v2(&s1, &params, &at(2, 101, 2), std::slice::from_ref(&family_object), None)
            .expect("the court grades the drill and records the family");
        let families = s2.chain_certified_families(PalwCertifiedLaneV1::FreePrompt);
        assert_eq!(families.len(), 1);
        assert_eq!(
            families[0],
            a16_fp_certificate_v1().expect("the A16 free-prompt certificate").family,
            "what the chain recorded is what the drill certifies"
        );
        assert!(
            s2.chain_certified_families(PalwCertifiedLaneV1::Attempt).is_empty(),
            "a free-prompt drill certifies the free-prompt lane only"
        );
        assert_eq!(revert_delta_v2(&s2, &d2, &params).unwrap().state_root(), s1.state_root(), "the certification delta reverts");
        assert!(matches!(
            apply_palw_transition_v2(&s2, &params, &at(3, 102, 3), &[family_object], None),
            Err(PalwStateV2Error::FamilyAlreadyCertified { lane: PalwCertifiedLaneV1::FreePrompt, .. })
        ));
        let mut tampered = evidence.clone();
        let first = tampered.evidence.vectors.first_mut().expect("the drill planted a fault");
        first.guilty = first.honest.clone();
        assert!(
            matches!(
                apply_palw_transition_v2(
                    &s2,
                    &params,
                    &at(3, 102, 3),
                    &[Obj::FamilyCertified { evidence: Box::new(PalwCertificationEvidenceV1::FreePrompt(tampered)) }],
                    None
                ),
                Err(PalwStateV2Error::CertificationRefused { lane: PalwCertifiedLaneV1::FreePrompt, .. })
            ),
            "evidence whose guilty run is an honest one certifies nothing"
        );

        // The class is bound by its own profile: another class's profile, or a lane no family
        // covers, is refused.
        let bind = |class_id: Hash64, lane: PalwCertifiedLaneV1, profile: &PalwShapeProfileV3| Obj::ClassLaneCertified {
            class_id,
            lane,
            profile: Box::new(profile.clone()),
        };
        assert_eq!(
            apply_palw_transition_v2(
                &s2,
                &params,
                &at(3, 102, 3),
                &[bind(q36_class, PalwCertifiedLaneV1::FreePrompt, &a16_profile)],
                None
            )
            .unwrap_err(),
            PalwStateV2Error::CertificationProfileIsNotTheClass { class: q36_class, derived: a16_class }
        );
        assert_eq!(
            apply_palw_transition_v2(
                &s2,
                &params,
                &at(3, 102, 3),
                &[bind(q36_class, PalwCertifiedLaneV1::FreePrompt, &q36_profile)],
                None
            )
            .unwrap_err(),
            PalwStateV2Error::NoCertifiedFamilyCovers { class: q36_class, lane: PalwCertifiedLaneV1::FreePrompt },
            "the A16 family does not reach the QWEN36 kernels"
        );
        let (s3, d3) = apply_palw_transition_v2(
            &s2,
            &params,
            &at(3, 102, 3),
            &[bind(a16_class, PalwCertifiedLaneV1::FreePrompt, &a16_profile)],
            None,
        )
        .expect("the A16 class binds to its own family");
        assert_eq!(s3.fp_lane_certification(&a16_class).map(|c| c.family_digest), Some(families[0].digest()));
        assert_eq!(revert_delta_v2(&s3, &d3, &params).unwrap().state_root(), s2.state_root());

        // And now the commitment the gate refused is admitted.
        let (s4, _) = apply_palw_transition_v2(&s3, &params, &at(4, 103, 4), &[commit], None)
            .expect("a free-prompt commitment on a chain-certified class enters the state");
        assert!(s4.claim(&h(0xFC)).is_some());

        // The attempt lane: the weightless QWEN36 class is seated once its family is certified.
        let attempt = rc_attempt_evidence_v1(PalwRcFamilyV1::Qwen36).expect("the QWEN36 fixture drills its attempt lane");
        let (s5, _) = apply_palw_transition_v2(
            &s4,
            &params,
            &at(5, 104, 5),
            &[Obj::FamilyCertified { evidence: Box::new(PalwCertificationEvidenceV1::Attempt(attempt)) }],
            None,
        )
        .expect("the court grades the attempt-lane drill");
        assert_eq!(s5.chain_certified_families(PalwCertifiedLaneV1::Attempt).len(), 1);
        let seat = bind(q36_class, PalwCertifiedLaneV1::Attempt, &q36_profile);
        let (s6, d6) = apply_palw_transition_v2(&s5, &params, &at(6, 105, 6), std::slice::from_ref(&seat), None)
            .expect("the covered class is seated");
        assert_eq!(s6.class_share_permille(&q36_class), Some(floor), "seated at the minimum grantable share");
        assert_eq!(s6.class_share_permille(&a16_class), Some(1000 - floor), "the incumbent donated it");
        assert_eq!(revert_delta_v2(&s6, &d6, &params).unwrap().state_root(), s5.state_root());
        assert_eq!(
            apply_palw_transition_v2(&s6, &params, &at(7, 106, 7), &[seat], None).unwrap_err(),
            PalwStateV2Error::ClassAlreadyWeighted { class: q36_class, share: floor }
        );
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
    /// **And the network's SET is the set this build drilled, family by family.**
    ///
    /// The root alone would let the two agree by luck of a hash; and the set is what CONSENSUS
    /// actually reads — `palw_rc_certified_families_v1` derives it without a model runtime, because
    /// a node's consensus never links one. That derivation recomputes the kernel sets from profiles
    /// and PINS the parts only a drill can measure (which graph was drilled, how many leaves were
    /// convicted). This is what keeps those pins honest: a build whose drill covers different
    /// kernels, or convicts a different number of leaves, no longer matches what the network says
    /// its court can play — and it should not be able to join quietly.
    #[test]
    fn the_committed_family_set_is_the_one_this_build_drilled() {
        super::register_builtin_certified_families_v1();
        let drilled = kaspa_consensus_core::palw_e2e_adjudicability::certified_families_v1();
        let committed = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
        assert_eq!(
            drilled.len(),
            committed.len(),
            "the network commits to {} families and this build drilled {}",
            committed.len(),
            drilled.len()
        );
        for want in &committed {
            let got = drilled
                .iter()
                .find(|f| f.family_id == want.family_id)
                .unwrap_or_else(|| panic!("this build drilled no family {}", want.family_id));
            assert_eq!(got, want, "the drilled family and the committed one differ");
        }
    }

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

#[cfg(test)]
mod exported_evidence_tests {
    use super::*;

    /// **A family can be certified by a machine that never ran it** (ADR-0069 Decision 3, the half
    /// that was missing).
    ///
    /// `drill_family_evidence_v1` needs a live backend, and the model tiers need tens of gigabytes
    /// of weights. Running that at build time would make `court_e2e_root` depend on which artifacts
    /// a node happens to hold — the exact order- and environment-dependence the root is pinned to
    /// avoid — so the model tiers had no path to a certificate at all, which is why they still
    /// register weightless even though their step spaces are adjudicable.
    ///
    /// This is that path: drill once where the weights are, write the vectors down, and grade them
    /// anywhere. The floor stands in for a model tier here because it is the family that needs no
    /// files, which is what lets the ROUND TRIP be tested rather than described.
    ///
    /// The property is asymmetric on purpose and the test asserts both halves: evidence may be
    /// deserialized by anyone, and evidence that does not convict still does not certify. If those
    /// were not both true this would be a hole rather than a mechanism.
    #[test]
    fn exported_vectors_certify_without_the_model_that_produced_them() {
        use kaspa_consensus_core::palw_e2e_adjudicability::{PalwE2eDrillEvidenceV1, certify_e2e_family_v1};

        let court =
            kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                .expect("shipped court");
        let entry = crate::classes::canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor");
        let root = crate::rc::palw_rc_base0_artifact_root_v1().expect("pinned");
        let resolved = crate::classes::resolve_class_v1(&court, entry.class_id(), root, &[]).expect("the floor resolves");
        let profile = resolved.profile.clone();
        let backend = crate::backend::Base0Backend::new(resolved);

        let evidence = drill_family_evidence_v1(base0_family_id_v1(), &backend, &profile, root, Hash64::from_u64_word(0xE7))
            .expect("the floor drills");

        // Written down and read back — the step a model tier's vectors would take through a file.
        let bytes = borsh::to_vec(&evidence).expect("evidence serializes");
        let restored: PalwE2eDrillEvidenceV1 = borsh::from_slice(&bytes).expect("and reads back");
        assert_eq!(restored, evidence, "the round trip is lossless, or a shipped vector is not what was drilled");

        // Graded with no backend in scope at all: this is the whole claim.
        let from_export = certify_e2e_family_v1(&restored).expect("recorded vectors certify");
        let from_live = certify_e2e_family_v1(&evidence).expect("so do the live ones");
        assert_eq!(from_export.family_digest, from_live.family_digest, "one family, whichever side of the file it was graded on");

        // **And the seal is not weakened.** Evidence is readable by anyone; a certificate is still
        // only what the adjudicator's verdicts produce. Corrupt one planted fault so the guilty run
        // no longer convicts, and the same deserialization path must refuse.
        let mut tampered = restored.clone();
        let vector = tampered.vectors.first_mut().expect("the drill planted at least one fault");
        vector.guilty = vector.honest.clone();
        let bytes = borsh::to_vec(&tampered).expect("tampered evidence still serializes");
        let tampered: PalwE2eDrillEvidenceV1 = borsh::from_slice(&bytes).expect("and still reads back");
        assert!(
            certify_e2e_family_v1(&tampered).is_err(),
            "evidence whose guilty run is an honest one must not certify — the grader is what the certificate means"
        );
    }

    /// **Recorded vectors certify the graph they were drilled under, and no other.**
    ///
    /// The certificate's kernel set, covering and drilled class id are read off `evidence.profile`;
    /// the verdicts are read off each vector's own binding. Before the certifier bound the two, a
    /// drill of one graph could be filed as evidence for another with the same leaf count — a
    /// kernel id moves the class id and nothing else — and the resulting certificate would vouch
    /// for kernels no court ever re-executed. The consensus path never consumed a certificate (it
    /// reads the pinned family set), so this was the mechanism's own promise failing rather than
    /// the chain's; the mechanism is what this ADR is.
    ///
    /// The substituted profile is well-formed, statically adjudicable, and enumerates the SAME
    /// step space (only the epsilon differs), so every check the certifier ran before this one
    /// passes — which is what makes the refusal below the graph binding and nothing else.
    #[test]
    fn exported_vectors_do_not_certify_a_graph_they_were_not_drilled_under() {
        use kaspa_consensus_core::palw_e2e_adjudicability::{PalwE2eError, certify_e2e_family_v1};

        let court =
            kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                .expect("shipped court");
        let entry = crate::classes::canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor");
        let root = crate::rc::palw_rc_base0_artifact_root_v1().expect("pinned");
        let resolved = crate::classes::resolve_class_v1(&court, entry.class_id(), root, &[]).expect("the floor resolves");
        let profile = resolved.profile.clone();
        let backend = crate::backend::Base0Backend::new(resolved);
        let evidence = drill_family_evidence_v1(base0_family_id_v1(), &backend, &profile, root, Hash64::from_u64_word(0xE8))
            .expect("the floor drills");
        certify_e2e_family_v1(&evidence).expect("the honest pairing certifies");

        // Another class: same tables, same tiling, same leaf count — a different graph by id.
        let mut other = evidence.profile.clone();
        other.base0_rms_eps_q += 1;
        other.validate_shape().expect("still well-formed");
        assert_ne!(other.shape_profile_id(), evidence.profile.shape_profile_id());
        assert_eq!(
            kaspa_consensus_core::palw_step::step_leaf_count(&other, &evidence.vectors[0].honest.binding.job_context).expect("counts"),
            evidence.vectors[0].honest.binding.step_leaf_count,
            "the substituted graph enumerates the same space, so the leaf-count check alone cannot tell them apart"
        );
        let filed_under_another_graph = PalwE2eDrillEvidenceV1 { profile: other, ..evidence.clone() };
        match certify_e2e_family_v1(&filed_under_another_graph) {
            Err(PalwE2eError::VectorIsAboutAnotherGraph { leaf, drilled, vector }) => {
                assert_eq!(leaf, evidence.vectors[0].leaf_index);
                assert_eq!(drilled, filed_under_another_graph.profile.shape_profile_id());
                assert_eq!(vector, evidence.profile.shape_profile_id());
            }
            other => panic!("evidence filed under a graph its vectors were not drilled under must be refused by name: {other:?}"),
        }
    }
}

#[cfg(test)]
mod registered_class_tests {
    use kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1;
    use kaspa_consensus_core::palw_e2e_adjudicability::{certified_families_v1, family_certified_for_kernels_v1};
    use kaspa_consensus_core::palw_step::PalwShapeProfileV3;

    fn covered(profile: &PalwShapeProfileV3) -> bool {
        super::register_builtin_certified_families_v1();
        family_certified_for_kernels_v1(&reachable_kernels_v1(profile)).is_some()
    }

    /// **The class a chain registers, the class a node dispatches on, and the class a drill
    /// certified are one class** (ADR-0069).
    ///
    /// Three modules derive that id independently — the registration builder, the SDK's lineage
    /// table, and this crate's drill — and nothing but this test compares them. The failure mode is
    /// specific and has happened here before: a registration naming an id no lineage serves is a
    /// class no node can run, and it looks like "my producer makes no blocks" with nothing in any
    /// log connecting the two.
    ///
    /// The pairing is also not obvious, which is why it is pinned rather than assumed. The hybrid's
    /// corrected row is **graph-v3** — the v2 PROJECTION over the eps-corrected GEOMETRY — and the
    /// name skips v2 because "graph-v2" is burned: that spelling reached testnet-11 first and a
    /// registered name cannot be re-pointed. Registering `qwen36_profile_v2(QWEN36_35B_A3B)`
    /// instead would produce a third id that passes every other check and that no node dispatches
    /// on.
    #[test]
    fn the_registered_model_classes_are_the_ones_this_build_serves_and_certified() {
        let court =
            kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                .expect("shipped court");

        // --- the hybrid tier: graph-v3 ---------------------------------------------------------
        let (profile, entry, _) =
            kaspa_consensus_core::palw_qwen36_profile::qwen36_registration_v3(kaspa_hashes::Hash64::from_u64_word(1), 1, 1, 1)
                .expect("the corrected hybrid registration derives");
        let row = crate::classes::qwen36_canonical_classes_v1()
            .into_iter()
            .find(|c| c.model_id == "Qwen3.6-35B-A3B/graph-v3")
            .expect("the lineage table carries the corrected row");
        assert_eq!(row.class_id(), Some(entry.class_id), "the registration and the lineage table name one class");
        assert_ne!(
            entry.class_id,
            kaspa_consensus_core::palw_qwen36_profile::qwen36_class_id_v1(),
            "and it is not the graph this build refuses to plan"
        );
        assert!(covered(&profile), "the drill's certified family covers the class the chain would register");

        // --- the dense tier: graph-v2 ----------------------------------------------------------
        let (profile, entry, _) =
            kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_registration_v2(kaspa_hashes::Hash64::from_u64_word(2), 1, 1, 1)
                .expect("the corrected dense registration derives");
        let row = crate::classes::canonical_class_by_model_id_v1(&court, "Qwen/Qwen2.5-1.5B/graph-v2")
            .expect("the lineage table carries the corrected row");
        assert_eq!(row.class_id(), entry.class_id, "the registration and the lineage table name one class");
        assert!(covered(&profile), "the drill's certified family covers the class the chain would register");

        // And the certified set really is the three families, not an accident of ordering.
        let names = super::register_builtin_certified_families_v1();
        assert_eq!(names.len(), 3, "the floor and both model tiers certify on this build: {names:?}");
        assert_eq!(certified_families_v1().len(), 3);
    }
}

#[cfg(test)]
mod permissionless_tests {
    use kaspa_consensus_core::palw_class_admission_v2::{reachable_kernels_v1, verify_class_admission_v2};
    use kaspa_consensus_core::palw_e2e_adjudicability::{certified_families_v1, family_certified_for_weight_v1};

    /// **A family nobody has drilled can still JOIN — and this build leaves room for one to exist**
    /// (ADR-0069 Decision 6).
    ///
    /// Weight is what certification buys; existence is not. The two rules that decide this were
    /// written apart and, together, closed the door completely: Decision H said an entrant takes
    /// exactly `min_grantable` (never zero), and ADR-0069 refused a nonzero grant to an uncertified
    /// family. A class reaching a catalogued-but-undrilled kernel was therefore statically
    /// adjudicable, refused weight, AND refused registration — it could not join at all, which is
    /// the opposite of what both ADRs decided.
    ///
    /// Two halves, and the first is what makes the second mean anything: there must BE kernels no
    /// family has drilled, or the whole rule is untested by construction. If a later build drills
    /// everything the adjudicator catalogs, this test starts passing vacuously and says so.
    #[test]
    fn an_uncertified_family_has_somewhere_to_stand() {
        super::register_builtin_certified_families_v1();
        let catalogued = kaspa_consensus_core::palw_step_refute::catalogued_kernel_ids_v1();
        let mut drilled = std::collections::BTreeSet::new();
        for f in certified_families_v1() {
            drilled.extend(f.kernel_ids);
        }
        let undrilled: Vec<_> = catalogued.difference(&drilled).copied().collect();
        assert!(
            !undrilled.is_empty(),
            "every catalogued kernel is drilled, so this build cannot express an uncertified family — the rule below is \
             untested rather than satisfied ({} catalogued)",
            catalogued.len()
        );

        // A class reaching one of those is covered by NO certified family: the uncertified case,
        // constructed from this build's own measurements rather than from a fixture.
        let want: std::collections::BTreeSet<_> = [undrilled[0]].into_iter().collect();
        let certified = certified_families_v1();
        let params: kaspa_consensus_core::config::params::Params =
            kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 11).into();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("testnet-11 ships a ConsensusV2 bundle");
        };
        assert!(
            family_certified_for_weight_v1(bundle.court_e2e_root, &certified, &want)
                .expect("the set matches the commitment")
                .is_none(),
            "an undrilled kernel must not be covered, or this test is about the wrong case"
        );

        // **The gate admits it weightless and refuses it weight — both, from one class.** The
        // floor's own profile stands in for the graph: what varies here is the SHARE, which is the
        // thing the two rules disagreed about.
        let court =
            kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                .expect("shipped court");
        let entry = crate::classes::canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor");
        let root = crate::rc::palw_rc_base0_artifact_root_v1().expect("pinned");
        let resolved = crate::classes::resolve_class_v1(&court, entry.class_id(), root, &[]).expect("resolves");
        let profile = resolved.profile.clone();
        let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(
            &profile,
            kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.0,
            kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.1,
        );
        assert!(
            family_certified_for_weight_v1(bundle.court_e2e_root, &certified, &reachable_kernels_v1(&profile))
                .expect("the set matches")
                .is_some(),
            "the floor is certified, which is what lets the weight-bearing half of this test be about the share"
        );

        let build = |share: u16| {
            kaspa_consensus_core::palw_class_admission_v2::palw_post_genesis_registration_v1(
                profile.clone(),
                canonical.clone(),
                root,
                share,
                1,
                1,
                0,
                kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
                    kaspa_consensus_core::tx::TransactionId::default(),
                    0,
                )),
                Vec::new(),
            )
            .expect("the object builds")
        };
        // A zero grant is a legal registration at the gate — the state ADR-0039 named and nothing
        // could reach.
        verify_class_admission_v2(bundle, &profile, &canonical, &build(0), &certified)
            .expect("a weightless registration is admissible");
        // And an uncertified family asking for weight is still refused, which is the rule the
        // weightless state exists to make survivable rather than fatal.
        assert!(
            matches!(
                verify_class_admission_v2(bundle, &profile, &canonical, &build(1), &[]),
                Err(kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError::Profile(_))
            ),
            "a certified set that does not match the network's commitment must be refused outright"
        );
    }
}

#[cfg(test)]
mod evidence_size_probe {
    use super::*;

    /// Where the bytes of a family drill go — printed, so the carriage design can be sized.
    #[test]
    fn print_evidence_size_breakdown() {
        for family in [PalwRcFamilyV1::Base0, PalwRcFamilyV1::Qwen25A16, PalwRcFamilyV1::Qwen36] {
            let ev = rc_attempt_evidence_v1(family).expect("drills");
            let total = borsh::to_vec(&ev).unwrap().len();
            let profile = borsh::to_vec(&ev.profile).unwrap().len();
            let v = &ev.vectors[0];
            let honest = borsh::to_vec(&v.honest).unwrap().len();
            let honest_profile = borsh::to_vec(&v.honest.binding.shape_profile).unwrap().len();
            let binding = borsh::to_vec(&v.honest.binding).unwrap().len();
            let openings = borsh::to_vec(&v.operand_openings).unwrap().len();
            println!(
                "{}: total={total} vectors={} profile={profile} | vector0: honest_refutation={honest} (binding={binding}, of which profile={honest_profile}) openings={openings} ({} openings)",
                family.name(),
                ev.vectors.len(),
                v.operand_openings.len()
            );
        }
    }
}
