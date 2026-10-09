//! **RFC-0015 Panel=0 (`OptimisticPublicVerification`) at the ledger level** (§13.1 / §13.2 reference half).
//!
//! Every verifier in these tests is built fresh from the replayed chain and public DA bytes with its own salt (`outsider`): no
//! producer trace, no Panel capture, no served view. Every block goes through the mini consensus consumer (strict encodings, refusals
//! leave the root byte-identical, settlement instructions applied to a bond book that must match the ledger's own view) and then the
//! OPV invariants are recomputed from the rows.
//!
//! The policy values are examples for tests, chosen for no network. The node half of §13.2 (real node, mempool, RPC, IBD) is lane D's
//! and stays a GAP: see `docs/design/palw/rfc-0015-panel-free-record.md`.

mod common;

use common::chain::T;
use common::ledger_world::*;
use common::opv_world::*;
use common::root_of;
use misaka_palw_challenge::beacon::eligibility_v1;
use misaka_palw_challenge::{BeaconContextV1, FinalPathV1, IneligibleV1, RootV1, SubjectKindV1, reference_policy_v1};
use misaka_palw_kernel::descriptor::{ContextPolicyV1, ModelKernelBindingV1, k2_tir_v1_descriptor};
use misaka_palw_kernel::hash::{Digest, hex};
use misaka_palw_kernel::job::KernelClaimV1;
use misaka_palw_kernel::ledger::{
    AuthV1, KernelLedgerV1, KernelRouteObjectV1 as O, LedgerEventV1 as E, OutsiderFindingV1, ProsecutionV1, RefusalKindV1,
};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::mode::{VerificationModeV1 as Mode, class_id_for_mode_v1};
use misaka_palw_kernel::opv::{
    CANONICAL_WORK_DOMAIN_V1, FinalAssuranceV1, FinalStandingV1, OpvPolicyV1, StateRootPartsV2, WorkFinalContextV1,
};
use misaka_palw_kernel::receipt::{ConstraintTallyV1, TallyPolicyV1, TallyStateV1};
use misaka_palw_kernel::route::{KERNEL_ROUTE_VERSION_V1, TAG_REGISTER_CLASS_V2, max_encoded_bytes_of_tag};
use misaka_palw_kernel::trace::ParamCommitmentsV1;

const OPV: Mode = Mode::OptimisticPublicVerification;

fn auth(bond: Digest) -> AuthV1 {
    AuthV1 { signer_bond: bond }
}

fn ctx(panel: Option<(Digest, u64)>) -> WorkFinalContextV1 {
    WorkFinalContextV1 {
        accepted_position: 10,
        settlement_position: 60,
        occurrence_index: 0,
        validity_independent: true,
        depends_on_profiles: vec![],
        panel,
    }
}

fn settled(ev: &[E]) -> Vec<&E> {
    ev.iter().filter(|e| matches!(e, E::Final { .. })).collect()
}

// ── §4.1 / §4.2: the mode is part of the class identity; registration is gated ────────────────────────────────────────────

#[test]
fn the_mode_is_part_of_the_class_identity_and_an_optimistic_class_registers_only_where_the_policy_the_fence_and_the_gate_allow() {
    // (a) A network with no OPV policy: tag 13 is refused whatever it names, and nothing it did changes the (historical) root.
    let mut w = World::new();
    assert_eq!(w.l.root(), w.l.root_parts().root(), "no OPV policy: the root is the historical one");
    let txs = register_in(&w, OPV, PRODUCER);
    let (attest, object) = (txs[0].clone(), txs.last().unwrap().clone());
    w.block_raw(2, vec![attest.clone()]);
    let before = w.l.root();
    let misaka_palw_kernel::ledger::LedgerTxV1::Object { auth: a, object: o } = object.clone() else { unreachable!() };
    let r = w.l.apply_object(&o, &a).unwrap_err();
    assert_eq!((r.kind, r.object), (RefusalKindV1::Rule, "RegisterClassV2"), "{r}");
    assert!(r.why.contains("no OPV policy"), "{r}");
    assert_eq!(w.l.root(), before, "a refusal leaves the state byte-identical");
    assert!(!w.l.optimistic_allowed());

    // (b) A policy whose fence is not reached: refused before it, registered at it.
    let mut opv = opv_example();
    opv.activation_daa = Some(100);
    let mut w = World::with_opv(policy(), opv);
    assert_eq!(w.class, [0; 64]);
    assert!(refused(&w.events).unwrap().contains("fence is not reached"), "{:?}", w.events);
    let ev = w.block_raw(99, register_in(&w, OPV, PRODUCER));
    assert!(refused(&ev).unwrap().contains("fence is not reached"), "{ev:?}");
    let ev = w.block_raw(100, register_in(&w, OPV, PRODUCER));
    let Some(E::ClassRegistered { class: opv_class }) = ev.iter().find(|e| matches!(e, E::ClassRegistered { .. })) else {
        panic!("{ev:?}")
    };
    assert!(w.l.optimistic_allowed());
    assert_eq!(w.l.mode_of_class(opv_class), OPV);

    // (c) The same program under both modes is two classes; the legacy one keeps its historical id, the OPV id binds the mode.
    let mut w = World::new_opv();
    let opv_class = w.class;
    let legacy = w.register_panel_class(3);
    assert_ne!(legacy, opv_class);
    let binding = ModelKernelBindingV1 {
        descriptor_digest: k2_tir_v1_descriptor().digest(),
        plan_root: w.l.classes[&legacy].plan.root(),
        program_root: root_of(&w.program),
        artifact_root: ParamCommitmentsV1::of(&w.params).root(),
        tokenizer_or_input_schema_root: [0; 64],
        task_output_schema: [0; 64],
        context_and_state_policy: ContextPolicyV1 { max_positions: 64 },
    };
    assert_eq!(legacy, binding.class_binding_id(), "a Panel-licensed class keeps the id it always had");
    assert_eq!(opv_class, class_id_for_mode_v1(&binding.class_binding_id(), OPV));
    assert_eq!((w.l.mode_of_class(&legacy), w.l.mode_of_class(&opv_class)), (Mode::PanelLicensed, OPV));

    // (d) A re-registration under the same mode is refused; the Panel-licensed mode has one registration path (tag 1).
    let ev = w.block_raw(4, register_in(&w, OPV, PRODUCER));
    assert!(refused(&ev).unwrap().contains("already registered"), "{ev:?}");
    let ev = w.block_raw(5, register_in(&w, Mode::PanelLicensed, PRODUCER));
    assert!(refused(&ev).unwrap().contains("tag 1"), "{ev:?}");

    // (e) A claim's evidence names its class (and so its mode): a valid trace of the Panel class cannot be committed on an OPV
    // job, however identical the prompt and the output are.
    let panel_job = w.with_class(legacy, |w| w.post_job(6, &[3, 17, 9], 3, 9));
    let opv_job = w.post_job(7, &[3, 17, 9], 3, 9);
    assert_ne!(panel_job.id(), opv_job.id());
    let p = w.with_class(legacy, |w| w.honest(&panel_job, 3));
    let T::CommitClaim { evidence, commitments, .. } = p.tx.clone() else { unreachable!() };
    let relabelled = KernelClaimV1 { job_id: opv_job.id(), ..p.claim.clone() };
    let ev = w.block(10, vec![T::CommitClaim { claim: relabelled, evidence, commitments }]);
    assert!(refused(&ev).unwrap().contains("WrongClass"), "the evidence names the Panel class: {ev:?}");

    // (f) Registration still needs the code-derived PUBLIC_PROSECUTION_COMPLETE (no exception for a mode without a Panel), and the
    // class's worst filing must fit the carriers it must be carried by.
    let mut strict = policy();
    strict.prosecution.max_public_bytes = 1024;
    let w = World::with_opv(strict, opv_example());
    assert_eq!(w.class, [0; 64]);
    assert!(refused(&w.events).unwrap().starts_with("not publicly prosecutable"), "{:?}", w.events);
    let mut small = opv_example();
    small.carrier.filing_cap = 1 << 10;
    let w = World::with_opv(policy(), small);
    assert_eq!(w.class, [0; 64]);
    assert!(refused(&w.events).unwrap().contains("not carriable"), "{:?}", w.events);

    // (g) The fence is re-read at every claim: an OPV class on a ledger whose fence is somehow not reached commits nothing.
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let saved = w.l.opv.policy.unwrap();
    let mut late = saved;
    late.activation_daa = Some(10_000);
    w.l.opv.policy = Some(late);
    let ev = w.block(10, vec![h.tx.clone()]);
    assert!(refused(&ev).unwrap().contains("fence is not reached"), "{ev:?}");
    w.l.opv.policy = Some(saved);
    let ev = w.block(11, vec![h.tx]);
    assert!(ev.contains(&E::ClaimCommitted { claim: h.claim.id() }), "{ev:?}");
}

#[test]
fn an_optimistic_policy_is_a_genesis_constant_validated_against_the_ledger_policy() {
    let d = k2_tir_v1_descriptor();
    let base = || KernelLedgerV1::genesis(policy(), common::active_for(&d), vec![d.clone()]).unwrap();
    base().with_opv_policy(opv_example()).unwrap();
    let mut thin = opv_example();
    thin.economics.reservation_per_claim = 199;
    assert!(base().with_opv_policy(thin).unwrap_err().contains("reservation"));
    // A ledger that has moved on takes no policy.
    let mut moved = base();
    moved.begin_block(5).unwrap();
    assert!(moved.with_opv_policy(opv_example()).unwrap_err().contains("genesis"));
}

#[test]
fn the_network_policy_admits_a_class_for_the_optimistic_mode_a_registrant_never_chooses_it_for_its_own_program() {
    let mut w = World::new_opv();
    // A second program (a different fixture) registered by a bond that the network never admitted: refused, whatever it signed.
    let fx = misaka_palw_tir_sketch::fixture::dense_moe_v1(8);
    let d = k2_tir_v1_descriptor();
    let plan = misaka_palw_kernel::plan::plan_for_tir_program_v1(&d, &w.program, root_of(&w.program), common::MAX_POSITIONS).unwrap();
    let other_params = fx.params;
    let pc = ParamCommitmentsV1::of(&other_params);
    let register = O::RegisterClassV2 {
        mode: OPV,
        descriptor: d.digest(),
        program_bytes: w.program.encode(),
        plan: plan.clone(),
        param_commitments: pc.clone(),
    };
    let attest = misaka_palw_kernel::ledger::LedgerTxV1::AttestArtifact { artifact_root: pc.root() };
    let obj = |o: O| misaka_palw_kernel::ledger::LedgerTxV1::Object { auth: auth(PRODUCER), object: o };
    let ev = w.block_raw(2, vec![attest.clone(), obj(register.clone())]);
    assert!(refused(&ev).unwrap().contains("has not admitted"), "{ev:?}");
    // Admitted but never attested public: still refused (the weights must be publicly obtainable, by the consumer's fact).
    let empty = ParamCommitmentsV1 { by_instance: Default::default() };
    let unattested = misaka_palw_kernel::ledger::single_class_id_v1(d.digest(), &w.program.encode(), &plan, &empty, OPV);
    let ev = w.block_raw(
        2,
        vec![
            misaka_palw_kernel::ledger::LedgerTxV1::AdmitOptimisticClass { class: unattested },
            obj(O::RegisterClassV2 {
                mode: OPV,
                descriptor: d.digest(),
                program_bytes: w.program.encode(),
                plan: plan.clone(),
                param_commitments: empty,
            }),
        ],
    );
    assert!(refused(&ev).unwrap().contains("not attested public"), "{ev:?}");
    // Admitting another id does not admit this one; admitting this one does.
    let ev =
        w.block_raw(3, vec![misaka_palw_kernel::ledger::LedgerTxV1::AdmitOptimisticClass { class: [7; 64] }, obj(register.clone())]);
    assert!(refused(&ev).unwrap().contains("has not admitted"), "{ev:?}");
    let id = misaka_palw_kernel::ledger::single_class_id_v1(d.digest(), &w.program.encode(), &plan, &pc, OPV);
    assert_ne!(id, w.class, "other weights, another class");
    let ev = w.block_raw(4, vec![misaka_palw_kernel::ledger::LedgerTxV1::AdmitOptimisticClass { class: id }, obj(register)]);
    assert!(ev.contains(&E::ClassRegistered { class: id }), "{ev:?}");
    assert_eq!(w.l.opv.classes.len(), 2);
    // A ledger with no OPV policy takes no admission (a hidden set outside the root would let nodes diverge silently).
    let mut dormant = World::new();
    assert!(dormant.l.admit_optimistic_class([1; 64]).is_err());
    assert!(dormant.l.opv.admitted.is_empty());
    let _ = &mut dormant;
}

#[test]
fn a_class_whose_prosecution_could_be_censored_for_less_than_it_pays_is_not_admitted() {
    // A court that runs two filings per block (one reserved for proofs, C4 F-C4R3-05) and charges 1 per dismissed filing: saturating it
    // through the window (50) and the liability horizon (200) costs 2 × 250 = 500. A class whose claims can gain more than that is
    // refused — its producer could buy the silence of every prosecutor.
    let mut narrow = policy();
    narrow.max_adjudications_per_block = 2;
    narrow.dismissed_proof_fee = 1;
    let mut opv = opv_example();
    opv.economics.external_gain_bound = 80;
    let w = World::with_opv(narrow, opv);
    assert_ne!(w.class, [0; 64], "a gain of 100 < 500 registers: {:?}", w.events);
    let mut rich = opv_example();
    rich.economics.external_gain_bound = 500; // gain 520; reservation must follow: max(620, 1040) after a self-recouped half (GAP-R7)
    rich.economics.reservation_per_claim = 2080;
    let w = World::with_opv(narrow, rich);
    assert_eq!(w.class, [0; 64]);
    assert!(refused(&w.events).unwrap().contains("could be censored"), "{:?}", w.events);
    // With the default court budget (64 runs, 5 per junk filing) the same gain is safe.
    let w = World::with_opv(policy(), rich);
    assert_ne!(w.class, [0; 64], "{:?}", w.events);
}

#[test]
fn a_copied_optimistic_claim_is_always_refused_because_the_first_reveal_holds_the_job_and_a_seal_cannot_be_pre_dated() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let original = w.honest_by(&job, HONEST, 3);
    let (id, copy) = (original.claim.id(), w.honest_by(&job, SQUATTER, 3));
    assert_ne!(copy.claim.id(), id, "another producer's claim of the same output is another claim");
    w.block(10, vec![original.tx.clone()]);
    // The copyist sees the reveal at 10: it can seal its copy no earlier than 10 and reveal no earlier than 11 — and the job is held.
    let ev = w.block(11, vec![copy.tx.clone()]);
    assert!(refused(&ev).unwrap().contains("holds the job") || refused(&ev).unwrap().contains("one claim per job"), "{ev:?}");
    assert!(!w.l.claims.contains_key(&copy.claim.id()));
    let ev = w.block(60, vec![]);
    assert_eq!(ev, vec![E::Final { claim: id, reward: 7 }]);
    assert_eq!(w.consumer.paid(&SQUATTER), 0, "the copy was never paid");
    assert_eq!(w.consumer.paid(&HONEST), 7);
}

// ── §4.1 / §6.2: an honest claim finalizes with no Panel ──────────────────────────────────────────────────────────────────

#[test]
fn an_honest_optimistic_claim_finalizes_by_the_window_rule_with_no_panel_and_no_fabricated_tally() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let (id, da) = (h.claim.id(), Da::publishing(&h.trace, &[]));
    let ev = w.block(10, vec![h.tx]);
    assert!(ev.contains(&E::ClaimCommitted { claim: id }), "{ev:?}");
    assert_eq!(w.state(&id), ClaimStateV1::Challengeable { since_daa: 10, window_end_daa: 60 });
    assert_eq!(w.l.mode_of_claim(&id), Some(OPV));
    assert_eq!((w.l.claims[&id].reserved, w.l.bonds[&PRODUCER].reserved), (1000, 1000));

    // Public discovery: the clock a fresh verifier plans by (admitted 10, window 50, budgets 10+10+2+2 before the first filing).
    let v = w.l.opv_claim_view(&id).unwrap();
    assert_eq!((v.admitted_daa, v.verifier_start_cutoff_daa, v.final_floor_daa, v.hard_deadline_daa), (10, 36, 60, 90));
    assert_eq!((v.reservation, v.max_gain, v.open_demands, v.served_positions), (1000, 100, 0, 0));
    assert_eq!(v.assurance, FinalAssuranceV1::OptimisticPublicVerification);

    // A Panel tally is refused whatever it says (a covered one and a silent one): nothing is licensed, nothing is fabricated.
    let ev = w.block(11, vec![T::PanelCovered { claim: id }]);
    assert!(refused(&ev).unwrap().contains("no Panel tally"), "{ev:?}");
    assert!(w.l.apply_panel_tally(&id, false).is_err() && w.l.apply_panel_tally(&id, true).is_err());
    assert_eq!(w.state(&id), ClaimStateV1::Challengeable { since_daa: 10, window_end_daa: 60 });

    // A fresh node replaying the chain from genesis finds the claim clean — and "clean" is not the chain's word.
    assert_eq!(outsider(&w, id, &da), OutsiderFindingV1::Clean);
    for daa in [30, 59] {
        assert!(w.block(daa, vec![]).is_empty(), "the window is open at {daa}");
    }
    let ev = w.block(60, vec![]);
    assert_eq!(ev, vec![E::Final { claim: id, reward: 7 }]);
    assert_eq!(w.state(&id), ClaimStateV1::Final { final_daa: 60 });
    assert_eq!(w.consumer.paid(&PRODUCER), 7);
    assert_eq!(w.l.job_claims[&job.id()], id, "an OPV claim holds its job from its first reveal and keeps it once Final");

    // The Final receipt: the facts RFC-0010's Panel-assignment beacon needs, Panel-independent by construction.
    let r = w.l.final_receipt(&id).unwrap();
    assert_eq!((r.mode, r.accepted_daa, r.final_daa, r.standing), (OPV, 10, 60, FinalStandingV1::Standing));
    assert_eq!(r.source_profile_id, w.class);
    assert_eq!(r.canonical_work_id, misaka_palw_kernel::hash::object_id(CANONICAL_WORK_DOMAIN_V1, &(w.class, job.id())));
    assert!(r.da_satisfied && r.assurance.statement().contains("not a proof"));
    let event = r.to_work_final_event(&ctx(None)).unwrap();
    assert_eq!(event.final_path, FinalPathV1::PanelIndependent);
    assert_eq!(w.l.final_receipts(), vec![r]);

    // The reservation is held through the liability horizon (60 + 200), then released; nothing else moves.
    assert!(w.block(260, vec![]).is_empty());
    assert_eq!(w.block(261, vec![]), vec![E::Released { claim: id }]);
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0);
    assert_eq!(w.l.opv_live_counts(&PRODUCER), (0, 0));
}

// ── §6.2 / §13.2: a lying claim, before and after Final ───────────────────────────────────────────────────────────────────

#[test]
fn a_lying_optimistic_claim_is_convicted_before_final_and_after_it_by_one_outsider_and_the_job_is_freed() {
    let mut w = World::new_opv();

    // Before Final: the producer lies, no Panel exists to be captured; one ordinary bond finds it from public bytes.
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("the lie is found from public material") };
    let ev = w.block(30, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: proof.clone() }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "{ev:?}");
    assert!(matches!(w.state(&id), ClaimStateV1::Convicted { .. }));
    // (5000 less the slashed reservation and the claim's non-refundable admission fee of 3, C4 F-C4R3-05.)
    assert_eq!((w.l.bonds[&PRODUCER].collateral, w.l.bonds[&PRODUCER].reserved), (4000 - 3, 0));
    assert_eq!(w.consumer.paid(&OUTSIDER), 500, "the bounty is a share of the collected reservation");
    assert_eq!(w.l.burned, 500 + 3 + 2, "the burned half of the slash, the admission fee and the job's posting fee (GAP-5)");
    let ev = w.block(31, vec![T::FileProof { accuser: SPAM1, claim: id, proof }]);
    assert_eq!(ev, vec![E::Duplicate { claim: id }], "a claim is convicted once; a copied proof pays nobody");
    w.block(200, vec![]);
    assert!(!w.l.claims[&id].rewarded, "a convicted claim never finalizes");
    assert!(w.l.final_receipt(&id).is_none());

    // The job it held is free again: an honest producer answers it and finalizes.
    let honest = w.honest_by(&job, HONEST, 3);
    let hid = honest.claim.id();
    w.block(210, vec![honest.tx]);
    assert_eq!(w.block(260, vec![]), vec![E::Final { claim: hid, reward: 7 }]);

    // After Final: nobody checked inside the window; the claim finalized and was paid; the liability horizon still convicts.
    let job = w.post_job(300, &[3, 17, 9], 3, 2);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(310, vec![lie.tx]);
    assert_eq!(w.block(360, vec![]), vec![E::Final { claim: id, reward: 7 }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(400, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, true)), "post-Final liability: {ev:?}");
    assert_eq!(w.l.bonds[&PRODUCER].collateral, 3000 - 2 * 3, "two slashed reservations and two admission fees");
    let r = w.l.final_receipt(&id).unwrap();
    assert_eq!(r.standing, FinalStandingV1::ConvictedAfterFinal);
    assert!(!r.to_work_final_event(&ctx(None)).unwrap().claim_final, "a convicted work is no longer a Final source");

    // Past the liability horizon the reservation was released (finite retained exposure) and a proof is refused: no court, no fee.
    let job = w.post_job(500, &[3, 17, 9], 3, 3);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(510, vec![lie.tx]);
    w.block(560, vec![]);
    assert_eq!(w.block(761, vec![]), vec![E::Released { claim: id }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let fee_payer = w.l.bonds[&OUTSIDER].collateral;
    let ev = w.block(762, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(refused(&ev).as_deref(), Some("past the liability horizon"), "{ev:?}");
    assert_eq!(w.l.bonds[&OUTSIDER].collateral, fee_payer, "a true proof is never charged the dismissal fee");
}

// ── §5 / §13.2: withheld material is availability, never fraud, and never Final ───────────────────────────────────────────

#[test]
fn withheld_material_is_a_default_not_a_fraud_conviction_and_the_claim_never_finalizes() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]));
    w.block(10, vec![lie.tx]);
    let OutsiderFindingV1::Demand(positions) = outsider(&w, id, &da) else { panic!("the withheld position must be demanded") };
    assert_eq!(positions, vec![(0, at.0)]);
    w.block(22, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    assert!(matches!(w.state(&id), ClaimStateV1::Disputed { .. }), "an accepted demand blocks Final");
    assert_eq!(w.l.opv_claim_view(&id).unwrap().open_demands, 1);
    let ev = w.block(42, vec![]);
    assert_eq!(ev, vec![E::ProducerDefault { claim: id, stage: 0, position: at.0, last: None, penalty: 100 }]);
    assert_eq!(w.state(&id), ClaimStateV1::Unavailable { daa: 42, producer_defaulted: true });
    assert!(!w.l.claims[&id].convicted, "availability, not fraud");
    // The penalty is split at least like a slash so a producer cannot cycle it through its own demander for nothing (the larger of
    // the policy's 100 permille and the 500 permille a slash burns, C4 F-C4R3-02), the rest to the demander. The rest of the
    // reservation is held through the default's liability horizon, then released (no valid proof ever arrived).
    assert_eq!((w.consumer.paid(&OUTSIDER), w.l.burned), (50, 50 + 3 + 2), "(and the admission fee and the job's posting fee)");
    assert_eq!((w.l.bonds[&PRODUCER].collateral, w.l.bonds[&PRODUCER].reserved), (4900 - 3, 900), "(and the admission fee)");
    assert_eq!(w.block(243, vec![]), vec![E::Released { claim: id }]);
    assert_eq!((w.l.bonds[&PRODUCER].collateral, w.l.bonds[&PRODUCER].reserved), (4900 - 3, 0));
    w.block(500, vec![]);
    assert!(!w.l.claims[&id].rewarded && w.l.final_receipt(&id).is_none());

    // After Final the same withholding forfeits the WHOLE remaining reservation, burned whole (the reward was already paid).
    let job = w.post_job(600, &[3, 17, 9], 3, 2);
    let (at, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]));
    w.block(610, vec![lie.tx]);
    assert_eq!(w.block(660, vec![]), vec![E::Final { claim: id, reward: 7 }]);
    let OutsiderFindingV1::Demand(_) = outsider(&w, id, &da) else { panic!() };
    let (paid, burned) = (w.consumer.paid(&OUTSIDER), w.l.burned);
    w.block(700, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    let ev = w.block(720, vec![]);
    assert_eq!(ev, vec![E::PostFinalDefault { claim: id, stage: 0, position: at.0, last: None, forfeited: 1000 }]);
    assert_eq!((w.consumer.paid(&OUTSIDER), w.l.burned), (paid, burned + 1000), "a post-Final forfeit pays no demander");
    assert!(!w.l.claims[&id].convicted && w.l.opv.claims[&id].forfeited_after_final);
    let r = w.l.final_receipt(&id).unwrap();
    assert_eq!(r.standing, FinalStandingV1::DaForfeitedAfterFinal);
    assert!(!r.to_work_final_event(&ctx(None)).unwrap().da_satisfied, "withheld material is no DA-satisfied beacon source");
    assert_eq!(w.l.opv_live_counts(&PRODUCER).0, 0, "the forfeit ended the reservation");
}

// ── §6: spam, windows and the hard deadline ───────────────────────────────────────────────────────────────────────────────

#[test]
fn spam_demands_and_joins_cannot_hold_an_optimistic_final_past_the_discovered_hard_deadline() {
    // Sweep the adversary's choice of when to open its demands: whenever it files, and however it joins, Final is never later
    // than the hard deadline the claim's view published at admission, and never earlier than the window's end.
    for filed_at in [11u64, 35, 58, 59] {
        let mut w = World::new_opv();
        let job = w.post_job(2, &[3, 17, 9], 3, 1);
        let h = w.honest(&job, 3);
        let (id, trace) = (h.claim.id(), h.trace.clone());
        w.block(10, vec![h.tx]); // window end 60, hard deadline 90
        let view = w.l.opv_claim_view(&id).unwrap();
        assert_eq!((view.final_floor_daa, view.hard_deadline_daa), (60, 90));
        let mut final_daa: Option<u64> = None;
        let mut step = |w: &mut World, daa: u64, txs: Vec<T>| -> Vec<E> {
            let ev = w.block(daa, txs);
            if final_daa.is_none() && ev.iter().any(|e| matches!(e, E::Final { .. })) {
                final_daa = Some(daa);
            }
            ev
        };
        let closed_window_demand = |w: &mut World, step: &mut dyn FnMut(&mut World, u64, Vec<T>) -> Vec<E>| {
            // A demand at the window's end, or after it, opens or joins nothing (so nothing restarts a clock).
            let ev = step(w, 60, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 7 }]);
            assert!(refused(&ev).is_some_and(|m| m.contains("window is closed")), "{ev:?}");
        };

        let demands: Vec<T> = (0..5)
            .flat_map(|p| {
                [
                    T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: p },
                    T::FileDemand { demander: SPAM2, claim: id, stage: 0, position: p },
                ]
            })
            .collect();
        let ev = step(&mut w, filed_at, demands);
        assert_eq!(ev.iter().filter(|e| matches!(e, E::DemandOpened { .. })).count(), 5);
        assert_eq!(ev.iter().filter(|e| matches!(e, E::DemandJoined { .. })).count(), 5, "a join shares the open demand's clock");
        // The honest producer serves each demand at its last moment; a served position cannot be demanded again.
        let deadline = filed_at + 20;
        if 60 < deadline {
            closed_window_demand(&mut w, &mut step);
        }
        let serve: Vec<T> =
            (0..5).map(|p| T::Respond { claim: id, stage: 0, position: p, bytes: position(&trace, p, |_| {}) }).collect();
        let ev = step(&mut w, deadline, serve);
        assert_eq!(ev.iter().filter(|e| matches!(e, E::Served { .. })).count(), 5, "{ev:?}");
        let ev = step(&mut w, deadline + 1, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 0 }]);
        assert!(refused(&ev).is_some(), "a served position is public: it cannot be demanded again to restart the grace");
        if 60 > deadline + 1 {
            closed_window_demand(&mut w, &mut step);
        }
        // Final waits out the proof grace after the last service, and the window.
        let want = (deadline + 10).max(60);
        let mut daa = deadline + 2;
        while step_final(&mut w, &mut step, &mut daa, want) {}
        let _ = &mut step;
        let final_daa = final_daa.expect("the claim finalizes");
        assert_eq!(final_daa, want, "filed at {filed_at}");
        assert!(final_daa <= view.hard_deadline_daa && final_daa >= view.final_floor_daa);
    }
}

/// Advance one block towards `want`; `true` while the claim has not finalized.
fn step_final(w: &mut World, step: &mut dyn FnMut(&mut World, u64, Vec<T>) -> Vec<E>, daa: &mut u64, want: u64) -> bool {
    if *daa > want + 1 {
        return false;
    }
    let ev = step(w, (*daa).max(w.l.daa), vec![]);
    *daa += 1;
    !ev.iter().any(|e| matches!(e, E::Final { .. }))
}

#[test]
fn a_proof_in_the_windows_last_block_blocks_final_and_an_unserved_demand_decides_the_claim_by_its_deadline() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx]); // window end 60
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(60, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "{ev:?}");
    assert!(settled(&ev).is_empty(), "the proof is applied before the window's tick: the claim never finalizes");

    // A demand in the last window block served at its deadline: the served values enable a proof BEFORE Final (the grace).
    let job = w.post_job(100, &[3, 17, 9], 3, 2);
    let (at, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]));
    let trace = lie.trace.clone();
    w.block(110, vec![lie.tx]); // window end 160
    w.block(159, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    let ev = w.block(178, vec![T::Respond { claim: id, stage: 0, position: at.0, bytes: position(&trace, at.0, |_| {}) }]);
    assert!(settled(&ev).is_empty(), "a service never hands the claim Final in its own block: {ev:?}");
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("the served values enable the proof") };
    let ev = w.block(179, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "convicted before Final, so no reward: {ev:?}");
    w.block(300, vec![]);
    assert!(!w.l.claims[&id].rewarded);
}

// ── §8: an invalid challenge is dismissed with its fee ────────────────────────────────────────────────────────────────────

#[test]
fn an_invalid_challenge_is_dismissed_with_its_fee_and_never_touches_an_honest_optimistic_claim() {
    let mut w = World::new_opv();
    let job1 = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job1, 3);
    let (lid, lda) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(5, vec![lie.tx]);
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Kernel(other_claims_proof)) = outsider(&w, lid, &lda) else { panic!() };

    let job2 = w.post_job(6, &[3, 17, 9], 3, 2);
    let h = w.honest_by(&job2, HONEST, 3);
    let (hid, hda) = (h.claim.id(), Da::publishing(&h.trace, &[]));
    w.block(10, vec![h.tx]);
    assert_eq!(outsider(&w, hid, &hda), OutsiderFindingV1::Clean);
    let (post, logits) = w.l.classes[&w.class].logits_at();
    let honest_logits = hda.get(2, post, logits).unwrap();
    let bad_decode =
        misaka_palw_kernel::job::DecodeFaultV1 { index: 0, logits: misaka_palw_kernel::public::TensorWireV1::of(&honest_logits) };
    let ev = w.block(
        20,
        vec![
            T::FileProof { accuser: OUTSIDER, claim: hid, proof: ProsecutionV1::Kernel(other_claims_proof) },
            T::FileProof { accuser: OUTSIDER, claim: hid, proof: ProsecutionV1::Kernel(vec![1, 2, 3]) },
            T::FileProof { accuser: OUTSIDER, claim: hid, proof: ProsecutionV1::Decode(bad_decode) },
        ],
    );
    assert_eq!(ev.iter().filter(|e| matches!(e, E::ProofDismissed { .. })).count(), 3, "{ev:?}");
    assert_eq!(w.l.bonds[&OUTSIDER].collateral, 1000 - 3 * 5, "a dismissed filing forfeits its fee (burned)");
    assert_eq!(w.state(&hid), ClaimStateV1::Challengeable { since_daa: 10, window_end_daa: 60 }, "a dismissal changes nothing");
    assert!(convicted(&ev).is_none());
    let ev = w.block(60, vec![]);
    assert!(ev.contains(&E::Final { claim: hid, reward: 7 }), "{ev:?}");
    assert_eq!(w.consumer.paid(&HONEST), 7);
}

// ── §6.2 / §13.2: a squatter pays for the job it holds ─────────────────────────────────────────────────────────────────────

#[test]
fn a_junk_squatter_is_slashed_by_any_outsider_and_a_withholding_squatter_defaults_so_squatting_a_job_costs_the_squatter() {
    let mut w = World::new_opv();

    // A well-formed but lying claim squats the job from its first reveal; the honest producer is refused while it stands.
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying_by(&job, SQUATTER, 3);
    let (sid, sda) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx]);
    assert_eq!(w.l.job_claims[&job.id()], sid, "the claim holds its job from its first reveal (no Panel coverage needed)");
    let honest = w.honest_by(&job, HONEST, 3);
    let ev = w.block(12, vec![honest.tx.clone()]);
    assert!(refused(&ev).unwrap().contains("holds the job") || refused(&ev).unwrap().contains("one claim per job"), "{ev:?}");

    // Any outsider convicts it; the squatter loses its whole reservation and the job is free again.
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, sid, &sda) else { panic!() };
    let ev = w.block(30, vec![T::FileProof { accuser: OUTSIDER, claim: sid, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "{ev:?}");
    assert_eq!(w.l.bonds[&SQUATTER].collateral, 3000 - 1000 - 3, "squatting cost the squatter its reservation and its admission fee");
    assert_eq!(w.consumer.paid(&OUTSIDER), 500);
    let ev = w.block(40, vec![honest.tx]);
    assert!(ev.contains(&E::ClaimCommitted { claim: honest.claim.id() }), "{ev:?}");
    assert_eq!(w.block(90, vec![]), vec![E::Final { claim: honest.claim.id(), reward: 7 }]);

    // A squatter that withholds is evicted by the very producer it blocks: that producer demands the material; the squatter's
    // default penalty goes to it and the job is free after the demand's deadline.
    let job = w.post_job(100, &[3, 17, 9], 3, 2);
    let (at, lie) = w.lying_by(&job, SQUATTER, 3);
    let sid = lie.claim.id();
    w.block(110, vec![lie.tx]);
    let honest = w.honest_by(&job, HONEST, 3);
    let ev = w.block(112, vec![honest.tx.clone()]);
    assert!(refused(&ev).is_some(), "still held: {ev:?}");
    let collateral = w.l.bonds[&SQUATTER].collateral;
    let paid = w.consumer.paid(&HONEST);
    w.block(113, vec![T::FileDemand { demander: HONEST, claim: sid, stage: 0, position: at.0 }]);
    let ev = w.block(133, vec![]);
    assert!(ev.iter().any(|e| matches!(e, E::ProducerDefault { penalty: 100, .. })), "{ev:?}");
    assert_eq!(w.l.bonds[&SQUATTER].collateral, collateral - 100, "the withholding squatter pays the default penalty");
    assert_eq!(w.consumer.paid(&HONEST), paid + 50, "to the producer that demanded (the burned half aside)");
    let ev = w.block(140, vec![honest.tx]);
    assert!(ev.contains(&E::ClaimCommitted { claim: honest.claim.id() }), "the job is free again: {ev:?}");
    assert_eq!(w.block(190, vec![]), vec![E::Final { claim: honest.claim.id(), reward: 7 }]);
}

// ── silence is never a pass, except the one explicit window rule ──────────────────────────────────────────────────────────

#[test]
fn empty_receipts_a_zero_quorum_and_a_missing_tally_never_count_as_checked() {
    // The Panel route's tally: no receipts and quorum 0 are not coverage.
    let c = common::Claim::honest();
    let ev = c.evidence_of(&c.trace);
    assert!(!ev.segments.is_empty());
    for quorum in [0u8, 1, 3] {
        let tally = ConstraintTallyV1::new(TallyPolicyV1 { per_segment_quorum: quorum }, &ev);
        assert!(matches!(tally.state(), TallyStateV1::Incomplete { .. }), "quorum {quorum} with no receipts is not Covered");
    }

    // A Panel-licensed claim in an OPV-enabled ledger still needs its tally: without one it times out and never passes.
    let mut w = World::new_opv();
    let legacy = w.register_panel_class(2);
    let job = w.with_class(legacy, |w| w.post_job(3, &[3, 17, 9], 3, 1));
    let h = w.with_class(legacy, |w| w.honest(&job, 3));
    let id = h.claim.id();
    w.block(10, vec![h.tx]);
    assert!(matches!(w.state(&id), ClaimStateV1::Checking { .. }));
    assert_eq!(w.l.mode_of_claim(&id), Some(Mode::PanelLicensed));
    let ev = w.block(111, vec![]);
    assert_eq!(ev, vec![E::TimedOut { claim: id }], "silence is a timeout, never a pass");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0);

    // An OPV claim has no tally to wait for: its Final is the explicit window rule, and nothing else can pass it.
    let job = w.post_job(200, &[3, 17, 9], 3, 2);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    w.block(210, vec![h.tx]);
    assert!(matches!(w.state(&id), ClaimStateV1::Challengeable { .. }));
    assert!(w.l.apply_panel_tally(&id, true).is_err() && w.l.apply_panel_tally(&id, false).is_err());
    assert_eq!(w.block(260, vec![]), vec![E::Final { claim: id, reward: 7 }]);
    let r = w.l.final_receipt(&id).unwrap();
    assert_eq!(r.assurance, FinalAssuranceV1::OptimisticPublicVerification);
}

// ── §8: the producer-centred economics ────────────────────────────────────────────────────────────────────────────────────

#[test]
fn the_reservation_is_the_policys_not_a_panel_lock_and_concurrent_claims_never_reuse_a_collateral() {
    // A reservation different from the Panel route's flat collateral, to tell them apart.
    let mut opv = opv_example();
    opv.economics.reservation_per_claim = 1500;
    let mut w = World::with_opv(policy(), opv);
    let legacy = w.register_panel_class(2);

    // A Panel-licensed claim reserves the flat collateral; an OPV claim reserves the OPV reservation, and nothing divides or locks
    // by any signer (there is none).
    let pj = w.with_class(legacy, |w| w.post_job(3, &[3, 17, 9], 3, 1));
    let p = w.with_class(legacy, |w| w.honest(&pj, 3));
    w.block(4, vec![p.tx]);
    assert_eq!(w.l.claims[&p.claim.id()].reserved, 1000);
    let mut ids = vec![];
    let mut txs = vec![];
    for nonce in 10..13u8 {
        let job = w.post_job(5 + nonce as u64, &[3, 17, 9], 3, nonce);
        let c = w.honest(&job, 3);
        ids.push(c.claim.id());
        txs.push(c.tx);
    }
    // PRODUCER (5000): the Panel claim holds 1000; two OPV claims of 1500 fit exactly (4000 of 5000), a third (5500) does not.
    let ev = w.block(30, txs);
    assert_eq!(ev.iter().filter(|e| matches!(e, E::ClaimCommitted { .. })).count(), 2, "{ev:?}");
    assert!(refused(&ev).unwrap().contains("no double use"), "{ev:?}");
    // (and the refused third claim's seal deposit, held until its seal is revealed or expires: bonded seals)
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 1000 + 1500 * 2 + 1);
    assert_eq!(w.l.claims[&ids[0]].reserved, 1500);
    assert_eq!(w.l.opv.claims[&ids[0]].reservation, 1500);
    assert!(!w.l.claims.contains_key(&ids[2]), "the refused claim left nothing behind");
    // The Panel claim, with no tally, times out and returns its reservation; the OPV claims are untouched by it.
    w.block(111, vec![]);
    // (the refused third claim's seal deposit is still held: that seal has not expired yet)
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 1500 * 2 + 1, "{:?}", w.l.bonds[&PRODUCER]);
}

#[test]
fn live_claim_caps_and_the_aggregate_gain_bound_a_producers_unsettled_exposure() {
    // 3 pre-Final claims per producer, 5 in total beyond each producer's first; reservation 1000, maximum gain 100, admission fee 3.
    let mut w = World::new_opv();
    // Post a job one block after the ledger's clock and commit the bond's honest claim one block after that.
    let place = |w: &mut World, nonce: u8, bond: Digest| -> Vec<E> {
        let t = w.l.daa + 1;
        let job = w.post_job(t, &[3, 17, 9], 3, nonce);
        let c = w.honest_by(&job, bond, 3);
        w.block(t + 1, vec![c.tx])
    };
    let producer0 = w.l.bonds[&PRODUCER].collateral;
    for nonce in 0..3 {
        let ev = place(&mut w, nonce, PRODUCER);
        assert!(refused(&ev).is_none(), "claim {nonce}: {ev:?}");
    }
    // A fourth claim of the same producer: refused by the cap although its collateral (5000) would allow it.
    let ev = place(&mut w, 3, PRODUCER);
    assert!(refused(&ev).unwrap().contains("pre-Final claims (cap 3)"), "{ev:?}");
    assert_eq!(w.l.opv_live_counts(&PRODUCER), (3, 3));
    assert_eq!(w.l.opv_unsettled_gain(&PRODUCER), 300, "the aggregate maximum gain: 3 claims of 100");
    assert!(w.l.opv_unsettled_gain(&PRODUCER) <= w.l.bonds[&PRODUCER].reserved as u128, "the gain is covered by what is reserved");
    // Every admission paid its non-refundable fee (C4 F-C4R3-05), burned.
    assert_eq!(w.l.bonds[&PRODUCER].collateral, producer0 - 3 * 3);
    // Two more from another producer reach the ledger's total (5). A producer with NO pre-Final claim is still admitted past it — no
    // set of bonds can hold the whole lane against another (C4 F-C4R3-05) — but its second claim is refused by the total.
    for nonce in 4..6 {
        let ev = place(&mut w, nonce, HONEST);
        assert!(refused(&ev).is_none(), "{ev:?}");
    }
    let ev = place(&mut w, 6, SQUATTER);
    assert!(refused(&ev).is_none(), "a fresh producer's first claim is admitted past the total: {ev:?}");
    let ev = place(&mut w, 8, SQUATTER);
    assert!(refused(&ev).unwrap().contains("pre-Final claims in the ledger"), "{ev:?}");
    assert_eq!(w.l.opv_open_counts(&SQUATTER), (1, 6));
    // A collateral is never used twice: the reservation must come out of FREE collateral.
    w.block(70, vec![T::RegisterBond { bond: HONEST, collateral: 2500 }]);
    assert_eq!(w.l.bonds[&HONEST].free(), 500);
    // Final (by 70) is not the end of the exposure — the reservations stay held through the liability horizon — but a Final claim
    // holds no admission slot: the producer is admitted again at once, its exposure bounded by its collateral (C4 F-C4R3-05).
    w.block(100, vec![]);
    assert_eq!(w.l.opv_live_counts(&PRODUCER), (3, 6), "Final is not the end of the exposure");
    assert_eq!(w.l.opv_open_counts(&PRODUCER), (0, 0), "but it is the end of the admission slot");
    let ev = place(&mut w, 7, PRODUCER);
    assert!(refused(&ev).is_none(), "{ev:?}");
    assert_eq!(w.l.opv_live_counts(&PRODUCER).0, 4);
    assert_eq!(w.l.opv_unsettled_gain(&PRODUCER), 400);
    // Once every horizon ends the exposure is gone; the fees stay burned (never refunded, whatever the claim became).
    w.block(160, vec![]); // the last claim's Final
    w.block(400, vec![]);
    assert_eq!(w.l.opv_live_counts(&PRODUCER), (0, 0));
    assert_eq!(w.l.opv_unsettled_gain(&PRODUCER), 0);
    // (and the refused fourth claim's seal, never revealed, forfeited its deposit: bonded seals)
    assert_eq!(w.l.bonds[&PRODUCER].collateral, producer0 - 4 * 3 - 1);
}

#[test]
fn a_collateral_another_subsystem_took_is_not_a_bounty_source_and_fake_fraud_and_fake_default_loops_cost_their_players() {
    // (a) The bounty is a share of the COLLECTED slash: a bond whose real collateral fell below the reservation pays 50% of what is
    // actually there, and the rest of the reservation is released, not "slashed" on paper.
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx]);
    w.block(11, vec![T::RegisterBond { bond: PRODUCER, collateral: 300 }]); // the consumer's real collateral fell
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(30, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((300, 150, false)), "nominal 1000, collected 300: {ev:?}");
    assert_eq!(w.consumer.paid(&OUTSIDER), 150);
    assert_eq!((w.l.bonds[&PRODUCER].collateral, w.l.bonds[&PRODUCER].reserved), (0, 0));

    // (b) A fake-fraud loop: the producer lies and its own Sybil convicts it. The pair gains nothing: the burned half is lost.
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let before = w.l.bonds[&PRODUCER].collateral + w.l.bonds[&SPAM1].collateral;
    w.block(30, vec![T::FileProof { accuser: SPAM1, claim: id, proof }]);
    let after = w.l.bonds[&PRODUCER].collateral + w.l.bonds[&SPAM1].collateral + w.consumer.paid(&SPAM1);
    assert_eq!(before - after, 500, "the colluding pair lost the burned half of the slash");

    // (c) A fake-default loop: the producer withholds and its Sybil demands. The burned part of the penalty is lost (a default is
    // split at least like a slash: 500 permille burned, C4 F-C4R3-02), and the claim earned nothing.
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let id = lie.claim.id();
    w.block(10, vec![lie.tx]);
    let before = w.l.bonds[&PRODUCER].collateral + w.l.bonds[&SPAM1].collateral;
    w.block(22, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: at.0 }]);
    w.block(42, vec![]);
    let after = w.l.bonds[&PRODUCER].collateral + w.l.bonds[&SPAM1].collateral + w.consumer.paid(&SPAM1);
    assert_eq!(before - after, 50);
    assert!(!w.l.claims[&id].rewarded);
}

// ── F-C4R3-05 (round 2): what N cheap Sybil bonds pay to hold the OPV lane ──────────────────────────────────────────────

/// **C4 F-C4R3-05, round 2 (the user's requirement: Sybil splitting must not defeat the cap).** A per-bond cap alone is defeated by
/// splitting across bonds, so the lane's capacity is built from four things together: a reservation per live claim (held through the
/// window AND the liability horizon), a non-refundable admission fee, slots that only PRE-FINAL claims hold, and a hard ceiling
/// (`max_live_claims_total + fresh_producer_slots`) the window's reserved proof runs bound — with every block's prosecution share
/// reserved for proofs. Here N = hard = 7 cheap Sybil bonds, each holding exactly one reservation + one fee, fill the whole lane: only
/// then is an honest new producer refused, and that cost them 7 fees burned and 7 reservations locked for window + liability. At Final
/// the slots free but the reservations stay locked, so the same cheap bonds cannot refill — the honest producer is admitted — and
/// holding the lane continuously costs `hard × reservation × (window + liability) / window` of locked collateral plus `hard × fee` per
/// window, however it is split across bonds.
#[test]
fn n_cheap_sybil_bonds_pay_for_every_slot_and_cannot_hold_the_opv_lane_past_one_window() {
    let p = opv_example();
    let e = p.economics;
    let hard = (e.max_live_claims_total + e.fresh_producer_slots) as usize;
    let mut w = World::new_opv();
    let sybils: Vec<Digest> = (0..hard).map(|i| [0xD0 + i as u8; 64]).collect();
    let cheap = e.reservation_per_claim + e.admission_fee; // exactly one claim's worth
    w.block(2, sybils.iter().map(|b| T::RegisterBond { bond: *b, collateral: cheap }).collect());
    let place = |w: &mut World, nonce: u8, bond: Digest| -> Vec<E> {
        let t = w.l.daa + 1;
        let job = w.post_job(t, &[3, 17, 9], 3, nonce);
        let c = w.honest_by(&job, bond, 3);
        w.block(t + 1, vec![c.tx])
    };
    let burned = w.l.burned;
    for (i, b) in sybils.iter().enumerate() {
        let ev = place(&mut w, i as u8, *b);
        assert!(refused(&ev).is_none(), "Sybil {i}: {ev:?}");
    }
    assert_eq!(w.l.opv_open_counts(&HONEST), (0, hard as u32), "the whole lane, total and fresh slots alike");
    // Only now is a new producer shut out — and the attacker paid for every slot.
    let ev = place(&mut w, 100, HONEST);
    assert!(refused(&ev).unwrap().contains("hard ceiling"), "{ev:?}");
    let fees = w.l.burned - burned - (hard as u64 + 1) * w.l.policy.job_fee; // (the jobs' own posting fees aside)
    let locked: u64 = sybils.iter().map(|b| w.l.bonds[b].reserved).sum();
    assert_eq!((fees, locked), (hard as u64 * e.admission_fee, hard as u64 * e.reservation_per_claim));
    // Bounded release: at Final the slots free, the reservations stay locked through the liability horizon — the cheap bonds cannot
    // refill, and the honest producer is admitted.
    let window_end = w.l.opv.claims.values().map(|o| o.window_end_daa).max().unwrap();
    w.block(window_end + 1, vec![]);
    assert_eq!(w.l.opv_open_counts(&HONEST), (0, 0), "no pre-Final claim holds a slot");
    assert_eq!(sybils.iter().map(|b| w.l.bonds[b].reserved).sum::<u64>(), locked, "but every reservation is still locked");
    let ev = place(&mut w, 101, sybils[0]);
    // (its bonded seal is refused for want of free collateral, so its reveal has no seal to stand on)
    assert!(refused(&ev).unwrap().contains("no seal of this claim"), "a cheap Sybil cannot refill: {ev:?}");
    assert_eq!(w.l.bonds[&sybils[0]].reserved, e.reservation_per_claim, "nothing more is reserved for it");
    let ev = place(&mut w, 102, HONEST);
    assert!(refused(&ev).is_none(), "the honest producer is admitted: {ev:?}");
    // The steady-state price of holding the lane, whoever and however many the bonds: (W + L) / W windows of reservations locked at
    // once, and a fee per slot per window.
    let (window, liability) = (p.window_daa(), w.l.policy.liability_daa);
    let steady_locked = hard as u64 * e.reservation_per_claim * (window + liability) / window;
    eprintln!(
        "[F-C4R3-05 Sybil] hard ceiling {hard}: {fees} burned and {locked} locked to hold one window; continuously: {steady_locked} \
         locked + {} burned per {window} DAA",
        hard as u64 * e.admission_fee
    );
    assert_eq!(steady_locked, 7 * 1000 * 5);
}

/// **C4 F-C4R3-05, round 2: prosecution room is always there.** With a four-run block, two runs are reserved for proofs: a flood of
/// claims in the same block takes only its share (the third claim is refused over budget), and the outsider's proof carried in the
/// same block is still adjudicated and convicts.
#[test]
fn a_flood_of_claims_never_takes_the_runs_reserved_for_proofs() {
    let mut narrow = policy();
    narrow.max_adjudications_per_block = 4;
    let mut w = World::with_opv(narrow, opv_example());
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let mut txs = Vec::new();
    for (nonce, bond) in [(2u8, HONEST), (3, SQUATTER), (4, SPAM1)] {
        let job = w.post_job(10 + nonce as u64, &[3, 17, 9], 3, nonce);
        txs.push(w.honest_by(&job, bond, 3).tx);
    }
    w.block(20, vec![T::RegisterBond { bond: SPAM1, collateral: 3000 }]);
    txs.push(T::FileProof { accuser: OUTSIDER, claim: id, proof });
    let ev = w.block(21, txs);
    assert_eq!(ev.iter().filter(|e| matches!(e, E::ClaimCommitted { .. })).count(), 2, "the admissions' share: two runs: {ev:?}");
    assert!(refused(&ev).unwrap().contains("reserved for proofs"), "{ev:?}");
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "the proof found its reserved run: {ev:?}");
}

// ── F-C4R3-02 on an OPV claim: a self-inflicted default never erases a provable fraud ─────────────────────────────────────

/// **C4 round 3, F-C4R3-02 (OPV, reference level)**: the producer's own bond demands at once and the producer stays silent; the
/// default burns at least a slash's share of the penalty, keeps the rest of the reservation through the default's horizon, and the
/// outsider's true proof filed after it convicts with the undiluted bounty and no fee.
#[test]
fn an_optimistic_self_inflicted_default_never_erases_a_provable_fraud() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("everything is published") };
    let producer0 = w.l.bonds[&PRODUCER].collateral;
    w.block(11, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 }]);
    let ev = w.block(31, vec![]);
    assert_eq!(ev, vec![E::ProducerDefault { claim: id, stage: 0, position: 0, last: None, penalty: 100 }]);
    assert_eq!((w.consumer.paid(&SPAM1), w.l.bonds[&PRODUCER].reserved), (50, 900));
    let outsider0 = w.l.bonds[&OUTSIDER].collateral;
    let ev = w.block(32, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((900, 500, false)), "{ev:?}");
    assert_eq!(w.l.bonds[&OUTSIDER].collateral, outsider0, "no fee for a true proof");
    assert_eq!(w.consumer.paid(&OUTSIDER), 500);
    assert_eq!(w.l.bonds[&PRODUCER].collateral, producer0 - 1000);
    assert_eq!(w.l.opv_live_counts(&PRODUCER).0, 0, "the conviction ended the reservation");
    assert!(w.l.final_receipt(&id).is_none(), "never Final");
}

// ── reorg, restart, replay ────────────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn replay_and_reorg_reach_the_same_state_with_no_double_reward_slash_or_clock_restart() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]));
    let trace = lie.trace.clone();
    w.block(10, vec![lie.tx]);
    let fork_point = w.blocks.len();
    // Branch A: a demand, its service, and the conviction it enables.
    w.block(22, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    w.block(30, vec![T::Respond { claim: id, stage: 0, position: at.0, bytes: position(&trace, at.0, |_| {}) }]);
    let da_full = Da::publishing(&trace, &[]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da_full) else { panic!() };
    let ev = w.block(31, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: proof.clone() }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)));
    // The same proof again, in this block and in a later one: one conviction, no second slash, no second bounty.
    let root_once = w.l.root();
    let burned = w.l.burned;
    let ev = w.block(31, vec![T::FileProof { accuser: SPAM1, claim: id, proof: proof.clone() }]);
    assert_eq!(ev, vec![E::Duplicate { claim: id }]);
    assert_eq!(w.l.root(), root_once, "a duplicate proof changes nothing");
    assert_eq!(w.l.burned, burned);
    let _ = da;

    // Restart / IBD: the state is the fold of the blocks from genesis, root included (the V2 root commits the OPV rows).
    let fresh = KernelLedgerV1::replay(&w.genesis, &w.blocks);
    assert_eq!(fresh.root(), w.l.root());
    assert_eq!(fresh.root_parts_v2(), w.l.root_parts_v2());
    assert_ne!(w.l.root(), w.l.root_parts().root(), "an OPV ledger commits the V2 root");
    fresh.opv_invariants().unwrap();
    // The derived live index is rebuilt from the rows.
    let mut rebuilt = fresh.clone();
    rebuilt.opv_rebuild_live();
    assert_eq!(rebuilt.opv.live_claims(), fresh.opv.live_claims());

    // Reorg: branch B shares the first blocks but has no demand and no proof; the state is a pure function of its own blocks, not of
    // anything an abandoned branch did.
    let mut branch_b = w.blocks[..fork_point].to_vec();
    branch_b.push(misaka_palw_kernel::ledger::LedgerBlockV1 { daa: 70, txs: vec![] });
    let b = KernelLedgerV1::replay(&w.genesis, &branch_b);
    assert_ne!(b.root(), w.l.root());
    assert!(matches!(b.claims[&id].life.state, ClaimStateV1::Final { final_daa: 70 }), "on branch B nobody challenged it");
    assert_eq!(b.claims[&id].reserved, 1000);
    // Applying B after A's blocks on a fresh genesis, in either order of construction, gives the same ledgers.
    let a_again = KernelLedgerV1::replay(&w.genesis, &w.blocks);
    assert_eq!(a_again.root(), w.l.root());
    assert_eq!(KernelLedgerV1::replay(&w.genesis, &branch_b).root(), b.root());
    b.opv_invariants().unwrap();
}

// ── Panel-licensed behaviour is unchanged ────────────────────────────────────────────────────────────────────────────────

fn panel_scenario(w: &mut World) -> Vec<Vec<E>> {
    let mut all = vec![];
    // A covered honest claim finalizes; a covered lying claim is convicted; a withholding claim defaults.
    let j1 = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&j1, 3);
    let (hid, _hda) = (h.claim.id(), Da::publishing(&h.trace, &[]));
    all.push(w.block(10, vec![h.tx, T::PanelCovered { claim: hid }]));
    let j2 = w.post_job(11, &[3, 17, 9], 3, 2);
    let (at, lie) = w.lying(&j2, 3);
    let (lid, lda) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    all.push(w.block(12, vec![lie.tx, T::PanelCovered { claim: lid }]));
    let OutsiderFindingV1::Prosecute(proof) = outsider(w, lid, &lda) else { panic!() };
    all.push(w.block(20, vec![T::FileProof { accuser: OUTSIDER, claim: lid, proof }]));
    let j3 = w.post_job(21, &[3, 17, 9], 3, 3);
    let (wat, wl) = w.lying(&j3, 3);
    let wid = wl.claim.id();
    all.push(w.block(22, vec![wl.tx, T::PanelCovered { claim: wid }]));
    all.push(w.block(30, vec![T::FileDemand { demander: SPAM1, claim: wid, stage: 0, position: wat.0 }]));
    all.push(w.block(60, vec![]));
    all.push(w.block(300, vec![]));
    let _ = at;
    all
}

#[test]
fn panel_licensed_classes_behave_exactly_as_before_whether_or_not_the_ledger_has_an_optimistic_policy() {
    // The same blocks on a ledger that never heard of OPV and on one carrying an OPV policy whose fence never activates.
    let mut never = opv_example();
    never.activation_daa = None;
    let mut plain = World::new();
    let mut with_policy = World::panel_world_with_opv(policy(), never);
    assert_eq!(plain.class, with_policy.class, "a Panel-licensed class id does not depend on the OPV policy");
    let a = panel_scenario(&mut plain);
    let b = panel_scenario(&mut with_policy);
    assert_eq!(a, b, "every receipt is the same");
    assert!(a.iter().flatten().any(|e| matches!(e, E::Final { .. })) && a.iter().flatten().any(|e| matches!(e, E::Convicted { .. })));
    assert!(a.iter().flatten().any(|e| matches!(e, E::ProducerDefault { .. })));
    // Every collection of the historical state is identical; only the root form differs (the V2 root wraps the same V1 root).
    assert_eq!(plain.l.root_parts(), with_policy.l.root_parts());
    assert_eq!(plain.l.root(), plain.l.root_parts().root(), "no OPV policy: the historical root, byte for byte");
    assert_eq!(with_policy.l.root_parts_v2().v1, plain.l.root());
    assert_ne!(with_policy.l.root(), plain.l.root());
    assert_eq!(plain.consumer.settlements, with_policy.consumer.settlements, "every settlement instruction is the same");
    assert!(plain.l.opv.is_dormant() && !with_policy.l.opv.is_dormant());
    assert!(with_policy.l.opv.classes.is_empty() && with_policy.l.opv.claims.is_empty());
}

// ── RFC-0010: a Panel-independent Final is a source a Panel-assignment beacon can use ──────────────────────────────────────

#[test]
fn an_optimistic_final_is_a_panel_independent_beacon_source_and_a_panel_licensed_one_is_not() {
    let mut w = World::new_opv();
    let legacy = w.register_panel_class(2);
    // One Final of each mode.
    let job = w.post_job(3, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let oid = h.claim.id();
    w.block(10, vec![h.tx]);
    let pj = w.with_class(legacy, |w| w.post_job(11, &[3, 17, 9], 3, 2));
    let p = w.with_class(legacy, |w| w.honest(&pj, 3));
    let pid = p.claim.id();
    w.block(12, vec![p.tx, T::PanelCovered { claim: pid }]);
    w.block(60, vec![]);
    w.block(62, vec![]);
    let (o, p) = (w.l.final_receipt(&oid).unwrap(), w.l.final_receipt(&pid).unwrap());
    assert_eq!((o.mode, p.mode), (OPV, Mode::PanelLicensed));
    assert_eq!((o.final_daa, p.final_daa), (60, 62));

    let policy = reference_policy_v1(1, 1, 100, 1, 4);
    let context = |kind: SubjectKindV1| BeaconContextV1 {
        chain_genesis: [9; 64],
        ruleset_id: [3; 64],
        policy: policy.clone(),
        subject_kind: kind,
        commitment_root: [1; 64],
        commitment_position: 0,
        challenge_epoch: 0,
        eligible_profiles: [w.class, legacy].into_iter().collect(),
        excluded_profiles: Default::default(),
        candidate_profile_id: RootV1::Absent,
    };
    let ev_o = o.to_work_final_event(&ctx(None)).unwrap();
    let ev_p = p.to_work_final_event(&ctx(Some(([7; 64], 4)))).unwrap();
    assert_eq!(ev_o.final_path, FinalPathV1::PanelIndependent);
    assert_eq!(ev_p.final_path, FinalPathV1::PanelLicensed { panel_seed_id: [7; 64], panel_epoch: 4 });
    // A Panel-assignment beacon takes the OPV Final and refuses the Panel-licensed one (the circularity RFC-0010 closes).
    let ctx_a = context(SubjectKindV1::PanelAssignment);
    assert_eq!(eligibility_v1(&ctx_a, &ev_o), Ok(()));
    assert_eq!(eligibility_v1(&ctx_a, &ev_p), Err(IneligibleV1::PanelDependentFinal));
    // Other subjects may use either.
    let ctx_c = context(SubjectKindV1::ClaimVerification);
    assert_eq!(eligibility_v1(&ctx_c, &ev_o), Ok(()));
    assert_eq!(eligibility_v1(&ctx_c, &ev_p), Ok(()));
    // A licence is never attached to an OPV Final, and a Panel-licensed Final never claims independence.
    assert!(o.to_work_final_event(&ctx(Some(([7; 64], 4)))).is_err());
    assert!(p.to_work_final_event(&ctx(None)).is_err());
    // The canonical order of every Final in the ledger is a function of state: (final DAA, work id, claim).
    let all = w.l.final_receipts();
    assert_eq!(all.iter().map(|r| r.claim).collect::<Vec<_>>(), vec![oid, pid]);
}

// ── the wire ──────────────────────────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn route_tag_13_is_the_mode_carrying_registration_and_an_unknown_mode_is_never_success() {
    let w = World::new_opv();
    let txs = register_in(&w, OPV, PRODUCER);
    let misaka_palw_kernel::ledger::LedgerTxV1::Object { object, .. } = txs.last().unwrap().clone() else { unreachable!() };
    assert_eq!(object.tag(), TAG_REGISTER_CLASS_V2);
    assert_eq!(object.name(), "RegisterClassV2");
    let bytes = object.encode();
    assert_eq!(&bytes[..3], &[KERNEL_ROUTE_VERSION_V1, 13, 1], "version, tag, the mode's declared discriminant");
    assert_eq!(O::decode(&bytes).unwrap(), object);
    assert!(bytes.len() <= max_encoded_bytes_of_tag(13).unwrap());
    // An unknown mode, a Panel-licensed mode in a re-encoded form (still decodes — the ledger refuses it), trailing bytes.
    let mut unknown = bytes.clone();
    unknown[2] = 2;
    assert_eq!(O::decode(&unknown).unwrap_err().kind, RefusalKindV1::Malformed);
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(O::decode(&trailing).unwrap_err().kind, RefusalKindV1::Malformed);
    let mut panel = bytes;
    panel[2] = 0;
    let O::RegisterClassV2 { mode, .. } = O::decode(&panel).unwrap() else { panic!() };
    assert_eq!(mode, Mode::PanelLicensed);
    // A node that has not reached the fence has no policy: the decoded object is refused by the ledger, not silently accepted.
    let mut dormant = World::new();
    dormant.block_raw(2, vec![txs[0].clone()]);
    let o = O::decode(&panel).unwrap();
    let r = dormant.l.apply_object(&o, &auth(PRODUCER)).unwrap_err();
    assert_eq!(r.kind, RefusalKindV1::Rule);
}

/// The golden vector of the OPV root form (a hand-built state, no fixtures): a change of the encoding of the policy, the OPV class
/// set or the OPV claim rows is a new root version.
#[test]
fn the_optimistic_state_root_form_is_versioned_and_pinned_by_a_golden_vector() {
    let d = k2_tir_v1_descriptor();
    let small = |opv: Option<OpvPolicyV1>| {
        let mut l = KernelLedgerV1::genesis(policy(), common::active_for(&d), vec![d.clone()]).unwrap();
        if let Some(p) = opv {
            l = l.with_opv_policy(p).unwrap();
        }
        l.begin_block(7).unwrap();
        l.sync_bond([1; 64], 5000);
        l.attest_artifact([3; 64]);
        l
    };
    let dormant = small(None);
    let opv = small(Some(opv_example()));
    assert_eq!(opv.root_parts(), dormant.root_parts(), "the historical collections are the same");
    let parts: StateRootPartsV2 = opv.root_parts_v2();
    assert_eq!(parts.version, 2);
    assert_eq!(parts.v1, dormant.root());
    assert_eq!(opv.root(), parts.root());
    assert_ne!(opv.root(), dormant.root());
    assert_eq!(
        [parts.opv_policy, parts.opv_admitted, parts.opv_classes, parts.opv_claims].map(|x| hex(&x[..8])),
        GOLDEN_OPV_PARTS,
        "a change of the canonical encoding of the OPV state is a new root version"
    );
    assert_eq!(hex(&opv.root()), GOLDEN_OPV_ROOT);
    // OPV-BOOT GAP-B1a: with tables 25 and 26 empty the root is the OPV form itself (no extension).
    assert!(opv.claim_beacon_salts.is_empty() && opv.forfeited_claim_seals.is_empty() && opv.job_posters.is_empty());
    // Any change of the policy is a different root.
    let mut other = opv_example();
    other.economics.reservation_per_claim += 1;
    assert_ne!(small(Some(other)).root(), opv.root());
}

/// The policy part moved with the G14-R4 fix of F-C4R3-05 (`OpvEconomicsV1::admission_fee`, `fresh_producer_slots`).
const GOLDEN_OPV_PARTS: [&str; 4] = ["c80c0cd94e6c9465", "557931bf322541c3", "b018a938b23ad4fb", "3fbfb5902dada353"];
/// Moved with the G14-R4 fixes: GAP-R7 and GAP-5 (the historical root inside it gained the proof-seal and job-escrow collections and
/// its policy the job fee and escrow TTL) and F-C4R3-05 (the OPV policy's admission fee).
const GOLDEN_OPV_ROOT: &str =
    "2be2a83576f67a33ef9a441d2d9dd263b14b63b7ef140080e9e5cd30b003e79a178dc93552523b394fe367cdd93fb54a32b3f290fb0d607e4c7079236fc603e5";

// ── OPV-BOOT GAP-B1a: the salted claim seal (claim seal v2) past `palw_panel_free_v1` ─────────────────────────────────────────────

/// The rows of `l` root like `l`, and rebuild a ledger that roots like it (tables 25 and 26 included).
fn rows_agree(l: &KernelLedgerV1) {
    let rows = l.to_rows();
    let from_rows = misaka_palw_kernel::rows::root_of_rows(&l.policy, l.config_root(), l.scalars(), l.opv.policy.as_ref(), &rows);
    assert_eq!(from_rows, l.root(), "the rows root like the ledger");
    let rebuilt = KernelLedgerV1::from_rows(l, l.scalars(), &rows).unwrap();
    assert_eq!(rebuilt.root(), l.root(), "and rebuild it");
    assert_eq!(
        (rebuilt.claim_beacon_salts.clone(), rebuilt.forfeited_claim_seals.clone()),
        (l.claim_beacon_salts.clone(), l.forfeited_claim_seals.clone())
    );
}

/// **Past `palw_panel_free_v1` a claim opens only its SALTED seal, and the ledger keeps the salt for the sealed-source beacon v3**
/// (OPV-BOOT GAP-B1a). A deterministic class's claim is a function of its public job and its producer, so `claim_seal_v1(claim id)`
/// hides nothing: anyone who runs the model computes it the moment the seal is posted. `claim_seal_v2(id, salt)` with a secret
/// 64-byte salt does hide it, and the salt is revealed atomically with the claim (`CommitClaimSalted`, inner kind 20). An unsalted
/// reveal of a seal accepted past the fence is refused (a sealer must not choose, after seeing the honest salts, between "revealed
/// but no beacon source" and a veto); a salt that does not open the seal is refused; nothing is committed by either.
#[test]
fn past_the_panel_free_fence_a_claim_opens_only_its_salted_seal_and_the_salt_is_kept_for_the_beacon() {
    use misaka_palw_kernel::ledger::{ClaimBeaconSealV1, LedgerTxV1, SaltedCommitV1, claim_seal_v1, claim_seal_v2};
    let mut w = World::new_opv();
    assert!(w.l.salted_seals_in_force() && w.l.salted_seals_from() == Some(0));
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    let (auth, plain) = h.tx.clone().signed(PRODUCER);
    let O::CommitClaim { claim, evidence, commitments } = plain.clone() else { unreachable!() };
    let salted = |salt: Digest| O::CommitClaimSalted {
        salt,
        commit: SaltedCommitV1::Claim { claim: claim.clone(), evidence: evidence.clone(), commitments: commitments.clone() },
    };
    let obj = |object: O| LedgerTxV1::Object { auth, object };
    let seal = |s: Digest| obj(O::SealClaim { producer: PRODUCER, job: job.id(), seal: s });
    // (1) A v1 seal: its unsalted reveal is refused past the fence, and no salt opens it.
    w.block_raw(4, vec![seal(claim_seal_v1(&id))]);
    let ev = w.block_raw(6, vec![obj(plain.clone())]);
    assert!(refused(&ev).unwrap().contains("revealed only with its salt"), "{ev:?}");
    let ev = w.block_raw(7, vec![obj(salted([1; 64]))]);
    assert!(refused(&ev).unwrap().contains("salt does not open"), "{ev:?}");
    // (2) A v2 seal — a re-seal past the fence, so the v1 seal it replaces is FORFEITED at its own position (ECON fix S1) and the
    // v2 seal is bonded afresh: another salt does not open it, and neither does an unsalted reveal.
    let salt = [0x5A; 64];
    let collateral = w.l.bonds[&PRODUCER].collateral;
    w.block_raw(8, vec![seal(claim_seal_v2(&id, &salt))]);
    assert_eq!(w.l.bonds[&PRODUCER].reserved, w.l.policy.seal_deposit, "the new seal's deposit (the old one was forfeited)");
    assert_eq!(w.l.bonds[&PRODUCER].collateral, collateral - w.l.policy.seal_deposit, "the replaced seal's deposit burned");
    assert_eq!(w.l.forfeited_claim_seals[&(job.id(), PRODUCER, 4)].seal, claim_seal_v1(&id), "and its position kept");
    let ev = w.block_raw(10, vec![obj(salted([0x5B; 64]))]);
    assert!(refused(&ev).unwrap().contains("salt does not open"), "{ev:?}");
    let ev = w.block_raw(10, vec![obj(plain)]);
    assert!(refused(&ev).unwrap().contains("revealed only with its salt"), "{ev:?}");
    assert!(!w.l.claims.contains_key(&id) && w.l.claim_beacon_salts.is_empty(), "nothing committed, nothing kept");
    // (3) The salted reveal commits, over the seal's DAA, and the salt is kept.
    let ev = w.block_raw(11, vec![obj(salted(salt))]);
    assert!(ev.contains(&E::ClaimCommitted { claim: id }), "{ev:?}");
    assert_eq!((w.l.claim_beacon_salt(&id), w.l.claims[&id].sealed_daa, w.l.claims[&id].committed_daa), (Some(salt), 8, 11));
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 1000, "the seal's deposit returned at the reveal; the claim's reservation alone");
    assert_eq!(
        w.l.claim_beacon_seals_v1(),
        vec![
            ClaimBeaconSealV1 {
                job: job.id(),
                producer: PRODUCER,
                seal: claim_seal_v1(&id),
                sealed_daa: 4,
                revealed: None,
                forfeited_daa: Some(8),
                poster: Some(common::chain::POSTER),
            },
            ClaimBeaconSealV1 {
                job: job.id(),
                producer: PRODUCER,
                seal: claim_seal_v2(&id, &salt),
                sealed_daa: 8,
                revealed: Some((id, 11, salt)),
                forfeited_daa: None,
                poster: Some(common::chain::POSTER),
            },
        ]
    );
    // C4R4 F-C4R4-08: the job's poster is kept past the fence (the beacon's consumer), beyond the escrow that also names it.
    assert_eq!(w.l.job_poster(&job.id()), Some(common::chain::POSTER));
    // The salt is state: in the root (one extension over tables 25 and 26), in the rows, and rebuilt from them.
    assert_ne!(w.l.root(), w.l.root_parts_v2().root(), "the extension is present once a salt is kept");
    rows_agree(&w.l);
    // A replay from genesis reaches the same state.
    assert_eq!(KernelLedgerV1::replay(&w.genesis, &w.blocks).root(), w.l.root());
    // The harness's own path (every reveal salted past the fence) finalizes like any OPV claim.
    let ev = w.block(61, vec![]);
    assert_eq!(ev, vec![E::Final { claim: id, reward: 7 }]);
    assert!(!w.l.job_escrows.contains_key(&job.id()), "the escrow is spent at Final");
    assert_eq!(w.l.job_poster(&job.id()), Some(common::chain::POSTER), "and the poster is still on record");
    rows_agree(&w.l);
}

/// **Below `palw_panel_free_v1` a salted reveal rides unjudged** (the A-2 rule): no OPV policy, no salted-seal rule — the ledger
/// refuses inner kind 20 and leaves its state byte-identical (the consumer checks it), and the historical unsalted path is intact.
#[test]
fn below_the_panel_free_fence_a_salted_reveal_is_refused_and_changes_nothing() {
    use misaka_palw_kernel::ledger::{LedgerTxV1, SaltedCommitV1, claim_seal_v2};
    let mut w = World::new();
    assert!(!w.l.salted_seals_in_force() && w.l.salted_seals_from().is_none());
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    let (auth, plain) = h.tx.clone().signed(PRODUCER);
    let O::CommitClaim { claim, evidence, commitments } = plain else { unreachable!() };
    let salt = [0x5A; 64];
    let reveal = O::CommitClaimSalted { salt, commit: SaltedCommitV1::Claim { claim, evidence, commitments } };
    w.block_raw(
        4,
        vec![LedgerTxV1::Object { auth, object: O::SealClaim { producer: PRODUCER, job: job.id(), seal: claim_seal_v2(&id, &salt) } }],
    );
    // (the consumer asserts that the refusal leaves the root byte-identical)
    let ev = w.block_raw(6, vec![LedgerTxV1::Object { auth, object: reveal }]);
    assert!(refused(&ev).unwrap().contains("palw_panel_free_v1 is not in force"), "{ev:?}");
    assert!(!w.l.claims.contains_key(&id) && w.l.claim_beacon_salts.is_empty());
    // The historical path is untouched: the harness seals v1 and reveals unsalted.
    let ev = w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    assert!(ev.contains(&E::ClaimCommitted { claim: id }), "{ev:?}");
    assert!(
        w.l.claim_beacon_salts.is_empty() && w.l.forfeited_claim_seals.is_empty() && w.l.job_posters.is_empty(),
        "no salt, no row, no poster: the historical root"
    );
    assert_eq!(w.l.root(), w.l.root_parts().root());
}

/// **A seal accepted past the fence that expires unrevealed stays readable** (table 26), so a withheld seal stays in the v3
/// beacon's mix and vetoes it instead of silently dropping out of it (SOUND SG-01a(i)). A re-seal counts at its LATEST seal. The
/// beacon read lists live, revealed and forfeited seals together in `(sealed_daa, seal)` order. And the v3 window fits the TTL:
/// `2·W ≤ seal_ttl_daa` (OPV-BOOT's interim W = 40 against the interim TTL of 100).
#[test]
fn past_the_fence_a_withheld_seal_is_kept_as_forfeited_and_the_beacon_read_lists_every_seal_in_seal_order() {
    use misaka_palw_kernel::ledger::{ForfeitedSealRowV1, claim_seal_v2, seal_ttl_admits_beacon_window_v1};
    let mut w = World::new_opv();
    let ttl = w.l.policy.seal_ttl_daa;
    assert!(seal_ttl_admits_beacon_window_v1(&w.l.policy, 40) && seal_ttl_admits_beacon_window_v1(&w.l.policy, ttl / 2));
    assert!(!seal_ttl_admits_beacon_window_v1(&w.l.policy, ttl / 2 + 1));
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let withheld = |n: u8| claim_seal_v2(&[n; 64], &[0x77; 64]);
    // SQUATTER seals at 20 and re-seals at 30 (only the latest counts); HONEST seals at 25. Neither reveals.
    w.block(20, vec![T::SealClaim { producer: SQUATTER, job: job.id(), seal: withheld(1) }]);
    w.block(25, vec![T::SealClaim { producer: HONEST, job: job.id(), seal: withheld(2) }]);
    w.block(30, vec![T::SealClaim { producer: SQUATTER, job: job.id(), seal: withheld(3) }]);
    let live = w.l.claim_beacon_seals_v1();
    assert_eq!(
        live.iter().map(|s| (s.producer, s.sealed_daa, s.seal)).collect::<Vec<_>>(),
        vec![(HONEST, 25, withheld(2)), (SQUATTER, 30, withheld(3))]
    );
    assert!(live.iter().all(|s| s.revealed.is_none() && s.forfeited_daa.is_none()), "live seals");
    // HONEST's expires first (25 + TTL), SQUATTER's latest seal later (30 + TTL): both are kept, with their seal and DAA.
    let ev = w.block(25 + ttl + 1, vec![]);
    assert!(ev.iter().any(|e| matches!(e, E::SealForfeited { producer, .. } if *producer == HONEST)), "{ev:?}");
    w.block(30 + ttl + 1, vec![]);
    assert!(w.l.seals.is_empty());
    assert_eq!(
        w.l.forfeited_claim_seals.iter().map(|(k, r)| (*k, *r)).collect::<Vec<_>>(),
        vec![
            ((job.id(), HONEST, 25), ForfeitedSealRowV1 { seal: withheld(2), forfeited_daa: 25 + ttl + 1 }),
            ((job.id(), SQUATTER, 30), ForfeitedSealRowV1 { seal: withheld(3), forfeited_daa: 30 + ttl + 1 }),
        ],
        "both withheld seals are kept (HONEST's key sorts first), the re-sealed one at its latest seal"
    );
    // An honest claim revealed salted afterwards sorts after them; every seal is in the read, in seal order.
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    let at = 30 + ttl + 5;
    let ev = w.block(at, vec![h.tx]);
    assert!(ev.contains(&E::ClaimCommitted { claim: id }), "{ev:?}");
    let read = w.l.claim_beacon_seals_v1();
    assert_eq!(
        read.iter().map(|s| (s.sealed_daa, s.forfeited_daa.is_some(), s.revealed.is_some())).collect::<Vec<_>>(),
        vec![(25, true, false), (30, true, false), (30 + ttl + 1, false, true),]
    );
    assert_eq!(read[2].revealed, Some((id, at, common::chain::test_salt(&id))));
    assert!(read.iter().all(|s| s.poster == Some(common::chain::POSTER)), "every seal reads its job's poster");
    rows_agree(&w.l);
    assert_eq!(KernelLedgerV1::replay(&w.genesis, &w.blocks).root(), w.l.root());
}

/// **ECON F-ECON-1 / F-ECON-2, fix S1: past the fence a seal position, once taken, is never withdrawn for free.** A re-seal of a live
/// `(job, producer)` FORFEITS the replaced seal at its own `sealed_daa` (table 26, its deposit burned) and bonds the new one afresh.
/// (1) "One deposit vetoes every attempt" is impossible: staying live by re-sealing under the TTL costs a deposit per re-seal, and
/// every earlier position stays in the beacon read. (2) A re-seal inside a reveal window does not move a mixed seal out of its seal
/// window: the replaced seal stays at its position, unrevealed and forfeited — a counted veto, never a silent withdrawal.
#[test]
fn past_the_fence_a_re_seal_forfeits_the_replaced_seal_at_its_position_so_no_seal_is_withdrawn_for_free() {
    use misaka_palw_kernel::ledger::claim_seal_v2;
    let mut w = World::new_opv();
    let d = w.l.policy.seal_deposit;
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let seal = |n: u8| claim_seal_v2(&[n; 64], &[0x77; 64]);
    let (collateral, burned) = (w.l.bonds[&SQUATTER].collateral, w.l.burned);
    // (1) Seal at 10, then re-seal every 83 DAA (under the TTL): each re-seal forfeits the seal it replaces.
    let positions = [10u64, 93, 176, 259];
    for (i, at) in positions.iter().enumerate() {
        let ev = w.block(*at, vec![T::SealClaim { producer: SQUATTER, job: job.id(), seal: seal(i as u8) }]);
        assert!(refused(&ev).is_none(), "{ev:?}");
        if i > 0 {
            assert!(ev.contains(&E::SealForfeited { job: job.id(), producer: SQUATTER, forfeited: d }), "re-seal {i}: {ev:?}");
        }
    }
    let replaced = (positions.len() - 1) as u64;
    assert_eq!(w.l.bonds[&SQUATTER].collateral, collateral - replaced * d, "one deposit burned per replaced seal");
    assert_eq!(w.l.burned - burned, replaced * d);
    assert_eq!(w.l.bonds[&SQUATTER].reserved, d, "and one live deposit");
    let read = w.l.claim_beacon_seals_v1();
    let mine: Vec<(u64, bool)> =
        read.iter().filter(|s| s.producer == SQUATTER).map(|s| (s.sealed_daa, s.forfeited_daa.is_some())).collect();
    assert_eq!(mine, vec![(10, true), (93, true), (176, true), (259, false)], "every position stays in the read");
    assert!(read.iter().filter(|s| s.producer == SQUATTER).all(|s| s.revealed.is_none()));
    // (2) A beacon whose seal window held the seal at 93 (say [90, 130)) sees it unrevealed in its reveal window [130, 170) — it was
    // replaced at 176, after that window, but ANY replacement leaves it at 93: forfeited, never revealed — a veto, never an exclusion.
    let at_93 = read.iter().find(|s| s.producer == SQUATTER && s.sealed_daa == 93).unwrap();
    assert_eq!((at_93.seal, at_93.forfeited_daa, at_93.revealed), (seal(1), Some(176), None));
    // A re-seal INSIDE a reveal window: seal at 300, re-seal at 345 (window [300, 340), reveals [340, 380)) — the seal at 300 stays.
    w.block(300, vec![T::SealClaim { producer: HONEST, job: job.id(), seal: seal(9) }]);
    w.block(345, vec![T::SealClaim { producer: HONEST, job: job.id(), seal: seal(10) }]);
    let read = w.l.claim_beacon_seals_v1();
    let at_300 = read.iter().find(|s| s.producer == HONEST && s.sealed_daa == 300).expect("the replaced seal is still read");
    assert_eq!((at_300.seal, at_300.forfeited_daa, at_300.revealed), (seal(9), Some(345), None), "unrevealed: a counted veto");
    // Below the fence the historical rule stands (one deposit kept): pinned by k2_ledger_route's bonded-seal test.
    rows_agree(&w.l);
    assert_eq!(KernelLedgerV1::replay(&w.genesis, &w.blocks).root(), w.l.root());
}
