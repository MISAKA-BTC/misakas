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
    chain_participation::{ChainParticipation, ChainParticipationPersistence, ChainParticipationSnapshot},
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
    /// Absolute unix-ms deadline of a `CandidateReview` floor. Absolute, not a duration, so a
    /// restart neither extends the floor nor escapes it.
    review_until_ms: u64,
    /// Fields added after the first release. `serde(default)` so rows written before they existed
    /// still decode, and their defaults are the safe ones: a node that has never been recorded as
    /// having participated is treated as never having done so.
    #[serde(default)]
    ever_ready: bool,
    #[serde(default)]
    adoption_generation: u64,
    #[serde(default)]
    switches: u32,
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
}

impl ChainParticipationPersistence for ChainParticipationStore {
    fn persist(&self, snapshot: ChainParticipationSnapshot) {
        let persisted = PersistedChainParticipation {
            state: snapshot.state.as_str().to_owned(),
            review_until_ms: snapshot.review_until_ms,
            ever_ready: snapshot.ever_ready,
            adoption_generation: snapshot.adoption_generation,
            switches: snapshot.switches,
        };
        // Non-fatal by contract: a node that cannot write this must keep running with an in-memory
        // gate rather than abort. Louder than a debug line because the consequence is that a restart
        // would silently resume participation.
        if let Err(e) = self.item.lock().unwrap().write(DirectDbWriter::new(&self.db), &persisted) {
            warn!("Could not persist chain-participation state {}: {}. A restart will not preserve it.", snapshot.state.as_str(), e);
        }
    }

    /// What was last written, if anything. `None` on a fresh node.
    ///
    /// A row that cannot be read or understood is reported as a quarantine rather than as absence:
    /// something wrote a participation state and we cannot tell what it said, and `Ready` is the one
    /// answer that could put a node back to signing on a chain it had already refused to vouch for.
    fn restore(&self) -> Option<ChainParticipationSnapshot> {
        let read = self.item.lock().unwrap().read().optional();
        let quarantined = |generation, switches| ChainParticipationSnapshot {
            state: ChainParticipation::Quarantined,
            review_until_ms: 0,
            ever_ready: false,
            adoption_generation: generation,
            switches,
        };
        match read {
            Ok(None) => None,
            Err(e) => {
                warn!("Could not read the stored chain-participation state ({e}); treating this node as quarantined.");
                Some(quarantined(0, 0))
            }
            Ok(Some(p)) => {
                let state = match p.state.as_str() {
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
                        return Some(quarantined(p.adoption_generation, p.switches));
                    }
                };
                Some(ChainParticipationSnapshot {
                    state,
                    review_until_ms: p.review_until_ms,
                    ever_ready: p.ever_ready,
                    adoption_generation: p.adoption_generation,
                    switches: p.switches,
                })
            }
        }
    }
}
