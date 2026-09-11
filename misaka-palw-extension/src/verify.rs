//! **The verifier** (ADR-0108 Decisions 2 and 3): parse at the boundary, run the checks every kind
//! shares, dispatch to the kind, and answer with exactly one classification and the depth reached.
//!
//! The verifier follows nothing (SA-2): every byte it reads is a path the person handed it,
//! resolved inside the manifest's own directory. It reads no URL, and it reads `source.digest`
//! for nothing (SA-7).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwRegistrationTermsV2};
use misaka_palw_sdk::PalwClassSdk;

use crate::manifest::{
    PalwExtensionError, PalwExtensionKindV1, PalwExtensionManifestV1, PalwFenceRequestV1, PalwParsedManifestV1, resolve_manifest_path,
};
use crate::report::{
    PalwExtensionCheckV1, PalwExtensionClassificationV1, PalwExtensionDepthV1, PalwExtensionOutcomeV1, PalwExtensionReportV1,
};

/// ADR-0108 §8: the sentence a report carries when it judged against the genesis terms.
pub const PALW_EXTENSION_GENESIS_TERMS: &str = "genesis only — the node exposes no terms RPC";

/// The most bytes the verifier reads from any file a manifest names, other than a class artifact
/// (which the SDK's own loader bounds): a certification object is at most a few hundred KB, a DSL
/// vector at most its transformer's `max_dsl_bytes`, a profile a few KB.
pub const PALW_EXTENSION_MAX_FILE_BYTES: u64 = 64 << 20;

/// What the verifier knows about the chain it answers for.
#[derive(Clone, Debug)]
pub struct PalwExtensionEnvV1 {
    pub network_id: NetworkId,
    /// The DAA score the gate's shape is resolved at. `None` = the genesis shape (DAA 0), the way
    /// `palw-class preflight` answers offline.
    pub daa_score: Option<u64>,
    /// The chain's registration terms, where the caller could read them. `None` = the network's
    /// genesis terms (ADR-0108 §8: no terms RPC on `main`).
    pub chain_terms: Option<PalwRegistrationTermsV2>,
}

impl PalwExtensionEnvV1 {
    /// The offline env: genesis shape, genesis terms.
    pub fn genesis(network_id: NetworkId) -> Self {
        Self { network_id, daa_score: None, chain_terms: None }
    }
}

/// The network's genesis registration terms, derived from its bundle the way the virtual
/// processor derives the live ones (`palw_v2_registration_terms_impl`): the base class's pricing,
/// every genesis class id and root, no chain-certified families.
pub fn genesis_registration_terms_v1(bundle: &PalwConsensusParamsV2) -> Option<PalwRegistrationTermsV2> {
    let mut registered_class_ids = Vec::new();
    let mut registered_artifact_roots = Vec::new();
    let mut base = None;
    for object in &bundle.genesis_objects {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, slash_value_per_pwu, initial_target, .. } = object {
            registered_class_ids.push(*class_id);
            registered_artifact_roots.push(*artifact_root);
            if *class_id == bundle.base_class_id {
                base = Some((*slash_value_per_pwu, *initial_target));
            }
        }
    }
    let (slash_value_per_pwu, initial_target) = base?;
    Some(PalwRegistrationTermsV2 {
        min_grantable_share_permille: bundle.state.min_grantable_share_permille(),
        slash_value_per_pwu,
        initial_target,
        registered_class_ids,
        registered_artifact_roots,
        chain_certified_families: Vec::new(),
    })
}

/// A fence's value as the report spells it.
pub fn fence_value_string(value: Option<ForkActivation>) -> String {
    match value {
        None => "absent".to_string(),
        Some(f) if f == ForkActivation::always() => "genesis".to_string(),
        Some(f) if f == ForkActivation::never() => "never".to_string(),
        Some(f) => f.daa_score().to_string(),
    }
}

/// What a kind answers: the tier, the depth it got to, and why it stopped if it did.
#[derive(Clone, Debug)]
pub(crate) struct KindOutcomeV1 {
    pub classification: PalwExtensionClassificationV1,
    pub depth_reached: PalwExtensionDepthV1,
    pub stopped_at: Option<String>,
}

impl KindOutcomeV1 {
    pub fn at(classification: PalwExtensionClassificationV1, depth_reached: PalwExtensionDepthV1) -> Self {
        Self { classification, depth_reached, stopped_at: None }
    }

    pub fn stopped(
        classification: PalwExtensionClassificationV1,
        depth_reached: PalwExtensionDepthV1,
        why: impl Into<String>,
    ) -> Self {
        Self { classification, depth_reached, stopped_at: Some(why.into()) }
    }
}

/// Everything a kind reads and writes while it verifies.
pub(crate) struct VerifyCx<'a> {
    pub parsed: &'a PalwParsedManifestV1,
    pub manifest_dir: &'a Path,
    pub env: &'a PalwExtensionEnvV1,
    pub depth: PalwExtensionDepthV1,
    pub params: Params,
    /// The network's V2 bundle; `None` where the network has none (then no class kind can answer).
    pub bundle: Option<PalwConsensusParamsV2>,
    pub daa: u64,
    pub checks: Vec<PalwExtensionCheckV1>,
    pub recomputed: BTreeMap<String, String>,
    sdk: Option<PalwClassSdk>,
}

impl<'a> VerifyCx<'a> {
    pub fn manifest(&self) -> &'a PalwExtensionManifestV1 {
        &self.parsed.manifest
    }

    pub fn pass(&mut self, name: impl Into<String>) {
        self.checks.push(PalwExtensionCheckV1 { name: name.into(), outcome: PalwExtensionOutcomeV1::Pass });
    }

    pub fn fail(&mut self, name: impl Into<String>, reason: impl Into<String>) {
        self.checks.push(PalwExtensionCheckV1 { name: name.into(), outcome: PalwExtensionOutcomeV1::Fail(reason.into()) });
    }

    pub fn skip(&mut self, name: impl Into<String>, reason: impl Into<String>) {
        self.checks.push(PalwExtensionCheckV1 { name: name.into(), outcome: PalwExtensionOutcomeV1::Skipped(reason.into()) });
    }

    pub fn record(&mut self, name: impl Into<String>, value: impl ToString) {
        self.recomputed.insert(name.into(), value.to_string());
    }

    /// `pass` or `fail` with the reason, and the classification `Refused` for the same field —
    /// the one-line spelling of "recomputed X must equal declared Y".
    pub fn refuse(&mut self, field: impl Into<String>, reason: impl Into<String>) -> PalwExtensionClassificationV1 {
        let field = field.into();
        let reason = reason.into();
        self.fail(field.clone(), reason.clone());
        PalwExtensionClassificationV1::refused(field, reason)
    }

    /// The bundle, or the refusal a class kind gives on a network without one.
    pub fn bundle(&self) -> Result<&PalwConsensusParamsV2, PalwExtensionError> {
        self.bundle.as_ref().ok_or_else(|| {
            PalwExtensionError::field(
                "network",
                format!("{} has no PALW V2 bundle, so it has no classes to speak of", self.env.network_id),
            )
        })
    }

    /// The SDK over this network's court and prompt-ids form, built once per verification (its
    /// constructor runs the certification drill once per process — see `PalwClassSdk::builtin_v1`).
    pub fn sdk(&mut self) -> Result<&PalwClassSdk, PalwExtensionError> {
        if self.sdk.is_none() {
            let bundle = self.bundle()?;
            let sdk = PalwClassSdk::builtin_v1(
                bundle.court,
                self.params.palw_prompt_ids_form_v1(),
                self.env.network_id.to_string().into_bytes(),
            );
            self.sdk = Some(sdk);
        }
        Ok(self.sdk.as_ref().expect("just built"))
    }

    /// The terms the verdict is judged against: the caller's, or the genesis terms.
    pub fn terms(&self) -> Result<PalwRegistrationTermsV2, PalwExtensionError> {
        if let Some(terms) = &self.env.chain_terms {
            return Ok(terms.clone());
        }
        genesis_registration_terms_v1(self.bundle()?)
            .ok_or_else(|| PalwExtensionError::field("network", "the network's genesis registers no base class, so it has no terms"))
    }

    pub fn chain_terms_sentence(&self) -> String {
        match &self.env.chain_terms {
            Some(terms) => format!(
                "the chain's terms as the caller read them: {} classes, {} chain-certified families",
                terms.registered_class_ids.len(),
                terms.chain_certified_families.len()
            ),
            None => PALW_EXTENSION_GENESIS_TERMS.to_string(),
        }
    }

    /// SA-2: the path resolved inside the manifest's directory. `Ok(None)` = not there.
    pub fn resolve_path(&self, field: &str, value: &str) -> Result<Option<PathBuf>, PalwExtensionError> {
        resolve_manifest_path(field, self.manifest_dir, value)
    }

    /// The bytes of a file the manifest names, bounded. `Ok(None)` = not readable here (a depth
    /// stop, not a refusal); `Err` = the path escapes or the file is over the bound.
    pub fn read_named_file(&self, field: &str, value: &str, max_bytes: u64) -> Result<Option<(PathBuf, Vec<u8>)>, PalwExtensionError> {
        let Some(path) = self.resolve_path(field, value)? else {
            return Ok(None);
        };
        let Ok(meta) = std::fs::metadata(&path) else {
            return Ok(None);
        };
        if !meta.is_file() {
            return Ok(None);
        }
        if meta.len() > max_bytes {
            return Err(PalwExtensionError::field(
                field,
                format!("{} bytes, over the {max_bytes}-byte bound this verifier reads", meta.len()),
            ));
        }
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some((path, bytes))),
            Err(_) => Ok(None),
        }
    }

    /// The fence value this build's preset carries at `daa`, by name.
    pub fn fence_active_now(&self, name: &str) -> Option<(Option<ForkActivation>, bool)> {
        self.params.palw_fences_v1().into_iter().find(|(n, _)| *n == name).map(|(_, v)| (v, v.is_some_and(|f| f.is_active(self.daa))))
    }
}

/// **`verify_extension_v1`**: the whole of Decisions 2 and 3 for one manifest.
///
/// `manifest_dir` is the directory every relative path in the manifest is resolved against (SA-2).
/// `Err` is a boundary refusal — the document never got a tier (SA-1); `Ok` always carries exactly
/// one classification.
pub fn verify_extension_v1(
    manifest_json: &[u8],
    manifest_dir: &Path,
    env: &PalwExtensionEnvV1,
    depth: PalwExtensionDepthV1,
) -> Result<PalwExtensionReportV1, PalwExtensionError> {
    let parsed = PalwParsedManifestV1::parse(manifest_json)?;
    verify_parsed_v1(&parsed, manifest_dir, env, depth)
}

/// [`verify_extension_v1`] over a manifest already through the boundary.
pub fn verify_parsed_v1(
    parsed: &PalwParsedManifestV1,
    manifest_dir: &Path,
    env: &PalwExtensionEnvV1,
    depth: PalwExtensionDepthV1,
) -> Result<PalwExtensionReportV1, PalwExtensionError> {
    let manifest = &parsed.manifest;
    if manifest.network != env.network_id.to_string() {
        return Err(PalwExtensionError::field(
            "network",
            format!(
                "the manifest is for {} and this verifier answers for {} — a manifest is verified on the network it names",
                manifest.network, env.network_id
            ),
        ));
    }
    let params = params_for(env.network_id)?;
    let bundle = match &params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.clone()),
        _ => None,
    };
    let daa = env.daa_score.unwrap_or(0);
    let mut cx =
        VerifyCx { parsed, manifest_dir, env, depth, params, bundle, daa, checks: Vec::new(), recomputed: BTreeMap::new(), sdk: None };
    cx.record("extension_id", parsed.extension_id);
    cx.record("canonical_bytes", parsed.canonical_bytes.len());

    let this_build_ruleset = cx.params.consensus_params_id().to_string();
    cx.record("ruleset_id_this_build", &this_build_ruleset);

    // Decision 1: a mismatch is reported, and verification proceeds — a manifest written before a
    // fingerprint moved is not thereby wrong.
    match &manifest.requires.ruleset_id {
        Some(id) if *id == this_build_ruleset => cx.pass("requires.ruleset_id"),
        Some(id) => cx.fail(
            "requires.ruleset_id",
            format!("the manifest was written against {id}; this build prints {this_build_ruleset} — verified anyway"),
        ),
        None => cx.skip("requires.ruleset_id", "not given"),
    }

    let outcome = match structural_common(&mut cx) {
        Some(refused) => KindOutcomeV1::at(refused, PalwExtensionDepthV1::Structural),
        None => match manifest.kind {
            PalwExtensionKindV1::ModelClass | PalwExtensionKindV1::ContextProfile => crate::kinds::model_class::verify(&mut cx)?,
            PalwExtensionKindV1::FamilyCertification | PalwExtensionKindV1::LaneCertification => {
                crate::kinds::certification::verify(&mut cx)?
            }
            PalwExtensionKindV1::DerivedTransformer => crate::kinds::derived_transformer::verify(&mut cx)?,
            PalwExtensionKindV1::RulesetCandidate => crate::kinds::ruleset_candidate::verify(&mut cx)?,
        },
    };

    let chain_terms = cx.chain_terms_sentence();
    Ok(PalwExtensionReportV1 {
        extension_id: parsed.extension_id.to_string(),
        kind: manifest.kind,
        network: manifest.network.clone(),
        classification: outcome.classification,
        depth_requested: depth,
        depth_reached: outcome.depth_reached.min(depth),
        stopped_at: outcome.stopped_at,
        checks: cx.checks,
        recomputed: cx.recomputed,
        ruleset_id_this_build: this_build_ruleset,
        ruleset_id_manifest: manifest.requires.ruleset_id.clone(),
        chain_terms,
        daa_score: daa,
    })
}

/// **What `submit` needs to build a `ClassRegistered` through the SDK** (ADR-0108 Decision 5): the
/// class as the manifest resolved it, and the root it names. The CLI hands these to
/// `PalwClassSdk::build_post_genesis_registration` exactly as `kaspad --palw-register-class` does.
#[derive(Clone, Debug)]
pub struct PalwClassRegistrationInputsV1 {
    pub profile: kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    pub canonical_job: (u32, u32),
    pub artifact_root: kaspa_hashes::Hash64,
    /// The ledger row's model id where the class is one, else the manifest's name.
    pub model_id: String,
    /// The ledger row's lineage where the class is one, else `manifest`.
    pub lineage_id: String,
    pub needs_artifact_file: bool,
    pub in_build_table: bool,
}

/// Resolve a `model-class` / `context-profile` manifest to the inputs a registration is built from.
/// `Err` is the boundary's or a tier already decided, rendered as its field and reason.
pub fn class_registration_inputs_v1(
    parsed: &PalwParsedManifestV1,
    manifest_dir: &Path,
    env: &PalwExtensionEnvV1,
) -> Result<PalwClassRegistrationInputsV1, PalwExtensionError> {
    let manifest = &parsed.manifest;
    if !matches!(manifest.kind, PalwExtensionKindV1::ModelClass | PalwExtensionKindV1::ContextProfile) {
        return Err(PalwExtensionError::field("kind", format!("a {} manifest builds no registration", manifest.kind)));
    }
    let params = params_for(env.network_id)?;
    let bundle = match &params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.clone()),
        _ => None,
    };
    let daa = env.daa_score.unwrap_or(0);
    let mut cx = VerifyCx {
        parsed,
        manifest_dir,
        env,
        depth: PalwExtensionDepthV1::Structural,
        params,
        bundle,
        daa,
        checks: Vec::new(),
        recomputed: BTreeMap::new(),
        sdk: None,
    };
    let class = match crate::kinds::model_class::resolve_class(&mut cx)? {
        Ok(class) => class,
        Err(classification) => return Err(PalwExtensionError::field("verification", classification.summary())),
    };
    let artifact = manifest
        .artifact
        .as_ref()
        .ok_or_else(|| PalwExtensionError::field("artifact", "a class names the root its registration pins"))?;
    let artifact_root = crate::manifest::parse_hash64("artifact.root", &artifact.root)?;
    let (model_id, lineage_id, needs_artifact_file) = match &class.ledger_entry {
        Some(entry) => (entry.model_id.to_string(), entry.lineage_id.to_string(), entry.needs_artifact_file),
        None => (manifest.name.clone(), "manifest".to_string(), true),
    };
    Ok(PalwClassRegistrationInputsV1 {
        profile: class.profile.clone(),
        canonical_job: class.canonical_job,
        artifact_root,
        model_id,
        lineage_id,
        needs_artifact_file,
        in_build_table: class.ledger_entry.is_some(),
    })
}

/// This build's materialised `Params` for the network — the same `From<NetworkId>` the node and
/// the fingerprint pin read, guarded against the suffixes that constructor panics on.
pub fn params_for(network_id: NetworkId) -> Result<Params, PalwExtensionError> {
    if network_id.network_type == NetworkType::Testnet && !matches!(network_id.suffix, Some(10) | Some(11)) {
        return Err(PalwExtensionError::field("network", format!("{network_id}: this build knows testnet-10 and testnet-11")));
    }
    Ok(Params::from(network_id))
}

/// The checks every kind shares, before any kind-specific work. `Some(Refused)` ends the
/// verification at Structural.
fn structural_common(cx: &mut VerifyCx<'_>) -> Option<PalwExtensionClassificationV1> {
    let manifest = cx.manifest();
    let kind = manifest.kind;

    // Decision 5's table: the manifest must name the object its kind rides as.
    let expected = kind.admission_object();
    if manifest.admission.object != expected {
        return Some(cx.refuse(
            "admission.object",
            format!("a {kind} manifest rides as `{expected}`, not `{}` (ADR-0108 Decision 5)", manifest.admission.object),
        ));
    }
    cx.pass("admission.object");

    // `requires.fences` for every kind but a candidate: what the manifest assumes, checked against
    // this build at the DAA the shape is resolved at. A candidate's fences are its proposal and are
    // read by its own kind.
    if kind != PalwExtensionKindV1::RulesetCandidate {
        let fences: Vec<(String, PalwFenceRequestV1)> = manifest.requires.fences.iter().map(|(n, r)| (n.clone(), r.clone())).collect();
        for (name, request) in fences {
            let check = format!("requires.fences.{name}");
            match cx.fence_active_now(&name) {
                None => cx.fail(
                    check,
                    format!(
                        "this build has no fence `{name}` — a fence the build lacks needs a build before a manifest can assume it"
                    ),
                ),
                Some((value, active)) => {
                    let daa = cx.daa;
                    let this_build = fence_value_string(value);
                    match request {
                        PalwFenceRequestV1::Named(word) if word == "active" => {
                            if active {
                                cx.pass(check)
                            } else {
                                cx.fail(check, format!("assumed active; this build has it {this_build} at daa {daa}"))
                            }
                        }
                        PalwFenceRequestV1::Named(word) if word == "dormant" => {
                            if active {
                                cx.fail(check, format!("assumed dormant; this build has it {this_build} at daa {daa}"))
                            } else {
                                cx.pass(check)
                            }
                        }
                        PalwFenceRequestV1::Named(_) => {
                            if value == Some(ForkActivation::always()) {
                                cx.pass(check)
                            } else {
                                cx.fail(check, format!("assumed at genesis; this build has it {this_build}"))
                            }
                        }
                        PalwFenceRequestV1::Height(h) => {
                            if value.is_some_and(|f| f.daa_score() == h) {
                                cx.pass(check)
                            } else {
                                cx.fail(check, format!("assumed at height {h}; this build has it {this_build}"))
                            }
                        }
                    }
                }
            }
        }
    }
    None
}
