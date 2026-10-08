//! `palw-remote-miner` — an ordinary PALW attempt miner that runs **no `kaspad`** (RFC-0009 stage A).
//!
//! ```text
//!   palw-remote-miner --network testnet-12 --rpc nodeA:17210,nodeB:17210,nodeC:17210 \
//!     --bond <txid>:<index> --class <class id> --artifact-root <root> --pay-address <addr> \
//!     --key-file <seed> --checkpoint <daa>:<block hash> [--pin <block hash>] \
//!     --executor-cmd <program> [--executor-arg <arg> …] --state-dir <dir> [--steps N] [--poll-secs N]
//! ```
//!
//! It reads the chain through several nodes (`getBlockDagInfo`, `getBlockTemplate`, `getPalwProducerFacts`), refuses on any disagreement, a
//! stale template, a foreign class or a bond whose registered key is not the one it holds, runs the miner's OWN executor, signs once and only on a
//! win, re-checks a fresh quorum, and submits the finished block to every node. **The node validates the block independently**; nothing here is an
//! authority. Facts read from nodes are `UNVERIFIED_REMOTE_STATE` unless `--pin` proves the bond against a block you pinned.
//!
//! **The executor is not part of this binary.** The model backend lives in `kaspad/src/palw_backends` and has not been extracted (recorded as a
//! gap); `--executor-cmd` is the seam: the program is run with the job anchor in `PALW_ANCHOR` (128 hex) and must print one JSON object on stdout,
//! `{"trace_root":"…","output_root":"…","execution_root":"…","trace_manifest_root":"…","trace_chunk_count":N,"material_file":"<path>"}` (roots are
//! 128 hex). There is deliberately no built-in "demo" executor: an attempt whose execution a Panel cannot replay is fraud, and a miner must not be
//! one flag away from producing one.
//!
//! **Keeping the material.** The Panel pulls the capture from the miner (or a provider). The file is kept under `--state-dir/material/<attempt
//! id>`; until the independent DA transport serves it for you (`misaka_palw_remote::evidence`), this machine must stay up and serve it — "the PC can
//! be off after claiming" is NOT claimed here.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::dns_finality::{PalwAttemptSignRecordV1, SignedEpochCheckOutcome};
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PalwAttemptExecutionV1, PalwAttemptUnsignedV2, palw_network_domain_v2_for,
};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use misaka_palw_remote::attempt::{AttemptExecutor, AttemptParams, AttemptSigner, ExecutedAttempt};
use misaka_palw_remote::checkpoint::Checkpoint;
use misaka_palw_remote::miner::{
    BlockObservation, BlockState, BlockTracker, MinerConfig, MinerHalt, MinerState, NodeTemplate, RemoteNode, StepOutcome,
    publish_block, step,
};
use misaka_palw_remote::relay::Reply;
use misaka_palw_remote::template::{TemplatePolicy, producer_facts_from_wire_v1};
use misaka_palw_remote::view::{CheckpointStatus, NodeFacts, QuorumPolicy, ViewError};

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("[palw-remote-miner] fatal: {msg}");
    std::process::exit(1);
}

fn hash(what: &str, text: &str) -> Hash64 {
    text.trim().parse::<Hash64>().unwrap_or_else(|_| die(format!("{what} {text:?} is not a 128-hex hash")))
}

fn outpoint(text: &str) -> TransactionOutpoint {
    let (txid, index) = text.rsplit_once(':').unwrap_or_else(|| die(format!("--bond {text:?} is not <txid>:<index>")));
    TransactionOutpoint::new(hash("--bond txid", txid), index.parse().unwrap_or_else(|_| die("--bond index is not a number")))
}

// ---------------------------------------------------------------------------------------------------------------------------
// The nodes
// ---------------------------------------------------------------------------------------------------------------------------

struct WrpcNode<'a> {
    endpoint: String,
    runtime: &'a tokio::runtime::Runtime,
    client: KaspaRpcClient,
    pay_address: kaspa_addresses::Address,
    class_id: String,
    bond: TransactionOutpoint,
}

impl RemoteNode for WrpcNode<'_> {
    fn node_id(&self) -> &str {
        &self.endpoint
    }

    fn chain_facts(&self) -> Result<NodeFacts, ViewError> {
        let info = self.runtime.block_on(self.client.get_block_dag_info()).map_err(|e| ViewError(e.to_string()))?;
        Ok(NodeFacts {
            network_id: info.network.to_string(),
            sink: info.sink,
            virtual_daa: info.virtual_daa_score,
            pruning_point: info.pruning_point_hash,
        })
    }

    fn checkpoint_status(&self, checkpoint: &Checkpoint) -> Result<CheckpointStatus, ViewError> {
        match self.runtime.block_on(self.client.get_block(checkpoint.block_hash, false)) {
            Ok(block) => Ok(match block.verbose_data {
                Some(v) if v.is_chain_block => CheckpointStatus::OnChain,
                Some(_) => CheckpointStatus::Conflicts,
                None => CheckpointStatus::Unknown,
            }),
            // A node that does not know the block says nothing about the chain: silence, not a fork.
            Err(_) => Ok(CheckpointStatus::Unknown),
        }
    }

    fn fetch_template(&self) -> Result<NodeTemplate, String> {
        let t = self
            .runtime
            .block_on(self.client.get_block_template(kaspa_rpc_core::RpcAddress::from(self.pay_address.clone()), Vec::new()))
            .map_err(|e| format!("getBlockTemplate: {e}"))?;
        if !t.is_synced {
            return Err("the node reports it is not synced".into());
        }
        let block = Block::try_from(t.block).map_err(|e| format!("the template does not convert: {e}"))?;
        let f = self
            .runtime
            .block_on(self.client.get_palw_producer_facts(
                self.class_id.clone(),
                self.bond.transaction_id.to_string(),
                self.bond.index,
                true,
            ))
            .map_err(|e| format!("getPalwProducerFacts: {e}"))?;
        if !f.available {
            return Err("the node has no PALW producer facts (not a ConsensusV2 network, or no state yet)".into());
        }
        let facts = producer_facts_from_wire_v1(
            &f.chain_point,
            &f.class_id,
            &f.artifact_root,
            &f.class_target,
            f.pwu,
            f.min_trace_retention_daa,
            &f.bond_registered_pubkey,
            &f.not_ready_reason,
        )?;
        Ok(NodeTemplate { block, facts })
    }

    fn submit_block(&self, block: &Block) -> Reply {
        let hash = block.header.hash;
        match self.runtime.block_on(self.client.submit_block(kaspa_rpc_core::RpcRawBlock::from(block), false)) {
            Ok(r) if r.report.is_success() => Reply::Accepted(hash),
            Ok(r) => Reply::Refused(format!("{:?}", r.report)),
            Err(e) => {
                let text = e.to_string();
                // Believed only for OUR hash: a sentence cannot make us count a block this node never held.
                if (text.to_lowercase().contains("already") || text.to_lowercase().contains("duplicate"))
                    && text.contains(&hash.to_string())
                {
                    Reply::AlreadyKnown(hash)
                } else {
                    Reply::Refused(text)
                }
            }
        }
    }

    fn observe_block(&self, block_hash: Hash64) -> Result<BlockObservation, String> {
        let info = self.runtime.block_on(self.client.get_block_dag_info()).map_err(|e| e.to_string())?;
        match self.runtime.block_on(self.client.get_block(block_hash, false)) {
            Ok(b) => Ok(BlockObservation {
                sink: info.sink,
                virtual_daa: info.virtual_daa_score,
                known: true,
                is_chain_block: b.verbose_data.as_ref().is_some_and(|v| v.is_chain_block),
                daa_score: b.header.daa_score,
            }),
            Err(_) => Ok(BlockObservation {
                sink: info.sink,
                virtual_daa: info.virtual_daa_score,
                known: false,
                is_chain_block: false,
                daa_score: 0,
            }),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// The miner's own: executor and signer
// ---------------------------------------------------------------------------------------------------------------------------

/// The executor seam: an external program the miner supplies (see the module docs).
struct CommandExecutor {
    program: PathBuf,
    args: Vec<String>,
}

impl AttemptExecutor for CommandExecutor {
    fn execute(&mut self, anchor: Hash64) -> Result<ExecutedAttempt, String> {
        let out = std::process::Command::new(&self.program)
            .args(&self.args)
            .env("PALW_ANCHOR", anchor.to_string())
            .output()
            .map_err(|e| format!("cannot run the executor {}: {e}", self.program.display()))?;
        if !out.status.success() {
            return Err(format!("the executor exited with {}: {}", out.status, String::from_utf8_lossy(&out.stderr).trim()));
        }
        let v: serde_json::Value =
            serde_json::from_slice(&out.stdout).map_err(|e| format!("the executor printed no JSON object: {e}"))?;
        let field = |k: &str| -> Result<Hash64, String> {
            v[k].as_str()
                .ok_or_else(|| format!("the executor's output has no {k}"))?
                .parse::<Hash64>()
                .map_err(|_| format!("{k} is not a 128-hex hash"))
        };
        let material = match v["material_file"].as_str() {
            Some(path) => std::fs::read(path).map_err(|e| format!("cannot read the capture {path}: {e}"))?,
            None => Vec::new(),
        };
        Ok(ExecutedAttempt {
            execution: PalwAttemptExecutionV1 {
                trace_root: field("trace_root")?,
                output_root: field("output_root")?,
                execution_root: field("execution_root")?,
                trace_manifest_root: field("trace_manifest_root")?,
                trace_chunk_count: v["trace_chunk_count"].as_u64().ok_or("the executor's output has no trace_chunk_count")? as u32,
            },
            material,
        })
    }
}

/// The bond key with the node producer's equivocation journal: one challenge, one attempt id. Signing a DIFFERENT attempt at a position already
/// signed is refused before the key is touched.
struct JournaledSigner {
    key: ValidatorKey,
    journal: RefCell<kaspa_pq_validator_core::PalwAttemptJournalStore>,
}

impl AttemptSigner for JournaledSigner {
    fn public_key(&self) -> Vec<u8> {
        self.key.public_key().to_vec()
    }
    fn sign_attempt_id(&self, attempt_id: &Hash64) -> Result<Vec<u8>, String> {
        Ok(self.key.sign_with_context(attempt_id.as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT).to_vec())
    }
    fn sign_attempt(&self, attempt: &PalwAttemptUnsignedV2, attempt_id: &Hash64) -> Result<Vec<u8>, String> {
        let signature = self.sign_attempt_id(attempt_id)?;
        let record = PalwAttemptSignRecordV1 {
            challenge: attempt.challenge,
            attempt_id: *attempt_id,
            signature_fingerprint: Hash64::from_bytes(
                blake2b_simd::Params::new().hash_length(64).hash(&signature).as_bytes().try_into().expect("64 bytes"),
            ),
        };
        let mut journal = self.journal.borrow_mut();
        match journal.check(&record) {
            SignedEpochCheckOutcome::Allow => journal.record_and_flush(record)?,
            SignedEpochCheckOutcome::AllowRebroadcast => {}
            SignedEpochCheckOutcome::Block => {
                return Err(format!(
                    "the journal holds another attempt at challenge {}: signing a second one would be an equivocation",
                    attempt.challenge
                ));
            }
        }
        Ok(signature)
    }
}

// ---------------------------------------------------------------------------------------------------------------------------

struct Args {
    network: String,
    rpc: Vec<String>,
    bond: TransactionOutpoint,
    class: String,
    artifact_root: Hash64,
    pay_address: String,
    key_file: String,
    checkpoint: Checkpoint,
    pin: Option<Hash64>,
    executor: PathBuf,
    executor_args: Vec<String>,
    state_dir: PathBuf,
    steps: Option<u64>,
    poll_secs: u64,
    min_submit: Option<usize>,
}

fn parse_args() -> Args {
    let mut it = std::env::args().skip(1);
    let (mut network, mut rpc, mut bond, mut class, mut root, mut pay, mut key, mut checkpoint, mut pin) =
        (None, Vec::new(), None, None, None, None, None, None, None);
    let (mut executor, mut executor_args, mut state_dir, mut steps, mut poll, mut min_submit) =
        (None, Vec::new(), None, None, 5u64, None);
    while let Some(flag) = it.next() {
        let mut value = |name: &str| it.next().unwrap_or_else(|| die(format!("{name} needs a value")));
        match flag.as_str() {
            "--network" => network = Some(value("--network")),
            "--rpc" => rpc = value("--rpc").split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            "--bond" => bond = Some(outpoint(&value("--bond"))),
            "--class" => class = Some(value("--class")),
            "--artifact-root" => root = Some(hash("--artifact-root", &value("--artifact-root"))),
            "--pay-address" => pay = Some(value("--pay-address")),
            "--key-file" => key = Some(value("--key-file")),
            "--checkpoint" => {
                let v = value("--checkpoint");
                let (daa, h) = v.split_once(':').unwrap_or_else(|| die("--checkpoint is <daa>:<block hash>"));
                checkpoint = Some((
                    daa.parse::<u64>().unwrap_or_else(|_| die("--checkpoint daa is not a number")),
                    hash("--checkpoint hash", h),
                ));
            }
            "--pin" => pin = Some(hash("--pin", &value("--pin"))),
            "--executor-cmd" => executor = Some(PathBuf::from(value("--executor-cmd"))),
            "--executor-arg" => executor_args.push(value("--executor-arg")),
            "--state-dir" => state_dir = Some(PathBuf::from(value("--state-dir"))),
            "--steps" => steps = Some(value("--steps").parse().unwrap_or_else(|_| die("--steps is not a number"))),
            "--poll-secs" => poll = value("--poll-secs").parse().unwrap_or_else(|_| die("--poll-secs is not a number")),
            "--min-submit" => min_submit = Some(value("--min-submit").parse().unwrap_or_else(|_| die("--min-submit is not a number"))),
            other => die(format!("unknown flag {other}")),
        }
    }
    let network = network.unwrap_or_else(|| die("--network is required"));
    let (daa, block_hash) =
        checkpoint.unwrap_or_else(|| die("--checkpoint <daa>:<block hash> is required: the trust root the nodes are held to"));
    if rpc.len() < 2 {
        die("--rpc needs at least two independent nodes: one node's word is not a view");
    }
    Args {
        checkpoint: Checkpoint { network_id: network.clone(), daa_score: daa, block_hash },
        network,
        rpc,
        bond: bond.unwrap_or_else(|| die("--bond is required")),
        class: class.unwrap_or_else(|| die("--class is required")),
        artifact_root: root.unwrap_or_else(|| die("--artifact-root is required (the root of the artifact you hold)")),
        pay_address: pay.unwrap_or_else(|| die("--pay-address is required")),
        key_file: key.unwrap_or_else(|| die("--key-file is required")),
        pin,
        executor: executor.unwrap_or_else(|| die("--executor-cmd is required: this binary does not contain a model backend")),
        executor_args,
        state_dir: state_dir.unwrap_or_else(|| die("--state-dir is required")),
        steps,
        poll_secs: poll,
        min_submit,
    }
}

fn connect(runtime: &tokio::runtime::Runtime, endpoint: &str) -> Result<KaspaRpcClient, String> {
    let url = if endpoint.contains("://") { endpoint.to_string() } else { format!("ws://{endpoint}") };
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
        .map_err(|e| format!("cannot build a client for {url}: {e}"))?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(std::time::Duration::from_secs(10)),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    runtime.block_on(client.connect(Some(options))).map_err(|e| format!("cannot reach {url}: {e}"))?;
    Ok(client)
}

fn say(event: &str, fields: serde_json::Value) {
    println!("{}", serde_json::json!({ "event": event, "detail": fields }));
}

fn keep_material(dir: &Path, attempt_id: &Hash64, material: &[u8]) {
    let dir = dir.join("material");
    let _ = std::fs::create_dir_all(&dir);
    if let Err(e) = std::fs::write(dir.join(attempt_id.to_string()), material) {
        eprintln!(
            "[palw-remote-miner] WARNING: cannot keep the capture for {attempt_id}: {e} — the Panel will not be able to read it"
        );
    }
}

fn main() {
    let args = parse_args();
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap_or_else(|e| die(e));
    let net: NetworkId = args.network.parse().unwrap_or_else(|e| die(format!("--network: {e}")));
    let params = Params::from(net);
    let key = ValidatorKey::from_seed(load_validator_seed(&args.key_file).unwrap_or_else(|e| die(e)));
    let pubkey = key.public_key().to_vec();
    let prefix = params.prefix();
    let pay_address =
        kaspa_addresses::Address::try_from(args.pay_address.as_str()).unwrap_or_else(|e| die(format!("--pay-address: {e}")));
    if pay_address.prefix != prefix {
        die("--pay-address is not an address of this network");
    }

    let mut clients = Vec::new();
    for endpoint in &args.rpc {
        match connect(&runtime, endpoint) {
            Ok(client) => clients.push((endpoint.clone(), client)),
            Err(e) => eprintln!("[palw-remote-miner] {e} — not counted"),
        }
    }
    let nodes: Vec<WrpcNode<'_>> = clients
        .into_iter()
        .map(|(endpoint, client)| WrpcNode {
            endpoint,
            runtime: &runtime,
            client,
            pay_address: pay_address.clone(),
            class_id: args.class.clone(),
            bond: args.bond,
        })
        .collect();
    if nodes.len() < 2 {
        die("fewer than two nodes answered: a remote miner never acts on one node's word");
    }
    let refs: Vec<&dyn RemoteNode> = nodes.iter().map(|n| n as &dyn RemoteNode).collect();

    // The bond: PROVEN against a block you pinned, or what the nodes say (and labelled so).
    match args.pin {
        Some(pin) => {
            use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
            let proof = runtime
                .block_on(nodes[0].client.get_palw_state_proof(kaspa_rpc_core::GetPalwStateProofRequest {
                    block_hash: pin.to_string(),
                    collection: "bonds".into(),
                }))
                .unwrap_or_else(|e| die(format!("getPalwStateProof: {e}")));
            if !proof.available {
                die(format!("{} cannot prove the bond table at your pin: {}", nodes[0].endpoint, proof.reason));
            }
            let header =
                kaspa_consensus_core::header::Header::try_from(&proof.header.unwrap_or_else(|| die("the proof carries no header")))
                    .unwrap_or_else(|e| die(e));
            let fact = misaka_palw_remote::proof::proof_from_parts_v1(
                proof.state_preimage,
                "bonds",
                proof.rows.into_iter().map(|r| (r.key, r.value)).collect(),
            );
            match misaka_palw_remote::proof::verify_bond_against_pin(&header, pin, &fact, &PalwBondKeyV2(args.bond), &pubkey) {
                Ok(state) => say(
                    "bond-proven",
                    serde_json::json!({ "pin": pin.to_string(), "collateral": state.collateral, "status": format!("{:?}", state.status) }),
                ),
                Err(e) => die(format!("the bond is not proven against your pin: {e}")),
            }
        }
        None => say(
            "bond-unverified",
            serde_json::json!({ "label": misaka_palw_remote::trust::UNVERIFIED_REMOTE_STATE, "note": "the bond's registered key and standing are what the nodes report; pass --pin <block hash> to prove them" }),
        ),
    }

    let _ = std::fs::create_dir_all(&args.state_dir);
    let journal =
        kaspa_pq_validator_core::PalwAttemptJournalStore::load_or_empty(args.state_dir.join("attempt-journal.json"), key.validator_id)
            .unwrap_or_else(|e| die(e));
    let signer = JournaledSigner { key, journal: RefCell::new(journal) };
    let mut executor = CommandExecutor { program: args.executor.clone(), args: args.executor_args.clone() };

    let network_domain = palw_network_domain_v2_for(args.network.as_bytes(), Some(params.genesis.hash));
    let cfg = MinerConfig {
        quorum: QuorumPolicy::new(args.checkpoint.clone()),
        template: TemplatePolicy {
            min_agree: nodes.len().max(2),
            max_template_age_daa: TemplatePolicy::DEFAULT_MAX_TEMPLATE_AGE_DAA,
            held_pubkey: pubkey.clone(),
            held_artifact_root: args.artifact_root,
        },
        attempt: AttemptParams {
            network_id: args.network.clone(),
            network_domain,
            bond: args.bond,
            operator_id: kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&pubkey),
            // Read per template from the facts by the driver's policy; the class lottery bar is the facts' own (refreshed each step below).
            class_target: u128::MAX,
            witness_chunks: 0,
            single_lottery: false,
            signature_len: kaspa_txscript::MLDSA87_SIG_LEN,
        },
        class_id: hash("--class", &args.class),
        min_submit: args.min_submit.unwrap_or(nodes.len() / 2 + 1),
        grace_daa: 60,
        finality_depth: 60,
    };
    let mut state = MinerState::default();
    let mut trackers: Vec<BlockTracker> = Vec::new();
    // The network lottery against the header's own bits (past `palw_single_lottery` the chain admits the digest unconditionally).
    let network_id_bytes = args.network.clone().into_bytes();
    let network_draw = |header: &kaspa_consensus_core::header::Header, nonce: u64, _single: bool| -> bool {
        let single = params.palw_single_lottery_at(header.daa_score);
        kaspa_pow::StateLayer0::new(header, &network_id_bytes).check_pow_layer0_v2(nonce, single).map(|(ok, _)| ok).unwrap_or(false)
    };

    let mut n = 0u64;
    loop {
        if args.steps.is_some_and(|max| n >= max) {
            break;
        }
        n += 1;
        // The class lottery bar comes from the facts of THIS template: refresh it from the first node that answers (the driver's quorum
        // check has already insisted every node agrees on it).
        let mut cfg_step = cfg.clone();
        if let Some(t) = refs.iter().find_map(|r| r.fetch_template().ok()) {
            cfg_step.attempt.class_target = t.facts.class_target;
        }
        match step(&refs, &cfg_step, &mut state, &mut executor, &signer, &network_draw) {
            Ok(StepOutcome::DrawLost { bucket }) => say("draw-lost", serde_json::json!({ "bucket": bucket })),
            Ok(StepOutcome::Published(p)) => {
                keep_material(&args.state_dir, &p.attempt_id, &p.material);
                say(
                    "published",
                    serde_json::json!({
                        "block": p.block_hash.to_string(), "attempt_id": p.attempt_id.to_string(), "bucket": p.nonce_bucket,
                        "accepted_by": p.report.successes, "nodes": p.report.per_node.len(),
                        "note": "an ACK is not inclusion; the nodes validate the block themselves; keep serving the capture until it is final",
                    }),
                );
                let daa = refs.iter().find_map(|r| r.chain_facts().ok()).map(|f| f.virtual_daa).unwrap_or(0);
                trackers.push(BlockTracker::new(p.block_hash, daa, cfg.min_submit, cfg.finality_depth, cfg.grace_daa));
            }
            Ok(StepOutcome::Republished { block_hash, report }) => {
                say("republished", serde_json::json!({ "block": block_hash.to_string(), "accepted_by": report.successes }));
            }
            // A STOP with its reason; the loop waits and reads again. Nothing is retried on another view.
            Err(halt @ MinerHalt::Equivocation { .. }) => die(halt),
            Err(halt) => say("halt", serde_json::json!({ "reason": halt.to_string() })),
        }
        // Follow what was published: quorum observations, walking backwards on a reorg; a Lost block is re-sent as the SAME bytes.
        for tracker in trackers.iter_mut() {
            if tracker.state().is_settled() && !matches!(tracker.state(), BlockState::Lost) {
                continue;
            }
            let round: Vec<BlockObservation> = refs.iter().filter_map(|r| r.observe_block(tracker.block_hash).ok()).collect();
            let before = tracker.state().clone();
            let after = tracker.observe(&round).clone();
            if before != after {
                say(
                    "block-state",
                    serde_json::json!({ "block": tracker.block_hash.to_string(), "from": format!("{before:?}"), "to": format!("{after:?}"), "label": misaka_palw_remote::trust::UNVERIFIED_REMOTE_STATE }),
                );
            }
            if after == BlockState::Lost
                && let Some((_, block)) = state.published().find(|(_, b)| b.header.hash == tracker.block_hash)
            {
                let block = block.clone();
                match publish_block(&refs, &cfg, &block) {
                    Ok(r) => say("resent", serde_json::json!({ "block": tracker.block_hash.to_string(), "accepted_by": r.successes })),
                    Err(e) => {
                        say("resend-failed", serde_json::json!({ "block": tracker.block_hash.to_string(), "reason": e.to_string() }))
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(args.poll_secs));
    }
}
