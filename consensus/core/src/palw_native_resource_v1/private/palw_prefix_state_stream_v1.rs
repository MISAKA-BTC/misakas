// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeHeapV1, NativeCoverageV1, native_struct, native_tuple_struct, native_enum, native_unit};
native_struct!(crate::palw_prefix_state_stream_v1::PalwPrefixStateStreamV1; context, binding, prefix_tokens, total_tiles, next_tile, value_bytes, maximum_leaves, digest, pending);
native_struct!(crate::palw_prefix_state_stream_v1::PalwPrefixStreamCheckpointV1; context, next_tile, value_bytes, digest, pending);
native_struct!(crate::palw_prefix_state_stream_v1::PalwPrefixTileStreamV1; length, offset, expected, hash);
