//! **RFC-0009 §3.3–§3.4 — detached signing for node-less model registration: export → sign → submit.**
//!
//! The in-process path of `model add --relay` builds the object, asks the user and signs in ONE process that also talks to the
//! network. This module splits that into three steps that can run on three machines, with the key present only at the second:
//!
//! 1. **export** ([`build_bundle_v1`]) — an online *builder* (a laptop, a hosted service, a friend's node) reads the chain through
//!    several RPCs, builds the UNSIGNED `ClassRegistered` object and the unsigned carrier plan, and writes a
//!    [`RegistrationBundleV1`]. It holds no key and learns none: it is given the owner's PUBLIC key and the payer's ADDRESS.
//! 2. **sign** ([`owner_sign_v1`], [`carrier_sign_v1`]) — an offline *signer* re-derives everything it signs from the bundle's own
//!    object bytes and from its OWN network parameters, refuses on any difference ([`BundleRefusalV1`]), and only then signs.
//!    The bond key signs the registration message; the payer key signs the carrier's funding input. They may be one key or two
//!    (the fee payer need not be the bond key), and the payer signs only after the owner's signature is in the object (the carrier's
//!    sighash covers the payload).
//! 3. **submit** ([`verify_signed_registration_v1`] + the CLI's relay fan-out) — anyone forwards the signed bytes. A relay cannot
//!    alter them: the owner's signature covers the object's fields and the funding signature (SIG_HASH_ALL) covers every output,
//!    the fee and the payload, so a flipped byte is a refusal here and a rejection on the chain.
//!
//! # What the signer trusts — and what the consensus wire cannot carry
//!
//! The builder is NOT trusted. The signer takes from the bundle only bytes it then checks against what the user EXPECTS
//! ([`ExpectationsV1`]: the class, the root and the owner bond, supplied by the user, never read back from the bundle) and
//! against its own build's network parameters (network domain, ruleset id, registration exposure, burn).
//!
//! What the chain checks inside the owner's signature is `palw_class_registration_message_v2` — network domain (network name +
//! genesis), class id, share, activation DAA, owner bond, artifact root, slash value, initial target, pwu rule and canonical job.
//! **It does not carry an expiry or a ruleset id**, and a carrier transaction cannot carry an expiry either: `lock_time` is a
//! not-before bound and the lifecycle carrier's inputs use the final sequence number, so it is not even evaluated. So:
//!
//! * the quote's expiry and the ruleset id are bound by a signed-nothing client check: the bundle (and the signed-registration file)
//!   names them, the signer recomputes the ruleset id from its own build, and `submit` refuses past the expiry. They are NOT enforced
//!   by consensus — a leaked signed carrier stays valid until its funding input is spent elsewhere. This is recorded as
//!   `CODE_GAP` G-EXPIRY / G-RULESET in `docs/design/palw/rfc-0009-remote-record.md` for the Lead; the registration wire is unchanged.
//! * every unproven fact the builder reports (the bond's backing, the wallet's UTXO, the registry rows) is labelled
//!   [`UNVERIFIED_REMOTE_STATE`]: several RPCs agreeing is an operational defence, not a proof.

use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::constants::{MAX_TX_IN_SEQUENCE_NUM, TX_VERSION};
use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
use kaspa_consensus_core::palw_model_registration_v1::palw_registration_object_id_v1;
use kaspa_consensus_core::palw_state_v2::{
    PALW_CLASS_REGISTRATION_BURN_SOMPI_V1, PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT, PalwBondKeyV2, PalwClassAdmissionCarriageV2,
    PalwConsensusObjectV2, PalwPwuRuleV2, palw_class_registration_message_v2,
};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
use kaspa_consensus_core::tx::{
    MutableTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_hashes::Hash64;
use kaspa_txscript::script_builder::ScriptBuilder;
use kaspa_txscript::{MLDSA87_PK_LEN, MLDSA87_SIG_LEN, MLDSA87_TX_CONTEXT, verify_mldsa87_with_context};
use serde::{Deserialize, Serialize};

use crate::proof::{StateProofV1, verify_bond_against_pin};
use crate::register::{DuplicateVerdictV1, RegistrationFactsV1, RegistrationQuoteV1};
use crate::relay::{carrier_funding_signature_valid, parse_two_pushes, tx_id_of_bytes};
use crate::trust::Provenance;

pub const BUNDLE_SCHEMA_V1: &str = "misaka.palw.registration-bundle.v1";
pub const SIGNED_SCHEMA_V1: &str = "misaka.palw.registration-signed.v1";

pub use crate::trust::UNVERIFIED_REMOTE_STATE;

/// A key that signs on the user's machine. The library never sees a seed: the CLI wraps its `ValidatorKey` (or, later, a sidecar
/// that holds the key in another process) behind this.
pub trait DetachedSigner {
    fn public_key(&self) -> Vec<u8>;
    /// ML-DSA-87 over `message` under `context`.
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String>;
}

/// The funding address's script for a key: the same derivation as `ValidatorKey::funding_address` (prefix does not enter the script).
pub fn funding_spk_of_pubkey_v1(pubkey: &[u8]) -> ScriptPublicKey {
    let payload = kaspa_hashes::blake2b_512_address_payload(pubkey).as_bytes();
    kaspa_txscript::pay_to_address_script(&Address::new(Prefix::Mainnet, Version::PubKeyHashMlDsa87, &payload))
}

/// A signature script of the real shape and length (`<sig‖hashtype> <pubkey>`), zero-filled: prices the carrier without a key.
pub fn placeholder_funding_script_v1() -> Vec<u8> {
    ScriptBuilder::new()
        .add_data(&vec![0u8; MLDSA87_SIG_LEN + 1])
        .and_then(|b| b.add_data(&vec![0u8; MLDSA87_PK_LEN]))
        .map(|b| b.drain())
        .expect("two data pushes of fixed length fit a script")
}

fn bond_text(b: &PalwBondKeyV2) -> String {
    format!("{}:{}", b.0.transaction_id, b.0.index)
}

fn hex(bytes: &[u8]) -> String {
    faster_hex::hex_string(bytes)
}

fn unhex(s: &str) -> Result<Vec<u8>, BundleRefusalV1> {
    let mut out = vec![0u8; s.len() / 2];
    if s.len() % 2 != 0 || faster_hex::hex_decode(s.as_bytes(), &mut out).is_err() {
        return Err(BundleRefusalV1::Malformed("a hex field is not hex".into()));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------------------
// The bundle
// ---------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BundleStageV1 {
    /// The object carries an empty owner signature.
    Unsigned,
    /// The bond key has signed the object; the carrier's funding input is still unsigned.
    OwnerSigned,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundingInputV1 {
    pub txid: Hash64,
    pub index: u32,
    pub amount: u64,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayerV1 {
    /// For display. Never a basis of any check: the script below is.
    pub address: String,
    /// The funding input's script (`ScriptPublicKey` text form). It must be the signer's own funding address script.
    pub spk: String,
    pub funding: FundingInputV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarrierPlanV1 {
    pub fee_sompi: u64,
    pub mass: u64,
    pub change_sompi: u64,
    /// Where the change goes. It must equal the payer's own script: a different recipient is a refusal.
    pub change_spk: String,
    /// The signed object's borsh length the fee was priced for (a signature of the real length is in it).
    pub signed_payload_len: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteSourceV1 {
    pub node: String,
    pub tip_hash: Hash64,
    pub tip_daa: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrationBundleV1 {
    pub schema: String,
    pub stage: BundleStageV1,
    pub network: String,
    pub network_domain: Hash64,
    pub ruleset_id: String,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    /// `txid:index` of the owner (registrant) bond.
    pub owner_bond: String,
    /// The owner bond's registered key, hex. As reported by the quoting nodes ([`UNVERIFIED_REMOTE_STATE`]); the signer compares it
    /// with its own key and refuses on a difference.
    pub owner_pubkey: String,
    /// A proof that the owner bond and its key are in the state a block commits, for a signer that pinned that block (`--pin`). Without it
    /// the key is a node's report and the signer shows it as such.
    #[serde(default)]
    pub owner_proof: Option<StateProofV1>,
    /// `borsh(PalwConsensusObjectV2::ClassRegistered)` as of this stage.
    pub object_hex: String,
    /// `palw_registration_object_id_v1` of the UNSIGNED object: what the quote names.
    pub object_digest: Hash64,
    /// What the bond key signs. Recomputed by the signer; a bundle whose value differs is refused.
    pub owner_message: Hash64,
    pub owner_context: String,
    pub payer: PayerV1,
    pub carrier: CarrierPlanV1,
    pub quote: RegistrationQuoteV1,
    pub sources: Vec<QuoteSourceV1>,
    /// Always [`UNVERIFIED_REMOTE_STATE`] for what the builder read.
    pub remote_state: String,
}

impl RegistrationBundleV1 {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a bundle is serializable")
    }

    pub fn from_json(text: &str) -> Result<Self, BundleRefusalV1> {
        let b: Self =
            serde_json::from_str(text).map_err(|e| BundleRefusalV1::Malformed(format!("the bundle is not valid JSON: {e}")))?;
        if b.schema != BUNDLE_SCHEMA_V1 {
            return Err(BundleRefusalV1::Schema(b.schema));
        }
        Ok(b)
    }
}

/// **Why nothing was signed** (or why a signed file is not submitted). Each is a distinct, named condition: the tests pin the four a
/// tampering builder or relay would try (recipient, fee, class root, owner).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BundleRefusalV1 {
    #[error("malformed: {0}")]
    Malformed(String),
    #[error("unknown schema {0:?}")]
    Schema(String),
    #[error("the bundle is for network {bundle:?}, this signer is on {ours:?}")]
    WrongNetwork { bundle: String, ours: String },
    #[error("the network domain differs from the one this signer derives from its own parameters (another network or genesis)")]
    NetworkDomain,
    #[error(
        "the ruleset id differs from the one this build derives for the network (another ruleset: re-export with a matching node)"
    )]
    Ruleset,
    #[error("the object is not a post-genesis ClassRegistered with its admission carriage")]
    NotARegistration,
    #[error("the stage is {have:?}, this step needs {need:?}")]
    Stage { have: BundleStageV1, need: BundleStageV1 },
    #[error("the object's profile does not hash to its class id")]
    ClassIsNotItsProfile,
    #[error("the bundle names class {bundle} but you expected {expected}: the class was swapped")]
    ClassSwapped { bundle: Hash64, expected: Hash64 },
    #[error("the bundle registers artifact root {bundle} but you expected {expected}: the class root was swapped")]
    RootSwapped { bundle: Hash64, expected: Hash64 },
    #[error("the bundle registers under owner bond {bundle} but you expected {expected}: the owner was swapped")]
    OwnerSwapped { bundle: String, expected: String },
    #[error(
        "this key is not the owner bond's registered key: a registration signed by it would be dropped as \"not signed by the bond\""
    )]
    OwnerKeyMismatch,
    #[error("the bundle's field {0} disagrees with the object it carries")]
    FieldMismatch(&'static str),
    #[error("the object digest the quote names is not the digest of the object in the bundle")]
    ObjectDigest,
    #[error("the message the bundle says the owner signs is not the one this signer derives from the object")]
    OwnerMessage,
    #[error("the signing context is not the registration context")]
    OwnerContext,
    #[error("the quote disagrees with the bundle: {0}")]
    QuoteInconsistent(&'static str),
    #[error("the quote understates a cost this build knows: {what} (quote {quoted}, this build {known})")]
    CostUnderstated { what: &'static str, quoted: u64, known: u64 },
    #[error("the fee is {fee} sompi and the most you allowed the wallet to pay is {cap}")]
    FeeAboveCap { fee: u64, cap: u64 },
    #[error("the carrier's fee ({fee}) is not the quoted fee ({quoted})")]
    FeeNotQuoted { fee: u64, quoted: u64 },
    #[error("the funding input and the change do not add up")]
    CarrierArithmetic,
    #[error("the change output goes to a script that is not the payer's own: a different recipient")]
    ChangeNotPayer,
    #[error("the funding input is not locked to this payer key's address")]
    PayerIsNotThisKey,
    #[error("the owner's signature is missing or does not verify over the registration message")]
    OwnerSignature,
    #[error("the quote lapsed at DAA {expiry} (the chain is at {now}): re-export")]
    Expired { expiry: u64, now: u64 },
    #[error("the signed transaction is not the carrier of this registration: {0}")]
    NotTheCarrier(&'static str),
    #[error("the funding signature does not verify: {0}")]
    FundingSignature(String),
    #[error("the signer failed: {0}")]
    Signer(String),
    #[error("you pinned a block but the bundle carries no proof of the owner bond (re-export with the same --pin)")]
    PinWithoutProof,
    #[error("the bundle's proof is for block {bundle}, not the block you pinned ({pinned})")]
    PinMismatch { bundle: Hash64, pinned: Hash64 },
    #[error("the owner bond's proof does not hold against your pin: {0}")]
    OwnerProof(String),
}

/// What the USER expects, entered or confirmed by the user (never read back from a bundle).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectationsV1 {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    /// `txid:index`.
    pub owner_bond: String,
}

/// The signer's own parameters and limits.
#[derive(Clone, Debug)]
pub struct SignerPolicyV1 {
    pub network: String,
    /// Derived from the signer's own network parameters (name + genesis), never read from the bundle.
    pub network_domain: Hash64,
    pub ruleset_id: String,
    /// The registration exposure of the signer's own build (`bundle.state.registration_exposure_sompi()`).
    pub registration_exposure_sompi: u64,
    pub expect: ExpectationsV1,
    /// The most the wallet may pay (carrier fee and wallet filings).
    pub max_wallet_sompi: u64,
    /// The chain's DAA when known (`--rpc` at sign time, or the relay's tip at submit time); `None` offline: the expiry is then not
    /// checked here and the output says so.
    pub now_daa: Option<u64>,
    /// The block the signer pinned (`--pin`). With it the bundle MUST carry a proof, and the owner bond's key is checked against the state
    /// that block commits; without it the key stays a node's report.
    pub pin: Option<Hash64>,
}

/// A script's text form: hex of the version (big endian) followed by the script, as `ScriptPublicKey::from_str` reads it.
pub fn spk_text_v1(spk: &ScriptPublicKey) -> String {
    let mut bytes = spk.version().to_be_bytes().to_vec();
    bytes.extend_from_slice(spk.script());
    hex(&bytes)
}

fn parse_spk(s: &str) -> Result<ScriptPublicKey, BundleRefusalV1> {
    s.parse::<ScriptPublicKey>().map_err(|_| BundleRefusalV1::Malformed("a script field is not a script".into()))
}

struct Parts<'a> {
    class_id: Hash64,
    artifact_root: Hash64,
    activation_daa: u64,
    share_permille: u16,
    slash_value_per_pwu: u64,
    initial_target: u128,
    pwu_rule: &'a PalwPwuRuleV2,
    carriage: &'a PalwClassAdmissionCarriageV2,
}

fn parts_of(obj: &PalwConsensusObjectV2) -> Result<Parts<'_>, BundleRefusalV1> {
    match obj {
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root,
            slash_value_per_pwu,
            pwu_rule,
            initial_target,
            share_permille,
            activation_daa,
            admission: Some(carriage),
        } => Ok(Parts {
            class_id: *class_id,
            artifact_root: *artifact_root,
            activation_daa: *activation_daa,
            share_permille: *share_permille,
            slash_value_per_pwu: *slash_value_per_pwu,
            initial_target: *initial_target,
            pwu_rule,
            carriage,
        }),
        _ => Err(BundleRefusalV1::NotARegistration),
    }
}

fn owner_message_of(domain: Hash64, p: &Parts<'_>) -> Hash64 {
    palw_class_registration_message_v2(
        domain,
        p.class_id,
        p.share_permille,
        p.activation_daa,
        &p.carriage.registrant_bond,
        p.artifact_root,
        p.slash_value_per_pwu,
        p.initial_target,
        p.pwu_rule,
        &p.carriage.canonical,
    )
}

/// The object with its owner signature emptied: the thing a quote names.
pub fn unsigned_object_of(obj: &PalwConsensusObjectV2) -> Result<PalwConsensusObjectV2, BundleRefusalV1> {
    parts_of(obj)?;
    let mut o = obj.clone();
    if let PalwConsensusObjectV2::ClassRegistered { admission: Some(c), .. } = &mut o {
        c.signature = Vec::new();
    }
    Ok(o)
}

pub fn object_bytes(obj: &PalwConsensusObjectV2) -> Vec<u8> {
    borsh::to_vec(obj).expect("a consensus object serializes")
}

/// `palw_registration_object_id_v1` of the UNSIGNED form (the quote's `object_digest`).
pub fn unsigned_object_digest_v1(obj: &PalwConsensusObjectV2) -> Result<Hash64, BundleRefusalV1> {
    Ok(palw_registration_object_id_v1(&object_bytes(&unsigned_object_of(obj)?)))
}

fn decode_object(hex_text: &str) -> Result<PalwConsensusObjectV2, BundleRefusalV1> {
    let bytes = unhex(hex_text)?;
    borsh::from_slice::<PalwConsensusObjectV2>(&bytes)
        .map_err(|e| BundleRefusalV1::Malformed(format!("the object does not decode: {e}")))
}

/// The carrier transaction body of `object` funded by one input and paying the change to `change_spk`. The same shape as
/// `ValidatorKey::build_palw_lifecycle_tx` (one input, final sequence, one change output, subnetwork 0x4b, lock time 0).
pub fn carrier_body_v1(
    object: &PalwConsensusObjectV2,
    outpoint: TransactionOutpoint,
    funding_amount: u64,
    fee: u64,
    change_spk: &ScriptPublicKey,
    signature_script: Vec<u8>,
) -> Result<Transaction, BundleRefusalV1> {
    if funding_amount <= fee {
        return Err(BundleRefusalV1::CarrierArithmetic);
    }
    let payload = PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() };
    let bytes = borsh::to_vec(&payload).map_err(|e| BundleRefusalV1::Malformed(format!("the payload does not serialize: {e}")))?;
    let input = TransactionInput::new(outpoint, signature_script, MAX_TX_IN_SEQUENCE_NUM, 1);
    let outputs = vec![TransactionOutput::new(funding_amount - fee, change_spk.clone())];
    Ok(Transaction::new(TX_VERSION, vec![input], outputs, 0, SUBNETWORK_ID_PALW_LIFECYCLE, 0, bytes))
}

// ---------------------------------------------------------------------------------------------------------------------------
// export (the builder holds no key)
// ---------------------------------------------------------------------------------------------------------------------------

/// What a key-less builder knows.
#[derive(Clone, Debug)]
pub struct BundleInputsV1 {
    pub network: String,
    pub network_domain: Hash64,
    pub ruleset_id: String,
    pub unsigned_object: PalwConsensusObjectV2,
    pub owner_pubkey: Vec<u8>,
    pub owner_proof: Option<StateProofV1>,
    pub payer_address: String,
    pub payer_spk: ScriptPublicKey,
    pub funding_outpoint: TransactionOutpoint,
    pub funding_entry: UtxoEntry,
    pub quote: RegistrationQuoteV1,
    pub sources: Vec<QuoteSourceV1>,
}

/// **Assemble the unsigned bundle.** Refuses to write a bundle that is not self-consistent (a builder bug is caught here rather than
/// at the signer).
pub fn build_bundle_v1(i: BundleInputsV1) -> Result<RegistrationBundleV1, BundleRefusalV1> {
    let unsigned = unsigned_object_of(&i.unsigned_object)?;
    let p = parts_of(&unsigned)?;
    let message = owner_message_of(i.network_domain, &p);
    let fee = i.quote.facts.carrier_fee_sompi;
    let change = i.funding_entry.amount.checked_sub(fee).ok_or(BundleRefusalV1::CarrierArithmetic)?;
    let signed_len = {
        let mut probe = unsigned.clone();
        if let PalwConsensusObjectV2::ClassRegistered { admission: Some(c), .. } = &mut probe {
            c.signature = vec![0u8; MLDSA87_SIG_LEN];
        }
        object_bytes(&probe).len() as u64
    };
    let bundle = RegistrationBundleV1 {
        schema: BUNDLE_SCHEMA_V1.into(),
        stage: BundleStageV1::Unsigned,
        network: i.network,
        network_domain: i.network_domain,
        ruleset_id: i.ruleset_id,
        class_id: p.class_id,
        artifact_root: p.artifact_root,
        owner_bond: bond_text(&p.carriage.registrant_bond),
        owner_pubkey: hex(&i.owner_pubkey),
        owner_proof: i.owner_proof,
        object_hex: hex(&object_bytes(&unsigned)),
        object_digest: palw_registration_object_id_v1(&object_bytes(&unsigned)),
        owner_message: message,
        owner_context: String::from_utf8_lossy(PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT).into_owned(),
        payer: PayerV1 {
            address: i.payer_address,
            spk: spk_text_v1(&i.payer_spk),
            funding: FundingInputV1 {
                txid: i.funding_outpoint.transaction_id,
                index: i.funding_outpoint.index,
                amount: i.funding_entry.amount,
                block_daa_score: i.funding_entry.block_daa_score,
                is_coinbase: i.funding_entry.is_coinbase,
            },
        },
        carrier: CarrierPlanV1 {
            fee_sompi: fee,
            mass: i.quote.facts.carrier_mass,
            change_sompi: change,
            change_spk: spk_text_v1(&i.payer_spk),
            signed_payload_len: signed_len,
        },
        quote: i.quote,
        sources: i.sources,
        remote_state: UNVERIFIED_REMOTE_STATE.into(),
    };
    Ok(bundle)
}

// ---------------------------------------------------------------------------------------------------------------------------
// the signer's re-check
// ---------------------------------------------------------------------------------------------------------------------------

/// A bundle that passed every check the signer can make from its own parameters and the user's expectations.
#[derive(Clone, Debug)]
pub struct CheckedBundleV1 {
    pub stage: BundleStageV1,
    /// The object as the bundle carries it (signed or not, per stage).
    pub object: PalwConsensusObjectV2,
    pub unsigned_object: PalwConsensusObjectV2,
    pub owner_pubkey: Vec<u8>,
    /// The message the signer derived itself.
    pub owner_message: Hash64,
    pub payer_spk: ScriptPublicKey,
    pub funding_outpoint: TransactionOutpoint,
    pub funding_entry: UtxoEntry,
    pub fee_sompi: u64,
    pub quote_digest: Hash64,
    pub expiry_daa: u64,
    /// Where the owner bond's key comes from: proven against the signer's pin, or a node's report.
    pub owner_key_provenance: Provenance,
}

impl CheckedBundleV1 {
    /// What the signer shows the user — every figure from a field it verified, not from the bundle's display text.
    pub fn review_lines(&self, b: &RegistrationBundleV1) -> Vec<String> {
        let mut out = vec![
            format!("network    {} (domain recomputed here)", b.network),
            format!("owner      bond {} — signs the registration, pays the burn, holds the exposure", b.owner_bond),
            format!("owner key  {}…  [{}]", &b.owner_pubkey[..32.min(b.owner_pubkey.len())], self.owner_key_provenance.label()),
        ];
        out.extend(b.quote.lines());
        out.push(format!("quote      {}", self.quote_digest));
        out.push(format!(
            "NOTE       the quote's chain facts (bond backing, wallet balance, registry rows) are {UNVERIFIED_REMOTE_STATE}: nodes agreed, nothing was proven"
        ));
        out
    }
}

fn wallet_filings(f: &RegistrationFactsV1) -> u64 {
    f.filings.iter().filter(|x| x.from_wallet).map(|x| x.sompi).sum()
}

fn bond_filings(f: &RegistrationFactsV1) -> u64 {
    f.filings.iter().filter(|x| !x.from_wallet).map(|x| x.sompi).sum()
}

/// **Everything the signer can verify without a key and without the network.**
pub fn check_bundle_v1(
    b: &RegistrationBundleV1,
    policy: &SignerPolicyV1,
    need: BundleStageV1,
) -> Result<CheckedBundleV1, BundleRefusalV1> {
    if b.schema != BUNDLE_SCHEMA_V1 {
        return Err(BundleRefusalV1::Schema(b.schema.clone()));
    }
    if b.stage != need {
        return Err(BundleRefusalV1::Stage { have: b.stage, need });
    }
    if b.network != policy.network {
        return Err(BundleRefusalV1::WrongNetwork { bundle: b.network.clone(), ours: policy.network.clone() });
    }
    if b.network_domain != policy.network_domain {
        return Err(BundleRefusalV1::NetworkDomain);
    }
    if b.ruleset_id != policy.ruleset_id {
        return Err(BundleRefusalV1::Ruleset);
    }
    let object = decode_object(&b.object_hex)?;
    let p = parts_of(&object)?;
    let have = if p.carriage.signature.is_empty() { BundleStageV1::Unsigned } else { BundleStageV1::OwnerSigned };
    if have != b.stage {
        // The label says one thing and the object another: the object is what is signed.
        return Err(BundleRefusalV1::Stage { have, need: b.stage });
    }
    if p.carriage.profile.shape_profile_id() != p.class_id {
        return Err(BundleRefusalV1::ClassIsNotItsProfile);
    }
    if p.class_id != b.class_id {
        return Err(BundleRefusalV1::FieldMismatch("class_id"));
    }
    if p.artifact_root != b.artifact_root {
        return Err(BundleRefusalV1::FieldMismatch("artifact_root"));
    }
    let owner_bond = bond_text(&p.carriage.registrant_bond);
    if owner_bond != b.owner_bond {
        return Err(BundleRefusalV1::FieldMismatch("owner_bond"));
    }
    // What the user EXPECTS, against what the OBJECT says (and so, the bundle).
    if p.class_id != policy.expect.class_id {
        return Err(BundleRefusalV1::ClassSwapped { bundle: p.class_id, expected: policy.expect.class_id });
    }
    if p.artifact_root != policy.expect.artifact_root {
        return Err(BundleRefusalV1::RootSwapped { bundle: p.artifact_root, expected: policy.expect.artifact_root });
    }
    if owner_bond != policy.expect.owner_bond {
        return Err(BundleRefusalV1::OwnerSwapped { bundle: owner_bond, expected: policy.expect.owner_bond.clone() });
    }
    let unsigned = unsigned_object_of(&object)?;
    let digest = palw_registration_object_id_v1(&object_bytes(&unsigned));
    if digest != b.object_digest {
        return Err(BundleRefusalV1::ObjectDigest);
    }
    let message = owner_message_of(policy.network_domain, &p);
    if message != b.owner_message {
        return Err(BundleRefusalV1::OwnerMessage);
    }
    if b.owner_context.as_bytes() != PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT {
        return Err(BundleRefusalV1::OwnerContext);
    }
    let owner_pubkey = unhex(&b.owner_pubkey)?;
    if owner_pubkey.len() != MLDSA87_PK_LEN {
        return Err(BundleRefusalV1::Malformed("the owner key is not an ML-DSA-87 key".into()));
    }
    // The owner bond's key: proven against the block the SIGNER pinned, or a node's report.
    let owner_key_provenance = match policy.pin {
        None => Provenance::UnverifiedRemoteState { agreeing: b.sources.len() },
        Some(pin) => {
            let proof = b.owner_proof.as_ref().ok_or(BundleRefusalV1::PinWithoutProof)?;
            if proof.block != pin {
                return Err(BundleRefusalV1::PinMismatch { bundle: proof.block, pinned: pin });
            }
            let (header, fact) = proof.open().map_err(BundleRefusalV1::OwnerProof)?;
            verify_bond_against_pin(&header, pin, &fact, &p.carriage.registrant_bond, &owner_pubkey)
                .map_err(|e| BundleRefusalV1::OwnerProof(e.to_string()))?;
            Provenance::ProvenAtPin { pinned_block: pin, header_daa: header.daa_score }
        }
    };

    // The quote against the bundle.
    let q = &b.quote;
    let f = &q.facts;
    if f.network_domain != b.network_domain {
        return Err(BundleRefusalV1::QuoteInconsistent("network domain"));
    }
    if f.class_id != p.class_id || f.artifact_root != p.artifact_root {
        return Err(BundleRefusalV1::QuoteInconsistent("class or root"));
    }
    if f.object_digest != b.object_digest {
        return Err(BundleRefusalV1::QuoteInconsistent("object digest"));
    }
    if parse_bond(&f.bond.outpoint).map(|o| format!("{}:{}", o.transaction_id, o.index)) != Some(owner_bond.clone()) {
        return Err(BundleRefusalV1::QuoteInconsistent("owner bond"));
    }
    if f.carrier_fee_sompi != b.carrier.fee_sompi || f.carrier_mass != b.carrier.mass {
        return Err(BundleRefusalV1::QuoteInconsistent("carrier fee or mass"));
    }
    if q.wallet_total_sompi != f.carrier_fee_sompi.saturating_add(wallet_filings(f))
        || q.bond_debit_sompi != f.burn_sompi.saturating_add(bond_filings(f))
    {
        return Err(BundleRefusalV1::QuoteInconsistent("totals"));
    }
    // Costs this build knows for itself must not be understated.
    if f.exposure_sompi != policy.registration_exposure_sompi {
        return Err(BundleRefusalV1::CostUnderstated {
            what: "registration exposure",
            quoted: f.exposure_sompi,
            known: policy.registration_exposure_sompi,
        });
    }
    if f.burn_sompi != 0 && f.burn_sompi != PALW_CLASS_REGISTRATION_BURN_SOMPI_V1 {
        return Err(BundleRefusalV1::CostUnderstated {
            what: "registration burn",
            quoted: f.burn_sompi,
            known: PALW_CLASS_REGISTRATION_BURN_SOMPI_V1,
        });
    }

    // The carrier.
    let payer_spk = parse_spk(&b.payer.spk)?;
    let change_spk = parse_spk(&b.carrier.change_spk)?;
    if change_spk != payer_spk {
        return Err(BundleRefusalV1::ChangeNotPayer);
    }
    let fund = &b.payer.funding;
    if b.carrier.fee_sompi.checked_add(b.carrier.change_sompi) != Some(fund.amount) {
        return Err(BundleRefusalV1::CarrierArithmetic);
    }
    if b.carrier.fee_sompi != f.carrier_fee_sompi {
        return Err(BundleRefusalV1::FeeNotQuoted { fee: b.carrier.fee_sompi, quoted: f.carrier_fee_sompi });
    }
    if q.wallet_total_sompi > policy.max_wallet_sompi {
        return Err(BundleRefusalV1::FeeAboveCap { fee: q.wallet_total_sompi, cap: policy.max_wallet_sompi });
    }
    let expected_len = {
        let mut probe = unsigned.clone();
        if let PalwConsensusObjectV2::ClassRegistered { admission: Some(c), .. } = &mut probe {
            c.signature = vec![0u8; MLDSA87_SIG_LEN];
        }
        object_bytes(&probe).len() as u64
    };
    if b.carrier.signed_payload_len != expected_len {
        return Err(BundleRefusalV1::QuoteInconsistent("priced payload length"));
    }
    if let Some(now) = policy.now_daa
        && now > q.expiry_daa
    {
        return Err(BundleRefusalV1::Expired { expiry: q.expiry_daa, now });
    }
    let funding_entry = UtxoEntry::new(fund.amount, payer_spk.clone(), fund.block_daa_score, fund.is_coinbase);
    Ok(CheckedBundleV1 {
        stage: b.stage,
        object,
        unsigned_object: unsigned,
        owner_pubkey,
        owner_message: message,
        payer_spk,
        funding_outpoint: TransactionOutpoint::new(fund.txid, fund.index),
        funding_entry,
        fee_sompi: b.carrier.fee_sompi,
        quote_digest: q.digest(),
        expiry_daa: q.expiry_daa,
        owner_key_provenance,
    })
}

fn parse_bond(s: &str) -> Option<TransactionOutpoint> {
    let (txid, index) = s.rsplit_once(':')?;
    Some(TransactionOutpoint::new(txid.parse().ok()?, index.parse().ok()?))
}

/// **Sign step 1: the bond key signs the registration message.** The message is the one THIS function derives from the object and
/// the signer's own network domain, never the bundle's `owner_message`.
pub fn owner_sign_v1(
    b: &RegistrationBundleV1,
    policy: &SignerPolicyV1,
    signer: &dyn DetachedSigner,
) -> Result<RegistrationBundleV1, BundleRefusalV1> {
    let checked = check_bundle_v1(b, policy, BundleStageV1::Unsigned)?;
    if signer.public_key() != checked.owner_pubkey {
        return Err(BundleRefusalV1::OwnerKeyMismatch);
    }
    let sig = signer
        .sign_with_context(checked.owner_message.as_byte_slice(), PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT)
        .map_err(BundleRefusalV1::Signer)?;
    if sig.len() != MLDSA87_SIG_LEN {
        return Err(BundleRefusalV1::Signer(format!("a signature of {} bytes, not {MLDSA87_SIG_LEN}", sig.len())));
    }
    if !matches!(
        verify_mldsa87_with_context(
            &checked.owner_pubkey,
            checked.owner_message.as_byte_slice(),
            &sig,
            PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT
        ),
        Ok(true)
    ) {
        return Err(BundleRefusalV1::OwnerSignature);
    }
    let mut object = checked.object;
    if let PalwConsensusObjectV2::ClassRegistered { admission: Some(c), .. } = &mut object {
        c.signature = sig;
    }
    let mut out = b.clone();
    out.stage = BundleStageV1::OwnerSigned;
    out.object_hex = hex(&object_bytes(&object));
    Ok(out)
}

/// A signed carrier, ready for any relay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedRegistrationV1 {
    pub schema: String,
    pub network: String,
    pub network_domain: Hash64,
    pub ruleset_id: String,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub owner_bond: String,
    pub owner_pubkey: String,
    pub payer_spk: String,
    pub funding: FundingInputV1,
    pub fee_sompi: u64,
    pub quote_digest: Hash64,
    pub expiry_daa: u64,
    /// `palw_registration_object_id_v1` of the SIGNED object (what `getPalwModelRegistrationStatus` is asked for).
    pub object_id: Hash64,
    /// The unsigned object's digest the quote named.
    pub object_digest: Hash64,
    /// `borsh(Transaction)` of the signed carrier.
    pub tx_hex: String,
    pub tx_id: Hash64,
}

impl SignedRegistrationV1 {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a signed registration is serializable")
    }

    pub fn from_json(text: &str) -> Result<Self, BundleRefusalV1> {
        let s: Self = serde_json::from_str(text).map_err(|e| BundleRefusalV1::Malformed(format!("not valid JSON: {e}")))?;
        if s.schema != SIGNED_SCHEMA_V1 {
            return Err(BundleRefusalV1::Schema(s.schema));
        }
        Ok(s)
    }

    pub fn transaction(&self) -> Result<Transaction, BundleRefusalV1> {
        let bytes = unhex(&self.tx_hex)?;
        borsh::from_slice::<Transaction>(&bytes)
            .map_err(|e| BundleRefusalV1::Malformed(format!("the transaction does not decode: {e}")))
    }
}

/// **Sign step 2: the payer key signs the carrier's funding input**, after the owner's signature is in the object.
pub fn carrier_sign_v1(
    b: &RegistrationBundleV1,
    policy: &SignerPolicyV1,
    payer: &dyn DetachedSigner,
) -> Result<SignedRegistrationV1, BundleRefusalV1> {
    let checked = check_bundle_v1(b, policy, BundleStageV1::OwnerSigned)?;
    let p = parts_of(&checked.object)?;
    if !matches!(
        verify_mldsa87_with_context(
            &checked.owner_pubkey,
            checked.owner_message.as_byte_slice(),
            &p.carriage.signature,
            PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT
        ),
        Ok(true)
    ) {
        return Err(BundleRefusalV1::OwnerSignature);
    }
    let payer_pk = payer.public_key();
    if funding_spk_of_pubkey_v1(&payer_pk) != checked.payer_spk {
        return Err(BundleRefusalV1::PayerIsNotThisKey);
    }
    let mut tx = carrier_body_v1(
        &checked.object,
        checked.funding_outpoint,
        checked.funding_entry.amount,
        checked.fee_sompi,
        &checked.payer_spk,
        vec![],
    )?;
    let mtx = MutableTransaction::with_entries(tx.clone(), vec![checked.funding_entry.clone()]);
    let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &Mldsa87SigHashReusedValuesUnsync::new());
    let mut sig = payer.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).map_err(BundleRefusalV1::Signer)?;
    if sig.len() != MLDSA87_SIG_LEN {
        return Err(BundleRefusalV1::Signer(format!("a signature of {} bytes, not {MLDSA87_SIG_LEN}", sig.len())));
    }
    sig.push(SIG_HASH_ALL.to_u8());
    tx.inputs[0].signature_script = ScriptBuilder::new()
        .add_data(&sig)
        .and_then(|s| s.add_data(&payer_pk))
        .map(|s| s.drain())
        .map_err(|e| BundleRefusalV1::Signer(format!("the signature script does not build: {e}")))?;
    tx.finalize();
    carrier_funding_signature_valid(&tx, &checked.funding_entry).map_err(BundleRefusalV1::FundingSignature)?;
    let object_id = palw_registration_object_id_v1(&object_bytes(&checked.object));
    Ok(SignedRegistrationV1 {
        schema: SIGNED_SCHEMA_V1.into(),
        network: b.network.clone(),
        network_domain: b.network_domain,
        ruleset_id: b.ruleset_id.clone(),
        class_id: b.class_id,
        artifact_root: b.artifact_root,
        owner_bond: b.owner_bond.clone(),
        owner_pubkey: b.owner_pubkey.clone(),
        payer_spk: b.payer.spk.clone(),
        funding: b.payer.funding.clone(),
        fee_sompi: checked.fee_sompi,
        quote_digest: checked.quote_digest,
        expiry_daa: checked.expiry_daa,
        object_id,
        object_digest: b.object_digest,
        tx_hex: hex(&borsh::to_vec(&tx).expect("a transaction serializes")),
        tx_id: tx_id_of_bytes(&tx),
    })
}

// ---------------------------------------------------------------------------------------------------------------------------
// submit: what anyone can verify from the signed bytes alone
// ---------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct SubmitPolicyV1 {
    pub network: String,
    pub network_domain: Hash64,
    pub expect: ExpectationsV1,
    /// The most the carrier may cost the payer.
    pub max_fee_sompi: u64,
    pub now_daa: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct VerifiedSignedV1 {
    pub tx: Transaction,
    pub tx_id: Hash64,
    pub object_id: Hash64,
    pub fee_sompi: u64,
    /// The signed object the carrier holds (the tracker needs its unsigned form).
    pub object: PalwConsensusObjectV2,
}

/// **Verify a signed registration from its bytes alone** (no key, no node): the transaction is the carrier of exactly the class,
/// root and owner the user expects, the owner's signature holds over the message recomputed here, the funding signature holds, the
/// only output is the change back to the payer, and the fee is within the cap. A relay that flipped any byte fails one of these.
pub fn verify_signed_registration_v1(s: &SignedRegistrationV1, policy: &SubmitPolicyV1) -> Result<VerifiedSignedV1, BundleRefusalV1> {
    if s.schema != SIGNED_SCHEMA_V1 {
        return Err(BundleRefusalV1::Schema(s.schema.clone()));
    }
    if s.network != policy.network {
        return Err(BundleRefusalV1::WrongNetwork { bundle: s.network.clone(), ours: policy.network.clone() });
    }
    if s.network_domain != policy.network_domain {
        return Err(BundleRefusalV1::NetworkDomain);
    }
    let tx = s.transaction()?;
    let id = tx_id_of_bytes(&tx);
    if id != s.tx_id {
        return Err(BundleRefusalV1::NotTheCarrier("the transaction id is not the declared one"));
    }
    if tx.version != TX_VERSION || tx.subnetwork_id != SUBNETWORK_ID_PALW_LIFECYCLE || tx.lock_time != 0 || tx.gas != 0 {
        return Err(BundleRefusalV1::NotTheCarrier("not a lifecycle carrier"));
    }
    let [input] = tx.inputs.as_slice() else {
        return Err(BundleRefusalV1::NotTheCarrier("a carrier has one funding input"));
    };
    if input.previous_outpoint != TransactionOutpoint::new(s.funding.txid, s.funding.index) {
        return Err(BundleRefusalV1::NotTheCarrier("the funding input is not the declared one"));
    }
    let payer_spk = parse_spk(&s.payer_spk)?;
    let [out] = tx.outputs.as_slice() else {
        return Err(BundleRefusalV1::NotTheCarrier("a carrier has exactly one output, the change"));
    };
    if out.script_public_key != payer_spk {
        return Err(BundleRefusalV1::ChangeNotPayer);
    }
    let fee = s.funding.amount.checked_sub(out.value).ok_or(BundleRefusalV1::CarrierArithmetic)?;
    if fee != s.fee_sompi {
        return Err(BundleRefusalV1::FeeNotQuoted { fee, quoted: s.fee_sompi });
    }
    if fee > policy.max_fee_sompi {
        return Err(BundleRefusalV1::FeeAboveCap { fee, cap: policy.max_fee_sompi });
    }
    // The payload is the registration the user expects, signed by the owner.
    let mut rest = tx.payload.as_slice();
    let payload: PalwLifecycleTxPayloadV2 =
        borsh::BorshDeserialize::deserialize(&mut rest).map_err(|_| BundleRefusalV1::NotTheCarrier("the payload does not decode"))?;
    if !rest.is_empty() || payload.version != PALW_LIFECYCLE_TX_VERSION_V2 {
        return Err(BundleRefusalV1::NotTheCarrier("the payload has trailing bytes or another version"));
    }
    let p = parts_of(&payload.object)?;
    if p.class_id != policy.expect.class_id {
        return Err(BundleRefusalV1::ClassSwapped { bundle: p.class_id, expected: policy.expect.class_id });
    }
    if p.artifact_root != policy.expect.artifact_root {
        return Err(BundleRefusalV1::RootSwapped { bundle: p.artifact_root, expected: policy.expect.artifact_root });
    }
    let owner_bond = bond_text(&p.carriage.registrant_bond);
    if owner_bond != policy.expect.owner_bond {
        return Err(BundleRefusalV1::OwnerSwapped { bundle: owner_bond, expected: policy.expect.owner_bond.clone() });
    }
    if p.carriage.profile.shape_profile_id() != p.class_id {
        return Err(BundleRefusalV1::ClassIsNotItsProfile);
    }
    let owner_pubkey = unhex(&s.owner_pubkey)?;
    let message = owner_message_of(policy.network_domain, &p);
    if !matches!(
        verify_mldsa87_with_context(
            &owner_pubkey,
            message.as_byte_slice(),
            &p.carriage.signature,
            PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT
        ),
        Ok(true)
    ) {
        return Err(BundleRefusalV1::OwnerSignature);
    }
    // The funding signature, and that its key owns the funded script.
    let entry = UtxoEntry::new(s.funding.amount, payer_spk.clone(), s.funding.block_daa_score, s.funding.is_coinbase);
    carrier_funding_signature_valid(&tx, &entry).map_err(BundleRefusalV1::FundingSignature)?;
    let (_, funding_pk) = parse_two_pushes(&input.signature_script).ok_or(BundleRefusalV1::NotTheCarrier("the signature script"))?;
    if funding_spk_of_pubkey_v1(funding_pk) != payer_spk {
        return Err(BundleRefusalV1::PayerIsNotThisKey);
    }
    if let Some(now) = policy.now_daa
        && now > s.expiry_daa
    {
        return Err(BundleRefusalV1::Expired { expiry: s.expiry_daa, now });
    }
    if palw_registration_object_id_v1(&object_bytes(&payload.object)) != s.object_id {
        return Err(BundleRefusalV1::NotTheCarrier("the object id is not the declared one"));
    }
    Ok(VerifiedSignedV1 { tx, tx_id: id, object_id: s.object_id, fee_sompi: fee, object: payload.object })
}

// ---------------------------------------------------------------------------------------------------------------------------
// resend vs duplicate registration
// ---------------------------------------------------------------------------------------------------------------------------

/// One earlier send recorded on this machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentRecordV1 {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub tx_id: Hash64,
    pub object_id: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubmissionKindV1 {
    /// Nothing on record: the first send of this registration.
    First,
    /// The same signed bytes (same tx id) were sent before. Sending them again is idempotent: no new fee, no new registration.
    Resend,
    /// ANOTHER carrier for the same class was sent before. A second one is a second fee for a registration the chain refuses as a
    /// duplicate (`DuplicateClass`) if the first folds: refused unless the first is known lost.
    DuplicateRegistration { earlier_tx: Hash64 },
    /// The registry already holds this exact class; nothing to submit. The lifecycle is shown as it stands.
    AlreadyRegistered { lifecycle: String },
    /// The class or the weights are on the registry under something else.
    Conflict(String),
}

/// **Resend or duplicate?** The registry's verdict wins (a class already registered is never re-filed); then the local record.
pub fn classify_submission_v1(
    signed_tx: Hash64,
    class_id: Hash64,
    artifact_root: Hash64,
    sent: &[SentRecordV1],
    registry: &DuplicateVerdictV1,
) -> SubmissionKindV1 {
    match registry {
        DuplicateVerdictV1::Reuse { lifecycle, .. } => return SubmissionKindV1::AlreadyRegistered { lifecycle: lifecycle.clone() },
        DuplicateVerdictV1::Conflict(why) => return SubmissionKindV1::Conflict(why.clone()),
        DuplicateVerdictV1::New => {}
    }
    let earlier: Vec<&SentRecordV1> = sent.iter().filter(|r| r.class_id == class_id && r.artifact_root == artifact_root).collect();
    if earlier.iter().any(|r| r.tx_id == signed_tx) {
        return SubmissionKindV1::Resend;
    }
    match earlier.first() {
        Some(r) => SubmissionKindV1::DuplicateRegistration { earlier_tx: r.tx_id },
        None => SubmissionKindV1::First,
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// multi-RPC quote agreement
// ---------------------------------------------------------------------------------------------------------------------------

/// Nodes that quoted different terms must not be averaged: the builder stops, nothing is exported, nothing is signed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum QuoteDisagreementV1 {
    #[error("only {got} node(s) answered, {need} are required")]
    TooFew { got: usize, need: usize },
    #[error("the nodes disagree on {field}: {values:?}")]
    Field { field: &'static str, values: Vec<(String, String)> },
    #[error("the nodes' tips are {spread} DAA apart, more than the allowed {max}")]
    DaaSkew { spread: u64, max: u64 },
}

/// The facts several nodes agreed on, with the nodes that said so. **Agreement is not a proof**: the result is
/// [`UNVERIFIED_REMOTE_STATE`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgreedQuoteV1 {
    pub facts: RegistrationFactsV1,
    pub sources: Vec<QuoteSourceV1>,
}

/// **Agree on the facts of a quote across independent RPCs.** Identity-type facts (network, terms, class, root, object, burn,
/// exposure, the bond's identity and standing, the carrier price) must be EQUAL on every node — any difference stops everything.
/// Balances may differ by a block: the quote then takes the WORST case (least collateral, most backing and locks, least spendable
/// BILI, highest fee), which can only make the quote stricter. At least `min_nodes` (never fewer than 2) distinct nodes must answer.
pub fn agree_quote_facts_v1(
    answers: &[(String, RegistrationFactsV1)],
    min_nodes: usize,
    max_daa_skew: u64,
) -> Result<AgreedQuoteV1, QuoteDisagreementV1> {
    let mut seen = std::collections::BTreeSet::new();
    let answers: Vec<&(String, RegistrationFactsV1)> = answers.iter().filter(|(n, _)| seen.insert(n.clone())).collect();
    let need = min_nodes.max(2);
    if answers.len() < need {
        return Err(QuoteDisagreementV1::TooFew { got: answers.len(), need });
    }
    macro_rules! same {
        ($field:literal, $get:expr) => {{
            let first = $get(&answers[0].1);
            if answers.iter().any(|(_, f)| $get(f) != first) {
                return Err(QuoteDisagreementV1::Field {
                    field: $field,
                    values: answers.iter().map(|(n, f)| (n.clone(), format!("{:?}", $get(f)))).collect(),
                });
            }
        }};
    }
    same!("network domain", |f: &RegistrationFactsV1| f.network_domain);
    same!("registration terms", |f: &RegistrationFactsV1| f.terms_digest);
    same!("class id", |f: &RegistrationFactsV1| f.class_id);
    same!("artifact root", |f: &RegistrationFactsV1| f.artifact_root);
    same!("object digest", |f: &RegistrationFactsV1| f.object_digest);
    same!("registration burn", |f: &RegistrationFactsV1| f.burn_sompi);
    same!("registration exposure", |f: &RegistrationFactsV1| f.exposure_sompi);
    same!("carrier mass", |f: &RegistrationFactsV1| f.carrier_mass);
    same!("bond identity", |f: &RegistrationFactsV1| (f.bond.outpoint.clone(), f.bond.known, f.bond.key_matches, f.bond.retiring));
    same!("filings", |f: &RegistrationFactsV1| f.filings.clone());
    let lo = answers.iter().map(|(_, f)| f.tip_daa).min().expect("non-empty");
    let hi = answers.iter().map(|(_, f)| f.tip_daa).max().expect("non-empty");
    if hi - lo > max_daa_skew {
        return Err(QuoteDisagreementV1::DaaSkew { spread: hi - lo, max: max_daa_skew });
    }
    let mut facts = answers[0].1.clone();
    // The conservative tip: the lowest DAA (its hash), so the expiry cannot outlive the slowest node's clock.
    let slowest = answers.iter().min_by_key(|(_, f)| f.tip_daa).expect("non-empty");
    facts.tip_daa = slowest.1.tip_daa;
    facts.tip_hash = slowest.1.tip_hash;
    facts.bond.collateral_sompi = answers.iter().map(|(_, f)| f.bond.collateral_sompi).min().expect("non-empty");
    facts.bond.backing_sompi = answers.iter().map(|(_, f)| f.bond.backing_sompi).max().expect("non-empty");
    facts.bond.live_locked_sompi = answers.iter().map(|(_, f)| f.bond.live_locked_sompi).max().expect("non-empty");
    facts.carrier_fee_sompi = answers.iter().map(|(_, f)| f.carrier_fee_sompi).max().expect("non-empty");
    facts.wallet_spendable_sompi = answers.iter().map(|(_, f)| f.wallet_spendable_sompi).min().expect("non-empty");
    let sources = answers.iter().map(|(n, f)| QuoteSourceV1 { node: n.clone(), tip_hash: f.tip_hash, tip_daa: f.tip_daa }).collect();
    Ok(AgreedQuoteV1 { facts, sources })
}

// ---------------------------------------------------------------------------------------------------------------------------
// the shared onboarding lifecycle, as this client can and cannot see it
// ---------------------------------------------------------------------------------------------------------------------------

/// One line of the onboarding view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnboardingLineV1 {
    pub code: &'static str,
    /// `reached` / `DORMANT_NOT_INTEGRATED` / `not reached`.
    pub status: &'static str,
    pub note: String,
}

/// **Accepted is not Active.** The chain's class registry is the only on-chain source this client reads, and a row there means
/// `REGISTERED_DORMANT` at most. `CHALLENGE_PENDING`, `CONFORMANCE_PASSED`, `G14_ELIGIBLE` and `ACTIVE_REWARDABLE`, and the pending
/// failures `BEACON_UNAVAILABLE` / `PUBLIC_PROSECUTION_INCOMPLETE`, have no on-chain source yet (their fences are unarmed): they are
/// shown as dormant, never inferred from a registry row, a relay ACK or a mined carrier. The registry's own native lifecycle string
/// (`Candidate`, `Active…`) is the class-share lifecycle of the existing chain, not the RFC-0011 `ACTIVE_REWARDABLE` state.
pub fn onboarding_view_v1(registry_lifecycle: Option<&str>) -> Vec<OnboardingLineV1> {
    use misaka_palw_challenge::{OnboardingFailureV1 as F, OnboardingStateV1 as S};
    let mut out = Vec::new();
    match registry_lifecycle {
        Some(native) => out.push(OnboardingLineV1 {
            code: S::RegisteredDormant.code(),
            status: "reached",
            note: format!("the registry holds the class (native registry state `{native}`, {UNVERIFIED_REMOTE_STATE}); the native state is the existing class-share lifecycle, not ACTIVE_REWARDABLE"),
        }),
        None => out.push(OnboardingLineV1 {
            code: S::RegisteredDormant.code(),
            status: "not reached",
            note: "no node reported a registry row for the class (a relay ACK or a mined carrier is not a registration)".into(),
        }),
    }
    for s in [S::ChallengePending, S::ConformancePassed, S::G14Eligible, S::ActiveRewardable] {
        out.push(OnboardingLineV1 {
            code: s.code(),
            status: "DORMANT_NOT_INTEGRATED",
            note: "no on-chain source reports this stage yet; it is never inferred from the registry row".into(),
        });
    }
    for f in [F::BeaconUnavailable, F::PublicProsecutionIncomplete] {
        out.push(OnboardingLineV1 {
            code: f.code(),
            status: "DORMANT_NOT_INTEGRATED",
            note: if f.is_pending() {
                "pending, retryable, never a pass and never fraud — tracked, not produced, until its fence is armed".into()
            } else {
                "a recorded outcome when the code-derived prosecution gate runs — not yet produced by any node".into()
            },
        });
    }
    out.push(OnboardingLineV1 {
        code: "MODEL_LINE",
        status: "not created",
        note: "class registration creates a class; a model line is a separate object (`misaka model market open`)".into(),
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::{BondFactsV1, RegistryRowV1, quote_registration_v1};
    use kaspa_consensus_core::palw_base0_profile::rc_job_context;
    use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_1_5B_A16, QWEN25_A16_CANONICAL, qwen25_a16_profile_v1};
    use libcrux_ml_dsa::ml_dsa_87;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }

    pub(crate) struct TestKey {
        kp: ml_dsa_87::MLDSA87KeyPair,
    }
    impl TestKey {
        pub(crate) fn new(seed: u8) -> Self {
            Self { kp: ml_dsa_87::generate_key_pair([seed; 32]) }
        }
    }
    impl DetachedSigner for TestKey {
        fn public_key(&self) -> Vec<u8> {
            let pk: &[u8] = self.kp.verification_key.as_ref();
            pk.to_vec()
        }
        fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
            ml_dsa_87::sign(&self.kp.signing_key, message, context, [9u8; 32])
                .map(|s| {
                    let sig: &[u8] = s.as_ref();
                    sig.to_vec()
                })
                .map_err(|e| format!("{e:?}"))
        }
    }

    const NET: &str = "testnet-12";

    fn domain() -> Hash64 {
        h(0x11)
    }
    fn ruleset() -> String {
        "ruleset-fingerprint-a".into()
    }
    fn bond_outpoint() -> TransactionOutpoint {
        TransactionOutpoint::new(h(0x33), 1)
    }
    fn bond_str() -> String {
        format!("{}:{}", h(0x33), 1)
    }

    fn unsigned_object(root: Hash64) -> PalwConsensusObjectV2 {
        let profile = qwen25_a16_profile_v1(QWEN25_1_5B_A16).expect("the A16 geometry projects");
        let canonical = rc_job_context(&profile, QWEN25_A16_CANONICAL.0, QWEN25_A16_CANONICAL.1);
        PalwConsensusObjectV2::ClassRegistered {
            class_id: profile.shape_profile_id(),
            artifact_root: root,
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1_000 },
            initial_target: 1u128 << 100,
            share_permille: 10,
            activation_daa: 0,
            admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
                profile,
                canonical,
                registrant_bond: PalwBondKeyV2(bond_outpoint()),
                signature: Vec::new(),
            })),
        }
    }

    struct Fixture {
        owner: TestKey,
        payer: TestKey,
        bundle: RegistrationBundleV1,
        policy: SignerPolicyV1,
    }

    fn facts_for(object: &PalwConsensusObjectV2, owner_pk: &[u8], fee: u64) -> RegistrationFactsV1 {
        let p = parts_of(object).unwrap();
        RegistrationFactsV1 {
            network_domain: domain(),
            terms_digest: h(0x44),
            tip_hash: h(0x55),
            tip_daa: 1_000,
            class_id: p.class_id,
            artifact_root: p.artifact_root,
            object_digest: unsigned_object_digest_v1(object).unwrap(),
            exposure_sompi: 40_000,
            burn_sompi: PALW_CLASS_REGISTRATION_BURN_SOMPI_V1,
            bond: BondFactsV1 {
                outpoint: bond_str(),
                known: true,
                key_matches: !owner_pk.is_empty(),
                retiring: false,
                collateral_sompi: 100 * 100_000_000,
                backing_sompi: 10 * 100_000_000,
                live_locked_sompi: 0,
            },
            carrier_mass: 40_000,
            carrier_fee_sompi: fee,
            wallet_spendable_sompi: 5_000_000,
            filings: vec![],
        }
    }

    fn fixture() -> Fixture {
        let owner = TestKey::new(1);
        let payer = TestKey::new(2);
        let root = h(0x77);
        let object = unsigned_object(root);
        let payer_spk = funding_spk_of_pubkey_v1(&payer.public_key());
        let fee = 200_000u64;
        let facts = facts_for(&object, &owner.public_key(), fee);
        let quote = quote_registration_v1(facts, 600, 300_000).expect("the fixture quote holds");
        let bundle = build_bundle_v1(BundleInputsV1 {
            network: NET.into(),
            network_domain: domain(),
            ruleset_id: ruleset(),
            unsigned_object: object.clone(),
            owner_pubkey: owner.public_key(),
            owner_proof: None,
            payer_address: "kaspatest:payer".into(),
            payer_spk: payer_spk.clone(),
            funding_outpoint: TransactionOutpoint::new(h(0x66), 0),
            funding_entry: UtxoEntry::new(5_000_000, payer_spk, 900, false),
            quote,
            sources: vec![QuoteSourceV1 { node: "a".into(), tip_hash: h(0x55), tip_daa: 1_000 }],
        })
        .expect("the fixture bundle builds");
        let policy = SignerPolicyV1 {
            network: NET.into(),
            network_domain: domain(),
            ruleset_id: ruleset(),
            registration_exposure_sompi: 40_000,
            expect: ExpectationsV1 { class_id: parts_of(&object).unwrap().class_id, artifact_root: root, owner_bond: bond_str() },
            max_wallet_sompi: 300_000,
            now_daa: Some(1_001),
            pin: None,
        };
        Fixture { owner, payer, bundle, policy }
    }

    fn sign_both(fx: &Fixture) -> SignedRegistrationV1 {
        let owner_signed = owner_sign_v1(&fx.bundle, &fx.policy, &fx.owner).expect("owner signs");
        carrier_sign_v1(&owner_signed, &fx.policy, &fx.payer).expect("payer signs")
    }

    fn submit_policy(fx: &Fixture) -> SubmitPolicyV1 {
        SubmitPolicyV1 {
            network: NET.into(),
            network_domain: domain(),
            expect: fx.policy.expect.clone(),
            max_fee_sompi: fx.policy.max_wallet_sompi,
            now_daa: Some(1_001),
        }
    }

    #[test]
    fn the_unsigned_bundle_holds_no_key_and_round_trips_as_json() {
        let fx = fixture();
        assert_eq!(fx.bundle.stage, BundleStageV1::Unsigned);
        let text = fx.bundle.to_json();
        assert_eq!(RegistrationBundleV1::from_json(&text).unwrap(), fx.bundle);
        // The quote carries u128 figures; they survive the JSON.
        let mut big = fx.bundle.clone();
        big.quote.bond_required_sompi = u128::from(u64::MAX) * 3;
        assert_eq!(RegistrationBundleV1::from_json(&big.to_json()).unwrap().quote.bond_required_sompi, u128::from(u64::MAX) * 3);
        assert_eq!(fx.bundle.remote_state, UNVERIFIED_REMOTE_STATE);
        // The unsigned object has an empty owner signature.
        let obj = decode_object(&fx.bundle.object_hex).unwrap();
        assert!(parts_of(&obj).unwrap().carriage.signature.is_empty());
    }

    #[test]
    fn detached_signing_yields_a_carrier_that_verifies_from_its_bytes_alone_with_two_different_keys() {
        let fx = fixture();
        let signed = sign_both(&fx);
        let v = verify_signed_registration_v1(&signed, &submit_policy(&fx)).expect("verifies");
        assert_eq!(v.tx_id, signed.tx_id);
        assert_eq!(v.fee_sompi, 200_000);
        // The fee payer is NOT the bond key: the input is locked to the payer's address, the object signed by the owner.
        assert_ne!(fx.owner.public_key(), fx.payer.public_key());
        assert_eq!(v.tx.outputs[0].script_public_key, funding_spk_of_pubkey_v1(&fx.payer.public_key()));
        assert_eq!(v.tx.outputs[0].value, 5_000_000 - 200_000);
        // The same key may do both.
        let mut same = fixture();
        let owner_spk = funding_spk_of_pubkey_v1(&same.owner.public_key());
        same.bundle.payer.spk = spk_text_v1(&owner_spk);
        same.bundle.carrier.change_spk = spk_text_v1(&owner_spk);
        let owner_signed = owner_sign_v1(&same.bundle, &same.policy, &same.owner).unwrap();
        let one_key = carrier_sign_v1(&owner_signed, &same.policy, &same.owner).unwrap();
        verify_signed_registration_v1(&one_key, &submit_policy(&same)).unwrap();
    }

    #[test]
    fn a_builder_that_swaps_the_change_recipient_is_refused_before_anything_is_signed() {
        let mut fx = fixture();
        fx.bundle.carrier.change_spk = spk_text_v1(&funding_spk_of_pubkey_v1(&TestKey::new(9).public_key()));
        assert_eq!(owner_sign_v1(&fx.bundle, &fx.policy, &fx.owner).unwrap_err(), BundleRefusalV1::ChangeNotPayer);
        // Replacing the funded script too: the payer key refuses a funding input that is not its own.
        let mut fx = fixture();
        let attacker = spk_text_v1(&funding_spk_of_pubkey_v1(&TestKey::new(9).public_key()));
        fx.bundle.payer.spk = attacker.clone();
        fx.bundle.carrier.change_spk = attacker;
        let owner_signed = owner_sign_v1(&fx.bundle, &fx.policy, &fx.owner).unwrap();
        assert_eq!(carrier_sign_v1(&owner_signed, &fx.policy, &fx.payer).unwrap_err(), BundleRefusalV1::PayerIsNotThisKey);
    }

    #[test]
    fn a_builder_that_inflates_the_fee_is_refused_by_the_cap_and_by_the_quote() {
        // Fee inflated inside the plan only: the quote still says 200,000.
        let mut fx = fixture();
        fx.bundle.carrier.fee_sompi = 4_000_000;
        fx.bundle.carrier.change_sompi = 1_000_000;
        assert!(matches!(
            owner_sign_v1(&fx.bundle, &fx.policy, &fx.owner),
            Err(BundleRefusalV1::QuoteInconsistent("carrier fee or mass"))
        ));
        // Fee AND quote inflated together: the signer's own cap stops it.
        let mut fx = fixture();
        fx.bundle.carrier.fee_sompi = 4_000_000;
        fx.bundle.carrier.change_sompi = 1_000_000;
        fx.bundle.quote.facts.carrier_fee_sompi = 4_000_000;
        fx.bundle.quote.wallet_total_sompi = 4_000_000;
        assert_eq!(
            owner_sign_v1(&fx.bundle, &fx.policy, &fx.owner).unwrap_err(),
            BundleRefusalV1::FeeAboveCap { fee: 4_000_000, cap: 300_000 }
        );
        // Change that does not add up (value created or burnt beside the fee).
        let mut fx = fixture();
        fx.bundle.carrier.change_sompi -= 1;
        assert_eq!(owner_sign_v1(&fx.bundle, &fx.policy, &fx.owner).unwrap_err(), BundleRefusalV1::CarrierArithmetic);
    }

    #[test]
    fn a_builder_that_swaps_the_class_root_or_the_owner_is_refused_against_what_the_user_expects() {
        // Another artifact root inside the object, with every derived field recomputed to be self-consistent: only the user's
        // expectation can catch it.
        let evil_object = unsigned_object(h(0x88));
        let fx = fixture();
        let mut evil = fx.bundle.clone();
        evil.object_hex = hex(&object_bytes(&evil_object));
        evil.artifact_root = h(0x88);
        evil.object_digest = unsigned_object_digest_v1(&evil_object).unwrap();
        evil.owner_message = owner_message_of(domain(), &parts_of(&evil_object).unwrap());
        evil.quote.facts.artifact_root = h(0x88);
        evil.quote.facts.object_digest = evil.object_digest;
        assert_eq!(
            owner_sign_v1(&evil, &fx.policy, &fx.owner).unwrap_err(),
            BundleRefusalV1::RootSwapped { bundle: h(0x88), expected: h(0x77) }
        );
        // The same object but the message the bundle says is signed is not the real one.
        let mut lying = fx.bundle.clone();
        lying.owner_message = h(0x99);
        assert_eq!(owner_sign_v1(&lying, &fx.policy, &fx.owner).unwrap_err(), BundleRefusalV1::OwnerMessage);
        // Another owner bond inside the object.
        let mut other_owner = unsigned_object(h(0x77));
        if let PalwConsensusObjectV2::ClassRegistered { admission: Some(c), .. } = &mut other_owner {
            c.registrant_bond = PalwBondKeyV2(TransactionOutpoint::new(h(0xAB), 0));
        }
        let mut evil = fx.bundle.clone();
        evil.object_hex = hex(&object_bytes(&other_owner));
        evil.owner_bond = format!("{}:{}", h(0xAB), 0);
        evil.object_digest = unsigned_object_digest_v1(&other_owner).unwrap();
        evil.owner_message = owner_message_of(domain(), &parts_of(&other_owner).unwrap());
        evil.quote.facts.object_digest = evil.object_digest;
        evil.quote.facts.bond.outpoint = evil.owner_bond.clone();
        assert!(matches!(owner_sign_v1(&evil, &fx.policy, &fx.owner).unwrap_err(), BundleRefusalV1::OwnerSwapped { .. }));
        // Another class id (the profile no longer hashes to it).
        let mut other_class = unsigned_object(h(0x77));
        if let PalwConsensusObjectV2::ClassRegistered { class_id, .. } = &mut other_class {
            *class_id = h(0xCD);
        }
        let mut evil = fx.bundle.clone();
        evil.object_hex = hex(&object_bytes(&other_class));
        evil.class_id = h(0xCD);
        assert!(matches!(owner_sign_v1(&evil, &fx.policy, &fx.owner).unwrap_err(), BundleRefusalV1::ClassIsNotItsProfile));
        // A key that is not the bond's: refused, not signed.
        assert_eq!(owner_sign_v1(&fx.bundle, &fx.policy, &TestKey::new(7)).unwrap_err(), BundleRefusalV1::OwnerKeyMismatch);
    }

    #[test]
    fn network_ruleset_expiry_and_understated_costs_are_refused() {
        let fx = fixture();
        let mut p = fx.policy.clone();
        p.network_domain = h(0x12);
        assert_eq!(owner_sign_v1(&fx.bundle, &p, &fx.owner).unwrap_err(), BundleRefusalV1::NetworkDomain);
        let mut p = fx.policy.clone();
        p.ruleset_id = "ruleset-fingerprint-b".into();
        assert_eq!(owner_sign_v1(&fx.bundle, &p, &fx.owner).unwrap_err(), BundleRefusalV1::Ruleset);
        let mut p = fx.policy.clone();
        p.network = "mainnet".into();
        assert!(matches!(owner_sign_v1(&fx.bundle, &p, &fx.owner), Err(BundleRefusalV1::WrongNetwork { .. })));
        let mut p = fx.policy.clone();
        p.now_daa = Some(1_601);
        assert_eq!(owner_sign_v1(&fx.bundle, &p, &fx.owner).unwrap_err(), BundleRefusalV1::Expired { expiry: 1_600, now: 1_601 });
        // Offline (no clock): the expiry is not checked here — the submit step checks it.
        let mut p = fx.policy.clone();
        p.now_daa = None;
        owner_sign_v1(&fx.bundle, &p, &fx.owner).unwrap();
        // A quote that understates the exposure this build knows.
        let mut p = fx.policy.clone();
        p.registration_exposure_sompi = 80_000;
        assert!(matches!(
            owner_sign_v1(&fx.bundle, &p, &fx.owner),
            Err(BundleRefusalV1::CostUnderstated { what: "registration exposure", .. })
        ));
        // Burn of another size than the chain's.
        let mut fx = fixture();
        fx.bundle.quote.facts.burn_sompi = 1;
        fx.bundle.quote.bond_debit_sompi = 1;
        assert!(matches!(
            owner_sign_v1(&fx.bundle, &fx.policy, &fx.owner),
            Err(BundleRefusalV1::CostUnderstated { what: "registration burn", .. })
        ));
    }

    #[test]
    fn the_stages_cannot_be_skipped_or_repeated() {
        let fx = fixture();
        // The payer cannot sign a bundle the owner has not signed.
        assert!(matches!(carrier_sign_v1(&fx.bundle, &fx.policy, &fx.payer), Err(BundleRefusalV1::Stage { .. })));
        // The owner cannot sign twice.
        let signed = owner_sign_v1(&fx.bundle, &fx.policy, &fx.owner).unwrap();
        assert!(matches!(owner_sign_v1(&signed, &fx.policy, &fx.owner), Err(BundleRefusalV1::Stage { .. })));
        // A forged owner signature in an "owner-signed" bundle is refused by the payer.
        let mut forged = signed.clone();
        let mut obj = decode_object(&forged.object_hex).unwrap();
        if let PalwConsensusObjectV2::ClassRegistered { admission: Some(c), .. } = &mut obj {
            c.signature[0] ^= 1;
        }
        forged.object_hex = hex(&object_bytes(&obj));
        assert_eq!(carrier_sign_v1(&forged, &fx.policy, &fx.payer).unwrap_err(), BundleRefusalV1::OwnerSignature);
    }

    #[test]
    fn a_relay_that_alters_the_signed_bytes_is_caught_before_and_after_the_wire() {
        let fx = fixture();
        let signed = sign_both(&fx);
        let tx = signed.transaction().unwrap();
        let reship = |tx: &Transaction| -> SignedRegistrationV1 {
            let mut s = signed.clone();
            s.tx_hex = hex(&borsh::to_vec(tx).unwrap());
            s.tx_id = tx_id_of_bytes(tx);
            s
        };
        // Recipient of the change.
        let mut t = tx.clone();
        t.outputs[0].script_public_key = funding_spk_of_pubkey_v1(&TestKey::new(9).public_key());
        assert_eq!(verify_signed_registration_v1(&reship(&t), &submit_policy(&fx)).unwrap_err(), BundleRefusalV1::ChangeNotPayer);
        // Fee (the change shrinks): the declared fee no longer matches, and the funding signature would not cover it anyway.
        let mut t = tx.clone();
        t.outputs[0].value -= 1;
        assert!(matches!(verify_signed_registration_v1(&reship(&t), &submit_policy(&fx)), Err(BundleRefusalV1::FeeNotQuoted { .. })));
        // A relay that also "corrects" the declared fee is stopped by the funding signature (SIG_HASH_ALL covers every output).
        let mut s = reship(&t);
        s.fee_sompi += 1;
        assert!(matches!(verify_signed_registration_v1(&s, &submit_policy(&fx)), Err(BundleRefusalV1::FundingSignature(_))));
        // Class root inside the payload: the owner signature no longer holds (and the funding signature neither).
        let mut t = tx.clone();
        let mut payload: PalwLifecycleTxPayloadV2 = borsh::from_slice(&t.payload).unwrap();
        if let PalwConsensusObjectV2::ClassRegistered { artifact_root, .. } = &mut payload.object {
            *artifact_root = h(0x88);
        }
        t.payload = borsh::to_vec(&payload).unwrap();
        assert!(matches!(verify_signed_registration_v1(&reship(&t), &submit_policy(&fx)), Err(BundleRefusalV1::RootSwapped { .. })));
        // Owner bond inside the payload.
        let mut t = tx.clone();
        let mut payload: PalwLifecycleTxPayloadV2 = borsh::from_slice(&t.payload).unwrap();
        if let PalwConsensusObjectV2::ClassRegistered { admission: Some(c), .. } = &mut payload.object {
            c.registrant_bond = PalwBondKeyV2(TransactionOutpoint::new(h(0xAB), 0));
        }
        t.payload = borsh::to_vec(&payload).unwrap();
        assert!(matches!(verify_signed_registration_v1(&reship(&t), &submit_policy(&fx)), Err(BundleRefusalV1::OwnerSwapped { .. })));
        // A flipped bit in the funding signature.
        let mut t = tx.clone();
        let n = t.inputs[0].signature_script.len();
        t.inputs[0].signature_script[n / 4] ^= 1;
        assert!(matches!(verify_signed_registration_v1(&reship(&t), &submit_policy(&fx)), Err(BundleRefusalV1::FundingSignature(_))));
        // A declared id that is not the transaction's.
        let mut s = signed.clone();
        s.tx_id = h(1);
        assert!(matches!(verify_signed_registration_v1(&s, &submit_policy(&fx)), Err(BundleRefusalV1::NotTheCarrier(_))));
        // The expiry is enforced at submit time (client side only).
        let mut late = submit_policy(&fx);
        late.now_daa = Some(1_601);
        assert!(matches!(verify_signed_registration_v1(&signed, &late), Err(BundleRefusalV1::Expired { .. })));
        // The fee cap is the SUBMITTER's.
        let mut tight = submit_policy(&fx);
        tight.max_fee_sompi = 199_999;
        assert!(matches!(verify_signed_registration_v1(&signed, &tight), Err(BundleRefusalV1::FeeAboveCap { .. })));
    }

    #[test]
    fn resend_is_idempotent_a_second_carrier_is_a_duplicate_and_the_registry_wins() {
        let fx = fixture();
        let signed = sign_both(&fx);
        let (c, r) = (signed.class_id, signed.artifact_root);
        let rec = SentRecordV1 { class_id: c, artifact_root: r, tx_id: signed.tx_id, object_id: signed.object_id };
        let new = DuplicateVerdictV1::New;
        assert_eq!(classify_submission_v1(signed.tx_id, c, r, &[], &new), SubmissionKindV1::First);
        assert_eq!(classify_submission_v1(signed.tx_id, c, r, std::slice::from_ref(&rec), &new), SubmissionKindV1::Resend);
        assert_eq!(
            classify_submission_v1(h(0xEE), c, r, std::slice::from_ref(&rec), &new),
            SubmissionKindV1::DuplicateRegistration { earlier_tx: signed.tx_id }
        );
        // Another class on record is unrelated.
        assert_eq!(classify_submission_v1(h(0xEE), h(1), h(2), std::slice::from_ref(&rec), &new), SubmissionKindV1::First);
        // The registry's own verdict wins over the local record, and the lifecycle is shown as it stands.
        let rows = vec![RegistryRowV1 { class_id: c, artifact_root: r, registrant_bond: None, lifecycle: "Candidate".into() }];
        let verdict = crate::register::exact_duplicate_v1(c, r, &rows);
        assert_eq!(
            classify_submission_v1(signed.tx_id, c, r, std::slice::from_ref(&rec), &verdict),
            SubmissionKindV1::AlreadyRegistered { lifecycle: "Candidate".into() }
        );
        let near = vec![RegistryRowV1 { class_id: c, artifact_root: h(9), registrant_bond: None, lifecycle: "Active".into() }];
        assert!(matches!(
            classify_submission_v1(signed.tx_id, c, r, &[], &crate::register::exact_duplicate_v1(c, r, &near)),
            SubmissionKindV1::Conflict(_)
        ));
    }

    #[test]
    fn nodes_that_quote_different_terms_are_never_averaged() {
        let fx = fixture();
        let base = fx.bundle.quote.facts.clone();
        let ans = |n: &str, f: RegistrationFactsV1| (n.to_string(), f);
        // Agreement, with the conservative merge of balances.
        let mut b = base.clone();
        b.tip_daa += 3;
        b.bond.backing_sompi += 7;
        b.wallet_spendable_sompi -= 5;
        b.bond.collateral_sompi -= 11;
        let agreed = agree_quote_facts_v1(&[ans("a", base.clone()), ans("b", b)], 2, 12).unwrap();
        assert_eq!(agreed.facts.tip_daa, base.tip_daa);
        assert_eq!(agreed.facts.bond.backing_sompi, base.bond.backing_sompi + 7);
        assert_eq!(agreed.facts.wallet_spendable_sompi, base.wallet_spendable_sompi - 5);
        assert_eq!(agreed.facts.bond.collateral_sompi, base.bond.collateral_sompi - 11);
        assert_eq!(agreed.sources.len(), 2);
        // One node (or the same node twice) is not agreement.
        assert!(matches!(
            agree_quote_facts_v1(&[ans("a", base.clone())], 1, 12),
            Err(QuoteDisagreementV1::TooFew { got: 1, need: 2 })
        ));
        assert!(matches!(
            agree_quote_facts_v1(&[ans("a", base.clone()), ans("a", base.clone())], 2, 12),
            Err(QuoteDisagreementV1::TooFew { got: 1, .. })
        ));
        // Terms, burn, exposure, root, bond standing: any difference stops everything.
        for (field, mutate) in [
            ("registration terms", (|f: &mut RegistrationFactsV1| f.terms_digest = h(0xF1)) as fn(&mut RegistrationFactsV1)),
            ("registration burn", |f| f.burn_sompi = 0),
            ("registration exposure", |f| f.exposure_sompi += 1),
            ("artifact root", |f| f.artifact_root = h(0xF2)),
            ("network domain", |f| f.network_domain = h(0xF3)),
            ("bond identity", |f| f.bond.retiring = true),
            ("carrier mass", |f| f.carrier_mass += 1),
        ] {
            let mut other = base.clone();
            mutate(&mut other);
            match agree_quote_facts_v1(&[ans("a", base.clone()), ans("b", other)], 2, 12) {
                Err(QuoteDisagreementV1::Field { field: got, .. }) => assert_eq!(got, field),
                other => panic!("{field}: {other:?}"),
            }
        }
        let mut far = base.clone();
        far.tip_daa += 100;
        assert!(matches!(
            agree_quote_facts_v1(&[ans("a", base.clone()), ans("b", far)], 2, 12),
            Err(QuoteDisagreementV1::DaaSkew { .. })
        ));
    }

    #[test]
    fn accepted_is_not_active_the_view_names_the_shared_codes_and_infers_nothing() {
        let none = onboarding_view_v1(None);
        assert_eq!(none[0].code, "REGISTERED_DORMANT");
        assert_eq!(none[0].status, "not reached");
        let reg = onboarding_view_v1(Some("Candidate"));
        assert_eq!(reg[0].status, "reached");
        assert!(reg[0].note.contains(UNVERIFIED_REMOTE_STATE));
        for later in [
            "CHALLENGE_PENDING",
            "CONFORMANCE_PASSED",
            "G14_ELIGIBLE",
            "ACTIVE_REWARDABLE",
            "BEACON_UNAVAILABLE",
            "PUBLIC_PROSECUTION_INCOMPLETE",
        ] {
            let l = reg.iter().find(|l| l.code == later).unwrap_or_else(|| panic!("{later} missing"));
            assert_eq!(l.status, "DORMANT_NOT_INTEGRATED", "{later}");
        }
        // Even a native `Active` registry row never yields the RFC-0011 rewardable state.
        let active = onboarding_view_v1(Some("Active (share 10‰)"));
        assert_eq!(active.iter().find(|l| l.code == "ACTIVE_REWARDABLE").unwrap().status, "DORMANT_NOT_INTEGRATED");
        assert_eq!(reg.last().unwrap().code, "MODEL_LINE");
    }

    #[test]
    fn the_carrier_body_has_the_shape_the_shipped_builder_makes() {
        // One input with the final sequence, one change output, subnetwork 0x4b, lock time 0 — what ValidatorKey::build_palw_lifecycle_tx makes.
        let fx = fixture();
        let obj = decode_object(&fx.bundle.object_hex).unwrap();
        let spk = funding_spk_of_pubkey_v1(&fx.payer.public_key());
        let tx =
            carrier_body_v1(&obj, TransactionOutpoint::new(h(0x66), 0), 5_000_000, 200_000, &spk, placeholder_funding_script_v1())
                .unwrap();
        assert_eq!(tx.version, TX_VERSION);
        assert_eq!(tx.subnetwork_id, SUBNETWORK_ID_PALW_LIFECYCLE);
        assert_eq!(tx.inputs[0].sequence, MAX_TX_IN_SEQUENCE_NUM);
        assert_eq!(tx.inputs[0].sig_op_count, 1);
        assert_eq!(tx.outputs.len(), 1);
        assert_eq!(tx.outputs[0].value, 4_800_000);
        // The placeholder script has the real length: sig (4627) + hashtype (1) + pubkey (2592) plus push framing.
        let real_len = {
            let signed = sign_both(&fx);
            signed.transaction().unwrap().inputs[0].signature_script.len()
        };
        assert_eq!(placeholder_funding_script_v1().len(), real_len);
    }

    /// A bonds collection committed by a header, hand-built: (header, proof of the bonds table).
    fn committed_bonds(
        bonds: Vec<(PalwBondKeyV2, kaspa_consensus_core::palw_state_v2::PalwBondStateV2)>,
    ) -> (kaspa_consensus_core::header::Header, crate::proof::StateProofV1) {
        use kaspa_consensus_core::palw_state_v2::{palw_collection_root_of_entries_v1, palw_state_root_of_preimage_v1};
        let rows: Vec<(Vec<u8>, Vec<u8>)> =
            bonds.iter().map(|(k, v)| (borsh::to_vec(k).unwrap(), borsh::to_vec(v).unwrap())).collect();
        let root = palw_collection_root_of_entries_v1(b"bonds", rows.len(), rows.iter().cloned());
        let mut preimage = vec![0xAA; 7];
        preimage.extend_from_slice(root.as_bytes().as_slice());
        let state_root = palw_state_root_of_preimage_v1(&preimage);
        let mut header = kaspa_consensus_core::header::Header::new_finalized(
            1,
            vec![vec![h(1)]].try_into().unwrap(),
            h(2),
            h(3),
            h(4),
            1,
            0x1d00ffff,
            0,
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
            55,
            0u64.into(),
            0,
            h(5),
        )
        .with_palw_state_root(state_root);
        header.finalize();
        let proof = kaspa_consensus_core::palw_state_proof_v1::PalwFactProofV1 {
            state: kaspa_consensus_core::palw_state_proof_v1::PalwStateOpeningV1 { preimage },
            collection: kaspa_consensus_core::palw_state_proof_v1::PalwCollectionOpeningV1 { label: b"bonds".to_vec(), rows },
        };
        let block = header.hash;
        (header.clone(), crate::proof::StateProofV1::new(block, &header, &proof))
    }

    fn bond_record(pubkey: Vec<u8>) -> kaspa_consensus_core::palw_state_v2::PalwBondStateV2 {
        kaspa_consensus_core::palw_state_v2::PalwBondStateV2 {
            pubkey,
            operator_id: h(1),
            collateral: 100,
            slashed: 0,
            status: kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active,
            registered_daa: 3,
            payout_payload: h(2),
            capable_classes: Default::default(),
        }
    }

    #[test]
    fn a_signer_that_pinned_a_block_checks_the_owner_key_against_it_offline() {
        let fx = fixture();
        let owner_pk = fx.owner.public_key();
        let bond = PalwBondKeyV2(bond_outpoint());
        // The proof is embedded by the builder; the signer's pin is what makes it count.
        let (header, proof) = committed_bonds(vec![(bond, bond_record(owner_pk.clone()))]);
        let mut with_proof = fx.bundle.clone();
        with_proof.owner_proof = Some(proof.clone());
        let mut pinned = fx.policy.clone();
        pinned.pin = Some(header.hash);
        let signed = owner_sign_v1(&with_proof, &pinned, &fx.owner).expect("the proven key signs");
        assert_eq!(signed.stage, BundleStageV1::OwnerSigned);
        let checked = check_bundle_v1(&with_proof, &pinned, BundleStageV1::Unsigned).unwrap();
        assert!(matches!(checked.owner_key_provenance, Provenance::ProvenAtPin { .. }));
        assert!(checked.review_lines(&with_proof).iter().any(|l| l.contains("PROVEN against pinned block")));
        // Without a pin the same bundle is shown as a node's report.
        let unpinned = check_bundle_v1(&with_proof, &fx.policy, BundleStageV1::Unsigned).unwrap();
        assert!(matches!(unpinned.owner_key_provenance, Provenance::UnverifiedRemoteState { .. }));
        assert!(unpinned.review_lines(&with_proof).iter().any(|l| l.contains(UNVERIFIED_REMOTE_STATE)));
        // A pin with no proof in the bundle: refused, never silently downgraded.
        assert_eq!(owner_sign_v1(&fx.bundle, &pinned, &fx.owner).unwrap_err(), BundleRefusalV1::PinWithoutProof);
        // A proof for another block than the one pinned.
        let mut other_pin = pinned.clone();
        other_pin.pin = Some(h(0x42));
        assert!(matches!(owner_sign_v1(&with_proof, &other_pin, &fx.owner), Err(BundleRefusalV1::PinMismatch { .. })));
        // A bond registered to ANOTHER key than the signer's: the proof contradicts the bundle's claimed key.
        let (header2, proof2) = committed_bonds(vec![(bond, bond_record(TestKey::new(9).public_key()))]);
        let mut lying = fx.bundle.clone();
        lying.owner_proof = Some(proof2);
        let mut p2 = fx.policy.clone();
        p2.pin = Some(header2.hash);
        assert!(matches!(owner_sign_v1(&lying, &p2, &fx.owner), Err(BundleRefusalV1::OwnerProof(_))));
        // The bond is not in the pinned state at all: proven absent is a refusal (pin a newer block).
        let (header3, proof3) = committed_bonds(vec![]);
        let mut absent = fx.bundle.clone();
        absent.owner_proof = Some(proof3);
        let mut p3 = fx.policy.clone();
        p3.pin = Some(header3.hash);
        assert!(matches!(owner_sign_v1(&absent, &p3, &fx.owner), Err(BundleRefusalV1::OwnerProof(_))));
        // A tampered row set (a node swapping the key) fails the commitment.
        let mut forged = with_proof.clone();
        if let Some(pf) = forged.owner_proof.as_mut() {
            let n = pf.rows[0].1.len();
            pf.rows[0].1.replace_range(n - 4..n, "ffff");
        }
        assert!(matches!(owner_sign_v1(&forged, &pinned, &fx.owner), Err(BundleRefusalV1::OwnerProof(_))));
    }
}
