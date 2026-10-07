//! **Pipeline claims on the in-process chain** (K2-TIR-v3; categories 7–9: output binding, segment/pipeline boundaries, media and
//! vision-language edges): a fresh outsider rebuilt by replay, reading only ledger state and public DA bytes, convicts a lie inside
//! a stage, a false edge, a false draw of `R` and a substituted generated id; a borrowed trace or another seed is refused at
//! inclusion; withheld stage positions are demanded and served on chain (stage inputs included).

#[path = "../../misaka-palw-tir/tests/v2common/mod.rs"]
mod v2common;

use std::collections::BTreeMap;

use misaka_palw_kernel::descriptor::{
    KernelScheduleV1, KernelStatusV1, k2_tir_v1_descriptor, k2_tir_v2_descriptor, k2_tir_v3_descriptor,
};
use misaka_palw_kernel::gate::ProsecutionPolicyV1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::DecodeRuleV1;
use misaka_palw_kernel::ledger::{
    KernelLedgerV1, LedgerBlockV1, LedgerEventV1 as E, LedgerPolicyV1, LedgerTxV1 as T, OutsiderFindingV1, OutsiderV1, ProsecutionV1,
    PublicSourceV1,
};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::pipeline::{PipelineTraceV1, build_pipeline_evidence_v1, pipeline_plan_v1, stage_view_v1, trace_pipeline_v1};
use misaka_palw_kernel::pipeline_public::{
    PipelineClaimV1, PipelineFaultWireV1, PipelineJobFactsV1, PipelineJobPostV1, PipelineRandomV1, StageCommitmentsV1,
};
use misaka_palw_kernel::public::{MaterialResponseV1, PositionResponseV1, TensorWireV1};
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir::Tensor;
use misaka_palw_tir::pipeline::{PipelineJob, TirPipelineV1, run_text_pipeline};
use misaka_palw_tir::program_v2::TirProgramV2;
use v2common::*;

const PRODUCER: Digest = [0xA1; 64];
const OUTSIDER: Digest = [0x0B; 64];
const RANDOM: PipelineRandomV1 = PipelineRandomV1 { seed: [4; 32], item: 0 };

fn policy() -> LedgerPolicyV1 {
    LedgerPolicyV1 {
        claim_collateral: 1000,
        demand_bond: 10,
        check_window_daa: 100,
        challenge_window_daa: 50,
        court_deadline_daa: 20,
        liability_daa: 200,
        exit_delay_daa: 30,
        dismissed_proof_fee: 5,
        accuser_reward_permille: 500,
        default_penalty: 100,
        claim_reward: 7,
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

/// Public DA: the bytes a producer published per stage, minus what it withholds.
#[derive(Default)]
struct Da {
    nodes: BTreeMap<(u8, u32, u16, u16), Vec<u8>>,
    inputs: BTreeMap<(u8, u32, u16), Vec<u8>>,
}

impl Da {
    fn publishing(trace: &PipelineTraceV1, withhold_positions: &[(u8, u32)]) -> Self {
        let mut da = Da::default();
        for (si, st) in trace.stages.iter().enumerate() {
            for (p, pos) in st.values.iter().enumerate() {
                if withhold_positions.contains(&(si as u8, p as u32)) {
                    continue;
                }
                for (s, occ) in pos.iter().enumerate() {
                    for (n, t) in occ.iter().enumerate() {
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

/// A stage position's response: its values and inputs from `trace`.
fn position(trace: &PipelineTraceV1, stage: u8, p: u32) -> Vec<u8> {
    let st = &trace.stages[stage as usize];
    let whole = |t: &Tensor| MaterialResponseV1::Whole(TensorWireV1::of(t));
    let values = st.values[p as usize].iter().map(|o| o.iter().map(whole).collect()).collect();
    let inputs = st.inputs.get(p as usize).map(|r| r.iter().map(whole).collect()).unwrap_or_default();
    borsh::to_vec(&PositionResponseV1 { values, inputs }).unwrap()
}

struct World {
    genesis: KernelLedgerV1,
    blocks: Vec<LedgerBlockV1>,
    l: KernelLedgerV1,
    class: Digest,
    p: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    params: ProgramParams,
}

struct Produced {
    claim: PipelineClaimV1,
    trace: PipelineTraceV1,
    tx: T,
}

impl World {
    fn new((p, programs): (TirPipelineV1, Vec<TirProgramV2>), seed: u64, decode: Option<DecodeRuleV1>) -> Self {
        let known = vec![k2_tir_v1_descriptor(), k2_tir_v2_descriptor(), k2_tir_v3_descriptor()];
        let genesis = KernelLedgerV1::genesis(policy(), armed(), known).unwrap();
        let params = ProgramParams(programs.iter().enumerate().map(|(i, prog)| materialize_v2(prog, seed + i as u64)).collect());
        let mut w = World { genesis: genesis.clone(), blocks: vec![], l: genesis, class: [0; 64], p, programs, params };
        let ev = w.block(
            1,
            vec![
                T::RegisterBond { bond: PRODUCER, collateral: 5000 },
                T::RegisterBond { bond: OUTSIDER, collateral: 1000 },
                w.register(decode),
            ],
        );
        w.class = ev
            .iter()
            .find_map(|e| if let E::ClassRegistered { class } = e { Some(*class) } else { None })
            .unwrap_or_else(|| panic!("{ev:?}"));
        w
    }

    fn register(&self, decode: Option<DecodeRuleV1>) -> T {
        let d = k2_tir_v3_descriptor();
        T::RegisterPipelineClass {
            descriptor: d.digest(),
            pipeline_bytes: self.p.encode(),
            program_bytes: self.programs.iter().map(TirProgramV2::encode).collect(),
            plan: pipeline_plan_v1(&d, &self.p, &self.programs).unwrap(),
            params: self.params.0.clone(),
            decode,
            network: [9; 64],
            ruleset: [3; 64],
        }
    }

    fn block(&mut self, daa: u64, txs: Vec<T>) -> Vec<E> {
        let b = LedgerBlockV1 { daa, txs };
        let before = self.l.events.len();
        self.l.apply_block(&b);
        self.blocks.push(b);
        self.l.events[before..].iter().map(|(_, e)| e.clone()).collect()
    }

    fn post(&mut self, daa: u64, job: &PipelineJob, max_new_tokens: u32, nonce: u8) -> PipelineJobPostV1 {
        let post = PipelineJobPostV1 {
            class_binding_id: self.class,
            facts: PipelineJobFactsV1::of(job, RANDOM),
            max_new_tokens,
            nonce: [nonce; 64],
        };
        let ev = self.block(daa, vec![T::PostPipelineJob { job: post.clone() }]);
        assert!(ev.contains(&E::JobPosted { job: post.id() }), "{ev:?}");
        post
    }

    /// The greedy generation of up to `n` ids.
    fn generate(&self, job: &PipelineJobPostV1, n: usize) -> Vec<u32> {
        run_text_pipeline(&self.p, &self.programs, &self.params, &RANDOM, &job.facts.job(&[]), &mut greedy(n)).unwrap().1
    }

    /// A claim delivering `generated`, traced with `random` (the job's own unless a test says otherwise), then edited by `lie`.
    fn produce(
        &self,
        job: &PipelineJobPostV1,
        generated: Vec<u32>,
        random: PipelineRandomV1,
        lie: impl FnOnce(&mut PipelineTraceV1),
    ) -> Produced {
        let d = k2_tir_v3_descriptor();
        let class = &self.l.pipeline_classes[&self.class];
        let full = job.facts.job(&generated);
        let mut trace = trace_pipeline_v1(&self.p, &self.programs, &self.params, &random, &full).unwrap();
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
            random.binding(),
            2,
        )
        .unwrap();
        let claim = PipelineClaimV1 {
            job_id: job.id(),
            producer_bond: PRODUCER,
            generated,
            output_root: ev.output_root,
            evidence_root: ev.root(),
        };
        let stages = trace.stages.iter().map(|t| StageCommitmentsV1::of(&t.evidence())).collect();
        let tx = T::CommitPipelineClaim { claim: claim.clone(), evidence: ev, stages };
        Produced { claim, trace, tx }
    }
}

/// **A fresh outsider**: replays the chain from genesis, then checks `claim` from the replayed state and `da` alone.
fn outsider(w: &World, claim: Digest, da: &Da) -> OutsiderFindingV1 {
    let fresh = KernelLedgerV1::replay(&w.genesis, &w.blocks);
    assert_eq!(fresh.root(), w.l.root());
    OutsiderV1 { ledger: &fresh, claim, material: da, salt: [0x5A; 64] }.check().unwrap()
}

fn convicted(ev: &[E]) -> Option<bool> {
    ev.iter().find_map(|e| if let E::Convicted { post_final, .. } = e { Some(*post_final) } else { None })
}

fn refused(ev: &[E]) -> Option<String> {
    ev.iter().find_map(|e| if let E::Refused { why, .. } = e { Some(why.clone()) } else { None })
}

fn fault(f: &OutsiderFindingV1) -> PipelineFaultWireV1 {
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Pipeline(b)) = f else { panic!("{f:?}") };
    borsh::from_slice(b).unwrap()
}

fn vlm_job() -> PipelineJob {
    PipelineJob { prompt: vec![3, PLACEHOLDER, PLACEHOLDER, 5], ..vision_job() }
}

#[test]
fn a_text_to_image_pipeline_with_r_finalizes_honest_and_a_stage_lie_an_edge_lie_or_a_false_draw_of_r_is_convicted() {
    let mut w = World::new(toy_pipeline(), 100, None);
    let job = w.post(2, &toy_job(), 0, 1);
    let honest = w.produce(&job, vec![], RANDOM, |_| {});
    let (id, da) = (honest.claim.id(), Da::publishing(&honest.trace, &[]));
    let ev = w.block(10, vec![honest.tx, T::PanelCovered { claim: id }]);
    assert!(ev.contains(&E::ClaimCommitted { claim: id }), "{ev:?}");
    assert_eq!(outsider(&w, id, &da), OutsiderFindingV1::Clean);
    let ev = w.block(60, vec![]);
    assert!(ev.contains(&E::Final { claim: id, reward: 7 }), "{ev:?}");

    // A lie inside the denoiser; a conditioning edge that is not the encoder's rows; a jitter that is not R's draw.
    let v = stage_view_v1(&w.programs[1]);
    let lies: Vec<(u8, Box<dyn Fn(&mut PipelineTraceV1)>)> = vec![
        (
            2,
            Box::new(move |t: &mut PipelineTraceV1| {
                let last = t.stages[1].values.len() - 1;
                let x = &mut t.stages[1].values[last][v.post_occurrence as usize][0];
                x.data[0] += if x.dtype.contains(x.data[0] + 1) { 1 } else { -1 };
            }),
        ),
        (3, Box::new(|t: &mut PipelineTraceV1| t.stages[1].inputs[0][IN_COND as usize].data[0] += 1)),
        (4, Box::new(|t: &mut PipelineTraceV1| t.stages[1].inputs[2][IN_JITTER as usize].data[3] ^= 1)),
    ];
    for (nonce, lie) in lies {
        let job = w.post(100 + 30 * nonce as u64, &toy_job(), 0, nonce);
        let bad = w.produce(&job, vec![], RANDOM, |t| lie(t));
        let (id, da) = (bad.claim.id(), Da::publishing(&bad.trace, &[]));
        w.block(110 + 30 * nonce as u64, vec![bad.tx, T::PanelCovered { claim: id }]);
        let f = outsider(&w, id, &da);
        match (nonce, fault(&f)) {
            (2, PipelineFaultWireV1::Stage { stage: 1, .. }) => {}
            (3, PipelineFaultWireV1::Edge { stage: 1, input, .. }) if input == IN_COND => {}
            (4, PipelineFaultWireV1::Edge { stage: 1, input, position: 2, .. }) if input == IN_JITTER => {}
            (n, other) => panic!("lie {n}: {other:?}"),
        }
        let OutsiderFindingV1::Prosecute(proof) = f else { unreachable!() };
        let ev = w.block(120 + 30 * nonce as u64, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
        assert_eq!(convicted(&ev), Some(false), "lie {nonce}: {ev:?}");
    }
}

#[test]
fn a_trace_drawn_from_another_seed_or_of_another_job_is_refused_and_an_r_that_is_not_the_jobs_seed_is_convicted() {
    let mut w = World::new(toy_pipeline(), 100, None);
    let job = w.post(2, &toy_job(), 0, 1);
    let other_job = w.post(3, &PipelineJob { scalars: vec![25, 1], ..toy_job() }, 0, 2);
    let other_seed = PipelineRandomV1 { seed: [5; 32], item: 0 };
    // Every value drawn and bound from another seed: the binding is not the job's seed.
    let seeded = w.produce(&job, vec![], other_seed, |_| {});
    // A valid trace of the other job offered as this job's.
    let borrowed = w.produce(&other_job, vec![], RANDOM, |_| {});
    let T::CommitPipelineClaim { evidence, stages, .. } = borrowed.tx.clone() else { unreachable!() };
    let as_this = PipelineClaimV1 { job_id: job.id(), ..borrowed.claim.clone() };
    // The delivered output is not the committed one.
    let honest = w.produce(&job, vec![], RANDOM, |_| {});
    let T::CommitPipelineClaim { evidence: hev, stages: hst, .. } = honest.tx.clone() else { unreachable!() };
    let other_output = PipelineClaimV1 { output_root: [1; 64], ..honest.claim.clone() };
    let ev = w.block(
        10,
        vec![
            seeded.tx,
            T::CommitPipelineClaim { claim: as_this, evidence, stages },
            T::CommitPipelineClaim { claim: other_output, evidence: hev, stages: hst },
        ],
    );
    let why: Vec<_> = ev.iter().filter_map(|e| if let E::Refused { why, .. } = e { Some(why.as_str()) } else { None }).collect();
    assert_eq!(why, ["binding fault WrongInput", "binding fault WrongInput", "binding fault WrongOutput"], "{ev:?}");
    assert!(w.l.claims.is_empty());

    // The binding names the job's seed but every R input is another seed's draw: the edge court convicts.
    let swapped = w.produce(&job, vec![], RANDOM, |t| {
        let full = toy_job();
        let (p, programs) = toy_pipeline();
        let params = ProgramParams(programs.iter().enumerate().map(|(i, prog)| materialize_v2(prog, 100 + i as u64)).collect());
        *t = trace_pipeline_v1(&p, &programs, &params, &other_seed, &full).unwrap();
    });
    let (id, da) = (swapped.claim.id(), Da::publishing(&swapped.trace, &[]));
    let ev = w.block(11, vec![swapped.tx, T::PanelCovered { claim: id }]);
    assert!(ev.contains(&E::ClaimCommitted { claim: id }), "{ev:?}");
    let f = outsider(&w, id, &da);
    assert!(matches!(fault(&f), PipelineFaultWireV1::Edge { stage: 1, .. }), "{f:?}");
    let OutsiderFindingV1::Prosecute(proof) = f else { unreachable!() };
    let ev = w.block(12, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some(false), "{ev:?}");
}

#[test]
fn a_vision_language_claim_with_a_substituted_id_is_convicted_by_the_decode_court_and_withheld_stages_are_demanded() {
    let mut w = World::new(vlm_pipeline(), 400, Some(DecodeRuleV1::Greedy));
    let job = w.post(2, &vlm_job(), 3, 1);
    let generated = w.generate(&job, 3);
    assert_eq!(generated.len(), 3);
    let honest = w.produce(&job, generated.clone(), RANDOM, |_| {});
    let (id, da) = (honest.claim.id(), Da::publishing(&honest.trace, &[]));
    w.block(10, vec![honest.tx, T::PanelCovered { claim: id }]);
    assert_eq!(outsider(&w, id, &da), OutsiderFindingV1::Clean);

    // The first delivered id substituted, the stream traced over it (self-consistent): the logits row before it selects another.
    let bound = w.programs[1].token_bound;
    let mut sub = generated.clone();
    sub[0] = (sub[0] + 1) % bound;
    let job2 = w.post(11, &vlm_job(), 3, 2);
    let bad = w.produce(&job2, sub, RANDOM, |_| {});
    let (id, da) = (bad.claim.id(), Da::publishing(&bad.trace, &[]));
    w.block(12, vec![bad.tx, T::PanelCovered { claim: id }]);
    let f = outsider(&w, id, &da);
    assert!(matches!(fault(&f), PipelineFaultWireV1::Decode { index: 0, .. }), "{f:?}");
    let OutsiderFindingV1::Prosecute(proof) = f else { unreachable!() };
    let ev = w.block(13, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some(false), "{ev:?}");

    // A false image edge whose vision stage is withheld: the outsider demands the stage position (its committed image input
    // included); the producer serves it on chain, and the edge court convicts from the served values.
    let job3 = w.post(20, &vlm_job(), 3, 3);
    let bad = w.produce(&job3, generated.clone(), RANDOM, |t| {
        t.stages[0].inputs[0][0].data[0] = (t.stages[0].inputs[0][0].data[0] + 1) % 256;
    });
    let (id, da) = (bad.claim.id(), Da::publishing(&bad.trace, &[(0, 0)]));
    let trace = bad.trace.clone();
    w.block(21, vec![bad.tx, T::PanelCovered { claim: id }]);
    assert_eq!(outsider(&w, id, &da), OutsiderFindingV1::Demand(vec![(0, 0)]));
    let ev = w.block(22, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 0 }]);
    assert!(ev.contains(&E::DemandOpened { claim: id, stage: 0, position: 0, deadline: 42 }), "{ev:?}");
    // The honest image instead of the committed one is not the committed input.
    let mut honest_input = trace.clone();
    honest_input.stages[0].inputs[0][0].data[0] = (honest_input.stages[0].inputs[0][0].data[0] + 255) % 256;
    let ev = w.block(23, vec![T::Respond { claim: id, stage: 0, position: 0, bytes: position(&honest_input, 0, 0) }]);
    assert_eq!(ev, vec![E::ResponseRejected { claim: id, stage: 0, position: 0, class: "wrong_bytes" }]);
    let ev = w.block(24, vec![T::Respond { claim: id, stage: 0, position: 0, bytes: position(&trace, 0, 0) }]);
    assert_eq!(ev, vec![E::Served { claim: id, stage: 0, position: 0 }]);
    let f = outsider(&w, id, &da);
    assert!(matches!(fault(&f), PipelineFaultWireV1::Edge { stage: 0, .. }), "{f:?}");
    let OutsiderFindingV1::Prosecute(proof) = f else { unreachable!() };
    let ev = w.block(25, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some(false), "{ev:?}");

    // Withheld and never served: the producer's availability default, not a conviction.
    let job4 = w.post(30, &vlm_job(), 3, 4);
    let held = w.produce(&job4, generated, RANDOM, |_| {});
    let id = held.claim.id();
    w.block(31, vec![held.tx, T::PanelCovered { claim: id }]);
    w.block(32, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 1, position: 2 }]);
    let ev = w.block(52, vec![]);
    assert_eq!(ev, vec![E::ProducerDefault { claim: id, stage: 1, position: 2, last: None, penalty: 100 }]);
    assert!(matches!(w.l.claims[&id].life.state, ClaimStateV1::Unavailable { producer_defaulted: true, .. }));
}

#[test]
fn a_pipeline_class_needs_its_decode_rule_and_the_media_pipeline_kernel_and_a_job_is_checked_at_posting() {
    // A text pipeline registered without a decode rule: its output would be unbound.
    let known = vec![k2_tir_v1_descriptor(), k2_tir_v2_descriptor(), k2_tir_v3_descriptor()];
    let mut l = KernelLedgerV1::genesis(policy(), armed(), known).unwrap();
    let mut w = World {
        genesis: l.clone(),
        blocks: vec![],
        l: l.clone(),
        class: [0; 64],
        p: vlm_pipeline().0,
        programs: vlm_pipeline().1,
        params: ProgramParams(vec![]),
    };
    w.params = ProgramParams(w.programs.iter().enumerate().map(|(i, prog)| materialize_v2(prog, 400 + i as u64)).collect());
    let ev = w.block(1, vec![w.register(None)]);
    assert!(refused(&ev).unwrap().contains("decode rule"), "{ev:?}");
    // Under a kernel without the media-pipeline family.
    let T::RegisterPipelineClass { pipeline_bytes, program_bytes, params, .. } = w.register(Some(DecodeRuleV1::Greedy)) else {
        unreachable!()
    };
    let d1 = k2_tir_v1_descriptor();
    l.apply_block(&LedgerBlockV1 {
        daa: 1,
        txs: vec![T::RegisterPipelineClass {
            descriptor: d1.digest(),
            pipeline_bytes,
            program_bytes,
            plan: pipeline_plan_v1(&k2_tir_v3_descriptor(), &w.p, &w.programs).unwrap(),
            params,
            decode: Some(DecodeRuleV1::Greedy),
            network: [9; 64],
            ruleset: [3; 64],
        }],
    });
    assert!(l.pipeline_classes.is_empty());

    // A registered text pipeline refuses a job without a generation budget or without a prompt.
    let mut w = World::new(vlm_pipeline(), 400, Some(DecodeRuleV1::Greedy));
    let bad = |job: &PipelineJob, n| PipelineJobPostV1 {
        class_binding_id: w.class,
        facts: PipelineJobFactsV1::of(job, RANDOM),
        max_new_tokens: n,
        nonce: [0; 64],
    };
    let (a, b) = (bad(&vlm_job(), 0), bad(&PipelineJob { prompt: vec![], ..vlm_job() }, 3));
    let ev = w.block(2, vec![T::PostPipelineJob { job: a }, T::PostPipelineJob { job: b }]);
    assert_eq!(ev.iter().filter(|e| matches!(e, E::Refused { .. })).count(), 2, "{ev:?}");
}
