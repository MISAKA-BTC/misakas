//! Retained PerPosition checkpoint-leg consumer. No served-chunk,
//! replay, authentication or refutation envelope is implied by this owner.
use crate::{
    gdn_evidence_v2::Base0GdnMaterialV2,
    legs::{Base0CheckpointCaptureV1, Base0CheckpointsV1},
};
use kaspa_consensus_core::{
    palw_context_ladder::{PalwCheckpointCadenceV1, palw_checkpoint_cadence_v1},
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, NativeHeapV1, reserve_native_requests_v1},
    palw_step_leg::{PALW_STEP_LEG_MAX_LEAVES, PalwCheckpointLeafV2},
};
use kaspa_hashes::Hash64;
use std::{mem::size_of, ops::Deref};

type Error = NativeDecodeErrorV1<String>;
const PATH: &str = "GDN PerPosition checkpoint-leg construction";
fn overflow<T>() -> Error {
    Error::Resource(NativeCoverageV1::overflow::<T>(PATH))
}
fn unsupported(why: &str) -> Error {
    Error::Resource(NativeCoverageV1::unsupported::<Base0GdnLeafLegV3>(PATH, why))
}
fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or_else(overflow::<Base0GdnLeafLegV3>)
}
fn slots<T>(n: u64) -> Result<u64, Error> {
    n.checked_mul(size_of::<T>() as u64).ok_or_else(overflow::<T>)
}

/// The actual push-built leaf/hash arrays use the pinned Rust Vec minimum
/// capacity and doubling; count both old/new buffers at a growth. Stack object
/// size and physical allocator metadata are not called heap requests here.
fn push_peak<T>(n: u64) -> Result<u64, Error> {
    if n == 0 {
        return Ok(0);
    }
    let minimum = if size_of::<T>() == 1 {
        8
    } else if size_of::<T>() <= 1024 {
        4
    } else {
        1
    };
    let capacity = n.max(minimum).checked_next_power_of_two().ok_or_else(overflow::<T>)?;
    slots::<T>(if capacity > minimum { add(capacity, capacity / 2)? } else { capacity })
}

/// Simulate a particular Writer's actual extend_from_slice lengths, including
/// its explicit initial allocation. Bulk network-id extension is not modeled
/// as byte-at-a-time push: reserve grows to max(old*2, required, minimum).
struct WriterRequest {
    len: u64,
    capacity: u64,
    peak: u64,
}
impl WriterRequest {
    fn new(capacity: u64) -> Self {
        Self { len: 0, capacity, peak: capacity }
    }
    fn extend(&mut self, n: u64) -> Result<(), Error> {
        self.len = add(self.len, n)?;
        if self.len > self.capacity {
            let next = self.capacity.checked_mul(2).ok_or_else(overflow::<u8>)?.max(self.len).max(8);
            self.peak = self.peak.max(add(self.capacity, next)?);
            self.capacity = next;
        }
        Ok(())
    }
}

/// Quote the existing constructor's CanonicalWriter/Writer buffers from their
/// frozen fields. Context hashing starts with 512 bytes; checkpoint-profile,
/// checkpoint-leaf and Merkle-leaf Writers start with 256. These are actual
/// source allocations, not guessed prefix or encoded-input multipliers.
fn hash_writer_peak(material: &Base0GdnMaterialV2<'_>) -> Result<u64, Error> {
    let ctx = &material.binding.job_context;
    let hash_bytes = ctx.job_id.as_byte_slice().len() as u64;
    let mut context = WriterRequest::new(512);
    context.extend(size_of::<u16>() as u64)?;
    context.extend(size_of::<u32>() as u64)?;
    context.extend(ctx.network_id.len() as u64)?;
    // Three identity hashes, the fixed seed, and the remaining eight hashes,
    // followed by the three declared token/context counters.
    for _ in 0..3 {
        context.extend(hash_bytes)?;
    }
    context.extend(ctx.execution_seed.len() as u64)?;
    for _ in 0..8 {
        context.extend(hash_bytes)?;
    }
    for _ in 0..3 {
        context.extend(size_of::<u32>() as u64)?;
    }
    let mut leaf = WriterRequest::new(256);
    leaf.extend(size_of::<u16>() as u64)?;
    for _ in 0..3 {
        leaf.extend(hash_bytes)?;
    }
    for _ in 0..2 {
        leaf.extend(size_of::<u32>() as u64)?;
    }
    leaf.extend(hash_bytes)?;
    leaf.extend(size_of::<u32>() as u64)?;
    leaf.extend(hash_bytes)?;
    Ok(context.peak.max(leaf.peak).max(256))
}

/// Persistent arrays are immutable and remain admitted through every caller's
/// borrow. No raw extraction, Clone or DerefMut can release this owner while
/// handing out an independently usable leg.
pub struct Base0GdnLeafLegV3 {
    leg: Base0CheckpointsV1,
    // Field order releases the arrays before their retained request.
    _lease: Option<NativeDemandLeaseV1>,
}
impl Deref for Base0GdnLeafLegV3 {
    type Target = Base0CheckpointsV1;
    fn deref(&self) -> &Self::Target {
        &self.leg
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LeafLegRequestV3 {
    pub(crate) persistent_bytes: u64,
    pub(crate) workspace_bytes: u64,
}
impl LeafLegRequestV3 {
    pub(crate) fn of(material: &Base0GdnMaterialV2<'_>) -> Result<Self, Error> {
        if palw_checkpoint_cadence_v1(&material.binding.shape_profile) != PalwCheckpointCadenceV1::PerPosition {
            return Err(unsupported("retained leaf-leg API requires the actual PerPosition cadence"));
        }
        if !material.served_chunks.is_empty() {
            return Err(unsupported("PerPosition leaf-leg input unexpectedly carries served chunks"));
        }
        let n = u64::try_from(material.checkpoint_leaves.len()).map_err(|_| overflow::<PalwCheckpointLeafV2>())?;
        if n > u64::from(u32::MAX) || n > PALW_STEP_LEG_MAX_LEAVES {
            return Err(Error::Decode("checkpoint leaf count exceeds the existing leg representation/cap".into()));
        }
        let persistent_bytes = add(push_peak::<PalwCheckpointLeafV2>(n)?, push_peak::<Hash64>(n)?)?;
        // Base0CheckpointCaptureV1::new clones these borrowed values; neither
        // clone is needed after finish. Native visitors include nested graph
        // rows/refs and network bytes, not just the struct's inline size.
        let clones = material
            .binding
            .job_context
            .native_heap_v1(PATH)
            .add::<Base0GdnLeafLegV3>(material.binding.shape_profile.native_heap_v1(PATH), PATH);
        let clones = clones.bytes().ok_or(Error::Resource(clones))?;
        // step_merkle_root_v1 first collects N transformed hashes, then holds
        // both that level and ceil(N/2) while building the next. Smaller levels
        // cannot exceed this first pair. An empty leg hashes without a Vec.
        let merkle = if n == 0 { 0 } else { slots::<Hash64>(add(n, n.div_ceil(2))?)? };
        let workspace_bytes = add(clones, add(merkle, hash_writer_peak(material)?)?)?;
        Ok(Self { persistent_bytes, workspace_bytes })
    }
}

/// Reuse the existing native leaf-chain/cadence/hash checks after admitting all
/// its named copies and scratch. Other cadences refuse; they never fall back to
/// the old raw leg helper under this retained API.
pub fn base0_gdn_leaf_leg_retained_v3(material: &Base0GdnMaterialV2<'_>) -> Result<Base0GdnLeafLegV3, Error> {
    let request = LeafLegRequestV3::of(material)?;
    let owner = reserve_native_requests_v1(request.persistent_bytes).map_err(Error::Resource)?;
    let _workspace = reserve_native_requests_v1(request.workspace_bytes).map_err(Error::Resource)?;
    let binding = material.binding;
    let leg = Base0CheckpointCaptureV1::from_leaves_v1(
        &binding.job_context,
        &binding.shape_profile,
        &binding.checkpoint_profile,
        material.checkpoint_leaves,
    )
    .map_err(|why| Error::Decode(format!("the checkpoint leg does not rebuild: {why:?}")))?;
    if leg.merkle_root != binding.checkpoint_merkle_root || leg.leaf_hashes.len() as u64 != u64::from(binding.checkpoint_count) {
        return Err(Error::Decode("the rebuilt checkpoint leg is not the one the claim's binding commits".into()));
    }
    // Confirm the retained result stayed inside the typed persistent quote;
    // constructor scratch/ctx/profile clones have been retired by finish.
    let actual =
        add(slots::<PalwCheckpointLeafV2>(leg.leaves.capacity() as u64)?, slots::<Hash64>(leg.leaf_hashes.capacity() as u64)?)?;
    if actual > request.persistent_bytes || !leg.chunks.is_empty() {
        return Err(unsupported("native leaf-leg result exceeds its admitted typed arrays"));
    }
    Ok(Base0GdnLeafLegV3 { leg, _lease: owner })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_hash_writer_growth_matches_actual_extend_reservations() {
        // Real Vec extend capacity controls: a bulk network-id append can grow
        // to a non-power-of-two capacity, unlike push-built leaf/hash arrays.
        for network in [0usize, 512, 513, 3000, 10_000] {
            let lengths = [2, 4, network, 64, 64, 64, 32, 64, 64, 64, 64, 64, 64, 64, 64, 4, 4, 4];
            let mut quote = WriterRequest::new(512);
            let mut actual = Vec::<u8>::with_capacity(512);
            let mut peak = actual.capacity();
            for n in lengths {
                let old = actual.capacity();
                let bytes = vec![0; n];
                actual.extend_from_slice(&bytes);
                let next = actual.capacity();
                peak = peak.max(if next != old { old + next } else { next });
                quote.extend(n as u64).unwrap();
                assert_eq!(quote.capacity as usize, actual.capacity());
                assert_eq!(quote.peak as usize, peak);
            }
        }
    }
}
