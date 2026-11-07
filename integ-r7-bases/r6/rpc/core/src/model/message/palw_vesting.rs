//! **ADR-0152 V-1…V-8 (R-core+, testnet-12), phase2-plan §1.6 P2-10: `getPalwVesting` (op 199)** — moved out of `message.rs` (2026-10-13, a behaviour-preserving split: every item is the same and keeps its path through `pub use palw_vesting::*`).

use super::*;

// ---------------------------------------------------------------------------------------------
// ADR-0152 V-1…V-8 (R-core+, testnet-12), phase2-plan §1.6 P2-10: `getPalwVesting` (op 199)
// ---------------------------------------------------------------------------------------------

/// ADR-0152 P2-10: `getPalwVesting` — the vesting table as the next block's step 3d will find it.
/// Name at most one of `bond` (`<txid>:<index>`: the rows it produced or was credited on, its
/// reporter rewards, and whether B-3 holds its collateral), `payout_address` (every leg and reporter
/// reward paid there) or `claim_id` (that claim's row); none of them reads the chain-wide totals
/// and the head of the queue. **Op 199 is new: a node built before it drops the WebSocket on it**,
/// so a client asks it last on a connection, or reconnects.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwVestingRequest {
    pub bond: String,
    pub payout_address: String,
    pub claim_id: String,
    /// At most this many rows; 0 asks for the node's cap (500).
    pub limit: u32,
    /// `<expiry_daa>:<claim_id>` — a previous answer's `next_after`; empty for the first page.
    pub after: String,
}

impl Serializer for GetPalwVestingRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond, writer)?;
        store!(String, &self.payout_address, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(u32, &self.limit, writer)?;
        store!(String, &self.after, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwVestingRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            bond: load!(String, reader)?,
            payout_address: load!(String, reader)?,
            claim_id: load!(String, reader)?,
            limit: load!(u32, reader)?,
            after: load!(String, reader)?,
        })
    }
}

/// One leg of a vesting row or move: `producer`, `seat`, `reserve` or `reporter`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwVestingLeg {
    pub kind: String,
    /// `<txid>:<index>`; empty for the reserve and for an awarded reporter reward.
    pub payee_bond: String,
    /// The 64-byte payout payload, hex (the coinbase pays `P2PKH-ML-DSA-87(payload)`); empty for
    /// the reserve.
    pub payload: String,
    pub sompi: u64,
    /// The `pending_payouts` key it lands on; empty for the reserve.
    pub queue_key: String,
}

impl Serializer for RpcPalwVestingLeg {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.kind, writer)?;
        store!(String, &self.payee_bond, writer)?;
        store!(String, &self.payload, writer)?;
        store!(u64, &self.sompi, writer)?;
        store!(String, &self.queue_key, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwVestingLeg {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            kind: load!(String, reader)?,
            payee_bond: load!(String, reader)?,
            payload: load!(String, reader)?,
            sompi: load!(u64, reader)?,
            queue_key: load!(String, reader)?,
        })
    }
}

/// **One vesting row** (ADR-0152 V-1), with V-4's maturity at the next block, its place in V-7's
/// order and the earliest DAA it can move.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwVestingRow {
    pub claim_id: String,
    pub class_id: String,
    pub producer_bond: String,
    /// The door of the Final-basis licence set: `quorum`, `coverage`, `optimistic`, `shard_part`.
    pub licence_door: String,
    pub basis_k: u32,
    pub escrow_sompi: u64,
    pub buyback_bound_sompi: u64,
    /// Every sompi the row holds (producer + seats + reserve).
    pub total_sompi: u64,
    pub reserve_sompi: u64,
    pub final_daa: u64,
    /// The DAA clock: the row cannot mature before it.
    pub expiry_daa: u64,
    /// The anchors settled at Final — the second clock counts from here.
    pub settled_at_final: u64,
    /// The latch (V-4 held once): set, the row is mature for good.
    pub matured_at: Option<u64>,
    /// `maturing` (not latched) or `latched`.
    pub stage: String,
    pub daa_clock_met: bool,
    pub licences_since_final: u64,
    /// The configured second-clock depth; `None`: no second clock on this network.
    pub licences_needed: Option<u64>,
    /// Where the second clock's per-obligation bound releases the row (an upper bound).
    pub second_clock_bound_daa: Option<u64>,
    pub da_session_open: bool,
    /// V-4 holds at the next block, or the row is latched.
    pub mature_now: bool,
    /// **V-4(a) alone: the row is still unmatured** — unlatched, its DAA clock or its second clock
    /// still running. While it is, B-3 locks the collateral of every bond it pays. Not
    /// `!matureNow`: a halt or a DA session keeps a row from maturing without holding its payees.
    pub lock_live: bool,
    /// `(moves, keys)` ahead of it in V-7's order.
    pub moves_ahead: Option<u64>,
    pub keys_ahead: Option<u64>,
    /// The next block's step 3d moves it; its mint is the block after.
    pub in_next_block: bool,
    /// The earliest DAA it can move — exact (the next block's) when `in_next_block`, else a lower
    /// bound and `eta_estimated`.
    pub eta_daa: u64,
    pub eta_estimated: bool,
    /// The legs the query is about (the payee's own, or all of them).
    pub legs: Vec<RpcPalwVestingLeg>,
    pub legs_sompi: u64,
}

impl Serializer for RpcPalwVestingRow {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.producer_bond, writer)?;
        store!(String, &self.licence_door, writer)?;
        store!(u32, &self.basis_k, writer)?;
        store!(u64, &self.escrow_sompi, writer)?;
        store!(u64, &self.buyback_bound_sompi, writer)?;
        store!(u64, &self.total_sompi, writer)?;
        store!(u64, &self.reserve_sompi, writer)?;
        store!(u64, &self.final_daa, writer)?;
        store!(u64, &self.expiry_daa, writer)?;
        store!(u64, &self.settled_at_final, writer)?;
        store!(Option<u64>, &self.matured_at, writer)?;
        store!(String, &self.stage, writer)?;
        store!(bool, &self.daa_clock_met, writer)?;
        store!(u64, &self.licences_since_final, writer)?;
        store!(Option<u64>, &self.licences_needed, writer)?;
        store!(Option<u64>, &self.second_clock_bound_daa, writer)?;
        store!(bool, &self.da_session_open, writer)?;
        store!(bool, &self.mature_now, writer)?;
        store!(bool, &self.lock_live, writer)?;
        store!(Option<u64>, &self.moves_ahead, writer)?;
        store!(Option<u64>, &self.keys_ahead, writer)?;
        store!(bool, &self.in_next_block, writer)?;
        store!(u64, &self.eta_daa, writer)?;
        store!(bool, &self.eta_estimated, writer)?;
        serialize!(Vec<RpcPalwVestingLeg>, &self.legs, writer)?;
        store!(u64, &self.legs_sompi, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwVestingRow {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            claim_id: load!(String, reader)?,
            class_id: load!(String, reader)?,
            producer_bond: load!(String, reader)?,
            licence_door: load!(String, reader)?,
            basis_k: load!(u32, reader)?,
            escrow_sompi: load!(u64, reader)?,
            buyback_bound_sompi: load!(u64, reader)?,
            total_sompi: load!(u64, reader)?,
            reserve_sompi: load!(u64, reader)?,
            final_daa: load!(u64, reader)?,
            expiry_daa: load!(u64, reader)?,
            settled_at_final: load!(u64, reader)?,
            matured_at: load!(Option<u64>, reader)?,
            stage: load!(String, reader)?,
            daa_clock_met: load!(bool, reader)?,
            licences_since_final: load!(u64, reader)?,
            licences_needed: load!(Option<u64>, reader)?,
            second_clock_bound_daa: load!(Option<u64>, reader)?,
            da_session_open: load!(bool, reader)?,
            mature_now: load!(bool, reader)?,
            lock_live: load!(bool, reader)?,
            moves_ahead: load!(Option<u64>, reader)?,
            keys_ahead: load!(Option<u64>, reader)?,
            in_next_block: load!(bool, reader)?,
            eta_daa: load!(u64, reader)?,
            eta_estimated: load!(bool, reader)?,
            legs: deserialize!(Vec<RpcPalwVestingLeg>, reader)?,
            legs_sompi: load!(u64, reader)?,
        })
    }
}

/// One reporter reward (S-7): `pending` in its reveal window (payee: the best reveal so far),
/// `awarded` once the window closed and step 3d has yet to move it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwReporterReward {
    pub offence_key: String,
    pub stage: String,
    /// The best reveal's bond while pending; empty once awarded.
    pub reporter_bond: String,
    pub payload: String,
    pub sompi: u64,
    pub reveal_until: Option<u64>,
    pub in_next_block: bool,
}

impl Serializer for RpcPalwReporterReward {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.offence_key, writer)?;
        store!(String, &self.stage, writer)?;
        store!(String, &self.reporter_bond, writer)?;
        store!(String, &self.payload, writer)?;
        store!(u64, &self.sompi, writer)?;
        store!(Option<u64>, &self.reveal_until, writer)?;
        store!(bool, &self.in_next_block, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwReporterReward {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            offence_key: load!(String, reader)?,
            stage: load!(String, reader)?,
            reporter_bond: load!(String, reader)?,
            payload: load!(String, reader)?,
            sompi: load!(u64, reader)?,
            reveal_until: load!(Option<u64>, reader)?,
            in_next_block: load!(bool, reader)?,
        })
    }
}

/// One move of the next block's plan: a `row` (by claim id) or a `reporter` reward (by offence
/// key), with its legs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwVestingMove {
    pub source: String,
    pub id: String,
    pub legs: Vec<RpcPalwVestingLeg>,
}

impl Serializer for RpcPalwVestingMove {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.source, writer)?;
        store!(String, &self.id, writer)?;
        serialize!(Vec<RpcPalwVestingLeg>, &self.legs, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwVestingMove {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { source: load!(String, reader)?, id: load!(String, reader)?, legs: deserialize!(Vec<RpcPalwVestingLeg>, reader)? })
    }
}

/// Live rows of one licence door (ADR-0152 §8.4 O-2's "p").
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwVestingDoorCount {
    pub door: String,
    pub rows: u64,
    pub latched_rows: u64,
    /// Decimal `u128`.
    pub sompi: String,
}

impl Serializer for RpcPalwVestingDoorCount {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.door, writer)?;
        store!(u64, &self.rows, writer)?;
        store!(u64, &self.latched_rows, writer)?;
        store!(String, &self.sompi, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwVestingDoorCount {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            door: load!(String, reader)?,
            rows: load!(u64, reader)?,
            latched_rows: load!(u64, reader)?,
            sompi: load!(String, reader)?,
        })
    }
}

/// **ADR-0152 P2-10: the `getPalwVesting` answer.** `u128` sums are decimal strings. Everything is
/// read at the NEXT block's DAA (`next_daa`) on the committed tip (`tip_daa`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwVestingResponse {
    /// False off `ConsensusV2`, or before the node has PALW state.
    pub available: bool,
    /// `Params::palw_rcore_plus` is in force: rewards vest. Below it nothing vests and the tables
    /// are empty.
    pub rcore_plus_active: bool,
    pub tip_daa: u64,
    pub next_daa: u64,
    /// V-4(b): a licence halt — nothing latches while it lasts.
    pub halted: bool,
    /// The configured second-clock depth (`None`: no second clock), and the depth after the
    /// liveness escape.
    pub second_clock_depth: Option<u64>,
    pub second_clock_escaped_depth: Option<u64>,
    /// Anchors settled so far (the second clock's count).
    pub settled_anchors: u64,
    /// This node's measure of the chain's pace: milliseconds per DAA over the last sampled span
    /// (`None` when it has no span to measure) — what an ETA in DAA is converted with.
    pub measured_ms_per_daa: Option<u64>,
    /// V-3's rooted counters: created, moved and burned since genesis.
    pub created_sompi: String,
    pub moved_sompi: String,
    pub burned_sompi: String,
    pub live_rows: u64,
    pub latched_rows: u64,
    pub live_sompi: String,
    pub latched_sompi: String,
    /// Latched rows held behind an unlatched head (stop, never skip).
    pub latched_behind_head: u64,
    pub reporter_pending_rows: u64,
    pub reporter_awarded_rows: u64,
    /// The next block's step 3d: its moves, legs, new queue keys, and why it stops (`not_latched`,
    /// `budget_full`, `empty`) at which row or reward.
    pub next_block_moves: Vec<RpcPalwVestingMove>,
    pub next_block_legs: u32,
    pub next_block_new_keys: u32,
    pub next_block_stopped: String,
    pub next_block_stopped_at: String,
    /// The latched backlog in payout keys — every awarded reporter reward and every latched row
    /// (behind an unlatched head included) at its cost as a block's first move — and the blocks it
    /// takes at V-7's full width of eight keys: an estimate (the market's reserve narrows a block
    /// to six; seat payees shared in one block widen it).
    pub backlog_keys: u64,
    pub backlog_blocks_est: u64,
    pub licence_histogram: Vec<RpcPalwVestingDoorCount>,
    /// The query, echoed.
    pub bond: String,
    pub payout_address: String,
    pub claim_id: String,
    /// A claim query's stage: `maturing` or `latched` while its row lives, `moved` once a vested
    /// Final's row left for the payout queue (minted by the block after the move), empty for a
    /// claim that did not vest, was voided — a convicted Final included, its row burned — or
    /// retired with no row left. Empty for other queries.
    pub claim_stage: String,
    /// The bond is in the registry (true for other queries).
    pub bond_known: bool,
    /// **B-3**: the bond is payee of a row still unmatured by V-4(a), so its collateral is locked.
    pub payee_holds_collateral: bool,
    /// Of every row the query matched — not only this page — those whose lock is live
    /// (`lockLive`), and the latest DAA clock among them. For a bond, the rows B-3 holds it for:
    /// `payeeHoldsCollateral == (lockLiveRows > 0)`. V-7's order puts them LAST, so a page's own
    /// `lockLive` rows can be none of them; count with these.
    pub lock_live_rows: u64,
    pub lock_live_last_expiry_daa: Option<u64>,
    pub rows: Vec<RpcPalwVestingRow>,
    /// Rows the query matched, before the page.
    pub rows_total: u64,
    /// Pass back as `after` for the next page; empty on the last.
    pub next_after: String,
    /// Σ of the query's legs over every matched row: not latched, and latched.
    pub maturing_sompi: String,
    pub query_latched_sompi: String,
    pub reporter_rewards: Vec<RpcPalwReporterReward>,
}

impl Serializer for GetPalwVestingResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.rcore_plus_active, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(u64, &self.next_daa, writer)?;
        store!(bool, &self.halted, writer)?;
        store!(Option<u64>, &self.second_clock_depth, writer)?;
        store!(Option<u64>, &self.second_clock_escaped_depth, writer)?;
        store!(u64, &self.settled_anchors, writer)?;
        store!(Option<u64>, &self.measured_ms_per_daa, writer)?;
        store!(String, &self.created_sompi, writer)?;
        store!(String, &self.moved_sompi, writer)?;
        store!(String, &self.burned_sompi, writer)?;
        store!(u64, &self.live_rows, writer)?;
        store!(u64, &self.latched_rows, writer)?;
        store!(String, &self.live_sompi, writer)?;
        store!(String, &self.latched_sompi, writer)?;
        store!(u64, &self.latched_behind_head, writer)?;
        store!(u64, &self.reporter_pending_rows, writer)?;
        store!(u64, &self.reporter_awarded_rows, writer)?;
        serialize!(Vec<RpcPalwVestingMove>, &self.next_block_moves, writer)?;
        store!(u32, &self.next_block_legs, writer)?;
        store!(u32, &self.next_block_new_keys, writer)?;
        store!(String, &self.next_block_stopped, writer)?;
        store!(String, &self.next_block_stopped_at, writer)?;
        store!(u64, &self.backlog_keys, writer)?;
        store!(u64, &self.backlog_blocks_est, writer)?;
        serialize!(Vec<RpcPalwVestingDoorCount>, &self.licence_histogram, writer)?;
        store!(String, &self.bond, writer)?;
        store!(String, &self.payout_address, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.claim_stage, writer)?;
        store!(bool, &self.bond_known, writer)?;
        store!(bool, &self.payee_holds_collateral, writer)?;
        store!(u64, &self.lock_live_rows, writer)?;
        store!(Option<u64>, &self.lock_live_last_expiry_daa, writer)?;
        serialize!(Vec<RpcPalwVestingRow>, &self.rows, writer)?;
        store!(u64, &self.rows_total, writer)?;
        store!(String, &self.next_after, writer)?;
        store!(String, &self.maturing_sompi, writer)?;
        store!(String, &self.query_latched_sompi, writer)?;
        serialize!(Vec<RpcPalwReporterReward>, &self.reporter_rewards, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwVestingResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            rcore_plus_active: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            next_daa: load!(u64, reader)?,
            halted: load!(bool, reader)?,
            second_clock_depth: load!(Option<u64>, reader)?,
            second_clock_escaped_depth: load!(Option<u64>, reader)?,
            settled_anchors: load!(u64, reader)?,
            measured_ms_per_daa: load!(Option<u64>, reader)?,
            created_sompi: load!(String, reader)?,
            moved_sompi: load!(String, reader)?,
            burned_sompi: load!(String, reader)?,
            live_rows: load!(u64, reader)?,
            latched_rows: load!(u64, reader)?,
            live_sompi: load!(String, reader)?,
            latched_sompi: load!(String, reader)?,
            latched_behind_head: load!(u64, reader)?,
            reporter_pending_rows: load!(u64, reader)?,
            reporter_awarded_rows: load!(u64, reader)?,
            next_block_moves: deserialize!(Vec<RpcPalwVestingMove>, reader)?,
            next_block_legs: load!(u32, reader)?,
            next_block_new_keys: load!(u32, reader)?,
            next_block_stopped: load!(String, reader)?,
            next_block_stopped_at: load!(String, reader)?,
            backlog_keys: load!(u64, reader)?,
            backlog_blocks_est: load!(u64, reader)?,
            licence_histogram: deserialize!(Vec<RpcPalwVestingDoorCount>, reader)?,
            bond: load!(String, reader)?,
            payout_address: load!(String, reader)?,
            claim_id: load!(String, reader)?,
            claim_stage: load!(String, reader)?,
            bond_known: load!(bool, reader)?,
            payee_holds_collateral: load!(bool, reader)?,
            lock_live_rows: load!(u64, reader)?,
            lock_live_last_expiry_daa: load!(Option<u64>, reader)?,
            rows: deserialize!(Vec<RpcPalwVestingRow>, reader)?,
            rows_total: load!(u64, reader)?,
            next_after: load!(String, reader)?,
            maturing_sompi: load!(String, reader)?,
            query_latched_sompi: load!(String, reader)?,
            reporter_rewards: deserialize!(Vec<RpcPalwReporterReward>, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwNodeStatusRequest {}

impl Serializer for GetPalwNodeStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwNodeStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwNodeStatusResponse {
    /// The fingerprint this node booted with and announces (`Consensus params fingerprint:`).
    pub consensus_params_id: String,
    /// The heights of its scheduled fences (`Consensus fence schedule:`).
    pub fence_schedule: Vec<u64>,
    pub consensus_schedule_id: String,
    /// The producer: `off` (not started), `disabled` (a startup refusal), `syncing`, `holding`,
    /// `drawing`, `stopped`.
    pub producer_state: String,
    /// The hold's or the refusal's sentence, as the producer logs it; empty while drawing.
    pub producer_reason: String,
    /// Unix seconds at which `producer_state` last changed.
    pub producer_since_unix: u64,
    pub producer_bond: String,
    pub producer_class: String,
    pub draws: u64,
    pub produced_blocks: u64,
    pub receipt_blocks: u64,
    /// Draws that won the class ticket and lost the network draw against bits.
    pub network_lost: u64,
    pub last_block: String,
    pub last_block_unix: u64,
    pub last_draw_unix: u64,
    pub panel_running: bool,
    /// The panel can carry receipts and answers (a fee outpoint resolves).
    pub panel_submitter: bool,
    pub retention_dir: String,
    /// **The memory reservation ledger** (ADR-0151 follow-up, items 3 and 6; version 2 of this
    /// message): the declared per-node share (0 = none), the host's live headroom (0 = unknown),
    /// what this process's duties hold, what one more may take, whether any bound is known, and
    /// who holds what — the figures a hold names, readable without the log.
    pub memory_share_bytes: u64,
    pub memory_headroom_bytes: u64,
    pub memory_reserved_bytes: u64,
    pub memory_available_bytes: u64,
    pub memory_bounded: bool,
    pub memory_holders: String,
    /// Version 3: the selected chain's last blocks by lane (`palw_lane_watch`) — see the node
    /// runtime's fields of the same names.
    pub lane_window_blocks: u64,
    pub lane_work_blocks: u64,
    pub lane_heartbeat_blocks: u64,
    pub lane_last_work_daa: u64,
    pub lane_mix: String,
    pub lane_alarm: String,
    /// **Version 4: the genesis this node runs, and its drill salt's id** (ADR-0152 §8.2, P2-12
    /// review finding 1). Every PALW signature is made under `palw_network_domain_v2_for(network,
    /// genesis)`, and a testnet-12 drill answers to the network name `testnet-12` on another
    /// genesis — so an off-node signer (the `misaka` CLI, the gateway rail) reads these and refuses
    /// to sign when its own params' genesis is not this one
    /// (`config::drill::palw_node_genesis_verdict_v1`). `drill_salt_id` is empty on every real
    /// network; it is the salt's short id, never the salt. Both empty from a version-3 sender.
    pub genesis_hash: String,
    pub drill_salt_id: String,
    /// **Version 5: the verification debt** (the 2026-10-01 panel backlog) — `key=value` pairs, the
    /// memory ledger's readings with and without the reclaimable pages, this seat's slots, schedule
    /// and receipts filed in the last hour, and the producer's backpressure gate. Empty from a
    /// version-4 sender.
    pub verification: String,
    /// Version 6: durable pruning-import authority and its incomplete marker,
    /// observed together. Older senders have `import_session_available=false`.
    #[serde(default)]
    pub import_session_available: bool,
    #[serde(default)]
    pub import_session_epoch: u64,
    #[serde(default)]
    pub import_session_point: String,
    #[serde(default)]
    pub import_session_phase: String,
    #[serde(default)]
    pub pruning_import_stable: bool,
}

impl Serializer for GetPalwNodeStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &6, writer)?;
        store!(String, &self.consensus_params_id, writer)?;
        store!(Vec<u64>, &self.fence_schedule, writer)?;
        store!(String, &self.consensus_schedule_id, writer)?;
        store!(String, &self.producer_state, writer)?;
        store!(String, &self.producer_reason, writer)?;
        store!(u64, &self.producer_since_unix, writer)?;
        store!(String, &self.producer_bond, writer)?;
        store!(String, &self.producer_class, writer)?;
        store!(u64, &self.draws, writer)?;
        store!(u64, &self.produced_blocks, writer)?;
        store!(u64, &self.receipt_blocks, writer)?;
        store!(u64, &self.network_lost, writer)?;
        store!(String, &self.last_block, writer)?;
        store!(u64, &self.last_block_unix, writer)?;
        store!(u64, &self.last_draw_unix, writer)?;
        store!(bool, &self.panel_running, writer)?;
        store!(bool, &self.panel_submitter, writer)?;
        store!(String, &self.retention_dir, writer)?;
        // Version 2: the memory ledger.
        store!(u64, &self.memory_share_bytes, writer)?;
        store!(u64, &self.memory_headroom_bytes, writer)?;
        store!(u64, &self.memory_reserved_bytes, writer)?;
        store!(u64, &self.memory_available_bytes, writer)?;
        store!(bool, &self.memory_bounded, writer)?;
        store!(String, &self.memory_holders, writer)?;
        // Version 3: the lane mix.
        store!(u64, &self.lane_window_blocks, writer)?;
        store!(u64, &self.lane_work_blocks, writer)?;
        store!(u64, &self.lane_heartbeat_blocks, writer)?;
        store!(u64, &self.lane_last_work_daa, writer)?;
        store!(String, &self.lane_mix, writer)?;
        store!(String, &self.lane_alarm, writer)?;
        // Version 4: the genesis and the drill salt's id.
        store!(String, &self.genesis_hash, writer)?;
        store!(String, &self.drill_salt_id, writer)?;
        // Version 5: the verification debt.
        store!(String, &self.verification, writer)?;
        // Version 6: session diagnostics. The version-5 prefix stays verbatim.
        store!(bool, &self.import_session_available, writer)?;
        store!(u64, &self.import_session_epoch, writer)?;
        store!(String, &self.import_session_point, writer)?;
        store!(String, &self.import_session_phase, writer)?;
        store!(bool, &self.pruning_import_stable, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwNodeStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let mut out = Self {
            consensus_params_id: load!(String, reader)?,
            fence_schedule: load!(Vec<u64>, reader)?,
            consensus_schedule_id: load!(String, reader)?,
            producer_state: load!(String, reader)?,
            producer_reason: load!(String, reader)?,
            producer_since_unix: load!(u64, reader)?,
            producer_bond: load!(String, reader)?,
            producer_class: load!(String, reader)?,
            draws: load!(u64, reader)?,
            produced_blocks: load!(u64, reader)?,
            receipt_blocks: load!(u64, reader)?,
            network_lost: load!(u64, reader)?,
            last_block: load!(String, reader)?,
            last_block_unix: load!(u64, reader)?,
            last_draw_unix: load!(u64, reader)?,
            panel_running: load!(bool, reader)?,
            panel_submitter: load!(bool, reader)?,
            retention_dir: load!(String, reader)?,
            ..Default::default()
        };
        // A version-1 sender stops here; the ledger fields stay at their defaults.
        if version >= 2 {
            out.memory_share_bytes = load!(u64, reader)?;
            out.memory_headroom_bytes = load!(u64, reader)?;
            out.memory_reserved_bytes = load!(u64, reader)?;
            out.memory_available_bytes = load!(u64, reader)?;
            out.memory_bounded = load!(bool, reader)?;
            out.memory_holders = load!(String, reader)?;
        }
        // A version-2 sender stops here; the lane fields stay at their defaults.
        if version >= 3 {
            out.lane_window_blocks = load!(u64, reader)?;
            out.lane_work_blocks = load!(u64, reader)?;
            out.lane_heartbeat_blocks = load!(u64, reader)?;
            out.lane_last_work_daa = load!(u64, reader)?;
            out.lane_mix = load!(String, reader)?;
            out.lane_alarm = load!(String, reader)?;
        }
        // A version-3 sender stops here: it predates the drill salt, so it is no drill.
        if version >= 4 {
            out.genesis_hash = load!(String, reader)?;
            out.drill_salt_id = load!(String, reader)?;
        }
        // A version-4 sender stops here: no verification debt to report.
        if version >= 5 {
            out.verification = load!(String, reader)?;
        }
        if version >= 6 {
            out.import_session_available = load!(bool, reader)?;
            out.import_session_epoch = load!(u64, reader)?;
            out.import_session_point = load!(String, reader)?;
            out.import_session_phase = load!(String, reader)?;
            out.pruning_import_stable = load!(bool, reader)?;
        }
        Ok(out)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenSupplyRequest {
    pub asset_id: u64,
}

impl Serializer for GetTokenSupplyRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.asset_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenSupplyRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let asset_id = load!(u64, reader)?;
        Ok(Self { asset_id })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenSupplyResponse {
    pub available: bool,
    /// Decimal strings (`u128` atomic units).
    pub minted: String,
    pub burned: String,
    pub circulating: String,
}

impl Serializer for GetTokenSupplyResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.minted, writer)?;
        store!(String, &self.burned, writer)?;
        store!(String, &self.circulating, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenSupplyResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let minted = load!(String, reader)?;
        let burned = load!(String, reader)?;
        let circulating = load!(String, reader)?;
        Ok(Self { available, minted, burned, circulating })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenEmissionInfoRequest {
    /// The epoch to read. Ignored when `latest` is true.
    #[serde(default)]
    pub epoch: u64,
    /// When true (the sugar default), read the most recently settled epoch.
    #[serde(default)]
    pub latest: bool,
}

impl Serializer for GetTokenEmissionInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.epoch, writer)?;
        store!(bool, &self.latest, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenEmissionInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let epoch = load!(u64, reader)?;
        let latest = load!(bool, reader)?;
        Ok(Self { epoch, latest })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenEmissionInfoResponse {
    pub available: bool,
    pub epoch: u64,
    /// False ⇒ the epoch has no settlement row yet (numeric fields are zero).
    pub settled: bool,
    /// Decimal strings (`u128`): R(E), X(E), Σ reward_i.
    pub budget: String,
    pub network_compute: String,
    pub paid_total: String,
    /// Audit-emission v0.2: the counted-verdict share of `paid_total` (decimal `u128`).
    #[serde(default)]
    pub audit_paid: String,
    pub reward_count: u32,
    /// Hex keyed-BLAKE2b-256 digest of the whole settlement (cross-node comparable).
    pub settlement_root: String,
    /// The live cursors — the ops gauges (design §9.2): next epoch settlement
    /// will consider, and the next selected-chain index the ledger fold processes.
    pub next_settlement_epoch: u64,
    pub fold_cursor: u64,
}

impl Serializer for GetTokenEmissionInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.epoch, writer)?;
        store!(bool, &self.settled, writer)?;
        store!(String, &self.budget, writer)?;
        store!(String, &self.network_compute, writer)?;
        store!(String, &self.paid_total, writer)?;
        store!(String, &self.audit_paid, writer)?;
        store!(u32, &self.reward_count, writer)?;
        store!(String, &self.settlement_root, writer)?;
        store!(u64, &self.next_settlement_epoch, writer)?;
        store!(u64, &self.fold_cursor, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenEmissionInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let epoch = load!(u64, reader)?;
        let settled = load!(bool, reader)?;
        let budget = load!(String, reader)?;
        let network_compute = load!(String, reader)?;
        let paid_total = load!(String, reader)?;
        let audit_paid = if version >= 2 { load!(String, reader)? } else { String::new() };
        let reward_count = load!(u32, reader)?;
        let settlement_root = load!(String, reader)?;
        let next_settlement_epoch = load!(u64, reader)?;
        let fold_cursor = load!(u64, reader)?;
        Ok(Self {
            available,
            epoch,
            settled,
            budget,
            network_compute,
            paid_total,
            audit_paid,
            reward_count,
            settlement_root,
            next_settlement_epoch,
            fold_cursor,
        })
    }
}

// kaspa-pq Phase 12 (ADR-0011): getValidatorAttestationTarget. Given a stake-bond
// outpoint ("txid_hex:index"), returns the exact ready-to-sign attestation message
// (and its bound fields) the validator must ML-DSA-87-sign for the current sink — so
// the `kaspa-pq-validator` sidecar can fetch the signing target over local wRPC.
// `available` is false when the overlay is not configured or no target can be assembled.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidatorAttestationTargetRequest {
    /// Stake-bond outpoint, "txid_hex:index" (txid is a 64-byte Hash64 = 128 hex chars).
    pub bond_outpoint: String,
}

impl Serializer for GetValidatorAttestationTargetRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond_outpoint, writer)?;
        Ok(())
    }
}

impl Deserializer for GetValidatorAttestationTargetRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let bond_outpoint = load!(String, reader)?;
        Ok(Self { bond_outpoint })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidatorAttestationTargetResponse {
    /// False when the overlay is not configured or no target could be assembled; the
    /// remaining fields are then defaults. (A malformed outpoint is a request error.)
    pub available: bool,
    pub epoch: u64,
    /// Selected-chain anchor the attestation approves (Hash64, hex).
    pub target_hash: String,
    pub target_daa_score: u64,
    /// Commitment over the active validator set (Hash64, hex).
    pub validator_set_commitment: String,
    /// The ready-to-sign 32-byte attestation message digest (hex). The sidecar signs
    /// this with its ML-DSA-87 validator key under the attestation context.
    pub message: String,
}

impl Serializer for GetValidatorAttestationTargetResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.epoch, writer)?;
        store!(String, &self.target_hash, writer)?;
        store!(u64, &self.target_daa_score, writer)?;
        store!(String, &self.validator_set_commitment, writer)?;
        store!(String, &self.message, writer)?;
        Ok(())
    }
}

impl Deserializer for GetValidatorAttestationTargetResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let epoch = load!(u64, reader)?;
        let target_hash = load!(String, reader)?;
        let target_daa_score = load!(u64, reader)?;
        let validator_set_commitment = load!(String, reader)?;
        let message = load!(String, reader)?;
        Ok(Self { available, epoch, target_hash, target_daa_score, validator_set_commitment, message })
    }
}

// kaspa-pq DNS v3 (batch): getValidatorAttestationTargets. Returns every READY, creditable
// attestation target for a bond in `[from_epoch, latest_ready]` (ascending, capped at `limit`),
// so an external `kaspa-pq-validator` that fell behind can sign every missed epoch in one poll
// instead of one epoch per poll (which lets a briefly-slow validator lag the epoch cadence).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidatorAttestationTargetsRequest {
    /// Stake-bond outpoint, "txid_hex:index" (txid is a 64-byte Hash64 = 128 hex chars).
    pub bond_outpoint: String,
    /// Lowest epoch to return (inclusive). Pass `last_attested_epoch + 1` to fetch the backlog.
    pub from_epoch: u64,
    /// Max targets to return; `0` yields none. The server also caps this.
    pub limit: u32,
}

impl Serializer for GetValidatorAttestationTargetsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond_outpoint, writer)?;
        store!(u64, &self.from_epoch, writer)?;
        store!(u32, &self.limit, writer)?;
        Ok(())
    }
}

impl Deserializer for GetValidatorAttestationTargetsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let bond_outpoint = load!(String, reader)?;
        let from_epoch = load!(u64, reader)?;
        let limit = load!(u32, reader)?;
        Ok(Self { bond_outpoint, from_epoch, limit })
    }
}

/// kaspa-pq DNS v3: one ready-to-sign attestation target in a [`GetValidatorAttestationTargetsResponse`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcValidatorAttestationTarget {
    pub epoch: u64,
    /// Selected-chain anchor the attestation approves (Hash64, hex).
    pub target_hash: String,
    pub target_daa_score: u64,
    /// Commitment over the active validator set (Hash64, hex).
    pub validator_set_commitment: String,
    /// The ready-to-sign 32-byte attestation message digest (hex).
    pub message: String,
}

impl Serializer for RpcValidatorAttestationTarget {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.epoch, writer)?;
        store!(String, &self.target_hash, writer)?;
        store!(u64, &self.target_daa_score, writer)?;
        store!(String, &self.validator_set_commitment, writer)?;
        store!(String, &self.message, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcValidatorAttestationTarget {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let epoch = load!(u64, reader)?;
        let target_hash = load!(String, reader)?;
        let target_daa_score = load!(u64, reader)?;
        let validator_set_commitment = load!(String, reader)?;
        let message = load!(String, reader)?;
        Ok(Self { epoch, target_hash, target_daa_score, validator_set_commitment, message })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidatorAttestationTargetsResponse {
    /// Ready targets in ascending epoch order (empty when the overlay is off or none are ready).
    pub targets: Vec<RpcValidatorAttestationTarget>,
}

impl Serializer for GetValidatorAttestationTargetsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcValidatorAttestationTarget>, &self.targets, writer)?;
        Ok(())
    }
}

impl Deserializer for GetValidatorAttestationTargetsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let targets = deserialize!(Vec<RpcValidatorAttestationTarget>, reader)?;
        Ok(Self { targets })
    }
}

// kaspa-pq Phase 12 (ADR-0011): getStakeBond. The sidecar's own stake-bond status,
// evaluated at the node's sink so it matches what the validator would attest for.
// `available` is false when the overlay is not configured or no such bond exists.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStakeBondRequest {
    /// Stake-bond outpoint, "txid_hex:index" (txid is a 64-byte Hash64 = 128 hex chars).
    pub bond_outpoint: String,
}

impl Serializer for GetStakeBondRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond_outpoint, writer)?;
        Ok(())
    }
}

impl Deserializer for GetStakeBondRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let bond_outpoint = load!(String, reader)?;
        Ok(Self { bond_outpoint })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStakeBondResponse {
    /// False when the overlay is not configured or no such bond exists; the remaining
    /// fields are then defaults. (A malformed outpoint is a request error.)
    pub available: bool,
    /// The bond's validator id (validator_pubkey_hash, Hash64 hex). The sidecar checks
    /// this matches its own key.
    pub validator_id: String,
    pub amount: u64,
    pub activation_daa_score: u64,
    /// Effective status at the node's sink: "pending" / "active" / "unbonding" / "slashed".
    pub effective_status: String,
}

impl Serializer for GetStakeBondResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.validator_id, writer)?;
        store!(u64, &self.amount, writer)?;
        store!(u64, &self.activation_daa_score, writer)?;
        store!(String, &self.effective_status, writer)?;
        Ok(())
    }
}

impl Deserializer for GetStakeBondResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let validator_id = load!(String, reader)?;
        let amount = load!(u64, reader)?;
        let activation_daa_score = load!(u64, reader)?;
        let effective_status = load!(String, reader)?;
        Ok(Self { available, validator_id, amount, activation_daa_score, effective_status })
    }
}

// kaspa-pq: getStakeBonds — a paged, filtered enumeration of the StakeBonds
// overlay store. The store is outpoint-keyed with no owner index, so the node
// does a full scan + in-memory filter; the request is always bounded by `limit`
// and walked with an outpoint `cursor`, so an owner can recover the outpoint(s)
// of bonds they funded (the only key `StakeUnbondRequest` binds to) without ever
// listing the whole set unbounded.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStakeBondsRequest {
    /// Restrict to bonds owned by this `owner_pubkey_hash` (Hash64 hex); `None` = any owner.
    pub owner_pubkey_hash: Option<String>,
    /// Restrict to bonds whose effective status is in this set. Each entry is one
    /// of "pending" / "active" / "unbonding" / "slashed" (case-insensitive).
    /// `None`/empty = any status.
    pub status_in: Option<Vec<String>>,
    /// Return only bonds ordered strictly after this outpoint ("txid_hex:index",
    /// exclusive). Pass a previous response's `next_cursor` to page.
    pub cursor: Option<String>,
    /// Max entries to return; `0` selects the server default and values above the
    /// server cap are clamped.
    pub limit: u32,
    /// Point-of-view DAA score for the effective-status filter/report. `None`
    /// uses the live sink. Pin it to page 1's `pov_daa_score` when walking a
    /// `status_in`-filtered result across pages so the effective-status set is a
    /// consistent snapshot (otherwise a bond whose status changes mid-walk can be
    /// skipped). Status is a read-only view, so this never affects consensus.
    pub pov_daa_score: Option<u64>,
}

impl Serializer for GetStakeBondsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Option<String>, &self.owner_pubkey_hash, writer)?;
        store!(Option<Vec<String>>, &self.status_in, writer)?;
        store!(Option<String>, &self.cursor, writer)?;
        store!(u32, &self.limit, writer)?;
        store!(Option<u64>, &self.pov_daa_score, writer)?;
        Ok(())
    }
}

impl Deserializer for GetStakeBondsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let owner_pubkey_hash = load!(Option<String>, reader)?;
        let status_in = load!(Option<Vec<String>>, reader)?;
        let cursor = load!(Option<String>, reader)?;
        let limit = load!(u32, reader)?;
        let pov_daa_score = load!(Option<u64>, reader)?;
        Ok(Self { owner_pubkey_hash, status_in, cursor, limit, pov_daa_score })
    }
}

/// kaspa-pq: one stake-bond entry in a [`GetStakeBondsResponse`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcStakeBondEntry {
    /// Bond outpoint, "txid_hex:index" — the key a `StakeUnbondRequest` binds to.
    pub bond_outpoint: String,
    /// The bond owner (owner_pubkey_hash, Hash64 hex).
    pub owner_pubkey_hash: String,
    /// The bond's validator id (validator_pubkey_hash, Hash64 hex).
    pub validator_id: String,
    pub amount: u64,
    pub activation_daa_score: u64,
    /// Unbonding period (blocks). With `unbond_request_daa_score` a client can
    /// compute the release height = `unbond_request_daa_score + unbonding_period_blocks`.
    pub unbonding_period_blocks: u64,
    /// DAA score at which an unbond request was accepted, or `None`.
    pub unbond_request_daa_score: Option<u64>,
    /// Stored bond status ("pending"/"active"/"unbonding"/"slashed").
    pub stored_status: String,
    /// Effective status at `pov_daa_score` (the sink), matching GetStakeBond.
    pub effective_status: String,
}

impl Serializer for RpcStakeBondEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond_outpoint, writer)?;
        store!(String, &self.owner_pubkey_hash, writer)?;
        store!(String, &self.validator_id, writer)?;
        store!(u64, &self.amount, writer)?;
        store!(u64, &self.activation_daa_score, writer)?;
        store!(u64, &self.unbonding_period_blocks, writer)?;
        store!(Option<u64>, &self.unbond_request_daa_score, writer)?;
        store!(String, &self.stored_status, writer)?;
        store!(String, &self.effective_status, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcStakeBondEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let bond_outpoint = load!(String, reader)?;
        let owner_pubkey_hash = load!(String, reader)?;
        let validator_id = load!(String, reader)?;
        let amount = load!(u64, reader)?;
        let activation_daa_score = load!(u64, reader)?;
        let unbonding_period_blocks = load!(u64, reader)?;
        let unbond_request_daa_score = load!(Option<u64>, reader)?;
        let stored_status = load!(String, reader)?;
        let effective_status = load!(String, reader)?;
        Ok(Self {
            bond_outpoint,
            owner_pubkey_hash,
            validator_id,
            amount,
            activation_daa_score,
            unbonding_period_blocks,
            unbond_request_daa_score,
            stored_status,
            effective_status,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStakeBondsResponse {
    pub bonds: Vec<RpcStakeBondEntry>,
    /// Pass as the next request's `cursor` to page; `None` on the last page.
    pub next_cursor: Option<String>,
    /// Sink DAA score the effective statuses were evaluated at.
    pub pov_daa_score: u64,
}

impl Serializer for GetStakeBondsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcStakeBondEntry>, &self.bonds, writer)?;
        store!(Option<String>, &self.next_cursor, writer)?;
        store!(u64, &self.pov_daa_score, writer)?;
        Ok(())
    }
}

impl Deserializer for GetStakeBondsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let bonds = deserialize!(Vec<RpcStakeBondEntry>, reader)?;
        let next_cursor = load!(Option<String>, reader)?;
        let pov_daa_score = load!(u64, reader)?;
        Ok(Self { bonds, next_cursor, pov_daa_score })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxosByAddressesRequest {
    pub addresses: Vec<RpcAddress>,
}

impl GetUtxosByAddressesRequest {
    pub fn new(addresses: Vec<RpcAddress>) -> Self {
        Self { addresses }
    }
}

impl Serializer for GetUtxosByAddressesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcAddress>, &self.addresses, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxosByAddressesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let addresses = load!(Vec<RpcAddress>, reader)?;

        Ok(Self { addresses })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxosByAddressesResponse {
    pub entries: Vec<RpcUtxosByAddressesEntry>,
}

impl GetUtxosByAddressesResponse {
    pub fn new(entries: Vec<RpcUtxosByAddressesEntry>) -> Self {
        Self { entries }
    }
}

impl Serializer for GetUtxosByAddressesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcUtxosByAddressesEntry>, &self.entries, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxosByAddressesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let entries = deserialize!(Vec<RpcUtxosByAddressesEntry>, reader)?;

        Ok(Self { entries })
    }
}

/// Cursor-paginated single-address UTXO query. Use this instead of `GetUtxosByAddresses` for
/// addresses with very large UTXO sets (e.g. an unconsolidated mining payout), whose full response
/// can exceed client message-size / timeout limits.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxosByAddressPageRequest {
    pub address: RpcAddress,
    /// Opaque resume token returned as `next_cursor` by the previous page; empty = start at the beginning.
    pub cursor: String,
    /// Maximum entries to return; 0 selects the server default. The server caps the effective value.
    pub limit: u64,
}

impl GetUtxosByAddressPageRequest {
    pub fn new(address: RpcAddress, cursor: String, limit: u64) -> Self {
        Self { address, cursor, limit }
    }
}

impl Serializer for GetUtxosByAddressPageRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcAddress, &self.address, writer)?;
        store!(String, &self.cursor, writer)?;
        store!(u64, &self.limit, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxosByAddressPageRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let address = load!(RpcAddress, reader)?;
        let cursor = load!(String, reader)?;
        let limit = load!(u64, reader)?;

        Ok(Self { address, cursor, limit })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxosByAddressPageResponse {
    pub entries: Vec<RpcUtxosByAddressesEntry>,
    /// Opaque resume token for the next page; empty = no further pages.
    pub next_cursor: String,
}

impl GetUtxosByAddressPageResponse {
    pub fn new(entries: Vec<RpcUtxosByAddressesEntry>, next_cursor: String) -> Self {
        Self { entries, next_cursor }
    }
}

impl Serializer for GetUtxosByAddressPageResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcUtxosByAddressesEntry>, &self.entries, writer)?;
        store!(String, &self.next_cursor, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxosByAddressPageResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let entries = deserialize!(Vec<RpcUtxosByAddressesEntry>, reader)?;
        let next_cursor = load!(String, reader)?;

        Ok(Self { entries, next_cursor })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BanRequest {
    pub ip: RpcIpAddress,
}

impl BanRequest {
    pub fn new(ip: RpcIpAddress) -> Self {
        Self { ip }
    }
}

impl Serializer for BanRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcIpAddress, &self.ip, writer)?;

        Ok(())
    }
}

impl Deserializer for BanRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let ip = load!(RpcIpAddress, reader)?;

        Ok(Self { ip })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BanResponse {}

impl Serializer for BanResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for BanResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnbanRequest {
    pub ip: RpcIpAddress,
}

impl UnbanRequest {
    pub fn new(ip: RpcIpAddress) -> Self {
        Self { ip }
    }
}

impl Serializer for UnbanRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcIpAddress, &self.ip, writer)?;

        Ok(())
    }
}

impl Deserializer for UnbanRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let ip = load!(RpcIpAddress, reader)?;

        Ok(Self { ip })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnbanResponse {}

impl Serializer for UnbanResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for UnbanResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateNetworkHashesPerSecondRequest {
    pub window_size: u32,
    pub start_hash: Option<RpcHash>,
}

impl EstimateNetworkHashesPerSecondRequest {
    pub fn new(window_size: u32, start_hash: Option<RpcHash>) -> Self {
        Self { window_size, start_hash }
    }
}

impl Serializer for EstimateNetworkHashesPerSecondRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.window_size, writer)?;
        store!(Option<RpcHash>, &self.start_hash, writer)?;

        Ok(())
    }
}

impl Deserializer for EstimateNetworkHashesPerSecondRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let window_size = load!(u32, reader)?;
        let start_hash = load!(Option<RpcHash>, reader)?;

        Ok(Self { window_size, start_hash })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateNetworkHashesPerSecondResponse {
    pub network_hashes_per_second: u64,
}

impl EstimateNetworkHashesPerSecondResponse {
    pub fn new(network_hashes_per_second: u64) -> Self {
        Self { network_hashes_per_second }
    }
}

impl Serializer for EstimateNetworkHashesPerSecondResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.network_hashes_per_second, writer)?;

        Ok(())
    }
}

impl Deserializer for EstimateNetworkHashesPerSecondResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let network_hashes_per_second = load!(u64, reader)?;

        Ok(Self { network_hashes_per_second })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesByAddressesRequest {
    pub addresses: Vec<RpcAddress>,
    pub include_orphan_pool: bool,
    // TODO: replace with `include_transaction_pool`
    pub filter_transaction_pool: bool,
}

impl GetMempoolEntriesByAddressesRequest {
    pub fn new(addresses: Vec<RpcAddress>, include_orphan_pool: bool, filter_transaction_pool: bool) -> Self {
        Self { addresses, include_orphan_pool, filter_transaction_pool }
    }
}

impl Serializer for GetMempoolEntriesByAddressesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcAddress>, &self.addresses, writer)?;
        store!(bool, &self.include_orphan_pool, writer)?;
        store!(bool, &self.filter_transaction_pool, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMempoolEntriesByAddressesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let addresses = load!(Vec<RpcAddress>, reader)?;
        let include_orphan_pool = load!(bool, reader)?;
        let filter_transaction_pool = load!(bool, reader)?;

        Ok(Self { addresses, include_orphan_pool, filter_transaction_pool })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesByAddressesResponse {
    pub entries: Vec<RpcMempoolEntryByAddress>,
}

impl GetMempoolEntriesByAddressesResponse {
    pub fn new(entries: Vec<RpcMempoolEntryByAddress>) -> Self {
        Self { entries }
    }
}

impl Serializer for GetMempoolEntriesByAddressesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcMempoolEntryByAddress>, &self.entries, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMempoolEntriesByAddressesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let entries = deserialize!(Vec<RpcMempoolEntryByAddress>, reader)?;

        Ok(Self { entries })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCoinSupplyRequest {}

impl Serializer for GetCoinSupplyRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetCoinSupplyRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCoinSupplyResponse {
    pub max_sompi: u64,
    pub circulating_sompi: u64,
}

impl GetCoinSupplyResponse {
    pub fn new(max_sompi: u64, circulating_sompi: u64) -> Self {
        Self { max_sompi, circulating_sompi }
    }
}

impl Serializer for GetCoinSupplyResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.max_sompi, writer)?;
        store!(u64, &self.circulating_sompi, writer)?;

        Ok(())
    }
}

impl Deserializer for GetCoinSupplyResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let max_sompi = load!(u64, reader)?;
        let circulating_sompi = load!(u64, reader)?;

        Ok(Self { max_sompi, circulating_sompi })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingRequest {}

impl Serializer for PingRequest {
    fn serialize<W: std::io::Write>(&self, _writer: &mut W) -> std::io::Result<()> {
        Ok(())
    }
}

impl Deserializer for PingRequest {
    fn deserialize<R: std::io::Read>(_reader: &mut R) -> std::io::Result<Self> {
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {}

impl Serializer for PingResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for PingResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsProfileData {
    pub cpu_usage: f32,
    pub memory_usage: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConnectionsRequest {
    pub include_profile_data: bool,
}

impl Serializer for GetConnectionsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(bool, &self.include_profile_data, writer)?;
        Ok(())
    }
}

impl Deserializer for GetConnectionsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let include_profile_data = load!(bool, reader)?;
        Ok(Self { include_profile_data })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConnectionsResponse {
    pub clients: u32,
    pub peers: u16,
    pub profile_data: Option<ConnectionsProfileData>,
}

impl Serializer for GetConnectionsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.clients, writer)?;
        store!(u16, &self.peers, writer)?;
        store!(Option<ConnectionsProfileData>, &self.profile_data, writer)?;
        Ok(())
    }
}

impl Deserializer for GetConnectionsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let clients = load!(u32, reader)?;
        let peers = load!(u16, reader)?;
        let extra = load!(Option<ConnectionsProfileData>, reader)?;
        Ok(Self { clients, peers, profile_data: extra })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSystemInfoRequest {}

impl Serializer for GetSystemInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        Ok(())
    }
}

impl Deserializer for GetSystemInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        Ok(Self {})
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSystemInfoResponse {
    pub version: String,
    pub system_id: Option<Vec<u8>>,
    pub git_hash: Option<Vec<u8>>,
    pub cpu_physical_cores: u16,
    pub total_memory: u64,
    pub fd_limit: u32,
    pub proxy_socket_limit_per_cpu_core: Option<u32>,
}

impl std::fmt::Debug for GetSystemInfoResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GetSystemInfoResponse")
            .field("version", &self.version)
            .field("system_id", &self.system_id.as_ref().map(|id| id.to_hex()))
            .field("git_hash", &self.git_hash.as_ref().map(|hash| hash.to_hex()))
            .field("cpu_physical_cores", &self.cpu_physical_cores)
            .field("total_memory", &self.total_memory)
            .field("fd_limit", &self.fd_limit)
            .field("proxy_socket_limit_per_cpu_core", &self.proxy_socket_limit_per_cpu_core)
            .finish()
    }
}

impl Serializer for GetSystemInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(String, &self.version, writer)?;
        store!(Option<Vec<u8>>, &self.system_id, writer)?;
        store!(Option<Vec<u8>>, &self.git_hash, writer)?;
        store!(u16, &self.cpu_physical_cores, writer)?;
        store!(u64, &self.total_memory, writer)?;
        store!(u32, &self.fd_limit, writer)?;
        store!(Option<u32>, &self.proxy_socket_limit_per_cpu_core, writer)?;

        Ok(())
    }
}

impl Deserializer for GetSystemInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let payload_version = load!(u16, reader)?;
        let version = load!(String, reader)?;
        let system_id = load!(Option<Vec<u8>>, reader)?;
        let git_hash = load!(Option<Vec<u8>>, reader)?;
        let cpu_physical_cores = load!(u16, reader)?;
        let total_memory = load!(u64, reader)?;
        let fd_limit = load!(u32, reader)?;

        let proxy_socket_limit_per_cpu_core = if payload_version > 1 { load!(Option<u32>, reader)? } else { None };

        Ok(Self { version, system_id, git_hash, cpu_physical_cores, total_memory, fd_limit, proxy_socket_limit_per_cpu_core })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMetricsRequest {
    pub process_metrics: bool,
    pub connection_metrics: bool,
    pub bandwidth_metrics: bool,
    pub consensus_metrics: bool,
    pub storage_metrics: bool,
    pub custom_metrics: bool,
}

impl Serializer for GetMetricsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.process_metrics, writer)?;
        store!(bool, &self.connection_metrics, writer)?;
        store!(bool, &self.bandwidth_metrics, writer)?;
        store!(bool, &self.consensus_metrics, writer)?;
        store!(bool, &self.storage_metrics, writer)?;
        store!(bool, &self.custom_metrics, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMetricsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let process_metrics = load!(bool, reader)?;
        let connection_metrics = load!(bool, reader)?;
        let bandwidth_metrics = load!(bool, reader)?;
        let consensus_metrics = load!(bool, reader)?;
        let storage_metrics = load!(bool, reader)?;
        let custom_metrics = load!(bool, reader)?;

        Ok(Self { process_metrics, connection_metrics, bandwidth_metrics, consensus_metrics, storage_metrics, custom_metrics })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetrics {
    pub resident_set_size: u64,
    pub virtual_memory_size: u64,
    pub core_num: u32,
    pub cpu_usage: f32,
    pub fd_num: u32,
    pub disk_io_read_bytes: u64,
    pub disk_io_write_bytes: u64,
    pub disk_io_read_per_sec: f32,
    pub disk_io_write_per_sec: f32,
}

impl Serializer for ProcessMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.resident_set_size, writer)?;
        store!(u64, &self.virtual_memory_size, writer)?;
        store!(u32, &self.core_num, writer)?;
        store!(f32, &self.cpu_usage, writer)?;
        store!(u32, &self.fd_num, writer)?;
        store!(u64, &self.disk_io_read_bytes, writer)?;
        store!(u64, &self.disk_io_write_bytes, writer)?;
        store!(f32, &self.disk_io_read_per_sec, writer)?;
        store!(f32, &self.disk_io_write_per_sec, writer)?;

        Ok(())
    }
}

impl Deserializer for ProcessMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let resident_set_size = load!(u64, reader)?;
        let virtual_memory_size = load!(u64, reader)?;
        let core_num = load!(u32, reader)?;
        let cpu_usage = load!(f32, reader)?;
        let fd_num = load!(u32, reader)?;
        let disk_io_read_bytes = load!(u64, reader)?;
        let disk_io_write_bytes = load!(u64, reader)?;
        let disk_io_read_per_sec = load!(f32, reader)?;
        let disk_io_write_per_sec = load!(f32, reader)?;

        Ok(Self {
            resident_set_size,
            virtual_memory_size,
            core_num,
            cpu_usage,
            fd_num,
            disk_io_read_bytes,
            disk_io_write_bytes,
            disk_io_read_per_sec,
            disk_io_write_per_sec,
        })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionMetrics {
    pub borsh_live_connections: u32,
    pub borsh_connection_attempts: u64,
    pub borsh_handshake_failures: u64,
    pub json_live_connections: u32,
    pub json_connection_attempts: u64,
    pub json_handshake_failures: u64,

    pub active_peers: u32,
}

impl Serializer for ConnectionMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.borsh_live_connections, writer)?;
        store!(u64, &self.borsh_connection_attempts, writer)?;
        store!(u64, &self.borsh_handshake_failures, writer)?;
        store!(u32, &self.json_live_connections, writer)?;
        store!(u64, &self.json_connection_attempts, writer)?;
        store!(u64, &self.json_handshake_failures, writer)?;
        store!(u32, &self.active_peers, writer)?;

        Ok(())
    }
}

impl Deserializer for ConnectionMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let borsh_live_connections = load!(u32, reader)?;
        let borsh_connection_attempts = load!(u64, reader)?;
        let borsh_handshake_failures = load!(u64, reader)?;
        let json_live_connections = load!(u32, reader)?;
        let json_connection_attempts = load!(u64, reader)?;
        let json_handshake_failures = load!(u64, reader)?;
        let active_peers = load!(u32, reader)?;

        Ok(Self {
            borsh_live_connections,
            borsh_connection_attempts,
            borsh_handshake_failures,
            json_live_connections,
            json_connection_attempts,
            json_handshake_failures,
            active_peers,
        })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BandwidthMetrics {
    pub borsh_bytes_tx: u64,
    pub borsh_bytes_rx: u64,
    pub json_bytes_tx: u64,
    pub json_bytes_rx: u64,
    pub p2p_bytes_tx: u64,
    pub p2p_bytes_rx: u64,
    pub grpc_bytes_tx: u64,
    pub grpc_bytes_rx: u64,
}

impl Serializer for BandwidthMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.borsh_bytes_tx, writer)?;
        store!(u64, &self.borsh_bytes_rx, writer)?;
        store!(u64, &self.json_bytes_tx, writer)?;
        store!(u64, &self.json_bytes_rx, writer)?;
        store!(u64, &self.p2p_bytes_tx, writer)?;
        store!(u64, &self.p2p_bytes_rx, writer)?;
        store!(u64, &self.grpc_bytes_tx, writer)?;
        store!(u64, &self.grpc_bytes_rx, writer)?;

        Ok(())
    }
}

impl Deserializer for BandwidthMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let borsh_bytes_tx = load!(u64, reader)?;
        let borsh_bytes_rx = load!(u64, reader)?;
        let json_bytes_tx = load!(u64, reader)?;
        let json_bytes_rx = load!(u64, reader)?;
        let p2p_bytes_tx = load!(u64, reader)?;
        let p2p_bytes_rx = load!(u64, reader)?;
        let grpc_bytes_tx = load!(u64, reader)?;
        let grpc_bytes_rx = load!(u64, reader)?;

        Ok(Self {
            borsh_bytes_tx,
            borsh_bytes_rx,
            json_bytes_tx,
            json_bytes_rx,
            p2p_bytes_tx,
            p2p_bytes_rx,
            grpc_bytes_tx,
            grpc_bytes_rx,
        })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsensusMetrics {
    pub node_blocks_submitted_count: u64,
    pub node_headers_processed_count: u64,
    pub node_dependencies_processed_count: u64,
    pub node_bodies_processed_count: u64,
    pub node_transactions_processed_count: u64,
    pub node_chain_blocks_processed_count: u64,
    pub node_mass_processed_count: u64,

    pub node_database_blocks_count: u64,
    pub node_database_headers_count: u64,

    pub network_mempool_size: u64,
    pub network_tip_hashes_count: u32,
    pub network_difficulty: f64,
    pub network_past_median_time: u64,
    pub network_virtual_parent_hashes_count: u32,
    pub network_virtual_daa_score: u64,
}

impl Serializer for ConsensusMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.node_blocks_submitted_count, writer)?;
        store!(u64, &self.node_headers_processed_count, writer)?;
        store!(u64, &self.node_dependencies_processed_count, writer)?;
        store!(u64, &self.node_bodies_processed_count, writer)?;
        store!(u64, &self.node_transactions_processed_count, writer)?;
        store!(u64, &self.node_chain_blocks_processed_count, writer)?;
        store!(u64, &self.node_mass_processed_count, writer)?;
        store!(u64, &self.node_database_blocks_count, writer)?;
        store!(u64, &self.node_database_headers_count, writer)?;
        store!(u64, &self.network_mempool_size, writer)?;
        store!(u32, &self.network_tip_hashes_count, writer)?;
        store!(f64, &self.network_difficulty, writer)?;
        store!(u64, &self.network_past_median_time, writer)?;
        store!(u32, &self.network_virtual_parent_hashes_count, writer)?;
        store!(u64, &self.network_virtual_daa_score, writer)?;

        Ok(())
    }
}

impl Deserializer for ConsensusMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let node_blocks_submitted_count = load!(u64, reader)?;
        let node_headers_processed_count = load!(u64, reader)?;
        let node_dependencies_processed_count = load!(u64, reader)?;
        let node_bodies_processed_count = load!(u64, reader)?;
        let node_transactions_processed_count = load!(u64, reader)?;
        let node_chain_blocks_processed_count = load!(u64, reader)?;
        let node_mass_processed_count = load!(u64, reader)?;
        let node_database_blocks_count = load!(u64, reader)?;
        let node_database_headers_count = load!(u64, reader)?;
        let network_mempool_size = load!(u64, reader)?;
        let network_tip_hashes_count = load!(u32, reader)?;
        let network_difficulty = load!(f64, reader)?;
        let network_past_median_time = load!(u64, reader)?;
        let network_virtual_parent_hashes_count = load!(u32, reader)?;
        let network_virtual_daa_score = load!(u64, reader)?;

        Ok(Self {
            node_blocks_submitted_count,
            node_headers_processed_count,
            node_dependencies_processed_count,
            node_bodies_processed_count,
            node_transactions_processed_count,
            node_chain_blocks_processed_count,
            node_mass_processed_count,
            node_database_blocks_count,
            node_database_headers_count,
            network_mempool_size,
            network_tip_hashes_count,
            network_difficulty,
            network_past_median_time,
            network_virtual_parent_hashes_count,
            network_virtual_daa_score,
        })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageMetrics {
    pub storage_size_bytes: u64,
}

impl Serializer for StorageMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.storage_size_bytes, writer)?;

        Ok(())
    }
}

impl Deserializer for StorageMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let storage_size_bytes = load!(u64, reader)?;

        Ok(Self { storage_size_bytes })
    }
}

// TODO: Custom metrics dictionary
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CustomMetricValue {
    Placeholder,
}

impl Serializer for CustomMetricValue {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        Ok(())
    }
}

impl Deserializer for CustomMetricValue {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        Ok(CustomMetricValue::Placeholder)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMetricsResponse {
    pub server_time: u64,
    pub process_metrics: Option<ProcessMetrics>,
    pub connection_metrics: Option<ConnectionMetrics>,
    pub bandwidth_metrics: Option<BandwidthMetrics>,
    pub consensus_metrics: Option<ConsensusMetrics>,
    pub storage_metrics: Option<StorageMetrics>,
    // TODO: this is currently a placeholder
    pub custom_metrics: Option<HashMap<String, CustomMetricValue>>,
}

impl GetMetricsResponse {
    pub fn new(
        server_time: u64,
        process_metrics: Option<ProcessMetrics>,
        connection_metrics: Option<ConnectionMetrics>,
        bandwidth_metrics: Option<BandwidthMetrics>,
        consensus_metrics: Option<ConsensusMetrics>,
        storage_metrics: Option<StorageMetrics>,
        custom_metrics: Option<HashMap<String, CustomMetricValue>>,
    ) -> Self {
        Self {
            process_metrics,
            connection_metrics,
            bandwidth_metrics,
            consensus_metrics,
            storage_metrics,
            server_time,
            custom_metrics,
        }
    }
}

impl Serializer for GetMetricsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.server_time, writer)?;
        serialize!(Option<ProcessMetrics>, &self.process_metrics, writer)?;
        serialize!(Option<ConnectionMetrics>, &self.connection_metrics, writer)?;
        serialize!(Option<BandwidthMetrics>, &self.bandwidth_metrics, writer)?;
        serialize!(Option<ConsensusMetrics>, &self.consensus_metrics, writer)?;
        serialize!(Option<StorageMetrics>, &self.storage_metrics, writer)?;
        serialize!(Option<HashMap<String, CustomMetricValue>>, &self.custom_metrics, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMetricsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let server_time = load!(u64, reader)?;
        let process_metrics = deserialize!(Option<ProcessMetrics>, reader)?;
        let connection_metrics = deserialize!(Option<ConnectionMetrics>, reader)?;
        let bandwidth_metrics = deserialize!(Option<BandwidthMetrics>, reader)?;
        let consensus_metrics = deserialize!(Option<ConsensusMetrics>, reader)?;
        let storage_metrics = deserialize!(Option<StorageMetrics>, reader)?;
        let custom_metrics = deserialize!(Option<HashMap<String, CustomMetricValue>>, reader)?;

        Ok(Self {
            server_time,
            process_metrics,
            connection_metrics,
            bandwidth_metrics,
            consensus_metrics,
            storage_metrics,
            custom_metrics,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
#[borsh(use_discriminant = true)]
pub enum RpcCaps {
    Full = 0,
    Blocks,
    UtxoIndex,
    Mempool,
    Metrics,
    Visualizer,
    Mining,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetServerInfoRequest {}

impl Serializer for GetServerInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetServerInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetServerInfoResponse {
    pub rpc_api_version: u16,
    pub rpc_api_revision: u16,
    pub server_version: String,
    pub network_id: RpcNetworkId,
    pub has_utxo_index: bool,
    pub is_synced: bool,
    pub virtual_daa_score: u64,
}

impl Serializer for GetServerInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        store!(u16, &self.rpc_api_version, writer)?;
        store!(u16, &self.rpc_api_revision, writer)?;

        store!(String, &self.server_version, writer)?;
        store!(RpcNetworkId, &self.network_id, writer)?;
        store!(bool, &self.has_utxo_index, writer)?;
        store!(bool, &self.is_synced, writer)?;
        store!(u64, &self.virtual_daa_score, writer)?;

        Ok(())
    }
}

impl Deserializer for GetServerInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        let rpc_api_version = load!(u16, reader)?;
        let rpc_api_revision = load!(u16, reader)?;

        let server_version = load!(String, reader)?;
        let network_id = load!(RpcNetworkId, reader)?;
        let has_utxo_index = load!(bool, reader)?;
        let is_synced = load!(bool, reader)?;
        let virtual_daa_score = load!(u64, reader)?;

        Ok(Self { rpc_api_version, rpc_api_revision, server_version, network_id, has_utxo_index, is_synced, virtual_daa_score })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSyncStatusRequest {}

impl Serializer for GetSyncStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSyncStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSyncStatusResponse {
    pub is_synced: bool,
}

impl Serializer for GetSyncStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.is_synced, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSyncStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let is_synced = load!(bool, reader)?;
        Ok(Self { is_synced })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDaaScoreTimestampEstimateRequest {
    pub daa_scores: Vec<u64>,
}

impl GetDaaScoreTimestampEstimateRequest {
    pub fn new(daa_scores: Vec<u64>) -> Self {
        Self { daa_scores }
    }
}

impl Serializer for GetDaaScoreTimestampEstimateRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<u64>, &self.daa_scores, writer)?;
        Ok(())
    }
}

impl Deserializer for GetDaaScoreTimestampEstimateRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let daa_scores = load!(Vec<u64>, reader)?;
        Ok(Self { daa_scores })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDaaScoreTimestampEstimateResponse {
    pub timestamps: Vec<u64>,
}

impl GetDaaScoreTimestampEstimateResponse {
    pub fn new(timestamps: Vec<u64>) -> Self {
        Self { timestamps }
    }
}

impl Serializer for GetDaaScoreTimestampEstimateResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<u64>, &self.timestamps, writer)?;
        Ok(())
    }
}

impl Deserializer for GetDaaScoreTimestampEstimateResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let timestamps = load!(Vec<u64>, reader)?;
        Ok(Self { timestamps })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// Fee rate estimations

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeEstimateRequest {}

impl Serializer for GetFeeEstimateRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetFeeEstimateRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeEstimateResponse {
    pub estimate: RpcFeeEstimate,
}

impl Serializer for GetFeeEstimateResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcFeeEstimate, &self.estimate, writer)?;
        Ok(())
    }
}

impl Deserializer for GetFeeEstimateResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let estimate = deserialize!(RpcFeeEstimate, reader)?;
        Ok(Self { estimate })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeEstimateExperimentalRequest {
    pub verbose: bool,
}

impl Serializer for GetFeeEstimateExperimentalRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.verbose, writer)?;
        Ok(())
    }
}

impl Deserializer for GetFeeEstimateExperimentalRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let verbose = load!(bool, reader)?;
        Ok(Self { verbose })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeEstimateExperimentalResponse {
    /// The usual feerate estimate response
    pub estimate: RpcFeeEstimate,

    /// Experimental verbose data
    pub verbose: Option<RpcFeeEstimateVerboseExperimentalData>,
}

impl Serializer for GetFeeEstimateExperimentalResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcFeeEstimate, &self.estimate, writer)?;
        serialize!(Option<RpcFeeEstimateVerboseExperimentalData>, &self.verbose, writer)?;
        Ok(())
    }
}

impl Deserializer for GetFeeEstimateExperimentalResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let estimate = deserialize!(RpcFeeEstimate, reader)?;
        let verbose = deserialize!(Option<RpcFeeEstimateVerboseExperimentalData>, reader)?;
        Ok(Self { estimate, verbose })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentBlockColorRequest {
    pub hash: RpcHash,
}

impl Serializer for GetCurrentBlockColorRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.hash, writer)?;

        Ok(())
    }
}

impl Deserializer for GetCurrentBlockColorRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let hash = load!(RpcHash, reader)?;

        Ok(Self { hash })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentBlockColorResponse {
    pub blue: bool,
}

impl Serializer for GetCurrentBlockColorResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.blue, writer)?;

        Ok(())
    }
}

impl Deserializer for GetCurrentBlockColorResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let blue = load!(bool, reader)?;

        Ok(Self { blue })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxoReturnAddressRequest {
    // PR-9.5f: txid widened to RpcTransactionId (= Hash64) so it
    // feeds the consensus `get_transactions_by_accepting_daa_score`
    // API, which takes `Option<Vec<TransactionId>>`.
    pub txid: RpcTransactionId,
    pub accepting_block_daa_score: u64,
}

impl GetUtxoReturnAddressRequest {
    pub fn new(txid: RpcTransactionId, accepting_block_daa_score: u64) -> Self {
        Self { txid, accepting_block_daa_score }
    }
}

impl Serializer for GetUtxoReturnAddressRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        // PR-9.5f: Hash64 wire width.
        store!(kaspa_consensus_core::Hash64, &self.txid, writer)?;
        store!(u64, &self.accepting_block_daa_score, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxoReturnAddressRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let txid = load!(kaspa_consensus_core::Hash64, reader)?;
        let accepting_block_daa_score = load!(u64, reader)?;

        Ok(Self { txid, accepting_block_daa_score })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxoReturnAddressResponse {
    pub return_address: RpcAddress,
}

impl GetUtxoReturnAddressResponse {
    pub fn new(return_address: RpcAddress) -> Self {
        Self { return_address }
    }
}

impl Serializer for GetUtxoReturnAddressResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcAddress, &self.return_address, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxoReturnAddressResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let return_address = load!(RpcAddress, reader)?;

        Ok(Self { return_address })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualChainFromBlockV2Request {
    pub start_hash: RpcHash,
    pub data_verbosity_level: Option<RpcDataVerbosityLevel>,
    pub min_confirmation_count: Option<u64>,
}

impl GetVirtualChainFromBlockV2Request {
    pub fn new(start_hash: RpcHash, data_verbosity_level: Option<RpcDataVerbosityLevel>, min_confirmation_count: Option<u64>) -> Self {
        Self { start_hash, data_verbosity_level, min_confirmation_count }
    }
}

impl Serializer for GetVirtualChainFromBlockV2Request {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.start_hash, writer)?;
        serialize!(Option<RpcDataVerbosityLevel>, &self.data_verbosity_level, writer)?;
        store!(Option<u64>, &self.min_confirmation_count, writer)?;

        Ok(())
    }
}

impl Deserializer for GetVirtualChainFromBlockV2Request {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let start_hash = load!(RpcHash, reader)?;
        let data_verbosity_level = deserialize!(Option<RpcDataVerbosityLevel>, reader)?;
        let min_confirmation_count = load!(Option<u64>, reader)?;

        Ok(Self { start_hash, data_verbosity_level, min_confirmation_count })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualChainFromBlockV2Response {
    /// always present, no matter the verbosity level
    pub removed_chain_block_hashes: Arc<Vec<RpcHash>>,
    /// always present, no matter the verbosity level
    pub added_chain_block_hashes: Arc<Vec<RpcHash>>,
    /// struct properties are optionally returned depending on the verbosity level
    pub chain_block_accepted_transactions: Arc<Vec<RpcChainBlockAcceptedTransactions>>,
}

impl Serializer for GetVirtualChainFromBlockV2Response {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcHash>, &self.removed_chain_block_hashes, writer)?;
        store!(Vec<RpcHash>, &self.added_chain_block_hashes, writer)?;
        serialize!(Vec<RpcChainBlockAcceptedTransactions>, &self.chain_block_accepted_transactions, writer)?;
        Ok(())
    }
}

impl Deserializer for GetVirtualChainFromBlockV2Response {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let removed_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let added_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let chain_block_accepted_transactions = deserialize!(Vec<RpcChainBlockAcceptedTransactions>, reader)?;
        Ok(Self {
            removed_chain_block_hashes: removed_chain_block_hashes.into(),
            added_chain_block_hashes: added_chain_block_hashes.into(),
            chain_block_accepted_transactions: chain_block_accepted_transactions.into(),
        })
    }
}
