//! **The consensus rules public prosecution relies on, as a deterministic fold a consensus node can embed.**
//!
//! [`KernelLedgerV1`] is a state that advances by the per-object API a consensus fold calls: [`KernelLedgerV1::begin_block`], then
//! per signed object [`KernelLedgerV1::apply_object`] (transactional: a refusal leaves the state byte-identical), the
//! consumer-derived inputs [`KernelLedgerV1::sync_bond`], [`KernelLedgerV1::attest_artifact`] and
//! [`KernelLedgerV1::apply_panel_tally`], and finally [`KernelLedgerV1::tick`]. [`KernelLedgerV1::apply_block`] and
//! [`KernelLedgerV1::replay`] are thin wrappers over those. The state is [`KernelLedgerV1::root`] (a versioned canonical root over
//! per-collection roots, see [`crate::state`]); **events are per-block receipts the consumer stores, not state**.
//!
//! It is not wired into the node here, and the real-chain drill stays an external gate. Every rule below reads only ledger state —
//! registered classes (program, plan and the artifact's *commitments*), posted jobs, committed claims (evidence object and trace
//! commitments) and material served in answer to demands — and the bytes of the object being applied, so every adjudication is
//! something any node replaying the blocks recomputes.
//!
//! * **Bonds are the consumer's.** The ledger holds a view of each bond's real locked collateral ([`KernelLedgerV1::sync_bond`]) and
//!   reservations against it; it never mints. Every slash, accuser reward, default share, refund, reservation and Final reward is an
//!   explicit [`crate::settle::SettlementInstructionV1`] receipt the consumer applies to its real bonds. Objects are signed by a
//!   bond ([`AuthV1`]); the signer must be the producer / accuser / demander the object names. The Panel's coverage is not an object:
//!   the consumer derives it ([`KernelLedgerV1::apply_panel_tally`]).
//! * **Collateral** (category 19): a claim reserves `claim_collateral` of its producer bond's FREE collateral (no double use);
//!   the reservation lasts until the claim's liability horizon ends; an exiting bond backs nothing new and withdraws only after
//!   its delay with nothing reserved.
//! * **Inclusion refusals** (categories 5–9, 12, 13): a claim whose evidence names another job, class, input (a borrowed trace),
//!   `R` seed, output, length or generation budget, whose trace commitments are not the evidence's, or whose evidence a court
//!   could only call malformed, is refused at inclusion — objectively, from public objects.
//! * **Direct proofs** (categories 1–4, 7–9, 11, 13, 16): a kernel fault proof, a decode fault, or a pipeline stage / edge /
//!   decode fault is adjudicated in the block that carries it by the same courts any node runs, from ledger state alone. It is
//!   never refused because a demand or another session is open: open sessions on a convicted claim settle as moot and their
//!   bonds return (no pre-emption). Courts read consensus state and the filing's bytes only — never the artifact: a `MatMul`
//!   scalar opens one row of `X`, one column of `W` and one row of `Y` against the registered *commitments*.
//! * **Demands** (categories 10, 14, 15): any bond may demand one committed POSITION of one stage (every node value and stage
//!   input of it); joining an open demand for the same position shares its progress. One session per stage position, so every
//!   position is demandable at once and no set of demanders (a producer's friends included) can starve another's: one round of
//!   demands then one direct proof is every prosecution's whole path. The response is bounded and classified (served /
//!   malformed / wrong bytes / wrong root / fake opening / partial / oversized); silence or non-serving past the deadline is
//!   the producer's availability default — a fixed penalty, never the fraud slash. A bond's open demands are limited by its
//!   free collateral, not by a count.
//! * **Filings are bounded and priced**: a filed proof must fit the class's filing envelope, and a dismissed one forfeits
//!   `dismissed_proof_fee` (a convicting or duplicate one does not). Each block has an adjudication budget
//!   ([`LedgerPolicyV1::max_adjudications_per_block`], `max_court_work_per_block`): an object past it is refused (dropped, never
//!   fatal, no fee) and may be included in a later block.
//! * **Final** (category 17): a claim needs the Panel's covered tally, the closed window, no open demand and — after a demand was
//!   served — `proof_grace_daa` more, so the prosecution the served values enable can still be filed before Final (a service
//!   in the window's last block must not hand the claim Final before the demander can use what it was served). New demands are
//!   accepted only while the window is open and each lives `court_deadline_daa`, so no spam extends Final past
//!   `window end + court deadline + proof grace`: demands open only inside the window, a join shares the open demand's
//!   deadline, and a position is served once. A dismissed direct proof changes nothing.
//! * **Liability** (category A): after Final the reservation is held for `liability_daa`; a proof convicting in that horizon
//!   slashes it (post-Final liability), even if every Panel seat signed the claim covered. A post-Final demand is accepted only
//!   while its whole path (deadline, service, grace) still fits inside the horizon; one that defaults forfeits the WHOLE
//!   remaining reservation (the reward was already paid), an availability outcome that is never the fraud conviction.
//! * **RFC-0015 `OptimisticPublicVerification`** (dormant; [`crate::opv`]): a class registered under that mode (route tags 13 / 14,
//!   the mode bound into the class id) has no Panel. Its claims are Challengeable from inclusion for a fixed window, hold their
//!   job from the first reveal, reserve a producer collateral sized to the claim's maximum gain, and reach Final by the explicit
//!   window rule alone; demands, proofs, default, post-Final liability and the one-claim-per-job and seal-then-reveal rules are
//!   the Panel-licensed claims' own. A Panel tally for such a claim is refused.
//! * **Idempotence and replay** (category 18): a claim is convicted at most once (a second proof is `Duplicate`); the state is
//!   a pure function of the block sequence, so a reorg is a replay of the new branch and a restart or IBD is a replay from
//!   genesis ([`KernelLedgerV1::replay`]).

use std::collections::{BTreeMap, BTreeSet};

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::pipeline::{TirPipelineV1, stream_stage};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::{MapParams, Tensor};

use crate::check::check_plan_v1;
use crate::descriptor::{ContextPolicyV1, KernelDescriptorV1, KernelScheduleV1, ModelKernelBindingV1};
use crate::evidence::{EvidenceHeaderV1, VerificationEvidenceV1};
use crate::gate::{ProsecutionBoundsV1, ProsecutionPolicyV1, public_pipeline_prosecution_complete_v1, public_prosecution_complete_v1};
use crate::hash::Digest;
use crate::job::{BindingFaultV1, DecodeFaultV1, DecodeRuleV1, KernelClaimV1, KernelJobV1, binding_fault_v1, verify_decode_fault_v1};
use crate::lifecycle::{ClaimEventV1, ClaimLifecycleV1, ClaimStateV1, LifecyclePolicyV1};
use crate::mode::{VerificationModeV1, class_id_for_mode_v1};
use crate::opv::OpvStateV1;
use crate::pipeline::{
    PipelineEvidenceV1, PipelineHeaderV1, PipelinePlanV1, check_pipeline_plan_v1, pipeline_job_root_v1, pipeline_root_v1,
    stage_view_v1,
};
use crate::pipeline_public::{
    FreshPipelineVerifierV1, PipelineClaimV1, PipelineClassV1, PipelineFindingV1, PipelineJobPostV1, PipelinePublicRecordV1,
    StageCommitmentsV1,
};
use crate::plan::VerificationPlanV1;
use crate::public::{
    FreshVerifierV1, ProfileMaterialV1, PublicClaimRecordV1, ServedPositionV1, TensorWireV1, classify_position_response_v1,
    program_root_v1,
};
use crate::receipt::TallyStateV1;
pub use crate::route::{AuthV1, KernelRefusalV1, KernelRouteObjectV1, ProsecutionV1, RefusalKindV1, claim_seal_v1};
use crate::settle::{SettlementInstructionV1, SettlementKindV1};
use crate::trace::{EvidenceV1, ParamCommitmentsV1, derived_mask_v1, tensor_commitment};
use crate::verify::MaterialV1;

/// The network's ledger policy: a consensus constant the consumer fixes at genesis (chain identity included — an object never
/// names its own network, ruleset or challenge policy).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LedgerPolicyV1 {
    /// `palw_network_domain_v2`: network id and chain genesis.
    pub network_domain: Digest,
    pub ruleset_digest: Digest,
    /// The id of the `PostCommitChallengePolicyV1` in force (`misaka-palw-challenge`), bound into every claim's challenge subject.
    pub challenge_policy_id: Digest,
    pub claim_collateral: u64,
    pub demand_bond: u64,
    pub check_window_daa: u64,
    pub challenge_window_daa: u64,
    pub court_deadline_daa: u64,
    /// After a demand is served, the claim may not reach Final for this long (the demander files the proof the served values enable).
    pub proof_grace_daa: u64,
    pub liability_daa: u64,
    pub exit_delay_daa: u64,
    /// What a dismissed proof forfeits (burned): filings are not free court work.
    pub dismissed_proof_fee: u64,
    /// Of a slashed reservation, the convicting accuser's share (permille); the rest is burned.
    pub accuser_reward_permille: u16,
    /// What an availability default forfeits (to the demanders), never more than the reservation.
    pub default_penalty: u64,
    /// Paid to the producer at Final (a settlement instruction; the consumer's reward path funds it).
    pub claim_reward: u64,
    /// The most court / classification / inclusion-check runs one block may trigger; an object past it is refused (dropped).
    pub max_adjudications_per_block: u32,
    /// The most court work (the class's declared worst court work, summed over the block's filed proofs) one block may trigger.
    pub max_court_work_per_block: u64,
    /// A claim commits only over its producer's seal at least this old (≥ 1: a seal in the same block as the reveal proves nothing).
    pub claim_seal_delay_daa: u64,
    /// An unrevealed seal is dropped after this long (bounds the state junk seals can occupy).
    pub seal_ttl_daa: u64,
    pub prosecution: ProsecutionPolicyV1,
}

impl LedgerPolicyV1 {
    /// The relations among the timings and amounts every rule below relies on.
    pub fn validate(&self) -> Result<(), String> {
        let p = self;
        let checks: [(bool, &str); 13] = [
            (p.court_deadline_daa == p.prosecution.court_deadline_daa, "the ledger's court deadline is the gate's"),
            (p.court_deadline_daa > 0 && p.challenge_window_daa > 0 && p.check_window_daa > 0, "every window is non-empty"),
            (p.proof_grace_daa > 0, "a served demand leaves a non-empty grace to file the proof it enables"),
            // A demand filed in the window's last block closes by `window end + court deadline`; the proof it enables must still
            // reach the claim, after Final if need be.
            (
                p.liability_daa > p.court_deadline_daa.saturating_add(p.proof_grace_daa),
                "the liability horizon outlasts a demand's deadline and the proof grace after it",
            ),
            (p.claim_reward < p.claim_collateral, "the Final reward is smaller than the reservation a post-Final default forfeits"),
            (p.default_penalty <= p.claim_collateral, "a default never takes more than the reservation"),
            (p.demand_bond > 0 && p.dismissed_proof_fee > 0, "demands and filings are not free"),
            (p.accuser_reward_permille <= 1000, "the accuser's share is a share"),
            (p.claim_collateral > 0, "a claim reserves collateral"),
            (p.max_adjudications_per_block > 0, "a block may adjudicate"),
            (p.max_court_work_per_block > 0, "a block may run a court"),
            (p.claim_seal_delay_daa > 0, "a seal precedes its reveal by at least one block"),
            (p.seal_ttl_daa >= p.claim_seal_delay_daa, "a seal lives long enough to be revealed"),
        ];
        match checks.iter().find(|(ok, _)| !ok) {
            Some((_, why)) => Err(format!("ledger policy: {why}")),
            None => Ok(()),
        }
    }
}

/// The ledger's view of one bond: the consumer's real locked collateral and what the kernel route has reserved against it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BondRowV1 {
    pub collateral: u64,
    pub reserved: u64,
    pub exit_requested: Option<u64>,
}

impl BondRowV1 {
    pub fn free(&self) -> u64 {
        self.collateral.saturating_sub(self.reserved)
    }
}

/// A registered single-program class: everything public a court reads about it. The artifact's tensors are NOT here — only their
/// commitments; outsiders read the tensors through a [`PublicArtifactV1`] source, authenticated against the commitments.
#[derive(Clone, Debug)]
pub struct ClassRowV1 {
    pub descriptor: KernelDescriptorV1,
    pub program_bytes: Vec<u8>,
    pub program: TirProgramV1,
    pub plan: VerificationPlanV1,
    pub param_commitments: ParamCommitmentsV1,
    pub network_domain: Digest,
    pub ruleset_digest: Digest,
    /// The code-derived gate's bounds (registration refuses a class without them).
    pub bounds: ProsecutionBoundsV1,
}

impl ClassRowV1 {
    pub fn header(&self, class_binding_id: Digest) -> EvidenceHeaderV1 {
        EvidenceHeaderV1 {
            network_domain: self.network_domain,
            ruleset_digest: self.ruleset_digest,
            class_binding_id,
            program_root: program_root_v1(&self.program_bytes),
            artifact_root: self.param_commitments.root(),
            plan_root: self.plan.root(),
        }
    }

    /// Where the logits sit in a position's trace: `(post occurrence, logits node)`.
    pub fn logits_at(&self) -> (u16, u16) {
        ((self.program.occurrences().len() - 1) as u16, self.program.logits)
    }
}

/// A registered pipeline class (K2-TIR-v3).
#[derive(Clone, Debug)]
pub struct PipelineClassRowV1 {
    pub descriptor: KernelDescriptorV1,
    pub pipeline_bytes: Vec<u8>,
    pub pipeline: TirPipelineV1,
    pub program_bytes: Vec<Vec<u8>>,
    pub programs: Vec<TirProgramV2>,
    pub plan: PipelinePlanV1,
    /// Each program's artifact commitments (the tensors are public off-chain material).
    pub param_commitments: Vec<ParamCommitmentsV1>,
    pub binding: PipelineClassV1,
    pub network_domain: Digest,
    pub ruleset_digest: Digest,
    pub bounds: ProsecutionBoundsV1,
}

impl PipelineClassRowV1 {
    pub fn header(&self, class_binding_id: Digest) -> PipelineHeaderV1 {
        PipelineHeaderV1 { network_domain: self.network_domain, ruleset_digest: self.ruleset_digest, class_binding_id }
    }
}

/// What a committed claim is: a single program's or a pipeline's.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ClaimBodyV1 {
    Program { claim: KernelClaimV1, evidence: VerificationEvidenceV1, commitments: Vec<Vec<Vec<Digest>>> } = 0,
    Pipeline { claim: PipelineClaimV1, evidence: PipelineEvidenceV1, stages: Vec<StageCommitmentsV1> } = 1,
}

impl ClaimBodyV1 {
    /// Stage `stage`'s committed node values and inputs at `position` (a single program is stage 0, with no inputs).
    pub fn position(&self, stage: u8, position: u32) -> Option<(&[Vec<Digest>], &[Digest])> {
        match self {
            Self::Program { commitments, .. } if stage == 0 => Some((commitments.get(position as usize)?, &[])),
            Self::Program { .. } => None,
            Self::Pipeline { stages, .. } => {
                let s = stages.get(stage as usize)?;
                let inputs = s.inputs.get(position as usize).map(Vec::as_slice).unwrap_or(&[]);
                Some((s.commitments.get(position as usize)?, inputs))
            }
        }
    }

    /// `(stage, positions)` of every stage.
    pub fn stages(&self) -> Vec<(u8, u32)> {
        match self {
            Self::Program { commitments, .. } => vec![(0, commitments.len() as u32)],
            Self::Pipeline { stages, .. } => stages.iter().enumerate().map(|(i, s)| (i as u8, s.commitments.len() as u32)).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ClaimRowV1 {
    pub producer: Digest,
    pub class_binding_id: Digest,
    pub job_id: Digest,
    pub body: ClaimBodyV1,
    pub committed_daa: u64,
    pub life: ClaimLifecycleV1,
    pub reserved: u64,
    pub liability_until: Option<u64>,
    pub convicted: bool,
    pub rewarded: bool,
}

/// An unrevealed seal: the claim seal and the DAA it was carried at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SealRowV1 {
    pub seal: Digest,
    pub daa: u64,
}

impl ClaimRowV1 {
    fn terminal_for_demands(&self) -> bool {
        self.convicted || matches!(self.life.state, ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. })
    }

    /// Whether this claim still holds its job (live or Final): only a failed claim frees the job for another.
    pub fn holds_job(&self) -> bool {
        !self.terminal_for_demands()
    }
}

/// The response classes a rejected response can have, as stored codes (`1 + index`).
pub const RESPONSE_CLASS_NAMES_V1: [&str; 6] = ["malformed", "wrong_bytes", "wrong_root", "fake_opening", "partial", "oversized"];

fn response_class_code(name: &str) -> u8 {
    RESPONSE_CLASS_NAMES_V1.iter().position(|n| *n == name).map(|i| i as u8 + 1).unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DemandRowV1 {
    /// Every bond that filed or joined it, with its bond amount.
    pub demanders: Vec<(Digest, u64)>,
    pub filed_daa: u64,
    pub deadline_daa: u64,
    /// The latest response's class code (`1 + index` into [`RESPONSE_CLASS_NAMES_V1`]).
    pub last: Option<u8>,
}

impl DemandRowV1 {
    /// The latest response's class name.
    pub fn last_class(&self) -> Option<&'static str> {
        self.last.and_then(|c| RESPONSE_CLASS_NAMES_V1.get((c as usize).wrapping_sub(1)).copied())
    }
}

/// The demand key: `(claim, stage, position)`.
pub type DemandKeyV1 = (Digest, u8, u32);

/// One input of a block. `SyncBond`, `AttestArtifact` and `PanelCovered` are **consumer-derived** from authenticated chain state
/// (never transactions anyone submits); `Object` is a signed public object.
#[derive(Clone, Debug)]
pub enum LedgerTxV1 {
    /// The real locked collateral of `bond`, as the consumer's bond state says.
    SyncBond { bond: Digest, collateral: u64 },
    /// The consumer's authenticated registry says the artifact with this root is publicly obtainable and was validated against its
    /// program (a class registers only over attested artifacts: weights are public by the consumer's fact, not a registrant flag).
    AttestArtifact { artifact_root: Digest },
    /// The Panel's tally reached coverage (an honest Panel, or every seat colluding: the ledger cannot tell, and need not).
    PanelCovered { claim: Digest },
    /// **RFC-0015 §4.1**: the network's policy admits the class with this id (its mode-bound id) for
    /// `OptimisticPublicVerification`. A registrant cannot pick the lighter mode for its own program; the consumer derives the
    /// admission from authenticated chain state (like [`Self::AttestArtifact`]).
    AdmitOptimisticClass { class: Digest },
    /// A signed public object.
    Object { auth: AuthV1, object: KernelRouteObjectV1 },
}

/// What an input did: a **receipt** (the consumer stores it per block; it is not part of the state).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum LedgerEventV1 {
    Refused {
        tx: &'static str,
        why: String,
    } = 0,
    ClassRegistered {
        class: Digest,
    } = 1,
    JobPosted {
        job: Digest,
    } = 2,
    ClaimCommitted {
        claim: Digest,
    } = 3,
    Convicted {
        claim: Digest,
        accuser: Digest,
        slashed: u64,
        accuser_reward: u64,
        post_final: bool,
    } = 4,
    ProofDismissed {
        claim: Digest,
        accuser: Digest,
        why: String,
        fee: u64,
    } = 5,
    Duplicate {
        claim: Digest,
    } = 6,
    DemandOpened {
        claim: Digest,
        stage: u8,
        position: u32,
        deadline: u64,
    } = 7,
    DemandJoined {
        claim: Digest,
        stage: u8,
        position: u32,
    } = 8,
    Served {
        claim: Digest,
        stage: u8,
        position: u32,
    } = 9,
    ResponseRejected {
        claim: Digest,
        stage: u8,
        position: u32,
        class: &'static str,
    } = 10,
    ProducerDefault {
        claim: Digest,
        stage: u8,
        position: u32,
        last: Option<&'static str>,
        penalty: u64,
    } = 11,
    /// A demand on a Final claim defaulted: the whole remaining reservation is forfeited (availability, never a conviction).
    PostFinalDefault {
        claim: Digest,
        stage: u8,
        position: u32,
        last: Option<&'static str>,
        forfeited: u64,
    } = 12,
    DemandsMoot {
        claim: Digest,
        refunded: u32,
    } = 13,
    Final {
        claim: Digest,
        reward: u64,
    } = 14,
    TimedOut {
        claim: Digest,
    } = 15,
    Released {
        claim: Digest,
    } = 16,
    ExitRequested {
        bond: Digest,
    } = 17,
    Withdrawn {
        bond: Digest,
        amount: u64,
    } = 18,
    /// An explicit instruction the consumer applies to its real bonds ([`crate::settle`]).
    Settlement(SettlementInstructionV1) = 19,
    ClaimSealed {
        job: Digest,
        producer: Digest,
    } = 20,
    /// **RFC-0009 §4.2 (lane DA16): a demand on a claim whose material obligation the consumer moved to bonded providers was not
    /// answered by its deadline.** The producer pays nothing (one failure, one party): the consumer charges every live lease of the claim
    /// and pays `demanders` from that charge; before Final the claim is `Unavailable { producer_defaulted: false }` (void, no reward), after
    /// Final its fact is withdrawn. The demand bonds were refunded exactly as on the producer path. Never a conviction. (Discriminant 30:
    /// 21–23 are G14-R4's.)
    ProviderLiableDefault {
        claim: Digest,
        stage: u8,
        position: u32,
        last: Option<&'static str>,
        post_final: bool,
        demanders: Vec<Digest>,
    } = 30,
    /// **DA16: every provider that stood behind a transferred claim's material has defaulted** (a common-mode outage): the claim lapses —
    /// before Final `Unavailable { producer_defaulted: false }` (no reward, the producer's reservation released, never slashed), after
    /// Final its fact is withdrawn. Never a conviction.
    ProviderLapsed {
        claim: Digest,
        post_final: bool,
    } = 31,
}

/// One block.
#[derive(Clone, Debug)]
pub struct LedgerBlockV1 {
    pub daa: u64,
    pub txs: Vec<LedgerTxV1>,
}

/// What the current block has spent of its adjudication budget. **Block-local scratch**: reset by `begin_block`, not part of the
/// state or its root (a refusal that did court work still spent it, so junk cannot be tried for free).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockBudgetV1 {
    pub adjudications: u32,
    pub court_work: u64,
}

/// **Any public source of the artifact's tensors** — a registry mirror, a DA provider, the model's host. Whatever it returns is
/// authenticated against the class's registered commitments before anything uses it.
pub trait PublicArtifactV1 {
    /// Program `program`'s param instance (a single-program class is program 0).
    fn param(&self, program: u16, index: u16, layer: Option<u16>) -> Option<Tensor>;
}

impl PublicArtifactV1 for MapParams {
    fn param(&self, program: u16, index: u16, layer: Option<u16>) -> Option<Tensor> {
        if program != 0 {
            return None;
        }
        self.tensors.get(&(index, layer)).cloned()
    }
}

impl PublicArtifactV1 for [MapParams] {
    fn param(&self, program: u16, index: u16, layer: Option<u16>) -> Option<Tensor> {
        self.get(program as usize)?.tensors.get(&(index, layer)).cloned()
    }
}

impl PublicArtifactV1 for Vec<MapParams> {
    fn param(&self, program: u16, index: u16, layer: Option<u16>) -> Option<Tensor> {
        self.as_slice().param(program, index, layer)
    }
}

/// The param instances a program may be committed for: every param, once per layer if it is per-layer.
fn declared_param_instances(params: &[misaka_palw_tir::program::ParamDecl], layers: usize) -> BTreeSet<(u16, Option<u16>)> {
    let mut out = BTreeSet::new();
    for (j, p) in params.iter().enumerate() {
        if p.per_layer {
            out.extend((0..layers).map(|l| (j as u16, Some(l as u16))));
        } else {
            out.insert((j as u16, None));
        }
    }
    out
}

/// The param instances a relation can read: every `Ref::Param(j)` (j < `real`: a pipeline stage's lifted inputs are committed as
/// stage inputs, not as artifact) of every node of every occurrence. A missing commitment for one makes the relation over it
/// unprovable ("no commitment for param"), so registration refuses a commitment set that lacks any.
fn used_param_instances(p: &TirProgramV1, real: usize) -> BTreeSet<(u16, Option<u16>)> {
    use misaka_palw_tir::program::Ref;
    let mut out = BTreeSet::new();
    for (b, layer) in p.occurrences() {
        for n in &p.blocks[b as usize].nodes {
            for r in &n.inputs {
                if let Ref::Param(j) = r
                    && (*j as usize) < real
                {
                    out.insert((*j, if p.params[*j as usize].per_layer { layer } else { None }));
                }
            }
        }
    }
    out
}

/// The commitments cover every instance a relation reads and name no instance the program does not declare.
fn check_commitment_set(
    pc: &ParamCommitmentsV1,
    used: &BTreeSet<(u16, Option<u16>)>,
    declared: &BTreeSet<(u16, Option<u16>)>,
) -> Result<(), String> {
    if let Some(missing) = used.iter().find(|k| !pc.by_instance.contains_key(k)) {
        return Err(format!("the artifact commitments lack param {} layer {:?}, which a relation reads", missing.0, missing.1));
    }
    if let Some(extra) = pc.by_instance.keys().find(|k| !declared.contains(k)) {
        return Err(format!(
            "the artifact commitments name param {} layer {:?}, which the program does not declare",
            extra.0, extra.1
        ));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct KernelLedgerV1 {
    pub policy: LedgerPolicyV1,
    pub schedule: KernelScheduleV1,
    /// Descriptors this binary implements (never bytes from a transaction).
    pub known: Vec<KernelDescriptorV1>,
    pub daa: u64,
    pub bonds: BTreeMap<Digest, BondRowV1>,
    pub classes: BTreeMap<Digest, ClassRowV1>,
    pub pipeline_classes: BTreeMap<Digest, PipelineClassRowV1>,
    pub jobs: BTreeMap<Digest, KernelJobV1>,
    pub pipeline_jobs: BTreeMap<Digest, PipelineJobPostV1>,
    pub claims: BTreeMap<Digest, ClaimRowV1>,
    pub demands: BTreeMap<DemandKeyV1, DemandRowV1>,
    /// Stage positions served in answer to demands: public from then on.
    pub served: BTreeMap<DemandKeyV1, ServedPositionV1>,
    /// Artifact roots the consumer attested public (see [`LedgerTxV1::AttestArtifact`]).
    pub attested_artifacts: BTreeSet<Digest>,
    /// **One claim per job**: the claim holding each job — the first the Panel covered. A job is paid once; a claim committed while
    /// another holds the job is refused, and a second claim's coverage is refused. A holder that fails (convicted, unavailable,
    /// timed out) frees the job.
    pub job_claims: BTreeMap<Digest, Digest>,
    /// Unrevealed seals, keyed `(job, producer)` (see [`KernelRouteObjectV1::SealClaim`]).
    pub seals: BTreeMap<(Digest, Digest), SealRowV1>,
    /// Cumulative amount burned (derived from the settlement instructions; kept as a checksum).
    pub burned: u64,
    /// RFC-0015: the OPV policy, classes and claim rows. Dormant (no policy) = the historical ledger, root included.
    pub opv: OpvStateV1,
    /// **RFC-0009 §4.2 (lane DA16): claims whose material obligation the consumer moved to bonded providers.** Consumer-derived from
    /// the consumer's own rooted rows and re-injected before every object and tick (like the attested set's source); NOT part of this
    /// state, its rows or its root. A demand on such a claim that nobody answers by its deadline is a
    /// [`LedgerEventV1::ProviderLiableDefault`] — the providers' failure, never the producer's; and [`Self::provider_lapse`] voids it.
    pub provider_liable: BTreeSet<Digest>,
    budget: BlockBudgetV1,
}

/// **The class binding a single-program registration commits to** (what [`KernelRouteObjectV1::RegisterClass`] and
/// [`KernelRouteObjectV1::RegisterClassV2`] bind): the kernel descriptor, the plan, the program, the artifact's commitments and the
/// plan's context bound.
pub fn single_class_binding_v1(
    descriptor_digest: Digest,
    program_bytes: &[u8],
    plan: &VerificationPlanV1,
    pc: &ParamCommitmentsV1,
) -> ModelKernelBindingV1 {
    ModelKernelBindingV1 {
        descriptor_digest,
        plan_root: plan.root(),
        program_root: program_root_v1(program_bytes),
        artifact_root: pc.root(),
        tokenizer_or_input_schema_root: [0; 64],
        task_output_schema: [0; 64],
        context_and_state_policy: ContextPolicyV1 { max_positions: plan.max_positions },
    }
}

/// **The id a single-program registration will have under `mode`** — what a consumer admits for OPV before the class registers
/// ([`KernelLedgerV1::admit_optimistic_class`]).
pub fn single_class_id_v1(
    descriptor_digest: Digest,
    program_bytes: &[u8],
    plan: &VerificationPlanV1,
    pc: &ParamCommitmentsV1,
    mode: VerificationModeV1,
) -> Digest {
    class_id_for_mode_v1(&single_class_binding_v1(descriptor_digest, program_bytes, plan, pc).class_binding_id(), mode)
}

fn settle(out: &mut Vec<LedgerEventV1>, bond: Digest, amount: u64, kind: SettlementKindV1, claim: Option<Digest>) {
    if amount > 0 {
        out.push(LedgerEventV1::Settlement(SettlementInstructionV1 { bond, amount, kind, claim }));
    }
}

impl KernelLedgerV1 {
    pub fn genesis(policy: LedgerPolicyV1, schedule: KernelScheduleV1, known: Vec<KernelDescriptorV1>) -> Result<Self, String> {
        policy.validate()?;
        Ok(Self {
            policy,
            schedule,
            known,
            daa: 0,
            bonds: BTreeMap::new(),
            classes: BTreeMap::new(),
            pipeline_classes: BTreeMap::new(),
            jobs: BTreeMap::new(),
            pipeline_jobs: BTreeMap::new(),
            claims: BTreeMap::new(),
            demands: BTreeMap::new(),
            served: BTreeMap::new(),
            attested_artifacts: BTreeSet::new(),
            job_claims: BTreeMap::new(),
            seals: BTreeMap::new(),
            burned: 0,
            opv: OpvStateV1::default(),
            provider_liable: BTreeSet::new(),
            budget: BlockBudgetV1::default(),
        })
    }

    /// **A restart, an IBD, or the new branch of a reorg**: the state is the fold of its blocks.
    pub fn replay(genesis: &Self, blocks: &[LedgerBlockV1]) -> Self {
        let mut s = genesis.clone();
        for b in blocks {
            s.apply_block(b);
        }
        s
    }

    /// What the current block has spent of its budget.
    pub fn budget_used(&self) -> BlockBudgetV1 {
        self.budget
    }

    /// **Resume a block's budget** after [`Self::begin_block`]: a consumer that rebuilds the ledger for every object of one block (rows
    /// in, rows out) hands back what the earlier objects of that same block already spent, so the budget bounds the BLOCK and not each
    /// object. Meaningful only straight after `begin_block` of the same block.
    pub fn restore_budget(&mut self, used: BlockBudgetV1) {
        self.budget = used;
    }

    // ── consumer-derived inputs ──────────────────────────────────────────────────────────────────────────────────────────

    /// **The consumer's real locked collateral of `bond`** (absolute, not a delta). Reservations and an exit request stay.
    pub fn sync_bond(&mut self, bond: Digest, collateral: u64) {
        self.bonds.entry(bond).or_insert(BondRowV1 { collateral: 0, reserved: 0, exit_requested: None }).collateral = collateral;
    }

    /// **The consumer attests an artifact public** (its tensors obtainable, validated against the program by the consumer's
    /// registry). A class registers only over attested artifacts.
    pub fn attest_artifact(&mut self, artifact_root: Digest) {
        self.attested_artifacts.insert(artifact_root);
    }

    /// **The Panel's tally, derived by the consumer**: `covered` starts the claim's pass and challenge window. `false` changes
    /// nothing (silence and partial coverage never pass).
    pub fn apply_panel_tally(&mut self, claim: &Digest, covered: bool) -> Result<Vec<LedgerEventV1>, KernelRefusalV1> {
        let daa = self.daa;
        let Some(job) = self.claims.get(claim).map(|r| r.job_id) else {
            return Err(KernelRefusalV1::rule("PanelTally", "no such claim"));
        };
        // RFC-0015: an OptimisticPublicVerification claim has no Panel. Whatever the tally says — covered, or silence — it is
        // refused, so nothing can fabricate a coverage for it and nothing reads a missing tally as anything.
        if self.opv.claims.contains_key(claim) {
            return Err(KernelRefusalV1::rule(
                "PanelTally",
                "an OptimisticPublicVerification claim has no Panel tally (RFC-0015 §4.1)",
            ));
        }
        if !covered {
            return Ok(Vec::new());
        }
        // **One claim per job, decided by the first pass**: a claim holds its job from the Panel's coverage, not from its commit, so
        // an unbacked claim nobody covers never locks a job against the honest producer (C4 F-C4-09). Once another claim holds
        // the job, a second claim's coverage is refused: it is never paid, and it times out with its collateral returned.
        if self.job_claims.get(&job).is_some_and(|h| h != claim && self.claims.get(h).is_some_and(ClaimRowV1::holds_job)) {
            return Err(KernelRefusalV1::rule("PanelTally", "another claim already holds the job (one claim per job)"));
        }
        let row = self.claims.get_mut(claim).expect("checked");
        let _ = row.life.apply(ClaimEventV1::Tally { daa, state: TallyStateV1::Covered });
        if row.holds_job() {
            self.job_claims.insert(job, *claim);
        }
        Ok(Vec::new())
    }

    // ── the per-object API ───────────────────────────────────────────────────────────────────────────────────────────────

    /// **Start a block at `daa`** (resets the adjudication budget). A block older than the ledger's clock is refused.
    pub fn begin_block(&mut self, daa: u64) -> Result<(), KernelRefusalV1> {
        if daa < self.daa {
            return Err(KernelRefusalV1::new(
                "Block",
                RefusalKindV1::Stale,
                format!("block {daa} is older than the ledger's {}", self.daa),
            ));
        }
        self.daa = daa;
        self.budget = BlockBudgetV1::default();
        Ok(())
    }

    /// **Apply one signed object**, parsed and signature-verified by the consumer. Transactional: a refusal leaves the state
    /// (everything [`Self::root`] covers) byte-identical; an over-budget object is dropped, never fatal.
    pub fn apply_object(&mut self, obj: &KernelRouteObjectV1, auth: &AuthV1) -> Result<Vec<LedgerEventV1>, KernelRefusalV1> {
        let name = obj.name();
        let len = obj.encoded_len();
        if len > obj.max_encoded_bytes() {
            return Err(KernelRefusalV1::new(
                name,
                RefusalKindV1::Oversized,
                format!("{len} bytes past the {}-byte ceiling", obj.max_encoded_bytes()),
            ));
        }
        self.authorize(obj, auth)?;
        let mut out = Vec::new();
        match obj {
            KernelRouteObjectV1::RegisterClass { descriptor, program_bytes, plan, param_commitments } => {
                let class =
                    self.register_class(VerificationModeV1::PanelLicensed, descriptor, program_bytes, plan, param_commitments)?;
                out.push(LedgerEventV1::ClassRegistered { class });
            }
            KernelRouteObjectV1::RegisterClassV2 { mode, descriptor, program_bytes, plan, param_commitments } => {
                if *mode == VerificationModeV1::PanelLicensed {
                    return Err(KernelRefusalV1::rule(
                        name,
                        "a Panel-licensed class registers by RegisterClass (tag 1), the one path of that mode",
                    ));
                }
                let class = self.register_class(*mode, descriptor, program_bytes, plan, param_commitments)?;
                out.push(LedgerEventV1::ClassRegistered { class });
            }
            KernelRouteObjectV1::RegisterPipelineClass {
                descriptor,
                pipeline_bytes,
                program_bytes,
                plan,
                param_commitments,
                decode,
            } => {
                let class = self.register_pipeline_class(
                    VerificationModeV1::PanelLicensed,
                    descriptor,
                    pipeline_bytes,
                    program_bytes,
                    plan,
                    param_commitments,
                    *decode,
                )?;
                out.push(LedgerEventV1::ClassRegistered { class });
            }
            KernelRouteObjectV1::RegisterPipelineClassV2 {
                mode,
                descriptor,
                pipeline_bytes,
                program_bytes,
                plan,
                param_commitments,
                decode,
            } => {
                if *mode == VerificationModeV1::PanelLicensed {
                    return Err(KernelRefusalV1::rule(
                        name,
                        "a Panel-licensed class registers by RegisterPipelineClass (tag 2), the one path of that mode",
                    ));
                }
                let class =
                    self.register_pipeline_class(*mode, descriptor, pipeline_bytes, program_bytes, plan, param_commitments, *decode)?;
                out.push(LedgerEventV1::ClassRegistered { class });
            }
            KernelRouteObjectV1::PostJob { job } => {
                let class = self.classes.get(&job.class_binding_id).ok_or_else(|| KernelRefusalV1::rule(name, "no such class"))?;
                job.well_formed(class.program.token_bound, class.plan.max_positions)
                    .map_err(|why| KernelRefusalV1::rule(name, why))?;
                let id = job.id();
                if self.jobs.contains_key(&id) {
                    return Err(KernelRefusalV1::rule(name, "the job is already posted"));
                }
                self.jobs.insert(id, job.clone());
                out.push(LedgerEventV1::JobPosted { job: id });
            }
            KernelRouteObjectV1::PostPipelineJob { job } => {
                let id = job.id();
                if self.pipeline_jobs.contains_key(&id) {
                    return Err(KernelRefusalV1::rule(name, "the job is already posted"));
                }
                self.charge(name, 0)?;
                self.post_pipeline_job(job).map_err(|why| KernelRefusalV1::rule(name, why))?;
                self.pipeline_jobs.insert(id, job.clone());
                out.push(LedgerEventV1::JobPosted { job: id });
            }
            KernelRouteObjectV1::CommitClaim { claim, evidence, commitments } => {
                self.commit_claim(claim, evidence, commitments, &mut out)?;
                out.push(LedgerEventV1::ClaimCommitted { claim: claim.id() });
            }
            KernelRouteObjectV1::CommitPipelineClaim { claim, evidence, stages } => {
                self.commit_pipeline_claim(claim, evidence, stages, &mut out)?;
                out.push(LedgerEventV1::ClaimCommitted { claim: claim.id() });
            }
            KernelRouteObjectV1::FileProof { accuser, claim, proof } => self.file_proof(accuser, claim, proof, &mut out)?,
            KernelRouteObjectV1::FileDemand { demander, claim, stage, position } => {
                self.file_demand(demander, claim, *stage, *position, &mut out)?
            }
            KernelRouteObjectV1::Respond { claim, stage, position, bytes } => {
                self.respond(claim, *stage, *position, bytes, &mut out)?
            }
            KernelRouteObjectV1::RequestExit { bond } => match self.bonds.get_mut(bond) {
                Some(b) if b.exit_requested.is_none() => {
                    b.exit_requested = Some(self.daa);
                    out.push(LedgerEventV1::ExitRequested { bond: *bond });
                }
                _ => return Err(KernelRefusalV1::rule(name, "no such bond, or already exiting")),
            },
            KernelRouteObjectV1::SealClaim { producer, job, seal } => {
                if !self.jobs.contains_key(job) && !self.pipeline_jobs.contains_key(job) {
                    return Err(KernelRefusalV1::rule(name, "no such job"));
                }
                match self.bonds.get(producer) {
                    None => return Err(KernelRefusalV1::rule(name, "the producer bond is not registered")),
                    Some(b) if b.exit_requested.is_some() => return Err(KernelRefusalV1::rule(name, "the producer bond is exiting")),
                    Some(_) => {}
                }
                if self.job_claims.get(job).and_then(|c| self.claims.get(c)).is_some_and(ClaimRowV1::holds_job) {
                    return Err(KernelRefusalV1::rule(name, "another claim already holds the job"));
                }
                // A producer may re-seal (another output): the latest seal replaces the earlier and its clock restarts.
                self.seals.insert((*job, *producer), SealRowV1 { seal: *seal, daa: self.daa });
                out.push(LedgerEventV1::ClaimSealed { job: *job, producer: *producer });
            }
            KernelRouteObjectV1::Withdraw { bond } => {
                let (daa, delay) = (self.daa, self.policy.exit_delay_daa);
                match self.bonds.get(bond) {
                    Some(b) if b.exit_requested.is_some_and(|at| daa >= at + delay) && b.reserved == 0 => {
                        let amount = b.collateral;
                        self.bonds.remove(bond);
                        out.push(LedgerEventV1::Withdrawn { bond: *bond, amount });
                        settle(&mut out, *bond, amount, SettlementKindV1::Withdraw, None);
                    }
                    Some(_) => return Err(KernelRefusalV1::rule(name, "not exiting, inside the delay, or collateral still reserved")),
                    None => return Err(KernelRefusalV1::rule(name, "no such bond")),
                }
            }
        }
        Ok(out)
    }

    /// Decode and apply one encoded object (strict: see [`KernelRouteObjectV1::decode`]).
    pub fn apply_encoded(&mut self, bytes: &[u8], auth: &AuthV1) -> Result<Vec<LedgerEventV1>, KernelRefusalV1> {
        let object = KernelRouteObjectV1::decode(bytes)?;
        self.apply_object(&object, auth)
    }

    /// **The block's closing step**: demand deadlines (the producer's availability default), windows, Final, liability release.
    pub fn tick(&mut self) -> Vec<LedgerEventV1> {
        let mut out = Vec::new();
        self.tick_into(&mut out);
        out
    }

    /// A whole block as one call (a thin wrapper over the per-object API): `begin_block`, each input, `tick`. Refusals become
    /// `Refused` receipts; a stale block is not applied.
    pub fn apply_block(&mut self, block: &LedgerBlockV1) -> Vec<LedgerEventV1> {
        let mut out = Vec::new();
        if self.begin_block(block.daa).is_err() {
            return out; // blocks are in DAA order; a stale block is not applied
        }
        for tx in &block.txs {
            let r = match tx {
                LedgerTxV1::SyncBond { bond, collateral } => {
                    self.sync_bond(*bond, *collateral);
                    Ok(Vec::new())
                }
                LedgerTxV1::AttestArtifact { artifact_root } => {
                    self.attest_artifact(*artifact_root);
                    Ok(Vec::new())
                }
                LedgerTxV1::PanelCovered { claim } => self.apply_panel_tally(claim, true),
                LedgerTxV1::AdmitOptimisticClass { class } => self.admit_optimistic_class(*class).map(|()| Vec::new()),
                LedgerTxV1::Object { auth, object } => self.apply_object(object, auth),
            };
            match r {
                Ok(ev) => out.extend(ev),
                Err(r) => out.push(LedgerEventV1::Refused { tx: r.object, why: r.why }),
            }
        }
        out.extend(self.tick());
        out
    }

    /// Who may sign what. Objects naming an actor must be signed by it; the rest need a bond the consumer synced.
    fn authorize(&self, obj: &KernelRouteObjectV1, auth: &AuthV1) -> Result<(), KernelRefusalV1> {
        use KernelRouteObjectV1 as O;
        let name = obj.name();
        let signer = auth.signer_bond;
        let named = match obj {
            O::CommitClaim { claim, .. } => Some((claim.producer_bond, "producer")),
            O::CommitPipelineClaim { claim, .. } => Some((claim.producer_bond, "producer")),
            O::FileProof { accuser, .. } => Some((*accuser, "accuser")),
            O::FileDemand { demander, .. } => Some((*demander, "demander")),
            O::RequestExit { bond } | O::Withdraw { bond } => Some((*bond, "bond")),
            O::SealClaim { producer, .. } => Some((*producer, "producer")),
            _ => None,
        };
        if let Some((actor, role)) = named {
            // Whether the actor is a registered, free-enough bond is the rule's own check (its message names the role).
            return if actor == signer {
                Ok(())
            } else {
                Err(KernelRefusalV1::new(name, RefusalKindV1::Unauthorized, format!("the signer is not the {role} the object names")))
            };
        }
        match self.bonds.get(&signer) {
            None => Err(KernelRefusalV1::new(name, RefusalKindV1::Unauthorized, "the signer is not a registered bond")),
            // A class or job is state that outlives the signer's exit: an exiting bond adds none. (A response is not state to
            // keep, and a producer must always be able to answer.)
            Some(b) if b.exit_requested.is_some() && !matches!(obj, O::Respond { .. }) => {
                Err(KernelRefusalV1::new(name, RefusalKindV1::Unauthorized, "the signer is exiting"))
            }
            Some(_) => Ok(()),
        }
    }

    /// Charge one adjudication (and `court_work`) to the block, or refuse the object as over budget. Nothing is charged on a refusal.
    fn charge(&mut self, name: &'static str, court_work: u64) -> Result<(), KernelRefusalV1> {
        let b = self.budget;
        if b.adjudications >= self.policy.max_adjudications_per_block
            || b.court_work.saturating_add(court_work) > self.policy.max_court_work_per_block
        {
            return Err(KernelRefusalV1::new(
                name,
                RefusalKindV1::OverBudget,
                format!("the block's adjudication budget is spent ({} runs, {} court work)", b.adjudications, b.court_work),
            ));
        }
        self.budget = BlockBudgetV1 { adjudications: b.adjudications + 1, court_work: b.court_work.saturating_add(court_work) };
        Ok(())
    }

    fn known_descriptor(&self, descriptor: &Digest) -> Result<KernelDescriptorV1, String> {
        self.known.iter().find(|d| d.digest() == *descriptor).cloned().ok_or_else(|| "a kernel this binary does not implement".into())
    }

    /// **RFC-0015 §4.2 at registration**: an OptimisticPublicVerification class registers only where this ledger has an OPV policy and
    /// the `palw_panel_free_v1` fence is reached. (Checked before any decoding; a refusal leaves the state byte-identical.)
    fn opv_register_gate(&self, name: &'static str) -> Result<crate::opv::OpvPolicyV1, KernelRefusalV1> {
        let Some(p) = self.opv.policy else {
            return Err(KernelRefusalV1::rule(
                name,
                "this ledger has no OPV policy: no OptimisticPublicVerification class can register",
            ));
        };
        if !p.allowed_at(self.daa) {
            return Err(KernelRefusalV1::rule(name, "the palw_panel_free_v1 fence is not reached"));
        }
        Ok(p)
    }

    /// **RFC-0015 §4.2 / §6.3 at registration, for what only the class knows**: its worst filing, response and commitments must fit
    /// the carriers (or a demand would always default and a proof could never be carried), and saturating the court budget for the
    /// claim's whole exposure must cost more than the claim's maximum gain (a producer must not be able to buy the censorship of
    /// its own prosecution).
    fn opv_class_economics(&self, p: &crate::opv::OpvPolicyV1, bounds: &ProsecutionBoundsV1) -> Result<(), String> {
        carrier_fit_v1(bounds, p.carrier.filing_cap as usize, p.carrier.response_cap as usize, p.carrier.commit_cap as usize)
            .map_err(|why| format!("not carriable: {why}"))?;
        let (cost, gain) = (p.censorship_cost(&self.policy, bounds.max_court_work), p.max_gain_per_claim(&self.policy));
        if cost <= gain {
            return Err(format!(
                "saturating the court budget for the claim's whole exposure costs {cost}, not more than the claim's maximum gain {gain}: \
                 its prosecution could be censored for less than it pays"
            ));
        }
        Ok(())
    }

    /// RFC-0015 §4.1: the network's policy admits the class; a registrant never chooses the lighter mode for its own program.
    fn opv_class_admitted(&self, name: &'static str, class: &Digest) -> Result<(), KernelRefusalV1> {
        if self.opv.admitted.contains(class) {
            Ok(())
        } else {
            Err(KernelRefusalV1::rule(name, "the network policy has not admitted this class for OptimisticPublicVerification"))
        }
    }

    fn register_class(
        &mut self,
        mode: VerificationModeV1,
        descriptor: &Digest,
        program_bytes: &[u8],
        plan: &VerificationPlanV1,
        pc: &ParamCommitmentsV1,
    ) -> Result<Digest, KernelRefusalV1> {
        let name: &'static str = if mode.is_optimistic() { "RegisterClassV2" } else { "RegisterClass" };
        let rule = |why: String| KernelRefusalV1::rule(name, why);
        let d = self.known_descriptor(descriptor).map_err(rule)?;
        let root = program_root_v1(program_bytes);
        let binding = single_class_binding_v1(d.digest(), program_bytes, plan, pc);
        // The mode is part of the class identity: the same program under another mode is another class (RFC-0015 §4.1).
        let class = class_id_for_mode_v1(&binding.class_binding_id(), mode);
        if self.classes.contains_key(&class) {
            // Re-registering would overwrite the row every court reads; the class id binds everything, so there is nothing to add.
            return Err(rule("the class is already registered".into()));
        }
        let opv = if mode.is_optimistic() { Some(self.opv_register_gate(name)?) } else { None };
        if mode.is_optimistic() {
            self.opv_class_admitted(name, &class)?;
        }
        if !self.attested_artifacts.contains(&binding.artifact_root) {
            return Err(rule("the artifact is not attested public by the consumer's registry".into()));
        }
        self.charge(name, 0)?;
        let program = TirProgramV1::decode_canonical(program_bytes).map_err(|e| rule(format!("program: {e}")))?;
        check_plan_v1(&self.schedule, &d, &program, root, plan, self.daa).map_err(|o| rule(o.to_string()))?;
        check_commitment_set(
            pc,
            &used_param_instances(&program, program.params.len()),
            &declared_param_instances(&program.params, program.schedule.layers.len()),
        )
        .map_err(rule)?;
        let nodes: u64 = program.occurrences().iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u64).sum();
        // The artifact is public by the consumer's attestation (checked above), not by a registrant's flag. PUBLIC_PROSECUTION_COMPLETE
        // is derived from code for every class of every mode; an OPV class has no Panel to fall back on, so there is no exception.
        let bounds = public_prosecution_complete_v1(&d, plan, nodes, &ProfileMaterialV1::kernel_route(true), &self.policy.prosecution)
            .map_err(|g| rule(format!("not publicly prosecutable: {g:?}")))?;
        if bounds.max_court_work > self.policy.max_court_work_per_block {
            return Err(rule("the class's worst court does not fit one block's court budget: nobody could prosecute it".into()));
        }
        if let Some(p) = opv {
            self.opv_class_economics(&p, &bounds).map_err(rule)?;
        }
        self.classes.insert(
            class,
            ClassRowV1 {
                descriptor: d,
                program_bytes: program_bytes.to_vec(),
                program,
                plan: plan.clone(),
                param_commitments: pc.clone(),
                network_domain: self.policy.network_domain,
                ruleset_digest: self.policy.ruleset_digest,
                bounds,
            },
        );
        if mode.is_optimistic() {
            self.opv.classes.insert(class);
        }
        Ok(class)
    }

    #[allow(clippy::too_many_arguments)]
    fn register_pipeline_class(
        &mut self,
        mode: VerificationModeV1,
        descriptor: &Digest,
        pipeline_bytes: &[u8],
        program_bytes: &[Vec<u8>],
        plan: &PipelinePlanV1,
        pcs: &[ParamCommitmentsV1],
        decode: Option<DecodeRuleV1>,
    ) -> Result<Digest, KernelRefusalV1> {
        let name: &'static str = if mode.is_optimistic() { "RegisterPipelineClassV2" } else { "RegisterPipelineClass" };
        let rule = |why: String| KernelRefusalV1::rule(name, why);
        let d = self.known_descriptor(descriptor).map_err(rule)?;
        let opv = if mode.is_optimistic() { Some(self.opv_register_gate(name)?) } else { None };
        if pcs.iter().any(|p| !self.attested_artifacts.contains(&p.root())) {
            return Err(rule("an artifact is not attested public by the consumer's registry".into()));
        }
        self.charge(name, 0)?;
        let programs = program_bytes
            .iter()
            .map(|b| TirProgramV2::decode_canonical(b).map_err(|e| rule(format!("program: {e}"))))
            .collect::<Result<Vec<_>, _>>()?;
        let pipeline = TirPipelineV1::decode_canonical(pipeline_bytes, &programs).map_err(|e| rule(format!("pipeline: {e}")))?;
        check_pipeline_plan_v1(&self.schedule, &d, &pipeline, &programs, plan, self.daa).map_err(|o| rule(o.to_string()))?;
        let bounds = public_pipeline_prosecution_complete_v1(
            &d,
            plan,
            &pipeline,
            &programs,
            &ProfileMaterialV1::kernel_route(true),
            &self.policy.prosecution,
        )
        .map_err(|g| rule(format!("not publicly prosecutable: {g:?}")))?;
        if pcs.len() != programs.len() {
            return Err(rule("one artifact per program".into()));
        }
        for (prog, pc) in programs.iter().zip(pcs) {
            check_commitment_set(
                pc,
                &used_param_instances(&stage_view_v1(prog).view, prog.params.len()),
                &declared_param_instances(&prog.params, prog.schedule.layers.len()),
            )
            .map_err(rule)?;
        }
        if stream_stage(&pipeline).is_some() != decode.is_some() {
            return Err(rule("a stream stage's output needs a decode rule; a pipeline without one has none".into()));
        }
        if bounds.max_court_work > self.policy.max_court_work_per_block {
            return Err(rule("the class's worst court does not fit one block's court budget: nobody could prosecute it".into()));
        }
        let binding = PipelineClassV1 {
            descriptor_digest: d.digest(),
            pipeline_root: pipeline_root_v1(&pipeline, &programs),
            plan_root: plan.root(),
            artifact_roots: pcs.iter().map(ParamCommitmentsV1::root).collect(),
            decode,
        };
        let class = class_id_for_mode_v1(&binding.class_binding_id(), mode);
        if self.pipeline_classes.contains_key(&class) {
            return Err(rule("the class is already registered".into()));
        }
        if mode.is_optimistic() {
            self.opv_class_admitted(name, &class)?;
        }
        if let Some(p) = opv {
            self.opv_class_economics(&p, &bounds).map_err(rule)?;
        }
        self.pipeline_classes.insert(
            class,
            PipelineClassRowV1 {
                descriptor: d,
                pipeline_bytes: pipeline_bytes.to_vec(),
                pipeline,
                program_bytes: program_bytes.to_vec(),
                programs,
                plan: plan.clone(),
                param_commitments: pcs.to_vec(),
                binding,
                network_domain: self.policy.network_domain,
                ruleset_digest: self.policy.ruleset_digest,
                bounds,
            },
        );
        if mode.is_optimistic() {
            self.opv.classes.insert(class);
        }
        Ok(class)
    }

    fn post_pipeline_job(&self, job: &PipelineJobPostV1) -> Result<(), String> {
        let class = self.pipeline_classes.get(&job.class_binding_id).ok_or("no such class")?;
        // The job's facts must run: probed with the largest generation it allows (a stream stage) or none.
        let probe: Vec<u32> = match stream_stage(&class.pipeline) {
            Some(_) if job.max_new_tokens == 0 => return Err("a text pipeline's job generates at least one id".into()),
            Some(si) => {
                let st = &class.pipeline.stages[si];
                let bound = class.programs[st.program as usize].token_bound;
                if job.facts.prompt.is_empty() || job.facts.prompt.iter().any(|t| *t >= bound) {
                    return Err("a text pipeline's prompt is non-empty and inside the token bound".into());
                }
                vec![0; job.max_new_tokens as usize]
            }
            None if job.max_new_tokens != 0 => return Err("a pipeline without a stream stage generates no ids".into()),
            None => Vec::new(),
        };
        misaka_palw_tir::pipeline::stage_job_facts(&class.pipeline, &class.programs, &job.facts.job(&probe))
            .map_err(|e| format!("the job's facts do not run: {e}"))?;
        Ok(())
    }

    /// Reserve a new claim's collateral and start its lifecycle. Every refusal is decided before the first mutation.
    fn admit(
        &mut self,
        id: Digest,
        producer: Digest,
        class: Digest,
        job: Digest,
        body: ClaimBodyV1,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), String> {
        if self.claims.contains_key(&id) {
            return Err("an exact duplicate claim".into());
        }
        // RFC-0015: an OptimisticPublicVerification claim reserves the OPV policy's per-claim reservation (not the Panel route's flat
        // collateral), subject to the producer's and the ledger's live-claim caps — all decided before the first mutation.
        let opv_row = if self.opv.classes.contains(&class) { Some(self.opv_admission(&producer)?) } else { None };
        let need = opv_row.map_or(self.policy.claim_collateral, |(need, _)| need);
        let bond = self.bonds.get_mut(&producer).ok_or("the producer bond is not registered")?;
        if bond.exit_requested.is_some() {
            return Err("the producer bond is exiting".into());
        }
        if bond.free() < need {
            return Err(format!("{} free collateral, the claim needs {need} (no double use)", bond.free()));
        }
        bond.reserved += need;
        let daa = self.daa;
        let mut life = ClaimLifecycleV1::new(LifecyclePolicyV1 {
            check_window_daa: self.policy.check_window_daa,
            challenge_window_daa: match &opv_row {
                Some((_, row)) => row.window_end_daa - daa,
                None => self.policy.challenge_window_daa,
            },
        });
        if opv_row.is_some() {
            // No Panel: the fixed public challenge window opens at inclusion, and nothing can pass the claim but the window rule.
            let _ = life.apply(ClaimEventV1::OpenChallengeWindow { daa });
        } else {
            let _ = life.apply(ClaimEventV1::BindChallenge { anchor_daa: daa });
            let _ = life.apply(ClaimEventV1::StartChecking { daa });
        }
        // The commitments are on chain from inclusion: the retention obligation for Final is met by the ledger itself.
        let _ = life.apply(ClaimEventV1::RetentionMet);
        self.claims.insert(
            id,
            ClaimRowV1 {
                producer,
                class_binding_id: class,
                job_id: job,
                body,
                committed_daa: daa,
                life,
                reserved: need,
                liability_until: None,
                convicted: false,
                rewarded: false,
            },
        );
        if let Some((_, row)) = opv_row {
            // An OPV claim holds its job from its first reveal: it is prosecutable by any bond from this block, so a junk claim
            // squatting the job is slashed (or defaults) at the squatter's cost, and a well-formed one is the job's answer.
            self.opv.claims.insert(id, row);
            self.opv.live.insert((producer, id));
            self.job_claims.insert(job, id);
        }
        settle(out, producer, need, SettlementKindV1::ReserveClaim, Some(id));
        Ok(())
    }

    /// A claim of an OPV class needs its producer's live-claim caps and free collateral to allow it: decided before the adjudication
    /// budget is spent and before any evidence is verified (a producer at its cap cannot make the ledger verify claims it must refuse).
    fn opv_claim_capacity(&self, class: &Digest, producer: &Digest) -> Result<(), String> {
        if !self.opv.classes.contains(class) {
            return Ok(());
        }
        let (need, _) = self.opv_admission(producer)?;
        match self.bonds.get(producer) {
            Some(b) if b.free() < need => Err(format!("{} free collateral, the claim needs {need} (no double use)", b.free())),
            _ => Ok(()),
        }
    }

    /// A claim of an OPV class commits only while the fence is reached (a cheap check; nothing is charged).
    fn opv_claim_gate(&self, class: &Digest) -> Result<(), String> {
        if self.opv.classes.contains(class) && !self.optimistic_allowed() {
            return Err("the palw_panel_free_v1 fence is not reached".into());
        }
        Ok(())
    }

    /// **May this claim be revealed now?** No other claim holds its job, its producer bond is ready, and the producer's seal of
    /// exactly this claim is at least `claim_seal_delay_daa` old.
    fn reveal_ready(&self, job: &Digest, producer: &Digest, id: &Digest) -> Result<(), String> {
        if self.job_claims.get(job).and_then(|c| self.claims.get(c)).is_some_and(ClaimRowV1::holds_job) {
            return Err("another claim already holds the job (one claim per job)".into());
        }
        match self.bonds.get(producer) {
            None => return Err("the producer bond is not registered".into()),
            Some(b) if b.exit_requested.is_some() => return Err("the producer bond is exiting".into()),
            Some(_) => {}
        }
        match self.seals.get(&(*job, *producer)) {
            Some(row) if row.seal == claim_seal_v1(id) && row.daa.saturating_add(self.policy.claim_seal_delay_daa) <= self.daa => {
                Ok(())
            }
            _ => Err(format!(
                "no seal of this claim by its producer at least {} DAA old (seal, then reveal)",
                self.policy.claim_seal_delay_daa
            )),
        }
    }

    fn commit_claim(
        &mut self,
        claim: &KernelClaimV1,
        evidence: &VerificationEvidenceV1,
        commitments: &[Vec<Vec<Digest>>],
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "CommitClaim";
        let rule = |why: String| KernelRefusalV1::rule(NAME, why);
        let job = self.jobs.get(&claim.job_id).ok_or_else(|| rule("no such job".into()))?;
        let class = self.classes.get(&job.class_binding_id).ok_or_else(|| rule("no such class".into()))?;
        self.opv_claim_gate(&job.class_binding_id).map_err(rule)?;
        if let Some(f) = binding_fault_v1(job, claim, evidence, class.program.token_bound) {
            return Err(rule(format!("binding fault {f:?}")));
        }
        if evidence.header != class.header(job.class_binding_id) {
            return Err(rule(format!("binding fault {:?}", BindingFaultV1::WrongClass)));
        }
        // The trace commitments are carried with the claim (they bound every later opening): they must be the evidence's.
        if EvidenceV1::new(commitments.to_vec()).root() != evidence.trace_root {
            return Err(rule("the carried trace commitments are not the evidence's trace root".into()));
        }
        let id = claim.id();
        if self.claims.contains_key(&id) {
            return Err(rule("an exact duplicate claim".into()));
        }
        // Cheap objective checks first: an unsealed or unready reveal never spends the block's adjudication budget (C4 O-C4-14).
        self.reveal_ready(&claim.job_id, &claim.producer_bond, &id).map_err(rule)?;
        self.opv_claim_capacity(&job.class_binding_id, &claim.producer_bond).map_err(rule)?;
        self.charge(NAME, 0)?;
        let job = self.jobs.get(&claim.job_id).expect("checked");
        let class = self.classes.get(&job.class_binding_id).expect("checked");
        // Everything a court checks before any relation, checked now: a claim is never committed in a shape a court could only
        // call malformed (fabricated segment boundaries, another suite, a wrong output or state root, misshapen commitments).
        let record = PublicClaimRecordV1 {
            claim_id: id,
            program_bytes: class.program_bytes.clone(),
            plan: class.plan.clone(),
            evidence: evidence.clone(),
            trace_commitments: commitments.to_vec(),
            param_commitments: class.param_commitments.by_instance.iter().map(|((j, l), d)| (*j, *l, *d)).collect(),
            tokens: claim.stream(job),
            // The legacy block-hash beacon is gone from the route: outsiders check with their own salt and the claim's challenge
            // subject comes from `misaka-palw-challenge`.
            beacon: [0; 64],
        };
        FreshVerifierV1::from_public_bytes(&record.to_bytes(), &self.known, class.header(job.class_binding_id))
            .and_then(|v| v.structure())
            .map_err(|why| rule(format!("malformed evidence: {why}")))?;
        let body = ClaimBodyV1::Program { claim: claim.clone(), evidence: evidence.clone(), commitments: commitments.to_vec() };
        let class_id = job.class_binding_id;
        self.admit(id, claim.producer_bond, class_id, claim.job_id, body, out).map_err(rule)?;
        self.seals.remove(&(claim.job_id, claim.producer_bond));
        Ok(())
    }

    fn commit_pipeline_claim(
        &mut self,
        claim: &PipelineClaimV1,
        evidence: &PipelineEvidenceV1,
        stages: &[StageCommitmentsV1],
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "CommitPipelineClaim";
        let rule = |why: String| KernelRefusalV1::rule(NAME, why);
        let job = self.pipeline_jobs.get(&claim.job_id).ok_or_else(|| rule("no such job".into()))?;
        let class = self.pipeline_classes.get(&job.class_binding_id).ok_or_else(|| rule("no such class".into()))?;
        self.opv_claim_gate(&job.class_binding_id).map_err(rule)?;
        use BindingFaultV1 as F;
        let fault = |f: F| Err(rule(format!("binding fault {f:?}")));
        if claim.evidence_root != evidence.root() {
            return fault(F::WrongEvidence);
        }
        let h = class.header(job.class_binding_id);
        if (evidence.network_domain, evidence.ruleset_digest, evidence.class_binding_id)
            != (h.network_domain, h.ruleset_digest, h.class_binding_id)
            || evidence.pipeline_root != class.binding.pipeline_root
            || evidence.plan_root != class.binding.plan_root
        {
            return fault(F::WrongClass);
        }
        match stream_stage(&class.pipeline) {
            Some(si) => {
                if claim.generated.is_empty() || claim.generated.len() as u64 > job.max_new_tokens as u64 {
                    return fault(F::WrongGenerationLength);
                }
                let bound = class.programs[class.pipeline.stages[si].program as usize].token_bound;
                if claim.generated.iter().any(|t| *t >= bound) {
                    return fault(F::TokenOutOfRange);
                }
            }
            None if !claim.generated.is_empty() => return fault(F::WrongGenerationLength),
            None => {}
        }
        // A trace of another job, or `R` drawn from another seed, is a borrowed trace.
        if evidence.job_root != pipeline_job_root_v1(&job.facts.job(&claim.generated))
            || evidence.random_binding != job.facts.random().binding()
        {
            return fault(F::WrongInput);
        }
        if claim.output_root != evidence.output_root {
            return Err(rule("binding fault WrongOutput".into()));
        }
        if stages.len() != evidence.stages.len()
            || stages.iter().zip(&evidence.stages).any(|(s, e)| s.evidence().root() != e.trace_root)
        {
            return Err(rule("the carried trace commitments are not the evidence's trace roots".into()));
        }
        let id = claim.id();
        if self.claims.contains_key(&id) {
            return Err(rule("an exact duplicate claim".into()));
        }
        // Cheap objective checks first: an unsealed or unready reveal never spends the block's adjudication budget (C4 O-C4-14).
        self.reveal_ready(&claim.job_id, &claim.producer_bond, &id).map_err(rule)?;
        self.opv_claim_capacity(&job.class_binding_id, &claim.producer_bond).map_err(rule)?;
        self.charge(NAME, 0)?;
        let job = self.pipeline_jobs.get(&claim.job_id).expect("checked");
        let class = self.pipeline_classes.get(&job.class_binding_id).expect("checked");
        let h = class.header(job.class_binding_id);
        let record = PipelinePublicRecordV1 {
            claim_id: id,
            pipeline_bytes: class.pipeline_bytes.clone(),
            program_bytes: class.program_bytes.clone(),
            plan: class.plan.clone(),
            evidence: evidence.clone(),
            stages: stages.to_vec(),
            param_commitments: class
                .param_commitments
                .iter()
                .map(|p| p.by_instance.iter().map(|((j, l), d)| (*j, *l, *d)).collect())
                .collect(),
            facts: job.facts.clone(),
            generated: claim.generated.clone(),
            beacon: [0; 64],
        };
        FreshPipelineVerifierV1::from_public_bytes_in_mode(
            &record.to_bytes(),
            &self.known,
            h,
            &class.binding,
            self.mode_of_class(&job.class_binding_id),
        )
        .and_then(|v| v.structure())
        .map_err(|why| rule(format!("malformed evidence: {why}")))?;
        let body = ClaimBodyV1::Pipeline { claim: claim.clone(), evidence: evidence.clone(), stages: stages.to_vec() };
        let class_id = job.class_binding_id;
        self.admit(id, claim.producer_bond, class_id, claim.job_id, body, out).map_err(rule)?;
        self.seals.remove(&(claim.job_id, claim.producer_bond));
        Ok(())
    }

    /// The public record of a single-program claim, assembled from ledger state only (what a court and a fresh verifier read).
    pub fn public_record(&self, claim: &Digest) -> Option<(PublicClaimRecordV1, EvidenceHeaderV1)> {
        let row = self.claims.get(claim)?;
        let ClaimBodyV1::Program { claim: c, evidence, commitments } = &row.body else { return None };
        let class = self.classes.get(&row.class_binding_id)?;
        let job = self.jobs.get(&row.job_id)?;
        let record = PublicClaimRecordV1 {
            claim_id: *claim,
            program_bytes: class.program_bytes.clone(),
            plan: class.plan.clone(),
            evidence: evidence.clone(),
            trace_commitments: commitments.clone(),
            param_commitments: class.param_commitments.by_instance.iter().map(|((j, l), d)| (*j, *l, *d)).collect(),
            tokens: c.stream(job),
            beacon: [0; 64],
        };
        Some((record, class.header(row.class_binding_id)))
    }

    /// The public record of a pipeline claim, with its header and class binding.
    pub fn pipeline_public_record(&self, claim: &Digest) -> Option<(PipelinePublicRecordV1, PipelineHeaderV1, PipelineClassV1)> {
        let row = self.claims.get(claim)?;
        let ClaimBodyV1::Pipeline { claim: c, evidence, stages } = &row.body else { return None };
        let class = self.pipeline_classes.get(&row.class_binding_id)?;
        let job = self.pipeline_jobs.get(&row.job_id)?;
        let record = PipelinePublicRecordV1 {
            claim_id: *claim,
            pipeline_bytes: class.pipeline_bytes.clone(),
            program_bytes: class.program_bytes.clone(),
            plan: class.plan.clone(),
            evidence: evidence.clone(),
            stages: stages.clone(),
            param_commitments: class
                .param_commitments
                .iter()
                .map(|p| p.by_instance.iter().map(|((j, l), d)| (*j, *l, *d)).collect())
                .collect(),
            facts: job.facts.clone(),
            generated: c.generated.clone(),
            beacon: [0; 64],
        };
        Some((record, class.header(row.class_binding_id), class.binding.clone()))
    }

    /// `derived[occurrence][node]` of a claim's stage: the values rebuilt from committed rows, never served or demanded.
    pub fn derived_mask(&self, row: &ClaimRowV1, stage: u8) -> Vec<Vec<bool>> {
        match &row.body {
            ClaimBodyV1::Program { .. } => {
                self.classes.get(&row.class_binding_id).map(|c| derived_mask_v1(&c.program)).unwrap_or_default()
            }
            ClaimBodyV1::Pipeline { .. } => self
                .pipeline_classes
                .get(&row.class_binding_id)
                .and_then(|c| {
                    let st = c.pipeline.stages.get(stage as usize)?;
                    Some(derived_mask_v1(&stage_view_v1(c.programs.get(st.program as usize)?).view))
                })
                .unwrap_or_default(),
        }
    }

    fn bounds_of(&self, class: &Digest) -> Option<ProsecutionBoundsV1> {
        self.classes.get(class).map(|c| c.bounds).or_else(|| self.pipeline_classes.get(class).map(|c| c.bounds))
    }

    /// **The court**, over ledger state only.
    fn adjudicate(&self, claim: &Digest, proof: &ProsecutionV1) -> Result<(), String> {
        let row = self.claims.get(claim).ok_or("no such claim")?;
        match (&row.body, proof) {
            (ClaimBodyV1::Program { .. }, ProsecutionV1::Kernel(bytes)) => {
                let (record, header) = self.public_record(claim).ok_or("no public record")?;
                let court = FreshVerifierV1::from_public_bytes(&record.to_bytes(), &self.known, header)?;
                court.try_proof(bytes).map(|_| ()).map_err(|d| format!("{d:?}"))
            }
            (ClaimBodyV1::Program { claim: c, commitments, .. }, ProsecutionV1::Decode(fault)) => {
                let class = self.classes.get(&row.class_binding_id).ok_or("no such class")?;
                let job = self.jobs.get(&row.job_id).ok_or("no such job")?;
                let trace = EvidenceV1::new(commitments.clone());
                verify_decode_fault_v1(job, c, &trace, class.logits_at(), fault).map(|_| ()).map_err(|d| format!("{d:?}"))
            }
            (ClaimBodyV1::Pipeline { .. }, ProsecutionV1::Pipeline(bytes)) => {
                let (record, header, binding) = self.pipeline_public_record(claim).ok_or("no public record")?;
                let mode = self.mode_of_class(&header.class_binding_id);
                FreshPipelineVerifierV1::from_public_bytes_in_mode(&record.to_bytes(), &self.known, header, &binding, mode)?
                    .try_proof(bytes)
            }
            _ => Err("a filing for another kind of claim".into()),
        }
    }

    fn file_proof(
        &mut self,
        accuser: &Digest,
        claim: &Digest,
        proof: &ProsecutionV1,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "FileProof";
        let rule = |why: &str| KernelRefusalV1::rule(NAME, why);
        let fee = self.policy.dismissed_proof_fee;
        match self.bonds.get(accuser) {
            None => return Err(rule("the accuser is not a registered bond")),
            Some(b) if b.free() < fee => return Err(rule("the accuser's free collateral does not cover the filing fee")),
            Some(_) => {}
        }
        let Some(row) = self.claims.get(claim) else { return Err(rule("no such claim")) };
        if row.convicted {
            // A second filing of the same fault, in this block or after a replay: one conviction, no second slash, no fee.
            out.push(LedgerEventV1::Duplicate { claim: *claim });
            return Ok(());
        }
        let is_final = matches!(row.life.state, ClaimStateV1::Final { .. });
        let bounds = self.bounds_of(&row.class_binding_id);
        let cheap = if is_final && row.liability_until.is_none_or(|until| self.daa > until) {
            Some("past the liability horizon".to_string())
        } else if row.reserved == 0 && !is_final {
            // Before Final nothing reserved means the claim already ended (timed out, unavailable). After Final inside the horizon a
            // valid proof still convicts — a post-Final default may have forfeited the reservation, but it never erases the liability.
            Some("nothing is reserved any more".to_string())
        } else {
            bounds.and_then(|b| oversized(proof, &b))
        };
        let verdict = match cheap {
            Some(why) => Err(why),
            None => {
                // The court is about to run: it spends the block's budget (a refusal here costs the accuser nothing).
                self.charge(NAME, bounds.map(|b| b.max_court_work).unwrap_or(0))?;
                self.adjudicate(claim, proof)
            }
        };
        match verdict {
            Err(why) => {
                let b = self.bonds.get_mut(accuser).expect("checked");
                b.collateral -= fee;
                self.burned += fee;
                out.push(LedgerEventV1::ProofDismissed { claim: *claim, accuser: *accuser, why, fee });
                settle(out, *accuser, fee, SettlementKindV1::SlashFiling, Some(*claim));
                settle(out, *accuser, fee, SettlementKindV1::Burn, Some(*claim));
            }
            Ok(()) => self.convict(claim, accuser, is_final, out),
        }
        Ok(())
    }

    fn convict(&mut self, claim: &Digest, accuser: &Digest, post_final: bool, out: &mut Vec<LedgerEventV1>) {
        let daa = self.daa;
        let producer_collateral = self.claims.get(claim).and_then(|r| self.bonds.get(&r.producer)).map_or(0, |b| b.collateral);
        let row = self.claims.get_mut(claim).expect("checked");
        // Never instruct more than the bond holds (another subsystem may have slashed it since the claim reserved).
        let reserved = row.reserved;
        let slashed = reserved.min(producer_collateral);
        row.reserved = 0;
        row.convicted = true;
        if !post_final {
            // Through the lifecycle: a filed dispute, then the court's conviction (Final can never follow).
            if !matches!(row.life.state, ClaimStateV1::Disputed { .. }) {
                let _ = row.life.apply(ClaimEventV1::DisputeFiled { daa });
            }
            let _ = row.life.apply(ClaimEventV1::CourtVerdict { daa, convicted: true });
            if !matches!(row.life.state, ClaimStateV1::Convicted { .. }) {
                row.life.state = ClaimStateV1::Convicted { daa };
            }
        }
        let producer = row.producer;
        if let Some(b) = self.bonds.get_mut(&producer) {
            b.reserved = b.reserved.saturating_sub(reserved);
            b.collateral = b.collateral.saturating_sub(slashed);
        }
        let reward = slashed * self.policy.accuser_reward_permille as u64 / 1000;
        self.burned += slashed - reward;
        out.push(LedgerEventV1::Convicted { claim: *claim, accuser: *accuser, slashed, accuser_reward: reward, post_final });
        settle(out, producer, slashed, SettlementKindV1::SlashFraud, Some(*claim));
        // What the bond no longer holds was not slashed: the rest of the reservation is released, so the consumer's mirror matches.
        settle(out, producer, reserved - slashed, SettlementKindV1::ReleaseClaim, Some(*claim));
        settle(out, *accuser, reward, SettlementKindV1::AccuserReward, Some(*claim));
        settle(out, producer, slashed - reward, SettlementKindV1::Burn, Some(*claim));
        self.settle_demands_moot(claim, out);
        self.opv_sync_live(claim);
    }

    /// Return every demand bond of `d` (the demand is over).
    fn refund(&mut self, claim: &Digest, d: &DemandRowV1, out: &mut Vec<LedgerEventV1>) {
        for (bond, amount) in &d.demanders {
            if let Some(b) = self.bonds.get_mut(bond) {
                b.reserved = b.reserved.saturating_sub(*amount);
            }
            settle(out, *bond, *amount, SettlementKindV1::ReleaseDemand, Some(*claim));
        }
    }

    /// Every open demand on a convicted claim is moot: its bonds return. No session survives to pre-empt anything.
    fn settle_demands_moot(&mut self, claim: &Digest, out: &mut Vec<LedgerEventV1>) {
        let keys: Vec<_> = self.demands.keys().filter(|(c, _, _)| c == claim).copied().collect();
        let mut refunded = 0u32;
        for k in keys {
            if let Some(d) = self.demands.remove(&k) {
                refunded += d.demanders.len() as u32;
                self.refund(claim, &d, out);
            }
        }
        if refunded > 0 {
            out.push(LedgerEventV1::DemandsMoot { claim: *claim, refunded });
        }
    }

    fn file_demand(
        &mut self,
        demander: &Digest,
        claim: &Digest,
        stage: u8,
        position: u32,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "FileDemand";
        let rule = |why: &str| KernelRefusalV1::rule(NAME, why);
        let daa = self.daa;
        let Some(row) = self.claims.get(claim) else { return Err(rule("no such claim")) };
        if row.terminal_for_demands() {
            return Err(rule("the claim is already decided"));
        }
        let is_final = matches!(row.life.state, ClaimStateV1::Final { .. });
        // Before Final, only while the challenge window is open (so no demand extends Final past window + deadline + grace); after
        // Final, within the liability horizon (material for a post-Final proof), without touching the lifecycle.
        let path = self.policy.court_deadline_daa.saturating_add(self.policy.proof_grace_daa);
        let window_open = match &row.life.state {
            // The demand's whole path (its deadline, then the grace after a service) must fit in the horizon, or a late demand
            // would serve material nobody could still prosecute with.
            ClaimStateV1::Final { .. } => row.liability_until.is_some_and(|u| daa.saturating_add(path) <= u),
            s => demand_window_open(s, daa),
        };
        if !window_open {
            return Err(rule("the challenge window is closed"));
        }
        if row.body.position(stage, position).is_none() {
            return Err(rule("the claim commits no such position"));
        }
        let k = (*claim, stage, position);
        if self.served.contains_key(&k) {
            return Err(rule("already served: it is public"));
        }
        let need = self.policy.demand_bond;
        let Some(b) = self.bonds.get(demander) else { return Err(rule("the demander is not a registered bond")) };
        if b.exit_requested.is_some() || b.free() < need {
            return Err(rule("the demander's free collateral does not cover the demand bond"));
        }
        if let Some(d) = self.demands.get_mut(&k) {
            // Shared progress: a second demander joins the open demand rather than being refused by it (its deadline is the open
            // demand's: joining restarts nothing).
            if !d.demanders.iter().any(|(b, _)| b == demander) {
                d.demanders.push((*demander, need));
                self.bonds.get_mut(demander).expect("checked").reserved += need;
                settle(out, *demander, need, SettlementKindV1::ReserveDemand, Some(*claim));
            }
            out.push(LedgerEventV1::DemandJoined { claim: *claim, stage, position });
            return Ok(());
        }
        // One session per stage position: the count is bounded by the claim's positions, and no demand can crowd out another.
        self.bonds.get_mut(demander).expect("checked").reserved += need;
        let deadline = daa + self.policy.court_deadline_daa;
        self.demands.insert(k, DemandRowV1 { demanders: vec![(*demander, need)], filed_daa: daa, deadline_daa: deadline, last: None });
        if !is_final {
            let row = self.claims.get_mut(claim).expect("checked");
            let _ = row.life.apply(ClaimEventV1::DisputeFiled { daa });
        }
        settle(out, *demander, need, SettlementKindV1::ReserveDemand, Some(*claim));
        out.push(LedgerEventV1::DemandOpened { claim: *claim, stage, position, deadline });
        Ok(())
    }

    fn close_demand(&mut self, k: DemandKeyV1, out: &mut Vec<LedgerEventV1>) {
        let Some(d) = self.demands.remove(&k) else { return };
        self.refund(&k.0, &d, out);
        let daa = self.daa;
        if let Some(row) = self.claims.get_mut(&k.0)
            && matches!(row.life.state, ClaimStateV1::Disputed { .. })
        {
            let _ = row.life.apply(ClaimEventV1::CourtVerdict { daa, convicted: false });
        }
    }

    fn respond(
        &mut self,
        claim: &Digest,
        stage: u8,
        position: u32,
        bytes: &[u8],
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "Respond";
        let k = (*claim, stage, position);
        if !self.demands.contains_key(&k) {
            return Err(KernelRefusalV1::rule(NAME, "no open demand for this position"));
        }
        let row = self.claims.get(claim).expect("a demand names a committed claim");
        let limit = self.bounds_of(&row.class_binding_id).map(|b| b.max_response_bytes).unwrap_or(0);
        let verdict = if bytes.len() as u128 > limit {
            Err("oversized")
        } else {
            self.charge(NAME, 0)?;
            let row = self.claims.get(claim).expect("checked");
            let (values, inputs) = row.body.position(stage, position).expect("a demand names a committed position");
            classify_position_response_v1(values, &self.derived_mask(row, stage), inputs, bytes)
        };
        match verdict {
            Ok(served) => {
                self.served.insert(k, served);
                // The values are public from now on: the claim may not reach Final before a prosecution they enable can be filed.
                let until = self.daa.saturating_add(self.policy.proof_grace_daa);
                if let Some(row) = self.claims.get_mut(claim) {
                    let _ = row.life.apply(ClaimEventV1::ProofGrace { until_daa: until });
                }
                self.close_demand(k, out);
                out.push(LedgerEventV1::Served { claim: *claim, stage, position });
            }
            Err(class) => {
                if let Some(d) = self.demands.get_mut(&k) {
                    d.last = Some(response_class_code(class));
                }
                out.push(LedgerEventV1::ResponseRejected { claim: *claim, stage, position, class });
            }
        }
        Ok(())
    }

    /// Deadlines, windows, Final, liability release.
    fn tick_into(&mut self, out: &mut Vec<LedgerEventV1>) {
        let daa = self.daa;
        // Unrevealed seals expire (a junk seal holds nothing and lives a bounded time).
        let ttl = self.policy.seal_ttl_daa;
        self.seals.retain(|_, row| daa <= row.daa.saturating_add(ttl));
        // Demands past their deadline: the producer's availability default.
        let due: Vec<_> = self.demands.iter().filter(|(_, d)| daa >= d.deadline_daa).map(|(k, _)| *k).collect();
        for (claim, stage, position) in due {
            let Some(d) = self.demands.remove(&(claim, stage, position)) else { continue };
            let last = d.last_class();
            let post_final = self.claims.get(&claim).is_some_and(|r| matches!(r.life.state, ClaimStateV1::Final { .. }));
            // DA16: a transferred claim's material is its providers' obligation — the producer is not charged for this failure.
            if self.provider_liable.contains(&claim) {
                self.provider_liable_default(claim, stage, position, last, post_final, d, out);
                continue;
            }
            // Before Final a default costs the fixed penalty (the claim is Unavailable and earns no reward). After Final the reward
            // was already paid, so a default forfeits the WHOLE remaining reservation: withholding is never cheaper than a
            // conviction would be for the reward it kept.
            let collateral = self.claims.get(&claim).and_then(|r| self.bonds.get(&r.producer)).map_or(0, |b| b.collateral);
            let taken = self
                .claims
                .get(&claim)
                .map(|r| if post_final { r.reserved } else { r.reserved.min(self.policy.default_penalty) })
                .unwrap_or(0);
            // Never instruct more than the bond holds.
            let penalty = taken.min(collateral);
            // Before Final the demanders share the penalty equally. After Final the forfeit is burned whole: a demander may be the
            // producer's own Sybil, and paying it would let the colluders recoup part of their forfeit. Their bonds return either way.
            // An OPV claim's pre-Final default also burns `default_burn_permille` of the penalty (RFC-0015 §8.2): a producer cannot
            // cycle its own penalty through a Sybil demander for nothing.
            let opv_burn = match (self.opv.claims.contains_key(&claim), post_final, self.opv.policy) {
                (true, false, Some(p)) => (penalty as u128 * p.economics.default_burn_permille as u128 / 1000) as u64,
                _ => 0,
            };
            let paid = if post_final { 0 } else { penalty - opv_burn };
            let share = if d.demanders.is_empty() { 0 } else { paid / d.demanders.len() as u64 };
            let burn = penalty - share * d.demanders.len() as u64;
            self.burned += burn;
            let producer = self.claims.get(&claim).map(|r| r.producer);
            if let Some(row) = self.claims.get_mut(&claim) {
                let was_final = matches!(row.life.state, ClaimStateV1::Final { .. });
                row.reserved -= taken;
                if !was_final {
                    let _ = row.life.apply(ClaimEventV1::MaterialUnavailable { daa, producer_defaulted: true });
                }
                if let Some(b) = self.bonds.get_mut(&row.producer) {
                    b.reserved = b.reserved.saturating_sub(taken);
                    b.collateral = b.collateral.saturating_sub(penalty);
                }
            }
            out.push(if post_final {
                LedgerEventV1::PostFinalDefault { claim, stage, position, last, forfeited: penalty }
            } else {
                LedgerEventV1::ProducerDefault { claim, stage, position, last, penalty }
            });
            if let Some(producer) = producer {
                settle(out, producer, penalty, SettlementKindV1::SlashDefault, Some(claim));
                settle(out, producer, taken - penalty, SettlementKindV1::ReleaseClaim, Some(claim));
                for (bond, _) in &d.demanders {
                    settle(out, *bond, share, SettlementKindV1::DemanderShare, Some(claim));
                }
                settle(out, producer, burn, SettlementKindV1::Burn, Some(claim));
            }
            self.refund(&claim, &d, out);
            // An unavailable claim never finalizes; its other open demands are moot.
            self.settle_demands_moot(&claim, out);
            if post_final && let Some(o) = self.opv.claims.get_mut(&claim) {
                o.forfeited_after_final = true;
            }
            self.opv_sync_live(&claim);
        }
        let ids: Vec<Digest> = self.claims.keys().copied().collect();
        for id in ids {
            let (producer, before, after, reserved, liability) = {
                let row = self.claims.get_mut(&id).expect("listed");
                let before = row.life.state.clone();
                let _ = row.life.apply(ClaimEventV1::Tick { daa });
                (row.producer, before, row.life.state.clone(), row.reserved, row.liability_until)
            };
            match (&before, &after) {
                (b, ClaimStateV1::Final { .. }) if !matches!(b, ClaimStateV1::Final { .. }) => {
                    let row = self.claims.get_mut(&id).expect("listed");
                    row.liability_until = Some(daa + self.policy.liability_daa);
                    let reward = if row.convicted { 0 } else { self.policy.claim_reward };
                    row.rewarded = reward > 0;
                    out.push(LedgerEventV1::Final { claim: id, reward });
                    settle(out, producer, reward, SettlementKindV1::FinalReward, Some(id));
                }
                (b, ClaimStateV1::TimedOut { .. }) if !matches!(b, ClaimStateV1::TimedOut { .. }) => {
                    self.release(&id, producer, reserved, out);
                    out.push(LedgerEventV1::TimedOut { claim: id });
                }
                (ClaimStateV1::Unavailable { .. }, _) if reserved > 0 => {
                    self.release(&id, producer, reserved, out);
                }
                (ClaimStateV1::Final { .. }, _) if reserved > 0 && liability.is_some_and(|u| daa > u) => {
                    self.release(&id, producer, reserved, out);
                    out.push(LedgerEventV1::Released { claim: id });
                }
                _ => {}
            }
        }
    }

    /// **DA16: would `bytes` serve this committed position?** — exactly the classification a `Respond` gets (the class's response
    /// envelope, then `classify_position_response_v1` against the committed values), without a demand and without touching the state.
    /// A provider court's answer and a transport fetch are judged by this one function.
    pub fn classify_served_position_v1(
        &self,
        claim: &Digest,
        stage: u8,
        position: u32,
        bytes: &[u8],
    ) -> Result<ServedPositionV1, &'static str> {
        let row = self.claims.get(claim).ok_or("no such claim")?;
        let (values, inputs) = row.body.position(stage, position).ok_or("the claim commits no such position")?;
        let limit = self.bounds_of(&row.class_binding_id).map(|b| b.max_response_bytes).unwrap_or(0);
        if bytes.len() as u128 > limit {
            return Err("oversized");
        }
        classify_position_response_v1(values, &self.derived_mask(row, stage), inputs, bytes)
    }

    /// **DA16: a demand on a provider-liable claim defaulted** (see [`LedgerEventV1::ProviderLiableDefault`]). The producer's reservation is
    /// untouched here: before Final the claim turns `Unavailable` and the tick's release returns it whole; after Final it stays held for
    /// the claim's fraud liability. The demand bonds return as on the producer path; the open demands of the claim are moot.
    #[allow(clippy::too_many_arguments)]
    fn provider_liable_default(
        &mut self,
        claim: Digest,
        stage: u8,
        position: u32,
        last: Option<&'static str>,
        post_final: bool,
        d: DemandRowV1,
        out: &mut Vec<LedgerEventV1>,
    ) {
        let daa = self.daa;
        if !post_final && let Some(row) = self.claims.get_mut(&claim) {
            let _ = row.life.apply(ClaimEventV1::MaterialUnavailable { daa, producer_defaulted: false });
        }
        let demanders = d.demanders.iter().map(|(b, _)| *b).collect();
        out.push(LedgerEventV1::ProviderLiableDefault { claim, stage, position, last, post_final, demanders });
        self.refund(&claim, &d, out);
        self.settle_demands_moot(&claim, out);
        if post_final && let Some(o) = self.opv.claims.get_mut(&claim) {
            o.forfeited_after_final = true;
        }
        self.opv_sync_live(&claim);
    }

    /// **DA16: every provider behind a transferred claim's material has defaulted — the claim lapses** (a common-mode outage, never a
    /// conviction). Consumer-called from its court's rows; refused unless the claim is provider-liable. A claim already decided
    /// (convicted, unavailable, timed out) is left as it is (`Ok` with no event).
    pub fn provider_lapse(&mut self, claim: &Digest) -> Result<Vec<LedgerEventV1>, KernelRefusalV1> {
        const NAME: &str = "ProviderLapse";
        if !self.provider_liable.contains(claim) {
            return Err(KernelRefusalV1::rule(NAME, "the claim's material is not its providers' obligation"));
        }
        let Some(row) = self.claims.get(claim) else { return Err(KernelRefusalV1::rule(NAME, "no such claim")) };
        if row.terminal_for_demands() {
            return Ok(Vec::new());
        }
        let post_final = matches!(row.life.state, ClaimStateV1::Final { .. });
        let mut out = Vec::new();
        if !post_final {
            let daa = self.daa;
            let row = self.claims.get_mut(claim).expect("checked");
            let _ = row.life.apply(ClaimEventV1::MaterialUnavailable { daa, producer_defaulted: false });
        }
        out.push(LedgerEventV1::ProviderLapsed { claim: *claim, post_final });
        self.settle_demands_moot(claim, &mut out);
        if post_final && let Some(o) = self.opv.claims.get_mut(claim) {
            o.forfeited_after_final = true;
        }
        self.opv_sync_live(claim);
        Ok(out)
    }

    fn release(&mut self, claim: &Digest, producer: Digest, amount: u64, out: &mut Vec<LedgerEventV1>) {
        if let Some(row) = self.claims.get_mut(claim) {
            row.reserved = 0;
        }
        if let Some(b) = self.bonds.get_mut(&producer) {
            b.reserved = b.reserved.saturating_sub(amount);
        }
        settle(out, producer, amount, SettlementKindV1::ReleaseClaim, Some(*claim));
        self.opv_sync_live(claim);
    }
}

/// The wire overhead around the payload of a `FileProof` (version, tag, accuser, claim, proof variant, length), of a `Respond`
/// (version, tag, claim, stage, position, length) and of the evidence object a `CommitClaim` carries beside the commitments.
const FILE_PROOF_OVERHEAD_V1: u128 = 1 + 1 + 64 + 64 + 1 + 4;
const RESPOND_OVERHEAD_V1: u128 = 1 + 1 + 64 + 1 + 4 + 4;
const COMMIT_OVERHEAD_V1: u128 = 1 << 16;

/// **A class is prosecutable only if its worst filing, its largest response and its commitments fit the carriers** — the encoded-size
/// ceilings of the objects that carry them. A court that cannot be carried by a `FileProof`, a response that cannot be carried by a
/// `Respond` (so every demand would default) or commitments that cannot be carried by a `CommitClaim` would make a class the gate
/// calls complete impossible to prosecute, serve or even commit.
///
/// **Not called by the ledger's registration**: the carrier's real size is the consumer's (its transaction mass limit), and this
/// crate's own ceilings are far below the gate's worst-case envelope even for the toy fixtures (a declared worst case at the history
/// bound: tens of MB per opening, ~190 MB per position response). The consumer calls it with its real caps when it accepts a class.
pub fn carrier_fit_v1(b: &ProsecutionBoundsV1, filing_cap: usize, response_cap: usize, commit_cap: usize) -> Result<(), String> {
    let filing = (b.max_filing_bytes as u128).max(b.max_response_bytes) + FILE_PROOF_OVERHEAD_V1;
    if filing > filing_cap as u128 {
        return Err(format!("a worst-case filing of {filing} bytes does not fit a FileProof ({filing_cap})"));
    }
    if b.max_response_bytes + RESPOND_OVERHEAD_V1 > response_cap as u128 {
        return Err(format!("a position response of {} bytes does not fit a Respond ({response_cap})", b.max_response_bytes));
    }
    if b.max_retained_state + COMMIT_OVERHEAD_V1 > commit_cap as u128 {
        return Err(format!("{} bytes of commitments do not fit a claim commitment ({commit_cap})", b.max_retained_state));
    }
    Ok(())
}

/// Why a filing is past the class's envelope, if it is (checked before any court runs).
fn oversized(proof: &ProsecutionV1, b: &ProsecutionBoundsV1) -> Option<String> {
    let (len, limit) = match proof {
        ProsecutionV1::Kernel(bytes) | ProsecutionV1::Pipeline(bytes) => (bytes.len() as u128, b.max_filing_bytes as u128),
        ProsecutionV1::Decode(f) => (f.logits.bytes.len() as u128, b.max_response_bytes),
    };
    (len > limit).then(|| format!("a {len}-byte filing past the class's {limit}-byte envelope"))
}

/// Whether a new demand may open against a claim in `state` before Final: while it is checking (up to the receipts' deadline) or
/// passed (up to the challenge window's end); a disputed claim by the state its disputes resume to. Every demand lives
/// `court_deadline_daa`, so Final is never later than `window end + court deadline + proof grace`, however many demands are filed.
pub fn demand_window_open(state: &ClaimStateV1, daa: u64) -> bool {
    match state {
        ClaimStateV1::Checking { deadline_daa, .. } => daa <= *deadline_daa,
        ClaimStateV1::ProbabilisticPass { window_end_daa, .. } | ClaimStateV1::Challengeable { window_end_daa, .. } => {
            daa < *window_end_daa
        }
        ClaimStateV1::Disputed { resume, .. } => demand_window_open(resume, daa),
        _ => false,
    }
}

/// **Any public source of committed values** — the producer's server, a DA provider. Values served on chain are read from the
/// ledger before it.
pub trait PublicSourceV1 {
    /// Stage `stage`'s node value (a single program is stage 0).
    fn node(&self, stage: u8, position: u32, occurrence: u16, node: u16) -> Option<Tensor>;
    /// Stage `stage`'s input `k` at `position` (none for a single program).
    fn input(&self, _stage: u8, _position: u32, _k: u16) -> Option<Tensor> {
        None
    }
}

/// **A fresh outsider's view of a claim, from ledger state and whatever material is served** — the verifier a new node builds
/// after the claim was published.
pub struct OutsiderV1<'a> {
    pub ledger: &'a KernelLedgerV1,
    pub claim: Digest,
    pub material: &'a dyn PublicSourceV1,
    /// The public artifact: tensors the class registered only the commitments of, authenticated against them before use.
    pub artifact: &'a dyn PublicArtifactV1,
    /// The outsider's own randomness for its probabilistic checks (never a claim's public beacon, which a producer may have
    /// predicted): the faults it finds are convictable whatever vectors found them.
    pub salt: Digest,
}

/// What an outsider concludes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutsiderFindingV1 {
    /// Every relation, the decode and the bindings check.
    Clean,
    /// A proof to file.
    Prosecute(ProsecutionV1),
    /// Stage positions some committed value of which nobody serves: demand them (one round, all at once).
    Demand(Vec<(u8, u32)>),
}

/// One stage's public material for a court: served values first, then the source; the class's public artifact, authenticated
/// against the registered commitments.
struct StageMaterial<'b> {
    o: &'b OutsiderV1<'b>,
    stage: u8,
    program: u16,
    commitments: &'b ParamCommitmentsV1,
}

impl MaterialV1 for StageMaterial<'_> {
    fn node_value(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
        self.o.node(self.stage, p, s, n)
    }
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        let want = self.commitments.by_instance.get(&(index, layer))?;
        let t = self.o.artifact.param(self.program, index, layer)?;
        // A tensor the registered commitment does not open to is unavailable, not trusted.
        (tensor_commitment(&t) == *want).then_some(t)
    }
    fn stage_input(&self, k: u16, position: u32) -> Option<Tensor> {
        self.o.input(self.stage, position, k)
    }
}

impl OutsiderV1<'_> {
    fn node(&self, stage: u8, p: u32, s: u16, n: u16) -> Option<Tensor> {
        if let Some(sp) = self.ledger.served.get(&(self.claim, stage, p)) {
            return sp.values.get(s as usize)?.get(n as usize)?.as_ref()?.decode().ok();
        }
        self.material.node(stage, p, s, n)
    }

    fn input(&self, stage: u8, p: u32, k: u16) -> Option<Tensor> {
        if let Some(sp) = self.ledger.served.get(&(self.claim, stage, p)) {
            return sp.inputs.get(k as usize)?.decode().ok();
        }
        self.material.input(stage, p, k)
    }

    /// Every committed value (but the derived ones, which the verifier rebuilds) and input, authenticated against its commitment:
    /// the stage positions any is missing from.
    fn missing(&self, row: &ClaimRowV1) -> Vec<(u8, u32)> {
        let body = &row.body;
        let ok = |t: Option<Tensor>, c: &Digest| t.is_some_and(|t| tensor_commitment(&t) == *c);
        let mut out = Vec::new();
        for (stage, positions) in body.stages() {
            let derived = self.ledger.derived_mask(row, stage);
            let is_derived = |s: usize, n: usize| derived.get(s).and_then(|o| o.get(n)).copied().unwrap_or(false);
            for p in 0..positions {
                let (values, inputs) = body.position(stage, p).expect("listed");
                let nodes_ok = values.iter().enumerate().all(|(s, occ)| {
                    occ.iter().enumerate().all(|(n, c)| is_derived(s, n) || ok(self.node(stage, p, s as u16, n as u16), c))
                });
                let inputs_ok = inputs.iter().enumerate().all(|(k, c)| ok(self.input(stage, p, k as u16), c));
                if !(nodes_ok && inputs_ok) {
                    out.push((stage, p));
                }
            }
        }
        out
    }

    /// **Check the claim.** The verifier is built from the ledger's public record bytes and the binary's own descriptors.
    pub fn check(&self) -> Result<OutsiderFindingV1, String> {
        let l = self.ledger;
        let row = l.claims.get(&self.claim).ok_or("no such claim")?;
        let missing = self.missing(row);
        if !missing.is_empty() {
            return Ok(OutsiderFindingV1::Demand(missing));
        }
        match &row.body {
            ClaimBodyV1::Program { claim, .. } => self.check_program(row, claim),
            ClaimBodyV1::Pipeline { .. } => self.check_pipeline(row),
        }
    }

    fn check_program(&self, row: &ClaimRowV1, claim: &KernelClaimV1) -> Result<OutsiderFindingV1, String> {
        use crate::verify::{ScopeV1, ScopeVerdictV1};
        let l = self.ledger;
        let class = l.classes.get(&row.class_binding_id).ok_or("no such class")?;
        let job = l.jobs.get(&row.job_id).ok_or("no such job")?;
        let (record, header) = l.public_record(&self.claim).ok_or("no record")?;
        let fresh = FreshVerifierV1::from_public_bytes(&record.to_bytes(), &l.known, header)?;
        // The decode relation: each delivered token against the committed logits that select it.
        let (post, logits) = class.logits_at();
        for r in 0..claim.generated.len() {
            let p = claim.select_position(job, r);
            let t = self.node(0, p, post, logits).ok_or(format!("({p}, {post}, {logits}) vanished"))?;
            if job.decode.select(&t) != Some(claim.generated[r]) {
                return Ok(OutsiderFindingV1::Prosecute(ProsecutionV1::Decode(DecodeFaultV1 {
                    index: r as u32,
                    logits: TensorWireV1::of(&t),
                })));
            }
        }
        // Then every relation the plan covers.
        let material = StageMaterial { o: self, stage: 0, program: 0, commitments: &class.param_commitments };
        Ok(match fresh.check_salted(&material, &ScopeV1::WholeClaim, self.salt) {
            ScopeVerdictV1::Pass { .. } => OutsiderFindingV1::Clean,
            ScopeVerdictV1::Fault(p) => {
                OutsiderFindingV1::Prosecute(ProsecutionV1::Kernel(crate::public::FaultProofWireV1::of(&p).to_bytes()))
            }
            ScopeVerdictV1::Unavailable { what } => return Err(format!("unavailable with every value in hand: {what}")),
            ScopeVerdictV1::EvidenceMalformed { why } => return Err(format!("malformed evidence on chain: {why}")),
            ScopeVerdictV1::Inconsistent { why } => return Err(format!("verifier inconsistency: {why}")),
        })
    }

    fn check_pipeline(&self, row: &ClaimRowV1) -> Result<OutsiderFindingV1, String> {
        let l = self.ledger;
        let class = l.pipeline_classes.get(&row.class_binding_id).ok_or("no such class")?;
        let (record, header, binding) = l.pipeline_public_record(&self.claim).ok_or("no record")?;
        let mode = l.mode_of_class(&header.class_binding_id);
        let fresh = FreshPipelineVerifierV1::from_public_bytes_in_mode(&record.to_bytes(), &l.known, header, &binding, mode)?;
        let mats: Vec<StageMaterial<'_>> = class
            .pipeline
            .stages
            .iter()
            .enumerate()
            .map(|(si, st)| StageMaterial {
                o: self,
                stage: si as u8,
                program: st.program,
                commitments: &class.param_commitments[st.program as usize],
            })
            .collect();
        let refs: Vec<&dyn MaterialV1> = mats.iter().map(|m| m as &dyn MaterialV1).collect();
        match fresh.check_salted(&refs, self.salt) {
            PipelineFindingV1::Pass => Ok(OutsiderFindingV1::Clean),
            PipelineFindingV1::Fault(w) => Ok(OutsiderFindingV1::Prosecute(ProsecutionV1::Pipeline(w.to_bytes()))),
            PipelineFindingV1::Unavailable(what) => Err(format!("unavailable with every value in hand: {what}")),
            PipelineFindingV1::Malformed(why) => Err(format!("malformed evidence on chain: {why}")),
            PipelineFindingV1::Inconsistent(why) => Err(format!("verifier inconsistency: {why}")),
        }
    }
}

/// The set of claims a ledger convicted (a convenience for tests and reports).
pub fn convicted_claims(l: &KernelLedgerV1) -> BTreeSet<Digest> {
    l.claims.iter().filter(|(_, r)| r.convicted).map(|(k, _)| *k).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filing_past_the_class_envelope_is_dismissed_before_any_court_runs() {
        let b = ProsecutionBoundsV1 {
            max_public_bytes: 0,
            max_opening_bytes: 10,
            max_filing_bytes: 100,
            max_response_bytes: 50,
            max_localization_rounds: 2,
            max_court_work: 0,
            max_verifier_ram: 0,
            max_retained_state: 0,
            max_concurrent_sessions: 1,
            deadline_daa: 1,
        };
        assert!(oversized(&ProsecutionV1::Kernel(vec![0; 100]), &b).is_none());
        assert!(oversized(&ProsecutionV1::Kernel(vec![0; 101]), &b).is_some());
        assert!(oversized(&ProsecutionV1::Pipeline(vec![0; 101]), &b).is_some());
        let logits = |n: usize| TensorWireV1 { dtype: 0, shape: vec![n as u64], bytes: vec![0; n] };
        assert!(oversized(&ProsecutionV1::Decode(DecodeFaultV1 { index: 0, logits: logits(50) }), &b).is_none());
        assert!(oversized(&ProsecutionV1::Decode(DecodeFaultV1 { index: 0, logits: logits(51) }), &b).is_some());
    }

    #[test]
    fn a_class_must_fit_its_filing_response_and_commitments_in_the_route_objects() {
        let b = ProsecutionBoundsV1 {
            max_public_bytes: 0,
            max_opening_bytes: 10,
            max_filing_bytes: 1000,
            max_response_bytes: 2000,
            max_localization_rounds: 2,
            max_court_work: 0,
            max_verifier_ram: 0,
            max_retained_state: 5000,
            max_concurrent_sessions: 1,
            deadline_daa: 1,
        };
        let roomy = 1 << 20;
        carrier_fit_v1(&b, roomy, roomy, roomy).unwrap();
        // A decode filing carries one logits tensor (up to a response's size): the larger of the two sizes the filing cap.
        assert!(carrier_fit_v1(&b, 2000, roomy, roomy).is_err(), "the filing envelope is the larger of a court and a logits row");
        assert!(carrier_fit_v1(&b, 2200, roomy, roomy).is_ok());
        assert!(carrier_fit_v1(&b, roomy, 2000, roomy).is_err(), "a response that cannot be carried is a demand that always defaults");
        assert!(carrier_fit_v1(&b, roomy, roomy, 5000).is_err(), "commitments that cannot be carried: no claim can be committed");
    }
}
