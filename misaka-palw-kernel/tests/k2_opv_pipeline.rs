//! **RFC-0015 on pipeline classes (K2-TIR-v3, route tag 14)**: the mode is part of a pipeline class's identity as well, a claim of an
//! OPV pipeline class has no Panel, a fresh outsider (replaying the chain, reading public DA bytes only) convicts a lie in a stage
//! and an honest one finalizes by the window rule alone.

#[path = "../../misaka-palw-tir/tests/v2common/mod.rs"]
mod v2common;

mod common;

use std::collections::BTreeMap;

use common::chain::{Consumer, T, block_of};
use common::opv_world::opv_example;
use misaka_palw_kernel::descriptor::{
    KernelScheduleV1, KernelStatusV1, k2_tir_v1_descriptor, k2_tir_v2_descriptor, k2_tir_v3_descriptor,
};
use misaka_palw_kernel::gate::ProsecutionPolicyV1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{
    AuthV1, KernelLedgerV1, KernelRouteObjectV1 as O, LedgerBlockV1, LedgerEventV1 as E, LedgerPolicyV1, LedgerTxV1,
    OutsiderFindingV1, OutsiderV1, ProsecutionV1, PublicSourceV1,
};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::mode::{VerificationModeV1 as Mode, class_id_for_mode_v1};
use misaka_palw_kernel::pipeline::{
    PipelineTraceV1, build_pipeline_evidence_v1, pipeline_plan_v1, pipeline_root_v1, stage_view_v1, trace_pipeline_v1,
};
use misaka_palw_kernel::pipeline_public::{
    PipelineClaimV1, PipelineClassV1, PipelineFaultWireV1, PipelineJobFactsV1, PipelineJobPostV1, PipelineRandomV1, StageCommitmentsV1,
};
use misaka_palw_kernel::public::TensorWireV1;
use misaka_palw_kernel::trace::{ParamCommitmentsV1, derived_mask_v1};
use misaka_palw_tir::Tensor;
use misaka_palw_tir::pipeline::{PipelineJob, TirPipelineV1};
use misaka_palw_tir::program_v2::TirProgramV2;
use v2common::*;

const PRODUCER: Digest = [0xA1; 64];
const OUTSIDER: Digest = [0x0B; 64];
const RANDOM: PipelineRandomV1 = PipelineRandomV1 { seed: [4; 32], item: 0 };
const OPV: Mode = Mode::OptimisticPublicVerification;

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
        job_fee: 2,
        job_escrow_ttl_daa: 300,
        max_adjudications_per_block: 64,
        max_court_work_per_block: u64::MAX,
        claim_seal_delay_daa: 1,
        seal_ttl_daa: 100,
        prosecution: ProsecutionPolicyV1 {
            court_deadline_daa: 20,
            max_sessions_per_claim: 1 << 10,
            max_public_bytes: 1 << 50,
            max_verifier_ram: 1 << 50,
            max_retained_state: 1 << 40,
        },
    }
}

fn armed() -> KernelScheduleV1 {
    KernelScheduleV1::default().with(k2_tir_v3_descriptor().digest(), KernelStatusV1::Active { since_daa: 0 })
}

#[derive(Default)]
struct Da {
    nodes: BTreeMap<(u8, u32, u16, u16), Vec<u8>>,
    inputs: BTreeMap<(u8, u32, u16), Vec<u8>>,
}

impl Da {
    fn publishing(trace: &PipelineTraceV1, masks: &[Vec<Vec<bool>>]) -> Self {
        let mut da = Da::default();
        for (si, st) in trace.stages.iter().enumerate() {
            for (p, pos) in st.values.iter().enumerate() {
                for (s, occ) in pos.iter().enumerate() {
                    for (n, t) in occ.iter().enumerate().filter(|(n, _)| !masks[si][s][*n]) {
                        da.nodes.insert((si as u8, p as u32, s as u16, n as u16), borsh::to_vec(&TensorWireV1::of(t)).unwrap());
                    }
                }
                for (k, t) in st.inputs.get(p).into_iter().flatten().enumerate() {
                    da.inputs.insert((si as u8, p as u32, k as u16), borsh::to_vec(&TensorWireV1::of(t)).unwrap());
                }
            }
        }
        da
    }

    fn wire(b: &[u8]) -> Option<Tensor> {
        borsh::from_slice::<TensorWireV1>(b).ok()?.decode().ok()
    }
}

impl PublicSourceV1 for Da {
    fn node(&self, stage: u8, p: u32, s: u16, n: u16) -> Option<Tensor> {
        Self::wire(self.nodes.get(&(stage, p, s, n))?)
    }
    fn input(&self, stage: u8, p: u32, k: u16) -> Option<Tensor> {
        Self::wire(self.inputs.get(&(stage, p, k))?)
    }
}

struct World {
    genesis: KernelLedgerV1,
    blocks: Vec<LedgerBlockV1>,
    l: KernelLedgerV1,
    consumer: Consumer,
    class: Digest,
    p: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    params: ProgramParams,
    events: Vec<E>,
}

struct Produced {
    claim: PipelineClaimV1,
    trace: PipelineTraceV1,
    tx: T,
}

impl World {
    /// `opv`: register the class under `OptimisticPublicVerification` (tag 14); otherwise by tag 2 (Panel-licensed).
    fn new(opv: Option<misaka_palw_kernel::opv::OpvPolicyV1>) -> Self {
        let (p, programs) = toy_pipeline();
        let known = vec![k2_tir_v1_descriptor(), k2_tir_v2_descriptor(), k2_tir_v3_descriptor()];
        let mut genesis = KernelLedgerV1::genesis(policy(), armed(), known).unwrap();
        if let Some(o) = opv {
            genesis = genesis.with_opv_policy(o).unwrap();
        }
        let params = ProgramParams(programs.iter().enumerate().map(|(i, prog)| materialize_v2(prog, 100 + i as u64)).collect());
        let mut w = World {
            genesis: genesis.clone(),
            blocks: vec![],
            l: genesis,
            consumer: Consumer::default(),
            class: [0; 64],
            p,
            programs,
            params,
            events: vec![],
        };
        let mut txs = vec![
            LedgerTxV1::SyncBond { bond: PRODUCER, collateral: 5000 },
            LedgerTxV1::SyncBond { bond: OUTSIDER, collateral: 1000 },
            LedgerTxV1::SyncBond { bond: common::chain::POSTER, collateral: common::chain::POSTER_COLLATERAL },
        ];
        txs.extend(w.register_txs(opv.map(|_| OPV)));
        let ev = w.block_raw(1, txs);
        if let Some(class) = ev.iter().find_map(|e| if let E::ClassRegistered { class } = e { Some(*class) } else { None }) {
            w.class = class;
        }
        w
    }

    fn register_txs(&self, mode: Option<Mode>) -> Vec<LedgerTxV1> {
        let d = k2_tir_v3_descriptor();
        let plan = pipeline_plan_v1(&d, &self.p, &self.programs).unwrap();
        let pcs: Vec<ParamCommitmentsV1> = self.params.0.iter().map(ParamCommitmentsV1::of).collect();
        let mut v: Vec<LedgerTxV1> = pcs.iter().map(|p| LedgerTxV1::AttestArtifact { artifact_root: p.root() }).collect();
        let (pipeline_bytes, program_bytes) = (self.p.encode(), self.programs.iter().map(TirProgramV2::encode).collect());
        if let Some(mode) = mode {
            // The network's policy admits the class (by its mode-bound id) before anyone registers it under the mode.
            let binding = PipelineClassV1 {
                descriptor_digest: d.digest(),
                pipeline_root: pipeline_root_v1(&self.p, &self.programs),
                plan_root: plan.root(),
                artifact_roots: pcs.iter().map(ParamCommitmentsV1::root).collect(),
                decode: None,
            };
            v.push(LedgerTxV1::AdmitOptimisticClass { class: class_id_for_mode_v1(&binding.class_binding_id(), mode) });
        }
        let object = match mode {
            Some(mode) => O::RegisterPipelineClassV2 {
                mode,
                descriptor: d.digest(),
                pipeline_bytes,
                program_bytes,
                plan,
                param_commitments: pcs,
                decode: None,
            },
            None => O::RegisterPipelineClass {
                descriptor: d.digest(),
                pipeline_bytes,
                program_bytes,
                plan,
                param_commitments: pcs,
                decode: None,
            },
        };
        v.push(LedgerTxV1::Object { auth: AuthV1 { signer_bond: PRODUCER }, object });
        v
    }

    fn block_raw(&mut self, daa: u64, txs: Vec<LedgerTxV1>) -> Vec<E> {
        let b = LedgerBlockV1 { daa, txs };
        let ev = self.consumer.apply(&mut self.l, &b);
        self.blocks.push(b);
        self.events.extend(ev.iter().cloned());
        self.l.opv_invariants().unwrap();
        ev
    }

    fn masks(&self) -> Vec<Vec<Vec<bool>>> {
        self.p.stages.iter().map(|st| derived_mask_v1(&stage_view_v1(&self.programs[st.program as usize]).view)).collect()
    }

    fn block(&mut self, daa: u64, txs: Vec<T>) -> Vec<E> {
        let seals = common::chain::seals_for(&txs);
        if !seals.is_empty() && self.l.daa < daa {
            let sb = block_of(self.l.daa, seals, PRODUCER);
            self.consumer.apply(&mut self.l, &sb);
            self.blocks.push(sb);
        }
        let b = block_of(daa, txs, PRODUCER);
        let ev = self.consumer.apply(&mut self.l, &b);
        self.blocks.push(b);
        self.l.opv_invariants().unwrap();
        ev
    }

    fn post(&mut self, daa: u64, job: &PipelineJob, nonce: u8) -> PipelineJobPostV1 {
        let post = PipelineJobPostV1 {
            class_binding_id: self.class,
            facts: PipelineJobFactsV1::of(job, RANDOM),
            max_new_tokens: 0,
            nonce: [nonce; 64],
        };
        let ev = self.block(daa, vec![T::PostPipelineJob { job: post.clone() }]);
        assert!(ev.contains(&E::JobPosted { job: post.id() }), "{ev:?}");
        post
    }

    fn produce(&self, job: &PipelineJobPostV1, lie: impl FnOnce(&mut PipelineTraceV1)) -> Produced {
        let d = k2_tir_v3_descriptor();
        let class = &self.l.pipeline_classes[&self.class];
        let full = job.facts.job(&[]);
        let mut trace = trace_pipeline_v1(&self.p, &self.programs, &self.params, &RANDOM, &full).unwrap();
        lie(&mut trace);
        let pcs: Vec<ParamCommitmentsV1> = self.params.0.iter().map(ParamCommitmentsV1::of).collect();
        let ev = build_pipeline_evidence_v1(
            class.header(self.class),
            &d,
            &self.p,
            &self.programs,
            &class.plan,
            &pcs,
            &trace,
            &full,
            RANDOM.binding(),
            2,
        )
        .unwrap();
        let claim = PipelineClaimV1 {
            job_id: job.id(),
            producer_bond: PRODUCER,
            generated: vec![],
            output_root: ev.output_root,
            evidence_root: ev.root(),
        };
        let stages = trace.stages.iter().map(|t| StageCommitmentsV1::of(&t.evidence())).collect();
        let tx = T::CommitPipelineClaim { claim: claim.clone(), evidence: ev, stages };
        Produced { claim, trace, tx }
    }
}

fn outsider(w: &World, claim: Digest, da: &Da) -> OutsiderFindingV1 {
    let fresh = KernelLedgerV1::replay(&w.genesis, &w.blocks);
    assert_eq!(fresh.root(), w.l.root());
    OutsiderV1 { ledger: &fresh, claim, material: da, artifact: &w.params.0, salt: [0x5A; 64] }.check().unwrap()
}

fn refused(ev: &[E]) -> Option<String> {
    ev.iter().find_map(|e| if let E::Refused { why, .. } = e { Some(why.clone()) } else { None })
}

#[test]
fn a_pipeline_class_binds_its_mode_and_an_optimistic_pipeline_claim_finalizes_with_no_panel() {
    let mut w = World::new(Some(opv_example()));
    assert_ne!(w.class, [0; 64], "{:?}", w.events);
    assert_eq!(w.l.mode_of_class(&w.class), OPV);
    // The same pipeline under the legacy mode is another class: its id is the historical one.
    let legacy = World::new(None);
    let binding: PipelineClassV1 = legacy.l.pipeline_classes[&legacy.class].binding.clone();
    assert_eq!(legacy.class, binding.class_binding_id(), "a Panel-licensed pipeline class keeps its historical id");
    assert_eq!(w.class, class_id_for_mode_v1(&binding.class_binding_id(), OPV));
    assert_ne!(w.class, legacy.class);

    let job = w.post(2, &toy_job(), 1);
    let honest = w.produce(&job, |_| {});
    let (id, da) = (honest.claim.id(), Da::publishing(&honest.trace, &w.masks()));
    let ev = w.block(10, vec![honest.tx]);
    assert!(ev.contains(&E::ClaimCommitted { claim: id }), "{ev:?}");
    assert_eq!(w.l.claims[&id].life.state, ClaimStateV1::Challengeable { since_daa: 10, window_end_daa: 60 });
    assert_eq!(outsider(&w, id, &da), OutsiderFindingV1::Clean);
    let ev = w.block(11, vec![T::PanelCovered { claim: id }]);
    assert!(refused(&ev).unwrap().contains("no Panel tally"), "{ev:?}");
    let ev = w.block(60, vec![]);
    assert_eq!(ev, vec![E::Final { claim: id, reward: 7 }]);
    let r = w.l.final_receipt(&id).unwrap();
    assert_eq!(r.mode, OPV);
    assert_eq!(r.source_profile_id, w.class);
}

#[test]
fn a_lie_in_an_optimistic_pipeline_stage_is_localized_and_convicted_by_a_fresh_outsider() {
    let mut w = World::new(Some(opv_example()));
    let job = w.post(2, &toy_job(), 1);
    let v = stage_view_v1(&w.programs[1]);
    let bad = w.produce(&job, |t| {
        let last = t.stages[1].values.len() - 1;
        let x = &mut t.stages[1].values[last][v.post_occurrence as usize][0];
        x.data[0] += if x.dtype.contains(x.data[0] + 1) { 1 } else { -1 };
    });
    let (id, da) = (bad.claim.id(), Da::publishing(&bad.trace, &w.masks()));
    w.block(10, vec![bad.tx]);
    let f = outsider(&w, id, &da);
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Pipeline(bytes)) = f else { panic!("{f:?}") };
    let wire: PipelineFaultWireV1 = borsh::from_slice(&bytes).unwrap();
    assert!(matches!(wire, PipelineFaultWireV1::Stage { stage: 1, .. }), "{wire:?}");
    let ev = w.block(30, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: ProsecutionV1::Pipeline(bytes) }]);
    assert!(
        ev.iter().any(|e| matches!(e, E::Convicted { slashed: 1000, accuser_reward: 500, post_final: false, .. })),
        "the OPV reservation is slashed and the outsider is paid its share: {ev:?}"
    );
    assert!(matches!(w.l.claims[&id].life.state, ClaimStateV1::Convicted { .. }));
    assert_eq!(w.l.opv_live_counts(&PRODUCER), (0, 0));
    // A lie that the OPV class's Panel-licensed twin would also catch is a Panel-free conviction here: the twin's evidence header
    // names another class and cannot be committed on this job.
    let legacy = World::new(None);
    let lj = legacy.l.pipeline_classes[&legacy.class].header(legacy.class);
    assert_ne!(lj.class_binding_id, w.class);
}
