//! `misaka-palw-gateway` — the free-prompt front end (ADR-0044 Decision 10, FP-07; ADR-0077).
//!
//! ```text
//! user app ──POST /v1/chat/completions──▶ this process ──▶ the node (ADR-0077 Decision 3)
//!     │  the chat template, as SEGMENTS (Decision 6)          registered / fp_certified /
//!     ▼                                                       bond_active / exposure_room
//! the family worker, RESIDENT (--mode v3-serve)                       │
//!     │  Token frames ──▶ SSE deltas as they arrive (Decision 2)      │
//!     └▶ Result frame ──▶ W5: the streamed bytes ARE the committed ───┘
//!                          rendering, or NO commitment is written
//! ```
//!
//! **One inference.** The gateway never re-runs the model for mining — there is no second lane,
//! and nothing here can create one: the worker result carries both the answer and the roots, and
//! the caller-side re-binding (`validate_against_request`) is the same discipline the agent
//! client uses — the worker is never trusted about what it was asked.
//!
//! **F1 lives here, on both sides.** The ids handed to the model are exactly the canonical
//! template over the user's messages: no DAA suffix, no job metadata, no mining fields. Chain
//! binding (anchor, nonce, bond) rides in the job identity, outside the token stream. ADR-0077
//! SA-3 adds the prompt side of the check — the control tokens in the committed ids are the ones
//! this gateway placed, or nothing is committed — and Decision 2 adds the answer side.
//!
//! **What the outbox holds, honestly.** The framed `PalwFpWorkerResultV3`, the UNSIGNED
//! `PalwFreePromptCommitmentV3` (with the real retained-trace DA trio — the worker chunks the
//! ordered event-hash list to `<outbox>/traces/<job-id>/` before its result frame exists), and
//! a JSON summary. The gateway does NOT fabricate the one piece it must not have: the ML-DSA
//! signature belongs to the signer sidecar (ADR-0079 Decision 4 — this process holds no key), and
//! the summary names that and `misaka-palw-fp-rail --submit` as the remaining steps.
//!
//! **HTTP, hand-rolled.** One POST route, a health probe and an artifact fetch over std's
//! `TcpListener`, following `rpc/eth`'s in-tree precedent of not pulling an async HTTP stack for a
//! small, exact surface. `stream: true` is served as SSE (ADR-0077 Decision 2).

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use misaka_palw::host_security::{
    ALLOW_PUBLIC_GATEWAY_ENV, Confinement, ConfinementBackend, check_public_bind, establish_confinement, harden_worker_command,
    listen_is_loopback, public_gateway_acknowledged, reachable_signing_secrets, worker_working_dir,
};

use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFpWorkerFrameV1,
    PalwFpWorkerInputV3, PalwFpWorkerManifestV1, PalwFpWorkerRequestV3, PalwFpWorkerResultV3, fp_class_quantum_leaves_v1,
    fp_job_id_v3, fp_quanta_v3, fp_worker_request_hash_v3,
};
use kaspa_consensus_core::palw_v2::{PALW_V2_MAX_FRAME_BYTES, write_framed};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use serde::Deserialize;

// ADR-0078 Decision 6: the derivation step and one-response delivery. One module, one hook.
mod derive;
// ADR-0077 Decision 3: the four facts the chain owns, and the anchor.
mod chain;
// ADR-0077 Decisions 2 and 6 + SA-3: the prompt plan, the stream, and the two bindings.
mod wire;

use wire::{AnswerStream, PromptPlan, Turn};

// ---------------------------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct IdentityFile {
    /// 64-byte hex: the network's domain separator — the same value the attempt lane binds.
    network_domain: String,
    /// 64-byte hex: the registered execution class this gateway's worker embodies.
    class_id: String,
    /// 64-byte hex transaction id of the executor bond outpoint.
    bond_txid: String,
    bond_index: u32,
    /// Hex: the bond's ML-DSA-87 public key (carried; the signer sidecar holds the secret).
    executor_pubkey: String,
    /// 64-byte hex: the operator identity registered with the bond.
    operator_id: String,
}

struct Identity {
    network_domain: Hash64,
    class_id: Hash64,
    class_id_hex: String,
    bond_txid_hex: String,
    executor_bond: TransactionOutpoint,
    executor_pubkey: Vec<u8>,
    operator_id: Hash64,
}

// ---------------------------------------------------------------------------------------------
// ADR-0079 Decision 10 / ADR-0077 SA-1 — the public entrance is BOUNDED, and every bound below is
// mandatory rather than a default an operator can raise into an unbounded surface.
//
// SA-8 is the reason the per-source rate is last in this list and not first: sources share
// addresses behind proxies, so a per-IP rate is a courtesy. The BINDING limits are the single job
// slot, the bounded in-flight queue, and the daily public-job budget tied to exposure.
// ---------------------------------------------------------------------------------------------

/// A chat body larger than this is refused before it is parsed. 1 MiB of chat is already ~30x the
/// prefill cap of every class in the tree.
const MAX_REQUEST_BODY_BYTES: usize = 1 << 20;
/// The rendered prompt handed to the model, in bytes. A hard ceiling on top of the class's own
/// `n_ctx`: the worker refuses an over-long prompt too, but a bound the ENTRANCE enforces is a
/// bound that costs the attacker a 4xx instead of a model load.
const HARD_MAX_PROMPT_BYTES: usize = 64 * 1024;
/// No `--max-decode-cap` may exceed this, whatever the flag says.
const HARD_MAX_DECODE_CAP: u32 = 4_096;
/// **The largest temperature the job's own field can hold**, derived from the field rather than
/// chosen: `temperature_q` is a `u32` in Q24, so the representable range is `[0, u32::MAX / 2^24]`
/// — a hair under 256. A request above it is refused by name rather than clamped, because a
/// clamped temperature is a job that ran under a rule the requester did not ask for.
const MAX_TEMPERATURE: f64 = (u32::MAX as f64) / (kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_T_ONE as f64);
/// A single chat turn may not carry more messages than this.
const MAX_CHAT_MESSAGES: usize = 64;
/// Open connections. Past this the listener answers 503 and closes, rather than growing threads.
const MAX_CONNECTIONS: usize = 64;
/// **The in-flight queue.** One job runs; at most this many wait for the slot. Past it the answer
/// is a 503 with a Retry-After, never a queue whose depth silently eats deadlines.
const MAX_IN_FLIGHT_JOBS: usize = 8;
/// The public-job budget window.
const PUBLIC_BUDGET_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);
/// The per-source window (SA-8: secondary).
const PER_SOURCE_WINDOW: Duration = Duration::from_secs(60 * 60);
/// **ADR-0078 SA-4: the read route's own per-source rate, over [`PER_SOURCE_WINDOW`].**
///
/// SA-4's rule is that the DSL data-availability election must not turn the executor into a public
/// file server. `GET /v1/artifacts/<derived-id>` is the one route in this binary that already IS
/// one: `derived_id` is a value the CHAIN publishes (it is `derived_id_v1` of an object in a
/// block), so anyone reading the chain can name every artifact this gateway ever built, and until
/// this bound existed they could fetch each one as often as they liked, unauthenticated.
///
/// Bonded-requester authentication is deliberately NOT applied here, and the reason is the ADR's:
/// Decision 6 makes this handle the delivery path for "the artifact, as bytes … by a fetch handle
/// above a size the gateway states", to the person who asked, in a browser. A bond key in a
/// browser is not the shape of that transaction. What SA-4's other two words — bounded and
/// rate-limited — mean here is this counter and the direct-path resolve that replaced a directory
/// walk. The DSL half, which SA-4 is actually about, is not served by this binary at all; see
/// `misaka-palw-gateway/tests/dsl_da_election_gate.rs`.
///
/// 512 an hour per address is far above one person collecting the artifacts of their own session
/// (a job is capped at `per_source_jobs_per_window`, and each job yields one artifact) and far
/// below a scrape.
const FETCH_PER_SOURCE_PER_WINDOW: u32 = 512;
/// ADR-0077 SA-1(d): the exposure ceiling ratio the RC enforces on a bond's collateral in flight
/// (`PalwStateV2Error::FreePromptExposureCeiling`). Printed in `/health` so the loss bound is a
/// number the operator reads, not a promise they infer.
const FREE_PROMPT_EXPOSURE_CEILING_PERMILLE: u64 = 500;
/// ADR-0077 SA-1(b): a queued commitment expires WITH ITS ANCHOR and is never submitted stale.
/// Past this many DAA beyond the anchor the outbox artifact is retired.
const COMMITMENT_ANCHOR_TTL_DAA: u64 = 3_000;
/// **ADR-0079 SA-7.** The worker child's stderr is the model runtime's, and a runtime line can
/// quote its input. The pipe is always drained; the lines are printed only when this is `1`.
const WORKER_STDERR_ENV: &str = "MISAKA_PALW_GATEWAY_LOG_WORKER_STDERR";

struct Config {
    listen: String,
    worker: PathBuf,
    outbox: PathBuf,
    identity_path: PathBuf,
    /// Devnet display aid: the class's canonical job in leaves, so the summary can say how many
    /// draws a job earned (a quantum is an eighth of it — ADR-0074 Decision 5). Zero disables the
    /// display (no class known).
    class_leaves: u64,
    max_decode_default: u32,
    max_decode_cap: u32,
    /// How long past the job's anchor the producer promises to serve retained-trace chunks, in
    /// DAA score. A chain-time promise, so it rides the caller side of `to_commitment`.
    trace_retention_window_daa: u64,
    /// ADR-0078 Decision 6: the bond key's seed, when the gateway signs derivations itself (the
    /// rail's local-seed form); `None` leaves the object unsigned for the rail.
    ///
    /// **The file must live outside `--identity`'s directory and outside `--outbox`** — the boot
    /// refusal below (ADR-0079 Decision 4 / S5) scans exactly those two directories for reachable
    /// signing secrets, and a 32-byte seed dropped in either is the shape it looks for. Put it in
    /// the signer sidecar's own directory and point `--derive-seed` at that.
    derive_seed: Option<[u8; kaspa_pq_validator_core::VALIDATOR_SEED_LEN]>,
    /// Artifacts at or under this many bytes ride inline in the response; larger ones by handle.
    artifact_inline_max: usize,
    /// ADR-0079 Decision 5: the worker child's working directory (never the operator's home).
    workdir: PathBuf,
    /// The rendered-prompt ceiling actually in force, `min(flag, HARD_MAX_PROMPT_BYTES)`.
    max_prompt_bytes: usize,
    /// ADR-0077 SA-1(a): the bond's exposure room, in sompi, as the OPERATOR declared it. Zero
    /// means "read it from the chain" — with `--rpc` that is the honest source, and without one a
    /// gateway that does not know what the bond can lose ANSWERS but does not commit.
    bond_exposure_room_sompi: u64,
    /// The fraction of the room public jobs may spend per window, so the operator's OWN claims are
    /// never starved by strangers'.
    public_job_budget_permille: u64,
    /// What one claim reserves on the bond, in sompi, as the operator declared it. Zero means
    /// "read it from the chain".
    claim_exposure_sompi: u64,
    /// ADR-0077 SA-1(c): the operator marks this source class "answer, never commit".
    answer_never_commit: bool,
    /// SA-8's secondary bound: public jobs per source address per [`PER_SOURCE_WINDOW`].
    per_source_jobs_per_window: u32,
    /// ADR-0079 Decision 5's platform half, installed and PROVEN at boot before the bind guard
    /// reads it. `none` when there is none — which Decision 10 then refuses a public bind on.
    confinement: Confinement,
}

/// The exposure numbers actually in force for one job: the operator's declaration where they made
/// one, the chain's reading otherwise. Held apart from `Config` because one of the two moves per
/// job and the other does not.
#[derive(Clone, Copy, Debug)]
struct ExposurePrice {
    room_sompi: u64,
    claim_sompi: u64,
}

impl ExposurePrice {
    fn resolve(config: &Config, facts: &chain::ChainFacts) -> Self {
        Self {
            room_sompi: if config.bond_exposure_room_sompi > 0 { config.bond_exposure_room_sompi } else { facts.exposure_room_sompi },
            claim_sompi: if config.claim_exposure_sompi > 0 { config.claim_exposure_sompi } else { facts.claim_exposure_sompi },
        }
    }
}

/// ADR-0077 SA-1(a): what public jobs have spent of the operator's exposure in this window, and
/// whether the next one may commit. A public prompt becomes the OPERATOR's claim — it reserves
/// `claim_exposure` on the bond and forfeits it if the pipeline is faulty — so the spend is
/// bounded here, at the entrance, rather than discovered at the transition (SA-7).
struct PublicJobBudget {
    window_started: Instant,
    spent_sompi: u64,
    committed_jobs: u64,
    answered_without_commit: u64,
}

impl PublicJobBudget {
    fn new() -> Self {
        Self { window_started: Instant::now(), spent_sompi: 0, committed_jobs: 0, answered_without_commit: 0 }
    }

    fn daily_budget(config: &Config, price: ExposurePrice) -> u64 {
        price.room_sompi.saturating_mul(config.public_job_budget_permille) / 1_000
    }

    /// May the next public job COMMIT? Answering is never refused on budget grounds — the user
    /// gets their answer either way, which is what makes "answer, never commit" a mode rather
    /// than an outage.
    fn may_commit(&mut self, config: &Config, price: ExposurePrice) -> Result<(), String> {
        if self.window_started.elapsed() >= PUBLIC_BUDGET_WINDOW {
            self.window_started = Instant::now();
            self.spent_sompi = 0;
        }
        if config.answer_never_commit {
            return Err("this gateway runs in `answer, never commit` mode (ADR-0077 SA-1c)".into());
        }
        if price.room_sompi == 0 || price.claim_sompi == 0 {
            return Err("the bond's exposure room is not known (--bond-exposure-room-sompi / --claim-exposure-sompi, or --rpc \
                 so the chain can be asked); a gateway that cannot price the spend does not spend"
                .into());
        }
        if price.claim_sompi > price.room_sompi {
            return Err(format!(
                "one claim reserves {} sompi and the bond's room is {} — refused at the entrance, not at the transition (ADR-0077 SA-7)",
                price.claim_sompi, price.room_sompi
            ));
        }
        let budget = Self::daily_budget(config, price);
        if self.spent_sompi.saturating_add(price.claim_sompi) > budget {
            return Err(format!(
                "the public-job budget for this window is spent ({} of {} sompi); the operator's own claims are not starved by strangers'",
                self.spent_sompi, budget
            ));
        }
        Ok(())
    }

    fn charge(&mut self, price: ExposurePrice) {
        self.spent_sompi = self.spent_sompi.saturating_add(price.claim_sompi);
        self.committed_jobs += 1;
    }
}

/// SA-8's secondary bound. Kept because a single noisy source is still worth slowing, and named
/// secondary because sources share addresses behind proxies and this one cannot be the bound.
///
/// **Two counters, not one** (ADR-0078 SA-4). A job and an artifact fetch are different spends: a
/// job costs a model run and a slice of the operator's exposure, a fetch costs a file read. They
/// must not share a counter in either direction — a fetch that spent a job token would let a
/// browser reloading a GLB lock the person who asked out of their next prompt, and a job that
/// spent a fetch token would make the fetch bound meaningless. So `admit` charges the job budget
/// and [`SourceRates::admit_fetch`] charges its own.
#[derive(Default)]
struct SourceRates {
    seen: HashMap<IpAddr, (Instant, u32)>,
    fetched: HashMap<IpAddr, (Instant, u32)>,
}

impl SourceRates {
    fn admit(&mut self, source: IpAddr, per_window: u32) -> bool {
        Self::charge(&mut self.seen, source, per_window)
    }

    /// **ADR-0078 SA-4's rate, on the read route** — see [`FETCH_PER_SOURCE_PER_WINDOW`].
    fn admit_fetch(&mut self, source: IpAddr) -> bool {
        Self::charge(&mut self.fetched, source, FETCH_PER_SOURCE_PER_WINDOW)
    }

    fn charge(map: &mut HashMap<IpAddr, (Instant, u32)>, source: IpAddr, per_window: u32) -> bool {
        if per_window == 0 {
            return true;
        }
        // Bounded map: a window's worth of distinct sources, then a sweep. An unbounded map keyed
        // by attacker-chosen addresses is itself the memory attack.
        if map.len() > 4_096 {
            map.retain(|_, (at, _)| at.elapsed() < PER_SOURCE_WINDOW);
        }
        let entry = map.entry(source).or_insert((Instant::now(), 0));
        if entry.0.elapsed() >= PER_SOURCE_WINDOW {
            *entry = (Instant::now(), 0);
        }
        entry.1 += 1;
        entry.1 <= per_window
    }
}

/// **ADR-0079 SA-7: the default is withheld.** Only the exact string `1` turns the relay on —
/// "0", "false", "no" and an empty value are all a variable somebody set and did not mean, and
/// reading any of them as consent would disclose the model's input.
fn worker_stderr_relay_enabled(read: impl Fn(&str) -> Option<String>) -> bool {
    read(WORKER_STDERR_ENV).as_deref() == Some("1")
}

fn die(msg: String) -> ! {
    eprintln!("[misaka-palw-gateway] fatal: {msg}");
    std::process::exit(1);
}

fn hex64(s: &str, what: &str) -> Hash64 {
    let mut out = [0u8; 64];
    if s.len() != 128 || faster_hex::hex_decode(s.as_bytes(), &mut out).is_err() {
        die(format!("{what} is not 128 hex chars"));
    }
    Hash64::from_bytes(out)
}

fn hex_bytes(s: &str, what: &str) -> Vec<u8> {
    if !s.len().is_multiple_of(2) {
        die(format!("{what} is not even-length hex"));
    }
    let mut out = vec![0u8; s.len() / 2];
    if faster_hex::hex_decode(s.as_bytes(), &mut out).is_err() {
        die(format!("{what} is not hex"));
    }
    out
}

fn load_identity(path: &Path) -> Identity {
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| die(format!("cannot read identity file {}: {e}", path.display())));
    let file: IdentityFile = serde_json::from_str(&raw).unwrap_or_else(|e| die(format!("identity file is not valid JSON: {e}")));
    let pubkey = hex_bytes(&file.executor_pubkey, "executor_pubkey");
    if pubkey.is_empty() {
        die("executor_pubkey is empty — an unaccountable gateway must not produce commitments".into());
    }
    Identity {
        network_domain: hex64(&file.network_domain, "network_domain"),
        class_id: hex64(&file.class_id, "class_id"),
        class_id_hex: file.class_id.clone(),
        bond_txid_hex: file.bond_txid.clone(),
        executor_bond: TransactionOutpoint {
            transaction_id: TransactionId::from_bytes(hex64(&file.bond_txid, "bond_txid").as_bytes()),
            index: file.bond_index,
        },
        executor_pubkey: pubkey,
        operator_id: hex64(&file.operator_id, "operator_id"),
    }
}

// ---------------------------------------------------------------------------------------------
// The resident worker (ADR-0077 Decision 1): the artifact is mapped ONCE
// ---------------------------------------------------------------------------------------------

/// One `--mode v3-serve` child, with its pipes held open.
///
/// The artifact used to be mapped inside every job — about eight minutes per REQUEST on the hybrid
/// class — and the resident mode pays that once. One generation at a time: a single engine and a
/// single KV cache, which is why the whole struct sits behind one mutex.
struct ResidentWorker {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    manifest: PalwFpWorkerManifestV1,
}

impl ResidentWorker {
    fn spawn(confinement: &Confinement, worker: &Path, workdir: &Path, trace_out: &Path) -> Result<Self, String> {
        let mut command = confinement.command(worker);
        command.args(["--mode", "v3-serve", "--trace-out", &trace_out.display().to_string()]);
        // ADR-0079 Decision 5: the process that parses a stranger's prompt starts with nothing — no
        // operator environment, no PATH, and a working directory that is not the operator's home.
        harden_worker_command(&mut command, workdir);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot spawn {}: {e}", worker.display()))?;

        // **Drain the pipe always; print it only on request** (ADR-0079 SA-7).
        //
        // The pipe MUST be drained for the child's whole life — a filled buffer wedges the worker,
        // which is the live incident its own docs record — but draining and relaying are two
        // different decisions. This stream is the model runtime's stderr, not only the worker's
        // own log: a runtime line can quote its input, and "private unless disputed" is false if
        // the default log is a disclosure. So the lines are counted and withheld unless the
        // operator turns them on, and the count itself is printed so nobody debugs a silent pipe.
        let stderr = child.stderr.take().expect("piped");
        let relay = worker_stderr_relay_enabled(|name| std::env::var(name).ok());
        std::thread::spawn(move || {
            let mut withheld = 0u64;
            let mut last_report = Instant::now();
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if relay {
                    eprintln!("[palw-worker] {line}");
                    continue;
                }
                withheld += 1;
                // One summary line per minute, and one at the end: enough to see the worker is
                // talking, never enough to disclose what it said.
                if last_report.elapsed() >= Duration::from_secs(60) {
                    eprintln!("[palw-worker] {withheld} log lines withheld (ADR-0079 SA-7 — set {WORKER_STDERR_ENV}=1 to print them)");
                    last_report = Instant::now();
                }
            }
            if !relay && withheld > 0 {
                eprintln!("[palw-worker] {withheld} log lines withheld (ADR-0079 SA-7 — set {WORKER_STDERR_ENV}=1 to print them)");
            }
        });

        let stdin = child.stdin.take().expect("piped");
        let mut stdout = BufReader::new(child.stdout.take().expect("piped"));
        let first = wire::read_frame_stream(&mut stdout, PALW_V2_MAX_FRAME_BYTES)?
            .ok_or_else(|| "the worker exited before announcing its manifest".to_string())?;
        let manifest = match borsh::from_slice::<PalwFpWorkerFrameV1>(&first) {
            Ok(PalwFpWorkerFrameV1::Manifest(manifest)) => manifest,
            Ok(_) => return Err("the worker's first frame is not its manifest".to_string()),
            Err(e) => return Err(format!("the worker's first frame does not decode: {e}")),
        };
        if manifest.n_ctx == 0 || manifest.prefill_single_batch_cap == 0 {
            return Err("the worker's manifest reports no shape limits".to_string());
        }
        Ok(Self { child, stdin, stdout, manifest })
    }

    /// One job on the resident loop. `on_token` sees every generated id in decode order, as soon
    /// as it is selected — Decision 2's side channel.
    fn run_job(
        &mut self,
        request: &PalwFpWorkerRequestV3,
        on_token: &mut dyn FnMut(u32, &[u8]),
    ) -> Result<PalwFpWorkerResultV3, String> {
        let payload = borsh::to_vec(request).map_err(|e| format!("cannot serialize the worker request: {e}"))?;
        let request_hash = fp_worker_request_hash_v3(&payload);
        write_framed(&mut self.stdin, &payload).map_err(|e| format!("cannot write the job frame: {e}"))?;
        self.stdin.flush().map_err(|e| format!("cannot flush the job frame: {e}"))?;
        loop {
            let Some(bytes) = wire::read_frame_stream(&mut self.stdout, PALW_V2_MAX_FRAME_BYTES)? else {
                return Err("the worker stream ended before a terminator frame".to_string());
            };
            match borsh::from_slice::<PalwFpWorkerFrameV1>(&bytes).map_err(|e| format!("a worker frame does not decode: {e}"))? {
                PalwFpWorkerFrameV1::Token { token_id, rendered } => on_token(token_id, &rendered),
                PalwFpWorkerFrameV1::Result(result) => {
                    // The caller-side re-binding: the worker is never trusted about what it was
                    // asked, and `request_hash` is re-derived from OUR canonical encoding.
                    result
                        .validate_against_request(request, request_hash)
                        .map_err(|e| format!("the worker result does not bind the request: {e}"))?;
                    return Ok(*result);
                }
                PalwFpWorkerFrameV1::Refused { reason } => return Err(format!("the worker refused the job: {reason}")),
                PalwFpWorkerFrameV1::Manifest(_) => {
                    return Err("the worker re-announced its manifest mid-session".to_string());
                }
            }
        }
    }
}

impl Drop for ResidentWorker {
    fn drop(&mut self) {
        // Closing stdin is how a resident worker is meant to stop; the kill is the backstop for a
        // child that is wedged inside a generation.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The one worker slot, respawned when its stream dies.
///
/// A transport failure is not a bad job: the artifact took minutes to map and the next request
/// deserves a worker, so the supervisor drops the corpse and maps again on the next call. A
/// `Refused` frame is the opposite — the worker is fine and the job was not — and leaves the
/// resident process exactly where it was.
struct WorkerSupervisor {
    worker: PathBuf,
    workdir: PathBuf,
    trace_out: PathBuf,
    confinement: Confinement,
    current: Mutex<Option<ResidentWorker>>,
    manifest: PalwFpWorkerManifestV1,
}

impl WorkerSupervisor {
    fn boot(confinement: Confinement, worker: PathBuf, workdir: PathBuf, trace_out: PathBuf) -> Result<Self, String> {
        let resident = ResidentWorker::spawn(&confinement, &worker, &workdir, &trace_out)?;
        let manifest = resident.manifest.clone();
        Ok(Self { worker, workdir, trace_out, confinement, current: Mutex::new(Some(resident)), manifest })
    }

    fn manifest(&self) -> &PalwFpWorkerManifestV1 {
        &self.manifest
    }

    fn run(&self, request: &PalwFpWorkerRequestV3, on_token: &mut dyn FnMut(u32, &[u8])) -> Result<PalwFpWorkerResultV3, String> {
        let mut slot = self.current.lock().expect("the worker lock is never poisoned");
        if slot.is_none() {
            *slot = Some(ResidentWorker::spawn(&self.confinement, &self.worker, &self.workdir, &self.trace_out)?);
        }
        let outcome = slot.as_mut().expect("just spawned").run_job(request, on_token);
        if let Err(e) = &outcome
            && !e.starts_with("the worker refused the job")
        {
            // The stream is no longer trustworthy: drop the child so the next request maps a
            // fresh artifact instead of talking into a dead pipe forever.
            *slot = None;
            eprintln!("[misaka-palw-gateway] the resident worker was dropped after a transport failure: {e}");
        }
        outcome
    }
}

// ---------------------------------------------------------------------------------------------
// OpenAI-compatible request/response shapes (the subset this surface serves)
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatRequest {
    #[serde(default)]
    model: Option<String>,
    messages: Vec<ChatMessage>,
    #[serde(default)]
    max_tokens: Option<u32>,
    /// ADR-0077 Decision 2: the answer streams as SSE; the commitment does not.
    #[serde(default)]
    stream: Option<bool>,
    /// ADR-0078: the kind the person asked for — a transformer name (`scene/glb/v1`) or a kind
    /// name (`scene`). Absent: the answer is the product and nothing is derived.
    #[serde(default)]
    derive: Option<String>,
    /// ADR-0078 Decision 6: elect this claim's DSL into the data-availability obligation, so
    /// third parties can verify the derivation on request. Default off — the DSL is the answer
    /// to the person's prompt, and it is theirs to publish.
    #[serde(default)]
    serve_dsl: bool,
    /// **ADR-0082 Decision 11: the sampling temperature the person asked for**, as an ordinary
    /// float, in the class's own logit units — the number an OpenAI-shaped client already sends.
    /// Quantized to Q24 by [`sampling_from_request`] and carried into the job id; absent or `0.0`
    /// is the greedy default, which is the only value a network without the fence admits.
    #[serde(default)]
    temperature: Option<f64>,
    /// ADR-0082 Decision 11: 64 hex characters, the seed the sampler draws under. Absent is the
    /// zero seed. Named by the requester rather than rolled here so the same request twice is the
    /// same answer twice — and so nothing about the draw is the gateway's to choose.
    #[serde(default)]
    seed: Option<String>,
}

/// **ADR-0082 Decision 11: the request's sampler inputs, quantized and gated on the chain.**
///
/// Returns the `(sampling_seed, temperature_q)` pair the job carries. Three rules, in refusal
/// order:
///
/// 1. **The fence decides, and the chain holds the fence.** While
///    `ChainFacts::fp_decode_rules_armed` is false — every shipped preset — anything but the
///    greedy defaults is refused HERE, with the fence's name. It is not a gateway flag: a flag
///    could disagree with the network, and the direction it would disagree in is the expensive one
///    (every commitment refused by the transition, after the inference is already paid for). It is
///    also not a silent downgrade: a user who asked for a temperature and got greedy has been told
///    a false thing about what ran.
/// 2. **Quantization is exact and stated.** `temperature_q = round(temperature × 2^24)` — Q24, the
///    class's own fixed point ([`kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_T_ONE`]),
///    so `1.0` is `16,777,216` and the number a user sets is the number the rule uses. A
///    temperature outside `[0, MAX_TEMPERATURE]`, or one that is not a number, is refused rather
///    than clamped.
/// 3. **The seed is the requester's.** 64 hex characters, or absent for the zero seed. The gateway
///    never rolls one: a seed this process chose would make the same request twice two different
///    answers, and would put the draw in the hands of the party that is not paying for it.
fn sampling_from_request(chat: &ChatRequest, facts: &chain::ChainFacts) -> Result<([u8; 32], u32), String> {
    use kaspa_consensus_core::palw_decode_select_v2::{PALW_DECODE_SEED_GREEDY, PALW_DECODE_T_ONE, PALW_DECODE_TEMPERATURE_GREEDY};
    let temperature_q = match chat.temperature {
        None => PALW_DECODE_TEMPERATURE_GREEDY,
        Some(t) if t.is_nan() || t < 0.0 || t > MAX_TEMPERATURE => {
            return Err(format!("temperature must be a number in 0..={MAX_TEMPERATURE:.3}; got {t}"));
        }
        Some(t) => (t * PALW_DECODE_T_ONE as f64).round() as u32,
    };
    let sampling_seed = match chat.seed.as_deref() {
        None | Some("") => PALW_DECODE_SEED_GREEDY,
        Some(hex) => {
            let mut out = [0u8; 32];
            if hex.len() != 64 || faster_hex::hex_decode(hex.as_bytes(), &mut out).is_err() {
                return Err("seed must be 64 hex characters".to_string());
            }
            out
        }
    };
    if !facts.fp_decode_rules_armed && (temperature_q != PALW_DECODE_TEMPERATURE_GREEDY || sampling_seed != PALW_DECODE_SEED_GREEDY) {
        return Err("this network has not armed ADR-0082 Decision 11's sampler (Params::palw_fp_decode_rules) — a job with a \
             temperature or a seed would be refused by the transition as SamplingNotArmed, after the inference had \
             already been paid for. Omit `temperature` and `seed`, or send temperature 0."
            .to_string());
    }
    Ok((sampling_seed, temperature_q))
}

// ---------------------------------------------------------------------------------------------
// HTTP plumbing (hand-rolled, one exact surface)
// ---------------------------------------------------------------------------------------------

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).map_err(|e| format!("cannot read the request line: {e}"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    let path = parts.next().unwrap_or("").to_string();
    let mut content_length: usize = 0;
    let mut transfer_encoding_chunked = false;
    let mut headers_read = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| format!("cannot read a header: {e}"))?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        headers_read += 1;
        if headers_read > 64 {
            return Err("too many request headers".into());
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().map_err(|_| "content-length is not a number".to_string())?;
            } else if name.eq_ignore_ascii_case("transfer-encoding") && value.to_ascii_lowercase().contains("chunked") {
                transfer_encoding_chunked = true;
            }
        }
    }
    if transfer_encoding_chunked {
        // Refused rather than parsed: a chunked body has no declared length, and a length this
        // surface cannot check before reading is a bound it does not have (ADR-0079 Decision 10).
        return Err("chunked transfer-encoding is not accepted; send a body with a content-length".into());
    }
    if content_length > MAX_REQUEST_BODY_BYTES {
        return Err(format!("body of {content_length} bytes exceeds the {MAX_REQUEST_BODY_BYTES}-byte cap"));
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).map_err(|e| format!("cannot read the body: {e}"))?;
    Ok(HttpRequest { method, path, body })
}

fn respond(stream: &mut TcpStream, status: &str, body: &serde_json::Value) {
    let bytes = body.to_string().into_bytes();
    let head =
        format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", bytes.len());
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&bytes);
    let _ = stream.flush();
}

/// A binary body (ADR-0078 Decision 6's fetch handle: the artifact by its derived id).
fn respond_bytes(stream: &mut TcpStream, status: &str, content_type: &str, bytes: &[u8]) {
    let head =
        format!("HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", bytes.len());
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(bytes);
    let _ = stream.flush();
}

fn error_body(message: &str) -> serde_json::Value {
    serde_json::json!({ "error": { "message": message, "type": "invalid_request_error" } })
}

fn hex(h: Hash64) -> String {
    faster_hex::hex_string(h.as_byte_slice())
}

// ---------------------------------------------------------------------------------------------
// ADR-0077 Decision 2 — the SSE surface
// ---------------------------------------------------------------------------------------------

/// Where an answer goes as it is produced. The non-streaming form discards deltas and returns the
/// whole answer at the end; the SSE form writes each one as it arrives. Both run the SAME job path,
/// which is what keeps "streaming is UX; the consensus object is untouched" true in the code and
/// not only in the ADR.
trait ChatSink {
    fn delta(&mut self, _text: &str) {}
}

struct BufferedSink;
impl ChatSink for BufferedSink {}

struct SseSink<'a> {
    stream: &'a mut TcpStream,
    id: String,
    model: String,
    started: bool,
}

impl SseSink<'_> {
    fn head(&mut self) {
        let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n";
        let _ = self.stream.write_all(head.as_bytes());
        let _ = self.stream.flush();
        self.started = true;
    }

    fn event(&mut self, value: &serde_json::Value) {
        let _ = self.stream.write_all(format!("data: {value}\n\n").as_bytes());
        let _ = self.stream.flush();
    }

    fn chunk(&mut self, delta: serde_json::Value, finish_reason: serde_json::Value) {
        let value = serde_json::json!({
            "id": self.id,
            "object": "chat.completion.chunk",
            "model": self.model,
            "choices": [{ "index": 0, "delta": delta, "finish_reason": finish_reason }],
        });
        self.event(&value);
    }

    fn done(&mut self) {
        let _ = self.stream.write_all(b"data: [DONE]\n\n");
        let _ = self.stream.flush();
    }
}

impl ChatSink for SseSink<'_> {
    fn delta(&mut self, text: &str) {
        self.chunk(serde_json::json!({ "content": text }), serde_json::Value::Null);
    }
}

// ---------------------------------------------------------------------------------------------
// The one route
// ---------------------------------------------------------------------------------------------

/// ADR-0077 SA-1(b): a queued commitment expires WITH ITS ANCHOR. Sweep the outbox for unsigned
/// commitments whose anchor the chain has left behind and retire them, so a rail can never pick up
/// a stale one and submit it. Named `.expired` rather than deleted: the artifact is evidence of
/// work the operator did, and evidence is not this function's to destroy.
///
/// The other half of the loop is `misaka_palw_fp_submit::load_unsigned_commitment`, which refuses
/// a stem whose `.expired` sibling exists — a rename nobody reads stops nothing.
fn expire_stale_commitments(outbox: &Path, current_anchor_daa: u64, ttl_daa: u64) -> usize {
    let Ok(entries) = std::fs::read_dir(outbox) else { return 0 };
    let mut retired = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.to_string_lossy().ends_with(".commitment-unsigned.borsh") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(commitment) = borsh::from_slice::<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3>(&bytes) else {
            continue;
        };
        if misaka_palw_fp_submit::AnchorExpiry::new(commitment.job.anchor_daa, ttl_daa).is_expired_at(current_anchor_daa) {
            let mut retired_path = path.clone().into_os_string();
            retired_path.push(misaka_palw_fp_submit::EXPIRED_SUFFIX);
            if std::fs::rename(&path, &retired_path).is_ok() {
                retired += 1;
            }
        }
    }
    retired
}

#[allow(clippy::too_many_arguments)]
fn handle_chat(
    config: &Config,
    identity: &Identity,
    worker: &WorkerSupervisor,
    budget: &Mutex<PublicJobBudget>,
    chain_source: &chain::ChainSource,
    chat: ChatRequest,
    sink: &mut dyn ChatSink,
) -> Result<serde_json::Value, String> {
    // ADR-0079 Decision 10: every bound is mandatory, and exceeding one is a 4xx rather than a
    // queue. These are checked BEFORE the job is sent, which is the point of having them here.
    if chat.messages.len() > MAX_CHAT_MESSAGES {
        return Err(format!("{} messages exceeds the {MAX_CHAT_MESSAGES}-message cap", chat.messages.len()));
    }
    let manifest = worker.manifest();
    let turns: Vec<Turn> = chat.messages.iter().map(|m| Turn { role: m.role.clone(), content: m.content.clone() }).collect();
    let plan: PromptPlan = wire::build_prompt(manifest, &turns)?;
    if plan.displayed_len() > config.max_prompt_bytes {
        return Err(format!(
            "the rendered prompt is {} bytes and the cap is {} — refused before the job is sent",
            plan.displayed_len(),
            config.max_prompt_bytes
        ));
    }
    let decode_limit = chat.max_tokens.unwrap_or(config.max_decode_default).clamp(1, config.max_decode_cap);

    // ADR-0077 Decision 3: the chain this gateway commits to, read for THIS job.
    let facts = chain_source.read();
    if facts.anchor_block == Hash64::default() {
        return Err(facts.read_error.clone().unwrap_or_else(|| "no anchor is available for this job".to_string()));
    }
    let (anchor_block, anchor_daa) = (facts.anchor_block, facts.anchor_daa);
    expire_stale_commitments(&config.outbox, anchor_daa, COMMITMENT_ANCHOR_TTL_DAA);

    // ADR-0077 SA-1 + Decision 3: a stranger's prompt becomes the OPERATOR's claim. Decide BEFORE
    // the inference whether this one may spend exposure — the answer is produced either way; only
    // the commitment is withheld, which is what makes "answer, never commit" a mode and not an
    // outage, and what makes an uncertified class an answer rather than a refusal.
    let price = ExposurePrice::resolve(config, &facts);
    let mut commit_refusal = facts.commit_refusal();
    if commit_refusal.is_none() {
        commit_refusal = budget.lock().expect("the budget lock is never poisoned").may_commit(config, price).err();
    }
    let mut job_nonce = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut job_nonce);

    // ADR-0082 Decision 11, decided from the request and the CHAIN — before the model is loaded,
    // so a refusal costs a 4xx rather than an inference.
    let (sampling_seed, temperature_q) = sampling_from_request(&chat, &facts)?;

    let request = PalwFpWorkerRequestV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: identity.network_domain,
        class_id: identity.class_id,
        executor_bond: identity.executor_bond,
        executor_pubkey: identity.executor_pubkey.clone(),
        operator_id: identity.operator_id,
        anchor_block,
        anchor_daa,
        job_nonce,
        decode_token_limit: decode_limit,
        max_context_tokens: manifest.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed,
        temperature_q,
        input: PalwFpWorkerInputV3::Segments(plan.segments.clone()),
        model_profile_id: manifest.model_profile_id,
        runtime_manifest_hash: manifest.runtime_manifest_hash,
        runtime_class_id: manifest.runtime_class_id,
        shape_profile_id: manifest.shape_profile_id,
        trace_scheme_id: manifest.trace_scheme_id,
    };

    // **Decision 2: the answer streams as it is decoded; the commitment does not exist yet.**
    let eog: BTreeSet<u32> = manifest.eog_token_ids.iter().copied().collect();
    let mut stream = AnswerStream::new();
    let result = {
        let mut on_token = |token_id: u32, rendered: &[u8]| {
            if let Some(delta) = stream.push(token_id, rendered, &eog) {
                sink.delta(&delta);
            }
        };
        worker.run(&request, &mut on_token)?
    };
    if let Some(delta) = stream.finish() {
        sink.delta(&delta);
    }

    // **The two bindings, before anything is committed** (Decision 2 / W5 and SA-3). Either
    // failing is the same verdict: the run is not the user's inference, so no commitment.
    let streamed_checked = wire::check_streamed_answer(&stream, &result)?;
    wire::check_committed_prompt_ids(&plan, &result.prompt_token_ids, &wire::control_token_ids(manifest))?;

    let job_id = fp_job_id_v3(&result.job);
    let commitment = result.to_commitment(anchor_daa.saturating_add(config.trace_retention_window_daa));
    let work_leaves = commitment.work_leaves;
    let claim_id = kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&commitment);
    // The lane's price is the chain's when the chain was asked, and the operator's devnet display
    // aid otherwise (ADR-0074 Decision 5: a gateway that hardcodes "an eighth" declares a number
    // the network owns).
    let quanta_per_job = if facts.fp_quanta_per_canonical_job > 0 { facts.fp_quanta_per_canonical_job } else { 8 };
    let quanta = if config.class_leaves == 0 {
        0
    } else {
        let cap = if facts.fp_max_quanta_per_receipt > 0 { facts.fp_max_quanta_per_receipt } else { u32::MAX };
        fp_quanta_v3(work_leaves, fp_class_quantum_leaves_v1(config.class_leaves, quanta_per_job), cap)
    };

    // The outbox artifact: the framed result (borsh) + a JSON summary. Everything the executor
    // rail needs to assemble, sign and submit the commitment — and an honest list of what is
    // still pending (see the module doc).
    let artifact_stem = format!("fp-job-{}", &hex(job_id)[..16]);
    let artifact_borsh = config.outbox.join(format!("{artifact_stem}.result.borsh"));
    let artifact_json = config.outbox.join(format!("{artifact_stem}.json"));
    let result_bytes = borsh::to_vec(&result).map_err(|e| format!("cannot serialize the artifact: {e}"))?;
    std::fs::write(&artifact_borsh, &result_bytes).map_err(|e| format!("cannot write {}: {e}", artifact_borsh.display()))?;
    // **The commitment is written only when the chain and the operator's exposure both allow it**
    // (ADR-0077 Decision 3 / SA-1 / SA-7). Refused, the user still gets the answer above; what
    // does not happen is a claim this bond cannot back, discovered at the transition instead of at
    // the entrance.
    match &commit_refusal {
        None => {
            let commitment_borsh = config.outbox.join(format!("{artifact_stem}.commitment-unsigned.borsh"));
            let commitment_bytes = borsh::to_vec(&commitment).map_err(|e| format!("cannot serialize the commitment: {e}"))?;
            std::fs::write(&commitment_borsh, &commitment_bytes)
                .map_err(|e| format!("cannot write {}: {e}", commitment_borsh.display()))?;
            budget.lock().expect("the budget lock is never poisoned").charge(price);
        }
        Some(_) => {
            let mut guard = budget.lock().expect("the budget lock is never poisoned");
            guard.answered_without_commit += 1;
        }
    }
    let rendered_string = String::from_utf8_lossy(&result.rendered).into_owned();
    // ADR-0078 Decision 6: derive from the FULL committed rendering (never the display trim —
    // a DSL hashed from a trimmed answer is one no verifier holding the ids can reach).
    //
    // **Gated on `commit_refusal.is_none()`.** A derivation names a claim, and consensus refuses a
    // `DerivedArtifactV1` whose claim never entered the state (`DerivedClaimMissing`). Deriving
    // for a commitment this gateway has just declined to write would put an object in the outbox
    // that no chain can ever accept, and would spend the derivation budget doing it.
    let derivation = match (chat.derive.as_deref(), commit_refusal.is_none()) {
        (Some(spec), true) => Some(derive::run(
            spec,
            &derive::DeriveConfig { seed: config.derive_seed, serve_dsl: chat.serve_dsl },
            &misaka_palw_derive::ClaimBinding {
                network_domain: identity.network_domain,
                claim_id,
                output_root: result.output_root,
                executor_pubkey: identity.executor_pubkey.clone(),
            },
            rendered_string.as_bytes(),
            &config.outbox,
            &artifact_stem,
        )?),
        _ => None,
    };
    let derive_refusal = match (chat.derive.as_deref(), commit_refusal.as_deref()) {
        (Some(_), Some(why)) => Some(format!(
            "nothing was derived: a derivation names a claim, and this answer did not become one ({why}) — consensus would \
             refuse the object as DerivedClaimMissing"
        )),
        _ => None,
    };
    let (job_context_hash, family) = derive::read_worker_manifest(&config.outbox.join("traces").join(hex(job_id)));
    let summary = serde_json::json!({
        "schema": "misaka.palw.fp-v3-gateway-artifact.v1",
        "fp_job_id": hex(job_id),
        "template_id": plan.template_id,
        "prompt_tokens": result.job.prompt_tokens,
        "decode_tokens_executed": result.decode_tokens_executed,
        "decode_token_limit": result.job.decode_token_limit,
        "stop_reason": match result.stop_reason { PalwFpStopReasonV3::ExactBudgetReached => "exact_budget", PalwFpStopReasonV3::EndOfGeneration => "end_of_generation" },
        "fp_claim_id": hex(claim_id),
        "trace_root": hex(result.trace_root),
        "output_root": hex(result.output_root),
        "schedule_root": hex(result.schedule_root),
        "trace_manifest_root": hex(result.trace_manifest_root),
        "trace_chunk_count": result.trace_chunk_count,
        "trace_retention_daa": commitment.trace_retention_daa,
        "trace_dir": config.outbox.join("traces").join(hex(job_id)).display().to_string(),
        "work_leaves": work_leaves,
        "class_leaves": config.class_leaves,
        "quanta_at_configured_quantum": quanta,
        "answer_untrimmed": rendered_string,
        "job_context_hash": job_context_hash,
        "family": family,
        "derivation": derivation.as_ref().map(|d| d.to_json(0)),
        "not_derived_because": derive_refusal,
        // ADR-0077 Decision 2 / W5 and SA-3: what was checked before this commitment was written.
        "answer_stream_checked": streamed_checked,
        "prompt_ids_checked": true,
        // ADR-0077 Decision 3: the chain this job was priced against.
        "chain": facts.health_json(),
        // ADR-0077 SA-1(b): the anchor this commitment is bound to, and the DAA past which it must
        // never be submitted. A rail that finds `.expired` beside a stem is looking at work whose
        // freshness binding has lapsed.
        "commit_by_anchor_daa": commitment.job.anchor_daa.saturating_add(COMMITMENT_ANCHOR_TTL_DAA),
        "committed": commit_refusal.is_none(),
        "not_committed_because": commit_refusal.clone(),
        "pending_for_chain_submission": [
            "ML-DSA-87 signature over fp_claim_id (signer sidecar, or the rail's --bond-key-seed)",
            "misaka-palw-fp-rail --artifact <stem> ... --submit --rpc <host:port> (ADR-0077 Decision 4)",
        ],
    });
    std::fs::write(&artifact_json, serde_json::to_vec_pretty(&summary).unwrap())
        .map_err(|e| format!("cannot write {}: {e}", artifact_json.display()))?;

    let shown = if streamed_checked { stream.shown() } else { wire::display_trim(&rendered_string).to_string() };
    let finish_reason = match result.stop_reason {
        PalwFpStopReasonV3::EndOfGeneration => "stop",
        PalwFpStopReasonV3::ExactBudgetReached => {
            if shown.len() < rendered_string.trim_end().len() {
                "stop" // the guard or an EOG id ended the shown answer; the budget ended the run
            } else {
                "length"
            }
        }
    };
    let misaka = serde_json::json!({
        "fp_job_id": hex(job_id),
        "trace_root": hex(result.trace_root),
        "output_root": hex(result.output_root),
        "schedule_root": hex(result.schedule_root),
        "work_leaves": work_leaves,
        "artifact": artifact_json.display().to_string(),
        // ADR-0078 X6: what a consumer needs beside the answer to recompute the claim's
        // output_root — the ids, the job's context hash, and which family's rendered-hash
        // rule applies — and the executor key the derivation is bound to.
        "fp_claim_id": hex(claim_id),
        "output_token_ids": result.output_token_ids,
        "job_context_hash": job_context_hash,
        "family": family,
        "executor_pubkey": faster_hex::hex_string(&identity.executor_pubkey),
        "derivation": derivation.as_ref().map(|d| d.to_json(config.artifact_inline_max)),
        "not_derived_because": derive_refusal,
        "template_id": plan.template_id,
        "answer_stream_checked": streamed_checked,
        // The caller is told, in the same response, whether this answer became a claim. A
        // gateway that silently answered without committing would be lying about what the
        // operator staked on it.
        "committed": commit_refusal.is_none(),
        "not_committed_because": commit_refusal,
    });
    Ok(serde_json::json!({
        "id": format!("palwcmpl-{}", &hex(job_id)[..24]),
        "object": "chat.completion",
        "model": chat.model.unwrap_or_else(|| "misaka-palw-fp-v3".to_string()),
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": shown },
            "finish_reason": finish_reason,
        }],
        "usage": {
            "prompt_tokens": result.job.prompt_tokens,
            "completion_tokens": result.decode_tokens_executed,
            "total_tokens": result.job.prompt_tokens + result.decode_tokens_executed,
        },
        "misaka": misaka,
    }))
}

fn main() {
    let mut args: VecDeque<String> = std::env::args().skip(1).collect();
    let mut listen = "127.0.0.1:8790".to_string();
    let mut worker: Option<PathBuf> = None;
    let mut outbox: Option<PathBuf> = None;
    let mut identity_path: Option<PathBuf> = None;
    let mut anchor_path: Option<PathBuf> = None;
    let mut rpc_endpoint: Option<String> = None;
    let mut rpc_timeout_secs: u64 = 5;
    let mut class_leaves: u64 = 0;
    let mut max_decode_default: u32 = 256;
    let mut max_decode_cap: u32 = 1024;
    let mut trace_retention_window_daa: u64 = 500_000;
    let mut derive_seed_path: Option<PathBuf> = None;
    let mut artifact_inline_max: usize = 4 << 20;
    let mut max_prompt_bytes: usize = HARD_MAX_PROMPT_BYTES;
    let mut bond_exposure_room_sompi: u64 = 0;
    let mut public_job_budget_permille: u64 = 200;
    let mut claim_exposure_sompi: u64 = 0;
    let mut answer_never_commit = false;
    let mut per_source_jobs_per_window: u32 = 120;
    while let Some(arg) = args.pop_front() {
        let mut value = |what: &str| args.pop_front().unwrap_or_else(|| die(format!("{what} needs a value")));
        match arg.as_str() {
            "--listen" => listen = value("--listen"),
            "--worker" => worker = Some(PathBuf::from(value("--worker"))),
            "--outbox" => outbox = Some(PathBuf::from(value("--outbox"))),
            "--identity" => identity_path = Some(PathBuf::from(value("--identity"))),
            "--anchor" => anchor_path = Some(PathBuf::from(value("--anchor"))),
            "--rpc" => rpc_endpoint = Some(value("--rpc")),
            "--rpc-timeout-secs" => rpc_timeout_secs = value("--rpc-timeout-secs").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--class-leaves" => class_leaves = value("--class-leaves").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--max-decode-default" => {
                max_decode_default = value("--max-decode-default").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--max-decode-cap" => max_decode_cap = value("--max-decode-cap").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--trace-retention-window" => {
                trace_retention_window_daa = value("--trace-retention-window").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--derive-seed" => derive_seed_path = Some(PathBuf::from(value("--derive-seed"))),
            "--artifact-inline-max" => {
                artifact_inline_max = value("--artifact-inline-max").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--max-prompt-bytes" => max_prompt_bytes = value("--max-prompt-bytes").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--bond-exposure-room-sompi" => {
                bond_exposure_room_sompi = value("--bond-exposure-room-sompi").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--public-job-budget-permille" => {
                public_job_budget_permille = value("--public-job-budget-permille").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--claim-exposure-sompi" => {
                claim_exposure_sompi = value("--claim-exposure-sompi").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--answer-never-commit" => answer_never_commit = true,
            "--per-source-jobs-per-window" => {
                per_source_jobs_per_window = value("--per-source-jobs-per-window").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            other => die(format!(
                "unknown argument {other:?}\nusage: misaka-palw-gateway --worker <family-fp-worker> --outbox <dir> --identity <json> (--rpc <host:port> | --anchor <json>) [--listen addr] [--rpc-timeout-secs n] [--class-leaves n] [--max-decode-default n] [--max-decode-cap n] [--max-prompt-bytes n] [--bond-exposure-room-sompi n --claim-exposure-sompi n [--public-job-budget-permille n]] [--answer-never-commit] [--per-source-jobs-per-window n] [--derive-seed <file OUTSIDE --identity's dir and --outbox>] [--artifact-inline-max <bytes>]"
            )),
        }
    }
    // ADR-0079 Decision 5: one working directory for every worker this process spawns, and it is
    // neither the operator's home nor the node's datadir.
    let workdir = match worker_working_dir(None) {
        Ok(dir) => dir,
        Err(e) => die(e),
    };
    let mut config = Config {
        listen,
        worker: worker.unwrap_or_else(|| die("--worker <family-fp-worker> is required".into())),
        outbox: outbox.unwrap_or_else(|| die("--outbox <dir> is required".into())),
        identity_path: identity_path.unwrap_or_else(|| die("--identity <json> is required".into())),
        class_leaves,
        max_decode_default,
        // The flag may only lower the hard cap, never raise it (Decision 10: the bounds are
        // mandatory, not defaults).
        max_decode_cap: max_decode_cap.clamp(1, HARD_MAX_DECODE_CAP),
        trace_retention_window_daa,
        derive_seed: derive_seed_path.map(|p| derive::read_seed(&p).unwrap_or_else(|e| die(e))),
        artifact_inline_max,
        workdir,
        max_prompt_bytes: max_prompt_bytes.clamp(1, HARD_MAX_PROMPT_BYTES),
        bond_exposure_room_sompi,
        public_job_budget_permille: public_job_budget_permille.min(1_000),
        claim_exposure_sompi,
        answer_never_commit,
        per_source_jobs_per_window,
        confinement: Confinement::none(),
    };

    // -----------------------------------------------------------------------------------------
    // ADR-0079 Decision 4 / S5 — this process parses a stranger's bytes, so it holds no key. It
    // refuses to boot if a signing secret is reachable in its OWN view: the ML-DSA signature
    // belongs to the signer sidecar, and a seed dropped next to the identity file "for now" is
    // how that stops being true. `--derive-seed` must therefore point OUTSIDE both directories.
    // -----------------------------------------------------------------------------------------
    let identity_dir = config.identity_path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let secret_dirs: Vec<&Path> = vec![identity_dir.as_path(), config.outbox.as_path()];
    let reachable = reachable_signing_secrets(|name| std::env::var(name).ok(), &secret_dirs);
    if !reachable.is_empty() {
        let found = reachable.iter().map(|r| r.to_string()).collect::<Vec<_>>().join("; ");
        die(format!(
            "refusing to boot: a signing secret is reachable in this gateway's own view — {found}.\n\
             This process parses public HTTP text and holds the executor PUBLIC key only (ADR-0079 Decision 4). \
             Move the seed to the signer sidecar's own directory, or unset the variable."
        ));
    }

    // -----------------------------------------------------------------------------------------
    // ADR-0079 Decision 10 / S6 — the public entrance is acknowledged, or it does not start. And
    // a public bind on a host whose confinement backend is `none` does not start at all: that is
    // the one place where a stranger chooses the model's input.
    // -----------------------------------------------------------------------------------------
    // The backend installs and PROVES its own denials here — before the bind guard asks what is in
    // force, because a guard that read a configured value would be reading a promise.
    let (confinement, confinement_notes) = establish_confinement(&config.workdir, &[config.workdir.clone(), config.outbox.clone()]);
    for note in &confinement_notes {
        eprintln!("[misaka-palw-gateway] confinement: {note}");
    }
    let backend = confinement.backend();
    config.confinement = confinement;
    let acknowledged = public_gateway_acknowledged();
    if let Err(e) = check_public_bind(&config.listen, acknowledged, backend) {
        die(e);
    }

    std::fs::create_dir_all(&config.outbox).unwrap_or_else(|e| die(format!("cannot create the outbox: {e}")));
    let identity = load_identity(&config.identity_path);

    // ADR-0077 Decision 3: the chain, or an honest statement that there is none.
    let chain_source = match (&rpc_endpoint, &anchor_path) {
        (Some(endpoint), _) => chain::ChainSource::Rpc(
            chain::RpcChainSource::new(
                endpoint,
                identity.class_id_hex.clone(),
                identity.bond_txid_hex.clone(),
                identity.executor_bond.index,
                rpc_timeout_secs,
            )
            .unwrap_or_else(|e| die(e)),
        ),
        (None, Some(path)) => {
            chain::read_anchor_file(path).unwrap_or_else(|e| die(e));
            chain::ChainSource::AnchorFile(path.clone())
        }
        (None, None) => {
            die("one of --rpc <host:port> (ADR-0077 Decision 3: the gateway reads the chain it commits to) or --anchor <json> \
             (the offline form, which cannot submit) is required"
                .into())
        }
    };

    let trace_dir = config.outbox.join("traces");
    std::fs::create_dir_all(&trace_dir).unwrap_or_else(|e| die(format!("cannot create the trace retention dir: {e}")));
    // ADR-0077 Decision 1: the artifact is mapped ONCE, here, before the listener opens.
    let worker = WorkerSupervisor::boot(config.confinement.clone(), config.worker.clone(), config.workdir.clone(), trace_dir)
        .unwrap_or_else(|e| die(e));

    eprintln!(
        "[misaka-palw-gateway] listening on {} ({}) — worker manifest {}…, class {}…, n_ctx {}, template {}",
        config.listen,
        if listen_is_loopback(&config.listen) { "loopback" } else { "PUBLIC, acknowledged" },
        &hex(worker.manifest().runtime_manifest_hash)[..16],
        &hex(identity.class_id)[..16],
        worker.manifest().n_ctx,
        wire::template_id_for(worker.manifest()),
    );
    let boot_facts = chain_source.read();
    eprintln!(
        "[misaka-palw-gateway] chain {} | registered {} | fp_certified {} | bond_active {} | exposure_room {}",
        boot_facts.source, boot_facts.registered, boot_facts.fp_certified, boot_facts.bond_active, boot_facts.exposure_room_sompi
    );
    eprintln!(
        "[misaka-palw-gateway] confinement backend {} | one job slot, {MAX_IN_FLIGHT_JOBS} may queue, {MAX_CONNECTIONS} connections | \
         prompt ≤ {} bytes, body ≤ {MAX_REQUEST_BODY_BYTES} bytes, decode ≤ {} | public-job budget {}‰",
        backend.name(),
        config.max_prompt_bytes,
        config.max_decode_cap,
        config.public_job_budget_permille,
    );

    let config = Arc::new(config);
    let identity = Arc::new(identity);
    let worker = Arc::new(worker);
    let chain_source = Arc::new(chain_source);
    // **One job slot** — the worker is a whole-model subprocess, and interleaving two would only
    // thrash the page cache. **A BOUNDED queue in front of it** — an unbounded one is a deadline
    // eater and a memory attack; past `MAX_IN_FLIGHT_JOBS` the answer is a 503, not a wait.
    let in_flight = Arc::new(AtomicUsize::new(0));
    let connections = Arc::new(AtomicUsize::new(0));
    let budget = Arc::new(Mutex::new(PublicJobBudget::new()));
    let sources = Arc::new(Mutex::new(SourceRates::default()));

    let listener = TcpListener::bind(&config.listen).unwrap_or_else(|e| die(format!("cannot bind {}: {e}", config.listen)));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if connections.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            connections.fetch_sub(1, Ordering::AcqRel);
            respond(&mut stream, "503 Service Unavailable", &error_body("connection cap reached"));
            continue;
        }
        let (config, identity, worker, chain_source) =
            (Arc::clone(&config), Arc::clone(&identity), Arc::clone(&worker), Arc::clone(&chain_source));
        let (in_flight, budget, sources) = (Arc::clone(&in_flight), Arc::clone(&budget), Arc::clone(&sources));
        let connections = Arc::clone(&connections);
        let acknowledged_bind = acknowledged;
        std::thread::spawn(move || {
            serve_connection(
                &mut stream,
                &config,
                &identity,
                &worker,
                &chain_source,
                &in_flight,
                &budget,
                &sources,
                backend,
                acknowledged_bind,
            );
            connections.fetch_sub(1, Ordering::AcqRel);
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn serve_connection(
    stream: &mut TcpStream,
    config: &Config,
    identity: &Identity,
    worker: &WorkerSupervisor,
    chain_source: &chain::ChainSource,
    in_flight: &AtomicUsize,
    budget: &Mutex<PublicJobBudget>,
    sources: &Mutex<SourceRates>,
    backend: ConfinementBackend,
    acknowledged_bind: bool,
) {
    let source = stream.peer_addr().map(|a| a.ip()).ok();
    let request = match read_http_request(stream) {
        Ok(r) => r,
        Err(e) => {
            respond(stream, "400 Bad Request", &error_body(&e));
            return;
        }
    };
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/health") => {
            let facts = chain_source.read();
            let price = ExposurePrice::resolve(config, &facts);
            let snapshot = budget.lock().expect("the budget lock is never poisoned");
            let daily = PublicJobBudget::daily_budget(config, price);
            respond(
                stream,
                "200 OK",
                // The identity, not just the runtime. A client cannot otherwise tell whether the
                // gateway it is talking to is accountable to anything: the class id is what a
                // chain registers and a court adjudicates, and the bond outpoint is who pays if
                // the answer was a lie. All three are public on-chain facts — a `/health` that
                // withheld them would only be hiding them from the person deciding to trust
                // this endpoint. The commitment carries the same values, so a caller can check
                // that the job it got back came from the identity advertised here.
                //
                // ADR-0077 Decision 3 adds the CHAIN's four answers by name — `registered`,
                // `fp_certified`, `bond_active`, `exposure_room` — because "why did my answer not
                // become a claim" must be a thing an operator reads rather than infers. SA-1(d)
                // adds the loss bound, for the same reason and the other direction: a stranger's
                // prompt spends the operator's exposure, and the amount is a number here.
                &serde_json::json!({
                    "status": "ok",
                    "runtime_manifest_hash": hex(worker.manifest().runtime_manifest_hash),
                    "template_id": wire::template_id_for(worker.manifest()),
                    "n_ctx": worker.manifest().n_ctx,
                    "class_id": hex(identity.class_id),
                    "network_domain": hex(identity.network_domain),
                    "operator_id": hex(identity.operator_id),
                    "bond": format!("{}:{}", identity.executor_bond.transaction_id, identity.executor_bond.index),
                    "chain": facts.health_json(),
                    "commit_refusal": facts.commit_refusal(),
                    "can_submit": chain_source.can_submit(),
                    "posture": {
                        "listen": config.listen,
                        "public_bind": !listen_is_loopback(&config.listen),
                        "acknowledgement_variable": ALLOW_PUBLIC_GATEWAY_ENV,
                        "acknowledgement_required": !listen_is_loopback(&config.listen),
                        "acknowledgement_given": acknowledged_bind,
                        "confinement_backend": backend.name(),
                        "holds_key_material": false,
                    },
                    "bounds": {
                        "max_request_body_bytes": MAX_REQUEST_BODY_BYTES,
                        "max_prompt_bytes": config.max_prompt_bytes,
                        "max_decode_cap": config.max_decode_cap,
                        "job_slots": 1,
                        "max_in_flight_jobs": MAX_IN_FLIGHT_JOBS,
                        "max_connections": MAX_CONNECTIONS,
                        "per_source_jobs_per_window": config.per_source_jobs_per_window,
                        "per_source_window_secs": PER_SOURCE_WINDOW.as_secs(),
                    },
                    "exposure": {
                        "loss_bound": "at most claim_exposure per claim, and at most the FreePromptExposureCeiling \
                                       ratio of collateral in flight",
                        "free_prompt_exposure_ceiling_permille": FREE_PROMPT_EXPOSURE_CEILING_PERMILLE,
                        "claim_exposure_sompi": price.claim_sompi,
                        "bond_exposure_room_sompi": price.room_sompi,
                        "public_job_budget_permille": config.public_job_budget_permille,
                        "public_job_budget_window_sompi": daily,
                        "public_job_budget_spent_sompi": snapshot.spent_sompi,
                        "public_job_budget_window_secs": PUBLIC_BUDGET_WINDOW.as_secs(),
                        "answer_never_commit": config.answer_never_commit,
                        "committed_jobs": snapshot.committed_jobs,
                        "answered_without_commit": snapshot.answered_without_commit,
                        "commitment_anchor_ttl_daa": COMMITMENT_ANCHOR_TTL_DAA,
                    },
                }),
            );
        }
        ("POST", "/v1/chat/completions") => {
            if let Some(source) = source
                && !sources.lock().expect("the source lock is never poisoned").admit(source, config.per_source_jobs_per_window)
            {
                respond(stream, "429 Too Many Requests", &error_body("per-source job rate exceeded"));
                return;
            }
            if request.body.len() > MAX_REQUEST_BODY_BYTES {
                respond(stream, "400 Bad Request", &error_body("the body exceeds the request cap"));
                return;
            }
            // Parsed BEFORE the queue reservation so `stream: true` decides the response shape
            // while a status code is still possible.
            let chat: ChatRequest = match serde_json::from_slice(&request.body) {
                Ok(chat) => chat,
                Err(e) => {
                    respond(stream, "400 Bad Request", &error_body(&format!("request body is not a chat completion: {e}")));
                    return;
                }
            };
            let streaming = chat.stream == Some(true);
            // The bounded in-flight queue. Reserved BEFORE the slot is contended, so the depth of
            // the wait is a number this process chose rather than one the network chose for it.
            if in_flight.fetch_add(1, Ordering::AcqRel) >= MAX_IN_FLIGHT_JOBS {
                in_flight.fetch_sub(1, Ordering::AcqRel);
                respond(
                    stream,
                    "503 Service Unavailable",
                    &error_body("the in-flight queue is full; one job runs at a time and the queue is bounded"),
                );
                return;
            }
            if streaming {
                // **A slow reader must not wedge the one job slot.** The deltas are written from
                // inside the worker's mutex, so a client that stops reading would otherwise block
                // on TCP back-pressure and hold the resident worker for as long as it liked. A
                // write timeout turns that into a dropped connection.
                let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
                let model = chat.model.clone().unwrap_or_else(|| "misaka-palw-fp-v3".to_string());
                let mut nonce = [0u8; 12];
                rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
                // The chunk id is drawn per RESPONSE: the job id does not exist until the run
                // ends, and an OpenAI client needs a stable id from the first chunk.
                let mut sink = SseSink { stream, id: format!("palwcmpl-{}", faster_hex::hex_string(&nonce)), model, started: false };
                sink.head();
                let outcome = handle_chat(config, identity, worker, budget, chain_source, chat, &mut sink);
                in_flight.fetch_sub(1, Ordering::AcqRel);
                match outcome {
                    Ok(body) => {
                        // The terminal chunk carries the finish reason and, in the same event, the
                        // `misaka` object: whether this answer became a claim and, if not, why.
                        // An SSE client that never sees it is a client that was told nothing.
                        let finish = body["choices"][0]["finish_reason"].clone();
                        sink.chunk(serde_json::json!({}), finish);
                        sink.event(&serde_json::json!({ "misaka": body["misaka"].clone(), "usage": body["usage"].clone() }));
                        sink.done();
                    }
                    Err(e) => {
                        // Past the head, an error can only be an event. Decision 2: a stream whose
                        // rendering is not the committed one is CLOSED with an error, and no
                        // commitment was written.
                        sink.event(&error_body(&e));
                        sink.done();
                    }
                }
            } else {
                let mut sink = BufferedSink;
                let outcome = handle_chat(config, identity, worker, budget, chain_source, chat, &mut sink);
                in_flight.fetch_sub(1, Ordering::AcqRel);
                match outcome {
                    Ok(body) => respond(stream, "200 OK", &body),
                    Err(e) => respond(stream, "400 Bad Request", &error_body(&e)),
                }
            }
        }
        // ADR-0078 Decision 6's fetch handle: a derived artifact too large to ride inline is
        // served by its derived id. A GET with no side effects, so it needs neither the job slot
        // nor the in-flight reservation — but it is dispatched HERE, inside the bounded accept
        // loop, so the connection cap still counts it.
        //
        // **ADR-0078 SA-4: bounded and rate-limited.** `derived_id` is published on chain, so this
        // route is addressable by every reader of the chain and not only by the person who asked.
        // The rate is its own (see `FETCH_PER_SOURCE_PER_WINDOW`) so that a fetch never spends a
        // job token, and the resolve behind it is a direct path rather than a directory walk, so a
        // stranger's 404 costs one `stat` and not a scan of every artifact this gateway has built.
        ("GET", path) if path.starts_with("/v1/artifacts/") => {
            if let Some(source) = source
                && !sources.lock().expect("the source lock is never poisoned").admit_fetch(source)
            {
                respond(stream, "429 Too Many Requests", &error_body("per-source artifact fetch rate exceeded"));
                return;
            }
            match derive::artifact_by_id(&config.outbox, &path["/v1/artifacts/".len()..]) {
                Some((bytes, content_type)) => respond_bytes(stream, "200 OK", content_type, &bytes),
                None => respond(stream, "404 Not Found", &error_body("no artifact under that derived id")),
            }
        }
        _ => respond(
            stream,
            "404 Not Found",
            &error_body("this gateway serves POST /v1/chat/completions, GET /health and GET /v1/artifacts/<derived-id>"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounded_config() -> Config {
        Config {
            listen: "127.0.0.1:8790".into(),
            worker: PathBuf::from("/nonexistent/worker"),
            outbox: std::env::temp_dir().join("palw-gw-test-outbox"),
            identity_path: PathBuf::from("/nonexistent/identity.json"),
            class_leaves: 0,
            max_decode_default: 256,
            max_decode_cap: 1024,
            trace_retention_window_daa: 500_000,
            workdir: std::env::temp_dir(),
            max_prompt_bytes: HARD_MAX_PROMPT_BYTES,
            bond_exposure_room_sompi: 1_000_000,
            public_job_budget_permille: 200,
            claim_exposure_sompi: 50_000,
            answer_never_commit: false,
            per_source_jobs_per_window: 2,
            confinement: Confinement::none(),
            derive_seed: None,
            artifact_inline_max: 4 << 20,
        }
    }

    fn declared_price(config: &Config) -> ExposurePrice {
        ExposurePrice::resolve(config, &chain::ChainFacts::default())
    }

    /// **ADR-0079 S6.** A public bind fails at startup without the acknowledgement, and fails
    /// UNCONDITIONALLY when the confinement backend in force is `none` — which is the state this
    /// tree ships in, so this is the rule that is actually load-bearing today.
    #[test]
    fn a_public_bind_is_refused_and_the_message_names_the_pattern() {
        assert!(check_public_bind("127.0.0.1:8790", false, ConfinementBackend::None).is_ok(), "loopback is the default and is fine");

        let err = check_public_bind("0.0.0.0:8790", false, ConfinementBackend::MacosSandboxExec).unwrap_err();
        assert!(err.contains(ALLOW_PUBLIC_GATEWAY_ENV));
        assert!(err.to_lowercase().contains("reverse proxy"));

        // The state a host with no requested backend ships in. The acknowledgement does not help.
        let err = check_public_bind("0.0.0.0:8790", true, ConfinementBackend::None).unwrap_err();
        assert!(err.contains("does NOT override"));
        assert_eq!(Confinement::none().backend(), ConfinementBackend::None, "and this is what `none` looks like");
    }

    /// **No wildcard CORS** — the house rule `SECURITY.md` already states for the mining bridge,
    /// held here too so a page on another origin cannot read this endpoint out of the operator's
    /// browser. The response head is pinned, not just the absence of a call to set the header.
    #[test]
    fn responses_carry_no_cors_header_at_all() {
        let bytes = serde_json::json!({"status": "ok"}).to_string().into_bytes();
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            bytes.len()
        );
        let lowered = head.to_lowercase();
        assert!(!lowered.contains("access-control-allow-origin"), "no CORS header, wildcard or otherwise");
        assert!(!lowered.contains('*'), "nothing in this response head is a wildcard");
        // And the shipped writer is the one that produced that head. The needle is assembled at
        // run time so this assertion does not match its own source line.
        let needle = ["access-control", "allow-origin"].join("-");
        let responses: Vec<&str> = std::include_str!("main.rs")
            .lines()
            .filter(|l| l.contains("HTTP/1.1") || l.trim_start().starts_with("let head ="))
            .collect();
        assert!(!responses.is_empty(), "the response writer must be findable for this assertion to mean anything");
        for line in responses {
            assert!(!line.to_lowercase().contains(&needle), "a response head writes a CORS header: {line}");
        }
    }

    /// **ADR-0077 SA-1 / SA-8.** The binding limits are the single slot, the bounded queue and the
    /// budget — and the budget refuses to COMMIT while still allowing the answer.
    #[test]
    fn the_public_job_budget_bounds_the_operators_exposure() {
        let config = bounded_config();
        let price = declared_price(&config);
        let mut budget = PublicJobBudget::new();
        // 200 permille of a 1,000,000-sompi room is 200,000; a 50,000-sompi claim fits four times.
        assert_eq!(PublicJobBudget::daily_budget(&config, price), 200_000);
        for _ in 0..4 {
            budget.may_commit(&config, price).expect("within the window budget");
            budget.charge(price);
        }
        let err = budget.may_commit(&config, price).unwrap_err();
        assert!(err.contains("budget for this window is spent"), "got {err}");
        assert_eq!(budget.committed_jobs, 4);

        // SA-1(c): the operator may mark the source class "answer, never commit".
        let never = Config { answer_never_commit: true, ..bounded_config() };
        let err = PublicJobBudget::new().may_commit(&never, declared_price(&never)).unwrap_err();
        assert!(err.contains("answer, never commit"));

        // SA-7: a claim that would exceed the bond's room is refused HERE, at the entrance.
        let over = Config { claim_exposure_sompi: 2_000_000, ..bounded_config() };
        let err = PublicJobBudget::new().may_commit(&over, declared_price(&over)).unwrap_err();
        assert!(err.contains("refused at the entrance"), "got {err}");

        // An unconfigured room with no chain to read it from is an unknown, and an unknown does
        // not spend.
        let unknown = Config { bond_exposure_room_sompi: 0, ..bounded_config() };
        assert!(PublicJobBudget::new().may_commit(&unknown, declared_price(&unknown)).is_err());
    }

    /// **ADR-0077 Decision 3 + SA-7.** With no declaration the exposure numbers come from the
    /// chain, and the SA-7 refusal then fires on the CHAIN's room rather than on a constant the
    /// operator typed.
    #[test]
    fn the_exposure_price_falls_back_to_the_chain_and_the_operator_may_lower_it() {
        let chain_facts = chain::ChainFacts { exposure_room_sompi: 900_000, claim_exposure_sompi: 3_000, ..Default::default() };
        let undeclared = Config { bond_exposure_room_sompi: 0, claim_exposure_sompi: 0, ..bounded_config() };
        let price = ExposurePrice::resolve(&undeclared, &chain_facts);
        assert_eq!((price.room_sompi, price.claim_sompi), (900_000, 3_000), "the chain owns these numbers");
        PublicJobBudget::new().may_commit(&undeclared, price).expect("a bond with room may commit");

        // A declaration wins, in both directions — it is the operator's own ceiling on the loss.
        let declared = Config { bond_exposure_room_sompi: 10_000, claim_exposure_sompi: 0, ..bounded_config() };
        assert_eq!(ExposurePrice::resolve(&declared, &chain_facts).room_sompi, 10_000);

        // SA-7 on the chain's numbers: a claim larger than the room never leaves the entrance.
        let tight = chain::ChainFacts { exposure_room_sompi: 1_000, claim_exposure_sompi: 50_000, ..Default::default() };
        let price = ExposurePrice::resolve(&undeclared, &tight);
        let err = PublicJobBudget::new().may_commit(&undeclared, price).unwrap_err();
        assert!(err.contains("refused at the entrance"), "got {err}");
    }

    /// The per-source rate is SECONDARY (SA-8) but it is real: the third job from one address in
    /// a window is refused when the operator set the quota to two.
    #[test]
    fn the_per_source_quota_admits_then_refuses() {
        let mut rates = SourceRates::default();
        let source: IpAddr = "203.0.113.7".parse().unwrap();
        assert!(rates.admit(source, 2));
        assert!(rates.admit(source, 2));
        assert!(!rates.admit(source, 2), "the third job in the window is refused");
        // Another source is unaffected — the quota is per source, not a global gate.
        assert!(rates.admit("198.51.100.9".parse().unwrap(), 2));
        // Zero disables it, because a quota of zero would otherwise mean "serve nobody".
        assert!(rates.admit(source, 0));
    }

    /// **ADR-0078 SA-4: the read route is rate-limited, and on its OWN counter.**
    ///
    /// `GET /v1/artifacts/<derived-id>` was unauthenticated and uncounted, and `derived_id` is a
    /// value the chain publishes — so every reader of the chain could name and re-fetch every
    /// artifact this gateway had ever built. The bound is per source over the same window as the
    /// job quota, and the two must not share a counter in either direction: a browser reloading a
    /// GLB must not be able to lock the person who asked out of their next prompt, and jobs must
    /// not be able to exhaust the fetch allowance of the answer they just produced.
    #[test]
    fn the_artifact_fetch_rate_is_bounded_and_does_not_spend_the_job_quota() {
        let mut rates = SourceRates::default();
        let source: IpAddr = "203.0.113.11".parse().unwrap();

        // A fetch does not spend a job token: two jobs are still available after many fetches.
        for _ in 0..64 {
            assert!(rates.admit_fetch(source));
        }
        assert!(rates.admit(source, 2));
        assert!(rates.admit(source, 2));
        assert!(!rates.admit(source, 2), "the job quota is still the operator's two");

        // And the fetch allowance is finite: one past the ceiling is refused.
        let mut fresh = SourceRates::default();
        let scraper: IpAddr = "198.51.100.22".parse().unwrap();
        for n in 0..FETCH_PER_SOURCE_PER_WINDOW {
            assert!(fresh.admit_fetch(scraper), "fetch {n} is within the allowance");
        }
        assert!(!fresh.admit_fetch(scraper), "the fetch past FETCH_PER_SOURCE_PER_WINDOW is refused");
        // Per source, not global: another address still fetches.
        assert!(fresh.admit_fetch("198.51.100.23".parse().unwrap()));
    }

    /// Every mandatory bound is a hard ceiling a flag may only LOWER. A `--max-decode-cap` of a
    /// million is a bound the operator does not have.
    #[test]
    fn the_flags_may_lower_a_bound_and_never_raise_it() {
        assert_eq!(1_000_000u32.clamp(1, HARD_MAX_DECODE_CAP), HARD_MAX_DECODE_CAP);
        assert_eq!(64u32.clamp(1, HARD_MAX_DECODE_CAP), 64);
        assert_eq!(usize::MAX.clamp(1, HARD_MAX_PROMPT_BYTES), HARD_MAX_PROMPT_BYTES);
        assert!(MAX_IN_FLIGHT_JOBS > 0 && MAX_IN_FLIGHT_JOBS < MAX_CONNECTIONS, "the queue is bounded and smaller than the accepts");
    }

    /// **ADR-0077 SA-1(b).** A queued commitment expires WITH ITS ANCHOR: past the TTL the outbox
    /// artifact is retired so no rail can pick it up and submit it stale. The suffix is the one
    /// `misaka-palw-fp-submit` refuses to read through — the two halves of the loop agree by
    /// sharing the constant, not by both spelling it.
    #[test]
    fn a_queued_commitment_expires_with_its_anchor() {
        let dir = std::env::temp_dir().join(format!("palw-gw-expiry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Nothing to sweep is not an error, and a non-commitment file is never touched.
        std::fs::write(dir.join("fp-job-abc.json"), b"{}").unwrap();
        assert_eq!(expire_stale_commitments(&dir, 10_000, COMMITMENT_ANCHOR_TTL_DAA), 0);
        assert!(dir.join("fp-job-abc.json").is_file());
        // A commitment file that does not decode is left alone rather than silently deleted.
        std::fs::write(dir.join("fp-job-abc.commitment-unsigned.borsh"), b"not borsh").unwrap();
        assert_eq!(expire_stale_commitments(&dir, 10_000, COMMITMENT_ANCHOR_TTL_DAA), 0);
        assert_eq!(misaka_palw_fp_submit::EXPIRED_SUFFIX, ".expired", "both halves of the loop share one suffix");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **ADR-0082 Decision 11 at the entrance: the fence is the chain's, the quantization is
    /// exact, and neither is silently softened.**
    #[test]
    fn the_gateway_refuses_sampling_until_the_chain_arms_it() {
        use kaspa_consensus_core::palw_decode_select_v2::{PALW_DECODE_SEED_GREEDY, PALW_DECODE_T_ONE};
        let chat = |temperature: Option<f64>, seed: Option<&str>| ChatRequest {
            model: None,
            messages: Vec::new(),
            max_tokens: None,
            stream: None,
            derive: None,
            serve_dsl: false,
            temperature,
            seed: seed.map(str::to_string),
        };
        let dormant = chain::ChainFacts::default();
        let armed = chain::ChainFacts { fp_decode_rules_armed: true, ..Default::default() };

        // Greedy is admissible on every network, and is what an absent field means.
        assert_eq!(sampling_from_request(&chat(None, None), &dormant), Ok((PALW_DECODE_SEED_GREEDY, 0)));
        assert_eq!(sampling_from_request(&chat(Some(0.0), Some("")), &dormant), Ok((PALW_DECODE_SEED_GREEDY, 0)));

        // A temperature on a network that has not armed the fence is a refusal that NAMES it —
        // not a silent downgrade to greedy, and not an inference the operator pays for and the
        // transition then refuses.
        let refused = sampling_from_request(&chat(Some(0.7), None), &dormant).unwrap_err();
        assert!(refused.contains("palw_fp_decode_rules"), "the refusal names the fence: {refused}");
        assert!(refused.contains("SamplingNotArmed"), "and the error the transition would raise: {refused}");
        // A seed alone is the same refusal — it is a field claiming to decide something.
        assert!(sampling_from_request(&chat(None, Some(&"ab".repeat(32))), &dormant).is_err());

        // Armed: the quantization is `round(t x 2^24)`, exactly.
        assert_eq!(sampling_from_request(&chat(Some(1.0), None), &armed), Ok((PALW_DECODE_SEED_GREEDY, PALW_DECODE_T_ONE as u32)));
        assert_eq!(sampling_from_request(&chat(Some(0.5), None), &armed).unwrap().1, (PALW_DECODE_T_ONE / 2) as u32);
        assert_eq!(sampling_from_request(&chat(Some(0.7), None), &armed).unwrap().1, 11_744_051, "0.7 x 2^24 rounded");

        // The seed is 64 hex characters or it is a refusal — never a truncation.
        let hex = "0123456789abcdef".repeat(4);
        assert_eq!(sampling_from_request(&chat(Some(1.0), Some(&hex)), &armed).unwrap().0[0], 0x01);
        assert!(sampling_from_request(&chat(Some(1.0), Some("dead")), &armed).is_err(), "a short seed is refused");
        assert!(sampling_from_request(&chat(Some(1.0), Some(&"zz".repeat(32))), &armed).is_err(), "non-hex is refused");

        // The ceiling is the FIELD's, derived: `u32::MAX / 2^24`. Above it is a refusal, because a
        // clamped temperature is a job that ran under a rule nobody asked for.
        assert!((MAX_TEMPERATURE - 255.999_999).abs() < 1e-4, "u32::MAX / 2^24 = {MAX_TEMPERATURE}");
        assert!(sampling_from_request(&chat(Some(MAX_TEMPERATURE), None), &armed).is_ok());
        assert!(sampling_from_request(&chat(Some(MAX_TEMPERATURE + 1.0), None), &armed).is_err());
        assert!(sampling_from_request(&chat(Some(-0.1), None), &armed).is_err());
        assert!(sampling_from_request(&chat(Some(f64::NAN), None), &armed).is_err());
    }

    /// **ADR-0079 S5.** The gateway holds the executor PUBLIC key only, and refuses to boot when a
    /// signing secret is reachable in its own view — which is why `--derive-seed` must point
    /// outside `--identity`'s directory and outside `--outbox`, the two this scans.
    #[test]
    fn a_reachable_signing_secret_is_a_boot_refusal() {
        let dir = std::env::temp_dir().join(format!("palw-gw-secret-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("identity.json"), b"{}").unwrap();
        assert!(reachable_signing_secrets(|_| None, &[dir.as_path()]).is_empty(), "an identity file is not a secret");

        std::fs::write(dir.join("bond.seed"), [3u8; 32]).unwrap();
        let found = reachable_signing_secrets(|_| None, &[dir.as_path()]);
        assert_eq!(found.len(), 1, "a 32-byte file beside the identity is the shape of a raw ML-DSA-87 seed");
        // And the usage text says where the seed may live, so the refusal is not the first time an
        // operator hears about it.
        let usage = std::include_str!("main.rs");
        assert!(usage.contains("--derive-seed <file OUTSIDE --identity's dir and --outbox>"), "the flag documents its own rule");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **ADR-0077 SA-5 / ADR-0079 SA-7.** Nothing in this binary logs a prompt or a prompt id.
    /// Checked over every log statement in the shipped source rather than over a helper, because
    /// the failure mode is one line added later, not a helper misused.
    #[test]
    fn the_gateway_logs_no_prompt() {
        for (file, source) in [
            ("main.rs", std::include_str!("main.rs")),
            ("wire.rs", std::include_str!("wire.rs")),
            ("chain.rs", std::include_str!("chain.rs")),
        ] {
            for line in source.lines() {
                let trimmed = line.trim_start();
                if !(trimmed.starts_with("eprintln!") || trimmed.starts_with("println!") || trimmed.starts_with("log::")) {
                    continue;
                }
                for forbidden in [
                    "rendered_prompt",
                    "chat.messages",
                    "message.content",
                    "prompt_token_ids",
                    "displayed",
                    "plan.segments",
                    "rendered_string",
                    "delta",
                    ".shown()",
                ] {
                    assert!(!line.contains(forbidden), "{file}: a log line carries {forbidden}: {line}");
                }
            }
        }
    }

    /// **ADR-0079 SA-7.** The worker's stderr is the model runtime's, and a runtime line can quote
    /// its input. The pipe is drained either way — a filled buffer wedges the child — but the
    /// lines are printed only on an explicit opt-in, and the summary line says how many were held.
    #[test]
    fn worker_stderr_is_withheld_unless_the_operator_asks_for_it() {
        assert!(!worker_stderr_relay_enabled(|_| None), "the default is withheld");
        assert!(worker_stderr_relay_enabled(|_| Some("1".into())));
        for not_consent in ["", "0", "true", "yes", "on", " 1"] {
            assert!(
                !worker_stderr_relay_enabled(|_| Some(not_consent.into())),
                "{not_consent:?} is a variable somebody set and did not mean; only `1` is consent"
            );
        }
        // And the summary line names the variable, so nobody debugs a silent pipe.
        let source = std::include_str!("main.rs");
        assert!(source.contains("log lines withheld (ADR-0079 SA-7"), "the withholding announces itself");
        assert_eq!(WORKER_STDERR_ENV, "MISAKA_PALW_GATEWAY_LOG_WORKER_STDERR");
    }

    /// The chat request parser accepts the OpenAI subset, and `stream: true` is now SERVED
    /// (ADR-0077 Decision 2) rather than refused.
    #[test]
    fn chat_request_subset_parses() {
        let parsed: ChatRequest =
            serde_json::from_str(r#"{"model":"x","messages":[{"role":"user","content":"hi"}],"max_tokens":32}"#).unwrap();
        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.max_tokens, Some(32));
        assert_eq!(parsed.stream, None);

        let stream: ChatRequest = serde_json::from_str(r#"{"messages":[{"role":"user","content":"hi"}],"stream":true}"#).unwrap();
        assert_eq!(stream.stream, Some(true));
    }
}
