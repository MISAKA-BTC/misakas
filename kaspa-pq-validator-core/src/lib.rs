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
    SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, SUBNETWORK_ID_STAKE_BOND, SUBNETWORK_ID_STAKE_UNBOND,
};
use kaspa_consensus_core::tx::{
    MutableTransaction, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_hashes::{Hash64, blake2b_512_address_payload};
use kaspa_txscript::{
    MLDSA87_SIG_LEN, MLDSA87_TX_CONTEXT, pay_to_address_script, script_builder::ScriptBuilder,
    script_class::evm_deposit_lock_script, verify_mldsa87_with_context,
};
use libcrux_ml_dsa::ml_dsa_87;
use rand::RngCore;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
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
        let reused_mldsa = Mldsa87SigHashReusedValuesUnsync::new();
        let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &reused_mldsa);

        let mut sig_data = self.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).to_vec();
        sig_data.push(SIG_HASH_ALL.to_u8()); // OpCheckSigMlDsa87 pops the trailing sighash-type byte
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
        match self.build_funded_unbond_tx(&[0u8; 32], TransactionOutpoint::new(Hash64::from_bytes([0u8; 64]), 0), outpoint, &funding, ATTESTATION_TX_FEE_FLOOR_SOMPI) {
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
        // Durability (audit H-3): the equivocation log MUST survive a crash. atomic rename alone
        // is not enough — if a written-AND-broadcast record is lost to a crash before it hits
        // stable storage, the validator could re-sign a DIFFERENT anchor for the same epoch on
        // restart (slashable). So fsync the temp file BEFORE the rename, then fsync the parent
        // directory so the new dirent is durable too. Fail-closed on any error.
        {
            let mut f =
                fs::File::create(&tmp).map_err(|e| format!("cannot create validator-state tmp {}: {e}", tmp.display()))?;
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
    if let Some((head, entry)) = pending_change {
        if entry.amount > fee {
            return Ok((*head, entry.clone()));
        }
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
        assert!(bond_fee > ATTESTATION_TX_FEE_FLOOR_SOMPI, "mass-based bond fee {bond_fee} must exceed the flat floor {ATTESTATION_TX_FEE_FLOOR_SOMPI}");
        assert!(unbond_fee > ATTESTATION_TX_FEE_FLOOR_SOMPI, "mass-based unbond fee {unbond_fee} must exceed the flat floor {ATTESTATION_TX_FEE_FLOOR_SOMPI}");
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
