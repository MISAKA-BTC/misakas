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
use kaspa_consensus_core::palw_class_admission_v2::{PalwAdmissionShapeV1, palw_post_genesis_registration_capped_v1};
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
    /// **ADR-0067 Decision 5's fence.** `false` (the default) keeps the chain-registered-class
    /// arm sealed: [`Self::resolve_chain_registered`] refuses with the fence named, and nothing
    /// this SDK serves can come from a profile the build's tables do not carry. Armed only by
    /// [`Self::with_chain_classes_v1`], which a node exposes as an operator flag — never a
    /// default — until the ADR's fuzz gate has run to its stated saturation.
    chain_classes: bool,
}

/// The model a row is a revision OF: `"Qwen/Qwen2.5-1.5B/graph-v2"` → `"Qwen/Qwen2.5-1.5B"`.
///
/// The `/graph-vN` suffix is the established spelling for a corrected node table over the same
/// weights — the dense family's precedent, reused by the qwen36 lineage. Anything else is its own
/// base model, suffix look-alikes included: only a wholly numeric revision counts.
fn base_model_id(model_id: &str) -> &str {
    match model_id.rsplit_once("/graph-v") {
        Some((base, rev)) if !base.is_empty() && !rev.is_empty() && rev.bytes().all(|b| b.is_ascii_digit()) => base,
        _ => model_id,
    }
}

impl PalwClassSdk {
    /// The SDK over [`builtin_lineages_v1`] — what node code uses.
    pub fn builtin_v1(court: PalwCourtParamsV2, network_id: Vec<u8>) -> Self {
        // **The certification drill runs here, once, for whoever builds an SDK** (ADR-0069).
        //
        // The weight gate reads this build's certified family set and refuses one that does not
        // hash to the network's committed `court_e2e_root`, so an empty registry is not a neutral
        // starting state — it is a different set, and every admission answer computed from it is
        // wrong rather than conservative. Anyone holding an SDK is by construction linked against
        // the crate that can drill, so this is the earliest point where "what can this build
        // prosecute" has a true answer, and it is cheap after the first call (the drill is cached
        // in `base0_certificate_v1`).
        //
        // The node also calls the registration explicitly at startup — that call is where the
        // operator-facing log and the pin check live, and both are idempotent.
        misaka_palw_base0::e2e_drill::register_builtin_certified_families_v1();
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
        Self { lineages, court, network_id, chain_classes: false }
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
            // **Weights the chain already pinned seed no NEW class — unless the chain's own owner
            // of those bytes is this model, in which case the not-yet-registered entries are its
            // GRAPH REVISIONS and refusing them contradicts the correction path this repo already
            // shipped once (`Qwen/Qwen2.5-1.5B/graph-v2`).**
            //
            // The blanket veto was the n_ctx-17 scar: with two same-shape files loaded, the first
            // filled every sibling ledger entry before the second's turn came, and the genesis
            // 1.5B digest landed under the Coder class id. That danger is CROSS-model — an
            // artifact seeding an entry whose weights on chain belong to somebody else. A revision
            // row is the opposite case: the chain says these bytes belong to THIS model (a
            // registered class of this lineage pairs with this artifact, base model equal), and
            // the candidate is the same model under a corrected graph. Measured the day this
            // clause was added: the Qwen3.6 graph-v2 row — built precisely because the registered
            // graph misdescribes the engine — was unregistrable through this filter while the
            // CHAIN's own transition would have accepted it (acceptance refuses `DuplicateClass`
            // by id and never by root).
            let weights_on_chain =
                lineage.registered_weight_keys(artifact).iter().any(|k| terms.registered_artifact_roots.contains(k));
            let entries = lineage.classes(&self.court);
            let owner_models: Vec<&str> = if weights_on_chain {
                entries
                    .iter()
                    .filter(|e| terms.registered_class_ids.contains(&e.class_id()))
                    .filter(|e| lineage.pair(&self.court, e, artifact).is_ok())
                    .map(|e| base_model_id(e.model_id))
                    .collect()
            } else {
                Vec::new()
            };
            if weights_on_chain && owner_models.is_empty() {
                // The scar's case, intact: bytes the chain pinned under a class this artifact
                // cannot claim as its own model. Not a candidate source for anything.
                continue;
            }
            for entry in entries {
                if weights_on_chain && !owner_models.contains(&base_model_id(entry.model_id)) {
                    // Registered weights may seed only their own model's revisions — a same-shape
                    // SIBLING still pairs, and letting it through would re-open the mispairing.
                    continue;
                }
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

    /// **ADR-0067 Decision 6 tier ④: load holdings under the operator's byte bound.**
    ///
    /// A permissionless registry multiplies MODELS; it must not multiply every node's resident
    /// bytes. Registration obligates no node to hold anything, and this is where that stops being
    /// a principle and becomes a number: artifacts load in the order the operator listed them —
    /// their priority, stated by them, not inferred — and loading stops at `bound_bytes`.
    ///
    /// **What is skipped is NAMED, not silently dropped.** A node that quietly held nine of ten
    /// artifacts would declare capability for nine classes and look, to its operator, like it
    /// served ten. The skipped ones are simply not held, and a class this node does not hold is
    /// one it cannot resolve, cannot declare and will not be drawn to judge — which is
    /// declaration-first eviction arriving structurally rather than as a rule somebody has to
    /// remember to obey.
    ///
    /// `bound_bytes == 0` means unbounded, which is the behaviour every node had before this and
    /// is right for an operator running one class on a dedicated box.
    ///
    /// **The bound is over FILE bytes, which overstates a mapped artifact's resident set.** The
    /// mmap container's whole point is that the kernel's page cache holds the fraction actually
    /// read — eight of two hundred and fifty-six experts per token — so a 33.5 GiB file is never
    /// 33.5 GiB of pressure. Bounding the file anyway is the conservative direction and the only
    /// one that can be checked before the bytes are touched; an operator who knows their working
    /// set is smaller raises the bound, and the log names what a too-small one cost them.
    pub fn load_artifacts_bounded_v1(
        &self,
        paths: &[std::path::PathBuf],
        bound_bytes: u64,
    ) -> (Vec<PalwLoadedArtifactV1>, Vec<(std::path::PathBuf, String)>) {
        self.load_artifacts_bounded_with_v1(paths, bound_bytes, |path| self.load_artifact(path))
    }

    /// [`Self::load_artifacts_bounded_v1`] with "load this path" supplied by the caller.
    ///
    /// The bound — its order, its arithmetic, its message — stays one spelling here; what changes
    /// is who answers for a path the bound admits. A node whose two duties are handed the same
    /// list answers from the holding it already has before it maps anything (kaspad's
    /// `load_class_holdings_v1`: each duty loading the list for itself was two mappings and two
    /// root passes over one 33 GiB file), and a plain consumer hands in [`Self::load_artifact`].
    /// `load` is asked only for paths the bound admits, in the operator's order, and its refusal
    /// is carried into the skipped list by name exactly like the bound's own.
    pub fn load_artifacts_bounded_with_v1<F>(
        &self,
        paths: &[std::path::PathBuf],
        bound_bytes: u64,
        mut load: F,
    ) -> (Vec<PalwLoadedArtifactV1>, Vec<(std::path::PathBuf, String)>)
    where
        F: FnMut(&Path) -> Result<PalwLoadedArtifactV1, String>,
    {
        let mut held = Vec::new();
        let mut skipped = Vec::new();
        let mut bytes: u64 = 0;
        for path in paths {
            // The file's size, before it is read: a bound that only notices after loading is a
            // bound that OOMs the node it was set to protect.
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            if bound_bytes > 0 && bytes.saturating_add(size) > bound_bytes {
                skipped.push((
                    path.clone(),
                    format!(
                        "{size} bytes would take this node past its {bound_bytes}-byte artifact bound (holding {bytes});                          raise --palw-class-cache-bytes or drop a class from the list"
                    ),
                ));
                continue;
            }
            match load(path) {
                Ok(holding) => {
                    bytes = bytes.saturating_add(size);
                    held.push(holding);
                }
                Err(err) => skipped.push((path.clone(), err)),
            }
        }
        (held, skipped)
    }

    /// Arm the ADR-0067 chain-registered-class arm. A separate constructor step rather than a
    /// parameter, so every call site that arms it is greppable and deliberate.
    pub fn with_chain_classes_v1(mut self) -> Self {
        self.chain_classes = true;
        self
    }

    /// **ADR-0067 Decisions 1–2: serve a class this build's tables never heard of, from the
    /// registration the chain carries.** The caller hands in what the chain holds — the
    /// admission carriage's profile and canonical job — and gets an execution backend whose
    /// every forward walks that declaration, or a refusal that names what this build cannot
    /// serve.
    ///
    /// Fenced (Decision 5): with the fence down this refuses unconditionally, naming the fence.
    /// The checks, in refusal order:
    ///
    /// 1. the fence;
    /// 2. the profile hashes to the class id — the id IS the declaration, so a mismatch is a
    ///    wrong object, not a wrong opinion;
    /// 3. the canonical job names the same class, because the pwu this class is paid per was
    ///    counted from that job (a canonical job for another graph prices another class);
    /// 4. an artifact whose COMPUTED root is the registered root is among `holdings` — Decision 6:
    ///    possession is the operator's chosen act, so its absence is a "fetch it" error, not a
    ///    protocol fault. The root also decides the CONTAINER: it is a digest over one artifact's
    ///    own bytes, so it matches at most one holding, and that holding's lineage — dense or
    ///    mmap — is the engine that serves the declaration. Nothing infers a family from the
    ///    profile, because a profile is what is being judged;
    /// 5. the plan compiles — every declared node lands inside this build's kernel vocabulary
    ///    and none of it contradicts the held artifact's geometry (Decision 3's boundary,
    ///    surfaced with the node or the field named).
    pub fn resolve_chain_registered(
        &self,
        class_id: Hash64,
        artifact_root: Hash64,
        holdings: &[PalwLoadedArtifactV1],
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        canonical: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
    ) -> Result<Box<dyn PalwExecutionBackendV1>, String> {
        if !self.chain_classes {
            return Err(format!(
                "class {class_id} is chain-registered and this node's chain-class arm is fenced off (ADR-0067 Decision 5) — \
                 arm it deliberately once the operator accepts interpreted execution"
            ));
        }
        let declared = profile.shape_profile_id();
        if declared != class_id {
            return Err(format!("the supplied profile hashes to {declared}, not to the registered class {class_id}"));
        }
        if canonical.shape_profile_id != class_id {
            return Err(format!(
                "the canonical job names class {}, not the registered class {class_id} — it prices another graph",
                canonical.shape_profile_id
            ));
        }
        if let Some(artifact) = crate::lineages::dense::dense_artifact_by_registered_root(holdings, artifact_root, profile) {
            // **The CHAIN lane** (audit D M-6). The tokenizer refusal is waived only for the
            // seeded floor, and no dense class the chain registers is that — so a chain-registered
            // row is refused whatever `derived_seed` the file on disk carries, which neither
            // `artifact_digest` nor the inventory root covers and which an operator or a fleet
            // image can therefore flip without moving the root this arm just matched.
            let backend = misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend::from_registered_profile_in_lane_v1(
                artifact,
                self.network_id.clone(),
                profile.clone(),
                (canonical.declared_prefill_tokens, canonical.exact_decode_tokens),
                misaka_palw_base0::classes::ArtifactSourceV1::ConvertedA16,
            )?;
            return Ok(Box::new(backend));
        }
        if let Some(artifact) = crate::lineages::qwen36::qwen36_artifact_by_root(holdings, artifact_root) {
            let backend = misaka_palw_base0::qwen36_backend::Qwen36Backend::from_registered_profile(
                artifact,
                self.network_id.clone(),
                profile.clone(),
                (canonical.declared_prefill_tokens, canonical.exact_decode_tokens),
            )?;
            return Ok(Box::new(backend));
        }
        Err(format!(
            "this node holds no artifact whose digest is the registered root {artifact_root} — fetch the class's \
             artifact and load it (--palw-class-artifact); registration does not obligate possession (ADR-0067 Decision 6)"
        ))
    }

    /// **The admission gate, run BEFORE anything is signed or funded.**
    ///
    /// This builds the exact registration object the chain would judge and asks
    /// [`verify_class_admission_v3`] — shape validation, both coverage gates, the ladder, the
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
        shape: &PalwAdmissionShapeV1,
    ) -> Result<PalwClassCatalogEntryV2, String> {
        self.preflight_admission_with_chain(bundle, entry, artifact_root, &[], shape)
    }

    /// [`Self::preflight_admission`] with the chain's own certified families in scope (ADR-0075
    /// Decision 4): `chain_certified` is `PalwRegistrationTermsV2::chain_certified_families`, and a
    /// family there prices the probe's share exactly as a genesis one does.
    ///
    /// **`shape` is the court and ladder the ACCEPTANCE path judges this registration under**
    /// (`palw_admission_shape_at_v1` from the network's `Params` at the point of submission), and
    /// the probe asks `verify_class_admission_v6` with exactly those. It asked the court-less
    /// `verify_class_admission_v3` until the ADR-0082 devnet drill: the graph-v5 row was refused
    /// by name (`FusedAttentionNeedsTheKaryCourt`) on a ruleset whose fence is armed, and the
    /// panel reported "would be refused by the admission gate" for a row the gate admits. A
    /// preflight that answers for a court the chain does not run is a limit rendered as a verdict.
    pub fn preflight_admission_with_chain(
        &self,
        bundle: &PalwConsensusParamsV2,
        entry: &PalwClassEntryV1,
        artifact_root: Hash64,
        chain_certified: &[kaspa_consensus_core::palw_e2e_adjudicability::PalwE2eFamilyV1],
        shape: &PalwAdmissionShapeV1,
    ) -> Result<PalwClassCatalogEntryV2, String> {
        let canonical = entry.canonical_context();
        // The build's certified families (ADR-0069 Decision 5) — the same set the consensus gate
        // reads, so a preflight that says "this would be admitted" is answering the question the
        // chain will actually ask.
        let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
        // **The probe's share is the one this class may actually take**, not a placeholder. Every
        // other economic field here is ignored by the gate; the share is not, since ADR-0069, and a
        // probe that asked for weight on behalf of an uncertified family would report "refused" for
        // a class that is in fact perfectly registrable — weightless. That refusal reads as "your
        // model cannot join", which is the opposite of what the chain means.
        let share = if kaspa_consensus_core::palw_e2e_adjudicability::family_certified_for_weight_v2(
            bundle.court_e2e_root,
            &certified,
            chain_certified,
            &kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&entry.profile),
        )
        .map_err(|e| format!("this node cannot price a registration for {}: {e}", entry.model_id))?
        .is_some()
        {
            1
        } else {
            0
        };
        // **Counted against the RULESET's ladder** (audit D H-5b): the uncapped helper counts at
        // the executor's `2^22`, so the graph-v5 512 row could not even be EXPRESSED as an object
        // — "the canonical job does not count against this profile: job shape yields 4223328 step
        // leaves, exceeding the 4194304 cap" — while the gate this probe feeds recounts at the
        // bundle's `2^26`. The preflight has the bundle in hand, so it uses it.
        let probe = palw_post_genesis_registration_capped_v1(
            entry.profile.clone(),
            canonical.clone(),
            artifact_root,
            share,
            1,
            1,
            0,
            PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::default(), 0)),
            Vec::new(),
            bundle.court.max_step_leaf_count(),
        )
        .map_err(|e| {
            // A canonical job that does not count against this ruleset's ladder is a class this
            // ruleset would refuse, so it is the gate's answer in substance and says so in the
            // gate's words — nothing here is signed or funded either way.
            format!(
                "the {} registration cannot be expressed against this ruleset, so nothing was signed or funded: {e}",
                entry.model_id
            )
        })?;
        kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v6(
            bundle,
            &entry.profile,
            &canonical,
            &probe,
            &certified,
            chain_certified,
            shape.ladder,
            shape.court,
            false,
        )
        .map_err(|e| {
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
        shape: &PalwAdmissionShapeV1,
    ) -> Result<PalwConsensusObjectV2, String> {
        self.preflight_admission_with_chain(
            bundle,
            &candidate.entry,
            candidate.artifact_root,
            &terms.chain_certified_families,
            shape,
        )?;
        // **The share an entrant may take is a function of its own graph** (ADR-0069 Decisions 5
        // and 6). `terms` carries the chain-wide minimum, which is the right value for a class some
        // end-to-end certified family covers; a class no family covers joins WEIGHTLESS instead,
        // and the acceptance path requires exactly that value rather than the minimum. Building at
        // the minimum regardless would produce an object every node refuses — a registrant told
        // "you may not join" when what the chain means is "you may join, and earn once somebody can
        // prosecute you".
        let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
        let reachable = kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&candidate.entry.profile);
        // ADR-0075 Decision 4: genesis ∪ chain — the set the processor prices this registration by.
        let prosecutable = kaspa_consensus_core::palw_e2e_adjudicability::family_certified_for_weight_v2(
            bundle.court_e2e_root,
            &certified,
            &terms.chain_certified_families,
            &reachable,
        )
        .map_err(|e| format!("this node cannot price a registration for {}: {e}", candidate.entry.model_id))?
        .is_some();
        // The ruleset's ladder, for the reason `preflight_admission_with_chain` gives: the object a
        // registrant signs and the object the gate recounts must be counted by one number, and that
        // number is the bundle's, never the executor's constant (audit D H-5b).
        palw_post_genesis_registration_capped_v1(
            candidate.entry.profile.clone(),
            candidate.entry.canonical_context(),
            candidate.artifact_root,
            if prosecutable { terms.min_grantable_share_permille } else { 0 },
            terms.initial_target,
            terms.slash_value_per_pwu,
            activation_daa,
            registrant_bond,
            signature,
            bundle.court.max_step_leaf_count(),
        )
        .map_err(|e| format!("this node cannot build a registration for {}: {e}", candidate.entry.model_id))
    }
}

#[cfg(test)]
mod chain_arm_tests_support {
    pub(super) use super::chain_arm_tests::{class, court, fp_job, holding};
}

#[cfg(test)]
mod chain_arm_tests {
    use super::*;
    use kaspa_consensus_core::palw_base0_profile::rc_job_context;
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v2};
    use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use misaka_palw_base0::engine_a16::derived_a16_store;

    pub(super) fn court() -> PalwCourtParamsV2 {
        // The EXECUTOR's default as this harness's ladder — not the shipped one, which is
        // `palw_fp_devnet_v3::COURT_MAX_STEP_LEAVES` (2^26). Left where it is deliberately: every
        // count in these tests is taken against `court()`, so the fixture is self-consistent at
        // whatever it declares, and moving it would re-measure the SDK battery rather than test it.
        PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("a legal court")
    }

    /// A tiny dense artifact + the CORRECTED profile for its geometry — a class that exists
    /// nowhere in this build's tables, which is the whole scenario.
    pub(super) fn class() -> (std::sync::Arc<Base0ArtifactV1>, kaspa_consensus_core::palw_step::PalwShapeProfileV3) {
        let shape = Base0ShapeV1 {
            n_layers: 1,
            n_heads: 4,
            n_kv_heads: 2,
            d_head: 4,
            d_ff: 12,
            vocab: 64,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        // **It declares a tokenizer, because a CHAIN-REGISTERED class must** (audit D M-6). The
        // exemption `check_tokenizer_declared_v1` grants is a property of the lane the resolver
        // decided, not of the artifact's `derived_seed` — a flag neither `artifact_digest` nor the
        // operand inventory covers — so a fixture standing in for a registered class has to carry
        // what a registered class carries.
        let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x0067)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("sorted and unique")
            .with_tokenizer_commitment(Base0ArtifactV1::tokenizer_commitment_of(b"{\"model\":\"misaka-palw-sdk-test\"}"));
        (std::sync::Arc::new(artifact), qwen25_a16_profile_v2(small_geometry()).expect("the corrected profile builds"))
    }

    /// The one-layer test geometry, shared with the revision-row tests (which vary its `n_ctx` to
    /// mint distinct class ids cheaply).
    pub(super) fn small_geometry() -> PalwQwen25GeometryV1 {
        PalwQwen25GeometryV1 {
            layer_count: 1,
            hidden_dim: 16,
            ffn_dim: 12,
            attn_heads: 4,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            vocab_size: 64,
            n_ctx: 16,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        }
    }

    pub(super) fn holding(artifact: std::sync::Arc<Base0ArtifactV1>) -> PalwLoadedArtifactV1 {
        PalwLoadedArtifactV1::from_parts(crate::lineages::dense::DENSE_LINEAGE_ID, None, "a test holding".into(), artifact)
    }

    pub(super) fn fp_job(
        class_id: Hash64,
        n_ctx: u32,
        prompt: &[u32],
        decode: u32,
    ) -> kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3 {
        kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3 {
            version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(9),
            class_id,
            executor_bond: kaspa_consensus_core::tx::TransactionOutpoint {
                transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(1),
                index: 0,
            },
            executor_pubkey: vec![7; 8],
            operator_id: Hash64::from_u64_word(4),
            anchor_block: Hash64::from_u64_word(0xA0),
            anchor_daa: 100,
            job_nonce: [0x67; 32],
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(prompt),
            prompt_tokens: prompt.len() as u32,
            decode_token_limit: decode,
            max_context_tokens: n_ctx,
            privacy_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        }
    }

    /// **The fence holds until armed, and names itself** (ADR-0067 Decision 5). The same call
    /// that is refused sealed succeeds armed — one flag, no other difference.
    #[test]
    fn the_fence_refuses_until_armed_and_the_armed_arm_serves() {
        let (artifact, profile) = class();
        let class_id = profile.shape_profile_id();
        let root = artifact.artifact_digest();
        let canonical = rc_job_context(&profile, 4, 2);
        let holdings = vec![holding(artifact)];

        let sealed = PalwClassSdk::builtin_v1(court(), b"misaka-palw-rc".to_vec());
        let err = sealed.resolve_chain_registered(class_id, root, &holdings, &profile, &canonical).map(drop).unwrap_err();
        assert!(err.contains("fenced off"), "the refusal names the fence: {err}");

        let armed = PalwClassSdk::builtin_v1(court(), b"misaka-palw-rc".to_vec()).with_chain_classes_v1();
        let backend = armed
            .resolve_chain_registered(class_id, root, &holdings, &profile, &canonical)
            .expect("a servable chain-registered class resolves");
        assert_eq!(backend.model_id(), "PALW-A16/chain-registered");
    }

    /// **ADR-0067 security amendment SA-4, which is a note rather than a mechanism — recorded here
    /// because the note is about something not to do, and a thing not to do needs a test or it is
    /// a memory.**
    ///
    /// R-7 stands: nothing in the transition pays a `PalwPanelSeatV2`. An unpaid seat has no income
    /// to lose, so its whole stake is its bond, and ADR-0065 measured what a post-genesis bond
    /// costs — 400,000 sompi. Arming chain-registered classes FOR WEIGHT before seats are paid
    /// therefore leaves the panel as the cheapest thing on the chain to buy: a registrant funds a
    /// quorum of judges for the price of a few bonds and then judges its own class.
    ///
    /// So the amendment's requirement reduces to "arming stays off", and this is that, asserted at
    /// the only door: every SDK a node builds through the ordinary constructor refuses a
    /// chain-registered class, and only the deliberate, greppable `with_chain_classes_v1` — which
    /// exists behind an operator flag and no default — opens it.
    #[test]
    fn arming_stays_off_because_an_unpaid_panel_is_the_cheapest_thing_on_the_chain_to_buy() {
        let (artifact, profile) = class();
        let canonical = rc_job_context(&profile, 4, 2);
        let holdings = vec![holding(artifact.clone())];
        // Every ordinary construction path, including the composed one, starts sealed.
        for sdk in [
            PalwClassSdk::builtin_v1(court(), b"misaka-palw-rc".to_vec()),
            PalwClassSdk::with_lineages(builtin_lineages_v1(), court(), b"misaka-palw-rc".to_vec()),
        ] {
            let err = sdk
                .resolve_chain_registered(profile.shape_profile_id(), artifact.artifact_digest(), &holdings, &profile, &canonical)
                .map(drop)
                .unwrap_err();
            assert!(err.contains("fenced off"), "a default SDK must not serve a stranger's class for weight: {err}");
        }
    }

    /// **The interpreted backend and the table-style backend agree on the work itself.** Same
    /// artifact, same profile, one constructed as the chain arm would and one as the compiled
    /// table would: a caller's prompt must land on the same execution root and the same answer,
    /// or the two authorities ADR-0067 merges are still two authorities.
    #[test]
    fn the_chain_arm_and_the_table_path_produce_the_same_execution() {
        let (artifact, profile) = class();
        let class_id = profile.shape_profile_id();
        let root = artifact.artifact_digest();
        let canonical = rc_job_context(&profile, 4, 2);
        let holdings = vec![holding(artifact.clone())];

        let armed = PalwClassSdk::builtin_v1(court(), b"misaka-palw-rc".to_vec()).with_chain_classes_v1();
        let interpreted = armed.resolve_chain_registered(class_id, root, &holdings, &profile, &canonical).expect("resolves");
        let compiled = misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend::new(
            artifact,
            b"misaka-palw-rc".to_vec(),
            profile.clone(),
            (4, 2),
        )
        .expect("the table path compiles this class's declaration");

        let prompt_ids: Vec<u32> = vec![3, 9, 17];
        let prompt: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
        let job = fp_job(class_id, profile.n_ctx, &prompt_ids, 3);
        let a = interpreted.execute_free_prompt(&job, &prompt).expect("the chain arm runs the prompt");
        let b = compiled.execute_free_prompt(&job, &prompt).expect("the table path runs the prompt");
        assert_eq!(a.outcome.execution_root, b.outcome.execution_root, "one root, whichever authority built the engine");
        assert_eq!(a.output_token_ids, b.output_token_ids, "and one answer");
        assert_eq!(a.facts, b.facts, "and the same measured facts");
    }

    /// **Each refusal names its boundary**: a root nobody holds points at Decision 6 (fetch it —
    /// registration does not obligate possession), a profile that hashes elsewhere is a wrong
    /// object, and a declaration outside the kernel vocabulary is refused with the node named.
    #[test]
    fn the_refusals_name_their_boundaries() {
        let (artifact, profile) = class();
        let class_id = profile.shape_profile_id();
        let root = artifact.artifact_digest();
        let canonical = rc_job_context(&profile, 4, 2);
        let holdings = vec![holding(artifact)];
        let armed = PalwClassSdk::builtin_v1(court(), b"misaka-palw-rc".to_vec()).with_chain_classes_v1();

        let missing = armed
            .resolve_chain_registered(class_id, Hash64::from_u64_word(0xBAD), &holdings, &profile, &canonical)
            .map(drop)
            .unwrap_err();
        assert!(missing.contains("does not obligate possession"), "Decision 6 is the answer: {missing}");

        let wrong_id = armed
            .resolve_chain_registered(Hash64::from_u64_word(0xFACE), root, &holdings, &profile, &canonical)
            .map(drop)
            .unwrap_err();
        assert!(wrong_id.contains("hashes to"), "a wrong object, said as one: {wrong_id}");

        let mut foreign = profile.clone();
        foreign.attn_nodes[0].kernel_semantics_id =
            kaspa_consensus_core::palw_step::kernel_semantics_id_v1("a16/some-future-kernel/v9");
        let foreign_id = foreign.shape_profile_id();
        let foreign_canonical = rc_job_context(&foreign, 4, 2);
        let unserved =
            armed.resolve_chain_registered(foreign_id, root, &holdings, &foreign, &foreign_canonical).map(drop).unwrap_err();
        assert!(unserved.contains("cannot serve the registered graph"), "the kernel boundary speaks: {unserved}");
    }
}

#[cfg(test)]
mod mmap_chain_arm_tests {
    use super::chain_arm_tests::court;
    use super::*;
    use kaspa_consensus_core::palw_base0_profile::rc_job_context;
    use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_profile_v1, qwen36_profile_v2};

    /// The geometry of `qwen36_dev_fixture(4, 8)`, spelled out — the SDK cannot reach the base0
    /// crate's test-only helper, and the interpreter's own differentials pin this mapping.
    fn fixture_geometry() -> PalwQwen36GeometryV1 {
        PalwQwen36GeometryV1 {
            layer_count: 4,
            full_attention_interval: 4,
            hidden_dim: 32,
            attn_heads: 4,
            attn_kv_heads: 2,
            attn_head_dim: 16,
            rope_dims: 4,
            rope_freq_base_bits: 0x4B18_9680,
            gdn_k_heads: 2,
            gdn_v_heads: 4,
            gdn_head_dim: 8,
            gdn_conv_kernel: 4,
            n_experts: 8,
            experts_per_token: 4,
            moe_dim: 16,
            shared_dim: 16,
            attn_output_gate: 1,
            vocab_size: 64,
            n_ctx: 8,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 512,
        }
    }

    /// **The chain arm serves the mmap container, and its execution is the compiled path's.**
    /// The same fence, the same refusal order, and the same promise the dense arm made: one
    /// anchor, two authorities, one set of roots — or ADR-0067's merge is still two authorities.
    #[test]
    fn the_chain_arm_serves_the_mmap_family_and_matches_the_compiled_path() {
        let artifact = std::sync::Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8));
        let profile = qwen36_profile_v2(fixture_geometry()).expect("the fixture geometry projects");
        let class_id = profile.shape_profile_id();
        let root = artifact.artifact_root();
        let canonical = rc_job_context(&profile, 4, 2);
        let holdings = vec![crate::lineages::qwen36::holding_from_artifact(artifact.clone(), None)];

        let sealed = PalwClassSdk::builtin_v1(court(), b"misaka-palw-rc".to_vec());
        let err = sealed.resolve_chain_registered(class_id, root, &holdings, &profile, &canonical).map(drop).unwrap_err();
        assert!(err.contains("fenced off"), "the mmap arm sits behind the same fence: {err}");

        let armed = PalwClassSdk::builtin_v1(court(), b"misaka-palw-rc".to_vec()).with_chain_classes_v1();
        let interpreted =
            armed.resolve_chain_registered(class_id, root, &holdings, &profile, &canonical).expect("a held mapping serves");
        assert_eq!(interpreted.model_id(), "PALW-QWEN36/chain-registered");

        // The ledger-compiled authority, handed the graph it serves. `::new` self-arms from THIS
        // BUILD's own class ledger, and this fixture class is deliberately not a ledger row — a
        // node whose tables never heard of the class would fall back to the legacy composite and
        // the comparison below would be against an authority that is not serving this graph at
        // all. `with_class_profile` is the same compiled authority for a caller that holds the
        // declaration, which is what a resolved ledger row is.
        let compiled = misaka_palw_base0::qwen36_backend::Qwen36Backend::with_class_profile(
            artifact,
            "Qwen3.6-fixture",
            (4, 2),
            profile.clone(),
            b"misaka-palw-rc".to_vec(),
        );
        assert_eq!(compiled.class_profile_id(), class_id, "the compiled authority serves the class the chain named");
        let anchor = Hash64::from_u64_word(0x67);
        let (job_i, prompt_i) = interpreted.job_for_anchor(anchor).expect("a job");
        let (job_c, prompt_c) = compiled.job_for_anchor(anchor).expect("a job");
        assert_eq!(prompt_i, prompt_c, "the prompt is a pure function of the anchor");
        assert_eq!(job_i.context_hash(), job_c.context_hash(), "one job, whichever authority derived it");
        let a = interpreted.execute(&job_i, &prompt_i).expect("the chain arm runs");
        let b = compiled.execute(&job_c, &prompt_c).expect("the compiled path runs");
        assert_eq!(a.execution_root, b.execution_root, "one root, whichever authority built the engine");
        assert_eq!(a.trace_root, b.trace_root);
        assert_eq!(a.output_root, b.output_root);
        assert_eq!(a.material, b.material);
    }

    /// **The v1 graph is refused by the arm with its defect named.** The conformance suite
    /// convicted v1's names against the artifact; here that conviction is what a node DOES —
    /// a chain-armed node asked to serve the v1 declaration says which node it cannot build.
    #[test]
    fn the_v1_declaration_is_refused_with_the_node_named() {
        let artifact = std::sync::Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8));
        let profile = qwen36_profile_v1(fixture_geometry()).expect("v1 projects");
        let class_id = profile.shape_profile_id();
        let root = artifact.artifact_root();
        let canonical = rc_job_context(&profile, 4, 2);
        let holdings = vec![crate::lineages::qwen36::holding_from_artifact(artifact, None)];
        let armed = PalwClassSdk::builtin_v1(court(), b"misaka-palw-rc".to_vec()).with_chain_classes_v1();
        let err = armed.resolve_chain_registered(class_id, root, &holdings, &profile, &canonical).map(drop).unwrap_err();
        assert!(err.contains("cannot serve the registered graph"), "the kernel boundary speaks: {err}");
    }

    /// **The root decides the container.** A dense declaration paired with a root that a held
    /// MAPPING answers to reaches the mmap engine — and is refused by that engine's geometry
    /// gate, because a family is never inferred from the profile under judgment.
    #[test]
    fn a_dense_declaration_on_a_mapped_root_is_refused_by_the_mmap_engine() {
        let artifact = std::sync::Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8));
        let (_, dense_profile) = super::chain_arm_tests::class();
        let class_id = dense_profile.shape_profile_id();
        let root = artifact.artifact_root();
        let canonical = rc_job_context(&dense_profile, 4, 2);
        let holdings = vec![crate::lineages::qwen36::holding_from_artifact(artifact, None)];
        let armed = PalwClassSdk::builtin_v1(court(), b"misaka-palw-rc".to_vec()).with_chain_classes_v1();
        let err = armed.resolve_chain_registered(class_id, root, &holdings, &dense_profile, &canonical).map(drop).unwrap_err();
        assert!(err.contains("cannot serve the registered graph"), "the geometry gate speaks: {err}");
    }
}

#[cfg(test)]
mod chain_only_lattice_tests {
    use super::chain_arm_tests_support::*;
    use super::*;
    use kaspa_consensus_core::palw_base0_profile::rc_job_context;
    use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_commitment_v3};
    use kaspa_consensus_core::palw_freeprompt_v3::{fp_claim_id_v3, fp_class_quantum_leaves_v1, fp_quanta_v3};
    use kaspa_consensus_core::palw_state_v2::{
        PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwPanelSeatV2,
        PalwPwuRuleV2, PalwStateParamsV2, apply_palw_transition_v2,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    const MATURITY: u64 = 5;
    const USE_WINDOW: u64 = 50;
    const NETWORK: &[u8] = b"misaka-palw-rc";

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// **ADR-0067's whole promise, walked: a class that exists ONLY as chain data goes from
    /// registration to a minted-receipt admission, with no row for it in any table.**
    ///
    /// The registration object is the SAME shape the wire carries (built by
    /// `palw_post_genesis_registration_v1`, carriage included). The execution backend comes from
    /// `resolve_chain_registered` — the fenced arm, armed — so every forward walks the registered
    /// declaration. The lattice is the state machine's own: committed → bound (and the duty says
    /// free-prompt) → licensed → Final → the FULL receipt-spend admission, ML-DSA-87 signature
    /// and all, for the envelope a mining node's producer builds.
    #[test]
    fn a_chain_only_class_certifies_and_earns_a_receipt_block_admission() {
        // ---- the class, existing nowhere but as data -------------------------------------
        let (artifact, profile) = class();
        let class_id = profile.shape_profile_id();
        let root = artifact.artifact_digest();
        let canonical = rc_job_context(&profile, 4, 2);
        let holdings = vec![holding(artifact.clone())];

        // The wire-shaped registration: profile and canonical RIDE the object (ADR-0049
        // Decision H), exactly as a stranger's carrier would deliver them.
        let bond_outpoint = TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 };
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed([0x67u8; kaspa_pq_validator_core::VALIDATOR_SEED_LEN]);
        let pubkey = key.public_key().to_vec();
        let registration = kaspa_consensus_core::palw_class_admission_v2::palw_post_genesis_registration_v1(
            profile.clone(),
            canonical.clone(),
            root,
            1,
            u128::MAX,
            5,
            0,
            PalwBondKeyV2(bond_outpoint),
            Vec::new(),
        )
        .expect("the chain-only class registers");

        // ---- the base the state machine requires, plus the bond, plus the stranger --------
        let floor =
            misaka_palw_base0::classes::canonical_class_by_model_id_v1(&court(), "PALW-BASE-0/rc").expect("the floor is registered");
        let floor_root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, floor.class_id(), 4, 1000, 1, 800, 0)
            .unwrap()
            .with_fp_quanta(8, 64)
            .unwrap();
        let at =
            |block: u64, daa: u64, blue: u64| PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: blue, subsidy: 0 };
        let genesis_objects = vec![
            Obj::ClassRegistered {
                class_id: floor.class_id(),
                artifact_root: floor_root,
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            Obj::BondRegistered {
                bond: PalwBondKeyV2(bond_outpoint),
                pubkey: pubkey.clone(),
                operator_pubkey: vec![21; 8],
                // Sized for the work it backs (admission item 8 reaches the free-prompt lane).
                collateral: 10_000,
                payout_payload: h(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ];
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &params, &at(1, 100, 1), &genesis_objects, None).unwrap();
        let (s2, _) = apply_palw_transition_v2(&s1, &params, &at(2, 101, 2), &[registration], None).unwrap();
        assert!(s2.class(&class_id).is_some(), "the chain now holds the stranger's class");

        // ---- execution, through the FENCED ARM — no table row exists to fall back to ------
        let armed = PalwClassSdk::builtin_v1(court(), NETWORK.to_vec()).with_chain_classes_v1();
        let backend = armed
            .resolve_chain_registered(class_id, root, &holdings, &profile, &canonical)
            .expect("the armed arm serves the chain-only class");

        let prompt_ids: Vec<u32> = vec![3, 9, 17];
        let prompt: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
        let mut job = fp_job(class_id, profile.n_ctx, &prompt_ids, 3);
        job.executor_bond = bond_outpoint;
        job.executor_pubkey = pubkey.clone();
        let run = backend.execute_free_prompt(&job, &prompt).expect("the declared graph runs the caller's prompt");

        let class_facts = PalwFpClassFactsV3 {
            model_profile_id: root,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: root,
            shape_profile_id: class_id,
            cu_ruleset_id: Hash64::default(),
        };
        let commitment = palw_fp_commitment_v3(&job, &class_facts, &run, NETWORK, 999_999).expect("a finished run commits");
        let claim_id = fp_claim_id_v3(&commitment);
        // The class's quantum is an eighth of its canonical job (ADR-0074 Decision 5).
        // **Counted against the COURT this fixture runs, not the executor's constant** (ADR-0082
        // Decision 1). The ruleset is right here — `court()` — and a harness that bounds the class
        // at `PALW_STEP_MAX_LEAVES` while the chain bounds it at `max_step_leaf_count` is a harness
        // that cannot exercise any class between the two.
        let canonical_leaves =
            kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&profile, &canonical, court().max_step_leaf_count())
                .expect("the class counts its job");
        let quanta = fp_quanta_v3(commitment.work_leaves, fp_class_quantum_leaves_v1(canonical_leaves, 8), 16);
        assert!(quanta >= 1, "the job earns a draw at the class's quantum, got {quanta} at {} leaves", commitment.work_leaves);

        // ---- the lattice: committed → bound → licensed → Final ---------------------------
        let committed = Obj::FreePromptCommitted {
            claim: claim_id,
            class_id,
            bond: PalwBondKeyV2(bond_outpoint),
            executor_pubkey: pubkey.clone(),
            work_leaves: commitment.work_leaves,
            prompt_token_ids_hash: commitment.job.prompt_token_ids_hash,
            decode_tokens_executed: commitment.decode_tokens_executed,
            trace_root: commitment.trace_root,
            output_root: commitment.output_root,
            execution_root: commitment.execution_root,
            trace_chunk_count: commitment.trace_chunk_count,
            trace_retention_daa: commitment.trace_retention_daa,
        };
        let (s3, _) = apply_palw_transition_v2(&s2, &params, &at(3, 102, 3), &[committed], None).unwrap();
        let seats = vec![PalwPanelSeatV2 { bond: PalwBondKeyV2(bond_outpoint), operator_id: h(90) }];
        let (s4, _) =
            apply_palw_transition_v2(&s3, &params, &at(4, 103, 4), &[Obj::PanelBound { claim: claim_id, anchor: h(77), seats }], None)
                .unwrap();
        let duties = kaspa_consensus_core::palw_producer_v2::palw_seat_duties_v2(&s4, &params, &[PalwBondKeyV2(bond_outpoint)]);
        let duty = duties.iter().find(|d| d.claim_id == claim_id).expect("the seat sees the stranger's claim");
        assert!(duty.free_prompt, "and its duty names the replay lane");
        let (s5, _) = apply_palw_transition_v2(
            &s4,
            &params,
            &at(5, 104, 5),
            &[Obj::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }],
            None,
        )
        .unwrap();
        let (state, _) = apply_palw_transition_v2(&s5, &params, &at(6, 125, 6), &[], None).unwrap();
        assert!(matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the chain-only claim certifies");

        // ---- the receipt block: the producer's envelope, fully admitted -------------------
        //
        // **The beacon is SEARCHED, not chosen.** A quantum's ticket is
        // `H(network ‖ beacon ‖ claim ‖ q)` compared against the class's receipt target, which the
        // RC seeds at `u128::MAX / 2` — a coin flip — and `claim_id` is one of its inputs. A
        // hard-coded beacon therefore makes this test pass or fail by luck about a lottery it is
        // not testing, and it came up tails the moment ADR-0082 Decision 11's two job fields moved
        // the claim id. Searching is what the shape asks for: a producer hunting a spendable
        // quantum does exactly this (`PalwFpSpendableQuantumV3`). Anything that is NOT the ticket
        // is re-raised on the spot, so the loop cannot hide a real refusal.
        let (pph, ts, nonce) = (h(0xB0), 1_700u64, 9u64);
        let mut admitted = None;
        for i in 0..64u64 {
            let beacon_block = h(0xBEAC + i);
            let beacon =
                kaspa_consensus_core::palw_freeprompt_v3::PalwBeaconFactV3 { beacon_block, beacon_daa: 131, prev_attempt_daa: 121 };
            let envelope = key.build_fp_receipt_spend_envelope(h(999), pph, ts, nonce, claim_id, 0, bond_outpoint, beacon_block);
            match kaspa_consensus_core::palw_fp_admission_v3::check_palw_receipt_spend_admission_full_v3(
                &state,
                &at(7, 132, 7),
                h(999),
                pph,
                ts,
                nonce,
                MATURITY,
                USE_WINDOW,
                &beacon,
                &envelope,
                |pk: &[u8], m: &[u8], c: &[u8], sig: &[u8]| {
                    kaspa_txscript::verify_mldsa87_with_context(pk, m, c, sig).unwrap_or(false)
                },
            ) {
                Ok(id) => {
                    admitted = Some(id);
                    break;
                }
                Err(kaspa_consensus_core::palw_fp_admission_v3::PalwFpAdmissionV3Error::TicketRejected { .. }) => continue,
                Err(e) => panic!("the chain refused the spend for a reason that is not the lottery: {e:?}"),
            }
        }
        let admitted = admitted.expect("no beacon in 64 tries won a one-in-two draw — the target or the ticket moved");
        assert_ne!(admitted, Hash64::default(), "the chain admits a receipt block for a class no binary ever tabled");
    }
}

#[cfg(test)]
mod revision_row_tests {
    //! **The weight-key veto, and the one exception it earned on 2026-09-01.**
    //!
    //! The mock's world is deliberately the dangerous one: every entry pairs with the artifact, so
    //! nothing here rests on shapes happening to differ. What separates the cases is only what the
    //! TERMS say the chain has — which is exactly the information the filter must decide by.

    use std::sync::Arc;

    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2;
    use kaspa_consensus_core::palw_step::PalwShapeProfileV3;

    use super::*;

    const ROOT: u64 = 0xA17;

    /// The fields this module's cases never read, at values the filter ignores.
    fn terms_rest() -> PalwRegistrationTermsV2 {
        PalwRegistrationTermsV2 {
            registered_class_ids: Vec::new(),
            registered_artifact_roots: Vec::new(),
            initial_target: 1u128,
            min_grantable_share_permille: 1,
            slash_value_per_pwu: 1,
            chain_certified_families: Vec::new(),
        }
    }

    fn profile(n_ctx: u32) -> PalwShapeProfileV3 {
        let geometry =
            kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 { n_ctx, ..super::chain_arm_tests::small_geometry() };
        qwen25_a16_profile_v2(geometry).expect("the test geometry projects")
    }

    struct MockLineage;

    impl PalwModelLineageV1 for MockLineage {
        fn lineage_id(&self) -> &'static str {
            "mock-revision"
        }
        fn classes(&self, _court: &PalwCourtParamsV2) -> Vec<PalwClassEntryV1> {
            vec![
                PalwClassEntryV1 {
                    model_id: "ModelM",
                    lineage_id: "mock-revision",
                    profile: profile(16),
                    canonical_job: (7, 2),
                    needs_artifact_file: true,
                },
                PalwClassEntryV1 {
                    model_id: "ModelM/graph-v2",
                    lineage_id: "mock-revision",
                    profile: profile(17),
                    canonical_job: (7, 2),
                    needs_artifact_file: true,
                },
                PalwClassEntryV1 {
                    model_id: "SiblingS",
                    lineage_id: "mock-revision",
                    profile: profile(18),
                    canonical_job: (7, 2),
                    needs_artifact_file: true,
                },
            ]
        }
        fn sniffs(&self, _head: &[u8; 8]) -> bool {
            false
        }
        fn load(&self, _path: &std::path::Path) -> Result<PalwLoadedArtifactV1, String> {
            Err("the mock loads nothing".into())
        }
        fn registered_weight_keys(&self, _artifact: &PalwLoadedArtifactV1) -> Vec<Hash64> {
            vec![Hash64::from_u64_word(ROOT)]
        }
        fn pair(
            &self,
            _court: &PalwCourtParamsV2,
            _entry: &PalwClassEntryV1,
            _artifact: &PalwLoadedArtifactV1,
        ) -> Result<Hash64, String> {
            Ok(Hash64::from_u64_word(ROOT))
        }
        fn resolve(
            &self,
            _court: &PalwCourtParamsV2,
            _class_id: Hash64,
            _artifact_root: Hash64,
            _holdings: &[PalwLoadedArtifactV1],
            _network_id: &[u8],
        ) -> Option<Result<Box<dyn PalwExecutionBackendV1>, String>> {
            None
        }
    }

    fn sdk() -> PalwClassSdk {
        PalwClassSdk::with_lineages(vec![Arc::new(MockLineage)], super::chain_arm_tests::court(), b"mock-net".to_vec())
    }

    fn holding() -> PalwLoadedArtifactV1 {
        PalwLoadedArtifactV1::from_parts("mock-revision", None, "a mock holding".into(), Arc::new(()))
    }

    fn id_of(sdk: &PalwClassSdk, model: &str) -> Hash64 {
        sdk.lineages()[0].classes(&super::chain_arm_tests::court()).iter().find(|e| e.model_id == model).unwrap().class_id()
    }

    fn models(candidates: &[PalwRegistrationCandidateV1]) -> Vec<&'static str> {
        candidates.iter().map(|c| c.entry.model_id).collect()
    }

    /// Fresh weights: every pairing entry is a candidate — the pre-existing behaviour.
    #[test]
    fn fresh_weights_offer_everything() {
        let s = sdk();
        let terms =
            PalwRegistrationTermsV2 { registered_class_ids: Vec::new(), registered_artifact_roots: Vec::new(), ..terms_rest() };
        assert_eq!(models(&s.candidate_classes(&[holding()], &terms)), vec!["ModelM", "ModelM/graph-v2", "SiblingS"]);
    }

    /// **The exception**: the chain's owner of these bytes is ModelM, so ModelM's revision row is a
    /// candidate — and the sibling, which pairs just as well, still is not.
    #[test]
    fn registered_weights_admit_their_own_models_revision_and_nothing_else() {
        let s = sdk();
        let terms = PalwRegistrationTermsV2 {
            registered_class_ids: vec![id_of(&s, "ModelM")],
            registered_artifact_roots: vec![Hash64::from_u64_word(ROOT)],
            ..terms_rest()
        };
        let got = models(&s.candidate_classes(&[holding()], &terms));
        assert!(got.contains(&"ModelM/graph-v2"), "the revision row is the whole point: {got:?}");
        assert!(!got.contains(&"SiblingS"), "a same-shape sibling on registered weights is the n_ctx-17 scar: {got:?}");
    }

    /// The scar, intact: weights pinned by a class this build cannot attribute seed nothing.
    #[test]
    fn registered_weights_with_no_known_owner_seed_nothing() {
        let s = sdk();
        let terms = PalwRegistrationTermsV2 {
            registered_class_ids: Vec::new(),
            registered_artifact_roots: vec![Hash64::from_u64_word(ROOT)],
            ..terms_rest()
        };
        assert!(s.candidate_classes(&[holding()], &terms).is_empty());
    }

    /// The sibling as owner: ModelM gets nothing off SiblingS's registered bytes — in either
    /// direction, revisions included.
    #[test]
    fn a_siblings_registered_weights_admit_only_the_sibling() {
        let s = sdk();
        let terms = PalwRegistrationTermsV2 {
            registered_class_ids: vec![id_of(&s, "SiblingS")],
            registered_artifact_roots: vec![Hash64::from_u64_word(ROOT)],
            ..terms_rest()
        };
        assert_eq!(models(&s.candidate_classes(&[holding()], &terms)), vec!["SiblingS"]);
    }

    /// The suffix rule is exact: only a wholly numeric `/graph-vN` is a revision.
    #[test]
    fn base_model_id_is_strict() {
        assert_eq!(base_model_id("Qwen/Qwen2.5-1.5B/graph-v2"), "Qwen/Qwen2.5-1.5B");
        assert_eq!(base_model_id("Qwen3.6-35B-A3B/graph-v12"), "Qwen3.6-35B-A3B");
        assert_eq!(base_model_id("ModelM"), "ModelM");
        assert_eq!(base_model_id("ModelM/graph-vNext"), "ModelM/graph-vNext");
        assert_eq!(base_model_id("/graph-v2"), "/graph-v2");
    }

    /// **The bound decides before the loader is asked, and the loader is asked once per admitted
    /// path.** A caller that answers a path from something it already holds — the node's
    /// per-process holdings — relies on exactly that: a path past the bound is skipped by name and
    /// never loaded, a loader's refusal is carried by name, and nothing is asked twice.
    #[test]
    fn the_bounded_loader_asks_its_loader_once_per_admitted_path() {
        let stamp = std::process::id();
        let small = std::env::temp_dir().join(format!("misaka-sdk-bound-small-{stamp}"));
        let large = std::env::temp_dir().join(format!("misaka-sdk-bound-large-{stamp}"));
        let broken = std::env::temp_dir().join(format!("misaka-sdk-bound-broken-{stamp}"));
        std::fs::write(&small, [0u8; 8]).expect("a small file");
        std::fs::write(&large, [0u8; 16]).expect("a larger file");
        std::fs::write(&broken, [0u8; 8]).expect("a file the loader refuses");
        let asked = std::cell::RefCell::new(Vec::new());
        let (held, skipped) = sdk().load_artifacts_bounded_with_v1(&[small.clone(), large.clone(), broken.clone()], 16, |path| {
            asked.borrow_mut().push(path.to_path_buf());
            if path == broken.as_path() { Err("refused by the loader".to_string()) } else { Ok(holding()) }
        });
        // 8 fits; 8 + 16 would pass 16 and is skipped before the loader hears of it; 8 + 8 fits
        // and the loader refuses it.
        assert_eq!(held.len(), 1);
        assert_eq!(*asked.borrow(), vec![small.clone(), broken.clone()], "asked once per admitted path, in the operator's order");
        assert_eq!(skipped.len(), 2);
        assert_eq!(skipped[0].0, large);
        assert!(skipped[0].1.contains("artifact bound"), "{}", skipped[0].1);
        assert_eq!(skipped[1], (broken.clone(), "refused by the loader".to_string()));
        for path in [small, large, broken] {
            std::fs::remove_file(path).ok();
        }
    }
}
