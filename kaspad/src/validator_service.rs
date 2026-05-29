//! kaspa-pq Phase 11 (ADR-0010): in-process validator node service.
//!
//! This is the wiring skeleton (PR-11.5). It parses validator CLI configuration,
//! loads the ML-DSA-65 signing seed, and runs an async heartbeat loop that waits
//! for the node to be usable and logs validator status. It deliberately does **not**
//! yet evaluate per-epoch eligibility, sign stake attestations, or submit attestation
//! shard transactions — those are later Phase 11 slices. The service is registered
//! only when `--enable-validator` is set, so default node behavior is unchanged.

use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    info,
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    trace, warn,
};
use std::{fmt, fs, str::FromStr, sync::Arc, time::Duration};

const VALIDATOR: &str = "validator-service";

/// Heartbeat cadence for the skeleton worker loop. Later slices replace this
/// fixed tick with epoch-boundary–driven attestation issuance.
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Length in bytes of the ML-DSA-65 keygen seed consumed by
/// `KaspaPqMlDsa65KeyPair::from_seed`.
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
/// (surrounding whitespace is ignored). The actual ML-DSA-65 keypair is
/// constructed by a later signing slice; this skeleton only validates and holds the seed.
fn load_validator_seed(path: &str) -> Result<[u8; VALIDATOR_SEED_LEN], String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("cannot read validator key file '{path}': {e}"))?;
    let hex = raw.trim();
    let mut seed = [0u8; VALIDATOR_SEED_LEN];
    faster_hex::hex_decode(hex.as_bytes(), &mut seed)
        .map_err(|e| format!("validator key file '{path}' must contain {VALIDATOR_SEED_LEN} bytes as hex: {e}"))?;
    Ok(seed)
}

/// Light shape validation of a "txid:index" stake-bond reference.
/// Full `TransactionOutpoint` construction is deferred to the eligibility slice.
fn validate_stake_bond_ref(s: &str) -> Result<(), String> {
    let (txid, index) = s.split_once(':').ok_or_else(|| format!("stake-bond '{s}' must be in 'txid:index' form"))?;
    if txid.is_empty() {
        return Err(format!("stake-bond '{s}' is missing a transaction id"));
    }
    index.parse::<u32>().map_err(|_| format!("stake-bond '{s}' has a non-numeric output index"))?;
    Ok(())
}

/// In-process validator node service (skeleton).
pub struct ValidatorService {
    config: ValidatorConfig,
    consensus_manager: Arc<ConsensusManager>,
    tick_service: Arc<TickService>,
    /// Loaded signing seed, held for later signing slices. `None` until/unless a key is configured.
    seed: Option<[u8; VALIDATOR_SEED_LEN]>,
}

impl ValidatorService {
    pub fn new(config: ValidatorConfig, consensus_manager: Arc<ConsensusManager>, tick_service: Arc<TickService>) -> Self {
        // Validate configuration eagerly so misconfiguration surfaces at startup, not at first use.
        let seed = match &config.key_path {
            Some(path) => match load_validator_seed(path) {
                Ok(seed) => {
                    info!("[{VALIDATOR}] loaded validator signing key from {path}");
                    Some(seed)
                }
                Err(err) => {
                    warn!("[{VALIDATOR}] {err} — validator will run without a signing key");
                    None
                }
            },
            None => None,
        };
        if let Some(Err(err)) = config.stake_bond.as_deref().map(validate_stake_bond_ref) {
            warn!("[{VALIDATOR}] {err}");
        }
        Self { config, consensus_manager, tick_service, seed }
    }

    pub async fn worker(self: &Arc<ValidatorService>) {
        info!(
            "[{VALIDATOR}] starting (mode={}, signing-key={}, stake-bond={})",
            self.config.mode,
            if self.seed.is_some() { "loaded" } else { "none" },
            self.config.stake_bond.as_deref().unwrap_or("none"),
        );
        if self.config.mode == ValidatorMode::Active && self.seed.is_none() {
            warn!("[{VALIDATOR}] mode=active but no signing key is loaded; no attestations can be produced");
        }

        loop {
            if let TickReason::Shutdown = self.tick_service.tick(Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await {
                break;
            }

            // Skeleton heartbeat: report the node tip and whether the DNS overlay is configured.
            // NOTE: this slice does not evaluate eligibility, sign, or submit anything.
            let session = self.consensus_manager.consensus().session().await;
            let sink = session.async_get_sink_daa_score_timestamp().await;
            let dns = session.async_get_dns_confirmation().await;
            drop(session);

            match dns {
                Some(conf) => info!(
                    "[{VALIDATOR}] heartbeat: mode={} sink_daa={} dns_overlay=configured (stage={:?}, dns_confirmed={})",
                    self.config.mode, sink.daa_score, conf.rollout_stage, conf.dns_confirmed
                ),
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
    fn stake_bond_ref_shape_validation() {
        assert!(validate_stake_bond_ref("abcd:0").is_ok());
        assert!(validate_stake_bond_ref("abcd:7").is_ok());
        assert!(validate_stake_bond_ref("abcd").is_err()); // no index
        assert!(validate_stake_bond_ref(":0").is_err()); // no txid
        assert!(validate_stake_bond_ref("abcd:x").is_err()); // non-numeric index
    }
}
