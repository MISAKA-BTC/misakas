//! **Agent C4 — wire fuzz of the PIPELINE (K2-TIR-v3) objects**: `RegisterPipelineClass`, `PostPipelineJob`, `CommitPipelineClaim`,
//! `FileProof/Pipeline` (stage, edge and decode wires) and a stage-position `Respond` carrying stage inputs, mutated at the byte
//! level and structurally (extreme tensor shapes / dtypes / openings inside otherwise valid proofs).
//!
//! Invariants per mutant (applied to a fresh clone of one ledger through the strict decoder): I1 no panic; I2 a refusal leaves the
//! root byte-identical; I3 an honest claim is never convicted; I4 every settlement instruction an applied mutant emits is
//! applicable to the independent bond book and the book agrees with the ledger's view (conservation).
//! Expected BEFORE running: PASS after the shape-arithmetic fix (098ffc749), the pipeline wires reuse `TensorWireV1::decode`.

#![allow(warnings)]
#[path = "../../tir/core/tests/v2common/mod.rs"]
mod v2common;

mod common;
#[path = "adv_c4_support/mod.rs"]
mod support;

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;

use common::chain::{T, block_of, seals_for};
use misaka_palw_kernel::descriptor::{
    KernelScheduleV1, KernelStatusV1, k2_tir_v1_descriptor, k2_tir_v2_descriptor, k2_tir_v3_descriptor,
};
use misaka_palw_kernel::gate::ProsecutionPolicyV1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{
    AuthV1, KernelLedgerV1, KernelRouteObjectV1 as O, LedgerEventV1 as E, LedgerPolicyV1, OutsiderFindingV1, OutsiderV1,
    ProsecutionV1, PublicSourceV1,
};
use misaka_palw_kernel::pipeline::{PipelineTraceV1, build_pipeline_evidence_v1, pipeline_plan_v1, stage_view_v1, trace_pipeline_v1};
use misaka_palw_kernel::pipeline_public::{
    PipelineClaimV1, PipelineFaultWireV1, PipelineJobFactsV1, PipelineJobPostV1, PipelineRandomV1, StageCommitmentsV1,
};
use misaka_palw_kernel::public::{MaterialResponseV1, PositionResponseV1, TensorWireV1};
use misaka_palw_kernel::trace::{ParamCommitmentsV1, derived_mask_v1};
use misaka_palw_tir::Tensor;
use misaka_palw_tir::pipeline::TirPipelineV1;
use misaka_palw_tir::program_v2::TirProgramV2;
use support::{Book, fold};
use v2common::*;

const PRODUCER: Digest = [0xA1; 64];
const OUTSIDER: Digest = [0x0B; 64];
const SPAM1: Digest = [0x51; 64];
const RANDOM: PipelineRandomV1 = PipelineRandomV1 { seed: [4; 32], item: 0 };

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
        // GAP-5 / seal / prosecution-reserve terms added after round 1 was written: the values the in-crate pipeline suites use
        // (`k2_ledger_pipeline.rs`).
        job_fee: 2,
        job_escrow_ttl_daa: 300,
        prosecution_reserve_permille: 500,
        seal_deposit: 1,
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

fn position(masks: &[Vec<Vec<bool>>], trace: &PipelineTraceV1, stage: u8, p: u32) -> Vec<u8> {
    let st = &trace.stages[stage as usize];
    let mask = &masks[stage as usize];
    let whole = |t: &Tensor| MaterialResponseV1::Whole(TensorWireV1::of(t));
    let values = st.values[p as usize]
        .iter()
        .enumerate()
        .map(|(s, o)| o.iter().enumerate().map(|(n, t)| if mask[s][n] { MaterialResponseV1::Omitted } else { whole(t) }).collect())
        .collect();
    let inputs = st.inputs.get(p as usize).map(|r| r.iter().map(whole).collect()).unwrap_or_default();
    borsh::to_vec(&PositionResponseV1 { values, inputs }).unwrap()
}

#[allow(dead_code)]
struct PW {
    genesis: KernelLedgerV1,
    l: KernelLedgerV1,
    book: Book,
    class: Digest,
    p: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    params: ProgramParams,
}

impl PW {
    fn new() -> Self {
        let (p, programs) = toy_pipeline();
        let known = vec![k2_tir_v1_descriptor(), k2_tir_v2_descriptor(), k2_tir_v3_descriptor()];
        let armed = KernelScheduleV1::default().with(k2_tir_v3_descriptor().digest(), KernelStatusV1::Active { since_daa: 0 });
        let genesis = KernelLedgerV1::genesis(policy(), armed, known).unwrap();
        let params = ProgramParams(programs.iter().enumerate().map(|(i, prog)| materialize_v2(prog, 100 + i as u64)).collect());
        let mut w = PW { genesis: genesis.clone(), l: genesis, book: Book::default(), class: [0; 64], p, programs, params };
        let d = k2_tir_v3_descriptor();
        let reg = T::RegisterPipelineClass {
            descriptor: d.digest(),
            pipeline_bytes: w.p.encode(),
            program_bytes: w.programs.iter().map(TirProgramV2::encode).collect(),
            plan: pipeline_plan_v1(&d, &w.p, &w.programs).unwrap(),
            params: w.params.0.clone(),
            decode: None,
        };
        let ev = w.blk(
            1,
            vec![
                T::RegisterBond { bond: PRODUCER, collateral: 5000 },
                T::RegisterBond { bond: OUTSIDER, collateral: 1000 },
                T::RegisterBond { bond: SPAM1, collateral: 1000 },
                // GAP-5: jobs have a price; the harness's poster bond posts (and pays for) every job, as every world registers it.
                T::RegisterBond { bond: common::chain::POSTER, collateral: common::chain::POSTER_COLLATERAL },
                reg,
            ],
        );
        w.class = ev.iter().find_map(|e| if let E::ClassRegistered { class } = e { Some(*class) } else { None }).expect("class");
        w.book = Book::from_ledger(&w.l);
        w
    }

    fn blk(&mut self, daa: u64, txs: Vec<T>) -> Vec<E> {
        let seals = seals_for(&txs);
        if !seals.is_empty() && self.l.daa < daa {
            let sb = block_of(self.l.daa, seals, PRODUCER);
            let book = if self.book.coll.is_empty() && self.l.bonds.is_empty() { None } else { Some(&mut self.book) };
            fold(&mut self.l, &sb, book);
        }
        let b = block_of(daa, txs, PRODUCER);
        let book = if self.book.coll.is_empty() && self.l.bonds.is_empty() { None } else { Some(&mut self.book) };
        fold(&mut self.l, &b, book)
    }

    fn masks(&self) -> Vec<Vec<Vec<bool>>> {
        self.p.stages.iter().map(|st| derived_mask_v1(&stage_view_v1(&self.programs[st.program as usize]).view)).collect()
    }

    fn post(&mut self, daa: u64, nonce: u8) -> PipelineJobPostV1 {
        let post = PipelineJobPostV1 {
            class_binding_id: self.class,
            facts: PipelineJobFactsV1::of(&toy_job(), RANDOM),
            max_new_tokens: 0,
            nonce: [nonce; 64],
        };
        let ev = self.blk(daa, vec![T::PostPipelineJob { job: post.clone() }]);
        assert!(ev.contains(&E::JobPosted { job: post.id() }), "{ev:?}");
        post
    }

    fn produce(&self, job: &PipelineJobPostV1, lie: impl FnOnce(&mut PipelineTraceV1)) -> (PipelineClaimV1, PipelineTraceV1, T) {
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
        (claim, trace, tx)
    }
}

static PANICS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

fn install_hook() {
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture().to_string();
        let caller = bt
            .lines()
            .filter(|l| l.contains("misaka_palw") && !l.contains("adv_c4") && !l.contains("{{closure}}"))
            .map(|l| l.trim().to_string())
            .next()
            .unwrap_or_default();
        let loc = format!("{} <- {}", info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default(), caller);
        let msg =
            info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned());
        PANICS.lock().unwrap().push((loc, msg.unwrap_or_default()));
    }));
}

fn tensor_mutants(t: &TensorWireV1) -> Vec<TensorWireV1> {
    let shapes: Vec<Vec<u64>> = vec![
        vec![u64::MAX, 2],
        vec![1 << 32, 1 << 32],
        vec![u64::MAX],
        vec![0, u64::MAX],
        vec![0, 1 << 40, 1 << 40],
        vec![0, u64::MAX, u64::MAX],
        vec![3, 0],
        vec![],
        vec![1; 300],
    ];
    let mut v: Vec<TensorWireV1> = shapes.into_iter().map(|s| TensorWireV1 { shape: s, ..t.clone() }).collect();
    for d in [1u8, 3, 5, 7, 9, 12, 255] {
        v.push(TensorWireV1 { dtype: d, ..t.clone() });
    }
    v.push(TensorWireV1 { bytes: vec![], ..t.clone() });
    v
}

fn structured(name: &str, obj: &O) -> Vec<(String, O)> {
    let mut out = Vec::new();
    match obj {
        O::FileProof { accuser, claim, proof: ProsecutionV1::Pipeline(bytes) } => {
            let Ok(w) = borsh::from_slice::<PipelineFaultWireV1>(bytes) else { return out };
            let put = |m: PipelineFaultWireV1| O::FileProof {
                accuser: *accuser,
                claim: *claim,
                proof: ProsecutionV1::Pipeline(borsh::to_vec(&m).unwrap()),
            };
            match &w {
                PipelineFaultWireV1::Edge { stage, input, position, claimed, upstream } => {
                    for (i, t) in tensor_mutants(claimed).into_iter().enumerate() {
                        out.push((
                            format!("{name}/claimed#{i}"),
                            put(PipelineFaultWireV1::Edge {
                                stage: *stage,
                                input: *input,
                                position: *position,
                                claimed: t,
                                upstream: upstream.clone(),
                            }),
                        ));
                    }
                    for k in 0..upstream.len() {
                        for (i, t) in tensor_mutants(&upstream[k]).into_iter().enumerate() {
                            let mut u = upstream.clone();
                            u[k] = t;
                            out.push((
                                format!("{name}/upstream{k}#{i}"),
                                put(PipelineFaultWireV1::Edge {
                                    stage: *stage,
                                    input: *input,
                                    position: *position,
                                    claimed: claimed.clone(),
                                    upstream: u,
                                }),
                            ));
                        }
                    }
                    for (st, inp, pos) in [(u8::MAX, 0u16, 0u32), (0, u16::MAX, 0), (0, 0, u32::MAX), (1, u16::MAX, u32::MAX)] {
                        out.push((
                            format!("{name}/where{st}.{inp}.{pos}"),
                            put(PipelineFaultWireV1::Edge {
                                stage: st,
                                input: inp,
                                position: pos,
                                claimed: claimed.clone(),
                                upstream: upstream.clone(),
                            }),
                        ));
                    }
                }
                // The commitment-only edge form (no claimed tensor; the edge is judged against its committed root): the same upstream
                // and coordinate mutants as `Edge`.
                PipelineFaultWireV1::EdgeCommitment { stage, input, position, upstream } => {
                    for k in 0..upstream.len() {
                        for (i, t) in tensor_mutants(&upstream[k]).into_iter().enumerate() {
                            let mut u = upstream.clone();
                            u[k] = t;
                            out.push((
                                format!("{name}/upstream{k}#{i}"),
                                put(PipelineFaultWireV1::EdgeCommitment {
                                    stage: *stage,
                                    input: *input,
                                    position: *position,
                                    upstream: u,
                                }),
                            ));
                        }
                    }
                    for (st, inp, pos) in [(u8::MAX, 0u16, 0u32), (0, u16::MAX, 0), (0, 0, u32::MAX), (1, u16::MAX, u32::MAX)] {
                        out.push((
                            format!("{name}/where{st}.{inp}.{pos}"),
                            put(PipelineFaultWireV1::EdgeCommitment {
                                stage: st,
                                input: inp,
                                position: pos,
                                upstream: upstream.clone(),
                            }),
                        ));
                    }
                }
                PipelineFaultWireV1::Stage { stage, proof } => {
                    for (i, t) in tensor_mutants(&proof.output).into_iter().enumerate() {
                        let mut p = proof.clone();
                        p.output = t;
                        out.push((format!("{name}/output#{i}"), put(PipelineFaultWireV1::Stage { stage: *stage, proof: p })));
                    }
                    for k in 0..proof.inputs.len() {
                        for (i, t) in tensor_mutants(&proof.inputs[k]).into_iter().enumerate() {
                            let mut p = proof.clone();
                            p.inputs[k] = t;
                            out.push((format!("{name}/input{k}#{i}"), put(PipelineFaultWireV1::Stage { stage: *stage, proof: p })));
                        }
                    }
                    for st in [u8::MAX, 0, 1, 2, 3] {
                        out.push((format!("{name}/stage{st}"), put(PipelineFaultWireV1::Stage { stage: st, proof: proof.clone() })));
                    }
                }
                PipelineFaultWireV1::Decode { index, logits } => {
                    for (i, t) in tensor_mutants(logits).into_iter().enumerate() {
                        out.push((format!("{name}/logits#{i}"), put(PipelineFaultWireV1::Decode { index: *index, logits: t })));
                    }
                }
            }
        }
        O::Respond { claim, stage, position, bytes } => {
            let Ok(r) = borsh::from_slice::<PositionResponseV1>(bytes) else { return out };
            let put = |m: PositionResponseV1| O::Respond {
                claim: *claim,
                stage: *stage,
                position: *position,
                bytes: borsh::to_vec(&m).unwrap(),
            };
            for k in 0..r.inputs.len() {
                if let MaterialResponseV1::Whole(t) = &r.inputs[k] {
                    for (i, m) in tensor_mutants(t).into_iter().enumerate() {
                        let mut rr = r.clone();
                        rr.inputs[k] = MaterialResponseV1::Whole(m);
                        out.push((format!("{name}/in{k}#{i}"), put(rr)));
                    }
                }
            }
            for (s, occ) in r.values.iter().enumerate() {
                for (n, v) in occ.iter().enumerate() {
                    if let MaterialResponseV1::Whole(t) = v {
                        for (i, m) in tensor_mutants(t).into_iter().enumerate() {
                            let mut rr = r.clone();
                            rr.values[s][n] = MaterialResponseV1::Whole(m);
                            out.push((format!("{name}/val{s}.{n}#{i}"), put(rr)));
                        }
                        break;
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn byte_mutants(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let n = bytes.len();
    let mut offs: Vec<usize> = (0..n.min(220)).collect();
    offs.extend((220..n).step_by((n / 200).max(1)));
    let mut v = Vec::new();
    for i in offs {
        for (tag, f) in [("xor80", 0x80u8), ("xorff", 0xFF), ("xor01", 0x01)] {
            let mut b = bytes.to_vec();
            b[i] ^= f;
            v.push((format!("byte{i}/{tag}"), b));
        }
    }
    for cut in [0, 1, 2, 3, 10, 66, 130, n / 2, n.saturating_sub(65), n.saturating_sub(5), n.saturating_sub(1)] {
        if cut < n {
            v.push((format!("trunc{cut}"), bytes[..cut].to_vec()));
        }
    }
    let mut t = bytes.to_vec();
    t.push(0);
    v.push(("trailing".into(), t));
    v
}

#[test]
fn a_stranger_who_mutates_a_pipeline_object_cannot_panic_the_fold_or_convict_an_honest_claim() {
    install_hook();
    let mut w = PW::new();
    let masks = w.masks();
    let job = w.post(2, 1);
    let (hc, _, htx) = w.produce(&job, |_| {});
    let hid = hc.id();
    w.blk(10, vec![htx, T::PanelCovered { claim: hid }]);
    // A lying claim: a conditioning edge that is not the encoder's rows (edge court) and a lie inside the denoiser (stage court).
    let job2 = w.post(11, 2);
    let (lc, ltrace, ltx) = w.produce(&job2, |t| t.stages[1].inputs[0][IN_COND as usize].data[0] += 1);
    let lid = lc.id();
    w.blk(12, vec![ltx, T::PanelCovered { claim: lid }]);
    let lda = Da::publishing(&ltrace, &masks);
    let edge = {
        let f = OutsiderV1 { ledger: &w.l, claim: lid, material: &lda, artifact: &w.params.0, salt: [0xC4; 64] }.check().unwrap();
        let OutsiderFindingV1::Prosecute(p) = f else { panic!("{f:?}") };
        p
    };
    let job3 = w.post(13, 3);
    let (lc3, ltrace3, ltx3) = w.produce(&job3, |t| {
        let v = stage_view_v1(&w.programs[1]);
        let last = t.stages[1].values.len() - 1;
        let x = &mut t.stages[1].values[last][v.post_occurrence as usize][0];
        x.data[0] += if x.dtype.contains(x.data[0] + 1) { 1 } else { -1 };
    });
    let lid3 = lc3.id();
    w.blk(14, vec![ltx3, T::PanelCovered { claim: lid3 }]);
    let lda3 = Da::publishing(&ltrace3, &masks);
    let stage = {
        let f = OutsiderV1 { ledger: &w.l, claim: lid3, material: &lda3, artifact: &w.params.0, salt: [0xC4; 64] }.check().unwrap();
        let OutsiderFindingV1::Prosecute(p) = f else { panic!("{f:?}") };
        p
    };
    w.blk(15, vec![T::FileDemand { demander: OUTSIDER, claim: lid, stage: 1, position: 0 }]);
    let job4 = PipelineJobPostV1 { nonce: [9; 64], ..job.clone() };
    let (c4, _, tx4) = w.produce(&job4, |_| {});
    let _ = c4;
    w.blk(16, seals_for(std::slice::from_ref(&tx4)));
    let reg = {
        let d = k2_tir_v3_descriptor();
        let pcs: Vec<ParamCommitmentsV1> = w.params.0.iter().map(ParamCommitmentsV1::of).collect();
        O::RegisterPipelineClass {
            descriptor: d.digest(),
            pipeline_bytes: w.p.encode(),
            program_bytes: w.programs.iter().map(TirProgramV2::encode).collect(),
            plan: pipeline_plan_v1(&d, &w.p, &w.programs).unwrap(),
            param_commitments: pcs,
            decode: None,
        }
    };
    let resp = O::Respond { claim: lid, stage: 1, position: 0, bytes: position(&masks, &ltrace, 1, 0) };
    let samples: Vec<(&str, O, Digest)> = vec![
        ("FileProof/Pipeline edge vs lying", O::FileProof { accuser: OUTSIDER, claim: lid, proof: edge.clone() }, OUTSIDER),
        ("FileProof/Pipeline edge vs honest", O::FileProof { accuser: OUTSIDER, claim: hid, proof: edge }, OUTSIDER),
        ("FileProof/Pipeline stage vs lying", O::FileProof { accuser: OUTSIDER, claim: lid3, proof: stage.clone() }, OUTSIDER),
        ("FileProof/Pipeline stage vs honest", O::FileProof { accuser: OUTSIDER, claim: hid, proof: stage }, OUTSIDER),
        ("Respond stage position", resp, SPAM1),
        ("PostPipelineJob", O::PostPipelineJob { job: PipelineJobPostV1 { nonce: [7; 64], ..job.clone() } }, PRODUCER),
        (
            "CommitPipelineClaim",
            {
                let T::CommitPipelineClaim { claim, evidence, stages } = tx4 else { unreachable!() };
                O::CommitPipelineClaim { claim, evidence, stages }
            },
            PRODUCER,
        ),
        ("RegisterPipelineClass", reg, PRODUCER),
    ];
    let mut stats: BTreeMap<&'static str, u32> = BTreeMap::new();
    for (name, obj, signer) in &samples {
        let before_stats = stats.clone();
        let enc = obj.encode();
        assert_eq!(&O::decode(&enc).expect("canonical"), obj, "{name}");
        let mut all: Vec<(String, Vec<u8>)> = vec![(format!("{name}/original"), enc.clone())];
        for (m, o) in structured(name, obj) {
            all.push((m, o.encode()));
        }
        for (m, b) in byte_mutants(&enc) {
            all.push((format!("{name}/{m}"), b));
        }
        for (label, bytes) in all {
            let mut l = w.l.clone();
            l.begin_block(w.l.daa + 1).unwrap();
            let mut bk = w.book.clone();
            let before = l.root();
            let r = catch_unwind(AssertUnwindSafe(|| l.apply_encoded(&bytes, &AuthV1 { signer_bond: *signer })));
            match r {
                Err(_) => {
                    *stats.entry("panic").or_default() += 1;
                    PANICS.lock().unwrap().last_mut().expect("hook").1.push_str(&format!(" :: {label}"));
                }
                Ok(Err(_)) => {
                    *stats.entry("refused").or_default() += 1;
                    assert_eq!(l.root(), before, "I2: a refusal changed the state ({label})");
                }
                Ok(Ok(ev)) => {
                    *stats.entry("applied").or_default() += 1;
                    assert!(
                        !ev.iter().any(|e| matches!(e, E::Convicted { claim, .. } if *claim == hid)),
                        "I3: honest claim convicted ({label})"
                    );
                    for e in &ev {
                        if let E::Settlement(s) = e {
                            bk.apply(s).unwrap_or_else(|why| panic!("I4: a settlement a real bond refuses ({label}): {why}"));
                        }
                    }
                    bk.check(&l).unwrap_or_else(|why| panic!("I4: accounting ({label}): {why}"));
                }
            }
        }
        let d = |k: &'static str| stats.get(k).copied().unwrap_or(0) - before_stats.get(k).copied().unwrap_or(0);
        eprintln!("  {name}: {} bytes, applied {}, refused {}, panicked {}", enc.len(), d("applied"), d("refused"), d("panic"));
    }
    let _ = std::panic::take_hook();
    let panics = PANICS.lock().unwrap().clone();
    let mut by_loc: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for (loc, msg) in &panics {
        let e = by_loc.entry(loc.clone()).or_insert((0, String::new()));
        e.0 += 1;
        if e.1.len() < 300 {
            e.1.push_str(&format!("[{msg}] "));
        }
    }
    eprintln!("mutants: {stats:?}");
    for (loc, (n, first)) in &by_loc {
        eprintln!("PANIC x{n} at {loc}: {first}");
    }
    assert!(by_loc.is_empty(), "I1 violated: {} distinct panic sites reachable from encoded pipeline objects", by_loc.len());
}

/// EXPECTED (before running): every byte flip of a valid `RegisterPipelineClass` (three variants per byte) is refused or applied with
/// no panic, a refusal leaves the root unchanged. VERDICT: PASS expected.
#[test]
fn every_byte_flip_of_a_pipeline_class_registration_is_refused_or_applied_without_a_panic() {
    let w = PW::new();
    let d = k2_tir_v3_descriptor();
    let pcs: Vec<ParamCommitmentsV1> = w.params.0.iter().map(ParamCommitmentsV1::of).collect();
    let mut base = w.genesis.clone();
    base.begin_block(1).unwrap();
    base.sync_bond(PRODUCER, 0);
    for p in &pcs {
        base.attest_artifact(p.root());
    }
    let obj = O::RegisterPipelineClass {
        descriptor: d.digest(),
        pipeline_bytes: w.p.encode(),
        program_bytes: w.programs.iter().map(TirProgramV2::encode).collect(),
        plan: pipeline_plan_v1(&d, &w.p, &w.programs).unwrap(),
        param_commitments: pcs,
        decode: None,
    };
    {
        let mut ok = base.clone();
        ok.begin_block(2).unwrap();
        ok.apply_encoded(&obj.encode(), &AuthV1 { signer_bond: PRODUCER }).expect("the unflipped registration applies");
    }
    let (applied, refused, panics) = support::exhaustive_flips(&base, &obj, PRODUCER);
    eprintln!(
        "RegisterPipelineClass byte flips: {} bytes, applied {applied}, refused {refused}, panics {}",
        obj.encoded_len(),
        panics.len()
    );
    assert!(panics.is_empty(), "I1 violated: {panics:?}");
}

/// EXPECTED (before running): the one-claim-per-job rule holds for pipeline claims too: a second claim (another bond re-signing the
/// published evidence and stage commitments) for a job that has a live claim is refused. VERDICT: PASS.
#[test]
fn a_second_pipeline_claim_for_the_same_job_is_refused() {
    let mut w = PW::new();
    let job = w.post(2, 1);
    let (claim, _, tx) = w.produce(&job, |_| {});
    let T::CommitPipelineClaim { claim: c, evidence, stages } = tx.clone() else { unreachable!() };
    let ev = w.blk(10, vec![tx, T::PanelCovered { claim: claim.id() }]);
    assert!(ev.contains(&E::ClaimCommitted { claim: claim.id() }), "{ev:?}");
    let copy = PipelineClaimV1 { producer_bond: SPAM1, ..c };
    let ev = w.blk(11, vec![T::CommitPipelineClaim { claim: copy, evidence, stages }]);
    assert!(ev.iter().any(|e| matches!(e, E::Refused { why, .. } if why.contains("one claim per job"))), "{ev:?}");
}

/// EXPECTED (before running): a conditioning-edge lie and a lie inside the denoiser stage of a pipeline claim, committed behind a
/// colluding Panel's cover, are found by an outsider from public material, convicted by the pipeline court with the correct money flow
/// (the accountant checks every instruction), and neither proof convicts the honest claim of another job. VERDICT: PASS.
#[test]
fn a_pipeline_edge_lie_and_a_stage_lie_are_convicted_and_do_not_touch_the_honest_claim() {
    let mut w = PW::new();
    let masks = w.masks();
    let job = w.post(2, 1);
    let (hc, _, htx) = w.produce(&job, |_| {});
    let hid = hc.id();
    w.blk(10, vec![htx, T::PanelCovered { claim: hid }]);
    for (nonce, daa, edge) in [(2u8, 20u64, true), (3, 40, false)] {
        let j = w.post(daa - 5, nonce);
        let programs = w.programs.clone();
        let (lc, ltrace, ltx) = w.produce(&j, |t| {
            if edge {
                t.stages[1].inputs[0][IN_COND as usize].data[0] += 1;
            } else {
                let v = stage_view_v1(&programs[1]);
                let last = t.stages[1].values.len() - 1;
                let x = &mut t.stages[1].values[last][v.post_occurrence as usize][0];
                x.data[0] += if x.dtype.contains(x.data[0] + 1) { 1 } else { -1 };
            }
        });
        let lid = lc.id();
        w.blk(daa, vec![ltx, T::PanelCovered { claim: lid }]);
        let da = Da::publishing(&ltrace, &masks);
        let f = OutsiderV1 { ledger: &w.l, claim: lid, material: &da, artifact: &w.params.0, salt: [0xC4; 64] }.check().unwrap();
        let OutsiderFindingV1::Prosecute(proof) = f else { panic!("the lie is found: {f:?}") };
        let ev = w.blk(daa + 1, vec![T::FileProof { accuser: OUTSIDER, claim: hid, proof: proof.clone() }]);
        assert!(!ev.iter().any(|e| matches!(e, E::Convicted { .. })), "the honest claim was convicted: {ev:?}");
        let ev = w.blk(daa + 2, vec![T::FileProof { accuser: OUTSIDER, claim: lid, proof }]);
        assert!(ev.iter().any(|e| matches!(e, E::Convicted { slashed: 1000, accuser_reward: 500, post_final: false, .. })), "{ev:?}");
    }
}
