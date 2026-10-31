//! Actual kernel reservations and settlement-book conservation, with private consumer-derived targets.
//! This is not node activation or a source-only beacon/security approval.
#![allow(warnings)]
mod common;

use common::chain::{T, test_salt};
use common::ledger_world::*;
use misaka_palw_challenge::{BeaconContextV1, RootV1, SubjectKindV1, sealed_source_policy_v3};
use misaka_palw_kernel::beacon_source::{BeaconSourceInputsV1, BeaconSourceOutcomeV1, BeaconSourceTargetV1, BeaconSourceTermsV1};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{AuthV1, KernelLedgerV1, KernelRouteObjectV1 as O, LedgerEventV1 as E, LedgerTxV1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::rows::{TABLE_BEACON_SOURCES_V1, diff_rows, root_of_rows};

fn target(w: &World, id: u8, commitment: u64, width: u64) -> BeaconSourceTargetV1 {
    let context = BeaconContextV1 {
        chain_genesis: w.l.policy.network_domain,
        ruleset_id: w.l.policy.ruleset_digest,
        policy: sealed_source_policy_v3(2, 2, width, 1, 1),
        subject_kind: SubjectKindV1::ModelConformance,
        commitment_root: [id; 64],
        commitment_position: commitment,
        challenge_epoch: 0,
        eligible_profiles: [w.class].into_iter().collect(),
        excluded_profiles: Default::default(),
        candidate_profile_id: RootV1::Present([0xEF; 64]),
    };
    let terms = BeaconSourceTermsV1 {
        context: context.id(),
        seal_start: context.start(),
        seal_end: context.window_end(),
        reveal_end: context.window_end() + width,
        final_by: 180,
        hold_until: 400,
        deposit: 100,
        max_per_producer: 2,
        max_total: 3,
    };
    BeaconSourceTargetV1 { context, terms }
}

fn inputs(w: &mut World, targets: &[BeaconSourceTargetV1], fence: Option<u64>) {
    w.l.set_beacon_source_inputs(BeaconSourceInputsV1 {
        activation_daa: fence,
        targets: targets.iter().cloned().map(|t| (t.terms.context, t)).collect(),
    })
    .unwrap();
}

fn opt(producer: Digest, job: Digest, seal: Digest, t: &BeaconSourceTargetV1) -> O {
    O::OptInBeaconSource { producer, job, seal, context: t.terms.context, terms: t.terms.id() }
}

fn submit(w: &mut World, daa: u64, signer: Digest, object: O) -> Vec<E> {
    w.block_raw(daa, vec![LedgerTxV1::Object { auth: AuthV1 { signer_bond: signer }, object }])
}

fn reveal_object(tx: T) -> O {
    let T::CommitClaim { claim, evidence, commitments } = tx else { panic!() };
    O::CommitClaimSalted {
        salt: test_salt(&claim.id()),
        commit: misaka_palw_kernel::ledger::SaltedCommitV1::Claim { claim, evidence, commitments },
    }
}

fn refusal_unchanged(w: &mut World, signer: Digest, object: &O) {
    let before = w.l.to_rows();
    assert!(w.l.apply_object(object, &AuthV1 { signer_bond: signer }).is_err());
    assert_eq!(w.l.to_rows(), before, "refusal changes no authoritative row");
}

fn roundtrip(l: &KernelLedgerV1) -> KernelLedgerV1 {
    let rows = l.to_rows();
    assert_eq!(
        root_of_rows(
            &l.policy,
            misaka_palw_kernel::rows::config_root_of(&l.schedule, &l.known),
            l.scalars(),
            l.opv.policy.as_ref(),
            &rows
        ),
        l.root()
    );
    let restored = KernelLedgerV1::from_rows(l, l.scalars(), &rows).unwrap();
    assert_eq!(restored.to_rows(), rows);
    assert_eq!(restored.root(), l.root());
    assert!(restored.beacon_source_inputs.activation_daa.is_none());
    assert!(restored.beacon_source_inputs.targets.is_empty());
    restored
}

#[test]
fn opt_in_is_signed_target_bound_and_closed_without_its_own_fence() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let p = w.honest(&job, 3);
    let seal = misaka_palw_kernel::route::claim_seal_v2(&p.claim.id(), &test_salt(&p.claim.id()));
    w.block(12, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal }]);
    let t = target(&w, 1, 10, 40);
    let object = opt(PRODUCER, job.id(), seal, &t);
    let encoded = object.encode();
    assert_eq!(&encoded[..2], &[1, 40]);
    assert_eq!(encoded.len(), 322);
    assert_eq!(O::decode(&encoded).unwrap(), object);
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(O::decode(&trailing).is_err());
    let oversized = vec![1u8; 385];
    let mut oversized = oversized;
    oversized[1] = 40;
    assert!(O::decode(&oversized).is_err());
    refusal_unchanged(&mut w, PRODUCER, &object);
    inputs(&mut w, std::slice::from_ref(&t), None);
    refusal_unchanged(&mut w, PRODUCER, &object);
    inputs(&mut w, std::slice::from_ref(&t), Some(13));
    refusal_unchanged(&mut w, PRODUCER, &object);
    inputs(&mut w, std::slice::from_ref(&t), Some(11));
    refusal_unchanged(&mut w, PRODUCER, &object); // target predates the new fence
    inputs(&mut w, std::slice::from_ref(&t), Some(0));
    let empty_root = w.l.root();
    let empty_rows = w.l.to_rows();
    let empty = roundtrip(&w.l);
    assert_eq!(empty.root(), empty_root);
    assert_eq!(empty.to_rows(), empty_rows);
    refusal_unchanged(&mut w, OUTSIDER, &object);
    let mut wrong = t.clone();
    wrong.terms.deposit += 1;
    refusal_unchanged(&mut w, PRODUCER, &opt(PRODUCER, job.id(), seal, &wrong));
    let mut bad = w.l.beacon_source_inputs.clone();
    bad.targets.get_mut(&t.terms.context).unwrap().context.challenge_epoch += 1;
    assert!(w.l.set_beacon_source_inputs(bad).is_err());
    assert!(!submit(&mut w, 12, PRODUCER, object.clone()).iter().any(|e| matches!(e, E::Refused { .. })));
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 101);
    refusal_unchanged(&mut w, PRODUCER, &object);
    roundtrip(&w.l);
}

#[test]
fn every_target_costs_a_fresh_deposit_and_failure_keeps_the_slot_until_its_horizon() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 2);
    let seal = [0xA5; 64];
    w.block(30, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal }]);
    let ts = [target(&w, 1, 0, 40), target(&w, 2, 10, 40), target(&w, 3, 25, 40)];
    inputs(&mut w, &ts, Some(0));
    for t in &ts[..2] {
        assert!(refused(&submit(&mut w, 30, PRODUCER, opt(PRODUCER, job.id(), seal, t))).is_none());
    }
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 201, "one ordinary deposit plus two independent source deposits");
    refusal_unchanged(&mut w, PRODUCER, &opt(PRODUCER, job.id(), seal, &ts[2]));
    let burned = w.l.burned;
    w.block(92, vec![]);
    assert_eq!(w.l.burned - burned, 200, "both withheld opt-ins burn separately");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 1, "ordinary seal is still distinct");
    assert!(w.l.beacon_sources.values().all(|s| s.held == 0 && matches!(s.outcome, Some(BeaconSourceOutcomeV1::Forfeited { .. }))));
    let next = target(&w, 4, 90, 40);
    inputs(&mut w, std::slice::from_ref(&next), Some(0));
    w.block(102, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal: [0xB5; 64] }]);
    refusal_unchanged(&mut w, PRODUCER, &opt(PRODUCER, job.id(), [0xB5; 64], &next));
    let late = target(&w, 5, 400, 40);
    let mut late = late;
    late.terms.final_by = 580;
    late.terms.hold_until = 800;
    inputs(&mut w, &[late.clone()], Some(0));
    w.block(402, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal: [0xC5; 64] }]);
    assert!(refused(&submit(&mut w, 402, PRODUCER, opt(PRODUCER, job.id(), [0xC5; 64], &late))).is_none());
    roundtrip(&w.l);
}

#[test]
fn same_position_reseals_cannot_erase_the_target_opt_in_or_avoid_its_forfeit() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 3);
    let seal = [0xA5; 64];
    w.block(12, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal }]);
    let t = target(&w, 1, 10, 40);
    inputs(&mut w, std::slice::from_ref(&t), Some(0));
    submit(&mut w, 12, PRODUCER, opt(PRODUCER, job.id(), seal, &t));
    let before = w.l.to_rows();
    let before_scalars = w.l.scalars();
    let before_root = w.l.root();
    let burned = w.l.burned;
    w.block(
        12,
        vec![
            T::SealClaim { producer: PRODUCER, job: job.id(), seal: [0xB5; 64] },
            T::SealClaim { producer: PRODUCER, job: job.id(), seal: [0xC5; 64] },
        ],
    );
    assert_eq!(w.l.burned - burned, 102);
    let row = &w.l.beacon_sources[&(t.terms.context, PRODUCER)];
    assert_eq!(row.seal, seal);
    assert_eq!(row.held, 0);
    assert_eq!(row.outcome, Some(BeaconSourceOutcomeV1::Forfeited { daa: 12 }));
    refusal_unchanged(&mut w, PRODUCER, &opt(PRODUCER, job.id(), [0xC5; 64], &t));
    let after = w.l.to_rows();
    let delta = diff_rows(&before, &after);
    assert!(delta.iter().any(|((table, _), _, _)| *table == TABLE_BEACON_SOURCES_V1));
    let mut rolled = after.clone();
    for (key, old, _) in &delta {
        if let Some(bytes) = old {
            rolled.insert(key.clone(), bytes.clone());
        } else {
            rolled.remove(key);
        }
    }
    assert_eq!(rolled, before);
    let restored = KernelLedgerV1::from_rows(&w.l, before_scalars, &rolled).unwrap();
    assert_eq!(restored.beacon_sources[&(t.terms.context, PRODUCER)].held, 100);
    assert_eq!(restored.bonds[&PRODUCER].reserved, 101);
    assert_eq!(restored.root(), before_root);
    roundtrip(&w.l);
}

#[test]
fn a_reveal_keeps_source_collateral_and_claim_liability_through_the_signed_horizon() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 4);
    let p = w.honest(&job, 3);
    let claim = p.claim.id();
    let seal = misaka_palw_kernel::route::claim_seal_v2(&claim, &test_salt(&claim));
    w.block(12, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal }]);
    let t = target(&w, 1, 10, 40);
    inputs(&mut w, std::slice::from_ref(&t), Some(0));
    submit(&mut w, 12, PRODUCER, opt(PRODUCER, job.id(), seal, &t));
    let T::CommitClaim { claim: c, evidence, commitments } = p.tx.clone() else { panic!() };
    let early = O::CommitClaimSalted {
        salt: test_salt(&claim),
        commit: misaka_palw_kernel::ledger::SaltedCommitV1::Claim { claim: c, evidence, commitments },
    };
    refusal_unchanged(&mut w, PRODUCER, &early);
    submit(&mut w, 52, PRODUCER, reveal_object(p.tx));
    assert!(w.l.claims.contains_key(&claim));
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 1100);
    assert_eq!(w.l.beacon_sources[&(t.terms.context, PRODUCER)].held, 100);
    assert_eq!(w.l.claims[&claim].liability_until, Some(400));
    let mut restored = roundtrip(&w.l);
    restored.begin_block(102).unwrap();
    restored.tick();
    assert!(matches!(restored.claims[&claim].life.state, ClaimStateV1::Final { .. }));
    assert_eq!(restored.bonds[&PRODUCER].reserved, 1100);
    w.block(102, vec![]);
    w.block(303, vec![]);
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 1100, "ordinary 200-DAA liability is too short for the opted-in target");
    w.block(400, vec![]);
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 1100);
    let ev = w.block(401, vec![]);
    assert!(ev.iter().any(|e| matches!(e, E::Released { claim: c } if *c == claim)));
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0);
    assert_eq!(
        w.l.beacon_sources[&(t.terms.context, PRODUCER)].outcome,
        Some(BeaconSourceOutcomeV1::Refunded { daa: 401, final_daa: 102 })
    );
    let before = w.l.to_rows();
    assert!(w.l.tick().is_empty());
    assert_eq!(w.l.to_rows(), before);
    roundtrip(&w.l);
}

#[test]
fn abandonment_after_reveal_burns_the_source_deposit_as_well_as_the_claim_default() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 5);
    let p = w.honest(&job, 3);
    let id = p.claim.id();
    let seal = misaka_palw_kernel::route::claim_seal_v2(&id, &test_salt(&id));
    w.block(12, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal }]);
    let t = target(&w, 1, 10, 40);
    inputs(&mut w, std::slice::from_ref(&t), Some(0));
    submit(&mut w, 12, PRODUCER, opt(PRODUCER, job.id(), seal, &t));
    submit(&mut w, 52, PRODUCER, reveal_object(p.tx));
    w.block(90, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 1 }]);
    let deadline = w.l.demands[&(id, 0, 1)].deadline_daa;
    let ev = w.block(deadline, vec![]);
    assert!(ev.iter().any(|e| matches!(e, E::ProducerDefault { claim, .. } if *claim == id)));
    assert_eq!(w.l.beacon_sources[&(t.terms.context, PRODUCER)].outcome, Some(BeaconSourceOutcomeV1::Forfeited { daa: deadline }));
    assert_eq!(w.l.bonds[&PRODUCER].collateral, 5000 - 3 - 100 - 100);
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 900);
    roundtrip(&w.l);
}

#[test]
fn source_terms_reject_clock_overflow_incompatible_reveals_and_corrupt_rows() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 6);
    w.block(12, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal: [0xA5; 64] }]);
    let t = target(&w, 1, 10, 40);
    let mut short = target(&w, 2, 10, 10);
    inputs(&mut w, &[t.clone(), short.clone()], Some(0));
    submit(&mut w, 12, PRODUCER, opt(PRODUCER, job.id(), [0xA5; 64], &t));
    refusal_unchanged(&mut w, PRODUCER, &opt(PRODUCER, job.id(), [0xA5; 64], &short));
    let mut no_candidate = t.clone();
    no_candidate.context.candidate_profile_id = RootV1::Absent;
    no_candidate.terms.context = no_candidate.context.id();
    assert!(no_candidate.validate().is_err());
    short.context.commitment_position = u64::MAX;
    assert!(short.validate().is_err());
    let mut ttl = w.l.policy;
    ttl.seal_ttl_daa = u64::MAX;
    assert!(!misaka_palw_kernel::ledger::seal_ttl_admits_beacon_window_v1(&ttl, u64::MAX));
    let clean = w.l.to_rows();
    let key = clean.keys().find(|(table, _)| *table == TABLE_BEACON_SOURCES_V1).unwrap().clone();
    for mutation in 0..5 {
        let mut row = w.l.beacon_sources[&(t.terms.context, PRODUCER)].clone();
        match mutation {
            0 => row.held = 0,
            1 => row.poster = [0; 64],
            2 => row.terms.context = [0; 64],
            3 => row.opted_in_daa = 99,
            _ => row.revealed = Some(([0; 64], 52)),
        }
        let mut rows = clean.clone();
        rows.insert(key.clone(), borsh::to_vec(&row).unwrap());
        assert!(KernelLedgerV1::from_rows(&w.l, w.l.scalars(), &rows).is_err());
    }
}

#[test]
fn a_public_conviction_after_source_final_collects_both_independent_reservations_once() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 7);
    let (_, p) = w.lying(&job, 3);
    let id = p.claim.id();
    let da = Da::publishing(&p.trace, &[]);
    let seal = misaka_palw_kernel::route::claim_seal_v2(&id, &test_salt(&id));
    w.block(12, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal }]);
    let t = target(&w, 1, 10, 40);
    inputs(&mut w, std::slice::from_ref(&t), Some(0));
    submit(&mut w, 12, PRODUCER, opt(PRODUCER, job.id(), seal, &t));
    submit(&mut w, 52, PRODUCER, reveal_object(p.tx));
    w.block(102, vec![]);
    assert!(matches!(w.state(&id), ClaimStateV1::Final { .. }));
    // Actual source hold persists beyond ordinary Final + 200, and a fresh outsider uses validated stored rows.
    w.block(350, vec![]);
    let restored = roundtrip(&w.l);
    let finding =
        misaka_palw_kernel::ledger::OutsiderV1 { ledger: &restored, claim: id, material: &da, artifact: &w.params, salt: [0x5A; 64] }
            .check()
            .unwrap();
    let misaka_palw_kernel::ledger::OutsiderFindingV1::Prosecute(proof) = finding else { panic!("public lie was not found") };
    let ev = w.block(351, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: proof.clone() }]);
    assert_eq!(convicted(&ev), Some((1000, 500, true)), "{ev:?}");
    assert_eq!(w.l.beacon_sources[&(t.terms.context, PRODUCER)].outcome, Some(BeaconSourceOutcomeV1::Forfeited { daa: 351 }));
    assert_eq!((w.l.bonds[&PRODUCER].collateral, w.l.bonds[&PRODUCER].reserved), (3897, 0));
    assert_eq!(w.consumer.paid(&OUTSIDER), 500, "the source deposit is burned, not a second unapproved reporter bounty");
    let burned = w.l.burned;
    let ev = w.block(352, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(ev, vec![E::Duplicate { claim: id }]);
    assert_eq!(w.l.burned, burned);
    roundtrip(&w.l);
}

#[test]
fn source_final_has_its_own_finite_deadline_and_free_collateral_cannot_be_reused() {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 8);
    let p = w.honest(&job, 3);
    let id = p.claim.id();
    let seal = misaka_palw_kernel::route::claim_seal_v2(&id, &test_salt(&id));
    w.block(12, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal }]);
    let mut expensive = target(&w, 1, 10, 40);
    expensive.terms.deposit = 5000;
    inputs(&mut w, &[expensive.clone()], Some(0));
    refusal_unchanged(&mut w, PRODUCER, &opt(PRODUCER, job.id(), seal, &expensive));
    let mut t = target(&w, 2, 10, 40);
    t.terms.final_by = 92;
    inputs(&mut w, &[t.clone()], Some(0));
    submit(&mut w, 12, PRODUCER, opt(PRODUCER, job.id(), seal, &t));
    submit(&mut w, 91, PRODUCER, reveal_object(p.tx));
    w.block(92, vec![]);
    assert_eq!(w.l.beacon_sources[&(t.terms.context, PRODUCER)].held, 100);
    w.block(93, vec![]);
    assert_eq!(w.l.beacon_sources[&(t.terms.context, PRODUCER)].outcome, Some(BeaconSourceOutcomeV1::Forfeited { daa: 93 }));
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 1000);
    w.block(141, vec![]);
    assert!(matches!(w.state(&id), ClaimStateV1::Final { .. }));
    assert_eq!(w.l.beacon_sources[&(t.terms.context, PRODUCER)].held, 0, "late ordinary Final cannot resurrect the source contract");
    roundtrip(&w.l);
}

#[test]
fn prospective_paid_job_enrollment_cannot_change_an_existing_seal_or_source_contract() {
    use misaka_palw_kernel::claim_paid_v2::paid_job_enrollment_is_prospective_v3;
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    assert!(paid_job_enrollment_is_prospective_v3(&w.l, &job.id(), w.l.daa));
    let seal = [0xA7; 64];
    w.block(12, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal }]);
    assert!(
        !paid_job_enrollment_is_prospective_v3(&w.l, &job.id(), w.l.daa),
        "payer cannot add checking terms after a producer's seal"
    );
    let target = target(&w, 71, 10, 40);
    inputs(&mut w, std::slice::from_ref(&target), Some(0));
    let events = submit(&mut w, 13, PRODUCER, opt(PRODUCER, job.id(), seal, &target));
    assert!(refused(&events).is_none(), "{events:?}");
    assert!(
        !paid_job_enrollment_is_prospective_v3(&w.l, &job.id(), w.l.daa),
        "existing source consent and fee/deposit rights are protected"
    );
    w.block(500, vec![]);
    assert!(w.l.beacon_sources.values().all(|source| source.outcome.is_some()));
    assert!(
        paid_job_enrollment_is_prospective_v3(&w.l, &job.id(), w.l.daa),
        "a new future enrollment may use an unclaimed job after old work and source contracts terminate"
    );
}
