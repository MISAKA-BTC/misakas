//! Isolated tests of production transport sources; no replacement budget/codec.
extern crate self as kaspa_core;
extern crate self as kaspa_utils_tower;
#[path = "../../../core/src/memory_budget.rs"]
pub mod memory_budget;
#[path = "../../../protocol/flows/src/snapshot_memory.rs"]
pub mod snapshot_memory;
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

// These tiny types instantiate only the unused error arms of the real macro.
// The reproduction exercises its successful move, not protocol error policy.
#[cfg(test)]
pub mod common {
    #[derive(Debug)]
    pub struct PayloadKind;
    impl From<&crate::p2p::kaspad_message::Payload> for PayloadKind {
        fn from(_: &crate::p2p::kaspad_message::Payload) -> Self {
            Self
        }
    }
    #[derive(Debug)]
    pub enum ProtocolError {
        UnexpectedMessage(&'static str, Option<PayloadKind>),
        ConnectionClosed,
    }
}
#[cfg(test)]
include!(concat!(env!("OUT_DIR"), "/production_unwrap_message.rs"));
#[cfg(test)]
mod inbound_gap_tests;
