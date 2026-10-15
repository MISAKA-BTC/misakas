// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeHeapV1, NativeCoverageV1, native_struct, native_tuple_struct, native_enum, native_unit};
native_struct!(crate::palw_prefix_public_source_v1::PalwPrefixPublicPendingV1; claim, owner, reserved_daa, seed_id, cursor_id, demand_id, deadline_daa, source, custodians, attempts, resolved);
native_struct!(crate::palw_prefix_public_source_v1::PalwPrefixPublicSourceV1; coordinate, leaf_index, value_count, byte_length, step_root, step_leaves, maximum_leaves, hash_header);
native_enum!(crate::palw_prefix_public_source_v1::PalwPrefixPublicAttemptV1; Waiting, Receiving { opening, hash, bytes }, Invalid);
