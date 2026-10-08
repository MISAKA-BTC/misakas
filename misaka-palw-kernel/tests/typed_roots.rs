//! **RFC-0004 Part II on the ledger**: computation specifications with typed roots — `Weights` byte for byte, `Memory` (steps over
//! overlays, the line carried across jobs, the pre-state's DA), `Retrieval` (wrong item, missed better item, slice DA), `Composite`
//! (a verified tool stage feeding a model; per-stage localisation; the logits→query edge court) and the bounds of every kind.
//!
//! Every scenario runs the ledger's own per-object API through blocks, and every prosecution is built by a fresh
//! [`SpecOutsiderV1`] over a ledger REBUILT FROM ROWS (`from_rows(to_rows)`), so nothing the producer holds privately reaches a court.

use std::collections::BTreeMap;

use misaka_palw_kernel::descriptor::{KernelScheduleV1, KernelStatusV1, k2_tir_v1_descriptor, k2_tir_v2_descriptor};
use misaka_palw_kernel::gate::ProsecutionPolicyV1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{
    AuthV1, KernelLedgerV1, KernelRouteObjectV1 as O, LedgerBlockV1, LedgerEventV1 as E, LedgerPolicyV1, LedgerTxV1,
    OutsiderFindingV1, ProsecutionV1, PublicSourceV1, claim_seal_v1, single_class_id_v1,
};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::mode::VerificationModeV1;
use misaka_palw_kernel::opv::{CarrierCapsV1, OpvBudgetsV1, OpvEconomicsV1, OpvPolicyV1, OpvWindowV1};
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::public::{MaterialResponseV1, PositionResponseV1, TensorWireV1, program_root_v1};
use misaka_palw_kernel::rows::root_of_rows;
use misaka_palw_kernel::spec::composite::{
    CompositeClaimV1, CompositeJobV1, CompositeRootV1, CompositeStageV1, QuerySourceV1, StageClaimV1, StageInputV1, TokenSourceV1,
};
use misaka_palw_kernel::spec::memory::{MemoryJobV1, MemoryRootV1, MemorySlotV1, memory_root_v1, slot_commitments_v1};
use misaka_palw_kernel::spec::outsider::SpecOutsiderV1;
use misaka_palw_kernel::spec::produce::{
    MemoryProductionV1, composite_job_id_v1, produce_memory_v1, produce_model_stage_v1, produce_retrieval_stage_v1,
};
use misaka_palw_kernel::spec::retrieval::{
    IndexV1, RetrievalClaimV1, RetrievalItemV1, RetrievalJobV1, RetrievalRootV1, RetrievalRuleV1, SnapshotDataV1,
};
use misaka_palw_kernel::spec::{
    ComputationSpecV1, MEMORY_PRE_STATE_STAGE_V1, SNAPSHOT_STAGE_BASE_V1, SpecClaimV1, SpecClassKindV1, SpecFaultV1, SpecJobV1,
    SpecObjectV1, TypedRootV1, WeightsRootV1, k2_tr_v1_descriptor, memory_bounds_v1, retrieval_bounds_v1,
    retrieval_claim_material_bytes_v1,
};
use misaka_palw_kernel::trace::{ParamCommitmentsV1, TraceV1};
use misaka_palw_kernel::{KernelDescriptorV1, VerificationPlanV1};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{MapParams, Prim, Tensor};
use misaka_palw_tir_sketch::fixture::{TirSketchFixtureV1, memory_ttt_v1, memory_v1, wide128_v1};

const REG: Digest = [0x0C; 64];
const P1: Digest = [0xA1; 64];
const P2: Digest = [0xA2; 64];
const OUT: Digest = [0x0B; 64];
const MAX_POSITIONS: u32 = 64;
const OPV: VerificationModeV1 = VerificationModeV1::OptimisticPublicVerification;

fn policy() -> LedgerPolicyV1 {
    LedgerPolicyV1 {
        network_domain: [9; 64],
        ruleset_digest: [3; 64],
        challenge_policy_id: [5; 64],
        claim_collateral: 1000,
        demand_bond: 10,
        check_window_daa: 100,
        challenge_window_daa: 50,
        court_deadline_daa: 20,
        proof_grace_daa: 10,
        liability_daa: 200,
        exit_delay_daa: 30,
        dismissed_proof_fee: 5,
        accuser_reward_permille: 500,
        default_penalty: 100,
        claim_reward: 7,
        max_adjudications_per_block: 64,
        max_court_work_per_block: u64::MAX,
        claim_seal_delay_daa: 1,
        seal_ttl_daa: 100,
        prosecution: ProsecutionPolicyV1 {
            court_deadline_daa: 20,
            max_sessions_per_claim: 1 << 10,
            max_public_bytes: 1 << 40,
            max_verifier_ram: 1 << 36,
            max_retained_state: 1 << 32,
        },
    }
}

fn opv() -> OpvPolicyV1 {
    OpvPolicyV1 {
        activation_daa: Some(0),
        window: OpvWindowV1 { base_challenge_window_daa: 40, verification_horizon_daa: 10 },
        budgets: OpvBudgetsV1 {
            cold_material_daa: 10,
            check_daa: 10,
            localize_daa: 2,
            disclose_daa: 8,
            court_daa: 3,
            carrier_daa: 2,
            reorg_slack_daa: 2,
        },
        economics: OpvEconomicsV1 {
            reservation_per_claim: 1000,
            work_credit_per_claim: 13,
            external_gain_bound: 80,
            assumed_detection_permille: 500,
            max_live_claims_per_producer: 8,
            max_live_claims_total: 32,
            default_burn_permille: 100,
        },
        carrier: CarrierCapsV1 { filing_cap: 1 << 26, response_cap: 1 << 27, commit_cap: 1 << 27 },
    }
}

/// K2-TIR-v1 and v2 Active; `typed`: K2-TR-v1 Active too (the consumer's `palw_typed_roots_v1` fence in force).
fn schedule(typed: bool) -> KernelScheduleV1 {
    let s = KernelScheduleV1::default()
        .with(k2_tir_v1_descriptor().digest(), KernelStatusV1::Active { since_daa: 0 })
        .with(k2_tir_v2_descriptor().digest(), KernelStatusV1::Active { since_daa: 0 });
    if typed { s.with(k2_tr_v1_descriptor().digest(), KernelStatusV1::Active { since_daa: 0 }) } else { s }
}

fn genesis(typed: bool) -> KernelLedgerV1 {
    KernelLedgerV1::genesis(policy(), schedule(typed), vec![k2_tir_v1_descriptor(), k2_tir_v2_descriptor()])
        .unwrap()
        .with_opv_policy(opv())
        .unwrap()
}

fn obj(signer: Digest, object: O) -> LedgerTxV1 {
    LedgerTxV1::Object { auth: AuthV1 { signer_bond: signer }, object }
}

fn spec_obj(signer: Digest, object: SpecObjectV1) -> LedgerTxV1 {
    obj(signer, O::Spec { object })
}

fn refused(ev: &[E]) -> Option<String> {
    ev.iter().find_map(|e| match e {
        E::Refused { why, .. } => Some(why.clone()),
        _ => None,
    })
}

struct W {
    l: KernelLedgerV1,
}

impl W {
    fn new(typed: bool) -> Self {
        let mut w = W { l: genesis(typed) };
        let bonds = [REG, P1, P2, OUT].map(|b| LedgerTxV1::SyncBond { bond: b, collateral: 1_000_000 }).to_vec();
        w.block(bonds);
        w
    }

    fn block(&mut self, txs: Vec<LedgerTxV1>) -> Vec<E> {
        let daa = self.l.daa + 1;
        self.l.apply_block(&LedgerBlockV1 { daa, txs })
    }

    fn beat_to(&mut self, daa: u64) {
        while self.l.daa < daa {
            self.block(Vec::new());
        }
    }

    /// The ledger a fresh node rebuilds from the rows (and the same root).
    fn rebuilt(&self) -> KernelLedgerV1 {
        let rows = self.l.to_rows();
        let r = KernelLedgerV1::from_rows(&genesis(self.l.typed_roots_active()), self.l.scalars(), &rows).unwrap();
        assert_eq!(r.root(), self.l.root(), "the rows rebuild the committed root");
        assert_eq!(root_of_rows(&self.l.policy, self.l.config_root(), self.l.scalars(), self.l.opv_policy(), &rows), self.l.root());
        r
    }

    fn register(&mut self, spec: ComputationSpecV1, attest: Option<Digest>) -> (Digest, Vec<E>) {
        let class = spec.class_id().unwrap();
        let mut txs = Vec::new();
        if let Some(a) = attest {
            txs.push(LedgerTxV1::AttestArtifact { artifact_root: a });
        }
        if spec.mode == OPV {
            txs.push(LedgerTxV1::AdmitOptimisticClass { class });
        }
        txs.push(spec_obj(REG, SpecObjectV1::RegisterClass { spec }));
        (class, self.block(txs))
    }

    fn post(&mut self, job: SpecJobV1) -> Digest {
        let id = job.id();
        let ev = self.block(vec![spec_obj(REG, SpecObjectV1::PostJob { job })]);
        assert!(ev.contains(&E::JobPosted { job: id }), "{ev:?}");
        // GAP-5 (G14-R4, at the merge): a typed job opens its poster's escrow exactly as `PostJob` does.
        if self.l.policy.claim_reward > 0 {
            assert_eq!(self.l.job_escrows.get(&id).map(|e| (e.poster, e.amount)), Some((REG, self.l.policy.claim_reward)));
        }
        id
    }

    /// Seal one block, then reveal. Past `palw_panel_free_v1` (OPV-BOOT GAP-B1a) the seal is salted (`claim_seal_v2`) and the reveal
    /// carries the salt (`CommitClaimSalted` with the typed claim); before it, the historical unsalted pair.
    fn commit(&mut self, claim: SpecClaimV1) -> (Digest, Vec<E>) {
        let (id, producer, job) = (claim.id(), claim.producer(), claim.job_id());
        if self.l.salted_seals_from().is_some_and(|at| self.l.daa >= at) {
            let salt = misaka_palw_kernel::hash::id(b"misaka-palw/test/claim-salt", &id);
            let seal = misaka_palw_kernel::ledger::claim_seal_v2(&id, &salt);
            self.block(vec![obj(producer, O::SealClaim { producer, job, seal })]);
            let reveal = O::CommitClaimSalted { salt, commit: misaka_palw_kernel::ledger::SaltedCommitV1::Spec { claim } };
            let ev = self.block(vec![obj(producer, reveal)]);
            return (id, ev);
        }
        self.block(vec![obj(producer, O::SealClaim { producer, job, seal: claim_seal_v1(&id) })]);
        let ev = self.block(vec![spec_obj(producer, SpecObjectV1::CommitClaim { claim })]);
        (id, ev)
    }

    fn file(&mut self, accuser: Digest, claim: Digest, fault: SpecFaultV1) -> Vec<E> {
        self.block(vec![obj(accuser, O::FileProof { accuser, claim, proof: ProsecutionV1::Spec(fault.to_bytes()) })])
    }

    fn state(&self, claim: &Digest) -> ClaimStateV1 {
        self.l.claims[claim].life.state.clone()
    }
}

/// A public DA directory: `(stage, position, occurrence, node) → value`.
#[derive(Default)]
struct Da(BTreeMap<(u8, u32, u16, u16), Tensor>);

impl Da {
    fn stage(&mut self, stage: u8, offset: u32, trace: &TraceV1) {
        for (p, pos) in trace.values.iter().enumerate() {
            for (s, occ) in pos.iter().enumerate() {
                for (n, t) in occ.iter().enumerate() {
                    self.0.insert((stage, offset + p as u32, s as u16, n as u16), t.clone());
                }
            }
        }
    }

    fn memory(prod: &MemoryProductionV1, pre: Option<&[Tensor]>) -> Da {
        let mut da = Da::default();
        let mut off = 0;
        for t in &prod.traces {
            da.stage(0, off, t);
            off += t.values.len() as u32;
        }
        if let Some(pre) = pre {
            for (k, t) in pre.iter().enumerate() {
                da.0.insert((MEMORY_PRE_STATE_STAGE_V1, 0, 0, k as u16), t.clone());
            }
        }
        da
    }
}

impl PublicSourceV1 for Da {
    fn node(&self, stage: u8, p: u32, s: u16, n: u16) -> Option<Tensor> {
        self.0.get(&(stage, p, s, n)).cloned()
    }
}

fn check(
    l: &KernelLedgerV1,
    claim: Digest,
    da: &Da,
    artifact: &dyn misaka_palw_kernel::ledger::PublicArtifactV1,
    snapshots: &dyn misaka_palw_kernel::spec::outsider::SnapshotSourceV1,
) -> OutsiderFindingV1 {
    SpecOutsiderV1 { ledger: l, claim, material: da, artifact, snapshots, salt: [0x5A; 64] }.check().unwrap()
}

fn plan_of(d: &KernelDescriptorV1, p: &TirProgramV1) -> VerificationPlanV1 {
    plan_for_tir_program_v1(d, p, program_root_v1(&p.encode()), MAX_POSITIONS).unwrap()
}

fn weights(fx: &TirSketchFixtureV1, d: &KernelDescriptorV1) -> WeightsRootV1 {
    WeightsRootV1 {
        descriptor: d.digest(),
        program_bytes: fx.program.encode(),
        plan: plan_of(d, &fx.program),
        param_commitments: ParamCommitmentsV1::of(&fx.params),
    }
}

fn bump(t: &mut Tensor, at: usize) {
    let v = t.data[at];
    t.data[at] = if t.dtype.contains(v + 1) { v + 1 } else { v - 1 };
}

/// The first `MatMul` of the program: `(occurrence, node)`.
fn matmul_of(p: &TirProgramV1) -> (usize, usize) {
    for (s, (b, _)) in p.occurrences().iter().enumerate() {
        for (n, node) in p.blocks[*b as usize].nodes.iter().enumerate() {
            if matches!(node.prim, Prim::MatMul) {
                return (s, n);
            }
        }
    }
    panic!("no MatMul")
}

// ---- 1. Weights: byte for byte ---------------------------------------------------------------------------------------------

#[test]
fn weights_only_spec_is_byte_for_byte_the_legacy_registration_in_both_modes() {
    let fx = wide128_v1(7);
    let d = k2_tir_v2_descriptor();
    let w = weights(&fx, &d);
    for mode in [VerificationModeV1::PanelLicensed, OPV] {
        let legacy = single_class_id_v1(w.descriptor, &w.program_bytes, &w.plan, &w.param_commitments, mode);
        let spec = ComputationSpecV1 { version: 1, mode, roots: vec![TypedRootV1::WeightsV1(w.clone())] };
        assert_eq!(spec.class_id().unwrap(), legacy, "the Weights-only id is the legacy id ({})", mode.name());

        // Ledger A: route tag 1 / 13. Ledger B: the Spec object.
        let mut a = W::new(true);
        let mut b = W::new(true);
        let mut txs = vec![LedgerTxV1::AttestArtifact { artifact_root: w.param_commitments.root() }];
        if mode == OPV {
            txs.push(LedgerTxV1::AdmitOptimisticClass { class: legacy });
        }
        let legacy_obj = if mode == OPV {
            O::RegisterClassV2 {
                mode,
                descriptor: w.descriptor,
                program_bytes: w.program_bytes.clone(),
                plan: w.plan.clone(),
                param_commitments: w.param_commitments.clone(),
            }
        } else {
            O::RegisterClass {
                descriptor: w.descriptor,
                program_bytes: w.program_bytes.clone(),
                plan: w.plan.clone(),
                param_commitments: w.param_commitments.clone(),
            }
        };
        let ea = a.block([txs.clone(), vec![obj(REG, legacy_obj)]].concat());
        let eb = b.block([txs, vec![spec_obj(REG, SpecObjectV1::RegisterClass { spec })]].concat());
        assert_eq!(ea, eb, "the same events");
        assert_eq!(ea.last(), Some(&E::ClassRegistered { class: legacy }));
        assert_eq!(a.l.to_rows(), b.l.to_rows(), "the same rows, byte for byte");
        assert_eq!(a.l.root(), b.l.root(), "the same ledger root");
        assert!(b.l.typed.is_empty(), "a Weights-only class is a legacy row, never a typed one");
        let row = &b.l.classes[&legacy];
        assert_eq!(row.header(legacy).program_root, program_root_v1(&w.program_bytes));
        assert_eq!(row.header(legacy).artifact_root, w.param_commitments.root());
        assert_eq!(row.header(legacy).plan_root, w.plan.root());
    }
}

#[test]
fn an_unarmed_ledger_refuses_every_spec_object_and_an_untyped_ledger_roots_as_before() {
    let fx = memory_v1(3);
    let d = k2_tir_v2_descriptor();
    let (spec, _) = memory_spec(&fx, &d);
    let mut w = W::new(false);
    let (_, ev) = w.register(spec, Some(ParamCommitmentsV1::of(&fx.params).root()));
    assert!(refused(&ev).unwrap().contains("KERNEL_NOT_ACTIVE [typed-roots]"), "{ev:?}");
    assert!(w.l.typed.is_empty() && w.l.opv.classes.is_empty());
    // Only the clock and the attested/admitted sets moved (consumer facts): the typed tables are empty, so the root is the historical form.
    assert_eq!(w.l.typed_root_parts(), vec![]);
    assert_eq!(w.l.root(), w.l.root_parts_v2().root(), "no typed table: the root is the OPV root form, unchanged");
    // An armed ledger with no typed row also roots as the historical form (only its config differs: the schedule).
    let armed = W::new(true);
    assert_eq!(armed.l.root(), armed.l.root_parts_v2().root());
    assert_ne!(armed.l.config_root(), W::new(false).l.config_root(), "arming changes the schedule, hence the config root");
}

// ---- 2. Memory ---------------------------------------------------------------------------------------------------------------

/// The memory spec over `fx`: slot param `mem0`/`fast.w0` (index 1, layer 0) ↔ state 0 in layer 0, eight steps at most.
fn memory_spec(fx: &TirSketchFixtureV1, d: &KernelDescriptorV1) -> (ComputationSpecV1, MemoryRootV1) {
    let w = weights(fx, d);
    let slots = vec![MemorySlotV1 { param: (1, Some(0)), state: (0, Some(0)) }];
    let m0 = slot_commitments_v1(&slots, &w.param_commitments).unwrap();
    let root = MemoryRootV1 {
        extension: k2_tr_v1_descriptor().digest(),
        line: [0x11; 64],
        initial_root: memory_root_v1(&slots, &m0),
        slots,
        max_steps: 8,
    };
    (ComputationSpecV1 { version: 1, mode: OPV, roots: vec![TypedRootV1::WeightsV1(w), TypedRootV1::MemoryV1(root.clone())] }, root)
}

struct Mem {
    w: W,
    fx: TirSketchFixtureV1,
    class: Digest,
    jobs: u8,
}

impl Mem {
    fn new(fx: TirSketchFixtureV1) -> Mem {
        let d = k2_tir_v2_descriptor();
        let (spec, _) = memory_spec(&fx, &d);
        let mut w = W::new(true);
        let (class, ev) = w.register(spec, Some(ParamCommitmentsV1::of(&fx.params).root()));
        assert_eq!(ev.last(), Some(&E::ClassRegistered { class }), "{ev:?}");
        assert!(w.l.opv.classes.contains(&class) && w.l.typed.lines.contains_key(&class));
        Mem { w, fx, class, jobs: 0 }
    }

    fn head(&self) -> (Vec<Digest>, Digest) {
        let line = &self.w.l.typed.lines[&self.class];
        (line.head.clone(), line.head_root)
    }

    fn job(&mut self, chunks: Vec<Vec<u32>>) -> (Digest, MemoryJobV1) {
        self.jobs += 1;
        let job = MemoryJobV1 { class: self.class, pre_root: self.head().1, chunks, nonce: [self.jobs; 64] };
        (self.w.post(SpecJobV1::Memory(job.clone())), job)
    }

    /// The producer's claim of `job` from the pre-state tensors `pre`, a step's trace edited by `lie`.
    fn produce(
        &self,
        job: &MemoryJobV1,
        producer: Digest,
        pre: &[Tensor],
        lie: impl FnMut(usize, &mut TraceV1),
    ) -> MemoryProductionV1 {
        let row = &self.w.l.typed.classes[&self.class];
        let SpecClassKindV1::Memory { rule, root, writers } = &row.kind else { panic!() };
        produce_memory_v1(&self.class, rule, root, writers, job, &producer, &self.fx.params, pre, 2, lie).unwrap()
    }

    fn m0(&self) -> Vec<Tensor> {
        vec![self.fx.params.tensors[&(1, Some(0))].clone()]
    }

    /// **The line head's tensors as a fresh node reads them**: from the rows (the carried post-state of the claim that advanced it)
    /// and the public artifact (`M0`) — never from any producer.
    fn chain_head(&self) -> Vec<Tensor> {
        self.w.rebuilt().memory_head_tensors_v1(&self.class, &self.fx.params).expect("the head is public")
    }
}

#[test]
fn memory_end_to_end_a_lie_in_one_step_is_convicted_and_memory_is_carried_across_two_jobs() {
    let mut m = Mem::new(memory_v1(3));
    let (m0_commitments, m0_root) = m.head();

    // Job 1 over M0: honest, Final, the line advances to its post-state.
    let (_, job1) = m.job(vec![vec![3, 17, 9], vec![5, 2], vec![30]]);
    let p1 = m.produce(&job1, P1, &m.m0(), |_, _| {});
    assert_eq!(p1.claim.step_roots[0], m0_root, "a claim starts from the line head");
    let (c1, ev) = m.w.commit(SpecClaimV1::Memory(p1.claim.clone()));
    assert!(ev.contains(&E::ClaimCommitted { claim: c1 }), "{ev:?}");
    assert!(matches!(m.w.state(&c1), ClaimStateV1::Challengeable { .. }), "an OPV claim: {:?}", m.w.state(&c1));
    let da1 = Da::memory(&p1, Some(&m.m0()));
    assert_eq!(check(&m.w.rebuilt(), c1, &da1, &m.fx.params, &()), OutsiderFindingV1::Clean, "an honest claim is clean");
    let final_at = m.w.l.opv.claims[&c1].window_end_daa;
    m.w.beat_to(final_at);
    assert!(matches!(m.w.state(&c1), ClaimStateV1::Final { .. }), "{:?}", m.w.state(&c1));
    let (head1, root1) = m.head();
    assert_ne!(head1, m0_commitments, "the line moved at Final");
    assert_eq!(root1, *p1.claim.step_roots.last().unwrap(), "to the claim's post-state");
    assert_eq!(m.w.l.typed.lines[&m.class].advances.len(), 1);
    // The head is public: a fresh node reads its tensors from the rows (job 1's carried post-state), not from job 1's producer.
    assert_eq!(m.w.l.typed.lines[&m.class].head_source, Some(c1));
    let pre2 = m.chain_head();
    assert_eq!(pre2, p1.post, "the chain holds exactly the memory job 1 left");

    // Job 2 over job 1's post-state: a lie in step 1 (one MatMul value at its first position), convicted by a fresh outsider.
    let (_, job2) = m.job(vec![vec![4, 4], vec![8, 1, 6], vec![2]]);
    assert_eq!(job2.pre_root, root1, "memory is carried: the next job runs on the head");
    let (s_at, n_at) = matmul_of(&m.fx.program);
    let lie = m.produce(&job2, P1, &pre2, |i, t| {
        if i == 1 {
            bump(&mut t.values[0][s_at][n_at], 1)
        }
    });
    let (c2, _) = m.w.commit(SpecClaimV1::Memory(lie.claim.clone()));
    // The outsider's step-0 pre-state comes from the chain too (no 0x40 material in its directory).
    let da2 = Da::memory(&lie, None);
    let finding = check(&m.w.rebuilt(), c2, &da2, &m.fx.params, &());
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Spec(bytes)) = &finding else { panic!("{finding:?}") };
    let fault: SpecFaultV1 = borsh::from_slice(bytes).unwrap();
    assert!(matches!(fault, SpecFaultV1::MemoryStep { step: 1, .. }), "localised to the lying step: {fault:?}");
    // The same proof filed against another step is dismissed (it names no instance of that step's record).
    let SpecFaultV1::MemoryStep { proof, .. } = &fault else { unreachable!() };
    let ev = m.w.file(OUT, c2, SpecFaultV1::MemoryStep { step: 0, proof: proof.clone() });
    assert!(ev.iter().any(|e| matches!(e, E::ProofDismissed { .. })), "{ev:?}");
    let ev = m.w.file(OUT, c2, fault);
    assert!(ev.iter().any(|e| matches!(e, E::Convicted { post_final: false, .. })), "{ev:?}");
    assert!(m.w.l.claims[&c2].convicted);
    assert_eq!(m.head().1, root1, "a convicted claim never moves the line");

    // The job is free again: P2 — which never held job 1's memory — produces from the chain alone; it finalizes and carries the
    // memory a second time.
    let honest = m.produce(&job2, P2, &pre2, |_, _| {});
    let (c3, _) = m.w.commit(SpecClaimV1::Memory(honest.claim.clone()));
    let da3 = Da::memory(&honest, None);
    assert_eq!(check(&m.w.rebuilt(), c3, &da3, &m.fx.params, &()), OutsiderFindingV1::Clean);
    let final_at = m.w.l.opv.claims[&c3].window_end_daa;
    m.w.beat_to(final_at);
    assert!(matches!(m.w.state(&c3), ClaimStateV1::Final { .. }));
    assert_eq!(m.head().1, *honest.claim.step_roots.last().unwrap(), "the head is job 2's post-state, computed from job 1's");
    assert_ne!(m.head().1, root1);
    assert_eq!((m.w.l.typed.lines[&m.class].head_source, m.chain_head()), (Some(c3), honest.post.clone()), "public again");
    // The post-state is not an independent run from M0: carrying memory changed the result.
    let fresh = m.produce(&job2, P2, &m.m0(), |_, _| {});
    assert_ne!(*fresh.claim.step_roots.last().unwrap(), m.head().1, "job 2 from M0 would leave another memory");
    m.w.rebuilt();
}

#[test]
fn memory_a_withheld_pre_state_is_a_default_and_a_served_one_completes_the_check() {
    let mut m = Mem::new(memory_v1(5));
    // Advance the line once, so the next job's pre-state is a committed value (job 1's carried post-state), not the registered M0.
    let (_, job1) = m.job(vec![vec![1, 2, 3]]);
    let p1 = m.produce(&job1, P1, &m.m0(), |_, _| {});
    let (c1, _) = m.w.commit(SpecClaimV1::Memory(p1.claim.clone()));
    let at = m.w.l.opv.claims[&c1].window_end_daa;
    m.w.beat_to(at);
    let head = m.chain_head();
    let mask = misaka_palw_kernel::trace::derived_mask_v1(&m.fx.program);

    // Two honest two-step claims over the new head. Step 0's pre-state is public on chain; step 1's pre-state — step 0's slot write at
    // its last position, a committed value of THIS claim — is the claim's DA. An outsider without it demands exactly that position.
    for (withhold, producer) in [(true, P1), (false, P2)] {
        let (_, job) = m.job(vec![vec![7, 7], vec![9]]);
        let prod = m.produce(&job, producer, &head, |_, _| {});
        let (c, _) = m.w.commit(SpecClaimV1::Memory(prod.claim.clone()));
        let at = prod.traces[0].values.len() as u32 - 1;
        let mut da = Da::memory(&prod, None);
        da.0.retain(|(stage, p, _, _), _| !(*stage == 0 && *p == at));
        assert_eq!(
            check(&m.w.rebuilt(), c, &da, &m.fx.params, &()),
            OutsiderFindingV1::Demand(vec![(0, at)]),
            "the outsider demands exactly step 1's pre-state (step 0's needs no demand: it is on chain)"
        );
        let ev = m.w.block(vec![obj(OUT, O::FileDemand { demander: OUT, claim: c, stage: 0, position: at })]);
        assert!(ev.iter().any(|e| matches!(e, E::DemandOpened { .. })), "{ev:?}");
        if withhold {
            let deadline = m.w.l.demands[&(c, 0, at)].deadline_daa;
            m.w.beat_to(deadline);
            assert!(matches!(m.w.state(&c), ClaimStateV1::Unavailable { producer_defaulted: true, .. }), "{:?}", m.w.state(&c));
            assert!(!m.w.l.claims[&c].convicted, "withholding is a default, never fraud");
            assert_eq!(m.head().1, *p1.claim.step_roots.last().unwrap(), "a defaulted claim never moves the line");
        } else {
            // The 0x40 obligation of RFC-0004 §II.2 stands as well (served from the chain's copy by the producer).
            let ev =
                m.w.block(vec![obj(OUT, O::FileDemand { demander: OUT, claim: c, stage: MEMORY_PRE_STATE_STAGE_V1, position: 0 })]);
            assert!(ev.iter().any(|e| matches!(e, E::DemandOpened { .. })), "{ev:?}");
            let whole = |t: &Tensor| MaterialResponseV1::Whole(TensorWireV1::of(t));
            let response = |values: &[Vec<Tensor>]| {
                let values = values
                    .iter()
                    .enumerate()
                    .map(|(s, o)| {
                        o.iter().enumerate().map(|(n, t)| if mask[s][n] { MaterialResponseV1::Omitted } else { whole(t) }).collect()
                    })
                    .collect();
                borsh::to_vec(&PositionResponseV1 { values, inputs: vec![] }).unwrap()
            };
            // A wrong value is rejected; the right position is served, and so is the pre-state.
            let mut wrong = prod.traces[0].values[at as usize].clone();
            let (s_at, n_at) = matmul_of(&m.fx.program);
            bump(&mut wrong[s_at][n_at], 0);
            let ev = m.w.block(vec![obj(producer, O::Respond { claim: c, stage: 0, position: at, bytes: response(&wrong) })]);
            assert!(ev.iter().any(|e| matches!(e, E::ResponseRejected { .. })), "{ev:?}");
            let right = response(&prod.traces[0].values[at as usize]);
            let pre = PositionResponseV1 { values: vec![head.iter().map(whole).collect()], inputs: vec![] };
            let ev = m.w.block(vec![
                obj(producer, O::Respond { claim: c, stage: 0, position: at, bytes: right }),
                obj(
                    producer,
                    O::Respond { claim: c, stage: MEMORY_PRE_STATE_STAGE_V1, position: 0, bytes: borsh::to_vec(&pre).unwrap() },
                ),
            ]);
            assert_eq!(ev.iter().filter(|e| matches!(e, E::Served { .. })).count(), 2, "{ev:?}");
            assert_eq!(check(&m.w.rebuilt(), c, &da, &m.fx.params, &()), OutsiderFindingV1::Clean, "complete from the chain alone");
        }
    }
}

#[test]
fn memory_a_lied_token_is_convicted_by_the_decode_court_of_its_step() {
    let mut m = Mem::new(memory_v1(9));
    let (_, job) = m.job(vec![vec![3, 1], vec![4, 1, 5]]);
    let mut prod = m.produce(&job, P1, &m.m0(), |_, _| {});
    prod.claim.generated[1] = (prod.claim.generated[1] + 1) % 32;
    let (c, _) = m.w.commit(SpecClaimV1::Memory(prod.claim.clone()));
    let finding = check(&m.w.rebuilt(), c, &Da::memory(&prod, Some(&m.m0())), &m.fx.params, &());
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Spec(bytes)) = finding else { panic!("{finding:?}") };
    let fault: SpecFaultV1 = borsh::from_slice(&bytes).unwrap();
    assert!(matches!(fault, SpecFaultV1::MemoryDecode { step: 1, .. }), "{fault:?}");
    let ev = m.w.file(OUT, c, fault);
    assert!(ev.iter().any(|e| matches!(e, E::Convicted { .. })), "{ev:?}");
}

#[test]
fn memory_a_lie_that_finalized_is_convicted_after_final_and_the_line_rolls_back() {
    let mut m = Mem::new(memory_v1(11));
    let (_, root0) = m.head();
    let (_, job1) = m.job(vec![vec![2, 2, 2]]);
    let (s_at, n_at) = matmul_of(&m.fx.program);
    let lie = m.produce(&job1, P1, &m.m0(), |_, t| bump(&mut t.values[1][s_at][n_at], 3));
    let (c1, _) = m.w.commit(SpecClaimV1::Memory(lie.claim.clone()));
    let at = m.w.l.opv.claims[&c1].window_end_daa;
    m.w.beat_to(at);
    assert!(matches!(m.w.state(&c1), ClaimStateV1::Final { .. }), "nobody prosecuted in the window");
    assert_ne!(m.head().1, root0, "the lie moved the line");
    // A later job is posted on the moved head and an honest claim of it (from the chain's copy) commits; then the old lie is convicted.
    let (_, job2) = m.job(vec![vec![1]]);
    assert_eq!(m.chain_head(), lie.post, "the lie's memory was public too");
    let p2 = m.produce(&job2, P2, &m.chain_head(), |_, _| {});
    let (c2, _) = m.w.commit(SpecClaimV1::Memory(p2.claim));
    let finding = check(&m.w.rebuilt(), c1, &Da::memory(&lie, Some(&m.m0())), &m.fx.params, &());
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Spec(bytes)) = finding else { panic!("{finding:?}") };
    let ev = m.w.file(OUT, c1, borsh::from_slice(&bytes).unwrap());
    assert!(ev.iter().any(|e| matches!(e, E::Convicted { post_final: true, .. })), "{ev:?}");
    assert_eq!(m.head().1, root0, "the line rolls back to the convicted claim's pre-state");
    assert_eq!(m.w.l.typed.lines[&m.class].head_source, None, "whose tensors are the registered M0's");
    assert_eq!(m.chain_head(), m.m0());
    // The later claim was not a fraud — it computed from the head of its time — but it is superseded: Final, paid, the line unmoved.
    let at = m.w.l.opv.claims[&c2].window_end_daa;
    m.w.beat_to(at);
    assert!(matches!(m.w.state(&c2), ClaimStateV1::Final { .. }));
    assert!(!m.w.l.claims[&c2].convicted);
    assert_eq!(m.head().1, root0, "a superseded claim never moves the line");
    m.w.rebuilt();
}

#[test]
fn memory_test_time_parameter_updates_are_a_parameter_subset_slot() {
    let mut m = Mem::new(memory_ttt_v1(13));
    let (_, job) = m.job(vec![vec![3, 9, 27], vec![1, 2]]);
    let honest = m.produce(&job, P1, &m.m0(), |_, _| {});
    assert_ne!(honest.post, m.m0(), "the weight slice was updated by the steps");
    let (c, _) = m.w.commit(SpecClaimV1::Memory(honest.claim.clone()));
    assert_eq!(check(&m.w.rebuilt(), c, &Da::memory(&honest, Some(&m.m0())), &m.fx.params, &()), OutsiderFindingV1::Clean);
    // A lie in the updated weights themselves (the StateWrite of step 0's last position) is convicted at step 0.
    let (_, job2) = m.job(vec![vec![5, 5], vec![6]]);
    let row = &m.w.l.typed.classes[&m.class];
    let SpecClassKindV1::Memory { writers, .. } = &row.kind else { panic!() };
    let (ws, wn) = writers[0];
    let lie = m.produce(&job2, P2, &m.m0(), |i, t| {
        if i == 0 {
            let last = t.values.len() - 1;
            bump(&mut t.values[last][ws as usize][wn as usize], 0)
        }
    });
    let (c2, _) = m.w.commit(SpecClaimV1::Memory(lie.claim.clone()));
    let finding = check(&m.w.rebuilt(), c2, &Da::memory(&lie, Some(&m.m0())), &m.fx.params, &());
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Spec(bytes)) = finding else { panic!("{finding:?}") };
    let fault: SpecFaultV1 = borsh::from_slice(&bytes).unwrap();
    assert!(matches!(fault, SpecFaultV1::MemoryStep { step: 0, .. }), "{fault:?}");
    assert!(m.w.file(OUT, c2, fault).iter().any(|e| matches!(e, E::Convicted { .. })));
}

#[test]
fn memory_registration_and_inclusion_refusals_are_by_name() {
    let fx = memory_v1(3);
    let d = k2_tir_v2_descriptor();
    let attest = Some(ParamCommitmentsV1::of(&fx.params).root());
    let (spec, root) = memory_spec(&fx, &d);
    let with = |f: &dyn Fn(&mut MemoryRootV1)| {
        let mut s = spec.clone();
        let TypedRootV1::MemoryV1(r) = &mut s.roots[1] else { unreachable!() };
        f(r);
        s
    };
    let mut fresh = W::new(true);
    let (_, ev) = fresh.register(spec.clone(), None);
    assert!(refused(&ev).unwrap().contains("not attested"), "{ev:?}");
    let mut w = W::new(true);
    for (bad, why) in [
        (with(&|r| r.initial_root = [1; 64]), "initial memory root"),
        (with(&|r| r.slots[0].param = (2, None)), "not of its state's type"),
        (with(&|r| r.slots[0].state = (0, None)), "no StateWrite"),
        (with(&|r| r.max_steps = 65), "1..=64 steps"),
        (with(&|r| r.extension = [7; 64]), "KERNEL_EXTENSION_REQUIRED"),
        (ComputationSpecV1 { mode: VerificationModeV1::PanelLicensed, ..spec.clone() }, "only under OptimisticPublicVerification"),
    ] {
        let (_, ev) = w.register(bad, attest);
        assert!(refused(&ev).is_some_and(|r| r.contains(why)), "{why}: {ev:?}");
    }
    assert!(w.l.typed.is_empty() && fresh.l.typed.is_empty(), "every refusal left the typed state untouched");

    // Inclusion: a fabricated boundary root, and a carried post-state that is not the committed write (another value, a missing
    // slot, another type), are refused before any court.
    let mut m = Mem::new(fx);
    let (_, job) = m.job(vec![vec![1, 2]]);
    let honest = m.produce(&job, P1, &m.m0(), |_, _| {});
    let mut prod = honest.clone();
    prod.claim.step_roots[1] = [9; 64];
    let (_, ev) = m.w.commit(SpecClaimV1::Memory(prod.claim));
    assert!(refused(&ev).unwrap().contains("boundary root 1"), "{ev:?}");
    let narrowed = {
        let t = &honest.post[0];
        Tensor::new(misaka_palw_tir::DType::I64, vec![t.data.len()], t.data.clone()).unwrap()
    };
    for (edit, why) in [
        (TensorWireV1::of(&m.m0()[0]), "not the committed write"),
        (TensorWireV1::of(&narrowed), "not of its param's type"),
        (TensorWireV1 { dtype: 0xEE, shape: vec![], bytes: vec![] }, "does not decode"),
    ] {
        let mut c = honest.claim.clone();
        c.post_state[0] = edit;
        let (_, ev) = m.w.commit(SpecClaimV1::Memory(c));
        assert!(refused(&ev).is_some_and(|r| r.contains(why)), "{why}: {ev:?}");
    }
    let mut c = honest.claim.clone();
    c.post_state.clear();
    let (_, ev) = m.w.commit(SpecClaimV1::Memory(c));
    assert!(refused(&ev).unwrap().contains("has 0 tensors"), "{ev:?}");
    let (id, ev) = m.w.commit(SpecClaimV1::Memory(honest.claim));
    assert!(ev.contains(&E::ClaimCommitted { claim: id }), "the honest claim still commits: {ev:?}");
    let _ = root;
}

// ---- 3. Retrieval ------------------------------------------------------------------------------------------------------------

fn corpus(n: usize, d: usize) -> Vec<RetrievalItemV1> {
    (0..n)
        .map(|i| RetrievalItemV1 {
            key: (0..d).map(|j| ((i * 7 + j * 3 + i * j) % 13) as i32 - 6).collect(),
            payload: vec![(i % 31) as u32, ((i * 5 + 1) % 31) as u32],
        })
        .collect()
}

fn retrieval_spec(data: &SnapshotDataV1, k: u16) -> (ComputationSpecV1, RetrievalRootV1) {
    let root = RetrievalRootV1 {
        extension: k2_tr_v1_descriptor().digest(),
        snapshot: data.snapshot,
        index: IndexV1::Flat,
        rule: RetrievalRuleV1::TopKCountingV1 { k, score_bits: 24 },
    };
    (ComputationSpecV1 { version: 1, mode: OPV, roots: vec![TypedRootV1::RetrievalV1(root.clone())] }, root)
}

#[test]
fn retrieval_a_wrong_item_and_a_missed_better_item_are_each_convicted_and_a_withheld_slice_defaults() {
    let data = SnapshotDataV1::new(corpus(40, 4), 4, 2, 8).unwrap();
    let (spec, root) = retrieval_spec(&data, 3);
    let mut w = W::new(true);
    let (class, ev) = w.register(spec, Some(data.snapshot.root()));
    assert_eq!(ev.last(), Some(&E::ClassRegistered { class }), "{ev:?}");
    let mut snapshots = BTreeMap::new();
    snapshots.insert(data.snapshot.root(), data.clone());
    let none = MapParams::default();

    let mut nonce = 0u8;
    let mut job = |w: &mut W, query: Vec<i32>| {
        nonce += 1;
        let j = RetrievalJobV1 { class, query, nonce: [nonce; 64] };
        (w.post(SpecJobV1::Retrieval(j.clone())), j)
    };

    // Honest: clean, Final.
    let (jid, j) = job(&mut w, vec![2, -1, 3, 1]);
    let honest = data.retrieve(&root, &j.query).unwrap();
    let (c_honest, _) = w.commit(SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: P1, result: honest.clone() }));
    assert_eq!(check(&w.rebuilt(), c_honest, &Da::default(), &none, &snapshots), OutsiderFindingV1::Clean);

    // A wrong item: entry 1's payload is not the snapshot's.
    let (jid, j) = job(&mut w, vec![1, 1, -2, 2]);
    let mut wrong = data.retrieve(&root, &j.query).unwrap();
    wrong[1].payload_digest = misaka_palw_kernel::spec::retrieval::payload_digest_v1(&[30, 30]);
    let (c, _) = w.commit(SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: P1, result: wrong }));
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Spec(bytes)) = check(&w.rebuilt(), c, &Da::default(), &none, &snapshots) else {
        panic!()
    };
    let fault: SpecFaultV1 = borsh::from_slice(&bytes).unwrap();
    assert!(matches!(
        &fault,
        SpecFaultV1::Retrieval { stage: 0, fault: misaka_palw_kernel::spec::retrieval::RetrievalFaultV1::WrongItem { index: 1, .. } }
    ));
    assert!(w.file(OUT, c, fault).iter().any(|e| matches!(e, E::Convicted { .. })));

    // A missed better item: the best item dropped, the rest shifted up.
    let (jid, j) = job(&mut w, vec![-3, 2, 2, 0]);
    let all = {
        let mut r4 = root.clone();
        r4.rule = RetrievalRuleV1::TopKCountingV1 { k: 4, score_bits: 24 };
        data.retrieve(&r4, &j.query).unwrap()
    };
    let missed = vec![all[1], all[2], all[3]];
    let (c, ev) = w.commit(SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: P2, result: missed }));
    assert!(ev.contains(&E::ClaimCommitted { claim: c }), "a well-ordered result commits: {ev:?}");
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Spec(bytes)) = check(&w.rebuilt(), c, &Da::default(), &none, &snapshots) else {
        panic!()
    };
    let fault: SpecFaultV1 = borsh::from_slice(&bytes).unwrap();
    let SpecFaultV1::Retrieval { fault: misaka_palw_kernel::spec::retrieval::RetrievalFaultV1::MissedBetter { id, .. }, .. } = &fault
    else {
        panic!("{fault:?}")
    };
    assert_eq!(*id, all[0].id);
    assert!(w.file(OUT, c, fault).iter().any(|e| matches!(e, E::Convicted { .. })));

    // A withheld slice: the public copy lacks slice 2 and the producer does not serve it.
    let (jid, j) = job(&mut w, vec![0, 1, 0, 1]);
    let result = data.retrieve(&root, &j.query).unwrap();
    let (c, _) = w.commit(SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: P2, result }));
    struct Holey<'a>(&'a SnapshotDataV1);
    impl misaka_palw_kernel::spec::outsider::SnapshotSourceV1 for Holey<'_> {
        fn item(&self, _: &Digest, id: u64) -> Option<RetrievalItemV1> {
            (!(16..24).contains(&id)).then(|| self.0.items[id as usize].clone())
        }
    }
    let stage = SNAPSHOT_STAGE_BASE_V1;
    assert_eq!(check(&w.rebuilt(), c, &Da::default(), &none, &Holey(&data)), OutsiderFindingV1::Demand(vec![(stage, 2)]));
    let ev = w.block(vec![obj(OUT, O::FileDemand { demander: OUT, claim: c, stage, position: 2 })]);
    assert!(ev.iter().any(|e| matches!(e, E::DemandOpened { .. })), "a snapshot slice is demandable: {ev:?}");
    let ev = w.block(vec![obj(OUT, O::FileDemand { demander: OUT, claim: c, stage, position: 5 })]);
    assert!(refused(&ev).unwrap().contains("no such position"), "no slice 5 of 5: {ev:?}");
    let deadline = w.l.demands[&(c, stage, 2)].deadline_daa;
    w.beat_to(deadline);
    assert!(matches!(w.state(&c), ClaimStateV1::Unavailable { producer_defaulted: true, .. }), "{:?}", w.state(&c));
    assert!(!w.l.claims[&c].convicted);

    // A served slice is public: the outsider completes from the chain.
    let (jid, j) = job(&mut w, vec![1, 0, 1, 0]);
    let result = data.retrieve(&root, &j.query).unwrap();
    let (c, _) = w.commit(SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: P1, result }));
    w.block(vec![obj(OUT, O::FileDemand { demander: OUT, claim: c, stage, position: 2 })]);
    let mut forged: misaka_palw_kernel::spec::retrieval::SliceResponseV1 =
        borsh::from_slice(&data.slice_response(2).unwrap()).unwrap();
    forged.items[3].key[0] += 1;
    let ev = w.block(vec![obj(P1, O::Respond { claim: c, stage, position: 2, bytes: borsh::to_vec(&forged).unwrap() })]);
    assert!(ev.iter().any(|e| matches!(e, E::ResponseRejected { class: "fake_opening", .. })), "{ev:?}");
    let ev = w.block(vec![obj(P1, O::Respond { claim: c, stage, position: 2, bytes: data.slice_response(2).unwrap() })]);
    assert!(ev.iter().any(|e| matches!(e, E::Served { .. })), "{ev:?}");
    assert_eq!(check(&w.rebuilt(), c, &Da::default(), &none, &Holey(&data)), OutsiderFindingV1::Clean);
    // An honest entry filed as wrong is dismissed (and costs the filer its fee).
    let (item, path) = data.open(honest[0].id).unwrap();
    let bogus = SpecFaultV1::Retrieval {
        stage: 0,
        fault: misaka_palw_kernel::spec::retrieval::RetrievalFaultV1::WrongItem { index: 0, item, path },
    };
    assert!(w.file(OUT, c_honest, bogus).iter().any(|e| matches!(e, E::ProofDismissed { .. })));
    w.rebuilt();
}

#[test]
fn retrieval_inclusion_refuses_a_misordered_or_short_result() {
    let data = SnapshotDataV1::new(corpus(20, 4), 4, 2, 8).unwrap();
    let (spec, root) = retrieval_spec(&data, 3);
    let mut w = W::new(true);
    let (class, _) = w.register(spec, Some(data.snapshot.root()));
    let j = RetrievalJobV1 { class, query: vec![1, 2, 3, 4], nonce: [1; 64] };
    let jid = w.post(SpecJobV1::Retrieval(j.clone()));
    let mut r = data.retrieve(&root, &j.query).unwrap();
    r.swap(0, 2);
    let (_, ev) = w.commit(SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: P1, result: r }));
    assert!(refused(&ev).unwrap().contains("rule's order"), "{ev:?}");
    let bad = RetrievalJobV1 { class, query: vec![1, 2, 3], nonce: [2; 64] };
    let ev = w.block(vec![spec_obj(REG, SpecObjectV1::PostJob { job: SpecJobV1::Retrieval(bad) })]);
    assert!(refused(&ev).unwrap().contains("elements"), "{ev:?}");
}

// ---- 4. Composite ------------------------------------------------------------------------------------------------------------

struct Comp {
    w: W,
    model: TirSketchFixtureV1,
    data: SnapshotDataV1,
    tool: Digest,
    model_class: Digest,
}

impl Comp {
    /// A retrieval tool (D = `dim`) and a registered Weights model (wide128: 32 tokens, 32 logits).
    fn new(dim: u16) -> Comp {
        let model = wide128_v1(7);
        let d = k2_tir_v2_descriptor();
        let mut w = W::new(true);
        let wr = weights(&model, &d);
        let model_class =
            ComputationSpecV1 { version: 1, mode: VerificationModeV1::PanelLicensed, roots: vec![TypedRootV1::WeightsV1(wr.clone())] };
        let (mc, ev) = w.register(model_class, Some(wr.param_commitments.root()));
        assert_eq!(ev.last(), Some(&E::ClassRegistered { class: mc }), "{ev:?}");
        let data = SnapshotDataV1::new(corpus(24, dim as usize), dim, 2, 8).unwrap();
        let (spec, _) = retrieval_spec(&data, 2);
        let (tool, ev) = w.register(spec, Some(data.snapshot.root()));
        assert_eq!(ev.last(), Some(&E::ClassRegistered { class: tool }), "{ev:?}");
        Comp { w, model, data, tool, model_class: mc }
    }

    fn root(&self) -> RetrievalRootV1 {
        let SpecClassKindV1::Retrieval { root } = &self.w.l.typed.classes[&self.tool].kind else { panic!() };
        root.clone()
    }

    fn register(&mut self, stages: Vec<CompositeStageV1>) -> Digest {
        let spec = ComputationSpecV1 {
            version: 1,
            mode: OPV,
            roots: vec![TypedRootV1::CompositeV1(CompositeRootV1 { extension: k2_tr_v1_descriptor().digest(), stages })],
        };
        let (class, ev) = self.w.register(spec, None);
        assert_eq!(ev.last(), Some(&E::ClassRegistered { class }), "{ev:?}");
        class
    }

    fn artifact(&self, stages: usize) -> Vec<MapParams> {
        (0..stages).map(|_| self.model.params.clone()).collect()
    }

    fn snapshots(&self) -> BTreeMap<Digest, SnapshotDataV1> {
        let mut m = BTreeMap::new();
        m.insert(self.data.snapshot.root(), self.data.clone());
        m
    }
}

#[test]
fn composite_a_lie_in_the_tool_stage_is_convicted_at_that_stage_and_a_filing_against_the_honest_model_is_dismissed() {
    let mut x = Comp::new(4);
    let stages = vec![
        CompositeStageV1 { component: x.tool, input: StageInputV1::Query(QuerySourceV1::JobQuery) },
        CompositeStageV1 {
            component: x.model_class,
            input: StageInputV1::Tokens {
                sources: vec![TokenSourceV1::JobPrompt, TokenSourceV1::StagePayloads { stage: 0 }],
                max_new_tokens: 2,
            },
        },
    ];
    let class = x.register(stages);
    let root = x.root();
    let row = x.w.l.classes[&x.model_class].clone();
    let job = CompositeJobV1 { class, prompt: vec![3, 17], query: vec![2, -1, 1, 3], nonce: [1; 64] };
    let jid = x.w.post(SpecJobV1::Composite(job.clone()));
    assert_eq!(jid, composite_job_id_v1(&job));

    // The tool lies: its result misses the best item; the model stage honestly consumes what the tool delivered.
    let all = {
        let mut r = root.clone();
        r.rule = RetrievalRuleV1::TopKCountingV1 { k: 3, score_bits: 24 };
        x.data.retrieve(&r, &job.query).unwrap()
    };
    let lied = vec![all[1], all[2]];
    let payloads: Vec<Vec<u32>> = lied.iter().map(|e| x.data.items[e.id as usize].payload.clone()).collect();
    let tool_stage = StageClaimV1::Retrieval { query: job.query.clone(), result: lied, payloads: payloads.clone() };
    let prompt: Vec<u32> = job.prompt.iter().copied().chain(payloads.into_iter().flatten()).collect();
    let (model_stage, trace) =
        produce_model_stage_v1(&x.model_class, &row, &jid, 1, prompt, 2, &P1, &x.model.params, 2, |_| {}).unwrap();
    let claim = CompositeClaimV1 { job_id: jid, producer_bond: P1, stages: vec![tool_stage, model_stage] };
    let (c, ev) = x.w.commit(SpecClaimV1::Composite(claim));
    assert!(ev.contains(&E::ClaimCommitted { claim: c }), "{ev:?}");
    let mut da = Da::default();
    da.stage(1, 0, &trace);
    let finding = check(&x.w.rebuilt(), c, &da, &x.artifact(2), &x.snapshots());
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Spec(bytes)) = finding else { panic!("{finding:?}") };
    let fault: SpecFaultV1 = borsh::from_slice(&bytes).unwrap();
    assert!(matches!(fault, SpecFaultV1::Retrieval { stage: 0, .. }), "localised to the tool stage: {fault:?}");
    // A decode filing against the honest model stage is dismissed; the tool's fault convicts.
    let honest_model = SpecFaultV1::StageDecode {
        stage: 1,
        index: 0,
        logits: TensorWireV1::of(&trace.values[trace.values.len() - 2][row.logits_at().0 as usize][row.logits_at().1 as usize]),
    };
    assert!(
        x.w.file(OUT, c, honest_model).iter().any(|e| matches!(e, E::ProofDismissed { .. })),
        "the honest model stage is not convicted"
    );
    assert!(x.w.file(OUT, c, fault).iter().any(|e| matches!(e, E::Convicted { .. })));
    x.w.rebuilt();
}

#[test]
fn composite_a_lie_in_the_model_stage_is_convicted_at_the_model_stage() {
    let mut x = Comp::new(4);
    let class = x.register(vec![
        CompositeStageV1 { component: x.tool, input: StageInputV1::Query(QuerySourceV1::JobQuery) },
        CompositeStageV1 {
            component: x.model_class,
            input: StageInputV1::Tokens {
                sources: vec![TokenSourceV1::StagePayloads { stage: 0 }, TokenSourceV1::JobPrompt],
                max_new_tokens: 1,
            },
        },
    ]);
    let root = x.root();
    let row = x.w.l.classes[&x.model_class].clone();
    let job = CompositeJobV1 { class, prompt: vec![9], query: vec![0, 1, 2, 3], nonce: [2; 64] };
    let jid = x.w.post(SpecJobV1::Composite(job.clone()));
    let tool_stage = produce_retrieval_stage_v1(&root, &x.data, &job.query).unwrap();
    let StageClaimV1::Retrieval { payloads, .. } = &tool_stage else { unreachable!() };
    let prompt: Vec<u32> = payloads.iter().flatten().copied().chain(job.prompt.iter().copied()).collect();
    let (s_at, n_at) = matmul_of(&x.model.program);
    let (model_stage, trace) = produce_model_stage_v1(&x.model_class, &row, &jid, 1, prompt, 1, &P1, &x.model.params, 2, |t| {
        bump(&mut t.values[1][s_at][n_at], 0)
    })
    .unwrap();
    let (c, _) =
        x.w.commit(SpecClaimV1::Composite(CompositeClaimV1 { job_id: jid, producer_bond: P1, stages: vec![tool_stage, model_stage] }));
    let mut da = Da::default();
    da.stage(1, 0, &trace);
    let finding = check(&x.w.rebuilt(), c, &da, &x.artifact(2), &x.snapshots());
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Spec(bytes)) = finding else { panic!("{finding:?}") };
    let fault: SpecFaultV1 = borsh::from_slice(&bytes).unwrap();
    assert!(matches!(fault, SpecFaultV1::StageKernel { stage: 1, .. }), "{fault:?}");
    assert!(x.w.file(OUT, c, fault).iter().any(|e| matches!(e, E::Convicted { .. })));
}

#[test]
fn composite_the_logits_to_query_edge_court_convicts_a_carried_query_that_is_not_the_upstream_logits() {
    // A model stage whose committed logits (32 values) are the query of a retrieval tool over 32-element keys.
    let mut x = Comp::new(32);
    let class = x.register(vec![
        CompositeStageV1 {
            component: x.model_class,
            input: StageInputV1::Tokens { sources: vec![TokenSourceV1::JobPrompt], max_new_tokens: 1 },
        },
        CompositeStageV1 { component: x.tool, input: StageInputV1::Query(QuerySourceV1::StageLogits { stage: 0 }) },
    ]);
    let root = x.root();
    let row = x.w.l.classes[&x.model_class].clone();
    for (n, lie) in [(1u8, false), (2u8, true)] {
        let job = CompositeJobV1 { class, prompt: vec![5, 6, 7], query: vec![], nonce: [n; 64] };
        let jid = x.w.post(SpecJobV1::Composite(job.clone()));
        let (model_stage, trace) =
            produce_model_stage_v1(&x.model_class, &row, &jid, 0, job.prompt.clone(), 1, &P1, &x.model.params, 2, |_| {}).unwrap();
        let (post, node) = row.logits_at();
        let mut query: Vec<i32> = trace.values.last().unwrap()[post as usize][node as usize].data.iter().map(|v| *v as i32).collect();
        if lie {
            query[3] += 1; // the carried query is not the upstream committed logits
        }
        let tool_stage = produce_retrieval_stage_v1(&root, &x.data, &query).unwrap();
        let claim = CompositeClaimV1 { job_id: jid, producer_bond: P1, stages: vec![model_stage, tool_stage] };
        let (c, ev) = x.w.commit(SpecClaimV1::Composite(claim));
        assert!(ev.contains(&E::ClaimCommitted { claim: c }), "an edge over a committed value is not checkable at inclusion: {ev:?}");
        let mut da = Da::default();
        da.stage(0, 0, &trace);
        let finding = check(&x.w.rebuilt(), c, &da, &x.artifact(2), &x.snapshots());
        if !lie {
            assert_eq!(finding, OutsiderFindingV1::Clean);
            continue;
        }
        let OutsiderFindingV1::Prosecute(ProsecutionV1::Spec(bytes)) = finding else { panic!("{finding:?}") };
        let fault: SpecFaultV1 = borsh::from_slice(&bytes).unwrap();
        assert!(matches!(fault, SpecFaultV1::Edge { stage: 1, .. }), "{fault:?}");
        // The edge court opens the upstream logits against stage 0's commitments: a forged opening is not authentic.
        let SpecFaultV1::Edge { logits, .. } = &fault else { unreachable!() };
        let mut forged = logits.decode().unwrap();
        forged.data[3] += 1;
        let ev = x.w.file(OUT, c, SpecFaultV1::Edge { stage: 1, logits: TensorWireV1::of(&forged) });
        assert!(ev.iter().any(|e| matches!(e, E::ProofDismissed { why, .. } if why.contains("NotAuthentic"))), "{ev:?}");
        assert!(x.w.file(OUT, c, fault).iter().any(|e| matches!(e, E::Convicted { .. })));
    }
}

#[test]
fn composite_registration_refuses_memory_components_nesting_and_kind_mismatches_by_name() {
    let mut x = Comp::new(4);
    let fx = memory_v1(3);
    let (mspec, _) = memory_spec(&fx, &k2_tir_v2_descriptor());
    let (mem, _) = x.w.register(mspec, Some(ParamCommitmentsV1::of(&fx.params).root()));
    let ext = k2_tr_v1_descriptor().digest();
    let try_reg = |w: &mut W, stages: Vec<CompositeStageV1>| {
        let spec = ComputationSpecV1 {
            version: 1,
            mode: OPV,
            roots: vec![TypedRootV1::CompositeV1(CompositeRootV1 { extension: ext, stages })],
        };
        w.register(spec, None).1
    };
    let tok = |c: Digest| CompositeStageV1 {
        component: c,
        input: StageInputV1::Tokens { sources: vec![TokenSourceV1::JobPrompt], max_new_tokens: 1 },
    };
    let q = |c: Digest| CompositeStageV1 { component: c, input: StageInputV1::Query(QuerySourceV1::JobQuery) };
    for (stages, why) in [
        (vec![tok(mem), tok(x.model_class)], "composite-memory"),
        (vec![tok(x.model_class), tok(x.tool)], "not the component's kind"),
        (vec![q(x.tool)], "2..=8 stages"),
        (
            vec![
                q(x.tool),
                CompositeStageV1 {
                    component: x.model_class,
                    input: StageInputV1::Tokens { sources: vec![TokenSourceV1::StageGenerated { stage: 0 }], max_new_tokens: 1 },
                },
            ],
            "no earlier stage of its kind",
        ),
        (vec![q(x.tool), q([0x77; 64])], "not a registered class"),
        (
            vec![
                q(x.tool),
                CompositeStageV1 { component: x.tool, input: StageInputV1::Query(QuerySourceV1::StageLogits { stage: 0 }) },
            ],
            "no earlier model stage",
        ),
    ] {
        let ev = try_reg(&mut x.w, stages);
        assert!(refused(&ev).is_some_and(|r| r.contains(why)), "{why}: {ev:?}");
    }
    // The logits edge needs the tool's key length to be the model's logits length (32), and this tool's keys have 4.
    let ev = try_reg(
        &mut x.w,
        vec![
            tok(x.model_class),
            CompositeStageV1 { component: x.tool, input: StageInputV1::Query(QuerySourceV1::StageLogits { stage: 0 }) },
        ],
    );
    assert!(refused(&ev).unwrap().contains("key length"), "{ev:?}");
}

// ---- 5. Bounds ---------------------------------------------------------------------------------------------------------------

#[test]
fn bounds_per_kind_fit_the_carriers_and_refuse_past_each_ceiling_by_name() {
    let p = policy().prosecution;
    let ext = k2_tr_v1_descriptor();
    // Memory: one step's material plus the pre-state per prosecution; sessions = steps × positions + 1.
    let m = Mem::new(memory_v1(3));
    let row = &m.w.l.typed.classes[&m.class];
    let SpecClassKindV1::Memory { rule, root, .. } = &row.kind else { panic!() };
    let b = row.bounds;
    assert_eq!(b.max_concurrent_sessions, 8 * MAX_POSITIONS + 1);
    assert_eq!(b.max_public_bytes, rule.bounds.max_public_bytes + 16 * 4 + 128, "one step's material and the 16 × i32 pre-state");
    let evidence = 64 * 16 + 64 * MAX_POSITIONS as u128 * 2;
    assert_eq!(
        b.max_retained_state,
        8 * (rule.bounds.max_retained_state + evidence) + 64 * 9 + 64 + 16 * 4 + 128,
        "8 steps' commitments and evidence, 9 boundary roots, the pre-state's commitment and the carried 16 × i32 post-state"
    );
    assert_eq!(
        (b.max_opening_bytes, b.max_court_work),
        (rule.bounds.max_opening_bytes, rule.bounds.max_court_work),
        "a step fault is a kernel fault"
    );
    assert_eq!(b.max_localization_rounds, 2);
    misaka_palw_kernel::ledger::carrier_fit_v1(&b, 1 << 20, 1 << 22, 1 << 24).unwrap();
    let mut long = root.clone();
    long.max_steps = 16; // 16 × 64 + 1 sessions > 1,024
    assert!(
        memory_bounds_v1(&rule.bounds, &rule.program, &rule.plan, &long, &p)
            .unwrap_err()
            .starts_with("BOUNDS_EXCEEDED [concurrent sessions]")
    );

    // Retrieval: one item and its path per filing; one slice per response; ⌈N/B⌉ sessions.
    let data = SnapshotDataV1::new(corpus(40, 4), 4, 2, 8).unwrap();
    let (_, r) = retrieval_spec(&data, 3);
    let b = retrieval_bounds_v1(&r, &ext, &p).unwrap();
    let item = 16 + 4 * 4 + 4 * 2 + 128;
    assert_eq!(b.max_opening_bytes, item + 64 * 6, "an item and a path of ⌈log2 40⌉ = 6 siblings");
    assert_eq!(b.max_concurrent_sessions, 5);
    assert_eq!(b.max_response_bytes, 8 * (item as u128 + 64 * 6 + 16) + 128);
    assert_eq!(retrieval_claim_material_bytes_v1(&r), 40 * item as u128, "detection reads the whole snapshot (the outsider's choice)");
    let mut huge = r.clone();
    huge.snapshot.items = 1025 * 8;
    assert!(retrieval_bounds_v1(&huge, &ext, &p).unwrap_err().starts_with("BOUNDS_EXCEEDED [concurrent sessions]"));
    let mut wide = r.clone();
    wide.snapshot.slice_items = 1 << 16;
    wide.snapshot.dim = 4096;
    wide.snapshot.max_payload = 4096;
    let wb = retrieval_bounds_v1(&wide, &ext, &p).unwrap();
    assert!(
        misaka_palw_kernel::ledger::carrier_fit_v1(&wb, 1 << 20, 1 << 22, 1 << 24).is_err(),
        "a slice past the carrier is not carriable"
    );
}
