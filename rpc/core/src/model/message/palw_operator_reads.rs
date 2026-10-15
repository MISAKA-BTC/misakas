//! **ADR-0122 §6.5: the operator's reads — a bond's claims, the class table, this node's runtime** — moved out of `message.rs` (2026-10-13, a behaviour-preserving split: every item is the same and keeps its path through `pub use palw_operator_reads::*`).

use super::*;

// ---------------------------------------------------------------------------------------------
// ADR-0122 §6.5: the operator's reads — a bond's claims, the class table, this node's runtime
// ---------------------------------------------------------------------------------------------

/// ADR-0122: `getPalwClaims` — a bond's claims, as executor or as a seat on their panels.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwClaimsRequest {
    /// The bond, `<txid>:<index>`.
    pub bond: String,
    /// `executor` (the claims this bond made) or `seat` (the claims whose panels seat it).
    pub role: String,
    /// Also the final and voided claims still in the state (they retire `claim_retirement` after
    /// they end).
    pub include_terminal: bool,
    /// At most this many rows, newest first; 0 asks for the node's cap.
    pub limit: u32,
}

impl Serializer for GetPalwClaimsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond, writer)?;
        store!(String, &self.role, writer)?;
        store!(bool, &self.include_terminal, writer)?;
        store!(u32, &self.limit, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwClaimsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let bond = load!(String, reader)?;
        let role = load!(String, reader)?;
        let include_terminal = load!(bool, reader)?;
        let limit = load!(u32, reader)?;
        Ok(Self { bond, role, include_terminal, limit })
    }
}

/// One claim as its operator reads it (`palw_producer_v2::PalwClaimRowV1`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwClaimRow {
    pub claim_id: String,
    pub is_free_prompt: bool,
    pub class_id: String,
    pub executor_bond: String,
    /// `provisional`, `panel_bound`, `receipt_licensed`, `final`, `default_disputed`, `voided` —
    /// the spelling `getPalwFreePromptClaim` uses.
    pub phase: String,
    pub void_reason: String,
    pub phase_daa: u64,
    pub accepted_daa: u64,
    pub accepted_block: String,
    pub rebound_daa: Option<u64>,
    pub bound_daa: Option<u64>,
    /// The panel's seats, `<txid>:<index>`, in seat order.
    pub seats: Vec<String>,
    /// When the current phase ends by itself (a window, a court's backstop, a retirement).
    pub deadline_daa: Option<u64>,
    /// **The claim's WEIGHT term** on its bond, in sompi, as a decimal string (a u128) — not
    /// everything the claim holds there: see `committed_sompi` (version 4). Held until the claim
    /// ends (Final or void).
    pub reserved_sompi: String,
    pub escrow_sompi: u64,
    pub payout_pending_sompi: Option<u64>,
    pub quanta: u32,
    pub quanta_spent: u32,
    pub work_leaves: u64,
    pub open_courts: u32,
    /// **The execution lane's view of this claim's Final** (route-matrix #7): `credited`,
    /// `maturing`, `scheduled`, or empty where it earned no credit or the state no longer keeps it.
    pub exec_stage: String,
    pub exec_credit: u64,
    /// The span it was gathered in, waits for, or was scheduled by.
    pub exec_span: Option<u64>,
    pub exec_tickets: u32,
    pub exec_tickets_spent: u32,
    pub exec_first_round: Option<u64>,
    pub exec_last_round: Option<u64>,
    /// **Version 3 — ADR-0152 R-core+ (phase2-plan §1.6): where this claim's reward stands past
    /// Final.** On testnet-12 a Final names its reward in a vesting row instead of paying it: the
    /// row matures on two clocks (V-4), latches, waits its turn (V-7), moves into the queue at step
    /// 3d and is minted by the next coinbase. `vesting_stage`: `maturing` (not latched), `latched`,
    /// `moved` (left the row; `payout_pending_sompi` says whether the next coinbase still has to
    /// mint it), or empty — the claim did not vest (not Final, below the fence, no escrow, the
    /// prompt lane) or was voided. A convicted Final shows as its void: the claim record cannot tell
    /// a burned row from a void before Final, and `getPalwVesting`'s `burnedSompi` counts burns.
    pub vesting_stage: String,
    /// Every sompi the row names (producer + credited seats + reserve); 0 once moved.
    pub vesting_sompi: u64,
    /// Σ of the row's legs that pay the queried bond (its producer leg, or its seat's).
    pub vesting_payee_sompi: u64,
    /// The DAA clock: the row cannot mature before it.
    pub vesting_expiry_daa: Option<u64>,
    /// Anchors settled since Final, and how many the second clock needs (`None`: no second clock).
    pub vesting_licences_since_final: u64,
    pub vesting_licences_needed: Option<u64>,
    /// The latch.
    pub vesting_matured_at: Option<u64>,
    /// The earliest DAA the row can move — the next block's when its plan takes it, else a lower
    /// bound (`vesting_eta_estimated`).
    pub vesting_eta_daa: Option<u64>,
    pub vesting_eta_estimated: bool,
    /// **Version 4 (testnet-12 lifecycle audit): everything the claim holds on its bond at the
    /// tip**, decimal sompi — its weight (`reserved_sompi`), plus the block lane's escrow term until
    /// the LICENCE releases it (past `palw_rcore_plus`; the weight stays to Final), plus a
    /// free-prompt claim's receipt rights. What the bond's reserved exposure sums. Empty from a
    /// version-3 peer, and where the node's figure overflows (never a sentinel amount).
    #[serde(default)]
    pub committed_sompi: String,
    /// **Version 5 — ADR-0176 amendment (ユーザー判断 2026-10-15 item 3)**: the reservation's decomposition as one JSON object
    /// (`type`, `mode`, `weight`, `right`, `penalty`, `other`, `collectableLoss`, `gain`, each component's hold and the fixed
    /// `capacityRecoveryDaa`). Empty for a claim accepted below `palw_bond_formula_v2` and from a version-4 peer.
    #[serde(default)]
    pub reservation_split: String,
    /// **Version 6 — RFC-0018 Gate C**: the claim's certification state (`pending`, `certified`, `not-certified`, …). Empty for a claim
    /// accepted below the fence (every shipped network) and from a version-5 peer. A certified claim's consensus credit is
    /// premise-based (see `GetPalwSettlementResponse::security_certification_premise`).
    #[serde(default)]
    pub security_certification: String,
}

impl Serializer for RpcPalwClaimRow {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // RFC-0018: version 6 only where the row carries a Gate C state (never on a shipped network), so older readers are unaffected.
        let version: u16 = if self.security_certification.is_empty() { 5 } else { 6 };
        store!(u16, &version, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(bool, &self.is_free_prompt, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.executor_bond, writer)?;
        store!(String, &self.phase, writer)?;
        store!(String, &self.void_reason, writer)?;
        store!(u64, &self.phase_daa, writer)?;
        store!(u64, &self.accepted_daa, writer)?;
        store!(String, &self.accepted_block, writer)?;
        store!(Option<u64>, &self.rebound_daa, writer)?;
        store!(Option<u64>, &self.bound_daa, writer)?;
        store!(Vec<String>, &self.seats, writer)?;
        store!(Option<u64>, &self.deadline_daa, writer)?;
        store!(String, &self.reserved_sompi, writer)?;
        store!(u64, &self.escrow_sompi, writer)?;
        store!(Option<u64>, &self.payout_pending_sompi, writer)?;
        store!(u32, &self.quanta, writer)?;
        store!(u32, &self.quanta_spent, writer)?;
        store!(u64, &self.work_leaves, writer)?;
        store!(u32, &self.open_courts, writer)?;
        // Version 2: the execution lane's view.
        store!(String, &self.exec_stage, writer)?;
        store!(u64, &self.exec_credit, writer)?;
        store!(Option<u64>, &self.exec_span, writer)?;
        store!(u32, &self.exec_tickets, writer)?;
        store!(u32, &self.exec_tickets_spent, writer)?;
        store!(Option<u64>, &self.exec_first_round, writer)?;
        store!(Option<u64>, &self.exec_last_round, writer)?;
        // Version 3: the vesting row (ADR-0152). Appended, so a version-2 reader stops before it —
        // each row is its own length-prefixed payload inside the response's list.
        store!(String, &self.vesting_stage, writer)?;
        store!(u64, &self.vesting_sompi, writer)?;
        store!(u64, &self.vesting_payee_sompi, writer)?;
        store!(Option<u64>, &self.vesting_expiry_daa, writer)?;
        store!(u64, &self.vesting_licences_since_final, writer)?;
        store!(Option<u64>, &self.vesting_licences_needed, writer)?;
        store!(Option<u64>, &self.vesting_matured_at, writer)?;
        store!(Option<u64>, &self.vesting_eta_daa, writer)?;
        store!(bool, &self.vesting_eta_estimated, writer)?;
        // Version 4: what the claim holds on its bond, whole (weight + escrow + rights).
        store!(String, &self.committed_sompi, writer)?;
        // Version 5: the reservation's decomposition (ADR-0176 amendment).
        store!(String, &self.reservation_split, writer)?;
        if version >= 6 {
            store!(String, &self.security_certification, writer)?;
        }
        Ok(())
    }
}

impl Deserializer for RpcPalwClaimRow {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let mut row = Self {
            claim_id: load!(String, reader)?,
            is_free_prompt: load!(bool, reader)?,
            class_id: load!(String, reader)?,
            executor_bond: load!(String, reader)?,
            phase: load!(String, reader)?,
            void_reason: load!(String, reader)?,
            phase_daa: load!(u64, reader)?,
            accepted_daa: load!(u64, reader)?,
            accepted_block: load!(String, reader)?,
            rebound_daa: load!(Option<u64>, reader)?,
            bound_daa: load!(Option<u64>, reader)?,
            seats: load!(Vec<String>, reader)?,
            deadline_daa: load!(Option<u64>, reader)?,
            reserved_sompi: load!(String, reader)?,
            escrow_sompi: load!(u64, reader)?,
            payout_pending_sompi: load!(Option<u64>, reader)?,
            quanta: load!(u32, reader)?,
            quanta_spent: load!(u32, reader)?,
            work_leaves: load!(u64, reader)?,
            open_courts: load!(u32, reader)?,
            ..Default::default()
        };
        if version >= 2 {
            row.exec_stage = load!(String, reader)?;
            row.exec_credit = load!(u64, reader)?;
            row.exec_span = load!(Option<u64>, reader)?;
            row.exec_tickets = load!(u32, reader)?;
            row.exec_tickets_spent = load!(u32, reader)?;
            row.exec_first_round = load!(Option<u64>, reader)?;
            row.exec_last_round = load!(Option<u64>, reader)?;
        }
        if version >= 3 {
            row.vesting_stage = load!(String, reader)?;
            row.vesting_sompi = load!(u64, reader)?;
            row.vesting_payee_sompi = load!(u64, reader)?;
            row.vesting_expiry_daa = load!(Option<u64>, reader)?;
            row.vesting_licences_since_final = load!(u64, reader)?;
            row.vesting_licences_needed = load!(Option<u64>, reader)?;
            row.vesting_matured_at = load!(Option<u64>, reader)?;
            row.vesting_eta_daa = load!(Option<u64>, reader)?;
            row.vesting_eta_estimated = load!(bool, reader)?;
        }
        if version >= 4 {
            row.committed_sompi = load!(String, reader)?;
        }
        if version >= 5 {
            row.reservation_split = load!(String, reader)?;
        }
        if version >= 6 {
            row.security_certification = load!(String, reader)?;
        }
        Ok(row)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwClaimsResponse {
    /// False off `ConsensusV2`.
    pub available: bool,
    /// The DAA the state was read at.
    pub tip_daa: u64,
    pub bond: String,
    pub role: String,
    pub claims: Vec<RpcPalwClaimRow>,
    /// `limit` left rows out.
    pub truncated: bool,
    /// The bond's own registry record. `bond_known` false: no bond at that outpoint.
    pub bond_known: bool,
    /// The key it was registered under, hex (who may sign for it).
    pub bond_pubkey: String,
    /// `None` while the bond is Active.
    pub bond_retiring_since_daa: Option<u64>,
    pub bond_collateral: u64,
    /// Every sompi debited from the bond's collateral: convictions AND, past
    /// `palw_audit_2026_09_23`, the designed 1 MSK burn of each class registration it bought
    /// (`PALW_CLASS_REGISTRATION_BURN_SOMPI_V1`) — a price, not a penalty.
    pub bond_slashed: u64,
    pub bond_registered_daa: u64,
    /// The classes this bond is seated for: a bond judges only the classes it declared, and a
    /// registration declares none.
    pub bond_capable_classes: Vec<String>,
    /// **Version 2 — ADR-0152 R-core+**: claims that RETIRED from the state while their reward
    /// still vests (asked with `include_terminal`). A claim retires `claim_retirement` after Final;
    /// its row lives until it moves, so from retirement on it is the only record of the reward.
    /// Each is shaped as a `final` claim row carrying its vesting fields; newest Final first.
    pub vesting_only_rows: Vec<RpcPalwClaimRow>,
    pub vesting_only_truncated: bool,
    /// **Version 3 (testnet-12 lifecycle audit T12-058): the bond's live `Valid` locks.** A seat
    /// holds one per `Valid` it signed, for `window_court` past the claim's Final, and while ANY is
    /// live the chain drops a `BondRetireRequested` ("still holds … sompi of slashable panel locks")
    /// after its carrier is mined. Judged as the next block's fold judges them, at
    /// `bond_locks_at_daa`. `bond_live_lock_count` / `bond_live_locked_sompi` are whole;
    /// `bond_locks` lists at most `limit` of them, earliest expiry first.
    #[serde(default)]
    pub bond_locks_at_daa: u64,
    #[serde(default)]
    pub bond_live_lock_count: u64,
    /// Decimal sompi (a u128).
    #[serde(default)]
    pub bond_live_locked_sompi: String,
    #[serde(default)]
    pub bond_locks: Vec<RpcPalwBondLock>,
    #[serde(default)]
    pub bond_locks_truncated: bool,
    /// The DAA every live lock's DAA clock has run by (0: none live) — the earliest a retirement
    /// can fold, where no anchor count holds a lock past it.
    #[serde(default)]
    pub bond_lock_daa_clock_release: u64,
    /// The latest the anchor count (the second clock) may hold any of them; `None` where no second
    /// clock runs.
    #[serde(default)]
    pub bond_lock_anchor_count_bound: Option<u64>,
    /// The chain refuses a retirement while a lock is live (`palw_objective_offence` armed).
    #[serde(default)]
    pub bond_retire_refused_while_locked: bool,
    /// How long a Retiring bond's collateral stays locked after `bond_retiring_since_daa`, at the
    /// next block (the bundle's delay plus the DA court's lattice where armed); a vesting row the
    /// bond is payee of can hold it longer (B-3).
    #[serde(default)]
    pub bond_withdrawal_delay_daa: u64,
}

/// One live `Valid` lock on a bond (`getPalwClaims` version 3).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwBondLock {
    pub claim_id: String,
    /// Decimal sompi (a u128).
    pub amount_sompi: String,
    /// The lock's DAA clock: live while the DAA is below it.
    pub expiry_daa: u64,
    /// The DAA clock has run and the anchor count alone holds it.
    pub held_by_anchor_count: bool,
}

impl Serializer for RpcPalwBondLock {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.amount_sompi, writer)?;
        store!(u64, &self.expiry_daa, writer)?;
        store!(bool, &self.held_by_anchor_count, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwBondLock {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            claim_id: load!(String, reader)?,
            amount_sompi: load!(String, reader)?,
            expiry_daa: load!(u64, reader)?,
            held_by_anchor_count: load!(bool, reader)?,
        })
    }
}

impl Serializer for GetPalwClaimsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &3, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(String, &self.bond, writer)?;
        store!(String, &self.role, writer)?;
        serialize!(Vec<RpcPalwClaimRow>, &self.claims, writer)?;
        store!(bool, &self.truncated, writer)?;
        store!(bool, &self.bond_known, writer)?;
        store!(String, &self.bond_pubkey, writer)?;
        store!(Option<u64>, &self.bond_retiring_since_daa, writer)?;
        store!(u64, &self.bond_collateral, writer)?;
        store!(u64, &self.bond_slashed, writer)?;
        store!(u64, &self.bond_registered_daa, writer)?;
        store!(Vec<String>, &self.bond_capable_classes, writer)?;
        // Version 2: the retired claims' vesting rows (ADR-0152).
        serialize!(Vec<RpcPalwClaimRow>, &self.vesting_only_rows, writer)?;
        store!(bool, &self.vesting_only_truncated, writer)?;
        // Version 3: the bond's live locks (T12-058).
        store!(u64, &self.bond_locks_at_daa, writer)?;
        store!(u64, &self.bond_live_lock_count, writer)?;
        store!(String, &self.bond_live_locked_sompi, writer)?;
        serialize!(Vec<RpcPalwBondLock>, &self.bond_locks, writer)?;
        store!(bool, &self.bond_locks_truncated, writer)?;
        store!(u64, &self.bond_lock_daa_clock_release, writer)?;
        store!(Option<u64>, &self.bond_lock_anchor_count_bound, writer)?;
        store!(bool, &self.bond_retire_refused_while_locked, writer)?;
        store!(u64, &self.bond_withdrawal_delay_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwClaimsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        // The version gates the tail (phase2-plan §1.6): a version-1 peer's answer ends at the
        // capable classes, and reading on would fail at the end of its frame.
        let version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let tip_daa = load!(u64, reader)?;
        let bond = load!(String, reader)?;
        let role = load!(String, reader)?;
        let claims = deserialize!(Vec<RpcPalwClaimRow>, reader)?;
        let truncated = load!(bool, reader)?;
        let bond_known = load!(bool, reader)?;
        let bond_pubkey = load!(String, reader)?;
        let bond_retiring_since_daa = load!(Option<u64>, reader)?;
        let bond_collateral = load!(u64, reader)?;
        let bond_slashed = load!(u64, reader)?;
        let bond_registered_daa = load!(u64, reader)?;
        let bond_capable_classes = load!(Vec<String>, reader)?;
        let (vesting_only_rows, vesting_only_truncated) =
            if version >= 2 { (deserialize!(Vec<RpcPalwClaimRow>, reader)?, load!(bool, reader)?) } else { (Vec::new(), false) };
        let mut locks = GetPalwClaimsResponse::default();
        if version >= 3 {
            locks.bond_locks_at_daa = load!(u64, reader)?;
            locks.bond_live_lock_count = load!(u64, reader)?;
            locks.bond_live_locked_sompi = load!(String, reader)?;
            locks.bond_locks = deserialize!(Vec<RpcPalwBondLock>, reader)?;
            locks.bond_locks_truncated = load!(bool, reader)?;
            locks.bond_lock_daa_clock_release = load!(u64, reader)?;
            locks.bond_lock_anchor_count_bound = load!(Option<u64>, reader)?;
            locks.bond_retire_refused_while_locked = load!(bool, reader)?;
            locks.bond_withdrawal_delay_daa = load!(u64, reader)?;
        }
        Ok(Self {
            available,
            tip_daa,
            bond,
            role,
            claims,
            truncated,
            bond_known,
            bond_pubkey,
            bond_retiring_since_daa,
            bond_collateral,
            bond_slashed,
            bond_registered_daa,
            bond_capable_classes,
            vesting_only_rows,
            vesting_only_truncated,
            ..locks
        })
    }
}

/// ADR-0122: `getPalwClasses` — the class table `--palw-dump-classes` prints, over RPC.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwClassesRequest {}

impl Serializer for GetPalwClassesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwClassesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

/// One class (`palw_state_v2::PalwClassRowV2`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwClassRow {
    pub class_id: String,
    pub is_base_class: bool,
    pub status: String,
    /// `None`: the class holds no share at all — not the same as a share with no budget.
    pub share_permille: Option<u16>,
    pub budget_blocks: u64,
    pub canonical_leaves: u64,
    pub artifact_root: String,
    pub fp_certified: bool,
    pub held: bool,
    pub registered_daa: u64,
}

impl Serializer for RpcPalwClassRow {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(bool, &self.is_base_class, writer)?;
        store!(String, &self.status, writer)?;
        store!(Option<u16>, &self.share_permille, writer)?;
        store!(u64, &self.budget_blocks, writer)?;
        store!(u64, &self.canonical_leaves, writer)?;
        store!(String, &self.artifact_root, writer)?;
        store!(bool, &self.fp_certified, writer)?;
        store!(bool, &self.held, writer)?;
        store!(u64, &self.registered_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwClassRow {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            class_id: load!(String, reader)?,
            is_base_class: load!(bool, reader)?,
            status: load!(String, reader)?,
            share_permille: load!(Option<u16>, reader)?,
            budget_blocks: load!(u64, reader)?,
            canonical_leaves: load!(u64, reader)?,
            artifact_root: load!(String, reader)?,
            fp_certified: load!(bool, reader)?,
            held: load!(bool, reader)?,
            registered_daa: load!(u64, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwClassesResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub classes: Vec<RpcPalwClassRow>,
}

impl Serializer for GetPalwClassesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        serialize!(Vec<RpcPalwClassRow>, &self.classes, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwClassesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let tip_daa = load!(u64, reader)?;
        let classes = deserialize!(Vec<RpcPalwClassRow>, reader)?;
        Ok(Self { available, tip_daa, classes })
    }
}

/// ADR-0122 §8.2: `getPalwRegistrationTerms` — ADR-0108 §8's read, built. What a class
/// registration must take from the chain rather than choose (`PalwRegistrationTermsV2`, one state
/// read), and every family the chain certified on both lanes. A registrant built from genesis terms
/// is refused as soon as the base class retargets; a lane binding no chain family covers is dropped
/// with its fee; a family filed twice is refused `FamilyAlreadyCertified` — all three were decidable
/// only from the node's log.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwRegistrationTermsRequest {}

impl Serializer for GetPalwRegistrationTermsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwRegistrationTermsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

/// One family the chain certified.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwCertifiedFamily {
    /// `attempt` or `free_prompt`.
    pub lane: String,
    pub digest: String,
    pub certified_daa: u64,
    /// The `PalwE2eFamilyV1`, borsh, hex — its kernel set is what a class's reachable kernels are
    /// checked against, and a client decodes it into the type the SDK's builder takes.
    pub family_hex: String,
}

impl Serializer for RpcPalwCertifiedFamily {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.lane, writer)?;
        store!(String, &self.digest, writer)?;
        store!(u64, &self.certified_daa, writer)?;
        store!(String, &self.family_hex, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwCertifiedFamily {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            lane: load!(String, reader)?,
            digest: load!(String, reader)?,
            certified_daa: load!(u64, reader)?,
            family_hex: load!(String, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwRegistrationTermsResponse {
    /// False off `ConsensusV2`, or before the state has a tip.
    pub available: bool,
    pub tip_daa: u64,
    pub base_class_id: String,
    pub min_grantable_share_permille: u16,
    pub slash_value_per_pwu: u64,
    /// The base class's current target, decimal (`u128`).
    pub initial_target: String,
    pub registered_class_ids: Vec<String>,
    pub registered_artifact_roots: Vec<String>,
    pub families: Vec<RpcPalwCertifiedFamily>,
}

impl Serializer for GetPalwRegistrationTermsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(String, &self.base_class_id, writer)?;
        store!(u16, &self.min_grantable_share_permille, writer)?;
        store!(u64, &self.slash_value_per_pwu, writer)?;
        store!(String, &self.initial_target, writer)?;
        store!(Vec<String>, &self.registered_class_ids, writer)?;
        store!(Vec<String>, &self.registered_artifact_roots, writer)?;
        serialize!(Vec<RpcPalwCertifiedFamily>, &self.families, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwRegistrationTermsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            base_class_id: load!(String, reader)?,
            min_grantable_share_permille: load!(u16, reader)?,
            slash_value_per_pwu: load!(u64, reader)?,
            initial_target: load!(String, reader)?,
            registered_class_ids: load!(Vec<String>, reader)?,
            registered_artifact_roots: load!(Vec<String>, reader)?,
            families: deserialize!(Vec<RpcPalwCertifiedFamily>, reader)?,
        })
    }
}

/// ADR-0125 §7.4: `getPalwRoundLane` — the execution lane as the node's sink state holds it. Rounds
/// are whole seconds since genesis; the node answers for the round its clock is in.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwRoundLaneRequest {
    /// Lane SCAN (version 2): how many recent executions to return; 0 asks for none, and the node
    /// caps it at 256.
    #[serde(default)]
    pub executions_limit: u32,
}

impl Serializer for GetPalwRoundLaneRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(u32, &self.executions_limit, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwRoundLaneRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        Ok(Self { executions_limit: if version >= 2 { load!(u32, reader)? } else { 0 } })
    }
}

/// One stage of the lane's width: from the first span opening at or past `activation_daa`, a round
/// holds `permits_per_round` permits. The first stage is the lane's opening.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwRoundLaneStage {
    pub activation_daa: u64,
    pub permits_per_round: u16,
}

impl Serializer for RpcPalwRoundLaneStage {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.activation_daa, writer)?;
        store!(u16, &self.permits_per_round, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwRoundLaneStage {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { activation_daa: load!(u64, reader)?, permits_per_round: load!(u16, reader)? })
    }
}

/// One permit of the current round.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwRoundPermit {
    pub index: u16,
    /// `<txid>:<index>` of the bond the permit is drawn to.
    pub bond: String,
    pub operator_id: String,
    pub domain: String,
    /// Accepted on the sink's chain already.
    pub used: bool,
}

impl Serializer for RpcPalwRoundPermit {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u16, &self.index, writer)?;
        store!(String, &self.bond, writer)?;
        store!(String, &self.operator_id, writer)?;
        store!(String, &self.domain, writer)?;
        store!(bool, &self.used, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwRoundPermit {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            index: load!(u16, reader)?,
            bond: load!(String, reader)?,
            operator_id: load!(String, reader)?,
            domain: load!(String, reader)?,
            used: load!(bool, reader)?,
        })
    }
}

/// One security domain of the span's schedule.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwRoundLaneDomain {
    pub domain: String,
    /// The canonical compute (pwu) its attempts finalized in the span before: each `Final` credits
    /// its class's inference, capped at the heaviest weight-bearing model class's.
    pub credits: u64,
    pub quota_permille: u16,
    /// The rounds it holds permits in: even (0) or odd (1).
    pub parity: u8,
    pub bonds: u32,
}

impl Serializer for RpcPalwRoundLaneDomain {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.domain, writer)?;
        store!(u64, &self.credits, writer)?;
        store!(u16, &self.quota_permille, writer)?;
        store!(u8, &self.parity, writer)?;
        store!(u32, &self.bonds, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwRoundLaneDomain {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            domain: load!(String, reader)?,
            credits: load!(u64, reader)?,
            quota_permille: load!(u16, reader)?,
            parity: load!(u8, reader)?,
            bonds: load!(u32, reader)?,
        })
    }
}

/// **Lane SCAN: one round block the lane's ledger names** (the last accepted, the newest judged).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwRoundMark {
    pub hash: String,
    pub round: u64,
    pub daa_score: u64,
    pub timestamp_ms: u64,
    pub bond: String,
    pub claim_id: Option<String>,
    /// `granted` or `refused`.
    pub verdict: String,
}

impl Serializer for RpcPalwRoundMark {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.hash, writer)?;
        store!(u64, &self.round, writer)?;
        store!(u64, &self.daa_score, writer)?;
        store!(u64, &self.timestamp_ms, writer)?;
        store!(String, &self.bond, writer)?;
        store!(Option<String>, &self.claim_id, writer)?;
        store!(String, &self.verdict, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwRoundMark {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            hash: load!(String, reader)?,
            round: load!(u64, reader)?,
            daa_score: load!(u64, reader)?,
            timestamp_ms: load!(u64, reader)?,
            bond: load!(String, reader)?,
            claim_id: load!(Option<String>, reader)?,
            verdict: load!(String, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwRefusalCount {
    pub reason: String,
    pub count: u64,
}

impl Serializer for RpcPalwRefusalCount {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.reason, writer)?;
        store!(u64, &self.count, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwRefusalCount {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { reason: load!(String, reader)?, count: load!(u64, reader)? })
    }
}

/// **Lane SCAN: the round lane's health as THIS node has seen it** — from its own ledger, which a
/// restart empties (`ledger_since_ms` says from when it counts).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwRoundLaneHealth {
    pub accepted_total: u64,
    pub refused_total: u64,
    /// Refusals since the last accepted round block (all of them while none was accepted).
    pub refused_since_last_accepted: u64,
    pub last_accepted: Option<RpcPalwRoundMark>,
    pub latest: Option<RpcPalwRoundMark>,
    /// Most frequent first.
    pub top_refusals: Vec<RpcPalwRefusalCount>,
    /// No accepted round block for `stale_after_ms` by the node's clock.
    pub stale: bool,
    pub stale_after_ms: u64,
    /// The node's clock when it answered, and when its ledger started counting.
    pub now_ms: u64,
    pub ledger_since_ms: u64,
}

impl Serializer for RpcPalwRoundLaneHealth {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.accepted_total, writer)?;
        store!(u64, &self.refused_total, writer)?;
        store!(u64, &self.refused_since_last_accepted, writer)?;
        serialize!(Option<RpcPalwRoundMark>, &self.last_accepted, writer)?;
        serialize!(Option<RpcPalwRoundMark>, &self.latest, writer)?;
        serialize!(Vec<RpcPalwRefusalCount>, &self.top_refusals, writer)?;
        store!(bool, &self.stale, writer)?;
        store!(u64, &self.stale_after_ms, writer)?;
        store!(u64, &self.now_ms, writer)?;
        store!(u64, &self.ledger_since_ms, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwRoundLaneHealth {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            accepted_total: load!(u64, reader)?,
            refused_total: load!(u64, reader)?,
            refused_since_last_accepted: load!(u64, reader)?,
            last_accepted: deserialize!(Option<RpcPalwRoundMark>, reader)?,
            latest: deserialize!(Option<RpcPalwRoundMark>, reader)?,
            top_refusals: deserialize!(Vec<RpcPalwRefusalCount>, reader)?,
            stale: load!(bool, reader)?,
            stale_after_ms: load!(u64, reader)?,
            now_ms: load!(u64, reader)?,
            ledger_since_ms: load!(u64, reader)?,
        })
    }
}

/// **Lane SCAN: one execution** — a claim whose Final earned execution-lane tickets: the rounds it is
/// permitted (`tickets`) and has spent (`tickets_spent`), read from the sink's state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwExecutionRow {
    pub claim_id: String,
    /// Empty once the claim retired from the state.
    pub class_id: String,
    pub executor_bond: String,
    pub domain: String,
    /// The lane's stage: `credited`, `maturing` or `scheduled`.
    pub stage: String,
    /// `credited`, `maturing`, `running`, `complete`, `no_tickets` or `voided`.
    pub status: String,
    pub credit: u64,
    pub span: u64,
    pub tickets: u32,
    pub tickets_spent: u32,
    pub first_round: Option<u64>,
    pub last_round: Option<u64>,
    pub accepted_daa: Option<u64>,
    pub accepted_block: Option<String>,
    /// The claim's phase while the state keeps it (`final`, `voided`, ...), else empty.
    pub phase: String,
    pub final_daa: Option<u64>,
}

impl Serializer for RpcPalwExecutionRow {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.executor_bond, writer)?;
        store!(String, &self.domain, writer)?;
        store!(String, &self.stage, writer)?;
        store!(String, &self.status, writer)?;
        store!(u64, &self.credit, writer)?;
        store!(u64, &self.span, writer)?;
        store!(u32, &self.tickets, writer)?;
        store!(u32, &self.tickets_spent, writer)?;
        store!(Option<u64>, &self.first_round, writer)?;
        store!(Option<u64>, &self.last_round, writer)?;
        store!(Option<u64>, &self.accepted_daa, writer)?;
        store!(Option<String>, &self.accepted_block, writer)?;
        store!(String, &self.phase, writer)?;
        store!(Option<u64>, &self.final_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwExecutionRow {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            claim_id: load!(String, reader)?,
            class_id: load!(String, reader)?,
            executor_bond: load!(String, reader)?,
            domain: load!(String, reader)?,
            stage: load!(String, reader)?,
            status: load!(String, reader)?,
            credit: load!(u64, reader)?,
            span: load!(u64, reader)?,
            tickets: load!(u32, reader)?,
            tickets_spent: load!(u32, reader)?,
            first_round: load!(Option<u64>, reader)?,
            last_round: load!(Option<u64>, reader)?,
            accepted_daa: load!(Option<u64>, reader)?,
            accepted_block: load!(Option<String>, reader)?,
            phase: load!(String, reader)?,
            final_daa: load!(Option<u64>, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwRoundLaneResponse {
    /// The network configures the lane.
    pub armed: bool,
    /// The lane is open at the virtual DAA score and the sink's state could be read.
    pub open: bool,
    pub schedule_span_daa: u64,
    pub max_per_mergeset: u64,
    /// The opening stage, then each widening, in height order.
    pub stages: Vec<RpcPalwRoundLaneStage>,
    pub virtual_daa: u64,
    pub round: u64,
    /// The span a round block built now hangs in, and its width: the most permits a round can hold.
    pub span: u64,
    pub permits_per_round: u16,
    /// This round's permits. A domain holds at most a third of a round's width and only rounds of
    /// its parity, and one operator one permit, so a round can hold fewer than the width.
    pub permits: Vec<RpcPalwRoundPermit>,
    pub domains: Vec<RpcPalwRoundLaneDomain>,
    pub accepted_in_span: u64,
    /// The span whose finalized attempts are being recorded, and their count. They are the
    /// participants of `finals_span + 2`, fixed at the first chain block of `finals_span + 1` and
    /// seeded into that span's schedule at its own first chain block (ADR-0130).
    pub finals_span: u64,
    pub finals: u64,
    /// The permits the schedule grants the round after this one — the other parity — so this
    /// round's and the next's together are the blocks per second the span can actually carry.
    pub next_round_permits: u16,
    /// **Version 3 (lane SCAN, node-only)**: the lane's health from this node's ledger, `None` on a
    /// node without it.
    #[serde(default)]
    pub health: Option<RpcPalwRoundLaneHealth>,
    /// The executions the sink's state holds, newest first (at most the request's `executions_limit`),
    /// and how many it holds in all.
    #[serde(default)]
    pub recent_executions: Vec<RpcPalwExecutionRow>,
    #[serde(default)]
    pub executions_total: u64,
}

impl Serializer for GetPalwRoundLaneResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &3, writer)?;
        store!(bool, &self.armed, writer)?;
        store!(bool, &self.open, writer)?;
        store!(u64, &self.schedule_span_daa, writer)?;
        store!(u64, &self.max_per_mergeset, writer)?;
        serialize!(Vec<RpcPalwRoundLaneStage>, &self.stages, writer)?;
        store!(u64, &self.virtual_daa, writer)?;
        store!(u64, &self.round, writer)?;
        store!(u64, &self.span, writer)?;
        store!(u16, &self.permits_per_round, writer)?;
        serialize!(Vec<RpcPalwRoundPermit>, &self.permits, writer)?;
        serialize!(Vec<RpcPalwRoundLaneDomain>, &self.domains, writer)?;
        store!(u64, &self.accepted_in_span, writer)?;
        store!(u64, &self.finals_span, writer)?;
        store!(u64, &self.finals, writer)?;
        store!(u16, &self.next_round_permits, writer)?;
        serialize!(Option<RpcPalwRoundLaneHealth>, &self.health, writer)?;
        serialize!(Vec<RpcPalwExecutionRow>, &self.recent_executions, writer)?;
        store!(u64, &self.executions_total, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwRoundLaneResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let mut response = Self {
            armed: load!(bool, reader)?,
            open: load!(bool, reader)?,
            schedule_span_daa: load!(u64, reader)?,
            max_per_mergeset: load!(u64, reader)?,
            stages: deserialize!(Vec<RpcPalwRoundLaneStage>, reader)?,
            virtual_daa: load!(u64, reader)?,
            round: load!(u64, reader)?,
            span: load!(u64, reader)?,
            permits_per_round: load!(u16, reader)?,
            permits: deserialize!(Vec<RpcPalwRoundPermit>, reader)?,
            domains: deserialize!(Vec<RpcPalwRoundLaneDomain>, reader)?,
            accepted_in_span: load!(u64, reader)?,
            finals_span: load!(u64, reader)?,
            finals: load!(u64, reader)?,
            next_round_permits: if version >= 2 { load!(u16, reader)? } else { 0 },
            ..Default::default()
        };
        if version >= 3 {
            response.health = deserialize!(Option<RpcPalwRoundLaneHealth>, reader)?;
            response.recent_executions = deserialize!(Vec<RpcPalwExecutionRow>, reader)?;
            response.executions_total = load!(u64, reader)?;
        }
        Ok(response)
    }
}

/// ADR-0127 Decision 3: `getPalwSettlement` — whether what the selected chain accepted at
/// `daa_score` (a UTXO entry's `block_daa_score`) is settled, and its settlement depth, read from
/// the sink's PALW state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwSettlementRequest {
    pub daa_score: u64,
}

impl Serializer for GetPalwSettlementRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.daa_score, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwSettlementRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { daa_score: load!(u64, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwSettlementResponse {
    /// The node keeps PALW V2 state and can date its safe frontier. When false, every field but
    /// `daa_score` and `sink_daa` is a default and says nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_retired_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_settlement: Option<kaspa_consensus_core::palw_native_settlement_v1::NativeSettlementSnapshotV1>,
    /// RFC-0012 D1: why `safe` stands where it does at the answer's sink — the structured reasons (waiting on maturity, an open
    /// claim or court session, a retention gap, missing history) behind the snapshot's single `stop`. Advisory, never consensus.
    /// Absent before the retirement fence, on a node that keeps no evaluation, and when the sink moved between the two reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_readiness: Option<kaspa_consensus_core::palw_native_readiness_v1::NativeSafeReadinessV1>,
    pub available: bool,
    /// The DAA score of the chain block the answer stands at.
    pub sink_daa: u64,
    /// The accepting DAA score this answer is about.
    pub daa_score: u64,
    /// The safe frontier stands at or past `daa_score`: what the chain accepted there is settled.
    pub settled: bool,
    /// `Final` PALW anchors at or after `daa_score` — anchors, never blocks.
    pub depth: u64,
    /// Anchors at or after `daa_score` whose claims are still on their way to `Final`.
    pub pending_anchors: u64,
    /// Anchors at or after `daa_score` may have retired from the state: the depth is at least `depth`.
    pub depth_is_lower_bound: bool,
    pub safe_frontier_blue_score: u64,
    pub safe_frontier_daa: u64,
    /// **RFC-0018 §7.8**: attempt claims at or after `daa_score` that are `Final` but not SecurityCertified (no consensus or settlement
    /// credit). Past Gate C a chain holding only such claims reports `settled = false`, `depth = 0`. Wire version 4, written only when
    /// non-zero (0 on every shipped network, so older readers are unaffected); not carried over gRPC (RFC-0018 GAP-R6).
    #[serde(default)]
    pub uncertified_final_anchors: u64,
    /// **RFC-0018 §7.2 — the premise, stated**: present wherever Gate C is in force at the answer's sink. PALW settlement finality there
    /// rests on SecurityCertified claims, whose certification is PREMISE-BASED (drawn verifiers' signed approvals, sound only while the
    /// honest fraction of drawn verifier stake is at least the threshold), not an unconditional mathematical proof. Wire version 4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security_certification_premise: Option<String>,
}

impl Serializer for GetPalwSettlementResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let version: u16 = if self.uncertified_final_anchors > 0 || self.security_certification_premise.is_some() {
            4
        } else if self.native_readiness.is_some() {
            3
        } else if self.dns_retired_at.is_some() || self.native_settlement.is_some() {
            2
        } else {
            1
        };
        store!(u16, &version, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.sink_daa, writer)?;
        store!(u64, &self.daa_score, writer)?;
        store!(bool, &self.settled, writer)?;
        store!(u64, &self.depth, writer)?;
        store!(u64, &self.pending_anchors, writer)?;
        store!(bool, &self.depth_is_lower_bound, writer)?;
        store!(u64, &self.safe_frontier_blue_score, writer)?;
        store!(u64, &self.safe_frontier_daa, writer)?;
        if version >= 2 {
            store!(Option<u64>, &self.dns_retired_at, writer)?;
            let json = self.native_settlement.as_ref().map(serde_json::to_string).transpose().map_err(std::io::Error::other)?;
            store!(Option<String>, &json, writer)?;
        }
        if version >= 3 {
            let json = self.native_readiness.as_ref().map(serde_json::to_string).transpose().map_err(std::io::Error::other)?;
            store!(Option<String>, &json, writer)?;
        }
        if version >= 4 {
            store!(u64, &self.uncertified_final_anchors, writer)?;
            store!(Option<String>, &self.security_certification_premise, writer)?;
        }
        Ok(())
    }
}

impl Deserializer for GetPalwSettlementResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if !(1..=4).contains(&version) {
            return Err(std::io::Error::other("unknown PALW settlement response version"));
        }
        Ok(Self {
            available: load!(bool, reader)?,
            sink_daa: load!(u64, reader)?,
            daa_score: load!(u64, reader)?,
            settled: load!(bool, reader)?,
            depth: load!(u64, reader)?,
            pending_anchors: load!(u64, reader)?,
            depth_is_lower_bound: load!(bool, reader)?,
            safe_frontier_blue_score: load!(u64, reader)?,
            safe_frontier_daa: load!(u64, reader)?,
            dns_retired_at: if version >= 2 { load!(Option<u64>, reader)? } else { None },
            native_settlement: if version >= 2 {
                load!(Option<String>, reader)?.map(|s| serde_json::from_str(&s)).transpose().map_err(std::io::Error::other)?
            } else {
                None
            },
            native_readiness: if version >= 3 {
                load!(Option<String>, reader)?.map(|s| serde_json::from_str(&s)).transpose().map_err(std::io::Error::other)?
            } else {
                None
            },
            uncertified_final_anchors: if version >= 4 { load!(u64, reader)? } else { 0 },
            security_certification_premise: if version >= 4 { load!(Option<String>, reader)? } else { None },
        })
    }
}

/// MISAKA §5 round 2: `getPrecommitDuty` — what the precommit round asks of one validator at the
/// node's sink.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPrecommitDutyRequest {
    /// 64-byte validator id (`validator_pubkey_hash`, Hash64 hex).
    pub validator_id: String,
    /// Stake-bond outpoint, "txid_hex:index" (txid is a 64-byte Hash64 = 128 hex chars).
    pub bond_outpoint: String,
}

impl Serializer for GetPrecommitDutyRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.validator_id, writer)?;
        store!(String, &self.bond_outpoint, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPrecommitDutyRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { validator_id: load!(String, reader)?, bond_outpoint: load!(String, reader)? })
    }
}

/// One epoch a validator still owes a precommit for.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPrecommitDue {
    pub epoch: u64,
    /// The epoch's anchor (Hash64 hex).
    pub anchor_hash: String,
    pub anchor_daa_score: u64,
    /// The epoch's frozen snapshot commitment the precommit binds (Hash64 hex).
    pub snapshot_commitment: String,
}

impl Serializer for RpcPrecommitDue {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.epoch, writer)?;
        store!(String, &self.anchor_hash, writer)?;
        store!(u64, &self.anchor_daa_score, writer)?;
        store!(String, &self.snapshot_commitment, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPrecommitDue {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            epoch: load!(u64, reader)?,
            anchor_hash: load!(String, reader)?,
            anchor_daa_score: load!(u64, reader)?,
            snapshot_commitment: load!(String, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPrecommitDutyResponse {
    /// False when the node has no duty view for this validator (no overlay, or the read is not
    /// implemented on this node); every other field is then a default. A malformed id or outpoint
    /// is a request error.
    /// RFC-0012 retirement is distinct from a dormant or temporarily unavailable duty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired_at: Option<u64>,
    pub available: bool,
    /// The precommit round is live at the sink.
    pub round_active: bool,
    pub sink_daa_score: u64,
    /// The lock this validator carries on the selected chain, as counted: its epoch and anchor
    /// (Hash64 hex). `0` and the zero hash before its first counted precommit.
    pub held_epoch: u64,
    pub held_anchor: String,
    /// Epochs whose prevote quorum is met and which this validator has not precommitted, ascending.
    pub due: Vec<RpcPrecommitDue>,
}

impl Serializer for GetPrecommitDutyResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &if self.retired_at.is_some() { 2 } else { 1 }, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.round_active, writer)?;
        store!(u64, &self.sink_daa_score, writer)?;
        store!(u64, &self.held_epoch, writer)?;
        store!(String, &self.held_anchor, writer)?;
        serialize!(Vec<RpcPrecommitDue>, &self.due, writer)?;
        if self.retired_at.is_some() {
            store!(Option<u64>, &self.retired_at, writer)?;
        }
        Ok(())
    }
}

impl Deserializer for GetPrecommitDutyResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 && version != 2 {
            return Err(std::io::Error::other("unknown precommit duty response version"));
        }
        Ok(Self {
            available: load!(bool, reader)?,
            round_active: load!(bool, reader)?,
            sink_daa_score: load!(u64, reader)?,
            held_epoch: load!(u64, reader)?,
            held_anchor: load!(String, reader)?,
            due: deserialize!(Vec<RpcPrecommitDue>, reader)?,
            retired_at: if version == 2 { load!(Option<u64>, reader)? } else { None },
        })
    }
}

/// `getPalwClassContexts` — the context each registered PALW class runs at, and where the numbers
/// came from. Its own op rather than new fields on `RpcPalwClassRow`, whose rows are versioned per
/// element: a field added there breaks every old client reading a new node.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwClassContextsRequest {}

impl Serializer for GetPalwClassContextsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwClassContextsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

/// One class's context.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwClassContext {
    /// 128 hex, as `getPalwClasses` spells it.
    pub class_id: String,
    /// The model the node's build ledger names for the class; empty when it names none.
    pub model_id: String,
    /// The class's window in cached positions. Every job of the class has a footprint at most this.
    pub n_ctx: u32,
    /// The canonical job's prefill and decode token counts.
    pub canonical_prefill_tokens: u32,
    pub canonical_decode_tokens: u32,
    /// **The canonical job's footprint**: the cached positions it occupies,
    /// `prefill + max(1, decode) − 1` (`palw_context_ladder::palw_job_footprint_v1`) — the first
    /// generated token is sampled from the prefill's last logits, so a job makes `decode − 1` decode
    /// calls. At most `n_ctx`, and equal to it when the job is sized to the window: a (7, 2) job at
    /// `n_ctx` 8 is a footprint of 8 and valid. Adding prefill and decode and comparing that with
    /// `n_ctx` overstates the job by one position and reports an overflow that is not there.
    pub canonical_footprint_positions: u32,
    /// The canonical job's declared context bound.
    pub max_context_tokens: u32,
    /// Where the numbers came from: `chain_registration` (the declaration the chain registered),
    /// `build_ledger` (this node's build knows the class) or `unknown` (the numbers are zeros).
    pub source: String,
}

impl Serializer for RpcPalwClassContext {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.model_id, writer)?;
        store!(u32, &self.n_ctx, writer)?;
        store!(u32, &self.canonical_prefill_tokens, writer)?;
        store!(u32, &self.canonical_decode_tokens, writer)?;
        store!(u32, &self.canonical_footprint_positions, writer)?;
        store!(u32, &self.max_context_tokens, writer)?;
        store!(String, &self.source, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwClassContext {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            class_id: load!(String, reader)?,
            model_id: load!(String, reader)?,
            n_ctx: load!(u32, reader)?,
            canonical_prefill_tokens: load!(u32, reader)?,
            canonical_decode_tokens: load!(u32, reader)?,
            canonical_footprint_positions: load!(u32, reader)?,
            max_context_tokens: load!(u32, reader)?,
            source: load!(String, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwClassContextsResponse {
    /// The node's class table is non-empty.
    pub available: bool,
    /// The free-prompt lane's token limits from the network's V2 bundle; 0 with no bundle. That lane
    /// admits a job with `prompt_tokens + decode_token_limit <= max_context_tokens <= n_ctx`
    /// (`palw_freeprompt_v3`) — one position stricter than the footprint rule — so a client sizing
    /// its own prompt for the lane gets at most `n_ctx − prompt` tokens back.
    pub fp_max_prompt_tokens: u32,
    pub fp_max_decode_tokens: u32,
    pub classes: Vec<RpcPalwClassContext>,
}

impl Serializer for GetPalwClassContextsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u32, &self.fp_max_prompt_tokens, writer)?;
        store!(u32, &self.fp_max_decode_tokens, writer)?;
        serialize!(Vec<RpcPalwClassContext>, &self.classes, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwClassContextsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            fp_max_prompt_tokens: load!(u32, reader)?,
            fp_max_decode_tokens: load!(u32, reader)?,
            classes: deserialize!(Vec<RpcPalwClassContext>, reader)?,
        })
    }
}

/// ADR-0131 Decision 1: `getPalwClassEconomics` — what the chain holds for each class and what its
/// job costs in economic compute, so a client can price every class on every reward basis and read
/// `MSK / compute` per class. Nothing here is a rule: the numbers a client derives are the shadow's.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwClassEconomicsRequest {}

impl Serializer for GetPalwClassEconomicsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwClassEconomicsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

/// ADR-0132: what this node's ledger holds for one class — the attempt-lane claims it followed from
/// acceptance to their terminal phase, the sompi their `Final`s named (by the rule in force then),
/// and the compute they cost: `attempted` = Σ class draws × network draws × one draw's job over
/// every claim, `final` = Σ one draw's job over the `Final`s, `verification` = Σ seats × one draw's
/// job over the claims that bound. Rates are sompi per 10⁹ MAC-eq, decimal `u128`; waits are mean
/// DAA; `available` is false on a node without a recorder. Node-local: two nodes hold two windows.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwClassLedgerTotals {
    pub available: bool,
    pub claims: u64,
    pub bound: u64,
    pub licensed: u64,
    pub finals: u64,
    pub voided: u64,
    pub redrawn: u64,
    pub paid_at_acceptance: u64,
    pub escrow_final_sompi: String,
    /// What the `Final`s NAMED the producers and panels — paid below ADR-0152's `palw_rcore_plus`,
    /// VESTED past it (testnet-12), where the row can still burn; `getPalwVesting` says which moved.
    /// Renamed from `producerPaidSompi` / `panelPaidSompi` (the Borsh position is unchanged, and a
    /// JSON reader still accepts the old names).
    #[serde(alias = "producerPaidSompi")]
    pub producer_named_sompi: String,
    #[serde(alias = "panelPaidSompi")]
    pub panel_named_sompi: String,
    pub reserve_sompi: String,
    pub burned_sompi: String,
    pub attempted_compute: String,
    pub final_compute: String,
    pub verification_compute: String,
    pub producer_per_attempted_compute: String,
    pub panel_per_verification_compute: String,
    pub total_per_attempted_compute: String,
    pub total_per_final_compute: String,
    pub licence_rate_permille: u32,
    pub final_of_licensed_permille: u32,
    pub final_rate_permille: u32,
    pub avg_bind_wait_daa: u64,
    pub avg_licence_wait_daa: u64,
    pub avg_final_wait_daa: u64,
    pub avg_void_wait_daa: u64,
    pub avg_expected_attempts_q32: String,
    pub avg_network_expected_attempts_q32: String,
    pub first_accepted_daa: u64,
    pub last_accepted_daa: u64,
}

impl Serializer for RpcPalwClassLedgerTotals {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.claims, writer)?;
        store!(u64, &self.bound, writer)?;
        store!(u64, &self.licensed, writer)?;
        store!(u64, &self.finals, writer)?;
        store!(u64, &self.voided, writer)?;
        store!(u64, &self.redrawn, writer)?;
        store!(u64, &self.paid_at_acceptance, writer)?;
        store!(String, &self.escrow_final_sompi, writer)?;
        store!(String, &self.producer_named_sompi, writer)?;
        store!(String, &self.panel_named_sompi, writer)?;
        store!(String, &self.reserve_sompi, writer)?;
        store!(String, &self.burned_sompi, writer)?;
        store!(String, &self.attempted_compute, writer)?;
        store!(String, &self.final_compute, writer)?;
        store!(String, &self.verification_compute, writer)?;
        store!(String, &self.producer_per_attempted_compute, writer)?;
        store!(String, &self.panel_per_verification_compute, writer)?;
        store!(String, &self.total_per_attempted_compute, writer)?;
        store!(String, &self.total_per_final_compute, writer)?;
        store!(u32, &self.licence_rate_permille, writer)?;
        store!(u32, &self.final_of_licensed_permille, writer)?;
        store!(u32, &self.final_rate_permille, writer)?;
        store!(u64, &self.avg_bind_wait_daa, writer)?;
        store!(u64, &self.avg_licence_wait_daa, writer)?;
        store!(u64, &self.avg_final_wait_daa, writer)?;
        store!(u64, &self.avg_void_wait_daa, writer)?;
        store!(String, &self.avg_expected_attempts_q32, writer)?;
        store!(String, &self.avg_network_expected_attempts_q32, writer)?;
        store!(u64, &self.first_accepted_daa, writer)?;
        store!(u64, &self.last_accepted_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwClassLedgerTotals {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            claims: load!(u64, reader)?,
            bound: load!(u64, reader)?,
            licensed: load!(u64, reader)?,
            finals: load!(u64, reader)?,
            voided: load!(u64, reader)?,
            redrawn: load!(u64, reader)?,
            paid_at_acceptance: load!(u64, reader)?,
            escrow_final_sompi: load!(String, reader)?,
            producer_named_sompi: load!(String, reader)?,
            panel_named_sompi: load!(String, reader)?,
            reserve_sompi: load!(String, reader)?,
            burned_sompi: load!(String, reader)?,
            attempted_compute: load!(String, reader)?,
            final_compute: load!(String, reader)?,
            verification_compute: load!(String, reader)?,
            producer_per_attempted_compute: load!(String, reader)?,
            panel_per_verification_compute: load!(String, reader)?,
            total_per_attempted_compute: load!(String, reader)?,
            total_per_final_compute: load!(String, reader)?,
            licence_rate_permille: load!(u32, reader)?,
            final_of_licensed_permille: load!(u32, reader)?,
            final_rate_permille: load!(u32, reader)?,
            avg_bind_wait_daa: load!(u64, reader)?,
            avg_licence_wait_daa: load!(u64, reader)?,
            avg_final_wait_daa: load!(u64, reader)?,
            avg_void_wait_daa: load!(u64, reader)?,
            avg_expected_attempts_q32: load!(String, reader)?,
            avg_network_expected_attempts_q32: load!(String, reader)?,
            first_accepted_daa: load!(u64, reader)?,
            last_accepted_daa: load!(u64, reader)?,
        })
    }
}

/// ADR-0132: what THIS node did for the class — its producer's draws (class wins, blocks, the
/// milliseconds a forward took, the MiB read from storage during the draws, ADR-0112) and its
/// seat's replays and receipts. Counters since the process started; `available` is false where the
/// node neither produces nor seats the class.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwClassNodeTelemetry {
    pub available: bool,
    pub draws: u64,
    pub class_wins: u64,
    pub produced: u64,
    pub draw_millis: u64,
    pub storage_read_mib: u64,
    pub replays: u64,
    pub replay_millis: u64,
    pub replay_leaves: u64,
    pub receipts_valid: u64,
    pub receipts_unavailable: u64,
    pub receipts_incapable: u64,
    pub receipts_other: u64,
    pub openings_held: u64,
}

impl Serializer for RpcPalwClassNodeTelemetry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.draws, writer)?;
        store!(u64, &self.class_wins, writer)?;
        store!(u64, &self.produced, writer)?;
        store!(u64, &self.draw_millis, writer)?;
        store!(u64, &self.storage_read_mib, writer)?;
        store!(u64, &self.replays, writer)?;
        store!(u64, &self.replay_millis, writer)?;
        store!(u64, &self.replay_leaves, writer)?;
        store!(u64, &self.receipts_valid, writer)?;
        store!(u64, &self.receipts_unavailable, writer)?;
        store!(u64, &self.receipts_incapable, writer)?;
        store!(u64, &self.receipts_other, writer)?;
        store!(u64, &self.openings_held, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwClassNodeTelemetry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            draws: load!(u64, reader)?,
            class_wins: load!(u64, reader)?,
            produced: load!(u64, reader)?,
            draw_millis: load!(u64, reader)?,
            storage_read_mib: load!(u64, reader)?,
            replays: load!(u64, reader)?,
            replay_millis: load!(u64, reader)?,
            replay_leaves: load!(u64, reader)?,
            receipts_valid: load!(u64, reader)?,
            receipts_unavailable: load!(u64, reader)?,
            receipts_incapable: load!(u64, reader)?,
            receipts_other: load!(u64, reader)?,
            openings_held: load!(u64, reader)?,
        })
    }
}

/// One class: its registration numbers, its target, its job's economic compute, and a census of the
/// attempt-lane claims the node's state still holds.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwClassEconomics {
    /// 128 hex, as `getPalwClasses` spells it.
    pub class_id: String,
    /// The model this node's build names for the class; empty when it names none.
    pub model_id: String,
    pub is_base_class: bool,
    pub status: String,
    /// The class's cadence share; 0 for a class holding none.
    pub share_permille: u16,
    /// The class's `pwu_per_inference`: its canonical job's step leaves, today's price basis.
    pub pwu_per_inference: u64,
    /// Decimal `u128`: the class target at the tip.
    pub class_target: String,
    /// The draws one claim costs in expectation at that target, as the fork-choice factor floors
    /// them, and the same in Q32 fixed point as a decimal `u128` (`palw_expected_attempts_q32_v1`).
    pub expected_attempts: u64,
    pub expected_attempts_q32: String,
    /// ADR-0131 `EconomicComputeV1`, in MAC-equivalents, decimal `u128`: the job an attempt runs at
    /// the tip's draw rule (`prefill_draw` on the response), and the canonical job. Both `"0"` and
    /// `economic_source` `unknown` when neither the chain's registration nor this build has the graph.
    pub economic_compute_job: String,
    pub economic_compute_canonical: String,
    /// `chain_registration`, `build_ledger` or `unknown`.
    pub economic_source: String,
    /// Attempt-lane claims in the state, by phase; `claims_redrawn` are those drawn a second panel.
    pub claims_accepted: u64,
    pub claims_provisional: u64,
    pub claims_panel_bound: u64,
    pub claims_licensed: u64,
    pub claims_final: u64,
    pub claims_voided: u64,
    pub claims_redrawn: u64,
    /// Decimal `u128`: Σ escrow over every accepted claim, and over the `Final` ones.
    pub escrow_accepted_sompi: String,
    pub escrow_final_sompi: String,
    /// ADR-0132: the node's end-to-end ledger for the class, and what this node itself did for it.
    pub ledger: RpcPalwClassLedgerTotals,
    pub telemetry: RpcPalwClassNodeTelemetry,
    /// ADR-0133: the panel's capacity facts — `Active` bonds that may judge the class, seats on duty
    /// over its bound claims, the seat exposure those duties hold and the eligible bonds' free
    /// collateral (decimal `u128`s).
    pub eligible_seats: u32,
    pub duty_seats_inflight: u64,
    pub seat_exposure_inflight_sompi: String,
    pub free_collateral_sompi: String,
}

impl Serializer for RpcPalwClassEconomics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.model_id, writer)?;
        store!(bool, &self.is_base_class, writer)?;
        store!(String, &self.status, writer)?;
        store!(u16, &self.share_permille, writer)?;
        store!(u64, &self.pwu_per_inference, writer)?;
        store!(String, &self.class_target, writer)?;
        store!(u64, &self.expected_attempts, writer)?;
        store!(String, &self.expected_attempts_q32, writer)?;
        store!(String, &self.economic_compute_job, writer)?;
        store!(String, &self.economic_compute_canonical, writer)?;
        store!(String, &self.economic_source, writer)?;
        store!(u64, &self.claims_accepted, writer)?;
        store!(u64, &self.claims_provisional, writer)?;
        store!(u64, &self.claims_panel_bound, writer)?;
        store!(u64, &self.claims_licensed, writer)?;
        store!(u64, &self.claims_final, writer)?;
        store!(u64, &self.claims_voided, writer)?;
        store!(u64, &self.claims_redrawn, writer)?;
        store!(String, &self.escrow_accepted_sompi, writer)?;
        store!(String, &self.escrow_final_sompi, writer)?;
        serialize!(RpcPalwClassLedgerTotals, &self.ledger, writer)?;
        serialize!(RpcPalwClassNodeTelemetry, &self.telemetry, writer)?;
        store!(u32, &self.eligible_seats, writer)?;
        store!(u64, &self.duty_seats_inflight, writer)?;
        store!(String, &self.seat_exposure_inflight_sompi, writer)?;
        store!(String, &self.free_collateral_sompi, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwClassEconomics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            class_id: load!(String, reader)?,
            model_id: load!(String, reader)?,
            is_base_class: load!(bool, reader)?,
            status: load!(String, reader)?,
            share_permille: load!(u16, reader)?,
            pwu_per_inference: load!(u64, reader)?,
            class_target: load!(String, reader)?,
            expected_attempts: load!(u64, reader)?,
            expected_attempts_q32: load!(String, reader)?,
            economic_compute_job: load!(String, reader)?,
            economic_compute_canonical: load!(String, reader)?,
            economic_source: load!(String, reader)?,
            claims_accepted: load!(u64, reader)?,
            claims_provisional: load!(u64, reader)?,
            claims_panel_bound: load!(u64, reader)?,
            claims_licensed: load!(u64, reader)?,
            claims_final: load!(u64, reader)?,
            claims_voided: load!(u64, reader)?,
            claims_redrawn: load!(u64, reader)?,
            escrow_accepted_sompi: load!(String, reader)?,
            escrow_final_sompi: load!(String, reader)?,
            ledger: deserialize!(RpcPalwClassLedgerTotals, reader)?,
            telemetry: deserialize!(RpcPalwClassNodeTelemetry, reader)?,
            eligible_seats: load!(u32, reader)?,
            duty_seats_inflight: load!(u64, reader)?,
            seat_exposure_inflight_sompi: load!(String, reader)?,
            free_collateral_sompi: load!(String, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwClassEconomicsResponse {
    /// False off `ConsensusV2`.
    pub available: bool,
    /// The DAA the state was read at.
    pub tip_daa: u64,
    /// `PALW_ECONOMIC_COMPUTE_VERSION_V1`: the cost table the compute numbers were derived with.
    pub economic_compute_version: u16,
    /// The panel's seats — how many replays license one claim.
    pub seat_count: u16,
    /// Whether an attempt at the tip runs its class's job without decode calls (ADR-0117).
    pub prefill_draw: bool,
    /// ADR-0132: the tip's compact `bits` and the network draws a class win costs at it, Q32 as a
    /// decimal `u128` (2.0 at the difficulty floor).
    pub network_bits: u32,
    pub network_expected_attempts_q32: String,
    /// ADR-0132: whether this node keeps an end-to-end ledger, how many claims it holds, and the
    /// accepted-DAA span they cover.
    pub ledger_available: bool,
    pub ledger_claims: u64,
    pub ledger_first_daa: u64,
    pub ledger_last_daa: u64,
    pub classes: Vec<RpcPalwClassEconomics>,
}

impl Serializer for GetPalwClassEconomicsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(u16, &self.economic_compute_version, writer)?;
        store!(u16, &self.seat_count, writer)?;
        store!(bool, &self.prefill_draw, writer)?;
        store!(u32, &self.network_bits, writer)?;
        store!(String, &self.network_expected_attempts_q32, writer)?;
        store!(bool, &self.ledger_available, writer)?;
        store!(u64, &self.ledger_claims, writer)?;
        store!(u64, &self.ledger_first_daa, writer)?;
        store!(u64, &self.ledger_last_daa, writer)?;
        serialize!(Vec<RpcPalwClassEconomics>, &self.classes, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwClassEconomicsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            economic_compute_version: load!(u16, reader)?,
            seat_count: load!(u16, reader)?,
            prefill_draw: load!(bool, reader)?,
            network_bits: load!(u32, reader)?,
            network_expected_attempts_q32: load!(String, reader)?,
            ledger_available: load!(bool, reader)?,
            ledger_claims: load!(u64, reader)?,
            ledger_first_daa: load!(u64, reader)?,
            ledger_last_daa: load!(u64, reader)?,
            classes: deserialize!(Vec<RpcPalwClassEconomics>, reader)?,
        })
    }
}

/// ADR-0122: `getPalwNodeStatus` — what this node is doing, which until now only its log said.
/// ADR-0135: the model registry as the tip state holds it. Nothing here is set by a human: the
/// profile is derived from the class's graph, the state from chain-visible facts.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelRegistryRequest {}

impl Serializer for GetPalwModelRegistryRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelRegistryRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

/// One class in the registry: its lifecycle row (`has_row` false for a class registered before the
/// fence without a carriage — never gated), the work read off its graph, the profile derived from
/// it, the last boundary's reading, and the reading now.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelLifecycle {
    pub class_id: String,
    pub artifact_root: String,
    pub is_base_class: bool,
    pub has_row: bool,
    pub state: String,
    pub since_span: u64,
    pub verification_ccu: String,
    pub economic_ccu_per_claim: String,
    pub artifact_bytes: u64,
    pub ops_supported: bool,
    pub verification_window_spans: u32,
    pub artifact_prefetch_spans: u32,
    pub max_inflight_claims: u32,
    pub required_ready_seats: u32,
    /// **DEPRECATED — NOT A PRICE** (user decision 2026-09-25). The registry profile's derived
    /// `bond_per_span × verification_window_spans` (2,799,000 MSK for the 2M class): never charged,
    /// never reserved, refunded as 0 — a registration costs its 1 MSK burn and a carrier fee. Kept on
    /// the wire, with its meaning unchanged, for readers built before `recommended_pool_sompi`;
    /// present that field instead.
    pub registration_bond_sompi: u64,
    pub admission_claims_per_span_milli: u64,
    pub probes_passed: u32,
    pub probes_failed: u32,
    pub ready_seats: u32,
    pub inflight_claims: u32,
    pub utilization_permille: u32,
    pub admission_milli: u64,
    /// ADR-0132 Upgrade C: the class's cap utilization at the last boundary, in permille of the
    /// escrow; `0` where nothing priced it.
    pub cap_utilization_permille: u32,
    /// ADR-0135 §7: the share the registry priced the class's target for when it seated it
    /// (`attempt_target_seed_v1(share, pwu)`, once); `0` until it is seated.
    pub priced_share_permille: u16,
    /// ADR-0137 (shadow): the class's CCU, `CCU / W` in permille, the expected forwards a win
    /// (Q32), the work target's ticket beside the class target, the panel room in this class's
    /// claims, and the reader's share of finalized work over ten and a hundred epochs.
    pub work_ratio_permille: u32,
    pub expected_forwards_q32: String,
    pub work_ticket_target: String,
    pub class_target: String,
    pub panel_room: u64,
    pub final_work_share_10_permille: u16,
    pub final_work_share_100_permille: u16,
    pub ready_seats_now: u32,
    pub inflight_now: u32,
    pub share_permille: u16,
    pub no_capable_panel_voids: u32,
    /// Why the row is where it is, from its last reading — a HELD by the rule reads as one.
    pub reason: String,
    /// **The NON-BINDING recommended Activation Pool of the listing** (user decision 2026-09-25;
    /// wire version 2): `16 · A_MAX / α` of the pool's terms in force
    /// (`palw_activation_recommended_pool_sompi_v1`, 2,400 MSK at the terms of 2026-09-25) — what a
    /// sponsor is told pays the class's preparers in full. Nothing enforces it; 0 where the pool is
    /// not armed, for the floor (which takes no top-up), and from a node that serves version 1.
    pub recommended_pool_sompi: u64,
    /// **RFC-0017 (wire version 3): the class's eligibility stage** — `not-governed` (below `palw_model_eligibility_v1`, every
    /// shipped network, a genesis class, and from a node serving version 1 or 2), `registered`, `probation`, `eligible`, `lapsed`.
    pub eligibility_stage: String,
    /// The DAA the class's eligibility lapses at without a passed renewal (grace not included); 0 = none.
    pub eligibility_next_renewal_daa: u64,
    /// The stage's three rights, separately (RFC-0017 §6): subsidy, reward-bearing block right, Final weight.
    pub eligibility_subsidy: bool,
    pub eligibility_block_right: bool,
    pub eligibility_final_weight: bool,
    pub eligibility_exams_passed: u32,
    /// Exam attempts consumed by the class's lineage (all registrations of the same weights).
    pub eligibility_lineage_attempts: u32,
}

impl Serializer for RpcPalwModelLifecycle {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // Version 2 appends `recommended_pool_sompi` (the Activation Pool's P4, 2026-09-25); version 3 the RFC-0017 eligibility fields.
        store!(u16, &3, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.artifact_root, writer)?;
        store!(bool, &self.is_base_class, writer)?;
        store!(bool, &self.has_row, writer)?;
        store!(String, &self.state, writer)?;
        store!(u64, &self.since_span, writer)?;
        store!(String, &self.verification_ccu, writer)?;
        store!(String, &self.economic_ccu_per_claim, writer)?;
        store!(u64, &self.artifact_bytes, writer)?;
        store!(bool, &self.ops_supported, writer)?;
        store!(u32, &self.verification_window_spans, writer)?;
        store!(u32, &self.artifact_prefetch_spans, writer)?;
        store!(u32, &self.max_inflight_claims, writer)?;
        store!(u32, &self.required_ready_seats, writer)?;
        store!(u64, &self.registration_bond_sompi, writer)?;
        store!(u64, &self.admission_claims_per_span_milli, writer)?;
        store!(u32, &self.probes_passed, writer)?;
        store!(u32, &self.probes_failed, writer)?;
        store!(u32, &self.ready_seats, writer)?;
        store!(u32, &self.inflight_claims, writer)?;
        store!(u32, &self.utilization_permille, writer)?;
        store!(u64, &self.admission_milli, writer)?;
        store!(u32, &self.cap_utilization_permille, writer)?;
        store!(u16, &self.priced_share_permille, writer)?;
        store!(u32, &self.ready_seats_now, writer)?;
        store!(u32, &self.inflight_now, writer)?;
        store!(u16, &self.share_permille, writer)?;
        store!(u32, &self.no_capable_panel_voids, writer)?;
        store!(String, &self.reason, writer)?;
        store!(u32, &self.work_ratio_permille, writer)?;
        store!(String, &self.expected_forwards_q32, writer)?;
        store!(String, &self.work_ticket_target, writer)?;
        store!(String, &self.class_target, writer)?;
        store!(u64, &self.panel_room, writer)?;
        store!(u16, &self.final_work_share_10_permille, writer)?;
        store!(u16, &self.final_work_share_100_permille, writer)?;
        store!(u64, &self.recommended_pool_sompi, writer)?;
        store!(String, &self.eligibility_stage, writer)?;
        store!(u64, &self.eligibility_next_renewal_daa, writer)?;
        store!(bool, &self.eligibility_subsidy, writer)?;
        store!(bool, &self.eligibility_block_right, writer)?;
        store!(bool, &self.eligibility_final_weight, writer)?;
        store!(u32, &self.eligibility_exams_passed, writer)?;
        store!(u32, &self.eligibility_lineage_attempts, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelLifecycle {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        Ok(Self {
            class_id: load!(String, reader)?,
            artifact_root: load!(String, reader)?,
            is_base_class: load!(bool, reader)?,
            has_row: load!(bool, reader)?,
            state: load!(String, reader)?,
            since_span: load!(u64, reader)?,
            verification_ccu: load!(String, reader)?,
            economic_ccu_per_claim: load!(String, reader)?,
            artifact_bytes: load!(u64, reader)?,
            ops_supported: load!(bool, reader)?,
            verification_window_spans: load!(u32, reader)?,
            artifact_prefetch_spans: load!(u32, reader)?,
            max_inflight_claims: load!(u32, reader)?,
            required_ready_seats: load!(u32, reader)?,
            registration_bond_sompi: load!(u64, reader)?,
            admission_claims_per_span_milli: load!(u64, reader)?,
            probes_passed: load!(u32, reader)?,
            probes_failed: load!(u32, reader)?,
            ready_seats: load!(u32, reader)?,
            inflight_claims: load!(u32, reader)?,
            utilization_permille: load!(u32, reader)?,
            admission_milli: load!(u64, reader)?,
            cap_utilization_permille: load!(u32, reader)?,
            priced_share_permille: load!(u16, reader)?,
            ready_seats_now: load!(u32, reader)?,
            inflight_now: load!(u32, reader)?,
            share_permille: load!(u16, reader)?,
            no_capable_panel_voids: load!(u32, reader)?,
            reason: load!(String, reader)?,
            work_ratio_permille: load!(u32, reader)?,
            expected_forwards_q32: load!(String, reader)?,
            work_ticket_target: load!(String, reader)?,
            class_target: load!(String, reader)?,
            panel_room: load!(u64, reader)?,
            final_work_share_10_permille: load!(u16, reader)?,
            final_work_share_100_permille: load!(u16, reader)?,
            // Version 1 (a node before the Activation Pool's P4) has no recommendation: 0.
            recommended_pool_sompi: if version >= 2 { load!(u64, reader)? } else { 0 },
            // Versions 1–2 (a node before RFC-0017) are ungoverned: the old rule's rights.
            eligibility_stage: if version >= 3 { load!(String, reader)? } else { "not-governed".to_string() },
            eligibility_next_renewal_daa: if version >= 3 { load!(u64, reader)? } else { 0 },
            eligibility_subsidy: if version >= 3 { load!(bool, reader)? } else { true },
            eligibility_block_right: if version >= 3 { load!(bool, reader)? } else { true },
            eligibility_final_weight: if version >= 3 { load!(bool, reader)? } else { true },
            eligibility_exams_passed: if version >= 3 { load!(u32, reader)? } else { 0 },
            eligibility_lineage_attempts: if version >= 3 { load!(u32, reader)? } else { 0 },
        })
    }
}

/// A seat's last possession proof for a class, and whether it is still fresh at the tip.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwSeatReadiness {
    pub bond_txid: String,
    pub bond_index: u32,
    pub class_id: String,
    pub proved_daa: u64,
    pub proved_span: u64,
    pub leaf_index: u32,
    pub fresh: bool,
    /// Why the seat does not count as ready now (`stale`, `bond inactive`, `below floor`,
    /// `collateral short`); empty while it counts.
    pub not_ready_reason: String,
}

impl Serializer for RpcPalwSeatReadiness {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond_txid, writer)?;
        store!(u32, &self.bond_index, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(u64, &self.proved_daa, writer)?;
        store!(u64, &self.proved_span, writer)?;
        store!(u32, &self.leaf_index, writer)?;
        store!(bool, &self.fresh, writer)?;
        store!(String, &self.not_ready_reason, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwSeatReadiness {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            bond_txid: load!(String, reader)?,
            bond_index: load!(u32, reader)?,
            class_id: load!(String, reader)?,
            proved_daa: load!(u64, reader)?,
            proved_span: load!(u64, reader)?,
            leaf_index: load!(u32, reader)?,
            fresh: load!(bool, reader)?,
            not_ready_reason: load!(String, reader)?,
        })
    }
}

/// **What blocks a class, and at which stage** (RFC-0002 Part II §II.7.4): the registry's own reading of a class's row as a
/// structured fact instead of a sentence. `stage` is `convert` (the artifact's graph is the problem), `register` (the network has
/// not admitted the class) or `mine` (the class is registered but not yet admitted to claims, or not at full rate); `code` is a
/// stable token; `what` says what is missing, with the numbers; `have` / `need` are present (`has_count`) where the condition is a
/// count; `next` says the act that lifts it. A class with nothing blocking has no entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwClassBlocking {
    pub class_id: String,
    pub stage: String,
    pub code: String,
    pub what: String,
    pub has_count: bool,
    pub have: u64,
    pub need: u64,
    pub next: String,
}

impl Serializer for RpcPalwClassBlocking {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.stage, writer)?;
        store!(String, &self.code, writer)?;
        store!(String, &self.what, writer)?;
        store!(bool, &self.has_count, writer)?;
        store!(u64, &self.have, writer)?;
        store!(u64, &self.need, writer)?;
        store!(String, &self.next, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwClassBlocking {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            class_id: load!(String, reader)?,
            stage: load!(String, reader)?,
            code: load!(String, reader)?,
            what: load!(String, reader)?,
            has_count: load!(bool, reader)?,
            have: load!(u64, reader)?,
            need: load!(u64, reader)?,
            next: load!(String, reader)?,
        })
    }
}

/// **A class's seating, as `palw_class_seating` reads it now** (RFC-0002 Part II §II.7.5 Proposal A; wire version 3 of the registry
/// response): executor-less, so nothing is excluded. Served only past the fence and only for a class that has a row.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwClassSeating {
    pub class_id: String,
    /// Distinct operators holding the class with a fresh possession proof (condition 1, have) and the panel's size (need).
    pub ready_operators: u32,
    pub needed_operators: u32,
    /// Ready operators in the network's base population other than the registrant's (condition 2, have) and the fence's floor.
    pub independent_operators: u32,
    pub needed_independent: u32,
    /// Operators of the base population other than the registrant's: the licensable share's denominator.
    pub base_operators: u32,
    /// `independent_operators × 1000 / base_operators`: the share of the class's claims whose outsider holds it.
    pub licensable_share_permille: u16,
}

impl Serializer for RpcPalwClassSeating {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(u32, &self.ready_operators, writer)?;
        store!(u32, &self.needed_operators, writer)?;
        store!(u32, &self.independent_operators, writer)?;
        store!(u32, &self.needed_independent, writer)?;
        store!(u32, &self.base_operators, writer)?;
        store!(u16, &self.licensable_share_permille, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwClassSeating {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            class_id: load!(String, reader)?,
            ready_operators: load!(u32, reader)?,
            needed_operators: load!(u32, reader)?,
            independent_operators: load!(u32, reader)?,
            needed_independent: load!(u32, reader)?,
            base_operators: load!(u32, reader)?,
            licensable_share_permille: load!(u16, reader)?,
        })
    }
}

/// `fence_daa` is 0 where no registry is scheduled (`scheduled` false); `active` is whether the
/// fence is in force at the tip; the reference constants are the globals every class is derived
/// against.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelRegistryResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub scheduled: bool,
    pub fence_daa: u64,
    pub active: bool,
    /// The registry governs from here (the fence plus one readiness age).
    pub grace_until_daa: u64,
    pub span_daa: u64,
    pub reference_work_per_span: String,
    pub reference_bytes_per_span: u64,
    pub seat_count: u16,
    pub spare_seats: u16,
    pub utilization_permille: u32,
    pub probation_claims: u32,
    pub stable_epochs: u32,
    /// The readiness age a seat's possession proof is judged by NOW, in spans — readiness V2's eight
    /// past `Params::palw_readiness_v2`, the registry globals' thirty before it (a node before the
    /// 2026-09-24 readiness-age sweep served the globals' thirty whatever the fence).
    pub readiness_probe_max_age_spans: u32,
    pub readiness_collateral_multiple: u32,
    pub classes: Vec<RpcPalwModelLifecycle>,
    pub readiness: Vec<RpcPalwSeatReadiness>,
    /// Rowed classes by state.
    pub classes_active: u32,
    pub classes_active_limited: u32,
    pub classes_probation: u32,
    pub classes_prefetching: u32,
    pub classes_registered: u32,
    pub classes_held: u32,
    /// Bonds that could count as ready seats with a fresh proof (active, above the floor, with the
    /// collateral multiple free), and every active bond.
    pub bonds_active: u32,
    pub bonds_with_headroom: u32,
    /// ADR-0137 (shadow): the work target `W` (CCU a block), its floor `W₀`, the epoch it governs,
    /// the closed epoch's model and expected blocks, the rate `W₀` was priced with, the panel's
    /// in-flight replay, the budget's horizon and the finalized-work epochs kept; `false` where no
    /// shadow has folded yet.
    pub work_target_shadow: bool,
    pub work_target: String,
    pub work_floor: String,
    pub work_network_draws_q32: String,
    pub work_effective: String,
    pub work_epoch_index: u64,
    pub work_closed_model_blocks: u64,
    pub work_closed_expected_blocks: u64,
    pub work_rate_sompi_per_giga: u64,
    pub panel_inflight_replay: String,
    pub panel_horizon_spans: u64,
    pub final_work_epochs: u64,
    /// **Version 2** (RFC-0002 Part II §II.7.4): what blocks each class and at which stage, for every class that has something
    /// blocking it. Appended last: a reader of version 1 stops before it. A node of version 1 serves none; derive the same
    /// reading from `classes` with [`crate::model::palw_registry_blockings`].
    #[serde(default)]
    pub blocking: Vec<RpcPalwClassBlocking>,
    /// **Version 3** (RFC-0002 Part II §II.7.5 Proposal A): the seating of every class `palw_class_seating` reads, past the fence.
    /// Appended after `blocking`: a reader of version 2 stops before it. Empty on a node of version 2 and below the fence.
    #[serde(default)]
    pub seating: Vec<RpcPalwClassSeating>,
}

impl Serializer for GetPalwModelRegistryResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // Version 2 appends `blocking` (the class's stage and what is missing); version 3 appends `seating`.
        store!(u16, &3, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(bool, &self.scheduled, writer)?;
        store!(u64, &self.fence_daa, writer)?;
        store!(bool, &self.active, writer)?;
        store!(u64, &self.grace_until_daa, writer)?;
        store!(u64, &self.span_daa, writer)?;
        store!(String, &self.reference_work_per_span, writer)?;
        store!(u64, &self.reference_bytes_per_span, writer)?;
        store!(u16, &self.seat_count, writer)?;
        store!(u16, &self.spare_seats, writer)?;
        store!(u32, &self.utilization_permille, writer)?;
        store!(u32, &self.probation_claims, writer)?;
        store!(u32, &self.stable_epochs, writer)?;
        store!(u32, &self.readiness_probe_max_age_spans, writer)?;
        store!(u32, &self.readiness_collateral_multiple, writer)?;
        serialize!(Vec<RpcPalwModelLifecycle>, &self.classes, writer)?;
        serialize!(Vec<RpcPalwSeatReadiness>, &self.readiness, writer)?;
        store!(u32, &self.classes_active, writer)?;
        store!(u32, &self.classes_active_limited, writer)?;
        store!(u32, &self.classes_probation, writer)?;
        store!(u32, &self.classes_prefetching, writer)?;
        store!(u32, &self.classes_registered, writer)?;
        store!(u32, &self.classes_held, writer)?;
        store!(u32, &self.bonds_active, writer)?;
        store!(u32, &self.bonds_with_headroom, writer)?;
        store!(bool, &self.work_target_shadow, writer)?;
        store!(String, &self.work_target, writer)?;
        store!(String, &self.work_floor, writer)?;
        store!(String, &self.work_network_draws_q32, writer)?;
        store!(String, &self.work_effective, writer)?;
        store!(u64, &self.work_epoch_index, writer)?;
        store!(u64, &self.work_closed_model_blocks, writer)?;
        store!(u64, &self.work_closed_expected_blocks, writer)?;
        store!(u64, &self.work_rate_sompi_per_giga, writer)?;
        store!(String, &self.panel_inflight_replay, writer)?;
        store!(u64, &self.panel_horizon_spans, writer)?;
        store!(u64, &self.final_work_epochs, writer)?;
        serialize!(Vec<RpcPalwClassBlocking>, &self.blocking, writer)?;
        serialize!(Vec<RpcPalwClassSeating>, &self.seating, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelRegistryResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            scheduled: load!(bool, reader)?,
            fence_daa: load!(u64, reader)?,
            active: load!(bool, reader)?,
            grace_until_daa: load!(u64, reader)?,
            span_daa: load!(u64, reader)?,
            reference_work_per_span: load!(String, reader)?,
            reference_bytes_per_span: load!(u64, reader)?,
            seat_count: load!(u16, reader)?,
            spare_seats: load!(u16, reader)?,
            utilization_permille: load!(u32, reader)?,
            probation_claims: load!(u32, reader)?,
            stable_epochs: load!(u32, reader)?,
            readiness_probe_max_age_spans: load!(u32, reader)?,
            readiness_collateral_multiple: load!(u32, reader)?,
            classes: deserialize!(Vec<RpcPalwModelLifecycle>, reader)?,
            readiness: deserialize!(Vec<RpcPalwSeatReadiness>, reader)?,
            classes_active: load!(u32, reader)?,
            classes_active_limited: load!(u32, reader)?,
            classes_probation: load!(u32, reader)?,
            classes_prefetching: load!(u32, reader)?,
            classes_registered: load!(u32, reader)?,
            classes_held: load!(u32, reader)?,
            bonds_active: load!(u32, reader)?,
            bonds_with_headroom: load!(u32, reader)?,
            work_target_shadow: load!(bool, reader)?,
            work_target: load!(String, reader)?,
            work_floor: load!(String, reader)?,
            work_network_draws_q32: load!(String, reader)?,
            work_effective: load!(String, reader)?,
            work_epoch_index: load!(u64, reader)?,
            work_closed_model_blocks: load!(u64, reader)?,
            work_closed_expected_blocks: load!(u64, reader)?,
            work_rate_sompi_per_giga: load!(u64, reader)?,
            panel_inflight_replay: load!(String, reader)?,
            panel_horizon_spans: load!(u64, reader)?,
            final_work_epochs: load!(u64, reader)?,
            // Version 1 (a node before RFC-0002 Part II §II.7.4) serves no blocking readings.
            blocking: if version >= 2 { deserialize!(Vec<RpcPalwClassBlocking>, reader)? } else { Vec::new() },
            // Versions 1 and 2 serve no seating.
            seating: if version >= 3 { deserialize!(Vec<RpcPalwClassSeating>, reader)? } else { Vec::new() },
        })
    }
}

/// ADR-0148: price ONE free-prompt job before its commitment is written — the facts the fold prices
/// a `FreePromptCommitted` from, as the executor's result states them.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwFreePromptPriceRequest {
    /// 128-hex class id.
    pub class_id: String,
    /// The prompt as the commitment will carry it — the chain reads which prefix of it the class
    /// has already been paid for (ADR-0145 §6), so a price without the ids is not the chain's.
    pub prompt_token_ids: Vec<u32>,
    pub prompt_tokens: u32,
    pub decode_tokens_executed: u32,
    /// The capture's leaf count — the comparand the fold refuses a mismatch of by name.
    pub work_leaves: u64,
    /// `txid_hex:index` of the executor bond, for its room; empty for the price alone.
    pub bond: String,
}

impl Serializer for GetPalwFreePromptPriceRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(Vec<u32>, &self.prompt_token_ids, writer)?;
        store!(u32, &self.prompt_tokens, writer)?;
        store!(u32, &self.decode_tokens_executed, writer)?;
        store!(u64, &self.work_leaves, writer)?;
        store!(String, &self.bond, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwFreePromptPriceRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let class_id = load!(String, reader)?;
        let prompt_token_ids = load!(Vec<u32>, reader)?;
        let prompt_tokens = load!(u32, reader)?;
        let decode_tokens_executed = load!(u32, reader)?;
        let work_leaves = load!(u64, reader)?;
        let bond = load!(String, reader)?;
        Ok(Self { class_id, prompt_token_ids, prompt_tokens, decode_tokens_executed, work_leaves, bond })
    }
}

/// ADR-0148: the fold's price for the job at the virtual's DAA, or its refusal by name.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwFreePromptPriceResponse {
    /// False off `ConsensusV2` and on a node with no PALW state; every other field is then empty.
    pub available: bool,
    /// The DAA the job was priced at.
    pub daa_score: u64,
    /// True when the fold would price the commitment; false when it would refuse it (`refusal`).
    pub priced: bool,
    /// The fold's refusal, by name, when `priced` is false.
    pub refusal: String,
    /// Past the canonical-work fence the lane prices compute (ADR-0148), below it leaves.
    pub priced_in_compute: bool,
    pub quanta: u32,
    pub pwu: u64,
    /// Decimal `u128`: what the claim reserves against its bond.
    pub reserved_sompi: String,
    /// Decimal `u128`: the bond's room (`ceiling − backed`); empty without a bond, or for one the
    /// chain does not hold.
    pub bond_room_sompi: String,
}

impl Serializer for GetPalwFreePromptPriceResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.daa_score, writer)?;
        store!(bool, &self.priced, writer)?;
        store!(String, &self.refusal, writer)?;
        store!(bool, &self.priced_in_compute, writer)?;
        store!(u32, &self.quanta, writer)?;
        store!(u64, &self.pwu, writer)?;
        store!(String, &self.reserved_sompi, writer)?;
        store!(String, &self.bond_room_sompi, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwFreePromptPriceResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let daa_score = load!(u64, reader)?;
        let priced = load!(bool, reader)?;
        let refusal = load!(String, reader)?;
        let priced_in_compute = load!(bool, reader)?;
        let quanta = load!(u32, reader)?;
        let pwu = load!(u64, reader)?;
        let reserved_sompi = load!(String, reader)?;
        let bond_room_sompi = load!(String, reader)?;
        Ok(Self { available, daa_score, priced, refusal, priced_in_compute, quanta, pwu, reserved_sompi, bond_room_sompi })
    }
}

/// Why a seat or this node is not ready. `code` is the machine token; `message` is for operators.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwPanelHoldReason {
    pub code: String,
    pub message: String,
}

impl Serializer for RpcPalwPanelHoldReason {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.code, writer)?;
        store!(String, &self.message, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwPanelHoldReason {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { code: load!(String, reader)?, message: load!(String, reader)? })
    }
}

/// Chain facts for one class's panel: seat counts stay distinct.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwClassPanelStatus {
    pub class_id: String,
    pub model_name: String,
    pub registry_state: String,
    pub bonded_seats: u32,
    pub ready_seats: u32,
    pub required_ready_seats: u32,
    pub selected_panel_seats: u32,
    pub valid_receipt_seats: u32,
    pub panel_size: u16,
    pub receipt_quorum: u16,
    pub full_seats_per_panel: u16,
    pub partial_seats_per_panel: u16,
    pub segment_count: u16,
    pub inflight_claims: u32,
    pub active_assignments: u32,
    pub admission_permille: u64,
    pub verification_mode: String,
    pub s1_active: bool,
    pub s1_scheduled_daa: u64,
    pub s3_active: bool,
    pub s3_scheduled_daa: u64,
    pub s2_active: bool,
    pub s2_scheduled_daa: u64,
    /// Local RPC only: how many of this node's seats hold the class. 0 on a chain-only read.
    pub holds_local: u32,
    pub missing: Vec<RpcPalwPanelHoldReasonCount>,
}

impl Serializer for RpcPalwClassPanelStatus {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.model_name, writer)?;
        store!(String, &self.registry_state, writer)?;
        store!(u32, &self.bonded_seats, writer)?;
        store!(u32, &self.ready_seats, writer)?;
        store!(u32, &self.required_ready_seats, writer)?;
        store!(u32, &self.selected_panel_seats, writer)?;
        store!(u32, &self.valid_receipt_seats, writer)?;
        store!(u16, &self.panel_size, writer)?;
        store!(u16, &self.receipt_quorum, writer)?;
        store!(u16, &self.full_seats_per_panel, writer)?;
        store!(u16, &self.partial_seats_per_panel, writer)?;
        store!(u16, &self.segment_count, writer)?;
        store!(u32, &self.inflight_claims, writer)?;
        store!(u32, &self.active_assignments, writer)?;
        store!(u64, &self.admission_permille, writer)?;
        store!(String, &self.verification_mode, writer)?;
        store!(bool, &self.s1_active, writer)?;
        store!(u64, &self.s1_scheduled_daa, writer)?;
        store!(bool, &self.s3_active, writer)?;
        store!(u64, &self.s3_scheduled_daa, writer)?;
        store!(bool, &self.s2_active, writer)?;
        store!(u64, &self.s2_scheduled_daa, writer)?;
        store!(u32, &self.holds_local, writer)?;
        serialize!(Vec<RpcPalwPanelHoldReasonCount>, &self.missing, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwClassPanelStatus {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            class_id: load!(String, reader)?,
            model_name: load!(String, reader)?,
            registry_state: load!(String, reader)?,
            bonded_seats: load!(u32, reader)?,
            ready_seats: load!(u32, reader)?,
            required_ready_seats: load!(u32, reader)?,
            selected_panel_seats: load!(u32, reader)?,
            valid_receipt_seats: load!(u32, reader)?,
            panel_size: load!(u16, reader)?,
            receipt_quorum: load!(u16, reader)?,
            full_seats_per_panel: load!(u16, reader)?,
            partial_seats_per_panel: load!(u16, reader)?,
            segment_count: load!(u16, reader)?,
            inflight_claims: load!(u32, reader)?,
            active_assignments: load!(u32, reader)?,
            admission_permille: load!(u64, reader)?,
            verification_mode: load!(String, reader)?,
            s1_active: load!(bool, reader)?,
            s1_scheduled_daa: load!(u64, reader)?,
            s3_active: load!(bool, reader)?,
            s3_scheduled_daa: load!(u64, reader)?,
            s2_active: load!(bool, reader)?,
            s2_scheduled_daa: load!(u64, reader)?,
            holds_local: load!(u32, reader)?,
            missing: deserialize!(Vec<RpcPalwPanelHoldReasonCount>, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwPanelHoldReasonCount {
    pub code: String,
    pub message: String,
    pub seats: u32,
}

impl Serializer for RpcPalwPanelHoldReasonCount {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.code, writer)?;
        store!(String, &self.message, writer)?;
        store!(u32, &self.seats, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwPanelHoldReasonCount {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { code: load!(String, reader)?, message: load!(String, reader)?, seats: load!(u32, reader)? })
    }
}

/// A bonded verifier seat as the chain recognises it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwPanelSeat {
    pub seat_id: String,
    pub bond_outpoint: String,
    pub class_id: String,
    pub ready: bool,
    pub eligible: bool,
    pub readiness_version: u8,
    pub readiness_proved_daa: u64,
    pub readiness_expires_daa: u64,
    pub collateral_available: String,
    pub collateral_locked: String,
    pub assigned: u32,
    pub hold: Option<RpcPalwPanelHoldReason>,
}

impl Serializer for RpcPalwPanelSeat {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.seat_id, writer)?;
        store!(String, &self.bond_outpoint, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(bool, &self.ready, writer)?;
        store!(bool, &self.eligible, writer)?;
        store!(u8, &self.readiness_version, writer)?;
        store!(u64, &self.readiness_proved_daa, writer)?;
        store!(u64, &self.readiness_expires_daa, writer)?;
        store!(String, &self.collateral_available, writer)?;
        store!(String, &self.collateral_locked, writer)?;
        store!(u32, &self.assigned, writer)?;
        serialize!(Option<RpcPalwPanelHoldReason>, &self.hold, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwPanelSeat {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            seat_id: load!(String, reader)?,
            bond_outpoint: load!(String, reader)?,
            class_id: load!(String, reader)?,
            ready: load!(bool, reader)?,
            eligible: load!(bool, reader)?,
            readiness_version: load!(u8, reader)?,
            readiness_proved_daa: load!(u64, reader)?,
            readiness_expires_daa: load!(u64, reader)?,
            collateral_available: load!(String, reader)?,
            collateral_locked: load!(String, reader)?,
            assigned: load!(u32, reader)?,
            hold: deserialize!(Option<RpcPalwPanelHoldReason>, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwPanelAssignmentSeat {
    pub seat_id: String,
    pub seat_index: u16,
    pub full_seat: bool,
    pub segment_index: Option<u16>,
    pub mask: u32,
    pub receipt_status: String,
    pub credited_daa: u64,
}

impl Serializer for RpcPalwPanelAssignmentSeat {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.seat_id, writer)?;
        store!(u16, &self.seat_index, writer)?;
        store!(bool, &self.full_seat, writer)?;
        store!(Option<u16>, &self.segment_index, writer)?;
        store!(u32, &self.mask, writer)?;
        store!(String, &self.receipt_status, writer)?;
        store!(u64, &self.credited_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwPanelAssignmentSeat {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            seat_id: load!(String, reader)?,
            seat_index: load!(u16, reader)?,
            full_seat: load!(bool, reader)?,
            segment_index: load!(Option<u16>, reader)?,
            mask: load!(u32, reader)?,
            receipt_status: load!(String, reader)?,
            credited_daa: load!(u64, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwPanelAssignment {
    pub claim_id: String,
    pub class_id: String,
    pub licensed_state: String,
    pub deadline_daa: u64,
    pub coverage_mask: u32,
    pub full_seat: String,
    pub valid_receipt_seats: u32,
    pub selected_panel_seats: u32,
    pub seats: Vec<RpcPalwPanelAssignmentSeat>,
}

impl Serializer for RpcPalwPanelAssignment {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.licensed_state, writer)?;
        store!(u64, &self.deadline_daa, writer)?;
        store!(u32, &self.coverage_mask, writer)?;
        store!(String, &self.full_seat, writer)?;
        store!(u32, &self.valid_receipt_seats, writer)?;
        store!(u32, &self.selected_panel_seats, writer)?;
        serialize!(Vec<RpcPalwPanelAssignmentSeat>, &self.seats, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwPanelAssignment {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            claim_id: load!(String, reader)?,
            class_id: load!(String, reader)?,
            licensed_state: load!(String, reader)?,
            deadline_daa: load!(u64, reader)?,
            coverage_mask: load!(u32, reader)?,
            full_seat: load!(String, reader)?,
            valid_receipt_seats: load!(u32, reader)?,
            selected_panel_seats: load!(u32, reader)?,
            seats: deserialize!(Vec<RpcPalwPanelAssignmentSeat>, reader)?,
        })
    }
}

/// This node's local panel state for one class — artifact, working set, bond, proof. Not chain fact.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwLocalPanelClass {
    pub class_id: String,
    pub model_name: String,
    pub seat_id: String,
    pub artifact_loaded: bool,
    pub artifact_root: String,
    pub working_set_bytes: u64,
    pub replay_capable: bool,
    pub synced: bool,
    pub bond_active: bool,
    pub collateral_sompi: u64,
    pub readiness_proof_accepted: bool,
    pub readiness_proved_daa: u64,
    pub chain_state: String,
    pub assignments: u32,
    pub hold: Option<RpcPalwPanelHoldReason>,
    /// **The resource profile by role** (ADR-0151 follow-up, item 6; version 2 of this message):
    /// the representation this node holds the class's cache in, the artifact's resident bytes
    /// (file plus derived tables), each role's need on this node, and whether the reservation
    /// ledger could cover it now. Node-local capacity, never a chain fact.
    pub runtime_profile: String,
    pub artifact_resident_bytes: u64,
    pub producer_working_set_bytes: u64,
    pub full_seat_working_set_bytes: u64,
    pub partial_seat_working_set_bytes: u64,
    pub producer_capable: bool,
    pub full_seat_capable: bool,
    pub partial_seat_capable: bool,
}

impl Serializer for RpcPalwLocalPanelClass {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.model_name, writer)?;
        store!(String, &self.seat_id, writer)?;
        store!(bool, &self.artifact_loaded, writer)?;
        store!(String, &self.artifact_root, writer)?;
        store!(u64, &self.working_set_bytes, writer)?;
        store!(bool, &self.replay_capable, writer)?;
        store!(bool, &self.synced, writer)?;
        store!(bool, &self.bond_active, writer)?;
        store!(u64, &self.collateral_sompi, writer)?;
        store!(bool, &self.readiness_proof_accepted, writer)?;
        store!(u64, &self.readiness_proved_daa, writer)?;
        store!(String, &self.chain_state, writer)?;
        store!(u32, &self.assignments, writer)?;
        serialize!(Option<RpcPalwPanelHoldReason>, &self.hold, writer)?;
        // Version 2: the resource profile by role.
        store!(String, &self.runtime_profile, writer)?;
        store!(u64, &self.artifact_resident_bytes, writer)?;
        store!(u64, &self.producer_working_set_bytes, writer)?;
        store!(u64, &self.full_seat_working_set_bytes, writer)?;
        store!(u64, &self.partial_seat_working_set_bytes, writer)?;
        store!(bool, &self.producer_capable, writer)?;
        store!(bool, &self.full_seat_capable, writer)?;
        store!(bool, &self.partial_seat_capable, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwLocalPanelClass {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let mut out = Self {
            class_id: load!(String, reader)?,
            model_name: load!(String, reader)?,
            seat_id: load!(String, reader)?,
            artifact_loaded: load!(bool, reader)?,
            artifact_root: load!(String, reader)?,
            working_set_bytes: load!(u64, reader)?,
            replay_capable: load!(bool, reader)?,
            synced: load!(bool, reader)?,
            bond_active: load!(bool, reader)?,
            collateral_sompi: load!(u64, reader)?,
            readiness_proof_accepted: load!(bool, reader)?,
            readiness_proved_daa: load!(u64, reader)?,
            chain_state: load!(String, reader)?,
            assignments: load!(u32, reader)?,
            hold: deserialize!(Option<RpcPalwPanelHoldReason>, reader)?,
            ..Default::default()
        };
        // A version-1 sender (a node before the resource profile) stops here; its rows read with
        // the profile fields at their defaults rather than as a truncated message.
        if version >= 2 {
            out.runtime_profile = load!(String, reader)?;
            out.artifact_resident_bytes = load!(u64, reader)?;
            out.producer_working_set_bytes = load!(u64, reader)?;
            out.full_seat_working_set_bytes = load!(u64, reader)?;
            out.partial_seat_working_set_bytes = load!(u64, reader)?;
            out.producer_capable = load!(bool, reader)?;
            out.full_seat_capable = load!(bool, reader)?;
            out.partial_seat_capable = load!(bool, reader)?;
        }
        Ok(out)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwClassPanelStatusRequest {
    pub class_id: String,
}

impl Serializer for GetPalwClassPanelStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwClassPanelStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { class_id: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwClassPanelStatusResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub found: bool,
    pub status: RpcPalwClassPanelStatus,
}

impl Serializer for GetPalwClassPanelStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(bool, &self.found, writer)?;
        serialize!(RpcPalwClassPanelStatus, &self.status, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwClassPanelStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            found: load!(bool, reader)?,
            status: deserialize!(RpcPalwClassPanelStatus, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPanelSeatsRequest {
    /// Empty: every bonded verifier seat. 128-hex or `QWEN36`: that class only.
    pub class_id: String,
}

impl Serializer for GetPalwPanelSeatsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPanelSeatsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { class_id: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPanelSeatsResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub seats: Vec<RpcPalwPanelSeat>,
}

impl Serializer for GetPalwPanelSeatsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        serialize!(Vec<RpcPalwPanelSeat>, &self.seats, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPanelSeatsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { available: load!(bool, reader)?, tip_daa: load!(u64, reader)?, seats: deserialize!(Vec<RpcPalwPanelSeat>, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPanelStatusRequest {
    pub class_id: String,
}

impl Serializer for GetPalwPanelStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPanelStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { class_id: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPanelStatusResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub panel_running: bool,
    pub panel_submitter: bool,
    pub synced: bool,
    pub classes: Vec<RpcPalwLocalPanelClass>,
}

impl Serializer for GetPalwPanelStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(bool, &self.panel_running, writer)?;
        store!(bool, &self.panel_submitter, writer)?;
        store!(bool, &self.synced, writer)?;
        serialize!(Vec<RpcPalwLocalPanelClass>, &self.classes, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPanelStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            panel_running: load!(bool, reader)?,
            panel_submitter: load!(bool, reader)?,
            synced: load!(bool, reader)?,
            classes: deserialize!(Vec<RpcPalwLocalPanelClass>, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPanelAssignmentsRequest {
    pub claim_id: String,
    pub seat_id: String,
}

impl Serializer for GetPalwPanelAssignmentsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.seat_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPanelAssignmentsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { claim_id: load!(String, reader)?, seat_id: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPanelAssignmentsResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub truncated: bool,
    pub assignments: Vec<RpcPalwPanelAssignment>,
}

impl Serializer for GetPalwPanelAssignmentsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(bool, &self.truncated, writer)?;
        serialize!(Vec<RpcPalwPanelAssignment>, &self.assignments, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPanelAssignmentsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            truncated: load!(bool, reader)?,
            assignments: deserialize!(Vec<RpcPalwPanelAssignment>, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelPreflightCheck {
    pub code: String,
    pub ok: bool,
    pub message: String,
}

impl Serializer for RpcPalwModelPreflightCheck {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.code, writer)?;
        store!(bool, &self.ok, writer)?;
        store!(String, &self.message, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelPreflightCheck {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { code: load!(String, reader)?, ok: load!(bool, reader)?, message: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelRegistration {
    pub object_id: String,
    pub class_id: String,
    pub constructed: bool,
    pub submitted: bool,
    pub accepted: bool,
    pub included: bool,
    pub folded: bool,
    pub submission_state: String,
    pub processor_verdict: String,
    pub reject_code: String,
    pub mempool_accepted: bool,
    pub included_block: String,
    pub included_daa: u64,
    pub registry_state: String,
    pub transaction_id: String,
    /// **Version 2 (testnet-12 lifecycle audit T12-030): why a MINED carrier wrote no class row** —
    /// set with `reject_code` `REGISTRATION_DROPPED`: the processor's refusal as the node's preflight
    /// re-derives it at the DAA the carrier was accepted at (e.g. `CLASS_NOT_ATTRIBUTABLE: …`), or a
    /// pointer to the node's log where the object could not be re-read. Empty otherwise.
    #[serde(default)]
    pub drop_reason: String,
}

impl Serializer for RpcPalwModelRegistration {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(String, &self.object_id, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(bool, &self.constructed, writer)?;
        store!(bool, &self.submitted, writer)?;
        store!(bool, &self.accepted, writer)?;
        store!(bool, &self.included, writer)?;
        store!(bool, &self.folded, writer)?;
        store!(String, &self.submission_state, writer)?;
        store!(String, &self.processor_verdict, writer)?;
        store!(String, &self.reject_code, writer)?;
        store!(bool, &self.mempool_accepted, writer)?;
        store!(String, &self.included_block, writer)?;
        store!(u64, &self.included_daa, writer)?;
        store!(String, &self.registry_state, writer)?;
        store!(String, &self.transaction_id, writer)?;
        // Version 2: the tail sits inside this record's own payload frame, so a version-1 reader
        // leaves it there.
        store!(String, &self.drop_reason, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelRegistration {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        Ok(Self {
            object_id: load!(String, reader)?,
            class_id: load!(String, reader)?,
            constructed: load!(bool, reader)?,
            submitted: load!(bool, reader)?,
            accepted: load!(bool, reader)?,
            included: load!(bool, reader)?,
            folded: load!(bool, reader)?,
            submission_state: load!(String, reader)?,
            processor_verdict: load!(String, reader)?,
            reject_code: load!(String, reader)?,
            mempool_accepted: load!(bool, reader)?,
            included_block: load!(String, reader)?,
            included_daa: load!(u64, reader)?,
            registry_state: load!(String, reader)?,
            transaction_id: load!(String, reader)?,
            drop_reason: if version >= 2 { load!(String, reader)? } else { String::new() },
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelPreflightRequest {
    pub object_hex: String,
    pub class_id: String,
}

impl Serializer for GetPalwModelPreflightRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.object_hex, writer)?;
        store!(String, &self.class_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelPreflightRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { object_hex: load!(String, reader)?, class_id: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelPreflightResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub class_id: String,
    pub artifact_root: String,
    pub n_ctx: u32,
    pub layer_count: u32,
    pub admissible: bool,
    pub processor_verdict: String,
    pub reject_code: String,
    pub checks: Vec<RpcPalwModelPreflightCheck>,
}

impl Serializer for GetPalwModelPreflightResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.artifact_root, writer)?;
        store!(u32, &self.n_ctx, writer)?;
        store!(u32, &self.layer_count, writer)?;
        store!(bool, &self.admissible, writer)?;
        store!(String, &self.processor_verdict, writer)?;
        store!(String, &self.reject_code, writer)?;
        serialize!(Vec<RpcPalwModelPreflightCheck>, &self.checks, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelPreflightResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            class_id: load!(String, reader)?,
            artifact_root: load!(String, reader)?,
            n_ctx: load!(u32, reader)?,
            layer_count: load!(u32, reader)?,
            admissible: load!(bool, reader)?,
            processor_verdict: load!(String, reader)?,
            reject_code: load!(String, reader)?,
            checks: deserialize!(Vec<RpcPalwModelPreflightCheck>, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitPalwModelRegistrationRequest {
    pub object_hex: String,
    pub transaction_id: String,
}

impl Serializer for SubmitPalwModelRegistrationRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.object_hex, writer)?;
        store!(String, &self.transaction_id, writer)?;
        Ok(())
    }
}

impl Deserializer for SubmitPalwModelRegistrationRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { object_hex: load!(String, reader)?, transaction_id: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitPalwModelRegistrationResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub registration: RpcPalwModelRegistration,
    pub checks: Vec<RpcPalwModelPreflightCheck>,
}

impl Serializer for SubmitPalwModelRegistrationResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        serialize!(RpcPalwModelRegistration, &self.registration, writer)?;
        serialize!(Vec<RpcPalwModelPreflightCheck>, &self.checks, writer)?;
        Ok(())
    }
}

impl Deserializer for SubmitPalwModelRegistrationResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            registration: deserialize!(RpcPalwModelRegistration, reader)?,
            checks: deserialize!(Vec<RpcPalwModelPreflightCheck>, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelRegistrationStatusRequest {
    pub class_id: String,
    pub object_id: String,
    pub transaction_id: String,
}

impl Serializer for GetPalwModelRegistrationStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.object_id, writer)?;
        store!(String, &self.transaction_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelRegistrationStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { class_id: load!(String, reader)?, object_id: load!(String, reader)?, transaction_id: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelRegistrationStatusResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub found: bool,
    pub registration: RpcPalwModelRegistration,
}

impl Serializer for GetPalwModelRegistrationStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(bool, &self.found, writer)?;
        serialize!(RpcPalwModelRegistration, &self.registration, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelRegistrationStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            found: load!(bool, reader)?,
            registration: deserialize!(RpcPalwModelRegistration, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelRequest {
    pub class_id: String,
}

impl Serializer for GetPalwModelRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { class_id: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub found: bool,
    pub class_id: String,
    pub model_name: String,
    pub n_ctx: u32,
    pub artifact_root: String,
    pub class_status: String,
    pub registry_state: String,
    pub ready_seats: u32,
    pub required_ready_seats: u32,
    pub inflight_claims: u32,
    pub admission_permille: u32,
    pub share_permille: u16,
    pub certified_family: String,
    pub fence_active: bool,
    pub reason: String,
}

impl Serializer for GetPalwModelResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.model_name, writer)?;
        store!(u32, &self.n_ctx, writer)?;
        store!(String, &self.artifact_root, writer)?;
        store!(String, &self.class_status, writer)?;
        store!(String, &self.registry_state, writer)?;
        store!(u32, &self.ready_seats, writer)?;
        store!(u32, &self.required_ready_seats, writer)?;
        store!(u32, &self.inflight_claims, writer)?;
        store!(u32, &self.admission_permille, writer)?;
        store!(u16, &self.share_permille, writer)?;
        store!(String, &self.certified_family, writer)?;
        store!(bool, &self.fence_active, writer)?;
        store!(String, &self.reason, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            found: load!(bool, reader)?,
            class_id: load!(String, reader)?,
            model_name: load!(String, reader)?,
            n_ctx: load!(u32, reader)?,
            artifact_root: load!(String, reader)?,
            class_status: load!(String, reader)?,
            registry_state: load!(String, reader)?,
            ready_seats: load!(u32, reader)?,
            required_ready_seats: load!(u32, reader)?,
            inflight_claims: load!(u32, reader)?,
            admission_permille: load!(u32, reader)?,
            share_permille: load!(u16, reader)?,
            certified_family: load!(String, reader)?,
            fence_active: load!(bool, reader)?,
            reason: load!(String, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelReadinessRequest {
    pub class_id: String,
}

impl Serializer for GetPalwModelReadinessRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelReadinessRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { class_id: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelSeatReadiness {
    pub seat_id: String,
    pub bond_txid: String,
    pub bond_index: u32,
    pub proved_daa: u64,
    pub proved_span: u64,
    pub expires_daa: u64,
    pub fresh: bool,
    pub ready: bool,
    pub collateral_sompi: u64,
    pub needed_collateral_sompi: u64,
    pub not_ready_reason: String,
}

impl Serializer for RpcPalwModelSeatReadiness {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.seat_id, writer)?;
        store!(String, &self.bond_txid, writer)?;
        store!(u32, &self.bond_index, writer)?;
        store!(u64, &self.proved_daa, writer)?;
        store!(u64, &self.proved_span, writer)?;
        store!(u64, &self.expires_daa, writer)?;
        store!(bool, &self.fresh, writer)?;
        store!(bool, &self.ready, writer)?;
        store!(u64, &self.collateral_sompi, writer)?;
        store!(u64, &self.needed_collateral_sompi, writer)?;
        store!(String, &self.not_ready_reason, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelSeatReadiness {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            seat_id: load!(String, reader)?,
            bond_txid: load!(String, reader)?,
            bond_index: load!(u32, reader)?,
            proved_daa: load!(u64, reader)?,
            proved_span: load!(u64, reader)?,
            expires_daa: load!(u64, reader)?,
            fresh: load!(bool, reader)?,
            ready: load!(bool, reader)?,
            collateral_sompi: load!(u64, reader)?,
            needed_collateral_sompi: load!(u64, reader)?,
            not_ready_reason: load!(String, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelReadinessResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub found: bool,
    pub class_id: String,
    pub registry_state: String,
    pub ready_seats: u32,
    pub required_ready_seats: u32,
    pub seats: Vec<RpcPalwModelSeatReadiness>,
}

impl Serializer for GetPalwModelReadinessResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.registry_state, writer)?;
        store!(u32, &self.ready_seats, writer)?;
        store!(u32, &self.required_ready_seats, writer)?;
        serialize!(Vec<RpcPalwModelSeatReadiness>, &self.seats, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelReadinessResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            found: load!(bool, reader)?,
            class_id: load!(String, reader)?,
            registry_state: load!(String, reader)?,
            ready_seats: load!(u32, reader)?,
            required_ready_seats: load!(u32, reader)?,
            seats: deserialize!(Vec<RpcPalwModelSeatReadiness>, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelAdmissionRequest {
    pub class_id: String,
    pub object_hex: String,
}

impl Serializer for GetPalwModelAdmissionRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.object_hex, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelAdmissionRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { class_id: load!(String, reader)?, object_hex: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelAdmissionResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub class_id: String,
    pub admissible: bool,
    pub processor_verdict: String,
    pub reject_code: String,
    pub checks: Vec<RpcPalwModelPreflightCheck>,
}

impl Serializer for GetPalwModelAdmissionResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(bool, &self.admissible, writer)?;
        store!(String, &self.processor_verdict, writer)?;
        store!(String, &self.reject_code, writer)?;
        serialize!(Vec<RpcPalwModelPreflightCheck>, &self.checks, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelAdmissionResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            class_id: load!(String, reader)?,
            admissible: load!(bool, reader)?,
            processor_verdict: load!(String, reader)?,
            reject_code: load!(String, reader)?,
            checks: deserialize!(Vec<RpcPalwModelPreflightCheck>, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelCertificationRequest {
    pub class_id: String,
}

impl Serializer for GetPalwModelCertificationRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelCertificationRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { class_id: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelCertifiedFamily {
    pub lane: String,
    pub digest: String,
    pub covers: bool,
}

impl Serializer for RpcPalwModelCertifiedFamily {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.lane, writer)?;
        store!(String, &self.digest, writer)?;
        store!(bool, &self.covers, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelCertifiedFamily {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { lane: load!(String, reader)?, digest: load!(String, reader)?, covers: load!(bool, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelCertificationResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub found: bool,
    pub class_id: String,
    pub end_to_end_certified: bool,
    pub families: Vec<RpcPalwModelCertifiedFamily>,
}

impl Serializer for GetPalwModelCertificationResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(bool, &self.end_to_end_certified, writer)?;
        serialize!(Vec<RpcPalwModelCertifiedFamily>, &self.families, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelCertificationResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            found: load!(bool, reader)?,
            class_id: load!(String, reader)?,
            end_to_end_certified: load!(bool, reader)?,
            families: deserialize!(Vec<RpcPalwModelCertifiedFamily>, reader)?,
        })
    }
}

/// G14 op 244: one original FP commitment and its public execution binding header, both as the
/// lowercase hex of their borsh encodings (the exact bytes the HFP1 envelope will carry).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwStagedPreclaimQuoteRequest {
    pub commitment_hex: String,
    pub binding_hex: String,
}

impl Serializer for GetPalwStagedPreclaimQuoteRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.commitment_hex, writer)?;
        store!(String, &self.binding_hex, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwStagedPreclaimQuoteRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown preclaim quote request version"));
        }
        let commitment_hex = load!(String, reader)?;
        let binding_hex = load!(String, reader)?;
        Ok(Self { commitment_hex, binding_hex })
    }
}

/// The native quote at the node's accepted snapshot. `quoted` false with an empty `refusal` means
/// the claim needs no required family (a plain carrier); a refusal names why admission would refuse.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwStagedPreclaimQuoteResponse {
    /// False off ConsensusV2, below the staged fence, or with no accepted PALW snapshot.
    pub available: bool,
    pub quoted: bool,
    pub refusal: String,
    pub snapshot_block: String,
    pub resource_plan_id: String,
    pub maximum_records: u64,
    pub maximum_custodians: u64,
    /// The quote's own admission horizon at the snapshot (the pledge's `accept_until_daa`).
    pub accept_until_daa: u64,
    /// Clock-free: `material_until_daa = accepted_daa + material_span_daa`.
    pub material_span_daa: u64,
}

impl Serializer for GetPalwStagedPreclaimQuoteResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.quoted, writer)?;
        store!(String, &self.refusal, writer)?;
        store!(String, &self.snapshot_block, writer)?;
        store!(String, &self.resource_plan_id, writer)?;
        store!(u64, &self.maximum_records, writer)?;
        store!(u64, &self.maximum_custodians, writer)?;
        store!(u64, &self.accept_until_daa, writer)?;
        store!(u64, &self.material_span_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwStagedPreclaimQuoteResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown preclaim quote response version"));
        }
        let available = load!(bool, reader)?;
        let quoted = load!(bool, reader)?;
        let refusal = load!(String, reader)?;
        let snapshot_block = load!(String, reader)?;
        let resource_plan_id = load!(String, reader)?;
        let maximum_records = load!(u64, reader)?;
        let maximum_custodians = load!(u64, reader)?;
        let accept_until_daa = load!(u64, reader)?;
        let material_span_daa = load!(u64, reader)?;
        Ok(Self {
            available,
            quoted,
            refusal,
            snapshot_block,
            resource_plan_id,
            maximum_records,
            maximum_custodians,
            accept_until_daa,
            material_span_daa,
        })
    }
}
