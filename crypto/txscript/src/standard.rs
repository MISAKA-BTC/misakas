use crate::{
    opcodes::codes::{
        OpBlake2b, OpBlake2b512, OpCheckSig, OpCheckSigECDSA, OpCheckSigMlDsa87, OpData32, OpData33, OpData64, OpDup, OpEqual,
        OpEqualVerify,
    },
    script_builder::{ScriptBuilder, ScriptBuilderResult},
    script_class::ScriptClass,
};
use blake2b_simd::Params;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::tx::{ScriptPublicKey, ScriptVec};
use kaspa_txscript_errors::TxScriptError;
use smallvec::SmallVec;
use std::iter::once;

mod multisig;

pub use multisig::{
    Error as MultisigCreateError, multisig_redeem_script, multisig_redeem_script_ecdsa, multisig_redeem_script_mldsa87,
};

/// Creates a new script to pay a transaction output to a 32-byte pubkey.
fn pay_to_pub_key(address_payload: &[u8]) -> ScriptVec {
    // TODO: use ScriptBuilder when add_op and add_data fns or equivalents are available
    assert_eq!(address_payload.len(), 32);
    SmallVec::from_iter(once(OpData32).chain(address_payload.iter().copied()).chain(once(OpCheckSig)))
}

/// Creates a new script to pay a transaction output to a 33-byte ECDSA pubkey.
fn pay_to_pub_key_ecdsa(address_payload: &[u8]) -> ScriptVec {
    // TODO: use ScriptBuilder when add_op and add_data fns or equivalents are available
    assert_eq!(address_payload.len(), 33);
    SmallVec::from_iter(once(OpData33).chain(address_payload.iter().copied()).chain(once(OpCheckSigECDSA)))
}

/// Creates a new script to pay a transaction output to a script hash.
/// It is expected that the input is a valid hash.
fn pay_to_script_hash(script_hash: &[u8]) -> ScriptVec {
    // TODO: use ScriptBuilder when add_op and add_data fns or equivalents are available
    assert_eq!(script_hash.len(), 32);
    SmallVec::from_iter([OpBlake2b, OpData32].iter().copied().chain(script_hash.iter().copied()).chain(once(OpEqual)))
}

/// Creates a new kaspa-pq ML-DSA P2PKH `scriptPubKey`.
///
/// The script template is (ADR-0019 §8 — widened from the former 32-byte
/// BLAKE2b-256 form):
/// ```text
///   OP_DUP
///   OP_BLAKE2B_512
///   OP_DATA64 <BLAKE2b-512(ML-DSA public key)>
///   OP_EQUALVERIFY
///   OP_CHECKSIG_MLDSA87
/// ```
///
/// Total length 69 bytes (5 opcodes + 64-byte payload). See
/// docs/adr/0002-mldsa65-p2pkh.md and docs/kaspa-pq-design-mldsa87.md §8.
fn pay_to_pub_key_hash_mldsa87(address_payload: &[u8]) -> ScriptVec {
    // TODO: use ScriptBuilder when add_op and add_data fns or equivalents are available
    assert_eq!(address_payload.len(), 64);
    SmallVec::from_iter(
        [OpDup, OpBlake2b512, OpData64]
            .iter()
            .copied()
            .chain(address_payload.iter().copied())
            .chain([OpEqualVerify, OpCheckSigMlDsa87].iter().copied()),
    )
}

/// Creates a new script to pay a transaction output to the specified address.
pub fn pay_to_address_script(address: &Address) -> ScriptPublicKey {
    let script = match address.version {
        Version::PubKey => pay_to_pub_key(address.payload.as_slice()),
        Version::PubKeyECDSA => pay_to_pub_key_ecdsa(address.payload.as_slice()),
        Version::PubKeyHashMlDsa87 => pay_to_pub_key_hash_mldsa87(address.payload.as_slice()),
        Version::ScriptHash => pay_to_script_hash(address.payload.as_slice()),
    };
    ScriptPublicKey::new(ScriptClass::from(address.version).version(), script)
}

/// kaspa-pq PQ-only (ADR-0019 §13 / docs/kaspa-pq-design-mldsa87.md §13.5): the
/// PQ-network variant of [`pay_to_address_script`]. It accepts ONLY the standard
/// ML-DSA-87 P2PKH address class ([`Version::PubKeyHashMlDsa87`]); any legacy
/// secp256k1 (`PubKey` / `PubKeyECDSA`) or `ScriptHash` address is rejected with
/// [`TxScriptError::LegacyAddressDisabledInPqMode`].
///
/// The wallet's transaction generator routes BOTH recipient and change outputs
/// through this on a PQ network, so no legacy output can ever be created by a
/// kaspa-pq wallet — the creation-side complement to the consensus output-class
/// rule (ADR-0019 §7) and the script-engine legacy-opcode rejection (§6).
pub fn pay_to_address_script_pq(address: &Address) -> Result<ScriptPublicKey, TxScriptError> {
    match address.version {
        Version::PubKeyHashMlDsa87 => Ok(pay_to_address_script(address)),
        other => Err(TxScriptError::LegacyAddressDisabledInPqMode(format!("{other:?}"))),
    }
}

/// Takes a script and returns an equivalent pay-to-script-hash script
pub fn pay_to_script_hash_script(redeem_script: &[u8]) -> ScriptPublicKey {
    let redeem_script_hash = Params::new().hash_length(32).to_state().update(redeem_script).finalize();
    let script = pay_to_script_hash(redeem_script_hash.as_bytes());
    ScriptPublicKey::new(ScriptClass::ScriptHash.version(), script)
}

/// Generates a signature script that fits a pay-to-script-hash script
pub fn pay_to_script_hash_signature_script(redeem_script: Vec<u8>, signature: Vec<u8>) -> ScriptBuilderResult<Vec<u8>> {
    let redeem_script_as_data = ScriptBuilder::new().add_data(&redeem_script)?.drain();
    Ok(Vec::from_iter(signature.iter().copied().chain(redeem_script_as_data.iter().copied())))
}

/// Returns the address encoded in a script public key.
///
/// Notes:
///  - This function only works for 'standard' transaction script types.
///    Any data such as public keys which are invalid will return the
///    `TxScriptError::PubKeyFormat` error.
///
///  - In case a ScriptClass is needed by the caller, call `ScriptClass::from(address.version)`
///    or use `address.version` directly instead, where address is the successfully
///    returned address.
pub fn extract_script_pub_key_address(script_public_key: &ScriptPublicKey, prefix: Prefix) -> Result<Address, TxScriptError> {
    let class = ScriptClass::from_script(script_public_key);
    if script_public_key.version() > class.version() {
        return Err(TxScriptError::PubKeyFormat);
    }
    let script = script_public_key.script();
    match class {
        ScriptClass::NonStandard => Err(TxScriptError::PubKeyFormat),
        ScriptClass::PubKey => Ok(Address::new(prefix, Version::PubKey, &script[1..33])),
        ScriptClass::PubKeyECDSA => Ok(Address::new(prefix, Version::PubKeyECDSA, &script[1..34])),
        // kaspa-pq ML-DSA P2PKH (ADR-0019 §8): layout is
        //   [OpDup, OpBlake2b512, OpData64, <64-byte payload>, OpEqualVerify, OpCheckSigMlDsa87]
        // so the address payload occupies script[3..67].
        ScriptClass::PubKeyHashMlDsa87 => Ok(Address::new(prefix, Version::PubKeyHashMlDsa87, &script[3..67])),
        ScriptClass::ScriptHash => Ok(Address::new(prefix, Version::ScriptHash, &script[2..34])),
        // kaspa-pq EVM Lane v0.4 §9.2: the deposit lock is not an
        // address-bearing send template (it is claimed by a system op or
        // refunded via its embedded P2PKH); no address form exists for it.
        ScriptClass::EvmDepositLock => Err(TxScriptError::PubKeyFormat),
    }
}

pub mod test_helpers {
    use super::*;
    use crate::{MAX_TX_IN_SEQUENCE_NUM, opcodes::codes::OpTrue};
    use kaspa_consensus_core::{
        constants::TX_VERSION,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput},
    };

    /// Returns a P2SH script paying to an anyone-can-spend address,
    /// The second return value is a redeemScript to be used with txscript.pay_to_script_hash_signature_script
    pub fn op_true_script() -> (ScriptPublicKey, Vec<u8>) {
        let redeem_script = vec![OpTrue];
        let script_public_key = pay_to_script_hash_script(&redeem_script);
        (script_public_key, redeem_script)
    }

    /// Creates a transaction that spends the first output of provided transaction.
    /// Assumes that the output being spent has opTrueScript as its scriptPublicKey.
    /// Creates the value of the spent output minus provided `fee` (in sompi).
    pub fn create_transaction(tx_to_spend: &Transaction, fee: u64) -> Transaction {
        let (script_public_key, redeem_script) = op_true_script();
        let signature_script = pay_to_script_hash_signature_script(redeem_script, vec![]).expect("the script is canonical");
        let previous_outpoint = TransactionOutpoint::new(tx_to_spend.id(), 0);
        let input = TransactionInput::new(previous_outpoint, signature_script, MAX_TX_IN_SEQUENCE_NUM, 1);
        let output = TransactionOutput::new(tx_to_spend.outputs[0].value - fee, script_public_key);
        Transaction::new(TX_VERSION, vec![input], vec![output], 0, SUBNETWORK_ID_NATIVE, 0, vec![])
    }

    /// Creates a transaction that spends the outputs of specified indexes (if they exist) of every provided transaction and returns an optional change.
    /// Assumes that the outputs being spent have opTrueScript as their scriptPublicKey.
    ///
    /// If some change is provided, creates two outputs, first one with the value of the spent outputs minus `change`
    /// and `fee` (in sompi) and second one of `change` amount.
    ///
    /// If no change is provided, creates only one output with the value of the spent outputs minus and `fee` (in sompi)
    pub fn create_transaction_with_change<'a>(
        txs_to_spend: impl Iterator<Item = &'a Transaction>,
        output_indexes: Vec<usize>,
        change: Option<u64>,
        fee: u64,
    ) -> Transaction {
        let (script_public_key, redeem_script) = op_true_script();
        let signature_script = pay_to_script_hash_signature_script(redeem_script, vec![]).expect("the script is canonical");
        let mut inputs_value: u64 = 0;
        let mut inputs = vec![];
        for tx_to_spend in txs_to_spend {
            for i in output_indexes.iter().copied() {
                if i < tx_to_spend.outputs.len() {
                    let previous_outpoint = TransactionOutpoint::new(tx_to_spend.id(), i as u32);
                    inputs.push(TransactionInput::new(previous_outpoint, signature_script.clone(), MAX_TX_IN_SEQUENCE_NUM, 1));
                    inputs_value += tx_to_spend.outputs[i].value;
                }
            }
        }
        let outputs = match change {
            Some(change) => vec![
                TransactionOutput::new(inputs_value - fee - change, script_public_key.clone()),
                TransactionOutput::new(change, script_public_key),
            ],
            None => vec![TransactionOutput::new(inputs_value - fee, script_public_key.clone())],
        };
        Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![])
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pay_to_address_script_pq_gates_legacy() {
        use kaspa_addresses::{Prefix, Version};
        // ML-DSA P2PKH (64-byte payload) is accepted and matches the non-gated builder.
        let ml = Address::new(Prefix::Mainnet, Version::PubKeyHashMlDsa87, &[0x11u8; 64]);
        let gated = pay_to_address_script_pq(&ml).expect("ML-DSA P2PKH is the standard PQ class");
        assert_eq!(gated, pay_to_address_script(&ml));
        assert_eq!(gated.script().len(), 69);
        // Every legacy class is rejected.
        for (ver, payload) in [
            (Version::PubKey, vec![0u8; 32]),
            (Version::PubKeyECDSA, vec![0u8; 33]),
            (Version::ScriptHash, vec![0u8; 32]),
        ] {
            let addr = Address::new(Prefix::Mainnet, ver, &payload);
            assert!(
                matches!(pay_to_address_script_pq(&addr), Err(TxScriptError::LegacyAddressDisabledInPqMode(_))),
                "version {ver:?} must be rejected on a PQ network"
            );
        }
    }

    use super::*;
    use kaspa_utils::hex::FromHex;

    #[test]
    fn test_extract_address_and_encode_script() {
        // kaspa-pq changed the address prefix family from upstream `kaspa*`
        // to `misaka*`; Phase 4 added `Version::PubKeyHashMlDsa87`. The
        // hardcoded bech32 strings below are reconstructed from the
        // (prefix, version, payload) triple at runtime so that the test
        // exercises encode/decode/extract round-tripping without
        // depending on a specific bech32 checksum value (the prefix change
        // invalidated upstream's checksums).
        struct Test {
            name: &'static str,
            script_pub_key: ScriptPublicKey,
            prefix: Prefix,
            expected_address: Result<Address, TxScriptError>,
        }

        // 32-byte payloads used for the well-formed test vectors.
        let p32_a: [u8; 32] = [
            0x7b, 0xc0, 0x41, 0x96, 0xf1, 0x12, 0x5e, 0x4f, 0x26, 0x76, 0xcd, 0x09, 0xed, 0x14, 0xaf, 0xb7, 0x72, 0x23, 0xb1, 0xf6,
            0x21, 0x77, 0xda, 0x54, 0x88, 0x34, 0x63, 0x23, 0xea, 0xa9, 0x1a, 0x69,
        ];
        let p33_b: [u8; 33] = [
            0xba, 0x01, 0xfc, 0x5f, 0x4e, 0x9d, 0x98, 0x79, 0x59, 0x9c, 0x69, 0xa3, 0xda, 0xfd, 0xb8, 0x35, 0xa7, 0x25, 0x5e, 0x5f,
            0x2e, 0x93, 0x4e, 0x93, 0x22, 0xec, 0xd3, 0xaf, 0x19, 0x0a, 0xb0, 0xf6, 0x0e,
        ];
        // ADR-0019 §8: ML-DSA P2PKH payload is a 64-byte BLAKE2b-512 hash.
        let p64_pq: [u8; 64] = [
            0x88, 0x44, 0xcc, 0x77, 0xee, 0x11, 0xaa, 0x99, 0x00, 0x33, 0xbb, 0x66, 0xdd, 0x22, 0x55, 0x44, 0x77, 0xee, 0x11, 0xaa,
            0x99, 0x88, 0x33, 0xbb, 0x66, 0xdd, 0x22, 0x55, 0x44, 0x77, 0xee, 0x11, 0x88, 0x44, 0xcc, 0x77, 0xee, 0x11, 0xaa, 0x99,
            0x00, 0x33, 0xbb, 0x66, 0xdd, 0x22, 0x55, 0x44, 0x77, 0xee, 0x11, 0xaa, 0x99, 0x88, 0x33, 0xbb, 0x66, 0xdd, 0x22, 0x55,
            0x44, 0x77, 0xee, 0x11,
        ];

        // cspell:disable
        let tests = vec![
            Test {
                name: "Mainnet PubKey script and address (legacy, parser-only)",
                script_pub_key: ScriptPublicKey::new(
                    ScriptClass::PubKey.version(),
                    ScriptVec::from_hex("207bc04196f1125e4f2676cd09ed14afb77223b1f62177da5488346323eaa91a69ac").unwrap(),
                ),
                prefix: Prefix::Mainnet,
                expected_address: Ok(Address::new(Prefix::Mainnet, Version::PubKey, &p32_a)),
            },
            Test {
                name: "Testnet PubKeyECDSA script and address (legacy, parser-only)",
                script_pub_key: ScriptPublicKey::new(
                    ScriptClass::PubKeyECDSA.version(),
                    ScriptVec::from_hex("21ba01fc5f4e9d9879599c69a3dafdb835a7255e5f2e934e9322ecd3af190ab0f60eab").unwrap(),
                ),
                prefix: Prefix::Testnet,
                expected_address: Ok(Address::new(Prefix::Testnet, Version::PubKeyECDSA, &p33_b)),
            },
            Test {
                name: "Mainnet ML-DSA P2PKH script and address (kaspa-pq standard, ADR-0019 §8)",
                script_pub_key: pay_to_address_script(&Address::new(Prefix::Mainnet, Version::PubKeyHashMlDsa87, &p64_pq)),
                prefix: Prefix::Mainnet,
                expected_address: Ok(Address::new(Prefix::Mainnet, Version::PubKeyHashMlDsa87, &p64_pq)),
            },
            Test {
                name: "Testnet non standard script",
                script_pub_key: ScriptPublicKey::new(
                    ScriptClass::PubKey.version(),
                    ScriptVec::from_hex("2001fc5f4e9d9879599c69a3dafdb835a7255e5f2e934e9322ecd3af190ab0f60eab").unwrap(),
                ),
                prefix: Prefix::Testnet,
                expected_address: Err(TxScriptError::PubKeyFormat),
            },
            Test {
                name: "Mainnet script with unknown version",
                script_pub_key: ScriptPublicKey::new(
                    ScriptClass::PubKey.version() + 1,
                    ScriptVec::from_hex("207bc04196f1125e4f2676cd09ed14afb77223b1f62177da5488346323eaa91a69ac").unwrap(),
                ),
                prefix: Prefix::Mainnet,
                expected_address: Err(TxScriptError::PubKeyFormat),
            },
        ];
        // cspell:enable

        for test in tests {
            let extracted = extract_script_pub_key_address(&test.script_pub_key, test.prefix);
            assert_eq!(extracted, test.expected_address, "extract address test failed for '{}'", test.name);
            if let Ok(ref address) = extracted {
                let encoded = pay_to_address_script(address);
                assert_eq!(encoded, test.script_pub_key, "encode public key script test failed for '{}'", test.name);
            }
        }
    }
}
