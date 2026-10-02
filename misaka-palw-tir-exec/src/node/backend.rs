//! **The generic IR backend** (RFC-0002 Phase F, F9; design §2.10): one IR class, served through
//! `PalwExecutionBackendV1` from a mapped PALWTIR1 artifact by the typed executor.
//!
//! * `job_for_anchor` — the chain's own J5 derivation for an IR class
//!   (`palw_tir_attempt_v1::palw_tir_attempt_job_for_anchor_of_v1` over the class's cached job
//!   facts): the canonical context with the anchor's id and seed, and the anchor's prompt ids over
//!   the program's `token_bound`, committed in the class's form.
//! * `execute` — the job run into its step leg and roots ([`super::run`]); the material is a
//!   [`TirCaptureV1`]: the binding, the prompt, the committed logits trace, and — within a byte cap —
//!   every leaf preimage (a DENSE capture, which anyone holding it opens at any leaf, as the legacy
//!   families' captures are); past the cap a FOLD, whose leaves are re-derived by replay.
//! * `verify_material` — a seat's check that the material answers for the claim: the claim's job,
//!   roots and output root, the binding, the trace root over the committed rows and ids, and the step
//!   root over the committed leaves (a dense capture's own; a fold's re-derived). It is not a
//!   judgement of honesty — that is the replay's (`execute_for_verdict`) and the court's.
//! * `bisect_prefix_state` — [`super::evidence::tir_bisect_prefix_state_v1`] over the capture's
//!   leaves (a dense capture's own, a fold's replayed).
//! * Readiness: the root, the leaf hashes and the drawn operands from the held inventory tree.
//! * The IR court's close proofs — `PalwCourtVerdictProofV2::TirCone`, `TirLogits`,
//!   `TirDecodeTokenTiled` / `TirDecodeToken` (F5, appended to the close proofs) — are built by
//!   inherent methods here ([`TirBackendV1::cone_close`], [`TirBackendV1::logits_close`],
//!   [`TirBackendV1::decode_token_close`]) over [`super::evidence::TirEvidenceV1`], under the rules
//!   the court grades them by ([`TirBackendV1::court_rules`]); the node's court flow asks them of an
//!   IR class (the trait's legacy verbs cannot carry an IR close). `supports_court` is `true`.

use std::sync::Arc;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1;
use kaspa_consensus_core::palw_attempt_rules_v1::{PalwAttemptRulesV1, palw_attempt_output_root_v1};
use kaspa_consensus_core::palw_backend::{
    PalwCaptureShapeV1, PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1,
};
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1};
use kaspa_consensus_core::palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1, step_merkle_root_capped_v1, step_tile_leaf_hash_v1};
use kaspa_consensus_core::palw_step_refute::{PalwDecodeTokenPinV1, PalwTiledDecodePinV1, tiled_logits_scheme_id_v1};
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_job_for_anchor_of_v1};
use kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1;
use kaspa_consensus_core::palw_tir_court_v1::{
    PalwTirConeRefutationV1, PalwTirCourtRulesV1, PalwTirLogitsConsistencyV1, PalwTirStepLeafDisclosureV1,
    build_tir_dissect_bottom_v1, build_tir_dissect_round_v1, build_tir_named_leaf_refutation_v1, build_tir_root_claim_v1,
    build_tir_step_leaf_disclosure_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{PalwTirDissectPhaseV1, PalwTirDissectRoundV1, PalwTirRootClaimV1};
use kaspa_consensus_core::palw_tir_step_v1::{
    PalwTirStepBindingV1, PalwTirStepSpaceV1, palw_tir_execution_root_v1, verify_tir_binding_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;

use super::annex::{PALW_TIR_LEAF_ANNEX_VERSION_V1, PalwTirAnnexTraceV1, PalwTirLeafAnnexV1, tir_annex_trace_v1};
use super::artifact::TirArtifactV1;
use super::evidence::{TirEvidenceV1, TirRetainedJobV1, TirTraceV1, tir_bisect_prefix_state_v1};
use super::run::TirClassRunnerV1;

/// The 8-byte head of an encoded [`TirCaptureV1`].
pub const TIR_CAPTURE_MAGIC_V1: [u8; 8] = *b"PALWTIRC";

/// A capture holds every preimage while their lanes stay within this many bytes (a dense
/// capture); past it, a fold. (A leaf is up to 16 KiB of lanes — a 4,096-lane logits tile — so the
/// cap is in bytes, not leaves.)
pub const TIR_DENSE_CAPTURE_BYTES_V1: usize = 64 << 20;

/// Whether a new [`TirBackendV1`] runs the fused kernels ([`set_tir_fused_kernels_default_v1`]).
static TIR_FUSED_KERNELS_DEFAULT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// **Run the fused kernels in every IR backend built from now on** (RFC-0002 §7, Phase G) — the
/// node's `--palw-tir-fused-kernels`, set once at start-up. Node software in no consensus object:
/// byte-identical to the generic kernels (tir-lower's `fused_gate`), so it moves speed and nothing
/// else. OFF by default, and kept off until the D-F drills pass with it on.
pub fn set_tir_fused_kernels_default_v1(on: bool) {
    TIR_FUSED_KERNELS_DEFAULT.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// What a new [`TirBackendV1`] starts with ([`set_tir_fused_kernels_default_v1`]).
pub fn tir_fused_kernels_default_v1() -> bool {
    TIR_FUSED_KERNELS_DEFAULT.load(std::sync::atomic::Ordering::Relaxed)
}

/// **One retained job, kept process-wide** — keyed by the class and the job's context hash. A fold's
/// every evidence build (an annex asked, a dissection's round) re-derives the execution it opens
/// ([`TirBackendV1::retain`], a whole run); a backend is built per request, so the memo lives here.
/// One entry: the retention is a class's leaf hashes, trace and resume points (≈ 180 MB for a 1.5B
/// class at 512 positions), and one pursuit or one serve at a time is what a node runs.
static TIR_RETAINED_MEMO: std::sync::Mutex<Option<(Hash64, Hash64, Arc<TirRetainedJobV1>)>> = std::sync::Mutex::new(None);

/// **Why an IR class takes no free prompt** (RFC-0002): its free-prompt lane stays closed until
/// Phase H. The node's backend, the RPC's pricing and the CLI refuse with these words.
pub const TIR_FREE_PROMPT_CLOSED_V1: &str =
    "free-prompt claims of an IR class are closed until RFC-0002 Phase H — an IR class serves attempts only";

/// **The child a challenger disputes** (RFC-0002 F7): the first of `phase`'s pending children whose
/// claimed partials are not the ones `honest` computes — the challenger's own round over the same
/// range, against the same root ([`TirBackendV1::dissect_round`] over its own execution). `None` when
/// every child agrees (the responder's children are the truth there) or the two rounds are not of one
/// shape.
pub fn tir_dissect_choice_v1(phase: &PalwTirDissectPhaseV1, honest: &PalwTirDissectRoundV1) -> Option<u8> {
    if phase.pending().len() != honest.children.len() {
        return None;
    }
    phase.pending().iter().zip(&honest.children).position(|(claimed, truth)| claimed != truth).and_then(|i| u8::try_from(i).ok())
}

/// **What an IR producer retains and serves for one execution** — the material of its outcome.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct TirCaptureV1 {
    pub binding: PalwTirStepBindingV1,
    /// The job's prompt ids (they commit to the context's `prompt_token_ids_hash`).
    pub prompt: Vec<u32>,
    /// The committed logits trace: every selecting row and the ids it selected.
    pub logits_rows: Vec<Vec<i32>>,
    pub generated: Vec<u32>,
    /// Every leaf preimage in step order (a dense capture), or none (a fold).
    pub leaves: Vec<PalwStepTileLeafV1>,
}

impl TirCaptureV1 {
    /// The 8-byte head of an encoded capture ([`TIR_CAPTURE_MAGIC_V1`]).
    pub const MAGIC: [u8; 8] = TIR_CAPTURE_MAGIC_V1;

    pub fn encode(&self) -> Vec<u8> {
        let mut out = TIR_CAPTURE_MAGIC_V1.to_vec();
        borsh::to_writer(&mut out, self).expect("a Vec writer does not fail");
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let body = bytes.strip_prefix(&TIR_CAPTURE_MAGIC_V1[..]).ok_or("not an IR capture")?;
        borsh::from_slice(body).map_err(|e| format!("the IR capture does not decode: {e}"))
    }

    pub fn is_dense(&self) -> bool {
        !self.leaves.is_empty()
    }
}

/// **An IR capture's answer to a trace-event demand** (RFC-0002 Phase F, F6 D): the event `(row,
/// tile)` of the committed logits trace, disclosed in the IR form from the capture's own binding,
/// rows and ids (`tir_logits_event_disclosure_v1`) — or `OutOfRange` over the binding alone when the
/// event is not in the run. No execution: the answer is the committed trace, opened. `None` when the
/// bytes are not an IR capture (a legacy family answers them).
pub fn tir_trace_event_disclosure_of_capture_v1(
    capture: &[u8],
    row: u32,
    tile: u8,
) -> Option<Result<kaspa_consensus_core::palw_tir_court_v1::PalwTirTraceEventDisclosureV1, String>> {
    use kaspa_consensus_core::palw_tir_court_v1::{PalwTirTraceEventDisclosureV1, tir_logits_event_disclosure_v1};
    if !capture.starts_with(&TIR_CAPTURE_MAGIC_V1) {
        return None;
    }
    Some(TirCaptureV1::decode(capture).map(|c| {
        tir_logits_event_disclosure_v1(&c.binding, &c.logits_rows, &c.generated, row, tile)
            .unwrap_or_else(|| PalwTirTraceEventDisclosureV1::OutOfRange { binding: Box::new(c.binding) })
    }))
}

fn count_of(binding: &PalwTirStepBindingV1) -> u64 {
    binding.step_leaf_count
}

/// **DRILL ONLY (RFC-0006, D-S3): the consistent boundary lie a producer commits** — process-wide, set once at start-up by
/// `--palw-drill-tamper-boundary` and consulted by [`TirBackendV1`]'s injected-fault runs (a node that sets none never lies).
static TIR_BOUNDARY_LIE_V1: std::sync::Mutex<Option<super::run::TirBoundaryLieV1>> = std::sync::Mutex::new(None);

pub fn set_tir_drill_boundary_lie_v1(lie: Option<super::run::TirBoundaryLieV1>) {
    *TIR_BOUNDARY_LIE_V1.lock().unwrap_or_else(|p| p.into_inner()) = lie;
}

fn tir_drill_boundary_lie_v1() -> Option<super::run::TirBoundaryLieV1> {
    *TIR_BOUNDARY_LIE_V1.lock().unwrap_or_else(|p| p.into_inner())
}

/// One IR class served from a mapped artifact.
pub struct TirBackendV1 {
    model_id: String,
    artifact: Arc<TirArtifactV1>,
    class: PalwTirClassV1,
    space: PalwTirStepSpaceV1,
    artifact_root: Hash64,
    class_id: Hash64,
    canonical: PalwJobContextV2,
    /// What the class's attempt jobs are a function of (consensus's `PalwTirJobFactsV1`, cached).
    facts: PalwTirJobFactsV1,
    /// The network's prompt form; the class's own is `facts.prompt_ids_form(network_form)`.
    network_form: PalwPromptIdsFormV1,
    /// The form this class commits its prompt ids in.
    prompt_ids_form: PalwPromptIdsFormV1,
    /// The ruleset's `max_step_leaf_count` (the ladder every opening and root is capped at).
    ladder: u64,
    attempt_rules: PalwAttemptRulesV1,
    /// The leaf hashes of the last fold re-executed, by job context — the ladder asks a fold's
    /// prefix state at every rung, and each answer would otherwise be a whole re-execution.
    fold_hashes: std::sync::Mutex<Option<(Hash64, Arc<Vec<Hash64>>)>>,
    /// The lane bytes up to which a capture is dense ([`TIR_DENSE_CAPTURE_BYTES_V1`] by default).
    dense_capture_bytes: usize,
    /// Run the fused kernels ([`set_tir_fused_kernels_default_v1`], [`Self::with_fused_kernels`]).
    fused: bool,
}

impl TirBackendV1 {
    /// Serve the class `artifact` declares under `artifact_root` (the inventory root the chain
    /// registered — derived from the artifact by the caller, never read from a sidecar here), at the
    /// canonical job `canonical` (its `shape_profile_id` must be the class id).
    pub fn new(
        model_id: String,
        artifact: Arc<TirArtifactV1>,
        artifact_root: Hash64,
        canonical: PalwJobContextV2,
        prompt_ids_form: PalwPromptIdsFormV1,
        ladder: u64,
    ) -> Result<Self, String> {
        let class = artifact.class()?;
        let space = PalwTirStepSpaceV1::new(&class).map_err(|e| e.to_string())?;
        let class_id = class.class_id(&artifact_root);
        let facts = PalwTirJobFactsV1::of(&class, &space.program, class_id);
        let network_form = prompt_ids_form;
        let prompt_ids_form = facts.prompt_ids_form(network_form);
        if canonical.shape_profile_id != class_id {
            return Err("the canonical job names another class".into());
        }
        space.leaf_count_capped(&canonical, ladder).map_err(|e| format!("the canonical job does not count: {e}"))?;
        Ok(Self {
            model_id,
            artifact,
            class,
            space,
            artifact_root,
            class_id,
            canonical,
            facts,
            network_form,
            prompt_ids_form,
            ladder,
            attempt_rules: PalwAttemptRulesV1::CoreV1,
            fold_hashes: std::sync::Mutex::new(None),
            dense_capture_bytes: TIR_DENSE_CAPTURE_BYTES_V1,
            fused: tir_fused_kernels_default_v1(),
        })
    }

    /// Run the fused kernels (or not), whatever the process default says.
    pub fn with_fused_kernels(mut self, on: bool) -> Self {
        self.fused = on;
        self
    }

    pub fn fused_kernels(&self) -> bool {
        self.fused
    }

    /// Keep captures dense only while their lanes fit `bytes` (0: every capture is a fold).
    pub fn with_dense_capture_bytes(mut self, bytes: usize) -> Self {
        self.dense_capture_bytes = bytes;
        self
    }

    pub fn class(&self) -> &PalwTirClassV1 {
        &self.class
    }

    pub fn class_id(&self) -> Hash64 {
        self.class_id
    }

    pub fn artifact_root(&self) -> Hash64 {
        self.artifact_root
    }

    pub fn space(&self) -> &PalwTirStepSpaceV1 {
        &self.space
    }

    pub fn artifact(&self) -> &Arc<TirArtifactV1> {
        &self.artifact
    }

    pub fn canonical(&self) -> &PalwJobContextV2 {
        &self.canonical
    }

    pub fn runner(&self) -> TirClassRunnerV1<'_> {
        TirClassRunnerV1::new(&self.space, self.artifact.plan(), self.artifact.params(), self.class_id)
            .expect("the space and the plan are of one program")
            .with_fused(self.fused)
    }

    fn ids(prompt: &[usize]) -> Result<Vec<u32>, String> {
        prompt.iter().map(|t| u32::try_from(*t).map_err(|_| "a prompt id past u32".to_string())).collect()
    }

    /// **Run a job and retain it** (the producer's retention: leaf hashes and resume points).
    pub fn retain(&self, job: &PalwJobContextV2, prompt: &[u32]) -> Result<TirRetainedJobV1, String> {
        if !prompt_token_ids_match_v1(self.prompt_ids_form, prompt, &job.prompt_token_ids_hash) {
            return Err("the prompt does not commit to the job's prompt hash".into());
        }
        self.runner().retain(&self.class, self.artifact_root, job, prompt, self.ladder)
    }

    /// [`Self::retain`], through the process-wide memo ([`TIR_RETAINED_MEMO`]): the same job of the same
    /// class is run once, however many evidence builds read it.
    pub fn retain_memo(&self, job: &PalwJobContextV2, prompt: &[u32]) -> Result<Arc<TirRetainedJobV1>, String> {
        let ctx_hash = job.context_hash();
        if let Ok(memo) = TIR_RETAINED_MEMO.lock()
            && let Some((class, ctx, held)) = memo.as_ref()
            && *class == self.class_id
            && *ctx == ctx_hash
            && held.prompt == prompt
        {
            return Ok(held.clone());
        }
        let job = Arc::new(self.retain(job, prompt)?);
        if let Ok(mut memo) = TIR_RETAINED_MEMO.lock() {
            *memo = Some((self.class_id, ctx_hash, job.clone()));
        }
        Ok(job)
    }

    /// Run a job into a capture — dense while its lanes fit the dense-capture bytes —
    /// optionally with one lane of leaf `fault` corrupted and the commitment re-derived over the lie
    /// (a drill; always dense).
    fn capture_run(&self, job: &PalwJobContextV2, prompt: &[u32], fault: Option<u64>) -> Result<TirCaptureV1, String> {
        if !prompt_token_ids_match_v1(self.prompt_ids_form, prompt, &job.prompt_token_ids_hash) {
            return Err("the prompt does not commit to the job's prompt hash".into());
        }
        let (mut dense, mut bytes) = (true, 0usize);
        let mut leaves = Vec::new();
        // The drill's consistent boundary lie replaces the single-leaf fault: the lie is committed AND computed on.
        let boundary = fault.and_then(|_| tir_drill_boundary_lie_v1());
        let fault = if boundary.is_some() { None } else { fault };
        let run = self.runner().with_boundary_lie(boundary).run(job, prompt, self.ladder, false, &mut |l| {
            if !dense {
                return;
            }
            bytes += l.preimage.values_le.len();
            if bytes > self.dense_capture_bytes && fault.is_none() && boundary.is_none() {
                (dense, leaves) = (false, Vec::new());
                return;
            }
            leaves.push(l.preimage.clone());
        })?;
        let mut binding = PalwTirStepBindingV1 {
            version: kaspa_consensus_core::palw_tir_step_v1::PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: job.clone(),
            class: self.class.clone(),
            artifact_root: self.artifact_root,
            full_logits_trace_root: run.trace_root,
            step_leaf_count: run.leaf_count,
            step_merkle_root: run.step_merkle_root,
            committed_execution_root: run.execution_root,
        };
        if let Some(i) = fault {
            let leaf = leaves.get_mut(i as usize).ok_or_else(|| format!("no leaf {i} to corrupt"))?;
            let lane = (leaf.value_count as usize / 2) * 4;
            let v = i32::from_le_bytes(leaf.values_le[lane..lane + 4].try_into().expect("four bytes"));
            leaf.values_le[lane..lane + 4].copy_from_slice(&v.wrapping_add(1).to_le_bytes());
            let ctx_hash = job.context_hash();
            let hashes: Vec<Hash64> = leaves.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash, &self.class_id, p)).collect();
            binding.step_merkle_root = step_merkle_root_capped_v1(&hashes, self.ladder).map_err(|e| e.to_string())?;
            binding.committed_execution_root = palw_tir_execution_root_v1(
                &ctx_hash,
                &binding.full_logits_trace_root,
                &self.class_id,
                binding.step_leaf_count,
                &binding.step_merkle_root,
            );
        }
        Ok(TirCaptureV1 { binding, prompt: prompt.to_vec(), logits_rows: run.logits_rows, generated: run.generated, leaves })
    }

    fn outcome(&self, capture: &TirCaptureV1) -> PalwExecutionOutcomeV1 {
        let b = &capture.binding;
        PalwExecutionOutcomeV1 {
            trace_root: b.full_logits_trace_root,
            output_root: palw_attempt_output_root_v1(&b.job_context, &capture.generated),
            execution_root: b.committed_execution_root,
            trace_manifest_root: kaspa_consensus_core::palw_attempt_v2::attempt_trace_manifest_root_v1(b.full_logits_trace_root, 1),
            trace_chunk_count: 1,
            material: capture.encode(),
        }
    }

    /// A capture of THIS class, decoded.
    pub fn decode_capture(&self, material: &[u8]) -> Result<TirCaptureV1, String> {
        let capture = TirCaptureV1::decode(material)?;
        if capture.binding.class.class_id(&capture.binding.artifact_root) != self.class_id {
            return Err("a capture of another class".into());
        }
        Ok(capture)
    }

    /// The leaf hashes a capture answers with: a dense capture's own; a fold's, replayed (which
    /// is its producer's execution when that execution was honest).
    fn capture_leaf_hashes(&self, capture: &TirCaptureV1) -> Result<Arc<Vec<Hash64>>, String> {
        let ctx = &capture.binding.job_context;
        let ctx_hash = ctx.context_hash();
        if capture.is_dense() {
            return Ok(Arc::new(capture.leaves.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash, &self.class_id, p)).collect()));
        }
        // The context commits to the prompt, so a re-execution is a function of the context once
        // the prompt is checked against it — which is what makes the context a sound memo key.
        if !prompt_token_ids_match_v1(self.prompt_ids_form, &capture.prompt, &ctx.prompt_token_ids_hash) {
            return Err("the capture's prompt does not commit to its job".into());
        }
        let mut memo = self.fold_hashes.lock().map_err(|_| "the fold memo is poisoned")?;
        if let Some((key, hashes)) = memo.as_ref()
            && *key == ctx_hash
        {
            return Ok(hashes.clone());
        }
        let run = self.runner().run(ctx, &capture.prompt, self.ladder, false, &mut |_| {})?;
        let hashes = Arc::new(run.leaf_hashes);
        *memo = Some((ctx_hash, hashes.clone()));
        Ok(hashes)
    }

    /// **The canonical cone refutation of leaf `index` of a capture** — the same object whichever
    /// party asks: from a dense capture's own preimages, or — for a fold — from this node's
    /// re-execution, which must be the capture's execution (a fold of a forged execution is opened
    /// with [`Self::challenger_refutation`] instead).
    pub fn cone_refutation(
        &self,
        material: &[u8],
        index: u64,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwTirConeRefutationV1, String> {
        self.with_capture_store(material, rules.prompt_form, |store, _| store.cone_refutation(index, rules).map_err(|e| e.to_string()))
    }

    /// **The evidence store over a capture, and the capture's binding** — a dense capture's own
    /// preimages, or — for a fold — this node's re-execution, which must be the capture's execution
    /// (a fold of an execution this node does not reproduce opens nothing here). What every builder
    /// that speaks for a capture's commitments reads: the cone close, and F7's root claim, rounds and
    /// bottom.
    fn with_capture_store<R>(
        &self,
        material: &[u8],
        prompt_form: PalwPromptIdsFormV1,
        f: impl FnOnce(&TirEvidenceV1<'_>, &PalwTirStepBindingV1) -> Result<R, String>,
    ) -> Result<R, String> {
        let capture = self.decode_capture(material)?;
        let runner = self.runner();
        if capture.is_dense() {
            let store = TirEvidenceV1::dense(
                &runner,
                &capture.binding,
                &capture.prompt,
                &capture.logits_rows,
                &capture.generated,
                &capture.leaves,
                self.artifact.as_ref(),
                prompt_form,
                self.ladder,
            )?;
            return f(&store, &capture.binding);
        }
        let own = self.retain_memo(&capture.binding.job_context, &capture.prompt)?;
        if own.binding != capture.binding {
            return Err("a fold of an execution this node does not reproduce: open it as a challenger".into());
        }
        let store = TirEvidenceV1::own(&runner, &own, self.artifact.as_ref(), prompt_form, self.ladder)?;
        f(&store, &own.binding)
    }

    /// **The served annex of leaf `leaf` of this node's capture** (RFC-0002's evidence transport, option
    /// B; [`super::annex`]): the claim's binding with its program stripped, the leaf's preimage and
    /// opening, and the trace summary — built from a dense capture's own preimages, or from this node's
    /// re-derivation of its fold (the executor answering for its own claim).
    pub fn leaf_annex(&self, material: &[u8], leaf: u64) -> Result<PalwTirLeafAnnexV1, String> {
        let capture = self.decode_capture(material)?;
        use kaspa_consensus_core::palw_tir_court_v1::PalwTirEvidenceStoreV1;
        let (opening, preimage, mut binding) = self.with_capture_store(material, self.prompt_ids_form, |store, binding| {
            let opening = store.step_opening(leaf).ok_or_else(|| format!("leaf {leaf} does not open"))?;
            let preimage = store.step_leaf(leaf).ok_or_else(|| format!("leaf {leaf} is not held"))?;
            Ok((opening, preimage, binding.clone()))
        })?;
        let trace = tir_annex_trace_v1(&self.space, &binding.job_context, &capture.logits_rows, &capture.generated, leaf)?;
        kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut binding);
        Ok(PalwTirLeafAnnexV1 { version: PALW_TIR_LEAF_ANNEX_VERSION_V1, binding, opening, preimage, trace })
    }

    /// **The answer to a `TirStepLeaf { index }` data-availability unit of this node's claim** (RFC-0002
    /// evidence transport C, the second IR fence): consensus's ONE builder
    /// (`build_tir_step_leaf_disclosure_v1`) over this capture's store — a dense capture's own
    /// preimages, or this node's re-derivation of its fold (the executor answering for its own claim) —
    /// with the row pin exactly where the fold asks for one, self-checked by the fold's own check, the
    /// program stripped.
    pub fn step_leaf_disclosure(&self, material: &[u8], index: u64) -> Result<PalwTirStepLeafDisclosureV1, String> {
        self.with_capture_store(material, self.prompt_ids_form, |store, binding| {
            build_tir_step_leaf_disclosure_v1(binding, index, store, self.ladder).map_err(|e| format!("step leaf {index}: {e}"))
        })
    }

    /// **The answer to an IR step unit of this node's claim** (the second IR fence's DA units,
    /// evidence transport C): a step leaf by its disclosure ([`Self::step_leaf_disclosure`]'s
    /// builder), an interior step node by its frontier and opening
    /// (`build_tir_step_node_disclosure_v1` over the store's tree), a node of the tiled trace's rows
    /// tree by its frontier (a row's: its tile leaves) and opening with the ids
    /// (`build_tir_row_node_disclosure_v1` over the capture's rows), and a unit past this execution —
    /// a leaf at or past its leaf count, a node past its tree, a row at or past the decode count, a
    /// rows node past the rows tree or of a flat trace — by the claim's binding proving so
    /// (`TirStepOutOfRange`, the program stripped). Every answer is self-checked by the fold's own
    /// check before it is returned.
    pub fn step_unit_answer(
        &self,
        material: &[u8],
        unit: kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1,
    ) -> Result<kaspa_consensus_core::palw_da_rcore_v1::PalwDaAnswerV1, String> {
        use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
        use kaspa_consensus_core::palw_tir_court_v1::{
            build_tir_row_node_disclosure_v1, build_tir_step_node_disclosure_v1, build_tir_step_run_disclosure_v1,
            palw_tir_step_tree_width_v1,
        };
        let tiled = Hash64::from_bytes(self.space.program.logits_scheme_id) == tiled_logits_scheme_id_v1();
        self.with_capture_store(material, self.prompt_ids_form, |store, binding| {
            let count = binding.step_leaf_count;
            let out_of_range = || {
                let mut proof = binding.clone();
                kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut proof);
                Ok(PalwDaAnswerV1::TirStepOutOfRange(Box::new(proof)))
            };
            match unit {
                PalwDaUnitV1::TirStepLeaf { index } if index >= count => out_of_range(),
                PalwDaUnitV1::TirStepLeaf { index } => build_tir_step_leaf_disclosure_v1(binding, index, store, self.ladder)
                    .map(|d| PalwDaAnswerV1::TirStepLeaf(Box::new(d)))
                    .map_err(|e| format!("step leaf {index}: {e}")),
                PalwDaUnitV1::TirStepNode { level: 0, index } => Err(format!("step node (0, {index}): a leaf is a TirStepLeaf")),
                PalwDaUnitV1::TirStepNode { level, index } if palw_tir_step_tree_width_v1(count, level).is_none_or(|w| index >= w) => {
                    out_of_range()
                }
                PalwDaUnitV1::TirStepNode { level, index } => {
                    build_tir_step_node_disclosure_v1(binding, level, index, store, self.ladder)
                        .map(|d| PalwDaAnswerV1::TirStepNode(Box::new(d)))
                        .map_err(|e| format!("step node ({level}, {index}): {e}"))
                }
                // The rows tree is the tiled scheme's (a flat trace hashes every row at once): a row
                // at or past the decode count, a node past the tree, or any node of a flat trace is
                // proven out of range — the fold's own predicate (`check_tir_step_out_of_range_v1`).
                PalwDaUnitV1::TirRowNode { level, index }
                    if !tiled
                        || palw_tir_step_tree_width_v1(u64::from(binding.job_context.exact_decode_tokens), level)
                            .is_none_or(|w| index >= w) =>
                {
                    out_of_range()
                }
                PalwDaUnitV1::TirRowNode { level, index } => {
                    build_tir_row_node_disclosure_v1(binding, level, index, store, self.ladder)
                        .map(|d| PalwDaAnswerV1::TirRowNode(Box::new(d)))
                        .map_err(|e| format!("rows-tree node ({level}, {index}): {e}"))
                }
                // RFC-0006's run unit: a contiguous run of leaves a cell reads, with one range opening. A run that is not
                // wholly inside the execution is proven out of range by the binding, as a leaf past it is.
                PalwDaUnitV1::TirStepRun { first, count } if count == 0 || first.saturating_add(u64::from(count)) > count_of(binding) => {
                    out_of_range()
                }
                PalwDaUnitV1::TirStepRun { first, count } => build_tir_step_run_disclosure_v1(binding, first, count, store, self.ladder)
                    .map(|d| PalwDaAnswerV1::TirStepRun(Box::new(d)))
                    .map_err(|e| format!("step run [{first}, +{count}): {e}")),
                other => Err(format!("{other:?} is not an IR step unit")),
            }
        })
    }

    /// **A challenger's store over a served annex** — its own execution `own` of the same job for every
    /// leaf before the annex's, the annex's leaf and trace summary for the accused's. `binding` is the
    /// annex's, verified and filled ([`palw_tir_leaf_annex_verify_v1`]).
    fn with_annex_store<R>(
        &self,
        binding: &PalwTirStepBindingV1,
        annex: &PalwTirLeafAnnexV1,
        own: &TirRetainedJobV1,
        f: impl FnOnce(&TirEvidenceV1<'_>) -> Result<R, String>,
    ) -> Result<R, String> {
        let runner = self.runner();
        let pins: Vec<PalwTiledDecodePinV1> = match &annex.trace {
            PalwTirAnnexTraceV1::Tiled { pin: Some(pin), .. } => vec![pin.clone()],
            _ => Vec::new(),
        };
        let trace = match &annex.trace {
            PalwTirAnnexTraceV1::Flat { logits_rows, generated_token_ids } => {
                TirTraceV1::Rows { rows: logits_rows, generated: generated_token_ids }
            }
            PalwTirAnnexTraceV1::Tiled { rows_root, generated_token_ids, .. } => {
                TirTraceV1::Summary { rows_root: *rows_root, generated: generated_token_ids, pins: &pins }
            }
        };
        let store = TirEvidenceV1::challenger_with_trace(
            &runner,
            binding,
            own,
            &annex.opening,
            annex.preimage.clone(),
            trace,
            self.artifact.as_ref(),
            self.prompt_ids_form,
            self.ladder,
        )?;
        f(&store)
    }

    /// **The cone close of the annex's leaf** from a challenger's own execution and the annex (the
    /// first leaf the two differ at): a `TirCone` the court convicts on when that leaf is false.
    pub fn annex_cone_close(
        &self,
        binding: &PalwTirStepBindingV1,
        annex: &PalwTirLeafAnnexV1,
        own: &TirRetainedJobV1,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwCourtVerdictProofV2, String> {
        let refutation =
            self.with_annex_store(binding, annex, own, |store| store.cone_refutation(annex.leaf(), rules).map_err(|e| e.to_string()))?;
        Ok(PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) })
    }

    /// **The named-leaf proof of the annex's leaf** (a dissected leaf's one-move challenge, F7) — the
    /// accused's binding, the leaf's opening and preimage, nothing else.
    pub fn annex_named_leaf(
        &self,
        binding: &PalwTirStepBindingV1,
        annex: &PalwTirLeafAnnexV1,
        own: &TirRetainedJobV1,
    ) -> Result<PalwCourtVerdictProofV2, String> {
        let refutation = self.with_annex_store(binding, annex, own, |store| {
            build_tir_named_leaf_refutation_v1(binding, annex.leaf(), store).map_err(|e| e.to_string())
        })?;
        Ok(PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) })
    }

    /// **The logits door at the annex's leaf** (a leaf of the logits node): its step tile against the
    /// same row's lanes in the committed trace, from the annex's pin.
    pub fn annex_logits_close(
        &self,
        binding: &PalwTirStepBindingV1,
        annex: &PalwTirLeafAnnexV1,
        own: &TirRetainedJobV1,
    ) -> Result<PalwCourtVerdictProofV2, String> {
        let accusation = self.with_annex_store(binding, annex, own, |store| store.logits_consistency(annex.leaf()))?;
        Ok(PalwCourtVerdictProofV2::TirLogits { accusation: Box::new(accusation) })
    }

    /// **The tiled decode-token door from an annex's pin**: the committed token of row `row` against
    /// `beat_lane` (the challenger's own token), which the pin's beat tile must hold.
    pub fn annex_decode_token_close(
        &self,
        binding: &PalwTirStepBindingV1,
        annex: &PalwTirLeafAnnexV1,
        row: u32,
        beat_lane: u32,
    ) -> Result<PalwCourtVerdictProofV2, String> {
        let PalwTirAnnexTraceV1::Tiled { pin: Some(pin), .. } = &annex.trace else {
            return Err("the annex carries no pin (not a leaf of the logits node, or the flat scheme)".into());
        };
        let pins = [pin.clone()];
        let trace = TirTraceV1::Summary { rows_root: Hash64::default(), generated: &[], pins: &pins };
        let pin = trace
            .pin_v1(&binding.job_context, row, beat_lane)
            .ok_or("the annex's pin is not of that row, or its tile holds no such lane")?;
        let mut binding = binding.clone();
        kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut binding);
        Ok(PalwCourtVerdictProofV2::TirDecodeTokenTiled { binding: Box::new(binding), pin })
    }

    /// **F7's bottom close for a challenger that holds no accused capture** (RFC-0002's evidence
    /// transport, option D): the accused's disputed leaf as its ON-CHAIN root claim carries it (the
    /// finalize's opening and preimage, and its decode pin when the finalize read one), every other leaf
    /// from the challenger's own execution of the same job (`own_material`, its capture). `root_binding`
    /// is the root claim's finalize binding with the class's program put back.
    pub fn dissect_bottom_from_root_claim(
        &self,
        own_material: &[u8],
        root: &PalwTirRootClaimV1,
        root_binding: &PalwTirStepBindingV1,
        phase: &PalwTirDissectPhaseV1,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwTirConeRefutationV1, String> {
        let own_capture = self.decode_capture(own_material)?;
        let own = self.retain_memo(&own_capture.binding.job_context, &own_capture.prompt)?;
        let runner = self.runner();
        let finalize = &root.finalize;
        let trace = match &finalize.decode_tokens {
            Some(PalwDecodeTokenPinV1::TiledV1(pin)) => {
                TirTraceV1::Summary { rows_root: pin.rows_root, generated: &pin.generated_token_ids, pins: &[] }
            }
            Some(PalwDecodeTokenPinV1::Base0V1(pin)) => {
                TirTraceV1::Rows { rows: &pin.logits_rows, generated: &pin.generated_token_ids }
            }
            _ => TirTraceV1::Absent,
        };
        let store = TirEvidenceV1::challenger_with_trace(
            &runner,
            root_binding,
            &own,
            &finalize.output_opening,
            finalize.output_preimage.clone(),
            trace,
            self.artifact.as_ref(),
            rules.prompt_form,
            self.ladder,
        )?;
        build_tir_dissect_bottom_v1(root_binding, phase, &store, rules).map_err(|e| e.to_string())
    }

    /// **RFC-0002 F7: the responder's IR root claim** at the narrowed dissected leaf `narrowed` of its
    /// own capture (`build_tir_root_claim_v1`): every reduction over `H`'s honest totals, the elements
    /// the finalize reads, and the finalize's carriage. The program rides in its binding; a filer
    /// strips it.
    pub fn root_claim(&self, material: &[u8], narrowed: u64, rules: &PalwTirCourtRulesV1) -> Result<PalwTirRootClaimV1, String> {
        self.with_capture_store(material, rules.prompt_form, |store, binding| {
            build_tir_root_claim_v1(binding, narrowed, store, rules).map_err(|e| e.to_string())
        })
    }

    /// **RFC-0002 F7: one round's children of `phase`'s disputed range**, from a capture's commitments
    /// (`build_tir_dissect_round_v1` at the class's history tile): what the responder files from its
    /// own capture, and what a challenger computes from ITS own execution to find the child it
    /// disputes ([`tir_dissect_choice_v1`]).
    pub fn dissect_round(
        &self,
        material: &[u8],
        phase: &PalwTirDissectPhaseV1,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwTirDissectRoundV1, String> {
        let tile = self.space.layout.h_tile;
        self.with_capture_store(material, rules.prompt_form, |store, binding| {
            build_tir_dissect_round_v1(binding, phase, tile, store, rules).map_err(|e| e.to_string())
        })
    }

    /// **RFC-0002 F7: the bottom close's carriage** over `phase`'s terminal tile, from the ACCUSED's
    /// capture (`build_tir_dissect_bottom_v1`) — the same object whichever party builds it; the court
    /// decides which way it reads (`PalwCourtVerdictProofV2::TirDissection`).
    pub fn dissect_bottom(
        &self,
        accused: &[u8],
        phase: &PalwTirDissectPhaseV1,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwTirConeRefutationV1, String> {
        self.with_capture_store(accused, rules.prompt_form, |store, binding| {
            build_tir_dissect_bottom_v1(binding, phase, store, rules).map_err(|e| e.to_string())
        })
    }

    /// **RFC-0002 F7: a one-move accusation's proof at a DISSECTED leaf** — the accused's leaf `index`
    /// named and nothing else (`build_tir_named_leaf_refutation_v1`): under the held regime the chain
    /// opens the dissection there instead of adjudicating.
    pub fn named_leaf_refutation(
        &self,
        accused: &[u8],
        index: u64,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwTirConeRefutationV1, String> {
        self.with_capture_store(accused, rules.prompt_form, |store, binding| {
            build_tir_named_leaf_refutation_v1(binding, index, store).map_err(|e| e.to_string())
        })
    }

    /// **A challenger's refutation of the accused's leaf `disputed_opening.leaf_index`**: this
    /// node's own honest execution of the accused's job before the disputed leaf, the accused's
    /// disputed leaf and path, and the accused's committed trace.
    #[allow(clippy::too_many_arguments)]
    pub fn challenger_refutation(
        &self,
        accused: &PalwTirStepBindingV1,
        prompt: &[u32],
        disputed_opening: &PalwStepOpeningV1,
        disputed_preimage: PalwStepTileLeafV1,
        accused_rows: &[Vec<i32>],
        accused_generated: &[u32],
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwTirConeRefutationV1, String> {
        let own = self.retain(&accused.job_context, prompt)?;
        let runner = self.runner();
        let store = TirEvidenceV1::challenger(
            &runner,
            accused,
            &own,
            disputed_opening,
            disputed_preimage,
            accused_rows,
            accused_generated,
            self.artifact.as_ref(),
            rules.prompt_form,
            self.ladder,
        )?;
        store.cone_refutation(disputed_opening.leaf_index, rules).map_err(|e| e.to_string())
    }

    /// The logits-consistency accusation over logits leaf `index` of a capture.
    pub fn logits_consistency(&self, material: &[u8], index: u64) -> Result<PalwTirLogitsConsistencyV1, String> {
        let capture = self.decode_capture(material)?;
        let runner = self.runner();
        if !capture.is_dense() {
            return Err("a fold carries no step tile to accuse".into());
        }
        let store = TirEvidenceV1::dense(
            &runner,
            &capture.binding,
            &capture.prompt,
            &capture.logits_rows,
            &capture.generated,
            &capture.leaves,
            self.artifact.as_ref(),
            self.prompt_ids_form,
            self.ladder,
        )?;
        store.logits_consistency(index)
    }

    /// The tiled decode pin of a capture's decode row `row` against lane `beat_lane`.
    pub fn decode_token_pin(&self, material: &[u8], row: u32, beat_lane: u32) -> Result<PalwTiledDecodePinV1, String> {
        let capture = self.decode_capture(material)?;
        kaspa_consensus_core::palw_step_refute::tiled_decode_pin_v1(
            &capture.binding.job_context,
            &capture.logits_rows,
            &capture.generated,
            row,
            beat_lane,
        )
        .ok_or_else(|| "the capture's trace has no such row or lane".into())
    }

    /// **The first step leaf at which an accused capture parts from this node's own execution of
    /// the same job** — found as the ladder finds it (`tir_first_divergence_v1` over both captures'
    /// leaves: a dense capture's own, a fold's re-derived). `None` when every leaf agrees (a lie in
    /// the trace alone is the logits or the decode-token door's). What a one-move accusation names.
    pub fn first_divergent_leaf(&self, accused: &[u8], own: &[u8]) -> Result<Option<u64>, String> {
        let (a, o) = (self.decode_capture(accused)?, self.decode_capture(own)?);
        if a.binding.job_context != o.binding.job_context {
            return Err("the two captures are of different jobs".into());
        }
        let (ha, ho) = (self.capture_leaf_hashes(&a)?, self.capture_leaf_hashes(&o)?);
        Ok(super::evidence::tir_first_divergence_v1(&a.binding.job_context, &ha, &ho))
    }

    /// **The rules the court grades this class's closes under** (`adjudicate_close_proof_v2`'s IR
    /// arm): this backend's ladder and prompt form, and the court's IR work limits
    /// (`palw_tir_court_limits_v1`).
    pub fn court_rules(&self, court: &kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2) -> PalwTirCourtRulesV1 {
        PalwTirCourtRulesV1 {
            max_step_leaf_count: self.ladder,
            prompt_form: self.prompt_ids_form,
            limits: kaspa_consensus_core::palw_court_v2::palw_tir_court_limits_v1(court),
        }
    }

    /// **The IR cone close of leaf `index`** — `PalwCourtVerdictProofV2::TirCone`, what a
    /// `CourtClosed` carries (the same object whichever party files it).
    pub fn cone_close(&self, material: &[u8], index: u64, rules: &PalwTirCourtRulesV1) -> Result<PalwCourtVerdictProofV2, String> {
        Ok(PalwCourtVerdictProofV2::TirCone { refutation: Box::new(self.cone_refutation(material, index, rules)?) })
    }

    /// **The IR logits close** over logits leaf `index` (`PalwCourtVerdictProofV2::TirLogits`).
    pub fn logits_close(&self, material: &[u8], index: u64) -> Result<PalwCourtVerdictProofV2, String> {
        Ok(PalwCourtVerdictProofV2::TirLogits { accusation: Box::new(self.logits_consistency(material, index)?) })
    }

    /// **The IR decode-token close** of decode row `row` in the class's scheme: the tiled door with
    /// `beat_lane` as the lane said to beat the committed token, or the flat door over every row.
    pub fn decode_token_close(&self, material: &[u8], row: u32, beat_lane: u32) -> Result<PalwCourtVerdictProofV2, String> {
        let capture = self.decode_capture(material)?;
        let binding = Box::new(capture.binding.clone());
        if Hash64::from_bytes(self.space.program.logits_scheme_id) == tiled_logits_scheme_id_v1() {
            Ok(PalwCourtVerdictProofV2::TirDecodeTokenTiled { binding, pin: self.decode_token_pin(material, row, beat_lane)? })
        } else {
            let pin = kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1 {
                logits_rows: capture.logits_rows,
                generated_token_ids: capture.generated,
            };
            Ok(PalwCourtVerdictProofV2::TirDecodeToken { binding, pin, position: row })
        }
    }

    /// **The step leaf of the logits node at decode row `row` whose tile holds lane `lane`** — the
    /// leaf a decode-token door's pin rides with (`lane` the seat's own token), or the one a logits
    /// door accuses (`lane` the first lane a disclosed trace tile parts at). `None` for a row before
    /// the first selecting position, or a lane no logits tile of that row holds.
    pub fn logits_leaf_holding(&self, ctx: &PalwJobContextV2, row: u32, lane: u64) -> Option<u64> {
        let position = (ctx.declared_prefill_tokens + row).checked_sub(1)?;
        let post = u32::try_from(self.space.occurrences().len().checked_sub(1)?).ok()?;
        let logits = self.space.program.logits;
        self.space
            .leaves_of_position(ctx, position)
            .into_iter()
            .find(|l| {
                matches!(l.kind, kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
                    if occurrence == post && node == logits && (first_element..first_element + u64::from(l.value_count)).contains(&lane))
            })
            .map(|l| l.index)
    }

    /// **The logits door over a trace the steps did not compute, from a disclosed trace event** (the
    /// end of the second IR fence's rows-tree descent): a seat whose own steps are the claim's — its
    /// step root, beside the claim's trace root, gives the claim's execution root — descends the
    /// claim's rows tree (`TirRowNode`) to tile `tile` of row `row`, and the claim's executor discloses
    /// that tile (`TirEvent`, tiled). The close is the seat's OWN step tile holding the first lane at
    /// which the disclosed tile parts from the seat's own row — the claim's own leaf, since the step
    /// trees are one — opened in its own tree, beside the disclosed tile: `TirLogits`, which the court
    /// convicts on (`TirLogitsTraceMismatch`). `accused` is the claim's binding, its program filled.
    pub fn trace_logits_close(
        &self,
        own: &TirRetainedJobV1,
        accused: &PalwTirStepBindingV1,
        row: u32,
        tile: u8,
        event: &kaspa_consensus_core::palw_tir_court_v1::PalwTirTraceEventDisclosureV1,
    ) -> Result<PalwCourtVerdictProofV2, String> {
        use kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES;
        use kaspa_consensus_core::palw_tir_court_v1::{PalwTirEvidenceStoreV1, PalwTirTraceEventDisclosureV1, PalwTirTraceLanesV1};
        let PalwTirTraceEventDisclosureV1::Tiled { generated_token_ids, row_root, row_opening, tile_lanes, tile_opening, .. } = event
        else {
            return Err("the rows tree ends at a tiled trace event".into());
        };
        let theirs = (&accused.job_context, accused.step_leaf_count, accused.step_merkle_root);
        if (&own.binding.job_context, own.binding.step_leaf_count, own.binding.step_merkle_root) != theirs {
            return Err("the seat's own steps are not the claim's: the step tree's descent is the path".into());
        }
        let first = usize::from(tile) * PALW_LOGITS_TILE_LANES;
        let own_row = own.logits_rows.get(row as usize).ok_or("the row is past the seat's own run")?;
        let own_lanes = own_row.get(first..(first + PALW_LOGITS_TILE_LANES).min(own_row.len())).ok_or("the tile is past the row")?;
        if own_lanes.len() != tile_lanes.len() {
            return Err("the disclosed tile is not the row's tile width".into());
        }
        let at = own_lanes.iter().zip(tile_lanes).position(|(a, b)| a != b).ok_or("the disclosed tile is the seat's own")?;
        let lane = (first + at) as u64;
        let leaf = self
            .logits_leaf_holding(&accused.job_context, row, lane)
            .ok_or_else(|| format!("no logits step tile of row {row} holds lane {lane}"))?;
        let runner = self.runner();
        let store = TirEvidenceV1::own(&runner, own, self.artifact.as_ref(), self.prompt_ids_form, self.ladder)?;
        let accusation = PalwTirLogitsConsistencyV1 {
            binding: accused.clone(),
            step_opening: store.step_opening(leaf).ok_or_else(|| format!("step leaf {leaf} does not open"))?,
            step_preimage: store.step_leaf(leaf).ok_or_else(|| format!("step leaf {leaf} is not re-derived"))?,
            trace: PalwTirTraceLanesV1::Tiled {
                generated_token_ids: generated_token_ids.clone(),
                row_root: *row_root,
                row_opening: row_opening.clone(),
                tile_lanes: tile_lanes.clone(),
                tile_opening: tile_opening.clone(),
            },
        };
        Ok(PalwCourtVerdictProofV2::TirLogits { accusation: Box::new(accusation) })
    }

    /// The operands of the drawn leaves, in the draw's order. `base` is where the drawn tree starts in the
    /// artifact's inventory: 0 for a single artifact, the split for a composite's adapter section — the draw
    /// is over the section's own leaves (RFC-0004 §6.7), each returned under the index it was drawn at.
    fn drawn_operands(
        &self,
        draw: &[u32],
        base: u32,
    ) -> Result<Vec<(u32, kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1)>, String> {
        use super::inventory::TirParamOpenerV1;
        draw.iter()
            .map(|i| {
                let leaf = base.checked_add(*i).ok_or_else(|| format!("inventory leaf {i} is past the inventory"))?;
                self.artifact
                    .param_opening(leaf)
                    .map(|o| (*i, o.operand))
                    .ok_or_else(|| format!("inventory leaf {leaf} does not open"))
            })
            .collect()
    }

    /// The step opening of leaf `index` of a capture (the disclosure the bisection's last rung asks).
    pub fn step_opening(&self, material: &[u8], index: u64) -> Result<(PalwStepOpeningV1, PalwStepTileLeafV1), String> {
        let capture = self.decode_capture(material)?;
        if !capture.is_dense() {
            return Err("a fold discloses by replay: open its leaves with a node's own retention".into());
        }
        let hashes = self.capture_leaf_hashes(&capture)?;
        let opening = super::tree::TirStepTreeV1::full(&hashes).opening(index).ok_or("no such leaf")?;
        Ok((opening, capture.leaves[index as usize].clone()))
    }
}

impl PalwExecutionBackendV1 for TirBackendV1 {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn job_for_anchor(&self, anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
        // The chain's J5 derivation (`palw_tir_attempt_v1`), from the cached facts.
        let canonical = (self.canonical.declared_prefill_tokens, self.canonical.exact_decode_tokens);
        let (ctx, prompt) = palw_tir_attempt_job_for_anchor_of_v1(&self.facts, &anchor, canonical, self.network_form)
            .ok_or("the anchor's prompt does not commit")?;
        Ok((ctx, prompt.into_iter().map(|t| t as usize).collect()))
    }

    fn set_attempt_rules_v1(&mut self, rules: PalwAttemptRulesV1) {
        self.attempt_rules = rules;
    }

    fn attempt_rules_v1(&self) -> PalwAttemptRulesV1 {
        self.attempt_rules
    }

    fn execute(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        let capture = self.capture_run(job, &Self::ids(prompt)?, None)?;
        Ok(self.outcome(&capture))
    }

    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        let Ok(capture) = self.decode_capture(material) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        let b = &capture.binding;
        // Which question it answers: the claim's job, derived from its block — never the capture's.
        if claim.anchor != Hash64::default() {
            if b.job_context.job_id != claim.anchor {
                return PalwMaterialVerdictV1::Mismatch;
            }
            if let Some(draw) = claim.attempt_draw {
                let Ok((canonical, _)) = self.job_for_anchor(claim.anchor) else {
                    return PalwMaterialVerdictV1::Unverifiable;
                };
                if b.job_context != kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(canonical, draw) {
                    return PalwMaterialVerdictV1::Mismatch;
                }
            }
        }
        if b.committed_execution_root != claim.execution_root || b.full_logits_trace_root != claim.trace_root {
            return PalwMaterialVerdictV1::Mismatch;
        }
        if let Some(committed) = claim.output_root
            && palw_attempt_output_root_v1(&b.job_context, &capture.generated) != committed
        {
            return PalwMaterialVerdictV1::Mismatch;
        }
        if verify_tir_binding_v1(b, self.ladder).is_err()
            || !prompt_token_ids_match_v1(self.prompt_ids_form, &capture.prompt, &b.job_context.prompt_token_ids_hash)
        {
            return PalwMaterialVerdictV1::Mismatch;
        }
        // **The material answers for the roots — which is not "the execution is honest"** (the
        // families' one reading of this check): the trace root is the committed rows' and ids', and
        // the step root is the committed leaves'. Whether those leaves are the program's is the
        // replay's question (`execute_for_verdict`) and, at a leaf, the court's — a challenger must
        // find a lying capture here to build the close that convicts it.
        let ctx = &b.job_context;
        let trace = if Hash64::from_bytes(self.space.program.logits_scheme_id) == tiled_logits_scheme_id_v1() {
            kaspa_consensus_core::palw_step_refute::tiled_logits_trace_root_v1(ctx, &capture.logits_rows, &capture.generated)
        } else {
            Some(kaspa_consensus_core::palw_step_refute::base0_logits_trace_root_v1(ctx, &capture.logits_rows, &capture.generated))
        };
        if trace != Some(b.full_logits_trace_root) {
            return PalwMaterialVerdictV1::Mismatch;
        }
        // The leaves: a dense capture's own preimages; a fold's, re-derived — so a fold answers only
        // for the execution this build reproduces.
        let hashes = match self.capture_leaf_hashes(&capture) {
            Ok(hashes) => hashes,
            Err(_) => return PalwMaterialVerdictV1::Unverifiable,
        };
        if hashes.len() as u64 != b.step_leaf_count
            || step_merkle_root_capped_v1(&hashes, self.ladder).ok() != Some(b.step_merkle_root)
        {
            return PalwMaterialVerdictV1::Mismatch;
        }
        PalwMaterialVerdictV1::Matches
    }

    fn capture_shape(&self, material: &[u8]) -> Option<PalwCaptureShapeV1> {
        let capture = self.decode_capture(material).ok()?;
        let layers = self.space.program.schedule.layers.len();
        Some(PalwCaptureShapeV1 {
            job_context: capture.binding.job_context.clone(),
            step_leaf_count: capture.binding.step_leaf_count,
            layer_count: u16::try_from(layers).unwrap_or(u16::MAX),
        })
    }

    fn bisect_prefix_state(&self, material: &[u8], index: u64) -> Option<Hash64> {
        let capture = self.decode_capture(material).ok()?;
        if capture.binding.step_leaf_count == 0 || capture.binding.step_leaf_count > self.ladder {
            return None;
        }
        let hashes = self.capture_leaf_hashes(&capture).ok()?;
        Some(tir_bisect_prefix_state_v1(&capture.binding.job_context, &hashes, index))
    }

    /// The replay a seat licenses an attempt by: the roots of this build's own execution of the
    /// job — no capture laid out.
    fn execute_for_verdict(
        &self,
        job: &PalwJobContextV2,
        prompt: &[usize],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwReplayRootsV1, String> {
        let ids = Self::ids(prompt)?;
        if !prompt_token_ids_match_v1(self.prompt_ids_form, &ids, &job.prompt_token_ids_hash) {
            return Err("the prompt does not commit to the job's prompt hash".into());
        }
        let run = self.runner().run(job, &ids, self.ladder, false, &mut |_| {})?;
        Ok(kaspa_consensus_core::palw_backend::PalwReplayRootsV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            work_leaves: None,
            output_root: Some(run.output_root),
        })
    }

    /// **The IR court is this family's**: the ladder's rungs (`bisect_prefix_state`) and the close
    /// (the IR proofs this backend builds — `cone_close`, `logits_close`, `decode_token_close` — which
    /// the node's court flow asks of it for an IR class).
    fn supports_court(&self) -> bool {
        true
    }

    fn artifact_root_and_leaf_count(&self) -> Result<(Hash64, u32), String> {
        if self.artifact.composite_ref().is_some() {
            // A composite candidate's root is the composite root over its two sections, not its tree's.
            return self.artifact.inventory_root();
        }
        let tree = self.artifact.inventory_tree()?;
        Ok((tree.root(), tree.leaf_count()))
    }

    /// **RFC-0004 §6.3/§6.7 (spec 17 §17.7.1): a composite candidate's possession tree is its adapter
    /// section** — the section's own root and leaf count. `None` for every single artifact.
    fn artifact_possession_tree_v1(&self) -> Option<Result<(Hash64, u32), String>> {
        match self.artifact.possession_section() {
            Ok(None) => None,
            Ok(Some((_, root, leaves))) => {
                Some(u32::try_from(leaves.len()).map(|n| (root, n)).map_err(|_| "the adapter section has too many leaves".to_string()))
            }
            Err(e) => Some(Err(e)),
        }
    }

    /// The readiness material from the held inventory tree: its root, every leaf hash, and the drawn
    /// leaves' operands in the draw's order. (The IR inventory is in declaration order, not the
    /// legacy digest's name order, so it is served from the tree, never through a digest.) A composite
    /// candidate serves its adapter section instead (the section's root and leaves, the draw rebased).
    fn artifact_readiness_material(
        &self,
        draw: &[u32],
    ) -> Result<(Hash64, Vec<Hash64>, Vec<(u32, kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1)>), String> {
        if let Some((split, root, leaves)) = self.artifact.possession_section()? {
            return Ok((root, leaves.to_vec(), self.drawn_operands(draw, split)?));
        }
        let tree = self.artifact.inventory_tree()?;
        Ok((tree.root(), tree.leaves().to_vec(), self.drawn_operands(draw, 0)?))
    }

    fn artifact_readiness_material_streamed_v1(
        &self,
        draw: &[u32],
        on_leaf: &mut dyn FnMut(Hash64),
    ) -> Option<Result<(Hash64, u32, Vec<(u32, kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1)>), String>> {
        Some((|| {
            if let Some((split, root, leaves)) = self.artifact.possession_section()? {
                for leaf in leaves {
                    on_leaf(*leaf);
                }
                let count = u32::try_from(leaves.len()).map_err(|_| "the adapter section has too many leaves".to_string())?;
                return Ok((root, count, self.drawn_operands(draw, split)?));
            }
            let tree = self.artifact.inventory_tree()?;
            for leaf in tree.leaves() {
                on_leaf(*leaf);
            }
            Ok((tree.root(), tree.leaf_count(), self.drawn_operands(draw, 0)?))
        })())
    }

    fn output_root_for_context_v1(&self, context: &PalwJobContextV2, output_token_ids: &[u32]) -> Option<Hash64> {
        Some(palw_attempt_output_root_v1(context, output_token_ids))
    }

    /// RFC-0002: an IR class's free-prompt lane stays closed until Phase H.
    fn execute_free_prompt(
        &self,
        _job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        _prompt_tokens: &[usize],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        Err(TIR_FREE_PROMPT_CLOSED_V1.to_string())
    }

    fn execute_with_injected_fault(
        &self,
        job: &PalwJobContextV2,
        prompt: &[usize],
        leaf_index: u64,
    ) -> Result<PalwExecutionOutcomeV1, String> {
        let capture = self.capture_run(job, &Self::ids(prompt)?, Some(leaf_index))?;
        Ok(self.outcome(&capture))
    }

    fn canonical_job_prefill_tokens(&self) -> Option<usize> {
        Some(self.canonical.declared_prefill_tokens as usize)
    }

    fn artifact_row_opening(&self, index: u32) -> Result<PalwArtifactOpeningV1, String> {
        use super::inventory::TirParamOpenerV1;
        self.artifact.param_opening(index).ok_or_else(|| format!("inventory leaf {index} does not open"))
    }
}
