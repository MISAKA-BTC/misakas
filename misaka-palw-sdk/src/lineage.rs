//! **The contract every model lineage signs: what a build must be able to do with a class
//! before that class can exist anywhere else in the system.**
//!
//! Before this trait, "what the two tables share is the CONTRACT" was a sentence in
//! `misaka_palw_base0::classes` — model id → frozen geometry → profile whose id is the class id —
//! and the contract was enforced by prose. Each new lineage (the dense floor, the A16 tier, the
//! Qwen3.6 mmap tier) re-stated it as a new table type, and every consumer (the panel's
//! registration loop, the backend registry's dispatch, the artifact loader's magic switch) grew
//! one arm per lineage. An arm forgotten in one consumer was a class that could register but not
//! produce, or produce but not be judged — and nothing pointed at the gap until a node hit it.
//!
//! [`PalwModelLineageV1`] is that contract as a type. A lineage that implements it can be loaded,
//! enumerated, paired with the weights it claims, registered, and executed — through
//! [`crate::PalwClassSdk`], which is the ONLY door those consumers use. A lineage that does not
//! implement it does not exist to any of them, which is the property the SDK is for: there is no
//! second path a new model can take.

use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_base0_profile::rc_job_context;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// One class some lineage of this build can supply, in the shape every consumer reads.
///
/// This is deliberately the SAME projection for every lineage: what a registration needs is the
/// model id (for the operator), the profile (the class — its `shape_profile_id` IS the class id,
/// ADR-0049 Decision G), the canonical job (what one unit of its work is), and whether the class's
/// weights arrive as a file or derive from a pin. Everything lineage-specific — container format,
/// root form, engine — stays behind the lineage that owns it.
#[derive(Clone, Debug)]
pub struct PalwClassEntryV1 {
    /// The identity a human uses. For a converted class it is the checkpoint's own id, so a
    /// conversion can be checked against the thing it claims to be.
    pub model_id: &'static str,
    /// The lineage that supplies this class — always the id of the lineage whose `classes()`
    /// produced the entry, asserted by the conformance harness.
    pub lineage_id: &'static str,
    /// The graph. `shape_profile_id()` is the class id the chain registers.
    pub profile: PalwShapeProfileV3,
    /// `(prefill, decode)` the class is paid per.
    pub canonical_job: (u32, u32),
    /// `false` for a derived class (the floor): every node can mint its artifact from the pinned
    /// seed, so it needs no file and no deployment.
    pub needs_artifact_file: bool,
}

impl PalwClassEntryV1 {
    /// The class id: its graph's id (ADR-0049 Decision G). Never declared beside the profile,
    /// because a second spelling would be a second thing to drift.
    pub fn class_id(&self) -> Hash64 {
        self.profile.shape_profile_id()
    }

    /// The canonical job as the context every consumer of it builds — the registration message,
    /// the admission gate's recount and the producer's pricing all read THIS value, from one
    /// derivation, so no two of them can describe the job differently.
    pub fn canonical_context(&self) -> PalwJobContextV2 {
        rc_job_context(&self.profile, self.canonical_job.0, self.canonical_job.1)
    }
}

/// One artifact file (or derived equivalent) as loaded by the lineage that owns its container.
///
/// The payload is opaque on purpose: only the lineage that loaded an artifact ever looks inside
/// it, so a lineage's container format is nobody else's business — the single-writer rule, applied
/// to bytes. Everything a consumer outside the lineage may need (which lineage, where from, what
/// to log) is carried beside the payload in plain fields.
#[derive(Clone)]
pub struct PalwLoadedArtifactV1 {
    /// The lineage whose `load` produced this. The SDK only ever hands an artifact back to the
    /// lineage this names, which is what makes the payload downcast inside that lineage total.
    pub lineage_id: &'static str,
    /// Where it came from, when it came from a file.
    pub path: Option<PathBuf>,
    /// One human line for the operator's log, written by the lineage at load time — the only
    /// moment the rich per-container detail (layer counts, GiB, computed roots) is naturally in
    /// hand.
    pub summary: String,
    payload: Arc<dyn Any + Send + Sync>,
}

impl PalwLoadedArtifactV1 {
    /// Assemble one. Called by lineage `load` implementations and their fixtures; consumers never
    /// build these by hand.
    pub fn from_parts(lineage_id: &'static str, path: Option<PathBuf>, summary: String, payload: Arc<dyn Any + Send + Sync>) -> Self {
        Self { lineage_id, path, summary, payload }
    }

    /// The payload, for the owning lineage to downcast. Anyone else holding this has taken a
    /// wrong turn — the type inside is the lineage's private business.
    pub fn payload(&self) -> Arc<dyn Any + Send + Sync> {
        self.payload.clone()
    }
}

impl std::fmt::Debug for PalwLoadedArtifactV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PalwLoadedArtifactV1").field("lineage_id", &self.lineage_id).field("path", &self.path).finish_non_exhaustive()
    }
}

/// **The interface every model lineage implements — the whole of it.**
///
/// A lineage is an artifact container plus a class table plus an execution backend: the unit at
/// which "how do we run this family of models" is actually decided. Adding a NEW MEMBER of a known
/// lineage (another checkpoint of an architecture this build already executes) is a data change —
/// a geometry constant and a table row inside that lineage — and no consumer moves. Adding a new
/// LINEAGE is implementing this trait and handing it to [`crate::PalwClassSdk`]; the trait is
/// object-safe, so the SDK's consumers (registration, resolution, loading, the CLI) work on the
/// new lineage without knowing it exists.
///
/// Implementations MUST pass [`crate::conformance::check_lineage_v1`] — the SDK's own test suite
/// runs it over every built-in lineage, and a new lineage's first test should be that one call.
/// The harness enforces the invariants the admission gate will later enforce on chain
/// (adjudicable shape, catalogued kernels, countable canonical job), so a lineage learns about a
/// hole at `cargo test`, not at registration.
pub trait PalwModelLineageV1: Send + Sync {
    /// Stable name, unique across the SDK's lineages. Stamped on every entry and artifact this
    /// lineage produces.
    fn lineage_id(&self) -> &'static str;

    /// Every class of this lineage this build can supply, at FROZEN geometries — a geometry that
    /// moved would silently rename a class the chain already registered.
    fn classes(&self, court: &PalwCourtParamsV2) -> Vec<PalwClassEntryV1>;

    /// Does the 8-byte file head name this lineage's container? Decided from the magic alone,
    /// without reading the body.
    fn sniffs(&self, head: &[u8; 8]) -> bool;

    /// A container whose decoder authenticates the format internally may claim the fallback slot:
    /// files no lineage sniffs are offered to it, and its decoder's refusal is the error the
    /// operator sees. At most one lineage per SDK may return `true` (asserted at construction).
    fn is_container_fallback(&self) -> bool {
        false
    }

    /// Load one artifact file of this lineage's container, verifying whatever the container
    /// verifies and computing whatever the chain will later be matched against (a mapped tier
    /// computes its root here, once — the root is this node's proof that it holds what the chain
    /// registered, and a root read from a sidecar would be a declaration).
    fn load(&self, path: &Path) -> Result<PalwLoadedArtifactV1, String>;

    /// **The roots under which THESE WEIGHTS could already sit on chain.** Matched against
    /// `PalwRegistrationTermsV2::registered_artifact_roots` before the artifact may candidate for
    /// any new class: re-registering known weights under a fresh id is never meaningful, and with
    /// two same-shape artifacts loaded it is exactly the mispairing that burned the n_ctx-17 seat
    /// on 2026-08-28 (the genesis 1.5B digest registered under the Coder class id). Known weights
    /// serve their own class; only unknown weights are looking for one.
    fn registered_weight_keys(&self, artifact: &PalwLoadedArtifactV1) -> Vec<Hash64>;

    /// Pair one artifact with one of this lineage's entries: check the artifact has the shape the
    /// class is defined at, and derive the `artifact_root` a registration for that pair would pin.
    /// `Err` carries the field-naming refusal — derive, never declare (ADR-0046).
    fn pair(&self, court: &PalwCourtParamsV2, entry: &PalwClassEntryV1, artifact: &PalwLoadedArtifactV1) -> Result<Hash64, String>;

    /// **Resolve the class the chain named into something that can run it** — or say it is not
    /// this lineage's class.
    ///
    /// `None` means "not mine": the SDK falls through to the next lineage. `Some(Err(..))` means
    /// "mine, and this node cannot serve it" — the refusal that names the missing artifact and the
    /// flag that fixes it. A lineage never substitutes: producing or judging under a class the
    /// chain did not name is worse than not participating.
    fn resolve(
        &self,
        court: &PalwCourtParamsV2,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        class_id: Hash64,
        artifact_root: Hash64,
        holdings: &[PalwLoadedArtifactV1],
        network_id: &[u8],
    ) -> Option<Result<Box<dyn PalwExecutionBackendV1>, String>>;
}
