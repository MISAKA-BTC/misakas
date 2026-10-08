//! **The shared ledger-test world**: a registered dense/MoE class on an in-process chain, honest and lying producers, the public DA
//! store, a fresh outsider rebuilt by replay, and the consumer (bond book) every settlement instruction is applied to.
#![allow(dead_code)]

use std::collections::BTreeMap;

use super::chain::{Consumer, T, block_of};
use super::{MAX_POSITIONS, active_for, bump, root_of};
use misaka_palw_kernel::descriptor::{k2_tir_v1_descriptor, k2_tir_v2_descriptor};
use misaka_palw_kernel::evidence::build_evidence_v1;
use misaka_palw_kernel::gate::ProsecutionPolicyV1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::{DecodeRuleV1, KernelClaimV1, KernelJobV1};
use misaka_palw_kernel::ledger::{
    KernelLedgerV1, LedgerBlockV1, LedgerEventV1 as E, LedgerPolicyV1, OutsiderFindingV1, OutsiderV1, PublicSourceV1,
};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::public::{MaterialResponseV1, PositionResponseV1, TensorWireV1};
use misaka_palw_kernel::trace::{TraceV1, WiringV1, derived_mask_v1, trace_v1};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{MapParams, Prim, Tensor};
use misaka_palw_tir_sketch::fixture::dense_moe_v1;

pub const PRODUCER: Digest = [0xA1; 64];
pub const OUTSIDER: Digest = [0x0B; 64];
pub const SPAM1: Digest = [0x51; 64];
pub const SPAM2: Digest = [0x52; 64];

pub const PROSECUTION: ProsecutionPolicyV1 = ProsecutionPolicyV1 {
    court_deadline_daa: 20,
    max_sessions_per_claim: 1 << 10,
    max_public_bytes: 1 << 40,
    max_verifier_ram: 1 << 36,
    max_retained_state: 1 << 32,
};

pub fn policy() -> LedgerPolicyV1 {
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
        prosecution: PROSECUTION,
    }
}

/// The values of the class's program that are derived — a `Hist` window and its views — which nobody serves.
pub fn derived() -> Vec<Vec<bool>> {
    derived_mask_v1(&dense_moe_v1(7).program)
}

/// A public DA provider: the bytes a producer published, minus what it withholds. It never publishes a derived value (a window is
/// rebuilt from the committed rows), so its bytes are linear in the claim's length.
pub struct Da(pub BTreeMap<(u32, u16, u16), Vec<u8>>);

impl Da {
    pub fn publishing(trace: &TraceV1, withhold: &[(u32, u16, u16)]) -> Self {
        let mask = derived();
        let mut m = BTreeMap::new();
        for (p, pos) in trace.values.iter().enumerate() {
            for (s, occ) in pos.iter().enumerate() {
                for (n, t) in occ.iter().enumerate() {
                    let k = (p as u32, s as u16, n as u16);
                    if !withhold.contains(&k) && !mask[s][n] {
                        m.insert(k, borsh::to_vec(&TensorWireV1::of(t)).unwrap());
                    }
                }
            }
        }
        Da(m)
    }

    pub fn get(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
        borsh::from_slice::<TensorWireV1>(self.0.get(&(p, s, n))?).ok()?.decode().ok()
    }
}

impl PublicSourceV1 for Da {
    fn node(&self, stage: u8, p: u32, s: u16, n: u16) -> Option<Tensor> {
        if stage == 0 { self.get(p, s, n) } else { None }
    }
}

/// **A fresh outsider**: a node that replays the chain from genesis, then checks `claim` from the replayed state and `da` alone.
pub fn outsider(w: &World, claim: Digest, da: &Da) -> OutsiderFindingV1 {
    let fresh = KernelLedgerV1::replay(&w.genesis, &w.blocks);
    assert_eq!(fresh.root(), w.l.root(), "a fresh node reaches the same state");
    OutsiderV1 { ledger: &fresh, claim, material: da, artifact: &w.params, salt: [0x5A; 64] }.check().unwrap()
}

/// A producer's claim (its private objects: the test drops them before an outsider looks).
pub struct Produced {
    pub claim: KernelClaimV1,
    pub trace: TraceV1,
    pub tx: T,
}

pub struct World {
    pub genesis: KernelLedgerV1,
    pub blocks: Vec<LedgerBlockV1>,
    pub l: KernelLedgerV1,
    /// The consumer: a bond book every settlement instruction is applied to.
    pub consumer: Consumer,
    /// Every receipt so far (settlement instructions are in the consumer's book).
    pub events: Vec<E>,
    pub class: Digest,
    pub program: TirProgramV1,
    pub params: MapParams,
}

impl World {
    pub fn new() -> Self {
        Self::with(policy())
    }

    pub fn with(policy: LedgerPolicyV1) -> Self {
        let fx = dense_moe_v1(7);
        let d = k2_tir_v1_descriptor();
        let genesis = KernelLedgerV1::genesis(policy, active_for(&d), vec![k2_tir_v1_descriptor(), k2_tir_v2_descriptor()]).unwrap();
        let mut w = World {
            genesis: genesis.clone(),
            blocks: vec![],
            l: genesis,
            consumer: Consumer::default(),
            events: vec![],
            class: [0; 64],
            program: fx.program,
            params: fx.params,
        };
        let ev = w.block(1, vec![bond(PRODUCER, 5000), bond(OUTSIDER, 1000), bond(SPAM1, 1000), bond(SPAM2, 1000), w.register()]);
        if let Some(class) = ev.iter().find_map(|e| match e {
            E::ClassRegistered { class } => Some(*class),
            _ => None,
        }) {
            w.class = class;
        }
        w
    }

    pub fn register(&self) -> T {
        let d = k2_tir_v1_descriptor();
        let plan = plan_for_tir_program_v1(&d, &self.program, root_of(&self.program), MAX_POSITIONS).unwrap();
        T::RegisterClass { descriptor: d.digest(), program_bytes: self.program.encode(), plan, params: self.params.clone() }
    }

    pub fn block(&mut self, daa: u64, txs: Vec<T>) -> Vec<E> {
        let b = block_of(daa, txs, PRODUCER);
        let ev = self.consumer.apply(&mut self.l, &b);
        self.blocks.push(b);
        self.events.extend(ev.iter().cloned());
        ev
    }

    pub fn post_job(&mut self, daa: u64, prompt: &[u32], max_new_tokens: u32, nonce: u8) -> KernelJobV1 {
        let job = KernelJobV1 {
            class_binding_id: self.class,
            prompt: prompt.to_vec(),
            max_new_tokens,
            decode: DecodeRuleV1::Greedy,
            nonce: [nonce; 64],
        };
        let ev = self.block(daa, vec![T::PostJob { job: job.clone() }]);
        assert!(ev.contains(&E::JobPosted { job: job.id() }), "{ev:?}");
        job
    }

    /// The honest greedy generation of `n` tokens under `params`.
    pub fn greedy(&self, params: &MapParams, prompt: &[u32], n: usize) -> Vec<u32> {
        let (post, logits) = self.l.classes[&self.class].logits_at();
        let mut stream = prompt.to_vec();
        let mut out = vec![];
        for _ in 0..n {
            let t = trace_v1(&self.program, params, &stream).unwrap();
            let tok = DecodeRuleV1::Greedy.select(&t.values[stream.len() - 1][post as usize][logits as usize]).unwrap();
            out.push(tok);
            stream.push(tok);
        }
        out
    }

    /// A claim of `job` delivering `generated`, its trace computed under `params` then edited by `lie`.
    pub fn produce(
        &self,
        job: &KernelJobV1,
        bond: Digest,
        generated: Vec<u32>,
        params: &MapParams,
        lie: impl FnOnce(&mut TraceV1),
    ) -> Produced {
        let fed = generated.len() - 1;
        let stream: Vec<u32> = job.prompt.iter().chain(&generated[..fed]).copied().collect();
        let mut trace = trace_v1(&self.program, params, &stream).unwrap();
        lie(&mut trace);
        let class = &self.l.classes[&self.class];
        let w = WiringV1::new(&self.program).unwrap();
        let ev = build_evidence_v1(&w, &trace.evidence(), &stream, class.header(self.class), &class.descriptor, 2).unwrap();
        let claim = KernelClaimV1 { job_id: job.id(), producer_bond: bond, generated, evidence_root: ev.root() };
        let tx = T::CommitClaim { claim: claim.clone(), evidence: ev, commitments: trace.evidence().commitments };
        Produced { claim, trace, tx }
    }

    pub fn honest(&self, job: &KernelJobV1, n: usize) -> Produced {
        let generated = self.greedy(&self.params, &job.prompt, n);
        self.produce(job, PRODUCER, generated, &self.params, |_| {})
    }

    /// An honest generation whose committed trace lies at one MatMul at position ≥ 1.
    pub fn lying(&self, job: &KernelJobV1, n: usize) -> ((u32, u16, u16), Produced) {
        let at = self.matmul_at(1);
        let generated = self.greedy(&self.params, &job.prompt, n);
        let p = self
            .produce(job, PRODUCER, generated, &self.params, |t| bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], 1));
        (at, p)
    }

    pub fn matmul_at(&self, from: u32) -> (u32, u16, u16) {
        for (s, (b, _)) in self.program.occurrences().iter().enumerate() {
            for (n, node) in self.program.blocks[*b as usize].nodes.iter().enumerate() {
                if matches!(node.prim, Prim::MatMul) {
                    return (from, s as u16, n as u16);
                }
            }
        }
        panic!("no MatMul")
    }

    pub fn state(&self, claim: &Digest) -> ClaimStateV1 {
        self.l.claims[claim].life.state.clone()
    }
}

pub fn bond(bond: Digest, collateral: u64) -> T {
    T::RegisterBond { bond, collateral }
}

pub fn convicted(ev: &[E]) -> Option<(u64, u64, bool)> {
    ev.iter().find_map(|e| match e {
        E::Convicted { slashed, accuser_reward, post_final, .. } => Some((*slashed, *accuser_reward, *post_final)),
        _ => None,
    })
}

pub fn refused(ev: &[E]) -> Option<String> {
    ev.iter().find_map(|e| match e {
        E::Refused { why, .. } => Some(why.clone()),
        _ => None,
    })
}

/// A position demand's response: every committed value of position `p` of `trace`, whole, then `edit`ed.
pub fn position(trace: &TraceV1, p: u32, edit: impl FnOnce(&mut Vec<Vec<MaterialResponseV1>>)) -> Vec<u8> {
    let mask = derived();
    let mut r: Vec<Vec<MaterialResponseV1>> = trace.values[p as usize]
        .iter()
        .enumerate()
        .map(|(s, o)| {
            o.iter()
                .enumerate()
                .map(|(n, t)| if mask[s][n] { MaterialResponseV1::Omitted } else { MaterialResponseV1::Whole(TensorWireV1::of(t)) })
                .collect()
        })
        .collect();
    edit(&mut r);
    borsh::to_vec(&PositionResponseV1 { values: r, inputs: vec![] }).unwrap()
}

/// A lying claim the Panel covered at daa 10 (window end 60) and that finalized at 60: liability until 260.
pub fn final_lying_claim(w: &mut World) -> (Digest, (u32, u16, u16), Da, TraceV1) {
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let (id, da, trace) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]), lie.trace.clone());
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    assert_eq!(w.block(60, vec![]), vec![E::Final { claim: id, reward: 7 }]);
    assert_eq!(w.l.claims[&id].liability_until, Some(260));
    (id, at, da, trace)
}
