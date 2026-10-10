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
//!   free collateral, not by a count. **A default never erases a provable fraud** (C4 F-C4R3-02): the penalty is split like a
//!   slash (the demanders take the accuser's share of it, the rest is burned — an OPV claim burns at least its policy's
//!   `default_burn_permille`), so a producer's own demander never recoups it; the rest of the reservation stays held until
//!   `default + liability_daa`, and a valid proof filed in that horizon convicts the defaulted claim and slashes it, paying the
//!   accuser the bounty it would have had without the default. Withholding stays a default, never fraud, when no valid proof
//!   ever arrives: the reservation is released at the horizon.
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
//!   remaining reservation (the reward was already paid), an availability outcome that is never the fraud conviction. A proof
//!   past the horizon, or against a claim that ended without passing (timed out), is REFUSED — no court runs and no fee is
//!   charged: a true proof never costs its filer the dismissal fee.
//! * **Accuser seals** (GAP-R7): an accuser may seal its proof first ([`KernelRouteObjectV1::SealProof`],
//!   `seal = proof_seal_v1(claim, accuser, proof)`) and file it once the seal is `claim_seal_delay_daa` old. At a conviction the
//!   bounty goes to the bond holding the EARLIEST such seal of the convicting proof's exact bytes — whoever filed them — so a
//!   copyist who lifts a sealed proof from its public carrier and gets it included first pays the sealer, not itself. An
//!   unsealed filing counts as sealed in its own block (it loses to any older seal of the same bytes). A colluding producer can
//!   still self-convict with a proof of its own and recoup the bounty; the reservation relations price that
//!   ([`LedgerPolicyV1::validate`], `OpvPolicyV1::required_reservation`).
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
pub use crate::route::{
    AuthV1, KernelRefusalV1, KernelRouteObjectV1, ProsecutionV1, RefusalKindV1, SaltedCommitV1, claim_seal_v1, claim_seal_v2,
    proof_digest_v1, proof_seal_of_digest_v1, proof_seal_v1,
};
use crate::settle::{SettlementInstructionV1, SettlementKindV1};
use crate::trace::{EvidenceV1, ParamCommitmentsV1, derived_mask_v1, tensor_commitment};
use crate::verify::MaterialV1;

/// RFC-0004 Part II: the typed roots on the ledger (a child module, so it applies the route's own private rules).
#[path = "spec/ledger_impl.rs"]
mod spec_impl;

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
    /// What an availability default forfeits, never more than the reservation: split like a slash — the demanders take
    /// `accuser_reward_permille` of it (an OPV claim's demanders at most `1000 − default_burn_permille`), the rest is burned.
    pub default_penalty: u64,
    /// Paid to the producer at Final — **out of the job's escrow, never issued** (GAP-5, the user's ruling: option A, user-pays
    /// escrow): every `PostJob` / `PostPipelineJob` reserves this much of its POSTER's free collateral as the job's escrow, and the
    /// job's first Final pays it out (the poster's bond is debited exactly what the producer is paid). An escrow pays at most once,
    /// and one no claim can still use is returned after `job_escrow_ttl_daa`.
    pub claim_reward: u64,
    /// GAP-5: what posting a job costs its poster, non-refundably (burned) — beside the escrow — so a self-posted job always costs
    /// something its own reward cannot pay back.
    pub job_fee: u64,
    /// GAP-5: an escrow is returned to its poster once this long has passed since the job was posted, no claim holds the job and no
    /// producer's seal of it is live (a sealed producer is never left working for an escrow that left).
    pub job_escrow_ttl_daa: u64,
    /// The most court / classification / inclusion-check runs one block may trigger; an object past it is refused (dropped).
    pub max_adjudications_per_block: u32,
    /// **Prosecution room that is always there** (C4 F-C4R3-05, round 2): this share (permille) of every block's
    /// `max_adjudications_per_block` and byte/court work only a `FileProof` may spend — admissions (claims, registrations, pipeline jobs) and responses use
    /// the rest — so no flood of claims can leave an outsider without a court run in the block that carries its proof. Below 1000.
    pub prosecution_reserve_permille: u16,
    /// The most shared structural admission and court work one block may trigger. The stored field
    /// name remains `court_work`; claim byte tariffs and worst-case public courts spend this same budget.
    pub max_court_work_per_block: u64,
    /// A claim commits only over its producer's seal at least this old (≥ 1: a seal in the same block as the reveal proves nothing).
    pub claim_seal_delay_daa: u64,
    /// An unrevealed seal is dropped after this long (bounds the state junk seals can occupy).
    pub seal_ttl_daa: u64,
    /// **What a claim seal holds of its producer's free collateral until it is revealed** (OPV-BOOT's sealed-source beacon v3): returned
    /// when the claim commits over it, FORFEITED (slashed, burned) when it expires unrevealed — withholding a seal is never free.
    pub seal_deposit: u64,
    pub prosecution: ProsecutionPolicyV1,
}

impl LedgerPolicyV1 {
    /// The court runs of every block only a `FileProof` may spend (C4 F-C4R3-05, round 2).
    pub const fn prosecution_reserved_runs(&self) -> u32 {
        (self.max_adjudications_per_block as u64 * self.prosecution_reserve_permille as u64 / 1000) as u32
    }

    /// Ordinary admission cannot spend the work reserved for public proofs. Use u128 before
    /// scaling: policies near u64::MAX must not overflow or silently eliminate the reserve.
    pub fn admission_work_limit_v1(&self) -> u64 {
        let reserved = self.max_court_work_per_block as u128 * self.prosecution_reserve_permille as u128 / 1000;
        self.max_court_work_per_block.saturating_sub(reserved.min(u64::MAX as u128) as u64)
    }

    /// A registered court must fit the room left after admissions fill their share. With no
    /// configured reserve the policy explicitly supplies only the full-block ceiling.
    pub fn guaranteed_proof_work_v1(&self) -> u64 {
        if self.prosecution_reserve_permille == 0 {
            self.max_court_work_per_block
        } else {
            self.max_court_work_per_block - self.admission_work_limit_v1()
        }
    }

    /// **What a dismissed filing that ran a court forfeits** (C4 F-C4R4-05): `dismissed_proof_fee` for each
    /// `1 / max_adjudications_per_block` share of the block's court work it was charged (its class's declared worst court), at least
    /// one fee. Every class shares ONE per-block court budget, so a junk filing against the heaviest class must cost what the runs it
    /// crowds out would have cost: saturating a block costs about `max_adjudications_per_block × dismissed_proof_fee` whichever classes
    /// the junk names, never one fee. A true proof still pays nothing.
    pub fn dismissal_fee_v1(&self, court_work: u64) -> u64 {
        let shares = (court_work as u128 * self.max_adjudications_per_block as u128)
            .div_ceil(self.max_court_work_per_block.max(1) as u128)
            .max(1);
        (self.dismissed_proof_fee as u128).saturating_mul(shares).min(u64::MAX as u128) as u64
    }

    /// The relations among the timings and amounts every rule below relies on.
    pub fn validate(&self) -> Result<(), String> {
        let p = self;
        let checks: [(bool, &str); 18] = [
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
            // GAP-R7: a colluding producer can always convict itself first with a proof of its own and recoup the accuser's share of
            // its slash; what it then loses must still exceed the Final reward it was after.
            (
                (p.claim_collateral as u128) * (1000u128.saturating_sub(p.accuser_reward_permille as u128))
                    > (p.claim_reward as u128) * 1000,
                "a self-convicted producer (recouping the accuser's share) still loses more than the Final reward",
            ),
            (p.default_penalty <= p.claim_collateral, "a default never takes more than the reservation"),
            (p.demand_bond > 0 && p.dismissed_proof_fee > 0, "demands and filings are not free"),
            // GAP-5: a self-posted job pays itself its own escrow back; only a burned fee makes it cost anything.
            (p.job_fee > 0, "posting a job is not free (a self-posted job must cost something its reward cannot recover)"),
            (p.job_escrow_ttl_daa >= p.seal_ttl_daa, "a job's escrow outlives a producer's seal of it"),
            // A slash and a default both burn part of what they take: a 100% share would let a producer's own accuser or demander
            // cycle it back for nothing (C4 F-C4R3-02).
            (p.accuser_reward_permille < 1000, "the accuser's share is a share, and part of every slash and default is burned"),
            (p.claim_collateral > 0, "a claim reserves collateral"),
            (p.max_adjudications_per_block > 0, "a block may adjudicate"),
            (p.prosecution_reserve_permille < 1000, "the prosecution reserve leaves admissions a share of the block"),
            (p.max_court_work_per_block > 0, "a block may run a court"),
            (p.claim_seal_delay_daa > 0, "a seal precedes its reveal by at least one block"),
            (p.seal_ttl_daa >= p.claim_seal_delay_daa, "a seal lives long enough to be revealed"),
            (p.seal_deposit > 0, "a claim seal is bonded (withholding a sealed reveal costs a forfeit)"),
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
    pub fn admission_work_v1(&self) -> Result<u64, String> {
        crate::gate::program_claim_admission_work_v1(&self.bounds, self.program_bytes.len(), &self.program, &self.plan)
    }

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
    pub fn admission_work_v1(&self) -> Result<u64, String> {
        let bytes = borsh::object_length(&(&self.pipeline_bytes, &self.program_bytes, &self.plan, &self.param_commitments))
            .map_err(|e| format!("pipeline schema: {e}"))?;
        crate::gate::claim_admission_work_v1(&self.bounds, bytes as u128)
    }

    pub fn header(&self, class_binding_id: Digest) -> PipelineHeaderV1 {
        PipelineHeaderV1 { network_domain: self.network_domain, ruleset_digest: self.ruleset_digest, class_binding_id }
    }
}

/// What a committed claim is: a single program's or a pipeline's.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ClaimBodyV1 {
    Program {
        claim: KernelClaimV1,
        evidence: VerificationEvidenceV1,
        commitments: Vec<Vec<Vec<Digest>>>,
    } = 0,
    Pipeline {
        claim: PipelineClaimV1,
        evidence: PipelineEvidenceV1,
        stages: Vec<StageCommitmentsV1>,
    } = 1,
    /// **K2-TIR-v4**: a segmented claim — the evidence object and one root per segment; node commitments are served material.
    Segmented {
        claim: KernelClaimV1,
        evidence: crate::seg::SegmentedEvidenceV2,
        segment_roots: Vec<Digest>,
    } = 2,
    /// RFC-0004 Part II: a typed claim (memory, retrieval, composite).
    Spec(Box<crate::spec::SpecClaimBodyV1>) = 3,
}

impl ClaimBodyV1 {
    /// Stage `stage`'s committed node values and inputs at `position` (a single program is stage 0, with no inputs).
    pub fn position(&self, stage: u8, position: u32) -> Option<(&[Vec<Digest>], &[Digest])> {
        match self {
            Self::Program { commitments, .. } if stage == 0 => Some((commitments.get(position as usize)?, &[])),
            Self::Program { .. } | Self::Segmented { .. } => None,
            Self::Pipeline { stages, .. } => {
                let s = stages.get(stage as usize)?;
                let inputs = s.inputs.get(position as usize).map(Vec::as_slice).unwrap_or(&[]);
                Some((s.commitments.get(position as usize)?, inputs))
            }
            Self::Spec(b) => b.position(stage, position),
        }
    }

    /// `(stage, positions)` of every stage.
    pub fn stages(&self) -> Vec<(u8, u32)> {
        match self {
            Self::Program { commitments, .. } => vec![(0, commitments.len() as u32)],
            Self::Pipeline { stages, .. } => stages.iter().enumerate().map(|(i, s)| (i as u8, s.commitments.len() as u32)).collect(),
            Self::Segmented { evidence, .. } => vec![(0, evidence.positions)],
            Self::Spec(b) => b.stages(),
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
    /// The DAA of the seal this claim was revealed over (kept on the row: the sealed-source beacon orders sources by it).
    pub sealed_daa: u64,
}

/// **GAP-5: a job's escrow** — what its poster reserved to fund the job's Final reward.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct JobEscrowRowV1 {
    pub poster: Digest,
    pub amount: u64,
    pub posted_daa: u64,
}

/// **A claim seal that expired unrevealed past `palw_panel_free_v1`** (table 26; key `(job, producer, sealed_daa)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ForfeitedSealRowV1 {
    pub seal: Digest,
    pub forfeited_daa: u64,
}

/// **One claim seal as the sealed-source beacon v3 reads it** ([`KernelLedgerV1::claim_beacon_seals_v1`]): live (neither field),
/// revealed with its salt (`revealed`: claim id, reveal DAA, salt) or forfeited (`forfeited_daa`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimBeaconSealV1 {
    pub job: Digest,
    pub producer: Digest,
    pub seal: Digest,
    pub sealed_daa: u64,
    pub revealed: Option<(Digest, u64, Digest)>,
    pub forfeited_daa: Option<u64>,
    /// The bond that posted the seal's job (`job_posters`; `None` for a job posted below the fence) — the beacon's consumer.
    pub poster: Option<Digest>,
}

/// **The v3 beacon's window against the seal TTL**: every in-window reveal is legal only if `2·W ≤ seal_ttl_daa` (a seal made at
/// the start of a seal window `[S, S + W)` must still be revealable at the end of its reveal window `[S + W, S + 2W)`). OPV-BOOT's
/// policy validation asks this with its `beacon_window_slots`.
pub fn seal_ttl_admits_beacon_window_v1(policy: &LedgerPolicyV1, beacon_window_daa: u64) -> bool {
    beacon_window_daa.saturating_mul(2) <= policy.seal_ttl_daa
}

/// An unrevealed seal: the claim seal and the DAA it was carried at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SealRowV1 {
    pub seal: Digest,
    pub daa: u64,
    /// What the seal holds of its sealer's free collateral (a claim seal: `seal_deposit`; a proof seal: 0).
    pub deposit: u64,
}

impl ClaimRowV1 {
    pub(crate) fn terminal_for_demands(&self) -> bool {
        self.convicted || matches!(self.life.state, ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. })
    }

    /// Whether this claim still holds its job (live or Final): only a failed claim frees the job for another.
    pub fn holds_job(&self) -> bool {
        !self.terminal_for_demands()
    }
}

/// The response classes a rejected response can have, as stored codes (`1 + index`).
pub const RESPONSE_CLASS_NAMES_V1: [&str; 6] = ["malformed", "wrong_bytes", "wrong_root", "fake_opening", "partial", "oversized"];

pub(crate) fn response_class_code(name: &str) -> u8 {
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
    /// A conformance candidate, with no execution or economic rights.
    ConformanceClassRegistered {
        class: Digest,
    } = 32,
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
    /// GAP-R7: an accuser sealed a proof against a claim.
    ProofSealed {
        claim: Digest,
        accuser: Digest,
    } = 21,
    /// GAP-5: an escrow no claim could still use went back to its poster.
    JobEscrowReturned {
        job: Digest,
        poster: Digest,
        amount: u64,
    } = 22,
    /// A claim seal expired unrevealed: its deposit was forfeited (burned).
    SealForfeited {
        job: Digest,
        producer: Digest,
        forfeited: u64,
    } = 23,
    /// A Final claim's liability horizon ended with no conviction: the bonds of the positions served on demand were burned.
    ServedDemandBondsBurned {
        claim: Digest,
        burned: u64,
    } = 24,
    /// **M\*-49 (`palw_verifier_pay_v1`)**: a drawn slot was paid its check fee out of the poster's escrow (on its attestation, or on
    /// the claim's conviction or producer default before the deadline).
    CheckFeePaid {
        claim: Digest,
        verifier: Digest,
        amount: u64,
    } = 25,
    /// **O2 (`palw_verifier_pay_v1`)**: a pre-Final default's demanders' share — `held` (still reserved on the producer's bond), then
    /// `joined_the_pool` (a conviction slashed it into the one 49 % pool) or `paid_to_demanders` (the horizon passed unconvicted).
    DefaultShareHeld {
        claim: Digest,
        held: u64,
        outcome: &'static str,
    } = 26,
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
    /// Shared admission byte work plus public court work (the persisted wire field keeps its name).
    pub court_work: u64,
}

impl BlockBudgetV1 {
    /// Shared by the kernel ledger and the node's onboarding fold. Refusals consume neither a
    /// run nor work; only public proof/refutation paths may use the reserved share.
    pub fn charged_v1(self, policy: &LedgerPolicyV1, work: u64, may_spend_reserve: bool) -> Option<Self> {
        let runs = if may_spend_reserve {
            policy.max_adjudications_per_block
        } else {
            policy.max_adjudications_per_block.saturating_sub(policy.prosecution_reserved_runs())
        };
        let work_limit = if may_spend_reserve { policy.max_court_work_per_block } else { policy.admission_work_limit_v1() };
        let court_work = self.court_work.checked_add(work)?;
        if self.adjudications >= runs || court_work > work_limit {
            return None;
        }
        Some(Self { adjudications: self.adjudications.checked_add(1)?, court_work })
    }
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

/// Maximum live preparation records per bonded actor; promotion releases a slot.
pub const MAX_CONFORMANCE_CLASSES_PER_BOND_V1: usize = 8;

#[derive(Clone, Debug)]
pub struct KernelLedgerV1 {
    pub policy: LedgerPolicyV1,
    pub schedule: KernelScheduleV1,
    /// Descriptors this binary implements (never bytes from a transaction).
    pub known: Vec<KernelDescriptorV1>,
    pub daa: u64,
    pub bonds: BTreeMap<Digest, BondRowV1>,
    pub classes: BTreeMap<Digest, ClassRowV1>,
    /// Conformance-only OPV identities, keyed class id: (preparing bond, checked metadata).
    /// No execution or payment path reads this table. Promotion removes the candidate.
    pub conformance_classes: BTreeMap<Digest, (Digest, ClassRowV1)>,
    /// Derived index, never committed: candidate admission reads only this bond's bounded set.
    pub(crate) conformance_class_owners: BTreeMap<Digest, BTreeSet<Digest>>,
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
    /// GAP-R7: accusers' proof seals, keyed `(claim, accuser)` (see [`KernelRouteObjectV1::SealProof`]).
    pub proof_seals: BTreeMap<(Digest, Digest), SealRowV1>,
    /// GAP-5: `job → its poster's escrow` — reserved at posting, paid out (once) as the job's Final reward, or returned.
    pub job_escrows: BTreeMap<Digest, JobEscrowRowV1>,
    /// **The demand bonds of SERVED positions, awaiting their fate** (K2S's producer-side DA griefing, G14-R4): a position served on
    /// chain keeps its demanders' bonds reserved; they are refunded the moment the claim is convicted, defaults or times out, and
    /// burned only when its liability horizon ends with no conviction — a true demand that leads to a conviction is never penalised.
    pub served_demands: BTreeMap<DemandKeyV1, Vec<(Digest, u64)>>,
    /// **OPV-BOOT GAP-B1a: `claim id → the salt its seal was made with`** (table 25), written by a salted reveal
    /// ([`KernelRouteObjectV1::CommitClaimSalted`]) past `palw_panel_free_v1` and never removed (like the claim rows): the
    /// sealed-source beacon v3 mixes it. Empty below the fence (the root is then the historical one).
    pub claim_beacon_salts: BTreeMap<Digest, Digest>,
    /// **OPV-BOOT GAP-B1a: the claim seals that expired unrevealed** (table 26), keyed `(job, producer, sealed_daa)`: written past
    /// `palw_panel_free_v1` for a seal accepted at or after the fence, and kept, so a withheld seal stays in the v3 beacon's mix and
    /// vetoes it rather than silently dropping out of it (SOUND SG-01a(i)). Every row cost a forfeited `seal_deposit`.
    pub forfeited_claim_seals: BTreeMap<(Digest, Digest, u64), ForfeitedSealRowV1>,
    /// **C4R4 F-C4R4-08: `job → the bond that posted it`** (table 18), written at every job post past `palw_panel_free_v1` and kept
    /// as long as the job row (never removed): the escrow that names the poster is spent at the job's Final, and the sealed-source
    /// beacon v3's distinct-consumer rule must read the same poster at every later tip. Empty below the fence.
    pub job_posters: BTreeMap<Digest, Digest>,
    /// Cumulative amount burned (derived from the settlement instructions; kept as a checksum).
    pub burned: u64,
    /// RFC-0015: the OPV policy, classes and claim rows. Dormant (no policy) = the historical ledger, root included.
    pub opv: OpvStateV1,
    /// K2-TIR-v4: jobs whose prompt is posted in tiles, and which tiles are posted (ledger table 20).
    pub tiled_jobs: BTreeMap<Digest, crate::seg::TiledJobRowV1>,
    /// K2-TIR-v4: the progress of every position demand of a segmented claim, and the served positions (ledger table 21).
    pub seg_progress: BTreeMap<DemandKeyV1, crate::seg_da::SegProgressV1>,
    /// RFC-0004 Part II: typed-root classes, their jobs and the memory lines (tables 22–24; empty = the historical ledger).
    pub typed: crate::spec::TypedStateV1,
    /// **RFC-0009 §4.2 (lane DA16): claims whose material obligation the consumer moved to bonded providers.** Consumer-derived from
    /// the consumer's own rooted rows and re-injected before every object and tick (like the attested set's source); NOT part of this
    /// state, its rows or its root. A demand on such a claim that nobody answers by its deadline is a
    /// [`LedgerEventV1::ProviderLiableDefault`] — the providers' failure, never the producer's; and [`Self::provider_lapse`] voids it.
    pub provider_liable: BTreeSet<Digest>,
    /// **`palw_verifier_pay_v1`'s terms** (M\*-49 and O2), consumer-injected before every object and tick like the attested set; NOT
    /// part of this state or its root. `None`: the fence is absent (every rule below it is the historical one).
    pub verifier_pay_policy: Option<crate::verifier_pay::VerifierPayPolicyV1>,
    /// **Table 27**: check-fee escrows, claim draws and held default shares (empty below the fence; in the root only once non-empty).
    pub verifier_pay: crate::verifier_pay::VerifierPayTableV1,
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

pub(crate) fn settle(out: &mut Vec<LedgerEventV1>, bond: Digest, amount: u64, kind: SettlementKindV1, claim: Option<Digest>) {
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
            conformance_classes: BTreeMap::new(),
            conformance_class_owners: BTreeMap::new(),
            pipeline_classes: BTreeMap::new(),
            jobs: BTreeMap::new(),
            pipeline_jobs: BTreeMap::new(),
            claims: BTreeMap::new(),
            demands: BTreeMap::new(),
            served: BTreeMap::new(),
            attested_artifacts: BTreeSet::new(),
            job_claims: BTreeMap::new(),
            seals: BTreeMap::new(),
            proof_seals: BTreeMap::new(),
            job_escrows: BTreeMap::new(),
            served_demands: BTreeMap::new(),
            claim_beacon_salts: BTreeMap::new(),
            forfeited_claim_seals: BTreeMap::new(),
            job_posters: BTreeMap::new(),
            burned: 0,
            opv: OpvStateV1::default(),
            tiled_jobs: BTreeMap::new(),
            seg_progress: BTreeMap::new(),
            typed: crate::spec::TypedStateV1::default(),
            provider_liable: BTreeSet::new(),
            verifier_pay_policy: None,
            verifier_pay: BTreeMap::new(),
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
            KernelRouteObjectV1::CompleteModelConformanceV2 { .. }
            | KernelRouteObjectV1::BindModelArtifactV2 { .. }
            | KernelRouteObjectV1::RefuteModelArtifactV2 { .. }
            | KernelRouteObjectV1::RegisterModelConformanceClassV2 { .. } => {
                return Err(KernelRefusalV1::rule(name, "model artifact statements require the authenticated consensus consumer"));
            }
            KernelRouteObjectV1::RegisterConformanceClass { descriptor, program_bytes, plan, param_commitments } => {
                let class = self.register_class_impl(
                    VerificationModeV1::OptimisticPublicVerification,
                    descriptor,
                    program_bytes,
                    plan,
                    param_commitments,
                    Some(auth.signer_bond),
                )?;
                out.push(LedgerEventV1::ConformanceClassRegistered { class });
            }
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
                self.job_escrow_affordable(name, &auth.signer_bond)?;
                self.jobs.insert(id, job.clone());
                out.push(LedgerEventV1::JobPosted { job: id });
                self.open_job_escrow(&auth.signer_bond, id, &mut out);
            }
            KernelRouteObjectV1::PostPipelineJob { job } => {
                let id = job.id();
                if self.pipeline_jobs.contains_key(&id) {
                    return Err(KernelRefusalV1::rule(name, "the job is already posted"));
                }
                self.job_escrow_affordable(name, &auth.signer_bond)?;
                self.charge(name, 0)?;
                self.post_pipeline_job(job).map_err(|why| KernelRefusalV1::rule(name, why))?;
                self.pipeline_jobs.insert(id, job.clone());
                out.push(LedgerEventV1::JobPosted { job: id });
                self.open_job_escrow(&auth.signer_bond, id, &mut out);
            }
            KernelRouteObjectV1::CommitClaim { claim, evidence, commitments } => {
                self.commit_claim(claim, evidence, commitments, None, &mut out)?;
                out.push(LedgerEventV1::ClaimCommitted { claim: claim.id() });
            }
            KernelRouteObjectV1::CommitPipelineClaim { claim, evidence, stages } => {
                self.commit_pipeline_claim(claim, evidence, stages, None, &mut out)?;
                out.push(LedgerEventV1::ClaimCommitted { claim: claim.id() });
            }
            // OPV-BOOT GAP-B1a: the salted reveal — refused below `palw_panel_free_v1` (A-2: no state), the commit's own rules above.
            KernelRouteObjectV1::CommitClaimSalted { salt, commit } => {
                if !self.salted_seals_in_force() {
                    return Err(KernelRefusalV1::rule(
                        name,
                        "a salted claim reveal is refused: palw_panel_free_v1 is not in force (claim seal v2)",
                    ));
                }
                let id = match commit {
                    SaltedCommitV1::Claim { claim, evidence, commitments } => {
                        self.commit_claim(claim, evidence, commitments, Some(salt), &mut out)?;
                        claim.id()
                    }
                    SaltedCommitV1::Pipeline { claim, evidence, stages } => {
                        self.commit_pipeline_claim(claim, evidence, stages, Some(salt), &mut out)?;
                        claim.id()
                    }
                    SaltedCommitV1::Spec { claim } => {
                        self.apply_salted_spec_claim(claim, salt, &mut out)?;
                        claim.id()
                    }
                    SaltedCommitV1::Segmented { claim, evidence, segment_roots } => {
                        self.commit_segmented_claim(claim, evidence, segment_roots, Some(salt), &mut out)?;
                        claim.id()
                    }
                };
                out.push(LedgerEventV1::ClaimCommitted { claim: id });
            }
            KernelRouteObjectV1::FileProof { accuser, claim, proof } => self.file_proof(accuser, claim, proof, &mut out)?,
            KernelRouteObjectV1::FileDemand { demander, claim, stage, position } => {
                self.file_demand(demander, claim, *stage, *position, &mut out)?
            }
            KernelRouteObjectV1::Respond { claim, stage, position, bytes } => {
                self.respond(&auth.signer_bond, claim, *stage, *position, bytes, &mut out)?
            }
            KernelRouteObjectV1::CommitSegmentedClaim { claim, evidence, segment_roots } => {
                self.commit_segmented_claim(claim, evidence, segment_roots, None, &mut out)?;
                out.push(LedgerEventV1::ClaimCommitted { claim: claim.id() });
            }
            KernelRouteObjectV1::PostTiledJob { job } => {
                // GAP-5 (user-pays escrow): a tiled job funds its Final reward and pays its fee exactly as `PostJob` does.
                self.job_escrow_affordable(name, &auth.signer_bond)?;
                self.post_tiled_job(job)?;
                out.push(LedgerEventV1::JobPosted { job: job.id() });
                self.open_job_escrow(&auth.signer_bond, job.id(), &mut out);
            }
            KernelRouteObjectV1::PostPromptTile { job, tile } => self.post_prompt_tile(job, tile)?,
            KernelRouteObjectV1::RequestExit { bond } => match self.bonds.get_mut(bond) {
                Some(b) if b.exit_requested.is_none() => {
                    b.exit_requested = Some(self.daa);
                    out.push(LedgerEventV1::ExitRequested { bond: *bond });
                }
                _ => return Err(KernelRefusalV1::rule(name, "no such bond, or already exiting")),
            },
            KernelRouteObjectV1::SealClaim { producer, job, seal } => {
                if !self.jobs.contains_key(job)
                    && !self.pipeline_jobs.contains_key(job)
                    && !self.tiled_jobs.contains_key(job)
                    && !self.typed.jobs.contains_key(job)
                {
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
                // A seal is bonded: `seal_deposit` of the producer's free collateral until it is revealed (forfeited if it expires). A
                // producer may re-seal (another output): the latest seal replaces the earlier and its clock restarts. Below the fence the
                // one deposit is kept. **Past `palw_panel_free_v1` (ECON F-ECON-1 / F-ECON-2, fix S1) the REPLACED seal is forfeited** at
                // its own position (table 26, its deposit burned) and the new seal is bonded afresh: a seal position once taken is
                // never withdrawn for free — re-sealing to stay live costs a deposit each time, and moving a mixed seal out of a beacon's
                // seal window leaves the old one in the mix as a veto, never a silent withdrawal.
                let held = self.seals.get(&(*job, *producer)).copied();
                let replaced = held.filter(|r| self.salted_seals_from().is_some_and(|at| r.daa >= at));
                let deposit = self.policy.seal_deposit;
                if held.is_none() || replaced.is_some() {
                    // (a forfeit of the replaced seal takes its deposit out of both the collateral and the reservation: free is unchanged)
                    if self.bonds[producer].free() < deposit {
                        return Err(KernelRefusalV1::rule(name, "the producer's free collateral does not cover the seal deposit"));
                    }
                    if let Some(row) = replaced {
                        self.seals.remove(&(*job, *producer));
                        self.forfeit_claim_seal(*job, *producer, row, &mut out);
                    }
                    self.bonds.get_mut(producer).expect("checked").reserved += deposit;
                    settle(&mut out, *producer, deposit, SettlementKindV1::ReserveSealDeposit, None);
                }
                let deposit = held.filter(|_| replaced.is_none()).map_or(deposit, |r| r.deposit);
                self.seals.insert((*job, *producer), SealRowV1 { seal: *seal, daa: self.daa, deposit });
                out.push(LedgerEventV1::ClaimSealed { job: *job, producer: *producer });
            }
            KernelRouteObjectV1::SealProof { accuser, claim, seal } => {
                const NAME: &str = "SealProof";
                let fee = self.policy.dismissed_proof_fee;
                match self.bonds.get(accuser) {
                    None => return Err(KernelRefusalV1::rule(NAME, "the accuser is not a registered bond")),
                    Some(b) if b.exit_requested.is_some() => return Err(KernelRefusalV1::rule(NAME, "the accuser bond is exiting")),
                    // A seal is a promise of a filing: the accuser must be able to stand behind it as a filer.
                    Some(b) if b.free() < fee => {
                        return Err(KernelRefusalV1::rule(NAME, "the accuser's free collateral does not cover the filing fee"));
                    }
                    Some(_) => {}
                }
                let row = self.claims.get(claim).ok_or_else(|| KernelRefusalV1::rule(NAME, "no such claim"))?;
                if row.convicted {
                    return Err(KernelRefusalV1::rule(NAME, "the claim is already convicted"));
                }
                self.proof_open(row).map_err(|why| KernelRefusalV1::rule(NAME, why))?;
                // One seal per (claim, accuser): a re-seal (another proof) replaces the earlier one and its clock restarts.
                if !self.proof_seals.contains_key(&(*claim, *accuser))
                    && self.proof_seals.range((*claim, [0; 64])..=(*claim, [u8::MAX; 64])).count()
                        >= crate::gate::MAX_PROOF_SEALS_PER_CLAIM_V1
                {
                    return Err(KernelRefusalV1::rule(
                        NAME,
                        "the claim's proof-seal metadata is full; a direct proof remains admissible",
                    ));
                }
                self.proof_seals.insert((*claim, *accuser), SealRowV1 { seal: *seal, daa: self.daa, deposit: 0 });
                out.push(LedgerEventV1::ProofSealed { claim: *claim, accuser: *accuser });
            }
            KernelRouteObjectV1::Spec { object } => self.apply_spec(object, auth, &mut out)?,
            KernelRouteObjectV1::Withdraw { bond } => {
                let (daa, delay) = (self.daa, self.policy.exit_delay_daa);
                match self.bonds.get(bond) {
                    Some(b) if b.exit_requested.is_some_and(|at| daa >= at + delay) && b.reserved == 0 => {
                        let amount = b.collateral;
                        self.bonds.remove(bond);
                        if let Some(classes) = self.conformance_class_owners.remove(bond) {
                            for class in classes {
                                self.conformance_classes.remove(&class);
                            }
                        }
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
            O::CommitSegmentedClaim { claim, .. } => Some((claim.producer_bond, "producer")),
            O::CommitClaimSalted { commit, .. } => Some((commit.producer(), "producer")),
            O::FileProof { accuser, .. } => Some((*accuser, "accuser")),
            O::FileDemand { demander, .. } => Some((*demander, "demander")),
            O::RequestExit { bond } | O::Withdraw { bond } => Some((*bond, "bond")),
            O::SealClaim { producer, .. } => Some((*producer, "producer")),
            O::SealProof { accuser, .. } => Some((*accuser, "accuser")),
            O::Spec { object } => object.named_actor(),
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

    /// **GAP-5: a job's escrow and fee** — `claim_reward` reserved (the job's Final reward) and `job_fee` burned — must be covered by
    /// its poster's free collateral (checked before any mutation).
    fn job_escrow_affordable(&self, name: &'static str, poster: &Digest) -> Result<(), KernelRefusalV1> {
        let need = self.policy.claim_reward.saturating_add(self.policy.job_fee).saturating_add(self.check_fee_escrow_v1());
        match self.bonds.get(poster) {
            Some(b) if b.free() >= need => Ok(()),
            _ => Err(KernelRefusalV1::rule(
                name,
                "the poster's free collateral does not cover the job's escrow and fee (the escrow funds the job's Final reward, GAP-5)",
            )),
        }
    }

    /// **GAP-5: open a job's escrow** — reserve `claim_reward` of the poster's free collateral against the job and burn `job_fee`.
    /// Every job post opens its escrow here (single-program, pipeline and typed jobs alike), so past `palw_panel_free_v1` the poster
    /// is recorded here too (`job_posters`, C4R4 F-C4R4-08): it outlives the escrow.
    fn open_job_escrow(&mut self, poster: &Digest, job: Digest, out: &mut Vec<LedgerEventV1>) {
        if self.salted_seals_in_force() {
            self.job_posters.insert(job, *poster);
        }
        let (amount, fee) = (self.policy.claim_reward, self.policy.job_fee);
        if let Some(b) = self.bonds.get_mut(poster) {
            b.reserved += amount;
            b.collateral -= fee;
        }
        self.burned += fee;
        if amount > 0 {
            self.job_escrows.insert(job, JobEscrowRowV1 { poster: *poster, amount, posted_daa: self.daa });
        }
        settle(out, *poster, amount, SettlementKindV1::ReserveJobEscrow, None);
        settle(out, *poster, fee, SettlementKindV1::JobFee, None);
        settle(out, *poster, fee, SettlementKindV1::Burn, None);
        // M*-49 (`palw_verifier_pay_v1`): the check fees of the job's drawn verifiers, escrowed beside the reward.
        self.open_check_fee_escrow_v1(poster, job, out);
    }

    /// **GAP-5: pay a Final out of its job's escrow** — the poster's bond is debited exactly what the producer is paid, and the escrow
    /// is spent (an escrow pays once). No escrow (already paid, or returned): no reward — nothing is ever issued.
    ///
    /// **ADR-0176 HOOK `budget-final` (lane BUDGET; not built here):** the reward is paid only within the R / F reservation the claim
    /// made at acceptance (`budget-accept`), re-checked here; the budget is not recovered before `d + W`, whatever the claim's fate.
    fn pay_from_job_escrow(&mut self, job: &Digest, claim: &Digest, producer: Digest, out: &mut Vec<LedgerEventV1>) -> u64 {
        let Some(row) = self.job_escrows.remove(job) else { return 0 };
        let collateral = self.bonds.get(&row.poster).map_or(0, |b| b.collateral);
        // Never instruct more than the poster's bond still holds (another subsystem may have slashed it since).
        let paid = row.amount.min(collateral);
        if let Some(b) = self.bonds.get_mut(&row.poster) {
            b.reserved = b.reserved.saturating_sub(row.amount);
            b.collateral -= paid;
        }
        settle(out, row.poster, paid, SettlementKindV1::PayJobEscrow, Some(*claim));
        settle(out, row.poster, row.amount - paid, SettlementKindV1::ReleaseJobEscrow, Some(*claim));
        settle(out, producer, paid, SettlementKindV1::FinalReward, Some(*claim));
        paid
    }

    /// **GAP-5: return the escrows no claim can still use**: past `job_escrow_ttl_daa`, the job held by no live claim and sealed by
    /// no producer. A job whose holder later fails keeps its escrow until this rule returns it.
    ///
    /// **A seal holds an escrow for one seal TTL past the escrow's, never longer** (C4 F-C4R4-03): a seal is a promise any bond may
    /// make on an unclaimed job, and a re-seal restarts its clock on the same deposit, so "no live seal" let a squatter keep the
    /// escrow — and, through `reserved`, the poster's whole bond exit — for as long as it re-sealed. A producer that sealed by the
    /// escrow's TTL has `seal_ttl_daa` to reveal (its seal expires then anyway).
    fn release_idle_job_escrows(&mut self, out: &mut Vec<LedgerEventV1>) {
        let ttl = self.policy.job_escrow_ttl_daa;
        let seal_ttl = self.policy.seal_ttl_daa;
        let idle: Vec<Digest> = self
            .job_escrows
            .iter()
            .filter(|(job, row)| {
                let expired = row.posted_daa.saturating_add(ttl);
                self.daa > expired
                    && !self.job_claims.get(*job).and_then(|c| self.claims.get(c)).is_some_and(ClaimRowV1::holds_job)
                    && (self.daa > expired.saturating_add(seal_ttl)
                        || self.seals.range((**job, [0u8; 64])..=(**job, [0xFFu8; 64])).next().is_none())
            })
            .map(|(job, _)| *job)
            .collect();
        for job in idle {
            let Some(row) = self.job_escrows.remove(&job) else { continue };
            if let Some(b) = self.bonds.get_mut(&row.poster) {
                b.reserved = b.reserved.saturating_sub(row.amount);
            }
            out.push(LedgerEventV1::JobEscrowReturned { job, poster: row.poster, amount: row.amount });
            settle(out, row.poster, row.amount, SettlementKindV1::ReleaseJobEscrow, None);
        }
    }

    /// Charge one adjudication (and `court_work`) to the block, or refuse the object as over budget. Nothing is charged on a refusal.
    pub(crate) fn charge(&mut self, name: &'static str, court_work: u64) -> Result<(), KernelRefusalV1> {
        let b = self.budget;
        let next = b.charged_v1(&self.policy, court_work, name == "FileProof");
        if next.is_none() {
            return Err(KernelRefusalV1::new(
                name,
                RefusalKindV1::OverBudget,
                format!(
                    "the block's adjudication budget is spent ({} runs, {} court work; {} runs are reserved for proofs)",
                    b.adjudications,
                    b.court_work,
                    self.policy.prosecution_reserved_runs()
                ),
            ));
        }
        self.budget = next.expect("checked");
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
        self.register_class_impl(mode, descriptor, program_bytes, plan, pc, None)
    }

    fn register_class_impl(
        &mut self,
        mode: VerificationModeV1,
        descriptor: &Digest,
        program_bytes: &[u8],
        plan: &VerificationPlanV1,
        pc: &ParamCommitmentsV1,
        candidate_owner: Option<Digest>,
    ) -> Result<Digest, KernelRefusalV1> {
        let name: &'static str = if candidate_owner.is_some() {
            "RegisterConformanceClass"
        } else if mode.is_optimistic() {
            "RegisterClassV2"
        } else {
            "RegisterClass"
        };
        let rule = |why: String| KernelRefusalV1::rule(name, why);
        let d = self.known_descriptor(descriptor).map_err(rule)?;
        if crate::descriptor::is_segmented_v1(&d) && !mode.is_optimistic() {
            return Err(rule(
                "a K2-TIR-v4 (segmented) class registers only under OptimisticPublicVerification: a Panel cannot cover a claim whose \
                 material is terabytes"
                    .into(),
            ));
        }
        let root = program_root_v1(program_bytes);
        let binding = single_class_binding_v1(d.digest(), program_bytes, plan, pc);
        // The mode is part of the class identity: the same program under another mode is another class (RFC-0015 §4.1).
        let class = class_id_for_mode_v1(&binding.class_binding_id(), mode);
        if self.classes.contains_key(&class) {
            // Re-registering would overwrite the row every court reads; the class id binds everything, so there is nothing to add.
            return Err(rule("the class is already registered".into()));
        }
        let opv = if mode.is_optimistic() { Some(self.opv_register_gate(name)?) } else { None };
        if let Some(owner) = candidate_owner {
            if self.conformance_classes.contains_key(&class) {
                return Err(rule("the conformance class is already prepared".into()));
            }
            if self.conformance_class_owners.get(&owner).is_some_and(|classes| classes.len() >= MAX_CONFORMANCE_CLASSES_PER_BOND_V1) {
                return Err(rule("the bond's conformance candidate limit is full".into()));
            }
        } else if mode.is_optimistic() {
            self.opv_class_admitted(name, &class)?;
        }
        if !self.attested_artifacts.contains(&binding.artifact_root) {
            return Err(rule("the artifact is not attested public by the consumer's registry".into()));
        }
        self.charge(name, 0)?;
        let program = TirProgramV1::decode_canonical(program_bytes).map_err(|e| rule(format!("program: {e}")))?;
        // K2-TIR-v5: the program's last two params are the job's input (`crate::seg_encoder`), never the artifact; one position; its
        // ranges proven with the inputs' intervals (the ids below the token bound, the count at most `L`).
        let artifact_params = if crate::descriptor::is_encoder_v1(&d) {
            let e = crate::seg_encoder::encoder_binding_v1(&program).map_err(rule)?;
            if plan.max_positions != 1 {
                return Err(rule("a K2-TIR-v5 (encoder) plan is of one position".into()));
            }
            crate::seg_encoder::prove_encoder_ranges_v1(&program, &e).map_err(|why| rule(format!("FRONTEND_REQUIRED: {why}")))?;
            crate::check::check_plan_with_v1(
                &self.schedule,
                &d,
                &program,
                root,
                plan,
                self.daa,
                crate::check::RangeRuleV1::ProvenByV2,
            )
            .map_err(|o| rule(o.to_string()))?;
            e.first_input as usize
        } else {
            check_plan_v1(&self.schedule, &d, &program, root, plan, self.daa).map_err(|o| rule(o.to_string()))?;
            program.params.len()
        };
        check_commitment_set(
            pc,
            &used_param_instances(&program, artifact_params),
            &declared_param_instances(&program.params[..artifact_params], program.schedule.layers.len()),
        )
        .map_err(rule)?;
        // The artifact is public by the consumer's attestation (checked above), not by a registrant's flag. PUBLIC_PROSECUTION_COMPLETE
        // is derived from code for every class of every mode; an OPV class has no Panel to fall back on, so there is no exception.
        // A K2-TIR-v4 plan is bounded per prosecution (`crate::gate::public_prosecution_complete_v4`).
        let bounds = crate::gate::class_prosecution_bounds_v1(
            &d,
            plan,
            &program,
            &ProfileMaterialV1::kernel_route(true),
            &self.policy.prosecution,
        )
        .map_err(|g| rule(format!("not publicly prosecutable: {g:?}")))?;
        self.check_court_work_v1(bounds.max_court_work).map_err(rule)?;
        if let Some(p) = opv {
            self.opv_class_economics(&p, &bounds).map_err(rule)?;
        }
        let row = ClassRowV1 {
            descriptor: d,
            program_bytes: program_bytes.to_vec(),
            program,
            plan: plan.clone(),
            param_commitments: pc.clone(),
            network_domain: self.policy.network_domain,
            ruleset_digest: self.policy.ruleset_digest,
            bounds,
        };
        self.check_admission_work_v1(row.admission_work_v1().map_err(rule)?).map_err(rule)?;
        if let Some(owner) = candidate_owner {
            // The run was charged before decoding. Candidates additionally consume the same
            // admission byte-work budget as claims, without spending a second run or the proof reserve.
            let work = row.admission_work_v1().map_err(rule)?;
            let total =
                self.budget.court_work.checked_add(work).filter(|w| *w <= self.policy.admission_work_limit_v1()).ok_or_else(|| {
                    KernelRefusalV1::new(name, RefusalKindV1::OverBudget, "the shared admission work budget is spent")
                })?;
            self.budget.court_work = total;
            self.conformance_classes.insert(class, (owner, row));
            self.conformance_class_owners.entry(owner).or_default().insert(class);
        } else {
            self.classes.insert(class, row);
            if let Some((owner, _)) = self.conformance_classes.remove(&class) {
                let classes = self.conformance_class_owners.get_mut(&owner).expect("candidate owner index");
                classes.remove(&class);
                if classes.is_empty() {
                    self.conformance_class_owners.remove(&owner);
                }
            }
            if mode.is_optimistic() {
                self.opv.classes.insert(class);
            }
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
        self.check_court_work_v1(bounds.max_court_work).map_err(rule)?;
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
        let row = PipelineClassRowV1 {
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
        };
        self.check_admission_work_v1(row.admission_work_v1().map_err(rule)?).map_err(rule)?;
        self.pipeline_classes.insert(class, row);
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
    pub(crate) fn admit(
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
        // C4 F-C4R3-05: an OPV claim's admission fee, non-refundable, from free collateral beside the reservation.
        let fee = if opv_row.is_some() { self.opv.policy.map_or(0, |p| p.economics.admission_fee) } else { 0 };
        if self.bonds.get(&producer).ok_or("the producer bond is not registered")?.exit_requested.is_some() {
            return Err("the producer bond is exiting".into());
        }
        // The seal's deposit is released by this very reveal: it counts as free here.
        let (seal_credit, sealed_daa) = self.seals.get(&(job, producer)).map_or((0, self.daa), |r| (r.deposit, r.daa));
        let bond = self.bonds.get_mut(&producer).expect("checked");
        if bond.free().saturating_add(seal_credit) < need.saturating_add(fee) {
            return Err(format!(
                "{} free collateral, the claim needs {need} and its admission fee {fee} (no double use)",
                bond.free()
            ));
        }
        // The seal is spent by its reveal: its deposit returns (OPV-BOOT's sealed-source beacon reads `sealed_daa` from the row).
        bond.reserved = bond.reserved.saturating_sub(seal_credit) + need;
        bond.collateral -= fee;
        self.burned += fee;
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
                sealed_daa,
            },
        );
        if let Some((_, row)) = opv_row {
            // An OPV claim holds its job from its first reveal: it is prosecutable by any bond from this block, so a junk claim
            // squatting the job is slashed (or defaults) at the squatter's cost, and a well-formed one is the job's answer.
            self.opv.claims.insert(id, row);
            self.opv.live.insert((producer, id));
            self.job_claims.insert(job, id);
        }
        // **ADR-0176 HOOK `budget-accept` (lane BUDGET, `palw_bond_budget_v1`; not built here).** Claim acceptance is where the
        // producer bond's Q/B/R/F budget over the common window W must reserve this claim: one unit of Q, and of R the reward the
        // claim can draw at Final (the job escrow's amount, GAP-5; for an OPV claim also `work_credit_per_claim`, which nothing
        // releases yet), plus any verifier bounty a later conviction could pay — refusing the commit when the budget is spent, as
        // `need` above refuses it when collateral is. Until the engine exists every reward fence stays refused (arming list, CODE).
        settle(out, producer, seal_credit, SettlementKindV1::ReleaseSealDeposit, Some(id));
        // S3 (`palw_verifier_pay_v1`): a revealed beacon-source seal returns its extra deposit.
        self.close_beacon_source_v1(&job, &producer, sealed_daa, false, out);
        settle(out, producer, need, SettlementKindV1::ReserveClaim, Some(id));
        settle(out, producer, fee, SettlementKindV1::AdmissionFee, Some(id));
        settle(out, producer, fee, SettlementKindV1::Burn, Some(id));
        Ok(())
    }

    /// A claim of an OPV class needs its producer's live-claim caps and free collateral to allow it: decided before the adjudication
    /// budget is spent and before any evidence is verified (a producer at its cap cannot make the ledger verify claims it must refuse).
    pub(crate) fn opv_claim_capacity(&self, class: &Digest, producer: &Digest, job: &Digest) -> Result<(), String> {
        if !self.opv.classes.contains(class) {
            return Ok(());
        }
        let (need, _) = self.opv_admission(producer)?;
        let fee = self.opv.policy.map_or(0, |p| p.economics.admission_fee);
        let credit = self.seals.get(&(*job, *producer)).map_or(0, |r| r.deposit);
        match self.bonds.get(producer) {
            Some(b) if b.free().saturating_add(credit) < need.saturating_add(fee) => {
                Err(format!("{} free collateral, the claim needs {need} and its admission fee {fee} (no double use)", b.free()))
            }
            _ => Ok(()),
        }
    }

    /// A claim of an OPV class commits only while the fence is reached (a cheap check; nothing is charged).
    pub(crate) fn opv_claim_gate(&self, class: &Digest) -> Result<(), String> {
        if self.opv.classes.contains(class) && !self.optimistic_allowed() {
            return Err("the palw_panel_free_v1 fence is not reached".into());
        }
        Ok(())
    }

    /// **May this claim be revealed now?** No other claim holds its job, its producer bond is ready, and the producer's seal of
    /// exactly this claim is at least `claim_seal_delay_daa` old.
    /// **Past `palw_panel_free_v1`** (the OPV policy's activation): every claim reveal of a seal accepted at or after the fence carries
    /// its seal's salt (claim seal v2), and the salt is kept for the sealed-source beacon v3. The DAA the fence is reached at, if
    /// this ledger has one.
    pub fn salted_seals_from(&self) -> Option<u64> {
        self.opv.policy.and_then(|p| p.activation_daa)
    }

    /// Whether the salted-seal rule is in force at the ledger's clock.
    pub fn salted_seals_in_force(&self) -> bool {
        self.salted_seals_from().is_some_and(|at| self.daa >= at)
    }

    /// The bond that posted `job`, past `palw_panel_free_v1` (C4R4 F-C4R4-08; `None`: posted below the fence, or unknown).
    pub fn job_poster(&self, job: &Digest) -> Option<Digest> {
        self.job_posters.get(job).copied()
    }

    /// The salt a claim's seal was made with (`None`: the claim was revealed unsalted, or is unknown).
    pub fn claim_beacon_salt(&self, claim: &Digest) -> Option<Digest> {
        self.claim_beacon_salts.get(claim).copied()
    }

    /// **Every claim seal the sealed-source beacon v3 can count**, in `(sealed_daa, seal)` order: the live seals (at their LATEST
    /// seal: a re-seal replaces the earlier one), the salted reveals (with the claim id, its reveal DAA and the salt) and the seals
    /// that expired unrevealed past the fence. A seal revealed unsalted (below the fence) is not a beacon seal and is not listed.
    pub fn claim_beacon_seals_v1(&self) -> Vec<ClaimBeaconSealV1> {
        let mut out: Vec<ClaimBeaconSealV1> = self
            .seals
            .iter()
            .map(|((job, producer), row)| ClaimBeaconSealV1 {
                job: *job,
                producer: *producer,
                seal: row.seal,
                sealed_daa: row.daa,
                revealed: None,
                forfeited_daa: None,
                poster: self.job_poster(job),
            })
            .collect();
        for (id, salt) in &self.claim_beacon_salts {
            if let Some(row) = self.claims.get(id) {
                out.push(ClaimBeaconSealV1 {
                    job: row.job_id,
                    producer: row.producer,
                    seal: claim_seal_v2(id, salt),
                    sealed_daa: row.sealed_daa,
                    revealed: Some((*id, row.committed_daa, *salt)),
                    forfeited_daa: None,
                    poster: self.job_poster(&row.job_id),
                });
            }
        }
        for ((job, producer, sealed_daa), row) in &self.forfeited_claim_seals {
            out.push(ClaimBeaconSealV1 {
                job: *job,
                producer: *producer,
                seal: row.seal,
                sealed_daa: *sealed_daa,
                revealed: None,
                forfeited_daa: Some(row.forfeited_daa),
                poster: self.job_poster(job),
            });
        }
        out.sort_by(|a, b| (a.sealed_daa, a.seal).cmp(&(b.sealed_daa, b.seal)));
        out
    }

    /// Whether `producer`'s live seal of `job` is the seal of claim `id` — `claim_seal_v1(id)` unsalted, `claim_seal_v2(id, salt)`
    /// salted — at least `claim_seal_delay_daa` old. Past `palw_panel_free_v1` a seal accepted at or after the fence opens only
    /// salted (OPV-BOOT GAP-B1a): an unsalted reveal of it would let a sealer choose, after seeing the honest salts, between
    /// "revealed but not a beacon source" and a veto.
    pub(crate) fn reveal_ready(&self, job: &Digest, producer: &Digest, id: &Digest, salt: Option<&Digest>) -> Result<(), String> {
        if self.job_claims.get(job).and_then(|c| self.claims.get(c)).is_some_and(ClaimRowV1::holds_job) {
            return Err("another claim already holds the job (one claim per job)".into());
        }
        match self.bonds.get(producer) {
            None => return Err("the producer bond is not registered".into()),
            Some(b) if b.exit_requested.is_some() => return Err("the producer bond is exiting".into()),
            Some(_) => {}
        }
        let expected = match salt {
            None => claim_seal_v1(id),
            Some(s) => claim_seal_v2(id, s),
        };
        match self.seals.get(&(*job, *producer)) {
            Some(row) if salt.is_none() && self.salted_seals_from().is_some_and(|at| row.daa >= at) => {
                Err("a seal accepted past palw_panel_free_v1 is revealed only with its salt (claim seal v2)".into())
            }
            Some(row) if row.seal == expected && row.daa.saturating_add(self.policy.claim_seal_delay_daa) <= self.daa => Ok(()),
            Some(row) if row.seal != expected && salt.is_some() => Err("the salt does not open the producer's seal of the job".into()),
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
        salt: Option<&Digest>,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "CommitClaim";
        let rule = |why: String| KernelRefusalV1::rule(NAME, why);
        let job = self.jobs.get(&claim.job_id).ok_or_else(|| rule("no such job".into()))?;
        let class = self.classes.get(&job.class_binding_id).ok_or_else(|| rule("no such class".into()))?;
        self.opv_claim_gate(&job.class_binding_id).map_err(rule)?;
        Self::check_claim_carrier_v1(NAME, &class.bounds, &(claim, evidence, commitments))?;
        if let Some(f) = binding_fault_v1(job, claim, evidence, class.program.token_bound) {
            return Err(rule(format!("binding fault {f:?}")));
        }
        if evidence.header != class.header(job.class_binding_id) {
            return Err(rule(format!("binding fault {:?}", BindingFaultV1::WrongClass)));
        }
        let id = claim.id();
        if self.claims.contains_key(&id) {
            return Err(rule("an exact duplicate claim".into()));
        }
        // Cheap objective checks first: an unsealed or unready reveal never spends the block's adjudication budget (C4 O-C4-14).
        self.reveal_ready(&claim.job_id, &claim.producer_bond, &id, salt).map_err(rule)?;
        self.opv_claim_capacity(&job.class_binding_id, &claim.producer_bond, &claim.job_id).map_err(rule)?;
        let work = class.admission_work_v1().map_err(rule)?;
        self.charge(NAME, work)?;
        // The trace commitments are carried with the claim (they bound every later opening): they must be the evidence's.
        if EvidenceV1::new(commitments.to_vec()).root() != evidence.trace_root {
            return Err(rule("the carried trace commitments are not the evidence's trace root".into()));
        }
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
        if let Some(salt) = salt {
            self.claim_beacon_salts.insert(id, *salt);
        }
        Ok(())
    }

    fn commit_pipeline_claim(
        &mut self,
        claim: &PipelineClaimV1,
        evidence: &PipelineEvidenceV1,
        stages: &[StageCommitmentsV1],
        salt: Option<&Digest>,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "CommitPipelineClaim";
        let rule = |why: String| KernelRefusalV1::rule(NAME, why);
        let job = self.pipeline_jobs.get(&claim.job_id).ok_or_else(|| rule("no such job".into()))?;
        let class = self.pipeline_classes.get(&job.class_binding_id).ok_or_else(|| rule("no such class".into()))?;
        self.opv_claim_gate(&job.class_binding_id).map_err(rule)?;
        Self::check_claim_carrier_v1(NAME, &class.bounds, &(claim, evidence, stages))?;
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
                if !crate::job::generation_length_matches_v1(job.max_new_tokens, claim.generated.len()) {
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
        let id = claim.id();
        if self.claims.contains_key(&id) {
            return Err(rule("an exact duplicate claim".into()));
        }
        // Cheap objective checks first: an unsealed or unready reveal never spends the block's adjudication budget (C4 O-C4-14).
        self.reveal_ready(&claim.job_id, &claim.producer_bond, &id, salt).map_err(rule)?;
        self.opv_claim_capacity(&job.class_binding_id, &claim.producer_bond, &claim.job_id).map_err(rule)?;
        let work = class.admission_work_v1().map_err(rule)?;
        self.charge(NAME, work)?;
        if stages.len() != evidence.stages.len()
            || stages.iter().zip(&evidence.stages).any(|(s, e)| s.evidence().root() != e.trace_root)
        {
            return Err(rule("the carried trace commitments are not the evidence's trace roots".into()));
        }
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
        if let Some(salt) = salt {
            self.claim_beacon_salts.insert(id, *salt);
        }
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
            // Every committed value of a segmented claim (derived windows included) is served on demand.
            ClaimBodyV1::Segmented { .. } => Vec::new(),
            ClaimBodyV1::Pipeline { .. } => self
                .pipeline_classes
                .get(&row.class_binding_id)
                .and_then(|c| {
                    let st = c.pipeline.stages.get(stage as usize)?;
                    Some(derived_mask_v1(&stage_view_v1(c.programs.get(st.program as usize)?).view))
                })
                .unwrap_or_default(),
            ClaimBodyV1::Spec(b) => self.spec_derived_mask(row, b, stage),
        }
    }

    pub(crate) fn check_court_work_v1(&self, work: u64) -> Result<(), String> {
        if work > self.policy.max_court_work_per_block {
            return Err("the class's worst court does not fit one block's court budget: nobody could prosecute it".into());
        }
        if work > self.policy.guaranteed_proof_work_v1() {
            return Err("the class's worst court does not fit guaranteed proof work: admissions could starve prosecution".into());
        }
        Ok(())
    }

    pub(crate) fn check_admission_work_v1(&self, work: u64) -> Result<(), String> {
        if work > self.policy.admission_work_limit_v1() {
            return Err("the class's structural claim admission cannot fit the block's non-proof work budget".into());
        }
        Ok(())
    }

    /// Count the borrowed carrier before allocating/copying a trace or hashing evidence. A global
    /// route envelope alone would let a small class carry arbitrarily larger malformed traces.
    pub(crate) fn check_claim_carrier_v1<T: BorshSerialize>(
        name: &'static str,
        bounds: &ProsecutionBoundsV1,
        value: &T,
    ) -> Result<(), KernelRefusalV1> {
        let len = borsh::object_length(value).map_err(|e| KernelRefusalV1::rule(name, e.to_string()))? as u128;
        let cap = bounds.max_commit_bytes.saturating_add(crate::gate::CLAIM_CARRIER_OVERHEAD_BYTES_V1);
        if len > cap {
            return Err(KernelRefusalV1::new(
                name,
                RefusalKindV1::Oversized,
                format!("claim carrier {len} bytes exceeds class bound {cap}"),
            ));
        }
        Ok(())
    }

    /// The prosecution bounds of a registered class of any kind (single program, pipeline, typed).
    pub fn bounds_of(&self, class: &Digest) -> Option<ProsecutionBoundsV1> {
        self.classes
            .get(class)
            .map(|c| c.bounds)
            .or_else(|| self.pipeline_classes.get(class).map(|c| c.bounds))
            .or_else(|| self.typed.classes.get(class).map(|c| c.bounds))
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
            (ClaimBodyV1::Segmented { .. }, ProsecutionV1::Segmented(bytes)) => self.seg_adjudicate(claim, bytes),
            (ClaimBodyV1::Pipeline { .. }, ProsecutionV1::Pipeline(bytes)) => {
                let (record, header, binding) = self.pipeline_public_record(claim).ok_or("no public record")?;
                let mode = self.mode_of_class(&header.class_binding_id);
                FreshPipelineVerifierV1::from_public_bytes_in_mode(&record.to_bytes(), &self.known, header, &binding, mode)?
                    .try_proof(bytes)
            }
            (ClaimBodyV1::Spec(_), ProsecutionV1::Spec(bytes)) => self.adjudicate_spec(claim, bytes),
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
        let mut fee = self.policy.dismissed_proof_fee;
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
        // **The claim's liability phase**: after Final, and after a pre-Final availability default (C4 F-C4R3-02: a default is never
        // a way out of a provable fraud), the claim stays convictable until `liability_until` — even with nothing reserved (a
        // post-Final default may have forfeited it). Past that horizon, or against a claim that ended without ever passing (timed
        // out), no proof can change anything: the filing is REFUSED before any court runs, and no fee is charged — a TRUE proof never
        // costs its filer the dismissal fee, and a refusal is free for everyone (it is dropped, as an unknown claim is).
        self.proof_open(row).map_err(rule)?;
        let bounds = self.bounds_of(&row.class_binding_id);
        let cheap = bounds.and_then(|b| oversized(proof, &b));
        let verdict = match cheap {
            Some(why) => Err(why),
            None => {
                // C4 F-C4R4-05: a filing that reserves a share of the block's court pays for that share if it is dismissed.
                let work = bounds.map(|b| b.max_court_work).unwrap_or(0);
                let scaled = self.policy.dismissal_fee_v1(work);
                if self.bonds.get(accuser).is_none_or(|b| b.free() < scaled) {
                    return Err(rule("the accuser's free collateral does not cover the filing fee"));
                }
                fee = scaled;
                // The court is about to run: it spends the block's budget (a refusal here costs the accuser nothing).
                self.charge(NAME, work)?;
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
            Ok(()) => {
                // GAP-R7: the bounty is the earliest sealer's of these exact bytes (or the filer's, if nobody sealed them earlier).
                let paid = self.bounty_holder(claim, accuser, proof);
                // M*-49: the drawn slots that sealed these bytes share up to `bounty_cap` of the bounty first.
                let drawn = self.drawn_sealers_v1(claim, &self.proof_sealers(claim, proof));
                self.convict(claim, &paid, &drawn, is_final, out)
            }
        }
        Ok(())
    }

    /// **Can a proof against this claim still change anything?** Live (it has not ended), or in its liability phase — after Final,
    /// or after a pre-Final availability default (C4 F-C4R3-02) — up to `liability_until`, even with nothing reserved. Past that
    /// horizon, or for a claim that ended without ever passing (timed out), it cannot: a filing (or a seal) is refused before any
    /// court runs and no fee is charged.
    fn proof_open(&self, row: &ClaimRowV1) -> Result<(), &'static str> {
        let liable = matches!(row.life.state, ClaimStateV1::Final { .. } | ClaimStateV1::Unavailable { .. });
        if liable && row.liability_until.is_none_or(|until| self.daa > until) {
            return Err("past the liability horizon");
        }
        if !liable && (row.life.state.is_terminal() || row.reserved == 0) {
            return Err("the claim ended without passing and holds nothing");
        }
        Ok(())
    }

    /// **Who is paid the bounty of a conviction by `proof`** (GAP-R7): the bond holding the EARLIEST seal of these exact proof bytes
    /// on this claim that is at least `claim_seal_delay_daa` old (ties broken by the bond digest) — whoever filed them; with no such
    /// seal, the filer (an unsealed filing counts as sealed in its own block). A copyist that lifts a sealed proof from its public
    /// carrier therefore pays the sealer. O(seals on the claim) hashes of 192 bytes, after one digest of the proof.
    fn bounty_holder(&self, claim: &Digest, filer: &Digest, proof: &ProsecutionV1) -> Digest {
        let digest = proof_digest_v1(proof);
        let delay = self.policy.claim_seal_delay_daa;
        self.proof_seals
            .range((*claim, [0u8; 64])..=(*claim, [0xFFu8; 64]))
            .filter(|((_, accuser), row)| {
                row.daa.saturating_add(delay) <= self.daa && row.seal == proof_seal_of_digest_v1(claim, accuser, &digest)
            })
            .min_by_key(|((_, accuser), row)| (row.daa, *accuser))
            .map_or(*filer, |((_, accuser), _)| *accuser)
    }

    /// Every bond holding a seal of these exact proof bytes on `claim` at least `claim_seal_delay_daa` old (the filer counts as sealed).
    fn proof_sealers(&self, claim: &Digest, proof: &ProsecutionV1) -> Vec<Digest> {
        let digest = proof_digest_v1(proof);
        let delay = self.policy.claim_seal_delay_daa;
        self.proof_seals
            .range((*claim, [0u8; 64])..=(*claim, [0xFFu8; 64]))
            .filter(|((_, accuser), row)| {
                row.daa.saturating_add(delay) <= self.daa && row.seal == proof_seal_of_digest_v1(claim, accuser, &digest)
            })
            .map(|((_, accuser), _)| *accuser)
            .collect()
    }

    fn convict(&mut self, claim: &Digest, accuser: &Digest, drawn: &[Digest], post_final: bool, out: &mut Vec<LedgerEventV1>) {
        let daa = self.daa;
        let producer_collateral = self.claims.get(claim).and_then(|r| self.bonds.get(&r.producer)).map_or(0, |b| b.collateral);
        // What a pre-Final default already took of this claim's reservation (0 for any other claim: before Final a reservation
        // changes only by a default or a conviction, and a post-Final forfeit leaves nothing to slash).
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
        // **One reporter pool per claim** (C4 F-C4R4-15, ADR-0032): the bounty is the accuser's share of what THIS conviction
        // collected, rounded down. A pre-Final default before it already paid its demanders at most the same share of the penalty,
        // rounded down too, so across the default and the conviction the reporters together receive at most `⌊share × collected⌋`
        // of everything collected — a coalition holding every reporter role loses ≥ 51 % of it (the 49 % share on the node,
        // `PALW_KERNEL_REPORTER_SHARE_PERMILLE_V1`). (This replaces F-C4R3-02's "as if no default had come first" basis, which
        // paid the penalty's share twice; the honest accuser's residual dilution is the Sybil demanders' part of the default's
        // share — recorded for ECON.) ADR-0176 HOOK `budget-bounty`: the bounty is a slash split, not issuance, but it counts
        // against the claim's R reservation made at `budget-accept`.
        //
        // **O2 (`palw_verifier_pay_v1`)**: a held default share is still in the reservation just slashed; the default itself collected
        // (and burned) `burned_at_default`, so the one pool is the share of everything collected — the honest accuser after a
        // self-inflicted default is paid as if no default had come first, and every reporter together still takes at most 49 %.
        let burned_at_default = match self.held_share_v1(claim) {
            Some((_, burned)) => {
                self.pool_held_share_v1(claim, out);
                burned
            }
            None => 0,
        };
        let basis = slashed as u128 + burned_at_default as u128;
        let reward = ((basis * self.policy.accuser_reward_permille as u128 / 1000) as u64).min(slashed);
        self.burned += slashed - reward;
        out.push(LedgerEventV1::Convicted { claim: *claim, accuser: *accuser, slashed, accuser_reward: reward, post_final });
        settle(out, producer, slashed, SettlementKindV1::SlashFraud, Some(*claim));
        // What the bond no longer holds was not slashed: the rest of the reservation is released, so the consumer's mirror matches.
        settle(out, producer, reserved - slashed, SettlementKindV1::ReleaseClaim, Some(*claim));
        // M*-49: the drawn sealers of the convicting bytes share up to `bounty_cap` first (equally); the earliest sealer keeps the rest.
        let to_drawn = match self.verifier_pay_in_force() {
            Some(p) if !drawn.is_empty() => {
                let each = reward.min(p.bounty_cap) / drawn.len() as u64;
                for d in drawn {
                    settle(out, *d, each, SettlementKindV1::AccuserReward, Some(*claim));
                }
                each * drawn.len() as u64
            }
            _ => 0,
        };
        settle(out, *accuser, reward - to_drawn, SettlementKindV1::AccuserReward, Some(*claim));
        settle(out, producer, slashed - reward, SettlementKindV1::Burn, Some(*claim));
        self.settle_demands_moot(claim, out);
        // The demands that led here are never penalised: the bonds of every served position of the claim return now.
        self.settle_served_demand_bonds(claim, false, out);
        self.opv_sync_live(claim);
        // A convicted claim takes no further filing: its proof seals are spent.
        let spent: Vec<(Digest, Digest)> =
            self.proof_seals.range((*claim, [0u8; 64])..=(*claim, [0xFFu8; 64])).map(|(k, _)| *k).collect();
        for k in spent {
            self.proof_seals.remove(&k);
        }
        if post_final {
            self.spec_on_post_final_conviction(claim);
        }
    }

    /// **Settle the bonds of a claim's SERVED positions**: refunded (`burn == false`: the claim was convicted, defaulted or timed out),
    /// or burned (`burn == true`: its liability horizon ended with no conviction — the demand only made an honest producer serve).
    fn settle_served_demand_bonds(&mut self, claim: &Digest, burn: bool, out: &mut Vec<LedgerEventV1>) {
        let keys: Vec<DemandKeyV1> =
            self.served_demands.range((*claim, 0u8, 0u32)..=(*claim, u8::MAX, u32::MAX)).map(|(k, _)| *k).collect();
        let mut burned = 0u64;
        for k in keys {
            let Some(bonds) = self.served_demands.remove(&k) else { continue };
            for (bond, amount) in bonds {
                let collateral = self.bonds.get(&bond).map_or(0, |b| b.collateral);
                // A-DEM (`palw_verifier_pay_v1`): a drawn slot's served demand bond is never burned.
                let taken = if burn && !self.is_drawn_slot_v1(claim, &bond) { amount.min(collateral) } else { 0 };
                if let Some(b) = self.bonds.get_mut(&bond) {
                    b.reserved = b.reserved.saturating_sub(amount);
                    b.collateral -= taken;
                }
                self.burned += taken;
                burned += taken;
                settle(out, bond, taken, SettlementKindV1::ForfeitDemandBond, Some(*claim));
                settle(out, bond, amount - taken, SettlementKindV1::ReleaseDemand, Some(*claim));
                settle(out, bond, taken, SettlementKindV1::Burn, Some(*claim));
            }
        }
        if burned > 0 {
            out.push(LedgerEventV1::ServedDemandBondsBurned { claim: *claim, burned });
        }
    }

    /// The served positions' bonds whose fate the claim's state now decides (the closing tick): refunded once the claim is decided
    /// without a Final standing (timed out, defaulted, convicted, or gone), burned once a Final claim's liability horizon has passed
    /// unconvicted; held otherwise.
    fn settle_due_served_demand_bonds(&mut self, out: &mut Vec<LedgerEventV1>) {
        let daa = self.daa;
        let claims: BTreeSet<Digest> = self.served_demands.keys().map(|(c, _, _)| *c).collect();
        for claim in claims {
            let fate = match self.claims.get(&claim) {
                None => Some(false),
                Some(r) if r.convicted => Some(false),
                Some(r) => match &r.life.state {
                    ClaimStateV1::TimedOut { .. } | ClaimStateV1::Unavailable { .. } | ClaimStateV1::Convicted { .. } => Some(false),
                    ClaimStateV1::Final { .. } if r.liability_until.is_some_and(|u| daa > u) => Some(true),
                    _ => None,
                },
            };
            if let Some(burn) = fate {
                self.settle_served_demand_bonds(&claim, burn, out);
            }
        }
    }

    /// **ADR-0177 D2 — the cumulative-scope hook** (C4 F-C4R4-17). Every demand passes here before it reserves anything: a demand
    /// whose served values, together with what the claim (and other claims of the class) already made public, would rebuild a
    /// registered weight tensor must be refused. The predicate is DA16b's central one (`da16/transport-provider-court`); until it
    /// lands this hook admits every demand, and the kernel PoC `f_c4r4_17_…` stays an ignored FAIL. The predicate must be a pure
    /// function of the ledger's rows (served positions, open demands, the class's program) so every node refuses the same demand.
    fn cumulative_scope_allows_v1(&self, claim: &Digest, stage: u8, position: u32) -> Result<(), &'static str> {
        let _ = (claim, stage, position);
        Ok(())
    }

    /// Return every demand bond of `d` (the demand is over).
    pub(crate) fn refund(&mut self, claim: &Digest, d: &DemandRowV1, out: &mut Vec<LedgerEventV1>) {
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
        // A segmented claim's served positions whose demand bonds wait for their grace: the claim is decided, so they return now.
        refunded += self.seg_release_held(claim, out);
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
        if matches!(self.claims.get(claim).map(|r| &r.body), Some(ClaimBodyV1::Segmented { .. })) {
            return self.seg_file_demand(demander, claim, stage, position, out);
        }
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
        if row.body.position(stage, position).is_none() && !self.spec_slice_demandable(row, stage, position) {
            return Err(rule("the claim commits no such position"));
        }
        let k = (*claim, stage, position);
        if self.served.contains_key(&k) {
            return Err(rule("already served: it is public"));
        }
        self.cumulative_scope_allows_v1(claim, stage, position).map_err(rule)?;
        let need = self.policy.demand_bond;
        let Some(b) = self.bonds.get(demander) else { return Err(rule("the demander is not a registered bond")) };
        if b.exit_requested.is_some() || b.free() < need {
            return Err(rule("the demander's free collateral does not cover the demand bond"));
        }
        if let Some(d) = self.demands.get_mut(&k) {
            // Shared progress: a second demander joins the open demand rather than being refused by it (its deadline is the open
            // demand's: joining restarts nothing).
            if !d.demanders.iter().any(|(b, _)| b == demander) {
                if d.demanders.len() >= crate::gate::MAX_DEMANDERS_PER_SESSION_V1 {
                    return Err(rule(
                        "the shared demand's collateral participants are full; its public response/default deadline is unchanged",
                    ));
                }
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

    /// A SERVED demand closes: its bonds stay reserved, their fate decided later (`served_demands`).
    fn close_demand(&mut self, k: DemandKeyV1, out: &mut Vec<LedgerEventV1>) {
        let Some(d) = self.demands.remove(&k) else { return };
        let _ = out;
        if !d.demanders.is_empty() {
            self.served_demands.insert(k, d.demanders);
        }
        let daa = self.daa;
        if let Some(row) = self.claims.get_mut(&k.0)
            && matches!(row.life.state, ClaimStateV1::Disputed { .. })
        {
            let _ = row.life.apply(ClaimEventV1::CourtVerdict { daa, convicted: false });
        }
    }

    /// **A response to an open demand** (C4 F-C4R4-14: the response lane). A response spends NO run of the block's shared
    /// adjudication budget: classifying it is linear in its own bytes (bounded by the class's `max_response_bytes` and carried in
    /// the block), so no other object can crowd an honest producer's valid answer out before its deadline, whoever orders the block.
    /// A REJECTED response (oversized, or classified as not serving the position) costs its signer `dismissed_proof_fee` (slashed,
    /// burned) — junk is never free — and changes nothing the producer needs: the demand stays open with its deadline, and only a
    /// response signed by the claim's producer records the demand's last rejection class. A signer whose free collateral cannot pay
    /// that fee is refused before anything is classified. A valid response pays nothing and closes the demand, whoever signs it.
    fn respond(
        &mut self,
        signer: &Digest,
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
        let fee = self.policy.dismissed_proof_fee;
        if self.bonds.get(signer).is_none_or(|b| b.free() < fee) {
            return Err(KernelRefusalV1::rule(NAME, "the signer's free collateral does not cover a rejected response's fee"));
        }
        // K2-TIR-v4: a segmented claim's demand is answered in parts, with the same fee for a rejected one.
        if matches!(self.claims.get(claim).map(|r| &r.body), Some(ClaimBodyV1::Segmented { .. })) {
            return self.seg_respond(signer, claim, position, bytes, out);
        }
        let row = self.claims.get(claim).expect("a demand names a committed claim");
        let by_producer = row.producer == *signer;
        let limit = self.bounds_of(&row.class_binding_id).map(|b| b.max_response_bytes).unwrap_or(0);
        let verdict = if bytes.len() as u128 > limit {
            Err("oversized")
        } else {
            let row = self.claims.get(claim).expect("checked");
            match self.spec_classify_slice(row, stage, position, bytes) {
                Some(slice) => slice,
                None => {
                    let (values, inputs) = row.body.position(stage, position).expect("a demand names a committed position");
                    classify_position_response_v1(values, &self.derived_mask(row, stage), inputs, bytes)
                }
            }
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
                if by_producer && let Some(d) = self.demands.get_mut(&k) {
                    d.last = Some(response_class_code(class));
                }
                let b = self.bonds.get_mut(signer).expect("checked");
                b.collateral -= fee;
                self.burned += fee;
                out.push(LedgerEventV1::ResponseRejected { claim: *claim, stage, position, class });
                settle(out, *signer, fee, SettlementKindV1::SlashFiling, Some(*claim));
                settle(out, *signer, fee, SettlementKindV1::Burn, Some(*claim));
            }
        }
        Ok(())
    }

    /// **Forfeit an unrevealed claim seal** (its row already removed from `seals`): the deposit is slashed and burned (as far as the
    /// bond still holds it), and a seal accepted past `palw_panel_free_v1` stays readable as `(job, producer, sealed_daa) →
    /// {seal, forfeited_daa}` (table 26) — it vetoes the v3 beacon it was mixed into; deleting it would turn the veto into a silent
    /// exclusion. Called at expiry (the tick) and, past the fence, when a re-seal replaces it (ECON fix S1).
    fn forfeit_claim_seal(&mut self, job: Digest, producer: Digest, row: SealRowV1, out: &mut Vec<LedgerEventV1>) {
        let daa = self.daa;
        // S3 (`palw_verifier_pay_v1`): a forfeited beacon-source seal forfeits its extra deposit too (`d_src` in all).
        self.close_beacon_source_v1(&job, &producer, row.daa, true, out);
        if self.salted_seals_from().is_some_and(|at| row.daa >= at) {
            self.forfeited_claim_seals.insert((job, producer, row.daa), ForfeitedSealRowV1 { seal: row.seal, forfeited_daa: daa });
        }
        let deposit = row.deposit;
        if deposit == 0 {
            return;
        }
        let collateral = self.bonds.get(&producer).map_or(0, |b| b.collateral);
        let taken = deposit.min(collateral);
        if let Some(b) = self.bonds.get_mut(&producer) {
            b.reserved = b.reserved.saturating_sub(deposit);
            b.collateral -= taken;
        }
        self.burned += taken;
        settle(out, producer, taken, SettlementKindV1::ForfeitSealDeposit, None);
        settle(out, producer, deposit - taken, SettlementKindV1::ReleaseSealDeposit, None);
        settle(out, producer, taken, SettlementKindV1::Burn, None);
        out.push(LedgerEventV1::SealForfeited { job, producer, forfeited: taken });
    }

    /// Deadlines, windows, Final, liability release.
    fn tick_into(&mut self, out: &mut Vec<LedgerEventV1>) {
        let daa = self.daa;
        // K2-TIR-v4: served positions whose proof grace ended settle their demanders' bonds; stale progress rows go.
        self.seg_tick(out);
        // Unrevealed seals expire (a junk seal holds nothing and lives a bounded time).
        let ttl = self.policy.seal_ttl_daa;
        // An unrevealed claim seal expires and FORFEITS its deposit (slashed, burned): withholding a sealed reveal is never free.
        let expired: Vec<((Digest, Digest), SealRowV1)> =
            self.seals.iter().filter(|(_, row)| daa > row.daa.saturating_add(ttl)).map(|(k, row)| (*k, *row)).collect();
        for ((job, producer), row) in expired {
            self.seals.remove(&(job, producer));
            self.forfeit_claim_seal(job, producer, row, out);
        }
        self.proof_seals.retain(|_, row| daa <= row.daa.saturating_add(ttl));
        // GAP-5: escrows no claim can still use go back to their posters.
        self.release_idle_job_escrows(out);
        self.spec_prune_lines(daa);
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
            // Before Final the demanders share part of the penalty equally and the rest is burned: a demander may be the producer's
            // own Sybil, so a default is split like a slash (C4 F-C4R3-02) — the demanders take the accuser's share
            // (`accuser_reward_permille`, < 1000) and an OPV claim's at most `1000 − default_burn_permille` (RFC-0015 §8.2): a producer
            // can never cycle its own penalty through a Sybil demander for nothing. After Final the forfeit is burned whole (the
            // colluders would otherwise recoup part of a forfeit that replaces the reward they kept). Their bonds return either way.
            let burn_permille = match (self.opv.claims.contains_key(&claim), self.opv.policy) {
                (true, Some(p)) => {
                    (1000 - self.policy.accuser_reward_permille.min(1000)).max(p.economics.default_burn_permille.min(1000))
                }
                _ => 1000 - self.policy.accuser_reward_permille.min(1000),
            };
            // Rounded DOWN (C4 F-C4R4-15): the demanders never take more than their share of what the default collected, so the
            // claim's one reporter pool (the default's share plus a later conviction's bounty) stays within `⌊share × collected⌋`.
            let paid = if post_final { 0 } else { (penalty as u128 * (1000 - burn_permille) as u128 / 1000) as u64 };
            let share = if d.demanders.is_empty() { 0 } else { paid / d.demanders.len() as u64 };
            // **O2 (`palw_verifier_pay_v1`)**: the demanders' share is HELD — still reserved on the producer's bond, collected only
            // by a conviction (into its one pool) or, unconvicted, at the liability horizon (then paid to the demanders).
            let held = if self.verifier_pay_in_force().is_some() && !post_final { share * d.demanders.len() as u64 } else { 0 };
            let share = if held > 0 { 0 } else { share };
            let burn = penalty - held - share * d.demanders.len() as u64;
            self.burned += burn;
            let producer = self.claims.get(&claim).map(|r| r.producer);
            if let Some(row) = self.claims.get_mut(&claim) {
                let was_final = matches!(row.life.state, ClaimStateV1::Final { .. });
                row.reserved -= taken - held;
                if !was_final {
                    let _ = row.life.apply(ClaimEventV1::MaterialUnavailable { daa, producer_defaulted: true });
                    // **A pre-Final default is not the end of the claim's liability** (C4 F-C4R3-02): the rest of the reservation
                    // stays held, and a valid proof filed by `daa + liability_daa` still convicts it — keeping one proof out of the
                    // chain for a demand's deadline must not turn a provable fraud into a cheap default. If no valid proof arrives
                    // the outcome stays the default (availability, never fraud) and the reservation is released at the horizon.
                    row.liability_until = Some(daa.saturating_add(self.policy.liability_daa));
                }
                if let Some(b) = self.bonds.get_mut(&row.producer) {
                    b.reserved = b.reserved.saturating_sub(taken - held);
                    b.collateral = b.collateral.saturating_sub(penalty - held);
                }
            }
            out.push(if post_final {
                LedgerEventV1::PostFinalDefault { claim, stage, position, last, forfeited: penalty }
            } else {
                LedgerEventV1::ProducerDefault { claim, stage, position, last, penalty }
            });
            if let Some(producer) = producer {
                settle(out, producer, penalty - held, SettlementKindV1::SlashDefault, Some(claim));
                settle(out, producer, taken - penalty, SettlementKindV1::ReleaseClaim, Some(claim));
                for (bond, _) in &d.demanders {
                    settle(out, *bond, share, SettlementKindV1::DemanderShare, Some(claim));
                }
                settle(out, producer, burn, SettlementKindV1::Burn, Some(claim));
            }
            if held > 0 {
                let demanders = d.demanders.iter().map(|(b, _)| *b).collect();
                self.hold_share_v1(claim, held, penalty - held, demanders, out);
            }
            self.refund(&claim, &d, out);
            // An unavailable claim never finalizes; its other open demands are moot, and the bonds of its served positions return.
            self.settle_demands_moot(&claim, out);
            self.settle_served_demand_bonds(&claim, false, out);
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
                    // GAP-5: the reward is paid out of the job's escrow (the poster pays), once — never issued. ADR-0176 hook
                    // (`palw_bond_budget_v1`, not built): this payout also re-checks and consumes the producer bond's reservation.
                    let (job, convicted) = (row.job_id, row.convicted);
                    let mut paid = Vec::new();
                    let reward = if convicted { 0 } else { self.pay_from_job_escrow(&job, &id, producer, &mut paid) };
                    self.claims.get_mut(&id).expect("listed").rewarded = reward > 0;
                    out.push(LedgerEventV1::Final { claim: id, reward });
                    out.extend(paid);
                    self.spec_on_final(&id);
                }
                (b, ClaimStateV1::TimedOut { .. }) if !matches!(b, ClaimStateV1::TimedOut { .. }) => {
                    self.release(&id, producer, reserved, out);
                    out.push(LedgerEventV1::TimedOut { claim: id });
                }
                // A defaulted claim's reservation is held through its liability horizon (C4 F-C4R3-02), then released.
                (ClaimStateV1::Unavailable { .. }, _) if reserved > 0 && liability.is_none_or(|u| daa > u) => {
                    // O2: the horizon passed unconvicted — the held demanders' share is collected now and paid to them.
                    let held = self.pay_held_share_v1(&id, producer, out);
                    if let Some(r) = self.claims.get_mut(&id) {
                        r.reserved -= held;
                    }
                    let reserved = reserved - held;
                    self.release(&id, producer, reserved, out);
                    out.push(LedgerEventV1::Released { claim: id });
                }
                (ClaimStateV1::Final { .. }, _) if reserved > 0 && liability.is_some_and(|u| daa > u) => {
                    self.release(&id, producer, reserved, out);
                    out.push(LedgerEventV1::Released { claim: id });
                }
                _ => {}
            }
        }
        // M*-49: draws paid on their claim's fate or returned; idle check-fee escrows returned.
        self.tick_verifier_pay_v1(out);
        // The served positions' demand bonds whose claim is now decided (after this block's defaults, Finals and releases).
        self.settle_due_served_demand_bonds(out);
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
const COMMIT_OVERHEAD_V1: u128 = crate::gate::CLAIM_CARRIER_OVERHEAD_BYTES_V1;

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
    if b.max_commit_bytes.saturating_add(COMMIT_OVERHEAD_V1) > commit_cap as u128 {
        return Err(format!("{} bytes of commitments do not fit a claim commitment ({commit_cap})", b.max_commit_bytes));
    }
    Ok(())
}

/// Why a filing is past the class's envelope, if it is (checked before any court runs).
fn oversized(proof: &ProsecutionV1, b: &ProsecutionBoundsV1) -> Option<String> {
    let (len, limit) = match proof {
        ProsecutionV1::Kernel(bytes) | ProsecutionV1::Pipeline(bytes) | ProsecutionV1::Segmented(bytes) => {
            (bytes.len() as u128, b.max_filing_bytes as u128)
        }
        ProsecutionV1::Decode(f) => (f.logits.bytes.len() as u128, b.max_response_bytes),
        // A typed fault opens at most one kernel instance, one item or one logits vector.
        ProsecutionV1::Spec(bytes) => (bytes.len() as u128, (b.max_filing_bytes as u128).max(b.max_response_bytes)),
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
        (crate::verify::canonical_tensor_v1(&t) && tensor_commitment(&t) == *want).then_some(t)
    }
    fn stage_input(&self, k: u16, position: u32) -> Option<Tensor> {
        self.o.input(self.stage, position, k)
    }
}

impl OutsiderV1<'_> {
    /// Deterministically check a program or pipeline claim's computation and decode from public commitments, job and the acquired
    /// registered artifact. No producer node values are requested, even when every position is withheld. Missing/incorrect
    /// artifact bytes are a verifier acquisition error, never producer misconduct. DA duties are checked separately by `check`.
    pub fn check_computation(&self) -> Result<OutsiderFindingV1, String> {
        let row = self.ledger.claims.get(&self.claim).ok_or("no such claim")?;
        if matches!(&row.body, ClaimBodyV1::Pipeline { .. }) {
            return self.check_pipeline_mode(row, true);
        }
        if matches!(&row.body, ClaimBodyV1::Segmented { .. }) {
            return Err(
                "a segmented claim: use check_segmented_computation with public position paths/parts and authenticated job ids".into(),
            );
        }
        let ClaimBodyV1::Program { claim, .. } = &row.body else {
            return Err("a typed claim: replay with crate::spec::outsider::SpecOutsiderV1".into());
        };
        let class = self.ledger.classes.get(&row.class_binding_id).ok_or("no such class")?;
        let job = self.ledger.jobs.get(&row.job_id).ok_or("no such job")?;
        let (record, header) = self.ledger.public_record(&self.claim).ok_or("no public record")?;
        let fresh = FreshVerifierV1::from_public_bytes(&record.to_bytes(), &self.ledger.known, header)?;
        let material = StageMaterial { o: self, stage: 0, program: 0, commitments: &class.param_commitments };
        match fresh.reexecute(&material)? {
            crate::verify::ReexecutionV1::Fault(proof) => {
                Ok(OutsiderFindingV1::Prosecute(ProsecutionV1::Kernel(crate::public::FaultProofWireV1::of(&proof).to_bytes())))
            }
            crate::verify::ReexecutionV1::Match { logits, .. } => {
                for r in 0..claim.generated.len() {
                    let t = logits.get(claim.select_position(job, r) as usize).ok_or("no replay decode position")?;
                    if job.decode.select(t) != Some(claim.generated[r]) {
                        return Ok(OutsiderFindingV1::Prosecute(ProsecutionV1::Decode(DecodeFaultV1 {
                            index: r as u32,
                            logits: TensorWireV1::of(t),
                        })));
                    }
                }
                Ok(OutsiderFindingV1::Clean)
            }
        }
    }

    /// Stream a segmented claim's replay from this ledger's public class/job/claim rows and the acquired registered model.
    /// Position material is the public stream or authenticated parts read from blocks, never producer private state.
    pub fn check_segmented_computation(
        &self,
        positions: &dyn crate::element::SegMaterialV1,
        tokens: &[u32],
    ) -> Result<OutsiderFindingV1, String> {
        let view = self.ledger.seg_claim_view_v1(&self.claim).ok_or("no segmented claim")?;
        let artifact = |index, layer| self.artifact.param(0, index, layer);
        let r = crate::seg_detect::reexecute_claim_v1(&view.context(), positions, &artifact, tokens);
        match r.finding {
            crate::element::SegFindingV1::Clean => Ok(OutsiderFindingV1::Clean),
            crate::element::SegFindingV1::Fault(f) => Ok(OutsiderFindingV1::Prosecute(ProsecutionV1::Segmented(f.to_bytes()))),
            crate::element::SegFindingV1::Demand(ps) => Ok(OutsiderFindingV1::Demand(ps.into_iter().map(|p| (0, p)).collect())),
            crate::element::SegFindingV1::Inconsistent(why) => Err(why),
        }
    }

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
        if matches!(row.body, ClaimBodyV1::Segmented { .. }) {
            return Err("a segmented claim is checked position by position (crate::element::check_positions_v1)".into());
        }
        let missing = self.missing(row);
        if !missing.is_empty() {
            return Ok(OutsiderFindingV1::Demand(missing));
        }
        match &row.body {
            ClaimBodyV1::Program { claim, .. } => self.check_program(row, claim),
            ClaimBodyV1::Pipeline { .. } => self.check_pipeline(row),
            ClaimBodyV1::Segmented { .. } => unreachable!("dispatched above"),
            ClaimBodyV1::Spec(_) => Err("a typed claim: check it with crate::spec::outsider::SpecOutsiderV1".into()),
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
        self.check_pipeline_mode(row, false)
    }

    fn check_pipeline_mode(&self, row: &ClaimRowV1, commitments_only: bool) -> Result<OutsiderFindingV1, String> {
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
        match if commitments_only { fresh.reexecute(&refs)? } else { fresh.check_salted(&refs, self.salt) } {
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
            max_commit_bytes: 0,
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
            max_commit_bytes: 5000,
            max_localization_rounds: 2,
            max_court_work: 0,
            max_verifier_ram: 0,
            max_retained_state: 5 << 30,
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
