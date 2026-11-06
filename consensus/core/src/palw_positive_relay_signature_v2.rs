//! RFC-0046's exact transaction-signature template. This authenticates a future
//! spend only; it creates no R registration, custody, UTXO, native due or credit.
use crate::{
    Hash64,
    hashing::{
        sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash},
        sighash_type::{SIG_HASH_ANY_ONE_CAN_PAY, SIG_HASH_NONE},
    },
    mldsa87_primitives::p2pkh_mldsa87_spk,
    palw_lifecycle_objects_v2::PALW_LIFECYCLE_TX_VERSION_V2,
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
    tx::{Transaction, TransactionInput, TransactionOutpoint, UtxoEntry, VerifiableTransaction},
};
use libcrux_ml_dsa::ml_dsa_87::{MLDSA87Signature, MLDSA87SigningKey, MLDSA87VerificationKey, portable as mldsa};
use misaka_palw_positive_contract::{
    assignment::AssignmentId,
    identity::{PUBKEY_BYTES, SIGNATURE_BYTES},
};

pub const ASSIGNMENT_RELAY_PAYLOAD_BYTES_V2: usize = 77;
pub const ASSIGNMENT_RELAY_SIGHASH_V2: u8 = 0x82;
const CONTEXT: &[u8] = b"kaspa-pq-v2/tx/mldsa87";

// The fixed ANYONECANPAY recipe reads exactly input zero. No fabricated foreign
// UTXOs and no input-count-dependent allocation are needed for its signature.
struct InputZero<'a>(&'a Transaction, &'a UtxoEntry);
impl VerifiableTransaction for InputZero<'_> {
    fn tx(&self) -> &Transaction {
        self.0
    }
    fn populated_input(&self, index: usize) -> (&TransactionInput, &UtxoEntry) {
        assert_eq!(index, 0, "fixed relay sighash may read only its current input");
        (&self.0.inputs[0], self.1)
    }
    fn utxo(&self, index: usize) -> Option<&UtxoEntry> {
        (index == 0).then_some(self.1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelaySignatureErrorV2 {
    Amount,
    Template,
    Signature,
}

/// Literal lifecycle header and op12 None/empty. Empty receipts select no
/// caller subset; the native money owner must resolve ALL actual account dues.
pub fn assignment_relay_payload_v2(assignment: AssignmentId) -> [u8; ASSIGNMENT_RELAY_PAYLOAD_BYTES_V2] {
    let mut bytes = [0; ASSIGNMENT_RELAY_PAYLOAD_BYTES_V2];
    bytes[..2].copy_from_slice(&PALW_LIFECYCLE_TX_VERSION_V2.to_le_bytes());
    bytes[2] = 176;
    bytes[3..8].copy_from_slice(&[2, 0, 1, 0, 12]);
    bytes[8..72].copy_from_slice(&assignment.0.0);
    // byte72=None Case; bytes73..77=zero receipt count.
    bytes
}

pub struct FundingRelaySignatureTemplateV2 {
    transaction: Transaction,
    entry: UtxoEntry,
    key: [u8; PUBKEY_BYTES],
}
impl FundingRelaySignatureTemplateV2 {
    pub fn new(
        assignment: AssignmentId,
        point: TransactionOutpoint,
        amount: u64,
        key: [u8; PUBKEY_BYTES],
    ) -> Result<Self, RelaySignatureErrorV2> {
        if amount == 0 || amount == u64::MAX {
            return Err(RelaySignatureErrorV2::Amount);
        }
        let script = p2pkh_mldsa87_spk(&kaspa_hashes::blake2b_512_address_payload(&key).as_bytes());
        let transaction = Transaction::new(
            0,
            vec![TransactionInput::new(point, Vec::new(), 0, 1)],
            Vec::new(),
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            assignment_relay_payload_v2(assignment).to_vec(),
        );
        // DAA/coinbase are NOT in this sighash. This is signing-only data, never
        // a claimed accepted origin, matured coin or broadcastable transaction.
        Ok(Self { transaction, entry: UtxoEntry::new(amount, script, 0, false), key })
    }
    fn hash_for(&self, tx: &Transaction, entry: &UtxoEntry) -> Hash64 {
        // ANYONECANPAY uses only the current populated entry. Foreign inputs
        // still require their own real entries/scripts during ordinary Full.
        let populated = InputZero(tx, entry);
        calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_NONE | SIG_HASH_ANY_ONE_CAN_PAY, &Mldsa87SigHashReusedValuesUnsync::new())
    }
    pub fn signature_hash(&self) -> Hash64 {
        self.hash_for(&self.transaction, &self.entry)
    }
    pub fn sign(&self, key: &MLDSA87SigningKey, randomness: [u8; 32]) -> Result<[u8; SIGNATURE_BYTES], RelaySignatureErrorV2> {
        let sig = mldsa::sign(key, self.signature_hash().as_byte_slice(), CONTEXT, randomness)
            .map_err(|_| RelaySignatureErrorV2::Signature)?;
        let bytes = *sig.as_ref();
        self.verify(&bytes)?;
        Ok(bytes)
    }
    pub fn verify(&self, signature: &[u8; SIGNATURE_BYTES]) -> Result<(), RelaySignatureErrorV2> {
        self.verify_spend_signature(&self.transaction, &self.entry, signature)
    }
    pub fn verify_spend_signature(
        &self,
        tx: &Transaction,
        entry: &UtxoEntry,
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<(), RelaySignatureErrorV2> {
        let first = tx.inputs.first().ok_or(RelaySignatureErrorV2::Template)?;
        if tx.version != 0
            || tx.lock_time != 0
            || tx.gas != 0
            || tx.subnetwork_id != SUBNETWORK_ID_PALW_LIFECYCLE
            || tx.payload != self.transaction.payload
            || first.previous_outpoint != self.transaction.inputs[0].previous_outpoint
            || first.sequence != 0
            || first.sig_op_count != 1
            || entry.amount != self.entry.amount
            || entry.script_public_key != self.entry.script_public_key
        {
            return Err(RelaySignatureErrorV2::Template);
        }
        mldsa::verify(
            &MLDSA87VerificationKey::new(self.key),
            self.hash_for(tx, entry).as_byte_slice(),
            CONTEXT,
            &MLDSA87Signature::new(*signature),
        )
        .map_err(|_| RelaySignatureErrorV2::Signature)
    }
    /// Canonical ordinary P2PKH unlock script. Caller must retain real memory
    /// and invoke Full script/fee/maturity checks on the final spend separately.
    pub fn unlock_script(&self, signature: &[u8; SIGNATURE_BYTES]) -> Result<Vec<u8>, RelaySignatureErrorV2> {
        self.verify(signature)?;
        let mut script = Vec::with_capacity(SIGNATURE_BYTES + PUBKEY_BYTES + 7);
        script.push(0x4d);
        script.extend_from_slice(&((SIGNATURE_BYTES + 1) as u16).to_le_bytes());
        script.extend_from_slice(signature);
        script.push(ASSIGNMENT_RELAY_SIGHASH_V2);
        script.push(0x4d);
        script.extend_from_slice(&(PUBKEY_BYTES as u16).to_le_bytes());
        script.extend_from_slice(&self.key);
        Ok(script)
    }
}

/// Identity of the fixed signature recipe, including the exact location of the
/// Assignment parameter in the payload. Outpoint/amount/key are parameters
/// authenticated by each concrete normal transaction signature and R terms.
/// This recipe digest establishes neither future UTXO existence nor credit.
pub fn funding_relay_template_id_v2() -> misaka_palw_positive_contract::identity::Digest {
    use borsh::BorshSerialize;
    use misaka_palw_positive_contract::{assignment::funding_relay::RelayTemplateV2, identity::Digest};
    #[derive(BorshSerialize)]
    struct Recipe<'a> {
        version: u16,
        revision: u16,
        template: RelayTemplateV2,
        current_input: u32,
        transaction_version: u16,
        sequence: u64,
        sig_op_count: u8,
        lock_time: u64,
        subnetwork: &'a crate::subnets::SubnetworkId,
        gas: u64,
        zero_assignment_payload: [u8; ASSIGNMENT_RELAY_PAYLOAD_BYTES_V2],
        assignment_start: u16,
        assignment_end: u16,
        sighash: u8,
        signature_context: &'a [u8],
        entry_script_recipe: &'a [u8],
    }
    funding_relay_data_hash_v2(
        b"misaka/positive/funding-relay-template/v2/r1",
        &Recipe {
            version: 2,
            revision: 1,
            template: RelayTemplateV2::NativeOp12NoCaseNoReceiptsV1,
            current_input: 0,
            transaction_version: 0,
            sequence: 0,
            sig_op_count: 1,
            lock_time: 0,
            subnetwork: &SUBNETWORK_ID_PALW_LIFECYCLE,
            gas: 0,
            zero_assignment_payload: assignment_relay_payload_v2(AssignmentId(Digest([0; 64]))),
            assignment_start: 8,
            assignment_end: 72,
            sighash: ASSIGNMENT_RELAY_SIGHASH_V2,
            signature_context: CONTEXT,
            entry_script_recipe: b"p2pkh_mldsa87_spk(blake2b_512_address_payload(key))",
        },
    )
}

/// Streaming canonical identity hash for these fixed native DATA recipes.
/// This function supplies no native acceptance or computation authority.
pub(crate) fn funding_relay_data_hash_v2<T: borsh::BorshSerialize>(
    domain: &[u8],
    value: &T,
) -> misaka_palw_positive_contract::identity::Digest {
    struct Writer(blake2b_simd::State);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer(blake2b_simd::Params::new().hash_length(64).key(domain).to_state());
    borsh::to_writer(&mut writer, value).expect("fixed relay identity serialization");
    misaka_palw_positive_contract::identity::Digest(writer.0.finalize().as_bytes().try_into().expect("64-byte digest"))
}
