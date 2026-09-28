//! **The executor as a node runs a class** (feature `node`; RFC-0002 Phase F step F9): a
//! PALWTIR1 artifact mapped in place ([`artifact`]), and a job executed into the step leg and the
//! roots Phase F defines ([`run`]). The consensus types these are stated in live in
//! `kaspa-consensus-core`; the executor itself does not need them, so this layer is a feature.

pub mod artifact;
pub mod backend;
pub mod drill;
pub mod evidence;
pub mod inventory;
pub mod mapped;
pub mod run;
pub mod tree;

pub use artifact::TirArtifactV1;
pub use backend::{TIR_CAPTURE_MAGIC_V1, TirBackendV1, TirCaptureV1};
pub use drill::{
    TirDrillCallV1, TirDrillUnitKindV1, TirDrillUnitV1, TirFamilyCertificateV1, tir_family_drill_v1, tir_prim_kernel_id_v1,
};
pub use evidence::{TirEvidenceV1, TirRetainedJobV1, tir_bisect_prefix_state_v1, tir_first_divergence_v1};
pub use inventory::{TirHeldInventoryV1, TirInventoryTreeV1, TirParamOpenerV1, TirParamsSourceV1};
pub use run::{TirClassRunnerV1, TirJobRunV1, TirLeafOutV1, TirRangeRunV1, TirResumePointV1};
pub use tree::TirStepTreeV1;
