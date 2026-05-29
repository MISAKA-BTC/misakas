//! kaspa-pq Phase 11 (ADR-0010): in-process validator node service.
//!
//! This is the validator wiring (PR-11.5 skeleton + PR-11.6 key/identity). It
//! parses validator CLI configuration, loads the ML-DSA-65 signing key and derives
//! the validator overlay identity (`validator_id = BLAKE2b-512(public_key)`), and
//! runs an async heartbeat loop that logs validator status. It deliberately does
//! **not** yet evaluate per-epoch eligibility, sign stake attestations, or submit
//! attestation shard transactions — those are later Phase 11 slices. The service is
//! registered only when `--enable-validator` is set, so default node behavior is
//! unchanged.

use kaspa_consensus_core::constants::{MAX_TX_IN_SEQUENCE_NUM, TX_VERSION};
use kaspa_consensus_core::dns_finality::{
    ATTESTATION_MLDSA65_CONTEXT, DNS_PAYLOAD_VERSION_V1, SignedEpochCheckOutcome, SignedEpochRecord, StakeAttestation,
    StakeAttestationShardPayload, ValidatorAttestationTarget, check_signed_epoch_record, effective_bond_status, is_bond_active_at,
    signature_fingerprint, single_attestation_shard, stake_attestation_shard_tx, validator_id_from_pubkey,
};
use kaspa_consensus_core::hashing::sighash::{SigHashReusedValuesUnsync, calc_schnorr_signature_hash};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_STAKE_ATTESTATION_SHARD;
use kaspa_consensus_core::tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    info,
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    trace, warn,
};
use kaspa_hashes::Hash64;
use kaspa_txscript::{MLDSA65_SIG_LEN, MLDSA65_TX_CONTEXT, script_builder::ScriptBuilder, verify_mldsa65_with_context};
use libcrux_ml_dsa::ml_dsa_65;
use rand::RngCore;
use std::{
    collections::BTreeMap,
    fmt, fs,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

const VALIDATOR: &str = "validator-service";

/// Heartbeat cadence for the skeleton worker loop. Later slices replace this
/// fixed tick with epoch-boundary–driven attestation issuance.
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Length in bytes of the ML-DSA-65 keygen seed consumed by
/// `ml_dsa_65::generate_key_pair` (matches the wallet's `KaspaPqMlDsa65KeyPair`).
const VALIDATOR_SEED_LEN: usize = 32;

/// Operating mode for the in-process validator service (ADR-0010, operational modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidatorMode {
    /// Sign and submit stake attestations when eligible (full validator).
    Active,
    /// Track eligibility and stay warm, but never sign/submit (hot spare for failover).
    Standby,
    /// Observe only — never sign, never submit (telemetry / dry-run). Default.
    #[default]
    Observer,
}

impl fmt::Display for ValidatorMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ValidatorMode::Active => "active",
            ValidatorMode::Standby => "standby",
            ValidatorMode::Observer => "observer",
        })
    }
}

impl FromStr for ValidatorMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "active" => Ok(ValidatorMode::Active),
            "standby" => Ok(ValidatorMode::Standby),
            "observer" => Ok(ValidatorMode::Observer),
            other => Err(format!("unknown validator mode '{other}' (expected one of: active, standby, observer)")),
        }
    }
}

/// Static validator configuration derived from CLI args (`--enable-validator` and friends).
#[derive(Debug, Clone)]
pub struct ValidatorConfig {
    pub mode: ValidatorMode,
    /// Path to the ML-DSA-65 signing seed file (64 hex chars = 32 bytes), if provided.
    pub key_path: Option<String>,
    /// Stake-bond outpoint backing this validator's attestations, as "txid:index", if provided.
    pub stake_bond: Option<String>,
    /// Path to the persistent equivocation-safety log (`validator-state.json`). When
    /// `None`, signing is disabled (the guard cannot be enforced without persistence).
    pub state_path: Option<PathBuf>,
}

/// Read and parse the validator signing seed from a hex file.
///
/// The file must contain exactly `VALIDATOR_SEED_LEN` bytes encoded as hex
/// (surrounding whitespace is ignored). The seed is then expanded into an
/// ML-DSA-65 keypair by [`ValidatorKey::from_seed`].
fn load_validator_seed(path: &str) -> Result<[u8; VALIDATOR_SEED_LEN], String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("cannot read validator key file '{path}': {e}"))?;
    let hex = raw.trim();
    let mut seed = [0u8; VALIDATOR_SEED_LEN];
    faster_hex::hex_decode(hex.as_bytes(), &mut seed)
        .map_err(|e| format!("validator key file '{path}' must contain {VALIDATOR_SEED_LEN} bytes as hex: {e}"))?;
    Ok(seed)
}

/// Materialised validator signing key: the ML-DSA-65 keypair plus its derived
/// overlay identity (`validator_id = BLAKE2b-512(public_key)`, per ADR-0008/0012).
///
/// Constructed once at startup from the seed file and held for the lifetime of
/// the service. The `keypair` is the signing material the attestation-issuance
/// slice will use via `sign_with_context(ATTESTATION_MLDSA65_CONTEXT, …)`; it is
/// stored now so the identity is derived from exactly the key that will sign.
struct ValidatorKey {
    keypair: ml_dsa_65::MLDSA65KeyPair,
    /// Overlay identity advertised to the network and matched against the bond.
    validator_id: Hash64,
}

impl ValidatorKey {
    fn from_seed(seed: [u8; VALIDATOR_SEED_LEN]) -> Self {
        let keypair = ml_dsa_65::generate_key_pair(seed);
        let validator_id = validator_id_from_pubkey(keypair.verification_key.as_ref());
        Self { keypair, validator_id }
    }

    /// Sign `message` under an explicit ML-DSA-65 `context` (domain separator) with
    /// fresh hedged randomness. Distinct contexts keep attestation signatures
    /// ([`ATTESTATION_MLDSA65_CONTEXT`]) and transaction-input signatures
    /// ([`MLDSA65_TX_CONTEXT`]) in disjoint domains — neither can be replayed as the other.
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> [u8; MLDSA65_SIG_LEN] {
        let mut randomness = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut randomness);
        let sig = ml_dsa_65::sign(&self.keypair.signing_key, message, context, randomness)
            .expect("ML-DSA-65 sign is infallible on a well-formed message");
        *sig.as_ref()
    }

    /// Sign a stake-attestation `message` digest under [`ATTESTATION_MLDSA65_CONTEXT`].
    /// Verifies via [`verify_mldsa65_with_context`] — the same call the
    /// `virtual_processor` aggregator uses.
    fn sign_attestation(&self, message: &[u8]) -> [u8; MLDSA65_SIG_LEN] {
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
    /// discovering the funding UTXO are the caller's job (the submission slice).
    #[allow(dead_code)] // wired by the submission slice (5b-3); exercised by tests today.
    fn build_funded_shard_tx(
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

    /// Verify an attestation signature against this key (local round-trip sanity check).
    fn verify_attestation(&self, message: &[u8], signature: &[u8]) -> bool {
        matches!(
            verify_mldsa65_with_context(self.keypair.verification_key.as_ref(), message, signature, ATTESTATION_MLDSA65_CONTEXT),
            Ok(true)
        )
    }
}

/// Parse a `"txid:index"` stake-bond reference into a [`TransactionOutpoint`].
/// `txid` is the 64-byte transaction id (128 hex chars); `index` is the output
/// index of the bond-creating output.
fn parse_stake_bond_ref(s: &str) -> Result<TransactionOutpoint, String> {
    let (txid, index) = s.split_once(':').ok_or_else(|| format!("stake-bond '{s}' must be in 'txid:index' form"))?;
    let transaction_id = Hash64::from_str(txid).map_err(|e| format!("stake-bond '{s}' has an invalid transaction id: {e}"))?;
    let index = index.parse::<u32>().map_err(|_| format!("stake-bond '{s}' has a non-numeric output index"))?;
    Ok(TransactionOutpoint::new(transaction_id, index))
}

const SIGNED_EPOCH_FILE_VERSION: u16 = 1;

/// On-disk shape of the per-validator equivocation-safety log (JSON). Bound to a
/// single `(validator_id, bond_outpoint)` so one host can never silently clobber
/// another key's safety record.
#[derive(serde::Serialize, serde::Deserialize)]
struct SignedEpochFile {
    version: u16,
    validator_id: Hash64,
    bond_outpoint: TransactionOutpoint,
    /// epoch -> the attestation signed for it.
    records: BTreeMap<u64, SignedEpochRecord>,
}

/// Persistent per-epoch signing log enforcing ADR-0011 equivocation safety across
/// restarts. Keyed in memory by epoch (the `(bond_outpoint, validator_id)` part of
/// the ADR triple is fixed for one running validator and lives in the file header).
struct SignedEpochStore {
    path: PathBuf,
    validator_id: Hash64,
    bond_outpoint: TransactionOutpoint,
    records: BTreeMap<u64, SignedEpochRecord>,
}

impl SignedEpochStore {
    /// Load the log for `(validator_id, bond_outpoint)` from `path`, or start empty if
    /// the file is absent. Errors if the file exists but belongs to a different
    /// validator/bond — refusing to operate is safer than risking cross-key equivocation.
    fn load_or_empty(path: PathBuf, validator_id: Hash64, bond_outpoint: TransactionOutpoint) -> Result<Self, String> {
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
    fn check(&self, candidate: &SignedEpochRecord) -> SignedEpochCheckOutcome {
        check_signed_epoch_record(self.records.get(&candidate.epoch), candidate)
    }

    /// Persist `record` for its epoch and flush atomically (temp file + rename so a
    /// crash mid-write cannot truncate the log). Call only after a successful sign and
    /// after [`Self::check`] returned [`SignedEpochCheckOutcome::Allow`].
    fn record_and_flush(&mut self, record: SignedEpochRecord) -> Result<(), String> {
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

/// In-process validator node service (skeleton).
pub struct ValidatorService {
    config: ValidatorConfig,
    consensus_manager: Arc<ConsensusManager>,
    tick_service: Arc<TickService>,
    /// Loaded signing key + derived identity. `None` until/unless a valid key is configured.
    key: Option<ValidatorKey>,
    /// Parsed stake-bond outpoint, if `--stake-bond` was provided and well-formed.
    bond_outpoint: Option<TransactionOutpoint>,
    /// Persistent equivocation-safety log. `None` (signing disabled) unless a key, bond,
    /// and state path are all present and the on-disk log belongs to this validator.
    signed_epochs: Mutex<Option<SignedEpochStore>>,
}

impl ValidatorService {
    pub fn new(config: ValidatorConfig, consensus_manager: Arc<ConsensusManager>, tick_service: Arc<TickService>) -> Self {
        // Validate configuration eagerly so misconfiguration surfaces at startup, not at first use.
        let key = match &config.key_path {
            Some(path) => match load_validator_seed(path) {
                Ok(seed) => {
                    let key = ValidatorKey::from_seed(seed);
                    info!("[{VALIDATOR}] loaded validator signing key from {path} (validator_id={})", key.validator_id);
                    Some(key)
                }
                Err(err) => {
                    warn!("[{VALIDATOR}] {err} — validator will run without a signing key");
                    None
                }
            },
            None => None,
        };
        let bond_outpoint = match &config.stake_bond {
            Some(s) => match parse_stake_bond_ref(s) {
                Ok(outpoint) => Some(outpoint),
                Err(err) => {
                    warn!("[{VALIDATOR}] {err}");
                    None
                }
            },
            None => None,
        };
        // The equivocation-safety log requires a key (validator_id), a bond, and a path.
        // A load failure (e.g. a foreign state file) leaves it `None`, which disables signing.
        let signed_epochs = match (&key, bond_outpoint, &config.state_path) {
            (Some(key), Some(outpoint), Some(path)) => match SignedEpochStore::load_or_empty(path.clone(), key.validator_id, outpoint)
            {
                Ok(store) => {
                    info!("[{VALIDATOR}] equivocation-safety log {} ({} prior epoch(s))", path.display(), store.records.len());
                    Some(store)
                }
                Err(err) => {
                    warn!("[{VALIDATOR}] {err} — signing disabled until resolved");
                    None
                }
            },
            _ => None,
        };
        Self { config, consensus_manager, tick_service, key, bond_outpoint, signed_epochs: Mutex::new(signed_epochs) }
    }

    pub async fn worker(self: &Arc<ValidatorService>) {
        let validator_id = match &self.key {
            Some(key) => key.validator_id.to_string(),
            None => "none".to_string(),
        };
        info!(
            "[{VALIDATOR}] starting (mode={}, validator_id={}, stake-bond={})",
            self.config.mode,
            validator_id,
            self.config.stake_bond.as_deref().unwrap_or("none"),
        );
        if self.config.mode == ValidatorMode::Active && self.key.is_none() {
            warn!("[{VALIDATOR}] mode=active but no signing key is loaded; no attestations can be produced");
        }

        loop {
            if let TickReason::Shutdown = self.tick_service.tick(Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await {
                break;
            }

            // Heartbeat: report the node tip, the validator's own bond status, and its
            // committee membership for the current epoch. When eligible (bond active AND
            // in committee) it also builds + signs the attestation for the sink and
            // verifies it locally — but does NOT gossip or submit it (the equivocation
            // guard and submission are later slices).
            let my_id = self.key.as_ref().map(|k| k.validator_id);
            let session = self.consensus_manager.consensus().session().await;
            let sink = session.async_get_sink_daa_score_timestamp().await;
            let dns = session.async_get_dns_confirmation().await;
            // The overlay reads return None on non-overlay networks too, so skip the
            // lookups there to avoid misleading status lines.
            let (bond, committee, attestation) = if dns.is_some() {
                let bond = match self.bond_outpoint {
                    Some(outpoint) => session.async_get_stake_bond(outpoint).await,
                    None => None,
                };
                let committee = session.async_get_validator_committee().await;
                // Eligible iff our bond is active AND our validator_id is in the committee.
                let eligible = match (&bond, &committee, my_id) {
                    (Some(b), Some(c), Some(id)) => is_bond_active_at(b, sink.daa_score) && c.members.contains(&id),
                    _ => false,
                };
                let attestation = match (eligible, self.bond_outpoint) {
                    (true, Some(outpoint)) => session.async_get_validator_attestation_target(outpoint).await,
                    _ => None,
                };
                (bond, committee, attestation)
            } else {
                (None, None, None)
            };
            drop(session);

            match dns {
                Some(conf) => {
                    let bond_status = match (self.bond_outpoint.is_some(), &bond) {
                        (false, _) => "unconfigured".to_string(),
                        (true, Some(b)) => {
                            format!("{:?}(active={})", effective_bond_status(b, sink.daa_score), is_bond_active_at(b, sink.daa_score))
                        }
                        (true, None) => "not-found".to_string(),
                    };
                    let committee_status = match (&committee, my_id) {
                        (Some(c), Some(id)) => format!(
                            "epoch={} in_committee={} (committee={}/active={})",
                            c.epoch,
                            c.members.contains(&id),
                            c.members.len(),
                            c.active_validator_count
                        ),
                        (Some(c), None) => format!("epoch={} no-signing-key (committee={})", c.epoch, c.members.len()),
                        (None, _) => "unavailable".to_string(),
                    };
                    info!(
                        "[{VALIDATOR}] heartbeat: mode={} sink_daa={} bond={} committee=[{}] dns_overlay=configured (stage={:?}, dns_confirmed={})",
                        self.config.mode, sink.daa_score, bond_status, committee_status, conf.rollout_stage, conf.dns_confirmed
                    );

                    // Eligible: sign under the equivocation guard and build the shard tx
                    // (DRY-RUN — verified locally and logged, but not gossiped/submitted yet).
                    if let (Some(target), Some(key), Some(outpoint)) = (&attestation, &self.key, self.bond_outpoint) {
                        self.guarded_sign(target, key, outpoint);
                    }
                }
                None => {
                    trace!("[{VALIDATOR}] heartbeat: mode={} sink_daa={} dns_overlay=not-configured", self.config.mode, sink.daa_score)
                }
            }
        }

        trace!("[{VALIDATOR}] worker exiting");
    }

    /// Equivocation-guarded attestation signing (ADR-0011). Consults the persistent
    /// signed-epoch log and signs only on [`SignedEpochCheckOutcome::Allow`], persisting
    /// the record before returning; refuses on `Block` (would be slashable) and skips on
    /// `AllowRebroadcast` (already signed this exact target). DRY-RUN: the signature is
    /// verified locally and logged but NOT gossiped/submitted (that is a later slice).
    fn guarded_sign(&self, target: &ValidatorAttestationTarget, key: &ValidatorKey, bond_outpoint: TransactionOutpoint) {
        let mut guard = self.signed_epochs.lock().unwrap();
        let Some(store) = guard.as_mut() else {
            trace!("[{VALIDATOR}] eligible for epoch {} but no equivocation-safety log; not signing", target.epoch);
            return;
        };
        // `signature_fingerprint` is not part of the equivocation predicate, so a
        // placeholder is fine for the pre-sign check; the stored record carries the real one.
        let candidate = SignedEpochRecord {
            epoch: target.epoch,
            target_hash: target.target_hash,
            target_daa_score: target.target_daa_score,
            signature_fingerprint: Hash64::from_bytes([0u8; 64]),
        };
        match store.check(&candidate) {
            SignedEpochCheckOutcome::Block => warn!(
                "[{VALIDATOR}] EQUIVOCATION BLOCKED: epoch {} already signed a different target; refusing to sign {}",
                target.epoch, target.target_hash
            ),
            SignedEpochCheckOutcome::AllowRebroadcast => {
                info!("[{VALIDATOR}] epoch {} already signed this target; rebroadcast-safe, not re-signing", target.epoch)
            }
            SignedEpochCheckOutcome::Allow => {
                let digest = target.message.as_bytes();
                let signature = key.sign_attestation(&digest);
                let verified = key.verify_attestation(&digest, &signature);
                let record = SignedEpochRecord { signature_fingerprint: signature_fingerprint(&signature), ..candidate };
                // Persist BEFORE any (future) gossip. If the flush fails, do not advance —
                // retrying next tick is safe, but signing without a durable record is not.
                if let Err(e) = store.record_and_flush(record) {
                    warn!("[{VALIDATOR}] failed to persist signed-epoch record (not advancing): {e}");
                    return;
                }

                // Build the attestation shard transaction (the exact artifact a future
                // submission slice will broadcast). NOT submitted: admitting a zero-input
                // overlay tx to the mempool (fee-funded input vs. a consensus exemption) is
                // an unresolved design question (ADR-0010 step 9), so we only build + log it.
                let attestation = StakeAttestation {
                    version: DNS_PAYLOAD_VERSION_V1,
                    validator_id: key.validator_id,
                    bond_outpoint,
                    epoch: target.epoch,
                    target_hash: target.target_hash,
                    target_daa_score: target.target_daa_score,
                    validator_set_commitment: target.validator_set_commitment,
                    signature: signature.to_vec(),
                };
                let shard_tx = stake_attestation_shard_tx(&single_attestation_shard(attestation));
                info!(
                    "[{VALIDATOR}] eligible — signed + built attestation shard tx (DRY-RUN, not submitted): epoch={} target={} vsc={} tx_id={} self_verify={}",
                    target.epoch,
                    target.target_hash,
                    target.validator_set_commitment,
                    shard_tx.id(),
                    verified
                );
            }
        }
    }
}

// service trait implementation for the validator service
impl AsyncService for ValidatorService {
    fn ident(self: Arc<Self>) -> &'static str {
        VALIDATOR
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", VALIDATOR);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", VALIDATOR);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn validator_mode_parsing_roundtrip() {
        for (s, m) in [("active", ValidatorMode::Active), ("standby", ValidatorMode::Standby), ("observer", ValidatorMode::Observer)] {
            assert_eq!(ValidatorMode::from_str(s).unwrap(), m);
            assert_eq!(m.to_string(), s);
        }
        // Case-insensitive and trimmed.
        assert_eq!(ValidatorMode::from_str("  ACTIVE ").unwrap(), ValidatorMode::Active);
        assert!(ValidatorMode::from_str("bogus").is_err());
        // Default is the safe observer mode.
        assert_eq!(ValidatorMode::default(), ValidatorMode::Observer);
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
