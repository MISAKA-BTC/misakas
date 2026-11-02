//! Derived reverse EXEC incidence. Not hashed/carried and not a new owner row.
//! Native root/slice events fund incremental memberships; full restore funds
//! its own rebuild. Per-source readers never scan unrelated EXEC roots/slices.
use super::*;
use crate::palw_native_resource_v1::{
    NativeRetainedV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
use crate::palw_work_slice_v2::{PalwWorkRootV2, PalwWorkSliceRowV2};
fn bad(s: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::DeltaMismatch(s)
}
fn unit() -> u64 {
    crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<(Hash64, Hash64), u64>() as u64
}
fn request(
    s: &PalwChainStateV2,
    root: Hash64,
    old: Option<Hash64>,
    new: Option<Hash64>,
) -> Result<u64, PalwStateV2Error> {
    if let Some(source) = old {
        if s.exec_v2_source_roots
            .get(&(source, root))
            .copied()
            .unwrap_or(0)
            == 0
        {
            return Err(bad("EXEC derived incidence old contribution missing"));
        }
    }
    if old == new {
        return Ok(0);
    }
    if let Some(source) = new {
        let n = s
            .exec_v2_source_roots
            .get(&(source, root))
            .copied()
            .unwrap_or(0);
        n.checked_add(1)
            .ok_or(PalwStateV2Error::Overflow("EXEC incidence references"))?;
        if n == 0 {
            return Ok(unit());
        }
    }
    Ok(0)
}
/// No mutation until both primary native write and its index membership are
/// admitted. Metadata changes no journal capacity; it is derived from the row.
pub(super) fn admit_change_v7(
    b: &mut TransitionBuilder<'_>,
    root: Hash64,
    old: Option<Hash64>,
    new: Option<Hash64>,
) -> Result<(), PalwStateV2Error> {
    let bytes = request(&b.state, root, old, new)?;
    if b.native_writers_tracking_v1() {
        b.native_reservation
            .grow_v1(bytes)
            .map_err(PalwStateV2Error::NativeResource)?
    }
    Ok(())
}
/// Infallible after admission; the single threaded native writer cannot change
/// these same counts between admission and this update. Keep old zero IDs too:
/// the historical raw equality predicate never silently filtered them.
pub(super) fn replace_v7(
    s: &mut PalwChainStateV2,
    root: Hash64,
    old: Option<Hash64>,
    new: Option<Hash64>,
) {
    if old == new {
        return;
    }
    if let Some(source) = old {
        let key = (source, root);
        let n = s
            .exec_v2_source_roots
            .get_mut(&key)
            .expect("admitted old EXEC incidence");
        *n -= 1;
        if *n == 0 {
            s.exec_v2_source_roots.remove(&key);
        }
    }
    if let Some(source) = new {
        let n = s.exec_v2_source_roots.entry((source, root)).or_default();
        *n = n.checked_add(1).expect("admitted EXEC incidence count");
    }
}
fn built(s: &PalwChainStateV2) -> BTreeMap<(Hash64, Hash64), u64> {
    let mut index = BTreeMap::new();
    for (root, row) in &s.exec_v2.roots {
        *index.entry((row.prefix_claim, *root)).or_insert(0u64) += 1;
    }
    for ((root, _), row) in &s.exec_v2.slices {
        *index.entry((row.evidence_root, *root)).or_insert(0u64) += 1;
    }
    index
}
fn rebuild_requests(s: &PalwChainStateV2) -> Result<u64, PalwStateV2Error> {
    let n = (s.exec_v2.roots.len() as u64)
        .checked_add(s.exec_v2.slices.len() as u64)
        .ok_or(PalwStateV2Error::Overflow("EXEC incidence rebuild sources"))?;
    n.checked_mul(unit())
        .ok_or(PalwStateV2Error::Overflow("EXEC incidence rebuild bytes"))
}
/// Called at actual cold-carriage restore, before consistency/root checks. The
/// decoded primary maps bound this global operation; no source fee pays it.
pub(super) fn rebuild_v7(s: &mut PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    let _lease = reserve_native_requests_v1(rebuild_requests(s)?)
        .map_err(PalwStateV2Error::NativeResource)?;
    let index = built(s);
    s.exec_v2_source_roots = index;
    Ok(())
}
pub(super) fn assert_consistent_v7(s: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    let _lease = reserve_native_requests_v1(rebuild_requests(s)?)
        .map_err(PalwStateV2Error::NativeResource)?;
    if built(s) != s.exec_v2_source_roots {
        return Err(PalwStateV2Error::CarriageInconsistent(
            "EXEC reverse incidence differs from primary rows".into(),
        ));
    }
    Ok(())
}
/// Exact former source_owns predicate as a scoped concrete set. Include direct
/// source; forward prefix/slices iff roots[source] exists; reverse roots only
/// if the actual primary root exists. Multiple incidences are deduplicated only
/// in this returned set, never erased prematurely from reference counts.
pub(super) fn source_roots_v7(
    s: &PalwChainStateV2,
    source: Hash64,
) -> Result<NativeRetainedV1<BTreeSet<Hash64>>, PalwStateV2Error> {
    let reverse = || reverse_roots_v7(s, source);
    let mut count = 1u64;
    if s.exec_v2.roots.contains_key(&source) {
        count = count
            .checked_add(1)
            .and_then(|n| {
                n.checked_add(
                    s.exec_v2
                        .slices
                        .range((source, 0)..=(source, u32::MAX))
                        .count() as u64,
                )
            })
            .ok_or(PalwStateV2Error::Overflow("EXEC scoped forward count"))?;
    }
    count = count
        .checked_add(reverse().count() as u64)
        .ok_or(PalwStateV2Error::Overflow("EXEC scoped reverse count"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<Hash64, ()>>(count)
        .map_err(PalwStateV2Error::NativeResource)?;
    let lease =
        reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut result = BTreeSet::new();
    result.insert(source);
    if let Some(root) = s.exec_v2.roots.get(&source) {
        result.insert(root.prefix_claim);
        for (_, slice) in s.exec_v2.slices.range((source, 0)..=(source, u32::MAX)) {
            result.insert(slice.evidence_root);
        }
    }
    result.extend(reverse());
    Ok(NativeRetainedV1::from_owner_reserved_v1(result, lease))
}
/// Same primary row codec/expectation check as the original replay, with the
/// derived membership reservation made before either primary/index mutation.
fn swap_indexed<K, V>(
    s: &mut PalwChainStateV2,
    key_raw: &[u8],
    old: &Option<Vec<u8>>,
    new: &Option<Vec<u8>>,
    revert: bool,
    map: fn(&mut PalwChainStateV2) -> &mut BTreeMap<K, V>,
    root_of: fn(K) -> Hash64,
    source_of: fn(&V) -> Hash64,
) -> Result<(), PalwStateV2Error>
where
    K: Copy + Ord + borsh::BorshDeserialize,
    V: PartialEq + borsh::BorshDeserialize,
{
    let decode = |_| bad("an indexed EXEC row's bytes do not decode");
    let key: K = borsh::from_slice(key_raw).map_err(decode)?;
    let (expected, install) = if revert { (new, old) } else { (old, new) };
    let expected: Option<V> = expected
        .as_deref()
        .map(borsh::from_slice)
        .transpose()
        .map_err(decode)?;
    if map(s).get(&key) != expected.as_ref() {
        return Err(bad("indexed EXEC row differs from delta expectation"));
    }
    let install: Option<V> = install
        .as_deref()
        .map(borsh::from_slice)
        .transpose()
        .map_err(decode)?;
    let before = expected.as_ref().map(source_of);
    let after = install.as_ref().map(source_of);
    let root = root_of(key);
    let _lease = reserve_native_requests_v1(request(s, root, before, after)?)
        .map_err(PalwStateV2Error::NativeResource)?;
    match install {
        Some(row) => {
            map(s).insert(key, row);
        }
        None => {
            map(s).remove(&key);
        }
    }
    replace_v7(s, root, before, after);
    Ok(())
}
pub(super) fn apply_root_delta_v7(
    s: &mut PalwChainStateV2,
    key: &[u8],
    old: &Option<Vec<u8>>,
    new: &Option<Vec<u8>>,
    revert: bool,
) -> Result<(), PalwStateV2Error> {
    swap_indexed::<Hash64, PalwWorkRootV2>(
        s,
        key,
        old,
        new,
        revert,
        |s| &mut s.exec_v2.roots,
        |key| key,
        |r| r.prefix_claim,
    )
}
pub(super) fn apply_slice_delta_v7(
    s: &mut PalwChainStateV2,
    key: &[u8],
    old: &Option<Vec<u8>>,
    new: &Option<Vec<u8>>,
    revert: bool,
) -> Result<(), PalwStateV2Error> {
    swap_indexed::<(Hash64, u32), PalwWorkSliceRowV2>(
        s,
        key,
        old,
        new,
        revert,
        |s| &mut s.exec_v2.slices,
        |key| key.0,
        |r| r.evidence_root,
    )
}

/// Allocation-free exact reverse incidence range for covenant singleton checks.
/// The caller MUST NOT expand all contracts containing these roots: the old
/// native rule references only each composite root's own bound covenant.
pub(super) fn reverse_roots_v7<'a>(
    s: &'a PalwChainStateV2,
    source: Hash64,
) -> impl Iterator<Item = Hash64> + 'a {
    s.exec_v2_source_roots
        .range((source, Hash64::default())..=(source, Hash64::from_bytes([255; 64])))
        .filter(move |((_, root), n)| **n != 0 && s.exec_v2.roots.contains_key(root))
        .map(|((_, root), _)| *root)
}
