//! **The executor as a node runs a class** (feature `node`; RFC-0002 Phase F step F9): a
//! PALWTIR1 artifact mapped in place or held within a budget ([`artifact`], [`residency`]), and a
//! job executed into the step leg and the roots Phase F defines ([`run`]). The consensus types
//! these are stated in live in `kaspa-consensus-core`; the executor itself does not need them, so
//! this layer is a feature.

pub mod annex;
pub mod artifact;
pub mod backend;
pub mod cell;
pub mod shardrows;
pub mod drill;
pub mod evidence;
pub mod inventory;
pub mod mapped;
pub mod residency;
pub mod run;
pub mod tree;

pub use annex::{
    PALW_TIR_LEAF_ANNEX_MAGIC_V1, PALW_TIR_LEAF_ANNEX_VERSION_V1, PalwTirAnnexTraceV1, PalwTirLeafAnnexV1, TirDivergenceV1,
    palw_tir_leaf_annex_verify_v1, tir_annex_trace_v1, tir_first_divergence_from_opening_v1,
};
pub use artifact::TirArtifactV1;
pub use backend::{
    set_tir_drill_boundary_lie_v1,
    TIR_CAPTURE_MAGIC_V1, TIR_FREE_PROMPT_CLOSED_V1, TirBackendV1, TirCaptureV1, set_tir_fused_kernels_default_v1,
    tir_dissect_choice_v1, tir_fused_kernels_default_v1, tir_trace_event_disclosure_of_capture_v1,
};
pub use cell::{
    CpuKernelBackendV1, DeviceKernelBackendV1, KernelBackendV1, KernelRefusedV1, TirCaptureInputsV1, TirCellInputsV1, TirCellRequestV1,
    TirCellV1, TirCellVerdictV1, TirRunInputsV1, cell_runs_v1, register_device_v1, tir_kernel_backend_registered_v1, tir_kernel_backend_v1,
    tir_shard_cells_over_v1, tir_shard_cells_v1, tir_shard_geometry_over_v1, tir_shard_geometry_v1, tir_verify_capture_cells_over_v1, tir_shard_weight_bytes_v1, tir_verify_capture_cells_v1, tokens_of_capture_v1,
    verify_cell_stepping_v1, verify_cell_v1,
};
pub use crate::cellstep::{CpuCellStepperV1, TirCellStepperV1, TirDeviceV1, buf_lanes_le_v1};
pub use shardrows::{
    TirFileMirrorV1, TirHeldRowsV1, TirRowFetcherV1, TirRowPursuitV1, TirShardHoldingV1, fetch_shard_params_v1, serve_rows_v1,
};
pub use drill::{
    TirCloseSizeV1, TirDrillCallV1, TirDrillUnitKindV1, TirDrillUnitV1, TirFamilyCertificateV1, tir_drill_covering_leaves_v1,
    tir_family_drill_v1, tir_family_evidence_v1, tir_family_id_v1, tir_prim_kernel_id_v1, tir_terminal_close_sizes_v1,
};
pub use evidence::{
    TirEvidenceV1, TirRetainedJobV1, TirTraceV1, tir_bisect_prefix_state_v1, tir_first_divergence_v1, tir_row_tile_leaves_v1,
    tir_rows_tree_v1,
};
pub use inventory::{
    TirByteSourceV1, TirHeldInventoryV1, TirInventoryTreeV1, TirParamOpenerV1, TirParamsSourceV1, TirWholeInstancesV1,
};
pub use residency::{
    TirHeldBytesV1, TirResidencyDeclinedV1, TirResidencyPolicyV1, TirResidencyStatsV1, TirStoreOpenV1, TirWeightStoreV1,
    tir_stream_leaves_v1, tir_weight_store_for_root_v1,
};
pub use run::{TirBoundaryLieV1, TirClassRunnerV1, TirJobRunV1, TirLeafOutV1, TirRangeRunV1, TirResumePointV1};
pub use tree::TirStepTreeV1;
