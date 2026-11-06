// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, native_enum, native_struct, native_tuple_struct, native_unit};
native_struct!(crate::palw_native_resource_v1::demand::NativeDemandLeaseV1; checker, memory, database, process);
native_struct!(crate::palw_native_resource_v1::demand::NativeDemandCheckerV1; limits, state);
native_struct!(crate::palw_native_resource_v1::demand::OutstandingV1; memory, database);

// Reflected owner-local layouts may require parent-private names; this import
// is intentionally conditional on that layout, like the visitor imports below.
#[allow(unused_imports)]
use super::*;
use crate::palw_native_resource_v1::wire::wire_struct;
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1};
wire_struct!(crate::palw_native_resource_v1::demand::NativeDemandCheckerV1; crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1, Mutex<crate::palw_native_resource_v1::demand::OutstandingV1>);
wire_struct!(crate::palw_native_resource_v1::demand::OutstandingV1; u64, u64);
