//! `misaka-mtp-service` — the MTP service-layer binary (ADR-0038 D2).
//!
//! Off-chain, consensus-neutral, **testnet-only**. Two subcommands cover the
//! trust-critical deterministic pipeline and the read-only self-serve surface;
//! the live collector I/O (p2p-crawler / wRPC chain-indexer / github-sync /
//! campaign-forms) is the injected non-deterministic edge (ADR-0038 §3 step 3)
//! and is wired per-deployment on top of the fact store this binary manages.
//!
//! ```text
//! misaka-mtp-service serve      --data-dir DIR --operator-key FILE --listen ADDR [--network testnet-20]
//! misaka-mtp-service run-epoch  --data-dir DIR --operator-key FILE \
//!                               --epoch N --start RFC3339 --end RFC3339 [--network testnet-20]
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
use misaka_mtp_collectors::{
    AcceptedPalwLeaf, AttestationRow, Identity, IdentityKind, NodeRecord, PalwReplicaCollector, Rejected, ReplicaSlot, UptimeSample,
};
use misaka_mtp_service::{
    Attributor, HttpState, LedgerArchive, NonceStore, PersistentStore, RegistrationRecord, config, epoch, extract_claim_token,
};

const USAGE: &str = "\
misaka-mtp-service — MISAKA Testnet Points Program service (ADR-0038, testnet-only)

USAGE:
  misaka-mtp-service serve     --data-dir DIR --operator-key FILE --listen ADDR [--network NET] [--pin STR]...
  misaka-mtp-service run-epoch --data-dir DIR --operator-key FILE --epoch N --start RFC3339 --end RFC3339 [--network NET]
  misaka-mtp-service issue-nonce --data-dir DIR --github ID --address ADDR [--network NET]
  misaka-mtp-service register    --data-dir DIR --request FILE
  misaka-mtp-service ingest-probes --data-dir DIR --file PROBES.jsonl [--roster ROSTER.jsonl] [--network NET]
  misaka-mtp-service ingest-attestations --data-dir DIR --file ATT.jsonl --roster VROSTER.jsonl [--bonds BONDS.jsonl] [--network NET]
  misaka-mtp-service ingest-palw --data-dir DIR --file LEAVES.jsonl [--network NET]

COMMON:
  --data-dir DIR        root data dir: <DIR>/facts (fact store), <DIR>/points (signed ledger archive),
                        <DIR>/registrations.jsonl (attribution registry)
  --operator-key FILE   dedicated MTP operator ML-DSA-87 seed file (0600, D7)
  --network NET         scored testnet network name (default: testnet-20)

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
        Some("ingest-probes") => cmd_ingest_probes(&args[1..]),
        Some("ingest-attestations") => cmd_ingest_attestations(&args[1..]),
        Some("ingest-palw") => cmd_ingest_palw(&args[1..]),
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
    let net = flags.opt("network").unwrap_or("testnet-20").to_string();
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
    eprintln!("the invitation is valid for 7 days (single-use); reissue if the round-trip takes longer");
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
    println!("claim token: {}", record.claim_token);
    eprintln!("appended to {} — the next `run-epoch` attributes this identity's facts", reg_path.display());
    eprintln!(
        "relay the claim token to the participant: restarting their node with --uacomment=mtp:{} \
         makes C1 uptime attribute to them automatically (no roster entry needed)",
        record.claim_token
    );
    Ok(())
}

fn decode_hex(s: &str, what: &str) -> Result<Vec<u8>, String> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    if !h.len().is_multiple_of(2) {
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

// ---------------------------------------------------------------------------
// ingest-probes — attribute a vantage's observations, into the fact store
// ---------------------------------------------------------------------------

/// One roster line: which ledger id owns a node key. This is the operator's **manual override**;
/// the self-serve path is the I-MTP-11 claim-token, which the participant's node advertises in its
/// user-agent (`--uacomment=mtp:<token>`) and ingestion resolves through the registration index.
/// A roster entry wins over a token for the same `node_key`, so a mis-pasted or hijacked comment
/// can always be corrected by the operator.
#[derive(serde::Deserialize)]
struct RosterEntry {
    node_key: String,
    owner_id: String,
}

/// Resolve the owner of one observed peer (C1 attribution). Precedence:
///
/// 1. an explicit roster entry for the `node_key` (operator override);
/// 2. an I-MTP-11 claim-token found in the observed user-agent that resolves to a
///    registration ([`extract_claim_token`] → [`Attributor::resolve_token`]).
///
/// Anything else is `None` — the peer stays unattributed (fail-closed; most of the
/// network is simply not registered). The token path yields a registered ledger id
/// by construction; a roster id that is stale or mistyped is still dropped later by
/// `resolve_attribution` (I-MTP-1), so no path can smuggle an unregistered id into
/// the scored ledger.
fn probe_owner(
    roster: &std::collections::HashMap<String, String>,
    attr: &Attributor,
    node_key: &str,
    user_agent: &str,
) -> Option<String> {
    if let Some(owner) = roster.get(node_key) {
        return Some(owner.clone());
    }
    extract_claim_token(user_agent).and_then(|t| attr.resolve_token(&t)).map(str::to_string)
}

/// One line of a vantage's `misaka mtp collect` output.
#[derive(serde::Deserialize)]
struct Probe {
    network: String,
    vantage: String,
    at_ms: u64,
    node_key: String,
    address: String,
    in_sync: bool,
    user_agent: String,
}

/// Derive the §5 co-location key from an observed peer address: the first three octets of an IPv4.
///
/// This is why C1 does not need a GeoIP feed to start. `NodeRecord` takes the /24 as the cap key and
/// ASN only as the alternate, and the /24 is already in the observation — no external data source,
/// no licence, nothing to keep fresh. ASN stays `None` until someone chooses a provider; the cap
/// still works on the /24.
fn ipv4_24(address: &str) -> Option<[u8; 3]> {
    let host = address.rsplit_once(':').map(|(h, _)| h).unwrap_or(address);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let mut parts = host.split('.');
    let a = parts.next()?.parse::<u8>().ok()?;
    let b = parts.next()?.parse::<u8>().ok()?;
    let c = parts.next()?.parse::<u8>().ok()?;
    parts.next()?.parse::<u8>().ok()?; // must be a full dotted quad, not a prefix
    if parts.next().is_some() {
        return None;
    }
    Some([a, b, c])
}

fn cmd_ingest_probes(args: &[String]) -> Result<(), String> {
    let flags = Flags::parse(args, &[])?;
    let data_dir = PathBuf::from(flags.get("data-dir")?);
    let file = flags.get("file")?.to_string();
    let network = network_or_default(&flags)?;

    // The roster is optional: since the claim-token path went live, a probe attributes itself when
    // the observed user-agent carries a registered `mtp:<token>`. The roster remains the operator's
    // explicit override (and the only path for peers that cannot restart with a uacomment).
    let mut owner_of: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(roster_path) = flags.opt("roster") {
        let roster_text = std::fs::read_to_string(roster_path).map_err(|e| format!("cannot read roster {roster_path}: {e}"))?;
        for (i, line) in roster_text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let e: RosterEntry = serde_json::from_str(line).map_err(|err| format!("{roster_path}:{}: {err}", i + 1))?;
            owner_of.insert(e.node_key, e.owner_id);
        }
    }

    // The registration index the claim-token path resolves through (same file `register` appends).
    let attr = Attributor::from_records(load_registrations(&data_dir.join("registrations.jsonl"))?);

    let text = std::fs::read_to_string(&file).map_err(|e| format!("cannot read {file}: {e}"))?;
    let mut store = PersistentStore::load(data_dir.join("facts")).map_err(|e| e.to_string())?;

    let (mut ingested, mut unattributed, mut wrong_net) = (0usize, 0usize, 0usize);
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let p: Probe = serde_json::from_str(line).map_err(|e| format!("{file}:{}: malformed probe: {e}", i + 1))?;
        if p.network != network {
            wrong_net += 1;
            continue;
        }
        // An unclaimed peer is not an error: most of the network is not registered. Count it and
        // move on rather than inventing an owner, which would put someone else's uptime on a ledger.
        let Some(owner) = probe_owner(&owner_of, &attr, &p.node_key, &p.user_agent) else {
            unattributed += 1;
            continue;
        };

        store.upsert_identity(Identity { id: owner.clone(), kind: IdentityKind::Node }).map_err(|e| e.to_string())?;
        store
            .upsert_node(NodeRecord {
                node_key: p.node_key.clone(),
                owner_id: owner.clone(),
                ip_v4_24: ipv4_24(&p.address),
                asn: None,
                // Both multipliers need cross-sample context this single-file pass does not have:
                // geo diversity compares an owner's nodes across vantages, and fast-follow compares
                // `user_agent` against the current release. Left false rather than guessed — an
                // unearned 1.5x or 1.2x is worse than a missing one.
                geo_diverse: false,
                fast_follow: false,
                first_seen_ms: p.at_ms,
            })
            .map_err(|e| e.to_string())?;
        store
            .append_sample(UptimeSample {
                node_key: p.node_key.clone(),
                at_ms: p.at_ms,
                in_sync: p.in_sync,
                vantage: p.vantage.clone(),
                evidence: format!("{}:{}:{}", p.vantage, p.at_ms, p.user_agent),
            })
            .map_err(|e| e.to_string())?;
        ingested += 1;
    }

    println!("ingested {ingested} sample(s) into {}", data_dir.join("facts").display());
    if unattributed > 0 {
        println!("  {unattributed} observation(s) skipped: no roster entry and no registered mtp:<token> in the user-agent");
    }
    if wrong_net > 0 {
        println!("  {wrong_net} observation(s) skipped: recorded on a different network than {network}");
    }
    Ok(())
}

#[cfg(test)]
mod ingest_tests {
    use super::{Attributor, RegistrationRecord, ipv4_24, probe_owner};

    /// C1 attribution resolves through the claim-token self-serve path (I-MTP-11), with the
    /// roster as the operator override — and through nothing else (fail-closed).
    #[test]
    fn probe_owner_resolves_roster_then_token_then_nothing() {
        let token = misaka_mtp_service::claim_token("alice", "misakatest:aaa");
        let attr = Attributor::from_records(vec![RegistrationRecord {
            github: "alice".into(),
            address: "misakatest:aaa".into(),
            pubkey: vec![],
            claim_token: token.clone(),
            registered_at_ms: 0,
        }]);
        let mut roster = std::collections::HashMap::new();
        roster.insert("node-r".to_string(), "gh:bob".to_string());

        let ua = format!("/kaspad:1.0.1/kaspad:1.0.1(mtp:{token})/");
        // Token in the observed user-agent → the registered ledger id, no roster line needed.
        assert_eq!(probe_owner(&roster, &attr, "node-t", &ua), Some("gh:alice".to_string()));
        // A roster entry wins over the token for the same node_key (operator override).
        assert_eq!(probe_owner(&roster, &attr, "node-r", &ua), Some("gh:bob".to_string()));
        // No roster entry, no token → unattributed; an UNREGISTERED token is likewise nothing —
        // a stranger advertising a random mtp:<hex> cannot park uptime anywhere.
        assert_eq!(probe_owner(&roster, &attr, "node-x", "/kaspad:1.0.1/"), None);
        assert_eq!(probe_owner(&roster, &attr, "node-x", "/kaspad:1.0.1(mtp:00112233445566778899aabb)/"), None);
    }

    #[test]
    fn the_colocation_key_comes_from_the_observation_not_a_geoip_feed() {
        // §5 caps co-location by /24, and the /24 is already inside the address the collector
        // recorded — this is the whole reason C1 needs no external data source to start.
        assert_eq!(ipv4_24("160.16.131.119:46611"), Some([160, 16, 131]));
        assert_eq!(ipv4_24("95.111.236.186"), Some([95, 111, 236]));
        // Two hosts in one /24 collapse to one key, which is what makes the cap bite.
        assert_eq!(ipv4_24("203.0.113.7:16611"), ipv4_24("203.0.113.250:16611"));
        // A different /24 must NOT collide with it.
        assert_ne!(ipv4_24("203.0.113.7:16611"), ipv4_24("203.0.114.7:16611"));

        // IPv6 has no /24 — `None` (no ASN either), so such a node is uncapped rather than
        // wrongly grouped. Recorded as a known gap, not silently mapped onto some other key.
        assert_eq!(ipv4_24("[2001:db8::1]:16611"), None);
        // Malformed / truncated / oversized inputs are None, never a partial key.
        assert_eq!(ipv4_24("160.16.131"), None);
        assert_eq!(ipv4_24("160.16.131.119.7"), None);
        assert_eq!(ipv4_24("999.1.1.1"), None);
        assert_eq!(ipv4_24("not-an-address"), None);
    }
}

// ---------------------------------------------------------------------------
// ingest-attestations — chain participation becomes C1 validator facts
// ---------------------------------------------------------------------------

/// One roster line mapping an on-chain validator to a ledger id.
#[derive(serde::Deserialize)]
struct ValidatorRosterEntry {
    /// The 64-byte validator hash as hex, exactly as `misaka mtp attestations` prints it.
    validator_id: String,
    owner_id: String,
}

/// One line of `misaka mtp attestations` output.
#[derive(serde::Deserialize)]
struct AttestationLine {
    network: String,
    validator_id: String,
    att_epoch: u64,
    /// The containing block's header timestamp. The fact store windows by wall clock, so this —
    /// not the epoch number — is what decides whether a run-epoch window sees the fact.
    at_ms: u64,
    evidence_block: Option<String>,
    evidence_tx: Option<String>,
}

/// One line of `misaka mtp validators` output — the slash half of the join.
#[derive(serde::Deserialize)]
struct BondLine {
    validator_id: String,
    slashed: bool,
}

fn cmd_ingest_attestations(args: &[String]) -> Result<(), String> {
    let flags = Flags::parse(args, &[])?;
    let data_dir = PathBuf::from(flags.get("data-dir")?);
    let file = flags.get("file")?.to_string();
    let roster_path = flags.get("roster")?.to_string();
    let bonds_path = flags.opt("bonds").map(|s| s.to_string());
    let network = network_or_default(&flags)?;

    // The roster is not a convenience: `EpochInput` filters attestations by `validator_id` against
    // the *registered ledger ids*, so a row carrying a raw chain hash is dropped silently. The
    // mapping has to happen here or the facts never score.
    let roster_text = std::fs::read_to_string(&roster_path).map_err(|e| format!("cannot read roster {roster_path}: {e}"))?;
    let mut owner_of: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (i, line) in roster_text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let e: ValidatorRosterEntry = serde_json::from_str(line).map_err(|err| format!("{roster_path}:{}: {err}", i + 1))?;
        owner_of.insert(e.validator_id, e.owner_id);
    }

    // Slash state is registry state, so it comes from the bond reader rather than the block walk —
    // one source per fact, so the two can never contradict each other.
    let mut slashed_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(p) = &bonds_path {
        let text = std::fs::read_to_string(p).map_err(|e| format!("cannot read bonds {p}: {e}"))?;
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let b: BondLine = serde_json::from_str(line).map_err(|err| format!("{p}:{}: {err}", i + 1))?;
            if b.slashed {
                slashed_ids.insert(b.validator_id);
            }
        }
    }

    let text = std::fs::read_to_string(&file).map_err(|e| format!("cannot read {file}: {e}"))?;
    let mut store = PersistentStore::load(data_dir.join("facts")).map_err(|e| e.to_string())?;

    // A DAG puts one shard tx in several blocks, so the index legitimately repeats a
    // (validator, epoch) pair. Collapsing here is what keeps `attested/total` from exceeding 1.
    let mut seen: std::collections::HashSet<(String, u64)> = std::collections::HashSet::new();
    let (mut ingested, mut duplicates, mut unattributed, mut wrong_net) = (0usize, 0usize, 0usize, 0usize);
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let a: AttestationLine = serde_json::from_str(line).map_err(|e| format!("{file}:{}: malformed attestation: {e}", i + 1))?;
        if a.network != network {
            wrong_net += 1;
            continue;
        }
        let Some(owner) = owner_of.get(&a.validator_id) else {
            unattributed += 1;
            continue;
        };
        if !seen.insert((owner.clone(), a.att_epoch)) {
            duplicates += 1;
            continue;
        }
        let slashed = slashed_ids.contains(&a.validator_id);
        store
            .append_attestation(
                a.at_ms,
                AttestationRow {
                    validator_id: owner.clone(),
                    att_epoch: a.att_epoch,
                    attested: true,
                    slashed,
                    evidence: format!("{}:{}", a.evidence_block.as_deref().unwrap_or("?"), a.evidence_tx.as_deref().unwrap_or("?")),
                },
            )
            .map_err(|e| e.to_string())?;
        ingested += 1;
    }

    println!("ingested {ingested} attestation fact(s) into {}", data_dir.join("facts").display());
    if duplicates > 0 {
        println!("  {duplicates} duplicate (validator, epoch) row(s) collapsed — the same shard tx in several blocks");
    }
    if unattributed > 0 {
        println!("  {unattributed} row(s) skipped: validator_id not in the roster (unregistered validators)");
    }
    if wrong_net > 0 {
        println!("  {wrong_net} row(s) skipped: recorded on a different network than {network}");
    }
    if bonds_path.is_none() {
        println!("  NOTE: no --bonds given, so every row is slashed=false. Pass `misaka mtp validators` output to fill it.");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ingest-palw — accepted C5 replica work becomes llm_replica_work facts
// ---------------------------------------------------------------------------

/// One slot of a `misaka mtp palw-leaves` line.
#[derive(serde::Deserialize)]
struct PalwSlotLine {
    replica_slot: u8,
    execution_nullifier: String,
    worker_credential_id: String,
    provider_bond: String,
    owner_address: String,
}

/// One line of `misaka mtp palw-leaves` output.
#[derive(serde::Deserialize)]
struct PalwLeafLine {
    network: String,
    /// The finality coordinate the reader scanned under. The collector re-checks every row
    /// against it, so a buggy or lying reader cannot smuggle an unburied leaf into the ledger.
    finality_daa_score: u64,
    batch_id: String,
    leaf_index: u32,
    accepting_block: String,
    accepted_daa_score: u64,
    completed_at_ms: u64,
    pair_id: String,
    job_challenge: String,
    k2_matched: bool,
    canonical_compute_units: u64,
    slots: Vec<PalwSlotLine>,
}

/// Parse a leaves JSONL into collector inputs: `(leaves, finality, wrong_net)`.
///
/// The finality coordinate is the MAX the file asserts: finality only moves forward, so rows from
/// an older scan (asserted under a lower coordinate) still satisfy `accepted <= finality`, while a
/// row claiming acceptance above every scan's coordinate is exactly the NotFinal drop the
/// collector exists to refuse. Slots are sorted A=0 / B=1 — the reader already emits them that
/// way, but the shared-credential refusal reads `slots[0]`/`slots[1]`, so ordering is a contract,
/// not a nicety.
fn palw_leaves_from_jsonl(text: &str, network: &str, source: &str) -> Result<(Vec<AcceptedPalwLeaf>, u64, usize), String> {
    let mut leaves = Vec::new();
    let mut finality = 0u64;
    let mut wrong_net = 0usize;
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let l: PalwLeafLine = serde_json::from_str(line).map_err(|e| format!("{source}:{}: malformed leaf row: {e}", i + 1))?;
        if l.network != network {
            wrong_net += 1;
            continue;
        }
        finality = finality.max(l.finality_daa_score);
        let mut slots: Vec<ReplicaSlot> = l
            .slots
            .into_iter()
            .map(|s| ReplicaSlot {
                replica_slot: s.replica_slot,
                execution_nullifier: s.execution_nullifier,
                worker_credential_id: s.worker_credential_id,
                provider_bond: s.provider_bond,
                owner_address: s.owner_address,
            })
            .collect();
        slots.sort_by_key(|s| s.replica_slot);
        leaves.push(AcceptedPalwLeaf {
            batch_id: l.batch_id,
            leaf_index: l.leaf_index,
            accepting_block: l.accepting_block,
            accepted_daa_score: l.accepted_daa_score,
            completed_at_ms: l.completed_at_ms,
            pair_id: l.pair_id,
            job_challenge: l.job_challenge,
            k2_matched: l.k2_matched,
            canonical_compute_units: l.canonical_compute_units,
            slots,
        });
    }
    Ok((leaves, finality, wrong_net))
}

/// `ingest-palw` — normalize `misaka mtp palw-leaves` output into C5 `llm_replica_work` facts.
///
/// The credit rules run in exactly one place — [`PalwReplicaCollector::normalize`], the same code
/// the pipeline tests pin — and the owner seam is the registration registry's own
/// `OwnerResolver` impl, so a bond-owner address earns points iff it belongs to a registered
/// participant. This command only parses, reports every drop with its reason, and appends the
/// surviving rows idempotently (a row whose execution nullifier is already on file is an
/// overlapping re-scan, not new work).
fn cmd_ingest_palw(args: &[String]) -> Result<(), String> {
    let flags = Flags::parse(args, &[])?;
    let data_dir = PathBuf::from(flags.get("data-dir")?);
    let file = flags.get("file")?.to_string();
    let network = network_or_default(&flags)?;

    let text = std::fs::read_to_string(&file).map_err(|e| format!("cannot read {file}: {e}"))?;
    let (leaves, finality_daa_score, wrong_net) = palw_leaves_from_jsonl(&text, &network, &file)?;

    let attr = Attributor::from_records(load_registrations(&data_dir.join("registrations.jsonl"))?);
    let report = PalwReplicaCollector { leaves, finality_daa_score, resolver: attr }.normalize();

    let mut store = PersistentStore::load(data_dir.join("facts")).map_err(|e| e.to_string())?;
    let mut seen: std::collections::HashSet<String> = store.llm_replica_nullifiers().map(str::to_string).collect();
    let (mut ingested, mut duplicates) = (0usize, 0usize);
    for row in report.rows {
        if !seen.insert(row.execution_nullifier.clone()) {
            duplicates += 1;
            continue;
        }
        store.upsert_identity(Identity { id: row.owner_id.clone(), kind: IdentityKind::Address }).map_err(|e| e.to_string())?;
        store.append_llm_replica_work(row.completed_at_ms, row).map_err(|e| e.to_string())?;
        ingested += 1;
    }

    println!("ingested {ingested} C5 replica slot(s) into {} (finality DAA {finality_daa_score})", data_dir.join("facts").display());
    if duplicates > 0 {
        println!("  {duplicates} slot(s) skipped: execution nullifier already on file (overlapping re-scan, not new work)");
    }
    if wrong_net > 0 {
        println!("  {wrong_net} row(s) skipped: recorded on a different network than {network}");
    }
    if !report.rejected.is_empty() {
        println!("  {} drop(s), each with its reason (a silent drop would read as \"no work\"):", report.rejected.len());
        for r in &report.rejected {
            match r {
                Rejected::NotMatched { pair_id } => println!("    pair {pair_id}: the k=2 replicas did not reproduce each other"),
                Rejected::NotFinal { pair_id, accepted_daa_score, finality_daa_score } => println!(
                    "    pair {pair_id}: accepted at daa {accepted_daa_score}, above the finality coordinate \
                     {finality_daa_score} — re-scan once buried"
                ),
                Rejected::MalformedPair { pair_id, slots } => {
                    println!("    pair {pair_id}: {slots} slot(s) — a PALW job is exactly two")
                }
                Rejected::SharedCredential { pair_id, worker_credential_id } => println!(
                    "    pair {pair_id}: both slots signed by credential {worker_credential_id} — internally \
                     inconsistent evidence, refused whole"
                ),
                Rejected::UnregisteredOwner { pair_id, owner_address } => println!(
                    "    pair {pair_id}: {owner_address} was not a registered participant — dropped, never parked \
                     (registering later must not claim earlier work)"
                ),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod palw_ingest_tests {
    use super::*;
    use misaka_mtp_collectors::OwnerResolver;

    fn leaf_line(network: &str, finality: u64, pair: &str, slots_json: &str) -> String {
        format!(
            r#"{{"network":"{network}","finality_daa_score":{finality},"batch_id":"batch-1","leaf_index":7,
                "minted_block":"m1","accepting_block":"c1","accepted_daa_score":900,"completed_at_ms":1700000000000,
                "pair_id":"{pair}","job_challenge":"ch-1","k2_matched":true,"canonical_compute_units":781556,
                "slots":[{slots_json}]}}"#
        )
        .replace('\n', " ")
    }

    fn slot_json(slot: u8, owner: &str, nullifier: &str) -> String {
        format!(
            r#"{{"replica_slot":{slot},"execution_nullifier":"{nullifier}","worker_credential_id":"cred-{slot}",
                "provider_bond":"bond-{slot}:0","owner_address":"{owner}"}}"#
        )
        .replace('\n', " ")
    }

    /// The DTO carries every credit-deciding field verbatim, sorts slots into the A=0/B=1
    /// contract even if the file has them reversed, takes the max finality across lines, and
    /// counts (never silently drops) rows from another network.
    #[test]
    fn lines_map_field_for_field_sort_slots_and_scope_by_network() {
        let text = format!(
            "{}\n{}\n",
            // slots deliberately reversed: B first.
            leaf_line(
                "testnet-20",
                1_000,
                "p1",
                &format!("{},{}", slot_json(1, "misakatest:bob", "nb"), slot_json(0, "misakatest:alice", "na"))
            ),
            leaf_line("testnet-200", 2_000, "p2", &slot_json(0, "misakatest:carol", "nc")),
        );
        let (leaves, finality, wrong_net) = palw_leaves_from_jsonl(&text, "testnet-20", "test").unwrap();
        assert_eq!(leaves.len(), 1, "the testnet-200 row is scoped out");
        assert_eq!(wrong_net, 1);
        assert_eq!(finality, 1_000, "finality comes from in-scope lines' max");
        let leaf = &leaves[0];
        assert_eq!(leaf.pair_id, "p1");
        assert_eq!(leaf.accepted_daa_score, 900);
        assert_eq!(leaf.completed_at_ms, 1_700_000_000_000);
        assert_eq!(leaf.canonical_compute_units, 781_556);
        assert_eq!((leaf.slots[0].replica_slot, leaf.slots[1].replica_slot), (0, 1), "reversed slots are re-sorted");
        assert_eq!(leaf.slots[0].owner_address, "misakatest:alice");
        assert_eq!(leaf.slots[0].execution_nullifier, "na");
        assert_eq!(leaf.slots[1].provider_bond, "bond-1:0");
    }

    /// A malformed line is a hard error naming the file and line — a fact file that half-parses
    /// must not half-ingest.
    #[test]
    fn a_malformed_line_names_its_source() {
        let err = palw_leaves_from_jsonl("{\"network\":42}\n", "testnet-20", "leaves.jsonl").unwrap_err();
        assert!(err.contains("leaves.jsonl:1"), "{err}");
    }

    /// End to end through the REAL collector: a parsed line normalizes into one row per slot,
    /// attributed through the registration registry's own resolver seam.
    #[test]
    fn parsed_lines_normalize_through_the_registry_resolver() {
        let text = leaf_line(
            "testnet-20",
            1_000,
            "p1",
            &format!("{},{}", slot_json(0, "misakatest:alice", "na"), slot_json(1, "misakatest:stranger", "nb")),
        );
        let (leaves, finality, _) = palw_leaves_from_jsonl(&text, "testnet-20", "test").unwrap();
        let attr = Attributor::from_records(vec![RegistrationRecord {
            github: "alice".into(),
            address: "misakatest:alice".into(),
            pubkey: vec![],
            claim_token: "t".into(),
            registered_at_ms: 0,
        }]);
        assert_eq!(attr.ledger_id_for_address("misakatest:alice").as_deref(), Some("gh:alice"));
        let report = PalwReplicaCollector { leaves, finality_daa_score: finality, resolver: attr }.normalize();
        assert_eq!(report.rows.len(), 1, "alice's slot is credited");
        assert_eq!(report.rows[0].owner_id, "gh:alice");
        assert_eq!(report.rows[0].evidence, "c1#batch-1:7", "the row cites the accepting block the line named");
        assert!(
            matches!(report.rejected.as_slice(), [Rejected::UnregisteredOwner { owner_address, .. }] if owner_address == "misakatest:stranger"),
            "the stranger is reported, not silently skipped: {:?}",
            report.rejected
        );
    }
}
