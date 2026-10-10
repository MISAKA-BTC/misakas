//! **RFC-0008 v2, amendment 1 §10.3: the EXEC v2 lane's relay admission** — node-local backpressure, never a validity rule.
//!
//! Past `palw_exec_payload_v2` a lane block costs its sender the constant algo-10 PoW and one ML-DSA-87 signature; the cost a flood can
//! still push onto the network is gossip amplification. A node therefore announces onward:
//!
//! * an `EXEC_TX` block only if it is the first it validated for its `(anchor span, round, permit index)` — a second block of a permit is
//!   kept, not announced (the fold's canonical order already makes it `PermitAlreadyUsed`; §10.6: a v2 permit signed twice is NOT an
//!   offence — an executor re-signs at a new anchor after a strand — so nothing is filed);
//! * an `EXEC_SLICE` block only if, against its sink state, the root is open, the executor authorised, the index inside
//!   `[next, next + pending depth)` and the verification claim held by the kernel route (`ConsensusApi::palw_exec_v2_slice_relayable_v1`),
//!   **and** its executor has announced fewer than [`PALW_EXEC_V2_RELAY_SLICES_PER_BOND_SPAN`] slice blocks in the anchor's span.
//!
//! A block the policy does not announce is still validated and stored if a peer sends it; validity never reads this. Bounded: the
//! memory keeps the newest [`PALW_EXEC_V2_RELAY_KEEP_SPANS`] spans.

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_work_slice_v2::PALW_EXEC_V2_MAX_PENDING_PER_BOND;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// The most `EXEC_SLICE` blocks one executor bond has announced per anchor span (the per-bond pending bound: more could never be
/// accepted before the next verification).
pub const PALW_EXEC_V2_RELAY_SLICES_PER_BOND_SPAN: u32 = PALW_EXEC_V2_MAX_PENDING_PER_BOND;
/// How many spans (the newest included) the relay memory keeps: the anchoring window is two spans.
pub const PALW_EXEC_V2_RELAY_KEEP_SPANS: u64 = 3;

/// What the v2 relay policy says about a validated lane block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwExecV2RelayVerdictV1 {
    /// Announce it.
    Relay,
    /// A second `EXEC_TX` block for a permit already announced: keep, do not announce.
    PermitRepeat,
    /// The executor has announced its quota of slice blocks in this span.
    OverQuota,
}

#[derive(Default)]
struct Inner {
    permits: BTreeMap<(u64, u64, u16), BlockHash>,
    quota: BTreeMap<(u64, PalwBondKeyV2), u32>,
    newest_span: u64,
}

impl Inner {
    fn prune(&mut self, span: u64) {
        if span <= self.newest_span {
            return;
        }
        self.newest_span = span;
        let floor = span.saturating_sub(PALW_EXEC_V2_RELAY_KEEP_SPANS - 1);
        self.permits.retain(|(s, _, _), _| *s >= floor);
        self.quota.retain(|(s, _), _| *s >= floor);
    }
}

/// The relay memory (one per node).
#[derive(Default)]
pub struct PalwExecV2RelayV1 {
    inner: Mutex<Inner>,
}

impl PalwExecV2RelayV1 {
    /// An `EXEC_TX` block `block` for `(span, round, permit index)`.
    pub fn observe_permit(&self, block: BlockHash, span: u64, round: u64, permit_index: u16) -> PalwExecV2RelayVerdictV1 {
        let mut inner = self.inner.lock().unwrap();
        inner.prune(span);
        match inner.permits.get(&(span, round, permit_index)) {
            Some(first) if *first != block => PalwExecV2RelayVerdictV1::PermitRepeat,
            Some(_) => PalwExecV2RelayVerdictV1::Relay,
            None => {
                inner.permits.insert((span, round, permit_index), block);
                PalwExecV2RelayVerdictV1::Relay
            }
        }
    }

    /// An `EXEC_SLICE` block by `executor` anchored in `span` (already judged relayable against the sink state): takes one of the
    /// executor's announcements for the span.
    pub fn take_slice_quota(&self, executor: PalwBondKeyV2, span: u64) -> PalwExecV2RelayVerdictV1 {
        let mut inner = self.inner.lock().unwrap();
        inner.prune(span);
        let used = inner.quota.entry((span, executor)).or_insert(0);
        if *used >= PALW_EXEC_V2_RELAY_SLICES_PER_BOND_SPAN {
            return PalwExecV2RelayVerdictV1::OverQuota;
        }
        *used += 1;
        PalwExecV2RelayVerdictV1::Relay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    fn bond(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(v), 0))
    }

    /// **The first block of a v2 permit is announced, a different second one is kept, and the memory forgets old spans.** The same
    /// block seen twice (from two peers) is announced as itself.
    #[test]
    fn a_permit_is_announced_once_per_span_and_old_spans_are_forgotten() {
        let relay = PalwExecV2RelayV1::default();
        let (a, b) = (BlockHash::from_u64_word(1), BlockHash::from_u64_word(2));
        assert_eq!(relay.observe_permit(a, 10, 7, 0), PalwExecV2RelayVerdictV1::Relay);
        assert_eq!(relay.observe_permit(a, 10, 7, 0), PalwExecV2RelayVerdictV1::Relay, "the same block again");
        assert_eq!(relay.observe_permit(b, 10, 7, 0), PalwExecV2RelayVerdictV1::PermitRepeat, "a second block of the permit");
        assert_eq!(relay.observe_permit(b, 10, 7, 1), PalwExecV2RelayVerdictV1::Relay, "another permit");
        // A re-signed carrier at a new anchor (another span) is an honest reattachment: announced.
        assert_eq!(relay.observe_permit(b, 11, 7, 0), PalwExecV2RelayVerdictV1::Relay);
        // Three spans on, span 10 is forgotten.
        relay.observe_permit(BlockHash::from_u64_word(9), 13, 1, 0);
        assert_eq!(relay.observe_permit(b, 10, 7, 0), PalwExecV2RelayVerdictV1::Relay, "a forgotten span");
    }

    /// **An executor announces at most its quota of slice blocks per span**; another executor and the next span are unaffected.
    #[test]
    fn an_executor_announces_at_most_its_quota_of_slice_blocks_per_span() {
        let relay = PalwExecV2RelayV1::default();
        for _ in 0..PALW_EXEC_V2_RELAY_SLICES_PER_BOND_SPAN {
            assert_eq!(relay.take_slice_quota(bond(1), 5), PalwExecV2RelayVerdictV1::Relay);
        }
        assert_eq!(relay.take_slice_quota(bond(1), 5), PalwExecV2RelayVerdictV1::OverQuota, "a flood from one bond stops here");
        assert_eq!(relay.take_slice_quota(bond(2), 5), PalwExecV2RelayVerdictV1::Relay, "another executor");
        assert_eq!(relay.take_slice_quota(bond(1), 6), PalwExecV2RelayVerdictV1::Relay, "the next span");
    }
}
