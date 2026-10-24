//! Scoped admission for plan allocations and temporary name construction.
use super::{InventoryBuildError, Q36PlannedTensorV1, Q36RowsV1};
use kaspa_consensus_core::palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1};
use std::{
    cell::RefCell,
    fmt::{self, Write},
    mem::size_of,
    ops::Deref,
};

pub(super) struct Plan {
    entries: Vec<Q36PlannedTensorV1>,
    _budget: Budget,
}
impl Deref for Plan {
    type Target = [Q36PlannedTensorV1];
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}
impl Plan {
    pub(super) fn finish(entries: Vec<Q36PlannedTensorV1>, budget: Budget) -> Self {
        Self { entries, _budget: budget }
    }
}
struct State {
    lease: Option<NativeDemandLeaseV1>,
    bytes: u64,
    peak: u64,
}
pub(super) struct Budget {
    state: RefCell<State>,
}
pub(super) struct Scoped<'a, T> {
    value: Option<T>,
    budget: &'a Budget,
    bytes: u64,
}
impl<T> Deref for Scoped<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value.as_ref().unwrap()
    }
}
impl<T: fmt::Display> fmt::Display for Scoped<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}
impl<T> Scoped<'_, T> {
    pub(super) fn keep(mut self) -> T {
        self.bytes = 0;
        self.value.take().unwrap()
    }
}
impl<T> Drop for Scoped<'_, T> {
    fn drop(&mut self) {
        drop(self.value.take()); // Release the payload before its reservation.
        self.budget.release(self.bytes);
    }
}
pub(super) fn overflow() -> NativeCoverageV1 {
    NativeCoverageV1::overflow::<Plan>("Q36 plan allocation request")
}
fn quote<T>(count: usize) -> Result<u64, NativeCoverageV1> {
    count.checked_mul(size_of::<T>()).and_then(|n| n.checked_mul(2)).and_then(|n| u64::try_from(n).ok()).ok_or_else(overflow)
}
fn allocation(e: std::collections::TryReserveError) -> NativeCoverageV1 {
    NativeCoverageV1::unsupported::<Plan>("Q36 plan allocation", &e.to_string())
}
impl Budget {
    pub(super) fn new() -> Result<Self, NativeCoverageV1> {
        let bytes = quote::<Plan>(1)?;
        Ok(Self { state: RefCell::new(State { lease: reserve_native_requests_v1(bytes)?, bytes, peak: bytes }) })
    }
    fn grow(&self, additional: u64) -> Result<(), NativeCoverageV1> {
        let mut state = self.state.borrow_mut();
        let bytes = state.bytes.checked_add(additional).ok_or_else(overflow)?;
        if let Some(lease) = &mut state.lease {
            lease.grow_memory_v1(additional)?;
        }
        state.bytes = bytes;
        state.peak = state.peak.max(bytes);
        Ok(())
    }
    fn release(&self, bytes: u64) {
        let mut state = self.state.borrow_mut();
        state.bytes = state.bytes.checked_sub(bytes).expect("plan loan is held by its budget");
        if let Some(lease) = &mut state.lease {
            drop(lease.split_memory_v1(bytes).expect("plan loan is held by its lease"));
        }
    }
    fn scoped<T>(&self, value: T, bytes: u64) -> Result<Scoped<'_, T>, NativeCoverageV1> {
        self.grow(bytes)?;
        Ok(Scoped { value: Some(value), budget: self, bytes })
    }
    pub(super) fn text(&self, args: fmt::Arguments<'_>) -> Result<Scoped<'_, String>, NativeCoverageV1> {
        struct Count(usize);
        impl Write for Count {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0 = self.0.checked_add(s.len()).ok_or(fmt::Error)?;
                Ok(())
            }
        }
        let mut count = Count(0);
        count.write_fmt(args).map_err(|_| overflow())?;
        let request = quote::<String>(1)?.checked_add(quote::<u8>(count.0)?).ok_or_else(overflow)?;
        let mut text = self.scoped(String::new(), request)?;
        text.value.as_mut().unwrap().try_reserve_exact(count.0).map_err(allocation)?;
        text.value.as_mut().unwrap().write_fmt(args).map_err(|_| overflow())?;
        Ok(text)
    }
    pub(super) fn sub(&self, name: &str, layer: Option<u16>) -> Result<Scoped<'_, String>, NativeCoverageV1> {
        let Some(layer) = layer else {
            return self.text(format_args!("{name}"));
        };
        let digits = self.text(format_args!("{layer}"))?;
        let occurrences = name.matches("{layer}").count();
        let len = name
            .len()
            .checked_sub(occurrences.checked_mul(7).ok_or_else(overflow)?)
            .and_then(|n| n.checked_add(occurrences.checked_mul(digits.len())?))
            .ok_or_else(overflow)?;
        let request = quote::<String>(1)?.checked_add(quote::<u8>(len)?).ok_or_else(overflow)?;
        let mut result = self.scoped(String::new(), request)?;
        let value = result.value.as_mut().unwrap();
        value.try_reserve_exact(len).map_err(allocation)?;
        let mut parts = name.split("{layer}");
        value.push_str(parts.next().unwrap());
        for part in parts {
            value.push_str(&digits);
            value.push_str(part);
        }
        Ok(result)
    }
    pub(super) fn small(
        &self,
        count: usize,
        width: usize,
        mut fill: impl FnMut(usize, &mut [u8]) -> Result<u32, InventoryBuildError>,
    ) -> Result<Q36RowsV1, InventoryBuildError> {
        let bytes = count.checked_mul(width).ok_or_else(overflow)?;
        let request = quote::<Vec<(u32, Vec<u8>)>>(1)?
            .checked_add(quote::<(u32, Vec<u8>)>(count)?)
            .and_then(|n| n.checked_add(quote::<u8>(bytes).ok()?))
            .ok_or_else(overflow)?;
        let mut rows = self.scoped(Vec::new(), request)?;
        let values = rows.value.as_mut().unwrap();
        values.try_reserve_exact(count).map_err(allocation)?;
        for i in 0..count {
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(width).map_err(allocation)?;
            bytes.resize(width, 0);
            let start = fill(i, &mut bytes)?;
            values.push((start, bytes));
        }
        Ok(Q36RowsV1::Small(rows.keep()))
    }
    pub(super) fn fused(
        &self,
        name: &str,
    ) -> Result<Scoped<'_, Option<kaspa_consensus_core::palw_step_refute::PalwAttnFusedTensorsV1>>, NativeCoverageV1> {
        use kaspa_consensus_core::palw_step_refute as kd;
        // The canonical helper allocates four result names and one temporary
        // suffix. Each result prefix is no longer than the input name.
        let bytes = name
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(kd::PALW_ATTN_FUSED_SCORES_LEAF.len()))
            .and_then(|n| n.checked_add(kd::PALW_ATTN_FUSED_PROBS_LEAF.len()))
            .and_then(|n| n.checked_add(kd::PALW_ATTN_FUSED_VALUES_LEAF.len()))
            .and_then(|n| n.checked_add(kd::PALW_ATTN_FUSED_SOFTMAX_STEM.len()))
            .and_then(|n| n.checked_add(kd::PALW_ATTN_FUSED_A16_SUFFIX.len()))
            .ok_or_else(overflow)?;
        let request = quote::<kd::PalwAttnFusedTensorsV1>(1)?
            .checked_add(quote::<String>(1)?)
            .and_then(|n| n.checked_add(quote::<u8>(bytes).ok()?))
            .ok_or_else(overflow)?;
        let mut result = self.scoped(None, request)?;
        *result.value.as_mut().unwrap() = kd::palw_attn_fused_tensors_v1(name);
        Ok(result)
    }
    pub(super) fn push(
        &self,
        plan: &mut Vec<Q36PlannedTensorV1>,
        slots: &mut u64,
        name: &str,
        layer: Option<u16>,
        rows: Q36RowsV1,
    ) -> Result<(), InventoryBuildError> {
        if plan.len() == plan.capacity() {
            let capacity = plan.capacity().max(1).checked_mul(2).ok_or_else(overflow)?;
            let request = quote::<Q36PlannedTensorV1>(capacity)?;
            let loan = self.scoped((), request)?;
            plan.try_reserve_exact(capacity - plan.len()).map_err(allocation)?;
            loan.keep();
            self.release(*slots);
            *slots = request;
        }
        let name = self.text(format_args!("{name}"))?.keep();
        plan.push(Q36PlannedTensorV1 { name, layer, rows });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{q36_missing, qwen36_emit_plan_v1, qwen36_inventory_plan_v1};
    use super::*;

    #[test]
    fn substitution_and_temporary_release_match_string_rules() {
        let budget = Budget::new().unwrap();
        let base = budget.state.borrow().bytes;
        for name in ["", "blk.{layer}.x", "{layer}{layer}", "あ.{layer}.{{layer}}", "plain"] {
            for layer in [None, Some(0), Some(9), Some(u16::MAX)] {
                let text = budget.sub(name, layer).unwrap();
                let expected = match layer {
                    Some(l) => name.replace("{layer}", &l.to_string()),
                    None => name.to_string(),
                };
                assert_eq!(&**text, &expected);
                drop(text);
                assert_eq!(budget.state.borrow().bytes, base);
            }
        }
        for name in ["blk.{layer}.attn_softmax_up.a16", "attn_softmax_up", "bad"] {
            let value = budget.fused(name).unwrap();
            assert_eq!(*value, kaspa_consensus_core::palw_step_refute::palw_attn_fused_tensors_v1(name));
            drop(value);
            assert_eq!(budget.state.borrow().bytes, base);
        }
        assert!(budget.small(usize::MAX, 17, |_, _| Ok(0)).is_err());
        assert!(budget.small(3, 17, |i, _| if i == 1 { Err(q36_missing("fill refused")) } else { Ok(0) }).is_err());
        assert_eq!(budget.state.borrow().bytes, base, "failed fill returns the whole row allocation");
    }

    #[test]
    fn capacity_child() {
        use kaspa_consensus_core::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
        let Ok(case) = std::env::var("MISAKA_PLAN_STORAGE_CASE") else {
            return;
        };
        let artifact = crate::qwen36::qwen36_dev_fixture_heads(4, 8, 4, 4, 8, 32);
        let geometry = kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
            layer_count: 4,
            full_attention_interval: 4,
            hidden_dim: 32,
            attn_heads: 4,
            attn_kv_heads: 2,
            attn_head_dim: 16,
            rope_dims: 4,
            rope_freq_base_bits: 0x4B18_9680,
            gdn_k_heads: 4,
            gdn_v_heads: 4,
            gdn_head_dim: 8,
            gdn_conv_kernel: 4,
            n_experts: 8,
            experts_per_token: 4,
            moe_dim: 16,
            shared_dim: 16,
            attn_output_gate: 1,
            vocab_size: 64,
            n_ctx: 8,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 512,
        };
        let profile = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v8(geometry).unwrap();
        let base = quote::<Plan>(1).unwrap();
        let text = quote::<String>(1).unwrap() + quote::<u8>(4).unwrap();
        let slots2 = quote::<Q36PlannedTensorV1>(2).unwrap();
        let slots4 = quote::<Q36PlannedTensorV1>(4).unwrap();
        let name = quote::<String>(1).unwrap() + quote::<u8>(1).unwrap();
        let memory = match case.as_str() {
            "zero" => 0,
            "text-short" => base + text - 1,
            "text-exact" => base + text,
            "slots-short" => base + slots2 + 2 * name + slots4 - 1,
            "slots-exact" => base + slots2 + 2 * name + slots4,
            _ => 1 << 24,
        };
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
        if case == "zero" {
            assert!(Budget::new().is_err());
        } else if case.starts_with("text") {
            let budget = Budget::new().unwrap();
            let result = budget.text(format_args!("abcd"));
            if case.ends_with("short") {
                assert!(result.is_err());
                drop(result);
            } else {
                let text = result.unwrap();
                assert!(reserve_native_requests_v1(1).is_err());
                drop(text);
            }
            assert_eq!(budget.state.borrow().bytes, base);
            drop(budget);
        } else if case.starts_with("slots") {
            let budget = Budget::new().unwrap();
            let mut entries = Vec::new();
            let mut slots = 0;
            for _ in 0..2 {
                budget.push(&mut entries, &mut slots, "a", None, Q36RowsV1::Zeros { total: 1, stride: 1 }).unwrap();
            }
            let result = budget.push(&mut entries, &mut slots, "b", None, Q36RowsV1::Zeros { total: 1, stride: 1 });
            if case.ends_with("short") {
                assert!(result.is_err());
                assert_eq!(entries.len(), 2);
            } else {
                result.unwrap();
                assert_eq!(entries.len(), 3);
            }
            let owner = Plan::finish(entries, budget);
            let held = owner._budget.state.borrow().bytes;
            assert!(reserve_native_requests_v1(memory - held + 1).is_err());
            assert!(reserve_native_requests_v1(memory - held).is_ok());
            drop(owner);
        } else if case == "actual-plan" {
            // Find the actual operation's admission peak, including separately
            // reserved parameter decoding, with blockers rather than rearming.
            let mut low = 0;
            let mut high = memory;
            while low < high {
                let limit = low + (high - low) / 2;
                let blocker = reserve_native_requests_v1(memory - limit).unwrap();
                match qwen36_inventory_plan_v1(&artifact, &profile) {
                    Ok(owner) => {
                        drop(owner);
                        high = limit;
                    }
                    Err(InventoryBuildError::Resource(_)) => low = limit + 1,
                    Err(other) => panic!("unexpected plan error: {other:?}"),
                }
                drop(blocker);
                assert!(reserve_native_requests_v1(memory).is_ok(), "trial leaks no partial plan");
            }
            assert!(low > base && low < memory);
            let blocker = reserve_native_requests_v1(memory - low + 1).unwrap();
            assert!(matches!(qwen36_inventory_plan_v1(&artifact, &profile), Err(InventoryBuildError::Resource(_))));
            drop(blocker);
            let blocker = reserve_native_requests_v1(memory - low).unwrap();
            let owner = qwen36_inventory_plan_v1(&artifact, &profile).unwrap();
            let held = owner._budget.state.borrow().bytes;
            assert!(reserve_native_requests_v1(low - held + 1).is_err());
            assert!(reserve_native_requests_v1(low - held).is_ok());
            drop(owner);
            drop(blocker);
            let owner = qwen36_inventory_plan_v1(&artifact, &profile).unwrap();
            let held = owner._budget.state.borrow().bytes;
            let mut seen = 0;
            qwen36_emit_plan_v1(&artifact, &owner, 17, &mut |_, _, _, _| {
                seen += 1;
                Ok(())
            })
            .unwrap();
            assert!(seen > 0);
            assert!(reserve_native_requests_v1(memory - held + 1).is_err());
            assert!(reserve_native_requests_v1(memory - held).is_ok(), "emitter scratch returns while the plan remains owned");
            drop(owner);
            println!("plan admission peak {low}; retained {held}; emitted {seen} rows");
        } else {
            panic!("unknown case");
        }
        assert!(reserve_native_requests_v1(memory).is_ok(), "all plan reservations return after drop/refusal");
    }
    #[test]
    fn admission_boundaries_and_actual_plan_owner_lifetime() {
        for case in ["zero", "text-short", "text-exact", "slots-short", "slots-exact", "actual-plan"] {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "inventory::q36_plan_storage_v1::tests::capacity_child", "--nocapture"])
                .env("MISAKA_PLAN_STORAGE_CASE", case)
                .output()
                .unwrap();
            assert!(out.status.success(), "{case}: {} {}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"));
            if case == "actual-plan" {
                print!("{}", String::from_utf8_lossy(&out.stdout));
            }
        }
    }
}
