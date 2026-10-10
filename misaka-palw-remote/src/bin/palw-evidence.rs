//! `palw-evidence` — the independent DA transport for a PALW claim's evidence (RFC-0009 stage B, the network half). Std only; no node, no key.
//!
//! ```text
//!   palw-evidence serve   --root <dir> --listen 0.0.0.0:8088            a reference provider: verifies what it is given, serves the layout over HTTP
//!   palw-evidence publish --claim <hex> --from <rail --evidence-out dir> --providers <list> [--min-verified N]
//!   palw-evidence status  --claim <hex> --roots <claim.json> --providers <list> [--now-daa N] [--min-copies K]
//!   palw-evidence fetch   --claim <hex> --roots <claim.json> --providers <list> --out <file>
//!   palw-evidence repair  --claim <hex> --roots <claim.json> --providers <list>
//!   palw-evidence redemption-publish --file <RDA4 bundle> --providers <list> [--min-verified N]       (the miner, once, before switching off)
//!   palw-evidence redemption-sync    --providers <list> --into <dir> [--network-domain <hex>] [--now-daa N]   (a builder: fills --palw-redemption-auth-dir)
//!
//!   (DA16, the public artifact — every leaf checked against the CHAIN's roots: the class id and artifact root you pass. NON-CONSENSUS,
//!   optional off-chain tooling since ADR-0177: no court or lease backs it, and the chain never reads its result)
//!   palw-evidence artifact-fetch  --network-domain <hex> --class <hex> --artifact-root <hex> [--kernel-root <hex>] --providers <list> --into <dir>
//!   palw-evidence artifact-verify --network-domain <hex> --class <hex> --artifact-root <hex> --kernel-root <hex> --providers <list>
//!   palw-evidence artifact-status --network-domain <hex> --class <hex> --artifact-root <hex> --providers <list>
//!   palw-evidence artifact-repair --network-domain <hex> --class <hex> --artifact-root <hex> --providers <list>
//!   palw-evidence artifact-hook   --network-domain <hex> --providers <list> <class> <root> [btv2_infohash=…] [bundle_commitment=…] drop_dir=<dir>
//!                                 (kaspad --palw-root-fetch-cmd "palw-evidence artifact-hook --network-domain … --providers …")
//! ```
//!
//! `artifact-verify` prints CONFIRMED when the bytes root to both the class's artifact root and the binding's kernel root, and
//! KERNEL_ROOT_DIFFERS with the differing instances otherwise (then force the bound side's units out of the pair's provider with a
//! provider-court challenge, tag 151, and refute with tag 105).
//!
//! `--providers` is a file (one provider per line) or a comma-separated list: a directory, `dir:<path>`, `http://host:port[/prefix]`, `https://…`
//! (through `curl`). There is no on-chain provider discovery: you choose where to place evidence and where to look.
//!
//! `--roots` is the CLAIM's commitments as the chain holds them (`getPalwFreePromptClaim`): `{"network_domain","trace_root","output_root",
//! "execution_root","trace_chunk_count","retention_deadline"}`, hashes 128-hex. A manifest that disagrees with them is another execution's evidence
//! and is never used, whoever serves it.
//!
//! **What a "healthy" line means.** It is `LOCAL_OBSERVATION`: this machine fetched bytes and they verified. It is not a proof for anyone else, a
//! storage receipt is a promise, an infohash or an HTTP 200 is not availability, and nothing here moves a slash. Run `status`/`repair` from cron
//! (with `--now-daa` from a node) to keep a claim's evidence alive after the miner's PC is off.

use std::path::{Path, PathBuf};

use kaspa_hashes::Hash64;
use misaka_palw_remote::evidence::{ClaimRoots, EvidenceManifestV1, ManifestLimits, manifest_id_v1};
use misaka_palw_remote::transport::{
    EvidenceProvider, ProbeMode, ProviderSpecV1, check_availability_v1, fetch_claim_material_any, open_providers_v1,
    parse_provider_list_v1, publish_redemption_v1, publish_to_providers_v1, repair_v1, server, sync_redemptions_v1,
};

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("[palw-evidence] fatal: {msg}");
    std::process::exit(1);
}

fn hash(what: &str, v: &serde_json::Value) -> Hash64 {
    v.as_str().and_then(|s| s.parse::<Hash64>().ok()).unwrap_or_else(|| die(format!("{what} is not a 128-hex hash")))
}

fn read_roots(path: &Path) -> ClaimRoots {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| die(format!("{}: {e}", path.display())));
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| die(format!("{}: {e}", path.display())));
    ClaimRoots {
        network_domain: hash("network_domain", &v["network_domain"]),
        trace_root: hash("trace_root", &v["trace_root"]),
        output_root: hash("output_root", &v["output_root"]),
        execution_root: hash("execution_root", &v["execution_root"]),
        trace_chunk_count: v["trace_chunk_count"].as_u64().unwrap_or_else(|| die("trace_chunk_count is not a number")) as u32,
        retention_deadline: v["retention_deadline"].as_u64().unwrap_or_else(|| die("retention_deadline is not a number")),
    }
}

fn providers_of(arg: &str) -> Vec<ProviderSpecV1> {
    let text = if Path::new(arg).is_file() {
        std::fs::read_to_string(arg).unwrap_or_else(|e| die(format!("{arg}: {e}")))
    } else {
        arg.to_string()
    };
    let specs = parse_provider_list_v1(&text).unwrap_or_else(|e| die(e));
    if specs.is_empty() {
        die("--providers names no provider");
    }
    specs
}

/// The manifest for a claim: the first provider whose manifest is admissible against the CLAIM's roots.
fn admissible_manifest(providers: &[&dyn EvidenceProvider], claim_hex: &str, roots: &ClaimRoots) -> EvidenceManifestV1 {
    let limits = ManifestLimits::default();
    let mut why = Vec::new();
    for p in providers {
        match p.manifest_for(claim_hex) {
            Ok(Some(m)) => match m.validate_shape(&limits).and_then(|()| m.verify_claim_binding(roots)) {
                Ok(()) => return m,
                Err(e) => why.push(format!("{}: {e}", p.provider_id())),
            },
            Ok(None) => {}
            Err(e) => why.push(format!("{}: {e}", p.provider_id())),
        }
    }
    die(format!("no provider holds a manifest for this claim that agrees with the claim's roots ({why:?})"))
}

fn material_refs(
    opened: &[Box<dyn misaka_palw_remote::public_material::MaterialProvider>],
) -> Vec<&dyn misaka_palw_remote::public_material::MaterialProvider> {
    opened.iter().map(|b| b.as_ref()).collect()
}

/// `artifact-*`: the public artifact (DA16).
#[allow(clippy::too_many_arguments)]
fn artifact_command(
    command: &str,
    network_domain: Hash64,
    class: Hash64,
    artifact_root: Hash64,
    kernel_root: Option<Hash64>,
    providers: &str,
    into: Option<PathBuf>,
) {
    use misaka_palw_remote::public_material::*;
    let opened = open_material_providers_v1(&providers_of(providers));
    let refs = material_refs(&opened);
    let seed = class;
    match command {
        "artifact-fetch" | "artifact-verify" | "artifact-hook" => {
            let fetched = fetch_artifact_v1(&refs, network_domain, class, artifact_root, kernel_root, seed).unwrap_or_else(|e| die(e));
            let mut out = serde_json::json!({
                "schema": "misaka.palw.artifact-fetch.v1",
                "class": class.to_string(),
                "artifact_root": artifact_root.to_string(),
                "leaves": fetched.leaves.len(),
                "served_by": fetched.report.served_by.iter().collect::<std::collections::BTreeSet<_>>(),
                "note": "every leaf checked against the class's artifact root at the coordinates its program fixes; LOCAL_OBSERVATION",
            });
            if let Some(kr) = kernel_root {
                out["binding"] = match check_binding_v1(&fetched, kr).unwrap_or_else(|e| die(e)) {
                    kaspa_consensus_core::palw_public_material_v1::BindingCheckV1::Confirmed => {
                        serde_json::json!({ "verdict": "CONFIRMED", "kernel_root": kr.to_string() })
                    }
                    kaspa_consensus_core::palw_public_material_v1::BindingCheckV1::KernelRootDiffers { true_commitments } => {
                        serde_json::json!({
                            "verdict": "KERNEL_ROOT_DIFFERS",
                            "kernel_root": kr.to_string(),
                            "true_kernel_root": Hash64::from_bytes(true_commitments.root()).to_string(),
                            "next": "fetch or force (tag 151) the bound commitments and a differing row from the pair's provider, then file tag 105",
                        })
                    }
                };
            }
            if let Some(dir) = into {
                std::fs::create_dir_all(&dir).unwrap_or_else(|e| die(format!("{}: {e}", dir.display())));
                let path = dir.join(format!("{artifact_root}.palwtir"));
                write_container_v1(&path, &fetched).unwrap_or_else(|e| die(e));
                out["container"] = serde_json::json!(path.display().to_string());
            }
            println!("{out}");
        }
        "artifact-status" => {
            let (manifest, program) =
                fetch_artifact_manifest_v1(&refs, network_domain, class, artifact_root, kernel_root).unwrap_or_else(|e| die(e));
            let av = artifact_availability_v1(&refs, &manifest, &program);
            println!(
                "{}",
                serde_json::json!({
                    "schema": "misaka.palw.artifact-status.v1",
                    "leaves": manifest.leaf_hashes.len(),
                    "retain_until_daa": manifest.retain_until_daa,
                    "per_provider": av.iter().map(|a| serde_json::json!({ "provider": a.provider, "manifest": a.manifest_ok, "verified": a.verified, "missing": a.missing.len(), "corrupt": a.corrupt.len(), "unreachable": a.unreachable })).collect::<Vec<_>>(),
                    "observation": misaka_palw_remote::transport::LOCAL_OBSERVATION,
                })
            );
        }
        "artifact-repair" => {
            let (manifest, _) =
                fetch_artifact_manifest_v1(&refs, network_domain, class, artifact_root, kernel_root).unwrap_or_else(|e| die(e));
            let copies = repair_artifact_v1(&refs, &manifest, seed).unwrap_or_else(|e| die(e));
            println!("{}", serde_json::json!({ "schema": "misaka.palw.artifact-repair.v1", "complete_copies": copies }));
        }
        other => die(format!("unknown command {other:?}")),
    }
}

fn hex_arg(what: &str, v: &str) -> Hash64 {
    v.parse::<Hash64>().unwrap_or_else(|_| die(format!("{what} is not a 128-hex hash")))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| die("usage: palw-evidence <serve|publish|status|fetch|repair|artifact-…> …"));
    // `artifact-hook` is called by kaspad's root-fetch hook: flags, then `<class> <root> [key=value…] drop_dir=<dir>`.
    if command == "artifact-hook" {
        let (mut network_domain, mut providers, mut positional, mut drop_dir) = (None, None, Vec::new(), None);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--network-domain" => network_domain = args.next(),
                "--providers" => providers = args.next(),
                kv if kv.starts_with("drop_dir=") => drop_dir = Some(PathBuf::from(&kv["drop_dir=".len()..])),
                kv if kv.contains('=') => {}
                other => positional.push(other.to_string()),
            }
        }
        let [class, root] = positional.as_slice() else { die("artifact-hook needs <class> <root>") };
        artifact_command(
            "artifact-hook",
            hex_arg("--network-domain", &network_domain.unwrap_or_else(|| die("--network-domain is required"))),
            hex_arg("class", class),
            hex_arg("root", root),
            None,
            &providers.unwrap_or_else(|| die("--providers is required")),
            Some(drop_dir.unwrap_or_else(|| die("drop_dir= is required"))),
        );
        return;
    }
    let (mut claim, mut roots, mut providers, mut out, mut from, mut root, mut listen) = (None, None, None, None, None, None, None);
    let (mut min_verified, mut min_copies, mut now_daa, mut max_store_gb) = (1usize, 2usize, None::<u64>, 64u64);
    let (mut file, mut into, mut network_domain) = (None::<PathBuf>, None::<PathBuf>, None::<String>);
    let (mut class, mut artifact_root, mut kernel_root) = (None::<String>, None::<String>, None::<String>);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().unwrap_or_else(|| die(format!("{name} needs a value")));
        match flag.as_str() {
            "--claim" => claim = Some(value("--claim").to_ascii_lowercase()),
            "--roots" => roots = Some(PathBuf::from(value("--roots"))),
            "--providers" => providers = Some(value("--providers")),
            "--out" => out = Some(PathBuf::from(value("--out"))),
            "--from" => from = Some(PathBuf::from(value("--from"))),
            "--root" => root = Some(PathBuf::from(value("--root"))),
            "--listen" => listen = Some(value("--listen")),
            "--min-verified" => {
                min_verified = value("--min-verified").parse().unwrap_or_else(|_| die("--min-verified is not a number"))
            }
            "--min-copies" => min_copies = value("--min-copies").parse().unwrap_or_else(|_| die("--min-copies is not a number")),
            "--now-daa" => now_daa = Some(value("--now-daa").parse().unwrap_or_else(|_| die("--now-daa is not a number"))),
            "--file" => file = Some(PathBuf::from(value("--file"))),
            "--into" => into = Some(PathBuf::from(value("--into"))),
            "--network-domain" => network_domain = Some(value("--network-domain")),
            "--class" => class = Some(value("--class")),
            "--artifact-root" => artifact_root = Some(value("--artifact-root")),
            "--kernel-root" => kernel_root = Some(value("--kernel-root")),
            "--max-store-gb" => {
                max_store_gb = value("--max-store-gb").parse().unwrap_or_else(|_| die("--max-store-gb is not a number"))
            }
            other => die(format!("unknown flag {other}")),
        }
    }
    let need = |v: &Option<String>, name: &str| v.clone().unwrap_or_else(|| die(format!("{name} is required")));
    if command.starts_with("artifact-") {
        if command == "artifact-verify" && kernel_root.is_none() {
            die("artifact-verify needs --kernel-root (the binding's)");
        }
        artifact_command(
            &command,
            hex_arg("--network-domain", &need(&network_domain, "--network-domain")),
            hex_arg("--class", &need(&class, "--class")),
            hex_arg("--artifact-root", &need(&artifact_root, "--artifact-root")),
            kernel_root.as_deref().map(|k| hex_arg("--kernel-root", k)),
            &need(&providers, "--providers"),
            into,
        );
        return;
    }
    match command.as_str() {
        "serve" => {
            let root = root.unwrap_or_else(|| die("--root is required"));
            std::fs::create_dir_all(&root).unwrap_or_else(|e| die(format!("{}: {e}", root.display())));
            let listen = listen.unwrap_or_else(|| "127.0.0.1:8088".to_string());
            let cfg = server::ServerConfig { max_store_bytes: max_store_gb << 30, ..Default::default() };
            let handle = server::start(&listen, root.clone(), cfg).unwrap_or_else(|e| die(format!("cannot listen on {listen}: {e}")));
            println!(
                "{}",
                serde_json::json!({ "event": "serving", "url": handle.url(), "root": root.display().to_string(), "note": "verifies manifests against their chunks; a storage promise is not availability" })
            );
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        }
        "publish" => {
            let claim = need(&claim, "--claim");
            let from = from.unwrap_or_else(|| die("--from <the rail's --evidence-out directory> is required"));
            let local = misaka_palw_remote::evidence::fs::FsProvider::new(from.clone());
            let manifest =
                local.manifest_for(&claim).unwrap_or_else(|| die(format!("{} holds no manifest for claim {claim}", from.display())));
            let id = manifest_id_v1(&manifest);
            let mut chunks = Vec::new();
            for entry in &manifest.chunks {
                let bytes = std::fs::read(misaka_palw_remote::evidence::fs::chunk_path(&from, id, entry.index))
                    .unwrap_or_else(|e| die(format!("chunk {}: {e}", entry.index)));
                manifest
                    .verify_chunk(entry.index, &bytes)
                    .unwrap_or_else(|e| die(format!("the local chunk {} does not match its manifest: {e}", entry.index)));
                chunks.push(bytes);
            }
            let specs = providers_of(&need(&providers, "--providers"));
            let opened = open_providers_v1(&specs);
            let refs: Vec<&dyn EvidenceProvider> = opened.iter().map(|b| b.as_ref()).collect();
            let (ok, report) = match publish_to_providers_v1(&claim, &manifest, &chunks, &refs, min_verified) {
                Ok(r) => (true, r),
                Err(r) => (false, r),
            };
            println!(
                "{}",
                serde_json::json!({
                    "schema": "misaka.palw.evidence-publish.v1",
                    "claim": claim,
                    "manifest_id": report.manifest_id.to_string(),
                    "verified_copies": report.verified_copies(),
                    "required": min_verified,
                    "per_provider": report.per_provider.iter().map(|o| serde_json::json!({ "provider": o.provider, "result": o.result.clone().map(|()| "read back and verified".to_string()).unwrap_or_else(|e| e) })).collect::<Vec<_>>(),
                    "safe_to_switch_off": ok,
                    "note": "an ACK is the provider's word; each copy above was read back and hashed against the manifest by this machine. nobody but you is slashed for withholding until a provider court is armed",
                })
            );
            if !ok {
                std::process::exit(2);
            }
        }
        "redemption-publish" => {
            let file = file.unwrap_or_else(|| die("--file <the RDA4 bundle> is required"));
            let bytes = std::fs::read(&file).unwrap_or_else(|e| die(format!("{}: {e}", file.display())));
            let specs = providers_of(&need(&providers, "--providers"));
            let opened = open_providers_v1(&specs);
            let refs: Vec<&dyn EvidenceProvider> = opened.iter().map(|b| b.as_ref()).collect();
            let (ok, outcomes) = match publish_redemption_v1(&bytes, &refs, min_verified) {
                Ok(o) => (true, o),
                Err(o) => (false, o),
            };
            println!(
                "{}",
                serde_json::json!({
                    "schema": "misaka.palw.redemption-publish.v1",
                    "filed_with": outcomes.iter().filter(|o| o.result.is_ok()).count(),
                    "required": min_verified,
                    "per_provider": outcomes.iter().map(|o| serde_json::json!({ "provider": o.provider, "result": o.result.clone().map(|()| "read back".to_string()).unwrap_or_else(|e| e) })).collect::<Vec<_>>(),
                    "note": "a bundle on a provider is a hint to a builder: the chain's admission decides whether it redeems anything",
                })
            );
            if !ok {
                std::process::exit(2);
            }
        }
        "redemption-sync" => {
            let into = into.unwrap_or_else(|| die("--into <the node's --palw-redemption-auth-dir> is required"));
            let specs = providers_of(&need(&providers, "--providers"));
            let opened = open_providers_v1(&specs);
            let refs: Vec<&dyn EvidenceProvider> = opened.iter().map(|b| b.as_ref()).collect();
            let domain = network_domain.map(|d| hash("--network-domain", &serde_json::Value::String(d)));
            let report =
                sync_redemptions_v1(&refs, &into, domain, now_daa).unwrap_or_else(|e| die(format!("{}: {e}", into.display())));
            println!(
                "{}",
                serde_json::json!({
                    "schema": "misaka.palw.redemption-sync.v1",
                    "into": into.display().to_string(),
                    "written": report.written.iter().map(|(c, f)| serde_json::json!({ "claim": c, "file": f })).collect::<Vec<_>>(),
                    "already_present": report.already_present,
                    "skipped": report.skipped.iter().map(|(p, c, why)| serde_json::json!({ "provider": p, "claim": c, "why": why })).collect::<Vec<_>>(),
                })
            );
        }
        "status" | "fetch" | "repair" => {
            let claim = need(&claim, "--claim");
            let roots =
                read_roots(&roots.unwrap_or_else(|| die("--roots <claim.json> is required: the claim's commitments from the chain")));
            let specs = providers_of(&need(&providers, "--providers"));
            let opened = open_providers_v1(&specs);
            let refs: Vec<&dyn EvidenceProvider> = opened.iter().map(|b| b.as_ref()).collect();
            if command == "fetch" {
                let out = out.unwrap_or_else(|| die("--out is required"));
                match fetch_claim_material_any(&refs, &claim, &roots, &ManifestLimits::default(), claim.parse().unwrap_or_default()) {
                    Ok((bytes, report)) => {
                        std::fs::write(&out, &bytes).unwrap_or_else(|e| die(format!("{}: {e}", out.display())));
                        println!(
                            "{}",
                            serde_json::json!({ "schema": "misaka.palw.evidence-fetch.v1", "claim": claim, "bytes": bytes.len(), "out": out.display().to_string(), "served_by": report.served_by, "failed_attempts": report.failures.len(), "note": "every chunk verified against a manifest that agrees with the claim's roots; the Panel's own re-execution still judges the material" })
                        );
                    }
                    Err(e) => die(e),
                }
                return;
            }
            let manifest = admissible_manifest(&refs, &claim, &roots);
            if command == "repair" {
                let r = repair_v1(&claim, &manifest, &refs);
                println!(
                    "{}",
                    serde_json::json!({ "schema": "misaka.palw.evidence-repair.v1", "claim": claim, "copied": r.copied.len(), "failed": r.failed, "unrepairable_chunks": r.unrepairable })
                );
                return;
            }
            let a = check_availability_v1(&claim, &manifest, &refs, ProbeMode::Verify);
            println!(
                "{}",
                serde_json::json!({
                    "schema": "misaka.palw.evidence-status.v1",
                    "claim": claim,
                    "manifest_id": a.manifest_id.to_string(),
                    "retention_until_daa": a.retention_until_daa,
                    "copies_per_chunk": a.copies,
                    "verdict": format!("{:?}", a.verdict(min_copies, now_daa)),
                    "per_provider": a.per_provider.iter().map(|p| serde_json::json!({ "provider": p.provider, "manifest": format!("{:?}", p.manifest), "verified_chunks": p.verified_chunks(), "chunks": p.chunks.len() })).collect::<Vec<_>>(),
                    "observation": a.line(min_copies, now_daa),
                })
            );
        }
        other => die(format!("unknown command {other:?}")),
    }
}
