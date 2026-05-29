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

use kaspa_consensus_core::dns_finality::{
    ATTESTATION_MLDSA65_CONTEXT, effective_bond_status, is_bond_active_at, validator_id_from_pubkey,
};
use kaspa_consensus_core::tx::TransactionOutpoint;
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
use kaspa_txscript::{MLDSA65_SIG_LEN, verify_mldsa65_with_context};
use libcrux_ml_dsa::ml_dsa_65;
use rand::RngCore;
use std::{fmt, fs, str::FromStr, sync::Arc, time::Duration};

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

    /// Sign a stake-attestation `message` digest under [`ATTESTATION_MLDSA65_CONTEXT`].
    /// The distinct context guarantees the signature can never be replayed as a
    /// transaction signature, and verifies via [`verify_mldsa65_with_context`] — the
    /// same call the `virtual_processor` aggregator uses.
    fn sign_attestation(&self, message: &[u8]) -> [u8; MLDSA65_SIG_LEN] {
        let mut randomness = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut randomness);
        let sig = ml_dsa_65::sign(&self.keypair.signing_key, message, ATTESTATION_MLDSA65_CONTEXT, randomness)
            .expect("ML-DSA-65 sign is infallible on a well-formed message");
        *sig.as_ref()
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

/// In-process validator node service (skeleton).
pub struct ValidatorService {
    config: ValidatorConfig,
    consensus_manager: Arc<ConsensusManager>,
    tick_service: Arc<TickService>,
    /// Loaded signing key + derived identity. `None` until/unless a valid key is configured.
    key: Option<ValidatorKey>,
    /// Parsed stake-bond outpoint, if `--stake-bond` was provided and well-formed.
    bond_outpoint: Option<TransactionOutpoint>,
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
        Self { config, consensus_manager, tick_service, key, bond_outpoint }
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

                    // Eligible: build + sign + locally verify the attestation. DRY-RUN —
                    // not gossiped/submitted (equivocation guard is the next slice).
                    if let (Some(target), Some(key)) = (&attestation, &self.key) {
                        let digest = target.message.as_bytes();
                        let signature = key.sign_attestation(&digest);
                        let verified = key.verify_attestation(&digest, &signature);
                        info!(
                            "[{VALIDATOR}] eligible — signed attestation (DRY-RUN, not submitted): epoch={} target={} vsc={} sig_len={} self_verify={}",
                            target.epoch,
                            target.target_hash,
                            target.validator_set_commitment,
                            signature.len(),
                            verified
                        );
                    }
                }
                None => {
                    trace!("[{VALIDATOR}] heartbeat: mode={} sink_daa={} dns_overlay=not-configured", self.config.mode, sink.daa_score)
                }
            }
        }

        trace!("[{VALIDATOR}] worker exiting");
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
}
