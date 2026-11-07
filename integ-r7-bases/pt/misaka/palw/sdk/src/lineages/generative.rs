//! **`GenLineageV1`: generative (pipeline) classes** (RFC-0003 §II.2.1; activation step 7's node half).
//!
//! The container is `PALWTIR2` (`misaka_palw_tir_artifact::PalwTirContainerV2`): the pipeline, every
//! program, the declared class (its layouts, output header, offers) and the tokenizer id in the header,
//! every param instance behind it. `load` opens and checks it, decodes the class it declares, streams the
//! pipeline inventory root ONCE (the root is this node's proof that it holds what the chain registered —
//! never read from a sidecar here) and derives the row the chain writes for that class and root
//! (`palw_gen_class_record_v1`). A generative class is DATA — the class the artifact declares — so the
//! lineage's classes are the artifacts it has loaded ([`PalwModelLineageV1::gen_classes`]), not a table; it
//! has no legacy class and pairs with no legacy entry.
//!
//! `resolve` serves a chain-named `(class_id, artifact_root)` from a held artifact that derives exactly
//! that pair, as a [`GenBackendV1`]. The backend is the readiness door (a seat proves it holds the class
//! by opening the drawn leaves of its pipeline inventory — the same multiproof every class kind proves
//! with) and the carrier of the node's generative verbs, which are `misaka_palw_base0::gen_tensor_worker`'s
//! (the tensor worker, a seat's replay, a court's moves): a generative class serves **jobs a requester
//! names**, never an anchor's, so the attempt lane's verbs (`job_for_anchor`, `execute`) refuse by name.

use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

use kaspa_consensus_core::palw_artifact::{
    PalwArtifactMerkleFrontierV1, PalwArtifactOpeningV1, PalwArtifactOperandV1, artifact_leaf_parts_v1,
};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_gen_artifact_v1::{palw_gen_inventory_root_v1, palw_gen_open_leaves_v1, palw_gen_visit_inventory_v1};
use kaspa_consensus_core::palw_gen_class_v1::{PalwGenClassV1, palw_gen_class_record_v1};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;
use misaka_palw_base0::gen_worker::GenHeldClassV1;
use misaka_palw_tir_artifact::PalwTirContainerV2;

use crate::lineage::{PalwClassEntryV1, PalwGenClassEntryV1, PalwLoadedArtifactV1, PalwModelLineageV1};

/// The lineage's id.
pub const GEN_LINEAGE_ID_V1: &str = "palw-gen-v1";

/// **The generative lineage.** Holds the classes of the artifacts it loaded.
#[derive(Default)]
pub struct GenLineageV1 {
    held: RwLock<Vec<PalwGenClassEntryV1>>,
}

/// The model id an artifact names: its provenance's `model_id` (or `model`) when the container's JSON meta
/// carries one, else the file's stem.
fn model_id_of(meta: &str, path: &Path) -> String {
    serde_json::from_str::<serde_json::Value>(meta)
        .ok()
        .and_then(|v| ["model_id", "model"].iter().find_map(|k| v.get(*k).and_then(|m| m.as_str()).map(str::to_string)))
        .unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "palw-gen".into()))
}

impl GenLineageV1 {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open one `PALWTIR2` file into the entry of the class it declares: the container's header is the
    /// declared class's (pipeline, programs, tokenizer), its tensors hash to the root the entry carries, and
    /// the row is the one a registration of that class under that root writes.
    pub fn open_entry(path: &Path) -> Result<PalwGenClassEntryV1, String> {
        let container = PalwTirContainerV2::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let h = &container.header;
        if h.class.is_empty() {
            return Err(format!(
                "{}: a generative class needs a declared class in its container (`palw-class declare-layout` writes one)",
                path.display()
            ));
        }
        let class: PalwGenClassV1 =
            borsh::from_slice(&h.class).map_err(|e| format!("{}: the declared class does not decode: {e}", path.display()))?;
        if class.pipeline != h.pipeline || class.programs != h.programs {
            return Err(format!("{}: the declared class's pipeline or programs are not the container's", path.display()));
        }
        if h.tokenizer_id != class.tokenizer_id.as_bytes() {
            return Err(format!("{}: the container's tokenizer is not the declared class's", path.display()));
        }
        let (artifact_root, _) = palw_gen_inventory_root_v1(&container.programs, &container)
            .map_err(|e| format!("{}: the weights have no pipeline inventory: {e}", path.display()))?;
        let row = palw_gen_class_record_v1(&class, &artifact_root).map_err(|e| format!("{}: {e}", path.display()))?;
        let model_id = model_id_of(&h.meta, path);
        let (programs, pipeline) = row.class.decode().map_err(|e| format!("{}: {e}", path.display()))?;
        // The class held over the container the inventory was just rooted from: nothing is read twice, and
        // nothing is claimed that was not hashed (`GenHeldClassV1::hold` would walk the weights again).
        let held = GenHeldClassV1 { row: row.clone(), pipeline, programs, params: container };
        Ok(PalwGenClassEntryV1 {
            model_id,
            lineage_id: GEN_LINEAGE_ID_V1,
            artifact_root,
            row,
            held: Arc::new(held),
            path: Some(path.to_path_buf()),
        })
    }

    /// The backend for one held entry, running under the network's prompt-commitment form.
    pub fn backend(entry: &PalwGenClassEntryV1, _court: &PalwCourtParamsV2, prompt_ids_form: PalwPromptIdsFormV1) -> GenBackendV1 {
        GenBackendV1::new(entry.clone(), prompt_ids_form)
    }
}

impl PalwModelLineageV1 for GenLineageV1 {
    fn lineage_id(&self) -> &'static str {
        GEN_LINEAGE_ID_V1
    }

    /// No legacy class: a pipeline class has no `PalwShapeProfileV3`.
    fn classes(&self, _court: &PalwCourtParamsV2) -> Vec<PalwClassEntryV1> {
        Vec::new()
    }

    fn gen_classes(&self) -> Vec<PalwGenClassEntryV1> {
        self.held.read().expect("the held list is never poisoned").clone()
    }

    fn sniffs(&self, head: &[u8; 8]) -> bool {
        head == misaka_palw_tir_artifact::PALW_TIR_CONTAINER_MAGIC_V2
    }

    /// Rooted, checked and remembered, so the class is one of this lineage's.
    fn load(&self, path: &Path, _residency: crate::lineage::PalwWeightResidencyV1) -> Result<PalwLoadedArtifactV1, String> {
        let entry = Self::open_entry(path)?;
        let summary = format!(
            "generative class {} ({}): {} programs, {} stages, output kind {}, artifact root {}",
            entry.class_id(),
            entry.model_id,
            entry.held.programs.len(),
            entry.held.pipeline.stages.len(),
            entry.row.class.output.kind,
            entry.artifact_root
        );
        {
            let mut held = self.held.write().expect("the held list is never poisoned");
            if !held.iter().any(|e| e.class_id() == entry.class_id()) {
                held.push(entry.clone());
            }
        }
        Ok(PalwLoadedArtifactV1::from_parts(GEN_LINEAGE_ID_V1, Some(path.to_path_buf()), summary, Arc::new(entry)))
    }

    fn registered_weight_keys(&self, artifact: &PalwLoadedArtifactV1) -> Vec<Hash64> {
        artifact.payload().downcast_ref::<PalwGenClassEntryV1>().map(|e| vec![e.artifact_root]).unwrap_or_default()
    }

    fn pair(&self, _court: &PalwCourtParamsV2, entry: &PalwClassEntryV1, _artifact: &PalwLoadedArtifactV1) -> Result<Hash64, String> {
        Err(format!("{}: a generative artifact is its own class and pairs with no legacy entry", entry.model_id))
    }

    fn resolve(
        &self,
        court: &PalwCourtParamsV2,
        prompt_ids_form: PalwPromptIdsFormV1,
        class_id: Hash64,
        artifact_root: Hash64,
        holdings: &[PalwLoadedArtifactV1],
        _network_id: &[u8],
    ) -> Option<Result<Box<dyn PalwExecutionBackendV1>, String>> {
        let entry = holdings
            .iter()
            .filter(|h| h.lineage_id == GEN_LINEAGE_ID_V1)
            .filter_map(|h| h.payload().downcast_ref::<PalwGenClassEntryV1>().cloned())
            .find(|e| e.class_id() == class_id && e.artifact_root == artifact_root)?;
        Some(Ok(Box::new(Self::backend(&entry, court, prompt_ids_form)) as Box<dyn PalwExecutionBackendV1>))
    }
}

/// **The backend of one held generative class.** The readiness door and the carrier of the generative
/// verbs; see the module doc.
pub struct GenBackendV1 {
    entry: PalwGenClassEntryV1,
    prompt_ids_form: PalwPromptIdsFormV1,
    /// The inventory's root and leaf count, streamed once per backend.
    inventory: OnceLock<Result<(Hash64, u32), String>>,
}

impl GenBackendV1 {
    pub fn new(entry: PalwGenClassEntryV1, prompt_ids_form: PalwPromptIdsFormV1) -> Self {
        Self { entry, prompt_ids_form, inventory: OnceLock::new() }
    }

    /// The class held: its row, pipeline, programs and weights (the generative verbs' input).
    pub fn held(&self) -> &GenHeldClassV1<PalwTirContainerV2> {
        &self.entry.held
    }

    /// The held entry (for a caller that keeps it beside the backend).
    pub fn entry(&self) -> &PalwGenClassEntryV1 {
        &self.entry
    }

    pub fn class_id(&self) -> Hash64 {
        self.entry.class_id()
    }

    pub fn artifact_root(&self) -> Hash64 {
        self.entry.artifact_root
    }

    /// The network's prompt-commitment form (a tensor worker and a seat check the ids against it).
    pub fn prompt_ids_form(&self) -> PalwPromptIdsFormV1 {
        self.prompt_ids_form
    }

    // ---- The generative verbs (the node's passes call these; they are `gen_tensor_worker`'s) ----

    /// **A capture's inputs as the worker takes them** — the job, the ids and the images — replayed over the
    /// held class: the run, whatever the capture says it computed.
    pub fn replay(
        &self,
        capture: &misaka_palw_base0::gen_tensor_worker::GenTensorCaptureV1,
    ) -> Result<misaka_palw_base0::gen_tensor_worker::GenTensorWorkV1, String> {
        let images: Vec<misaka_palw_tir::pipeline::JobImageV1> =
            capture.images.iter().map(|i| misaka_palw_tir::pipeline::JobImageV1 { h: i.h, w: i.w, rgb: i.rgb.clone() }).collect();
        self.held().run_tensor(&capture.job, &capture.prompt, &capture.negative, &images, self.prompt_ids_form)
    }

    /// **A replay's roots, as a seat compares them to its claim's**: the tensor execution root, the step root,
    /// the step leaf count and the canonical output's root.
    pub fn replay_roots(
        &self,
        capture: &misaka_palw_base0::gen_tensor_worker::GenTensorCaptureV1,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwReplayRootsV1, String> {
        let work = self.replay(capture)?;
        Ok(kaspa_consensus_core::palw_backend::PalwReplayRootsV1 {
            execution_root: work.execution_root(),
            trace_root: work.binding.step_root(),
            work_leaves: Some(work.binding.step_leaf_count),
            output_root: Some(work.binding.output_root),
        })
    }

    /// **The accused's execution, rebuilt from its capture** (lies included): the roots the capture's leaves
    /// and output commit to, whatever computed them.
    pub fn rebuild(
        &self,
        capture: &misaka_palw_base0::gen_tensor_worker::GenTensorCaptureV1,
    ) -> Result<misaka_palw_base0::gen_tensor_worker::GenTensorWorkV1, String> {
        capture.rebuild(self.held())
    }

    /// **The moves a party may file at leaf `index` of the accused's execution** (see
    /// `gen_tensor_court_candidates_v1`).
    pub fn court_candidates(
        &self,
        accused: &misaka_palw_base0::gen_tensor_worker::GenTensorWorkV1,
        index: u64,
        challenger: bool,
        limits: &misaka_palw_tir::demand::DemandLimits,
    ) -> Vec<(&'static str, Result<misaka_palw_base0::gen_worker::GenCourtMoveV1, String>)> {
        misaka_palw_base0::gen_tensor_worker::gen_tensor_court_candidates_v1(self.held(), accused, index, challenger, limits)
    }

    /// **The first leaf, in the claim's one order, where the accused's commitments part from an honest run
    /// of the same job** (the leaf a challenger disputes).
    pub fn first_divergence(
        &self,
        accused: &misaka_palw_base0::gen_tensor_worker::GenTensorWorkV1,
        own: &misaka_palw_base0::gen_tensor_worker::GenTensorWorkV1,
    ) -> Option<u64> {
        misaka_palw_base0::gen_worker::gen_execution_first_divergence_v1(&accused.execution, &own.execution)
    }

    /// **The first output tile whose canonical bytes are not the accused's own committed step tile's** (see
    /// `gen_tensor_output_audit_v1`): `(tile, the step tile's global leaf, the fault)`.
    pub fn output_audit(
        &self,
        accused: &misaka_palw_base0::gen_tensor_worker::GenTensorWorkV1,
    ) -> Option<(u64, u64, kaspa_consensus_core::palw_step_leg::PalwStepFaultV1)> {
        misaka_palw_base0::gen_tensor_worker::gen_tensor_output_audit_v1(self.held(), accused)
    }

    /// **Run a request frame** (the worker's one step: a request in, the binding or a refusal out) over the held
    /// class under the network's prompt-commitment form.
    pub fn answer(
        &self,
        request: &[u8],
    ) -> (misaka_palw_base0::gen_tensor_worker::PalwGenTensorAnswerV1, Option<misaka_palw_base0::gen_tensor_worker::GenTensorWorkV1>)
    {
        misaka_palw_base0::gen_tensor_worker::gen_tensor_answer_v1(self.held(), request, self.prompt_ids_form)
    }

    /// The readiness walk: the one pass over the inventory the root, every leaf hash and the drawn leaves'
    /// operands are read from. `on_leaf` takes each leaf's hash in inventory order; the drawn operands come
    /// back in the draw's order. `(root, leaf_count, drawn)`.
    #[allow(clippy::type_complexity)]
    fn walk_readiness(
        &self,
        draw: &[u32],
        on_leaf: &mut dyn FnMut(Hash64),
    ) -> Result<(Hash64, u32, Vec<(u32, PalwArtifactOperandV1)>), String> {
        let held = self.held();
        let wanted: std::collections::BTreeSet<u32> = draw.iter().copied().collect();
        let mut kept: std::collections::BTreeMap<u32, PalwArtifactOperandV1> = std::collections::BTreeMap::new();
        let mut frontier = PalwArtifactMerkleFrontierV1::new();
        let mut index = 0u32;
        let count = palw_gen_visit_inventory_v1(&held.programs, &held.params, &mut |name, layer, start, piece| {
            let leaf = artifact_leaf_parts_v1(name, layer, start, piece);
            frontier.push(leaf);
            on_leaf(leaf);
            if wanted.contains(&index) {
                kept.insert(
                    index,
                    PalwArtifactOperandV1 { tensor_name: name.to_string(), layer, row_start: start, bytes: piece.to_vec() },
                );
            }
            index += 1;
        })
        .map_err(|e| e.to_string())?;
        let root = frontier.root().ok_or("the class's inventory is empty")?;
        let drawn = draw
            .iter()
            .map(|i| {
                kept.get(i).cloned().map(|operand| (*i, operand)).ok_or_else(|| format!("leaf {i} is outside an inventory of {count}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((root, count, drawn))
    }
}

impl PalwExecutionBackendV1 for GenBackendV1 {
    fn model_id(&self) -> &str {
        &self.entry.model_id
    }

    /// A generative class's jobs are the requester's, never an anchor's: the attempt lane has none.
    fn job_for_anchor(&self, _anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
        Err("a generative class serves the jobs a requester names (a tensor job), never an anchor's".to_string())
    }

    fn execute(&self, _job: &PalwJobContextV2, _prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        Err("a generative class has no attempt lane: its claims are tensor jobs on the free-prompt lane".to_string())
    }

    fn verify_material(&self, _material: &[u8], _claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        PalwMaterialVerdictV1::Unverifiable
    }

    fn artifact_root_and_leaf_count(&self) -> Result<(Hash64, u32), String> {
        self.inventory
            .get_or_init(|| {
                let held = self.held();
                palw_gen_inventory_root_v1(&held.programs, &held.params).map_err(|e| e.to_string())
            })
            .clone()
    }

    /// The readiness material from the held inventory: its root, every leaf hash, and the drawn leaves'
    /// operands in the draw's order.
    fn artifact_readiness_material(&self, draw: &[u32]) -> Result<(Hash64, Vec<Hash64>, Vec<(u32, PalwArtifactOperandV1)>), String> {
        let mut leaves = Vec::new();
        let (root, _, drawn) = self.walk_readiness(draw, &mut |leaf| leaves.push(leaf))?;
        Ok((root, leaves, drawn))
    }

    fn artifact_readiness_material_streamed_v1(
        &self,
        draw: &[u32],
        on_leaf: &mut dyn FnMut(Hash64),
    ) -> Option<Result<(Hash64, u32, Vec<(u32, PalwArtifactOperandV1)>), String>> {
        Some(self.walk_readiness(draw, on_leaf))
    }

    fn artifact_row_opening(&self, index: u32) -> Result<PalwArtifactOpeningV1, String> {
        let held = self.held();
        palw_gen_open_leaves_v1(&held.programs, &held.params, [index])
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .ok_or_else(|| format!("inventory leaf {index} does not open"))
    }
}
