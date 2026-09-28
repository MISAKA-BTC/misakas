//! **`TirLineageV1`: IR classes** (RFC-0002 Phase F, F9; design §2.10).
//!
//! The container is F3's `PALWTIR1` (`misaka_palw_tir_artifact`): the program, the layout and the
//! tokenizer id in the header, every param instance 64-byte aligned behind it. `load` opens and
//! checks it, maps it, binds every param in place and computes the TIR inventory root ONCE, streamed
//! (the root is this node's proof that it holds what the chain registered — never read from a
//! sidecar here). An IR class is DATA — the class the artifact declares, under that root — so the
//! lineage's classes are the artifacts it has loaded ([`PalwModelLineageV1::tir_classes`]), not a
//! table; it has no legacy class, and pairs with no legacy entry.
//!
//! `resolve` serves a chain-named `(class_id, artifact_root)` from a held artifact that derives
//! exactly that pair, through `misaka_palw_tir_exec`'s generic backend; any other pair is not this
//! lineage's.

use std::path::Path;
use std::sync::{Arc, RwLock};

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_attempt_canonical_v1;
use kaspa_hashes::Hash64;
use misaka_palw_tir_exec::node::TirArtifactV1;

use crate::lineage::{PalwClassEntryV1, PalwLoadedArtifactV1, PalwModelLineageV1, PalwTirClassEntryV1};

/// The IR backend and its capture, for the node's IR-only verbs (the IR court's close proofs).
pub use misaka_palw_tir_exec::node::{
    TIR_FREE_PROMPT_CLOSED_V1, TirBackendV1, TirCaptureV1, set_tir_fused_kernels_default_v1, tir_drill_covering_leaves_v1,
    tir_fused_kernels_default_v1, tir_trace_event_disclosure_of_capture_v1,
};

/// The lineage's id.
pub const TIR_LINEAGE_ID_V1: &str = "palw-tir-v1";

/// **The IR lineage.** Holds the classes of the artifacts it loaded.
#[derive(Default)]
pub struct TirLineageV1 {
    held: RwLock<Vec<PalwTirClassEntryV1>>,
}

/// The model id an artifact names: its provenance's `model_id` (or `model`) when the container's
/// JSON meta carries one, else the file's stem.
fn model_id_of(meta: &str, path: &Path) -> String {
    serde_json::from_str::<serde_json::Value>(meta)
        .ok()
        .and_then(|v| ["model_id", "model"].iter().find_map(|k| v.get(*k).and_then(|m| m.as_str()).map(str::to_string)))
        .unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "palw-tir".into()))
}

impl TirLineageV1 {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open one PALWTIR1 file into the entry of the class it declares.
    pub fn open_entry(path: &Path) -> Result<PalwTirClassEntryV1, String> {
        let artifact = Arc::new(TirArtifactV1::open(path)?);
        let class = artifact.class().map_err(|e| {
            format!("{}: {e} — an IR class needs a declared layout (`palw-class declare-layout` writes one)", path.display())
        })?;
        let (artifact_root, _) = artifact.inventory_root().map_err(|e| format!("{}: {e}", path.display()))?;
        let canonical_job = palw_tir_attempt_canonical_v1(&class).ok_or_else(|| {
            format!("{}: a context of {} positions is too narrow for a canonical job", path.display(), class.layout.max_context)
        })?;
        Ok(PalwTirClassEntryV1 {
            model_id: model_id_of(&artifact.container().header.meta, path),
            lineage_id: TIR_LINEAGE_ID_V1,
            class: Arc::new(class),
            artifact_root,
            canonical_job,
            artifact,
            path: Some(path.to_path_buf()),
        })
    }

    /// The backend for one held entry.
    pub fn backend(
        entry: &PalwTirClassEntryV1,
        court: &PalwCourtParamsV2,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ) -> Result<TirBackendV1, String> {
        TirBackendV1::new(
            entry.model_id.clone(),
            entry.artifact.clone(),
            entry.artifact_root,
            entry.canonical_context(),
            prompt_ids_form,
            court.max_step_leaf_count(),
        )
    }
}

impl PalwModelLineageV1 for TirLineageV1 {
    fn lineage_id(&self) -> &'static str {
        TIR_LINEAGE_ID_V1
    }

    /// No legacy class: an IR class has no `PalwShapeProfileV3`.
    fn classes(&self, _court: &PalwCourtParamsV2) -> Vec<PalwClassEntryV1> {
        Vec::new()
    }

    fn tir_classes(&self) -> Vec<PalwTirClassEntryV1> {
        self.held.read().expect("the held list is never poisoned").clone()
    }

    fn sniffs(&self, head: &[u8; 8]) -> bool {
        head == misaka_palw_tir_artifact::PALW_TIR_CONTAINER_MAGIC_V1
    }

    /// Mapped, checked, rooted — and remembered, so the class is one of this lineage's.
    fn load(&self, path: &Path, _residency: crate::lineage::PalwWeightResidencyV1) -> Result<PalwLoadedArtifactV1, String> {
        let entry = Self::open_entry(path)?;
        let summary = format!(
            "IR class {} ({}): {} params, {} blocks, max_context {}, artifact root {}",
            entry.class_id(),
            entry.model_id,
            entry.artifact.plan().program.params.len(),
            entry.artifact.plan().program.blocks.len(),
            entry.class.layout.max_context,
            entry.artifact_root
        );
        {
            let mut held = self.held.write().expect("the held list is never poisoned");
            if !held.iter().any(|e| e.class_id() == entry.class_id()) {
                held.push(entry.clone());
            }
        }
        Ok(PalwLoadedArtifactV1::from_parts(TIR_LINEAGE_ID_V1, Some(path.to_path_buf()), summary, Arc::new(entry)))
    }

    fn registered_weight_keys(&self, artifact: &PalwLoadedArtifactV1) -> Vec<Hash64> {
        artifact.payload().downcast_ref::<PalwTirClassEntryV1>().map(|e| vec![e.artifact_root]).unwrap_or_default()
    }

    fn pair(&self, _court: &PalwCourtParamsV2, entry: &PalwClassEntryV1, _artifact: &PalwLoadedArtifactV1) -> Result<Hash64, String> {
        Err(format!("{}: an IR artifact is its own class and pairs with no legacy entry", entry.model_id))
    }

    fn resolve(
        &self,
        court: &PalwCourtParamsV2,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        class_id: Hash64,
        artifact_root: Hash64,
        holdings: &[PalwLoadedArtifactV1],
        _network_id: &[u8],
    ) -> Option<Result<Box<dyn PalwExecutionBackendV1>, String>> {
        let entry = holdings
            .iter()
            .filter(|h| h.lineage_id == TIR_LINEAGE_ID_V1)
            .filter_map(|h| h.payload().downcast_ref::<PalwTirClassEntryV1>().cloned())
            .find(|e| e.class_id() == class_id && e.artifact_root == artifact_root)?;
        Some(Self::backend(&entry, court, prompt_ids_form).map(|b| Box::new(b) as Box<dyn PalwExecutionBackendV1>))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PalwClassSdk;
    use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwMaterialVerdictV1};
    use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
    use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use kaspa_consensus_core::palw_step_refute::{PALW_LOGITS_TILE_LANES, flat_logits_scheme_id_v1, tiled_logits_scheme_id_v1};
    use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
    use misaka_palw_tir::TirProgramV1;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn court() -> PalwCourtParamsV2 {
        PalwCourtParamsV2::new(kaspa_consensus_core::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT, 4, 2)
            .expect("shipped court")
    }

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
    }

    /// The admissible golden program vectors (program, then each param instance's bytes), under
    /// the flat and the tiled logits scheme in turn.
    /// Each param instance's little-endian bytes.
    type Tensors = BTreeMap<(u16, Option<u16>), Vec<u8>>;

    fn vectors() -> Vec<(String, TirProgramV1, Tensors)> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs");
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir).expect("vectors").map(|e| e.unwrap().path()).collect();
        files.sort();
        let mut out = Vec::new();
        for (k, path) in files.into_iter().enumerate() {
            let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let mut p = TirProgramV1::decode_canonical(&unhex(v["program_borsh_hex"].as_str().unwrap())).expect("canonical");
            if misaka_palw_tir::interval::analyze_ranges(&p).is_err() || p.params.is_empty() {
                continue;
            }
            let scheme = if k % 2 == 0 { flat_logits_scheme_id_v1() } else { tiled_logits_scheme_id_v1() };
            p.logits_scheme_id.copy_from_slice(scheme.as_byte_slice());
            let p = TirProgramV1::decode_canonical(&p.encode()).expect("still canonical");
            let params = v["params"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| {
                    let key = (t["param"].as_u64().unwrap() as u16, t["layer"].as_u64().map(|l| l as u16));
                    (key, unhex(t["le_hex"].as_str().unwrap()))
                })
                .collect();
            out.push((v["name"].as_str().unwrap().to_string(), p, params));
        }
        out
    }

    fn layout(p: &TirProgramV1) -> PalwTirLayoutV1 {
        let tiled = Hash64::from_bytes(p.logits_scheme_id) == tiled_logits_scheme_id_v1();
        let mut commit_tiles = Vec::new();
        for (bi, b) in p.blocks.iter().enumerate() {
            for (ni, n) in b.nodes.iter().enumerate() {
                if n.commit {
                    let logits = bi == p.schedule.post as usize && ni == p.logits as usize;
                    commit_tiles.push(if logits && tiled { PALW_LOGITS_TILE_LANES as u32 } else { 8 });
                }
            }
        }
        PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 64,
            checkpoint_interval: 2,
            h_tile: 4,
            commit_tiles,
            state_tiles: p.states.iter().map(|_| 4).collect(),
        }
    }

    fn write(dir: &Path, name: &str, p: &TirProgramV1, params: &Tensors, lay: Vec<u8>) -> PathBuf {
        let path = dir.join(format!("{name}.palwtir"));
        let meta = serde_json::json!({ "model_id": format!("test/{name}") }).to_string();
        misaka_palw_tir_artifact::write_container_v1(&path, p, lay, [9; 64], meta, &mut |j, l| {
            params.get(&(j, l)).cloned().ok_or_else(|| format!("no tensor {j} {l:?}"))
        })
        .expect("a container");
        path
    }

    /// **An IR artifact through the SDK's one door**: loaded by its magic into the IR lineage,
    /// listed as an IR class (and never as a legacy one), passing the conformance battery,
    /// resolved by `(class_id, artifact_root)` to the generic backend, whose capture a seat checks.
    #[test]
    fn ir_artifacts_load_conform_and_resolve() {
        let dir = std::env::temp_dir().join(format!("palw-sdk-tir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sdk = PalwClassSdk::builtin_v1(court(), PalwPromptIdsFormV1::Flat, b"misaka-palw-rc".to_vec());
        let legacy = sdk.ledger().len();
        let mut holdings = Vec::new();
        for (name, p, params) in vectors() {
            let path = write(&dir, &name, &p, &params, borsh::to_vec(&layout(&p)).unwrap());
            let loaded = sdk.load_artifact(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(loaded.lineage_id, TIR_LINEAGE_ID_V1, "{name}: dispatched by its magic");
            assert!(sdk.pairings(&loaded).is_empty(), "{name}: an IR artifact pairs with no legacy entry");
            holdings.push(loaded);
        }
        assert!(holdings.len() >= 4, "{} IR artifacts", holdings.len());
        assert_eq!(sdk.ledger().len(), legacy, "IR classes stay out of the legacy ledger");
        let tir = sdk.tir_ledger();
        assert_eq!(tir.len(), holdings.len());
        assert!(tir.iter().all(|e| e.model_id.starts_with("test/")), "the model id is the container's provenance");
        crate::conformance::check_sdk_v1(&sdk).expect("every IR class satisfies the battery");
        for entry in &tir {
            let backend =
                sdk.resolve(entry.class_id(), entry.artifact_root, &holdings).unwrap_or_else(|e| panic!("{}: {e}", entry.model_id));
            assert_eq!(backend.model_id(), entry.model_id);
            let anchor = Hash64::from_bytes([0x42; 64]);
            let (job, prompt) = backend.job_for_anchor(anchor).unwrap();
            assert_eq!(job.shape_profile_id, entry.class_id());
            let outcome = backend.execute(&job, &prompt).unwrap();
            let claim = PalwClaimRootsV1 {
                execution_root: outcome.execution_root,
                trace_root: outcome.trace_root,
                anchor,
                attempt_draw: Some(false),
                output_root: Some(outcome.output_root),
                job_pin: None,
            };
            assert_eq!(backend.verify_material(&outcome.material, claim), PalwMaterialVerdictV1::Matches, "{}", entry.model_id);
            // Another root is another class: nobody serves it.
            let err = sdk.resolve(entry.class_id(), Hash64::from_bytes([1; 64]), &holdings).map(|_| ()).unwrap_err();
            assert!(err.contains("cannot serve the registered class"), "{err}");
        }
        // A container that declares no layout is no class yet, and says why.
        let (name, p, params) = vectors().remove(0);
        let path = write(&dir, &format!("{name}-nolayout"), &p, &params, Vec::new());
        let err = sdk.load_artifact(&path).unwrap_err();
        assert!(err.contains("declared layout"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **The fused kernels change nothing a node commits** (RFC-0002 Phase G, the node's
    /// `--palw-tir-fused-kernels`): the tiny Qwen3.5 hybrid — whose gated delta step the fused layer
    /// matches — lowered, declared and loaded as an IR class, runs its canonical job to the same
    /// capture with the fused kernels on as off, and on is really on (regions run fused).
    #[test]
    fn an_ir_class_runs_the_same_with_the_fused_kernels_on() {
        use misaka_palw_tir_lower::float_ref::ParamStore;
        use misaka_palw_tir_lower::float_ref::stream::Resident;
        use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
        use misaka_palw_tir_lower::quant::QuantPolicy;
        use misaka_palw_tir_lower::weights::Checkpoint;
        use misaka_palw_tir_lower::{artifact, fidelity};
        const CONTEXT: u32 = 32;
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures/hf/qwen3_5");
        if !fixture.join("model.safetensors").exists() {
            eprintln!("the qwen3_5 fixture is missing: skipped");
            return;
        }
        let config = std::fs::read_to_string(fixture.join("config.json")).unwrap();
        let prep = fidelity::prepare(&config, &LowerOpts { max_window: Some(CONTEXT), ..Default::default() }).expect("lowered");
        let ck = Checkpoint::open(&fixture).unwrap();
        let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap();
        let loader = Resident(Arc::new(params));
        let calib = fidelity::random_sequences(prep.hl.vocab, 2, CONTEXT as usize, 7);
        let quiet = |_: usize, _: usize| {};
        let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).unwrap();
        let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).unwrap();
        let dir = std::env::temp_dir().join(format!("palw-sdk-tir-fused-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lowered = dir.join("q35.palwtir");
        let meta = serde_json::json!({ "calibrated_context": CONTEXT, "model_id": "test/qwen3_5-tiny" });
        artifact::write(&lowered, &prep.lowered.program, &mat.params, [0u8; 64], meta).unwrap();
        let declared = dir.join("q35.class.palwtir");
        let net = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &net.palw_consensus_mode else { panic!() };
        let choice = crate::tir_layout::TirLayoutChoiceV1 { max_context: Some(CONTEXT), ..Default::default() };
        crate::tir_layout::tir_declare_layout_v1(&net, bundle, &lowered, &declared, &choice, None).expect("declared");
        let entry = TirLineageV1::open_entry(&declared).expect("an IR class");
        let mut exec = misaka_palw_tir_exec::TirExecutor::new(entry.artifact.plan(), entry.artifact.params()).unwrap();
        exec.set_fused(true);
        assert!(exec.fused_summary().iter().any(|(_, regions)| *regions > 0), "a kernel matched: {:?}", exec.fused_summary());
        let form = PalwPromptIdsFormV1::Flat;
        let off = TirLineageV1::backend(&entry, &court(), form).unwrap().with_fused_kernels(false);
        let on = TirLineageV1::backend(&entry, &court(), form).unwrap().with_fused_kernels(true);
        assert!(on.fused_kernels() && !off.fused_kernels());
        let (job, prompt) = PalwExecutionBackendV1::job_for_anchor(&off, Hash64::from_bytes([7; 64])).unwrap();
        let (a, b) = (
            PalwExecutionBackendV1::execute(&off, &job, &prompt).unwrap(),
            PalwExecutionBackendV1::execute(&on, &job, &prompt).unwrap(),
        );
        assert_eq!((a.execution_root, a.trace_root), (b.execution_root, b.trace_root), "the same roots");
        assert_eq!(a.material, b.material, "the same capture, byte for byte");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **The calibration-length rule at declare-layout** (freeze-v1): a recurrent program is declared
    /// at no longer a context than it was calibrated on, unless the conversion waived the rule, and
    /// one that records no calibration length is refused; an attention-only program is not bound.
    #[test]
    fn a_recurrent_class_is_declared_within_its_calibration() {
        use crate::tir_layout::tir_calibration_covers_context_v1 as covers;
        use misaka_palw_tir::StateKind;
        let all = vectors();
        let recurrent = all.iter().find(|(_, p, _)| p.states.iter().any(|s| matches!(s.kind, StateKind::Fixed { .. })));
        let attention = all.iter().find(|(_, p, _)| p.states.iter().all(|s| !matches!(s.kind, StateKind::Fixed { .. })));
        let (_, rec, _) = recurrent.expect("a golden program with a fixed state");
        let (_, att, _) = attention.expect("an attention-only golden program");
        let meta = |v: serde_json::Value| v;
        assert!(covers(rec, &meta(serde_json::json!({ "calibrated_context": 64 })), 64).is_ok());
        let err = covers(rec, &meta(serde_json::json!({ "calibrated_context": 32 })), 64).unwrap_err();
        assert!(err.contains("at most 32 positions"), "{err}");
        assert!(covers(rec, &meta(serde_json::json!({})), 16).unwrap_err().contains("records no calibrated context"));
        let waived = serde_json::json!({ "calibrated_context": 8, "calibration_length_rule": { "rule": "waived", "longest": 8, "context": 16 } });
        assert!(covers(rec, &waived, 64).is_ok(), "waived at conversion");
        let met =
            serde_json::json!({ "calibrated_context": 8, "calibration_length_rule": { "rule": "met", "longest": 8, "context": 8 } });
        assert!(covers(rec, &met, 64).is_err(), "met at 8 is not a waiver for 64");
        assert!(covers(att, &meta(serde_json::json!({})), 1 << 18).is_ok(), "attention is not bound");
    }

    /// **An IR family's certification, as the node's operator files it** (`palw-class certify`):
    /// the class's drill in the chain's evidence form, graded by the chain's certifier into a
    /// family over every primitive the program reaches, reproducible byte for byte, and the lane
    /// object that seats the class.
    #[test]
    fn an_ir_family_certification_is_built_and_graded() {
        use crate::tir_certification::{tir_family_certification_v1, tir_lane_certification_v1};
        use kaspa_consensus_core::palw_state_v2::{PalwCertificationEvidenceV1, PalwConsensusObjectV2};
        let dir = std::env::temp_dir().join(format!("palw-sdk-tir-cert-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sdk = PalwClassSdk::builtin_v1(court(), PalwPromptIdsFormV1::Flat, b"misaka-palw-rc".to_vec());
        let (name, p, params) = vectors().remove(0);
        // The narrowest context a class may declare: the drill runs its canonical job once per
        // committed unit and call class, both ways.
        let narrow = PalwTirLayoutV1 { max_context: 16, ..layout(&p) };
        let path = write(&dir, &name, &p, &params, borsh::to_vec(&narrow).unwrap());
        let holdings = vec![sdk.load_artifact(&path).unwrap()];
        let entry = crate::tir_registration::tir_entries_of_v1(&holdings).remove(0);
        let (object, family) = tir_family_certification_v1(&entry, &court(), PalwPromptIdsFormV1::Flat, None).expect("certifies");
        let PalwConsensusObjectV2::FamilyCertified { evidence } = &object else { panic!("a FamilyCertified") };
        let PalwCertificationEvidenceV1::TirAttempt(drill) = evidence.as_ref() else { panic!("the IR attempt lane") };
        assert!(!drill.vectors.is_empty() && drill.malformed_inputs_refused > 0);
        assert_eq!(family.drilled_class_id, entry.class_id());
        assert_eq!(family.kernel_ids, kaspa_consensus_core::palw_tir_admission_v1::palw_tir_reachable_prims_v1(&p));
        assert_eq!(evidence.grade().expect("the chain's grader"), family, "the family is the grader's");
        // A family named by the operator; otherwise one build over one artifact writes one object.
        let named = Hash64::from_bytes([5; 64]);
        let (again, again_family) = tir_family_certification_v1(&entry, &court(), PalwPromptIdsFormV1::Flat, Some(named)).unwrap();
        assert_eq!(again_family.family_id, named);
        let PalwConsensusObjectV2::FamilyCertified { evidence: again } = again else { panic!() };
        let PalwCertificationEvidenceV1::TirAttempt(mut again) = *again else { panic!() };
        again.family_id = drill.family_id;
        assert_eq!(borsh::to_vec(&again).unwrap(), borsh::to_vec(drill).unwrap(), "the drill is reproducible");
        let PalwConsensusObjectV2::ClassLaneCertifiedTirV1 { class_id, artifact_root, class } = tir_lane_certification_v1(&entry)
        else {
            panic!("the lane object")
        };
        assert_eq!((class.class_id(&artifact_root), class_id), (entry.class_id(), entry.class_id()));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **An IR registration, built, signed and read back** (F6's node half): the candidate is the
    /// held class, dropped once the chain holds its id or its weights; the object carries the class,
    /// the formula's canonical job and the counted pwu; the signature verifies under the registrant
    /// bond's key over the message of the object's own fields; and the gate refuses a network that
    /// has not armed `palw_tir_v1`, or one where it is not yet in force.
    #[test]
    fn an_ir_registration_is_built_signed_and_gated() {
        use crate::tir_registration::{build_tir_registration_v1, tir_registration_candidate_v1, tir_registration_message_v1};
        use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
        use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
        use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwRegistrationTermsV2};
        use kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1;
        use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1;
        use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
        use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

        let dir = std::env::temp_dir().join(format!("palw-sdk-tir-reg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sdk = PalwClassSdk::builtin_v1(court(), PalwPromptIdsFormV1::Flat, b"misaka-palw-rc".to_vec());
        let (name, p, params) = vectors().remove(0);
        let path = write(&dir, &name, &p, &params, borsh::to_vec(&layout(&p)).unwrap());
        let holdings = vec![sdk.load_artifact(&path).unwrap()];
        let mut terms = PalwRegistrationTermsV2 {
            min_grantable_share_permille: 1,
            slash_value_per_pwu: 7,
            initial_target: 1 << 100,
            registered_class_ids: Vec::new(),
            registered_artifact_roots: Vec::new(),
            chain_certified_families: Vec::new(),
        };
        let entry = tir_registration_candidate_v1(&holdings, &terms, None).unwrap();
        assert_eq!(tir_registration_candidate_v1(&holdings, &terms, Some(&entry.model_id)).unwrap().class_id(), entry.class_id());
        assert!(tir_registration_candidate_v1(&holdings, &terms, Some(&entry.class_id().to_string())).is_ok(), "by class id too");
        assert!(tir_registration_candidate_v1(&holdings, &terms, Some("another/model")).unwrap_err().contains("names no IR class"));

        // A network that armed the IR fence at 100.
        let mut net = palw_t12_shipped_params();
        net.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(100)));
        net.sync_palw_tir_v1();
        let PalwConsensusMode::ConsensusV2(bundle) = &net.palw_consensus_mode else { panic!("t12 is V2") };
        let bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([4; 64]), 0));
        let build = |sig: Vec<u8>, daa: u64| build_tir_registration_v1(&net, bundle, &entry, &terms, 0, bond, sig, daa);
        let below = build(Vec::new(), 99).unwrap_err();
        assert!(below.contains("FAMILY_FENCE_CLOSED"), "below the fence nothing is built: {below}");
        let unarmed = palw_t12_shipped_params();
        let PalwConsensusMode::ConsensusV2(b2) = &unarmed.palw_consensus_mode else { panic!() };
        let never = build_tir_registration_v1(&unarmed, b2, &entry, &terms, 0, bond, Vec::new(), 1_000).unwrap_err();
        assert!(never.contains("FAMILY_FENCE_CLOSED"), "an unarmed network registers no IR class: {never}");

        let unsigned = build(Vec::new(), 100).unwrap();
        let PalwConsensusObjectV2::ClassRegisteredTirV1 {
            class_id,
            artifact_root,
            pwu_rule,
            share_permille,
            slash_value_per_pwu,
            admission,
            ..
        } = &unsigned
        else {
            panic!("an IR registration")
        };
        assert_eq!((*class_id, *artifact_root), (entry.class_id(), entry.artifact_root));
        assert_eq!((*share_permille, *slash_value_per_pwu), (0, 7), "weightless, at the network's pricing");
        assert_eq!(admission.class, *entry.class);
        assert_eq!(admission.canonical, entry.canonical_context());
        let space = PalwTirStepSpaceV1::new(&entry.class).unwrap();
        let counted = space.leaf_count_capped(&admission.canonical, bundle.court.max_step_leaf_count()).unwrap();
        assert_eq!(*pwu_rule, PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted });

        // Signed by the bond's ML-DSA-87 key over the object's own message; the signed object is
        // the unsigned one plus the signature, and verifies.
        let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([9u8; 32]);
        let domain = Hash64::from_bytes([0x5D; 64]);
        let message = tir_registration_message_v1(domain, &unsigned).unwrap();
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &keys.signing_key,
            message.as_byte_slice(),
            PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1,
            [3u8; 32],
        )
        .unwrap();
        let signed = build(signature.as_ref().to_vec(), 100).unwrap();
        assert_eq!(tir_registration_message_v1(domain, &signed), Some(message), "the signature is not in its own message");
        let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = &signed else { unreachable!() };
        let verified = kaspa_txscript::verify_mldsa87_with_context(
            keys.verification_key.as_ref(),
            message.as_byte_slice(),
            &admission.signature,
            PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1,
        );
        assert_eq!(verified, Ok(true), "the bond's key verifies the registration");

        // Once the chain holds the class — or its weights — it is no candidate.
        terms.registered_artifact_roots.push(entry.artifact_root);
        assert!(tir_registration_candidate_v1(&holdings, &terms, None).unwrap_err().contains("already registered"));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The battery refuses what the IR admission gate would: a checkpoint interval past the
    /// `min_j C_j` the states' replay fits in.
    #[test]
    fn the_battery_names_an_inadmissible_ir_class() {
        let dir = std::env::temp_dir().join(format!("palw-sdk-tir-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lineage = TirLineageV1::new();
        let mut refused = 0;
        for (name, p, params) in vectors() {
            let mut lay = layout(&p);
            lay.checkpoint_interval = 1 << 17;
            let path = write(&dir, &name, &p, &params, borsh::to_vec(&lay).unwrap());
            if lineage.load(&path, crate::lineage::PalwWeightResidencyV1::PageCache).is_ok() {
                let err = crate::conformance::check_lineage_v1(&lineage, &court()).unwrap_err();
                assert!(err.contains("checkpoint interval"), "{name}: {err}");
                refused += 1;
                break;
            }
        }
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(refused, 1);
    }
}
