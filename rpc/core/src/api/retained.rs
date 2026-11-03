//! Internal, non-wire ownership at reviewed RPC construction boundaries.
use kaspa_core::memory_budget::MemoryLease;
#[path = "retained/utxo_memory.rs"]
pub mod utxo_memory;

/// Rust response and its not-yet-built transport representation have separate
/// leases because both allocations coexist during conversion. Do not derive
/// Clone: an independent response clone must reserve its own allocation first.
#[derive(Debug)]
pub struct RetainedRpcReply<T> {
    pub response: T,
    pub retained: Option<MemoryLease>,
    pub conversion: Option<MemoryLease>,
}
impl<T> RetainedRpcReply<T> {
    pub fn untracked(response: T) -> Self {
        Self { response, retained: None, conversion: None }
    }
    pub fn into_parts(self) -> (T, Option<MemoryLease>, Option<MemoryLease>) {
        (self.response, self.retained, self.conversion)
    }
    /// Legacy adapters use the existing wire response. This does not retain a
    /// transport lease; migrate their serializer before claiming full coverage.
    pub fn into_response(self) -> T {
        self.response
    }
}
