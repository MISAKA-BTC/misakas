//! Shared kaspa-pq validator signing primitives (ADR-0010 / ADR-0011).
//!
//! Used by BOTH the in-process `--enable-validator` service in `kaspad` and the
//! standalone `kaspa-pq-validator` sidecar binary, so the two deployment shapes share a
//! single implementation of: the ML-DSA-87 validator key + its derived overlay identity
//! ([`ValidatorKey`]), fee-funded attestation-shard transaction building, and the
//! persistent equivocation-safety log ([`SignedEpochStore`], ADR-0011). No consensus
//! surface — this is a node-local helper crate.

use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::constants::{MAX_TX_IN_SEQUENCE_NUM, TX_VERSION};
use kaspa_consensus_core::dns_finality::{
    ATTESTATION_MLDSA87_CONTEXT, DNS_PAYLOAD_VERSION_V1, SignedEpochCheckOutcome, SignedEpochRecord, StakeAttestation,
    StakeAttestationShardPayload, StakeBondPayload, StakeUnbondRequestPayload, UNBOND_REQUEST_CONTEXT, check_signed_epoch_record,
    single_attestation_shard, unbond_request_message, validator_id_from_pubkey,
};
use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::subnets::{
    SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, SUBNETWORK_ID_STAKE_BOND, SUBNETWORK_ID_STAKE_UNBOND, SubnetworkId,
};
use kaspa_consensus_core::tx::{
    MutableTransaction, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
    UtxoEntry,
};
use kaspa_hashes::{Hash64, blake2b_512_address_payload};
use kaspa_txscript::{
    MLDSA87_SIG_LEN, MLDSA87_TX_CONTEXT, pay_to_address_script, script_builder::ScriptBuilder, script_class::evm_deposit_lock_script,
    verify_mldsa87_with_context,
};
use libcrux_ml_dsa::ml_dsa_87;
use rand::RngCore;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Length in bytes of the ML-DSA-87 keygen seed consumed by [`ValidatorKey::from_seed`]
/// (matches the wallet's `KaspaPqMlDsa87KeyPair`).
pub const VALIDATOR_SEED_LEN: usize = 32;

/// Safety floor (sompi) for an overlay-tx fee — attestation shard / StakeBond / StakeUnbondRequest.
/// It is the minimum the mass-based estimators ([`ValidatorKey::estimate_attestation_fee`],
/// [`ValidatorKey::estimate_bond_fee`], [`ValidatorKey::estimate_unbond_fee`]) ever return, and the
/// value used when a `MassCalculator` is unavailable. The real fee is the relay-rate fee derived
/// from the transaction's compute mass — see [`relay_fee_for_compute_mass`], which mirrors the
/// node's `minimum_required_transaction_relay_fee` at the kaspa-pq production rate (10× compute
/// mass); for these payload-heavy ML-DSA txs (2592-byte pubkey, 4627-byte sig) that lands at
/// ≈ 272 000–319 000 sompi, all comfortably above this floor (so on the normal path the floor never
/// bites — it only guards the rare fallback path and any caller that uses the flat constant
/// directly).
///
/// Set to 250 000 (was a flat 30 000): the live devnet mempool minimum for an attestation shard is
/// ≈ 232 600 sompi, so a 30 000 fallback was ~8× too low and got **rejected as under-fee**
/// (`fees 30000 … under the required amount of 232600`), wedging any validator that hit the
/// fallback. 250 000 sits above that observed minimum yet below every real mass-based fee, so it can
/// never be the under-fee cause again without over-charging the normal path.
pub const ATTESTATION_TX_FEE_FLOOR_SOMPI: u64 = 250_000;

/// Convert a transaction's non-contextual **compute mass** into the node's minimum relay fee
/// (sompi), matching `minimum_required_transaction_relay_fee` in
/// `kaspa_mining::mempool::check_transaction_standard`:
///
/// ```text
///   min_fee = compute_mass * relay_rate / 1000        (relay_rate in sompi per kilogram of mass)
/// ```
///
/// kaspa-pq's `MiningManager` sets `relay_rate` unconditionally to
/// `PQ_PRODUCTION_MINIMUM_RELAY_TRANSACTION_FEE` (= 10_000 sompi/kg, i.e. fee = 10 × compute_mass) —
/// so the payload-heavy StakeBond / StakeUnbondRequest transactions (2592-byte pubkey, 4627-byte
/// sig) need a fee far above the flat [`ATTESTATION_TX_FEE_FLOOR_SOMPI`]. We mirror that rate here
/// rather than depend on the heavy `kaspa-mining` crate. A 25% margin absorbs a few bytes of size
/// variance (or a node configured with a higher rate); the result is clamped up to the floor.
pub fn relay_fee_for_compute_mass(compute_mass: u64) -> u64 {
    const MEMPOOL_RELAY_FEE_SOMPI_PER_KG: u64 = 10_000; // == kaspa_mining ... PQ_PRODUCTION_MINIMUM_RELAY_TRANSACTION_FEE
    let min_fee = compute_mass.saturating_mul(MEMPOOL_RELAY_FEE_SOMPI_PER_KG) / 1000;
    (min_fee + min_fee / 4).max(ATTESTATION_TX_FEE_FLOOR_SOMPI)
}

/// Sum of the funding UTXO amounts (overflow-checked).
fn sum_funding(fundings: &[(TransactionOutpoint, UtxoEntry)]) -> Result<u64, String> {
    let mut total: u64 = 0;
    for (_, e) in fundings {
        total = total.checked_add(e.amount).ok_or_else(|| "funding total overflows u64".to_string())?;
    }
    Ok(total)
}

const SIGNED_EPOCH_FILE_VERSION: u16 = 1;

/// Refuse secret/safety state that could be redirected through a symlink or read by another local
/// account. These files either prevent equivocation or hold an unrevealed PALW commitment opening,
/// so silently accepting a loose mode is not an operator convenience.
fn require_private_regular_file(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| format!("cannot stat {label} file {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("{label} file {} is not a regular file (symlink/device/fifo refused)", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(format!(
                "{label} file {} is group/world-accessible (mode {mode:o}); restrict it to 0600 (chmod 600)",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Create a deterministic atomic-write temp file without ever following a stale symlink and with
/// owner-only permissions from its first byte. A regular stale temp from a crashed prior flush is
/// safe to replace; every other file type fails closed.
fn create_private_temp_file(path: &Path, label: &str) -> Result<fs::File, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() {
                return Err(format!("{label} temp {} is not a regular file (symlink/device/fifo refused)", path.display()));
            }
            fs::remove_file(path).map_err(|error| format!("cannot replace stale {label} temp {}: {error}", path.display()))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("cannot stat {label} temp {}: {error}", path.display())),
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|error| format!("cannot create {label} temp {}: {error}", path.display()))
}

/// Load a 32-byte ML-DSA-87 seed from a hex file (whitespace-trimmed). The file must
/// contain exactly [`VALIDATOR_SEED_LEN`] bytes as hex, which seeds the deterministic
/// ML-DSA-87 keypair via [`ValidatorKey::from_seed`].
pub fn load_validator_seed(path: &str) -> Result<[u8; VALIDATOR_SEED_LEN], String> {
    // Audit M-02: fail CLOSED on an unsafe seed file (was: warn-only, and followed
    // symlinks). The seed is the validator's ML-DSA-87 signing key — refuse a
    // non-regular file (symlink/device/fifo — `symlink_metadata` does NOT follow
    // the link) and a group/world-readable mode, rather than silently signing with
    // a key any local user could read. Mirrors the misaka-cli EVM key-file guard.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = fs::symlink_metadata(path).map_err(|e| format!("cannot stat validator key file '{path}': {e}"))?;
        if !meta.file_type().is_file() {
            return Err(format!("validator key file '{path}' is not a regular file (symlink/device/fifo refused)"));
        }
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(format!(
                "validator key file '{path}' is group/world-accessible (mode {mode:o}); restrict it to 0600 (chmod 600)"
            ));
        }
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("cannot read validator key file '{path}': {e}"))?;
    let hex = raw.trim();
    let mut seed = [0u8; VALIDATOR_SEED_LEN];
    faster_hex::hex_decode(hex.as_bytes(), &mut seed)
        .map_err(|e| format!("validator key file '{path}' must contain {VALIDATOR_SEED_LEN} bytes as hex: {e}"))?;
    Ok(seed)
}

/// Materialised validator signing key: the ML-DSA-87 keypair plus its derived overlay
/// identity (`validator_id = BLAKE2b-512(public_key)`, per ADR-0008/0012).
///
/// Constructed once at startup from the seed file and held for the validator's lifetime.
pub struct ValidatorKey {
    keypair: ml_dsa_87::MLDSA87KeyPair,
    /// Overlay identity advertised to the network and matched against the bond.
    pub validator_id: Hash64,
}

impl ValidatorKey {
    pub fn from_seed(seed: [u8; VALIDATOR_SEED_LEN]) -> Self {
        let keypair = ml_dsa_87::generate_key_pair(seed);
        let validator_id = validator_id_from_pubkey(keypair.verification_key.as_ref());
        Self { keypair, validator_id }
    }

    /// The raw `MLDSA87_PK_LEN`-byte ML-DSA-87 verification (public) key. Exposed for
    /// the PREA CLI signer, which carries the pubkey verbatim in the F003 v0x02
    /// precompile input (the account binds it to its stored address payload).
    pub fn public_key(&self) -> &[u8] {
        self.keypair.verification_key.as_ref()
    }

    /// The validator's own P2PKH-ML-DSA address — `(prefix, PubKeyHashMlDsa87,
    /// keyed_BLAKE2b-512("kaspa-pq-v2/address/mldsa87", public_key))`. This is the
    /// **spend** address (64-byte keyed BLAKE2b-512 payload — md2 §4.2 / ADR-0019
    /// §8), distinct from the 64-byte overlay `validator_id` (an *unkeyed*
    /// BLAKE2b-512). Funding UTXOs sent here back the attestation-shard
    /// transactions (funding model A).
    pub fn funding_address(&self, prefix: Prefix) -> Address {
        let payload = blake2b_512_address_payload(self.keypair.verification_key.as_ref()).as_bytes();
        Address::new(prefix, Version::PubKeyHashMlDsa87, &payload)
    }

    /// Sign `message` under an explicit ML-DSA-87 `context` (domain separator) with fresh
    /// hedged randomness. Distinct contexts keep attestation signatures
    /// ([`ATTESTATION_MLDSA87_CONTEXT`]) and transaction-input signatures
    /// ([`MLDSA87_TX_CONTEXT`]) in disjoint domains — neither can be replayed as the other.
    pub fn sign_with_context(&self, message: &[u8], context: &[u8]) -> [u8; MLDSA87_SIG_LEN] {
        // audit L: ML-DSA `sign` only fails for an over-long (>255-byte) context; every caller
        // passes a short fixed domain-separator constant, so this precondition turns the
        // (otherwise unreachable) failure into an explicit, clearly-attributed panic rather than
        // an opaque libcrux error. Randomness is hedged; ML-DSA is not randomness-fragile.
        assert!(context.len() <= 255, "ML-DSA signing context must be <= 255 bytes, got {}", context.len());
        let mut randomness = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut randomness);
        let sig = ml_dsa_87::sign(&self.keypair.signing_key, message, context, randomness)
            .expect("ML-DSA-87 sign is infallible for a <= 255-byte context");
        *sig.as_ref()
    }

    /// Sign a stake-attestation `message` digest under [`ATTESTATION_MLDSA87_CONTEXT`].
    /// Verifies via [`verify_mldsa87_with_context`] — the same call the `virtual_processor`
    /// aggregator uses.
    pub fn sign_attestation(&self, message: &[u8]) -> [u8; MLDSA87_SIG_LEN] {
        self.sign_with_context(message, ATTESTATION_MLDSA87_CONTEXT)
    }

    /// Build a fee-funded, signed `StakeAttestationShard` transaction (ADR-0010 step 9,
    /// funding model A). Spends `funding` — a UTXO locked to this key's own P2PKH-ML-DSA
    /// script — to pay the fee, returns the change to the same script, and carries the
    /// borsh-encoded `shard` payload. The single input is signed under
    /// [`MLDSA87_TX_CONTEXT`] over the SIG_HASH_ALL sighash and wrapped as
    /// `<sig ‖ sighash-type> <pubkey>` so it satisfies `OpCheckSigMlDsa87`.
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
        let payload = borsh::to_vec(shard).expect("borsh serialization of a well-formed shard is infallible");
        self.build_funded_overlay_tx(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, payload, funding_outpoint, funding, fee)
    }

    /// Build a fee-funded, signed **overlay** transaction on `subnetwork_id` carrying `payload` — the
    /// generic 1-input/1-output self-spend shape [`Self::build_funded_shard_tx`] uses, parameterized
    /// so it also carries a PALW overlay payload (e.g. a beacon commit/reveal on
    /// [`kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_BEACON_COMMIT`]/`..REVEAL`). One input at the
    /// operator's own P2PKH-ML-DSA `funding` UTXO pays the fee; the change returns to the same script;
    /// `lock_time`/`gas` are 0. The input is signed under [`MLDSA87_TX_CONTEXT`] over the SIG_HASH_ALL
    /// sighash and wrapped as `<sig ‖ sighash-type> <pubkey>` for `OpCheckSigMlDsa87`. `payload` must
    /// already be the final wire bytes (for a beacon tx, the `PALW_BEACON_MLDSA87_CONTEXT`-signed
    /// `borsh(PalwBeaconCommitV1/RevealV1)` — a distinct signature from this funding-input signature).
    ///
    /// `fee` is the caller's choice (mass-based minimum + funding discovery are the caller's job).
    pub fn build_funded_overlay_tx(
        &self,
        subnetwork_id: SubnetworkId,
        payload: Vec<u8>,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        fee: u64,
    ) -> Result<Transaction, String> {
        self.build_funded_overlay_tx_with_outputs_multi(
            subnetwork_id,
            payload,
            Vec::new(),
            &[(funding_outpoint, funding.clone())],
            fee,
        )
    }

    /// Multi-input overlay builder with payload-mandated outputs placed before change.
    ///
    /// Most overlay transactions only burn a fee and therefore use [`Self::build_funded_overlay_tx`].
    /// PALW provider bonds are different: consensus requires output 0 to lock the exact amount and
    /// script declared by the payload. `required_outputs` models that carrier-level requirement; the
    /// builder preserves their order and appends one positive change output back to the funding key.
    /// Every funding input is signed independently under [`MLDSA87_TX_CONTEXT`].
    ///
    /// This helper deliberately does not interpret `payload`. Callers must derive `required_outputs`
    /// from the decoded payload and run the appropriate consensus validator before submission. Keeping
    /// the signing primitive generic lets the standalone sidecar submit new overlay kinds without
    /// duplicating ML-DSA transaction signing, while the consensus validator remains the authority for
    /// payload/output coupling.
    pub fn build_funded_overlay_tx_with_outputs_multi(
        &self,
        subnetwork_id: SubnetworkId,
        payload: Vec<u8>,
        required_outputs: Vec<TransactionOutput>,
        fundings: &[(TransactionOutpoint, UtxoEntry)],
        fee: u64,
    ) -> Result<Transaction, String> {
        self.build_funded_overlay_tx_with_outputs_multi_inner(subnetwork_id, payload, required_outputs, fundings, fee, None)
    }

    /// The staged-operator variant of [`Self::build_funded_overlay_tx_with_outputs_multi`]. It
    /// computes and commits KIP-9 persistent storage mass from the real funding entries and output
    /// values before signing. Callers must still enforce the node's standard/consensus mass ceilings;
    /// this method makes the exact contextual mass available as [`Transaction::mass`].
    pub fn build_funded_overlay_tx_with_outputs_multi_and_storage_mass(
        &self,
        subnetwork_id: SubnetworkId,
        payload: Vec<u8>,
        required_outputs: Vec<TransactionOutput>,
        fundings: &[(TransactionOutpoint, UtxoEntry)],
        fee: u64,
        storage_mass_parameter: u64,
    ) -> Result<Transaction, String> {
        self.build_funded_overlay_tx_with_outputs_multi_inner(
            subnetwork_id,
            payload,
            required_outputs,
            fundings,
            fee,
            Some(storage_mass_parameter),
        )
    }

    fn build_funded_overlay_tx_with_outputs_multi_inner(
        &self,
        subnetwork_id: SubnetworkId,
        payload: Vec<u8>,
        required_outputs: Vec<TransactionOutput>,
        fundings: &[(TransactionOutpoint, UtxoEntry)],
        fee: u64,
        storage_mass_parameter: Option<u64>,
    ) -> Result<Transaction, String> {
        if fundings.is_empty() {
            return Err("overlay transaction needs at least one funding UTXO".to_string());
        }
        let funding_script = fundings[0].1.script_public_key.clone();
        if fundings.iter().any(|(_, entry)| entry.script_public_key != funding_script) {
            return Err("overlay funding UTXOs must all use the same script public key".to_string());
        }
        let total = sum_funding(fundings)?;
        let required_total = required_outputs.iter().try_fold(0u64, |sum, output| {
            sum.checked_add(output.value).ok_or_else(|| "required overlay output total overflows u64".to_string())
        })?;
        let needed = required_total.checked_add(fee).ok_or_else(|| "required output total + fee overflows u64".to_string())?;
        // A positive change output is intentional: it gives an operator a deterministic outpoint to
        // wait for before submitting the next dependency layer (manifest -> leaf -> certificate).
        if total <= needed {
            return Err(format!(
                "funding UTXOs total {total} must exceed required output total {required_total} + fee {fee} to leave change"
            ));
        }

        let inputs: Vec<TransactionInput> =
            fundings.iter().map(|(outpoint, _)| TransactionInput::new(*outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1)).collect();
        let mut outputs = required_outputs;
        outputs.push(TransactionOutput::new(total - needed, funding_script));
        let tx = Transaction::new(TX_VERSION, inputs, outputs, 0, subnetwork_id, 0, payload);

        let entries: Vec<UtxoEntry> = fundings.iter().map(|(_, entry)| entry.clone()).collect();
        if let Some(storage_mass_parameter) = storage_mass_parameter {
            let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
                .calc_contextual_masses(&PopulatedTransaction::new(&tx, entries.clone()))
                .ok_or_else(|| "contextual mass not computable for the overlay transaction".to_string())?
                .storage_mass;
            tx.set_mass(storage_mass);
        }

        // Sighashes are computed over the canonical transaction with every signature script empty.
        let mtx = MutableTransaction::with_entries(tx, entries);
        let reused_mldsa = Mldsa87SigHashReusedValuesUnsync::new();
        let mut scripts = Vec::with_capacity(fundings.len());
        for input_index in 0..fundings.len() {
            let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), input_index, SIG_HASH_ALL, &reused_mldsa);
            let mut sig_data = self.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).to_vec();
            sig_data.push(SIG_HASH_ALL.to_u8());
            let signature_script = ScriptBuilder::new()
                .add_data(&sig_data)
                .map_err(|e| format!("overlay funding sig push failed: {e}"))?
                .add_data(self.keypair.verification_key.as_ref())
                .map_err(|e| format!("overlay funding pubkey push failed: {e}"))?
                .drain();
            scripts.push(signature_script);
        }

        let mut tx = mtx.tx;
        for (input, signature_script) in tx.inputs.iter_mut().zip(scripts) {
            input.signature_script = signature_script;
        }
        Ok(tx)
    }

    /// Mass-based fee (sompi) for a funded overlay tx carrying `payload` on `subnetwork_id` — the same
    /// approach as [`Self::estimate_attestation_fee`], but over the ACTUAL payload (a beacon commit /
    /// reveal is ~4.7 KB, close to an attestation shard, so the fee is comparable). Floors at
    /// [`ATTESTATION_TX_FEE_FLOOR_SOMPI`].
    pub fn estimate_overlay_fee(
        &self,
        mass_calculator: &MassCalculator,
        prefix: Prefix,
        subnetwork_id: SubnetworkId,
        payload: Vec<u8>,
    ) -> u64 {
        self.estimate_overlay_fee_with_outputs_for_inputs(mass_calculator, prefix, subnetwork_id, payload, Vec::new(), 1)
    }

    /// Mass-based fee for the exact overlay carrier shape produced by
    /// [`Self::build_funded_overlay_tx_with_outputs_multi`]. `required_outputs` must be the same ordered
    /// prefix the real transaction will carry; `n_inputs` accounts for aggregating mined coinbase
    /// fragments when one UTXO cannot cover a PALW provider bond.
    pub fn estimate_overlay_fee_with_outputs_for_inputs(
        &self,
        mass_calculator: &MassCalculator,
        prefix: Prefix,
        subnetwork_id: SubnetworkId,
        payload: Vec<u8>,
        required_outputs: Vec<TransactionOutput>,
        n_inputs: usize,
    ) -> u64 {
        let funding_spk = pay_to_address_script(&self.funding_address(prefix));
        let n = n_inputs.max(1);
        let per = u64::MAX / (2 * n as u64);
        let fundings: Vec<(TransactionOutpoint, UtxoEntry)> = (0..n)
            .map(|i| {
                let mut id = [0u8; 64];
                id[0] = i as u8;
                id[1] = (i >> 8) as u8;
                (TransactionOutpoint::new(Hash64::from_bytes(id), 0), UtxoEntry::new(per, funding_spk.clone(), 0, false))
            })
            .collect();
        match self.build_funded_overlay_tx_with_outputs_multi(
            subnetwork_id,
            payload,
            required_outputs,
            &fundings,
            ATTESTATION_TX_FEE_FLOOR_SOMPI,
        ) {
            Ok(tx) => relay_fee_for_compute_mass(mass_calculator.calc_non_contextual_masses(&tx).compute_mass),
            Err(_) => ATTESTATION_TX_FEE_FLOOR_SOMPI,
        }
    }

    /// Build a fee-funded, signed NATIVE transfer that SPLITS one funding UTXO into
    /// `num_outputs` change outputs back to this key's own P2PKH-ML-DSA script — a generic
    /// value-moving transaction used for load generation (each output becomes a fresh
    /// spendable UTXO, so a chain of these fans out into many transactions). The KIP-9
    /// storage mass is committed (value-based, so it matches the node's `calc_contextual_masses`
    /// recheck), and input-0 is ML-DSA-signed over the `SIG_HASH_ALL` v2 sighash under
    /// [`MLDSA87_TX_CONTEXT`] exactly as [`Self::build_funded_shard_tx`].
    pub fn build_funded_split_tx(
        &self,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        fee: u64,
        num_outputs: usize,
        storage_mass_parameter: u64,
    ) -> Result<Transaction, String> {
        let n = (num_outputs.max(1)) as u64;
        if funding.amount <= fee {
            return Err(format!("funding UTXO amount {} does not cover fee {}", funding.amount, fee));
        }
        let spendable = funding.amount - fee;
        let per = spendable / n;
        if per == 0 {
            return Err(format!("funding {} too small to split into {} outputs after fee {}", funding.amount, n, fee));
        }
        // K outputs back to self; the division remainder is folded into output-0.
        let spk = funding.script_public_key.clone();
        let remainder = spendable - per * n;
        let outputs: Vec<TransactionOutput> =
            (0..n).map(|i| TransactionOutput::new(if i == 0 { per + remainder } else { per }, spk.clone())).collect();
        let input = TransactionInput::new(funding_outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1);
        let tx = Transaction::new(TX_VERSION, vec![input], outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);

        // KIP-9 storage-mass commitment (value-based, independent of the empty signature script).
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![funding.clone()]))
            .ok_or_else(|| "contextual mass not computable for the split tx".to_string())?
            .storage_mass;
        tx.set_mass(storage_mass);

        let mtx = MutableTransaction::with_entries(tx, vec![funding.clone()]);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &reused);
        let mut sig_data = self.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).to_vec();
        sig_data.push(SIG_HASH_ALL.to_u8());
        let signature_script = ScriptBuilder::new()
            .add_data(&sig_data)
            .map_err(|e| format!("split funding sig push failed: {e}"))?
            .add_data(self.keypair.verification_key.as_ref())
            .map_err(|e| format!("split funding pubkey push failed: {e}"))?
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
    /// 2592-byte ML-DSA-87 pubkey and the matching `validator_pubkey_hash`/`owner_pubkey_hash`
    /// (both = `validator_id`) are written so any node can verify attestations without a
    /// registry. `owner_reward_spk_payload` is where this bond's rewards are paid — set to the
    /// caller-supplied 64-byte P2PKH-ML-DSA payload (ADR-0019 §8; defaults to the validator's
    /// own funding payload). The single input is signed under [`MLDSA87_TX_CONTEXT`] exactly as
    /// [`Self::build_funded_shard_tx`].
    #[allow(clippy::too_many_arguments)]
    pub fn build_funded_stake_bond_tx(
        &self,
        amount: u64,
        activation_daa_score: u64,
        unbonding_period_blocks: u64,
        owner_reward_spk_payload: [u8; 64],
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        fee: u64,
    ) -> Result<Transaction, String> {
        self.build_funded_stake_bond_tx_multi(
            amount,
            activation_daa_score,
            unbonding_period_blocks,
            owner_reward_spk_payload,
            &[(funding_outpoint, funding.clone())],
            fee,
        )
    }

    /// Multi-input variant of [`Self::build_funded_stake_bond_tx`]: fund the bond from SEVERAL
    /// mature UTXOs at this key's own funding address. Mining pays the funding address as many
    /// ~subsidy-sized coinbase fragments, so a single UTXO rarely covers `amount + fee`; the `bond`
    /// CLI aggregates the largest mature ones here. All `fundings` MUST be at this key's funding
    /// script (self-spend); each input is signed independently under [`MLDSA87_TX_CONTEXT`].
    /// output-0 is the locked stake (== `amount`); the remainder (Σ funding − amount − fee) is a
    /// single change output back to the funding script. The caller keeps the input count within the
    /// block mass limit (each ML-DSA-87 input adds a ~2592-byte pubkey + ~4627-byte signature).
    #[allow(clippy::too_many_arguments)]
    pub fn build_funded_stake_bond_tx_multi(
        &self,
        amount: u64,
        activation_daa_score: u64,
        unbonding_period_blocks: u64,
        owner_reward_spk_payload: [u8; 64],
        fundings: &[(TransactionOutpoint, UtxoEntry)],
        fee: u64,
    ) -> Result<Transaction, String> {
        if amount == 0 {
            return Err("stake-bond amount must be > 0".to_string());
        }
        if fundings.is_empty() {
            return Err("stake-bond needs at least one funding UTXO".to_string());
        }
        let needed = amount.checked_add(fee).ok_or_else(|| "amount + fee overflows u64".to_string())?;
        let mut total: u64 = 0;
        for (_, e) in fundings {
            total = total.checked_add(e.amount).ok_or_else(|| "funding total overflows u64".to_string())?;
        }
        if total < needed {
            return Err(format!("funding UTXOs total {total} does not cover amount {amount} + fee {fee}"));
        }
        // validator_id = BLAKE2b-512(pubkey) is both the owner and validator identity for a
        // self-bonded validator; the 64-byte reward payload is a separate spend target.
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

        // All fundings are at this key's own funding script (self-spend), so change goes back there.
        let spk = fundings[0].1.script_public_key.clone();
        let inputs: Vec<TransactionInput> =
            fundings.iter().map(|(op, _)| TransactionInput::new(*op, vec![], MAX_TX_IN_SEQUENCE_NUM, 1)).collect();
        // output-0 MUST be the locked stake (value == amount); change (if any) follows.
        let mut outputs = vec![TransactionOutput::new(amount, spk.clone())];
        let change = total - needed;
        if change > 0 {
            outputs.push(TransactionOutput::new(change, spk));
        }
        let tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_STAKE_BOND, 0, payload);

        // Sighash over the canonical (empty-sig-script) tx for EACH input, then fill the scripts.
        // Every input spends the same self funding script, so all are signed with this key.
        let entries: Vec<UtxoEntry> = fundings.iter().map(|(_, e)| e.clone()).collect();
        let mtx = MutableTransaction::with_entries(tx, entries);
        let reused_mldsa = Mldsa87SigHashReusedValuesUnsync::new();
        let mut sig_scripts = Vec::with_capacity(fundings.len());
        for i in 0..fundings.len() {
            let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), i, SIG_HASH_ALL, &reused_mldsa);
            let mut sig_data = self.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).to_vec();
            sig_data.push(SIG_HASH_ALL.to_u8());
            let signature_script = ScriptBuilder::new()
                .add_data(&sig_data)
                .map_err(|e| format!("stake-bond funding sig push failed: {e}"))?
                .add_data(self.keypair.verification_key.as_ref())
                .map_err(|e| format!("stake-bond funding pubkey push failed: {e}"))?
                .drain();
            sig_scripts.push(signature_script);
        }
        let mut tx = mtx.tx;
        for (i, script) in sig_scripts.into_iter().enumerate() {
            tx.inputs[i].signature_script = script;
        }
        Ok(tx)
    }

    /// Build a fee-funded, signed NATIVE SEND: spend `fundings` (all at this key's
    /// own funding script — a self-spend) into output-0 = `amount` to
    /// `recipient_spk`, output-1 = change back to self (emitted only when > 0).
    /// Plain native subnetwork, no payload, KIP-9 storage mass committed. Each
    /// input is signed independently under [`MLDSA87_TX_CONTEXT`] — the SAME proven
    /// path as [`Self::build_funded_stake_bond_tx_multi`] (only the outputs +
    /// subnetwork differ), so signature validity is inherited from the bond path.
    pub fn build_funded_send_tx(
        &self,
        recipient_spk: ScriptPublicKey,
        amount: u64,
        fundings: &[(TransactionOutpoint, UtxoEntry)],
        fee: u64,
        storage_mass_parameter: u64,
    ) -> Result<Transaction, String> {
        if amount == 0 {
            return Err("send amount must be > 0".to_string());
        }
        if fundings.is_empty() {
            return Err("send needs at least one funding UTXO".to_string());
        }
        let needed = amount.checked_add(fee).ok_or_else(|| "amount + fee overflows u64".to_string())?;
        let total = sum_funding(fundings)?;
        if total < needed {
            return Err(format!("funding UTXOs total {total} does not cover amount {amount} + fee {fee}"));
        }
        let self_spk = fundings[0].1.script_public_key.clone();
        let mut outputs = vec![TransactionOutput::new(amount, recipient_spk)];
        let change = total - needed;
        if change > 0 {
            outputs.push(TransactionOutput::new(change, self_spk));
        }
        self.sign_native_multi(fundings, outputs, storage_mass_parameter)
    }

    /// Build a fee-funded, signed NATIVE CONSOLIDATE: spend `fundings` (all at this
    /// key's own funding script) into a SINGLE self-output of `Σ inputs − fee`.
    /// Merges many small UTXOs into one — the large-UTXO remedy. Same proven
    /// per-input signing as the send/bond path.
    pub fn build_funded_consolidate_tx(
        &self,
        fundings: &[(TransactionOutpoint, UtxoEntry)],
        fee: u64,
        storage_mass_parameter: u64,
    ) -> Result<Transaction, String> {
        if fundings.is_empty() {
            return Err("consolidate needs at least one funding UTXO".to_string());
        }
        let total = sum_funding(fundings)?;
        if total <= fee {
            return Err(format!("funding total {total} does not cover fee {fee}"));
        }
        let self_spk = fundings[0].1.script_public_key.clone();
        let outputs = vec![TransactionOutput::new(total - fee, self_spk)];
        self.sign_native_multi(fundings, outputs, storage_mass_parameter)
    }

    /// Shared tail for native, self-funded multi-input txs: assemble the inputs,
    /// commit the KIP-9 value-based storage mass, then sign EACH input under
    /// [`MLDSA87_TX_CONTEXT`] (all inputs spend the same self funding script).
    fn sign_native_multi(
        &self,
        fundings: &[(TransactionOutpoint, UtxoEntry)],
        outputs: Vec<TransactionOutput>,
        storage_mass_parameter: u64,
    ) -> Result<Transaction, String> {
        let inputs: Vec<TransactionInput> =
            fundings.iter().map(|(op, _)| TransactionInput::new(*op, vec![], MAX_TX_IN_SEQUENCE_NUM, 1)).collect();
        let entries: Vec<UtxoEntry> = fundings.iter().map(|(_, e)| e.clone()).collect();
        let tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, entries.clone()))
            .ok_or_else(|| "contextual mass not computable for the native tx".to_string())?
            .storage_mass;
        tx.set_mass(storage_mass);
        let mtx = MutableTransaction::with_entries(tx, entries);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let mut sig_scripts = Vec::with_capacity(fundings.len());
        for i in 0..fundings.len() {
            let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), i, SIG_HASH_ALL, &reused);
            let mut sig_data = self.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).to_vec();
            sig_data.push(SIG_HASH_ALL.to_u8());
            let signature_script = ScriptBuilder::new()
                .add_data(&sig_data)
                .map_err(|e| format!("native funding sig push failed: {e}"))?
                .add_data(self.keypair.verification_key.as_ref())
                .map_err(|e| format!("native funding pubkey push failed: {e}"))?
                .drain();
            sig_scripts.push(signature_script);
        }
        let mut tx = mtx.tx;
        for (i, script) in sig_scripts.into_iter().enumerate() {
            tx.inputs[i].signature_script = script;
        }
        Ok(tx)
    }

    /// kaspa-pq EVM Lane v0.4 (§7.2 / §9.2): build a funded, signed NATIVE
    /// transaction creating an `EVM_DEPOSIT_LOCK` output — the UTXO side of a
    /// bridge deposit. output-0 is the lock (value == `amount`, script =
    /// [`evm_deposit_lock_script`] binding the EVM credit address, the refund
    /// timeout and the claim tip); change goes back to the funding script. The
    /// lock's refund script is this key's own funding P2PKH, so the depositor
    /// can reclaim after `timeout_daa_score` if no producer claims it. Once
    /// accepted, claim it via `submitEvmDepositClaim(txid, 0)` on a mining
    /// node — the claim executes in an accepting chain block and credits
    /// `(amount − claim_tip) × EVM_NATIVE_SCALE` wei to `evm_address`.
    pub fn build_funded_deposit_lock_tx_multi(
        &self,
        amount: u64,
        evm_address: [u8; 20],
        timeout_daa_score: u64,
        claim_tip_sompi: u64,
        fundings: &[(TransactionOutpoint, UtxoEntry)],
        fee: u64,
    ) -> Result<Transaction, String> {
        if amount == 0 {
            return Err("deposit amount must be > 0".to_string());
        }
        if claim_tip_sompi > amount {
            return Err(format!("claim tip {claim_tip_sompi} exceeds the deposit amount {amount}"));
        }
        if fundings.is_empty() {
            return Err("deposit-lock needs at least one funding UTXO".to_string());
        }
        let needed = amount.checked_add(fee).ok_or_else(|| "amount + fee overflows u64".to_string())?;
        let mut total: u64 = 0;
        for (_, e) in fundings {
            total = total.checked_add(e.amount).ok_or_else(|| "funding total overflows u64".to_string())?;
        }
        if total < needed {
            return Err(format!("funding UTXOs total {total} does not cover amount {amount} + fee {fee}"));
        }

        // All fundings are at this key's own funding script (a standard 69-byte
        // ML-DSA P2PKH — exactly what the lock's refund slot requires).
        let funding_spk = fundings[0].1.script_public_key.clone();
        let lock_spk = evm_deposit_lock_script(evm_address, timeout_daa_score, claim_tip_sompi, funding_spk.script());
        let inputs: Vec<TransactionInput> =
            fundings.iter().map(|(op, _)| TransactionInput::new(*op, vec![], MAX_TX_IN_SEQUENCE_NUM, 1)).collect();
        let mut outputs = vec![TransactionOutput::new(amount, lock_spk)];
        let change = total - needed;
        if change > 0 {
            outputs.push(TransactionOutput::new(change, funding_spk));
        }
        let tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);

        // Sign each self-spend input over the canonical sighash (same loop as the bond builder).
        let entries: Vec<UtxoEntry> = fundings.iter().map(|(_, e)| e.clone()).collect();
        let mtx = MutableTransaction::with_entries(tx, entries);
        let reused_mldsa = Mldsa87SigHashReusedValuesUnsync::new();
        let mut sig_scripts = Vec::with_capacity(fundings.len());
        for i in 0..fundings.len() {
            let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), i, SIG_HASH_ALL, &reused_mldsa);
            let mut sig_data = self.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).to_vec();
            sig_data.push(SIG_HASH_ALL.to_u8());
            let signature_script = ScriptBuilder::new()
                .add_data(&sig_data)
                .map_err(|e| format!("deposit-lock funding sig push failed: {e}"))?
                .add_data(self.keypair.verification_key.as_ref())
                .map_err(|e| format!("deposit-lock funding pubkey push failed: {e}"))?
                .drain();
            sig_scripts.push(signature_script);
        }
        let mut tx = mtx.tx;
        for (i, script) in sig_scripts.into_iter().enumerate() {
            tx.inputs[i].signature_script = script;
        }
        Ok(tx)
    }

    /// Mass-based fee (sompi) for an `n_inputs`-funded deposit-lock tx — the
    /// same dummy-shape approach as [`Self::estimate_bond_fee_for_inputs`].
    pub fn estimate_deposit_lock_fee_for_inputs(&self, mass_calculator: &MassCalculator, prefix: Prefix, n_inputs: usize) -> u64 {
        let funding_spk = pay_to_address_script(&self.funding_address(prefix));
        let n = n_inputs.max(1);
        let per = u64::MAX / (2 * n as u64);
        let fundings: Vec<(TransactionOutpoint, UtxoEntry)> = (0..n)
            .map(|i| {
                let mut id = [0u8; 64];
                id[0] = i as u8;
                id[1] = (i >> 8) as u8;
                (TransactionOutpoint::new(Hash64::from_bytes(id), 0), UtxoEntry::new(per, funding_spk.clone(), 0, false))
            })
            .collect();
        match self.build_funded_deposit_lock_tx_multi(1, [0u8; 20], u64::MAX, 0, &fundings, ATTESTATION_TX_FEE_FLOOR_SOMPI) {
            Ok(tx) => relay_fee_for_compute_mass(mass_calculator.calc_non_contextual_masses(&tx).compute_mass),
            Err(_) => ATTESTATION_TX_FEE_FLOOR_SOMPI,
        }
    }

    /// Build a fee-funded, signed `StakeUnbondRequest` transaction (subnetwork
    /// `SUBNETWORK_ID_STAKE_UNBOND`, ADR-0016 / audit H-05) that begins unbonding the
    /// `StakeBond` at `bond_outpoint`. Accepting it stamps the bond's
    /// `unbond_request_daa_score` (→ `Unbonding`); the bond's locked output-0 then becomes
    /// spendable once `unbond_request_daa_score + unbonding_period_blocks` is reached
    /// (the consensus `bond_spend_gate`).
    ///
    /// `funding` is a UTXO at this key's own P2PKH-ML-DSA funding script — and MUST NOT be the
    /// bond's locked output-0 (the spend-gate keeps that locked until release). It is spent into
    /// a single change output (`funding.amount − fee`) back to the same script so the validator
    /// can fund the next overlay tx. The borsh-encoded [`StakeUnbondRequestPayload`] carries the
    /// owner's authorization, and input-0 carries the funding-spend authorization — two
    /// independent ML-DSA-87 signatures under two distinct domains:
    ///   - the payload `signature` is the owner's authorization over [`unbond_request_message`]
    ///     (`bond_outpoint`) under [`UNBOND_REQUEST_CONTEXT`] — without it any party could grief
    ///     honest validators into `Unbonding` and out of the active set (audit H-05). It carries
    ///     no trailing sighash-type byte: it is verified by the stateful `unbond_request_authorized`
    ///     rule, which also binds the key (`validator_id_from_pubkey(owner_pubkey) ==
    ///     bond.owner_pubkey_hash`).
    ///   - input-0's `signature_script` proves the funding spend, signed over the tx sighash
    ///     under [`MLDSA87_TX_CONTEXT`] exactly as [`Self::build_funded_shard_tx`].
    pub fn build_funded_unbond_tx(
        &self,
        network_id: &[u8],
        bond_outpoint: TransactionOutpoint,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        fee: u64,
    ) -> Result<Transaction, String> {
        if funding.amount <= fee {
            return Err(format!("funding UTXO amount {} does not cover fee {}", funding.amount, fee));
        }
        // Owner authorization: ML-DSA-87 signature over the network- and bond-bound unbond message
        // (audit M-04: `network_id` = the node's genesis hash, prevents cross-network replay) under
        // the unbond context (domain-separated from the tx-spend context). Standalone — no trailing
        // sighash-type byte — since it is the payload's own authorization, not a script signature.
        let auth_bytes = unbond_request_message(network_id, bond_outpoint).as_bytes();
        let auth_sig = self.sign_with_context(&auth_bytes[..], UNBOND_REQUEST_CONTEXT).to_vec();
        let payload = borsh::to_vec(&StakeUnbondRequestPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            bond_outpoint,
            owner_pubkey: self.keypair.verification_key.as_ref().to_vec(),
            signature: auth_sig,
        })
        .expect("borsh serialization of a well-formed unbond request is infallible");

        let spk = funding.script_public_key.clone(); // self-spend; change returns to the funding script
        let input = TransactionInput::new(funding_outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1);
        let change = TransactionOutput::new(funding.amount - fee, spk);
        let tx = Transaction::new(TX_VERSION, vec![input], vec![change], 0, SUBNETWORK_ID_STAKE_UNBOND, 0, payload);

        // Sighash over the canonical (empty-sig-script) tx, then fill input 0's spend script.
        let mtx = MutableTransaction::with_entries(tx, vec![funding.clone()]);
        let reused_mldsa = Mldsa87SigHashReusedValuesUnsync::new();
        let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &reused_mldsa);
        let mut sig_data = self.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).to_vec();
        sig_data.push(SIG_HASH_ALL.to_u8());
        let signature_script = ScriptBuilder::new()
            .add_data(&sig_data)
            .map_err(|e| format!("unbond funding sig push failed: {e}"))?
            .add_data(self.keypair.verification_key.as_ref())
            .map_err(|e| format!("unbond funding pubkey push failed: {e}"))?
            .drain();
        let mut tx = mtx.tx;
        tx.inputs[0].signature_script = signature_script;
        Ok(tx)
    }

    /// The 64-byte P2PKH-ML-DSA reward payload for this key — keyed
    /// `BLAKE2b-512(public_key)` under `kaspa-pq-v2/address/mldsa87` (md2 §4.2 /
    /// ADR-0019 §8), the same payload as [`Self::funding_address`]. Default
    /// `owner_reward_spk_payload` for a self-bonded validator (rewards return to the
    /// validator's own spend address).
    pub fn reward_spk_payload(&self) -> [u8; 64] {
        blake2b_512_address_payload(self.keypair.verification_key.as_ref()).as_bytes()
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
            signature: vec![0u8; MLDSA87_SIG_LEN],
        };
        let shard = single_attestation_shard(dummy);
        let funding = UtxoEntry::new(u64::MAX / 2, funding_spk, 0, false);
        let outpoint = TransactionOutpoint::new(Hash64::from_bytes([0u8; 64]), 0);
        match self.build_funded_shard_tx(&shard, outpoint, &funding, ATTESTATION_TX_FEE_FLOOR_SOMPI) {
            Ok(tx) => relay_fee_for_compute_mass(mass_calculator.calc_non_contextual_masses(&tx).compute_mass),
            Err(_) => ATTESTATION_TX_FEE_FLOOR_SOMPI,
        }
    }

    /// Mass-based fee (sompi) for this validator's `StakeBond` transaction — same approach as
    /// [`Self::estimate_attestation_fee`]. Builds a dummy bond of the real shape (a bond is always
    /// a 2592-byte-pubkey payload + locked output + change output, so the field *sizes* — not the
    /// amount/term *values* — drive the compute mass; a dummy's mass equals the real one's), takes
    /// its non-contextual compute mass (the 1 sompi/gram relay minimum), and clamps up to
    /// [`ATTESTATION_TX_FEE_FLOOR_SOMPI`]. The flat attestation floor is far below a bond's
    /// mempool minimum, so `bond` sizes its fee from the network's mass params via this.
    pub fn estimate_bond_fee(&self, mass_calculator: &MassCalculator, prefix: Prefix) -> u64 {
        self.estimate_bond_fee_for_inputs(mass_calculator, prefix, 1)
    }

    /// Mass-based bond fee for `n_inputs` funding UTXOs. Each ML-DSA-87 input adds a ~2592-byte
    /// pubkey + ~4627-byte signature, so the fee grows materially with the input count; `bond`
    /// recomputes this as it aggregates coinbase fragments. Builds a dummy `n_inputs`-input bond of
    /// the real shape (field *sizes*, not values, drive the mass) and takes its relay fee.
    pub fn estimate_bond_fee_for_inputs(&self, mass_calculator: &MassCalculator, prefix: Prefix, n_inputs: usize) -> u64 {
        let funding_spk = pay_to_address_script(&self.funding_address(prefix));
        let n = n_inputs.max(1);
        let per = u64::MAX / (2 * n as u64); // each dummy big enough that Σ ≥ amount(1) + fee floor
        let fundings: Vec<(TransactionOutpoint, UtxoEntry)> = (0..n)
            .map(|i| {
                let mut id = [0u8; 64];
                id[0] = i as u8;
                id[1] = (i >> 8) as u8;
                (TransactionOutpoint::new(Hash64::from_bytes(id), 0), UtxoEntry::new(per, funding_spk.clone(), 0, false))
            })
            .collect();
        match self.build_funded_stake_bond_tx_multi(1, 0, 0, [0u8; 64], &fundings, ATTESTATION_TX_FEE_FLOOR_SOMPI) {
            Ok(tx) => relay_fee_for_compute_mass(mass_calculator.calc_non_contextual_masses(&tx).compute_mass),
            Err(_) => ATTESTATION_TX_FEE_FLOOR_SOMPI,
        }
    }

    /// Mass-based fee (sompi) for this validator's `StakeUnbondRequest` transaction — same approach
    /// as [`Self::estimate_bond_fee`]. The unbond payload carries the 2592-byte owner pubkey plus a
    /// 4627-byte authorization signature, so its compute mass (and thus this fee) is well above the
    /// flat attestation floor.
    pub fn estimate_unbond_fee(&self, mass_calculator: &MassCalculator, prefix: Prefix) -> u64 {
        let funding_spk = pay_to_address_script(&self.funding_address(prefix));
        let funding = UtxoEntry::new(u64::MAX / 2, funding_spk, 0, false);
        let outpoint = TransactionOutpoint::new(Hash64::from_bytes([0u8; 64]), 0);
        // Dummy bond_outpoint + net_id — the payload's field sizes drive the mass (the ML-DSA-87
        // signature is fixed-length regardless of the message), not the values.
        match self.build_funded_unbond_tx(
            &[0u8; 32],
            TransactionOutpoint::new(Hash64::from_bytes([0u8; 64]), 0),
            outpoint,
            &funding,
            ATTESTATION_TX_FEE_FLOOR_SOMPI,
        ) {
            Ok(tx) => relay_fee_for_compute_mass(mass_calculator.calc_non_contextual_masses(&tx).compute_mass),
            Err(_) => ATTESTATION_TX_FEE_FLOOR_SOMPI,
        }
    }

    /// Verify an attestation signature against this key (local round-trip sanity check).
    pub fn verify_attestation(&self, message: &[u8], signature: &[u8]) -> bool {
        matches!(
            verify_mldsa87_with_context(self.keypair.verification_key.as_ref(), message, signature, ATTESTATION_MLDSA87_CONTEXT),
            Ok(true)
        )
    }

    /// Verify a signature this key produced under an explicit `context`
    /// domain separator (audit M-04: used by the signer to self-check
    /// its own audit-log checkpoint signatures at startup). Returns
    /// `false` on any verification failure or malformed signature.
    pub fn verify_with_context(&self, message: &[u8], signature: &[u8], context: &[u8]) -> bool {
        matches!(verify_mldsa87_with_context(self.keypair.verification_key.as_ref(), message, signature, context), Ok(true))
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
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self { path, validator_id, bond_outpoint, records: BTreeMap::new() });
            }
            Err(error) => return Err(format!("cannot stat validator-state file {}: {error}", path.display())),
            Ok(_) => {}
        }
        require_private_regular_file(&path, "validator-state")?;
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
        self.flush()
    }

    /// Drop every signing record for epochs `<= through_epoch` and flush (only when
    /// something was actually dropped). Keeps `validator-state.json` bounded — without
    /// this the log grows one record per attestation epoch forever AND, because every
    /// sign rewrites the whole file, write amplification is O(lifetime) too.
    ///
    /// SAFETY (this log exists for ADR-0011 equivocation protection): a record for epoch
    /// E only matters while E can still be (re-)offered for signing; the attestation
    /// scheduler walks ready epochs forward within a bounded window, so records far
    /// enough below the newest signed epoch are unreachable. The caller must pass a
    /// horizon comfortably below the current epoch (at least one full ready window).
    /// The NEWEST record is never dropped regardless of the horizon: `last_signed_epoch`
    /// is the restart-safe forward cursor and must survive any prune.
    pub fn prune_through(&mut self, through_epoch: u64) -> Result<(), String> {
        let Some(&newest) = self.records.keys().next_back() else { return Ok(()) };
        let cut = through_epoch.min(newest.saturating_sub(1));
        let before = self.records.len();
        self.records.retain(|epoch, _| *epoch > cut);
        if self.records.len() != before { self.flush() } else { Ok(()) }
    }

    fn flush(&self) -> Result<(), String> {
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
        // Durability (audit H-3): the equivocation log MUST survive a crash. atomic rename alone
        // is not enough — if a written-AND-broadcast record is lost to a crash before it hits
        // stable storage, the validator could re-sign a DIFFERENT anchor for the same epoch on
        // restart (slashable). So fsync the temp file BEFORE the rename, then fsync the parent
        // directory so the new dirent is durable too. Fail-closed on any error.
        {
            let mut f = create_private_temp_file(&tmp, "validator-state")?;
            f.write_all(json.as_bytes()).map_err(|e| format!("cannot write validator-state tmp {}: {e}", tmp.display()))?;
            f.sync_all().map_err(|e| format!("cannot fsync validator-state tmp {}: {e}", tmp.display()))?;
        }
        fs::rename(&tmp, &self.path).map_err(|e| format!("cannot commit validator-state {}: {e}", self.path.display()))?;
        if let Some(parent) = self.path.parent() {
            // Best-effort: persist the rename. Unix fsyncs a directory via an opened handle; other
            // platforms may not support it, in which case the temp-file fsync above still holds.
            if let Ok(dir) = fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }
        Ok(())
    }
}

/// File version for the beacon-secret store (ADR-0039 §11.2).
const BEACON_SECRET_FILE_VERSION: u16 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct BeaconSecretFile {
    version: u16,
    validator_id: Hash64,
    bond_outpoint: TransactionOutpoint,
    /// target_epoch -> the 64-byte secret committed for it (kept until it is revealed).
    secrets: BTreeMap<u64, Vec<u8>>,
}

const TICKET_SECRET_FILE_VERSION: u16 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct TicketSecretFile {
    version: u16,
    /// The ticket authority whose leaves these secrets belong to — a foreign file is refused.
    authority_pk_hash: Hash64,
    /// `(batch_id_hex, leaf_index)` -> the raw ticket nullifier the leaf's commitment opens to.
    secrets: BTreeMap<String, Hash64>,
}

fn ticket_secret_key(batch_id: &Hash64, leaf_index: u32) -> String {
    format!("{batch_id:?}:{leaf_index}")
}

/// kaspa-pq **ADR-0040 (C-1)** — durable store for PALW **ticket nullifiers**, keyed by
/// `(batch_id, leaf_index)`.
///
/// # Why a registered leaf needs a secret file at all
///
/// A PALW leaf publishes `ticket_nullifier_commitment`, never the nullifier. The nullifier is chosen
/// once, at registration time, by grinding it so the leaf's clause-9 draw wins — and after that it is
/// FIXED: clause 1 compares `ticket_nullifier_commitment(disclosed)` against the commitment the
/// on-chain leaf carries, so the leaf gets exactly one nullifier and one draw per interval. There is no
/// re-roll and no derivation from chain state.
///
/// So a node that loses this file loses every leaf it has ever registered. The leaves stay on chain,
/// stay eligible, and can never be mined by anyone — the registration cost is paid and the ticket is
/// permanently dead. That is a worse failure than losing a beacon secret (which stalls one epoch), and
/// it is silent: nothing on chain indicates that the holder can no longer open its own commitment.
///
/// Written with the same atomic temp+fsync+rename discipline as [`BeaconSecretStore`], and created
/// **0600 from the start** — see [`Self::flush`].
pub struct TicketSecretStore {
    path: PathBuf,
    authority_pk_hash: Hash64,
    secrets: BTreeMap<String, Hash64>,
}

impl TicketSecretStore {
    /// Load the ticket-secret store for `authority_pk_hash` from `path`, or start empty if absent.
    /// Refuses a file belonging to a different authority: mixing two authorities' secrets would let a
    /// miner draw with a nullifier whose leaf names a key it does not hold, which clause 7 rejects
    /// after the interval is already spent.
    pub fn load_or_empty(path: PathBuf, authority_pk_hash: Hash64) -> Result<Self, String> {
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self { path, authority_pk_hash, secrets: BTreeMap::new() });
            }
            Err(error) => return Err(format!("cannot stat ticket-secret file {}: {error}", path.display())),
            Ok(_) => {}
        }
        require_private_regular_file(&path, "ticket-secret")?;
        let raw = fs::read_to_string(&path).map_err(|e| format!("cannot read ticket-secret file {}: {e}", path.display()))?;
        let file: TicketSecretFile =
            serde_json::from_str(&raw).map_err(|e| format!("cannot parse ticket-secret file {}: {e}", path.display()))?;
        if file.authority_pk_hash != authority_pk_hash {
            return Err(format!("ticket-secret file {} belongs to a different ticket authority; refusing to use it", path.display()));
        }
        Ok(Self { path, authority_pk_hash, secrets: file.secrets })
    }

    /// The raw nullifier stored for `(batch_id, leaf_index)`, if any.
    pub fn secret_for(&self, batch_id: &Hash64, leaf_index: u32) -> Option<Hash64> {
        self.secrets.get(&ticket_secret_key(batch_id, leaf_index)).copied()
    }

    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    /// Persist the nullifier chosen for `(batch_id, leaf_index)` and flush atomically.
    ///
    /// Call this BEFORE submitting the leaf-chunk registration transaction. A crash between "leaf
    /// registered on chain" and "secret persisted" produces exactly the dead ticket described above, and
    /// the ordering is the only thing that prevents it.
    ///
    /// Refuses to overwrite an existing entry with a different value: a registered leaf's nullifier is
    /// immutable, so a differing write means either a key collision or a caller that re-ground a
    /// registered leaf (the C-1 hazard). Silently replacing it would destroy the only copy of the
    /// secret that still opens the on-chain commitment.
    pub fn record_and_flush(&mut self, batch_id: Hash64, leaf_index: u32, raw_nullifier: Hash64) -> Result<(), String> {
        let key = ticket_secret_key(&batch_id, leaf_index);
        if let Some(existing) = self.secrets.get(&key)
            && *existing != raw_nullifier
        {
            return Err(format!(
                "refusing to overwrite the ticket nullifier for {key}: a registered leaf's nullifier is immutable, \
                 and replacing it would destroy the only value that opens its on-chain commitment"
            ));
        }
        self.secrets.insert(key, raw_nullifier);
        self.flush()
    }

    /// Drop the secret for a leaf that can no longer be mined (expired or already spent), and flush.
    pub fn forget(&mut self, batch_id: &Hash64, leaf_index: u32) -> Result<(), String> {
        if self.secrets.remove(&ticket_secret_key(batch_id, leaf_index)).is_some() { self.flush() } else { Ok(()) }
    }

    fn flush(&self) -> Result<(), String> {
        let file = TicketSecretFile {
            version: TICKET_SECRET_FILE_VERSION,
            authority_pk_hash: self.authority_pk_hash,
            secrets: self.secrets.clone(),
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| format!("cannot serialize ticket-secret store: {e}"))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("cannot create ticket-secret dir {}: {e}", parent.display()))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut f = create_private_temp_file(&tmp, "ticket-secret")?;
            f.write_all(json.as_bytes()).map_err(|e| format!("cannot write ticket-secret tmp {}: {e}", tmp.display()))?;
            f.sync_all().map_err(|e| format!("cannot fsync ticket-secret tmp {}: {e}", tmp.display()))?;
        }
        fs::rename(&tmp, &self.path).map_err(|e| format!("cannot commit ticket-secret store {}: {e}", self.path.display()))?;
        if let Some(parent) = self.path.parent()
            && let Ok(dir) = fs::File::open(parent)
        {
            let _ = dir.sync_all();
        }
        Ok(())
    }
}

/// Durable store for PALW beacon **commit secrets** (ADR-0039 §11.2), keyed by the beacon epoch `E`
/// the secret targets. A commit is carried in epoch `E-2` and its reveal (opening the secret) in
/// `E-1`, so the 64-byte secret must survive at least two epochs AND any node restart between them.
///
/// Losing a committed secret is NOT neutral: the committed bond stake stays in the beacon quorum's
/// denominator (`beacon_quorum_reached` weighs revealed/committed) with nothing in the numerator, which
/// can drop the epoch below the `2/3` threshold and stall `R_E`. So the secret is fsync'd (temp file +
/// rename, mirroring [`SignedEpochStore`]) BEFORE the commit tx is submitted. The `(validator_id,
/// bond_outpoint)` file header refuses a foreign key's file, exactly like the equivocation log.
pub struct BeaconSecretStore {
    path: PathBuf,
    validator_id: Hash64,
    bond_outpoint: TransactionOutpoint,
    secrets: BTreeMap<u64, [u8; 64]>,
}

impl BeaconSecretStore {
    /// Load the beacon-secret store for `(validator_id, bond_outpoint)` from `path`, or start empty if
    /// absent. Errors if the file exists but belongs to a different validator/bond, or holds a
    /// mis-sized secret.
    pub fn load_or_empty(path: PathBuf, validator_id: Hash64, bond_outpoint: TransactionOutpoint) -> Result<Self, String> {
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self { path, validator_id, bond_outpoint, secrets: BTreeMap::new() });
            }
            Err(error) => return Err(format!("cannot stat beacon-secret file {}: {error}", path.display())),
            Ok(_) => {}
        }
        require_private_regular_file(&path, "beacon-secret")?;
        let raw = fs::read_to_string(&path).map_err(|e| format!("cannot read beacon-secret file {}: {e}", path.display()))?;
        let file: BeaconSecretFile =
            serde_json::from_str(&raw).map_err(|e| format!("cannot parse beacon-secret file {}: {e}", path.display()))?;
        if file.validator_id != validator_id || file.bond_outpoint != bond_outpoint {
            return Err(format!("beacon-secret file {} belongs to a different validator/bond; refusing to use it", path.display()));
        }
        let mut secrets = BTreeMap::new();
        for (epoch, bytes) in file.secrets {
            let arr: [u8; 64] = bytes
                .try_into()
                .map_err(|_| format!("beacon-secret file {} has a mis-sized secret for epoch {epoch}", path.display()))?;
            secrets.insert(epoch, arr);
        }
        Ok(Self { path, validator_id, bond_outpoint, secrets })
    }

    /// The secret committed for beacon epoch `target_epoch`, if one is stored.
    pub fn secret_for(&self, target_epoch: u64) -> Option<[u8; 64]> {
        self.secrets.get(&target_epoch).copied()
    }

    /// Whether a secret is stored for `target_epoch`.
    pub fn has_secret(&self, target_epoch: u64) -> bool {
        self.secrets.contains_key(&target_epoch)
    }

    /// Number of stored (unrevealed) secrets.
    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    /// True if no secrets are stored.
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    /// Persist the `secret` committed for `target_epoch` and flush atomically (fsync temp + rename).
    /// Call BEFORE submitting the commit tx, so a crash between commit and reveal cannot lose it.
    pub fn record_and_flush(&mut self, target_epoch: u64, secret: [u8; 64]) -> Result<(), String> {
        self.secrets.insert(target_epoch, secret);
        self.flush()
    }

    /// Drop every secret whose target epoch is `<= through_epoch` (already revealed or expired) and
    /// flush. Keeps the store bounded.
    pub fn prune_through(&mut self, through_epoch: u64) -> Result<(), String> {
        let before = self.secrets.len();
        self.secrets.retain(|epoch, _| *epoch > through_epoch);
        if self.secrets.len() != before { self.flush() } else { Ok(()) }
    }

    fn flush(&self) -> Result<(), String> {
        let file = BeaconSecretFile {
            version: BEACON_SECRET_FILE_VERSION,
            validator_id: self.validator_id,
            bond_outpoint: self.bond_outpoint,
            secrets: self.secrets.iter().map(|(e, s)| (*e, s.to_vec())).collect(),
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| format!("cannot serialize beacon-secret store: {e}"))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("cannot create beacon-secret dir {}: {e}", parent.display()))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut f = create_private_temp_file(&tmp, "beacon-secret")?;
            f.write_all(json.as_bytes()).map_err(|e| format!("cannot write beacon-secret tmp {}: {e}", tmp.display()))?;
            f.sync_all().map_err(|e| format!("cannot fsync beacon-secret tmp {}: {e}", tmp.display()))?;
        }
        fs::rename(&tmp, &self.path).map_err(|e| format!("cannot commit beacon-secret store {}: {e}", self.path.display()))?;
        if let Some(parent) = self.path.parent()
            && let Ok(dir) = fs::File::open(parent)
        {
            let _ = dir.sync_all();
        }
        Ok(())
    }
}

/// Whether a funding UTXO can be spent right now. A coinbase output is locked until
/// `coinbase_maturity` blocks have passed since it was mined (consensus rule); a non-coinbase
/// output is always spendable. `virtual_daa` is the node's current virtual DAA score. Saturating
/// so a (transient) `block_daa_score > virtual_daa` reads as "not yet mature". Takes raw fields
/// (not a typed entry) so it works for both `UtxoEntry` and the RPC `RpcUtxoEntry` (same fields).
pub fn is_spendable(is_coinbase: bool, block_daa_score: u64, virtual_daa: u64, coinbase_maturity: u64) -> bool {
    if !is_coinbase {
        return true;
    }
    virtual_daa.saturating_sub(block_daa_score) >= coinbase_maturity
}

/// Choose the funding input for the next attestation tx. Prefers the local funding-chain head (our
/// previous change output, still unconfirmed in the node's utxoindex view) so we never re-select a
/// UTXO our own in-flight tx already spent — the cause of "output … already spent … in the mempool".
/// Falls back to the largest MATURE node UTXO not already spent in flight. Pure (no I/O); the caller
/// resyncs a mined chain head (`pending_change`) and prunes `inflight_spent` against the node's
/// current set before calling. Shared by the standalone `kaspa-pq-validator` daemon and the
/// in-process `--enable-validator` service so both funding paths behave identically.
pub fn select_funding(
    pending_change: &Option<(TransactionOutpoint, UtxoEntry)>,
    inflight_spent: &HashSet<TransactionOutpoint>,
    node_utxos: Vec<(TransactionOutpoint, UtxoEntry)>,
    fee: u64,
    virtual_daa: u64,
    coinbase_maturity: u64,
) -> Result<(TransactionOutpoint, UtxoEntry), String> {
    // Chain off our own unconfirmed change while it still covers the fee (the mempool accepts a
    // chained spend of an unconfirmed parent output).
    if let Some((head, entry)) = pending_change
        && entry.amount > fee
    {
        return Ok((*head, entry.clone()));
    }
    // Otherwise pick the largest mature node UTXO we have not already spent in flight. Skipping
    // immature coinbase UTXOs avoids the consensus "spends an immature UTXO" rejection.
    node_utxos
        .into_iter()
        .filter(|(op, en)| {
            en.amount > fee
                && is_spendable(en.is_coinbase, en.block_daa_score, virtual_daa, coinbase_maturity)
                && !inflight_spent.contains(op)
        })
        .max_by_key(|(_, en)| en.amount)
        .ok_or_else(|| {
            format!(
                "no MATURE funding UTXO > {fee} sompi at the validator funding address; \
                 send funds there and wait for coinbase maturity ({coinbase_maturity} blocks)"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, Params, SIMNET_PARAMS, TESTNET_PARAMS};
    use kaspa_consensus_core::tx::ScriptPublicKey;
    use std::io::Write;

    // ---- funding selection (shared by the daemon + the in-process service) ----

    fn fop(seed: u8, idx: u32) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([seed; 64]), idx)
    }
    fn fentry(amount: u64, daa: u64, coinbase: bool) -> UtxoEntry {
        UtxoEntry::new(amount, ScriptPublicKey::default(), daa, coinbase)
    }
    const SF_FEE: u64 = 250_000;
    const SF_MATURITY: u64 = 100;
    const SF_VDAA: u64 = 10_000;

    #[test]
    fn is_spendable_respects_coinbase_maturity() {
        let maturity = 1000;
        assert!(!is_spendable(true, 5000, 5500, maturity), "depth 500 < 1000 → immature");
        assert!(!is_spendable(true, 5000, 5999, maturity), "depth 999 < 1000 → immature");
        assert!(is_spendable(true, 5000, 6000, maturity), "depth exactly 1000 → mature");
        assert!(is_spendable(true, 5000, 9000, maturity), "depth 4000 → mature");
        assert!(!is_spendable(true, 6000, 5000, maturity), "future coinbase reads as not-yet-mature");
        assert!(is_spendable(false, 5999, 6000, maturity), "non-coinbase always spendable");
        assert!(is_spendable(false, 6000, 6000, maturity));
    }

    #[test]
    fn select_funding_chains_off_unconfirmed_change() {
        // The chain head (our previous change) is preferred over node UTXOs, so we never re-pick a
        // funding UTXO the node still lists but our in-flight tx already spent.
        let head = fop(0x11, 0);
        let pending = Some((head, fentry(1_000_000, SF_VDAA, false)));
        let node = vec![(fop(0x22, 0), fentry(5_000_000, 0, false))]; // bigger, but a node UTXO
        let (sel_op, sel_en) = select_funding(&pending, &HashSet::new(), node, SF_FEE, SF_VDAA, SF_MATURITY).unwrap();
        assert_eq!(sel_op, head, "must spend the unconfirmed change head, not the node UTXO");
        assert_eq!(sel_en.amount, 1_000_000);
    }

    #[test]
    fn select_funding_skips_depleted_chain_head() {
        // A chain head that can no longer cover the fee falls back to the node view.
        let pending = Some((fop(0x11, 0), fentry(SF_FEE, SF_VDAA, false))); // amount == fee → not > fee
        let node = vec![(fop(0x22, 0), fentry(3_000_000, 0, false))];
        let (sel_op, _) = select_funding(&pending, &HashSet::new(), node, SF_FEE, SF_VDAA, SF_MATURITY).unwrap();
        assert_eq!(sel_op, fop(0x22, 0), "depleted head → use the node UTXO");
    }

    #[test]
    fn select_funding_excludes_inflight_and_picks_largest() {
        // The fallback excludes outpoints we already spent in flight and picks the largest survivor.
        let spent = fop(0x33, 0);
        let big = fop(0x44, 0);
        let small = fop(0x55, 0);
        let node = vec![
            (spent, fentry(9_000_000, 0, false)), // largest, but in-flight-spent → excluded
            (big, fentry(4_000_000, 0, false)),
            (small, fentry(1_000_000, 0, false)),
        ];
        let inflight: HashSet<TransactionOutpoint> = [spent].into_iter().collect();
        let (sel_op, sel_en) = select_funding(&None, &inflight, node, SF_FEE, SF_VDAA, SF_MATURITY).unwrap();
        assert_eq!(sel_op, big, "largest non-excluded UTXO");
        assert_eq!(sel_en.amount, 4_000_000);
    }

    #[test]
    fn select_funding_skips_immature_coinbase_and_underfunded() {
        // Immature coinbase (depth < maturity) and amount <= fee are both filtered out.
        let immature = (fop(0x66, 0), fentry(8_000_000, SF_VDAA, true)); // depth 0 < 100 → immature
        let underfunded = (fop(0x77, 0), fentry(SF_FEE, 0, false)); // amount == fee → not > fee
        let good = (fop(0x88, 0), fentry(2_000_000, 0, false));
        let node = vec![immature, underfunded, good];
        let (sel_op, _) = select_funding(&None, &HashSet::new(), node, SF_FEE, SF_VDAA, SF_MATURITY).unwrap();
        assert_eq!(sel_op, fop(0x88, 0), "only the mature, sufficiently-funded UTXO qualifies");
    }

    #[test]
    fn select_funding_errors_when_no_candidate() {
        // No chain head and every node UTXO excluded/ineligible → a descriptive error, no panic.
        let spent = fop(0x99, 0);
        let node = vec![(spent, fentry(5_000_000, 0, false))];
        let inflight: HashSet<TransactionOutpoint> = [spent].into_iter().collect();
        let err = select_funding(&None, &inflight, node, SF_FEE, SF_VDAA, SF_MATURITY).unwrap_err();
        assert!(err.contains("no MATURE funding UTXO"), "got: {err}");
    }

    #[test]
    fn load_validator_seed_accepts_32_byte_hex() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let seed_hex = "11".repeat(VALIDATOR_SEED_LEN); // 32 bytes of 0x11
        writeln!(f, "  {seed_hex}").unwrap();
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
    fn funding_address_is_p2pkh_mldsa87_over_blake2b_512_pubkey() {
        let key = ValidatorKey::from_seed([0x44u8; VALIDATOR_SEED_LEN]);
        let addr = key.funding_address(Prefix::Devnet);
        assert_eq!(addr.version, Version::PubKeyHashMlDsa87);
        assert_eq!(addr.prefix, Prefix::Devnet);
        // Payload = keyed BLAKE2b-512(pubkey) under `kaspa-pq-v2/address/mldsa87`
        // (md2 §4.2) — the 64-byte spend hash; the overlay validator_id is an
        // unkeyed BLAKE2b-512, not this value.
        let expected = blake2b_512_address_payload(key.keypair.verification_key.as_ref()).as_bytes();
        assert_eq!(addr.payload.as_slice(), &expected);
    }

    #[test]
    fn sign_attestation_roundtrip_and_tamper() {
        let key = ValidatorKey::from_seed([0x55u8; VALIDATOR_SEED_LEN]);
        let msg = [0x99u8; 32]; // stand-in for a stake_attestation_message digest
        let sig = key.sign_attestation(&msg);
        assert_eq!(sig.len(), MLDSA87_SIG_LEN);
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
        let sig = key.sign_with_context(&msg, MLDSA87_TX_CONTEXT);
        let pk = key.keypair.verification_key.as_ref();
        // Verifies under the tx context...
        assert!(matches!(verify_mldsa87_with_context(pk, &msg, &sig, MLDSA87_TX_CONTEXT), Ok(true)));
        // ...but NOT under the attestation context (domain separation).
        assert!(!matches!(verify_mldsa87_with_context(pk, &msg, &sig, ATTESTATION_MLDSA87_CONTEXT), Ok(true)));
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
            validator_set_commitment: Hash64::from_bytes([0u8; 64]), // ADR-0017: VSC is a fixed-zero wire invariant (sortition committee dropped)
            signature: vec![0u8; MLDSA87_SIG_LEN],
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

    fn funded_single_attestation_shard_mass(params: &Params) -> u64 {
        let key = ValidatorKey::from_seed([0x77u8; VALIDATOR_SEED_LEN]);
        let shard = single_attestation_shard(StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: key.validator_id,
            bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([0x01u8; 64]), 0),
            epoch: 7,
            target_hash: Hash64::from_bytes([0x11u8; 64]),
            target_daa_score: 700,
            validator_set_commitment: Hash64::from_bytes([0u8; 64]),
            signature: vec![0u8; MLDSA87_SIG_LEN],
        });
        let funding_spk = pay_to_address_script(&key.funding_address(Prefix::Testnet));
        let funding = UtxoEntry::new(10_000_000, funding_spk, 1, false);
        let funding_outpoint = TransactionOutpoint::new(Hash64::from_bytes([0x99u8; 64]), 3);
        let tx = key.build_funded_shard_tx(&shard, funding_outpoint, &funding, ATTESTATION_TX_FEE_FLOOR_SOMPI).unwrap();
        MassCalculator::new(
            params.mass_per_tx_byte,
            params.mass_per_script_pub_key_byte,
            params.mass_per_sig_op,
            params.storage_mass_parameter,
        )
        .calc_non_contextual_masses(&tx)
        .max()
    }

    #[test]
    fn funded_single_attestation_shard_mass_fits_all_dns_param_caps() {
        for (name, params) in
            [("mainnet", &MAINNET_PARAMS), ("testnet-10", &TESTNET_PARAMS), ("devnet", &DEVNET_PARAMS), ("simnet", &SIMNET_PARAMS)]
        {
            let Some(dns_params) = params.dns_params.as_ref() else {
                continue;
            };
            let mass = funded_single_attestation_shard_mass(params);
            assert!(
                mass <= dns_params.max_attestation_shard_mass,
                "{name} funded single-attestation shard mass {mass} exceeds cap {}",
                dns_params.max_attestation_shard_mass
            );
        }
    }

    #[test]
    fn build_funded_unbond_tx_structure_and_auth() {
        use kaspa_consensus_core::dns_finality::validate_stake_unbond_payload;
        use kaspa_consensus_core::tx::ScriptPublicKey;

        let key = ValidatorKey::from_seed([0x33u8; VALIDATOR_SEED_LEN]);
        let bond_outpoint = TransactionOutpoint::new(Hash64::from_bytes([0x07u8; 64]), 0);
        let funding_spk = ScriptPublicKey::default();
        let funding = UtxoEntry::new(1_000, funding_spk.clone(), 1, false);
        let funding_outpoint = TransactionOutpoint::new(Hash64::from_bytes([0x44u8; 64]), 2);
        let net_id: &[u8] = &[0x55u8; 32]; // audit M-04: the network the unbond authorizes on

        let tx = key.build_funded_unbond_tx(net_id, bond_outpoint, funding_outpoint, &funding, 250).unwrap();
        assert_eq!(tx.inputs.len(), 1);
        assert_eq!(tx.inputs[0].previous_outpoint, funding_outpoint);
        assert!(!tx.inputs[0].signature_script.is_empty()); // funding spend signed
        assert_eq!(tx.outputs.len(), 1);
        assert_eq!(tx.outputs[0].value, 750); // funding − fee, change back to self
        assert_eq!(tx.outputs[0].script_public_key, funding_spk);
        assert_eq!(tx.subnetwork_id, SUBNETWORK_ID_STAKE_UNBOND);
        assert_eq!(tx.gas, 0);

        // Payload decodes + passes stateless validation, carries the requested bond_outpoint,
        // and binds THIS validator's key (its derived overlay id matches).
        assert!(validate_stake_unbond_payload(&tx.payload).is_ok());
        let req: StakeUnbondRequestPayload = borsh::from_slice(&tx.payload).unwrap();
        assert_eq!(req.bond_outpoint, bond_outpoint);
        assert_eq!(validator_id_from_pubkey(&req.owner_pubkey), key.validator_id);

        // The owner authorization signature verifies over the network- and bond-bound message under
        // the unbond context — and is bound to THIS (network, bond) pair.
        let auth_bytes = unbond_request_message(net_id, bond_outpoint).as_bytes();
        assert!(matches!(
            verify_mldsa87_with_context(&req.owner_pubkey, &auth_bytes[..], &req.signature, UNBOND_REQUEST_CONTEXT),
            Ok(true)
        ));
        // Bond-binding: a DIFFERENT bond (same network) must not verify.
        let other_bond = unbond_request_message(net_id, TransactionOutpoint::new(Hash64::from_bytes([0x08u8; 64]), 0)).as_bytes();
        assert!(!matches!(
            verify_mldsa87_with_context(&req.owner_pubkey, &other_bond[..], &req.signature, UNBOND_REQUEST_CONTEXT),
            Ok(true)
        ));
        // audit M-04 — network-binding: the SAME bond on a DIFFERENT network must not verify
        // (cross-network replay of the unbond authorization is prevented).
        let other_net = unbond_request_message(&[0xAAu8; 32], bond_outpoint).as_bytes();
        assert!(!matches!(
            verify_mldsa87_with_context(&req.owner_pubkey, &other_net[..], &req.signature, UNBOND_REQUEST_CONTEXT),
            Ok(true)
        ));

        // Fee must be strictly less than the funding amount.
        assert!(key.build_funded_unbond_tx(net_id, bond_outpoint, funding_outpoint, &funding, 1_000).is_err());
    }

    #[test]
    fn mass_based_bond_and_unbond_fees_exceed_the_flat_floor() {
        // StakeBond / StakeUnbondRequest carry the 2592-byte ML-DSA-87 pubkey (+ a 4627-byte sig),
        // so a mass-based fee (≈ 272 000 / 319 000 sompi) stays above the safety floor even after it
        // was raised to 250 000 — that gap is exactly why the bond/unbond commands estimate from the
        // network mass params instead of pinning the floor.
        let key = ValidatorKey::from_seed([0x5au8; VALIDATOR_SEED_LEN]);
        // kaspa-pq mass params (mass_per_sig_op = 10_000 per the Phase-7 recalibration).
        let mc = MassCalculator::new(1, 10, 10_000, 10_000_000_000);
        let bond_fee = key.estimate_bond_fee(&mc, Prefix::Testnet);
        let unbond_fee = key.estimate_unbond_fee(&mc, Prefix::Testnet);
        assert!(
            bond_fee > ATTESTATION_TX_FEE_FLOOR_SOMPI,
            "mass-based bond fee {bond_fee} must exceed the flat floor {ATTESTATION_TX_FEE_FLOOR_SOMPI}"
        );
        assert!(
            unbond_fee > ATTESTATION_TX_FEE_FLOOR_SOMPI,
            "mass-based unbond fee {unbond_fee} must exceed the flat floor {ATTESTATION_TX_FEE_FLOOR_SOMPI}"
        );
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::symlink_metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode & 0o077, 0, "validator-state must be owner-only (mode {mode:o})");
        }
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

    // ---- Phase 6: funded PALW overlay tx builder + beacon-secret store ----

    fn funding_at(key: &ValidatorKey, amount: u64) -> UtxoEntry {
        UtxoEntry::new(amount, pay_to_address_script(&key.funding_address(Prefix::Testnet)), 0, false)
    }

    /// The generalized overlay builder wraps a PALW beacon-commit payload on subnetwork 0x35 into a
    /// funded tx the stateless consensus validator accepts, with the fee taken from the funding UTXO.
    #[test]
    fn build_funded_overlay_tx_wraps_a_valid_beacon_commit() {
        use kaspa_consensus_core::palw::{PalwBeaconCommitV1, validate_palw_overlay_payload};
        use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_BEACON_COMMIT;

        let key = ValidatorKey::from_seed([0x11; 32]);
        let funding = funding_at(&key, 10_000_000);
        // A beacon commit with a correctly-SIZED signature (validate_palw_overlay_payload length-checks it).
        let commit = PalwBeaconCommitV1 {
            version: 1,
            epoch: 12,
            bond_outpoint: fop(6, 0),
            commitment: Hash64::from_bytes([9; 64]),
            signature: vec![0x55; MLDSA87_SIG_LEN],
        };
        let payload = borsh::to_vec(&commit).unwrap();
        let fee = 300_000;
        let tx = key.build_funded_overlay_tx(SUBNETWORK_ID_PALW_BEACON_COMMIT, payload.clone(), fop(7, 0), &funding, fee).unwrap();

        assert_eq!(tx.subnetwork_id, SUBNETWORK_ID_PALW_BEACON_COMMIT);
        assert_eq!(tx.payload, payload);
        // The exact stateless check the mempool / body validator runs on a 0x35 tx accepts it.
        assert_eq!(validate_palw_overlay_payload(0x35, &tx.payload), Ok(()));
        // One funding input (now signed) and one change output = funding - fee back to the same script.
        assert_eq!(tx.inputs.len(), 1);
        assert!(!tx.inputs[0].signature_script.is_empty(), "the funding input is signed");
        assert_eq!(tx.outputs.len(), 1);
        assert_eq!(tx.outputs[0].value, funding.amount - fee);
        assert_eq!(tx.outputs[0].script_public_key, funding.script_public_key);
        // Under-funded (fee >= amount) is rejected.
        assert!(key.build_funded_overlay_tx(SUBNETWORK_ID_PALW_BEACON_COMMIT, payload, fop(7, 0), &funding, funding.amount).is_err());
    }

    /// PALW provider bonds are the overlay carrier that cannot use the legacy one-change-output
    /// shape: consensus pins the payload-derived value lock at output 0. The generalized multi-input
    /// builder must preserve that lock, put change at output 1, and produce a transaction accepted by
    /// the exact isolation validator used by the mempool/body pipeline.
    #[test]
    fn funded_palw_provider_bond_places_lock_before_change_and_passes_consensus_shape_validation() {
        use kaspa_consensus_core::palw::{
            PALW_PAYLOAD_VERSION_V1, PalwProviderBondPayloadV1, provider_bond_lock_spk, validate_palw_overlay_tx,
        };
        use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_PROVIDER_BOND;

        let key = ValidatorKey::from_seed([0x19; 32]);
        let amount = 7_000_000;
        let fee = 300_000;
        let bond = PalwProviderBondPayloadV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            owner_public_key: key.public_key().to_vec(),
            operator_group_id: Hash64::from_bytes([0x21; 64]),
            runtime_classes: vec![Hash64::from_bytes([0x22; 64])],
            capacity_by_shape: vec![(1, 1)],
            reward_key_root: Hash64::from_bytes([0x23; 64]),
            amount_sompi: amount,
            unbond_delay_epochs: 2,
        };
        let payload = borsh::to_vec(&bond).unwrap();
        let lock = TransactionOutput::new(amount, provider_bond_lock_spk(&bond.owner_public_key));
        let fundings = vec![(fop(0x31, 0), funding_at(&key, 4_000_000)), (fop(0x32, 0), funding_at(&key, 5_000_000))];

        let tx = key
            .build_funded_overlay_tx_with_outputs_multi(
                SUBNETWORK_ID_PALW_PROVIDER_BOND,
                payload.clone(),
                vec![lock.clone()],
                &fundings,
                fee,
            )
            .unwrap();
        assert_eq!(tx.inputs.len(), 2);
        assert!(tx.inputs.iter().all(|input| !input.signature_script.is_empty()));
        assert_eq!(tx.outputs[0], lock);
        assert_eq!(tx.outputs[1].value, 2_000_000 - fee);
        assert_eq!(tx.outputs[1].script_public_key, fundings[0].1.script_public_key);
        assert_eq!(validate_palw_overlay_tx(0x30, &payload, &tx.outputs), Ok(()));

        let underfunded = vec![(fop(0x33, 0), funding_at(&key, amount + fee))];
        assert!(
            key.build_funded_overlay_tx_with_outputs_multi(SUBNETWORK_ID_PALW_PROVIDER_BOND, payload, vec![lock], &underfunded, fee,)
                .is_err(),
            "the staged submitter needs a positive change outpoint to confirm before its next phase"
        );
    }

    /// ECON-03 release safety: the provider lock must use the address-domain hash recomputed by
    /// `OP_BLAKE2B_512`, not the unkeyed overlay credential id. Carrier-shape validation alone cannot
    /// catch this class of bug because construction and validation call the same lock helper. Execute a
    /// real owner-signed spend through the production PQ-only script engine, and pin the old identity-
    /// hash shape as unspendable by that same owner.
    #[test]
    fn palw_provider_bond_lock_is_spendable_by_its_owner() {
        use kaspa_consensus_core::dns_finality::{p2pkh_mldsa87_spk, validator_id_from_pubkey};
        use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
        use kaspa_consensus_core::palw::provider_bond_lock_spk;
        use kaspa_txscript::{ScriptPolicy, TxScriptEngine, caches::Cache};

        fn first_input_executes(tx: &Transaction, entry: UtxoEntry) -> bool {
            let populated = PopulatedTransaction::new(tx, vec![entry]);
            let reused = SigHashReusedValuesUnsync::new();
            let cache = Cache::new(10_000);
            TxScriptEngine::from_transaction_input(&populated, &populated.tx.inputs[0], 0, &populated.entries[0], &reused, &cache)
                .with_script_policy(ScriptPolicy::PQ_ONLY)
                .execute()
                .is_ok()
        }

        let key = ValidatorKey::from_seed([0x2a; 32]);
        let amount = 2_000_000;
        let fee = 300_000;
        let bond_outpoint = fop(0x71, 0);
        let spendable_entry = UtxoEntry::new(amount, provider_bond_lock_spk(key.public_key()), 10, false);
        let spend = key
            .build_funded_overlay_tx(SUBNETWORK_ID_NATIVE, Vec::new(), bond_outpoint, &spendable_entry, fee)
            .expect("owner can construct the post-release spend");
        assert!(first_input_executes(&spend, spendable_entry), "provider-bond output must unlock under its owner's ML-DSA-87 key");

        let identity_payload = validator_id_from_pubkey(key.public_key());
        let old_unspendable_entry = UtxoEntry::new(amount, p2pkh_mldsa87_spk(&identity_payload.as_bytes()), 10, false);
        let old_shape_spend = key
            .build_funded_overlay_tx(SUBNETWORK_ID_NATIVE, Vec::new(), bond_outpoint, &old_unspendable_entry, fee)
            .expect("signing succeeds even though the old lock's hash comparison cannot");
        assert!(
            !first_input_executes(&old_shape_spend, old_unspendable_entry),
            "the unkeyed overlay identity must never be reused as an ML-DSA-87 P2PKH spend payload"
        );
    }

    /// The refactor is behavior-preserving: build_funded_shard_tx now delegates to
    /// build_funded_overlay_tx(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, borsh(shard), …). Every
    /// deterministic field matches (subnetwork, payload, funding input, change output, lock_time/gas);
    /// only the ML-DSA-87 signature script differs — that primitive is RANDOMIZED (hedged), so two
    /// signs of the same message never byte-match, which is exactly why the whole txs are not compared.
    #[test]
    fn overlay_builder_matches_the_shard_builder_on_every_deterministic_field() {
        let key = ValidatorKey::from_seed([0x22; 32]);
        let funding = funding_at(&key, 5_000_000);
        let att = StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: key.validator_id,
            bond_outpoint: fop(6, 0),
            epoch: 3,
            target_hash: Hash64::from_bytes([1; 64]),
            target_daa_score: 7,
            validator_set_commitment: Hash64::from_bytes([2; 64]),
            signature: vec![0x77; MLDSA87_SIG_LEN],
        };
        let shard = single_attestation_shard(att);
        let a = key.build_funded_shard_tx(&shard, fop(9, 0), &funding, 250_000).unwrap();
        let b = key
            .build_funded_overlay_tx(
                SUBNETWORK_ID_STAKE_ATTESTATION_SHARD,
                borsh::to_vec(&shard).unwrap(),
                fop(9, 0),
                &funding,
                250_000,
            )
            .unwrap();
        assert_eq!(a.subnetwork_id, b.subnetwork_id);
        assert_eq!(a.payload, b.payload);
        assert_eq!(a.payload, borsh::to_vec(&shard).unwrap());
        assert_eq!(a.inputs.len(), 1);
        assert_eq!(a.inputs[0].previous_outpoint, b.inputs[0].previous_outpoint);
        assert_eq!(a.inputs[0].sequence, b.inputs[0].sequence);
        assert_eq!(a.outputs, b.outputs);
        assert_eq!((a.lock_time, a.gas), (b.lock_time, b.gas));
        // Both are genuinely signed (non-empty sig scripts); the bytes differ only by hedged randomness.
        assert!(!a.inputs[0].signature_script.is_empty() && !b.inputs[0].signature_script.is_empty());
    }

    /// The beacon-secret store keeps a committed secret across reload, prunes revealed epochs, and
    /// refuses a foreign key's file (so a restart between commit E-2 and reveal E-1 never loses it).
    #[test]
    fn beacon_secret_store_persists_reloads_prunes_and_rejects_foreign() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("beacon-secret.json");
        let vid = Hash64::from_bytes([0x0a; 64]);
        let bond = TransactionOutpoint::new(Hash64::from_bytes([0x06; 64]), 0);

        let mut store = BeaconSecretStore::load_or_empty(path.clone(), vid, bond).unwrap();
        assert!(store.is_empty());
        store.record_and_flush(12, [0x5A; 64]).unwrap();
        store.record_and_flush(13, [0x5B; 64]).unwrap();

        // Reload sees both secrets.
        let reloaded = BeaconSecretStore::load_or_empty(path.clone(), vid, bond).unwrap();
        assert_eq!(reloaded.secret_for(12), Some([0x5A; 64]));
        assert!(reloaded.has_secret(13));
        assert_eq!(reloaded.len(), 2);

        // Prune through epoch 12 drops the revealed secret, keeps the future one.
        let mut store = reloaded;
        store.prune_through(12).unwrap();
        assert!(!store.has_secret(12));
        assert_eq!(store.secret_for(13), Some([0x5B; 64]));
        assert_eq!(BeaconSecretStore::load_or_empty(path.clone(), vid, bond).unwrap().len(), 1);

        // A foreign validator/bond must refuse the file rather than clobber it.
        let foreign_bond = TransactionOutpoint::new(Hash64::from_bytes([0x07; 64]), 0);
        assert!(BeaconSecretStore::load_or_empty(path.clone(), vid, foreign_bond).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::symlink_metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode & 0o077, 0, "beacon-secret must be owner-only (mode {mode:o})");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            assert!(BeaconSecretStore::load_or_empty(path, vid, bond).err().unwrap().contains("group/world-accessible"));
        }
    }

    /// kaspa-pq ADR-0040 (C-1): the ticket-secret store survives reload, refuses a foreign authority's
    /// file, refuses to replace a registered leaf's nullifier, and is never world-readable.
    ///
    /// The immutability arm is the load-bearing one. A registered leaf's nullifier is fixed by the
    /// commitment already on chain; silently overwriting it would destroy the only value that opens
    /// that commitment, permanently killing a ticket whose registration was already paid for.
    #[test]
    fn ticket_secret_store_persists_reloads_and_refuses_overwrite_or_foreign_authority() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ticket-secrets.json");
        let authority = Hash64::from_bytes([0x11; 64]);
        let batch = Hash64::from_bytes([0x22; 64]);
        let nullifier = Hash64::from_bytes([0x33; 64]);

        let mut store = TicketSecretStore::load_or_empty(path.clone(), authority).unwrap();
        assert!(store.is_empty());
        store.record_and_flush(batch, 0, nullifier).unwrap();
        assert_eq!(store.secret_for(&batch, 0), Some(nullifier));
        // Leaf index is part of the key: a sibling leaf in the same batch is a different ticket.
        assert_eq!(store.secret_for(&batch, 1), None);

        // Survives a restart — the whole reason the file exists.
        let reloaded = TicketSecretStore::load_or_empty(path.clone(), authority).unwrap();
        assert_eq!(reloaded.secret_for(&batch, 0), Some(nullifier));
        assert_eq!(reloaded.len(), 1);

        // Re-recording the SAME value is fine (idempotent retry); a DIFFERENT value is refused.
        let mut store = TicketSecretStore::load_or_empty(path.clone(), authority).unwrap();
        store.record_and_flush(batch, 0, nullifier).unwrap();
        let err = store.record_and_flush(batch, 0, Hash64::from_bytes([0x44; 64])).unwrap_err();
        assert!(err.contains("immutable"), "{err}");
        assert_eq!(store.secret_for(&batch, 0), Some(nullifier), "the original secret must survive the refused write");

        // A foreign authority must refuse the file rather than draw with secrets it cannot authorize.
        assert!(TicketSecretStore::load_or_empty(path.clone(), Hash64::from_bytes([0x99; 64])).is_err());

        // Never world-readable, from the first write (not chmod'd afterwards).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode & 0o077, 0, "ticket-secret file must not be group/world-accessible (mode {mode:o})");
        }

        // Forgetting a spent leaf drops only that key.
        let mut store = TicketSecretStore::load_or_empty(path.clone(), authority).unwrap();
        store.record_and_flush(batch, 1, Hash64::from_bytes([0x55; 64])).unwrap();
        store.forget(&batch, 0).unwrap();
        assert_eq!(store.secret_for(&batch, 0), None);
        assert!(store.secret_for(&batch, 1).is_some());
    }
}
