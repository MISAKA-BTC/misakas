//! Explicit local CPU/capture reproduction, never native or arithmetic authority.
//! Source-only preparation against 4753ee4be; compilation/execution require a
//! separately approved candidate. Quote-only performs identity/setup reads but
//! never enters either CPU runner or allocates the dense leaf/capture buffers.
use kaspa_consensus_core::{
    Hash64,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1},
    palw_step_leg::{PalwStepTileLeafV1, step_merkle_root_capped_v1, step_tile_leaf_hash_v1},
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_profile_resolver_v2::ResolvedTirJobProfileV2,
    palw_tir_step_v1::{
        PalwTirLeafV1, PalwTirStepBindingV1, PalwTirStepSpaceV1, PalwTirStepSpaceV2, palw_tir_lane_values_v1,
        palw_tir_leaf_preimage_v1, verify_tir_binding_v1, verify_tir_binding_v2,
    },
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::admit::node_cost;
use misaka_palw_tir_exec::node::{
    TirArtifactV1, TirCaptureV1, TirClassRunnerV1, memory,
    run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirWideRunLimitsV2},
    tir_authenticate_capture_metadata_v1,
    wide_capture_v2::{TirWideCaptureV2, WideCaptureAuthLimitsV2, WideCaptureLimitsV2},
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    mem::size_of,
    path::{Path, PathBuf},
    time::Instant,
};

type Result<T> = std::result::Result<T, String>;
const NETWORK: &[u8] = b"a7-local-self-consistency";
const SMALL_INPUT_CAP: u64 = 1 << 20;

struct Options {
    model: String,
    artifact: PathBuf,
    fixture: PathBuf,
    prompt: PathBuf,
    out: PathBuf,
    decode: u32,
    quote_only: bool,
    positions: u32,
    leaves: u64,
    lanes: u64,
    work: u64,
    requested: u64,
    capture: u64,
    external: u64,
}

fn options() -> Result<Options> {
    let mut values = BTreeMap::new();
    let mut quote_only = false;
    let mut run = false;
    let mut args = std::env::args().skip(1);
    while let Some(key) = args.next() {
        match key.as_str() {
            "--quote-only" if !quote_only => quote_only = true,
            "--run" if !run => run = true,
            "--model"
            | "--artifact"
            | "--fixture"
            | "--prompt-ids"
            | "--decode"
            | "--out"
            | "--max-positions"
            | "--max-step-leaves"
            | "--max-row-lanes"
            | "--max-work"
            | "--max-requested-bytes"
            | "--max-capture-bytes"
            | "--external-retained-bytes" => {
                let value = args.next().ok_or_else(|| format!("missing value for {key}"))?;
                if values.insert(key.clone(), value).is_some() {
                    return Err(format!("duplicate {key}"));
                }
            }
            _ => return Err(format!("unknown/duplicate option {key}")),
        }
    }
    if quote_only == run {
        return Err("choose exactly one of --quote-only or --run; there is no implicit CPU run".into());
    }
    let get = |name: &str| values.get(name).cloned().ok_or_else(|| format!("required {name}"));
    let number = |name: &str| -> Result<u64> {
        get(name)?.parse::<u64>().ok().filter(|n| *n != 0).ok_or_else(|| format!("{name} must be a positive u64"))
    };
    Ok(Options {
        model: get("--model")?,
        artifact: get("--artifact")?.into(),
        fixture: get("--fixture")?.into(),
        prompt: get("--prompt-ids")?.into(),
        out: get("--out")?.into(),
        decode: u32::try_from(number("--decode")?).map_err(|_| "decode width")?,
        quote_only,
        positions: u32::try_from(number("--max-positions")?).map_err(|_| "position width")?,
        leaves: number("--max-step-leaves")?,
        lanes: number("--max-row-lanes")?,
        work: number("--max-work")?,
        requested: number("--max-requested-bytes")?,
        capture: number("--max-capture-bytes")?,
        external: number("--external-retained-bytes")?,
    })
}

fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or_else(|| "byte/work addition overflow".into())
}
fn mul(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b).ok_or_else(|| "byte/work multiplication overflow".into())
}
fn check(ok: bool, why: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(why.into()) }
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key].as_str().ok_or_else(|| format!("missing string {key}"))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(value: &str) -> Result<Vec<u8>> {
    check(value.len().is_multiple_of(2), "odd hex length")?;
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let s = std::str::from_utf8(pair).map_err(|e| e.to_string())?;
            u8::from_str_radix(s, 16).map_err(|e| e.to_string())
        })
        .collect()
}
fn hash64(value: &str) -> Result<Hash64> {
    Ok(Hash64::from_bytes(unhex(value)?.try_into().map_err(|_| "expected 64-byte hash")?))
}
fn read_json(path: &Path) -> Result<(Value, String)> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let before = file.metadata().map_err(|e| e.to_string())?;
    check(before.len() <= SMALL_INPUT_CAP, "input JSON byte cap")?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file).take(SMALL_INPUT_CAP + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    check(bytes.len() as u64 <= SMALL_INPUT_CAP, "input JSON grew past cap")?;
    let after = file.metadata().map_err(|e| e.to_string())?;
    check(before.len() == after.len() && before.modified().ok() == after.modified().ok(), "input JSON changed during bounded read")?;
    let value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok((value, hex(&Sha256::digest(&bytes))))
}
fn sha_file(path: &Path) -> Result<(String, u64)> {
    let before = fs::metadata(path).map_err(|e| e.to_string())?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let after = fs::metadata(path).map_err(|e| e.to_string())?;
    check(before.len() == after.len() && before.modified().ok() == after.modified().ok(), "file changed during SHA read")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata().map_err(|e| e.to_string())?;
        check(
            before.dev() == after.dev()
                && before.ino() == after.ino()
                && after.ino() == opened.ino()
                && before.ctime_nsec() == after.ctime_nsec()
                && before.ctime() == after.ctime(),
            "file replaced during SHA read",
        )?;
    }
    Ok((hex(&hasher.finalize()), after.len()))
}

fn model(name: &str) -> Result<(u32, bool, &'static str)> {
    match name {
        "HuggingFaceTB/SmolLM2-360M" => Ok((8192, false, "d17a82c83fcd86736feb60b05e492ebae79f5e2507600bc35ec2c024a2fe36c8")),
        "HuggingFaceTB/SmolLM2-135M" => Ok((8192, false, "48c72278b0d9b45c7ab92b6e70f5fbcef47fa86cc8644cd4307798354639fe90")),
        "EleutherAI/pythia-70m" => Ok((2048, true, "2d1010327664743f240ea8d5bc089daed5a704d4340fea322d1f9cafbe009651")),
        _ => Err("model is outside the fixed three-model cohort; no toy/fallback is permitted".into()),
    }
}

fn local_job(class: &PalwTirClassV1, artifact: Hash64, prompt: &[u32], decode: u32) -> Result<PalwJobContextV2> {
    let zero = Hash64::default();
    Ok(PalwJobContextV2 {
        version: 2,
        network_id: NETWORK.to_vec(),
        job_id: Hash64::from_u64_word(7),
        job_nullifier: zero,
        assignment_id: zero,
        execution_seed: [7; 32],
        model_profile_id: zero,
        runtime_manifest_hash: zero,
        runtime_class_id: zero,
        shape_profile_id: class.class_id(&artifact),
        trace_scheme_id: Hash64::from_bytes(class.decode_program().map_err(|e| e.to_string())?.logits_scheme_id),
        cu_ruleset_id: zero,
        // Keep the canonical job's existing zero field distinct from the
        // registered tokenizer which remains bound in the class ID.
        tokenizer_id: zero,
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, prompt).map_err(|e| e.to_string())?,
        declared_prefill_tokens: u32::try_from(prompt.len()).map_err(|_| "prefill width")?,
        exact_decode_tokens: decode,
        max_context_tokens: class.layout.max_context,
    })
}

/// Defense for narrow as well as wide: the legacy CPU runner alone does not
/// enforce every job/prompt field. No canonical anchor-derived job is claimed.
fn common_preflight(ctx: &PalwJobContextV2, class: &PalwTirClassV1, artifact: Hash64, prompt: &[u32], original: u32) -> Result<u32> {
    let program = class.decode_program().map_err(|e| e.to_string())?;
    let budget = ctx.declared_prefill_tokens.checked_add(ctx.exact_decode_tokens).ok_or("token budget overflow")?;
    check(ctx.version == 2 && ctx.network_id.as_slice() == NETWORK, "local context version/network mismatch")?;
    check(ctx.shape_profile_id == class.class_id(&artifact), "local context class mismatch")?;
    check(ctx.trace_scheme_id == Hash64::from_bytes(program.logits_scheme_id), "local context scheme mismatch")?;
    check(class.layout.max_context == original && ctx.max_context_tokens == original, "original declared context changed")?;
    check(
        ctx.declared_prefill_tokens != 0 && ctx.exact_decode_tokens != 0 && budget <= original,
        "P+D exceeds original declared context budget",
    )?;
    check(
        prompt.len() == ctx.declared_prefill_tokens as usize && prompt.iter().all(|id| *id < program.token_bound),
        "prompt length/ID bound",
    )?;
    check(
        prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, prompt).map_err(|e| e.to_string())? == ctx.prompt_token_ids_hash,
        "Flat prompt commitment mismatch",
    )?;
    Ok(budget - 1)
}

fn binding(
    class: &PalwTirClassV1,
    root: Hash64,
    ctx: &PalwJobContextV2,
    count: u64,
    trace: Hash64,
    step: Hash64,
    execution: Hash64,
) -> PalwTirStepBindingV1 {
    PalwTirStepBindingV1 {
        version: 1,
        job_context: ctx.clone(),
        class: class.clone(),
        artifact_root: root,
        full_logits_trace_root: trace,
        step_leaf_count: count,
        step_merkle_root: step,
        committed_execution_root: execution,
    }
}

struct GeometryQuote {
    payload_sum: u64,
    maximum_payload: u64,
    visits: u64,
    reserved_work: u64,
}
fn leaf_payload(leaf: &PalwTirLeafV1, wide: bool) -> Result<u64> {
    let width = if leaf.dtype == misaka_palw_tir::DType::I64 {
        check(wide, "wide leaf in narrow capture")?;
        8
    } else {
        check(leaf.dtype.committable(), "uncommittable narrow leaf dtype")?;
        4
    };
    mul(u64::from(leaf.value_count), width)
}
// This bounded geometry walk retains no leaf list. Reserve its symbolic work
// before entering it, including later store/authentication geometry passes.
fn geometry_quote(
    count: u64,
    per_visit: u64,
    remaining_work: u64,
    wide: bool,
    at: impl Fn(u64) -> Option<PalwTirLeafV1>,
) -> Result<GeometryQuote> {
    check(count <= remaining_work, "geometry visit count exceeds remaining work cap")?;
    let reserved_work = mul(mul(count, per_visit)?, 4)?;
    check(reserved_work <= remaining_work, "geometry walk/pass work ceiling")?;
    let (mut payload_sum, mut maximum_payload) = (0u64, 0u64);
    for index in 0..count {
        let leaf = at(index).ok_or("canonical quote leaf missing")?;
        let bytes = leaf_payload(&leaf, wide)?;
        payload_sum = add(payload_sum, bytes)?;
        maximum_payload = maximum_payload.max(bytes);
    }
    Ok(GeometryQuote { payload_sum, maximum_payload, visits: count, reserved_work })
}

// Preallocate the entire dense callback store only AFTER the upper quote passes.
// No callback clone can enlarge it. Quote-only never calls this function.
fn leaf_store(count: u64, wide: bool, at: impl Fn(u64) -> Option<PalwTirLeafV1>) -> Result<Vec<PalwStepTileLeafV1>> {
    let n = usize::try_from(count).map_err(|_| "leaf count native width")?;
    let mut leaves = Vec::new();
    leaves.try_reserve_exact(n).map_err(|e| e.to_string())?;
    check(leaves.capacity() == n, "leaf Vec allocator capacity exceeded quote")?;
    for i in 0..count {
        let leaf = at(i).ok_or("canonical leaf missing")?;
        let bytes = usize::try_from(leaf_payload(&leaf, wide)?).map_err(|_| "payload native width")?;
        let mut values_le = Vec::new();
        values_le.try_reserve_exact(bytes).map_err(|e| e.to_string())?;
        check(values_le.capacity() == bytes, "payload allocator capacity exceeded quote")?;
        leaves.push(PalwStepTileLeafV1 { version: 1, coord: leaf.coord, value_count: leaf.value_count, values_le });
    }
    Ok(leaves)
}
fn save_leaf(leaves: &mut [PalwStepTileLeafV1], next: &mut usize, refused: &mut bool, index: u64, source: &PalwStepTileLeafV1) {
    if *refused {
        return;
    }
    let Some(target) = leaves.get_mut(*next) else {
        *refused = true;
        return;
    };
    if index != *next as u64
        || target.coord != source.coord
        || target.value_count != source.value_count
        || source.values_le.len() > target.values_le.capacity()
    {
        *refused = true;
        return;
    }
    target.version = source.version;
    target.values_le.extend_from_slice(&source.values_le);
    *next += 1;
}

fn capture_limits(o: &Options, payload: u64, external: u64) -> WideCaptureLimitsV2 {
    WideCaptureLimitsV2 {
        max_frame_bytes: o.capture,
        max_decoded_bytes: o.capture,
        max_rows: u64::from(o.decode),
        max_row_lanes: o.lanes,
        max_prompt_ids: u64::from(o.positions),
        max_leaves: o.leaves,
        max_leaf_payload_bytes: payload,
        max_total_leaf_payload_bytes: o.capture,
        max_operation_bytes: o.requested,
        external_retained_bytes: external,
    }
}
// Keep JSON export bounded independently of decode count. Exact row values
// are compared in memory by the capture roundtrip; only their frame SHA and
// extrema are exported, not a second serde_json tree containing every logit.
fn logits_summary(values: impl Iterator<Item = i64>) -> Value {
    let (mut lanes, mut outside, mut minimum, mut maximum) = (0u64, 0u64, None::<i64>, None::<i64>);
    let mut row_hash = Sha256::new();
    for value in values {
        row_hash.update(value.to_le_bytes());
        lanes += 1;
        outside += u64::from(i32::try_from(value).is_err());
        minimum = Some(minimum.map_or(value, |old| old.min(value)));
        maximum = Some(maximum.map_or(value, |old| old.max(value)));
    }
    json!({"lanes":lanes,"minimum":minimum,"maximum":maximum,"outside_i32_lanes":outside,"beyond_i32_range":outside != 0,
        "ordered_values_i64_le_sha256":hex(&row_hash.finalize()),"digest_encoding":"row-major signed i64 LE values; narrow rows widened only for this diagnostic digest, never for execution/wire",
        "full_logit_values_exported":false,"exact_values_compared_in_roundtrip":true})
}

fn capture_quote_json(q: &misaka_palw_tir_exec::node::wide_capture_v2::WideCaptureQuoteV2) -> Value {
    json!({"input_frame_bytes":q.input_frame_bytes,"source_run_bytes":q.source_run_bytes,"moved_leaf_bytes":q.moved_leaf_bytes,
        "capture_bytes":q.capture_bytes,"scratch_bytes":q.scratch_bytes,"encoded_frame_bytes":q.encoded_frame_bytes,
        "external_retained_bytes":q.external_retained_bytes,"operation_peak_bytes":q.operation_peak_bytes})
}

fn execute(o: &Options, report: &mut Value) -> Result<()> {
    let (original, wide, pinned_sha) = model(&o.model)?;
    report["model"] = json!(o.model);
    report["mode"] = json!(if o.quote_only { "quote-only" } else { "run" });
    report["original_max_context"] = json!(original);
    report["caps"] = json!({"max_positions":o.positions,"max_step_leaves":o.leaves,"max_row_lanes":o.lanes,
        "max_work":o.work,"max_requested_bytes":o.requested,"max_capture_bytes":o.capture});
    report["phase"] = json!("identity_and_setup");
    let started = Instant::now();
    let (fixture, fixture_sha) = read_json(&o.fixture)?;
    let (prompt_value, prompt_sha) = read_json(&o.prompt)?;
    let prompt: Vec<u32> =
        serde_json::from_value(prompt_value).map_err(|e| format!("prompt must be a literal JSON u32 array: {e}"))?;
    report["prompt_tokens"] = json!(prompt.len());
    report["declared_prefill_tokens"] = json!(prompt.len());
    report["exact_decode_tokens"] = json!(o.decode);
    report["prompt_ids_sha256"] = json!(prompt_sha);
    let (artifact_sha, artifact_bytes) = sha_file(&o.artifact)?;
    check(
        artifact_sha == pinned_sha && artifact_sha == text(&fixture, "artifact_sha256")?,
        "declared artifact SHA differs from fixed cohort/fixture",
    )?;
    check(fixture["artifact_bytes"].as_u64() == Some(artifact_bytes), "fixture artifact length mismatch")?;
    let artifact = TirArtifactV1::open(&o.artifact)?;
    artifact.check_weights_unmodified()?;
    let class = artifact.class()?;
    check(class.program == unhex(text(&fixture, "program_borsh_hex")?)?, "declared program differs from saved fixture")?;
    check(
        borsh::to_vec(&class.layout).map_err(|e| e.to_string())? == unhex(text(&fixture, "layout_borsh_hex")?)?,
        "declared layout differs from saved fixture",
    )?;
    check(class.tokenizer_id == hash64(text(&fixture, "tokenizer_id_hex")?)?, "registered tokenizer differs from fixture")?;
    check(
        class.layout.max_context == original && fixture["max_context"].as_u64() == Some(u64::from(original)),
        "original maximum context changed",
    )?;
    let (root, inventory_leaves) = artifact.inventory_root()?; // real all-weight streaming identity pass
    check(root == hash64(text(&fixture, "artifact_root_hex")?)?, "actual streamed parameter root mismatch")?;
    let class_id = class.class_id(&root);
    report["class_id"] = json!(class_id.to_string());
    report["artifact_sha256"] = json!(artifact_sha);
    check(class_id == hash64(text(&fixture, "class_id_hex")?)?, "class identity mismatch")?;
    check(artifact.plan().program.wide_logits_q24() == wide, "fixed model/profile mismatch; no narrow/wide fallback")?;
    let resolved = ResolvedTirJobProfileV2::new(&class, root, misaka_palw_tir::program::MAX_PROGRAM_BYTES)
        .map_err(|e| format!("profile: {e:?}"))?;
    check(resolved.class_id() == class_id && resolved.max_context() == original, "resolved class/context mismatch")?;
    report["resolved_profile"] = json!(format!("{:?}", resolved.profile()));
    let ctx = local_job(&class, root, &prompt, o.decode)?;
    let positions = common_preflight(&ctx, &class, root, &prompt, original)?;
    check(positions <= o.positions, "executed position cap")?;
    // Same-length wrong prompt must fail the common preflight, including on
    // narrow where the runner does not check this commitment itself.
    let mut wrong = prompt.clone();
    let token_bound = artifact.plan().program.token_bound;
    check(token_bound > 1, "negative prompt check needs two legal token IDs")?;
    wrong[0] = (wrong[0] + 1) % token_bound;
    check(common_preflight(&ctx, &class, root, &wrong, original).is_err(), "same-length wrong prompt was not rejected")?;
    drop(wrong);
    let vocab = artifact.plan().program.blocks[artifact.plan().program.schedule.post as usize].nodes
        [artifact.plan().program.logits as usize]
        .out
        .elements_at(1);
    check(vocab != 0 && vocab <= o.lanes, "logits row lane cap")?;
    let external_floor = add(artifact_bytes, add(mul(class.program.len() as u64, 64)?, add(SMALL_INPUT_CAP * 4, 64 << 20)?)?)?;
    check(o.external >= external_floor, "caller external owner allowance below conservative artifact/metadata floor")?;
    report["inputs"] = json!({"model":o.model,"artifact":o.artifact,"artifact_realpath":fs::canonicalize(&o.artifact).map_err(|e| e.to_string())?,
        "artifact_sha256":artifact_sha,"artifact_bytes":artifact_bytes,"fixture":o.fixture,"fixture_sha256":fixture_sha,
        "prompt_ids_file":o.prompt,"prompt_ids_sha256":prompt_sha,"prompt_ids":prompt,"class_id":class_id.to_string(),
        "artifact_root":root.to_string(),"inventory_leaves":inventory_leaves,"registered_tokenizer":class.tokenizer_id.to_string(),
        "job_tokenizer_field":ctx.tokenizer_id.to_string(),"metadata_parsing":misaka_palw_tir_artifact::metadata_v16::parse_recorded_metadata_v16(&artifact.header().meta)?["metadata_parsing"].clone()});
    report["job"] = json!({"kind":"SHORT_LOCAL_NONCANONICAL_JOB","job_id":ctx.job_id.to_string(),"context_hash":ctx.context_hash().to_string(),
        "network":String::from_utf8_lossy(NETWORK),"prefill":prompt.len(),"decode":o.decode,"executed_positions":positions,
        "declared_context_budget":ctx.max_context_tokens,"original_max_context":original,"trace_scheme":ctx.trace_scheme_id.to_string(),
        "selection":"fixed-length greedy; no EOS early stop","same_length_wrong_prompt_rejected":true});
    report["setup_seconds"] = json!(started.elapsed().as_secs_f64());
    report["phase"] = json!("quote");
    let mut wide_quote = None;
    let (count, geometry, cpu_bytes, cpu_quote) = if wide {
        let space = PalwTirStepSpaceV2::new(&class).map_err(|e| e.to_string())?;
        let runner = TirClassRunnerWideQ24V2::new(&space, artifact.plan(), artifact.params(), class_id)?;
        let limits = TirWideRunLimitsV2 {
            max_positions: o.positions,
            max_step_leaves: o.leaves,
            max_row_lanes: o.lanes,
            max_work: o.work,
            max_requested_bytes: o.requested,
        };
        let q = runner.quote(&ctx, limits)?;
        let value = json!({"kind":"wide_explicit_api","leaf_count":q.leaf_count,"node_visits":q.node_visits,"work":q.work,
            "executor_bytes":q.executor_bytes,"retained_bytes":q.retained_bytes,"scratch_bytes":q.scratch_bytes,"requested_bytes":q.requested_bytes});
        let geometry = geometry_quote(
            q.leaf_count,
            class.program.len() as u64 + 1,
            o.work.checked_sub(q.work).ok_or("wide CPU work exceeds cap")?,
            true,
            |i| space.leaf_at(&ctx, i),
        )?;
        let result = (q.leaf_count, geometry, q.requested_bytes, value);
        wide_quote = Some(limits);
        result
    } else {
        let space = PalwTirStepSpaceV1::new(&class).map_err(|e| e.to_string())?;
        let count = space.leaf_count_capped(&ctx, o.leaves).map_err(|e| e.to_string())?;
        let executor = memory::tir_executor_storage_need_v1(&space, artifact.plan(), &ctx)?;
        let retained = memory::tir_retained_storage_need_v1(&space, &class, &ctx, o.leaves)?;
        let trees = memory::tir_full_tree_storage_need_v1(count)?;
        let bytes = add(executor, add(retained, trees)?)?;
        let mut kernel_work = 0;
        for &(block, _) in space.occurrences() {
            for node in 0..space.program.blocks[block as usize].nodes.len() {
                let cost =
                    node_cost(&space.program, block as usize, node, u64::from(space.info.blocks[block as usize].window.unwrap_or(1)));
                for n in [cost.macs, cost.elementwise, cost.transcendentals, cost.bytes_read, cost.bytes_written] {
                    check(n != u64::MAX, "saturated narrow kernel cost")?;
                    kernel_work = add(kernel_work, n)?;
                }
            }
        }
        kernel_work = mul(kernel_work, u64::from(positions))?;
        check(kernel_work <= o.work, "narrow symbolic kernel work cap")?;
        let geometry = geometry_quote(
            count,
            class.program.len() as u64 + 1,
            o.work.checked_sub(kernel_work).ok_or("narrow CPU work exceeds cap")?,
            false,
            |i| space.leaf_at(&ctx, i),
        )?;
        (
            count,
            geometry,
            bytes,
            json!({"kind":"legacy_conservative_storage_and_kernel_quote",
            "leaf_count":count,"executor_bytes":executor,"retained_bytes":retained,"tree_bytes":trees,"requested_bytes":bytes,
            "kernel_work":kernel_work,"full_host_work_quote":"NOT_AVAILABLE_ON_LEGACY_API"}),
        )
    };
    let payload = geometry.maximum_payload;
    let cpu_work = cpu_quote["work"].as_u64().or_else(|| cpu_quote["kernel_work"].as_u64()).ok_or("missing CPU quote work")?;
    let cpu_plus_geometry_work = add(cpu_work, geometry.reserved_work)?;
    check(cpu_plus_geometry_work <= o.work, "CPU plus reserved geometry work cap")?;
    let dense = add(mul(count, size_of::<PalwStepTileLeafV1>() as u64)?, geometry.payload_sum)?;
    let blank = binding(&class, root, &ctx, count, Hash64::default(), Hash64::default(), Hash64::default());
    let metadata = borsh::object_length(&blank).map_err(|e| e.to_string())? as u64;
    let rows = mul(u64::from(o.decode), add(4, mul(vocab, if wide { 8 } else { 4 })?)?)?;
    let prompt_wire = add(4, mul(prompt.len() as u64, 4)?)?;
    let rows_wire = add(4, rows)?;
    let generated_wire = add(4, mul(u64::from(o.decode), 4)?)?;
    let leaves_wire = add(4, add(mul(count, 26)?, geometry.payload_sum)?)?;
    let frame = add(8, add(metadata, add(prompt_wire, add(rows_wire, add(generated_wire, leaves_wire)?)?)?)?)?;
    let export_and_lane_scratch = add(64 << 20, mul(payload, 16)?)?;
    let peak = add(o.external, add(cpu_bytes, add(mul(dense, 3)?, add(mul(frame, 2)?, export_and_lane_scratch)?)?)?)?;
    report["quote"] = json!({"cpu":cpu_quote,"maximum_leaf_payload_bytes":payload,"exact_leaf_payload_sum":geometry.payload_sum,
        "geometry_walk_visits":geometry.visits,"geometry_work_reserved":geometry.reserved_work,
        "geometry_passes_reserved":4,"geometry_work_per_visit":class.program.len() as u64 + 1,
        "geometry_work_scope":"symbolic program-byte walk units, not measured CPU instructions",
        "cpu_plus_reserved_geometry_work":cpu_plus_geometry_work,
        "work_cap_scope":"per-operation logical CPU/geometry/authentication ceilings; not cumulative whole-harness cost, setup hashing, export visits, OS instructions or fees",
        "leaf_count":count,"dense_callback_backing_upper":dense,
        "capture_frame_upper":frame,"export_and_lane_scratch_upper":export_and_lane_scratch,"simultaneous_operation_upper":peak,"external_retained_bytes":o.external,"external_floor":external_floor,
        "external_allowance_completeness":"CALLER_DECLARED_NOT_MEASURED_OR_CERTIFIED","moved_leaves_are_capture_subset":true,
        "owners":["artifact mmap/params/plan","class/profile/space/expected binding","prompt/fixture/metadata","run and lease lifetime",
            "preallocated dense callback leaves","capture","encoded frame","decoded capture","authentication/hash scratch"],
        "limits":{"max_positions":o.positions,"max_step_leaves":o.leaves,"max_row_lanes":o.lanes,"max_work":o.work,
            "max_requested_bytes":o.requested,"max_capture_bytes":o.capture,"external_retained_bytes":o.external}});
    check(
        cpu_bytes <= o.requested && dense <= o.capture && frame <= o.capture && peak <= o.requested,
        "RESOURCE_LIMIT: logical CPU/dense capture/frame/simultaneous upper exceeds explicit cap",
    )?;
    if o.quote_only {
        report["result"] = json!("QUOTE_ONLY_SELF_CONSISTENCY_NOT_RUN");
        report["status"] = json!("QUOTE_ONLY");
        report["phase"] = json!("quote_complete");
        return Ok(());
    }
    report["phase"] = json!("cpu_and_capture");
    report["cpu_execution_started"] = json!(true);
    artifact.check_weights_unmodified()?;
    let started = Instant::now();
    let mut next = 0usize;
    let mut refused = false;
    let outcome = if wide {
        let space = PalwTirStepSpaceV2::new(&class).map_err(|e| e.to_string())?;
        let mut leaves = leaf_store(count, wide, |i| space.leaf_at(&ctx, i))?;
        let runner = TirClassRunnerWideQ24V2::new(&space, artifact.plan(), artifact.params(), class_id)?;
        let run = runner.run(&ctx, &prompt, ctx.context_hash(), wide_quote.ok_or("missing wide quote")?, &mut |leaf| {
            save_leaf(&mut leaves, &mut next, &mut refused, leaf.index, leaf.preimage)
        })?;
        check(!refused && next as u64 == count, "callback count/geometry/capacity refusal")?;
        let expected = binding(&class, root, &ctx, count, run.trace_root(), run.step_merkle_root(), run.execution_root());
        let verified = verify_tir_binding_v2(&expected, o.leaves).map_err(|e| e.to_string())?;
        let limits = capture_limits(o, payload, o.external);
        let build = TirWideCaptureV2::quote_from_run(&expected, &prompt, &run, &leaves, limits)?;
        let capture = TirWideCaptureV2::from_run(&expected, &prompt, &run, leaves, limits)?;
        let encode_limits = capture_limits(o, payload, add(o.external, run.reserved_bytes())?);
        let encoded_quote = capture.quote_encode(encode_limits)?;
        let encoded = capture.encode_bounded(encode_limits)?;
        check(encoded.len() as u64 <= o.capture, "wide frame cap")?;
        let decode_limits = capture_limits(o, payload, add(add(o.external, run.reserved_bytes())?, capture.backing_bytes()?)?);
        let decoded = TirWideCaptureV2::decode_bounded(&encoded, decode_limits)?;
        check(decoded == capture, "wide byte roundtrip mismatch")?;
        let auth_external = add(add(add(o.external, run.reserved_bytes())?, capture.backing_bytes()?)?, encoded.capacity() as u64)?;
        let auth = WideCaptureAuthLimitsV2 {
            max_step_leaves: o.leaves,
            max_work: o.work,
            max_scratch_bytes: o.capture,
            max_operation_bytes: o.requested,
            external_retained_bytes: auth_external,
        };
        let auth_quote = decoded.quote_auth_expected(&expected, &verified, auth)?;
        decoded.authenticate_expected(&expected, &verified, auth)?;
        json!({"profile":"i64-Q24-v2","units":"nats * 2^24","generated":run.generated(),"logits_summary":logits_summary(run.logits_rows().iter().flatten().copied()),"logits_row_count":run.logits_rows().len(),"frame_bytes":encoded.len(),
            "frame_sha256":hex(&Sha256::digest(&encoded)),"leaf_count":count,"dense":decoded.is_dense(),"build_quote":capture_quote_json(&build),
            "encode_quote":capture_quote_json(&encoded_quote),"auth_quote":{"work":auth_quote.work,"scratch_bytes":auth_quote.scratch_bytes,"operation_peak_bytes":auth_quote.operation_peak_bytes},
            "trace_root":run.trace_root().to_string(),"step_merkle_root":run.step_merkle_root().to_string(),"output_root":run.output_root().to_string(),"execution_root":run.execution_root().to_string(),
            "run_reserved_bytes_are_logical_not_native_host_lease":run.reserved_bytes()})
    } else {
        let space = PalwTirStepSpaceV1::new(&class).map_err(|e| e.to_string())?;
        let mut leaves = leaf_store(count, wide, |i| space.leaf_at(&ctx, i))?;
        let runner = TirClassRunnerV1::new(&space, artifact.plan(), artifact.params(), class_id)?;
        let run = runner.run(&ctx, &prompt, o.leaves, true, &mut |leaf| {
            save_leaf(&mut leaves, &mut next, &mut refused, leaf.index, leaf.preimage)
        })?;
        check(!refused && next as u64 == count && run.leaf_count == count, "narrow callback count/geometry/capacity refusal")?;
        let expected = binding(&class, root, &ctx, count, run.trace_root, run.step_merkle_root, run.execution_root);
        let capture = TirCaptureV1 {
            binding: expected.clone(),
            prompt: prompt.clone(),
            logits_rows: run.logits_rows,
            generated: run.generated,
            leaves,
        };
        let length = add(8, borsh::object_length(&capture).map_err(|e| e.to_string())? as u64)?;
        check(length <= o.capture && length <= frame, "narrow frame refused before encode/decode allocation")?;
        let encoded = capture.encode();
        check(encoded.len() as u64 == length, "narrow encoded length drift")?;
        let decoded = TirCaptureV1::decode(&encoded)?;
        check(decoded == capture && decoded.binding == expected, "narrow roundtrip/expected binding mismatch")?;
        tir_authenticate_capture_metadata_v1(&decoded, class_id, PalwPromptIdsFormV1::Flat, o.leaves)?;
        verify_tir_binding_v1(&expected, o.leaves).map_err(|e| e.to_string())?;
        let mut hashes = Vec::new();
        hashes.try_reserve_exact(decoded.leaves.len()).map_err(|e| e.to_string())?;
        for (i, preimage) in decoded.leaves.iter().enumerate() {
            let leaf = space.leaf_at(&ctx, i as u64).ok_or("narrow canonical leaf missing")?;
            check(
                preimage.version == 1
                    && preimage.coord == leaf.coord
                    && preimage.value_count == leaf.value_count
                    && preimage.values_le.len() as u64 == mul(u64::from(leaf.value_count), 4)?,
                "narrow dense leaf order/geometry/width",
            )?;
            let values = palw_tir_lane_values_v1(leaf.dtype, &preimage.values_le).map_err(|e| e.to_string())?;
            check(
                palw_tir_leaf_preimage_v1(&leaf, &values).map_err(|e| e.to_string())? == *preimage,
                "narrow leaf lane reconstruction",
            )?;
            hashes.push(step_tile_leaf_hash_v1(&ctx.context_hash(), &class_id, preimage));
        }
        check(
            hashes.len() as u64 == count
                && step_merkle_root_capped_v1(&hashes, o.leaves).map_err(|e| e.to_string())? == expected.step_merkle_root,
            "narrow dense Merkle mismatch",
        )?;
        json!({"profile":"legacy-i32-v1","units":"unchanged legacy i32 profile","generated":decoded.generated,"logits_summary":logits_summary(decoded.logits_rows.iter().flatten().copied().map(i64::from)),"logits_row_count":decoded.logits_rows.len(),"frame_bytes":encoded.len(),
            "frame_sha256":hex(&Sha256::digest(&encoded)),"leaf_count":count,"dense":true,"dense_geometry_and_merkle_checked":true,
            "trace_root":expected.full_logits_trace_root.to_string(),"step_merkle_root":expected.step_merkle_root.to_string(),"output_root":run.output_root.to_string(),"execution_root":expected.committed_execution_root.to_string()})
    };
    artifact.check_weights_unmodified()?;
    check(sha_file(&o.artifact)?.0 == artifact_sha, "artifact SHA changed after run")?;
    report["execution_seconds_including_capture"] = json!(started.elapsed().as_secs_f64());
    report["local_roundtrip"] = outcome;
    report["phase"] = json!("complete");
    report["result"] = json!("SELF_CONSISTENCY_ONLY");
    report["status"] = json!("LOCAL_ROUNDTRIP_PASS");
    report["roundtrip_pass"] = json!(true);
    Ok(())
}

fn main() {
    let mut report = json!({"schema":"misaka.a7.real-llm-repro.v1","scope":"SELF_CONSISTENCY_ONLY",
        "native_v2_admission_executed":false,"credit_executed":false,"independent_expected_executed":false,
        "status":"NOT_RUN","roundtrip_pass":false,"cpu_execution_started":false,"source_api_candidate":"4753ee4beae5e245095954c6ef6e9b96fa05f361",
        "phase":"arguments","result":"NOT_RUN","host_reserver_installed":false,"native_lease":false,
        "not_run":["independent expected/arithmetic comparison","full original maximum position coverage","native registration/admission/registry/restart","claim P certification","credit/entitlements"],
        "authority":"No model review or tier decision; captured roots authenticate local data only.","argv":std::env::args().collect::<Vec<_>>()});
    let parsed = options();
    let (out, error) = match parsed {
        Ok(o) => {
            let error = execute(&o, &mut report).err();
            (Some(o.out), error)
        }
        Err(error) => {
            let args: Vec<_> = std::env::args().collect();
            (args.windows(2).find(|p| p[0] == "--out").map(|p| PathBuf::from(&p[1])), Some(error))
        }
    };
    let failed = error.is_some();
    if let Some(error) = error {
        report["error"] = json!(error);
        report["result"] = json!("FAILED_OR_RESOURCE_LIMIT");
        report["status"] = json!("FAILED_OR_RESOURCE_LIMIT");
    }
    let bytes = serde_json::to_vec_pretty(&report).expect("JSON report");
    if let Some(out) = out {
        match OpenOptions::new().write(true).create_new(true).open(&out).and_then(|mut file| {
            file.write_all(&bytes)?;
            file.write_all(b"\n")
        }) {
            Ok(()) => {}
            Err(error) => {
                eprintln!("report write refused: {error}");
                eprintln!("{}", String::from_utf8_lossy(&bytes));
                std::process::exit(2);
            }
        }
    } else {
        eprintln!("{}", String::from_utf8_lossy(&bytes));
    }
    if failed {
        std::process::exit(1);
    }
}
