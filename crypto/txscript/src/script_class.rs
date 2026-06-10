use crate::{MAX_SCRIPT_PUBLIC_KEY_VERSION, opcodes};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Version;
use kaspa_consensus_core::tx::{ScriptPublicKey, ScriptPublicKeyVersion};
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};
use thiserror::Error;

#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum Error {
    #[error("Invalid script class {0}")]
    InvalidScriptClass(String),
}

/// Standard classes of script payment in the blockDAG.
///
/// In kaspa-pq, [`ScriptClass::PubKeyHashMlDsa87`] is the **only** standard
/// send template. The legacy upstream `PubKey` / `PubKeyECDSA` / `ScriptHash`
/// variants are retained for parser completeness (and for borsh
/// discriminant stability), but the wallet and mempool will not emit them
/// as standard sends. See docs/adr/0002-mldsa65-p2pkh.md.
#[derive(PartialEq, Eq, Hash, Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ScriptClass {
    /// None of the recognized forms
    NonStandard = 0,
    /// Pay to pubkey (32-byte Schnorr — kaspa-pq non-standard)
    PubKey,
    /// Pay to pubkey ECDSA (33-byte ECDSA — kaspa-pq non-standard)
    PubKeyECDSA,
    /// Pay to script hash (32-byte BLAKE2b-256 of redeem script)
    ScriptHash,
    /// kaspa-pq pay to ML-DSA public-key hash (64-byte BLAKE2b-512 of
    /// the ML-DSA public key — ADR-0019 §8, widened from the former
    /// 32-byte BLAKE2b-256). The only kaspa-pq standard send template.
    PubKeyHashMlDsa87,
    /// kaspa-pq EVM Lane v0.4 §9.2 (ADR-0020): the `EVM_DEPOSIT_LOCK` output.
    /// Layout (108 bytes):
    /// `[OpNop, OpData36, <evm_address(20) ‖ timeout_daa(8 LE) ‖ claim_tip(8 LE)>,
    ///   OpDrop, <69-byte ML-DSA P2PKH refund script>]`.
    /// The lock data prefix is a push-and-drop (script-engine no-op), so
    /// SPENDING the output simply satisfies the embedded refund P2PKH — no new
    /// VM semantics. Two ways out of the UTXO set: (a) a `DepositClaim` system
    /// op consumes it via the accepting chain block's diff (consensus-side, no
    /// script run), valid while `pov_daa < timeout`; (b) the refund key spends
    /// it normally once `pov_daa ≥ timeout` (the timeout is a tx-validation
    /// context rule keyed off this class — AC-2 exclusivity, no overlap).
    /// NOT a standard send class (wallets only build it deliberately), but
    /// consensus-allowed as an output (PQ-safe: the only spend path is ML-DSA).
    EvmDepositLock,
}

const NON_STANDARD: &str = "nonstandard";
const PUB_KEY: &str = "pubkey";
const PUB_KEY_ECDSA: &str = "pubkeyecdsa";
const SCRIPT_HASH: &str = "scripthash";
const PUB_KEY_HASH_MLDSA87: &str = "pubkeyhashmldsa87";
const EVM_DEPOSIT_LOCK: &str = "evmdepositlock";

impl ScriptClass {
    pub fn from_script(script_public_key: &ScriptPublicKey) -> Self {
        let script_public_key_ = script_public_key.script();
        if script_public_key.version() == MAX_SCRIPT_PUBLIC_KEY_VERSION {
            if Self::is_pay_to_pub_key_hash_mldsa87(script_public_key_) {
                ScriptClass::PubKeyHashMlDsa87
            } else if Self::is_evm_deposit_lock(script_public_key_) {
                ScriptClass::EvmDepositLock
            } else if Self::is_pay_to_pubkey(script_public_key_) {
                ScriptClass::PubKey
            } else if Self::is_pay_to_pubkey_ecdsa(script_public_key_) {
                Self::PubKeyECDSA
            } else if Self::is_pay_to_script_hash(script_public_key_) {
                Self::ScriptHash
            } else {
                ScriptClass::NonStandard
            }
        } else {
            ScriptClass::NonStandard
        }
    }

    // Returns true if the script passed is a pay-to-pubkey
    // transaction, false otherwise.
    #[inline(always)]
    pub fn is_pay_to_pubkey(script_public_key: &[u8]) -> bool {
        (script_public_key.len() == 34) && // 2 opcodes number + 32 data
        (script_public_key[0] == opcodes::codes::OpData32) &&
        (script_public_key[33] == opcodes::codes::OpCheckSig)
    }

    // Returns returns true if the script passed is an ECDSA pay-to-pubkey
    /// transaction, false otherwise.
    #[inline(always)]
    pub fn is_pay_to_pubkey_ecdsa(script_public_key: &[u8]) -> bool {
        (script_public_key.len() == 35) && // 2 opcodes number + 33 data
        (script_public_key[0] == opcodes::codes::OpData33) &&
        (script_public_key[34] == opcodes::codes::OpCheckSigECDSA)
    }

    /// Returns true if the script is in the standard
    /// pay-to-script-hash (P2SH) format, false otherwise.
    #[inline(always)]
    pub fn is_pay_to_script_hash(script_public_key: &[u8]) -> bool {
        (script_public_key.len() == 35) && // 3 opcodes number + 32 data
        (script_public_key[0] == opcodes::codes::OpBlake2b) &&
        (script_public_key[1] == opcodes::codes::OpData32) &&
        (script_public_key[34] == opcodes::codes::OpEqual)
    }

    /// Returns true if the script is in the kaspa-pq standard
    /// ML-DSA P2PKH format, false otherwise. The byte layout is
    /// (ADR-0019 §8 — widened from the former 32-byte BLAKE2b-256 form):
    /// `[OpDup, OpBlake2b512, OpData64, <64-byte pubkey hash>, OpEqualVerify, OpCheckSigMlDsa87]`
    /// — total 69 bytes (5 opcodes + 64 data).
    #[inline(always)]
    pub fn is_pay_to_pub_key_hash_mldsa87(script_public_key: &[u8]) -> bool {
        (script_public_key.len() == 69)
            && (script_public_key[0] == opcodes::codes::OpDup)
            && (script_public_key[1] == opcodes::codes::OpBlake2b512)
            && (script_public_key[2] == opcodes::codes::OpData64)
            && (script_public_key[67] == opcodes::codes::OpEqualVerify)
            && (script_public_key[68] == opcodes::codes::OpCheckSigMlDsa87)
    }

    /// Returns true if the script is the kaspa-pq EVM deposit-lock format
    /// (see [`ScriptClass::EvmDepositLock`]). 108 bytes total: a 36-byte
    /// push-and-drop lock-data prefix followed by a verbatim ML-DSA P2PKH
    /// refund script.
    #[inline(always)]
    pub fn is_evm_deposit_lock(script_public_key: &[u8]) -> bool {
        (script_public_key.len() == 108)
            && (script_public_key[0] == opcodes::codes::OpNop)
            && (script_public_key[1] == opcodes::codes::OpData36)
            && (script_public_key[38] == opcodes::codes::OpDrop)
            && Self::is_pay_to_pub_key_hash_mldsa87(&script_public_key[39..])
    }

    /// kaspa-pq PQ-only (ADR-0019 §7 / docs/kaspa-pq-design-mldsa87.md): the sole
    /// standard and consensus-allowed script class on a PQ-active network is
    /// ML-DSA P2PKH. Used by mempool standardness and by consensus output-class
    /// enforcement (`check_transaction_pq_output_classes`) to reject every
    /// legacy (secp256k1 / P2SH) class.
    pub fn is_pq_standard(&self) -> bool {
        matches!(self, ScriptClass::PubKeyHashMlDsa87)
    }

    fn as_str(&self) -> &'static str {
        match self {
            ScriptClass::NonStandard => NON_STANDARD,
            ScriptClass::PubKey => PUB_KEY,
            ScriptClass::PubKeyECDSA => PUB_KEY_ECDSA,
            ScriptClass::ScriptHash => SCRIPT_HASH,
            ScriptClass::PubKeyHashMlDsa87 => PUB_KEY_HASH_MLDSA87,
            ScriptClass::EvmDepositLock => EVM_DEPOSIT_LOCK,
        }
    }

    pub fn version(&self) -> ScriptPublicKeyVersion {
        match self {
            ScriptClass::NonStandard => 0,
            ScriptClass::PubKey => MAX_SCRIPT_PUBLIC_KEY_VERSION,
            ScriptClass::PubKeyECDSA => MAX_SCRIPT_PUBLIC_KEY_VERSION,
            ScriptClass::ScriptHash => MAX_SCRIPT_PUBLIC_KEY_VERSION,
            ScriptClass::PubKeyHashMlDsa87 => MAX_SCRIPT_PUBLIC_KEY_VERSION,
            ScriptClass::EvmDepositLock => MAX_SCRIPT_PUBLIC_KEY_VERSION,
        }
    }
}

impl Display for ScriptClass {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ScriptClass {
    type Err = Error;

    fn from_str(script_class: &str) -> Result<Self, Self::Err> {
        match script_class {
            NON_STANDARD => Ok(ScriptClass::NonStandard),
            PUB_KEY => Ok(ScriptClass::PubKey),
            PUB_KEY_ECDSA => Ok(ScriptClass::PubKeyECDSA),
            SCRIPT_HASH => Ok(ScriptClass::ScriptHash),
            PUB_KEY_HASH_MLDSA87 => Ok(ScriptClass::PubKeyHashMlDsa87),
            EVM_DEPOSIT_LOCK => Ok(ScriptClass::EvmDepositLock),
            _ => Err(Error::InvalidScriptClass(script_class.to_string())),
        }
    }
}

impl TryFrom<&str> for ScriptClass {
    type Error = Error;

    fn try_from(script_class: &str) -> Result<Self, Self::Error> {
        script_class.parse()
    }
}

impl From<Version> for ScriptClass {
    fn from(value: Version) -> Self {
        match value {
            Version::PubKey => ScriptClass::PubKey,
            Version::PubKeyECDSA => ScriptClass::PubKeyECDSA,
            Version::PubKeyHashMlDsa87 => ScriptClass::PubKeyHashMlDsa87,
            Version::ScriptHash => ScriptClass::ScriptHash,
        }
    }
}

#[cfg(test)]
mod tests {
    use kaspa_consensus_core::tx::ScriptVec;
    use kaspa_utils::hex::FromHex;

    use super::*;

    #[test]
    fn test_script_class_from_script() {
        struct Test {
            name: &'static str,
            script: Vec<u8>,
            version: ScriptPublicKeyVersion,
            class: ScriptClass,
        }

        // cspell:disable
        let tests = vec![
            Test {
                name: "valid pubkey script",
                script: Vec::from_hex("204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ac").unwrap(),
                version: 0,
                class: ScriptClass::PubKey,
            },
            Test {
                name: "valid pubkey ecdsa script",
                script: Vec::from_hex("21fd4a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ab").unwrap(),
                version: 0,
                class: ScriptClass::PubKeyECDSA,
            },
            Test {
                name: "valid scripthash script",
                script: Vec::from_hex("aa204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f815187").unwrap(),
                version: 0,
                class: ScriptClass::ScriptHash,
            },
            Test {
                name: "non standard script (unexpected version)",
                script: Vec::from_hex("204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ac").unwrap(),
                version: MAX_SCRIPT_PUBLIC_KEY_VERSION + 1,
                class: ScriptClass::NonStandard,
            },
            Test {
                name: "non standard script (unexpected key len)",
                script: Vec::from_hex("1f4a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f81ac").unwrap(),
                version: 0,
                class: ScriptClass::NonStandard,
            },
            Test {
                name: "non standard script (unexpected final check sig op)",
                script: Vec::from_hex("204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ad").unwrap(),
                version: 0,
                class: ScriptClass::NonStandard,
            },
        ];
        // cspell:enable

        for test in tests {
            let script_public_key = ScriptPublicKey::new(test.version, ScriptVec::from_iter(test.script.iter().copied()));
            assert_eq!(test.class, ScriptClass::from_script(&script_public_key), "{} wrong script class", test.name);
        }
    }
}

/// The decoded fields of an [`ScriptClass::EvmDepositLock`] output script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvmDepositLockFields {
    /// The EVM account credited on claim (20 bytes).
    pub evm_address: [u8; 20],
    /// First DAA score at which the REFUND path opens (claim valid strictly
    /// below it — AC-2 exclusivity). `u64::MAX` = never refundable.
    pub timeout_daa_score: u64,
    /// The AH-1 claim-inclusion tip (sompi), paid to the accepting coinbase.
    pub claim_tip_sompi: u64,
    /// The embedded ML-DSA P2PKH refund script (verbatim 69-byte suffix).
    pub refund_script_public_key: ScriptPublicKey,
}

/// Build an `EVM_DEPOSIT_LOCK` ScriptPublicKey (see [`ScriptClass::EvmDepositLock`]).
/// `refund_script` MUST be a standard 69-byte ML-DSA P2PKH script.
pub fn evm_deposit_lock_script(
    evm_address: [u8; 20],
    timeout_daa_score: u64,
    claim_tip_sompi: u64,
    refund_script: &[u8],
) -> ScriptPublicKey {
    debug_assert!(ScriptClass::is_pay_to_pub_key_hash_mldsa87(refund_script));
    let mut script = Vec::with_capacity(108);
    script.push(opcodes::codes::OpNop);
    script.push(opcodes::codes::OpData36);
    script.extend_from_slice(&evm_address);
    script.extend_from_slice(&timeout_daa_score.to_le_bytes());
    script.extend_from_slice(&claim_tip_sompi.to_le_bytes());
    script.push(opcodes::codes::OpDrop);
    script.extend_from_slice(refund_script);
    ScriptPublicKey::from_vec(MAX_SCRIPT_PUBLIC_KEY_VERSION, script)
}

/// Parse an `EVM_DEPOSIT_LOCK` ScriptPublicKey. `None` if the script is not
/// exactly the lock format.
pub fn parse_evm_deposit_lock(spk: &ScriptPublicKey) -> Option<EvmDepositLockFields> {
    if ScriptClass::from_script(spk) != ScriptClass::EvmDepositLock {
        return None;
    }
    let script = spk.script();
    let mut evm_address = [0u8; 20];
    evm_address.copy_from_slice(&script[2..22]);
    let timeout_daa_score = u64::from_le_bytes(script[22..30].try_into().unwrap());
    let claim_tip_sompi = u64::from_le_bytes(script[30..38].try_into().unwrap());
    let refund_script_public_key = ScriptPublicKey::from_vec(MAX_SCRIPT_PUBLIC_KEY_VERSION, script[39..].to_vec());
    Some(EvmDepositLockFields { evm_address, timeout_daa_score, claim_tip_sompi, refund_script_public_key })
}

#[cfg(test)]
mod evm_deposit_lock_tests {
    use super::*;

    fn refund_script() -> Vec<u8> {
        // [OpDup, OpBlake2b512, OpData64, <64B>, OpEqualVerify, OpCheckSigMlDsa87]
        let mut s = vec![opcodes::codes::OpDup, opcodes::codes::OpBlake2b512, opcodes::codes::OpData64];
        s.extend_from_slice(&[0x42u8; 64]);
        s.push(opcodes::codes::OpEqualVerify);
        s.push(opcodes::codes::OpCheckSigMlDsa87);
        s
    }

    #[test]
    fn evm_deposit_lock_build_parse_roundtrip_and_class() {
        let refund = refund_script();
        let spk = evm_deposit_lock_script([0xAB; 20], 12_345, 7, &refund);
        assert_eq!(spk.script().len(), 108);
        assert_eq!(ScriptClass::from_script(&spk), ScriptClass::EvmDepositLock);
        assert!(!ScriptClass::from_script(&spk).is_pq_standard(), "not a standard SEND class");
        let fields = parse_evm_deposit_lock(&spk).unwrap();
        assert_eq!(fields.evm_address, [0xAB; 20]);
        assert_eq!(fields.timeout_daa_score, 12_345);
        assert_eq!(fields.claim_tip_sompi, 7);
        assert_eq!(fields.refund_script_public_key.script(), refund.as_slice());
        assert_eq!(ScriptClass::from_script(&fields.refund_script_public_key), ScriptClass::PubKeyHashMlDsa87);
        // String round-trip.
        assert_eq!("evmdepositlock".parse::<ScriptClass>().unwrap(), ScriptClass::EvmDepositLock);
        // A 108-byte script that is NOT the lock shape stays non-standard.
        let mut junk = spk.script().to_vec();
        junk[0] = opcodes::codes::OpDup;
        let junk_spk = ScriptPublicKey::from_vec(MAX_SCRIPT_PUBLIC_KEY_VERSION, junk);
        assert_eq!(ScriptClass::from_script(&junk_spk), ScriptClass::NonStandard);
    }
}
