// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeHeapV1, NativeCoverageV1, native_struct, native_tuple_struct, native_enum, native_unit};
native_struct!(crate::palw_prefix_source_stream_v1::PalwPrefixSourceStreamV1; context, execution_root, source, next_tile, values, pending);
native_struct!(crate::palw_prefix_source_stream_v1::PendingV1; length, offset, expected, hash);

use super::*;
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeWireScanV1,NativeWireReaderV1,NativeWireShapeV1};
use crate::palw_native_resource_v1::wire::wire_struct;
wire_struct!(crate::palw_prefix_source_stream_v1::PendingV1; u32, u32, crate::Hash64, crate::palw_rendering_stream_v1::LeafHashContinuationV1);
