// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeHeapV1, NativeCoverageV1, native_struct, native_tuple_struct, native_enum, native_unit};
native_struct!(crate::palw_prefix_family_court_v1::PalwPrefixFamilyCourtV1; context, claim, binding, job, semantic, named_root, prefix_tokens, maximum_leaves, maximum_working_set_bytes, family);
native_struct!(crate::palw_prefix_family_court_v1::PalwPrefixFamilyCheckpointV1; context, claim, execution_root, family);
native_enum!(crate::palw_prefix_family_court_v1::FamilyV1; Dense(f0), Base0 { digest, pending }, Q36 { digest, head, public });
native_enum!(crate::palw_prefix_family_court_v1::CheckpointFamilyV1; Dense(f0), Base0 { digest, pending }, Q36 { digest, head, public });
native_struct!(crate::palw_prefix_family_court_v1::Base0PendingTileV1; coordinate, length, offset, expected, hash);
native_enum!(crate::palw_prefix_family_court_v1::Q36HeadV1; Whole { replay, contract, input }, Paged { contract, state, input });
native_struct!(crate::palw_prefix_family_court_v1::PublicInputsV1; unit, offset, length, sources, source, bytes, stream);
native_struct!(crate::palw_prefix_family_court_v1::HeadInputsV1; rows, stream);
