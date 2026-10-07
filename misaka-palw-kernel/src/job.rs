//! **Job, input and output binding on the kernel route** — what makes a claim a claim of THIS job (ADR-0173; RFC-0011 §15.3).
//!
//! A [`KernelJobV1`] is posted on chain: the class it runs, the prompt, the generation budget and the decode rule. A
//! [`KernelClaimV1`] answers it: the generated tokens it delivers, the producer bond, and the roots of what it committed (its
//! §15.3 evidence object). Everything the claim must agree with is public, so every disagreement is an objective fault:
//!
//! * **Binding faults** ([`BindingFaultV1`]) — the evidence names another class, another input (a valid trace of another job
//!   borrowed — RFC-0011's "a claim cannot substitute a different output"), another context length, or a generation outside the
//!   job's budget. Proved from the job, the claim and the evidence object alone; no opening.
//! * **Decode faults** ([`DecodeFaultV1`]) — a delivered token is not the job's decode rule applied to the committed logits of
//!   the position before it. Proved by opening ONE logits tensor against the trace commitment.
//!
//! The decode rules this version implements are [`DecodeRuleV1::Greedy`] (arg-max, lowest index on a tie). A job naming any
//! other rule is refused at posting: an unsupported relation is never a success.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::Tensor;

use crate::evidence::{VerificationEvidenceV1, job_input_root_v1};
use crate::hash::{Digest, object_id};
use crate::trace::{EvidenceV1, tensor_commitment};

pub const KERNEL_JOB_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/job/v1";
pub const KERNEL_CLAIM_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/claim/v1";

/// How a delivered token is chosen from the committed logits of the position before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum DecodeRuleV1 {
    /// Arg-max over the vocabulary, the lowest token id on a tie.
    Greedy = 0,
}

impl DecodeRuleV1 {
    /// The token this rule selects from `logits` (a rank-1 vector over the vocabulary, or any tensor flattened). `None` for an
    /// empty vector.
    pub fn select(self, logits: &Tensor) -> Option<u32> {
        match self {
            Self::Greedy => {
                let mut best: Option<(usize, i128)> = None;
                for (i, v) in logits.data.iter().enumerate() {
                    if best.is_none_or(|(_, b)| *v > b) {
                        best = Some((i, *v));
                    }
                }
                best.and_then(|(i, _)| u32::try_from(i).ok())
            }
        }
    }
}

/// A job, as posted on chain.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct KernelJobV1 {
    pub class_binding_id: Digest,
    pub prompt: Vec<u32>,
    /// At most this many generated tokens (at least one is always delivered).
    pub max_new_tokens: u32,
    pub decode: DecodeRuleV1,
    /// The requester's nonce: two identical requests are two jobs.
    pub nonce: Digest,
}

impl KernelJobV1 {
    pub fn id(&self) -> Digest {
        object_id(KERNEL_JOB_DOMAIN_V1, self)
    }

    /// The structural refusals at posting.
    pub fn well_formed(&self, token_bound: u32, max_positions: u32) -> Result<(), String> {
        if self.prompt.is_empty() {
            return Err("a job has at least one prompt token".into());
        }
        if self.max_new_tokens == 0 {
            return Err("a job generates at least one token".into());
        }
        if self.prompt.iter().any(|t| *t >= token_bound) {
            return Err(format!("a prompt token is past the class's token bound {token_bound}"));
        }
        let positions = self.prompt.len() as u64 + self.max_new_tokens as u64 - 1;
        if positions > max_positions as u64 {
            return Err(format!("{positions} positions exceed the class's {max_positions}"));
        }
        Ok(())
    }
}

/// A claim answering a job.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct KernelClaimV1 {
    pub job_id: Digest,
    pub producer_bond: Digest,
    /// The delivered tokens.
    pub generated: Vec<u32>,
    /// The root of the claim's §15.3 evidence object.
    pub evidence_root: Digest,
}

impl KernelClaimV1 {
    pub fn id(&self) -> Digest {
        object_id(KERNEL_CLAIM_DOMAIN_V1, self)
    }

    /// The token run the program is evaluated over: the prompt, then every generated token but the last (never fed back).
    pub fn stream(&self, job: &KernelJobV1) -> Vec<u32> {
        let fed = self.generated.len().saturating_sub(1);
        job.prompt.iter().chain(&self.generated[..fed]).copied().collect()
    }

    /// The position whose logits select generated token `r`.
    pub fn select_position(&self, job: &KernelJobV1, r: usize) -> u32 {
        (job.prompt.len() - 1 + r) as u32
    }
}

/// A claim that is not a claim of its job, provable from public objects alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BindingFaultV1 {
    /// The claim names another job.
    WrongJob,
    /// The evidence object is not the one the claim commits.
    WrongEvidence,
    /// The evidence is a trace of another class.
    WrongClass,
    /// The evidence's input is not this job's prompt followed by the delivered tokens (a borrowed trace).
    WrongInput,
    /// The evidence covers another number of positions than prompt + generated − 1.
    WrongLength,
    /// No token, or more than the job allows.
    WrongGenerationLength,
    /// A delivered token is past the class's token bound.
    TokenOutOfRange,
}

/// **The binding check** — every disagreement among the job, the claim and the evidence object, first found.
pub fn binding_fault_v1(
    job: &KernelJobV1,
    claim: &KernelClaimV1,
    evidence: &VerificationEvidenceV1,
    token_bound: u32,
) -> Option<BindingFaultV1> {
    use BindingFaultV1 as F;
    if claim.job_id != job.id() {
        return Some(F::WrongJob);
    }
    if claim.evidence_root != evidence.root() {
        return Some(F::WrongEvidence);
    }
    if evidence.header.class_binding_id != job.class_binding_id {
        return Some(F::WrongClass);
    }
    if claim.generated.is_empty() || claim.generated.len() as u64 > job.max_new_tokens as u64 {
        return Some(F::WrongGenerationLength);
    }
    if claim.generated.iter().any(|t| *t >= token_bound) {
        return Some(F::TokenOutOfRange);
    }
    let stream = claim.stream(job);
    if evidence.positions as usize != stream.len() {
        return Some(F::WrongLength);
    }
    if evidence.job_input_root != job_input_root_v1(&stream) {
        return Some(F::WrongInput);
    }
    None
}

/// **A decode fault**: delivered token `r` is not the decode rule applied to the committed logits that select it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DecodeFaultV1 {
    /// Which delivered token.
    pub index: u32,
    /// The committed logits of the selecting position, opened whole (one vocabulary vector).
    pub logits: crate::public::TensorWireV1,
}

/// Why a decode accusation was dismissed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeDismissalV1 {
    NotAuthentic(String),
    NoFault,
}

/// **The decode court**: authenticate the opened logits against the committed trace at the selecting position, then apply the
/// job's rule. `post_occurrence` / `logits_node` locate the logits value in the trace.
pub fn verify_decode_fault_v1(
    job: &KernelJobV1,
    claim: &KernelClaimV1,
    trace: &EvidenceV1,
    (post_occurrence, logits_node): (u16, u16),
    fault: &DecodeFaultV1,
) -> Result<u32, DecodeDismissalV1> {
    use DecodeDismissalV1 as D;
    let r = fault.index as usize;
    let Some(delivered) = claim.generated.get(r).copied() else {
        return Err(D::NotAuthentic("no such delivered token".into()));
    };
    let logits = fault.logits.decode().map_err(D::NotAuthentic)?;
    let p = claim.select_position(job, r);
    match trace.at(p, post_occurrence, logits_node) {
        Some(c) if *c == tensor_commitment(&logits) => {}
        _ => return Err(D::NotAuthentic("the opened logits are not the committed ones".into())),
    }
    match job.decode.select(&logits) {
        Some(t) if t == delivered => Err(D::NoFault),
        _ => Ok(fault.index),
    }
}

/// **Find a decode fault** with the committed logits in hand (the verifier that opened them).
pub fn find_decode_fault_v1(
    job: &KernelJobV1,
    claim: &KernelClaimV1,
    logits_at: &dyn Fn(u32) -> Option<Tensor>,
) -> Result<Option<DecodeFaultV1>, u32> {
    for (r, delivered) in claim.generated.iter().enumerate() {
        let p = claim.select_position(job, r);
        let logits = logits_at(p).ok_or(p)?;
        if job.decode.select(&logits) != Some(*delivered) {
            return Ok(Some(DecodeFaultV1 { index: r as u32, logits: crate::public::TensorWireV1::of(&logits) }));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::DType;

    #[test]
    fn greedy_takes_the_lowest_index_on_a_tie() {
        let t = Tensor::new(DType::I32, vec![5], vec![3, 9, 2, 9, 1]).unwrap();
        assert_eq!(DecodeRuleV1::Greedy.select(&t), Some(1));
        let e = Tensor::new(DType::I32, vec![0], vec![]).unwrap();
        assert_eq!(DecodeRuleV1::Greedy.select(&e), None);
    }

    #[test]
    fn a_job_is_refused_at_posting_outside_the_class() {
        let job = KernelJobV1 {
            class_binding_id: [1; 64],
            prompt: vec![1, 2],
            max_new_tokens: 3,
            decode: DecodeRuleV1::Greedy,
            nonce: [0; 64],
        };
        job.well_formed(32, 64).unwrap();
        assert!(job.well_formed(2, 64).is_err(), "a prompt token past the bound");
        assert!(job.well_formed(32, 3).is_err(), "more positions than the class admits");
        assert!(KernelJobV1 { prompt: vec![], ..job.clone() }.well_formed(32, 64).is_err());
        assert_ne!(job.id(), KernelJobV1 { nonce: [1; 64], ..job }.id(), "two requests are two jobs");
    }
}
