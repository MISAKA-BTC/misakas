//! **ADR-0152 v3.1 H-1: the lifecycle objects a heartbeat must be able to carry** — the one list
//! the node half (P2-9) reads.
//!
//! V-8 lets a licence halt run for as long as its trigger lasts, and a row that outlives
//! `F + 9,000` matures at the first licence after the halt — so the conviction that should stop it
//! has to land DURING the halt, when the chain may be minting nothing but heartbeats. H-1 therefore
//! binds three parties to one set of objects: the fold applies them at step 3 in any block (C-7,
//! Phase 1's half), the heartbeat miner includes them ahead of better-paying traffic, and the H2
//! relay allowance never drops a heartbeat for carrying them. The two node parties —
//! `kaspa-mining`'s carrier lane (`TransactionsPool::build_palw_carrier_lane`) and
//! `kaspa-p2p-flows`' relay exemption (`FlowContext::palw_heartbeat_h1_exempt`) — both ask
//! [`palw_h1_carrier_tx_v1`], so what the miner hurries and what the relay spares cannot drift apart.
//!
//! **Policy, never a rule.** Nothing in validation or the fold reads this module; a block with or
//! without these objects is exactly as valid as before, and no id, root or fingerprint moves. It
//! decides only which paid transactions a template takes first and which heartbeat the relay
//! downloads past a peer's allowance.
//!
//! **A carrier is what the extractor would yield.** [`palw_h1_carrier_tx_v1`] runs
//! `palw_lifecycle_objects_from_accepted_txs_v2` itself — the walk the fold reads — rather than a
//! cheaper tag peek, because admission TOLERATES an undecodable 0x4b payload past the audit fence
//! (A-2): a tag byte followed by garbage would be a carrier to a peek and nothing to the fold, and
//! the lane would be selling priority to bytes that fold nothing. Only the object's KIND is judged
//! here; whether it is admissible in the chain's state (the claim exists, the session is open, the
//! signature verifies) is the acceptance layer's and cannot be asked of a mempool. A kind-valid
//! object the fold later refuses still pays its fee — the lane's budget, not this list, is what
//! bounds that (`PALW_H1_CARRIER_LANE_MASS_DIVISOR` in `kaspa-mining`).
//!
//! **Why exactly these** (H-1's own enumeration, ADR-0152 §3.3):
//! * conviction-bearing filings — `ObjectiveOffence` of kind `ExecutorEquivocation`,
//!   `PanelFalseValidV2` and `ExecutorRefuted`, the three kinds a reporter files past
//!   `palw_offence_attribution` (which R-core+ requires at or below itself). `PanelFalseValid` (V1)
//!   is refused past that fence, `CourtExecutorGuilty` is filed by `CourtClosed` and never
//!   standalone, and `DaDefault` / `CourtConviction` are rows only the fold writes — none of them
//!   can fold, so none of them may buy the lane;
//! * DA — `DefaultAccused`, `DefaultAccusedHeld`, `MaterialDisclosedV2` (DA-1: V2 replaces both
//!   older disclosures past R-core+, so they are not carriers here);
//! * the reporter's commit–reveal — `ReporterCommitted`, `ReporterRevealed`;
//! * the court — `CourtOpened` and the session's moves (close, its declaration and chunks, the
//!   bisection rungs and the k-ary dissection's moves). Whether the phase allows a move is the fold's.
//!
//! Everything else stays in the fee market, deliberately: licences and receipts compete there, and
//! the lane taking at most half a block is what keeps a DA or disclosure storm from starving them
//! (the Phase 2 plan's §5.7 warning). The one-move and checkpoint courts' accusations and the round
//! lane's equivocation evidence are outside H-1's enumeration; adding one is one arm below. The
//! match is exhaustive on purpose: whoever appends an object kind decides here whether a heartbeat
//! must carry it.

use crate::palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2;
use crate::palw_offence_v1::PalwOffenceKindV1;
use crate::palw_state_v2::PalwConsensusObjectV2;
use crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
use crate::tx::{Transaction, TransactionId};

/// Whether `object` is one H-1 obliges a heartbeat to carry. See the module doc for each arm.
pub fn palw_h1_carrier_object_v1(object: &PalwConsensusObjectV2) -> bool {
    use PalwConsensusObjectV2 as O;
    match object {
        O::ObjectiveOffence { kind, .. } => match kind {
            PalwOffenceKindV1::ExecutorEquivocation | PalwOffenceKindV1::PanelFalseValidV2 | PalwOffenceKindV1::ExecutorRefuted => {
                true
            }
            PalwOffenceKindV1::PanelFalseValid
            | PalwOffenceKindV1::CourtExecutorGuilty
            | PalwOffenceKindV1::DaDefault
            | PalwOffenceKindV1::CourtConviction => false,
        },
        O::DefaultAccused { .. } | O::DefaultAccusedHeld { .. } | O::MaterialDisclosedV2 { .. } => true,
        O::ReporterCommitted { .. } | O::ReporterRevealed { .. } => true,
        O::CourtOpened { .. }
        | O::CourtClosed { .. }
        | O::CourtDisclosed { .. }
        | O::CourtVerdictPosted { .. }
        | O::CourtCloseDeclared { .. }
        | O::CourtCloseChunk { .. }
        | O::CourtAttnRootClaimed { .. }
        | O::CourtAttnRootClaimedAnchored { .. }
        | O::CourtAttnDissected { .. }
        | O::CourtAttnChildChosen { .. } => true,
        O::BondRegistered { .. }
        | O::BondCapabilityDeclared { .. }
        | O::BondRetireRequested { .. }
        | O::ClassRegistered { .. }
        | O::ClassFrozen(..)
        | O::PanelBound { .. }
        | O::ReceiptLicensed { .. }
        | O::ProducerDefaulted { .. }
        | O::FreePromptCommitted { .. }
        | O::FamilyCertified { .. }
        | O::ClassLaneCertified { .. }
        | O::ObjectChunk { .. }
        | O::DerivedArtifactV1 { .. }
        | O::MaterialDisclosed { .. }
        | O::ModelBuy { .. }
        | O::ModelSell { .. }
        | O::ModelLineFounded { .. }
        | O::ModelVersionPublished { .. }
        | O::ModelVersionPromoted { .. }
        | O::ModelVersionWithdrawn { .. }
        | O::ModelLineRolesSet { .. }
        | O::ModelLineOwnerTransferred { .. }
        | O::ModelLineRetired { .. }
        | O::ModelProposalPosted { .. }
        | O::ModelProposalClosed { .. }
        | O::ModelEvaluationPosted { .. }
        | O::ModelSeed { .. }
        | O::ModelLineBenefitsDeclared { .. }
        | O::ShardCourtAccused { .. }
        | O::ClassShardPlanDeclared { .. }
        | O::BondShardsDeclared { .. }
        | O::ShardReceiptLicensed { .. }
        | O::CheckpointAccused { .. }
        | O::MaterialDisclosedHeld { .. }
        | O::RoundPermitEquivocated { .. }
        | O::SeatReadinessProved { .. }
        | O::ClassManifestV2 { .. }
        | O::ReceiptLicensedV2 { .. }
        | O::SeatReadinessProvedV2 { .. }
        | O::OptimisticLicensed { .. }
        | O::PanelUnavailableQuorum { .. } => false,
    }
}

/// Whether `tx` carries an H-1 object: a 0x4b transaction the extraction walk turns into an object
/// [`palw_h1_carrier_object_v1`] names. Decodes the payload once; any other subnetwork is answered
/// without decoding.
pub fn palw_h1_carrier_tx_v1(tx: &Transaction) -> bool {
    tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE
        && palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(tx))
            .objects
            .first()
            .is_some_and(|carried| palw_h1_carrier_object_v1(&carried.object))
}

/// The ids of the H-1 carriers among `txs`, in order — what a heartbeat body is judged by.
pub fn palw_h1_carrier_ids_v1(txs: &[Transaction]) -> Vec<TransactionId> {
    txs.iter().filter(|tx| palw_h1_carrier_tx_v1(tx)).map(|tx| tx.id()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    use crate::palw_state_v2::PalwBondKeyV2;
    use crate::subnets::SUBNETWORK_ID_NATIVE;
    use crate::tx::{ScriptPublicKey, TransactionOutpoint, TransactionOutput};
    use kaspa_hashes::Hash64;

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(n), 0))
    }

    fn tx_with(subnetwork: crate::subnets::SubnetworkId, payload: Vec<u8>) -> Transaction {
        Transaction::new(
            0,
            vec![],
            vec![TransactionOutput::new(1, ScriptPublicKey::from_vec(0, vec![0x51]))],
            0,
            subnetwork,
            0,
            payload,
        )
    }

    fn carrier(object: PalwConsensusObjectV2) -> Transaction {
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object })
            .expect("a lifecycle payload serializes");
        tx_with(SUBNETWORK_ID_PALW_LIFECYCLE, payload)
    }

    fn offence(kind: PalwOffenceKindV1) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::ObjectiveOffence { kind, accused: bond(1), evidence_id: Hash64::from_u64_word(2), evidence: vec![3] }
    }

    /// **T38 (node half): the three conviction kinds a reporter files ride the lane, and the four that
    /// cannot fold on an R-core+ chain do not.** A kind the fold refuses by name is exactly what a
    /// spammer would file to buy priority for nothing.
    #[test]
    fn the_filed_convictions_are_carriers_and_the_unfileable_kinds_are_not() {
        for kind in [PalwOffenceKindV1::ExecutorEquivocation, PalwOffenceKindV1::PanelFalseValidV2, PalwOffenceKindV1::ExecutorRefuted]
        {
            assert!(palw_h1_carrier_tx_v1(&carrier(offence(kind))), "{kind:?} is an H-1 carrier");
        }
        for kind in [
            PalwOffenceKindV1::PanelFalseValid,
            PalwOffenceKindV1::CourtExecutorGuilty,
            PalwOffenceKindV1::DaDefault,
            PalwOffenceKindV1::CourtConviction,
        ] {
            assert!(!palw_h1_carrier_tx_v1(&carrier(offence(kind))), "{kind:?} cannot fold here, so it buys nothing");
        }
    }

    /// DA and the reporter's commit–reveal ride the lane; a licence, a quorum of `Unavailable` and a
    /// market move compete for fees like any transaction.
    #[test]
    fn da_and_reporter_objects_ride_and_licences_do_not() {
        let accused = PalwConsensusObjectV2::DefaultAccused {
            claim: Hash64::from_u64_word(7),
            missing_event_index: 0,
            accuser: bond(2),
            signature: vec![1; 8],
        };
        let committed = PalwConsensusObjectV2::ReporterCommitted {
            commitment: Hash64::from_u64_word(8),
            reporter: bond(3),
            signature: vec![1; 8],
        };
        let revealed =
            PalwConsensusObjectV2::ReporterRevealed { offence_key: Hash64::from_u64_word(9), reporter: bond(3), salt: [4; 32] };
        for object in [accused, committed, revealed] {
            assert!(palw_h1_carrier_tx_v1(&carrier(object.clone())), "{object:?}");
        }
        let quorum = PalwConsensusObjectV2::PanelUnavailableQuorum { claim: Hash64::from_u64_word(10), receipts: vec![] };
        let licence = PalwConsensusObjectV2::ReceiptLicensed { claim: Hash64::from_u64_word(11), receipts: vec![] };
        for object in [quorum, licence] {
            assert!(!palw_h1_carrier_tx_v1(&carrier(object.clone())), "{object:?} stays in the fee market");
        }
    }

    /// **A carrier is what the extraction walk yields, never a tag.** An undecodable payload (which
    /// admission tolerates past the audit fence), a payload at an unknown wire version, a kind the
    /// may-ride table refuses (an unsigned accusation), and the right bytes on another subnetwork
    /// are all nothing to the fold — and so nothing to the lane or the relay.
    #[test]
    fn only_what_the_extractor_yields_is_a_carrier() {
        let signed = PalwConsensusObjectV2::DefaultAccused {
            claim: Hash64::from_u64_word(7),
            missing_event_index: 0,
            accuser: bond(2),
            signature: vec![1; 8],
        };
        let good = carrier(signed.clone());
        assert!(palw_h1_carrier_tx_v1(&good));

        let mut truncated = good.payload.clone();
        truncated.truncate(truncated.len() / 2);
        assert!(!palw_h1_carrier_tx_v1(&tx_with(SUBNETWORK_ID_PALW_LIFECYCLE, truncated)), "undecodable");

        let future = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2 + 1, object: signed })
            .expect("a lifecycle payload serializes");
        assert!(!palw_h1_carrier_tx_v1(&tx_with(SUBNETWORK_ID_PALW_LIFECYCLE, future)), "unknown wire version");

        let unsigned = carrier(PalwConsensusObjectV2::DefaultAccused {
            claim: Hash64::from_u64_word(7),
            missing_event_index: 0,
            accuser: bond(2),
            signature: vec![],
        });
        assert!(!palw_h1_carrier_tx_v1(&unsigned), "the may-ride table refuses an unsigned accusation");

        assert!(!palw_h1_carrier_tx_v1(&tx_with(SUBNETWORK_ID_NATIVE, good.payload.clone())), "another subnetwork");

        // And the id list names exactly the carriers, in block order.
        let plain = tx_with(SUBNETWORK_ID_NATIVE, vec![]);
        let revealed = carrier(PalwConsensusObjectV2::ReporterRevealed {
            offence_key: Hash64::from_u64_word(9),
            reporter: bond(3),
            salt: [4; 32],
        });
        assert_eq!(palw_h1_carrier_ids_v1(&[plain, good.clone(), unsigned, revealed.clone()]), vec![good.id(), revealed.id()]);
    }
}
