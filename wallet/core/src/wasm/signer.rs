use crate::imports::*;
use crate::result::Result;
use js_sys::Array;
use kaspa_consensus_client::{Transaction, sign_with_multiple_v3};
use kaspa_consensus_core::hashing::wasm::SighashType;
use kaspa_consensus_core::sign::sign_input;
use kaspa_consensus_core::tx::PopulatedTransaction;
use kaspa_consensus_core::{hashing::sighash_type::SIG_HASH_ALL, sign::verify};
// kaspa-pq ML-DSA-87 signer imports. `kaspa_pq_wasm` is only compiled for
// wasm32/test (it pulls wasm-bindgen), so the helper imports and the
// `signTransactionMlDsa87` fn below are gated to match — native builds exclude them.
#[cfg(any(target_arch = "wasm32", test))]
use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
use kaspa_hashes::Hash;
#[cfg(any(target_arch = "wasm32", test))]
use kaspa_txscript::script_builder::ScriptBuilder;
#[cfg(any(target_arch = "wasm32", test))]
use kaspa_wallet_keys::kaspa_pq_wasm::KaspaPqKeyPair;
use kaspa_wallet_keys::privatekey::PrivateKey;
use kaspa_wasm_core::types::HexString;
use serde_wasm_bindgen::from_value;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Array, is_type_of = Array::is_array, typescript_type = "(PrivateKey | HexString | Uint8Array)[]")]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub type PrivateKeyArrayT;
}

impl TryFrom<PrivateKeyArrayT> for Vec<PrivateKey> {
    type Error = crate::error::Error;
    fn try_from(keys: PrivateKeyArrayT) -> std::result::Result<Self, Self::Error> {
        let mut private_keys: Vec<PrivateKey> = vec![];
        for key in keys.iter() {
            private_keys
                .push(PrivateKey::try_owned_from(key).map_err(|_| Self::Error::Custom("Unable to cast PrivateKey".to_string()))?);
        }

        Ok(private_keys)
    }
}

/// `signTransaction()` is a helper function to sign a transaction using a private key array or a signer array.
/// @category Wallet SDK
#[wasm_bindgen(js_name = "signTransaction")]
pub fn js_sign_transaction(tx: &Transaction, signer: &PrivateKeyArrayT, verify_sig: bool) -> Result<Transaction> {
    if signer.is_array() {
        let mut private_keys: Vec<[u8; 32]> = vec![];
        for key in Array::from(signer).iter() {
            let key = PrivateKey::try_cast_from(&key).map_err(|_| Error::Custom("Unable to cast PrivateKey".to_string()))?;
            private_keys.push(key.as_ref().secret_bytes());
        }

        let tx = sign_transaction(tx, &private_keys, verify_sig).map_err(|err| Error::Custom(format!("Unable to sign: {err:?}")))?;
        private_keys.zeroize();
        Ok(tx.clone())
    } else {
        Err(Error::custom("signTransaction() requires an array of signatures"))
    }
}

/// `signTransactionMlDsa87()` signs every input of `tx` with a kaspa-pq
/// ML-DSA-87 keypair, producing the canonical P2PKH unlock script
/// `<signature || sighash_type> <public_key>` for each input. The signed
/// message is the 64-byte `calc_mldsa87_signature_hash(.., SIG_HASH_ALL, ..)`
/// (ADR-0019 §9) — the exact digest the `OpCheckSigMlDsa87` consensus opcode
/// recomputes and verifies under `MLDSA87_TX_CONTEXT`.
///
/// `randomness` must be 32 bytes (e.g. `crypto.getRandomValues`). Per-input
/// randomness is derived from it so distinct inputs use distinct hedging
/// randomness.
///
/// Assumes the transaction's inputs are fully UTXO-populated and that every
/// input is locked to this keypair's address (a single-key wallet).
/// @category Wallet SDK
#[cfg(any(target_arch = "wasm32", test))]
#[wasm_bindgen(js_name = "signTransactionMlDsa87")]
pub fn js_sign_transaction_mldsa87(tx: &Transaction, keypair: &KaspaPqKeyPair, randomness: Vec<u8>) -> Result<Transaction> {
    if randomness.len() != 32 {
        return Err(Error::custom("signTransactionMlDsa87() requires 32 bytes of randomness"));
    }
    let public_key = keypair.public_key().to_bytes();

    let reused_values = Mldsa87SigHashReusedValuesUnsync::new();
    let input_len = tx.inner().inputs.len();
    let (cctx, utxos) = tx.tx_and_utxos()?;
    let populated_transaction = PopulatedTransaction::new(&cctx, utxos);
    for i in 0..input_len {
        let sig_hash = calc_mldsa87_signature_hash(&populated_transaction, i, SIG_HASH_ALL, &reused_values);

        // audit L: per-input hedging randomness = root XOR the input's full 32-byte sig_hash
        // (a BLAKE2b digest committing to this input's outpoint/amounts), so all 32 bytes vary by
        // a per-input cryptographic value rather than the old 8-byte-XOR-index. ML-DSA is not
        // nonce-fragile across distinct messages (each input signs a distinct sig_hash, and
        // deterministic ML-DSA is itself secure), so this is robustness, not a correctness fix.
        let mut input_randomness = [0u8; 32];
        let sh = sig_hash.as_bytes();
        for k in 0..32 {
            input_randomness[k] = randomness[k] ^ sh[k];
        }

        let signature = keypair
            .sign(sig_hash.as_bytes().to_vec(), input_randomness.to_vec())
            .map_err(|e| Error::Custom(format!("ML-DSA-87 sign failed: {e:?}")))?;

        // OpCheckSigMlDsa87 pops [sig, key] and strips the trailing sighash-type
        // byte off the signature, mirroring schnorr OP_CHECKSIG.
        let mut sig_data = signature.to_bytes();
        sig_data.push(SIG_HASH_ALL.to_u8());

        let script = ScriptBuilder::new()
            .add_data(&sig_data)
            .map_err(|e| Error::Custom(format!("signature push: {e:?}")))?
            .add_data(&public_key)
            .map_err(|e| Error::Custom(format!("public key push: {e:?}")))?
            .drain();
        tx.set_signature_script(i, script)?;
    }
    Ok(tx.clone())
}

fn sign_transaction<'a>(tx: &'a Transaction, private_keys: &[[u8; 32]], verify_sig: bool) -> Result<&'a Transaction> {
    let tx = sign(tx, private_keys)?;
    if verify_sig {
        let (cctx, utxos) = tx.tx_and_utxos()?;
        let populated_transaction = PopulatedTransaction::new(&cctx, utxos);
        verify(&populated_transaction)?;
    }
    Ok(tx)
}

/// Sign a transaction using schnorr, returns a new transaction with the signatures added.
/// The resulting transaction may be partially signed if the supplied keys are not sufficient
/// to sign all of its inputs.
pub fn sign<'a>(tx: &'a Transaction, privkeys: &[[u8; 32]]) -> Result<&'a Transaction> {
    Ok(sign_with_multiple_v3(tx, privkeys)?.unwrap())
}

/// `createInputSignature()` is a helper function to sign a transaction input with a specific SigHash type using a private key.
/// @category Wallet SDK
#[wasm_bindgen(js_name = "createInputSignature")]
pub fn create_input_signature(
    tx: &Transaction,
    input_index: u8,
    private_key: &PrivateKey,
    sighash_type: Option<SighashType>,
) -> Result<HexString> {
    let (cctx, utxos) = tx.tx_and_utxos()?;
    let populated_transaction = PopulatedTransaction::new(&cctx, utxos);

    let signature = sign_input(
        &populated_transaction,
        input_index.into(),
        &private_key.secret_bytes(),
        sighash_type.unwrap_or(SighashType::All).into(),
    );

    Ok(signature.to_hex().into())
}

/// @category Wallet SDK
#[wasm_bindgen(js_name=signScriptHash)]
pub fn sign_script_hash(script_hash: JsValue, privkey: &PrivateKey) -> Result<String> {
    let script_hash = from_value(script_hash)?;
    let result = sign_hash(script_hash, &privkey.into())?;
    Ok(result.to_hex())
}

fn sign_hash(sig_hash: Hash, privkey: &[u8; 32]) -> Result<Vec<u8>> {
    let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice())?;
    let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, privkey)?;
    let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
    let signature = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
    Ok(signature)
}

#[cfg(test)]
mod mldsa_parity_tests {
    //! kaspa-pq (ADR-0019 §13) Phase 5e: the WASM `signTransactionMlDsa87` helper
    //! and the native `kaspa_wallet_keys::kaspa_pq::sign_transaction_inputs_mldsa87`
    //! must produce byte-identical unlock scripts for the same key + transaction +
    //! randomness, so the WASM (JS) wallet and the native Rust wallet are
    //! interoperable (a tx signed by one verifies / is indistinguishable from the
    //! other). This is reachable natively because both the WASM signer and its
    //! `KaspaPqKeyPair` dependency are `#[cfg(any(target_arch = "wasm32", test))]`.

    use super::*;
    use ahash::AHashMap;
    use kaspa_consensus_client::{Transaction as ClientTransaction, UtxoEntry as ClientUtxoEntry};
    use kaspa_consensus_core::tx::{
        SignableTransaction, Transaction as CcTransaction, TransactionId, TransactionInput, TransactionOutpoint as CcOutpoint,
        TransactionOutput, UtxoEntry as CcUtxoEntry,
    };
    use kaspa_txscript::pay_to_address_script;
    use kaspa_wallet_keys::kaspa_pq::{KaspaPqMlDsa87KeyPair, sign_transaction_inputs_mldsa87};

    #[test]
    fn wasm_and_native_mldsa_signers_agree() {
        // Same 32-byte seed => identical ML-DSA-87 keypair in both worlds.
        let seed = [0x42u8; 32];
        let native_kp = KaspaPqMlDsa87KeyPair::from_seed(seed);
        let wasm_kp = KaspaPqKeyPair::from_seed(seed.to_vec()).ok().expect("valid 32-byte seed");

        // A 1-input / 1-output spend of a UTXO locked to that key.
        let address = native_kp.address(kaspa_addresses::Prefix::Mainnet);
        let spk = pay_to_address_script(&address);
        let outpoint = CcOutpoint { transaction_id: TransactionId::from_bytes([0x09u8; 64]), index: 0 };
        let cctx = CcTransaction::new(
            0,
            vec![TransactionInput { previous_outpoint: outpoint, signature_script: vec![], sequence: 0, sig_op_count: 1 }],
            vec![TransactionOutput { value: 500, script_public_key: spk.clone() }],
            0,
            Default::default(),
            0,
            vec![],
        );
        let cc_entry = CcUtxoEntry { amount: 1000, script_public_key: spk.clone(), block_daa_score: 0, is_coinbase: false };

        // Shared base randomness. The WASM signer derives per-input randomness as
        // `base XOR (input_index as u64 LE)`; the native closure replicates it so
        // the two signatures are byte-identical (ML-DSA sign is deterministic given
        // key + message + context + randomness). For input 0 the xor is a no-op.
        let base = [0x5au8; 32];

        // Native signer over a SignableTransaction.
        let mut signable = SignableTransaction::with_entries(cctx.clone(), vec![cc_entry.clone()]);
        let signed = sign_transaction_inputs_mldsa87(&native_kp, &mut signable, |i, _sig_hash| {
            let mut r = base;
            let ib = (i as u64).to_le_bytes();
            for k in 0..8 {
                r[k] ^= ib[k];
            }
            r
        });
        assert_eq!(signed, 1, "native signer signed exactly one input");
        let native_script = signable.tx.inputs[0].signature_script.clone();
        assert!(!native_script.is_empty());

        // WASM signer over the equivalent client transaction (inputs populated with
        // the same UTXO so the sighash is computed over identical data).
        let client_utxo = ClientUtxoEntry {
            address: None,
            outpoint: outpoint.into(),
            amount: 1000,
            script_public_key: spk.clone(),
            block_daa_score: 0,
            is_coinbase: false,
        };
        let utxo_ref: kaspa_consensus_client::UtxoEntryReference = client_utxo.into();
        let mut utxos = AHashMap::new();
        utxos.insert(utxo_ref.id(), utxo_ref);
        let client_tx = ClientTransaction::from_cctx_transaction(&cctx, &utxos);
        let signed_client = js_sign_transaction_mldsa87(&client_tx, &wasm_kp, base.to_vec()).expect("wasm ML-DSA sign");
        let (wasm_cctx, _utxos) = signed_client.tx_and_utxos().expect("tx_and_utxos");
        let wasm_script = wasm_cctx.inputs[0].signature_script.clone();

        // The whole point: identical unlock scripts from the two implementations.
        assert_eq!(native_script, wasm_script, "WASM and native ML-DSA signers must produce identical unlock scripts");
    }
}
