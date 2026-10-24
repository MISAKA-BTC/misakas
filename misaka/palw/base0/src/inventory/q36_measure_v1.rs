//! Measurement sinks with scoped stream scratch and retained tensor metadata.
use super::{InventoryBuildError, Qwen36RowSinkV1, qwen36_visit_inventory_rows_v1};
use crate::qwen36::Qwen36ArtifactV1;
use kaspa_consensus_core::{
    palw_artifact::{PalwArtifactInventoryStreamV1, PalwArtifactInventorySummaryV1},
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1},
    palw_shard_plan_v1::PalwInventoryRowMetaV1,
    palw_step::PalwShapeProfileV3,
};
use kaspa_hashes::Hash64;
use std::{mem::size_of, ops::Deref};
fn overflow() -> NativeCoverageV1 {
    NativeCoverageV1::overflow::<Qwen36InventoryMetadataV1>("Q36 measurement request")
}
fn quote<T>(count: usize) -> Result<u64, NativeCoverageV1> {
    count.checked_mul(size_of::<T>()).and_then(|n| n.checked_mul(2)).and_then(|n| u64::try_from(n).ok()).ok_or_else(overflow)
}
fn allocation(e: std::collections::TryReserveError) -> NativeCoverageV1 {
    NativeCoverageV1::unsupported::<Qwen36InventoryMetadataV1>("Q36 measurement allocation", &e.to_string())
}
struct Stream {
    stream: PalwArtifactInventoryStreamV1,
    name: String,
    count: u32,
    bytes: u64,
    name_ticket: Option<NativeDemandLeaseV1>,
    _base_ticket: Option<NativeDemandLeaseV1>,
}
impl Stream {
    fn new() -> Result<Self, NativeCoverageV1> {
        // u32 leaf count admits at most 33 frontier levels. Cover a Vec's
        // 32+64 old/new slot growth as typed scratch before the first push.
        let request = quote::<Self>(1)?.checked_add(quote::<Option<Hash64>>(96)?).ok_or_else(overflow)?;
        let ticket = reserve_native_requests_v1(request)?;
        Ok(Self {
            stream: PalwArtifactInventoryStreamV1::new(),
            name: String::new(),
            count: 0,
            bytes: 0,
            name_ticket: None,
            _base_ticket: ticket,
        })
    }
    fn push(&mut self, name: &str, layer: Option<u16>, start: u32, bytes: &[u8]) -> Result<(), InventoryBuildError> {
        u32::try_from(bytes.len()).map_err(|_| overflow())?;
        let count = self.count.checked_add(1).ok_or_else(overflow)?;
        let total = self.bytes.checked_add(u64::try_from(bytes.len()).map_err(|_| overflow())?).ok_or_else(overflow)?;
        if self.name != name {
            // One checker-owned name plus the comparison name. Retain the
            // old names' ticket through both new allocations and core push.
            let request = quote::<u8>(name.len())?.checked_mul(2).ok_or_else(overflow)?;
            let ticket = reserve_native_requests_v1(request)?;
            let mut next = String::new();
            next.try_reserve_exact(name.len()).map_err(allocation)?;
            next.push_str(name);
            self.stream.push(name, layer, start, bytes).map_err(InventoryBuildError::NotCanonical)?;
            self.name = next;
            self.name_ticket = ticket;
        } else {
            self.stream.push(name, layer, start, bytes).map_err(InventoryBuildError::NotCanonical)?;
        }
        self.count = count;
        self.bytes = total;
        Ok(())
    }
    fn finish(self) -> Result<PalwArtifactInventorySummaryV1, InventoryBuildError> {
        self.stream.finish().map_err(InventoryBuildError::NotCanonical)
    }
}

/// Tensor metadata with its construction reservation, borrowed by placement.
/// No Clone or Vec extraction can detach retained names from their ticket.
pub struct Qwen36InventoryMetadataV1 {
    tensors: Vec<PalwInventoryRowMetaV1>,
    ticket: Option<NativeDemandLeaseV1>,
    memory: u64,
    slots: u64,
}
impl Deref for Qwen36InventoryMetadataV1 {
    type Target = [PalwInventoryRowMetaV1];
    fn deref(&self) -> &Self::Target {
        &self.tensors
    }
}
impl Qwen36InventoryMetadataV1 {
    fn new() -> Result<Self, NativeCoverageV1> {
        let memory = quote::<Self>(1)?;
        Ok(Self { tensors: Vec::new(), ticket: reserve_native_requests_v1(memory)?, memory, slots: 0 })
    }
    fn grow(&mut self, additional: u64) -> Result<(), NativeCoverageV1> {
        let memory = self.memory.checked_add(additional).ok_or_else(overflow)?;
        if let Some(ticket) = &mut self.ticket {
            ticket.grow_memory_v1(additional)?;
        }
        self.memory = memory;
        Ok(())
    }
    fn release(&mut self, bytes: u64) {
        if let Some(ticket) = &mut self.ticket {
            drop(ticket.split_memory_v1(bytes).expect("metadata owns released demand"));
        }
        self.memory = self.memory.checked_sub(bytes).expect("metadata owns released bytes");
    }
    fn push(&mut self, name: &str, layer: Option<u16>, bytes: usize) -> Result<(), NativeCoverageV1> {
        let bytes = u64::try_from(bytes).map_err(|_| overflow())?;
        if let Some(last) = self.tensors.last_mut() {
            if last.tensor_name == name && last.layer == layer {
                last.bytes = last.bytes.checked_add(bytes).ok_or_else(overflow)?;
                return Ok(());
            }
        }
        if self.tensors.len() == self.tensors.capacity() {
            let capacity = self.tensors.capacity().max(1).checked_mul(2).ok_or_else(overflow)?;
            let request = quote::<PalwInventoryRowMetaV1>(capacity)?;
            self.grow(request)?;
            if let Err(e) = self.tensors.try_reserve_exact(capacity - self.tensors.len()) {
                self.release(request);
                return Err(allocation(e));
            }
            self.release(self.slots);
            self.slots = request;
        }
        let request = quote::<u8>(name.len())?;
        self.grow(request)?;
        let mut tensor_name = String::new();
        if let Err(e) = tensor_name.try_reserve_exact(name.len()) {
            self.release(request);
            return Err(allocation(e));
        }
        tensor_name.push_str(name);
        self.tensors.push(PalwInventoryRowMetaV1 { tensor_name, layer, bytes });
        Ok(())
    }
}

/// Summary sink only: it retains no per-tensor metadata. Plan/input ownership is
/// still held by the shared emitter and is separate from this sink's scratch.
pub fn summary(
    artifact: &Qwen36ArtifactV1,
    profile: &PalwShapeProfileV3,
) -> Result<PalwArtifactInventorySummaryV1, InventoryBuildError> {
    let mut stream = Stream::new()?;
    let sink: &mut Qwen36RowSinkV1<'_> = &mut |name, layer, start, bytes| stream.push(name, layer, start, bytes);
    qwen36_visit_inventory_rows_v1(artifact, profile, sink)?;
    stream.finish()
}
/// Summary plus one retained, admitted metadata entry per tensor/layer.
pub fn measure(
    artifact: &Qwen36ArtifactV1,
    profile: &PalwShapeProfileV3,
) -> Result<(PalwArtifactInventorySummaryV1, Qwen36InventoryMetadataV1), InventoryBuildError> {
    let mut stream = Stream::new()?;
    let mut tensors = Qwen36InventoryMetadataV1::new()?;
    qwen36_visit_inventory_rows_v1(artifact, profile, &mut |name, layer, start, bytes| {
        stream.push(name, layer, start, bytes)?;
        tensors.push(name, layer, bytes.len())?;
        Ok(())
    })?;
    Ok((stream.finish()?, tensors))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streamed_summary_matches_core_and_checks_count_overflow() {
        for count in [1, 2, 3, 31, 32, 33, 1023, 1024, 1025] {
            let mut ours = Stream::new().unwrap();
            let mut core = PalwArtifactInventoryStreamV1::new();
            for i in 0..count {
                let bytes = (i as u32).to_le_bytes();
                ours.push("tensor", None, i * 4, &bytes).unwrap();
                core.push("tensor", None, i * 4, &bytes).unwrap();
            }
            assert_eq!(ours.finish().unwrap(), core.finish().unwrap());
        }
        let mut stream = Stream::new().unwrap();
        stream.count = u32::MAX;
        assert!(matches!(stream.push("a", None, 0, &[1]), Err(InventoryBuildError::Resource(_))));
        assert_eq!(stream.count, u32::MAX);
        let mut stream = Stream::new().unwrap();
        stream.bytes = u64::MAX;
        assert!(matches!(stream.push("a", None, 0, &[1]), Err(InventoryBuildError::Resource(_))));
    }
    #[test]
    fn capacity_child() {
        use kaspa_consensus_core::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
        let Ok(case) = std::env::var("MISAKA_MEASUREMENT_CASE") else {
            return;
        };
        let artifact = crate::qwen36::qwen36_dev_fixture_heads(4, 8, 4, 4, 8, 32);
        let profile =
            kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v8(crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4))
                .unwrap();
        let expected = super::super::qwen36_inventory_v1(&artifact, &profile).unwrap().summary();
        let base = quote::<Qwen36InventoryMetadataV1>(1).unwrap();
        let stream_base = quote::<Stream>(1).unwrap() + quote::<Option<Hash64>>(96).unwrap();
        let slots2 = quote::<PalwInventoryRowMetaV1>(2).unwrap();
        let slots4 = quote::<PalwInventoryRowMetaV1>(4).unwrap();
        let name = quote::<u8>(1).unwrap();
        let memory = match case.as_str() {
            "zero" => 0,
            "slots-short" => base + slots2 + 2 * name + slots4 - 1,
            "slots-exact" => base + slots2 + 2 * name + slots4,
            "name-short" => base + slots2 + name - 1,
            "stream-short" => stream_base + 2 * name + quote::<u8>(2).unwrap() * 2 - 1,
            "stream-exact" => stream_base + 2 * name + quote::<u8>(2).unwrap() * 2,
            _ => 1 << 24,
        };
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
        if case == "zero" {
            assert!(Stream::new().is_err());
            assert!(Qwen36InventoryMetadataV1::new().is_err());
        } else if case.starts_with("stream") {
            let mut stream = Stream::new().unwrap();
            stream.push("a", None, 0, &[1]).unwrap();
            let result = stream.push("zz", None, 0, &[2]);
            if case.ends_with("short") {
                assert!(matches!(result, Err(InventoryBuildError::Resource(_))));
                assert_eq!(stream.count, 1);
                stream.push("b", None, 0, &[2]).unwrap();
            } else {
                result.unwrap();
            }
            assert_eq!(stream.finish().unwrap().leaf_count, 2);
        } else if case.starts_with("slots") || case == "name-short" {
            let mut owner = Qwen36InventoryMetadataV1::new().unwrap();
            if case == "name-short" {
                assert!(owner.push("a", None, 1).is_err());
                assert!(owner.is_empty());
            } else {
                owner.push("a", None, 1).unwrap();
                owner.push("a", None, 2).unwrap();
                assert_eq!(owner.len(), 1);
                assert_eq!(owner[0].bytes, 3);
                owner.push("a", Some(1), 1).unwrap();
                let result = owner.push("b", None, 1);
                if case.ends_with("short") {
                    assert!(result.is_err());
                    assert_eq!(owner.len(), 2);
                } else {
                    result.unwrap();
                    assert_eq!(owner.len(), 3);
                }
                assert!(reserve_native_requests_v1(memory - owner.memory + 1).is_err());
                assert!(reserve_native_requests_v1(memory - owner.memory).is_ok());
            }
            drop(owner);
        } else if case == "measure" {
            let mut low = 0;
            let mut high = memory;
            while low < high {
                let limit = low + (high - low) / 2;
                let blocker = reserve_native_requests_v1(memory - limit).unwrap();
                match measure(&artifact, &profile) {
                    Ok((_, owner)) => {
                        drop(owner);
                        high = limit;
                    }
                    Err(InventoryBuildError::Resource(_)) => low = limit + 1,
                    Err(other) => panic!("unexpected measure refusal {other:?}"),
                }
                drop(blocker);
                assert!(reserve_native_requests_v1(memory).is_ok());
            }
            assert!(low > base && low < memory);
            let blocker = reserve_native_requests_v1(memory - low + 1).unwrap();
            assert!(matches!(measure(&artifact, &profile), Err(InventoryBuildError::Resource(_))));
            drop(blocker);
            let blocker = reserve_native_requests_v1(memory - low).unwrap();
            let (got, owner) = measure(&artifact, &profile).unwrap();
            assert_eq!(got, expected);
            assert_eq!(owner.iter().map(|t| t.bytes).sum::<u64>(), expected.artifact_bytes);
            assert!(reserve_native_requests_v1(low - owner.memory + 1).is_err());
            assert!(reserve_native_requests_v1(low - owner.memory).is_ok());
            println!("measurement admission peak {low}; metadata retained {}; tensors {}", owner.memory, owner.len());
            drop(blocker);
            assert_eq!(summary(&artifact, &profile).unwrap(), expected);
            drop(owner);
        } else {
            panic!("unknown case");
        }
        assert!(reserve_native_requests_v1(memory).is_ok(), "scratch and metadata release completely");
    }
    #[test]
    fn stream_metadata_capacity_refusal_and_returned_owner_lifetimes() {
        for case in ["zero", "slots-short", "slots-exact", "name-short", "stream-short", "stream-exact", "measure"] {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "inventory::q36_measure_v1::tests::capacity_child", "--nocapture"])
                .env("MISAKA_MEASUREMENT_CASE", case)
                .output()
                .unwrap();
            assert!(out.status.success(), "{case}: {} {}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"));
            if case == "measure" {
                print!("{}", String::from_utf8_lossy(&out.stdout));
            }
        }
    }
}
