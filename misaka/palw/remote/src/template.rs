//! The remote attempt template: what a miner without `kaspad` checks before it spends an inference, and how it submits the finished
//! block.
//!
//! No new node op is needed. The facts an attempt is mounted on are already served: `getBlockTemplate` (the header position),
//! `getPalwProducerFacts` (the class target, pwu, artifact root, retention, the bond's key and readiness) and `getBlockDagInfo`.
//! What a remote miner must not do is take ONE node's word for them — a stale parent, a wrong target or a spoofed bond state turns a
//! minute of inference into nothing, and the consensus signature the miner makes afterwards covers the miner's own bytes, not the
//! node's claims. So the facts from several nodes are folded into a [`template_digest_v1`], the digests must match, the template
//! must be fresh against the quorum's clock, and the same check is repeated immediately before the block goes out.

use crate::relay::{RelayFailure, RelayReport, Reply, fan_out_verdict};
use crate::{finish, keyed, put_len};
use kaspa_hashes::Hash64;

pub const DOMAIN_TEMPLATE: &[u8] = b"misaka-palw/remote/attempt-template/v1";

/// The producer facts a template is judged on (a subset of `GetPalwProducerFactsResponse`, already parsed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProducerFactsSummary {
    pub chain_point: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub class_target: u128,
    pub pwu: u64,
    pub min_trace_retention_daa: u64,
    /// The bond's registered ML-DSA-87 key as the chain reports it.
    pub bond_pubkey: Vec<u8>,
    /// Empty = the bond may produce now (`not_ready_reason`).
    pub not_ready_reason: String,
}

/// One node's template, as the adapter read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateObservation {
    pub node: String,
    pub network_id: String,
    pub pruning_point: Hash64,
    pub version: u16,
    pub pow_algo_id: u8,
    pub bits: u32,
    pub daa_score: u64,
    pub palw_state_root: Hash64,
    pub parents: Vec<Hash64>,
    pub facts: ProducerFactsSummary,
}

/// The wire spellings `getPalwProducerFacts` answers in, parsed: 128-hex ids, a decimal `u128` target, a hex public key.
#[allow(clippy::too_many_arguments)]
pub fn producer_facts_from_wire_v1(
    chain_point: &str,
    class_id: &str,
    artifact_root: &str,
    class_target: &str,
    pwu: u64,
    min_trace_retention_daa: u64,
    bond_registered_pubkey_hex: &str,
    not_ready_reason: &str,
) -> Result<ProducerFactsSummary, String> {
    let hash = |what: &str, s: &str| s.trim().parse::<Hash64>().map_err(|_| format!("{what} '{s}' is not a 128-hex id"));
    let mut pubkey = vec![0u8; bond_registered_pubkey_hex.len() / 2];
    if bond_registered_pubkey_hex.len() % 2 != 0
        || (0..pubkey.len())
            .any(|i| u8::from_str_radix(&bond_registered_pubkey_hex[2 * i..2 * i + 2], 16).map(|b| pubkey[i] = b).is_err())
    {
        return Err("the bond's registered key is not hex".into());
    }
    Ok(ProducerFactsSummary {
        chain_point: hash("chain point", chain_point)?,
        class_id: hash("class id", class_id)?,
        artifact_root: hash("artifact root", artifact_root)?,
        class_target: class_target.trim().parse().map_err(|_| format!("class target '{class_target}' is not a u128"))?,
        pwu,
        min_trace_retention_daa,
        bond_pubkey: pubkey,
        not_ready_reason: not_ready_reason.to_string(),
    })
}

/// **One node's template as an observation** — the header `getBlockTemplate` returned (converted to a consensus header) and that
/// node's producer facts. Parents are the header's direct parents.
pub fn template_observation_v1(
    node: &str,
    network_id: &str,
    header: &kaspa_consensus_core::header::Header,
    facts: ProducerFactsSummary,
) -> TemplateObservation {
    TemplateObservation {
        node: node.to_string(),
        network_id: network_id.to_string(),
        pruning_point: header.pruning_point,
        version: header.version,
        pow_algo_id: header.pow_algo_id,
        bits: header.bits,
        daa_score: header.daa_score,
        palw_state_root: header.palw_state_root,
        parents: header.direct_parents().to_vec(),
        facts,
    }
}

/// **Where a template's block reward goes**: the script in its coinbase payload (blue score u64, subsidy u64, script version u16, script
/// length u8, script — `CoinbaseManager::serialize_coinbase_payload`). A job service (a pool, a node) that hands the miner a template paying
/// ITS script is offering custody of the reward; the miner refuses it by default (RFC-0009 mode C: non-custodial by default).
pub fn template_pays_v1(block: &kaspa_consensus_core::block::Block) -> Option<kaspa_consensus_core::tx::ScriptPublicKey> {
    let payload = &block.transactions.first()?.payload;
    let version = u16::from_le_bytes(payload.get(16..18)?.try_into().ok()?);
    let len = *payload.get(18)? as usize;
    let script = payload.get(19..19 + len)?;
    Some(kaspa_consensus_core::tx::ScriptPublicKey::new(version, script.to_vec().into()))
}

/// What the nodes must agree on. Excludes the timestamp, nonce, transactions, coinbase and merkle roots, which legitimately differ
/// per node and per moment; includes the parents (the attempt's challenge binds `pre_pow_hash`, which commits to them) and every
/// chain fact the attempt is priced against.
pub fn template_digest_v1(t: &TemplateObservation) -> Hash64 {
    let mut s = keyed(DOMAIN_TEMPLATE);
    put_len(&mut s, t.network_id.as_bytes());
    s.update(t.pruning_point.as_bytes().as_slice());
    s.update(&t.version.to_le_bytes());
    s.update(&[t.pow_algo_id]);
    s.update(&t.bits.to_le_bytes());
    s.update(&t.daa_score.to_le_bytes());
    s.update(t.palw_state_root.as_bytes().as_slice());
    let mut parents = t.parents.clone();
    parents.sort();
    parents.dedup();
    s.update(&(parents.len() as u32).to_le_bytes());
    for p in &parents {
        s.update(p.as_bytes().as_slice());
    }
    s.update(t.facts.chain_point.as_bytes().as_slice());
    s.update(t.facts.class_id.as_bytes().as_slice());
    s.update(t.facts.artifact_root.as_bytes().as_slice());
    s.update(&t.facts.class_target.to_le_bytes());
    s.update(&t.facts.pwu.to_le_bytes());
    s.update(&t.facts.min_trace_retention_daa.to_le_bytes());
    finish(s)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplatePolicy {
    /// Independent nodes whose templates must carry the same digest (clamped to ≥ 2).
    pub min_agree: usize,
    /// `virtual_daa − template.daa_score` must not exceed this, at start and again before submission.
    pub max_template_age_daa: u64,
    /// The key and artifact the miner actually holds.
    pub held_pubkey: Vec<u8>,
    pub held_artifact_root: Hash64,
}

impl TemplatePolicy {
    pub const DEFAULT_MAX_TEMPLATE_AGE_DAA: u64 = 30;
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TemplateRefusal {
    #[error("only {got} node template(s) agree, {need} required (digests seen: {digests:?})")]
    NoQuorum { got: usize, need: usize, digests: Vec<(String, Hash64)> },
    #[error("the nodes disagree on the template and one dissent is enough to stop: {digests:?}")]
    Disagreement { digests: Vec<(String, Hash64)> },
    #[error("the template is {age} DAA behind the chain view, more than the allowed {max}")]
    Stale { age: u64, max: u64 },
    #[error("the template is ahead of the chain view ({template} > {view}): the view is the stale one")]
    AheadOfView { template: u64, view: u64 },
    #[error("the bond may not produce now: {0}")]
    NotReady(String),
    #[error("the chain's registered key for this bond is not the key this miner holds")]
    BondKeyMismatch,
    #[error("the class artifact on chain is not the artifact this miner holds")]
    ArtifactMismatch,
    #[error("the chain point moved between the first check and the last: the attempt was mounted on a stale view")]
    ChainMoved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedTemplate {
    pub digest: Hash64,
    pub chain_point: Hash64,
    pub daa_score: u64,
    /// The nodes whose templates matched.
    pub agreeing: Vec<String>,
    pub observation: TemplateObservation,
}

/// Judge the observations against the quorum's clock (`view_daa` = [`crate::view::AgreedView::virtual_daa`]).
pub fn check_templates(
    observations: &[TemplateObservation],
    policy: &TemplatePolicy,
    view_daa: u64,
) -> Result<AcceptedTemplate, TemplateRefusal> {
    let need = policy.min_agree.max(2);
    let mut by_digest: Vec<(Hash64, Vec<&TemplateObservation>)> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut digests = Vec::new();
    for obs in observations.iter().filter(|o| seen.insert(o.node.clone())) {
        let d = template_digest_v1(obs);
        digests.push((obs.node.clone(), d));
        match by_digest.iter_mut().find(|(k, _)| *k == d) {
            Some((_, v)) => v.push(obs),
            None => by_digest.push((d, vec![obs])),
        }
    }
    let best = by_digest.iter().max_by_key(|(_, v)| v.len()).filter(|(_, v)| v.len() >= need);
    let Some((digest, group)) = best else {
        return Err(TemplateRefusal::NoQuorum { got: by_digest.iter().map(|(_, v)| v.len()).max().unwrap_or(0), need, digests });
    };
    // Unanimity, not majority: with two agreeing and one dissenting the dissent is a signal that the view is moving or that a node
    // lies, and the cost of acting on a wrong template is an inference — so the miner stops and retries instead of outvoting it.
    if by_digest.len() > 1 {
        return Err(TemplateRefusal::Disagreement { digests });
    }
    let obs = group[0];
    if obs.daa_score > view_daa {
        return Err(TemplateRefusal::AheadOfView { template: obs.daa_score, view: view_daa });
    }
    let age = view_daa - obs.daa_score;
    if age > policy.max_template_age_daa {
        return Err(TemplateRefusal::Stale { age, max: policy.max_template_age_daa });
    }
    if !obs.facts.not_ready_reason.is_empty() {
        return Err(TemplateRefusal::NotReady(obs.facts.not_ready_reason.clone()));
    }
    if obs.facts.bond_pubkey != policy.held_pubkey {
        return Err(TemplateRefusal::BondKeyMismatch);
    }
    if obs.facts.artifact_root != policy.held_artifact_root {
        return Err(TemplateRefusal::ArtifactMismatch);
    }
    Ok(AcceptedTemplate {
        digest: *digest,
        chain_point: obs.facts.chain_point,
        daa_score: obs.daa_score,
        agreeing: group.iter().map(|o| o.node.clone()).collect(),
        observation: obs.clone(),
    })
}

/// The check made immediately before the finished block is submitted: a fresh quorum must still stand on the same chain point and
/// the template's age against the NEW clock must still be within bound. The inference took time; this is what notices it.
pub fn recheck_before_submit(
    accepted: &AcceptedTemplate,
    fresh: &[TemplateObservation],
    policy: &TemplatePolicy,
    view_daa: u64,
) -> Result<(), TemplateRefusal> {
    let now = check_templates(fresh, policy, view_daa)?;
    if now.chain_point != accepted.chain_point {
        return Err(TemplateRefusal::ChainMoved);
    }
    let age = view_daa.saturating_sub(accepted.daa_score);
    if age > policy.max_template_age_daa {
        return Err(TemplateRefusal::Stale { age, max: policy.max_template_age_daa });
    }
    Ok(())
}

/// One node that takes a completed block.
pub trait BlockNode {
    fn node_id(&self) -> &str;
    fn submit_block(&self, block_hash: Hash64, block_bytes: &[u8]) -> Reply;
}

/// Submit the completed block to every node. The hash is the miner's own; a node that returns another is tampering; "already known"
/// is success, so submitting twice (or to nodes that already heard it by gossip) is safe.
pub fn submit_block_idempotent(
    block_hash: Hash64,
    block_bytes: &[u8],
    nodes: &[&dyn BlockNode],
    min_accept: usize,
) -> Result<RelayReport, RelayFailure> {
    let mut seen = std::collections::BTreeSet::new();
    let replies = nodes
        .iter()
        .filter(|n| seen.insert(n.node_id().to_string()))
        .map(|n| (n.node_id().to_string(), n.submit_block(block_hash, block_bytes)))
        .collect();
    fan_out_verdict(block_hash, replies, min_accept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::NodeOutcome;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn obs(node: &str, daa: u64) -> TemplateObservation {
        TemplateObservation {
            node: node.into(),
            network_id: "testnet-12".into(),
            pruning_point: h(1),
            version: 2,
            pow_algo_id: 9,
            bits: 0x1d00ffff,
            daa_score: daa,
            palw_state_root: h(2),
            parents: vec![h(4), h(3)],
            facts: ProducerFactsSummary {
                chain_point: h(5),
                class_id: h(6),
                artifact_root: h(7),
                class_target: 1 << 100,
                pwu: 7_708,
                min_trace_retention_daa: 3_000,
                bond_pubkey: vec![9; 8],
                not_ready_reason: String::new(),
            },
        }
    }
    fn policy() -> TemplatePolicy {
        TemplatePolicy { min_agree: 2, max_template_age_daa: 30, held_pubkey: vec![9; 8], held_artifact_root: h(7) }
    }

    #[test]
    fn wire_facts_parse_strictly_and_a_header_becomes_the_observation_it_digests_as() {
        let hex = |b: u8| Hash64::from_bytes([b; 64]).to_string();
        let f =
            producer_facts_from_wire_v1(&hex(5), &hex(6), &hex(7), "1267650600228229401496703205376", 7_708, 100, "0aff", "").unwrap();
        assert_eq!(f.class_target, 1u128 << 100);
        assert_eq!(f.bond_pubkey, vec![0x0a, 0xff]);
        assert!(producer_facts_from_wire_v1("zz", &hex(6), &hex(7), "1", 1, 1, "", "").is_err());
        assert!(producer_facts_from_wire_v1(&hex(5), &hex(6), &hex(7), "-1", 1, 1, "", "").is_err());
        assert!(producer_facts_from_wire_v1(&hex(5), &hex(6), &hex(7), "1", 1, 1, "abc", "").is_err());
        let header = kaspa_consensus_core::header::Header::from_precomputed_hash(h(9), vec![h(4), h(3)]);
        let o = template_observation_v1("a", "testnet-12", &header, f.clone());
        assert_eq!(o.parents, vec![h(4), h(3)]);
        assert_eq!(
            template_digest_v1(&o),
            template_digest_v1(&template_observation_v1("b", "testnet-12", &header, f)),
            "the node name is not in the digest"
        );
    }

    #[test]
    fn the_digest_ignores_parent_order_and_what_legitimately_differs_per_node_but_not_chain_facts() {
        let a = obs("a", 100);
        let mut b = obs("b", 100);
        b.parents.reverse();
        assert_eq!(template_digest_v1(&a), template_digest_v1(&b));
        for mutate in [
            (|t: &mut TemplateObservation| t.bits += 1) as fn(&mut TemplateObservation),
            |t| t.daa_score += 1,
            |t| t.parents.push(h(8)),
            |t| t.palw_state_root = h(9),
            |t| t.facts.class_target += 1,
            |t| t.facts.pwu += 1,
            |t| t.facts.chain_point = h(0x55),
            |t| t.facts.artifact_root = h(0x56),
            |t| t.network_id = "other".into(),
        ] {
            let mut c = obs("c", 100);
            mutate(&mut c);
            assert_ne!(template_digest_v1(&a), template_digest_v1(&c));
        }
    }

    #[test]
    fn two_agreeing_fresh_templates_with_the_held_key_are_accepted() {
        let t = check_templates(&[obs("a", 100), obs("b", 100)], &policy(), 110).unwrap();
        assert_eq!(t.agreeing, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn a_stale_template_is_refused_at_start_and_again_before_submission() {
        assert_eq!(
            check_templates(&[obs("a", 100), obs("b", 100)], &policy(), 131).unwrap_err(),
            TemplateRefusal::Stale { age: 31, max: 30 }
        );
        let accepted = check_templates(&[obs("a", 100), obs("b", 100)], &policy(), 110).unwrap();
        // The inference took 40 DAA: the same agreeing quorum no longer makes the template fresh.
        assert!(matches!(
            recheck_before_submit(&accepted, &[obs("a", 100), obs("b", 100)], &policy(), 140),
            Err(TemplateRefusal::Stale { .. })
        ));
        // And a quorum on a different chain point is "the view moved".
        let mut moved_a = obs("a", 120);
        let mut moved_b = obs("b", 120);
        moved_a.facts.chain_point = h(0x77);
        moved_b.facts.chain_point = h(0x77);
        assert_eq!(recheck_before_submit(&accepted, &[moved_a, moved_b], &policy(), 125), Err(TemplateRefusal::ChainMoved));
        assert!(recheck_before_submit(&accepted, &[obs("a", 100), obs("b", 100)], &policy(), 120).is_ok());
    }

    #[test]
    fn one_dissenting_node_stops_the_miner_even_when_two_agree() {
        let mut liar = obs("liar", 100);
        liar.facts.class_target = 1 << 120; // an easier target: the miner would mount an attempt that cannot win
        assert!(matches!(
            check_templates(&[obs("a", 100), obs("b", 100), liar], &policy(), 105),
            Err(TemplateRefusal::Disagreement { .. })
        ));
        assert!(matches!(check_templates(&[obs("a", 100)], &policy(), 105), Err(TemplateRefusal::NoQuorum { got: 1, .. })));
    }

    #[test]
    fn facts_the_miner_can_check_against_what_it_holds_are_checked() {
        let mut p = policy();
        p.held_pubkey = vec![1; 8];
        assert_eq!(check_templates(&[obs("a", 100), obs("b", 100)], &p, 105).unwrap_err(), TemplateRefusal::BondKeyMismatch);
        let mut p = policy();
        p.held_artifact_root = h(0x99);
        assert_eq!(check_templates(&[obs("a", 100), obs("b", 100)], &p, 105).unwrap_err(), TemplateRefusal::ArtifactMismatch);
        let mut a = obs("a", 100);
        let mut b = obs("b", 100);
        a.facts.not_ready_reason = "exposure ceiling".into();
        b.facts.not_ready_reason = "exposure ceiling".into();
        assert!(matches!(check_templates(&[a, b], &policy(), 105), Err(TemplateRefusal::NotReady(_))));
        let ahead = check_templates(&[obs("a", 200), obs("b", 200)], &policy(), 105);
        assert!(matches!(ahead, Err(TemplateRefusal::AheadOfView { .. })));
    }

    struct Node(&'static str, fn(Hash64) -> Reply);
    impl BlockNode for Node {
        fn node_id(&self) -> &str {
            self.0
        }
        fn submit_block(&self, hash: Hash64, _: &[u8]) -> Reply {
            (self.1)(hash)
        }
    }

    #[test]
    fn a_completed_block_goes_to_every_node_and_a_double_submission_is_a_success() {
        let ok = Node("a", |hh| Reply::Accepted(hh));
        let dup = Node("b", |hh| Reply::AlreadyKnown(hh));
        let evil = Node("c", |_| Reply::Accepted(Hash64::from_bytes([0xEE; 64])));
        let nodes: Vec<&dyn BlockNode> = vec![&ok, &dup, &evil, &ok];
        let r = submit_block_idempotent(h(1), b"block", &nodes, 2).unwrap();
        assert_eq!(r.successes, 2, "the repeated endpoint is one node");
        assert_eq!(r.tampered(), vec!["c"]);
        assert!(r.per_node.iter().any(|(n, o)| n == "b" && *o == NodeOutcome::AlreadyKnown));
    }
}
