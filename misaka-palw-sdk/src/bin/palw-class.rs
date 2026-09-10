//! **`palw-class` — the operator's window into the SDK, before any coin moves.**
//!
//! Three questions, answered offline from the network's own ruleset:
//!
//! * `ledger` — what classes can this build supply, and which does the network's genesis register?
//! * `inspect` — which class does this artifact file pair with, under which root, and why not the
//!   others?
//! * `preflight` — would the admission gate accept this pairing on this network? Asked BEFORE a
//!   registration is signed or funded, because the same refusal after submission costs the carrier
//!   fee — and a wrong pairing burned a class seat once already.
//!
//! The genesis view is static: a LIVE chain may have registered more classes since genesis, and
//! the node's own `--palw-register-class` path reads live terms for exactly that reason. This tool
//! is the dry run, not the submission.

use std::path::PathBuf;

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_hashes::Hash64;
use misaka_palw_sdk::PalwClassSdk;

const USAGE: &str = "palw-class — inspect and preflight PALW model classes through the SDK

USAGE:
    palw-class ledger    --network <id>
    palw-class inspect   --network <id> <artifact-path>
    palw-class preflight --network <id> <artifact-path> [--model-id <model-id>]
    palw-class bind-tokenizer --network <id> --tokenizer <tokenizer.json> --out <path> [--model-id <model-id>] <artifact-path>
    palw-class measure   --network <id> [--name <model name>] [--replay-ms <ms> --measured-on <host>]
                         [--key-file <ml-dsa-87 seed>] [--out <measured.json>] <artifact-path>
    palw-class verify    --network <id> [--artifact <artifact-path>] <measured.json>

`measure` (ADR-0100) reads the geometry off the artifact, measures its bytes from the inventory,
evaluates every wall of ADR-0097 at 512 / 32,768 / 131,072 / 1,048,576 positions and the shard
plans a seat budget allows, and writes the Measured Model Artifact — signed under the bond key
when --key-file is given. `verify` recomputes a document: with --artifact every field, without it
everything but the artifact's own (which it names as needing the artifact); exits 1 on a mismatch
or a signature that does not verify.

NETWORKS: a network id with a PALW V2 bundle, e.g. testnet-11 or devnet.

`preflight` exits 0 only if every requested pairing passes the admission gate.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

struct NetworkView {
    bundle: PalwConsensusParamsV2,
    network_id: NetworkId,
    /// The preset itself — the fences (`palw_kary_court`, `palw_context_ladder`) live here, not
    /// on the bundle, and the gate's shape is resolved from them.
    params: Params,
    /// `(class_id, artifact_root)` of every class the GENESIS registers — the static half of the
    /// live chain's terms.
    genesis_classes: Vec<(Hash64, Hash64)>,
}

fn network_view(raw: &str) -> Result<NetworkView, String> {
    let network_id: NetworkId = raw.parse().map_err(|e| format!("--network {raw}: {e}"))?;
    let params: Params = network_id.into();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err(format!("{network_id} has no PALW V2 bundle, so it has no classes to speak of"));
    };
    let bundle = bundle.clone();
    let genesis_classes = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } => Some((*class_id, *artifact_root)),
            _ => None,
        })
        .collect();
    Ok(NetworkView { bundle, network_id, genesis_classes, params })
}

fn sdk_for(view: &NetworkView) -> PalwClassSdk {
    PalwClassSdk::builtin_v1(view.bundle.court, view.params.palw_prompt_ids_form_v1(), view.network_id.to_string().into_bytes())
}

/// Pull `--flag value` out of the argument list, leaving positionals in place.
fn take_flag(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    if i + 1 >= args.len() {
        return None;
    }
    args.remove(i);
    Some(args.remove(i))
}

fn run(args: &[String]) -> Result<(), String> {
    let mut args = args.to_vec();
    let command = if args.is_empty() { String::new() } else { args.remove(0) };
    let network = take_flag(&mut args, "--network");
    match command.as_str() {
        "ledger" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            ledger(&view);
            Ok(())
        }
        "inspect" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let path = PathBuf::from(args.first().ok_or(USAGE)?);
            inspect(&view, &path)
        }
        "preflight" => {
            let wanted = take_flag(&mut args, "--model-id");
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let path = PathBuf::from(args.first().ok_or(USAGE)?);
            preflight(&view, &path, wanted.as_deref())
        }
        "measure" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let name = take_flag(&mut args, "--name");
            let replay_ms = match take_flag(&mut args, "--replay-ms") {
                Some(v) => Some(v.parse::<u64>().map_err(|e| format!("--replay-ms {v}: {e}"))?),
                None => None,
            };
            let measured_on = take_flag(&mut args, "--measured-on").unwrap_or_else(|| "not measured".to_string());
            let key_file = take_flag(&mut args, "--key-file");
            let out = take_flag(&mut args, "--out");
            let path = args.first().ok_or(USAGE)?;
            measure(&view, std::path::Path::new(path), name, replay_ms, &measured_on, key_file.as_deref(), out.as_deref())
        }
        "verify" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let artifact = take_flag(&mut args, "--artifact");
            let path = args.first().ok_or(USAGE)?;
            verify(&view, std::path::Path::new(path), artifact.as_deref().map(std::path::Path::new))
        }
        "bind-tokenizer" => {
            let tokenizer = take_flag(&mut args, "--tokenizer").ok_or(USAGE)?;
            let out = take_flag(&mut args, "--out").ok_or(USAGE)?;
            let wanted = take_flag(&mut args, "--model-id");
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let path = PathBuf::from(args.first().ok_or(USAGE)?);
            bind_tokenizer(&view, &path, &PathBuf::from(tokenizer), &PathBuf::from(out), wanted.as_deref())
        }
        _ => Err(USAGE.to_string()),
    }
}

/// **ADR-0096 Decision 10: bind a tokenizer into a converted artifact, and MEASURE that the
/// class's registered root did not move.**
///
/// The binding itself is `misaka_palw_base0::artifact::bind_tokenizer_file_v1` (one field set,
/// read back bound). What this command adds is the measurement a person needs before they trust
/// the output: the SDK's own pairing is run on the input and on the output, and the root each
/// pairs to is printed side by side.
///
/// **Two kinds of row answer differently, and both answers are printed** (measured 2026-09-10 on
/// the published dense file): a row that registers the INVENTORY root — the tiled-map rows,
/// `graph-v2`, `graph-v3`, `graph-v5@512` — keeps its root, because the tokenizer commitment is not
/// an input to it; a row that registers the artifact DIGEST — the one-byte-map rows — moves,
/// because the commitment is inside the digest, and for that row the output is a new artifact.
/// The gate is the class the person is binding FOR: `--model-id` names it, and the command fails
/// (and deletes the output) if that row's root moved; with no `--model-id`, it fails only if
/// every row moved. A file that registers somewhere else is not the file the person asked for.
///
/// Written to `<out>.tmp` and renamed, so a crash leaves no half file under the name a runtime
/// looks for. Holds the artifact in memory twice (about 3.6 GB for the 1.7 GB dense file).
fn bind_tokenizer(
    view: &NetworkView,
    input: &std::path::Path,
    tokenizer: &std::path::Path,
    out: &std::path::Path,
    wanted: Option<&str>,
) -> Result<(), String> {
    use misaka_palw_base0::artifact::{TokenizerBindOutcomeV1, bind_tokenizer_file_v1};
    if out == input {
        return Err("--out must not be the input: the input is the evidence the output is compared against".to_string());
    }
    let artifact_bytes = std::fs::read(input).map_err(|e| format!("{}: {e}", input.display()))?;
    let tokenizer_bytes = std::fs::read(tokenizer).map_err(|e| format!("{}: {e}", tokenizer.display()))?;
    let outcome = bind_tokenizer_file_v1(&artifact_bytes, &tokenizer_bytes)?;
    drop(artifact_bytes);
    let (bytes, commitment, before, after) = match outcome {
        TokenizerBindOutcomeV1::AlreadyBound { commitment, artifact_digest } => {
            println!("already bound: {} declares tokenizer {commitment} (artifact digest {artifact_digest}); nothing written", input.display());
            return Ok(());
        }
        TokenizerBindOutcomeV1::Bound { bytes, commitment, digest_before, digest_after } => (bytes, commitment, digest_before, digest_after),
    };
    let tmp = out.with_extension("tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    drop(bytes);
    std::fs::rename(&tmp, out).map_err(|e| format!("{} -> {}: {e}", tmp.display(), out.display()))?;

    let sdk = sdk_for(view);
    let roots = |path: &std::path::Path| -> Result<Vec<(String, Result<Hash64, String>)>, String> {
        let loaded = sdk.load_artifact(path)?;
        Ok(sdk.pairings(&loaded).into_iter().map(|(entry, root)| (entry.model_id.to_string(), root)).collect())
    };
    let (input_roots, output_roots) = (roots(input)?, roots(out)?);
    let (mut kept, mut moved) = (Vec::new(), Vec::new());
    for ((model, a), (_, b)) in input_roots.iter().zip(output_roots.iter()) {
        if let (Ok(a), Ok(b)) = (a, b) {
            if a == b {
                println!("  {model}: registered root {a}  (unchanged — this row registers the inventory root)");
                kept.push(model.clone());
            } else {
                println!("  {model}: registered root {a} → {b}  (MOVED — this row registers the artifact digest; for it this is a new artifact)");
                moved.push(model.clone());
            }
        }
    }
    let refusal = match wanted {
        Some(model) if moved.iter().any(|m| m == model) => Some(format!("binding moved the registered root of {model}, the class named by --model-id")),
        Some(model) if !kept.iter().any(|m| m == model) => Some(format!("{model} does not pair with this artifact — run `palw-class inspect`")),
        None if kept.is_empty() => Some("binding moved the registered root of every class this artifact pairs with".to_string()),
        _ => None,
    };
    if let Some(why) = refusal {
        let _ = std::fs::remove_file(out);
        return Err(format!("{why} — the output was deleted"));
    }
    println!("tokenizer commitment {commitment}");
    println!("artifact digest      {before} → {after}");
    println!("wrote                {}", out.display());
    Ok(())
}

fn ledger(view: &NetworkView) {
    let sdk = sdk_for(view);
    println!("classes this build can supply, against {}:", view.network_id);
    for entry in sdk.ledger() {
        let class_id = entry.class_id();
        let registered =
            if view.genesis_classes.iter().any(|(id, _)| *id == class_id) { "genesis-registered" } else { "unregistered" };
        let file = if entry.needs_artifact_file { "needs artifact file" } else { "derived, no file" };
        println!("  {}  [{}]", entry.model_id, entry.lineage_id);
        println!("    class id   {class_id}");
        println!(
            "    canonical  prefill {} / decode {}   {file}   {registered} (genesis view)",
            entry.canonical_job.0, entry.canonical_job.1
        );
    }
}

fn inspect(view: &NetworkView, path: &std::path::Path) -> Result<(), String> {
    let sdk = sdk_for(view);
    let artifact = sdk.load_artifact(path)?;
    println!("{}", artifact.summary);
    println!("lineage: {}", artifact.lineage_id);
    for (entry, paired) in sdk.pairings(&artifact) {
        match paired {
            Ok(root) => {
                let taken = view.genesis_classes.iter().any(|(id, _)| *id == entry.class_id());
                let status = if taken { " (class already in genesis)" } else { "" };
                println!("  PAIRS   {}  root {root}{status}", entry.model_id);
            }
            Err(why) => println!("  no      {}  — {why}", entry.model_id),
        }
    }
    Ok(())
}

fn preflight(view: &NetworkView, path: &std::path::Path, wanted: Option<&str>) -> Result<(), String> {
    let sdk = sdk_for(view);
    let artifact = sdk.load_artifact(path)?;
    println!("{}", artifact.summary);
    let pairings: Vec<_> = sdk
        .pairings(&artifact)
        .into_iter()
        .filter_map(|(entry, paired)| paired.ok().map(|root| (entry, root)))
        .filter(|(entry, _)| wanted.is_none_or(|w| w == entry.model_id))
        .collect();
    if pairings.is_empty() {
        return Err(match wanted {
            Some(w) => format!("this artifact pairs with no class named {w} — run `palw-class inspect` for the reasons"),
            None => "this artifact pairs with no class this build knows — run `palw-class inspect` for the reasons".to_string(),
        });
    }
    let mut refused = false;
    for (entry, root) in pairings {
        if let Some((_, genesis_root)) = view.genesis_classes.iter().find(|(id, _)| *id == entry.class_id()) {
            if *genesis_root == root {
                println!("  ALREADY REGISTERED  {}  — this exact (class, root) is in {}'s genesis", entry.model_id, view.network_id);
            } else {
                println!(
                    "  REFUSED   {}  — the class is in {}'s genesis under root {genesis_root}, and this artifact roots to {root}: \
                     different weights",
                    entry.model_id, view.network_id
                );
                refused = true;
            }
            continue;
        }
        // The gate's shape at genesis (DAA 0): the fences a shipped preset arms are `always()`,
        // so the genesis point answers for the chain's whole life. A live chain judges at the
        // virtual DAA, which is the panel's reading, not this offline one.
        let shape = match kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1(
            &view.params,
            &view.bundle,
            &entry.profile,
            0,
        ) {
            Ok(shape) => shape,
            Err(why) => {
                refused = true;
                println!("  REFUSED   {}  — {why}", entry.model_id);
                continue;
            }
        };
        match sdk.preflight_admission(&view.bundle, &entry, root, &shape) {
            Ok(catalog) => println!(
                "  ADMISSIBLE  {}  root {root}  pwu/inference {}  (gate verdict on {}; a live chain may hold more classes than genesis)",
                entry.model_id, catalog.canonical_step_leaf_count, view.network_id
            ),
            Err(why) => {
                refused = true;
                println!("  REFUSED   {}  — {why}", entry.model_id);
            }
        }
    }
    if refused {
        Err("at least one pairing would be refused — nothing should be signed or funded for it".to_string())
    } else {
        Ok(())
    }
}

// =================================================================================================
// ADR-0100 — `measure` and `verify`: the Measured Model Artifact from a HELD artifact
// =================================================================================================

use kaspa_consensus_core::palw_measured_model_v1::{
    PALW_MEASURED_MODEL_MLDSA87_CONTEXT, PalwHeldArtifactV1, PalwMeasureInputsV1, PalwMeasuredCheckV1, PalwMeasuredModelV1,
    PalwModelManifestV1, palw_measure_model_v1, palw_measured_model_id_v1, palw_verify_measured_model_v1,
};
use kaspa_consensus_core::palw_shard_plan_v1::{PalwArtifactBytesV1, PalwInventoryRowMetaV1, palw_artifact_bytes_from_inventory_v1};

const MEASURE_CONTEXTS: [u32; 4] = [512, 32_768, 131_072, 1_048_576];
const MEASURE_SEAT_GIBS: [u64; 4] = [24, 64, 128, 512];
/// The point of judgement the fences are read at: every scheduled fence armed, `never()` dormant.
const MEASURE_EVER: u64 = u64::MAX - 1;

/// What a held artifact yields before any ruleset is consulted: its manifest (the geometry read
/// off the file), its inventory-measured bytes, and the root the court-capable row registers.
struct HeldMeasurementV1 {
    manifest: PalwModelManifestV1,
    bytes: PalwArtifactBytesV1,
    inventory_root: Hash64,
    lineage: &'static str,
    layers: u16,
}

fn held_measurement(loaded: &misaka_palw_sdk::PalwLoadedArtifactV1, name: Option<String>) -> Result<HeldMeasurementV1, String> {
    if let Some(artifact) = misaka_palw_sdk::lineages::dense::artifact_of(loaded) {
        use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, QWEN25_A16_GRAPH_V5_N_CTX};
        let sh = &artifact.shape;
        let g = PalwQwen25GeometryV1 {
            layer_count: sh.n_layers as u16,
            hidden_dim: sh.d_model() as u32,
            ffn_dim: sh.d_ff as u32,
            attn_heads: sh.n_heads as u16,
            attn_kv_heads: sh.n_kv_heads as u16,
            attn_head_dim: sh.d_head as u32,
            vocab_size: sh.vocab as u32,
            rms_eps_q: sh.eps_q,
            ..QWEN25_1_5B
        };
        let manifest = PalwModelManifestV1::from_dense(name.unwrap_or_else(|| loaded.summary.clone()), &g);
        let profile =
            manifest.profile(QWEN25_A16_GRAPH_V5_N_CTX).map_err(|e| format!("the dense manifest builds no profile: {e:?}"))?;
        let inventory = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile)
            .map_err(|e| format!("the artifact yields no inventory under its own profile: {e:?}"))?;
        let rows: Vec<PalwInventoryRowMetaV1> = inventory.operands().iter().map(PalwInventoryRowMetaV1::from).collect();
        let bytes = palw_artifact_bytes_from_inventory_v1(&profile, &rows).map_err(|e| e.to_string())?;
        return Ok(HeldMeasurementV1 {
            manifest,
            bytes,
            inventory_root: inventory.root(),
            lineage: loaded.lineage_id,
            layers: g.layer_count,
        });
    }
    if let Some((_root, artifact)) = misaka_palw_sdk::lineages::qwen36::parts_of(loaded) {
        use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN36_35B_A3B};
        use misaka_palw_base0::qwen36::Qwen36LayerKind;
        let sh = &artifact.shape;
        let interval = sh
            .layer_types
            .iter()
            .position(|k| matches!(k, Qwen36LayerKind::FullAttention))
            .map(|i| i as u16 + 1)
            .ok_or_else(|| "a hybrid artifact with no full-attention layer has no graph in this tree".to_string())?;
        let g = PalwQwen36GeometryV1 {
            layer_count: sh.n_layers() as u16,
            full_attention_interval: interval,
            hidden_dim: sh.d_model as u32,
            attn_heads: sh.n_heads as u16,
            attn_kv_heads: sh.n_kv_heads as u16,
            attn_head_dim: sh.head_dim as u32,
            rope_dims: sh.rotary_dim as u16,
            gdn_k_heads: sh.linear_k_heads as u16,
            gdn_v_heads: sh.linear_v_heads as u16,
            gdn_head_dim: sh.linear_head_dim as u32,
            gdn_conv_kernel: sh.conv_kernel as u16,
            n_experts: sh.n_experts as u32,
            experts_per_token: sh.experts_per_token as u32,
            moe_dim: sh.moe_dim as u32,
            shared_dim: sh.shared_dim as u32,
            attn_output_gate: u8::from(sh.attn_output_gate()),
            vocab_size: sh.vocab as u32,
            rms_eps_q: sh.eps_q,
            ..QWEN36_35B_A3B
        };
        // **ADR-0102: a held hybrid artifact is measured under graph-v6**, the graph that reads its
        // embedding lift the way the engine executes it (per token; a one-row store lifting every
        // token alike). Under graph-v5 the converter's calibrated store — one triple per
        // vocabulary row — has no inventory at all, and this measurement refused the Qwen3.5-2B
        // artifact by name on exactly that.
        let manifest = PalwModelManifestV1::from_hybrid_token_lift(name.unwrap_or_else(|| loaded.summary.clone()), &g, None);
        let profile = manifest.profile(MEASURE_CONTEXTS[0]).map_err(|e| format!("the hybrid manifest builds no profile: {e:?}"))?;
        let inventory = misaka_palw_base0::inventory::qwen36_inventory_v1(&artifact, &profile)
            .map_err(|e| format!("the artifact yields no inventory under its own profile: {e:?}"))?;
        let rows: Vec<PalwInventoryRowMetaV1> = inventory.operands().iter().map(PalwInventoryRowMetaV1::from).collect();
        let bytes = palw_artifact_bytes_from_inventory_v1(&profile, &rows).map_err(|e| e.to_string())?;
        return Ok(HeldMeasurementV1 {
            manifest,
            bytes,
            inventory_root: inventory.root(),
            lineage: loaded.lineage_id,
            layers: g.layer_count,
        });
    }
    Err(format!("the {} lineage has no measurement path in this build", loaded.lineage_id))
}

fn hex_bytes(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn measure_inputs<'a>(
    view: &'a NetworkView,
    name: &'a str,
    fingerprint: &'a str,
    budgets: &'a [u64],
    held: Option<PalwHeldArtifactV1<'a>>,
) -> PalwMeasureInputsV1<'a> {
    PalwMeasureInputsV1 {
        ruleset: name,
        ruleset_fingerprint_hex: fingerprint,
        bundle: &view.bundle,
        contexts: &MEASURE_CONTEXTS,
        seat_budgets: budgets,
        max_shards: 1_024,
        held,
    }
}

fn court_for_view(
    view: &NetworkView,
) -> impl Fn(
    &PalwShapeProfileV3,
) -> (
    Option<kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1>,
    kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) + '_ {
    move |profile: &PalwShapeProfileV3| {
        let shape = kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1(
            &view.params,
            &view.bundle,
            profile,
            MEASURE_EVER,
        )
        .expect("a network with a V2 bundle has an admission shape");
        (shape.court, view.params.palw_prompt_ids_form_at(MEASURE_EVER))
    }
}

fn measure(
    view: &NetworkView,
    path: &std::path::Path,
    name: Option<String>,
    replay_ms: Option<u64>,
    measured_on: &str,
    key_file: Option<&str>,
    out: Option<&str>,
) -> Result<(), String> {
    let sdk = sdk_for(view);
    let loaded = sdk.load_artifact(path)?;
    let held = held_measurement(&loaded, name)?;
    let file_bytes = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    let pairings = sdk.pairings(&loaded);
    let ruleset_name = view.network_id.to_string();
    let fingerprint = format!("{}", view.params.consensus_params_id());
    let budgets: Vec<u64> = MEASURE_SEAT_GIBS.iter().map(|g| g << 30).collect();
    let court_for = court_for_view(view);
    let inputs = measure_inputs(
        view,
        &ruleset_name,
        &fingerprint,
        &budgets,
        Some(PalwHeldArtifactV1 { bytes: &held.bytes, root: held.inventory_root, file_bytes }),
    );
    let mut doc = palw_measure_model_v1(&held.manifest, inputs, &court_for, replay_ms, measured_on);
    if let Some(key_file) = key_file {
        let seed = kaspa_pq_validator_core::load_validator_seed(key_file)?;
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
        doc.adder_pubkey_hex = hex_bytes(key.public_key());
        doc.signature_hex.clear();
        let id = palw_measured_model_id_v1(&doc);
        doc.signature_hex = hex_bytes(&key.sign_with_context(id.as_byte_slice(), PALW_MEASURED_MODEL_MLDSA87_CONTEXT));
    }
    let id = palw_measured_model_id_v1(&doc);
    let out_path = out.map(std::path::PathBuf::from).unwrap_or_else(|| {
        let mut p = path.to_path_buf();
        p.set_extension("measured.json");
        p
    });
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    std::fs::write(&out_path, text).map_err(|e| format!("{}: {e}", out_path.display()))?;

    println!("# Measured Model Artifact — {} on {ruleset_name}\n", held.manifest.name);
    println!("- artifact `{}` ({} bytes on disk), lineage `{}`, {} layers", path.display(), file_bytes, held.lineage, held.layers);
    println!(
        "- bytes measured from the inventory: **{}** ({}); the family formula says {}",
        held.bytes.total(),
        held.bytes.basis,
        held.manifest.artifact_bytes().total()
    );
    println!("- inventory root `{}`", held.inventory_root);
    for (entry, root) in &pairings {
        match root {
            Ok(r) => println!(
                "- registers as `{}` (class `{}`) with root `{r}`{}",
                entry.model_id,
                entry.profile.shape_profile_id(),
                if *r == held.inventory_root { " — the inventory root" } else { "" }
            ),
            Err(e) => println!("- `{}`: no root ({e})", entry.model_id),
        }
    }
    println!();
    println!("| n_ctx | class id | fit | refused by | cache | fewest shards per seat budget |");
    println!("|---|---|---|---|---|---|");
    for row in &doc.deterministic.rows {
        let plans: Vec<String> = row
            .plans
            .iter()
            .map(|p| {
                format!(
                    "{} GiB → {}",
                    p.seat_budget_bytes >> 30,
                    p.shard_count.map(|c| c.to_string()).unwrap_or_else(|| "none".into())
                )
            })
            .collect();
        println!(
            "| {} | `{}` | {} | {} | {:.1} GiB | {} |",
            row.n_ctx,
            row.shape_profile_id_hex.as_deref().map(|h| format!("{}…", &h[..12])).unwrap_or_else(|| "— (no profile)".into()),
            if row.fit_admitted { "admitted" } else { "**refused**" },
            row.refusing_walls.join(", "),
            row.kv_cache_bytes as f64 / (1u64 << 30) as f64,
            plans.join("; ")
        );
    }
    println!();
    match &doc.self_reported.replay_ms_per_position {
        Some(ms) => println!(
            "- self-reported: replay {ms} ms/position on \"{}\" → {} positions within window_receipt; verified by {}",
            doc.self_reported.measured_on,
            doc.self_reported.positions_within_window_receipt.map(|p| p.to_string()).unwrap_or_else(|| "—".into()),
            doc.self_reported.verified_by
        ),
        None => println!("- self-reported: no replay rate given (--replay-ms); the certification drill is what measures one"),
    }
    println!(
        "- id `{id}`; {}",
        if doc.signature_hex.is_empty() {
            "unsigned (give --key-file to sign under the bond key)".to_string()
        } else {
            format!("signed by `{}…`", &doc.adder_pubkey_hex[..16])
        }
    );
    println!("- written: `{}`", out_path.display());
    Ok(())
}

fn verify(view: &NetworkView, path: &std::path::Path, artifact: Option<&std::path::Path>) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc: PalwMeasuredModelV1 =
        serde_json::from_str(&text).map_err(|e| format!("{} is not a PalwMeasuredModelV1: {e}", path.display()))?;
    let ruleset_name = view.network_id.to_string();
    let fingerprint = format!("{}", view.params.consensus_params_id());
    let budgets: Vec<u64> =
        doc.deterministic.rows.first().map(|r| r.plans.iter().map(|p| p.seat_budget_bytes).collect()).unwrap_or_default();
    let held_owned = match artifact {
        Some(p) => {
            let sdk = sdk_for(view);
            let loaded = sdk.load_artifact(p)?;
            let held = held_measurement(&loaded, Some(doc.manifest.name.clone()))?;
            let file_bytes = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?.len();
            Some((held, file_bytes))
        }
        None => None,
    };
    let held = held_owned.as_ref().map(|(h, file_bytes)| PalwHeldArtifactV1 {
        bytes: &h.bytes,
        root: h.inventory_root,
        file_bytes: *file_bytes,
    });
    let court_for = court_for_view(view);
    let verdict = palw_verify_measured_model_v1(&doc, measure_inputs(view, &ruleset_name, &fingerprint, &budgets, held), &court_for);
    println!("# Measured Model Artifact — verification on {ruleset_name}\n");
    println!("- model **{}**, id `{}`\n", doc.manifest.name, palw_measured_model_id_v1(&doc));
    println!("| field | verdict |");
    println!("|---|---|");
    for check in &verdict.checks {
        match check {
            PalwMeasuredCheckV1::Recomputed { field } => println!("| {field} | recomputed, equal |"),
            PalwMeasuredCheckV1::Mismatch { field, expected, got } => {
                println!("| {field} | **MISMATCH** — recomputed `{expected}`, the document says `{got}` |")
            }
            PalwMeasuredCheckV1::SelfReported { field, verified_by } => {
                println!("| {field} | self-reported; verified by {verified_by} |")
            }
            PalwMeasuredCheckV1::NeedsTheArtifact { field } => println!(
                "| {field} | needs the artifact: measured from its inventory, recomputable only by a holder (give --artifact) |"
            ),
        }
    }
    let signature = if doc.signature_hex.is_empty() {
        "unsigned".to_string()
    } else {
        let pk = decode_hex(&doc.adder_pubkey_hex)?;
        let sig = decode_hex(&doc.signature_hex)?;
        let id = palw_measured_model_id_v1(&doc);
        match kaspa_txscript::verify_mldsa87_with_context(&pk, id.as_byte_slice(), &sig, PALW_MEASURED_MODEL_MLDSA87_CONTEXT) {
            Ok(true) => format!("verifies under `{}…`", &doc.adder_pubkey_hex[..16]),
            Ok(false) | Err(_) => "**DOES NOT VERIFY**".to_string(),
        }
    };
    println!();
    println!("- signature: {signature}");
    let needs = verdict.needs_the_artifact();
    if !needs.is_empty() {
        println!("- {} field(s) need the artifact and were not compared: {}", needs.len(), needs.join(", "));
    }
    println!("- deterministic half: **{}**", if verdict.deterministic_ok() { "agrees" } else { "REFUSED" });
    if !verdict.deterministic_ok() || signature.contains("DOES NOT VERIFY") {
        std::process::exit(1);
    }
    Ok(())
}

fn decode_hex(h: &str) -> Result<Vec<u8>, String> {
    if !h.len().is_multiple_of(2) || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("not hex".into());
    }
    (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).map_err(|e| e.to_string())).collect()
}
