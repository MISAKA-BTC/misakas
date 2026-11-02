//! Retained-byte admission for protobuf queues and their encoded HTTP frames.
//!
//! Queue admission is immediate: no caller can sleep while holding consensus locks.
//! This accounts owned protobuf capacities, not serialized length. Producers of large
//! replies must additionally reserve before building them (see the PALW snapshot lane).
use kaspa_core::memory_budget::{MemoryBudget, MemoryClass, MemoryLease, global_memory_budget};
use prost::Message;
use std::{fmt::Debug, marker::PhantomData, sync::Arc};
use tonic::Status;
use tonic::codec::{Codec, EncodeBuf, Encoder, RetainedResources};

/// Heap bytes owned by a protobuf value, excluding the value's inline storage.
/// Implementations for generated messages are emitted by `build_transport.rs`.
pub trait RetainedSize {
    fn retained_heap_bytes(&self) -> u64;
}
macro_rules! scalars {
    ($($t:ty),*) => { $(impl RetainedSize for $t { fn retained_heap_bytes(&self) -> u64 { 0 } })* };
}
scalars!(bool, u8, u32, u64, i32, i64, f32, f64);
impl RetainedSize for String {
    fn retained_heap_bytes(&self) -> u64 {
        self.capacity() as u64
    }
}
impl<T: RetainedSize> RetainedSize for Vec<T> {
    fn retained_heap_bytes(&self) -> u64 {
        self.iter().fold((self.capacity() as u64).saturating_mul(std::mem::size_of::<T>() as u64), |n, item| {
            n.saturating_add(item.retained_heap_bytes())
        })
    }
}
impl<T: RetainedSize> RetainedSize for Option<T> {
    fn retained_heap_bytes(&self) -> u64 {
        self.as_ref().map_or(0, RetainedSize::retained_heap_bytes)
    }
}
impl<T: RetainedSize> RetainedSize for Box<T> {
    fn retained_heap_bytes(&self) -> u64 {
        (std::mem::size_of::<T>() as u64).saturating_add((**self).retained_heap_bytes())
    }
}

/// Each connection has its own ceiling inside the process Transport ceiling.
/// 1.5 GiB admits a 320 MiB message with simultaneous codec allocations.
pub fn connection_budget() -> Arc<MemoryBudget> {
    MemoryBudget::new(1536 << 20, [1536 << 20; 4])
}

/// A queue item is intentionally not Clone: a deep protobuf copy needs new tickets.
#[derive(Debug)]
pub struct Outbound<T> {
    message: T,
    resources: RetainedResources,
    connection: Arc<MemoryBudget>,
}
impl<T: Message + RetainedSize> Outbound<T> {
    pub fn try_new(message: T, connection: &Arc<MemoryBudget>) -> Result<Self, Status> {
        Self::with_resources(message, connection, Vec::new())
    }
    pub fn with_resources(message: T, connection: &Arc<MemoryBudget>, mut resources: RetainedResources) -> Result<Self, Status> {
        Self::with_admission(message, connection, None, &mut resources)
    }
    /// Move an existing generation ticket without charging the same raw payload
    /// twice. Its purpose class stays Snapshot until its final transport owner drops.
    pub fn with_preallocated(
        message: T,
        connection: &Arc<MemoryBudget>,
        lease: MemoryLease,
        mut resources: RetainedResources,
    ) -> Result<Self, Status> {
        Self::with_admission(message, connection, Some(lease), &mut resources)
    }
    fn with_admission(
        message: T,
        connection: &Arc<MemoryBudget>,
        preallocated: Option<MemoryLease>,
        resources: &mut RetainedResources,
    ) -> Result<Self, Status> {
        let owned = (std::mem::size_of::<T>() as u64).saturating_add(message.retained_heap_bytes()).saturating_add(128);
        let process = if let Some(mut lease) = preallocated {
            if lease.bytes() < owned {
                return Err(Status::resource_exhausted("preallocated snapshot envelope is smaller than retained payload"));
            }
            // Return unused generation headroom atomically while keeping actual
            // owned capacity continuously charged. No new global admission.
            let actual = lease.split(owned).map_err(|e| Status::resource_exhausted(e.to_string()))?;
            drop(lease);
            actual
        } else {
            global_memory_budget()
                .try_reserve_retained(MemoryClass::Transport, owned)
                .map_err(|e| Status::resource_exhausted(e.to_string()))?
        };
        let local =
            connection.try_reserve_retained(MemoryClass::Transport, owned).map_err(|e| Status::resource_exhausted(e.to_string()))?;
        process.materialized();
        local.materialized();
        resources.push(Box::new(process));
        resources.push(Box::new(local));
        Ok(Self { message, resources: std::mem::take(resources), connection: Arc::clone(connection) })
    }
}

/// Encoder admits its simultaneous scratch/output allocations before prost writes.
/// tonic moves both these and the original queue resources into the actual Bytes
/// owner. Bytes clones/slices made by h2 therefore retain every ticket.
pub struct BudgetedProstCodec<T, U>(PhantomData<fn() -> (T, U)>);
impl<T, U> Default for BudgetedProstCodec<T, U> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
pub struct BudgetedEncoder<T> {
    resources: RetainedResources,
    scratch: Vec<MemoryLease>,
    marker: PhantomData<fn() -> T>,
}
impl<T, U> Codec for BudgetedProstCodec<T, U>
where
    T: Message + Send + 'static,
    U: Message + Default + Send + 'static,
{
    type Encode = Outbound<T>;
    type Decode = U;
    type Encoder = BudgetedEncoder<T>;
    type Decoder = <tonic_prost::ProstCodec<T, U> as Codec>::Decoder;
    fn encoder(&mut self) -> Self::Encoder {
        BudgetedEncoder { resources: Vec::new(), scratch: Vec::new(), marker: PhantomData }
    }
    fn decoder(&mut self) -> Self::Decoder {
        tonic_prost::ProstCodec::<T, U>::default().decoder()
    }
}
impl<T: Message> Encoder for BudgetedEncoder<T> {
    type Item = Outbound<T>;
    type Error = Status;
    fn encoded_len(&self, item: &Self::Item) -> Option<usize> {
        Some(item.message.encoded_len())
    }
    fn prepare_encode(&mut self, item: &Self::Item) -> Result<(), Status> {
        // The fresh large buffers in tonic avoid geometric capacity growth. Three
        // times the wire length covers plain scratch, gzip output (including its
        // framing expansion) and the optional compact-frame copy simultaneously.
        let bytes = (item.message.encoded_len() as u64).saturating_add(64 << 10).saturating_mul(3);
        let lease: MemoryLease = global_memory_budget()
            .try_reserve(MemoryClass::Transport, bytes)
            .map_err(|e| Status::resource_exhausted(e.to_string()))?;
        let local =
            item.connection.try_reserve(MemoryClass::Transport, bytes).map_err(|e| Status::resource_exhausted(e.to_string()))?;
        self.scratch.push(lease);
        self.scratch.push(local);
        Ok(())
    }
    fn encode(&mut self, mut item: Self::Item, dst: &mut EncodeBuf<'_>) -> Result<(), Status> {
        item.message.encode(dst).map_err(|e| Status::internal(e.to_string()))?;
        // No further allocations use this envelope after encoding/compression.
        // Keep its conservative bytes charged until the HTTP owner's last drop.
        self.resources.append(&mut item.resources);
        Ok(())
    }
    fn take_resources(&mut self) -> RetainedResources {
        for lease in self.scratch.drain(..) {
            lease.materialized();
            self.resources.push(Box::new(lease));
        }
        std::mem::take(&mut self.resources)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_unused_capacity_and_nested_empty_values() {
        let mut values: Vec<Vec<u8>> = Vec::with_capacity(10);
        values.push(Vec::with_capacity(4096));
        assert_eq!(values.retained_heap_bytes(), 10 * std::mem::size_of::<Vec<u8>>() as u64 + 4096);
        values.clear();
        assert_eq!(values.retained_heap_bytes(), 10 * std::mem::size_of::<Vec<u8>>() as u64);
    }
}
