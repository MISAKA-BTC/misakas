//! `palw-bridge-verify` — drive the four consensus seams against a RUNNING bridge and a LIVE
//! node, end to end, and print a pass/fail table.
//!
//! This is the live counterpart of `tests/consensus_seams.rs`: same assertions, but every fact
//! comes off the chain instead of a pinned file — the challenge is salted by the node's real
//! buried beacon, the bonds are real on-chain PALW provider bonds, and the DA chunk sample is
//! drawn from the real beacon seed. It plays all three roles (submitter A, replica B, auditor C)
//! so one operator can verify the whole lane on one desk.
//!
//! It is a VERIFIER, not a miner: it submits jobs whose "outputs" are chosen token vectors, so
//! the k=2 predicate is exercised over real commitments without needing three 35B engines. What
//! it proves is the coordination lane — lease → bonded submit → DA obligation/proof → replica
//! match, and the mismatch → auditor draw → attribution path. What it does not prove is that any
//! particular model produced those tokens; that is the gateway's job and is covered by the
//! qwen-8.0 side's live 35B run.

use std::io::{Read, Write};

use kaspa_consensus_core::palw::da::{PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT, PalwProviderSessionAuthorizationV1};
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::ValidatorKey;
use misaka_palw_bridge::chain::parse_outpoint;
use misaka_palw_bridge::da::{ChatContextObjectV4, DaCommitmentWire, DaObligation, DaResponseWire};
use misaka_palw_bridge::match_key::{RUNTIME_CLASS_LABEL, bytes_hex, decode_hex, hash64_hex};
use misaka_palw_bridge::provider::{BRIDGE_REQUEST_MLDSA87_CONTEXT, body_digest, request_signing_hash};
use serde_json::{Value, json};

struct Args {
    bridge: String,
    network_id: u32,
    /// (label, owner seed path, bond outpoint)
    providers: Vec<(String, String, String)>,
}

fn parse_args() -> Result<Args, String> {
    let mut bridge = "http://127.0.0.1:26621/palw/v1".to_string();
    let mut network_id = 20u32;
    let mut providers = Vec::new();
    let argv: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        let take = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            argv.get(*i).cloned().ok_or_else(|| format!("{} needs a value", argv[*i - 1]))
        };
        match argv[i].as_str() {
            "--bridge" => bridge = take(&mut i)?,
            "--network-id" => network_id = take(&mut i)?.parse().map_err(|e| format!("--network-id: {e}"))?,
            // --provider LABEL=SEED_PATH=txid:index
            "--provider" => {
                let spec = take(&mut i)?;
                let parts: Vec<&str> = spec.splitn(3, '=').collect();
                if parts.len() != 3 {
                    return Err(format!("--provider wants LABEL=SEED_PATH=txid:index, got {spec:?}"));
                }
                providers.push((parts[0].into(), parts[1].into(), parts[2].into()));
            }
            other => return Err(format!("unknown flag {other}")),
        }
        i += 1;
    }
    if providers.len() < 3 {
        return Err("need three --provider entries (submitter, replica, auditor)".into());
    }
    Ok(Args { bridge: bridge.trim_end_matches('/').to_string(), network_id, providers })
}

// ---- minimal blocking HTTP ---------------------------------------------------------------

fn http(method: &str, url: &str, body: Option<&Value>, signature: Option<(&str, &str)>) -> Result<(u16, Value), String> {
    let rest = url.strip_prefix("http://").ok_or("only http:// urls")?;
    let (host_port, path) = match rest.split_once('/') {
        Some((hp, p)) => (hp, format!("/{p}")),
        None => (rest, "/".into()),
    };
    let authority = if host_port.contains(':') { host_port.to_string() } else { format!("{host_port}:80") };
    let mut stream = std::net::TcpStream::connect(&authority).map_err(|e| format!("connect {authority}: {e}"))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(60))).map_err(|e| e.to_string())?;

    let payload = body.map(serde_json::to_vec).transpose().map_err(|e| e.to_string())?.unwrap_or_default();
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        payload.len()
    );
    if let Some((bond, sig)) = signature {
        head.push_str(&format!("X-Palw-Bond: {bond}\r\nX-Palw-Signature: {sig}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).map_err(|e| e.to_string())?;
    stream.write_all(&payload).map_err(|e| e.to_string())?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| format!("read {url}: {e}"))?;
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("malformed response")?;
    let status: u16 = std::str::from_utf8(&raw[..split])
        .ok()
        .and_then(|h| h.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|s| s.parse().ok()))
        .ok_or("bad status line")?;
    let body_bytes = &raw[split + 4..];
    let value: Value = if body_bytes.is_empty() { Value::Null } else { serde_json::from_slice(body_bytes).unwrap_or(Value::Null) };
    Ok((status, value))
}

struct Provider {
    label: String,
    owner: ValidatorKey,
    session: ValidatorKey,
    bond: String,
}

impl Provider {
    fn load(label: &str, seed_path: &str, bond: &str) -> Result<Self, String> {
        let hex = std::fs::read_to_string(seed_path).map_err(|e| format!("read {seed_path}: {e}"))?;
        let seed_bytes = decode_hex(hex.trim())?;
        let seed: [u8; 32] = seed_bytes.as_slice().try_into().map_err(|_| format!("{seed_path}: want a 32-byte seed"))?;
        let owner = ValidatorKey::from_seed(seed);
        // Session key: deterministically derived from the owner seed so a re-run reuses it.
        let mut session_seed = [0u8; 32];
        let derived = kaspa_hashes::blake2b_512_keyed(b"misaka-palw-bridge-v1/verify-session", &seed);
        session_seed.copy_from_slice(&derived.as_bytes()[..32]);
        Ok(Self { label: label.into(), owner, session: ValidatorKey::from_seed(session_seed), bond: bond.into() })
    }

    /// A signed request against the bridge: session key over route + body.
    ///
    /// The signed route must be the path the SERVER sees (`/palw/v1/challenges`), not the
    /// caller-side suffix (`/challenges`) — the bridge hashes `request.path`. Deriving it from
    /// the URL here keeps the two sides from drifting apart silently.
    fn signed(&self, bridge: &str, route: &str, body: &Value) -> Result<(u16, Value), String> {
        let url = format!("{bridge}{route}");
        let server_path = url
            .strip_prefix("http://")
            .and_then(|rest| rest.split_once('/'))
            .map(|(_, path)| format!("/{path}"))
            .ok_or_else(|| format!("cannot derive a server path from {url}"))?;
        let bytes = serde_json::to_vec(body).map_err(|e| e.to_string())?;
        let hash = request_signing_hash(&self.bond, &server_path, &body_digest(&bytes));
        let sig = self.session.sign_with_context(hash.as_byte_slice(), BRIDGE_REQUEST_MLDSA87_CONTEXT);
        http("POST", &url, Some(body), Some((&self.bond, &bytes_hex(&sig))))
    }

    fn registration(&self, network_id: u32, valid_from: u64, valid_until: u64) -> Result<Value, String> {
        let mut auth = PalwProviderSessionAuthorizationV1 {
            version: 1,
            network_id,
            provider_bond: parse_outpoint(&self.bond)?,
            owner_public_key: self.owner.public_key().to_vec(),
            session_public_key: self.session.public_key().to_vec(),
            valid_from_epoch: valid_from,
            valid_until_epoch: valid_until,
            authorization_nonce: Hash64::from_bytes([9u8; 64]),
            signature: Vec::new(),
        };
        let sig = self.owner.sign_with_context(auth.signing_hash().as_byte_slice(), PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT);
        auth.signature = sig.to_vec();
        Ok(json!({
            "bond_outpoint": self.bond,
            "owner_public_key_hex": bytes_hex(self.owner.public_key()),
            "session_authorization_hex": bytes_hex(&borsh::to_vec(&auth).map_err(|e| e.to_string())?),
        }))
    }
}

fn output_root_hex(ids: &[u32]) -> String {
    let mut h = blake2b_simd::Params::new().hash_length(32).to_state();
    for id in ids {
        h.update(&id.to_le_bytes());
    }
    h.finalize().as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

fn roots(route: &str) -> Value {
    json!({ "route": route, "kv": "bb22", "state": "cc33" })
}

struct Report(Vec<(String, bool, String)>);

impl Report {
    fn check(&mut self, name: &str, ok: bool, detail: impl Into<String>) {
        let detail = detail.into();
        println!("[{}] {name}{}", if ok { " PASS " } else { " FAIL " }, if detail.is_empty() { String::new() } else { format!(" — {detail}") });
        self.0.push((name.into(), ok, detail));
    }
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("palw-bridge-verify: {e}");
            eprintln!(
                "usage: palw-bridge-verify --bridge http://host:port/palw/v1 --network-id N \\\n  \
                 --provider A=<seed> =<txid:index> (x3, as LABEL=SEED=txid:index)"
            );
            std::process::exit(2);
        }
    };
    match run(&args) {
        Ok(report) => {
            let failed = report.0.iter().filter(|(_, ok, _)| !ok).count();
            println!("\n{} checks, {} failed", report.0.len(), failed);
            std::process::exit(if failed == 0 { 0 } else { 1 });
        }
        Err(e) => {
            eprintln!("palw-bridge-verify: {e}");
            std::process::exit(1);
        }
    }
}

fn run(args: &Args) -> Result<Report, String> {
    let mut report = Report(Vec::new());

    // ---- live chain facts --------------------------------------------------------------
    let (_, status) = http("GET", &format!("{}/status", args.bridge), None, None)?;
    let seams = &status["consensus_seams"];
    let live = seams["chain_facts_live"].as_bool().unwrap_or(false);
    let beacon_epoch = seams["beacon"]["epoch"].as_u64();
    let current_epoch = seams["beacon"]["current_epoch"].as_u64().unwrap_or(0);
    report.check(
        "chain facts are LIVE (not pinned)",
        live,
        seams["chain_facts"].as_str().unwrap_or("?").to_string(),
    );
    report.check(
        "live buried beacon sample available",
        beacon_epoch.is_some(),
        match beacon_epoch {
            Some(e) => format!("epoch {e}, seed {}…", &seams["beacon"]["seed"].as_str().unwrap_or("")[..16]),
            None => seams["beacon"]["error"].as_str().unwrap_or("absent").to_string(),
        },
    );
    if beacon_epoch.is_none() {
        return Ok(report);
    }

    // ---- seam 2: bonded registration ---------------------------------------------------
    let providers: Vec<Provider> = args
        .providers
        .iter()
        .map(|(l, s, b)| Provider::load(l, s, b))
        .collect::<Result<_, _>>()?;
    for p in &providers {
        let registration = p.registration(args.network_id, 0, current_epoch + 1_000)?;
        let (code, body) = http("POST", &format!("{}/providers", args.bridge), Some(&registration), None)?;
        report.check(
            &format!("seam 2: {} registers against its on-chain bond", p.label),
            code == 200,
            if code == 200 { body["credential"].as_str().unwrap_or("").chars().take(16).collect::<String>() } else { body.to_string() },
        );
    }
    // Negative: an unsigned consequential request must be refused.
    let (code, _) = http("POST", &format!("{}/challenges", args.bridge), Some(&json!({"provider_bond": providers[0].bond, "prompt_ids":[1], "max_new": 8})), None)?;
    report.check("seam 2: unsigned request refused", code != 200, format!("HTTP {code}"));

    // ---- seam 1: challenge leased from the LIVE beacon ---------------------------------
    let prompt: Vec<u32> = (1u32..=64).collect();
    let max_new = 256u32;
    let lease_body = json!({ "provider_bond": providers[0].bond, "prompt_ids": prompt, "max_new": max_new, "shape_id": 1 });
    let (code, lease) = providers[0].signed(&args.bridge, "/challenges", &lease_body)?;
    if code != 200 {
        report.check("seam 1: challenge lease", false, lease.to_string());
        return Ok(report);
    }
    let challenge_hex = lease["job_challenge_hex"].as_str().unwrap_or_default().to_string();
    report.check(
        "seam 1: challenge leased from the live beacon",
        lease["beacon_epoch"].as_u64() == beacon_epoch && !challenge_hex.is_empty(),
        format!("challenge {}… bound to beacon epoch {}", &challenge_hex[..16], lease["beacon_epoch"]),
    );
    // The lease must be reproducible AND prompt-bound.
    let (_, again) = providers[0].signed(&args.bridge, "/challenges", &lease_body)?;
    report.check("seam 1: re-lease is idempotent (no re-roll)", again["job_challenge_hex"] == lease["job_challenge_hex"], "");
    let other_body = json!({ "provider_bond": providers[0].bond, "prompt_ids": [9,9,9], "max_new": max_new, "shape_id": 1 });
    let (_, other) = providers[0].signed(&args.bridge, "/challenges", &other_body)?;
    report.check(
        "seam 1: a different prompt gets a different challenge",
        other["job_challenge_hex"] != lease["job_challenge_hex"],
        "",
    );

    // ---- seam 1+3: submit a job under the lease ----------------------------------------
    let challenge = misaka_palw_bridge::chain::parse_hash64(&challenge_hex)?;
    let output: Vec<u32> = (1000u32..1300).collect();
    let commitment = hash64_hex(&misaka_palw_bridge::challenge::salted_output_commitment(&output, &challenge));
    let job_id = format!("live-{}", &challenge_hex[..16]);
    let submit = json!({
        "job_id": job_id,
        "provider_id": providers[0].bond,
        "prompt_ids": prompt,
        "max_new": max_new,
        "output_root": output_root_hex(&output),
        "runtime_roots": roots("aa11"),
        "job_challenge": challenge_hex,
        "output_token_ids": output,
        "output_commitment": commitment,
    });
    let (code, submitted) = providers[0].signed(&args.bridge, "/jobs", &submit)?;
    report.check(
        "seam 1: job accepted under its lease + salted commitment",
        code == 200,
        if code == 200 { String::new() } else { submitted.to_string() },
    );
    if code != 200 {
        return Ok(report);
    }
    let obligations: Vec<DaObligation> = serde_json::from_value(submitted["da_obligations"].clone()).unwrap_or_default();
    report.check(
        "seam 3: DA obligations registered from the live beacon sample",
        !obligations.is_empty(),
        obligations
            .first()
            .map(|o| format!("chunk {} of {}, beacon epoch {}", o.chunk_index, o.commitment.chunk_count, o.beacon_epoch))
            .unwrap_or_default(),
    );

    // A submission whose commitment does not match its own output ids must be refused.
    let bad = json!({
        "job_id": format!("{job_id}-bad"), "provider_id": providers[0].bond, "prompt_ids": prompt,
        "max_new": max_new, "output_root": output_root_hex(&output), "runtime_roots": roots("aa11"),
        "job_challenge": challenge_hex, "output_token_ids": [7,7,7], "output_commitment": commitment,
    });
    let (code, _) = providers[0].signed(&args.bridge, "/jobs", &bad)?;
    report.check("seam 1: answer-swap under a leased challenge refused", code != 200, format!("HTTP {code}"));

    // ---- seam 3: answer the DA challenge with a real chunk proof ------------------------
    if let Some(obligation) = obligations.first() {
        let object = ChatContextObjectV4 {
            network_id: args.network_id,
            job_challenge: challenge,
            class_label: RUNTIME_CLASS_LABEL.to_vec(),
            max_new,
            prompt_token_ids: prompt.clone(),
            output_token_ids: output.clone(),
        };
        let bytes = object.encode()?;
        let rebuilt = DaCommitmentWire::from_commitment(&object.commitment()?);
        report.check("seam 3: provider rebuilds the same DA root", rebuilt == obligation.commitment, rebuilt.root_hex[..16].to_string());

        let (_, listed) = http(
            "GET",
            &format!("{}/da/obligations?provider_bond={}", args.bridge, providers[0].bond),
            None,
            None,
        )?;
        let challenged: Vec<DaObligation> = serde_json::from_value(listed["obligations"].clone()).unwrap_or_default();
        let target = challenged.iter().find(|o| o.obligation_id_hex == obligation.obligation_id_hex).cloned().unwrap_or_else(|| obligation.clone());
        let response = DaResponseWire::prove(&target, &bytes)?;
        let (code, answered) = providers[0].signed(&args.bridge, "/da/responses", &serde_json::to_value(&response).map_err(|e| e.to_string())?)?;
        report.check("seam 3: sampled chunk proof accepted", code == 200, if code == 200 { String::new() } else { answered.to_string() });

        // A tampered chunk must be refused by the node's own verifier.
        let mut tampered = response.clone();
        let mut chunk = decode_hex(&tampered.chunk_hex)?;
        chunk[0] ^= 0xff;
        tampered.chunk_hex = bytes_hex(&chunk);
        let (code, _) = providers[0].signed(&args.bridge, "/da/responses", &serde_json::to_value(&tampered).map_err(|e| e.to_string())?)?;
        report.check("seam 3: tampered chunk proof refused", code != 200, format!("HTTP {code}"));
    }

    // ---- k=2: the honest replica matches ------------------------------------------------
    let (_, assignments) = http("GET", &format!("{}/assignments?provider_id={}", args.bridge, providers[1].bond), None, None)?;
    let count = assignments["assignments"].as_array().map(|a| a.len()).unwrap_or(0);
    report.check("independence: the job is offered to B, never to its submitter", count >= 1, format!("{count} assignment(s)"));
    let (_, self_offer) = http("GET", &format!("{}/assignments?provider_id={}", args.bridge, providers[0].bond), None, None)?;
    report.check(
        "independence: submitter is not offered its own job",
        self_offer["assignments"].as_array().map(|a| a.is_empty()).unwrap_or(false),
        "",
    );

    let result = json!({
        "job_id": job_id, "provider_id": providers[1].bond,
        "output_root": output_root_hex(&output), "runtime_roots": roots("aa11"),
    });
    let (code, matched) = providers[1].signed(&args.bridge, "/replica-results", &result)?;
    report.check(
        "k=2: honest replica MATCHES through the node's run_replica_k2",
        code == 200 && matched["matched"].as_bool() == Some(true),
        matched.to_string().chars().take(80).collect::<String>(),
    );
    let (_, verdicts) = http("POST", &format!("{}/verdicts", args.bridge), Some(&json!({"job_ids": [job_id]})), None)?;
    let first = verdicts["verdicts"][0]["verdict"].as_str().unwrap_or("");
    report.check("k=2: verdict replica_matched delivered", first == "replica_matched", first.to_string());
    let (_, verdicts) = http("POST", &format!("{}/verdicts", args.bridge), Some(&json!({"job_ids": [job_id]})), None)?;
    report.check(
        "k=2: promotes to certified on the next observation",
        verdicts["verdicts"][0]["verdict"].as_str() == Some("certified"),
        "",
    );

    // ---- seam 4: a mismatch is arbitrated ----------------------------------------------
    let prompt2: Vec<u32> = (100u32..=160).collect();
    let lease2_body = json!({ "provider_bond": providers[0].bond, "prompt_ids": prompt2, "max_new": max_new, "shape_id": 1 });
    let (_, lease2) = providers[0].signed(&args.bridge, "/challenges", &lease2_body)?;
    let challenge2_hex = lease2["job_challenge_hex"].as_str().unwrap_or_default().to_string();
    let challenge2 = misaka_palw_bridge::chain::parse_hash64(&challenge2_hex)?;
    let out_a: Vec<u32> = (2000u32..2100).collect();
    let job2 = format!("live-dispute-{}", &challenge2_hex[..12]);
    let submit2 = json!({
        "job_id": job2, "provider_id": providers[0].bond, "prompt_ids": prompt2, "max_new": max_new,
        "output_root": output_root_hex(&out_a), "runtime_roots": roots("aa11"),
        "job_challenge": challenge2_hex, "output_token_ids": out_a,
        "output_commitment": hash64_hex(&misaka_palw_bridge::challenge::salted_output_commitment(&out_a, &challenge2)),
    });
    let (code, _) = providers[0].signed(&args.bridge, "/jobs", &submit2)?;
    if code != 200 {
        report.check("seam 4: second job accepted", false, format!("HTTP {code}"));
        return Ok(report);
    }
    http("GET", &format!("{}/assignments?provider_id={}", args.bridge, providers[1].bond), None, None)?;
    let out_b: Vec<u32> = vec![9999];
    let bad_result = json!({
        "job_id": job2, "provider_id": providers[1].bond,
        "output_root": output_root_hex(&out_b), "runtime_roots": roots("aa11"),
    });
    let (_, mismatched) = providers[1].signed(&args.bridge, "/replica-results", &bad_result)?;
    report.check(
        "seam 4: divergent replica is a MISMATCH, and opens a dispute",
        mismatched["matched"].as_bool() == Some(false) && !mismatched["dispute"].is_null(),
        "",
    );
    let dispute = &mismatched["dispute"];
    let auditor = dispute["auditor"].as_str().unwrap_or("");
    report.check(
        "seam 4: escalated and an unconflicted auditor drawn",
        dispute["escalated"].as_bool() == Some(true) && auditor == providers[2].bond,
        if auditor.is_empty() { "no auditor".into() } else { format!("auditor = {}", &auditor[..16]) },
    );
    let dispute_id = dispute["dispute_id_hex"].as_str().unwrap_or("").to_string();

    let (_, audits) = http("GET", &format!("{}/audits?auditor_bond={}", args.bridge, providers[2].bond), None, None)?;
    report.check(
        "seam 4: auditor sees the disputed job to replay",
        audits["audits"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "",
    );
    // A disputant must not be able to adjudicate.
    let verdict_body = json!({
        "dispute_id": dispute_id, "auditor_bond": providers[1].bond,
        "output_root": output_root_hex(&out_a), "runtime_roots": roots("aa11"),
    });
    let (code, _) = providers[1].signed(&args.bridge, "/audits/verdicts", &verdict_body)?;
    report.check("seam 4: a disputant cannot adjudicate its own dispute", code != 200, format!("HTTP {code}"));

    // The auditor's reference run agrees with A ⇒ B is the slash target.
    let verdict_body = json!({
        "dispute_id": dispute_id, "auditor_bond": providers[2].bond,
        "output_root": output_root_hex(&out_a), "runtime_roots": roots("aa11"),
    });
    let (code, evidence) = providers[2].signed(&args.bridge, "/audits/verdicts", &verdict_body)?;
    let targets = evidence["slash_targets"].as_array().cloned().unwrap_or_default();
    report.check(
        "seam 4: attribution names the deviating provider",
        code == 200 && evidence["verdict"] == "slash_b" && targets.first().and_then(|t| t.as_str()) == Some(providers[1].bond.as_str()),
        format!("verdict={} targets={}", evidence["verdict"], targets.len()),
    );
    report.check(
        "seam 4: slash evidence is anchored to a journal position",
        evidence["journal_root_hex"].as_str().map(|r| !r.is_empty()).unwrap_or(false),
        evidence["journal_root_hex"].as_str().unwrap_or("").chars().take(16).collect::<String>(),
    );

    Ok(report)
}
