//! From accepted transactions to claim-lifecycle objects (P0-11).
//!
//! `palw_fp_objects_v3` does this for the one object a free-prompt commitment carries. This does
//! it for the objects that move a claim through the lattice — the panel binding, the licensing,
//! the producer default, and the four court moves — and its absence was a liveness hole rather
//! than a missing feature: with no extractor for `PanelBound`, no block could carry one, so every
//! claim on a V2 network sat `Provisional` until `window_bind` lapsed and voided as
//! `BindTimeout`. `safe_weight` never grew, the safe frontier never left the zero point, and PALW
//! weight — the network's entire fork choice — was permanently zero.
//!
//! # What may ride here, and what deliberately may not
//!
//! The payload is a `PalwConsensusObjectV2` and the extractor accepts a fixed subset of its
//! variants. The exclusions are the point:
//!
//! * **`BondRegistered` may not.** The object DECLARES `collateral`, and nothing on this path
//!   locks a UTXO behind that number. A transaction that says "I have staked a million" would
//!   stake a million, and every exposure ceiling, every slash and Decision 7's whole Sybil bound
//!   are denominated in it. Bonds come from the genesis registration list until a collateral
//!   lock exists — which is the coinbase/UTXO gate, not this seam.
//! * **`ClassRegistered` may not.** A class entering a live chain moves the share table
//!   (ADR-0045 Decision 3 funds an entrant by donation from every incumbent) and brings its own
//!   `pwu_rule`. `verify_palw_genesis_v2` refuses `MaxPerAttempt` at genesis precisely because a
//!   ceiling makes weight a measure of collateral rather than of work; letting a class in through
//!   a transaction would route around that check. Keeping classes to genesis closes the H3 tail
//!   structurally instead of with a second copy of the same rule.
//! * **`FreePromptCommitted` may not.** It has its own subnetwork, its own codec and its own
//!   pricing rules; accepting it here would be a second path into one object with one of them
//!   unpriced.
//! * **`PanelBound` may not, and this one is a later decision than the module.** A panel is
//!   `derive_panel_v2` of the anchor block and the bond registry — a pure function of chain state
//!   with nothing for a publisher to choose. Carrying it made someone send an object nobody was
//!   paid to send for a claim that was not theirs, so in practice the producer decided whether
//!   its own claim proceeded (audit C5's tail). The chain derives the binding itself now
//!   (`palw_v2_derived_panel_bindings`), and a carried one would be a second answer to a question
//!   that has one.
//!
//! Everything else advances a claim without minting or locking value, and each kind already has
//! an acceptance check the pipeline runs before the transition folds it
//! (`validate_panel_bound_v2`, `check_court_open_acceptance_v2`, `adjudicate_court_close_v2`, and
//! the two rung signature checks).
//!
//! # Why a malformed carrier is SKIPPED, not fatal
//!
//! Same rule, same reason as the free-prompt walk beside it: transaction-level validity is the
//! transaction validator's job, while this walk must be a total function of whatever DID get
//! accepted. A walk that could panic or reject on a peer-supplied payload would be a remote
//! denial of service wearing a consensus rule's clothes. The skipped list is returned so a caller
//! can log it — a silently dropped carrier is the "reads as nothing" failure ADR-0042 Decision 5
//! warns about.

use crate::palw_state_v2::PalwConsensusObjectV2;
use crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
use crate::tx::{Transaction, TransactionId};

/// Wire version for a lifecycle carriage payload. A payload naming any other version is skipped,
/// never reinterpreted.
pub const PALW_LIFECYCLE_TX_VERSION_V2: u16 = 1;

/// What one transaction carries: exactly one lifecycle object.
///
/// One object per transaction rather than a batch, so that a malformed member cannot take valid
/// siblings down with it and so the carrier id names precisely one object for attribution.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwLifecycleTxPayloadV2 {
    pub version: u16,
    pub object: PalwConsensusObjectV2,
}

/// An extracted object plus the transaction that carried it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwLifecycleCarrierV2 {
    pub carrier: TransactionId,
    pub object: PalwConsensusObjectV2,
}

/// What one block's lifecycle extraction produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwLifecycleExtractionV2 {
    /// The objects, in acceptance order — the transition's order IS consensus.
    pub objects: Vec<PalwLifecycleCarrierV2>,
    /// Carriers routed here that produced no object, and why.
    pub skipped: Vec<(TransactionId, &'static str)>,
}

/// Whether this object kind may enter a chain through a transaction. See the module doc for why
/// each exclusion is an exclusion.
pub fn palw_lifecycle_object_may_ride_v2(object: &PalwConsensusObjectV2) -> Result<(), &'static str> {
    match object {
        PalwConsensusObjectV2::ReceiptLicensed { .. }
        | PalwConsensusObjectV2::ProducerDefaulted { .. }
        | PalwConsensusObjectV2::CourtOpened { .. }
        | PalwConsensusObjectV2::CourtClosed { .. }
        | PalwConsensusObjectV2::CourtDisclosed { .. }
        | PalwConsensusObjectV2::CourtVerdictPosted { .. }
        | PalwConsensusObjectV2::BondRetireRequested { .. }
        | PalwConsensusObjectV2::ClassFrozen { .. } => Ok(()),
        PalwConsensusObjectV2::PanelBound { .. } => {
            Err("the chain derives panel bindings; a carried one would be a second answer to a question with one")
        }
        PalwConsensusObjectV2::BondRegistered { .. } => {
            Err("a bond registration declares collateral nothing on this path locks — bonds come from genesis")
        }
        PalwConsensusObjectV2::ClassRegistered { .. } => {
            Err("a class entering a live chain moves the share table and brings its own pwu rule — classes come from genesis")
        }
        PalwConsensusObjectV2::FreePromptCommitted { .. } => {
            Err("a free-prompt commitment rides its own subnetwork, where its price is checked")
        }
    }
}

/// One chain block's accepted lifecycle transactions, as consensus objects in acceptance order.
///
/// A pure function of the transactions: no chain state is read here. Everything that needs state
/// — that the claim exists and is in the right phase, that the panel is the one this chain
/// derives, that a court close's proof adjudicates — is the acceptance layer's and the
/// transition's, exactly as it is for the free-prompt walk.
pub fn palw_lifecycle_objects_from_accepted_txs_v2(txs: &[Transaction]) -> PalwLifecycleExtractionV2 {
    let mut out = PalwLifecycleExtractionV2::default();
    for tx in txs {
        if tx.subnetwork_id != SUBNETWORK_ID_PALW_LIFECYCLE {
            continue;
        }
        let id = tx.id();
        let payload: PalwLifecycleTxPayloadV2 = match borsh::from_slice(&tx.payload) {
            Ok(payload) => payload,
            Err(_) => {
                out.skipped.push((id, "payload does not decode"));
                continue;
            }
        };
        if payload.version != PALW_LIFECYCLE_TX_VERSION_V2 {
            out.skipped.push((id, "payload names an unsupported wire version"));
            continue;
        }
        if let Err(reason) = palw_lifecycle_object_may_ride_v2(&payload.object) {
            out.skipped.push((id, reason));
            continue;
        }
        out.objects.push(PalwLifecycleCarrierV2 { carrier: id, object: payload.object });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_state_v2::{PalwBondKeyV2, PalwPanelSeatV2, PalwPwuRuleV2};
    use crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
    use crate::tx::{ScriptPublicKey, Transaction, TransactionOutpoint, TransactionOutput};
    use kaspa_hashes::Hash64;

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(n), index: 0 })
    }

    fn carrier(subnetwork: crate::subnets::SubnetworkId, payload: Vec<u8>) -> Transaction {
        Transaction::new(
            0,
            Vec::new(),
            vec![TransactionOutput::new(1, ScriptPublicKey::from_vec(0, vec![0x51]))],
            0,
            subnetwork,
            0,
            payload,
        )
    }

    fn lifecycle_tx(object: PalwConsensusObjectV2) -> Transaction {
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object })
            .expect("a lifecycle payload is borsh-serializable");
        carrier(SUBNETWORK_ID_PALW_LIFECYCLE, payload)
    }

    fn panel_bound() -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::PanelBound {
            claim: h64(0xC1),
            anchor: h64(0x77),
            seats: vec![PalwPanelSeatV2 { bond: bond(2), operator_id: h64(0x22) }],
        }
    }

    /// **P0-11's fix, at this layer.** A `PanelBound` in a transaction becomes a `PanelBound` the
    /// transition can fold — which no block could do at all before this module existed.
    #[test]
    fn a_lifecycle_object_rides_a_transaction_and_arrives_in_acceptance_order() {
        let first = lifecycle_tx(PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xC1), receipts: Vec::new() });
        let second = lifecycle_tx(PalwConsensusObjectV2::BondRetireRequested { bond: bond(3) });
        let unrelated = carrier(crate::subnets::SUBNETWORK_ID_NATIVE, Vec::new());
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[first.clone(), unrelated, second.clone()]);

        assert_eq!(out.objects.len(), 2, "two carriers, two objects; the native transaction is not one");
        assert!(out.skipped.is_empty());
        assert_eq!(
            out.objects[0].object,
            PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xC1), receipts: Vec::new() },
            "and it is the object the payload carried"
        );
        assert_eq!(out.objects[0].carrier, first.id(), "attributed to the transaction that carried it");
        // Acceptance ORDER is consensus: the transition folds them in this sequence.
        assert_eq!(out.objects[1].carrier, second.id());
    }

    /// The three kinds that may not ride, each for its own reason, each skipped with that reason
    /// rather than silently dropped.
    #[test]
    fn objects_that_must_not_ride_are_skipped_with_their_own_reason() {
        let bond_registration = PalwConsensusObjectV2::BondRegistered {
            bond: bond(9),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21; 8],
            // The number that would be free: nothing on this path locks a UTXO behind it.
            collateral: 1_000_000_000_000,
            payout_payload: h64(0x9A11),
        };
        let class_registration = PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(0xC1A55),
            artifact_root: h64(0xA7),
            slash_value_per_pwu: 1,
            // The rule genesis refuses, arriving through the side door.
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(u64::MAX),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
        };
        let fp = PalwConsensusObjectV2::FreePromptCommitted {
            claim: h64(0xF1),
            class_id: h64(1),
            bond: bond(1),
            pwu: 8,
            quanta: 2,
            trace_root: h64(41),
            output_root: h64(42),
            execution_root: h64(43),
            trace_chunk_count: 4,
            trace_retention_daa: 99,
        };
        // The panel binding: excluded for a different reason than the other three — not because
        // it moves value, but because the chain already derives it and one question gets one
        // answer.
        let panel = panel_bound();
        for object in [bond_registration, class_registration, fp, panel] {
            let out = palw_lifecycle_objects_from_accepted_txs_v2(&[lifecycle_tx(object.clone())]);
            assert!(out.objects.is_empty(), "{object:?} must not enter a chain here");
            assert_eq!(out.skipped.len(), 1, "and the drop is reported, not silent");
        }
    }

    /// A payload that does not decode, or names another wire version, contributes no object and
    /// does not reject the block — the walk is total over whatever was accepted.
    #[test]
    fn a_malformed_carrier_is_skipped_with_a_reason() {
        let garbage = carrier(SUBNETWORK_ID_PALW_LIFECYCLE, vec![0xFF; 8]);
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[garbage]);
        assert!(out.objects.is_empty());
        assert_eq!(out.skipped[0].1, "payload does not decode");

        let wrong_version = {
            let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: 99, object: panel_bound() }).unwrap();
            carrier(SUBNETWORK_ID_PALW_LIFECYCLE, payload)
        };
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[wrong_version]);
        assert!(out.objects.is_empty());
        assert_eq!(out.skipped[0].1, "payload names an unsupported wire version");
    }

    /// A lifecycle payload routed to the free-prompt subnetwork is not a lifecycle object: the
    /// band ids exist so one band's payload never reaches another band's validator.
    #[test]
    fn the_band_id_is_what_selects_this_walk() {
        let payload =
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: panel_bound() }).unwrap();
        let misrouted = carrier(SUBNETWORK_ID_PALW_FP_COMMITMENT, payload);
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[misrouted]);
        assert!(out.objects.is_empty(), "this walk reads its own band and nothing else");
        assert!(out.skipped.is_empty(), "and a foreign band is not even a skip — it is not addressed to us");
    }
}
