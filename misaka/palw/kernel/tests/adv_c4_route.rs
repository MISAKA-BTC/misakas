//! **Agent C4 — independent adversarial E2E of the kernel route ledger** (`KernelLedgerV1` + the strict `KernelRouteObjectV1` codec).
//!
//! Independence rules kept here (the lane B/D suites are NOT trusted):
//!
//! * every block is folded through the strict wire form: each object is `encode()`d and applied with `apply_encoded` (a verifier
//!   only ever has bytes);
//! * every verifier is built FRESH: [`fresh`] replays the encoded blocks from the genesis configuration alone, and the outsider
//!   reads only that replayed ledger, a public DA store and the public artifact;
//! * money is checked by [`Book`], this file's own settlement accountant (not `SettlementBookV1`): every instruction is applied to
//!   a bond book that refuses anything a real bond could not do, a slash is routed 1:1 to accuser reward + burn, the ledger's own
//!   view of every bond equals the book's after every block, and total value is conserved except for the explicit `FinalReward`
//!   issuance and the consumer's external `sync_bond`.
//!
//! Cases are named for the attack. `#[ignore = "FAIL F-C4-n …"]` marks a proven defect: the test asserts the SAFE property and
//! fails today; run `-- --ignored` to see the failure. Verdicts are recorded in `docs/evidence/palw/implementation-records/adversarial-e2e-record.md`.

mod common;

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use common::chain::{T, block_of, seals_for};
use common::ledger_world::*;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::KernelClaimV1;
use misaka_palw_kernel::ledger::{
    AuthV1, KernelLedgerV1, KernelRouteObjectV1 as O, LedgerEventV1 as E, LedgerTxV1, OutsiderFindingV1, OutsiderV1, ProsecutionV1,
    RefusalKindV1,
};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::settle::SettlementKindV1 as K;

#[path = "adv_c4_support/mod.rs"]
mod support;
use support::{Book, fold};

struct Net {
    w: World,
    book: Book,
    /// A node kept in sync from the ENCODED blocks only (used where a full replay per query would be quadratic).
    replica: KernelLedgerV1,
    /// Who signs the objects that name no actor (jobs, classes, responses).
    signer: Digest,
}

impl Net {
    fn new() -> Self {
        Self::of(World::new())
    }

    fn of(w: World) -> Self {
        let book = Book::from_ledger(&w.l);
        let replica = fresh(&w);
        assert_eq!(replica.root(), w.l.root());
        Net { w, book, replica, signer: PRODUCER }
    }

    fn blk(&mut self, daa: u64, txs: Vec<T>) -> Vec<E> {
        // Seal, then reveal: a claim is sealed by its producer in a block at the ledger's current DAA, then revealed in this one.
        let seals = seals_for(&txs);
        if !seals.is_empty() && self.w.l.daa < daa {
            let sb = block_of(self.w.l.daa, seals, self.signer);
            fold(&mut self.w.l, &sb, Some(&mut self.book));
            fold(&mut self.replica, &sb, None);
            self.w.blocks.push(sb);
        }
        self.blk_raw(daa, txs)
    }

    /// A block exactly as given (no automatic seal).
    fn blk_raw(&mut self, daa: u64, txs: Vec<T>) -> Vec<E> {
        let b = block_of(daa, txs, self.signer);
        let ev = fold(&mut self.w.l, &b, Some(&mut self.book));
        fold(&mut self.replica, &b, None);
        self.w.blocks.push(b);
        ev
    }

    fn post_job(&mut self, daa: u64, prompt: &[u32], n: u32, nonce: u8) -> misaka_palw_kernel::job::KernelJobV1 {
        self.post_job_n(daa, prompt, n, [nonce; 64])
    }

    fn post_job_n(&mut self, daa: u64, prompt: &[u32], n: u32, nonce: Digest) -> misaka_palw_kernel::job::KernelJobV1 {
        let job = misaka_palw_kernel::job::KernelJobV1 {
            class_binding_id: self.w.class,
            prompt: prompt.to_vec(),
            max_new_tokens: n,
            decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
            nonce,
        };
        let ev = self.blk(daa, vec![T::PostJob { job: job.clone() }]);
        assert!(ev.contains(&E::JobPosted { job: job.id() }), "{ev:?}");
        job
    }
}

/// A fresh node: the genesis configuration and the ENCODED blocks, nothing else.
fn fresh(w: &World) -> KernelLedgerV1 {
    let mut l = w.genesis.clone();
    for b in &w.blocks {
        fold(&mut l, b, None);
    }
    l
}

/// A fresh outsider: replays from encoded bytes, then reads that ledger, the public DA and the public artifact only.
fn outsider2(w: &World, claim: Digest, da: &Da) -> OutsiderFindingV1 {
    let f = fresh(w);
    assert_eq!(f.root(), w.l.root(), "a fresh node from the encoded blocks reaches the same state root");
    OutsiderV1 { ledger: &f, claim, material: da, artifact: &w.params, salt: [0xC4; 64] }.check().unwrap()
}

/// The same outsider over the incrementally maintained replica (built from the encoded blocks only; equal root asserted).
fn outsider_replica(n: &Net, claim: Digest, da: &Da) -> OutsiderFindingV1 {
    assert_eq!(n.replica.root(), n.w.l.root());
    OutsiderV1 { ledger: &n.replica, claim, material: da, artifact: &n.w.params, salt: [0xC4; 64] }.check().unwrap()
}

fn proof_of(f: OutsiderFindingV1) -> ProsecutionV1 {
    match f {
        OutsiderFindingV1::Prosecute(p) => p,
        other => panic!("expected a prosecution, got {other:?}"),
    }
}

fn state_of(n: &Net, id: &Digest) -> ClaimStateV1 {
    n.w.l.claims[id].life.state.clone()
}

/// A lying claim committed and covered by a colluding Panel, its DA fully published.
fn lying_covered(
    n: &mut Net,
    daa_post: u64,
    daa_commit: u64,
    nonce: u8,
) -> (Digest, (u32, u16, u16), Da, misaka_palw_kernel::trace::TraceV1) {
    let job = n.post_job(daa_post, &[3, 17, 9], 3, nonce);
    let (at, lie) = n.w.lying(&job, 3);
    let (id, da, trace) = (lie.claim.id(), Da::publishing(&lie.trace, &[]), lie.trace.clone());
    n.blk(daa_commit, vec![lie.tx, T::PanelCovered { claim: id }]);
    (id, at, da, trace)
}

fn convicted2(ev: &[E]) -> Option<(u64, u64, bool)> {
    convicted(ev)
}

// ── A: the producer and every Panel seat collude ─────────────────────────────────────────────────────────────────────────

/// EXPECTED (before running): a fully colluding Panel's cover does not protect a lying claim. One ordinary bond outside the
/// Panel, built fresh from encoded bytes + public DA, convicts it before Final (never Final, no reward) and after Final (slash,
/// reward already paid stays paid), and a proof past the liability horizon is refused before any court runs, for no fee (since C4
/// round 3, `KernelLedgerV1::proof_open`; round 1 dismissed it for a fee). Settlement accounting balances in every block.
/// VERDICT: PASS.
#[test]
fn a_producer_and_every_panel_seat_colluding_lose_to_one_ordinary_bond_with_a_fresh_verifier() {
    let mut n = Net::new();
    // Pre-Final.
    let (id, _, da, _) = lying_covered(&mut n, 2, 10, 1);
    assert!(matches!(state_of(&n, &id), ClaimStateV1::ProbabilisticPass { .. }), "the colluding Panel passed it");
    let proof = proof_of(outsider2(&n.w, id, &da));
    let ev = n.blk(30, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted2(&ev), Some((1000, 500, false)), "{ev:?}");
    n.blk(300, vec![]);
    assert!(!n.w.l.claims[&id].rewarded && matches!(state_of(&n, &id), ClaimStateV1::Convicted { .. }));
    // Post-Final.
    let (id, _, da, _) = lying_covered(&mut n, 310, 320, 2);
    let ev = n.blk(370, vec![]);
    assert!(ev.contains(&E::Final { claim: id, reward: 7 }), "{ev:?}");
    let proof = proof_of(outsider2(&n.w, id, &da));
    let ev = n.blk(400, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted2(&ev), Some((1000, 500, true)), "post-Final liability: {ev:?}");
    // Past the horizon.
    let (id, _, da, _) = lying_covered(&mut n, 410, 420, 3);
    n.blk(470, vec![]);
    n.blk(700, vec![]);
    let proof = proof_of(outsider2(&n.w, id, &da));
    let collateral = n.book.coll[&OUTSIDER];
    let ev = n.blk(701, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert!(matches!(&ev[..], [E::Refused { why, .. }] if why.contains("past the liability horizon")), "{ev:?}");
    assert_eq!(n.book.coll[&OUTSIDER], collateral, "refused before any court: no fee");
}

/// EXPECTED (before running): the liability promise ("a proof inside `liability_daa` slashes a Final claim even if every Panel seat
/// signed it") holds against a producer whose Sybil files a demand and lets it default. FAILS today: a post-Final default forfeits the
/// WHOLE reservation (half to the demander, half burned) and the claim keeps Final with `reserved == 0`, so the honest outsider's
/// perfectly valid proof, filed 1 block later and far inside the horizon, is dismissed for a fee. The Sybil recovers the
/// accuser's share of its own forfeit, and the effective liability horizon collapses to `court_deadline_daa`.
#[test]
fn a_post_final_sybil_demand_default_must_not_erase_the_liability_of_a_lying_final_claim() {
    let mut n = Net::new();
    let (id, _, da, _) = lying_covered(&mut n, 2, 10, 1);
    n.blk(60, vec![]);
    assert!(matches!(state_of(&n, &id), ClaimStateV1::Final { .. }));
    let proof = proof_of(outsider2(&n.w, id, &da));
    // The producer's friend demands any position right after Final; the producer stays silent.
    n.blk(61, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 }]);
    let ev = n.blk(81, vec![]);
    assert!(matches!(&ev[..], [E::PostFinalDefault { forfeited: 1000, .. }]), "{ev:?}");
    let sybil_paid = n.book.paid.get(&SPAM1).copied().unwrap_or(0);
    eprintln!("F-C4-02: producer lost 1000, its Sybil was paid {sybil_paid}, burned {}", n.book.burned);
    // The honest outsider files at 82: day 22 of a 200-DAA horizon, with a valid proof from public DA.
    let ev = n.blk(82, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert!(convicted2(&ev).is_some(), "the in-horizon proof must still convict a lying Final claim: {ev:?}");
}

/// EXPECTED (before running): the same attack BEFORE Final costs the colluders nothing but also gains nothing: the claim goes
/// Unavailable (never Final, no reward), the Sybil gets the penalty back, and the outsider who is fast enough (inside the court
/// deadline) still convicts. VERDICT: PASS. Round 1 documented that a slow outsider lost the conviction; since C4 F-C4R3-02 a
/// pre-Final default keeps the rest of the reservation liable to `daa + liability_daa`, so the slow proof convicts too (the
/// remaining 900; the demander took its accuser share of the 100 penalty, the rest burned).
#[test]
fn a_pre_final_sybil_demand_default_cannot_stop_a_fast_outsider_and_never_yields_a_reward() {
    let mut n = Net::new();
    let (id, _, da, _) = lying_covered(&mut n, 2, 10, 1);
    let proof = proof_of(outsider2(&n.w, id, &da));
    n.blk(11, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 }]);
    // Fast outsider: inside the 20-DAA court deadline.
    let ev = n.blk(30, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: proof.clone() }]);
    assert_eq!(convicted2(&ev), Some((1000, 500, false)), "{ev:?}");
    assert!(n.w.l.demands.is_empty(), "the Sybil's demand is moot and its bond returns");

    // Slow outsider: the claim is Unavailable by then; the fraud is not convicted, but it earned nothing and cost the producer
    // exactly the penalty (which its own Sybil collected).
    let (id2, _, da2, _) = lying_covered(&mut n, 100, 110, 2);
    let proof2 = proof_of(outsider2(&n.w, id2, &da2));
    n.blk(111, vec![T::FileDemand { demander: SPAM2, claim: id2, stage: 0, position: 0 }]);
    let ev = n.blk(131, vec![]);
    assert!(matches!(&ev[..], [E::ProducerDefault { penalty: 100, .. }]), "{ev:?}");
    let ev = n.blk(132, vec![T::FileProof { accuser: OUTSIDER, claim: id2, proof: proof2 }]);
    assert_eq!(convicted2(&ev), Some((900, 450, false)), "a slow proof still convicts inside the liability horizon: {ev:?}");
    n.blk(500, vec![]);
    assert!(!n.w.l.claims[&id2].rewarded && n.w.l.claims[&id2].convicted);
}

// ── work plagiarism ─────────────────────────────────────────────────────────────────────────────────────────────────────

/// EXPECTED (before running): a second bond that re-signs an honest claim's PUBLIC evidence and commitments verbatim (it ran
/// nothing) must not earn a Final reward for the same job. FAILS today: nothing binds a trace to its producer and the ledger
/// admits any number of claims per job, so the copyist is covered, reaches Final and is paid `claim_reward` for zero compute.
#[test]
fn a_bond_that_copies_a_published_claim_must_not_be_paid_for_the_same_job_twice() {
    let mut n = Net::new();
    let job = n.post_job(2, &[3, 17, 9], 3, 1);
    let h = n.w.honest(&job, 3);
    let T::CommitClaim { claim, evidence, commitments } = h.tx.clone() else { unreachable!() };
    let id = claim.id();
    n.blk(10, vec![h.tx.clone(), T::PanelCovered { claim: id }]);
    // The copyist reads the chain, re-signs the same bytes with its own bond, and the Panel (replaying a CORRECT trace) covers it.
    let copy = KernelClaimV1 { producer_bond: SPAM1, ..claim };
    let cid = copy.id();
    let ev = n.blk(11, vec![T::CommitClaim { claim: copy, evidence, commitments }, T::PanelCovered { claim: cid }]);
    let accepted = ev.contains(&E::ClaimCommitted { claim: cid });
    n.blk(70, vec![]);
    let rewarded = n.w.l.claims.get(&cid).is_some_and(|r| r.rewarded);
    eprintln!("F-C4-03: copy accepted={accepted}, copy rewarded={rewarded}, issued={}", n.book.issued);
    assert!(!rewarded, "the copyist ran nothing and must not be paid; total issued {}", n.book.issued);
}

// ── B / G / H: garbage, input substitution, output substitution ─────────────────────────────────────────────────────────

/// EXPECTED (before running): every node of the class at one position, lied about one at a time (a +1 inside the dtype), is either
/// refused at inclusion or found by a fresh outsider from public material and convicted by the ledger's court with the correct
/// money flow; the honest claim of the same job is never convicted by any of those proofs. VERDICT: PASS.
#[test]
fn a_single_lie_at_any_node_of_position_one_is_refused_at_inclusion_or_convicted_and_never_survives_clean() {
    let mut n = Net::new();
    let job = n.post_job(2, &[3, 17, 9], 3, 1);
    let honest = n.w.honest(&job, 3);
    let hid = honest.claim.id();
    n.blk(3, vec![honest.tx.clone(), T::PanelCovered { claim: hid }]);
    let occ = n.w.program.occurrences();
    let mut stats: BTreeMap<String, u32> = BTreeMap::new();
    let mut daa = 10u64;
    let mut k = 0u8;
    // C4_EXHAUSTIVE=1 lies at every node (385 of them, ~210 s in a debug build); the default checks every 4th.
    let stride = if std::env::var("C4_EXHAUSTIVE").is_ok() { 1 } else { 4 };
    for s in 0..occ.len() {
        let nodes = n.w.program.blocks[occ[s].0 as usize].nodes.len();
        for nd in (0..nodes).filter(|nd| (s * 1000 + nd) % stride == 0) {
            // One freshly funded producer bond per lie, so a refusal can never be an artefact of exhausted collateral.
            k = k.wrapping_add(1);
            let bond = {
                let mut b = [0xB0; 64];
                b[63] = k;
                b
            };
            daa += 1;
            let accuser = {
                let mut b = [0xAC; 64];
                b[63] = k;
                b
            };
            n.blk(daa, vec![T::RegisterBond { bond, collateral: 2000 }, T::RegisterBond { bond: accuser, collateral: 100 }]);
            // One claim per job: each lie answers its own job (same prompt, its own nonce).
            let mut nonce = [0x3E; 64];
            nonce[..8].copy_from_slice(&((s * 1000 + nd) as u64).to_le_bytes());
            daa += 1;
            let lie_job = n.post_job_n(daa, &[3, 17, 9], 3, nonce);
            let generated = n.w.greedy(&n.w.params, &lie_job.prompt, 3);
            let p = n.w.produce(&lie_job, bond, generated, &n.w.params.clone(), |t| common::bump(&mut t.values[1][s][nd], 0));
            let id = p.claim.id();
            let da = Da::publishing(&p.trace, &[]);
            daa += 1;
            let ev = n.blk(daa, vec![p.tx]);
            if !ev.contains(&E::ClaimCommitted { claim: id }) {
                let why = refused(&ev).expect("a refusal reason");
                assert!(
                    why.starts_with("malformed evidence") || why.starts_with("binding fault"),
                    "({s},{nd}) refused for a reason that is not the lie's: {why}"
                );
                *stats.entry(why.chars().take(40).collect()).or_insert(0) += 1;
                continue;
            }
            match outsider_replica(&n, id, &da) {
                OutsiderFindingV1::Prosecute(proof) => {
                    // The same proof against the HONEST claim must be dismissed.
                    daa += 1;
                    let ev = n.blk(daa, vec![T::FileProof { accuser, claim: hid, proof: proof.clone() }]);
                    assert!(convicted2(&ev).is_none(), "an honest claim convicted by a proof of another claim's lie at ({s},{nd})");
                    daa += 1;
                    let ev = n.blk(daa, vec![T::FileProof { accuser, claim: id, proof }]);
                    assert_eq!(convicted2(&ev), Some((1000, 500, false)), "({s},{nd}): {ev:?}");
                    *stats.entry("convicted".into()).or_insert(0) += 1;
                }
                OutsiderFindingV1::Clean => {
                    panic!("a lie at ({s},{nd}) of position 1 was committed and found CLEAN by a fresh outsider")
                }
                OutsiderFindingV1::Demand(d) => panic!("({s},{nd}): all material was public yet the outsider demands {d:?}"),
            }
        }
    }
    eprintln!("per-node lie outcomes: {stats:?}");
    assert!(stats.get("convicted").copied().unwrap_or(0) > 0);
}

/// EXPECTED (before running): a garbage trace (every weight perturbed, self-consistent) is convicted against the public artifact;
/// a trace borrowed from another job, a substituted delivered token (last and mid-stream) and a prompt substitution are refused at
/// inclusion or convicted. VERDICT: PASS.
#[test]
fn garbage_borrowed_and_substituted_traces_are_all_refused_or_convicted() {
    let mut n = Net::new();
    let mut garbage = n.w.params.clone();
    for t in garbage.tensors.values_mut() {
        for i in 0..t.data.len() {
            common::bump(t, i);
        }
    }
    let job = n.post_job(2, &[3, 17, 9], 3, 1);
    let g = {
        let generated = n.w.greedy(&garbage, &job.prompt, 3);
        n.w.produce(&job, PRODUCER, generated, &garbage, |_| {})
    };
    let (gid, gda) = (g.claim.id(), Da::publishing(&g.trace, &[]));
    n.blk(10, vec![g.tx, T::PanelCovered { claim: gid }]);
    let proof = proof_of(outsider2(&n.w, gid, &gda));
    let ev = n.blk(11, vec![T::FileProof { accuser: OUTSIDER, claim: gid, proof }]);
    assert_eq!(convicted2(&ev), Some((1000, 500, false)), "garbage: {ev:?}");

    // Borrowed from another job (different prompt).
    let j1 = n.post_job(12, &[3, 17, 9], 3, 2);
    let j2 = n.post_job(13, &[4, 18, 10], 3, 2);
    let h1 = n.w.honest(&j1, 3);
    let T::CommitClaim { claim, evidence, commitments } = h1.tx.clone() else { unreachable!() };
    let ev = n.blk(
        14,
        vec![T::CommitClaim {
            claim: KernelClaimV1 { job_id: j2.id(), ..claim.clone() },
            evidence: evidence.clone(),
            commitments: commitments.clone(),
        }],
    );
    assert_eq!(refused(&ev).as_deref(), Some("binding fault WrongInput"), "{ev:?}");

    // Input substitution: the trace of a DIFFERENT prompt under this job's id (re-rooting the evidence honestly).
    let other = n.w.produce(&j2, PRODUCER, n.w.greedy(&n.w.params, &j2.prompt, 3), &n.w.params.clone(), |_| {});
    let T::CommitClaim { claim: oc, evidence: oe, commitments: ocm } = other.tx.clone() else { unreachable!() };
    let sub = KernelClaimV1 { job_id: j1.id(), evidence_root: oe.root(), ..oc };
    let ev = n.blk(15, vec![T::CommitClaim { claim: sub, evidence: oe, commitments: ocm }]);
    assert!(refused(&ev).is_some(), "an input-substituted trace was committed: {ev:?}");

    // Output substitution: last token (never fed back) and a mid-stream token.
    let mut last = n.w.greedy(&n.w.params, &j1.prompt, 3);
    last[2] = (last[2] + 1) % n.w.program.token_bound;
    let s = n.w.produce(&j1, PRODUCER, last, &n.w.params.clone(), |_| {});
    let (sid, sda) = (s.claim.id(), Da::publishing(&s.trace, &[]));
    n.blk(16, vec![s.tx, T::PanelCovered { claim: sid }]);
    let proof = proof_of(outsider2(&n.w, sid, &sda));
    assert!(matches!(proof, ProsecutionV1::Decode(_)));
    let ev = n.blk(17, vec![T::FileProof { accuser: OUTSIDER, claim: sid, proof }]);
    assert_eq!(convicted2(&ev), Some((1000, 500, false)), "last token: {ev:?}");

    let mut mid = n.w.greedy(&n.w.params, &j2.prompt, 3);
    mid[0] = (mid[0] + 1) % n.w.program.token_bound;
    let s = n.w.produce(&j2, PRODUCER, mid, &n.w.params.clone(), |_| {});
    let (sid, sda) = (s.claim.id(), Da::publishing(&s.trace, &[]));
    n.blk(18, vec![s.tx, T::PanelCovered { claim: sid }]);
    let proof = proof_of(outsider2(&n.w, sid, &sda));
    let ev = n.blk(19, vec![T::FileProof { accuser: OUTSIDER, claim: sid, proof }]);
    assert_eq!(convicted2(&ev), Some((1000, 500, false)), "mid-stream token: {ev:?}");
}

// ── L / M: withheld and fake material ──────────────────────────────────────────────────────────────────────────────────

/// EXPECTED (before running): the producer publishes everything but the one convicting position. The fresh outsider demands exactly
/// that position; fake material of every shape (transposed shape, other dtype, one wrong element, a valid row where the whole is
/// owed, an opening of another position's tensor, a third party's garbage) is rejected with the right class and never serves;
/// silence ends in `ProducerDefault` (not a conviction); a THIRD PARTY serving the true bytes does serve (a DA provider is not a
/// computation authority: the served bytes are re-authenticated against the commitments). VERDICT: PASS.
#[test]
fn withheld_material_fake_material_and_a_third_party_server_are_classified_and_never_convict_by_themselves() {
    let mut n = Net::new();
    let job = n.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = n.w.lying(&job, 3);
    let (id, trace) = (lie.claim.id(), lie.trace.clone());
    let da = Da::publishing(&lie.trace, &[at, (3, 0, 0)]);
    n.blk(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Demand(pos) = outsider2(&n.w, id, &da) else { panic!("withheld material is demanded, never convicted") };
    assert_eq!(pos, vec![(0, at.0), (0, 3)]);
    n.blk(11, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);

    use misaka_palw_kernel::merkle::TensorOpeningV1;
    use misaka_palw_kernel::public::{MaterialResponseV1 as M, TensorWireV1};
    let (s, nd) = (at.1 as usize, at.2 as usize);
    let committed = trace.values[at.0 as usize][s][nd].clone();
    let transposed = {
        let mut w = TensorWireV1::of(&committed);
        w.shape.reverse();
        w
    };
    let other_dtype = {
        let mut w = TensorWireV1::of(&committed);
        w.dtype = if w.dtype == 4 { 5 } else { 4 };
        w
    };
    let mut off_by_one = committed.clone();
    common::bump(&mut off_by_one, 0);
    let row_only = TensorOpeningV1::row(&committed, 0).unwrap();
    let neighbour = trace.values[at.0 as usize][s][(nd + 1) % trace.values[at.0 as usize][s].len()].clone();
    let attempts: Vec<(&str, Vec<u8>)> = vec![
        ("transposed", position(&trace, at.0, |r| r[s][nd] = M::Whole(transposed))),
        ("other dtype", position(&trace, at.0, |r| r[s][nd] = M::Whole(other_dtype))),
        ("one element off", position(&trace, at.0, |r| r[s][nd] = M::Whole(TensorWireV1::of(&off_by_one)))),
        ("a row, whole owed", position(&trace, at.0, |r| r[s][nd] = M::Part(row_only))),
        ("another node's tensor", position(&trace, at.0, |r| r[s][nd] = M::Whole(TensorWireV1::of(&neighbour)))),
        ("garbage bytes", vec![0xAB; 64]),
        ("empty", vec![]),
    ];
    for (what, bytes) in attempts {
        let ev = n.blk(12, vec![T::Respond { claim: id, stage: 0, position: at.0, bytes }]);
        assert!(matches!(&ev[..], [E::ResponseRejected { .. }]) || refused(&ev).is_some(), "{what}: {ev:?}");
        assert!(!n.w.l.served.contains_key(&(id, 0, at.0)), "{what} served a position");
        assert!(convicted2(&ev).is_none(), "{what} convicted");
    }
    // A third party (any bond) serves the TRUE bytes: that is service.
    let ev = n.blk(13, vec![T::Respond { claim: id, stage: 0, position: at.0, bytes: position(&trace, at.0, |_| {}) }]);
    assert_eq!(ev, vec![E::Served { claim: id, stage: 0, position: at.0 }]);
    // The values are public from now: the outsider convicts with them; it is the court that convicts, never the DA provider.
    let da2 = Da::publishing(&trace, &[(3, 0, 0)]);
    let _ = da2;
    let OutsiderFindingV1::Demand(rest) = outsider2(&n.w, id, &da) else { panic!("position 3 is still withheld") };
    assert_eq!(rest, vec![(0, 3)]);

    // Silence on the other: default, never fraud.
    n.blk(20, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 3 }]);
    let coll = n.w.l.bonds[&PRODUCER].collateral;
    let ev = n.blk(40, vec![]);
    assert!(matches!(&ev[..], [E::ProducerDefault { position: 3, .. }]), "{ev:?}");
    assert_eq!(n.w.l.bonds[&PRODUCER].collateral, coll - 100, "a penalty, never the fraud slash");
    assert!(!n.w.l.claims[&id].convicted);
}

// ── N / O / P / Q: pre-emption, simultaneous challengers, last block before Final, invalid-challenge spam ──────────────────

/// EXPECTED (before running): demands on every position by hostile bonds cannot pre-empt the direct proof; three outsiders filing
/// the same valid proof in one block → one conviction, one reward, two `Duplicate`s with no second slash and no fee; the accounting
/// balances. VERDICT: PASS.
#[test]
fn court_pre_emption_and_simultaneous_challengers_convict_once_and_pay_once() {
    let mut n = Net::new();
    let (id, _, da, _) = lying_covered(&mut n, 2, 10, 1);
    let proof = proof_of(outsider2(&n.w, id, &da));
    let demands: Vec<T> =
        (0..5).map(|p| T::FileDemand { demander: if p % 2 == 0 { SPAM1 } else { SPAM2 }, claim: id, stage: 0, position: p }).collect();
    n.blk(11, demands);
    assert_eq!(n.w.l.demands.len(), 5);
    let c = n.book.coll[&OUTSIDER];
    let ev = n.blk(
        12,
        vec![
            T::FileProof { accuser: OUTSIDER, claim: id, proof: proof.clone() },
            T::FileProof { accuser: SPAM1, claim: id, proof: proof.clone() },
            T::FileProof { accuser: SPAM2, claim: id, proof: proof.clone() },
        ],
    );
    assert_eq!(ev.iter().filter(|e| matches!(e, E::Convicted { .. })).count(), 1, "{ev:?}");
    assert_eq!(ev.iter().filter(|e| matches!(e, E::Duplicate { .. })).count(), 2, "{ev:?}");
    assert_eq!(n.book.paid.get(&OUTSIDER).copied().unwrap_or(0), 500);
    assert_eq!(n.book.paid.get(&SPAM1).copied().unwrap_or(0), 0);
    assert_eq!(n.book.coll[&OUTSIDER], c, "no fee for a convicting or duplicate filing");
    assert!(n.w.l.demands.is_empty(), "the six sessions are moot and every bond returned");
    for b in [SPAM1, SPAM2, OUTSIDER] {
        assert_eq!(n.w.l.bonds[&b].reserved, 0);
    }
}

/// EXPECTED (before running): a proof filed in the block that also closes the challenge window is applied before the tick: the
/// claim never finalizes and the reward is never issued; a proof one block later is a post-Final conviction (reward already paid
/// stays paid); and a demand served in the window's last block holds Final for the grace so the enabled proof still lands before
/// Final. VERDICT: PASS.
#[test]
fn a_proof_at_the_last_block_before_final_wins_and_one_block_after_is_a_post_final_conviction() {
    let mut n = Net::new();
    let (id, _, da, _) = lying_covered(&mut n, 2, 10, 1); // window end 60
    let proof = proof_of(outsider2(&n.w, id, &da));
    let ev = n.blk(60, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted2(&ev), Some((1000, 500, false)), "{ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, E::Final { .. })), "{ev:?}");
    // GAP-5: a Final reward is the job's escrow routed to the producer, never issued: none was routed here.
    assert_eq!((n.book.issued, n.book.final_rewards), (0, 0), "no reward was paid");

    let (id, _, da, _) = lying_covered(&mut n, 100, 110, 2); // window end 160
    let proof = proof_of(outsider2(&n.w, id, &da));
    let ev = n.blk(160, vec![]);
    assert!(ev.contains(&E::Final { claim: id, reward: 7 }));
    let ev = n.blk(161, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted2(&ev), Some((1000, 500, true)), "{ev:?}");
    assert_eq!((n.book.issued, n.book.final_rewards), (0, 7), "the reward paid at Final (the job's escrow, GAP-5) stays paid");
}

/// EXPECTED (before running): 300 junk proofs (garbage bytes, a valid-looking proof for another claim, a decode accusation of a
/// correct token) from 3 bonds cost one fee each from FREE collateral and are refused for lack of it when it runs out; the block
/// budget bounds the court work; the claim, the lifecycle and the other bonds are unchanged; the honest outsider's proof in a
/// later block still convicts. VERDICT: PASS.
#[test]
fn invalid_challenge_spam_is_priced_bounded_and_changes_nothing() {
    let mut n = Net::new();
    let job = n.post_job(2, &[3, 17, 9], 3, 1);
    let h = n.w.honest(&job, 3);
    let hid = h.claim.id();
    n.blk(3, vec![h.tx, T::PanelCovered { claim: hid }]);
    let (lid, _, lda, _) = lying_covered(&mut n, 4, 5, 2);
    let lproof = proof_of(outsider2(&n.w, lid, &lda));
    let before = state_of(&n, &hid);
    let junk: Vec<T> = (0..300)
        .map(|i| T::FileProof {
            accuser: [SPAM1, SPAM2, OUTSIDER][i % 3],
            claim: hid,
            proof: if i % 2 == 0 { ProsecutionV1::Kernel(vec![i as u8; 40]) } else { lproof.clone() },
        })
        .collect();
    let ev = n.blk(6, junk);
    let dismissed = ev.iter().filter(|e| matches!(e, E::ProofDismissed { .. })).count();
    let over = ev.iter().filter(|e| matches!(e, E::Refused { why, .. } if why.contains("budget"))).count();
    eprintln!("junk filings: {dismissed} dismissed (fee), {over} over budget");
    assert!(dismissed <= 64 + 3 * 200, "bounded by budget and collateral");
    assert!(convicted2(&ev).is_none());
    assert_eq!(state_of(&n, &hid), before, "an honest claim's lifecycle is untouched by dismissed filings");
    let ev = n.blk(7, vec![T::FileProof { accuser: SPAM1, claim: lid, proof: lproof }]);
    // SPAM1's collateral may be exhausted by fees; whoever can pay convicts.
    assert!(convicted2(&ev).is_some() || refused(&ev).is_some(), "{ev:?}");
}

/// EXPECTED (before running, round 1): bonds burning the block's adjudication budget with structurally-malformed `CommitClaim`s
/// (charged before the collateral check) push the honest `FileProof` behind them in the SAME block to `OverBudget`; the next block
/// convicts (OBSERVATION O-C4-06: the junk claim costs no ledger fee). Since round 1 the attack is narrower and the proof is no
/// longer delayed: seal-then-reveal checks the seal BEFORE the budget (O-C4-14) and a claim seal is bonded (`seal_deposit`), so a
/// junk bond must hold (and, its reveal refused, forfeit at the seal's expiry) one seal deposit; and the block budget keeps runs
/// reserved for proofs, so junk claims exhaust only the claims' share — the proof convicts in the same block.
#[test]
fn a_block_budget_exhausted_by_free_junk_claims_delays_a_proof_by_one_block_and_never_loses_it() {
    let mut n = Net::new();
    let (id, _, da, _) = lying_covered(&mut n, 2, 10, 1);
    let proof = proof_of(outsider2(&n.w, id, &da));
    let job = n.post_job(11, &[3, 17, 9], 3, 9);
    let h = n.w.honest(&job, 3);
    let T::CommitClaim { claim, evidence, commitments } = h.tx.clone() else { unreachable!() };
    // Malformed (fabricated segment boundary) but passing the cheap checks: it spends one adjudication.
    let mut fabricated = evidence.clone();
    fabricated.segments[0].exit_state_root = [1; 64];
    let seal_deposit = n.w.l.policy.seal_deposit;
    // Distinct bonds holding exactly one seal deposit give distinct claim ids that pass every cheap check (they can seal; their
    // collateral cannot back a claim, which is checked after the budget is charged).
    let junk: Vec<Digest> = (0..70u8)
        .map(|i| {
            let mut b = [0xE0; 64];
            b[63] = i;
            b
        })
        .collect();
    n.blk(12, junk.iter().map(|bond| T::RegisterBond { bond: *bond, collateral: seal_deposit }).collect());
    let mut txs = Vec::new();
    for bond in &junk {
        let c = KernelClaimV1 { evidence_root: fabricated.root(), producer_bond: *bond, ..claim.clone() };
        txs.push(T::CommitClaim { claim: c, evidence: fabricated.clone(), commitments: commitments.clone() });
    }
    txs.push(T::FileProof { accuser: OUTSIDER, claim: id, proof: proof.clone() });
    // The junk claims are sealed (by their bonds, at DAA 12) and revealed at 13, with the proof behind them.
    let ev = n.blk(13, txs);
    let over = ev.iter().filter(|e| matches!(e, E::Refused { why, .. } if why.contains("budget"))).count();
    eprintln!("over-budget refusals in the block: {over}");
    assert!(over >= 1, "the junk claims exhausted the claims' share of the block budget: {ev:?}");
    assert_eq!(convicted2(&ev), Some((1000, 500, false)), "the proof's reserved run convicts in the same block: {ev:?}");
    let ev = n.blk(14, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert!(convicted2(&ev).is_none(), "a second proof after the conviction convicts nothing again: {ev:?}");
}

/// The round-1 shape of the same attack, now closed: zero-collateral bonds cannot seal (a claim seal is bonded), so their junk
/// claims are refused at the seal check before any budget is spent (O-C4-14) and the proof convicts in the SAME block.
#[test]
fn free_junk_claims_from_unsealable_bonds_spend_no_budget_and_the_proof_convicts_at_once() {
    let mut n = Net::new();
    let (id, _, da, _) = lying_covered(&mut n, 2, 10, 1);
    let proof = proof_of(outsider2(&n.w, id, &da));
    let job = n.post_job(11, &[3, 17, 9], 3, 9);
    let h = n.w.honest(&job, 3);
    let T::CommitClaim { claim, evidence, commitments } = h.tx.clone() else { unreachable!() };
    let mut fabricated = evidence.clone();
    fabricated.segments[0].exit_state_root = [1; 64];
    let mut txs = Vec::new();
    for i in 0..70u8 {
        let bond = {
            let mut b = [0xE0; 64];
            b[63] = i;
            b
        };
        txs.push(T::RegisterBond { bond, collateral: 0 });
        let c = KernelClaimV1 { evidence_root: fabricated.root(), producer_bond: bond, ..claim.clone() };
        txs.push(T::CommitClaim { claim: c, evidence: fabricated.clone(), commitments: commitments.clone() });
    }
    txs.push(T::FileProof { accuser: OUTSIDER, claim: id, proof });
    let ev = n.blk(12, txs);
    let unsealed = ev.iter().filter(|e| matches!(e, E::Refused { why, .. } if why.contains("no seal"))).count();
    let over = ev.iter().filter(|e| matches!(e, E::Refused { why, .. } if why.contains("budget"))).count();
    assert_eq!((unsealed, over), (70, 0), "every junk claim is refused at its seal, none at the budget: {ev:?}");
    assert_eq!(convicted2(&ev), Some((1000, 500, false)), "{ev:?}");
}

// ── R / T: restart mid-prosecution, replay, duplicate proof after replay ───────────────────────────────────────────────────

/// EXPECTED (before running): a node that restarts (or IBDs) at ANY block boundary from the encoded blocks and continues reaches
/// the same state root after every following block as the uninterrupted node; replaying the conviction block again after the
/// restart is a `Duplicate` (no second slash) and leaves the root unchanged. VERDICT: PASS.
#[test]
fn restart_at_every_block_boundary_mid_prosecution_reaches_the_same_roots_and_a_replayed_proof_is_a_duplicate() {
    let mut n = Net::new();
    let (id, _, da, _) = lying_covered(&mut n, 2, 10, 1);
    n.blk(11, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 2 }]);
    let proof = proof_of(outsider2(&n.w, id, &da));
    n.blk(15, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: proof.clone() }]);
    n.blk(16, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    n.blk(300, vec![]);
    // The reference run's root after every block.
    let mut roots = Vec::new();
    let mut r = n.w.genesis.clone();
    for b in &n.w.blocks {
        fold(&mut r, b, None);
        roots.push(r.root());
    }
    assert_eq!(*roots.last().unwrap(), n.w.l.root());
    // A node that is killed after block k and restarted from the encoded prefix, then continues.
    for k in 0..n.w.blocks.len() {
        let mut node = n.w.genesis.clone();
        for b in &n.w.blocks[..=k] {
            fold(&mut node, b, None);
        }
        assert_eq!(node.root(), roots[k], "restart after block {k}");
        let mut restarted = node.clone(); // "persisted" state
        for (j, b) in n.w.blocks.iter().enumerate().skip(k + 1) {
            fold(&mut restarted, b, None);
            assert_eq!(restarted.root(), roots[j], "restart after {k}, then block {j}");
        }
    }
    // Duplicate after replay: the conviction block applied once more to the final state.
    let conv_block = n.w.blocks.iter().find(|b| b.daa == 15).unwrap().clone();
    let mut again = n.w.l.clone();
    let before = again.root();
    again.begin_block(300).unwrap();
    for tx in &conv_block.txs {
        if let LedgerTxV1::Object { auth, object } = tx {
            let ev = again.apply_encoded(&object.encode(), auth).unwrap();
            assert_eq!(ev, vec![E::Duplicate { claim: id }]);
        }
    }
    assert_eq!(again.root(), before);
}

// ── U: bond exit while liability exists ────────────────────────────────────────────────────────────────────────────────────

/// EXPECTED (before running): a producer that requests exit as soon as its lying claim is Final cannot withdraw while the
/// reservation stands (even after the exit delay), cannot start new claims, can still be convicted inside the horizon; after the
/// horizon the remaining collateral withdraws exactly. A demander cannot withdraw while its demand bond is reserved. VERDICT: PASS.
#[test]
fn a_bond_cannot_leave_while_a_claim_or_a_demand_holds_a_reservation() {
    let mut n = Net::new();
    let (id, _, da, _) = lying_covered(&mut n, 2, 10, 1);
    let job = n.post_job(11, &[3, 17, 9], 3, 9);
    n.blk(60, vec![]);
    let ev = n.blk(61, vec![T::RequestExit { bond: PRODUCER }]);
    assert!(ev.contains(&E::ExitRequested { bond: PRODUCER }));
    let ev = n.blk(200, vec![T::Withdraw { bond: PRODUCER }]);
    assert!(refused(&ev).is_some(), "exit delay over but the reservation stands: {ev:?}");
    let h = n.w.honest(&job, 3);
    let ev = n.blk(202, vec![h.tx]);
    assert!(refused(&ev).is_some(), "an exiting bond starts no new claim: {ev:?}");
    let proof = proof_of(outsider2(&n.w, id, &da));
    let ev = n.blk(203, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted2(&ev), Some((1000, 500, true)), "still convictable inside the horizon: {ev:?}");
    let ev = n.blk(204, vec![T::Withdraw { bond: PRODUCER }]);
    assert!(ev.contains(&E::Withdrawn { bond: PRODUCER, amount: 4000 }), "{ev:?}");
    assert!(!n.w.l.bonds.contains_key(&PRODUCER));
    // A demand bond is a reservation too: no withdrawal while it is held; after the demand's default it returns and the bond leaves.
    let mut n = Net::new();
    let (id2, _, _, _) = lying_covered(&mut n, 2, 10, 3);
    n.blk(11, vec![T::FileDemand { demander: SPAM2, claim: id2, stage: 0, position: 1 }, T::RequestExit { bond: SPAM2 }]);
    let ev = n.blk(25, vec![T::Withdraw { bond: SPAM2 }]);
    assert!(refused(&ev).is_some(), "demand bond still reserved: {ev:?}");
    // Objects are applied before the block's tick, so the deadline's default (which returns the bond) lands in block 45 and the
    // withdrawal in the next one.
    let ev = n.blk(45, vec![T::Withdraw { bond: SPAM2 }]);
    assert!(refused(&ev).is_some() && ev.iter().any(|e| matches!(e, E::ProducerDefault { .. })), "{ev:?}");
    let ev = n.blk(46, vec![T::Withdraw { bond: SPAM2 }]);
    assert!(ev.iter().any(|e| matches!(e, E::Withdrawn { bond, .. } if *bond == SPAM2)), "{ev:?}");
}

// ── what the one-claim-per-job rule opens ───────────────────────────────────────────────────────────────────────────────

/// A claim for `job` whose trace commitments are junk digests of the right SHAPE: no model was run. Built from the public
/// evidence builder, exactly as an honest producer builds its evidence.
fn unbacked_claim(n: &Net, job: &misaka_palw_kernel::job::KernelJobV1, bond: Digest, seed: u8) -> (KernelClaimV1, T) {
    use misaka_palw_kernel::evidence::build_evidence_v1;
    use misaka_palw_kernel::trace::{EvidenceV1, WiringV1};
    let honest = n.w.honest(job, 3);
    let T::CommitClaim { claim, commitments, .. } = honest.tx.clone() else { unreachable!() };
    let mut k = 0u32;
    let fake: Vec<Vec<Vec<Digest>>> = commitments
        .iter()
        .map(|pos| {
            pos.iter()
                .map(|occ| {
                    occ.iter()
                        .map(|_| {
                            k += 1;
                            let mut d = [seed; 64];
                            d[..4].copy_from_slice(&k.to_le_bytes());
                            d
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    let class = &n.w.l.classes[&n.w.class];
    let stream = claim.stream(job);
    let w = WiringV1::new(&n.w.program).unwrap();
    let ev = build_evidence_v1(&w, &EvidenceV1::new(fake.clone()), &stream, class.header(n.w.class), &class.descriptor, 2).unwrap();
    let c = KernelClaimV1 { job_id: job.id(), producer_bond: bond, generated: claim.generated.clone(), evidence_root: ev.root() };
    (c.clone(), T::CommitClaim { claim: c, evidence: ev, commitments: fake })
}

/// EXPECTED (before running): a claim whose trace commitments are junk digests of the right shape (no model run, no compute) is
/// structurally valid — the ledger cannot see values — so it IS committed. With one claim per job, that bond now holds the job,
/// and the honest producer's real claim is refused until the squatter's claim ends. The safe property is that an unbacked claim
/// must not be able to lock a job against honest producers for free. FAILS today (new with the one-claim-per-job rule): the
/// squat costs the attacker nothing (collateral locked, returned at the check window's timeout) and repeats with a fresh bond.
#[test]
fn an_unbacked_claim_must_not_be_able_to_lock_a_job_against_the_honest_producer() {
    let mut n = Net::new();
    n.blk(3, vec![T::RegisterBond { bond: [0xE5; 64], collateral: 1000 }]);
    let job = n.post_job(4, &[3, 17, 9], 3, 1);
    // The squatter: no compute at all.
    let (sq, tx) = unbacked_claim(&n, &job, [0xE5; 64], 0x6B);
    let sid = sq.id();
    let ev = n.blk(5, vec![tx]);
    let squatted = ev.contains(&E::ClaimCommitted { claim: sid });
    // The honest producer answers the same job one block later with a real trace.
    let honest = n.w.honest(&job, 3);
    let hid = honest.claim.id();
    let ev = n.blk(6, vec![honest.tx, T::PanelCovered { claim: hid }]);
    let blocked = !ev.contains(&E::ClaimCommitted { claim: hid });
    eprintln!("F-C4-09: junk claim committed={squatted}; honest producer blocked={blocked}: {:?}", refused(&ev));
    // Without anyone doing anything the squat lasts the whole check window, then the collateral is simply returned.
    n.blk(120, vec![]);
    let released = n.w.l.bonds[&[0xE5; 64]].reserved == 0 && n.w.l.bonds[&[0xE5; 64]].collateral == 1000;
    eprintln!("F-C4-09: after the timeout the squatter is out nothing: released={released}");
    assert!(!blocked, "an honest producer was locked out of its job by a claim that ran nothing");
}

/// EXPECTED (before running): after the F-C4-09 fix an UNCOVERED claim holds nothing, so only a claim the (colluding) Panel covers
/// can squat a job. Such a squatter is still breakable for a profit: a demand on a position of its unbacked claim defaults after the
/// court deadline (the demander collects the penalty), the claim is Unavailable, the job is freed, and the honest producer's claim —
/// whose coverage was refused while the squatter held the job — is covered on the Panel's retry and reaches Final with the reward.
/// VERDICT: PASS.
#[test]
fn a_panel_covered_squatter_is_freed_by_a_demand_default_and_the_honest_producer_then_finalizes() {
    let mut n = Net::new();
    n.blk(3, vec![T::RegisterBond { bond: [0xE5; 64], collateral: 1000 }]);
    let job = n.post_job(4, &[3, 17, 9], 3, 1);
    let (sq, tx) = unbacked_claim(&n, &job, [0xE5; 64], 0x6B);
    let sid = sq.id();
    let honest = n.w.honest(&job, 3);
    let hid = honest.claim.id();
    // Both commit (nothing holds the job yet); the colluding Panel covers the squatter first.
    let ev = n.blk(5, vec![tx, honest.tx.clone()]);
    assert!(ev.contains(&E::ClaimCommitted { claim: sid }) && ev.contains(&E::ClaimCommitted { claim: hid }), "{ev:?}");
    let ev = n.blk(6, vec![T::PanelCovered { claim: sid }, T::PanelCovered { claim: hid }]);
    assert!(refused(&ev).is_some_and(|w| w.contains("one claim per job")), "the honest claim's coverage is refused: {ev:?}");
    assert_eq!(n.w.l.job_claims.get(&job.id()), Some(&sid));
    n.blk(7, vec![T::FileDemand { demander: OUTSIDER, claim: sid, stage: 0, position: 1 }]);
    let ev = n.blk(27, vec![]);
    assert!(matches!(&ev[..], [E::ProducerDefault { penalty: 100, .. }]), "{ev:?}");
    // Split like a slash since C4 F-C4R3-02: the demander takes the accuser share (`accuser_reward_permille`), the rest is burned.
    let share = 100 * u64::from(n.w.l.policy.accuser_reward_permille) / 1000;
    assert_eq!(n.book.paid.get(&OUTSIDER).copied().unwrap_or(0), share, "the demander is paid for breaking the squat");
    assert!(!n.w.l.job_claims.get(&job.id()).is_some_and(|c| n.w.l.claims[c].holds_job()), "the job is free");
    // The Panel retries the honest claim's coverage (it is still inside its check window).
    n.blk(28, vec![T::PanelCovered { claim: hid }]);
    assert_eq!(n.w.l.job_claims.get(&job.id()), Some(&hid));
    let ev = n.blk(90, vec![]);
    assert!(ev.contains(&E::Final { claim: hid, reward: 7 }), "{ev:?}");
}

/// EXPECTED (before running, round 1): the job is freed by a CONVICTION — also a post-Final one — and a second claim then reaches
/// Final: one job pays twice, once to the fraud (whose 1000 is slashed) and once to the honest producer (O-C4-09: the first reward
/// is not clawed back, only the collateral is). Since GAP-5 a Final reward is the job's escrow routed to the producer, and a job
/// escrows one reward: the fraud's 7 stays paid (not clawed back) and the honest second claim finalizes with the escrow spent —
/// the job is paid ONCE. Nothing else about the job can be claimed twice while a holder is live: a second claim by ANY bond
/// (including the same one) is refused while the first holds.
#[test]
fn observation_a_post_final_conviction_frees_the_job_and_the_job_is_paid_a_second_time() {
    let mut n = Net::new();
    let (id, _, da, _) = lying_covered(&mut n, 2, 10, 1);
    let job = n.w.l.jobs.values().next().cloned().unwrap();
    n.blk(60, vec![]);
    let honest = n.w.honest(&job, 3);
    let hid = honest.claim.id();
    let ev = n.blk(61, vec![honest.tx.clone()]);
    assert!(refused(&ev).is_some_and(|w| w.contains("one claim per job")), "a Final holder keeps the job: {ev:?}");
    let proof = proof_of(outsider2(&n.w, id, &da));
    let ev = n.blk(62, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted2(&ev), Some((1000, 500, true)));
    let ev = n.blk(63, vec![honest.tx, T::PanelCovered { claim: hid }]);
    assert!(ev.contains(&E::ClaimCommitted { claim: hid }), "{ev:?}");
    n.blk(120, vec![]);
    assert!(matches!(state_of(&n, &hid), ClaimStateV1::Final { .. }), "the honest second claim finalizes");
    assert_eq!(n.book.final_rewards, 7, "O-C4-09 since GAP-5: the fraud's reward (7) was not clawed back; the job's escrow paid once");
}

/// EXPECTED (before running): the one-claim-per-job rule closes the SAME-job copy but not the same-PROMPT copy: a bond posts its own
/// job with an identical prompt (jobs are free) and answers it with the published claim's evidence and commitments. The copy is
/// accepted, covered and paid. Recorded as residual risk R-C4-10 (design: deterministic output is cacheable; the defence is pricing
/// jobs at least at the reward, which the ledger does not do).
#[test]
fn observation_a_copyist_answers_its_own_identical_prompt_job_with_a_published_trace() {
    let mut n = Net::new();
    n.blk(3, vec![T::RegisterBond { bond: [0xC9; 64], collateral: 1000 }]);
    let job1 = n.post_job(4, &[3, 17, 9], 3, 1);
    let h = n.w.honest(&job1, 3);
    let T::CommitClaim { claim, evidence, commitments } = h.tx.clone() else { unreachable!() };
    n.blk(10, vec![h.tx, T::PanelCovered { claim: claim.id() }]);
    // The copyist's own job: same prompt, other nonce.
    let job2 = n.post_job(11, &[3, 17, 9], 3, 2);
    let evidence2 = misaka_palw_kernel::evidence::VerificationEvidenceV1 { ..evidence.clone() };
    let copy = KernelClaimV1 {
        job_id: job2.id(),
        producer_bond: [0xC9; 64],
        generated: claim.generated.clone(),
        evidence_root: evidence2.root(),
    };
    let cid = copy.id();
    let ev = n.blk(12, vec![T::CommitClaim { claim: copy, evidence: evidence2, commitments }, T::PanelCovered { claim: cid }]);
    let accepted = ev.contains(&E::ClaimCommitted { claim: cid });
    n.blk(80, vec![]);
    eprintln!("R-C4-10: same-prompt copy accepted={accepted}, rewards={}", n.book.final_rewards);
    assert!(accepted && n.book.final_rewards == 14, "residual: one computation, two rewards, two jobs");
}

// ── the seal-then-reveal surface ───────────────────────────────────────────────────────────────────────────────────────────

fn seal_obj(producer: Digest, job: &misaka_palw_kernel::job::KernelJobV1, claim: &KernelClaimV1) -> T {
    T::SealClaim { producer, job: job.id(), seal: misaka_palw_kernel::ledger::claim_seal_v1(&claim.id()) }
}

fn copy_of(claim: &KernelClaimV1, bond: Digest) -> KernelClaimV1 {
    KernelClaimV1 { producer_bond: bond, ..claim.clone() }
}

fn commit_of(c: &KernelClaimV1, tx: &T) -> T {
    let T::CommitClaim { evidence, commitments, .. } = tx.clone() else { unreachable!() };
    T::CommitClaim { claim: c.clone(), evidence, commitments }
}

/// EXPECTED (before running): a copyist that sees the original's reveal in the mempool cannot get its copy in first: a seal and the
/// commit it opens in the SAME block fail (age 0 < delay), so the copy commits at best one block after the original; a seal that
/// does not name the copy's claim id opens nothing; the original's seal is not transferable to another bond. VERDICT: PASS.
#[test]
fn a_copyist_that_seals_after_seeing_the_reveal_cannot_commit_before_the_original() {
    let mut n = Net::new();
    n.blk(3, vec![T::RegisterBond { bond: [0xC9; 64], collateral: 1000 }]);
    let job = n.post_job(4, &[3, 17, 9], 3, 1);
    let h = n.w.honest(&job, 3);
    let T::CommitClaim { claim, .. } = h.tx.clone() else { unreachable!() };
    let copy = copy_of(&claim, [0xC9; 64]);
    // The original seals at 10; the copyist sees nothing yet.
    n.blk_raw(10, vec![seal_obj(PRODUCER, &job, &claim)]);
    // Block 11: the reveal is in the mempool; the copyist's seal and copy go into the same block as the reveal.
    let ev = n.blk_raw(11, vec![seal_obj([0xC9; 64], &job, &copy), commit_of(&copy, &h.tx), h.tx.clone()]);
    let why: Vec<_> = ev.iter().filter_map(|e| if let E::Refused { why, .. } = e { Some(why.clone()) } else { None }).collect();
    assert_eq!(why.len(), 1, "only the copy is refused: {ev:?}");
    assert!(why[0].contains("no seal of this claim"), "{why:?}");
    assert!(ev.contains(&E::ClaimCommitted { claim: claim.id() }), "the original commits: {ev:?}");
    // A seal that names another claim id (a guess) opens nothing, and the original's seal cannot be used by another bond.
    n.blk_raw(12, vec![T::SealClaim { producer: [0xC9; 64], job: job.id(), seal: [0x11; 64] }]);
    let ev = n.blk_raw(13, vec![commit_of(&copy, &h.tx)]);
    assert!(refused(&ev).is_some_and(|w| w.contains("no seal of this claim")), "{ev:?}");
    // The copy sealed properly in 12..13 commits one block later: it coexists, uncovered, and holds nothing.
    n.blk_raw(14, vec![seal_obj([0xC9; 64], &job, &copy)]);
    let ev = n.blk_raw(15, vec![commit_of(&copy, &h.tx)]);
    assert!(ev.contains(&E::ClaimCommitted { claim: copy.id() }), "{ev:?}");
    assert!(n.w.l.job_claims.is_empty(), "neither claim is covered: nothing holds the job");
}

/// OBSERVATION R-C4-12 (descriptive, passes): the commit order is NOT a priority. Two correct claims of one job (an original and a
/// later copy that sealed after it) both commit; whichever the Panel covers FIRST holds the job and is paid; the other's coverage is
/// refused and it times out with its collateral returned. A copy committed later but covered first wins the reward — only the
/// Panel's (or a colluding Panel's) order decides. The ledger cannot prefer the earlier claim without re-opening the squat.
#[test]
fn observation_the_panel_s_coverage_order_not_the_commit_order_decides_the_holder() {
    let mut n = Net::new();
    n.blk(3, vec![T::RegisterBond { bond: [0xC9; 64], collateral: 1000 }]);
    let job = n.post_job(4, &[3, 17, 9], 3, 1);
    let h = n.w.honest(&job, 3);
    let T::CommitClaim { claim, .. } = h.tx.clone() else { unreachable!() };
    let copy = copy_of(&claim, [0xC9; 64]);
    n.blk_raw(10, vec![seal_obj(PRODUCER, &job, &claim)]);
    n.blk_raw(11, vec![h.tx.clone(), seal_obj([0xC9; 64], &job, &copy)]);
    n.blk_raw(12, vec![commit_of(&copy, &h.tx)]);
    // The Panel happens to cover the LATER copy first.
    n.blk_raw(13, vec![T::PanelCovered { claim: copy.id() }]);
    let ev = n.blk_raw(14, vec![T::PanelCovered { claim: claim.id() }]);
    assert!(refused(&ev).is_some_and(|w| w.contains("one claim per job")), "{ev:?}");
    n.blk_raw(80, vec![]);
    n.blk_raw(200, vec![]);
    assert!(
        n.w.l.claims[&copy.id()].rewarded && !n.w.l.claims[&claim.id()].rewarded,
        "R-C4-12: the later copy was paid, the original was not"
    );
    assert!(matches!(state_of(&n, &claim.id()), ClaimStateV1::TimedOut { .. } | ClaimStateV1::Checking { .. }));
    assert_eq!(n.w.l.bonds[&PRODUCER].reserved, 0, "the loser's collateral is returned at the timeout");
}

/// EXPECTED (before running): after the holder FAILS (convicted before Final), the job is free and a second live claim whose
/// coverage was refused earlier IS covered on retry (the ledger side of liveness; a consumer that never retries loses the claim at
/// its check window). VERDICT: PASS.
#[test]
fn a_refused_coverage_can_be_retried_after_the_holder_fails() {
    let mut n = Net::new();
    let job = n.post_job(2, &[3, 17, 9], 3, 1);
    let honest = n.w.honest(&job, 3);
    let hid = honest.claim.id();
    let (at, lie) = n.w.lying(&job, 3);
    let (lid, lda) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    let _ = at;
    // Both commit before either is covered; the colluding Panel covers the lie first.
    n.blk(8, vec![honest.tx.clone()]);
    n.blk(9, vec![lie.tx]);
    n.blk(10, vec![T::PanelCovered { claim: lid }]);
    let ev = n.blk(11, vec![T::PanelCovered { claim: hid }]);
    assert!(refused(&ev).is_some_and(|w| w.contains("one claim per job")), "the lying claim holds the job: {ev:?}");
    let proof = proof_of(outsider2(&n.w, lid, &lda));
    n.blk(13, vec![T::FileProof { accuser: OUTSIDER, claim: lid, proof }]);
    n.blk(14, vec![T::PanelCovered { claim: hid }]);
    assert_eq!(n.w.l.job_claims.get(&job.id()), Some(&hid));
    let ev = n.blk(70, vec![]);
    assert!(ev.contains(&E::Final { claim: hid, reward: 7 }), "{ev:?}");
}

/// EXPECTED (before running): seal lifecycle edges. A re-seal replaces the earlier seal and restarts its clock (the first claim can
/// no longer be revealed, the second cannot be revealed in the re-seal's own block); a seal expires after `seal_ttl_daa` (state
/// bounded: 3,000 junk seals vanish and the root returns to its pre-spam value apart from the clock); a seal for a job nobody
/// posted, one signed by another bond, and one by an exiting bond are refused; a seal made before `RequestExit` cannot be
/// revealed afterwards. VERDICT: PASS (with the lazy-expiry edge recorded below).
#[test]
fn seal_re_seal_expiry_exit_and_signer_edges() {
    let mut n = Net::new();
    let job = n.post_job(4, &[3, 17, 9], 3, 1);
    let h = n.w.honest(&job, 3);
    let T::CommitClaim { claim: c1, .. } = h.tx.clone() else { unreachable!() };
    // A second, different claim of the same job by the same producer (another output).
    let mut other = c1.clone();
    other.generated[2] = (other.generated[2] + 1) % n.w.program.token_bound;
    let h2 = n.w.produce(&job, PRODUCER, other.generated.clone(), &n.w.params.clone(), |_| {});
    let T::CommitClaim { claim: c2, .. } = h2.tx.clone() else { unreachable!() };
    // Re-seal: X at 10, Y at 12 replaces it.
    n.blk_raw(10, vec![seal_obj(PRODUCER, &job, &c1)]);
    n.blk_raw(12, vec![seal_obj(PRODUCER, &job, &c2)]);
    let ev = n.blk_raw(13, vec![h.tx.clone()]);
    assert!(refused(&ev).is_some_and(|w| w.contains("no seal of this claim")), "the replaced seal opens nothing: {ev:?}");
    n.blk_raw(20, vec![seal_obj(PRODUCER, &job, &c2)]);
    let ev = n.blk_raw(20, vec![h2.tx.clone()]);
    assert!(refused(&ev).is_some(), "a seal in the same block as the reveal proves nothing: {ev:?}");
    let ev = n.blk_raw(21, vec![h2.tx.clone()]);
    assert!(ev.contains(&E::ClaimCommitted { claim: c2.id() }), "{ev:?}");

    // Wrong signer, unknown job, exiting bond.
    let mut l = n.w.l.clone();
    l.begin_block(l.daa + 1).unwrap();
    let r = l.apply_encoded(&O::SealClaim { producer: SPAM1, job: job.id(), seal: [1; 64] }.encode(), &AuthV1 { signer_bond: SPAM2 });
    assert_eq!(r.unwrap_err().kind, RefusalKindV1::Unauthorized);
    let r =
        l.apply_encoded(&O::SealClaim { producer: SPAM1, job: [0x44; 64], seal: [1; 64] }.encode(), &AuthV1 { signer_bond: SPAM1 });
    assert!(r.unwrap_err().why.contains("no such job"));
    let mut n2 = Net::new();
    let job2 = n2.post_job(4, &[3, 17, 9], 3, 1);
    let hh = n2.w.honest(&job2, 3);
    let T::CommitClaim { claim: cc, .. } = hh.tx.clone() else { unreachable!() };
    n2.blk_raw(10, vec![seal_obj(PRODUCER, &job2, &cc)]);
    n2.blk_raw(11, vec![T::RequestExit { bond: PRODUCER }]);
    let ev = n2.blk_raw(12, vec![hh.tx.clone()]);
    assert!(
        refused(&ev).is_some_and(|w| w.contains("exiting")),
        "a seal made before the exit request cannot be revealed after it: {ev:?}"
    );
    let ev = n2.blk_raw(13, vec![seal_obj(PRODUCER, &job2, &cc)]);
    assert!(refused(&ev).is_some_and(|w| w.contains("exiting")), "an exiting bond seals nothing: {ev:?}");

    // Expiry and bounded state: 3,000 junk seals by 3,000 bonds holding one seal deposit each (a claim seal is bonded; a
    // zero-collateral bond seals nothing), gone after the ttl with their deposits forfeited.
    let mut n3 = Net::new();
    let job3 = n3.post_job(4, &[3, 17, 9], 3, 1);
    let before = n3.w.l.root_parts();
    let mut txs = Vec::new();
    for i in 0..3000u32 {
        let mut b = [0xF0; 64];
        b[..4].copy_from_slice(&i.to_le_bytes());
        txs.push(T::RegisterBond { bond: b, collateral: n3.w.l.policy.seal_deposit });
        txs.push(T::SealClaim { producer: b, job: job3.id(), seal: [0x5E; 64] });
    }
    n3.blk_raw(10, txs);
    assert_eq!(n3.w.l.seals.len(), 3000);
    n3.blk_raw(110, vec![]);
    assert_eq!(n3.w.l.seals.len(), 3000, "ttl 100: still held at 10 + 100");
    n3.blk_raw(111, vec![]);
    assert!(n3.w.l.seals.is_empty(), "swept after the ttl: junk seals occupy bounded state");
    assert_eq!(n3.w.l.root_parts().seals, before.seals, "the seal collection's root is back to empty");
    // Lazy expiry: objects are applied BEFORE the tick, so a seal past its ttl is still honoured in the first block that reaches it.
    let h3 = n3.w.honest(&job3, 3);
    let T::CommitClaim { claim: c3, .. } = h3.tx.clone() else { unreachable!() };
    n3.blk_raw(200, vec![seal_obj(PRODUCER, &job3, &c3)]);
    let ev = n3.blk_raw(301, vec![h3.tx]);
    eprintln!(
        "O-C4-13 lazy expiry: a seal of age 101 > ttl 100 revealed in the block that sweeps it: committed={}",
        ev.contains(&E::ClaimCommitted { claim: c3.id() })
    );
}

/// EXPECTED (before running): the cheap check should come first. An UNSEALED but structurally valid `CommitClaim` is refused at the
/// seal check, which runs after the structure check: it still spends one adjudication of the block budget. Recorded as
/// observation O-C4-14 (low: sealing is cheap for the attacker too, so reordering does not remove the budget attack, only the
/// unsealed variant of it).
#[test]
fn observation_an_unsealed_claim_is_refused_after_it_has_spent_an_adjudication() {
    let mut n = Net::new();
    let job = n.post_job(4, &[3, 17, 9], 3, 1);
    let h = n.w.honest(&job, 3);
    n.w.l.begin_block(10).unwrap();
    let T::CommitClaim { claim, evidence, commitments } = h.tx else { unreachable!() };
    let r = n.w.l.apply_object(&O::CommitClaim { claim, evidence, commitments }, &AuthV1 { signer_bond: PRODUCER });
    assert!(r.unwrap_err().why.contains("no seal"));
    eprintln!("O-C4-14: adjudications spent by the unsealed claim: {}", n.w.l.budget_used().adjudications);
}

// ── mapping hazards for the real node ──────────────────────────────────────────────────────────────────────────────────────

/// EXPECTED (before running): the ledger never instructs the consumer to slash more than the collateral the consumer has told it
/// the bond holds, even if the consumer re-synced the bond LOWER than the reservation (another subsystem slashed it meanwhile); the
/// unslashed rest of the reservation is released explicitly; the independent accountant accepts every instruction, routes the
/// slash 1:1 and agrees with the ledger's view of the bond afterwards. Two claims of one bond (the second's reservation then exceeds
/// the collateral) are tried as well. VERDICT: was FAIL F-C4-04 (fixed in acf26ede7); now PASS.
#[test]
fn a_conviction_never_instructs_more_than_the_synced_collateral() {
    let mut n = Net::new();
    let (id, _, da, _) = lying_covered(&mut n, 2, 10, 1);
    let proof = proof_of(outsider2(&n.w, id, &da));
    // Another subsystem slashed the bond: the consumer syncs the lower collateral (500 < the 1000 reserved).
    n.blk(11, vec![T::RegisterBond { bond: PRODUCER, collateral: 500 }]);
    let ev = {
        let b = block_of(12, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }], PRODUCER);
        let mut l = n.w.l.clone();
        fold_collecting(&mut l, &b)
    };
    let max_slash =
        ev.iter().filter_map(|e| if let E::Settlement(s) = e { (s.kind == K::SlashFraud).then_some(s.amount) } else { None }).max();
    assert_eq!(max_slash, Some(500), "SlashFraud clamps to the synced collateral");
    assert!(ev.iter().any(|e| matches!(e, E::Settlement(s) if s.kind == K::ReleaseClaim && s.amount == 500)), "the rest is released");
    // The same through the accountant (it refuses an instruction a real bond could not execute and checks conservation).
    let proof = proof_of(outsider2(&n.w, id, &da));
    let ev = n.blk(12, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted2(&ev), Some((500, 250, false)), "{ev:?}");
    assert_eq!(n.w.l.bonds[&PRODUCER].reserved, 0);
    assert_eq!(n.w.l.bonds[&PRODUCER].collateral, 0);
    // Two live claims of one bond; the bond is then cut below the first claim's reservation; both are convicted in turn.
    let mut n = Net::new();
    n.blk(3, vec![T::RegisterBond { bond: [0xB7; 64], collateral: 2000 }]);
    let job1 = n.post_job(4, &[3, 17, 9], 3, 1);
    let job2 = n.post_job(5, &[3, 17, 9], 3, 2);
    let mut ids = Vec::new();
    for (k, job) in [&job1, &job2].into_iter().enumerate() {
        let generated = n.w.greedy(&n.w.params, &job.prompt, 3);
        let p = n.w.produce(job, [0xB7; 64], generated, &n.w.params.clone(), |t| common::bump(&mut t.values[1][1][1], 0));
        let (cid, cda) = (p.claim.id(), Da::publishing(&p.trace, &[]));
        n.blk(10 + k as u64, vec![p.tx, T::PanelCovered { claim: cid }]);
        ids.push((cid, cda));
    }
    n.blk(12, vec![T::RegisterBond { bond: [0xB7; 64], collateral: 1000 }]);
    for (k, (cid, cda)) in ids.iter().enumerate() {
        let proof = proof_of(outsider2(&n.w, *cid, cda));
        n.blk(13 + k as u64, vec![T::FileProof { accuser: OUTSIDER, claim: *cid, proof }]);
    }
    assert_eq!(n.w.l.bonds[&[0xB7; 64]].reserved, 0);
}

/// Fold a block on a COPY of the ledger and return the raw receipts INCLUDING the settlement instructions.
fn fold_collecting(l: &mut KernelLedgerV1, b: &misaka_palw_kernel::ledger::LedgerBlockV1) -> Vec<E> {
    let mut out = Vec::new();
    l.begin_block(b.daa).unwrap();
    for tx in &b.txs {
        if let LedgerTxV1::Object { auth, object } = tx {
            out.extend(l.apply_encoded(&object.encode(), auth).unwrap_or_default());
        }
    }
    out
}

/// OBSERVATION O-C4-05 (recorded as a passing, descriptive test): the ledger's `RegisterClass` and `PostJob` take no fee and need
/// only a synced bond (even one holding zero collateral), so state growth is priced only by the consumer's carrier fee/mass. Many
/// registrations of the same program under different plan bounds are distinct classes.
#[test]
fn observation_class_and_job_registration_are_free_at_the_ledger_level() {
    let mut n = Net::new();
    let d = misaka_palw_kernel::descriptor::k2_tir_v1_descriptor();
    let mut accepted = 0;
    for max_positions in 8..40u32 {
        let plan =
            misaka_palw_kernel::plan::plan_for_tir_program_v1(&d, &n.w.program, common::root_of(&n.w.program), max_positions).unwrap();
        let t = T::RegisterClass { descriptor: d.digest(), program_bytes: n.w.program.encode(), plan, params: n.w.params.clone() };
        let ev = n.blk(2 + max_positions as u64, vec![T::RegisterBond { bond: [0xDD; 64], collateral: 0 }, t]);
        if ev.iter().any(|e| matches!(e, E::ClassRegistered { .. })) {
            accepted += 1;
        }
    }
    eprintln!("O-C4-05: {accepted} variant classes registered by a zero-collateral bond, {} classes in state", n.w.l.classes.len());
    assert!(accepted > 1);
}

// ── codec attacks ──────────────────────────────────────────────────────────────────────────────────────────────────────────

fn route_samples(n: &mut Net) -> Vec<(&'static str, O, Digest)> {
    let job = n.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = n.w.lying(&job, 3);
    let (id, da, trace) = (lie.claim.id(), Da::publishing(&lie.trace, &[]), lie.trace.clone());
    let h = n.w.honest(&n.w.l.jobs.values().next().cloned().unwrap(), 3);
    let _ = h;
    let T::CommitClaim { claim, evidence, commitments } = lie.tx.clone() else { unreachable!() };
    n.blk(10, vec![lie.tx.clone(), T::PanelCovered { claim: id }]);
    n.blk(11, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    let proof = proof_of(outsider2(&n.w, id, &da));
    let T::RegisterClass { descriptor, program_bytes, plan, params } = n.w.register() else { unreachable!() };
    vec![
        (
            "RegisterClass",
            O::RegisterClass {
                descriptor,
                program_bytes,
                plan,
                param_commitments: misaka_palw_kernel::trace::ParamCommitmentsV1::of(&params),
            },
            PRODUCER,
        ),
        ("PostJob", O::PostJob { job: n.w.l.jobs.values().next().cloned().unwrap() }, PRODUCER),
        ("CommitClaim", O::CommitClaim { claim, evidence, commitments }, PRODUCER),
        ("FileProof", O::FileProof { accuser: OUTSIDER, claim: id, proof }, OUTSIDER),
        ("FileDemand", O::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 2 }, SPAM1),
        ("Respond", O::Respond { claim: id, stage: 0, position: at.0, bytes: position(&trace, at.0, |_| {}) }, SPAM2),
        ("RequestExit", O::RequestExit { bond: SPAM1 }, SPAM1),
        ("Withdraw", O::Withdraw { bond: SPAM1 }, SPAM1),
        ("SealClaim", O::SealClaim { producer: SPAM2, job: n.w.l.jobs.keys().next().copied().unwrap(), seal: [0x5E; 64] }, SPAM2),
    ]
}

/// EXPECTED (before running): signer/actor mismatch — every object that names an actor, signed by ANY other bond (a registered
/// one or an unknown digest), is `Unauthorized` and changes nothing; an object that names none, signed by an unknown digest, is
/// `Unauthorized`; an exiting bond is refused state-adding objects but may still `Respond`. VERDICT: PASS.
#[test]
fn an_object_signed_by_anyone_but_the_actor_it_names_is_unauthorized_and_changes_nothing() {
    let mut n = Net::new();
    let samples = route_samples(&mut n);
    let others = [PRODUCER, OUTSIDER, SPAM1, SPAM2, [0x77; 64]];
    for (name, obj, signer) in &samples {
        let named = matches!(
            obj,
            O::CommitClaim { .. }
                | O::FileProof { .. }
                | O::FileDemand { .. }
                | O::RequestExit { .. }
                | O::Withdraw { .. }
                | O::SealClaim { .. }
        );
        for other in others.iter().filter(|o| *o != signer) {
            let mut l = n.w.l.clone();
            l.begin_block(l.daa + 1).unwrap();
            let before = l.root();
            let r = l.apply_encoded(&obj.encode(), &AuthV1 { signer_bond: *other });
            if named {
                let e = r.expect_err(name);
                assert_eq!(e.kind, RefusalKindV1::Unauthorized, "{name} signed by {:02x?}", &other[..2]);
                assert_eq!(l.root(), before);
            } else if *other == [0x77; 64] {
                let e = r.expect_err(name);
                assert_eq!(e.kind, RefusalKindV1::Unauthorized, "{name} signed by an unknown digest");
            }
        }
    }
}

/// EXPECTED (before running): non-canonical encodings of the SAME logical object — a reordered `BTreeMap` (param commitments), a
/// duplicated map key, an `Option` tag of 2, a trailing zero, a wrong version, every one-byte retag of every sample — are
/// refused by `decode` or, when they happen to decode, equal their own canonical encoding; none panics; none applies twice.
/// VERDICT: PASS.
#[test]
fn non_canonical_encodings_length_lies_and_retagged_objects_are_refused_or_canonical() {
    let mut n = Net::new();
    let samples = route_samples(&mut n);
    for (name, obj, signer) in &samples {
        let enc = obj.encode();
        assert_eq!(&O::decode(&enc).unwrap(), obj, "{name}");
        for tag in 0u8..=15 {
            let mut b = enc.clone();
            b[1] = tag;
            let mut l = n.w.l.clone();
            l.begin_block(l.daa + 1).unwrap();
            let before = l.root();
            let r = catch_unwind(AssertUnwindSafe(|| {
                match O::decode(&b) {
                    Ok(o) => assert_eq!(o.encode(), b, "{name} retagged {tag}: decode accepted a non-canonical form"),
                    Err(_) => {}
                }
                l.apply_encoded(&b, &AuthV1 { signer_bond: *signer })
            }));
            let r = r.unwrap_or_else(|_| panic!("{name} retagged to {tag} panicked"));
            if r.is_err() {
                assert_eq!(l.root(), before);
            }
        }
        let mut wrong_version = enc.clone();
        wrong_version[0] = 2;
        assert!(O::decode(&wrong_version).is_err());
    }
    // Reordered / duplicated BTreeMap entries inside RegisterClass.param_commitments (the last field of the object).
    let (_, reg, _) = &samples[0];
    let O::RegisterClass { param_commitments: pc, .. } = reg else { unreachable!() };
    assert!(pc.by_instance.len() >= 2);
    let canon = reg.encode();
    let pc_bytes = borsh::to_vec(pc).unwrap();
    assert!(canon.ends_with(&pc_bytes));
    let prefix = &canon[..canon.len() - pc_bytes.len()];
    let entries: Vec<Vec<u8>> = pc.by_instance.iter().map(|(k, v)| borsh::to_vec(&(k, v)).unwrap()).collect();
    let count = (entries.len() as u32).to_le_bytes();
    let build = |es: &[&Vec<u8>], count: [u8; 4]| {
        let mut v = prefix.to_vec();
        v.extend(count);
        for e in es {
            v.extend(*e);
        }
        v
    };
    let ordered: Vec<&Vec<u8>> = entries.iter().collect();
    assert_eq!(build(&ordered, count), canon, "the splice reproduces the canonical form");
    let mut reordered = ordered.clone();
    reordered.swap(0, 1);
    assert!(O::decode(&build(&reordered, count)).is_err(), "a reordered map is not canonical");
    let mut dup = ordered.clone();
    dup[1] = dup[0];
    assert!(O::decode(&build(&dup, count)).is_err(), "a duplicated key is not canonical");
    let mut bad_option = build(&ordered, count);
    // The first entry's key is (u16, Option<u16>): flip the Option tag (offset: prefix + 4 count + 2 index bytes) to 2.
    let at = prefix.len() + 4 + 2;
    bad_option[at] = 2;
    assert!(O::decode(&bad_option).is_err(), "an Option tag of 2");
}

// ── the strict decoder at its ceilings ─────────────────────────────────────────────────────────────────────────────────────

/// EXPECTED (before running): an object one byte past its variant's ceiling is `Oversized` before parsing; a length prefix claiming
/// 4 GiB inside a tiny object never allocates and is `Malformed`; both leave the state unchanged. VERDICT: PASS.
#[test]
fn the_decoder_bounds_every_object_before_parsing_and_never_trusts_a_length_prefix() {
    let mut n = Net::new();
    let _ = route_samples(&mut n);
    let mut l = n.w.l.clone();
    l.begin_block(l.daa + 1).unwrap();
    let before = l.root();
    for tag in 1u8..=12 {
        let limit = misaka_palw_kernel::route::max_encoded_bytes_of_tag(tag).unwrap();
        if limit > (1 << 24) {
            continue; // ceilings of tens of MiB are exercised by the size check, not by allocating them here
        }
        let mut huge = vec![1u8, tag];
        huge.resize(limit + 1, 0);
        let e = l.apply_encoded(&huge, &AuthV1 { signer_bond: PRODUCER }).unwrap_err();
        assert_eq!(e.kind, RefusalKindV1::Oversized, "tag {tag}");
    }
    for tag in [1u8, 3, 5, 7, 9] {
        let mut lie = vec![1u8, tag];
        lie.extend([0xFF; 200]);
        let e = l.apply_encoded(&lie, &AuthV1 { signer_bond: PRODUCER }).unwrap_err();
        assert!(matches!(e.kind, RefusalKindV1::Malformed | RefusalKindV1::Unauthorized), "tag {tag}: {e}");
    }
    assert_eq!(l.root(), before);
}
