//! **The execution-backend seam** (ADR-0053; the adapter surface ADR-0026 adopted from Ambient
//! and never made a type).
//!
//! A PALW node does three things with an execution and knows, at each of them, only what the CHAIN
//! told it: run the job this template implies, commit to what it ran, and — as a panel seat —
//! decide whether somebody else's material answers for the claim they published. This trait is
//! where those three verbs live, so that no consumer names a runtime crate directly.
//!
//! # One execution family, and why the seam survives it
//!
//! ADR-0051 proposed a second *family* — a verification scheme, not a model: a pinned GGUF under a
//! pinned Metal runtime, committing what the inference SAID and verified by tolerant replay.
//! **ADR-0053 withdraws it.** A tolerance can acquit but never convict, so half the economy would
//! have been non-convictable work, and the three mechanisms that were supposed to bound that —
//! the 500‰ family cap, the per-class panel, the court exclusion — were respectively never
//! constructed, never consumed, and a runtime `if`. What removed the motive was measurement:
//! Qwen3.6 runs in the integer runtime with 100 % kernel-catalog coverage, so the model the black
//! box existed to serve is adjudicable without it.
//!
//! So there is exactly one family — pinned integer arithmetic, a graph projected from a canonical
//! IR, disputes ending in the ADR-0049 court — and it is not a value any object carries. **Every
//! registered class is court-adjudicable by construction**, which is a stronger statement than a
//! flag that says so: there is no arm to get wrong.
//!
//! # What this trait deliberately does not abstract
//!
//! Not the court's rules — a backend supplies evidence (`bisect_prefix_state`,
//! `refutation_for_index`) and never a verdict. Not the artifact format either: the floor DERIVES
//! its weights from a seed and a converted class ships a file, which
//! `misaka_palw_base0::classes::resolve_class_v1` already reconciles against the chain's
//! `(class_id, artifact_root)` pair.

use crate::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// The roots an attempt carries, and the bytes that answer for them.
///
/// Family-agnostic on purpose: a block header commits these four values and does not know which
/// backend produced them. What differs between families is how `material` is checked, not what
/// the header holds.
pub struct PalwExecutionOutcomeV1 {
    /// The logits leg. Which SCHEME this root is under is a class fact — the integer family
    /// commits `base0_logits_trace_root_v1`, the float families the v2 event-tree root — and the
    /// dispatch on the class's registered lane already exists in `palw_step_refute`.
    pub trace_root: Hash64,
    pub output_root: Hash64,
    /// The composite the court (where there is one) pins a refutation's binding against.
    pub execution_root: Hash64,
    pub trace_manifest_root: Hash64,
    pub trace_chunk_count: u32,
    /// What the producer retains for `trace_retention_daa` and broadcasts to its panel. Opaque
    /// here: only the backend that wrote it can read it, which is the point of the seam.
    pub material: Vec<u8>,
}

/// The two roots a seat checks material against — the claim's own, read from chain state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClaimRootsV1 {
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    /// **The job this claim's block asked for**, derived from the block — never read off the
    /// capture being checked.
    ///
    /// The roots alone say "this material computes to what the claim announced". They do not say
    /// WHICH question was answered, and the anchor is the question: it is a pure function of the
    /// claim's own block (its pre-PoW hash), its network, its class and its executor bond. Without
    /// it a gossiped capture is a re-usable asset — anyone can mine a fresh block, announce the
    /// borrowed roots, and both halves of the verification agree, because a seat compares roots and
    /// a challenger re-executes the anchor the capture itself names. One inference, unlimited
    /// blocks, by parties that ran nothing.
    ///
    /// `Hash64::default()` means "this caller has no block to bind to" and skips the check — the
    /// producer checking its own fresh run, and the fixtures. Every path that judges SOMEBODY
    /// ELSE's material must supply it.
    pub anchor: Hash64,
}

/// What a seat concluded about served material.
///
/// Three outcomes and not two, because "I could not verify" and "this does not match" are
/// different accusations and the receipt lane already distinguishes them: the first is
/// `Unavailable` against the producer's data-availability obligation, the second is a claim that
/// simply gathers no quorum and voids. Collapsing them would either accuse an honest producer of
/// withholding or let a mismatch pass as a network hiccup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwMaterialVerdictV1 {
    /// The material answers for the claim's committed roots.
    Matches,
    /// It decoded and does not answer for them.
    Mismatch,
    /// It could not be checked at all (undecodable, wrong family, unavailable operand).
    Unverifiable,
}

/// **One free-prompt run, as the lane's callers need it.**
///
/// Three values rather than a tuple because they answer three different questions and only one of
/// them is the chain's: `outcome` is what a panel serves and a court reads, `facts` is what
/// `palw_fp_job_context_v3` derives a binding from, and `output_token_ids` is **the answer** — the
/// reason a person ran this at all. An execution path that returned only the first two would price
/// the work and lose the product, which is the failure the free-prompt lane exists to avoid: one
/// inference, both halves.
pub struct PalwFpRunV1 {
    pub outcome: PalwExecutionOutcomeV1,
    pub facts: crate::palw_fp_execution_v3::PalwFpRunFactsV3,
    /// What the model produced, in token ids. Ids and not text: a family without a tokenizer has
    /// no rendering to give, and ids are the execution identity in either case (v2 design §10.7).
    pub output_token_ids: Vec<u32>,
}

/// What a seat concluded about ONE opened checkpoint interval (ADR-0077 Decision 8).
///
/// Four outcomes, because they are four different accusations: `Valid` is every replayed row
/// equal to its committed hash; `Fault` is a row that is not — the court's question, carried
/// with the leaf a challenger opens at; `Mismatch` is an opening that does not bind to THIS
/// claim's roots at all (a forgery, or the wrong claim), which is the same as nothing served;
/// `Unverifiable` is bytes this family cannot read. A seat's verdict convicts nobody — conviction
/// runs only through the court's bisection to one leaf (ADR-0026, ADR-0028).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFpIntervalVerdictV1 {
    Valid,
    Fault { leaf_index: u64 },
    Mismatch,
    Unverifiable,
}

/// The execution path, as a node uses it.
///
/// Implementor: `misaka_palw_base0::backend::Base0Backend`. The trait stays a trait because the
/// consumers must not name that crate — a producer, a seat and the court reach an execution
/// through the same three verbs, and a second implementor is a test double, not a second family.
/// **What a capture SAYS it ran — read off its binding, never off a duty** (ADR-0073 Decision
/// 1e). The job context whose hash the execution root commits to, and the size of its step space.
/// A seat that has verified the capture against the claim's roots prices the claim from
/// `declared_prefill_tokens` / `exact_decode_tokens` here — the shape the execution root binds,
/// rather than the shape the commitment declared — and draws its sampled leaves from
/// `step_leaf_count`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCaptureShapeV1 {
    pub job_context: PalwJobContextV2,
    pub step_leaf_count: u64,
}

pub trait PalwExecutionBackendV1: Send + Sync {
    /// A human-readable identity for logs — the model id for a converted class, the floor's name
    /// for the derived one. Never used for dispatch: the chain's `class_id` is.
    fn model_id(&self) -> &str;

    /// **The job this anchor implies.** A producer must not choose its own prompt — a class whose
    /// executor picks the input is a class where "run the model" and "find an input whose output I
    /// like" are the same move — so the job is derived from the template's anchor, and this is
    /// where that derivation lives for each family.
    fn job_for_anchor(&self, anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String>;

    /// Run the job and commit to it. Pure CPU/GPU work with no chain access: the caller runs it off
    /// the async runtime.
    fn execute(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String>;

    /// **The free-prompt lane's run — the one verb whose tokens the caller chooses.**
    ///
    /// `job_for_anchor` states why an executor must not pick the attempt lane's input: a class
    /// whose executor chooses the prompt is a class where "run the model" and "find an input whose
    /// output I like" are the same move. That rule is not relaxed here; it is answered by
    /// different machinery. A free-prompt win is a quantum ticket against the class's receipt
    /// target (`palw_fp_admission_v3` item 5) — not a property of the output — the claim binds a
    /// beacon it cannot have chosen (item 3), its use window is fixed (item 4), and nothing pays
    /// until the claim certifies (item 1). Grinding the prompt buys none of that.
    ///
    /// It is a SEPARATE verb for the same reason. An `execute` that accepted arbitrary tokens and
    /// was reachable from the attempt path would be exactly the hole the rule closes, so the
    /// attempt path keeps a verb whose prompt it cannot supply.
    ///
    /// Returns [`PalwFpRunV1`]: the outcome a panel and a court consume, the facts the derivation
    /// needs, and the answer itself.
    ///
    /// **The run must be performed under the context the derivation produces**, not under a
    /// convenience context that resembles it. `palw_fp_execution_root_v3` recomputes the root the
    /// court demands from that context, so an execution carried out under any other one commits a
    /// root nobody can reproduce — an honest producer, unconvictable and unpayable.
    ///
    /// Defaulted to a refusal: a family that has not implemented this has no free-prompt path, and
    /// saying so is better than a default that silently produces something the court cannot read.
    fn execute_free_prompt(
        &self,
        _job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        _prompt_tokens: &[usize],
    ) -> Result<PalwFpRunV1, String> {
        Err("this backend has no free-prompt path".to_string())
    }

    /// **A seat's check, before it signs.** Never a conviction — a mismatch is the court's to
    /// convict; a seat that disagrees signs nothing on the merits and the claim voids for want of
    /// a quorum.
    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1;

    /// The shape a capture commits to — see [`PalwCaptureShapeV1`]. `None` when the bytes are not
    /// this family's capture. Meaningful only AFTER [`Self::verify_material`] has said `Matches`
    /// for the claim in hand: an unverified capture states whatever its author likes.
    fn capture_shape(&self, _material: &[u8]) -> Option<PalwCaptureShapeV1> {
        None
    }

    /// **A party's answer at one rung of the bisection: its execution's state at `index`.**
    ///
    /// The ladder converges only if this is a PREFIX commitment — two executions agreeing through
    /// `index` must agree here, and two differing before it must not — because that is what makes
    /// "the first index we disagree on" the same as "the first leaf our executions differ at".
    ///
    /// `None` is the honest answer for material this backend cannot read. A silent party loses its
    /// rung, which is the correct outcome for a party that cannot substantiate its own execution.
    fn bisect_prefix_state(&self, _material: &[u8], _index: u64) -> Option<Hash64> {
        None
    }

    /// **Can this backend take a court's turn at all?**
    ///
    /// `false` by default, because the two methods above are defaulted: a family that has not
    /// implemented them cannot disclose at a rung and cannot assemble a close, so a dispute about
    /// one of its claims can never leave round 0 whichever party is honest (audit3 H4). That was
    /// invisible — it read as an ordinary silence — and it decided real money: the chain charged
    /// the ACCUSER for it, so accusing such a class was a guaranteed loss and its arithmetic was
    /// unpunishable.
    ///
    /// The charge is gone (`rearm_after_unanswered_opening`), but the underlying fact is not, and
    /// a node should say it out loud at startup rather than let an operator discover it from a
    /// court that never resolves. Declared rather than probed, because probing would need a
    /// capture and there is none at boot.
    fn supports_court(&self) -> bool {
        false
    }

    /// **The terminal move's evidence: everything the court needs to recompute step `index`.**
    ///
    /// Returned by BOTH sides, and deliberately the same call for both: an honest executor closing
    /// its own case and a challenger closing a real fraud assemble the identical object, and
    /// `adjudicate_court_close_v2` is what decides which way it reads. A prover that could only be
    /// run by one side would be a prover that decides the verdict.
    ///
    /// `Err` for an index this capture cannot open.
    fn refutation_for_index(
        &self,
        _material: &[u8],
        _index: u64,
    ) -> Result<crate::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        Err("this backend cannot open a refutation at that index".to_string())
    }

    /// **The anchor a block asks its job of** — the family's own derivation, from chain facts only.
    ///
    /// Every input is recomputable by anyone holding the block: the network domain, the header's
    /// pre-PoW hash, the class, and the executor's bond outpoint. That is what makes the anchor
    /// checkable rather than merely declared — a producer cannot choose it, and a party verifying
    /// somebody else's claim derives the same value the producer was forced to use.
    ///
    /// **The default is the shared derivation, deliberately** — the producer computes the anchor
    /// before it resolves a backend at all, so every family already runs the job this names. A
    /// `None` default would have been the quiet kind of wrong: a family that simply never
    /// implemented the method would answer "I cannot derive it", every seat would decline to judge
    /// its claims, and the class would stop licensing with nothing in any log saying why.
    ///
    /// A family that genuinely derives its job differently overrides this. `None` is reserved for
    /// a family with no canonical job at all, and a caller that gets it must decline to judge
    /// rather than fall back to the anchor named inside the material — that is the thing under
    /// test.
    /// `nonce_bucket` is `palw_nonce_bucket_v1(header.nonce)` — the execution the block's nonce was
    /// supposed to be paid for by (ADR-0071 Decision 2). A verifier reads it off the accepted
    /// header it already fetched, so it is a fact about the claim like the other four and not
    /// something the material under judgement gets to say.
    /// **The prover's refutation for leaf `index` of a FREE-PROMPT capture** (ADR-0073 Decision
    /// 1c) — the same object [`Self::refutation_for_index`] assembles, for a capture whose prompt
    /// the caller chose.
    ///
    /// An attempt's prompt is a pure function of the job's anchor, so that prover re-derives it and
    /// a carried copy would only be a second place to disagree with the chain. A free-prompt job's
    /// prompt is the user's: it exists on chain (the 0x4a payload) and in the job material,
    /// hash-bound to the binding's `job_context.prompt_token_ids_hash`, and derives from nothing —
    /// so it is an INPUT here. An implementation MUST refuse ids that do not hash to the binding's
    /// own value rather than pass them through: the court reads a wrong list as
    /// `InputSetNotCanonical`, which is no verdict, and a close that assembles but never adjudicates
    /// is a stalled court that the backstop settles against whoever was owed the move.
    ///
    /// Default: refused. A family with no free-prompt path has no capture this could open.
    fn refutation_for_free_prompt_index(
        &self,
        _material: &[u8],
        _index: u64,
        _prompt_token_ids: &[u32],
    ) -> Result<crate::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        Err("this backend cannot open a free-prompt refutation at that index".to_string())
    }

    fn job_anchor_v1(
        &self,
        network_domain: Hash64,
        pre_pow_hash: Hash64,
        class_id: Hash64,
        executor_bond: &crate::tx::TransactionOutpoint,
        nonce_bucket: u64,
    ) -> Option<Hash64> {
        Some(crate::palw_attempt_v2::palw_job_anchor_v1(network_domain, pre_pow_hash, class_id, executor_bond, nonce_bucket))
    }

    /// **The weight rows that refutation reads — exactly those, proven against the class root.**
    ///
    /// The court recomputes the disputed step, and recomputing it means reading operands out of
    /// the registered artifact. It holds no weights of its own, so a close must carry them; and it
    /// refuses any row that does not prove against the `artifact_root` the class registered under,
    /// so carrying the wrong ones is the same as carrying none.
    ///
    /// **Asked of the adjudicator, never enumerated here.** The set of rows a step reads is decided
    /// by the arithmetic the adjudicator walks, and a second enumeration written on the prover side
    /// would be a second opinion about that — one that agrees today and diverges the first time a
    /// kernel changes which operand it touches, in the direction where an honest producer cannot
    /// close. So the implementation runs the real adjudicator against the full inventory through a
    /// recording oracle and opens what it actually resolved. Opening the whole inventory instead
    /// would be correct and unaffordable: a close has a byte ceiling, and a class's weights do not
    /// fit under it.
    ///
    /// `Err` for a family with no court and for an artifact this backend cannot root.
    fn operand_openings_for(
        &self,
        _refutation: &crate::palw_step_refute::PalwExecutionStepRefutationV1,
    ) -> Result<Vec<crate::palw_artifact::PalwArtifactOpeningV1>, String> {
        Err("this execution family cannot be adjudicated".to_string())
    }

    /// **The free-prompt run, streamed** (ADR-0077 Decision 2): `on_token` is called with each
    /// generated id in decode order, as soon as it is selected, from the SAME run whose capture
    /// and commitment the returned [`PalwFpRunV1`] carries — never from a second inference. The
    /// default runs the non-streaming verb and replays the ids afterwards, which is correct and not
    /// streaming; a family overrides it inside its decode loop.
    fn execute_free_prompt_streaming(
        &self,
        job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        on_token: &mut dyn FnMut(u32),
    ) -> Result<PalwFpRunV1, String> {
        let run = self.execute_free_prompt(job, prompt_tokens)?;
        for id in &run.output_token_ids {
            on_token(*id);
        }
        Ok(run)
    }

    /// **How many checkpoint intervals a retained free-prompt capture has** (ADR-0077
    /// Decision 8). Interval 0 is the prefill and the calls up to the first checkpoint (replayed
    /// from genesis — the prompt); interval `j ≥ 1` is the calls after checkpoint `j − 1`, replayed
    /// from that checkpoint's state chunks. `None` when this family cannot read the bytes.
    fn fp_interval_count(&self, _capture: &[u8]) -> Option<u32> {
        None
    }

    /// **The same count, from CHAIN data alone** — the job's prompt length and the commitment's
    /// executed decode count (both on the accepted 0x4a payload) and this family's own checkpoint
    /// interval for the class. A seat draws its intervals from this, never from a count the
    /// executor reports: an executor that could shrink the count could predict the draw. Must
    /// agree with [`Self::fp_interval_count`] on every capture this family produces (a test
    /// pins it). `None` when this family has no free-prompt path.
    fn fp_interval_count_for(&self, _prompt_tokens: u32, _decode_tokens_executed: u32) -> Option<u32> {
        None
    }

    /// **The executor's side of Decision 8: open interval `index` of its own retained capture.**
    ///
    /// The opening carries the binding (what `verify_binding_v1` authenticates against the
    /// claim's execution root), the checkpoint chunk at the interval's start opened against the
    /// checkpoint leg root, the range opening of the interval's leaves against the step leg root,
    /// and the ids the interval consumed and produced opened against the prompt hash and the
    /// decode pin — never the capture whole. Opaque bytes: only the family that wrote them reads
    /// them, which is the point of the seam. Bytes are `O(interval × row + log₂ leaves)`,
    /// independent of the job's length (R1).
    fn open_fp_interval(&self, _capture: &[u8], _index: u32, _prompt_token_ids: &[u32]) -> Result<Vec<u8>, String> {
        Err("this execution family cannot open a checkpoint interval".to_string())
    }

    /// **The seat's side of Decision 8: replay one opened interval and compare every row
    /// EXACTLY.** The class is a pinned integer computation, so "close" is not a verdict. The
    /// seat first binds the opening to `claim` (execution root, trace root, the FP job id as the
    /// anchor) and prices it (`work_leaves` must equal the binding's `step_leaf_count`), then
    /// restores the state from the opened chunk, runs the interval with the class's own kernels,
    /// and checks the recomputed leaves against the committed range. Fetching and hashing the whole
    /// capture is what this replaces.
    fn verify_fp_interval_opening(
        &self,
        _opening: &[u8],
        _claim: PalwClaimRootsV1,
        _index: u32,
        _prompt_token_ids: &[u32],
        _work_leaves: u64,
    ) -> PalwFpIntervalVerdictV1 {
        PalwFpIntervalVerdictV1::Unverifiable
    }

    /// **ADR-0082 Decision 9: the seat's cache is RECOMPUTED from the prompt it holds, never
    /// fetched.** Run the job's prefill and its first `decode_calls` decode calls (the committed
    /// output ids, teacher-forced) with this family's own kernels, and return the checkpoint
    /// leg's committed root for the state at that point — the tiled cache root under the
    /// class's state chunk map — so a seat can compare 64 bytes against the executor's
    /// checkpoint instead of fetching a history that grows with the context. Compute is one
    /// forward pass of the job; bytes are none. Defaulted to a refusal: a family without this
    /// verb cannot seat a graph-v5 row, and `Incapable` is the honest verdict.
    ///
    /// **`covered` is the checkpoint LEAF's own counter, in the unit the CLASS's cadence counts
    /// in** — `palw_checkpoint_covered_at_index_v1`'s value, which is a decode call on a per-call
    /// class and a cache POSITION on a per-position one. It is the number an interval opening
    /// carries, so it is the number the seam takes; a seam that took decode calls would have every
    /// caller convert, and the caller that forgot would compare a `prefill + covered`-position
    /// state against a `covered`-position root — audit B's C-2, and a guaranteed mismatch on every
    /// honest graph-v5 claim. The family converts once, with
    /// `palw_checkpoint_positions_at_v1`.
    fn fp_recompute_checkpoint_root(
        &self,
        _job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        _prompt_token_ids: &[u32],
        _output_token_ids: &[u32],
        _covered: u32,
    ) -> Result<crate::Hash64, String> {
        Err("this execution family cannot recompute a checkpoint root from the prompt".to_string())
    }

    /// **The largest `covered` a checkpoint leaf of this job's leg can carry** — the cheap half of
    /// the guard a panel applies before it spends a forward pass on an opening's claimed anchor.
    ///
    /// In the CLASS's cadence unit, like every other `covered` on this seam: `decode_calls` on a
    /// per-call class, `prefill + decode_calls` on a per-position one. The default is the per-call
    /// rule spelled out — a DECLARED choice rather than a panel-side assumption, and the honest
    /// one for a family whose classes all run that cadence. A family with a per-position class
    /// overrides it, and one that did not would have its panel refuse every honest opening of that
    /// class before recomputing anything (audit B, C-2).
    ///
    /// This is a bound and not the rule: `verify_fp_interval_opening` re-derives the anchor from
    /// the geometry and refuses an opening that named another one, so a loose bound costs a
    /// forward pass and a tight one costs the quorum.
    fn fp_checkpoint_covered_bound_v1(&self, job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3) -> u32 {
        job.decode_token_limit.saturating_sub(1)
    }

    /// **The committed output ids of a retained capture, read by the family that wrote it**
    /// (ADR-0082 Decision 9's companion verb). A seat that recomputes the cache teacher-forces the
    /// executor's own answer, so it needs the ids the commitment binds — and it must get them
    /// through the seam, never by naming a family's material decoder from the panel. `None` when
    /// the bytes are not this family's capture. Defaulted so a family without a free-prompt path
    /// answers honestly.
    fn fp_committed_output_ids(&self, _capture: &[u8]) -> Option<Vec<u32>> {
        None
    }

    /// **The claim's `output_root`, recomputed from the answer's ids** (ADR-0084 Decision 1;
    /// ADR-0078 X6's rule, `output_commitment_v2(job_context_hash, ids, rendered_hash)`, spelled by
    /// the family that holds the context and the rendered-hash rule).
    ///
    /// This is how a seat binds a served answer envelope (`FPA1`) to the chain before it spends a
    /// forward pass on its ids: the root the ids recompute must equal the root the claim
    /// committed. The context is built exactly as [`Self::fp_recompute_checkpoint_root`] builds
    /// it — the executed count is the job's budget and the stop reason is `ExactBudgetReached`,
    /// which is the one shape this lane's runs have — so an honest claim's ids recompute its root
    /// and a forged list never does. `None` when this family has no free-prompt path; a caller
    /// then holds no binding and files nothing on those ids.
    fn fp_output_root_v1(&self, _job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3, _output_token_ids: &[u32]) -> Option<crate::Hash64> {
        None
    }

    /// **A DRILL fault: run the job, corrupt one lane of one tile, and commit to the result.**
    ///
    /// A court that has never convicted on a live chain is a court nobody has evidence works, and
    /// the only way to get that evidence is for some producer to actually be wrong. Re-deriving
    /// the commitment from the corrupted capture is what makes this a real fraud rather than a
    /// mismatch: the producer's roots are self-consistent and honestly its own, and the ONLY way
    /// to catch it is to run the canonical job yourself — which is exactly the fraud the court
    /// exists for and exactly the one no seat check can see.
    ///
    /// Callers must refuse to reach this on a network carrying value. It is a method rather than a
    /// test helper because the drill has to go through the same production path the honest
    /// producer does; a fault injected somewhere else would prove something about the injector.
    fn execute_with_injected_fault(
        &self,
        _job: &PalwJobContextV2,
        _prompt: &[usize],
        _leaf_index: u64,
    ) -> Result<PalwExecutionOutcomeV1, String> {
        Err("this backend has no drill fault".to_string())
    }
}

#[cfg(test)]
mod tests {
    /// **There is no family value to get wrong.** The type that used to answer "can a dispute
    /// about this class end in a conviction?" is gone (ADR-0053), and this test is what keeps it
    /// gone: a re-introduced flag would give some future consumer an arm to take, and the arm the
    /// withdrawn family needed was the one that skipped the coverage gate.
    #[test]
    fn the_seam_carries_no_verification_scheme_flag() {
        let src = include_str!("palw_backend.rs");
        for banned in ["PalwExecutionFamilyV1", "is_court_adjudicable", "MetalGguf"] {
            assert!(
                !src.split("fn the_seam_carries_no_verification_scheme_flag").next().unwrap().contains(banned),
                "{banned} is back in the execution seam — ADR-0053 withdrew the second family"
            );
        }
    }
}
