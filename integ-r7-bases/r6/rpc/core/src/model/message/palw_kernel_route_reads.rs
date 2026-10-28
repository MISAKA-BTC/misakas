//! **G14 lane D — the kernel route's public read (ops 210, 211)** — moved out of `message.rs` (2026-10-13, a behaviour-preserving split: every item is the same and keeps its path through `pub use palw_kernel_route_reads::*`).

use super::*;

// ---------------------------------------------------------------------------------------------------------------------------
// G14 lane D — the kernel route's public read (ops 210, 211)
// ---------------------------------------------------------------------------------------------------------------------------

/// **`getPalwKernelClaim` (op 210)**: one claim of the probabilistic-constraint route, as the tip state holds it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwKernelClaimRequest {
    /// The claim id: 128 hex.
    pub claim_id: String,
}

impl Serializer for GetPalwKernelClaimRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwKernelClaimRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { claim_id: load!(String, reader)? })
    }
}

/// One position served on chain in answer to a demand: public from then on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwKernelServed {
    pub stage: u32,
    pub position: u32,
    /// Borsh of the kernel's `ServedPositionV1`, hex.
    pub bytes: String,
}

impl Serializer for RpcPalwKernelServed {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.stage, writer)?;
        store!(u32, &self.position, writer)?;
        store!(String, &self.bytes, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwKernelServed {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { stage: load!(u32, reader)?, position: load!(u32, reader)?, bytes: load!(String, reader)? })
    }
}

/// One open position demand.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwKernelDemand {
    pub stage: u32,
    pub position: u32,
    pub demanders: u32,
    pub filed_daa: u64,
    pub deadline_daa: u64,
    /// The latest rejected response's class (`malformed`, `wrong_bytes`, `wrong_root`, `fake_opening`, `partial`); empty if none.
    pub last_rejection: String,
}

impl Serializer for RpcPalwKernelDemand {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.stage, writer)?;
        store!(u32, &self.position, writer)?;
        store!(u32, &self.demanders, writer)?;
        store!(u64, &self.filed_daa, writer)?;
        store!(u64, &self.deadline_daa, writer)?;
        store!(String, &self.last_rejection, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwKernelDemand {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            stage: load!(u32, reader)?,
            position: load!(u32, reader)?,
            demanders: load!(u32, reader)?,
            filed_daa: load!(u64, reader)?,
            deadline_daa: load!(u64, reader)?,
            last_rejection: load!(String, reader)?,
        })
    }
}

/// One interim seat: the V2 bond (`txid:index`) and the kernel's digest of it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwKernelSeat {
    pub bond: String,
    pub kernel_bond: String,
}

impl Serializer for RpcPalwKernelSeat {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond, writer)?;
        store!(String, &self.kernel_bond, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwKernelSeat {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { bond: load!(String, reader)?, kernel_bond: load!(String, reader)? })
    }
}

/// Everything public about one kernel-route claim. Every digest is 128 hex, every blob hex; every amount in sompi. **Nothing here is
/// private**: the route holds chain state only. The per-block verdict and settlement events are not stored (they are the fold's
/// output); what they decided is here — `state`, `convicted`, `rewarded`, `reserved` — and replaying the chain reproduces them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwKernelClaimResponse {
    /// The node answered from a state that holds a kernel route (`false` before the fence's first block, and off ConsensusV2).
    pub available: bool,
    pub found: bool,
    pub tip_daa: u64,
    pub claim_id: String,
    /// `program` or `pipeline`.
    pub kind: String,
    /// The lifecycle state, spelled out.
    pub state: String,
    /// The DAA the claim finalized at (0 if it has not).
    pub final_daa: u64,
    pub convicted: bool,
    pub rewarded: bool,
    pub reserved_sompi: u64,
    pub committed_daa: u64,
    /// The end of the liability horizon (0 if none).
    pub liability_until: u64,
    pub producer_bond: String,
    pub job_id: String,
    pub class_id: String,
    /// The public claim record a fresh verifier is built from (`PublicClaimRecordV1::to_bytes`), hex.
    pub public_record: String,
    /// Borsh of the class header the verifier is built with, hex (empty for a pipeline claim).
    pub record_header: String,
    pub served: Vec<RpcPalwKernelServed>,
    pub demands: Vec<RpcPalwKernelDemand>,
    /// The interim Panel assignment while the receipts are counted (empty once covered or ended).
    pub seats: Vec<RpcPalwKernelSeat>,
    pub quorum: u32,
    pub assignment_deadline_daa: u64,
    pub receipts_counted: u32,
    /// The committed ledger root and the consensus tables' root (see `getPalwKernelRows`).
    pub ledger_root: String,
    pub aux_root: String,
    /// RFC-0015: `PanelLicensed` or `OptimisticPublicVerification`.
    pub mode: String,
    /// The OPV clock facts a fresh verifier plans by (all zero / empty for a Panel-licensed claim).
    pub opv: bool,
    pub opv_admitted_daa: u64,
    pub opv_verifier_start_cutoff_daa: u64,
    pub opv_final_floor_daa: u64,
    pub opv_hard_deadline_daa: u64,
    pub opv_reservation_sompi: u64,
    pub opv_max_gain_sompi: u64,
    /// What a Final of this claim means (an OPV Final is never shown as a proof of correctness).
    pub final_statement: String,
    /// RFC-0015 §9 / §11.3 (wire version 2): `pending`, `final-revocable` (Final but still convictable — not safe for an irreversible
    /// release), `settled`, `convicted` or `defaulted`; empty from a version-1 peer.
    pub settlement: String,
    /// The first DAA the claim is (or, absent a conviction or default, will be) settled at; 0 before Final.
    pub settled_from_daa: u64,
}

impl Serializer for GetPalwKernelClaimResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.found, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.kind, writer)?;
        store!(String, &self.state, writer)?;
        store!(u64, &self.final_daa, writer)?;
        store!(bool, &self.convicted, writer)?;
        store!(bool, &self.rewarded, writer)?;
        store!(u64, &self.reserved_sompi, writer)?;
        store!(u64, &self.committed_daa, writer)?;
        store!(u64, &self.liability_until, writer)?;
        store!(String, &self.producer_bond, writer)?;
        store!(String, &self.job_id, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.public_record, writer)?;
        store!(String, &self.record_header, writer)?;
        serialize!(Vec<RpcPalwKernelServed>, &self.served, writer)?;
        serialize!(Vec<RpcPalwKernelDemand>, &self.demands, writer)?;
        serialize!(Vec<RpcPalwKernelSeat>, &self.seats, writer)?;
        store!(u32, &self.quorum, writer)?;
        store!(u64, &self.assignment_deadline_daa, writer)?;
        store!(u32, &self.receipts_counted, writer)?;
        store!(String, &self.ledger_root, writer)?;
        store!(String, &self.aux_root, writer)?;
        store!(String, &self.mode, writer)?;
        store!(bool, &self.opv, writer)?;
        store!(u64, &self.opv_admitted_daa, writer)?;
        store!(u64, &self.opv_verifier_start_cutoff_daa, writer)?;
        store!(u64, &self.opv_final_floor_daa, writer)?;
        store!(u64, &self.opv_hard_deadline_daa, writer)?;
        store!(u64, &self.opv_reservation_sompi, writer)?;
        store!(u64, &self.opv_max_gain_sompi, writer)?;
        store!(String, &self.final_statement, writer)?;
        store!(String, &self.settlement, writer)?;
        store!(u64, &self.settled_from_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwKernelClaimResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let mut out = Self {
            available: load!(bool, reader)?,
            found: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            claim_id: load!(String, reader)?,
            kind: load!(String, reader)?,
            state: load!(String, reader)?,
            final_daa: load!(u64, reader)?,
            convicted: load!(bool, reader)?,
            rewarded: load!(bool, reader)?,
            reserved_sompi: load!(u64, reader)?,
            committed_daa: load!(u64, reader)?,
            liability_until: load!(u64, reader)?,
            producer_bond: load!(String, reader)?,
            job_id: load!(String, reader)?,
            class_id: load!(String, reader)?,
            public_record: load!(String, reader)?,
            record_header: load!(String, reader)?,
            served: deserialize!(Vec<RpcPalwKernelServed>, reader)?,
            demands: deserialize!(Vec<RpcPalwKernelDemand>, reader)?,
            seats: deserialize!(Vec<RpcPalwKernelSeat>, reader)?,
            quorum: load!(u32, reader)?,
            assignment_deadline_daa: load!(u64, reader)?,
            receipts_counted: load!(u32, reader)?,
            ledger_root: load!(String, reader)?,
            aux_root: load!(String, reader)?,
            mode: load!(String, reader)?,
            opv: load!(bool, reader)?,
            opv_admitted_daa: load!(u64, reader)?,
            opv_verifier_start_cutoff_daa: load!(u64, reader)?,
            opv_final_floor_daa: load!(u64, reader)?,
            opv_hard_deadline_daa: load!(u64, reader)?,
            opv_reservation_sompi: load!(u64, reader)?,
            opv_max_gain_sompi: load!(u64, reader)?,
            final_statement: load!(String, reader)?,
            settlement: String::new(),
            settled_from_daa: 0,
        };
        if version >= 2 {
            out.settlement = load!(String, reader)?;
            out.settled_from_daa = load!(u64, reader)?;
        }
        Ok(out)
    }
}

/// **`getPalwKernelRows` (op 211)**: the kernel route's rows, a page at a time.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwKernelRowsRequest {
    /// Resume after this row (exclusive). `has_cursor == false` starts at the beginning.
    pub has_cursor: bool,
    pub after_table: u32,
    /// The cursor's key, hex.
    pub after_key: String,
    /// The page's budget in key and row bytes (0 = the node's default; capped by the node).
    pub max_bytes: u32,
}

impl Serializer for GetPalwKernelRowsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.has_cursor, writer)?;
        store!(u32, &self.after_table, writer)?;
        store!(String, &self.after_key, writer)?;
        store!(u32, &self.max_bytes, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwKernelRowsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            has_cursor: load!(bool, reader)?,
            after_table: load!(u32, reader)?,
            after_key: load!(String, reader)?,
            max_bytes: load!(u32, reader)?,
        })
    }
}

/// One row: `(table, key, row)`, key and row hex.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwKernelRow {
    pub table: u32,
    pub key: String,
    pub row: String,
}

impl Serializer for RpcPalwKernelRow {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.table, writer)?;
        store!(String, &self.key, writer)?;
        store!(String, &self.row, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwKernelRow {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { table: load!(u32, reader)?, key: load!(String, reader)?, row: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwKernelRowsResponse {
    pub available: bool,
    pub tip_daa: u64,
    /// The committed roots. A reader that gathers every page, rebuilds the ledger from the ledger tables (below 32) with the policy in
    /// `header` and checks `ledger_root`, holds exactly what the chain committed.
    pub ledger_root: String,
    pub aux_root: String,
    /// Borsh of the route's header (policy, config root, scalars), hex.
    pub header: String,
    pub rows: Vec<RpcPalwKernelRow>,
    /// More rows follow this page (resume after `next_table` / `next_key`).
    pub more: bool,
    pub next_table: u32,
    pub next_key: String,
    pub total_rows: u64,
}

impl Serializer for GetPalwKernelRowsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(String, &self.ledger_root, writer)?;
        store!(String, &self.aux_root, writer)?;
        store!(String, &self.header, writer)?;
        serialize!(Vec<RpcPalwKernelRow>, &self.rows, writer)?;
        store!(bool, &self.more, writer)?;
        store!(u32, &self.next_table, writer)?;
        store!(String, &self.next_key, writer)?;
        store!(u64, &self.total_rows, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwKernelRowsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            ledger_root: load!(String, reader)?,
            aux_root: load!(String, reader)?,
            header: load!(String, reader)?,
            rows: deserialize!(Vec<RpcPalwKernelRow>, reader)?,
            more: load!(bool, reader)?,
            next_table: load!(u32, reader)?,
            next_key: load!(String, reader)?,
            total_rows: load!(u64, reader)?,
        })
    }
}

/// **`getPalwKernelFinals` (op 212)**: every Final the kernel route holds, in canonical order, with the beacon's fact for it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwKernelFinalsRequest {
    /// The most Finals to return (0 = the node's default; capped by the node).
    pub limit: u32,
}

impl Serializer for GetPalwKernelFinalsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.limit, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwKernelFinalsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { limit: load!(u32, reader)? })
    }
}

/// One Final: the kernel's receipt, how the work reached it, and the borsh `WorkFinalEventV1` where the route can state every fact the
/// beacon needs (RFC-0015: an OPV Final is `PanelIndependent`; a Panel-licensed Final exports none).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwKernelFinal {
    pub claim_id: String,
    /// `PanelLicensed` or `OptimisticPublicVerification`.
    pub mode: String,
    /// `PanelIndependent` or `PanelLicensed`.
    pub final_path: String,
    pub source_profile_id: String,
    pub canonical_work_id: String,
    pub execution_commitment: String,
    pub accepted_daa: u64,
    pub final_daa: u64,
    pub da_satisfied: bool,
    /// `Standing`, `ConvictedAfterFinal` or `DaForfeitedAfterFinal`.
    pub standing: String,
    /// Borsh of `WorkFinalEventV1`, hex (empty where the route cannot state the beacon's facts).
    pub work_final_event: String,
    pub statement: String,
}

impl Serializer for RpcPalwKernelFinal {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.mode, writer)?;
        store!(String, &self.final_path, writer)?;
        store!(String, &self.source_profile_id, writer)?;
        store!(String, &self.canonical_work_id, writer)?;
        store!(String, &self.execution_commitment, writer)?;
        store!(u64, &self.accepted_daa, writer)?;
        store!(u64, &self.final_daa, writer)?;
        store!(bool, &self.da_satisfied, writer)?;
        store!(String, &self.standing, writer)?;
        store!(String, &self.work_final_event, writer)?;
        store!(String, &self.statement, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwKernelFinal {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            claim_id: load!(String, reader)?,
            mode: load!(String, reader)?,
            final_path: load!(String, reader)?,
            source_profile_id: load!(String, reader)?,
            canonical_work_id: load!(String, reader)?,
            execution_commitment: load!(String, reader)?,
            accepted_daa: load!(u64, reader)?,
            final_daa: load!(u64, reader)?,
            da_satisfied: load!(bool, reader)?,
            standing: load!(String, reader)?,
            work_final_event: load!(String, reader)?,
            statement: load!(String, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwKernelFinalsResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub finals: Vec<RpcPalwKernelFinal>,
    /// How many Finals the route holds (more than `finals.len()` when the page was capped).
    pub total: u64,
    pub ledger_root: String,
}

impl Serializer for GetPalwKernelFinalsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        serialize!(Vec<RpcPalwKernelFinal>, &self.finals, writer)?;
        store!(u64, &self.total, writer)?;
        store!(String, &self.ledger_root, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwKernelFinalsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            finals: deserialize!(Vec<RpcPalwKernelFinal>, reader)?,
            total: load!(u64, reader)?,
            ledger_root: load!(String, reader)?,
        })
    }
}

/// **`getPalwModelRefutationSeals` (op 213, RFC02-T)**: one bond's candidate-refutation seal rows.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelRefutationSealsRequest {
    /// A bond `txid_hex:index`.
    pub bond: String,
}

impl Serializer for GetPalwModelRefutationSealsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelRefutationSealsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { bond: load!(String, reader)? })
    }
}

/// One seal row (`ModelRefutationSealRowV4`): the hidden seal, its clock and the original collateral it reserves while live.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelRefutationSeal {
    pub slot: u32,
    pub seal: String,
    pub posted_daa: u64,
    pub mature_daa: u64,
    pub expires_daa: u64,
    pub deposit_sompi: u64,
    /// Unexpired at the tip (its deposit is reserved).
    pub live: bool,
    /// Mature and unexpired at the tip (a reveal opening it is admissible).
    pub revealable: bool,
}

impl Serializer for RpcPalwModelRefutationSeal {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.slot, writer)?;
        store!(String, &self.seal, writer)?;
        store!(u64, &self.posted_daa, writer)?;
        store!(u64, &self.mature_daa, writer)?;
        store!(u64, &self.expires_daa, writer)?;
        store!(u64, &self.deposit_sompi, writer)?;
        store!(bool, &self.live, writer)?;
        store!(bool, &self.revealable, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelRefutationSeal {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            slot: load!(u32, reader)?,
            seal: load!(String, reader)?,
            posted_daa: load!(u64, reader)?,
            mature_daa: load!(u64, reader)?,
            expires_daa: load!(u64, reader)?,
            deposit_sompi: load!(u64, reader)?,
            live: load!(bool, reader)?,
            revealable: load!(bool, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelRefutationSealsResponse {
    pub available: bool,
    pub tip_daa: u64,
    pub bond: String,
    pub seals: Vec<RpcPalwModelRefutationSeal>,
    /// The original collateral the bond's live seals reserve at the tip.
    pub reserved_sompi: u64,
    pub aux_root: String,
}

impl Serializer for GetPalwModelRefutationSealsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(String, &self.bond, writer)?;
        serialize!(Vec<RpcPalwModelRefutationSeal>, &self.seals, writer)?;
        store!(u64, &self.reserved_sompi, writer)?;
        store!(String, &self.aux_root, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelRefutationSealsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            bond: load!(String, reader)?,
            seals: deserialize!(Vec<RpcPalwModelRefutationSeal>, reader)?,
            reserved_sompi: load!(u64, reader)?,
            aux_root: load!(String, reader)?,
        })
    }
}

/// **`getPalwOnboarding` (op 230)**: where one V2 class stands on the onboarding path.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwOnboardingRequest {
    /// The V2 class id: 128 hex.
    pub class_id: String,
}

impl Serializer for GetPalwOnboardingRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwOnboardingRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { class_id: load!(String, reader)? })
    }
}

/// One artifact binding of the class.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwOnboardingBinding {
    pub kernel_param_root: String,
    /// `Pending`, `Matured`, `Final` or `Refuted` at the tip.
    pub state: String,
    pub binder: String,
    pub bound_daa: u64,
    pub matures_daa: u64,
    pub final_daa: u64,
    pub reserved_sompi: u64,
}

impl Serializer for RpcPalwOnboardingBinding {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.kernel_param_root, writer)?;
        store!(String, &self.state, writer)?;
        store!(String, &self.binder, writer)?;
        store!(u64, &self.bound_daa, writer)?;
        store!(u64, &self.matures_daa, writer)?;
        store!(u64, &self.final_daa, writer)?;
        store!(u64, &self.reserved_sompi, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwOnboardingBinding {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            kernel_param_root: load!(String, reader)?,
            state: load!(String, reader)?,
            binder: load!(String, reader)?,
            bound_daa: load!(u64, reader)?,
            matures_daa: load!(u64, reader)?,
            final_daa: load!(u64, reader)?,
            reserved_sompi: load!(u64, reader)?,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwOnboardingResponse {
    /// The node answered from a V2 state.
    pub available: bool,
    pub found: bool,
    pub tip_daa: u64,
    pub class_id: String,
    pub status: String,
    pub artifact_root: String,
    /// The class's registrant bond, `txid:index`.
    pub registrant: String,
    pub artifact_bindings: Vec<RpcPalwOnboardingBinding>,
    pub kernel_bound: bool,
    pub kernel_class: String,
    pub kernel_plan_root: String,
    pub challenge_policy_id: String,
    pub conformance_committed: bool,
    pub conformance_statement_root: String,
    pub conformance_daa: u64,
    /// `NotKernelBound` (the legacy path), `Ready` or `Held`.
    pub gate: String,
    /// The RFC-0011 §17 failure / state that names the wait (empty unless `Held`).
    pub gate_code: String,
    pub gate_reason: String,
    pub ledger_root: String,
    pub aux_root: String,
}

impl Serializer for GetPalwOnboardingResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.found, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.status, writer)?;
        store!(String, &self.artifact_root, writer)?;
        store!(String, &self.registrant, writer)?;
        serialize!(Vec<RpcPalwOnboardingBinding>, &self.artifact_bindings, writer)?;
        store!(bool, &self.kernel_bound, writer)?;
        store!(String, &self.kernel_class, writer)?;
        store!(String, &self.kernel_plan_root, writer)?;
        store!(String, &self.challenge_policy_id, writer)?;
        store!(bool, &self.conformance_committed, writer)?;
        store!(String, &self.conformance_statement_root, writer)?;
        store!(u64, &self.conformance_daa, writer)?;
        store!(String, &self.gate, writer)?;
        store!(String, &self.gate_code, writer)?;
        store!(String, &self.gate_reason, writer)?;
        store!(String, &self.ledger_root, writer)?;
        store!(String, &self.aux_root, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwOnboardingResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            found: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            class_id: load!(String, reader)?,
            status: load!(String, reader)?,
            artifact_root: load!(String, reader)?,
            registrant: load!(String, reader)?,
            artifact_bindings: deserialize!(Vec<RpcPalwOnboardingBinding>, reader)?,
            kernel_bound: load!(bool, reader)?,
            kernel_class: load!(String, reader)?,
            kernel_plan_root: load!(String, reader)?,
            challenge_policy_id: load!(String, reader)?,
            conformance_committed: load!(bool, reader)?,
            conformance_statement_root: load!(String, reader)?,
            conformance_daa: load!(u64, reader)?,
            gate: load!(String, reader)?,
            gate_code: load!(String, reader)?,
            gate_reason: load!(String, reader)?,
            ledger_root: load!(String, reader)?,
            aux_root: load!(String, reader)?,
        })
    }
}

/// **`getPalwConformanceEvidence` (op 231, onboarding P0)**: a V2 class's conformance record, its current attempt and the evidence
/// posted for it — the raw rows of the route's aux tables 39 and 40, the network's challenge policy and the beacon state the node
/// derives. With op 212's Finals and op 230's class this is everything a fresh verifier needs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwConformanceEvidenceRequest {
    /// The V2 class id: 128 hex.
    pub class_id: String,
}

impl Serializer for GetPalwConformanceEvidenceRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwConformanceEvidenceRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { class_id: load!(String, reader)? })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwConformanceEvidenceResponse {
    /// The node answered from a V2 state.
    pub available: bool,
    pub found: bool,
    pub tip_daa: u64,
    pub class_id: String,
    /// The class's `OnboardingStateV1` code (`CHALLENGE_PENDING`, `CONFORMANCE_PASSED`, `ACTIVE_REWARDABLE`, ...); empty with no record.
    pub lifecycle_state: String,
    /// The record's last `OnboardingFailureV1` code (`BEACON_UNAVAILABLE`, `CONFORMANCE_FAILED`, ...), or empty.
    pub last_failure: String,
    /// How the last closed attempt ended (`BEACON_UNAVAILABLE`, `BEACON_CHANGED`, `EVIDENCE_FAILED`, `REFUTED`, `EVIDENCE_WITHHELD`), or empty.
    pub attempt_end: String,
    /// Counted attempts so far (`beacon_retries + conformance_failures`) and the policy's limit.
    pub attempts: u32,
    pub attempt_limit: u32,
    pub challenge_policy_id: String,
    /// The network's `PostCommitChallengePolicyV1`, Borsh, hex (its id is `challenge_policy_id`).
    pub challenge_policy: String,
    /// The current attempt's `ConformanceCommitmentV1::statement_root`, its accepted DAA and its ordinal (the beacon's epoch).
    pub statement_root: String,
    pub committed_daa: u64,
    pub challenge_epoch: u64,
    /// `COLLECTING`, `CANDIDATE`, `LOCKED`, `UNAVAILABLE` (empty with no open attempt), derived by the node at `tip_daa`.
    pub beacon_state: String,
    pub beacon_have: u32,
    pub beacon_need: u32,
    pub lock_position: u64,
    pub beacon_output: String,
    /// The attempt's posted evidence: its id, DAA and the end (exclusive) of its challenge window.
    pub evidence_posted: bool,
    pub evidence_id: String,
    pub evidence_daa: u64,
    pub window_end_daa: u64,
    /// `NotKernelBound`, `Ready` or `Held` (with the code and the reason), as op 230 says.
    pub gate: String,
    pub gate_code: String,
    pub gate_reason: String,
    /// Table 39's row and table 40's row, raw Borsh in hex: a fresh verifier decodes them itself (and can check them against the aux root op 211's rows rebuild).
    pub attempt_row: String,
    pub evidence_row: String,
    /// The class's canonical program bytes, hex (the V2 class's IR record; byte-for-byte its bound kernel class's, tag 106).
    pub program: String,
    pub ledger_root: String,
    pub aux_root: String,
}

impl Serializer for GetPalwConformanceEvidenceResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.found, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.lifecycle_state, writer)?;
        store!(String, &self.last_failure, writer)?;
        store!(String, &self.attempt_end, writer)?;
        store!(u32, &self.attempts, writer)?;
        store!(u32, &self.attempt_limit, writer)?;
        store!(String, &self.challenge_policy_id, writer)?;
        store!(String, &self.challenge_policy, writer)?;
        store!(String, &self.statement_root, writer)?;
        store!(u64, &self.committed_daa, writer)?;
        store!(u64, &self.challenge_epoch, writer)?;
        store!(String, &self.beacon_state, writer)?;
        store!(u32, &self.beacon_have, writer)?;
        store!(u32, &self.beacon_need, writer)?;
        store!(u64, &self.lock_position, writer)?;
        store!(String, &self.beacon_output, writer)?;
        store!(bool, &self.evidence_posted, writer)?;
        store!(String, &self.evidence_id, writer)?;
        store!(u64, &self.evidence_daa, writer)?;
        store!(u64, &self.window_end_daa, writer)?;
        store!(String, &self.gate, writer)?;
        store!(String, &self.gate_code, writer)?;
        store!(String, &self.gate_reason, writer)?;
        store!(String, &self.attempt_row, writer)?;
        store!(String, &self.evidence_row, writer)?;
        store!(String, &self.program, writer)?;
        store!(String, &self.ledger_root, writer)?;
        store!(String, &self.aux_root, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwConformanceEvidenceResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            found: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            class_id: load!(String, reader)?,
            lifecycle_state: load!(String, reader)?,
            last_failure: load!(String, reader)?,
            attempt_end: load!(String, reader)?,
            attempts: load!(u32, reader)?,
            attempt_limit: load!(u32, reader)?,
            challenge_policy_id: load!(String, reader)?,
            challenge_policy: load!(String, reader)?,
            statement_root: load!(String, reader)?,
            committed_daa: load!(u64, reader)?,
            challenge_epoch: load!(u64, reader)?,
            beacon_state: load!(String, reader)?,
            beacon_have: load!(u32, reader)?,
            beacon_need: load!(u32, reader)?,
            lock_position: load!(u64, reader)?,
            beacon_output: load!(String, reader)?,
            evidence_posted: load!(bool, reader)?,
            evidence_id: load!(String, reader)?,
            evidence_daa: load!(u64, reader)?,
            window_end_daa: load!(u64, reader)?,
            gate: load!(String, reader)?,
            gate_code: load!(String, reader)?,
            gate_reason: load!(String, reader)?,
            attempt_row: load!(String, reader)?,
            evidence_row: load!(String, reader)?,
            program: load!(String, reader)?,
            ledger_root: load!(String, reader)?,
            aux_root: load!(String, reader)?,
        })
    }
}

// RFC6/8/12: one snapshot of all reward funds; independent paging of native rows and EXEC legs.
// New op 241 preserves op 199's fixed binary layout. Global counters conserve outstandingSompi.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwRewardVestingRequest {
    pub bond: String,
    pub payout_address: String,
    pub claim_id: String,
    pub limit: u32,
    pub after: String,
    pub exec_after: String,
}
impl Serializer for GetPalwRewardVestingRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond, writer)?;
        store!(String, &self.payout_address, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(u32, &self.limit, writer)?;
        store!(String, &self.after, writer)?;
        store!(String, &self.exec_after, writer)?;
        Ok(())
    }
}
impl Deserializer for GetPalwRewardVestingRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported reward vesting version"));
        }
        Ok(Self {
            bond: load!(String, reader)?,
            payout_address: load!(String, reader)?,
            claim_id: load!(String, reader)?,
            limit: load!(u32, reader)?,
            after: load!(String, reader)?,
            exec_after: load!(String, reader)?,
        })
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwExecRewardLeg {
    pub claim_id: String,
    pub bond: String,
    pub fund: String,
    pub payload: String,
    pub sompi: String,
    pub final_daa: u64,
    pub earliest_daa: u64,
    pub native_pending: bool,
    pub native_clock_live: bool,
    pub native_expiry_daa: Option<u64>,
    pub native_second_clock_bound_daa: Option<u64>,
    pub lock_live: bool,
    pub ready: bool,
    pub in_next_block: bool,
}
impl Serializer for RpcPalwExecRewardLeg {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.bond, writer)?;
        store!(String, &self.fund, writer)?;
        store!(String, &self.payload, writer)?;
        store!(String, &self.sompi, writer)?;
        store!(u64, &self.final_daa, writer)?;
        store!(u64, &self.earliest_daa, writer)?;
        store!(bool, &self.native_pending, writer)?;
        store!(bool, &self.native_clock_live, writer)?;
        store!(Option<u64>, &self.native_expiry_daa, writer)?;
        store!(Option<u64>, &self.native_second_clock_bound_daa, writer)?;
        store!(bool, &self.lock_live, writer)?;
        store!(bool, &self.ready, writer)?;
        store!(bool, &self.in_next_block, writer)?;
        Ok(())
    }
}
impl Deserializer for RpcPalwExecRewardLeg {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported reward vesting version"));
        }
        Ok(Self {
            claim_id: load!(String, reader)?,
            bond: load!(String, reader)?,
            fund: load!(String, reader)?,
            payload: load!(String, reader)?,
            sompi: load!(String, reader)?,
            final_daa: load!(u64, reader)?,
            earliest_daa: load!(u64, reader)?,
            native_pending: load!(bool, reader)?,
            native_clock_live: load!(bool, reader)?,
            native_expiry_daa: load!(Option<u64>, reader)?,
            native_second_clock_bound_daa: load!(Option<u64>, reader)?,
            lock_live: load!(bool, reader)?,
            ready: load!(bool, reader)?,
            in_next_block: load!(bool, reader)?,
        })
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwRewardVestingResponse {
    pub available: bool,
    pub native: GetPalwVestingResponse,
    pub state_root: String,
    pub outstanding_sompi: String,
    pub exec_slice_sompi: String,
    pub exec_kernel_fee_sompi: String,
    pub query_outstanding_sompi: String,
    pub query_exec_pending_sompi: String,
    pub query_exec_ready_sompi: String,
    pub reporter_pending_sompi_of_query: String,
    pub reporter_awarded_sompi_of_query: String,
    pub locked_sources: u64,
    pub last_lock_daa: Option<u64>,
    pub exec_rows: Vec<RpcPalwExecRewardLeg>,
    pub exec_rows_total: u64,
    pub exec_next_after: String,
}
impl Serializer for GetPalwRewardVestingResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        serialize!(GetPalwVestingResponse, &self.native, writer)?;
        store!(String, &self.state_root, writer)?;
        store!(String, &self.outstanding_sompi, writer)?;
        store!(String, &self.exec_slice_sompi, writer)?;
        store!(String, &self.exec_kernel_fee_sompi, writer)?;
        store!(String, &self.query_outstanding_sompi, writer)?;
        store!(String, &self.query_exec_pending_sompi, writer)?;
        store!(String, &self.query_exec_ready_sompi, writer)?;
        store!(String, &self.reporter_pending_sompi_of_query, writer)?;
        store!(String, &self.reporter_awarded_sompi_of_query, writer)?;
        store!(u64, &self.locked_sources, writer)?;
        store!(Option<u64>, &self.last_lock_daa, writer)?;
        serialize!(Vec<RpcPalwExecRewardLeg>, &self.exec_rows, writer)?;
        store!(u64, &self.exec_rows_total, writer)?;
        store!(String, &self.exec_next_after, writer)?;
        Ok(())
    }
}
impl Deserializer for GetPalwRewardVestingResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported reward vesting version"));
        }
        Ok(Self {
            available: load!(bool, reader)?,
            native: deserialize!(GetPalwVestingResponse, reader)?,
            state_root: load!(String, reader)?,
            outstanding_sompi: load!(String, reader)?,
            exec_slice_sompi: load!(String, reader)?,
            exec_kernel_fee_sompi: load!(String, reader)?,
            query_outstanding_sompi: load!(String, reader)?,
            query_exec_pending_sompi: load!(String, reader)?,
            query_exec_ready_sompi: load!(String, reader)?,
            reporter_pending_sompi_of_query: load!(String, reader)?,
            reporter_awarded_sompi_of_query: load!(String, reader)?,
            locked_sources: load!(u64, reader)?,
            last_lock_daa: load!(Option<u64>, reader)?,
            exec_rows: deserialize!(Vec<RpcPalwExecRewardLeg>, reader)?,
            exec_rows_total: load!(u64, reader)?,
            exec_next_after: load!(String, reader)?,
        })
    }
}
