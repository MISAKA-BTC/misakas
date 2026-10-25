//! `palw-fork-choice-issuer` — the issuer side of RFC-0009 L2 option D: attests the fork-choice roots the issuer's OWN full node computed.
//!
//! ```text
//!   palw-fork-choice-issuer --network testnet-12 --rpc <your own node>:17210 --key-file <seed> --key-id <label> --out <dir> \
//!     [--every-secs N] [--rounds N] [--keep N] [--palw-drill-genesis-salt <hex> [--palw-drill-ruleset <params id>:<schedule id>]]
//!   palw-fork-choice-issuer --key-file <seed> --print-public-key <file>
//! ```
//!
//! Each round asks the node for its sink's and tips' openings (op 203, `getPalwForkChoiceOpening`), keeps only post-states committed in
//! the fork-choice form whose served opening hashes to the served root and names the block (`misaka_palw_remote::l2::issue_attestations_v1`),
//! signs each with ML-DSA-87 under the attestation context, and writes `<out>/<block>.json` (atomically). A miner that chose this issuer
//! (`palw-remote-miner --fork-choice-issuer <label>=<public key file> --fork-choice-attestations <dir>`) reads that directory, synced by
//! any transport: the files are signed, the transport is untrusted.
//!
//! **What signing states.** "My full node computed this block's post-state, so its whole selected chain passed validation, and that
//! post-state commits this root." Run it ONLY against a node you operate, over a channel you trust: the issuer's re-execution is the
//! trust a `VERIFIED_REMOTE` miner rests on. The issuer refuses a node on another ruleset (network, genesis, params id, schedule id) and
//! signs nothing below `palw_fork_choice_commitment_v1` (dormant on every network today, so it signs nothing anywhere yet and says so).

use std::path::{Path, PathBuf};

use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwDnsGateFactV1;
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use misaka_palw_remote::l2::{FORK_CHOICE_ATTESTATION_MLDSA87_CONTEXT, IssuerServedV1, issue_attestations_v1, opening_from_wire_v1};
use misaka_palw_remote::verify::ClientRulesetV1;

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("[palw-fork-choice-issuer] fatal: {msg}");
    std::process::exit(1);
}

fn say(event: &str, fields: serde_json::Value) {
    println!("{}", serde_json::json!({ "event": event, "detail": fields }));
}

struct Args {
    network: Option<String>,
    rpc: Option<String>,
    key_file: String,
    key_id: Option<String>,
    out: Option<PathBuf>,
    every_secs: u64,
    rounds: Option<u64>,
    keep: usize,
    print_public_key: Option<PathBuf>,
    drill_salt: Option<kaspa_consensus_core::config::drill::PalwDrillSaltV1>,
    drill_ruleset: Option<(String, String)>,
}

fn parse_args() -> Args {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        network: None,
        rpc: None,
        key_file: String::new(),
        key_id: None,
        out: None,
        every_secs: 5,
        rounds: None,
        keep: 64,
        print_public_key: None,
        drill_salt: None,
        drill_ruleset: None,
    };
    let mut key_file = None;
    while let Some(flag) = it.next() {
        let mut value = |name: &str| it.next().unwrap_or_else(|| die(format!("{name} needs a value")));
        match flag.as_str() {
            "--network" => a.network = Some(value("--network")),
            "--rpc" => a.rpc = Some(value("--rpc")),
            "--key-file" => key_file = Some(value("--key-file")),
            "--key-id" => a.key_id = Some(value("--key-id")),
            "--out" => a.out = Some(PathBuf::from(value("--out"))),
            "--every-secs" => a.every_secs = value("--every-secs").parse().unwrap_or_else(|_| die("--every-secs is not a number")),
            "--rounds" => a.rounds = Some(value("--rounds").parse().unwrap_or_else(|_| die("--rounds is not a number"))),
            "--keep" => a.keep = value("--keep").parse().unwrap_or_else(|_| die("--keep is not a number")),
            "--print-public-key" => a.print_public_key = Some(PathBuf::from(value("--print-public-key"))),
            "--palw-drill-genesis-salt" => {
                let v = value("--palw-drill-genesis-salt");
                a.drill_salt = Some(
                    kaspa_consensus_core::config::drill::PalwDrillSaltV1::from_hex(&v)
                        .unwrap_or_else(|e| die(format!("--palw-drill-genesis-salt: {e}"))),
                )
            }
            "--palw-drill-ruleset" => {
                let v = value("--palw-drill-ruleset");
                let (p, s) =
                    v.split_once(':').unwrap_or_else(|| die("--palw-drill-ruleset is <consensus_params_id>:<consensus_schedule_id>"));
                a.drill_ruleset = Some((p.to_string(), s.to_string()))
            }
            other => die(format!("unknown flag {other}")),
        }
    }
    a.key_file = key_file.unwrap_or_else(|| die("--key-file is required (the issuer's ML-DSA-87 seed)"));
    if a.drill_ruleset.is_some() && a.drill_salt.is_none() {
        die("--palw-drill-ruleset needs --palw-drill-genesis-salt: a stated ruleset is accepted only on a drill");
    }
    if a.keep == 0 {
        die("--keep 0 would delete every attestation just written");
    }
    a
}

/// Write `bytes` to `path` atomically (a temporary file in the same directory, then a rename): a reader never sees half a file.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Keep the newest `keep` attestation files (by modification time); older ones are this tool's own output and are removed.
fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in files.into_iter().skip(keep) {
        let _ = std::fs::remove_file(path);
    }
}

fn main() {
    let args = parse_args();
    let key = ValidatorKey::from_seed(load_validator_seed(&args.key_file).unwrap_or_else(|e| die(e)));
    if let Some(path) = &args.print_public_key {
        std::fs::write(path, faster_hex::hex_string(key.public_key())).unwrap_or_else(|e| die(format!("{}: {e}", path.display())));
        say("public-key", serde_json::json!({ "file": path.display().to_string(), "bytes": key.public_key().len() }));
        return;
    }
    let network = args.network.clone().unwrap_or_else(|| die("--network is required"));
    let endpoint = args.rpc.clone().unwrap_or_else(|| die("--rpc is required: your own full node"));
    let key_id = args.key_id.clone().unwrap_or_else(|| die("--key-id is required: the label miners configure for this key"));
    if key_id.is_empty() || key_id.starts_with("0x") {
        die("--key-id: a non-empty label, not 0x-prefixed");
    }
    let out = args.out.clone().unwrap_or_else(|| die("--out is required: the directory miners read attestations from"));
    std::fs::create_dir_all(&out).unwrap_or_else(|e| die(format!("{}: {e}", out.display())));
    let net: NetworkId = network.parse().unwrap_or_else(|e| die(format!("--network: {e}")));
    let params = kaspa_consensus_core::config::drill::palw_chain_params_v1(net, args.drill_salt.as_ref())
        .unwrap_or_else(|e| die(format!("--palw-drill-genesis-salt: {e}")));
    let mut ours = ClientRulesetV1::of(&params);
    if let Some((p, s)) = &args.drill_ruleset {
        ours.consensus_params_id = p.clone();
        ours.consensus_schedule_id = s.clone();
    }

    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap_or_else(|e| die(e));
    let url = if endpoint.contains("://") { endpoint.clone() } else { format!("ws://{endpoint}") };
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None).unwrap_or_else(|e| die(format!("{url}: {e}")));
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(std::time::Duration::from_secs(10)),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    runtime.block_on(client.connect(Some(options))).unwrap_or_else(|e| die(format!("cannot reach {url}: {e}")));

    // The node must run this issuer's ruleset — an issuer attests nothing judged under rules it does not run.
    let info = runtime.block_on(client.get_block_dag_info()).unwrap_or_else(|e| die(format!("getBlockDagInfo: {e}")));
    let status = runtime.block_on(client.get_palw_node_status()).unwrap_or_else(|e| die(format!("getPalwNodeStatus: {e}")));
    let theirs = misaka_palw_remote::verify::NodeRulesetV1 {
        network_id: info.network.to_string(),
        genesis: (!status.genesis_hash.is_empty()).then(|| status.genesis_hash.clone()),
        consensus_params_id: status.consensus_params_id.clone(),
        consensus_schedule_id: status.consensus_schedule_id.clone(),
    };
    if theirs.genesis.is_none() {
        die("the node does not report its genesis (getPalwNodeStatus version < 4): an attestation names the genesis it was made on");
    }
    misaka_palw_remote::verify::check_ruleset_v1(&ours, &theirs)
        .unwrap_or_else(|e| die(format!("the node is not on this ruleset: {e}")));
    say(
        "issuer",
        serde_json::json!({
            "key_id": key_id, "node": url, "network": ours.network_id, "genesis": ours.genesis,
            "commitment_fence": params.palw_fork_choice_commitment_v1.map(|f| f.daa_score()),
            "note": "attests only post-states this node served in the fork-choice form; below palw_fork_choice_commitment_v1 nothing is signed",
        }),
    );

    let sign = |msg: &[u8]| key.sign_with_context(msg, FORK_CHOICE_ATTESTATION_MLDSA87_CONTEXT).to_vec();
    let mut n = 0u64;
    loop {
        if args.rounds.is_some_and(|max| n >= max) {
            break;
        }
        n += 1;
        if n > 1 {
            std::thread::sleep(std::time::Duration::from_secs(args.every_secs));
        }
        let now_daa = match runtime.block_on(client.get_block_dag_info()) {
            Ok(i) => i.virtual_daa_score,
            Err(e) => {
                say("round-failed", serde_json::json!({ "why": format!("getBlockDagInfo: {e}") }));
                continue;
            }
        };
        let served = match runtime.block_on(
            client.get_palw_fork_choice_opening(kaspa_rpc_core::GetPalwForkChoiceOpeningRequest { block_hashes: Vec::new() }),
        ) {
            Ok(r) => r,
            Err(e) => {
                say("round-failed", serde_json::json!({ "why": format!("getPalwForkChoiceOpening: {e}") }));
                continue;
            }
        };
        if !served.available {
            say("nothing-served", serde_json::json!({ "why": served.reason }));
            continue;
        }
        let dns_gate = Some(PalwDnsGateFactV1 {
            stage_active: served.dns_overlay && served.dns_stage_active,
            confirmed_anchor: served
                .dns_confirmed_anchor
                .parse::<Hash64>()
                .ok()
                .filter(|_| served.dns_overlay)
                .map(|a| (a, served.dns_confirmed_anchor_daa)),
        });
        let mut skipped: Vec<(String, String)> = Vec::new();
        let entries: Vec<IssuerServedV1> = served
            .entries
            .iter()
            .filter_map(|e| {
                let fail = |why: &str| (e.block_hash.clone(), why.to_string());
                if !e.available {
                    skipped.push(fail(&e.reason));
                    return None;
                }
                let (Ok(block), Ok(committed_root)) = (e.block_hash.parse::<Hash64>(), e.committed_root.parse::<Hash64>()) else {
                    skipped.push(fail("malformed hash"));
                    return None;
                };
                let Some(header) = &e.header else {
                    skipped.push(fail("no header"));
                    return None;
                };
                let Some(opening) = opening_from_wire_v1(&e.leaf, &e.inner_root) else {
                    skipped.push(fail(if e.committed_form { "the leaf does not decode" } else { "below the commitment fence" }));
                    return None;
                };
                Some(IssuerServedV1 { block, daa_score: header.daa_score, opening, committed_root, committed_form: e.committed_form })
            })
            .collect();
        let (attestations, refused) = issue_attestations_v1(&ours, &entries, dns_gate, now_daa, key_id.as_bytes(), &sign);
        skipped.extend(refused.into_iter().map(|(b, why)| (b.to_string(), why)));
        let mut written = Vec::new();
        for att in &attestations {
            let path = out.join(format!("{}.json", att.block));
            match write_atomic(&path, att.to_json_v1().as_bytes()) {
                Ok(()) => written.push(att.block.to_string()),
                Err(e) => skipped.push((att.block.to_string(), format!("cannot write {}: {e}", path.display()))),
            }
        }
        prune(&out, args.keep);
        say(
            "round",
            serde_json::json!({
                "issued_at_daa": now_daa, "sink": served.sink, "tips": served.tips, "attested": written,
                "not_attested": skipped.iter().map(|(b, why)| serde_json::json!({ "block": b, "why": why })).collect::<Vec<_>>(),
            }),
        );
    }
}
