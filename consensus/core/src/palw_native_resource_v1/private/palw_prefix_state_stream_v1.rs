// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeHeapV1, NativeCoverageV1, native_struct, native_tuple_struct, native_enum, native_unit};
native_struct!(crate::palw_prefix_state_stream_v1::PalwPrefixStateStreamV1; context, binding, prefix_tokens, total_tiles, next_tile, value_bytes, maximum_leaves, digest, pending);
native_struct!(crate::palw_prefix_state_stream_v1::PalwPrefixStreamCheckpointV1; context, next_tile, value_bytes, digest, pending);
native_struct!(crate::palw_prefix_state_stream_v1::PalwPrefixTileStreamV1; length, offset, expected, hash);

use super::*;
use crate::palw_native_resource_v1::wire::wire_struct;
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1};
wire_struct!(crate::palw_prefix_state_stream_v1::PalwPrefixStateStreamV1; crate::Hash64, crate::palw_step_leg::PalwStepBindingV2, u32, u64, u64, u64, u64, crate::palw_rendering_stream_v1::LeafHashContinuationV1, Option<crate::palw_prefix_state_stream_v1::PalwPrefixTileStreamV1>);
wire_struct!(crate::palw_prefix_state_stream_v1::PalwPrefixStreamCheckpointV1; crate::Hash64, u64, u64, crate::palw_rendering_stream_v1::LeafHashContinuationV1, Option<crate::palw_prefix_state_stream_v1::PalwPrefixTileStreamV1>);
wire_struct!(crate::palw_prefix_state_stream_v1::PalwPrefixTileStreamV1; u32, u32, crate::Hash64, crate::palw_rendering_stream_v1::LeafHashContinuationV1);
