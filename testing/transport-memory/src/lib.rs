//! Isolated tests of production transport sources; no replacement budget/codec.
extern crate self as kaspa_core;
extern crate self as kaspa_utils_tower;
#[path = "../../../core/src/memory_budget.rs"]
pub mod memory_budget;
#[path = "../../../utils/tower/src/transport.rs"]
pub mod transport;
#[path = "../../../rpc/core/src/api/retained/utxo_memory.rs"]
pub mod utxo_memory;
#[path = "../../../utils/tower/src/wire_preflight.rs"]
pub mod wire_preflight;
pub mod p2p {
    include!(concat!(env!("OUT_DIR"), "/p2p/protowire.rs"));
}
pub mod rpc {
    include!(concat!(env!("OUT_DIR"), "/rpc/protowire.kaspapq.rs"));
}
pub mod fixtures {
    include!(concat!(env!("OUT_DIR"), "/oom.memory.rs"));
}
#[cfg(test)]
mod preflight_tests;
#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
#[path = "../../../utils/tower/tests/transport_budget.rs"]
mod transport_budget_tests;
