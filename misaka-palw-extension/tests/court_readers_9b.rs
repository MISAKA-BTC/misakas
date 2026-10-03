//! **The manifest verifier, the SDK preflight and the chain gate agree for a Qwen3.5 hybrid at n_ctx 16, canonical job [1, 2], on
//! testnet-12 (DAA 0, 3,752, 5,300)** — the class of the 2026-10-03 refusal ("needs 5,102 DAA, window 3,000"). The manifest verifier
//! (`misaka model add --manifest`, `palw extension preflight`) asked `verify_class_admission_v6(…, false)` directly — no held regime — and
//! charged the ladder clock; it and the SDK now call ONE probe (`palw_admission_probe_v1`) with the shape's court, ladder and held reading.
//!
//! The verdict is asked past the court window too: the test prints the gate's refusal for the class (the NEXT wall, with its numbers) and
//! asserts every reader names the same one.

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_base0_profile::rc_job_context;
use kaspa_consensus_core::palw_class_admission_v2::{palw_admission_probe_v1, palw_admission_shape_at_v1, PalwAdmissionProbeRefusalV1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_profile_v7};
use kaspa_hashes::Hash64;
use misaka_palw_extension::{PalwExtensionClassificationV1 as C, PalwExtensionDepthV1 as D, PalwExtensionEnvV1, verify_extension_v1};
use misaka_palw_sdk::{PalwClassEntryV1, PalwClassSdk};

#[test]
fn the_manifest_verifier_the_sdk_and_the_gate_name_the_same_verdict_for_the_9b_hybrid_on_testnet12() {
    let t12 = NetworkId::with_suffix(NetworkType::Testnet, 12);
    let params: Params = t12.into();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("testnet-12 ships a bundle") };
    let profile = qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 16, ..QWEN36_35B_A3B })).expect("the hybrid row");
    let class_id = profile.shape_profile_id();
    let root = Hash64::from_u64_word(0x9B);
    let canonical = rc_job_context(&profile, 1, 2);
    let sdk = PalwClassSdk::builtin_v1(bundle.court, params.palw_prompt_ids_form_v1(), params.net.to_string().into_bytes());
    let entry = PalwClassEntryV1 { model_id: "Huihui-Qwen3.5-9B", lineage_id: "test", profile: profile.clone(), canonical_job: (1, 2), needs_artifact_file: true };
    let hex = {
        let bytes = borsh::to_vec(&profile).expect("borsh");
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    let manifest = format!(
        r#"{{
  "manifest": "misaka-palw/extension-manifest/v1",
  "kind": "model-class",
  "name": "Huihui-Qwen3.5-9B shaped hybrid, n_ctx 16",
  "network": "testnet-12",
  "source": {{ "note": "a Qwen3.5 hybrid profile" }},
  "artifact": {{ "root": "{root}" }},
  "declares": {{ "object_id": "{class_id}", "capabilities": ["attempt"] }},
  "verification": {{ "profile_borsh_hex": "{hex}", "canonical_job": [1, 2], "n_ctx": 16 }},
  "admission": {{ "object": "ClassRegistered" }}
}}"#
    );
    let dir = tempfile::tempdir().expect("tmp");
    let mut seen = Vec::new();
    for daa in [0u64, 3_752, 5_300] {
        // the chain gate (the shared probe, at the shape the acceptance path reads)
        let shape = palw_admission_shape_at_v1(&params, bundle, &profile, daa).expect("a shape");
        let gate = palw_admission_probe_v1(bundle, &profile, &canonical, root, &[], &shape);
        let gate_code = match &gate {
            Ok(_) => "ADMITTED".to_string(),
            Err(PalwAdmissionProbeRefusalV1::Gate(e)) | Err(PalwAdmissionProbeRefusalV1::Express(e)) => e.code().to_string(),
            Err(PalwAdmissionProbeRefusalV1::Price(m)) => format!("PRICE: {m}"),
        };
        // the SDK's pre-signing gate
        let sdk_v = sdk.preflight_admission(bundle, &entry, root, &shape);
        // the manifest verifier, with the env at that height
        let env = PalwExtensionEnvV1 { network_id: t12, daa_score: Some(daa), chain_terms: None };
        let report = verify_extension_v1(manifest.as_bytes(), dir.path(), &env, D::Vectors).expect("no boundary refusal");
        let manifest_says = format!("{:?}", report.classification);
        println!("daa {daa:>5}: gate {gate_code} | sdk {} | manifest {}", sdk_v.as_ref().map(|_| "ok".to_string()).unwrap_or_else(|e| e.chars().take(200).collect()), manifest_says.chars().take(300).collect::<String>());
        if let Err(PalwAdmissionProbeRefusalV1::Gate(e)) = &gate {
            let decided = kaspa_consensus_core::palw_refusal_v1::palw_refusal_decided_by_v1(shape.held.armed, Some(daa), params.palw_held_context.map(|f| f.daa_score()));
            println!("          structured: {}", e.refusal_v1(&decided).to_json());
        }
        // No reader charges the ladder clock.
        assert!(!manifest_says.contains("5102") && !manifest_says.contains("5,102"), "daa {daa}: the manifest verifier charged the ladder clock: {manifest_says}");
        assert!(gate_code == "ADMITTED", "daa {daa}: the gate (the court window and every admission condition after it) admits: {gate_code}");
        // The gate admits; the processor's attribution check beside it (C5) is the next wall, and the SDK and the manifest verifier both name it.
        let sdk_err = sdk_v.expect_err("the SDK names the processor's refusal");
        assert!(sdk_err.contains("HELD_CLASS_UNANSWERABLE"), "daa {daa}: {sdk_err}");
        let C::Refused { reason, .. } = &report.classification else { panic!("daa {daa}: the manifest verifier says {manifest_says}") };
        assert!(reason.contains("HELD_CLASS_UNANSWERABLE"), "daa {daa}: {reason}");
        seen.push(gate_code);
    }
    assert!(seen.windows(2).all(|w| w[0] == w[1]), "the verdict moves with the height: {seen:?}");
}
