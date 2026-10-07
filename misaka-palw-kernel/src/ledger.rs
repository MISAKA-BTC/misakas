//! **An in-process chain for the kernel route: the consensus rules public prosecution relies on, as a deterministic fold.**
//!
//! [`KernelLedgerV1`] is a state that [`KernelLedgerV1::apply_block`] advances by blocks of [`LedgerTxV1`]. It is the local
//! harness for the consensus properties a library cannot show alone (ADR-0173, RFC-0015 §1.1): it is not wired into the node,
//! and the real-chain drill stays an external gate. Every rule below reads only ledger state — registered classes (program or
//! pipeline, plan, public artifact), posted jobs, committed claims (evidence object and trace commitments) and material served
//! in answer to demands — so every adjudication is something any node replaying the blocks recomputes.
//!
//! * **Collateral** (category 19): a claim reserves `claim_collateral` of its producer bond's FREE collateral (no double use);
//!   the reservation lasts until the claim's liability horizon ends; an exiting bond backs nothing new and withdraws only after
//!   its delay with nothing reserved.
//! * **Inclusion refusals** (categories 5–9, 12, 13): a claim whose evidence names another job, class, input (a borrowed trace),
//!   `R` seed, output, length or generation budget, whose trace commitments are not the evidence's, or whose evidence a court
//!   could only call malformed, is refused at inclusion — objectively, from public objects.
//! * **Direct proofs** (categories 1–4, 7–9, 11, 13, 16): a kernel fault proof, a decode fault, or a pipeline stage / edge /
//!   decode fault is adjudicated in the block that carries it by the same courts any node runs, from ledger state alone. It is
//!   never refused because a demand or another session is open: open sessions on a convicted claim settle as moot and their
//!   bonds return (no pre-emption).
//! * **Demands** (categories 10, 14, 15): any bond may demand one committed POSITION of one stage (every node value and stage
//!   input of it); joining an open demand for the same position shares its progress. One session per stage position, so every
//!   position is demandable at once and no set of demanders (a producer's friends included) can starve another's: one round of
//!   demands then one direct proof is every prosecution's whole path. The response is bounded and classified (served /
//!   malformed / wrong bytes / wrong root / fake opening / partial / oversized); silence or non-serving past the deadline is
//!   the producer's availability default — a fixed penalty, never the fraud slash. A bond's open demands are limited by its
//!   free collateral, not by a count.
//! * **Filings are bounded and priced**: a filed proof must fit the class's filing envelope, and a dismissed one forfeits
//!   `dismissed_proof_fee` (a convicting or duplicate one does not).
//! * **Final** (category 17): a claim needs the Panel's covered tally, the closed window and no open demand. New demands are
//!   accepted only while the window is open and each lives `court_deadline_daa`, so no spam extends Final past
//!   `window end + court deadline`. A dismissed direct proof changes nothing.
//! * **Liability** (category A): after Final the reservation is held for `liability_daa`; a proof convicting in that horizon
//!   slashes it (post-Final liability), even if every Panel seat signed the claim covered.
//! * **Idempotence and replay** (category 18): a claim is convicted at most once (a second proof is `Duplicate`); the state is
//!   a pure function of the block sequence, so a reorg is a replay of the new branch and a restart or IBD is a replay from
//!   genesis ([`KernelLedgerV1::replay`]).

use std::collections::{BTreeMap, BTreeSet};

use misaka_palw_tir::pipeline::{TirPipelineV1, stream_stage};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::{MapParams, Tensor};

use crate::check::check_plan_v1;
use crate::descriptor::{ContextPolicyV1, KernelDescriptorV1, KernelScheduleV1, ModelKernelBindingV1};
use crate::evidence::{EvidenceHeaderV1, VerificationEvidenceV1};
use crate::gate::{ProsecutionBoundsV1, ProsecutionPolicyV1, public_pipeline_prosecution_complete_v1, public_prosecution_complete_v1};
use crate::hash::{Digest, finish, id, keyed};
use crate::job::{BindingFaultV1, DecodeFaultV1, DecodeRuleV1, KernelClaimV1, KernelJobV1, binding_fault_v1, verify_decode_fault_v1};
use crate::lifecycle::{ClaimEventV1, ClaimLifecycleV1, ClaimStateV1, LifecyclePolicyV1};
use crate::pipeline::{
    PipelineEvidenceV1, PipelineHeaderV1, PipelinePlanV1, check_pipeline_plan_v1, pipeline_job_root_v1, pipeline_root_v1,
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
use crate::trace::{EvidenceV1, ParamCommitmentsV1, tensor_commitment};
use crate::verify::MaterialV1;

/// The network's ledger policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedgerPolicyV1 {
    pub claim_collateral: u64,
    pub demand_bond: u64,
    pub check_window_daa: u64,
    pub challenge_window_daa: u64,
    pub court_deadline_daa: u64,
    pub liability_daa: u64,
    pub exit_delay_daa: u64,
    /// What a dismissed proof forfeits (burned): filings are not free court work.
    pub dismissed_proof_fee: u64,
    /// Of a slashed reservation, the convicting accuser's share (permille); the rest is burned.
    pub accuser_reward_permille: u16,
    /// What an availability default forfeits (to the demanders), never more than the reservation.
    pub default_penalty: u64,
    /// Paid to the producer at Final.
    pub claim_reward: u64,
    pub prosecution: ProsecutionPolicyV1,
}

impl LedgerPolicyV1 {
    /// The relations among the timings and amounts every rule below relies on.
    pub fn validate(&self) -> Result<(), String> {
        let p = self;
        let checks: [(bool, &str); 7] = [
            (p.court_deadline_daa == p.prosecution.court_deadline_daa, "the ledger's court deadline is the gate's"),
            (p.court_deadline_daa > 0 && p.challenge_window_daa > 0 && p.check_window_daa > 0, "every window is non-empty"),
            // A demand filed in the window's last block closes by `window end + court deadline`; the proof it enables must still
            // reach the claim, after Final if need be.
            (p.liability_daa > p.court_deadline_daa, "the liability horizon outlasts a demand's deadline"),
            (p.default_penalty <= p.claim_collateral, "a default never takes more than the reservation"),
            (p.demand_bond > 0 && p.dismissed_proof_fee > 0, "demands and filings are not free"),
            (p.accuser_reward_permille <= 1000, "the accuser's share is a share"),
            (p.claim_collateral > 0, "a claim reserves collateral"),
        ];
        match checks.iter().find(|(ok, _)| !ok) {
            Some((_, why)) => Err(format!("ledger policy: {why}")),
            None => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BondRowV1 {
    pub collateral: u64,
    pub reserved: u64,
    pub exit_requested: Option<u64>,
    pub credits: u64,
}

impl BondRowV1 {
    pub fn free(&self) -> u64 {
        self.collateral.saturating_sub(self.reserved)
    }
}

/// A registered single-program class: everything public a court reads about it.
#[derive(Clone, Debug)]
pub struct ClassRowV1 {
    pub descriptor: KernelDescriptorV1,
    pub program_bytes: Vec<u8>,
    pub program: TirProgramV1,
    pub plan: VerificationPlanV1,
    /// The artifact, public (a public-prosecution profile's weights are public by the gate).
    pub params: MapParams,
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
    /// Each program's artifact, public.
    pub params: Vec<MapParams>,
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
#[derive(Clone, Debug)]
pub enum ClaimBodyV1 {
    Program { claim: KernelClaimV1, evidence: VerificationEvidenceV1, commitments: Vec<Vec<Vec<Digest>>> },
    Pipeline { claim: PipelineClaimV1, evidence: PipelineEvidenceV1, stages: Vec<StageCommitmentsV1> },
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

#[derive(Clone, Debug)]
pub struct ClaimRowV1 {
    pub producer: Digest,
    pub class_binding_id: Digest,
    pub job_id: Digest,
    pub body: ClaimBodyV1,
    pub committed_daa: u64,
    pub beacon: Digest,
    pub life: ClaimLifecycleV1,
    pub reserved: u64,
    pub liability_until: Option<u64>,
    pub convicted: bool,
    pub rewarded: bool,
}

impl ClaimRowV1 {
    fn terminal_for_demands(&self) -> bool {
        self.convicted || matches!(self.life.state, ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemandRowV1 {
    /// Every bond that filed or joined it, with its bond amount.
    pub demanders: Vec<(Digest, u64)>,
    pub filed_daa: u64,
    pub deadline_daa: u64,
    /// The latest response's class name.
    pub last: Option<&'static str>,
}

/// What a bond files against a claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProsecutionV1 {
    /// A kernel fault proof's canonical bytes ([`crate::public::FaultProofWireV1`]).
    Kernel(Vec<u8>),
    /// A delivered token that is not the decode of the committed logits.
    Decode(DecodeFaultV1),
    /// A pipeline fault's canonical bytes ([`crate::pipeline_public::PipelineFaultWireV1`]: stage, edge or decode).
    Pipeline(Vec<u8>),
}

/// The demand key: `(claim, stage, position)`.
pub type DemandKeyV1 = (Digest, u8, u32);

#[derive(Clone, Debug)]
pub enum LedgerTxV1 {
    RegisterBond {
        bond: Digest,
        collateral: u64,
    },
    RequestExit {
        bond: Digest,
    },
    Withdraw {
        bond: Digest,
    },
    RegisterClass {
        descriptor: Digest,
        program_bytes: Vec<u8>,
        plan: VerificationPlanV1,
        params: MapParams,
        network: Digest,
        ruleset: Digest,
    },
    RegisterPipelineClass {
        descriptor: Digest,
        pipeline_bytes: Vec<u8>,
        program_bytes: Vec<Vec<u8>>,
        plan: PipelinePlanV1,
        params: Vec<MapParams>,
        decode: Option<DecodeRuleV1>,
        network: Digest,
        ruleset: Digest,
    },
    PostJob {
        job: KernelJobV1,
    },
    PostPipelineJob {
        job: PipelineJobPostV1,
    },
    CommitClaim {
        claim: KernelClaimV1,
        evidence: VerificationEvidenceV1,
        commitments: Vec<Vec<Vec<Digest>>>,
    },
    CommitPipelineClaim {
        claim: PipelineClaimV1,
        evidence: PipelineEvidenceV1,
        stages: Vec<StageCommitmentsV1>,
    },
    /// The Panel's tally reached coverage (an honest Panel, or every seat colluding: the ledger cannot tell, and need not).
    PanelCovered {
        claim: Digest,
    },
    FileProof {
        accuser: Digest,
        claim: Digest,
        proof: ProsecutionV1,
    },
    /// A demand for every committed value (and stage input) of one stage position.
    FileDemand {
        demander: Digest,
        claim: Digest,
        stage: u8,
        position: u32,
    },
    /// Anyone (the producer, a DA provider) answers a position demand: borsh [`crate::public::PositionResponseV1`].
    Respond {
        claim: Digest,
        stage: u8,
        position: u32,
        bytes: Vec<u8>,
    },
}

/// What a transaction did (the ledger's receipt log; part of the state).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LedgerEventV1 {
    Refused { tx: &'static str, why: String },
    ClassRegistered { class: Digest },
    JobPosted { job: Digest },
    ClaimCommitted { claim: Digest },
    Convicted { claim: Digest, accuser: Digest, slashed: u64, accuser_reward: u64, post_final: bool },
    ProofDismissed { claim: Digest, accuser: Digest, why: String, fee: u64 },
    Duplicate { claim: Digest },
    DemandOpened { claim: Digest, stage: u8, position: u32, deadline: u64 },
    DemandJoined { claim: Digest, stage: u8, position: u32 },
    Served { claim: Digest, stage: u8, position: u32 },
    ResponseRejected { claim: Digest, stage: u8, position: u32, class: &'static str },
    ProducerDefault { claim: Digest, stage: u8, position: u32, last: Option<&'static str>, penalty: u64 },
    DemandsMoot { claim: Digest, refunded: u32 },
    Final { claim: Digest, reward: u64 },
    TimedOut { claim: Digest },
    Released { claim: Digest },
    Withdrawn { bond: Digest, amount: u64 },
}

/// One block.
#[derive(Clone, Debug)]
pub struct LedgerBlockV1 {
    pub daa: u64,
    pub txs: Vec<LedgerTxV1>,
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
    pub events: Vec<(u64, LedgerEventV1)>,
    pub burned: u64,
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
            events: Vec::new(),
            burned: 0,
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

    /// A digest of the whole state (two nodes agree iff their roots do).
    pub fn root(&self) -> Digest {
        id(b"misaka-palw/kernel/ledger-root/v1", format!("{self:?}").as_bytes())
    }

    fn log(&mut self, e: LedgerEventV1) {
        self.events.push((self.daa, e));
    }

    pub fn events_at(&self, daa: u64) -> impl Iterator<Item = &LedgerEventV1> {
        self.events.iter().filter(move |(d, _)| *d == daa).map(|(_, e)| e)
    }

    pub fn apply_block(&mut self, block: &LedgerBlockV1) {
        if block.daa < self.daa {
            return; // blocks are in DAA order; a stale block is not applied
        }
        self.daa = block.daa;
        for tx in &block.txs {
            self.apply_tx(tx);
        }
        self.tick();
    }

    fn refuse(&mut self, tx: &'static str, why: impl Into<String>) {
        self.log(LedgerEventV1::Refused { tx, why: why.into() });
    }

    fn apply_tx(&mut self, tx: &LedgerTxV1) {
        match tx {
            LedgerTxV1::RegisterBond { bond, collateral } => {
                let row =
                    self.bonds.entry(*bond).or_insert(BondRowV1 { collateral: 0, reserved: 0, exit_requested: None, credits: 0 });
                if row.exit_requested.is_some() {
                    self.refuse("RegisterBond", "the bond is exiting");
                } else {
                    row.collateral = row.collateral.saturating_add(*collateral);
                }
            }
            LedgerTxV1::RequestExit { bond } => match self.bonds.get_mut(bond) {
                Some(b) if b.exit_requested.is_none() => b.exit_requested = Some(self.daa),
                _ => self.refuse("RequestExit", "no such bond, or already exiting"),
            },
            LedgerTxV1::Withdraw { bond } => {
                let (daa, delay) = (self.daa, self.policy.exit_delay_daa);
                match self.bonds.get(bond) {
                    Some(b) if b.exit_requested.is_some_and(|at| daa >= at + delay) && b.reserved == 0 => {
                        let amount = b.collateral + b.credits;
                        self.bonds.remove(bond);
                        self.log(LedgerEventV1::Withdrawn { bond: *bond, amount });
                    }
                    Some(_) => self.refuse("Withdraw", "not exiting, inside the delay, or collateral still reserved"),
                    None => self.refuse("Withdraw", "no such bond"),
                }
            }
            LedgerTxV1::RegisterClass { descriptor, program_bytes, plan, params, network, ruleset } => {
                match self.register_class(descriptor, program_bytes, plan, params, *network, *ruleset) {
                    Ok(class) => self.log(LedgerEventV1::ClassRegistered { class }),
                    Err(why) => self.refuse("RegisterClass", why),
                }
            }
            LedgerTxV1::RegisterPipelineClass {
                descriptor,
                pipeline_bytes,
                program_bytes,
                plan,
                params,
                decode,
                network,
                ruleset,
            } => {
                match self.register_pipeline_class(
                    descriptor,
                    pipeline_bytes,
                    program_bytes,
                    plan,
                    params,
                    *decode,
                    *network,
                    *ruleset,
                ) {
                    Ok(class) => self.log(LedgerEventV1::ClassRegistered { class }),
                    Err(why) => self.refuse("RegisterPipelineClass", why),
                }
            }
            LedgerTxV1::PostJob { job } => {
                let Some(class) = self.classes.get(&job.class_binding_id) else {
                    return self.refuse("PostJob", "no such class");
                };
                match job.well_formed(class.program.token_bound, class.plan.max_positions) {
                    Ok(()) => {
                        self.jobs.insert(job.id(), job.clone());
                        self.log(LedgerEventV1::JobPosted { job: job.id() });
                    }
                    Err(why) => self.refuse("PostJob", why),
                }
            }
            LedgerTxV1::PostPipelineJob { job } => match self.post_pipeline_job(job) {
                Ok(()) => {
                    self.pipeline_jobs.insert(job.id(), job.clone());
                    self.log(LedgerEventV1::JobPosted { job: job.id() });
                }
                Err(why) => self.refuse("PostPipelineJob", why),
            },
            LedgerTxV1::CommitClaim { claim, evidence, commitments } => match self.commit_claim(claim, evidence, commitments) {
                Ok(()) => self.log(LedgerEventV1::ClaimCommitted { claim: claim.id() }),
                Err(why) => self.refuse("CommitClaim", why),
            },
            LedgerTxV1::CommitPipelineClaim { claim, evidence, stages } => match self.commit_pipeline_claim(claim, evidence, stages) {
                Ok(()) => self.log(LedgerEventV1::ClaimCommitted { claim: claim.id() }),
                Err(why) => self.refuse("CommitPipelineClaim", why),
            },
            LedgerTxV1::PanelCovered { claim } => {
                let daa = self.daa;
                match self.claims.get_mut(claim) {
                    Some(row) => {
                        let _ = row.life.apply(ClaimEventV1::Tally { daa, state: TallyStateV1::Covered });
                    }
                    None => self.refuse("PanelCovered", "no such claim"),
                }
            }
            LedgerTxV1::FileProof { accuser, claim, proof } => self.file_proof(accuser, claim, proof),
            LedgerTxV1::FileDemand { demander, claim, stage, position } => self.file_demand(demander, claim, *stage, *position),
            LedgerTxV1::Respond { claim, stage, position, bytes } => self.respond(claim, *stage, *position, bytes),
        }
    }

    fn known_descriptor(&self, descriptor: &Digest) -> Result<KernelDescriptorV1, String> {
        self.known.iter().find(|d| d.digest() == *descriptor).cloned().ok_or_else(|| "a kernel this binary does not implement".into())
    }

    fn register_class(
        &mut self,
        descriptor: &Digest,
        program_bytes: &[u8],
        plan: &VerificationPlanV1,
        params: &MapParams,
        network: Digest,
        ruleset: Digest,
    ) -> Result<Digest, String> {
        let d = self.known_descriptor(descriptor)?;
        let program = TirProgramV1::decode_canonical(program_bytes).map_err(|e| format!("program: {e}"))?;
        let root = program_root_v1(program_bytes);
        check_plan_v1(&self.schedule, &d, &program, root, plan, self.daa).map_err(|o| o.to_string())?;
        let nodes: u64 = program.occurrences().iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u64).sum();
        let bounds = public_prosecution_complete_v1(&d, plan, nodes, &ProfileMaterialV1::kernel_route(true), &self.policy.prosecution)
            .map_err(|g| format!("not publicly prosecutable: {g:?}"))?;
        let param_commitments = ParamCommitmentsV1::of(params);
        let binding = ModelKernelBindingV1 {
            descriptor_digest: d.digest(),
            plan_root: plan.root(),
            program_root: root,
            artifact_root: param_commitments.root(),
            tokenizer_or_input_schema_root: [0; 64],
            task_output_schema: [0; 64],
            context_and_state_policy: ContextPolicyV1 { max_positions: plan.max_positions },
        };
        let class = binding.class_binding_id();
        self.classes.insert(
            class,
            ClassRowV1 {
                descriptor: d,
                program_bytes: program_bytes.to_vec(),
                program,
                plan: plan.clone(),
                params: params.clone(),
                param_commitments,
                network_domain: network,
                ruleset_digest: ruleset,
                bounds,
            },
        );
        Ok(class)
    }

    #[allow(clippy::too_many_arguments)]
    fn register_pipeline_class(
        &mut self,
        descriptor: &Digest,
        pipeline_bytes: &[u8],
        program_bytes: &[Vec<u8>],
        plan: &PipelinePlanV1,
        params: &[MapParams],
        decode: Option<DecodeRuleV1>,
        network: Digest,
        ruleset: Digest,
    ) -> Result<Digest, String> {
        let d = self.known_descriptor(descriptor)?;
        let programs = program_bytes
            .iter()
            .map(|b| TirProgramV2::decode_canonical(b).map_err(|e| format!("program: {e}")))
            .collect::<Result<Vec<_>, _>>()?;
        let pipeline = TirPipelineV1::decode_canonical(pipeline_bytes, &programs).map_err(|e| format!("pipeline: {e}"))?;
        check_pipeline_plan_v1(&self.schedule, &d, &pipeline, &programs, plan, self.daa).map_err(|o| o.to_string())?;
        let bounds = public_pipeline_prosecution_complete_v1(
            &d,
            plan,
            &pipeline,
            &programs,
            &ProfileMaterialV1::kernel_route(true),
            &self.policy.prosecution,
        )
        .map_err(|g| format!("not publicly prosecutable: {g:?}"))?;
        if params.len() != programs.len() {
            return Err("one artifact per program".into());
        }
        if stream_stage(&pipeline).is_some() != decode.is_some() {
            return Err("a stream stage's output needs a decode rule; a pipeline without one has none".into());
        }
        let param_commitments: Vec<ParamCommitmentsV1> = params.iter().map(ParamCommitmentsV1::of).collect();
        let binding = PipelineClassV1 {
            descriptor_digest: d.digest(),
            pipeline_root: pipeline_root_v1(&pipeline, &programs),
            plan_root: plan.root(),
            artifact_roots: param_commitments.iter().map(ParamCommitmentsV1::root).collect(),
            decode,
        };
        let class = binding.class_binding_id();
        self.pipeline_classes.insert(
            class,
            PipelineClassRowV1 {
                descriptor: d,
                pipeline_bytes: pipeline_bytes.to_vec(),
                pipeline,
                program_bytes: program_bytes.to_vec(),
                programs,
                plan: plan.clone(),
                params: params.to_vec(),
                param_commitments,
                binding,
                network_domain: network,
                ruleset_digest: ruleset,
                bounds,
            },
        );
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

    /// Reserve a new claim's collateral and start its lifecycle.
    fn admit(&mut self, id: Digest, producer: Digest, class: Digest, job: Digest, body: ClaimBodyV1) -> Result<(), String> {
        if self.claims.contains_key(&id) {
            return Err("an exact duplicate claim".into());
        }
        let need = self.policy.claim_collateral;
        let bond = self.bonds.get_mut(&producer).ok_or("the producer bond is not registered")?;
        if bond.exit_requested.is_some() {
            return Err("the producer bond is exiting".into());
        }
        if bond.free() < need {
            return Err(format!("{} free collateral, the claim needs {need} (no double use)", bond.free()));
        }
        bond.reserved += need;
        let mut life = ClaimLifecycleV1::new(LifecyclePolicyV1 {
            check_window_daa: self.policy.check_window_daa,
            challenge_window_daa: self.policy.challenge_window_daa,
        });
        let daa = self.daa;
        let _ = life.apply(ClaimEventV1::BindChallenge { anchor_daa: daa });
        let _ = life.apply(ClaimEventV1::StartChecking { daa });
        // The commitments are on chain from inclusion: the retention obligation for Final is met by the ledger itself.
        let _ = life.apply(ClaimEventV1::RetentionMet);
        let mut s = keyed(b"misaka-palw/kernel/ledger-beacon/v1");
        s.update(&id).update(&daa.to_le_bytes());
        self.claims.insert(
            id,
            ClaimRowV1 {
                producer,
                class_binding_id: class,
                job_id: job,
                body,
                committed_daa: daa,
                beacon: finish(s),
                life,
                reserved: need,
                liability_until: None,
                convicted: false,
                rewarded: false,
            },
        );
        Ok(())
    }

    fn commit_claim(
        &mut self,
        claim: &KernelClaimV1,
        evidence: &VerificationEvidenceV1,
        commitments: &[Vec<Vec<Digest>>],
    ) -> Result<(), String> {
        let job = self.jobs.get(&claim.job_id).ok_or("no such job")?;
        let class = self.classes.get(&job.class_binding_id).ok_or("no such class")?;
        if let Some(f) = binding_fault_v1(job, claim, evidence, class.program.token_bound) {
            return Err(format!("binding fault {f:?}"));
        }
        if evidence.header != class.header(job.class_binding_id) {
            return Err(format!("binding fault {:?}", BindingFaultV1::WrongClass));
        }
        // The trace commitments are carried with the claim (they bound every later opening): they must be the evidence's.
        if EvidenceV1::new(commitments.to_vec()).root() != evidence.trace_root {
            return Err("the carried trace commitments are not the evidence's trace root".into());
        }
        let id = claim.id();
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
            beacon: [0; 64],
        };
        FreshVerifierV1::from_public_bytes(&record.to_bytes(), &self.known, class.header(job.class_binding_id))
            .and_then(|v| v.structure())
            .map_err(|why| format!("malformed evidence: {why}"))?;
        let body = ClaimBodyV1::Program { claim: claim.clone(), evidence: evidence.clone(), commitments: commitments.to_vec() };
        let class_id = job.class_binding_id;
        self.admit(id, claim.producer_bond, class_id, claim.job_id, body)
    }

    fn commit_pipeline_claim(
        &mut self,
        claim: &PipelineClaimV1,
        evidence: &PipelineEvidenceV1,
        stages: &[StageCommitmentsV1],
    ) -> Result<(), String> {
        let job = self.pipeline_jobs.get(&claim.job_id).ok_or("no such job")?;
        let class = self.pipeline_classes.get(&job.class_binding_id).ok_or("no such class")?;
        use BindingFaultV1 as F;
        let fault = |f: F| Err(format!("binding fault {f:?}"));
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
            return Err("binding fault WrongOutput".into());
        }
        if stages.len() != evidence.stages.len()
            || stages.iter().zip(&evidence.stages).any(|(s, e)| s.evidence().root() != e.trace_root)
        {
            return Err("the carried trace commitments are not the evidence's trace roots".into());
        }
        let id = claim.id();
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
        FreshPipelineVerifierV1::from_public_bytes(&record.to_bytes(), &self.known, h, &class.binding)
            .and_then(|v| v.structure())
            .map_err(|why| format!("malformed evidence: {why}"))?;
        let body = ClaimBodyV1::Pipeline { claim: claim.clone(), evidence: evidence.clone(), stages: stages.to_vec() };
        let class_id = job.class_binding_id;
        self.admit(id, claim.producer_bond, class_id, claim.job_id, body)
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
            beacon: row.beacon,
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
            beacon: row.beacon,
        };
        Some((record, class.header(row.class_binding_id), class.binding.clone()))
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
                FreshPipelineVerifierV1::from_public_bytes(&record.to_bytes(), &self.known, header, &binding)?.try_proof(bytes)
            }
            _ => Err("a filing for another kind of claim".into()),
        }
    }

    fn file_proof(&mut self, accuser: &Digest, claim: &Digest, proof: &ProsecutionV1) {
        let fee = self.policy.dismissed_proof_fee;
        match self.bonds.get(accuser) {
            None => return self.refuse("FileProof", "the accuser is not a registered bond"),
            Some(b) if b.free() < fee => {
                return self.refuse("FileProof", "the accuser's free collateral does not cover the filing fee");
            }
            Some(_) => {}
        }
        let Some(row) = self.claims.get(claim) else { return self.refuse("FileProof", "no such claim") };
        if row.convicted {
            return self.log(LedgerEventV1::Duplicate { claim: *claim });
        }
        let is_final = matches!(row.life.state, ClaimStateV1::Final { .. });
        let bounds = self.bounds_of(&row.class_binding_id);
        let verdict = if is_final && row.liability_until.is_none_or(|until| self.daa > until) {
            Err("past the liability horizon".to_string())
        } else if row.reserved == 0 {
            Err("nothing is reserved any more".to_string())
        } else if let Some(why) = bounds.and_then(|b| oversized(proof, &b)) {
            Err(why)
        } else {
            self.adjudicate(claim, proof)
        };
        match verdict {
            Err(why) => {
                let b = self.bonds.get_mut(accuser).expect("checked");
                b.collateral -= fee;
                self.burned += fee;
                self.log(LedgerEventV1::ProofDismissed { claim: *claim, accuser: *accuser, why, fee })
            }
            Ok(()) => self.convict(claim, accuser, is_final),
        }
    }

    fn convict(&mut self, claim: &Digest, accuser: &Digest, post_final: bool) {
        let daa = self.daa;
        let row = self.claims.get_mut(claim).expect("checked");
        let slashed = row.reserved;
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
            b.reserved = b.reserved.saturating_sub(slashed);
            b.collateral = b.collateral.saturating_sub(slashed);
        }
        let reward = slashed * self.policy.accuser_reward_permille as u64 / 1000;
        if let Some(a) = self.bonds.get_mut(accuser) {
            a.credits += reward;
        }
        self.burned += slashed - reward;
        self.log(LedgerEventV1::Convicted { claim: *claim, accuser: *accuser, slashed, accuser_reward: reward, post_final });
        self.settle_demands_moot(claim);
    }

    /// Every open demand on a convicted claim is moot: its bonds return. No session survives to pre-empt anything.
    fn settle_demands_moot(&mut self, claim: &Digest) {
        let keys: Vec<_> = self.demands.keys().filter(|(c, _, _)| c == claim).copied().collect();
        let mut refunded = 0u32;
        for k in keys {
            if let Some(d) = self.demands.remove(&k) {
                for (bond, amount) in d.demanders {
                    if let Some(b) = self.bonds.get_mut(&bond) {
                        b.reserved = b.reserved.saturating_sub(amount);
                    }
                    refunded += 1;
                }
            }
        }
        if refunded > 0 {
            self.log(LedgerEventV1::DemandsMoot { claim: *claim, refunded });
        }
    }

    fn file_demand(&mut self, demander: &Digest, claim: &Digest, stage: u8, position: u32) {
        let daa = self.daa;
        let Some(row) = self.claims.get(claim) else { return self.refuse("FileDemand", "no such claim") };
        if row.terminal_for_demands() {
            return self.refuse("FileDemand", "the claim is already decided");
        }
        let is_final = matches!(row.life.state, ClaimStateV1::Final { .. });
        // Before Final, only while the challenge window is open (so no demand extends Final past window + deadline); after Final,
        // within the liability horizon (material for a post-Final proof), without touching the lifecycle.
        let window_open = match &row.life.state {
            ClaimStateV1::Final { .. } => row.liability_until.is_some_and(|u| daa <= u),
            s => demand_window_open(s, daa),
        };
        if !window_open {
            return self.refuse("FileDemand", "the challenge window is closed");
        }
        if row.body.position(stage, position).is_none() {
            return self.refuse("FileDemand", "the claim commits no such position");
        }
        let k = (*claim, stage, position);
        if self.served.contains_key(&k) {
            return self.refuse("FileDemand", "already served: it is public");
        }
        let need = self.policy.demand_bond;
        let Some(b) = self.bonds.get(demander) else { return self.refuse("FileDemand", "the demander is not a registered bond") };
        if b.exit_requested.is_some() || b.free() < need {
            return self.refuse("FileDemand", "the demander's free collateral does not cover the demand bond");
        }
        if let Some(d) = self.demands.get_mut(&k) {
            // Shared progress: a second demander joins the open demand rather than being refused by it.
            if !d.demanders.iter().any(|(b, _)| b == demander) {
                d.demanders.push((*demander, need));
                self.bonds.get_mut(demander).expect("checked").reserved += need;
            }
            return self.log(LedgerEventV1::DemandJoined { claim: *claim, stage, position });
        }
        // One session per stage position: the count is bounded by the claim's positions, and no demand can crowd out another.
        self.bonds.get_mut(demander).expect("checked").reserved += need;
        let deadline = daa + self.policy.court_deadline_daa;
        self.demands.insert(k, DemandRowV1 { demanders: vec![(*demander, need)], filed_daa: daa, deadline_daa: deadline, last: None });
        if !is_final {
            let row = self.claims.get_mut(claim).expect("checked");
            let _ = row.life.apply(ClaimEventV1::DisputeFiled { daa });
        }
        self.log(LedgerEventV1::DemandOpened { claim: *claim, stage, position, deadline });
    }

    fn close_demand(&mut self, k: DemandKeyV1) -> Option<DemandRowV1> {
        let d = self.demands.remove(&k)?;
        for (bond, amount) in &d.demanders {
            if let Some(b) = self.bonds.get_mut(bond) {
                b.reserved = b.reserved.saturating_sub(*amount);
            }
        }
        let daa = self.daa;
        if let Some(row) = self.claims.get_mut(&k.0)
            && matches!(row.life.state, ClaimStateV1::Disputed { .. })
        {
            let _ = row.life.apply(ClaimEventV1::CourtVerdict { daa, convicted: false });
        }
        Some(d)
    }

    fn respond(&mut self, claim: &Digest, stage: u8, position: u32, bytes: &[u8]) {
        let k = (*claim, stage, position);
        if !self.demands.contains_key(&k) {
            return self.refuse("Respond", "no open demand for this position");
        }
        let row = self.claims.get(claim).expect("a demand names a committed claim");
        let limit = self.bounds_of(&row.class_binding_id).map(|b| b.max_response_bytes).unwrap_or(0);
        let (values, inputs) = row.body.position(stage, position).expect("a demand names a committed position");
        let verdict =
            if bytes.len() as u128 > limit { Err("oversized") } else { classify_position_response_v1(values, inputs, bytes) };
        match verdict {
            Ok(served) => {
                self.served.insert(k, served);
                self.close_demand(k);
                self.log(LedgerEventV1::Served { claim: *claim, stage, position });
            }
            Err(class) => {
                if let Some(d) = self.demands.get_mut(&k) {
                    d.last = Some(class);
                }
                self.log(LedgerEventV1::ResponseRejected { claim: *claim, stage, position, class });
            }
        }
    }

    /// Deadlines, windows, Final, liability release.
    fn tick(&mut self) {
        let daa = self.daa;
        // Demands past their deadline: the producer's availability default.
        let due: Vec<_> = self.demands.iter().filter(|(_, d)| daa >= d.deadline_daa).map(|(k, d)| (*k, d.last)).collect();
        for ((claim, stage, position), last) in due {
            let Some(d) = self.demands.remove(&(claim, stage, position)) else { continue };
            let penalty = self.claims.get(&claim).map(|r| r.reserved.min(self.policy.default_penalty)).unwrap_or(0);
            // The penalty goes to the demanders, equally; their bonds return.
            let share = if d.demanders.is_empty() { 0 } else { penalty / d.demanders.len() as u64 };
            for (bond, amount) in &d.demanders {
                if let Some(b) = self.bonds.get_mut(bond) {
                    b.reserved = b.reserved.saturating_sub(*amount);
                    b.credits += share;
                }
            }
            self.burned += penalty - share * d.demanders.len() as u64;
            if let Some(row) = self.claims.get_mut(&claim) {
                let producer = row.producer;
                let was_final = matches!(row.life.state, ClaimStateV1::Final { .. });
                row.reserved -= penalty;
                if !was_final {
                    let _ = row.life.apply(ClaimEventV1::MaterialUnavailable { daa, producer_defaulted: true });
                }
                if let Some(b) = self.bonds.get_mut(&producer) {
                    b.reserved = b.reserved.saturating_sub(penalty);
                    b.collateral = b.collateral.saturating_sub(penalty);
                }
            }
            self.log(LedgerEventV1::ProducerDefault { claim, stage, position, last, penalty });
            // An unavailable claim never finalizes; its other open demands are moot.
            self.settle_demands_moot(&claim);
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
                    if let Some(b) = self.bonds.get_mut(&producer) {
                        b.credits += reward;
                    }
                    self.log(LedgerEventV1::Final { claim: id, reward });
                }
                (b, ClaimStateV1::TimedOut { .. }) if !matches!(b, ClaimStateV1::TimedOut { .. }) => {
                    self.release(&id, producer, reserved);
                    self.log(LedgerEventV1::TimedOut { claim: id });
                }
                (ClaimStateV1::Unavailable { .. }, _) if reserved > 0 => {
                    self.release(&id, producer, reserved);
                }
                (ClaimStateV1::Final { .. }, _) if reserved > 0 && liability.is_some_and(|u| daa > u) => {
                    self.release(&id, producer, reserved);
                    self.log(LedgerEventV1::Released { claim: id });
                }
                _ => {}
            }
        }
    }

    fn release(&mut self, claim: &Digest, producer: Digest, amount: u64) {
        if let Some(row) = self.claims.get_mut(claim) {
            row.reserved = 0;
        }
        if let Some(b) = self.bonds.get_mut(&producer) {
            b.reserved = b.reserved.saturating_sub(amount);
        }
    }
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
/// `court_deadline_daa`, so Final is never later than `window end + court deadline`, however many demands are filed.
pub fn demand_window_open(state: &ClaimStateV1, daa: u64) -> bool {
    match state {
        ClaimStateV1::Checking { deadline_daa, .. } => daa <= *deadline_daa,
        ClaimStateV1::ProbabilisticPass { window_end_daa, .. } => daa < *window_end_daa,
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
    /// The outsider's own randomness for its probabilistic checks (never the claim's public beacon, which a producer may have
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

/// One stage's public material for a court: served values first, then the source; the class's public artifact.
struct StageMaterial<'b> {
    o: &'b OutsiderV1<'b>,
    stage: u8,
    params: &'b MapParams,
}

impl MaterialV1 for StageMaterial<'_> {
    fn node_value(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
        self.o.node(self.stage, p, s, n)
    }
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        self.params.tensors.get(&(index, layer)).cloned()
    }
    fn stage_input(&self, k: u16, position: u32) -> Option<Tensor> {
        self.o.input(self.stage, position, k)
    }
}

impl OutsiderV1<'_> {
    fn node(&self, stage: u8, p: u32, s: u16, n: u16) -> Option<Tensor> {
        if let Some(sp) = self.ledger.served.get(&(self.claim, stage, p)) {
            return sp.values.get(s as usize)?.get(n as usize)?.decode().ok();
        }
        self.material.node(stage, p, s, n)
    }

    fn input(&self, stage: u8, p: u32, k: u16) -> Option<Tensor> {
        if let Some(sp) = self.ledger.served.get(&(self.claim, stage, p)) {
            return sp.inputs.get(k as usize)?.decode().ok();
        }
        self.material.input(stage, p, k)
    }

    /// Every committed value and input, authenticated against its commitment: the stage positions any is missing from.
    fn missing(&self, body: &ClaimBodyV1) -> Vec<(u8, u32)> {
        let ok = |t: Option<Tensor>, c: &Digest| t.is_some_and(|t| tensor_commitment(&t) == *c);
        let mut out = Vec::new();
        for (stage, positions) in body.stages() {
            for p in 0..positions {
                let (values, inputs) = body.position(stage, p).expect("listed");
                let nodes_ok = values
                    .iter()
                    .enumerate()
                    .all(|(s, occ)| occ.iter().enumerate().all(|(n, c)| ok(self.node(stage, p, s as u16, n as u16), c)));
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
        let missing = self.missing(&row.body);
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
        let material = StageMaterial { o: self, stage: 0, params: &class.params };
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
        let fresh = FreshPipelineVerifierV1::from_public_bytes(&record.to_bytes(), &l.known, header, &binding)?;
        let mats: Vec<StageMaterial<'_>> = class
            .pipeline
            .stages
            .iter()
            .enumerate()
            .map(|(si, st)| StageMaterial { o: self, stage: si as u8, params: &class.params[st.program as usize] })
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
}
