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
use crate::l2::{
    ForkChoiceAttestationV1, ForkChoiceEvidenceV1, ForkChoiceRulesV1, L2InputV1, L2LimitsV1, L2StopV1, L2VerdictV1, PeerViewV1,
    candidate_chains_v1, l3_root_under_l2_v1, openings_wanted_v1, verify_fork_choice_v1,
};
use crate::relay::{RelayFailure, RelayReport, Reply, fan_out_verdict};
use crate::template::{
    AcceptedTemplate, ProducerFactsSummary, TemplatePolicy, TemplateRefusal, check_templates, template_observation_v1,
};
use crate::verify::{
    ClientRulesetV1, GateRefusalV1, L2StatusV1, ModeLabelV1, NodeJobFactsV1, NodeRulesetV1, TrustedCheckpointV1, VerifiedChainV1,
    VerifyErrorV1, VerifyLimitsV1, bond_on_chain_under_v1, bond_on_chain_v1, check_job_facts_v1, check_ruleset_v1, check_template_v1,
    class_on_chain_under_v1, class_on_chain_v1, l2_status_v1, l3_root_at_header_v1, merge_views_v1, mode_label_v1, signing_gate_v1,
    verify_header_chain_v1,
};
use crate::view::{AgreedView, ChainView, CheckpointStatus, Halt as ViewHalt, NodeFacts, QuorumPolicy, ViewError, agree};
use kaspa_consensus_core::palw_fork_choice_commitment_v1::{PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1, PalwForkChoiceOpeningV1};
use kaspa_consensus_core::palw_state_proof_v1::PalwFactProofV1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;

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
    /// **L1's input**: the headers from `from` (the client's checkpoint) to this node's sink along its selected chain, `from` first. Never
    /// trusted: [`crate::verify::verify_header_chain_v1`] judges them. The default serves none (the client then cannot leave
    /// `UNVERIFIED_REMOTE`).
    fn header_chain(&self, from: Hash64) -> Result<Vec<Header>, String> {
        let _ = from;
        Err("this node serves no header chain".into())
    }
    /// **L3's input** (op 202): the header of `block` and one collection of the state it commits.
    fn state_proof(&self, block: Hash64, collection: &str) -> Result<(Header, PalwFactProofV1), String> {
        let _ = (block, collection);
        Err("this node serves no state proof".into())
    }
    /// **RFC-0009 L2's input** (op 203): the fork-choice openings of `blocks`' post-states (at most
    /// [`PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1`]). Never trusted: `crate::l2` uses an opening only where it hashes to a root it
    /// established. The default serves none.
    fn fork_choice_openings(&self, blocks: &[Hash64]) -> Result<Vec<PalwForkChoiceOpeningV1>, String> {
        let _ = blocks;
        Err("this node serves no fork-choice opening".into())
    }
    /// The ruleset the node says it runs (network, genesis, params id, schedule id).
    fn ruleset(&self) -> Result<NodeRulesetV1, String> {
        Err("this node does not say which ruleset it runs".into())
    }
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
    /// The security class this miner may sign in, and how it verifies (RFC-0009 operating modes, 2026-10-08).
    pub trust: MinerTrustV1,
}

/// **How a remote miner establishes its view, and which class it accepts.** `verification: None` is the quorum alone
/// (`UNVERIFIED_REMOTE`); `Some` runs L1 (every node's header chain from the trusted checkpoint, merged by containment), L3 (the bond and
/// the class proven at the verified tip) and L2 (established only at a trusted checkpoint that is the decision point, or — with
/// `fork_choice` — by an attested fork choice, RFC-0009 L2) — see [`crate::verify`] and [`crate::l2`]. Below `VERIFIED_REMOTE` nothing is
/// executed or signed unless `accept_unverified` names the class.
#[derive(Clone, Debug, Default)]
pub struct MinerTrustV1 {
    pub own_full_node: bool,
    pub verification: Option<RemoteVerificationV1>,
    pub accept_unverified: Option<ModeLabelV1>,
    /// The miner's own pay script. When set, a template whose coinbase pays anyone else — a pool offering custody of the reward — is
    /// refused (RFC-0009 mode C: a pool is a job service, relay, provider or builder; non-custodial by default).
    pub pay_to: Option<kaspa_consensus_core::tx::ScriptPublicKey>,
}

#[derive(Clone, Debug)]
pub struct RemoteVerificationV1 {
    pub ruleset: ClientRulesetV1,
    pub checkpoint: TrustedCheckpointV1,
    pub limits: VerifyLimitsV1,
    /// The miner's bond (its key is `template.held_pubkey`).
    pub bond: PalwBondKeyV2,
    /// The client's clock, in ms.
    pub now_ms: fn() -> u64,
    /// RFC-0009 L2 by attestation. `None`: L2 is C1r2's checkpoint rule.
    pub fork_choice: Option<RemoteForkChoiceV1>,
}

/// **RFC-0009 L2 by attestation** ([`crate::l2`]): the issuers the user chose before talking to any node, the channel their fresh
/// attestations arrive by (an issuer service, or the user's own node over an authenticated channel — the 1-of-1 case), and the rules L2
/// runs under. Configured, it decides L2 instead of the checkpoint rule: the views are weighed by the node's own decision functions on
/// opened values, a conflict the comparator cannot settle is a STOP, and L3 is proven under the attested root (`l3_root_under_l2_v1`).
/// The issuer's trust is printed with the mode (`MinerState::last_l2`).
#[derive(Clone)]
pub struct RemoteForkChoiceV1 {
    pub rules: ForkChoiceRulesV1,
    pub limits: L2LimitsV1,
    /// `(key id, public key)` per issuer.
    pub issuers: Vec<(Vec<u8>, Vec<u8>)>,
    /// Fresh attestations of these blocks (the views' tips), from the issuers' channel. An error is no attestation: L2 is then not verified.
    pub attestations: std::sync::Arc<dyn Fn(&[Hash64]) -> Result<Vec<ForkChoiceAttestationV1>, String> + Send + Sync>,
    /// The signature primitive (ML-DSA-87 in the binaries): `(public key, message, signature)`.
    pub verify_signature: fn(&[u8], &[u8], &[u8]) -> bool,
}

impl std::fmt::Debug for RemoteForkChoiceV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteForkChoiceV1")
            .field("rules", &self.rules)
            .field("limits", &self.limits)
            .field("issuers", &self.issuers.iter().map(|(id, _)| String::from_utf8_lossy(id).into_owned()).collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
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
    #[error("verification refused the view, before any inference or signature: {0}")]
    Verify(#[from] VerifyErrorV1),
    #[error("{0}")]
    Gate(#[from] GateRefusalV1),
    #[error("RFC-0009 L2 STOP, before any inference or signature: {0}")]
    ForkChoice(#[from] L2StopV1),
    #[error(
        "the template pays its block reward to {got}, not to this miner ({want}): a custodial job — refused (non-custodial by default)"
    )]
    Custodial { got: String, want: String },
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
    /// The security class the last step ran in — every output prints it.
    pub last_mode: Option<ModeLabelV1>,
    /// With an attested fork choice: the L2 line printed beside the mode — the issuer whose attestation the verdict rests on, or why L2
    /// is not verified.
    pub last_l2: Option<String>,
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

    // **Non-custodial by default**: the template's own reward must be paid to this miner, whoever served the job.
    if let Some(want) = &cfg.trust.pay_to {
        let got = crate::template::template_pays_v1(&chosen.block);
        if got.as_ref() != Some(want) {
            return Err(MinerHalt::Custodial { got: format!("{got:?}"), want: format!("{want:?}") });
        }
    }
    // **The security class, before any inference** (L1/L3/L2 when configured; the quorum alone is UNVERIFIED_REMOTE), and the gate.
    let job = NodeJobFactsV1 {
        class_id: accepted.observation.facts.class_id,
        artifact_root: accepted.observation.facts.artifact_root,
        bond: cfg.trust.verification.as_ref().map(|v| v.bond).unwrap_or(PalwBondKeyV2(cfg.attempt.bond)),
        bond_pubkey: accepted.observation.facts.bond_pubkey.clone(),
        bond_may_produce: true,
    };
    let mode = remote_mode_v1(nodes, &cfg.trust, cfg.attempt.network_domain, template_header, &job)?;
    let label = mode.label;
    state.last_mode = Some(label);
    state.last_l2 = mode.l2_line;
    signing_gate_v1(label, cfg.trust.accept_unverified)?;
    // …and again RIGHT BEFORE the signature: the inference took time, so the view is re-verified and the template's parent must still be
    // the verified tip.
    let recheck = || -> Result<(), String> {
        let again = remote_mode_v1(nodes, &cfg.trust, cfg.attempt.network_domain, template_header, &job).map_err(|e| e.to_string())?;
        signing_gate_v1(again.label, cfg.trust.accept_unverified).map_err(|e| e.to_string())
    };
    let gated = GatedSigner { inner: signer, recheck: &recheck };

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
        &gated,
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

/// A signer that re-verifies the view right before it signs (the second half of the gate).
struct GatedSigner<'a> {
    inner: &'a dyn AttemptSigner,
    recheck: &'a dyn Fn() -> Result<(), String>,
}

impl AttemptSigner for GatedSigner<'_> {
    fn public_key(&self) -> Vec<u8> {
        self.inner.public_key()
    }
    fn sign_attempt_id(&self, attempt_id: &Hash64) -> Result<Vec<u8>, String> {
        (self.recheck)().map_err(|why| format!("not signed — the view did not survive the re-check: {why}"))?;
        self.inner.sign_attempt_id(attempt_id)
    }
    fn sign_attempt(
        &self,
        attempt: &kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2,
        attempt_id: &Hash64,
    ) -> Result<Vec<u8>, String> {
        (self.recheck)().map_err(|why| format!("not signed — the view did not survive the re-check: {why}"))?;
        self.inner.sign_attempt(attempt, attempt_id)
    }
}

/// What [`remote_mode_v1`] established: the class, and (with an attested fork choice) the L2 line printed beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteModeV1 {
    pub label: ModeLabelV1,
    pub l2_line: Option<String>,
}

/// **The class a step runs in.** Without `verification`: the quorum alone, `UNVERIFIED_REMOTE` (or `FULL_NODE` when the one node is the
/// user's own). With it: every node's ruleset must be ours; every node's header chain from the checkpoint must pass L1, and the views must
/// not conflict (a conflict STOPS — blue work never decides) unless an attested fork choice weighs them ([`RemoteForkChoiceV1`]); the
/// template must stand on the verified (or chosen) tip at a sane target; the bond and the class must be proven at the tip and agree with
/// the node's facts. Any failure is a STOP, not a downgrade: a node that lies about something it had to prove is not a node to mine through.
pub fn remote_mode_v1(
    nodes: &[&dyn RemoteNode],
    trust: &MinerTrustV1,
    network_domain: Hash64,
    template: &Header,
    job: &NodeJobFactsV1,
) -> Result<RemoteModeV1, MinerHalt> {
    let Some(v) = &trust.verification else {
        let label = mode_label_v1(trust.own_full_node, false, false, &L2StatusV1::Unverified("the quorum alone proves nothing"));
        return Ok(RemoteModeV1 { label, l2_line: None });
    };
    let now = (v.now_ms)();
    let mut views: Vec<VerifiedChainV1> = Vec::new();
    let mut peers: Vec<PeerViewV1> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for node in nodes.iter().filter(|n| seen.insert(n.node_id().to_string())) {
        let theirs = node.ruleset().map_err(|why| VerifyErrorV1::WrongRuleset {
            what: "ruleset (not reported)",
            ours: v.ruleset.consensus_params_id.clone(),
            theirs: why,
        })?;
        check_ruleset_v1(&v.ruleset, &theirs)?;
        let headers =
            node.header_chain(v.checkpoint.block).map_err(|why| VerifyErrorV1::Proof(format!("{}: {why}", node.node_id())))?;
        let chain = verify_header_chain_v1(&v.checkpoint, &headers, network_domain, now, &v.limits)?;
        peers.push(PeerViewV1 { peer: node.node_id().to_string(), chain: chain.clone() });
        views.push(chain);
    }
    if let Some(fc) = &v.fork_choice {
        return attested_mode_v1(nodes, trust, fc, &peers, &views, template, job);
    }
    let chain = merge_views_v1(&views)?;
    check_template_v1(template, &chain)?;
    let tip = chain.tip_hash();
    let node = nodes.first().ok_or(VerifyErrorV1::Empty)?;
    let (bh, bonds) = node.state_proof(tip, "bonds").map_err(VerifyErrorV1::Proof)?;
    let (ch, classes) = node.state_proof(tip, "classes").map_err(VerifyErrorV1::Proof)?;
    let bond = bond_on_chain_v1(&chain, &bh, &bonds, &job.bond)?;
    let class = class_on_chain_v1(&chain, &ch, &classes, &job.class_id)?;
    check_job_facts_v1(job, &class, &bond)?;
    let l2 = l2_status_v1(&v.checkpoint, &chain, Some(bh.hash), &v.limits);
    Ok(RemoteModeV1 { label: mode_label_v1(trust.own_full_node, true, true, &l2), l2_line: None })
}

/// **[`remote_mode_v1`] with an attested fork choice** (RFC-0009 L2, [`crate::l2`]).
///
/// The openings `crate::l2` needs are asked of every node (op 203; untrusted, each checked against a root before use), the attestations of
/// the views' tips of the issuers' channel. A STOP is a halt. Established: the template must stand on the CHOSEN tip, and the bond and the
/// class are proven under the attested root (`l3_root_under_l2_v1`) — `VERIFIED_REMOTE` with L1, L2 and L3, the issuer named on the L2
/// line. Not established: the views must still not conflict (a conflict nobody can weigh is a STOP), L3 is proven at the verified tip's
/// header (`l3_root_at_header_v1`), and the class is at most `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED`, the reason on the L2 line.
fn attested_mode_v1(
    nodes: &[&dyn RemoteNode],
    trust: &MinerTrustV1,
    fc: &RemoteForkChoiceV1,
    peers: &[PeerViewV1],
    views: &[VerifiedChainV1],
    template: &Header,
    job: &NodeJobFactsV1,
) -> Result<RemoteModeV1, MinerHalt> {
    let v = trust.verification.as_ref().expect("called with a verification config");
    let candidates = candidate_chains_v1(peers)?;
    let mut openings: Vec<PalwForkChoiceOpeningV1> = Vec::new();
    let wanted = openings_wanted_v1(&candidates);
    for node in nodes {
        for chunk in wanted.chunks(PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1) {
            // A node that serves nothing is no evidence of anything: the openings another node served may still verify.
            if let Ok(served) = node.fork_choice_openings(chunk) {
                openings.extend(served);
            }
        }
    }
    let tips: Vec<Hash64> = candidates.iter().map(|c| c.tip_hash()).collect();
    let attestations = (fc.attestations)(&tips).unwrap_or_default();
    // Each attestation with an opening of its block: the one that hashes to the attested root if any node served it, else any (refused
    // by name inside the verdict).
    let evidence: Vec<ForkChoiceEvidenceV1> = attestations
        .into_iter()
        .filter_map(|attestation| {
            let mut of_block = openings.iter().filter(|o| o.leaf.block == attestation.block);
            let first = of_block.clone().next().copied();
            let opening = of_block.find(|o| o.committed_root() == attestation.committed_root).copied().or(first)?;
            Some(ForkChoiceEvidenceV1 { attestation, opening })
        })
        .collect();
    let input = L2InputV1 {
        views: peers,
        evidence: &evidence,
        chain_openings: &openings,
        trusted: &fc.issuers,
        ruleset: &v.ruleset,
        rules: &fc.rules,
        limits: &fc.limits,
    };
    let verdict = verify_fork_choice_v1(&input, &fc.verify_signature)?;
    let node = nodes.first().ok_or(VerifyErrorV1::Empty)?;
    let (bond, class) = match &verdict {
        L2VerdictV1::Established { chosen, .. } => {
            check_template_v1(template, chosen)?;
            let tip = chosen.tip_hash();
            let (bh, bonds) = node.state_proof(tip, "bonds").map_err(VerifyErrorV1::Proof)?;
            let (ch, classes) = node.state_proof(tip, "classes").map_err(VerifyErrorV1::Proof)?;
            let under = |h: &Header| l3_root_under_l2_v1(&verdict, h, &openings, &fc.rules).map_err(VerifyErrorV1::Proof);
            let bond = bond_on_chain_under_v1(chosen, &bh, under(&bh)?, &bonds, &job.bond)?;
            let class = class_on_chain_under_v1(chosen, &ch, under(&ch)?, &classes, &job.class_id)?;
            (bond, class)
        }
        L2VerdictV1::Unverified(_) => {
            let chain = merge_views_v1(views)?;
            check_template_v1(template, &chain)?;
            let tip = chain.tip_hash();
            let (bh, bonds) = node.state_proof(tip, "bonds").map_err(VerifyErrorV1::Proof)?;
            let (ch, classes) = node.state_proof(tip, "classes").map_err(VerifyErrorV1::Proof)?;
            let at = |h: &Header| l3_root_at_header_v1(&chain, h, &openings, fc.rules.commitment);
            let bond = bond_on_chain_under_v1(&chain, &bh, at(&bh)?, &bonds, &job.bond)?;
            let class = class_on_chain_under_v1(&chain, &ch, at(&ch)?, &classes, &job.class_id)?;
            (bond, class)
        }
    };
    check_job_facts_v1(job, &class, &bond)?;
    Ok(RemoteModeV1 { label: mode_label_v1(trust.own_full_node, true, true, &verdict.status()), l2_line: Some(verdict.trust_line()) })
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
    use crate::attempt::{ExecutedAttempt, MlDsaAttemptSigner};
    use kaspa_consensus_core::palw_attempt_v2::PalwAttemptExecutionV1;
    use kaspa_consensus_core::tx::TransactionOutpoint;
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
            // These fakes serve no headers or proofs: the quorum alone, explicitly accepted as UNVERIFIED_REMOTE.
            trust: MinerTrustV1 { accept_unverified: Some(ModeLabelV1::UnverifiedRemote), ..Default::default() },
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

    // ---- RFC-0009 operating modes: the class a step runs in, and the gate (2026-10-08) ------------------------------------------------

    /// **No opt-in, no work**: the quorum alone is UNVERIFIED_REMOTE, and without `--accept-unverified-state` the miner neither executes nor
    /// signs — and says which class it is in.
    #[test]
    fn an_unverified_quorum_neither_executes_nor_signs_without_the_opt_in() {
        let signer = keypair();
        let (a, b) = (Fake::new("a", &pk(&signer)), Fake::new("b", &pk(&signer)));
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        let mut strict = cfg(pk(&signer));
        strict.trust.accept_unverified = None;
        let out = run(&[&a, &b], &strict, &mut state, &mut exec, &signer);
        assert!(matches!(&out, Err(MinerHalt::Gate(g)) if g.label == ModeLabelV1::UnverifiedRemote), "{out:?}");
        assert!(out.unwrap_err().to_string().contains("--accept-unverified-state UNVERIFIED_REMOTE"));
        assert_eq!(exec.calls.get(), 0, "no inference");
        assert!(a.submitted.borrow().is_empty() && b.submitted.borrow().is_empty(), "nothing signed or sent");
        assert_eq!(state.last_mode, Some(ModeLabelV1::UnverifiedRemote));
        // Accepting the header-verified class does not accept the unverified one.
        strict.trust.accept_unverified = Some(ModeLabelV1::HeaderVerifiedForkChoiceUnverified);
        assert!(matches!(run(&[&a, &b], &strict, &mut state, &mut exec, &signer), Err(MinerHalt::Gate(_))));
        assert_eq!(exec.calls.get(), 0);
    }

    const VNOW: u64 = 1_800_000_000_000;
    fn vnow() -> u64 {
        VNOW
    }

    /// A node that serves a header chain from the checkpoint, the state's proofs at its tip, and its ruleset; its template stands on its tip.
    struct VFake {
        inner: Fake,
        chain: RefCell<Vec<Header>>,
        /// Served from the second `header_chain` call on (a reorg between the gate and the signature).
        later: RefCell<Option<Vec<Header>>>,
        calls: Cell<u32>,
        state: kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        ruleset: crate::verify::NodeRulesetV1,
        /// RFC-0009 L2: the fork-choice openings this node serves (op 203); empty for a chain below the commitment fence.
        openings: Vec<PalwForkChoiceOpeningV1>,
    }

    fn vheader(parent: Hash64, daa: u64, ts: u64, root: Hash64, salt: u64) -> Header {
        let mut x = Header::new_finalized(
            1,
            vec![vec![parent]].try_into().unwrap(),
            h(2),
            h(3),
            h(4),
            ts,
            0x1d00ffff,
            salt,
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1,
            daa,
            daa.into(),
            daa,
            h(5),
        )
        .with_palw_state_root(root);
        x.finalize();
        x
    }

    fn vchain(root: Hash64, salt: u64, n: u64) -> Vec<Header> {
        let mut out = vec![vheader(h(0xC0), 96, VNOW - 10 * 120_000, root, 0)];
        for i in 0..n {
            let p = out.last().unwrap().clone();
            out.push(vheader(p.hash, p.daa_score + 1, p.timestamp + 120_000, root, salt * 100 + i));
        }
        out
    }

    /// The state the chain commits: class h(30) (the base class) over `root`, and the miner's bond (h(21):0) holding `pubkey`.
    fn vstate(pubkey: &[u8], root: Hash64) -> kaspa_consensus_core::palw_state_v2::PalwChainStateV2 {
        use kaspa_consensus_core::palw_state_v2::{
            PalwBlockContextV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwPwuRuleV2, PalwStateParamsV2,
            apply_palw_transition_v2,
        };
        let params = PalwStateParamsV2::new(100, 1, 1, 1, 500, 1_000, h(30), 4, 1_000, 100, 1_000, 0).unwrap();
        let objects = vec![
            Obj::ClassRegistered {
                class_id: h(30),
                artifact_root: root,
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            Obj::BondRegistered {
                bond: PalwBondKeyV2(TransactionOutpoint::new(h(21), 0)),
                pubkey: pubkey.to_vec(),
                operator_pubkey: vec![8; 8],
                collateral: 1 << 40,
                payout_payload: h(0x9A),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ];
        let cx = PalwBlockContextV2 { block: h(10), daa_score: 1, blue_score: 1, subsidy: 0 };
        apply_palw_transition_v2(&PalwChainStateV2::genesis(), &params, &cx, &objects, None).unwrap().0
    }

    fn ours() -> crate::verify::ClientRulesetV1 {
        crate::verify::ClientRulesetV1 {
            network_id: "testnet-12".into(),
            genesis: "g".into(),
            consensus_params_id: "p".into(),
            consensus_schedule_id: "s".into(),
        }
    }

    impl VFake {
        fn new(id: &str, pubkey: &[u8], proven_root: Hash64, salt: u64) -> Self {
            let state = vstate(pubkey, proven_root);
            let chain = vchain(state.state_root(), salt, 3);
            Self {
                inner: Fake::new(id, pubkey),
                chain: RefCell::new(chain),
                later: RefCell::new(None),
                calls: Cell::new(0),
                state,
                ruleset: crate::verify::NodeRulesetV1 {
                    network_id: "testnet-12".into(),
                    genesis: Some("g".into()),
                    consensus_params_id: "p".into(),
                    consensus_schedule_id: "s".into(),
                },
                openings: Vec::new(),
            }
        }
        /// RFC-0009 L2, past the commitment fence: every header commits its predecessor's post-state as `H(leaf ‖ root)` (the leaf naming
        /// the predecessor, the ADR-0043 root the state's), and the node serves every block's opening, the tip's included.
        fn enveloped(id: &str, pubkey: &[u8], proven_root: Hash64, salt: u64) -> Self {
            use kaspa_consensus_core::palw_fork_choice_commitment_v1::PALW_FORK_CHOICE_LEAF_VERSION_V1 as V;
            let mut node = Self::new(id, pubkey, proven_root, salt);
            let inner = node.state.state_root();
            let opening_of = |x: &Header| PalwForkChoiceOpeningV1 {
                leaf: kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoiceLeafV1 {
                    leaf_version: V,
                    block: x.hash,
                    daa_score: x.daa_score,
                    blue_score: x.blue_score,
                    safe_frontier_blue_score: 0,
                    safe_frontier: Hash64::default(),
                    safe_weight: 0,
                    bounded_immature: 0,
                    bonds_len: 1,
                },
                inner_root: inner,
            };
            let mut chain = vec![vheader(h(0xC0), 96, VNOW - 10 * 120_000, inner, 0)];
            for i in 0..3 {
                let p = chain.last().unwrap().clone();
                let root = opening_of(&p).committed_root();
                chain.push(vheader(p.hash, p.daa_score + 1, p.timestamp + 120_000, root, salt * 100 + i));
            }
            node.openings = chain.iter().map(opening_of).collect();
            *node.chain.borrow_mut() = chain;
            node
        }
        fn tip(&self) -> Header {
            self.chain.borrow().last().unwrap().clone()
        }
    }

    impl RemoteNode for VFake {
        fn node_id(&self) -> &str {
            self.inner.node_id()
        }
        fn chain_facts(&self) -> Result<NodeFacts, ViewError> {
            self.inner.chain_facts()
        }
        fn checkpoint_status(&self, c: &Checkpoint) -> Result<CheckpointStatus, ViewError> {
            self.inner.checkpoint_status(c)
        }
        fn fetch_template(&self) -> Result<NodeTemplate, String> {
            let mut t = self.inner.fetch_template()?;
            let tip = self.tip();
            let mut x = header(tip.daa_score + 1, 0);
            x.parents_by_level = vec![vec![tip.hash]].try_into().unwrap();
            x.finalize();
            // A coinbase paying the miner's script (what getBlockTemplate with the miner's pay address returns).
            let mut payload = vec![0u8; 16];
            payload.extend_from_slice(&0u16.to_le_bytes());
            payload.push(34);
            payload.extend_from_slice(&[0x5A; 34]);
            let coinbase = kaspa_consensus_core::tx::Transaction::new(
                0,
                vec![],
                vec![],
                0,
                kaspa_consensus_core::subnets::SUBNETWORK_ID_COINBASE,
                0,
                payload,
            );
            t.block = Block::new(x, vec![coinbase]);
            Ok(t)
        }
        fn submit_block(&self, block: &Block) -> Reply {
            self.inner.submit_block(block)
        }
        fn observe_block(&self, hash: Hash64) -> Result<BlockObservation, String> {
            self.inner.observe_block(hash)
        }
        fn header_chain(&self, from: Hash64) -> Result<Vec<Header>, String> {
            self.calls.set(self.calls.get() + 1);
            if self.calls.get() > 1
                && let Some(later) = self.later.borrow().clone()
            {
                *self.chain.borrow_mut() = later;
            }
            let chain = self.chain.borrow().clone();
            if chain.first().map(|h| h.hash) != Some(from) {
                return Err("not from that checkpoint".into());
            }
            Ok(chain)
        }
        fn fork_choice_openings(&self, blocks: &[Hash64]) -> Result<Vec<PalwForkChoiceOpeningV1>, String> {
            Ok(self.openings.iter().filter(|o| blocks.contains(&o.leaf.block)).copied().collect())
        }
        fn state_proof(&self, block: Hash64, collection: &str) -> Result<(Header, PalwFactProofV1), String> {
            use kaspa_consensus_core::palw_state_proof_v1::{prove_bonds_v1, prove_classes_v1};
            let header = self.chain.borrow().iter().find(|x| x.hash == block).cloned().ok_or("unknown block")?;
            let proof = match collection {
                "bonds" => prove_bonds_v1(&self.state),
                "classes" => prove_classes_v1(&self.state),
                _ => return Err("unknown collection".into()),
            };
            Ok((header, proof))
        }
        fn ruleset(&self) -> Result<crate::verify::NodeRulesetV1, String> {
            Ok(self.ruleset.clone())
        }
    }

    fn vcfg(pubkey: Vec<u8>, checkpoint: &Header, accept: Option<ModeLabelV1>) -> MinerConfig {
        let mut c = cfg(pubkey);
        c.trust = MinerTrustV1 {
            own_full_node: false,
            verification: Some(RemoteVerificationV1 {
                ruleset: ours(),
                checkpoint: crate::verify::TrustedCheckpointV1 {
                    block: checkpoint.hash,
                    daa_score: checkpoint.daa_score,
                    trust: crate::verify::CheckpointTrustV1::Signed { issued_at_daa: checkpoint.daa_score },
                },
                limits: VerifyLimitsV1::default(),
                bond: PalwBondKeyV2(TransactionOutpoint::new(h(21), 0)),
                now_ms: vnow,
                fork_choice: None,
            }),
            accept_unverified: accept,
            pay_to: None,
        };
        c
    }

    fn vrun(
        nodes: &[&VFake],
        cfg: &MinerConfig,
        state: &mut MinerState,
        exec: &mut Exec,
        signer: &MlDsaAttemptSigner,
    ) -> Result<StepOutcome, MinerHalt> {
        let refs: Vec<&dyn RemoteNode> = nodes.iter().map(|n| *n as &dyn RemoteNode).collect();
        step(&refs, cfg, state, exec, signer, &|_, _, _| true)
    }

    /// RFC-0009 L2 by attestation over `node`'s tip: issuer `own-node` (a toy signature: the digest's first three bytes under key `pk`).
    fn attested_cfg(pubkey: Vec<u8>, node: &VFake, attest: bool, accept: Option<ModeLabelV1>) -> MinerConfig {
        use kaspa_consensus_core::config::params::ForkActivation;
        fn toy(pk: &[u8], msg: &[u8], sig: &[u8]) -> bool {
            pk == b"pk" && sig == &msg[..3]
        }
        let tip = node.tip();
        let opening = *node.openings.iter().find(|o| o.leaf.block == tip.hash).expect("the tip's opening");
        let mut att = ForkChoiceAttestationV1 {
            network_id: "testnet-12".into(),
            consensus_params_id: "p".into(),
            consensus_schedule_id: "s".into(),
            block: tip.hash,
            block_daa: tip.daa_score,
            committed_root: opening.committed_root(),
            leaf_version: opening.leaf.leaf_version,
            dns_gate: None,
            issued_at_daa: tip.daa_score,
            key_id: b"own-node".to_vec(),
            signature: Vec::new(),
        };
        att.signature = att.signing_digest().as_bytes().as_slice()[..3].to_vec();
        let mut c = vcfg(pubkey, &node.chain.borrow()[0], accept);
        c.trust.verification.as_mut().unwrap().fork_choice = Some(RemoteForkChoiceV1 {
            rules: ForkChoiceRulesV1 {
                commitment: Some(ForkActivation::new(0)),
                strict_win: None,
                ibd_strict: None,
                frontier_provenance: None,
                dns_gate: None,
                dns_retired: None,
                rule_e: None,
                finality_depth: 600,
                panel: None,
            },
            limits: L2LimitsV1::default(),
            issuers: vec![(b"own-node".to_vec(), b"pk".to_vec())],
            attestations: std::sync::Arc::new(move |_: &[Hash64]| {
                if attest { Ok(vec![att.clone()]) } else { Err(String::from("the issuer is unreachable")) }
            }),
            verify_signature: toy,
        });
        c
    }

    /// **RFC-0009 L2 by attestation, end to end through the step**: two nodes past the commitment fence, the tip attested by the user's
    /// issuer — L1 (headers), L2 (the attested root opened and the tip chosen) and L3 (bond and class under the attested root, through the
    /// walk from the tip) hold: `VERIFIED_REMOTE` with no opt-in, the issuer named on the L2 line. Without a fresh attestation the same
    /// views are `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED` (L3 at the tip's header, the envelope unwrapped by the predecessor's opening),
    /// the reason on the L2 line; a lying class row still stops everything before any work.
    #[test]
    fn an_attested_fork_choice_lifts_the_miner_to_verified_remote_and_names_the_issuer() {
        let signer = keypair();
        let (a, b) = (VFake::enveloped("a", &pk(&signer), h(31), 1), VFake::enveloped("b", &pk(&signer), h(31), 1));
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        let out = vrun(&[&a, &b], &attested_cfg(pk(&signer), &a, true, None), &mut state, &mut exec, &signer).unwrap();
        assert!(matches!(out, StepOutcome::Published(_)), "{out:?}");
        assert_eq!(state.last_mode, Some(ModeLabelV1::VerifiedRemote));
        let line = state.last_l2.clone().unwrap();
        assert!(line.contains("VERIFIED") && line.contains("issuer 'own-node'"), "{line}");

        // No fresh attestation: L2 is not verified, L3 still holds at the header — HEADER_VERIFIED, and only on that opt-in.
        let (a, b) = (VFake::enveloped("a", &pk(&signer), h(31), 1), VFake::enveloped("b", &pk(&signer), h(31), 1));
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        let out = vrun(&[&a, &b], &attested_cfg(pk(&signer), &a, false, None), &mut state, &mut exec, &signer);
        assert!(matches!(&out, Err(MinerHalt::Gate(g)) if g.label == ModeLabelV1::HeaderVerifiedForkChoiceUnverified), "{out:?}");
        assert!(state.last_l2.as_deref().is_some_and(|l| l.contains("NOT verified")), "{:?}", state.last_l2);
        assert_eq!(exec.calls.get(), 0);

        // A lying class row under an attested fork choice: refused before any work.
        let (a, b) = (VFake::enveloped("a", &pk(&signer), h(0x99), 1), VFake::enveloped("b", &pk(&signer), h(0x99), 1));
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        let out = vrun(&[&a, &b], &attested_cfg(pk(&signer), &a, true, None), &mut state, &mut exec, &signer);
        assert!(matches!(out, Err(MinerHalt::Verify(VerifyErrorV1::ContradictsProof { .. }))), "{out:?}");
        assert_eq!(exec.calls.get(), 0);
    }

    /// **Verified remote, honestly labelled**: two nodes serving the same verified chain and proofs put the miner in
    /// HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED (L2 is not verified past the checkpoint), which mines only on that class's explicit opt-in.
    #[test]
    fn a_verified_header_chain_with_proven_rows_is_header_verified_and_mines_only_on_that_opt_in() {
        let signer = keypair();
        let (a, b) = (VFake::new("a", &pk(&signer), h(31), 1), VFake::new("b", &pk(&signer), h(31), 1));
        let checkpoint = a.chain.borrow()[0].clone();
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        let out = vrun(&[&a, &b], &vcfg(pk(&signer), &checkpoint, None), &mut state, &mut exec, &signer);
        assert!(matches!(&out, Err(MinerHalt::Gate(g)) if g.label == ModeLabelV1::HeaderVerifiedForkChoiceUnverified), "{out:?}");
        assert_eq!(exec.calls.get(), 0);
        let accept = Some(ModeLabelV1::HeaderVerifiedForkChoiceUnverified);
        let out = vrun(&[&a, &b], &vcfg(pk(&signer), &checkpoint, accept), &mut state, &mut exec, &signer).unwrap();
        assert!(matches!(out, StepOutcome::Published(_)), "{out:?}");
        assert_eq!(state.last_mode, Some(ModeLabelV1::HeaderVerifiedForkChoiceUnverified));
    }

    /// **Malicious nodes, refused before execution or signing**: a fake class row (the node's facts name an artifact root the proof does
    /// not hold), a node on another fence schedule, a node whose chain conflicts with another's (a hidden competing tip revealed).
    #[test]
    fn a_lying_class_row_a_wrong_schedule_and_a_conflicting_view_stop_the_miner_before_any_work() {
        let signer = keypair();
        let accept = Some(ModeLabelV1::UnverifiedRemote);
        // fake class row: the template facts say root h(31), the chain proves h(0x99)
        let (a, b) = (VFake::new("a", &pk(&signer), h(0x99), 1), VFake::new("b", &pk(&signer), h(0x99), 1));
        let checkpoint = a.chain.borrow()[0].clone();
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        let out = vrun(&[&a, &b], &vcfg(pk(&signer), &checkpoint, accept), &mut state, &mut exec, &signer);
        assert!(matches!(out, Err(MinerHalt::Verify(VerifyErrorV1::ContradictsProof { .. }))), "{out:?}");
        assert_eq!(exec.calls.get(), 0);
        // another fence schedule
        let (a, mut b) = (VFake::new("a", &pk(&signer), h(31), 1), VFake::new("b", &pk(&signer), h(31), 1));
        b.ruleset.consensus_schedule_id = "s-with-another-fence".into();
        let checkpoint = a.chain.borrow()[0].clone();
        let out = vrun(&[&a, &b], &vcfg(pk(&signer), &checkpoint, accept), &mut state, &mut exec, &signer);
        assert!(matches!(out, Err(MinerHalt::Verify(VerifyErrorV1::WrongRuleset { .. }))), "{out:?}");
        // a second node shows a competing branch from the same checkpoint
        let (a, b) = (VFake::new("a", &pk(&signer), h(31), 1), VFake::new("b", &pk(&signer), h(31), 2));
        let checkpoint = a.chain.borrow()[0].clone();
        assert_eq!(checkpoint.hash, b.chain.borrow()[0].hash);
        let out = vrun(&[&a, &b], &vcfg(pk(&signer), &checkpoint, accept), &mut state, &mut exec, &signer);
        assert!(
            matches!(out, Err(MinerHalt::Verify(VerifyErrorV1::ConflictingForks { .. })) | Err(MinerHalt::Template(_))),
            "{out:?}"
        );
        assert_eq!(exec.calls.get(), 0, "no inference in any of the three");
        assert!(a.inner.submitted.borrow().is_empty());
    }

    /// **The re-check right before the signature**: the view moves between the gate and the signature (a reorg to another branch) — the
    /// inference ran, but nothing is signed or sent.
    #[test]
    fn a_reorg_between_the_gate_and_the_signature_leaves_nothing_signed() {
        let signer = keypair();
        let (a, b) = (VFake::new("a", &pk(&signer), h(31), 1), VFake::new("b", &pk(&signer), h(31), 1));
        let checkpoint = a.chain.borrow()[0].clone();
        let other = vchain(a.state.state_root(), 7, 4);
        *a.later.borrow_mut() = Some(other.clone());
        *b.later.borrow_mut() = Some(other);
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        let out = vrun(
            &[&a, &b],
            &vcfg(pk(&signer), &checkpoint, Some(ModeLabelV1::HeaderVerifiedForkChoiceUnverified)),
            &mut state,
            &mut exec,
            &signer,
        );
        assert!(matches!(&out, Err(MinerHalt::Attempt(AttemptError::Signer(why))) if why.contains("re-check")), "{out:?}");
        assert_eq!(exec.calls.get(), 1, "the inference ran");
        assert!(a.inner.submitted.borrow().is_empty() && b.inner.submitted.borrow().is_empty(), "nothing signed, nothing sent");
    }

    /// **RFC-0009 mode C — a pool is a job service, nothing more.** A job a "pool" serves goes through exactly the checks a node's does
    /// (here: two pool endpoints serving the verified chain and proofs land in the same class as two nodes would, and a pool that lies
    /// about the class row is stopped the same way); a pool template that pays the POOL is refused as custodial; and a miner with no pool
    /// at all mines (every other test in this module). There is no pool concept in consensus or in this driver — only nodes.
    #[test]
    fn a_pool_is_only_a_job_service_it_gets_no_trust_and_no_custody() {
        let signer = keypair();
        let accept = Some(ModeLabelV1::HeaderVerifiedForkChoiceUnverified);
        let (p1, p2) = (VFake::new("pool-1", &pk(&signer), h(31), 1), VFake::new("pool-2", &pk(&signer), h(31), 1));
        let checkpoint = p1.chain.borrow()[0].clone();
        let (mut state, mut exec) = (MinerState::default(), Exec { calls: Cell::new(0) });
        // the same class a pair of nodes gives
        let mut c = vcfg(pk(&signer), &checkpoint, accept);
        let pays_us = crate::template::template_pays_v1(&p1.fetch_template().unwrap().block);
        assert_eq!(
            pays_us,
            Some(kaspa_consensus_core::tx::ScriptPublicKey::new(0, vec![0x5A; 34].into())),
            "the coinbase payload parses"
        );
        c.trust.pay_to = pays_us.clone();
        let out = vrun(&[&p1, &p2], &c, &mut state, &mut exec, &signer).unwrap();
        assert!(matches!(out, StepOutcome::Published(_)));
        assert_eq!(state.last_mode, Some(ModeLabelV1::HeaderVerifiedForkChoiceUnverified));
        // a pool lying about the class row is stopped exactly as a node would be
        let (l1, l2) = (VFake::new("pool-1", &pk(&signer), h(0x99), 1), VFake::new("pool-2", &pk(&signer), h(0x99), 1));
        let checkpoint = l1.chain.borrow()[0].clone();
        let calls = exec.calls.get();
        let out = vrun(&[&l1, &l2], &vcfg(pk(&signer), &checkpoint, accept), &mut state, &mut exec, &signer);
        assert!(matches!(out, Err(MinerHalt::Verify(VerifyErrorV1::ContradictsProof { .. }))), "{out:?}");
        assert_eq!(exec.calls.get(), calls, "no inference");
        // a pool template paying the pool: custodial, refused before any work
        let mut c = vcfg(pk(&signer), &p1.chain.borrow()[0].clone(), accept);
        c.trust.pay_to = Some(kaspa_consensus_core::tx::ScriptPublicKey::new(0, vec![0xAB; 34].into()));
        let out = vrun(&[&p1, &p2], &c, &mut state, &mut exec, &signer);
        assert!(matches!(out, Err(MinerHalt::Custodial { .. })), "{out:?}");
        assert_eq!(exec.calls.get(), calls);
    }
}
