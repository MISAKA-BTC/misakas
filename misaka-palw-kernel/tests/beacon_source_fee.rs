//! Signed, prepaid source fees through the ledger and its money-moving consumer.
//! Fixture prices are not measured profit or an approved source/security policy.
mod common;
use common::chain::{T, test_salt};
use common::ledger_world::*;
use misaka_palw_challenge::{BeaconContextV1, RootV1, SubjectKindV1, sealed_source_policy_v3};
use misaka_palw_kernel::beacon_source::{BeaconSourceInputsV1, BeaconSourceTargetV1, BeaconSourceTermsV1};
use misaka_palw_kernel::beacon_source_fee::*;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{AuthV1, KernelLedgerV1, KernelRouteObjectV1 as O, LedgerEventV1 as E, LedgerTxV1};
use misaka_palw_kernel::rows::{TABLE_BEACON_SOURCE_FEE_PROMISES_V1, TABLE_BEACON_SOURCE_FEES_V1, diff_rows};
use misaka_palw_kernel::settle::{SettlementBookV1, SettlementInstructionV1, SettlementKindV1 as K};

fn fixture() -> (World, BeaconSourceTargetV1, BeaconSourceFeeTermsV1, Digest, O) {
    let mut w = World::new_opv();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let p = w.honest(&job, 3);
    let claim = p.claim.id();
    let salt = test_salt(&claim);
    let seal = misaka_palw_kernel::route::claim_seal_v2(&claim, &salt);
    w.block(12, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal }]);
    let context = BeaconContextV1 {
        chain_genesis: w.l.policy.network_domain,
        ruleset_id: w.l.policy.ruleset_digest,
        policy: sealed_source_policy_v3(2, 2, 40, 1, 1),
        subject_kind: SubjectKindV1::ModelConformance,
        commitment_root: [0xD1; 64],
        commitment_position: 10,
        challenge_epoch: 0,
        eligible_profiles: [w.class].into_iter().collect(),
        excluded_profiles: Default::default(),
        candidate_profile_id: RootV1::Present([0xEF; 64]),
    };
    let t = BeaconSourceTargetV1 {
        terms: BeaconSourceTermsV1 {
            context: context.id(),
            seal_start: context.start(),
            seal_end: context.window_end(),
            reveal_end: context.window_end() + 40,
            final_by: 180,
            hold_until: 400,
            deposit: 100,
            max_per_producer: 2,
            max_total: 3,
        },
        context,
    };
    let terms = BeaconSourceFeeTermsV1 { source: t.terms, sponsor: OUTSIDER, fee_per_source: 50, max_sources: 3 };
    inject(&mut w, &t, Some(0), Some(0));
    let T::CommitClaim { claim, evidence, commitments } = p.tx else { panic!() };
    let reveal =
        O::CommitClaimSalted { salt, commit: misaka_palw_kernel::ledger::SaltedCommitV1::Claim { claim, evidence, commitments } };
    (w, t, terms, seal, reveal)
}
fn inject(w: &mut World, t: &BeaconSourceTargetV1, source: Option<u64>, fee: Option<u64>) {
    w.l.set_beacon_source_inputs(BeaconSourceInputsV1 { activation_daa: source, targets: [(t.terms.context, t.clone())].into() })
        .unwrap();
    w.l.set_beacon_source_fee_inputs(BeaconSourceFeeInputsV1 {
        activation_daa: fee,
        targets: [(
            t.terms.context,
            BeaconSourceFeeTargetV1 { target: t.clone(), sponsor: OUTSIDER, minimum_fee: 50, max_sources: 3 },
        )]
        .into(),
    })
    .unwrap();
}
fn submit(w: &mut World, daa: u64, signer: Digest, object: O) -> Vec<E> {
    w.block_raw(daa, vec![LedgerTxV1::Object { auth: AuthV1 { signer_bond: signer }, object }])
}
fn opt(w: &World, seal: Digest, terms: BeaconSourceFeeTermsV1) -> O {
    O::OptInPaidBeaconSource {
        producer: PRODUCER,
        job: *w.l.jobs.keys().next().unwrap(),
        seal,
        context: terms.source.context,
        source_terms: terms.source.id(),
        fee_terms: terms.id(),
    }
}
fn refuse(w: &mut World, signer: Digest, o: O) {
    let rows = w.l.to_rows();
    let root = w.l.root();
    assert!(w.l.apply_object(&o, &AuthV1 { signer_bond: signer }).is_err());
    assert_eq!(w.l.to_rows(), rows);
    assert_eq!(w.l.root(), root);
}
fn fund(w: &mut World, terms: BeaconSourceFeeTermsV1) {
    let e = submit(w, 12, OUTSIDER, O::FundBeaconSourceFee { terms });
    assert!(refused(&e).is_none(), "{e:?}");
}
fn check(w: &World) {
    w.l.validate_beacon_source_fees().unwrap();
    w.consumer.check_view(&w.l);
    assert_eq!(
        misaka_palw_kernel::rows::root_of_rows(
            &w.l.policy,
            misaka_palw_kernel::rows::config_root_of(&w.l.schedule, &w.l.known),
            w.l.scalars(),
            w.l.opv.policy.as_ref(),
            &w.l.to_rows()
        ),
        w.l.root()
    );
    let rebuilt = KernelLedgerV1::from_rows(&w.l, w.l.scalars(), &w.l.to_rows()).unwrap();
    assert_eq!(rebuilt.root(), w.l.root());
    assert!(rebuilt.beacon_source_fee_inputs.activation_daa.is_none());
    assert!(rebuilt.beacon_source_fee_inputs.targets.is_empty());
}
#[test]
fn funding_requires_both_fences_signature_exact_target_and_real_free_collateral() {
    let (mut w, t, terms, _, _) = fixture();
    let object = O::FundBeaconSourceFee { terms };
    for (s, f) in [(None, Some(0)), (Some(0), None), (Some(0), Some(13)), (Some(0), Some(11))] {
        inject(&mut w, &t, s, f);
        refuse(&mut w, OUTSIDER, object.clone());
    }
    inject(&mut w, &t, Some(0), Some(0));
    refuse(&mut w, PRODUCER, object.clone());
    for bad in [
        BeaconSourceFeeTermsV1 { fee_per_source: 49, ..terms },
        BeaconSourceFeeTermsV1 { sponsor: PRODUCER, ..terms },
        BeaconSourceFeeTermsV1 { fee_per_source: u64::MAX, ..terms },
        BeaconSourceFeeTermsV1 { fee_per_source: 500, ..terms },
        BeaconSourceFeeTermsV1 { max_sources: 4, ..terms },
    ] {
        refuse(&mut w, bad.sponsor, O::FundBeaconSourceFee { terms: bad });
    }
    let c = w.l.bonds[&OUTSIDER].collateral;
    let burned = w.l.burned;
    let paid = w.consumer.paid(&OUTSIDER);
    fund(&mut w, terms);
    assert_eq!(w.l.bonds[&OUTSIDER].collateral, c - 150);
    assert_eq!(w.l.source_fee_escrow_total().unwrap(), 150);
    assert_eq!(w.l.burned, burned);
    assert_eq!(w.consumer.paid(&OUTSIDER), paid);
    refuse(&mut w, OUTSIDER, object);
    check(&w);
}
#[test]
fn fee_and_source_consent_are_bound_to_prepaid_terms_and_not_retroactive() {
    let (mut w, _, terms, seal, _) = fixture();
    let object = opt(&w, seal, terms);
    refuse(&mut w, PRODUCER, object.clone());
    fund(&mut w, terms);
    refuse(&mut w, OUTSIDER, object.clone());
    let wrong = opt(&w, seal, BeaconSourceFeeTermsV1 { fee_per_source: 51, ..terms });
    refuse(&mut w, PRODUCER, wrong);
    let old = O::OptInBeaconSource {
        producer: PRODUCER,
        job: *w.l.jobs.keys().next().unwrap(),
        seal,
        context: terms.source.context,
        terms: terms.source.id(),
    };
    assert!(refused(&submit(&mut w, 12, PRODUCER, old)).is_none());
    refuse(&mut w, PRODUCER, object);
    assert!(w.l.beacon_source_fee_promises.is_empty());
    w.block(52, vec![]);
    assert_eq!(w.consumer.paid(&OUTSIDER), 150);
    w.block(92, vec![]);
    assert_eq!(w.consumer.paid(&PRODUCER), 0);
    check(&w);
}

#[test]
fn even_a_consumer_authenticated_future_target_cannot_debit_the_sponsor_early() {
    let (mut w, mut target, terms, _, _) = fixture();
    target.context.commitment_position = 20;
    target.terms.context = target.context.id();
    target.terms.seal_start = target.context.start();
    target.terms.seal_end = target.context.window_end();
    target.terms.reveal_end = target.terms.seal_end + 40;
    inject(&mut w, &target, Some(0), Some(0));
    refuse(&mut w, OUTSIDER, O::FundBeaconSourceFee { terms: BeaconSourceFeeTermsV1 { source: target.terms, ..terms } });
    assert_eq!(w.l.source_fee_escrow_total().unwrap(), 0);
}
#[test]
fn only_a_standing_timely_final_vests_at_the_full_signed_horizon_once() {
    let (mut w, _, terms, seal, reveal) = fixture();
    fund(&mut w, terms);
    let object = opt(&w, seal, terms);
    assert!(refused(&submit(&mut w, 12, PRODUCER, object)).is_none());
    submit(&mut w, 52, PRODUCER, reveal);
    assert_eq!(w.consumer.paid(&PRODUCER), 0);
    assert_eq!(w.consumer.paid(&OUTSIDER), 100);
    w.block(102, vec![]);
    let final_paid = w.consumer.paid(&PRODUCER);
    assert_eq!(w.l.source_fee_escrow_total().unwrap(), 50);
    w.block(400, vec![]);
    assert_eq!(w.consumer.paid(&PRODUCER), final_paid);
    let rows = w.l.to_rows();
    let scalars = w.l.scalars();
    let root = w.l.root();
    let before = w.consumer.clone();
    w.block(401, vec![]);
    assert_eq!(w.consumer.paid(&PRODUCER), final_paid + 50);
    assert_eq!(w.l.source_fee_escrow_total().unwrap(), 0);
    check(&w);
    w.block(402, vec![]);
    assert_eq!(w.consumer.paid(&PRODUCER), final_paid + 50);
    let mut rolled = w.l.to_rows();
    for (key, old, _) in diff_rows(&rows, &rolled) {
        if let Some(v) = old {
            rolled.insert(key, v);
        } else {
            rolled.remove(&key);
        }
    }
    let restored = KernelLedgerV1::from_rows(&w.l, scalars, &rolled).unwrap();
    assert_eq!(restored.root(), root);
    w.l = restored;
    w.consumer = before;
    w.block(401, vec![]);
    assert_eq!(w.consumer.paid(&PRODUCER), final_paid + 50);
    check(&w);
}
#[test]
fn withhold_and_reseal_return_fee_principal_once_and_burn_the_independent_deposit() {
    for reseal in [false, true] {
        let (mut w, _, terms, seal, _) = fixture();
        fund(&mut w, terms);
        let object = opt(&w, seal, terms);
        submit(&mut w, 12, PRODUCER, object.clone());
        let burned = w.l.burned;
        if reseal {
            w.block(13, vec![T::SealClaim { producer: PRODUCER, job: *w.l.jobs.keys().next().unwrap(), seal: [0xA5; 64] }]);
        } else {
            w.block(92, vec![]);
        }
        assert!(w.l.burned >= burned + 100);
        w.block(93, vec![]);
        assert_eq!(w.consumer.paid(&OUTSIDER), 150);
        assert_eq!(w.consumer.paid(&PRODUCER), 0);
        assert_eq!(w.l.source_fee_escrow_total().unwrap(), 0);
        refuse(&mut w, PRODUCER, object);
        check(&w);
    }
}
#[test]
fn wire_ceiling_and_rooted_money_inventory_refuse_corruption() {
    let (mut w, _, terms, seal, _) = fixture();
    for object in [O::FundBeaconSourceFee { terms }, opt(&w, seal, terms)] {
        let bytes = object.encode();
        assert_eq!(O::decode(&bytes).unwrap(), object);
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(O::decode(&trailing).is_err());
        let mut large = vec![0; object.max_encoded_bytes() + 1];
        large[0] = 1;
        large[1] = object.tag();
        assert!(O::decode(&large).is_err());
    }
    fund(&mut w, terms);
    let object = opt(&w, seal, terms);
    submit(&mut w, 12, PRODUCER, object);
    check(&w);
    let rows = w.l.to_rows();
    for table in [TABLE_BEACON_SOURCE_FEES_V1, TABLE_BEACON_SOURCE_FEE_PROMISES_V1] {
        let mut bad = rows.clone();
        let key = bad.keys().find(|(t, _)| *t == table).unwrap().clone();
        bad.remove(&key);
        assert!(KernelLedgerV1::from_rows(&w.l, w.l.scalars(), &bad).is_err());
    }
    for mode in 0..4 {
        let mut bad = w.l.clone();
        let f = bad.beacon_source_fees.get_mut(&terms.source.context).unwrap();
        match mode {
            0 => f.held -= 1,
            1 => f.funded_daa = 13,
            2 => f.allocated = 2,
            _ => f.terms.max_sources = 0,
        }
        assert!(KernelLedgerV1::from_rows(&w.l, w.l.scalars(), &bad.to_rows()).is_err());
    }
}
#[test]
fn prepaid_source_income_survives_later_loss_of_the_sponsors_remaining_balance() {
    let ctx = [0xD1; 64];
    let mut book = SettlementBookV1::default();
    book.set_collateral(OUTSIDER, 1000);
    let receipt = |bond, amount, kind| SettlementInstructionV1 { bond, amount, kind, claim: Some(ctx) };
    book.apply(&receipt(OUTSIDER, 150, K::FundSourceFeeEscrow)).unwrap();
    book.balanced().unwrap();
    // A different native lane can slash the entire remainder; it cannot debit this escrow.
    book.apply(&receipt(OUTSIDER, 850, K::SlashFiling)).unwrap();
    book.apply(&receipt(OUTSIDER, 850, K::Burn)).unwrap();
    book.balanced().unwrap();
    assert_eq!(book.collateral[&OUTSIDER], 0);
    book.apply(&receipt(PRODUCER, 50, K::PaySourceFeeEscrow)).unwrap();
    book.apply(&receipt(OUTSIDER, 100, K::RefundSourceFeeEscrow)).unwrap();
    book.balanced().unwrap();
    assert_eq!(book.paid[&PRODUCER], 50);
    assert!(book.apply(&receipt(PRODUCER, 1, K::PaySourceFeeEscrow)).is_err());
}
