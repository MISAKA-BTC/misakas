//! PALW v2 agent monitor — the kaspad end of `--compute-endpoint`
//! (docs/misaka-palw-vps-canonical-worker-design-v0.1-ja.md §10.3, §12.3).
//!
//! Land stage, and deliberately so: this module OBSERVES a `palw-agent` and maintains a
//! capability state; nothing consensus-visible consumes that state yet. What it already
//! delivers is the §12.3 contract's shape:
//!
//! * **health probe** — a periodic `Health` round trip over the framed Borsh UDS protocol;
//! * **capability withdraw/quarantine** — the [`PalwAgentCapability`] handle flips to
//!   `Quarantined`/`Unreachable` the moment the agent does, which is the exact hook a future
//!   VLT v2 capability declaration must consult before announcing compute (VPS §6.1: declare
//!   nothing while quarantined);
//! * **validator-only continuation** — an absent, dying or quarantined agent changes NOTHING
//!   about the node's validator behavior. Compute is the part that stops (VPS §16 P0 item 10),
//!   which is why every failure here is a log line and a state, never an exit.
//!
//! Transitions are logged once, not per probe: a fleet operator reading logs needs the moment
//! the state changed, not thirty lines a minute confirming it has not.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kaspa_consensus_core::palw_v2::{PalwAgentHealthV1, PalwAgentStateV1};
use kaspa_core::{info, warn};
use misaka_palw::agent_client::PalwAgentClient;

const PALW_AGENT: &str = "palw-agent-monitor";
/// Probe cadence. The agent answers health without touching the worker or the model, so this is
/// cheap on both sides; 30 s bounds how stale the capability state can be.
const PROBE_INTERVAL: Duration = Duration::from_secs(30);
/// Connect/write/read ceiling for one health probe.
const PROBE_IO_TIMEOUT: Duration = Duration::from_secs(10);

/// What this node currently knows about its v2 compute runtime. `Available` means the agent
/// answered and is not quarantined (Ready or Busy — a busy slot is still a capable runtime).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PalwComputeCapability {
    /// No successful probe yet, or the last probe failed (agent down, socket missing, protocol
    /// violation). Compute must not be declared.
    Unreachable = 0,
    /// The agent is up and REFUSING work: its runtime failed a conformance gate (golden
    /// selftest, artifact hash). Compute must not be declared, and unlike `Unreachable` this
    /// state names a runtime that needs operator attention, not a restart.
    Quarantined = 1,
    /// The agent is serving its gated runtime.
    Available = 2,
}

/// Shared, lock-free-readable capability handle. Clone the `Arc` anywhere that will later need
/// to ask "may this node declare v2 compute right now?".
pub struct PalwAgentCapability {
    endpoint: PathBuf,
    state: AtomicU8,
    last_health: Mutex<Option<PalwAgentHealthV1>>,
}

impl PalwAgentCapability {
    pub fn capability(&self) -> PalwComputeCapability {
        match self.state.load(Ordering::Acquire) {
            2 => PalwComputeCapability::Available,
            1 => PalwComputeCapability::Quarantined,
            _ => PalwComputeCapability::Unreachable,
        }
    }

    /// The last health frame the agent answered with, if any. Telemetry — never a validation
    /// input.
    pub fn last_health(&self) -> Option<PalwAgentHealthV1> {
        self.last_health.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    pub fn endpoint(&self) -> &std::path::Path {
        &self.endpoint
    }

    fn store(&self, capability: PalwComputeCapability, health: Option<PalwAgentHealthV1>) {
        *self.last_health.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = health;
        self.state.store(capability as u8, Ordering::Release);
    }
}

fn short(hash: &kaspa_hashes::Hash64) -> String {
    faster_hex::hex_string(&hash.as_byte_slice()[..8])
}

/// Spawns the monitor thread and returns the capability handle. The thread runs for the life of
/// the process — kaspad's shutdown model for auxiliary observers is process exit.
pub fn spawn_palw_agent_monitor(endpoint: PathBuf) -> Arc<PalwAgentCapability> {
    let handle = Arc::new(PalwAgentCapability {
        endpoint: endpoint.clone(),
        state: AtomicU8::new(PalwComputeCapability::Unreachable as u8),
        last_health: Mutex::new(None),
    });
    let shared = Arc::clone(&handle);
    info!(
        "[{PALW_AGENT}] monitoring the PALW v2 agent at {} (observation only at this stage; \
         the VLT compute role is configured separately)",
        endpoint.display()
    );
    std::thread::Builder::new()
        .name("palw-agent-monitor".into())
        .spawn(move || {
            let client = PalwAgentClient::new(&endpoint, PROBE_IO_TIMEOUT);
            let mut last: Option<PalwComputeCapability> = None;
            loop {
                let (capability, health) = match client.health() {
                    Ok(health) => {
                        let capability = match health.state {
                            PalwAgentStateV1::Quarantined => PalwComputeCapability::Quarantined,
                            PalwAgentStateV1::Ready | PalwAgentStateV1::Busy => PalwComputeCapability::Available,
                        };
                        (capability, Some(health))
                    }
                    Err(e) => {
                        if last != Some(PalwComputeCapability::Unreachable) {
                            warn!("[{PALW_AGENT}] agent at {} is unreachable ({e}); v2 compute capability withdrawn, validator unaffected", endpoint.display());
                        }
                        (PalwComputeCapability::Unreachable, None)
                    }
                };
                if last != Some(capability) {
                    match (&capability, &health) {
                        (PalwComputeCapability::Available, Some(h)) => info!(
                            "[{PALW_AGENT}] agent available: manifest {}… golden {}… selftest_passed={} (jobs ok/rej/fail {}/{}/{})",
                            short(&h.runtime_manifest_hash),
                            short(&h.golden_vector_root),
                            h.selftest_passed,
                            h.jobs_ok,
                            h.jobs_rejected,
                            h.jobs_failed
                        ),
                        (PalwComputeCapability::Quarantined, Some(h)) => warn!(
                            "[{PALW_AGENT}] agent QUARANTINED (manifest {}…): its runtime failed a conformance gate; \
                             v2 compute capability withdrawn until an operator intervenes — validator unaffected",
                            short(&h.runtime_manifest_hash)
                        ),
                        _ => {}
                    }
                }
                shared.store(capability, health);
                last = Some(capability);
                std::thread::sleep(PROBE_INTERVAL);
            }
        })
        .expect("spawning the palw-agent monitor thread");
    handle
}
