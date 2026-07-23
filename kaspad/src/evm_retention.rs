//! The service that actually runs EVM retention.
//!
//! The planner and executor live in consensus; this is only the clock. It exists
//! as its own service, rather than as work hung off the L1 pruning worker,
//! because that worker stands down while consensus is transitional — correctly,
//! and for the entire IBD window, which is when the node writes the most and
//! when the 144 GB accumulated.
//!
//! Two operational properties:
//!
//! * It runs at a fixed interval and each pass is row-bounded, so retention is a
//!   background trickle rather than a periodic stall. Reclaiming space by making
//!   the node unresponsive is not reclaiming space.
//! * It escalates under disk pressure by running MORE OFTEN, never by relaxing
//!   what may be deleted. The state-history segments stay behind the L1 pruning
//!   point at every pressure level: an index can be rebuilt, a state diff cannot.

use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    info,
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    trace,
};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::disk_guard::{DiskPressure, DiskPressureHandle};

pub const SERVICE_NAME: &str = "evm-retention";

/// Shortest interval the pressure escalation may reach. A pass is bounded, so a
/// tighter loop mostly buys write amplification.
const MIN_INTERVAL: Duration = Duration::from_secs(5);

pub struct EvmRetentionService {
    tick_service: Arc<TickService>,
    consensus_manager: Arc<ConsensusManager>,
    interval: Duration,
    pressure: DiskPressureHandle,
}

impl EvmRetentionService {
    pub fn new(
        tick_service: Arc<TickService>,
        consensus_manager: Arc<ConsensusManager>,
        interval: Duration,
        pressure: DiskPressureHandle,
    ) -> Self {
        Self { tick_service, consensus_manager, interval: interval.max(MIN_INTERVAL), pressure }
    }

    pub async fn worker(&self) {
        info!("[{SERVICE_NAME}] EVM retention runs every {}s, independently of L1 pruning", self.interval.as_secs());
        let mut reported_inactive = false;
        let mut total_rows = 0u64;

        loop {
            let interval = interval_for(self.interval, self.pressure.level());
            if let TickReason::Shutdown = self.tick_service.tick(interval).await {
                break;
            }
            let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
            // On a blocking pool: a pass reads and deletes RocksDB rows, which is
            // exactly the work that must not run on an async executor thread.
            let session = self.consensus_manager.consensus().unguarded_session();
            let report = session.spawn_blocking(move |c| c.run_evm_retention_pass(now_ms)).await;

            if report.inactive {
                // Once, not every tick: an inert lane is a configuration, not an
                // event.
                if !reported_inactive {
                    info!("[{SERVICE_NAME}] the EVM lane is inert on this network; retention has nothing to do");
                    reported_inactive = true;
                }
                continue;
            }
            reported_inactive = false;
            if report.rows_deleted > 0 {
                total_rows += report.rows_deleted;
                info!(
                    "[{SERVICE_NAME}] reclaimed {} rows across {} segment(s) ({} total since start)",
                    report.rows_deleted, report.segments_advanced, total_rows
                );
            }
        }
        trace!("[{SERVICE_NAME}] worker exiting");
    }
}

impl AsyncService for EvmRetentionService {
    fn ident(self: Arc<Self>) -> &'static str {
        SERVICE_NAME
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {SERVICE_NAME}");
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{SERVICE_NAME} stopped");
            Ok(())
        })
    }
}

/// Under pressure, run more often — but never delete more.
///
/// What may be reclaimed is a correctness question, and it does not get a
/// disk-space override: an index can be rebuilt from retained blocks, a state
/// diff cannot be recovered at all. Pressure buys frequency, nothing else.
///
/// A free function so the escalation is testable without standing up a
/// consensus.
fn interval_for(base: Duration, pressure: DiskPressure) -> Duration {
    match pressure {
        DiskPressure::Normal => base.max(MIN_INTERVAL),
        DiskPressure::Warning => (base / 2).max(MIN_INTERVAL),
        DiskPressure::Critical | DiskPressure::Emergency => MIN_INTERVAL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_shortens_the_interval_and_never_below_the_floor() {
        let base = Duration::from_secs(60);
        assert_eq!(interval_for(base, DiskPressure::Normal), base);
        assert_eq!(interval_for(base, DiskPressure::Warning), Duration::from_secs(30));
        // Critical and emergency both go to the floor: there is nothing more
        // aggressive left that is still safe, and passes are row-bounded anyway.
        assert_eq!(interval_for(base, DiskPressure::Critical), MIN_INTERVAL);
        assert_eq!(interval_for(base, DiskPressure::Emergency), MIN_INTERVAL);
    }

    #[test]
    fn a_short_configured_interval_is_never_made_shorter() {
        // Otherwise pressure could turn retention into a hot loop that competes
        // with block processing for the same disk.
        let base = Duration::from_secs(1);
        for p in [DiskPressure::Normal, DiskPressure::Warning, DiskPressure::Critical, DiskPressure::Emergency] {
            assert!(interval_for(base, p) >= MIN_INTERVAL, "{p:?} produced a sub-floor interval");
        }
    }
}
