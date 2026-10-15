//! Native allocation requests, visited by type rather than inferred from codec bytes.
//! This is a pinned Rust BTree allocation envelope, not physical RSS approval.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NativeCoverageV1 {
    Bounded { bytes: u64 },
    Unsupported { component: String, native_type: String, reason: String },
    Overflow { component: String, native_type: String },
}
impl NativeCoverageV1 {
    pub fn zero() -> Self {
        Self::Bounded { bytes: 0 }
    }
    pub fn unsupported<T: ?Sized>(component: &str, reason: &str) -> Self {
        Self::Unsupported { component: component.into(), native_type: std::any::type_name::<T>().into(), reason: reason.into() }
    }
    pub fn overflow<T: ?Sized>(component: &str) -> Self {
        Self::Overflow { component: component.into(), native_type: std::any::type_name::<T>().into() }
    }
    pub fn bytes(&self) -> Option<u64> {
        if let Self::Bounded { bytes } = self { Some(*bytes) } else { None }
    }
    pub fn add<T: ?Sized>(self, other: Self, component: &str) -> Self {
        match (self, other) {
            (Self::Bounded { bytes: a }, Self::Bounded { bytes: b }) => {
                a.checked_add(b).map(|bytes| Self::Bounded { bytes }).unwrap_or_else(|| Self::overflow::<T>(component))
            }
            (problem @ Self::Overflow { .. }, _) | (_, problem @ Self::Overflow { .. }) => problem,
            (problem @ Self::Unsupported { .. }, _) | (_, problem @ Self::Unsupported { .. }) => problem,
        }
    }
    pub fn scale<T: ?Sized>(self, copies: u64, component: &str) -> Self {
        match self {
            Self::Bounded { bytes } => {
                bytes.checked_mul(copies).map(|bytes| Self::Bounded { bytes }).unwrap_or_else(|| Self::overflow::<T>(component))
            }
            problem => problem,
        }
    }
}
impl std::fmt::Display for NativeCoverageV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bounded { bytes } => write!(f, "bounded {bytes} bytes"),
            Self::Unsupported { component, native_type, reason } => write!(f, "unsupported {component} ({native_type}): {reason}"),
            Self::Overflow { component, native_type } => write!(f, "overflow {component} ({native_type})"),
        }
    }
}
impl std::error::Error for NativeCoverageV1 {}
pub trait NativeHeapV1 {
    /// Additional heap beyond `size_of::<Self>()`, including capacity and nested ownership.
    fn native_heap_v1(&self, component: &str) -> NativeCoverageV1;
    fn native_entries_v1(&self) -> u64 {
        1
    }
    /// Actual nested typed memberships, excluding byte-vector elements. This
    /// cardinality bounds restoration index fan-out; it is not encoded length.
    fn native_members_v1(&self) -> Option<u64> {
        Some(0)
    }
    fn native_components_v1(&self) -> Vec<NativeFieldInventoryV1> {
        Vec::new()
    }
}
fn members_v1<'a, T: NativeHeapV1 + 'a>(mut items: impl Iterator<Item = &'a T>) -> Option<u64> {
    items.try_fold(0u64, |n, item| n.checked_add(item.native_members_v1()?))
}
fn allocation<T>(count: usize, component: &str) -> NativeCoverageV1 {
    u64::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(size_of::<T>() as u64))
        .map(|bytes| NativeCoverageV1::Bounded { bytes })
        .unwrap_or_else(|| NativeCoverageV1::overflow::<T>(component))
}
macro_rules! fixed_native { ($($ty:ty),* $(,)?) => {$ (
    impl NativeHeapV1 for $ty { fn native_heap_v1(&self, _: &str) -> NativeCoverageV1 { NativeCoverageV1::zero() } }
)*}; }
fixed_native!(
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    bool,
    f32,
    f64,
    (),
    crate::Hash64,
    kaspa_hashes::Hash,
    kaspa_math::Uint512
);
impl<T: NativeHeapV1, const N: usize> NativeHeapV1 for [T; N] {
    fn native_members_v1(&self) -> Option<u64> {
        if std::any::type_name::<T>() == "u8" { Some(0) } else { (N as u64).checked_add(members_v1(self.iter())?) }
    }
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        if std::any::type_name::<T>() == "u8" {
            return NativeCoverageV1::zero();
        }
        self.iter().fold(NativeCoverageV1::zero(), |sum, item| sum.add::<Self>(item.native_heap_v1(path), path))
    }
}
impl<T: NativeHeapV1> NativeHeapV1 for Vec<T> {
    fn native_members_v1(&self) -> Option<u64> {
        if std::any::type_name::<T>() == "u8" {
            Some(0)
        } else {
            u64::try_from(self.len()).ok()?.checked_add(members_v1(self.iter())?)
        }
    }
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Some(growth) = self.len().checked_mul(2) else { return NativeCoverageV1::overflow::<Self>(path) };
        // The point retains the current Vec capacity; a serialized cache/restore
        // can additionally use the pinned Borsh geometric growth capacity.
        let inline = allocation::<T>(self.capacity().max(growth), path);
        if std::any::type_name::<T>() == "u8" {
            return inline;
        }
        self.iter().fold(inline, |sum, item| sum.add::<Self>(item.native_heap_v1(path), path))
    }
    fn native_entries_v1(&self) -> u64 {
        self.len() as u64
    }
}
impl NativeHeapV1 for String {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Some(growth) = self.len().checked_mul(2) else { return NativeCoverageV1::overflow::<Self>(path) };
        allocation::<u8>(self.capacity().max(growth), path)
    }
    fn native_entries_v1(&self) -> u64 {
        self.len() as u64
    }
}
impl<T: NativeHeapV1> NativeHeapV1 for Option<T> {
    fn native_members_v1(&self) -> Option<u64> {
        self.as_ref().map_or(Some(0), NativeHeapV1::native_members_v1)
    }
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        self.as_ref().map_or_else(NativeCoverageV1::zero, |value| value.native_heap_v1(path))
    }
    fn native_entries_v1(&self) -> u64 {
        u64::from(self.is_some())
    }
}
impl<T: NativeHeapV1> NativeHeapV1 for Box<T> {
    fn native_members_v1(&self) -> Option<u64> {
        (**self).native_members_v1()
    }
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        allocation::<T>(1, path).add::<Self>((**self).native_heap_v1(path), path)
    }
}
impl<T: NativeHeapV1> NativeHeapV1 for crate::palw_staged_court_state_v1::PalwSharedCourtV1<T> {
    fn native_members_v1(&self) -> Option<u64> {
        self.as_ref().native_members_v1()
    }
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        // An `Arc<T>` newtype (G14): the shared allocation, counted as the Arc it is.
        let header = size_of::<[usize; 2]>() as u64 + std::mem::align_of::<T>() as u64;
        allocation::<T>(1, path)
            .add::<Self>(NativeCoverageV1::Bounded { bytes: header }, path)
            .add::<Self>(self.as_ref().native_heap_v1(path), path)
    }
}
impl<T: NativeHeapV1> NativeHeapV1 for Arc<T> {
    fn native_members_v1(&self) -> Option<u64> {
        (**self).native_members_v1()
    }
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        // Counting a shared Arc's payload more than once is conservative. The
        // concrete Arc allocation includes two reference counters and alignment.
        let header = size_of::<[usize; 2]>() as u64 + std::mem::align_of::<T>() as u64;
        allocation::<T>(1, path)
            .add::<Self>(NativeCoverageV1::Bounded { bytes: header }, path)
            .add::<Self>((**self).native_heap_v1(path), path)
    }
}
/// Rust1.93 BTree nodes have 11 key/value slots and at most12 edges. The typed
/// 16-slot envelope includes parent/index/len metadata and alignment padding.
/// One node per entry bounds sparsity; nested ownership is visited separately.
/// This is allocation-request accounting under the repository's pinned std;
/// allocator arenas, committed pages and physical overhead are separate coverage.
#[allow(dead_code)]
pub(crate) struct BTreeNodeEnvelopeV1<K, V> {
    parent: usize,
    metadata: [usize; 4],
    keys: [K; 16],
    values: [V; 16],
    edges: [usize; 17],
}
impl<K: NativeHeapV1, V: NativeHeapV1> NativeHeapV1 for BTreeMap<K, V> {
    fn native_members_v1(&self) -> Option<u64> {
        self.iter().try_fold(self.len() as u64, |n, (k, v)| n.checked_add(k.native_members_v1()?)?.checked_add(v.native_members_v1()?))
    }
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        if cfg!(not(target_pointer_width = "64")) && !self.is_empty() {
            return NativeCoverageV1::unsupported::<Self>(path, "pinned BTree allocation layout only covers64-bit targets");
        }
        self.iter().fold(allocation::<BTreeNodeEnvelopeV1<K, V>>(self.len(), path), |sum, (key, value)| {
            sum.add::<Self>(key.native_heap_v1(path), path).add::<Self>(value.native_heap_v1(path), path)
        })
    }
    fn native_entries_v1(&self) -> u64 {
        self.len() as u64
    }
}
impl<T: NativeHeapV1> NativeHeapV1 for BTreeSet<T> {
    fn native_members_v1(&self) -> Option<u64> {
        (self.len() as u64).checked_add(members_v1(self.iter())?)
    }
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        if cfg!(not(target_pointer_width = "64")) && !self.is_empty() {
            return NativeCoverageV1::unsupported::<Self>(path, "pinned BTree allocation layout only covers64-bit targets");
        }
        self.iter().fold(allocation::<BTreeNodeEnvelopeV1<T, ()>>(self.len(), path), |sum, key| {
            sum.add::<Self>(key.native_heap_v1(path), path)
        })
    }
    fn native_entries_v1(&self) -> u64 {
        self.len() as u64
    }
}
macro_rules! tuple_native { ($($ty:ident:$idx:tt),+) => {
    impl<$($ty:NativeHeapV1),+> NativeHeapV1 for ($($ty,)+) {
        fn native_members_v1(&self)->Option<u64> {
            let mut n=0u64; $(n=n.checked_add(self.$idx.native_members_v1()?)?;)+ Some(n)
        }
        fn native_heap_v1(&self, path:&str)->NativeCoverageV1 {
            let mut sum=NativeCoverageV1::zero(); $(sum=sum.add::<Self>(self.$idx.native_heap_v1(path),path);)+ sum
        }
    }
}; }
tuple_native!(A:0);
tuple_native!(A:0,B:1);
tuple_native!(A:0,B:1,C:2);
tuple_native!(A:0,B:1,C:2,D:3);
tuple_native!(A:0,B:1,C:2,D:3,E:4);
tuple_native!(A:0,B:1,C:2,D:3,E:4,F:5);

macro_rules! native_struct { ($ty:ty; $($field:ident),* $(,)?) => {
    impl $crate::palw_native_resource_v1::NativeHeapV1 for $ty {
        fn native_heap_v1(&self,path:&str)->$crate::palw_native_resource_v1::NativeCoverageV1 {
            let Self { $($field),* } = self;
            let mut sum=$crate::palw_native_resource_v1::NativeCoverageV1::zero();
            $(sum=sum.add::<Self>($crate::palw_native_resource_v1::NativeHeapV1::native_heap_v1($field,path),path);)* sum
        }
        fn native_members_v1(&self)->Option<u64> {
            let Self { $($field),* } = self;
            #[allow(unused_mut)] let mut n=0u64;
            $(n=n.checked_add($crate::palw_native_resource_v1::NativeHeapV1::native_members_v1($field)?)?;)* Some(n)
        }
        fn native_components_v1(&self)->Vec<$crate::palw_native_resource_v1::NativeFieldInventoryV1> {
            let Self { $($field),* }=self;
            vec![$($crate::palw_native_resource_v1::NativeFieldInventoryV1::of(concat!(stringify!($ty),".",stringify!($field)), $field)),*]
        }
    }
}; }
macro_rules! native_tuple_struct { ($ty:ty; $($field:ident),* $(,)?) => {
    impl $crate::palw_native_resource_v1::NativeHeapV1 for $ty {
        fn native_heap_v1(&self,path:&str)->$crate::palw_native_resource_v1::NativeCoverageV1 {
            let Self($($field),*) = self;
            let mut sum=$crate::palw_native_resource_v1::NativeCoverageV1::zero();
            $(sum=sum.add::<Self>($crate::palw_native_resource_v1::NativeHeapV1::native_heap_v1($field,path),path);)* sum
        }
        fn native_members_v1(&self)->Option<u64> {
            let Self($($field),*) = self;
            #[allow(unused_mut)] let mut n=0u64;
            $(n=n.checked_add($crate::palw_native_resource_v1::NativeHeapV1::native_members_v1($field)?)?;)* Some(n)
        }
    }
}; }
macro_rules! native_unit {
    ($ty:ty) => {
        impl NativeHeapV1 for $ty {
            fn native_heap_v1(&self, _: &str) -> NativeCoverageV1 {
                NativeCoverageV1::zero()
            }
        }
    };
}
macro_rules! native_enum {
    ($ty:ty; $($variant:ident $( { $($named:ident),* } )? $( ( $($tuple:ident),* ) )?),* $(,)?) => {
        impl NativeHeapV1 for $ty {
            fn native_members_v1(&self)->Option<u64> {
                match self { $(Self::$variant $( { $($named),* } )? $( ( $($tuple),* ) )? => {
                    #[allow(unused_mut)] let mut n=0u64;
                    $($(n=n.checked_add($named.native_members_v1()?)?;)*)?
                    $($(n=n.checked_add($tuple.native_members_v1()?)?;)*)?
                    Some(n)
                }),* }
            }
            fn native_heap_v1(&self,path:&str)->NativeCoverageV1 {
                match self { $(Self::$variant $( { $($named),* } )? $( ( $($tuple),* ) )? => {
                    #[allow(unused_mut)] let mut sum=NativeCoverageV1::zero();
                    $($(sum=sum.add::<Self>($named.native_heap_v1(path),path);)*)?
                    $($(sum=sum.add::<Self>($tuple.native_heap_v1(path),path);)*)?
                    sum
                }),* }
            }
        }
    };
}
macro_rules! native_unsupported {
    ($ty:ty) => {
        impl NativeHeapV1 for $ty {
            fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
                NativeCoverageV1::unsupported::<Self>(path, "native layout visitor is unavailable")
            }
        }
    };
}
pub(crate) use {native_enum, native_struct, native_tuple_struct, native_unit, native_unsupported};

impl<T: NativeHeapV1 + ?Sized> NativeHeapV1 for &T {
    fn native_members_v1(&self) -> Option<u64> {
        (**self).native_members_v1()
    }
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        (**self).native_heap_v1(path)
    }
    fn native_entries_v1(&self) -> u64 {
        (**self).native_entries_v1()
    }
}
impl NativeHeapV1 for misaka_palw_panel::PanelCandidatesV1 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let shared_header = size_of::<Vec<misaka_palw_panel::SeatCandidateV1>>() as u64 + size_of::<[usize; 3]>() as u64;
        (**self).native_heap_v1(path).add::<Self>(NativeCoverageV1::Bounded { bytes: shared_header }, path)
    }
    fn native_entries_v1(&self) -> u64 {
        self.len() as u64
    }
}
impl NativeHeapV1 for misaka_palw_panel::PermissionlessPanelStateV1 {
    fn native_members_v1(&self) -> Option<u64> {
        self.native_inventory_parts_v1().native_members_v1()
    }
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        self.native_inventory_parts_v1().native_heap_v1(path)
    }
}
impl NativeHeapV1 for crate::palw_kernel_route_v1::KernelLedgerCacheV1 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        self.native_ledger_ref_v1().map_or_else(NativeCoverageV1::zero, |ledger| {
            ledger.native_heap_v1(path).add::<Self>(
                NativeCoverageV1::Bounded {
                    bytes: (size_of::<crate::palw_native_resource_v1::ReservedKernelLedgerV1>() + size_of::<[usize; 3]>()) as u64,
                },
                path,
            )
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeFieldInventoryV1 {
    pub component: String,
    pub native_type: String,
    pub inline_bytes: u64,
    pub entries: u64,
    pub heap: NativeCoverageV1,
}
impl NativeFieldInventoryV1 {
    pub fn of<T: NativeHeapV1>(component: &str, value: &T) -> Self {
        Self {
            component: component.into(),
            native_type: std::any::type_name::<T>().into(),
            inline_bytes: std::mem::size_of::<T>() as u64,
            entries: value.native_entries_v1(),
            heap: value.native_heap_v1(component),
        }
    }
}

impl NativeHeapV1 for misaka_palw_kernel::opv::OpvStateV1 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        self.native_inventory_parts_v1().native_heap_v1(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vector_capacity_nested_btree_and_arc_aliases_are_visited() {
        let mut bytes = Vec::<u8>::with_capacity(128);
        bytes.extend_from_slice(&[1, 2, 3]);
        assert_eq!(bytes.native_heap_v1("vec").bytes(), Some(128));
        let mut map = BTreeMap::new();
        map.insert(1u64, bytes);
        let heap = map.native_heap_v1("map").bytes().unwrap();
        assert!(heap >= 128 + size_of::<BTreeNodeEnvelopeV1<u64, Vec<u8>>>() as u64);
        let a = Arc::new(map);
        let b = Arc::clone(&a);
        assert_eq!((a.clone(), b).native_heap_v1("aliases").bytes(), a.native_heap_v1("one").bytes().and_then(|n| n.checked_mul(2)));
        assert!(matches!(NativeCoverageV1::Bounded { bytes: u64::MAX }.scale::<u8>(2, "overflow"), NativeCoverageV1::Overflow { .. }));
    }
    #[test]
    fn state_inventory_contains_private_restore_fields_and_lifetime_types() {
        let state = crate::palw_state_v2::PalwChainStateV2::genesis();
        let fields = state.native_components_v1();
        for expected in [".staged_due", ".deadlines", ".capacity_weight_index"] {
            assert!(fields.iter().any(|field| field.component.ends_with(expected)), "{expected}");
        }
        assert!(state.native_heap_v1("state").bytes().is_some());
    }
}

// The owner returns only borrowed references and exhaustively destructures all
// private fields; extending that record requires a matching inventory bridge.
impl NativeHeapV1 for misaka_palw_kernel::flat_quota11::stage14::source26::StageSourcePlanRecordV26 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        self.native_inventory_parts_v1().native_heap_v1(path)
    }
}
