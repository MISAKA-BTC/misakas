//! Historical OPV settlement remains readable, but lifecycle Final supplies no
//! selected approval for EXEC credit, its root Final, or beacon influence.
//! Conviction/default still adjudicate the dependency. The inherited fixture
//! uses a test-only REAL admission seam; it is not an all-route acceptance run.
//! Source controls only: run during consolidated validation.
use super::*;

/// The claim read's settlement standing and first settled DAA.
fn settlement_of(cw: &Cw, claim: &Digest) -> (&'static str, Option<u64>) {
    let read = cw.net.api().expect("the route").claim_read_v1(claim).unwrap().expect("the claim");
    (read.settlement, read.settled_from_daa)
}

/// The beacon's Final fact for `claim` (`finals_read_v1`, the source of RFC-0010 V3's chain history), with its settlement label.
fn beacon_fact_of(cw: &Cw, claim: &Digest) -> Option<(&'static str, misaka_palw_challenge::WorkFinalEventV1)> {
    let finals = cw.net.api().expect("the route").finals_read_v1().unwrap();
    let f = finals.into_iter().find(|f| f.receipt.claim == *claim)?;
    let work: misaka_palw_challenge::AttributedWorkV1 = borsh::from_slice(f.event.as_ref()?).unwrap();
    Some((f.settlement, work.event))
}

/// Legacy settlement advances while an unapproved EXEC dependency stays Pending.
#[tokio::test]
async fn x8_g14_opv_settlement_never_substitutes_for_selected_exec_approval() {
    let mut cw = active_world().await;
    // Past the paid path every session claim is a paid claim; slice 0's is locked but never receipted, so its OPV lifecycle runs to
    // Final while its paid authority stays unapproved. The root's signed lifetime outlasts that Final.
    cw.unapproved = vec![1];
    let mut s = opened(&mut cw, false, |_| false, |_, _, _| Vec::new()).await;
    for index in 0..3 {
        s.lane.push(carry_slice(&mut cw, s.real.claim, index as u32, &s.slices[index].claim).await);
    }
    let root_claim = s.real.claim;
    let k0 = s.slices[0].claim.id;
    assert_eq!(settlement_of(&cw, &k0), ("pending", None), "a Challengeable claim is pending");
    assert!(matches!(cw.net.chain.tip_state().1.exec_v2_slice_v1(&root_claim, 0).unwrap().stage, PalwWorkSliceStageV2::Pending));
    assert!(beacon_fact_of(&cw, &k0).is_none(), "no beacon fact before Final");

    let api = cw.net.api().unwrap();
    let floor = s
        .slices
        .iter()
        .chain(std::iter::once(&s.prefix))
        .map(|k| kernel_final_due(&api, &k.claim.id))
        .max()
        .unwrap();
    cw.net.beat_to(floor + 1).await;
    // A paid claim's Final lands at the close of its signed windows: beat to it.
    let ttpb = cw.net.ttpb();
    for _ in 0..2_000 {
        if matches!(cw.net.claim_state(&k0), ClaimStateV1::Final { .. }) {
            break;
        }
        cw.net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    let (label, from) = settlement_of(&cw, &k0);
    // Past the paid path an unapproved paid claim reaches no Final at all: its locked round without receipts defaults (the stronger
    // property — no lifecycle settlement exists to substitute for the selected approval).
    if label == "defaulted" {
        assert!(beacon_fact_of(&cw, &k0).is_none(), "a defaulted unapproved claim exports no beacon fact");
        assert!(
            !matches!(
                cw.net.chain.tip_state().1.exec_v2_slice_v1(&root_claim, 0).unwrap().stage,
                PalwWorkSliceStageV2::Verified { .. }
            ),
            "an unapproved dependency never verifies its slice"
        );
        // A defaulted dependency voids the root (as the withheld-slice test asserts), and a Voided root no longer "holds" Final —
        // it can never reach one. Either form means the unapproved work released no root Final.
        let tip = cw.net.chain.tip_state().1;
        assert!(
            tip.exec_v2_holds_final_v1(&root_claim)
                || matches!(tip.exec_v2_root_v1(&root_claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { .. }),
            "unapproved work cannot release root Final: {:?}",
            tip.exec_v2_root_v1(&root_claim).unwrap().phase
        );
        let z = replay_with_lane(&cw.net, &s.lane).await;
        cw.net.assert_same(&z, "replay");
        return;
    }
    assert_eq!(label, "final-revocable", "Final within liability is revocable, never safe");
    let liability = cw.net.ledger().claims[&k0].liability_until.expect("a Final claim's horizon");
    assert_eq!(from, Some(liability + 1));
    assert!(matches!(cw.net.chain.tip_state().1.exec_v2_slice_v1(&root_claim, 0).unwrap().stage, PalwWorkSliceStageV2::Pending));
    assert!(beacon_fact_of(&cw, &k0).is_none(), "optimistic settlement supplies no selected-verifier approval");

    assert!(cw.net.chain.tip_state().1.exec_v2_holds_final_v1(&root_claim), "unapproved work cannot release root Final");
    cw.net.beat_to(liability + 1).await;
    assert_eq!(settlement_of(&cw, &k0), ("settled", Some(liability + 1)), "past the horizon, unconvicted: settled");
    // (The slice's stage past the horizon is the EXEC root's business — the REAL root claim's own Final needs the legacy channel this
    // harness cannot prove, module doc of the parent — so only the kernel claim's standing is asserted here.)
    assert!(beacon_fact_of(&cw, &k0).is_none(), "settlement finality does not create a selected approval");
    let z = replay_with_lane(&cw.net, &s.lane).await;
    cw.net.assert_same(&z, "replay");
}

/// **A conviction revokes**: slice 1's lying OPV claim, convicted by an outsider before its Final — the claim read says `convicted`,
/// the slice is proven false, the suffix and the root void; nothing about it ever reads safe.
#[tokio::test]
async fn x8_g14_rfc15_a_conviction_revokes_the_slice_through_the_settlement_predicate() {
    let mut cw = active_world().await;
    let s = session(&mut cw, false, |index| index == 1).await;
    let root_claim = s.real.claim;
    let lie = &s.slices[1].claim;
    let api = cw.net.api().expect("the read API serves the route");
    let fresh = Fresh::from_api(&api, api.ledger_root(), 0x5D);
    let proof = match fresh.check(lie.id, &lie.published(&cw.cand, &[]), &cw.cand.params) {
        OutsiderFindingV1::Prosecute(proof) => proof,
        other => panic!("the fresh verifier should prosecute slice 1's claim: {other:?}"),
    };
    let o = cw.net.route(ACCUSER, &K::FileProof { accuser: cw.net.kid(ACCUSER), claim: lie.id, proof });
    cw.net.send(vec![(ACCUSER, o)]).await;
    assert_eq!(settlement_of(&cw, &lie.id), ("convicted", None));
    let state = cw.net.chain.tip_state().1;
    assert!(matches!(state.exec_v2_slice_v1(&root_claim, 1).unwrap().stage, PalwWorkSliceStageV2::ProvenFalse { .. }));
    assert!(matches!(state.exec_v2_root_v1(&root_claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { from_index: 1, .. }));
    assert!(
        beacon_fact_of(&cw, &lie.id).is_none_or(|(l, e)| l == "convicted" && !e.claim_final),
        "a convicted claim is no standing fact"
    );
}

/// **A default revokes**: slice 1's OPV claim is demanded by an outsider at one position and the executor never serves it; at the
/// deadline the claim read says `defaulted` and the slice is `Defaulted` (availability, never fraud), the suffix and the root void.
#[tokio::test]
async fn x8_g14_rfc15_a_default_revokes_the_slice_through_the_settlement_predicate() {
    let mut cw = active_world().await;
    let s = session(&mut cw, false, |_| false).await;
    let root_claim = s.real.claim;
    let withheld = &s.slices[1].claim;
    let o =
        cw.net.route(ACCUSER, &K::FileDemand { demander: cw.net.kid(ACCUSER), claim: withheld.id, stage: 0, position: withheld.at.0 });
    cw.net.send(vec![(ACCUSER, o)]).await;
    let deadline = cw.net.ledger().demands[&(withheld.id, 0, withheld.at.0)].deadline_daa;
    cw.net.beat_to(deadline).await;
    assert!(
        matches!(cw.net.claim_state(&withheld.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
        "{:?}",
        cw.net.claim_state(&withheld.id)
    );
    assert_eq!(settlement_of(&cw, &withheld.id), ("defaulted", None));
    let state = cw.net.chain.tip_state().1;
    assert!(matches!(state.exec_v2_slice_v1(&root_claim, 1).unwrap().stage, PalwWorkSliceStageV2::Defaulted { .. }));
    assert!(matches!(state.exec_v2_root_v1(&root_claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { from_index: 1, .. }));
}
