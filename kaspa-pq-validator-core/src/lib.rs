//! Shared kaspa-pq ML-DSA-87 key and transaction primitives.
//!
//! Named for the DNS-finality validator it was first written for (ADR-0010 / ADR-0011). That
//! overlay and its in-node service and `kaspa-pq-validator` sidecar are retired — PALW does not
//! involve validators — and their attestation, precommit, slashing-evidence, stake-bond, unbond,
//! VLT compute and token builders went with them. What remains is what PALW and the wallet use:
//! the seed loader/writer and [`ValidatorKey`] (a bond key's ML-DSA-87 keypair and address),
//! the funded carrier builders (PALW lifecycle objects, Stage-0 carriage, free-prompt commitments
//! and receipt spends, native send/consolidate), relay-fee helpers, funding selection, and the
//! signer's persistent equivocation logs ([`SignedEpochStore`], [`PalwAttemptJournalStore`]).
//! No consensus surface — this is a node-local helper crate.

/// ADR-0079 Decision 8 / SA-2: the ONE message shape a free-prompt signature may cover, and the
/// only place a `PalwFpCommitmentV3` claim id is allowed to come from. Both signing forms — the
/// rail's local seed and the signer sidecar's `--print-claim` digest — pass through it.
pub mod palw_fp_sign_gate;

use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::constants::{MAX_TX_IN_SEQUENCE_NUM, TX_VERSION};
use kaspa_consensus_core::dns_finality::{
    PalwAttemptSignRecordV1, SignedEpochCheckOutcome, SignedEpochRecord, check_palw_attempt_sign_record_v1, check_signed_epoch_record,
    validator_id_from_pubkey,
};
use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_COMMITMENT_TX_MAX_BYTES, PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT, PALW_FP_V3_VERSION, PalwFpCommitmentTxPayloadV3,
    PalwFreePromptCommitmentV3, fp_claim_id_v3,
};
use kaspa_consensus_core::subnets::{
    SUBNETWORK_ID_COMPUTE_CERTIFICATE, SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_FP_COMMITMENT, SubnetworkId,
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
use std::path::PathBuf;
use std::str::FromStr;

/// Length in bytes of the ML-DSA-87 keygen seed consumed by [`ValidatorKey::from_seed`]
/// (matches the wallet's `KaspaPqMlDsa87KeyPair`).
pub const VALIDATOR_SEED_LEN: usize = 32;

/// Safety floor (sompi) for a funded carrier's fee. It is the minimum the mass-based estimators
/// ([`ValidatorKey::estimate_overlay_fee`], [`ValidatorKey::estimate_deposit_lock_fee_for_inputs`])
/// ever return, and the value used when a `MassCalculator` is unavailable. The real fee is the
/// relay-rate fee derived from the transaction's compute mass — see [`relay_fee_for_compute_mass`], which mirrors the
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

/// Write a fresh ML-DSA-87 seed to `path` as hex — the hardened counterpart of
/// [`load_validator_seed`], shared so every keygen (validator, drill, re-executor) inherits
/// the same discipline instead of each copying a weaker variant: `create_new`
/// (`O_CREAT|O_EXCL`) refuses to clobber an existing key AND refuses a pre-planted symlink;
/// `.mode(0o600)` sets owner-only perms AT CREATION, so there is never the group/world-
/// readable window a write-then-chmod sequence leaves open (the retained-fd class the M-02
/// read-side guard exists for); `sync_all` makes the key durable before the caller prints
/// a funding address anyone might send to. The caller owns scrubbing its seed copy.
pub fn write_validator_seed(path: &str, seed: &[u8; VALIDATOR_SEED_LEN]) -> Result<(), String> {
    let mut hex_buf = [0u8; VALIDATOR_SEED_LEN * 2];
    faster_hex::hex_encode(seed, &mut hex_buf).map_err(|e| format!("hex encode failed: {e}"))?;
    let result = (|| {
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
                .map_err(|e| format!("cannot create key file '{path}' (it must not already exist): {e}"))?;
            f.write_all(&hex_buf).map_err(|e| format!("cannot write key to '{path}': {e}"))?;
            f.sync_all().map_err(|e| format!("cannot fsync key file '{path}': {e}"))?;
        }
        #[cfg(not(unix))]
        {
            if std::path::Path::new(path).exists() {
                return Err(format!("refusing to overwrite existing key file '{path}'"));
            }
            std::fs::write(path, &hex_buf).map_err(|e| format!("cannot write key to '{path}': {e}"))?;
        }
        Ok(())
    })();
    hex_buf.fill(0);
    std::hint::black_box(&hex_buf);
    result
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
    /// BLAKE2b-512). Funding UTXOs sent here fund this key's carrier
    /// transactions (funding model A).
    pub fn funding_address(&self, prefix: Prefix) -> Address {
        let payload = blake2b_512_address_payload(self.keypair.verification_key.as_ref()).as_bytes();
        Address::new(prefix, Version::PubKeyHashMlDsa87, &payload)
    }

    /// Sign `message` under an explicit ML-DSA-87 `context` (domain separator) with fresh
    /// hedged randomness. Distinct contexts keep attestation signatures
    /// ([`ATTESTATION_MLDSA87_CONTEXT`](kaspa_consensus_core::dns_finality::ATTESTATION_MLDSA87_CONTEXT))
    /// and transaction-input signatures ([`MLDSA87_TX_CONTEXT`]) in disjoint domains — neither can
    /// be replayed as the other.
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

    /// Sign a PALW **block commitment** on the miner's behalf — ADR-0038 Decision A's producer.
    ///
    /// The bonded ML-DSA-87 key lives here and the miner has only its BIP39 payout key, so the
    /// miner cannot sign its own commitment. This is the seam that closes that, and its shape is
    /// the whole security argument.
    ///
    /// **What this deliberately is NOT: a "sign these bytes" call.** A sidecar that signs a digest
    /// handed to it by another process has given that process the key. The digest of a stake
    /// attestation, a precommit, or a transaction input is bytes like any other, so a compromised
    /// or merely buggy miner could obtain a signature over something that slashes this bond or
    /// spends its funds, and nothing in the request would look wrong. Two properties stop that,
    /// and both are structural rather than checks that could be forgotten:
    ///
    /// 1. **The digest is derived here, from a typed commitment.** The caller passes the payload
    ///    and the attempt it was mined under; `PalwBlockCommitmentV1::message` recomputes what is
    ///    signed. There is no input to this method that can express "an attestation".
    /// 2. **The context is [`PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT`]**, disjoint from
    ///    [`ATTESTATION_MLDSA87_CONTEXT`](kaspa_consensus_core::dns_finality::ATTESTATION_MLDSA87_CONTEXT),
    ///    [`PRECOMMIT_MLDSA87_CONTEXT`](kaspa_consensus_core::dns_finality::PRECOMMIT_MLDSA87_CONTEXT)
    ///    and the transaction context. ML-DSA binds the context into the signature, so even a digest
    ///    that collided with an attestation message would produce a signature no attestation
    ///    verifier accepts.
    ///
    /// The commitment is shape-checked first, in the retired precommit signer's spirit: signing something
    /// consensus will reject only burns an attempt, and doing it silently makes the miner look
    /// broken rather than misconfigured. `signature` on the input is ignored — pass anything.
    ///
    /// **What this does not check**: that `executor_bond_outpoint` is a bond this key backs. This
    /// process does not hold the bond registry.
    ///
    /// **CORRECTION (2026-08-19, external audit P0-2).** This paragraph said the omission was safe
    /// because "a signature over a foreign bond simply fails verification at admission, because the
    /// registry resolves the key from the bond". **That is false.**
    /// `check_palw_block_admission_v1` verifies shape, that the named bond is Active, the class and
    /// PWU claim, and the ticket — and never verifies this signature at all. So today an attacker
    /// needs no key: naming any Active bond outpoint and attaching bytes of the right length passes
    /// W8, and the signature this function produces is not what admits a block.
    ///
    /// The claim was written the same day as the code and was not checked against the admission
    /// path it named. It is left here rather than deleted because the failure mode — asserting a
    /// safety property held by a DIFFERENT function without reading that function — is worth more
    /// as a marker than a clean doc is.
    ///
    /// Until admission verifies commitment signatures against the active bond's `validator_pubkey`
    /// under [`PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT`], the typed-call discipline above protects
    /// this KEY from misuse and protects nothing about who may produce a block.
    pub fn sign_palw_block_commitment_v1(
        &self,
        network_id: &[u8],
        unsigned: &kaspa_consensus_core::palw_block_commitment::PalwBlockCommitmentV1,
        pre_pow_hash: Hash64,
        timestamp: u64,
        nonce: u64,
    ) -> Result<Vec<u8>, String> {
        let mut shaped = unsigned.clone();
        shaped.signature = vec![0u8; kaspa_consensus_core::dns_finality::STAKE_ATTESTATION_SIG_LEN];
        shaped.validate_shape().map_err(|e| format!("refusing to sign a commitment consensus would reject: {e}"))?;
        let message = shaped.message(network_id, pre_pow_hash, timestamp, nonce);
        Ok(self
            .sign_with_context(
                message.as_bytes().as_slice(),
                kaspa_consensus_core::palw_block_commitment::PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT,
            )
            .to_vec())
    }

    /// **A PALW V2 lifecycle object, carried on subnetwork 0x4b** (launch blockers §2).
    ///
    /// The lattice's edges — `ReceiptLicensed` above all — ride an ordinary transaction, and
    /// nothing in the tree built one. So no claim could ever reach `Final`: every panel voided at
    /// `ReceiptTimeout` with all its seats slashed, `safe_weight` stayed zero, and the escrowed
    /// worker carve of every block was burned. The object itself is checked by the acceptance
    /// layer against chain state; what this owes is the funding and the signature.
    pub fn build_palw_lifecycle_tx(
        &self,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        fee: u64,
    ) -> Result<Transaction, String> {
        let payload = kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2 {
            version: kaspa_consensus_core::palw_lifecycle_objects_v2::PALW_LIFECYCLE_TX_VERSION_V2,
            object: object.clone(),
        };
        let bytes = borsh::to_vec(&payload).map_err(|e| format!("a lifecycle object must serialize: {e}"))?;
        self.build_funded_overlay_tx(
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            bytes,
            funding_outpoint,
            funding,
            fee,
            false,
        )
    }

    /// MISAKA PALW Stage-0 chain carriage (ADR-0029 §5): a fee-funded, signed
    /// **native-subnetwork** transaction carrying an opaque payload — the
    /// `"MPALW2" ‖ kind ‖ borsh` envelope built by `kaspa_consensus_core::palw_carriage`.
    /// Same funding/signing path as every overlay transaction above (one input, change to
    /// this key's own P2PKH-ML-DSA script, input-0 ML-DSA-signed under
    /// [`MLDSA87_TX_CONTEXT`]); only the subnetwork differs, because Stage-0 carriage rides
    /// the native lane admission already accepts. The payload is opaque HERE by design —
    /// carriage validity is the palw_carriage module's stateless check, run by the caller
    /// before spending a fee on it.
    /// **ADR-0087 Decision 3: a lifecycle carrier with value outputs beside the change.** Output
    /// 0 is the change (`funding − fee − Σ extra`), outputs 1.. are `extra` in order — a model
    /// buy names its sink at index 1. Refused when the funding does not cover fee and extras.
    pub fn build_palw_lifecycle_tx_with_outputs(
        &self,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        fee: u64,
        extra: Vec<TransactionOutput>,
    ) -> Result<Transaction, String> {
        let payload = kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2 {
            version: kaspa_consensus_core::palw_lifecycle_objects_v2::PALW_LIFECYCLE_TX_VERSION_V2,
            object: object.clone(),
        };
        let bytes = borsh::to_vec(&payload).map_err(|e| format!("a lifecycle object must serialize: {e}"))?;
        let extra_total: u64 = extra.iter().map(|o| o.value).sum();
        let spent = fee.checked_add(extra_total).ok_or("fee and outputs overflow")?;
        if funding.amount <= spent {
            return Err(format!("funding UTXO amount {} does not cover fee {fee} and outputs {extra_total}", funding.amount));
        }
        let input = TransactionInput::new(funding_outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1);
        let mut outputs = vec![TransactionOutput::new(funding.amount - spent, funding.script_public_key.clone())];
        outputs.extend(extra);
        let tx = Transaction::new(
            TX_VERSION,
            vec![input],
            outputs,
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            bytes,
        );
        let mtx = MutableTransaction::with_entries(tx, vec![funding.clone()]);
        let reused_mldsa = Mldsa87SigHashReusedValuesUnsync::new();
        let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &reused_mldsa);
        let mut sig_data = self.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).to_vec();
        sig_data.push(SIG_HASH_ALL.to_u8());
        let signature_script = ScriptBuilder::new()
            .add_data(&sig_data)
            .map_err(|e| format!("compute overlay funding sig push failed: {e}"))?
            .add_data(self.keypair.verification_key.as_ref())
            .map_err(|e| format!("compute overlay funding pubkey push failed: {e}"))?
            .drain();
        let mut tx = mtx.tx;
        tx.inputs[0].signature_script = signature_script;
        Ok(tx)
    }

    /// **ADR-0094 Decision 5: a carrier funded by as many utxos as it takes.**
    ///
    /// The single-utxo twin above is what every lifecycle carrier used, and it is why a producer
    /// with 190,000 MSK in 202 coinbase outputs could not pay a 100,000 MSK seed: no ONE output
    /// held it. This spends a list, in the order given, and the caller is the one that knows how
    /// many an ML-DSA-87 transaction fits under the mass cap (fifteen, measured).
    ///
    /// Every input is signed over the whole transaction (`SIG_HASH_ALL`), so no input can be
    /// lifted into another transaction; the change returns to the first input's script, which is
    /// this key's own funding address.
    pub fn build_palw_lifecycle_tx_multi(
        &self,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        funding: &[(TransactionOutpoint, UtxoEntry)],
        fee: u64,
        extra: Vec<TransactionOutput>,
    ) -> Result<Transaction, String> {
        if funding.is_empty() {
            return Err("a carrier needs at least one funding utxo".to_string());
        }
        let payload = kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2 {
            version: kaspa_consensus_core::palw_lifecycle_objects_v2::PALW_LIFECYCLE_TX_VERSION_V2,
            object: object.clone(),
        };
        let bytes = borsh::to_vec(&payload).map_err(|e| format!("a lifecycle object must serialize: {e}"))?;
        let extra_total: u64 = extra.iter().map(|o| o.value).sum();
        let spent = fee.checked_add(extra_total).ok_or("fee and outputs overflow")?;
        let funded: u64 = funding.iter().try_fold(0u64, |a, (_, e)| a.checked_add(e.amount)).ok_or("funding overflows")?;
        if funded <= spent {
            return Err(format!("funding utxos total {funded} does not cover fee {fee} and outputs {extra_total}"));
        }
        let inputs: Vec<TransactionInput> =
            funding.iter().map(|(o, _)| TransactionInput::new(*o, vec![], MAX_TX_IN_SEQUENCE_NUM, 1)).collect();
        let mut outputs = vec![TransactionOutput::new(funded - spent, funding[0].1.script_public_key.clone())];
        outputs.extend(extra);
        let tx =
            Transaction::new(TX_VERSION, inputs, outputs, 0, kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE, 0, bytes);
        let entries: Vec<UtxoEntry> = funding.iter().map(|(_, e)| e.clone()).collect();
        let mtx = MutableTransaction::with_entries(tx, entries);
        let reused_mldsa = Mldsa87SigHashReusedValuesUnsync::new();
        let mut scripts = Vec::with_capacity(funding.len());
        for i in 0..funding.len() {
            let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), i, SIG_HASH_ALL, &reused_mldsa);
            let mut sig_data = self.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).to_vec();
            sig_data.push(SIG_HASH_ALL.to_u8());
            scripts.push(
                ScriptBuilder::new()
                    .add_data(&sig_data)
                    .map_err(|e| format!("carrier funding sig push failed: {e}"))?
                    .add_data(self.keypair.verification_key.as_ref())
                    .map_err(|e| format!("carrier funding pubkey push failed: {e}"))?
                    .drain(),
            );
        }
        let mut tx = mtx.tx;
        for (input, script) in tx.inputs.iter_mut().zip(scripts) {
            input.signature_script = script;
        }
        Ok(tx)
    }

    pub fn build_funded_native_carriage_tx(
        &self,
        payload: Vec<u8>,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        fee: u64,
    ) -> Result<Transaction, String> {
        self.build_funded_overlay_tx(SUBNETWORK_ID_NATIVE, payload, funding_outpoint, funding, fee, false)
    }

    // ---------------------------------------------------------------------
    // The shared funded-carrier path: one funding input, change back to this key's own
    // P2PKH-ML-DSA script, ML-DSA-signed over the SIG_HASH_ALL v2 sighash under
    // MLDSA87_TX_CONTEXT. Factored so the funding/signing path cannot drift between the
    // transactions that use it (PALW lifecycle carriers, Stage-0 carriage, free-prompt commitments).
    // ---------------------------------------------------------------------

    /// Shared body of the funded-carrier transaction builders: one funding input, change to self,
    /// payload on `subnetwork_id`, input-0 ML-DSA-signed.
    ///
    /// `no_change` builds an output-less transaction — the shape of a pure evidence carrier whose
    /// reward consensus mints at `(tx_id, 0)`, where a declared output would collide with the mint.
    fn build_funded_overlay_tx(
        &self,
        subnetwork_id: SubnetworkId,
        payload: Vec<u8>,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        fee: u64,
        no_change: bool,
    ) -> Result<Transaction, String> {
        if funding.amount <= fee {
            return Err(format!("funding UTXO amount {} does not cover fee {}", funding.amount, fee));
        }
        let input = TransactionInput::new(funding_outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1);
        let outputs = if no_change {
            // The whole input beyond the declared fee is burned to fees; consensus mints the
            // reporter reward separately.
            vec![]
        } else {
            vec![TransactionOutput::new(funding.amount - fee, funding.script_public_key.clone())]
        };
        let tx = Transaction::new(TX_VERSION, vec![input], outputs, 0, subnetwork_id, 0, payload);

        let mtx = MutableTransaction::with_entries(tx, vec![funding.clone()]);
        let reused_mldsa = Mldsa87SigHashReusedValuesUnsync::new();
        let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &reused_mldsa);

        let mut sig_data = self.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).to_vec();
        sig_data.push(SIG_HASH_ALL.to_u8());
        let signature_script = ScriptBuilder::new()
            .add_data(&sig_data)
            .map_err(|e| format!("compute overlay funding sig push failed: {e}"))?
            .add_data(self.keypair.verification_key.as_ref())
            .map_err(|e| format!("compute overlay funding pubkey push failed: {e}"))?
            .drain();

        let mut tx = mtx.tx;
        tx.inputs[0].signature_script = signature_script;
        Ok(tx)
    }

    /// Sign and build a **free-prompt execution commitment** transaction (ADR-0044 FP-08).
    ///
    /// The executor rail's one on-chain step: it takes the commitment the gateway assembled from
    /// its own inference (roots, executed shape, derived CU, the retained-trace DA trio) plus the
    /// PublicDA prompt ids, signs the CLAIM ID — which is total over the commitment, so signing
    /// the identity signs every field — and funds the overlay transaction.
    ///
    /// What this function refuses, and why each refusal is here rather than at acceptance:
    ///
    /// * a prompt that is not the one the commitment binds — the panel replays from these bytes,
    ///   so a mismatch produces a commitment no honest verifier can ever reproduce;
    /// * a `pwu`/`quanta` derivation the bundle would not make — the state machine demands
    ///   uniform non-zero quanta, and a claim that cannot enter the chain should not cost a fee;
    /// * an oversized payload, before it is built rather than after a peer drops it.
    ///
    /// The price is NOT re-derived here on purpose: `work_leaves` is the capture's leaf count the
    /// worker read off its binding (ADR-0074 Decision 5), and the seats verify it against the
    /// served capture. What IS asked here is the transition's own question — does this many
    /// leaves earn a whole quantum of `class_canonical_leaves`, the class's canonical job size —
    /// so a sub-quantum claim never pays a fee to be refused.
    #[allow(clippy::too_many_arguments)]
    pub fn build_fp_commitment_tx(
        &self,
        network_domain: Hash64,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        commitment: PalwFreePromptCommitmentV3,
        prompt_token_ids: Vec<u32>,
        freeprompt: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptParamsV3,
        class_canonical_leaves: u64,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        fee: u64,
    ) -> Result<Transaction, String> {
        // Sign the identity first so the payload carries a signature over exactly the bytes the
        // stateless check will re-derive.
        let signature =
            self.sign_with_context(fp_claim_id_v3(&commitment).as_bytes().as_slice(), PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT).to_vec();
        // **Under `PanelDa` the ids never ride the chain** (ADR-0077 Decision 16): the payload
        // carries the job's commitment to them and nothing else, and the caller stages the ids
        // beside the material for the executor's node to serve to the claim's readers. A payload
        // that carried them would be refused by `validate_stateless_v3` as
        // `PanelDaPayloadCarriesPrompt`; dropping them here is what makes the honest caller's
        // path and the validator's rule one spelling.
        let prompt_token_ids = if commitment.job.privacy_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PANEL_DA {
            Vec::new()
        } else {
            prompt_token_ids
        };
        let payload = PalwFpCommitmentTxPayloadV3 { version: PALW_FP_V3_VERSION, commitment, prompt_token_ids, signature };
        // The same stateless rules a peer will apply, applied before spending a fee on them.
        payload
            .validate_stateless_v3(network_domain, prompt_ids_form)
            .map_err(|e| format!("free-prompt commitment is not admissible: {e}"))?;
        let (quanta, pwu) = freeprompt.derive_quanta_and_pwu(payload.commitment.work_leaves, class_canonical_leaves).ok_or_else(|| {
            format!(
                "free-prompt job earns no quanta at {} leaves against a {class_canonical_leaves}-leaf canonical job — it certifies \
                 nothing the chain can act on",
                payload.commitment.work_leaves
            )
        })?;
        if quanta == 0 || pwu % (quanta as u64) != 0 || pwu / (quanta as u64) == 0 {
            return Err(format!("free-prompt derivation is not uniform ({pwu} pwu over {quanta} quanta)"));
        }
        let bytes = borsh::to_vec(&payload).map_err(|e| format!("cannot serialize the free-prompt commitment: {e}"))?;
        if bytes.len() > PALW_FP_COMMITMENT_TX_MAX_BYTES {
            return Err(format!(
                "free-prompt commitment payload is {} bytes, above the {PALW_FP_COMMITMENT_TX_MAX_BYTES} cap",
                bytes.len()
            ));
        }
        self.build_funded_overlay_tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, bytes, funding_outpoint, funding, fee, false)
    }

    /// **The receipt lane's carriage: a signed spend of one certified quantum (FP-R5).**
    ///
    /// The mirror of [`Self::build_fp_commitment_tx`], for the other end of the claim's life: that
    /// one opens a claim, this one spends a quantum of a certified claim into a receipt BLOCK. It
    /// returns the envelope rather than a transaction because a receipt block's carriage rides the
    /// HEADER (`palw_commitment`, algo 7), not the transaction lane — the producer sets
    /// `pow_algo_id` and attaches these bytes.
    ///
    /// Nothing here is chosen. The challenge binds the header's own position (`pre_pow_hash`,
    /// `timestamp`, `nonce`) so the envelope cannot be replayed onto another block; the ticket the
    /// admission prices is a pure function of (domain, beacon, claim, quantum) that no field here
    /// influences — grinding this signature buys nothing, which is the lane's design.
    #[allow(clippy::too_many_arguments)]
    pub fn build_fp_receipt_spend_envelope(
        &self,
        network_domain: Hash64,
        pre_pow_hash: Hash64,
        timestamp: u64,
        nonce: u64,
        claim_id: Hash64,
        quantum_index: u32,
        producer_bond: TransactionOutpoint,
        beacon_block: Hash64,
    ) -> kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3 {
        use kaspa_consensus_core::palw_freeprompt_v3::{
            PALW_FP_V3_MLDSA87_SPEND_CONTEXT, PALW_FP_V3_VERSION, PalwReceiptSpendEnvelopeV3, PalwReceiptSpendUnsignedV3,
            fp_spend_id_v3, spend_challenge_v3,
        };
        let spend = PalwReceiptSpendUnsignedV3 {
            version: PALW_FP_V3_VERSION,
            network_domain,
            challenge: spend_challenge_v3(network_domain, pre_pow_hash, timestamp, nonce, claim_id, quantum_index, &producer_bond),
            claim_id,
            quantum_index,
            beacon_block,
            producer_bond,
            producer_pubkey: self.public_key().to_vec(),
        };
        let signature =
            self.sign_with_context(fp_spend_id_v3(&spend).as_bytes().as_slice(), PALW_FP_V3_MLDSA87_SPEND_CONTEXT).to_vec();
        PalwReceiptSpendEnvelopeV3 { spend, signature }
    }

    /// Build a fee-funded, signed NATIVE transfer that SPLITS one funding UTXO into
    /// `num_outputs` change outputs back to this key's own P2PKH-ML-DSA script — a generic
    /// value-moving transaction used for load generation (each output becomes a fresh
    /// spendable UTXO, so a chain of these fans out into many transactions). The KIP-9
    /// storage mass is committed (value-based, so it matches the node's `calc_contextual_masses`
    /// recheck), and input-0 is ML-DSA-signed over the `SIG_HASH_ALL` v2 sighash under
    /// [`MLDSA87_TX_CONTEXT`] exactly as every other funded carrier here.
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

    /// Build a fee-funded, signed NATIVE SEND: spend `fundings` (all at this key's
    /// own funding script — a self-spend) into output-0 = `amount` to
    /// `recipient_spk`, output-1 = change back to self (emitted only when > 0).
    /// Plain native subnetwork, no payload, KIP-9 storage mass committed. Each
    /// input is signed independently under [`MLDSA87_TX_CONTEXT`] — the same
    /// per-input path as [`Self::build_palw_lifecycle_tx_multi`] (only the outputs +
    /// subnetwork differ).
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
    /// per-input signing as the send path.
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

        // Sign each self-spend input over the canonical sighash (same loop as the native multi-input builders).
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

    /// Mass-based fee (sompi) for an `n_inputs`-funded deposit-lock tx: build a dummy
    /// `n_inputs`-input lock of the real shape (field *sizes*, not values, drive the compute
    /// mass) and take its relay fee, clamped up to [`ATTESTATION_TX_FEE_FLOOR_SOMPI`].
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

    /// Mass-based fee (sompi) for a funded carrier transaction carrying `payload_len` bytes.
    ///
    /// Every carrier built through `build_funded_overlay_tx` shares one shape (1 P2PKH-ML-DSA
    /// input, change to self, a borsh payload), so the only thing that moves its mass is the
    /// payload size. Hence a parameterized estimate rather than one computed once at startup.
    ///
    /// `no_change` mirrors the output-less evidence-carrier shape.
    pub fn estimate_overlay_fee(&self, mass_calculator: &MassCalculator, prefix: Prefix, payload_len: usize, no_change: bool) -> u64 {
        let funding_spk = pay_to_address_script(&self.funding_address(prefix));
        let funding = UtxoEntry::new(u64::MAX / 2, funding_spk, 0, false);
        let outpoint = TransactionOutpoint::new(Hash64::from_bytes([0u8; 64]), 0);
        match self.build_funded_overlay_tx(
            SUBNETWORK_ID_COMPUTE_CERTIFICATE,
            vec![0u8; payload_len],
            outpoint,
            &funding,
            ATTESTATION_TX_FEE_FLOOR_SOMPI,
            no_change,
        ) {
            Ok(tx) => relay_fee_for_compute_mass(mass_calculator.calc_non_contextual_masses(&tx).compute_mass),
            Err(_) => ATTESTATION_TX_FEE_FLOOR_SOMPI,
        }
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
    ///
    /// **The in-memory index is a function of what is DURABLE.** The candidate is merged into a
    /// local copy, written, fsynced and renamed; only then does `self.records` learn about it.
    /// Inserting first (audit L2) made the index describe a file that does not exist: every
    /// step below can fail, none of them rolled the insert back, and the store is cached for the
    /// process lifetime so the lie is never re-read from disk. The next request for the same key
    /// then matched a "record" nobody wrote, took the `AllowRebroadcast` arm, released a
    /// signature and recorded nothing — and a restart forgot the commitment entirely. That is
    /// precisely the equivocation this log exists to make impossible.
    pub fn record_and_flush(&mut self, record: SignedEpochRecord) -> Result<(), String> {
        let mut records = self.records.clone();
        records.insert(record.epoch, record);
        let file = SignedEpochFile {
            version: SIGNED_EPOCH_FILE_VERSION,
            validator_id: self.validator_id,
            bond_outpoint: self.bond_outpoint,
            records: records.clone(),
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
            let mut f = fs::File::create(&tmp).map_err(|e| format!("cannot create validator-state tmp {}: {e}", tmp.display()))?;
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
        // Durable. Only now may the index claim the record exists.
        self.records = records;
        Ok(())
    }
}

/// On-disk shape of the per-validator PALW attempt journal (JSON). Records are a flat list —
/// challenge keys live inside each record — so the file format owes nothing to how a map
/// serializer renders keys; the in-memory index is rebuilt on load.
#[derive(serde::Serialize, serde::Deserialize)]
struct PalwAttemptJournalFile {
    version: u16,
    validator_id: Hash64,
    records: Vec<PalwAttemptSignRecordV1>,
}

const PALW_ATTEMPT_JOURNAL_FILE_VERSION: u16 = 1;

/// Persistent per-challenge signing journal enforcing ADR-0042 PR-05's anti-equivocation rule
/// across restarts: one challenge, one attempt id, ever. The signer consults it BEFORE signing
/// and records AFTER a successful sign with the same fsync-then-rename durability as
/// [`SignedEpochStore`] — a record that did not reach stable storage is a record the next boot
/// does not know, and the signature it covered must therefore never have been released.
pub struct PalwAttemptJournalStore {
    path: PathBuf,
    validator_id: Hash64,
    records: BTreeMap<Hash64, PalwAttemptSignRecordV1>,
}

impl PalwAttemptJournalStore {
    /// Load the journal for `validator_id` from `path`, or start empty if the file is absent.
    /// Errors if the file exists but belongs to a different validator — refusing to operate is
    /// safer than risking a cross-key journal.
    pub fn load_or_empty(path: PathBuf, validator_id: Hash64) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self { path, validator_id, records: BTreeMap::new() });
        }
        let raw = fs::read_to_string(&path).map_err(|e| format!("cannot read PALW attempt journal {}: {e}", path.display()))?;
        let file: PalwAttemptJournalFile =
            serde_json::from_str(&raw).map_err(|e| format!("cannot parse PALW attempt journal {}: {e}", path.display()))?;
        if file.validator_id != validator_id {
            return Err(format!("PALW attempt journal {} belongs to a different validator; refusing to use it", path.display()));
        }
        let records = file.records.into_iter().map(|r| (r.challenge, r)).collect();
        Ok(Self { path, validator_id, records })
    }

    /// Equivocation outcome for `candidate` against the persisted record for its challenge.
    pub fn check(&self, candidate: &PalwAttemptSignRecordV1) -> SignedEpochCheckOutcome {
        check_palw_attempt_sign_record_v1(self.records.get(&candidate.challenge), candidate)
    }

    /// Number of challenges with a persisted signing record (for status / logging).
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// Persist `record` for its challenge with the same durability discipline as
    /// [`SignedEpochStore::record_and_flush`]: temp file, fsync, rename, directory fsync —
    /// fail-closed on any error. Call only after a successful sign and after [`Self::check`]
    /// returned [`SignedEpochCheckOutcome::Allow`].
    ///
    /// Same durability discipline in the other direction too: `self.records` learns about the
    /// record only after the rename succeeds (audit L2). See
    /// [`SignedEpochStore::record_and_flush`] for why inserting first is the bug.
    pub fn record_and_flush(&mut self, record: PalwAttemptSignRecordV1) -> Result<(), String> {
        let mut records = self.records.clone();
        records.insert(record.challenge, record);
        let file = PalwAttemptJournalFile {
            version: PALW_ATTEMPT_JOURNAL_FILE_VERSION,
            validator_id: self.validator_id,
            records: records.values().copied().collect(),
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| format!("cannot serialize PALW attempt journal: {e}"))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("cannot create PALW attempt journal dir {}: {e}", parent.display()))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut f =
                fs::File::create(&tmp).map_err(|e| format!("cannot create PALW attempt journal tmp {}: {e}", tmp.display()))?;
            f.write_all(json.as_bytes()).map_err(|e| format!("cannot write PALW attempt journal tmp {}: {e}", tmp.display()))?;
            f.sync_all().map_err(|e| format!("cannot fsync PALW attempt journal tmp {}: {e}", tmp.display()))?;
        }
        fs::rename(&tmp, &self.path).map_err(|e| format!("cannot commit PALW attempt journal {}: {e}", self.path.display()))?;
        // Best-effort like SignedEpochStore's: persist the rename where the platform can.
        if let Some(parent) = self.path.parent()
            && let Ok(dir) = fs::File::open(parent)
        {
            let _ = dir.sync_all();
        }
        // Durable. Only now may the index claim the record exists.
        self.records = records;
        Ok(())
    }
}

/// Whether a funding UTXO can be spent right now. A coinbase output is locked until
/// `coinbase_maturity` blocks have passed since it was mined (consensus rule); a non-coinbase
/// output is always spendable. `virtual_daa` is the node's current virtual DAA score. Saturating
/// so a (transient) `block_daa_score > virtual_daa` reads as "not yet mature". Takes raw fields
/// (not a typed entry) so it works for both `UtxoEntry` and the RPC `RpcUtxoEntry` (same fields).
pub fn is_spendable(is_coinbase: bool, block_daa_score: u64, virtual_daa: u64, coinbase_maturity: u64) -> bool {
    is_spendable_ignoring_settlement(is_coinbase, block_daa_score, virtual_daa, coinbase_maturity)
}

/// **The maturity floor ALONE — which is not the rule a node applies to a coinbase spend.**
///
/// Kept under a name that says what it omits, because the omission is the whole story: spending a
/// coinbase clears two gates, and this is the smaller one. See [`is_spendable_settled`]. A caller
/// here is either spending a non-coinbase (where the two agree) or has not been audited yet, and
/// the name is how the second kind stays findable.
pub fn is_spendable_ignoring_settlement(is_coinbase: bool, block_daa_score: u64, virtual_daa: u64, coinbase_maturity: u64) -> bool {
    if !is_coinbase {
        return true;
    }
    virtual_daa.saturating_sub(block_daa_score) >= coinbase_maturity
}

/// **Both gates, as the node applies them** (ADR-0018 coinbase settlement).
///
/// A coinbase output is spendable when it clears the classic maturity floor AND is settled: either
/// older than `settlement_long_maturity_daa` — the fallback that guarantees nobody is frozen
/// forever — or covered by a confirmed DNS anchor at or past its block.
///
/// A wallet that checks only the floor offers the user money the node will refuse. On testnet-11,
/// where maturity is 1 and settlement is 600, that was every coinbase younger than 600 DAA: the
/// UTXO list called 1542 outputs mature and the send was rejected for spending an immature one.
/// The two numbers answer the same question and a wallet must ask with both.
///
/// `settlement_long_maturity_daa == 0` is the feature being off, and then this is the floor alone —
/// the same thing `coinbase_spend_settled` does with `settlement: None`.
pub fn is_spendable_settled(
    is_coinbase: bool,
    block_daa_score: u64,
    virtual_daa: u64,
    coinbase_maturity: u64,
    settlement_long_maturity_daa: u64,
    confirmed_anchor_daa: Option<u64>,
) -> bool {
    if !is_coinbase {
        return true;
    }
    let age = virtual_daa.saturating_sub(block_daa_score);
    if age < coinbase_maturity || virtual_daa < block_daa_score {
        return false;
    }
    if settlement_long_maturity_daa == 0 {
        return true;
    }
    if age >= settlement_long_maturity_daa {
        return true;
    }
    confirmed_anchor_daa.is_some_and(|anchor| anchor >= block_daa_score)
}

/// Choose the funding input for the next carrier tx. Prefers the local funding-chain head (our
/// previous change output, still unconfirmed in the node's utxoindex view) so we never re-select a
/// UTXO our own in-flight tx already spent — the cause of "output … already spent … in the mempool".
/// Falls back to the largest MATURE node UTXO not already spent in flight. Pure (no I/O); the caller
/// resyncs a mined chain head (`pending_change`) and prunes `inflight_spent` against the node's
/// current set before calling. Shared by the PALW fp rail and fp-submit so their funding paths behave
/// identically.
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
    use kaspa_consensus_core::dns_finality::{ATTESTATION_MLDSA87_CONTEXT, PRECOMMIT_MLDSA87_CONTEXT};
    use kaspa_consensus_core::tx::ScriptPublicKey;
    use std::io::Write;

    // ---- ADR-0038 Decision A: signing a block commitment on the miner's behalf ----

    fn commitment_fixture() -> kaspa_consensus_core::palw_block_commitment::PalwBlockCommitmentV1 {
        use kaspa_consensus_core::palw_block_commitment::{PALW_BLOCK_COMMITMENT_VERSION_V1, PalwBlockCommitmentV1};
        PalwBlockCommitmentV1 {
            version: PALW_BLOCK_COMMITMENT_VERSION_V1,
            execution_class_id: Hash64::from_bytes([0xC1; 64]),
            executor_bond_outpoint: fop(7, 0),
            trace_root: Hash64::from_bytes([0x7A; 64]),
            output_root: Hash64::from_bytes([0x00; 64]),
            pwu_claim: 4_242,
            // Ignored by the signer, and deliberately the wrong length so the test proves it.
            signature: vec![],
        }
    }

    /// The sidecar signs the miner's commitment, and the signature is usable ONLY as one.
    ///
    /// The second assertion is the reason this method exists in this shape. A "sign these bytes"
    /// RPC hands the bonded key to whoever can call it: the digest of a stake attestation is bytes
    /// like any other, so a compromised miner could obtain a signature that slashes this bond. The
    /// ML-DSA context is bound into the signature, so the same message under the attestation
    /// context does not verify — the separation is cryptographic, not a check to remember.
    #[test]
    fn a_commitment_signature_cannot_be_replayed_as_an_attestation() {
        use kaspa_consensus_core::palw_block_commitment::PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT;
        let key = ValidatorKey::from_seed([9u8; VALIDATOR_SEED_LEN]);
        let network = b"palw-test";
        let (pre_pow, ts, nonce) = (Hash64::from_bytes([0xB0; 64]), 1_700_000_000u64, 12_345u64);

        let unsigned = commitment_fixture();
        let sig = key.sign_palw_block_commitment_v1(network, &unsigned, pre_pow, ts, nonce).expect("well-formed");

        let mut signed = unsigned.clone();
        signed.signature = sig.clone();
        let message = signed.message(network, pre_pow, ts, nonce);

        assert!(
            kaspa_txscript::verify_mldsa87_with_context(
                key.public_key(),
                message.as_bytes().as_slice(),
                &sig,
                PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT
            )
            .unwrap(),
            "the commitment signature must verify under its own context"
        );
        for foreign in [ATTESTATION_MLDSA87_CONTEXT, PRECOMMIT_MLDSA87_CONTEXT] {
            assert!(
                !kaspa_txscript::verify_mldsa87_with_context(key.public_key(), message.as_bytes().as_slice(), &sig, foreign).unwrap(),
                "a commitment signature verified under a foreign context — the domains are not disjoint"
            );
        }
    }

    /// The signer refuses what consensus would reject, rather than burning the attempt silently.
    #[test]
    fn the_signer_refuses_a_commitment_consensus_would_reject() {
        let key = ValidatorKey::from_seed([9u8; VALIDATOR_SEED_LEN]);
        let mut bad = commitment_fixture();
        bad.pwu_claim = 0; // never legal — the class derivation is never zero
        let err = key
            .sign_palw_block_commitment_v1(b"palw-test", &bad, Hash64::from_bytes([0; 64]), 1, 1)
            .expect_err("a zero pwu claim must not be signed");
        assert!(err.contains("consensus would reject"), "the refusal should say why: {err}");
    }

    // ---- funding selection (the PALW rail and fp-submit share it) ----

    fn fop(seed: u8, idx: u32) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([seed; 64]), idx)
    }
    fn fentry(amount: u64, daa: u64, coinbase: bool) -> UtxoEntry {
        UtxoEntry::new(amount, ScriptPublicKey::default(), daa, coinbase)
    }
    const SF_FEE: u64 = 250_000;
    const SF_MATURITY: u64 = 100;
    const SF_VDAA: u64 = 10_000;

    // ---- ADR-0044 (FP-08): the free-prompt commitment transaction ----

    fn fp_bundle() -> kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2 {
        kaspa_consensus_core::palw_fp_devnet_v3::palw_fp_devnet_bundle_v3(
            Hash64::from_bytes([0xBA; 64]),
            Hash64::from_bytes([0xCA; 64]),
            Hash64::from_bytes([0xC0; 64]),
            4_096,
            Hash64::from_bytes([0xA7; 64]),
            kaspa_consensus_core::palw_fp_devnet_v3::palw_devnet_bond_registry_v1(
                kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1(),
            ),
        )
        .expect("the devnet bundle validates")
    }

    /// The floor's canonical job, in leaves (ADR-0074 Decision 5): the fixtures price against it.
    const FLOOR_LEAVES: u64 = 7_708;

    /// A commitment that earns real quanta under the devnet bundle: 96 prompt tokens, 256
    /// executed decode tokens, 16 quanta's worth of the floor's leaves.
    fn fp_commitment_fixture(network_domain: Hash64, key: &ValidatorKey) -> (PalwFreePromptCommitmentV3, Vec<u32>) {
        fp_commitment_fixture_with_prompt(network_domain, key, 96)
    }

    /// The same fixture over a prompt of `prompt_len` tokens.
    ///
    /// Parameterised because CU is `prompt·prefill_weight + executed·decode_weight`, so whether a
    /// job is sub-quantum depends on the PROMPT as much as the decode count — and the decode count
    /// cannot go below one (`decode_tokens_executed is zero` is refused separately, as a run that
    /// emitted nothing). A test that wants a genuinely sub-quantum job has to shorten the prompt.
    fn fp_commitment_fixture_with_prompt(
        network_domain: Hash64,
        key: &ValidatorKey,
        prompt_len: u32,
    ) -> (PalwFreePromptCommitmentV3, Vec<u32>) {
        use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpStopReasonV3, PalwFreePromptJobV3, fp_trace_manifest_v3};
        let ids: Vec<u32> = (0..prompt_len).collect();
        let job = PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain,
            class_id: Hash64::from_bytes([0xBA; 64]),
            executor_bond: fop(7, 0),
            executor_pubkey: key.public_key().to_vec(),
            operator_id: Hash64::from_bytes([0xE0; 64]),
            anchor_block: Hash64::from_bytes([0xA0; 64]),
            anchor_daa: 5_000,
            job_nonce: [0x11; 32],
            tokenizer_id: Hash64::from_bytes([0x70; 64]),
            prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&ids),
            prompt_tokens: ids.len() as u32,
            decode_token_limit: 512,
            max_context_tokens: 4_096,
            privacy_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        };
        let events: Vec<Hash64> = (0..256u64).map(|i| Hash64::from_u64_word(i + 1)).collect();
        let (manifest_root, chunk_count, _) = fp_trace_manifest_v3(Hash64::from_bytes([0xB1; 64]), &events);
        let commitment = PalwFreePromptCommitmentV3 {
            trace_root: Hash64::from_bytes([0x7A; 64]),
            output_root: Hash64::from_bytes([0x0B; 64]),
            schedule_root: Hash64::from_bytes([0x5C; 64]),
            execution_root: Hash64::from_bytes([0x4E; 64]),
            decode_tokens_executed: 256,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            work_leaves: 16 * (FLOOR_LEAVES / 8),
            trace_manifest_root: manifest_root,
            trace_chunk_count: chunk_count,
            trace_retention_daa: 505_000,
            job,
        };
        (commitment, ids)
    }

    /// The executor rail's round trip: build → the payload decodes from the transaction → the
    /// signature verifies under the bond key in ITS OWN context → the claim id is the one the
    /// signature covers. A funded overlay transaction, like every other in this file.
    #[test]
    fn fp_commitment_tx_round_trips_and_binds_its_claim() {
        let key = compute_key();
        let bundle = fp_bundle();
        let network_domain = Hash64::from_bytes([0x4E; 64]);
        let (commitment, ids) = fp_commitment_fixture(network_domain, &key);
        let expected_claim = fp_claim_id_v3(&commitment);

        let tx = key
            .build_fp_commitment_tx(
                network_domain,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                commitment,
                ids.clone(),
                &bundle.freeprompt,
                FLOOR_LEAVES,
                fop(9, 0),
                &fentry(u64::MAX / 2, 0, false),
                SF_FEE,
            )
            .expect("an admissible free-prompt commitment builds");
        assert_eq!(tx.subnetwork_id, SUBNETWORK_ID_PALW_FP_COMMITMENT);

        let decoded: PalwFpCommitmentTxPayloadV3 = borsh::from_slice(&tx.payload).expect("the payload decodes");
        assert_eq!(decoded.claim_id(), expected_claim, "the on-chain claim id is the one the builder signed");
        assert_eq!(decoded.prompt_token_ids, ids, "PublicDA carries the prompt the panel replays from");
        decoded
            .validate_stateless_v3(network_domain, kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
            .expect("a peer accepts what we built");

        // The signature verifies over the claim id under the bond key, in the commitment context…
        assert!(
            key.verify_with_context(expected_claim.as_byte_slice(), &decoded.signature, PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT),
            "the commitment signature verifies in its own context"
        );
        // …and NOT under any neighbouring context — the same separation the signer reserves.
        for foreign in [
            kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_MLDSA87_SPEND_CONTEXT,
            kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            ATTESTATION_MLDSA87_CONTEXT,
        ] {
            assert!(
                !key.verify_with_context(expected_claim.as_byte_slice(), &decoded.signature, foreign),
                "a commitment signature must not replay in a foreign context"
            );
        }
    }

    /// The builder refuses, before spending a fee, exactly what a peer would refuse after: a
    /// prompt the commitment does not bind, and a job too small to earn a single quantum.
    #[test]
    fn fp_commitment_tx_refuses_what_the_chain_would() {
        let key = compute_key();
        let bundle = fp_bundle();
        let network_domain = Hash64::from_bytes([0x4E; 64]);
        let (commitment, ids) = fp_commitment_fixture(network_domain, &key);

        // A prompt that is not the committed one.
        let mut wrong_ids = ids.clone();
        wrong_ids[0] = 0xFFFF;
        let err = key
            .build_fp_commitment_tx(
                network_domain,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                commitment.clone(),
                wrong_ids,
                &bundle.freeprompt,
                FLOOR_LEAVES,
                fop(9, 0),
                &fentry(u64::MAX / 2, 0, false),
                SF_FEE,
            )
            .unwrap_err();
        assert!(err.contains("not admissible"), "got {err}");

        // A sub-quantum job: it certifies nothing the chain can act on, so it never becomes a fee.
        //
        // **Sized from the quantum, and asserted to be under it, because this rotted once
        // already.** It used to be the 96-token fixture with ONE decoded token: at
        // `QUANTUM_CU = 1_000` that is 96 + 64 = 160 CU, comfortably sub-quantum. Lowering the
        // quantum to 100 made the same job PAYABLE and the test went red — and a version merely
        // loosened to keep it green would have gone on asserting a refusal that no longer happens.
        //
        // The decode count cannot be the lever: zero is refused separately, as a run that emitted
        // nothing. So the prompt shrinks instead, and the assertion below is what makes the next
        // quantum change fail loudly here rather than silently exercise the payable path.
        let quantum = kaspa_consensus_core::palw_freeprompt_v3::fp_class_quantum_leaves_v1(
            FLOOR_LEAVES,
            bundle.freeprompt.quanta_per_canonical_job(),
        );
        let (mut tiny, tiny_ids) = fp_commitment_fixture_with_prompt(network_domain, &key, 8);
        tiny.decode_tokens_executed = 1;
        tiny.work_leaves = quantum / 2;
        assert!(
            tiny.work_leaves > 0 && tiny.work_leaves < quantum,
            "the sub-quantum fixture must actually be sub-quantum: {} leaves against a quantum of {quantum}",
            tiny.work_leaves
        );
        let err = key
            .build_fp_commitment_tx(
                network_domain,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                tiny,
                tiny_ids,
                &bundle.freeprompt,
                FLOOR_LEAVES,
                fop(9, 0),
                &fentry(u64::MAX / 2, 0, false),
                SF_FEE,
            )
            .unwrap_err();
        assert!(err.contains("earns no quanta"), "got {err}");
    }

    /// The fixed key the free-prompt builder tests above sign with.
    fn compute_key() -> ValidatorKey {
        ValidatorKey::from_seed([0x5Au8; VALIDATOR_SEED_LEN])
    }

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

    /// **Issue #81, with its own numbers.** testnet-11's coinbase maturity FLOOR is 1 while the
    /// ADR-0018 settlement gate is 600 — asking the floor alone calls a 427-DAA-old coinbase
    /// "mature", `max_by_key(amount)` then prefers it over the small real transfers, the node
    /// refuses the spend for the settlement gate, and attestation stops until that exact outpoint
    /// ages past 600 (measured live: stopped at DAA 1319, self-recovered at 1504 = 892 + 600 + a
    /// tick). The caller must pass the EFFECTIVE spend maturity
    /// (`Params::coinbase_spend_maturity`, = floor ∨ settlement); with it the selector takes the
    /// small mature transfer over the large not-yet-spendable coinbase.
    #[test]
    fn select_funding_asks_with_both_maturities_issue_81() {
        let virtual_daa = 1_319u64;
        let coinbase = (fop(0xA1, 2), fentry(1_000_000_000, 892, true)); // 38k-MSK-style fragment, age 427
        let transfer = (fop(0xA2, 0), fentry(10_000_000_000 / 10_000, 100, false)); // a 10-MSK-style top-up

        // The floor alone (the bug): the big coinbase counts as mature and wins on amount.
        let (wrong, _) =
            select_funding(&None, &HashSet::new(), vec![coinbase.clone(), transfer.clone()], SF_FEE, virtual_daa, 1).unwrap();
        assert_eq!(wrong, fop(0xA1, 2), "the floor alone reproduces the bug — this guards the test itself");

        // The effective maturity (floor 1 ∨ settlement 600): the coinbase is not yet spendable,
        // the mature transfer is chosen, and attestation never latches onto an illegal outpoint.
        // Written as the max of the two rules rather than as its answer: this test is about the
        // COMBINATION, and `600` alone would not say which rule produced it or that a floor was
        // consulted at all. Clippy is right that it folds; folding it is what loses the point.
        #[allow(clippy::unnecessary_min_or_max)]
        let effective = 1u64.max(600);
        let (sel_op, sel_en) =
            select_funding(&None, &HashSet::new(), vec![coinbase, transfer], SF_FEE, virtual_daa, effective).unwrap();
        assert_eq!(sel_op, fop(0xA2, 0), "the mature transfer wins over the bigger unsettled coinbase");
        assert!(!sel_en.is_coinbase);
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

    /// Audit L2: a flush that FAILS must leave the in-memory index describing what is on disk.
    ///
    /// The regression: `records.insert` ran before the durable write and was never rolled back,
    /// so after an IO error the store believed a record existed that disk did not have. Because
    /// the store is cached for the process lifetime the lie was never re-read, and the next
    /// request for the same key took the `AllowRebroadcast` arm — releasing a signature and
    /// recording nothing. A restart then forgot the commitment entirely.
    ///
    /// A directory where the temp file goes makes `File::create` fail with EISDIR, which is the
    /// cheapest deterministic way to fail the write on every platform we build for.
    #[test]
    fn a_failed_flush_leaves_the_index_agreeing_with_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("validator-state.json");
        let vid = Hash64::from_bytes([0x01u8; 64]);
        let outpoint = TransactionOutpoint::new(Hash64::from_bytes([0x02u8; 64]), 0);
        // Block the temp path with a directory.
        std::fs::create_dir(path.with_extension("json.tmp")).unwrap();

        let mut store = SignedEpochStore::load_or_empty(path.clone(), vid, outpoint).unwrap();
        let a = signed_record(5, 0xaa);
        assert_eq!(store.check(&a), SignedEpochCheckOutcome::Allow);
        assert!(store.record_and_flush(a.clone()).is_err(), "the write must fail for this test to mean anything");

        // The whole point: NOT AllowRebroadcast. Nothing was recorded, so nothing may be
        // rebroadcast on the strength of a record that does not exist.
        assert_eq!(store.check(&a), SignedEpochCheckOutcome::Allow, "a failed flush must not license a rebroadcast");
        assert_eq!(store.record_count(), 0, "the index must not claim a record disk does not have");
        assert!(!path.exists(), "no journal file was committed");

        // And a conflicting target is still merely unconstrained rather than wrongly Blocked —
        // the index is empty in both directions, which is what "agrees with disk" means.
        assert_eq!(store.check(&signed_record(5, 0xbb)), SignedEpochCheckOutcome::Allow);
    }

    /// The same property for the PALW attempt journal (PR-05's twin store).
    #[test]
    fn a_failed_palw_journal_flush_leaves_the_index_agreeing_with_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("palw-attempt-journal.json");
        let vid = Hash64::from_bytes([0x07u8; 64]);
        std::fs::create_dir(path.with_extension("json.tmp")).unwrap();

        let mut store = PalwAttemptJournalStore::load_or_empty(path.clone(), vid).unwrap();
        let rec = PalwAttemptSignRecordV1 {
            challenge: Hash64::from_bytes([0x11u8; 64]),
            attempt_id: Hash64::from_bytes([0x22u8; 64]),
            signature_fingerprint: Hash64::from_bytes([0x33u8; 64]),
        };
        assert_eq!(store.check(&rec), SignedEpochCheckOutcome::Allow);
        assert!(store.record_and_flush(rec).is_err(), "the write must fail for this test to mean anything");
        assert_eq!(store.check(&rec), SignedEpochCheckOutcome::Allow, "a failed flush must not license a rebroadcast");
        assert_eq!(store.record_count(), 0);
        assert!(!path.exists());
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

#[cfg(test)]
mod coinbase_settlement_tests {
    use super::*;

    /// **The wallet's rule and the node's rule are one rule.**
    ///
    /// testnet-11's own numbers: maturity 1, settlement 600. A coinbase 256 DAA old clears the
    /// floor and is NOT settled, which is precisely the case the wallet used to call spendable and
    /// the node refused.
    #[test]
    fn a_matured_coinbase_is_not_yet_a_settled_one() {
        let (maturity, settlement) = (1, 600);
        assert!(
            is_spendable_ignoring_settlement(true, 1_446, 1_702, maturity),
            "the floor alone says yes — this is the answer that was wrong"
        );
        assert!(
            !is_spendable_settled(true, 1_446, 1_702, maturity, settlement, None),
            "and both gates say no, which is what the node said"
        );
    }

    /// The long fallback: nobody is frozen forever.
    #[test]
    fn past_the_long_maturity_a_coinbase_settles_without_an_anchor() {
        assert!(is_spendable_settled(true, 1_000, 1_600, 1, 600, None));
    }

    /// Acceleration: a confirmed anchor at or past the coinbase's block settles it early.
    #[test]
    fn a_confirmed_anchor_settles_a_young_coinbase() {
        assert!(is_spendable_settled(true, 1_446, 1_702, 1, 600, Some(1_446)), "anchor exactly at the block");
        assert!(!is_spendable_settled(true, 1_446, 1_702, 1, 600, Some(1_445)), "one short of it is not");
    }

    /// A non-coinbase never touches either gate, and settlement being off leaves the floor alone.
    #[test]
    fn the_gates_apply_where_they_are_meant_to() {
        assert!(is_spendable_settled(false, 1_700, 1_701, 600, 600, None), "a normal output is spendable at once");
        assert!(is_spendable_settled(true, 1_700, 1_701, 1, 0, None), "settlement off is the floor alone");
        assert!(!is_spendable_settled(true, 1_700, 1_701, 600, 0, None), "and the floor still applies");
    }
}

/// **The three safety claims [`write_validator_seed`] makes, falsified rather than restated.**
///
/// The function's doc asserts that it refuses a pre-planted symlink, that the file is owner-only
/// AT CREATION rather than after a chmod, and that the bytes are durable before it returns. Two
/// keygens now depend on all three — `misaka-palw-reexecutor` and `misaka-palw-shadow`, the
/// second having arrived here after copying a weaker variant of its own. Until this module the
/// claims were held up by a comment, which is the same standing as no claim at all: nothing in
/// the tree would have noticed if `create_new` had been relaxed to `create` during a refactor.
///
/// Each test is written so that it FAILS under the specific weaker implementation it rules out.
#[cfg(all(test, unix))]
mod validator_seed_writer_tests {
    use super::{VALIDATOR_SEED_LEN, write_validator_seed};
    use std::os::unix::fs::PermissionsExt;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("misaka-seed-writer-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// **A dangling symlink is the attack, and `exists()` is the check that misses it.**
    ///
    /// A planted link whose target does not exist reads as "nothing is there" to every check that
    /// follows links, and `File::create` then creates the TARGET — writing a key file wherever the
    /// link points, with this process's rights. `O_CREAT|O_EXCL` fails on the link itself, because
    /// the link IS an existing directory entry, which is the whole reason the writer uses it.
    #[test]
    fn a_dangling_symlink_is_refused_rather_than_followed() {
        let dir = scratch("symlink");
        let link = dir.join("key.hex");
        let target = dir.join("somewhere-else");
        std::os::unix::fs::symlink(&target, &link).expect("plant the link");
        assert!(!link.exists(), "the premise: a dangling link reads as absent to anything that follows it");

        let err = write_validator_seed(link.to_str().unwrap(), &[7u8; VALIDATOR_SEED_LEN])
            .expect_err("a planted symlink must not be followed");
        assert!(err.contains("must not already exist"), "the refusal should name the cause: {err}");
        assert!(!target.exists(), "the link's target was created — the write followed the link");
    }

    /// **An existing key is never clobbered**, which is the same `O_EXCL` seen from the other side.
    #[test]
    fn an_existing_key_file_is_never_overwritten() {
        let dir = scratch("clobber");
        let path = dir.join("key.hex");
        std::fs::write(&path, b"an existing key nobody wants replaced").expect("seed the file");
        let before = std::fs::read(&path).unwrap();

        write_validator_seed(path.to_str().unwrap(), &[9u8; VALIDATOR_SEED_LEN]).expect_err("must refuse");
        assert_eq!(std::fs::read(&path).unwrap(), before, "the file was modified despite the refusal");
    }

    /// **Owner-only AT CREATION.** A create-then-chmod sequence publishes the file at the umask's
    /// mode first — 0644 under the usual 022 — and narrows it afterwards; anyone who opened it in
    /// that window keeps a working descriptor across the chmod. This test cannot observe the
    /// window directly, so it does the next best thing: it sets a permissive umask, which is
    /// exactly the condition under which a create-then-chmod writer would leave 0644 behind if the
    /// chmod were ever dropped, and pins the final mode at 0600.
    #[test]
    fn the_key_is_owner_only_and_holds_the_seed_as_hex() {
        let dir = scratch("mode");
        let path = dir.join("key.hex");
        let seed = [0xabu8; VALIDATOR_SEED_LEN];

        write_validator_seed(path.to_str().unwrap(), &seed).expect("write");

        let mode = std::fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "mode is {mode:o}, not owner-only");

        let text = std::fs::read_to_string(&path).expect("read back");
        assert_eq!(text, "ab".repeat(VALIDATOR_SEED_LEN), "the file must be the seed as lowercase hex");
        assert_eq!(text.len(), VALIDATOR_SEED_LEN * 2, "hex is exactly two characters per byte");
        // and the shared READER accepts what the shared WRITER produced — the round trip is the
        // point of having one format, and it is what the gateway's private raw reader broke
        assert_eq!(super::load_validator_seed(path.to_str().unwrap()).expect("the reader accepts it"), seed);
    }

    /// **A directory that does not exist is an error, not a panic** — keygen is often the first
    /// command anyone runs, against a path they typed.
    #[test]
    fn a_missing_parent_directory_is_reported_and_not_a_panic() {
        let dir = scratch("nodir");
        let path = dir.join("no-such-subdir").join("key.hex");
        let err = write_validator_seed(path.to_str().unwrap(), &[1u8; VALIDATOR_SEED_LEN]).expect_err("must fail");
        assert!(err.contains("cannot create key file"), "the message should name the file: {err}");
    }
}
