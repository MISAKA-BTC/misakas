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
//! **HTTP, hand-rolled.** One POST route, a health probe, a models list and an artifact fetch over
//! std's `TcpListener`, following `rpc/eth`'s in-tree precedent of not pulling an async HTTP stack
//! for a small, exact surface. `stream: true` is served as SSE (ADR-0077 Decision 2).
//!
//! **The OpenAI surface is one module** (ADR-0096 Decision 1): `surface` reads the request shape a
//! stock client sends — content parts, tools and tool turns, `response_format`, the sampling knobs
//! an SDK sets by default — and `surface::admit_request` is the ONE function every refusal comes
//! from, called before the queue is reserved and before the worker is touched. What it admits is
//! rendered by `wire` (tool turns as the model's own text, Decision 2) and checked after the run
//! (`response_format`, advisory on every network whose fence is dormant, Decision 3).

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
// ADR-0096 Decision 1: the request shape, and every refusal, before the worker.
mod surface;
// RFC-0001 §2.7: the worker pool and the per-source bounds.
mod pool;
mod tensor;
// RFC-0001 §2.7: Retry-After, the bounded queue as a reservation, and the client-presence probe.
mod serving;
// RFC-0001 §2.7: an Idempotency-Key makes a retry the same claim, not a second one.
mod idempotency;
// RFC-0001 §2.7: streaming → committed → submitted → final | voided, and what each is worth.
mod status;

use surface::{AdmittedRequest, ChatRequest};
use wire::{AnswerStream, PromptPlan};

// ---------------------------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct IdentityFile {
    /// 64-byte hex: the network's domain separator — the same value the attempt lane binds. May be
    /// absent for an answer-only gateway (ADR-0096 Decision 10): nothing it produces is filed, so
    /// no domain separates it from anything, and the job carries zeros.
    #[serde(default)]
    network_domain: String,
    /// 64-byte hex: the registered execution class this gateway's worker embodies. May be absent
    /// for an answer-only gateway reading an anchor file (ADR-0096 Decision 10): the worker IS the
    /// class, so its manifest's class id is adopted after boot. A caller that runs any artifact —
    /// the Studio, for a class no table of its own names — then cannot name the wrong one.
    #[serde(default)]
    class_id: String,
    /// 64-byte hex transaction id of the executor bond outpoint. **May be absent** (ADR-0096
    /// Decision 10): a gateway that only ANSWERS — the Studio's local integer engine — has no
    /// bond, and is admitted without one exactly when `--answer-never-commit` is set, so the two
    /// facts cannot disagree (a bond-less gateway that could commit would sign nothing anyone
    /// could adjudicate; a bonded one that never commits is just a choice).
    #[serde(default)]
    bond_txid: String,
    #[serde(default)]
    bond_index: u32,
    /// Hex: the bond's ML-DSA-87 public key (carried; the signer sidecar holds the secret). May be
    /// absent under the same rule as `bond_txid`.
    #[serde(default)]
    executor_pubkey: String,
    /// 64-byte hex: the operator identity registered with the bond. May be absent for an
    /// answer-only gateway, under the same rule as the bond.
    #[serde(default)]
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
/// **The in-flight cap with `processes` job slots** (RFC-0001 §2.7): the slots plus the bounded
/// queue behind them. One process keeps the cap it always had ([`MAX_IN_FLIGHT_JOBS`], one running
/// and seven waiting).
fn in_flight_cap(processes: usize) -> usize {
    processes + (MAX_IN_FLIGHT_JOBS - 1)
}
/// Default per-source bounds (RFC-0001 §2.7): open connections and jobs in flight for one address.
const DEFAULT_MAX_CONNECTIONS_PER_SOURCE: u32 = 8;
const DEFAULT_MAX_JOBS_PER_SOURCE: u32 = 4;
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
    /// **Which privacy mode every job this gateway files declares** (ADR-0077 Decision 16):
    /// `PALW_FP_PRIVACY_PUBLIC_DA` (the prompt rides the commitment, readable by anyone) or
    /// `PALW_FP_PRIVACY_PANEL_DA` (the prompt reaches only the drawn seats over the authenticated
    /// pull, and the chain carries its commitment alone). Mode 2 is refused per request where the
    /// chain has not armed the fence (`ChainFacts::panel_da_armed`), before the inference.
    privacy_mode: u8,
    /// SA-8's secondary bound: public jobs per source address per [`PER_SOURCE_WINDOW`].
    per_source_jobs_per_window: u32,
    /// ADR-0079 Decision 5's platform half, installed and PROVEN at boot before the bind guard
    /// reads it. `none` when there is none — which Decision 10 then refuses a public bind on.
    confinement: Confinement,
    /// When this process started, in Unix seconds — `GET /v1/models` reports it as the model's
    /// `created`, the moment the class became reachable here.
    booted_at_unix: u64,
    /// **RFC-0001 §2.7 stage 1: resident worker processes for this class** (`--worker-processes`).
    worker_processes: usize,
    /// Extra arguments every worker process is started with (`--kv-cache-budget-mib`, …).
    worker_args: Vec<String>,
    /// **RFC-0001 §2.6/§2.7: answer a job that will not be committed WITHOUT folding it** (on by
    /// default; `--no-answer-fast-path` turns it off). A worker that does not serve the path says
    /// so once and the gateway stops asking.
    answer_fast_path: bool,
    /// Per-source open connections and in-flight jobs (RFC-0001 §2.7: connection limits).
    max_connections_per_source: u32,
    max_jobs_per_source: u32,
    /// **RFC-0001 §2.9: the artifact sidecar this gateway reads** (`--sidecar`): its chat template
    /// (preferred over the built-in one) and its generation defaults (gateway defaults, applied
    /// through the entrance's own admission).
    sidecar: Option<SidecarRuntime>,
    /// **RFC-0001 §2.7: stop serving, and never commit, a request whose client has gone** (on by default;
    /// `--no-cancel-on-disconnect` turns it off for a client that half-closes after sending its request).
    cancel_on_disconnect: bool,
    /// How deep (DAA) a `Final`/void claim row must be before `GET /v1/requests/<id>` calls it settled.
    finality_depth: u64,
}

/// A loaded, verified sidecar and the two template ids it runs under (leaked once at boot: the
/// prompt plan's id is a `&'static str` and a boot-time constant is the honest lifetime).
struct SidecarRuntime {
    digest: String,
    path: PathBuf,
    template: Option<misaka_palw_base0::sidecar::ChatTemplateSpecV1>,
    template_ids: Option<(&'static str, &'static str)>,
    generation: Option<misaka_palw_base0::sidecar::GenerationConfigV1>,
}

impl SidecarRuntime {
    fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let side = misaka_palw_base0::sidecar::parse_sidecar_v2(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        let template_ids = side.template_id().map(|id| {
            let plain: &'static str = Box::leak(id.clone().into_boxed_str());
            let tools: &'static str = Box::leak(format!("{id}+tools").into_boxed_str());
            (plain, tools)
        });
        Ok(Self {
            digest: faster_hex::hex_string(side.digest().as_byte_slice()),
            path: path.to_path_buf(),
            template: side.chat_template,
            template_ids,
            generation: side.generation_config,
        })
    }

    /// Fill what the request omitted from the sidecar's generation defaults — through the same
    /// admission as anything else: the sampler's defaults only where the chain has armed it, the
    /// knobs this lane has no rule for never. Returns the report the response carries.
    fn apply_defaults(&self, chat: &mut ChatRequest, facts: &chain::ChainFacts) -> serde_json::Value {
        let mut applied: Vec<String> = Vec::new();
        let mut not_applied: Vec<serde_json::Value> = Vec::new();
        if let Some(g) = &self.generation {
            if chat.max_tokens.is_none() && chat.max_completion_tokens.is_none()
                && let Some(n) = g.max_new_tokens
            {
                chat.max_tokens = Some(n);
                applied.push("max_new_tokens".to_string());
            }
            let sampler = facts.fp_decode_rules_armed;
            let reason = "palw_fp_decode_rules is not armed on this network (ADR-0082 Decision 11): a sampler default would be refused";
            macro_rules! fill {
                ($field:ident, $name:literal) => {
                    if let Some(v) = g.$field
                        && chat.$field.is_none()
                    {
                        if sampler {
                            chat.$field = Some(v);
                            applied.push($name.to_string());
                        } else {
                            not_applied.push(serde_json::json!({ "field": $name, "reason": reason }));
                        }
                    }
                };
            }
            fill!(temperature, "temperature");
            fill!(repeat_penalty, "repeat_penalty");
            fill!(frequency_penalty, "frequency_penalty");
            fill!(presence_penalty, "presence_penalty");
            if let Some(n) = g.repeat_last_n
                && chat.repeat_last_n.is_none()
            {
                if sampler {
                    chat.repeat_last_n = Some(n);
                    applied.push("repeat_last_n".to_string());
                } else {
                    not_applied.push(serde_json::json!({ "field": "repeat_last_n", "reason": reason }));
                }
            }
            if !g.stop.is_empty() && chat.stop.as_ref().is_none_or(|v| v.is_null()) {
                if sampler {
                    chat.stop = Some(serde_json::json!(g.stop));
                    applied.push("stop".to_string());
                } else {
                    not_applied.push(serde_json::json!({ "field": "stop", "reason": reason }));
                }
            }
            for (name, present) in [("top_p", g.top_p.is_some()), ("top_k", g.top_k.is_some())] {
                if present {
                    not_applied.push(serde_json::json!({ "field": name, "reason": "this lane has no rule for it (ADR-0096 Decision 4)" }));
                }
            }
        }
        serde_json::json!({
            "digest": self.digest,
            "template_id": self.template_ids.map(|(plain, _)| plain),
            "defaults_applied": applied,
            "defaults_not_applied": not_applied,
        })
    }
}

/// The template id the gateway advertises: the sidecar's when it has a chat template, the model's
/// own selection otherwise.
fn advertised_template_id(config: &Config, manifest: &PalwFpWorkerManifestV1) -> String {
    match config.sidecar.as_ref().and_then(|s| s.template_ids) {
        Some((plain, _)) => plain.to_string(),
        None => wire::template_id_for(manifest).to_string(),
    }
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
        // **A claim larger than the whole window is not a window that got spent.** Said as "spent
        // (0 of N)" it read like a busy day; it is a setting under which no public job can EVER
        // commit — the 2026-09-20 economy drill: past the ADR-0145 bundle the entrance priced one
        // claim at the compute era's 147,880,590 sompi against a 200‰ budget of 110,000,868.
        if price.claim_sompi > budget {
            return Err(format!(
                "one claim reserves {} sompi and this window's whole public-job budget is {} ({}‰ of the bond's room {}): \
                 no public job can commit at this setting — raise --public-job-budget-permille (the Studio pool runs 1000) \
                 or the bond's room",
                price.claim_sompi, budget, config.public_job_budget_permille, price.room_sompi
            ));
        }
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

fn load_identity(path: &Path, answer_never_commit: bool) -> Identity {
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| die(format!("cannot read identity file {}: {e}", path.display())));
    let file: IdentityFile = serde_json::from_str(&raw).unwrap_or_else(|e| die(format!("identity file is not valid JSON: {e}")));
    identity_from_file(file, answer_never_commit).unwrap_or_else(|e| die(e))
}

/// **A bond-less identity is admitted only for a gateway that never commits** (ADR-0096 Decision
/// 10). The Studio's local integer engine is this gateway with no bond and no key: it answers
/// under the class, `/health` says `can_submit: false` and `bond: null`, and every job is
/// answered-not-committed by the same refusal a bonded gateway in that mode gives. Without the
/// flag, a missing bond or key is refused here rather than discovered as an unsignable
/// commitment after the inference.
fn identity_from_file(file: IdentityFile, answer_never_commit: bool) -> Result<Identity, String> {
    let bond_absent = file.bond_txid.trim().is_empty();
    let pubkey = if file.executor_pubkey.trim().is_empty() { Vec::new() } else { hex_bytes(&file.executor_pubkey, "executor_pubkey") };
    if (bond_absent || pubkey.is_empty()) && !answer_never_commit {
        return Err(format!(
            "identity file names no {}: an unaccountable gateway must not produce commitments — run with --answer-never-commit \
             (ADR-0096 Decision 10: a gateway that only answers needs no bond), or name the bond and its key",
            if bond_absent { "bond (`bond_txid`)" } else { "executor key (`executor_pubkey`)" }
        ));
    }
    let executor_bond = if bond_absent {
        TransactionOutpoint { transaction_id: TransactionId::from_bytes(Hash64::default().as_bytes()), index: 0 }
    } else {
        TransactionOutpoint {
            transaction_id: TransactionId::from_bytes(hex64(&file.bond_txid, "bond_txid").as_bytes()),
            index: file.bond_index,
        }
    };
    Ok(Identity {
        network_domain: hex64_or_absent(&file.network_domain, "network_domain", answer_never_commit)?,
        class_id: hex64_or_absent(&file.class_id, "class_id", answer_never_commit)?,
        class_id_hex: file.class_id.clone(),
        bond_txid_hex: file.bond_txid.clone(),
        executor_bond,
        executor_pubkey: pubkey,
        operator_id: hex64_or_absent(&file.operator_id, "operator_id", answer_never_commit)?,
    })
}

/// A 64-byte hex field, or — for an answer-only gateway only — an absent one read as zeros
/// (ADR-0096 Decision 10). A gateway that can commit names every field or is refused here.
fn hex64_or_absent(value: &str, what: &str, answer_never_commit: bool) -> Result<Hash64, String> {
    if value.trim().is_empty() {
        return if answer_never_commit {
            Ok(Hash64::default())
        } else {
            Err(format!("identity file names no `{what}`: only an --answer-never-commit gateway may omit it (ADR-0096 Decision 10)"))
        };
    }
    Ok(hex64(value, what))
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
    fn spawn(confinement: &Confinement, worker: &Path, workdir: &Path, trace_out: &Path, extra_args: &[String]) -> Result<Self, String> {
        let mut command = confinement.command(worker);
        command.args(["--mode", "v3-serve", "--trace-out", &trace_out.display().to_string()]);
        command.args(extra_args);
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
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
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
                        .validate_against_request(request, request_hash, prompt_ids_form)
                        .map_err(|e| format!("the worker result does not bind the request: {e}"))?;
                    return Ok(*result);
                }
                PalwFpWorkerFrameV1::Refused { reason } => return Err(format!("the worker refused the job: {reason}")),
                PalwFpWorkerFrameV1::Manifest(_) => {
                    return Err("the worker re-announced its manifest mid-session".to_string());
                }
                PalwFpWorkerFrameV1::Answered(_)
                | PalwFpWorkerFrameV1::AnsweredBatch(_)
                | PalwFpWorkerFrameV1::BatchToken { .. }
                | PalwFpWorkerFrameV1::Embedded(_) => {
                    return Err("the worker answered a committed job with an answer-only frame".to_string());
                }
            }
        }
    }

    /// **RFC-0001 §2.6/§2.7: one job, answered with no commitment.** The same request, behind the
    /// answer-only magic; the worker runs the same decoder with no fold and may resume from its
    /// KV prefix cache. [`AnswerRun::Unsupported`] is the worker saying it serves no such path
    /// (a family without one, or a build older than this contract) — the caller runs the committed
    /// path instead, and the worker is untouched.
    fn run_answer(
        &mut self,
        request: &PalwFpWorkerRequestV3,
        on_token: &mut dyn FnMut(u32, &[u8]),
    ) -> Result<AnswerRun, String> {
        let payload = borsh::to_vec(request).map_err(|e| format!("cannot serialize the worker request: {e}"))?;
        let request_hash = fp_worker_request_hash_v3(&payload);
        let mut framed = kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_WORKER_ANSWER_ONLY_MAGIC_V1.to_vec();
        framed.extend_from_slice(&payload);
        write_framed(&mut self.stdin, &framed).map_err(|e| format!("cannot write the job frame: {e}"))?;
        self.stdin.flush().map_err(|e| format!("cannot flush the job frame: {e}"))?;
        loop {
            let Some(bytes) = wire::read_frame_stream(&mut self.stdout, PALW_V2_MAX_FRAME_BYTES)? else {
                return Err("the worker stream ended before a terminator frame".to_string());
            };
            match borsh::from_slice::<PalwFpWorkerFrameV1>(&bytes).map_err(|e| format!("a worker frame does not decode: {e}"))? {
                PalwFpWorkerFrameV1::Token { token_id, rendered } => on_token(token_id, &rendered),
                PalwFpWorkerFrameV1::Answered(answer) => {
                    if answer.request_hash != request_hash {
                        return Err("the worker's answer does not bind the request it was asked".to_string());
                    }
                    return Ok(AnswerRun::Answered(*answer));
                }
                PalwFpWorkerFrameV1::Refused { reason } => {
                    return if reason.contains("serves no answer-only path") || reason.contains("not a v3 request") {
                        Ok(AnswerRun::Unsupported)
                    } else {
                        Err(format!("the worker refused the job: {reason}"))
                    };
                }
                PalwFpWorkerFrameV1::Result(_)
                | PalwFpWorkerFrameV1::AnsweredBatch(_)
                | PalwFpWorkerFrameV1::BatchToken { .. }
                | PalwFpWorkerFrameV1::Embedded(_) => {
                    return Err("the worker answered an answer-only request with a frame of another kind".to_string());
                }
                PalwFpWorkerFrameV1::Manifest(_) => {
                    return Err("the worker re-announced its manifest mid-session".to_string());
                }
            }
        }
    }

    /// **RFC-0001 §2.8: one embedding.** Bound to the request's bytes; `Unsupported` is a worker that
    /// serves no embedding path (the route answers 501 and the worker is untouched).
    fn run_embed(&mut self, request: &kaspa_consensus_core::palw_freeprompt_v3::PalwFpEmbedRequestV1) -> Result<EmbedRun, String> {
        let payload = borsh::to_vec(request).map_err(|e| format!("cannot serialize the embed request: {e}"))?;
        let request_hash = fp_worker_request_hash_v3(&payload);
        let mut framed = kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_WORKER_EMBED_MAGIC_V1.to_vec();
        framed.extend_from_slice(&payload);
        write_framed(&mut self.stdin, &framed).map_err(|e| format!("cannot write the embed frame: {e}"))?;
        self.stdin.flush().map_err(|e| format!("cannot flush the embed frame: {e}"))?;
        let Some(bytes) = wire::read_frame_stream(&mut self.stdout, PALW_V2_MAX_FRAME_BYTES)? else {
            return Err("the worker stream ended before a terminator frame".to_string());
        };
        match borsh::from_slice::<PalwFpWorkerFrameV1>(&bytes).map_err(|e| format!("a worker frame does not decode: {e}"))? {
            PalwFpWorkerFrameV1::Embedded(e) => {
                if e.request_hash != request_hash {
                    return Err("the worker's embedding does not bind the request it was asked".to_string());
                }
                Ok(EmbedRun::Embedded(*e))
            }
            PalwFpWorkerFrameV1::Refused { reason } => {
                if reason.contains("serves no embedding path") || reason.contains("not a v3 request") {
                    Ok(EmbedRun::Unsupported)
                } else {
                    Err(format!("the worker refused the job: {reason}"))
                }
            }
            _ => Err("the worker answered an embed request with a frame of another kind".to_string()),
        }
    }

    /// **RFC-0001 §2.7 stage 2: `requests` answered in ONE batched decode.** Answers come back in
    /// request order, each bound to its own request bytes.
    fn run_answer_batch(
        &mut self,
        requests: &[PalwFpWorkerRequestV3],
        on_token: &mut dyn FnMut(usize, u32, &[u8]),
    ) -> Result<AnswerBatchRun, String> {
        let payloads: Vec<Vec<u8>> =
            requests.iter().map(|r| borsh::to_vec(r).map_err(|e| format!("cannot serialize a worker request: {e}"))).collect::<Result<_, _>>()?;
        let hashes: Vec<Hash64> = payloads.iter().map(|p| fp_worker_request_hash_v3(p)).collect();
        let mut framed = kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_WORKER_ANSWER_BATCH_MAGIC_V1.to_vec();
        framed.extend_from_slice(&borsh::to_vec(&payloads).map_err(|e| format!("cannot serialize the batch: {e}"))?);
        write_framed(&mut self.stdin, &framed).map_err(|e| format!("cannot write the batch frame: {e}"))?;
        self.stdin.flush().map_err(|e| format!("cannot flush the batch frame: {e}"))?;
        loop {
            let Some(bytes) = wire::read_frame_stream(&mut self.stdout, PALW_V2_MAX_FRAME_BYTES)? else {
                return Err("the worker stream ended before a terminator frame".to_string());
            };
            match borsh::from_slice::<PalwFpWorkerFrameV1>(&bytes).map_err(|e| format!("a worker frame does not decode: {e}"))? {
                PalwFpWorkerFrameV1::BatchToken { index, token_id, rendered } => {
                    if index as usize >= hashes.len() {
                        return Err("the worker streamed an id for a request the batch does not have".to_string());
                    }
                    on_token(index as usize, token_id, &rendered)
                }
                PalwFpWorkerFrameV1::AnsweredBatch(answers) => {
                    if answers.len() != hashes.len() || answers.iter().zip(&hashes).any(|(a, h)| a.request_hash != *h) {
                        return Err("the worker's batch does not bind the requests it was asked".to_string());
                    }
                    return Ok(AnswerBatchRun::Answered(answers));
                }
                PalwFpWorkerFrameV1::Refused { reason } => {
                    return if reason.contains("serves no answer-only path")
                        || reason.contains("not a v3 request")
                        || reason.contains("not a list of requests")
                    {
                        Ok(AnswerBatchRun::Unsupported)
                    } else {
                        Err(format!("the worker refused the job: {reason}"))
                    };
                }
                _ => return Err("the worker answered a batch with a frame of another kind".to_string()),
            }
        }
    }
}

/// What an embedding request came to.
enum EmbedRun {
    Embedded(kaspa_consensus_core::palw_freeprompt_v3::PalwFpWorkerEmbeddingV1),
    Unsupported,
}

/// What a batch request came to.
enum AnswerBatchRun {
    Answered(Vec<kaspa_consensus_core::palw_freeprompt_v3::PalwFpWorkerAnswerV1>),
    Unsupported,
}

/// What an answer-only request came to.
enum AnswerRun {
    Answered(kaspa_consensus_core::palw_freeprompt_v3::PalwFpWorkerAnswerV1),
    /// The worker serves no answer-only path: run the committed one.
    Unsupported,
}

impl Drop for ResidentWorker {
    fn drop(&mut self) {
        // Closing stdin is how a resident worker is meant to stop; the kill is the backstop for a
        // child that is wedged inside a generation.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// **The class's worker pool** (RFC-0001 §2.7 stage 1), each slot respawned when its stream dies.
///
/// `--worker-processes N` resident workers serve one class: the artifact is mapped read-only by
/// every process, so the weights live once in the OS page cache and each process adds its own KV and
/// scratch; a request takes the next idle slot in arrival order ([`pool::SlotPool`]). With `N = 1`
/// this is exactly the one worker slot it replaces.
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
    worker_args: Vec<String>,
    pool: pool::SlotPool<Option<ResidentWorker>>,
    manifest: PalwFpWorkerManifestV1,
    /// Cleared the first time a worker says it serves no answer-only path, so the gateway stops
    /// asking (and stops paying a round trip) for the rest of its life.
    answer_only: std::sync::atomic::AtomicBool,
}

impl WorkerSupervisor {
    fn boot(
        confinement: Confinement,
        worker: PathBuf,
        workdir: PathBuf,
        trace_out: PathBuf,
        processes: usize,
        worker_args: Vec<String>,
    ) -> Result<Self, String> {
        let processes = processes.clamp(1, MAX_WORKER_PROCESSES);
        let mut residents = Vec::with_capacity(processes);
        for _ in 0..processes {
            residents.push(Some(ResidentWorker::spawn(&confinement, &worker, &workdir, &trace_out, &worker_args)?));
        }
        let manifest = residents[0].as_ref().expect("just spawned").manifest.clone();
        // Every process must be the same class: a pool whose members disagree answers one class
        // with several models.
        if residents.iter().flatten().any(|r| r.manifest != manifest) {
            return Err("the pool's workers announced different manifests".to_string());
        }
        Ok(Self {
            worker,
            workdir,
            trace_out,
            confinement,
            worker_args,
            pool: pool::SlotPool::new(residents),
            manifest,
            answer_only: std::sync::atomic::AtomicBool::new(true),
        })
    }

    fn manifest(&self) -> &PalwFpWorkerManifestV1 {
        &self.manifest
    }

    fn processes(&self) -> usize {
        self.pool.slots()
    }

    fn waiting(&self) -> usize {
        self.pool.waiting()
    }

    fn answer_only_supported(&self) -> bool {
        self.answer_only.load(Ordering::Relaxed)
    }

    fn run(
        &self,
        request: &PalwFpWorkerRequestV3,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        on_token: &mut dyn FnMut(u32, &[u8]),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<PalwFpWorkerResultV3, String> {
        let mut guard = self.pool.acquire();
        // **A request whose client left while it waited for this slot is not run** (RFC-0001 §2.7): the slot goes straight
        // to the next in line and the worker is untouched.
        if cancelled() {
            return Err(serving::CANCELLED_BY_CLIENT.to_string());
        }
        let slot = guard.get_mut();
        if slot.is_none() {
            *slot = Some(ResidentWorker::spawn(&self.confinement, &self.worker, &self.workdir, &self.trace_out, &self.worker_args)?);
        }
        let outcome = slot.as_mut().expect("just spawned").run_job(request, prompt_ids_form, on_token);
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

    /// RFC-0001 §2.6/§2.7: the answer with no commitment, on the next idle worker.
    fn run_answer(
        &self,
        request: &PalwFpWorkerRequestV3,
        on_token: &mut dyn FnMut(u32, &[u8]),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<AnswerRun, String> {
        let mut guard = self.pool.acquire();
        if cancelled() {
            return Err(serving::CANCELLED_BY_CLIENT.to_string());
        }
        let slot = guard.get_mut();
        if slot.is_none() {
            *slot = Some(ResidentWorker::spawn(&self.confinement, &self.worker, &self.workdir, &self.trace_out, &self.worker_args)?);
        }
        let outcome = slot.as_mut().expect("just spawned").run_answer(request, on_token);
        match &outcome {
            Ok(AnswerRun::Unsupported) => self.answer_only.store(false, Ordering::Relaxed),
            Err(e) if !e.starts_with("the worker refused the job") => {
                *slot = None;
                eprintln!("[misaka-palw-gateway] the resident worker was dropped after a transport failure: {e}");
            }
            _ => {}
        }
        outcome
    }
}

impl WorkerSupervisor {
    /// RFC-0001 §2.7 stage 2: a batch of answers on the next idle worker, decoded together.
    fn run_answer_batch(
        &self,
        requests: &[PalwFpWorkerRequestV3],
        on_token: &mut dyn FnMut(usize, u32, &[u8]),
    ) -> Result<AnswerBatchRun, String> {
        let mut guard = self.pool.acquire();
        let slot = guard.get_mut();
        if slot.is_none() {
            *slot = Some(ResidentWorker::spawn(&self.confinement, &self.worker, &self.workdir, &self.trace_out, &self.worker_args)?);
        }
        let outcome = slot.as_mut().expect("just spawned").run_answer_batch(requests, on_token);
        match &outcome {
            Ok(AnswerBatchRun::Unsupported) => self.answer_only.store(false, Ordering::Relaxed),
            Err(e) if !e.starts_with("the worker refused the job") => {
                *slot = None;
                eprintln!("[misaka-palw-gateway] the resident worker was dropped after a transport failure: {e}");
            }
            _ => {}
        }
        outcome
    }
}

impl WorkerSupervisor {
    /// RFC-0001 §2.8: an embedding on the next idle worker.
    fn run_embed(&self, request: &kaspa_consensus_core::palw_freeprompt_v3::PalwFpEmbedRequestV1) -> Result<EmbedRun, String> {
        let mut guard = self.pool.acquire();
        let slot = guard.get_mut();
        if slot.is_none() {
            *slot = Some(ResidentWorker::spawn(&self.confinement, &self.worker, &self.workdir, &self.trace_out, &self.worker_args)?);
        }
        let outcome = slot.as_mut().expect("just spawned").run_embed(request);
        if let Err(e) = &outcome
            && !e.starts_with("the worker refused the job")
        {
            *slot = None;
            eprintln!("[misaka-palw-gateway] the resident worker was dropped after a transport failure: {e}");
        }
        outcome
    }
}

/// The most worker processes one gateway runs for one class (`--worker-processes`).
const MAX_WORKER_PROCESSES: usize = 16;

/// **What the entrance needs of a pool of resident workers** — and nothing else, so the whole chat path (admission, the job
/// built from the request, the worker's frames, the bindings, the outbox) runs unchanged against an in-process worker in a
/// test. The shipped implementation is [`WorkerSupervisor`], whose methods these delegate to.
trait JobRunner: Sync {
    fn manifest(&self) -> &PalwFpWorkerManifestV1;
    fn processes(&self) -> usize;
    fn waiting(&self) -> usize;
    fn answer_only_supported(&self) -> bool;
    /// One committed job. `cancelled` is asked once the slot is granted and before the worker is touched.
    fn run(
        &self,
        request: &PalwFpWorkerRequestV3,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        on_token: &mut dyn FnMut(u32, &[u8]),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<PalwFpWorkerResultV3, String>;
    fn run_answer(
        &self,
        request: &PalwFpWorkerRequestV3,
        on_token: &mut dyn FnMut(u32, &[u8]),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<AnswerRun, String>;
    fn run_answer_batch(
        &self,
        requests: &[PalwFpWorkerRequestV3],
        on_token: &mut dyn FnMut(usize, u32, &[u8]),
    ) -> Result<AnswerBatchRun, String>;
    fn run_embed(&self, request: &kaspa_consensus_core::palw_freeprompt_v3::PalwFpEmbedRequestV1) -> Result<EmbedRun, String>;
}

impl JobRunner for WorkerSupervisor {
    fn manifest(&self) -> &PalwFpWorkerManifestV1 {
        WorkerSupervisor::manifest(self)
    }
    fn processes(&self) -> usize {
        WorkerSupervisor::processes(self)
    }
    fn waiting(&self) -> usize {
        WorkerSupervisor::waiting(self)
    }
    fn answer_only_supported(&self) -> bool {
        WorkerSupervisor::answer_only_supported(self)
    }
    fn run(
        &self,
        request: &PalwFpWorkerRequestV3,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        on_token: &mut dyn FnMut(u32, &[u8]),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<PalwFpWorkerResultV3, String> {
        WorkerSupervisor::run(self, request, prompt_ids_form, on_token, cancelled)
    }
    fn run_answer(
        &self,
        request: &PalwFpWorkerRequestV3,
        on_token: &mut dyn FnMut(u32, &[u8]),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<AnswerRun, String> {
        WorkerSupervisor::run_answer(self, request, on_token, cancelled)
    }
    fn run_answer_batch(
        &self,
        requests: &[PalwFpWorkerRequestV3],
        on_token: &mut dyn FnMut(usize, u32, &[u8]),
    ) -> Result<AnswerBatchRun, String> {
        WorkerSupervisor::run_answer_batch(self, requests, on_token)
    }
    fn run_embed(&self, request: &kaspa_consensus_core::palw_freeprompt_v3::PalwFpEmbedRequestV1) -> Result<EmbedRun, String> {
        WorkerSupervisor::run_embed(self, request)
    }
}

/// What a request carries besides its content: whether its client is still there, and where its status is recorded.
struct RequestCtx<'a> {
    link: &'a dyn serving::ClientLink,
}

// ---------------------------------------------------------------------------------------------
// OpenAI-compatible request/response shapes: the request lives in `surface` (ADR-0096 Decision 1);
// the sampler rule stays here, spelled once, and `surface::admit_request` calls it.
// ---------------------------------------------------------------------------------------------

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
/// **The privacy mode a job may declare on THIS chain** (ADR-0077 Decision 16), decided from the
/// configuration and the chain before the model is loaded — `sampling_from_request`'s shape, for
/// its reason: a mode-2 commitment the chain will not extract is an inference spent for nothing,
/// so the refusal names the fence and costs a 4xx.
fn privacy_mode_for_request(config: &Config, facts: &chain::ChainFacts) -> Result<u8, String> {
    if config.privacy_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PANEL_DA && !facts.panel_da_armed {
        return Err("this gateway is configured for --privacy panel-da, and the chain has not armed palw_panel_da (ADR-0077 D16): a \
             private commitment would not become a claim, so the job is refused before the inference"
            .to_string());
    }
    // **ADR-0118 Decision 5: a held class's ids do not ride a PublicDa carrier on a network
    // minted flat.** Its jobs commit Merkle ids, and the chain's stateless check holds no class
    // and reads the network's flat form — so the carrier would be refused at the door after the
    // inference had been paid for. Refused here, by name, before it.
    if config.privacy_mode != kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PANEL_DA && facts.public_ids_cannot_ride() {
        return Err("this class is under the held regime and commits its prompt ids as a tiled Merkle root, and this network's \
             carriers are checked under its flat form (ADR-0118 Decision 5): a PublicDa commitment of it would be refused at the \
             door, so the job is refused before the inference — serve this class with --privacy panel-da"
            .to_string());
    }
    Ok(config.privacy_mode)
}

fn sampling_from_request(chat: &ChatRequest, facts: &chain::ChainFacts) -> Result<([u8; 32], u32), String> {
    use kaspa_consensus_core::palw_decode_select_v2::{PALW_DECODE_SEED_GREEDY, PALW_DECODE_T_ONE, PALW_DECODE_TEMPERATURE_GREEDY};
    let temperature_q = match chat.temperature {
        None => PALW_DECODE_TEMPERATURE_GREEDY,
        Some(t) if t.is_nan() || !(0.0..=MAX_TEMPERATURE).contains(&t) => {
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
    /// The raw `Idempotency-Key` header, if one was sent (validated by [`idempotency::IdempotencyKey::parse`]).
    idempotency_key: Option<String>,
}

/// **A request line and a header line are bounded** (mainnet audit, 2026-09-05).
///
/// `BufRead::read_line` grows its `String` until it meets a newline. The body below is capped and
/// the header COUNT is capped, but each line was not: one unauthenticated connection sending bytes
/// with no `\n` grew a `String` until the public entrance died, and the 30 s read timeout only
/// bounds how long it has to do it in. 8 KiB is the same order as nginx's
/// `large_client_header_buffers`, so no request a client legitimately sends is near it.
const MAX_REQUEST_LINE_BYTES: u64 = 8 * 1024;

/// Read one CRLF-terminated line, refusing rather than growing past the cap.
///
/// Generic over `BufRead` so both arms are reachable from a test; `read_http_request` is the only
/// production caller and it passes the socket's `BufReader`.
fn read_capped_line<R: BufRead>(reader: &mut R, what: &str) -> Result<String, String> {
    let mut line = String::new();
    let read = reader.take(MAX_REQUEST_LINE_BYTES).read_line(&mut line).map_err(|e| format!("cannot read {what}: {e}"))?;
    if read as u64 == MAX_REQUEST_LINE_BYTES && !line.ends_with('\n') {
        return Err(format!("{what} exceeds the {MAX_REQUEST_LINE_BYTES}-byte cap"));
    }
    Ok(line)
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let request_line = read_capped_line(&mut reader, "the request line")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    let path = parts.next().unwrap_or("").to_string();
    let mut content_length: usize = 0;
    let mut transfer_encoding_chunked = false;
    let mut idempotency_key: Option<String> = None;
    let mut headers_read = 0usize;
    loop {
        let line = read_capped_line(&mut reader, "a request header")?;
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
            } else if name.eq_ignore_ascii_case(idempotency::IDEMPOTENCY_HEADER) {
                // A second header of the same name is refused rather than merged: two keys are two answers to "which request".
                if idempotency_key.replace(value.trim().to_string()).is_some() {
                    return Err("the Idempotency-Key header was sent twice".into());
                }
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
    Ok(HttpRequest { method, path, body, idempotency_key })
}

/// **Every JSON response goes through [`serving::render_head`]**, so a 503 or a 429 carries its `Retry-After` whichever call
/// site produced it (RFC-0001 §2.7: the queue's refusal is "a 503 with a Retry-After", and for a long while it was a 503
/// without one).
fn respond(stream: &mut TcpStream, status: &str, body: &serde_json::Value) {
    respond_with(stream, status, body, None, &[]);
}

/// [`respond`] with an explicit `Retry-After` and extra headers (e.g. `idempotent-replayed`).
fn respond_with(stream: &mut TcpStream, status: &str, body: &serde_json::Value, retry_after: Option<u32>, extra: &[(&str, &str)]) {
    let bytes = body.to_string().into_bytes();
    let head = serving::render_head(status, "application/json", bytes.len(), retry_after, extra);
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&bytes);
    let _ = stream.flush();
}

/// A binary body (ADR-0078 Decision 6's fetch handle: the artifact by its derived id).
fn respond_bytes(stream: &mut TcpStream, status: &str, content_type: &str, bytes: &[u8]) {
    let head = serving::render_head(status, content_type, bytes.len(), None, &[]);
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(bytes);
    let _ = stream.flush();
}

/// ADR-0097 Decision 2: the three bounds of THIS process, copied for the surface's limits object.
fn surface_limits(config: &Config) -> surface::SurfaceLimits {
    surface::SurfaceLimits {
        max_decode_cap: config.max_decode_cap,
        max_decode_default: config.max_decode_default,
        max_prompt_bytes: config.max_prompt_bytes,
    }
}

/// The one spelling lives in `surface` (ADR-0097 Decision 2), so a refusal and an error cannot drift.
fn error_body(message: &str) -> serde_json::Value {
    surface::error_body(message)
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
    /// One more piece of the answer, for the person watching. `false` means it could not be delivered (the client is gone):
    /// the run is NOT interrupted — the resident worker has no cancel frame, so it finishes and its result is discarded
    /// (`serving::CANCELLED_BY_CLIENT`) — but nothing more is written and no commitment will be.
    fn delta(&mut self, _text: &str) -> bool {
        true
    }
}

struct BufferedSink;
impl ChatSink for BufferedSink {}

struct SseSink<'a> {
    stream: &'a mut TcpStream,
    id: String,
    model: String,
    started: bool,
    /// A write failed: the client is gone. Latched; nothing further is written.
    broken: bool,
}

impl SseSink<'_> {
    fn head(&mut self) {
        let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n";
        self.write(head.as_bytes());
        self.started = true;
    }

    fn write(&mut self, bytes: &[u8]) {
        if self.broken {
            return;
        }
        if self.stream.write_all(bytes).and_then(|()| self.stream.flush()).is_err() {
            self.broken = true;
        }
    }

    fn event(&mut self, value: &serde_json::Value) {
        self.write(format!("data: {value}\n\n").as_bytes());
    }

    fn chunk(&mut self, delta: serde_json::Value, finish_reason: serde_json::Value) {
        // **A streamed piece is PROVISIONAL**: `misaka.status` says so on every chunk, so a client that renders tokens as they
        // arrive can never mistake them for a committed (let alone final) result — the commitment does not exist until the
        // run ends, and the terminal event says what became of it.
        let value = serde_json::json!({
            "id": self.id,
            "object": "chat.completion.chunk",
            "model": self.model,
            "choices": [{ "index": 0, "delta": delta, "finish_reason": finish_reason }],
            "misaka": { "status": status::RequestStatus::Streaming.as_str(), "final": false },
        });
        self.event(&value);
    }

    /// OpenAI's `stream_options.include_usage` chunk: an empty `choices` and the `usage` counts.
    fn usage_chunk(&mut self, usage: serde_json::Value) {
        let value = serde_json::json!({
            "id": self.id,
            "object": "chat.completion.chunk",
            "model": self.model,
            "choices": [],
            "usage": usage,
        });
        self.event(&value);
    }

    fn done(&mut self) {
        self.write(b"data: [DONE]\n\n");
    }
}

impl ChatSink for SseSink<'_> {
    fn delta(&mut self, text: &str) -> bool {
        self.chunk(serde_json::json!({ "content": text }), serde_json::Value::Null);
        !self.broken
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

/// `chat` is the parsed request and `admitted` is what `surface::admit_request` made of it: every
/// refusal the surface can raise has already been raised, before the queue and before the worker
/// (ADR-0096 invariant 6). `facts` were read once, by the caller, for this job.
/// Everything a job needs before any worker is asked: the prompt plan, the decode limit and the
/// worker request, built once for a single choice and once per candidate (RFC-0001 §2.4).
struct PreparedJob {
    plan: PromptPlan,
    decode_limit: u32,
    request: PalwFpWorkerRequestV3,
    anchor_daa: u64,
}

fn prepare_request(
    config: &Config,
    identity: &Identity,
    manifest: &PalwFpWorkerManifestV1,
    facts: &chain::ChainFacts,
    admitted: &AdmittedRequest,
    sampling: ([u8; 32], u32),
) -> Result<PreparedJob, String> {
    // RFC-0001 §2.11: the request's images are decoded tensors, committed as a V5 job's slot references; the family worker
    // behind this gateway reads token ids and runs no image encoder, so a request that carries one is refused HERE, by name,
    // rather than answered as if the picture had been read.
    if !admitted.images.is_empty() {
        return Err(format!(
            "this request carries {} image(s) and the class behind this gateway has no image slots: its worker runs no image encoder, \
             so the images would be dropped from a prompt the person wrote (RFC-0001 §2.11; V5 slots are a class's, RFC-0003 §II.2.1)",
            admitted.images.len()
        ));
    }
    // ADR-0096 Decision 2: tool turns and the tool list become the model's own text; Decision 3:
    // the format instruction rides the system turn as text. Both BEFORE the template, which then
    // sees plain turns and nothing else.
    let mut tool_turns = wire::render_tools_into_turns(&admitted.turns, &admitted.tools, &admitted.tool_choice)?;
    if let Some(format) = &admitted.format {
        wire::append_to_system_turn(&mut tool_turns.turns, &format.instruction());
    }
    // RFC-0001 §2.9: the sidecar's chat template wins over the built-in one; absent, the model's own
    // selection (`wire::build_prompt`) is what it always was.
    let plan: PromptPlan = match config.sidecar.as_ref().and_then(|s| s.template.as_ref().zip(s.template_ids)) {
        Some((spec, ids)) => wire::build_prompt_with_sidecar_spec(manifest, spec, ids, &tool_turns)?,
        None => wire::build_prompt_with_tools(manifest, &tool_turns)?,
    };
    // ADR-0079 Decision 10: every bound is mandatory, and exceeding one is a 4xx rather than a
    // queue. Checked BEFORE the job is sent, which is the point of having it here.
    if plan.displayed_len() > config.max_prompt_bytes {
        return Err(format!(
            "the rendered prompt is {} bytes and the cap is {} — refused before the job is sent",
            plan.displayed_len(),
            config.max_prompt_bytes
        ));
    }
    let decode_limit = admitted.max_tokens.unwrap_or(config.max_decode_default).clamp(1, config.max_decode_cap);

    // ADR-0077 Decision 3: the chain this gateway commits to, read for THIS job.
    if facts.anchor_block == Hash64::default() {
        return Err(facts.read_error.clone().unwrap_or_else(|| "no anchor is available for this job".to_string()));
    }
    let (anchor_block, anchor_daa) = (facts.anchor_block, facts.anchor_daa);
    let mut job_nonce = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut job_nonce);

    // ADR-0082 Decision 11, decided from the request and the CHAIN by `admit_request` — before the
    // model is loaded, so a refusal cost a 4xx rather than an inference.
    let (sampling_seed, temperature_q) = sampling;

    // **RFC-0001 §A: past the decode-rules fence every job is FP Job V4** — its controls in the one
    // canonical form `admit_request` normalized (the no-op when nothing was asked), its stop strings
    // for the worker to spell with the class's tokenizer; below it, a V3 job exactly as before.
    // **ADR-0096 Decisions 7–8: where the network has armed `palw_fp_decode_constraint`, a `response_format` is COMMITTED** — the
    // job is FP job version 6 carrying the compiled constraint, and the answer is the argmax over the lanes it admits.
    let constraint = committed_constraint_v1(admitted, facts, sampling)?;
    let request_version = if constraint.is_some() {
        kaspa_consensus_core::palw_fp_constraint_job_v1::PALW_FP_CONSTRAINT_VERSION
    } else if admitted.decode.is_some() {
        kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V4_VERSION
    } else {
        PALW_FP_V3_VERSION
    };
    let request = PalwFpWorkerRequestV3 {
        version: request_version,
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
        privacy_mode: privacy_mode_for_request(config, facts)?,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed,
        temperature_q,
        input: PalwFpWorkerInputV3::Segments(plan.segments.clone()),
        model_profile_id: manifest.model_profile_id,
        runtime_manifest_hash: manifest.runtime_manifest_hash,
        runtime_class_id: manifest.runtime_class_id,
        shape_profile_id: manifest.shape_profile_id,
        trace_scheme_id: manifest.trace_scheme_id,
        decode: if constraint.is_some() { None } else { admitted.decode.clone() },
        stop_texts: if constraint.is_some() { Vec::new() } else { admitted.stop_texts.iter().map(|t| t.as_bytes().to_vec()).collect() },
        constraint,
    };
    Ok(PreparedJob { plan, decode_limit, request, anchor_daa })
}

/// **The decode constraint a request commits, if the network commits formats** (ADR-0096 Decisions 7–8): `Ok(None)` where
/// the fence is dormant or no format was asked (the advisory path, unchanged); otherwise the canonical bytes of the
/// `response_format`'s automaton — `json_object` the pinned any-object form, `json_schema` its compiled schema (the first
/// subset, `misaka-palw-constraint::compile`). A committed job is a V3 job under a mask: it is greedy, and carries no sampler
/// controls or stop strings — a request that asks for them beside a committed format is refused by name rather than
/// answered under a rule it did not choose. A schema outside the first subset is refused by name.
fn committed_constraint_v1(
    admitted: &AdmittedRequest,
    facts: &chain::ChainFacts,
    sampling: ([u8; 32], u32),
) -> Result<Option<Vec<u8>>, String> {
    use surface::FormatKind;
    if !facts.fp_decode_constraint_armed {
        return Ok(None);
    }
    let Some(format) = &admitted.format else { return Ok(None) };
    let automaton = match (&format.kind, &format.schema) {
        (FormatKind::JsonObject, _) => misaka_palw_constraint::compile::compile_json_object_v1(),
        (FormatKind::JsonSchema, Some(schema)) => misaka_palw_constraint::compile::compile_v1(schema)
            .map_err(|e| format!("this response_format's schema is outside the first constraint subset and cannot be committed: {e}"))?,
        (FormatKind::JsonSchema, None) => return Err("a json_schema format without its schema".to_string()),
    };
    if sampling.1 != kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY {
        return Err("a committed response_format is a greedy job under a mask: a temperature beside it is refused by name".to_string());
    }
    if admitted.decode.as_ref().is_some_and(|d| !d.is_noop()) || !admitted.stop_texts.is_empty() {
        return Err(
            "a committed response_format carries no sampler controls or stop strings (penalties, logit_bias, stop): the job is a V3 job under \
             a mask, and a control beside it would be answered under a rule the request did not choose"
                .to_string(),
        );
    }
    Ok(Some(automaton.to_bytes()))
}

#[allow(clippy::too_many_arguments)]
fn handle_chat(
    config: &Config,
    identity: &Identity,
    worker: &dyn JobRunner,
    budget: &Mutex<PublicJobBudget>,
    facts: &chain::ChainFacts,
    chain_source: &chain::ChainSource,
    chat: &ChatRequest,
    admitted: &AdmittedRequest,
    // ADR-0082 Decision 11's `(sampling_seed, temperature_q)` THIS job runs under — the request's
    // own for a single choice, `H(base_seed ‖ i)` for candidate `i` of `n` (RFC-0001 §2.4).
    sampling: ([u8; 32], u32),
    sink: &mut dyn ChatSink,
    ctx: &RequestCtx<'_>,
) -> Result<serde_json::Value, String> {
    let manifest = worker.manifest();
    let PreparedJob { plan, decode_limit, request, anchor_daa } =
        prepare_request(config, identity, manifest, facts, admitted, sampling)?;
    expire_stale_commitments(&config.outbox, anchor_daa, COMMITMENT_ANCHOR_TTL_DAA);

    // ADR-0077 SA-1 + Decision 3: a stranger's prompt becomes the OPERATOR's claim. Decide BEFORE
    // the inference whether this one may spend exposure — the answer is produced either way; only
    // the commitment is withheld, which is what makes "answer, never commit" a mode and not an
    // outage, and what makes an uncertified class an answer rather than a refusal.
    let mut price = ExposurePrice::resolve(config, facts);
    let mut commit_refusal = facts.commit_refusal();
    if commit_refusal.is_none() {
        commit_refusal = budget.lock().expect("the budget lock is never poisoned").may_commit(config, price).err();
    }
    let (sampling_seed, temperature_q) = sampling;

    // **RFC-0001 §2.6/§2.7: a job that will not be committed is answered, not folded.** The refusal
    // is known before the run (the chain's facts, the budget, `--answer-never-commit`), so the
    // worker is asked for the ANSWER ONLY: the same decoder, no capture, and the KV prefix cache
    // where it holds one. A worker that serves no such path says so once and the committed path
    // below runs as it always did.
    if let Some(why) = &commit_refusal
        && config.answer_fast_path
        && worker.answer_only_supported()
        && let Some(body) = answer_only_response(config, worker, chat, admitted, &request, &plan, why, sink, ctx)?
    {
        budget.lock().expect("the budget lock is never poisoned").answered_without_commit += 1;
        return Ok(body);
    }

    // **Decision 2: the answer streams as it is decoded; the commitment does not exist yet.**
    // A V4 job with stop strings holds its last 16 ids back: a stop sequence ends the run, so it is
    // always the stream's tail, and it is cut from the display once the result names it.
    let eog: BTreeSet<u32> = manifest.eog_token_ids.iter().copied().collect();
    let mut stream = if admitted.stop_texts.is_empty() {
        AnswerStream::new()
    } else {
        AnswerStream::with_stop_holdback(kaspa_consensus_core::palw_decode_pipeline_v4::PALW_DECODE_V4_MAX_STOP_TOKENS)
    };
    // **Cancellation (RFC-0001 §2.7).** `gone` latches the first sign that the person who asked has left: a delta that could
    // not be written, or a probe of the connection. A request that is gone before the slot is granted never runs; one that
    // leaves mid-run is DRAINED (the resident worker has no cancel frame, and killing it would make every dropped
    // connection cost a model re-map) and its result is discarded below — before any outbox file, any budget charge.
    let gone = std::cell::Cell::new(false);
    let is_gone = || gone.get() || ctx.link.is_gone();
    let result = {
        let mut tokens_seen = 0u32;
        let mut on_token = |token_id: u32, rendered: &[u8]| {
            if let Some(delta) = stream.push(token_id, rendered, &eog)
                && !sink.delta(&delta)
            {
                gone.set(true);
            }
            tokens_seen += 1;
            if tokens_seen % 8 == 0 && !gone.get() && ctx.link.is_gone() {
                gone.set(true);
            }
        };
        worker.run(&request, facts.prompt_ids_form(), &mut on_token, &is_gone)?
    };
    if is_gone() {
        // Nothing of this run is kept: the retained trace the worker wrote for a claim that will never exist goes too.
        let _ = std::fs::remove_dir_all(config.outbox.join("traces").join(hex(fp_job_id_v3(&result.job))));
        return Err(serving::CANCELLED_BY_CLIENT.to_string());
    }
    // RFC-0001 §A.3 step 7: where the job's own stop rule ended the answer — derived from the job
    // the worker bound and the ids it committed, never taken from the worker's word.
    let v4_stop = result.job.decode.as_ref().filter(|_| result.job.is_v4()).map(|decode| {
        (
            decode.clone(),
            kaspa_consensus_core::palw_decode_pipeline_v4::decode_answer_stop_v4(
                decode,
                result.job.decode_token_limit,
                u32::MAX,
                &result.output_token_ids,
            ),
        )
    });
    let stop_len = match &v4_stop {
        Some((decode, Ok(stop))) => match stop.reason {
            kaspa_consensus_core::palw_decode_pipeline_v4::PalwFpDecodeStopReasonV1::StopSequence { index } => {
                decode.stop_sequences.get(index as usize).map(Vec::len)
            }
            _ => None,
        },
        Some((_, Err(why))) => return Err(format!("the worker's V4 answer does not end where its job's stop rule ends it: {why}")),
        None => None,
    };
    if let Some(delta) = stream.finish_with_stop(stop_len) {
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
    // The operator's `--class-leaves` where given, else the chain's own canonical job — the flag
    // was "for the quanta display" and defaulted to 0, so every gateway that did not set it
    // printed zero quanta for every job.
    let class_leaves = if config.class_leaves > 0 { config.class_leaves } else { facts.class_canonical_leaves };
    let quanta = if class_leaves == 0 {
        0
    } else {
        let cap = if facts.fp_max_quanta_per_receipt > 0 { facts.fp_max_quanta_per_receipt } else { u32::MAX };
        fp_quanta_v3(work_leaves, fp_class_quantum_leaves_v1(class_leaves, quanta_per_job), cap)
    };
    // **ADR-0148: the chain's own price for THIS job, where the node answers it.** The leaves
    // figures above are the lane's price below the canonical-work fence only; past it the ledger
    // prices compute in the network's unit, and a wide model's claim reserves several times its
    // leaves. The node prices the job with the fold's own function at the virtual's DAA — the same
    // prefix accounting, the same quanta, the same reservation — so what is checked here is what
    // will be reserved there. A node older than the op answers nothing, and the estimate stands.
    // Asked with the ids the carrier will hold — none under PanelDa — because those are the ids
    // the fold's prefix accounting reads; the full prompt would quote a different claim.
    let carried_ids = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_carried_prompt_ids_v1(&result.job, &result.prompt_token_ids);
    let chain_price = chain_source
        .fp_job_price(&carried_ids, result.job.prompt_tokens, commitment.decode_tokens_executed, work_leaves)
        .filter(|_| commit_refusal.is_none());
    let quanta = chain_price.as_ref().filter(|p| p.priced).map(|p| p.quanta).unwrap_or(quanta);
    if let Some(refused) = chain_price.as_ref().filter(|p| !p.priced) {
        commit_refusal = Some(format!("the chain would refuse this commitment: {}", refused.refusal));
    }
    // **This claim's own price, now that the job has run** (SA-1/SA-7 at their word: refused at
    // the entrance, not at the transition). The entrance above could only price one CANONICAL
    // claim, and a free-prompt claim reserves its own quanta's worth — a 256-token answer on
    // testnet-11's A16 class is five canonical jobs. A gateway that stopped at the lower bound
    // wrote commitments the transition then refused as `FreePromptExposureCeiling`, after the
    // rail had paid to carry them. Re-checked against the room and the window budget with the
    // exact figure, and that figure is what the budget is charged; the operator's own
    // `--claim-exposure-sompi`, where declared, stays the price (it is their bound to set).
    if commit_refusal.is_none()
        && config.claim_exposure_sompi == 0
        && let Some(exact) = chain_price.as_ref().map(|p| p.reserved_sompi).or_else(|| facts.fp_claim_exposure(work_leaves))
    {
        // The room too, when the node answered: it was read with the price, after the job, where
        // `price.room_sompi` is the reading from before the inference started.
        let room_sompi = chain_price
            .as_ref()
            .and_then(|p| p.bond_room_sompi)
            .map(|room| u64::try_from(room).unwrap_or(u64::MAX))
            .unwrap_or(price.room_sompi);
        let exact = ExposurePrice { room_sompi, claim_sompi: u64::try_from(exact).unwrap_or(u64::MAX) };
        commit_refusal = budget.lock().expect("the budget lock is never poisoned").may_commit(config, exact).err().map(|why| {
            format!("{why} (this answer's claim is {quanta} quanta; one canonical claim would have been {})", price.claim_sompi)
        });
        price = exact;
    }

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
    // The bytes handed to the derivation are `result.rendered` ITSELF, not the lossy string above:
    // `misaka_palw_derive::render_answer_v1` (the join a bound verifier recomputes) returns the raw
    // rendering, and an answer that ends mid-sequence or spells an id to invalid UTF-8 would give
    // the lossy form a different `dsl_hash` — a MISMATCH against an honest executor.
    //
    // **Gated on `commit_refusal.is_none()`.** A derivation names a claim, and consensus refuses a
    // `DerivedArtifactV1` whose claim never entered the state (`DerivedClaimMissing`). Deriving
    // for a commitment this gateway has just declined to write would put an object in the outbox
    // that no chain can ever accept, and would spend the derivation budget doing it.
    //
    // **ADR-0096 Decision 3, advisory mode: "the `json` kind (Decision 9) is derived when it
    // parses."** A request that carried a `response_format` and named no kind is derived under
    // `json/canonical/v1` — the transformer by name, so the entrance does not depend on which
    // transformer a kind row happens to resolve to first. An explicit `derive` wins: a person who
    // asked for a scene under a schema gets the scene. A parse failure is `Outcome::Refused` and
    // never an error (X4); the claim is untouched either way.
    let derive_spec: Option<&str> =
        chat.derive.as_deref().or_else(|| admitted.format.as_ref().map(|_| misaka_palw_derive::kinds::json::TRANSFORMER_NAME));
    let derivation = match (derive_spec, commit_refusal.is_none()) {
        (Some(spec), true) => Some(derive::run(
            spec,
            &derive::DeriveConfig { seed: config.derive_seed, serve_dsl: chat.serve_dsl },
            &misaka_palw_derive::ClaimBinding {
                network_domain: identity.network_domain,
                claim_id,
                output_root: result.output_root,
                executor_pubkey: identity.executor_pubkey.clone(),
            },
            &result.rendered,
            &config.outbox,
            &artifact_stem,
        )?),
        _ => None,
    };
    let derive_refusal = match (derive_spec, commit_refusal.as_deref()) {
        (Some(_), Some(why)) => Some(format!(
            "nothing was derived: a derivation names a claim, and this answer did not become one ({why}) — consensus would \
             refuse the object as DerivedClaimMissing"
        )),
        _ => None,
    };
    let (job_context_hash, family, job_context) = derive::read_worker_manifest(&config.outbox.join("traces").join(hex(job_id)));

    // **ADR-0096 Decisions 2 and 3, on the SHOWN answer.** Both read the display string and write
    // new strings: the ids and the bytes the commitment covers are untouched, which is why they
    // run here, after the bindings and after the commitment was (or was not) written.
    let shown = if streamed_checked { stream.shown() } else { wire::display_trim(&rendered_string).to_string() };
    let parsed = wire::parse_tool_calls(&shown);
    let format_report = admitted.format.as_ref().map(|format| format.check(&shown));
    let format_json = admitted.format.as_ref().zip(format_report.as_ref()).map(|(format, report)| format.report_json(report));
    let tool_calls_json: Vec<serde_json::Value> = parsed
        .calls
        .iter()
        .enumerate()
        .map(|(index, call)| {
            let arguments = misaka_palw_constraint::canonical::to_rfc8785(&call.arguments)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_else(|_| call.arguments.to_string());
            serde_json::json!({
                "id": wire::tool_call_id(job_id, index as u32),
                "type": "function",
                "function": { "name": call.name, "arguments": arguments },
            })
        })
        .collect();
    let summary = serde_json::json!({
        "schema": "misaka.palw.fp-v3-gateway-artifact.v1",
        "fp_job_id": hex(job_id),
        "template_id": plan.template_id,
        "prompt_tokens": result.job.prompt_tokens,
        "decode_tokens_executed": result.decode_tokens_executed,
        "decode_token_limit": result.job.decode_token_limit,
        "stop_reason": match result.stop_reason { PalwFpStopReasonV3::ExactBudgetReached => "exact_budget", PalwFpStopReasonV3::EndOfGeneration => "end_of_generation" },
        "fp_claim_id": hex(claim_id),
        // The bond the claim names, for the status route: a chain row naming ANOTHER bond is not this gateway's claim.
        "executor_bond": format!("{}:{}", result.job.executor_bond.transaction_id, result.job.executor_bond.index),
        "trace_root": hex(result.trace_root),
        "output_root": hex(result.output_root),
        "schedule_root": hex(result.schedule_root),
        "trace_manifest_root": hex(result.trace_manifest_root),
        "trace_chunk_count": result.trace_chunk_count,
        "trace_retention_daa": commitment.trace_retention_daa,
        "trace_dir": config.outbox.join("traces").join(hex(job_id)).display().to_string(),
        "work_leaves": work_leaves,
        "class_leaves": class_leaves,
        "quanta_at_configured_quantum": quanta,
        // What this claim reserves on the bond, as the room and budget were checked against it —
        // the exact figure once the job ran, the canonical one where the chain did not report
        // the ingredients (or the operator declared --claim-exposure-sompi).
        "claim_exposure_sompi": price.claim_sompi,
        "answer_untrimmed": rendered_string,
        "job_context_hash": job_context_hash,
        // ADR-0078 X6's binding leg: the whole `PalwJobContextV2` as borsh hex, beside its hash.
        // `output_root` needs only the hash; the binding needs `tokenizer_id`, which is a FIELD
        // and is not recoverable from a hash. `null` here is a job whose worker predates the
        // field, and a verifier then says the binding was not checked rather than passing.
        "job_context": job_context,
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
        // ADR-0096: what the entrance made of the shown answer — presentation, never commitment.
        "tool_calls_parsed": parsed.calls.len(),
        "tool_calls_unparsed": parsed.unparsed_blocks,
        "format": format_json,
        "pending_for_chain_submission": [
            "ML-DSA-87 signature over fp_claim_id (signer sidecar, or the rail's --bond-key-seed)",
            "misaka-palw-fp-rail --watch <outbox> (every job), or --artifact <stem> ... --submit --rpc <host:port> (this one)",
        ],
    });
    std::fs::write(&artifact_json, serde_json::to_vec_pretty(&summary).unwrap())
        .map_err(|e| format!("cannot write {}: {e}", artifact_json.display()))?;
    // **One line per job, in the log an operator already watches.** The gateway printed nothing
    // per job, so the only line an operator saw was the worker's `v3 executed` — which is printed
    // for every job, committed or not, and says nothing about the chain. Outside operators read it
    // as "mined". This says which of the two happened, and what carries a commitment onward. Ids
    // and counts only: nothing of the prompt or the answer (ADR-0079 SA-7).
    match &commit_refusal {
        None => eprintln!(
            "[misaka-palw-gateway] {artifact_stem}: committed claim {} ({quanta} quanta, {} sompi of exposure) — in the outbox, \
             NOT on chain until misaka-palw-fp-rail submits it (--watch {})",
            hex(claim_id),
            price.claim_sompi,
            config.outbox.display()
        ),
        Some(why) => eprintln!("[misaka-palw-gateway] {artifact_stem}: answered, not committed — {why}"),
    }
    // ADR-0122 Decision 8: the job's own lines, beside the prose above — by its job id from the run,
    // and by the claim id too from the commitment on, the id every later stage of it carries.
    // `key=value`, no spaces in a value; the prose line says why a job was not committed.
    let job16 = &hex(job_id)[..16];
    eprintln!("[misaka-palw-gateway] event job={job16} lane=prompt stage=EXECUTED leaves={work_leaves}");
    match &commit_refusal {
        None => {
            let claim = hex(claim_id);
            eprintln!(
                "[misaka-palw-gateway] event work={} job={job16} lane=prompt stage=COMMITTED quanta={quanta} claim={claim}",
                &claim[..16]
            );
        }
        Some(_) => eprintln!("[misaka-palw-gateway] event job={job16} lane=prompt stage=NOT_COMMITTED"),
    }

    let finish_reason = if !parsed.calls.is_empty() {
        "tool_calls" // ADR-0096 Decision 2: OpenAI's word for an answer that made calls
    } else if stop_len.is_some() {
        "stop" // RFC-0001 §A.3: a stop sequence ended the run
    } else {
        match result.stop_reason {
            PalwFpStopReasonV3::EndOfGeneration => "stop",
            PalwFpStopReasonV3::ExactBudgetReached => {
                if shown.len() < rendered_string.trim_end().len() {
                    "stop" // the guard or an EOG id ended the shown answer; the budget ended the run
                } else {
                    "length"
                }
            }
        }
    };
    // ADR-0096 Decision 4: what was asked, beside what ran. Nobody is told a false thing about
    // the decode, because the greedy rule the seat replays is printed next to the request's knobs.
    let sampling = serde_json::json!({
        "requested": admitted.sampling_requested,
        "applied": {
            "temperature": temperature_q as f64 / kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_T_ONE as f64,
            "seed": faster_hex::hex_string(&sampling_seed),
        },
        "reason": if facts.fp_decode_rules_armed {
            "palw_fp_decode_rules is armed on this network (ADR-0082 Decision 11): the job carries the temperature and seed it asked for"
        } else {
            "palw_fp_decode_rules is not armed on this network (ADR-0082 Decision 11): the seat replays a greedy decode and nothing else"
        },
        "not_a_rule_on_this_lane": admitted.not_a_rule_on_this_lane,
        // RFC-0001 §A: the job's decode controls as the chain holds them (V4 only), and what the
        // stop strings are — token sequences, matched only as they tokenize alone.
        "decode_v4": result.job.decode.as_ref().filter(|_| result.job.is_v4()).map(|decode| serde_json::json!({
            "config": decode,
            "stop": v4_stop.as_ref().and_then(|(_, stop)| stop.as_ref().ok()).map(|stop| serde_json::json!(stop)),
            "stop_strings": admitted.stop_texts,
            "stop_note": "each stop string is the token sequence it encodes to ALONE under this class's tokenizer; generated \
                          text that tokenizes the same characters differently does not match and does not stop",
        })),
    });
    let tool_choice = (admitted.tool_choice_given || !admitted.tools.is_empty())
        .then(|| serde_json::json!({ "requested": admitted.tool_choice.requested_json(), "enforcement": "advisory" }));
    // **What this response is worth** (RFC-0001 §2.7): a commitment in the outbox is `committed` and an answer that became no
    // claim is `answered`; NEITHER is final. `GET /v1/requests/<id>` follows it from here, and only the chain's Final is final.
    let request_status = if commit_refusal.is_none() { status::RequestStatus::Committed } else { status::RequestStatus::Answered };
    let misaka = serde_json::json!({
        "request": {
            "id": format!("{}{}", status::COMPLETION_PREFIX, &hex(job_id)[..24]),
            "status": request_status.as_str(),
            "final": false,
            "status_route": format!("/v1/requests/{}{}", status::COMPLETION_PREFIX, &hex(job_id)[..24]),
        },
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
        // And what X6's BINDING leg needs on top of that: the context itself (borsh hex), whose
        // `tokenizer_id` says which tokenizer may render those ids into the derivation's DSL. A
        // consumer derives the hash above from these bytes rather than trusting the pair, so the
        // tokenizer it renders under and the root it checks cannot come from two contexts:
        // `misaka palw derived-verify --tokenizer <tokenizer.json>` then reports
        // `binding_checked: true`, which without this field no production job could reach.
        "job_context": job_context,
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
        // ADR-0096 Decision 2: the shown answer before any `<tool_call>` block was lifted out of
        // it — the text the model actually said, whole.
        "answer_untrimmed": rendered_string,
        "tool_choice": tool_choice,
        "tool_calls_unparsed": parsed.unparsed_blocks,
        // ADR-0096 Decision 3: which mode served the format, and what the check found.
        "format": format_json,
        // ADR-0096 Decisions 1 and 4.
        "sampling": sampling,
        "ignored_fields": admitted.ignored_fields,
        "sidecar": admitted.sidecar_report,
        // ADR-0097 Decision 2: what was asked for the answer's length, beside what ran. The cap
        // is the operator's and the window is the class's; a request past either is clamped, and
        // a clamp nobody is told about is a downgrade nobody agreed to.
        "decode": {
            "requested_max_tokens": admitted.max_tokens,
            "applied_limit": decode_limit,
            "cap": config.max_decode_cap,
            "clamped": admitted.max_tokens.is_some_and(|asked| asked != decode_limit),
        },
    });
    let mut message = serde_json::json!({ "role": "assistant", "content": shown });
    if !parsed.calls.is_empty() {
        // OpenAI's shape: the calls as `tool_calls[]`, the remaining text as `content` or `null`.
        message["content"] = if parsed.text.is_empty() { serde_json::Value::Null } else { serde_json::json!(parsed.text) };
        message["tool_calls"] = serde_json::Value::Array(tool_calls_json);
    }
    Ok(serde_json::json!({
        "id": format!("palwcmpl-{}", &hex(job_id)[..24]),
        "object": "chat.completion",
        "model": chat.model.clone().unwrap_or_else(|| surface::MODEL_ID.to_string()),
        "choices": [{
            "index": 0,
            "message": message,
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

/// **RFC-0001 §2.6/§2.7: the response for a job answered without a commitment.** `Ok(None)` when
/// the worker serves no answer-only path (the caller runs the committed one). Every binding the
/// committed path makes that does not need a commitment is made here: the prompt ids are the ones
/// the gateway's own template placed (SA-3), and the streamed bytes are the rendering of the ids
/// the worker returned (W5). What is absent is exactly what was refused up front — a trace, a
/// root, a claim — and the response says so in `misaka.not_committed_because`.
#[allow(clippy::too_many_arguments)]
fn answer_only_response(
    config: &Config,
    worker: &dyn JobRunner,
    chat: &ChatRequest,
    admitted: &AdmittedRequest,
    request: &PalwFpWorkerRequestV3,
    plan: &PromptPlan,
    why_not_committed: &str,
    sink: &mut dyn ChatSink,
    ctx: &RequestCtx<'_>,
) -> Result<Option<serde_json::Value>, String> {
    let manifest = worker.manifest();
    let eog: BTreeSet<u32> = manifest.eog_token_ids.iter().copied().collect();
    let mut stream = if admitted.stop_texts.is_empty() {
        AnswerStream::new()
    } else {
        AnswerStream::with_stop_holdback(kaspa_consensus_core::palw_decode_pipeline_v4::PALW_DECODE_V4_MAX_STOP_TOKENS)
    };
    let gone = std::cell::Cell::new(false);
    let is_gone = || gone.get() || ctx.link.is_gone();
    let answer = {
        let mut on_token = |token_id: u32, rendered: &[u8]| {
            if let Some(delta) = stream.push(token_id, rendered, &eog)
                && !sink.delta(&delta)
            {
                gone.set(true);
            }
        };
        match worker.run_answer(request, &mut on_token, &is_gone)? {
            AnswerRun::Answered(answer) => answer,
            AnswerRun::Unsupported => return Ok(None),
        }
    };
    if is_gone() {
        return Err(serving::CANCELLED_BY_CLIENT.to_string());
    }
    Ok(Some(answer_only_body(config, worker, chat, admitted, request, plan, why_not_committed, &mut stream, answer, sink)?))
}

/// The body of an answer-only response, from the run's stream and the worker's answer — one
/// spelling for the single answer and for each candidate of a batch.
#[allow(clippy::too_many_arguments)]
fn answer_only_body(
    config: &Config,
    worker: &dyn JobRunner,
    chat: &ChatRequest,
    admitted: &AdmittedRequest,
    request: &PalwFpWorkerRequestV3,
    plan: &PromptPlan,
    why_not_committed: &str,
    stream: &mut AnswerStream,
    answer: kaspa_consensus_core::palw_freeprompt_v3::PalwFpWorkerAnswerV1,
    sink: &mut dyn ChatSink,
) -> Result<serde_json::Value, String> {
    let manifest = worker.manifest();
    let stop_len = answer.stop_sequence_len.map(|n| n as usize);
    if let Some(delta) = stream.finish_with_stop(stop_len) {
        sink.delta(&delta);
    }
    // W5 without a commitment: the shown bytes are the rendering of the returned ids.
    if stream.streamed() && (stream.ids() != answer.output_token_ids.as_slice() || stream.bytes() != answer.rendered.as_slice()) {
        return Err("W5: the streamed answer is not the rendering of the ids the worker returned".to_string());
    }
    wire::check_committed_prompt_ids(plan, &answer.prompt_token_ids, &wire::control_token_ids(manifest))?;

    let rendered_string = String::from_utf8_lossy(&answer.rendered).into_owned();
    let shown = if stream.streamed() { stream.shown() } else { wire::display_trim(&rendered_string).to_string() };
    let parsed = wire::parse_tool_calls(&shown);
    let format_report = admitted.format.as_ref().map(|format| format.check(&shown));
    let format_json = admitted.format.as_ref().zip(format_report.as_ref()).map(|(format, report)| format.report_json(report));
    let answer_id = hex(answer.request_hash);
    let tool_calls_json: Vec<serde_json::Value> = parsed
        .calls
        .iter()
        .enumerate()
        .map(|(index, call)| {
            let arguments = misaka_palw_constraint::canonical::to_rfc8785(&call.arguments)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_else(|_| call.arguments.to_string());
            serde_json::json!({
                "id": format!("call_{}_{index}", &answer_id[..16]),
                "type": "function",
                "function": { "name": call.name, "arguments": arguments },
            })
        })
        .collect();
    let finish_reason = if !parsed.calls.is_empty() {
        "tool_calls"
    } else if stop_len.is_some() || answer.ended_on_stop_id {
        "stop"
    } else {
        "length"
    };
    let completion_tokens = answer.output_token_ids.len() as u32;
    let prompt_tokens = answer.prompt_token_ids.len() as u32;
    let mut message = serde_json::json!({ "role": "assistant", "content": shown });
    if !parsed.calls.is_empty() {
        message["content"] = if parsed.text.is_empty() { serde_json::Value::Null } else { serde_json::json!(parsed.text) };
        message["tool_calls"] = serde_json::Value::Array(tool_calls_json);
    }
    eprintln!(
        "[misaka-palw-gateway] answer-only {}: prefill {prompt_tokens} (cached {}), decode {completion_tokens} — not committed: {why_not_committed}",
        &answer_id[..16],
        answer.cached_prefix_tokens
    );
    Ok(serde_json::json!({
        "id": format!("palwcmpl-{}", &answer_id[..24]),
        "object": "chat.completion",
        "model": chat.model.clone().unwrap_or_else(|| surface::MODEL_ID.to_string()),
        "choices": [{ "index": 0, "message": message, "finish_reason": finish_reason }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": prompt_tokens + completion_tokens,
        },
        "misaka": {
            "request": {
                "id": format!("{}{}", status::COMPLETION_PREFIX, &answer_id[..24]),
                "status": status::RequestStatus::Answered.as_str(),
                "final": false,
            },
            "committed": false,
            "not_committed_because": why_not_committed,
            "output_token_ids": answer.output_token_ids,
            "answer_untrimmed": rendered_string,
            "template_id": plan.template_id,
            "format": format_json,
            "tool_calls_unparsed": parsed.unparsed_blocks,
            "ignored_fields": admitted.ignored_fields,
            "sidecar": admitted.sidecar_report,
            // RFC-0001 §2.6/§2.7, I-3: a node-local fact about HOW the answer was served — never an
            // input to any claim, because there is none.
            "serving": {
                "answer_only": true,
                "cached_prefix_tokens": answer.cached_prefix_tokens,
                "execute_ms": answer.execute_ms,
                "worker_processes": worker.processes(),
            },
            "sampling": {
                "requested": admitted.sampling_requested,
                "applied": {
                    "temperature": request.temperature_q as f64 / kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_T_ONE as f64,
                    "seed": faster_hex::hex_string(&request.sampling_seed),
                },
                "not_a_rule_on_this_lane": admitted.not_a_rule_on_this_lane,
            },
            "decode": {
                "requested_max_tokens": admitted.max_tokens,
                "applied_limit": request.decode_token_limit,
                "cap": config.max_decode_cap,
                "clamped": admitted.max_tokens.is_some_and(|asked| asked != request.decode_token_limit),
            },
        },
    }))
}

/// **RFC-0001 §2.7 stage 2 for the `n` candidates of one request**: every candidate's request is
/// built (its own seed, `H(base_seed ‖ i)`), the worker decodes them in one batch, and each answer
/// gets the same response body a single answer-only job would. `Ok(None)` when the worker serves no
/// batch path (the caller runs the candidates as separate jobs).
fn answer_batch_candidates(
    config: &Config,
    identity: &Identity,
    worker: &dyn JobRunner,
    facts: &chain::ChainFacts,
    chat: &ChatRequest,
    admitted: &AdmittedRequest,
    why_not_committed: &str,
) -> Result<Option<Vec<serde_json::Value>>, String> {
    let manifest = worker.manifest();
    let (base_seed, temperature_q) = admitted.sampling;
    let mut prepared = Vec::with_capacity(admitted.candidates as usize);
    for i in 0..admitted.candidates {
        prepared.push(prepare_request(config, identity, manifest, facts, admitted, (surface::candidate_seed_v1(&base_seed, i), temperature_q))?);
    }
    let requests: Vec<PalwFpWorkerRequestV3> = prepared.iter().map(|p| p.request.clone()).collect();
    let eog: BTreeSet<u32> = manifest.eog_token_ids.iter().copied().collect();
    let new_stream = || {
        if admitted.stop_texts.is_empty() {
            AnswerStream::new()
        } else {
            AnswerStream::with_stop_holdback(kaspa_consensus_core::palw_decode_pipeline_v4::PALW_DECODE_V4_MAX_STOP_TOKENS)
        }
    };
    let mut streams: Vec<AnswerStream> = (0..requests.len()).map(|_| new_stream()).collect();
    let answers = {
        let mut on_token = |index: usize, token_id: u32, rendered: &[u8]| {
            let _ = streams[index].push(token_id, rendered, &eog);
        };
        match worker.run_answer_batch(&requests, &mut on_token)? {
            AnswerBatchRun::Answered(answers) => answers,
            AnswerBatchRun::Unsupported => return Ok(None),
        }
    };
    let mut bodies = Vec::with_capacity(answers.len());
    for ((answer, prepared), stream) in answers.into_iter().zip(&prepared).zip(streams.iter_mut()) {
        let mut sink = BufferedSink;
        bodies.push(answer_only_body(config, worker, chat, admitted, &prepared.request, &prepared.plan, why_not_committed, stream, answer, &mut sink)?);
    }
    Ok(Some(bodies))
}

/// **RFC-0001 §2.4: `n` candidates are `n` jobs.** Each candidate is its own inference, its own
/// commitment and its own claim, under its own seed `H(base_seed ‖ i)`
/// ([`surface::candidate_seed_v1`]); they run on the worker pool in parallel (the pool's slots and
/// the entrance's in-flight cap bound how many at once) and come back as one `choices` array. The
/// first failure fails the request — the other candidates' outbox files stay (each is a complete,
/// separate job) and the error says so.
#[allow(clippy::too_many_arguments)]
fn handle_chat_candidates(
    config: &Config,
    identity: &Identity,
    worker: &dyn JobRunner,
    budget: &Mutex<PublicJobBudget>,
    facts: &chain::ChainFacts,
    chain_source: &chain::ChainSource,
    chat: &ChatRequest,
    admitted: &AdmittedRequest,
    ctx: &RequestCtx<'_>,
) -> Result<serde_json::Value, String> {
    let (base_seed, temperature_q) = admitted.sampling;
    let n = admitted.candidates;
    // **RFC-0001 §2.7 stage 2: when the refusal to commit is static** (the chain's facts, or
    // `--answer-never-commit` — not the budget, which moves per job) every candidate is an answer
    // and none a claim, so the worker decodes them TOGETHER, each in its own sequence.
    let static_refusal = facts.commit_refusal().or_else(|| {
        config.answer_never_commit.then(|| "this gateway runs in `answer, never commit` mode (ADR-0077 SA-1c)".to_string())
    });
    let bodies_from_batch = match static_refusal {
        Some(why) if config.answer_fast_path && worker.answer_only_supported() => {
            answer_batch_candidates(config, identity, worker, facts, chat, admitted, &why)?
        }
        _ => None,
    };
    let results: Vec<Result<serde_json::Value, String>> = match bodies_from_batch {
        Some(bodies) => bodies.into_iter().map(Ok).collect(),
        None => std::thread::scope(|scope| {
        let handles: Vec<_> = (0..n)
            .map(|i| {
                let seed = surface::candidate_seed_v1(&base_seed, i);
                scope.spawn(move || {
                    let mut sink = BufferedSink;
                    handle_chat(config, identity, worker, budget, facts, chain_source, chat, admitted, (seed, temperature_q), &mut sink, ctx)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("a candidate's job panicked".to_string()))).collect()
        }),
    };
    let mut bodies = Vec::with_capacity(results.len());
    for (i, result) in results.into_iter().enumerate() {
        bodies.push(result.map_err(|e| {
            format!("candidate {i} of {n} failed: {e} (the other candidates are separate jobs; any that completed is in the outbox)")
        })?);
    }
    let mut merged = bodies[0].clone();
    let mut choices = Vec::with_capacity(bodies.len());
    let (mut prompt_tokens, mut completion_tokens) = (0u64, 0u64);
    let mut candidate_reports = Vec::with_capacity(bodies.len());
    for (i, body) in bodies.iter().enumerate() {
        let mut choice = body["choices"][0].clone();
        choice["index"] = serde_json::json!(i);
        choices.push(choice);
        prompt_tokens += body["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
        completion_tokens += body["usage"]["completion_tokens"].as_u64().unwrap_or(0);
        candidate_reports.push(serde_json::json!({
            "index": i,
            "seed": faster_hex::hex_string(&surface::candidate_seed_v1(&base_seed, i as u32)),
            "misaka": body["misaka"].clone(),
        }));
    }
    merged["choices"] = serde_json::Value::Array(choices);
    merged["usage"] = serde_json::json!({
        "prompt_tokens": prompt_tokens,
        "completion_tokens": completion_tokens,
        "total_tokens": prompt_tokens + completion_tokens,
    });
    merged["misaka"]["n"] = serde_json::json!(n);
    merged["misaka"]["candidate_seed_rule"] = serde_json::json!("sha256(\"misaka.palw.fp.n-candidate-seed.v1\" || base_seed || u32_le(i))");
    merged["misaka"]["candidates"] = serde_json::Value::Array(candidate_reports);
    Ok(merged)
}

/// **RFC-0001 §2.8: `POST /v1/embeddings`** — the pooled final-layer hidden state of each input, as
/// OpenAI's list shape. Local serving: no job, no claim, no reward, nothing committed; the response
/// says so (`misaka.committed: false`). Each input is one worker forward pass, run on the pool.
/// `Ok(None)` means the worker serves no embedding path (the route answers 501).
fn handle_embeddings(
    config: &Config,
    worker: &dyn JobRunner,
    request: &surface::EmbeddingsRequest,
    admitted: &surface::AdmittedEmbeddings,
) -> Result<Option<serde_json::Value>, String> {
    use kaspa_consensus_core::palw_embedding_pool_v1::palw_embedding_l2_q24_v1;
    let manifest = worker.manifest();
    let mut data = Vec::with_capacity(admitted.inputs.len());
    let mut prompt_tokens = 0u64;
    let mut cached = 0u64;
    let mut dims = 0usize;
    for (index, text) in admitted.inputs.iter().enumerate() {
        let embed = kaspa_consensus_core::palw_freeprompt_v3::PalwFpEmbedRequestV1 {
            version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_EMBED_REQUEST_VERSION_V1,
            class_id: manifest.class_id,
            shape_profile_id: manifest.shape_profile_id,
            input: PalwFpWorkerInputV3::Text(text.as_bytes().to_vec()),
            pool: admitted.pool as u8,
        };
        let answer = match worker.run_embed(&embed)? {
            EmbedRun::Embedded(answer) => answer,
            EmbedRun::Unsupported => return Ok(None),
        };
        prompt_tokens += answer.prompt_token_ids.len() as u64;
        cached += u64::from(answer.cached_prefix_tokens);
        dims = answer.raw.len();
        let q24 = if admitted.normalize { palw_embedding_l2_q24_v1(&answer.raw) } else { answer.raw.clone() };
        let scale = if admitted.normalize { (1u64 << 24) as f64 } else { 1.0 };
        data.push(serde_json::json!({
            "object": "embedding",
            "index": index,
            "embedding": q24.iter().map(|v| *v as f64 / scale).collect::<Vec<f64>>(),
            // The exact integers the floats are made of (Q24 when normalized, the pooled A16 codes
            // when not): what a client that compares embeddings bit for bit reads.
            "embedding_integers": q24,
        }));
    }
    let _ = config;
    Ok(Some(serde_json::json!({
        "object": "list",
        "data": data,
        "model": request.model.clone().unwrap_or_else(|| surface::MODEL_ID.to_string()),
        "usage": { "prompt_tokens": prompt_tokens, "total_tokens": prompt_tokens },
        "misaka": {
            // RFC-0001 §2.8 / I-3: a local service. There is no job, no claim and no reward here.
            "committed": false,
            "not_committed_because": "embeddings are served locally (RFC-0001 §2.8): the claim form is RFC-0003's embedding profile, not this route",
            "pool": admitted.pool.name(),
            "normalized": admitted.normalize,
            "scale": if admitted.normalize { "q24_unit_vector" } else { "a16_hidden_code" },
            "dimensions": dims,
            "cached_prefix_tokens": cached,
            "ignored_fields": admitted.ignored_fields,
        },
    })))
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
    let mut privacy_mode: u8 = PALW_FP_PRIVACY_PUBLIC_DA;
    let mut per_source_jobs_per_window: u32 = 120;
    let mut worker_processes: usize = 1;
    let mut worker_args: Vec<String> = Vec::new();
    let mut answer_fast_path = true;
    let mut max_connections_per_source = DEFAULT_MAX_CONNECTIONS_PER_SOURCE;
    let mut max_jobs_per_source = DEFAULT_MAX_JOBS_PER_SOURCE;
    let mut sidecar_path: Option<PathBuf> = None;
    let mut cancel_on_disconnect = true;
    let mut finality_depth = status::DEFAULT_FINALITY_DEPTH;
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
            "--privacy" => {
                privacy_mode = match value("--privacy").as_str() {
                    "public-da" => PALW_FP_PRIVACY_PUBLIC_DA,
                    "panel-da" => kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PANEL_DA,
                    other => die(format!("--privacy {other}: expected `public-da` or `panel-da`")),
                }
            }
            "--worker-processes" => worker_processes = value("--worker-processes").parse().unwrap_or_else(|e| die(format!("{e}"))),
            // The KV prefix cache lives in each worker process (RFC-0001 §2.6): the budget is PER
            // process, and these two are forwarded to every one.
            "--kv-cache-budget-mib" => {
                let mib: u64 = value("--kv-cache-budget-mib").parse().unwrap_or_else(|e| die(format!("{e}")));
                worker_args.extend(["--kv-cache-budget-mib".to_string(), mib.to_string()]);
            }
            "--kv-cache-verify-every" => {
                let n: u64 = value("--kv-cache-verify-every").parse().unwrap_or_else(|e| die(format!("{e}")));
                worker_args.extend(["--kv-cache-verify-every".to_string(), n.to_string()]);
            }
            "--no-answer-fast-path" => answer_fast_path = false,
            "--no-cancel-on-disconnect" => cancel_on_disconnect = false,
            "--finality-depth" => finality_depth = value("--finality-depth").parse().unwrap_or_else(|e| die(format!("{e}"))),
            // RFC-0001 §2.9: the artifact sidecar — read here for its chat template and defaults,
            // and forwarded to every worker for its tokenizer.
            "--sidecar" => {
                let path = PathBuf::from(value("--sidecar"));
                worker_args.extend(["--sidecar".to_string(), path.display().to_string()]);
                sidecar_path = Some(path);
            }
            "--max-connections-per-source" => {
                max_connections_per_source = value("--max-connections-per-source").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--max-jobs-per-source" => max_jobs_per_source = value("--max-jobs-per-source").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--per-source-jobs-per-window" => {
                per_source_jobs_per_window = value("--per-source-jobs-per-window").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            other => die(format!(
                "unknown argument {other:?}\nusage: misaka-palw-gateway --worker <family-fp-worker> --outbox <dir> --identity <json> (--rpc <host:port> | --anchor <json>) [--listen addr] [--rpc-timeout-secs n] [--class-leaves n] [--max-decode-default n] [--max-decode-cap n] [--max-prompt-bytes n] [--bond-exposure-room-sompi n --claim-exposure-sompi n [--public-job-budget-permille n]] [--answer-never-commit] [--per-source-jobs-per-window n] [--worker-processes n] [--kv-cache-budget-mib n [--kv-cache-verify-every n]] [--no-answer-fast-path] [--sidecar <file>] [--no-cancel-on-disconnect] [--finality-depth n] [--max-connections-per-source n] [--max-jobs-per-source n] [--derive-seed <file OUTSIDE --identity's dir and --outbox>] [--artifact-inline-max <bytes>]"
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
        privacy_mode,
        per_source_jobs_per_window,
        confinement: Confinement::none(),
        booted_at_unix: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        worker_processes: worker_processes.clamp(1, MAX_WORKER_PROCESSES),
        worker_args,
        answer_fast_path,
        max_connections_per_source: max_connections_per_source.max(1),
        max_jobs_per_source: max_jobs_per_source.max(1),
        sidecar: sidecar_path.as_deref().map(|p| SidecarRuntime::load(p).unwrap_or_else(|e| die(e))),
        cancel_on_disconnect,
        finality_depth,
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
    let mut identity = load_identity(&config.identity_path, config.answer_never_commit);
    // An absent class id is the answer-only offline form's alone: `--rpc` reads the class's facts
    // BY id, so a gateway that asks the node must say which class it is asking about.
    if identity.class_id == Hash64::default() && rpc_endpoint.is_some() {
        die("the identity names no class_id: an answer-only gateway may omit it only with --anchor, because --rpc reads the \
             class's facts by its id"
            .into());
    }

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
    let worker = WorkerSupervisor::boot(
        config.confinement.clone(),
        config.worker.clone(),
        config.workdir.clone(),
        trace_dir,
        config.worker_processes,
        config.worker_args.clone(),
    )
    .unwrap_or_else(|e| die(e));
    // ADR-0096 Decision 10: an answer-only gateway that named no class adopts its worker's — the
    // one value the worker would refuse any other of (`check_identity_v1` pins it per job).
    if identity.class_id == Hash64::default() {
        identity.class_id = worker.manifest().class_id;
        identity.class_id_hex = hex(identity.class_id);
    }

    eprintln!(
        "[misaka-palw-gateway] listening on {} ({}) — worker manifest {}…, class {}…, n_ctx {}, template {}",
        config.listen,
        if listen_is_loopback(&config.listen) { "loopback" } else { "PUBLIC, acknowledged" },
        &hex(worker.manifest().runtime_manifest_hash)[..16],
        &hex(identity.class_id)[..16],
        worker.manifest().n_ctx,
        advertised_template_id(&config, worker.manifest()),
    );
    // ADR-0077 Decision 16's disclosure, verbatim, on every boot that files private commitments:
    // the operator is the one who reads it, and it says what "private" buys and does not.
    if config.privacy_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PANEL_DA {
        eprintln!(
            "[misaka-palw-gateway] --privacy panel-da: {}",
            kaspa_consensus_core::palw_panel_da_v1::PALW_FP_PANEL_DA_DISCLOSURE_V1
        );
    }
    let boot_facts = chain_source.read();
    eprintln!(
        "[misaka-palw-gateway] chain {} | registered {} | fp_certified {} | bond_active {} | exposure_room {}",
        boot_facts.source, boot_facts.registered, boot_facts.fp_certified, boot_facts.bond_active, boot_facts.exposure_room_sompi
    );
    eprintln!(
        "[misaka-palw-gateway] confinement backend {} | {} job slot(s), {} may queue, {MAX_CONNECTIONS} connections | \
         prompt ≤ {} bytes, body ≤ {MAX_REQUEST_BODY_BYTES} bytes, decode ≤ {} | public-job budget {}‰",
        backend.name(),
        worker.processes(),
        in_flight_cap(worker.processes()) - worker.processes(),
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
    // RFC-0001 §2.7: per-source open connections and jobs in flight.
    let gate = Arc::new(pool::SourceGate::new(config.max_connections_per_source, config.max_jobs_per_source));
    let services = Arc::new(Services::open(&config.outbox));

    let listener = TcpListener::bind(&config.listen).unwrap_or_else(|e| die(format!("cannot bind {}: {e}", config.listen)));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if connections.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            connections.fetch_sub(1, Ordering::AcqRel);
            respond_with(&mut stream, "503 Service Unavailable", &error_body("connection cap reached"), Some(serving::RETRY_AFTER_CONNECTION_SECS), &[]);
            continue;
        }
        let (config, identity, worker, chain_source) =
            (Arc::clone(&config), Arc::clone(&identity), Arc::clone(&worker), Arc::clone(&chain_source));
        let (in_flight, budget, sources) = (Arc::clone(&in_flight), Arc::clone(&budget), Arc::clone(&sources));
        let connections = Arc::clone(&connections);
        let gate = Arc::clone(&gate);
        let services = Arc::clone(&services);
        let acknowledged_bind = acknowledged;
        // The per-source connection share: counted here, before the thread, so a source over its
        // share costs a 503 and not a thread.
        let peer = stream.peer_addr().map(|a| a.ip()).ok();
        if let Some(peer) = peer
            && gate.open_connection(peer).is_err()
        {
            connections.fetch_sub(1, Ordering::AcqRel);
            respond(&mut stream, "429 Too Many Requests", &error_body("per-source connection share exceeded"));
            continue;
        }
        std::thread::spawn(move || {
            serve_connection(
                &mut stream,
                &config,
                &identity,
                &*worker,
                &chain_source,
                &in_flight,
                &budget,
                &sources,
                backend,
                acknowledged_bind,
                &gate,
                &services,
            );
            if let Some(peer) = peer {
                gate.close_connection(peer);
            }
            connections.fetch_sub(1, Ordering::AcqRel);
        });
    }
}

/// The status route's observer: a node over RPC answers (one node: `agreeing = 1`, which is a report and not a proof); the
/// anchor-file form has no node and says so.
impl status::ClaimObserver for chain::ChainSource {
    fn observe(
        &self,
        claim_id: Hash64,
        tx_id: Option<Hash64>,
    ) -> Result<(misaka_palw_remote::track::ChainObservation, usize), String> {
        match self {
            chain::ChainSource::Rpc(rpc) => rpc.observe_claim(claim_id, tx_id).map(|obs| (obs, 1)),
            chain::ChainSource::AnchorFile(_) => Err("this gateway has no node to ask (--anchor form)".into()),
        }
    }
}

/// **The per-process serving state the entrance shares across connections**: the idempotency table and the book of requests in
/// flight / cancelled.
struct Services {
    idempotency: idempotency::IdempotencyStore,
    book: status::RequestBook,
}

impl Services {
    fn open(outbox: &Path) -> Self {
        Self { idempotency: idempotency::IdempotencyStore::open(outbox), book: status::RequestBook::default() }
    }
}

/// A source's share of in-flight jobs, given back when the request ends however it ends.
struct SourceJobsGuard<'a> {
    gate: &'a pool::SourceGate,
    source: Option<IpAddr>,
    jobs: u32,
}

impl Drop for SourceJobsGuard<'_> {
    fn drop(&mut self) {
        if let Some(source) = self.source {
            self.gate.finish_jobs(source, self.jobs);
        }
    }
}

/// How a request is turned away (or answered) before it costs anything.
enum Early {
    Refuse(&'static str, String),
    Replay(serde_json::Value),
}

/// Whether the body asks for a streamed delivery (read leniently: a malformed body is refused later, by the entrance).
fn streaming_requested(body: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(body).ok().and_then(|v| v.get("stream").and_then(serde_json::Value::as_bool)).unwrap_or(false)
}

/// **The idempotency decision** (see [`idempotency`]): no header → `Ok(None)` (the request runs as it always did); a fresh key →
/// `Ok(Some(reservation))`; a finished request → `Early::Replay`; anything else a refusal that says why.
fn begin_idempotent<'a>(services: &'a Services, request: &HttpRequest) -> Result<Option<idempotency::Reservation<'a>>, Early> {
    let Some(raw) = request.idempotency_key.as_deref() else { return Ok(None) };
    let key = idempotency::IdempotencyKey::parse(raw).map_err(|e| Early::Refuse("400 Bad Request", e))?;
    let digest = idempotency::request_digest(&request.body).map_err(|e| Early::Refuse("400 Bad Request", e))?;
    match services.idempotency.begin(&key, digest) {
        idempotency::Begin::Fresh(reservation) => Ok(Some(reservation)),
        idempotency::Begin::Replay(body) => Err(Early::Replay(body)),
        idempotency::Begin::InProgress => Err(Early::Refuse(
            "409 Conflict",
            "a request with this Idempotency-Key is still running; retry shortly and you will receive its response".into(),
        )),
        idempotency::Begin::Conflict => Err(Early::Refuse(
            "409 Conflict",
            "this Idempotency-Key was used for a different request; a key names one request".into(),
        )),
        idempotency::Begin::Overloaded => Err(Early::Refuse("503 Service Unavailable", "the idempotency table is full of running requests".into())),
    }
}

/// The status of a finished job's completion id, read from the outbox (and the chain, when a node can be asked).
fn status_of_completion(config: &Config, chain_source: &chain::ChainSource, id: &str) -> Option<(status::StatusReport, status::LocalFacts)> {
    let stem = status::stem_of_completion_id(id)?;
    let local = status::read_local_facts(&config.outbox, &stem)?;
    let observer: Option<&dyn status::ClaimObserver> = chain_source.can_submit().then_some(chain_source as &dyn status::ClaimObserver);
    let report = status::status_with_chain(&local, observer, config.finality_depth, None);
    Some((report, local))
}

/// **Serve a stored response again** — the same claim, no inference — with its status recomputed NOW (a replay a day later may
/// find the claim submitted, or final) and the replay marked. A streamed retry gets the whole answer as one delta and the same
/// terminal event.
fn replay_stored(
    stream: &mut TcpStream,
    config: &Config,
    chain_source: &chain::ChainSource,
    body: &serde_json::Value,
    streaming: bool,
) {
    let mut body = body.clone();
    if let Some((report, local)) = body["id"].as_str().and_then(|id| status_of_completion(config, chain_source, id)) {
        body["misaka"]["request"] = report.to_json(&local);
    }
    body["misaka"]["idempotent_replay"] = serde_json::json!(true);
    if !streaming {
        respond_with(stream, "200 OK", &body, None, &[("idempotent-replayed", "true")]);
        return;
    }
    let mut sink = SseSink {
        stream,
        id: body["id"].as_str().unwrap_or("palwcmpl-replay").to_string(),
        model: body["model"].as_str().unwrap_or(surface::MODEL_ID).to_string(),
        started: false,
        broken: false,
    };
    sink.head();
    let message = &body["choices"][0]["message"];
    if let Some(text) = message["content"].as_str() {
        sink.delta(text);
    }
    let delta = match message.get("tool_calls").and_then(serde_json::Value::as_array) {
        Some(calls) => serde_json::json!({ "tool_calls": calls.iter().enumerate().map(|(i, c)| { let mut c = c.clone(); c["index"] = serde_json::json!(i); c }).collect::<Vec<_>>() }),
        None => serde_json::json!({}),
    };
    sink.chunk(delta, body["choices"][0]["finish_reason"].clone());
    sink.event(&serde_json::json!({ "misaka": body["misaka"].clone(), "usage": body["usage"].clone() }));
    sink.done();
}

#[allow(clippy::too_many_arguments)]
fn serve_connection(
    stream: &mut TcpStream,
    config: &Config,
    identity: &Identity,
    worker: &dyn JobRunner,
    chain_source: &chain::ChainSource,
    in_flight: &AtomicUsize,
    budget: &Mutex<PublicJobBudget>,
    sources: &Mutex<SourceRates>,
    backend: ConfinementBackend,
    acknowledged_bind: bool,
    gate: &pool::SourceGate,
    services: &Services,
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
                    "template_id": advertised_template_id(config, worker.manifest()),
                    "sidecar": config.sidecar.as_ref().map(|s| serde_json::json!({ "digest": s.digest, "path": s.path.display().to_string() })),
                    "n_ctx": worker.manifest().n_ctx,
                    // ADR-0097 Decision 2: the same limits object `GET /v1/models` serves.
                    "limits": surface::limits_body(worker.manifest(), &surface_limits(config), &facts),
                    "class_id": hex(identity.class_id),
                    "network_domain": hex(identity.network_domain),
                    "operator_id": hex(identity.operator_id),
                    // `null` for a bond-less answer-only gateway (ADR-0096 Decision 10), so a
                    // reader sees "no bond" rather than an all-zero outpoint that looks like one.
                    "bond": if identity.bond_txid_hex.is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::json!(format!("{}:{}", identity.executor_bond.transaction_id, identity.executor_bond.index))
                    },
                    "bond_present": !identity.bond_txid_hex.is_empty(),
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
                        "job_slots": worker.processes(),
                        "max_in_flight_jobs": in_flight_cap(worker.processes()),
                        "queued_jobs": worker.waiting(),
                        "max_connections": MAX_CONNECTIONS,
                        "max_connections_per_source": config.max_connections_per_source,
                        "max_jobs_per_source": config.max_jobs_per_source,
                        "answer_fast_path": config.answer_fast_path && worker.answer_only_supported(),
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
            if request.body.len() > MAX_REQUEST_BODY_BYTES {
                respond(stream, "400 Bad Request", &error_body("the body exceeds the request cap"));
                return;
            }
            // **Idempotency (RFC-0001 §2.7), BEFORE anything that costs.** A retry of a finished request is answered from the
            // stored response: no chain read, no queue place, no per-source token, no inference, no second claim.
            let reservation = match begin_idempotent(services, &request) {
                Ok(reservation) => reservation,
                Err(Early::Refuse(status, message)) => {
                    respond(stream, status, &error_body(&message));
                    return;
                }
                Err(Early::Replay(body)) => {
                    replay_stored(stream, config, chain_source, &body, streaming_requested(&request.body));
                    return;
                }
            };
            if let Some(source) = source
                && !sources.lock().expect("the source lock is never poisoned").admit(source, config.per_source_jobs_per_window)
            {
                respond(stream, "429 Too Many Requests", &error_body("per-source job rate exceeded"));
                return;
            }
            // Parsed AND ADMITTED before the queue reservation and before the worker is touched
            // (ADR-0096 invariant 6): every refusal the surface can raise is a status code here,
            // never an inference — and `stream: true` decides the response shape only for a
            // request that was admitted. The chain is read once, for this job; `handle_chat`
            // prices and anchors against the same facts the sampler and the format were gated on.
            let facts = chain_source.read();
            let (chat, admitted) = match surface::parse_and_admit_with(&request.body, &facts, |chat| {
                // RFC-0001 §2.9: the sidecar's defaults fill what the request omitted, BEFORE the
                // entrance admits — so a default is held to every rule an explicit field is.
                config.sidecar.as_ref().map(|s| s.apply_defaults(chat, &facts))
            }) {
                Ok((chat, mut admitted, report)) => {
                    admitted.sidecar_report = report;
                    (chat, admitted)
                }
                Err(e) => {
                    respond(stream, "400 Bad Request", &error_body(&e));
                    return;
                }
            };
            let streaming = chat.stream == Some(true);
            // The bounded in-flight queue. Reserved BEFORE the slot is contended, so the depth of
            // the wait is a number this process chose rather than one the network chose for it. A
            // request for `n` candidates is `n` jobs (RFC-0001 §2.4), against the queue and against
            // its source's share alike. **A reservation, released by drop** (`serving::QueueGate`): it cannot leak on a
            // panic and it never over-reserves, even for an instant.
            let jobs = admitted.candidates.max(1);
            let _queue_place = match serving::QueueGate::try_reserve(in_flight, jobs as usize, in_flight_cap(worker.processes())) {
                Ok(guard) => guard,
                Err(_) => {
                    respond(
                        stream,
                        "503 Service Unavailable",
                        &error_body("the in-flight queue is full; the worker slots are busy and the queue behind them is bounded"),
                    );
                    return;
                }
            };
            if let Some(source) = source
                && gate.start_jobs(source, jobs).is_err()
            {
                respond(stream, "429 Too Many Requests", &error_body("per-source jobs in flight exceeded"));
                return;
            }
            let _source_share = SourceJobsGuard { gate, source, jobs };
            let link: Box<dyn serving::ClientLink> = match serving::TcpLink::new(stream) {
                Some(link) if config.cancel_on_disconnect => Box::new(link),
                _ => Box::new(serving::AlwaysPresent),
            };
            let ctx = RequestCtx { link: link.as_ref() };
            if streaming {
                // **A slow reader must not wedge the one job slot.** The deltas are written from
                // inside the worker's mutex, so a client that stops reading would otherwise block
                // on TCP back-pressure and hold the resident worker for as long as it liked. A
                // write timeout turns that into a dropped connection.
                let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
                let model = chat.model.clone().unwrap_or_else(|| surface::MODEL_ID.to_string());
                let mut nonce = [0u8; 12];
                rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
                // The chunk id is drawn per RESPONSE: the job id does not exist until the run
                // ends, and an OpenAI client needs a stable id from the first chunk.
                let include_usage = admitted.include_usage;
                let stream_id = format!("palwcmpl-{}", faster_hex::hex_string(&nonce));
                let ticket = services.book.begin(&stream_id);
                let mut sink = SseSink { stream, id: stream_id, model, started: false, broken: false };
                sink.head();
                let outcome = handle_chat(config, identity, worker, budget, &facts, chain_source, &chat, &admitted, admitted.sampling, &mut sink, &ctx);
                match outcome {
                    Ok(body) => {
                        drop(ticket);
                        // The terminal chunk carries the finish reason and, in the same event, the
                        // `misaka` object: whether this answer became a claim and, if not, why.
                        // An SSE client that never sees it is a client that was told nothing.
                        // ADR-0096 Decision 2: the block text streamed as text; the parsed calls
                        // ride the terminal delta in OpenAI's streaming shape (with `index`).
                        let finish = body["choices"][0]["finish_reason"].clone();
                        let delta = match body["choices"][0]["message"].get("tool_calls").and_then(serde_json::Value::as_array) {
                            Some(calls) => serde_json::json!({
                                "tool_calls": calls.iter().enumerate().map(|(index, call)| {
                                    let mut call = call.clone();
                                    call["index"] = serde_json::json!(index);
                                    call
                                }).collect::<Vec<_>>(),
                            }),
                            None => serde_json::json!({}),
                        };
                        sink.chunk(delta, finish);
                        if include_usage {
                            // `stream_options.include_usage` (ADR-0096 Decision 1): OpenAI's own
                            // usage chunk — no choices, the counts — for a client that reads
                            // that and not the `misaka` event below, which carries them anyway.
                            sink.usage_chunk(body["usage"].clone());
                        }
                        sink.event(&serde_json::json!({ "misaka": body["misaka"].clone(), "usage": body["usage"].clone() }));
                        sink.done();
                        if let Some(reservation) = reservation {
                            reservation.complete(body);
                        }
                    }
                    Err(e) if serving::is_cancelled(&e) => {
                        // The client is gone: nothing was committed and there is nobody to tell.
                        ticket.cancelled();
                        eprintln!("[misaka-palw-gateway] event lane=prompt stage=CANCELLED reason=client_disconnected");
                    }
                    Err(e) => {
                        // Past the head, an error can only be an event. Decision 2: a stream whose
                        // rendering is not the committed one is CLOSED with an error, and no
                        // commitment was written.
                        drop(ticket);
                        sink.event(&surface::refusal_body(&e));
                        sink.done();
                    }
                }
            } else {
                let outcome = if admitted.candidates > 1 {
                    handle_chat_candidates(config, identity, worker, budget, &facts, chain_source, &chat, &admitted, &ctx)
                } else {
                    let mut sink = BufferedSink;
                    handle_chat(config, identity, worker, budget, &facts, chain_source, &chat, &admitted, admitted.sampling, &mut sink, &ctx)
                };
                match outcome {
                    Ok(body) => {
                        respond(stream, "200 OK", &body);
                        if let Some(reservation) = reservation {
                            reservation.complete(body);
                        }
                    }
                    Err(e) if serving::is_cancelled(&e) => {
                        eprintln!("[misaka-palw-gateway] event lane=prompt stage=CANCELLED reason=client_disconnected");
                    }
                    Err(e) => respond(stream, "400 Bad Request", &surface::refusal_body(&e)),
                }
            }
        }
        // **RFC-0001 §2.8 (P1): the local embeddings route.** The same entrance bounds as a chat job
        // (the in-flight cap, the per-source share, the source's hourly rate); one request is one
        // job whatever its input count, and its inputs run in order on the worker pool.
        ("POST", "/v1/embeddings") => {
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
            let parsed: surface::EmbeddingsRequest = match serde_json::from_slice(&request.body) {
                Ok(parsed) => parsed,
                Err(e) => {
                    respond(stream, "400 Bad Request", &error_body(&format!("request body is not an embeddings request: {e}")));
                    return;
                }
            };
            let admitted = match surface::admit_embeddings(&parsed, config.max_prompt_bytes) {
                Ok(admitted) => admitted,
                Err(e) => {
                    respond(stream, "400 Bad Request", &error_body(&e));
                    return;
                }
            };
            let _queue_place = match serving::QueueGate::try_reserve(in_flight, 1, in_flight_cap(worker.processes())) {
                Ok(guard) => guard,
                Err(_) => {
                    respond(stream, "503 Service Unavailable", &error_body("the in-flight queue is full; the worker slots are busy and the queue behind them is bounded"));
                    return;
                }
            };
            if let Some(source) = source
                && gate.start_jobs(source, 1).is_err()
            {
                respond(stream, "429 Too Many Requests", &error_body("per-source jobs in flight exceeded"));
                return;
            }
            let _source_share = SourceJobsGuard { gate, source, jobs: 1 };
            let outcome = handle_embeddings(config, worker, &parsed, &admitted);
            match outcome {
                Ok(Some(body)) => respond(stream, "200 OK", &body),
                Ok(None) => respond(stream, "501 Not Implemented", &error_body("this class's worker serves no embeddings path")),
                Err(e) => respond(stream, "400 Bad Request", &surface::refusal_body(&e)),
            }
        }
        // **RFC-0001 §2.7: what became of a request** — `streaming | answered | committed | submitted | final | voided | cancelled`.
        // A GET with no side effects, bounded by the same per-source fetch rate as the artifact route. `final` appears only from a
        // chain fact, and carries the label that says how the chain was read (see `status`).
        ("GET", path) if path.starts_with("/v1/requests/") => {
            if let Some(source) = source
                && !sources.lock().expect("the source lock is never poisoned").admit_fetch(source)
            {
                respond(stream, "429 Too Many Requests", &error_body("per-source status fetch rate exceeded"));
                return;
            }
            let id = &path["/v1/requests/".len()..];
            if let Some(report) = status::status_from_book(&services.book, id) {
                respond(stream, "200 OK", &report.to_json(&status::LocalFacts::default()));
            } else if let Some((report, local)) = status_of_completion(config, chain_source, id) {
                respond(stream, "200 OK", &report.to_json(&local));
            } else {
                respond(stream, "404 Not Found", &error_body("no request under that id: it is neither streaming here nor in this gateway's outbox"));
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
        // ADR-0096 Decision 1: the one class this gateway serves, in the shape a stock client lists
        // models with — so a base-URL change is the whole migration.
        ("GET", "/v1/models") => respond(
            stream,
            "200 OK",
            &surface::models_body(
                &hex(identity.class_id),
                worker.manifest(),
                &advertised_template_id(config, worker.manifest()),
                config.booted_at_unix,
                surface::limits_body(worker.manifest(), &surface_limits(config), &chain_source.read()),
            ),
        ),
        _ => respond(
            stream,
            "404 Not Found",
            &error_body(
                "this gateway serves POST /v1/chat/completions, POST /v1/embeddings, GET /v1/models, GET /health, GET /v1/requests/<id> and GET /v1/artifacts/<derived-id>",
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    /// **ADR-0096 Decisions 7-8 at the entrance**: where the network commits formats, a `response_format` becomes a
    /// constraint the job carries (version 6); where it does not, or none was asked, nothing changes; and a committed format
    /// beside sampler controls is refused by name.
    #[test]
    fn a_response_format_is_committed_only_where_the_fence_is_armed_and_beside_no_sampler_controls() {
        let body = |extra: serde_json::Value| {
            let mut v = serde_json::json!({ "messages": [{ "role": "user", "content": "hi" }], "response_format": { "type": "json_object" } });
            v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            serde_json::to_vec(&v).unwrap()
        };
        let armed = chain::ChainFacts { fp_decode_constraint_armed: true, fp_decode_rules_armed: true, ..Default::default() };
        let dormant = chain::ChainFacts { fp_decode_rules_armed: true, ..Default::default() };
        let admit = |facts: &chain::ChainFacts, extra| surface::parse_and_admit(&body(extra), facts).map(|(_, a)| a);
        let ok = admit(&armed, serde_json::json!({})).expect("admitted");
        let bytes = committed_constraint_v1(&ok, &armed, ok.sampling).unwrap().expect("committed on an armed network");
        kaspa_consensus_core::palw_fp_constraint_job_v1::palw_constraint_of_bytes_v1(&bytes).expect("the bytes are an admitted constraint");
        let advisory = admit(&dormant, serde_json::json!({})).expect("admitted");
        assert_eq!(committed_constraint_v1(&advisory, &dormant, advisory.sampling).unwrap(), None, "dormant: advisory, as before");
        let plain = surface::parse_and_admit(&serde_json::to_vec(&serde_json::json!({ "messages": [{ "role": "user", "content": "hi" }] })).unwrap(), &armed)
            .map(|(_, a)| a)
            .unwrap();
        assert_eq!(committed_constraint_v1(&plain, &armed, plain.sampling).unwrap(), None, "no format asked");
        let with_stop = admit(&armed, serde_json::json!({ "stop": ["END"] })).expect("admitted");
        let err = committed_constraint_v1(&with_stop, &armed, with_stop.sampling).unwrap_err();
        assert!(err.contains("no sampler controls or stop strings"), "{err}");
    }

    /// **ADR-0096 Decision 10: a bond-less identity is admitted exactly when the gateway never
    /// commits.** Without the flag the refusal names what is missing and the flag that would
    /// admit it; with it the bond is the zero outpoint and the key is empty, which `/health`
    /// reports as `bond: null` rather than as an outpoint that looks like one.
    #[test]
    fn a_bondless_identity_is_admitted_only_for_an_answer_only_gateway() {
        let file = || IdentityFile {
            network_domain: "11".repeat(64),
            class_id: "22".repeat(64),
            bond_txid: String::new(),
            bond_index: 0,
            executor_pubkey: String::new(),
            operator_id: "33".repeat(64),
        };
        let refused = identity_from_file(file(), false).err().expect("no bond, no flag");
        assert!(refused.contains("--answer-never-commit") && refused.contains("bond_txid"), "{refused}");
        let admitted = identity_from_file(file(), true).expect("no bond, never commits");
        assert!(admitted.bond_txid_hex.is_empty());
        assert_eq!(admitted.executor_bond.transaction_id, TransactionId::from_bytes(Hash64::default().as_bytes()));
        assert!(admitted.executor_pubkey.is_empty());
        // A bond with no key is the other half of the same rule.
        let mut keyless = file();
        keyless.bond_txid = "44".repeat(64);
        let refused = identity_from_file(keyless, false).err().expect("a bond with no key cannot sign");
        assert!(refused.contains("executor_pubkey"), "{refused}");
        // An answer-only identity may name no class, domain or operator: zeros, adopted after boot.
        let mut bare = file();
        bare.class_id = String::new();
        bare.network_domain = String::new();
        bare.operator_id = String::new();
        let bare_identity = identity_from_file(bare, true).expect("answer-only: every field but the class's own facts may be absent");
        assert_eq!(bare_identity.class_id, Hash64::default(), "absent reads as zeros, and boot adopts the worker's");
        let mut committing = file();
        committing.class_id = String::new();
        committing.bond_txid = "44".repeat(64);
        committing.executor_pubkey = "ab".repeat(8);
        let refused = identity_from_file(committing, false).err().expect("a committing gateway names its class");
        assert!(refused.contains("class_id"), "{refused}");
        // Through JSON, the way the file is actually read: the Studio's answer-only identity is `{}`.
        let empty: IdentityFile = serde_json::from_str("{}").expect("every field of an answer-only identity may be absent");
        assert!(identity_from_file(empty, true).is_ok(), "an empty answer-only identity loads");
        let empty: IdentityFile = serde_json::from_str("{}").unwrap();
        assert!(identity_from_file(empty, false).is_err(), "and a committing gateway refuses the same file");
        // And a complete identity parses as it always did, flag or no flag.
        let mut complete = file();
        complete.bond_txid = "44".repeat(64);
        complete.executor_pubkey = "ab".repeat(8);
        let full = identity_from_file(complete, false).expect("bonded");
        assert_eq!(full.executor_bond.index, 0);
        assert_eq!(full.executor_pubkey.len(), 8);
    }

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
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            per_source_jobs_per_window: 2,
            confinement: Confinement::none(),
            derive_seed: None,
            artifact_inline_max: 4 << 20,
            booted_at_unix: 0,
            worker_processes: 1,
            worker_args: Vec::new(),
            answer_fast_path: true,
            max_connections_per_source: DEFAULT_MAX_CONNECTIONS_PER_SOURCE,
            max_jobs_per_source: DEFAULT_MAX_JOBS_PER_SOURCE,
            sidecar: None,
            cancel_on_disconnect: true,
            finality_depth: status::DEFAULT_FINALITY_DEPTH,
        }
    }

    fn declared_price(config: &Config) -> ExposurePrice {
        ExposurePrice::resolve(config, &chain::ChainFacts::default())
    }

    /// **ADR-0118 Decision 5: a held class on a network minted flat commits under PanelDa.** Its
    /// ids are a Merkle root and the chain checks a carrier under the network's flat form, so a
    /// PublicDa gateway is refused by name before the inference; a panel-da one is served; and a
    /// class whose form is the network's — Merkle on a Merkle genesis, flat on a flat one — is
    /// served under either mode exactly as before.
    #[test]
    fn a_held_class_on_a_flat_network_commits_under_panel_da_and_is_refused_public_da_by_name() {
        use kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PANEL_DA;
        let held_on_flat = chain::ChainFacts { panel_da_armed: true, class_prompt_ids_merkle: true, ..Default::default() };
        let refused = privacy_mode_for_request(&bounded_config(), &held_on_flat).expect_err("PublicDa cannot carry the held ids");
        assert!(refused.contains("ADR-0118") && refused.contains("--privacy panel-da"), "{refused}");
        let panel = Config { privacy_mode: PALW_FP_PRIVACY_PANEL_DA, ..bounded_config() };
        assert_eq!(privacy_mode_for_request(&panel, &held_on_flat), Ok(PALW_FP_PRIVACY_PANEL_DA));
        assert_eq!(held_on_flat.prompt_ids_form(), kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1);
        let merkle_genesis =
            chain::ChainFacts { panel_da_armed: true, prompt_ids_merkle: true, class_prompt_ids_merkle: true, ..Default::default() };
        assert_eq!(privacy_mode_for_request(&bounded_config(), &merkle_genesis), Ok(PALW_FP_PRIVACY_PUBLIC_DA));
        assert_eq!(privacy_mode_for_request(&bounded_config(), &chain::ChainFacts::default()), Ok(PALW_FP_PRIVACY_PUBLIC_DA));
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

    /// **Every response that publishes `job_context_hash` publishes the CONTEXT beside it**
    /// (ADR-0078 X6's binding leg).
    ///
    /// The two response builders — the outbox summary and the `misaka` block of the chat
    /// completion — are assembled inside the request path, which needs a worker, an identity and
    /// a chain to reach; so the pin is on the source that builds them, the way
    /// [`responses_carry_no_cors_header_at_all`] pins the response head. What it is worth pinning:
    /// a consumer can recompute `output_root` from the hash alone, but the BINDING — is this
    /// artifact's DSL the rendering of this claim's ids? — needs `tokenizer_id`, a field of the
    /// context, and a response that published only the hash left `binding_checked: true`
    /// unreachable for every real job. A builder that grows a third `job_context_hash` site
    /// without the context fails here.
    #[test]
    fn every_response_that_publishes_the_context_hash_publishes_the_context() {
        // Assembled at run time so these assertions do not match their own source lines.
        let hash_field = format!("\"job_{0}_hash\": job_{0}_hash", "context");
        let context_field = format!("\"job_{0}\": job_{0}", "context");
        let source = std::include_str!("main.rs");
        let hashes = source.lines().filter(|l| l.contains(&hash_field)).count();
        let contexts = source.lines().filter(|l| l.contains(&context_field)).count();
        assert_eq!(hashes, 2, "the two response builders are the outbox summary and the `misaka` block");
        assert_eq!(contexts, hashes, "a response publishes the job context wherever it publishes the context hash");
        // And the reader that supplies them returns three values, not two: the third is the
        // context, and a call site that ignored it would not compile.
        let (hash, family, context) = derive::read_worker_manifest(std::path::Path::new("/nonexistent/traces/job"));
        assert_eq!((hash, family, context), (None, None, None), "no manifest is three absences, never a guess");
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

        // A claim that fits the room but not the whole window's budget is a setting that can never
        // commit, and says so — not "spent (0 of …)", which read like a busy day.
        let never_fits = Config { claim_exposure_sompi: 300_000, ..bounded_config() };
        let err = PublicJobBudget::new().may_commit(&never_fits, declared_price(&never_fits)).unwrap_err();
        assert!(err.contains("no public job can commit at this setting") && err.contains("200‰"), "got {err}");

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
            temperature,
            seed: seed.map(str::to_string),
            ..Default::default()
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

    /// **An unterminated request line is refused, not grown** (mainnet audit, 2026-09-05).
    ///
    /// Both arms: an ordinary line still reads, and a line that never ends is refused at the cap
    /// instead of being accumulated into a `String` until the public entrance dies.
    #[test]
    fn an_unterminated_request_line_is_refused_at_the_cap() {
        use std::io::Cursor;

        let mut ok = Cursor::new(b"GET /healthz HTTP/1.1\r\n".to_vec());
        assert_eq!(read_capped_line(&mut ok, "the request line").unwrap().trim_end(), "GET /healthz HTTP/1.1");

        let flood = vec![b'A'; (MAX_REQUEST_LINE_BYTES as usize) + 4096];
        let mut endless = Cursor::new(flood);
        let e = read_capped_line(&mut endless, "the request line").expect_err("a line with no newline must be refused");
        assert!(e.contains("exceeds the"), "the refusal names the cap: {e}");

        // A line that ends exactly at the cap is a legal line, not a flood.
        let mut exact = Cursor::new({
            let mut v = vec![b'B'; (MAX_REQUEST_LINE_BYTES as usize) - 1];
            v.push(b'\n');
            v
        });
        assert!(read_capped_line(&mut exact, "the request line").is_ok(), "the boundary itself is admissible");
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
            ("surface.rs", std::include_str!("surface.rs")),
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

    fn sidecar_with(generation: misaka_palw_base0::sidecar::GenerationConfigV1) -> SidecarRuntime {
        SidecarRuntime {
            digest: "00".repeat(64),
            path: PathBuf::from("/nonexistent/sidecar"),
            template: None,
            template_ids: None,
            generation: Some(generation),
        }
    }

    fn bare_chat() -> ChatRequest {
        serde_json::from_value(serde_json::json!({ "messages": [{ "role": "user", "content": "hi" }] })).unwrap()
    }

    /// **RFC-0001 §2.9: the sidecar's generation defaults fill what a request omitted, through the
    /// entrance's own admission** — an explicit field always wins; the sampler's defaults only where
    /// the chain armed it (and are reported as not applied where it did not); the knobs this lane has
    /// no rule for are never applied.
    #[test]
    fn a_sidecars_generation_defaults_fill_the_gaps_and_never_override_or_outrun_the_chain() {
        let generation = misaka_palw_base0::sidecar::GenerationConfigV1 {
            max_new_tokens: Some(77),
            temperature: Some(0.7),
            repeat_penalty: Some(1.2),
            repeat_last_n: Some(32),
            stop: vec!["###".into()],
            top_p: Some(0.8),
            ..Default::default()
        };
        let side = sidecar_with(generation);
        // Dormant network: only the length default applies; the sampler's are reported, not applied.
        let dormant = chain::ChainFacts::default();
        let mut chat = bare_chat();
        let report = side.apply_defaults(&mut chat, &dormant);
        assert_eq!(chat.max_tokens, Some(77));
        assert!(chat.temperature.is_none() && chat.repeat_penalty.is_none() && chat.stop.is_none());
        let not_applied: Vec<String> =
            report["defaults_not_applied"].as_array().unwrap().iter().map(|r| r["field"].as_str().unwrap().to_string()).collect();
        for field in ["temperature", "repeat_penalty", "repeat_last_n", "stop", "top_p"] {
            assert!(not_applied.contains(&field.to_string()), "{field} is reported as not applied: {not_applied:?}");
        }
        assert_eq!(report["defaults_applied"], serde_json::json!(["max_new_tokens"]));
        assert!(surface::admit_request(&chat, &dormant).is_ok(), "the request with the defaults the network allows is admissible");
        // Armed network: the sampler's defaults apply, and the request is admitted with them.
        let armed = chain::ChainFacts { fp_decode_rules_armed: true, ..Default::default() };
        let mut chat = bare_chat();
        let report = side.apply_defaults(&mut chat, &armed);
        assert_eq!((chat.temperature, chat.repeat_penalty, chat.repeat_last_n), (Some(0.7), Some(1.2), Some(32)));
        assert_eq!(chat.stop, Some(serde_json::json!(["###"])));
        assert!(surface::admit_request(&chat, &armed).is_ok());
        assert!(report["defaults_applied"].as_array().unwrap().len() >= 5);
        // An explicit field always wins.
        let mut explicit = bare_chat();
        explicit.max_tokens = Some(5);
        explicit.temperature = Some(0.1);
        side.apply_defaults(&mut explicit, &armed);
        assert_eq!((explicit.max_tokens, explicit.temperature), (Some(5), Some(0.1)));
        let mut aliased = bare_chat();
        aliased.max_completion_tokens = Some(9);
        side.apply_defaults(&mut aliased, &armed);
        assert_eq!(aliased.max_tokens, None, "an alias counts as the request having named a length");
    }

    #[test]
    fn the_pool_sizes_and_the_in_flight_cap_keep_the_single_worker_numbers() {
        assert_eq!(in_flight_cap(1), MAX_IN_FLIGHT_JOBS, "one process: one running and seven waiting, as it always was");
        assert_eq!(in_flight_cap(4), 4 + 7);
    }
}
