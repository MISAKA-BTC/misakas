//! Read-only: print the class table an operator cannot otherwise see.
//!
//! A producer is gated on the pair (share, budget). `GetPalwProducerFacts` returns the budget and
//! not the share, so "budget 0" covers two different faults — a class that was never granted
//! share, and a class that holds share but has no row in this epoch's table — and a node that
//! holds forever could not say which. That ambiguity cost a live investigation: the mid-epoch
//! budget defect was diagnosed from "budget 0" alone, and the boundary that should have cleared it
//! did not, because the class had no share in the first place.

use std::sync::Arc;

use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    info,
    task::service::{AsyncService, AsyncServiceFuture},
    trace, warn,
};

const PALW_DUMP: &str = "palw-dump";

pub struct PalwDumpService {
    consensus_manager: Arc<ConsensusManager>,
}

impl PalwDumpService {
    pub fn new(consensus_manager: Arc<ConsensusManager>) -> Self {
        Self { consensus_manager }
    }

    async fn worker(self: &Arc<Self>) {
        // Wait for a chain to describe. Dumping an empty store says nothing and reads as "no
        // classes", which is the same wrong answer this exists to stop.
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let session = self.consensus_manager.consensus().unguarded_session();
            if session.async_is_consensus_in_transitional_ibd_state().await {
                continue;
            }
            // **Refuse to answer from genesis.** The first version of this read the class table
            // before the virtual state was up and printed "1 class ... at daa 0" -- true of the
            // genesis state and worthless as an answer about the tip. A diagnostic that can report
            // the wrong epoch without saying so is the exact fault it was written to find.
            let daa = session.get_virtual_daa_score();
            if daa == 0 {
                trace!("[{PALW_DUMP}] virtual daa is still 0 — waiting for the tip rather than answering from genesis");
                continue;
            }
            let rows = session.palw_v2_class_table();
            if rows.is_empty() {
                warn!("[{PALW_DUMP}] this chain holds no PALW classes, or is not a ConsensusV2 network");
                return;
            }
            info!("[{PALW_DUMP}] {} class(es) at daa {}", rows.len(), daa);
            for row in rows {
                info!(
                    "[{PALW_DUMP}]   class={} base={} status={} share={} budget={}",
                    row.class_id,
                    row.is_base_class,
                    row.status,
                    // The distinction the whole file is for: absent share is not zero budget.
                    row.share_permille.map(|s| s.to_string()).unwrap_or_else(|| "NONE".to_string()),
                    row.budget_blocks
                );
            }
            return;
        }
    }
}

impl AsyncService for PalwDumpService {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_DUMP
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_DUMP);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_DUMP);
            Ok(())
        })
    }
}
