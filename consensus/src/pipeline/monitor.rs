use super::ProcessingCounters;
use kaspa_core::{
    info,
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    trace, warn,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const MONITOR: &str = "consensus-monitor";

pub struct ConsensusMonitor {
    // Counters
    counters: Arc<ProcessingCounters>,

    // Tick service
    tick_service: Arc<TickService>,
}

impl ConsensusMonitor {
    pub fn new(counters: Arc<ProcessingCounters>, tick_service: Arc<TickService>) -> ConsensusMonitor {
        ConsensusMonitor { counters, tick_service }
    }

    pub async fn worker(self: &Arc<ConsensusMonitor>) {
        let mut last_snapshot = self.counters.snapshot();
        let mut last_log_time = Instant::now();
        let snapshot_interval = 10;
        loop {
            if let TickReason::Shutdown = self.tick_service.tick(Duration::from_secs(snapshot_interval)).await {
                // Let the system print final logs before exiting
                tokio::time::sleep(Duration::from_millis(500)).await;
                break;
            }

            let snapshot = self.counters.snapshot();
            if snapshot == last_snapshot {
                // No update, avoid printing useless info
                last_log_time = Instant::now();
                continue;
            }

            // Subtract the snapshots
            let delta = &snapshot - &last_snapshot;
            let now = Instant::now();

            info!(
                "Processed {} blocks and {} headers in the last {:.2}s ({} transactions; {} UTXO-validated blocks; {:.2} parents; {:.2} mergeset; {:.2} TPB; {:.1} mass)",
                delta.body_counts,
                delta.header_counts,
                (now - last_log_time).as_secs_f64(),
                delta.txs_counts,
                delta.chain_block_counts,
                if delta.header_counts != 0 { delta.dep_counts as f64 / delta.header_counts as f64 } else { 0f64 },
                if delta.header_counts != 0 { delta.mergeset_counts as f64 / delta.header_counts as f64 } else { 0f64 },
                if delta.body_counts != 0 { delta.txs_counts as f64 / delta.body_counts as f64 } else { 0f64 },
                if delta.body_counts != 0 { delta.mass_counts as f64 / delta.body_counts as f64 } else { 0f64 },
            );

            // [ibd-perf §7-1] Per-header serial-fraction breakdown to decide whether the
            // compute-parallel/commit-serial lane is worthwhile or Phase-B is the ceiling.
            if delta.hdr_timed_counts != 0 {
                let n = delta.hdr_timed_counts as f64;
                let validate_us = delta.hdr_validate_ns as f64 / n / 1000.0;
                let commit_us = delta.hdr_commit_ns as f64 / n / 1000.0;
                let addblock_us = delta.hdr_addblock_ns as f64 / n / 1000.0;
                let dbwrite_us = delta.hdr_dbwrite_ns as f64 / n / 1000.0;
                let heldlock_us = delta.hdr_heldlock_ns as f64 / n / 1000.0;
                let total_us = validate_us + commit_us;
                let f_serial = if total_us > 0.0 { commit_us / total_us } else { 0.0 };
                let f_reach = if total_us > 0.0 { addblock_us / total_us } else { 0.0 };
                let ceiling = if f_serial > 0.0 { 1.0 / f_serial } else { 0.0 };
                info!(
                    "[ibd-perf] {} hdrs avg: validate(A,parallelizable) {:.1}us | commit(serial) {:.1}us [add_block {:.1} + db.write {:.1}; held-lock {:.1}] | f_serial={:.3} f_reach={:.3} | parallelize-ceiling {:.2}x",
                    delta.hdr_timed_counts, validate_us, commit_us, addblock_us, dbwrite_us, heldlock_us, f_serial, f_reach, ceiling,
                );
            }

            if delta.chain_disqualified_counts > 0 {
                warn!(
                    "Consensus detected UTXO-invalid blocks which are disqualified from the virtual selected chain (possibly due to inheritance): {} disqualified vs. {} valid chain blocks",
                    delta.chain_disqualified_counts, delta.chain_block_counts
                );
            }

            last_snapshot = snapshot;
            last_log_time = now;
        }

        trace!("monitor thread exiting");
    }
}

// service trait implementation for Monitor
impl AsyncService for ConsensusMonitor {
    fn ident(self: Arc<Self>) -> &'static str {
        MONITOR
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", MONITOR);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", MONITOR);
            Ok(())
        })
    }
}
