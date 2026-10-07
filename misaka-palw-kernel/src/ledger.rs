//! **An in-process chain for the kernel route: the consensus rules public prosecution relies on, as a deterministic fold.**
//!
//! [`KernelLedgerV1`] is a state that [`KernelLedgerV1::apply_block`] advances by blocks of [`LedgerTxV1`]. It is the local
//! harness for the consensus properties a library cannot show alone (ADR-0173, RFC-0015 §1.1): it is not wired into the node,
//! and the real-chain drill stays an external gate. Every rule below reads only ledger state — registered classes (program,
//! plan, public artifact), posted jobs, committed claims (evidence object and trace commitments) and material served in answer
//! to demands — so every adjudication is something any node replaying the blocks recomputes.
//!
//! * **Collateral** (category 19): a claim reserves `claim_collateral` of its producer bond's FREE collateral (no double use);
//!   the reservation lasts until the claim's liability horizon ends; an exiting bond backs nothing new and withdraws only after
//!   its delay with nothing reserved.
//! * **Inclusion refusals** (categories 5–7, 12): a claim whose evidence names another job, class, input (a borrowed trace),
//!   length or generation budget, or whose trace commitments are not the evidence's, is refused at inclusion — objectively,
//!   from public objects.
//! * **Direct proofs** (categories 1–4, 7, 11, 13, 16): a kernel fault proof or a decode fault is adjudicated in the block that
//!   carries it by the same courts any node runs, from ledger state alone. It is never refused because a demand or another
//!   session is open: open sessions on a convicted claim settle as moot and their bonds return (no pre-emption).
//! * **Demands** (categories 10, 14, 15): any bond may demand one committed POSITION (every node value of it); joining an open
//!   demand for the same position shares its progress. One session per position, so every position is demandable at once and
//!   no set of demanders (a producer's friends included) can starve another's: one round of demands then one direct proof is
//!   every prosecution's whole path. The response is bounded and classified (served / malformed / wrong bytes / wrong root /
//!   fake opening / partial / oversized); silence or non-serving past the deadline is the producer's availability default — a
//!   fixed penalty, never the fraud slash. A bond's open demands are limited by its free collateral, not by a count.
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

use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{MapParams, Tensor};

use crate::check::check_plan_v1;
use crate::descriptor::{ContextPolicyV1, KernelDescriptorV1, KernelScheduleV1, ModelKernelBindingV1};
use crate::evidence::{EvidenceHeaderV1, VerificationEvidenceV1};
use crate::gate::{ProsecutionBoundsV1, ProsecutionPolicyV1, public_prosecution_complete_v1};
use crate::hash::{Digest, finish, id, keyed};
use crate::job::{BindingFaultV1, DecodeFaultV1, KernelClaimV1, KernelJobV1, binding_fault_v1, verify_decode_fault_v1};
use crate::lifecycle::{ClaimEventV1, ClaimLifecycleV1, ClaimStateV1, LifecyclePolicyV1};
use crate::plan::VerificationPlanV1;
use crate::public::{
    FreshVerifierV1, ProfileMaterialV1, PublicClaimRecordV1, TensorWireV1, classify_position_response_v1, program_root_v1,
};
use crate::receipt::TallyStateV1;
use crate::trace::{EvidenceV1, ParamCommitmentsV1};

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

/// A registered class: everything public a court reads about it.
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

#[derive(Clone, Debug)]
pub struct ClaimRowV1 {
    pub claim: KernelClaimV1,
    pub class_binding_id: Digest,
    pub evidence: VerificationEvidenceV1,
    pub commitments: Vec<Vec<Vec<Digest>>>,
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
}

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
    PostJob {
        job: KernelJobV1,
    },
    CommitClaim {
        claim: KernelClaimV1,
        evidence: VerificationEvidenceV1,
        commitments: Vec<Vec<Vec<Digest>>>,
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
    /// A demand for every committed value of one position.
    FileDemand {
        demander: Digest,
        claim: Digest,
        position: u32,
    },
    /// Anyone (the producer, a DA provider) answers a position demand: borsh [`crate::public::PositionResponseV1`].
    Respond {
        claim: Digest,
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
    DemandOpened { claim: Digest, position: u32, deadline: u64 },
    DemandJoined { claim: Digest, position: u32 },
    Served { claim: Digest, position: u32 },
    ResponseRejected { claim: Digest, position: u32, class: &'static str },
    ProducerDefault { claim: Digest, position: u32, last: Option<&'static str>, penalty: u64 },
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
    pub jobs: BTreeMap<Digest, KernelJobV1>,
    pub claims: BTreeMap<Digest, ClaimRowV1>,
    pub demands: BTreeMap<(Digest, u32), DemandRowV1>,
    /// Positions served in answer to demands, `[occurrence][node]`: public from then on.
    pub served: BTreeMap<(Digest, u32), Vec<Vec<TensorWireV1>>>,
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
            jobs: BTreeMap::new(),
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
            LedgerTxV1::CommitClaim { claim, evidence, commitments } => match self.commit_claim(claim, evidence, commitments) {
                Ok(()) => self.log(LedgerEventV1::ClaimCommitted { claim: claim.id() }),
                Err(why) => self.refuse("CommitClaim", why),
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
            LedgerTxV1::FileDemand { demander, claim, position } => self.file_demand(demander, claim, *position),
            LedgerTxV1::Respond { claim, position, bytes } => self.respond(claim, *position, bytes),
        }
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
        let d = self.known.iter().find(|d| d.digest() == *descriptor).cloned().ok_or("a kernel this binary does not implement")?;
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
        if self.claims.contains_key(&id) {
            return Err("an exact duplicate claim".into());
        }
        let need = self.policy.claim_collateral;
        let bond = self.bonds.get_mut(&claim.producer_bond).ok_or("the producer bond is not registered")?;
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
                claim: claim.clone(),
                class_binding_id: job.class_binding_id,
                evidence: evidence.clone(),
                commitments: commitments.to_vec(),
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

    /// The public record of a claim, assembled from ledger state only (what a court and a fresh verifier read).
    pub fn public_record(&self, claim: &Digest) -> Option<(PublicClaimRecordV1, EvidenceHeaderV1)> {
        let row = self.claims.get(claim)?;
        let class = self.classes.get(&row.class_binding_id)?;
        let job = self.jobs.get(&row.claim.job_id)?;
        let record = PublicClaimRecordV1 {
            claim_id: *claim,
            program_bytes: class.program_bytes.clone(),
            plan: class.plan.clone(),
            evidence: row.evidence.clone(),
            trace_commitments: row.commitments.clone(),
            param_commitments: class.param_commitments.by_instance.iter().map(|((j, l), d)| (*j, *l, *d)).collect(),
            tokens: row.claim.stream(job),
            beacon: row.beacon,
        };
        Some((record, class.header(row.class_binding_id)))
    }

    /// **The court**, over ledger state only.
    fn adjudicate(&self, claim: &Digest, proof: &ProsecutionV1) -> Result<(), String> {
        let row = self.claims.get(claim).ok_or("no such claim")?;
        let class = self.classes.get(&row.class_binding_id).ok_or("no such class")?;
        match proof {
            ProsecutionV1::Kernel(bytes) => {
                let (record, header) = self.public_record(claim).ok_or("no public record")?;
                let court = FreshVerifierV1::from_public_bytes(&record.to_bytes(), &self.known, header)?;
                court.try_proof(bytes).map(|_| ()).map_err(|d| format!("{d:?}"))
            }
            ProsecutionV1::Decode(fault) => {
                let job = self.jobs.get(&row.claim.job_id).ok_or("no such job")?;
                let trace = EvidenceV1::new(row.commitments.clone());
                verify_decode_fault_v1(job, &row.claim, &trace, class.logits_at(), fault).map(|_| ()).map_err(|d| format!("{d:?}"))
            }
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
        let bounds = self.classes.get(&row.class_binding_id).map(|c| c.bounds);
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
        let producer = row.claim.producer_bond;
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
        let keys: Vec<_> = self.demands.keys().filter(|(c, _)| c == claim).copied().collect();
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

    fn file_demand(&mut self, demander: &Digest, claim: &Digest, position: u32) {
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
        if position as usize >= row.commitments.len() {
            return self.refuse("FileDemand", "the claim commits no such position");
        }
        let k = (*claim, position);
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
            return self.log(LedgerEventV1::DemandJoined { claim: *claim, position });
        }
        // One session per position: the count is bounded by the claim's positions, and no demand can crowd out another.
        self.bonds.get_mut(demander).expect("checked").reserved += need;
        let deadline = daa + self.policy.court_deadline_daa;
        self.demands.insert(k, DemandRowV1 { demanders: vec![(*demander, need)], filed_daa: daa, deadline_daa: deadline, last: None });
        if !is_final {
            let row = self.claims.get_mut(claim).expect("checked");
            let _ = row.life.apply(ClaimEventV1::DisputeFiled { daa });
        }
        self.log(LedgerEventV1::DemandOpened { claim: *claim, position, deadline });
    }

    fn close_demand(&mut self, claim: &Digest, k: (Digest, u32)) -> Option<DemandRowV1> {
        let d = self.demands.remove(&k)?;
        for (bond, amount) in &d.demanders {
            if let Some(b) = self.bonds.get_mut(bond) {
                b.reserved = b.reserved.saturating_sub(*amount);
            }
        }
        let daa = self.daa;
        if let Some(row) = self.claims.get_mut(claim)
            && matches!(row.life.state, ClaimStateV1::Disputed { .. })
        {
            let _ = row.life.apply(ClaimEventV1::CourtVerdict { daa, convicted: false });
        }
        Some(d)
    }

    fn respond(&mut self, claim: &Digest, position: u32, bytes: &[u8]) {
        let k = (*claim, position);
        if !self.demands.contains_key(&k) {
            return self.refuse("Respond", "no open demand for this position");
        }
        let row = self.claims.get(claim).expect("a demand names a committed claim");
        let limit = self.classes.get(&row.class_binding_id).map(|c| c.bounds.max_response_bytes).unwrap_or(0);
        let verdict = if bytes.len() as u128 > limit {
            Err("oversized")
        } else {
            classify_position_response_v1(&row.commitments[position as usize], bytes)
        };
        match verdict {
            Ok(values) => {
                self.served.insert(k, values);
                self.close_demand(claim, k);
                self.log(LedgerEventV1::Served { claim: *claim, position });
            }
            Err(class) => {
                if let Some(d) = self.demands.get_mut(&k) {
                    d.last = Some(class);
                }
                self.log(LedgerEventV1::ResponseRejected { claim: *claim, position, class });
            }
        }
    }

    /// Deadlines, windows, Final, liability release.
    fn tick(&mut self) {
        let daa = self.daa;
        // Demands past their deadline: the producer's availability default.
        let due: Vec<_> = self.demands.iter().filter(|(_, d)| daa >= d.deadline_daa).map(|(k, d)| (*k, d.last)).collect();
        for ((claim, position), last) in due {
            let Some(d) = self.demands.remove(&(claim, position)) else { continue };
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
                let producer = row.claim.producer_bond;
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
            self.log(LedgerEventV1::ProducerDefault { claim, position, last, penalty });
            // An unavailable claim never finalizes; its other open demands are moot.
            self.settle_demands_moot(&claim);
        }
        let ids: Vec<Digest> = self.claims.keys().copied().collect();
        for id in ids {
            let (producer, before, after, reserved, liability) = {
                let row = self.claims.get_mut(&id).expect("listed");
                let before = row.life.state.clone();
                let _ = row.life.apply(ClaimEventV1::Tick { daa });
                (row.claim.producer_bond, before, row.life.state.clone(), row.reserved, row.liability_until)
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
        ProsecutionV1::Kernel(bytes) => (bytes.len() as u128, b.max_filing_bytes as u128),
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

/// **A fresh outsider's view of a claim, from ledger state and whatever material is served** — the verifier a new node builds
/// after the claim was published. `material(p, s, n)` is any public source of node values (the producer's server, a DA
/// provider); values served on chain in answer to demands are read from the ledger first.
pub struct OutsiderV1<'a> {
    pub ledger: &'a KernelLedgerV1,
    pub claim: Digest,
    pub material: &'a dyn Fn(u32, u16, u16) -> Option<Tensor>,
}

/// What an outsider concludes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutsiderFindingV1 {
    /// Every relation, the decode and the bindings check.
    Clean,
    /// A proof to file.
    Prosecute(ProsecutionV1),
    /// Positions some committed value of which nobody serves: demand them (one round, all at once).
    Demand(Vec<u32>),
}

impl OutsiderV1<'_> {
    fn node(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
        if let Some(values) = self.ledger.served.get(&(self.claim, p)) {
            return values.get(s as usize)?.get(n as usize)?.decode().ok();
        }
        (self.material)(p, s, n)
    }

    /// **Check the claim.** The verifier is built from the ledger's public record bytes and the binary's own descriptors.
    pub fn check(&self) -> Result<OutsiderFindingV1, String> {
        use crate::verify::{MaterialV1, ScopeV1, ScopeVerdictV1};
        let l = self.ledger;
        let row = l.claims.get(&self.claim).ok_or("no such claim")?;
        let class = l.classes.get(&row.class_binding_id).ok_or("no such class")?;
        let job = l.jobs.get(&row.claim.job_id).ok_or("no such job")?;
        let (record, header) = l.public_record(&self.claim).ok_or("no record")?;
        let fresh = FreshVerifierV1::from_public_bytes(&record.to_bytes(), &l.known, header)?;
        // Every committed value, authenticated against its commitment: the positions any value is missing from are demanded
        // together, in one round.
        let missing: Vec<u32> = (0..row.commitments.len() as u32)
            .filter(|&p| {
                row.commitments[p as usize].iter().enumerate().any(|(s, occ)| {
                    occ.iter()
                        .enumerate()
                        .any(|(n, c)| self.node(p, s as u16, n as u16).is_none_or(|t| crate::trace::tensor_commitment(&t) != *c))
                })
            })
            .collect();
        if !missing.is_empty() {
            return Ok(OutsiderFindingV1::Demand(missing));
        }
        let value = |p: u32, s: u16, n: u16| self.node(p, s, n).ok_or(format!("({p}, {s}, {n}) vanished"));
        // The decode relation: each delivered token against the committed logits that select it.
        let (post, logits) = class.logits_at();
        for r in 0..row.claim.generated.len() {
            let t = value(row.claim.select_position(job, r), post, logits)?;
            if job.decode.select(&t) != Some(row.claim.generated[r]) {
                return Ok(OutsiderFindingV1::Prosecute(ProsecutionV1::Decode(DecodeFaultV1 {
                    index: r as u32,
                    logits: TensorWireV1::of(&t),
                })));
            }
        }
        // Then every relation the plan covers.
        struct Public<'b> {
            o: &'b OutsiderV1<'b>,
            params: &'b MapParams,
        }
        impl MaterialV1 for Public<'_> {
            fn node_value(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
                self.o.node(p, s, n)
            }
            fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
                self.params.tensors.get(&(index, layer)).cloned()
            }
        }
        Ok(match fresh.check(&Public { o: self, params: &class.params }, &ScopeV1::WholeClaim) {
            ScopeVerdictV1::Pass { .. } => OutsiderFindingV1::Clean,
            ScopeVerdictV1::Fault(p) => {
                OutsiderFindingV1::Prosecute(ProsecutionV1::Kernel(crate::public::FaultProofWireV1::of(&p).to_bytes()))
            }
            ScopeVerdictV1::Unavailable { what } => return Err(format!("unavailable with every value in hand: {what}")),
            ScopeVerdictV1::EvidenceMalformed { why } => return Err(format!("malformed evidence on chain: {why}")),
            ScopeVerdictV1::Inconsistent { why } => return Err(format!("verifier inconsistency: {why}")),
        })
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
        let logits = |n: usize| TensorWireV1 { dtype: 0, shape: vec![n as u64], bytes: vec![0; n] };
        assert!(oversized(&ProsecutionV1::Decode(DecodeFaultV1 { index: 0, logits: logits(50) }), &b).is_none());
        assert!(oversized(&ProsecutionV1::Decode(DecodeFaultV1 { index: 0, logits: logits(51) }), &b).is_some());
    }
}
