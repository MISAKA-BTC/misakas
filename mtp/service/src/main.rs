//! `misaka-mtp-service` — the MTP service-layer binary (ADR-0038 D2).
//!
//! Off-chain, consensus-neutral, **testnet-only**. Two subcommands cover the
//! trust-critical deterministic pipeline and the read-only self-serve surface;
//! the live collector I/O (p2p-crawler / wRPC chain-indexer / github-sync /
//! campaign-forms) is the injected non-deterministic edge (ADR-0038 §3 step 3)
//! and is wired per-deployment on top of the fact store this binary manages.
//!
//! ```text
//! misaka-mtp-service serve      --data-dir DIR --operator-key FILE --listen ADDR [--network testnet-10]
//! misaka-mtp-service run-epoch  --data-dir DIR --operator-key FILE \
//!                               --epoch N --start RFC3339 --end RFC3339 [--network testnet-10]
//! ```
//!
//! `serve` opens the signed-ledger archive read-only and serves the D3 query API.
//! `run-epoch` builds a fresh single-epoch fact store (G3), resolves attribution
//! (G1), scores + signs (core), and publishes the signed ledger (D6).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use kaspa_addresses::Prefix;
use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed};
use misaka_mtp::{Rules, Stage};
use misaka_mtp_collectors::EpochWindow;
use misaka_mtp_service::{Attributor, HttpState, LedgerArchive, NonceStore, PersistentStore, RegistrationRecord, config, epoch};

const USAGE: &str = "\
misaka-mtp-service — MISAKA Testnet Points Program service (ADR-0038, testnet-only)

USAGE:
  misaka-mtp-service serve     --data-dir DIR --operator-key FILE --listen ADDR [--network NET] [--pin STR]...
  misaka-mtp-service run-epoch --data-dir DIR --operator-key FILE --epoch N --start RFC3339 --end RFC3339 [--network NET]
  misaka-mtp-service issue-nonce --data-dir DIR --github ID --address ADDR [--network NET]
  misaka-mtp-service register    --data-dir DIR --request FILE

COMMON:
  --data-dir DIR        root data dir: <DIR>/facts (fact store), <DIR>/points (signed ledger archive),
                        <DIR>/registrations.jsonl (attribution registry)
  --operator-key FILE   dedicated MTP operator ML-DSA-87 seed file (0600, D7)
  --network NET         scored testnet network name (default: testnet-10)

serve:
  --listen ADDR         query-http bind address, e.g. 127.0.0.1:8790
  --pin STR             an out-of-band operator-key pin surfaced by /mtp/v1/operator (repeatable)

run-epoch:
  --epoch N             epoch number
  --start / --end       RFC-3339 UTC window bounds [start, end)
";

fn main() {
    std::process::exit(match run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("misaka-mtp-service: error: {e}");
            1
        }
    });
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => cmd_serve(&args[1..]),
        Some("run-epoch") => cmd_run_epoch(&args[1..]),
        Some("issue-nonce") => cmd_issue_nonce(&args[1..]),
        Some("register") => cmd_register(&args[1..]),
        Some("-h") | Some("--help") | Some("help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown subcommand '{other}'\n\n{USAGE}")),
    }
}

/// A tiny `--flag value` parser (no clap dep — mirrors the eth-rpc house style of
/// keeping the service dependency-light).
struct Flags {
    map: std::collections::HashMap<String, String>,
    multi: std::collections::HashMap<String, Vec<String>>,
}

impl Flags {
    fn parse(args: &[String], repeatable: &[&str]) -> Result<Self, String> {
        let mut map = std::collections::HashMap::new();
        let mut multi: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        let mut i = 0;
        while i < args.len() {
            let k = &args[i];
            let key = k.strip_prefix("--").ok_or_else(|| format!("expected a --flag, got '{k}'"))?;
            let v = args.get(i + 1).ok_or_else(|| format!("flag --{key} needs a value"))?.clone();
            if repeatable.contains(&key) {
                multi.entry(key.to_string()).or_default().push(v);
            } else {
                map.insert(key.to_string(), v);
            }
            i += 2;
        }
        Ok(Self { map, multi })
    }
    fn get(&self, k: &str) -> Result<&str, String> {
        self.map.get(k).map(String::as_str).ok_or_else(|| format!("missing required flag --{k}"))
    }
    fn opt(&self, k: &str) -> Option<&str> {
        self.map.get(k).map(String::as_str)
    }
    fn list(&self, k: &str) -> Vec<String> {
        self.multi.get(k).cloned().unwrap_or_default()
    }
}

fn network_or_default(flags: &Flags) -> Result<String, String> {
    let net = flags.opt("network").unwrap_or("testnet-10").to_string();
    if config::stage_for(&net).is_none() {
        return Err(format!(
            "network '{net}' is not in the testnet scope {:?} (D1)",
            config::NETWORKS.iter().map(|(n, _)| *n).collect::<Vec<_>>()
        ));
    }
    Ok(net)
}

fn load_operator_key(path: &str) -> Result<ValidatorKey, String> {
    let seed = load_validator_seed(path)?;
    Ok(ValidatorKey::from_seed(seed))
}

/// Load the persisted registrations (JSONL, one [`RegistrationRecord`] per line).
/// A missing file is an empty registry (fresh deployment).
fn load_registrations(path: &PathBuf) -> Result<Vec<RegistrationRecord>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(serde_json::from_str(line).map_err(|e| format!("{}:{}: malformed registration: {e}", path.display(), i + 1))?);
    }
    Ok(out)
}

fn cmd_serve(args: &[String]) -> Result<(), String> {
    let flags = Flags::parse(args, &["pin"])?;
    let data_dir = PathBuf::from(flags.get("data-dir")?);
    let listen: SocketAddr = flags.get("listen")?.parse().map_err(|e| format!("bad --listen address: {e}"))?;
    let _network = network_or_default(&flags)?;
    let key = load_operator_key(flags.get("operator-key")?)?;

    let archive_dir = data_dir.join("points");
    // Ensure the archive dir exists so the query API can open it immediately.
    LedgerArchive::open(&archive_dir).map_err(|e| e.to_string())?;

    let state = Arc::new(HttpState {
        archive_dir,
        operator_pubkey_hex: faster_hex::hex_string(key.public_key()),
        rules: Rules::default(),
        operator_pins: flags.list("pin"),
    });

    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| format!("tokio runtime: {e}"))?;
    rt.block_on(async move {
        // Stop cleanly on Ctrl-C so the process doesn't wedge (eth-rpc lesson).
        let shutdown = async {
            let _ = tokio::signal::ctrl_c().await;
        };
        misaka_mtp_service::serve_http_with_shutdown(listen, state, shutdown).await.map_err(|e| format!("http server: {e}"))
    })
}

fn cmd_run_epoch(args: &[String]) -> Result<(), String> {
    let flags = Flags::parse(args, &[])?;
    let data_dir = PathBuf::from(flags.get("data-dir")?);
    let network = network_or_default(&flags)?;
    let stage: Stage = config::stage_for(&network).expect("checked in network_or_default");
    let epoch_n: u64 = flags.get("epoch")?.parse().map_err(|e| format!("bad --epoch: {e}"))?;
    let start = flags.get("start")?.to_string();
    let end = flags.get("end")?.to_string();
    let key = load_operator_key(flags.get("operator-key")?)?;

    let store = PersistentStore::load(data_dir.join("facts")).map_err(|e| e.to_string())?;
    let attr = Attributor::from_records(load_registrations(&data_dir.join("registrations.jsonl"))?);
    let mut archive = LedgerArchive::open(data_dir.join("points")).map_err(|e| e.to_string())?;

    let window = EpochWindow { epoch: epoch_n, range: [start, end], network, stage };
    // Verification-required categories (C2 bug, C3/C4 verify/infra) are NOT auto-collected;
    // the operator hand-curates them with `misaka mtp award`, appending to this JSONL. Load
    // the awards for this (epoch, network) and merge them into the scored ledger.
    let manual = misaka_mtp_collectors::load_manual_awards(data_dir.join("manual-awards.jsonl"), window.epoch, &window.network)?;
    let ledger =
        epoch::run_epoch(&store, &attr, &Rules::default(), &key, &window, &mut archive, &manual).map_err(|e| e.to_string())?;

    let entry = archive.latest(epoch_n).expect("just published");
    println!(
        "published epoch {} issue {} — {} score rows, digest {} → {}",
        ledger.epoch,
        entry.issue,
        ledger.scores.len(),
        &entry.digest[..16.min(entry.digest.len())],
        entry.file
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Operator-side registration (ADR-0038 D3 preserved: no write endpoint added)
// ---------------------------------------------------------------------------

/// One issued, not-yet-consumed invitation. Persisted because issuing and ingesting are separate
/// operator runs — the in-memory `NonceStore` cannot span two processes, and the whole point of the
/// D3-preserving flow is that the participant signs out-of-band in between.
#[derive(serde::Serialize, serde::Deserialize)]
struct IssuedNonce {
    network: String,
    github: String,
    address: String,
    nonce: String,
    issued_at_ms: u64,
}

fn nonces_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("nonces.jsonl")
}

fn load_nonces(path: &PathBuf) -> Result<Vec<IssuedNonce>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(serde_json::from_str(line).map_err(|e| format!("{}:{}: malformed nonce: {e}", path.display(), i + 1))?);
    }
    Ok(out)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// `issue-nonce` — mint a single-use invitation for one (github, address) pair.
///
/// The operator hands the emitted JSON to the participant, who signs it with `misaka mtp register`
/// and submits the result back. Nothing is served over HTTP: D3 keeps that surface read-only.
fn cmd_issue_nonce(args: &[String]) -> Result<(), String> {
    let flags = Flags::parse(args, &[])?;
    let data_dir = PathBuf::from(flags.get("data-dir")?);
    let github = flags.get("github")?.to_string();
    let address = flags.get("address")?.to_string();
    let network = network_or_default(&flags)?;

    let mut nonce = [0u8; 32];
    getrandom_bytes(&mut nonce)?;
    let nonce_hex = faster_hex::hex_string(&nonce);
    let issued_at_ms = now_ms();

    let record = IssuedNonce {
        network: network.clone(),
        github: github.clone(),
        address: address.clone(),
        nonce: nonce_hex.clone(),
        issued_at_ms,
    };
    let path = nonces_path(&data_dir);
    let line = serde_json::to_string(&record).map_err(|e| format!("nonce JSON: {e}"))?;
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    writeln!(f, "{line}").map_err(|e| format!("cannot append {}: {e}", path.display()))?;

    // The participant needs exactly these fields to rebuild the challenge byte-for-byte.
    println!("{}", serde_json::to_string_pretty(&record).map_err(|e| format!("invitation JSON: {e}"))?);
    eprintln!("issued invitation for {github} / {address} [{network}] — recorded in {}", path.display());
    eprintln!("hand the JSON above to the participant; they run: misaka mtp register --invitation <file> --key-file <seed>");
    Ok(())
}

/// `register` — verify a participant's signed request and admit it to the attribution registry.
///
/// Fail-closed at every step: the nonce must exist and match the pair it was issued for, it must be
/// within TTL, the address must bind the pubkey, and the ML-DSA-87 signature must verify over the
/// exact challenge. The nonce is consumed either way, so a replay finds nothing.
fn cmd_register(args: &[String]) -> Result<(), String> {
    let flags = Flags::parse(args, &[])?;
    let data_dir = PathBuf::from(flags.get("data-dir")?);
    let request_path = flags.get("request")?.to_string();

    let raw = std::fs::read_to_string(&request_path).map_err(|e| format!("cannot read {request_path}: {e}"))?;
    let req: serde_json::Value = serde_json::from_str(&raw).map_err(|e| format!("request is not JSON: {e}"))?;
    let field = |k: &str| -> Result<String, String> {
        req.get(k).and_then(|v| v.as_str()).map(str::to_owned).ok_or_else(|| format!("request is missing string field '{k}'"))
    };
    let network = field("network")?;
    let github = field("github")?;
    let address = field("address")?;
    let nonce = field("nonce")?;
    let pubkey = decode_hex(&field("pubkey_hex")?, "pubkey_hex")?;
    let signature = decode_hex(&field("signature_hex")?, "signature_hex")?;

    if config::stage_for(&network).is_none() {
        return Err(format!("network '{network}' is not in the testnet scope (D1) — the request cannot be admitted"));
    }

    // Rebuild the nonce store from disk, replay the issued invitations into it, then consume.
    let npath = nonces_path(&data_dir);
    let issued = load_nonces(&npath)?;
    let mut nonces = NonceStore::new();
    let mut nonce_bytes = [0u8; 32];
    for rec in &issued {
        faster_hex::hex_decode(rec.nonce.as_bytes(), &mut nonce_bytes)
            .map_err(|e| format!("stored nonce {} is not 32-byte hex: {e}", rec.nonce))?;
        nonces.issue(&rec.network, &rec.github, &rec.address, nonce_bytes, rec.issued_at_ms);
    }

    let reg_path = data_dir.join("registrations.jsonl");
    let mut attr = Attributor::from_records(load_registrations(&reg_path)?);
    // D1 pins the scope to testnet names only (`config::NETWORKS`), and `network_or_default` /the
    // check above already refused anything outside it — so the address prefix is a constant here.
    // Deriving it through `NetworkId` would mean pulling kaspa-consensus-core into a service that is
    // deliberately dependency-light, to compute a value D1 has already fixed.
    let prefix = Prefix::Testnet;
    let record = attr
        .register(&mut nonces, &network, &github, &address, &pubkey, &nonce, &signature, now_ms(), prefix)
        .map_err(|e| format!("registration rejected: {e}"))?;

    let line = serde_json::to_string(&record).map_err(|e| format!("record JSON: {e}"))?;
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&reg_path)
        .map_err(|e| format!("cannot open {}: {e}", reg_path.display()))?;
    writeln!(f, "{line}").map_err(|e| format!("cannot append {}: {e}", reg_path.display()))?;

    // Burn the consumed invitation so a second run cannot replay it from the file we rebuilt from.
    let remaining: Vec<String> = issued
        .iter()
        .filter(|r| r.nonce != nonce)
        .map(|r| serde_json::to_string(r).unwrap_or_default())
        .filter(|s| !s.is_empty())
        .collect();
    std::fs::write(&npath, if remaining.is_empty() { String::new() } else { format!("{}\n", remaining.join("\n")) })
        .map_err(|e| format!("cannot rewrite {}: {e}", npath.display()))?;

    println!("registered {} → ledger id {}", record.address, record.ledger_id());
    eprintln!("appended to {} — the next `run-epoch` attributes this identity's facts", reg_path.display());
    Ok(())
}

fn decode_hex(s: &str, what: &str) -> Result<Vec<u8>, String> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    if h.len() % 2 != 0 {
        return Err(format!("{what} is not valid hex (odd length)"));
    }
    let mut out = vec![0u8; h.len() / 2];
    faster_hex::hex_decode(h.as_bytes(), &mut out).map_err(|e| format!("{what} is not valid hex: {e}"))?;
    Ok(out)
}

fn getrandom_bytes(buf: &mut [u8]) -> Result<(), String> {
    use std::io::Read as _;
    let mut f = std::fs::File::open("/dev/urandom").map_err(|e| format!("cannot open /dev/urandom: {e}"))?;
    f.read_exact(buf).map_err(|e| format!("cannot read /dev/urandom: {e}"))
}
