//! **ADR-0152 v3.1 H-1: the lifecycle objects a heartbeat must be able to carry** — the one list
//! the node half (P2-9) reads.
//!
//! V-8 lets a licence halt run for as long as its trigger lasts, and a row that outlives
//! `F + 9,000` matures at the first licence after the halt — so the conviction that should stop it
//! has to land DURING the halt, when the chain may be minting nothing but heartbeats. H-1 therefore
//! binds three parties to one set of objects: the fold applies them at step 3 in any block (C-7,
//! Phase 1's half), the heartbeat miner includes them ahead of better-paying traffic, and the H2
//! relay allowance never drops a heartbeat for carrying them. The node parties — `kaspa-mining`'s
//! carrier lane and reserve (`TransactionsPool::build_palw_carrier_lane`, `PalwCarrierReserveV1`),
//! `kaspa-p2p-flows`' relay exemption (`palw_heartbeat_h1_exemption_v1`) and the virtual processor's
//! H-1 gate — all decode through [`palw_h1_carrier_object_of_tx_v1`], so what the miner hurries, the
//! pool keeps, the relay spares and the gate checks cannot drift apart.
//!
//! **Policy, never a rule.** Nothing in block validation or the fold reads this module; a block
//! with or without these objects is exactly as valid as before, and no id, root or fingerprint
//! moves. It decides only which paid transactions a node admits into a full pool and takes first,
//! and which heartbeat the relay downloads past a peer's allowance.
//!
//! **A carrier is what the extractor would yield.** [`palw_h1_carrier_object_of_tx_v1`] runs
//! `palw_lifecycle_objects_from_accepted_txs_v2` itself — the walk the fold reads — rather than a
//! cheaper tag peek, because admission TOLERATES an undecodable 0x4b payload past the audit fence
//! (A-2): a tag byte followed by garbage would be a carrier to a peek and nothing to the fold, and
//! the lane would be selling priority to bytes that fold nothing.
//!
//! **This list judges the KIND; the node's H-1 gate judges the object.** A kind-valid object can
//! still be one the fold refuses — an accusation on a claim with a session already open, a forged
//! signature, a reveal with no pending reward — and a list that sold priority by kind alone sold
//! half of every testnet-12 block to such junk at the floor price (P2-9 review, finding 5). So the
//! virtual processor asks the fold about every carrier this list names, at the mempool and again at
//! every template (`palw_mempool_h1_carrier_refusal`, the P-B3 pattern): the acceptance layer and
//! the fold's own arm, on the tip. A carrier the tip's fold refuses is never admitted, relayed,
//! spared or mined by this node. What this module adds on top is [`palw_h1_carrier_lane_key_v1`]:
//! two carriers the tip would each take can still be one too many for one block (two accusations
//! of one claim), and the lane takes the first of each key.
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
use crate::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
use crate::tx::{Transaction, TransactionId};
use kaspa_hashes::Hash64;

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

/// **The object `tx` carries, when it is one H-1 names** — the one decode every node half shares
/// (the pool's carrier index, the H-1 gate, the relay's candidates). A 0x4b transaction the
/// extraction walk turns into an object [`palw_h1_carrier_object_v1`] names; `None` for everything
/// else, and any other subnetwork is answered without decoding.
pub fn palw_h1_carrier_object_of_tx_v1(tx: &Transaction) -> Option<PalwConsensusObjectV2> {
    if tx.subnetwork_id != SUBNETWORK_ID_PALW_LIFECYCLE {
        return None;
    }
    palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(tx))
        .objects
        .into_iter()
        .next()
        .map(|carried| carried.object)
        .filter(palw_h1_carrier_object_v1)
}

/// Whether `tx` carries an H-1 object ([`palw_h1_carrier_object_of_tx_v1`]).
pub fn palw_h1_carrier_tx_v1(tx: &Transaction) -> bool {
    palw_h1_carrier_object_of_tx_v1(tx).is_some()
}

/// The ids of the H-1 carriers among `txs`, in order — the count a node logs for its own beat.
/// Unbounded: never ask it of a block a peer sent (that is [`palw_h1_carrier_candidates_v1`]).
pub fn palw_h1_carrier_ids_v1(txs: &[Transaction]) -> Vec<TransactionId> {
    txs.iter().filter(|tx| palw_h1_carrier_tx_v1(tx)).map(|tx| tx.id()).collect()
}

/// **The most 0x4b payloads the relay decodes in a beat a peer sent before validating it** (P2-9
/// review, finding 4). One open carrier is all an exemption needs, and a template built by this
/// code puts its carriers first: the lane's batch is the first the template builder takes, and a
/// block keeps its selection order behind the coinbase (no rule re-sorts a body), so a lane-built
/// beat's first lifecycle transactions ARE its carriers. Eight is room for a lane whose head the
/// sending node's tip still took and this node's no longer does, and it bounds what a junk body
/// can make this node decode to eight payloads, whatever the message holds.
pub const PALW_H1_CARRIERS_EXAMINED_PER_BEAT_V1: usize = 8;

/// **The H-1 carriers the relay may ask about in an unvalidated beat**: among the first
/// [`PALW_H1_CARRIERS_EXAMINED_PER_BEAT_V1`] lifecycle transactions of `txs`, the ones that carry
/// an H-1 object, in order. Every other subnetwork is skipped on a field compare; no payload past
/// the bound is decoded.
pub fn palw_h1_carrier_candidates_v1(txs: &[Transaction]) -> Vec<TransactionId> {
    txs.iter()
        .filter(|tx| tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE)
        .take(PALW_H1_CARRIERS_EXAMINED_PER_BEAT_V1)
        .filter(|tx| palw_h1_carrier_tx_v1(tx))
        .map(|tx| tx.id())
        .collect()
}

/// **What makes two H-1 carriers one too many for a block** — the lane's de-duplication key (P2-9
/// review, finding 5).
///
/// The H-1 gate asks the fold about each carrier on its own, against the tip, so two carriers can
/// each pass it and still be refused TOGETHER: the fold opens one DA session per claim (DA-3, C-8's
/// `DaAccusationAlreadyOpen`), takes one answer per demanded unit, convicts once per offence, and
/// one court opening per claim is all a claim needs. The second of such a pair is dropped with the
/// block standing — a pure cost to its filer, and half-block lane space an accused would happily
/// fill with copies. So the lane takes the FIRST carrier of each key, in its own order (feerate,
/// then arrival), and leaves the rest to the fee market, where the gate evicts them once the first
/// has folded. A reporter's objects share one key per reporter bond: a bond's commitments are its
/// own to pace (64 open at most), and one lane slot per bond per template keeps a reporter flood
/// to the price of the bonds behind it.
///
/// Court moves have no key: each is signed by one of its session's two parties (the gate checks the
/// key), the phase admits them in turn, and the fold's per-block adjudication slot bounds the
/// expensive ones — a chunked close needs every chunk to land, and a key would pace it to one chunk
/// a template.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PalwH1LaneKeyV1 {
    /// A data-availability session: one per claim.
    DaSession(Hash64),
    /// An answer to a demanded unit (the unit's canonical bytes): one per (claim, unit).
    DaAnswer(Hash64, Vec<u8>),
    /// A conviction: one per (accused bond, evidence id).
    Offence(PalwBondKeyV2, Hash64),
    /// A reporter's commitment or reveal: one per reporter bond.
    Reporter(PalwBondKeyV2),
    /// A bisection court's opening: one per claim.
    CourtOpening(Hash64),
}

/// The lane key of an H-1 carrier's object ([`PalwH1LaneKeyV1`]); `None` for a court move and for
/// every object that is not an H-1 carrier.
pub fn palw_h1_carrier_lane_key_v1(object: &PalwConsensusObjectV2) -> Option<PalwH1LaneKeyV1> {
    use PalwConsensusObjectV2 as O;
    if !palw_h1_carrier_object_v1(object) {
        return None;
    }
    match object {
        O::DefaultAccused { claim, .. } => Some(PalwH1LaneKeyV1::DaSession(*claim)),
        O::DefaultAccusedHeld { accusation } => Some(PalwH1LaneKeyV1::DaSession(accusation.claim)),
        O::MaterialDisclosedV2 { claim, unit, .. } => {
            Some(PalwH1LaneKeyV1::DaAnswer(*claim, borsh::to_vec(unit).expect("a DA unit serializes")))
        }
        O::ObjectiveOffence { accused, evidence_id, .. } => Some(PalwH1LaneKeyV1::Offence(*accused, *evidence_id)),
        O::ReporterCommitted { reporter, .. } | O::ReporterRevealed { reporter, .. } => Some(PalwH1LaneKeyV1::Reporter(*reporter)),
        O::CourtOpened { claim, .. } => Some(PalwH1LaneKeyV1::CourtOpening(*claim)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    use crate::subnets::SUBNETWORK_ID_NATIVE;
    use crate::tx::{ScriptPublicKey, TransactionOutpoint, TransactionOutput};

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

    /// **The lane key names what the fold takes once per block** (P2-9 review, finding 5). Two
    /// accusations of one claim share a key whoever files them — the fold opens one session per
    /// claim — and so do a conviction's copies and a reporter's objects; a court move has none (its
    /// session's parties sign it, and a chunked close needs every chunk), and neither has anything
    /// that is not an H-1 carrier.
    #[test]
    fn the_lane_key_names_what_the_fold_takes_once_per_block() {
        let accused = |claim: u64, accuser: u64| PalwConsensusObjectV2::DefaultAccused {
            claim: Hash64::from_u64_word(claim),
            missing_event_index: 0,
            accuser: bond(accuser),
            signature: vec![1; 8],
        };
        let key = palw_h1_carrier_lane_key_v1;
        assert_eq!(key(&accused(7, 2)), key(&accused(7, 3)), "one DA session per claim, whoever accuses");
        assert_ne!(key(&accused(7, 2)), key(&accused(8, 2)), "another claim is another session");
        assert_eq!(key(&accused(7, 2)), Some(PalwH1LaneKeyV1::DaSession(Hash64::from_u64_word(7))));

        let refuted = |evidence: u64| PalwConsensusObjectV2::ObjectiveOffence {
            kind: PalwOffenceKindV1::ExecutorRefuted,
            accused: bond(1),
            evidence_id: Hash64::from_u64_word(evidence),
            evidence: vec![3],
        };
        assert_eq!(key(&refuted(2)), key(&refuted(2)), "a conviction's copies are one");
        assert_ne!(key(&refuted(2)), key(&refuted(3)), "other evidence is another offence");

        let committed = PalwConsensusObjectV2::ReporterCommitted {
            commitment: Hash64::from_u64_word(8),
            reporter: bond(3),
            signature: vec![1; 8],
        };
        let revealed =
            PalwConsensusObjectV2::ReporterRevealed { offence_key: Hash64::from_u64_word(9), reporter: bond(3), salt: [4; 32] };
        assert_eq!(key(&committed), key(&revealed), "one lane slot per reporter bond");

        let chunk = PalwConsensusObjectV2::CourtCloseChunk {
            session_id: Hash64::from_u64_word(10),
            side: crate::palw_state_v2::PalwCourtSideV1::Executor,
            index: 0,
            bytes: vec![1],
        };
        assert!(palw_h1_carrier_object_v1(&chunk) && key(&chunk).is_none(), "a court move rides unkeyed");
        let licence = PalwConsensusObjectV2::ReceiptLicensed { claim: Hash64::from_u64_word(11), receipts: vec![] };
        assert!(key(&licence).is_none(), "a licence is no carrier at all");
        assert!(key(&offence(PalwOffenceKindV1::DaDefault)).is_none(), "nor a kind only the fold writes");
    }

    /// **The relay decodes at most eight lifecycle payloads of a beat it has not validated** (P2-9
    /// review, finding 4): a carrier behind eight other lifecycle transactions is not looked at,
    /// every other subnetwork costs a field compare, and a lane-built beat — carriers first — is
    /// found at once.
    #[test]
    fn the_relay_examines_a_bounded_head_of_a_beat() {
        let licence = |n: u64| carrier(PalwConsensusObjectV2::ReceiptLicensed { claim: Hash64::from_u64_word(n), receipts: vec![] });
        let accused = carrier(PalwConsensusObjectV2::DefaultAccused {
            claim: Hash64::from_u64_word(7),
            missing_event_index: 0,
            accuser: bond(2),
            signature: vec![1; 8],
        });
        let natives: Vec<Transaction> = (0..100u64)
            .map(|n| {
                Transaction::new(
                    0,
                    vec![],
                    vec![TransactionOutput::new(n, ScriptPublicKey::from_vec(0, vec![0x51]))],
                    0,
                    SUBNETWORK_ID_NATIVE,
                    0,
                    vec![],
                )
            })
            .collect();

        let mut lane_built = natives.clone();
        lane_built.push(accused.clone());
        lane_built.extend((0..20).map(licence));
        assert_eq!(palw_h1_carrier_candidates_v1(&lane_built), vec![accused.id()], "the lane's carrier leads its lifecycle txs");

        let mut buried = natives;
        buried.extend((0..PALW_H1_CARRIERS_EXAMINED_PER_BEAT_V1 as u64).map(licence));
        buried.push(accused.clone());
        assert!(palw_h1_carrier_candidates_v1(&buried).is_empty(), "nothing past the bound is decoded");
        assert_eq!(palw_h1_carrier_ids_v1(&buried), vec![accused.id()], "the unbounded count still sees it");
    }
}
