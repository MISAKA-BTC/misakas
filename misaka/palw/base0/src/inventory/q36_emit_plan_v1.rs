//! Reserved canonical ordering and per-tensor overlap merge workspace.
use super::{InventoryBuildError, Q36PlannedTensorV1, Q36RowReaderV1, Qwen36RowSinkV1};
use kaspa_consensus_core::palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1};
use std::mem::size_of;

fn quote<T>(count: usize) -> Result<u64, NativeCoverageV1> {
    count
        .checked_mul(size_of::<T>())
        .and_then(|n| n.checked_mul(2))
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| NativeCoverageV1::overflow::<T>("Q36 emitter workspace"))
}
fn alloc_error<T>(e: std::collections::TryReserveError) -> NativeCoverageV1 {
    NativeCoverageV1::unsupported::<T>("Q36 emitter allocation", &e.to_string())
}
struct Indices {
    order: Vec<usize>,
    distinct: Vec<usize>,
    _ticket: Option<NativeDemandLeaseV1>,
}
impl Indices {
    fn request(len: usize) -> Result<u64, NativeCoverageV1> {
        quote::<Self>(1)?
            .checked_add(quote::<usize>(len)?.checked_mul(2).ok_or_else(|| NativeCoverageV1::overflow::<Self>("Q36 index arrays"))?)
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>("Q36 index owner"))
    }
    fn new(plan: &[Q36PlannedTensorV1]) -> Result<Self, NativeCoverageV1> {
        let ticket = reserve_native_requests_v1(Self::request(plan.len())?)?;
        let mut order = Vec::new();
        order.try_reserve_exact(plan.len()).map_err(alloc_error::<Self>)?;
        order.extend(0..plan.len());
        // Explicit original index makes this identical to the former stable
        // key sort, while unstable sorting needs no separate heap scratch.
        order.sort_unstable_by(|&a, &b| (plan[a].name.as_str(), plan[a].layer, a).cmp(&(plan[b].name.as_str(), plan[b].layer, b)));
        let mut distinct = Vec::new();
        distinct.try_reserve_exact(plan.len()).map_err(alloc_error::<Self>)?;
        Ok(Self { order, distinct, _ticket: ticket })
    }
}
struct Row {
    start: u32,
    sequence: usize,
    bytes: Vec<u8>,
    _ticket: Option<NativeDemandLeaseV1>,
}
struct MergeRows {
    rows: Vec<Row>,
    slots_ticket: Option<NativeDemandLeaseV1>,
    _base_ticket: Option<NativeDemandLeaseV1>,
}
impl MergeRows {
    fn new() -> Result<Self, NativeCoverageV1> {
        let ticket = reserve_native_requests_v1(quote::<Self>(1)?)?;
        Ok(Self { rows: Vec::new(), slots_ticket: None, _base_ticket: ticket })
    }
    fn push(&mut self, start: u32, source: &[u8]) -> Result<(), NativeCoverageV1> {
        if self.rows.len() == self.rows.capacity() {
            let len = self
                .rows
                .capacity()
                .max(1)
                .checked_mul(2)
                .ok_or_else(|| NativeCoverageV1::overflow::<Self>("Q36 merge slot growth"))?;
            let ticket = reserve_native_requests_v1(quote::<Row>(len)?)?;
            // Old slots stay admitted through fallible realloc, and failed
            // growth leaves the old rows and slot reservation unchanged.
            self.rows.try_reserve_exact(len - self.rows.len()).map_err(alloc_error::<Self>)?;
            self.slots_ticket = ticket;
        }
        let ticket = reserve_native_requests_v1(quote::<u8>(source.len())?)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(source.len()).map_err(alloc_error::<Row>)?;
        bytes.extend_from_slice(source);
        self.rows.push(Row { start, sequence: self.rows.len(), bytes, _ticket: ticket });
        Ok(())
    }
    fn canonicalize(&mut self) {
        // Keep the same first-planned row at each offset without stable sort's
        // hidden allocation. Dropped duplicate payloads release their tickets.
        self.rows.sort_unstable_by_key(|row| (row.start, row.sequence));
        self.rows.dedup_by_key(|row| row.start);
    }
}

pub(super) fn emit(
    artifact: &crate::qwen36::Qwen36ArtifactV1,
    plan: &[Q36PlannedTensorV1],
    block: usize,
    sink: &mut Qwen36RowSinkV1<'_>,
) -> Result<(), InventoryBuildError> {
    let mut indices = Indices::new(plan)?;
    let mut reader = Q36RowReaderV1::new(artifact, block)?;
    let mut i = 0;
    while i < indices.order.len() {
        let key = (plan[indices.order[i]].name.as_str(), plan[indices.order[i]].layer);
        let mut j = i + 1;
        while j < indices.order.len() && (plan[indices.order[j]].name.as_str(), plan[indices.order[j]].layer) == key {
            j += 1;
        }
        indices.distinct.clear();
        for &k in &indices.order[i..j] {
            if !indices.distinct.iter().any(|&d| plan[d].rows == plan[k].rows) {
                indices.distinct.push(k);
            }
        }
        if let [only] = indices.distinct[..] {
            reader.stream(&plan[only], sink)?;
        } else {
            // Only one tensor's distinct plans are materialized at a time.
            let mut rows = MergeRows::new()?;
            for &k in &indices.distinct {
                reader.stream(&plan[k], &mut |_, _, start, bytes| {
                    rows.push(start, bytes)?;
                    Ok(())
                })?;
            }
            rows.canonicalize();
            for row in &rows.rows {
                sink(key.0, key.1, row.start, &row.bytes)?;
            }
        }
        i = j;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{Q36RowsV1, q36_missing};
    use super::*;

    fn plan() -> Vec<Q36PlannedTensorV1> {
        vec![
            Q36PlannedTensorV1 { name: "z".into(), layer: Some(1), rows: Q36RowsV1::Small(vec![(4, vec![4]), (0, vec![1])]) },
            Q36PlannedTensorV1 { name: "a".into(), layer: None, rows: Q36RowsV1::Small(vec![(0, vec![9])]) },
            Q36PlannedTensorV1 { name: "z".into(), layer: Some(1), rows: Q36RowsV1::Small(vec![(4, vec![4]), (0, vec![1])]) },
            Q36PlannedTensorV1 { name: "z".into(), layer: Some(1), rows: Q36RowsV1::Zeros { total: 8, stride: 4 } },
            Q36PlannedTensorV1 { name: "z".into(), layer: None, rows: Q36RowsV1::Small(vec![(0, vec![8])]) },
        ]
    }
    #[test]
    fn overlapping_plans_preserve_first_planned_rows_and_canonical_keys() {
        let artifact = crate::qwen36::qwen36_dev_fixture_heads(4, 8, 4, 4, 8, 32);
        let plan = plan();
        let mut got = Vec::new();
        emit(&artifact, &plan, 3, &mut |name, layer, start, bytes| {
            got.push((name.to_string(), layer, start, bytes.to_vec()));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            got,
            vec![
                ("a".into(), None, 0, vec![9]),
                ("z".into(), None, 0, vec![8]),
                ("z".into(), Some(1), 0, vec![1]),
                ("z".into(), Some(1), 4, vec![4])
            ]
        );
    }
    #[test]
    fn merge_matches_stable_first_wins_for_many_duplicate_offsets() {
        let mut rows = MergeRows::new().unwrap();
        let mut expected = Vec::new();
        for i in 0..1024u32 {
            let offset = (i * 71) % 97;
            let bytes = i.to_le_bytes();
            rows.push(offset, &bytes).unwrap();
            expected.push((offset, bytes.to_vec()));
        }
        expected.sort_by_key(|(offset, _)| *offset);
        expected.dedup_by_key(|(offset, _)| *offset);
        rows.canonicalize();
        let actual: Vec<_> = rows.rows.iter().map(|row| (row.start, row.bytes.clone())).collect();
        assert_eq!(actual, expected);
    }
    #[test]
    fn capacity_child() {
        use kaspa_consensus_core::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
        let Ok(case) = std::env::var("MISAKA_EMITTER_WORKSPACE_CASE") else {
            return;
        };
        let artifact = crate::qwen36::qwen36_dev_fixture_heads(4, 8, 4, 4, 8, 32);
        let plan = plan();
        let base = quote::<MergeRows>(1).unwrap();
        let slots2 = quote::<Row>(2).unwrap();
        let slots4 = quote::<Row>(4).unwrap();
        let payload = quote::<u8>(4).unwrap();
        let memory = match case.as_str() {
            "indices-short" => Indices::request(plan.len()).unwrap() - 1,
            "indices-exact" => Indices::request(plan.len()).unwrap(),
            "merge-short" => base + slots2 + payload * 2 + slots4 - 1,
            "merge-exact" => base + slots2 + payload * 2 + slots4,
            "bytes-short" => base + slots2 + payload - 1,
            _ => 1 << 20,
        };
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
        if case.starts_with("indices") {
            let result = Indices::new(&plan);
            if case.ends_with("short") {
                assert!(result.is_err());
            } else {
                let owner = result.unwrap();
                assert!(reserve_native_requests_v1(1).is_err());
                drop(owner);
            }
            assert!(Indices::request(usize::MAX).is_err());
        } else if case.starts_with("merge") || case == "bytes-short" {
            let mut rows = MergeRows::new().unwrap();
            if case == "bytes-short" {
                assert!(rows.push(0, &[1; 4]).is_err());
                assert!(rows.rows.is_empty());
            } else {
                rows.push(0, &[1; 4]).unwrap();
                rows.push(0, &[2; 4]).unwrap();
                assert_eq!(rows.rows.capacity(), 2);
                let result = rows.push(4, &[3; 4]);
                if case == "merge-short" {
                    assert!(result.is_err());
                    assert_eq!(rows.rows.len(), 2);
                    assert_eq!(rows.rows[0].bytes, [1; 4]);
                } else {
                    result.unwrap();
                    rows.canonicalize();
                    assert_eq!(rows.rows.len(), 2);
                    let resident = base + slots4 + payload * 2;
                    assert!(reserve_native_requests_v1(memory - resident + 1).is_err());
                    assert!(reserve_native_requests_v1(memory - resident).is_ok(), "duplicate payload ticket is returned");
                }
            }
            drop(rows);
        } else if case == "sink-error" {
            let mut calls = 0;
            let result = emit(&artifact, &plan[0..1], 3, &mut |_, _, _, _| {
                calls += 1;
                Err(q36_missing("test sink refusal"))
            });
            assert!(result.is_err());
            assert_eq!(calls, 1);
            // Exercise the merge arm's sink-error cleanup, not only direct rows.
            let result = emit(&artifact, &plan, 3, &mut |name, layer, _, _| {
                if name == "z" && layer == Some(1) {
                    return Err(q36_missing("merged sink refusal"));
                }
                Ok(())
            });
            assert!(result.is_err());
        } else {
            panic!("unknown case");
        }
        assert!(reserve_native_requests_v1(memory).is_ok(), "all workspace reservations are returned");
    }
    #[test]
    fn index_merge_peak_payload_and_sink_error_lifetimes() {
        for case in ["indices-short", "indices-exact", "merge-short", "merge-exact", "bytes-short", "sink-error"] {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "inventory::q36_emit_plan_v1::tests::capacity_child", "--nocapture"])
                .env("MISAKA_EMITTER_WORKSPACE_CASE", case)
                .output()
                .unwrap();
            assert!(out.status.success(), "{case}: {} {}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"));
        }
    }
}
