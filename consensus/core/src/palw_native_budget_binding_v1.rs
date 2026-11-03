//! Exact finite provenance for native budget collateral, separate from generic
//! FreePrompt/KernelRoute origins. No carrier can create a binding.
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeHeapV1, NativeRetainedV1};
use crate::{
    Hash64,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateV2Error},
};
use borsh::{BorshDeserialize, BorshSerialize};
pub const NATIVE_BUDGET_BINDING_TABLE_V1: u8 = 197;
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NativeBudgetSourceV1 {
    Mining39,
    Job39,
    MiningSeat39,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeBudgetBindingV1 {
    pub version: u16,
    pub source: NativeBudgetSourceV1,
    pub source_id: Hash64,
    pub budget_id: Hash64,
    pub producer: PalwBondKeyV2,
    pub accepted_daa: u64,
    pub liability_sompi: u128,
    /// The actual signed native-job reserve already counted by its own catalogue.
    /// Mining has no pre-existing principal reservation in that catalogue.
    pub original_reserve_sompi: u128,
    pub closed: bool,
}
crate::palw_native_resource_v1::native_enum!(NativeBudgetSourceV1;Mining39,Job39,MiningSeat39);
crate::palw_native_resource_v1::native_struct!(NativeBudgetBindingV1;version,source,source_id,budget_id,producer,accepted_daa,liability_sompi,original_reserve_sompi,closed);
crate::palw_native_resource_v1::wire::wire_struct!(NativeBudgetBindingV1;u16,NativeBudgetSourceV1,Hash64,Hash64,PalwBondKeyV2,u64,u128,u128,bool);
impl crate::palw_native_resource_v1::NativeWireScanV1 for NativeBudgetSourceV1 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        r.step()?;
        match r.byte()? {
            0..=2 => Ok(Default::default()),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "native budget source kind")),
        }
    }
}
fn bad(s: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.to_string())
}
pub fn native_budget_binding_key_v1(id: Hash64) -> Vec<u8> {
    let mut k = vec![1];
    k.extend_from_slice(id.as_byte_slice());
    k
}
pub fn native_budget_binding_producer_key_v1(bond: &PalwBondKeyV2, id: Hash64) -> Vec<u8> {
    let mut k = vec![2];
    k.extend(borsh::to_vec(bond).expect("fixed bond"));
    k.extend_from_slice(id.as_byte_slice());
    k
}
fn decode(bytes: &[u8]) -> Result<NativeRetainedV1<NativeBudgetBindingV1>, PalwStateV2Error> {
    // A missing local reservation is not a corrupt committed row, and a
    // malformed fixed-schema row is not local capacity. Inspect before Borsh.
    let shape = crate::palw_native_resource_v1::preflight_native_value_v1::<NativeBudgetBindingV1>(bytes, bytes.len() as u64)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::OutOfMemory => {
                PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<NativeBudgetBindingV1>("native budget binding decode"))
            }
            std::io::ErrorKind::Unsupported | std::io::ErrorKind::TimedOut => PalwStateV2Error::NativeResource(
                NativeCoverageV1::unsupported::<NativeBudgetBindingV1>("native budget binding decode", &error.to_string()),
            ),
            _ => bad(error),
        })?;
    let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(shape.logical_requests_v1())
        .map_err(PalwStateV2Error::NativeResource)?;
    let row = borsh::from_slice(bytes).map_err(bad)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(row, lease))
}
impl NativeBudgetBindingV1 {
    pub fn validate_shape_v1(&self) -> Result<(), PalwStateV2Error> {
        let expected = match self.source {
            NativeBudgetSourceV1::MiningSeat39 => Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
                b"misaka-palw/native/mining-seat-budget/v39",
                &(self.source_id.as_bytes(), self.producer),
            )),
            NativeBudgetSourceV1::Mining39 => crate::palw_native_receipt_v39::native_mining_budget_id_v39(&self.source_id.as_bytes()),
            NativeBudgetSourceV1::Job39 => Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
                b"misaka-palw/native/original-owning-job-budget/v39",
                &self.source_id.as_bytes(),
            )),
        };
        if self.version != 1
            || self.source_id == Hash64::default()
            || self.budget_id != expected
            || self.liability_sompi == 0
            || !matches!(self.source, NativeBudgetSourceV1::Job39) && self.original_reserve_sompi != 0
        {
            return Err(bad("native budget binding identity/source"));
        }
        Ok(())
    }
    /// Actual held amount, not its historic price or merely a live period row.
    pub fn actual_held_v1(&self, state: &PalwChainStateV2, now: u64) -> Result<u128, PalwStateV2Error> {
        use crate::palw_bond_budget_v1::*;
        self.validate_shape_v1()?;
        let budget = state.bond_budget().ok_or_else(|| bad("native binding budget absent"))?;
        let mut held = 0;
        if let Some(row) = budget.claim_row(&self.budget_id) {
            let origin = match self.source {
                NativeBudgetSourceV1::Mining39 => PalwBudgetOriginV1::FreePrompt,
                NativeBudgetSourceV1::Job39 | NativeBudgetSourceV1::MiningSeat39 => PalwBudgetOriginV1::KernelRoute,
            };
            if row.bond != self.producer
                || row.origin != origin
                || row.accepted_daa != self.accepted_daa
                || row.liability_sompi != self.liability_sompi
                || row.open == self.closed
            {
                return Err(bad("native binding differs from actual owner/birth/liability/closure"));
            }
            if row.open {
                held = row.liability_sompi;
            }
        } else if !self.closed {
            return Err(bad("live native binding lost its actual budget row"));
        }
        if let Some(row) = budget.claim_row(&palw_bond_hold_shadow_claim_v1(PalwHoldViewV1::Liability, &self.budget_id)) {
            if row.bond != palw_bond_hold_shadow_bond_v1(PalwHoldViewV1::Liability, &self.producer)
                || row.origin != PalwBudgetOriginV1::HoldLiability
                || row.liability_sompi != self.liability_sompi
                || row.accepted_daa != self.accepted_daa
            {
                return Err(bad("native binding differs from exact kept liability shadow"));
            }
            if row.open && row.in_window && row.reuse_not_before > now {
                held = held.max(row.liability_sompi);
            }
        }
        Ok(held)
    }
    pub fn exclusive_held_v1(&self, state: &PalwChainStateV2, now: u64) -> Result<u128, PalwStateV2Error> {
        let held = self.actual_held_v1(state, now)?;
        let baseline = match self.source {
            NativeBudgetSourceV1::MiningSeat39 => {
                use crate::palw_native_mining_v39::{MINING_SEAT_KIND_V39, NativeMiningSeatRecordV39};
                let route = state.kernel_route_state_v1().ok_or_else(|| bad("native seat binding route absent"))?;
                let key = borsh::to_vec(&(39u8, MINING_SEAT_KIND_V39, (self.source_id.as_bytes(), self.producer))).map_err(bad)?;
                if let Some(bytes) = route.aux.get(&(112, key)) {
                    if bytes.len() > 2048 {
                        return Err(bad("native bound seat extent"));
                    }
                    let (root, seat): ([u8; 64], NativeMiningSeatRecordV39) = borsh::from_slice(bytes).map_err(bad)?;
                    if seat.root() != root
                        || seat.version != 39
                        || seat.model != self.source_id.as_bytes()
                        || seat.bond != self.producer
                        || seat.since != self.accepted_daa
                        || u128::from(seat.stake) != self.liability_sompi
                    {
                        return Err(bad("native binding differs from actual possession seat"));
                    }
                } else if !self.closed {
                    return Err(bad("open native binding lost possession seat"));
                }
                0
            }
            NativeBudgetSourceV1::Mining39 => {
                use crate::palw_native_mining_v39::{MINING_CLAIM_KIND_V39, MINING_COLLECTED_KIND_V39, NativeMiningClaimRecordV39};
                let route = state.kernel_route_state_v1().ok_or_else(|| bad("native mining binding route absent"))?;
                let key = borsh::to_vec(&(39u8, MINING_CLAIM_KIND_V39, self.source_id.as_bytes())).map_err(bad)?;
                if let Some(bytes) = route.aux.get(&(112, key)) {
                    if bytes.len() > 16384 {
                        return Err(bad("native binding source claim extent"));
                    }
                    let pair = crate::palw_native_resource_v1::retain_native_value_decode_classified_v1::<
                        ([u8; 64], NativeMiningClaimRecordV39),
                        std::io::Error,
                    >(bytes, || borsh::from_slice(bytes))
                    .map_err(|e| match e {
                        NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
                        NativeDecodeErrorV1::Decode(e) => bad(e),
                    })?;
                    let _hash = crate::palw_native_resource_v1::reserve_native_requests_v1((bytes.len() as u64) * 2)
                        .map_err(PalwStateV2Error::NativeResource)?;
                    let (root, claim) = &*pair;
                    if claim.root() != *root
                        || claim.id != self.source_id.as_bytes()
                        || claim.producer != self.producer
                        || claim.admitted_daa != self.accepted_daa
                    {
                        return Err(bad("native binding differs from actual mining source"));
                    }
                } else {
                    let key = borsh::to_vec(&(39u8, MINING_COLLECTED_KIND_V39, self.source_id.as_bytes())).map_err(bad)?;
                    let bytes =
                        route.aux.get(&(112, key)).ok_or_else(|| bad("native mining binding lost source and collection tombstone"))?;
                    let _: ([u8; 64], Option<(u64, bool)>, u64) = borsh::from_slice(bytes).map_err(bad)?;
                    if !self.closed {
                        return Err(bad("open native binding cannot name a collected source"));
                    }
                }
                0
            }
            NativeBudgetSourceV1::Job39 => {
                state.native_job_source_baseline_v39(self.source_id, self.producer, self.accepted_daa, self.original_reserve_sompi)?
            }
        };
        Ok(held.saturating_sub(baseline))
    }
}
pub fn native_budget_binding_v1(
    state: &PalwChainStateV2,
    id: Hash64,
) -> Result<Option<NativeRetainedV1<NativeBudgetBindingV1>>, PalwStateV2Error> {
    let Some(route) = state.kernel_route_state_v1() else { return Ok(None) };
    if route
        .aux
        .range((NATIVE_BUDGET_BINDING_TABLE_V1, Vec::new())..)
        .next()
        .is_none_or(|((table, _), _)| *table != NATIVE_BUDGET_BINDING_TABLE_V1)
    {
        return Ok(None);
    }
    let _scratch = crate::palw_native_resource_v1::reserve_native_requests_v1(4096).map_err(PalwStateV2Error::NativeResource)?;
    let Some(bytes) = route.aux.get(&(NATIVE_BUDGET_BINDING_TABLE_V1, native_budget_binding_key_v1(id))) else { return Ok(None) };
    let row = decode(bytes)?;
    row.validate_shape_v1()?;
    if row.budget_id != id {
        return Err(bad("native budget binding key differs"));
    }
    Ok(Some(row))
}
pub fn native_budget_excess_for_bond_v1(state: &PalwChainStateV2, bond: &PalwBondKeyV2, now: u64) -> Result<u128, PalwStateV2Error> {
    sum_for_bond_v1(state, bond, now, true)
}
/// Actual native holds for a view that has not already included native originals.
/// Shared committed capital uses the exclusive accessor instead.
pub fn native_budget_held_for_bond_v1(state: &PalwChainStateV2, bond: &PalwBondKeyV2, now: u64) -> Result<u128, PalwStateV2Error> {
    sum_for_bond_v1(state, bond, now, false)
}
fn sum_for_bond_v1(
    state: &PalwChainStateV2,
    bond: &PalwBondKeyV2,
    now: u64,
    subtract_baseline: bool,
) -> Result<u128, PalwStateV2Error> {
    if !state.kernel_route_state_v1().is_some_and(|r| {
        r.aux
            .range((NATIVE_BUDGET_BINDING_TABLE_V1, Vec::new())..)
            .next()
            .is_some_and(|((table, _), _)| *table == NATIVE_BUDGET_BINDING_TABLE_V1)
    }) {
        return Ok(0);
    }
    let _scratch = crate::palw_native_resource_v1::reserve_native_requests_v1(4096).map_err(PalwStateV2Error::NativeResource)?;
    let Some(route) = state.kernel_route_state_v1() else { return Ok(0) };
    let prefix = borsh::to_vec(&(2u8, bond)).map_err(bad)?;
    let mut sum = 0u128;
    for ((table, key), bytes) in route.aux.range((NATIVE_BUDGET_BINDING_TABLE_V1, prefix.clone())..) {
        if *table != NATIVE_BUDGET_BINDING_TABLE_V1 || !key.starts_with(&prefix) {
            break;
        }
        let row = decode(bytes)?;
        row.validate_shape_v1()?;
        if row.producer != *bond || *key != native_budget_binding_producer_key_v1(bond, row.budget_id) {
            return Err(bad("native producer binding index differs"));
        }
        let primary =
            native_budget_binding_v1(state, row.budget_id)?.ok_or_else(|| bad("native producer index has no exact binding"))?;
        if *primary != *row {
            return Err(bad("native binding index body differs"));
        }
        // Always re-authenticate the actual source, even when this caller has
        // no original reservation to subtract.
        let exclusive = row.exclusive_held_v1(state, now)?;
        let amount = if subtract_baseline { exclusive } else { row.actual_held_v1(state, now)? };
        sum = sum.checked_add(amount).ok_or(PalwStateV2Error::Overflow("native exclusive collateral sum"))?;
    }
    Ok(sum)
}
pub fn validate_native_budget_bindings_v1(state: &PalwChainStateV2, now: u64) -> Result<(), PalwStateV2Error> {
    if !state.kernel_route_state_v1().is_some_and(|r| {
        r.aux
            .range((NATIVE_BUDGET_BINDING_TABLE_V1, Vec::new())..)
            .next()
            .is_some_and(|((table, _), _)| *table == NATIVE_BUDGET_BINDING_TABLE_V1)
    }) {
        return Ok(());
    }
    let _scratch = crate::palw_native_resource_v1::reserve_native_requests_v1(4096).map_err(PalwStateV2Error::NativeResource)?;
    if let Some(route) = state.kernel_route_state_v1() {
        for ((table, key), bytes) in &route.aux {
            if *table != NATIVE_BUDGET_BINDING_TABLE_V1 {
                continue;
            }
            let row = decode(bytes)?;
            row.validate_shape_v1()?;
            let expected = if key.first() == Some(&1) {
                native_budget_binding_key_v1(row.budget_id)
            } else if key.first() == Some(&2) {
                native_budget_binding_producer_key_v1(&row.producer, row.budget_id)
            } else {
                return Err(bad("native binding key kind"));
            };
            if *key != expected {
                return Err(bad("native binding canonical key"));
            }
            let other = if key[0] == 1 {
                native_budget_binding_producer_key_v1(&row.producer, row.budget_id)
            } else {
                native_budget_binding_key_v1(row.budget_id)
            };
            if route.aux.get(&(NATIVE_BUDGET_BINDING_TABLE_V1, other)) != Some(bytes) {
                return Err(bad("native binding requires exact primary/index pair"));
            }
            row.exclusive_held_v1(state, now)?;
        }
    }
    Ok(())
}
