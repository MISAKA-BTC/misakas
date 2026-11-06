//! Owning-fold lifecycle of exact native budget provenance.
use super::*;
use crate::palw_native_budget_binding_v1::*;
fn bad(s: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.to_string())
}
pub(super) fn install_native_budget_binding_v1(
    b: &mut TransitionBuilder<'_>,
    source: NativeBudgetSourceV1,
    source_id: Hash64,
    budget_id: Hash64,
    producer: PalwBondKeyV2,
    birth: u64,
    baseline: u128,
) -> Result<(), PalwStateV2Error> {
    let row = b
        .state
        .bond_budget()
        .and_then(|budget| budget.claim_row(&budget_id))
        .ok_or_else(|| bad("native source has no committed actual budget"))?;
    let origin = match source {
        NativeBudgetSourceV1::Mining39 => crate::palw_bond_budget_v1::PalwBudgetOriginV1::FreePrompt,
        NativeBudgetSourceV1::Job39 | NativeBudgetSourceV1::MiningSeat39 => {
            crate::palw_bond_budget_v1::PalwBudgetOriginV1::KernelRoute
        }
    };
    if row.bond != producer || row.accepted_daa != birth || row.origin != origin || !row.open || !row.live {
        return Err(bad("native budget admission provenance differs"));
    }
    if row.liability_sompi == 0 {
        return Ok(());
    }
    let binding = NativeBudgetBindingV1 {
        version: 1,
        source,
        source_id,
        budget_id,
        producer,
        accepted_daa: birth,
        liability_sompi: row.liability_sompi,
        original_reserve_sompi: baseline,
        closed: false,
    };
    binding.validate_shape_v1()?;
    if native_budget_binding_v1(&b.state, budget_id)?.is_some() {
        return Err(bad("native budget binding duplicate"));
    }
    write(b, &binding)
}
fn write(b: &mut TransitionBuilder<'_>, row: &NativeBudgetBindingV1) -> Result<(), PalwStateV2Error> {
    b.write_kernel_row_encoded_v1(NATIVE_BUDGET_BINDING_TABLE_V1, &native_budget_binding_key_v1(row.budget_id), Some(row))?;
    b.write_kernel_row_encoded_v1(
        NATIVE_BUDGET_BINDING_TABLE_V1,
        &native_budget_binding_producer_key_v1(&row.producer, row.budget_id),
        Some(row),
    )
}
pub(super) fn sync_native_budget_binding_v1(b: &mut TransitionBuilder<'_>, id: Hash64) -> Result<(), PalwStateV2Error> {
    let Some(saved) = native_budget_binding_v1(&b.state, id)? else { return Ok(()) };
    let mut row = (*saved).clone();
    let actual = b.state.bond_budget().and_then(|budget| budget.claim_row(&id));
    if let Some(actual) = actual {
        row.closed = !actual.open;
    } else {
        row.closed = true;
    }
    if *saved != row {
        write(b, &row)?;
    }
    Ok(())
}
pub(super) fn sweep_native_budget_bindings_v1(b: &mut TransitionBuilder<'_>, now: u64) -> Result<(), PalwStateV2Error> {
    use crate::palw_bond_budget_v1::{PalwHoldViewV1, palw_bond_hold_shadow_claim_v1};
    let route = match b.state.kernel_route_state_v1() {
        Some(route) => route,
        None => return Ok(()),
    };
    let count = route.aux.keys().filter(|(table, key)| *table == NATIVE_BUDGET_BINDING_TABLE_V1 && key.first() == Some(&1)).count();
    if count == 0 {
        return Ok(());
    }
    let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    q.exact_v1::<Hash64>(count as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _lease = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut removed = Vec::new();
    removed.try_reserve_exact(count).map_err(|_| {
        PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<Hash64>(
            "native budget binding retirement",
            "allocation refused",
        ))
    })?;
    for (table, key) in route.aux.keys() {
        if *table != NATIVE_BUDGET_BINDING_TABLE_V1 || key.first() != Some(&1) {
            continue;
        }
        if key.len() != 65 {
            return Err(bad("native budget primary key extent"));
        }
        let id = Hash64::from_bytes(key[1..].try_into().map_err(bad)?);
        let row = native_budget_binding_v1(&b.state, id)?.ok_or_else(|| bad("native binding vanished"))?;
        let budget = b.state.bond_budget().ok_or_else(|| bad("native budget binding has no ledger"))?;
        let window = budget.claim_row(&id).is_some_and(|r| r.in_window || r.open);
        let shadow =
            budget.claim_row(&palw_bond_hold_shadow_claim_v1(PalwHoldViewV1::Liability, &id)).is_some_and(|r| r.open || r.in_window);
        if row.closed && !window && !shadow && row.actual_held_v1(&b.state, now)? == 0 {
            removed.push(id);
        }
    }
    for id in removed {
        let row = native_budget_binding_v1(&b.state, id)?.ok_or_else(|| bad("native cleanup binding absent"))?;
        b.write_kernel_row(NATIVE_BUDGET_BINDING_TABLE_V1, native_budget_binding_producer_key_v1(&row.producer, id), None)?;
        b.write_kernel_row(NATIVE_BUDGET_BINDING_TABLE_V1, native_budget_binding_key_v1(id), None)?;
    }
    Ok(())
}
