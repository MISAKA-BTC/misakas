//! Class-only analysis of the fixed three trained-artifact declarations.
//! This binary never constructs a registration carrier, signs, executes a model,
//! installs state, selects protocol Params, or grants admission/credit authority.
//! Header limits are local parser limits. Program decode, geometry, read maps,
//! sizing caches and exports are NOT a bounded startup/whole-process RSS quote.
use borsh::BorshDeserialize;
use kaspa_consensus_core::{
    Hash64,
    palw_tir_admission_v1::PALW_TIR_DISSECT_CARRIER_BYTES_V1,
    palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1},
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_close_range_v1::palw_tir_worst_closes_range_work_v1,
    palw_tir_close_size_v1::{
        PALW_TIR_CLOSE_SIZING_OVER_CAP_V1, PALW_TIR_CLOSE_SIZING_WORK_CAP_V1, PalwTirCloseSizingV1, PalwTirParamFormV1,
        palw_tir_worst_closes_work_v1,
    },
    palw_tir_close_size_v2::{PalwTirCloseSizingV2, palw_tir_worst_closes_range_work_v2, palw_tir_worst_closes_work_v2},
    palw_tir_court_v1::PalwTirInventoryIndexV1,
    palw_tir_profile_resolver_v2::ResolvedTirJobProfileV2,
    palw_tir_step_v1::{PalwTirStepSpaceV1, PalwTirStepSpaceV2},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::program::MAX_PROGRAM_BYTES;
use misaka_palw_tir_artifact::bounded_v1::{PalwTirHeaderLimitsV1, read_header_bounded_v1};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::Instant,
};

type Result<T> = std::result::Result<T, String>;
const INPUT_CAP: u64 = 1 << 20;
const SCHEMA: &str = "misaka.a7.real-llm-class-analysis.v1";

struct Options {
    model: String,
    artifact: PathBuf,
    fixture: PathBuf,
    out: PathBuf,
    leaf_cap: u64,
    courts: Vec<bool>,
}
struct Model {
    original: u32,
    wide: bool,
    bytes: u64,
    artifact_sha: &'static str,
    fixture_sha: &'static str,
    revision: &'static str,
}
fn model(name: &str) -> Result<Model> {
    match name {
        "HuggingFaceTB/SmolLM2-360M" => Ok(Model {
            original: 8192,
            wide: false,
            bytes: 436395392,
            artifact_sha: "d17a82c83fcd86736feb60b05e492ebae79f5e2507600bc35ec2c024a2fe36c8",
            fixture_sha: "c56ab4d0491f2be025b4efc37f0769b8bee4c7bd23564e0373270ad5d1a3bc40",
            revision: "f8027fd0eaeea54caa13c31d31b9fdc459c38b49",
        }),
        "HuggingFaceTB/SmolLM2-135M" => Ok(Model {
            original: 8192,
            wide: false,
            bytes: 180336320,
            artifact_sha: "48c72278b0d9b45c7ab92b6e70f5fbcef47fa86cc8644cd4307798354639fe90",
            fixture_sha: "252c20d0aab613bdc3e7349ba85e1a1306167cf06185ffff4bf12dea6df6e1b9",
            revision: "93efa2f097d58c2a74874c7e644dbc9b0cee75a2",
        }),
        "EleutherAI/pythia-70m" => Ok(Model {
            original: 2048,
            wide: true,
            bytes: 103402560,
            artifact_sha: "2d1010327664743f240ea8d5bc089daed5a704d4340fea322d1f9cafbe009651",
            fixture_sha: "29c0d221c7f998dee2e4778bd8bfd9f9b8b4e610b17a62b1bf6f4484b7f367a8",
            revision: "a39f36b100fe8a5377810d56c3f4789b9c53ac42",
        }),
        _ => Err("model is outside the fixed three-model cohort; no fallback".into()),
    }
}
fn options() -> Result<Options> {
    let mut values = BTreeMap::new();
    let mut args = std::env::args().skip(1);
    while let Some(key) = args.next() {
        match key.as_str() {
            "--model" | "--artifact" | "--fixture" | "--out" | "--diagnostic-leaf-cap" | "--diagnostic-court" => {}
            _ => return Err(format!("unknown option {key}")),
        }
        let value = args.next().ok_or_else(|| format!("missing value for {key}"))?;
        if values.insert(key.clone(), value).is_some() {
            return Err(format!("duplicate {key}"));
        }
    }
    let get = |key: &str| values.get(key).cloned().ok_or_else(|| format!("required {key}"));
    let leaf_cap = get("--diagnostic-leaf-cap")?.parse::<u64>().map_err(|e| e.to_string())?;
    ensure(leaf_cap > 0, "diagnostic leaf cap must be positive")?;
    let courts = match get("--diagnostic-court")?.as_str() {
        "on" => vec![true],
        "off" => vec![false],
        "both" => vec![false, true],
        _ => return Err("--diagnostic-court must be on, off, or both".into()),
    };
    Ok(Options {
        model: get("--model")?,
        artifact: get("--artifact")?.into(),
        fixture: get("--fixture")?.into(),
        out: get("--out")?.into(),
        leaf_cap,
        courts,
    })
}
fn ensure(ok: bool, why: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(why.into()) }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(s: &str) -> Result<Vec<u8>> {
    ensure(s.len().is_multiple_of(2), "odd hex length")?;
    s.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).map_err(|e| e.to_string())?, 16).map_err(|e| e.to_string()))
        .collect()
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key].as_str().ok_or_else(|| format!("missing string {key}"))
}
fn hash64(s: &str) -> Result<Hash64> {
    Ok(Hash64::from_bytes(unhex(s)?.try_into().map_err(|_| "expected 64-byte hash")?))
}
fn same_metadata(a: &Metadata, b: &Metadata) -> bool {
    let same = a.len() == b.len() && a.modified().ok() == b.modified().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        same && a.dev() == b.dev() && a.ino() == b.ino() && a.ctime() == b.ctime() && a.ctime_nsec() == b.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        same
    }
}
fn stable(file: &File, before: &Metadata, path: &Path) -> Result<()> {
    ensure(same_metadata(before, &file.metadata().map_err(|e| e.to_string())?), "opened input changed")?;
    ensure(same_metadata(before, &fs::metadata(path).map_err(|e| e.to_string())?), "input path changed or was replaced")
}
fn read_fixture(path: &Path, expected_sha: &str) -> Result<(Value, String)> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let before = file.metadata().map_err(|e| e.to_string())?;
    ensure(before.is_file() && before.len() <= INPUT_CAP, "fixture file/byte cap")?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file).take(INPUT_CAP + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    ensure(bytes.len() as u64 <= INPUT_CAP, "fixture grew beyond bounded read")?;
    stable(&file, &before, path)?;
    let sha = hex(&Sha256::digest(&bytes));
    ensure(sha == expected_sha, "fixture SHA differs from original pinned cohort fixture (checked before JSON decode)")?;
    Ok((serde_json::from_slice(&bytes).map_err(|e| e.to_string())?, sha))
}
fn stream_sha(file: &mut File, expected_bytes: u64) -> Result<String> {
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 << 10];
    let mut bounded = Read::by_ref(file).take(expected_bytes.checked_add(1).ok_or("SHA read limit overflow")?);
    let mut total = 0u64;
    loop {
        let n = bounded.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total = total.checked_add(n as u64).ok_or("SHA byte counter overflow")?;
        hash.update(&buffer[..n]);
    }
    ensure(total == expected_bytes, "artifact length changed during capped SHA read")?;
    Ok(hex(&hash.finalize()))
}
fn context(ctx: &PalwJobContextV2) -> Result<Value> {
    let bytes = borsh::to_vec(ctx).map_err(|e| e.to_string())?;
    let positions = u64::from(ctx.declared_prefill_tokens)
        .checked_add(u64::from(ctx.exact_decode_tokens))
        .and_then(|n| n.checked_sub(1))
        .ok_or("context position arithmetic")?;
    Ok(json!({"borsh_hex":hex(&bytes),"borsh_sha256":hex(&Sha256::digest(&bytes)),
        "prefill_tokens":ctx.declared_prefill_tokens,"decode_tokens":ctx.exact_decode_tokens,
        "executed_positions":positions,"max_context_tokens":ctx.max_context_tokens,
        "shape_profile_id":ctx.shape_profile_id.to_string(),"trace_scheme_id":ctx.trace_scheme_id.to_string(),
        "tokenizer_id":ctx.tokenizer_id.to_string(),"network_id_hex":hex(&ctx.network_id)}))
}
fn error_status(error: &str) -> &'static str {
    if error == PALW_TIR_CLOSE_SIZING_OVER_CAP_V1 {
        "SIZING_CAP_REFUSED"
    } else if error.contains("overflow") {
        "ARITHMETIC_ERROR"
    } else if error.contains("allocat") || error.contains("resource") {
        "RESOURCE_ERROR"
    } else if error.contains("not implemented") || error.contains("unsupported") {
        "UNSUPPORTED"
    } else {
        "FORMAT_OR_GEOMETRY_ERROR"
    }
}
// Numeric data projection only; no V2 -> V1 bound or admission conversion.
type BoundData = (u8, u16, Option<u16>, bool, u64, u64);
fn summarize_bounds(bounds: impl Iterator<Item = BoundData>, expected: &BTreeSet<(u8, u16)>, work: u64) -> Result<Value> {
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    let (mut terminal, mut root, mut dissected) = (0u64, 0u64, 0usize);
    for (block, node, checkpoint, is_dissected, close_bytes, root_bytes) in bounds {
        ensure(checkpoint.is_none(), "ordinary TIR sizing unexpectedly returned a GEN checkpoint")?;
        ensure(seen.insert((block, node)), "duplicate sizing commit key")?;
        terminal = terminal.max(close_bytes);
        if is_dissected {
            root = root.max(root_bytes);
            dissected += 1;
        }
        rows.push(json!({"block":block,"node":node,"checkpoint":checkpoint,"dissected":is_dissected,
            "terminal_close_bytes":close_bytes,"root_claim_bytes":root_bytes}));
    }
    ensure(!rows.is_empty() && &seen == expected, "sizing returned an incomplete or unexpected commit-key set")?;
    ensure(work <= PALW_TIR_CLOSE_SIZING_WORK_CAP_V1, "sizing success exceeds its fixed work cap")?;
    rows.sort_by_key(|row| (row["block"].as_u64(), row["node"].as_u64()));
    Ok(json!({"status":"FULL_TRAVERSAL_COMPLETE","full_traversal_complete":true,"sizing_work":work,
        "coverage":{"ordinary_commit_points":expected.len(),"returned_bounds":rows.len(),"exact_key_set":true,
            "layer_occurrences_multiplied":false,"gen_checkpoints_expected":false},
        "typed_max_terminal_close_bytes":terminal,"typed_max_dissected_root_claim_bytes":root,
        "dissected_points":dissected,"bounds":rows,
        "root_claim_format_comparison":{"scope":"FIXED_ROOT_ONE_CARRIER_FORMAT_BOUND_ONLY",
            "cap_bytes":PALW_TIR_DISSECT_CARRIER_BYTES_V1,"maximum_bytes":root,
            "fits":root <= PALW_TIR_DISSECT_CARRIER_BYTES_V1,"applicable":dissected != 0},
        "protocol_terminal_comparison":"NOT_CHECKED_MISSING_ACTUAL_COURT",
        "coarse_close_bytes":{"status":"NOT_CHECKED_MISSING_ADMISSION_RULES","value":null},
        "coarse_missing_dependencies":["original model_rules.court/F7/window", "original demand rules", "original fence ceilings"],
        "coarse_typed_envelope":{"status":"NOT_CHECKED_MISSING_ADMISSION_RULES","value":null}}))
}
fn sizing_case(
    wide: bool,
    court: bool,
    range: bool,
    expected: &BTreeSet<(u8, u16)>,
    narrow: Option<(&PalwTirStepSpaceV1, &PalwTirInventoryIndexV1)>,
    wide_space: Option<&PalwTirStepSpaceV2>,
    longest: &PalwJobContextV2,
) -> Value {
    let started = Instant::now();
    let result = if wide {
        let sizing = PalwTirCloseSizingV2 {
            form: PalwTirParamFormV1::Multiproof,
            court,
            cap: PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
            stop_above: None,
        };
        wide_space.ok_or_else(|| "missing typed wide geometry".to_string()).and_then(|space| {
            let measured = if range {
                palw_tir_worst_closes_range_work_v2(space, longest, &sizing)
            } else {
                palw_tir_worst_closes_work_v2(space, longest, &sizing)
            }?;
            summarize_bounds(
                measured.bounds().iter().map(|b| (b.block, b.node, b.checkpoint, b.dissected, b.close_bytes, b.root_claim_bytes)),
                expected,
                measured.sizing_work(),
            )
        })
    } else {
        let sizing = PalwTirCloseSizingV1 {
            form: PalwTirParamFormV1::Multiproof,
            court,
            cap: PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
            stop_above: None,
        };
        narrow.ok_or_else(|| "missing legacy narrow geometry".to_string()).and_then(|(space, inventory)| {
            let (bounds, work) = if range {
                palw_tir_worst_closes_range_work_v1(space, inventory, longest, &sizing)
            } else {
                palw_tir_worst_closes_work_v1(space, inventory, longest, &sizing)
            }?;
            summarize_bounds(
                bounds.iter().map(|b| (b.block, b.node, b.checkpoint, b.dissected, b.close_bytes, b.root_claim_bytes)),
                expected,
                work,
            )
        })
    };
    let mut result = match result {
        Ok(value) => value,
        Err(error) => json!({"status":error_status(&error),"error":error,"full_traversal_complete":false,
            "sizing_work":null,"work_measurement":"API does not return partial work on error",
            "typed_max_terminal_close_bytes":null,"typed_max_dissected_root_claim_bytes":null,
            "protocol_terminal_comparison":"NOT_CHECKED_MISSING_ACTUAL_COURT",
            "coarse_close_bytes":{"status":"NOT_CHECKED_MISSING_ADMISSION_RULES","value":null},
            "coarse_typed_envelope":{"status":"NOT_CHECKED_MISSING_ADMISSION_RULES","value":null}}),
    };
    result["profile_api"] = json!(if wide { "EXPLICIT_TYPED_V2_WIDE" } else { "LEGACY_V1_NARROW" });
    result["twin"] = json!(if range { "range" } else { "element" });
    result["diagnostic_court"] = json!(court);
    result["court_source"] = json!("EXPLICIT_CALLER_ASSUMPTION_NOT_PROTOCOL_RULES");
    result["parameter_form"] = json!("Multiproof");
    result["step_lane_price"] = json!(if wide { "CONSERVATIVE_ALL_8B_MIXED_WIRE_UNCHANGED" } else { "LEGACY_V1" });
    result["work_cap"] = json!(PALW_TIR_CLOSE_SIZING_WORK_CAP_V1);
    result["stop_above"] = Value::Null;
    match context(longest) {
        Ok(value) => result["analysis_probe_borsh_sha256"] = value["borsh_sha256"].clone(),
        Err(error) => {
            result["status"] = json!("CONTEXT_ENCODING_ERROR");
            result["full_traversal_complete"] = json!(false);
            result["error"] = json!(error);
        }
    }
    result["elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
    result
}
fn execute(o: &Options, report: &mut Value) -> Result<bool> {
    let pin = model(&o.model)?;
    report["model"] = json!(o.model);
    report["revision"] = json!(pin.revision);
    report["original_max_context"] = json!(pin.original);
    report["diagnostic_options"] = json!({"leaf_cap":o.leaf_cap,"court_assumptions":o.courts,
        "protocol_ladder":null,"sizing_work_cap":PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,"stop_above":null});
    report["phase"] = json!("fixture_identity");
    let (fixture, fixture_sha) = read_fixture(&o.fixture, pin.fixture_sha)?;
    ensure(string(&fixture, "schema")? == "misaka.tir.real-artifact-registration-fixture.v1", "fixture schema mismatch")?;
    ensure(fixture["registration_performed"].as_bool() == Some(false), "expected original class-only export")?;
    report["fixture_sha256"] = json!(fixture_sha);
    report["phase"] = json!("artifact_sha_and_header");
    let started = Instant::now();
    let mut file = File::open(&o.artifact).map_err(|e| e.to_string())?;
    let before = file.metadata().map_err(|e| e.to_string())?;
    ensure(before.is_file() && before.len() == pin.bytes, "original artifact file/length changed")?;
    let artifact_sha = stream_sha(&mut file, pin.bytes)?;
    stable(&file, &before, &o.artifact)?;
    ensure(
        artifact_sha == pin.artifact_sha && artifact_sha == string(&fixture, "artifact_sha256")?,
        "original artifact SHA mismatch",
    )?;
    ensure(fixture["artifact_bytes"].as_u64() == Some(pin.bytes), "fixture artifact length mismatch")?;
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let limits = PalwTirHeaderLimitsV1 {
        max_file_bytes: pin.bytes,
        max_header_bytes: INPUT_CAP,
        max_program_bytes: MAX_PROGRAM_BYTES as u64,
        max_layout_bytes: INPUT_CAP,
        max_tensors: 1 << 16,
        max_meta_bytes: INPUT_CAP,
        max_native_bytes: 16 << 20,
        max_copy_bytes: 16 << 20,
        max_work: 1 << 40,
        max_operation_bytes: 64 << 20,
        external_retained_bytes: 0,
    };
    let header = read_header_bounded_v1(&mut file, before.len(), limits).map_err(|e| e.to_string())?;
    let raw = header.header();
    ensure(raw.program == unhex(string(&fixture, "program_borsh_hex")?)?, "original program bytes mismatch")?;
    ensure(raw.layout == unhex(string(&fixture, "layout_borsh_hex")?)?, "original layout bytes mismatch")?;
    ensure(Hash64::from_bytes(raw.tokenizer_id) == hash64(string(&fixture, "tokenizer_id_hex")?)?, "original tokenizer mismatch")?;
    let class = PalwTirClassV1 {
        version: 1,
        program: raw.program.clone(),
        layout: PalwTirLayoutV1::try_from_slice(&raw.layout).map_err(|e| e.to_string())?,
        tokenizer_id: Hash64::from_bytes(raw.tokenizer_id),
    };
    let program = class.decode_program().map_err(|e| e.to_string())?;
    header.validate_with_program(&program, limits).map_err(|e| e.to_string())?;
    ensure(
        class.layout.max_context == pin.original && fixture["max_context"].as_u64() == Some(u64::from(pin.original)),
        "original maximum context changed",
    )?;
    ensure(program.wide_logits_q24() == pin.wide, "original narrow/wide profile changed")?;
    let root = hash64(string(&fixture, "artifact_root_hex")?)?;
    let class_id = class.class_id(&root);
    ensure(class_id == hash64(string(&fixture, "class_id_hex")?)?, "class identity mismatch")?;
    let class_bytes = borsh::to_vec(&class).map_err(|e| e.to_string())?;
    report["identity"] = json!({"artifact_path":o.artifact,"artifact_realpath":fs::canonicalize(&o.artifact).map_err(|e|e.to_string())?,
        "fixture_path":o.fixture,"fixture_realpath":fs::canonicalize(&o.fixture).map_err(|e|e.to_string())?,
        "artifact_bytes":before.len(),"artifact_sha256":artifact_sha,"fixture_sha256":fixture_sha,
        "class_borsh_sha256":hex(&Sha256::digest(&class_bytes)),"program_sha256":hex(&Sha256::digest(&class.program)),
        "layout_borsh_sha256":hex(&Sha256::digest(&raw.layout)),"tokenizer_id":class.tokenizer_id.to_string(),
        "artifact_root_from_original_fixture":root.to_string(),"class_id":class_id.to_string(),
        "full_artifact_sha_streamed":true,"weight_inventory_root_recomputed":false,
        "artifact_sha256_verified":true,"inventory_root_recomputed":false,"artifact_root_source":"FIXTURE_DECLARED",
        "class_id_recomputed_from_declared_root":true,
        "weights_verified":false,"weight_verification_scope":"WHOLE_FILE_SHA_IDENTITY_ONLY; inventory root not independently recomputed",
        "program_layout_tokenizer_exact":true,"program_history_bound":program.history_bound,
        "header_parser":"read_header_bounded_v1 + validate_with_program",
        "parser_limits":{"max_file_bytes":limits.max_file_bytes,"max_header_bytes":limits.max_header_bytes,"max_program_bytes":limits.max_program_bytes,
            "max_layout_bytes":limits.max_layout_bytes,"max_tensors":limits.max_tensors,"max_meta_bytes":limits.max_meta_bytes,
            "max_native_bytes":limits.max_native_bytes,"max_copy_bytes":limits.max_copy_bytes,"max_work":limits.max_work,
            "max_operation_bytes":limits.max_operation_bytes,"external_retained_bytes":0},
        "parser_limit_scope":"PARSER_INTERNAL_ONLY; external owners and TIR decode/geometry/sizing excluded",
        "identity_setup_seconds":started.elapsed().as_secs_f64()});
    report["class_id"] = json!(class_id.to_string());
    report["artifact_sha256"] = json!(artifact_sha);
    report["phase"] = json!("canonical_and_geometry");
    let profile = ResolvedTirJobProfileV2::new(&class, root, MAX_PROGRAM_BYTES).map_err(|e| format!("profile: {e:?}"))?;
    let canonical = if pin.wide {
        profile.ctx().ok_or("missing wide canonical")?
    } else {
        let facts = PalwTirJobFactsV1::of_class(&class, class_id).map_err(|e| e.to_string())?;
        palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&class).ok_or("missing narrow canonical")?)
    };
    ensure(profile.ctx().as_ref() == Some(&canonical), "legacy/resolved canonical mismatch")?;
    ensure(
        canonical.declared_prefill_tokens == pin.original / 8 - 1
            && canonical.exact_decode_tokens == 2
            && canonical.max_context_tokens == pin.original + 1,
        "original canonical yardstick changed",
    )?;
    let mut longest = canonical.clone();
    longest.declared_prefill_tokens = 1;
    longest.exact_decode_tokens = pin.original;
    longest.max_context_tokens = u32::MAX;
    let mut expected = BTreeSet::new();
    for (bi, block) in program.blocks.iter().enumerate() {
        for (ni, node) in block.nodes.iter().enumerate() {
            if node.commit {
                expected.insert((
                    u8::try_from(bi).map_err(|_| "block index overflow")?,
                    u16::try_from(ni).map_err(|_| "node index overflow")?,
                ));
            }
        }
    }
    ensure(!expected.is_empty(), "no committed nodes")?;
    let narrow = if pin.wide { None } else { Some(PalwTirStepSpaceV1::new(&class).map_err(|e| e.to_string())?) };
    let wide = if pin.wide { Some(PalwTirStepSpaceV2::new(&class).map_err(|e| e.to_string())?) } else { None };
    let (canonical_count, longest_count) = if let Some(space) = &wide {
        (space.leaf_count_capped(&canonical, o.leaf_cap), space.leaf_count_capped(&longest, o.leaf_cap))
    } else {
        let space = narrow.as_ref().ok_or("missing narrow space")?;
        (space.leaf_count_capped(&canonical, o.leaf_cap), space.leaf_count_capped(&longest, o.leaf_cap))
    };
    report["canonical"] = context(&canonical)?;
    report["longest_analysis_probe"] = context(&longest)?;
    report["longest_analysis_probe"]["scope"] = json!("SEPARATE_GEOMETRY_PROBE_NOT_CPU_EXECUTION_OR_JOB_AUTHORITY");
    let count_result = |result: std::result::Result<u64, _>| match result {
        Ok(n) => json!({"status":"COUNTED","value":n}),
        Err(e) => json!({"status":"DIAGNOSTIC_COUNT_REFUSED","error":format!("{e}"),"value":null}),
    };
    let counts_complete = canonical_count.is_ok() && longest_count.is_ok();
    report["geometry"] = json!({"profile":format!("{:?}",profile.profile()),"original_layout_max_positions":pin.original,
        "canonical_leaf_count":count_result(canonical_count),"worst_leaf_count":count_result(longest_count),
        "diagnostic_leaf_cap":o.leaf_cap,"protocol_ladder_comparison":"NOT_CHECKED_MISSING_ACTUAL_LADDER",
        "commit_points":expected.len(),"commit_tile_entries":class.layout.commit_tiles.len(),
        "state_tile_entries":class.layout.state_tiles.len(),"checkpoint_interval":class.layout.checkpoint_interval,
        "h_tile":class.layout.h_tile,"token_bound":program.token_bound,"program_unmodified":true});
    report["phase"] = json!("full_close_sizing");
    let inventory =
        if pin.wide { None } else { Some(PalwTirInventoryIndexV1::new(&program).ok_or("unrepresentable inventory index")?) };
    report["close_cases"] = json!([]);
    report["twin_comparisons"] = json!([]);
    let mut complete = counts_complete;
    for &court in &o.courts {
        let element = sizing_case(pin.wide, court, false, &expected, narrow.as_ref().zip(inventory.as_ref()), wide.as_ref(), &longest);
        let range = sizing_case(pin.wide, court, true, &expected, narrow.as_ref().zip(inventory.as_ref()), wide.as_ref(), &longest);
        let comparable =
            element["full_traversal_complete"].as_bool() == Some(true) && range["full_traversal_complete"].as_bool() == Some(true);
        let equal = comparable && element["bounds"] == range["bounds"];
        complete &= equal;
        report["twin_comparisons"].as_array_mut().ok_or("twin comparison array")?.push(json!({
            "diagnostic_court":court,"status":if !comparable {"NOT_COMPARABLE_INCOMPLETE"} else if equal {"MATCH"} else {"MISMATCH"},
            "both_full_traversals_complete":comparable,"bounds_equal":if comparable {Some(equal)} else {None},
            "compared":"canonical (block,node,checkpoint,dissected,terminal_close_bytes,root_claim_bytes) data only",
            "work_counters_expected_equal":false,"cross_court_comparison":false,
            "scope":"SIZING_TWIN_CONSISTENCY_ONLY_NOT_INDEPENDENT_MODEL_EXPECTED"}));
        report["close_cases"].as_array_mut().ok_or("close case array")?.extend([element, range]);
    }
    stable(&file, &before, &o.artifact)?;
    // Recheck the small pinned fixture and its path identity after the analysis.
    read_fixture(&o.fixture, pin.fixture_sha)?;
    report["identity"]["artifact_stat_stable_through_analysis"] = json!(true);
    report["identity"]["fixture_sha_rechecked_after_analysis"] = json!(true);
    report["phase"] = json!("complete");
    Ok(complete)
}
fn initial_report() -> Value {
    json!({"schema":SCHEMA,"mode":"class-only-analysis","scope":"CLASS_ONLY_DATA_ANALYSIS","status":"NOT_STARTED",
        "source_base":"07d7eaebad0021164476c6289d8ccbbcbc45862c","phase":"arguments",
        "protocol_admission":"NOT_CHECKED_MISSING_ACTUAL_RULES_COURT_CEILINGS",
        "constructor":"NOT_RUN_MISSING_REGISTRATION_INPUTS","full_pure_gate":"NOT_RUN_MISSING_REGISTRATION_INPUTS",
        "missing_registration_inputs":["authentic original share_permille","authentic original initial_target",
            "authentic original slash_value_per_pwu","authentic original activation_daa","authentic original registrant_bond",
            "authentic original signature","actual registration rules/DAA/bundle/ladder","authenticated genesis/chain family-certified facts"],
        "economic_or_authentication_fields_synthesized":false,"protocol_params_modified":false,
        "cpu_executed":false,"native_v2_admission_executed":false,"credit_executed":false,
        "independent_expected_executed":false,"registered_or_rooted_state_installed":false,
        "startup_allocation_before_limit_guaranteed":false,"whole_process_rss_bounded":false,
        "host_reserver_installed":false,"native_lease":false,
        "resource_scope":"logical local parser/sizing operation limits only; no total RAM/work/fee/native resource guarantee",
        "external_owners_excluded_from_parser_quote":["fixture JSON/raw hex owners","decoded TIR program/class/layout",
            "StepSpace and inventory metadata","read maps and sizing caches","bounds and JSON export buffers"],
        "whole_process_observation":"requires separately authorized external timeout/RSS observation",
        "coarse_close_bytes":{"status":"NOT_CHECKED_MISSING_ADMISSION_RULES","value":null},
        "coarse_missing_dependencies":["original model_rules.court/F7/window", "original demand rules", "original fence ceilings"],
        "terminal_carriable_bytes":{"status":"NOT_CHECKED_MISSING_ACTUAL_COURT","value":null},
        "root_claim_one_carrier_format_cap_bytes":PALW_TIR_DISSECT_CARRIER_BYTES_V1})
}
fn save(path: &Path, report: &Value) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut file, report).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}
fn main() {
    let mut report = initial_report();
    let parsed = options();
    let mut out = None;
    let mut exit_code = 1;
    match parsed {
        Ok(o) => {
            out = Some(o.out.clone());
            match execute(&o, &mut report) {
                Ok(true) => {
                    report["status"] = json!("CLASS_ONLY_ANALYSIS_COMPLETE");
                    exit_code = 0;
                }
                Ok(false) => {
                    report["status"] = json!("CLASS_ONLY_ANALYSIS_INCOMPLETE");
                    exit_code = 2;
                }
                Err(error) => {
                    report["status"] = json!("ANALYSIS_ERROR");
                    report["error"] = json!(error);
                }
            }
        }
        Err(error) => {
            report["status"] = json!("ARGUMENT_ERROR");
            report["error"] = json!(error);
        }
    }
    // Preserve a structured error when the CLI includes --out, even if another
    // option is invalid. Never overwrite an existing artifact, fixture or report.
    if out.is_none() {
        let args: Vec<_> = std::env::args().collect();
        out = args.windows(2).find(|pair| pair[0] == "--out").map(|pair| PathBuf::from(&pair[1]));
    }
    if let Some(path) = out {
        if let Err(error) = save(&path, &report) {
            eprintln!("cannot save class-only report: {error}");
            exit_code = 1;
        }
    } else {
        eprintln!("{report}");
    }
    std::process::exit(exit_code);
}
