//! **ADR-0160 lane verify V1, the node's side of the batch licence** (node policy; a child of
//! `palw_panel`). Nothing here is a rule: the fold judges whatever arrives, and below
//! `Params::palw_capacity_batch_licence` the consensus assembler answers `None` and none of this runs.
//!
//! * **A seat signs one window root a DAA** ([`PalwReceiptWindowSignerV1`]): every V3 receipt the seat
//!   signs is also recorded as a leaf (its claim, the claim's panel anchor, the verdict, the DAA and
//!   the mask), and once a DAA has passed its leaves are sealed into one [`PalwSeatWindowV1`] — one
//!   ML-DSA-87 signature over the root instead of one per (seat, claim).
//! * **A collector pools windows** ([`PalwBatchWindowPoolV1`]) — its own seats' and the ones it hears —
//!   keeps them while any leaf's claim could still be licensed (the receipt window), and
//! * **assembles one batch a block** ([`palw_batch_licence_offer_v1`]): the due claims in the V04 order
//!   (oldest bind first, [`crate::palw_licence_order::palw_licence_claim_order_v1`]), through the
//!   consensus assembler (`ConsensusApi::palw_v2_batch_licence_assemble`: each window verified, each
//!   entry kept only where acceptance takes it and the fold licenses it), within one standard carrier.
//!
//! **Not wired here: the gossip of windows.** A coverage licence needs at least three seats'
//! receipts, so a collector that holds only its own seat's windows batches nothing; the P2P message
//! that relays a [`PalwSeatWindowV1`] (root, signature, leaves) the way `palw_gossip` relays V3
//! receipts is the next node change of this lane. The capacity harness drives these functions with
//! every seat's windows (`t12_capacity_verify`).

use std::collections::BTreeMap;

use kaspa_consensus_core::palw_batch_licence_v1::{PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT, PalwSeatWindowV1, PalwWindowLeafV1};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_hashes::Hash64;

/// The most payload bytes one batch carrier takes: a standard transaction's transient mass
/// (`MAXIMUM_STANDARD_TRANSACTION_MASS` = 480,000, four mass a byte) less a carrier's own inputs,
/// output and signature.
pub(crate) const PALW_BATCH_LICENCE_MAX_BYTES_V1: usize = 480_000 / 4 - 8_000;

/// How long a pooled window is kept past its last DAA: a claim's receipt window (`window_receipt`,
/// 600 on testnet-12) — a leaf older than that can license nothing.
pub(crate) const PALW_BATCH_WINDOW_KEEP_DAA_V1: u64 = 600;

/// **One seat's leaves, grouped by the DAA they were signed at, sealed one root a DAA.**
#[derive(Debug, Default)]
pub(crate) struct PalwReceiptWindowSignerV1 {
    pending: BTreeMap<u64, Vec<PalwWindowLeafV1>>,
}

impl PalwReceiptWindowSignerV1 {
    /// Record a receipt this seat signed (its leaf). A receipt recorded twice is kept once.
    pub(crate) fn record(&mut self, leaf: PalwWindowLeafV1) {
        let leaves = self.pending.entry(leaf.signed_daa).or_default();
        if !leaves.contains(&leaf) {
            leaves.push(leaf);
        }
    }

    /// **Seal every DAA strictly below `now_daa`** into one signed window each (`sign` is the seat's
    /// ML-DSA-87 signer under [`PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT`]); the current DAA stays open.
    pub(crate) fn seal(
        &mut self,
        network_domain: Hash64,
        seat: PalwBondKeyV2,
        now_daa: u64,
        sign: &dyn Fn(&[u8], &[u8]) -> Vec<u8>,
    ) -> Vec<PalwSeatWindowV1> {
        let open = self.pending.split_off(&now_daa);
        let sealed = std::mem::replace(&mut self.pending, open);
        sealed
            .into_iter()
            .filter_map(|(daa, leaves)| {
                PalwSeatWindowV1::sign(network_domain, seat, daa, daa, leaves, |message| sign(message, PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT))
            })
            .collect()
    }

    pub(crate) fn pending_leaves(&self) -> usize {
        self.pending.values().map(Vec::len).sum()
    }
}

/// **The windows a collector holds**, one entry per (seat, root).
#[derive(Debug, Default)]
pub(crate) struct PalwBatchWindowPoolV1 {
    windows: BTreeMap<(PalwBondKeyV2, Hash64), PalwSeatWindowV1>,
}

impl PalwBatchWindowPoolV1 {
    /// Pool a window (a second copy of the same root is ignored).
    pub(crate) fn insert(&mut self, window: PalwSeatWindowV1) {
        self.windows.entry((window.root.seat_bond, window.root.root)).or_insert(window);
    }

    /// Drop every window whose last DAA is more than [`PALW_BATCH_WINDOW_KEEP_DAA_V1`] behind `now_daa`.
    pub(crate) fn prune(&mut self, now_daa: u64) {
        self.windows.retain(|_, window| window.root.to_daa.saturating_add(PALW_BATCH_WINDOW_KEEP_DAA_V1) >= now_daa);
    }

    pub(crate) fn windows(&self) -> Vec<PalwSeatWindowV1> {
        self.windows.values().cloned().collect()
    }

    /// The claims any pooled leaf names — what the collector orders into `due`.
    pub(crate) fn claims(&self) -> Vec<Hash64> {
        let mut claims: Vec<Hash64> = self.windows.values().flat_map(|window| window.leaves.iter().map(|leaf| leaf.claim)).collect();
        claims.sort_unstable();
        claims.dedup();
        claims
    }

    pub(crate) fn len(&self) -> usize {
        self.windows.len()
    }
}

/// **The collector's batch for this tick**: `due` (already in the V04 order) over the pool, asked of
/// the consensus assembler within [`PALW_BATCH_LICENCE_MAX_BYTES_V1`]. `None` below the fence or
/// when nothing licenses.
pub(crate) fn palw_batch_licence_offer_v1(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    pool: &PalwBatchWindowPoolV1,
    due: Vec<Hash64>,
) -> Option<PalwConsensusObjectV2> {
    if pool.len() == 0 || due.is_empty() {
        return None;
    }
    consensus.palw_v2_batch_licence_assemble(pool.windows(), due, PALW_BATCH_LICENCE_MAX_BYTES_V1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_batch_licence_v1::{palw_receipt_window_fold_v1, palw_receipt_window_message_v1, palw_receipt_window_path_v1};
    use kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2;
    use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
    use kaspa_consensus_core::tx::TransactionOutpoint;

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn leaf(claim: u64, daa: u64) -> PalwWindowLeafV1 {
        PalwWindowLeafV1 { claim: h(claim), anchor_hash: h(0xA0 + claim), verdict: PalwReceiptVerdictV2::Valid, signed_daa: daa, mask: PalwSegmentMaskV2(1) }
    }

    /// A seat's receipts become one signed root a DAA; the open DAA stays open; the "signature" (the
    /// message itself here) is over exactly the window message, and every leaf folds to its root.
    #[test]
    fn a_seat_seals_one_root_a_daa_and_keeps_the_open_one() {
        let seat = PalwBondKeyV2(TransactionOutpoint::new(h(0x5EA7), 1));
        let domain = h(0xD0);
        let mut signer = PalwReceiptWindowSignerV1::default();
        for (claim, daa) in [(1, 10), (2, 10), (3, 11), (4, 12), (2, 10)] {
            signer.record(leaf(claim, daa));
        }
        assert_eq!(signer.pending_leaves(), 4, "a receipt recorded twice is one leaf");
        let sign = |message: &[u8], context: &[u8]| {
            assert_eq!(context, PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT);
            message.to_vec()
        };
        let sealed = signer.seal(domain, seat, 12, &sign);
        assert_eq!(sealed.len(), 2, "DAA 10 and 11 sealed, 12 still open");
        assert_eq!(signer.pending_leaves(), 1);
        for window in &sealed {
            let r = &window.root;
            assert_eq!((r.from_daa, r.to_daa), (window.leaves[0].signed_daa, window.leaves[0].signed_daa));
            assert_eq!(r.signature, palw_receipt_window_message_v1(domain, &seat, r.from_daa, r.to_daa, r.root, r.count).as_byte_slice());
            let hashes: Vec<Hash64> = window.leaves.iter().map(|l| l.leaf(domain)).collect();
            for (i, l) in hashes.iter().enumerate() {
                let path = palw_receipt_window_path_v1(&hashes, i).unwrap();
                assert_eq!(palw_receipt_window_fold_v1(*l, i as u32, r.count, &path), Some(r.root));
            }
        }
        assert!(signer.seal(domain, seat, 12, &sign).is_empty(), "nothing new below 12");
        assert_eq!(signer.seal(domain, seat, 13, &sign).len(), 1);
    }

    #[test]
    fn the_pool_keeps_one_copy_and_prunes_past_the_receipt_window() {
        let seat = PalwBondKeyV2(TransactionOutpoint::new(h(0x5EA7), 2));
        let mut signer = PalwReceiptWindowSignerV1::default();
        signer.record(leaf(1, 10));
        signer.record(leaf(2, 700));
        let sealed = signer.seal(h(0xD0), seat, 701, &|m, _| m.to_vec());
        let mut pool = PalwBatchWindowPoolV1::default();
        for w in sealed.iter().chain(sealed.iter()) {
            pool.insert(w.clone());
        }
        assert_eq!(pool.len(), 2, "one copy of each root");
        assert_eq!(pool.claims(), vec![h(1), h(2)]);
        pool.prune(10 + PALW_BATCH_WINDOW_KEEP_DAA_V1);
        assert_eq!(pool.len(), 2, "kept through the receipt window");
        pool.prune(11 + PALW_BATCH_WINDOW_KEEP_DAA_V1);
        assert_eq!(pool.claims(), vec![h(2)], "the DAA-10 window leaves once no leaf can license");
    }
}
