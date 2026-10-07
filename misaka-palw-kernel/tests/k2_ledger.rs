//! **Public prosecution on an in-process chain** (ADR-0173; RFC-0015 §1.1 G14): the producer and every Panel seat colluding, one
//! ordinary bond outside the Panel, and nothing but ledger state and public DA bytes.
//!
//! The outsider in every test is built by replaying the block sequence from genesis (a fresh node's IBD) and reads committed
//! values only from a [`Da`] store of bytes the producer published (minus what it withholds) or from values served on chain in
//! answer to demands. It never sees the producer's trace, heap or private state, a Panel seat's capture, a court's internals or
//! a served view: the producer's objects are dropped before the outsider is built.

mod common;

use std::collections::BTreeMap;

use common::{MAX_POSITIONS, active_for, bump, root_of};
use misaka_palw_kernel::descriptor::{k2_tir_v1_descriptor, k2_tir_v2_descriptor};
use misaka_palw_kernel::evidence::build_evidence_v1;
use misaka_palw_kernel::gate::ProsecutionPolicyV1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::{DecodeRuleV1, KernelClaimV1, KernelJobV1};
use misaka_palw_kernel::ledger::{
    KernelLedgerV1, LedgerBlockV1, LedgerEventV1 as E, LedgerPolicyV1, LedgerTxV1 as T, OutsiderFindingV1, OutsiderV1, ProsecutionV1,
};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::merkle::TensorOpeningV1;
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::public::{MaterialResponseV1, TensorWireV1};
use misaka_palw_kernel::trace::{TraceV1, WiringV1, trace_v1};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{MapParams, Prim, Tensor};
use misaka_palw_tir_sketch::fixture::dense_moe_v1;

const PRODUCER: Digest = [0xA1; 64];
const OUTSIDER: Digest = [0x0B; 64];
const SPAM1: Digest = [0x51; 64];
const SPAM2: Digest = [0x52; 64];

const PROSECUTION: ProsecutionPolicyV1 = ProsecutionPolicyV1 {
    court_deadline_daa: 20,
    max_sessions_per_claim: 1 << 10,
    max_public_bytes: 1 << 40,
    max_verifier_ram: 1 << 36,
    max_retained_state: 1 << 32,
};

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
        prosecution: PROSECUTION,
    }
}

/// A public DA provider: the bytes a producer published, minus what it withholds.
struct Da(BTreeMap<(u32, u16, u16), Vec<u8>>);

impl Da {
    fn publishing(trace: &TraceV1, withhold: &[(u32, u16, u16)]) -> Self {
        let mut m = BTreeMap::new();
        for (p, pos) in trace.values.iter().enumerate() {
            for (s, occ) in pos.iter().enumerate() {
                for (n, t) in occ.iter().enumerate() {
                    let k = (p as u32, s as u16, n as u16);
                    if !withhold.contains(&k) {
                        m.insert(k, borsh::to_vec(&TensorWireV1::of(t)).unwrap());
                    }
                }
            }
        }
        Da(m)
    }

    fn get(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
        borsh::from_slice::<TensorWireV1>(self.0.get(&(p, s, n))?).ok()?.decode().ok()
    }
}

/// **A fresh outsider**: a node that replays the chain from genesis, then checks `claim` from the replayed state and `da` alone.
fn outsider(w: &World, claim: Digest, da: &Da) -> OutsiderFindingV1 {
    let fresh = KernelLedgerV1::replay(&w.genesis, &w.blocks);
    assert_eq!(fresh.root(), w.l.root(), "a fresh node reaches the same state");
    let material = |p, s, n| da.get(p, s, n);
    OutsiderV1 { ledger: &fresh, claim, material: &material }.check().unwrap()
}

/// A producer's claim (its private objects: the test drops them before an outsider looks).
struct Produced {
    claim: KernelClaimV1,
    trace: TraceV1,
    tx: T,
}

struct World {
    genesis: KernelLedgerV1,
    blocks: Vec<LedgerBlockV1>,
    l: KernelLedgerV1,
    class: Digest,
    program: TirProgramV1,
    params: MapParams,
}

impl World {
    fn new() -> Self {
        Self::with(policy())
    }

    fn with(policy: LedgerPolicyV1) -> Self {
        let fx = dense_moe_v1(7);
        let d = k2_tir_v1_descriptor();
        let genesis = KernelLedgerV1::genesis(policy, active_for(&d), vec![k2_tir_v1_descriptor(), k2_tir_v2_descriptor()]).unwrap();
        let mut w =
            World { genesis: genesis.clone(), blocks: vec![], l: genesis, class: [0; 64], program: fx.program, params: fx.params };
        let ev = w.block(1, vec![bond(PRODUCER, 5000), bond(OUTSIDER, 1000), bond(SPAM1, 1000), bond(SPAM2, 1000), w.register()]);
        if let Some(class) = ev.iter().find_map(|e| match e {
            E::ClassRegistered { class } => Some(*class),
            _ => None,
        }) {
            w.class = class;
        }
        w
    }

    fn register(&self) -> T {
        let d = k2_tir_v1_descriptor();
        let plan = plan_for_tir_program_v1(&d, &self.program, root_of(&self.program), MAX_POSITIONS).unwrap();
        T::RegisterClass {
            descriptor: d.digest(),
            program_bytes: self.program.encode(),
            plan,
            params: self.params.clone(),
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

    fn post_job(&mut self, daa: u64, prompt: &[u32], max_new_tokens: u32, nonce: u8) -> KernelJobV1 {
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
    fn greedy(&self, params: &MapParams, prompt: &[u32], n: usize) -> Vec<u32> {
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
    fn produce(
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

    fn honest(&self, job: &KernelJobV1, n: usize) -> Produced {
        let generated = self.greedy(&self.params, &job.prompt, n);
        self.produce(job, PRODUCER, generated, &self.params, |_| {})
    }

    /// An honest generation whose committed trace lies at one MatMul at position ≥ 1.
    fn lying(&self, job: &KernelJobV1, n: usize) -> ((u32, u16, u16), Produced) {
        let at = self.matmul_at(1);
        let generated = self.greedy(&self.params, &job.prompt, n);
        let p = self
            .produce(job, PRODUCER, generated, &self.params, |t| bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], 1));
        (at, p)
    }

    fn matmul_at(&self, from: u32) -> (u32, u16, u16) {
        for (s, (b, _)) in self.program.occurrences().iter().enumerate() {
            for (n, node) in self.program.blocks[*b as usize].nodes.iter().enumerate() {
                if matches!(node.prim, Prim::MatMul) {
                    return (from, s as u16, n as u16);
                }
            }
        }
        panic!("no MatMul")
    }

    fn state(&self, claim: &Digest) -> ClaimStateV1 {
        self.l.claims[claim].life.state.clone()
    }
}

fn bond(bond: Digest, collateral: u64) -> T {
    T::RegisterBond { bond, collateral }
}

fn convicted(ev: &[E]) -> Option<(u64, u64, bool)> {
    ev.iter().find_map(|e| match e {
        E::Convicted { slashed, accuser_reward, post_final, .. } => Some((*slashed, *accuser_reward, *post_final)),
        _ => None,
    })
}

fn refused(ev: &[E]) -> Option<String> {
    ev.iter().find_map(|e| match e {
        E::Refused { why, .. } => Some(why.clone()),
        _ => None,
    })
}

/// A position demand's response: every committed value of position `p` of `trace`, whole, then `edit`ed.
fn position(trace: &TraceV1, p: u32, edit: impl FnOnce(&mut Vec<Vec<MaterialResponseV1>>)) -> Vec<u8> {
    let mut r: Vec<Vec<MaterialResponseV1>> =
        trace.values[p as usize].iter().map(|o| o.iter().map(|t| MaterialResponseV1::Whole(TensorWireV1::of(t))).collect()).collect();
    edit(&mut r);
    borsh::to_vec(&r).unwrap()
}

// ── A: the producer and every Panel seat collude ──────────────────────────────────────────────────────────────────────────

#[test]
fn a_full_panel_collusion_loses_to_one_outside_bond_before_final_and_after_it() {
    let mut w = World::new();
    assert_ne!(w.class, [0; 64], "the class registers: it is publicly prosecutable");

    // Before Final: the claim is committed and every Panel seat signs it covered in the same block.
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    let ev = w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    assert!(ev.contains(&E::ClaimCommitted { claim: id }), "{ev:?}");
    assert!(matches!(w.state(&id), ClaimStateV1::ProbabilisticPass { .. }), "the colluding Panel passed it");
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("the lie is found from public material") };
    let ev = w.block(30, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: proof.clone() }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "{ev:?}");
    w.block(200, vec![]);
    assert!(matches!(w.state(&id), ClaimStateV1::Convicted { .. }), "a convicted claim never finalizes");
    assert!(!w.l.claims[&id].rewarded);
    assert_eq!(w.l.bonds[&PRODUCER].collateral, 4000);
    assert_eq!(w.l.bonds[&OUTSIDER].credits, 500);
    let ev = w.block(201, vec![T::FileProof { accuser: SPAM1, claim: id, proof }]);
    assert_eq!(ev, vec![E::Duplicate { claim: id }], "a claim is convicted once");

    // After Final: nobody prosecuted inside the window, the claim finalized and was paid; the liability horizon still convicts.
    let job = w.post_job(210, &[3, 17, 9], 3, 2);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(220, vec![lie.tx, T::PanelCovered { claim: id }]);
    let ev = w.block(270, vec![]);
    assert_eq!(ev, vec![E::Final { claim: id, reward: 7 }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(300, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, true)), "post-Final liability");
    assert_eq!(w.l.bonds[&PRODUCER].collateral, 3000);

    // Past the liability horizon a proof is dismissed and the reservation released (finite retained exposure).
    let job = w.post_job(310, &[3, 17, 9], 3, 3);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(320, vec![lie.tx, T::PanelCovered { claim: id }]);
    w.block(370, vec![]);
    let ev = w.block(571, vec![]);
    assert_eq!(ev, vec![E::Released { claim: id }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(572, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert!(matches!(&ev[..], [E::ProofDismissed { .. }]), "{ev:?}");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0);
}

// ── B: self-consistent garbage, and nothing but an exact court convicts ───────────────────────────────────────────────────

#[test]
fn a_self_consistent_trace_under_other_weights_is_convicted_and_a_failed_check_alone_never_convicts() {
    let mut w = World::new();
    // Garbage: every weight perturbed, the trace recomputed consistently, the tokens its own greedy decode.
    let mut garbage = w.params.clone();
    for t in garbage.tensors.values_mut() {
        for i in 0..t.data.len() {
            bump(t, i);
        }
    }
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let generated = w.greedy(&garbage, &job.prompt, 3);
    let g = w.produce(&job, PRODUCER, generated, &garbage, |_| {});
    let (id, da) = (g.claim.id(), Da::publishing(&g.trace, &[]));
    drop((g.trace, garbage));
    w.block(10, vec![g.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Kernel(proof)) = outsider(&w, id, &da) else {
        panic!("the registered public weights expose it")
    };

    // An honest claim: a fresh outsider finds it clean; the garbage claim's proof, junk bytes and a decode accusation of a
    // correct token are dismissed against it, and it finalizes. A dismissal never touches the lifecycle.
    let job2 = w.post_job(11, &[3, 17, 9], 3, 2);
    let h = w.honest(&job2, 3);
    let (hid, hda) = (h.claim.id(), Da::publishing(&h.trace, &[]));
    w.block(12, vec![h.tx, T::PanelCovered { claim: hid }]);
    assert_eq!(outsider(&w, hid, &hda), OutsiderFindingV1::Clean);
    let (post, logits) = w.l.classes[&w.class].logits_at();
    let honest_logits = hda.get(2, post, logits).unwrap();
    let bad_decode = misaka_palw_kernel::job::DecodeFaultV1 { index: 0, logits: TensorWireV1::of(&honest_logits) };
    let ev = w.block(
        20,
        vec![
            T::FileProof { accuser: OUTSIDER, claim: hid, proof: ProsecutionV1::Kernel(proof.clone()) },
            T::FileProof { accuser: OUTSIDER, claim: hid, proof: ProsecutionV1::Kernel(vec![1, 2, 3]) },
            T::FileProof { accuser: OUTSIDER, claim: hid, proof: ProsecutionV1::Decode(bad_decode) },
        ],
    );
    assert_eq!(ev.iter().filter(|e| matches!(e, E::ProofDismissed { .. })).count(), 3, "{ev:?}");
    assert_eq!(w.l.bonds[&OUTSIDER].collateral, 1000 - 3 * 5, "a dismissed filing forfeits its fee");
    assert!(convicted(&ev).is_none());
    let ev = w.block(62, vec![]);
    assert!(ev.contains(&E::Final { claim: hid, reward: 7 }), "{ev:?}");

    // The garbage claim's proof convicts it.
    let ev = w.block(30, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: ProsecutionV1::Kernel(proof) }]);
    assert!(ev.is_empty(), "a stale block is not applied");
    let ev = w.block(62, vec![]);
    assert!(ev.is_empty());
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(63, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, true)), "{ev:?}");
}

// ── C: borrowed traces and substituted outputs ────────────────────────────────────────────────────────────────────────────

#[test]
fn a_borrowed_trace_is_refused_at_inclusion_and_a_substituted_output_is_convicted_by_the_decode_court() {
    let mut w = World::new();
    let job1 = w.post_job(2, &[3, 17, 9], 3, 1);
    let job2 = w.post_job(3, &[4, 18, 10], 3, 1);
    let h1 = w.honest(&job1, 3);
    let T::CommitClaim { evidence, commitments, .. } = h1.tx.clone() else { unreachable!() };

    // A valid trace of job 1 offered as job 2's: the evidence's input is not job 2's.
    let borrowed = KernelClaimV1 { job_id: job2.id(), ..h1.claim.clone() };
    let ev = w.block(5, vec![T::CommitClaim { claim: borrowed, evidence: evidence.clone(), commitments: commitments.clone() }]);
    assert_eq!(refused(&ev).as_deref(), Some("binding fault WrongInput"), "{ev:?}");
    // Another evidence object than the claim commits; a job nobody posted; a token past the bound; commitments not the evidence's.
    let wrong_ev = KernelClaimV1 { evidence_root: [1; 64], ..h1.claim.clone() };
    let no_job = KernelClaimV1 { job_id: [2; 64], ..h1.claim.clone() };
    let mut oob = h1.claim.clone();
    oob.generated[2] = w.program.token_bound;
    let mut other_commitments = commitments.clone();
    other_commitments[0][0][0] = [7; 64];
    let ev = w.block(
        6,
        vec![
            T::CommitClaim { claim: wrong_ev, evidence: evidence.clone(), commitments: commitments.clone() },
            T::CommitClaim { claim: no_job, evidence: evidence.clone(), commitments: commitments.clone() },
            T::CommitClaim { claim: oob, evidence: evidence.clone(), commitments: commitments.clone() },
            T::CommitClaim { claim: h1.claim.clone(), evidence: evidence.clone(), commitments: other_commitments },
        ],
    );
    let why: Vec<_> = ev.iter().filter_map(|e| if let E::Refused { why, .. } = e { Some(why.as_str()) } else { None }).collect();
    assert_eq!(
        why,
        [
            "binding fault WrongEvidence",
            "no such job",
            "binding fault TokenOutOfRange",
            "the carried trace commitments are not the evidence's trace root"
        ]
    );
    assert!(w.l.claims.is_empty(), "nothing was committed, nothing reserved");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0);

    // The last delivered token substituted (it is never fed back, so the trace is the honest one): the decode court convicts.
    let mut last = h1.claim.generated.clone();
    last[2] = (last[2] + 1) % w.program.token_bound;
    let sub = w.produce(&job1, PRODUCER, last, &w.params.clone(), |_| {});
    let (id, da) = (sub.claim.id(), Da::publishing(&sub.trace, &[]));
    w.block(7, vec![sub.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Decode(f)) = outsider(&w, id, &da) else { panic!() };
    assert_eq!(f.index, 2);
    let ev = w.block(8, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: ProsecutionV1::Decode(f) }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)));

    // A token substituted mid-generation, the trace recomputed over the substituted stream (self-consistent): the logits of
    // the position before it select another token.
    let mut mid = w.greedy(&w.params, &job2.prompt, 3);
    mid[0] = (mid[0] + 1) % w.program.token_bound;
    let sub = w.produce(&job2, PRODUCER, mid, &w.params.clone(), |_| {});
    let (id, da) = (sub.claim.id(), Da::publishing(&sub.trace, &[]));
    w.block(9, vec![sub.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Decode(f)) = outsider(&w, id, &da) else { panic!() };
    assert_eq!(f.index, 0);
    let ev = w.block(10, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: ProsecutionV1::Decode(f) }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)));
}

// ── D: withheld values are obtained through the chain, never a served view ────────────────────────────────────────────────

#[test]
fn withheld_positions_are_demanded_in_one_round_and_the_served_values_convict_or_silence_defaults() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let id = lie.claim.id();
    // The producer publishes everything but the value that convicts it and one value of position 3; it keeps its trace.
    let da = Da::publishing(&lie.trace, &[at, (3, 0, 0)]);
    let trace = lie.trace.clone();
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    assert_eq!(outsider(&w, id, &da), OutsiderFindingV1::Demand(vec![1, 3]), "no pass, no conviction: one round of demands");

    let ev = w.block(
        11,
        vec![
            T::FileDemand { demander: OUTSIDER, claim: id, position: 1 },
            T::FileDemand { demander: OUTSIDER, claim: id, position: 3 },
        ],
    );
    assert_eq!(
        ev,
        vec![E::DemandOpened { claim: id, position: 1, deadline: 31 }, E::DemandOpened { claim: id, position: 3, deadline: 31 }]
    );
    assert!(matches!(w.state(&id), ClaimStateV1::Disputed { open: 2, .. }), "open demands block Final");
    // An authentic row where the whole value is owed does not serve the position: the demand stays open.
    let committed = trace.values[at.0 as usize][at.1 as usize][at.2 as usize].clone();
    let row = position(&trace, 1, |r| {
        r[at.1 as usize][at.2 as usize] = MaterialResponseV1::Part(TensorOpeningV1::row(&committed, 0).unwrap())
    });
    let ev = w.block(12, vec![T::Respond { claim: id, position: 1, bytes: row }]);
    assert_eq!(ev, vec![E::ResponseRejected { claim: id, position: 1, class: "partial" }]);
    // The committed values, served on chain: public from then on, and they convict (an authentic opening is not an acquittal).
    let ev = w.block(
        13,
        vec![
            T::Respond { claim: id, position: 1, bytes: position(&trace, 1, |_| {}) },
            T::Respond { claim: id, position: 3, bytes: position(&trace, 3, |_| {}) },
        ],
    );
    assert_eq!(ev, vec![E::Served { claim: id, position: 1 }, E::Served { claim: id, position: 3 }]);
    assert_eq!(w.l.bonds[&OUTSIDER].reserved, 0, "the demand bonds return");
    let ev = w.block(14, vec![T::FileDemand { demander: SPAM1, claim: id, position: 1 }]);
    assert_eq!(refused(&ev).as_deref(), Some("already served: it is public"));
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("the served values complete the check") };
    let ev = w.block(15, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)));

    // The same lie, and the producer stays silent: an availability default, never the fraud slash.
    let job = w.post_job(20, &[3, 17, 9], 3, 2);
    let (at, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]));
    w.block(21, vec![lie.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Demand(positions) = outsider(&w, id, &da) else { panic!() };
    assert_eq!(positions, vec![at.0]);
    w.block(22, vec![T::FileDemand { demander: OUTSIDER, claim: id, position: at.0 }]);
    let collateral = w.l.bonds[&PRODUCER].collateral;
    let ev = w.block(42, vec![]);
    assert_eq!(ev, vec![E::ProducerDefault { claim: id, position: at.0, last: None, penalty: 100 }]);
    assert_eq!(w.state(&id), ClaimStateV1::Unavailable { daa: 42, producer_defaulted: true });
    assert_eq!(w.l.bonds[&PRODUCER].collateral, collateral - 100);
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0, "the reservation is released, not slashed");
    w.block(500, vec![]);
    assert!(!w.l.claims[&id].rewarded && !w.l.claims[&id].convicted);
}

#[test]
fn a_class_without_a_complete_public_prosecution_never_registers() {
    // A prosecution policy whose public-bytes ceiling the plan exceeds: the gate refuses the class, so no job, claim or reward.
    let mut p = policy();
    p.prosecution.max_public_bytes = 1024;
    let w = World::with(p);
    assert_eq!(w.class, [0; 64]);
    assert!(w.l.classes.is_empty());
    let why = refused(&w.l.events.iter().map(|(_, e)| e.clone()).collect::<Vec<_>>()).unwrap();
    assert!(why.starts_with("not publicly prosecutable") && why.contains("public bytes"), "{why}");
    // Fewer demand sessions than the plan has positions: some position could not be demanded, so the class is refused.
    let mut p = policy();
    p.prosecution.max_sessions_per_claim = MAX_POSITIONS - 1;
    let w = World::with(p);
    assert!(w.l.classes.is_empty());

    // A kernel this binary does not implement, and a plan for another program, are refused too.
    let mut w = World::new();
    let T::RegisterClass { program_bytes, plan, params, network, ruleset, .. } = w.register() else { unreachable!() };
    let unknown = T::RegisterClass {
        descriptor: [0xEE; 64],
        program_bytes: program_bytes.clone(),
        plan: plan.clone(),
        params: params.clone(),
        network,
        ruleset,
    };
    let mut other = plan.clone();
    other.program_root = [1; 64];
    let mismatched =
        T::RegisterClass { descriptor: k2_tir_v1_descriptor().digest(), program_bytes, plan: other, params, network, ruleset };
    let ev = w.block(2, vec![unknown, mismatched]);
    assert_eq!(ev.iter().filter(|e| matches!(e, E::Refused { tx: "RegisterClass", .. })).count(), 2, "{ev:?}");
    // A job outside the class is refused at posting (an unsupported relation is never a success).
    let ev = w.block(
        3,
        vec![T::PostJob {
            job: KernelJobV1 {
                class_binding_id: w.class,
                prompt: vec![w.program.token_bound],
                max_new_tokens: 1,
                decode: DecodeRuleV1::Greedy,
                nonce: [0; 64],
            },
        }],
    );
    assert!(refused(&ev).is_some());
    assert!(borsh::from_slice::<DecodeRuleV1>(&[1]).is_err(), "no decode rule but those implemented");

    // A ledger policy whose liability horizon a demand's deadline outlasts is refused at genesis.
    let mut p = policy();
    p.liability_daa = p.court_deadline_daa;
    let d = k2_tir_v1_descriptor();
    assert!(KernelLedgerV1::genesis(p, active_for(&d), vec![d.clone()]).is_err());
}

// ── E: a direct proof is never pre-empted, and no demander starves another ───────────────────────────────────────────────

#[test]
fn open_demand_sessions_never_preempt_a_direct_proof_or_crowd_out_a_demand_and_settle_deterministically() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    // The producer's friends hold demands on most positions; another bond still opens the last one and joins another.
    let ev = w.block(
        11,
        vec![
            T::FileDemand { demander: SPAM1, claim: id, position: 0 },
            T::FileDemand { demander: SPAM1, claim: id, position: 1 },
            T::FileDemand { demander: SPAM2, claim: id, position: 2 },
            T::FileDemand { demander: SPAM2, claim: id, position: 3 },
            T::FileDemand { demander: OUTSIDER, claim: id, position: 4 },
            T::FileDemand { demander: OUTSIDER, claim: id, position: 0 },
        ],
    );
    assert!(refused(&ev).is_none(), "{ev:?}");
    assert_eq!(ev.iter().filter(|e| matches!(e, E::DemandOpened { .. })).count(), 5);
    assert!(ev.contains(&E::DemandJoined { claim: id, position: 0 }));
    let ev = w.block(12, vec![T::FileDemand { demander: OUTSIDER, claim: id, position: 5 }]);
    assert_eq!(refused(&ev).as_deref(), Some("the claim commits no such position"), "sessions are bounded by positions");

    // The direct proof convicts in the block that carries it; every open session settles as moot and every bond returns.
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(13, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)));
    assert!(ev.contains(&E::DemandsMoot { claim: id, refunded: 6 }), "{ev:?}");
    assert!(w.l.demands.is_empty());
    for b in [SPAM1, SPAM2, OUTSIDER] {
        assert_eq!(w.l.bonds[&b].reserved, 0);
    }
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0);
    // Nothing is left to time out against anyone.
    let ev = w.block(100, vec![]);
    assert!(ev.is_empty(), "{ev:?}");
}

// ── F: the Final race ─────────────────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn spam_cannot_hold_final_past_window_end_plus_court_deadline_and_a_proof_at_window_end_blocks_final() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    let trace = h.trace.clone();
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]); // window end 60
    let ev = w.block(
        59,
        vec![
            T::FileDemand { demander: SPAM1, claim: id, position: 0 },
            T::FileDemand { demander: SPAM1, claim: id, position: 1 },
            T::FileDemand { demander: SPAM2, claim: id, position: 2 },
        ],
    );
    assert_eq!(ev.iter().filter(|e| matches!(e, E::DemandOpened { .. })).count(), 3);
    let ev = w.block(60, vec![T::FileDemand { demander: SPAM2, claim: id, position: 3 }]);
    assert_eq!(refused(&ev).as_deref(), Some("the challenge window is closed"), "the window closes on time");
    // The honest producer answers each demand at its last moment.
    let serve = |p: u32| T::Respond { claim: id, position: p, bytes: position(&trace, p, |_| {}) };
    w.block(78, vec![serve(0), serve(1)]);
    assert!(matches!(w.state(&id), ClaimStateV1::Disputed { .. }));
    let ev = w.block(78, vec![serve(2)]);
    assert!(ev.contains(&E::Final { claim: id, reward: 7 }), "{ev:?}");
    let ClaimStateV1::Final { final_daa } = w.state(&id) else { panic!() };
    assert!(final_daa <= 60 + 20);

    // Demands filed while the claim is still checking hold it too, and the Panel's coverage meanwhile starts the window.
    let job = w.post_job(100, &[3, 17, 9], 3, 2);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    let trace = h.trace.clone();
    w.block(101, vec![h.tx]);
    w.block(102, vec![T::FileDemand { demander: SPAM1, claim: id, position: 0 }]);
    w.block(103, vec![T::PanelCovered { claim: id }]); // window end 153
    w.block(110, vec![T::Respond { claim: id, position: 0, bytes: position(&trace, 0, |_| {}) }]);
    assert!(matches!(w.state(&id), ClaimStateV1::ProbabilisticPass { window_end_daa: 153, .. }));
    let ev = w.block(153, vec![]);
    assert_eq!(ev, vec![E::Final { claim: id, reward: 7 }]);

    // A qualified prosecution in the block where the window closes is applied before Final: the claim never finalizes.
    let job = w.post_job(200, &[3, 17, 9], 3, 3);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(201, vec![lie.tx, T::PanelCovered { claim: id }]); // window end 251
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(251, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "{ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, E::Final { .. })));

    // A demand in the window's last block, served at its deadline, still reaches a post-Final proof inside the liability horizon.
    let job = w.post_job(300, &[3, 17, 9], 3, 4);
    let (at, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]));
    let trace = lie.trace.clone();
    w.block(301, vec![lie.tx, T::PanelCovered { claim: id }]); // window end 351
    w.block(350, vec![T::FileDemand { demander: OUTSIDER, claim: id, position: at.0 }]);
    let ev = w.block(369, vec![T::Respond { claim: id, position: at.0, bytes: position(&trace, at.0, |_| {}) }]);
    assert!(ev.contains(&E::Final { claim: id, reward: 7 }), "{ev:?}");
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(370, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, true)));
}

// ── G: DA outcomes are classified, and none of them is the fraud slash ───────────────────────────────────────────────────

#[test]
fn da_responses_are_classified_and_a_default_is_an_availability_penalty_not_a_conviction() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    let trace = h.trace.clone();
    let other = trace.values[2][0][0].clone();
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);

    let ev = w.block(
        11,
        vec![
            T::Respond { claim: id, position: 1, bytes: position(&trace, 1, |_| {}) },
            T::FileDemand { demander: OUTSIDER, claim: id, position: 99 },
        ],
    );
    assert_eq!(
        ev.iter().filter_map(|e| if let E::Refused { why, .. } = e { Some(why.as_str()) } else { None }).collect::<Vec<_>>(),
        ["no open demand for this position", "the claim commits no such position"]
    );
    w.block(12, vec![T::FileDemand { demander: OUTSIDER, claim: id, position: 1 }]);
    let mut fake = TensorOpeningV1::row(&trace.values[1][0][0], 0).unwrap();
    fake.siblings.extend([[0; 64]; 3]);
    let ev = w.block(
        13,
        vec![
            T::Respond { claim: id, position: 1, bytes: vec![0xFF, 0x00] },
            T::Respond {
                claim: id,
                position: 1,
                bytes: position(&trace, 1, |r| {
                    r.pop();
                }),
            },
            T::Respond {
                claim: id,
                position: 1,
                bytes: position(&trace, 1, |r| r[0][0] = MaterialResponseV1::Whole(TensorWireV1::of(&other))),
            },
            T::Respond {
                claim: id,
                position: 1,
                bytes: position(&trace, 1, |r| r[0][0] = MaterialResponseV1::Part(TensorOpeningV1::row(&other, 0).unwrap())),
            },
            T::Respond { claim: id, position: 1, bytes: position(&trace, 1, |r| r[0][0] = MaterialResponseV1::Part(fake)) },
        ],
    );
    let classes: Vec<_> =
        ev.iter().filter_map(|e| if let E::ResponseRejected { class, .. } = e { Some(*class) } else { None }).collect();
    assert_eq!(classes, ["malformed", "malformed", "wrong_bytes", "wrong_root", "fake_opening"]);

    // The deadline passes on a non-serving response: the producer's default, a fixed penalty to the demander.
    let (collateral, credits) = (w.l.bonds[&PRODUCER].collateral, w.l.bonds[&OUTSIDER].credits);
    let ev = w.block(32, vec![]);
    assert_eq!(ev, vec![E::ProducerDefault { claim: id, position: 1, last: Some("fake_opening"), penalty: 100 }]);
    assert!(convicted(&ev).is_none());
    assert_eq!(w.l.bonds[&PRODUCER].collateral, collateral - 100, "a penalty, not the 1000 fraud slash");
    assert_eq!(w.l.bonds[&OUTSIDER].credits, credits + 100);
    assert_eq!(w.l.bonds[&OUTSIDER].reserved, 0);
    assert_eq!(w.state(&id), ClaimStateV1::Unavailable { daa: 32, producer_defaulted: true });
    let ev = w.block(33, vec![T::FileDemand { demander: OUTSIDER, claim: id, position: 1 }]);
    assert_eq!(refused(&ev).as_deref(), Some("the claim is already decided"));
}

// ── Duplicates, reorg, restart, IBD; collateral double use and exit ──────────────────────────────────────────────────────

#[test]
fn the_state_is_a_pure_fold_so_restart_ibd_and_reorg_agree_and_collateral_is_never_double_used() {
    let mut w = World::new();
    w.block(2, vec![bond([0xC0; 64], 1500)]);
    let job1 = w.post_job(3, &[3, 17, 9], 3, 1);
    let job2 = w.post_job(4, &[3, 17, 9], 3, 2);
    let generated = w.greedy(&w.params, &job1.prompt, 3);
    let params = w.params.clone();
    let c1 = w.produce(&job1, [0xC0; 64], generated.clone(), &params, |_| {});
    let at = w.matmul_at(1);
    let c2 = w.produce(&job2, [0xC0; 64], generated, &params, |t| bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], 1));
    let (id1, id2) = (c1.claim.id(), c2.claim.id());
    let ev = w.block(10, vec![c1.tx, c2.tx.clone(), T::PanelCovered { claim: c1.claim.id() }]);
    assert!(refused(&ev).unwrap().contains("no double use"), "{ev:?}");
    assert_eq!(w.l.bonds[&[0xC0; 64]].reserved, 1000);

    // The exit: nothing new is backed, and nothing is withdrawn while reserved or inside the delay.
    w.block(11, vec![T::RequestExit { bond: [0xC0; 64] }]);
    let ev = w.block(12, vec![T::Withdraw { bond: [0xC0; 64] }]);
    assert!(refused(&ev).is_some());
    w.block(60, vec![]); // Final for claim 1
    assert_eq!(w.l.bonds[&[0xC0; 64]].credits, 7);
    let ev = w.block(261, vec![T::Withdraw { bond: [0xC0; 64] }]); // the release happens in this block's tick, after the tx
    assert!(refused(&ev).is_some() && ev.contains(&E::Released { claim: id1 }), "{ev:?}");
    let ev = w.block(262, vec![c2.tx.clone()]);
    assert_eq!(refused(&ev).as_deref(), Some("the producer bond is exiting"));
    let ev = w.block(263, vec![T::Withdraw { bond: [0xC0; 64] }]);
    assert_eq!(ev, vec![E::Withdrawn { bond: [0xC0; 64], amount: 1507 }]);

    // The second claim, by the main producer; convicted once; a second proof is a duplicate.
    let c2 = w.produce(&job2, PRODUCER, c2.claim.generated.clone(), &params, |t| {
        bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], 1)
    });
    let id2b = c2.claim.id();
    assert_ne!(id2, id2b);
    let da2 = Da::publishing(&c2.trace, &[]);
    w.block(270, vec![c2.tx]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id2b, &da2) else { panic!() };
    let proof_block = w.blocks.len();
    w.block(275, vec![T::FileProof { accuser: OUTSIDER, claim: id2b, proof: proof.clone() }]);
    let ev = w.block(276, vec![T::FileProof { accuser: SPAM1, claim: id2b, proof }]);
    assert_eq!(ev, vec![E::Duplicate { claim: id2b }]);
    w.block(400, vec![]);

    // Restart: a node that stopped half-way and resumes reaches the same root; IBD from genesis too.
    let half = w.blocks.len() / 2;
    let mut restarted = KernelLedgerV1::replay(&w.genesis, &w.blocks[..half]);
    for b in &w.blocks[half..] {
        restarted.apply_block(b);
    }
    assert_eq!(restarted.root(), w.l.root());
    assert_eq!(KernelLedgerV1::replay(&w.genesis, &w.blocks).root(), w.l.root());

    // A reorg dropping the proof's block: the other branch is its own fold (the claim times out unconvicted there, its
    // reservation released) and switching back is a replay of the first.
    let mut branch: Vec<LedgerBlockV1> = w.blocks[..proof_block].to_vec();
    branch.push(LedgerBlockV1 { daa: 400, txs: vec![] });
    let other = KernelLedgerV1::replay(&w.genesis, &branch);
    assert!(!other.claims[&id2b].convicted);
    assert!(matches!(other.claims[&id2b].life.state, ClaimStateV1::TimedOut { .. }));
    assert_ne!(other.root(), w.l.root());
    assert!(w.l.claims[&id2b].convicted);
    assert_eq!(KernelLedgerV1::replay(&w.genesis, &w.blocks).root(), w.l.root());
}
