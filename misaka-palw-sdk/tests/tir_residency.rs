//! **A lowered model runs the same within a budget as through the page cache** (ADR-0112 for IR
//! classes, through the SDK; `docs/design/palw/tir/runtime-residency.md`).
//!
//! Hugging Face mixtures and a dense decoder, lowered, calibrated, materialised and declared as the
//! converter does it, then served three ways: mapped (the page cache, `--palw-class-resident-bytes
//! 0`), through the lineage under a stated budget (the node's door: `TirLineageV1::load`), and held
//! at the class's floor with every row-addressed param served by rows — the lowered experts routed,
//! their reshaped scales with them, the embeddings and position tables gathered. The same anchor's
//! job computes the same capture every way, byte for byte; the routed and gathered paths really ran;
//! nothing was mapped and nothing served by rows was read whole.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_hashes::Hash64;
use misaka_palw_sdk::lineages::tir::{TirLineageV1, tir_residency_stats_of};
use misaka_palw_sdk::{PalwModelLineageV1, PalwTirClassEntryV1, PalwWeightResidencyV1};
use misaka_palw_tir_exec::node::{TirArtifactV1, TirResidencyPolicyV1};
use misaka_palw_tir_exec::{TirTierRulesV1, TirTierV1, TirTiersV1};

const CONTEXT: u32 = 32;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("palw-sdk-residency-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The fixture lowered, calibrated, materialised and declared as a class (`palw-tir-fidelity`, then
/// `palw-class declare-layout`): the declared container's path.
fn declared(fixture: &str, dir: &Path) -> Option<PathBuf> {
    use misaka_palw_tir_lower::float_ref::ParamStore;
    use misaka_palw_tir_lower::float_ref::stream::Resident;
    use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
    use misaka_palw_tir_lower::quant::QuantPolicy;
    use misaka_palw_tir_lower::weights::Checkpoint;
    use misaka_palw_tir_lower::{artifact, fidelity};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures/hf").join(fixture);
    if !root.join("model.safetensors").exists() {
        eprintln!("the {fixture} fixture is missing: skipped");
        return None;
    }
    let config = std::fs::read_to_string(root.join("config.json")).unwrap();
    let prep = fidelity::prepare(&config, &LowerOpts { max_window: Some(CONTEXT), ..Default::default() }).expect("lowered");
    let ck = Checkpoint::open(&root).unwrap();
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap();
    let loader = Resident(Arc::new(params));
    let calib = fidelity::random_sequences(prep.hl.vocab, 2, CONTEXT as usize, 7);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).unwrap();
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).unwrap();
    let lowered = dir.join(format!("{fixture}.palwtir"));
    let meta = serde_json::json!({ "calibrated_context": CONTEXT, "model_id": format!("test/{fixture}") });
    artifact::write(&lowered, &prep.lowered.program, &mat.params, [0u8; 64], meta).unwrap();
    let declared = dir.join(format!("{fixture}.class.palwtir"));
    let net = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &net.palw_consensus_mode else { panic!() };
    let choice = misaka_palw_sdk::tir_layout::TirLayoutChoiceV1 { max_context: Some(CONTEXT), ..Default::default() };
    misaka_palw_sdk::tir_layout::tir_declare_layout_v1(&net, bundle, &lowered, &declared, &choice, None).expect("declared");
    Some(declared)
}

fn court() -> PalwCourtParamsV2 {
    PalwCourtParamsV2::new(kaspa_consensus_core::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT, 4, 2).expect("a court")
}

/// The anchor's job, executed: the capture's roots and bytes, and three cone closes of it.
fn served(entry: &PalwTirClassEntryV1) -> (Hash64, Hash64, Hash64, Vec<u8>, Vec<Vec<u8>>) {
    let backend = TirLineageV1::backend(entry, &court(), PalwPromptIdsFormV1::Flat).expect("a backend");
    let (job, prompt) = backend.job_for_anchor(Hash64::from_bytes([0x2B; 64])).unwrap();
    let outcome = backend.execute(&job, &prompt).expect("the job runs");
    let rules = backend.court_rules(&court());
    let n = misaka_palw_tir_exec::node::TirCaptureV1::decode(&outcome.material).unwrap().binding.step_leaf_count;
    let closes = [0, n / 2, n - 1]
        .iter()
        .map(|i| borsh::to_vec(&backend.cone_close(&outcome.material, *i, &rules).unwrap()).unwrap())
        .collect();
    (outcome.execution_root, outcome.trace_root, outcome.output_root, outcome.material, closes)
}

#[test]
fn a_lowered_model_runs_the_same_within_a_budget_as_through_the_page_cache() {
    let dir = Scratch::new("lowered");
    let all_rows = TirTierRulesV1 { pin_below_bytes: 0 };
    let mut routed_models = 0;
    for fixture in ["qwen3_moe", "mixtral", "olmoe", "llama"] {
        let Some(path) = declared(fixture, &dir.0) else { continue };
        let mapped = TirLineageV1::open_entry(&path).expect("mapped");
        let want = served(&mapped);
        // The tiers of the lowered program: a mixture's experts and their scales are routed, the
        // embedding is gathered by the token.
        let program = &mapped.artifact.plan().program;
        let tiers = TirTiersV1::of(program, all_rows);
        let routed = tiers.params.iter().filter(|t| t.tier == TirTierV1::Routed).count();
        let gathered = tiers.params.iter().filter(|t| t.tier == TirTierV1::Gathered).count();
        assert!(gathered > 0, "{fixture}: an embedding is gathered");
        if fixture != "llama" {
            assert!(routed >= 3, "{fixture}: the expert stacks are routed ({routed})");
            routed_models += 1;
        }
        // Held at its floor with every row-addressed param served by rows.
        let a = tiers.arithmetic();
        let art = Arc::new(TirArtifactV1::open_with_rules(&path, TirResidencyPolicyV1::Bytes(a.floor_bytes), all_rows).unwrap());
        assert!(!art.is_mapped(), "{fixture}");
        let resident = PalwTirClassEntryV1 { artifact: art.clone(), ..mapped.clone() };
        assert_eq!(served(&resident), want, "{fixture}: the floor computes the page cache's capture and closes");
        let s = art.residency_stats().unwrap();
        assert!(s.gathered_rows > 0, "{fixture}: {s:?}");
        if fixture != "llama" {
            assert!(s.hits + s.misses > 0, "{fixture}: the routed path ran: {s:?}");
        }
        assert_eq!(s.whole_reads, 0, "{fixture}: nothing served by rows was read whole");
        // Through the lineage, the node's door, under a stated budget (the node's tier rules).
        let lineage = TirLineageV1::new();
        let holding = lineage.load(&path, PalwWeightResidencyV1::Bytes(a.weight_bytes * 2)).expect("loads within a budget");
        assert!(holding.summary.contains("resident within"), "{fixture}: {}", holding.summary);
        assert!(tir_residency_stats_of(&holding).is_some(), "{fixture}");
        let entry = lineage.tir_classes().remove(0);
        assert_eq!((entry.class_id(), entry.artifact_root), (mapped.class_id(), mapped.artifact_root), "{fixture}");
        assert_eq!(served(&entry), want, "{fixture}: the lineage's residency computes the same");
        // The page cache, asked for by `0`: mapped as before, no numbers.
        let paged = TirLineageV1::new().load(&path, PalwWeightResidencyV1::PageCache).unwrap();
        assert!(tir_residency_stats_of(&paged).is_none() && paged.summary.contains("page cache"), "{}", paged.summary);
    }
    assert!(routed_models >= 2, "the mixtures ran: {routed_models}");
}
