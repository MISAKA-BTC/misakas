//! **The fork id — what a node has already crossed, and what it will cross next** (ADR-0072 SA-2).
//!
//! `consensus_identity_id` refuses a peer that disagrees about a rule in force NOW, and
//! `consensus_schedule_id` reports a peer that disagrees about a FUTURE fence without refusing it.
//! That pair is what makes a scheduled activation deployable at all (audit M1-6): two builds that
//! differ only in when a fence fires agree about every block either can produce today, so they must
//! stay peers for the whole rollout.
//!
//! It also leaves the hole this module closes, which the identity id's own doc names as its
//! residual: **at the fence, the disagreement stops being about the future.** The un-upgraded
//! minority keeps extending the old arm, refuses every block the majority produces under the new
//! one, and both handshakes still say the two are peers — because the identity is computed with
//! every fence normalised away, and normalising a fence cannot tell whether the chain has passed
//! it. Neither side sees a partition. It is a silent fork with a warning in the log at worst, and
//! this repository has already paid for one.
//!
//! The fork id is the missing term: the height. A node announces
//!
//! * the **genesis** it builds on,
//! * a digest of the **fences it has already crossed** at its own DAA score, and
//! * the **next fence** it expects to cross,
//!
//! and past a fence of its own it refuses a peer whose fired set is not its own schedule seen from
//! some point along it. The set is derived from [`Params::fence_schedule_v1`] and the node's DAA —
//! never from the peer's word about itself, which is an unauthenticated claim and would let a
//! stale node talk its way past the gate by announcing whatever the gate wanted to hear.
//!
//! ## Why "differs" is not "is not identical"
//!
//! A fresh node has crossed nothing. If "the fired set differs" meant refusal, a node past the
//! fence would refuse every node that still has to sync the pre-fence history — which is every new
//! node, forever, and which would make ADR-0072 SA-3's one-binary mixed history unreachable in
//! practice. So the comparison is against the SCHEDULE, not against the fired set:
//!
//! * a peer whose fired set is a **prefix** of this build's schedule, and whose next fence is the
//!   one that follows that prefix, is the same build seen from an earlier height — keep it;
//! * a peer whose fired set matches this build's schedule **entirely** has crossed everything this
//!   build knows about — keep it (it may know about fences this build does not; that is this build
//!   being out of date, and fork choice, not the handshake, is the instrument for it);
//! * anything else is a different schedule.
//!
//! An un-upgraded node lands in the third case precisely: its schedule is empty, so it announces
//! the empty fired set with **no** next fence, and the empty set matches this build's length-0
//! prefix while `u64::MAX` is not the fence that follows it. That is the whole gate — it separates
//! "behind" from "different" using the one field a stale-but-honest node always gets right.
//!
//! ## Why the gate only bites once a fence has fired
//!
//! Refusing a different schedule at ANY height would re-create the deploy-day partition M1-6 exists
//! to remove: the first operator to publish a build that arms a future fence would disconnect from
//! every un-upgraded peer immediately, for the whole rollout, with nothing about the fence height
//! involved. So before this node has crossed a fence of its own, a schedule difference is a
//! WARNING — the same verdict `consensus_schedule_id` already earns. At and after the first fence
//! it is a refusal, because from there the two builds disagree about blocks that exist.
//!
//! ## Why the gate is armed by a field and not by "does this build schedule anything"
//!
//! The obvious arming condition — refuse as soon as this build has crossed any fence at all — is
//! **not dormant, and the measurement says so**: three shipped presets already schedule a fence.
//! Mainnet schedules crescendo at `110_165_000` and testnet-10/-11 at `2_125_000`
//! (`the_shipped_schedules_are_measured_not_assumed` prints and pins them). Under that condition
//! this module would, the day a live testnet crossed `2_125_000`, start refusing every peer running
//! a build that predates the fork-id field — a peering change on a running network, shipped by a
//! commit whose whole premise is that it changes nothing until an operator arms it.
//!
//! So the gate is armed by [`fork_id_gate_armed_v1`], which today reads
//! `Params::palw_attempt_activation` — ADR-0072's fence, the one this module was written to make
//! survivable. The fork id is still ADVERTISED by every node from the moment this lands, because an
//! advertisement refuses nobody and a gate that has never been on the wire is a gate nobody has
//! tested. What is fenced is only the refusal.
//!
//! A later ADR arming a different fence must add it to that predicate, and the test
//! `the_gate_is_disarmed_on_every_shipped_preset` is what makes forgetting visible.

use crate::config::params::Params;
use kaspa_hashes::{ConsensusParamsId, Hash, Hash64};

/// Domain separator for the fired-fence digest. Versioned so a future encoding change is a
/// deliberate break rather than a silent one, exactly as `CONSENSUS_FINGERPRINT_DOMAIN_V1` is.
pub const FORK_ID_DOMAIN_V1: &[u8] = b"misaka/consensus-fork-id/v1";

/// The sentinel a node announces when it has no further fence to cross. Not a height any chain
/// reaches, and the same value `ForkActivation::never()` carries, so "no next fence" and "a fence
/// that never fires" are one answer rather than two.
pub const FORK_ID_NO_NEXT_FENCE: u64 = u64::MAX;

/// **What a node advertises at the handshake** (ADR-0072 SA-2).
///
/// The genesis is not carried here: it is already its own handshake field, compared before this
/// one, and it is hashed into [`Self::fired`] so a fired set cannot be replayed onto another chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForkIdV1 {
    /// Digest of `(genesis, the fence heights this node has already crossed, in order)`.
    pub fired: Hash,
    /// The lowest scheduled fence strictly above this node's DAA score, or
    /// [`FORK_ID_NO_NEXT_FENCE`].
    pub next: u64,
}

/// Why two fork ids are not the same schedule. Carried so the refusal names the case rather than
/// leaving an operator to infer it from two hex strings — the three have different operator
/// responses (upgrade the peer, upgrade this node, or check which build is which).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForkIdMismatch {
    /// The peer sent no fork id at all: a build that predates the field, and therefore one that
    /// cannot know about the fence this node has crossed.
    Absent,
    /// The peer's fired set is not this build's schedule seen from any point along it. Either it
    /// carries fences this build does not, or it crossed them at different heights.
    UnknownFiredSet,
    /// The peer's fired set IS a prefix of this build's schedule, but it expects a different fence
    /// next — so the two agree about the past and disagree about the very next rule change.
    NextFenceDiffers { expected: u64, got: u64 },
}

impl std::fmt::Display for ForkIdMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Absent => write!(f, "the peer sent no fork id (a build predating the fence)"),
            Self::UnknownFiredSet => {
                write!(f, "the peer's fired-fence set is not this build's schedule seen from any height along it")
            }
            Self::NextFenceDiffers { expected, got } => {
                write!(f, "the peer agrees about every fence crossed so far but expects fence {got} next, not {expected}")
            }
        }
    }
}

/// The verdict on a peer's fork id. Three outcomes rather than a bool, because "different" and
/// "refuse" are not the same question — see the module doc's last section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForkIdVerdict {
    /// The gate is not armed on this build (see [`fork_id_gate_armed_v1`]) or this build schedules
    /// no fence at all, so the fork id decides nothing here. Every shipped preset takes this arm,
    /// which is why adding the field changes no live network's peering.
    Unfenced,
    /// The peer runs this build's schedule, seen from some height along it.
    Agree,
    /// A different schedule, but this node has not crossed a fence yet: the two still agree about
    /// every block either can produce, so refusing would partition the network for the whole
    /// rollout (M1-6). Warn.
    DisagreeBeforeAnyFence(ForkIdMismatch),
    /// A different schedule and this node is already past `fired_through`: from here the two
    /// disagree about blocks that exist. Refuse.
    DisagreePastFence { mismatch: ForkIdMismatch, fired_through: u64 },
}

impl ForkIdVerdict {
    /// Is this a refusal? The one place the p2p layer asks, so the policy lives here rather than
    /// being re-derived at the call site.
    pub fn refuses(&self) -> bool {
        matches!(self, Self::DisagreePastFence { .. })
    }
}

/// **Is the fork-id gate armed on this build?**
///
/// `false` on every shipped preset, and therefore inert everywhere today — see the module doc's
/// last section for why this is a named field rather than "does the schedule contain anything",
/// and what the measurement was that made the difference matter.
///
/// A fence added later that needs the gate must be ORed in HERE, and only here.
pub fn fork_id_gate_armed_v1(params: &Params) -> bool {
    params.palw_attempt_activation.is_some()
}

/// The fence heights at or below `daa_score`, in order — what this node has crossed.
fn fired_at(schedule: &[u64], daa_score: u64) -> &[u64] {
    let crossed = schedule.partition_point(|&fence| fence <= daa_score);
    &schedule[..crossed]
}

/// `(genesis, fired fences)` as one digest.
///
/// The genesis is inside it so a fired set cannot be lifted onto another chain that happens to
/// schedule the same heights, and the count is written before the heights so no two distinct sets
/// can collide by concatenation — the same discipline `consensus_params_id` keeps.
pub fn fired_fences_digest_v1(genesis: Hash64, fired: &[u64]) -> Hash {
    let mut h = ConsensusParamsId::new();
    h.write(FORK_ID_DOMAIN_V1);
    h.write(genesis.as_byte_slice());
    h.write((fired.len() as u64).to_le_bytes());
    for fence in fired {
        h.write(fence.to_le_bytes());
    }
    h.finalize()
}

/// **What this node advertises at `daa_score`.**
pub fn fork_id_v1(params: &Params, daa_score: u64) -> ForkIdV1 {
    let schedule = params.fence_schedule_v1();
    let fired = fired_at(&schedule, daa_score);
    let next = schedule.get(fired.len()).copied().unwrap_or(FORK_ID_NO_NEXT_FENCE);
    ForkIdV1 { fired: fired_fences_digest_v1(params.genesis.hash, fired), next }
}

/// **The gate.** `peer_fired` is the peer's advertised digest as raw bytes (empty from a build that
/// predates the field); `peer_next` is its advertised next fence.
///
/// Nothing the peer says about ITSELF is trusted: the comparison is against digests this node
/// computes from its own `Params` and its own DAA score, and the peer's bytes are only ever
/// compared, never interpreted.
pub fn evaluate_fork_id_v1(params: &Params, local_daa_score: u64, peer_fired: &[u8], peer_next: u64) -> ForkIdVerdict {
    if !fork_id_gate_armed_v1(params) {
        return ForkIdVerdict::Unfenced;
    }
    let schedule = params.fence_schedule_v1();
    if schedule.is_empty() {
        return ForkIdVerdict::Unfenced;
    }
    let local_fired = fired_at(&schedule, local_daa_score);
    // The verdict on a difference depends only on whether THIS node has crossed anything.
    let classify = |mismatch: ForkIdMismatch| match local_fired.last() {
        Some(&fired_through) => ForkIdVerdict::DisagreePastFence { mismatch, fired_through },
        None => ForkIdVerdict::DisagreeBeforeAnyFence(mismatch),
    };

    if peer_fired.is_empty() {
        return classify(ForkIdMismatch::Absent);
    }

    // Which prefix of this build's schedule is the peer claiming? Computed rather than read: the
    // peer sends a digest, and a digest can only be recognised by recomputing the sets this node
    // is willing to accept. `0..=len` because "crossed nothing" and "crossed everything" are both
    // legitimate positions on one schedule.
    let genesis = params.genesis.hash;
    let matched = (0..=schedule.len()).find(|&k| fired_fences_digest_v1(genesis, &schedule[..k]).as_bytes() == peer_fired);

    let Some(k) = matched else {
        return classify(ForkIdMismatch::UnknownFiredSet);
    };

    match schedule.get(k) {
        // The peer is somewhere along this schedule with a fence still ahead of it. That fence is
        // the one term a stale-but-honest node still gets right, and an un-upgraded node cannot:
        // it is the rule the peer has not reached, not the history it has.
        Some(&expected) if peer_next != expected => classify(ForkIdMismatch::NextFenceDiffers { expected, got: peer_next }),
        // Either the next fence agrees, or the peer has crossed this build's whole schedule and is
        // announcing a fence this build does not carry. The second is this node being out of date,
        // which is a fork-choice question and not a reason to refuse a peer.
        _ => ForkIdVerdict::Agree,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{ForkActivation, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS};
    use crate::network::NetworkId;

    fn shipped() -> Vec<(&'static str, Params)> {
        vec![
            ("mainnet", Params::from(NetworkId::new(crate::network::NetworkType::Mainnet))),
            ("testnet-10", Params::from(NetworkId::with_suffix(crate::network::NetworkType::Testnet, 10))),
            ("testnet-11", Params::from(NetworkId::with_suffix(crate::network::NetworkType::Testnet, 11))),
            ("devnet", Params::from(NetworkId::new(crate::network::NetworkType::Devnet))),
            ("simnet", Params::from(NetworkId::new(crate::network::NetworkType::Simnet))),
        ]
    }

    /// **The shipped schedules, measured rather than assumed** — and the measurement is the reason
    /// this module has an arming field at all.
    ///
    /// The first draft of `the_gate_is_disarmed_on_every_shipped_preset` asserted that no shipped
    /// preset schedules a fence, and three of them do. Crescendo is a real scheduled activation on
    /// mainnet and on both testnets, so "this node has crossed a fence" is already true of a live
    /// network at a height it will actually reach — which would have made a module that changes
    /// nothing today start refusing peers at testnet DAA 2,125,000.
    ///
    /// Pinned, so that a preset gaining or losing a fence is a decision someone takes here.
    #[test]
    fn the_shipped_schedules_are_measured_not_assumed() {
        let measured: Vec<(&str, Vec<u64>)> = shipped().into_iter().map(|(n, p)| (n, p.fence_schedule_v1())).collect();
        assert_eq!(
            measured,
            vec![
                ("mainnet", vec![110_165_000]),
                ("testnet-10", vec![2_125_000]),
                ("testnet-11", vec![2_125_000]),
                ("devnet", vec![]),
                ("simnet", vec![]),
            ],
            "a shipped preset's fence schedule moved — read the module doc before deciding this is fine"
        );
    }

    /// **The gate is disarmed on every shipped preset, so it cannot refuse anybody today.**
    ///
    /// This is the claim that makes the module safe to land on a running network: `evaluate` takes
    /// the `Unfenced` arm before it compares anything, at every height, for every peer — including
    /// the un-upgraded one it exists to refuse. Arming it is `palw_attempt_activation`, which is
    /// `None` everywhere, and that is a coordinated flag day rather than a default.
    #[test]
    fn the_gate_is_disarmed_on_every_shipped_preset() {
        for (name, params) in shipped() {
            assert!(!fork_id_gate_armed_v1(&params), "{name}: a shipped preset must not arm the fork-id gate");
            // Past crescendo included: the whole point is that crossing a PRE-EXISTING fence must
            // not turn the gate on.
            for daa in [0u64, 1, 2_125_000, 110_165_000, u64::MAX] {
                for (peer_fired, peer_next) in [(&[][..], FORK_ID_NO_NEXT_FENCE), (&[0u8; 32][..], 7)] {
                    let verdict = evaluate_fork_id_v1(&params, daa, peer_fired, peer_next);
                    assert_eq!(verdict, ForkIdVerdict::Unfenced, "{name} at {daa}");
                    assert!(!verdict.refuses(), "{name} at {daa}");
                }
            }
        }
    }

    /// The advertisement ships on day one even though the gate does not: a node past crescendo
    /// announces that it has crossed it, so an operator can SEE the two sides differ before any
    /// build is armed to act on it.
    #[test]
    fn the_advertisement_is_live_even_where_the_gate_is_not() {
        let mainnet = Params::from(NetworkId::new(crate::network::NetworkType::Mainnet));
        assert!(!fork_id_gate_armed_v1(&mainnet));
        let before = fork_id_v1(&mainnet, 110_164_999);
        let after = fork_id_v1(&mainnet, 110_165_000);
        assert_eq!(before.next, 110_165_000);
        assert_eq!(after.next, FORK_ID_NO_NEXT_FENCE);
        assert_ne!(before.fired, after.fired, "crossing a fence must change what a node advertises");
    }

    /// **A fence active at genesis is not on the schedule, and `never()` is not either.**
    ///
    /// The first is `consensus_identity_id`'s business (re-audit R-1) and the second is not a
    /// height. Either one leaking into the schedule would make the fork id claim two builds "cross"
    /// something they never do.
    #[test]
    fn the_schedule_carries_heights_a_chain_can_actually_reach() {
        // The raw const, not `Params::from` — and crescendo is on it, so the assertions below are
        // about what ADDING a fence does rather than about the schedule being empty.
        let base = MAINNET_PARAMS.fence_schedule_v1();
        assert_eq!(base, vec![110_165_000]);

        let mut params = MAINNET_PARAMS;
        params.palw_attempt_activation = Some(ForkActivation::always());
        assert_eq!(params.fence_schedule_v1(), base, "a genesis-active fence is a rule about block 1, not a schedule");

        params.palw_attempt_activation = Some(ForkActivation::never());
        assert_eq!(params.fence_schedule_v1(), base, "`never()` is not a height");

        params.palw_attempt_activation = Some(ForkActivation::new(9_000_000));
        assert_eq!(params.fence_schedule_v1(), vec![9_000_000, 110_165_000], "sorted, whatever order the visitor ran in");
    }

    /// A network with exactly one scheduled fence — ADR-0072's — so the tests below read as the
    /// activation they are about. Crescendo is cleared for the same reason: two fences would test
    /// two things at once.
    fn armed_at(height: u64) -> Params {
        let mut params = MAINNET_PARAMS;
        params.crescendo_activation = ForkActivation::never();
        params.palw_attempt_activation = Some(ForkActivation::new(height));
        assert_eq!(params.fence_schedule_v1(), vec![height]);
        params
    }

    /// **The rollout survives: the same build at two heights is one network.**
    ///
    /// A node past the fence and a node still syncing towards it announce different fired sets, and
    /// they must stay peers — otherwise the fence makes IBD impossible and SA-3's one-binary mixed
    /// history is unreachable in practice.
    #[test]
    fn a_peer_that_is_merely_behind_the_fence_is_kept() {
        let params = armed_at(1_000);
        let ahead = fork_id_v1(&params, 5_000);
        let behind = fork_id_v1(&params, 10);
        assert_ne!(ahead.fired, behind.fired, "the two really are at different points on the schedule");
        assert_eq!(behind.next, 1_000, "a node below the fence names the fence it has not crossed");
        assert_eq!(ahead.next, FORK_ID_NO_NEXT_FENCE, "a node past the last fence has nothing left to cross");

        // Each side judges the other.
        assert_eq!(evaluate_fork_id_v1(&params, 5_000, behind.fired.as_bytes().as_slice(), behind.next), ForkIdVerdict::Agree);
        assert_eq!(evaluate_fork_id_v1(&params, 10, ahead.fired.as_bytes().as_slice(), ahead.next), ForkIdVerdict::Agree);
    }

    /// **The silent fork this exists for: an un-upgraded peer past the fence is refused.**
    ///
    /// The un-upgraded build has no fence, so it announces the empty fired set with no next fence.
    /// The empty set is a legitimate prefix of the armed schedule — which is exactly why the NEXT
    /// fence has to be in the message. Without it, "I have crossed nothing" from a node that will
    /// never cross anything is indistinguishable from "I have crossed nothing yet".
    #[test]
    fn an_un_upgraded_peer_is_refused_once_this_node_is_past_the_fence() {
        let armed = armed_at(1_000);
        // The un-upgraded build: same network, same genesis, no ADR-0072 fence.
        let mut un_upgraded = MAINNET_PARAMS;
        un_upgraded.crescendo_activation = ForkActivation::never();
        assert!(un_upgraded.fence_schedule_v1().is_empty());
        let stale = fork_id_v1(&un_upgraded, 5_000);
        assert_eq!(stale.next, FORK_ID_NO_NEXT_FENCE);

        let verdict = evaluate_fork_id_v1(&armed, 5_000, stale.fired.as_bytes().as_slice(), stale.next);
        assert_eq!(
            verdict,
            ForkIdVerdict::DisagreePastFence {
                mismatch: ForkIdMismatch::NextFenceDiffers { expected: 1_000, got: FORK_ID_NO_NEXT_FENCE },
                fired_through: 1_000
            }
        );
        assert!(verdict.refuses(), "past the fence, an un-upgraded peer must not become an IBD source");

        // And the same peer BEFORE the fence fires is only a warning — otherwise arming a future
        // fence would disconnect the first upgrading operator from the whole network (M1-6).
        let early = evaluate_fork_id_v1(&armed, 999, stale.fired.as_bytes().as_slice(), stale.next);
        assert_eq!(
            early,
            ForkIdVerdict::DisagreeBeforeAnyFence(ForkIdMismatch::NextFenceDiffers { expected: 1_000, got: FORK_ID_NO_NEXT_FENCE })
        );
        assert!(!early.refuses(), "a scheduled fence must not partition the network on deploy day");
    }

    /// A build that predates the field sends nothing, and nothing is not a fired set.
    #[test]
    fn an_absent_fork_id_is_a_mismatch_named_as_one() {
        let armed = armed_at(1_000);
        assert_eq!(
            evaluate_fork_id_v1(&armed, 5_000, &[], 0),
            ForkIdVerdict::DisagreePastFence { mismatch: ForkIdMismatch::Absent, fired_through: 1_000 }
        );
        assert_eq!(evaluate_fork_id_v1(&armed, 10, &[], 0), ForkIdVerdict::DisagreeBeforeAnyFence(ForkIdMismatch::Absent));
    }

    /// **Two builds arming one rule at DIFFERENT heights are a fork, and the fork id says so.**
    ///
    /// This is the residual `consensus_identity_id`'s own doc records as beyond its reach: both
    /// builds normalise their fence away, produce one identity, and peer. Past either height they
    /// disagree about blocks that exist.
    #[test]
    fn two_builds_arming_one_fence_at_different_heights_are_refused_past_it() {
        let ours = armed_at(1_000);
        let theirs = armed_at(2_000);
        assert_eq!(
            ours.consensus_identity_id(),
            theirs.consensus_identity_id(),
            "the identity id cannot see this difference — that is why the fork id exists"
        );

        // Their node is past ITS fence; ours is past OURS.
        let peer = fork_id_v1(&theirs, 3_000);
        let verdict = evaluate_fork_id_v1(&ours, 3_000, peer.fired.as_bytes().as_slice(), peer.next);
        assert_eq!(verdict, ForkIdVerdict::DisagreePastFence { mismatch: ForkIdMismatch::UnknownFiredSet, fired_through: 1_000 });
        assert!(verdict.refuses());
    }

    /// **The fired set is bound to the chain it was fired on.**
    ///
    /// Two networks scheduling the same heights must not accept each other's fork ids; the genesis
    /// inside the digest is what stops it. (The handshake compares genesis separately too — this is
    /// the belt to that pair of braces, and it costs one hash input.)
    #[test]
    fn a_fired_set_from_another_genesis_does_not_match() {
        let mut ours = armed_at(1_000);
        let mut theirs = armed_at(1_000);
        theirs.genesis = SIMNET_PARAMS.genesis;
        assert_ne!(ours.genesis.hash, theirs.genesis.hash);

        let peer = fork_id_v1(&theirs, 5_000);
        assert!(evaluate_fork_id_v1(&ours, 5_000, peer.fired.as_bytes().as_slice(), peer.next).refuses());

        // Same genesis, same schedule: agree. (Guards against the test above passing for the wrong
        // reason.)
        ours.genesis = SIMNET_PARAMS.genesis;
        assert_eq!(evaluate_fork_id_v1(&ours, 5_000, peer.fired.as_bytes().as_slice(), peer.next), ForkIdVerdict::Agree);
    }

    /// **A peer that has crossed a fence this build does not carry is kept, not refused.**
    ///
    /// It means this node is the out-of-date one. Refusing there would let a stale minority
    /// disconnect itself from the upgrade it needs to receive, and which chain wins is fork
    /// choice's question, not the handshake's.
    #[test]
    fn a_peer_ahead_of_this_builds_whole_schedule_is_kept() {
        let ours = armed_at(1_000);
        let mut theirs = armed_at(1_000);
        theirs.pq_activation_daa_score = 5_000; // a second fence, only they know about
        assert_eq!(theirs.fence_schedule_v1(), vec![1_000, 5_000]);

        let peer = fork_id_v1(&theirs, 9_000); // crossed both
        // Their fired set [1000, 5000] is NOT a prefix of ours — ours is [1000] and stops there.
        assert!(evaluate_fork_id_v1(&ours, 9_000, peer.fired.as_bytes().as_slice(), peer.next).refuses());

        // But before their extra fence fires, they announce exactly our schedule with a next fence
        // we do not carry — and that is the M1-6 case: keep the peer.
        let peer_early = fork_id_v1(&theirs, 3_000);
        assert_eq!(peer_early.next, 5_000);
        assert_eq!(evaluate_fork_id_v1(&ours, 3_000, peer_early.fired.as_bytes().as_slice(), peer_early.next), ForkIdVerdict::Agree);
    }

    /// **The peer's word about itself decides nothing.**
    ///
    /// A stale node that simply echoes the digest an upgraded node would send must not thereby pass
    /// — but the point of the test is the converse and it is the one that matters: the ACCEPTING
    /// side computes every digest it is willing to match from its own params and its own DAA, so
    /// there is no field a peer can set to make the gate compute a different set.
    #[test]
    fn the_accepted_sets_are_derived_locally_not_read_from_the_peer() {
        let ours = armed_at(1_000);
        // A digest of a set nobody scheduled.
        let invented = fired_fences_digest_v1(ours.genesis.hash, &[7, 8, 9]);
        assert!(evaluate_fork_id_v1(&ours, 5_000, invented.as_bytes().as_slice(), FORK_ID_NO_NEXT_FENCE).refuses());
        // Garbage of the right length is refused for the same reason, and shorter/longer garbage
        // never reaches a comparison at all (the p2p boundary bounds the field first).
        assert!(evaluate_fork_id_v1(&ours, 5_000, &[0u8; 32], 1_000).refuses());
        assert!(evaluate_fork_id_v1(&ours, 5_000, &[0xffu8; 8], 1_000).refuses());
    }

    /// Distinct sets do not collide, including the pair a length-free encoding would confuse.
    #[test]
    fn the_digest_separates_sets_that_a_bare_concatenation_would_not() {
        let g = TESTNET_PARAMS.genesis.hash;
        assert_ne!(fired_fences_digest_v1(g, &[]), fired_fences_digest_v1(g, &[0]));
        assert_ne!(fired_fences_digest_v1(g, &[1, 2]), fired_fences_digest_v1(g, &[1]));
        // `[0x0102…]` vs `[0x01, 0x02…]`: the length prefix is what keeps these apart.
        assert_ne!(fired_fences_digest_v1(g, &[1, 2]), fired_fences_digest_v1(g, &[(2u64 << 32) | 1]));
        assert_eq!(fired_fences_digest_v1(g, &[1, 2]), fired_fences_digest_v1(g, &[1, 2]), "and it is a function");
    }
}
