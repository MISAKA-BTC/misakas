//! Actual reference39 producer. Decode is computed online from original typed
//! parameters/materials. Only then are actual firstEOS inputs bound and the
//! full all-node trace independently reconstructed. No future padding IDs or
//! producer DTO can fabricate a native Final through this API.
use misaka_palw_kernel::{
    flat_detect39::IndependentArchiveV39,
    flat_job39::{
        ActualInputResolverV39, CheckedJobMaximumV39, GenerationInputProviderV39, InputBindingV39, MaterialManifestV39, RequestStopV39,
    },
    flat_material39::{MaterialDescriptorV39, MaterialPageReaderV39, MaterialPageV39},
    flat_source39::CheckedSourceV39,
    flat_trace39::{TraceSummaryV39, trace_streamed_v39},
    hash::Digest,
};
use misaka_palw_tir::{
    interp::RunState,
    interp_v39::{InterpreterV39, ParamSourceV39},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
pub struct ProducedReferenceV39 {
    pub generated: Vec<u32>,
    pub trace: TraceSummaryV39,
}
pub fn produce_reference_v39<A: IndependentArchiveV39>(
    source: &CheckedSourceV39,
    maximum: &CheckedJobMaximumV39,
    materials: &dyn MaterialPageReaderV39,
    parameters: &dyn ParamSourceV39,
    archive: &mut A,
) -> Result<ProducedReferenceV39, String> {
    let mut generated = Vec::new();
    if let RequestStopV39::Decode { prompt, requested_new_tokens, eos } = &maximum.request().stop {
        let provider = GenerationInputProviderV39::derive(source, maximum, materials)?;
        let interpreter = InterpreterV39::new(source.program()).map_err(|e| e.to_string())?;
        let mut state = RunState::default();
        let mut last = None;
        for token in prompt {
            let output = interpreter
                .step_with_observer(parameters, &provider, &mut state, *token, &mut |_| Ok(()))
                .map_err(|e| e.to_string())?;
            last = Some(output);
        }
        let mut logits = last.ok_or("producer39 original prompt is empty")?;
        for index in 0..*requested_new_tokens {
            if logits.data.len() != source.program().token_bound as usize {
                return Err("producer39 original logits vocabulary extent".into());
            }
            let selected = logits
                .data
                .iter()
                .enumerate()
                .max_by(|(i, a), (j, b)| a.cmp(b).then_with(|| j.cmp(i)))
                .map(|(i, _)| i as u32)
                .ok_or("producer39 original logits are empty")?;
            generated.push(selected);
            if eos.binary_search(&selected).is_ok() || index + 1 == *requested_new_tokens {
                break;
            }
            logits = interpreter
                .step_with_observer(parameters, &provider, &mut state, selected, &mut |_| Ok(()))
                .map_err(|e| e.to_string())?;
        }
    }
    let actual = ActualInputResolverV39::derive(source, maximum, &generated, materials)?;
    archive.begin(source.root(), maximum.root(), actual.root())?;
    let trace = trace_streamed_v39(source, maximum, &actual, parameters, archive)?;
    archive.finish(&trace)?;
    // Greedy selection is checked again at the original prediction positions by
    // trace_streamed39, including the first EOS and exact source input binding.
    if trace.claim.input_binding != actual.root() || trace.claim.positions as usize != actual.tokens().len() {
        return Err("producer39 actual trace/input binding differs".into());
    }
    Ok(ProducedReferenceV39 { generated, trace })
}
/// A concrete local page store, bound to the immutable original input manifest.
/// Every returned page is authenticated again by the shared39 input resolver.
pub struct MaterialDirectoriesV39 {
    directories: BTreeMap<Digest, PathBuf>,
}
impl MaterialDirectoriesV39 {
    pub fn open(mapping: &BTreeMap<String, String>, manifest: &MaterialManifestV39, root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let mut expected = BTreeSet::new();
        let mut directories = BTreeMap::new();
        for binding in &manifest.bindings {
            if let InputBindingV39::Material { descriptor, .. } = binding {
                let id = descriptor.root();
                if !expected.insert(id) {
                    continue;
                }
                let name = crate::runtime_pack::commit::hex(&id);
                let relative = mapping.get(&name).ok_or("producer39 original material directory missing")?;
                let directory = root.join(relative).canonicalize().map_err(|e| e.to_string())?;
                if !directory.starts_with(&root) {
                    return Err("producer39 material path escapes pinned root".into());
                }
                let raw = crate::native_audit_io_v32::read_bounded_file_v32(&directory.join("descriptor39.borsh"), 2048)?;
                let stored = MaterialDescriptorV39::from_bytes(&raw)?;
                if stored != *descriptor {
                    return Err("producer39 stored material descriptor differs from original maximum".into());
                }
                directories.insert(id, directory);
            }
        }
        if expected.len() != mapping.len() {
            return Err("producer39 material directory set has unbound entries".into());
        }
        Ok(Self { directories })
    }
}
impl MaterialPageReaderV39 for MaterialDirectoriesV39 {
    fn page(&self, descriptor: Digest, index: u16) -> Result<MaterialPageV39, String> {
        let directory = self.directories.get(&descriptor).ok_or("producer39 material descriptor absent")?;
        let path = directory.join(format!("page-{index:04}.borsh"));
        let raw = crate::native_audit_io_v32::read_bounded_file_v32(&path, (32 << 10) + 2048)?;
        let page = MaterialPageV39::from_bytes(&raw)?;
        if page.index != index {
            return Err("producer39 original material page index differs".into());
        }
        Ok(page)
    }
}

/// All input files are independently pinned before execution. Parameter payloads
/// are checked against the original real PC instead of trusting directory names.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedProducerFileV39 {
    pub path: String,
    pub sha256: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerPlanV39 {
    pub version: u16,
    pub source: PinnedProducerFileV39,
    pub maximum: PinnedProducerFileV39,
    pub materials: PinnedProducerFileV39,
    pub material_directories: PinnedProducerFileV39,
    pub parameter_directory: String,
    /// None selects the original PALWQ361 writer's names. Some selects the
    /// complete frontend writer's exact stage-indexed parameter names.
    pub parameter_stage: Option<u16>,
    pub parameter_workspace_bytes: u64,
    pub parameter_read_elements: usize,
    pub archive_disk_bytes: u64,
    /// Original funded job identity; the native fold still checks funding.
    pub job: String,
}
fn decode_hex<const N: usize>(raw: &str) -> Result<[u8; N], String> {
    misaka_palw_tir_lower::frontend_pack::program::unhex(raw, N)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "producer39 original digest extent".into())
}
pub(crate) fn relative_file(root: &Path, path: &str) -> Result<PathBuf, String> {
    if Path::new(path).is_absolute() {
        return Err("producer39 input paths must be relative to original input root".into());
    }
    let path = root.join(path).canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(root) {
        return Err("producer39 original input path escapes pinned root".into());
    }
    Ok(path)
}
pub(crate) fn pinned_input(root: &Path, file: &PinnedProducerFileV39, cap: usize) -> Result<Vec<u8>, String> {
    use sha2::{Digest as _, Sha256};
    let raw = crate::native_audit_io_v32::read_bounded_file_v32(&relative_file(root, &file.path)?, cap)?;
    if <[u8; 32]>::from(Sha256::digest(&raw)) != decode_hex::<32>(&file.sha256)? {
        return Err("producer39 independently pinned input SHA256 differs".into());
    }
    Ok(raw)
}
/// Concrete offline producer driver. Its canonical Commit is merely a signed
/// payload candidate until the genuine native fold accepts the funded job.
pub fn produce_from_plan_v39(
    plan_file: &Path,
    plan_sha256: [u8; 32],
    input_root: &Path,
    output: &Path,
) -> Result<serde_json::Value, String> {
    use crate::{full_component_v28::q36mirror40::SourceDirectoryV39, native_trace_archive_v39::DiskTraceArchiveV39};
    use kaspa_consensus_core::palw_native_job_v39::NativeCommitWireV39;
    use misaka_palw_kernel::{
        flat_job39::JobMaximumV39,
        flat_source39::{FlatSourceWireV39, SOURCE39_MAX_BYTES},
    };
    use sha2::{Digest as _, Sha256};
    let plan_raw = crate::native_audit_io_v32::read_bounded_file_v32(plan_file, 64 << 10)?;
    if <[u8; 32]>::from(Sha256::digest(&plan_raw)) != plan_sha256 {
        return Err("producer39 original producer plan SHA256 differs".into());
    }
    let plan: ProducerPlanV39 = serde_json::from_slice(&plan_raw).map_err(|e| e.to_string())?;
    if plan.version != 39 || output.exists() {
        return Err("producer39 requires new owning39 plan and fresh output directory".into());
    }
    let root = input_root.canonicalize().map_err(|e| e.to_string())?;
    let source = FlatSourceWireV39::from_bytes(&pinned_input(&root, &plan.source, SOURCE39_MAX_BYTES)?)?.check()?;
    let request = JobMaximumV39::from_bytes(&pinned_input(&root, &plan.maximum, 8 << 20)?)?;
    let material = MaterialManifestV39::from_bytes(&pinned_input(&root, &plan.materials, 64 << 10)?)?;
    let maximum = CheckedJobMaximumV39::check(&source, &request, &material)?;
    let mapping: BTreeMap<String, String> =
        serde_json::from_slice(&pinned_input(&root, &plan.material_directories, 64 << 10)?).map_err(|e| e.to_string())?;
    let materials = MaterialDirectoriesV39::open(&mapping, &material, &root)?;
    let parameter_directory = relative_file(&root, &plan.parameter_directory)?;
    let params = match plan.parameter_stage {
        Some(stage) => SourceDirectoryV39::open_stage(
            &source,
            &parameter_directory,
            stage,
            plan.parameter_workspace_bytes,
            plan.parameter_read_elements,
        )?,
        None => SourceDirectoryV39::open(&source, &parameter_directory, plan.parameter_workspace_bytes, plan.parameter_read_elements)?,
    };
    let job = decode_hex::<64>(&plan.job)?;
    if job == [0; 64] {
        return Err("producer39 original funded job identity absent".into());
    }
    std::fs::create_dir(output).map_err(|e| e.to_string())?;
    let mut archive = DiskTraceArchiveV39::create(&output.join("trace39"), &source, plan.archive_disk_bytes)?;
    let produced = produce_reference_v39(&source, &maximum, &materials, &params, &mut archive)?;
    let commit = NativeCommitWireV39 { version: 39, job, generated: produced.generated.clone(), claim: produced.trace.claim.clone() };
    let raw = borsh::to_vec(&commit).map_err(|e| e.to_string())?;
    NativeCommitWireV39::from_bytes(&raw, maximum.maximum_positions())?;
    std::fs::write(output.join("commit39.borsh"), &raw).map_err(|e| e.to_string())?;
    let hex = crate::runtime_pack::commit::hex;
    let report = serde_json::json!({
        "schema":"misaka.palw.native-reference-produced.v39", "version":39,
        "producer_plan_sha256":hex(&plan_sha256), "source":hex(&source.root()),
        "program":hex(&source.program_root()), "real_parameter":hex(&source.parameters().root()),
        "inventory":hex(&source.inventory_root()), "maximum":hex(&maximum.root()), "job":hex(&job),
        "maximum_positions":maximum.maximum_positions(), "actual_positions":produced.trace.claim.positions,
        "generated":produced.generated, "claim":hex(&produced.trace.claim.root()),
        "commit_sha256":hex(&<[u8;32]>::from(Sha256::digest(&raw))),
        "trace_disk_bytes":archive.stored_bytes(), "native_commit":"NOT_RUN", "native_final":"NOT_RUN",
        "independent_detection":"NOT_RUN", "source_fidelity":"NOT_RUN", "a6_eligible":false,"a8_eligible":false
    });
    std::fs::write(output.join("produced39.json"), serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(report)
}

/// A concrete replay/public-reply driver over original owning39 files. This
/// performs arithmetic replay and local court checking; it grants no native
/// verdict, validator approval, model epoch, or coinbase eligibility.
pub fn audit_from_plan_v39(
    plan_file: &Path,
    plan_sha256: [u8; 32],
    input_root: &Path,
    post_file: &PinnedProducerFileV39,
    class_file: &PinnedProducerFileV39,
    owning_params: &kaspa_consensus_core::config::params::Params,
    producer_archive: &Path,
    output: &Path,
    cache_disk_bytes: u64,
    independent_disk_bytes: u64,
    demand: Option<&[u8]>,
) -> Result<serde_json::Value, String> {
    use crate::{
        full_component_v28::q36mirror40::SourceDirectoryV39, native_public_data_v39::PublicDataV39,
        native_trace_archive_v39::DiskTraceArchiveV39,
    };
    use kaspa_consensus_core::{palw_native_execution_v39::NativeClassWireV39, palw_native_job_v39::NativeCommitWireV39};
    use misaka_palw_kernel::{
        flat_detect39::*,
        flat_job39::JobMaximumV39,
        flat_point39::CheckedPointContextV39,
        flat_public39::FlatDemandV39,
        flat_quota11::owning39::derive_quota_v39,
        flat_source39::{FlatSourceWireV39, SOURCE39_MAX_BYTES},
    };
    use sha2::{Digest as _, Sha256};
    let plan_raw = crate::native_audit_io_v32::read_bounded_file_v32(plan_file, 64 << 10)?;
    if <[u8; 32]>::from(Sha256::digest(&plan_raw)) != plan_sha256 {
        return Err("auditor39 independently pinned original producer plan differs".into());
    }
    let plan: ProducerPlanV39 = serde_json::from_slice(&plan_raw).map_err(|e| e.to_string())?;
    if plan.version != 39 || output.exists() {
        return Err("auditor39 requires owning39 plan/fresh output".into());
    }
    let root = input_root.canonicalize().map_err(|e| e.to_string())?;
    let source = FlatSourceWireV39::from_bytes(&pinned_input(&root, &plan.source, SOURCE39_MAX_BYTES)?)?.check()?;
    let request = JobMaximumV39::from_bytes(&pinned_input(&root, &plan.maximum, 8 << 20)?)?;
    let material = MaterialManifestV39::from_bytes(&pinned_input(&root, &plan.materials, 64 << 10)?)?;
    let maximum = CheckedJobMaximumV39::check(&source, &request, &material)?;
    let mapping: BTreeMap<String, String> =
        serde_json::from_slice(&pinned_input(&root, &plan.material_directories, 64 << 10)?).map_err(|e| e.to_string())?;
    let materials = MaterialDirectoriesV39::open(&mapping, &material, &root)?;
    let post = NativeCommitWireV39::from_bytes(&pinned_input(&root, post_file, 9 << 20)?, maximum.maximum_positions())?;
    if post.job != decode_hex::<64>(&plan.job)? {
        return Err("auditor39 original job differs from post".into());
    }
    let actual = ActualInputResolverV39::derive(&source, &maximum, &post.generated, &materials)?;
    let context = CheckedPointContextV39::check(&source, &maximum, post.claim.root(), &post.claim, &actual)?;
    owning_params.validate_palw_native_execution_v39().map_err(|e| e.to_string())?;
    let policies = &owning_params.palw_native_execution_v39.as_ref().ok_or("auditor39 owning native policy absent")?.policies;
    let class = NativeClassWireV39::from_bytes(&pinned_input(&root, class_file, 262)?)?.check(&source, policies)?;
    let quota = derive_quota_v39(&source, &maximum, class.root(), policies.root(), class.record().max_positions, 32)?;
    let params_path = relative_file(&root, &plan.parameter_directory)?;
    let parameters = match plan.parameter_stage {
        Some(stage) => {
            SourceDirectoryV39::open_stage(&source, &params_path, stage, plan.parameter_workspace_bytes, plan.parameter_read_elements)?
        }
        None => SourceDirectoryV39::open(&source, &params_path, plan.parameter_workspace_bytes, plan.parameter_read_elements)?,
    };
    let producer =
        DiskTraceArchiveV39::open_existing(producer_archive, &source, &maximum, &actual, &post.claim, plan.archive_disk_bytes)?;
    std::fs::create_dir(output).map_err(|e| e.to_string())?;
    let mut public = PublicDataV39::create(&output.join("public-cache39"), &context, &parameters, &producer, cache_disk_bytes)?;
    let hex = crate::runtime_pack::commit::hex;
    let result = if let Some(raw) = demand {
        let request = FlatDemandV39::from_bytes(raw)?;
        match public.reply(&request)? {
            Some(reply) => {
                std::fs::write(output.join("public-reply39.borsh"), &reply).map_err(|e| e.to_string())?;
                serde_json::json!({"status":"PUBLIC_IDENTITY_REPLY","demand":hex(&request.root()),"reply_sha256":hex(&<[u8;32]>::from(Sha256::digest(&reply))),"bytes":reply.len()})
            }
            None => serde_json::json!({"status":"NEED_DATA","demand":hex(&request.root())}),
        }
    } else {
        let mut independent = DiskTraceArchiveV39::create(&output.join("independent39"), &source, independent_disk_bytes)?;
        let replay = execute_independent_replay_v39(&source, &maximum, &actual, &parameters, &mut independent)?;
        match detect_first_fault_v39(&context, &quota, &replay, &mut public)? {
            FindingV39::Consistent { replayed_claim } => {
                serde_json::json!({"status":"REPLAY_CONSISTENT","replayed_claim":hex(&replayed_claim)})
            }
            FindingV39::Need(request) => {
                std::fs::write(output.join("needed39.borsh"), request.to_bytes()).map_err(|e| e.to_string())?;
                serde_json::json!({"status":"NEED_DATA","demand":hex(&request.root())})
            }
            FindingV39::Fault { filing, .. } => {
                std::fs::write(output.join("filing39.borsh"), &filing).map_err(|e| e.to_string())?;
                let mut salt = [0u8; 64];
                rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
                let mut owned = Vec::with_capacity(68 + filing.len());
                owned.extend_from_slice(&salt);
                owned.extend_from_slice(&(filing.len() as u32).to_le_bytes());
                owned.extend_from_slice(&filing);
                let root = crate::native_payload_v29::owned_file_root_v39(&owned)?;
                std::fs::write(output.join("owned-filing39.borsh"), &owned).map_err(|e| e.to_string())?;
                serde_json::json!({"status":"SELF_CHECKED_LOCAL_FILING","filing_sha256":hex(&<[u8;32]>::from(Sha256::digest(&filing))),"bytes":filing.len(),"owned_file":hex(&root),"owned_file_bytes":owned.len()})
            }
        }
    };
    let report = serde_json::json!({"schema":"misaka.palw.original39-independent-observation","source":hex(&source.root()),"maximum":hex(&maximum.root()),"claim":hex(&post.claim.root()),"result":result,"public_cache_disk_bytes":public.stored_bytes(),"native_verdict":"NOT_RUN","native_final":"NOT_RUN","source_fidelity":"NOT_RUN","validator_approval":"NOT_RUN","a6_eligible":false,"a8_eligible":false});
    std::fs::write(output.join("observation39.json"), serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(report)
}
