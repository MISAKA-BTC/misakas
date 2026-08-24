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
//! * **`BondRegistered` may, and the lock is what lets it.** The object DECLARES `collateral`,
//!   and for a long time nothing on this path locked a UTXO behind that number — a transaction
//!   saying "I have staked a million" would have staked a million, and every exposure ceiling,
//!   every slash and Decision 7's whole Sybil bound are denominated in it. So bonds came from the
//!   genesis registration list only.
//!
//!   [`palw_bond_registration_binds_its_carrier_v2`] is that lock, and the extractor calls it on
//!   every registration: the outpoint must be an output of the CARRYING transaction, holding at
//!   least the collateral it declares, paying to the P2PKH of the payload the registration names
//!   as its payee. Nothing is looked up, because nothing needs to be — the output is created by
//!   the transaction carrying the object, so its existence, amount and script are facts block
//!   validation established before this object was decoded. The carrier proves the money; the
//!   signature this list demands proves the owner.
//! * **`ClassRegistered` may, but only carrying what makes it checkable** (ADR-0049 Decision H —
//!   this used to be an outright refusal). A class entering a live chain moves the share table
//!   (ADR-0045 Decision 3 funds an entrant by donation from every incumbent) and brings its own
//!   `pwu_rule`, and nothing checked either — which was the real objection, and it is a statement
//!   about CHECKING rather than about forbidding. So the object must carry its shape profile and
//!   canonical job, and `verify_class_admission_v2` decides at acceptance: coverage over
//!   coordinates, the four cost bounds, the ladder, and the derived-pwu rule the genesis loader
//!   already enforces. A registration WITHOUT that material is still refused here, because there
//!   is nothing to check it with.
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
        | PalwConsensusObjectV2::CourtVerdictPosted { .. } => Ok(()),
        // **Audit M-01: a door nobody can authenticate is shut.**
        //
        // `BondRetireRequested { bond }` carried no signature and no owner binding, and a bond key
        // IS a premine outpoint — a public constant. One ordinary transaction from any stranger
        // flipped any bond to `Retiring`, with no inverse; on a network with one producer that is a
        // permanent halt for a transaction fee. `ClassFrozen`'s contradiction certificate has
        // signatures that `check_class_contradiction_shape_v2` explicitly defers to "the acceptance
        // layer", which had no arm for this object — so a forged certificate froze a class forever,
        // and there is deliberately no `ClassUnfrozen`.
        //
        // Both belong on chain eventually and neither can be re-admitted without carrying its own
        // authorization: an owner signature over the bond key, and a contradiction adjudicated by
        // `adjudicate_class_contradiction_v1` (which takes a verifier, and is wired only into the
        // other band today). Until then they are refused here AND at acceptance — one lock is a
        // lock somebody removes while refactoring.
        // **Re-admitted, now that it carries the authorisation the refusal stood in for.**
        //
        // The refusal was right and it was also a permanent capital lock: retirement is the ONLY
        // writer of `Retiring`, `palw_bond_collateral_is_locked_v2` is unconditionally true for an
        // `Active` bond, and the C-08 burn is collected only from a bond the lock has released —
        // so with this door shut, every genesis collateral outpoint is unspendable forever and
        // every slashed sompi freezes instead of being destroyed. "Stake" that can never be
        // withdrawn is not stake.
        //
        // This layer is stateless, so it checks SHAPE only: a retirement must carry a signature.
        // Whether that signature is the bond's own is the acceptance layer's, where the registry
        // is in hand — the same split `ClassRegistered` uses two arms below.
        PalwConsensusObjectV2::BondRetireRequested { signature, .. } if !signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::BondRetireRequested { .. } => Err(
            "a bond retirement must carry the owner signature that authorizes it — a bond key is a public outpoint, so without one anyone could retire anyone's bond",
        ),
        PalwConsensusObjectV2::ClassFrozen { .. } => Err(
            "a class freeze carries a contradiction certificate no layer verifies — a forged one freezes a class permanently, and there is no unfreeze",
        ),
        // **ADR-0049 Decision H: a class MAY register post-genesis — gated, not forbidden.**
        //
        // The objection this refusal carried is correct and it is a statement about CHECKING: a
        // class entering a live chain moves the share table and brings its own `pwu_rule`, and
        // nothing checked either. Decisions C and D are that check, so the refusal is replaced by
        // the gate. What rides here is the SHAPE of a checkable registration; whether the graph
        // covers, fits the ladder, costs what the ruleset allows and counts the pwu it declares is
        // `verify_class_admission_v2`'s, at acceptance, where the bundle is in hand.
        PalwConsensusObjectV2::ClassRegistered { admission: Some(_), .. } => Ok(()),
        PalwConsensusObjectV2::ClassRegistered { admission: None, .. } => Err(
            "a class registered on a running chain must carry its shape profile and canonical job —              without them nothing can check its coverage, its ladder depth or its declared pwu",
        ),
        PalwConsensusObjectV2::PanelBound { .. } => {
            Err("the chain derives panel bindings; a carried one would be a second answer to a question with one")
        }
        // **Re-admitted, and the thing the refusal named is now proven by the carrier itself.**
        //
        // The objection was exact: a registration DECLARES collateral, and nothing on this path
        // locked it — so "stake" meant a number in an object. It could not be checked here either,
        // because this layer is stateless and has no UTXO set to look an outpoint up in.
        //
        // So the outpoint is not looked up: it is CREATED. A registration must name an output of
        // its own carrying transaction, which makes existence, value and script something block
        // validation has already established before this object is read —
        // `palw_bond_registration_binds_its_carrier_v2` is what checks that binding, and it needs
        // the transaction, so it runs in the extractor rather than here. What is left for this
        // layer is the shape: a registration must carry the signature that proves the registrant
        // holds the key it declares, since anyone can pay to somebody else's script.
        PalwConsensusObjectV2::BondRegistered { signature, .. } if !signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::BondRegistered { .. } => Err(
            "a bond registration must carry the registrant's signature over the key it declares — the carrier proves the collateral, not the owner",
        ),
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
/// **A bond registration must name an output of its own carrier, and that output must be the
/// collateral it declares.**
///
/// This is what replaces "bonds come from genesis". A registration used to declare a `bond`
/// outpoint and a `collateral` amount that no layer could check: the stateless ride list has no
/// UTXO set, and the acceptance validator is handed PALW state rather than the UTXO diff. Both
/// facts are still true — so the outpoint is not looked up anywhere. It is created by the
/// transaction carrying the registration, which means existence, amount and script are things
/// block validation established before this object was ever decoded.
///
/// The script must be the P2PKH-ML-DSA-87 of the payload the registration names as its payee, so
/// the collateral is reclaimable by exactly whoever the rewards are. Together with the signature
/// the ride list demands, that is the pair the audit asked for: the carrier proves the money, the
/// signature proves the owner.
pub fn palw_bond_registration_binds_its_carrier_v2(tx: &Transaction, object: &PalwConsensusObjectV2) -> Result<(), &'static str> {
    let PalwConsensusObjectV2::BondRegistered { bond, collateral, payout_payload, .. } = object else {
        return Ok(());
    };
    if bond.0.transaction_id != tx.id() {
        return Err("a bond registration must name an output of its own carrying transaction");
    }
    let Some(output) = tx.outputs.get(bond.0.index as usize) else {
        return Err("a bond registration names an output its carrier does not have");
    };
    if output.value < *collateral {
        return Err("a bond registration declares more collateral than the output it names holds");
    }
    // The same script the chain will pay this bond's rewards to. Two things follow from that
    // choice: the collateral is reclaimable by whoever the rewards are reclaimable by, and the
    // registration cannot lock money behind a script it did not also name as its own payee.
    let owner: [u8; 64] = *payout_payload.as_byte_slice();
    if output.script_public_key != crate::dns_finality::p2pkh_mldsa87_spk(&owner) {
        return Err("a bond's collateral output must pay to the payload the registration names as its payee");
    }
    Ok(())
}

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
        if let Err(reason) = palw_bond_registration_binds_its_carrier_v2(tx, &payload.object) {
            out.skipped.push((id, reason));
            continue;
        }
        out.objects.push(PalwLifecycleCarrierV2 { carrier: id, object: payload.object });
    }
    out
}

/// Why a lifecycle carrier was refused at ADMISSION. Distinct from the walk's `skipped` strings
/// because a rejection is a fact about a transaction and has to name itself in a block-rule
/// error, while a skip is a note in a log.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwLifecycleTxError {
    #[error("the payload does not decode as a lifecycle carriage")]
    Undecodable,
    #[error("the payload names wire version {got}, not {expected}")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("this object kind may not ride a transaction: {0}")]
    ObjectMayNotRide(&'static str),
}

/// **Transaction-level admission for [`SUBNETWORK_ID_PALW_LIFECYCLE`] (0x4b).**
///
/// The module doc above explains what the extractor is for; this is the gate that lets a carrier
/// reach it. Without it the id was defined, tested and unreachable: `check_transaction_subnetwork`
/// had no arm for 0x4b, so every lifecycle transaction was `SubnetworksDisabled` at admission and
/// the liveness hole the extractor was written to close stayed exactly as open as before — a
/// claim still could not be licensed, no court move could be filed, and PALW weight was still
/// permanently zero. An extractor with no door in front of it extracts nothing.
///
/// The rules are the walk's own, in the walk's order, so admission and extraction cannot
/// disagree: decode, wire version, and the may-ride table. Everything past that — that the claim
/// exists, that it is in the right phase, that a court close adjudicates — is stateful and stays
/// where it is, in the transition and its acceptance checks.
pub fn validate_palw_lifecycle_tx(payload: &[u8]) -> Result<(), PalwLifecycleTxError> {
    let payload: PalwLifecycleTxPayloadV2 = borsh::from_slice(payload).map_err(|_| PalwLifecycleTxError::Undecodable)?;
    if payload.version != PALW_LIFECYCLE_TX_VERSION_V2 {
        return Err(PalwLifecycleTxError::UnsupportedVersion { got: payload.version, expected: PALW_LIFECYCLE_TX_VERSION_V2 });
    }
    palw_lifecycle_object_may_ride_v2(&payload.object).map_err(PalwLifecycleTxError::ObjectMayNotRide)
}

#[cfg(test)]
mod tests {
    /// **A bond CAN enter through a transaction — this is what it has to prove.**
    ///
    /// The carrier proves the money and the signature proves the owner. Neither is a promise: the
    /// output is created by the very transaction carrying the registration, so its existence,
    /// amount and script are facts block validation established before this object was decoded.
    #[test]
    fn a_bond_registration_that_locks_its_collateral_rides() {
        let payee = h64(0xBEEF);
        let owner: [u8; 64] = *payee.as_byte_slice();
        let spk = crate::dns_finality::p2pkh_mldsa87_spk(&owner);
        let tx = Transaction::new(
            0,
            vec![],
            vec![TransactionOutput::new(500_000, spk.clone())],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE.clone(),
            0,
            vec![],
        );
        let object = PalwConsensusObjectV2::BondRegistered {
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(tx.id(), 0)),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21; 8],
            collateral: 500_000,
            payout_payload: payee,
            signature: vec![1; 8],
        };
        palw_bond_registration_binds_its_carrier_v2(&tx, &object)
            .expect("a registration whose carrier holds the collateral it declares must bind");
        palw_lifecycle_object_may_ride_v2(&object).expect("and the ride list must accept it");
    }

    /// **Every way the lock can be lied to, refused by its own reason.**
    ///
    /// These are the four the audit named, and the first is the one that made the whole seam
    /// unsafe before the lock existed: a registration could DECLARE a million and stake a million,
    /// because nothing looked at any output.
    #[test]
    fn a_bond_registration_cannot_declare_collateral_it_did_not_lock() {
        let payee = h64(0xBEEF);
        let owner: [u8; 64] = *payee.as_byte_slice();
        let spk = crate::dns_finality::p2pkh_mldsa87_spk(&owner);
        let tx = |value: u64, script: crate::tx::ScriptPublicKey| {
            Transaction::new(
                0,
                vec![],
                vec![TransactionOutput::new(value, script)],
                0,
                SUBNETWORK_ID_PALW_LIFECYCLE.clone(),
                0,
                vec![],
            )
        };
        let reg = |t: &Transaction, index: u32, collateral: u64, payee: crate::Hash64| PalwConsensusObjectV2::BondRegistered {
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(t.id(), index)),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21; 8],
            collateral,
            payout_payload: payee,
            signature: vec![1; 8],
        };

        // 1. Declaring more than the output holds — the free million.
        let t = tx(500_000, spk.clone());
        let e = palw_bond_registration_binds_its_carrier_v2(&t, &reg(&t, 0, 1_000_000_000, payee)).unwrap_err();
        assert!(e.contains("more collateral than the output"), "{e}");

        // 2. Naming an output the carrier does not have.
        let t = tx(500_000, spk.clone());
        let e = palw_bond_registration_binds_its_carrier_v2(&t, &reg(&t, 7, 500_000, payee)).unwrap_err();
        assert!(e.contains("does not have"), "{e}");

        // 3. Naming somebody ELSE's transaction — an output this registration did not create, and
        //    therefore one no layer on this path can check.
        let t = tx(500_000, spk.clone());
        let other = tx(999, spk.clone());
        let e = palw_bond_registration_binds_its_carrier_v2(&t, &reg(&other, 0, 500_000, payee)).unwrap_err();
        assert!(e.contains("its own carrying transaction"), "{e}");

        // 4. Locking the money behind a script that is not the payee's, so the collateral and the
        //    rewards would be reclaimable by different people.
        let t = tx(500_000, crate::dns_finality::p2pkh_mldsa87_spk(&[9u8; 64]));
        let e = palw_bond_registration_binds_its_carrier_v2(&t, &reg(&t, 0, 500_000, payee)).unwrap_err();
        assert!(e.contains("names as its payee"), "{e}");
    }

    /// A registration with no signature is still refused: the carrier proves the collateral, and
    /// only the signature proves who owns the key being registered.
    #[test]
    fn a_bond_registration_without_a_signature_does_not_ride() {
        let object = PalwConsensusObjectV2::BondRegistered {
            bond: bond(9),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21; 8],
            collateral: 500_000,
            payout_payload: h64(0x9A11),
            signature: Vec::new(),
        };
        let e = palw_lifecycle_object_may_ride_v2(&object).unwrap_err();
        assert!(e.contains("signature over the key it declares"), "{e}");
    }

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
        let second = lifecycle_tx(PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xC2), receipts: Vec::new() });
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

    /// **Audit M-01: the two doors nobody could authenticate are shut, and say so.**
    ///
    /// `BondRetireRequested` names a bond key — a PUBLIC premine outpoint — and carried no owner
    /// signature, so one ordinary transaction from any stranger retired any bond, permanently and
    /// with no inverse. `ClassFrozen`'s contradiction certificate has signatures the shape check
    /// explicitly defers to "the acceptance layer", which had no arm for the object at all.
    #[test]
    fn the_two_unauthenticated_objects_may_not_ride() {
        for (object, needle) in [
            (PalwConsensusObjectV2::BondRetireRequested { bond: bond(3), signature: Vec::new() }, "must carry the owner signature"),
            (
                PalwConsensusObjectV2::ClassFrozen {
                    class_id: h64(1),
                    certificate: crate::palw_state_v2::tests::contradiction(h64(1)),
                },
                "no layer verifies",
            ),
        ] {
            let err = palw_lifecycle_object_may_ride_v2(&object).unwrap_err();
            assert!(err.contains(needle), "the refusal must say why: got {err}");
            let out = palw_lifecycle_objects_from_accepted_txs_v2(&[lifecycle_tx(object)]);
            assert!(out.objects.is_empty(), "and it must not reach the transition");
            assert_eq!(out.skipped.len(), 1, "skipped with its reason, not silently dropped");
        }
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
            payout_payload: h64(0x9A11), signature: Vec::new() };
        // A class registration with NO admission material: still refused, because there is
        // nothing to check it with (ADR-0049 Decision H replaced the blanket refusal with a gate,
        // and a gate needs an input).
        let class_registration = PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(0xC1A55),
            terms: crate::palw_state_v2::PalwClassTermsV2::deterministic_default(),
            artifact_root: h64(0xA7),
            slash_value_per_pwu: 1,
            // The rule genesis refuses, arriving through the side door.
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(u64::MAX),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        };
        let fp = PalwConsensusObjectV2::FreePromptCommitted {
            claim: h64(0xF1),
            class_id: h64(1),
            bond: bond(1),
            // Any key: this test asks whether the object may ride a carriage, which is decided
            // before any state is consulted.
            executor_pubkey: vec![7; 4],
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

    /// **ADR-0049 Decision H: one registration policy, and it is a gate.**
    ///
    /// Three policies coexisted — the carriage refused `ClassRegistered` outright,
    /// `verify_class_admission_v2` would have admitted it at the minimum grantable share, and the
    /// state machine implements a weightless activation clock. The carriage's objection was the
    /// right one and it was a statement about CHECKING, so the refusal is replaced by the gate:
    /// a registration that carries the graph and the canonical job RIDES, and the acceptance layer
    /// decides whether the graph covers, fits the ladder, costs what the ruleset allows and counts
    /// the pwu it declares.
    ///
    /// What this layer owns is the shape: carried material rides, missing material does not.
    #[test]
    fn a_class_registration_rides_only_when_it_carries_what_checks_it() {
        use crate::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
        use crate::palw_state_v2::PalwClassAdmissionCarriageV2;

        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor's own graph");
        let carriage = PalwClassAdmissionCarriageV2 {
            canonical: rc_job_context(&profile, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1),
            profile: profile.clone(),
            // The ride list checks SHAPE only; who signed is the acceptance layer's, where the
            // registrant bond is resolved against chain state.
            registrant_bond: bond(1),
            signature: Vec::new(),
        };
        let registration = |admission: Option<Box<PalwClassAdmissionCarriageV2>>| PalwConsensusObjectV2::ClassRegistered {
            class_id: profile.shape_profile_id(),
            terms: crate::palw_state_v2::PalwClassTermsV2::deterministic_default(),
            artifact_root: h64(0xA7),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 4_096 },
            initial_target: u128::MAX / 2,
            share_permille: 1,
            activation_daa: 0,
            admission,
        };

        // Carried: it rides, and the acceptance layer gets something to check.
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[lifecycle_tx(registration(Some(Box::new(carriage))))]);
        assert_eq!(out.objects.len(), 1, "a checkable registration reaches the chain");
        assert!(out.skipped.is_empty());

        // Missing: refused HERE, because the gate downstream has no input. The reason names the
        // material rather than the policy — the policy is now "check it", not "never".
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[lifecycle_tx(registration(None))]);
        assert!(out.objects.is_empty());
        assert!(out.skipped[0].1.contains("shape profile"), "got {:?}", out.skipped[0].1);
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

    /// **Admission and extraction must give ONE answer.**
    ///
    /// The transaction validator decides what may be in a block; this walk decides what a block's
    /// contents mean. If the two disagreed in the permissive direction a carrier would be admitted
    /// and then silently dropped (the "reads as nothing" failure); in the strict direction a
    /// carrier the walk would have credited could never reach a block at all. Both are closed by
    /// running one table from one place — asserted here over every case the pair can see, rather
    /// than left to the fact that today they call the same function.
    #[test]
    fn admission_accepts_exactly_what_the_walk_extracts() {
        let cases: Vec<Vec<u8>> = vec![
            // Rides.
            borsh::to_vec(&PalwLifecycleTxPayloadV2 {
                version: PALW_LIFECYCLE_TX_VERSION_V2,
                object: PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xC1), receipts: Vec::new() },
            })
            .unwrap(),
            borsh::to_vec(&PalwLifecycleTxPayloadV2 {
                version: PALW_LIFECYCLE_TX_VERSION_V2,
                object: PalwConsensusObjectV2::BondRetireRequested { bond: bond(3), signature: vec![0xEE; 8] },
            })
            .unwrap(),
            // Does not ride: the chain derives panel bindings.
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: panel_bound() }).unwrap(),
            // Does not ride: a declared collateral nothing on this path locks.
            borsh::to_vec(&PalwLifecycleTxPayloadV2 {
                version: PALW_LIFECYCLE_TX_VERSION_V2,
                object: PalwConsensusObjectV2::BondRegistered {
                    bond: bond(9),
                    pubkey: vec![3u8; 8],
                    operator_pubkey: vec![5u8; 8],
                    collateral: 1_000_000,
                    payout_payload: h64(0x1234), signature: Vec::new() },
            })
            .unwrap(),
            // Wrong wire version.
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: 99, object: panel_bound() }).unwrap(),
            // Undecodable.
            vec![0xFF; 8],
        ];
        for payload in cases {
            let admitted = validate_palw_lifecycle_tx(&payload).is_ok();
            let extracted = !palw_lifecycle_objects_from_accepted_txs_v2(&[carrier(SUBNETWORK_ID_PALW_LIFECYCLE, payload.clone())])
                .objects
                .is_empty();
            assert_eq!(admitted, extracted, "admission and extraction disagree on {payload:?}");
        }
    }
}
