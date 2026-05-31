use arc_swap::ArcSwapOption;
use kaspa_hashes::{
    Hash, Hash64, Hasher, HasherBase, TransactionSigningHash, TransactionSigningHash64, TransactionSigningHashECDSA, ZERO_HASH,
    ZERO_HASH64,
};
use std::cell::Cell;
use std::sync::Arc;

use crate::tx::{ScriptPublicKey, Transaction, TransactionOutpoint, TransactionOutput, VerifiableTransaction};

use super::{HasherExtensions, sighash_type::SigHashType};

/// Holds all fields used in the calculation of a transaction's sig_hash which are
/// the same for all transaction inputs.
/// Reuse of such values prevents the quadratic hashing problem.
#[derive(Default)]
pub struct SigHashReusedValuesUnsync {
    previous_outputs_hash: Cell<Option<Hash>>,
    sequences_hash: Cell<Option<Hash>>,
    sig_op_counts_hash: Cell<Option<Hash>>,
    outputs_hash: Cell<Option<Hash>>,
    payload_hash: Cell<Option<Hash>>,
}

impl SigHashReusedValuesUnsync {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Default)]
pub struct SigHashReusedValuesSync {
    previous_outputs_hash: ArcSwapOption<Hash>,
    sequences_hash: ArcSwapOption<Hash>,
    sig_op_counts_hash: ArcSwapOption<Hash>,
    outputs_hash: ArcSwapOption<Hash>,
    payload_hash: ArcSwapOption<Hash>,
}

impl SigHashReusedValuesSync {
    pub fn new() -> Self {
        Self::default()
    }
}

pub trait SigHashReusedValues {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash) -> Hash;
    fn sequences_hash(&self, set: impl Fn() -> Hash) -> Hash;
    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash) -> Hash;
    fn outputs_hash(&self, set: impl Fn() -> Hash) -> Hash;
    fn payload_hash(&self, set: impl Fn() -> Hash) -> Hash;
}

impl SigHashReusedValues for SigHashReusedValuesUnsync {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.previous_outputs_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.previous_outputs_hash.set(Some(hash));
            hash
        })
    }

    fn sequences_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.sequences_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.sequences_hash.set(Some(hash));
            hash
        })
    }

    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.sig_op_counts_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.sig_op_counts_hash.set(Some(hash));
            hash
        })
    }

    fn outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.outputs_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.outputs_hash.set(Some(hash));
            hash
        })
    }

    fn payload_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.payload_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.payload_hash.set(Some(hash));
            hash
        })
    }
}

impl SigHashReusedValues for SigHashReusedValuesSync {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.previous_outputs_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.previous_outputs_hash.rcu(|_| Arc::new(hash));
        hash
    }

    fn sequences_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.sequences_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.sequences_hash.rcu(|_| Arc::new(hash));
        hash
    }

    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.sig_op_counts_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.sig_op_counts_hash.rcu(|_| Arc::new(hash));
        hash
    }

    fn outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.outputs_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.outputs_hash.rcu(|_| Arc::new(hash));
        hash
    }

    fn payload_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.payload_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.payload_hash.rcu(|_| Arc::new(hash));
        hash
    }
}

pub fn previous_outputs_hash(tx: &Transaction, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_anyone_can_pay() {
        return ZERO_HASH;
    }
    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.inputs.iter() {
            hasher.update(input.previous_outpoint.transaction_id.as_bytes());
            hasher.write_u32(input.previous_outpoint.index);
        }
        hasher.finalize()
    };
    reused_values.previous_outputs_hash(hash)
}

pub fn sequences_hash(tx: &Transaction, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_single() || hash_type.is_sighash_anyone_can_pay() || hash_type.is_sighash_none() {
        return ZERO_HASH;
    }
    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.inputs.iter() {
            hasher.write_u64(input.sequence);
        }
        hasher.finalize()
    };
    reused_values.sequences_hash(hash)
}

pub fn sig_op_counts_hash(tx: &Transaction, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_anyone_can_pay() {
        return ZERO_HASH;
    }

    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.inputs.iter() {
            hasher.write_u8(input.sig_op_count);
        }
        hasher.finalize()
    };
    reused_values.sig_op_counts_hash(hash)
}

pub fn payload_hash(tx: &Transaction, reused_values: &impl SigHashReusedValues) -> Hash {
    if tx.subnetwork_id.is_native() && tx.payload.is_empty() {
        return ZERO_HASH;
    }

    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        hasher.write_var_bytes(&tx.payload);
        hasher.finalize()
    };
    reused_values.payload_hash(hash)
}

pub fn outputs_hash(tx: &Transaction, hash_type: SigHashType, reused_values: &impl SigHashReusedValues, input_index: usize) -> Hash {
    if hash_type.is_sighash_none() {
        return ZERO_HASH;
    }

    if hash_type.is_sighash_single() {
        // If the relevant output exists - return its hash, otherwise return zero-hash
        if input_index >= tx.outputs.len() {
            return ZERO_HASH;
        }

        let mut hasher = TransactionSigningHash::new();
        hash_output(&mut hasher, &tx.outputs[input_index]);
        return hasher.finalize();
    }
    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for output in tx.outputs.iter() {
            hash_output(&mut hasher, output);
        }
        hasher.finalize()
    };
    // Otherwise, return hash of all outputs. Re-use hash if available.
    reused_values.outputs_hash(hash)
}

pub fn hash_outpoint(hasher: &mut impl Hasher, outpoint: TransactionOutpoint) {
    hasher.update(outpoint.transaction_id);
    hasher.write_u32(outpoint.index);
}

pub fn hash_output(hasher: &mut impl Hasher, output: &TransactionOutput) {
    hasher.write_u64(output.value);
    hash_script_public_key(hasher, &output.script_public_key);
}

pub fn hash_script_public_key(hasher: &mut impl Hasher, script_public_key: &ScriptPublicKey) {
    hasher.write_u16(script_public_key.version());
    hasher.write_var_bytes(script_public_key.script());
}

pub fn calc_schnorr_signature_hash(
    verifiable_tx: &impl VerifiableTransaction,
    input_index: usize,
    hash_type: SigHashType,
    reused_values: &impl SigHashReusedValues,
) -> Hash {
    let input = verifiable_tx.populated_input(input_index);
    let tx = verifiable_tx.tx();
    let mut hasher = TransactionSigningHash::new();
    hasher
        .write_u16(tx.version)
        .update(previous_outputs_hash(tx, hash_type, reused_values))
        .update(sequences_hash(tx, hash_type, reused_values))
        .update(sig_op_counts_hash(tx, hash_type, reused_values));
    hash_outpoint(&mut hasher, input.0.previous_outpoint);
    hash_script_public_key(&mut hasher, &input.1.script_public_key);
    hasher
        .write_u64(input.1.amount)
        .write_u64(input.0.sequence)
        .write_u8(input.0.sig_op_count)
        .update(outputs_hash(tx, hash_type, reused_values, input_index))
        .write_u64(tx.lock_time)
        .update(&tx.subnetwork_id)
        .write_u64(tx.gas)
        .update(payload_hash(tx, reused_values))
        .write_u8(hash_type.to_u8());
    hasher.finalize()
}

pub fn calc_ecdsa_signature_hash(
    tx: &impl VerifiableTransaction,
    input_index: usize,
    hash_type: SigHashType,
    reused_values: &impl SigHashReusedValues,
) -> Hash {
    let hash = calc_schnorr_signature_hash(tx, input_index, hash_type, reused_values);
    let mut hasher = TransactionSigningHashECDSA::new();
    hasher.update(hash);
    hasher.finalize()
}

// =====================================================================
// kaspa-pq PQ-only ML-DSA-87 signature hash (ADR-0019 §9 /
// docs/kaspa-pq-design-mldsa87.md §9).
//
// The legacy ML-DSA opcode path reused `calc_schnorr_signature_hash`
// (a 32-byte digest under the `b"TransactionSigningHash"` domain). That
// is weak on two axes: hash width (256-bit, ~128-bit post-quantum
// preimage margin under Grover) and scheme separation (ML-DSA shares the
// secp256k1 signing transcript). This module replaces it with a dedicated
// 64-byte digest:
//
//  - **Width:** built with [`TransactionSigningHash64`] (BLAKE2b,
//    64-byte output) so the commitment domain matches the rest of the
//    kaspa-pq 64-byte consensus identity (ADR-0008).
//  - **Scheme + version separation:** the hasher is keyed by the distinct
//    `b"TransactionSigningHash64"` domain string AND the transcript is
//    prefixed with the literal [`MLDSA87_SIGHASH_DOMAIN`]. A signature
//    made over a schnorr (32-byte) digest can therefore never verify here,
//    and vice-versa.
//
// The transcript covers exactly the same semantic fields as the schnorr
// sighash, in the same order, so the security review carries over field
// for field. The [`Hash64`] hasher family intentionally does NOT
// implement the 32-byte `Hasher` trait (ADR-0008), so the field encoding
// is written explicitly via the inherent `write`, using little-endian
// integer encodings and a u64 length prefix for variable-length data
// (the same convention as `HasherExtensions`).
// =====================================================================

/// Literal domain tag prefixed to every ML-DSA-87 transaction sighash
/// transcript (docs/kaspa-pq-design-mldsa87.md §9.3 / md2 §3.1, v2). Belt-and-
/// braces scheme/version separation on top of the keyed 64-byte hasher.
pub const MLDSA87_SIGHASH_DOMAIN: &[u8] = b"kaspa-pq-v2/sighash/mldsa87";

/// Hash64 analogue of [`SigHashReusedValues`]: caches the per-transaction
/// sub-hashes that are identical across all inputs, preventing the
/// quadratic-hashing problem for multi-input ML-DSA-87 transactions.
pub trait Mldsa87SigHashReusedValues {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash64) -> Hash64;
    fn sequences_hash(&self, set: impl Fn() -> Hash64) -> Hash64;
    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash64) -> Hash64;
    fn outputs_hash(&self, set: impl Fn() -> Hash64) -> Hash64;
    fn payload_hash(&self, set: impl Fn() -> Hash64) -> Hash64;
}

/// Single-threaded reuse cache (mirrors [`SigHashReusedValuesUnsync`]).
#[derive(Default)]
pub struct Mldsa87SigHashReusedValuesUnsync {
    previous_outputs_hash: Cell<Option<Hash64>>,
    sequences_hash: Cell<Option<Hash64>>,
    sig_op_counts_hash: Cell<Option<Hash64>>,
    outputs_hash: Cell<Option<Hash64>>,
    payload_hash: Cell<Option<Hash64>>,
}

impl Mldsa87SigHashReusedValuesUnsync {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Thread-safe reuse cache (mirrors [`SigHashReusedValuesSync`]), used by
/// the parallel script-verification path.
#[derive(Default)]
pub struct Mldsa87SigHashReusedValuesSync {
    previous_outputs_hash: ArcSwapOption<Hash64>,
    sequences_hash: ArcSwapOption<Hash64>,
    sig_op_counts_hash: ArcSwapOption<Hash64>,
    outputs_hash: ArcSwapOption<Hash64>,
    payload_hash: ArcSwapOption<Hash64>,
}

impl Mldsa87SigHashReusedValuesSync {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Mldsa87SigHashReusedValues for Mldsa87SigHashReusedValuesUnsync {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash64) -> Hash64 {
        self.previous_outputs_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.previous_outputs_hash.set(Some(hash));
            hash
        })
    }
    fn sequences_hash(&self, set: impl Fn() -> Hash64) -> Hash64 {
        self.sequences_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.sequences_hash.set(Some(hash));
            hash
        })
    }
    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash64) -> Hash64 {
        self.sig_op_counts_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.sig_op_counts_hash.set(Some(hash));
            hash
        })
    }
    fn outputs_hash(&self, set: impl Fn() -> Hash64) -> Hash64 {
        self.outputs_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.outputs_hash.set(Some(hash));
            hash
        })
    }
    fn payload_hash(&self, set: impl Fn() -> Hash64) -> Hash64 {
        self.payload_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.payload_hash.set(Some(hash));
            hash
        })
    }
}

impl Mldsa87SigHashReusedValues for Mldsa87SigHashReusedValuesSync {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash64) -> Hash64 {
        if let Some(value) = self.previous_outputs_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.previous_outputs_hash.rcu(|_| Arc::new(hash));
        hash
    }
    fn sequences_hash(&self, set: impl Fn() -> Hash64) -> Hash64 {
        if let Some(value) = self.sequences_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.sequences_hash.rcu(|_| Arc::new(hash));
        hash
    }
    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash64) -> Hash64 {
        if let Some(value) = self.sig_op_counts_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.sig_op_counts_hash.rcu(|_| Arc::new(hash));
        hash
    }
    fn outputs_hash(&self, set: impl Fn() -> Hash64) -> Hash64 {
        if let Some(value) = self.outputs_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.outputs_hash.rcu(|_| Arc::new(hash));
        hash
    }
    fn payload_hash(&self, set: impl Fn() -> Hash64) -> Hash64 {
        if let Some(value) = self.payload_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.payload_hash.rcu(|_| Arc::new(hash));
        hash
    }
}

// --- 64-bit transcript field encoders (explicit; the Hash64 hasher has
// no `Hasher`/`HasherExtensions` impl). ---

#[inline]
fn mldsa87_write_outpoint(hasher: &mut TransactionSigningHash64, outpoint: &TransactionOutpoint) {
    hasher.write(outpoint.transaction_id.as_bytes());
    hasher.write(outpoint.index.to_le_bytes());
}

#[inline]
fn mldsa87_write_script_public_key(hasher: &mut TransactionSigningHash64, spk: &ScriptPublicKey) {
    hasher.write(spk.version().to_le_bytes());
    // var-bytes: u64 little-endian length prefix, then the bytes.
    hasher.write((spk.script().len() as u64).to_le_bytes());
    hasher.write(spk.script());
}

#[inline]
fn mldsa87_write_output(hasher: &mut TransactionSigningHash64, output: &TransactionOutput) {
    hasher.write(output.value.to_le_bytes());
    mldsa87_write_script_public_key(hasher, &output.script_public_key);
}

fn mldsa87_previous_outputs_hash(tx: &Transaction, hash_type: SigHashType, reused: &impl Mldsa87SigHashReusedValues) -> Hash64 {
    if hash_type.is_sighash_anyone_can_pay() {
        return ZERO_HASH64;
    }
    reused.previous_outputs_hash(|| {
        let mut hasher = TransactionSigningHash64::new();
        for input in tx.inputs.iter() {
            mldsa87_write_outpoint(&mut hasher, &input.previous_outpoint);
        }
        hasher.finalize()
    })
}

fn mldsa87_sequences_hash(tx: &Transaction, hash_type: SigHashType, reused: &impl Mldsa87SigHashReusedValues) -> Hash64 {
    if hash_type.is_sighash_single() || hash_type.is_sighash_anyone_can_pay() || hash_type.is_sighash_none() {
        return ZERO_HASH64;
    }
    reused.sequences_hash(|| {
        let mut hasher = TransactionSigningHash64::new();
        for input in tx.inputs.iter() {
            hasher.write(input.sequence.to_le_bytes());
        }
        hasher.finalize()
    })
}

fn mldsa87_sig_op_counts_hash(tx: &Transaction, hash_type: SigHashType, reused: &impl Mldsa87SigHashReusedValues) -> Hash64 {
    if hash_type.is_sighash_anyone_can_pay() {
        return ZERO_HASH64;
    }
    reused.sig_op_counts_hash(|| {
        let mut hasher = TransactionSigningHash64::new();
        for input in tx.inputs.iter() {
            hasher.write(&[input.sig_op_count][..]);
        }
        hasher.finalize()
    })
}

fn mldsa87_payload_hash(tx: &Transaction, reused: &impl Mldsa87SigHashReusedValues) -> Hash64 {
    if tx.subnetwork_id.is_native() && tx.payload.is_empty() {
        return ZERO_HASH64;
    }
    reused.payload_hash(|| {
        let mut hasher = TransactionSigningHash64::new();
        hasher.write((tx.payload.len() as u64).to_le_bytes());
        hasher.write(&tx.payload);
        hasher.finalize()
    })
}

fn mldsa87_outputs_hash(
    tx: &Transaction,
    hash_type: SigHashType,
    reused: &impl Mldsa87SigHashReusedValues,
    input_index: usize,
) -> Hash64 {
    if hash_type.is_sighash_none() {
        return ZERO_HASH64;
    }
    if hash_type.is_sighash_single() {
        if input_index >= tx.outputs.len() {
            return ZERO_HASH64;
        }
        let mut hasher = TransactionSigningHash64::new();
        mldsa87_write_output(&mut hasher, &tx.outputs[input_index]);
        return hasher.finalize();
    }
    reused.outputs_hash(|| {
        let mut hasher = TransactionSigningHash64::new();
        for output in tx.outputs.iter() {
            mldsa87_write_output(&mut hasher, output);
        }
        hasher.finalize()
    })
}

/// kaspa-pq PQ-only (ADR-0019 §9): compute the 64-byte ML-DSA-87 signature
/// hash for `input_index` of `verifiable_tx`. This is the message both the
/// wallet/validator signer and the `OP_CHECKSIG_MLDSA87` verifier feed into
/// `libcrux_ml_dsa::ml_dsa_87` under `MLDSA87_TX_CONTEXT`; they MUST call
/// this same function so signer and verifier stay byte-for-byte in lockstep.
pub fn calc_mldsa87_signature_hash(
    verifiable_tx: &impl VerifiableTransaction,
    input_index: usize,
    hash_type: SigHashType,
    reused_values: &impl Mldsa87SigHashReusedValues,
) -> Hash64 {
    let input = verifiable_tx.populated_input(input_index);
    let tx = verifiable_tx.tx();
    let mut hasher = TransactionSigningHash64::new();
    hasher.write(MLDSA87_SIGHASH_DOMAIN);
    hasher.write(tx.version.to_le_bytes());
    hasher.write(mldsa87_previous_outputs_hash(tx, hash_type, reused_values).as_bytes());
    hasher.write(mldsa87_sequences_hash(tx, hash_type, reused_values).as_bytes());
    hasher.write(mldsa87_sig_op_counts_hash(tx, hash_type, reused_values).as_bytes());
    mldsa87_write_outpoint(&mut hasher, &input.0.previous_outpoint);
    mldsa87_write_script_public_key(&mut hasher, &input.1.script_public_key);
    hasher.write(input.1.amount.to_le_bytes());
    hasher.write(input.0.sequence.to_le_bytes());
    hasher.write(&[input.0.sig_op_count][..]);
    hasher.write(mldsa87_outputs_hash(tx, hash_type, reused_values, input_index).as_bytes());
    hasher.write(tx.lock_time.to_le_bytes());
    // SubnetworkId impls both AsRef<[u8;20]> and AsRef<[u8]>; pin the 20-byte
    // array form explicitly (matches the schnorr transcript's `update(&id)`).
    hasher.write(AsRef::<[u8; crate::subnets::SUBNETWORK_ID_SIZE]>::as_ref(&tx.subnetwork_id));
    hasher.write(tx.gas.to_le_bytes());
    hasher.write(mldsa87_payload_hash(tx, reused_values).as_bytes());
    hasher.write(&[hash_type.to_u8()][..]);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, vec};

    use smallvec::SmallVec;

    use crate::{
        hashing::sighash_type::{SIG_HASH_ALL, SIG_HASH_ANY_ONE_CAN_PAY, SIG_HASH_NONE, SIG_HASH_SINGLE},
        subnets::{SUBNETWORK_ID_NATIVE, SubnetworkId},
        tx::{PopulatedTransaction, Transaction, TransactionId, TransactionInput, UtxoEntry},
    };

    use super::*;

    // kaspa-pq Phase 9: pins specific sighash digest values regenerated for the
    // 64-byte (Hash64) transaction identity per ADR-0008.
    #[test]
    fn test_signature_hash() {
        // TODO: Copy all sighash tests from go kaspad.
        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("208325613d2eeaf7176ac6c670b13c0043156c427438ed72d74b7800862ad884e8ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("20fcef4c106cf11135bbd70f02a726a92162d2fb8b22f0469126f800862ad884e8ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_2 = SmallVec::from_vec(bytes.to_vec());

        let native_tx = Transaction::new(
            0,
            vec![
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                    signature_script: vec![],
                    sequence: 0,
                    sig_op_count: 0,
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 1 },
                    signature_script: vec![],
                    sequence: 1,
                    sig_op_count: 0,
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 2 },
                    signature_script: vec![],
                    sequence: 2,
                    sig_op_count: 0,
                },
            ],
            vec![
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()) },
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()) },
            ],
            1615462089000,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );

        let native_populated_tx = PopulatedTransaction::new(
            &native_tx,
            vec![
                UtxoEntry {
                    amount: 100,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
                UtxoEntry {
                    amount: 200,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
                UtxoEntry {
                    amount: 300,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
            ],
        );

        let mut subnetwork_tx = native_tx.clone();
        subnetwork_tx.subnetwork_id = SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        subnetwork_tx.gas = 250;
        subnetwork_tx.payload = vec![10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
        let subnetwork_populated_tx = PopulatedTransaction::new(
            &subnetwork_tx,
            vec![
                UtxoEntry {
                    amount: 100,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_1),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
                UtxoEntry {
                    amount: 200,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
                UtxoEntry {
                    amount: 300,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
            ],
        );

        enum ModifyAction {
            NoAction,
            Output(usize),
            Input(usize),
            AmountSpent(usize),
            PrevScriptPublicKey(usize),
            Sequence(usize),
            Payload,
            Gas,
            SubnetworkId,
        }

        struct TestVector<'a> {
            name: &'static str,
            populated_tx: &'a PopulatedTransaction<'a>,
            hash_type: SigHashType,
            input_index: usize,
            action: ModifyAction,
            expected_hash: &'static str,
        }

        const SIG_HASH_ALL_ANYONE_CAN_PAY: SigHashType = SigHashType(SIG_HASH_ALL.0 | SIG_HASH_ANY_ONE_CAN_PAY.0);
        const SIG_HASH_NONE_ANYONE_CAN_PAY: SigHashType = SigHashType(SIG_HASH_NONE.0 | SIG_HASH_ANY_ONE_CAN_PAY.0);
        const SIG_HASH_SINGLE_ANYONE_CAN_PAY: SigHashType = SigHashType(SIG_HASH_SINGLE.0 | SIG_HASH_ANY_ONE_CAN_PAY.0);

        let tests = [
            // SIG_HASH_ALL
            TestVector {
                name: "native-all-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "6a86e4ace13e3fc8e2ecf651b30080f82cebfe6fcf9a081903ec80360b0a3e04",
            },
            TestVector {
                name: "native-all-0-modify-input-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Input(1),
                expected_hash: "20ce8b5bc4f26eec304a51808bad59f42cb7b117fa8bdbc81f8ce547b885ea81", // should change the hash
            },
            TestVector {
                name: "native-all-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Output(1),
                expected_hash: "0649f9d04c3898923bce2232ff7e256ba9503f31e3fdaf2465c83bc8d01a4db7", // should change the hash
            },
            TestVector {
                name: "native-all-0-modify-sequence-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Sequence(1),
                expected_hash: "c307d2cd929a4aa409d514e6a8785779b41596ca4c3a76876b3888a38cab8512", // should change the hash
            },
            TestVector {
                name: "native-all-anyonecanpay-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "8c98e114b7b8ac45894cf498d9520e2c53268bb0145b503608a25e5f6d196c5c", // should change the hash
            },
            TestVector {
                name: "native-all-anyonecanpay-0-modify-input-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::Input(0),
                expected_hash: "c7e5208f132f9137f87a66ad22d55d2be7083121a726f8e5ecbabc4585608f74", // should change the hash
            },
            TestVector {
                name: "native-all-anyonecanpay-0-modify-input-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::Input(1),
                expected_hash: "8c98e114b7b8ac45894cf498d9520e2c53268bb0145b503608a25e5f6d196c5c", // shouldn't change the hash
            },
            TestVector {
                name: "native-all-anyonecanpay-0-modify-sequence",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::Sequence(1),
                expected_hash: "8c98e114b7b8ac45894cf498d9520e2c53268bb0145b503608a25e5f6d196c5c", // shouldn't change the hash
            },
            // SIG_HASH_NONE
            TestVector {
                name: "native-none-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "893916b1eb14164761d9c4fb68bf8d285ac4e77008818b354007755b5ea132e4", // should change the hash
            },
            TestVector {
                name: "native-none-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Output(1),
                expected_hash: "893916b1eb14164761d9c4fb68bf8d285ac4e77008818b354007755b5ea132e4", // shouldn't change the hash
            },
            TestVector {
                name: "native-none-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Output(1),
                expected_hash: "893916b1eb14164761d9c4fb68bf8d285ac4e77008818b354007755b5ea132e4", // should change the hash
            },
            TestVector {
                name: "native-none-0-modify-sequence-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Sequence(0),
                expected_hash: "5a04cf539cefa7dbfd39818721267f0845aafafc759b11f09bb90bcd63c230e7", // shouldn't change the hash
            },
            TestVector {
                name: "native-none-0-modify-sequence-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Sequence(1),
                expected_hash: "893916b1eb14164761d9c4fb68bf8d285ac4e77008818b354007755b5ea132e4", // should change the hash
            },
            TestVector {
                name: "native-none-anyonecanpay-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "f6ce9dd6eee1b95c25f49e1340cc8f9280f388f71fb173bb6d4a6f20af4db658", // should change the hash
            },
            TestVector {
                name: "native-none-anyonecanpay-0-modify-amount-spent",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::AmountSpent(0),
                expected_hash: "d6524b482c5b04e3c86e45cae9ca31605cc8f676eefd27413fe8213ef635ed76", // should change the hash
            },
            TestVector {
                name: "native-none-anyonecanpay-0-modify-script-public-key",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::PrevScriptPublicKey(0),
                expected_hash: "6e1d8980d09507166ab01511f3d11f72912a6d55231affc13d73d7736f690290", // should change the hash
            },
            // SIG_HASH_SINGLE
            TestVector {
                name: "native-single-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "3aed2646e0e95f59e7cd52a1edb2ad556ccf9389b10fac1497b4c6f90ffedcfb", // should change the hash
            },
            TestVector {
                name: "native-single-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::Output(1),
                expected_hash: "3aed2646e0e95f59e7cd52a1edb2ad556ccf9389b10fac1497b4c6f90ffedcfb", // should change the hash
            },
            TestVector {
                name: "native-single-0-modify-sequence-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::Sequence(0),
                expected_hash: "01897729b7d8f5575904654ea4e9af1dc19c8fa0960ce145cf3a561722e8588a", // should change the hash
            },
            TestVector {
                name: "native-single-0-modify-sequence-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::Sequence(1),
                expected_hash: "3aed2646e0e95f59e7cd52a1edb2ad556ccf9389b10fac1497b4c6f90ffedcfb", // shouldn't change the hash
            },
            TestVector {
                name: "native-single-2-no-corresponding-output",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 2,
                action: ModifyAction::NoAction,
                expected_hash: "1a359aa4e762aeb6439766895cd0477e7d0460a5be20a46cec39bb6ac5c8f073", // should change the hash
            },
            TestVector {
                name: "native-single-2-no-corresponding-output-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 2,
                action: ModifyAction::Output(1),
                expected_hash: "1a359aa4e762aeb6439766895cd0477e7d0460a5be20a46cec39bb6ac5c8f073", // shouldn't change the hash
            },
            TestVector {
                name: "native-single-anyonecanpay-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "ed4afd6a4137174b8e238a1840f77b852bf58c1a822ce48acf44f75173fbbe7d", // should change the hash
            },
            TestVector {
                name: "native-single-anyonecanpay-2-no-corresponding-output",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE_ANYONE_CAN_PAY,
                input_index: 2,
                action: ModifyAction::NoAction,
                expected_hash: "4acb5d92b29442799bbc4c22d16ae36dac76dc66fe00d3ca6b01d4167f48c9ea", // should change the hash
            },
            TestVector {
                name: "native-all-0-modify-payload",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Payload,
                expected_hash: "a00b2b81f2d4b2478465ecd0f3ea6c1d669f9763586e5ad1040e1aa7f716d781", // should change the hash
            },
            // subnetwork transaction
            TestVector {
                name: "subnetwork-all-0",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "19912e35d13c510380557640790fd27a6f30ec683ee6d722b5c98d4b0b147209", // should change the hash
            },
            TestVector {
                name: "subnetwork-all-modify-payload",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Payload,
                expected_hash: "568ef718eb6f3c2c8e91a31e6cd8c837fff20ad56fbc1773603d6f2e85bb336a", // should change the hash
            },
            TestVector {
                name: "subnetwork-all-modify-gas",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Gas,
                expected_hash: "2720f6367ff38805052ea533d33c4a3e748d27cf35c881dc61ce9804b5348cb4", // should change the hash
            },
            TestVector {
                name: "subnetwork-all-subnetwork-id",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::SubnetworkId,
                expected_hash: "ac9359e1175461919a364e79702d39ba7145a1378f7c4f6ae0dee004b24514c0", // should change the hash
            },
        ];

        for test in tests {
            let mut tx = test.populated_tx.tx.clone();
            let mut entries = test.populated_tx.entries.clone();
            match test.action {
                ModifyAction::NoAction => {}
                ModifyAction::Output(i) => {
                    tx.outputs[i].value = 100;
                }
                ModifyAction::Input(i) => {
                    tx.inputs[i].previous_outpoint.index = 2;
                }
                ModifyAction::AmountSpent(i) => {
                    entries[i].amount = 666;
                }
                ModifyAction::PrevScriptPublicKey(i) => {
                    let mut script_vec = entries[i].script_public_key.script().to_vec();
                    script_vec.append(&mut vec![1, 2, 3]);
                    entries[i].script_public_key = ScriptPublicKey::new(entries[i].script_public_key.version(), script_vec.into());
                }
                ModifyAction::Sequence(i) => {
                    tx.inputs[i].sequence = 12345;
                }
                ModifyAction::Payload => tx.payload = vec![6, 6, 6, 4, 2, 0, 1, 3, 3, 7],
                ModifyAction::Gas => tx.gas = 1234,
                ModifyAction::SubnetworkId => {
                    tx.subnetwork_id = SubnetworkId::from_bytes([6, 6, 6, 4, 2, 0, 1, 3, 3, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
                }
            }
            let populated_tx = PopulatedTransaction::new(&tx, entries);
            let reused_values = SigHashReusedValuesUnsync::new();
            assert_eq!(
                calc_schnorr_signature_hash(&populated_tx, test.input_index, test.hash_type, &reused_values).to_string(),
                test.expected_hash,
                "test {} failed",
                test.name
            );
        }
    }
}

#[cfg(test)]
mod mldsa87_sighash_tests {
    //! kaspa-pq PQ-only (ADR-0019 §9): the ML-DSA-87 signature hash. These
    //! lock the consensus-critical properties of `calc_mldsa87_signature_hash`:
    //! determinism, 64-byte width, field sensitivity, and — crucially — that it
    //! is a DIFFERENT digest from the legacy 32-byte schnorr sighash (so a
    //! signature made over one can never verify against the other).
    use super::*;
    use crate::hashing::sighash_type::{SIG_HASH_ALL, SIG_HASH_NONE, SIG_HASH_SINGLE};
    use crate::subnets::SUBNETWORK_ID_NATIVE;
    use crate::tx::{PopulatedTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
    use smallvec::smallvec;

    fn sample_tx() -> Transaction {
        Transaction::new(
            0,
            vec![
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: TransactionId::from_slice(&[0x11u8; 64]), index: 0 },
                    signature_script: vec![],
                    sequence: 5,
                    sig_op_count: 1,
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: TransactionId::from_slice(&[0x22u8; 64]), index: 7 },
                    signature_script: vec![],
                    sequence: 9,
                    sig_op_count: 1,
                },
            ],
            vec![
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, smallvec![0x76, 0xaa, 0x20]) },
                TransactionOutput { value: 400, script_public_key: ScriptPublicKey::new(0, smallvec![0x51]) },
            ],
            123,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        )
    }

    fn entries() -> Vec<UtxoEntry> {
        vec![
            UtxoEntry { amount: 1000, script_public_key: ScriptPublicKey::new(0, smallvec![0x76, 0xaa, 0x20]), block_daa_score: 0, is_coinbase: false },
            UtxoEntry { amount: 2000, script_public_key: ScriptPublicKey::new(0, smallvec![0x51]), block_daa_score: 0, is_coinbase: false },
        ]
    }

    fn digest(tx: &Transaction, entries: Vec<UtxoEntry>, idx: usize, ht: SigHashType) -> Hash64 {
        let pt = PopulatedTransaction::new(tx, entries);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        calc_mldsa87_signature_hash(&pt, idx, ht, &reused)
    }

    #[test]
    fn deterministic_and_64_bytes() {
        let tx = sample_tx();
        let a = digest(&tx, entries(), 0, SIG_HASH_ALL);
        let b = digest(&tx, entries(), 0, SIG_HASH_ALL);
        assert_eq!(a, b, "same input must yield same digest");
        assert_eq!(a.as_bytes().len(), 64, "ML-DSA-87 sighash must be 64 bytes");
        assert_ne!(a, ZERO_HASH64, "a real digest is never the zero hash");
    }

    #[test]
    fn sync_and_unsync_reuse_caches_agree() {
        let tx = sample_tx();
        let pt = PopulatedTransaction::new(&tx, entries());
        let unsync = Mldsa87SigHashReusedValuesUnsync::new();
        let sync = Mldsa87SigHashReusedValuesSync::new();
        // Two inputs exercise the cross-input reuse cache on both impls.
        for idx in 0..2 {
            let u = calc_mldsa87_signature_hash(&pt, idx, SIG_HASH_ALL, &unsync);
            let s = calc_mldsa87_signature_hash(&pt, idx, SIG_HASH_ALL, &sync);
            assert_eq!(u, s, "sync and unsync reuse caches must agree at input {idx}");
        }
    }

    #[test]
    fn distinct_inputs_distinct_digests() {
        let tx = sample_tx();
        let d0 = digest(&tx, entries(), 0, SIG_HASH_ALL);
        let d1 = digest(&tx, entries(), 1, SIG_HASH_ALL);
        assert_ne!(d0, d1, "different input indices must produce different digests");
    }

    #[test]
    fn output_mutation_changes_digest() {
        let tx = sample_tx();
        let base = digest(&tx, entries(), 0, SIG_HASH_ALL);
        let mut tx2 = sample_tx();
        tx2.outputs[0].value = 301;
        let mutated = digest(&tx2, entries(), 0, SIG_HASH_ALL);
        assert_ne!(base, mutated, "mutating an output must change the SIG_HASH_ALL digest");
    }

    #[test]
    fn hash_type_changes_digest() {
        let tx = sample_tx();
        let all = digest(&tx, entries(), 0, SIG_HASH_ALL);
        let none = digest(&tx, entries(), 0, SIG_HASH_NONE);
        let single = digest(&tx, entries(), 0, SIG_HASH_SINGLE);
        assert_ne!(all, none);
        assert_ne!(all, single);
        assert_ne!(none, single);
    }

    #[test]
    fn differs_from_schnorr_sighash() {
        // The whole point of §9: the ML-DSA-87 digest must NOT equal the legacy
        // 32-byte schnorr sighash (scheme + width separation). Compare the raw
        // bytes — a 64-byte digest can never share all bytes with a 32-byte one,
        // but we also assert the 32-byte prefix differs so a truncating verifier
        // could not be fooled.
        let tx = sample_tx();
        let pt = PopulatedTransaction::new(&tx, entries());
        let mldsa = {
            let reused = Mldsa87SigHashReusedValuesUnsync::new();
            calc_mldsa87_signature_hash(&pt, 0, SIG_HASH_ALL, &reused)
        };
        let schnorr = {
            let reused = SigHashReusedValuesUnsync::new();
            calc_schnorr_signature_hash(&pt, 0, SIG_HASH_ALL, &reused)
        };
        assert_ne!(&mldsa.as_bytes()[..32], &schnorr.as_bytes()[..], "ML-DSA-87 digest must differ from schnorr digest");
    }
}
