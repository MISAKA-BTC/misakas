mod body_validation_in_context;
mod body_validation_in_isolation;
mod processor;
pub use processor::*;

// Bridge audit BR-1 (2026-09-25): a peer-chosen EVM payload never condemns a block id.
#[cfg(test)]
mod br1_payload_delivery_tests;
