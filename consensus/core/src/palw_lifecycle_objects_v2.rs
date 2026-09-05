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
        | PalwConsensusObjectV2::CourtVerdictPosted { .. }
        // ADR-0075: certification rides an ordinary transaction. Neither object carries a
        // signature because neither needs one — the evidence is graded by the court in the
        // transition, and the class binding is checked against the class's own profile hash.
        | PalwConsensusObjectV2::FamilyCertified { .. }
        | PalwConsensusObjectV2::ClassLaneCertified { .. }
        | PalwConsensusObjectV2::ObjectChunk { .. }
        // **ADR-0080 design A: the split close.** The declaration carries the signature of one of
        // the two bonds the session id binds, checked at acceptance against that bond's registered
        // key — the same split every other court move uses. A chunk carries none and needs none:
        // the declaration already pinned its bytes at its index, so a chunk that is not the pinned
        // preimage is refused by the transition whoever sent it, and requiring a signature would
        // only stop a stranger from paying to deliver a mover's own evidence.
        | PalwConsensusObjectV2::CourtCloseChunk { .. } => Ok(()),
        // **ADR-0082 Decision 2: the dissection's three moves ride, each carrying its own
        // authorisation.** The same stateless/stateful split every other court move uses — this
        // layer checks that a signature is PRESENT, and whether it is the responder's or the
        // challenger's key is the acceptance layer's, where the registry is in hand. Unsigned,
        // either party could write the other's moves: a challenger writing disclosures binds an
        // honest executor to partial sums it never claimed, and a responder writing choices steers
        // the dissection away from its own divergence.
        //
        // Whether the k-ary court is armed at all is also the acceptance layer's, for the reason
        // this list is stateless: a fence is resolved at a DAA score, and this function does not
        // have one.
        PalwConsensusObjectV2::CourtAttnRootClaimed { signature, .. }
        | PalwConsensusObjectV2::CourtAttnDissected { signature, .. }
        | PalwConsensusObjectV2::CourtAttnChildChosen { signature, .. }
            if !signature.is_empty() =>
        {
            Ok(())
        }
        PalwConsensusObjectV2::CourtAttnRootClaimed { .. }
        | PalwConsensusObjectV2::CourtAttnDissected { .. }
        | PalwConsensusObjectV2::CourtAttnChildChosen { .. } => Err(
            "a fused-attention dissection move must carry the signature of the party it is attributed to — unsigned, either side could write the other's moves",
        ),
        PalwConsensusObjectV2::CourtCloseDeclared { signature, .. } if !signature.is_empty() => Ok(()),
        // ADR-0087 Decision 3: a buy is bound to its carrier's sink output below; a sell must carry
        // the holder's signature, checked at acceptance against the payload it names.
        PalwConsensusObjectV2::ModelBuy { .. } => Ok(()),
        // ADR-0090: a seed is bound to its carrier's sink output exactly as a buy is.
        PalwConsensusObjectV2::ModelSeed { .. } => Ok(()),
        PalwConsensusObjectV2::ModelSell { signature, .. } if !signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::ModelSell { .. } => Err("a model sell must carry the holder's signature — unsigned, anyone could drain a position"),
        // ADR-0088: every registry object is attributed to a bond and carries that bond's signature,
        // checked at acceptance against the bond's stored key; unsigned, anyone could publish a
        // version, hand a line over or speak in a line's name.
        PalwConsensusObjectV2::ModelLineFounded { signature, name, .. } => {
            if signature.is_empty() {
                Err("a line founding must carry the founder's signature")
            } else if name.is_empty() || name.len() > crate::palw_model_lines_v1::PALW_MODEL_LINE_NAME_MAX_BYTES {
                Err("a line's name must be 1..=64 bytes")
            } else {
                Ok(())
            }
        }
        PalwConsensusObjectV2::ModelVersionPublished { signature, .. }
        | PalwConsensusObjectV2::ModelVersionPromoted { signature, .. }
        | PalwConsensusObjectV2::ModelVersionWithdrawn { signature, .. }
        | PalwConsensusObjectV2::ModelLineRolesSet { signature, .. }
        | PalwConsensusObjectV2::ModelLineOwnerTransferred { signature, .. }
        | PalwConsensusObjectV2::ModelLineRetired { signature, .. }
        | PalwConsensusObjectV2::ModelProposalPosted { signature, .. }
        | PalwConsensusObjectV2::ModelProposalClosed { signature, .. }
        | PalwConsensusObjectV2::ModelEvaluationPosted { signature, .. } => {
            if signature.is_empty() {
                Err("a model registry object must carry the signature of the bond it is attributed to")
            } else {
                Ok(())
            }
        }
        PalwConsensusObjectV2::CourtCloseDeclared { .. } => Err(
            "a close declaration must carry the signature of the side it declares for — without one either party could write the other's close and pin it to a verdict it never asserted",
        ),
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
        // **A capability declaration rides, and carries its own authorisation** (ADR-0071
        // Decision 3). Same split as retirement: this layer is stateless, so it checks that a
        // signature is PRESENT; whether it is the bond's own is the acceptance layer's, where the
        // registry is in hand. Without one, a relayer could volunteer any bond — a public outpoint
        // — for duty on any class, and the duty accounting convicts the seats the draw names.
        PalwConsensusObjectV2::BondCapabilityDeclared { signature, .. } if !signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::BondCapabilityDeclared { .. } => Err(
            "a capability declaration must carry the owner signature that authorizes it — a bond key is a public outpoint, so without one anyone could volunteer anyone's collateral for duty",
        ),
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
        // **ADR-0078: a derivation rides, and carries the executor's authorisation.** Same split
        // as a bond's declarations: this layer is stateless, so it checks SHAPE — the object's
        // version, a non-zero kind, an ML-DSA-87-sized executor key, a non-empty artifact — and
        // that a signature is present. Whether the signature is the declared key's is the
        // acceptance layer's; whether the declared key is the claim's bond key is the
        // transition's, where the registry and the claim table are in hand.
        //
        // **And the signature's LENGTH is pinned here, because this is the only layer that can
        // pin it** (audit 2026-09-02, X1). "Present" was the whole rule, which made the one
        // object that must never carry bytes the only lifecycle object with a free-length byte
        // field — and this list is a BLOCK rule (`tx_validation_in_isolation`) while the
        // acceptance layer merely drops an object and lets the block stand, so a refusal there
        // would leave the bytes in the chain. Pinned, a derivation's wire size is a constant.
        PalwConsensusObjectV2::DerivedArtifactV1 { signature, .. } if signature.is_empty() => Err(
            "a derived artifact must carry the claim executor's signature — without one anyone could put their name on a derivation of anyone's answer",
        ),
        PalwConsensusObjectV2::DerivedArtifactV1 { object, signature } => {
            crate::palw_derived_v1::check_derived_carriage_v1(object, signature)
        }
        // **ADR-0062 SA-1/SA-2: both DA-court objects ride, and both carry their own
        // authorisation.** Same stateless/stateful split as retirement: this layer checks that a
        // signature is PRESENT and that a disclosure fits the close ceiling; whether the signature
        // is the accuser's bond key or the claim's producer key is the acceptance layer's, where
        // the registry and the claim are in hand.
        //
        // Carriage is deliberately permissionless for the disclosure (SA-3): the object is signed
        // by the producer's bond, so who carried it is irrelevant, and that is precisely what makes
        // suppressing it cost an attacker every producer for a whole window instead of one.
        PalwConsensusObjectV2::DefaultAccused { signature, .. } if !signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::DefaultAccused { .. } => Err(
            "a data-availability accusation must carry the accuser's signature — a bond key is a public outpoint, so without one anyone could accuse under a stranger's identity",
        ),
        PalwConsensusObjectV2::MaterialDisclosed { preimage, signature, .. } if !signature.is_empty() => {
            // The ceiling a close is priced at, applied to the disclosure for the reason the ADR
            // measures: a FLAT event preimage at a Qwen-class vocabulary is 607,744 bytes, 7.4× the
            // whole budget, so an unbounded disclosure would be a block-sized object nobody priced.
            // The class's OWN registered ceiling is checked at acceptance, where the bundle is.
            if preimage.len() > crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES as usize {
                return Err("a data-availability disclosure is above the close-byte ceiling this ruleset prices");
            }
            Ok(())
        }
        PalwConsensusObjectV2::MaterialDisclosed { .. } => Err(
            "a data-availability disclosure must carry the producer's signature — unsigned, a third party could bind a producer to material it never published",
        ),
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
    // **Named by index, with a zero id.** The output really must belong to the carrying
    // transaction — that is what makes the money a fact rather than a claim — but the registration
    // cannot NAME the carrier by id: the object travels in the payload, and `write_transaction`
    // folds the payload into the id, so an outpoint naming its own carrier is a hash fixed point.
    // A registrant would have to find a payload containing the id of the transaction that payload
    // produces. The zero id is "this carrier", and the chain substitutes the id it observes
    // (`palw_bond_registration_keyed_to_its_carrier_v2`). The index is checked against the outputs
    // below, so "belongs to this transaction" is enforced exactly as before.
    if bond.0.transaction_id != TransactionId::default() {
        return Err("a bond registration must name its collateral output by index, with a zero transaction id");
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

/// **The bond key a registrant can actually SIGN.**
///
/// A carried registration names its collateral output by index with a zero transaction id, because
/// the carrier's id is a function of the payload the signature goes into — see
/// [`palw_bond_registration_binds_its_carrier_v2`]. So the signature is made over the zero form,
/// and every verifier has to rebuild that same form rather than the substituted one.
///
/// One function, two call sites — the extractor's substitution and the block validator's signature
/// check — because a registrant and a verifier that disagreed about which bytes were signed would
/// reject every honest registration with "not signed by the key it declares".
/// **ADR-0087 Decision 3: a buy's carrier pays `msk_in` to the class's sink**, at the output the
/// object names, or the object does not ride. The value is read off the carrier and never off
/// the object alone, so the reserve the fold credits is MSK that left a spendable output.
pub fn palw_model_buy_binds_its_carrier_v1(tx: &Transaction, object: &PalwConsensusObjectV2) -> Result<(), &'static str> {
    // ADR-0090: a seed is bound the same way — the whole seed sits in the line's sink.
    let (line_id, msk, sink_index, what) = match object {
        PalwConsensusObjectV2::ModelBuy { line_id, msk_in, sink_index, .. } => (line_id, msk_in, sink_index, "buy"),
        PalwConsensusObjectV2::ModelSeed { line_id, msk_seed, sink_index, .. } => (line_id, msk_seed, sink_index, "seed"),
        _ => return Ok(()),
    };
    let Some(output) = tx.outputs.get(*sink_index as usize) else {
        return Err(if what == "buy" {
            "a model buy names a sink output its carrier does not have"
        } else {
            "a model seed names a sink output its carrier does not have"
        });
    };
    if output.value != *msk {
        return Err(if what == "buy" {
            "a model buy declares an MSK leg its sink output does not hold"
        } else {
            "a model seed declares an MSK seed its sink output does not hold"
        });
    }
    if crate::palw_model_market_v1::palw_model_sink_class_v1(&output.script_public_key) != Some(*line_id) {
        return Err(if what == "buy" {
            "a model buy's sink output must be the class's own sink script"
        } else {
            "a model seed's sink output must be the line's own sink script"
        });
    }
    Ok(())
}

pub fn palw_bond_registration_signed_key_v2(bond: &crate::palw_state_v2::PalwBondKeyV2) -> crate::palw_state_v2::PalwBondKeyV2 {
    crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(TransactionId::default(), bond.0.index))
}

/// Key a carried bond registration to the transaction that carried it.
///
/// Every other object passes through unchanged: this substitution exists only because a bond names
/// an output of its own carrier, and only a carrier knows its own id.
pub fn palw_bond_registration_keyed_to_its_carrier_v2(carrier: TransactionId, object: PalwConsensusObjectV2) -> PalwConsensusObjectV2 {
    match object {
        PalwConsensusObjectV2::BondRegistered {
            bond,
            pubkey,
            operator_pubkey,
            collateral,
            payout_payload,
            capable_classes,
            signature,
        } => PalwConsensusObjectV2::BondRegistered {
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(carrier, bond.0.index)),
            pubkey,
            operator_pubkey,
            collateral,
            payout_payload,
            capable_classes,
            signature,
        },
        other => other,
    }
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
        if let Err(reason) = palw_model_buy_binds_its_carrier_v1(tx, &payload.object) {
            out.skipped.push((id, reason));
            continue;
        }
        // The chain supplies the half the registrant could not: the outpoint the bond is keyed
        // under from here on is a real one, so state, exposure and `--palw-producer-bond` all name
        // the same output the collateral sits in.
        let object = palw_bond_registration_keyed_to_its_carrier_v2(id, payload.object);
        out.objects.push(PalwLifecycleCarrierV2 { carrier: id, object });
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
    /// **…except no registrant can build one, and the test above did not notice.**
    ///
    /// `a_bond_registration_that_locks_its_collateral_rides` constructs the transaction with an
    /// EMPTY payload, takes its id, and only then builds the object naming that id. That pair is
    /// consistent, so the lock accepts it — but it is not a transaction anyone can broadcast,
    /// because the object has to travel IN the payload and `write_transaction` folds the payload
    /// into the id (`hashing/tx.rs`). Put the object where it must go and the id moves out from
    /// under the outpoint that names it.
    ///
    /// So `bond.transaction_id == tx.id()` is a hash fixed point: to satisfy it a registrant would
    /// have to find a payload containing the id of the transaction that payload produces. That is
    /// preimage resistance, not an engineering problem.
    ///
    /// The consequence is the one an operator on testnet-11 reported and was told was fixed: no
    /// bond can enter after genesis, so only the holders of the genesis registry can ever produce.
    /// The rule is not wrong about what it wants — the carrier really should prove the money — it
    /// is wrong about how the carrier can name itself. See
    /// [`palw_bond_registration_binds_its_carrier_v2`] for the form that is constructible.
    #[test]
    fn naming_the_carrier_by_id_is_a_fixed_point_no_registrant_can_solve() {
        let payee = h64(0xBEEF);
        let owner: [u8; 64] = *payee.as_byte_slice();
        let spk = crate::dns_finality::p2pkh_mldsa87_spk(&owner);
        let outputs = vec![TransactionOutput::new(500_000, spk)];

        // The carrier, built the only way a registrant can build one: the object goes in the
        // payload, because that is the only place the chain reads it from.
        let carrier = |named: TransactionId| {
            let object = PalwConsensusObjectV2::BondRegistered {
                bond: PalwBondKeyV2(TransactionOutpoint::new(named, 0)),
                pubkey: vec![7; 4],
                operator_pubkey: vec![21; 8],
                collateral: 500_000,
                payout_payload: payee,
                capable_classes: Default::default(),
                signature: vec![1; 8],
            };
            let payload = borsh::to_vec(&crate::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2 {
                version: PALW_LIFECYCLE_TX_VERSION_V2,
                object,
            })
            .expect("the lifecycle payload serializes");
            Transaction::new(0, vec![], outputs.clone(), 0, SUBNETWORK_ID_PALW_LIFECYCLE.clone(), 0, payload)
        };

        // The registrant's best move: name the id the carrier would have had, then look at the id
        // it actually has once that name is inside it.
        let probe = carrier(TransactionId::default());
        let attempt = carrier(probe.id());
        assert_ne!(attempt.id(), probe.id(), "writing the id into the payload moves the id");

        // And the chain refuses it, by the rule's own words.
        let extracted = crate::palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2(&[attempt]);
        assert!(extracted.objects.is_empty(), "no bond may enter through a transaction under this rule");
        assert_eq!(
            extracted.skipped.first().map(|(_, why)| *why),
            Some("a bond registration must name its collateral output by index, with a zero transaction id"),
            "and the refusal is the carrier-binding rule, pointing at the form that IS constructible"
        );
    }

    /// **A bond CAN enter through a transaction — this is what it has to prove.**
    ///
    /// The carrier proves the money and the signature proves the owner. Neither is a promise: the
    /// output is created by the very transaction carrying the registration, so its existence,
    /// amount and script are facts block validation established before this object was decoded.
    ///
    /// Driven through the REAL round trip — object into the payload, payload into the transaction,
    /// transaction through the extractor — because the earlier version of this test built the
    /// transaction with an empty payload and only then named its id. That pair was consistent, so
    /// the rule accepted it, and the test reported a capability nobody could use: putting the
    /// object where it must go moves the id out from under the outpoint naming it. A bond seam is
    /// only proven by the trip a registrant actually makes.
    #[test]
    fn a_bond_registration_that_locks_its_collateral_rides() {
        let payee = h64(0xBEEF);
        let owner: [u8; 64] = *payee.as_byte_slice();
        let spk = crate::dns_finality::p2pkh_mldsa87_spk(&owner);
        let object = PalwConsensusObjectV2::BondRegistered {
            // Named by index with a zero id: "the output at index 0 of whatever carries me".
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(TransactionId::default(), 0)),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21; 8],
            collateral: 500_000,
            payout_payload: payee,
            capable_classes: Default::default(),
            signature: vec![1; 8],
        };
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object })
            .expect("the lifecycle payload serializes");
        let tx = Transaction::new(
            0,
            vec![],
            vec![TransactionOutput::new(500_000, spk)],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE.clone(),
            0,
            payload,
        );

        let extracted = palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(&tx));
        assert!(extracted.skipped.is_empty(), "nothing should be skipped: {:?}", extracted.skipped);
        let [carried] = &extracted.objects[..] else { panic!("exactly one object rides") };
        let PalwConsensusObjectV2::BondRegistered { bond, collateral, .. } = &carried.object else {
            panic!("and it is the bond registration")
        };
        // The chain supplied the half the registrant could not.
        assert_eq!(bond.0.transaction_id, tx.id(), "the bond is keyed to the transaction that carried it");
        assert_eq!(bond.0.index, 0, "at the output it named");
        assert_eq!(*collateral, 500_000);
        // And the verifier can rebuild exactly what was signed, from the substituted key alone.
        assert_eq!(
            palw_bond_registration_signed_key_v2(bond).0,
            crate::tx::TransactionOutpoint::new(TransactionId::default(), 0),
            "a verifier recovers the signed form without needing the carrier"
        );
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
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(
                // `t` selects which id the lie uses: the zero sentinel for the honest form, and a
                // real id for lie 3, which is what naming somebody else's transaction now looks like.
                if t.payload.is_empty() && t.outputs.first().map(|o| o.value) == Some(999) {
                    t.id()
                } else {
                    TransactionId::default()
                },
                index,
            )),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21; 8],
            collateral,
            payout_payload: payee,
            capable_classes: Default::default(),
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
        assert!(e.contains("by index, with a zero transaction id"), "{e}");

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
            capable_classes: Default::default(),
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
            // Built through the transition's own `#[cfg(test)]` fixture, because ADR-0063 SA-5
            // left `ClassFrozen` with no constructor outside `palw_state_v2` — this test could
            // not spell the object by hand even to prove the door is shut, which is the point.
            (crate::palw_state_v2::tests::freeze(h64(1)), "no layer verifies"),
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
            payout_payload: h64(0x9A11),
            capable_classes: Default::default(),
            signature: Vec::new(),
        };
        // A class registration with NO admission material: still refused, because there is
        // nothing to check it with (ADR-0049 Decision H replaced the blanket refusal with a gate,
        // and a gate needs an input).
        let class_registration = PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(0xC1A55),
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
            work_leaves: 8,
            prompt_token_ids_hash: h64(0x7E),
            decode_tokens_executed: 2,
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
                    payout_payload: h64(0x1234),
                    capable_classes: Default::default(),
                    signature: Vec::new(),
                },
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

    /// ADR-0075: both certification objects ride the lifecycle subnetwork — admission and
    /// extraction give one answer — and neither needs a carrier-bound outpoint, so the object
    /// extracted is the object carried, byte for byte.
    #[test]
    fn certification_objects_ride_and_extract_unchanged() {
        use crate::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
        use crate::palw_e2e_adjudicability::{PalwE2eDrillEvidenceV1, palw_e2e_family_id_v1};
        use crate::palw_state_v2::{PalwCertificationEvidenceV1, PalwCertifiedLaneV1};

        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor's profile");
        let bind = PalwConsensusObjectV2::ClassLaneCertified {
            class_id: profile.shape_profile_id(),
            lane: PalwCertifiedLaneV1::FreePrompt,
            profile: Box::new(profile.clone()),
        };
        let family = PalwConsensusObjectV2::FamilyCertified {
            evidence: Box::new(PalwCertificationEvidenceV1::Attempt(PalwE2eDrillEvidenceV1 {
                family_id: palw_e2e_family_id_v1("RIDES"),
                profile,
                artifact_root: h64(9),
                vectors: Vec::new(),
                malformed_inputs_refused: 0,
            })),
        };
        for object in [bind, family] {
            let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() })
                .expect("serializes");
            validate_palw_lifecycle_tx(&payload).expect("a certification object may ride");
            let tx = carrier(SUBNETWORK_ID_PALW_LIFECYCLE.clone(), payload);
            let extracted = palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(&tx));
            assert!(extracted.skipped.is_empty(), "{:?}", extracted.skipped);
            assert_eq!(extracted.objects.len(), 1);
            assert_eq!(extracted.objects[0].object, object, "extracted unchanged — nothing is keyed to the carrier");
            assert_eq!(extracted.objects[0].carrier, tx.id());
        }
    }

    /// ADR-0078: a derivation rides the lifecycle subnetwork with its signature and its shape,
    /// extracts unchanged, and is refused — admission and extraction agreeing — when the signature
    /// is missing or the shape is wrong.
    #[test]
    fn derived_artifacts_ride_signed_and_shaped_and_extract_unchanged() {
        use crate::palw_derived_v1::{
            PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN, PALW_DERIVED_V1_SIGNATURE_LEN, PALW_DERIVED_V1_VERSION, PalwDerivedArtifactV1, kind,
        };
        let object = PalwDerivedArtifactV1 {
            version: PALW_DERIVED_V1_VERSION,
            network_domain: h64(1),
            claim_id: h64(2),
            output_root: h64(3),
            grammar_id: h64(4),
            transformer_id: h64(5),
            kind: kind::MUSIC,
            dsl_hash: h64(6),
            artifact_hash: h64(7),
            artifact_bytes: 99,
            executor_pubkey: vec![9; PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN],
        };
        let signed = PalwConsensusObjectV2::DerivedArtifactV1 {
            object: Box::new(object.clone()),
            signature: vec![1; PALW_DERIVED_V1_SIGNATURE_LEN],
        };
        let payload =
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: signed.clone() }).unwrap();
        validate_palw_lifecycle_tx(&payload).expect("a signed, shaped derivation may ride");
        let tx = carrier(SUBNETWORK_ID_PALW_LIFECYCLE.clone(), payload);
        let extracted = palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(&tx));
        assert!(extracted.skipped.is_empty(), "{:?}", extracted.skipped);
        assert_eq!(extracted.objects[0].object, signed, "extracted unchanged");

        let unsigned = PalwConsensusObjectV2::DerivedArtifactV1 { object: Box::new(object.clone()), signature: Vec::new() };
        let mut zero_kind = object.clone();
        zero_kind.kind = 0;
        let unshaped = PalwConsensusObjectV2::DerivedArtifactV1 {
            object: Box::new(zero_kind),
            signature: vec![1; PALW_DERIVED_V1_SIGNATURE_LEN],
        };
        // **X1: a free-length signature is where a GLB would go.** A refusal at the ACCEPTANCE
        // layer drops the object and lets the block stand, so bytes refused there still ride an
        // accepted transaction forever; this list is a block rule, so it is where "under any
        // size" is enforced. A 4 MiB signature is refused by name, exactly like a 16-byte one.
        let overlong = PalwConsensusObjectV2::DerivedArtifactV1 { object: Box::new(object.clone()), signature: vec![0xAB; 4 << 20] };
        let short = PalwConsensusObjectV2::DerivedArtifactV1 { object: Box::new(object.clone()), signature: vec![1; 16] };
        for (refused, why) in
            [(unsigned, "signature"), (unshaped, "kind 0"), (overlong, "free-length field"), (short, "free-length field")]
        {
            let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: refused }).unwrap();
            let err = validate_palw_lifecycle_tx(&payload).expect_err("refused at admission");
            assert!(format!("{err:?}").contains(why), "{err:?}");
            let tx = carrier(SUBNETWORK_ID_PALW_LIFECYCLE.clone(), payload);
            let extracted = palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(&tx));
            assert!(extracted.objects.is_empty(), "and skipped by the walk");
            assert_eq!(extracted.skipped.len(), 1);
        }
    }
}
