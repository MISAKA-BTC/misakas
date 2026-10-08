//! Following a claim from "we sent bytes" to `Final` or void — across reorgs.
//!
//! Four different things are easy to confuse and are tracked separately: the **tx id** (what we sent), the **claim id** (what the
//! chain will call it), **inclusion** (a block, which can be reorged out), and the **claim phase** (licence, challenge window,
//! `Final`, void — each a fact about the selected chain at the observation's sink). Nothing is terminal on a single sighting: a
//! phase seen at the sink is reported as *settled* only once it is `finality_depth` DAA deep, and every poll re-derives the state
//! from scratch, so a reorg simply makes the next observation say something else and the tracker walks **backwards**.
//!
//! **Attribution is the point of the exercise.** A relay or a stale view must never leave the miner believing a claim is its own
//! when the chain's row names another executor bond. [`TrackState::Misattributed`] is the loud answer.

use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimPhaseObs {
    Provisional,
    PanelBound,
    ReceiptLicensed {
        licensed_daa: u64,
    },
    /// The challenge window is open and ends at `ends_daa` (derived from the chain's params by the adapter).
    Challengeable {
        ends_daa: u64,
    },
    Final {
        final_daa: u64,
    },
    Voided {
        voided_daa: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimObs {
    /// The executor bond the CHAIN's claim row names.
    pub executor_bond: TransactionOutpoint,
    pub accepted_block: Hash64,
    pub accepted_daa: u64,
    pub phase: ClaimPhaseObs,
}

/// One poll: what the selected chain at `sink` says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainObservation {
    pub sink: Hash64,
    pub virtual_daa: u64,
    pub tx_in_mempool: bool,
    /// `None` = the claim does not exist at this sink.
    pub claim: Option<ClaimObs>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackState {
    /// Signed, not yet sent (or sent and not yet seen anywhere).
    Built,
    /// A node holds it (mempool) but no block does.
    Relayed,
    /// Neither mempool nor chain after having been seen: it was dropped, or reorged out — re-broadcast the same bytes.
    NeedsRebroadcast,
    Included {
        block: Hash64,
        daa: u64,
    },
    Licensed {
        licensed_daa: u64,
    },
    ChallengeWindow {
        ends_daa: u64,
    },
    /// `Final` at the sink but shallower than the finality depth: still reversible.
    FinalPending {
        final_daa: u64,
    },
    Final {
        final_daa: u64,
    },
    VoidPending {
        voided_daa: u64,
    },
    Void {
        voided_daa: u64,
    },
    /// The chain's claim row names a different executor bond. Terminal and loud; never "ours".
    Misattributed {
        chain_bond: TransactionOutpoint,
    },
}

impl TrackState {
    pub fn is_settled(&self) -> bool {
        matches!(self, TrackState::Final { .. } | TrackState::Void { .. } | TrackState::Misattributed { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    pub at_sink: Hash64,
    pub at_daa: u64,
    pub from: TrackState,
    pub to: TrackState,
}

#[derive(Clone, Debug)]
pub struct ClaimTracker {
    pub tx_id: Hash64,
    pub claim_id: Hash64,
    pub our_bond: TransactionOutpoint,
    pub finality_depth: u64,
    state: TrackState,
    ever_seen: bool,
    history: Vec<Transition>,
    /// `(DAA the relay ACKed at, bound)`: an ACK no observer ever confirms within the bound is a swallowed carrier (RFC-0009 mandatory test
    /// 2, "obstruct"), and the SAME bytes go out through another relay.
    relayed: Option<(u64, u64)>,
}

impl ClaimTracker {
    pub fn new(tx_id: Hash64, claim_id: Hash64, our_bond: TransactionOutpoint, finality_depth: u64) -> Self {
        Self {
            tx_id,
            claim_id,
            our_bond,
            finality_depth,
            state: TrackState::Built,
            ever_seen: false,
            history: Vec::new(),
            relayed: None,
        }
    }

    /// **A relay ACKed at `at_daa`.** An ACK is the relay's word: if within `bound_daa` no observer (a node other than the one that ACKed)
    /// shows the carrier in its mempool or the claim on chain, the tracker asks for the same bytes again — through ANOTHER relay.
    pub fn relayed_at(&mut self, at_daa: u64, bound_daa: u64) {
        self.relayed = Some((at_daa, bound_daa));
    }

    pub fn state(&self) -> &TrackState {
        &self.state
    }

    pub fn history(&self) -> &[Transition] {
        &self.history
    }

    /// Had the chain ever shown this tracker anything beyond "built"? (A claim that regresses to a state it already passed is a reorg.)
    pub fn reorgs_seen(&self) -> usize {
        fn rank(s: &TrackState) -> u8 {
            match s {
                TrackState::Built => 0,
                TrackState::NeedsRebroadcast => 1,
                TrackState::Relayed => 2,
                TrackState::Included { .. } => 3,
                TrackState::Licensed { .. } => 4,
                TrackState::ChallengeWindow { .. } => 5,
                TrackState::FinalPending { .. } | TrackState::VoidPending { .. } => 6,
                TrackState::Final { .. } | TrackState::Void { .. } | TrackState::Misattributed { .. } => 7,
            }
        }
        self.history.iter().filter(|t| rank(&t.to) < rank(&t.from) && !matches!(t.to, TrackState::Misattributed { .. })).count()
    }

    /// Fold one observation. Every call re-derives the state from the observation alone (plus "have we ever seen it"), so there is no
    /// path along which a stale sighting survives a reorg.
    pub fn observe(&mut self, obs: &ChainObservation) -> &TrackState {
        let next = self.derive(obs);
        if next != self.state {
            self.history.push(Transition { at_sink: obs.sink, at_daa: obs.virtual_daa, from: self.state.clone(), to: next.clone() });
            self.state = next;
        }
        &self.state
    }

    fn derive(&mut self, obs: &ChainObservation) -> TrackState {
        use ClaimPhaseObs as P;
        let Some(claim) = &obs.claim else {
            return if obs.tx_in_mempool {
                self.ever_seen = true;
                TrackState::Relayed
            } else if self.ever_seen {
                TrackState::NeedsRebroadcast
            } else if self.relayed.is_some_and(|(at, bound)| obs.virtual_daa >= at.saturating_add(bound)) {
                // ACKed, and nobody has seen it since: the relay swallowed it.
                TrackState::NeedsRebroadcast
            } else {
                TrackState::Built
            };
        };
        self.ever_seen = true;
        if claim.executor_bond != self.our_bond {
            return TrackState::Misattributed { chain_bond: claim.executor_bond };
        }
        let deep = |daa: u64| obs.virtual_daa.saturating_sub(daa) >= self.finality_depth;
        match claim.phase {
            P::Provisional | P::PanelBound => TrackState::Included { block: claim.accepted_block, daa: claim.accepted_daa },
            P::ReceiptLicensed { licensed_daa } => TrackState::Licensed { licensed_daa },
            P::Challengeable { ends_daa } => TrackState::ChallengeWindow { ends_daa },
            P::Final { final_daa } if deep(final_daa) => TrackState::Final { final_daa },
            P::Final { final_daa } => TrackState::FinalPending { final_daa },
            P::Voided { voided_daa } if deep(voided_daa) => TrackState::Void { voided_daa },
            P::Voided { voided_daa } => TrackState::VoidPending { voided_daa },
        }
    }

    /// Whether the same signed bytes should be sent again (idempotent): the claim left the chain and no node holds the tx.
    pub fn needs_rebroadcast(&self) -> bool {
        matches!(self.state, TrackState::NeedsRebroadcast)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn bond(n: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(h(n), 0)
    }
    fn tracker() -> ClaimTracker {
        ClaimTracker::new(h(1), h(2), bond(10), 60)
    }
    fn obs(sink: u8, daa: u64, mempool: bool, claim: Option<ClaimObs>) -> ChainObservation {
        ChainObservation { sink: h(sink), virtual_daa: daa, tx_in_mempool: mempool, claim }
    }
    fn claim(b: u8, phase: ClaimPhaseObs) -> Option<ClaimObs> {
        Some(ClaimObs { executor_bond: bond(b), accepted_block: h(0xB1), accepted_daa: 100, phase })
    }

    #[test]
    fn a_claim_walks_from_relayed_to_final_and_final_waits_for_depth() {
        let mut t = tracker();
        assert_eq!(t.observe(&obs(1, 90, false, None)), &TrackState::Built);
        assert_eq!(t.observe(&obs(2, 95, true, None)), &TrackState::Relayed);
        assert!(matches!(t.observe(&obs(3, 101, false, claim(10, ClaimPhaseObs::Provisional))), TrackState::Included { .. }));
        assert!(matches!(
            t.observe(&obs(4, 150, false, claim(10, ClaimPhaseObs::ReceiptLicensed { licensed_daa: 140 }))),
            TrackState::Licensed { .. }
        ));
        assert!(matches!(
            t.observe(&obs(5, 200, false, claim(10, ClaimPhaseObs::Challengeable { ends_daa: 260 }))),
            TrackState::ChallengeWindow { .. }
        ));
        assert_eq!(
            t.observe(&obs(6, 270, false, claim(10, ClaimPhaseObs::Final { final_daa: 265 }))),
            &TrackState::FinalPending { final_daa: 265 }
        );
        assert!(!t.state().is_settled());
        assert_eq!(
            t.observe(&obs(7, 330, false, claim(10, ClaimPhaseObs::Final { final_daa: 265 }))),
            &TrackState::Final { final_daa: 265 }
        );
        assert!(t.state().is_settled());
        assert_eq!(t.reorgs_seen(), 0);
    }

    #[test]
    fn a_reorg_walks_the_tracker_backwards_and_asks_for_a_rebroadcast_of_the_same_bytes() {
        let mut t = tracker();
        t.observe(&obs(1, 101, false, claim(10, ClaimPhaseObs::Provisional)));
        // The including block is reorged out; the tx is in nobody's mempool.
        assert_eq!(t.observe(&obs(9, 103, false, None)), &TrackState::NeedsRebroadcast);
        assert!(t.needs_rebroadcast());
        // A node takes it back into its mempool, then it is included again on the new chain.
        assert_eq!(t.observe(&obs(9, 104, true, None)), &TrackState::Relayed);
        assert!(matches!(t.observe(&obs(10, 106, false, claim(10, ClaimPhaseObs::Provisional))), TrackState::Included { .. }));
        assert_eq!(t.reorgs_seen(), 1, "included→rebroadcast is the regression; rebroadcast→relayed is progress");
    }

    #[test]
    fn a_pending_final_that_a_reorg_voids_is_reversible_and_never_reported_settled() {
        let mut t = tracker();
        t.observe(&obs(1, 270, false, claim(10, ClaimPhaseObs::Final { final_daa: 265 })));
        assert!(matches!(t.state(), TrackState::FinalPending { .. }));
        t.observe(&obs(2, 280, false, claim(10, ClaimPhaseObs::Voided { voided_daa: 275 })));
        assert!(matches!(t.state(), TrackState::VoidPending { .. }));
        assert!(!t.state().is_settled());
    }

    #[test]
    fn a_claim_row_naming_another_executor_is_never_ours() {
        let mut t = tracker();
        let s = t.observe(&obs(1, 101, false, claim(77, ClaimPhaseObs::Final { final_daa: 50 })));
        assert_eq!(s, &TrackState::Misattributed { chain_bond: bond(77) });
        assert!(t.state().is_settled());
        // A later observation on a reorged chain where the claim is ours again corrects it — attribution is re-derived each poll.
        t.observe(&obs(2, 120, false, claim(10, ClaimPhaseObs::Provisional)));
        assert!(matches!(t.state(), TrackState::Included { .. }));
    }

    #[test]
    fn a_tx_that_was_never_seen_stays_built_it_is_not_a_drop() {
        let mut t = tracker();
        assert_eq!(t.observe(&obs(1, 90, false, None)), &TrackState::Built);
        assert!(!t.needs_rebroadcast());
    }

    /// **RFC-0009 mandatory test 2, "obstruct": a relay that ACKs and never forwards.** The tracker sees nothing within the bound, asks for
    /// the same bytes again, and they go out through ANOTHER relay: the same tx id (so no second fee is charged — the first relay never
    /// forwarded the first copy, and an honest node that did hold it answers "already known"), the same claim id (the claim is a function of
    /// the signed bytes), one claim on the chain.
    #[test]
    fn a_relay_that_swallows_the_carrier_is_routed_around_with_the_same_bytes() {
        use crate::relay::fake::{FakeRelay, Mode};
        use crate::relay::{RelayNode, Reply, broadcast_signed_tx, tx_id_of_bytes};
        use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
        use kaspa_consensus_core::tx::Transaction;
        let tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![7, 7, 7]);
        // The obstructing relay: it ACKs with the right id and forwards nothing.
        struct Swallow(std::cell::RefCell<u32>);
        impl RelayNode for Swallow {
            fn node_id(&self) -> &str {
                "swallow"
            }
            fn submit_raw_tx(&self, tx: &Transaction) -> Reply {
                *self.0.borrow_mut() += 1;
                Reply::Accepted(tx.id())
            }
        }
        let swallow = Swallow(std::cell::RefCell::new(0));
        let first = broadcast_signed_tx(&tx, None, &[&swallow as &dyn RelayNode], 1).expect("the ACK looks like success");
        assert_eq!(first.successes, 1);
        let mut t = ClaimTracker::new(tx_id_of_bytes(&tx), h(2), bond(10), 60);
        t.relayed_at(100, 10);
        // Observers (other nodes) never see it: inside the bound it is merely not yet seen; past it, swallowed.
        assert_eq!(t.observe(&obs(1, 105, false, None)), &TrackState::Built);
        assert_eq!(t.observe(&obs(1, 110, false, None)), &TrackState::NeedsRebroadcast);
        assert!(t.needs_rebroadcast());
        // The same bytes through another relay; an honest node that already held them would answer "already known" — never a new claim.
        let other = FakeRelay::new("other", Mode::Honest);
        let again = broadcast_signed_tx(&tx, None, &[&other as &dyn RelayNode], 1).unwrap();
        assert_eq!(again.successes, 1);
        assert_eq!(other.sent.borrow()[0].id(), tx.id(), "the SAME transaction, not a re-signed one: no second fee, no second claim");
        let twice = broadcast_signed_tx(&tx, None, &[&other as &dyn RelayNode], 1).unwrap();
        assert!(matches!(twice.per_node[0].1, crate::relay::NodeOutcome::AlreadyKnown), "a resend is idempotent");
        t.relayed_at(110, 10);
        assert_eq!(t.observe(&obs(2, 112, true, None)), &TrackState::Relayed);
        assert!(matches!(t.observe(&obs(3, 115, false, claim(10, ClaimPhaseObs::Provisional))), TrackState::Included { .. }));
        assert_eq!(*swallow.0.borrow(), 1, "the obstructing relay is not asked again");
    }
}
