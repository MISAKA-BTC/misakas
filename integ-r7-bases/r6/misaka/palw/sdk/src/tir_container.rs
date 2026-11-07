//! **RFC-0002 / genesis 8 audit 02-A, SDK half: a TIR class's `PALWART3` container binding** (tag 175, dormant behind
//! `palw_tir_container_identity_v1`).
//!
//! The chain checks a binding with [`kaspa_consensus_core::palw_tir_container_v1::check_tir_container_binding_v1`]; this module
//! builds one from a container's header — the opening that recomputes `palwart_root_v3` without a weight byte — and the signed
//! object the class's registrant carries. Nothing here enters a verdict on its own: the fold recomputes everything it binds.
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::palw_tir_container_v1::{
    PALW_TIR_CONTAINER_BINDING_VERSION_V1, PalwArtRootOpeningV1, PalwArtSectionOpeningV1, PalwTirContainerBindingV1,
};
use misaka_palw_tir_artifact::palwart_v3::{PalwArtHeaderV3, palwart_entries_root_v3, palwart_root_v3};

/// **The root opening of a `PALWART3` header**: every field `palwart_root_v3` hashes, the manifest and each entry table as
/// their digests, the program whole.
pub fn palwart_root_opening_v1(header: &PalwArtHeaderV3) -> PalwArtRootOpeningV1 {
    PalwArtRootOpeningV1 {
        version: header.version,
        manifest_digest: PalwArtRootOpeningV1::manifest_digest_of_v1(&header.manifest),
        program_kind: header.program.kind,
        program: header.program.bytes.clone(),
        kernel_id: Hash64::from_bytes(header.program.kernel_id),
        kernel_version: header.program.kernel_version,
        tokenizer_id: Hash64::from_bytes(header.program.tokenizer_id),
        sections: header
            .sections
            .iter()
            .map(|s| PalwArtSectionOpeningV1 {
                kind: s.kind,
                family: s.family,
                name: s.name.clone(),
                inner_root_scheme: s.inner_root_scheme,
                inner_root: Hash64::from_bytes(s.inner_root),
                inner_leaf_count: s.inner_leaf_count,
                entries_root: Hash64::from_bytes(palwart_entries_root_v3(&s.entries)),
            })
            .collect(),
    }
}

/// **The binding of `class_id` to the container `header` describes** (unsigned: the registrant signs
/// [`kaspa_consensus_core::palw_tir_container_v1::palw_tir_container_binding_message_v1`] of it). Refuses an opening that does
/// not recompute the header's own root — a header and its twin must never disagree silently.
pub fn tir_container_binding_v1(header: &PalwArtHeaderV3, class_id: Hash64) -> Result<PalwTirContainerBindingV1, String> {
    let opening = palwart_root_opening_v1(header);
    let container_root = Hash64::from_bytes(palwart_root_v3(header));
    if opening.root_v1() != container_root {
        return Err("the consensus opening does not recompute this PALWART3 root (twin drift)".into());
    }
    Ok(PalwTirContainerBindingV1 { version: PALW_TIR_CONTAINER_BINDING_VERSION_V1, class_id, container_root, opening })
}

/// The tag-175 object carrying a signed binding.
pub fn tir_container_binding_object_v1(binding: PalwTirContainerBindingV1, signature: Vec<u8>) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::TirClassContainerBoundV1 { binding: Box::new(binding), signature }
}
