//! A chain view assembled from several independent nodes, against a pinned checkpoint.
//!
//! The rule is **stop on disagreement**. There is no "majority wins": an inference costs real compute and a signature costs a bond's
//! credibility, so a view that two nodes cannot agree on is a view the miner does not act on — it waits, retries, and reports.

use crate::checkpoint::Checkpoint;
use kaspa_hashes::Hash64;
use std::collections::BTreeSet;

/// What one node reports about itself. Plain data: the adapter reads it off `getBlockDagInfo`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeFacts {
    pub network_id: String,
    pub sink: Hash64,
    pub virtual_daa: u64,
    pub pruning_point: Hash64,
}

/// A node's answer to "is this checkpoint block on your selected chain".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointStatus {
    /// The block is known and is a selected-chain block.
    OnChain,
    /// The node does not know the block (not synced that far, or the checkpoint is ahead of it): says nothing about the chain.
    Unknown,
    /// The node knows a *different* block at the checkpoint's DAA score, or knows this one off its selected chain: a fork.
    Conflicts,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ViewError(pub String);

/// One node as the quorum sees it.
pub trait ChainView {
    /// A stable, distinct name for the endpoint; two views with the same id count once.
    fn node_id(&self) -> &str;
    fn facts(&self) -> Result<NodeFacts, ViewError>;
    fn checkpoint_status(&self, checkpoint: &Checkpoint) -> Result<CheckpointStatus, ViewError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuorumPolicy {
    /// Independent nodes that must agree. Clamped to at least 2: a quorum of one is a single RPC's say-so.
    pub min_agree: usize,
    /// The largest spread of `virtual_daa` across the responders, in DAA. Nodes a block or two apart are normal; more is not a view.
    pub max_daa_skew: u64,
    /// The pinned checkpoint every responder must be on (or not yet past).
    pub checkpoint: Checkpoint,
}

impl QuorumPolicy {
    pub const DEFAULT_MAX_DAA_SKEW: u64 = 12;
    pub fn new(checkpoint: Checkpoint) -> Self {
        Self { min_agree: 2, max_daa_skew: Self::DEFAULT_MAX_DAA_SKEW, checkpoint }
    }
    pub fn effective_min_agree(&self) -> usize {
        self.min_agree.max(2)
    }
}

/// What the quorum agreed on. `virtual_daa` is the **lowest** reported (the conservative clock for freshness).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgreedView {
    pub network_id: String,
    pub virtual_daa: u64,
    pub pruning_point: Hash64,
    pub sinks: Vec<(String, Hash64)>,
    pub nodes: Vec<String>,
}

impl AgreedView {
    /// What this view is worth: several nodes agreeing, with nothing proven against a header. Every fact taken from it is
    /// [`crate::trust::UNVERIFIED_REMOTE_STATE`] until a state proof (`crate::proof`) says otherwise.
    pub fn provenance(&self) -> crate::trust::Provenance {
        crate::trust::Provenance::UnverifiedRemoteState { agreeing: self.nodes.len() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Halt {
    #[error("only {responding} distinct node(s) answered and agree on the checkpoint, {needed} are required (unreachable or unsynced: {silent:?})")]
    TooFewNodes { responding: usize, needed: usize, silent: Vec<String> },
    #[error("nodes disagree on the network: {0:?}")]
    NetworkMismatch(Vec<(String, String)>),
    #[error("node {node} reports the pinned checkpoint {checkpoint_daa} on another chain — fork or lying node")]
    CheckpointConflict { node: String, checkpoint_daa: u64 },
    #[error("nodes disagree on the pruning point: {0:?}")]
    PruningPointMismatch(Vec<(String, Hash64)>),
    #[error("virtual DAA spread {spread} across nodes exceeds the allowed {max}")]
    DaaSkew { spread: u64, max: u64 },
    #[error("the pinned checkpoint is for network {pinned:?} but the nodes report {reported:?}")]
    CheckpointNetwork { pinned: String, reported: String },
}

/// Ask every node, then decide. Errors from a node are *silence*, not disagreement; contradiction is what halts.
pub fn agree(nodes: &[&dyn ChainView], policy: &QuorumPolicy) -> Result<AgreedView, Halt> {
    let needed = policy.effective_min_agree();
    let mut seen = BTreeSet::new();
    let mut answers: Vec<(String, NodeFacts)> = Vec::new();
    let mut silent: Vec<String> = Vec::new();
    for node in nodes {
        let id = node.node_id().to_string();
        if !seen.insert(id.clone()) {
            continue; // the same endpoint twice is one voice
        }
        let facts = match node.facts() {
            Ok(f) => f,
            Err(_) => {
                silent.push(id);
                continue;
            }
        };
        match node.checkpoint_status(&policy.checkpoint) {
            Ok(CheckpointStatus::OnChain) => answers.push((id, facts)),
            Ok(CheckpointStatus::Unknown) | Err(_) => silent.push(id),
            Ok(CheckpointStatus::Conflicts) => {
                return Err(Halt::CheckpointConflict { node: id, checkpoint_daa: policy.checkpoint.daa_score });
            }
        }
    }
    if answers.len() < needed {
        return Err(Halt::TooFewNodes { responding: answers.len(), needed, silent });
    }
    let network = answers[0].1.network_id.clone();
    if answers.iter().any(|(_, f)| f.network_id != network) {
        return Err(Halt::NetworkMismatch(answers.iter().map(|(n, f)| (n.clone(), f.network_id.clone())).collect()));
    }
    if network != policy.checkpoint.network_id {
        return Err(Halt::CheckpointNetwork { pinned: policy.checkpoint.network_id.clone(), reported: network });
    }
    let pruning = answers[0].1.pruning_point;
    if answers.iter().any(|(_, f)| f.pruning_point != pruning) {
        return Err(Halt::PruningPointMismatch(answers.iter().map(|(n, f)| (n.clone(), f.pruning_point)).collect()));
    }
    let lo = answers.iter().map(|(_, f)| f.virtual_daa).min().expect("non-empty");
    let hi = answers.iter().map(|(_, f)| f.virtual_daa).max().expect("non-empty");
    if hi - lo > policy.max_daa_skew {
        return Err(Halt::DaaSkew { spread: hi - lo, max: policy.max_daa_skew });
    }
    Ok(AgreedView {
        network_id: network,
        virtual_daa: lo,
        pruning_point: pruning,
        sinks: answers.iter().map(|(n, f)| (n.clone(), f.sink)).collect(),
        nodes: answers.into_iter().map(|(n, _)| n).collect(),
    })
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    pub struct FakeNode {
        pub id: String,
        pub facts: Result<NodeFacts, ViewError>,
        pub status: Result<CheckpointStatus, ViewError>,
    }
    impl ChainView for FakeNode {
        fn node_id(&self) -> &str {
            &self.id
        }
        fn facts(&self) -> Result<NodeFacts, ViewError> {
            self.facts.clone()
        }
        fn checkpoint_status(&self, _: &Checkpoint) -> Result<CheckpointStatus, ViewError> {
            self.status.clone()
        }
    }
    pub fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    pub fn node(id: &str, daa: u64, status: CheckpointStatus) -> FakeNode {
        FakeNode {
            id: id.into(),
            facts: Ok(NodeFacts { network_id: "testnet-12".into(), sink: h(daa as u8), virtual_daa: daa, pruning_point: h(0xEE) }),
            status: Ok(status),
        }
    }
    pub fn policy() -> QuorumPolicy {
        QuorumPolicy::new(Checkpoint { network_id: "testnet-12".into(), daa_score: 5_000, block_hash: h(0xCC) })
    }
}

#[cfg(test)]
mod tests {
    use super::fake::*;
    use super::*;

    fn refs<'a>(v: &'a [FakeNode]) -> Vec<&'a dyn ChainView> {
        v.iter().map(|n| n as &dyn ChainView).collect()
    }

    #[test]
    fn two_independent_nodes_on_the_checkpoint_make_a_view_and_the_clock_is_the_lowest() {
        let nodes = [node("a", 6_000, CheckpointStatus::OnChain), node("b", 6_003, CheckpointStatus::OnChain)];
        let v = agree(&refs(&nodes), &policy()).unwrap();
        assert_eq!(v.virtual_daa, 6_000);
        assert_eq!(v.nodes, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn one_node_is_never_a_quorum_and_the_same_endpoint_twice_is_one_voice() {
        let one = [node("a", 6_000, CheckpointStatus::OnChain)];
        assert!(matches!(agree(&refs(&one), &policy()), Err(Halt::TooFewNodes { responding: 1, needed: 2, .. })));
        let twice = [node("a", 6_000, CheckpointStatus::OnChain), node("a", 6_000, CheckpointStatus::OnChain)];
        assert!(matches!(agree(&refs(&twice), &policy()), Err(Halt::TooFewNodes { responding: 1, .. })));
        let mut p = policy();
        p.min_agree = 1;
        assert_eq!(p.effective_min_agree(), 2, "a configured quorum of one is raised to two");
    }

    #[test]
    fn a_node_that_contradicts_the_pinned_checkpoint_halts_even_when_the_others_agree() {
        let nodes = [
            node("a", 6_000, CheckpointStatus::OnChain),
            node("b", 6_000, CheckpointStatus::OnChain),
            node("evil", 6_000, CheckpointStatus::Conflicts),
        ];
        assert!(matches!(agree(&refs(&nodes), &policy()), Err(Halt::CheckpointConflict { node, .. }) if node == "evil"));
    }

    #[test]
    fn an_unsynced_or_unreachable_node_is_silence_not_a_vote_and_not_a_halt() {
        let mut dead = node("dead", 6_000, CheckpointStatus::OnChain);
        dead.facts = Err(ViewError("connection refused".into()));
        let nodes = [node("a", 6_000, CheckpointStatus::OnChain), node("behind", 100, CheckpointStatus::Unknown), dead];
        match agree(&refs(&nodes), &policy()) {
            Err(Halt::TooFewNodes { responding: 1, silent, .. }) => assert_eq!(silent.len(), 2),
            other => panic!("{other:?}"),
        }
        let three = [node("a", 6_000, CheckpointStatus::OnChain), node("b", 6_001, CheckpointStatus::OnChain), node("behind", 100, CheckpointStatus::Unknown)];
        assert!(agree(&refs(&three), &policy()).is_ok());
    }

    #[test]
    fn disagreement_on_network_pruning_point_or_clock_halts() {
        let mut other_net = node("b", 6_000, CheckpointStatus::OnChain);
        other_net.facts.as_mut().unwrap().network_id = "mainnet".into();
        let nodes = [node("a", 6_000, CheckpointStatus::OnChain), other_net];
        assert!(matches!(agree(&refs(&nodes), &policy()), Err(Halt::NetworkMismatch(_))));
        let mut other_pp = node("b", 6_000, CheckpointStatus::OnChain);
        other_pp.facts.as_mut().unwrap().pruning_point = h(1);
        let nodes = [node("a", 6_000, CheckpointStatus::OnChain), other_pp];
        assert!(matches!(agree(&refs(&nodes), &policy()), Err(Halt::PruningPointMismatch(_))));
        let nodes = [node("a", 6_000, CheckpointStatus::OnChain), node("b", 6_100, CheckpointStatus::OnChain)];
        assert_eq!(agree(&refs(&nodes), &policy()), Err(Halt::DaaSkew { spread: 100, max: 12 }));
    }

    #[test]
    fn a_checkpoint_pinned_for_another_network_is_refused() {
        let nodes = [node("a", 6_000, CheckpointStatus::OnChain), node("b", 6_000, CheckpointStatus::OnChain)];
        let mut p = policy();
        p.checkpoint.network_id = "testnet-11".into();
        assert!(matches!(agree(&refs(&nodes), &p), Err(Halt::CheckpointNetwork { .. })));
    }

    #[test]
    fn an_agreed_view_is_labelled_unverified_remote_state_however_many_nodes_agree() {
        let nodes = [node("a", 6_000, CheckpointStatus::OnChain), node("b", 6_001, CheckpointStatus::OnChain), node("c", 6_002, CheckpointStatus::OnChain)];
        let v = agree(&refs(&nodes), &policy()).unwrap();
        let p = v.provenance();
        assert!(!p.is_proven());
        assert!(p.label().starts_with(crate::trust::UNVERIFIED_REMOTE_STATE));
        assert!(p.label().contains("3 node(s)"));
    }
}
