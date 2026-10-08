//! **The remote attempt miner's driver (RFC-0009 stage A): one step of "read the chain through several nodes, mount an attempt, publish it, follow it".**
//!
//! This is the loop body of a miner that runs no `kaspad`. It owns no socket: the binary supplies [`RemoteNode`]s and the miner's own
//! executor and signer, and everything that decides is here and unit-tested against fake nodes:
//!
//! ```text
//!   view::agree            several nodes on one network, one pruning point, within a DAA skew, none contradicting the pinned checkpoint
//!   fetch_template × N     getBlockTemplate (+ producer facts) from every node → TemplateObservation
//!   check_templates        the SAME digest everywhere, fresh, the held key is the bond's key, the held artifact is the class's  ── else STOP
//!   class / bond identity  the facts name OUR class and OUR key; a stale or foreign work identity is refused before any inference
//!   mount_attempt          execute (the miner's machine) → class lottery → network lottery → sign ONCE, only on a win
//!   ready_to_publish       a FRESH quorum still stands on the same chain point and inside the freshness bound
//!   publish                the finished block to every node, idempotently; one position, one attempt (no second block at a position)
//!   BlockTracker           known → selected chain → settled; a reorg walks it backwards; nothing is re-signed or re-published as another block
//! ```
//!
//! **Every node-supplied fact stays `UNVERIFIED_REMOTE_STATE`** unless the binary proved it against a pinned block (`crate::proof`); the quorum
//! makes one lying node useless and is not a proof. **The node validates the block independently** — nothing here is an authority over what the
//! chain accepts; a published block is only a candidate until the chain's own rules take it.

use std::collections::BTreeMap;

use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::header::Header;
use kaspa_hashes::Hash64;

use crate::attempt::{AttemptError, AttemptExecutor, AttemptParams, AttemptSigner, MountedAttempt, mount_attempt, ready_to_publish};
use crate::checkpoint::Checkpoint;
use crate::relay::{RelayFailure, RelayReport, Reply, fan_out_verdict};
use crate::template::{
    AcceptedTemplate, ProducerFactsSummary, TemplatePolicy, TemplateRefusal, check_templates, template_observation_v1,
};
use crate::view::{AgreedView, ChainView, CheckpointStatus, Halt as ViewHalt, NodeFacts, QuorumPolicy, ViewError, agree};

/// What `getBlockTemplate` + `getPalwProducerFacts` gave for one node.
#[derive(Clone, Debug)]
pub struct NodeTemplate {
    pub block: Block,
    pub facts: ProducerFactsSummary,
}

/// One node's answer about a block we published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockObservation {
    pub sink: Hash64,
    pub virtual_daa: u64,
    /// The node holds the block (any status).
    pub known: bool,
    /// The block is on the node's selected chain.
    pub is_chain_block: bool,
    pub daa_score: u64,
}

/// One node a remote miner talks to. The binary implements it over wRPC; tests implement it over a script.
pub trait RemoteNode {
    fn node_id(&self) -> &str;
    fn chain_facts(&self) -> Result<NodeFacts, ViewError>;
    fn checkpoint_status(&self, checkpoint: &Checkpoint) -> Result<CheckpointStatus, ViewError>;
    fn fetch_template(&self) -> Result<NodeTemplate, String>;
    /// Submit the finished block. `Accepted`/`AlreadyKnown` carry the hash the NODE holds.
    fn submit_block(&self, block: &Block) -> Reply;
    fn observe_block(&self, hash: Hash64) -> Result<BlockObservation, String>;
}

struct ViewAdapter<'a>(&'a dyn RemoteNode);

impl ChainView for ViewAdapter<'_> {
    fn node_id(&self) -> &str {
        self.0.node_id()
    }
    fn facts(&self) -> Result<NodeFacts, ViewError> {
        self.0.chain_facts()
    }
    fn checkpoint_status(&self, checkpoint: &Checkpoint) -> Result<CheckpointStatus, ViewError> {
        self.0.checkpoint_status(checkpoint)
    }
}

/// What the miner holds and the rules it runs under.
#[derive(Clone)]
pub struct MinerConfig {
    pub quorum: QuorumPolicy,
    pub template: TemplatePolicy,
    pub attempt: AttemptParams,
    /// The class this miner mines. A node whose facts name another class is not describing this miner's work.
    pub class_id: Hash64,
    /// Nodes that must accept the finished block (≥ 1; the binary defaults to a majority).
    pub min_submit: usize,
    /// A published block that no quorum node knows after this many DAA is `Lost`.
    pub grace_daa: u64,
    /// Chain blocks this deep are settled.
    pub finality_depth: u64,
}

/// Why a step produced no block. Every one is a STOP with a reason, never a silent retry with another view.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MinerHalt {
    #[error("the chain view halted: {0}")]
    View(#[from] ViewHalt),
    #[error("no node produced a template ({0:?})")]
    NoTemplates(Vec<(String, String)>),
    #[error("the template was refused: {0}")]
    Template(#[from] TemplateRefusal),
    #[error("the facts name class {got}, this miner mines {want}: not this miner's work identity")]
    WrongClass { got: Hash64, want: Hash64 },
    #[error("the attempt failed: {0}")]
    Attempt(#[from] AttemptError),
    #[error("position {challenge} already carries attempt {earlier}; a second attempt there would be an equivocation")]
    Equivocation { challenge: Hash64, earlier: Hash64 },
    #[error("the finished block was not taken: {0}")]
    Publish(#[from] RelayFailure),
}

#[derive(Clone, Debug)]
pub enum StepOutcome {
    /// Both lotteries lost: the normal case. The next call walks to the next bucket of the same template.
    DrawLost { bucket: u64 },
    /// A won draw, finished, re-checked against a fresh quorum, and given to the nodes.
    Published(Published),
    /// Re-publishing a block this miner already made (a node that restarted, a lost ACK): the SAME bytes, idempotent.
    Republished { block_hash: Hash64, report: RelayReport },
}

#[derive(Clone, Debug)]
pub struct Published {
    pub block_hash: Hash64,
    pub attempt_id: Hash64,
    pub challenge: Hash64,
    pub nonce_bucket: u64,
    pub report: RelayReport,
    pub template_digest: Hash64,
    pub material: Vec<u8>,
}

/// What a miner remembers between steps: the bucket cursor, the one attempt per position, and the blocks it published.
#[derive(Default)]
pub struct MinerState {
    cursor: Option<(Hash64, u64)>,
    /// `challenge → (attempt id, block)`: one position, one attempt, one block.
    positions: BTreeMap<Hash64, (Hash64, Block)>,
}

impl MinerState {
    pub fn published(&self) -> impl Iterator<Item = (&Hash64, &Block)> {
        self.positions.values().map(|(id, b)| (id, b))
    }
}

fn gather(nodes: &[&dyn RemoteNode], network_id: &str) -> (Vec<(String, NodeTemplate)>, Vec<(String, String)>) {
    let mut ok = Vec::new();
    let mut silent = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for node in nodes.iter().filter(|n| seen.insert(n.node_id().to_string())) {
        match node.fetch_template() {
            Ok(t) => ok.push((node.node_id().to_string(), t)),
            Err(e) => silent.push((node.node_id().to_string(), e)),
        }
    }
    let _ = network_id;
    (ok, silent)
}

fn observations(templates: &[(String, NodeTemplate)], network_id: &str) -> Vec<crate::template::TemplateObservation> {
    templates.iter().map(|(n, t)| template_observation_v1(n, network_id, &t.block.header, t.facts.clone())).collect()
}

/// **One step.** Reads the chain, mounts the next bucket of the current template, and publishes if both lotteries are won.
pub fn step(
    nodes: &[&dyn RemoteNode],
    cfg: &MinerConfig,
    state: &mut MinerState,
    executor: &mut dyn AttemptExecutor,
    signer: &dyn AttemptSigner,
    network_draw: &dyn Fn(&Header, u64, bool) -> bool,
) -> Result<StepOutcome, MinerHalt> {
    let views: Vec<ViewAdapter<'_>> = nodes.iter().map(|n| ViewAdapter(*n)).collect();
    let view_refs: Vec<&dyn ChainView> = views.iter().map(|v| v as &dyn ChainView).collect();
    let view: AgreedView = agree(&view_refs, &cfg.quorum)?;

    let (templates, silent) = gather(nodes, &cfg.attempt.network_id);
    if templates.is_empty() {
        return Err(MinerHalt::NoTemplates(silent));
    }
    let accepted: AcceptedTemplate =
        check_templates(&observations(&templates, &cfg.attempt.network_id), &cfg.template, view.virtual_daa)?;
    // The work identity: the facts must name OUR class. (The held key and artifact were checked against the same facts by the policy.)
    if accepted.observation.facts.class_id != cfg.class_id {
        return Err(MinerHalt::WrongClass { got: accepted.observation.facts.class_id, want: cfg.class_id });
    }
    // The block body comes from one of the agreeing nodes; its header is what the attempt is mounted on.
    let (_, chosen) = templates
        .iter()
        .find(|(n, _)| accepted.agreeing.contains(n))
        .expect("an accepted template has at least one agreeing node, and every agreeing node answered");
    let template_header: &Header = &chosen.block.header;

    let mounted: MountedAttempt = match mount_attempt(
        &accepted,
        template_header,
        &cfg.attempt,
        accepted.observation.facts.pwu,
        accepted.observation.facts.min_trace_retention_daa,
        accepted.observation.facts.artifact_root,
        cfg.class_id,
        &mut state.cursor,
        executor,
        signer,
        network_draw,
    )? {
        Some(m) => m,
        None => {
            let bucket = state.cursor.map(|(_, next)| next.saturating_sub(1)).unwrap_or(0);
            return Ok(StepOutcome::DrawLost { bucket });
        }
    };

    // One position, one attempt, one block. The same attempt again is a re-publication of the same bytes; another is refused.
    let challenge = mounted.attempt.challenge;
    if let Some((earlier, block)) = state.positions.get(&challenge) {
        if *earlier != mounted.attempt_id {
            return Err(MinerHalt::Equivocation { challenge, earlier: *earlier });
        }
        let report = publish_block(nodes, cfg, block)?;
        return Ok(StepOutcome::Republished { block_hash: block.header.hash, report });
    }

    // The inference took time: a FRESH quorum must still stand on the same chain point, inside the freshness bound.
    let fresh_view = agree(&view_refs, &cfg.quorum)?;
    let (fresh_templates, _) = gather(nodes, &cfg.attempt.network_id);
    ready_to_publish(&accepted, &observations(&fresh_templates, &cfg.attempt.network_id), &cfg.template, fresh_view.virtual_daa)?;

    let mut block = chosen.block.clone();
    block.header = std::sync::Arc::new(mounted.header.clone());
    let report = publish_block(nodes, cfg, &block)?;
    state.positions.insert(challenge, (mounted.attempt_id, block.clone()));
    Ok(StepOutcome::Published(Published {
        block_hash: block.header.hash,
        attempt_id: mounted.attempt_id,
        challenge,
        nonce_bucket: mounted.nonce_bucket,
        report,
        template_digest: accepted.digest,
        material: mounted.material,
    }))
}

/// Give `block` to every node, idempotently: the hash is the miner's own, a node answering with another never counts, "already known" is
/// success. The binary also calls this to re-send the SAME bytes of a block that went `Lost`.
pub fn publish_block(nodes: &[&dyn RemoteNode], cfg: &MinerConfig, block: &Block) -> Result<RelayReport, RelayFailure> {
    let expected = block.header.hash;
    let mut seen = std::collections::BTreeSet::new();
    let replies: Vec<(String, Reply)> = nodes
        .iter()
        .filter(|n| seen.insert(n.node_id().to_string()))
        .map(|n| (n.node_id().to_string(), n.submit_block(block)))
        .collect();
    fan_out_verdict(expected, replies, cfg.min_submit)
}

// ---------------------------------------------------------------------------------------------------------------------------
// Following a published block
// ---------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockState {
    /// Sent; no quorum node reports it yet.
    Submitted,
    /// A quorum of nodes holds it; it is not on the selected chain (yet).
    Known,
    /// On the selected chain of a quorum, shallower than the finality depth.
    OnChain { daa: u64 },
    /// On the selected chain, `finality_depth` deep.
    Settled { daa: u64 },
    /// It WAS on the selected chain and no longer is: a reorg. The block may re-enter; the claim it carried is not ours to re-make.
    ReorgedOut,
    /// No quorum node knows it after the grace period: the submission was swallowed or refused. The SAME bytes may be re-sent; another block
    /// at the same position never is.
    Lost,
}

impl BlockState {
    pub fn is_settled(&self) -> bool {
        matches!(self, BlockState::Settled { .. } | BlockState::Lost)
    }
}

/// Follows one published block through quorum observations, walking backwards on a reorg.
#[derive(Clone, Debug)]
pub struct BlockTracker {
    pub block_hash: Hash64,
    pub published_at_daa: u64,
    pub min_agree: usize,
    pub finality_depth: u64,
    pub grace_daa: u64,
    state: BlockState,
    was_on_chain: bool,
    ever_known: bool,
}

impl BlockTracker {
    pub fn new(block_hash: Hash64, published_at_daa: u64, min_agree: usize, finality_depth: u64, grace_daa: u64) -> Self {
        Self {
            block_hash,
            published_at_daa,
            min_agree: min_agree.max(1),
            finality_depth,
            grace_daa,
            state: BlockState::Submitted,
            was_on_chain: false,
            ever_known: false,
        }
    }

    pub fn state(&self) -> &BlockState {
        &self.state
    }

    /// Fold one round of per-node observations (a node that could not answer is simply absent).
    pub fn observe(&mut self, round: &[BlockObservation]) -> &BlockState {
        let know = round.iter().filter(|o| o.known).count();
        let chain = round.iter().filter(|o| o.known && o.is_chain_block).count();
        let now = round.iter().map(|o| o.virtual_daa).min().unwrap_or(self.published_at_daa);
        self.state = if chain >= self.min_agree {
            self.was_on_chain = true;
            self.ever_known = true;
            let daa = round.iter().filter(|o| o.is_chain_block).map(|o| o.daa_score).min().unwrap_or(0);
            if now.saturating_sub(daa) >= self.finality_depth { BlockState::Settled { daa } } else { BlockState::OnChain { daa } }
        } else if know >= self.min_agree {
            self.ever_known = true;
            if self.was_on_chain { BlockState::ReorgedOut } else { BlockState::Known }
        } else if self.ever_known {
            BlockState::ReorgedOut
        } else if now.saturating_sub(self.published_at_daa) > self.grace_daa {
            BlockState::Lost
        } else {
            BlockState::Submitted
        };
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use crate::attempt::{ExecutedAttempt, MlDsaAttemptSigner};
    use kaspa_consensus_core::palw_attempt_v2::PalwAttemptExecutionV1;
    use std::cell::{Cell, RefCell};

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }

    fn header(daa: u64, parent: u8) -> Header {
        Header::new_finalized(
            1,
            vec![vec![h(parent)]].try_into().unwrap(),
            h(2),
            h(3),
            h(4),
            1_700_000,
            0x1d00ffff,
            0,
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
            daa,
            0u64.into(),
            0,
            h(5),
        )
    }

    fn block(daa: u64, parent: u8) -> Block {
        Block::from_header(header(daa, parent))
    }

    /// A scripted node.
    struct Fake {
        id: String,
        daa: Cell<u64>,
        point: Cell<u8>,
        bond_pubkey: Vec<u8>,
        artifact: u8,
        class: u8,
        template_parent: Cell<u8>,
        reply: RefCell<Option<Reply>>,
        submitted: RefCell<Vec<Hash64>>,
        down: Cell<bool>,
        checkpoint: CheckpointStatus,
        observation: RefCell<Option<BlockObservation>>,
    }

    impl Fake {
        fn new(id: &str, pubkey: &[u8]) -> Self {
            Self {
                id: id.into(),
                daa: Cell::new(105),
                point: Cell::new(5),
                bond_pubkey: pubkey.to_vec(),
                artifact: 31,
                class: 30,
                template_parent: Cell::new(1),
                reply: RefCell::new(None),
                submitted: RefCell::new(vec![]),
                down: Cell::new(false),
                checkpoint: CheckpointStatus::OnChain,
                observation: RefCell::new(None),
            }
        }
    }

    impl RemoteNode for Fake {
        fn node_id(&self) -> &str {
            &self.id
        }
        fn chain_facts(&self) -> Result<NodeFacts, ViewError> {
            if self.down.get() {
                return Err(ViewError("down".into()));
            }
            Ok(NodeFacts { network_id: "testnet-12".into(), sink: h(9), virtual_daa: self.daa.get(), pruning_point: h(1) })
        }
        fn checkpoint_status(&self, _: &Checkpoint) -> Result<CheckpointStatus, ViewError> {
            Ok(self.checkpoint)
        }
        fn fetch_template(&self) -> Result<NodeTemplate, String> {
            if self.down.get() {
                return Err("down".into());
            }
            Ok(NodeTemplate {
                block: block(100, self.template_parent.get()),
                facts: ProducerFactsSummary {
                    chain_point: h(self.point.get()),
                    class_id: h(self.class),
                    artifact_root: h(self.artifact),
                    class_target: u128::MAX,
                    pwu: 7_708,
                    min_trace_retention_daa: 3_000,
                    bond_pubkey: self.bond_pubkey.clone(),
                    not_ready_reason: String::new(),
                },
            })
        }
        fn submit_block(&self, block: &Block) -> Reply {
            self.submitted.borrow_mut().push(block.header.hash);
            self.reply.borrow().clone().unwrap_or(Reply::Accepted(block.header.hash))
        }
        fn observe_block(&self, _: Hash64) -> Result<BlockObservation, String> {
            self.observation.borrow().clone().ok_or_else(|| "no observation".to_string())
        }
    }

    struct Exec {
        calls: Cell<u32>,
    }
    impl AttemptExecutor for Exec {
        fn execute(&mut self, _anchor: Hash64) -> Result<ExecutedAttempt, String> {
            self.calls.set(self.calls.get() + 1);
            Ok(ExecutedAttempt {
                execution: PalwAttemptExecutionV1 {
                    trace_root: h(10),
                    output_root: h(11),
                    execution_root: h(12),
                    trace_manifest_root: h(13),
                    trace_chunk_count: 1,
                },
                material: vec![7; 16],
            })
        }
    }

    fn keypair() -> MlDsaAttemptSigner {
        MlDsaAttemptSigner { keypair: libcrux_ml_dsa::ml_dsa_87::generate_key_pair([3u8; 32]) }
    }

    fn cfg(pubkey: Vec<u8>) -> MinerConfig {
        MinerConfig {
            quorum: QuorumPolicy::new(Checkpoint { network_id: "testnet-12".into(), daa_score: 50, block_hash: h(0xCC) }),
            template: TemplatePolicy { min_agree: 2, max_template_age_daa: 30, held_pubkey: pubkey, held_artifact_root: h(31) },
            attempt: AttemptParams {
                network_id: "testnet-12".into(),
                network_domain: h(20),
                bond: TransactionOutpoint::new(h(21), 0),
                operator_id: h(22),
                class_target: u128::MAX,
                witness_chunks: 0,
                single_lottery: true,
                signature_len: 4627,
            },
            class_id: h(30),
            min_submit: 2,
            grace_daa: 60,
            finality_depth: 60,
        }
    }

    fn run(
        nodes: &[&Fake],
        cfg: &MinerConfig,
        state: &mut MinerState,
        exec: &mut Exec,
        signer: &MlDsaAttemptSigner,
    ) -> Result<StepOutcome, MinerHalt> {
        let refs: Vec<&dyn RemoteNode> = nodes.iter().map(|n| *n as &dyn RemoteNode).collect();
        step(&refs, cfg, state, exec, signer, &|_, _, _| true)
    }

    fn pk(s: &MlDsaAttemptSigner) -> Vec<u8> {
        s.public_key()
    }

    #[test]
    fn a_won_draw_is_published_to_every_node_after_a_fresh_recheck_and_signed_once() {
        let signer = keypair();
        let (a, b) = (Fake::new("a", &pk(&signer)), Fake::new("b", &pk(&signer)));
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        let out = run(&[&a, &b], &cfg(pk(&signer)), &mut state, &mut exec, &signer).unwrap();
        let StepOutcome::Published(p) = out else { panic!("{out:?}") };
        assert_eq!(p.report.successes, 2);
        assert_eq!(a.submitted.borrow().as_slice(), &[p.block_hash]);
        assert_eq!(b.submitted.borrow().as_slice(), &[p.block_hash]);
        assert_eq!(exec.calls.get(), 1);
        // The block that went out carries the signed attempt for THIS position and hashes to what was announced.
        let (_, sent) = state.published().next().unwrap();
        assert_eq!(sent.header.hash, p.block_hash);
        let env = kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&sent.header.palw_commitment).unwrap();
        assert_eq!(kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&env.attempt), p.attempt_id);
    }

    #[test]
    fn a_stale_template_is_refused_before_any_inference() {
        let signer = keypair();
        let (a, b) = (Fake::new("a", &pk(&signer)), Fake::new("b", &pk(&signer)));
        a.daa.set(200);
        b.daa.set(200); // the template (DAA 100) is 100 behind the quorum's clock
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        let r = run(&[&a, &b], &cfg(pk(&signer)), &mut state, &mut exec, &signer);
        assert!(matches!(r, Err(MinerHalt::Template(TemplateRefusal::Stale { .. }))), "{r:?}");
        assert_eq!(exec.calls.get(), 0, "no inference was spent on a stale template");
        assert!(a.submitted.borrow().is_empty());
    }

    #[test]
    fn a_template_that_ages_during_the_inference_is_dropped_not_published() {
        // The nodes agree at the start; by the re-check the chain has moved on (a different chain point).
        struct Moving<'a>(&'a Fake, &'a Cell<u32>);
        impl RemoteNode for Moving<'_> {
            fn node_id(&self) -> &str {
                self.0.node_id()
            }
            fn chain_facts(&self) -> Result<NodeFacts, ViewError> {
                self.0.chain_facts()
            }
            fn checkpoint_status(&self, c: &Checkpoint) -> Result<CheckpointStatus, ViewError> {
                self.0.checkpoint_status(c)
            }
            fn fetch_template(&self) -> Result<NodeTemplate, String> {
                // The second fetch (the re-check) sees a new chain point.
                self.1.set(self.1.get() + 1);
                if self.1.get() > 2 {
                    self.0.point.set(6);
                }
                self.0.fetch_template()
            }
            fn submit_block(&self, b: &Block) -> Reply {
                self.0.submit_block(b)
            }
            fn observe_block(&self, hh: Hash64) -> Result<BlockObservation, String> {
                self.0.observe_block(hh)
            }
        }
        let signer = keypair();
        let (a, b) = (Fake::new("a", &pk(&signer)), Fake::new("b", &pk(&signer)));
        let counter = Cell::new(0);
        let (ma, mb) = (Moving(&a, &counter), Moving(&b, &counter));
        let refs: Vec<&dyn RemoteNode> = vec![&ma, &mb];
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        let r = step(&refs, &cfg(pk(&signer)), &mut state, &mut exec, &signer, &|_, _, _| true);
        assert!(matches!(r, Err(MinerHalt::Attempt(AttemptError::Template(TemplateRefusal::ChainMoved)))), "{r:?}");
        assert!(a.submitted.borrow().is_empty() && b.submitted.borrow().is_empty(), "nothing leaves on a stale view");
        assert_eq!(state.published().count(), 0);
    }

    #[test]
    fn a_wrong_bond_key_a_foreign_class_and_a_disagreeing_node_each_stop_the_miner() {
        let signer = keypair();
        // The chain's registered key for the bond is not the key this miner holds.
        let (a, b) = (Fake::new("a", &[1, 2, 3]), Fake::new("b", &[1, 2, 3]));
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        assert!(matches!(
            run(&[&a, &b], &cfg(pk(&signer)), &mut state, &mut exec, &signer),
            Err(MinerHalt::Template(TemplateRefusal::BondKeyMismatch))
        ));
        // Facts name another class than the one configured.
        let (a, b) = (Fake::new("a", &pk(&signer)), Fake::new("b", &pk(&signer)));
        let mut c = cfg(pk(&signer));
        c.class_id = h(77);
        assert!(matches!(run(&[&a, &b], &c, &mut state, &mut exec, &signer), Err(MinerHalt::WrongClass { .. })));
        // One node on another chain point (a lying or lagging node): unanimity, not majority.
        let (a, b, c3) = (Fake::new("a", &pk(&signer)), Fake::new("b", &pk(&signer)), Fake::new("c", &pk(&signer)));
        c3.point.set(6);
        assert!(matches!(
            run(&[&a, &b, &c3], &cfg(pk(&signer)), &mut state, &mut exec, &signer),
            Err(MinerHalt::Template(TemplateRefusal::Disagreement { .. }))
        ));
        assert_eq!(exec.calls.get(), 0);
        // One node alone is not a quorum.
        assert!(matches!(run(&[&a], &cfg(pk(&signer)), &mut state, &mut exec, &signer), Err(MinerHalt::View(_))));
        // A node contradicting the pinned checkpoint halts everything.
        let mut evil = Fake::new("evil", &pk(&signer));
        evil.checkpoint = CheckpointStatus::Conflicts;
        assert!(matches!(
            run(&[&a, &b, &evil], &cfg(pk(&signer)), &mut state, &mut exec, &signer),
            Err(MinerHalt::View(ViewHalt::CheckpointConflict { .. }))
        ));
    }

    #[test]
    fn publishing_the_same_block_twice_is_idempotent_and_a_second_attempt_at_a_position_is_refused() {
        let signer = keypair();
        let (a, b) = (Fake::new("a", &pk(&signer)), Fake::new("b", &pk(&signer)));
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        let c = cfg(pk(&signer));
        let StepOutcome::Published(first) = run(&[&a, &b], &c, &mut state, &mut exec, &signer).unwrap() else { panic!() };
        // The same template again resumes at the next bucket: a DIFFERENT position (a different nonce → challenge), a different block.
        let StepOutcome::Published(second) = run(&[&a, &b], &c, &mut state, &mut exec, &signer).unwrap() else { panic!() };
        assert_ne!(first.challenge, second.challenge);
        assert_eq!(state.published().count(), 2);
        // A restart that forgot the bucket cursor re-makes bucket 0: the same position, the same attempt → the SAME block bytes are re-sent
        // (nodes already holding it say so, which is success), never a second block.
        state.cursor = None;
        *a.reply.borrow_mut() = Some(Reply::AlreadyKnown(first.block_hash));
        *b.reply.borrow_mut() = Some(Reply::AlreadyKnown(first.block_hash));
        match run(&[&a, &b], &c, &mut state, &mut exec, &signer).unwrap() {
            StepOutcome::Republished { block_hash, report } => {
                assert_eq!(block_hash, first.block_hash);
                assert!(report.per_node.iter().all(|(_, o)| *o == crate::relay::NodeOutcome::AlreadyKnown));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(state.published().count(), 2, "no third block");
        // An executor that returns DIFFERENT roots for the same position would make a second attempt there: refused, nothing sent.
        struct Drifting(u8);
        impl AttemptExecutor for Drifting {
            fn execute(&mut self, _: Hash64) -> Result<ExecutedAttempt, String> {
                self.0 += 1;
                Ok(ExecutedAttempt {
                    execution: PalwAttemptExecutionV1 {
                        trace_root: h(100 + self.0),
                        output_root: h(11),
                        execution_root: h(12),
                        trace_manifest_root: h(13),
                        trace_chunk_count: 1,
                    },
                    material: vec![],
                })
            }
        }
        state.cursor = None;
        let sent_before = a.submitted.borrow().len();
        let refs: Vec<&dyn RemoteNode> = vec![&a, &b];
        let r = step(&refs, &c, &mut state, &mut Drifting(0), &signer, &|_, _, _| true);
        assert!(matches!(r, Err(MinerHalt::Equivocation { .. })), "{r:?}");
        assert_eq!(a.submitted.borrow().len(), sent_before, "nothing was sent for the second attempt");
    }

    #[test]
    fn a_node_that_answers_with_another_hash_never_counts_as_having_taken_the_block() {
        let signer = keypair();
        let (a, b) = (Fake::new("a", &pk(&signer)), Fake::new("liar", &pk(&signer)));
        *b.reply.borrow_mut() = Some(Reply::Accepted(h(0x66)));
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        let r = run(&[&a, &b], &cfg(pk(&signer)), &mut state, &mut exec, &signer);
        assert!(matches!(r, Err(MinerHalt::Publish(RelayFailure::TooFew { got: 1, need: 2, .. }))), "{r:?}");
        // The block was NOT recorded as published: a retry re-makes the same bytes.
        assert_eq!(state.published().count(), 0);
    }

    #[test]
    fn a_published_block_is_followed_to_settled_and_a_reorg_walks_it_backwards() {
        let obs = |known: bool, chain: bool, daa: u64, virt: u64| BlockObservation {
            sink: h(1),
            virtual_daa: virt,
            known,
            is_chain_block: chain,
            daa_score: daa,
        };
        let mut t = BlockTracker::new(h(9), 100, 2, 60, 60);
        assert_eq!(t.observe(&[obs(false, false, 0, 105), obs(false, false, 0, 105)]), &BlockState::Submitted);
        assert_eq!(t.observe(&[obs(true, false, 101, 110), obs(true, false, 101, 110)]), &BlockState::Known);
        assert_eq!(t.observe(&[obs(true, true, 101, 120), obs(true, true, 101, 120)]), &BlockState::OnChain { daa: 101 });
        // One node alone claiming a chain block is not believed.
        assert_eq!(t.observe(&[obs(true, true, 101, 125), obs(true, false, 101, 125)]), &BlockState::ReorgedOut);
        assert_eq!(t.observe(&[obs(true, true, 101, 130), obs(true, true, 101, 130)]), &BlockState::OnChain { daa: 101 });
        assert_eq!(t.observe(&[obs(true, true, 101, 170), obs(true, true, 101, 170)]), &BlockState::Settled { daa: 101 });
        // A reorg that removes it entirely walks back.
        let mut t = BlockTracker::new(h(9), 100, 2, 60, 60);
        t.observe(&[obs(true, true, 101, 120), obs(true, true, 101, 120)]);
        assert_eq!(t.observe(&[obs(false, false, 0, 130), obs(false, false, 0, 130)]), &BlockState::ReorgedOut);
        // Never seen by anyone past the grace period: lost (the same bytes may be re-sent; another block never).
        let mut t = BlockTracker::new(h(9), 100, 2, 60, 60);
        assert_eq!(t.observe(&[obs(false, false, 0, 150), obs(false, false, 0, 150)]), &BlockState::Submitted);
        assert_eq!(t.observe(&[obs(false, false, 0, 170), obs(false, false, 0, 170)]), &BlockState::Lost);
        assert!(t.state().is_settled());
    }
}
