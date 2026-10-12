//! **ADR-0080 design A — a declared court close, mid-assembly** — moved out of `message.rs` (2026-10-13, a behaviour-preserving split: every item is the same and keeps its path through `pub use palw_court_close::*`).

use super::*;

// -----------------------------------------------------------------------------------------------
// ADR-0080 design A — a declared court close, mid-assembly
// -----------------------------------------------------------------------------------------------
//
// A close too wide for one carrier rides as a signed `CourtCloseDeclared` and its chunks, in a
// table keyed `(session_id, side)`. The mover is under a court deadline and its carriers can be
// orphaned, so it has to be able to ask what the CHAIN thinks it has received. Before this call the
// only answer was `misaka palw court-close`'s own journal on the mover's disk — and a journal that
// believes itself skips a part whose carrier was reorged out and completes a group that can never
// assemble. It also could not answer the two preflights that matter: whether a declaration for this
// `(session, side)` already exists (one per side, ever) and how much time is left to assemble.

/// ADR-0080 design A: which declared close to read — a session id and one of its two sides.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPendingChunkGroupRequest {
    /// 128-hex court session id.
    pub session_id: String,
    /// `challenger` or `executor`. Not defaulted: a group is keyed by the side, and answering for
    /// the wrong one is answering about the other party's carriage.
    pub side: String,
}

impl Serializer for GetPalwPendingChunkGroupRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.session_id, writer)?;
        store!(String, &self.side, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPendingChunkGroupRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let session_id = load!(String, reader)?;
        let side = load!(String, reader)?;
        Ok(Self { session_id, side })
    }
}

/// **One side's declared close, as the chain holds it** (ADR-0080 design A §2.2).
///
/// `present` is the row's own `u64` bitmap and is carried as a bitmap rather than as a count,
/// because chunks arrive in ANY order: "four of seven have landed" does not say which three to
/// send, and a resume that guessed would re-pay for a carrier the chain already has and leave a
/// hole it does not.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPendingChunkGroupResponse {
    /// False off `ConsensusV2` and for a `(session, side)` that has declared no close. It is also
    /// false once the group is GONE — adjudicated, convicted or swept with its session — so a
    /// filer that saw a group and now does not has an answer rather than a silence.
    pub found: bool,
    pub session_id: String,
    pub side: String,
    /// The chunks the declaration pinned. Every consensus rule on a split close counts these.
    pub count: u32,
    /// One bit per index, exactly as `PalwCourtCloseGroupV2::present` holds it.
    pub present: u64,
    pub parts_present: u32,
    pub complete: bool,
    pub declared_daa: u64,
    /// `declared_daa + PALW_COURT_CLOSE_INCLUSION_MARGIN × count`, and never past the session's
    /// own backstop — the declaration may not extend it.
    pub assembly_deadline_daa: u64,
    /// 128-hex keyed hash of the concatenation. A filer compares its own cut against this before
    /// spending a carrier on a group it would convict itself by completing.
    pub close_digest: String,
    /// `executor_guilty` / `challenger_defeated`.
    pub verdict: String,
    /// `txid:index` — the bond that declared, which is also the outpoint backing the deposit.
    pub declarer_bond: String,
    /// The assembly reserve at risk, forfeited if the group never assembles.
    pub deposit: u64,
}

impl Serializer for GetPalwPendingChunkGroupResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.session_id, writer)?;
        store!(String, &self.side, writer)?;
        store!(u32, &self.count, writer)?;
        store!(u64, &self.present, writer)?;
        store!(u32, &self.parts_present, writer)?;
        store!(bool, &self.complete, writer)?;
        store!(u64, &self.declared_daa, writer)?;
        store!(u64, &self.assembly_deadline_daa, writer)?;
        store!(String, &self.close_digest, writer)?;
        store!(String, &self.verdict, writer)?;
        store!(String, &self.declarer_bond, writer)?;
        store!(u64, &self.deposit, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPendingChunkGroupResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let found = load!(bool, reader)?;
        let session_id = load!(String, reader)?;
        let side = load!(String, reader)?;
        let count = load!(u32, reader)?;
        let present = load!(u64, reader)?;
        let parts_present = load!(u32, reader)?;
        let complete = load!(bool, reader)?;
        let declared_daa = load!(u64, reader)?;
        let assembly_deadline_daa = load!(u64, reader)?;
        let close_digest = load!(String, reader)?;
        let verdict = load!(String, reader)?;
        let declarer_bond = load!(String, reader)?;
        let deposit = load!(u64, reader)?;
        Ok(Self {
            found,
            session_id,
            side,
            count,
            present,
            parts_present,
            complete,
            declared_daa,
            assembly_deadline_daa,
            close_digest,
            verdict,
            declarer_bond,
            deposit,
        })
    }
}

/// ADR-0087 Decision 8: the market of one line (ADR-0088 Decision 9: keyed by line; a class's
/// founding line has the class id as its line id).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelMarketRequest {
    /// 128-hex line id — a class id names the class's founding line.
    pub line_id: String,
}

impl Serializer for GetPalwModelMarketRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelMarketRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        Ok(Self { line_id })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelMarketResponse {
    /// False when the line (or its class) does not exist on this chain (or the chain is not
    /// ConsensusV2).
    pub found: bool,
    pub line_id: String,
    /// **True once the line is a market** — its seed reached the floor (ADR-0094 Decision 2) and
    /// the curve holds the supply. A line still collecting its floor has a row, a first payer in
    /// `seeded_by` and a total in `seed_pledged_sompi`, and reads false here, as does a line nothing
    /// was paid into (the numbers below are then the unopened market's). Until the 2026-09-23
    /// Position route matrix (P10) a node answered "a row exists", so a pledge alone read true.
    pub opened: bool,
    pub opened_daa: u64,
    pub msk_reserve: u64,
    pub position_units: u64,
    pub sold_units: u64,
    pub burned_sompi: u64,
    /// The owner's total of the 1 % leg (ADR-0088 Decision 8: the field keeps its name and now
    /// means the owner's).
    pub registrant_paid_sompi: u64,
    pub closed_to_buys: bool,
    /// `(reserve + V) / positions`, in sompi per position, rounded down.
    pub price_sompi_per_position: u64,
    pub supply_units: u64,
    pub virtual_sompi: u64,
    /// The class's status as the registry names it (`Active`, `Frozen {..}`, `Registered {..}`).
    pub class_status: String,
    /// ADR-0088 Decision 8: the part of the leg paid to an adopted contributor.
    pub contributor_paid_sompi: u64,
    /// ADR-0090: the seed the market opened with — locked for good (0 while unseeded).
    pub seed_sompi: u64,
    /// ADR-0090: who seeded it, as the seeder's holder id (128 hex; empty while unseeded) — kept
    /// for the record and never paid, so not a payout payload.
    pub seeded_by: String,
    /// ADR-0090: the least seed this network takes, in sompi.
    pub seed_min_sompi: u64,
    /// ADR-0094: what this line has collected toward the floor — locked in its sink from the
    /// moment each payment landed. Equals `seed_sompi` once the market opened.
    pub seed_pledged_sompi: u64,
    /// ADR-0091: MSK the mining reward has put into the curve, cumulative — five percent of every
    /// model block's escrowed worker reward, at the claim's `Final`.
    pub buyback_sompi: u64,
    /// ADR-0091: the positions the reward's buys retired — the chain's, for good; no holder has
    /// them, and `position_units + Σ holders + retired_units = supply_units`.
    pub retired_units: u64,
    /// ADR-0114: the schedule a move is quoted under at this node's virtual DAA — what the MSK leg
    /// burns and what it pays the line's owner, in permille. A version-5 peer served none; it quoted
    /// under ADR-0087 Decision 4's 50/10, which is what those peers' fold does.
    pub burn_permille: u64,
    pub leg_permille: u64,
    /// ADR-0114: the DAA the five-percent leg takes effect at on this network (0 = not scheduled).
    pub leg_v2_activation_daa: u64,
    /// **The 2026-09-23 Position route matrix, P-B3: the registry lifecycle of the line's class**
    /// (`Prefetching`, `Probation { .. }`, `Active`, `Held`, …); empty where the registry holds no
    /// row, and from a peer older than version 7.
    #[serde(default)]
    pub class_lifecycle: String,
    /// **P-B3: why the fold at the node's next block would refuse a seed or a buy of this line on
    /// its class's lifecycle; empty where it would take them.** A client asks this before it pays,
    /// because a refused carrier seed or buy loses its sink output (P-B1). A sell is never refused
    /// for it. A peer older than version 7 serves it empty — its fold asked no such question.
    #[serde(default)]
    pub market_refusal: String,
}

impl Serializer for GetPalwModelMarketResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &7, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(bool, &self.opened, writer)?;
        store!(u64, &self.opened_daa, writer)?;
        store!(u64, &self.msk_reserve, writer)?;
        store!(u64, &self.position_units, writer)?;
        store!(u64, &self.sold_units, writer)?;
        store!(u64, &self.burned_sompi, writer)?;
        store!(u64, &self.registrant_paid_sompi, writer)?;
        store!(bool, &self.closed_to_buys, writer)?;
        store!(u64, &self.price_sompi_per_position, writer)?;
        store!(u64, &self.supply_units, writer)?;
        store!(u64, &self.virtual_sompi, writer)?;
        store!(String, &self.class_status, writer)?;
        store!(u64, &self.contributor_paid_sompi, writer)?;
        // Version 3 (ADR-0090): the seed, its payer and the network's least seed.
        store!(u64, &self.seed_sompi, writer)?;
        store!(String, &self.seeded_by, writer)?;
        store!(u64, &self.seed_min_sompi, writer)?;
        // Version 4 (ADR-0091): what the mining reward bought, and what it retired.
        store!(u64, &self.buyback_sompi, writer)?;
        store!(u64, &self.retired_units, writer)?;
        // Version 5 (ADR-0094): the instalments collected toward the floor.
        store!(u64, &self.seed_pledged_sompi, writer)?;
        // Version 6 (ADR-0114): the fee schedule in force, and the height the owner's leg moves at.
        store!(u64, &self.burn_permille, writer)?;
        store!(u64, &self.leg_permille, writer)?;
        store!(u64, &self.leg_v2_activation_daa, writer)?;
        // Version 7 (the 2026-09-23 Position route matrix, P-B3): the lifecycle and the market gate.
        store!(String, &self.class_lifecycle, writer)?;
        store!(String, &self.market_refusal, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelMarketResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let found = load!(bool, reader)?;
        let line_id = load!(String, reader)?;
        let opened = load!(bool, reader)?;
        let opened_daa = load!(u64, reader)?;
        let msk_reserve = load!(u64, reader)?;
        let position_units = load!(u64, reader)?;
        let sold_units = load!(u64, reader)?;
        let burned_sompi = load!(u64, reader)?;
        let registrant_paid_sompi = load!(u64, reader)?;
        let closed_to_buys = load!(bool, reader)?;
        let price_sompi_per_position = load!(u64, reader)?;
        let supply_units = load!(u64, reader)?;
        let virtual_sompi = load!(u64, reader)?;
        let class_status = load!(String, reader)?;
        // Version 2 (ADR-0088) appended the contributor's leg; a version-1 peer sent none.
        let contributor_paid_sompi = if version >= 2 { load!(u64, reader)? } else { 0 };
        // Version 3 (ADR-0090): the seed, its payer and the network's least seed.
        let (seed_sompi, seeded_by, seed_min_sompi) =
            if version >= 3 { (load!(u64, reader)?, load!(String, reader)?, load!(u64, reader)?) } else { (0, String::new(), 0) };
        // Version 4 (ADR-0091): a version-3 peer's chain had no buyback, so zero reads it exactly.
        let (buyback_sompi, retired_units) = if version >= 4 { (load!(u64, reader)?, load!(u64, reader)?) } else { (0, 0) };
        // Version 5 (ADR-0094): a peer older than this served no pledge, and on such a peer a
        // market that exists has always been fully seeded — so its opened seed IS the total.
        let seed_pledged_sompi = if version >= 5 { load!(u64, reader)? } else { seed_sompi };
        // Version 6 (ADR-0114): an older peer's fold splits every move 50/10 and schedules nothing.
        let (burn_permille, leg_permille, leg_v2_activation_daa) = if version >= 6 {
            (load!(u64, reader)?, load!(u64, reader)?, load!(u64, reader)?)
        } else {
            let v1 = kaspa_consensus_core::palw_model_market_v1::PalwModelFeesV1::V1;
            (v1.burn_permille, v1.leg_permille, 0)
        };
        // Version 7 (P-B3): an older peer's fold never asked the registry, so it names no refusal.
        let (class_lifecycle, market_refusal) =
            if version >= 7 { (load!(String, reader)?, load!(String, reader)?) } else { (String::new(), String::new()) };
        Ok(Self {
            found,
            line_id,
            opened,
            opened_daa,
            msk_reserve,
            position_units,
            sold_units,
            burned_sompi,
            registrant_paid_sompi,
            closed_to_buys,
            price_sompi_per_position,
            supply_units,
            virtual_sompi,
            class_status,
            contributor_paid_sompi,
            seed_sompi,
            seeded_by,
            seed_min_sompi,
            buyback_sompi,
            retired_units,
            seed_pledged_sompi,
            burn_permille,
            leg_permille,
            leg_v2_activation_daa,
            class_lifecycle,
            market_refusal,
        })
    }
}

/// ADR-0087 Decision 8: a holder's positions.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelPositionsRequest {
    /// 128-hex holder id — NOT a payout payload (the 2026-09-23 Position route matrix, P-B4). On
    /// the carrier lane it is `palw_model_holder_of_pubkey_v1(pubkey)`, the UNKEYED BLAKE2b-512 of
    /// the ML-DSA-87 public key (`mldsa87_key_id`); the keyed `blake2b_512_address_payload` in the
    /// key's address is a different value and holds nothing. On the ADR-0089 EVM lane it is
    /// `evm_holder_v1(chain_id, address)`. A provider derives the id from the key or account the
    /// member PROVED, never from the request.
    pub holder: String,
}

impl Serializer for GetPalwModelPositionsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.holder, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelPositionsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let holder = load!(String, reader)?;
        Ok(Self { holder })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelPosition {
    /// 128-hex line id (ADR-0088 Decision 9).
    pub line_id: String,
    pub units: u64,
    /// **Version 2 — ADR-0095 §4.5 and §4.3 for this id, at the response's `tip_daa`** (the
    /// 2026-09-23 Position route matrix, P-B2): the membership a provider serves on, which the
    /// wire used not to carry at all. When this id's tenure clock last started; `None` where the
    /// chain keeps no clock (a holding from before the membership fence) or the peer is older.
    #[serde(default)]
    pub holding_since_daa: Option<u64>,
    /// How long this id has held without selling — the chain's `model_position_tenure`.
    #[serde(default)]
    pub tenure_daa: u64,
    /// The index of the tier this id ALONE buys in the line's tiers in effect (`getPalwModelLine`'s
    /// `benefits.tiers` at the same height), and that tier. `None` below the first rung, where
    /// nothing is in effect, or from an older peer — which a gateway reads as "grants nothing".
    #[serde(default)]
    pub tier_index: Option<u32>,
    #[serde(default)]
    pub tier: Option<RpcPalwModelBenefitTier>,
}

impl Serializer for RpcPalwModelPosition {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(u64, &self.units, writer)?;
        // Version 2 (ADR-0095 §4.3/§4.5): appended, so a version-1 reader stops before it — each
        // element is its own length-framed payload, and the bytes it does not know are skipped.
        store!(Option<u64>, &self.holding_since_daa, writer)?;
        store!(u64, &self.tenure_daa, writer)?;
        store!(Option<u32>, &self.tier_index, writer)?;
        serialize!(Option<RpcPalwModelBenefitTier>, &self.tier, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelPosition {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        let units = load!(u64, reader)?;
        // A version-1 peer computed no tenure and no tier. Read FAIL-CLOSED: no clock, no tier —
        // never "tier 0", which on a line whose first rung is tenure-free would grant service on
        // a peer's ignorance.
        let (holding_since_daa, tenure_daa, tier_index, tier) = if version >= 2 {
            (
                load!(Option<u64>, reader)?,
                load!(u64, reader)?,
                load!(Option<u32>, reader)?,
                deserialize!(Option<RpcPalwModelBenefitTier>, reader)?,
            )
        } else {
            (None, 0, None, None)
        };
        Ok(Self { line_id, units, holding_since_daa, tenure_daa, tier_index, tier })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelPositionsResponse {
    pub holder: String,
    pub positions: Vec<RpcPalwModelPosition>,
    /// Version 2 (the 2026-09-23 Position route matrix, P-B2): the tip every row's tenure and tier
    /// was read at — the height a membership challenge names (ADR-0095 §4.8, A8). 0 from a peer
    /// that computed neither, so a caller can tell "no tier" from "not asked".
    #[serde(default)]
    pub tip_daa: u64,
    /// Version 2: the 128-hex hash of that tip block. A DAA score does not name a state — after a
    /// reorg another block carries the same score — so a caller that combines several reads (this
    /// one per id, `getPalwModelLine`'s tiers) checks they share THIS hash, not only the score.
    /// Empty from a peer that computed no tier, or off ConsensusV2.
    #[serde(default)]
    pub tip_hash: String,
}

impl Serializer for GetPalwModelPositionsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(String, &self.holder, writer)?;
        serialize!(Vec<RpcPalwModelPosition>, &self.positions, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(String, &self.tip_hash, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelPositionsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let holder = load!(String, reader)?;
        let positions = deserialize!(Vec<RpcPalwModelPosition>, reader)?;
        let (tip_daa, tip_hash) = if version >= 2 { (load!(u64, reader)?, load!(String, reader)?) } else { (0, String::new()) };
        Ok(Self { holder, positions, tip_daa, tip_hash })
    }
}
