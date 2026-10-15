//! **Node-local RFC-0016 observations — NOT consensus.** Counters this process saw while validating or pre-checking (the fold's
//! refusals for rights and for capacity, as asked by the producer's pre-check, the virtual processor and RPC readers alike). They
//! differ between nodes, restart at zero with the process, and may count one refusal more than once (a block re-validated, a
//! pre-check repeated). The report exposes them under `node_local` and never as an outcome; nothing consensual reads them.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::Hash64;

/// What kind of refusal was observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PalwRfc0016ObservationV1 {
    RefusedForRights,
    RefusedForCapacity,
    QueueFull,
    Queued,
}

static OBSERVATIONS: Mutex<BTreeMap<(Hash64, PalwRfc0016ObservationV1), u64>> = Mutex::new(BTreeMap::new());

/// Count one observation for a model class (best effort: a poisoned lock drops it).
pub fn palw_rfc0016_observe_v1(class: &Hash64, kind: PalwRfc0016ObservationV1) {
    if let Ok(mut map) = OBSERVATIONS.lock() {
        *map.entry((*class, kind)).or_insert(0) += 1;
    }
}

/// The observations so far for a class (node-local).
pub fn palw_rfc0016_observed_v1(class: &Hash64, kind: PalwRfc0016ObservationV1) -> u64 {
    OBSERVATIONS.lock().map(|map| map.get(&(*class, kind)).copied().unwrap_or(0)).unwrap_or(0)
}
