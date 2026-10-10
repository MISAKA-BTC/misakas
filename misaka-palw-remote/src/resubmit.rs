//! **RFC-0009 mandatory test 2, "obstruct", wired: a relay that ACKs and never forwards is routed around — with the SAME bytes.**
//!
//! `misaka-palw-fp-rail --track` polled each node once and forgot. [`track::ClaimTracker::relayed_at`] already knew the rule (an ACK that no
//! observer ever confirms within the bound is a swallowed carrier); nothing remembered the ACK between two invocations, so nothing acted on
//! it. This module is that memory and that action, as a pure state machine the rail drives once per poll:
//!
//! ```text
//!   ResubmitStateV1 (a JSON file, atomically rewritten)  ──┐
//!   one ChainObservation per answering node               ──┼──▶ decide_resubmit_v1 ──▶ SendFirst | Wait | Confirmed | Settled
//!   the signed carrier (its id is checked against the file)─┘                            | Resend{relay} | Blind | Exhausted
//! ```
//!
//! # What makes a resubmission safe
//!
//! * **The same bytes, never a re-sign.** The state names the transaction id; [`step_resubmit_v1`] recomputes the id of the bytes it is
//!   about to send ([`relay::tx_id_of_bytes`]) and refuses anything else ([`ResubmitError::CarrierChanged`]). This module holds no key and no
//!   signer, so it cannot build another carrier. The claim id is a function of the signed commitment and the carrier spends one funding
//!   input, so a second copy through another relay is the same transaction: at most one of them can ever be mined, the fee is paid once
//!   (the transaction's own input − output difference), and the chain refuses a duplicate claim id idempotently. A node that already holds the
//!   bytes answers "already known" — a success, not a second charge ([`relay::NodeOutcome::AlreadyKnown`]).
//! * **An ACK is a relay's word.** It is confirmed only by an INDEPENDENT observer: a node that is not the relay being judged and not a relay
//!   already found to be swallowing. A relay saying "it is in my mempool" proves nothing; the chain showing the claim (reported by a node that
//!   is not under suspicion) does. Nothing here is a proof: every observation is `UNVERIFIED_REMOTE_STATE`.
//! * **Time is a median.** A node reporting a far-future DAA would otherwise make every ACK look overdue and burn through the relay list;
//!   the clock is the lower median of the answering nodes' DAA, so one liar cannot move it.
//! * **Bounded.** A relay found swallowing (or refusing, tampering, unreachable) is never asked again, so a plan sends at most once per
//!   relay, plus [`RESUBMIT_MAX_RESENDS_V1`] re-sends after a carrier that was SEEN has vanished (a reorg or an evicted mempool). With every
//!   relay spent the answer is [`ResubmitDecision::Exhausted`] — loud, and the operator adds relays; the loop never spins.
//! * **No independent observer, no action.** With nobody to confirm or refute an ACK the decision is [`ResubmitDecision::Blind`]: nothing is
//!   sent and nothing is believed.

use crate::relay::{NodeOutcome, RelayFailure, RelayNode, broadcast_signed_tx, tx_id_of_bytes};
use crate::track::{ChainObservation, ClaimTracker, TrackState};
use kaspa_consensus_core::tx::{Transaction, TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The schema tag of the state file.
pub const RESUBMIT_STATE_SCHEMA_V1: &str = "misaka.palw.rail-resubmit.v1";
/// How many DAA an ACK may stay unconfirmed before the relay is judged to have swallowed the carrier.
pub const RESUBMIT_DEFAULT_BOUND_DAA: u64 = 120;
/// The depth at which `Final`/void is reported settled (the rail's `--finality-depth` default).
pub const RESUBMIT_DEFAULT_FINALITY_DEPTH: u64 = 60;
/// Re-sends after the carrier was seen and then vanished (reorg, mempool eviction). Beyond this the plan reports [`ResubmitDecision::Exhausted`].
pub const RESUBMIT_MAX_RESENDS_V1: u32 = 3;

/// One relay's ACK of the carrier, at the clock the plan read when it was received.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResubmitAttemptV1 {
    pub relay: String,
    pub acked_at_daa: u64,
}

/// **The memory between two `--track` invocations.** Plain JSON; rewritten atomically after every step that changes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResubmitStateV1 {
    pub schema: String,
    /// The transaction id of the carrier, 128 lowercase hex — what the bytes MUST hash to before they are sent anywhere.
    pub tx_id: String,
    pub claim_id: String,
    /// The executor bond the chain's claim row must name: `txid:index`.
    pub bond: String,
    pub bound_daa: u64,
    pub finality_depth: u64,
    /// The relays, in the order they are tried.
    pub relays: Vec<String>,
    /// Every ACK, in order.
    pub attempts: Vec<ResubmitAttemptV1>,
    /// Relays never asked again: ACKed and never confirmed, or refused, tampered with the id, or were unreachable.
    pub suspects: Vec<String>,
    /// An independent observer has shown the carrier (mempool or chain) at least once.
    pub ever_seen: bool,
    /// Re-sends made after the carrier was seen and then vanished.
    pub resends_after_seen: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResubmitError {
    #[error(
        "the carrier's bytes hash to {got}, not the {want} this plan was made for: a resubmission sends the same bytes or nothing"
    )]
    CarrierChanged { want: String, got: String },
    #[error("the carrier fails the miner's own pre-flight and was sent nowhere: {0}")]
    Preflight(String),
    #[error("the resubmit state is not usable: {0}")]
    State(String),
}

impl ResubmitStateV1 {
    /// A new plan for `tx`, claim `claim_id` under executor bond `bond`, through `relays` in order (duplicates dropped, order kept).
    pub fn new(
        tx: &Transaction,
        claim_id: Hash64,
        bond: TransactionOutpoint,
        relays: &[String],
        bound_daa: u64,
        finality_depth: u64,
    ) -> Result<Self, ResubmitError> {
        let mut seen = std::collections::BTreeSet::new();
        let relays: Vec<String> = relays.iter().filter(|r| !r.is_empty() && seen.insert((*r).clone())).cloned().collect();
        if relays.is_empty() {
            return Err(ResubmitError::State("a resubmit plan needs at least one relay".to_string()));
        }
        if bound_daa == 0 {
            return Err(ResubmitError::State(
                "the confirmation bound is zero DAA: every ACK would be judged swallowed at once".to_string(),
            ));
        }
        Ok(Self {
            schema: RESUBMIT_STATE_SCHEMA_V1.to_string(),
            tx_id: tx_id_of_bytes(tx).to_string(),
            claim_id: claim_id.to_string(),
            bond: format!("{}:{}", bond.transaction_id, bond.index),
            bound_daa,
            finality_depth,
            relays,
            attempts: Vec::new(),
            suspects: Vec::new(),
            ever_seen: false,
            resends_after_seen: 0,
        })
    }

    /// Load a state file; a missing file is `Ok(None)`, a file that is not this schema is an error (never silently replaced).
    pub fn load(path: &Path) -> Result<Option<Self>, ResubmitError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(ResubmitError::State(format!("cannot read {}: {e}", path.display()))),
        };
        let state: Self = serde_json::from_str(&text)
            .map_err(|e| ResubmitError::State(format!("{} is not a resubmit state: {e}", path.display())))?;
        if state.schema != RESUBMIT_STATE_SCHEMA_V1 {
            return Err(ResubmitError::State(format!(
                "{} has schema {:?}, not {RESUBMIT_STATE_SCHEMA_V1}",
                path.display(),
                state.schema
            )));
        }
        Ok(Some(state))
    }

    /// Write atomically (a temporary file in the same directory, then a rename), so a crash leaves the old state or the new one.
    pub fn save(&self, path: &Path) -> Result<(), ResubmitError> {
        let text = serde_json::to_string_pretty(self).map_err(|e| ResubmitError::State(e.to_string()))?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, text).map_err(|e| ResubmitError::State(format!("cannot write {}: {e}", tmp.display())))?;
        std::fs::rename(&tmp, path).map_err(|e| ResubmitError::State(format!("cannot move {} into place: {e}", path.display())))
    }

    fn parse(&self) -> Result<(Hash64, Hash64, TransactionOutpoint), ResubmitError> {
        let hash = |what: &str, text: &str| -> Result<Hash64, ResubmitError> {
            let mut out = [0u8; 64];
            if text.len() != 128 || faster_hex::hex_decode(text.as_bytes(), &mut out).is_err() {
                return Err(ResubmitError::State(format!("{what} is not 128 hex chars")));
            }
            Ok(Hash64::from_bytes(out))
        };
        let tx_id = hash("tx_id", &self.tx_id)?;
        let claim_id = hash("claim_id", &self.claim_id)?;
        let (txid, index) = self.bond.split_once(':').ok_or_else(|| ResubmitError::State("bond is not txid:index".to_string()))?;
        let bond = TransactionOutpoint::new(
            hash("bond txid", txid)?,
            index.parse::<u32>().map_err(|e| ResubmitError::State(format!("bond index: {e}")))?,
        );
        Ok((tx_id, claim_id, bond))
    }

    fn is_suspect(&self, relay: &str) -> bool {
        self.suspects.iter().any(|s| s == relay)
    }

    fn mark_suspect(&mut self, relay: &str) {
        if !self.is_suspect(relay) {
            self.suspects.push(relay.to_string());
        }
    }

    /// Relays still worth asking, in preference order: not suspect; fewest ACKs first (ties in the configured order); `avoid` last.
    fn candidates(&self, avoid: Option<&str>) -> Vec<String> {
        let acks = |r: &str| self.attempts.iter().filter(|a| a.relay == r).count();
        let mut live: Vec<(usize, usize, &String)> =
            self.relays.iter().enumerate().filter(|(_, r)| !self.is_suspect(r)).map(|(i, r)| (acks(r), i, r)).collect();
        live.sort_by_key(|(n, i, r)| (avoid == Some(r.as_str()), *n, *i));
        live.into_iter().map(|(_, _, r)| r.clone()).collect()
    }
}

/// Why the plan sends the bytes again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResendReason {
    /// `relay` ACKed `bound_daa` ago and no independent observer has ever seen the carrier.
    Swallowed { relay: String },
    /// The carrier was seen, and now no independent observer holds it nor shows the claim: a reorg or an evicted mempool.
    DroppedAfterSeen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResubmitDecision {
    /// No ACK yet: send through `relay`.
    SendFirst { relay: String },
    /// An ACK is inside its bound: ask again at `until_daa`.
    Wait { until_daa: u64 },
    /// An independent observer shows the carrier or the claim. Not settled.
    Confirmed,
    /// The claim is `Final`/void at depth, or the chain's row names another executor bond: sending is over.
    Settled(TrackState),
    /// Send the same bytes through `relay`.
    Resend { relay: String, reason: ResendReason },
    /// Nobody independent answered: nothing can be confirmed or refuted, so nothing is sent.
    Blind,
    /// Every relay has been tried (or the resend budget is spent) and the carrier is not confirmed. `swallowed` is the relay this very
    /// decision judged to have swallowed it (the step marks it spent), if any.
    Exhausted { tried: Vec<String>, swallowed: Option<String> },
}

/// The lower median of the answering nodes' DAA (`None` when nobody answered).
fn median_daa(observations: &[(String, ChainObservation)]) -> Option<u64> {
    let mut v: Vec<u64> = observations.iter().map(|(_, o)| o.virtual_daa).collect();
    v.sort_unstable();
    v.get(v.len().checked_sub(1)? / 2).copied()
}

/// **What to do now**, from the state and one observation per answering node. Pure: it sends nothing and changes nothing.
pub fn decide_resubmit_v1(
    state: &ResubmitStateV1,
    observations: &[(String, ChainObservation)],
) -> Result<ResubmitDecision, ResubmitError> {
    let (tx_id, claim_id, bond) = state.parse()?;
    let Some(now) = median_daa(observations) else { return Ok(ResubmitDecision::Blind) };

    // The current attempt is the last ACK; its relay is the one being judged.
    let current = state.attempts.last();
    // An independent observer: not the relay being judged, not a relay already found swallowing.
    let independent: Vec<&(String, ChainObservation)> = observations
        .iter()
        .filter(|(node, _)| Some(node.as_str()) != current.map(|a| a.relay.as_str()) && !state.is_suspect(node))
        .collect();

    // What the independent observers say, through the tracker's own classification (finality depth, attribution).
    let mut seen = false;
    for (_, obs) in &independent {
        let mut tracker = ClaimTracker::new(tx_id, claim_id, bond, state.finality_depth);
        let verdict = tracker.observe(obs).clone();
        match verdict {
            TrackState::Misattributed { .. } | TrackState::Final { .. } | TrackState::Void { .. } => {
                return Ok(ResubmitDecision::Settled(verdict));
            }
            TrackState::Built | TrackState::NeedsRebroadcast => {}
            _ => seen = true,
        }
    }
    if seen {
        return Ok(ResubmitDecision::Confirmed);
    }
    if independent.is_empty() {
        return Ok(ResubmitDecision::Blind);
    }

    let Some(current) = current else {
        // Never ACKed: nobody independent has it either (checked above), so send.
        return Ok(match state.candidates(None).into_iter().next() {
            Some(relay) => ResubmitDecision::SendFirst { relay },
            None => ResubmitDecision::Exhausted { tried: state.relays.clone(), swallowed: None },
        });
    };
    let due = current.acked_at_daa.saturating_add(state.bound_daa);
    if now < due {
        return Ok(ResubmitDecision::Wait { until_daa: due });
    }
    if state.ever_seen {
        // It was seen, and nobody independent holds it now.
        if state.resends_after_seen >= RESUBMIT_MAX_RESENDS_V1 {
            return Ok(ResubmitDecision::Exhausted { tried: state.relays.clone(), swallowed: None });
        }
        return Ok(match state.candidates(Some(&current.relay)).into_iter().next() {
            Some(relay) => ResubmitDecision::Resend { relay, reason: ResendReason::DroppedAfterSeen },
            None => ResubmitDecision::Exhausted { tried: state.relays.clone(), swallowed: None },
        });
    }
    // ACKed, past its bound, never seen by anyone independent: swallowed. The relay is spent; the next one is chosen without it.
    let mut after = state.clone();
    after.mark_suspect(&current.relay);
    Ok(match after.candidates(None).into_iter().next() {
        Some(relay) => ResubmitDecision::Resend { relay, reason: ResendReason::Swallowed { relay: current.relay.clone() } },
        None => ResubmitDecision::Exhausted { tried: state.relays.clone(), swallowed: Some(current.relay.clone()) },
    })
}

/// What one [`step_resubmit_v1`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResubmitStep {
    pub decision: ResubmitDecision,
    /// Every relay asked in this step, with what it said. Empty when nothing was sent.
    pub sent: Vec<(String, NodeOutcome)>,
    /// The relay whose ACK is now the plan's current attempt, if this step made one.
    pub acked_by: Option<String>,
    /// The decision after the sends: `Exhausted` when every candidate refused.
    pub outcome: ResubmitDecision,
}

/// **Decide, and if the decision is to send, send the same bytes through the chosen relay** (falling through to the next candidate when a
/// relay refuses, tampers with the id or does not answer — each such relay is spent). Mutates `state` (the caller saves it).
///
/// `nodes` are the relays this process can reach; a relay named in the plan that is not among them is simply not asked (and not spent).
pub fn step_resubmit_v1(
    state: &mut ResubmitStateV1,
    tx: &Transaction,
    funding: Option<&UtxoEntry>,
    observations: &[(String, ChainObservation)],
    nodes: &[&dyn RelayNode],
) -> Result<ResubmitStep, ResubmitError> {
    let got = tx_id_of_bytes(tx).to_string();
    if got != state.tx_id {
        return Err(ResubmitError::CarrierChanged { want: state.tx_id.clone(), got });
    }
    let decision = decide_resubmit_v1(state, observations)?;
    let now = median_daa(observations).unwrap_or(0);
    let mut step = ResubmitStep { decision: decision.clone(), sent: Vec::new(), acked_by: None, outcome: decision.clone() };
    let first = match &decision {
        ResubmitDecision::Confirmed => {
            state.ever_seen = true;
            return Ok(step);
        }
        ResubmitDecision::SendFirst { relay } => relay.clone(),
        ResubmitDecision::Resend { relay, reason } => {
            match reason {
                ResendReason::Swallowed { relay: swallower } => state.mark_suspect(swallower),
                ResendReason::DroppedAfterSeen => state.resends_after_seen += 1,
            }
            relay.clone()
        }
        ResubmitDecision::Exhausted { swallowed, .. } => {
            // The last relay that was waited on is judged too, so the saved state says why nothing more will be sent.
            if let Some(relay) = swallowed {
                state.mark_suspect(relay);
            }
            return Ok(step);
        }
        _ => return Ok(step),
    };
    // Try the chosen relay, then the rest of the candidates in preference order.
    let mut order = vec![first.clone()];
    order.extend(state.candidates(state.attempts.last().map(|a| a.relay.as_str())).into_iter().filter(|r| *r != first));
    for relay in order {
        let Some(node) = nodes.iter().find(|n| n.node_id() == relay) else { continue };
        match broadcast_signed_tx(tx, funding, &[*node], 1) {
            Ok(report) => {
                let outcome = report.per_node.into_iter().next().map(|(_, o)| o).unwrap_or(NodeOutcome::Accepted);
                step.sent.push((relay.clone(), outcome));
                state.attempts.push(ResubmitAttemptV1 { relay: relay.clone(), acked_at_daa: now });
                step.acked_by = Some(relay);
                step.outcome = decision;
                return Ok(step);
            }
            Err(RelayFailure::Preflight(why)) => return Err(ResubmitError::Preflight(why)),
            Err(RelayFailure::NoneAccepted(report)) | Err(RelayFailure::TooFew { report, .. }) => {
                // Refused, tampered with the id or silent: this relay is spent, the next one is tried.
                let outcome =
                    report.per_node.into_iter().next().map(|(_, o)| o).unwrap_or(NodeOutcome::Unreachable("no reply".to_string()));
                step.sent.push((relay.clone(), outcome));
                state.mark_suspect(&relay);
            }
        }
    }
    step.outcome = ResubmitDecision::Exhausted { tried: state.relays.clone(), swallowed: None };
    Ok(step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::Reply;
    use crate::track::{ClaimObs, ClaimPhaseObs};
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeSet;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn bond() -> TransactionOutpoint {
        TransactionOutpoint::new(h(10), 0)
    }
    fn carrier() -> Transaction {
        Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![7, 7, 7])
    }
    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    fn plan(relays: &[&str], bound: u64) -> ResubmitStateV1 {
        ResubmitStateV1::new(&carrier(), h(2), bond(), &names(relays), bound, 60).unwrap()
    }
    fn obs(daa: u64, in_mempool: bool, claim: Option<ClaimPhaseObs>) -> ChainObservation {
        ChainObservation {
            sink: h(1),
            virtual_daa: daa,
            tx_in_mempool: in_mempool,
            claim: claim.map(|phase| ClaimObs { executor_bond: bond(), accepted_block: h(0xB1), accepted_daa: 100, phase }),
        }
    }
    fn all(daa: u64, who: &[(&str, bool)]) -> Vec<(String, ChainObservation)> {
        who.iter().map(|(n, m)| (n.to_string(), obs(daa, *m, None))).collect()
    }

    /// A small network: every node answers with whether IT holds the carrier. An honest relay that takes the bytes gossips them to the other
    /// honest nodes at once; a swallowing relay ACKs with the right id and holds and forwards nothing.
    struct SimNet {
        daa: Cell<u64>,
        holders: RefCell<BTreeSet<String>>,
        on_chain: Cell<bool>,
        honest: Vec<&'static str>,
        asked: RefCell<Vec<String>>,
    }
    struct SimRelay<'a> {
        id: &'static str,
        net: &'a SimNet,
        swallows: bool,
        refuses: bool,
    }
    impl RelayNode for SimRelay<'_> {
        fn node_id(&self) -> &str {
            self.id
        }
        fn submit_raw_tx(&self, tx: &Transaction) -> Reply {
            self.net.asked.borrow_mut().push(self.id.to_string());
            if self.refuses {
                return Reply::Refused("mempool full".to_string());
            }
            let already = self.net.holders.borrow().contains(self.id);
            if !self.swallows {
                let mut holders = self.net.holders.borrow_mut();
                holders.extend(self.net.honest.iter().map(|s| s.to_string()));
            }
            let id = tx.id();
            if already { Reply::AlreadyKnown(id) } else { Reply::Accepted(id) }
        }
    }
    impl SimNet {
        fn new(honest: &[&'static str]) -> Self {
            SimNet {
                daa: Cell::new(1_000),
                holders: RefCell::new(BTreeSet::new()),
                on_chain: Cell::new(false),
                honest: honest.to_vec(),
                asked: RefCell::new(vec![]),
            }
        }
        /// What every named node would answer a poll now.
        fn poll(&self, nodes: &[&str]) -> Vec<(String, ChainObservation)> {
            nodes
                .iter()
                .map(|n| {
                    let claim = self.on_chain.get().then_some(ClaimPhaseObs::Provisional);
                    (n.to_string(), obs(self.daa.get(), self.holders.borrow().contains(*n), claim))
                })
                .collect()
        }
    }

    #[test]
    fn a_plan_names_the_bytes_and_refuses_other_bytes() {
        let mut s = plan(&["a", "b"], 10);
        let mut other = carrier();
        other.payload[0] ^= 1;
        other.finalize();
        let net = SimNet::new(&["b", "c"]);
        let r = SimRelay { id: "a", net: &net, swallows: false, refuses: false };
        let e = step_resubmit_v1(&mut s, &other, None, &all(1_000, &[("c", false)]), &[&r]).unwrap_err();
        assert!(matches!(e, ResubmitError::CarrierChanged { .. }), "{e:?}");
        assert!(net.asked.borrow().is_empty(), "nothing was sent");
        // A stale cached id on an edited-but-not-finalized transaction is not the bytes either: the id is recomputed from the fields.
        let mut stale = carrier();
        stale.payload[0] ^= 1;
        assert!(step_resubmit_v1(&mut s, &stale, None, &all(1_000, &[("c", false)]), &[&r]).is_err());
        // A plan needs a relay and a non-zero bound.
        assert!(ResubmitStateV1::new(&carrier(), h(2), bond(), &[], 10, 60).is_err());
        assert!(ResubmitStateV1::new(&carrier(), h(2), bond(), &names(&["a"]), 0, 60).is_err());
        // Duplicates in the relay list are dropped, order kept.
        assert_eq!(plan(&["b", "a", "b", ""], 10).relays, names(&["b", "a"]));
    }

    /// **The obstruction case, end to end through the state machine.** Relay `a` ACKs and forwards nothing; the observers `c` and `d` never see
    /// the carrier. Inside the bound the plan waits; at the bound it sends the SAME bytes through `b`, which is honest: the observers see it,
    /// the claim reaches the chain, and `a` is never asked again.
    #[test]
    fn a_relay_that_swallows_the_carrier_is_routed_around_with_the_same_bytes_and_asked_once() {
        let tx = carrier();
        let net = SimNet::new(&["b", "c", "d"]);
        let a = SimRelay { id: "a", net: &net, swallows: true, refuses: false };
        let b = SimRelay { id: "b", net: &net, swallows: false, refuses: false };
        let nodes: [&dyn RelayNode; 2] = [&a, &b];
        let all_nodes = ["a", "b", "c", "d"];
        let mut s = plan(&["a", "b"], 10);

        // Poll 1: nothing yet. Send through the first relay.
        let step = step_resubmit_v1(&mut s, &tx, None, &net.poll(&all_nodes), &nodes).unwrap();
        assert_eq!(step.decision, ResubmitDecision::SendFirst { relay: "a".to_string() });
        assert_eq!(step.acked_by.as_deref(), Some("a"));
        assert_eq!(s.attempts, vec![ResubmitAttemptV1 { relay: "a".to_string(), acked_at_daa: 1_000 }]);

        // Poll 2, inside the bound: `a` says nothing the observers can confirm; wait.
        net.daa.set(1_005);
        let step = step_resubmit_v1(&mut s, &tx, None, &net.poll(&all_nodes), &nodes).unwrap();
        assert_eq!(step.decision, ResubmitDecision::Wait { until_daa: 1_010 });
        assert!(step.sent.is_empty());

        // Poll 3, at the bound: the observers still do not hold it. `a` swallowed it; the SAME bytes go through `b`.
        net.daa.set(1_010);
        let step = step_resubmit_v1(&mut s, &tx, None, &net.poll(&all_nodes), &nodes).unwrap();
        assert_eq!(
            step.decision,
            ResubmitDecision::Resend { relay: "b".to_string(), reason: ResendReason::Swallowed { relay: "a".to_string() } }
        );
        assert_eq!(step.acked_by.as_deref(), Some("b"));
        assert_eq!(s.suspects, names(&["a"]));
        assert_eq!(net.asked.borrow().as_slice(), ["a", "b"], "one send each, in order");

        // Poll 4: the independent observers (c, d — and a, who is suspect, is ignored) now hold it. Confirmed.
        net.daa.set(1_012);
        let step = step_resubmit_v1(&mut s, &tx, None, &net.poll(&all_nodes), &nodes).unwrap();
        assert_eq!(step.decision, ResubmitDecision::Confirmed);
        assert!(step.sent.is_empty() && s.ever_seen);

        // Poll 5: the claim is on chain. Still confirmed (the chain row is ours); nothing more is sent.
        net.on_chain.set(true);
        net.daa.set(1_100);
        let step = step_resubmit_v1(&mut s, &tx, None, &net.poll(&all_nodes), &nodes).unwrap();
        assert_eq!(step.decision, ResubmitDecision::Confirmed);
        // Poll 6: past the finality depth the claim is settled.
        let late = vec![("c".to_string(), obs(1_200, false, Some(ClaimPhaseObs::Final { final_daa: 1_100 })))];
        let step = step_resubmit_v1(&mut s, &tx, None, &late, &nodes).unwrap();
        assert!(matches!(step.decision, ResubmitDecision::Settled(TrackState::Final { .. })), "{:?}", step.decision);

        assert_eq!(
            net.asked.borrow().as_slice(),
            ["a", "b"],
            "the swallowing relay was asked once, the honest one once, and nobody again"
        );
    }

    #[test]
    fn an_acking_relays_own_word_and_a_suspects_word_confirm_nothing() {
        // `a` ACKed and says it holds the carrier; nobody else does. Its own word is not confirmation: at the bound it is judged swallowed.
        let mut s = plan(&["a", "b"], 10);
        s.attempts.push(ResubmitAttemptV1 { relay: "a".to_string(), acked_at_daa: 1_000 });
        let polled = all(1_010, &[("a", true), ("c", false)]);
        assert_eq!(
            decide_resubmit_v1(&s, &polled).unwrap(),
            ResubmitDecision::Resend { relay: "b".to_string(), reason: ResendReason::Swallowed { relay: "a".to_string() } }
        );
        // A relay already found swallowing cannot confirm the NEXT attempt by saying so (nor by showing a claim row it invented).
        s.suspects.push("a".to_string());
        s.attempts.push(ResubmitAttemptV1 { relay: "b".to_string(), acked_at_daa: 1_010 });
        let liar = vec![
            ("a".to_string(), obs(1_015, true, Some(ClaimPhaseObs::Final { final_daa: 1_000 }))),
            ("c".to_string(), obs(1_015, false, None)),
        ];
        assert_eq!(decide_resubmit_v1(&s, &liar).unwrap(), ResubmitDecision::Wait { until_daa: 1_020 });
        // The only observer is the relay being judged: nobody independent, so nothing is believed and nothing is sent.
        assert_eq!(decide_resubmit_v1(&s, &all(1_030, &[("b", true)])).unwrap(), ResubmitDecision::Blind);
        assert_eq!(decide_resubmit_v1(&s, &[]).unwrap(), ResubmitDecision::Blind);
    }

    #[test]
    fn one_node_reporting_a_far_future_clock_cannot_make_an_ack_look_overdue() {
        let mut s = plan(&["a", "b"], 10);
        s.attempts.push(ResubmitAttemptV1 { relay: "a".to_string(), acked_at_daa: 1_000 });
        let mut polled = all(1_003, &[("c", false), ("d", false)]);
        polled.push(("liar".to_string(), obs(u64::MAX, false, None)));
        assert_eq!(decide_resubmit_v1(&s, &polled).unwrap(), ResubmitDecision::Wait { until_daa: 1_010 }, "the median clock is 1,003");
        // A lone liar answering IS the median of one: it can move the clock, which costs one idempotent resend through another relay
        // and nothing else (the same bytes, the same fee, the same claim).
        let only = vec![("liar".to_string(), obs(u64::MAX, false, None))];
        assert!(matches!(decide_resubmit_v1(&s, &only).unwrap(), ResubmitDecision::Resend { .. }));
    }

    #[test]
    fn a_relay_that_refuses_or_lies_about_the_id_is_spent_and_the_next_is_tried_in_the_same_step() {
        let tx = carrier();
        let net = SimNet::new(&["c", "d", "good"]);
        let refuses = SimRelay { id: "no", net: &net, swallows: false, refuses: true };
        let good = SimRelay { id: "good", net: &net, swallows: false, refuses: false };
        let nodes: [&dyn RelayNode; 2] = [&refuses, &good];
        let mut s = plan(&["no", "good"], 10);
        let step = step_resubmit_v1(&mut s, &tx, None, &net.poll(&["c", "d", "no", "good"]), &nodes).unwrap();
        assert_eq!(step.sent.len(), 2);
        assert_eq!(step.sent[0], ("no".to_string(), NodeOutcome::Refused("mempool full".to_string())));
        assert!(step.sent[1].1.is_success());
        assert_eq!(step.acked_by.as_deref(), Some("good"));
        assert_eq!(s.suspects, names(&["no"]), "a refusing relay is never asked again");

        // A relay that returns another id (tampering) is spent too.
        struct Liar;
        impl RelayNode for Liar {
            fn node_id(&self) -> &str {
                "liar"
            }
            fn submit_raw_tx(&self, _: &Transaction) -> Reply {
                Reply::Accepted(h(0x66))
            }
        }
        let mut s = plan(&["liar"], 10);
        let step = step_resubmit_v1(&mut s, &tx, None, &all(1_000, &[("c", false)]), &[&Liar]).unwrap();
        assert!(matches!(step.sent[0].1, NodeOutcome::Tampered { .. }));
        assert_eq!(step.outcome, ResubmitDecision::Exhausted { tried: names(&["liar"]), swallowed: None });
        assert!(s.attempts.is_empty() && s.suspects == names(&["liar"]));
    }

    #[test]
    fn with_every_relay_spent_the_plan_is_exhausted_and_stays_so() {
        let tx = carrier();
        let net = SimNet::new(&["c"]);
        let a = SimRelay { id: "a", net: &net, swallows: true, refuses: false };
        let b = SimRelay { id: "b", net: &net, swallows: true, refuses: false };
        let nodes: [&dyn RelayNode; 2] = [&a, &b];
        let mut s = plan(&["a", "b"], 10);
        for (daa, want_sent) in [(1_000u64, true), (1_010, true), (1_020, false), (1_100, false)] {
            net.daa.set(daa);
            let step = step_resubmit_v1(&mut s, &tx, None, &net.poll(&["a", "b", "c"]), &nodes).unwrap();
            assert_eq!(step.acked_by.is_some(), want_sent, "daa {daa}: {:?}", step.decision);
        }
        assert!(matches!(decide_resubmit_v1(&s, &net.poll(&["a", "b", "c"])).unwrap(), ResubmitDecision::Exhausted { .. }));
        assert_eq!(net.asked.borrow().as_slice(), ["a", "b"], "each swallowing relay was asked exactly once");
        assert_eq!(s.suspects, names(&["a", "b"]));
    }

    #[test]
    fn a_carrier_that_was_seen_and_vanished_is_resent_a_bounded_number_of_times() {
        let tx = carrier();
        let net = SimNet::new(&["a", "b", "c"]);
        let a = SimRelay { id: "a", net: &net, swallows: false, refuses: false };
        let b = SimRelay { id: "b", net: &net, swallows: false, refuses: false };
        let nodes: [&dyn RelayNode; 2] = [&a, &b];
        let mut s = plan(&["a", "b"], 10);
        step_resubmit_v1(&mut s, &tx, None, &net.poll(&["a", "b", "c"]), &nodes).unwrap();
        net.daa.set(1_001);
        assert_eq!(
            step_resubmit_v1(&mut s, &tx, None, &net.poll(&["a", "b", "c"]), &nodes).unwrap().decision,
            ResubmitDecision::Confirmed
        );
        assert!(s.ever_seen);
        // The mempools drop it (an eviction or a reorg): within the bound the plan waits, past it the bytes go out again — through the OTHER relay.
        for round in 0..RESUBMIT_MAX_RESENDS_V1 {
            net.holders.borrow_mut().clear();
            net.daa.set(net.daa.get() + 11);
            let step = step_resubmit_v1(&mut s, &tx, None, &net.poll(&["a", "b", "c"]), &nodes).unwrap();
            assert!(
                matches!(step.decision, ResubmitDecision::Resend { reason: ResendReason::DroppedAfterSeen, .. }),
                "round {round}: {:?}",
                step.decision
            );
            // A resend after a vanish is not a verdict on the relay: nobody is spent.
            assert!(s.suspects.is_empty());
            // It is seen again, then dropped again.
            net.daa.set(net.daa.get() + 1);
            assert_eq!(
                step_resubmit_v1(&mut s, &tx, None, &net.poll(&["a", "b", "c"]), &nodes).unwrap().decision,
                ResubmitDecision::Confirmed
            );
        }
        net.holders.borrow_mut().clear();
        net.daa.set(net.daa.get() + 11);
        let step = step_resubmit_v1(&mut s, &tx, None, &net.poll(&["a", "b", "c"]), &nodes).unwrap();
        assert!(matches!(step.decision, ResubmitDecision::Exhausted { .. }), "the resend budget is spent: {:?}", step.decision);
        assert_eq!(s.resends_after_seen, RESUBMIT_MAX_RESENDS_V1);
    }

    #[test]
    fn a_claim_naming_another_bond_ends_the_sending_and_is_never_ours() {
        let s = plan(&["a", "b"], 10);
        let foreign = ChainObservation {
            sink: h(1),
            virtual_daa: 1_000,
            tx_in_mempool: false,
            claim: Some(ClaimObs {
                executor_bond: TransactionOutpoint::new(h(77), 0),
                accepted_block: h(0xB1),
                accepted_daa: 100,
                phase: ClaimPhaseObs::Provisional,
            }),
        };
        let d = decide_resubmit_v1(&s, &[("c".to_string(), foreign)]).unwrap();
        assert!(matches!(d, ResubmitDecision::Settled(TrackState::Misattributed { .. })), "{d:?}");
        // …and a claim already on chain under our bond before the first send is a reason not to send at all.
        let ours = vec![("c".to_string(), obs(1_000, false, Some(ClaimPhaseObs::Provisional)))];
        assert_eq!(decide_resubmit_v1(&s, &ours).unwrap(), ResubmitDecision::Confirmed);
    }

    #[test]
    fn the_state_file_round_trips_atomically_and_refuses_what_it_is_not() {
        let dir = std::env::temp_dir().join(format!("palw-resubmit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        assert_eq!(ResubmitStateV1::load(&path), Ok(None), "no file is no plan");
        let mut s = plan(&["a", "b"], 10);
        s.attempts.push(ResubmitAttemptV1 { relay: "a".to_string(), acked_at_daa: 7 });
        s.suspects.push("a".to_string());
        s.save(&path).unwrap();
        assert!(!path.with_extension("tmp").exists(), "the temporary file was renamed into place");
        assert_eq!(ResubmitStateV1::load(&path).unwrap(), Some(s.clone()));
        // Not this schema, not JSON, a damaged id: refused, never replaced.
        let mut other = s.clone();
        other.schema = "something.else".to_string();
        std::fs::write(&path, serde_json::to_string(&other).unwrap()).unwrap();
        assert!(ResubmitStateV1::load(&path).is_err());
        std::fs::write(&path, "not json").unwrap();
        assert!(ResubmitStateV1::load(&path).is_err());
        let mut damaged = s;
        damaged.tx_id = "abc".to_string();
        assert!(matches!(decide_resubmit_v1(&damaged, &all(1, &[("c", false)])), Err(ResubmitError::State(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
