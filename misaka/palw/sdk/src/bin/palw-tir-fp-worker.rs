//! RFC-0009 worker for tiled TIR classes. The worker has no RPC or signing key.
use std::io;
use std::path::{Path, PathBuf};

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_class_semantics_v1::{PALW_CLASS_SEMANTICS_VERSION_V1, PalwClassSemanticsDeclarationV1};
use kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_WORKER_MODE_JOB_V3, PALW_FP_WORKER_MODE_MANIFEST_V3, PALW_FP_WORKER_MODE_SERVE_V3,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_tir_attempt_v1::PalwTirJobFactsV1;
use kaspa_consensus_core::palw_tokenizer_rendering_v1::{PalwTokenizerRenderingRuleV1, palw_tokenizer_key_events_v1};
use kaspa_hashes::Hash64;
use misaka_palw_base0::fp_worker::{self, FpWorkerFamilyV1, FpWorkerRuntime, MappedArtifactV1};
use misaka_palw_sdk::kernel_job_v1::kernel_tokenizer_v1;
use misaka_palw_sdk::lineages::tir::{TirBackendV1, TirLineageV1};

const EOG_TOKENS: &[&str] = &["<|im_end|>", "<|endoftext|>", "<|eot_id|>", "<|end_of_text|>", "</s>"];
const MAX_TOKENIZER_BYTES: u64 = 32 << 20;

fn env_path(key: &str) -> Result<PathBuf, String> {
    std::env::var_os(key).map(PathBuf::from).ok_or_else(|| format!("{key} is required"))
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|why| format!("{}: {why}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds {limit} bytes", path.display()));
    }
    Ok(bytes)
}

fn load() -> Result<(FpWorkerRuntime<TirBackendV1>, PalwClassSemanticsDeclarationV1), String> {
    let start = std::time::Instant::now();
    let network = std::env::var("MISAKA_PALW_NETWORK_ID").map_err(|why| why.to_string())?;
    let court = fp_worker::fp_worker_court_params_v1(&network)?;
    let form = fp_worker::fp_worker_prompt_ids_form_v1(&network)?;
    let rules = fp_worker::fp_worker_attempt_rules_v1(&network)?;
    let artifact = env_path("MISAKA_PALW_ARTIFACT")?;
    let guard = MappedArtifactV1::verify_by_reading(&artifact)?;
    let entry = TirLineageV1::open_entry(&artifact)?;
    let class_id = entry.class_id();
    if let Some(wanted) = std::env::var_os("MISAKA_PALW_TIR_CLASS_ID") {
        let wanted: Hash64 = wanted.to_str().ok_or("invalid TIR class ID")?.parse().map_err(|_| "invalid TIR class ID")?;
        if class_id != wanted {
            return Err(format!("artifact derives class {class_id}; expected {wanted}"));
        }
    }
    let tir = PalwTirJobFactsV1::of(&entry.class, &entry.artifact.plan().program, class_id);
    if !tir.tiled || entry.class.tokenizer_id == Hash64::default() {
        return Err("TIR prompt mining requires tiled logits and a committed tokenizer".into());
    }
    let form = if tir.held { PalwPromptIdsFormV1::MerkleV1 } else { form };
    let raw = read_bounded(&env_path("MISAKA_PALW_TOKENIZER")?, MAX_TOKENIZER_BYTES)?;
    let tokenizer = kernel_tokenizer_v1(&raw, Some(entry.class.tokenizer_id))?.tokenizer;
    let key_events = palw_tokenizer_key_events_v1(&raw).map_err(|why| why.to_string())?;
    let mut context = entry.canonical_context();
    let declaration = std::env::var_os("MISAKA_PALW_TIR_SEMANTICS")
        .map(|path| {
            let raw = read_bounded(Path::new(&path), 4096)?;
            borsh::from_slice::<PalwClassSemanticsDeclarationV1>(&raw).map_err(|why| why.to_string())
        })
        .transpose()?;
    if let Some(declaration) = &declaration {
        declaration.check().map_err(str::to_owned)?;
        if declaration.class_id != class_id
            || declaration.artifact_root != entry.artifact_root
            || declaration.tokenizer_id != entry.class.tokenizer_id
            || declaration.raw_tokenizer_bytes != raw.len() as u64
            || declaration.tokenizer_key_events < key_events
            || declaration.rendering_rule != PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1
        {
            return Err("the class semantics do not bind this TIR artifact and tokenizer".into());
        }
        context.model_profile_id = declaration.model_profile_id;
        context.runtime_manifest_hash = declaration.runtime_manifest_hash;
        context.runtime_class_id = declaration.runtime_class_id;
        context.cu_ruleset_id = declaration.cu_ruleset_id;
    }
    let mut backend = TirLineageV1::backend(&entry, &court, form)?.with_network_id_v1(network.as_bytes()).with_fp_class_facts_v1(
        PalwFpClassFactsV3 {
            model_profile_id: context.model_profile_id,
            runtime_manifest_hash: context.runtime_manifest_hash,
            runtime_class_id: context.runtime_class_id,
            shape_profile_id: class_id,
            cu_ruleset_id: context.cu_ruleset_id,
        },
    );
    backend.set_attempt_rules_v1(rules);
    let runtime = FpWorkerRuntime::new_tir(
        backend,
        &context,
        tokenizer,
        FpWorkerFamilyV1 {
            model_id: entry.model_id,
            runtime_identity: context.runtime_class_id,
            tokenizer_id: entry.class.tokenizer_id,
            vocab: tir.token_bound,
            retention_schema: "misaka.palw.fp-v3-tir-retention.v1",
            retention_family: "tir",
            eog_token_names: EOG_TOKENS,
            artifact: Some(guard),
        },
        network.into_bytes(),
        start.elapsed().as_millis() as u64,
        form,
    )?;
    let token_table_root = runtime.token_table_v1().root();
    if declaration.as_ref().is_some_and(|d| d.token_table_root != token_table_root) {
        return Err("the worker's rendered token table differs from the declared root".into());
    }
    let declaration = declaration.unwrap_or(PalwClassSemanticsDeclarationV1 {
        version: PALW_CLASS_SEMANTICS_VERSION_V1,
        class_id,
        artifact_root: entry.artifact_root,
        model_profile_id: context.model_profile_id,
        runtime_manifest_hash: context.runtime_manifest_hash,
        runtime_class_id: context.runtime_class_id,
        tokenizer_id: entry.class.tokenizer_id,
        cu_ruleset_id: context.cu_ruleset_id,
        token_table_root,
        rendering_rule: PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
        raw_tokenizer_bytes: raw.len() as u64,
        tokenizer_key_events: key_events,
        prefix_state_relation: None,
    });
    Ok((runtime, declaration))
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let flag = |name| args.windows(2).find(|pair| pair[0] == name).map(|pair| pair[1].as_str());
    let trace = || flag("--trace-out").map(Path::new).ok_or("--trace-out is required".to_owned());
    match flag("--mode") {
        Some(PALW_FP_WORKER_MODE_MANIFEST_V3) => fp_worker::run_v3_manifest_v1(&load()?.0, &mut io::stdout().lock()),
        Some(PALW_FP_WORKER_MODE_JOB_V3) => {
            let load = || load().unwrap_or_else(|why| die(&why)).0;
            fp_worker::run_v3_job_v1(&mut io::stdin().lock(), &mut io::stdout().lock(), trace()?, load)
        }
        Some(PALW_FP_WORKER_MODE_SERVE_V3) => fp_worker::run_v3_serve_cancellable_v1(
            &load()?.0,
            &mut fp_worker::ThreadedFrames::spawn(io::stdin()),
            &mut io::stdout().lock(),
            trace()?,
        ),
        Some("class-semantics") => {
            let out = flag("--out").ok_or("--out is required")?;
            let bytes = borsh::to_vec(&load()?.1).map_err(|why| why.to_string())?;
            std::fs::write(out, bytes).map_err(|why| why.to_string())
        }
        _ => Err("use --mode v3-manifest, v3-job, v3-serve or class-semantics".into()),
    }
}

fn die(why: &str) -> ! {
    eprintln!("[palw-tir-fp-worker] {why}");
    std::process::exit(1)
}

fn main() {
    if let misaka_palw_host_security::ExecveDenial::Failed(why) = misaka_palw_host_security::confine_self_after_exec() {
        die(&why);
    }
    if let Err(why) = run() {
        die(&why);
    }
}
