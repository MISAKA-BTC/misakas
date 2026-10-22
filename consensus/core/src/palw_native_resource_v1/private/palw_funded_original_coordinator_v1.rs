// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeHeapV1, NativeCoverageV1, native_struct, native_tuple_struct, native_enum, native_unit};
native_struct!(crate::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1; carrier, envelope, claim, outpoint);
native_struct!(crate::palw_funded_original_coordinator_v1::PalwFundedOriginalScheduleV1; fee_profile_id, capital_profile_id, retention_profile_id, settlement_policy_id, fee_sompi, capital_sompi, numerical, input, source, adopted_roles);
native_struct!(crate::palw_funded_original_coordinator_v1::SourceBudgetV1; max_queries, max_roles, max_vertices, max_edges);

use super::*;
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeWireScanV1,NativeWireReaderV1,NativeWireShapeV1};
use crate::palw_native_resource_v1::wire::wire_struct;
wire_struct!(crate::palw_funded_original_coordinator_v1::PalwFundedOriginalScheduleV1; crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, u64, u64, crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3, crate::palw_tir_public_parameter_cover_v1::OriginalParameterCoverBudgetV1, crate::palw_funded_original_coordinator_v1::SourceBudgetV1, u8);
wire_struct!(crate::palw_funded_original_coordinator_v1::SourceBudgetV1; u64, u64, u64, u64);
