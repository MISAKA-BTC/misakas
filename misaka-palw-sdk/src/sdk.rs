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
    /// **ADR-0067 Decision 5's fence.** `false` (the default) keeps the chain-registered-class
    /// arm sealed: [`Self::resolve_chain_registered`] refuses with the fence named, and nothing
    /// this SDK serves can come from a profile the build's tables do not carry. Armed only by
    /// [`Self::with_chain_classes_v1`], which a node exposes as an operator flag — never a
    /// default — until the ADR's fuzz gate has run to its stated saturation.
    chain_classes: bool,
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
            match self.load_artifact(path) {
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
    /// 4. an artifact whose digest is the registered root is among `holdings` — Decision 6:
    ///    possession is the operator's chosen act, so its absence is a "fetch it" error, not a
    ///    protocol fault;
    /// 5. the plan compiles — every declared node lands inside this build's kernel vocabulary
    ///    (Decision 3's boundary, surfaced with the node named).
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
        let Some(artifact) = crate::lineages::dense::dense_artifact_by_digest(holdings, artifact_root) else {
            return Err(format!(
                "this node holds no artifact whose digest is the registered root {artifact_root} — fetch the class's \
                 artifact and load it (--palw-class-artifact); registration does not obligate possession (ADR-0067 Decision 6)"
            ));
        };
        let backend = misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend::from_registered_profile(
            artifact,
            self.network_id.clone(),
            profile.clone(),
            (canonical.declared_prefill_tokens, canonical.exact_decode_tokens),
        )?;
        Ok(Box::new(backend))
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
        PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court")
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
        let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x0067)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("sorted and unique");
        let geometry = PalwQwen25GeometryV1 {
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
        };
        (std::sync::Arc::new(artifact), qwen25_a16_profile_v2(geometry).expect("the corrected profile builds"))
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
        );

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
mod chain_only_lattice_tests {
    use super::chain_arm_tests_support::*;
    use super::*;
    use kaspa_consensus_core::palw_base0_profile::rc_job_context;
    use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_commitment_v3};
    use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpCuWeightsV3, fp_claim_id_v3, fp_quanta_v3};
    use kaspa_consensus_core::palw_state_v2::{
        PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwPanelSeatV2,
        PalwPwuRuleV2, PalwStateParamsV2, apply_palw_transition_v2,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    const MATURITY: u64 = 5;
    const USE_WINDOW: u64 = 50;
    const NETWORK: &[u8] = b"misaka-palw-rc";
    const QUANTUM_CU: u128 = 100;

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
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, floor.class_id(), 4, 1000, 1, 800, 0).unwrap();
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
                collateral: 1_000,
                payout_payload: h(0x9A11),
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
        let weights = PalwFpCuWeightsV3 { prefill_weight: 1, decode_weight: 64 };
        let commitment = palw_fp_commitment_v3(&job, &class_facts, &run, NETWORK, &weights, 999_999).expect("a finished run commits");
        let claim_id = fp_claim_id_v3(&commitment);
        let quanta = fp_quanta_v3(commitment.cu, QUANTUM_CU, 16);
        assert!(quanta >= 1, "the job earns a draw at the shipped quantum, got {quanta} at cu {}", commitment.cu);

        // ---- the lattice: committed → bound → licensed → Final ---------------------------
        let committed = Obj::FreePromptCommitted {
            claim: claim_id,
            class_id,
            bond: PalwBondKeyV2(bond_outpoint),
            executor_pubkey: pubkey.clone(),
            pwu: quanta as u64 * 10,
            quanta,
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
        let beacon = kaspa_consensus_core::palw_freeprompt_v3::PalwBeaconFactV3 {
            beacon_block: h(0xBEAC),
            beacon_daa: 131,
            prev_attempt_daa: 121,
        };
        let (pph, ts, nonce) = (h(0xB0), 1_700u64, 9u64);
        let envelope = key.build_fp_receipt_spend_envelope(h(999), pph, ts, nonce, claim_id, 0, bond_outpoint, h(0xBEAC));
        let admitted = kaspa_consensus_core::palw_fp_admission_v3::check_palw_receipt_spend_admission_full_v3(
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
            |pk: &[u8], m: &[u8], c: &[u8], sig: &[u8]| kaspa_txscript::verify_mldsa87_with_context(pk, m, c, sig).unwrap_or(false),
        )
        .expect("the chain admits a receipt block for a class no binary ever tabled");
        assert_ne!(admitted, Hash64::default());
    }
}
