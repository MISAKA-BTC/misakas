//! **RFC-0004 Part II — computation specifications with typed roots** (`docs/design/palw/rfc-0004-part2-typed-roots.md`).
//!
//! A class binds a versioned [`ComputationSpecV1`]: its verification mode and a list of [`TypedRootV1`]s. MISAKA registers, executes
//! and settles independently verifiable useful AI computation, not weight files; the kinds are
//!
//! | roots | class | judged by |
//! | --- | --- | --- |
//! | `[WeightsV1]` | today's single-program class, **byte for byte** (same id, row and ledger root as route tags 1 / 13) | the K2 courts |
//! | `[WeightsV1, MemoryV1]` | a model whose memory is updated per step and carried across jobs ([`memory`]) | the K2 courts on one step's record |
//! | `[RetrievalV1]` | a deterministic retrieval over a public snapshot ([`retrieval`]) | per-item courts against the snapshot root |
//! | `[CompositeV1]` | a pipeline of registered models and verified tools ([`composite`]) | each stage's own courts; exact edges |
//!
//! Every typed kind needs the typed-roots extension descriptor [`k2_tr_v1_descriptor`] to be **Active in the ledger's schedule** (the
//! consumer puts it there only when its `palw_typed_roots_v1` fence is in force), registers only under
//! `OptimisticPublicVerification`, and rides the kernel route as one object, [`crate::route::KernelRouteObjectV1::Spec`], whose
//! sub-objects are versioned here ([`SpecObjectV1`]). Claims live in the route's own `claims` table ([`SpecClaimBodyV1`]), so seal-then-
//! reveal, one claim per job, the OPV window and reservation, demands, defaults, Final, liability and settlement are the route's.
//!
//! Forbidden in every kind, by construction: an external API, unverified code, a nondeterministic retrieval, and any state an
//! outsider cannot obtain (every value a court reads is on chain, committed, the registered artifact or snapshot, or a claim's DA).

pub mod composite;
pub mod memory;
pub mod outsider;
pub mod produce;
pub mod retrieval;

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::program::TirProgramV1;

use crate::descriptor::{KERNEL_DESCRIPTOR_DOMAIN_V1, KernelDescriptorV1, k2_tir_v2_descriptor};
use crate::gate::{FILING_HEADER_BYTES_V1, ProsecutionBoundsV1, ProsecutionPolicyV1, WIRE_HEADER_BYTES_V1};
use crate::hash::{Digest, object_id};
use crate::ledger::ClassRowV1;
use crate::mode::VerificationModeV1;
use crate::plan::VerificationPlanV1;
use crate::public::TensorWireV1;
use crate::trace::ParamCommitmentsV1;

use composite::{ComponentV1, CompositeClaimV1, CompositeJobV1, CompositeRootV1, StageClaimV1, StageInputV1};
use memory::{MemoryClaimV1, MemoryJobV1, MemoryLineV1, MemoryRootV1, locate_v1};
use retrieval::{RetrievalClaimV1, RetrievalFaultV1, RetrievalItemV1, RetrievalJobV1, RetrievalRootV1, depth_v1};

pub const SPEC_VERSION_V1: u16 = 1;
pub const SPEC_CLASS_DOMAIN_V1: &[u8] = b"misaka-palw/spec/class/v1";
pub const SPEC_JOB_DOMAIN_V1: &[u8] = b"misaka-palw/spec/job/v1";
pub const SPEC_CLAIM_DOMAIN_V1: &[u8] = b"misaka-palw/spec/claim/v1";
pub const SPEC_EXECUTION_DOMAIN_V1: &[u8] = b"misaka-palw/spec/execution/v1";
pub const TYPED_STATE_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/ledger-typed-root/v1";

/// Demand stage of a memory claim's pre-state (position 0: one value per slot).
pub const MEMORY_PRE_STATE_STAGE_V1: u8 = 0x40;
/// Demand stage of stage `s`'s snapshot slices: `0x80 + s` (a plain retrieval class is stage 0).
pub const SNAPSHOT_STAGE_BASE_V1: u8 = 0x80;

/// The normative text of the typed-roots extension (its digest is the descriptor's identity; a new meaning is a new descriptor).
pub const K2_TR_V1_SEMANTICS: &[u8] = b"misaka-palw-kernel K2-TR-v1: RFC-0004 Part II typed roots over K2-TIR-v1/v2 classes. \
Weights: today's single-program class unchanged. Memory: slots pair a param instance (the pre-state a step reads) with a Fixed \
StateWrite (the post-state at the step's last position); step i is the K2 claim of the sub-job (chunk i, one greedy token, nonce \
H(job, i)) over the rule program with param commitments = base overlaid by the pre-state (the line head for step 0, step i-1's \
committed write otherwise); boundary roots derived from the traces; the pre-state is the claim's DA (stage 0x40); the line head \
advances at Final if unmoved and rolls back on a post-Final conviction. Retrieval: items {i32 key, u32 payload} under a binary \
Merkle snapshot root; Flat index; score = clamp(dot, +-(2^SB-1)), kappa = (s+2^SB)*2^b + (2^b-1-id), b = ceil(log2 N); the min(k,N) \
largest kappa in descending order; courts: wrong item (opening or score), missed better item; slices of B items are the claim's DA \
(stage 0x80+s). Composite: 2..8 stages over registered Weights (model) and Retrieval (tool) classes; token edges (job prompt, \
payloads, delivered tokens) recomputed at inclusion; the logits-to-query edge judged by opening the upstream committed logits.";

/// **`K2-TR-v1`**: the typed-roots extension (kernel line 3). It has no TIR families of its own — every TIR value a typed class
/// commits is judged by the K2-TIR descriptor of its program — so it is a pure activation and meaning token: a typed class names its
/// digest, and registers only while it is Active in the ledger's schedule.
pub fn k2_tr_v1_descriptor() -> KernelDescriptorV1 {
    let mut d = k2_tir_v2_descriptor();
    d.kernel_id = 3;
    d.version = 1;
    d.semantics_digest = crate::hash::id(KERNEL_DESCRIPTOR_DOMAIN_V1, K2_TR_V1_SEMANTICS);
    d.constraint_set_id = 0;
    d.court_suite_id = 0;
    d.families = Vec::new();
    d
}

/// The root kinds this binary implements, with their versions (the census predicate of RFC-0011 §18 reads this list).
pub const SUPPORTED_KINDS_V1: [(&str, u16); 4] = [("weights", 1), ("memory", 1), ("retrieval", 1), ("composite", 1)];

/// **The `Weights` root kind** (version 1): exactly today's single-program registration payload.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WeightsRootV1 {
    pub descriptor: Digest,
    pub program_bytes: Vec<u8>,
    pub plan: VerificationPlanV1,
    pub param_commitments: ParamCommitmentsV1,
}

/// A typed root. Each kind's version is its discriminant: a new version is a new discriminant, never a reinterpretation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum TypedRootV1 {
    WeightsV1(WeightsRootV1) = 0,
    MemoryV1(MemoryRootV1) = 1,
    RetrievalV1(RetrievalRootV1) = 2,
    CompositeV1(CompositeRootV1) = 3,
}

impl TypedRootV1 {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::WeightsV1(_) => "weights/v1",
            Self::MemoryV1(_) => "memory/v1",
            Self::RetrievalV1(_) => "retrieval/v1",
            Self::CompositeV1(_) => "composite/v1",
        }
    }

    fn extension(&self) -> Option<&Digest> {
        match self {
            Self::WeightsV1(_) => None,
            Self::MemoryV1(m) => Some(&m.extension),
            Self::RetrievalV1(r) => Some(&r.extension),
            Self::CompositeV1(c) => Some(&c.extension),
        }
    }
}

/// **The computation specification a class binds.**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ComputationSpecV1 {
    pub version: u16,
    pub mode: VerificationModeV1,
    pub roots: Vec<TypedRootV1>,
}

/// The accepted combinations of roots.
#[derive(Clone, Copy, Debug)]
pub enum SpecShapeV1<'a> {
    Weights(&'a WeightsRootV1),
    Memory(&'a WeightsRootV1, &'a MemoryRootV1),
    Retrieval(&'a RetrievalRootV1),
    Composite(&'a CompositeRootV1),
}

impl ComputationSpecV1 {
    /// The combination, or the refusal by name.
    pub fn shape(&self) -> Result<SpecShapeV1<'_>, String> {
        if self.version != SPEC_VERSION_V1 {
            return Err(format!("computation specification version {} is not implemented", self.version));
        }
        Ok(match self.roots.as_slice() {
            [TypedRootV1::WeightsV1(w)] => SpecShapeV1::Weights(w),
            [TypedRootV1::WeightsV1(w), TypedRootV1::MemoryV1(m)] => SpecShapeV1::Memory(w, m),
            [TypedRootV1::RetrievalV1(r)] => SpecShapeV1::Retrieval(r),
            [TypedRootV1::CompositeV1(c)] => SpecShapeV1::Composite(c),
            other => {
                let names: Vec<&str> = other.iter().map(TypedRootV1::name).collect();
                return Err(format!("KIND_COMBINATION_UNSUPPORTED [{}]", names.join(", ")));
            }
        })
    }

    /// Every typed root names this binary's extension (a root naming another is `KERNEL_EXTENSION_REQUIRED`).
    pub fn check_extension(&self) -> Result<(), String> {
        let ours = k2_tr_v1_descriptor().digest();
        match self.roots.iter().find(|r| r.extension().is_some_and(|e| *e != ours)) {
            Some(r) => {
                Err(format!("KERNEL_EXTENSION_REQUIRED [{}]: the root names an extension this binary does not implement", r.name()))
            }
            None => Ok(()),
        }
    }

    /// **The class id.** `[WeightsV1]` is the legacy id ([`crate::ledger::single_class_id_v1`], the mode bound as today); every typed
    /// class is `H(spec-class/v1; borsh(spec))` — the mode, every root and the extension inside.
    pub fn class_id(&self) -> Result<Digest, String> {
        Ok(match self.shape()? {
            SpecShapeV1::Weights(w) => {
                crate::ledger::single_class_id_v1(w.descriptor, &w.program_bytes, &w.plan, &w.param_commitments, self.mode)
            }
            _ => object_id(SPEC_CLASS_DOMAIN_V1, self),
        })
    }
}

/// The sub-objects of the kernel route's `Spec` object.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SpecObjectV1 {
    /// Any registered bond.
    RegisterClass { spec: ComputationSpecV1 } = 0,
    /// Any registered bond.
    PostJob { job: SpecJobV1 } = 1,
    /// Signed by the claim's producer, over its seal (`SealClaim` with the spec job's id).
    CommitClaim { claim: SpecClaimV1 } = 2,
}

/// Per-sub-object ceilings inside the route's one `Spec` ceiling.
pub const MAX_SPEC_REGISTER_BYTES_V1: usize = crate::route::MAX_REGISTER_CLASS_BYTES_V1;
pub const MAX_SPEC_JOB_BYTES_V1: usize = crate::route::MAX_POST_JOB_BYTES_V1;
pub const MAX_SPEC_CLAIM_BYTES_V1: usize = crate::route::MAX_COMMIT_CLAIM_BYTES_V1;

impl SpecObjectV1 {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::RegisterClass { .. } => "SpecRegisterClass",
            Self::PostJob { .. } => "SpecPostJob",
            Self::CommitClaim { .. } => "SpecCommitClaim",
        }
    }

    pub const fn max_bytes(&self) -> usize {
        match self {
            Self::RegisterClass { .. } => MAX_SPEC_REGISTER_BYTES_V1,
            Self::PostJob { .. } => MAX_SPEC_JOB_BYTES_V1,
            Self::CommitClaim { .. } => MAX_SPEC_CLAIM_BYTES_V1,
        }
    }

    /// The actor the object names (the signer must be it): the producer of a claim.
    pub fn named_actor(&self) -> Option<(Digest, &'static str)> {
        match self {
            Self::CommitClaim { claim } => Some((claim.producer(), "producer")),
            _ => None,
        }
    }
}

/// A typed job.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SpecJobV1 {
    Memory(MemoryJobV1) = 0,
    Retrieval(RetrievalJobV1) = 1,
    Composite(CompositeJobV1) = 2,
}

impl SpecJobV1 {
    pub fn id(&self) -> Digest {
        object_id(SPEC_JOB_DOMAIN_V1, self)
    }

    pub fn class(&self) -> Digest {
        match self {
            Self::Memory(j) => j.class,
            Self::Retrieval(j) => j.class,
            Self::Composite(j) => j.class,
        }
    }
}

/// A typed claim.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SpecClaimV1 {
    Memory(MemoryClaimV1) = 0,
    Retrieval(RetrievalClaimV1) = 1,
    Composite(CompositeClaimV1) = 2,
}

impl SpecClaimV1 {
    /// The claim id: every byte of the claim (its commitments included), under the spec claim domain.
    pub fn id(&self) -> Digest {
        object_id(SPEC_CLAIM_DOMAIN_V1, self)
    }

    pub fn job_id(&self) -> Digest {
        match self {
            Self::Memory(c) => c.job_id,
            Self::Retrieval(c) => c.job_id,
            Self::Composite(c) => c.job_id,
        }
    }

    pub fn producer(&self) -> Digest {
        match self {
            Self::Memory(c) => c.producer_bond,
            Self::Retrieval(c) => c.producer_bond,
            Self::Composite(c) => c.producer_bond,
        }
    }

    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Memory(_) => "memory",
            Self::Retrieval(_) => "retrieval",
            Self::Composite(_) => "composite",
        }
    }
}

/// **A typed claim as the ledger stores it**: the claim, and (memory) the pre-state it committed over, as one "position" whose values
/// are the slot commitments (demand stage [`MEMORY_PRE_STATE_STAGE_V1`]).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SpecClaimBodyV1 {
    pub claim: SpecClaimV1,
    /// Memory: `[slot commitments of the line head at commit]`; empty otherwise.
    pub pre_state: Vec<Vec<Digest>>,
}

impl SpecClaimBodyV1 {
    /// A committed "position" of a demand stage: memory stage 0 (the steps' positions in one index) and `0x40` (the pre-state);
    /// composite stage `s` (a model stage's positions). Snapshot slices are not committed values (see the ledger's slice demand).
    pub fn position(&self, stage: u8, position: u32) -> Option<(&[Vec<Digest>], &[Digest])> {
        match &self.claim {
            SpecClaimV1::Memory(c) if stage == 0 => {
                let (i, p) = locate_v1(&c.steps, position)?;
                Some((c.steps[i].commitments.get(p as usize)?.as_slice(), &[]))
            }
            SpecClaimV1::Memory(_) if stage == MEMORY_PRE_STATE_STAGE_V1 && position == 0 => Some((self.pre_state.as_slice(), &[])),
            SpecClaimV1::Composite(c) => match c.stages.get(stage as usize)? {
                StageClaimV1::Model { commitments, .. } => Some((commitments.get(position as usize)?.as_slice(), &[])),
                StageClaimV1::Retrieval { .. } => None,
            },
            _ => None,
        }
    }

    /// `(stage, positions)` of every committed demand stage.
    pub fn stages(&self) -> Vec<(u8, u32)> {
        match &self.claim {
            SpecClaimV1::Memory(c) => {
                vec![(0, c.steps.iter().map(|s| s.commitments.len() as u32).sum()), (MEMORY_PRE_STATE_STAGE_V1, 1)]
            }
            SpecClaimV1::Composite(c) => c
                .stages
                .iter()
                .enumerate()
                .filter_map(|(s, st)| match st {
                    StageClaimV1::Model { commitments, .. } => Some((s as u8, commitments.len() as u32)),
                    StageClaimV1::Retrieval { .. } => None,
                })
                .collect(),
            SpecClaimV1::Retrieval(_) => Vec::new(),
        }
    }

    /// What the claim committed, as one digest (the OPV Final receipt's execution commitment).
    pub fn execution_root(&self) -> Digest {
        object_id(SPEC_EXECUTION_DOMAIN_V1, &self.claim)
    }
}

/// What a bond files against a typed claim (the bytes of [`crate::route::ProsecutionV1::Spec`]).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SpecFaultV1 {
    /// A kernel fault proof's canonical bytes against memory step `step`'s record.
    MemoryStep { step: u32, proof: Vec<u8> } = 0,
    /// Step `step`'s delivered token is not the greedy token of its committed logits.
    MemoryDecode { step: u32, logits: TensorWireV1 } = 1,
    /// A retrieval fault against stage `stage` (0 for a plain retrieval class).
    Retrieval { stage: u8, fault: RetrievalFaultV1 } = 2,
    /// A kernel fault proof against a composite's model stage.
    StageKernel { stage: u8, proof: Vec<u8> } = 3,
    /// A composite model stage's delivered token `index` is not the greedy token of its committed logits.
    StageDecode { stage: u8, index: u32, logits: TensorWireV1 } = 4,
    /// The edge court: retrieval stage `stage`'s carried query is not its upstream model stage's committed logits (opened whole).
    Edge { stage: u8, logits: TensorWireV1 } = 5,
}

impl SpecFaultV1 {
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("in-memory borsh")
    }
}

/// A class kind, with what its courts read (derived from the record and the ledger: never stored).
#[derive(Clone, Debug)]
pub enum SpecClassKindV1 {
    Memory { rule: Box<ClassRowV1>, root: MemoryRootV1, writers: Vec<(u16, u16)> },
    Retrieval { root: RetrievalRootV1 },
    Composite { root: CompositeRootV1, components: Vec<ComponentV1> },
}

/// A registered typed class: its specification (the stored record) and what is derived from it.
#[derive(Clone, Debug)]
pub struct SpecClassRowV1 {
    pub spec: ComputationSpecV1,
    pub kind: SpecClassKindV1,
    pub bounds: ProsecutionBoundsV1,
}

/// **The typed part of the ledger** (tables 22–24; every one in the root only when non-empty).
#[derive(Clone, Debug, Default)]
pub struct TypedStateV1 {
    pub classes: BTreeMap<Digest, SpecClassRowV1>,
    pub jobs: BTreeMap<Digest, SpecJobV1>,
    pub lines: BTreeMap<Digest, MemoryLineV1>,
}

impl TypedStateV1 {
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.jobs.is_empty() && self.lines.is_empty()
    }
}

/// **The typed root**: `base` itself when every typed table is empty (every existing root unchanged), else
/// `H(typed-root/v1; base, [(table, collection root)] of the non-empty tables)`.
pub fn typed_root_v1(base: Digest, parts: &[(u8, Digest)]) -> Digest {
    if parts.is_empty() { base } else { object_id(TYPED_STATE_ROOT_DOMAIN_V1, &(SPEC_VERSION_V1, base, parts.to_vec())) }
}

// ---- bounds per kind ------------------------------------------------------------------------------------------------------

fn bounded(what: &'static str, required: u128, limit: u128) -> Result<(), String> {
    if required >= u128::MAX / 2 || required > limit {
        return Err(format!("BOUNDS_EXCEEDED [{what}]: {required} > {limit}"));
    }
    Ok(())
}

fn slot_bytes(program: &TirProgramV1, root: &MemoryRootV1) -> u128 {
    root.slots
        .iter()
        .map(|s| {
            let d = &program.params[s.param.0 as usize];
            let n: u128 = d.shape.iter().map(|x| *x as u128).product();
            n.saturating_mul(d.dtype.width() as u128).saturating_add(WIRE_HEADER_BYTES_V1 as u128)
        })
        .sum()
}

/// **Memory bounds** (design §2.6) from the rule's per-step bounds `base`.
pub fn memory_bounds_v1(
    base: &ProsecutionBoundsV1,
    program: &TirProgramV1,
    plan: &VerificationPlanV1,
    root: &MemoryRootV1,
    policy: &ProsecutionPolicyV1,
) -> Result<ProsecutionBoundsV1, String> {
    let m = slot_bytes(program, root);
    let steps = root.max_steps as u128;
    let evidence = 64 * 16 + 64 * (plan.max_positions as u128) * 2;
    let b = ProsecutionBoundsV1 {
        max_public_bytes: base.max_public_bytes.saturating_add(m),
        max_opening_bytes: base.max_opening_bytes,
        max_filing_bytes: base.max_filing_bytes.saturating_add(64),
        max_response_bytes: base.max_response_bytes.max(m.saturating_add(WIRE_HEADER_BYTES_V1 as u128)),
        max_localization_rounds: 2,
        max_court_work: base.max_court_work,
        max_verifier_ram: base.max_verifier_ram.saturating_add(m),
        max_retained_state: steps
            .saturating_mul(base.max_retained_state.saturating_add(evidence))
            .saturating_add(64 * (steps + 1))
            .saturating_add(64 * root.slots.len() as u128),
        max_concurrent_sessions: (root.max_steps as u64 * plan.max_positions as u64 + 1).min(u32::MAX as u64) as u32,
        deadline_daa: policy.court_deadline_daa,
    };
    bounded("public bytes", b.max_public_bytes, policy.max_public_bytes)?;
    bounded("verifier RAM", b.max_verifier_ram, policy.max_verifier_ram)?;
    bounded("retained state", b.max_retained_state, policy.max_retained_state)?;
    bounded("concurrent sessions", b.max_concurrent_sessions as u128, policy.max_sessions_per_claim as u128)?;
    Ok(b)
}

/// One retrieval item with its digests, an upper bound.
fn item_bytes(root: &RetrievalRootV1) -> u128 {
    RetrievalItemV1::bytes(root.snapshot.dim, root.snapshot.max_payload) as u128 + 128
}

/// **Retrieval bounds** (design §3.5). `limits` are the extension descriptor's.
pub fn retrieval_bounds_v1(
    root: &RetrievalRootV1,
    descriptor: &KernelDescriptorV1,
    policy: &ProsecutionPolicyV1,
) -> Result<ProsecutionBoundsV1, String> {
    root.well_formed()?;
    let s = &root.snapshot;
    let item = item_bytes(root);
    let path = 64 * depth_v1(s.items) as u128;
    let k = root.result_len() as u128;
    let opening = item + path;
    let response = (s.slice_items as u128).saturating_mul(opening + 16).saturating_add(WIRE_HEADER_BYTES_V1 as u128);
    let retained = k * 144 + 64;
    let b = ProsecutionBoundsV1 {
        max_public_bytes: response.saturating_add(retained),
        max_opening_bytes: opening.min(u64::MAX as u128) as u64,
        max_filing_bytes: (opening + FILING_HEADER_BYTES_V1 as u128).min(u64::MAX as u128) as u64,
        max_response_bytes: response,
        max_localization_rounds: 2,
        max_court_work: (s.dim as u64).saturating_add(depth_v1(s.items) as u64).saturating_add(k as u64).saturating_add(16),
        max_verifier_ram: response.saturating_add(opening),
        max_retained_state: retained,
        max_concurrent_sessions: s.slices().min(u32::MAX as u64) as u32,
        deadline_daa: policy.court_deadline_daa,
    };
    bounded("opening bytes", b.max_opening_bytes as u128, descriptor.limits.max_court_bytes as u128)?;
    bounded("court work", b.max_court_work as u128, descriptor.limits.max_court_work as u128)?;
    bounded("public bytes", b.max_public_bytes, policy.max_public_bytes)?;
    bounded("verifier RAM", b.max_verifier_ram, policy.max_verifier_ram)?;
    bounded("concurrent sessions", b.max_concurrent_sessions as u128, policy.max_sessions_per_claim as u128)?;
    Ok(b)
}

/// **What detecting a missed item costs** (the outsider's choice, never what G14 bounds): the whole snapshot.
pub fn retrieval_claim_material_bytes_v1(root: &RetrievalRootV1) -> u128 {
    (root.snapshot.items as u128).saturating_mul(item_bytes(root))
}

/// **Composite bounds** (design §4): Σ over stages for public bytes, RAM, retained state and sessions; max for the rest; the edge
/// filing (one logits vector) included.
pub fn composite_bounds_v1(
    root: &CompositeRootV1,
    components: &[ComponentV1],
    descriptor: &KernelDescriptorV1,
    policy: &ProsecutionPolicyV1,
) -> Result<ProsecutionBoundsV1, String> {
    let mut t = ProsecutionBoundsV1 {
        max_public_bytes: 0,
        max_opening_bytes: 0,
        max_filing_bytes: 0,
        max_response_bytes: 0,
        max_localization_rounds: 2,
        max_court_work: 0,
        max_verifier_ram: 0,
        max_retained_state: 0,
        max_concurrent_sessions: 0,
        deadline_daa: policy.court_deadline_daa,
    };
    for (st, c) in root.stages.iter().zip(components) {
        let b = match c {
            ComponentV1::Model(m) => m.bounds,
            ComponentV1::Retrieval(r) => {
                let mut b = retrieval_bounds_v1(r, descriptor, policy)?;
                if let StageInputV1::Query(composite::QuerySourceV1::StageLogits { .. }) = st.input {
                    // The edge filing opens one i32 logits vector of the snapshot's key length.
                    let edge = 4 * r.snapshot.dim as u64 + WIRE_HEADER_BYTES_V1 + FILING_HEADER_BYTES_V1;
                    b.max_filing_bytes = b.max_filing_bytes.max(edge);
                }
                b
            }
        };
        t.max_public_bytes = t.max_public_bytes.saturating_add(b.max_public_bytes);
        t.max_opening_bytes = t.max_opening_bytes.max(b.max_opening_bytes);
        t.max_filing_bytes = t.max_filing_bytes.max(b.max_filing_bytes);
        t.max_response_bytes = t.max_response_bytes.max(b.max_response_bytes);
        t.max_court_work = t.max_court_work.max(b.max_court_work);
        t.max_verifier_ram = t.max_verifier_ram.saturating_add(b.max_verifier_ram);
        t.max_retained_state = t.max_retained_state.saturating_add(b.max_retained_state);
        t.max_concurrent_sessions = t.max_concurrent_sessions.saturating_add(b.max_concurrent_sessions);
    }
    bounded("public bytes", t.max_public_bytes, policy.max_public_bytes)?;
    bounded("verifier RAM", t.max_verifier_ram, policy.max_verifier_ram)?;
    bounded("retained state", t.max_retained_state, policy.max_retained_state)?;
    bounded("concurrent sessions", t.max_concurrent_sessions as u128, policy.max_sessions_per_claim as u128)?;
    Ok(t)
}

/// **The roots a typed claim's challenge subject names**: `(program, artifact, state)` — memory: the rule program, its artifact (base
/// weights and `M0`) and `(pre root, post root)`; retrieval: the index commitment and the snapshot root; composite: the component ids.
pub fn subject_roots_v1(class: &SpecClassRowV1, body: &SpecClaimBodyV1) -> (Digest, Digest, Option<(Digest, Digest)>) {
    match (&class.kind, &body.claim) {
        (SpecClassKindV1::Memory { rule, .. }, claim) => {
            let state = match claim {
                SpecClaimV1::Memory(c) => c.step_roots.first().copied().zip(c.step_roots.last().copied()),
                _ => None,
            };
            (crate::public::program_root_v1(&rule.program_bytes), rule.param_commitments.root(), state)
        }
        (SpecClassKindV1::Retrieval { root }, _) => (root.index_commitment(), root.snapshot.root(), None),
        (SpecClassKindV1::Composite { root, .. }, _) => {
            let ids: Vec<Digest> = root.stages.iter().map(|s| s.component).collect();
            let d = object_id(SPEC_CLASS_DOMAIN_V1, &ids);
            (d, d, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_is_its_own_descriptor_and_well_formed() {
        let d = k2_tr_v1_descriptor();
        d.well_formed().unwrap();
        assert_ne!(d.digest(), k2_tir_v2_descriptor().digest());
        assert_eq!((d.kernel_id, d.version), (3, 1));
        assert!(d.families.is_empty(), "an extension judges no TIR family of its own");
    }

    #[test]
    fn a_combination_outside_the_four_is_refused_by_name_and_a_version_too() {
        let r = RetrievalRootV1 {
            extension: k2_tr_v1_descriptor().digest(),
            snapshot: retrieval::SnapshotV1 { items: 4, dim: 2, max_payload: 1, slice_items: 2, merkle_root: [0; 64] },
            index: retrieval::IndexV1::Flat,
            rule: retrieval::RetrievalRuleV1::TopKCountingV1 { k: 1, score_bits: 16 },
        };
        let two = ComputationSpecV1 {
            version: 1,
            mode: VerificationModeV1::OptimisticPublicVerification,
            roots: vec![TypedRootV1::RetrievalV1(r.clone()), TypedRootV1::RetrievalV1(r.clone())],
        };
        assert!(two.shape().unwrap_err().starts_with("KIND_COMBINATION_UNSUPPORTED [retrieval/v1, retrieval/v1]"));
        let v2 = ComputationSpecV1 { version: 2, roots: vec![TypedRootV1::RetrievalV1(r.clone())], ..two.clone() };
        assert!(v2.shape().is_err());
        let mut alien = r;
        alien.extension = [7; 64];
        let alien = ComputationSpecV1 { version: 1, roots: vec![TypedRootV1::RetrievalV1(alien)], ..two };
        assert!(alien.check_extension().unwrap_err().starts_with("KERNEL_EXTENSION_REQUIRED [retrieval/v1]"));
        assert!(borsh::from_slice::<TypedRootV1>(&[9]).is_err(), "an unknown kind does not decode");
    }
}
