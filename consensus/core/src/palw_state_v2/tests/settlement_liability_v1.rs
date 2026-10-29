//! RFC-0008 RFC08-2: the combined liability predicate wired into settlement (`settle_once_liable_v1`) and admission
//! (`admit_claim_liability_v1`, `admit_native_claim_liability_v1`). SOURCE ONLY — written, never compiled or executed in this lane.
//! The pure predicate's every term is pinned in `palw_settlement_liability_v1`'s own tests; these pin the WIRING: dormancy below
//! the fence, the old ruleset for a claim accepted before it, an unsourced term refusing (never defaulted) with a zero-amount
//! marker that closes the obligation, and a leg with no liable claim passing by name.
use super::super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1;
use super::super::palw_settlement_once_fold_v1::{PalwSettleOnceV1, palw_settled_once_v1};
use super::*;
use crate::palw_settlement_liability_v1::PalwSettlementLiabilityRefusalV1;
use crate::palw_settlement_once_v1::*;

fn with_route(mut state: PalwChainStateV2) -> PalwChainStateV2 {
    state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    ));
    state
}

fn obligation(leg: PalwSettlementLegV1, subject: u8) -> PalwSettlementObligationV1 {
    PalwSettlementObligationV1::of(leg, [subject; 64])
}

fn v2_leg(claim: Hash64) -> PalwLegLiabilityV1 {
    PalwLegLiabilityV1::V2Claim {
        claim,
        final_daa: Some(124),
        horizon: None,
        draw: 0,
        paid_now: 5,
        exported_before: None,
        vest_until: None,
    }
}

#[test]
fn below_the_fence_the_predicate_reads_nothing_and_every_leg_is_dormant() {
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    for leg in [PalwLegLiabilityV1::Unsourced("x"), PalwLegLiabilityV1::NoLiableClaim("y"), v2_leg(h64(9))] {
        let once = b
            .settle_once_liable_v1(
                100,
                obligation(PalwSettlementLegV1::GateDeferredRelease, 1),
                PalwSettlementConsumerV1::VerificationGate,
                5,
                PalwSettlementDebitV1::Held { amount: 5 },
                leg,
            )
            .unwrap();
        assert_eq!(once, PalwSettleOnceV1::Dormant);
        assert_eq!(b.leg_liability_refusal_v1(100, &leg, 5), None, "nothing binds below the fence");
    }
    b.admit_native_claim_liability_v1(100, None, 500).unwrap();
    assert_eq!(b.state, parent);
    assert!(b.entries.is_empty());
}

#[test]
fn an_unsourced_term_refuses_the_leg_by_name_and_closes_the_obligation_at_zero() {
    let p = params().with_settlement_once_from(Some(0));
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let o = obligation(PalwSettlementLegV1::VerifierCheckFee, 3);
    let leg = PalwLegLiabilityV1::Unsourced("RFC07-5: no verifier horizon");
    let once = b
        .settle_once_liable_v1(100, o, PalwSettlementConsumerV1::KernelTick, 7, PalwSettlementDebitV1::Held { amount: 7 }, leg)
        .unwrap();
    assert_eq!(once, PalwSettleOnceV1::Refused(PalwSettlementLiabilityRefusalV1::Unsourced("RFC07-5: no verifier horizon")));
    assert!(!once.pays());
    let row = palw_settled_once_v1(&b.state, &o).expect("the refusal is journaled");
    assert_eq!((row.amount, row.consumer), (0, PalwSettlementConsumerV1::KernelTick), "nothing was paid");
    // The closed obligation is not re-judged: any later consumer finds it settled (at zero) and pays nothing.
    let again = b
        .settle_once_liable_v1(101, o, PalwSettlementConsumerV1::KernelObject, 7, PalwSettlementDebitV1::Held { amount: 7 }, leg)
        .unwrap();
    assert!(matches!(again, PalwSettleOnceV1::AlreadySettled(row) if row.amount == 0));
    // ADR-0184 §1 (user decision 2026-10-18): a native claim is no longer unsourced — its maximum horizon is derived from the
    // producer-signed deadline that bounds its Final floor, plus `window_court`; an unbudgeted one has no reservation to bind.
    assert_eq!(super::super::palw_settlement_liability_fold_v1::native_claim_hard_deadline_v1(&p, 500), 500 + p.window_court());
    b.admit_native_claim_liability_v1(100, None, 500).unwrap();
}

#[test]
fn a_leg_with_no_liable_claim_settles_by_name_and_an_unknown_claim_is_unsourced() {
    let p = params().with_settlement_once_from(Some(0));
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let funding = obligation(PalwSettlementLegV1::MaterialFunding, 4);
    let once = b
        .settle_once_liable_v1(
            100,
            funding,
            PalwSettlementConsumerV1::MonetaryMaterial,
            9,
            PalwSettlementDebitV1::Prepaid { sponsor: [1; 64], amount: 9 },
            PalwLegLiabilityV1::NoLiableClaim("a sponsor's prepaid funding debit"),
        )
        .unwrap();
    assert_eq!(once, PalwSettleOnceV1::First);
    assert_eq!(palw_settled_once_v1(&b.state, &funding).unwrap().amount, 9);
    // A V2 claim the state has never seen: its acceptance is unsourced — refused, never defaulted to "old ruleset".
    assert_eq!(
        b.leg_liability_refusal_v1(100, &v2_leg(h64(0x77)), 5),
        Some(PalwSettlementLiabilityRefusalV1::Unsourced("the claim's acceptance (no claim record, no reservation row)"))
    );
    // A kernel claim the route has no row for.
    let kernel = PalwLegLiabilityV1::KernelClaim { claim: [0x77; 64], draw: 5, paid_now: 5, vest_until: None };
    assert_eq!(b.leg_liability_refusal_v1(100, &kernel, 5), Some(PalwSettlementLiabilityRefusalV1::Unsourced("the kernel claim row")));
}

#[test]
fn a_claim_accepted_before_the_fence_keeps_its_old_ruleset_and_a_later_one_needs_its_reservation() {
    // The fixture's free-prompt claim is accepted at DAA 101 and Final at 124.
    let base = params();
    let certified = with_route(certify_fp_claim(&base, 60, 3));
    let late = base.clone().with_settlement_once_from(Some(130));
    let extras = PalwTransitionExtrasV1::default();
    let mut b = TransitionBuilder::new(&certified, &late, false, false, false, false, &extras);
    assert_eq!(b.leg_liability_refusal_v1(130, &v2_leg(h64(0xFC)), 5), None, "accepted at 101 < 130: the old ruleset");
    let o = PalwSettlementObligationV1::of(PalwSettlementLegV1::CertificationRelease, h64(0xFC).as_bytes());
    let once = b
        .settle_once_liable_v1(
            130,
            o,
            PalwSettlementConsumerV1::SecurityCertification,
            5,
            PalwSettlementDebitV1::Held { amount: 5 },
            v2_leg(h64(0xFC)),
        )
        .unwrap();
    assert_eq!(once, PalwSettleOnceV1::First);
    // The same claim under a fence that predates its acceptance binds — and with no ADR-0176 engine its terms are unsourced.
    let early = base.with_settlement_once_from(Some(0));
    let b = TransitionBuilder::new(&certified, &early, false, false, false, false, &extras);
    assert_eq!(
        b.leg_liability_refusal_v1(130, &v2_leg(h64(0xFC)), 5),
        Some(PalwSettlementLiabilityRefusalV1::Unsourced("the claim's ADR-0176 reservation (no engine)"))
    );
}

#[test]
fn ordinary_kernel_hard_deadline_uses_its_acceptance_bound_x3_without_replacing_paid_terms() {
    use super::super::palw_settlement_liability_fold_v1::kernel_claim_signed_hard_deadline_v1 as hard;
    use crate::palw_ruling_fences_v1::{PalwClaimHardDeadlineFenceV1, PalwClaimHardDeadlineMirrorV1};
    let dormant = params();
    assert!(hard(&dormant, 100, None).is_err());
    assert_eq!(hard(&dormant, 100, Some(555)), Ok(555), "existing signed paid lifetime is unchanged");
    let policy = PalwClaimHardDeadlineFenceV1::unapproved_probe_at(crate::config::params::ForkActivation::new(100)).policy;
    let span = policy.prepare_daa + policy.observe_daa + policy.resolve_daa;
    let armed = dormant.with_claim_hard_deadline_v1(Some(PalwClaimHardDeadlineMirrorV1 { from_daa: 100, policy }));
    assert!(hard(&armed, 99, None).is_err(), "later activation never rewrites an old acceptance");
    assert_eq!(hard(&armed, 100, None), Ok(100 + span));
    assert_eq!(hard(&armed, 101, None), Ok(101 + span));
    assert_eq!(hard(&armed, 100, Some(555)), Ok(555), "X3 is not a silent extension of paid consent");
    assert!(hard(&armed, u64::MAX - span + 1, None).is_err(), "overflow cannot turn into an unlimited term");
}

/// Settlement adapter fixture: rows stand for already authenticated ledger/material state.
/// This checks the consumer's money/identity bindings, not producer signature admission or source independence.
#[test]
fn beacon_source_fee_reads_signed_hold_actual_material_and_exact_prepaid_promise() {
    use crate::palw_monetary_material_v1::*;
    use misaka_palw_kernel::{beacon_source::*, beacon_source_fee::*, evidence::*, job::KernelClaimV1, ledger::*, lifecycle::*};
    let p = params().with_settlement_once_from(Some(10)).with_bond_budget(Some(crate::palw_bond_budget_v1::PalwBondBudgetMirrorV1 {
        from_daa: 10,
        policy: crate::palw_values_p4_v1::palw_bond_budget_policy_provisional_v1(),
        allocation: None,
    }));
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    let claim = [7; 64];
    let producer = [8; 64];
    let sponsor = [9; 64];
    let context = misaka_palw_challenge::BeaconContextV1 {
        chain_genesis: [1; 64],
        ruleset_id: [2; 64],
        policy: misaka_palw_challenge::sealed_source_policy_v3(2, 2, 40, 1, 1),
        subject_kind: misaka_palw_challenge::SubjectKindV1::ModelConformance,
        commitment_root: [3; 64],
        commitment_position: 10,
        challenge_epoch: 0,
        eligible_profiles: Default::default(),
        excluded_profiles: Default::default(),
        candidate_profile_id: misaka_palw_challenge::RootV1::Present([4; 64]),
    };
    let key = context.id();
    let source = BeaconSourceTermsV1 {
        context: key,
        seal_start: 12,
        seal_end: 52,
        reveal_end: 92,
        final_by: 180,
        hold_until: 400,
        deposit: 100,
        max_per_producer: 1,
        max_total: 1,
    };
    let fee_terms = BeaconSourceFeeTermsV1 { source, sponsor, fee_per_source: 50, max_sources: 1 };
    let mut ledger = parent.kernel_route.as_ref().unwrap().ledger_checked_v1().unwrap();
    let l = ledger.preview_mut_v1();
    l.beacon_source_fees.insert(
        key,
        BeaconSourceFeeFundingV1 {
            target: BeaconSourceTargetV1 { context, terms: source },
            terms: fee_terms,
            funded_daa: 12,
            held: 50,
            allocated: 1,
            paid: 0,
            refunded: 0,
            unused_refunded: true,
        },
    );
    l.beacon_source_fee_promises
        .insert((key, producer), BeaconSourceFeePromiseV1 { terms_id: fee_terms.id(), accepted_daa: 12, fee: 50, outcome: None });
    l.beacon_sources.insert(
        (key, producer),
        BeaconSourceRowV1 {
            producer,
            job: [5; 64],
            profile: [6; 64],
            poster: sponsor,
            seal: [11; 64],
            sealed_daa: 12,
            opted_in_daa: 12,
            terms: source,
            held: 100,
            revealed: Some((claim, 60)),
            outcome: None,
        },
    );
    l.bonds.insert(producer, BondRowV1 { collateral: 200, reserved: 150, exit_requested: None });
    let header = EvidenceHeaderV1 {
        network_domain: [1; 64],
        ruleset_digest: [2; 64],
        class_binding_id: [6; 64],
        program_root: [0; 64],
        artifact_root: [0; 64],
        plan_root: [0; 64],
    };
    l.claims.insert(
        claim,
        ClaimRowV1 {
            producer,
            class_binding_id: [6; 64],
            job_id: [5; 64],
            body: ClaimBodyV1::Program {
                claim: KernelClaimV1 { job_id: [5; 64], producer_bond: producer, generated: vec![1], evidence_root: [0; 64] },
                evidence: VerificationEvidenceV1 {
                    version: 1,
                    header,
                    job_input_root: [0; 64],
                    positions: 1,
                    initial_state_root: [0; 64],
                    final_state_root: [0; 64],
                    trace_root: [0; 64],
                    output_root: [0; 64],
                    segments: Vec::new(),
                    suite: SuiteParamsV1 {
                        descriptor_digest: [0; 64],
                        checker_suite_id: 0,
                        challenge_policy_id: 0,
                        repetitions: 0,
                        target_bits: 0,
                        field_bits: 0,
                    },
                },
                commitments: Vec::new(),
            },
            committed_daa: 60,
            life: ClaimLifecycleV1 {
                policy: LifecyclePolicyV1 { check_window_daa: 30, challenge_window_daa: 30 },
                state: ClaimStateV1::Final { final_daa: 120 },
                retention_met: true,
                final_hold_until: 120,
            },
            reserved: 50,
            liability_until: Some(400),
            convicted: false,
            rewarded: true,
            sealed_daa: 12,
        },
    );
    let material = PalwMonetaryMaterialRowV1 {
        terms: PalwMonetaryMaterialTermsV1 {
            version: 1,
            root: Hash64::from_bytes(claim),
            dependencies: vec![],
            exec_plan_root: None,
            sponsor: bond_key(1),
            nonce: h64(101),
            policy: h64(102),
            max_material_bytes: 100,
            material_manifest: None,
            native_session: None,
            window_daa: 50,
            enrollment_until_daa: 100,
            payment_relative_until_finality: true,
        },
        consent: h64(103),
        sequence: 0,
        accepted_daa: 12,
        root_seen: true,
        native_binding: None,
        providers: [1, 2]
            .map(|id| PalwMonetaryMaterialProviderV1 {
                provider: bond_key(id),
                collateral_sompi: 100,
                prepaid_fee_sompi: 10,
                charged: false,
                fee_income: h64(id),
                fee_context: h64(id + 10),
                fee_accepted_daa: 12,
                fee_queued: false,
                fee_paid: false,
            })
            .to_vec(),
        challenges: vec![],
        last_tail_until: None,
        tail_block: None,
        tail_blue_score: None,
        last_payment: None,
        payment_history_root: h64(0),
        service_fee_count: 0,
        service_fee_total_sompi: 0,
        service_fee_paid_sompi: 0,
        service_fee_history_root: h64(0),
        queued_entitlements: Default::default(),
    };
    for id in [1, 2] {
        parent.bonds.insert(
            bond_key(id),
            PalwBondStateV2 {
                pubkey: vec![7; 4],
                operator_id: h64(id),
                collateral: 1000,
                slashed: 0,
                status: PalwBondStatusV2::Active,
                registered_daa: 0,
                payout_payload: h64(id + 20),
                capable_classes: Default::default(),
            },
        );
    }
    let material_key = (PALW_MONETARY_MATERIAL_TABLE_V1, claim.to_vec());
    parent.kernel_route.as_mut().unwrap().aux.insert(material_key.clone(), borsh::to_vec(&material).unwrap());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let terms = b.beacon_source_fee_terms_v1(&ledger, key, producer, 401, 50).unwrap().unwrap();
    crate::palw_settlement_liability_v1::palw_settlement_liability_sourced_v1(&terms).unwrap();
    assert_eq!(terms.signed_hard_deadline, Ok(400));
    assert!(b.beacon_source_fee_terms_v1(&ledger, key, producer, 400, 50).is_err(), "inclusive hold");
    assert!(b.beacon_source_fee_terms_v1(&ledger, key, producer, 401, 51).is_err(), "exact fee binding");
    ledger.preview_mut_v1().beacon_source_fee_promises.get_mut(&(key, producer)).unwrap().terms_id = [0; 64];
    assert!(b.beacon_source_fee_terms_v1(&ledger, key, producer, 401, 50).is_err(), "terms cannot be substituted");
    ledger.preview_mut_v1().beacon_source_fee_promises.get_mut(&(key, producer)).unwrap().terms_id = fee_terms.id();
    b.state.kernel_route.as_mut().unwrap().aux.remove(&material_key);
    let missing = b.beacon_source_fee_terms_v1(&ledger, key, producer, 401, 50).unwrap().unwrap();
    assert!(
        matches!(
            crate::palw_settlement_liability_v1::palw_settlement_liability_sourced_v1(&missing),
            Err(PalwSettlementLiabilityRefusalV1::Unsourced(_))
        ),
        "no material exemption"
    );
    b.state.kernel_route.as_mut().unwrap().aux.insert(material_key, borsh::to_vec(&material).unwrap());
    let obligation = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::SourceFeePromise, key, producer, 0);
    let debit = PalwSettlementDebitV1::Prepaid { sponsor, amount: 50 };
    assert_eq!(
        b.settle_once_beacon_source_fee_v1(401, obligation, PalwSettlementConsumerV1::KernelTick, 50, debit, &ledger, key, producer)
            .unwrap(),
        PalwSettleOnceV1::First
    );
    assert!(matches!(
        b.settle_once_beacon_source_fee_v1(402, obligation, PalwSettlementConsumerV1::KernelObject, 50, debit, &ledger, key, producer)
            .unwrap(),
        PalwSettleOnceV1::AlreadySettled(_)
    ));
    let old = p.clone().with_settlement_once_from(Some(13));
    let old_b = TransitionBuilder::new(&parent, &old, false, false, false, false, &extras);
    assert!(old_b.beacon_source_fee_terms_v1(&ledger, key, producer, 401, 50).unwrap().is_none(), "old signed opt-in unchanged");
}

#[test]
fn ordinary_verifier_fee_deadline_refund_late_verdict_and_replay_conserve_escrow() {
    use super::super::palw_verifier_fee_consent_fold_v1 as fee_fold;
    use crate::palw_verifier_fee_consent_v1::*;
    use misaka_palw_kernel::verifier_fee_consent_v1::verifier_fee_policy_digest_v1;
    let p = params().with_settlement_once_from(Some(10)).with_bond_budget(Some(crate::palw_bond_budget_v1::PalwBondBudgetMirrorV1 {
        from_daa: 10,
        policy: crate::palw_values_p4_v1::palw_bond_budget_policy_provisional_v1(),
        allocation: None,
    }));
    let extras = PalwTransitionExtrasV1::default();
    for (conviction, fee) in [(None, 17), (Some(100), 17), (Some(101), 17), (None, 0)] {
        let mut parent = with_route(PalwChainStateV2::genesis());
        for id in [1, 2] {
            parent.bonds.insert(
                bond_key(id),
                PalwBondStateV2 {
                    pubkey: vec![7; 4],
                    operator_id: h64(id),
                    collateral: 1000,
                    slashed: 0,
                    status: PalwBondStatusV2::Active,
                    registered_daa: 0,
                    payout_payload: h64(id + 20),
                    capable_classes: Default::default(),
                },
            );
        }
        let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
        let poster = b.note_kernel_bond(&bond_key(1)).unwrap();
        let verifier = b.note_kernel_bond(&bond_key(2)).unwrap();
        let row = HeldVerifierFeeV1 {
            terms: VerifierFeeConsentV1 {
                version: 1,
                network: [1; 64],
                ruleset: [2; 64],
                claim: [9; 64],
                verifier,
                slot: 3,
                poster,
                fee,
                liability_until: 100,
                material_until: 100,
                policy_digest: verifier_fee_policy_digest_v1(),
                nonce: 0,
                clean: true,
            },
            amount: fee,
            convicted_daa: conviction,
        };
        // A typed escrow fixture, not a signature/admission test. There is deliberately no Final claim.
        if fee > 0 {
            b.reserve_source_income_v1(
                &ctx(90, 90, 90),
                verifier_fee_income_context_v1(row.terms.claim, verifier, 3),
                Hash64::from_bytes(row.terms.claim),
                bond_key(2),
                100,
                fee,
            )
            .unwrap();
        }
        fee_fold::write_hold(&mut b, &row).unwrap();
        let held = b.state.clone();
        fee_fold::release(&mut b, &ctx(100, 100, 100)).unwrap();
        assert_eq!(b.state, held, "inclusive fee deadline");
        fee_fold::release(&mut b, &ctx(101, 101, 101)).unwrap();
        let obligation = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::VerifierCheckFee, [9; 64], verifier, 3);
        assert_eq!(palw_settled_once_v1(&b.state, &obligation).unwrap().amount, fee);
        let recipient = if conviction == Some(100) { h64(21) } else { h64(22) };
        assert_eq!(b.state.pending_payouts.values().map(|v| v.amount).sum::<u64>(), fee);
        assert!(fee == 0 || b.state.pending_payouts.values().any(|v| v.payload == recipient && v.amount == fee));
        let paid = b.state.clone();
        fee_fold::release(&mut b, &ctx(102, 102, 102)).unwrap();
        assert_eq!(b.state, paid, "release replay cannot duplicate payment");
        // Reorg returns to the rooted pre-release state, then selected-chain DAA gives the same result.
        let mut replay = TransitionBuilder::new(&held, &p, false, false, false, false, &extras);
        fee_fold::release(&mut replay, &ctx(101, 101, 101)).unwrap();
        assert_eq!(replay.state, paid);
    }
}

#[test]
fn ordinary_fee_fate_requires_exact_rooted_draw_timely_fate_and_no_clean_consent() {
    use super::super::palw_verifier_fee_consent_fold_v1::{FeeDisposition, prepare_hold};
    use misaka_palw_kernel::{evidence::*, job::KernelClaimV1, ledger::*, lifecycle::*, verifier_pay::*};
    let p = params().with_settlement_once_from(Some(10));
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    let claim = [7; 64];
    let producer = [8; 64];
    let verifier = [9; 64];
    parent.bonds.insert(
        bond_key(1),
        PalwBondStateV2 {
            pubkey: vec![7; 4],
            operator_id: h64(1),
            collateral: 1000,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 0,
            payout_payload: h64(21),
            capable_classes: Default::default(),
        },
    );
    let (mapped, poster) = {
        let mut setup = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
        let id = setup.note_kernel_bond(&bond_key(1)).unwrap();
        (setup.state.clone(), id)
    };
    parent = mapped;
    let mut ledger = parent.kernel_route.as_ref().unwrap().ledger_checked_v1().unwrap();
    let l = ledger.preview_mut_v1();
    let header = EvidenceHeaderV1 {
        network_domain: [1; 64],
        ruleset_digest: [2; 64],
        class_binding_id: [6; 64],
        program_root: [0; 64],
        artifact_root: [0; 64],
        plan_root: [0; 64],
    };
    l.claims.insert(
        claim,
        ClaimRowV1 {
            producer,
            class_binding_id: [6; 64],
            job_id: [5; 64],
            body: ClaimBodyV1::Program {
                claim: KernelClaimV1 { job_id: [5; 64], producer_bond: producer, generated: vec![1], evidence_root: [0; 64] },
                evidence: VerificationEvidenceV1 {
                    version: 1,
                    header,
                    job_input_root: [0; 64],
                    positions: 1,
                    initial_state_root: [0; 64],
                    final_state_root: [0; 64],
                    trace_root: [0; 64],
                    output_root: [0; 64],
                    segments: Vec::new(),
                    suite: SuiteParamsV1 {
                        descriptor_digest: [0; 64],
                        checker_suite_id: 0,
                        challenge_policy_id: 0,
                        repetitions: 0,
                        target_bits: 0,
                        field_bits: 0,
                    },
                },
                commitments: Vec::new(),
            },
            committed_daa: 60,
            life: ClaimLifecycleV1 {
                policy: LifecyclePolicyV1 { check_window_daa: 30, challenge_window_daa: 30 },
                state: ClaimStateV1::Convicted { daa: 90 },
                retention_met: true,
                final_hold_until: 120,
            },
            reserved: 50,
            liability_until: Some(400),
            convicted: false,
            rewarded: true,
            sealed_daa: 12,
        },
    );

    l.verifier_pay.insert(
        (VERIFIER_PAY_DRAW_V1, claim),
        VerifierPayRowV1::Draw {
            job: [5; 64],
            poster,
            fee: 17,
            slots: vec![(verifier, false), ([11; 64], false), (verifier, false)],
            deadline_daa: 100,
        },
    );
    parent.kernel_route.as_mut().unwrap().rows = ledger.to_rows();
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    assert!(matches!(prepare_hold(&mut b, claim, verifier, 0, poster, 17, 101, false, &[]).unwrap(), FeeDisposition::Fate(0)));
    assert!(matches!(prepare_hold(&mut b, claim, verifier, 1, poster, 17, 101, false, &[]).unwrap(), FeeDisposition::Fate(2)));
    assert!(prepare_hold(&mut b, claim, verifier, 0, poster, 18, 101, false, &[]).is_err(), "fee amount must match draw");
    assert!(prepare_hold(&mut b, claim, verifier, 0, poster, 17, 101, true, &[]).is_err(), "Clean object cannot use fate exemption");
    assert!(prepare_hold(&mut b, claim, verifier, 2, poster, 17, 101, false, &[]).is_err(), "no fabricated extra slot");
    let mut partial = parent.clone();
    partial.bonds.get_mut(&bond_key(1)).unwrap().collateral = 9;
    let mut pb = TransitionBuilder::new(&partial, &p, false, false, false, false, &extras);
    assert!(
        matches!(prepare_hold(&mut pb, claim, verifier, 0, poster, 9, 101, false, &[]).unwrap(), FeeDisposition::Fate(0)),
        "legacy fate can collect only the real collateral"
    );
    assert!(prepare_hold(&mut pb, claim, verifier, 0, poster, 17, 101, false, &[]).is_err(), "never mint the uncollectible remainder");
    let mut late = parent.kernel_route.as_ref().unwrap().ledger_checked_v1().unwrap();
    late.preview_mut_v1().claims.get_mut(&claim).unwrap().life.state = ClaimStateV1::Convicted { daa: 101 };
    parent.kernel_route.as_mut().unwrap().rows = late.to_rows();
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    assert!(prepare_hold(&mut b, claim, verifier, 0, poster, 17, 101, false, &[]).is_err(), "late fate cannot mint fee entitlement");
    late.preview_mut_v1().claims.get_mut(&claim).unwrap().life.state = ClaimStateV1::Final { final_daa: 90 };
    parent.kernel_route.as_mut().unwrap().rows = late.to_rows();
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    assert!(prepare_hold(&mut b, claim, verifier, 0, poster, 17, 99, false, &[]).is_err(), "Final is not producer fault");
}

#[test]
fn ordinary_clean_consent_funds_one_full_slot_before_future_slashes() {
    use misaka_palw_kernel::{ledger::BondRowV1, verifier_pay::*};
    let state = with_route(PalwChainStateV2::genesis());
    let mut ledger = state.kernel_route.as_ref().unwrap().ledger_checked_v1().unwrap();
    let claim = [7; 64];
    let poster = [8; 64];
    let verifier = [9; 64];
    let l = ledger.preview_mut_v1();
    l.daa = 90;
    l.bonds.insert(poster, BondRowV1 { collateral: 40, reserved: 34, exit_requested: None });
    l.verifier_pay.insert(
        (VERIFIER_PAY_DRAW_V1, claim),
        VerifierPayRowV1::Draw { job: [5; 64], poster, fee: 17, slots: vec![(verifier, false), (verifier, false)], deadline_daa: 100 },
    );
    ledger.fund_consented_slot_reserved_v1(&claim, &verifier, 1, 17).unwrap();
    assert_eq!((ledger.bonds[&poster].collateral, ledger.bonds[&poster].reserved), (23, 17));
    assert!(
        matches!(ledger.verifier_pay.get(&(VERIFIER_PAY_DRAW_V1,claim)),Some(VerifierPayRowV1::Draw{slots,..}) if slots==&vec![(verifier,false),(verifier,true)])
    );
    assert!(ledger.fund_consented_slot_reserved_v1(&claim, &verifier, 1, 17).is_err(), "same slot cannot fund twice");
    ledger.preview_mut_v1().bonds.get_mut(&poster).unwrap().collateral = 9;
    let before = ledger.to_rows();
    assert!(
        ledger.fund_consented_slot_reserved_v1(&claim, &verifier, 0, 17).is_err(),
        "a new Clean consent cannot silently hold nine of seventeen"
    );
    assert_eq!(ledger.to_rows(), before, "underfunded consent leaves draw and balances unchanged");
}

#[test]
fn verifier_draw_zero_cash_and_refund_cancel_original_r_but_funded_hold_survives() {
    use misaka_palw_kernel::verifier_pay::{VERIFIER_PAY_DRAW_V1,VerifierPayRowV1};
    let p=params().with_settlement_once_from(Some(10)).with_bond_budget(Some(crate::palw_bond_budget_v1::PalwBondBudgetMirrorV1{
        from_daa:10,policy:crate::palw_values_p4_v1::palw_bond_budget_policy_provisional_v1(),allocation:None,
    }));
    let extras=PalwTransitionExtrasV1::default();
    for held in [false,true] {
        let mut parent=with_route(PalwChainStateV2::genesis());
        parent.bonds.insert(bond_key(1),PalwBondStateV2{pubkey:vec![7;4],operator_id:h64(1),collateral:1000,slashed:0,
            status:PalwBondStatusV2::Active,registered_daa:0,payout_payload:h64(21),capable_classes:Default::default()});
        let mut setup=TransitionBuilder::new(&parent,&p,false,false,false,false,&extras);
        let verifier=setup.note_kernel_bond(&bond_key(1)).unwrap();
        let claim=[81;64];let context=crate::palw_verifier_fee_consent_v1::verifier_fee_income_context_v1(claim,verifier,0);
        setup.reserve_source_income_v1(&ctx(1,90,90),context,Hash64::from_bytes(claim),bond_key(1),100,17).unwrap();
        let income=super::super::palw_bond_budget_fold_v1::source_income_id_v1(context,Hash64::from_bytes(claim),&bond_key(1));
        let mut before=setup.state.kernel_route.as_ref().unwrap().ledger_checked_v1().unwrap();
        before.preview_mut_v1().verifier_pay.insert((VERIFIER_PAY_DRAW_V1,claim),VerifierPayRowV1::Draw{
            job:[82;64],poster:[83;64],fee:17,slots:vec![(verifier,false)],deadline_daa:100,
        });
        setup.state.kernel_route.as_mut().unwrap().rows=before.to_rows();
        if held {
            let row=crate::palw_paid_claim_trap_v1::PalwHeldCheckFeeRowV1{version:1,claim,verifier,slot:0,poster:[83;64],fee:17,release_after_daa:100};
            setup.write_kernel_row_encoded_v1(142,&crate::palw_paid_claim_trap_v1::palw_paid_trap_held_fee_key_v1(&claim,&verifier,0),Some(&row)).unwrap();
        }
        let mut after=setup.state.kernel_route.as_ref().unwrap().ledger_checked_v1().unwrap();
        after.preview_mut_v1().daa=101;
        after.preview_mut_v1().verifier_pay.remove(&(VERIFIER_PAY_DRAW_V1,claim));
        super::super::palw_verifier_fee_consent_fold_v1::reconcile_closed_draws(&mut setup,&after).unwrap();
        assert_eq!(setup.state.bond_budget.as_ref().unwrap().claim_row(&income).is_some_and(|r|r.live),held);
        assert!(setup.state.pending_payouts.is_empty(),"slot closure cannot mint a fee");
    }
}
