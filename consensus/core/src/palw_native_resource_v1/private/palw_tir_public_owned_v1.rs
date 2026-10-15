// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeHeapV1, NativeCoverageV1, native_struct, native_tuple_struct, native_enum, native_unit};
native_struct!(crate::palw_tir_public_owned_v1::PalwTirPublicOwnedV1; contract, pending, spent, history, work, preparation, prepared_responders, accepted_rows);
native_struct!(crate::palw_tir_public_owned_v1::PalwTirPublicPreparationV1; contract, work, ready, defaulted);
native_struct!(crate::palw_tir_public_owned_v1::History; count, peaks);
native_struct!(crate::palw_tir_public_owned_v1::PalwTirPublicSlotV1; body_hash, total_bytes, accepted_offset, head, finished, hash);
