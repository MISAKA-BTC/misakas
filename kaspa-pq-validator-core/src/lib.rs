//! Shared kaspa-pq validator signing primitives (ADR-0010 / ADR-0011).
//!
//! Used by BOTH the in-process `--enable-validator` service in `kaspad` and the
//! standalone `kaspa-pq-validator` sidecar binary, so the two deployment shapes share a
//! single implementation of: the ML-DSA-65 validator key + its derived overlay identity
//! ([`ValidatorKey`]), fee-funded attestation-shard transaction building, and the
//! persistent equivocation-safety log ([`SignedEpochStore`], ADR-0011). No consensus
//! surface — this is a node-local helper crate.

use blake2b_simd::Params as Blake2bParams;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::constants::{MAX_TX_IN_SEQUENCE_NUM, TX_VERSION};
use kaspa_consensus_core::dns_finality::{
    ATTESTATION_MLDSA65_CONTEXT, DNS_PAYLOAD_VERSION_V1, SignedEpochCheckOutcome, SignedEpochRecord, StakeAttestation,
    StakeAttestationShardPayload, StakeBondPayload, check_signed_epoch_record, single_attestation_shard, validator_id_from_pubkey,
};
use kaspa_consensus_core::hashing::sighash::{SigHashReusedValuesUnsync, calc_schnorr_signature_hash};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::subnets::{SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, SUBNETWORK_ID_STAKE_BOND};
use kaspa_consensus_core::tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use kaspa_txscript::{
    MLDSA65_SIG_LEN, MLDSA65_TX_CONTEXT, pay_to_address_script, script_builder::ScriptBuilder, verify_mldsa65_with_context,
};
use libcrux_ml_dsa::ml_dsa_65;
use rand::RngCore;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

/// Length in bytes of the ML-DSA-65 keygen seed consumed by [`ValidatorKey::from_seed`]
/// (matches the wallet's `KaspaPqMlDsa65KeyPair`).
pub const VALIDATOR_SEED_LEN: usize = 32;

/// Floor (sompi) for the attestation-shard transaction fee. The actual fee should be the
/// transaction's compute mass (a safe >= mempool-minimum at the 1 sompi/gram relay rate),
/// clamped up to this floor. Set above the node's mass-based standard minimum for the
/// single-input ML-DSA-65 shard shape (observed ~15600 on devnet) so the shard is not
/// rejected as non-standard; the mass-based path (`estimate_attestation_fee`) overrides this
/// when a `MassCalculator` is available (the in-process service), and the sidecar's flat
/// fallback uses this floor.
pub const ATTESTATION_TX_FEE_FLOOR_SOMPI: u64 = 30_000;

const SIGNED_EPOCH_FILE_VERSION: u16 = 1;

/// Load a 32-byte ML-DSA-65 seed from a hex file (whitespace-trimmed). The file must
/// contain exactly [`VALIDATOR_SEED_LEN`] bytes as hex, which seeds the deterministic
/// ML-DSA-65 keypair via [`ValidatorKey::from_seed`].
pub fn load_validator_seed(path: &str) -> Result<[u8; VALIDATOR_SEED_LEN], String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("cannot read validator key file '{path}': {e}"))?;
    let hex = raw.trim();
    let mut seed = [0u8; VALIDATOR_SEED_LEN];
    faster_hex::hex_decode(hex.as_bytes(), &mut seed)
        .map_err(|e| format!("validator key file '{path}' must contain {VALIDATOR_SEED_LEN} bytes as hex: {e}"))?;
    Ok(seed)
}

/// Materialised validator signing key: the ML-DSA-65 keypair plus its derived overlay
/// identity (`validator_id = BLAKE2b-512(public_key)`, per ADR-0008/0012).
///
/// Constructed once at startup from the seed file and held for the validator's lifetime.
pub struct ValidatorKey {
    keypair: ml_dsa_65::MLDSA65KeyPair,
    /// Overlay identity advertised to the network and matched against the bond.
    pub validator_id: Hash64,
}

impl ValidatorKey {
    pub fn from_seed(seed: [u8; VALIDATOR_SEED_LEN]) -> Self {
        let keypair = ml_dsa_65::generate_key_pair(seed);
        let validator_id = validator_id_from_pubkey(keypair.verification_key.as_ref());
        Self { keypair, validator_id }
    }

    /// The validator's own P2PKH-ML-DSA address — `(prefix, PubKeyHashMlDsa65,
    /// BLAKE2b-256(public_key))`. This is the **spend** address (32-byte BLAKE2b-256
    /// payload), distinct from the 64-byte overlay `validator_id`. Funding UTXOs sent
    /// here back the attestation-shard transactions (funding model A).
    pub fn funding_address(&self, prefix: Prefix) -> Address {
        let mut payload = [0u8; 32];
        payload.copy_from_slice(
            Blake2bParams::new().hash_length(32).to_state().update(self.keypair.verification_key.as_ref()).finalize().as_bytes(),
        );
        Address::new(prefix, Version::PubKeyHashMlDsa65, &payload)
    }

    /// Sign `message` under an explicit ML-DSA-65 `context` (domain separator) with fresh
    /// hedged randomness. Distinct contexts keep attestation signatures
    /// ([`ATTESTATION_MLDSA65_CONTEXT`]) and transaction-input signatures
    /// ([`MLDSA65_TX_CONTEXT`]) in disjoint domains — neither can be replayed as the other.
    pub fn sign_with_context(&self, message: &[u8], context: &[u8]) -> [u8; MLDSA65_SIG_LEN] {
        let mut randomness = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut randomness);
        let sig = ml_dsa_65::sign(&self.keypair.signing_key, message, context, randomness)
            .expect("ML-DSA-65 sign is infallible on a well-formed message");
        *sig.as_ref()
    }

    /// Sign a stake-attestation `message` digest under [`ATTESTATION_MLDSA65_CONTEXT`].
    /// Verifies via [`verify_mldsa65_with_context`] — the same call the `virtual_processor`
    /// aggregator uses.
    pub fn sign_attestation(&self, message: &[u8]) -> [u8; MLDSA65_SIG_LEN] {
        self.sign_with_context(message, ATTESTATION_MLDSA65_CONTEXT)
    }

    /// Build a fee-funded, signed `StakeAttestationShard` transaction (ADR-0010 step 9,
    /// funding model A). Spends `funding` — a UTXO locked to this key's own P2PKH-ML-DSA
    /// script — to pay the fee, returns the change to the same script, and carries the
    /// borsh-encoded `shard` payload. The single input is signed under
    /// [`MLDSA65_TX_CONTEXT`] over the SIG_HASH_ALL sighash and wrapped as
    /// `<sig ‖ sighash-type> <pubkey>` so it satisfies `OpCheckSigMlDsa65`.
    ///
    /// `fee` is taken as a parameter; choosing it from the mass-based minimum and
    /// discovering the funding UTXO are the caller's job.
    pub fn build_funded_shard_tx(
        &self,
        shard: &StakeAttestationShardPayload,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        fee: u64,
    ) -> Result<Transaction, String> {
        if funding.amount <= fee {
            return Err(format!("funding UTXO amount {} does not cover fee {}", funding.amount, fee));
        }
        let payload = borsh::to_vec(shard).expect("borsh serialization of a well-formed shard is infallible");
        // Input with an empty signature script (filled after the sighash is computed);
        // change returns to the same script so the validator can fund the next attestation.
        let input = TransactionInput::new(funding_outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1);
        let change = TransactionOutput::new(funding.amount - fee, funding.script_public_key.clone());
        let tx = Transaction::new(TX_VERSION, vec![input], vec![change], 0, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, 0, payload);

        // Sighash is computed over the tx with empty signature scripts (canonical), so
        // signing before filling the script is correct.
        let mtx = MutableTransaction::with_entries(tx, vec![funding.clone()]);
        let reused = SigHashReusedValuesUnsync::new();
        let sighash = calc_schnorr_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &reused);

        let mut sig_data = self.sign_with_context(sighash.as_bytes().as_slice(), MLDSA65_TX_CONTEXT).to_vec();
        sig_data.push(SIG_HASH_ALL.to_u8()); // OpCheckSigMlDsa65 pops the trailing sighash-type byte
        let signature_script = ScriptBuilder::new()
            .add_data(&sig_data)
            .map_err(|e| format!("attestation funding sig push failed: {e}"))?
            .add_data(self.keypair.verification_key.as_ref())
            .map_err(|e| format!("attestation funding pubkey push failed: {e}"))?
            .drain();

        let mut tx = mtx.tx;
        tx.inputs[0].signature_script = signature_script;
        Ok(tx)
    }

    /// Build a fee-funded, signed `StakeBond` transaction (ADR-0010 / ADR-0016 §D.1) that
    /// stakes `amount` sompi: this is how mined coins become locked stake backing a
    /// validator. Spends `funding` — a UTXO at this key's own P2PKH-ML-DSA script — into:
    ///   - **output-0** = `amount` to the same script (the *locked stake*; its outpoint
    ///     `(txid, 0)` becomes the `bond_outpoint`). Consensus pins this output's value to
    ///     `payload.amount` at acceptance (§D.1) and the bond-spend-gate locks it while the
    ///     bond is Pending/Active/unbonding, so the declared `amount` is real capital.
    ///   - **output-1** = change (`funding.amount − amount − fee`) to the same script, emitted
    ///     only when non-zero.
    /// The borsh-encoded [`StakeBondPayload`] carries the bond terms; the validator's own
    /// 1952-byte ML-DSA-65 pubkey and the matching `validator_pubkey_hash`/`owner_pubkey_hash`
    /// (both = `validator_id`) are written so any node can verify attestations without a
    /// registry. `owner_reward_spk_payload` is where this bond's rewards are paid — set to the
    /// caller-supplied 32-byte P2PKH-ML-DSA payload (defaults to the validator's own funding
    /// payload). The single input is signed under [`MLDSA65_TX_CONTEXT`] exactly as
    /// [`Self::build_funded_shard_tx`].
    #[allow(clippy::too_many_arguments)]
    pub fn build_funded_stake_bond_tx(
        &self,
        amount: u64,
        activation_daa_score: u64,
        unbonding_period_blocks: u64,
        owner_reward_spk_payload: [u8; 32],
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        fee: u64,
    ) -> Result<Transaction, String> {
        if amount == 0 {
            return Err("stake-bond amount must be > 0".to_string());
        }
        let needed = amount.checked_add(fee).ok_or_else(|| "amount + fee overflows u64".to_string())?;
        if funding.amount < needed {
            return Err(format!("funding UTXO amount {} does not cover amount {} + fee {}", funding.amount, amount, fee));
        }
        // validator_id = BLAKE2b-512(pubkey) is both the owner and validator identity for a
        // self-bonded validator; the 32-byte reward payload is a separate spend target.
        let payload = StakeBondPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            owner_pubkey_hash: self.validator_id,
            validator_pubkey_hash: self.validator_id,
            validator_pubkey: self.keypair.verification_key.as_ref().to_vec(),
            amount,
            activation_daa_score,
            unbonding_period_blocks,
            owner_reward_spk_payload,
        };
        let payload = borsh::to_vec(&payload).expect("borsh serialization of a well-formed stake-bond is infallible");

        let spk = funding.script_public_key.clone(); // == pay_to_address_script(funding_address): self-spend
        let input = TransactionInput::new(funding_outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1);
        // output-0 MUST be the locked stake (value == amount); change (if any) follows.
        let mut outputs = vec![TransactionOutput::new(amount, spk.clone())];
        let change = funding.amount - needed;
        if change > 0 {
            outputs.push(TransactionOutput::new(change, spk));
        }
        let tx = Transaction::new(TX_VERSION, vec![input], outputs, 0, SUBNETWORK_ID_STAKE_BOND, 0, payload);

        // Sighash over the canonical (empty-sig-script) tx, then fill input 0's script.
        let mtx = MutableTransaction::with_entries(tx, vec![funding.clone()]);
        let reused = SigHashReusedValuesUnsync::new();
        let sighash = calc_schnorr_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &reused);
        let mut sig_data = self.sign_with_context(sighash.as_bytes().as_slice(), MLDSA65_TX_CONTEXT).to_vec();
        sig_data.push(SIG_HASH_ALL.to_u8());
        let signature_script = ScriptBuilder::new()
            .add_data(&sig_data)
            .map_err(|e| format!("stake-bond funding sig push failed: {e}"))?
            .add_data(self.keypair.verification_key.as_ref())
            .map_err(|e| format!("stake-bond funding pubkey push failed: {e}"))?
            .drain();
        let mut tx = mtx.tx;
        tx.inputs[0].signature_script = signature_script;
        Ok(tx)
    }

    /// The 32-byte P2PKH-ML-DSA reward payload for this key — `BLAKE2b-256(public_key)`, the
    /// same payload as [`Self::funding_address`]. Default `owner_reward_spk_payload` for a
    /// self-bonded validator (rewards return to the validator's own spend address).
    pub fn reward_spk_payload(&self) -> [u8; 32] {
        let mut payload = [0u8; 32];
        payload.copy_from_slice(
            Blake2bParams::new().hash_length(32).to_state().update(self.keypair.verification_key.as_ref()).finalize().as_bytes(),
        );
        payload
    }

    /// Mass-based fee (sompi) for this validator's attestation-shard transaction. The tx
    /// shape is fixed (1 P2PKH-ML-DSA input, 1 change output, a single-attestation shard),
    /// so a dummy build's compute mass equals the real one's — letting the service compute
    /// the fee once at startup. Clamped up to [`ATTESTATION_TX_FEE_FLOOR_SOMPI`].
    pub fn estimate_attestation_fee(&self, mass_calculator: &MassCalculator, prefix: Prefix) -> u64 {
        let funding_spk = pay_to_address_script(&self.funding_address(prefix));
        let dummy = StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: self.validator_id,
            bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([0u8; 64]), 0),
            epoch: 0,
            target_hash: Hash64::from_bytes([0u8; 64]),
            target_daa_score: 0,
            validator_set_commitment: Hash64::from_bytes([0u8; 64]),
            signature: vec![0u8; MLDSA65_SIG_LEN],
        };
        let shard = single_attestation_shard(dummy);
        let funding = UtxoEntry::new(u64::MAX / 2, funding_spk, 0, false);
        let outpoint = TransactionOutpoint::new(Hash64::from_bytes([0u8; 64]), 0);
        match self.build_funded_shard_tx(&shard, outpoint, &funding, ATTESTATION_TX_FEE_FLOOR_SOMPI) {
            Ok(tx) => mass_calculator.calc_non_contextual_masses(&tx).compute_mass.max(ATTESTATION_TX_FEE_FLOOR_SOMPI),
            Err(_) => ATTESTATION_TX_FEE_FLOOR_SOMPI,
        }
    }

    /// Verify an attestation signature against this key (local round-trip sanity check).
    pub fn verify_attestation(&self, message: &[u8], signature: &[u8]) -> bool {
        matches!(
            verify_mldsa65_with_context(self.keypair.verification_key.as_ref(), message, signature, ATTESTATION_MLDSA65_CONTEXT),
            Ok(true)
        )
    }
}

/// Parse a `"txid:index"` stake-bond reference into a [`TransactionOutpoint`]. `txid` is
/// the 64-byte transaction id (128 hex chars); `index` is the output index of the
/// bond-creating output.
pub fn parse_stake_bond_ref(s: &str) -> Result<TransactionOutpoint, String> {
    let (txid, index) = s.split_once(':').ok_or_else(|| format!("stake-bond '{s}' must be in 'txid:index' form"))?;
    let transaction_id = Hash64::from_str(txid).map_err(|e| format!("stake-bond '{s}' has an invalid transaction id: {e}"))?;
    let index = index.parse::<u32>().map_err(|_| format!("stake-bond '{s}' has a non-numeric output index"))?;
    Ok(TransactionOutpoint::new(transaction_id, index))
}

/// On-disk shape of the per-validator equivocation-safety log (JSON). Bound to a single
/// `(validator_id, bond_outpoint)` so one host can never silently clobber another key's
/// safety record.
#[derive(serde::Serialize, serde::Deserialize)]
struct SignedEpochFile {
    version: u16,
    validator_id: Hash64,
    bond_outpoint: TransactionOutpoint,
    /// epoch -> the attestation signed for it.
    records: BTreeMap<u64, SignedEpochRecord>,
}

/// Persistent per-epoch signing log enforcing ADR-0011 equivocation safety across
/// restarts. Keyed in memory by epoch (the `(bond_outpoint, validator_id)` part of the
/// ADR triple is fixed for one running validator and lives in the file header).
pub struct SignedEpochStore {
    path: PathBuf,
    validator_id: Hash64,
    bond_outpoint: TransactionOutpoint,
    records: BTreeMap<u64, SignedEpochRecord>,
}

impl SignedEpochStore {
    /// Load the log for `(validator_id, bond_outpoint)` from `path`, or start empty if the
    /// file is absent. Errors if the file exists but belongs to a different validator/bond
    /// — refusing to operate is safer than risking cross-key equivocation.
    pub fn load_or_empty(path: PathBuf, validator_id: Hash64, bond_outpoint: TransactionOutpoint) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self { path, validator_id, bond_outpoint, records: BTreeMap::new() });
        }
        let raw = fs::read_to_string(&path).map_err(|e| format!("cannot read validator-state file {}: {e}", path.display()))?;
        let file: SignedEpochFile =
            serde_json::from_str(&raw).map_err(|e| format!("cannot parse validator-state file {}: {e}", path.display()))?;
        if file.validator_id != validator_id || file.bond_outpoint != bond_outpoint {
            return Err(format!("validator-state file {} belongs to a different validator/bond; refusing to use it", path.display()));
        }
        Ok(Self { path, validator_id, bond_outpoint, records: file.records })
    }

    /// Equivocation outcome for `candidate` against the persisted record for its epoch.
    pub fn check(&self, candidate: &SignedEpochRecord) -> SignedEpochCheckOutcome {
        check_signed_epoch_record(self.records.get(&candidate.epoch), candidate)
    }

    /// Highest epoch this validator has a signing record for (`None` if it never signed).
    pub fn last_signed_epoch(&self) -> Option<u64> {
        self.records.keys().next_back().copied()
    }

    /// Whether a signing record exists for `epoch`.
    pub fn has_signed_epoch(&self, epoch: u64) -> bool {
        self.records.contains_key(&epoch)
    }

    /// Number of epochs with a persisted signing record (for status / logging).
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// Persist `record` for its epoch and flush atomically (temp file + rename so a crash
    /// mid-write cannot truncate the log). Call only after a successful sign and after
    /// [`Self::check`] returned [`SignedEpochCheckOutcome::Allow`].
    pub fn record_and_flush(&mut self, record: SignedEpochRecord) -> Result<(), String> {
        self.records.insert(record.epoch, record);
        let file = SignedEpochFile {
            version: SIGNED_EPOCH_FILE_VERSION,
            validator_id: self.validator_id,
            bond_outpoint: self.bond_outpoint,
            records: self.records.clone(),
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| format!("cannot serialize validator-state: {e}"))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("cannot create validator-state dir {}: {e}", parent.display()))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| format!("cannot write validator-state tmp {}: {e}", tmp.display()))?;
        fs::rename(&tmp, &self.path).map_err(|e| format!("cannot commit validator-state {}: {e}", self.path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_validator_seed_accepts_32_byte_hex() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let seed_hex = "11".repeat(VALIDATOR_SEED_LEN); // 32 bytes of 0x11
        write!(f, "  {seed_hex}\n").unwrap();
        let seed = load_validator_seed(f.path().to_str().unwrap()).unwrap();
        assert_eq!(seed, [0x11u8; VALIDATOR_SEED_LEN]);
    }

    #[test]
    fn load_validator_seed_rejects_wrong_length() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "1122").unwrap(); // only 2 bytes
        assert!(load_validator_seed(f.path().to_str().unwrap()).is_err());
    }

    #[test]
    fn parse_stake_bond_ref_valid_and_invalid() {
        let txid = "ab".repeat(64); // 128 hex chars = 64-byte Hash64
        let op = parse_stake_bond_ref(&format!("{txid}:7")).unwrap();
        assert_eq!(op.index, 7);
        assert_eq!(op.transaction_id, Hash64::from_str(&txid).unwrap());
        // Errors:
        assert!(parse_stake_bond_ref(&txid).is_err()); // no ':' separator / index
        assert!(parse_stake_bond_ref(&format!("{txid}:x")).is_err()); // non-numeric index
        assert!(parse_stake_bond_ref("abcd:0").is_err()); // txid too short for Hash64
        assert!(parse_stake_bond_ref(":0").is_err()); // empty txid
    }

    #[test]
    fn validator_key_from_seed_is_deterministic_and_seed_dependent() {
        // Same seed → same keypair → same validator_id (keygen is deterministic).
        let id_a = ValidatorKey::from_seed([0x11u8; VALIDATOR_SEED_LEN]).validator_id;
        let id_a2 = ValidatorKey::from_seed([0x11u8; VALIDATOR_SEED_LEN]).validator_id;
        assert_eq!(id_a, id_a2);
        // Different seed → different identity.
        let id_b = ValidatorKey::from_seed([0x22u8; VALIDATOR_SEED_LEN]).validator_id;
        assert_ne!(id_a, id_b);
    }

    #[test]
    fn validator_id_matches_blake2b_512_of_public_key() {
        // The advertised validator_id must equal the canonical
        // dns_finality::validator_id_from_pubkey over this key's public key.
        let key = ValidatorKey::from_seed([0x33u8; VALIDATOR_SEED_LEN]);
        let expected = validator_id_from_pubkey(key.keypair.verification_key.as_ref());
        assert_eq!(key.validator_id, expected);
    }

    #[test]
    fn funding_address_is_p2pkh_mldsa65_over_blake2b_256_pubkey() {
        let key = ValidatorKey::from_seed([0x44u8; VALIDATOR_SEED_LEN]);
        let addr = key.funding_address(Prefix::Devnet);
        assert_eq!(addr.version, Version::PubKeyHashMlDsa65);
        assert_eq!(addr.prefix, Prefix::Devnet);
        // Payload = BLAKE2b-256(pubkey) — the 32-byte spend hash, not the 64-byte validator_id.
        let mut expected = [0u8; 32];
        expected.copy_from_slice(
            Blake2bParams::new().hash_length(32).to_state().update(key.keypair.verification_key.as_ref()).finalize().as_bytes(),
        );
        assert_eq!(addr.payload.as_slice(), &expected);
    }

    #[test]
    fn sign_attestation_roundtrip_and_tamper() {
        let key = ValidatorKey::from_seed([0x55u8; VALIDATOR_SEED_LEN]);
        let msg = [0x99u8; 32]; // stand-in for a stake_attestation_message digest
        let sig = key.sign_attestation(&msg);
        assert_eq!(sig.len(), MLDSA65_SIG_LEN);
        assert!(key.verify_attestation(&msg, &sig));
        // A tampered digest must fail verification.
        let mut bad = msg;
        bad[0] ^= 0x01;
        assert!(!key.verify_attestation(&bad, &sig));
    }

    #[test]
    fn sign_with_context_is_domain_separated() {
        let key = ValidatorKey::from_seed([0x88u8; VALIDATOR_SEED_LEN]);
        let msg = [0x5au8; 32]; // stand-in for a SIG_HASH_ALL sighash
        let sig = key.sign_with_context(&msg, MLDSA65_TX_CONTEXT);
        let pk = key.keypair.verification_key.as_ref();
        // Verifies under the tx context...
        assert!(matches!(verify_mldsa65_with_context(pk, &msg, &sig, MLDSA65_TX_CONTEXT), Ok(true)));
        // ...but NOT under the attestation context (domain separation).
        assert!(!matches!(verify_mldsa65_with_context(pk, &msg, &sig, ATTESTATION_MLDSA65_CONTEXT), Ok(true)));
    }

    #[test]
    fn build_funded_shard_tx_structure_and_funding() {
        use kaspa_consensus_core::dns_finality::validate_stake_attestation_shard_payload;
        use kaspa_consensus_core::tx::ScriptPublicKey;

        let key = ValidatorKey::from_seed([0x77u8; VALIDATOR_SEED_LEN]);
        let shard = single_attestation_shard(StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: key.validator_id,
            bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([0x01u8; 64]), 0),
            epoch: 7,
            target_hash: Hash64::from_bytes([0x11u8; 64]),
            target_daa_score: 700,
            validator_set_commitment: Hash64::from_bytes([0x22u8; 64]),
            signature: vec![0u8; MLDSA65_SIG_LEN],
        });
        let funding_spk = ScriptPublicKey::default();
        let funding = UtxoEntry::new(1_000, funding_spk.clone(), 1, false);
        let funding_outpoint = TransactionOutpoint::new(Hash64::from_bytes([0x99u8; 64]), 3);

        let tx = key.build_funded_shard_tx(&shard, funding_outpoint, &funding, 250).unwrap();
        assert_eq!(tx.inputs.len(), 1);
        assert_eq!(tx.inputs[0].previous_outpoint, funding_outpoint);
        assert!(!tx.inputs[0].signature_script.is_empty()); // signed
        assert_eq!(tx.outputs.len(), 1);
        assert_eq!(tx.outputs[0].value, 750); // amount - fee, change back to self
        assert_eq!(tx.outputs[0].script_public_key, funding_spk);
        assert_eq!(tx.subnetwork_id, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD);
        assert_eq!(tx.gas, 0);
        assert!(validate_stake_attestation_shard_payload(&tx.payload).is_ok());

        // Fee must be strictly less than the funding amount.
        assert!(key.build_funded_shard_tx(&shard, funding_outpoint, &funding, 1_000).is_err());
    }

    #[test]
    fn build_funded_stake_bond_tx_structure_and_lock() {
        use kaspa_consensus_core::dns_finality::{StakeBondPayload, validate_stake_bond_payload};
        use kaspa_consensus_core::subnets::SUBNETWORK_ID_STAKE_BOND;
        use kaspa_consensus_core::tx::ScriptPublicKey;

        let key = ValidatorKey::from_seed([0x66u8; VALIDATOR_SEED_LEN]);
        let funding_spk = ScriptPublicKey::default();
        let funding = UtxoEntry::new(10_000, funding_spk.clone(), 1, false);
        let funding_outpoint = TransactionOutpoint::new(Hash64::from_bytes([0x42u8; 64]), 2);
        let reward = key.reward_spk_payload();

        // Stake 6_000 with a 250 fee from a 10_000 UTXO → output-0=6_000 (locked), change=3_750.
        let tx = key.build_funded_stake_bond_tx(6_000, 0, 700, reward, funding_outpoint, &funding, 250).unwrap();
        assert_eq!(tx.subnetwork_id, SUBNETWORK_ID_STAKE_BOND);
        assert_eq!(tx.gas, 0);
        assert_eq!(tx.inputs.len(), 1);
        assert!(!tx.inputs[0].signature_script.is_empty()); // signed
        assert_eq!(tx.outputs.len(), 2);
        assert_eq!(tx.outputs[0].value, 6_000); // §D.1: output-0 == amount (locked stake)
        assert_eq!(tx.outputs[0].script_public_key, funding_spk);
        assert_eq!(tx.outputs[1].value, 3_750); // change = 10_000 - 6_000 - 250
        // Payload round-trips, is stateless-valid, and binds the validator pubkey + reward target.
        assert!(validate_stake_bond_payload(&tx.payload).is_ok());
        let decoded: StakeBondPayload = borsh::from_slice(&tx.payload).unwrap();
        assert_eq!(decoded.amount, 6_000);
        assert_eq!(decoded.validator_pubkey_hash, key.validator_id);
        assert_eq!(decoded.owner_reward_spk_payload, reward);
        assert_eq!(decoded.validator_pubkey, key.keypair.verification_key.as_ref().to_vec());

        // Exact-fit (amount + fee == funding) → no change output.
        let exact = key.build_funded_stake_bond_tx(9_750, 0, 700, reward, funding_outpoint, &funding, 250).unwrap();
        assert_eq!(exact.outputs.len(), 1);
        assert_eq!(exact.outputs[0].value, 9_750);
        // Underfunded (amount + fee > funding) → error; zero amount → error.
        assert!(key.build_funded_stake_bond_tx(10_000, 0, 700, reward, funding_outpoint, &funding, 250).is_err());
        assert!(key.build_funded_stake_bond_tx(0, 0, 700, reward, funding_outpoint, &funding, 250).is_err());
    }

    fn signed_record(epoch: u64, target: u8) -> SignedEpochRecord {
        SignedEpochRecord {
            epoch,
            target_hash: Hash64::from_bytes([target; 64]),
            target_daa_score: epoch * 100,
            signature_fingerprint: Hash64::from_bytes([0u8; 64]),
        }
    }

    #[test]
    fn signed_epoch_store_guard_and_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("validator-state.json");
        let vid = Hash64::from_bytes([0x01u8; 64]);
        let outpoint = TransactionOutpoint::new(Hash64::from_bytes([0x02u8; 64]), 0);

        let mut store = SignedEpochStore::load_or_empty(path.clone(), vid, outpoint).unwrap();
        let a = signed_record(5, 0xaa);
        // First sign for epoch 5 -> Allow, then record.
        assert_eq!(store.check(&a), SignedEpochCheckOutcome::Allow);
        store.record_and_flush(a.clone()).unwrap();
        // Re-signing the same target is rebroadcast-safe; a different target equivocates.
        assert_eq!(store.check(&a), SignedEpochCheckOutcome::AllowRebroadcast);
        assert_eq!(store.check(&signed_record(5, 0xbb)), SignedEpochCheckOutcome::Block);

        // Restart safety: a fresh load from disk must preserve the verdicts.
        let reloaded = SignedEpochStore::load_or_empty(path, vid, outpoint).unwrap();
        assert_eq!(reloaded.check(&a), SignedEpochCheckOutcome::AllowRebroadcast);
        assert_eq!(reloaded.check(&signed_record(5, 0xbb)), SignedEpochCheckOutcome::Block);
        // A different epoch is unconstrained.
        assert_eq!(reloaded.check(&signed_record(6, 0xcc)), SignedEpochCheckOutcome::Allow);
    }

    #[test]
    fn signed_epoch_store_rejects_foreign_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("validator-state.json");
        let outpoint = TransactionOutpoint::new(Hash64::from_bytes([0x02u8; 64]), 0);
        // Validator A writes its log.
        let mut a = SignedEpochStore::load_or_empty(path.clone(), Hash64::from_bytes([0x0au8; 64]), outpoint).unwrap();
        a.record_and_flush(signed_record(1, 0x11)).unwrap();
        // Validator B must refuse to use A's file rather than clobber it.
        assert!(SignedEpochStore::load_or_empty(path, Hash64::from_bytes([0x0bu8; 64]), outpoint).is_err());
    }
}
