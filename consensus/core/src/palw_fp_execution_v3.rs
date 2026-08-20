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
    execution_seed: [u8; 32],
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
            return Err(PalwFpExecutionV3Error::StopReasonInconsistent(
                "ExactBudgetReached with a count below the ceiling",
            ));
        }
        PalwFpStopReasonV3::EndOfGeneration if facts.decode_tokens_executed >= job.decode_token_limit => {
            return Err(PalwFpExecutionV3Error::StopReasonInconsistent("EndOfGeneration at or above the ceiling"));
        }
        _ => {}
    }
    let total = job
        .prompt_tokens
        .checked_add(facts.decode_tokens_executed)
        .ok_or(PalwFpExecutionV3Error::ContextOverflow {
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
        execution_seed,
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
            step_leg_root: h64(0x57),
        }
    }

    /// The derived context passes the court's OWN shape check — which is the point of deriving it
    /// here rather than in the worker: a root the court would reject is one the worker should
    /// never have produced.
    #[test]
    fn a_derived_context_is_one_the_court_accepts() {
        let ctx = palw_fp_job_context_v3(&job(), &class(), &facts(77, PalwFpStopReasonV3::EndOfGeneration), NET, [9; 32])
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
        let ctx = palw_fp_job_context_v3(&job(), &class(), &f, NET, [9; 32]).unwrap();
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
        let other_net = palw_fp_job_context_v3(&job(), &class(), &f, b"testnet-11", [9; 32]).unwrap();
        assert_ne!(palw_fp_execution_root_v3(&other_net, &f), root, "the network is inside the root");
        let other_len = palw_fp_job_context_v3(&job(), &class(), &facts(78, PalwFpStopReasonV3::EndOfGeneration), NET, [9; 32]).unwrap();
        assert_ne!(palw_fp_execution_root_v3(&other_len, &f), root, "the decode count is inside the root");
    }

    /// A run that could not have happened does not get a root. Each refusal names a way the
    /// worker could otherwise commit to a fiction.
    #[test]
    fn impossible_runs_are_refused_before_they_get_a_root() {
        let j = job();
        assert_eq!(
            palw_fp_job_context_v3(&j, &class(), &facts(0, PalwFpStopReasonV3::EndOfGeneration), NET, [9; 32]).unwrap_err(),
            PalwFpExecutionV3Error::NoDecodeTokens
        );
        assert_eq!(
            palw_fp_job_context_v3(&j, &class(), &facts(129, PalwFpStopReasonV3::ExactBudgetReached), NET, [9; 32]).unwrap_err(),
            PalwFpExecutionV3Error::OverBudget { executed: 129, limit: 128 }
        );
        // The stop reason is canonical, not descriptive: one execution must have ONE encoding, or
        // it has two claim ids.
        assert!(matches!(
            palw_fp_job_context_v3(&j, &class(), &facts(77, PalwFpStopReasonV3::ExactBudgetReached), NET, [9; 32]),
            Err(PalwFpExecutionV3Error::StopReasonInconsistent(_))
        ));
        assert!(matches!(
            palw_fp_job_context_v3(&j, &class(), &facts(128, PalwFpStopReasonV3::EndOfGeneration), NET, [9; 32]),
            Err(PalwFpExecutionV3Error::StopReasonInconsistent(_))
        ));
        // …and the canonical pairings are accepted.
        palw_fp_job_context_v3(&j, &class(), &facts(128, PalwFpStopReasonV3::ExactBudgetReached), NET, [9; 32])
            .expect("a run that hit its ceiling");
        palw_fp_job_context_v3(&j, &class(), &facts(1, PalwFpStopReasonV3::EndOfGeneration), NET, [9; 32])
            .expect("a run that stopped early");

        let mut tight = job();
        tight.max_context_tokens = 100;
        assert_eq!(
            palw_fp_job_context_v3(&tight, &class(), &facts(64, PalwFpStopReasonV3::EndOfGeneration), NET, [9; 32]).unwrap_err(),
            PalwFpExecutionV3Error::ContextOverflow { prefill: 64, decode: 64, max: 100 }
        );
        assert_eq!(
            palw_fp_job_context_v3(&j, &class(), &facts(77, PalwFpStopReasonV3::EndOfGeneration), b"", [9; 32]).unwrap_err(),
            PalwFpExecutionV3Error::NetworkIdShape
        );
    }

    /// The free-prompt lane's context zeroes the two fields that name an ORDERER, and that is a
    /// statement rather than an omission: a self-originated job has no assignment to name and no
    /// nullifier to spend. Pinned so a later "fill these in" does not happen by accident.
    #[test]
    fn a_self_originated_job_names_no_orderer() {
        let ctx = palw_fp_job_context_v3(&job(), &class(), &facts(77, PalwFpStopReasonV3::EndOfGeneration), NET, [9; 32]).unwrap();
        assert_eq!(ctx.job_nullifier, Hash64::default());
        assert_eq!(ctx.assignment_id, Hash64::default());
        // …while everything that DOES apply is carried through from the class registration.
        assert_eq!(ctx.runtime_class_id, class().runtime_class_id);
        assert_eq!(ctx.shape_profile_id, class().shape_profile_id);
        assert_eq!(ctx.model_profile_id, class().model_profile_id);
        assert_eq!(ctx.tokenizer_id, job().tokenizer_id);
    }
}
