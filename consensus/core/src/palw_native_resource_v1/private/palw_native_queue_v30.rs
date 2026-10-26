// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, native_enum, native_struct, native_tuple_struct, native_unit};
native_struct!(crate::palw_native_queue_v30::NativeQueueGlobalV30; version, policy, service, live, packets, owned_bytes, entries);
native_struct!(crate::palw_native_queue_v30::QueueEntryV30; job, funding, lease, packets, owned_bytes);

use super::*;
use crate::palw_native_resource_v1::wire::wire_struct;
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1};
wire_struct!(crate::palw_native_queue_v30::NativeQueueGlobalV30; u16, [u8;64], [u8;64], u32, u32, u64, Vec<crate::palw_native_queue_v30::QueueEntryV30>);
wire_struct!(crate::palw_native_queue_v30::QueueEntryV30; [u8;64], [u8;64], [u8;64], u32, u64);
