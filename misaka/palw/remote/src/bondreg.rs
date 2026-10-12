//! **Node-less bond registration** (RFC-0009 stage A0, mode B — H1's real-user loop, 2026-10-08: `NODELESS_BOND_REGISTRATION_ABSENT`).
//!
//! Until now the only way to put a bond on a chain was `kaspad --palw-register-bond`, which runs a full node. The same three steps as the
//! class registration's detached flow ([`crate::bundle`]), on three possibly different machines, the key present only in the second:
//!
//! ```text
//!   export (no key)   the unsigned BondRegistered + its carrier plan (collateral output, change, fee priced by the caller) → bundle
//!   sign (offline)    the owner key signs the registration (and, past palw_operator_id_unique, its operator possession); the payer key
//!                     signs the carrier's funding input — each refuses a bundle that does not hold (recipient, collateral, payout, fee)
//!   submit (no key)   the signed bytes, verified from themselves, relayed through any nodes; the bond is (carrier txid : 0)
//! ```
//!
//! What the owner signs (`palw_bond_registration_message_v2`) binds the network, the key, the operator key, the collateral, the payout and
//! the declared classes — so a builder or relay that moves the payout, the collateral or the classes invalidates the signature; and the
//! chain requires the collateral output to pay exactly the payout's script (`palw_bond_registration_binds_its_carrier_v2`), so the money
//! returns to whoever the rewards go to. The registration is position-free (it names its collateral as output 0 of "this carrier"), so
//! the owner can sign before the carrier exists.

use kaspa_consensus_core::constants::{MAX_TX_IN_SEQUENCE_NUM, TX_VERSION};
use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::palw_lifecycle_objects_v2::{
    PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2, palw_bond_registration_binds_its_carrier_v2,
    palw_bond_registration_signed_key_v2,
};
use kaspa_consensus_core::palw_state_v2::{
    PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT, PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT, PalwBondKeyV2, PalwConsensusObjectV2,
    palw_bond_registration_message_v2, palw_operator_possession_message_v1,
};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
use kaspa_consensus_core::tx::{
    MutableTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput,
    UtxoEntry,
};
use kaspa_hashes::Hash64;
use kaspa_txscript::script_builder::ScriptBuilder;
use kaspa_txscript::{MLDSA87_SIG_LEN, MLDSA87_TX_CONTEXT, verify_mldsa87_with_context};
use serde::{Deserialize, Serialize};

use crate::bundle::{DetachedSigner, FundingInputV1, funding_spk_of_pubkey_v1, spk_text_v1};
use crate::relay::{carrier_funding_signature_valid, tx_id_of_bytes};
use crate::trust::UNVERIFIED_REMOTE_STATE;

pub const BOND_BUNDLE_SCHEMA_V1: &str = "misaka.palw.bond-bundle.v1";
pub const BOND_SIGNED_SCHEMA_V1: &str = "misaka.palw.bond-signed.v1";

fn hex(bytes: &[u8]) -> String {
    faster_hex::hex_string(bytes)
}

fn unhex(s: &str) -> Result<Vec<u8>, BondRefusalV1> {
    let mut out = vec![0u8; s.len() / 2];
    if s.len() % 2 != 0 || faster_hex::hex_decode(s.as_bytes(), &mut out).is_err() {
        return Err(BondRefusalV1::Malformed("a hex field is not hex".into()));
    }
    Ok(out)
}

fn parse_spk(s: &str) -> Result<ScriptPublicKey, BondRefusalV1> {
    s.parse::<ScriptPublicKey>().map_err(|_| BondRefusalV1::Malformed("a script field is not a script".into()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BondStageV1 {
    Unsigned,
    OwnerSigned,
}

/// **The unsigned bond registration and its carrier, as a file** — what a key-less builder writes and an offline signer re-derives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BondBundleV1 {
    pub schema: String,
    pub stage: BondStageV1,
    pub network: String,
    pub network_domain: Hash64,
    pub ruleset_id: String,
    /// The bond key (hex). The operator key is the same key (a self-operated bond, as `kaspad --palw-register-bond` makes).
    pub pubkey: String,
    pub collateral: u64,
    /// The payout payload: the collateral output pays `p2pkh_mldsa87_spk(payout)`, and so do the bond's rewards.
    pub payout_payload: Hash64,
    /// Declared classes (none by default — `misaka bond capability` changes them later, signed by the bond).
    pub capable_classes: Vec<Hash64>,
    /// Past `palw_operator_id_unique` the signature carries the operator's possession proof too. A ruleset fact at the quote's DAA.
    pub operator_possession: bool,
    /// The registration signature (hex), empty until the owner signs.
    pub signature: String,
    /// The payer: its address (display), its funding script and the single input it spends.
    pub payer_address: String,
    pub payer_spk: String,
    pub funding: FundingInputV1,
    pub fee_sompi: u64,
    /// The network's registration floor, from the CLIENT's own ruleset (`palw_bond_registration_floor_v1`) — not a node's word.
    pub collateral_floor_sompi: u64,
    /// The DAA past which the quote is stale (the signer refuses past it when it has a clock).
    pub expiry_daa: u64,
    /// The nodes the builder read (`UNVERIFIED_REMOTE_STATE`).
    pub sources: Vec<String>,
    pub remote_state: String,
}

impl BondBundleV1 {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a bundle is serializable")
    }
    pub fn from_json(text: &str) -> Result<Self, BondRefusalV1> {
        let b: Self = serde_json::from_str(text).map_err(|e| BondRefusalV1::Malformed(format!("not a bond bundle: {e}")))?;
        if b.schema != BOND_BUNDLE_SCHEMA_V1 {
            return Err(BondRefusalV1::Malformed(format!("schema {:?}", b.schema)));
        }
        Ok(b)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BondRefusalV1 {
    #[error("malformed: {0}")]
    Malformed(String),
    #[error("the bundle is for network {got:?}, this build runs {want:?}")]
    WrongNetwork { got: String, want: String },
    #[error("the bundle names another network domain or ruleset than this build's")]
    WrongRuleset,
    #[error("the bond key in the bundle is not the key signing it")]
    OwnerKeyMismatch,
    #[error("the collateral {got} is not the {want} you expect")]
    CollateralNotExpected { got: u64, want: u64 },
    #[error("the collateral {got} is below the network's registration floor {floor}")]
    BelowFloor { got: u64, floor: u64 },
    #[error("the payout is not the one you expect: the collateral and every reward would go to {0}")]
    PayoutNotExpected(String),
    #[error("the funding input is not locked to this payer key's own address")]
    PayerIsNotThisKey,
    #[error("the change does not go back to the payer")]
    ChangeNotPayer,
    #[error("the fee {fee} is above your cap {cap}")]
    FeeAboveCap { fee: u64, cap: u64 },
    #[error("the funding ({funding}) does not cover the collateral {collateral} and the fee {fee}")]
    CarrierArithmetic { funding: u64, collateral: u64, fee: u64 },
    #[error("the quote expired at DAA {expiry} (now {now})")]
    Expired { expiry: u64, now: u64 },
    #[error("the stage is {0:?}; this step needs another")]
    Stage(BondStageV1),
    #[error("the owner signature does not verify over what the bundle registers")]
    OwnerSignature,
    #[error("the carrier does not hold: {0}")]
    Carrier(String),
    #[error("the signer failed: {0}")]
    Signer(String),
}

/// What the signer holds and expects — the user's own facts; the bundle is never believed about them.
#[derive(Clone, Debug)]
pub struct BondSignerPolicyV1 {
    pub network: String,
    pub network_domain: Hash64,
    pub ruleset_id: String,
    pub collateral_floor_sompi: u64,
    /// The collateral the user means to lock (refused on any other).
    pub expect_collateral: u64,
    /// The payout payload the user means (default: the bond key's own address payload).
    pub expect_payout: Hash64,
    pub max_fee_sompi: u64,
    pub now_daa: Option<u64>,
}

/// The P2PKH-ML-DSA-87 address payload a key's own funding address uses — the default payout of a self-registered bond.
pub fn own_payout_payload_v1(pubkey: &[u8]) -> Hash64 {
    Hash64::from_bytes(kaspa_hashes::blake2b_512_address_payload(pubkey).as_bytes())
}

fn classes(b: &BondBundleV1) -> std::collections::BTreeSet<Hash64> {
    b.capable_classes.iter().copied().collect()
}

/// The bond key the registration names: output 0 of "this carrier" (zero id, substituted by the chain).
pub fn carrier_bond_key_v1() -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(TransactionId::default(), 0))
}

/// **The message the owner signs** — re-derived from the bundle's fields by every party (the bundle carries no message field to lie with).
pub fn bond_registration_message_of_v1(b: &BondBundleV1) -> Result<Hash64, BondRefusalV1> {
    let pubkey = unhex(&b.pubkey)?;
    Ok(palw_bond_registration_message_v2(
        b.network_domain,
        &palw_bond_registration_signed_key_v2(&carrier_bond_key_v1()),
        &pubkey,
        &pubkey,
        b.collateral,
        &b.payout_payload,
        &classes(b),
    ))
}

/// The registration object, with the bundle's signature (empty until signed).
pub fn bond_object_of_v1(b: &BondBundleV1) -> Result<PalwConsensusObjectV2, BondRefusalV1> {
    let pubkey = unhex(&b.pubkey)?;
    Ok(PalwConsensusObjectV2::BondRegistered {
        bond: carrier_bond_key_v1(),
        pubkey: pubkey.clone(),
        operator_pubkey: pubkey,
        collateral: b.collateral,
        payout_payload: b.payout_payload,
        capable_classes: classes(b),
        signature: unhex(&b.signature)?,
    })
}

/// **The carrier body**: output 0 the collateral to the payout's script, output 1 the change to the payer's own script, the object in a
/// lifecycle payload. `signature_script` empty for an unsigned body, the placeholder for pricing.
pub fn bond_carrier_body_v1(
    b: &BondBundleV1,
    object: &PalwConsensusObjectV2,
    signature_script: Vec<u8>,
) -> Result<Transaction, BondRefusalV1> {
    let payer_spk = parse_spk(&b.payer_spk)?;
    let needed = b.collateral.saturating_add(b.fee_sompi);
    if b.funding.amount <= needed {
        return Err(BondRefusalV1::CarrierArithmetic { funding: b.funding.amount, collateral: b.collateral, fee: b.fee_sompi });
    }
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() })
        .map_err(|e| BondRefusalV1::Malformed(e.to_string()))?;
    let input =
        TransactionInput::new(TransactionOutpoint::new(b.funding.txid, b.funding.index), signature_script, MAX_TX_IN_SEQUENCE_NUM, 1);
    let outputs = vec![
        TransactionOutput::new(
            b.collateral,
            kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(b.payout_payload.as_byte_slice()),
        ),
        TransactionOutput::new(b.funding.amount - needed, payer_spk),
    ];
    Ok(Transaction::new(TX_VERSION, vec![input], outputs, 0, SUBNETWORK_ID_PALW_LIFECYCLE, 0, payload))
}

/// **Every check a signer makes before any key is touched**, against the USER's policy.
pub fn check_bond_bundle_v1(b: &BondBundleV1, policy: &BondSignerPolicyV1) -> Result<(), BondRefusalV1> {
    if b.network != policy.network {
        return Err(BondRefusalV1::WrongNetwork { got: b.network.clone(), want: policy.network.clone() });
    }
    if b.network_domain != policy.network_domain || b.ruleset_id != policy.ruleset_id {
        return Err(BondRefusalV1::WrongRuleset);
    }
    if b.collateral != policy.expect_collateral {
        return Err(BondRefusalV1::CollateralNotExpected { got: b.collateral, want: policy.expect_collateral });
    }
    if b.collateral < policy.collateral_floor_sompi {
        return Err(BondRefusalV1::BelowFloor { got: b.collateral, floor: policy.collateral_floor_sompi });
    }
    if b.payout_payload != policy.expect_payout {
        return Err(BondRefusalV1::PayoutNotExpected(b.payout_payload.to_string()));
    }
    if b.fee_sompi > policy.max_fee_sompi {
        return Err(BondRefusalV1::FeeAboveCap { fee: b.fee_sompi, cap: policy.max_fee_sompi });
    }
    if let Some(now) = policy.now_daa
        && now > b.expiry_daa
    {
        return Err(BondRefusalV1::Expired { expiry: b.expiry_daa, now });
    }
    bond_carrier_body_v1(b, &bond_object_of_v1(b)?, vec![]).map(|_| ())
}

/// **Step 2a: the owner signs the registration** (and its operator possession past the fence). The signer's key must BE the bond key.
pub fn bond_owner_sign_v1(
    b: &BondBundleV1,
    policy: &BondSignerPolicyV1,
    owner: &dyn DetachedSigner,
) -> Result<BondBundleV1, BondRefusalV1> {
    if b.stage != BondStageV1::Unsigned {
        return Err(BondRefusalV1::Stage(b.stage));
    }
    check_bond_bundle_v1(b, policy)?;
    let pubkey = unhex(&b.pubkey)?;
    if owner.public_key() != pubkey {
        return Err(BondRefusalV1::OwnerKeyMismatch);
    }
    let message = bond_registration_message_of_v1(b)?;
    let mut signature =
        owner.sign_with_context(message.as_byte_slice(), PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT).map_err(BondRefusalV1::Signer)?;
    if b.operator_possession {
        let possession = palw_operator_possession_message_v1(
            b.network_domain,
            &palw_bond_registration_signed_key_v2(&carrier_bond_key_v1()),
            &pubkey,
            &pubkey,
        );
        signature.extend(
            owner
                .sign_with_context(possession.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT)
                .map_err(BondRefusalV1::Signer)?,
        );
    }
    let mut out = b.clone();
    out.signature = hex(&signature);
    out.stage = BondStageV1::OwnerSigned;
    verify_owner_signature_v1(&out)?;
    Ok(out)
}

/// The owner signature (and possession) over what the bundle registers, verified from the bundle alone.
pub fn verify_owner_signature_v1(b: &BondBundleV1) -> Result<(), BondRefusalV1> {
    let pubkey = unhex(&b.pubkey)?;
    let sig = unhex(&b.signature)?;
    let message = bond_registration_message_of_v1(b)?;
    let first = sig.get(..MLDSA87_SIG_LEN).ok_or(BondRefusalV1::OwnerSignature)?;
    if !matches!(
        verify_mldsa87_with_context(&pubkey, message.as_byte_slice(), first, PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT),
        Ok(true)
    ) {
        return Err(BondRefusalV1::OwnerSignature);
    }
    match (b.operator_possession, sig.len()) {
        (false, n) if n == MLDSA87_SIG_LEN => Ok(()),
        (true, n) if n == 2 * MLDSA87_SIG_LEN => {
            let possession = palw_operator_possession_message_v1(
                b.network_domain,
                &palw_bond_registration_signed_key_v2(&carrier_bond_key_v1()),
                &pubkey,
                &pubkey,
            );
            if matches!(
                verify_mldsa87_with_context(
                    &pubkey,
                    possession.as_byte_slice(),
                    &sig[MLDSA87_SIG_LEN..],
                    PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT
                ),
                Ok(true)
            ) {
                Ok(())
            } else {
                Err(BondRefusalV1::OwnerSignature)
            }
        }
        _ => Err(BondRefusalV1::OwnerSignature),
    }
}

/// **The signed carrier as a file** — what `submit` relays and anyone verifies from the bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedBondRegistrationV1 {
    pub schema: String,
    pub network: String,
    pub network_domain: Hash64,
    pub ruleset_id: String,
    pub pubkey: String,
    pub collateral: u64,
    pub payout_payload: Hash64,
    pub payer_spk: String,
    pub funding: FundingInputV1,
    pub fee_sompi: u64,
    pub expiry_daa: u64,
    pub tx_hex: String,
    pub tx_id: Hash64,
    /// The bond the chain will key: `<carrier txid>:0`.
    pub bond: String,
}

impl SignedBondRegistrationV1 {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serializable")
    }
    pub fn from_json(text: &str) -> Result<Self, BondRefusalV1> {
        let s: Self =
            serde_json::from_str(text).map_err(|e| BondRefusalV1::Malformed(format!("not a signed bond registration: {e}")))?;
        if s.schema != BOND_SIGNED_SCHEMA_V1 {
            return Err(BondRefusalV1::Malformed(format!("schema {:?}", s.schema)));
        }
        Ok(s)
    }
    pub fn transaction(&self) -> Result<Transaction, BondRefusalV1> {
        borsh::from_slice(&unhex(&self.tx_hex)?).map_err(|e| BondRefusalV1::Malformed(format!("the transaction does not decode: {e}")))
    }
}

/// **Step 2b: the payer signs the carrier's one funding input** — after the owner, because the sighash covers the payload. The payer may
/// be another key than the bond's; its funding must be locked to its own address.
pub fn bond_carrier_sign_v1(
    b: &BondBundleV1,
    policy: &BondSignerPolicyV1,
    payer: &dyn DetachedSigner,
) -> Result<SignedBondRegistrationV1, BondRefusalV1> {
    if b.stage != BondStageV1::OwnerSigned {
        return Err(BondRefusalV1::Stage(b.stage));
    }
    check_bond_bundle_v1(b, policy)?;
    verify_owner_signature_v1(b)?;
    let payer_pk = payer.public_key();
    let payer_spk = parse_spk(&b.payer_spk)?;
    if funding_spk_of_pubkey_v1(&payer_pk) != payer_spk {
        return Err(BondRefusalV1::PayerIsNotThisKey);
    }
    let object = bond_object_of_v1(b)?;
    let mut tx = bond_carrier_body_v1(b, &object, vec![])?;
    let entry = UtxoEntry::new(b.funding.amount, payer_spk, b.funding.block_daa_score, b.funding.is_coinbase);
    let mtx = MutableTransaction::with_entries(tx.clone(), vec![entry.clone()]);
    let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &Mldsa87SigHashReusedValuesUnsync::new());
    let mut sig = payer.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).map_err(BondRefusalV1::Signer)?;
    if sig.len() != MLDSA87_SIG_LEN {
        return Err(BondRefusalV1::Signer(format!("a signature of {} bytes", sig.len())));
    }
    sig.push(SIG_HASH_ALL.to_u8());
    tx.inputs[0].signature_script = ScriptBuilder::new()
        .add_data(&sig)
        .and_then(|s| s.add_data(&payer_pk))
        .map(|s| s.drain())
        .map_err(|e| BondRefusalV1::Signer(e.to_string()))?;
    tx.finalize();
    carrier_funding_signature_valid(&tx, &entry).map_err(BondRefusalV1::Carrier)?;
    let tx_id = tx_id_of_bytes(&tx);
    Ok(SignedBondRegistrationV1 {
        schema: BOND_SIGNED_SCHEMA_V1.into(),
        network: b.network.clone(),
        network_domain: b.network_domain,
        ruleset_id: b.ruleset_id.clone(),
        pubkey: b.pubkey.clone(),
        collateral: b.collateral,
        payout_payload: b.payout_payload,
        payer_spk: b.payer_spk.clone(),
        funding: b.funding.clone(),
        fee_sompi: b.fee_sompi,
        expiry_daa: b.expiry_daa,
        tx_hex: hex(&borsh::to_vec(&tx).expect("a transaction serializes")),
        tx_id,
        bond: format!("{tx_id}:0"),
    })
}

/// **What anyone verifies from the signed bytes alone** before relaying (a relay that altered a byte is caught here and by every node):
/// the id, the payload is a signed `BondRegistered` whose fields are the file's, the owner signature, the carrier binding (output 0 is the
/// collateral to the payout's script), the change to the payer, the fee, the funding signature.
pub fn verify_signed_bond_v1(
    s: &SignedBondRegistrationV1,
    network_domain: Hash64,
    max_fee_sompi: u64,
) -> Result<PalwBondKeyV2, BondRefusalV1> {
    if s.network_domain != network_domain {
        return Err(BondRefusalV1::WrongRuleset);
    }
    let tx = s.transaction()?;
    if tx_id_of_bytes(&tx) != s.tx_id {
        return Err(BondRefusalV1::Carrier("the transaction's id is not the file's".into()));
    }
    let payload: PalwLifecycleTxPayloadV2 =
        borsh::from_slice(&tx.payload).map_err(|e| BondRefusalV1::Carrier(format!("the payload does not decode: {e}")))?;
    let PalwConsensusObjectV2::BondRegistered {
        bond,
        pubkey,
        operator_pubkey,
        collateral,
        payout_payload,
        capable_classes,
        signature,
    } = &payload.object
    else {
        return Err(BondRefusalV1::Carrier("the payload is not a bond registration".into()));
    };
    if *bond != carrier_bond_key_v1() || hex(pubkey) != s.pubkey || operator_pubkey != pubkey || *collateral != s.collateral {
        return Err(BondRefusalV1::Carrier("the registration in the bytes is not the one the file names".into()));
    }
    if *payout_payload != s.payout_payload {
        return Err(BondRefusalV1::PayoutNotExpected(payout_payload.to_string()));
    }
    // Rebuild a bundle view to verify the owner signature over exactly these fields.
    let view = BondBundleV1 {
        schema: BOND_BUNDLE_SCHEMA_V1.into(),
        stage: BondStageV1::OwnerSigned,
        network: s.network.clone(),
        network_domain: s.network_domain,
        ruleset_id: s.ruleset_id.clone(),
        pubkey: s.pubkey.clone(),
        collateral: *collateral,
        payout_payload: *payout_payload,
        capable_classes: capable_classes.iter().copied().collect(),
        operator_possession: signature.len() == 2 * MLDSA87_SIG_LEN,
        signature: hex(signature),
        payer_address: String::new(),
        payer_spk: s.payer_spk.clone(),
        funding: s.funding.clone(),
        fee_sompi: s.fee_sompi,
        collateral_floor_sompi: 0,
        expiry_daa: s.expiry_daa,
        sources: Vec::new(),
        remote_state: UNVERIFIED_REMOTE_STATE.into(),
    };
    verify_owner_signature_v1(&view)?;
    palw_bond_registration_binds_its_carrier_v2(&tx, &payload.object).map_err(|e| BondRefusalV1::Carrier(e.into()))?;
    let payer_spk = parse_spk(&s.payer_spk)?;
    if tx.outputs.len() != 2 || tx.outputs[1].script_public_key != payer_spk {
        return Err(BondRefusalV1::ChangeNotPayer);
    }
    let paid_out: u64 = tx.outputs.iter().map(|o| o.value).sum();
    let fee = s.funding.amount.saturating_sub(paid_out);
    if fee > max_fee_sompi {
        return Err(BondRefusalV1::FeeAboveCap { fee, cap: max_fee_sompi });
    }
    let entry = UtxoEntry::new(s.funding.amount, payer_spk, s.funding.block_daa_score, s.funding.is_coinbase);
    carrier_funding_signature_valid(&tx, &entry).map_err(BondRefusalV1::Carrier)?;
    Ok(PalwBondKeyV2(TransactionOutpoint::new(s.tx_id, 0)))
}

/// The bundle's builder (no key): the caller prices the carrier (`fee_sompi`) the way its node relays.
#[allow(clippy::too_many_arguments)]
pub fn build_bond_bundle_v1(
    network: &str,
    network_domain: Hash64,
    ruleset_id: &str,
    pubkey: &[u8],
    collateral: u64,
    payout_payload: Hash64,
    operator_possession: bool,
    payer_address: &str,
    payer_spk: &ScriptPublicKey,
    funding: FundingInputV1,
    fee_sompi: u64,
    collateral_floor_sompi: u64,
    expiry_daa: u64,
    sources: Vec<String>,
) -> Result<BondBundleV1, BondRefusalV1> {
    if collateral < collateral_floor_sompi {
        return Err(BondRefusalV1::BelowFloor { got: collateral, floor: collateral_floor_sompi });
    }
    let b = BondBundleV1 {
        schema: BOND_BUNDLE_SCHEMA_V1.into(),
        stage: BondStageV1::Unsigned,
        network: network.into(),
        network_domain,
        ruleset_id: ruleset_id.into(),
        pubkey: hex(pubkey),
        collateral,
        payout_payload,
        capable_classes: Vec::new(),
        operator_possession,
        signature: String::new(),
        payer_address: payer_address.into(),
        payer_spk: spk_text_v1(payer_spk),
        funding,
        fee_sompi,
        collateral_floor_sompi,
        expiry_daa,
        sources,
        remote_state: UNVERIFIED_REMOTE_STATE.into(),
    };
    bond_carrier_body_v1(&b, &bond_object_of_v1(&b)?, vec![])?;
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::placeholder_funding_script_v1;

    struct Key(libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair);
    impl DetachedSigner for Key {
        fn public_key(&self) -> Vec<u8> {
            self.0.verification_key.as_ref().to_vec()
        }
        fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
            libcrux_ml_dsa::ml_dsa_87::sign(&self.0.signing_key, message, context, [7u8; 32])
                .map(|s| s.as_ref().to_vec())
                .map_err(|e| format!("{e:?}"))
        }
    }
    fn key(seed: u8) -> Key {
        Key(libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32]))
    }
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    const FLOOR: u64 = 1_000_000_000;

    fn fixture(owner: &Key, payer: &Key, possession: bool) -> (BondBundleV1, BondSignerPolicyV1) {
        let payout = own_payout_payload_v1(&owner.public_key());
        let b = build_bond_bundle_v1(
            "testnet-12",
            h(0xD0),
            "ruleset",
            &owner.public_key(),
            5 * FLOOR,
            payout,
            possession,
            "misakatest:payer",
            &funding_spk_of_pubkey_v1(&payer.public_key()),
            FundingInputV1 { txid: h(0xF0), index: 1, amount: 10 * FLOOR, block_daa_score: 3, is_coinbase: false },
            300_000,
            FLOOR,
            1_000,
            vec!["a".into(), "b".into()],
        )
        .unwrap();
        let policy = BondSignerPolicyV1 {
            network: "testnet-12".into(),
            network_domain: h(0xD0),
            ruleset_id: "ruleset".into(),
            collateral_floor_sompi: FLOOR,
            expect_collateral: 5 * FLOOR,
            expect_payout: payout,
            max_fee_sompi: 300_000,
            now_daa: Some(900),
        };
        (b, policy)
    }

    /// **The detached flow end to end**: export (no key) → the owner signs → a DIFFERENT payer key signs the carrier → anyone verifies the
    /// bytes → the node's own lifecycle extraction reads a BondRegistered keyed to the carrier (txid:0) whose collateral output pays the
    /// payout's script. With and without the operator-possession half.
    #[test]
    fn a_bond_is_registered_from_three_machines_and_the_chain_reads_it_from_the_bytes() {
        for possession in [false, true] {
            let (owner, payer) = (key(1), key(2));
            let (b, policy) = fixture(&owner, &payer, possession);
            let owner_signed = bond_owner_sign_v1(&b, &policy, &owner).unwrap();
            let signed = bond_carrier_sign_v1(&owner_signed, &policy, &payer).unwrap();
            let bond = verify_signed_bond_v1(&signed, h(0xD0), 300_000).unwrap();
            assert_eq!(bond.0, TransactionOutpoint::new(signed.tx_id, 0));
            let tx = signed.transaction().unwrap();
            let carried = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2(
                std::slice::from_ref(&tx),
            );
            assert!(carried.skipped.is_empty(), "{:?}", carried.skipped);
            let PalwConsensusObjectV2::BondRegistered { bond: keyed, collateral, .. } = &carried.objects[0].object else { panic!() };
            assert_eq!((keyed.0.transaction_id, keyed.0.index, *collateral), (tx.id(), 0, 5 * FLOOR), "keyed to its carrier");
            // the signed file round-trips, and a resend is the same bytes
            assert_eq!(SignedBondRegistrationV1::from_json(&signed.to_json()).unwrap(), signed);
            // the pricing placeholder has the real length (a fee priced on it is the fee of the signed carrier)
            let probe =
                bond_carrier_body_v1(&owner_signed, &bond_object_of_v1(&owner_signed).unwrap(), placeholder_funding_script_v1())
                    .unwrap();
            assert_eq!(probe.inputs[0].signature_script.len(), tx.inputs[0].signature_script.len());
        }
    }

    /// **A builder that tampers is refused before any key is touched**: the payout (where the collateral AND the rewards go), the collateral,
    /// the change recipient, the fee, the floor, the network, the expiry, the wrong owner key, the wrong payer key.
    #[test]
    fn a_builder_that_moves_the_payout_the_collateral_the_change_or_the_fee_is_refused_before_signing() {
        let (owner, payer) = (key(1), key(2));
        let (b, policy) = fixture(&owner, &payer, true);
        let refused = |x: &BondBundleV1| bond_owner_sign_v1(x, &policy, &owner).unwrap_err();
        assert!(matches!(refused(&BondBundleV1 { payout_payload: h(0xEE), ..b.clone() }), BondRefusalV1::PayoutNotExpected(_)));
        assert!(matches!(refused(&BondBundleV1 { collateral: 6 * FLOOR, ..b.clone() }), BondRefusalV1::CollateralNotExpected { .. }));
        assert!(matches!(refused(&BondBundleV1 { fee_sompi: 300_001, ..b.clone() }), BondRefusalV1::FeeAboveCap { .. }));
        assert!(matches!(refused(&BondBundleV1 { network_domain: h(0xD1), ..b.clone() }), BondRefusalV1::WrongRuleset));
        assert!(matches!(refused(&BondBundleV1 { expiry_daa: 10, ..b.clone() }), BondRefusalV1::Expired { .. }));
        let low = BondSignerPolicyV1 { expect_collateral: FLOOR / 2, ..policy.clone() };
        assert!(matches!(
            bond_owner_sign_v1(&BondBundleV1 { collateral: FLOOR / 2, ..b.clone() }, &low, &owner),
            Err(BondRefusalV1::BelowFloor { .. })
        ));
        assert!(matches!(bond_owner_sign_v1(&b, &policy, &key(9)), Err(BondRefusalV1::OwnerKeyMismatch)));
        // the change recipient swapped: the payer's own key does not hold the script the change goes to
        let owner_signed = bond_owner_sign_v1(&b, &policy, &owner).unwrap();
        assert!(matches!(bond_carrier_sign_v1(&owner_signed, &policy, &key(9)), Err(BondRefusalV1::PayerIsNotThisKey)));
        let swapped = BondBundleV1 { payer_spk: spk_text_v1(&funding_spk_of_pubkey_v1(&key(9).public_key())), ..owner_signed.clone() };
        assert!(matches!(bond_carrier_sign_v1(&swapped, &policy, &payer), Err(BondRefusalV1::PayerIsNotThisKey)));
        // a builder that changes the collateral AFTER the owner signed: the signature no longer covers it
        let after = BondBundleV1 { collateral: 6 * FLOOR, ..owner_signed.clone() };
        let policy6 = BondSignerPolicyV1 { expect_collateral: 6 * FLOOR, ..policy.clone() };
        assert!(matches!(bond_carrier_sign_v1(&after, &policy6, &payer), Err(BondRefusalV1::OwnerSignature)));
    }

    /// **A relay that alters, steals or re-attributes** is caught from the bytes (and by every node): the payout swapped to the relay's own
    /// (the owner signature binds it), the change redirected, a byte of the funding signature flipped.
    #[test]
    fn a_relay_cannot_redirect_the_collateral_or_the_change_or_alter_a_byte() {
        let (owner, payer) = (key(1), key(2));
        let (b, policy) = fixture(&owner, &payer, true);
        let signed = bond_carrier_sign_v1(&bond_owner_sign_v1(&b, &policy, &owner).unwrap(), &policy, &payer).unwrap();
        let rewrite = |f: &dyn Fn(&mut Transaction)| {
            let mut tx = signed.transaction().unwrap();
            f(&mut tx);
            tx.finalize();
            SignedBondRegistrationV1 { tx_hex: hex(&borsh::to_vec(&tx).unwrap()), tx_id: tx_id_of_bytes(&tx), ..signed.clone() }
        };
        // steal: the collateral output and the declared payout moved to the relay's own payload — the owner signature does not cover it
        let thief = own_payout_payload_v1(&key(9).public_key());
        let stolen = rewrite(&|tx| {
            let mut p: PalwLifecycleTxPayloadV2 = borsh::from_slice(&tx.payload).unwrap();
            if let PalwConsensusObjectV2::BondRegistered { payout_payload, .. } = &mut p.object {
                *payout_payload = thief;
            }
            tx.payload = borsh::to_vec(&p).unwrap();
            tx.outputs[0].script_public_key = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(thief.as_byte_slice());
        });
        let stolen = SignedBondRegistrationV1 { payout_payload: thief, ..stolen };
        assert!(matches!(verify_signed_bond_v1(&stolen, h(0xD0), 300_000), Err(BondRefusalV1::OwnerSignature)));
        // the change redirected
        let redirected = rewrite(&|tx| tx.outputs[1].script_public_key = funding_spk_of_pubkey_v1(&key(9).public_key()));
        assert!(verify_signed_bond_v1(&redirected, h(0xD0), 300_000).is_err());
        // one byte of the funding signature
        let flipped = rewrite(&|tx| tx.inputs[0].signature_script[5] ^= 1);
        assert!(matches!(verify_signed_bond_v1(&flipped, h(0xD0), 300_000), Err(BondRefusalV1::Carrier(_))));
        // the honest file still verifies (and the bond it makes is the carrier's output 0)
        assert!(verify_signed_bond_v1(&signed, h(0xD0), 300_000).is_ok());
    }
}
