//! **The claim's evidence leaves the producer's machine before the producer may go away** (RFC-0009 stage B, via `misaka-palw-remote`'s
//! transport; RFC-0001 + RFC-0003 delivery task 3).
//!
//! A free-prompt claim is a promise to serve its material (the job, the prompt, the family capture) until `trace_retention_daa`. If the
//! only copy is on the producer's disk, a producer that switches off defaults the claim and a public verifier has nothing to read. The
//! gateway therefore places the material with one or more independent providers (`--evidence-provider <dir|http://…>`, repeatable) **before it
//! writes the commitment into the outbox**, and only counts the placement if each copy was READ BACK and verified against the manifest
//! (`publish_to_providers_v1`): a provider's acknowledgement is the provider's word.
//!
//! * The material is exactly what the node would serve (`FPC1`: job + prompt ids + the worker's capture), split into chunks and named by a
//!   manifest whose roots are the claim's own — the same bytes and the same manifest id for the node, the Panel and a public verifier.
//! * **A `PanelDa` claim is never published** (ADR-0077 Decision 16). Its prompt ids are the private input only the drawn seats may pull;
//!   a public provider would disclose them. The gateway refuses the combination at boot and per job, by name.
//! * What this proves is `LOCAL_OBSERVATION`: this machine read the copies back. It is not availability for anyone else, and it moves no slash.
//! * What this does not do: choose providers for anyone (there is no on-chain provider discovery — DESIGN_GAP, `transport` module doc), or
//!   keep them healthy (`transport::RetentionMonitor` is the repair loop).

use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PANEL_DA, PalwFpCaptureV1, PalwFreePromptCommitmentV3, palw_fp_capture_decode_v1, palw_fp_capture_encode_v1,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_hashes::Hash64;
use misaka_palw_remote::evidence::{ClaimRoots, EvidenceManifestV1, ManifestLimits, manifest_id_v1};
use misaka_palw_remote::transport::{EvidenceProvider, LOCAL_OBSERVATION, PublishReportV1, fetch_claim_material_any, publish_to_providers_v1};

/// Material chunk size: the same the rail's `--evidence-out` uses (1 MiB), so a manifest built here and one built there for the same
/// claim are the same manifest.
pub const CHUNK_BYTES: usize = 1 << 20;

/// One claim's evidence, ready to place.
#[derive(Clone, Debug)]
pub struct ClaimEvidence {
    pub claim_hex: String,
    pub manifest: EvidenceManifestV1,
    pub chunks: Vec<Vec<u8>>,
    pub material_bytes: usize,
}

/// The roots a manifest must agree with — the commitment's own.
pub fn claim_roots(c: &PalwFreePromptCommitmentV3) -> ClaimRoots {
    ClaimRoots {
        network_domain: c.job.network_domain,
        trace_root: c.trace_root,
        output_root: c.output_root,
        execution_root: c.execution_root,
        trace_chunk_count: c.trace_chunk_count,
        retention_deadline: c.trace_retention_daa,
    }
}

/// **Build the evidence** for a committed claim: `FPC1` material from the job, the prompt ids and the worker's capture, chunked and named
/// by a manifest over the claim's roots. Refused for a `PanelDa` claim (see the module doc) and for a capture that is not a capture.
pub fn build(commitment: &PalwFreePromptCommitmentV3, prompt_ids: &[u32], capture: &[u8]) -> Result<ClaimEvidence, String> {
    if commitment.job.privacy_mode == PALW_FP_PRIVACY_PANEL_DA {
        return Err("this claim is PanelDa: its prompt ids are private to the drawn seats (ADR-0077 Decision 16) and a public evidence provider \
                    would disclose them — its material is served only over the authenticated pull"
            .to_string());
    }
    misaka_palw_fp_submit::check_capture_shape(capture).map_err(|e| e.to_string())?;
    let material = palw_fp_capture_encode_v1(&commitment.job, prompt_ids, capture);
    let chunks = misaka_palw_remote::evidence::fs::chunk_material(&material, CHUNK_BYTES);
    let manifest = EvidenceManifestV1::build(
        commitment.job.network_domain,
        &commitment.job.executor_bond,
        &commitment.job.job_nonce,
        commitment.trace_root,
        commitment.output_root,
        commitment.execution_root,
        commitment.trace_chunk_count,
        commitment.trace_retention_daa,
        &chunks,
    );
    manifest.validate_shape(&ManifestLimits::default()).map_err(|e| format!("the evidence manifest is not admissible: {e}"))?;
    manifest.verify_claim_binding(&claim_roots(commitment)).map_err(|e| format!("the evidence manifest does not agree with its own claim: {e}"))?;
    Ok(ClaimEvidence {
        claim_hex: faster_hex::hex_string(kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(commitment).as_byte_slice()),
        manifest,
        chunks,
        material_bytes: material.len(),
    })
}

/// Place the evidence with `providers`; `Ok` only when at least `min_verified` of them hold a copy that was read back and verified.
pub fn publish(
    evidence: &ClaimEvidence,
    providers: &[&dyn EvidenceProvider],
    min_verified: usize,
) -> Result<PublishReportV1, PublishReportV1> {
    publish_to_providers_v1(&evidence.claim_hex, &evidence.manifest, &evidence.chunks, providers, min_verified)
}

/// The JSON a response and the outbox summary carry about a placement.
pub fn report_json(evidence: &ClaimEvidence, report: &PublishReportV1) -> serde_json::Value {
    serde_json::json!({
        "claim_id": evidence.claim_hex,
        "manifest_id": faster_hex::hex_string(manifest_id_v1(&evidence.manifest).as_byte_slice()),
        "chunks": evidence.chunks.len(),
        "material_bytes": evidence.material_bytes,
        "retention_until_daa": evidence.manifest.retention_until_daa,
        "verified_copies": report.verified_copies(),
        "providers": report.per_provider.iter().map(|o| serde_json::json!({
            "provider": o.provider,
            "read_back_verified": o.result.is_ok(),
            "error": o.result.as_ref().err(),
        })).collect::<Vec<_>>(),
        // What "verified" is worth: this machine read the bytes back from that provider. It says nothing a third party can rely on.
        "provenance": LOCAL_OBSERVATION,
        "note": "a placement read back by this gateway, not proof of future availability; the producer stays accountable for the claim's retention window",
    })
}

/// What a public verifier ends up holding after a successful fetch.
#[derive(Clone, Debug)]
pub struct PublicMaterial {
    pub payload: PalwFpCaptureV1,
    pub bytes: usize,
}

/// **The public verifier's checks on fetched material**, beyond the per-chunk hashes the transport already made: it is an `FPC1` payload
/// whose prompt ids bind its job (in the network's form), and the job is THE CLAIM'S job — not another claim's material served under this
/// claim's manifest. What it cannot check is the arithmetic: that is a replay (the seat's, or anyone's) over this material.
pub fn verify_public_material(bytes: &[u8], commitment: &PalwFreePromptCommitmentV3, form: PalwPromptIdsFormV1) -> Result<PublicMaterial, String> {
    let payload = palw_fp_capture_decode_v1(bytes, form).ok_or("the fetched bytes are not an FPC1 payload whose prompt ids bind its job")?;
    if payload.material.job != commitment.job {
        return Err("the fetched material is for a different job than the claim's".to_string());
    }
    Ok(PublicMaterial { payload, bytes: bytes.len() })
}

/// **Fetch a claim's material from any provider and verify it** — what a public verifier does with a claim id, the commitment (read from the
/// chain) and a provider list, with the producer switched off.
pub fn fetch_and_verify(
    providers: &[&dyn EvidenceProvider],
    commitment: &PalwFreePromptCommitmentV3,
    form: PalwPromptIdsFormV1,
) -> Result<PublicMaterial, String> {
    let claim_hex = faster_hex::hex_string(kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(commitment).as_byte_slice());
    let seed = kaspa_hashes::blake2b_512_keyed(b"misaka-palw/gateway/evidence-fetch-order/v1", claim_hex.as_bytes());
    let (bytes, _report) = fetch_claim_material_any(providers, &claim_hex, &claim_roots(commitment), &ManifestLimits::default(), seed)
        .map_err(|e| format!("no provider could serve the claim's material: {e}"))?;
    verify_public_material(&bytes, commitment, form)
}

/// The retained capture of a job, where the worker wrote it (`traces/<job id>/material.bin`).
pub fn read_capture(outbox: &std::path::Path, job_id: &Hash64) -> Result<Vec<u8>, String> {
    let path = outbox.join("traces").join(faster_hex::hex_string(job_id.as_byte_slice())).join("material.bin");
    std::fs::read(&path).map_err(|e| format!("cannot read the worker's retained capture at {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{FloorWorker, certified_facts, config, identity, temp_dir};
    use crate::{JobRunner, chain, serving};
    use misaka_palw_remote::transport::{ProviderSpecV1, open_providers_v1, server};

    const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::MerkleV1;

    /// One real committed claim: its commitment, its prompt ids and its capture.
    fn a_claim(dir: &std::path::Path) -> (PalwFreePromptCommitmentV3, Vec<u32>, Vec<u8>) {
        let w = FloorWorker::new(&dir.join("traces"), FORM);
        let (cfg, id, facts) = (config(dir), identity(&w.profile), certified_facts(false));
        let source = crate::testkit::offline_source(dir);
        let body = serde_json::json!({ "messages": [{ "role": "user", "content": "hi" }], "max_tokens": 4 });
        let out = crate::testkit::chat(&cfg, &id, &w, &facts, &source, &body, &serving::AlwaysPresent).expect("answers");
        assert_eq!(out["misaka"]["committed"], true);
        let job_id = out["misaka"]["fp_job_id"].as_str().unwrap().to_string();
        let stem = format!("fp-job-{}", &job_id[..16]);
        let commitment: PalwFreePromptCommitmentV3 = borsh::from_slice(&std::fs::read(dir.join(format!("{stem}.commitment-unsigned.borsh"))).unwrap()).unwrap();
        let result: kaspa_consensus_core::palw_freeprompt_v3::PalwFpWorkerResultV3 =
            borsh::from_slice(&std::fs::read(dir.join(format!("{stem}.result.borsh"))).unwrap()).unwrap();
        let capture = read_capture(dir, &kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(&commitment.job)).expect("the worker retained its capture");
        let _ = (chain::ChainFacts::default(), JobRunner::processes(&w));
        (commitment, result.prompt_token_ids, capture)
    }

    #[test]
    fn evidence_placed_with_a_directory_and_an_http_provider_outlives_the_producer() {
        let dir = temp_dir("evidence-offline");
        let (commitment, ids, capture) = a_claim(&dir);
        let evidence = build(&commitment, &ids, &capture).expect("builds");
        assert_eq!(evidence.manifest.trace_root, commitment.trace_root, "the manifest is over the claim's own roots");

        // Two independent providers: a directory, and the reference HTTP provider over another directory.
        let dir_provider = temp_dir("evidence-provider-dir");
        let http_root = temp_dir("evidence-provider-http");
        let http = server::start("127.0.0.1:0", http_root.clone(), server::ServerConfig::default()).expect("the reference provider");
        let specs = vec![ProviderSpecV1::Dir(dir_provider.clone()), ProviderSpecV1::Http(http.url())];
        let opened = open_providers_v1(&specs);
        let refs: Vec<&dyn EvidenceProvider> = opened.iter().map(|b| b.as_ref()).collect();
        let report = publish(&evidence, &refs, 2).expect("both copies were read back and verified");
        assert_eq!(report.verified_copies(), 2);
        let summary = report_json(&evidence, &report);
        assert_eq!(summary["verified_copies"], 2);
        assert_eq!(summary["provenance"], LOCAL_OBSERVATION, "what the gateway read back is a local observation, and says so");

        // THE PRODUCER GOES OFFLINE: the outbox, the retained traces, the worker — everything local is gone.
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(!dir.exists());

        // A public verifier holding only the commitment (from the chain) and the provider list fetches and verifies.
        let verifier = open_providers_v1(&specs);
        let verifier_refs: Vec<&dyn EvidenceProvider> = verifier.iter().map(|b| b.as_ref()).collect();
        let public = fetch_and_verify(&verifier_refs, &commitment, FORM).expect("the material is public");
        assert!(public.bytes > 0);
        assert_eq!(public.payload.material.job, commitment.job);
        assert_eq!(public.payload.material.prompt_token_ids, ids);
        assert_eq!(public.payload.capture, capture, "the very capture the worker produced, byte for byte");
        // Either provider alone is enough: lose the directory, then lose the HTTP one instead.
        let only_http = open_providers_v1(&specs[1..]);
        assert!(fetch_and_verify(&[only_http[0].as_ref()], &commitment, FORM).is_ok());
        std::fs::remove_dir_all(&dir_provider).unwrap();
        assert!(fetch_and_verify(&[only_http[0].as_ref()], &commitment, FORM).is_ok(), "the HTTP provider alone still serves it");
        http.stop();
        let gone = open_providers_v1(&specs);
        let gone_refs: Vec<&dyn EvidenceProvider> = gone.iter().map(|b| b.as_ref()).collect();
        let err = fetch_and_verify(&gone_refs, &commitment, FORM).unwrap_err();
        assert!(err.contains("no provider"), "with every copy gone the verifier says so and invents nothing: {err}");
        let _ = std::fs::remove_dir_all(&http_root);
    }

    #[test]
    fn a_provider_that_serves_other_bytes_or_another_claims_manifest_is_never_believed() {
        let dir = temp_dir("evidence-tamper");
        let (commitment, ids, capture) = a_claim(&dir);
        let evidence = build(&commitment, &ids, &capture).unwrap();
        let (good, bad) = (temp_dir("evidence-good"), temp_dir("evidence-bad"));
        let specs = vec![ProviderSpecV1::Dir(bad.clone()), ProviderSpecV1::Dir(good.clone())];
        let opened = open_providers_v1(&specs);
        let refs: Vec<&dyn EvidenceProvider> = opened.iter().map(|b| b.as_ref()).collect();
        publish(&evidence, &refs, 2).unwrap();
        // The first provider's chunk is corrupted on disk: its bytes no longer hash to the manifest, the other provider serves the claim.
        let manifest_id = faster_hex::hex_string(manifest_id_v1(&evidence.manifest).as_byte_slice());
        let chunk0 = bad.join("chunks").join(&manifest_id).join("0.chunk");
        let mut bytes = std::fs::read(&chunk0).unwrap();
        bytes[10] ^= 1;
        std::fs::write(&chunk0, bytes).unwrap();
        let public = fetch_and_verify(&refs, &commitment, FORM).expect("the good provider still serves it");
        assert_eq!(public.payload.capture, capture);
        // With only the corrupted provider there is nothing to believe.
        assert!(fetch_and_verify(&[refs[0]], &commitment, FORM).is_err());
        // A different claim's roots do not accept this provider's manifest.
        let mut other = commitment.clone();
        other.output_root = Hash64::from_u64_word(0xBAD);
        assert!(fetch_and_verify(&refs, &other, FORM).is_err(), "a claim with another output root has no manifest here");
        // Valid chunks of ANOTHER job served under this claim: the material decodes, but it is not the claim's job.
        let mut other_job = commitment.clone();
        other_job.job.job_nonce[0] ^= 1;
        let foreign = palw_fp_capture_encode_v1(&other_job.job, &ids, &capture);
        assert!(verify_public_material(&foreign, &commitment, FORM).unwrap_err().contains("different job"));
        assert!(verify_public_material(b"not material", &commitment, FORM).is_err());
        let _ = (std::fs::remove_dir_all(&dir), std::fs::remove_dir_all(&good), std::fs::remove_dir_all(&bad));
    }

    #[test]
    fn a_panel_da_claim_is_never_published_and_a_non_capture_is_refused() {
        let dir = temp_dir("evidence-panel");
        let (mut commitment, ids, capture) = a_claim(&dir);
        assert!(build(&commitment, &ids, &[]).is_err(), "an empty capture is not a capture");
        assert!(build(&commitment, &ids, b"FPM1....").is_err(), "a material is not a capture");
        commitment.job.privacy_mode = PALW_FP_PRIVACY_PANEL_DA;
        let err = build(&commitment, &ids, &capture).unwrap_err();
        assert!(err.contains("PanelDa") && err.contains("disclose"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Through the gateway**: with providers configured the commitment enters the outbox only after its evidence stands elsewhere.
    #[test]
    fn the_gateway_publishes_before_it_commits_and_withholds_the_commitment_if_the_copies_are_not_there() {
        let dir = temp_dir("evidence-gateway");
        let w = FloorWorker::new(&dir.join("traces"), FORM);
        let (id, facts) = (identity(&w.profile), certified_facts(false));
        let source = crate::testkit::offline_source(&dir);
        let body = serde_json::json!({ "messages": [{ "role": "user", "content": "hi" }], "max_tokens": 4 });
        let provider_dir = temp_dir("evidence-gw-provider");

        // (1) Two copies required, one provider unreachable (nothing listens there): the answer is returned, the claim is not committed.
        let dead = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead_url = format!("http://{}", dead.local_addr().unwrap());
        drop(dead);
        let mut cfg = config(&dir);
        cfg.evidence_providers = vec![ProviderSpecV1::Dir(provider_dir.clone()), ProviderSpecV1::Http(dead_url)];
        cfg.evidence_min_copies = 2;
        let refused = crate::testkit::chat(&cfg, &id, &w, &facts, &source, &body, &serving::AlwaysPresent).unwrap();
        assert_eq!(refused["misaka"]["committed"], false);
        assert!(refused["misaka"]["not_committed_because"].as_str().unwrap().contains("evidence"), "{}", refused["misaka"]["not_committed_because"]);
        assert_eq!(std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".commitment-unsigned.borsh")).count(), 0);
        assert_eq!(refused["misaka"]["request"]["status"], "answered");

        // (2) One copy suffices at --evidence-min-copies 1: committed, with a report that says where the copies are.
        cfg.evidence_min_copies = 1;
        let ok = crate::testkit::chat(&cfg, &id, &w, &facts, &source, &body, &serving::AlwaysPresent).unwrap();
        assert_eq!(ok["misaka"]["committed"], true);
        assert_eq!(ok["misaka"]["evidence"]["verified_copies"], 1);
        assert_eq!(ok["misaka"]["evidence"]["provenance"], LOCAL_OBSERVATION);
        let job_id = ok["misaka"]["fp_job_id"].as_str().unwrap().to_string();
        let stem = format!("fp-job-{}", &job_id[..16]);
        let commitment: PalwFreePromptCommitmentV3 = borsh::from_slice(&std::fs::read(dir.join(format!("{stem}.commitment-unsigned.borsh"))).unwrap()).unwrap();
        let summary: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{stem}.json"))).unwrap()).unwrap();
        assert_eq!(summary["evidence"]["verified_copies"], 1, "the outbox summary records the placement");

        // (3) The producer's machine is gone; a verifier with the commitment and the provider list reads it.
        std::fs::remove_dir_all(&dir).unwrap();
        let verifier = open_providers_v1(&[ProviderSpecV1::Dir(provider_dir.clone())]);
        let material = fetch_and_verify(&[verifier[0].as_ref()], &commitment, FORM).expect("public after the producer left");
        assert_eq!(material.payload.material.job, commitment.job);
        let _ = std::fs::remove_dir_all(&provider_dir);
    }

    #[test]
    fn a_panel_da_gateway_configured_with_public_providers_is_refused_by_name() {
        let dir = temp_dir("evidence-boot");
        let mut cfg = config(&dir);
        cfg.privacy_mode = PALW_FP_PRIVACY_PANEL_DA;
        cfg.evidence_providers = vec![ProviderSpecV1::Dir(dir.clone())];
        let err = boot_check(&cfg).unwrap_err();
        assert!(err.contains("panel-da") && err.contains("private"), "{err}");
        cfg.privacy_mode = PALW_FP_PRIVACY_PUBLIC_DA_FOR_TEST;
        assert!(boot_check(&cfg).is_ok());
        cfg.evidence_min_copies = 5;
        assert!(boot_check(&cfg).unwrap_err().contains("more copies"), "asking for more copies than providers can never succeed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    const PALW_FP_PRIVACY_PUBLIC_DA_FOR_TEST: u8 = kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA;
}

/// The boot-time check of the evidence configuration: a `PanelDa` gateway must not point at public providers, and the required copy count
/// must be satisfiable by the providers named.
pub fn boot_check(config: &crate::Config) -> Result<(), String> {
    if config.evidence_providers.is_empty() {
        return Ok(());
    }
    if config.privacy_mode == PALW_FP_PRIVACY_PANEL_DA {
        return Err("--privacy panel-da with --evidence-provider: a PanelDa claim's prompt ids are private to the drawn seats (ADR-0077 Decision 16) \
                    and an evidence provider is public — refusing to boot rather than disclose them"
            .to_string());
    }
    if config.evidence_min_copies > config.evidence_providers.len() {
        return Err(format!(
            "--evidence-min-copies {} asks for more copies than the {} provider(s) named can hold",
            config.evidence_min_copies,
            config.evidence_providers.len()
        ));
    }
    Ok(())
}
