//! **The pool's half: everything that needs a node, and nothing that needs a key.**
//!
//! The server is written against a seam ([`PoolChainV1`]) rather than against `kaspad` directly.
//! Not for taste — the pool's whole job is chain-shaped (facts, a template, a submission, a
//! gossip), so a server bolted to a live node would be a server that can only be tested by
//! standing up a network. Behind the seam is `kaspad/src/palw_pool.rs`; in the tests it is a fake
//! chain that answers the same four questions.
//!
//! # What the pool does with a solution, and what it refuses to do
//!
//! It mounts it. The miner's envelope is dropped into the header the pool handed out, the header
//! is finalized and the block is submitted — the pool does not re-sign, cannot re-sign, and does
//! not re-derive the attempt's fields, because every one of them is either the chain's (which the
//! pool relayed) or the execution's (which only the miner ran).
//!
//! It does re-check three cheap things first, because a pool that submits anything a socket hands
//! it is a pool that can be made to spend its node's block-submission path on garbage: the job id
//! must be one it issued, the nonce must be in the range it assigned, and the envelope must decode
//! and name the bond the session authenticated as. The expensive checks are consensus's, and
//! duplicating them here would be a second implementation to disagree with.
//!
//! **The material is retained and gossiped before the block is submitted**, in that order, for the
//! reason `palw_producer.rs` gives: an attempt promises a retention window, and a claim whose
//! material nobody serves cannot be licensed. The miner has no mouth to serve it from — that is
//! the whole reason it is pooling — so this is the obligation the pool takes on in exchange for
//! the miner's inference.

use crate::protocol::{JobV1, MinerMessageV1, PoolMessageV1, SolutionV1, from_hex, to_hex};
use crate::session::{BondStandingV1, GateRefusalV1, MinerIdentityV1, admit_v1, identify_v1};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use std::collections::HashMap;

/// The facts and powers a pool needs from the node it runs beside. Four questions, each of which
/// only a node can answer.
#[async_trait::async_trait]
pub trait PoolChainV1: Send + Sync {
    /// The network as consensus spells it (`testnet-11`) — part of the auth message and what
    /// Layer-0 is checked against.
    fn network_id(&self) -> String;
    /// The address prefix payouts must carry on this network.
    fn address_prefix(&self) -> kaspa_addresses::Prefix;
    /// The class this pool produces for. Constant for the pool's life — an operator chooses it at
    /// startup, and a pool that changed it under a connected miner would be handing out jobs for a
    /// class the miner resolved a different artifact for.
    fn job_for_class_id(&self) -> Hash64;
    /// What a miner needs to resolve the class locally: its id, the weights the chain registered,
    /// the borsh-encoded court params that decide the class's geometry, and whether it is the
    /// derived floor (which is also the answer to "is there anything to download?").
    async fn class_facts(&self) -> (Hash64, Hash64, Vec<u8>, bool);
    /// What the chain says about a bond, for the gate.
    async fn bond_standing(&self, class_id: Hash64, bond: TransactionOutpoint) -> Result<BondStandingV1, String>;
    /// A template built to pay THIS miner, with the class facts read at the same chain point.
    /// Both together, because a template from one chain point and facts from another is a block
    /// whose attempt admission checks against a state it was not built on.
    async fn job_for(&self, identity: MinerIdentityV1) -> Result<PreparedJobV1, String>;
    /// Retain the material for the promised window, gossip it to the panel, then submit the block.
    /// One call, because the order is load-bearing and a caller that could get it wrong eventually
    /// would.
    async fn publish(
        &self,
        attempt_id: Hash64,
        material: Vec<u8>,
        block: kaspa_consensus_core::block::Block,
    ) -> Result<Hash64, String>;
}

/// A template and the facts that go with it, as the node prepared them.
pub struct PreparedJobV1 {
    pub header: kaspa_consensus_core::header::Header,
    pub transactions: Vec<kaspa_consensus_core::tx::Transaction>,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub class_target: u128,
    pub pwu: u64,
    pub operator_id: Hash64,
    pub trace_retention_daa: u64,
}

/// How many nonces one job hands a miner. The producer's own bound, for the producer's own reason:
/// a template goes stale as the past-median time moves, so the search is bounded and the loop
/// refetches rather than grinding a dead template forever.
pub const NONCES_PER_JOB: u64 = 4_000_000;

/// One connected miner, after the gate.
pub struct MinerSessionV1 {
    pub identity: MinerIdentityV1,
    pub session_nonce: [u8; 32],
    pub authenticated: bool,
    /// The template behind the job this miner currently holds. Kept because the solution must be
    /// mounted into the very header whose pre-pow the miner derived its anchor from.
    pub outstanding: Option<OutstandingJobV1>,
    pub nonces_tried: u64,
    pub blocks: u64,
}

/// A job handed out and not yet answered.
pub struct OutstandingJobV1 {
    pub job_id: String,
    pub header: kaspa_consensus_core::header::Header,
    pub transactions: Vec<kaspa_consensus_core::tx::Transaction>,
    pub nonce_start: u64,
    pub nonce_end: u64,
}

/// Why a solution could not be mounted. Cheap refusals only — the expensive judgement is
/// consensus's, and this list deliberately does not duplicate it.
#[derive(Debug, PartialEq, Eq)]
pub enum MountRefusalV1 {
    /// No such job, or one already answered. Stale is the common case: the pool moved the template
    /// on while the miner was grinding.
    UnknownJob {
        job_id: String,
    },
    /// The nonce is outside the range this miner was assigned.
    NonceOutOfRange {
        nonce: u64,
        start: u64,
        end: u64,
    },
    /// The envelope did not decode as a `PalwAttemptEnvelopeV2`.
    Undecodable,
    /// The attempt names a bond other than the one this session authenticated as. **The one that
    /// matters**: without it an authenticated miner could mount attempts for somebody else's bond.
    NotThisBond,
    /// The attempt names a key other than the one the chain says this bond registered.
    NotThisKey,
    Malformed(String),
}

impl std::fmt::Display for MountRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownJob { job_id } => write!(f, "job {job_id} is not one this pool is waiting on (it most likely went stale)"),
            Self::NonceOutOfRange { nonce, start, end } => {
                write!(f, "nonce {nonce} is outside the assigned range [{start}, {end})")
            }
            Self::Undecodable => write!(f, "the envelope is not a PalwAttemptEnvelopeV2"),
            Self::NotThisBond => write!(f, "the attempt names a bond this session did not authenticate as"),
            Self::NotThisKey => write!(f, "the attempt names a key the chain does not say this bond registered"),
            Self::Malformed(what) => write!(f, "{what}"),
        }
    }
}

/// The pool's whole state: who is connected, and which bonds are taken.
#[derive(Default)]
pub struct PoolStateV1 {
    sessions: HashMap<u64, MinerSessionV1>,
    /// Bond text → session id. One bond, one session; see `session.rs` for why.
    bonds: HashMap<String, u64>,
    next_job: u64,
}

impl PoolStateV1 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn connected(&self) -> usize {
        self.sessions.len()
    }

    pub fn session(&self, id: u64) -> Option<&MinerSessionV1> {
        self.sessions.get(&id)
    }

    /// A miner said hello. Shape-checks it and opens an unauthenticated session; the challenge the
    /// caller sends back is this session's nonce.
    #[allow(clippy::too_many_arguments)]
    pub fn hello(
        &mut self,
        id: u64,
        protocol: u32,
        bond: &str,
        pubkey: &str,
        pay_address: &str,
        agent: &str,
        prefix: kaspa_addresses::Prefix,
        session_nonce: [u8; 32],
    ) -> Result<&MinerSessionV1, GateRefusalV1> {
        let identity = identify_v1(protocol, bond, pubkey, pay_address, agent, prefix)?;
        // One bond, one session — and a stale session for that bond does not hold it forever, so
        // the check is against LIVE sessions only (`drop_session` releases it).
        if let Some(holder) = self.bonds.get(&identity.bond_text)
            && *holder != id
        {
            return Err(GateRefusalV1::BondAlreadyConnected { bond: identity.bond_text.clone() });
        }
        self.bonds.insert(identity.bond_text.clone(), id);
        self.sessions.insert(
            id,
            MinerSessionV1 { identity, session_nonce, authenticated: false, outstanding: None, nonces_tried: 0, blocks: 0 },
        );
        Ok(self.sessions.get(&id).expect("just inserted"))
    }

    /// The signature came back. On success the session may take work.
    pub fn authenticate(
        &mut self,
        id: u64,
        standing: &BondStandingV1,
        network_id: &str,
        signature: &[u8],
    ) -> Result<(), GateRefusalV1> {
        let session = self.sessions.get_mut(&id).ok_or_else(|| GateRefusalV1::Malformed("no such session".into()))?;
        admit_v1(&session.identity, standing, &session.session_nonce, network_id, signature)?;
        session.authenticated = true;
        Ok(())
    }

    /// Turn a node-prepared template into the job a miner receives, and remember the template.
    pub fn issue_job(&mut self, id: u64, prepared: PreparedJobV1) -> Result<JobV1, String> {
        self.next_job += 1;
        let job_id = format!("j{}", self.next_job);
        let session = self.sessions.get_mut(&id).ok_or("no such session")?;
        if !session.authenticated {
            return Err("this session has not proved its bond".into());
        }
        let (nonce_start, nonce_end) = (0u64, NONCES_PER_JOB);
        let job = JobV1 {
            job_id: job_id.clone(),
            header: to_hex(&borsh::to_vec(&prepared.header).map_err(|e| format!("header does not encode: {e}"))?),
            transactions: prepared
                .transactions
                .iter()
                .map(|t| borsh::to_vec(t).map(|b| to_hex(&b)).map_err(|e| format!("a transaction does not encode: {e}")))
                .collect::<Result<Vec<_>, _>>()?,
            class_id: prepared.class_id.to_string(),
            artifact_root: prepared.artifact_root.to_string(),
            class_target: prepared.class_target.to_string(),
            pwu: prepared.pwu,
            operator_id: prepared.operator_id.to_string(),
            trace_retention_daa: prepared.trace_retention_daa,
            nonce_start,
            nonce_end,
        };
        session.outstanding =
            Some(OutstandingJobV1 { job_id, header: prepared.header, transactions: prepared.transactions, nonce_start, nonce_end });
        Ok(job)
    }

    /// **Check a solution and build the block it belongs in.** Cheap checks only; see the module
    /// docs for which ones and why the list stops where it does.
    pub fn mount(
        &mut self,
        id: u64,
        solution: &SolutionV1,
        registered_pubkey: &[u8],
    ) -> Result<(Hash64, Vec<u8>, kaspa_consensus_core::block::Block), MountRefusalV1> {
        let session = self.sessions.get_mut(&id).ok_or_else(|| MountRefusalV1::Malformed("no such session".into()))?;
        let outstanding =
            session.outstanding.as_ref().ok_or_else(|| MountRefusalV1::UnknownJob { job_id: solution.job_id.clone() })?;
        if outstanding.job_id != solution.job_id {
            return Err(MountRefusalV1::UnknownJob { job_id: solution.job_id.clone() });
        }
        if solution.nonce < outstanding.nonce_start || solution.nonce >= outstanding.nonce_end {
            return Err(MountRefusalV1::NonceOutOfRange {
                nonce: solution.nonce,
                start: outstanding.nonce_start,
                end: outstanding.nonce_end,
            });
        }
        let envelope_bytes = from_hex(&solution.envelope).map_err(|e| MountRefusalV1::Malformed(format!("envelope: {e}")))?;
        let envelope = kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&envelope_bytes)
            .map_err(|_| MountRefusalV1::Undecodable)?;
        // The session authenticated as one bond. An attempt naming another is not this miner's to
        // mount, whatever it did to produce it.
        if envelope.attempt.executor_bond != session.identity.bond {
            return Err(MountRefusalV1::NotThisBond);
        }
        if envelope.attempt.executor_pubkey != registered_pubkey {
            return Err(MountRefusalV1::NotThisKey);
        }
        let material = from_hex(&solution.material).map_err(|e| MountRefusalV1::Malformed(format!("material: {e}")))?;
        let attempt_id = kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&envelope.attempt);

        let mut header = outstanding.header.clone();
        header.nonce = solution.nonce;
        header.palw_commitment = envelope_bytes;
        header.finalize();
        let block = kaspa_consensus_core::block::MutableBlock::new(header, outstanding.transactions.clone()).to_immutable();
        session.outstanding = None;
        session.blocks += 1;
        Ok((attempt_id, material, block))
    }

    /// The miner reported a finished range. Retires the job and books the work.
    pub fn exhausted(&mut self, id: u64, job_id: &str, nonces_tried: u64) {
        if let Some(session) = self.sessions.get_mut(&id) {
            session.nonces_tried = session.nonces_tried.saturating_add(nonces_tried);
            if session.outstanding.as_ref().is_some_and(|o| o.job_id == job_id) {
                session.outstanding = None;
            }
        }
    }

    /// The connection went away. Releases the bond so the same miner can reconnect.
    pub fn drop_session(&mut self, id: u64) {
        if let Some(session) = self.sessions.remove(&id) {
            // Only if this session still holds it — a reconnect that already took the bond must
            // not have it pulled out from under it by the old socket's cleanup.
            if self.bonds.get(&session.identity.bond_text) == Some(&id) {
                self.bonds.remove(&session.identity.bond_text);
            }
        }
    }
}

/// Render a gate refusal as the message that goes down the wire.
pub fn rejection(refusal: &GateRefusalV1) -> PoolMessageV1 {
    PoolMessageV1::Rejected { reason: refusal.to_string() }
}

/// The `Challenge` a fresh session answers with.
pub fn challenge_for(session_nonce: &[u8; 32], network_id: &str) -> PoolMessageV1 {
    PoolMessageV1::Challenge {
        protocol: crate::protocol::PALW_POOL_PROTOCOL_VERSION,
        session_nonce: to_hex(session_nonce),
        network_id: network_id.to_string(),
    }
}

/// Read the signature out of an `Auth`, or say what arrived instead.
pub fn auth_signature(message: &MinerMessageV1) -> Result<Vec<u8>, String> {
    match message {
        MinerMessageV1::Auth { signature } => from_hex(signature).map_err(|e| format!("signature: {e}")),
        other => Err(format!("expected an auth message, got {other:?}")),
    }
}

// ---------------------------------------------------------------------------------------------
// The listener
// ---------------------------------------------------------------------------------------------

/// **Serve miners over TCP until told to stop.**
///
/// One task per connection, one `PoolStateV1` behind a mutex for all of them. The mutex is not a
/// bottleneck and would be the wrong thing to optimize: everything expensive in this system is on
/// the other side of the socket (a miner's inference) or on the other side of the seam (a block
/// submission), and what happens under the lock is a hash-map lookup and a header clone.
pub async fn serve_v1(
    chain: std::sync::Arc<dyn PoolChainV1>,
    listener: tokio::net::TcpListener,
    state: std::sync::Arc<tokio::sync::Mutex<PoolStateV1>>,
    max_miners: usize,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut next_id = 0u64;
    loop {
        let accepted = tokio::select! {
            r = listener.accept() => r,
            _ = shutdown.changed() => break,
        };
        let (stream, peer) = match accepted {
            Ok(pair) => pair,
            Err(e) => {
                kaspa_core::warn!("[palw-pool] accept failed: {e}");
                continue;
            }
        };
        {
            let live = state.lock().await;
            if live.connected() >= max_miners {
                kaspa_core::warn!("[palw-pool] {peer} refused: this pool is serving its {max_miners}-miner limit");
                continue;
            }
        }
        next_id += 1;
        let (id, chain, state, shutdown) = (next_id, chain.clone(), state.clone(), shutdown.clone());
        tokio::spawn(async move {
            if let Err(e) = session_loop_v1(id, chain, stream, peer, state.clone(), shutdown).await {
                kaspa_core::debug!("[palw-pool] session {id} ({peer}) ended: {e}");
            }
            state.lock().await.drop_session(id);
        });
    }
}

/// One message, written as its line. A free function rather than a closure: a closure that
/// borrows the writer and returns a future cannot name the lifetime that ties the two together.
async fn send_line(message: PoolMessageV1, out: &mut tokio::net::tcp::OwnedWriteHalf) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let line = crate::protocol::encode_line(&message)?;
    out.write_all(line.as_bytes()).await.map_err(|e| format!("write: {e}"))
}

/// One connection, from hello to hang-up.
async fn session_loop_v1(
    id: u64,
    chain: std::sync::Arc<dyn PoolChainV1>,
    stream: tokio::net::TcpStream,
    peer: std::net::SocketAddr,
    state: std::sync::Arc<tokio::sync::Mutex<PoolStateV1>>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();
    let network_id = chain.network_id();

    // A session nonce this connection and no other will ever be asked to sign over.
    let mut session_nonce = [0u8; 32];
    {
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut session_nonce);
    }
    // The chain's answer about the bond, cached from the gate — `mount` re-checks the attempt's
    // key against it, so it must be the CHAIN's copy rather than anything the miner sent.
    let mut registered_pubkey: Vec<u8> = Vec::new();

    loop {
        let line = tokio::select! {
            l = lines.next_line() => l.map_err(|e| format!("read: {e}"))?,
            _ = shutdown.changed() => return Ok(()),
        };
        let Some(line) = line else { return Ok(()) };
        if line.trim().is_empty() {
            continue;
        }
        let message: MinerMessageV1 = match crate::protocol::parse_line(line.trim()) {
            Ok(m) => m,
            Err(e) => {
                send_line(PoolMessageV1::Rejected { reason: e }, &mut write_half).await?;
                return Ok(());
            }
        };

        match message {
            MinerMessageV1::Hello { protocol, bond, pubkey, pay_address, agent } => {
                let outcome = {
                    let mut live = state.lock().await;
                    live.hello(id, protocol, &bond, &pubkey, &pay_address, &agent, chain.address_prefix(), session_nonce)
                        .map(|s| (s.identity.bond, s.identity.agent.clone()))
                };
                match outcome {
                    Err(refusal) => {
                        kaspa_core::info!("[palw-pool] {peer} refused: {refusal}");
                        send_line(rejection(&refusal), &mut write_half).await?;
                        return Ok(());
                    }
                    Ok((bond_outpoint, agent)) => {
                        // The bond is looked up against the chain BEFORE the challenge, so an
                        // unbonded miner is told what is wrong rather than being asked to sign.
                        let class_id = chain.job_for_class_id();
                        match chain.bond_standing(class_id, bond_outpoint).await {
                            Ok(standing) if standing.known => {
                                registered_pubkey = standing.registered_pubkey.clone();
                                kaspa_core::info!("[palw-pool] {peer} says hello for bond {bond} ({agent})");
                                send_line(challenge_for(&session_nonce, &network_id), &mut write_half).await?;
                            }
                            Ok(_) => {
                                let refusal = GateRefusalV1::BondUnknown { bond: bond.clone() };
                                kaspa_core::info!("[palw-pool] {peer} refused: {refusal}");
                                send_line(rejection(&refusal), &mut write_half).await?;
                                return Ok(());
                            }
                            Err(e) => {
                                send_line(PoolMessageV1::Standby { reason: e, retry_after_ms: 5_000 }, &mut write_half).await?;
                                return Ok(());
                            }
                        }
                    }
                }
            }
            MinerMessageV1::Auth { signature } => {
                let signature = crate::protocol::from_hex(&signature).unwrap_or_default();
                let bond = {
                    let live = state.lock().await;
                    live.session(id).map(|s| s.identity.bond)
                };
                let Some(bond) = bond else {
                    send_line(PoolMessageV1::Rejected { reason: "auth before hello".into() }, &mut write_half).await?;
                    return Ok(());
                };
                // Re-read the standing at auth time rather than trusting the hello's snapshot: a
                // bond can lose its room between two messages, and the miner should learn that
                // now rather than after an inference.
                let class_id = chain.job_for_class_id();
                let standing = match chain.bond_standing(class_id, bond).await {
                    Ok(s) => s,
                    Err(e) => {
                        send_line(PoolMessageV1::Standby { reason: e, retry_after_ms: 5_000 }, &mut write_half).await?;
                        return Ok(());
                    }
                };
                registered_pubkey = standing.registered_pubkey.clone();
                let admitted = {
                    let mut live = state.lock().await;
                    live.authenticate(id, &standing, &network_id, &signature)
                };
                match admitted {
                    Err(refusal) => {
                        kaspa_core::info!("[palw-pool] {peer} refused at auth: {refusal}");
                        send_line(rejection(&refusal), &mut write_half).await?;
                        return Ok(());
                    }
                    Ok(()) => {
                        let (class_id, artifact_root, court, is_base) = chain.class_facts().await;
                        kaspa_core::info!("[palw-pool] {peer} admitted on a bond the chain holds");
                        send_line(
                            PoolMessageV1::Welcome {
                                class_id: class_id.to_string(),
                                artifact_root: artifact_root.to_string(),
                                court: to_hex(&court),
                                is_base_class: is_base,
                            },
                            &mut write_half,
                        )
                        .await?;
                    }
                }
            }
            MinerMessageV1::JobRequest { finished } => {
                if let Some(job_id) = finished {
                    state.lock().await.exhausted(id, &job_id, 0);
                }
                let identity = {
                    let live = state.lock().await;
                    match live.session(id) {
                        Some(s) if s.authenticated => s.identity.clone(),
                        _ => {
                            send_line(
                                PoolMessageV1::Rejected { reason: "ask for work after proving your bond".into() },
                                &mut write_half,
                            )
                            .await?;
                            return Ok(());
                        }
                    }
                };
                match chain.job_for(identity).await {
                    Ok(prepared) => {
                        let job = { state.lock().await.issue_job(id, prepared) };
                        match job {
                            Ok(job) => send_line(PoolMessageV1::Job(Box::new(job)), &mut write_half).await?,
                            Err(e) => send_line(PoolMessageV1::Standby { reason: e, retry_after_ms: 2_000 }, &mut write_half).await?,
                        }
                    }
                    Err(e) => send_line(PoolMessageV1::Standby { reason: e, retry_after_ms: 2_000 }, &mut write_half).await?,
                }
            }
            MinerMessageV1::Solution(solution) => {
                let mounted = {
                    let mut live = state.lock().await;
                    live.mount(id, &solution, &registered_pubkey)
                };
                let result = match mounted {
                    Err(refusal) => PoolMessageV1::SolutionResult {
                        job_id: solution.job_id.clone(),
                        accepted: false,
                        block_hash: String::new(),
                        reason: refusal.to_string(),
                    },
                    Ok((attempt_id, material, block)) => {
                        // Retention, gossip, submission — in that order, behind the seam.
                        match chain.publish(attempt_id, material, block).await {
                            Ok(hash) => {
                                kaspa_core::info!("[palw-pool] {peer} produced block {hash}");
                                PoolMessageV1::SolutionResult {
                                    job_id: solution.job_id.clone(),
                                    accepted: true,
                                    block_hash: hash.to_string(),
                                    reason: String::new(),
                                }
                            }
                            Err(e) => {
                                kaspa_core::warn!("[palw-pool] {peer}'s block was refused: {e}");
                                PoolMessageV1::SolutionResult {
                                    job_id: solution.job_id.clone(),
                                    accepted: false,
                                    block_hash: String::new(),
                                    reason: e,
                                }
                            }
                        }
                    }
                };
                send_line(result, &mut write_half).await?;
            }
            MinerMessageV1::Exhausted { job_id, nonces_tried } => {
                state.lock().await.exhausted(id, &job_id, nonces_tried);
            }
            MinerMessageV1::Heartbeat { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_addresses::{Prefix, Version};
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_COINBASE;
    use kaspa_consensus_core::tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionOutput};

    fn address() -> String {
        kaspa_addresses::Address::new(Prefix::Testnet, Version::PubKeyHashMlDsa87, &[3u8; 64]).to_string()
    }

    fn bond_text(word: u64) -> String {
        format!("{}:0", kaspa_consensus_core::tx::TransactionId::from_u64_word(word))
    }

    fn prepared() -> PreparedJobV1 {
        let transactions = vec![Transaction::new(
            0,
            Vec::new(),
            vec![TransactionOutput::new(5000, ScriptPublicKey::new(0, ScriptVec::from_slice(&[0xAA; 34])))],
            0,
            SUBNETWORK_ID_COINBASE,
            0,
            Vec::new(),
        )];
        let mut header = kaspa_consensus_core::header::Header::from_precomputed_hash(Hash64::from_u64_word(1), Vec::new());
        header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2;
        header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(transactions.iter());
        PreparedJobV1 {
            header,
            transactions,
            class_id: Hash64::from_u64_word(11),
            artifact_root: Hash64::from_u64_word(12),
            class_target: u128::MAX,
            pwu: 7900,
            operator_id: Hash64::from_u64_word(13),
            trace_retention_daa: 500,
        }
    }

    fn admitted(pool: &mut PoolStateV1, id: u64, kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair, bond: &str) -> BondStandingV1 {
        pool.hello(id, 1, bond, &to_hex(kp.verification_key.as_ref()), &address(), "t", Prefix::Testnet, [0x42u8; 32]).expect("hello");
        let standing =
            BondStandingV1 { known: true, registered_pubkey: kp.verification_key.as_ref().to_vec(), not_ready_reason: String::new() };
        let sig = crate::session::sign_pool_auth_v1(
            &kp.signing_key,
            &[0x42u8; 32],
            "testnet-11",
            bond,
            &standing.registered_pubkey,
            &address(),
        )
        .expect("signs");
        pool.authenticate(id, &standing, "testnet-11", &sig).expect("admitted");
        standing
    }

    /// The lifecycle: hello, auth, job, and the job is remembered against the session.
    #[test]
    fn an_admitted_miner_gets_a_job_the_pool_remembers() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x11u8; 32]);
        let mut pool = PoolStateV1::new();
        admitted(&mut pool, 1, &kp, &bond_text(0xB0));
        let job = pool.issue_job(1, prepared()).expect("a job");
        assert_eq!(job.nonce_end, NONCES_PER_JOB);
        assert_eq!(job.pwu, 7900);
        assert!(!job.transactions.is_empty(), "the coinbase travels so the miner can check who is paid");
        assert_eq!(pool.session(1).expect("session").outstanding.as_ref().expect("held").job_id, job.job_id);
    }

    /// **An unauthenticated session gets no work**, however well-formed its hello was.
    #[test]
    fn a_session_that_has_not_proved_its_bond_gets_no_work() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x11u8; 32]);
        let mut pool = PoolStateV1::new();
        pool.hello(1, 1, &bond_text(0xB0), &to_hex(kp.verification_key.as_ref()), &address(), "t", Prefix::Testnet, [0x42u8; 32])
            .expect("hello");
        assert!(pool.issue_job(1, prepared()).is_err(), "a hello is a claim, not a credential");
    }

    /// One bond, one session — and the bond comes free again when the socket goes.
    #[test]
    fn one_bond_is_one_session_and_a_disconnect_releases_it() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x11u8; 32]);
        let mut pool = PoolStateV1::new();
        let bond = bond_text(0xB0);
        admitted(&mut pool, 1, &kp, &bond);
        let second = pool.hello(2, 1, &bond, &to_hex(kp.verification_key.as_ref()), &address(), "t", Prefix::Testnet, [0x43u8; 32]);
        assert_eq!(second.err(), Some(GateRefusalV1::BondAlreadyConnected { bond: bond.clone() }));

        pool.drop_session(1);
        assert!(
            pool.hello(2, 1, &bond, &to_hex(kp.verification_key.as_ref()), &address(), "t", Prefix::Testnet, [0x43u8; 32]).is_ok(),
            "a reconnect after a disconnect is the ordinary case and must work"
        );
    }

    /// A solution mounts into the very header the job was cut from, and the pool hands back the
    /// attempt id, the material and the block — the three things publishing needs.
    #[test]
    fn a_solution_mounts_into_the_header_it_was_cut_from() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x11u8; 32]);
        let mut pool = PoolStateV1::new();
        let bond = bond_text(0xB0);
        let standing = admitted(&mut pool, 1, &kp, &bond);
        let job = pool.issue_job(1, prepared()).expect("a job");

        let attempt = kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2 {
            version: kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION,
            network_domain: Hash64::from_u64_word(0xD0),
            challenge: Hash64::from_u64_word(0xC0),
            class_id: Hash64::from_u64_word(11),
            executor_bond: kaspa_consensus_core::tx::TransactionOutpoint::new(
                kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0),
                0,
            ),
            executor_pubkey: kp.verification_key.as_ref().to_vec(),
            operator_id: Hash64::from_u64_word(13),
            artifact_root: Hash64::from_u64_word(12),
            trace_root: Hash64::from_u64_word(4),
            output_root: Hash64::from_u64_word(5),
            execution_root: Hash64::from_u64_word(6),
            pwu: 7900,
            trace_manifest_root: Hash64::from_u64_word(7),
            trace_chunk_count: 1,
            trace_retention_daa: 500,
        };
        let envelope =
            kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2 { attempt: attempt.clone(), signature: vec![0u8; 4627] };
        let solution = SolutionV1 {
            job_id: job.job_id.clone(),
            nonce: 7,
            envelope: to_hex(&envelope.encode_wire()),
            material: to_hex(&[1u8, 2, 3]),
        };
        let (attempt_id, material, block) = pool.mount(1, &solution, &standing.registered_pubkey).expect("mounts");
        assert_eq!(attempt_id, kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&attempt));
        assert_eq!(material, vec![1u8, 2, 3]);
        assert_eq!(block.header.nonce, 7, "the nonce the miner won on");
        assert!(!block.header.palw_commitment.is_empty(), "the miner's envelope rides in the header");
        assert!(pool.session(1).expect("session").outstanding.is_none(), "an answered job is retired");
        assert_eq!(pool.session(1).expect("session").blocks, 1);
    }

    /// **An authenticated miner cannot mount an attempt for somebody else's bond** — the check
    /// that keeps a session's authority to its own bond.
    #[test]
    fn a_solution_for_another_bond_is_refused() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x11u8; 32]);
        let mut pool = PoolStateV1::new();
        let standing = admitted(&mut pool, 1, &kp, &bond_text(0xB0));
        let job = pool.issue_job(1, prepared()).expect("a job");

        let mut attempt = kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2 {
            version: kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION,
            network_domain: Hash64::from_u64_word(0xD0),
            challenge: Hash64::from_u64_word(0xC0),
            class_id: Hash64::from_u64_word(11),
            // Somebody else's bond.
            executor_bond: kaspa_consensus_core::tx::TransactionOutpoint::new(
                kaspa_consensus_core::tx::TransactionId::from_u64_word(0xFF),
                0,
            ),
            executor_pubkey: kp.verification_key.as_ref().to_vec(),
            operator_id: Hash64::from_u64_word(13),
            artifact_root: Hash64::from_u64_word(12),
            trace_root: Hash64::from_u64_word(4),
            output_root: Hash64::from_u64_word(5),
            execution_root: Hash64::from_u64_word(6),
            pwu: 7900,
            trace_manifest_root: Hash64::from_u64_word(7),
            trace_chunk_count: 1,
            trace_retention_daa: 500,
        };
        let wire = |a: &kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2| {
            to_hex(
                &kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2 { attempt: a.clone(), signature: vec![0u8; 4627] }
                    .encode_wire(),
            )
        };
        let solution = SolutionV1 { job_id: job.job_id.clone(), nonce: 7, envelope: wire(&attempt), material: String::new() };
        assert_eq!(pool.mount(1, &solution, &standing.registered_pubkey).err(), Some(MountRefusalV1::NotThisBond));

        // Right bond, wrong key: also refused.
        attempt.executor_bond =
            kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0), 0);
        attempt.executor_pubkey = vec![9u8; 2592];
        let solution = SolutionV1 { job_id: job.job_id.clone(), nonce: 7, envelope: wire(&attempt), material: String::new() };
        assert_eq!(pool.mount(1, &solution, &standing.registered_pubkey).err(), Some(MountRefusalV1::NotThisKey));
    }

    /// Stale jobs, out-of-range nonces and undecodable envelopes are refused cheaply and by name.
    #[test]
    fn a_stale_or_out_of_range_solution_is_refused_by_name() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x11u8; 32]);
        let mut pool = PoolStateV1::new();
        let standing = admitted(&mut pool, 1, &kp, &bond_text(0xB0));
        let job = pool.issue_job(1, prepared()).expect("a job");

        let stale = SolutionV1 { job_id: "j999".into(), nonce: 1, envelope: String::new(), material: String::new() };
        assert!(matches!(pool.mount(1, &stale, &standing.registered_pubkey), Err(MountRefusalV1::UnknownJob { .. })));

        let far = SolutionV1 { job_id: job.job_id.clone(), nonce: NONCES_PER_JOB, envelope: String::new(), material: String::new() };
        assert!(matches!(pool.mount(1, &far, &standing.registered_pubkey), Err(MountRefusalV1::NonceOutOfRange { .. })));

        let junk = SolutionV1 { job_id: job.job_id.clone(), nonce: 1, envelope: to_hex(&[0u8; 8]), material: String::new() };
        assert_eq!(pool.mount(1, &junk, &standing.registered_pubkey).err(), Some(MountRefusalV1::Undecodable));

        // An exhausted report retires the job and books the work.
        pool.exhausted(1, &job.job_id, 4_000_000);
        assert_eq!(pool.session(1).expect("session").nonces_tried, 4_000_000);
        assert!(pool.session(1).expect("session").outstanding.is_none());
    }
}
