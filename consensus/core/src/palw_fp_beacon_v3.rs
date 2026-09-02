//! Deriving a beacon fact from the chain (ADR-0044 Decision 4, Unit C item 4).
//!
//! A receipt block's lottery is drawn from a **beacon**: the first attempt-class chain block at
//! or after the claim's draw slot. The whole safety argument of the free-prompt lane rests on
//! that value being a chain fact rather than a producer's assertion —
//!
//! * the beacon's hash is downstream of an attempt's commitment root, which is downstream of a
//!   fresh inference, so re-rolling it costs one inference per sample;
//! * receipt blocks' own hashes are costlessly malleable and therefore can never be beacons
//!   (invariant F15);
//! * and the fact is derived by the VALIDATOR from its own candidate chain, so a spending block
//!   that names a different block is refused rather than believed.
//!
//! This module is the derivation as a pure function over an iterator of chain blocks, so the
//! pipeline supplies the walk and the rule lives in one place. The pipeline's job is to hand over
//! `(block, daa_score, algo_id)` for chain blocks in DESCENDING order from the candidate — which
//! is what `default_backward_chain_iterator` already produces — and this decides which one is the
//! beacon.
//!
//! # Why descending, and why a bound
//!
//! Walking down from the candidate is the only direction a validator can walk cheaply (the chain
//! is a parent list). The first attempt block at or after the slot is therefore the LAST one seen
//! while walking down through the region `daa >= slot`, and the walk stops as soon as it crosses
//! below the slot — at which point the block it just saw is the predecessor witness the fact
//! carries. A caller must still bound the walk (the use window does that in practice); an
//! unbounded scan on a peer-supplied claim would be a denial of service.

use crate::Hash64;
use crate::palw_freeprompt_v3::PalwBeaconFactV3;

/// One chain block, as the derivation reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwChainBlockFactV3 {
    pub block: Hash64,
    pub daa_score: u64,
    /// The header's declared algorithm. Only an attempt-lane id makes a block a beacon — which of
    /// the two ADR-0072 ids that is depends on the block's own height, so the derivation asks
    /// `is_attempt_class_v3` rather than comparing against one number.
    pub pow_algo_id: u8,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwBeaconDeriveV3Error {
    #[error("no attempt-class chain block at or after slot {slot} within the walked range — the draw has not happened yet")]
    NoBeaconYet { slot: u64 },
    #[error("the walk was exhausted before reaching slot {slot}; it must extend below the slot to witness the predecessor")]
    WalkTooShort { slot: u64 },
}

/// **Is this chain block on the attempt lane of a network whose attempt id is `attempt_algo_id`?**
///
/// Exact equality, PLUS ADR-0072's other attempt id when `attempt_algo_id` is a PALW attempt id at
/// all — because the walk crosses the fence and the chain it walks does not.
///
/// A beacon walk is the one consumer that cannot resolve the lane at a single height: it descends
/// through the fence, so the blocks it must recognise as attempt-class carry algo-6 below it and
/// algo-9 at and above it. Asked with one id it saw the attempt chain stop at the fence —
/// `prev_attempt_daa` froze on the last pre-fence attempt block and never advanced again, and every
/// ADR-0044 receipt spend for the rest of the chain's life drew against that stale witness. There
/// is no ambiguity to resolve: exactly one of the two ids is admissible at any height, and the
/// header gate refused the other one before the block was stored.
///
/// The "not in V2 mode cannot accidentally match" property is kept: if `attempt_algo_id` is not
/// itself an attempt id, only exact equality matches.
#[inline]
fn is_attempt_class_v3(declared: u8, attempt_algo_id: u8) -> bool {
    declared == attempt_algo_id
        || (crate::pow_layer0::is_palw_attempt_algo_id(attempt_algo_id) && crate::pow_layer0::is_palw_attempt_algo_id(declared))
}

/// Derive the beacon fact for `slot` from chain blocks in DESCENDING DAA order.
///
/// `attempt_algo_id` is the network's attempt-lane id (the bundle's `algorithm_id`), passed in
/// rather than hard-coded so a network that is not in V2 mode cannot accidentally match. Matched
/// through [`is_attempt_class_v3`], which is what makes the walk survive ADR-0072's fence.
///
/// The walk must continue past the slot: the fact carries `prev_attempt_daa` — the last
/// attempt-class block strictly BELOW the slot — and that witness is what makes "first at or
/// after" checkable by someone who did not do the walk. A walk that stops at the slot cannot
/// produce it, and this returns `WalkTooShort` rather than inventing a zero.
pub fn derive_beacon_fact_v3<I>(
    slot: u64,
    attempt_algo_id: u8,
    descending_chain: I,
) -> Result<PalwBeaconFactV3, PalwBeaconDeriveV3Error>
where
    I: IntoIterator<Item = PalwChainBlockFactV3>,
{
    // The last attempt block seen while still at or above the slot is the FIRST one at or after
    // it, because the walk descends.
    let mut candidate: Option<PalwChainBlockFactV3> = None;
    for fact in descending_chain {
        if fact.daa_score >= slot {
            if is_attempt_class_v3(fact.pow_algo_id, attempt_algo_id) {
                candidate = Some(fact);
            }
            continue;
        }
        // Below the slot: this is the region the predecessor witness comes from. The first
        // attempt block found here is the last one before the slot; genesis-shaped chains with
        // none use 0, which the validator's inequality accepts (`prev < slot`).
        let prev_attempt_daa = if is_attempt_class_v3(fact.pow_algo_id, attempt_algo_id) {
            fact.daa_score
        } else {
            // Keep descending for the witness — but only through this same iterator, which the
            // caller bounded.
            continue;
        };
        let beacon = candidate.ok_or(PalwBeaconDeriveV3Error::NoBeaconYet { slot })?;
        return Ok(PalwBeaconFactV3 { beacon_block: beacon.block, beacon_daa: beacon.daa_score, prev_attempt_daa });
    }
    // The iterator ran out. If it ran out BELOW the slot we simply never met an attempt block
    // there, which on a young chain means "none before the slot" — but this function cannot tell
    // that from "the caller stopped walking", and guessing would manufacture a witness. The
    // caller knows whether it walked to genesis; `walk_to_genesis` says so explicitly.
    Err(PalwBeaconDeriveV3Error::WalkTooShort { slot })
}

/// [`derive_beacon_fact_v3`] for a walk the caller drove all the way to genesis, where "no
/// attempt block below the slot" is a FACT rather than a truncation — the witness is 0.
pub fn derive_beacon_fact_to_genesis_v3<I>(
    slot: u64,
    attempt_algo_id: u8,
    descending_chain_to_genesis: I,
) -> Result<PalwBeaconFactV3, PalwBeaconDeriveV3Error>
where
    I: IntoIterator<Item = PalwChainBlockFactV3>,
{
    let mut candidate: Option<PalwChainBlockFactV3> = None;
    for fact in descending_chain_to_genesis {
        if fact.daa_score >= slot {
            if is_attempt_class_v3(fact.pow_algo_id, attempt_algo_id) {
                candidate = Some(fact);
            }
            continue;
        }
        if is_attempt_class_v3(fact.pow_algo_id, attempt_algo_id) {
            let beacon = candidate.ok_or(PalwBeaconDeriveV3Error::NoBeaconYet { slot })?;
            return Ok(PalwBeaconFactV3 {
                beacon_block: beacon.block,
                beacon_daa: beacon.daa_score,
                prev_attempt_daa: fact.daa_score,
            });
        }
    }
    let beacon = candidate.ok_or(PalwBeaconDeriveV3Error::NoBeaconYet { slot })?;
    Ok(PalwBeaconFactV3 { beacon_block: beacon.block, beacon_daa: beacon.daa_score, prev_attempt_daa: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_freeprompt_v3::validate_beacon_fact_v3;
    use crate::pow_layer0::{POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3};

    const ATTEMPT: u8 = POW_ALGO_ID_PALW_COMMITTED_V2;
    const RECEIPT: u8 = POW_ALGO_ID_PALW_RECEIPT_V3;

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// `(daa, algo)` pairs, youngest first — the order a backward chain iterator yields.
    fn chain(pairs: &[(u64, u8)]) -> Vec<PalwChainBlockFactV3> {
        pairs.iter().map(|(daa, algo)| PalwChainBlockFactV3 { block: h64(*daa), daa_score: *daa, pow_algo_id: *algo }).collect()
    }

    /// The beacon is the FIRST attempt block at or after the slot, and what is derived validates
    /// — the derivation and the check are two sides of one rule.
    #[test]
    fn the_beacon_is_the_first_attempt_block_at_or_after_the_slot() {
        // Slot 100. Descending: 140(attempt), 130(receipt), 120(attempt), 110(receipt), 95(attempt).
        // The first attempt at-or-after 100 is 120; the witness below is 95.
        let fact = derive_beacon_fact_v3(
            100,
            ATTEMPT,
            chain(&[(140, ATTEMPT), (130, RECEIPT), (120, ATTEMPT), (110, RECEIPT), (95, ATTEMPT)]),
        )
        .unwrap();
        assert_eq!((fact.beacon_daa, fact.prev_attempt_daa), (120, 95));
        assert_eq!(fact.beacon_block, h64(120));
        validate_beacon_fact_v3(100, &fact).expect("what the chain derived, the validator accepts");

        // A block exactly AT the slot is the beacon — the slot's own score is inside the region.
        let fact = derive_beacon_fact_v3(100, ATTEMPT, chain(&[(140, ATTEMPT), (100, ATTEMPT), (90, ATTEMPT)])).unwrap();
        assert_eq!((fact.beacon_daa, fact.prev_attempt_daa), (100, 90));
        validate_beacon_fact_v3(100, &fact).unwrap();
    }

    /// **The walk crosses ADR-0072's fence, so both attempt ids are attempt-class.**
    ///
    /// Past the fence every attempt block carries [`crate::pow_layer0::POW_ALGO_ID_PALW_EXEC_V3`]
    /// and the pre-fence history carries [`POW_ALGO_ID_PALW_COMMITTED_V2`]. Matching one id, the
    /// derivation saw the attempt chain STOP at the fence: no beacon existed above it, and
    /// `prev_attempt_daa` froze on the last pre-fence attempt block for the rest of the chain's
    /// life. The caller cannot fix this by passing a different number — the network's attempt id
    /// comes from the bundle, whose `algorithm_id` `PalwRulesetV2::validate` pins at 6.
    #[test]
    fn a_chain_that_crossed_the_fence_still_has_a_beacon() {
        const EXEC: u8 = crate::pow_layer0::POW_ALGO_ID_PALW_EXEC_V3;
        // Fence at 100: 140 and 120 are post-fence (algo-9), 95 and 80 are pre-fence (algo-6).
        let fact =
            derive_beacon_fact_v3(110, ATTEMPT, chain(&[(140, EXEC), (130, RECEIPT), (120, EXEC), (95, ATTEMPT), (80, ATTEMPT)]))
                .unwrap();
        assert_eq!((fact.beacon_daa, fact.prev_attempt_daa), (120, 95), "the post-fence attempt block is a beacon");
        validate_beacon_fact_v3(110, &fact).expect("what the chain derived, the validator accepts");

        // And the witness below the slot is found on the OTHER side of the fence, which is the half
        // that freezes: with only the pre-fence id matched, `prev_attempt_daa` is the last thing
        // that ever moves.
        let fact = derive_beacon_fact_v3(90, ATTEMPT, chain(&[(140, EXEC), (120, EXEC), (85, ATTEMPT)])).unwrap();
        assert_eq!((fact.beacon_daa, fact.prev_attempt_daa), (120, 85));

        // The receipt lane is still never a beacon on either side of the fence — the widening is to
        // the attempt lane's two ids and to nothing else.
        assert_eq!(
            derive_beacon_fact_v3(100, ATTEMPT, chain(&[(160, RECEIPT), (150, RECEIPT), (95, EXEC)])).unwrap_err(),
            PalwBeaconDeriveV3Error::NoBeaconYet { slot: 100 }
        );

        // A non-PALW network's id matches only itself: `is_attempt_class_v3`'s fallback is exact
        // equality, so nothing about algo-1 chains moved.
        assert!(!is_attempt_class_v3(ATTEMPT, crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH));
        assert!(!is_attempt_class_v3(EXEC, crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH));
    }

    /// **Receipt blocks are never beacons** (invariant F15) — a chain of nothing but receipt
    /// blocks above the slot has no beacon yet, however many blocks it has.
    #[test]
    fn receipt_blocks_are_never_beacons() {
        let err =
            derive_beacon_fact_v3(100, ATTEMPT, chain(&[(160, RECEIPT), (150, RECEIPT), (140, RECEIPT), (95, ATTEMPT)])).unwrap_err();
        assert_eq!(err, PalwBeaconDeriveV3Error::NoBeaconYet { slot: 100 });

        // …and one attempt block among them is the beacon, whatever surrounds it.
        let fact =
            derive_beacon_fact_v3(100, ATTEMPT, chain(&[(160, RECEIPT), (150, ATTEMPT), (140, RECEIPT), (95, ATTEMPT)])).unwrap();
        assert_eq!(fact.beacon_daa, 150);
    }

    /// A walk that stops before witnessing the predecessor is TOO SHORT, not a fact with a zero
    /// witness — manufacturing the witness would let a truncated walk answer a question it did
    /// not look at.
    #[test]
    fn a_truncated_walk_is_named_not_guessed() {
        let err = derive_beacon_fact_v3(100, ATTEMPT, chain(&[(140, ATTEMPT), (120, ATTEMPT)])).unwrap_err();
        assert_eq!(err, PalwBeaconDeriveV3Error::WalkTooShort { slot: 100 });

        // The to-genesis variant may answer with witness 0, because there the absence IS the fact.
        let fact = derive_beacon_fact_to_genesis_v3(100, ATTEMPT, chain(&[(140, ATTEMPT), (120, ATTEMPT)])).unwrap();
        assert_eq!((fact.beacon_daa, fact.prev_attempt_daa), (120, 0));
        validate_beacon_fact_v3(100, &fact).unwrap();

        // …and it still refuses when there is no beacon at all.
        assert_eq!(
            derive_beacon_fact_to_genesis_v3(100, ATTEMPT, chain(&[(90, ATTEMPT), (80, ATTEMPT)])).unwrap_err(),
            PalwBeaconDeriveV3Error::NoBeaconYet { slot: 100 }
        );
    }

    /// A derived fact is one a validator accepts, across a sweep of chain shapes — the two halves
    /// of the rule agree by construction, not by inspection.
    #[test]
    fn derivation_and_validation_agree_across_shapes() {
        let shapes: Vec<Vec<(u64, u8)>> = vec![
            vec![(200, ATTEMPT), (150, ATTEMPT), (100, ATTEMPT), (50, ATTEMPT)],
            vec![(200, RECEIPT), (150, ATTEMPT), (149, RECEIPT), (99, ATTEMPT)],
            vec![(101, ATTEMPT), (100, RECEIPT), (99, RECEIPT), (98, ATTEMPT)],
            vec![(300, ATTEMPT), (299, ATTEMPT), (298, ATTEMPT), (1, ATTEMPT)],
        ];
        for shape in shapes {
            for slot in [1u64, 50, 99, 100, 101, 150, 250] {
                match derive_beacon_fact_v3(slot, ATTEMPT, chain(&shape)) {
                    Ok(fact) => {
                        validate_beacon_fact_v3(slot, &fact)
                            .unwrap_or_else(|e| panic!("derived a fact the validator rejects at slot {slot} on {shape:?}: {e}"));
                        // And the beacon really is an attempt block from this chain.
                        assert!(
                            shape.iter().any(|(daa, algo)| *daa == fact.beacon_daa && *algo == ATTEMPT),
                            "the beacon must be an attempt block of the walked chain"
                        );
                    }
                    Err(_) => { /* a shape with no beacon in range is a legitimate answer */ }
                }
            }
        }
    }
}
