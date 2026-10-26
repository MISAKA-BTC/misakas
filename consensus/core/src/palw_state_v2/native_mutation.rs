//! Owner-local before-allocation quotes for the claim/liability/vesting writer
//! chain. The caller reserves these requests before any dependent writer runs.
use super::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, TirNativeRequestsV1, typed::BTreeNodeEnvelopeV1};

fn copy<T: NativeHeapV1 + ?Sized>(q: &mut TirNativeRequestsV1, value: &T, purpose: &str) -> Result<(), PalwStateV2Error> {
    let heap = value.native_heap_v1(purpose);
    q.add_v1(heap.bytes().ok_or(PalwStateV2Error::NativeResource(heap))?).map_err(PalwStateV2Error::NativeResource)
}
fn tree<K, V>(q: &mut TirNativeRequestsV1, n: u64) -> Result<(), PalwStateV2Error> {
    q.exact_v1::<BTreeNodeEnvelopeV1<K, V>>(n).map_err(PalwStateV2Error::NativeResource)
}
fn encoded<T: borsh::BorshSerialize>(q: &mut TirNativeRequestsV1, value: &T) -> Result<(), PalwStateV2Error> {
    let n = borsh::object_length(value).map_err(|e| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<T>("claim cascade canonical buffer", &e.to_string()))
    })? as u64;
    q.vec_v1::<u8>(n.max(1024)).map_err(PalwStateV2Error::NativeResource)
}
impl<'a> TransitionBuilder<'a> {
    /// Reserve a whole dependent-writer sequence before its first mutation.
    /// Existing and replacement journal frames coexist through realloc.
    pub(super) fn reserve_mutation_sequence_v1(&mut self, bytes: u64, slots: usize) -> Result<(), PalwStateV2Error> {
        if !self.native_reservation.is_tracking_v1() {
            return Ok(());
        }
        if cfg!(not(target_pointer_width = "64")) {
            return Err(PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PalwDeltaEntryV2>(
                "native mutation sequence",
                "pinned tree profile requires 64-bit",
            )));
        }
        let needed = self.entries.len().checked_add(slots).ok_or_else(|| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("native mutation journal slots"))
        })?;
        let mut capacity = self.entries.capacity();
        if needed > capacity {
            capacity = capacity.max(2);
            while capacity < needed {
                capacity = capacity.checked_mul(2).ok_or_else(|| {
                    PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("native mutation journal growth"))
                })?;
            }
        }
        let journal = if capacity > self.entries.capacity() {
            (capacity as u64).checked_mul(std::mem::size_of::<PalwDeltaEntryV2>() as u64).ok_or_else(|| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("native mutation journal bytes"))
            })?
        } else {
            0
        };
        self.native_reservation
            .grow_v1(bytes.checked_add(journal).ok_or_else(|| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("native mutation coexisting requests"))
            })?)
            .map_err(PalwStateV2Error::NativeResource)?;
        if capacity > self.entries.capacity() {
            self.entries.try_reserve_exact(capacity - self.entries.len()).map_err(|_| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PalwDeltaEntryV2>(
                    "native mutation journal",
                    "allocator refused pre-admitted journal",
                ))
            })?;
        }
        Ok(())
    }
    /// The returned count includes journal slots for all existing dependent
    /// rows this one claim write can delete, including every actual forfeit.
    pub(super) fn claim_cascade_requests_v1(
        &self,
        key: Hash64,
        new: &Option<PalwClaimStateV2>,
    ) -> Result<(u64, usize), PalwStateV2Error> {
        let mut q = TirNativeRequestsV1::new();
        let mut slots = 1usize;
        if cfg!(not(target_pointer_width = "64")) {
            return Err(PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PalwClaimStateV2>(
                "claim writer indexes",
                "pinned tree profile requires 64-bit",
            )));
        }
        if let Some(row) = new {
            if !row.phase.is_terminal() {
                tree::<(u64, Hash64), ()>(&mut q, 1)?;
            }
            if row.work_id.is_some() {
                tree::<Hash64, Hash64>(&mut q, 1)?;
            }
            if self.params.capacity_weight_cap_from_daa().is_some() {
                tree::<PalwBondKeyV2, crate::palw_weight_cap_v1::PalwBondWeightV1>(&mut q, 1)?;
            }
            if self.inflight_index.borrow().is_some() {
                tree::<Hash64, PalwInflightTallyV1>(&mut q, 1)?;
                tree::<PalwBondKeyV2, u32>(&mut q, 1)?;
            }
            if self.extras.class_semantics_active
                && !self.state.claims.contains_key(&key)
                && let Some(identity) = self.state.class_semantics.get(&row.class_id)
            {
                // Stamp's caller clone occurs before its map writer.
                copy(&mut q, identity, "new claim's immutable identity input clone")?;
                slots = slots.checked_add(1).ok_or_else(|| {
                    PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("claim cascade journal slots"))
                })?;
            }
        }
        // Removal writers transfer their actual old payload into the journal.
        // Their possible canonical encodings are charged separately below.
        macro_rules! deletion {
            ($field:ident) => {
                if self.state.$field.contains_key(&key) {
                    slots = slots.checked_add(1).ok_or_else(|| {
                        PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("claim cascade journal slots"))
                    })?;
                }
            };
        }
        deletion!(claim_semantics);
        deletion!(fp_public_jobs);
        deletion!(exec_v2_origins);
        deletion!(claim_roots);
        deletion!(shard_licensing);
        deletion!(held_da_missing);
        deletion!(held_leaf_demands);
        deletion!(panel_duties);
        deletion!(tir_shard_claims);
        if let Some(row) = self.state.exec_v2_origins.get(&key) {
            encoded(&mut q, row)?;
            encoded(&mut q, &key)?;
        }
        let forfeits = self.state.held_forfeits_of_claim(&key).count();
        slots = slots.checked_add(forfeits).ok_or_else(|| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("claim forfeit journal slots"))
        })?;
        q.vec_v1::<Hash64>(forfeits as u64).map_err(PalwStateV2Error::NativeResource)?;
        // Vertex removal journals encode the old record and key into concrete
        // Vec<u8>s. They do not deserialize or trust lengths inside those Vecs.
        if let Some(row) = self.state.vertex.tallies.get(&key) {
            slots = slots.checked_add(1).ok_or_else(|| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("claim tally journal"))
            })?;
            encoded(&mut q, row)?;
            encoded(&mut q, &key)?;
        }
        if let Some(row) = self.state.vertex.held.get(&key) {
            slots = slots.checked_add(1).ok_or_else(|| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("claim held journal"))
            })?;
            encoded(&mut q, row)?;
            encoded(&mut q, &key)?;
        }
        if let Some(row) = self.state.vertex.mesh.capped.get(&key) {
            slots = slots.checked_add(1).ok_or_else(|| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("claim capped journal"))
            })?;
            encoded(&mut q, row)?;
            encoded(&mut q, &key)?;
        }
        // Leaving DefaultDisputed may update one existing exposure row. Its
        // journal slot is distinct from the claim's slot even when no tree
        // membership is created.
        if self.state.claims.get(&key).is_some_and(|row| matches!(row.phase, PalwClaimPhaseV2::DefaultDisputed { .. })) {
            slots = slots.checked_add(1).ok_or_else(|| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("claim exposure journal"))
            })?;
        }
        Ok((q.bytes_v1(), slots))
    }

    pub(super) fn panel_liability_copy_requests_v1(&self, key: &Hash64) -> Result<u64, PalwStateV2Error> {
        let mut q = TirNativeRequestsV1::new();
        if let Some(old) = self.state.panel_liabilities.get(key) {
            copy(&mut q, old, "panel liability previous-row clone")?;
        }
        Ok(q.bytes_v1())
    }
    pub(super) fn vesting_index_requests_v1(new: &Option<crate::palw_vesting_v1::PalwVestingRowV1>) -> Result<u64, PalwStateV2Error> {
        let mut q = TirNativeRequestsV1::new();
        if let Some(row) = new {
            tree::<(u64, Hash64), ()>(&mut q, 1)?; // expiry
            if row.matured_at.is_none() {
                tree::<(u64, Hash64), ()>(&mut q, 1)?;
                tree::<(u64, Hash64), ()>(&mut q, 1)?;
            }
            let payees = (row.seats.len() as u64).checked_add(1).ok_or_else(|| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("vesting payee count"))
            })?;
            tree::<(PalwBondKeyV2, bool, u64, Hash64), ()>(&mut q, payees)?;
        }
        Ok(q.bytes_v1())
    }
}
