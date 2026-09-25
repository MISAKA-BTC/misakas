//! **ADR-0152 v3.1 Phase 2, P2-8e — T54g's processor half: the fold's rehearsal of ONE object**
//! (`VirtualStateProcessor::palw_object_rehearsal_v1_on`, what `ConsensusApi::palw_object_rehearsal_v1`
//! answers at the tip) on this suite's real claims, through the processor's own acceptance layer and
//! flags.
//!
//! kaspad's P2-8e filer asks it before it queues a held dissection's opening (`ShardCourtAccused` at a
//! fused-attention leaf), an object no H-1 gate rehearses. What this pins is that the read is the
//! fold's own answer: an object the walk and the fold take is `Accepted`; the same object once the
//! claim is convicted is the fold's refusal, typed; an object the acceptance layer refuses (a bond the
//! chain does not hold) is `NotAccepted` before any state is cloned. The held opening itself — the
//! 8k fixtures, the run, the book, the session played to `CourtHeldVerdict` — is kaspad's
//! `palw_filer_held` e2e, on the real transition.
use super::*;
use kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1 as Rehearsal;

/// **The rehearsal is the fold's answer**: T54f's kind-4 filing on a real garbage claim is `Accepted`
/// by the read and then by the walk and the fold; the same object after its conviction is refused by
/// the fold's own arm; a filing from a bond the chain does not hold is refused by the acceptance
/// layer. With `palw_rcore_plus` off (the twin) the same read answers the same way — it is a read, and
/// the fences it applies are the point's own.
#[tokio::test]
async fn t54g_the_object_rehearsal_is_the_folds_own_answer() {
    for rcore in [true, false] {
        let h = if rcore { harness(true) } else { harness_rcore_off() };
        let mut walk = h.genesis_walk();
        let claim = h.open_claim(&mut walk, Fault::Step);
        let id = claim.claim_id;
        h.bind(&mut walk, id);
        let object = h.refuted(id, claim.contradiction());
        let point = walk.next();
        assert_eq!(h.vp().palw_object_rehearsal_v1_on(&walk.state, h.sp(), &point, &object), Rehearsal::Accepted, "rcore {rcore}");
        // The acceptance layer first: an accusation by a bond the chain does not hold.
        let stranger = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
            kaspa_consensus_core::tx::TransactionId::from_u64_word(0x5714_A6E4),
            0,
        ));
        let foreign = kaspa_consensus_core::palw_da_rcore_v1::palw_da_accusation_object_v1(
            &h.domain,
            id,
            kaspa_consensus_core::palw_da_rcore_v1::PALW_DA_AUTO_NAMED_UNIT_V1,
            stranger,
            |message, context| Some(sign(PANEL[0], message, context)),
        )
        .expect("built");
        assert!(
            matches!(h.vp().palw_object_rehearsal_v1_on(&walk.state, h.sp(), &point, &foreign), Rehearsal::NotAccepted(_)),
            "rcore {rcore}: the acceptance layer refuses a bond it does not hold"
        );
        // Folded, the claim convicted: the same object is the fold's refusal.
        h.carry(&mut walk, vec![object.clone()]);
        let again = h.vp().palw_object_rehearsal_v1_on(&walk.state, h.sp(), &walk.next(), &object);
        assert!(matches!(again, Rehearsal::Refused(_) | Rehearsal::NotAccepted(_)), "rcore {rcore}: convicted once: {again:?}");
    }
}
