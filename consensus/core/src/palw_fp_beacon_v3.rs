//! Deriving a beacon fact from the chain (ADR-0044 Decision 4, Unit C item 4).
//!
//! A receipt block's lottery is drawn from a **beacon**: the first `k` attempt-class chain blocks
//! at or after the claim's draw slot, folded. The whole safety argument of the free-prompt lane
//! rests on that value being a chain fact rather than a producer's assertion —
//!
//! * the beacon's hash is downstream of an attempt's commitment root, which is downstream of a
//!   fresh inference, so re-rolling it costs one inference per sample;
//! * receipt blocks' own hashes are costlessly malleable and therefore can never be beacons
//!   (invariant F15);
//! * and the fact is derived by the VALIDATOR from its own candidate chain, so a spending block
//!   that names a different block is refused rather than believed.
//!
//! **ADR-0073 SA-1: why `k` and not one.** Re-rolling the beacon costs an inference, but
//! WITHHOLDING it costs only that block's subsidy — a producer whose own block would be the beacon
//! can simply drop it when the draw is unfavourable to its own pending claims, and try again with
//! the next one. Today the receipt lane is weightless so the stake on that single bit is small;
//! ADR-0073 Phase ④ gives receipt blocks chain position and share, which multiplies it. Folding
//! the first `k ≥ 3` attempt blocks means a producer holding attempt share `p` must hold ALL `k`
//! to choose the draw: `p^k` instead of `p`. Nothing else about the derivation moves — the walk is
//! still the validator's own, still descending, still bounded, and the fold is over blocks that
//! each already cost an inference.
//!
//! `k = 1` is the pre-SA-1 rule and stays byte-identical: at one block the "fold" is the IDENTITY,
//! not a one-element digest, so a network with the fence off derives exactly the values it always
//! did. `k` reaches here as an argument rather than a constant precisely because two builds that
//! disagreed about it in silence would derive different draws from one chain — it is a fence's
//! companion value in [`crate::config::params::PalwBeaconFoldV1`], inside `consensus_params_id`.
//!
//! This module is the derivation as a pure function over an iterator of chain blocks, so the
//! pipeline supplies the walk and the rule lives in one place. The pipeline's job is to hand over
//! `(block, daa_score, algo_id)` for chain blocks in DESCENDING order from the candidate — which
//! is what `default_backward_chain_iterator` already produces — and this decides which ones are
//! the beacon.
//!
//! # Why descending, and why a bound
//!
//! Walking down from the candidate is the only direction a validator can walk cheaply (the chain
//! is a parent list). The first `k` attempt blocks at or after the slot are therefore the LAST `k`
//! seen while walking down through the region `daa >= slot`, and the walk stops as soon as it
//! crosses below the slot — at which point the block it just saw is the predecessor witness the
//! fact carries. A caller must still bound the walk (the use window does that in practice); an
//! unbounded scan on a peer-supplied claim would be a denial of service. The `k` blocks are held
//! in a ring of capacity `k`, so the memory the walk costs is the rule's own width and not the
//! chain's length.

use crate::Hash64;
use crate::palw_freeprompt_v3::{PalwBeaconFactV3, fp_beacon_fold_v3};

/// **ADR-0073 SA-1's floor.** A fold narrower than three blocks does not bound the withholding
/// bias the amendment is about, so an armed fence naming less is refused at construction
/// (`Params::validate_palw_v2`) rather than shipped as a rule that reads like one and is not.
pub const PALW_BEACON_FOLD_MIN_K_V1: u8 = 3;

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
    #[error(
        "only {found} of the {needed} attempt-class chain blocks the fold needs exist at or after slot {slot} within the walked range — the draw has not happened yet"
    )]
    NoBeaconYet { slot: u64, needed: u8, found: u8 },
    #[error("the walk was exhausted before reaching slot {slot}; it must extend below the slot to witness the predecessor")]
    WalkTooShort { slot: u64 },
}

/// The last `k` attempt blocks the descending walk has seen at or above the slot — which, because
/// the walk descends, are the FIRST `k` at or after it.
///
/// A ring rather than a list: the region above the slot is unbounded in principle, and a
/// derivation that collected all of it would let a peer-supplied claim size a node's allocation.
struct FirstKDescending {
    k: usize,
    /// In walk (descending) order; the front is the oldest push, i.e. the highest DAA.
    seen: Vec<PalwChainBlockFactV3>,
}

impl FirstKDescending {
    fn new(k: usize) -> Self {
        Self { k, seen: Vec::with_capacity(k) }
    }

    fn push(&mut self, fact: PalwChainBlockFactV3) {
        if self.seen.len() == self.k {
            // Evicts the highest-DAA entry: it is further from the slot than everything still
            // held, so it can only be the (k+1)-th or later "first at or after".
            self.seen.remove(0);
        }
        self.seen.push(fact);
    }

    fn found(&self) -> u8 {
        self.seen.len().min(u8::MAX as usize) as u8
    }

    /// `(beacon_block, beacon_daa)` once `k` blocks are in hand, `None` while fewer are.
    ///
    /// `beacon_daa` is the `k`-th block's — the height at which the draw becomes DETERMINED. The
    /// use window has to start there: a window opened at the first of the `k` would license spends
    /// against a draw that did not exist yet.
    fn fold(&self) -> Option<(Hash64, u64)> {
        if self.seen.len() < self.k {
            return None;
        }
        // Ascending chain order — the order the blocks were produced in, and the canonical one for
        // the fold (see `fp_beacon_fold_v3`).
        let ascending: Vec<PalwChainBlockFactV3> = self.seen.iter().rev().copied().collect();
        let kth = *ascending.last().expect("k >= 1 and the ring is full");
        // **`k = 1` is the IDENTITY, not a one-element digest.** The pre-SA-1 rule's bytes are what
        // every fence-off network derives, and a digest here would change them silently.
        let beacon_block = if self.k == 1 {
            kth.block
        } else {
            let hashes: Vec<Hash64> = ascending.iter().map(|fact| fact.block).collect();
            fp_beacon_fold_v3(&hashes)
        };
        Some((beacon_block, kth.daa_score))
    }
}

/// Normalise a caller's `fold_k` to a width the ring can hold. `0` is the pre-SA-1 rule, not an
/// empty fold: a fence that resolved to nothing must derive what an unfenced network derives.
fn fold_width(fold_k: u8) -> usize {
    fold_k.max(1) as usize
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
/// `fold_k` is ADR-0073 SA-1's width, resolved by the caller from the fence at the DRAW'S OWN SLOT
/// (`Params::palw_beacon_fold`). `1` is the pre-SA-1 rule and derives byte-identical facts.
///
/// The walk must continue past the slot: the fact carries `prev_attempt_daa` — the last
/// attempt-class block strictly BELOW the slot — and that witness is what makes "first at or
/// after" checkable by someone who did not do the walk. A walk that stops at the slot cannot
/// produce it, and this returns `WalkTooShort` rather than inventing a zero.
///
/// **The two refusals are different answers and stay different.** `WalkTooShort` says the caller
/// stopped looking; `NoBeaconYet` says the chain has not produced the fold yet — and with `k`
/// blocks required, "fewer than `k` attempt blocks exist at or after the slot" IS the not-drawn-yet
/// case. It reports how many it found, so an operator can tell "one short" from "none at all".
pub fn derive_beacon_fact_v3<I>(
    slot: u64,
    attempt_algo_id: u8,
    fold_k: u8,
    descending_chain: I,
) -> Result<PalwBeaconFactV3, PalwBeaconDeriveV3Error>
where
    I: IntoIterator<Item = PalwChainBlockFactV3>,
{
    // The last k attempt blocks seen while still at or above the slot are the FIRST k at or after
    // it, because the walk descends.
    let needed = fold_width(fold_k);
    let mut ring = FirstKDescending::new(needed);
    for fact in descending_chain {
        if fact.daa_score >= slot {
            if is_attempt_class_v3(fact.pow_algo_id, attempt_algo_id) {
                ring.push(fact);
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
        let (beacon_block, beacon_daa) =
            ring.fold().ok_or(PalwBeaconDeriveV3Error::NoBeaconYet { slot, needed: needed as u8, found: ring.found() })?;
        return Ok(PalwBeaconFactV3 { beacon_block, beacon_daa, prev_attempt_daa });
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
    fold_k: u8,
    descending_chain_to_genesis: I,
) -> Result<PalwBeaconFactV3, PalwBeaconDeriveV3Error>
where
    I: IntoIterator<Item = PalwChainBlockFactV3>,
{
    let needed = fold_width(fold_k);
    let mut ring = FirstKDescending::new(needed);
    let not_yet = |ring: &FirstKDescending| PalwBeaconDeriveV3Error::NoBeaconYet { slot, needed: needed as u8, found: ring.found() };
    for fact in descending_chain_to_genesis {
        if fact.daa_score >= slot {
            if is_attempt_class_v3(fact.pow_algo_id, attempt_algo_id) {
                ring.push(fact);
            }
            continue;
        }
        if is_attempt_class_v3(fact.pow_algo_id, attempt_algo_id) {
            let (beacon_block, beacon_daa) = ring.fold().ok_or_else(|| not_yet(&ring))?;
            return Ok(PalwBeaconFactV3 { beacon_block, beacon_daa, prev_attempt_daa: fact.daa_score });
        }
    }
    let (beacon_block, beacon_daa) = ring.fold().ok_or_else(|| not_yet(&ring))?;
    Ok(PalwBeaconFactV3 { beacon_block, beacon_daa, prev_attempt_daa: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_freeprompt_v3::validate_beacon_fact_v3;
    use crate::pow_layer0::{POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3};

    const ATTEMPT: u8 = POW_ALGO_ID_PALW_COMMITTED_V2;
    const RECEIPT: u8 = POW_ALGO_ID_PALW_RECEIPT_V3;
    /// The fence-off width: the pre-SA-1 single-block beacon.
    const NO_FOLD: u8 = 1;
    /// The armed width every SA-1 case below uses — the amendment's floor.
    const K3: u8 = PALW_BEACON_FOLD_MIN_K_V1;

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
            NO_FOLD,
            chain(&[(140, ATTEMPT), (130, RECEIPT), (120, ATTEMPT), (110, RECEIPT), (95, ATTEMPT)]),
        )
        .unwrap();
        assert_eq!((fact.beacon_daa, fact.prev_attempt_daa), (120, 95));
        assert_eq!(fact.beacon_block, h64(120));
        validate_beacon_fact_v3(100, &fact).expect("what the chain derived, the validator accepts");

        // A block exactly AT the slot is the beacon — the slot's own score is inside the region.
        let fact = derive_beacon_fact_v3(100, ATTEMPT, NO_FOLD, chain(&[(140, ATTEMPT), (100, ATTEMPT), (90, ATTEMPT)])).unwrap();
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
            derive_beacon_fact_v3(110, ATTEMPT, 1, chain(&[(140, EXEC), (130, RECEIPT), (120, EXEC), (95, ATTEMPT), (80, ATTEMPT)]))
                .unwrap();
        assert_eq!((fact.beacon_daa, fact.prev_attempt_daa), (120, 95), "the post-fence attempt block is a beacon");
        validate_beacon_fact_v3(110, &fact).expect("what the chain derived, the validator accepts");

        // And the witness below the slot is found on the OTHER side of the fence, which is the half
        // that freezes: with only the pre-fence id matched, `prev_attempt_daa` is the last thing
        // that ever moves.
        let fact = derive_beacon_fact_v3(90, ATTEMPT, 1, chain(&[(140, EXEC), (120, EXEC), (85, ATTEMPT)])).unwrap();
        assert_eq!((fact.beacon_daa, fact.prev_attempt_daa), (120, 85));

        // The receipt lane is still never a beacon on either side of the fence — the widening is to
        // the attempt lane's two ids and to nothing else.
        assert_eq!(
            derive_beacon_fact_v3(100, ATTEMPT, 1, chain(&[(160, RECEIPT), (150, RECEIPT), (95, EXEC)])).unwrap_err(),
            PalwBeaconDeriveV3Error::NoBeaconYet { slot: 100, found: 0, needed: 1 }
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
            derive_beacon_fact_v3(100, ATTEMPT, NO_FOLD, chain(&[(160, RECEIPT), (150, RECEIPT), (140, RECEIPT), (95, ATTEMPT)]))
                .unwrap_err();
        assert_eq!(err, PalwBeaconDeriveV3Error::NoBeaconYet { slot: 100, needed: 1, found: 0 });

        // …and one attempt block among them is the beacon, whatever surrounds it.
        let fact =
            derive_beacon_fact_v3(100, ATTEMPT, NO_FOLD, chain(&[(160, RECEIPT), (150, ATTEMPT), (140, RECEIPT), (95, ATTEMPT)]))
                .unwrap();
        assert_eq!(fact.beacon_daa, 150);
    }

    /// A walk that stops before witnessing the predecessor is TOO SHORT, not a fact with a zero
    /// witness — manufacturing the witness would let a truncated walk answer a question it did
    /// not look at.
    #[test]
    fn a_truncated_walk_is_named_not_guessed() {
        let err = derive_beacon_fact_v3(100, ATTEMPT, NO_FOLD, chain(&[(140, ATTEMPT), (120, ATTEMPT)])).unwrap_err();
        assert_eq!(err, PalwBeaconDeriveV3Error::WalkTooShort { slot: 100 });

        // The to-genesis variant may answer with witness 0, because there the absence IS the fact.
        let fact = derive_beacon_fact_to_genesis_v3(100, ATTEMPT, NO_FOLD, chain(&[(140, ATTEMPT), (120, ATTEMPT)])).unwrap();
        assert_eq!((fact.beacon_daa, fact.prev_attempt_daa), (120, 0));
        validate_beacon_fact_v3(100, &fact).unwrap();

        // …and it still refuses when there is no beacon at all.
        assert_eq!(
            derive_beacon_fact_to_genesis_v3(100, ATTEMPT, NO_FOLD, chain(&[(90, ATTEMPT), (80, ATTEMPT)])).unwrap_err(),
            PalwBeaconDeriveV3Error::NoBeaconYet { slot: 100, needed: 1, found: 0 }
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
                // Both regimes: a fold-derived fact must satisfy the same validator the
                // single-block one does, or the fence would ship a fact its own checker rejects.
                for k in [NO_FOLD, K3] {
                    match derive_beacon_fact_v3(slot, ATTEMPT, k, chain(&shape)) {
                        Ok(fact) => {
                            validate_beacon_fact_v3(slot, &fact).unwrap_or_else(|e| {
                                panic!("derived a fact the validator rejects at slot {slot} k {k} on {shape:?}: {e}")
                            });
                            // And the beacon's DAA really is an attempt block from this chain.
                            assert!(
                                shape.iter().any(|(daa, algo)| *daa == fact.beacon_daa && *algo == ATTEMPT),
                                "the beacon must be an attempt block of the walked chain"
                            );
                            if k == NO_FOLD {
                                assert_eq!(fact.beacon_block, h64(fact.beacon_daa), "at k=1 the fact names the block itself");
                            }
                        }
                        Err(_) => { /* a shape with no beacon in range is a legitimate answer */ }
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // ADR-0073 SA-1
    // ---------------------------------------------------------------------------------------

    /// **Fence off is byte-identical.** `k = 1` (and a caller that resolved the fence to nothing)
    /// derives exactly the fact the pre-SA-1 rule did — the block's OWN hash, not a one-element
    /// digest of it. This is the property the shipped presets rest on: none of them arm the fence,
    /// so none of their draws move.
    #[test]
    fn with_the_fence_off_the_beacon_is_the_block_itself() {
        let shape = chain(&[(140, ATTEMPT), (130, RECEIPT), (120, ATTEMPT), (110, RECEIPT), (95, ATTEMPT)]);
        let one = derive_beacon_fact_v3(100, ATTEMPT, 1, shape.clone()).unwrap();
        let zero = derive_beacon_fact_v3(100, ATTEMPT, 0, shape.clone()).unwrap();
        assert_eq!(one, zero, "a fence that resolved to nothing is the pre-SA-1 rule, not an empty fold");
        assert_eq!(one.beacon_block, h64(120), "the fact names the block, so no golden vector moves");
        // And it is NOT the one-element fold — the identity arm is what keeps the old bytes.
        assert_ne!(one.beacon_block, fp_beacon_fold_v3(&[h64(120)]));
    }

    /// **A slot with only `k − 1` attempt blocks after it has not been drawn yet** — and that is
    /// `NoBeaconYet`, reporting what it found, never an invented zero or a shorter fold.
    #[test]
    fn a_fold_short_of_k_is_not_yet_drawn() {
        // Two attempt blocks at or after slot 100, and a witness below it. At k=1 this draws.
        let two_above = chain(&[(120, ATTEMPT), (110, RECEIPT), (105, ATTEMPT), (95, ATTEMPT)]);
        assert!(derive_beacon_fact_v3(100, ATTEMPT, NO_FOLD, two_above.clone()).is_ok(), "one is enough at k=1");
        assert_eq!(
            derive_beacon_fact_v3(100, ATTEMPT, K3, two_above.clone()).unwrap_err(),
            PalwBeaconDeriveV3Error::NoBeaconYet { slot: 100, needed: 3, found: 2 },
            "two of three is not drawn yet, and the answer says so"
        );
        // The to-genesis walk answers the same way: a full walk that found only two really has
        // only two, so this is a fact about the chain and not about the walk.
        assert_eq!(
            derive_beacon_fact_to_genesis_v3(100, ATTEMPT, K3, two_above).unwrap_err(),
            PalwBeaconDeriveV3Error::NoBeaconYet { slot: 100, needed: 3, found: 2 }
        );
        // The third block completes it.
        let three_above = chain(&[(130, ATTEMPT), (120, ATTEMPT), (110, RECEIPT), (105, ATTEMPT), (95, ATTEMPT)]);
        let fact = derive_beacon_fact_v3(100, ATTEMPT, K3, three_above).unwrap();
        assert_eq!(fact.prev_attempt_daa, 95);
        // `beacon_daa` is the k-th — the height at which the draw is DETERMINED, so the use window
        // cannot open against a fold that does not exist yet.
        assert_eq!(fact.beacon_daa, 130);
        validate_beacon_fact_v3(100, &fact).unwrap();
    }

    /// **The fold depends on ALL k blocks**, which is the whole of SA-1: withholding any one of
    /// them changes the draw, so a producer must control every one of the `k` to choose it.
    #[test]
    fn withholding_any_one_of_the_k_moves_the_draw() {
        // slot 100; the first three attempt blocks at or after it are 105, 120, 130.
        let full = chain(&[(140, ATTEMPT), (130, ATTEMPT), (120, ATTEMPT), (105, ATTEMPT), (95, ATTEMPT)]);
        let base = derive_beacon_fact_v3(100, ATTEMPT, K3, full.clone()).unwrap();
        assert_eq!(base.beacon_daa, 130, "the third of the three, not the fourth: 140 is outside the fold");
        assert_eq!(
            base.beacon_block,
            fp_beacon_fold_v3(&[h64(105), h64(120), h64(130)]),
            "the fold is over the first k in ASCENDING chain order"
        );

        // Drop each of the three in turn. Every one of them moves the value.
        for dropped in [105u64, 120, 130] {
            let withheld: Vec<PalwChainBlockFactV3> = full.iter().copied().filter(|f| f.daa_score != dropped).collect();
            let after = derive_beacon_fact_v3(100, ATTEMPT, K3, withheld).unwrap();
            assert_ne!(after.beacon_block, base.beacon_block, "withholding the block at {dropped} left the draw unchanged");
        }

        // Order is load-bearing too: the same three blocks in the other order are a different fold,
        // so a walk that reversed itself could not be mistaken for this one.
        assert_ne!(base.beacon_block, fp_beacon_fold_v3(&[h64(130), h64(120), h64(105)]));
    }

    /// **Receipt blocks are never beacons, fold or no fold** (invariant F15). Receipt hashes are
    /// costlessly malleable by their producers, so a fold that admitted one would be a fold the
    /// producer could re-roll for free — the exact opposite of what widening it is for.
    #[test]
    fn a_receipt_block_is_never_part_of_the_fold() {
        // Three attempt blocks and three receipt blocks interleaved above the slot.
        let mixed =
            chain(&[(135, RECEIPT), (130, ATTEMPT), (125, RECEIPT), (120, ATTEMPT), (115, RECEIPT), (105, ATTEMPT), (95, ATTEMPT)]);
        let fact = derive_beacon_fact_v3(100, ATTEMPT, K3, mixed).unwrap();
        assert_eq!(
            fact.beacon_block,
            fp_beacon_fold_v3(&[h64(105), h64(120), h64(130)]),
            "the receipt blocks between them contribute nothing"
        );

        // A region of receipt blocks alone never completes a fold, however many there are.
        let receipts_only = chain(&[(160, RECEIPT), (150, RECEIPT), (140, RECEIPT), (130, RECEIPT), (95, ATTEMPT)]);
        assert_eq!(
            derive_beacon_fact_v3(100, ATTEMPT, K3, receipts_only).unwrap_err(),
            PalwBeaconDeriveV3Error::NoBeaconYet { slot: 100, needed: 3, found: 0 }
        );
    }

    /// **The predecessor witness survives the fold**, so "the fold begins at the slot" stays
    /// checkable by someone who did not walk: `prev_attempt_daa < slot ≤ beacon_daa` is the same
    /// pair of inequalities `validate_beacon_fact_v3` has always checked, and a fold that started
    /// one attempt block early would have to name a witness at or above the slot to do it.
    #[test]
    fn the_predecessor_witness_still_pins_where_the_fold_starts() {
        let shape = chain(&[(130, ATTEMPT), (120, ATTEMPT), (105, ATTEMPT), (99, ATTEMPT), (90, ATTEMPT)]);
        let fact = derive_beacon_fact_v3(100, ATTEMPT, K3, shape.clone()).unwrap();
        assert_eq!(fact.prev_attempt_daa, 99, "the last attempt block strictly below the slot");
        validate_beacon_fact_v3(100, &fact).unwrap();

        // A fold that began one block early (99,105,120) would have to claim a witness at or above
        // the slot, and the validator refuses exactly that.
        let stolen = PalwBeaconFactV3 {
            beacon_block: fp_beacon_fold_v3(&[h64(99), h64(105), h64(120)]),
            beacon_daa: 120,
            prev_attempt_daa: 105,
        };
        assert!(validate_beacon_fact_v3(100, &stolen).is_err(), "a fold starting below the slot cannot present a witness");

        // And the walk is bounded: only `k` blocks are ever held, whatever the chain does above
        // the slot. A hundred attempt blocks above it still fold the first three.
        let mut tall: Vec<(u64, u8)> = (0..100).rev().map(|i| (200 + i * 7, ATTEMPT)).collect();
        tall.extend_from_slice(&[(130, ATTEMPT), (120, ATTEMPT), (105, ATTEMPT), (95, ATTEMPT)]);
        let fact = derive_beacon_fact_v3(100, ATTEMPT, K3, chain(&tall)).unwrap();
        assert_eq!(fact.beacon_block, fp_beacon_fold_v3(&[h64(105), h64(120), h64(130)]));
        assert_eq!(fact.beacon_daa, 130);
    }

    /// **The fold is a function of the CHAIN, not of where the walker started.** Two nodes on one
    /// candidate at different heights derive one fact — the property that makes the beacon a
    /// consensus value at all, and the one a fold could most easily have broken (a later block
    /// must not join a fold that is already complete).
    #[test]
    fn the_fold_does_not_move_as_the_chain_grows() {
        let base: Vec<(u64, u8)> = vec![(130, ATTEMPT), (120, ATTEMPT), (105, ATTEMPT), (95, ATTEMPT)];
        let settled = derive_beacon_fact_v3(100, ATTEMPT, K3, chain(&base)).unwrap();
        for grown in 1..=5u64 {
            let mut taller: Vec<(u64, u8)> = (0..grown).rev().map(|i| (140 + i * 10, ATTEMPT)).collect();
            taller.extend_from_slice(&base);
            assert_eq!(
                derive_beacon_fact_v3(100, ATTEMPT, K3, chain(&taller)).unwrap(),
                settled,
                "{grown} more blocks on top changed a draw that was already complete"
            );
        }
    }

    /// **The fold and the fence, together — the one place neither amendment was tested.**
    ///
    /// ADR-0073 SA-1 (the ring folds `k` attempt blocks) and ADR-0072 SA-3/SA-4 (an attempt block
    /// is attempt-class on EITHER side of the activation) were written by two lanes, for two
    /// reasons, over the same walk. Each lane's own tests hold: the fold cases all run on one algo
    /// id, and the fence cases all run at `k = 1`, which is the fold's IDENTITY and therefore
    /// asserts nothing about folding. Their confluence — a fold whose `k` blocks straddle the
    /// fence — belonged to neither, and it is the only shape a live network actually produces:
    /// a chain re-minted onto the execution-priced lane keeps its pre-fence history forever.
    ///
    /// Taking either amendment alone loses a network here, which is why this is an assertion and
    /// not a comment:
    ///
    /// * the ring with an EXACT id comparison collects only the post-fence blocks, so a slot whose
    ///   `k`-th block is below the fence never completes — `NoBeaconYet` for the rest of the
    ///   chain's life, and every ADR-0044 receipt spend after it is undrawable;
    /// * the predicate without the ring answers with the first block alone, which is the pre-SA-1
    ///   rule wearing the amendment's name.
    #[test]
    fn a_fold_whose_blocks_straddle_the_fence_is_still_a_fold() {
        const EXEC: u8 = crate::pow_layer0::POW_ALGO_ID_PALW_EXEC_V3;
        // Slot 100, fence somewhere in (105, 120]: 130 and 120 are post-fence attempt blocks,
        // 105 is a pre-fence one, and 95 is the predecessor witness below the slot.
        let straddling = chain(&[(130, EXEC), (125, RECEIPT), (120, EXEC), (105, ATTEMPT), (95, ATTEMPT)]);
        let fact = derive_beacon_fact_v3(100, ATTEMPT, K3, straddling).unwrap();

        // All three attempt blocks joined the fold, in ascending chain order, and the draw is
        // determined at the k-th — the same answer the same chain gives with one id throughout.
        assert_eq!(fact.beacon_block, fp_beacon_fold_v3(&[h64(105), h64(120), h64(130)]));
        assert_eq!((fact.beacon_daa, fact.prev_attempt_daa), (130, 95));
        assert_eq!(
            fact,
            derive_beacon_fact_v3(100, ATTEMPT, K3, chain(&[(130, ATTEMPT), (125, RECEIPT), (120, ATTEMPT), (105, ATTEMPT), (95, ATTEMPT)]))
                .unwrap(),
            "the fence changed the ANSWER, not just which ids the walk recognises"
        );
        validate_beacon_fact_v3(100, &fact).expect("what the chain derived, the validator accepts");

        // And the failure the union prevents, stated as its own case: with only two blocks above
        // the slot once the pre-fence one is excluded, an exact-comparison walk would report a
        // short fold. The union reports a complete one.
        let two_above = chain(&[(130, EXEC), (120, EXEC), (105, ATTEMPT), (95, ATTEMPT)]);
        assert_eq!(derive_beacon_fact_v3(100, ATTEMPT, K3, two_above).unwrap().beacon_daa, 130);

        // The fold still refuses to complete when the chain genuinely lacks `k` attempt blocks,
        // on either lane — the predicate widens what counts, never how many are needed.
        assert_eq!(
            derive_beacon_fact_v3(100, ATTEMPT, K3, chain(&[(130, EXEC), (120, RECEIPT), (95, ATTEMPT)])).unwrap_err(),
            PalwBeaconDeriveV3Error::NoBeaconYet { slot: 100, needed: 3, found: 1 }
        );
    }
}
