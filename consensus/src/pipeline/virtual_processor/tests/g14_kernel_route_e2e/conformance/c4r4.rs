//! **C4 round 4 on the real node: tag 109 and the block's adjudication budget.**
//!
//! A conformance `Refute` (tag 109) is judged in the onboarding fold and spends the SAME per-block budget the kernel route's objects
//! spend (`charge_route_budget_v1`, `consensus/core/src/palw_kernel_route_fold_v1.rs`). Two things about that charge:
//!
//! * **F-C4R4-10 (P1 where armed).** It compares the runs with `max_adjudications_per_block` — the whole block — while the kernel's
//!   own `charge` lets every object but a `FileProof` stop short of `prosecution_reserved_runs()` (G14-R4's F-C4R3-05 round-2 fix: "no
//!   flood of claims leaves an outsider without a run"). Refutations are open to any Active bond of another operator and a refutation
//!   that proves nothing is dismissed with NO fee, so a block's reserved proof runs are spent for free and every `FileProof` after them
//!   is `OverBudget`.
//! * **F-C4R4-11 (P1 where armed; fixed by OPVB: a refutation may spend the proof reserve, is judged once per bond per window, and
//!   pays `dismissed_proof_fee` when it proves nothing).** For the same reason (free dismissed refutations), junk refutations ahead of a valid one in
//!   every block of the evidence's 80-DAA window keep FORGED conformance evidence unrefuted until it passes: the class reaches
//!   G14_ELIGIBLE on forged evidence for nothing but carrier fees. The reserve fix does not close this (honest and junk refutations
//!   share the runs left); a dismissal fee for a refutation that proves nothing (as a `FileProof` has) does.

use super::*;
use kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1;

/// The harness's block cap (asserted by the lane's own tests) and the share of it the kernel reserves for proofs.
fn cap_and_reserved() -> (u32, u32) {
    let mut p = palw_kernel_route_policy_v1(Hash64::from_bytes([0; 64]), Hash64::from_bytes([0; 64]));
    p.max_adjudications_per_block = 4;
    (p.max_adjudications_per_block, p.prosecution_reserved_runs())
}

/// **F-C4R4-10, as the F-C4R4-11 fix restates it: junk conformance refutations never spend the runs reserved for proofs for FREE.**
/// Four refutations that prove nothing (the TRUE opening of an honestly reported leaf), from four bonds, in one block of the
/// four-adjudication harness. A refutation is a proof: like a `FileProof` it may take a reserved run, and like a dismissed `FileProof`
/// one that proves nothing pays `dismissed_proof_fee` — and its bond is judged once per evidence window (OPVB, on the Lead's sketch;
/// the round-4 fix had them stop short of the reserve, which F-C4R4-11 showed is not enough). SAFE: every run a junk refutation took
/// was paid for, and a repeat from the same bond is refused before any charge.
#[tokio::test]
async fn g14_c4r4_junk_conformance_refutations_must_not_spend_the_runs_reserved_for_proofs() {
    kaspa_core::log::try_init_logger("warn");
    let (cap, reserved) = cap_and_reserved();
    assert!(reserved > 0);
    let mut cw = Cw::new().await;
    cw.commit(0x22).await;
    cw.source_claims(2).await;
    cw.until_locked().await;
    let (_, _, selection) = cw.selection();
    cw.send_post(cw.post(|_, _| {}), None).await;
    let posted = cw.attempt();
    let evidence_id = posted.evidence.expect("posted").evidence_id;
    let junk: Vec<(usize, Obj)> = [OUTSIDER, 0, 2, 4]
        .into_iter()
        .map(|card| {
            let fault = cw.leaf_fault(&selection, 1); // an honest leaf: the refutation proves nothing
            (card, cw.evidence_object(card, ConformanceEvidenceActionV1::Refute { evidence_id, fault: Box::new(fault) }))
        })
        .collect();
    assert_eq!(junk.len() as u32, cap);
    let fee = cw.net.api().unwrap().header.policy.dismissed_proof_fee;
    let cards: Vec<usize> = junk.iter().map(|(card, _)| *card).collect();
    let slashed = |cw: &Cw, card: usize| cw.net.chain.tip_state().1.bond(&cw.net.bond(card)).expect("the bond").slashed;
    let before: Vec<u64> = cards.iter().map(|card| slashed(&cw, *card)).collect();
    let repeat = junk[0].clone();
    cw.net.send(junk).await;
    assert_eq!(decided(&cw.attempt()), decided(&posted), "every junk refutation is dismissed");
    let (_, adjudications, _): (u64, u32, u64) = borsh::from_slice(&cw.budget_row().expect("the budget row")).expect("decodes");
    let judged = cw.attempt().refuters_judged;
    eprintln!("[F-C4R4-10] {adjudications} of {cap} runs ({reserved} reserved for proofs) taken by junk refutations, each paid {fee}");
    assert_eq!(judged.len() as u32, adjudications, "every run a refutation took is a judged bond");
    for (i, card) in cards.iter().enumerate() {
        if judged.contains(&cw.net.bond(*card)) {
            assert_eq!(slashed(&cw, *card) - before[i], fee, "card {card}: a dismissed refutation pays dismissed_proof_fee");
        }
    }
    let rows = cw.net.api().unwrap().aux.clone();
    cw.net.send(vec![repeat]).await;
    assert_eq!(cw.net.api().unwrap().aux, rows, "a bond's second refutation in the window is refused before any charge");
}

/// **F-C4R4-11: free junk refutations must not keep forged conformance evidence unrefuted.** The registrant posts consistently forged
/// evidence (the lane's own forgery: one leaf outcome flipped). In every block of its window the attacker's bonds send `cap` junk
/// refutations, then the outsider its valid one. SAFE: the forgery is refuted (or never passes).
#[tokio::test]
async fn g14_c4r4_free_junk_refutations_must_not_carry_forged_evidence_through_its_window() {
    kaspa_core::log::try_init_logger("warn");
    let (cap, _) = cap_and_reserved();
    let mut cw = Cw::new().await;
    cw.commit(0x22).await;
    cw.source_claims(2).await;
    cw.until_locked().await;
    let (_, _, selection) = cw.selection();
    let forged = cw.post(|id, o| {
        if id == "leaf/r0/k0" {
            for r in [&mut o.reference, &mut o.independent, &mut o.backend] {
                if let ResultV1::Ran { a, .. } = r {
                    a[0] ^= 1;
                }
            }
        }
    });
    cw.send_post(forged, None).await;
    let posted = cw.attempt().evidence.expect("the forgery entered its window");
    let evidence_id = posted.evidence_id;
    let mut blocks = 0u32;
    while cw.net.daa() < posted.window_end_daa && !matches!(cw.attempt().last_end, Some((ConformanceAttemptEndV1::Refuted, _))) {
        let mut items: Vec<(usize, Obj)> = [0usize, 2, 4, 5]
            .into_iter()
            .take(cap as usize)
            .map(|card| {
                let fault = cw.leaf_fault(&selection, 1);
                (card, cw.evidence_object(card, ConformanceEvidenceActionV1::Refute { evidence_id, fault: Box::new(fault) }))
            })
            .collect();
        let valid = cw.leaf_fault(&selection, 0);
        items.push((
            OUTSIDER,
            cw.evidence_object(OUTSIDER, ConformanceEvidenceActionV1::Refute { evidence_id, fault: Box::new(valid) }),
        ));
        cw.net.send(items).await;
        blocks += 1;
    }
    cw.net.beat_to(posted.window_end_daa + 1).await;
    let (s, _, _) = state(&cw);
    eprintln!(
        "[F-C4R4-11] {blocks} blocks of {cap} junk refutations + 1 valid: the forged evidence ends {s:?}, last end {:?}",
        cw.attempt().last_end
    );
    assert!(
        matches!(cw.attempt().last_end, Some((ConformanceAttemptEndV1::Refuted, _))),
        "forged evidence passed its window unrefuted ({s:?}): the valid refutation never found a run"
    );
}

// ── A-2 completeness: tag 113 below its fence ───────────────────────────────────────────────────────────────────────────────

/// **C4 round 4, A-2 completeness: tag 113 (`KernelRouteChunkV1`, G14-R4's F-C4R3-03 fix, `a09c6119d`) is a member A2U's table does
/// not list** (A2U's base `35a9ae1c8` predates it). testnet-12's live int-12 cannot decode tag 113: under the audit fence it tolerates
/// the carrier and skips it. This build decodes it and runs its may-ride arm AT ISOLATION, with no height: an unsigned chunk, or one
/// whose index/count/part is out of range, is `ObjectMayNotRide` — the newer build marks invalid a block int-12 accepts (A2U's member
/// #3, for tag 110). A2U's exhaustive owner match will force a row for 113 when the branches meet; until then this pins the member.
/// SAFE: below its fence a tag-113 carrier gets the verdict of a payload no build decodes.
mod a2_tag113 {
    use kaspa_consensus_core::Hash64;
    use kaspa_consensus_core::palw_kernel_route_v1::{PalwKernelChunkTargetV1, PalwKernelChunkV1};
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{
        PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2, validate_palw_lifecycle_tx,
    };
    use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    fn carrier(signature: Vec<u8>, index: u8, count: u8) -> Vec<u8> {
        let chunk = PalwKernelChunkV1 {
            opener: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([7u8; 64]), 0)),
            group: Hash64::from_bytes([2; 64]),
            target: PalwKernelChunkTargetV1::Claim([3; 64]),
            index,
            count,
            bytes: vec![1, 2, 3],
        };
        let object = PalwConsensusObjectV2::KernelRouteChunkV1 { chunk: Box::new(chunk), signature };
        borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap()
    }

    #[test]
    #[ignore = "FAIL C4R4 A-2: tag 113's may-ride arm refuses at isolation what int-12 tolerates (A2U's central rule needs a 113 row)"]
    fn g14_c4r4_a2_a_tag_113_carrier_below_its_fence_is_judged_as_undecodable_bytes() {
        let signed = carrier(vec![9; 32], 0, 1);
        // A payload no build decodes: the same bytes with the object tag replaced by 254.
        let mut undecodable = signed.clone();
        assert_eq!(undecodable[2], 113, "version (u16), then the object tag");
        undecodable[2] = 254;
        let reference = validate_palw_lifecycle_tx(&undecodable, true);
        assert!(reference.is_ok(), "the audit fence tolerates undecodable bytes");
        for (what, bytes) in
            [("unsigned", carrier(vec![], 0, 1)), ("index past count", carrier(vec![9; 32], 3, 1)), ("signed", signed)]
        {
            let verdict = validate_palw_lifecycle_tx(&bytes, true);
            eprintln!("[C4R4 A-2] tag 113 {what}: {verdict:?} (int-12 reads undecodable bytes: {reference:?})");
            assert_eq!(verdict.is_ok(), reference.is_ok(), "{what}: the newer build's isolation verdict differs from int-12's");
        }
    }
}
