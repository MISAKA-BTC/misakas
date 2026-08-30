//! **[`PalwClassSdk`]: the one door between "a model exists" and "the chain runs it".**
//!
//! Loading an artifact, listing what this build can supply, choosing what an operator's files may
//! register, checking a registration against the admission gate BEFORE anything is signed or
//! funded, and resolving a chain-named class to an engine — every one of those used to be a
//! per-lineage arm in some consumer, and every new lineage grew each consumer by one arm. They are
//! methods here now, generic over [`PalwModelLineageV1`], and the consumers (the kaspad panel, the
//! producer's backend registry, the `palw-class` CLI) hold an SDK instead of a lineage list.

use std::path::Path;
use std::sync::Arc;

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_class_admission_v2::{palw_post_genesis_registration_v1, verify_class_admission_v2};
use kaspa_consensus_core::palw_mode_v2::{PalwClassCatalogEntryV2, PalwConsensusParamsV2, PalwCourtParamsV2};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwRegistrationTermsV2};
use kaspa_hashes::Hash64;

use crate::lineage::{PalwClassEntryV1, PalwLoadedArtifactV1, PalwModelLineageV1};

/// One registrable pairing: a class this build supplies, and the root the operator's artifact
/// derives for it. Everything a registration carries beyond this is the NETWORK's to choose
/// (share, target, slash value — from the chain's own terms), which is why the candidate does not
/// hold them.
#[derive(Clone, Debug)]
pub struct PalwRegistrationCandidateV1 {
    pub entry: PalwClassEntryV1,
    pub artifact_root: Hash64,
}

/// Why no single registration candidate stands. The texts are the panel's own, verbatim — they
/// are what operators and runbooks already grep for.
#[derive(Debug)]
pub enum PalwCandidateError {
    /// No loaded artifact matches any class of any lineage.
    NoMatch,
    /// Artifacts matched, and every matched class is already on chain.
    AllRegistered,
    /// `--palw-register-class` named something the matches do not contain.
    FilterMatchesNothing { wanted: String },
    /// More than one unregistered class matched and nothing picked.
    Ambiguous { model_ids: Vec<&'static str> },
}

impl std::fmt::Display for PalwCandidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoMatch => write!(f, "no --palw-class-artifact matches a class this build knows, so there is nothing to register"),
            Self::AllRegistered => write!(f, "every class this node's artifacts match is already registered on this chain"),
            Self::FilterMatchesNothing { wanted } => write!(
                f,
                "--palw-register-class {wanted} names no unregistered class this node's artifacts match — \
                 check the model id against the build's ledger and the artifact against the model"
            ),
            Self::Ambiguous { model_ids } => write!(
                f,
                "this node's artifacts match {} unregistered classes ({}) — name one with --palw-register-class <model-id>",
                model_ids.len(),
                model_ids.join(", ")
            ),
        }
    }
}

impl std::error::Error for PalwCandidateError {}

/// The built-in lineages, in resolution order: the dense container (floor, converted dense, A16)
/// and the Qwen3.6 mmap tier. **This list is the extension point for a new model family**: a new
/// lineage implements [`PalwModelLineageV1`], takes a row here (or arrives via
/// [`PalwClassSdk::with_lineage`] where a caller composes its own), and every consumer of the SDK
/// serves it with no further wiring.
pub fn builtin_lineages_v1() -> Vec<Arc<dyn PalwModelLineageV1>> {
    vec![Arc::new(crate::lineages::dense::DenseLineageV1), Arc::new(crate::lineages::qwen36::Qwen36LineageV1)]
}

/// See the module doc. Construction asserts the lineage set is coherent (distinct ids, at most
/// one container fallback) — the properties every method below silently relies on.
pub struct PalwClassSdk {
    lineages: Vec<Arc<dyn PalwModelLineageV1>>,
    court: PalwCourtParamsV2,
    network_id: Vec<u8>,
}

impl PalwClassSdk {
    /// The SDK over [`builtin_lineages_v1`] — what node code uses.
    pub fn builtin_v1(court: PalwCourtParamsV2, network_id: Vec<u8>) -> Self {
        Self::with_lineages(builtin_lineages_v1(), court, network_id)
    }

    /// The SDK over a caller-composed lineage set. Panics on an incoherent set, because every
    /// caller is a constructor-time path and a lineage list that two lineages both claim is a
    /// build defect, not an input.
    pub fn with_lineages(lineages: Vec<Arc<dyn PalwModelLineageV1>>, court: PalwCourtParamsV2, network_id: Vec<u8>) -> Self {
        let mut ids: Vec<&'static str> = lineages.iter().map(|l| l.lineage_id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), lineages.len(), "two lineages share a lineage id");
        let fallbacks = lineages.iter().filter(|l| l.is_container_fallback()).count();
        assert!(fallbacks <= 1, "{fallbacks} lineages claim the container-fallback slot, and files can only fall back to one");
        Self { lineages, court, network_id }
    }

    /// Add one lineage to an already-built SDK — the composition point for a lineage that lives
    /// outside this crate.
    pub fn with_lineage(mut self, lineage: Arc<dyn PalwModelLineageV1>) -> Self {
        let mut lineages = std::mem::take(&mut self.lineages);
        lineages.push(lineage);
        Self::with_lineages(lineages, self.court, std::mem::take(&mut self.network_id))
    }

    pub fn court(&self) -> &PalwCourtParamsV2 {
        &self.court
    }

    pub fn lineages(&self) -> &[Arc<dyn PalwModelLineageV1>] {
        &self.lineages
    }

    fn lineage_by_id(&self, lineage_id: &str) -> Option<&Arc<dyn PalwModelLineageV1>> {
        self.lineages.iter().find(|l| l.lineage_id() == lineage_id)
    }

    /// Every class this build can supply, across every lineage — the ledger consumers list and
    /// operators read.
    pub fn ledger(&self) -> Vec<PalwClassEntryV1> {
        self.lineages.iter().flat_map(|l| l.classes(&self.court)).collect()
    }

    /// **Load one `--palw-class-artifact` path, dispatched by the file's own magic.** The lineage
    /// that sniffs the head loads the file; a head nothing claims goes to the container-fallback
    /// lineage, whose decoder authenticates the format internally and whose refusal is the error
    /// the operator sees.
    pub fn load_artifact(&self, path: &Path) -> Result<PalwLoadedArtifactV1, String> {
        let mut head = [0u8; 8];
        {
            use std::io::Read;
            let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let n = f.read(&mut head).map_err(|e| format!("{}: {e}", path.display()))?;
            if n < 8 {
                return Err(format!("{}: shorter than any artifact magic", path.display()));
            }
        }
        let lineage = self
            .lineages
            .iter()
            .find(|l| l.sniffs(&head))
            .or_else(|| self.lineages.iter().find(|l| l.is_container_fallback()))
            .ok_or_else(|| format!("{}: no lineage in this build claims this container", path.display()))?;
        lineage.load(path)
    }

    /// Every `(entry, pairing result)` for one artifact against its own lineage's classes — the
    /// inspection view. A refusal is carried, not dropped, because "why does my file match
    /// nothing" is the question this view exists to answer.
    pub fn pairings(&self, artifact: &PalwLoadedArtifactV1) -> Vec<(PalwClassEntryV1, Result<Hash64, String>)> {
        let Some(lineage) = self.lineage_by_id(artifact.lineage_id) else {
            return Vec::new();
        };
        lineage
            .classes(&self.court)
            .into_iter()
            .map(|entry| {
                let paired = lineage.pair(&self.court, &entry, artifact);
                (entry, paired)
            })
            .collect()
    }

    /// **Every class the operator's artifacts could register on a chain with these terms** —
    /// before the already-registered-class filter, exactly as the panel enumerated (its "nothing
    /// matches" and "everything is registered" refusals are different sentences, so the raw match
    /// set has to be observable).
    ///
    /// Two rules, both scars:
    /// * an artifact whose weights are already on chain (any [`registered_weight_keys`] hit) is
    ///   not a candidate for a NEW class — the n_ctx-17 mispairing rule;
    /// * one artifact per class id, first pairing wins — with two same-shape files loaded, each
    ///   still fills each ledger entry once.
    ///
    /// [`registered_weight_keys`]: PalwModelLineageV1::registered_weight_keys
    pub fn candidate_classes(
        &self,
        holdings: &[PalwLoadedArtifactV1],
        terms: &PalwRegistrationTermsV2,
    ) -> Vec<PalwRegistrationCandidateV1> {
        let mut out: Vec<PalwRegistrationCandidateV1> = Vec::new();
        for artifact in holdings {
            let Some(lineage) = self.lineage_by_id(artifact.lineage_id) else {
                debug_assert!(false, "a holding carries lineage id {:?}, which this SDK does not hold", artifact.lineage_id);
                continue;
            };
            if lineage.registered_weight_keys(artifact).iter().any(|k| terms.registered_artifact_roots.contains(k)) {
                continue;
            }
            for entry in lineage.classes(&self.court) {
                if let Ok(artifact_root) = lineage.pair(&self.court, &entry, artifact)
                    && !out.iter().any(|seen| seen.entry.class_id() == entry.class_id())
                {
                    out.push(PalwRegistrationCandidateV1 { entry, artifact_root });
                }
            }
        }
        out
    }

    /// **The one registration this node should attempt, or the reason there is none** — the
    /// panel's selection sequence, verbatim: match, drop the already-registered, apply the
    /// operator's `--palw-register-class` filter, and refuse ambiguity rather than pick.
    pub fn registration_candidate(
        &self,
        holdings: &[PalwLoadedArtifactV1],
        terms: &PalwRegistrationTermsV2,
        wanted_model_id: Option<&str>,
    ) -> Result<PalwRegistrationCandidateV1, PalwCandidateError> {
        let mut candidates = self.candidate_classes(holdings, terms);
        if candidates.is_empty() {
            return Err(PalwCandidateError::NoMatch);
        }
        candidates.retain(|c| !terms.registered_class_ids.contains(&c.entry.class_id()));
        if candidates.is_empty() {
            return Err(PalwCandidateError::AllRegistered);
        }
        let wanted = wanted_model_id.unwrap_or("");
        if !wanted.is_empty() {
            candidates.retain(|c| c.entry.model_id == wanted);
            if candidates.is_empty() {
                return Err(PalwCandidateError::FilterMatchesNothing { wanted: wanted.to_string() });
            }
        }
        if candidates.len() > 1 {
            return Err(PalwCandidateError::Ambiguous { model_ids: candidates.iter().map(|c| c.entry.model_id).collect() });
        }
        Ok(candidates.pop().expect("length checked above"))
    }

    /// **Resolve the class the chain named into something that can run it.** `class_id` and
    /// `artifact_root` come off the class record, so they are the chain's answer; each lineage
    /// either serves it, refuses it by name, or passes. A node that cannot serve a class says so —
    /// it does not fall back to one it can, because producing or judging under a class the chain
    /// did not name is worse than not participating.
    pub fn resolve(
        &self,
        class_id: Hash64,
        artifact_root: Hash64,
        holdings: &[PalwLoadedArtifactV1],
    ) -> Result<Box<dyn PalwExecutionBackendV1>, String> {
        for lineage in &self.lineages {
            if let Some(outcome) = lineage.resolve(&self.court, class_id, artifact_root, holdings, &self.network_id) {
                return outcome;
            }
        }
        Err(format!("this node cannot serve the registered class {class_id} (artifact root {artifact_root})"))
    }

    /// **The admission gate, run BEFORE anything is signed or funded.**
    ///
    /// This builds the exact registration object the chain would judge and asks
    /// [`verify_class_admission_v2`] — shape validation, both coverage gates, the ladder, the
    /// court-cost ceilings, the PWU recount — with placeholder economics, which the gate does not
    /// read. A refusal here costs nothing; the same refusal after submission costs the carrier fee
    /// and, on a wrong pairing, a burned seat. Nothing in [`build_post_genesis_registration`]
    /// skips this.
    ///
    /// [`build_post_genesis_registration`]: Self::build_post_genesis_registration
    pub fn preflight_admission(
        &self,
        bundle: &PalwConsensusParamsV2,
        entry: &PalwClassEntryV1,
        artifact_root: Hash64,
    ) -> Result<PalwClassCatalogEntryV2, String> {
        let canonical = entry.canonical_context();
        let probe = palw_post_genesis_registration_v1(
            entry.profile.clone(),
            canonical.clone(),
            artifact_root,
            1,
            1,
            1,
            0,
            PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::default(), 0)),
            Vec::new(),
        )
        .map_err(|e| format!("this build cannot express a registration for {}: {e}", entry.model_id))?;
        verify_class_admission_v2(bundle, &entry.profile, &canonical, &probe).map_err(|e| {
            format!("the {} registration would be refused by the admission gate, so nothing was signed or funded: {e}", entry.model_id)
        })
    }

    /// **Build the post-genesis registration object for one candidate — after the gate.**
    ///
    /// Wraps `palw_post_genesis_registration_v1` with the network's own terms (an entrant joins at
    /// the minimum grantable share and the base class's pricing — a registrant naming its own
    /// would be choosing its weight), and refuses to build at all if [`Self::preflight_admission`]
    /// refuses. Call once with an empty signature to learn the object to sign over, then again
    /// with the signature — both calls run the gate, both build from the same derivation.
    pub fn build_post_genesis_registration(
        &self,
        bundle: &PalwConsensusParamsV2,
        candidate: &PalwRegistrationCandidateV1,
        terms: &PalwRegistrationTermsV2,
        activation_daa: u64,
        registrant_bond: PalwBondKeyV2,
        signature: Vec<u8>,
    ) -> Result<PalwConsensusObjectV2, String> {
        self.preflight_admission(bundle, &candidate.entry, candidate.artifact_root)?;
        palw_post_genesis_registration_v1(
            candidate.entry.profile.clone(),
            candidate.entry.canonical_context(),
            candidate.artifact_root,
            terms.min_grantable_share_permille,
            terms.initial_target,
            terms.slash_value_per_pwu,
            activation_daa,
            registrant_bond,
            signature,
        )
        .map_err(|e| format!("this node cannot build a registration for {}: {e}", candidate.entry.model_id))
    }
}
