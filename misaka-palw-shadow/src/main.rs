//! `palw-shadow` — the ADR-0029 §5 Stage-0 drill: submitter, attester, watcher, reporter.
//!
//! Everything here is telemetry plumbing over objects other modules own: carriage bodies and
//! their stateless validation from `kaspa_consensus_core::palw_carriage`, schedules/panels/
//! ledger from `palw_schedule`, execution from the pinned `palw-worker` subprocess, funding and
//! signing from `kaspa_pq_validator_core::ValidatorKey`. Nothing consensus-affecting happens in
//! this binary, and nothing here may ever ground an offense against a third party — Stage-0
//! carriage is measurement-grade by the ADR's own premise.
//!
//! Modes:
//! * `keygen`             — a DRILL identity (fresh ML-DSA-87 seed). Never a production
//!                          validator key: the drill refuses nothing statelessly, so keeping the
//!                          namespaces disjoint is a rule, not a mechanism.
//! * `submit-commitment`  — run one v2 legs job under a DRILL network id, wrap it as carriage
//!                          kind 0x01 (composite), sign, fund, submit. `--offline-out` builds
//!                          and self-validates everything but touches no node.
//! * `attest`             — follow a watcher's event log; when this identity is on a job's
//!                          panel, replay the job and submit kind 0x02 on a root match. Induced
//!                          negatives (`--noshow-nth`, `--late-nth/--late-secs`) are drill
//!                          schedule, not misbehavior. A root MISMATCH is never auto-filed —
//!                          it is logged loudly and left for a human (ADR-0029 §5).
//! * `watch`              — follow the node's selected chain (blocks for bodies, virtual chain
//!                          for acceptance), extract valid carriages, append an event log.
//! * `report`             — replay the event log into `PalwShadowLedgerV1` and print the §12
//!                          artifact JSON.

use clap::{Parser, Subcommand};
use kaspa_addresses::Prefix;
use kaspa_consensus_core::config::params::BlockrateParams;
use kaspa_consensus_core::palw_carriage::{
    PALW_CARRIAGE_MLDSA87_COMMITMENT_CONTEXT, PALW_CARRIAGE_VERSION_V1, PalwAttestationCarriageV1, PalwCarriageV1,
    PalwCommitmentCarriageV1, decode_palw_carriage_v1, encode_palw_carriage_v1, validate_palw_carriage_v1,
};
use kaspa_consensus_core::palw_schedule::{
    PalwDutyObservationV1, PalwPanelCandidateV1, PalwScheduleParamsV1, PalwShadowJobObservationV1, PalwShadowLedgerV1,
    job_schedule_v1, select_replay_panel_v1,
};
use kaspa_consensus_core::palw_slash::{PALW_S_MLDSA87_ATTESTATION_CONTEXT, PALW_S_OBJECT_VERSION_V1, PalwExecutionAttestationV1};
use kaspa_consensus_core::palw_v2::{PalwJobEnvelopeV2, decode_framed_borsh, read_framed, write_framed};
use kaspa_consensus_core::tx::{Transaction, TransactionOutpoint};
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{RpcHash, RpcTransaction};
use kaspa_wrpc_client::prelude::{ConnectOptions, ConnectStrategy};
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

// ---------------------------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "palw-shadow", about = "MISAKA PALW Stage-0 shadow drill (ADR-0029 §5) — telemetry only")]
struct Cli {
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand)]
enum Mode {
    /// Generate a DRILL ML-DSA-87 identity seed (never reuse a production validator key).
    Keygen {
        /// Seed file to write (0600).
        #[arg(long)]
        out: PathBuf,
        /// Address prefix for the printed funding address (testnet/simnet/devnet).
        #[arg(long, default_value = "testnet")]
        prefix: String,
    },
    /// Execute one v2 legs job under the drill namespace and submit it as carriage kind 0x01.
    SubmitCommitment {
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        worker: PathBuf,
        /// Golden job name used as the envelope source (`v2-golden-envelope`).
        #[arg(long, default_value = "golden-probe-12tok-d16")]
        name: String,
        /// Override the decode budget (the envelope's max_context is raised to 4096 with it).
        #[arg(long)]
        decode: Option<u32>,
        /// Drill namespace written into the envelope BEFORE execution.
        #[arg(long, default_value = "misaka-palw-drill/v1")]
        network_id: String,
        /// wRPC endpoint (host:port, borsh). Unused with --offline-out.
        #[arg(long, default_value = "127.0.0.1:17110")]
        rpc: String,
        #[arg(long, default_value_t = 100_000)]
        fee: u64,
        /// Coinbase maturity used when filtering funding UTXOs.
        #[arg(long, default_value_t = 30_000)]
        coinbase_maturity: u64,
        #[arg(long, default_value = "testnet")]
        prefix: String,
        /// Build, sign and self-validate everything, write artifacts here, submit nothing.
        #[arg(long)]
        offline_out: Option<PathBuf>,
    },
    /// Follow a watcher's event log and answer this identity's assigned duties.
    Attest {
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        worker: PathBuf,
        #[arg(long)]
        state_dir: PathBuf,
        /// Roster JSON: {"q":2,"delta_bind":..,"candidates":[{"validator_id":"..","class":".."}]}
        #[arg(long)]
        roster: PathBuf,
        #[arg(long, default_value = "127.0.0.1:17110")]
        rpc: String,
        #[arg(long, default_value_t = 100_000)]
        fee: u64,
        #[arg(long, default_value_t = 30_000)]
        coinbase_maturity: u64,
        #[arg(long, default_value = "testnet")]
        prefix: String,
        /// Skip (stay silent on) the Nth duty this process would otherwise answer. 1-based.
        #[arg(long)]
        noshow_nth: Option<u64>,
        /// Delay the Nth duty's attestation by --late-secs. 1-based.
        #[arg(long)]
        late_nth: Option<u64>,
        #[arg(long, default_value_t = 0)]
        late_secs: u64,
        #[arg(long, default_value_t = 10)]
        poll_secs: u64,
        /// Exit after this many duties have been handled (0 = run forever).
        #[arg(long, default_value_t = 0)]
        max_duties: u64,
    },
    /// Follow the node and append carriage acceptance events to the state dir.
    Watch {
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long, default_value = "127.0.0.1:17110")]
        rpc: String,
        #[arg(long, default_value_t = 5)]
        poll_secs: u64,
        /// Exit after this many poll rounds (0 = run forever). For smoke tests.
        #[arg(long, default_value_t = 0)]
        max_rounds: u64,
    },
    /// Replay the event log into the shadow ledger and print the §12 artifact JSON.
    Report {
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        roster: PathBuf,
        /// Window parameter set: deci (10 s blocks) or two-minute (120 s blocks).
        #[arg(long, default_value = "two-minute")]
        params: String,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.mode {
        Mode::Keygen { out, prefix } => keygen(&out, &prefix),
        Mode::SubmitCommitment { key, worker, name, decode, network_id, rpc, fee, coinbase_maturity, prefix, offline_out } => {
            submit_commitment(&key, &worker, &name, decode, &network_id, &rpc, fee, coinbase_maturity, &prefix, offline_out)
        }
        Mode::Attest {
            key,
            worker,
            state_dir,
            roster,
            rpc,
            fee,
            coinbase_maturity,
            prefix,
            noshow_nth,
            late_nth,
            late_secs,
            poll_secs,
            max_duties,
        } => attest(
            &key,
            &worker,
            &state_dir,
            &roster,
            &rpc,
            fee,
            coinbase_maturity,
            &prefix,
            noshow_nth,
            late_nth,
            late_secs,
            poll_secs,
            max_duties,
        ),
        Mode::Watch { state_dir, rpc, poll_secs, max_rounds } => watch(&state_dir, &rpc, poll_secs, max_rounds),
        Mode::Report { state_dir, roster, params } => report(&state_dir, &roster, &params),
    };
    if let Err(e) = result {
        eprintln!("[palw-shadow] FATAL: {e}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------------

fn hex64(h: &Hash64) -> String {
    h.as_byte_slice().iter().map(|b| format!("{b:02x}")).collect()
}

fn parse_hash64(s: &str) -> Result<Hash64, String> {
    let mut bytes = [0u8; 64];
    if s.len() != 128 {
        return Err(format!("expected 128 hex chars, got {}", s.len()));
    }
    faster_hex::hex_decode(s.as_bytes(), &mut bytes).map_err(|e| format!("bad hex: {e}"))?;
    Ok(Hash64::from_bytes(bytes))
}

fn parse_prefix(s: &str) -> Result<Prefix, String> {
    match s {
        "mainnet" => Ok(Prefix::Mainnet),
        "testnet" => Ok(Prefix::Testnet),
        "simnet" => Ok(Prefix::Simnet),
        "devnet" => Ok(Prefix::Devnet),
        other => Err(format!("unknown prefix {other:?}")),
    }
}

fn load_key(path: &Path) -> Result<ValidatorKey, String> {
    let seed = load_validator_seed(&path.display().to_string())?;
    Ok(ValidatorKey::from_seed(seed))
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime")
}

async fn connect(node_rpc: &str) -> Result<KaspaRpcClient, String> {
    let url = format!("ws://{node_rpc}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None).map_err(|e| format!("wRPC client: {e}"))?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_millis(5_000)),
        strategy: ConnectStrategy::Retry,
        ..Default::default()
    };
    client.connect(Some(options)).await.map_err(|e| format!("connect {url}: {e}"))?;
    Ok(client)
}

/// Drives one `palw-worker` mode over the framed-stdio contract: writes `input` as one frame if
/// given, reads one frame back if `expect_output`.
fn drive_worker(worker: &Path, args: &[&str], input: Option<&[u8]>, expect_output: bool) -> Result<Vec<u8>, String> {
    let mut child = Command::new(worker)
        .args(args)
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("spawn {}: {e}", worker.display()))?;
    if let Some(bytes) = input {
        let mut stdin = child.stdin.take().expect("piped");
        write_framed(&mut stdin, bytes).map_err(|e| format!("write frame to worker: {e}"))?;
        drop(stdin);
    }
    let mut stdout = child.stdout.take().expect("piped");
    let out = if expect_output {
        read_framed(&mut stdout, kaspa_consensus_core::palw_v2::PALW_V2_MAX_FRAME_BYTES)
            .map_err(|e| format!("read frame from worker: {e}"))?
    } else {
        Vec::new()
    };
    let status = child.wait().map_err(|e| format!("wait worker: {e}"))?;
    if !status.success() {
        return Err(format!("worker exited with {status}"));
    }
    Ok(out)
}

/// Runs the job and returns (result, envelope) — envelope generation, drill patching and the
/// legs execution in one place so submit and attest cannot drift.
fn run_legs_job(worker: &Path, envelope: &PalwJobEnvelopeV2) -> Result<kaspa_consensus_core::palw_legs::PalwLegsJobResultV1, String> {
    let bytes = borsh::to_vec(envelope).expect("borsh envelope");
    let out = drive_worker(worker, &["--mode", "v2-legs-job"], Some(&bytes), true)?;
    let result: kaspa_consensus_core::palw_legs::PalwLegsJobResultV1 =
        decode_framed_borsh(&out).map_err(|e| format!("decode legs result: {e}"))?;
    result.validate_coherence().map_err(|e| format!("incoherent legs result: {e}"))?;
    Ok(result)
}

/// The drill envelope: the named golden job's envelope with the drill namespace and an optional
/// decode override, patched BEFORE execution so every derived context is namespaced.
fn drill_envelope(worker: &Path, name: &str, network_id: &str, decode: Option<u32>) -> Result<PalwJobEnvelopeV2, String> {
    let frame = drive_worker(worker, &["--mode", "v2-golden-envelope", "--name", name], None, true)?;
    let mut envelope: PalwJobEnvelopeV2 = decode_framed_borsh(&frame).map_err(|e| format!("decode envelope: {e}"))?;
    envelope.network_id = network_id.as_bytes().to_vec();
    if let Some(d) = decode {
        envelope.exact_decode_tokens = d;
        envelope.max_context_tokens = 4096;
    }
    envelope.validate_shape(envelope.max_context_tokens).map_err(|e| format!("patched envelope invalid: {e}"))?;
    Ok(envelope)
}

async fn pick_funding(
    client: &KaspaRpcClient,
    key: &ValidatorKey,
    prefix: Prefix,
    fee: u64,
    coinbase_maturity: u64,
) -> Result<(TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry), String> {
    let virtual_daa = client.get_block_dag_info().await.map_err(|e| format!("getBlockDagInfo: {e}"))?.virtual_daa_score;
    let address = key.funding_address(prefix);
    let mut cursor = String::new();
    let mut best: Option<(TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)> = None;
    loop {
        let page = client
            .get_utxos_by_address_page(address.clone(), cursor, 1_000)
            .await
            .map_err(|e| format!("getUtxosByAddressPage (does the node run --utxoindex?): {e}"))?;
        for e in page.entries {
            let outpoint = TransactionOutpoint::from(e.outpoint);
            let entry = kaspa_consensus_core::tx::UtxoEntry::from(e.utxo_entry);
            if entry.is_coinbase && virtual_daa < entry.block_daa_score.saturating_add(coinbase_maturity) {
                continue;
            }
            if entry.amount <= fee {
                continue;
            }
            if best.as_ref().map(|(_, b)| entry.amount > b.amount).unwrap_or(true) {
                best = Some((outpoint, entry));
            }
        }
        if page.next_cursor.is_empty() {
            break;
        }
        cursor = page.next_cursor;
    }
    best.ok_or_else(|| format!("no spendable UTXO above the fee at {address} — fund the drill wallet first (runbook §2)"))
}

async fn submit_carriage_tx(
    client: &KaspaRpcClient,
    key: &ValidatorKey,
    prefix: Prefix,
    payload: Vec<u8>,
    fee: u64,
    coinbase_maturity: u64,
) -> Result<String, String> {
    let (outpoint, entry) = pick_funding(client, key, prefix, fee, coinbase_maturity).await?;
    let tx = key.build_funded_native_carriage_tx(payload, outpoint, &entry, fee)?;
    let id = client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| format!("submitTransaction: {e}"))?;
    Ok(id.to_string())
}

// ---------------------------------------------------------------------------------------------
// keygen
// ---------------------------------------------------------------------------------------------

fn keygen(out: &Path, prefix: &str) -> Result<(), String> {
    use rand::RngCore;
    let prefix = parse_prefix(prefix)?;
    if out.exists() {
        return Err(format!("{} already exists — refusing to overwrite a key file", out.display()));
    }
    let mut seed = [0u8; kaspa_pq_validator_core::VALIDATOR_SEED_LEN];
    rand::thread_rng().fill_bytes(&mut seed);
    let hex: String = seed.iter().map(|b| format!("{b:02x}")).collect();
    let mut file = std::fs::File::create(out).map_err(|e| format!("create {}: {e}", out.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600)).map_err(|e| format!("chmod: {e}"))?;
    }
    file.write_all(hex.as_bytes()).map_err(|e| format!("write: {e}"))?;
    let key = ValidatorKey::from_seed(seed);
    println!("drill identity written to {}", out.display());
    println!("validator_id     = {}", hex64(&key.validator_id));
    println!("funding address  = {}", key.funding_address(prefix));
    println!("NOTE: a DRILL identity. Never point this tool at a production validator seed.");
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// submit-commitment
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn submit_commitment(
    key_path: &Path,
    worker: &Path,
    name: &str,
    decode: Option<u32>,
    network_id: &str,
    rpc: &str,
    fee: u64,
    coinbase_maturity: u64,
    prefix: &str,
    offline_out: Option<PathBuf>,
) -> Result<(), String> {
    let prefix = parse_prefix(prefix)?;
    let key = load_key(key_path)?;
    let envelope = drill_envelope(worker, name, network_id, decode)?;
    eprintln!("[palw-shadow] executing drill job (network {network_id:?}, decode {})", envelope.exact_decode_tokens);
    let result = run_legs_job(worker, &envelope)?;
    let binding = result.binding;
    let mut carriage = PalwCommitmentCarriageV1 {
        version: PALW_CARRIAGE_VERSION_V1,
        envelope,
        committed_form: 1,
        committed_root: binding.committed_execution_root,
        binding: Some(binding),
        validator_id: key.validator_id,
        bond_outpoint: TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_bytes([0u8; 64]), 0),
        signature: Vec::new(),
    };
    let message = carriage.message();
    carriage.signature = key.sign_with_context(message.as_bytes().as_slice(), PALW_CARRIAGE_MLDSA87_COMMITMENT_CONTEXT).to_vec();
    let wrapped = PalwCarriageV1::Commitment(carriage);
    validate_palw_carriage_v1(&wrapped).map_err(|e| format!("self-validation failed (bug): {e}"))?;
    let payload = encode_palw_carriage_v1(&wrapped);
    let root = match &wrapped {
        PalwCarriageV1::Commitment(c) => c.committed_root,
        _ => unreachable!(),
    };
    eprintln!("[palw-shadow] commitment root {}…, payload {} bytes", &hex64(&root)[..16], payload.len());

    if let Some(out) = offline_out {
        std::fs::create_dir_all(&out).map_err(|e| format!("mkdir {}: {e}", out.display()))?;
        std::fs::write(out.join("commitment-payload.bin"), &payload).map_err(|e| e.to_string())?;
        let summary = serde_json::json!({
            "schema": "misaka.palw-shadow.offline-commitment.v1",
            "committed_root": hex64(&root),
            "payload_bytes": payload.len(),
            "validator_id": hex64(&key.validator_id),
            "stateless_valid": true,
        });
        std::fs::write(out.join("commitment-summary.json"), serde_json::to_string_pretty(&summary).unwrap())
            .map_err(|e| e.to_string())?;
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        return Ok(());
    }

    rt().block_on(async {
        let client = connect(rpc).await?;
        let tx_id = submit_carriage_tx(&client, &key, prefix, payload, fee, coinbase_maturity).await?;
        println!("submitted commitment tx {tx_id} (root {}…)", &hex64(&root)[..16]);
        Ok(())
    })
}

// ---------------------------------------------------------------------------------------------
// The event log (watch writes, attest and report read)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum Event {
    ChainAdd {
        block: String,
        daa: u64,
    },
    ChainRemove {
        block: String,
    },
    Carriage {
        block: String,
        daa: u64,
        tx: String,
        #[serde(flatten)]
        body: CarriageEvent,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CarriageEvent {
    Commitment {
        root: String,
        class: String,
        executor: String,
        network_id: String,
        /// The logits root the credit gate matches attestations against (composite: from the
        /// binding; bare: the committed root itself).
        logits_root: String,
        envelope_hex: String,
    },
    Attestation {
        commitment_root: String,
        attester: String,
        attested_logits_root: String,
    },
    OpeningCall {
        root: String,
        openings: usize,
    },
    OpeningAnswer {
        call_tx: String,
    },
    Refutation {
        target_root: String,
    },
}

fn events_path(state_dir: &Path) -> PathBuf {
    state_dir.join("events.jsonl")
}

fn append_event(state_dir: &Path, event: &Event) -> Result<(), String> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(events_path(state_dir))
        .map_err(|e| format!("open events log: {e}"))?;
    let line = serde_json::to_string(event).expect("serializable event");
    writeln!(f, "{line}").map_err(|e| format!("append event: {e}"))?;
    Ok(())
}

fn read_events(state_dir: &Path, from_line: usize) -> Result<(Vec<Event>, usize), String> {
    let path = events_path(state_dir);
    if !path.exists() {
        return Ok((Vec::new(), from_line));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read events log: {e}"))?;
    let mut events = Vec::new();
    let mut count = 0usize;
    for (i, line) in text.lines().enumerate() {
        count = i + 1;
        if i < from_line || line.trim().is_empty() {
            continue;
        }
        let event: Event = serde_json::from_str(line).map_err(|e| format!("events.jsonl line {}: {e}", i + 1))?;
        events.push(event);
    }
    Ok((events, count))
}

fn carriage_event_of(carriage: &PalwCarriageV1) -> CarriageEvent {
    match carriage {
        PalwCarriageV1::Commitment(c) => {
            let logits_root = match (&c.binding, c.committed_form) {
                (Some(binding), 1) => binding.full_logits_trace_root,
                _ => c.committed_root,
            };
            CarriageEvent::Commitment {
                root: hex64(&c.committed_root),
                class: hex64(&c.envelope.runtime_class_id),
                executor: hex64(&c.validator_id),
                network_id: String::from_utf8_lossy(&c.envelope.network_id).into_owned(),
                logits_root: hex64(&logits_root),
                envelope_hex: {
                    let bytes = borsh::to_vec(&c.envelope).expect("borsh envelope");
                    let mut s = vec![0u8; bytes.len() * 2];
                    faster_hex::hex_encode(&bytes, &mut s).expect("hex");
                    String::from_utf8(s).expect("hex utf8")
                },
            }
        }
        PalwCarriageV1::Attestation(a) => CarriageEvent::Attestation {
            commitment_root: hex64(&a.commitment_root),
            attester: hex64(&a.attester_id),
            attested_logits_root: hex64(&a.attestation.full_logits_trace_root),
        },
        PalwCarriageV1::OpeningCall(c) => CarriageEvent::OpeningCall {
            root: hex64(&c.call.request.committed_execution_root),
            openings: c.call.request.activation.len() + c.call.request.checkpoint_indices.len(),
        },
        PalwCarriageV1::OpeningAnswer(a) => CarriageEvent::OpeningAnswer { call_tx: a.call_tx_id.to_string() },
        PalwCarriageV1::Refutation(r) => CarriageEvent::Refutation {
            target_root: match &r.evidence {
                kaspa_consensus_core::palw_carriage::PalwCarriedEvidenceV1::Legs(l) => hex64(&l.binding.committed_execution_root),
                kaspa_consensus_core::palw_carriage::PalwCarriedEvidenceV1::Summary(s) => hex64(&s.committed_trace_root),
            },
        },
    }
}

// ---------------------------------------------------------------------------------------------
// watch
// ---------------------------------------------------------------------------------------------

fn watch(state_dir: &Path, rpc: &str, poll_secs: u64, max_rounds: u64) -> Result<(), String> {
    std::fs::create_dir_all(state_dir).map_err(|e| format!("mkdir {}: {e}", state_dir.display()))?;
    rt().block_on(async {
        let client = connect(rpc).await?;
        let dag = client.get_block_dag_info().await.map_err(|e| format!("getBlockDagInfo: {e}"))?;
        // Rebuild dedup state from the existing log so restarts append, never duplicate.
        let (existing, _) = read_events(state_dir, 0)?;
        let mut seen_accepted: HashSet<(String, String)> = HashSet::new();
        let mut seen_chain: HashSet<String> = HashSet::new();
        let mut sync_hash: RpcHash = dag.pruning_point_hash;
        for event in &existing {
            match event {
                Event::ChainAdd { block, .. } => {
                    seen_chain.insert(block.clone());
                    if let Ok(h) = block.parse::<RpcHash>() {
                        sync_hash = h;
                    }
                }
                Event::ChainRemove { block } => {
                    seen_chain.remove(block);
                }
                Event::Carriage { block, tx, .. } => {
                    seen_accepted.insert((block.clone(), tx.clone()));
                }
            }
        }
        eprintln!("[palw-shadow] watch: starting from {} ({} prior events)", sync_hash, existing.len());

        // Bodies: tx-id → carriage, populated by a block scan from the pruning point (bounded
        // by the pruning horizon; acceptance below joins ids against it).
        let mut bodies: HashMap<String, PalwCarriageV1> = HashMap::new();
        let mut low_hash: Option<RpcHash> = Some(dag.pruning_point_hash);
        let mut rounds = 0u64;
        loop {
            // (a) bodies scan
            loop {
                let resp = client.get_blocks(low_hash, true, true).await.map_err(|e| format!("getBlocks: {e}"))?;
                let n = resp.blocks.len();
                for block in &resp.blocks {
                    for rpc_tx in &block.transactions {
                        let Ok(tx) = Transaction::try_from(rpc_tx.clone()) else { continue };
                        if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE {
                            continue;
                        }
                        match decode_palw_carriage_v1(&tx.payload) {
                            Ok(Some(carriage)) => {
                                if validate_palw_carriage_v1(&carriage).is_ok() {
                                    bodies.insert(tx.id().to_string(), carriage);
                                } else {
                                    eprintln!("[palw-shadow] watch: tx {} claims the magic, fails stateless validation", tx.id());
                                }
                            }
                            Ok(None) => {}
                            Err(e) => eprintln!("[palw-shadow] watch: tx {} broken claimant: {e}", tx.id()),
                        }
                    }
                }
                if let Some(last) = resp.block_hashes.last() {
                    low_hash = Some(*last);
                }
                if n <= 1 {
                    break; // caught up (the response echoes the low hash)
                }
            }
            // (b) acceptance walk
            let chain = client
                .get_virtual_chain_from_block(sync_hash, true, None)
                .await
                .map_err(|e| format!("getVirtualChainFromBlock: {e}"))?;
            for removed in &chain.removed_chain_block_hashes {
                let key = removed.to_string();
                if seen_chain.remove(&key) {
                    append_event(state_dir, &Event::ChainRemove { block: key })?;
                }
            }
            let accepted_by_block: HashMap<String, Vec<String>> = chain
                .accepted_transaction_ids
                .iter()
                .map(|a| (a.accepting_block_hash.to_string(), a.accepted_transaction_ids.iter().map(|t| t.to_string()).collect()))
                .collect();
            for added in &chain.added_chain_block_hashes {
                let key = added.to_string();
                let block = client.get_block(*added, false).await.map_err(|e| format!("getBlock {added}: {e}"))?;
                let daa = block.header.daa_score;
                if seen_chain.insert(key.clone()) {
                    append_event(state_dir, &Event::ChainAdd { block: key.clone(), daa })?;
                }
                if let Some(tx_ids) = accepted_by_block.get(&key) {
                    for tx_id in tx_ids {
                        if let Some(carriage) = bodies.get(tx_id) {
                            let dedup = (key.clone(), tx_id.clone());
                            if seen_accepted.insert(dedup) {
                                let event =
                                    Event::Carriage { block: key.clone(), daa, tx: tx_id.clone(), body: carriage_event_of(carriage) };
                                append_event(state_dir, &event)?;
                                eprintln!("[palw-shadow] watch: accepted {} at daa {daa} (tx {tx_id})", kind_name(carriage));
                            }
                        }
                    }
                }
                sync_hash = *added;
            }
            rounds += 1;
            if max_rounds != 0 && rounds >= max_rounds {
                eprintln!("[palw-shadow] watch: max rounds reached, exiting");
                return Ok(());
            }
            tokio::time::sleep(Duration::from_secs(poll_secs)).await;
        }
    })
}

fn kind_name(carriage: &PalwCarriageV1) -> &'static str {
    match carriage {
        PalwCarriageV1::Commitment(_) => "commitment",
        PalwCarriageV1::Attestation(_) => "attestation",
        PalwCarriageV1::OpeningCall(_) => "opening-call",
        PalwCarriageV1::OpeningAnswer(_) => "opening-answer",
        PalwCarriageV1::Refutation(_) => "refutation",
    }
}

// ---------------------------------------------------------------------------------------------
// attest
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct Roster {
    #[serde(default = "default_q")]
    q: usize,
    #[serde(default = "default_delta_bind")]
    delta_bind: u64,
    candidates: Vec<RosterEntry>,
}

#[derive(Deserialize)]
struct RosterEntry {
    validator_id: String,
    class: String,
}

fn default_q() -> usize {
    2
}
fn default_delta_bind() -> u64 {
    10
}

fn load_roster(path: &Path) -> Result<(Roster, Vec<PalwPanelCandidateV1>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read roster: {e}"))?;
    let roster: Roster = serde_json::from_str(&text).map_err(|e| format!("roster json: {e}"))?;
    let mut candidates = Vec::new();
    for entry in &roster.candidates {
        candidates.push(PalwPanelCandidateV1 {
            validator_id: parse_hash64(&entry.validator_id)?,
            runtime_class_id: parse_hash64(&entry.class)?,
            bonded: true,
            frozen: false,
        });
    }
    Ok((roster, candidates))
}

#[allow(clippy::too_many_arguments)]
fn attest(
    key_path: &Path,
    worker: &Path,
    state_dir: &Path,
    roster_path: &Path,
    rpc: &str,
    fee: u64,
    coinbase_maturity: u64,
    prefix: &str,
    noshow_nth: Option<u64>,
    late_nth: Option<u64>,
    late_secs: u64,
    poll_secs: u64,
    max_duties: u64,
) -> Result<(), String> {
    let prefix = parse_prefix(prefix)?;
    let key = load_key(key_path)?;
    let (roster, candidates) = load_roster(roster_path)?;
    eprintln!("[palw-shadow] attest: identity {}…, roster of {}, q={}", &hex64(&key.validator_id)[..16], candidates.len(), roster.q);
    let mut cursor = 0usize;
    let mut chain: Vec<(String, u64)> = Vec::new(); // (hash, daa) in add order
    let mut duties_seen = 0u64;
    let mut handled: HashSet<String> = HashSet::new(); // commitment roots this process answered
    let mut pending: Vec<(String, u64, String, String, String)> = Vec::new(); // (root, commit_daa, class, executor, envelope_hex)
    loop {
        let (events, next) = read_events(state_dir, cursor)?;
        cursor = next;
        for event in events {
            match event {
                Event::ChainAdd { block, daa } => chain.push((block, daa)),
                Event::ChainRemove { block } => chain.retain(|(h, _)| *h != block),
                Event::Carriage { daa, body, .. } => {
                    if let CarriageEvent::Commitment { root, class, executor, envelope_hex, .. } = body {
                        pending.push((root, daa, class, executor, envelope_hex));
                    }
                }
            }
        }
        let mut still_pending = Vec::new();
        for (root, commit_daa, class, executor, envelope_hex) in pending.drain(..) {
            if handled.contains(&root) {
                continue;
            }
            // Anchor: the first chain block at or past commit_daa + delta_bind.
            let anchor = chain.iter().find(|(_, daa)| *daa >= commit_daa + roster.delta_bind).cloned();
            let Some((anchor_hash, _)) = anchor else {
                still_pending.push((root, commit_daa, class, executor, envelope_hex));
                continue;
            };
            let root_h = parse_hash64(&root)?;
            let executor_h = parse_hash64(&executor)?;
            let class_h = parse_hash64(&class)?;
            let anchor_h = parse_hash64(&anchor_hash)?;
            let panel = select_replay_panel_v1(&root_h, &executor_h, &anchor_h, &class_h, &candidates, roster.q);
            if !panel.contains(&key.validator_id) {
                handled.insert(root);
                continue;
            }
            duties_seen += 1;
            eprintln!("[palw-shadow] attest: duty #{duties_seen} for root {}…", &root[..16]);
            if noshow_nth == Some(duties_seen) {
                eprintln!("[palw-shadow] attest: DRILL no-show on duty #{duties_seen} — staying silent by schedule");
                handled.insert(root);
                continue;
            }
            let mut envelope_bytes = vec![0u8; envelope_hex.len() / 2];
            faster_hex::hex_decode(envelope_hex.as_bytes(), &mut envelope_bytes).map_err(|e| format!("envelope hex: {e}"))?;
            let envelope: PalwJobEnvelopeV2 = borsh::from_slice(&envelope_bytes).map_err(|e| format!("envelope decode: {e}"))?;
            let result = run_legs_job(worker, &envelope)?;
            let our_logits_root = result.result.projection.full_logits_trace_root;
            let our_composite = result.binding.committed_execution_root;
            if our_composite != root_h {
                eprintln!(
                    "[palw-shadow] attest: ROOT MISMATCH on {}… (ours {}…) — NOT auto-filing anything; a human looks at this (ADR-0029 §5)",
                    &root[..16],
                    &hex64(&our_composite)[..16]
                );
                handled.insert(root);
                continue;
            }
            if late_nth == Some(duties_seen) && late_secs > 0 {
                eprintln!("[palw-shadow] attest: DRILL late answer on duty #{duties_seen} — sleeping {late_secs}s by schedule");
                std::thread::sleep(Duration::from_secs(late_secs));
            }
            let context_hash =
                kaspa_consensus_core::palw_v2::PalwJobContextV2::from_envelope(&envelope, result.binding.job_context.tokenizer_id)
                    .context_hash();
            let mut attestation = PalwExecutionAttestationV1 {
                version: PALW_S_OBJECT_VERSION_V1,
                executor_id: key.validator_id,
                job_context_hash: context_hash,
                full_logits_trace_root: our_logits_root,
                signature: Vec::new(),
            };
            let message = attestation.message(&envelope.network_id);
            attestation.signature = key.sign_with_context(message.as_bytes().as_slice(), PALW_S_MLDSA87_ATTESTATION_CONTEXT).to_vec();
            let carriage = PalwCarriageV1::Attestation(PalwAttestationCarriageV1 {
                version: PALW_CARRIAGE_VERSION_V1,
                commitment_root: root_h,
                attestation,
                attester_id: key.validator_id,
                bond_outpoint: TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_bytes([0u8; 64]), 0),
            });
            validate_palw_carriage_v1(&carriage).map_err(|e| format!("self-validation failed (bug): {e}"))?;
            let payload = encode_palw_carriage_v1(&carriage);
            let tx_id = rt().block_on(async {
                let client = connect(rpc).await?;
                submit_carriage_tx(&client, &key, prefix, payload, fee, coinbase_maturity).await
            })?;
            eprintln!("[palw-shadow] attest: submitted attestation tx {tx_id} for root {}…", &root[..16]);
            handled.insert(root);
            if max_duties != 0 && duties_seen >= max_duties {
                eprintln!("[palw-shadow] attest: max duties reached, exiting");
                return Ok(());
            }
        }
        pending = still_pending;
        std::thread::sleep(Duration::from_secs(poll_secs));
    }
}

// ---------------------------------------------------------------------------------------------
// report
// ---------------------------------------------------------------------------------------------

fn report(state_dir: &Path, roster_path: &Path, params_name: &str) -> Result<(), String> {
    let params = match params_name {
        "deci" => PalwScheduleParamsV1::stage1_defaults_deci_bps(),
        "two-minute" => PalwScheduleParamsV1::stage1_defaults_two_minute_bps(),
        other => return Err(format!("unknown --params {other:?} (deci | two-minute)")),
    };
    params
        .validate(&if params_name == "deci" { BlockrateParams::new_deci_bps() } else { BlockrateParams::new_two_minute_bps() })
        .map_err(|e| format!("params do not validate: {e}"))?;
    let (roster, candidates) = load_roster(roster_path)?;
    let (events, _) = read_events(state_dir, 0)?;

    // Replay: live chain view + carriage records keyed by their accepting block (a removed
    // block takes its records with it — the same revert rule the capability store applies).
    struct Commitment {
        root: Hash64,
        commit_daa: u64,
        class: Hash64,
        executor: Hash64,
        logits_root: Hash64,
    }
    let mut chain: Vec<(String, u64)> = Vec::new();
    let mut commitments: Vec<(String, Commitment)> = Vec::new(); // (accepting block, record)
    let mut attestations: Vec<(String, (Hash64, Hash64, Hash64, u64))> = Vec::new(); // block, (root, attester, attested_logits, daa)
    let mut refutations: Vec<(String, (Hash64, u64))> = Vec::new();
    let mut calls = 0usize;
    let mut answers = 0usize;
    for event in &events {
        match event {
            Event::ChainAdd { block, daa } => chain.push((block.clone(), *daa)),
            Event::ChainRemove { block } => {
                chain.retain(|(h, _)| h != block);
                commitments.retain(|(b, _)| b != block);
                attestations.retain(|(b, _)| b != block);
                refutations.retain(|(b, _)| b != block);
            }
            Event::Carriage { block, daa, body, .. } => match body {
                CarriageEvent::Commitment { root, class, executor, logits_root, .. } => commitments.push((
                    block.clone(),
                    Commitment {
                        root: parse_hash64(root)?,
                        commit_daa: *daa,
                        class: parse_hash64(class)?,
                        executor: parse_hash64(executor)?,
                        logits_root: parse_hash64(logits_root)?,
                    },
                )),
                CarriageEvent::Attestation { commitment_root, attester, attested_logits_root } => attestations.push((
                    block.clone(),
                    (parse_hash64(commitment_root)?, parse_hash64(attester)?, parse_hash64(attested_logits_root)?, *daa),
                )),
                CarriageEvent::Refutation { target_root } => refutations.push((block.clone(), (parse_hash64(target_root)?, *daa))),
                CarriageEvent::OpeningCall { .. } => calls += 1,
                CarriageEvent::OpeningAnswer { .. } => answers += 1,
            },
        }
    }
    let last_daa = chain.iter().map(|(_, d)| *d).max().unwrap_or(0);

    let mut ledger = PalwShadowLedgerV1::new();
    let mut closed = 0usize;
    let mut pending = Vec::new();
    for (_, c) in &commitments {
        let schedule = job_schedule_v1(&params, c.commit_daa).map_err(|e| e.to_string())?;
        if schedule.challenge_close_daa > last_daa {
            pending.push(serde_json::json!({
                "root": hex64(&c.root),
                "commit_daa": c.commit_daa,
                "challenge_close_daa": schedule.challenge_close_daa,
            }));
            continue;
        }
        let anchor = chain.iter().find(|(_, daa)| *daa >= schedule.anchor_daa);
        let Some((anchor_hash, _)) = anchor else {
            pending.push(serde_json::json!({ "root": hex64(&c.root), "note": "no anchor block observed" }));
            continue;
        };
        let anchor_h = parse_hash64(anchor_hash)?;
        let panel = select_replay_panel_v1(&c.root, &c.executor, &anchor_h, &c.class, &candidates, roster.q);
        let duties: Vec<PalwDutyObservationV1> = panel
            .iter()
            .map(|member| {
                attestations
                    .iter()
                    .find(|(_, (root, attester, _, _))| *root == c.root && attester == member)
                    .map(|(_, (_, _, attested_logits, daa))| PalwDutyObservationV1::Attested {
                        root_matched: *attested_logits == c.logits_root,
                        attest_daa: *daa,
                    })
                    .unwrap_or(PalwDutyObservationV1::Silent)
            })
            .collect();
        let refutation_included_daa = refutations.iter().filter(|(_, (root, _))| *root == c.root).map(|(_, (_, daa))| *daa).min();
        ledger.observe_job(
            c.class,
            &PalwShadowJobObservationV1 { schedule, duties, refutation_included_daa, replay_durations_ms: vec![] },
        );
        closed += 1;
    }

    let classes: Vec<serde_json::Value> = ledger
        .report()
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "runtime_class_id": hex64(&r.runtime_class_id),
                "jobs": r.jobs,
                "creditable_jobs": r.creditable_jobs,
                "jobs_with_on_time_match": r.jobs_with_on_time_match,
                "refuted_in_window_jobs": r.refuted_in_window_jobs,
                "refuted_after_close_jobs": r.refuted_after_close_jobs,
                "duties": r.duties,
                "on_time_matches": r.on_time_matches,
                "late_matches": r.late_matches,
                "mismatches": r.mismatches,
                "no_shows": r.no_shows,
                "attest_latency_daa": {
                    "samples": r.attest_latency_daa.samples,
                    "p50": r.attest_latency_daa.p50, "p95": r.attest_latency_daa.p95,
                    "p99": r.attest_latency_daa.p99, "max": r.attest_latency_daa.max,
                },
            })
        })
        .collect();
    let doc = serde_json::json!({
        "schema": "misaka.palw-shadow.stage0-report.v1",
        "params": params_name,
        "events": events.len(),
        "last_seen_daa": last_daa,
        "commitments_seen": commitments.len(),
        "attestations_seen": attestations.len(),
        "opening_calls_seen": calls,
        "opening_answers_seen": answers,
        "jobs_closed": closed,
        "jobs_pending": pending,
        "classes": classes,
    });
    println!("{}", serde_json::to_string_pretty(&doc).expect("serializable"));
    Ok(())
}
