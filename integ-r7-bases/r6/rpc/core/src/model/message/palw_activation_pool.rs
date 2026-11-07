//! **ADR-0152-adjacent: the Activation Pool (op 200; user decision 2026-09-25)** — moved out of `message.rs` (2026-10-13, a behaviour-preserving split: every item is the same and keeps its path through `pub use palw_activation_pool::*`).

use super::*;

// ---------------------------------------------------------------------------------------------
// ADR-0152-adjacent: the Activation Pool (op 200; user decision 2026-09-25)
// ---------------------------------------------------------------------------------------------

/// **`getPalwActivationPool` (op 200)**: one class's Activation Pool as the tip state holds it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwActivationPoolRequest {
    /// A class id (128 hex), or 8+ hex of one, or `base`.
    pub class_id: String,
}

impl Serializer for GetPalwActivationPoolRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwActivationPoolRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { class_id: load!(String, reader)? })
    }
}

/// One class's pool, its terms and the chain-wide counters. Every amount in sompi.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwActivationPoolResponse {
    /// The node answered from a V2 state (`false` off ConsensusV2 or before the first state).
    pub available: bool,
    /// `Params::palw_activation_pool` is in force at the tip (testnet-12 alone).
    pub pool_armed: bool,
    pub tip_daa: u64,
    /// The state holds the class.
    pub class_found: bool,
    pub class_id: String,
    /// `active`, `dormant`, `frozen` or `registered` (the class record's status).
    pub class_status: String,
    /// The registry row's state (`Candidate`, `Probation { probes_passed: 3 }`, …); empty without a row.
    pub lifecycle: String,
    /// The class has a pool row (a bought class from its registration, any class from its first top-up).
    pub has_pool: bool,
    pub prep_sompi: u64,
    pub bonus_sompi: u64,
    pub funded_sompi: u64,
    pub paid_sompi: u64,
    pub withheld_sompi: u64,
    pub opened_daa: u64,
    /// Operators paid (a), sorted, hex.
    pub prep_paid: Vec<String>,
    /// Operators paid (b), sorted, hex.
    pub bonus_paid: Vec<String>,
    /// Operators credited on the class's probe Finals in its current probation run (paid (b) at `Probation → ActiveLimited`).
    pub probe_credited: Vec<String>,
    /// The registrant's operator id, excluded from (a) and (b); empty for a genesis class.
    pub registrant_operator: String,
    /// (a)'s per-payee amount were this Candidate's audit now: `min(A_MAX(age), ⌊prep/10⌋)`; 0 outside Candidate.
    pub prep_reward_now_sompi: u64,
    /// `A_MAX(age)` now — `A0` at opening, `3·A0` after the ramp.
    pub prep_cap_now_sompi: u64,
    /// The span this Candidate next meets its jury at (R2's stagger); 0 for any other class.
    pub next_audit_span: u64,
    pub span_daa: u64,
    /// The script a top-up pays into (`OP_RETURN OP_DATA8 "MSKACT01" OP_DATA64 <class>`), hex.
    pub sink_script: String,
    pub min_topup_sompi: u64,
    pub prep_base_sompi: u64,
    pub prep_share_permille: u32,
    pub bonus_share_permille: u32,
    pub ramp_daa: u64,
    pub prep_payee_cap: u32,
    pub bonus_payee_cap: u32,
    pub total_funded_sompi: u64,
    pub total_paid_sompi: u64,
    pub total_withheld_sompi: u64,
    /// Chain-wide `prep + bonus` over every pool (the counters, saturated to u64).
    pub total_available_sompi: u64,
    /// Decided and owed, not yet flushed into the payout queue (the fix round's F5).
    pub scheduled_sompi: u64,
    /// The class is the network's floor, which takes no top-up (F3).
    pub class_is_floor: bool,
    /// **The NON-BINDING recommended pool** (user decision 2026-09-25): `16 · A_MAX / α` of the terms
    /// in force (`palw_activation_recommended_pool_sompi_v1`) — what pays a listing's preparers in
    /// full. Nothing enforces it; 0 where the pool is not armed or the class is the floor.
    pub recommended_pool_sompi: u64,
    /// `b_cap`: the most (b) pays one operator (the pool's P1) — the per-Final seat pay at the
    /// heaviest class; 0 where the pool is not armed.
    pub bonus_cap_sompi: u64,
}

impl Serializer for GetPalwActivationPoolResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.pool_armed, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(bool, &self.class_found, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.class_status, writer)?;
        store!(String, &self.lifecycle, writer)?;
        store!(bool, &self.has_pool, writer)?;
        store!(u64, &self.prep_sompi, writer)?;
        store!(u64, &self.bonus_sompi, writer)?;
        store!(u64, &self.funded_sompi, writer)?;
        store!(u64, &self.paid_sompi, writer)?;
        store!(u64, &self.withheld_sompi, writer)?;
        store!(u64, &self.opened_daa, writer)?;
        store!(Vec<String>, &self.prep_paid, writer)?;
        store!(Vec<String>, &self.bonus_paid, writer)?;
        store!(Vec<String>, &self.probe_credited, writer)?;
        store!(String, &self.registrant_operator, writer)?;
        store!(u64, &self.prep_reward_now_sompi, writer)?;
        store!(u64, &self.prep_cap_now_sompi, writer)?;
        store!(u64, &self.next_audit_span, writer)?;
        store!(u64, &self.span_daa, writer)?;
        store!(String, &self.sink_script, writer)?;
        store!(u64, &self.min_topup_sompi, writer)?;
        store!(u64, &self.prep_base_sompi, writer)?;
        store!(u32, &self.prep_share_permille, writer)?;
        store!(u32, &self.bonus_share_permille, writer)?;
        store!(u64, &self.ramp_daa, writer)?;
        store!(u32, &self.prep_payee_cap, writer)?;
        store!(u32, &self.bonus_payee_cap, writer)?;
        store!(u64, &self.total_funded_sompi, writer)?;
        store!(u64, &self.total_paid_sompi, writer)?;
        store!(u64, &self.total_withheld_sompi, writer)?;
        store!(u64, &self.total_available_sompi, writer)?;
        store!(u64, &self.scheduled_sompi, writer)?;
        store!(bool, &self.class_is_floor, writer)?;
        store!(u64, &self.recommended_pool_sompi, writer)?;
        store!(u64, &self.bonus_cap_sompi, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwActivationPoolResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            pool_armed: load!(bool, reader)?,
            tip_daa: load!(u64, reader)?,
            class_found: load!(bool, reader)?,
            class_id: load!(String, reader)?,
            class_status: load!(String, reader)?,
            lifecycle: load!(String, reader)?,
            has_pool: load!(bool, reader)?,
            prep_sompi: load!(u64, reader)?,
            bonus_sompi: load!(u64, reader)?,
            funded_sompi: load!(u64, reader)?,
            paid_sompi: load!(u64, reader)?,
            withheld_sompi: load!(u64, reader)?,
            opened_daa: load!(u64, reader)?,
            prep_paid: load!(Vec<String>, reader)?,
            bonus_paid: load!(Vec<String>, reader)?,
            probe_credited: load!(Vec<String>, reader)?,
            registrant_operator: load!(String, reader)?,
            prep_reward_now_sompi: load!(u64, reader)?,
            prep_cap_now_sompi: load!(u64, reader)?,
            next_audit_span: load!(u64, reader)?,
            span_daa: load!(u64, reader)?,
            sink_script: load!(String, reader)?,
            min_topup_sompi: load!(u64, reader)?,
            prep_base_sompi: load!(u64, reader)?,
            prep_share_permille: load!(u32, reader)?,
            bonus_share_permille: load!(u32, reader)?,
            ramp_daa: load!(u64, reader)?,
            prep_payee_cap: load!(u32, reader)?,
            bonus_payee_cap: load!(u32, reader)?,
            total_funded_sompi: load!(u64, reader)?,
            total_paid_sompi: load!(u64, reader)?,
            total_withheld_sompi: load!(u64, reader)?,
            total_available_sompi: load!(u64, reader)?,
            scheduled_sompi: load!(u64, reader)?,
            class_is_floor: load!(bool, reader)?,
            recommended_pool_sompi: load!(u64, reader)?,
            bonus_cap_sompi: load!(u64, reader)?,
        })
    }
}
