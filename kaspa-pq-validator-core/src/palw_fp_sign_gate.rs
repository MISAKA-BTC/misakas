//! **The one message shape a free-prompt signature may cover** (ADR-0079 Decision 8, S10, as
//! corrected by the security amendment SA-2).
//!
//! Decision 8's sentence is short and the whole of it matters:
//!
//! > The signer signs **one message shape** — a claim id or a lifecycle object id it re-derives
//! > itself from the object it was handed. A signer that will sign arbitrary bytes is a key the
//! > gateway holds by proxy, and this is the sentence that forbids building one.
//!
//! SA-2 adds the second half, which the ADR body did not have: re-deriving the id is not enough.
//! The id is a function of the commitment, so a fabricated commitment yields a perfectly
//! well-derived id for a claim that never ran. **The commitment must also match the worker result
//! frame** — otherwise a compromised gateway obtains signatures on fabricated commitments and an
//! honest court then slashes the operator's bond for them.
//!
//! So this module is the ONE place a `PalwFpCommitmentV3` claim id may come from, in either form
//! the tree supports:
//!
//! | form | how it signs | where the gate runs |
//! |---|---|---|
//! | the rail's local seed (`--bond-key-seed`, drills and devnets) | [`ValidatorKey::build_fp_commitment_tx`] re-derives the id itself | `misaka-palw-fp-rail` calls [`signable_claim_id`] before the key is even read |
//! | the signer sidecar (`kaspa-pq-signer`, production) | `SigningPurpose::PalwFpCommitmentV3` over a 64-byte digest under a reserved context | `misaka-palw-fp-rail --print-claim` emits ONLY a gated id |
//!
//! **The residual, stated rather than hidden.** The sidecar's wire request
//! (`kaspa_consensus_core::dns_finality::SignerRequest`) carries a typed *digest* and a
//! `SignerMetadata` with no arm for a free-prompt commitment, so the sidecar cannot re-derive from
//! the object: it has never been handed one. It therefore enforces the *shape* (a typed 64-byte
//! digest under a context reserved to the purpose — never arbitrary bytes) while the *derivation*
//! is enforced here, at the only process that holds both the commitment and the result. Giving the
//! sidecar the object too is a `SignerMetadata` arm, which is a change to a consensus-core type and
//! belongs to whoever is allowed to move one.
//!
//! And the residual beneath that one, also stated: a compromised WORKER can still fabricate — that
//! is the court doing its job, and the loss is bounded by the exposure ceiling (ADR-0077 SA-1).

use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpWorkerResultV3, PalwFreePromptCommitmentV3, fp_claim_id_v3};
use kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2;
use kaspa_hashes::Hash64;

/// Why a commitment may not be signed. Every arm names a field, because "the commitment does not
/// match" tells an operator nothing about which byte was edited between the inference and the
/// signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FpSignGateError {
    /// The two artifacts describe different jobs entirely.
    JobMismatch,
    /// A root, a count or the stop reason differs from the execution the result records.
    RootMismatch(&'static str),
    /// The result's prompt ids do not hash to the ids its own job binds.
    PromptIdsUnbound,
    /// A retention deadline at or before the job's anchor is a promise to serve nothing.
    RetentionAlreadyExpired { retention_daa: u64, anchor_daa: u64 },
}

impl std::fmt::Display for FpSignGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FpSignGateError::JobMismatch => {
                write!(f, "the commitment and the worker result describe different jobs")
            }
            FpSignGateError::RootMismatch(field) => {
                write!(f, "the commitment's {field} is not the one this execution produced")
            }
            FpSignGateError::PromptIdsUnbound => {
                write!(f, "the worker result's prompt ids are not the ones its job binds")
            }
            FpSignGateError::RetentionAlreadyExpired { retention_daa, anchor_daa } => {
                write!(
                    f,
                    "the retention deadline {retention_daa} is at or before the job's anchor {anchor_daa} — a promise to serve nothing"
                )
            }
        }
    }
}

impl std::error::Error for FpSignGateError {}

/// **The gate.** Returns the claim id it RE-DERIVES from the commitment — never one it was handed —
/// and only after the commitment has been checked, field by field, against the worker result frame
/// that is supposed to have produced it.
///
/// Every field of `PalwFreePromptCommitmentV3` is accounted for here, deliberately: a field this
/// function forgets is a field a compromised caller may choose freely, and a freely chosen field
/// inside a signed object is the shape ADR-0072 Decision 8 already convicted once.
///
/// * `job` — equality, whole.
/// * `trace_root`, `output_root`, `schedule_root` — the three commitment roots.
/// * `execution_root` — what a court binds a refutation to. **Its absence from the old rail check
///   was the hole SA-2 names**: a commitment carrying someone else's execution root is
///   unadjudicable in the executor's favour and slashable in the court's.
/// * `work_leaves` — the run's PRICE, `step_leaf_count` on the result. Also absent before.
/// * `decode_tokens_executed`, `stop_reason` — what actually ran.
/// * `trace_manifest_root`, `trace_chunk_count` — the DA obligation the producer must serve.
/// * `trace_retention_daa` — the ONE field with no counterpart in the result: it is a chain-time
///   promise the caller makes, so it cannot be cross-checked. What CAN be checked is that the
///   promise was not already broken when it was made.
pub fn signable_claim_id(commitment: &PalwFreePromptCommitmentV3, result: &PalwFpWorkerResultV3) -> Result<Hash64, FpSignGateError> {
    if commitment.job != result.job {
        return Err(FpSignGateError::JobMismatch);
    }
    let checks: [(&'static str, bool); 9] = [
        ("trace_root", commitment.trace_root == result.trace_root),
        ("output_root", commitment.output_root == result.output_root),
        ("schedule_root", commitment.schedule_root == result.schedule_root),
        ("execution_root", commitment.execution_root == result.execution_root),
        ("work_leaves", commitment.work_leaves == result.step_leaf_count),
        ("decode_tokens_executed", commitment.decode_tokens_executed == result.decode_tokens_executed),
        ("stop_reason", commitment.stop_reason == result.stop_reason),
        ("trace_manifest_root", commitment.trace_manifest_root == result.trace_manifest_root),
        ("trace_chunk_count", commitment.trace_chunk_count == result.trace_chunk_count),
    ];
    for (field, ok) in checks {
        if !ok {
            return Err(FpSignGateError::RootMismatch(field));
        }
    }
    if result.job.prompt_token_ids_hash != prompt_token_ids_hash_v2(&result.prompt_token_ids) {
        return Err(FpSignGateError::PromptIdsUnbound);
    }
    if commitment.trace_retention_daa <= commitment.job.anchor_daa {
        return Err(FpSignGateError::RetentionAlreadyExpired {
            retention_daa: commitment.trace_retention_daa,
            anchor_daa: commitment.job.anchor_daa,
        });
    }
    Ok(fp_claim_id_v3(commitment))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFreePromptJobV3};
    use kaspa_consensus_core::tx::TransactionOutpoint;

    fn h(word: u64) -> Hash64 {
        Hash64::from_u64_word(word)
    }

    fn fixture() -> (PalwFreePromptCommitmentV3, PalwFpWorkerResultV3) {
        let prompt_token_ids = vec![7u32, 11, 13];
        let job = PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: h(1),
            class_id: h(2),
            executor_bond: TransactionOutpoint::new(h(3), 0),
            executor_pubkey: vec![9u8; 8],
            operator_id: h(4),
            anchor_block: h(5),
            anchor_daa: 1_000,
            job_nonce: [1u8; 32],
            tokenizer_id: h(6),
            prompt_token_ids_hash: prompt_token_ids_hash_v2(&prompt_token_ids),
            prompt_tokens: prompt_token_ids.len() as u32,
            decode_token_limit: 32,
            max_context_tokens: 4_096,
            privacy_mode: 0,
            prompt_mode: 0,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        };
        let result = PalwFpWorkerResultV3 {
            version: PALW_FP_V3_VERSION,
            request_hash: h(20),
            job: job.clone(),
            prompt_token_ids,
            trace_root: h(30),
            output_root: h(31),
            schedule_root: h(32),
            execution_root: h(33),
            trace_manifest_root: h(34),
            trace_chunk_count: 3,
            trace_event_count: 32,
            decode_tokens_executed: 32,
            step_leaf_count: 7_708,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            output_token_ids: vec![1, 2, 3],
            rendered: b"an answer".to_vec(),
            model_load_ms: 1,
            execute_ms: 2,
        };
        let commitment = PalwFreePromptCommitmentV3 {
            job,
            trace_root: result.trace_root,
            output_root: result.output_root,
            schedule_root: result.schedule_root,
            execution_root: result.execution_root,
            decode_tokens_executed: result.decode_tokens_executed,
            stop_reason: result.stop_reason,
            work_leaves: result.step_leaf_count,
            trace_manifest_root: result.trace_manifest_root,
            trace_chunk_count: result.trace_chunk_count,
            trace_retention_daa: 501_000,
        };
        (commitment, result)
    }

    /// The honest pair passes, and the id it yields is the one `fp_claim_id_v3` computes from the
    /// commitment — RE-DERIVED, never accepted from a caller.
    #[test]
    fn an_honest_pair_yields_the_rederived_id() {
        let (commitment, result) = fixture();
        let id = signable_claim_id(&commitment, &result).expect("an honest pair signs");
        assert_eq!(id, fp_claim_id_v3(&commitment));
    }

    /// **S10 / SA-2.** Every root the commitment carries is checked against the result frame, one
    /// field at a time — including the two the rail's old inline check did not have. Without
    /// `execution_root` a compromised gateway obtains a signature on a claim no court can
    /// adjudicate in the executor's favour; without `work_leaves` it obtains one on a claim priced
    /// for work that never ran. Both are the operator's bond.
    #[test]
    fn a_fabricated_commitment_is_refused_field_by_field() {
        let (base, result) = fixture();

        let mut tampered = base.clone();
        tampered.trace_root = h(999);
        assert_eq!(signable_claim_id(&tampered, &result), Err(FpSignGateError::RootMismatch("trace_root")));

        let mut tampered = base.clone();
        tampered.output_root = h(999);
        assert_eq!(signable_claim_id(&tampered, &result), Err(FpSignGateError::RootMismatch("output_root")));

        let mut tampered = base.clone();
        tampered.schedule_root = h(999);
        assert_eq!(signable_claim_id(&tampered, &result), Err(FpSignGateError::RootMismatch("schedule_root")));

        let mut tampered = base.clone();
        tampered.execution_root = h(999);
        assert_eq!(
            signable_claim_id(&tampered, &result),
            Err(FpSignGateError::RootMismatch("execution_root")),
            "SA-2: the field a court binds a refutation to"
        );

        let mut tampered = base.clone();
        tampered.work_leaves = base.work_leaves * 100;
        assert_eq!(
            signable_claim_id(&tampered, &result),
            Err(FpSignGateError::RootMismatch("work_leaves")),
            "SA-2: the field that prices the claim"
        );

        let mut tampered = base.clone();
        tampered.decode_tokens_executed += 1;
        assert_eq!(signable_claim_id(&tampered, &result), Err(FpSignGateError::RootMismatch("decode_tokens_executed")));

        let mut tampered = base.clone();
        tampered.stop_reason = PalwFpStopReasonV3::EndOfGeneration;
        assert_eq!(signable_claim_id(&tampered, &result), Err(FpSignGateError::RootMismatch("stop_reason")));

        let mut tampered = base.clone();
        tampered.trace_manifest_root = h(999);
        assert_eq!(signable_claim_id(&tampered, &result), Err(FpSignGateError::RootMismatch("trace_manifest_root")));

        let mut tampered = base.clone();
        tampered.trace_chunk_count += 1;
        assert_eq!(signable_claim_id(&tampered, &result), Err(FpSignGateError::RootMismatch("trace_chunk_count")));

        let mut tampered = base.clone();
        tampered.job.class_id = h(999);
        assert_eq!(signable_claim_id(&tampered, &result), Err(FpSignGateError::JobMismatch));
    }

    /// The prompt the result carries must be the prompt its job binds, and a retention deadline at
    /// or before the anchor is a promise to serve nothing.
    #[test]
    fn unbound_prompt_ids_and_an_expired_promise_are_refused() {
        let (commitment, mut result) = fixture();
        result.prompt_token_ids = vec![1, 2, 3, 4];
        assert_eq!(signable_claim_id(&commitment, &result), Err(FpSignGateError::PromptIdsUnbound));

        let (mut commitment, result) = fixture();
        commitment.trace_retention_daa = commitment.job.anchor_daa;
        assert_eq!(
            signable_claim_id(&commitment, &result),
            Err(FpSignGateError::RetentionAlreadyExpired { retention_daa: 1_000, anchor_daa: 1_000 })
        );
    }
}
