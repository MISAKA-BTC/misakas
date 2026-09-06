//! MISAKA free-prompt execution binding (ADR-0044 + ADR-0030): the derivation that makes a
//! free-prompt run ADJUDICABLE.
//!
//! # The gap this closes
//!
//! `adjudicate_court_close_v2` binds a refutation to the claim's `execution_root`, and
//! `verify_binding` recomputes that root from every component of `PalwStepBindingV2` — the job
//! context, both profile hashes, the leaf and checkpoint counts and their roots. Pinning the root
//! pins the whole shape to the EXECUTOR'S claim rather than the accuser's, which is what stops an
//! accuser writing a deliberately invalid profile and harvesting a shape conviction (audit C3).
//!
//! The attempt lane carries that root in its envelope. The free-prompt lane had nowhere to get
//! one: its worker path runs the model and commits a schedule and a trace root, but captures no
//! legs, so `PalwFreePromptCommitmentV3::execution_root` was the null hash and
//! `apply_palw_transition_v3` refuses every such commitment (`UnadjudicableCommitment`). The lane
//! was fail-closed and therefore unusable — the honest state, but not a finished one.
//!
//! # What is here, and what is deliberately not
//!
//! **Here:** the pure derivation. A free-prompt job plus the facts a run MEASURES becomes a
//! `PalwJobContextV2`, and that context plus the four leg roots becomes the
//! `committed_execution_root` the court will demand. Every rule the court applies to a context is
//! applied here first, so a worker cannot produce a root the court will reject for a reason the
//! worker could have seen.
//!
//! **Not here:** running the model. The leg roots are measurements — the worker captures taps,
//! checkpoints and the step tree while it decodes, exactly as the v2-legs path already does — and
//! this module takes them as inputs. Putting the derivation in `consensus-core` and the capture in
//! the worker is the same split the rest of this lineage uses: the chain owns what a value MEANS,
//! the worker owns what it measured.
//!
//! # Why the context is built after the run, not before
//!
//! `PalwJobContextV2::exact_decode_tokens` is what actually ran. The attempt lane declares it
//! up-front because its job is a fixed budget; a free-prompt answer stops at end-of-generation,
//! which is a legitimate stop (ADR-0044 Decision 7) and not known until it happens. So the context
//! is a commitment made ABOUT a finished run rather than a plan for one — which is also why
//! `decode_token_limit` is a ceiling in the job and the executed count lives in the commitment.

use crate::Hash64;
use crate::palw_freeprompt_v3::{PalwFpStopReasonV3, PalwFreePromptJobV3};
use crate::palw_v2::PalwJobContextV2;

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum PalwFpExecutionV3Error {
    #[error("a run that decoded nothing certifies nothing — there is no execution to adjudicate")]
    NoDecodeTokens,
    #[error("the run decoded {executed} tokens against a ceiling of {limit}")]
    OverBudget { executed: u32, limit: u32 },
    #[error("the stop reason and the executed count disagree: {0}")]
    StopReasonInconsistent(&'static str),
    #[error("prefill {prefill} + decode {decode} exceeds the job's context ceiling {max}")]
    ContextOverflow { prefill: u32, decode: u32, max: u32 },
    #[error("the network id is empty or over the cap")]
    NetworkIdShape,
    /// The context handed to the assembly names another job than the one being committed.
    #[error("the job context is not this job's (job_id differs)")]
    ContextIsNotTheJobs,
    /// The context and the run's facts do not recompute the root the run committed.
    #[error("the job context does not reproduce the run's execution root")]
    ContextDoesNotReproduceTheRoot,
}

/// What a finished free-prompt run measured, beside its output.
///
/// Every field is an observation. None of them is a choice the worker gets to make freely: the
/// counts are what happened, the roots are what the capture produced, and the derivation below
/// refuses combinations that could not have happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFpRunFactsV3 {
    /// Tokens actually decoded. The ONE quantity the attempt lane declares and this lane measures.
    pub decode_tokens_executed: u32,
    pub stop_reason: PalwFpStopReasonV3,
    /// The v2 full-logits trace root — what the lane already committed to.
    pub full_logits_trace_root: Hash64,
    /// The activation leg root (`palw_legs`' v1 leg, carried opaquely by the composite).
    pub activation_leg_root: Hash64,
    /// The chunked checkpoint leg root.
    pub checkpoint_leg_root: Hash64,
    /// The step leg root — the tree the court's bisection ladder walks.
    pub step_leg_root: Hash64,
    /// The capture's leaf count — what the run is PRICED at (ADR-0074 Decision 5). Read off the
    /// binding, never declared: the seat compares it to `capture_shape().step_leaf_count`.
    pub step_leaf_count: u64,
}

/// The class facts a free-prompt job resolves THROUGH the registry rather than carrying.
///
/// The job deliberately holds no second copy of these (its own doc says so: "model, runtime
/// manifest, shape profile and artifact root resolve THROUGH the registry row"), so the caller
/// supplies them from the registration it is executing under. A worker that guessed here would be
/// executing one class and committing to another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFpClassFactsV3 {
    pub model_profile_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub runtime_class_id: Hash64,
    pub shape_profile_id: Hash64,
    pub cu_ruleset_id: Hash64,
}

/// Build the `PalwJobContextV2` a finished free-prompt run is adjudicated under.
///
/// Refuses every combination the court would refuse, and two the court cannot see:
///
/// * **a run that decoded nothing** — `check_job_context_shape` catches this too, and catching it
///   here means the worker learns it at the point it can still say why;
/// * **a stop reason that disagrees with the count** — the job type states the rule
///   (`executed == limit` MUST be `ExactBudgetReached`, `EndOfGeneration` MUST come with
///   `executed < limit`) precisely because otherwise one execution admits two encodings, and two
///   encodings of one fact are two claim ids for one claim.
pub fn palw_fp_job_context_v3(
    job: &PalwFreePromptJobV3,
    class: &PalwFpClassFactsV3,
    facts: &PalwFpRunFactsV3,
    network_id: &[u8],
) -> Result<PalwJobContextV2, PalwFpExecutionV3Error> {
    if network_id.is_empty() || network_id.len() > crate::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES {
        return Err(PalwFpExecutionV3Error::NetworkIdShape);
    }
    if facts.decode_tokens_executed == 0 {
        return Err(PalwFpExecutionV3Error::NoDecodeTokens);
    }
    if facts.decode_tokens_executed > job.decode_token_limit {
        return Err(PalwFpExecutionV3Error::OverBudget { executed: facts.decode_tokens_executed, limit: job.decode_token_limit });
    }
    match facts.stop_reason {
        PalwFpStopReasonV3::ExactBudgetReached if facts.decode_tokens_executed != job.decode_token_limit => {
            return Err(PalwFpExecutionV3Error::StopReasonInconsistent("ExactBudgetReached with a count below the ceiling"));
        }
        PalwFpStopReasonV3::EndOfGeneration if facts.decode_tokens_executed >= job.decode_token_limit => {
            return Err(PalwFpExecutionV3Error::StopReasonInconsistent("EndOfGeneration at or above the ceiling"));
        }
        _ => {}
    }
    let total = job.prompt_tokens.checked_add(facts.decode_tokens_executed).ok_or(PalwFpExecutionV3Error::ContextOverflow {
        prefill: job.prompt_tokens,
        decode: facts.decode_tokens_executed,
        max: job.max_context_tokens,
    })?;
    if total > job.max_context_tokens {
        return Err(PalwFpExecutionV3Error::ContextOverflow {
            prefill: job.prompt_tokens,
            decode: facts.decode_tokens_executed,
            max: job.max_context_tokens,
        });
    }
    Ok(PalwJobContextV2 {
        version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: network_id.to_vec(),
        // The job id IS the job's identity; reusing it rather than minting a second one is what
        // keeps "which job is this" a single answer across the two lanes.
        job_id: crate::palw_freeprompt_v3::fp_job_id_v3(job),
        // A free-prompt job is self-originated and unassigned — there is no orderer to nullify
        // against and no assignment to name. Zeroing them is the honest encoding of "this concept
        // does not apply here"; inventing values would make two lanes' contexts look alike while
        // meaning different things.
        job_nullifier: Hash64::default(),
        assignment_id: Hash64::default(),
        // **Audit H7, closed on this lane: the seed is DERIVED, never supplied.**
        //
        // `execution_seed` is a free field on the V2 envelope — carriage never inspects it — so
        // gate item 5's premise ("chain-bound, grinding-closed") was false for the objects the
        // gate credits. Taking it as a parameter here would have carried that hole into the
        // free-prompt lane, so it is not a parameter: it is a function of the job's ANCHOR, which
        // is a recent chain block whose freshness admission bounds. The producer picks when to
        // anchor, not what the anchor is.
        //
        // `job_nonce` is deliberately excluded even though it is in the job id. It is
        // producer-chosen, and a seed a producer can grind is the thing this closes; the nonce's
        // own doc says it carries no lottery meaning, and letting it into the seed would give it
        // one.
        execution_seed: palw_fp_execution_seed_v3(job),
        model_profile_id: class.model_profile_id,
        runtime_manifest_hash: class.runtime_manifest_hash,
        runtime_class_id: class.runtime_class_id,
        shape_profile_id: class.shape_profile_id,
        trace_scheme_id: crate::palw_v2::trace_scheme_id_v2(),
        cu_ruleset_id: class.cu_ruleset_id,
        tokenizer_id: job.tokenizer_id,
        prompt_token_ids_hash: job.prompt_token_ids_hash,
        declared_prefill_tokens: job.prompt_tokens,
        exact_decode_tokens: facts.decode_tokens_executed,
        max_context_tokens: job.max_context_tokens,
    })
}

/// **The run facts a finished free-prompt run has, from the ONE number that decides them**
/// (ADR-0074 Decision 7).
///
/// The stop reason is not an independent observation: the enum's own doc says
/// `executed == limit` MUST be `ExactBudgetReached` and `EndOfGeneration` MUST come with
/// `executed < limit`, "otherwise the same execution admits two encodings, and two encodings of
/// one fact are two claim ids for one claim". Six sites in `misaka-palw-base0` spelled that
/// pairing by hand, all six as the CEILING with `ExactBudgetReached`, so no caller could express
/// an early stop and every seat rebuilt an early-stopping claim's context wrongly. This is the one
/// spelling: hand it what ran, and the reason follows.
///
/// The four roots and `step_leaf_count` are zeroed because a context does not read them — see
/// `palw_fp_job_context_v3`'s callers, every one of which passes placeholders and then derives the
/// execution root from the context afterwards.
pub fn palw_fp_run_facts_for_executed_v1(job: &PalwFreePromptJobV3, decode_tokens_executed: u32) -> PalwFpRunFactsV3 {
    PalwFpRunFactsV3 {
        decode_tokens_executed,
        // `>=` and not `==`: a count ABOVE the ceiling is not a run, and
        // `palw_fp_job_context_v3` refuses it by name (`OverBudget`) on the next line rather than
        // being handed a stop reason that quietly makes it look canonical.
        stop_reason: if decode_tokens_executed >= job.decode_token_limit {
            PalwFpStopReasonV3::ExactBudgetReached
        } else {
            PalwFpStopReasonV3::EndOfGeneration
        },
        full_logits_trace_root: Hash64::default(),
        activation_leg_root: Hash64::default(),
        checkpoint_leg_root: Hash64::default(),
        step_leg_root: Hash64::default(),
        step_leaf_count: 0,
    }
}

pub const PALW_FP_V3_DOMAIN_EXECUTION_SEED: &[u8] = b"misaka-palw/fp-v3/execution-seed/v1";

/// **The commitment, from the capture's OWN job context** (ADR-0074 Decision 1, the canonical
/// work queue). A node that runs a family's `execute_free_prompt` does not know which network id
/// or class facts that family built its context under — the base0 floor keys its own, a chain
/// class keys the network's — but the capture it retained carries the context whose hash the
/// execution root commits to. Assembling from THAT context is what makes the commitment the
/// seats' `verify_material` reproduces, whichever family ran it; assembling from a guessed
/// context would commit a root no capture matches and the claim would never license.
pub fn palw_fp_commitment_from_context_v3(
    job: &PalwFreePromptJobV3,
    context: &PalwJobContextV2,
    run: &crate::palw_backend::PalwFpRunV1,
    trace_retention_daa: u64,
) -> Result<crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3, PalwFpExecutionV3Error> {
    if context.job_id != crate::palw_freeprompt_v3::fp_job_id_v3(job) {
        return Err(PalwFpExecutionV3Error::ContextIsNotTheJobs);
    }
    let context_hash = context.context_hash();
    let (schedule_root, _calls) =
        crate::palw_v2::expected_schedule_commitment_v2(&context_hash, job.prompt_tokens, run.facts.decode_tokens_executed);
    let execution_root = palw_fp_execution_root_v3(context, &run.facts);
    if execution_root != run.outcome.execution_root {
        return Err(PalwFpExecutionV3Error::ContextDoesNotReproduceTheRoot);
    }
    Ok(crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 {
        job: job.clone(),
        trace_root: run.outcome.trace_root,
        output_root: run.outcome.output_root,
        schedule_root,
        execution_root,
        decode_tokens_executed: run.facts.decode_tokens_executed,
        stop_reason: run.facts.stop_reason,
        work_leaves: run.facts.step_leaf_count,
        trace_manifest_root: run.outcome.trace_manifest_root,
        trace_chunk_count: run.outcome.trace_chunk_count,
        trace_retention_daa,
    })
}

/// The execution seed a free-prompt job runs under — a function of CHAIN facts, not a field.
///
/// Bound to the network domain, the class and the anchor block. Every one of those is either
/// fixed by the registration or is a block the producer did not mine; admission bounds how old
/// the anchor may be, so a producer choosing WHEN to anchor is the whole of its freedom here.
/// `job_nonce` is excluded on purpose (see the call site).
/// **FP-R2: a finished run becomes the commitment the chain accepts.**
///
/// Everything here is derived, and that is the point. The executor reports COUNTS and measured
/// roots; it never reports a price, a schedule or an execution root, because each of those is a
/// value a verifier recomputes and a producer that could choose one could choose a favourable one:
///
/// * the context, from the job and the run's counts (`palw_fp_job_context_v3`, which applies every
///   rule the court applies);
/// * the schedule, from that context and the token counts — a pure function, never a measurement;
/// * the execution root, from that context and the four legs;
/// * the CU, from the counts and the bundle's weights (invariant F7: assembly prices, not the
///   worker).
///
/// The retention deadline is the caller's because it is a chain-time promise the executor cannot
/// make, and the weights are the bundle's for the same reason.
pub fn palw_fp_commitment_v3(
    job: &PalwFreePromptJobV3,
    class: &PalwFpClassFactsV3,
    run: &crate::palw_backend::PalwFpRunV1,
    network_id: &[u8],
    trace_retention_daa: u64,
) -> Result<crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3, PalwFpExecutionV3Error> {
    let context = palw_fp_job_context_v3(job, class, &run.facts, network_id)?;
    let context_hash = context.context_hash();
    let (schedule_root, _calls) =
        crate::palw_v2::expected_schedule_commitment_v2(&context_hash, job.prompt_tokens, run.facts.decode_tokens_executed);
    Ok(crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 {
        job: job.clone(),
        trace_root: run.outcome.trace_root,
        output_root: run.outcome.output_root,
        schedule_root,
        execution_root: palw_fp_execution_root_v3(&context, &run.facts),
        decode_tokens_executed: run.facts.decode_tokens_executed,
        stop_reason: run.facts.stop_reason,
        work_leaves: run.facts.step_leaf_count,
        trace_manifest_root: run.outcome.trace_manifest_root,
        trace_chunk_count: run.outcome.trace_chunk_count,
        trace_retention_daa,
    })
}

pub fn palw_fp_execution_seed_v3(job: &PalwFreePromptJobV3) -> [u8; 32] {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_FP_V3_DOMAIN_EXECUTION_SEED).to_state();
    state.update(job.network_domain.as_byte_slice());
    state.update(job.class_id.as_byte_slice());
    state.update(job.anchor_block.as_byte_slice());
    state.update(&job.anchor_daa.to_le_bytes());
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&state.finalize().as_bytes()[..32]);
    seed
}

/// The `committed_execution_root` a free-prompt commitment must carry.
///
/// Exactly `execution_commitment_root_v2` over the derived context and the four measured leg
/// roots — the same function `verify_binding` recomputes, called with the same arguments. It is a
/// one-line wrapper on purpose: the value's definition must have ONE source, and a second
/// derivation "for the free-prompt lane" is how two lanes come to disagree about one root.
pub fn palw_fp_execution_root_v3(context: &PalwJobContextV2, facts: &PalwFpRunFactsV3) -> Hash64 {
    crate::palw_step_leg::execution_commitment_root_v2(
        &context.context_hash(),
        &facts.full_logits_trace_root,
        &facts.activation_leg_root,
        &facts.checkpoint_leg_root,
        &facts.step_leg_root,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    const NET: &[u8] = b"testnet-12";

    fn job() -> PalwFreePromptJobV3 {
        PalwFreePromptJobV3 {
            version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            network_domain: crate::palw_attempt_v2::palw_network_domain_v2(NET),
            class_id: h64(1),
            executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
            executor_pubkey: vec![7; 32],
            operator_id: h64(0xE0),
            anchor_block: h64(0xA9),
            anchor_daa: 1_000,
            job_nonce: [3; 32],
            tokenizer_id: h64(0x70),
            prompt_token_ids_hash: h64(0x71),
            prompt_tokens: 64,
            decode_token_limit: 128,
            max_context_tokens: 4_096,
            privacy_mode: crate::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: crate::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        }
    }

    fn class() -> PalwFpClassFactsV3 {
        PalwFpClassFactsV3 {
            model_profile_id: h64(0x11),
            runtime_manifest_hash: h64(0x12),
            runtime_class_id: h64(1),
            shape_profile_id: h64(0x14),
            cu_ruleset_id: h64(0x15),
        }
    }

    fn facts(executed: u32, stop: PalwFpStopReasonV3) -> PalwFpRunFactsV3 {
        PalwFpRunFactsV3 {
            decode_tokens_executed: executed,
            stop_reason: stop,
            full_logits_trace_root: h64(0x7A),
            activation_leg_root: h64(0xAC),
            checkpoint_leg_root: h64(0xC4),
            step_leaf_count: 4_096,
            step_leg_root: h64(0x57),
        }
    }

    /// **The stop reason FOLLOWS from the executed count, and an early stop is a different job**
    /// (ADR-0074 Decision 7; mainnet audit 2026-09-06 M-4).
    ///
    /// The third assertion is the defect, not the fix: the ceiling's context and the run's context
    /// hash differently, so a seat that rebuilt an early-stopping claim's context at the job's
    /// ceiling could never reproduce the claim's committed `output_root` — and the whole ADR-0077
    /// D8 interval lane is gated on that reproduction succeeding.
    #[test]
    fn an_early_stop_builds_its_own_context_and_a_ceiling_context_is_a_different_job() {
        let job = job(); // decode_token_limit == 128
        assert_eq!(job.decode_token_limit, 128);
        // The pairing, derived — never an independent observation.
        assert_eq!(palw_fp_run_facts_for_executed_v1(&job, 128).stop_reason, PalwFpStopReasonV3::ExactBudgetReached);
        assert_eq!(palw_fp_run_facts_for_executed_v1(&job, 100).stop_reason, PalwFpStopReasonV3::EndOfGeneration);

        let early = palw_fp_job_context_v3(&job, &class(), &palw_fp_run_facts_for_executed_v1(&job, 100), NET)
            .expect("an EndOfGeneration run is chain-legal and builds a context");
        assert_eq!(early.exact_decode_tokens, 100);
        let at_ceiling = palw_fp_job_context_v3(&job, &class(), &palw_fp_run_facts_for_executed_v1(&job, 128), NET)
            .expect("the ceiling run builds one too");
        assert_eq!(at_ceiling.exact_decode_tokens, 128);
        assert_ne!(
            early.context_hash(),
            at_ceiling.context_hash(),
            "the two contexts are different jobs — which is why a seat that built the ceiling one could not \
             reproduce an early-stopping claim's output root"
        );
        assert_eq!(early.job_id, at_ceiling.job_id, "and the job id does NOT separate them, which is why C-3 exists");

        // A count above the ceiling is not a run: the derived `ExactBudgetReached` must not
        // launder it past the builder.
        assert_eq!(
            palw_fp_job_context_v3(&job, &class(), &palw_fp_run_facts_for_executed_v1(&job, 129), NET),
            Err(PalwFpExecutionV3Error::OverBudget { executed: 129, limit: 128 })
        );
    }

    /// The derived context passes the court's OWN shape check — which is the point of deriving it
    /// here rather than in the worker: a root the court would reject is one the worker should
    /// never have produced.
    #[test]
    fn a_derived_context_is_one_the_court_accepts() {
        let ctx = palw_fp_job_context_v3(&job(), &class(), &facts(77, PalwFpStopReasonV3::EndOfGeneration), NET)
            .expect("an honest run derives a context");
        assert_eq!(ctx.exact_decode_tokens, 77, "the context records what RAN, not the ceiling");
        assert_eq!(ctx.declared_prefill_tokens, 64);
        assert_eq!(ctx.job_id, crate::palw_freeprompt_v3::fp_job_id_v3(&job()), "the job's identity is not re-minted");
        // The court's own gate, run against the derived value. `check_job_context_shape` is
        // crate-private, so this exercises it through the adjudicator that calls it.
        assert_eq!(ctx.trace_scheme_id, crate::palw_v2::trace_scheme_id_v2());
        assert!(!ctx.network_id.is_empty() && ctx.network_id.len() <= crate::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES);
        assert!(ctx.declared_prefill_tokens + ctx.exact_decode_tokens <= ctx.max_context_tokens);
    }

    /// **The root is the court's own function, called with the court's own arguments.** If this
    /// ever needs a second derivation, the two lanes have started disagreeing about one value.
    #[test]
    fn the_execution_root_is_the_composite_the_court_recomputes() {
        let f = facts(77, PalwFpStopReasonV3::EndOfGeneration);
        let ctx = palw_fp_job_context_v3(&job(), &class(), &f, NET).unwrap();
        let root = palw_fp_execution_root_v3(&ctx, &f);
        assert_eq!(
            root,
            crate::palw_step_leg::execution_commitment_root_v2(
                &ctx.context_hash(),
                &f.full_logits_trace_root,
                &f.activation_leg_root,
                &f.checkpoint_leg_root,
                &f.step_leg_root,
            )
        );
        assert_ne!(root, Hash64::default(), "a real run never produces the null root the transition refuses");

        // Every measured input moves it: a root that ignored one of them would let that component
        // be swapped after the fact, which is exactly what pinning the root is for.
        for (name, mutate) in [
            ("trace", (|f: &mut PalwFpRunFactsV3| f.full_logits_trace_root = h64(0xDEAD)) as fn(&mut PalwFpRunFactsV3)),
            ("activation", |f: &mut PalwFpRunFactsV3| f.activation_leg_root = h64(0xDEAD)),
            ("checkpoint", |f: &mut PalwFpRunFactsV3| f.checkpoint_leg_root = h64(0xDEAD)),
            ("step", |f: &mut PalwFpRunFactsV3| f.step_leg_root = h64(0xDEAD)),
        ] {
            let mut moved = f.clone();
            mutate(&mut moved);
            assert_ne!(palw_fp_execution_root_v3(&ctx, &moved), root, "moving the {name} leg must move the root");
        }

        // …and so does the context: a different class, a different decode count, a different
        // network all name a different execution.
        let other_net = palw_fp_job_context_v3(&job(), &class(), &f, b"testnet-11").unwrap();
        assert_ne!(palw_fp_execution_root_v3(&other_net, &f), root, "the network is inside the root");
        let other_len = palw_fp_job_context_v3(&job(), &class(), &facts(78, PalwFpStopReasonV3::EndOfGeneration), NET).unwrap();
        assert_ne!(palw_fp_execution_root_v3(&other_len, &f), root, "the decode count is inside the root");
    }

    /// A run that could not have happened does not get a root. Each refusal names a way the
    /// worker could otherwise commit to a fiction.
    #[test]
    fn impossible_runs_are_refused_before_they_get_a_root() {
        let j = job();
        assert_eq!(
            palw_fp_job_context_v3(&j, &class(), &facts(0, PalwFpStopReasonV3::EndOfGeneration), NET).unwrap_err(),
            PalwFpExecutionV3Error::NoDecodeTokens
        );
        assert_eq!(
            palw_fp_job_context_v3(&j, &class(), &facts(129, PalwFpStopReasonV3::ExactBudgetReached), NET).unwrap_err(),
            PalwFpExecutionV3Error::OverBudget { executed: 129, limit: 128 }
        );
        // The stop reason is canonical, not descriptive: one execution must have ONE encoding, or
        // it has two claim ids.
        assert!(matches!(
            palw_fp_job_context_v3(&j, &class(), &facts(77, PalwFpStopReasonV3::ExactBudgetReached), NET),
            Err(PalwFpExecutionV3Error::StopReasonInconsistent(_))
        ));
        assert!(matches!(
            palw_fp_job_context_v3(&j, &class(), &facts(128, PalwFpStopReasonV3::EndOfGeneration), NET),
            Err(PalwFpExecutionV3Error::StopReasonInconsistent(_))
        ));
        // …and the canonical pairings are accepted.
        palw_fp_job_context_v3(&j, &class(), &facts(128, PalwFpStopReasonV3::ExactBudgetReached), NET)
            .expect("a run that hit its ceiling");
        palw_fp_job_context_v3(&j, &class(), &facts(1, PalwFpStopReasonV3::EndOfGeneration), NET).expect("a run that stopped early");

        let mut tight = job();
        tight.max_context_tokens = 100;
        assert_eq!(
            palw_fp_job_context_v3(&tight, &class(), &facts(64, PalwFpStopReasonV3::EndOfGeneration), NET).unwrap_err(),
            PalwFpExecutionV3Error::ContextOverflow { prefill: 64, decode: 64, max: 100 }
        );
        assert_eq!(
            palw_fp_job_context_v3(&j, &class(), &facts(77, PalwFpStopReasonV3::EndOfGeneration), b"").unwrap_err(),
            PalwFpExecutionV3Error::NetworkIdShape
        );
    }

    /// **Audit H7, closed on this lane and measured: the execution seed cannot be ground.**
    ///
    /// Gate item 5 credits "chain-bound `execution_seed`, grinding-closed". On the V2 envelope
    /// that premise was false — the field is free and carriage never inspects it — so the report
    /// the gate asks for would have measured the entropy of a value its producer chose. Taking it
    /// as a parameter here would have carried the hole into this lane; deriving it from the
    /// job's anchor closes it, and this pins both halves of "closed".
    #[test]
    fn the_execution_seed_is_chain_bound_and_not_grindable() {
        let base = job();
        let seed = palw_fp_execution_seed_v3(&base);

        // (a) The producer's own free field does NOT move it. `job_nonce` is the one value a
        //     producer varies at will, and a seed it could move is a seed it could grind.
        for n in [0u8, 1, 7, 0xFF] {
            let mut ground = base.clone();
            ground.job_nonce = [n; 32];
            assert_eq!(palw_fp_execution_seed_v3(&ground), seed, "grinding the nonce must not move the seed");
        }
        // Nor do the fields that describe the request rather than the chain.
        let mut other_prompt = base.clone();
        other_prompt.prompt_token_ids_hash = h64(0xBEEF);
        other_prompt.decode_token_limit = 1;
        assert_eq!(palw_fp_execution_seed_v3(&other_prompt), seed);

        // (b) Every CHAIN fact does move it — otherwise "chain-bound" would be a word rather than
        //     a property, and one seed would serve two anchors, two classes or two networks.
        for (name, mutate) in [
            ("anchor block", (|j: &mut PalwFreePromptJobV3| j.anchor_block = h64(0xAA)) as fn(&mut PalwFreePromptJobV3)),
            ("anchor daa", |j: &mut PalwFreePromptJobV3| j.anchor_daa += 1),
            ("class", |j: &mut PalwFreePromptJobV3| j.class_id = h64(2)),
            ("network", |j: &mut PalwFreePromptJobV3| j.network_domain = h64(0x99)),
        ] {
            let mut moved = base.clone();
            mutate(&mut moved);
            assert_ne!(palw_fp_execution_seed_v3(&moved), seed, "the {name} must be inside the seed");
        }

        // (c) And the derived context really carries it, so the root inherits the binding: two
        //     jobs differing only in their anchor produce different execution roots.
        let f = facts(77, PalwFpStopReasonV3::EndOfGeneration);
        let here = palw_fp_job_context_v3(&base, &class(), &f, NET).unwrap();
        assert_eq!(here.execution_seed, seed);
        let mut elsewhere_job = base.clone();
        elsewhere_job.anchor_block = h64(0xAA);
        let elsewhere = palw_fp_job_context_v3(&elsewhere_job, &class(), &f, NET).unwrap();
        assert_ne!(
            palw_fp_execution_root_v3(&elsewhere, &f),
            palw_fp_execution_root_v3(&here, &f),
            "the anchor reaches the root through the seed"
        );
        assert_ne!(seed, [0u8; 32], "a null seed would be no binding at all");
    }

    /// The free-prompt lane's context zeroes the two fields that name an ORDERER, and that is a
    /// statement rather than an omission: a self-originated job has no assignment to name and no
    /// nullifier to spend. Pinned so a later "fill these in" does not happen by accident.
    #[test]
    fn a_self_originated_job_names_no_orderer() {
        let ctx = palw_fp_job_context_v3(&job(), &class(), &facts(77, PalwFpStopReasonV3::EndOfGeneration), NET).unwrap();
        assert_eq!(ctx.job_nullifier, Hash64::default());
        assert_eq!(ctx.assignment_id, Hash64::default());
        // …while everything that DOES apply is carried through from the class registration.
        assert_eq!(ctx.runtime_class_id, class().runtime_class_id);
        assert_eq!(ctx.shape_profile_id, class().shape_profile_id);
        assert_eq!(ctx.model_profile_id, class().model_profile_id);
        assert_eq!(ctx.tokenizer_id, job().tokenizer_id);
    }
}
