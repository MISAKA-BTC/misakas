//! **RFC-0004 Part II `Memory`: authenticated state carried across update steps and across jobs.**
//!
//! A memory class binds a Weights root (the **update rule**: a TIR v1 program, its plan and base weights including the initial memory
//! `M0`) and a [`MemoryRootV1`]: the **slots**, each pairing a param instance (the pre-state a step reads) with a `Fixed` state
//! instance (the post-state its `StateWrite` leaves at the step's last position).
//!
//! A job absorbs `S` chunks. **Step `i` is a K2 claim** of the sub-job `KernelJobV1 { class, chunk_i, 1 token, Greedy, H(job, i) }`
//! over the rule program whose param commitments are the **overlay** `overlay_i`: the base commitments with every slot param replaced
//! by the pre-state of step `i` — the line head for `i = 0`, otherwise step `i−1`'s committed `StateWrite` at its last position. So the
//! per-step pre/post roots are **derived from the committed traces** (never a statement the producer could fabricate), a fault is
//! localised to one step and judged by the kernel's own court over that step's record ([`step_record_v1`]), and the pre-state is the
//! claim's DA obligation (demand stage [`super::MEMORY_PRE_STATE_STAGE_V1`]).
//!
//! The chain tracks one head per memory class ([`MemoryLineV1`]): a claim's Final advances it if the head is still the claim's
//! pre-state (otherwise the claim is superseded: Final and paid, the line unmoved); a post-Final conviction of an advancing claim rolls
//! it back to that claim's pre-state and drops every later advance.
//!
//! **The head is public by construction.** A claim carries its post-state OPENED ([`MemoryClaimV1::post_state`]), checked at inclusion
//! against the commitments its traces derive, and the line records which claim holds its head's tensors
//! ([`MemoryLineV1::head_source`]; `None` = the registered `M0`, part of the attested artifact). So any bond can produce the next job
//! from the chain alone ([`crate::ledger::KernelLedgerV1::memory_head_tensors_v1`]): a line is never the private property of the
//! producer who advanced it last, and it never stalls on tensors that nobody published.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::Prim;
use misaka_palw_tir::program::{StateKind, TirProgramV1};

use crate::evidence::{EvidenceHeaderV1, VerificationEvidenceV1};
use crate::hash::{Digest, finish, keyed, object_id};
use crate::job::{DecodeRuleV1, KernelClaimV1, KernelJobV1};
use crate::ledger::ClassRowV1;
use crate::public::{PublicClaimRecordV1, TensorWireV1, program_root_v1};
use crate::trace::{ParamCommitmentsV1, tensor_commitment};

pub const MEMORY_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/spec/memory-root/v1";
pub const MEMORY_STEP_NONCE_DOMAIN_V1: &[u8] = b"misaka-palw/spec/memory-step-nonce/v1";
pub const MEMORY_STEP_CLAIM_DOMAIN_V1: &[u8] = b"misaka-palw/spec/memory-step-claim/v1";

pub const MAX_MEMORY_SLOTS_V1: usize = 16;
pub const MAX_MEMORY_STEPS_V1: u32 = 64;

/// One memory slot: `(param, layer)` as [`ParamCommitmentsV1`] keys the pre-state's param instance, and `(state, layer)` as the wiring
/// keys the `StateWrite` that leaves the post-state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct MemorySlotV1 {
    pub param: (u16, Option<u16>),
    pub state: (u16, Option<u16>),
}

/// **The `Memory` root kind** (version 1).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MemoryRootV1 {
    /// The typed-roots extension descriptor's digest (`K2-TR-v1`).
    pub extension: Digest,
    /// A free label: the same rule and `M0` may run several independent lines, as several classes.
    pub line: Digest,
    /// Strictly ascending by param; distinct states.
    pub slots: Vec<MemorySlotV1>,
    /// `memory_root_v1(slots, M0's commitments)`, stated and checked against the Weights root.
    pub initial_root: Digest,
    /// The most steps one job may have.
    pub max_steps: u32,
}

/// `H(memory-root; n; for each slot: param, layer, state, layer, commitment)`.
pub fn memory_root_v1(slots: &[MemorySlotV1], commitments: &[Digest]) -> Digest {
    let mut s = keyed(MEMORY_ROOT_DOMAIN_V1);
    s.update(&(slots.len() as u64).to_le_bytes());
    let tag = |l: Option<u16>| l.map(|l| l as u32 + 1).unwrap_or(0).to_le_bytes();
    for (slot, c) in slots.iter().zip(commitments) {
        s.update(&slot.param.0.to_le_bytes()).update(&tag(slot.param.1));
        s.update(&slot.state.0.to_le_bytes()).update(&tag(slot.state.1));
        s.update(c);
    }
    finish(s)
}

/// The `(occurrence, node)` of every `StateWrite`, keyed `(state, layer of the occurrence)`; `Err` if one state instance is written
/// twice (the wiring would keep only one).
pub fn state_writers_v1(program: &TirProgramV1) -> Result<std::collections::BTreeMap<(u16, Option<u16>), (u16, u16)>, String> {
    let mut out = std::collections::BTreeMap::new();
    for (s, (b, layer)) in program.occurrences().iter().enumerate() {
        for (n, node) in program.blocks[*b as usize].nodes.iter().enumerate() {
            if let Prim::StateWrite { state } = node.prim
                && out.insert((state, *layer), (s as u16, n as u16)).is_some()
            {
                return Err(format!("state {state} is written twice in one layer"));
            }
        }
    }
    Ok(out)
}

/// **The registration checks of a memory root** against its rule program and the Weights root's commitments; the writer
/// `(occurrence, node)` of each slot.
pub fn check_memory_root_v1(root: &MemoryRootV1, program: &TirProgramV1, pc: &ParamCommitmentsV1) -> Result<Vec<(u16, u16)>, String> {
    if root.slots.is_empty() || root.slots.len() > MAX_MEMORY_SLOTS_V1 {
        return Err(format!("a memory root has 1..={MAX_MEMORY_SLOTS_V1} slots"));
    }
    if root.max_steps == 0 || root.max_steps > MAX_MEMORY_STEPS_V1 {
        return Err(format!("a memory job has 1..={MAX_MEMORY_STEPS_V1} steps"));
    }
    if root.slots.windows(2).any(|w| w[0].param >= w[1].param) {
        return Err("the slots are not strictly ascending by param".into());
    }
    let mut states = std::collections::BTreeSet::new();
    let writers = state_writers_v1(program)?;
    let layers = program.schedule.layers.len();
    let mut out = Vec::with_capacity(root.slots.len());
    let mut initial = Vec::with_capacity(root.slots.len());
    for slot in &root.slots {
        if !states.insert(slot.state) {
            return Err("two slots write one state".into());
        }
        let (j, pl) = slot.param;
        let decl = program.params.get(j as usize).ok_or(format!("slot param {j} is not declared"))?;
        let instance_ok = match pl {
            Some(l) => decl.per_layer && (l as usize) < layers,
            None => !decl.per_layer,
        };
        if !instance_ok {
            return Err(format!("slot param {j} layer {pl:?} is not a declared instance"));
        }
        let c = pc.by_instance.get(&slot.param).ok_or(format!("the Weights root does not commit slot param {j} layer {pl:?}"))?;
        initial.push(*c);
        let (s, sl) = slot.state;
        let st = program.states.get(s as usize).ok_or(format!("slot state {s} is not declared"))?;
        if matches!(st.kind, StateKind::Hist { .. }) {
            return Err("KERNEL_EXTENSION_REQUIRED [memory-hist-carry]: a memory slot over a Hist state".into());
        }
        let &(occ, node) = writers.get(&slot.state).ok_or(format!("no StateWrite of state {s} in layer {sl:?}"))?;
        let b = program.occurrences()[occ as usize].0 as usize;
        let out_ty = &program.blocks[b].nodes[node as usize].out;
        let param_shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
        if out_ty.dtype != decl.dtype || out_ty.resolve(1) != param_shape || st.dtype != decl.dtype {
            return Err(format!("slot param {j} is not of its state's type (the post-state could not be the next pre-state)"));
        }
        out.push((occ, node));
    }
    if memory_root_v1(&root.slots, &initial) != root.initial_root {
        return Err("the stated initial memory root is not the Weights root's commitments of the slot params".into());
    }
    Ok(out)
}

/// The commitments of the slot params in a commitment set, in slot order.
pub fn slot_commitments_v1(slots: &[MemorySlotV1], pc: &ParamCommitmentsV1) -> Option<Vec<Digest>> {
    slots.iter().map(|s| pc.by_instance.get(&s.param).copied()).collect()
}

/// **The overlay**: `base` with every slot param's commitment replaced by `state`'s.
pub fn overlay_v1(base: &ParamCommitmentsV1, slots: &[MemorySlotV1], state: &[Digest]) -> ParamCommitmentsV1 {
    let mut out = base.clone();
    for (slot, c) in slots.iter().zip(state) {
        out.by_instance.insert(slot.param, *c);
    }
    out
}

/// The post-state a step's commitments leave: each slot's `StateWrite` at the step's last position.
pub fn post_state_v1(writers: &[(u16, u16)], commitments: &[Vec<Vec<Digest>>]) -> Option<Vec<Digest>> {
    let last = commitments.last()?;
    writers.iter().map(|(s, n)| last.get(*s as usize)?.get(*n as usize).copied()).collect()
}

/// A memory job: the class, the pre-state root it runs on (the line head at posting), the chunks, a nonce.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MemoryJobV1 {
    pub class: Digest,
    pub pre_root: Digest,
    pub chunks: Vec<Vec<u32>>,
    pub nonce: Digest,
}

impl MemoryJobV1 {
    pub fn well_formed(&self, root: &MemoryRootV1, rule: &ClassRowV1) -> Result<(), String> {
        if self.chunks.is_empty() || self.chunks.len() as u64 > root.max_steps as u64 {
            return Err(format!("a memory job has 1..={} steps", root.max_steps));
        }
        for (i, c) in self.chunks.iter().enumerate() {
            step_job_v1(&self.class, &[0; 64], i as u32, c)
                .well_formed(rule.program.token_bound, rule.plan.max_positions)
                .map_err(|e| format!("chunk {i}: {e}"))?;
        }
        Ok(())
    }
}

/// One step's committed evidence and trace commitments.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StepCommitV1 {
    pub evidence: VerificationEvidenceV1,
    pub commitments: Vec<Vec<Vec<Digest>>>,
}

/// A memory claim (signed by `producer_bond`): one delivered token per step, the `S + 1` boundary roots, every step's commitments,
/// and the post-state opened.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MemoryClaimV1 {
    pub job_id: Digest,
    pub producer_bond: Digest,
    pub generated: Vec<u32>,
    pub step_roots: Vec<Digest>,
    pub steps: Vec<StepCommitV1>,
    /// **The post-state, opened**: one tensor per slot, in slot order — the last step's slot writes at its last position. Checked at
    /// inclusion against the commitments the traces derive ([`check_post_state_v1`]), so the state a Final could make the head is
    /// public before it can be: the next job's producer reads it from the chain, never from this producer.
    pub post_state: Vec<TensorWireV1>,
}

/// **The inclusion check of a carried post-state**: one tensor per slot, each of its slot param's declared dtype and shape, each the
/// committed value `post[k]` (the commitment the traces derive). A wrong value under the right commitment cannot exist; a lie in the
/// committed write itself is the step's kernel fault, judged by the court like any other value.
pub fn check_post_state_v1(
    program: &TirProgramV1,
    slots: &[MemorySlotV1],
    post: &[Digest],
    carried: &[TensorWireV1],
) -> Result<(), String> {
    if carried.len() != slots.len() || post.len() != slots.len() {
        return Err(format!("the carried post-state has {} tensors, the class {} slots", carried.len(), slots.len()));
    }
    for (k, ((slot, c), w)) in slots.iter().zip(post).zip(carried).enumerate() {
        let t = w.decode().map_err(|e| format!("the carried post-state's slot {k} does not decode: {e}"))?;
        let decl = program.params.get(slot.param.0 as usize).ok_or(format!("slot {k}'s param is not declared"))?;
        let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
        if t.dtype != decl.dtype || t.shape != shape {
            return Err(format!("the carried post-state's slot {k} is not of its param's type"));
        }
        if tensor_commitment(&t) != *c {
            return Err(format!(
                "the carried post-state's slot {k} is not the committed write (memory is carried only in public, as committed)"
            ));
        }
    }
    Ok(())
}

/// The sub-job step `i` is a claim of.
pub fn step_job_v1(class: &Digest, job_id: &Digest, i: u32, chunk: &[u32]) -> KernelJobV1 {
    KernelJobV1 {
        class_binding_id: *class,
        prompt: chunk.to_vec(),
        max_new_tokens: 1,
        decode: DecodeRuleV1::Greedy,
        nonce: object_id(MEMORY_STEP_NONCE_DOMAIN_V1, &(*job_id, i)),
    }
}

/// The sub-claim of step `i`.
pub fn step_claim_v1(job: &KernelJobV1, producer: &Digest, token: u32, evidence_root: Digest) -> KernelClaimV1 {
    KernelClaimV1 { job_id: job.id(), producer_bond: *producer, generated: vec![token], evidence_root }
}

/// The id a step's record is bound to (the challenge binding's claim id).
pub fn step_claim_id_v1(claim: &Digest, i: u32) -> Digest {
    object_id(MEMORY_STEP_CLAIM_DOMAIN_V1, &(*claim, i))
}

/// The header step `i`'s evidence must carry: the rule's, with the overlay's artifact root.
pub fn step_header_v1(rule: &ClassRowV1, class: &Digest, overlay: &ParamCommitmentsV1) -> EvidenceHeaderV1 {
    EvidenceHeaderV1 {
        network_domain: rule.network_domain,
        ruleset_digest: rule.ruleset_digest,
        class_binding_id: *class,
        program_root: program_root_v1(&rule.program_bytes),
        artifact_root: overlay.root(),
        plan_root: rule.plan.root(),
    }
}

/// **Step `i`'s public record**: exactly what a plain K2 claim of the sub-job publishes, with the overlay as its param commitments.
pub fn step_record_v1(
    claim_id: &Digest,
    i: u32,
    rule: &ClassRowV1,
    step: &StepCommitV1,
    overlay: &ParamCommitmentsV1,
    chunk: &[u32],
) -> PublicClaimRecordV1 {
    PublicClaimRecordV1 {
        claim_id: step_claim_id_v1(claim_id, i),
        program_bytes: rule.program_bytes.clone(),
        plan: rule.plan.clone(),
        evidence: step.evidence.clone(),
        trace_commitments: step.commitments.clone(),
        param_commitments: overlay.by_instance.iter().map(|((j, l), d)| (*j, *l, *d)).collect(),
        tokens: chunk.to_vec(),
        beacon: [0; 64],
    }
}

/// **The boundary states** of a claim: `pre`, then each step's post-state (`None`: a step's commitments do not name a slot's write).
pub fn boundary_states_v1(writers: &[(u16, u16)], pre: &[Digest], steps: &[StepCommitV1]) -> Option<Vec<Vec<Digest>>> {
    let mut out = vec![pre.to_vec()];
    for s in steps {
        out.push(post_state_v1(writers, &s.commitments)?);
    }
    Some(out)
}

/// The global stage-0 position of step `i`'s first position (the steps' positions concatenated in order).
pub fn step_offsets_v1(steps: &[StepCommitV1]) -> Vec<u32> {
    let mut out = Vec::with_capacity(steps.len() + 1);
    let mut at = 0u32;
    out.push(0);
    for s in steps {
        at = at.saturating_add(s.commitments.len() as u32);
        out.push(at);
    }
    out
}

/// The step holding global position `g`, and the local position.
pub fn locate_v1(steps: &[StepCommitV1], g: u32) -> Option<(usize, u32)> {
    let mut at = 0u32;
    for (i, s) in steps.iter().enumerate() {
        let n = s.commitments.len() as u32;
        if g < at.saturating_add(n) {
            return Some((i, g - at));
        }
        at = at.saturating_add(n);
    }
    None
}

/// One advance of a line: the claim that moved it, from what (and which claim carried that state's tensors), to what, and until when
/// a conviction can still reach it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AdvanceV1 {
    pub claim: Digest,
    pub pre: Vec<Digest>,
    /// The claim whose carried post-state is `pre` (`None`: the registered `M0`).
    pub pre_source: Option<Digest>,
    pub post: Vec<Digest>,
    pub until_daa: u64,
}

/// **The chain-tracked memory of one class**: the head's slot commitments and root, where its tensors are public, and the advances a
/// conviction can still reach.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MemoryLineV1 {
    pub head: Vec<Digest>,
    pub head_root: Digest,
    /// The claim whose carried post-state ([`MemoryClaimV1::post_state`]) is the head's tensors; `None`: the registered `M0`, public
    /// as part of the attested artifact.
    pub head_source: Option<Digest>,
    pub advances: Vec<AdvanceV1>,
}

impl MemoryLineV1 {
    pub fn genesis(slots: &[MemorySlotV1], m0: Vec<Digest>) -> Self {
        Self { head_root: memory_root_v1(slots, &m0), head: m0, head_source: None, advances: Vec::new() }
    }

    /// A claim reached Final from `pre` to `post`: the head moves if it is still `pre` (`true`), and its tensors are then the claim's
    /// carried post-state; else the claim is superseded.
    pub fn on_final(&mut self, slots: &[MemorySlotV1], claim: Digest, pre: &[Digest], post: Vec<Digest>, until_daa: u64) -> bool {
        if self.head != pre {
            return false;
        }
        self.advances.push(AdvanceV1 { claim, pre: pre.to_vec(), pre_source: self.head_source, post: post.clone(), until_daa });
        self.head_root = memory_root_v1(slots, &post);
        self.head = post;
        self.head_source = Some(claim);
        true
    }

    /// A claim was convicted after Final: if it advanced the line, the head returns to its pre-state (whose tensors are where they
    /// were) and every later advance is dropped (`true`).
    pub fn on_post_final_conviction(&mut self, slots: &[MemorySlotV1], claim: &Digest) -> bool {
        let Some(k) = self.advances.iter().position(|a| a.claim == *claim) else { return false };
        let (pre, source) = (self.advances[k].pre.clone(), self.advances[k].pre_source);
        self.advances.truncate(k);
        self.head_root = memory_root_v1(slots, &pre);
        self.head = pre;
        self.head_source = source;
        true
    }

    /// Advances past their horizon can no longer be reached by a conviction.
    pub fn prune(&mut self, daa: u64) {
        self.advances.retain(|a| daa <= a.until_daa);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots() -> Vec<MemorySlotV1> {
        vec![MemorySlotV1 { param: (3, None), state: (0, Some(0)) }]
    }

    #[test]
    fn a_line_advances_at_final_is_superseded_by_a_stale_claim_and_rolls_back_on_a_late_conviction() {
        let s = slots();
        let mut line = MemoryLineV1::genesis(&s, vec![[0; 64]]);
        let r0 = line.head_root;
        assert_eq!(line.head_source, None, "the genesis head is the registered M0");
        assert!(line.on_final(&s, [1; 64], &[[0; 64]], vec![[1; 64]], 100));
        assert_eq!(line.head_source, Some([1; 64]), "the head's tensors are the advancing claim's carried post-state");
        assert!(!line.on_final(&s, [2; 64], &[[0; 64]], vec![[2; 64]], 100), "a claim over a stale head is superseded");
        assert_eq!(line.head_source, Some([1; 64]), "a superseded claim never becomes the head's source");
        assert!(line.on_final(&s, [3; 64], &[[1; 64]], vec![[3; 64]], 120));
        assert_eq!((line.head.clone(), line.head_source), (vec![[3; 64]], Some([3; 64])));
        assert_eq!(line.advances[1].pre_source, Some([1; 64]));
        assert!(line.on_post_final_conviction(&s, &[1; 64]), "the first advance is convicted");
        assert_eq!((line.head.clone(), line.head_root), (vec![[0; 64]], r0), "back to its pre-state; the later advance dropped");
        assert_eq!(line.head_source, None, "and to where that state's tensors are public (M0)");
        assert!(line.advances.is_empty());
        assert!(!line.on_post_final_conviction(&s, &[3; 64]), "a dropped advance is no longer the line's");
        line.on_final(&s, [4; 64], &[[0; 64]], vec![[4; 64]], 10);
        line.prune(11);
        assert!(line.advances.is_empty() && line.head == vec![[4; 64]], "pruning forgets the advance, never the head");
    }

    #[test]
    fn the_memory_root_binds_slots_and_commitments() {
        let s = slots();
        let a = memory_root_v1(&s, &[[1; 64]]);
        assert_ne!(a, memory_root_v1(&s, &[[2; 64]]));
        let other = vec![MemorySlotV1 { param: (3, None), state: (1, Some(0)) }];
        assert_ne!(a, memory_root_v1(&other, &[[1; 64]]));
    }
}
