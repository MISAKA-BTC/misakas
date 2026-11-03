//! Focused real G14 geometry/source engine tests without the full core unit harness.
use kaspa_consensus_core::{
    Hash64,
    palw_positive_source_geometry_v2::*,
    palw_prompt_ids_v1::{PALW_PROMPT_IDS_OPENING_HEADER_BYTES, PalwPromptIdsFormV1},
    palw_tir_admission_v1::PalwTirClassRecordV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1},
    palw_v2::{PALW_V2_MAX_NETWORK_ID_BYTES, PalwJobContextV2},
};
use misaka_palw_positive_contract::{
    identity::Digest,
    messages::{MinerResultCommit, SourceKind, SourceRequest},
};
use misaka_palw_tir::TirProgramV1;

#[path = "../src/palw_positive_source_geometry_v2/tests.rs"]
mod geometry;
