//! **The held leaf challenge, filed from a node** (RFC-0003 decision 22, spec 04b §15.15.6 and §15.15.8).
//!
//! Under the held regime (no bisection) the one-move accusation carries the whole close in ONE carrier, so a
//! lie at a leaf whose close does not fit one is licence-refused and never slashed (G24). Once
//! `palw_held_close_chunks_v1` is armed the node files such a close the way the court carries one that does not
//! fit: a signed **named-leaf challenge that declares the close** (`HeldLeafChallengeDeclared`, tag 90), then the
//! close's **chunks** (`CourtCloseChunk`, challenger side) once the declared group stands on the chain. This
//! module is the node's half and it is generic: an IR class's one-move pass and a pipeline class's both hand it
//! the close they built and the leaf it convicts at.
//!
//! * [`plan_held_leaf_challenge_v1`] — pure: the close cut by the court's own cut
//!   ([`PALW_COURT_CLOSE_CHUNK_MAX_BYTES`]), the declaration over its digests, signed over the challenge digest,
//!   every shape the acceptance layer and the carrier gate will ask asked first, and the court's two carried
//!   limits (the ruleset's chunk count, an assembly clock that ends inside the session's backstop) checked
//!   BEFORE a fee is spent.
//! * [`PalwPanelService::file_held_leaf_challenge_v1`] — plans, asks the fold what it makes of the declaration
//!   at the tip (`palw_object_rehearsal_v1`: never pay a carrier the fold refuses), queues it on the court's
//!   priority lane and remembers the chunks. A restart finds the group already standing under this bond and
//!   the same close digest and resumes delivering what is missing.
//! * [`PalwPanelService::held_chunks_tick_v1`] — once a tick: the chunks the chain has not received go onto the
//!   queue, each at its own round (the queue sorts a session's items by round, so the declaration is carried
//!   before its chunks and the carriers chain in that order), due by the group's assembly deadline; an entry
//!   leaves when its group is gone (the completing chunk applied the close, or the sweep convicted the
//!   declarer) or when its declaration never landed.
//!
//! The clock this is racing is the group's own: `4 · count` DAA from the block that carried the declaration, at
//! one carrier a block under the node's one-in-flight rule. The plan refuses a close whose assembly window
//! would not fit the session (`CourtCloseCannotAssemble`) before it starts. A close that never completes
//! convicts its declarer — this node's accuser bond — and leaves the claim alone, which is why the planner is
//! strict and the tick is patient only for what the chain has not yet seen.

use std::collections::HashMap;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_held_close_v1::{
    PALW_HELD_LEAF_CHALLENGE_MLDSA87_CONTEXT_V1, PALW_HELD_LEAF_CHALLENGE_VERSION_V1, PalwHeldLeafChallengeV1,
    palw_held_leaf_challenge_digest_v1, palw_held_leaf_challenge_session_id_v1, palw_held_leaf_challenge_shape_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_producer_v2::{PalwDisputableClaimV2, PalwObjectRehearsalV1};
use kaspa_consensus_core::palw_state_v2::{
    PALW_COURT_CLOSE_CHUNK_MAX_BYTES, PALW_COURT_CLOSE_MAX_CHUNKS, PalwBondKeyV2, PalwConsensusObjectV2, PalwCourtSideV1,
    PalwCourtVerdictV2, palw_close_assembly_daa_v1, palw_court_close_chunk_digest_v1,
};
use kaspa_core::{info, warn};

use super::{COURT_MOVE_REPLAN_DAA, PALW_PANEL};

/// How long a declaration that has not been seen on the chain is waited for, from the tick that queued it. A
/// declaration is one carrier; past this it was refused (the fold or the mempool said no) or lost, and the claim
/// is recorded, not retried — the next one is a decision for the next run.
pub(super) const PALW_HELD_DECLARATION_PATIENCE_DAA_V1: u64 = 200;

/// **One challenge this node is carrying** (see the module doc).
#[derive(Clone, Debug)]
pub(super) struct PalwHeldChunksV1 {
    pub claim: Hash64,
    pub session_id: Hash64,
    /// The digest the declaration pinned: the chain's group is ours only if it carries it.
    pub close_digest: Hash64,
    /// The close's chunks, in index order.
    pub chunks: Vec<Vec<u8>>,
    /// The DAA the declaration was queued (or the group found), for the patience.
    pub queued_daa: u64,
    /// Whether the group has been seen standing: once it was and is gone, the entry is done.
    pub group_seen: bool,
}

/// What the planner reads of the chain and the claim.
pub(super) struct PalwHeldChallengeInputsV1<'a> {
    pub claim: &'a PalwDisputableClaimV2,
    /// The accuser: this node's bond, the session's challenger and the group's declarer.
    pub accuser: PalwBondKeyV2,
    /// The global step leaf the close convicts at.
    pub leaf: u64,
    /// The claim's class ladder as the chain reads it (`class_step_ladder_v1`: the network's for an IR or a
    /// pipeline class, which record none): the session id derives from it.
    pub class_ladder: u64,
    pub court: &'a PalwCourtParamsV2,
    /// The court window: the session's backstop is `daa + window_court`, and the group's assembly clock must end
    /// inside it.
    pub window_court: u64,
    /// The network domain the challenge digest is taken under (the acceptance layer's, `palw_network_domain_v2_for`).
    pub network_domain: Hash64,
}

/// **A challenge, planned**: the declaration to carry first, the chunks to follow it, the session they belong to.
#[derive(Clone, Debug)]
pub(super) struct PalwHeldChallengePlanV1 {
    pub session_id: Hash64,
    pub declaration: PalwConsensusObjectV2,
    pub chunks: Vec<Vec<u8>>,
    pub close_digest: Hash64,
}

/// **The close a proof makes, for the session a challenge opens**: the court's own object
/// (`CourtClosed { session_id, ExecutorGuilty, proof }`), serialized — the bytes the chunks cut and the digest pins.
pub(super) fn held_close_bytes_v1(session_id: Hash64, proof: PalwCourtVerdictProofV2) -> Result<Vec<u8>, String> {
    let close = PalwConsensusObjectV2::CourtClosed { session_id, verdict: PalwCourtVerdictV2::ExecutorGuilty, proof };
    borsh::to_vec(&close).map_err(|e| format!("the close does not serialize: {e}"))
}

/// **Plan a held leaf challenge** (see the module doc). `close` makes the close's bytes for the session the
/// challenge opens (the id is derived first: the close names it); `sign` signs the challenge digest under
/// [`PALW_HELD_LEAF_CHALLENGE_MLDSA87_CONTEXT_V1`] with the accuser's key (`None`: this node holds no key).
pub(super) fn plan_held_leaf_challenge_v1(
    i: PalwHeldChallengeInputsV1<'_>,
    close: impl FnOnce(Hash64) -> Result<Vec<u8>, String>,
    sign: impl FnOnce(&[u8]) -> Option<Vec<u8>>,
) -> Result<PalwHeldChallengePlanV1, String> {
    let claim = i.claim;
    let mut challenge = PalwHeldLeafChallengeV1 {
        version: PALW_HELD_LEAF_CHALLENGE_VERSION_V1,
        claim: claim.claim_id,
        execution_root: claim.execution_root,
        trace_root: claim.trace_root,
        executor_bond: claim.executor_bond,
        accuser_bond: i.accuser,
        leaf_index: i.leaf,
        count: 0,
        chunk_digests: Vec::new(),
        close_digest: Hash64::default(),
        signature: Vec::new(),
    };
    // The close the session will be asked to grade: the court's own object, by the session the challenge opens.
    let session_id = palw_held_leaf_challenge_session_id_v1(&challenge, i.class_ladder);
    let whole = close(session_id)?;
    // The court's cut, and the court's two carried limits: the ruleset's chunk count and an assembly clock that
    // ends inside the session's backstop — refused here, where a smaller close can still be chosen.
    let count = whole.len().div_ceil(PALW_COURT_CLOSE_CHUNK_MAX_BYTES);
    let ceiling = i.court.max_close_chunks().min(u64::from(PALW_COURT_CLOSE_MAX_CHUNKS));
    if count == 0 || count as u64 > ceiling {
        return Err(format!(
            "the close is {} bytes: {count} carriers of {PALW_COURT_CLOSE_CHUNK_MAX_BYTES}, and this court carries at most {ceiling}",
            whole.len()
        ));
    }
    let count = count as u8;
    let assembly = palw_close_assembly_daa_v1(count);
    if assembly > i.window_court {
        return Err(format!(
            "the close needs {assembly} DAA to assemble ({count} chunks), more than the court window of {} DAA",
            i.window_court
        ));
    }
    let chunks: Vec<Vec<u8>> = whole.chunks(PALW_COURT_CLOSE_CHUNK_MAX_BYTES).map(<[u8]>::to_vec).collect();
    debug_assert_eq!(chunks.len(), count as usize);
    challenge.count = count;
    challenge.chunk_digests = chunks.iter().map(|part| palw_court_close_chunk_digest_v1(part)).collect();
    challenge.close_digest = palw_court_close_chunk_digest_v1(&whole);
    let digest = palw_held_leaf_challenge_digest_v1(i.network_domain.as_byte_slice(), &challenge);
    challenge.signature = sign(digest.as_byte_slice()).ok_or("this node holds no bond key, so it cannot sign a challenge")?;
    palw_held_leaf_challenge_shape_v1(&challenge).map_err(|e| format!("the challenge is not well-formed: {e}"))?;
    let close_digest = challenge.close_digest;
    let declaration = PalwConsensusObjectV2::HeldLeafChallengeDeclared { challenge: Box::new(challenge) };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&declaration)
        .map_err(|e| format!("the declaration cannot ride a carrier: {e}"))?;
    Ok(PalwHeldChallengePlanV1 { session_id, declaration, chunks, close_digest })
}

/// **The chunk `index` of a close**, as the court's table carries it (`CourtCloseChunk`, challenger side).
pub(super) fn held_chunk_object_v1(session_id: Hash64, index: usize, bytes: &[u8]) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::CourtCloseChunk { session_id, side: PalwCourtSideV1::Challenger, index: index as u8, bytes: bytes.to_vec() }
}

/// **The court queue's key of a challenge's carrier**: the declaration is round 0 and chunk `i` round `1 + i`
/// (the queue sorts a session's items by round, so the carriers chain declaration-first), on the challenger's
/// side. Node bookkeeping only (`court_pending`, and `court_moved` as the debounce of a carrier in flight).
pub(super) fn held_queue_key_v1(session_id: Hash64, round: u32) -> (Hash64, u32, bool) {
    (session_id, round, false)
}

impl super::PalwPanelService {
    /// **File a held leaf challenge for the close `proof` convicts with at `leaf`** (see the module doc). Nothing is
    /// filed — and the claim is recorded by the caller — when the plan refuses, the fold refuses the declaration or
    /// another group stands on the session.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn file_held_leaf_challenge_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        target: &PalwDisputableClaimV2,
        bond_key: PalwBondKeyV2,
        leaf: u64,
        proof: PalwCourtVerdictProofV2,
        label: &str,
        due: u64,
        current_daa: u64,
        held: &mut Vec<PalwHeldChunksV1>,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) {
        let params = &self.consensus_config.params;
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            return;
        };
        let court = self.config.court;
        let class_ladder = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
            &court,
            params.palw_court_ladder.is_some_and(|f| f.is_active(current_daa)),
        );
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            params.net.to_string().as_bytes(),
            Some(self.consensus_config.genesis.hash),
        );
        let plan = match plan_held_leaf_challenge_v1(
            PalwHeldChallengeInputsV1 {
                claim: target,
                accuser: bond_key,
                leaf,
                class_ladder,
                court: &court,
                window_court: bundle.state.window_court(),
                network_domain,
            },
            |session_id| held_close_bytes_v1(session_id, proof),
            |digest| self.sign(digest, PALW_HELD_LEAF_CHALLENGE_MLDSA87_CONTEXT_V1),
        ) {
            Ok(plan) => plan,
            Err(why) => {
                warn!(
                    "[{PALW_PANEL}] claim {}: its {label} close at leaf {leaf} cannot be filed as a held leaf challenge ({why}); \
                     recorded, not filed",
                    target.claim_id
                );
                return;
            }
        };
        // A restart (or another run of this pass) may find the group already standing: ours when it is this bond's
        // and pins this close — resume delivering; anyone else's, or another close, stands as it is.
        if let Some(group) = session.palw_court_close_group_v1(plan.session_id, PalwCourtSideV1::Challenger) {
            if group.declarer == bond_key && group.close_digest == plan.close_digest {
                info!(
                    "[{PALW_PANEL}] claim {}: the held leaf challenge's close group already stands on session {}; delivering \
                     what is missing",
                    target.claim_id, plan.session_id
                );
                held.push(PalwHeldChunksV1 {
                    claim: target.claim_id,
                    session_id: plan.session_id,
                    close_digest: plan.close_digest,
                    chunks: plan.chunks,
                    queued_daa: current_daa,
                    group_seen: true,
                });
            } else {
                warn!(
                    "[{PALW_PANEL}] claim {}: a challenger-side close group already stands on session {} and is not this node's \
                     close; recorded, not filed",
                    target.claim_id, plan.session_id
                );
            }
            return;
        }
        // The fold's own answer first: the acceptance layer (the bond and its signature, the carriage count) and
        // the arm (the claim live, the leaf in its step space, the court's capacity, the accuser's standing).
        match session.palw_object_rehearsal_v1(&plan.declaration) {
            Some(PalwObjectRehearsalV1::Accepted) => {}
            Some(PalwObjectRehearsalV1::NotAccepted(why)) => {
                warn!(
                    "[{PALW_PANEL}] claim {}: the acceptance layer refuses the held leaf challenge ({why}); recorded, not filed",
                    target.claim_id
                );
                return;
            }
            Some(PalwObjectRehearsalV1::Refused(why)) => {
                warn!(
                    "[{PALW_PANEL}] claim {}: the fold refuses the held leaf challenge ({why}); recorded, not filed",
                    target.claim_id
                );
                return;
            }
            None => {
                warn!(
                    "[{PALW_PANEL}] claim {}: no chain tip to rehearse the held leaf challenge on; recorded, not filed",
                    target.claim_id
                );
                return;
            }
        }
        let key = held_queue_key_v1(plan.session_id, 0);
        if court_pending.iter().any(|(sid, round, responder, _)| (*sid, *round, *responder) == key) {
            return;
        }
        info!(
            "[{PALW_PANEL}] claim {}: its {label} close at leaf {leaf} is {} chunks — filing it as a held leaf challenge (declaration, \
             then the chunks), session {}",
            target.claim_id,
            plan.chunks.len(),
            plan.session_id
        );
        court_due.insert(key, due);
        court_pending.push((plan.session_id, 0, false, plan.declaration));
        held.push(PalwHeldChunksV1 {
            claim: target.claim_id,
            session_id: plan.session_id,
            close_digest: plan.close_digest,
            chunks: plan.chunks,
            queued_daa: current_daa,
            group_seen: false,
        });
    }

    /// **Carry the chunks of every challenge this node holds, once a tick** (see the module doc).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn held_chunks_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        bond_key: PalwBondKeyV2,
        current_daa: u64,
        held: &mut Vec<PalwHeldChunksV1>,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
        court_moved: &HashMap<(Hash64, u32, bool), u64>,
    ) {
        let mut kept: Vec<PalwHeldChunksV1> = Vec::with_capacity(held.len());
        let mut done: Vec<PalwHeldChunksV1> = Vec::new();
        for mut entry in std::mem::take(held) {
            let keep = match session.palw_court_close_group_v1(entry.session_id, PalwCourtSideV1::Challenger) {
                Some(group) if group.declarer == bond_key && group.close_digest == entry.close_digest => {
                    entry.group_seen = true;
                    for (index, bytes) in entry.chunks.iter().enumerate() {
                        if group.has(index as u8) {
                            continue;
                        }
                        let key = held_queue_key_v1(entry.session_id, 1 + index as u32);
                        if court_pending.iter().any(|(sid, round, responder, _)| (*sid, *round, *responder) == key) {
                            continue;
                        }
                        if court_moved.get(&key).is_some_and(|sent| current_daa < sent.saturating_add(COURT_MOVE_REPLAN_DAA)) {
                            continue;
                        }
                        court_due.insert(key, group.assembly_deadline_daa);
                        court_pending.push((entry.session_id, key.1, false, held_chunk_object_v1(entry.session_id, index, bytes)));
                    }
                    true
                }
                // Another group is on the session: not ours to complete.
                Some(_) => false,
                // No group. Seen before: the close completed (the session ended with its verdict) or the sweep
                // convicted its declarer — nothing left to carry. Never seen: the declaration is still queued, in
                // the mempool or in a block not yet accepted; wait for it, within the patience.
                None if entry.group_seen => false,
                None => {
                    let declaration_queued = court_pending
                        .iter()
                        .any(|(sid, round, responder, _)| (*sid, *round, *responder) == held_queue_key_v1(entry.session_id, 0));
                    declaration_queued || current_daa <= entry.queued_daa.saturating_add(PALW_HELD_DECLARATION_PATIENCE_DAA_V1)
                }
            };
            if keep { kept.push(entry) } else { done.push(entry) }
        }
        *held = kept;
        // A chunk still queued whose challenge is done would be refused by the fold (no session, no group): it
        // leaves the queue unsent, rather than paying a carrier. Only chunks that are byte for byte a done
        // challenge's own go — the queue holds other lanes' chunk objects too, which this never touches.
        court_pending
            .retain(|(_, _, _, object)| !done.iter().any(|entry| is_held_challenge_chunk_v1(object, std::slice::from_ref(entry))));
    }
}

/// Whether a queued chunk is one of this module's (its session is a challenge this node ever carried): the queue
/// holds other lanes' chunk objects too, which the retain above must never drop. A chunk of a session this module
/// no longer holds is indistinguishable by shape from a legacy close's challenger chunk, so only the ones whose
/// round is `1 + index` of a held entry's chunks count — and nothing else is dropped.
fn is_held_challenge_chunk_v1(object: &PalwConsensusObjectV2, held: &[PalwHeldChunksV1]) -> bool {
    let PalwConsensusObjectV2::CourtCloseChunk { session_id, side: PalwCourtSideV1::Challenger, index, bytes } = object else {
        return false;
    };
    held.iter().any(|entry| entry.session_id == *session_id && entry.chunks.get(*index as usize).is_some_and(|chunk| chunk == bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim() -> PalwDisputableClaimV2 {
        PalwDisputableClaimV2 {
            accepted_block: Hash64::from_bytes([1; 64]),
            claim_id: Hash64::from_bytes([2; 64]),
            class_id: Hash64::from_bytes([3; 64]),
            artifact_root: Hash64::from_bytes([4; 64]),
            executor_bond: bond(1),
            trace_root: Hash64::from_bytes([5; 64]),
            execution_root: Hash64::from_bytes([6; 64]),
            licensed_daa: 100,
            free_prompt: true,
        }
    }

    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
            kaspa_consensus_core::tx::TransactionId::from_bytes([n; 64]),
            0,
        ))
    }

    /// Testnet-12's own court (what a node on it plans against) and its court window.
    fn t12_court() -> (PalwCourtParamsV2, u64) {
        let params = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode else {
            panic!("testnet-12 runs V2")
        };
        (bundle.court, bundle.state.window_court())
    }

    /// The most chunks testnet-12's court carries.
    fn ceiling() -> usize {
        t12_court().0.max_close_chunks().min(u64::from(PALW_COURT_CLOSE_MAX_CHUNKS)) as usize
    }

    /// A close of `bytes` bytes naming the session it is made for in its first 64 (the planner only cuts, pins
    /// and declares it; whether the proof convicts is the chain's grading).
    fn close_of(bytes: usize) -> impl FnOnce(Hash64) -> Result<Vec<u8>, String> {
        move |session_id| {
            let mut whole = session_id.as_byte_slice().to_vec();
            whole.resize(bytes.max(64), 7);
            Ok(whole)
        }
    }

    fn plan(bytes: usize, window: u64) -> Result<PalwHeldChallengePlanV1, String> {
        let (court, _) = t12_court();
        plan_held_leaf_challenge_v1(
            PalwHeldChallengeInputsV1 {
                claim: &claim(),
                accuser: bond(9),
                leaf: 12,
                class_ladder: 1 << 20,
                court: &court,
                window_court: window,
                network_domain: Hash64::from_bytes([8; 64]),
            },
            close_of(bytes),
            |digest| Some(digest.to_vec()),
        )
    }

    #[test]
    fn a_close_is_cut_by_the_courts_cut_and_declared_over_its_digests() {
        let chunks = ceiling().min(3);
        assert!(chunks >= 2, "testnet-12's court carries more than one chunk");
        let bytes = (chunks - 1) * PALW_COURT_CLOSE_CHUNK_MAX_BYTES + 5;
        let plan = plan(bytes, 3_000).expect("the close plans");
        assert_eq!(plan.chunks.len(), chunks);
        assert!(plan.chunks[..chunks - 1].iter().all(|c| c.len() == PALW_COURT_CLOSE_CHUNK_MAX_BYTES));
        assert_eq!(plan.chunks[chunks - 1].len(), 5, "the last chunk is the remainder");
        let PalwConsensusObjectV2::HeldLeafChallengeDeclared { challenge } = &plan.declaration else { panic!("a challenge") };
        assert_eq!((challenge.count as usize, challenge.leaf_index, challenge.claim), (chunks, 12, claim().claim_id));
        assert_eq!(challenge.accuser_bond, bond(9));
        assert_eq!(
            challenge.chunk_digests,
            plan.chunks.iter().map(|c| palw_court_close_chunk_digest_v1(c)).collect::<Vec<_>>(),
            "one digest per chunk, over the bytes the chunk object will carry"
        );
        let whole: Vec<u8> = plan.chunks.concat();
        assert_eq!(whole.len(), bytes);
        assert_eq!(challenge.close_digest, palw_court_close_chunk_digest_v1(&whole), "the digest of the assembled close");
        assert_eq!(&whole[..64], plan.session_id.as_byte_slice(), "the close is made for the session the challenge opens");
        let mut unsigned = (**challenge).clone();
        unsigned.signature.clear();
        assert_eq!(
            challenge.signature,
            palw_held_leaf_challenge_digest_v1(Hash64::from_bytes([8; 64]).as_byte_slice(), &unsigned).as_byte_slice().to_vec(),
            "signed over the digest of every field but the signature, under the network's domain"
        );
        assert_eq!(plan.close_digest, challenge.close_digest);
    }

    #[test]
    fn a_close_that_fits_one_chunk_is_still_declared() {
        // The accusation object of a close just under a carrier is over it (the framing): one chunk, declared.
        let plan = plan(PALW_COURT_CLOSE_CHUNK_MAX_BYTES - 1_000, 3_000).expect("plans");
        assert_eq!(plan.chunks.len(), 1);
    }

    #[test]
    fn what_the_court_cannot_carry_is_refused_before_a_fee_is_spent() {
        let too_big = plan((ceiling() + 1) * PALW_COURT_CLOSE_CHUNK_MAX_BYTES, 3_000);
        assert!(too_big.is_err(), "past the {} chunks this court carries: {too_big:?}", ceiling());
        // An assembly clock that does not end inside the court window.
        let slow = plan(2 * PALW_COURT_CLOSE_CHUNK_MAX_BYTES, 4);
        assert!(slow.as_ref().is_err_and(|e| e.contains("assemble")), "{slow:?}");
        // No key.
        let (court, window_court) = t12_court();
        let unsigned = plan_held_leaf_challenge_v1(
            PalwHeldChallengeInputsV1 {
                claim: &claim(),
                accuser: bond(9),
                leaf: 0,
                class_ladder: 1 << 20,
                court: &court,
                window_court,
                network_domain: Hash64::default(),
            },
            close_of(10),
            |_| None,
        );
        assert!(unsigned.is_err());
    }

    #[test]
    fn the_declaration_sorts_before_its_chunks_in_the_court_queue() {
        let bytes = PALW_COURT_CLOSE_CHUNK_MAX_BYTES + 5;
        let plan = plan(bytes, 3_000).expect("plans");
        assert_eq!(plan.chunks.len(), 2);
        let mut queue: Vec<(Hash64, u32, bool, PalwConsensusObjectV2)> = Vec::new();
        let mut due: HashMap<(Hash64, u32, bool), u64> = HashMap::new();
        // Pushed in reverse: the queue's own ordering (by round within a session) puts the declaration first.
        for (index, bytes) in plan.chunks.iter().enumerate().rev() {
            let key = held_queue_key_v1(plan.session_id, 1 + index as u32);
            due.insert(key, 500);
            queue.push((plan.session_id, key.1, false, held_chunk_object_v1(plan.session_id, index, bytes)));
        }
        let key = held_queue_key_v1(plan.session_id, 0);
        due.insert(key, 500);
        queue.push((plan.session_id, 0, false, plan.declaration.clone()));
        super::super::palw_court_queue_edf_v1(&mut queue, &due);
        let rounds: Vec<u32> = queue.iter().map(|(_, round, _, _)| *round).collect();
        assert_eq!(rounds, vec![0, 1, 2], "the declaration, then the chunks in index order");
        assert!(matches!(queue[0].3, PalwConsensusObjectV2::HeldLeafChallengeDeclared { .. }));
    }

    #[test]
    fn only_a_done_challenges_own_chunks_are_recognised_for_dropping() {
        let plan = plan(PALW_COURT_CLOSE_CHUNK_MAX_BYTES + 5, 3_000).expect("plans");
        let entry = PalwHeldChunksV1 {
            claim: claim().claim_id,
            session_id: plan.session_id,
            close_digest: plan.close_digest,
            chunks: plan.chunks.clone(),
            queued_daa: 0,
            group_seen: true,
        };
        let ours = held_chunk_object_v1(plan.session_id, 1, &plan.chunks[1]);
        let another_lane = PalwConsensusObjectV2::CourtCloseChunk {
            session_id: Hash64::from_bytes([77; 64]),
            side: PalwCourtSideV1::Challenger,
            index: 0,
            bytes: vec![1, 2, 3],
        };
        assert!(is_held_challenge_chunk_v1(&ours, std::slice::from_ref(&entry)));
        assert!(
            !is_held_challenge_chunk_v1(&another_lane, std::slice::from_ref(&entry)),
            "a legacy close's chunk is not this module's"
        );
        assert!(!is_held_challenge_chunk_v1(&ours, &[]), "with no done entry nothing is recognised — and the retain keeps it");
    }
}
