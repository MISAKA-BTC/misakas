//! The Q36 serializer's temporary recurrence snapshot, not its output/prefix.
use super::*;
use crate::{
    fp_capture::Base0GdnLayerStateV1,
    qwen36::{Qwen36Cache, Qwen36LayerKind, Qwen36ShapeV1},
};
use kaspa_consensus_core::{
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, reserve_native_requests_v1},
    palw_qwen36_ops::Qwen36GdnStateV1,
};
use std::mem::size_of;
type Error = NativeDecodeErrorV1<String>;
const PATH: &str = "Q36 recurrence serializer snapshot";
fn resource<T: ?Sized>(why: &str) -> Error {
    Error::Resource(NativeCoverageV1::unsupported::<T>(PATH, why))
}
fn slots<T>(n: usize) -> Result<u64, Error> {
    u64::try_from(n)
        .ok()
        .and_then(|n| n.checked_mul(size_of::<T>() as u64))
        .ok_or_else(|| Error::Resource(NativeCoverageV1::overflow::<T>(PATH)))
}
fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or_else(|| Error::Resource(NativeCoverageV1::overflow::<Base0GdnLayerStateV1>(PATH)))
}
fn request(shape: &Qwen36ShapeV1, cache: &Qwen36Cache) -> Result<u64, Error> {
    let count = shape.layer_types.iter().filter(|kind| **kind == Qwen36LayerKind::LinearAttention).count();
    let mut bytes = add(slots::<u16>(count)?, slots::<Base0GdnLayerStateV1>(count)?)?;
    for (index, kind) in shape.layer_types.iter().enumerate() {
        if *kind != Qwen36LayerKind::LinearAttention {
            continue;
        }
        if let Some(heads) = cache.gdn.get(index) {
            bytes = add(bytes, slots::<Qwen36GdnStateV1>(heads.len())?)?;
            for head in heads {
                bytes = add(bytes, slots::<i32>(head.s.len())?)?;
            }
        }
        if let Some(rows) = cache.conv.get(index) {
            bytes = add(bytes, slots::<Vec<i32>>(rows.len())?)?;
            for row in rows {
                bytes = add(bytes, slots::<i32>(row.len())?)?;
            }
        }
    }
    Ok(bytes)
}
fn exact<T>(count: usize) -> Result<Vec<T>, Error> {
    let mut out = Vec::new();
    out.try_reserve_exact(count).map_err(|_| resource::<T>("snapshot allocation failed"))?;
    if out.capacity() != count {
        return Err(resource::<T>("allocator returned an unquoted snapshot capacity"));
    }
    Ok(out)
}
fn copied<T: Copy>(source: &[T]) -> Result<Vec<T>, Error> {
    let mut out = exact(source.len())?;
    out.extend_from_slice(source);
    Ok(out)
}
/// Values precede the lease on normal return, construction failure and unwind.
/// Source cache and serialized destination chunks have separate ownership.
struct Snapshot {
    layers: Vec<u16>,
    states: Vec<Base0GdnLayerStateV1>,
    _lease: Option<NativeDemandLeaseV1>,
}
impl Snapshot {
    fn new(shape: &Qwen36ShapeV1, cache: &Qwen36Cache) -> Result<Self, Error> {
        let bytes = request(shape, cache)?;
        let lease = reserve_native_requests_v1(bytes).map_err(Error::Resource)?;
        // Construct the owner first: a later allocation failure retires every
        // completed nested allocation before releasing its admitted request.
        let mut out = Self { layers: Vec::new(), states: Vec::new(), _lease: lease };
        let count = shape.layer_types.iter().filter(|kind| **kind == Qwen36LayerKind::LinearAttention).count();
        out.layers = exact(count)?;
        out.states = exact(count)?;
        for (index, kind) in shape.layer_types.iter().enumerate() {
            if *kind != Qwen36LayerKind::LinearAttention {
                continue;
            }
            out.layers.push(index as u16); // historical collector's exact spelling
            let source_heads = cache.gdn.get(index).map(Vec::as_slice).unwrap_or(&[]);
            let source_conv = cache.conv.get(index).map(Vec::as_slice).unwrap_or(&[]);
            let mut state = Base0GdnLayerStateV1 { heads: exact(source_heads.len())?, conv: Vec::new() };
            state.conv = exact(source_conv.len())?;
            for head in source_heads {
                state.heads.push(Qwen36GdnStateV1 { d_v: head.d_v, d_k: head.d_k, s: copied(&head.s)? });
            }
            for row in source_conv {
                state.conv.push(copied(row)?);
            }
            out.states.push(state);
        }
        Ok(out)
    }
}
pub(super) fn serialize_v4(
    kernels: &Qwen36RecomputeKernelsV1<'_>,
    profile: &PalwShapeProfileV3,
    positions: u32,
) -> Result<Vec<Vec<u8>>, Error> {
    let snapshot = Snapshot::new(kernels.shape, &kernels.cache)?;
    kernels
        .state_chunks_from_recurrence_v4(profile, positions, &snapshot.layers, &snapshot.states)
        .map_err(|semantic| Error::Decode(semantic.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
    fn free(ceiling: u64) -> u64 {
        let (mut lo, mut hi) = (0, ceiling);
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if reserve_native_requests_v1(mid).is_ok() { lo = mid } else { hi = mid - 1 }
        }
        lo
    }
    #[test]
    fn actual_native_cache_snapshot_admits_nested_copies_and_retires_lease_after_values() {
        const CHILD: &str = "MISAKA_Q36_SNAPSHOT_OWNER_CHILD";
        if std::env::var(CHILD).as_deref() != Ok("1") {
            let full = format!("{}::actual_native_cache_snapshot_admits_nested_copies_and_retires_lease_after_values", module_path!());
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
                    .env(CHILD, "1")
                    .status()
                    .unwrap()
                    .success()
            );
            return;
        }
        let artifact = crate::qwen36::qwen36_dev_fixture(4, 8);
        let cache = Qwen36Cache::new(&artifact.shape);
        let raw = qwen36_recurrence_state_v1(&artifact.shape, &cache);
        let bytes = request(&artifact.shape, &cache).unwrap();
        assert!(bytes > 0 && !raw.1.is_empty());
        let ceiling = 256 << 20;
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
        let owner = Snapshot::new(&artifact.shape, &cache).unwrap();
        assert_eq!((&owner.layers, &owner.states), (&raw.0, &raw.1));
        assert_eq!(free(ceiling), ceiling - bytes, "all nested snapshot values still carry their exact typed admission");
        assert_eq!(owner.layers.capacity(), owner.layers.len());
        assert_eq!(owner.states.capacity(), owner.states.len());
        for state in &owner.states {
            assert_eq!(state.heads.capacity(), state.heads.len());
            assert_eq!(state.conv.capacity(), state.conv.len());
            for head in &state.heads {
                assert_eq!(head.s.capacity(), head.s.len());
            }
            for row in &state.conv {
                assert_eq!(row.capacity(), row.len());
            }
        }
        drop(owner);
        assert_eq!(free(ceiling), ceiling);
        let blocker = reserve_native_requests_v1(ceiling).unwrap();
        assert!(
            matches!(Snapshot::new(&artifact.shape, &cache), Err(Error::Resource(_))),
            "target constructor admission itself, no prefix/leg/root prerequisite"
        );
        assert_eq!(qwen36_recurrence_state_v1(&artifact.shape, &cache), raw);
        drop(blocker);
        let retry = Snapshot::new(&artifact.shape, &cache).unwrap();
        assert_eq!((&retry.layers, &retry.states), (&raw.0, &raw.1));
        drop(retry);
        assert_eq!(free(ceiling), ceiling);
        assert!(matches!(slots::<Qwen36GdnStateV1>(usize::MAX), Err(Error::Resource(_))));
    }
}
