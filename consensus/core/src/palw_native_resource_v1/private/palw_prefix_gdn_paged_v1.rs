// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeHeapV1, NativeCoverageV1, native_struct, native_tuple_struct, native_enum, native_unit};
native_struct!(crate::palw_prefix_gdn_paged_v1::PalwGdnPagedStateV1; context, prefix_tokens, position, d_k, d_v, pages_per_row, depth, root, parameters, phase, row, page, accumulator, delta, decay, beta, key, value);
native_enum!(crate::palw_prefix_gdn_paged_v1::PhaseV1; Idle, Read, Write);

use super::*;
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeWireScanV1,NativeWireReaderV1,NativeWireShapeV1};
use crate::palw_native_resource_v1::wire::wire_struct;
impl NativeWireScanV1 for crate::palw_prefix_gdn_paged_v1::PhaseV1 {fn scan_native_wire_v1<R:std::io::Read>(reader:&mut NativeWireReaderV1<R>)->std::io::Result<NativeWireShapeV1> {reader.step()?;match reader.byte()? {0 => {let mut sum=NativeWireShapeV1::default();Ok(sum)},1 => {let mut sum=NativeWireShapeV1::default();Ok(sum)},2 => {let mut sum=NativeWireShapeV1::default();Ok(sum)},_=>Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant"))}}}
