//! **The generic IR backend** (RFC-0002 Phase F, F9; design §2.10): one IR class, served through
//! `PalwExecutionBackendV1` from a mapped PALWTIR1 artifact by the typed executor.
//!
//! * `job_for_anchor` — the IR twin of the model classes' `CoreV1` derivation
//!   (`palw_attempt_job_for_anchor_v1`): the class's canonical context with the anchor's id and seed,
//!   and the anchor's prompt ids over the program's `token_bound`, committed in the network's form.
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
use kaspa_consensus_core::palw_attempt_rules_v1::{PalwAttemptRulesV1, palw_attempt_output_root_v1, palw_attempt_prompt_ids_v1};
use kaspa_consensus_core::palw_backend::{
    PalwCaptureShapeV1, PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1,
};
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1, prompt_token_ids_match_v1};
use kaspa_consensus_core::palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1, step_merkle_root_capped_v1, step_tile_leaf_hash_v1};
use kaspa_consensus_core::palw_step_refute::{PalwTiledDecodePinV1, tiled_logits_scheme_id_v1};
use kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1;
use kaspa_consensus_core::palw_tir_court_v1::{PalwTirConeRefutationV1, PalwTirCourtRulesV1, PalwTirLogitsConsistencyV1};
use kaspa_consensus_core::palw_tir_step_v1::{
    PalwTirStepBindingV1, PalwTirStepSpaceV1, palw_tir_execution_root_v1, verify_tir_binding_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;

use super::artifact::TirArtifactV1;
use super::evidence::{TirEvidenceV1, TirRetainedJobV1, tir_bisect_prefix_state_v1};
use super::run::TirClassRunnerV1;

/// The 8-byte head of an encoded [`TirCaptureV1`].
pub const TIR_CAPTURE_MAGIC_V1: [u8; 8] = *b"PALWTIRC";

/// A capture holds every preimage while their lanes stay within this many bytes (a dense
/// capture); past it, a fold. (A leaf is up to 16 KiB of lanes — a 4,096-lane logits tile — so the
/// cap is in bytes, not leaves.)
pub const TIR_DENSE_CAPTURE_BYTES_V1: usize = 64 << 20;

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

/// **The canonical job `(prefill, decode)` of an IR class**: the model classes' `CoreV1` formula
/// (`palw_attempt_canonical_v1`) over the layout's `max_context` — `(f − 1, 2)` with `f` =
/// `palw_canonical_footprint_floor_v1(max_context)` — and `None` for a context too narrow for it.
pub fn tir_attempt_canonical_v1(class: &PalwTirClassV1) -> Option<(u32, u32)> {
    let floor =
        u32::try_from(kaspa_consensus_core::palw_context_ladder::palw_canonical_footprint_floor_v1(class.layout.max_context)).ok()?;
    (floor >= 2).then_some((floor - 1, 2))
}

/// **The context of an IR class's job `(prefill, decode)`**: the model classes' `rc_job_context`
/// with the IR class id in the profile's place, the class's context bound and tokenizer, and the
/// trace scheme its logits scheme pins (the flat scheme runs under the v2 trace id).
pub fn tir_job_context_v1(class: &PalwTirClassV1, class_id: Hash64, prefill: u32, decode: u32) -> PalwJobContextV2 {
    let tiled = misaka_palw_tir::TirProgramV1::decode_canonical(&class.program)
        .map(|p| Hash64::from_bytes(p.logits_scheme_id) == tiled_logits_scheme_id_v1())
        .unwrap_or(false);
    PalwJobContextV2 {
        version: kaspa_consensus_core::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"misaka-palw-rc".to_vec(),
        job_id: Hash64::default(),
        job_nullifier: Hash64::default(),
        assignment_id: Hash64::default(),
        execution_seed: [0; 32],
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: class_id,
        trace_scheme_id: if tiled { tiled_logits_scheme_id_v1() } else { kaspa_consensus_core::palw_v2::trace_scheme_id_v2() },
        cu_ruleset_id: Hash64::default(),
        tokenizer_id: class.tokenizer_id,
        prompt_token_ids_hash: Hash64::default(),
        declared_prefill_tokens: prefill,
        exact_decode_tokens: decode,
        max_context_tokens: class.layout.max_context,
    }
}

/// **The job an anchor implies for an IR class** — the canonical context with the anchor's id and
/// seed, and the anchor's prompt over `token_bound`, committed under `form`.
pub fn tir_job_for_anchor_v1(
    canonical: &PalwJobContextV2,
    token_bound: u32,
    anchor: &Hash64,
    form: PalwPromptIdsFormV1,
) -> Option<(PalwJobContextV2, Vec<u32>)> {
    let prompt = palw_attempt_prompt_ids_v1(anchor, u64::from(token_bound), canonical.declared_prefill_tokens);
    let mut ctx = canonical.clone();
    ctx.job_id = *anchor;
    ctx.execution_seed = anchor.as_byte_slice()[..32].try_into().expect("a 64-byte hash has 32 bytes");
    ctx.prompt_token_ids_hash = prompt_token_ids_commitment_v1(form, &prompt).ok()?;
    Some((ctx, prompt))
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
    prompt_ids_form: PalwPromptIdsFormV1,
    /// The ruleset's `max_step_leaf_count` (the ladder every opening and root is capped at).
    ladder: u64,
    attempt_rules: PalwAttemptRulesV1,
    /// The leaf hashes of the last fold re-executed, by job context — the ladder asks a fold's
    /// prefix state at every rung, and each answer would otherwise be a whole re-execution.
    fold_hashes: std::sync::Mutex<Option<(Hash64, Arc<Vec<Hash64>>)>>,
    /// The lane bytes up to which a capture is dense ([`TIR_DENSE_CAPTURE_BYTES_V1`] by default).
    dense_capture_bytes: usize,
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
            prompt_ids_form,
            ladder,
            attempt_rules: PalwAttemptRulesV1::CoreV1,
            fold_hashes: std::sync::Mutex::new(None),
            dense_capture_bytes: TIR_DENSE_CAPTURE_BYTES_V1,
        })
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

    /// Run a job into a capture — dense while its lanes fit the dense-capture bytes —
    /// optionally with one lane of leaf `fault` corrupted and the commitment re-derived over the lie
    /// (a drill; always dense).
    fn capture_run(&self, job: &PalwJobContextV2, prompt: &[u32], fault: Option<u64>) -> Result<TirCaptureV1, String> {
        if !prompt_token_ids_match_v1(self.prompt_ids_form, prompt, &job.prompt_token_ids_hash) {
            return Err("the prompt does not commit to the job's prompt hash".into());
        }
        let (mut dense, mut bytes) = (true, 0usize);
        let mut leaves = Vec::new();
        let run = self.runner().run(job, prompt, self.ladder, false, &mut |l| {
            if !dense {
                return;
            }
            bytes += l.preimage.values_le.len();
            if bytes > self.dense_capture_bytes && fault.is_none() {
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
                rules.prompt_form,
                self.ladder,
            )?;
            return store.cone_refutation(index, rules).map_err(|e| e.to_string());
        }
        let own = self.retain(&capture.binding.job_context, &capture.prompt)?;
        if own.binding != capture.binding {
            return Err("a fold of an execution this node does not reproduce: open it as a challenger".into());
        }
        let store = TirEvidenceV1::own(&runner, &own, self.artifact.as_ref(), rules.prompt_form, self.ladder)?;
        store.cone_refutation(index, rules).map_err(|e| e.to_string())
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

    /// The operands of the drawn inventory leaves, in the draw's order.
    fn drawn_operands(&self, draw: &[u32]) -> Result<Vec<(u32, kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1)>, String> {
        use super::inventory::TirParamOpenerV1;
        draw.iter()
            .map(|i| {
                self.artifact.param_opening(*i).map(|o| (*i, o.operand)).ok_or_else(|| format!("inventory leaf {i} does not open"))
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
        let (ctx, prompt) = tir_job_for_anchor_v1(&self.canonical, self.space.program.token_bound, &anchor, self.prompt_ids_form)
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
        let tree = self.artifact.inventory_tree()?;
        Ok((tree.root(), tree.leaf_count()))
    }

    /// The readiness material from the held inventory tree: its root, every leaf hash, and the drawn
    /// leaves' operands in the draw's order. (The IR inventory is in declaration order, not the
    /// legacy digest's name order, so it is served from the tree, never through a digest.)
    fn artifact_readiness_material(
        &self,
        draw: &[u32],
    ) -> Result<(Hash64, Vec<Hash64>, Vec<(u32, kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1)>), String> {
        let tree = self.artifact.inventory_tree()?;
        Ok((tree.root(), tree.leaves().to_vec(), self.drawn_operands(draw)?))
    }

    fn artifact_readiness_material_streamed_v1(
        &self,
        draw: &[u32],
        on_leaf: &mut dyn FnMut(Hash64),
    ) -> Option<Result<(Hash64, u32, Vec<(u32, kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1)>), String>> {
        Some((|| {
            let tree = self.artifact.inventory_tree()?;
            for leaf in tree.leaves() {
                on_leaf(*leaf);
            }
            Ok((tree.root(), tree.leaf_count(), self.drawn_operands(draw)?))
        })())
    }

    fn output_root_for_context_v1(&self, context: &PalwJobContextV2, output_token_ids: &[u32]) -> Option<Hash64> {
        Some(palw_attempt_output_root_v1(context, output_token_ids))
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
