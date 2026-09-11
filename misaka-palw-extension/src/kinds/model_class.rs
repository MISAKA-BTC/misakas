//! **`model-class` and `context-profile`** (ADR-0108 Decisions 1, 7 and 8): a class IS its profile
//! (`class_id = H(profile)`, ADR-0049 Decision G), a width is a class inside the ladder and a
//! ruleset candidate outside it, and serving needs the arm the report names.
//!
//! The gate asked is the one the acceptance path asks — `verify_class_admission_v6` at the shape
//! `palw_admission_shape_at_v1` resolves from the ruleset at the DAA the env names — through the
//! same probe the SDK's `preflight_admission_with_chain` builds. The probe is restated here rather
//! than called because the SDK returns the gate's answer as text, and mapping a refusal onto a
//! tier (a kernel outside the vocabulary is a release; a width past the ladder is a fence; a
//! job that does not count is the manifest's fault) needs the variant. `tests/model_class.rs`
//! holds the two to one verdict over every ledger row.

use std::collections::BTreeSet;

use kaspa_consensus_core::palw_class_admission_v2::{
    PalwAdmissionShapeV1, PalwClassAdmissionError, palw_admission_shape_at_v1, palw_post_genesis_registration_capped_v1,
    reachable_kernels_v1, verify_class_admission_v6,
};
use kaspa_consensus_core::palw_e2e_adjudicability::{PalwE2eFamilyV1, family_certified_for_weight_v2, palw_rc_certified_families_v1};
use kaspa_consensus_core::palw_mode_v2::{PalwClassCatalogEntryV2, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw_sdk::PalwClassEntryV1;

use crate::manifest::{PalwExtensionError, PalwExtensionKindV1, parse_hash64};
use crate::report::{PalwExtensionClassificationV1, PalwExtensionDepthV1, PalwExtensionServingV1, PalwFenceDeltaV1};
use crate::verify::{KindOutcomeV1, VerifyCx, fence_value_string};

/// The projection a `context-profile` may ask for by name.
pub const PALW_EXTENSION_PROJECT_A16_CONTEXT_ROW: &str = "a16-context-row";

/// The class a manifest resolved to, however it named it.
pub(crate) struct ResolvedClassV1 {
    pub profile: PalwShapeProfileV3,
    pub canonical_job: (u32, u32),
    /// The ledger row it is, if it is one.
    pub ledger_entry: Option<PalwClassEntryV1>,
    /// Which manifest field carried the profile — the field a profile-shaped refusal names.
    pub profile_field: &'static str,
    /// One line for the report: how the class was named.
    pub named_by: String,
}

impl ResolvedClassV1 {
    pub fn class_id(&self) -> Hash64 {
        self.profile.shape_profile_id()
    }

    pub fn canonical_context(&self) -> PalwJobContextV2 {
        kaspa_consensus_core::palw_base0_profile::rc_job_context(&self.profile, self.canonical_job.0, self.canonical_job.1)
    }
}

/// Resolve the profile the manifest names: a ledger row, a projection, or an inline profile.
/// `Err(classification)` is a tier already decided (a refusal by field, or a class this build has
/// no row for and no inline profile of).
pub(crate) fn resolve_class(
    cx: &mut VerifyCx<'_>,
) -> Result<Result<ResolvedClassV1, PalwExtensionClassificationV1>, PalwExtensionError> {
    let manifest = cx.manifest();
    let kind = manifest.kind;
    let v = &manifest.verification;
    let inline_given = v.profile_borsh_hex.is_some() || v.profile_path.is_some();
    let ways = [v.model_id.is_some(), inline_given, v.project.is_some()].iter().filter(|w| **w).count();
    if ways == 0 {
        return Ok(Err(cx.refuse(
            "verification",
            "name the class one way: `model_id` (a row of this build's ledger), `profile_borsh_hex` | `profile_path` (the profile itself), \
             or `project` + `n_ctx` (a projection)",
        )));
    }
    if ways > 1 || (v.profile_borsh_hex.is_some() && v.profile_path.is_some()) {
        return Ok(Err(cx.refuse(
            "verification",
            "the class is named more than one way — a second spelling of a graph is a second thing to drift",
        )));
    }

    // A projection: the dense A16 family's row at the width, with the shipped ladder function.
    if let Some(project) = &v.project {
        if kind != PalwExtensionKindV1::ContextProfile {
            return Ok(Err(
                cx.refuse("verification.project", "a projection names a width, which is a `context-profile` manifest (Decision 7)")
            ));
        }
        if project != PALW_EXTENSION_PROJECT_A16_CONTEXT_ROW {
            return Ok(Err(cx.refuse(
                "verification.project",
                format!("`{project}` is not a projection this build has (`{PALW_EXTENSION_PROJECT_A16_CONTEXT_ROW}`)"),
            )));
        }
        let Some(n_ctx) = v.n_ctx else {
            return Ok(Err(cx.refuse("verification.n_ctx", "a projection needs the width")));
        };
        let profile = match kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v1(n_ctx) {
            Ok(profile) => profile,
            Err(e) => return Ok(Err(cx.refuse("verification.n_ctx", format!("the A16 row does not project at {n_ctx}: {e}")))),
        };
        cx.pass("verification.project");
        let canonical_job = v.canonical_job.unwrap_or(kaspa_consensus_core::palw_qwen25_profile::QWEN25_A16_CANONICAL);
        let class_id = profile.shape_profile_id();
        let ledger_entry = cx.sdk()?.ledger().into_iter().find(|e| e.class_id() == class_id);
        return Ok(Ok(ResolvedClassV1 {
            profile,
            canonical_job,
            ledger_entry,
            profile_field: "verification.n_ctx",
            named_by: format!("{PALW_EXTENSION_PROJECT_A16_CONTEXT_ROW} at n_ctx {n_ctx}"),
        }));
    }

    // A ledger row, by model id — at its own width, or (context-profile) at a tabled width.
    if let Some(model_id) = &v.model_id {
        let model_id = model_id.clone();
        let n_ctx = v.n_ctx;
        let ledger = cx.sdk()?.ledger();
        let Some(entry) = ledger.iter().find(|e| e.model_id == model_id).cloned() else {
            cx.fail("verification.model_id", format!("`{model_id}` is not a row of this build's ledger"));
            return Ok(Err(PalwExtensionClassificationV1::node_extension(format!(
                "class `{model_id}` in this build's ledger — a build that carries the row can verify it, or the manifest can carry the \
                 profile itself (verification.profile_borsh_hex | profile_path) and be verified here"
            ))));
        };
        cx.pass("verification.model_id");
        let (profile, named_by) = match (kind, n_ctx) {
            (PalwExtensionKindV1::ContextProfile, Some(n)) if n != entry.profile.n_ctx => {
                let court = *cx.sdk()?.court();
                match misaka_palw_base0::classes::a16_ladder_row_v1(&court, n, Some(&model_id)) {
                    Ok((profile, row)) => (profile, format!("{model_id} at the tabled width {n} (row {row})")),
                    Err(e) => {
                        return Ok(Err(cx.refuse(
                            "verification.n_ctx",
                            format!(
                                "no tabled row of `{model_id}` is at width {n} ({e:?}); a width no row spells is a projection \
                                 (`project`) or an inline profile"
                            ),
                        )));
                    }
                }
            }
            (_, Some(n)) if n != entry.profile.n_ctx => {
                return Ok(Err(cx.refuse(
                    "verification.n_ctx",
                    format!(
                        "`{model_id}` is defined at n_ctx {}, not {n}; a width beside a row is a second spelling of the graph",
                        entry.profile.n_ctx
                    ),
                )));
            }
            _ => (entry.profile.clone(), format!("{model_id} (ledger row, n_ctx {})", entry.profile.n_ctx)),
        };
        let canonical_job = v.canonical_job.unwrap_or(entry.canonical_job);
        let class_id = profile.shape_profile_id();
        let ledger_entry = ledger.into_iter().find(|e| e.class_id() == class_id);
        return Ok(Ok(ResolvedClassV1 { profile, canonical_job, ledger_entry, profile_field: "verification.model_id", named_by }));
    }

    // The profile itself.
    let (field, bytes) = if let Some(hex) = &v.profile_borsh_hex {
        let mut bytes = vec![0u8; hex.len() / 2];
        if faster_hex::hex_decode(hex.as_bytes(), &mut bytes).is_err() {
            return Ok(Err(cx.refuse("verification.profile_borsh_hex", "not hex")));
        }
        ("verification.profile_borsh_hex", bytes)
    } else {
        let path = v.profile_path.clone().expect("one of the two inline forms is given");
        match cx.read_named_file("verification.profile_path", &path, 1 << 20)? {
            Some((_, bytes)) => ("verification.profile_path", bytes),
            None => {
                cx.fail("verification.profile_path", format!("{path}: not readable on this machine"));
                return Ok(Err(PalwExtensionClassificationV1::node_extension(format!(
                    "the profile file verification.profile_path ({path}) — not readable on this machine, so the class id cannot be recomputed here"
                ))));
            }
        }
    };
    let profile: PalwShapeProfileV3 = match borsh::from_slice(&bytes) {
        Ok(profile) => profile,
        Err(e) => return Ok(Err(cx.refuse(field, format!("not the borsh of a PalwShapeProfileV3: {e}")))),
    };
    if let Err(e) = profile.validate_shape() {
        return Ok(Err(cx.refuse(field, format!("the profile is not a valid graph: {e}"))));
    }
    cx.pass(field);
    if let Some(n) = v.n_ctx
        && n != profile.n_ctx
    {
        return Ok(Err(cx.refuse("verification.n_ctx", format!("the profile is at n_ctx {}, not {n}", profile.n_ctx))));
    }
    let class_id = profile.shape_profile_id();
    let ledger_entry = cx.sdk()?.ledger().into_iter().find(|e| e.class_id() == class_id);
    let canonical_job = match (v.canonical_job, &ledger_entry) {
        (Some(job), _) => job,
        (None, Some(entry)) => entry.canonical_job,
        (None, None) => {
            return Ok(Err(cx.refuse(
                "verification.canonical_job",
                "an inline profile of a class no ledger row of this build carries needs `[prefill, decode]` — the job the class is paid per",
            )));
        }
    };
    let named_by = format!("an inline profile ({field}), n_ctx {}", profile.n_ctx);
    Ok(Ok(ResolvedClassV1 { profile, canonical_job, ledger_entry, profile_field: field, named_by }))
}

/// Why the probe could not answer, or what the gate answered.
pub(crate) enum ProbeRefusalV1 {
    /// `family_certified_for_weight_v2` could not price the class (a root this build does not certify).
    Price(String),
    /// The registration object could not be expressed against the ruleset's ladder.
    Express(PalwClassAdmissionError),
    /// The gate's own refusal.
    Gate(PalwClassAdmissionError),
}

/// **The SDK's `preflight_admission_with_chain`, with the variant kept.** Same probe, same share
/// rule (a class no certified family covers asks for 0 and joins weightless), same ladder (the
/// bundle's), same gate at the same shape.
pub(crate) fn admission_probe_v1(
    bundle: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    artifact_root: Hash64,
    chain_certified: &[PalwE2eFamilyV1],
    shape: &PalwAdmissionShapeV1,
) -> Result<(PalwClassCatalogEntryV2, bool), ProbeRefusalV1> {
    let certified = palw_rc_certified_families_v1();
    let prosecutable =
        family_certified_for_weight_v2(bundle.court_e2e_root, &certified, chain_certified, &reachable_kernels_v1(profile))
            .map_err(|e| ProbeRefusalV1::Price(e.to_string()))?
            .is_some();
    let probe = palw_post_genesis_registration_capped_v1(
        profile.clone(),
        canonical.clone(),
        artifact_root,
        if prosecutable { 1 } else { 0 },
        1,
        1,
        0,
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::default(), 0)),
        Vec::new(),
        bundle.court.max_step_leaf_count(),
    )
    .map_err(ProbeRefusalV1::Express)?;
    let entry =
        verify_class_admission_v6(bundle, profile, canonical, &probe, &certified, chain_certified, shape.ladder, shape.court, false)
            .map_err(ProbeRefusalV1::Gate)?;
    Ok((entry, prosecutable))
}

/// Which of the ruleset's walls a width hits first (Decision 7).
fn wall_of(err: &PalwClassAdmissionError) -> Option<&'static str> {
    match err {
        PalwClassAdmissionError::DeeperThanTheLadder { .. } => Some("the ladder"),
        PalwClassAdmissionError::CourtCostExceedsCeiling { .. } => Some("the court's cost shape"),
        PalwClassAdmissionError::CanonicalFootprintUnderTheRow { .. } => Some("the row's canonical footprint floor"),
        PalwClassAdmissionError::CourtWindowTooShort { .. } => Some("the court window"),
        _ => None,
    }
}

/// The gate's refusal, translated into a tier (Decisions 2 and 7; ADR-0067 for a kernel).
fn classify_gate_refusal(
    cx: &mut VerifyCx<'_>,
    class: &ResolvedClassV1,
    err: &PalwClassAdmissionError,
) -> PalwExtensionClassificationV1 {
    let kind = cx.manifest().kind;
    let field = class.profile_field;
    if let Some(wall) = wall_of(err) {
        return match kind {
            PalwExtensionKindV1::ContextProfile => {
                let (value, _) = cx.fence_active_now("palw_context_ladder").unwrap_or((None, false));
                PalwExtensionClassificationV1::RulesetChange {
                    fences: vec![PalwFenceDeltaV1 {
                        name: "palw_context_ladder".to_string(),
                        requested: "?".to_string(),
                        this_build: fence_value_string(value),
                    }],
                    would_print: None,
                    flag_day: true,
                    reason: format!(
                        "the width {} hits {wall} first: {err} — outside the ladder a width is a release (Decision 7)",
                        class.profile.n_ctx
                    ),
                }
            }
            _ => cx.refuse(field, format!("the class exceeds {wall} of this ruleset: {err}")),
        };
    }
    match err {
        PalwClassAdmissionError::CoverageGap => {
            let reachable = reachable_kernels_v1(&class.profile);
            let catalogued = kaspa_consensus_core::palw_step_refute::catalogued_kernel_ids_v1();
            let outside: Vec<String> = reachable.difference(&catalogued).map(|k| k.to_string()).collect();
            let reason = if outside.is_empty() {
                match kaspa_consensus_core::palw_catalog_coverage::verify_profile_coverage_v1(&class.profile) {
                    Err(e) => format!(
                        "every kernel is catalogued, but the adjudicator cannot serve a node at its shape: {e} — a court that can is a release"
                    ),
                    Ok(()) => format!("{err} — a court that can is a release"),
                }
            } else {
                format!(
                    "kernel{} {} {} outside this build's vocabulary — a new kernel is a release (ADR-0067)",
                    if outside.len() == 1 { "" } else { "s" },
                    outside.join(", "),
                    if outside.len() == 1 { "is" } else { "are" }
                )
            };
            cx.fail("admission.gate", reason.clone());
            PalwExtensionClassificationV1::RulesetChange { fences: Vec::new(), would_print: None, flag_day: true, reason }
        }
        PalwClassAdmissionError::Profile(msg) if msg.to_ascii_lowercase().contains("kernel") => {
            let reason = format!("{msg} — a kernel this build's vocabulary cannot price is a release (ADR-0067)");
            cx.fail("admission.gate", reason.clone());
            PalwExtensionClassificationV1::RulesetChange { fences: Vec::new(), would_print: None, flag_day: true, reason }
        }
        PalwClassAdmissionError::FusedAttentionNeedsTheKaryCourt => {
            let (value, _) = cx.fence_active_now("palw_kary_court").unwrap_or((None, false));
            let reason = format!("{err} — the k-ary court is a fence (ADR-0082)");
            cx.fail("admission.gate", reason.clone());
            PalwExtensionClassificationV1::RulesetChange {
                fences: vec![PalwFenceDeltaV1 {
                    name: "palw_kary_court".to_string(),
                    requested: "active".to_string(),
                    this_build: fence_value_string(value),
                }],
                would_print: None,
                flag_day: true,
                reason,
            }
        }
        PalwClassAdmissionError::NotEndToEndCertified { .. } => {
            // Unreachable through the probe (it asks for share 0 when no family covers the class)
            // and kept honest anyway: weightless is a fact, not a refusal.
            cx.pass("admission.gate");
            expressible(cx, class, true, false, None)
        }
        PalwClassAdmissionError::ClassIsNotDerived
        | PalwClassAdmissionError::PwuPerInferenceMismatch { .. }
        | PalwClassAdmissionError::CanonicalDeeperThanWorstCase { .. } => cx.refuse("verification.canonical_job", err.to_string()),
        _ => cx.refuse(field, err.to_string()),
    }
}

fn expressible(
    cx: &VerifyCx<'_>,
    class: &ResolvedClassV1,
    weightless: bool,
    already_registered: bool,
    would_be_refused: Option<String>,
) -> PalwExtensionClassificationV1 {
    let in_build_table = class.ledger_entry.is_some();
    PalwExtensionClassificationV1::Expressible {
        admission_object: cx.manifest().kind.admission_object().to_string(),
        would_be_refused,
        serving: Some(PalwExtensionServingV1 { in_build_table, needs_chain_classes_arm: !in_build_table }),
        weightless,
        already_registered,
    }
}

pub(crate) fn verify(cx: &mut VerifyCx<'_>) -> Result<KindOutcomeV1, PalwExtensionError> {
    use PalwExtensionDepthV1::{Full, Structural, Vectors};
    let bundle = cx.bundle()?.clone();
    let manifest = cx.manifest();

    // ---- Structural: the identity, recomputed from what the manifest carries -----------------
    let class = match resolve_class(cx)? {
        Ok(class) => class,
        Err(classification) => return Ok(KindOutcomeV1::at(classification, Structural)),
    };
    let class_id = class.class_id();
    cx.record("class_id", class_id);
    cx.record("n_ctx", class.profile.n_ctx);
    cx.record("canonical_job", format!("{}/{}", class.canonical_job.0, class.canonical_job.1));
    cx.record("named_by", &class.named_by);
    let declared = parse_hash64("declares.object_id", &manifest.declares.object_id)?;
    if declared != class_id {
        return Ok(KindOutcomeV1::at(
            cx.refuse(
                "declares.object_id",
                format!("the profile hashes to {class_id}, not to the declared {declared} — a class id is derived, never declared"),
            ),
            Structural,
        ));
    }
    cx.pass("declares.object_id");
    if let Some(expected) = manifest.verification.expected.get("class_id") {
        let expected = parse_hash64("verification.expected.class_id", expected)?;
        if expected != class_id {
            return Ok(KindOutcomeV1::at(cx.refuse("verification.expected.class_id", format!("recomputed {class_id}")), Structural));
        }
        cx.pass("verification.expected.class_id");
    }

    let reachable = reachable_kernels_v1(&class.profile);
    cx.record("reachable_kernels", reachable.len());
    // ADR-0067: a kernel outside this build's adjudication table is a release, and the question is
    // answerable from the profile alone — so it is answered here, before the gate would fold it
    // into `CoverageGap`.
    let catalogued = kaspa_consensus_core::palw_step_refute::catalogued_kernel_ids_v1();
    let outside: Vec<String> = reachable.difference(&catalogued).map(|k| k.to_string()).collect();
    if !outside.is_empty() {
        let reason = format!(
            "kernel{} {} {} outside this build's vocabulary — a new kernel is a release (ADR-0067)",
            if outside.len() == 1 { "" } else { "s" },
            outside.join(", "),
            if outside.len() == 1 { "is" } else { "are" }
        );
        cx.fail("kernel_vocabulary", reason.clone());
        return Ok(KindOutcomeV1::at(
            PalwExtensionClassificationV1::RulesetChange { fences: Vec::new(), would_print: None, flag_day: true, reason },
            Structural,
        ));
    }
    cx.pass("kernel_vocabulary");
    if !manifest.requires.kernel_ids.is_empty() {
        let mut listed = BTreeSet::new();
        for (i, id) in manifest.requires.kernel_ids.iter().enumerate() {
            listed.insert(parse_hash64(&format!("requires.kernel_ids[{i}]"), id)?);
        }
        if listed != reachable {
            return Ok(KindOutcomeV1::at(
                cx.refuse(
                    "requires.kernel_ids",
                    format!(
                        "the profile reaches {} kernels and the manifest lists {} — the set must be the profile's own",
                        reachable.len(),
                        listed.len()
                    ),
                ),
                Structural,
            ));
        }
        cx.pass("requires.kernel_ids");
    }

    let Some(artifact) = &manifest.artifact else {
        return Ok(KindOutcomeV1::at(
            cx.refuse("artifact", "a class names the root its registration pins (`artifact.root`)"),
            Structural,
        ));
    };
    let root = parse_hash64("artifact.root", &artifact.root)?;
    if let Some(expected) = manifest.verification.expected.get("artifact_root") {
        let expected = parse_hash64("verification.expected.artifact_root", expected)?;
        if expected != root {
            return Ok(KindOutcomeV1::at(
                cx.refuse("verification.expected.artifact_root", format!("artifact.root is {root}")),
                Structural,
            ));
        }
        cx.pass("verification.expected.artifact_root");
    }

    // Already on the chain? The genesis set and the terms the caller read, in one place.
    let terms = cx.terms()?;
    let genesis_root = bundle.genesis_objects.iter().find_map(|o| match o {
        PalwConsensusObjectV2::ClassRegistered { class_id: id, artifact_root, .. } if *id == class_id => Some(*artifact_root),
        _ => None,
    });
    let mut already_registered = false;
    let mut would_be_refused = None;
    if let Some(genesis_root) = genesis_root {
        if genesis_root == root {
            already_registered = true;
            would_be_refused = Some(format!(
                "the class is already registered under this exact root in {}'s genesis: a second registration is DuplicateClass",
                cx.env.network_id
            ));
            cx.pass("registration.already");
        } else {
            return Ok(KindOutcomeV1::at(
                cx.refuse(
                    "artifact.root",
                    format!(
                        "the class is in {}'s genesis under root {genesis_root}, and this manifest roots to {root}: different weights",
                        cx.env.network_id
                    ),
                ),
                Structural,
            ));
        }
    } else if terms.registered_class_ids.contains(&class_id) {
        already_registered = true;
        would_be_refused = Some(
            "the class is already registered on this chain (per the terms the caller read): a second registration is DuplicateClass"
                .to_string(),
        );
        cx.pass("registration.already");
    } else {
        cx.pass("registration.new");
        // Not a chain rule: nothing in the `ClassRegistered` transition refuses a second class over
        // weights it already holds. It is the SDK's candidate rule (a node never builds one for
        // itself — the 2026-08-28 mispairing), so it is named here and the tier is left alone: a
        // limit of one path is not a verdict on the manifest.
        if terms.registered_artifact_roots.contains(&root) {
            cx.fail(
                "registration.new_weights",
                "this root is already registered under another class id — the chain admits a second class over the same weights, and \
                 the SDK never builds one for a node's own registration (the 2026-08-28 mispairing rule): make sure this is a new \
                 graph over those weights and not a mispairing",
            );
        } else {
            cx.pass("registration.new_weights");
        }
    }

    // ADR-0069 Decision 5: weight is what certification buys; a class no family covers registers
    // weightless. A fact about the class, so it is recomputed at every depth.
    let certified = palw_rc_certified_families_v1();
    let weightless =
        match family_certified_for_weight_v2(bundle.court_e2e_root, &certified, &terms.chain_certified_families, &reachable) {
            Ok(Some(family)) => {
                cx.record("covering_family", family.family_id);
                false
            }
            Ok(None) => {
                cx.record("covering_family", "none (weightless)");
                true
            }
            Err(e) => {
                return Ok(KindOutcomeV1::at(
                    cx.refuse(class.profile_field, format!("this build cannot price the class: {e}")),
                    Structural,
                ));
            }
        };
    if cx.depth == Structural {
        return Ok(KindOutcomeV1::at(expressible(cx, &class, weightless, already_registered, would_be_refused), Structural));
    }

    // ---- Vectors: the gate, at the shape the acceptance path judges under -------------------
    let canonical = class.canonical_context();
    let shape = match palw_admission_shape_at_v1(&cx.params, &bundle, &class.profile, cx.daa) {
        Ok(shape) => shape,
        Err(why) => {
            let reason = format!("this ruleset has no admission shape at daa {}: {why}", cx.daa);
            cx.fail("admission.shape", reason.clone());
            return Ok(KindOutcomeV1::at(
                PalwExtensionClassificationV1::RulesetChange { fences: Vec::new(), would_print: None, flag_day: true, reason },
                Structural,
            ));
        }
    };
    cx.record(
        "shape.court",
        match shape.court {
            Some(court) => {
                format!("arity {} / {:?} / window {}", court.dissection_arity, court.prompt_ids_form, court.window_court_daa)
            }
            None => "dormant".to_string(),
        },
    );
    cx.record("shape.ladder", if shape.ladder.is_some() { "rules" } else { "dormant" });
    cx.pass("admission.shape");
    match admission_probe_v1(&bundle, &class.profile, &canonical, root, &terms.chain_certified_families, &shape) {
        Ok((entry, _)) => {
            cx.record("pwu_per_inference", entry.canonical_step_leaf_count);
            cx.pass("admission.gate");
            // The transition's one rule the gate does not hold: the registrant bond must afford the
            // registration's exposure (`RegistrationExposureUnaffordable`). Bond state is the chain's.
            cx.skip(
                "chain.bond_affords_registration",
                "the registrant bond's collateral and exposure are chain state this verifier cannot read",
            );
        }
        Err(ProbeRefusalV1::Price(why)) => {
            return Ok(KindOutcomeV1::at(
                cx.refuse(class.profile_field, format!("this build cannot price the class: {why}")),
                Structural,
            ));
        }
        Err(ProbeRefusalV1::Express(err)) => {
            return Ok(KindOutcomeV1::at(
                cx.refuse("verification.canonical_job", format!("the registration cannot be expressed against this ruleset: {err}")),
                Structural,
            ));
        }
        Err(ProbeRefusalV1::Gate(err)) => {
            let classification = classify_gate_refusal(cx, &class, &err);
            return Ok(KindOutcomeV1::at(classification, Structural));
        }
    }
    if cx.depth == Vectors {
        return Ok(KindOutcomeV1::at(expressible(cx, &class, weightless, already_registered, would_be_refused), Vectors));
    }

    // ---- Full: the root, recomputed from the bytes the person holds --------------------------
    let derived = class.ledger_entry.as_ref().is_some_and(|e| !e.needs_artifact_file);
    if derived {
        // The floor: every node mints its artifact from the pinned seed; the root is recomputed
        // from nothing on disk.
        match misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1() {
            Ok(recomputed) => {
                cx.record("artifact_root", recomputed);
                if recomputed != root {
                    return Ok(KindOutcomeV1::at(
                        cx.refuse("artifact.root", format!("the derived class roots to {recomputed}")),
                        Vectors,
                    ));
                }
                cx.pass("artifact.root");
            }
            Err(e) => {
                return Ok(KindOutcomeV1::at(
                    cx.refuse("artifact.root", format!("this build cannot derive the floor's root: {e:?}")),
                    Vectors,
                ));
            }
        }
        return Ok(KindOutcomeV1::at(expressible(cx, &class, weightless, already_registered, would_be_refused), Full));
    }
    let Some(path) = &artifact.path else {
        cx.skip("artifact.root", "artifact.path not given — the root was not recomputed here");
        return Ok(KindOutcomeV1::stopped(
            expressible(cx, &class, weightless, already_registered, would_be_refused),
            Vectors,
            "artifact.path not given",
        ));
    };
    let Some(resolved) = cx.resolve_path("artifact.path", path)? else {
        cx.skip("artifact.root", format!("artifact.path ({path}) not readable on this machine"));
        return Ok(KindOutcomeV1::stopped(
            expressible(cx, &class, weightless, already_registered, would_be_refused),
            Vectors,
            "artifact.path not readable",
        ));
    };
    let file_len = std::fs::metadata(&resolved).map(|m| m.len()).unwrap_or(0);
    if let Some(bytes) = artifact.bytes
        && bytes != file_len
    {
        return Ok(KindOutcomeV1::at(cx.refuse("artifact.bytes", format!("the file is {file_len} bytes")), Vectors));
    }
    cx.record("artifact_bytes", file_len);
    // Which lineage claims the container, by the file's own magic — before any lineage reads the
    // body. A head nothing claims and the fallback decoder refuses is not a wrong manifest: it is a
    // container this build has no lineage for.
    let mut head = [0u8; 8];
    {
        use std::io::Read as _;
        let mut f =
            std::fs::File::open(&resolved).map_err(|e| PalwExtensionError::Internal(format!("{}: {e}", resolved.display())))?;
        let n = f.read(&mut head).map_err(|e| PalwExtensionError::Internal(format!("{}: {e}", resolved.display())))?;
        if n < 8 {
            return Ok(KindOutcomeV1::at(cx.refuse("artifact.path", "shorter than any artifact magic"), Vectors));
        }
    }
    let magic = String::from_utf8_lossy(&head).into_owned();
    let dense_magic = head == *misaka_palw_base0::artifact::BASE0_ARTIFACT_FILE_MAGIC
        || head == *misaka_palw_base0::artifact::BASE0_ARTIFACT_FILE_MAGIC_V1;
    let sniffed = cx.sdk()?.lineages().iter().any(|l| l.sniffs(&head)) || dense_magic;
    if !sniffed {
        cx.fail(
            "artifact.lineage",
            format!("no lineage of this build sniffs the container `{magic}` ({})", faster_hex::hex_string(&head)),
        );
        return Ok(KindOutcomeV1::at(
            PalwExtensionClassificationV1::node_extension(format!(
                "lineage for container `{magic}` ({})",
                faster_hex::hex_string(&head)
            )),
            Vectors,
        ));
    }
    cx.pass("artifact.lineage");
    let recomputed = if let Some(entry) = &class.ledger_entry {
        // The SDK's own pairing: the artifact against the row, the root the registration pins.
        let sdk = cx.sdk()?;
        let loaded = match sdk.load_artifact(&resolved) {
            Ok(loaded) => loaded,
            Err(why) => return Ok(KindOutcomeV1::at(cx.refuse("artifact.path", why), Vectors)),
        };
        cx.record("artifact_summary", &loaded.summary);
        // The one pairing this class needs, by the lineage that loaded the file — `pairings` would
        // pair every row of that lineage, and on the dense tier each pairing is a pass over the
        // whole artifact (measured: six rows, 67 s, for the published 1.7 GB file).
        let sdk = cx.sdk()?;
        let paired = if entry.lineage_id == loaded.lineage_id {
            sdk.lineages().iter().find(|l| l.lineage_id() == loaded.lineage_id).map(|l| l.pair(sdk.court(), entry, &loaded))
        } else {
            None
        };
        match paired {
            Some(Ok(root)) => root,
            Some(Err(why)) => {
                return Ok(KindOutcomeV1::at(cx.refuse("artifact.path", format!("does not pair with the class: {why}")), Vectors));
            }
            None => {
                return Ok(KindOutcomeV1::at(
                    cx.refuse(
                        "artifact.path",
                        format!(
                            "the file is a {} container and the class is a row of the {} lineage",
                            loaded.lineage_id, entry.lineage_id
                        ),
                    ),
                    Vectors,
                ));
            }
        }
    } else if dense_magic {
        // A class outside the ledger, in the dense container. The chain arm serves the holding
        // whose DIGEST is the registered root, or — for a court-capable profile — whose INVENTORY
        // root under that profile is (`dense_artifact_by_registered_root`, ADR-0067 Decision 6).
        // Both are recomputed, and the manifest's root must be one of them.
        let bytes = std::fs::read(&resolved).map_err(|e| PalwExtensionError::Internal(format!("{}: {e}", resolved.display())))?;
        let decoded = match misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes) {
            Ok(decoded) => decoded,
            Err(e) => {
                return Ok(KindOutcomeV1::at(cx.refuse("artifact.path", format!("not a readable dense artifact: {e}")), Vectors));
            }
        };
        drop(bytes);
        let digest = decoded.artifact_digest();
        cx.record("artifact_digest", digest);
        let inventory = if misaka_palw_base0::qwen25_a16_backend::a16_court_capable_v1(&class.profile) {
            match misaka_palw_base0::inventory::a16_inventory_v1(&decoded, &class.profile) {
                Ok(inventory) => {
                    let inventory_root = inventory.root();
                    cx.record("artifact_inventory_root", inventory_root);
                    Some(inventory_root)
                }
                Err(e) => {
                    cx.record("artifact_inventory_root", format!("none: {e:?}"));
                    None
                }
            }
        } else {
            None
        };
        if inventory == Some(root) {
            cx.record("artifact_root_form", "the inventory root under this profile — the form a court-capable row registers");
            root
        } else if digest == root {
            cx.record("artifact_root_form", "the artifact digest");
            root
        } else {
            let found = match inventory {
                Some(inventory_root) => format!(
                    "the file's inventory root under this profile is {inventory_root} and its digest is {digest} — a court-capable row registers the inventory root"
                ),
                None => format!("the file's digest is {digest}"),
            };
            return Ok(KindOutcomeV1::at(cx.refuse("artifact.root", found), Vectors));
        }
    } else {
        cx.fail("artifact.root", "a class outside this build's ledger in a mapped container: this build has no pairing rule for it");
        return Ok(KindOutcomeV1::at(
            PalwExtensionClassificationV1::node_extension(format!(
                "a pairing rule for a class outside this build's ledger in the `{magic}` container"
            )),
            Vectors,
        ));
    };
    cx.record("artifact_root", recomputed);
    if recomputed != root {
        return Ok(KindOutcomeV1::at(cx.refuse("artifact.root", format!("the artifact roots to {recomputed}")), Vectors));
    }
    cx.pass("artifact.root");
    Ok(KindOutcomeV1::at(expressible(cx, &class, weightless, already_registered, would_be_refused), Full))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    use misaka_palw_sdk::PalwClassSdk;

    /// **The probe restated here answers as the SDK's does, row for row.** Same Ok, same Err text
    /// — so the tier mapping reads the variant of the answer the SDK gives, never a different
    /// answer. Run over every ledger row on testnet-11's own shape.
    #[test]
    fn the_probe_agrees_with_the_sdks_preflight_over_every_ledger_row() {
        let params: kaspa_consensus_core::config::params::Params = NetworkId::with_suffix(NetworkType::Testnet, 11).into();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("testnet-11 ships a ConsensusV2 bundle");
        };
        let sdk = PalwClassSdk::builtin_v1(bundle.court, params.palw_prompt_ids_form_v1(), params.net.to_string().into_bytes());
        let root = Hash64::from_u64_word(0xA16);
        let mut rows = 0;
        for entry in sdk.ledger() {
            let Ok(shape) = palw_admission_shape_at_v1(&params, bundle, &entry.profile, 0) else { continue };
            let theirs = sdk.preflight_admission(bundle, &entry, root, &shape);
            let ours = admission_probe_v1(bundle, &entry.profile, &entry.canonical_context(), root, &[], &shape);
            match (theirs, ours) {
                (Ok(a), Ok((b, _))) => assert_eq!(format!("{a:?}"), format!("{b:?}"), "{}", entry.model_id),
                (Err(text), Err(ProbeRefusalV1::Gate(err))) => {
                    assert!(text.ends_with(&err.to_string()), "{}: {text} / {err}", entry.model_id)
                }
                (Err(text), Err(ProbeRefusalV1::Express(err))) => {
                    assert!(text.ends_with(&err.to_string()), "{}: {text} / {err}", entry.model_id)
                }
                (Err(text), Err(ProbeRefusalV1::Price(why))) => assert!(text.ends_with(&why), "{}: {text} / {why}", entry.model_id),
                (a, b) => panic!("{}: the SDK says {a:?} and the probe says {}", entry.model_id, b.is_ok()),
            }
            rows += 1;
        }
        assert!(rows >= 5, "the ledger names the shipped classes");
    }
}
