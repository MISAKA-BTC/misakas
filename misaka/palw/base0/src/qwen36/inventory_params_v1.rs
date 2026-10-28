//! Inventory-only parameter reads: reserve decoded rows and any file scratch
//! before allocation, without admitting an entire routed expert into the LRU.
use super::{Qwen36ArtifactV1, Store, expert_key_v1};
use crate::{inventory::InventoryBuildError, operands::OperandError};
use kaspa_consensus_core::{
    palw_base0_a16::A16QuantParams,
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1},
};
use std::{mem::size_of, ops::Deref};

pub(crate) struct InventoryParamRowsV1 {
    rows: Vec<A16QuantParams>,
    _ticket: Option<NativeDemandLeaseV1>,
}
impl Deref for InventoryParamRowsV1 {
    type Target = [A16QuantParams];
    fn deref(&self) -> &Self::Target {
        &self.rows
    }
}

fn missing(name: &str) -> InventoryBuildError {
    InventoryBuildError::Operand(OperandError::UnknownTensor { name: name.to_string() })
}
fn count(len: usize, name: &str) -> Result<usize, InventoryBuildError> {
    if len == 0 || !len.is_multiple_of(A16QuantParams::WIRE_BYTES) {
        return Err(missing(name));
    }
    Ok(len / A16QuantParams::WIRE_BYTES)
}
fn request<T>(count: usize, element: usize) -> Result<u64, NativeCoverageV1> {
    count
        .checked_mul(element)
        .and_then(|n| n.checked_add(size_of::<T>()))
        .and_then(|n| n.checked_mul(2))
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| NativeCoverageV1::overflow::<T>("inventory parameter request"))
}
impl InventoryParamRowsV1 {
    fn decode(bytes: &[u8], name: &str) -> Result<Self, InventoryBuildError> {
        let count = count(bytes.len(), name)?;
        let ticket = reserve_native_requests_v1(request::<Self>(count, size_of::<A16QuantParams>())?)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(count)
            .map_err(|e| NativeCoverageV1::unsupported::<Self>("inventory decoded parameter allocation", &e.to_string()))?;
        for chunk in bytes.chunks_exact(A16QuantParams::WIRE_BYTES) {
            rows.push(A16QuantParams::from_wire(chunk).map_err(|_| missing(name))?);
        }
        Ok(Self { rows, _ticket: ticket })
    }
}

// Payload precedes ticket in drop order, including read/decode failures.
struct WireScratch {
    bytes: Vec<u8>,
    _ticket: Option<NativeDemandLeaseV1>,
}
impl Qwen36ArtifactV1 {
    /// Borrow metadata without allocating an error String on an expected miss
    /// (expert scans and absent optional exponent tensors use that outcome).
    pub(crate) fn inventory_tensor_len_v1(&self, name: &str) -> Option<usize> {
        match &self.store {
            Store::Owned(tensors) => tensors.get(name).map(Vec::len),
            Store::Mapped { map, directory } => {
                let &(offset, len) = directory.get(name)?;
                offset.checked_add(len).filter(|&end| end <= map.len()).map(|_| len)
            }
        }
    }
    pub(crate) fn inventory_param_rows_v1(&self, name: &str) -> Result<InventoryParamRowsV1, InventoryBuildError> {
        // Match the engine's source selection. The extent directory is immutable
        // artifact metadata; using it avoids an LRU miss loading unrelated weights.
        if let (Store::Mapped { map, .. }, Some(residency), Some((key, _))) = (&self.store, &self.residency, expert_key_v1(name))
            && residency.experts.contains_key(&key)
        {
            let &(offset, len) = residency.expert_param_extents.get(name).ok_or_else(|| missing(name))?;
            count(len, name)?;
            let ticket = reserve_native_requests_v1(request::<WireScratch>(len, 1)?)?;
            let mut wire = WireScratch { bytes: Vec::new(), _ticket: ticket };
            wire.bytes
                .try_reserve_exact(len)
                .map_err(|e| NativeCoverageV1::unsupported::<WireScratch>("inventory encoded parameter allocation", &e.to_string()))?;
            wire.bytes.resize(len, 0);
            let offset = u64::try_from(offset).map_err(|_| NativeCoverageV1::overflow::<WireScratch>("parameter file offset"))?;
            map.read_exact_at(offset, &mut wire.bytes).map_err(|_| missing(name))?;
            return InventoryParamRowsV1::decode(&wire.bytes, name);
        }
        InventoryParamRowsV1::decode(self.params.get(name).ok_or_else(|| missing(name))?, name)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Qwen36ResidencyPolicyV1, Qwen36Writer, open_artifact_with_residency, qwen36_dev_fixture_heads};
    use super::*;

    fn mapped(owned: &Qwen36ArtifactV1, suffix: &str) -> (Qwen36ArtifactV1, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("misaka-inventory-params-{}-{suffix}.palwq36", std::process::id()));
        let plan: Vec<_> = owned.tensor_names().into_iter().map(|n| (n.to_string(), owned.tensor(n).unwrap().len())).collect();
        let mut writer = Qwen36Writer::create(&path, &owned.shape, &owned.rope, owned.params_map(), plan.clone()).unwrap();
        for (name, _) in &plan {
            writer.push(name, &owned.tensor(name).unwrap()).unwrap();
        }
        writer.finish().unwrap();
        let mapped = open_artifact_with_residency(&path, Qwen36ResidencyPolicyV1::Bytes(owned.weight_bytes() as u64)).unwrap();
        assert!(mapped.residency.is_some());
        (mapped, path)
    }

    #[test]
    fn owned_and_file_parameters_match_engine_without_admitting_experts() {
        let owned = qwen36_dev_fixture_heads(4, 8, 4, 4, 8, 32);
        let (mapped, path) = mapped(&owned, "all");
        let before = mapped.residency.as_ref().unwrap().stats();
        let mut experts = 0;
        for name in owned.params.keys() {
            let expected = owned.param_rows(name).unwrap();
            assert_eq!(&*owned.inventory_param_rows_v1(name).unwrap(), expected.as_slice(), "owned {name}");
            assert_eq!(&*mapped.inventory_param_rows_v1(name).unwrap(), expected.as_slice(), "mapped {name}");
            experts += usize::from(mapped.residency.as_ref().unwrap().expert_param_extents.contains_key(name));
        }
        assert!(experts > 0);
        let after = mapped.residency.as_ref().unwrap().stats();
        assert_eq!(before.misses, after.misses);
        assert_eq!(before.bytes_read, after.bytes_read);
        assert_eq!(before.resident_expert_bytes, after.resident_expert_bytes);
        assert!(matches!(mapped.inventory_param_rows_v1("missing"), Err(InventoryBuildError::Operand(_))));
        drop(mapped);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn metadata_presence_matches_engine_bounds_on_owned_and_mapped_stores() {
        let owned = qwen36_dev_fixture_heads(4, 8, 4, 4, 8, 32);
        let (mut mapped, path) = mapped(&owned, "metadata");
        for name in owned.tensor_names() {
            assert_eq!(owned.inventory_tensor_len_v1(name), owned.tensor_len(name).ok());
            assert_eq!(mapped.inventory_tensor_len_v1(name), mapped.tensor_len(name).ok());
        }
        assert_eq!(owned.inventory_tensor_len_v1("absent"), None);
        assert_eq!(mapped.inventory_tensor_len_v1("absent"), None);
        let Store::Mapped { directory, .. } = &mut mapped.store else {
            panic!("mapped fixture");
        };
        directory.insert("invalid-overflow".into(), (usize::MAX, 2));
        directory.insert("invalid-extent".into(), (0, usize::MAX));
        for name in ["invalid-overflow", "invalid-extent"] {
            assert_eq!(mapped.inventory_tensor_len_v1(name), None);
            assert!(mapped.tensor_len(name).is_err());
        }
        drop(mapped);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn capacity_child() {
        use kaspa_consensus_core::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
        let Ok(case) = std::env::var("MISAKA_INVENTORY_PARAM_CASE") else {
            return;
        };
        let mut owned = qwen36_dev_fixture_heads(4, 8, 4, 4, 8, 32);
        let (mut mapped, path) = mapped(&owned, &case);
        let name = mapped.residency.as_ref().unwrap().expert_param_extents.keys().next().unwrap().clone();
        let len = owned.params[&name].len();
        let decoded = request::<InventoryParamRowsV1>(count(len, &name).unwrap(), size_of::<A16QuantParams>()).unwrap();
        let wire = request::<WireScratch>(len, 1).unwrap();
        let is_file = case.starts_with("file");
        let peak = decoded + if is_file { wire } else { 0 };
        let memory = peak - u64::from(case.ends_with("short"));
        if case == "bad-decode" {
            owned.params.get_mut(&name).unwrap()[8] = 63;
        }
        if case == "file-read-error" {
            mapped.residency.as_mut().unwrap().expert_param_extents.get_mut(&name).unwrap().0 = usize::MAX;
        }
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
        let result = if is_file { mapped.inventory_param_rows_v1(&name) } else { owned.inventory_param_rows_v1(&name) };
        if case.ends_with("short") {
            assert!(matches!(result, Err(InventoryBuildError::Resource(_))));
        } else if case == "bad-decode" || case == "file-read-error" {
            assert!(matches!(result, Err(InventoryBuildError::Operand(_))));
        } else {
            let rows = result.unwrap();
            assert_eq!(&*rows, owned.param_rows(&name).unwrap().as_slice());
            assert!(reserve_native_requests_v1(memory - decoded + 1).is_err(), "decoded rows retain their reservation");
            // File scratch is released before returning the decoded owner.
            assert!(reserve_native_requests_v1(memory - decoded).is_ok());
            drop(rows);
        }
        assert!(reserve_native_requests_v1(memory).is_ok(), "all tickets return on drop/refusal");
        drop(mapped);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn allocation_refusal_decode_read_failure_and_owner_lifetime() {
        for case in ["owned-short", "owned-exact", "file-short", "file-exact", "bad-decode", "file-read-error"] {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "qwen36::inventory_params_v1::tests::capacity_child", "--nocapture"])
                .env("MISAKA_INVENTORY_PARAM_CASE", case)
                .output()
                .unwrap();
            assert!(out.status.success(), "{case}: {} {}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"));
        }
    }
}
