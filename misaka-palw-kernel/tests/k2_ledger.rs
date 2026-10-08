//! **Public prosecution on an in-process chain** (ADR-0173; RFC-0015 §1.1 G14): the producer and every Panel seat colluding, one
//! ordinary bond outside the Panel, and nothing but ledger state and public DA bytes.
//!
//! The outsider in every test is built by replaying the block sequence from genesis (a fresh node's IBD) and reads committed
//! values only from a [`Da`] store of bytes the producer published (minus what it withholds) or from values served on chain in
//! answer to demands. It never sees the producer's trace, heap or private state, a Panel seat's capture, a court's internals or
//! a served view: the producer's objects are dropped before the outsider is built.

mod common;

use common::chain::T;
use common::ledger_world::*;
use common::{MAX_POSITIONS, active_for, bump};
use misaka_palw_kernel::descriptor::k2_tir_v1_descriptor;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::{DecodeRuleV1, KernelClaimV1, KernelJobV1};
use misaka_palw_kernel::ledger::{KernelLedgerV1, LedgerBlockV1, LedgerEventV1 as E, OutsiderFindingV1, ProsecutionV1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::merkle::TensorOpeningV1;
use misaka_palw_kernel::public::{MaterialResponseV1, TensorWireV1};
use misaka_palw_tir::Prim;

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
    assert_eq!(w.consumer.paid(&OUTSIDER), 500);
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

    // Past the liability horizon the reservation is released (finite retained exposure) and a proof is REFUSED: no court runs and
    // no fee is charged (a true proof never costs its filer the dismissal fee, C4 F-C4R3-02).
    let job = w.post_job(310, &[3, 17, 9], 3, 3);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(320, vec![lie.tx, T::PanelCovered { claim: id }]);
    w.block(370, vec![]);
    let ev = w.block(571, vec![]);
    assert_eq!(ev, vec![E::Released { claim: id }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let outsider_collateral = w.l.bonds[&OUTSIDER].collateral;
    let ev = w.block(572, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(refused(&ev).as_deref(), Some("past the liability horizon"), "{ev:?}");
    assert_eq!(w.l.bonds[&OUTSIDER].collateral, outsider_collateral, "no fee for a proof the horizon refuses");
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
    // Evidence a court could only call malformed is never committed: a fabricated segment boundary, misshapen commitments.
    let mut fabricated = evidence.clone();
    fabricated.segments[0].exit_state_root = [1; 64];
    let mut short = commitments.clone();
    short[0][0].pop();
    let mut short_ev = evidence.clone();
    short_ev.trace_root = misaka_palw_kernel::trace::EvidenceV1::new(short.clone()).root();
    // One block each: a producer's seal for a job is one seal (a re-seal replaces it), so each reveal is sealed on its own.
    let mut ev = w.block(
        7,
        vec![T::CommitClaim {
            claim: KernelClaimV1 { evidence_root: fabricated.root(), ..h1.claim.clone() },
            evidence: fabricated,
            commitments: commitments.clone(),
        }],
    );
    ev.extend(w.block(
        8,
        vec![T::CommitClaim {
            claim: KernelClaimV1 { evidence_root: short_ev.root(), ..h1.claim.clone() },
            evidence: short_ev,
            commitments: short,
        }],
    ));
    let why: Vec<_> = ev.iter().filter_map(|e| if let E::Refused { why, .. } = e { Some(why.clone()) } else { None }).collect();
    assert_eq!(why.len(), 2, "{ev:?}");
    assert!(why.iter().all(|w| w.starts_with("malformed evidence")), "{why:?}");
    assert!(w.l.claims.is_empty(), "nothing was committed, nothing reserved");
    // (only the two refused claims' seal deposits, held until their seals are revealed or expire: bonded seals)
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 2);

    // The last delivered token substituted (it is never fed back, so the trace is the honest one): the decode court convicts.
    let mut last = h1.claim.generated.clone();
    last[2] = (last[2] + 1) % w.program.token_bound;
    let sub = w.produce(&job1, PRODUCER, last, &w.params.clone(), |_| {});
    let (id, da) = (sub.claim.id(), Da::publishing(&sub.trace, &[]));
    w.block(9, vec![sub.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Decode(f)) = outsider(&w, id, &da) else { panic!() };
    assert_eq!(f.index, 2);
    let ev = w.block(10, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: ProsecutionV1::Decode(f) }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)));

    // A token substituted mid-generation, the trace recomputed over the substituted stream (self-consistent): the logits of
    // the position before it select another token.
    let mut mid = w.greedy(&w.params, &job2.prompt, 3);
    mid[0] = (mid[0] + 1) % w.program.token_bound;
    let sub = w.produce(&job2, PRODUCER, mid, &w.params.clone(), |_| {});
    let (id, da) = (sub.claim.id(), Da::publishing(&sub.trace, &[]));
    w.block(11, vec![sub.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Decode(f)) = outsider(&w, id, &da) else { panic!() };
    assert_eq!(f.index, 0);
    let ev = w.block(12, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: ProsecutionV1::Decode(f) }]);
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
    assert_eq!(outsider(&w, id, &da), OutsiderFindingV1::Demand(vec![(0, 1), (0, 3)]), "no pass, no conviction: one round of demands");

    let ev = w.block(
        11,
        vec![
            T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 1 },
            T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 3 },
        ],
    );
    assert_eq!(
        ev,
        vec![
            E::DemandOpened { claim: id, stage: 0, position: 1, deadline: 31 },
            E::DemandOpened { claim: id, stage: 0, position: 3, deadline: 31 }
        ]
    );
    assert!(matches!(w.state(&id), ClaimStateV1::Disputed { open: 2, .. }), "open demands block Final");
    // An authentic row where the whole value is owed does not serve the position: the demand stays open.
    let committed = trace.values[at.0 as usize][at.1 as usize][at.2 as usize].clone();
    let row = position(&trace, 1, |r| {
        r[at.1 as usize][at.2 as usize] = MaterialResponseV1::Part(TensorOpeningV1::row(&committed, 0).unwrap())
    });
    let ev = w.block(12, vec![T::Respond { claim: id, stage: 0, position: 1, bytes: row }]);
    assert_eq!(ev, vec![E::ResponseRejected { claim: id, stage: 0, position: 1, class: "partial" }]);
    // The committed values, served on chain: public from then on, and they convict (an authentic opening is not an acquittal).
    let ev = w.block(
        13,
        vec![
            T::Respond { claim: id, stage: 0, position: 1, bytes: position(&trace, 1, |_| {}) },
            T::Respond { claim: id, stage: 0, position: 3, bytes: position(&trace, 3, |_| {}) },
        ],
    );
    assert_eq!(ev, vec![E::Served { claim: id, stage: 0, position: 1 }, E::Served { claim: id, stage: 0, position: 3 }]);
    assert_eq!(w.l.bonds[&OUTSIDER].reserved, 20, "the served positions' demand bonds stay reserved until the claim's fate");
    let ev = w.block(14, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 1 }]);
    assert_eq!(refused(&ev).as_deref(), Some("already served: it is public"));
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("the served values complete the check") };
    let ev = w.block(15, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)));
    assert_eq!(
        (w.l.bonds[&OUTSIDER].reserved, w.l.bonds[&OUTSIDER].collateral),
        (0, 1000),
        "a true demand: refunded at the conviction"
    );

    // The same lie, and the producer stays silent: an availability default, never the fraud slash.
    let job = w.post_job(20, &[3, 17, 9], 3, 2);
    let (at, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]));
    w.block(21, vec![lie.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Demand(positions) = outsider(&w, id, &da) else { panic!() };
    assert_eq!(positions, vec![(0, at.0)]);
    w.block(22, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    let collateral = w.l.bonds[&PRODUCER].collateral;
    let ev = w.block(42, vec![]);
    assert_eq!(ev, vec![E::ProducerDefault { claim: id, stage: 0, position: at.0, last: None, penalty: 100 }]);
    assert_eq!(w.state(&id), ClaimStateV1::Unavailable { daa: 42, producer_defaulted: true });
    assert_eq!(w.l.bonds[&PRODUCER].collateral, collateral - 100);
    // The rest of the reservation is held through the default's liability horizon (C4 F-C4R3-02), never slashed for withholding.
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 900, "held, not slashed");
    assert_eq!(w.l.claims[&id].liability_until, Some(42 + 200));
    assert_eq!(w.block(243, vec![]), vec![E::Released { claim: id }], "released at the horizon: no valid proof ever arrived");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0);
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
    let why = refused(&w.events).unwrap();
    assert!(why.starts_with("not publicly prosecutable") && why.contains("public bytes"), "{why}");
    // Fewer demand sessions than the plan has positions: some position could not be demanded, so the class is refused.
    let mut p = policy();
    p.prosecution.max_sessions_per_claim = MAX_POSITIONS - 1;
    let w = World::with(p);
    assert!(w.l.classes.is_empty());

    // A kernel this binary does not implement, and a plan for another program, are refused too.
    let mut w = World::new();
    let T::RegisterClass { program_bytes, plan, params, .. } = w.register() else { unreachable!() };
    let unknown =
        T::RegisterClass { descriptor: [0xEE; 64], program_bytes: program_bytes.clone(), plan: plan.clone(), params: params.clone() };
    let mut other = plan.clone();
    other.program_root = [1; 64];
    let mismatched = T::RegisterClass { descriptor: k2_tir_v1_descriptor().digest(), program_bytes, plan: other, params };
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
            T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 },
            T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 1 },
            T::FileDemand { demander: SPAM2, claim: id, stage: 0, position: 2 },
            T::FileDemand { demander: SPAM2, claim: id, stage: 0, position: 3 },
            T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 4 },
            T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 0 },
        ],
    );
    assert!(refused(&ev).is_none(), "{ev:?}");
    assert_eq!(ev.iter().filter(|e| matches!(e, E::DemandOpened { .. })).count(), 5);
    assert!(ev.contains(&E::DemandJoined { claim: id, stage: 0, position: 0 }));
    let ev = w.block(12, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 5 }]);
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
            T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 },
            T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 1 },
            T::FileDemand { demander: SPAM2, claim: id, stage: 0, position: 2 },
        ],
    );
    assert_eq!(ev.iter().filter(|e| matches!(e, E::DemandOpened { .. })).count(), 3);
    let ev = w.block(60, vec![T::FileDemand { demander: SPAM2, claim: id, stage: 0, position: 3 }]);
    assert_eq!(refused(&ev).as_deref(), Some("the challenge window is closed"), "the window closes on time");
    // The honest producer answers each demand at its last moment.
    let serve = |p: u32| T::Respond { claim: id, stage: 0, position: p, bytes: position(&trace, p, |_| {}) };
    w.block(78, vec![serve(0), serve(1)]);
    assert!(matches!(w.state(&id), ClaimStateV1::Disputed { .. }));
    // The last service hands the claim no Final in its own block: the demander gets `proof_grace_daa` to use what it was served.
    let ev = w.block(78, vec![serve(2)]);
    assert!(!ev.iter().any(|e| matches!(e, E::Final { .. })), "{ev:?}");
    assert!(matches!(w.state(&id), ClaimStateV1::WindowClosed { .. }), "{:?}", w.state(&id));
    assert!(w.block(87, vec![]).is_empty());
    let ev = w.block(88, vec![]);
    assert_eq!(ev, vec![E::Final { claim: id, reward: 7 }]);
    let ClaimStateV1::Final { final_daa } = w.state(&id) else { panic!() };
    assert!(final_daa <= 60 + 20 + 10, "window end + court deadline + proof grace");

    // Demands filed while the claim is still checking hold it too, and the Panel's coverage meanwhile starts the window.
    let job = w.post_job(100, &[3, 17, 9], 3, 2);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    let trace = h.trace.clone();
    w.block(101, vec![h.tx]);
    w.block(102, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 }]);
    w.block(103, vec![T::PanelCovered { claim: id }]); // window end 153
    w.block(110, vec![T::Respond { claim: id, stage: 0, position: 0, bytes: position(&trace, 0, |_| {}) }]);
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

    // A demand in the window's last block, served at its deadline: Final waits out the proof grace, so the proof the served values
    // enable convicts BEFORE Final and the producer never earns the reward.
    let job = w.post_job(300, &[3, 17, 9], 3, 4);
    let (at, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]));
    let trace = lie.trace.clone();
    w.block(301, vec![lie.tx, T::PanelCovered { claim: id }]); // window end 351
    w.block(350, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    let ev = w.block(369, vec![T::Respond { claim: id, stage: 0, position: at.0, bytes: position(&trace, at.0, |_| {}) }]);
    assert!(
        !ev.iter().any(|e| matches!(e, E::Final { .. })),
        "the claim must not finalize in the block that served the demand: {ev:?}"
    );
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(370, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "a pre-Final conviction: {ev:?}");
    w.block(500, vec![]);
    assert!(!w.l.claims[&id].rewarded && matches!(w.state(&id), ClaimStateV1::Convicted { .. }));

    // The same, but nobody files inside the grace: Final (and the reward) follow at `served + grace`, and a proof after the grace
    // but inside the liability horizon still convicts, post-Final.
    let job = w.post_job(600, &[3, 17, 9], 3, 5);
    let (at, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]));
    let trace = lie.trace.clone();
    w.block(601, vec![lie.tx, T::PanelCovered { claim: id }]); // window end 651
    w.block(650, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    w.block(669, vec![T::Respond { claim: id, stage: 0, position: at.0, bytes: position(&trace, at.0, |_| {}) }]);
    assert!(w.block(678, vec![]).is_empty());
    assert_eq!(w.block(679, vec![]), vec![E::Final { claim: id, reward: 7 }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(680, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, true)), "post-Final liability: {ev:?}");
}

#[test]
fn spam_demands_and_joins_cannot_push_final_past_window_end_plus_court_deadline_plus_proof_grace() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let (id, trace) = (h.claim.id(), h.trace.clone());
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]); // window end 60
    // Every position demanded in the window's last block by two bonds (the second one joins), then joins later in the window of
    // service: none of it restarts a clock.
    let demands: Vec<T> = (0..5)
        .flat_map(|p| {
            [
                T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: p },
                T::FileDemand { demander: SPAM2, claim: id, stage: 0, position: p },
            ]
        })
        .collect();
    let ev = w.block(59, demands);
    assert_eq!(ev.iter().filter(|e| matches!(e, E::DemandOpened { deadline: 79, .. })).count(), 5);
    assert_eq!(ev.iter().filter(|e| matches!(e, E::DemandJoined { .. })).count(), 5);
    let ev = w.block(70, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 0 }]);
    assert_eq!(refused(&ev).as_deref(), Some("the challenge window is closed"), "no demand opens or joins past the window");
    // The producer serves every demand at its deadline; a position is served once and cannot be demanded (or served) again.
    let serve: Vec<T> = (0..5).map(|p| T::Respond { claim: id, stage: 0, position: p, bytes: position(&trace, p, |_| {}) }).collect();
    let ev = w.block(79, serve);
    assert_eq!(ev.iter().filter(|e| matches!(e, E::Served { .. })).count(), 5, "{ev:?}");
    let ev = w.block(80, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 0 }]);
    assert!(refused(&ev).is_some(), "a served position cannot be demanded again to restart the grace: {ev:?}");
    assert!(!w.l.demands.contains_key(&(id, 0, 0)));
    assert!(w.block(88, vec![]).is_empty(), "served at 79: Final waits until 89");
    let ev = w.block(89, vec![]);
    assert_eq!(ev, vec![E::Final { claim: id, reward: 7 }]);
    let ClaimStateV1::Final { final_daa } = w.state(&id) else { panic!() };
    assert_eq!(final_daa, 89);
    assert!(final_daa <= 60 + 20 + 10, "the absolute bound: window end + court deadline + proof grace");
}

#[test]
fn a_post_final_demand_must_fit_its_whole_path_in_the_liability_horizon() {
    // A demand whose deadline + grace would pass the horizon is refused: what it served nobody could still prosecute with.
    let mut w = World::new();
    let (id, at, _, _) = final_lying_claim(&mut w);
    let ev = w.block(231, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    assert_eq!(refused(&ev).as_deref(), Some("the challenge window is closed"), "231 + 20 + 10 > 260: {ev:?}");

    // The last demand that fits (230 + 20 + 10 = 260): served at its deadline, the proof it enables is still filed inside the horizon.
    let mut w = World::new();
    let (id, at, da, trace) = final_lying_claim(&mut w);
    let ev = w.block(230, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    assert_eq!(ev, vec![E::DemandOpened { claim: id, stage: 0, position: at.0, deadline: 250 }]);
    w.block(250, vec![T::Respond { claim: id, stage: 0, position: at.0, bytes: position(&trace, at.0, |_| {}) }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(260, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, true)), "{ev:?}");
}

#[test]
fn a_post_final_default_forfeits_the_whole_reservation_and_is_never_a_conviction() {
    let mut w = World::new();
    let (id, at, da, _) = final_lying_claim(&mut w);
    assert_eq!(w.consumer.paid(&PRODUCER), 7, "the reward was paid at Final");
    let OutsiderFindingV1::Demand(p) = outsider(&w, id, &da) else { panic!() };
    assert_eq!(p, vec![(0, at.0)]);
    w.block(100, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    let collateral = w.l.bonds[&PRODUCER].collateral;
    let burned = w.l.burned;
    let ev = w.block(120, vec![]);
    assert_eq!(ev, vec![E::PostFinalDefault { claim: id, stage: 0, position: at.0, last: None, forfeited: 1000 }]);
    assert!(convicted(&ev).is_none() && !w.l.claims[&id].convicted, "an availability outcome, not fraud");
    assert!(matches!(w.state(&id), ClaimStateV1::Final { .. }), "the claim stays Final");
    assert_eq!(w.l.bonds[&PRODUCER].collateral, collateral - 1000, "the whole reservation, not default_penalty");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0);
    // Burned whole: a post-Final demander may be the producer's own Sybil (C4 F-C4-02), so nobody is paid from the forfeit.
    assert_eq!(w.consumer.paid(&OUTSIDER), 0, "no demander share after Final");
    assert_eq!(w.l.burned, burned + 1000);
    assert_eq!(w.l.bonds[&OUTSIDER].reserved, 0, "the demand bond returns");

    // Before Final the same default costs only `default_penalty` and the claim earns no reward.
    let job = w.post_job(200, &[3, 17, 9], 3, 2);
    let (at, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]));
    w.block(201, vec![lie.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Demand(_) = outsider(&w, id, &da) else { panic!() };
    w.block(202, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    let ev = w.block(222, vec![]);
    assert_eq!(ev, vec![E::ProducerDefault { claim: id, stage: 0, position: at.0, last: None, penalty: 100 }]);
    assert!(!w.l.claims[&id].rewarded);
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
            T::Respond { claim: id, stage: 0, position: 1, bytes: position(&trace, 1, |_| {}) },
            T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 99 },
        ],
    );
    assert_eq!(
        ev.iter().filter_map(|e| if let E::Refused { why, .. } = e { Some(why.as_str()) } else { None }).collect::<Vec<_>>(),
        ["no open demand for this position", "the claim commits no such position"]
    );
    w.block(12, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 1 }]);
    let mut fake = TensorOpeningV1::row(&trace.values[1][0][0], 0).unwrap();
    fake.siblings.extend([[0; 64]; 3]);
    let ev = w.block(
        13,
        vec![
            T::Respond { claim: id, stage: 0, position: 1, bytes: vec![0xFF, 0x00] },
            T::Respond {
                claim: id,
                stage: 0,
                position: 1,
                bytes: position(&trace, 1, |r| {
                    r.pop();
                }),
            },
            T::Respond {
                claim: id,
                stage: 0,
                position: 1,
                bytes: position(&trace, 1, |r| r[0][0] = MaterialResponseV1::Whole(TensorWireV1::of(&other))),
            },
            T::Respond {
                claim: id,
                stage: 0,
                position: 1,
                bytes: position(&trace, 1, |r| r[0][0] = MaterialResponseV1::Part(TensorOpeningV1::row(&other, 0).unwrap())),
            },
            T::Respond { claim: id, stage: 0, position: 1, bytes: position(&trace, 1, |r| r[0][0] = MaterialResponseV1::Part(fake)) },
        ],
    );
    let classes: Vec<_> =
        ev.iter().filter_map(|e| if let E::ResponseRejected { class, .. } = e { Some(*class) } else { None }).collect();
    assert_eq!(classes, ["malformed", "malformed", "wrong_bytes", "wrong_root", "fake_opening"]);

    // The deadline passes on a non-serving response: the producer's default, a fixed penalty to the demander.
    let (collateral, credits) = (w.l.bonds[&PRODUCER].collateral, w.consumer.paid(&OUTSIDER));
    let ev = w.block(32, vec![]);
    assert_eq!(ev, vec![E::ProducerDefault { claim: id, stage: 0, position: 1, last: Some("fake_opening"), penalty: 100 }]);
    assert!(convicted(&ev).is_none());
    assert_eq!(w.l.bonds[&PRODUCER].collateral, collateral - 100, "a penalty, not the 1000 fraud slash");
    // Split like a slash (C4 F-C4R3-02): the demander's share is the accuser's 500 permille, the rest is burned.
    assert_eq!(w.consumer.paid(&OUTSIDER), credits + 50);
    assert_eq!(w.l.bonds[&OUTSIDER].reserved, 0);
    assert_eq!(w.state(&id), ClaimStateV1::Unavailable { daa: 32, producer_defaulted: true });
    let ev = w.block(33, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 1 }]);
    assert_eq!(refused(&ev).as_deref(), Some("the claim is already decided"));
}

// ── F-C4R3-02: a self-inflicted default never erases a provable fraud ─────────────────────────────────────────────────────

/// **C4 round 3, F-C4R3-02 at reference level.** The producer publishes EVERYTHING (an outsider can convict from public material
/// alone), its own bond (`SPAM1`) demands a position at once, and the producer stays silent while the outsider's proof is kept out
/// of the chain until the demand's deadline. Before the fix the default released the reservation and the true proof was dismissed
/// with the filing fee. Now: the default is split like a slash (the colluding demander recoups only the accuser's share of the
/// penalty), the rest of the reservation stays held through the default's liability horizon, and the outsider's proof convicts —
/// paying it the bounty it would have had with no default, and charging it nothing.
#[test]
fn a_self_inflicted_default_never_erases_a_provable_fraud_and_a_true_proof_never_pays_a_fee() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("everything is published") };
    let (producer0, sybil0) = (w.l.bonds[&PRODUCER].collateral, w.l.bonds[&SPAM1].collateral);
    w.block(11, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 }]);
    let ev = w.block(31, vec![]);
    assert_eq!(ev, vec![E::ProducerDefault { claim: id, stage: 0, position: 0, last: None, penalty: 100 }]);
    assert_eq!(w.consumer.paid(&SPAM1), 50, "the colluding demander recoups only the accuser's share of the penalty");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 900, "the rest of the reservation is held through the default's horizon");
    assert_eq!(w.l.claims[&id].liability_until, Some(231));
    // The outsider's proof, kept out until after the default, still convicts inside the default's horizon.
    let outsider0 = w.l.bonds[&OUTSIDER].collateral;
    let ev = w.block(32, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: proof.clone() }]);
    assert_eq!(convicted(&ev), Some((900, 500, false)), "{ev:?}");
    assert!(matches!(w.state(&id), ClaimStateV1::Convicted { .. }), "a fraud, not a default: {:?}", w.state(&id));
    assert_eq!(w.l.bonds[&OUTSIDER].collateral, outsider0, "a true proof is never charged the dismissal fee");
    assert_eq!(w.consumer.paid(&OUTSIDER), 500, "the bounty the outsider would have had with no default");
    assert_eq!(w.l.bonds[&PRODUCER].collateral, producer0 - 1000, "the producer loses the whole reservation");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0);
    let colluders = (w.l.bonds[&PRODUCER].collateral + w.l.bonds[&SPAM1].collateral + w.consumer.paid(&SPAM1)) as i128
        - (producer0 + sybil0) as i128;
    assert_eq!(colluders, -950, "the escape saves the colluders at most their demanders' share of the penalty");
    let ev = w.block(33, vec![T::FileProof { accuser: SPAM2, claim: id, proof }]);
    assert_eq!(ev, vec![E::Duplicate { claim: id }]);

    // A FALSE proof against a defaulted claim inside its horizon is still dismissed with the fee; with no valid proof the outcome
    // stays the default (availability, never fraud) and the reservation is released at the horizon; a proof past it is refused free.
    let job = w.post_job(40, &[3, 17, 9], 3, 2);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(41, vec![lie.tx, T::PanelCovered { claim: id }]);
    w.block(42, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 }]);
    w.block(62, vec![]);
    let junk = w.l.bonds[&SPAM2].collateral;
    let ev = w.block(63, vec![T::FileProof { accuser: SPAM2, claim: id, proof: ProsecutionV1::Kernel(vec![1, 2, 3]) }]);
    assert!(matches!(&ev[..], [E::ProofDismissed { fee: 5, .. }, ..]), "{ev:?}");
    assert_eq!(w.l.bonds[&SPAM2].collateral, junk - 5, "a false proof pays its fee whatever the claim's state");
    assert_eq!(w.block(263, vec![]), vec![E::Released { claim: id }]);
    assert!(!w.l.claims[&id].convicted && matches!(w.state(&id), ClaimStateV1::Unavailable { .. }), "withholding stays a default");
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let outsider0 = w.l.bonds[&OUTSIDER].collateral;
    let ev = w.block(264, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(refused(&ev).as_deref(), Some("past the liability horizon"));
    assert_eq!(w.l.bonds[&OUTSIDER].collateral, outsider0);

    // A claim that never passed (the Panel never covered it) times out with its reservation returned; a proof against it is refused
    // free — it never paid anything and holds nothing.
    let job = w.post_job(300, &[3, 17, 9], 3, 3);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(301, vec![lie.tx]);
    let ev = w.block(402, vec![]);
    assert!(ev.contains(&E::TimedOut { claim: id }), "{ev:?}");
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(403, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(refused(&ev).as_deref(), Some("the claim ended without passing and holds nothing"));
    assert_eq!(w.l.bonds[&OUTSIDER].collateral, outsider0, "no fee");
}

// ── GAP-R7: accuser seals ──────────────────────────────────────────────────────────────────────────────────────────────────

fn seal(accuser: Digest, claim: Digest, proof: &ProsecutionV1) -> T {
    T::SealProof { accuser, claim, seal: misaka_palw_kernel::ledger::proof_seal_v1(&claim, &accuser, proof) }
}

/// **GAP-R7 at reference level.** A proof names no accuser in its bytes, so a copyist who lifts it from the honest filer's public
/// carrier and gets it included first used to take the whole bounty. Now the bounty belongs to the EARLIEST seal of the convicting
/// bytes that is at least `claim_seal_delay_daa` old, whoever files them: the lifted copy convicts and pays the sealer. A seal in the
/// same block as the filing proves nothing (the filer is paid); a seal of OTHER bytes takes nothing; an unsealed filing is the filer's.
/// A colluding producer that convicts itself with its own proof recoups the accuser's share — and still loses more than the Final
/// reward (the policy's relation).
#[test]
fn a_lifted_proof_pays_its_earliest_sealer_and_self_conviction_still_costs_more_than_the_reward() {
    let mut w = World::new();
    let post = |w: &mut World, daa: u64, nonce: u8| {
        let job = w.post_job(daa, &[3, 17, 9], 3, nonce);
        let (_, lie) = w.lying(&job, 3);
        let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
        w.block(daa + 1, vec![lie.tx, T::PanelCovered { claim: id }]);
        let OutsiderFindingV1::Prosecute(proof) = outsider(w, id, &da) else { panic!() };
        (id, proof)
    };

    // The outsider seals, then files; a copyist (SPAM1) lifts the filed bytes and its copy is included FIRST.
    let (id, proof) = post(&mut w, 2, 1);
    let ev = w.block(4, vec![seal(OUTSIDER, id, &proof)]);
    assert_eq!(ev, vec![E::ProofSealed { claim: id, accuser: OUTSIDER }]);
    let ev = w.block(5, vec![T::FileProof { accuser: SPAM1, claim: id, proof: proof.clone() }]);
    assert!(
        ev.contains(&E::Convicted { claim: id, accuser: OUTSIDER, slashed: 1000, accuser_reward: 500, post_final: false }),
        "{ev:?}"
    );
    assert_eq!((w.consumer.paid(&OUTSIDER), w.consumer.paid(&SPAM1)), (500, 0), "the sealer is paid, never the copyist");
    assert!(w.l.proof_seals.is_empty(), "a conviction spends the claim's seals");
    assert_eq!(w.block(6, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]), vec![E::Duplicate { claim: id }]);

    // A seal in the same block as the filing proves nothing: the filer (here the copyist) is paid.
    let (id, proof) = post(&mut w, 10, 2);
    let paid = w.consumer.paid(&SPAM1);
    w.block(12, vec![seal(OUTSIDER, id, &proof), T::FileProof { accuser: SPAM1, claim: id, proof }]);
    assert_eq!(w.consumer.paid(&SPAM1), paid + 500, "a same-block seal is younger than the delay");

    // A seal of OTHER bytes takes nothing; the earliest of two seals of the convicting bytes wins, whoever files.
    let (id, proof) = post(&mut w, 20, 3);
    let junk = ProsecutionV1::Kernel(vec![1, 2, 3]);
    w.block(22, vec![seal(SPAM2, id, &junk)]);
    w.block(23, vec![seal(OUTSIDER, id, &proof)]);
    w.block(24, vec![seal(SPAM1, id, &proof)]);
    let (o, s1, s2) = (w.consumer.paid(&OUTSIDER), w.consumer.paid(&SPAM1), w.consumer.paid(&SPAM2));
    let ev = w.block(26, vec![T::FileProof { accuser: SPAM1, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)));
    assert_eq!((w.consumer.paid(&OUTSIDER) - o, w.consumer.paid(&SPAM1) - s1, w.consumer.paid(&SPAM2) - s2), (500, 0, 0));

    // Seals are refused where a proof would be: no such claim, a convicted claim; and an unrevealed seal expires.
    let ev = w.block(27, vec![seal(OUTSIDER, [0xEE; 64], &junk), seal(OUTSIDER, id, &junk)]);
    let why: Vec<&str> = ev.iter().filter_map(|e| if let E::Refused { why, .. } = e { Some(why.as_str()) } else { None }).collect();
    assert_eq!(why, ["no such claim", "the claim is already convicted"]);
    let (id, proof) = post(&mut w, 30, 4);
    w.block(32, vec![seal(SPAM2, id, &proof)]);
    assert_eq!(w.l.proof_seals.len(), 1);
    w.block(32 + 100 + 1, vec![]);
    assert!(w.l.proof_seals.is_empty(), "an unrevealed seal lives seal_ttl_daa");
    let _ = proof;

    // Self-recoup: the producer convicts itself with its own proof and is paid its own bounty — it still loses half the reservation,
    // more than the Final reward it lied for (LedgerPolicyV1::validate's relation).
    w.block(139, vec![T::RegisterBond { bond: PRODUCER, collateral: 10_000 }]); // room for its own filing fee beside two claims
    let (id, proof) = post(&mut w, 140, 5);
    let before = w.l.bonds[&PRODUCER].collateral as i128 + w.consumer.paid(&PRODUCER) as i128;
    w.block(142, vec![T::FileProof { accuser: PRODUCER, claim: id, proof }]);
    let net = w.l.bonds[&PRODUCER].collateral as i128 + w.consumer.paid(&PRODUCER) as i128 - before;
    assert_eq!(net, -500);
    assert!(-net > w.l.policy.claim_reward as i128);
    let mut p = policy();
    p.claim_reward = 500;
    assert!(p.validate().is_err(), "a reward a self-convicted producer could still profit from is refused");
}

// ── K2S's producer-side DA griefing: the fate of a served position's demand bond ───────────────────────────────────────────

/// **The demand bond of a SERVED position** (K2S's producer-side DA griefing; G14-R4): a demander can force an honest producer to
/// serve committed values on chain, so the bond of a position that was served stays reserved and is BURNED only when the claim's
/// liability horizon ends with no conviction. It is refunded the moment the claim is convicted (whenever the proof lands — inside the
/// grace or long after it, post-Final), defaults (another position withheld) or times out: a true demand that leads to a conviction is
/// never penalised, and G14 does not rest on the burn.
#[test]
fn a_served_demand_bond_is_refunded_on_conviction_or_default_and_burned_only_at_an_unconvicted_horizon() {
    let serve = |w: &mut World, daa: u64, id: Digest, trace: &misaka_palw_kernel::trace::TraceV1, p: u32| {
        let ev = w.block(daa, vec![T::Respond { claim: id, stage: 0, position: p, bytes: position(trace, p, |_| {}) }]);
        assert!(ev.contains(&E::Served { claim: id, stage: 0, position: p }), "{ev:?}");
    };
    // An HONEST claim: a demand only made the producer serve. Final, then its liability horizon ends unconvicted: the bond burns.
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let (id, trace) = (h.claim.id(), h.trace.clone());
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    w.block(11, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 1 }]);
    serve(&mut w, 12, id, &trace, 1);
    assert_eq!(w.l.bonds[&OUTSIDER].reserved, 10, "held, its fate pending");
    assert!(w.block(60, vec![]).contains(&E::Final { claim: id, reward: 7 }));
    assert_eq!(w.l.bonds[&OUTSIDER].reserved, 10, "Final is not the fate: the claim stays convictable through its horizon");
    let ev = w.block(261, vec![]);
    assert!(ev.contains(&E::ServedDemandBondsBurned { claim: id, burned: 10 }), "{ev:?}");
    assert_eq!((w.l.bonds[&OUTSIDER].reserved, w.l.bonds[&OUTSIDER].collateral), (0, 990));

    // A LYING claim whose proof lands only after Final (the grace long gone): the bond held all along is refunded at the conviction.
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let (id, da, trace) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]), lie.trace.clone());
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    w.block(11, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    serve(&mut w, 12, id, &trace, at.0);
    assert!(w.block(60, vec![]).contains(&E::Final { claim: id, reward: 7 }));
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("the served values complete the check") };
    let ev = w.block(100, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, true)), "{ev:?}");
    assert_eq!((w.l.bonds[&OUTSIDER].reserved, w.l.bonds[&OUTSIDER].collateral), (0, 1000), "a true demand is never penalised");

    // A claim that DEFAULTS on another position: the served position's bond is refunded with the default.
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let (id, trace) = (h.claim.id(), h.trace.clone());
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    w.block(
        11,
        vec![
            T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 1 },
            T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 2 },
        ],
    );
    serve(&mut w, 12, id, &trace, 1);
    let ev = w.block(31, vec![]);
    assert!(ev.iter().any(|e| matches!(e, E::ProducerDefault { position: 2, .. })), "{ev:?}");
    assert_eq!((w.l.bonds[&OUTSIDER].reserved, w.l.bonds[&OUTSIDER].collateral), (0, 1000), "refunded with the default");
    assert!(w.l.served_demands.is_empty());
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
    assert_eq!(w.l.bonds[&[0xC0; 64]].reserved, 1000 + 1, "(and the refused claim's seal deposit: bonded seals)");

    // The exit: nothing new is backed, and nothing is withdrawn while reserved or inside the delay.
    w.block(11, vec![T::RequestExit { bond: [0xC0; 64] }]);
    let ev = w.block(12, vec![T::Withdraw { bond: [0xC0; 64] }]);
    assert!(refused(&ev).is_some());
    w.block(60, vec![]); // Final for claim 1
    assert_eq!(w.consumer.paid(&[0xC0; 64]), 7);
    let ev = w.block(261, vec![T::Withdraw { bond: [0xC0; 64] }]); // the release happens in this block's tick, after the tx
    assert!(refused(&ev).is_some() && ev.contains(&E::Released { claim: id1 }), "{ev:?}");
    let ev = w.block(262, vec![c2.tx.clone()]);
    assert_eq!(refused(&ev).as_deref(), Some("the producer bond is exiting"));
    let ev = w.block(263, vec![T::Withdraw { bond: [0xC0; 64] }]);
    // (1500 less the refused second claim's seal deposit, forfeited when that seal expired unrevealed: bonded seals)
    assert_eq!(ev, vec![E::Withdrawn { bond: [0xC0; 64], amount: 1499 }], "the collateral; the 7 reward was paid out at Final");

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

// ── D′: a long-history (held) window is derived: never served, convicted from the committed rows ──────────────────────────

#[test]
fn a_history_window_is_never_served_and_a_misderived_window_is_convicted_from_the_rows_alone() {
    use misaka_palw_kernel::public::FaultProofWireV1;
    let mut w = World::new();
    let mask = derived();
    let appends: Vec<(u16, u16)> = w
        .program
        .occurrences()
        .iter()
        .enumerate()
        .flat_map(|(s, (b, _))| {
            w.program.blocks[*b as usize]
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| matches!(n.prim, Prim::HistAppend { .. }))
                .map(move |(n, _)| (s as u16, n as u16))
        })
        .collect();
    assert!(!appends.is_empty() && appends.iter().all(|(s, n)| mask[*s as usize][*n as usize]), "every window is derived");
    assert!(mask.iter().flatten().filter(|d| **d).count() > appends.len(), "and so are the views of a window");

    // Honest: the DA publishes no window at all, and a fresh outsider passes the claim.
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let (id, da) = (h.claim.id(), Da::publishing(&h.trace, &[]));
    assert!(da.0.keys().all(|(_, s, n)| !mask[*s as usize][*n as usize]));
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    assert_eq!(outsider(&w, id, &da), OutsiderFindingV1::Clean);

    // A permuted window at position 3: committed, never published; the outsider rebuilds it from the rows and convicts.
    let (s, n) = appends[0];
    let job = w.post_job(11, &[3, 17, 9], 3, 2);
    let generated = w.greedy(&w.params.clone(), &job.prompt, 3);
    let lie = w.produce(&job, PRODUCER, generated, &w.params.clone(), |t| {
        let win = &mut t.values[3][s as usize][n as usize];
        let row: usize = win.shape[1..].iter().product();
        let (a, b) = win.data.split_at_mut(row);
        a.swap_with_slice(&mut b[..row]);
    });
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(12, vec![lie.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Kernel(bytes)) = outsider(&w, id, &da) else { panic!() };
    let wire: FaultProofWireV1 = borsh::from_slice(&bytes).unwrap();
    assert_eq!((wire.position, wire.occurrence, wire.node, wire.kind), (3, s, n, 3), "misderived");
    assert!(wire.output.bytes.is_empty(), "no byte of the producer's window is in the proof");
    let ev = w.block(13, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: ProsecutionV1::Kernel(bytes) }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "{ev:?}");

    // A position response carries no window: one that serves one is malformed.
    let job = w.post_job(20, &[3, 17, 9], 3, 3);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    let trace2 = h.trace.clone();
    w.block(21, vec![h.tx, T::PanelCovered { claim: id }]);
    w.block(22, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 3 }]);
    let served_window = position(&trace2, 3, |r| {
        r[s as usize][n as usize] = MaterialResponseV1::Whole(TensorWireV1::of(&trace2.values[3][s as usize][n as usize]))
    });
    let ev = w.block(23, vec![T::Respond { claim: id, stage: 0, position: 3, bytes: served_window }]);
    assert_eq!(ev, vec![E::ResponseRejected { claim: id, stage: 0, position: 3, class: "malformed" }]);
    let ev = w.block(24, vec![T::Respond { claim: id, stage: 0, position: 3, bytes: position(&trace2, 3, |_| {}) }]);
    assert_eq!(ev, vec![E::Served { claim: id, stage: 0, position: 3 }]);
}

// ── one claim per job (C4 F-C4-03): a copied claim is never paid twice ────────────────────────────────────────────────────

#[test]
fn a_bond_that_copies_a_published_claim_is_refused_and_a_failed_holder_frees_the_job() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let honest = w.honest(&job, 3);
    let id = honest.claim.id();
    let T::CommitClaim { evidence, commitments, .. } = honest.tx.clone() else { unreachable!() };
    let ev = w.block(10, vec![honest.tx.clone(), T::PanelCovered { claim: id }]);
    assert!(ev.contains(&E::ClaimCommitted { claim: id }), "{ev:?}");
    // Another bond re-signs the same evidence and commitments for the same job.
    let copy = KernelClaimV1 { producer_bond: SPAM1, ..honest.claim.clone() };
    let ev = w.block(11, vec![T::CommitClaim { claim: copy.clone(), evidence: evidence.clone(), commitments: commitments.clone() }]);
    assert_eq!(refused(&ev).as_deref(), Some("another claim already holds the job (one claim per job)"), "{ev:?}");
    // After Final the job stays taken: one computation, one reward.
    w.block(200, vec![]);
    assert!(matches!(w.state(&id), ClaimStateV1::Final { .. }));
    let ev = w.block(201, vec![T::CommitClaim { claim: copy, evidence, commitments }]);
    assert!(refused(&ev).is_some(), "{ev:?}");
    assert_eq!(w.l.claims.values().filter(|r| r.rewarded).count(), 1, "paid once for the job");

    // A convicted holder frees its job for an honest claim.
    let job = w.post_job(210, &[3, 17, 9], 3, 2);
    let (_, lie) = w.lying(&job, 3);
    let lie_id = lie.claim.id();
    let da = Da::publishing(&lie.trace, &[]);
    w.block(211, vec![lie.tx, T::PanelCovered { claim: lie_id }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, lie_id, &da) else { panic!() };
    let ev = w.block(212, vec![T::FileProof { accuser: OUTSIDER, claim: lie_id, proof }]);
    assert!(convicted(&ev).is_some(), "{ev:?}");
    let redo = w.produce(&job, OUTSIDER, w.greedy(&w.params.clone(), &job.prompt, 3), &w.params.clone(), |_| {});
    let ev = w.block(213, vec![redo.tx]);
    assert!(ev.contains(&E::ClaimCommitted { claim: redo.claim.id() }), "{ev:?}");
}

#[test]
fn an_uncovered_claim_never_locks_its_job_and_the_first_covered_claim_holds_it() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    // A squatter commits a claim nobody covers (here a lie; junk of the right shape behaves the same).
    let (_, squat) = w.lying(&job, 3);
    let squat_id = squat.claim.id();
    let ev = w.block(10, vec![squat.tx]);
    assert!(ev.contains(&E::ClaimCommitted { claim: squat_id }), "{ev:?}");
    // The honest producer's claim for the same job is still accepted, and its coverage takes the job.
    let honest = w.produce(&job, OUTSIDER, w.greedy(&w.params.clone(), &job.prompt, 3), &w.params.clone(), |_| {});
    let id = honest.claim.id();
    let ev = w.block(12, vec![honest.tx, T::PanelCovered { claim: id }]);
    assert!(ev.contains(&E::ClaimCommitted { claim: id }), "{ev:?}");
    assert_eq!(w.l.job_claims.get(&job.id()), Some(&id));
    // The squatter's coverage is refused now: it is never paid, and it times out with its collateral returned.
    let ev = w.block(13, vec![T::PanelCovered { claim: squat_id }]);
    assert_eq!(refused(&ev).as_deref(), Some("another claim already holds the job (one claim per job)"), "{ev:?}");
    w.block(300, vec![]);
    assert!(matches!(w.state(&id), ClaimStateV1::Final { .. }));
    assert!(matches!(w.state(&squat_id), ClaimStateV1::TimedOut { .. }));
    assert_eq!(w.l.claims.values().filter(|r| r.rewarded).count(), 1);
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0, "the squatter's reservation came back, it earned nothing");
}

// ── DA16 (RFC-0009 §4.2): a claim whose material obligation moved to bonded providers ─────────────────────────────────────────

/// **A provider-liable default never charges the producer, and a lapse voids without a conviction.** The consumer marks the claim
/// provider-liable (its court's rows say the material moved): the withheld position's demand defaults as `ProviderLiableDefault` — the
/// producer's collateral untouched, the claim void (`Unavailable`, not producer-defaulted), the demand bond back, no reward ever. A
/// second claim lapses through `provider_lapse`; a third, not provider-liable, cannot be lapsed and defaults on the producer as today.
/// After Final a provider-liable default leaves the producer's reward and reservation alone (the providers pay; the fact is withdrawn).
#[test]
fn a_provider_liable_default_never_charges_the_producer_and_a_lapse_voids_without_conviction() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let id = lie.claim.id();
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    w.l.provider_liable.insert(id);
    w.block(12, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    let (collateral, burned) = (w.l.bonds[&PRODUCER].collateral, w.l.burned);
    let ev = w.block(32, vec![]);
    assert!(
        ev.contains(&E::ProviderLiableDefault {
            claim: id,
            stage: 0,
            position: at.0,
            last: None,
            post_final: false,
            demanders: vec![OUTSIDER]
        }),
        "{ev:?}"
    );
    assert!(!ev.iter().any(|e| matches!(e, E::ProducerDefault { .. } | E::Convicted { .. })), "{ev:?}");
    assert_eq!(w.state(&id), ClaimStateV1::Unavailable { daa: 32, producer_defaulted: false });
    assert_eq!(w.l.bonds[&PRODUCER].collateral, collateral, "the producer pays nothing for its providers' failure");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0, "its reservation is released whole");
    assert_eq!(w.l.burned, burned);
    assert_eq!(w.l.bonds[&OUTSIDER].reserved, 0, "the demand bond returns");
    w.block(100, vec![]);
    assert!(!w.l.claims[&id].rewarded && !w.l.claims[&id].convicted, "void: no reward, no conviction");

    // A lapse (every provider charged on its own challenges) voids a live claim the same way.
    let job = w.post_job(110, &[3, 17, 9], 3, 2);
    let h = w.honest(&job, 3);
    let id2 = h.claim.id();
    w.block(111, vec![h.tx, T::PanelCovered { claim: id2 }]);
    assert!(w.l.provider_lapse(&id2).is_err(), "not provider-liable: the producer's own claim cannot be lapsed by a court");
    w.l.provider_liable.insert(id2);
    let ev = w.l.provider_lapse(&id2).unwrap();
    assert_eq!(ev, vec![E::ProviderLapsed { claim: id2, post_final: false }]);
    assert!(matches!(w.state(&id2), ClaimStateV1::Unavailable { producer_defaulted: false, .. }));
    assert!(w.l.provider_lapse(&id2).unwrap().is_empty(), "a decided claim lapses once");
    w.block(200, vec![]);
    assert!(!w.l.claims[&id2].rewarded && !w.l.claims[&id2].convicted);
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0);

    // After Final: the providers pay; the producer keeps its reward and its reservation backs its fraud liability.
    let mut w = World::new();
    let (id, at, _, _) = final_lying_claim(&mut w);
    w.l.provider_liable.insert(id);
    w.block(100, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    let collateral = w.l.bonds[&PRODUCER].collateral;
    let ev = w.block(120, vec![]);
    assert!(ev.contains(&E::ProviderLiableDefault {
        claim: id,
        stage: 0,
        position: at.0,
        last: None,
        post_final: true,
        demanders: vec![OUTSIDER]
    }));
    assert!(matches!(w.state(&id), ClaimStateV1::Final { .. }));
    assert_eq!(w.l.bonds[&PRODUCER].collateral, collateral);
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 1000, "held until the liability horizon: a false computation stays the miner's");
}
