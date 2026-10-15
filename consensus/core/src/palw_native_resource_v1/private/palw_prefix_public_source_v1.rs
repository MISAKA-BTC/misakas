// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeHeapV1, NativeCoverageV1, native_struct, native_tuple_struct, native_enum, native_unit};
native_struct!(crate::palw_prefix_public_source_v1::PalwPrefixPublicPendingV1; claim, owner, reserved_daa, seed_id, cursor_id, demand_id, deadline_daa, source, custodians, attempts, resolved);
native_struct!(crate::palw_prefix_public_source_v1::PalwPrefixPublicSourceV1; coordinate, leaf_index, value_count, byte_length, step_root, step_leaves, maximum_leaves, hash_header);
native_enum!(crate::palw_prefix_public_source_v1::PalwPrefixPublicAttemptV1; Waiting, Receiving { opening, hash, bytes }, Invalid);

use super::*;
use crate::palw_native_resource_v1::wire::wire_struct;
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1};
impl NativeWireScanV1 for crate::palw_prefix_public_source_v1::PalwPrefixPublicAttemptV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_step_leg::PalwStepOpeningV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(
                    <crate::palw_rendering_stream_v1::LeafHashContinuationV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?,
                )?;
                sum = sum.merge(<Vec<u8> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_prefix_public_source_v1::PalwPrefixPublicPendingV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::Hash64, crate::Hash64, u64, crate::palw_prefix_public_source_v1::PalwPrefixPublicSourceV1, BTreeSet<crate::palw_state_v2::PalwBondKeyV2>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_prefix_public_source_v1::PalwPrefixPublicAttemptV1>, Option<(crate::palw_step_leg::PalwStepOpeningV1, Vec<u8>)>);
