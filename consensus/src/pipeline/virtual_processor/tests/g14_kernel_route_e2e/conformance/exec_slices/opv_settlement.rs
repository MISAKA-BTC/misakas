//! **RFC-0015 §9 × RFC-0008 v2 on the real node: an EXEC work slice verified by an OPV kernel claim follows the ONE kernel settlement
//! predicate** (`kaspa_consensus_core::palw_kernel_settlement_v1`) — the same standing the claim read / RPC (`settlement`), the beacon's
//! Final facts (`finals_read_v1`, RFC-0010 V3's `ChainPanelBeaconHistoryV1`) and the slice's own stage report:
//!
//! ```text
//! committed (Challengeable)          settlement pending          slice Pending
//! window closed, Final               settlement final-revocable  slice Verified; no selected-approval beacon fact
//! past liability_until               settlement settled          slice Verified; safe
//! conviction (pre-Final)             settlement convicted        slice ProvenFalse; beacon fact withdrawn
//! demanded, unserved, deadline       settlement defaulted        slice Defaulted
//! ```
//!
//! No extra weight or reward anywhere: kernel claims reserve `B = F = 0` (the slice's leg is the EXEC root's, capped at the claim's
//! reservation). Test source, 2026-10-15; NOT RUN in the change that wrote it (no cargo invocation was allowed). The harness's seams are
//! the parent module's (`exec_v2_test_admit_class_v1` for the REAL root's admission).
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

/// **pending → final-revocable → settled**: an honest slice's OPV kernel claim, read three ways at each step (claim read, beacon
/// fact, slice stage), agrees with itself; Final is never shown safe before the claim's liability horizon.
#[tokio::test]
async fn x8_g14_rfc15_a_slice_follows_its_opv_claims_settlement_from_pending_to_final_revocable_to_settled() {
    let mut cw = active_world().await;
    let mut s = opened_with_lifetime(&mut cw, false, |_| false, |_, _, _| Vec::new(), 300).await;
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
        .map(|k| api.claim_read_v1(&k.claim.id).unwrap().unwrap().opv.expect("an OPV claim").final_floor_daa)
        .max()
        .unwrap();
    cw.net.beat_to(floor + 1).await;
    let (label, from) = settlement_of(&cw, &k0);
    assert_eq!(label, "final-revocable", "Final within liability is revocable, never safe");
    let liability = cw.net.ledger().claims[&k0].liability_until.expect("a Final claim's horizon");
    assert_eq!(from, Some(liability + 1));
    assert!(matches!(
        cw.net.chain.tip_state().1.exec_v2_slice_v1(&root_claim, 0).unwrap().stage,
        PalwWorkSliceStageV2::Verified { .. }
    ));
    assert!(beacon_fact_of(&cw, &k0).is_none(), "optimistic settlement supplies no selected-verifier approval");

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
