// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, native_enum, native_struct, native_tuple_struct, native_unit};
native_struct!(crate::palw_float_constraint_source_v1::PalwFloatConstraintSourceSessionV1; claim, owner, reserved_daa, bundle_id, source_id, respond_until_daa, collect_until_daa, nomination_until_daa, producer, custodians, binding_root, job_context, expected, prefill, decode, vocab, slots, nomination, authenticated, ready, collected);
native_struct!(crate::palw_float_constraint_source_v1::PalwFloatSourceSlotV1; ids, row, facts);

use super::*;
use crate::palw_native_resource_v1::wire::wire_struct;
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1};
wire_struct!(crate::palw_float_constraint_source_v1::PalwFloatConstraintSourceSessionV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::Hash64, u64, u64, u64, crate::palw_state_v2::PalwBondKeyV2, BTreeSet<crate::palw_state_v2::PalwBondKeyV2>, crate::Hash64, crate::Hash64, crate::Hash64, u32, u32, u32, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_float_constraint_source_v1::PalwFloatSourceSlotV1>, Option<(u32, Option<u32>)>, Option<crate::palw_state_v2::PalwBondKeyV2>, Option<crate::palw_state_v2::PalwBondKeyV2>, bool);
wire_struct!(crate::palw_float_constraint_source_v1::PalwFloatSourceSlotV1; Option<crate::palw_step_refute::PalwDecodeTokensV1>, Option<crate::palw_float_decode_v1::PalwFloatRowStreamV1>, Option<crate::palw_float_decode_v1::PalwFloatRowFactsV1>);
