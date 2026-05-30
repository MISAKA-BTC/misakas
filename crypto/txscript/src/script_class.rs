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
/// In kaspa-pq, [`ScriptClass::PubKeyHashMlDsa65`] is the **only** standard
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
    PubKeyHashMlDsa65,
}

const NON_STANDARD: &str = "nonstandard";
const PUB_KEY: &str = "pubkey";
const PUB_KEY_ECDSA: &str = "pubkeyecdsa";
const SCRIPT_HASH: &str = "scripthash";
const PUB_KEY_HASH_MLDSA65: &str = "pubkeyhashmldsa65";

impl ScriptClass {
    pub fn from_script(script_public_key: &ScriptPublicKey) -> Self {
        let script_public_key_ = script_public_key.script();
        if script_public_key.version() == MAX_SCRIPT_PUBLIC_KEY_VERSION {
            if Self::is_pay_to_pub_key_hash_mldsa65(script_public_key_) {
                ScriptClass::PubKeyHashMlDsa65
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
    /// `[OpDup, OpBlake2b512, OpData64, <64-byte pubkey hash>, OpEqualVerify, OpCheckSigMlDsa65]`
    /// — total 69 bytes (5 opcodes + 64 data).
    #[inline(always)]
    pub fn is_pay_to_pub_key_hash_mldsa65(script_public_key: &[u8]) -> bool {
        (script_public_key.len() == 69)
            && (script_public_key[0] == opcodes::codes::OpDup)
            && (script_public_key[1] == opcodes::codes::OpBlake2b512)
            && (script_public_key[2] == opcodes::codes::OpData64)
            && (script_public_key[67] == opcodes::codes::OpEqualVerify)
            && (script_public_key[68] == opcodes::codes::OpCheckSigMlDsa65)
    }

    /// kaspa-pq PQ-only (ADR-0019 §7 / docs/kaspa-pq-design-mldsa87.md): the sole
    /// standard and consensus-allowed script class on a PQ-active network is
    /// ML-DSA P2PKH. Used by mempool standardness and by consensus output-class
    /// enforcement (`check_transaction_pq_output_classes`) to reject every
    /// legacy (secp256k1 / P2SH) class.
    pub fn is_pq_standard(&self) -> bool {
        matches!(self, ScriptClass::PubKeyHashMlDsa65)
    }

    fn as_str(&self) -> &'static str {
        match self {
            ScriptClass::NonStandard => NON_STANDARD,
            ScriptClass::PubKey => PUB_KEY,
            ScriptClass::PubKeyECDSA => PUB_KEY_ECDSA,
            ScriptClass::ScriptHash => SCRIPT_HASH,
            ScriptClass::PubKeyHashMlDsa65 => PUB_KEY_HASH_MLDSA65,
        }
    }

    pub fn version(&self) -> ScriptPublicKeyVersion {
        match self {
            ScriptClass::NonStandard => 0,
            ScriptClass::PubKey => MAX_SCRIPT_PUBLIC_KEY_VERSION,
            ScriptClass::PubKeyECDSA => MAX_SCRIPT_PUBLIC_KEY_VERSION,
            ScriptClass::ScriptHash => MAX_SCRIPT_PUBLIC_KEY_VERSION,
            ScriptClass::PubKeyHashMlDsa65 => MAX_SCRIPT_PUBLIC_KEY_VERSION,
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
            PUB_KEY_HASH_MLDSA65 => Ok(ScriptClass::PubKeyHashMlDsa65),
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
            Version::PubKeyHashMlDsa65 => ScriptClass::PubKeyHashMlDsa65,
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
