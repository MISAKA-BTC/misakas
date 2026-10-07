//! Idempotent relay of bytes the miner already signed.
//!
//! The miner computes the identity of what it sends (tx id, block hash) **locally**, from the signed bytes. A node's reply is
//! only ever compared with that: a node that answers with another id, or that "accepts" something we cannot match, has not relayed
//! our bytes and is recorded as [`NodeOutcome::Tampered`]. Re-sending is safe because the id is a function of the signed fields —
//! "already known" is a success, not an error. An accept is **not** inclusion; that is [`crate::track`]'s business.

use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::tx::{MutableTransaction, Transaction, UtxoEntry};
use kaspa_hashes::Hash64;

/// What a node said about the submission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// Accepted into the node's mempool / DAG; carries the id the NODE computed.
    Accepted(Hash64),
    /// The node already holds it (a previous send, or another relay); carries the id the node holds.
    AlreadyKnown(Hash64),
    /// A named refusal (mass, fee, orphan, double spend...). Not evidence of tampering.
    Refused(String),
    /// No answer (connection, timeout).
    Unreachable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeOutcome {
    Accepted,
    AlreadyKnown,
    Refused(String),
    /// The node answered success for an id that is not ours: it relayed something else, or lied.
    Tampered { returned: Hash64 },
    Unreachable(String),
}

impl NodeOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, NodeOutcome::Accepted | NodeOutcome::AlreadyKnown)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayReport {
    /// The identity we computed from our own bytes.
    pub id: Hash64,
    pub per_node: Vec<(String, NodeOutcome)>,
    pub successes: usize,
}

impl RelayReport {
    pub fn tampered(&self) -> Vec<&str> {
        self.per_node.iter().filter(|(_, o)| matches!(o, NodeOutcome::Tampered { .. })).map(|(n, _)| n.as_str()).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelayFailure {
    #[error("no node accepted: {0:?}")]
    NoneAccepted(RelayReport),
    #[error("only {got} of the required {need} node(s) accepted: {report:?}")]
    TooFew { got: usize, need: usize, report: RelayReport },
    #[error("the carrier failed the miner's own pre-flight: {0}")]
    Preflight(String),
}

/// Judge the replies against `expected`. `Ok` only when at least `min_accept` nodes returned our id; tampering is reported in the
/// report even on success (a miner should know a node lied to it).
pub fn fan_out_verdict(expected: Hash64, replies: Vec<(String, Reply)>, min_accept: usize) -> Result<RelayReport, RelayFailure> {
    let per_node: Vec<(String, NodeOutcome)> = replies
        .into_iter()
        .map(|(node, reply)| {
            let outcome = match reply {
                Reply::Accepted(id) if id == expected => NodeOutcome::Accepted,
                Reply::AlreadyKnown(id) if id == expected => NodeOutcome::AlreadyKnown,
                Reply::Accepted(id) | Reply::AlreadyKnown(id) => NodeOutcome::Tampered { returned: id },
                Reply::Refused(why) => NodeOutcome::Refused(why),
                Reply::Unreachable(why) => NodeOutcome::Unreachable(why),
            };
            (node, outcome)
        })
        .collect();
    let successes = per_node.iter().filter(|(_, o)| o.is_success()).count();
    let report = RelayReport { id: expected, per_node, successes };
    let need = min_accept.max(1);
    if successes == 0 {
        return Err(RelayFailure::NoneAccepted(report));
    }
    if successes < need {
        return Err(RelayFailure::TooFew { got: successes, need, report });
    }
    Ok(report)
}

/// The id of `tx` recomputed from its fields, ignoring the cached one (an in-process object edited without `finalize` carries a stale
/// id — the 2026-09-26 split — and a miner must hand a node the id of the bytes it is actually sending).
pub fn tx_id_of_bytes(tx: &Transaction) -> Hash64 {
    let mut fresh = tx.clone();
    fresh.finalize();
    fresh.id()
}

/// One node that takes a raw transaction.
pub trait RelayNode {
    fn node_id(&self) -> &str;
    fn submit_raw_tx(&self, tx: &Transaction) -> Reply;
}

/// The miner's own pre-flight of a carrier it is about to relay: the funding signature must verify over the transaction's own
/// sighash against the funding entry the miner believes it spends. Catches a bad signer output (or a sidecar that signed other
/// bytes) before a fee is risked, and is the same check the chain's script engine will make.
pub fn carrier_funding_signature_valid(tx: &Transaction, funding: &UtxoEntry) -> Result<(), String> {
    if tx.inputs.len() != 1 {
        return Err(format!("a carrier built by the rail has one funding input, this has {}", tx.inputs.len()));
    }
    let script = &tx.inputs[0].signature_script;
    let (sig_ht, pubkey) = parse_two_pushes(script).ok_or("the signature script is not <signature‖hashtype> <pubkey>")?;
    let (sig, hash_type) = sig_ht.split_at(sig_ht.len().saturating_sub(1));
    if hash_type != [SIG_HASH_ALL.to_u8()] {
        return Err("the funding signature is not SIG_HASH_ALL: a relay could alter what it does not cover".into());
    }
    let mtx = MutableTransaction::with_entries(tx.clone(), vec![funding.clone()]);
    let digest = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &Mldsa87SigHashReusedValuesUnsync::new());
    match kaspa_txscript::verify_mldsa87_with_context(pubkey, digest.as_bytes().as_slice(), sig, kaspa_txscript::MLDSA87_TX_CONTEXT) {
        Ok(true) => Ok(()),
        Ok(false) => Err("the funding signature does not verify over the transaction".into()),
        Err(e) => Err(format!("the funding signature is malformed: {e:?}")),
    }
}

fn read_push(script: &[u8]) -> Option<(&[u8], &[u8])> {
    let (&op, rest) = script.split_first()?;
    let (len, rest) = match op {
        1..=75 => (op as usize, rest),
        0x4c => (*rest.first()? as usize, rest.get(1..)?),
        0x4d => (u16::from_le_bytes([*rest.first()?, *rest.get(1)?]) as usize, rest.get(2..)?),
        _ => return None,
    };
    (rest.len() >= len).then(|| rest.split_at(len))
}

fn parse_two_pushes(script: &[u8]) -> Option<(&[u8], &[u8])> {
    let (first, rest) = read_push(script)?;
    let (second, tail) = read_push(rest)?;
    tail.is_empty().then_some((first, second))
}

/// Send one signed carrier to every node and judge the replies. Pre-flights the signature when the funding entry is known.
pub fn broadcast_signed_tx(
    tx: &Transaction,
    funding: Option<&UtxoEntry>,
    nodes: &[&dyn RelayNode],
    min_accept: usize,
) -> Result<RelayReport, RelayFailure> {
    if let Some(entry) = funding {
        carrier_funding_signature_valid(tx, entry).map_err(RelayFailure::Preflight)?;
    }
    // Recomputed from the bytes, never the cached `tx.id()`: an in-process object edited without `finalize` carries a stale id.
    let expected = tx_id_of_bytes(tx);
    let mut seen = std::collections::BTreeSet::new();
    let replies = nodes
        .iter()
        .filter(|n| seen.insert(n.node_id().to_string()))
        .map(|n| (n.node_id().to_string(), n.submit_raw_tx(tx)))
        .collect();
    fan_out_verdict(expected, replies, min_accept)
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::cell::RefCell;
    pub enum Mode {
        Honest,
        Refuse(&'static str),
        Down,
        ReturnsOtherId,
    }
    pub struct FakeRelay {
        pub id: String,
        pub mode: Mode,
        pub known: RefCell<bool>,
        pub sent: RefCell<Vec<Transaction>>,
    }
    impl FakeRelay {
        pub fn new(id: &str, mode: Mode) -> Self {
            Self { id: id.into(), mode, known: RefCell::new(false), sent: RefCell::new(vec![]) }
        }
    }
    impl RelayNode for FakeRelay {
        fn node_id(&self) -> &str {
            &self.id
        }
        fn submit_raw_tx(&self, tx: &Transaction) -> Reply {
            self.sent.borrow_mut().push(tx.clone());
            match self.mode {
                Mode::Down => Reply::Unreachable("timeout".into()),
                Mode::Refuse(why) => Reply::Refused(why.into()),
                Mode::ReturnsOtherId => Reply::Accepted(Hash64::from_bytes([0x66; 64])),
                Mode::Honest => {
                    // A real node recomputes the id from the bytes it received.
                    let id = tx.id();
                    if self.known.replace(true) { Reply::AlreadyKnown(id) } else { Reply::Accepted(id) }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::*;
    use super::*;
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;

    fn tx() -> Transaction {
        Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![1, 2, 3])
    }
    fn refs(v: &[FakeRelay]) -> Vec<&dyn RelayNode> {
        v.iter().map(|n| n as &dyn RelayNode).collect()
    }

    #[test]
    fn honest_nodes_accept_and_a_second_send_is_idempotent() {
        let nodes = [FakeRelay::new("a", Mode::Honest), FakeRelay::new("b", Mode::Honest)];
        let r1 = broadcast_signed_tx(&tx(), None, &refs(&nodes), 2).unwrap();
        assert_eq!(r1.successes, 2);
        assert_eq!(r1.id, tx().id());
        let r2 = broadcast_signed_tx(&tx(), None, &refs(&nodes), 2).unwrap();
        assert!(r2.per_node.iter().all(|(_, o)| *o == NodeOutcome::AlreadyKnown), "double submission is a no-op success");
    }

    #[test]
    fn a_node_that_returns_another_id_is_tampering_and_never_counts() {
        let nodes = [FakeRelay::new("a", Mode::Honest), FakeRelay::new("liar", Mode::ReturnsOtherId)];
        let r = broadcast_signed_tx(&tx(), None, &refs(&nodes), 1).unwrap();
        assert_eq!(r.successes, 1);
        assert_eq!(r.tampered(), vec!["liar"]);
        // …and when only the liar answers, there is no success at all.
        let only = [FakeRelay::new("liar", Mode::ReturnsOtherId)];
        assert!(matches!(broadcast_signed_tx(&tx(), None, &refs(&only), 1), Err(RelayFailure::NoneAccepted(_))));
    }

    #[test]
    fn refusals_and_silence_are_reported_not_hidden_and_min_accept_is_enforced() {
        let nodes = [FakeRelay::new("a", Mode::Honest), FakeRelay::new("b", Mode::Refuse("orphan")), FakeRelay::new("c", Mode::Down)];
        assert!(matches!(broadcast_signed_tx(&tx(), None, &refs(&nodes), 2), Err(RelayFailure::TooFew { got: 1, need: 2, .. })));
        let ok = broadcast_signed_tx(&tx(), None, &refs(&nodes), 1).unwrap();
        assert!(ok.per_node.iter().any(|(n, o)| n == "b" && *o == NodeOutcome::Refused("orphan".into())));
    }

    #[test]
    fn a_relay_that_mutates_the_bytes_changes_the_id_so_the_miner_sees_a_different_tx() {
        // The miner's expected id is computed from ITS bytes. A node that forwards altered bytes answers with the altered id.
        let mut altered = tx();
        altered.payload[0] ^= 1;
        altered.finalize();
        assert_ne!(altered.id(), tx().id());
        let r = fan_out_verdict(tx().id(), vec![("n".into(), Reply::Accepted(altered.id()))], 1);
        assert!(matches!(r, Err(RelayFailure::NoneAccepted(_))));
    }
}
