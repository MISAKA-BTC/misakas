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
use kaspa_hashes::Hash64;
use misaka_palw_sdk::PalwClassSdk;

const USAGE: &str = "palw-class — inspect and preflight PALW model classes through the SDK

USAGE:
    palw-class ledger    --network <id>
    palw-class inspect   --network <id> <artifact-path>
    palw-class preflight --network <id> <artifact-path> [--model-id <model-id>]
    palw-class bind-tokenizer --network <id> --tokenizer <tokenizer.json> --out <path> [--model-id <model-id>] <artifact-path>

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
