//! Publish the shape preimage of the already-registered t12 8K genesis class.
//! Existing kernel-court vectors and shape metadata only: no LLM model review or new class registration.
//! The existing ClassLaneCertified wire object can publish a missing profile without changing
//! an existing class certification. Inspect the live class and missing-profile refusal first.
use kaspa_consensus_core::palw_state_v2::{
    PalwCertificationEvidenceV1, PalwCertifiedLaneV1, PalwConsensusObjectV2, palw_object_chunks_v1,
};
use misaka_palw_base0::classes::{A16_GRAPH_V7_8K_MODEL_ID, canonical_class_by_model_id_v1};
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).ok_or("usage: publish-t12-fp-profile <new object.borsh> [--kernel-family]")?;
    let court = misaka_palw_base0::fp_worker::fp_worker_court_params_v1("testnet-12")?;
    let row = canonical_class_by_model_id_v1(&court, A16_GRAPH_V7_8K_MODEL_ID).ok_or("missing t12 8K catalog row")?;
    let class_id = row.profile.shape_profile_id();
    if class_id.to_string()
        != "ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01"
    {
        return Err("the catalog does not match the public t12 8K identity".into());
    }
    let family_mode = std::env::args().any(|arg| arg == "--kernel-family");
    let object = if family_mode {
        // Public integer-kernel court vectors on tiny fixtures; no LLM weights or model review.
        let family = misaka_palw_base0::e2e_drill::covering_rc_family_v1(&row.profile, PalwCertifiedLaneV1::FreePrompt)
            .ok_or("this build has no kernel family for the fixed genesis profile")?;
        let evidence = PalwCertificationEvidenceV1::FreePrompt(
            misaka_palw_base0::e2e_drill::rc_free_prompt_evidence_v1(family).map_err(|e| format!("{e:?}"))?,
        );
        let graded = evidence.grade().map_err(|e| format!("{e:?}"))?;
        eprintln!(
            "kernel family {}: {} vectors, {} kernels, digest {}",
            family.name(),
            evidence.vector_count(),
            graded.kernel_ids.len(),
            graded.digest()
        );
        PalwConsensusObjectV2::FamilyCertified { evidence: Box::new(evidence) }
    } else {
        PalwConsensusObjectV2::ClassLaneCertified { class_id, lane: PalwCertifiedLaneV1::FreePrompt, profile: Box::new(row.profile) }
    };
    if let Some(chunks) = palw_object_chunks_v1(&object)? {
        for (index, chunk) in chunks.iter().enumerate() {
            let name = format!("{output}.chunk{index}");
            std::fs::OpenOptions::new().write(true).create_new(true).open(name)?.write_all(&borsh::to_vec(chunk)?)?;
        }
        eprintln!("{} carrier chunks", chunks.len());
    }
    let bytes = borsh::to_vec(&object)?;
    std::fs::OpenOptions::new().write(true).create_new(true).open(&output)?.write_all(&bytes)?;
    println!(
        "{}",
        serde_json::json!({"class_id":class_id.to_string(),"object":output,"bytes":bytes.len(),"purpose": if family_mode {"publish existing integer-kernel court vectors"} else {"publish the missing shape preimage of an existing genesis class"}})
    );
    Ok(())
}
