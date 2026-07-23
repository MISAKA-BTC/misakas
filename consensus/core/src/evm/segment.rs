//! EVM data segments and their retention — the vocabulary the segment pruner uses.
//!
//! The 144 GB incident had two halves. The first was writing too much (206, and
//! then 221). The second is this one: EVM data was reclaimed *only* by the L1
//! pruning processor, which correctly refuses to run while consensus is
//! transitional or virtual has not caught up. That is exactly the IBD window, so
//! the node spent its highest-write period with reclamation switched off.
//!
//! Most EVM data does not need that gate. A receipt, a log posting, a raw
//! transaction and a trace plan are RPC data: deleting one cannot make a block
//! validate differently. Only a few stores are load-bearing for execution or for
//! reorg safety. Separating them lets retention run continuously — including
//! during IBD, which is when it matters most — without touching anything
//! consensus depends on.
//!
//! Segments also give retention a vocabulary an operator can reason about:
//! "receipts for 7 days, no traces, logs off" is a sentence about this enum.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// One independently-prunable class of EVM data.
///
/// Ordered by prefix so the enum reads against `DatabaseStorePrefixes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "kebab-case")]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum EvmPruneSegment {
    /// 203 — per-block receipts. RPC only.
    Receipts = 0,
    /// 204 — `tx_hash → locations`. RPC only.
    TransactionLookup = 1,
    /// 205 — address/topic posting index. RPC only, and the largest multiplier of
    /// the group: one log yields up to five postings.
    LogPostings = 2,
    /// 210 — `evm_block_hash → l1_hash`. RPC only.
    BlockHashMap = 3,
    /// 213 — `evm_number → l1_hash`, canonical. RPC only.
    NumberIndex = 4,
    /// 217 — raw transaction bytes. RPC only, and duplicated in 211.
    RawTransactions = 5,
    /// 219 — `debug_traceTransaction` replay plans. Debug RPC only.
    TraceReplay = 6,
    /// 220 — forward state diffs. Reorg/state-history safety.
    StateDiffs = 7,
    /// 221/223 — reconstruction anchors. Reorg/state-history safety.
    StateAnchors = 8,
    /// 232 — `block → state_root`. Reorg/state-history safety.
    BlockStateRoots = 9,
    /// 222 — content-addressed bytecode. Live state; reclaimed by mark-and-sweep,
    /// never by block range, because entries are SHARED.
    CodeGc = 10,
}

impl EvmPruneSegment {
    pub const ALL: [EvmPruneSegment; 11] = [
        Self::Receipts,
        Self::TransactionLookup,
        Self::LogPostings,
        Self::BlockHashMap,
        Self::NumberIndex,
        Self::RawTransactions,
        Self::TraceReplay,
        Self::StateDiffs,
        Self::StateAnchors,
        Self::BlockStateRoots,
        Self::CodeGc,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Receipts => "receipts",
            Self::TransactionLookup => "transaction-lookup",
            Self::LogPostings => "log-postings",
            Self::BlockHashMap => "block-hash-map",
            Self::NumberIndex => "number-index",
            Self::RawTransactions => "raw-transactions",
            Self::TraceReplay => "trace-replay",
            Self::StateDiffs => "state-diffs",
            Self::StateAnchors => "state-anchors",
            Self::BlockStateRoots => "block-state-roots",
            Self::CodeGc => "code-gc",
        }
    }

    /// Whether this segment may be reclaimed while consensus is transitional or
    /// catching up.
    ///
    /// True for RPC-only data: deleting it cannot change how a block validates,
    /// so waiting for L1 pruning buys nothing and costs the entire IBD window.
    /// False for anything execution or reorg-recovery reads — those follow the
    /// L1 pruning point, and no disk-pressure argument overrides that.
    pub fn prunable_during_ibd(self) -> bool {
        match self {
            Self::Receipts
            | Self::TransactionLookup
            | Self::LogPostings
            | Self::BlockHashMap
            | Self::NumberIndex
            | Self::RawTransactions
            | Self::TraceReplay => true,
            Self::StateDiffs | Self::StateAnchors | Self::BlockStateRoots | Self::CodeGc => false,
        }
    }

    /// Whether the segment is rebuildable from data the node still holds — a
    /// pruned RPC index can be reconstructed by replaying retained blocks, while
    /// a pruned state diff cannot be recovered at all.
    pub fn rebuildable(self) -> bool {
        self.prunable_during_ibd()
    }
}

/// How much of a segment to keep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SegmentRetention {
    /// Never written and eagerly reclaimed. For data a node's role does not need
    /// at all — a validator serving no debug RPC has no use for trace plans.
    Off,
    /// Keep this many EVM blocks back from the canonical head.
    Blocks(u64),
    /// Keep this long back from the canonical head, in milliseconds. Portable
    /// across BPS changes in a way a block count is not.
    Duration { ms: u64 },
    /// Never reclaimed.
    Archive,
}

impl SegmentRetention {
    pub fn is_off(self) -> bool {
        matches!(self, Self::Off)
    }

    pub fn is_archive(self) -> bool {
        matches!(self, Self::Archive)
    }

    /// The oldest EVM number to KEEP, given the head and the observed block rate.
    ///
    /// `None` means "keep everything". `blocks_per_second` converts a duration
    /// into a block distance; it is measured rather than assumed, because the
    /// whole point of expressing retention in time is that the block rate is not
    /// a constant of the system.
    pub fn keep_from(self, head_evm_number: u64, blocks_per_second: f64) -> Option<u64> {
        match self {
            // `Off` keeps nothing, so everything up to and including the head is
            // reclaimable. Saturating at the head rather than u64::MAX keeps the
            // caller's arithmetic honest.
            Self::Off => Some(head_evm_number.saturating_add(1)),
            Self::Archive => None,
            Self::Blocks(n) => Some(head_evm_number.saturating_sub(n)),
            Self::Duration { ms } => {
                let rate = if blocks_per_second.is_finite() && blocks_per_second > 0.0 { blocks_per_second } else { 1.0 };
                let blocks = (ms as f64 / 1000.0 * rate).min(u64::MAX as f64) as u64;
                Some(head_evm_number.saturating_sub(blocks))
            }
        }
    }
}

/// The node's role, as a retention posture across every segment.
///
/// Named after what a node is FOR rather than after a size, because that is the
/// question an operator can actually answer about their own deployment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvmNodeRole {
    /// Validates and attests; serves no history. Keeps only what execution and
    /// reorg recovery need.
    CompactValidator,
    /// Serves recent RPC history. The default: it is what most operators want and
    /// it is bounded.
    #[default]
    RpcRecent,
    /// Serves everything, forever. Explicit opt-in — this is the posture that
    /// makes a node grow without bound, and it should be a decision, not a
    /// default someone inherits.
    ArchiveIndexer,
}

impl EvmNodeRole {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "compact-validator" | "compact" => Some(Self::CompactValidator),
            "rpc-recent" | "recent" => Some(Self::RpcRecent),
            "archive-indexer" | "archive" => Some(Self::ArchiveIndexer),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::CompactValidator => "compact-validator",
            Self::RpcRecent => "rpc-recent",
            Self::ArchiveIndexer => "archive-indexer",
        }
    }
}

const HOUR_MS: u64 = 60 * 60 * 1000;
const DAY_MS: u64 = 24 * HOUR_MS;

/// Per-segment retention, plus the knobs the pruner's scheduler needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmRetentionPolicy {
    pub role: EvmNodeRole,
    pub receipts: SegmentRetention,
    pub transaction_lookup: SegmentRetention,
    pub log_postings: SegmentRetention,
    pub block_hash_map: SegmentRetention,
    pub number_index: SegmentRetention,
    pub raw_transactions: SegmentRetention,
    pub trace_replay: SegmentRetention,
    /// Reorg-safety floor for the state-history segments. Expressed in blocks
    /// because what it protects against — a reorg — is measured in blocks.
    pub state_safety_blocks: u64,
    /// Rows a single pruner pass may delete. Bounds the latency each pass adds to
    /// the node, so retention is a background trickle and not a stall.
    pub batch_rows: usize,
    /// How often a pass runs.
    pub interval_ms: u64,
}

impl Default for EvmRetentionPolicy {
    fn default() -> Self {
        Self::for_role(EvmNodeRole::default())
    }
}

impl EvmRetentionPolicy {
    pub fn for_role(role: EvmNodeRole) -> Self {
        // The state-safety floor is the same in every role: it is a correctness
        // bound, not a preference. Only the RPC segments differ.
        let state_safety_blocks = 100_000;
        match role {
            EvmNodeRole::CompactValidator => Self {
                role,
                // Short, not off: a validator still answers the occasional receipt
                // query, and a few hours costs almost nothing.
                receipts: SegmentRetention::Duration { ms: 6 * HOUR_MS },
                transaction_lookup: SegmentRetention::Duration { ms: 6 * HOUR_MS },
                // The biggest index and the one a validator never queries.
                log_postings: SegmentRetention::Off,
                block_hash_map: SegmentRetention::Duration { ms: 6 * HOUR_MS },
                number_index: SegmentRetention::Duration { ms: 6 * HOUR_MS },
                raw_transactions: SegmentRetention::Duration { ms: 6 * HOUR_MS },
                trace_replay: SegmentRetention::Off,
                state_safety_blocks,
                batch_rows: 4096,
                interval_ms: 30_000,
            },
            EvmNodeRole::RpcRecent => Self {
                role,
                receipts: SegmentRetention::Duration { ms: 7 * DAY_MS },
                transaction_lookup: SegmentRetention::Duration { ms: 7 * DAY_MS },
                log_postings: SegmentRetention::Duration { ms: 7 * DAY_MS },
                block_hash_map: SegmentRetention::Duration { ms: 7 * DAY_MS },
                number_index: SegmentRetention::Duration { ms: 7 * DAY_MS },
                raw_transactions: SegmentRetention::Duration { ms: 7 * DAY_MS },
                // Trace plans are large and serve one debug endpoint; a short
                // window covers the "what did this tx do" case without the cost.
                trace_replay: SegmentRetention::Duration { ms: 2 * HOUR_MS },
                state_safety_blocks,
                batch_rows: 4096,
                interval_ms: 30_000,
            },
            EvmNodeRole::ArchiveIndexer => Self {
                role,
                receipts: SegmentRetention::Archive,
                transaction_lookup: SegmentRetention::Archive,
                log_postings: SegmentRetention::Archive,
                block_hash_map: SegmentRetention::Archive,
                number_index: SegmentRetention::Archive,
                raw_transactions: SegmentRetention::Archive,
                trace_replay: SegmentRetention::Archive,
                state_safety_blocks,
                batch_rows: 4096,
                interval_ms: 60_000,
            },
        }
    }

    pub fn retention(&self, segment: EvmPruneSegment) -> SegmentRetention {
        match segment {
            EvmPruneSegment::Receipts => self.receipts,
            EvmPruneSegment::TransactionLookup => self.transaction_lookup,
            EvmPruneSegment::LogPostings => self.log_postings,
            EvmPruneSegment::BlockHashMap => self.block_hash_map,
            EvmPruneSegment::NumberIndex => self.number_index,
            EvmPruneSegment::RawTransactions => self.raw_transactions,
            EvmPruneSegment::TraceReplay => self.trace_replay,
            // The state segments follow the reorg-safety floor, and are
            // additionally gated on the L1 pruning point by the caller.
            EvmPruneSegment::StateDiffs | EvmPruneSegment::StateAnchors | EvmPruneSegment::BlockStateRoots => {
                if self.role == EvmNodeRole::ArchiveIndexer {
                    SegmentRetention::Archive
                } else {
                    SegmentRetention::Blocks(self.state_safety_blocks)
                }
            }
            // Never a block range: 222 entries are shared, so only reachability
            // decides, and that is the mark-and-sweep pass.
            EvmPruneSegment::CodeGc => SegmentRetention::Archive,
        }
    }

    /// Whether a segment's data should be WRITTEN at all.
    ///
    /// Not writing beats writing-then-deleting: it saves the write, the WAL, the
    /// compaction and the tombstone. This is the whole benefit of `Off` for trace
    /// plans and log postings on a validator.
    pub fn writes(&self, segment: EvmPruneSegment) -> bool {
        !self.retention(segment).is_off()
    }
}

/// Per-segment progress. Persisted (prefix 225) so a pass is resumable and so
/// RPC can answer "from where is this actually available" instead of returning an
/// empty result that reads like "nothing happened here".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmPruneCursor {
    /// Everything strictly below this EVM number has been reclaimed.
    pub pruned_through: u64,
    /// The oldest EVM number still answerable from this segment. What an RPC
    /// `history pruned` error reports, so a caller can retry against an archive
    /// node instead of guessing.
    pub available_from: u64,
    pub last_run_unix_ms: u64,
    pub rows_deleted_total: u64,
    pub format_version: u16,
}

pub const EVM_PRUNE_CURSOR_FORMAT: u16 = 1;

impl kaspa_utils::mem_size::MemSizeEstimator for EvmPruneCursor {}

impl EvmPruneCursor {
    pub fn new() -> Self {
        Self { format_version: EVM_PRUNE_CURSOR_FORMAT, ..Default::default() }
    }

    /// Advance after a pass. `available_from` only ever RISES: it is a floor on
    /// what the node can answer, and lowering it would advertise data that has
    /// already been deleted.
    pub fn advance(&mut self, pruned_through: u64, rows_deleted: u64, now_ms: u64) {
        self.pruned_through = self.pruned_through.max(pruned_through);
        self.available_from = self.available_from.max(pruned_through);
        self.rows_deleted_total = self.rows_deleted_total.saturating_add(rows_deleted);
        self.last_run_unix_ms = now_ms;
        self.format_version = EVM_PRUNE_CURSOR_FORMAT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_segments_prune_during_ibd_and_state_segments_do_not() {
        // The core of the fix: the segments that grew fastest during IBD are
        // exactly the ones that do not need the L1 pruning gate.
        for s in [
            EvmPruneSegment::Receipts,
            EvmPruneSegment::TransactionLookup,
            EvmPruneSegment::LogPostings,
            EvmPruneSegment::BlockHashMap,
            EvmPruneSegment::NumberIndex,
            EvmPruneSegment::RawTransactions,
            EvmPruneSegment::TraceReplay,
        ] {
            assert!(s.prunable_during_ibd(), "{} is RPC-only and must not wait for L1 pruning", s.as_str());
        }
        for s in
            [EvmPruneSegment::StateDiffs, EvmPruneSegment::StateAnchors, EvmPruneSegment::BlockStateRoots, EvmPruneSegment::CodeGc]
        {
            assert!(!s.prunable_during_ibd(), "{} is load-bearing and must follow the L1 pruning point", s.as_str());
        }
    }

    #[test]
    fn every_segment_is_covered_by_the_policy_lookup() {
        // A segment added without a retention entry would silently inherit
        // whatever the match arm defaults to; enumerate to force the choice.
        let policy = EvmRetentionPolicy::default();
        for s in EvmPruneSegment::ALL {
            let _ = policy.retention(s);
        }
        assert_eq!(EvmPruneSegment::ALL.len(), 11);
    }

    #[test]
    fn compact_validator_turns_off_the_indexes_it_never_reads() {
        let policy = EvmRetentionPolicy::for_role(EvmNodeRole::CompactValidator);
        assert!(policy.retention(EvmPruneSegment::LogPostings).is_off());
        assert!(policy.retention(EvmPruneSegment::TraceReplay).is_off());
        // And `Off` means NOT WRITTEN, not written-then-deleted.
        assert!(!policy.writes(EvmPruneSegment::LogPostings));
        assert!(!policy.writes(EvmPruneSegment::TraceReplay));
        assert!(policy.writes(EvmPruneSegment::Receipts));
    }

    #[test]
    fn archive_never_reclaims_anything() {
        let policy = EvmRetentionPolicy::for_role(EvmNodeRole::ArchiveIndexer);
        for s in EvmPruneSegment::ALL {
            assert!(policy.retention(s).is_archive(), "{} must be retained in the archive role", s.as_str());
            assert!(policy.retention(s).keep_from(1_000_000, 10.0).is_none());
        }
    }

    #[test]
    fn state_segments_keep_a_reorg_floor_in_every_non_archive_role() {
        for role in [EvmNodeRole::CompactValidator, EvmNodeRole::RpcRecent] {
            let policy = EvmRetentionPolicy::for_role(role);
            // Correctness, not preference: identical across roles.
            assert_eq!(policy.retention(EvmPruneSegment::StateDiffs), SegmentRetention::Blocks(100_000));
            assert_eq!(policy.retention(EvmPruneSegment::StateAnchors), SegmentRetention::Blocks(100_000));
        }
    }

    #[test]
    fn duration_retention_converts_through_the_observed_block_rate() {
        // An hour at 10 BPS is 36_000 blocks; at 1 BPS it is 3_600. Expressing
        // retention in blocks would have meant two different things.
        let hour = SegmentRetention::Duration { ms: HOUR_MS };
        assert_eq!(hour.keep_from(1_000_000, 10.0), Some(1_000_000 - 36_000));
        assert_eq!(hour.keep_from(1_000_000, 1.0), Some(1_000_000 - 3_600));
        // A nonsense rate falls back to 1 BPS rather than producing a keep_from
        // of 0 (delete nothing) or the head (delete everything).
        assert_eq!(hour.keep_from(1_000_000, 0.0), Some(1_000_000 - 3_600));
        assert_eq!(hour.keep_from(1_000_000, f64::NAN), Some(1_000_000 - 3_600));
        // Near genesis it saturates instead of wrapping.
        assert_eq!(hour.keep_from(10, 10.0), Some(0));
    }

    #[test]
    fn off_reclaims_the_head_too_and_blocks_keeps_a_window() {
        assert_eq!(SegmentRetention::Off.keep_from(500, 10.0), Some(501));
        assert_eq!(SegmentRetention::Blocks(100).keep_from(500, 10.0), Some(400));
        assert_eq!(SegmentRetention::Blocks(1000).keep_from(500, 10.0), Some(0));
    }

    #[test]
    fn the_availability_floor_only_ever_rises() {
        // It is a promise about what the node can still answer. Lowering it would
        // advertise data that has already been deleted.
        let mut cursor = EvmPruneCursor::new();
        cursor.advance(100, 10, 1);
        assert_eq!(cursor.available_from, 100);
        cursor.advance(50, 5, 2);
        assert_eq!(cursor.available_from, 100, "a later pass with a lower bound must not lower the floor");
        assert_eq!(cursor.pruned_through, 100);
        assert_eq!(cursor.rows_deleted_total, 15);
    }

    #[test]
    fn an_idle_pass_says_which_kind_of_idle_it_was() {
        // Found by live-net verification: a node mid-IBD on an EVM-ACTIVE network
        // reported "the EVM lane is inert on this network". Three operationally
        // different causes were collapsed into one message — one is permanent, one
        // resolves by itself, one is a fault — and only the permanent one should be
        // said once and then go quiet.
        assert!(!EvmRetentionIdleReason::Ran.is_idle());
        for r in [EvmRetentionIdleReason::LaneInert, EvmRetentionIdleReason::NoEvmHeadYet, EvmRetentionIdleReason::StoreUnavailable] {
            assert!(r.is_idle(), "{r:?}");
        }
        assert!(EvmRetentionIdleReason::LaneInert.is_permanent());
        // A node without an EVM head yet WILL get one; repeating is not noise.
        assert!(!EvmRetentionIdleReason::NoEvmHeadYet.is_permanent());
        // A store fault must never be silenced after the first sighting.
        assert!(!EvmRetentionIdleReason::StoreUnavailable.is_permanent());

        // The message an operator on an active network must NOT see.
        assert!(!EvmRetentionIdleReason::NoEvmHeadYet.describe().contains("never activates"));
        assert!(EvmRetentionIdleReason::NoEvmHeadYet.describe().contains("header sync"));
    }

    #[test]
    fn role_names_round_trip_including_the_short_aliases() {
        for role in [EvmNodeRole::CompactValidator, EvmNodeRole::RpcRecent, EvmNodeRole::ArchiveIndexer] {
            assert_eq!(EvmNodeRole::from_str_opt(role.as_str()), Some(role));
        }
        assert_eq!(EvmNodeRole::from_str_opt("archive"), Some(EvmNodeRole::ArchiveIndexer));
        assert_eq!(EvmNodeRole::from_str_opt("nonsense"), None);
    }
}

/// Why a pass did no work.
///
/// Live-net verification caught these being collapsed into one message: a node
/// mid-IBD on an EVM-ACTIVE network logged "the EVM lane is inert on this
/// network", which is false and would send an operator looking for a
/// misconfiguration that does not exist. The three causes are operationally
/// different — one is permanent, one resolves on its own, one is a fault.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvmRetentionIdleReason {
    /// Ran normally (possibly finding nothing to reclaim, which is healthy).
    #[default]
    Ran,
    /// The EVM lane never activates on this network. Permanent; say it once.
    LaneInert,
    /// The lane is active but this node has no EVM head yet — normal during
    /// header sync, and it resolves without operator action.
    NoEvmHeadYet,
    /// A store read failed. Not "nothing to do" — a fault worth surfacing, and
    /// the case that must never be reported as one of the benign two.
    StoreUnavailable,
}

impl EvmRetentionIdleReason {
    pub fn is_idle(self) -> bool {
        !matches!(self, Self::Ran)
    }

    /// Whether repeating this every tick is noise. A permanent condition is said
    /// once; a fault is said every time, because it may clear or worsen.
    pub fn is_permanent(self) -> bool {
        matches!(self, Self::LaneInert)
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::Ran => "ran",
            Self::LaneInert => "the EVM lane never activates on this network; retention has nothing to do",
            Self::NoEvmHeadYet => "no EVM head yet (normal during header sync); retention will start once the lane has a head",
            Self::StoreUnavailable => "an EVM store could not be read; retention skipped this pass",
        }
    }
}

/// What one retention pass did. Reported so growth and reclamation are both
/// visible in the log, rather than only the growth.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmRetentionReport {
    pub rows_deleted: u64,
    pub segments_advanced: u32,
    /// Why the pass did nothing, when it did nothing. Distinguishes "nothing to
    /// do" from "ran and found nothing" — the difference between a healthy node
    /// and a stuck pruner — and distinguishes the benign causes from a fault.
    pub idle_reason: EvmRetentionIdleReason,
}
