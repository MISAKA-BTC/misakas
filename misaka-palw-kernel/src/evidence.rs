//! **`VerificationEvidenceV1` — everything a claim binds before its challenge** (RFC-0011 §15.3, RFC-0007 §V.5).
//!
//! The object binds the network (chain genesis and network id, as one domain), the ruleset, the class binding, the program, artifact
//! and plan roots, the job's input, the initial and final state roots, the trace commitment, the output, the full semantic context
//! (positions) and a **segment directory**, and the suite's version and security parameters. Its root is the `evidence_root` the
//! challenge seed is drawn from; a verifier recomputes every field it can and refuses an object that disagrees with the trace, so a
//! producer cannot substitute an output, an entry state or a weaker suite after a challenge is known.
//!
//! The segment directory partitions the positions into contiguous segments. Each segment's entry and exit state roots are **derived
//! from the committed trace** ([`crate::trace::WiringV1::state_root_entering`]): a segment's entry is its predecessor's exit by
//! construction, so a correct isolated segment with a fabricated entry state cannot pass (RFC-0011 §15.2's boundary row).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::descriptor::KernelDescriptorV1;
use crate::hash::{Digest, finish, keyed, object_id};
use crate::trace::{EvidenceV1, WiringV1};

pub const VERIFICATION_EVIDENCE_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/verification-evidence/v1";
pub const JOB_INPUT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/job-input/v1";
pub const VERIFICATION_EVIDENCE_VERSION_V1: u16 = 1;
/// The most segments a directory may list (a parse/DoS bound, not a semantic one).
pub const MAX_SEGMENTS_V1: usize = 1 << 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SegmentV1 {
    /// Positions `[first, end)`.
    pub first: u32,
    pub end: u32,
    pub entry_state_root: Digest,
    pub exit_state_root: Digest,
}

/// The suite and its security parameters, as the descriptor fixes them (a claim cannot state its own).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SuiteParamsV1 {
    pub descriptor_digest: Digest,
    pub checker_suite_id: u32,
    pub challenge_policy_id: u32,
    pub repetitions: u8,
    pub target_bits: u16,
    pub field_bits: u16,
}

impl SuiteParamsV1 {
    pub fn of(d: &KernelDescriptorV1) -> Self {
        SuiteParamsV1 {
            descriptor_digest: d.digest(),
            checker_suite_id: d.checker_suite_id,
            challenge_policy_id: d.challenge_policy_id,
            repetitions: d.soundness.repetitions,
            target_bits: d.soundness.target_bits,
            field_bits: d.per_repetition_bits() as u16,
        }
    }
}

/// The commitments a claim header names, beside what the trace derives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EvidenceHeaderV1 {
    /// `palw_network_domain_v2`: network id and chain genesis.
    pub network_domain: Digest,
    pub ruleset_digest: Digest,
    pub class_binding_id: Digest,
    pub program_root: Digest,
    pub artifact_root: Digest,
    pub plan_root: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct VerificationEvidenceV1 {
    pub version: u16,
    pub header: EvidenceHeaderV1,
    pub job_input_root: Digest,
    /// The full semantic context: every position the claim computes.
    pub positions: u32,
    pub initial_state_root: Digest,
    pub final_state_root: Digest,
    /// The root of every committed node value ([`EvidenceV1::root`]).
    pub trace_root: Digest,
    pub output_root: Digest,
    pub segments: Vec<SegmentV1>,
    pub suite: SuiteParamsV1,
}

impl VerificationEvidenceV1 {
    pub fn root(&self) -> Digest {
        object_id(VERIFICATION_EVIDENCE_DOMAIN_V1, self)
    }

    pub fn segment(&self, i: u32) -> Option<&SegmentV1> {
        self.segments.get(i as usize)
    }
}

pub fn job_input_root_v1(tokens: &[u32]) -> Digest {
    let mut s = keyed(JOB_INPUT_DOMAIN_V1);
    s.update(&(tokens.len() as u64).to_le_bytes());
    for t in tokens {
        s.update(&t.to_le_bytes());
    }
    finish(s)
}

/// **Build the evidence object** a producer commits: segments of `segment_len` positions (the last may be shorter).
pub fn build_evidence_v1(
    w: &WiringV1<'_>,
    trace: &EvidenceV1,
    tokens: &[u32],
    header: EvidenceHeaderV1,
    descriptor: &KernelDescriptorV1,
    segment_len: u32,
) -> Result<VerificationEvidenceV1, String> {
    let n = tokens.len() as u32;
    if n == 0 || segment_len == 0 {
        return Err("a claim computes at least one position, in segments of at least one".into());
    }
    let root = |p: u32| w.state_root_entering(trace, tokens, p).map_err(|e| e.to_string());
    let mut segments = Vec::new();
    let mut first = 0u32;
    while first < n {
        let end = first.saturating_add(segment_len).min(n);
        segments.push(SegmentV1 { first, end, entry_state_root: root(first)?, exit_state_root: root(end)? });
        first = end;
    }
    Ok(VerificationEvidenceV1 {
        version: VERIFICATION_EVIDENCE_VERSION_V1,
        header,
        job_input_root: job_input_root_v1(tokens),
        positions: n,
        initial_state_root: root(0)?,
        final_state_root: root(n)?,
        trace_root: trace.root(),
        output_root: trace.output_root(w.program),
        segments,
        suite: SuiteParamsV1::of(descriptor),
    })
}

/// **Check an evidence object against the trace commitments, the job and the descriptor** — every derivable field recomputed.
pub fn check_evidence_v1(
    w: &WiringV1<'_>,
    trace: &EvidenceV1,
    tokens: &[u32],
    ev: &VerificationEvidenceV1,
    descriptor: &KernelDescriptorV1,
    expected_header: &EvidenceHeaderV1,
) -> Result<(), String> {
    if ev.version != VERIFICATION_EVIDENCE_VERSION_V1 {
        return Err(format!("evidence version {} is not supported", ev.version));
    }
    if ev.header != *expected_header {
        return Err("the evidence names another network, ruleset, class, program, artifact or plan".into());
    }
    if ev.suite != SuiteParamsV1::of(descriptor) {
        return Err("the evidence names another suite or other security parameters than the class's descriptor".into());
    }
    let n = tokens.len() as u32;
    if ev.positions != n || trace.commitments.len() as u32 != n {
        return Err(format!(
            "the evidence states {} positions; the job has {n} and the trace {}",
            ev.positions,
            trace.commitments.len()
        ));
    }
    if ev.job_input_root != job_input_root_v1(tokens) {
        return Err("the job input is not the one committed".into());
    }
    if ev.trace_root != trace.root() || ev.output_root != trace.output_root(w.program) {
        return Err("the trace or output root is not the committed trace's".into());
    }
    let root = |p: u32| w.state_root_entering(trace, tokens, p).map_err(|e| e.to_string());
    if ev.initial_state_root != root(0)? || ev.final_state_root != root(n)? {
        return Err("the initial or final state root is not the trace's".into());
    }
    if ev.segments.is_empty() || ev.segments.len() > MAX_SEGMENTS_V1 {
        return Err(format!("{} segments", ev.segments.len()));
    }
    let mut next = 0u32;
    for (i, seg) in ev.segments.iter().enumerate() {
        if seg.first != next || seg.end <= seg.first || seg.end > n {
            return Err(format!("segment {i} [{}, {}) is not contiguous after {next}", seg.first, seg.end));
        }
        if seg.entry_state_root != root(seg.first)? || seg.exit_state_root != root(seg.end)? {
            return Err(format!("segment {i}'s entry or exit state root is not the committed trace's (a fabricated boundary)"));
        }
        next = seg.end;
    }
    if next != n {
        return Err(format!("the segments end at {next}, the claim at {n}"));
    }
    Ok(())
}
