//! Durable backing for [`ChainParticipationGate`].
//!
//! A quarantine that a process restart clears is not a quarantine. The gate closes because an IBD
//! may have replaced this node's active consensus with a branch nothing has compared — and killing
//! the process does not compare it. Without this, `kaspad` restart was a supported way to resume
//! mining and attesting on a chain the node had already decided it could not vouch for.
//!
//! Kept in the node-level **meta** DB, deliberately not in a consensus DB: the state being recorded
//! is "a `staging.commit()` may have swapped my consensus out", so storing it inside the thing that
//! gets swapped would lose it precisely when it is needed.

use std::sync::Arc;

use kaspa_core::{
    chain_participation::{ChainParticipation, ChainParticipationPersistence},
    warn,
};
use kaspa_database::{
    prelude::{CachedDbItem, DB, DirectDbWriter, StoreResultExt},
    registry::DatabaseStorePrefixes,
};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Wire form. `state` is stored as its stable slug rather than a discriminant so that reordering
/// the enum cannot silently turn a quarantine into `Ready` after an upgrade — an unknown slug is
/// treated as quarantine, which is the safe direction.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PersistedChainParticipation {
    state: String,
    /// Syncs abandoned for a verified-better chain. `Option` so rows written before this field
    /// existed still decode — absent reads as zero, which is the fresh-node value.
    #[serde(default)]
    switches: u32,
    /// Absolute unix-ms deadline of a `CandidateReview` floor. Absolute, not a duration, so a
    /// restart neither extends the floor nor escapes it.
    review_until_ms: u64,
}

pub struct ChainParticipationStore {
    item: Mutex<CachedDbItem<PersistedChainParticipation>>,
    db: Arc<DB>,
}

// `DB` is not `Debug`, and the gate only ever needs to name its persistence backend in diagnostics.
impl std::fmt::Debug for ChainParticipationStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChainParticipationStore")
    }
}

impl ChainParticipationStore {
    pub fn new(db: Arc<DB>) -> Self {
        let item = CachedDbItem::new(db.clone(), DatabaseStorePrefixes::ChainParticipation.into());
        Self { item: Mutex::new(item), db }
    }

    /// What was last written, if anything. `None` on a fresh node or an unreadable row.
    ///
    /// A row that fails to decode is reported as a quarantine rather than as absence: something
    /// wrote a participation state and we cannot tell what it said, and guessing `Ready` is the one
    /// answer that could put a compromised node back to signing.
    pub fn load(&self) -> Option<(ChainParticipation, u64)> {
        let read = self.item.lock().unwrap().read().optional();
        match read {
            Ok(None) => None,
            // Unreadable is not the same as absent. Something wrote a state and we cannot tell what
            // it said; `Ready` is the one answer that could put a node back to signing on a chain it
            // had already refused to vouch for.
            Err(e) => {
                warn!("Could not read the stored chain-participation state ({e}); treating this node as quarantined.");
                Some((ChainParticipation::Quarantined, 0))
            }
            Ok(Some(persisted)) => {
                let state = match persisted.state.as_str() {
                    "ready" => ChainParticipation::Ready,
                    "ibd-running" => ChainParticipation::IbdRunning,
                    "candidate-review" => ChainParticipation::CandidateReview,
                    "quarantined" => ChainParticipation::Quarantined,
                    unknown => {
                        warn!(
                            "Stored chain-participation state {:?} is not recognized by this build; treating as quarantined. \
                             Resolve which chain this node is on before clearing it.",
                            unknown
                        );
                        ChainParticipation::Quarantined
                    }
                };
                Some((state, persisted.review_until_ms))
            }
        }
    }
}

impl ChainParticipationPersistence for ChainParticipationStore {
    fn persist_switches(&self, switches: u32) {
        let (state, review_until_ms) = self.load().map(|(s, r)| (s.as_str().to_owned(), r)).unwrap_or(("ready".to_owned(), 0));
        let persisted = PersistedChainParticipation { state, review_until_ms, switches };
        if let Err(e) = self.item.lock().unwrap().write(DirectDbWriter::new(&self.db), &persisted) {
            warn!("Could not persist the chain-switch count ({switches}): {e}. A restart will not preserve it.");
        }
    }

    fn restore_switches(&self) -> u32 {
        self.item.lock().unwrap().read().optional().ok().flatten().map(|p| p.switches).unwrap_or(0)
    }

    fn persist(&self, state: ChainParticipation, review_until_ms: u64) {
        let switches = self.restore_switches();
        let persisted = PersistedChainParticipation { state: state.as_str().to_owned(), review_until_ms, switches };
        // Non-fatal by contract: a node that cannot write this must keep running with an in-memory
        // gate rather than abort. It is louder than a debug line because the consequence is that a
        // restart would silently resume participation.
        if let Err(e) = self.item.lock().unwrap().write(DirectDbWriter::new(&self.db), &persisted) {
            warn!("Could not persist chain-participation state {}: {}. A restart will not preserve it.", state.as_str(), e);
        }
    }
}
