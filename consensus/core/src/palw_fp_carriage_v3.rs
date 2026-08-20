//! What a V2 header commits about its parent's PALW state, and how a pruned node is allowed to
//! believe a carriage it did not compute (ADR-0042 Decision 5, ADR-0043 §4, ADR-0044 Unit E).
//!
//! # The one rule, in one place
//!
//! Both lanes carry `parent_state_root` — the PALW state root **after the block's selected
//! parent**. That is the whole hinge of the pruning story: a node that has thrown away every
//! block below the pruning point can still check a peer-supplied snapshot, because any header
//! whose selected parent IS the pruning point commits to exactly that snapshot's root, and those
//! headers arrive under proof-of-work and the headers proof.
//!
//! The block validator already needed this decode; so does the import gate. Two decoders for one
//! field is two rules that drift, so this module is the only one, and
//! [`palw_carried_parent_state_root_v3`] is what both call.
//!
//! # Why the root and not the carriage digest
//!
//! [`PalwStateCarriageV2::digest`] identifies a *serialization*; `state_root` identifies a
//! *state*. The chain commits to the second and never to the first, so the second is what an
//! importer must check against — and it is the stronger one anyway, since two carriages that
//! deserialize to the same state are the same fact regardless of their bytes.
//!
//! # Why "no witness" is a refusal, not a pass
//!
//! [`verify_pruning_point_carriage_v3`] takes the committed root as a REQUIRED argument. A
//! caller that has no child header committing to the pruning point has nothing to check against,
//! and the answer there is to wait for the header — not to load. An unverifiable snapshot is
//! precisely the one an attacker supplies, and the PALW state is bonds, class targets and
//! claims: voting weight, lottery difficulty, and reward eligibility. Detection after the write
//! is not a defence (the same lesson the DNS-overlay import records in its own gate).

use crate::Hash64;
use crate::palw_mode_v2::PalwConsensusParamsV2;
use crate::palw_state_v2::{PalwChainStateV2, PalwStateCarriageV2, PalwStateV2Error};

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwCarriedRootV3Error {
    #[error("algorithm {0} carries no PALW commitment, so it commits to no parent state root")]
    NotAWorkAlgorithm(u8),
    #[error("attempt carriage: {0}")]
    AttemptUndecodable(String),
    #[error("spend carriage: {0}")]
    SpendUndecodable(String),
}

/// One V2 header's decoded work, whichever lane produced it.
///
/// This exists so there is exactly ONE decoder. The block validator needs the whole envelope
/// (to admit it against chain state); the pruning-carriage import gate needs only
/// `parent_state_root` and holds no chain state at all. Giving each its own `decode` call would
/// be two readings of one field, and a third lane would silently get only one of them.
#[derive(Clone, Debug)]
pub enum PalwCarriedWorkV3 {
    Attempt(crate::palw_attempt_v2::PalwAttemptEnvelopeV2),
    Spend(crate::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3),
}

impl PalwCarriedWorkV3 {
    /// The PALW state root **after this block's selected parent** — the field both lanes carry,
    /// and the hinge of the whole pruning story (see the module docs).
    pub fn parent_state_root(&self) -> Hash64 {
        match self {
            Self::Attempt(e) => e.attempt.parent_state_root,
            Self::Spend(e) => e.spend.parent_state_root,
        }
    }
}

/// Decode a V2 header's carriage into its lane's envelope.
///
/// `bundle` names both lane ids, so a network that is not in V2 mode cannot match by accident and
/// a lane id that moves does not need a second edit here.
///
/// This decodes. It does not verify the carriage's signature, ticket, or admission — those are
/// the block validator's job and happen after, with chain state in hand. The separation is
/// deliberate: the import gate has no chain state to admit against, and must still be able to
/// read what a header committed.
pub fn palw_decode_carried_work_v3(
    bundle: &PalwConsensusParamsV2,
    algo_id: u8,
    palw_commitment: &[u8],
) -> Result<PalwCarriedWorkV3, PalwCarriedRootV3Error> {
    if algo_id == bundle.algorithm_id {
        crate::palw_attempt_v2::PalwAttemptEnvelopeV2::decode(palw_commitment)
            .map(PalwCarriedWorkV3::Attempt)
            .map_err(|e| PalwCarriedRootV3Error::AttemptUndecodable(e.to_string()))
    } else if algo_id == bundle.freeprompt.receipt_algorithm_id() {
        crate::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3::decode(palw_commitment)
            .map(PalwCarriedWorkV3::Spend)
            .map_err(|e| PalwCarriedRootV3Error::SpendUndecodable(e.to_string()))
    } else {
        Err(PalwCarriedRootV3Error::NotAWorkAlgorithm(algo_id))
    }
}

/// [`palw_decode_carried_work_v3`] for the caller that wants only the committed root — the
/// pruning-carriage import gate.
pub fn palw_carried_parent_state_root_v3(
    bundle: &PalwConsensusParamsV2,
    algo_id: u8,
    palw_commitment: &[u8],
) -> Result<Hash64, PalwCarriedRootV3Error> {
    palw_decode_carried_work_v3(bundle, algo_id, palw_commitment).map(|w| w.parent_state_root())
}

/// What one node hands another: the pruning point, and the PALW state as-of it.
///
/// The pruning point travels WITH the snapshot even though the requester named it, for the same
/// reason the DNS-overlay snapshot carries one — the server may have advanced past the point the
/// requester asked about between the request and the reply, and a snapshot of the wrong point
/// must be recognisable as such rather than checked against the wrong root.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPruningCarriageWire {
    pub pruning_point: Hash64,
    pub carriage: PalwStateCarriageV2,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwCarriageImportV3Error {
    #[error("the carriage does not load: {0}")]
    Unloadable(String),
    #[error("the carriage stands at {got:?}, not at the pruning point {want}")]
    WrongPoint { got: Option<Hash64>, want: Hash64 },
}

/// Load a peer-supplied pruning-point carriage, or refuse it.
///
/// `committed_root` is what a header whose selected parent is `pruning_point` says the state
/// there is. Two things must hold, and both are checked before any caller writes anything:
///
/// 1. the carriage rebuilds into a self-consistent state whose root IS `committed_root`
///    (`into_state` with `Some`, which is the peer-supplied discipline of ADR-0043 §4 — the
///    `None` form exists only for a node's own disk);
/// 2. the state stands at `pruning_point`. The root already covers `last_point`, so (1) implies
///    this; it is checked anyway so a mismatch is reported as the wrong-block error it is rather
///    than as an opaque root mismatch.
pub fn verify_pruning_point_carriage_v3(
    bundle: &PalwConsensusParamsV2,
    pruning_point: Hash64,
    committed_root: Hash64,
    carriage: PalwStateCarriageV2,
) -> Result<PalwChainStateV2, PalwCarriageImportV3Error> {
    let state = carriage
        .into_state(&bundle.state, Some(committed_root))
        .map_err(|e: PalwStateV2Error| PalwCarriageImportV3Error::Unloadable(e.to_string()))?;
    let at = state.last_point().map(|p| p.block);
    if at != Some(pruning_point) {
        return Err(PalwCarriageImportV3Error::WrongPoint { got: at, want: pruning_point });
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_fp_devnet_v3::palw_fp_devnet_bundle_derived_root_v3;
    use crate::palw_state_v2::{PalwBlockContextV2, PalwBlockWorkV3, PalwStateCarriageV2};

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bundle() -> PalwConsensusParamsV2 {
        palw_fp_devnet_bundle_derived_root_v3(
            h64(0xBA5E),
            h64(0xC0757),
            crate::tx::TransactionOutpoint::new(h64(0xB0D).into(), 0),
            vec![0x11; 32],
            h64(0xE0),
        )
        .expect("the devnet bundle is well-formed")
    }

    /// A state standing at a block, built the way the walk builds one: the genesis registrations
    /// applied to the empty state at a chain point. The point is a carriage with a `last_point`,
    /// not a realistic history.
    fn state_at(bundle: &PalwConsensusParamsV2, block: Hash64) -> PalwChainStateV2 {
        let ctx = PalwBlockContextV2 { block, daa_score: 4242, blue_score: 4242 };
        let (state, _) = crate::palw_state_v2::apply_palw_transition_v3(
            &PalwChainStateV2::genesis(),
            &bundle.state,
            &ctx,
            &crate::palw_mode_v2::palw_genesis_objects_v2(&bundle.genesis),
            PalwBlockWorkV3::None,
        )
        .expect("the genesis registrations apply to the empty state");
        state
    }

    /// The round trip the importer performs: capture a state, ship its carriage, load it back
    /// against the root the chain committed — and get the same state.
    #[test]
    fn a_carriage_loads_against_the_root_the_chain_committed() {
        let bundle = bundle();
        let state = state_at(&bundle, h64(7));
        let root = state.state_root();
        let carriage = PalwStateCarriageV2::from_state(&state);

        let loaded = verify_pruning_point_carriage_v3(&bundle, h64(7), root, carriage).expect("the carriage loads");
        assert_eq!(loaded.state_root(), root);
        assert_eq!(loaded.last_point().map(|p| p.block), Some(h64(7)));
    }

    /// **The gate is the root, not self-consistency.** A carriage that is internally coherent but
    /// is a snapshot of a DIFFERENT state is refused — this is the whole reason `into_state` is
    /// called with `Some(root)` on a peer-supplied snapshot.
    #[test]
    fn a_coherent_carriage_of_a_different_state_is_refused() {
        let bundle = bundle();
        let real = state_at(&bundle, h64(7));
        let root = real.state_root();

        // Same shape, different chain point: coherent, and not this state.
        let other = state_at(&bundle, h64(8));
        assert_ne!(other.state_root(), root);
        let err = verify_pruning_point_carriage_v3(&bundle, h64(7), root, PalwStateCarriageV2::from_state(&other))
            .expect_err("a snapshot of another state must not load against this root");
        assert!(matches!(err, PalwCarriageImportV3Error::Unloadable(_)), "got {err:?}");

        // …and a carriage whose root DOES match but which stands elsewhere cannot exist, because
        // the root covers `last_point`. Asking for the wrong point is therefore also a refusal.
        let err = verify_pruning_point_carriage_v3(&bundle, h64(9), root, PalwStateCarriageV2::from_state(&real))
            .expect_err("a carriage standing at another block must not import as this pruning point");
        assert!(
            matches!(err, PalwCarriageImportV3Error::Unloadable(_) | PalwCarriageImportV3Error::WrongPoint { .. }),
            "got {err:?}"
        );
    }

    /// A tamper that keeps the carriage coherent still moves the root — including one in
    /// `receipt_targets`, the collection the free-prompt lane's difficulty lives in. (ADR-0043 §2
    /// used to omit both receipt collections from its written preimage; the code never did, and
    /// this pins it.)
    #[test]
    fn tampering_with_the_receipt_lane_moves_the_root() {
        let bundle = bundle();
        let state = state_at(&bundle, h64(7));
        let root = state.state_root();

        let mut carriage = PalwStateCarriageV2::from_state(&state);
        assert!(!carriage.receipt_targets.is_empty(), "the devnet bundle registers a class, so the receipt lane has a target");
        for target in carriage.receipt_targets.values_mut() {
            // The devnet BOOT target already admits everything (`u128::MAX`), so "make it easier"
            // is not available as a tamper here — which is the point. What is pinned is that ANY
            // difference in the receipt lane's difficulty is caught, in either direction, because
            // the collection is inside the root's preimage at all.
            target.target = u128::MAX / 2;
            assert_ne!(target.target, u128::MAX, "the tamper must actually change the value");
        }
        let err = verify_pruning_point_carriage_v3(&bundle, h64(7), root, carriage)
            .expect_err("a forged receipt target must not load against the honest root");
        assert!(matches!(err, PalwCarriageImportV3Error::Unloadable(_)), "got {err:?}");
    }

    /// Only the two work algorithms commit to a parent state root, and each reads its OWN lane's
    /// carriage — a spend envelope presented as an attempt is undecodable, not silently accepted.
    #[test]
    fn each_lane_reads_its_own_carriage_and_nothing_else() {
        let bundle = bundle();
        for foreign in [0u8, 1, 4, 5, 200] {
            if bundle.accepts_algo_id(foreign) {
                continue;
            }
            assert_eq!(
                palw_carried_parent_state_root_v3(&bundle, foreign, &[]).unwrap_err(),
                PalwCarriedRootV3Error::NotAWorkAlgorithm(foreign)
            );
        }
        // A payload that is not this lane's carriage fails to decode rather than yielding a root.
        assert!(matches!(
            palw_carried_parent_state_root_v3(&bundle, bundle.algorithm_id, b"PFS3not-an-attempt"),
            Err(PalwCarriedRootV3Error::AttemptUndecodable(_))
        ));
        assert!(matches!(
            palw_carried_parent_state_root_v3(&bundle, bundle.freeprompt.receipt_algorithm_id(), b"PAT2not-a-spend"),
            Err(PalwCarriedRootV3Error::SpendUndecodable(_))
        ));
    }
}
