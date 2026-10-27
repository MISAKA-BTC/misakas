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
