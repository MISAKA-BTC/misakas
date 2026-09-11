//! **`misaka palw extension`** (ADR-0108 §4): one manifest in, one tier out.
//!
//! ```text
//! inspect        <manifest>                 what it is, its id, its tier by structure alone
//! verify         <manifest> [--depth …]     recompute; write a receipt with --receipt-out, signed with --key-file
//! preflight      <manifest>                 verify at Full, plus the chain's shape at the node's DAA
//! submit         <manifest> --key-file …    build the EXISTING object and file it (dry-run unless --yes)
//! receipt-verify <receipt> [--manifest …]   the signature, the id, the ruleset it was made on
//! ```
//!
//! Exit codes distinguish the tiers so a script can branch (`crate::exit`): expressible and would
//! be admitted at the depth asked for, `0`; refused — the manifest is wrong about itself —
//! `EXTENSION_REFUSED`; a node extension — unverifiable here — `EXTENSION_NODE_EXTENSION`; a ruleset
//! change `EXTENSION_RULESET_CHANGE`; a depth not reached because this machine lacks the bytes
//! `EXTENSION_DEPTH_NOT_REACHED`. Every non-zero exit prints the field or the missing thing by name.
//!
//! Nothing here follows a reference (SA-2): every byte read is the manifest, a path beside it, or
//! the node the operator named. `submit` signs with the operator's key and never one the manifest
//! names (SA-6) — a manifest has no field for one.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kaspa_consensus_core::network::NetworkId;
use kaspa_rpc_core::api::rpc::RpcApi;
use misaka_palw_extension::{
    PalwExtensionClassificationV1, PalwExtensionDepthV1, PalwExtensionEnvV1, PalwExtensionKindV1, PalwExtensionOutcomeV1,
    PalwExtensionReceiptV1, PalwExtensionReportV1, PalwParsedManifestV1, class_registration_inputs_v1, genesis_registration_terms_v1,
    params_for, sign_receipt_v1, verify_parsed_v1, verify_receipt_v1,
};

use crate::keys::KeySource;
use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};

/// A manifest read from disk: the file, the directory its paths resolve against, and the parsed
/// document with its id.
struct ManifestFile {
    path: PathBuf,
    dir: PathBuf,
    parsed: PalwParsedManifestV1,
}

fn read_manifest(path: &Path) -> Result<ManifestFile, CliError> {
    let bytes = std::fs::read(path).map_err(|e| CliError::generic(format!("{}: {e}", path.display())))?;
    let parsed =
        PalwParsedManifestV1::parse(&bytes).map_err(|e| CliError::new(exit::EXTENSION_REFUSED, format!("{}: {e}", path.display())))?;
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Ok(ManifestFile { path: path.to_path_buf(), dir, parsed })
}

fn network_of(mf: &ManifestFile) -> Result<NetworkId, CliError> {
    mf.parsed.manifest.network.parse::<NetworkId>().map_err(|e| {
        CliError::new(exit::EXTENSION_REFUSED, format!("network: `{}` is not a network id: {e}", mf.parsed.manifest.network))
    })
}

fn json_mode(ctx: &Ctx, json: bool) -> bool {
    json || ctx.output == OutputFormat::Json
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// `TransformerManifest`-style `&'static str` fields on the SDK's class entry: a registration is
/// built once per invocation, so leaking two short strings is the honest cost of not restating
/// the entry type over `String`s.
fn leak(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

fn print_report(ctx: &Ctx, json: bool, report: &PalwExtensionReportV1, extra: serde_json::Value) {
    if json_mode(ctx, json) {
        let mut value = serde_json::to_value(report).unwrap_or(serde_json::Value::Null);
        if let (Some(object), Some(more)) = (value.as_object_mut(), extra.as_object()) {
            for (k, v) in more {
                object.insert(k.clone(), v.clone());
            }
        }
        println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
        return;
    }
    println!("{} manifest  extension id {}", report.kind, report.extension_id);
    println!("  network    {}  (shape at daa {})", report.network, report.daa_score);
    println!("  tier       {}", report.classification.summary());
    match &report.classification {
        PalwExtensionClassificationV1::Expressible { serving: Some(serving), .. } => {
            println!(
                "  serving    in this build's table: {}; needs the chain-class arm to be served: {} (ADR-0067 D5)",
                serving.in_build_table, serving.needs_chain_classes_arm
            );
        }
        PalwExtensionClassificationV1::RulesetChange { fences, would_print, .. } => {
            for fence in fences {
                println!("  fence      {}: requested {}, this build {}", fence.name, fence.requested, fence.this_build);
            }
            if let Some(wp) = would_print {
                println!(
                    "  arming build would print  params {}  identity {}  schedule {}",
                    wp.params_id, wp.identity_id, wp.schedule_id
                );
                println!("  this build prints         params {}", report.ruleset_id_this_build);
                println!("  fence schedule            {:?}", wp.fence_schedule);
            }
        }
        _ => {}
    }
    match &report.stopped_at {
        Some(why) => {
            println!("  depth      reached {} of {} requested — stopped: {why}", report.depth_reached, report.depth_requested)
        }
        None => println!("  depth      reached {} of {} requested", report.depth_reached, report.depth_requested),
    }
    println!("  checks     {} pass, {} fail, {} skipped", report.passed(), report.failed(), report.skipped());
    for check in &report.checks {
        match &check.outcome {
            PalwExtensionOutcomeV1::Pass => println!("    [pass] {}", check.name),
            PalwExtensionOutcomeV1::Fail(why) => println!("    [FAIL] {} — {why}", check.name),
            PalwExtensionOutcomeV1::Skipped(why) => println!("    [skip] {} — {why}", check.name),
        }
    }
    if !report.recomputed.is_empty() {
        println!("  recomputed");
        for (k, v) in &report.recomputed {
            println!("    {k}: {v}");
        }
    }
    println!(
        "  ruleset    this build {}; manifest {}",
        report.ruleset_id_this_build,
        report.ruleset_id_manifest.as_deref().unwrap_or("not given")
    );
    println!("  terms      {}", report.chain_terms);
    if let Some(extra) = extra.as_object() {
        for (k, v) in extra {
            match v {
                serde_json::Value::String(s) => println!("  {k:<10} {s}"),
                other => println!("  {k:<10} {other}"),
            }
        }
    }
}

/// The exit a tier earns (ADR-0108 §4). `Ok` only for expressible-and-would-be-admitted at the
/// depth asked for.
fn exit_for(report: &PalwExtensionReportV1) -> CliResult {
    match &report.classification {
        PalwExtensionClassificationV1::Refused { field, reason } => {
            Err(CliError::new(exit::EXTENSION_REFUSED, format!("refused — {field}: {reason}")))
        }
        PalwExtensionClassificationV1::NodeExtension { missing } => Err(CliError::new(
            exit::EXTENSION_NODE_EXTENSION,
            format!("node extension — unverifiable here; this build lacks: {}", missing.join("; ")),
        )),
        PalwExtensionClassificationV1::RulesetChange { reason, flag_day, .. } => Err(CliError::new(
            exit::EXTENSION_RULESET_CHANGE,
            format!("ruleset change{} — {reason}", if *flag_day { " (flag day)" } else { "" }),
        )),
        PalwExtensionClassificationV1::Expressible { would_be_refused: Some(_), .. } => {
            Err(CliError::new(exit::EXTENSION_REFUSED, report.classification.summary()))
        }
        PalwExtensionClassificationV1::Expressible { .. } if report.depth_not_reached() => Err(CliError::new(
            exit::EXTENSION_DEPTH_NOT_REACHED,
            format!(
                "depth {} not reached (got to {}): {}",
                report.depth_requested,
                report.depth_reached,
                report.stopped_at.as_deref().unwrap_or("this machine lacks the bytes")
            ),
        )),
        PalwExtensionClassificationV1::Expressible { .. } => Ok(()),
    }
}

/// `inspect`: what it is, its id, its tier by structure alone — offline, at the genesis shape.
pub fn inspect(ctx: &Ctx, manifest: &Path, json: bool) -> CliResult {
    let mf = read_manifest(manifest)?;
    let net = network_of(&mf)?;
    let env = PalwExtensionEnvV1::genesis(net);
    let report = verify_parsed_v1(&mf.parsed, &mf.dir, &env, PalwExtensionDepthV1::Structural)
        .map_err(|e| CliError::new(exit::EXTENSION_REFUSED, format!("{}: {e}", mf.path.display())))?;
    let m = &mf.parsed.manifest;
    if json_mode(ctx, json) {
        let value = serde_json::json!({
            "manifest": mf.path.display().to_string(),
            "extension_id": mf.parsed.extension_id.to_string(),
            "kind": m.kind,
            "name": m.name,
            "network": m.network,
            "canonical_bytes": mf.parsed.canonical_bytes.len(),
            "admission_object": m.kind.admission_object(),
            "source": m.source,
            "declares": m.declares,
            "requires": m.requires,
            "structural": report,
        });
        println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
    } else {
        println!("manifest      {}", mf.path.display());
        println!("extension id  {}", mf.parsed.extension_id);
        println!("kind          {}  (rides as {})", m.kind, m.kind.admission_object());
        println!("name          {}", m.name);
        println!("network       {}", m.network);
        println!("canonical     {} bytes", mf.parsed.canonical_bytes.len());
        println!(
            "source        digest {}; note {}  (inert: carried for you, read by nothing — SA-7)",
            m.source.digest.as_deref().unwrap_or("-"),
            m.source.note.as_deref().unwrap_or("-")
        );
        println!("declares      object_id {}", m.declares.object_id);
        if !m.declares.capabilities.is_empty() {
            println!("              capabilities {}", m.declares.capabilities.join(", "));
        }
        println!(
            "requires      ruleset_id {}; {} fence(s); {} kernel id(s)",
            m.requires.ruleset_id.as_deref().unwrap_or("not given"),
            m.requires.fences.len(),
            m.requires.kernel_ids.len()
        );
        if let Some(artifact) = &m.artifact {
            println!("artifact      root {}; path {}", artifact.root, artifact.path.as_deref().unwrap_or("not given"));
        }
        println!("by structure  {}", report.classification.summary());
        for check in &report.checks {
            if let PalwExtensionOutcomeV1::Fail(why) = &check.outcome {
                println!("    [FAIL] {} — {why}", check.name);
            }
        }
    }
    match &report.classification {
        PalwExtensionClassificationV1::Expressible { .. } => Ok(()),
        _ => exit_for(&report),
    }
}

/// `verify`: recompute at the depth asked for; write a receipt, signed with the operator's key
/// when one is given.
pub fn verify(
    ctx: &Ctx,
    manifest: &Path,
    depth: PalwExtensionDepthV1,
    receipt_out: Option<&Path>,
    key: Option<KeySource>,
    json: bool,
) -> CliResult {
    let mf = read_manifest(manifest)?;
    let net = network_of(&mf)?;
    let env = PalwExtensionEnvV1::genesis(net);
    let report = verify_parsed_v1(&mf.parsed, &mf.dir, &env, depth)
        .map_err(|e| CliError::new(exit::EXTENSION_REFUSED, format!("{}: {e}", mf.path.display())))?;
    let mut extra = serde_json::Map::new();
    if let Some(out) = receipt_out {
        let mut receipt = PalwExtensionReceiptV1::issue(report.clone(), now_ms());
        let signer = match key {
            Some(source) => {
                let key = source.load_key()?;
                sign_receipt_v1(&mut receipt, &key).map_err(|e| CliError::generic(e.to_string()))?;
                format!("signed by {}…", &faster_hex::hex_string(key.public_key())[..16])
            }
            None => "unsigned (pass --key-file to sign)".to_string(),
        };
        let bytes = receipt.canonical_json().map_err(|e| CliError::generic(e.to_string()))?;
        std::fs::write(out, &bytes).map_err(|e| CliError::generic(format!("{}: {e}", out.display())))?;
        let id = receipt.receipt_id().map_err(|e| CliError::generic(e.to_string()))?;
        extra.insert("receipt".into(), serde_json::json!(format!("{} id {id} {signer}", out.display())));
    }
    print_report(ctx, json, &report, serde_json::Value::Object(extra));
    exit_for(&report)
}

/// The node's virtual DAA, through the read-only connection `palw derived` uses.
async fn node_daa(ctx: &Ctx, mf: &ManifestFile) -> Result<u64, CliError> {
    if mf.parsed.manifest.network != ctx.network {
        return Err(CliError::new(
            exit::NETWORK_MISMATCH,
            format!("the manifest is for {} and --network is {}", mf.parsed.manifest.network, ctx.network),
        ));
    }
    let reader = crate::palw_derived::connect(ctx).await?;
    let dag = reader.client.get_block_dag_info().await.map_err(|e| CliError::connection(format!("getBlockDagInfo: {e}")))?;
    Ok(dag.virtual_daa_score)
}

/// `preflight`: `verify` at Full, plus the shape the chain would judge at the node's DAA, the
/// object it would build, what it costs, and the refusal it would meet — before any fee.
pub async fn preflight(ctx: &Ctx, manifest: &Path, json: bool) -> CliResult {
    let mf = read_manifest(manifest)?;
    let net = network_of(&mf)?;
    let daa = node_daa(ctx, &mf).await?;
    let env = PalwExtensionEnvV1 { network_id: net, daa_score: Some(daa), chain_terms: None };
    let report = verify_parsed_v1(&mf.parsed, &mf.dir, &env, PalwExtensionDepthV1::Full)
        .map_err(|e| CliError::new(exit::EXTENSION_REFUSED, format!("{}: {e}", mf.path.display())))?;
    let mut extra = serde_json::Map::new();
    let kind = mf.parsed.manifest.kind;
    extra.insert("object".into(), serde_json::json!(kind.admission_object()));
    extra.insert(
        "shape".into(),
        serde_json::json!(format!(
            "at daa {daa}: court {}, ladder {}",
            report.recomputed.get("shape.court").map(String::as_str).unwrap_or("not asked"),
            report.recomputed.get("shape.ladder").map(String::as_str).unwrap_or("not asked")
        )),
    );
    let cost = match kind {
        PalwExtensionKindV1::ModelClass | PalwExtensionKindV1::ContextProfile => {
            let params = params_for(net).map_err(|e| CliError::generic(e.to_string()))?;
            match &params.palw_consensus_mode {
                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                    match genesis_registration_terms_v1(bundle) {
                        Some(terms) => {
                            let weightless =
                                matches!(report.classification, PalwExtensionClassificationV1::Expressible { weightless: true, .. });
                            format!(
                                "share {}‰ (min grantable {}‰{}), slash value {} per pwu, initial target {} — the genesis terms; a registration is signed by the bond key and carried at the carrier's fee",
                                if weightless { 0 } else { terms.min_grantable_share_permille },
                                terms.min_grantable_share_permille,
                                if weightless { ", weightless: no certified family covers the class" } else { "" },
                                terms.slash_value_per_pwu,
                                terms.initial_target
                            )
                        }
                        None => "the network's genesis registers no base class".to_string(),
                    }
                }
                _ => "no PALW V2 bundle".to_string(),
            }
        }
        PalwExtensionKindV1::FamilyCertification | PalwExtensionKindV1::LaneCertification => {
            "the carrier's fee is the rent (ADR-0075): sized from the object's compute mass — `submit` prints it in its dry run"
                .to_string()
        }
        PalwExtensionKindV1::DerivedTransformer => {
            "nothing rides for a transformer; each derivation over it rides per claim as a DerivedArtifactV1".to_string()
        }
        PalwExtensionKindV1::RulesetCandidate => "a fence is a release, not a fee".to_string(),
    };
    extra.insert("cost".into(), serde_json::json!(cost));
    let refusal = match &report.classification {
        PalwExtensionClassificationV1::Expressible { would_be_refused: Some(why), .. } => why.clone(),
        PalwExtensionClassificationV1::Expressible { .. } => "none — the object would be admitted at this shape".to_string(),
        other => other.summary(),
    };
    extra.insert("refusal".into(), serde_json::json!(refusal));
    print_report(ctx, json, &report, serde_json::Value::Object(extra));
    exit_for(&report)
}

/// `submit`: build the existing object and file it through `palw submit-object`'s path (dry-run
/// unless `--yes`). A `model-class` / `context-profile` becomes a `ClassRegistered` built by the SDK
/// and signed by the operator's key, which must be the bond's; a certification is its object file;
/// a transformer and a ruleset candidate have nothing to file, and say so.
pub async fn submit(ctx: &Ctx, manifest: &Path, ks: &KeySource, bond: Option<&str>, yes: bool, json: bool) -> CliResult {
    let mf = read_manifest(manifest)?;
    let net = network_of(&mf)?;
    let kind = mf.parsed.manifest.kind;
    match kind {
        PalwExtensionKindV1::DerivedTransformer => Err(CliError::generic(
            "admission.object: none — a transformer is not a chain object; each derivation over it rides per claim as a DerivedArtifactV1 (`misaka palw fp-submit`, ADR-0078)",
        )),
        PalwExtensionKindV1::RulesetCandidate => Err(CliError::new(
            exit::EXTENSION_RULESET_CHANGE,
            "a ruleset candidate is a release, not an object: nothing rides (ADR-0108 Decision 6) — schedule the height in the preset, rebuild, and give the notice the fork-id gate makes necessary (ADR-0105 §7)",
        )),
        PalwExtensionKindV1::FamilyCertification | PalwExtensionKindV1::LaneCertification => {
            let env = PalwExtensionEnvV1::genesis(net);
            let report = verify_parsed_v1(&mf.parsed, &mf.dir, &env, PalwExtensionDepthV1::Vectors)
                .map_err(|e| CliError::new(exit::EXTENSION_REFUSED, format!("{}: {e}", mf.path.display())))?;
            print_report(ctx, json, &report, serde_json::json!({}));
            exit_for(&report)?;
            let object = mf
                .parsed
                .manifest
                .verification
                .object_path
                .clone()
                .ok_or_else(|| CliError::generic("verification.object_path: missing"))?;
            let path = mf.dir.join(object);
            // ADR-0075 Decision 14: a drill above one carrier's bytes rides in chunks, written
            // beside the object under the names `palw-certify` uses and submitted in index order.
            let bytes = std::fs::read(&path).map_err(|e| CliError::generic(format!("{}: {e}", path.display())))?;
            let whole: kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 = borsh::from_slice(&bytes)
                .map_err(|e| CliError::generic(format!("{} is not a borsh consensus object: {e}", path.display())))?;
            let carriers = match kaspa_consensus_core::palw_state_v2::palw_object_chunks_v1(&whole) {
                Ok(None) => vec![path],
                Ok(Some(chunks)) => {
                    let mut names = Vec::with_capacity(chunks.len());
                    for chunk in &chunks {
                        let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectChunk { index, .. } = chunk else {
                            return Err(CliError::generic("the chunker produced something that is not a chunk"));
                        };
                        let name = PathBuf::from(format!("{}.chunk{index}", path.display()));
                        std::fs::write(
                            &name,
                            borsh::to_vec(chunk).map_err(|e| CliError::generic(format!("the chunk does not serialize: {e}")))?,
                        )
                        .map_err(|e| CliError::generic(format!("{}: {e}", name.display())))?;
                        names.push(name);
                    }
                    if !json_mode(ctx, json) {
                        println!(
                            "{} bytes is above one carrier's {}: cut into {} chunks beside the object (ADR-0075 Decision 14), submitted in index order",
                            bytes.len(),
                            kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES,
                            names.len()
                        );
                    }
                    names
                }
                Err(e) => return Err(CliError::generic(format!("{}: cannot be chunked: {e}", path.display()))),
            };
            crate::palw_fp::submit_objects(ctx, ks, &carriers, yes).await
        }
        PalwExtensionKindV1::ModelClass | PalwExtensionKindV1::ContextProfile => {
            let bond = bond.ok_or_else(|| {
                CliError::generic("--bond <txid:index> is required: a registration is signed by the registrant's bond key, and this key must be that bond's")
            })?;
            let bond = kaspa_pq_validator_core::parse_stake_bond_ref(bond).map_err(CliError::generic)?;
            let daa = node_daa(ctx, &mf).await?;
            let env = PalwExtensionEnvV1 { network_id: net, daa_score: Some(daa), chain_terms: None };
            let report = verify_parsed_v1(&mf.parsed, &mf.dir, &env, PalwExtensionDepthV1::Full)
                .map_err(|e| CliError::new(exit::EXTENSION_REFUSED, format!("{}: {e}", mf.path.display())))?;
            print_report(ctx, json, &report, serde_json::json!({}));
            exit_for(&report)?;
            let inputs = class_registration_inputs_v1(&mf.parsed, &mf.dir, &env)
                .map_err(|e| CliError::new(exit::EXTENSION_REFUSED, e.to_string()))?;
            let params = params_for(net).map_err(|e| CliError::generic(e.to_string()))?;
            let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
                return Err(CliError::generic(format!("{net} has no PALW V2 bundle")));
            };
            let terms = genesis_registration_terms_v1(bundle)
                .ok_or_else(|| CliError::generic("the network's genesis registers no base class"))?;
            let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(
                bundle.court,
                params.palw_prompt_ids_form_v1(),
                net.to_string().into_bytes(),
            );
            let shape =
                kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1(&params, bundle, &inputs.profile, daa)
                    .map_err(CliError::generic)?;
            let entry = misaka_palw_sdk::PalwClassEntryV1 {
                model_id: leak(&inputs.model_id),
                lineage_id: leak(&inputs.lineage_id),
                profile: inputs.profile.clone(),
                canonical_job: inputs.canonical_job,
                needs_artifact_file: inputs.needs_artifact_file,
            };
            let candidate = misaka_palw_sdk::PalwRegistrationCandidateV1 { entry, artifact_root: inputs.artifact_root };
            let bond_key = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(bond);
            // Built twice, as the panel builds it: once to learn the object, once with the
            // signature over it. Both runs pass the SDK's gate.
            let build = |signature: Vec<u8>| {
                sdk.build_post_genesis_registration(bundle, &candidate, &terms, 0, bond_key, signature, &shape)
                    .map_err(CliError::generic)
            };
            let unsigned = build(Vec::new())?;
            let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
                class_id,
                activation_daa,
                artifact_root,
                slash_value_per_pwu,
                initial_target,
                pwu_rule,
                share_permille,
                ..
            } = &unsigned
            else {
                return Err(CliError::generic("the SDK did not build a registration"));
            };
            let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                params.net.to_string().as_bytes(),
                Some(params.genesis.hash),
            );
            let canonical = candidate.entry.canonical_context();
            let message = kaspa_consensus_core::palw_state_v2::palw_class_registration_message_v2(
                network_domain,
                *class_id,
                *share_permille,
                *activation_daa,
                &bond_key,
                *artifact_root,
                *slash_value_per_pwu,
                *initial_target,
                pwu_rule,
                &canonical,
            );
            let key = ks.load_key()?;
            let signature = key.sign_with_context(
                message.as_byte_slice(),
                kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT,
            );
            let signed = build(signature.to_vec())?;
            let out = mf.path.with_extension("class-registered.borsh");
            std::fs::write(
                &out,
                borsh::to_vec(&signed).map_err(|e| CliError::generic(format!("the object does not serialize: {e}")))?,
            )
            .map_err(|e| CliError::generic(format!("{}: {e}", out.display())))?;
            if !json_mode(ctx, json) {
                println!(
                    "built ClassRegistered: class {class_id}, root {artifact_root}, share {share_permille}‰, activation daa {activation_daa}, signed by this key for bond {}:{} — written to {}",
                    bond.transaction_id,
                    bond.index,
                    out.display()
                );
                println!("  the signature verifies on chain only if this key is the bond's registered key (ADR-0049 Decision H)");
            }
            crate::palw_fp::submit_objects(ctx, ks, &[out], yes).await
        }
    }
}

/// `receipt-verify`: one receipt (SA-5) — its canonical form, its id, its signature, and the
/// ruleset it names against this build's ruleset for that network (SA-3). With `--manifest`, the
/// receipt must also be about that manifest.
pub fn receipt_verify(ctx: &Ctx, receipt: &Path, manifest: Option<&Path>, json: bool) -> CliResult {
    let bytes = std::fs::read(receipt).map_err(|e| CliError::generic(format!("{}: {e}", receipt.display())))?;
    // The network the receipt names decides which ruleset "this one" is; a manifest, when given,
    // names it too and must agree.
    let peek: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| CliError::new(exit::EXTENSION_REFUSED, format!("receipt: not JSON: {e}")))?;
    let named_network = peek
        .get("verifier")
        .and_then(|v| v.get("network"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| CliError::new(exit::EXTENSION_REFUSED, "verifier.network: missing"))?
        .to_string();
    let mf = manifest.map(read_manifest).transpose()?;
    if let Some(mf) = &mf
        && mf.parsed.manifest.network != named_network
    {
        return Err(CliError::new(
            exit::EXTENSION_REFUSED,
            format!(
                "verifier.network: the receipt was made on {named_network} and the manifest is for {}",
                mf.parsed.manifest.network
            ),
        ));
    }
    let net: NetworkId = named_network
        .parse()
        .map_err(|e| CliError::new(exit::EXTENSION_REFUSED, format!("verifier.network: `{named_network}`: {e}")))?;
    let expected = params_for(net).map_err(|e| CliError::generic(e.to_string()))?.consensus_params_id().to_string();
    let verdict = verify_receipt_v1(&bytes, Some(&expected))
        .map_err(|e| CliError::new(exit::EXTENSION_REFUSED, format!("{}: {e}", receipt.display())))?;
    if let Some(mf) = &mf
        && verdict.extension_id != mf.parsed.extension_id.to_string()
    {
        return Err(CliError::new(
            exit::EXTENSION_REFUSED,
            format!("extension_id: the receipt is about {} and the manifest's id is {}", verdict.extension_id, mf.parsed.extension_id),
        ));
    }
    if json_mode(ctx, json) {
        println!("{}", serde_json::to_string_pretty(&verdict).unwrap_or_default());
    } else {
        println!("receipt       {}", receipt.display());
        println!("receipt id    {}", verdict.receipt_id);
        println!("extension id  {}{}", verdict.extension_id, if mf.is_some() { "  (matches the manifest)" } else { "" });
        println!("ruleset       {} on {}  (this build's ruleset for that network)", verdict.ruleset_id, verdict.network);
        println!("signature     {}", verdict.note);
        println!(
            "report        tier {}, depth reached {}, issued at {} ms",
            verdict.tier, verdict.depth_reached, verdict.issued_at_unix_ms
        );
    }
    Ok(())
}
