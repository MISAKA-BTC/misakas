//! **The receipt** (ADR-0108 Decision 4): evidence of one reproduction, signed by whoever ran it,
//! that no consensus path reads and no count of which admits anything. SA-3: it names the ruleset
//! it was made on, and `verify_receipt_v1` refuses to call a receipt made on another one a receipt
//! for this one. SA-4: the signature context is this crate's own and is not in the chain's context
//! set. SA-5: nothing here holds a set of receipts — `verify_receipt_v1` takes one.

use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::ValidatorKey;
use kaspa_txscript::{MLDSA87_PK_LEN, MLDSA87_SIG_LEN, verify_mldsa87_with_context};
use serde::{Deserialize, Serialize};

use crate::manifest::{PalwExtensionError, canonical_json, check_hex, keyed_id_v1};
use crate::report::{PalwExtensionDepthV1, PalwExtensionReportV1};

/// The literal every receipt opens with.
pub const PALW_EXTENSION_RECEIPT_V1: &str = "misaka-palw/extension-receipt/v1";
/// `receipt_id = BLAKE2b-512(key = this, len ‖ canonical bytes without the signer fields)`.
pub const PALW_EXTENSION_RECEIPT_ID_DOMAIN_V1: &[u8] = b"misaka-palw/extension-receipt/v1/id";
/// **SA-4: the receipt's own ML-DSA-87 context**, used for receipts and nothing else, and
/// deliberately NOT a member of the chain's `signature_contexts_root` (Decision 4): a receipt signs
/// nothing the chain verifies, and adding a member to that set moves the fingerprint.
pub const PALW_EXTENSION_RECEIPT_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/extension-receipt/v1";
/// The most bytes a receipt may be, read by `verify_receipt_v1` before it is parsed.
pub const PALW_EXTENSION_RECEIPT_MAX_BYTES: usize = 4 << 20;

/// Who made the receipt, in the only terms that matter to a reader: which code, which ruleset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwExtensionReceiptVerifierV1 {
    pub crate_version: String,
    /// 64 hex — `consensus_params_id` of the ruleset the report was made on (SA-3).
    pub ruleset_id: String,
    pub network: String,
}

/// The receipt (Decision 4). `signer_pubkey_hex` and `signature_hex` are outside the id and the
/// signed bytes; everything else is inside both.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwExtensionReceiptV1 {
    pub receipt: String,
    /// 128 hex — the manifest's id, repeated from the report so a reader can match a receipt to
    /// a manifest without opening the report.
    pub extension_id: String,
    pub report: PalwExtensionReportV1,
    pub verifier: PalwExtensionReceiptVerifierV1,
    /// A timestamp the verifier chose. Evidence of when, not an input to anything.
    pub issued_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer_pubkey_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_hex: Option<String>,
}

impl PalwExtensionReceiptV1 {
    /// A receipt over one report, unsigned.
    pub fn issue(report: PalwExtensionReportV1, issued_at_unix_ms: u64) -> Self {
        Self {
            receipt: PALW_EXTENSION_RECEIPT_V1.to_string(),
            extension_id: report.extension_id.clone(),
            verifier: PalwExtensionReceiptVerifierV1 {
                crate_version: env!("CARGO_PKG_VERSION").to_string(),
                ruleset_id: report.ruleset_id_this_build.clone(),
                network: report.network.clone(),
            },
            report,
            issued_at_unix_ms,
            signer_pubkey_hex: None,
            signature_hex: None,
        }
    }

    /// The canonical bytes with the signer fields absent — the preimage of the id and of the
    /// signature.
    pub fn unsigned_canonical_bytes(&self) -> Result<Vec<u8>, PalwExtensionError> {
        let unsigned = Self { signer_pubkey_hex: None, signature_hex: None, ..self.clone() };
        let raw =
            serde_json::to_vec(&unsigned).map_err(|e| PalwExtensionError::Internal(format!("the receipt does not serialize: {e}")))?;
        canonical_json(&raw)
    }

    pub fn receipt_id(&self) -> Result<Hash64, PalwExtensionError> {
        Ok(receipt_id_v1(&self.unsigned_canonical_bytes()?))
    }

    /// The whole receipt, signer fields included, as canonical JSON — what is written to a file.
    pub fn canonical_json(&self) -> Result<Vec<u8>, PalwExtensionError> {
        let raw =
            serde_json::to_vec(self).map_err(|e| PalwExtensionError::Internal(format!("the receipt does not serialize: {e}")))?;
        canonical_json(&raw)
    }

    pub fn is_signed(&self) -> bool {
        self.signature_hex.is_some()
    }
}

/// `receipt_id_v1`: the keyed digest over the unsigned canonical bytes.
pub fn receipt_id_v1(unsigned_canonical_bytes: &[u8]) -> Hash64 {
    keyed_id_v1(PALW_EXTENSION_RECEIPT_ID_DOMAIN_V1, unsigned_canonical_bytes)
}

/// Sign a receipt with the verifier's own key under [`PALW_EXTENSION_RECEIPT_MLDSA87_CONTEXT`]:
/// the signature is over the unsigned canonical bytes, and the signer's public key rides beside
/// it so a stranger can verify without knowing who the signer is.
pub fn sign_receipt_v1(receipt: &mut PalwExtensionReceiptV1, key: &ValidatorKey) -> Result<(), PalwExtensionError> {
    let message = receipt.unsigned_canonical_bytes()?;
    let signature = key.sign_with_context(&message, PALW_EXTENSION_RECEIPT_MLDSA87_CONTEXT);
    receipt.signer_pubkey_hex = Some(faster_hex::hex_string(key.public_key()));
    receipt.signature_hex = Some(faster_hex::hex_string(&signature));
    Ok(())
}

/// What `verify_receipt_v1` says about one receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwReceiptVerdictV1 {
    pub receipt_id: String,
    pub extension_id: String,
    /// `true` only when a signature was present AND verified under the receipt context.
    pub signed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer_pubkey_hex: Option<String>,
    pub ruleset_id: String,
    pub network: String,
    pub tier: String,
    pub depth_reached: PalwExtensionDepthV1,
    pub issued_at_unix_ms: u64,
    /// One sentence a reader sees first: signed by whom, or unsigned and what that means.
    pub note: String,
}

/// **`verify_receipt_v1`**: canonical form, the id, the signature (if any) under the receipt
/// context, and SA-3 — a receipt made on another ruleset is refused naming `verifier.ruleset_id`.
/// Takes ONE receipt (SA-5).
pub fn verify_receipt_v1(receipt_json: &[u8], expected_ruleset_id: Option<&str>) -> Result<PalwReceiptVerdictV1, PalwExtensionError> {
    if receipt_json.len() > PALW_EXTENSION_RECEIPT_MAX_BYTES {
        return Err(PalwExtensionError::field(
            "receipt",
            format!("{} bytes, over the {PALW_EXTENSION_RECEIPT_MAX_BYTES}-byte bound", receipt_json.len()),
        ));
    }
    let canonical = canonical_json(receipt_json).map_err(|e| match e {
        PalwExtensionError::Field { reason, .. } => PalwExtensionError::field("receipt", reason),
        other => other,
    })?;
    let receipt: PalwExtensionReceiptV1 =
        serde_json::from_slice(&canonical).map_err(|e| PalwExtensionError::field("receipt", format!("not a receipt: {e}")))?;
    if receipt.receipt != PALW_EXTENSION_RECEIPT_V1 {
        return Err(PalwExtensionError::field("receipt", format!("`{}` is not `{PALW_EXTENSION_RECEIPT_V1}`", receipt.receipt)));
    }
    check_hex("extension_id", &receipt.extension_id, 128)?;
    if receipt.extension_id != receipt.report.extension_id {
        return Err(PalwExtensionError::field(
            "extension_id",
            format!("the receipt says {} and its report says {}", receipt.extension_id, receipt.report.extension_id),
        ));
    }
    check_hex("verifier.ruleset_id", &receipt.verifier.ruleset_id, 64)?;
    if receipt.verifier.ruleset_id != receipt.report.ruleset_id_this_build {
        return Err(PalwExtensionError::field(
            "verifier.ruleset_id",
            format!(
                "the receipt names {} and its report was made on {}",
                receipt.verifier.ruleset_id, receipt.report.ruleset_id_this_build
            ),
        ));
    }
    // SA-3 before the signature: a perfectly signed receipt from a devnet is not evidence about
    // testnet-11, and saying "signature ok" first would invite reading it as one.
    if let Some(expected) = expected_ruleset_id
        && receipt.verifier.ruleset_id != expected
    {
        return Err(PalwExtensionError::field(
            "verifier.ruleset_id",
            format!(
                "the receipt was made on ruleset {} and this is ruleset {expected} — a receipt from another ruleset is not a receipt for this one (SA-3)",
                receipt.verifier.ruleset_id
            ),
        ));
    }
    let unsigned = receipt.unsigned_canonical_bytes()?;
    let receipt_id = receipt_id_v1(&unsigned);
    let (signed, note) = match (&receipt.signer_pubkey_hex, &receipt.signature_hex) {
        (None, None) => (false, "unsigned: the report is whoever handed you this file says it is — nobody vouches for it".to_string()),
        (Some(_), None) => return Err(PalwExtensionError::field("signature_hex", "a signer public key without a signature")),
        (None, Some(_)) => return Err(PalwExtensionError::field("signer_pubkey_hex", "a signature without the signer's public key")),
        (Some(pubkey_hex), Some(signature_hex)) => {
            check_hex("signer_pubkey_hex", pubkey_hex, MLDSA87_PK_LEN * 2)?;
            check_hex("signature_hex", signature_hex, MLDSA87_SIG_LEN * 2)?;
            let mut pubkey = vec![0u8; MLDSA87_PK_LEN];
            faster_hex::hex_decode(pubkey_hex.as_bytes(), &mut pubkey)
                .map_err(|e| PalwExtensionError::field("signer_pubkey_hex", e.to_string()))?;
            let mut signature = vec![0u8; MLDSA87_SIG_LEN];
            faster_hex::hex_decode(signature_hex.as_bytes(), &mut signature)
                .map_err(|e| PalwExtensionError::field("signature_hex", e.to_string()))?;
            match verify_mldsa87_with_context(&pubkey, &unsigned, &signature, PALW_EXTENSION_RECEIPT_MLDSA87_CONTEXT) {
                Ok(true) => (
                    true,
                    format!(
                        "signed under {} by {}…",
                        String::from_utf8_lossy(PALW_EXTENSION_RECEIPT_MLDSA87_CONTEXT),
                        &pubkey_hex[..16]
                    ),
                ),
                Ok(false) => {
                    return Err(PalwExtensionError::field(
                        "signature_hex",
                        "does not verify over the receipt's canonical bytes under the receipt context — the report changed after signing, or this was signed as something else (SA-4)",
                    ));
                }
                Err(e) => return Err(PalwExtensionError::field("signature_hex", format!("{e}"))),
            }
        }
    };
    Ok(PalwReceiptVerdictV1 {
        receipt_id: receipt_id.to_string(),
        extension_id: receipt.extension_id.clone(),
        signed,
        signer_pubkey_hex: receipt.signer_pubkey_hex.clone(),
        ruleset_id: receipt.verifier.ruleset_id.clone(),
        network: receipt.verifier.network.clone(),
        tier: receipt.report.classification.tier().to_string(),
        depth_reached: receipt.report.depth_reached,
        issued_at_unix_ms: receipt.issued_at_unix_ms,
        note,
    })
}
