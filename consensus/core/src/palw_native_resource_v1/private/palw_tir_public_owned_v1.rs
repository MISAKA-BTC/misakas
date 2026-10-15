// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeHeapV1, NativeCoverageV1, native_struct, native_tuple_struct, native_enum, native_unit};
native_struct!(crate::palw_tir_public_owned_v1::PalwTirPublicOwnedV1; contract, pending, spent, history, work, preparation, prepared_responders, accepted_rows);
native_struct!(crate::palw_tir_public_owned_v1::PalwTirPublicPreparationV1; contract, work, ready, defaulted);
native_struct!(crate::palw_tir_public_owned_v1::History; count, peaks);
native_struct!(crate::palw_tir_public_owned_v1::PalwTirPublicSlotV1; body_hash, total_bytes, accepted_offset, head, finished, hash);

use super::*;
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeWireScanV1,NativeWireReaderV1,NativeWireShapeV1};
use crate::palw_native_resource_v1::wire::wire_struct;
wire_struct!(crate::palw_tir_public_owned_v1::History; u64, [Option<crate::Hash64>; 64]);
wire_struct!(crate::palw_tir_public_owned_v1::PalwTirPublicOwnedV1; crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_tir_public_owned_v1::PalwTirPublicContractV1>, Option<crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_tir_public_owned_v1::PalwTirPublicPendingV1>>, crate::palw_tir_public_owned_v1::PalwTirPublicSpentV1, crate::palw_tir_public_owned_v1::History, Option<crate::palw_tir_public_owned_v1::PalwTirPublicWorkStateV1>, Option<crate::palw_tir_public_owned_v1::PalwTirPublicPreparationReferenceV1>, Option<BTreeSet<crate::palw_state_v2::PalwBondKeyV2>>, Option<crate::palw_tir_public_owned_v1::PalwTirPublicAcceptedRowsV1>);
